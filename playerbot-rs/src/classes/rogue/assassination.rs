/// Assassination Rogue behavior tree (Classic / Vanilla).
///
/// Focuses on poison procs and Backstab (requires behind target).
/// Priority: Vanish (emergency) → Backstab / Ambush → Eviscerate → Slice and Dice →
///   Garrote (opener) → Sinister Strike (builder)
use crate::{
    data::spells::vanilla::rogue::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cast_on_current_target, cd_gate,
                    cond, gcd_gate, sel, seq},
        context::TickContext,
    },
};

pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Vanish emergency
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

        seq(vec![
            cond(|ctx| ctx.in_combat()),
            sel(vec![
                // Kick — interrupt
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        ctx.interface.get_unit_snapshot(t).is_casting)),
                    cast_on_current_target(KICK),
                ]),

                // Slice and Dice
                seq(vec![
                    cond(|ctx| !ctx.interface.has_aura(ctx.bot_handle, SLICE_AND_DICE)),
                    cast_on_current_target(SLICE_AND_DICE),
                ]),

                // Backstab — high damage if behind target (can_cast handles position)
                cast_on_current_target(BACKSTAB),

                // Hemorrhage (talent) — bleed
                cast_on_current_target(HEMORRHAGE),

                // Eviscerate
                cast_on_current_target(EVISCERATE),

                // Rupture DoT
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        !ctx.interface.has_aura(t, RUPTURE))),
                    cast_on_current_target(RUPTURE),
                ]),

                // Sinister Strike — builder
                cast_on_current_target(SINISTER_STRIKE),
            ]),
        ]),
    ])
}
