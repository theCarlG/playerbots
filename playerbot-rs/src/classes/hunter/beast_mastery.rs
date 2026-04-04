/// Beast Mastery Hunter behavior tree (Classic / Vanilla).
///
/// BM hunter relies heavily on pet damage. The bot manages:
/// Pet attack → Bestial Wrath → Hunter's Mark → ranged shots
use crate::{
    data::spells::vanilla::hunter::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cast_on_current_target, cd_gate,
                    cond, gcd_gate, sel, seq},
        context::TickContext,
    },
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

        // Aspect of the Hawk
        seq(vec![
            cond(needs_hawk),
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
                // Hunter's Mark
                seq(vec![
                    cond(|ctx| {
                        ctx.current_target().map_or(false, |t|
                            !ctx.interface.has_aura(t, HUNTERS_MARK))
                    }),
                    cast_on_current_target(HUNTERS_MARK),
                ]),

                // Bestial Wrath (BM talent) — burst
                cast_on_current_target(BESTIAL_WRATH),

                // Aimed Shot
                cast_on_current_target(AIMED_SHOT),

                // Multi-Shot
                cast_on_current_target(MULTI_SHOT),

                // Arcane Shot
                cast_on_current_target(ARCANE_SHOT),

                // Serpent Sting
                seq(vec![
                    cond(|ctx| {
                        ctx.current_target().map_or(false, |t|
                            !ctx.interface.has_aura(t, SERPENT_STING))
                    }),
                    cast_on_current_target(SERPENT_STING),
                ]),

                // Raptor Strike (melee)
                cast_on_current_target(RAPTOR_STRIKE),
            ]),
        ]),
    ])
}

fn needs_hawk(ctx: &TickContext<'_>) -> bool {
    !ctx.interface.has_aura(ctx.bot_handle, ASPECT_OF_THE_HAWK)
}
