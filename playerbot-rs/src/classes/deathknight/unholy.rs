/// Unholy Death Knight behavior tree (WotLK only) — Disease DPS spec.
///
/// Priority: diseases → Scourge Strike → Death Coil → Bone Shield

pub fn build_tree() -> Box<dyn crate::engine::bt_nodes::BtNode> {
    #[cfg(not(feature = "wotlk"))]
    return crate::engine::bt_nodes::sel(vec![crate::engine::bt_nodes::cond(|_| false)]);

    #[cfg(feature = "wotlk")]
    {
        use crate::{
            data::spells::vanilla::deathknight::*,
            engine::bt_nodes::{BtResult, action, cast_on_current_target, cd_gate,
                                cond, gcd_gate, sel, seq},
        };
        // Scourge Strike: 55271 (rank 4), Bone Shield: 49222
        const SCOURGE_STRIKE: u32 = 55271;

        sel(vec![
            // Bone Shield — self buff
            seq(vec![
                cond(|ctx| !ctx.interface.has_aura(ctx.bot_handle, BONE_SHIELD)),
                gcd_gate(cd_gate(BONE_SHIELD, action(|ctx| {
                    let me = ctx.bot_handle;
                    if ctx.interface.cast_spell(BONE_SHIELD, me) {
                        ctx.timers.on_spell_cast(BONE_SHIELD, ctx.server_time_ms);
                        BtResult::Success
                    } else {
                        BtResult::Failure
                    }
                }))),
            ]),

            // Death Grip
            seq(vec![
                cond(|ctx| ctx.current_target().map_or(false, |t|
                    ctx.interface.unit_distance(t) > 15.0)),
                cast_on_current_target(DEATH_GRIP),
            ]),

            seq(vec![
                cond(|ctx| ctx.in_combat()),
                sel(vec![
                    // Apply diseases
                    seq(vec![
                        cond(|ctx| ctx.current_target().map_or(false, |t|
                            !ctx.interface.has_aura(t, 55095))),
                        cast_on_current_target(ICY_TOUCH),
                    ]),
                    seq(vec![
                        cond(|ctx| ctx.current_target().map_or(false, |t|
                            !ctx.interface.has_aura(t, 55078))),
                        cast_on_current_target(PLAGUE_STRIKE),
                    ]),

                    // Scourge Strike — main damage
                    cast_on_current_target(SCOURGE_STRIKE),

                    // Blood Boil — AoE spread diseases
                    seq(vec![
                        cond(|ctx| ctx.nearby.len() >= 2),
                        cast_on_current_target(BLOOD_BOIL),
                    ]),

                    // Death Coil — Runic Power dump
                    cast_on_current_target(DEATH_COIL),

                    // Death and Decay — AoE
                    seq(vec![
                        cond(|ctx| ctx.nearby.len() >= 2),
                        cast_on_current_target(DEATH_AND_DECAY),
                    ]),
                ]),
            ]),
        ])
    }
}
