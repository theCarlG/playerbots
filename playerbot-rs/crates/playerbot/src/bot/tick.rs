/// Main tick logic — called from `playerbot_update`.
///
/// Sequence per tick:
///   1.  Refresh snapshot + group membership + LOD tier
///   1c. LOD tick-skip (Background/Dormant skip most ticks)
///   2.  Throttled refresh of nearby/attacker lists (LOD-gated)
///   3.  Zone-change check → create/destroy encounter FSM
///   4.  Process push events → update encounter FSM + blackboard
///   4.7 BDI belief update (every tick, ~80ns)
///   4.8 BDI desire evaluation (LOD-gated interval)
///   4.9 GOAP planning (on intention change or plan staleness)
///   4.10 Write BDI/GOAP/LOD to blackboard + compute strategy flags
///   5.  Determine `ActiveFsm` (Dead > Combat > World)
///   6.  Build `TickContext`, run BT tree
///   6b. GOAP plan step advancement (step complete/failed signals)
///   7.  Monitor, addon pushes, KTM, timers
use crate::{
    bot::state::BotState,
    encounters::{EncounterEvent, coordinator},
    engine::{
        bt_nodes::BtNode,
        context::TickContext,
        macro_fsm::{ActiveFsm, WorldSub},
    },
};

pub fn tick(bot: &mut BotState, elapsed_ms: u32, minimal: bool) {
    let cfg = crate::config::get();

    // 1. Refresh snapshot (always — one C++ call)
    bot.snap = bot.interface.get_snapshot();
    let now_ms = bot.snap.server_time_ms;

    // 1a. Reconcile the shared GroupState handle with the snapshot. The
    // RAII `GroupHandle` returned by the registry automatically deregisters
    // itself when dropped, so reassignment here is also how we leave the
    // previous group.
    bot.refresh_group_membership();

    // 1b. Determine AI LOD tier — scales processing depth with human proximity.
    bot.lod = crate::bot::lod::determine_lod(bot);

    // 1c. LOD tick-skip — Background/Dormant bots skip most ticks entirely.
    if bot.lod.should_skip_tick(bot.handle, now_ms) {
        bot.timers.advance(now_ms);
        return;
    }

    // 2. Throttled attacker/nearby refresh
    if !minimal && now_ms.saturating_sub(bot.last_attackers_refresh_ms) >= cfg.attacker_refresh_ms {
        bot.attackers = bot.interface.get_attackers().to_vec();
        bot.last_attackers_refresh_ms = now_ms;
    }
    // Skip nearby scan for Background/Dormant — biggest CPU savings.
    if !minimal && bot.lod.should_scan_nearby()
        && now_ms.saturating_sub(bot.last_nearby_refresh_ms) >= cfg.nearby_refresh_ms
    {
        bot.nearby_units = bot
            .interface
            .get_nearby_units(cfg.nearby_scan_range, false)
            .to_vec();
        bot.last_nearby_refresh_ms = now_ms;
    }

    // 3. Zone-change detection — create or destroy encounter FSM.
    let zone_id = bot.snap.zone_id;
    let need_new_encounter = match &bot.encounter {
        None => coordinator::encounter_for_zone(zone_id).is_some(),
        Some(enc) => enc.is_done(),
    };
    if need_new_encounter {
        bot.encounter = coordinator::encounter_for_zone(zone_id);
    }
    // If we left a known zone, clear the FSM.
    if bot.encounter.is_some() && coordinator::encounter_for_zone(zone_id).is_none() {
        bot.encounter = None;
    }

    // 4. Process push events
    process_events(bot, now_ms);

    // 4.1 Sync local encounter FSM → shared GroupState.
    // Every bot writes its local view; since all bots in the same zone see
    // the same events, they converge on the same state. The write is cheap
    // (try_write fails silently if another bot holds the lock).
    sync_encounter_to_group(bot, now_ms);

    // 4.2 Auto-respond to pending group invites (PB2 parity).
    // NOTE: Ready checks are handled C++-side (packet-driven, not polled)
    // to avoid spamming responses every tick.
    bot.interface.accept_group_invite();

    // 4.5 Process pending chat commands → mutate settings
    crate::commands::process_commands(bot);

    // 4.6 Update travel target FSM — check status, sync to blackboard.
    update_travel_target(bot, now_ms);

    // ── BDI + GOAP evaluation ────────────────────────────────────────

    // 4.7 Populate group + target beliefs from unit snapshots (FFI calls).
    populate_group_beliefs(bot);

    // 4.7b Update beliefs from snapshot (every tick, ~80ns).
    crate::bdi::beliefs::update(&mut bot.bdi.beliefs, &bot.snap, &bot.attackers);
    bot.bdi.beliefs.encounter_active = bot
        .encounter
        .as_ref()
        .is_some_and(|e| e.is_active());
    // Boss HP for execute-phase desires and BossBelow* GOAP atoms.
    // Uses the same resolver as the encounter FSM so HP-gated phase
    // transitions agree with belief-based desire scoring.
    bot.bdi.beliefs.boss_hp_pct = (resolve_boss_hp_pct(bot) * 100.0) as u8;

    // 4.8 BDI: evaluate desires and select/maintain intention.
    //     Gated by LOD tier interval (Full: 500ms, Active: 2s, Background: 5s, Dormant: never).
    let encounter_active = bot.bdi.beliefs.encounter_active;
    let bdi_interval = bot.lod.bdi_interval_ms();
    let bdi_due = now_ms.saturating_sub(bot.bdi.last_eval_ms) >= bdi_interval;
    if !minimal && bdi_due {
        // Build group desire counts for coordination (if grouped).
        let group_desires = bot.group_state.as_ref().and_then(|gh| {
            let gs = gh.state().try_read().ok()?;
            let mut gd = crate::bdi::desires::GroupDesireCounts::default();
            for md in &gs.member_desires {
                if md.handle != 0 && md.handle != bot.handle {
                    let idx = md.desire as usize;
                    if idx < crate::bdi::desires::DesireKind::COUNT {
                        gd.counts[idx] = gd.counts[idx].saturating_add(1);
                    }
                }
            }
            Some(gd)
        });
        crate::bdi::evaluate(
            &mut bot.bdi,
            bot.class,
            bot.role,
            encounter_active,
            now_ms,
            bot.settings.mode,
            group_desires.as_ref(),
        );
    }

    // 4.9 GOAP: advance plan steps whose effects are now satisfied, then
    //     replan if intention changed or plan is stale/complete.
    if !minimal {
        let current_ws = crate::goap::world_state::from_beliefs(&bot.bdi.beliefs);
        // Natural step advancement — when BT has fulfilled a step's
        // effects (e.g. `acquire_target` sets HasTarget), auto-advance.
        bot.bdi
            .plan_cache
            .try_advance_completed_steps(current_ws, crate::goap::actions::registry());
        if bot.bdi.needs_replan(now_ms) {
            let desire = bot.bdi.active_desire();
            if let Some(plan) = crate::goap::plan_for_intention(
                desire,
                current_ws,
                &bot.bdi.available_actions,
                now_ms,
            ) {
                bot.bdi.plan_cache.plan = plan;
            }
        }
    }

    // 4.10 Write BDI/GOAP/LOD state to blackboard.
    crate::bdi::write_to_blackboard(&bot.bdi, &mut bot.blackboard);
    crate::goap::write_to_blackboard(&bot.bdi.plan_cache, &mut bot.blackboard);
    {
        use crate::engine::blackboard::{Key, Value};
        bot.blackboard.set(Key::AiLodTier, Value::U32(bot.lod as u32));
    }
    let goap_flags = bot.bdi.goap_strategy_flags();

    // ── End BDI + GOAP ───────────────────────────────────────────────

    // 4.95 RTSC forced-intention consumer.
    //
    // RTSC moves bypass GOAP — they are positional, not goal-shaped.
    // Drive `move_to` (and the two-stage jump rotation) every tick
    // until arrival, then clear the intention. This is what makes the
    // RTSC waypoint persist across ticks instead of getting clobbered
    // by follow / combat movement. See `playerbot-rs/GAPS.md` Gaps
    // #10-#13.
    tick_rtsc_forced_intention(bot);

    // 5. Determine ActiveFsm (Dead > Combat > World)
    // A focus_target (from pull/attack commands) also triggers Combat so
    // the bot approaches and engages rather than staying in Follow mode.
    //
    // Validate the focus target first — a stale or dead target must be
    // cleared here because TickContext only has a shared ref to settings
    // and can't mutate it. Without this, the bot gets stuck in Combat
    // FSM forever with a non-attackable focus target.
    // Autonomous bots (no master, or a self-bot that is its own master)
    // promote a quest-objective mob chosen last tick (EngageTargetHandle, set
    // by tick_attack_quest_mob) to focus_target, so the combat FSM engages it
    // with the full class rotation. Quest kill mobs are routinely NEUTRAL and
    // never trip the combat FSM on their own — this is what lets a questing
    // bot actually fight (and a mage cast, not flail in melee). Commanded bots
    // (a different player's master) keep player-directed targeting.
    {
        use crate::engine::blackboard::Key;
        let autonomous = !matches!(bot.master_guid, Some(m) if m != bot.handle);
        if let Some(engage) = bot.blackboard.get_u64(Key::EngageTargetHandle) {
            bot.blackboard.clear(Key::EngageTargetHandle);
            // NOTE: deliberately NOT gated on `!in_combat`. The melee
            // auto-attack attempt from the World-tree tick can flag the bot
            // in-combat before this promotion runs; gating on `!in_combat`
            // then left it in the Combat FSM with no focus and a hostile-only
            // fallback that rejects the neutral quest mob — so it just held a
            // melee order it couldn't land. Promote regardless of in_combat;
            // `attackers.is_empty()` still defers to genuine defense.
            if autonomous
                && bot.settings.focus_target.is_none()
                && bot.attackers.is_empty()
                && bot.snap.self_.is_alive
                && bot.interface.can_attack(engage)
                // Don't re-pick a target we just abandoned as unreachable.
                && !matches!(
                    bot.blackboard.get_u64(Key::BadTargetHandle),
                    Some(bad) if bad == engage
                        && bot.snap.server_time_ms
                            < bot.blackboard.get_u64(Key::BadTargetUntilMs).unwrap_or(0)
                )
            {
                bot.settings.focus_target = Some(engage);
            }
        }
    }

    // NOTE: the anti-wedge TELEPORT was removed. A server-side bot teleporting
    // to its goal sounds clean but in practice it repeatedly went wrong —
    // teleporting bots under the map to their deaths, into caves, and out of
    // vendor buildings. Bots now WALK everywhere via normal pathfinding; a bot
    // that genuinely can't path somewhere re-plans rather than blinking. Hard
    // pathing cases (mob in a cave) are to be solved by better walk-pathing, not
    // teleport-to-coords.

    // Abandon a focus the bot can't make progress on. "Progress" = actually
    // fighting it (in_combat), walking toward it (is_moving), or casting at it
    // (is_casting). If a focus stops being progressable for 4s the bot is stuck
    // on it — either it's invalid (dead/evaded), OR it drifted out of reach (a
    // quest mob that wandered 60y away, out of cast range and unreachable by
    // path: cast=false, los=false, mov=false forever). Now that there's no
    // teleport rescue, an unreachable focus would freeze the bot in the Combat
    // FSM, ignoring the quest mobs right next to it. Drop it and briefly
    // blacklist it so the bot re-targets something it can actually engage.
    {
        use crate::engine::blackboard::{Key, Value};
        let now = bot.snap.server_time_ms;
        let progressing = bot.snap.self_.in_combat
            || bot.snap.self_.is_moving
            || bot.snap.self_.is_casting;
        // Autonomous (self/solo) bots only — a grouped bot's focus is set by
        // command (e.g. told to wait on a target) and must not be auto-dropped.
        let autonomous = !matches!(bot.master_guid, Some(m) if m != bot.handle);
        match bot.settings.focus_target {
            Some(focus) if !progressing && autonomous => {
                // Unreachable NOW: the focus is invalid (dead/evaded) OR it has
                // drifted beyond cast+chase range (a quest mob that wandered off:
                // e.g. focusDist 52, los=false, the bot can't path to it) while
                // the bot isn't even moving toward it. Quest-objective mobs are
                // only ever picked from the ~40y nearby scan, so a focus past 45y
                // that the bot isn't chasing is a drifted lock — drop it at once
                // (don't wait the 4s) so the bot engages a reachable mob instead
                // of freezing. The 4s grace still covers in-range stalls (e.g. a
                // mob behind a wall 20y away).
                let dist = bot.interface.unit_distance(focus);
                let fs = bot.interface.get_unit_snapshot(focus);
                let unreachable = !bot.interface.can_attack(focus)
                    || dist > 45.0
                    // No navmesh route to it (e.g. a caged rattlecage mob 28y
                    // away the bot can't walk to). Drop it at once so the bot
                    // engages a reachable mob or travels, instead of freezing.
                    || (!bot.snap.self_.is_moving
                        && !bot.interface.can_pathfind_to(fs.pos.x, fs.pos.y, fs.pos.z));
                let last = bot.blackboard.get_u64(Key::FocusProgressMs).unwrap_or(now);
                if unreachable || now.saturating_sub(last) > 4_000 {
                    bot.settings.focus_target = None;
                    bot.blackboard.clear(Key::EngageTargetHandle);
                    bot.blackboard.set(Key::BadTargetHandle, Value::U64(focus));
                    bot.blackboard
                        // Short cooldown: a mob abandoned as momentarily
                        // unreachable (LOS/path) should be RE-TRIED soon, not
                        // locked out for 30s — otherwise the bot blacklists every
                        // nearby quest mob one by one and ends up standing among
                        // them with nothing left to target.
                        .set(Key::BadTargetUntilMs, Value::U64(now.saturating_add(6_000)));
                }
            }
            // Progressing (fighting/chasing/casting) or no focus — reset clock.
            _ => bot.blackboard.set(Key::FocusProgressMs, Value::U64(now)),
        }
    }

    // ── Anti-deadlock watchdog ──────────────────────────────────────────────
    // A bot must NEVER just freeze with no purpose. Standing still is only OK
    // when it has a reason: commanded Stay, in combat, casting/channeling
    // (incl. drinking), recovering HP/mana, or physically moving. We track the
    // last time the bot made real PROGRESS — moved >5y, fought, cast, or its
    // hp/mana rose (eating/drinking). If NONE of that happens for 12s the bot is
    // deadlocked: force-break every state that can pin it (focus, engage /
    // blacklist target handles, travel destination) so it drops back through
    // travel → grind → rpg-wander and moves again. This is the safety net under
    // every specific stuck-fix: whatever new way a bot finds to wedge, it
    // un-wedges within 12s instead of standing forever.
    //
    // ONLY while ALIVE. A dead bot/ghost has its own purpose — the death FSM
    // (release spirit → corpse run → reclaim). Running the watchdog on a ghost
    // force-relocated it to random wander points whenever its corpse-run move
    // hitched for a moment, making the ghost "glide around everywhere" instead
    // of going to its body.
    if bot.snap.self_.is_alive {
        use crate::engine::blackboard::{Key, Value};
        let now = bot.snap.server_time_ms;
        let pos = bot.snap.self_.pos;
        let hp = bot.snap.self_.health as f32 / bot.snap.self_.max_health.max(1) as f32;
        let mana = bot.snap.self_.mana as f32 / bot.snap.self_.max_mana.max(1) as f32;
        let ax = bot.blackboard.get_f32(Key::WedgeAnchorX);
        let ay = bot.blackboard.get_f32(Key::WedgeAnchorY);
        let ams = bot.blackboard.get_u64(Key::WedgeAnchorMs).unwrap_or(now);
        let ahp = bot.blackboard.get_f32(Key::IdleAnchorHp).unwrap_or(hp);
        let amana = bot.blackboard.get_f32(Key::IdleAnchorMana).unwrap_or(mana);
        let moved = match (ax, ay) {
            (Some(x), Some(y)) => (pos.x - x).powi(2) + (pos.y - y).powi(2) > 5.0 * 5.0,
            _ => true,
        };
        let _ = (ahp, amana);
        // PURPOSEFUL recovery only — set by the eat/drink behavior when it's
        // actively drinking/eating. Passive idle mana regen must NOT count
        // (otherwise a bot just standing while mana ticks up looks "busy" and
        // never trips the watchdog — exactly what happened post-fight).
        let recovering = bot
            .blackboard
            .get_u64(Key::RecoverActiveMs)
            .is_some_and(|t| now.saturating_sub(t) < 8_000);
        // Commanded Stay / Guard etc. are purposeful stands (only Follow roams).
        let purposeful_stand =
            !matches!(bot.settings.mode, crate::bot::settings::BehaviorMode::Follow);
        // In-combat counts as progress ONLY when the bot can actually SEE its
        // target. A bot fighting normally (meleeing or casting) has LOS to its
        // focus; a bot wedged inside geometry — taking hits but unable to reach
        // or see the mob — does NOT, and previously `in_combat` alone marked it
        // "busy" forever, so it sat there getting hit and doing nothing (never
        // rescued). Now that wedged-in-combat state trips the watchdog after 10s.
        let fighting_with_los = bot.snap.self_.in_combat
            && bot
                .settings
                .focus_target
                .is_some_and(|f| bot.interface.has_los(f));
        let progressing = moved
            || fighting_with_los
            || bot.snap.self_.is_moving
            || bot.snap.self_.is_casting
            || bot.snap.self_.is_channeling
            || recovering
            || purposeful_stand;
        if progressing {
            bot.blackboard.set(Key::WedgeAnchorX, Value::F32(pos.x));
            bot.blackboard.set(Key::WedgeAnchorY, Value::F32(pos.y));
            bot.blackboard.set(Key::WedgeAnchorMs, Value::U64(now));
            bot.blackboard.set(Key::IdleAnchorHp, Value::F32(hp));
            bot.blackboard.set(Key::IdleAnchorMana, Value::F32(mana));
        } else if now.saturating_sub(ams) > 10_000 {
            // Deadlocked. Clearing state isn't enough — the same unreachable
            // objective just gets re-picked (e.g. all remaining quest mobs are
            // caged/unreachable from here). So PHYSICALLY RELOCATE: walk to a
            // reachable point ~30y away. From a new position a different/closer
            // objective becomes available, or it can re-approach. Guarantees the
            // bot is never frozen.
            bot.settings.focus_target = None;
            bot.blackboard.clear(Key::EngageTargetHandle);
            bot.blackboard.clear(Key::BadTargetHandle);
            bot.blackboard.clear(Key::BadTargetUntilMs);
            // Pick a reachable wander point (vary direction by handle+time).
            let seed = now.wrapping_add(bot.handle.rotate_left(11));
            let mut chosen: Option<(f32, f32)> = None;
            for i in 0..8u64 {
                let ang = ((seed.wrapping_add(i.wrapping_mul(2_654_435_761)) >> 8) % 360) as f32
                    * std::f32::consts::PI
                    / 180.0;
                let wx = pos.x + ang.cos() * 30.0;
                let wy = pos.y + ang.sin() * 30.0;
                if bot.interface.can_pathfind_to(wx, wy, pos.z) {
                    chosen = Some((wx, wy));
                    break;
                }
            }
            if let Some((wx, wy)) = chosen {
                bot.blackboard.set(Key::TravelDestX, Value::F32(wx));
                bot.blackboard.set(Key::TravelDestY, Value::F32(wy));
                bot.blackboard.set(Key::TravelDestZ, Value::F32(pos.z));
                bot.blackboard
                    .set(Key::TravelDestMap, Value::U32(pos.map_id));
            } else {
                // Nothing reachable nearby — clear dest so it re-plans freely.
                bot.blackboard.clear(Key::TravelDestX);
                bot.blackboard.clear(Key::TravelDestY);
                bot.blackboard.clear(Key::TravelDestZ);
            }
            // Re-anchor so the next window starts fresh after the break.
            bot.blackboard.set(Key::WedgeAnchorX, Value::F32(pos.x));
            bot.blackboard.set(Key::WedgeAnchorY, Value::F32(pos.y));
            bot.blackboard.set(Key::WedgeAnchorMs, Value::U64(now));
            bot.blackboard.set(Key::IdleAnchorHp, Value::F32(hp));
            bot.blackboard.set(Key::IdleAnchorMana, Value::F32(mana));
            if bot.master_guid == Some(bot.handle) {
                crate::log_warn!(
                    "[QTrace][SELF] anti-deadlock: 10s no progress — relocating to a reachable point"
                );
            }
        }
    }

    if let Some(focus) = bot.settings.focus_target
        && !bot.interface.can_attack(focus) {
            bot.settings.focus_target = None;
            // If the forced intention targeted this unit, clear it too.
            if bot.bdi.forced_intention.is_some_and(|fi| fi.target() == Some(focus)) {
                bot.bdi.forced_intention = None;
                bot.bdi.intention_changed = true;
            }
        }
    let has_engagement = !bot.attackers.is_empty() || bot.settings.focus_target.is_some();
    let active_fsm = ActiveFsm::determine(
        bot.snap.self_.is_alive,
        bot.snap.self_.in_combat,
        has_engagement,
    );
    bot.active_fsm = active_fsm;

    // 5.1 FSM transition detection — fire entry/exit actions on state change.
    let prev_fsm = bot.prev_active_fsm;
    if active_fsm != prev_fsm {
        on_fsm_exit(bot, prev_fsm);
        on_fsm_enter(bot, active_fsm);
        bot.prev_active_fsm = active_fsm;
    }

    // Write FSM state to blackboard so BT nodes can read it.
    use crate::engine::blackboard::{Key, Value};
    bot.blackboard
        .set(Key::ActiveFsmState, Value::U32(active_fsm as u32));

    // 5a. Derive World sub-state when in World FSM.
    if active_fsm == ActiveFsm::World {
        let is_traveling = bot.travel_target.is_active();
        let world_sub = if is_traveling {
            WorldSub::Travel
        } else {
            WorldSub::derive(bot.settings.mode, false)
        };
        bot.blackboard
            .set(Key::WorldSubState, Value::U32(world_sub as u32));

        // 5b. WorldSub transition detection.
        if world_sub != bot.prev_world_sub {
            on_world_sub_exit(bot, bot.prev_world_sub);
            on_world_sub_enter(bot, world_sub);
            bot.prev_world_sub = world_sub;
        }
    }

    // 6. Build TickContext and run BT
    //
    // Destructure bot so the borrow checker sees each field independently.
    // This avoids the raw pointer cast that was previously needed.
    let monitor_active = bot.monitor_active;

    let BotState {
        ref snap,
        ref attackers,
        ref nearby_units,
        ref interface,
        ref mut blackboard,
        ref mut timers,
        ref mut throttles,
        ref group_state,
        master_guid,
        active_fsm,
        ref encounter,
        ref trees,
        handle: bot_handle,
        class,
        role,
        ref settings,
        ..
    } = *bot;

    let ctx_group = group_state
        .as_ref()
        .and_then(|handle| handle.state().try_read().ok());

    // Check for encounter override before building TickContext.
    // The encounter's phase_bt() is dynamic (phase changes each tick).
    let enc_override = encounter.as_ref().and_then(|enc| enc.phase_bt(active_fsm));

    let mut ctx = TickContext {
        snap,
        nearby: nearby_units,
        attackers,
        group_state: ctx_group.as_deref(),
        group_handle: group_state.as_ref(),
        interface: interface.as_ref(),
        blackboard,
        timers,
        throttles,
        server_time_ms: now_ms,
        elapsed_ms,
        minimal,
        bot_handle,
        master_guid,
        active_fsm,
        encounter: encounter.as_deref(),
        class,
        role,
        settings,
        goap_flags,
        monitor_trace: if monitor_active {
            Some(std::cell::RefCell::new(Vec::new()))
        } else {
            None
        },
        pending_target: std::cell::Cell::new(None),
    };

    // FSM-based dispatch: primary tree + encounter overlay.
    //
    // The primary tree runs first (combat rotation, targeting, reactive
    // subtrees, follow, etc.). The encounter BT (if any) runs afterward
    // as a high-priority overlay — encounter-specific mechanics like
    // positioning overrides, flee-from-AoE, and zone-wide duties (rune
    // dousing, suppression devices). Running the encounter BT last
    // ensures its movement commands take precedence over the primary
    // tree's generic positioning.
    let primary = match active_fsm {
        ActiveFsm::Dead => &trees.dead,
        ActiveFsm::Combat => &trees.combat,
        ActiveFsm::World => &trees.world,
    };
    let _ = primary.tick(&mut ctx);
    if let Some(ref enc_bt) = enc_override {
        let _ = enc_bt.tick(&mut ctx);
    }

    // Maintenance runs after primary in all states except Dead.
    if active_fsm != ActiveFsm::Dead {
        let _ = trees.maintenance.tick(&mut ctx);
    }

    // 6a. GOAP plan step advancement — check BT signals before dropping ctx.
    let goap_step_complete = ctx
        .blackboard
        .get_bool(crate::engine::blackboard::Key::GoapStepCompleteSignal)
        .unwrap_or(false);
    let goap_step_failed = ctx
        .blackboard
        .get_bool(crate::engine::blackboard::Key::GoapStepFailedSignal)
        .unwrap_or(false);
    // Clear the signals for next tick.
    ctx.blackboard.clear(crate::engine::blackboard::Key::GoapStepCompleteSignal);
    ctx.blackboard.clear(crate::engine::blackboard::Key::GoapStepFailedSignal);

    // 6. Monitor: BT path tracing + throttled tick summary
    //
    // Collect data from `ctx` first (which borrows `bot` fields), then
    // drop the borrows before accessing `bot` directly for logging.
    let monitor_path = if monitor_active {
        ctx.monitor_trace
            .as_ref()
            .map(|trace| trace.borrow().join(" > "))
    } else {
        None
    };
    // Collect monitor diagnostic lines before dropping ctx.
    let monitor_lines: Vec<String> = if monitor_active {
        ctx.blackboard.drain_monitor_lines().collect()
    } else {
        Vec::new()
    };
    drop(ctx);
    drop(ctx_group);

    // 6b. Apply GOAP plan step signals now that borrows are released.
    if goap_step_complete {
        bot.bdi.plan_cache.plan.advance();
    }
    if goap_step_failed {
        // Invalidate the plan — next tick's needs_replan() will trigger replanning.
        bot.bdi.plan_cache.plan = crate::goap::plan::GoapPlan::default();
        bot.bdi.intention_changed = true;
    }

    // 6c. Publish heal target for soft coordination (HealAssignment tracking).
    // Healers publish their current target so other healers can deprioritize
    // already-covered allies and spread heals across the group.
    if bot.role.is_heal() {
        let target = bot.snap.self_.current_target;
        if target != 0
            && let Some(ref gh) = bot.group_state
                && let Ok(mut gs) = gh.state().try_write() {
                    gs.publish_heal_target(bot.handle, target, now_ms);
                }
    }

    if monitor_active {
        // Log BT path changes (which leaf node was reached).
        if let Some(path) = monitor_path
            && !path.is_empty() && path != bot.last_bt_path {
                crate::bot::monitor::monitor_bt_path(bot, &path);
                bot.last_bt_path = path;
            }
        // Log all diagnostic lines collected during BT evaluation.
        for line in monitor_lines {
            crate::bot::monitor::monitor_log(bot, &line);
        }
        // Detect combat state transitions and log them.
        let in_combat = bot.snap.self_.in_combat;
        let was_in_combat = bot.last_monitor_in_combat;
        if in_combat != was_in_combat {
            bot.last_monitor_in_combat = in_combat;
            if in_combat {
                crate::bot::monitor::monitor_log(bot, ">>> COMBAT ENTERED <<<");
                crate::bot::monitor::monitor_tick_summary(bot);
            } else {
                crate::bot::monitor::monitor_log(bot, ">>> COMBAT LEFT <<<");
            }
        }
        // Log FSM state transitions.
        crate::bot::monitor::monitor_log(bot, &format!("FSM: {:?}", bot.active_fsm));
        // Periodic tick summary (every 2s for detailed debugging).
        if now_ms.saturating_sub(bot.last_monitor_summary_ms) >= 2000 {
            crate::bot::monitor::monitor_tick_summary(bot);
            if !bot.last_bt_path.is_empty() {
                crate::bot::monitor::monitor_bt_path(bot, &bot.last_bt_path);
            }
            bot.last_monitor_summary_ms = now_ms;
        }
    }

    // 7. Addon state pushes — send structured updates to subscribed addons.
    if !bot.addon_subs.is_empty() && now_ms.saturating_sub(bot.last_addon_push_ms) >= 2000 {
        push_addon_state(bot, now_ms);
        bot.last_addon_push_ms = now_ms;
    }

    // 8. KLHThreatMeter broadcast — send threat to group so human players
    //    with KTM installed can see bot threat values.
    broadcast_ktm_threat(bot, now_ms);

    // 9. Advance timers
    bot.timers.advance(now_ms);
}

