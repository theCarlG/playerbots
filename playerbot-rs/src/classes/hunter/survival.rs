/// Survival Hunter behavior tree (Classic / Vanilla).
///
/// Survival focuses on traps, melee-range escapes, and counter-attack procs.
/// Priority: Feign Death (emergency) → Counterattack → Aimed Shot → Multi-Shot →
///   Arcane Shot → Serpent Sting → Explosive Trap
use crate::{
    data::spells::vanilla::hunter::*,
    engine::bt_nodes::{BtNode, BtResult, action, cast_on_current_target, cd_gate,
                        cond, gcd_gate, sel, seq},
    ffi::SpellId,
};

// Counterattack: 19306 rank 1, 20909 rank 2, 20910 rank 3
const COUNTERATTACK: SpellId = SpellId(20910);

pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Feign Death at critical HP
        seq(vec![
            cond(|ctx| ctx.self_hp_pct() < 0.15),
            gcd_gate(cd_gate(FEIGN_DEATH, action(|ctx| {
                let me = ctx.bot_handle;
                if ctx.interface.cast_spell(FEIGN_DEATH, me) {
                    ctx.timers.on_spell_cast(FEIGN_DEATH, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        seq(vec![
            cond(|ctx| ctx.in_combat()),
            sel(vec![
                // Hunter's Mark
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        !ctx.interface.has_aura(t, HUNTERS_MARK))),
                    cast_on_current_target(HUNTERS_MARK),
                ]),

                // Counterattack (proc after parry)
                cast_on_current_target(COUNTERATTACK),

                // Scatter Shot (interrupt)
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        ctx.interface.get_unit_snapshot(t).is_casting)),
                    cast_on_current_target(SCATTER_SHOT),
                ]),

                // Explosive Trap — place at feet when engaged
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        ctx.interface.unit_distance(t) < 5.0)),
                    cast_on_current_target(EXPLOSIVE_TRAP),
                ]),

                // Wing Clip if too close
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        ctx.interface.unit_distance(t) < 5.0)),
                    cast_on_current_target(WING_CLIP),
                ]),

                // Ranged rotation
                cast_on_current_target(AIMED_SHOT),
                cast_on_current_target(MULTI_SHOT),
                cast_on_current_target(ARCANE_SHOT),
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        !ctx.interface.has_aura(t, SERPENT_STING))),
                    cast_on_current_target(SERPENT_STING),
                ]),
            ]),
        ]),
    ])
}
