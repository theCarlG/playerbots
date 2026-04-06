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
    engine::{bt_nodes::BtNode, context::TickContext},
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

    // 4.5 Process pending chat commands → mutate settings
    crate::commands::process_commands(bot);

    // 4.6 Update travel target FSM — check status, sync to blackboard.
    update_travel_target(bot, now_ms);

    // 5. Build TickContext and run BT
    //
    // Destructure bot so the borrow checker sees each field independently.
    // This avoids the raw pointer cast that was previously needed.
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
        ref encounter,
        ref root_tree,
        handle: bot_handle,
        class,
        role,
        ref settings,
        ..
    } = *bot;

    let ctx_group = group_state
        .as_ref()
        .and_then(|handle| handle.state().try_read().ok());

    let mut ctx = TickContext {
        snap,
        nearby: nearby_units,
        attackers,
        group_state: ctx_group.as_deref(),
        interface: interface.as_ref(),
        blackboard,
        timers,
        throttles,
        server_time_ms: now_ms,
        elapsed_ms,
        minimal,
        bot_handle,
        master_guid,
        encounter: encounter.as_deref(),
        class,
        role,
        settings,
    };

    let _ = root_tree.tick(&mut ctx);

    // 7. Advance timers
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

    // First pass: convert bot events to encounter events and dispatch.
    while let Some(event) = bot.events.pop_front() {
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
                && count > 0 {
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

    // If bot entered combat, notify encounter FSM.
    if bot.snap.self_.in_combat
        && let Some(enc) = &mut bot.encounter
            && !enc.is_active() {
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
    use crate::travel::planner;
    use crate::travel::destination::TravelStatus;

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