/// Broadcast this bot's threat to the group via the `KLHThreatMeter` addon
/// protocol. Mirrors the update cadence from `KTM_Net.lua:updatethreattoraid`:
///   - 500ms when threat is changing
///   - 5000ms when stable
///   - skip entirely when threat is 0 and was already 0
fn broadcast_ktm_threat(bot: &mut BotState, now_ms: u64) {
    // Only broadcast when grouped with a human player.
    if bot.master_guid.is_none() || bot.snap.group_size == 0 {
        return;
    }

    // Determine current threat on our target.
    let target = bot.snap.self_.current_target;
    let threat_int = if target != 0 && bot.snap.self_.in_combat {
        bot.interface
            .get_unit_threat(target, bot.handle)
            as i64
    } else {
        0
    };

    // Don't send when threat is 0 and was already 0.
    if threat_int == 0 && bot.last_ktm_threat_sent == 0 {
        return;
    }

    // Throttle: 500ms if value changed, 5000ms if stable.
    let interval = if threat_int != bot.last_ktm_threat_sent {
        500
    } else {
        5000
    };
    if now_ms.saturating_sub(bot.last_ktm_threat_time_ms) < interval {
        return;
    }

    // Send the update.
    let msg = format!("t {}", threat_int);
    bot.interface.send_group_addon("KLHTM", &msg);
    bot.last_ktm_threat_sent = threat_int;
    bot.last_ktm_threat_time_ms = now_ms;
}

