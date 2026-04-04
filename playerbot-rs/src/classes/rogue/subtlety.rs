/// Subtlety Rogue behavior tree (Classic / Vanilla).
///
/// Stealth opener, Premeditation, Ambush, then Eviscerate finisher.
/// Priority: opener from stealth → Eviscerate → Sinister Strike
use crate::{
    data::spells::vanilla::rogue::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cast_on_current_target, cd_gate,
                    cond, gcd_gate, sel, seq},
        context::TickContext,
    },
};

// Premeditation: 14183 (rank 1), 14185 (rank 2) — adds combo points
const PREMEDITATION: u32 = 14185;

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
                // Kick interrupt
                seq(vec![
                    cond(|ctx| ctx.current_target().map_or(false, |t|
                        ctx.interface.get_unit_snapshot(t).is_casting)),
                    cast_on_current_target(KICK),
                ]),

                // Gouge — stun
                seq(vec![
                    cond(|ctx| ctx.self_hp_pct() < 0.40),
                    cast_on_current_target(GOUGE),
                ]),

                // Slice and Dice
                seq(vec![
                    cond(|ctx| !ctx.interface.has_aura(ctx.bot_handle, SLICE_AND_DICE)),
                    cast_on_current_target(SLICE_AND_DICE),
                ]),

                // Ambush from stealth
                cast_on_current_target(AMBUSH),

                // Hemorrhage (Subtlety talent) — bleed
                cast_on_current_target(HEMORRHAGE),

                // Eviscerate
                cast_on_current_target(EVISCERATE),

                // Sinister Strike — builder
                cast_on_current_target(SINISTER_STRIKE),
            ]),
        ]),
    ])
}
