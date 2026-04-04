/// Frost Death Knight behavior tree (WotLK only).
///
/// Two-handed Frost: Howling Blast → Obliterate → Frost Strike → Icy Touch → Plague Strike
#[cfg(feature = "wotlk")]
use crate::{
    data::spells::vanilla::deathknight::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cast_on_current_target, cd_gate,
                    cond, gcd_gate, sel, seq},
        context::TickContext,
    },
};

#[cfg(feature = "wotlk")]
pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Death Grip — pull enemy
        seq(vec![
            cond(|ctx| ctx.current_target().map_or(false, |t|
                ctx.interface.unit_distance(t) > 15.0)),
            cast_on_current_target(DEATH_GRIP),
        ]),

        seq(vec![
            cond(|ctx| ctx.in_combat()),
            sel(vec![
                // Anti-Magic Shell — absorb magic damage
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        ctx.interface.get_unit_snapshot(t).is_casting)),
                    gcd_gate(cd_gate(ANTI_MAGIC_SHELL, action(|ctx| {
                        let me = ctx.bot_handle;
                        if ctx.interface.cast_spell(ANTI_MAGIC_SHELL, me) {
                            ctx.timers.on_spell_cast(ANTI_MAGIC_SHELL, ctx.server_time_ms);
                            BtResult::Success
                        } else {
                            BtResult::Failure
                        }
                    }))),
                ]),

                // Howling Blast (burst + AoE)
                cast_on_current_target(HOWLING_BLAST),

                // Obliterate — main melee
                cast_on_current_target(OBLITERATE),

                // Icy Touch — apply Frost Fever (required for Obliterate buff)
                seq(vec![
                    cond(target_needs_frost_fever),
                    cast_on_current_target(ICY_TOUCH),
                ]),

                // Plague Strike — apply Blood Plague
                seq(vec![
                    cond(target_needs_blood_plague),
                    cast_on_current_target(PLAGUE_STRIKE),
                ]),

                // Frost Strike — Runic Power dump
                cast_on_current_target(FROST_STRIKE),

                // Chains of Ice — snare
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        ctx.interface.unit_distance(t) > 8.0)),
                    cast_on_current_target(CHAINS_OF_ICE),
                ]),
            ]),
        ]),
    ])
}

#[cfg(feature = "wotlk")]
fn target_needs_frost_fever(ctx: &TickContext<'_>) -> bool {
    // Frost Fever aura: 55095
    ctx.current_target().map_or(false, |t| !ctx.interface.has_aura(t, 55095))
}

#[cfg(feature = "wotlk")]
fn target_needs_blood_plague(ctx: &TickContext<'_>) -> bool {
    // Blood Plague aura: 55078
    ctx.current_target().map_or(false, |t| !ctx.interface.has_aura(t, 55078))
}

#[cfg(not(feature = "wotlk"))]
pub fn build_tree() -> Box<dyn crate::engine::bt_nodes::BtNode> {
    crate::engine::bt_nodes::sel(vec![crate::engine::bt_nodes::cond(|_| false)])
}