/// Resolve the active boss unit handle by `npc_entry` from the encounter FSM.
///
/// Scans, in priority order: current target, attackers, nearby units. Returns
/// `0` when there is no encounter active, no known boss entry, or no match.
/// This is used by HP polling and belief population so HP-gated phase
/// transitions fire correctly for bots that aren't directly targeting the boss.
fn resolve_boss_handle(bot: &BotState) -> cmangos::UnitHandle {
    let Some(enc) = bot.encounter.as_ref() else {
        return 0;
    };
    let boss_entry = enc.boss_entry();
    if boss_entry == 0 {
        return 0;
    }
    // 1. Current target
    let target = bot.snap.self_.current_target;
    if target != 0 {
        let s = bot.interface.get_unit_snapshot(target);
        if s.npc_entry == boss_entry {
            return target;
        }
    }
    // 2. Attackers
    for &h in &bot.attackers {
        if h == 0 {
            continue;
        }
        let s = bot.interface.get_unit_snapshot(h);
        if s.npc_entry == boss_entry {
            return h;
        }
    }
    // 3. Nearby units (most expensive — bounded by LOD scan range)
    for &h in &bot.nearby_units {
        if h == 0 {
            continue;
        }
        let s = bot.interface.get_unit_snapshot(h);
        if s.npc_entry == boss_entry {
            return h;
        }
    }
    0
}

