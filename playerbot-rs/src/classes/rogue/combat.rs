/// Combat Rogue behavior tree (Classic / Vanilla).
///
/// Priority: Slice and Dice (maintain) → Eviscerate (5 CP) → Rupture (DoT) →
///   Sinister Strike (builder) → Blade Flurry (AoE burst)
use crate::{
    data::spells::vanilla::rogue::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cast_on_current_target, cd_gate,
                    cond, gcd_gate, sel, seq},
        context::TickContext,
    },
    ffi::SpellId,
};

// Riposte: 14251 (requires parry proc)
const RIPOSTE: SpellId = SpellId(14251);

pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Vanish at critical HP
        seq(vec![
            cond(|ctx| ctx.self_hp_pct() < 0.15),
            gcd_gate(cd_gate(VANISH, action(|ctx| {
                let me = ctx.bot_handle;
                if ctx.interface.cast_spell(VANISH, me) {
                    ctx.timers.on_spell_cast(VANISH, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        // Evasion when taking heavy damage
        seq(vec![
            cond(|ctx| ctx.self_hp_pct() < 0.30 && !ctx.attackers.is_empty()),
            gcd_gate(cd_gate(EVASION, action(|ctx| {
                let me = ctx.bot_handle;
                if ctx.interface.cast_spell(EVASION, me) {
                    ctx.timers.on_spell_cast(EVASION, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        seq(vec![
            cond(|ctx| ctx.in_combat()),
            sel(vec![
                // Kidney Shot — CC for interrupt/stun
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        ctx.interface.get_unit_snapshot(t).is_casting)),
                    cast_on_current_target(KICK),
                ]),

                // Riposte — proc after parry
                cast_on_current_target(RIPOSTE),

                // Blade Flurry — AoE
                seq(vec![
                    cond(|ctx| ctx.nearby.len() >= 2),
                    gcd_gate(cd_gate(BLADE_FLURRY, action(|ctx| {
                        let me = ctx.bot_handle;
                        if ctx.interface.cast_spell(BLADE_FLURRY, me) {
                            ctx.timers.on_spell_cast(BLADE_FLURRY, ctx.server_time_ms);
                            BtResult::Success
                        } else {
                            BtResult::Failure
                        }
                    }))),
                ]),

                // Slice and Dice — must maintain
                seq(vec![
                    cond(needs_slice_and_dice),
                    cast_on_current_target(SLICE_AND_DICE),
                ]),

                // Eviscerate — finisher when SD is up
                seq(vec![
                    cond(|ctx| ctx.interface.has_aura(ctx.bot_handle, SLICE_AND_DICE)),
                    cast_on_current_target(EVISCERATE),
                ]),

                // Rupture — DoT finisher
                seq(vec![
                    cond(target_needs_rupture),
                    cast_on_current_target(RUPTURE),
                ]),

                // Sinister Strike — builder
                cast_on_current_target(SINISTER_STRIKE),
            ]),
        ]),
    ])
}

fn needs_slice_and_dice(ctx: &TickContext<'_>) -> bool {
    !ctx.interface.has_aura(ctx.bot_handle, SLICE_AND_DICE)
}

fn target_needs_rupture(ctx: &TickContext<'_>) -> bool {
    let Some(t) = ctx.current_target() else { return false };
    use crate::engine::aura_helpers::{has_any_rank, RUPTURE_RANKS};
    !has_any_rank(ctx.interface, t, RUPTURE_RANKS)
}
