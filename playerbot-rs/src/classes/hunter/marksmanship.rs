/// Marksmanship Hunter behavior tree (Classic / Vanilla).
///
/// Priority: Hunter's Mark → Aimed Shot → Multi-Shot → Arcane Shot →
///   Serpent Sting → Auto Shot (handled by attack mode)
/// Close range: Raptor Strike → Wing Clip → Disengage
use crate::{
    data::spells::vanilla::hunter::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cast_on_current_target, cd_gate,
                    cond, gcd_gate, sel, seq},
        context::TickContext,
    },
    ffi::SpellId,
};

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

        // Aspect of the Hawk — maintain
        seq(vec![
            cond(needs_hawk_aspect),
            gcd_gate(cd_gate(ASPECT_OF_THE_HAWK, action(|ctx| {
                let me = ctx.bot_handle;
                if ctx.interface.cast_spell(ASPECT_OF_THE_HAWK, me) {
                    ctx.timers.on_spell_cast(ASPECT_OF_THE_HAWK, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        seq(vec![
            cond(|ctx| ctx.in_combat()),
            sel(vec![
                // Hunter's Mark — debuff, maintain
                seq(vec![
                    cond(target_needs_hunters_mark),
                    cast_on_current_target(HUNTERS_MARK),
                ]),

                // If enemy is in melee range: disengage, wing clip, raptor strike
                seq(vec![
                    cond(enemy_is_close),
                    sel(vec![
                        cast_on_current_target(WING_CLIP),
                        cast_on_current_target(RAPTOR_STRIKE),
                    ]),
                ]),

                // Scatter Shot — interrupt/CC if enemy casts
                seq(vec![
                    cond(target_is_casting),
                    cast_on_current_target(SCATTER_SHOT),
                ]),

                // Aimed Shot — high damage, 3.5s cast
                cast_on_current_target(AIMED_SHOT),

                // Multi-Shot — AoE/burst
                cast_on_current_target(MULTI_SHOT),

                // Arcane Shot — instant, mana costly
                cast_on_current_target(ARCANE_SHOT),

                // Serpent Sting — maintain DoT
                seq(vec![
                    cond(target_needs_serpent_sting),
                    cast_on_current_target(SERPENT_STING),
                ]),
            ]),
        ]),
    ])
}

fn needs_hawk_aspect(ctx: &TickContext<'_>) -> bool {
    let me = ctx.bot_handle;
    !ctx.interface.has_aura(me, ASPECT_OF_THE_HAWK)
        && !ctx.interface.has_aura(me, SpellId(25296))
}

fn target_needs_hunters_mark(ctx: &TickContext<'_>) -> bool {
    let Some(t) = ctx.current_target() else { return false };
    use crate::engine::aura_helpers::{has_any_rank, HUNTERS_MARK_RANKS};
    !has_any_rank(ctx.interface, t, HUNTERS_MARK_RANKS)
}

fn enemy_is_close(ctx: &TickContext<'_>) -> bool {
    let Some(t) = ctx.current_target() else { return false };
    ctx.interface.unit_distance(t) < 5.0
}

fn target_is_casting(ctx: &TickContext<'_>) -> bool {
    let Some(t) = ctx.current_target() else { return false };
    let snap = ctx.interface.get_unit_snapshot(t);
    snap.is_casting
}

fn target_needs_serpent_sting(ctx: &TickContext<'_>) -> bool {
    let Some(t) = ctx.current_target() else { return false };
    use crate::engine::aura_helpers::{has_any_rank, SERPENT_STING_RANKS};
    !has_any_rank(ctx.interface, t, SERPENT_STING_RANKS)
}