/// Read boss HP as a fraction (0.0..=1.0) from the resolved boss handle.
/// Returns 1.0 when no boss is resolvable so HP-gated transitions don't fire
/// spuriously on the idle value.
fn resolve_boss_hp_pct(bot: &BotState) -> f32 {
    let handle = resolve_boss_handle(bot);
    if handle == 0 {
        return 1.0;
    }
    let s = bot.interface.get_unit_snapshot(handle);
    if s.max_health == 0 {
        return 1.0;
    }
    (s.health as f32 / s.max_health as f32).clamp(0.0, 1.0)
}

/// Squared 3D distance considered "arrived" at an RTSC waypoint.
/// Slightly larger than the regular pathing tolerance because RTSC
/// targets are master-clicked terrain points where the master rarely
/// cares about sub-yard precision.
const RTSC_ARRIVAL_DIST_SQ: f32 = 4.0; // 2 yards

/// Drive an active [`ForcedIntention::MoveToRtsc`] /
/// [`ForcedIntention::JumpRtsc`] each tick. Issues `move_to` until the
/// bot is within [`RTSC_ARRIVAL_DIST_SQ`] of the target, then clears
/// the intention. The two-stage jump flips `at_stage_two` once stage
/// one is reached and fires `bot_jump()` on the second arrival.
///
/// This is the consumer that makes RTSC moves survive across BT ticks
/// (Gap #10) and replaces the never-wired `RtscConsumeMoveQueue` BT
/// leaf (Gap #11). The jump executor (Gap #12) is the same function
/// because both kinds share the same forced-intention slot.
fn tick_rtsc_forced_intention(bot: &mut BotState) {
    use crate::bdi::intentions::ForcedIntention;

    let Some(forced) = bot.bdi.forced_intention else {
        return;
    };
    let pos = bot.snap.self_.pos;
    match forced {
        ForcedIntention::MoveToRtsc { x, y, z, exact: _ } => {
            let dx = pos.x - x;
            let dy = pos.y - y;
            let dz = pos.z - z;
            if dx * dx + dy * dy + dz * dz <= RTSC_ARRIVAL_DIST_SQ {
                // Arrived — drop the intention so normal BDI/GOAP
                // resumes next tick.
                bot.bdi.forced_intention = None;
                bot.bdi.intention_changed = true;
            } else {
                bot.interface.move_to(x, y, z);
            }
        }
        ForcedIntention::JumpRtsc {
            stage1,
            stage2,
            at_stage_two,
        } => {
            let target = if at_stage_two { stage2 } else { stage1 };
            let dx = pos.x - target.0;
            let dy = pos.y - target.1;
            let dz = pos.z - target.2;
            if dx * dx + dy * dy + dz * dz <= RTSC_ARRIVAL_DIST_SQ {
                if at_stage_two {
                    // Stage two reached — fire the actual jump and clear
                    // both the intention and the strategy bit.
                    bot.interface.bot_jump();
                    bot.bdi.forced_intention = None;
                    bot.bdi.intention_changed = true;
                    bot.settings.rtsc_waypoints.remove(crate::rtsc::JUMP_SLOT);
                    bot.settings
                        .rtsc_waypoints
                        .remove(crate::rtsc::JUMP_POINT_SLOT);
                    use crate::bot::settings::{BotStateKind, StrategyFlags};
                    bot.settings
                        .strategies
                        .get_mut(BotStateKind::NonCombat)
                        .remove(StrategyFlags::RTSC_JUMP);
                } else {
                    // Stage one reached — flip to stage two and start
                    // moving toward the jump point on the next tick.
                    bot.bdi.forced_intention = Some(ForcedIntention::JumpRtsc {
                        stage1,
                        stage2,
                        at_stage_two: true,
                    });
                }
            } else {
                bot.interface.move_to(target.0, target.1, target.2);
            }
        }
        ForcedIntention::Desire { .. } => {
            // Non-RTSC forced intention — handled by the normal BDI/GOAP path.
        }
    }
}

