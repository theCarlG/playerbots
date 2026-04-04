/// Enhancement Shaman behavior tree (Classic / Vanilla).
///
/// Priority: totems down → Stormstrike (talent) → Earth Shock → maintain Lightning Shield
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
        // Maintain Lightning Shield
        seq(vec![
            cond(|ctx| !ctx.interface.has_aura(ctx.bot_handle, LIGHTNING_SHIELD)
                && !ctx.interface.has_aura(ctx.bot_handle, 10432)),
            gcd_gate(cd_gate(LIGHTNING_SHIELD, action(|ctx| {
                let me = ctx.bot_handle;
                if ctx.interface.cast_spell(LIGHTNING_SHIELD, me) {
                    ctx.timers.on_spell_cast(LIGHTNING_SHIELD, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        // Drop Windfury Totem
        seq(vec![
            cond(|ctx| !ctx.interface.has_aura(ctx.bot_handle, WINDFURY_TOTEM)
                && !ctx.interface.has_aura(ctx.bot_handle, 25587)),
            gcd_gate(cd_gate(WINDFURY_TOTEM, action(|ctx| {
                let me = ctx.bot_handle;
                if ctx.interface.cast_spell(WINDFURY_TOTEM, me) {
                    ctx.timers.on_spell_cast(WINDFURY_TOTEM, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        // Strength of Earth Totem
        seq(vec![
            cond(|ctx| !ctx.interface.has_aura(ctx.bot_handle, STRENGTH_OF_EARTH_TOTEM)),
            gcd_gate(cd_gate(STRENGTH_OF_EARTH_TOTEM, action(|ctx| {
                let me = ctx.bot_handle;
                if ctx.interface.cast_spell(STRENGTH_OF_EARTH_TOTEM, me) {
                    ctx.timers.on_spell_cast(STRENGTH_OF_EARTH_TOTEM, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        seq(vec![
            cond(|ctx| ctx.in_combat()),
            sel(vec![
                // Nature's Swiftness for instant heal when critical
                seq(vec![
                    cond(|ctx| ctx.self_hp_pct() < 0.25),
                    gcd_gate(cd_gate(NATURE_SWIFTNESS, action(|ctx| {
                        let me = ctx.bot_handle;
                        if ctx.interface.cast_spell(NATURE_SWIFTNESS, me) {
                            ctx.timers.on_spell_cast(NATURE_SWIFTNESS, ctx.server_time_ms);
                            BtResult::Success
                        } else {
                            BtResult::Failure
                        }
                    }))),
                ]),

                // Self-heal via Lesser Healing Wave when low HP
                seq(vec![
                    cond(|ctx| ctx.self_hp_pct() < 0.35),
                    gcd_gate(cd_gate(LESSER_HEALING_WAVE, action(|ctx| {
                        let me = ctx.bot_handle;
                        if ctx.interface.cast_spell(LESSER_HEALING_WAVE, me) {
                            ctx.timers.on_spell_cast(LESSER_HEALING_WAVE, ctx.server_time_ms);
                            BtResult::Success
                        } else {
                            BtResult::Failure
                        }
                    }))),
                ]),

                // Stormstrike — talent, high melee damage
                cast_on_current_target(STORMSTRIKE),

                // Earth Shock — interrupt + instant damage
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        ctx.interface.get_unit_snapshot(t).is_casting)),
                    cast_on_current_target(EARTH_SHOCK),
                ]),

                // Flame Shock — fire DoT
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        !ctx.interface.has_aura(t, FLAME_SHOCK))),
                    cast_on_current_target(FLAME_SHOCK),
                ]),

                // Earth Shock — filler
                cast_on_current_target(EARTH_SHOCK),
            ]),
        ]),
    ])
}
