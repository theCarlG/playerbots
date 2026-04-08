/// Main tick logic — called from `playerbot_update`.
///
/// Sequence per tick:
///   1. Refresh snapshot (one C++ call)
///   2. Throttled refresh of nearby/attacker lists
///   3. Zone-change check → create/destroy encounter FSM
///   4. Process push events → update encounter FSM + blackboard
///   5. Build `TickContext`
///   6. Run the BT tree
///   7. Advance timers
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

    // 2. Throttled attacker/nearby refresh
    if !minimal && now_ms.saturating_sub(bot.last_attackers_refresh_ms) >= cfg.attacker_refresh_ms {
        bot.attackers = bot
            .interface
            .get_nearby_units(cfg.attacker_scan_range, true);
        bot.last_attackers_refresh_ms = now_ms;
    }
    if !minimal && now_ms.saturating_sub(bot.last_nearby_refresh_ms) >= cfg.nearby_refresh_ms {
        bot.nearby_units = bot.interface.get_nearby_units(cfg.nearby_scan_range, false);
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

    // 5. Determine ActiveFsm (Dead > Combat > World)
    // A focus_target (from pull/attack commands) also triggers Combat so
    // the bot approaches and engages rather than staying in Follow mode.
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

    // 8. Advance timers
    bot.timers.advance(now_ms);
}

fn process_events(bot: &mut BotState, _now_ms: u64) {
    // Determine boss HP for FSM updates.
    let boss_hp_pct = {
        let target = bot.snap.self_.current_target;
        if target != 0 {
            let snap = bot.interface.get_unit_snapshot(target);
            if snap.max_health > 0 {
                snap.health as f32 / snap.max_health as f32
            } else {
                1.0
            }
        } else {
            1.0
        }
    };

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
        let zone = enc.safe_zone_hint();
        if zone > 0 {
            bot.blackboard
                .set(Key::EncounterSafeZone, Value::U32(zone as u32));
        }
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
    use crate::engine::blackboard::Key;
    use crate::travel::destination::TravelStatus;
    use crate::travel::planner;

    let pos = &bot.snap.self_.pos;
    let tt = &mut bot.travel_target;

    // Skip if no active target.
    if !tt.is_active() {
        // Make sure blackboard is clear if target is gone.
        if bot.blackboard.get_f32(Key::TravelDestX).is_some() {
            planner::clear_travel_dest(&mut bot.blackboard);
        }
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

    // Sync encounter metadata.
    if let Some(enc) = &bot.encounter {
        gs.encounter.boss_entry = enc.boss_entry();
        gs.encounter.phase_id = enc.phase_id();
        gs.encounter.active = enc.is_active();
        gs.encounter_active = enc.is_active();
    } else {
        gs.encounter.active = false;
        gs.encounter_active = false;
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

    // Periodic claim GC — every 5 seconds, sweep expired claims.
    // Cheap: iterates a 32-element array.
    if now_ms.saturating_sub(gs.last_computed_ms) >= 5000 {
        gs.encounter.claims.gc(now_ms);
        gs.last_computed_ms = now_ms;
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
            bot.settings.focus_target = None;
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