fn process_events(bot: &mut BotState, _now_ms: u64) {
    // Determine boss HP for FSM updates.
    //
    // The encounter FSM needs HP of the *boss* — not whatever the bot is
    // currently targeting. A DPS bot might target an add, a healer might
    // target the tank, etc. Use the encounter's boss entry to find the
    // correct unit, scanning attackers, current target, and nearby units.
    let boss_hp_pct = resolve_boss_hp_pct(bot);

    let now_ms = bot.snap.server_time_ms;

    // Drain the mutex-protected event queue into a local vec. This is
    // critical for thread safety — push-event FFI functions can fire from
    // the session thread while we run on the map-worker thread.
    let events: Vec<_> = bot.events.lock().unwrap().drain(..).collect();

    // First pass: convert bot events to encounter events and dispatch.
    for event in events {
        use crate::bot::events::BotEvent;

        let enc_event = match &event {
            BotEvent::UnitSpellCast {
                caster,
                spell_id,
                target: _,
                success,
            } => Some(EncounterEvent::SpellCast {
                caster: *caster,
                spell_id: *spell_id,
                success: *success,
            }),
            BotEvent::AuraChanged {
                unit,
                spell_id,
                applied,
                ..
            } => Some(EncounterEvent::AuraChanged {
                unit: *unit,
                spell_id: *spell_id,
                applied: *applied,
            }),
            BotEvent::UnitDied { victim, .. } => Some(EncounterEvent::UnitDied { victim: *victim }),
            BotEvent::DamageTaken { .. } => None,
            BotEvent::PacketIn { .. } | BotEvent::PacketOut { .. } => None,
        };

        // Dispatch to encounter FSM.
        if let (Some(enc_ev), Some(enc)) = (enc_event, &mut bot.encounter) {
            enc.update(&enc_ev, boss_hp_pct, now_ms);
        }

        // Generic blackboard side effects.
        if let BotEvent::UnitDied { victim: _, .. } = &event {
            use crate::engine::blackboard::{Key, Value};
            if let Some(count) = bot.blackboard.get_u32(Key::AddCount)
                && count > 0
            {
                bot.blackboard.set(Key::AddCount, Value::U32(count - 1));
            }
        }
    }

    // Regular tick: update encounter FSM with None event (HP polling).
    if let Some(enc) = &mut bot.encounter {
        enc.update(&EncounterEvent::None, boss_hp_pct, now_ms);
        // Write encounter hints to blackboard for BT nodes.
        use crate::engine::blackboard::{Key, Value};
        // Current phase — read by phase-gated BT leaves and overlay trees.
        bot.blackboard
            .set(Key::CurrentEncounterPhase, Value::U32(enc.phase_id()));
        let zone = enc.safe_zone_hint();
        if zone > 0 {
            bot.blackboard
                .set(Key::EncounterSafeZone, Value::U32(zone as u32));
        } else {
            bot.blackboard.clear(Key::EncounterSafeZone);
        }
    } else {
        // No encounter — clear stale phase/zone blackboard entries so
        // gated BT leaves don't read values from a previous encounter.
        use crate::engine::blackboard::Key;
        bot.blackboard.clear(Key::CurrentEncounterPhase);
        bot.blackboard.clear(Key::EncounterSafeZone);
    }

    // Boss detection: when the encounter FSM has no active boss yet, scan
    // the bot's target and attackers for known boss NPC entries. This is
    // how instance wrappers (MC, BWL, etc.) learn which boss fight is
    // happening and switch to the correct sub-FSM.
    if bot.snap.self_.in_combat
        && let Some(enc) = &mut bot.encounter
            && enc.boss_entry() == 0 {
                // Check current target first.
                let target = bot.snap.self_.current_target;
                if target != 0 {
                    let snap = bot.interface.get_unit_snapshot(target);
                    if snap.npc_entry != 0 {
                        enc.set_boss_entry(snap.npc_entry);
                    }
                }
                // If still no boss, scan attackers.
                if enc.boss_entry() == 0 {
                    for &attacker in &bot.attackers {
                        let snap = bot.interface.get_unit_snapshot(attacker);
                        if snap.npc_entry != 0 {
                            enc.set_boss_entry(snap.npc_entry);
                            if enc.boss_entry() != 0 {
                                break;
                            }
                        }
                    }
                }
            }

    // If bot entered combat, notify encounter FSM.
    if bot.snap.self_.in_combat
        && let Some(enc) = &mut bot.encounter
        && !enc.is_active()
    {
        enc.update(&EncounterEvent::CombatStarted, boss_hp_pct, now_ms);
    }
}

