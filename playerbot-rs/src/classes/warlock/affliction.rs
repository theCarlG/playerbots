/// Affliction Warlock behavior tree (Classic / Vanilla).
///
/// DoT stacking and drain sustain.
/// Priority: Corruption → Curse of Agony → Immolate → Shadow Word: Pain DoT →
///   Drain Life (self sustain) → Shadow Bolt (nuke)
use crate::{
    data::spells::vanilla::warlock::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cast_on_current_target, cd_gate,
                    cond, gcd_gate, sel, seq},
        context::TickContext,
    },
};

const CURSE_OF_AGONY: u32 = 11722;

pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Demon Armor
        seq(vec![
            cond(|ctx| !ctx.interface.has_aura(ctx.bot_handle, DEMON_ARMOR)),
            gcd_gate(cd_gate(DEMON_ARMOR, action(|ctx| {
                let me = ctx.bot_handle;
                if ctx.interface.cast_spell(DEMON_ARMOR, me) {
                    ctx.timers.on_spell_cast(DEMON_ARMOR, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        // Life Tap mana restore
        seq(vec![
            cond(|ctx| ctx.self_mana_pct() < 0.20 && ctx.self_hp_pct() > 0.50),
            gcd_gate(cd_gate(LIFE_TAP, action(|ctx| {
                let me = ctx.bot_handle;
                if ctx.interface.cast_spell(LIFE_TAP, me) {
                    ctx.timers.on_spell_cast(LIFE_TAP, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        seq(vec![
            cond(|ctx| ctx.in_combat()),
            sel(vec![
                // Curse of Agony — maintain
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        !ctx.interface.has_aura(t, CURSE_OF_AGONY))),
                    gcd_gate(cd_gate(CURSE_OF_AGONY, action(|ctx| {
                        let Some(t) = ctx.current_target() else { return BtResult::Failure };
                        if ctx.interface.cast_spell(CURSE_OF_AGONY, t) {
                            ctx.timers.on_spell_cast(CURSE_OF_AGONY, ctx.server_time_ms);
                            BtResult::Success
                        } else {
                            BtResult::Failure
                        }
                    }))),
                ]),

                // Corruption — maintain
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        !ctx.interface.has_aura(t, CORRUPTION))),
                    cast_on_current_target(CORRUPTION),
                ]),

                // Immolate — fire DoT
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        !ctx.interface.has_aura(t, IMMOLATE))),
                    cast_on_current_target(IMMOLATE),
                ]),

                // Drain Life — self heal + damage
                seq(vec![
                    cond(|ctx| ctx.self_hp_pct() < 0.60),
                    cast_on_current_target(DRAIN_LIFE),
                ]),

                // Shadow Bolt — nuke filler
                cast_on_current_target(SHADOW_BOLT),
            ]),
        ]),
    ])
}
