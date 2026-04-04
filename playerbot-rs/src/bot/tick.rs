/// Main tick logic — called from `playerbot_update`.
///
/// Sequence per tick:
///   1. Refresh snapshot (one C++ call)
///   2. Throttled refresh of nearby/attacker lists
///   3. Zone-change check → create/destroy encounter FSM
///   4. Process push events → update encounter FSM + blackboard
///   5. Build TickContext
///   6. Run the BT tree
///   7. Advance timers
use crate::{
    bot::state::BotState,
    encounters::{coordinator, EncounterEvent},
    engine::context::TickContext,
};

const ATTACKERS_REFRESH_INTERVAL_MS:  u64 = 500;
const NEARBY_REFRESH_INTERVAL_MS:     u64 = 1000;

pub fn tick(bot: &mut BotState, elapsed_ms: u32, minimal: bool) {
    // 1. Refresh snapshot (always — one C++ call)
    bot.snap = bot.interface.get_snapshot();
    let now_ms = bot.snap.server_time_ms;

    // 2. Throttled attacker/nearby refresh
    if !minimal && now_ms.saturating_sub(bot.last_attackers_refresh_ms) >= ATTACKERS_REFRESH_INTERVAL_MS {
        bot.attackers = bot.interface.get_nearby_units(40.0, true);
        bot.last_attackers_refresh_ms = now_ms;
    }
    if !minimal && now_ms.saturating_sub(bot.last_nearby_refresh_ms) >= NEARBY_REFRESH_INTERVAL_MS {
        bot.nearby_units = bot.interface.get_nearby_units(60.0, false);
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

    // 5. Build TickContext and run BT
    let snap = &bot.snap;
    let attackers: &[u64] = &bot.attackers;
    let nearby: &[u64] = &bot.nearby_units;

    let ctx_group = bot.group_state.as_ref()
        .and_then(|arc| arc.try_read().ok());

    let bot_handle = bot.handle;
    let mut ctx = TickContext {
        snap,
        nearby,
        attackers,
        group_state: ctx_group.as_deref(),
        interface:   bot.interface.as_ref(),
        blackboard:  &mut bot.blackboard,
        timers:      &mut bot.timers,
        server_time_ms: now_ms,
        elapsed_ms,
        minimal,
        bot_handle,
    };

    // Run the root BT tree via a raw pointer to avoid double-borrow on bot.
    let tree_ptr = &*bot.root_tree as *const dyn crate::engine::bt_nodes::BtNode;
    // SAFETY: root_tree is alive for the duration of this tick; ctx does not hold
    // references into root_tree; root_tree does not hold references into ctx.
    let result = unsafe { (*tree_ptr).tick(&mut ctx) };
    let _ = result; // Running is handled by blackboard state across ticks

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
            } else { 1.0 }
        } else { 1.0 }
    };

    let now_ms = bot.snap.server_time_ms;

    // First pass: convert bot events to encounter events and dispatch.
    while let Some(event) = bot.events.pop_front() {
        use crate::bot::events::BotEvent;

        let enc_event = match &event {
            BotEvent::UnitSpellCast { caster, spell_id, target: _, success } =>
                Some(EncounterEvent::SpellCast {
                    caster: *caster, spell_id: *spell_id, success: *success
                }),
            BotEvent::AuraChanged { unit, spell_id, applied, .. } =>
                Some(EncounterEvent::AuraChanged {
                    unit: *unit, spell_id: *spell_id, applied: *applied
                }),
            BotEvent::UnitDied { victim, .. } =>
                Some(EncounterEvent::UnitDied { victim: *victim }),
            BotEvent::DamageTaken { .. } => None,
            BotEvent::PacketIn { .. } | BotEvent::PacketOut { .. } => None,
        };

        // Dispatch to encounter FSM.
        if let (Some(enc_ev), Some(enc)) = (enc_event, &mut bot.encounter) {
            enc.update(&enc_ev, boss_hp_pct, now_ms);
        }

        // Generic blackboard side effects.
        match &event {
            BotEvent::UnitDied { victim: _, .. } => {
                use crate::engine::blackboard::{Key, Value};
                if let Some(count) = bot.blackboard.get_u32(Key::AddCount) {
                    if count > 0 {
                        bot.blackboard.set(Key::AddCount, Value::U32(count - 1));
                    }
                }
            }
            _ => {}
        }
    }

    // Regular tick: update encounter FSM with None event (HP polling).
    if let Some(enc) = &mut bot.encounter {
        enc.update(&EncounterEvent::None, boss_hp_pct, now_ms);
    }

    // If bot entered combat, notify encounter FSM.
    if bot.snap.self_.in_combat {
        if let Some(enc) = &mut bot.encounter {
            if !enc.is_active() {
                enc.update(&EncounterEvent::CombatStarted, boss_hp_pct, now_ms);
            }
        }
    }
}

// Allow accessing self_ field generated by bindgen (which uses C naming)
trait SnapSelfAccess {
    fn self_unit(&self) -> &crate::ffi::BotUnitSnapshot;
}
impl SnapSelfAccess for crate::ffi::BotWorldSnapshot {
    fn self_unit(&self) -> &crate::ffi::BotUnitSnapshot {
        &self.self_
    }
}