/// Update the travel target FSM and sync its destination to the blackboard.
///
/// Called before the BT runs each tick. This handles:
/// - Checking if the current travel status should advance/expire.
/// - Writing the active destination coords to the blackboard so
///   `TravelToBlackboard` can navigate there.
/// - Clearing the blackboard when the target is no longer active.
fn update_travel_target(bot: &mut BotState, now_ms: u64) {
    use crate::travel::destination::TravelStatus;
    use crate::travel::planner;

    let pos = &bot.snap.self_.pos;
    let tt = &mut bot.travel_target;

    // Skip if no active target.
    //
    // IMPORTANT: the `TravelTarget` FSM is currently never populated — the
    // active travel system is the BT (`ChooseTravelTarget` writes the
    // blackboard destination directly, and `TravelToBlackboard`/`tick_travel`
    // owns arrival, replanning, and clearing). This sync function must NOT
    // clear the blackboard destination when the FSM is inactive: doing so wiped
    // the destination the BT set on the previous tick (this runs first each
    // tick), so the bot never kept a travel goal — `dest=none` forever — and
    // could never reach a quest objective or turn-in. Just return and let the
    // BT own the blackboard destination.
    if !tt.is_active() {
        return;
    }

    // Check arrival — are we within 5 yards of the destination?
    let at_dest = tt.destination.dist_sq_2d(pos.x, pos.y) < 25.0;

    // Advance FSM.
    tt.check_status(now_ms, at_dest);

    // Sync to blackboard based on status.
    match tt.status {
        TravelStatus::Travel => {
            // Write destination to blackboard so TravelToBlackboard picks it up.
            planner::set_travel_dest(&mut bot.blackboard, &tt.destination);
        }
        TravelStatus::Work | TravelStatus::Cooldown => {
            // Clear the movement destination — we've arrived.
            planner::clear_travel_dest(&mut bot.blackboard);
        }
        TravelStatus::Expired | TravelStatus::None => {
            // Target is done — clear everything.
            planner::clear_travel_dest(&mut bot.blackboard);
            tt.clear();
        }
        _ => {
            // Prepare/Ready — don't move yet.
        }
    }
}

