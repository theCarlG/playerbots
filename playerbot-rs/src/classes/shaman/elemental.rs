/// Elemental Shaman behavior tree (Classic / Vanilla).
///
/// Priority: Flame Shock → Lightning Bolt → Chain Lightning (AoE) → Earth Shock (interrupt)
use crate::{
    data::spells::vanilla::shaman::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cast_on_current_target, cd_gate,
                    cond, gcd_gate, sel, seq},
        context::TickContext,
    },
};

pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Drop totems
        seq(vec![
            cond(|ctx| !ctx.interface.has_aura(ctx.bot_handle, GRACE_OF_AIR_TOTEM)
                && ctx.snap.group_size > 1),
            gcd_gate(cd_gate(GRACE_OF_AIR_TOTEM, action(|ctx| {
                let me = ctx.bot_handle;
                if ctx.interface.cast_spell(GRACE_OF_AIR_TOTEM, me) {
                    ctx.timers.on_spell_cast(GRACE_OF_AIR_TOTEM, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        // Mana Spring Totem
        seq(vec![
            cond(|ctx| !ctx.interface.has_aura(ctx.bot_handle, MANA_SPRING_TOTEM)),
            gcd_gate(cd_gate(MANA_SPRING_TOTEM, action(|ctx| {
                let me = ctx.bot_handle;
                if ctx.interface.cast_spell(MANA_SPRING_TOTEM, me) {
                    ctx.timers.on_spell_cast(MANA_SPRING_TOTEM, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        seq(vec![
            cond(|ctx| ctx.in_combat()),
            sel(vec![
                // Earth Shock — interrupt
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        ctx.interface.get_unit_snapshot(t).is_casting)),
                    cast_on_current_target(EARTH_SHOCK),
                ]),

                // Flame Shock — maintain DoT
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        !ctx.interface.has_aura(t, FLAME_SHOCK))),
                    cast_on_current_target(FLAME_SHOCK),
                ]),

                // Chain Lightning — AoE
                seq(vec![
                    cond(|ctx| ctx.nearby.len() >= 3),
                    cast_on_current_target(CHAIN_LIGHTNING),
                ]),

                // Lightning Bolt — main nuke
                cast_on_current_target(LIGHTNING_BOLT),

                // Frost Shock — filler / snare
                cast_on_current_target(FROST_SHOCK),
            ]),
        ]),
    ])
}
