/// Blood Death Knight behavior tree (WotLK only) — Tank/DPS spec.
///
/// Self-sustaining through Death Strike heals.
/// Priority: Death Grip → Heart Strike → Death Strike → Blood Strike → Death Coil

pub fn build_tree() -> Box<dyn crate::engine::bt_nodes::BtNode> {
    use crate::engine::bt_nodes::{BtResult, action, cast_on_current_target, cd_gate,
                                    cond, gcd_gate, sel, seq};
    use crate::ffi::SpellId;

    #[cfg(feature = "wotlk")]
    use crate::data::spells::vanilla::deathknight::*;

    #[cfg(not(feature = "wotlk"))]
    return sel(vec![cond(|_| false)]);

    #[cfg(feature = "wotlk")]
    sel(vec![
        // Death Grip — pull
        seq(vec![
            cond(|ctx| ctx.current_target().map_or(false, |t|
                ctx.interface.unit_distance(t) > 15.0)),
            cast_on_current_target(DEATH_GRIP),
        ]),

        seq(vec![
            cond(|ctx| ctx.in_combat()),
            sel(vec![
                // Dancing Rune Weapon — boost threat
                gcd_gate(cd_gate(DANCING_RUNE_WEAPON, action(|ctx| {
                    let me = ctx.bot_handle;
                    if ctx.interface.cast_spell(DANCING_RUNE_WEAPON, me) {
                        ctx.timers.on_spell_cast(DANCING_RUNE_WEAPON, ctx.server_time_ms);
                        BtResult::Success
                    } else {
                        BtResult::Failure
                    }
                }))),

                // Dark Command — taunt
                cast_on_current_target(DARK_COMMAND),

                // Icy Touch + Plague Strike — diseases
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        !ctx.interface.has_aura(t, SpellId(55095)))), // Frost Fever
                    cast_on_current_target(ICY_TOUCH),
                ]),
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        !ctx.interface.has_aura(t, SpellId(55078)))), // Blood Plague
                    cast_on_current_target(PLAGUE_STRIKE),
                ]),

                // Death Strike — heal + damage
                seq(vec![
                    cond(|ctx| ctx.self_hp_pct() < 0.70),
                    cast_on_current_target(DEATH_STRIKE),
                ]),

                // Heart Strike — main melee
                cast_on_current_target(HEART_STRIKE),

                // Blood Strike
                cast_on_current_target(BLOOD_STRIKE),

                // Death Coil — Runic Power
                cast_on_current_target(DEATH_COIL),

                // Blood Boil — AoE
                seq(vec![
                    cond(|ctx| ctx.nearby.len() >= 2),
                    cast_on_current_target(BLOOD_BOIL),
                ]),

                // Death and Decay — AoE ground
                seq(vec![
                    cond(|ctx| ctx.nearby.len() >= 2),
                    cast_on_current_target(DEATH_AND_DECAY),
                ]),
            ]),
        ]),
    ])
}