/// Publish local encounter FSM state to the shared `GroupState`.
///
/// Every bot in the group writes its local encounter view. Since all bots
/// process the same events, they converge. The shared state is the
/// coordination surface — `ClaimTable` lives here, encounter phase is readable
/// by any bot without polling the local FSM of others.
///
/// Also GCs expired claims every ~5 seconds (piggybacks on a write lock
/// we're already taking).
fn sync_encounter_to_group(bot: &mut BotState, now_ms: u64) {
    let group_handle = match &bot.group_state {
        Some(h) => h,
        None => return,
    };

    // Only attempt a write lock — don't block the tick if someone else holds it.
    let Ok(mut gs) = group_handle.state().try_write() else {
        return;
    };

    // Sync encounter metadata, publishing raid-wide events on transitions
    // so out-of-LOS / dormant-LOD bots don't miss phase changes or wipes.
    // Captured BEFORE the overwrite so we can compare old vs new.
    let prev_active = gs.encounter.active;
    let prev_phase = gs.encounter.phase_id;
    if let Some(enc) = &bot.encounter {
        let new_active = enc.is_active();
        let new_phase = enc.phase_id();
        gs.encounter.boss_entry = enc.boss_entry();
        gs.encounter.phase_id = new_phase;
        gs.encounter.active = new_active;
        gs.encounter_active = new_active;

        // Phase transitions inside an active encounter are interesting
        // to every bot regardless of LOD — publish on change.
        if new_active && new_phase != prev_phase {
            gs.encounter
                .publish_event(crate::engine::group_state::RaidEventKind::PhaseChange(new_phase), now_ms);
        }
        // Active → inactive transition during a fight is a wipe (or kill).
        // We can't distinguish here without boss death info, but the
        // raid-wide reset behavior is identical, so publish Wipe.
        if prev_active && !new_active {
            gs.encounter
                .publish_event(crate::engine::group_state::RaidEventKind::Wipe, now_ms);
        }
    } else {
        gs.encounter.active = false;
        gs.encounter_active = false;
        if prev_active {
            gs.encounter
                .publish_event(crate::engine::group_state::RaidEventKind::Wipe, now_ms);
        }
    }

    // Auto-register tanks in group coordination. Bots with the TANK flag
    // register as main tank (first) or off-tank (subsequent). This ensures
    // DPS bots can assist the tank and targeting subtrees work correctly.
    {
        use crate::bot::settings::{BotStateKind, StrategyFlags};
        let combat_flags = bot.settings.strategies.get(BotStateKind::Combat);
        if combat_flags.contains(StrategyFlags::TANK) {
            if gs.coordination.main_tank().is_none() {
                gs.coordination.set_main_tank(bot.handle);
            } else if gs.coordination.main_tank() != Some(bot.handle)
                && !gs.coordination.off_tanks().any(|h| h == bot.handle)
            {
                gs.coordination.add_off_tank(bot.handle);
            }
        }
    }

    // Sync paladin blessings to group coordination.
    if bot.class == crate::bot::state::PlayerClass::Paladin
        && let crate::bot::class_prefs::ClassPrefs::Paladin(ref prefs) = bot.settings.class_prefs
            && let Some(blessing) = prefs.blessing {
                let ranks = blessing.ranks();
                if let Some(&spell_id) = ranks.first() {
                    gs.coordination.set_paladin_blessing(bot.handle, spell_id);
                }
            }

    // Sync heal priority — ensure main tank is always heal_priority[0].
    if let Some(mt) = gs.coordination.main_tank()
        && gs.coordination.heal_priority[0] != mt {
            gs.coordination.heal_priority.rotate_right(1);
            gs.coordination.heal_priority[0] = mt;
        }

    // Rebuild the shared EncounterAssignments roster from the current
    // group snapshot. Every bot runs this — the input (per-member
    // class + role from get_unit_snapshot) is deterministic across
    // bots, so all group members converge on the same roster without
    // further coordination.
    //
    // Cost: ~50ns per get_unit_snapshot × up-to-40 members ≈ 2µs,
    // negligible next to the belief population that already runs this
    // tick. We iterate directly instead of collecting into a Vec so
    // the rebuild consumes a zero-alloc iterator.
    {
        let size = bot.snap.group_size as usize;
        let cap = bot.snap.group_members.len();
        let members = bot.snap.group_members[..size.min(cap)]
            .iter()
            .copied()
            .filter(|&h| h != 0)
            .map(|h| {
                let us = bot.interface.get_unit_snapshot(h);
                (h, us.class_id, us.role)
            });
        gs.assignments.rebuild(members);
    }

    // Publish this bot's BDI desire to the group for coordination.
    gs.publish_desire(bot.handle, bot.bdi.active_desire() as u8);

    // Periodic claim GC — every 5 seconds, sweep expired claims.
    // Cheap: iterates a 32-element array.
    if now_ms.saturating_sub(gs.last_computed_ms) >= 5000 {
        gs.encounter.claims.gc(now_ms);
        gs.last_computed_ms = now_ms;
    }

    // Drain raid-wide events newer than what this bot has already seen.
    // Collect inside the lock, then drop it before mutating bot state.
    let (new_events, new_seq) = gs
        .encounter
        .drain_events_since(bot.last_seen_raid_event_seq, now_ms);
    drop(gs);

    if !new_events.is_empty() {
        bot.last_seen_raid_event_seq = new_seq;
        for ev in &new_events {
            apply_raid_event(bot, ev, now_ms);
        }
    } else {
        // Keep the high-water mark in sync even on no-op drains so a
        // wrapped sequence (>16 publishes between drains) isn't replayed.
        bot.last_seen_raid_event_seq = new_seq;
    }
}

/// Fan-out side effects for a single drained raid event.
///
/// Bots that are LOD-dormant or out of LOS may never see the originating
/// snapshot transition, so this is the only path by which they learn that
/// the boss phase changed or the group wiped. Keep the per-event work tiny
/// — this runs once per event per bot per tick under the bot mutex.
fn apply_raid_event(
    bot: &mut BotState,
    ev: &crate::engine::group_state::RaidEvent,
    now_ms: u64,
) {
    use crate::encounters::EncounterEvent;
    use crate::engine::group_state::RaidEventKind;

    match ev.kind {
        RaidEventKind::Wipe => {
            // Force the local FSM into wipe handling so dormant bots
            // tear down combat-only state alongside everyone else.
            if let Some(enc) = bot.encounter.as_deref_mut() {
                enc.update(&EncounterEvent::GroupWipe, 0.0, now_ms);
            }
            bot.bdi.beliefs.encounter_active = false;
        }
        RaidEventKind::PhaseChange(_) | RaidEventKind::BossCast(_) => {
            // Mark the encounter active for desire reasoning even on a
            // dormant tier; the local FSM will catch up to the new phase
            // on its own next snapshot tick.
            bot.bdi.beliefs.encounter_active = true;
        }
        RaidEventKind::AddSpawn(_) => {
            // Pickup is brokered through `ClaimTable::AddPickup` (Gap #9);
            // the fanout itself just informs reasoning that adds exist.
            bot.bdi.beliefs.encounter_active = true;
        }
        RaidEventKind::None => {}
    }
}

