/// Shadow Priest behavior tree (Classic / Vanilla).
///
/// Priority: Shadowform → Vampiric Embrace → Shadow Word: Pain → Mind Blast →
///   Mind Flay (channel) → Devouring Plague (Undead racial) → wand if OOM
use crate::{
    combat::targeting::self_buff_action,
    data::spells::vanilla::priest::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cast_on_current_target, cd_gate,
                    cond, gcd_gate, sel, seq},
        context::TickContext,
    },
};

pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Shadowform — maintain always
        seq(vec![
            cond(|ctx| !ctx.interface.has_aura(ctx.bot_handle, SHADOWFORM)),
            self_buff_action(SHADOWFORM, |ctx| !ctx.interface.has_aura(ctx.bot_handle, SHADOWFORM)),
        ]),

        // Fade when taking melee damage
        seq(vec![
            cond(|ctx| !ctx.attackers.is_empty()),
            self_buff_action(FADE, |ctx| !ctx.attackers.is_empty()),
        ]),

        // Self-heal if low HP (drop shadow form)
        seq(vec![
            cond(|ctx| ctx.self_hp_pct() < 0.30),
            gcd_gate(cd_gate(FLASH_HEAL, action(|ctx| {
                let me = ctx.bot_handle;
                if me == 0 { return BtResult::Failure; }
                if ctx.interface.cast_spell(FLASH_HEAL, me) {
                    ctx.timers.on_spell_cast(FLASH_HEAL, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),

        seq(vec![
            cond(|ctx| ctx.in_combat()),
            sel(vec![
                // Vampiric Embrace — passive drain, maintain
                seq(vec![
                    cond(|ctx| {
                        ctx.current_target().map_or(false, |t|
                            !ctx.interface.has_aura(t, VAMPIRIC_EMBRACE))
                    }),
                    cast_on_current_target(VAMPIRIC_EMBRACE),
                ]),

                // Shadow Word: Pain — DoT, maintain
                seq(vec![
                    cond(target_needs_sw_pain),
                    cast_on_current_target(SHADOW_WORD_PAIN),
                ]),

                // Mind Blast — nuke on CD
                cast_on_current_target(MIND_BLAST),

                // Devouring Plague (Undead racial) — powerful DoT
                seq(vec![
                    cond(target_needs_devouring_plague),
                    cast_on_current_target(DEVOURING_PLAGUE),
                ]),

                // Mind Flay — channeled, filler
                cast_on_current_target(MIND_FLAY),

                // Psychic Scream — if multiple melee enemies close
                seq(vec![
                    cond(|ctx| ctx.attackers.len() >= 2),
                    cast_on_current_target(PSYCHIC_SCREAM),
                ]),
            ]),
        ]),
    ])
}

fn target_needs_sw_pain(ctx: &TickContext<'_>) -> bool {
    let Some(t) = ctx.current_target() else { return false };
    !ctx.interface.has_aura(t, SHADOW_WORD_PAIN)
        && !ctx.interface.has_aura(t, 10894) // check all ranks
}

fn target_needs_devouring_plague(ctx: &TickContext<'_>) -> bool {
    let Some(t) = ctx.current_target() else { return false };
    !ctx.interface.has_aura(t, DEVOURING_PLAGUE)
        && !ctx.interface.has_aura(t, 19276)
}
