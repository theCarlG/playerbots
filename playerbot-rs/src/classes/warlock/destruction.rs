/// Destruction Warlock behavior tree (Classic / Vanilla).
///
/// Priority: Corruption → Curse of Agony → Immolate → Conflagrate → Shadow Bolt
use crate::{
    data::spells::vanilla::warlock::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cast_on_current_target, cd_gate,
                    cond, gcd_gate, sel, seq},
        context::TickContext,
    },
    ffi::SpellId,
};

// Curse of Elements: 1490 (rank 1), 17937 (rank 4), 11722 — actually different IDs
const CURSE_OF_ELEMENTS: SpellId = SpellId(17937); // rank 4
const CURSE_OF_AGONY: SpellId = SpellId(11722);    // rank 7 (already in vanilla.rs)

pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Maintain Demon Armor
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

        // Life Tap when low mana (convert HP to mana)
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
                // Curse of Elements — fire damage amp
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        !ctx.interface.has_aura(t, CURSE_OF_ELEMENTS))),
                    gcd_gate(cd_gate(CURSE_OF_ELEMENTS, action(|ctx| {
                        let Some(t) = ctx.current_target() else { return BtResult::Failure };
                        if ctx.interface.cast_spell(CURSE_OF_ELEMENTS, t) {
                            ctx.timers.on_spell_cast(CURSE_OF_ELEMENTS, ctx.server_time_ms);
                            BtResult::Success
                        } else {
                            BtResult::Failure
                        }
                    }))),
                ]),

                // Immolate — fire DoT (required for Conflagrate)
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        !ctx.interface.has_aura(t, IMMOLATE))),
                    cast_on_current_target(IMMOLATE),
                ]),

                // Conflagrate (talent) — burst, consumes Immolate
                cast_on_current_target(CONFLAGRATE),

                // Corruption
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        !ctx.interface.has_aura(t, CORRUPTION))),
                    cast_on_current_target(CORRUPTION),
                ]),

                // Curse of Agony
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

                // Shadow Bolt — main nuke
                cast_on_current_target(SHADOW_BOLT),

                // Shadowburn (talent) — execute at <20%
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t| {
                        let s = ctx.interface.get_unit_snapshot(t);
                        s.max_health > 0 && (s.health as f32 / s.max_health as f32) < 0.20
                    })),
                    cast_on_current_target(SHADOWBURN),
                ]),
            ]),
        ]),
    ])
}