/// Populate group + target beliefs from unit snapshots.
///
/// Iterates group members and calls `get_unit_snapshot()` per live member to
/// fill group-level belief fields (min HP%, injured count, dead count,
/// tank/healer alive). Also fills target HP% from the current target snapshot.
///
/// Called before `beliefs::update()` so the derived fields (`party_needs_heals`,
/// `party_needs_rez`) incorporate group data.
///
/// Cost: ~50ns per `get_unit_snapshot()` × 40 members max = ~2µs worst case.
fn populate_group_beliefs(bot: &mut BotState) {
    use cmangos::BotRole;

    let beliefs = &mut bot.bdi.beliefs;
    let size = bot.snap.group_size as usize;

    // Reset group fields to defaults before accumulating.
    beliefs.group_hp_min_pct = 100;
    beliefs.group_injured_count = 0;
    beliefs.group_dead_count = 0;
    beliefs.tank_alive = false;
    beliefs.healer_alive = false;
    beliefs.tank_hp_pct = 100;

    if size > 0 {
        let cap = bot.snap.group_members.len();
        for &h in &bot.snap.group_members[..size.min(cap)] {
            if h == 0 || h == bot.handle {
                continue;
            }
            let us = bot.interface.get_unit_snapshot(h);

            if !us.is_alive {
                beliefs.group_dead_count = beliefs.group_dead_count.saturating_add(1);
                continue;
            }

            let hp_pct = if us.max_health > 0 {
                ((us.health as u64 * 100) / us.max_health as u64).min(100) as u8
            } else {
                0
            };

            if hp_pct < beliefs.group_hp_min_pct {
                beliefs.group_hp_min_pct = hp_pct;
            }
            if hp_pct < 80 {
                beliefs.group_injured_count = beliefs.group_injured_count.saturating_add(1);
            }

            let role = BotRole(us.role);
            if role.is_tank() {
                beliefs.tank_alive = true;
                if hp_pct < beliefs.tank_hp_pct {
                    beliefs.tank_hp_pct = hp_pct;
                }
            }
            if role.is_heal() {
                beliefs.healer_alive = true;
            }
        }
    }

    // Target HP from unit snapshot.
    let target = bot.snap.self_.current_target;
    if target != 0 {
        let ts = bot.interface.get_unit_snapshot(target);
        beliefs.target_hp_pct = if ts.max_health > 0 {
            ((ts.health as u64 * 100) / ts.max_health as u64).min(100) as u8
        } else {
            0
        };
    } else {
        beliefs.target_hp_pct = 0;
    }
}

/// Actions to perform when leaving an FSM state.
fn on_fsm_exit(bot: &mut BotState, prev: ActiveFsm) {
    if bot.monitor_active {
        crate::bot::monitor::monitor_log(bot, &format!("FSM EXIT: {:?}", prev));
    }
    match prev {
        ActiveFsm::Combat => {
            // Leaving combat: clear combat-only blackboard keys and
            // transient targeting state so the bot returns to Follow.
            use crate::engine::blackboard::Key;
            bot.blackboard.clear(Key::LastAttackTarget);
            bot.blackboard.clear(Key::IsPulling);
            bot.settings.focus_target = None;

            // Clear heal target tracking so stale assignments don't
            // persist into the next combat.
            if bot.role.is_heal()
                && let Some(ref gh) = bot.group_state
                    && let Ok(mut gs) = gh.state().try_write() {
                        gs.clear_heal_target(bot.handle);
                    }

            // Cancel feign death if the hunter is still lying down.
            // Feign death applies an aura that persists until manually
            // cancelled — without this the hunter stays "dead" forever.
            if bot.class == crate::bot::state::PlayerClass::Hunter {
                use crate::data::spells::vanilla::hunter::FEIGN_DEATH;
                if bot.interface.has_aura(bot.handle, FEIGN_DEATH) {
                    bot.interface.remove_aura(FEIGN_DEATH);
                }
            }
        }
        ActiveFsm::Dead => {
            // Revived: clear death-related state.
        }
        ActiveFsm::World => {}
    }
}

/// Actions to perform when entering an FSM state.
fn on_fsm_enter(bot: &mut BotState, new: ActiveFsm) {
    if bot.monitor_active {
        crate::bot::monitor::monitor_log(bot, &format!("FSM ENTER: {:?}", new));
    }
    match new {
        ActiveFsm::Combat => {
            // Entering combat: release any stale heal claims so we start fresh.
            if let Some(ref gh) = bot.group_state
                && let Ok(mut gs) = gh.state().try_write() {
                    gs.encounter.claims.release_all(bot.handle);
                }
        }
        ActiveFsm::Dead => {}
        ActiveFsm::World => {}
    }
}

/// Actions when entering a World sub-state.
fn on_world_sub_enter(bot: &mut BotState, sub: WorldSub) {
    if bot.monitor_active {
        crate::bot::monitor::monitor_log(bot, &format!("WORLD SUB ENTER: {:?}", sub));
    }
}

/// Actions when leaving a World sub-state.
fn on_world_sub_exit(bot: &mut BotState, prev: WorldSub) {
    if bot.monitor_active {
        crate::bot::monitor::monitor_log(bot, &format!("WORLD SUB EXIT: {:?}", prev));
    }
}

/// Push structured state updates to subscribed addons.
///
/// Called every ~2 seconds when any subscriptions exist. Each category
/// is only sent if at least one player is subscribed to it.
fn push_addon_state(bot: &BotState, _now_ms: u64) {
    use crate::commands::protocol::{StateCategory, format_state_update};

    let subs = &bot.addon_subs;

    // FSM state
    if subs.has_subscribers(StateCategory::Fsm) {
        let fsm_str = format!("{:?}", bot.active_fsm);
        let sub_str = bot
            .blackboard
            .get_u32(crate::engine::blackboard::Key::WorldSubState)
            .map_or("n/a", |v| match v {
                0 => "Follow",
                1 => "Grind",
                2 => "Quest",
                3 => "Rest",
                4 => "Guard",
                5 => "Rpg",
                6 => "Stay",
                7 => "Bg",
                8 => "Travel",
                _ => "?",
            });
        let msg = format_state_update(StateCategory::Fsm, &[("state", &fsm_str), ("sub", sub_str)]);
        for guid in subs.subscribers(StateCategory::Fsm) {
            bot.interface.tell_addon(guid, &msg);
        }
    }

    // Vitals
    if subs.has_subscribers(StateCategory::Vitals) {
        let hp = format!(
            "{:.0}",
            bot.snap.self_.health as f32 / bot.snap.self_.max_health.max(1) as f32 * 100.0
        );
        let mp = format!(
            "{:.0}",
            bot.snap.self_.mana as f32 / bot.snap.self_.max_mana.max(1) as f32 * 100.0
        );
        let msg = format_state_update(StateCategory::Vitals, &[("hp", &hp), ("mp", &mp)]);
        for guid in subs.subscribers(StateCategory::Vitals) {
            bot.interface.tell_addon(guid, &msg);
        }
    }

    // Encounter
    if subs.has_subscribers(StateCategory::Encounter) {
        let (boss, phase, active) = bot
            .encounter
            .as_ref().map_or_else(|| ("0".to_string(), "0".to_string(), "0"), |e| {
                (
                    e.boss_entry().to_string(),
                    e.phase_id().to_string(),
                    if e.is_active() { "1" } else { "0" },
                )
            });
        let msg = format_state_update(
            StateCategory::Encounter,
            &[("boss", &boss), ("phase", &phase), ("active", active)],
        );
        for guid in subs.subscribers(StateCategory::Encounter) {
            bot.interface.tell_addon(guid, &msg);
        }
    }

    // BT path
    if subs.has_subscribers(StateCategory::BtPath) && !bot.last_bt_path.is_empty() {
        let msg = format_state_update(StateCategory::BtPath, &[("path", &bot.last_bt_path)]);
        for guid in subs.subscribers(StateCategory::BtPath) {
            bot.interface.tell_addon(guid, &msg);
        }
    }
}
