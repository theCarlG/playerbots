/// Arcane Mage behavior tree (Classic / Vanilla).
///
/// Vanilla arcane is mostly support. Rotation: Counterspell → Arcane Missiles →
///   Arcane Explosion (AoE) → Frostbolt (filler) → Arcane Intellect (buff)
use crate::{
    data::spells::vanilla::mage::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cast_on_current_target, cd_gate,
                    cond, gcd_gate, sel, seq},
        context::TickContext,
    },
};

pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Ice Block emergency
        seq(vec![
            cond(|ctx| ctx.self_hp_pct() < 0.20),
            gcd_gate(cd_gate(ICE_BLOCK, action(|ctx| {
                let me = ctx.bot_handle;
                if ctx.interface.cast_spell(ICE_BLOCK, me) {
                    ctx.timers.on_spell_cast(ICE_BLOCK, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        // Evocation
        seq(vec![
            cond(|ctx| ctx.self_mana_pct() < 0.10),
            gcd_gate(cd_gate(EVOCATION, action(|ctx| {
                let me = ctx.bot_handle;
                if ctx.interface.cast_spell(EVOCATION, me) {
                    ctx.timers.on_spell_cast(EVOCATION, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        seq(vec![
            cond(|ctx| ctx.in_combat()),
            sel(vec![
                // Counterspell
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        ctx.interface.get_unit_snapshot(t).is_casting)),
                    cast_on_current_target(COUNTERSPELL),
                ]),

                // Fire Blast execute
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t| {
                        let s = ctx.interface.get_unit_snapshot(t);
                        s.max_health > 0 && (s.health as f32 / s.max_health as f32) < 0.20
                    })),
                    cast_on_current_target(FIRE_BLAST),
                ]),

                // Arcane Missiles — main channeled nuke
                cast_on_current_target(ARCANE_MISSILES),

                // Frostbolt — efficient filler
                cast_on_current_target(FROSTBOLT),

                // Arcane Explosion — AoE
                seq(vec![
                    cond(|ctx| ctx.nearby.len() >= 3),
                    cast_on_current_target(ARCANE_EXPLOSION),
                ]),
            ]),
        ]),
    ])
}
