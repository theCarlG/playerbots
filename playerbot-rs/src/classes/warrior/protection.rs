/// Protection Warrior behavior tree (Classic / Vanilla).
///
/// Tank priority: survival CDs → taunt on aggro loss → Shield Block → Revenge →
///   Shield Slam → Sunder Armor → Heroic Strike → Demoralizing Shout
use crate::{
    combat::targeting::self_buff_action,
    data::spells::vanilla::warrior::*,
    engine::bt_nodes::{BtNode, BtResult, action, cast_on_current_target, cd_gate,
                        cond, gcd_gate, sel, seq},
    engine::context::TickContext,
    ffi::SpellId,
};

pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Emergency survival CDs
        seq(vec![
            cond(|ctx| ctx.self_hp_pct() < 0.20),
            sel(vec![
                cast_on_current_target(SHIELD_WALL),
                cast_on_current_target(LAST_STAND),
            ]),
        ]),

        // Close gap
        cast_on_current_target(CHARGE),

        seq(vec![
            cond(|ctx| ctx.in_combat()),
            sel(vec![
                // Taunt if we lost aggro (interface can_cast returns true only when
                // the server's aggro-loss condition is met for taunt procs)
                cast_on_current_target(TAUNT),

                // Shield Block — mitigation
                self_buff_action(SHIELD_BLOCK, |ctx| ctx.in_combat()),

                // Bloodrage — rage generation
                self_buff_action(BLOODRAGE, |ctx| {
                    ctx.in_combat() && ctx.self_hp_pct() > 0.30
                }),

                // Revenge — high priority proc
                cast_on_current_target(REVENGE),

                // Shield Slam — high damage (requires talent)
                cast_on_current_target(SHIELD_SLAM),

                // Sunder Armor — debuff stacks
                cast_on_current_target(SUNDER_ARMOR),

                // Heroic Strike — rage dump
                cast_on_current_target(HEROIC_STRIKE),

                // Demoralizing Shout — melee debuff
                seq(vec![
                    cond(target_needs_demo_shout),
                    cast_on_current_target(DEMORALIZING_SHOUT),
                ]),
            ]),
        ]),
    ])
}

fn target_needs_demo_shout(ctx: &TickContext<'_>) -> bool {
    let Some(target) = ctx.current_target() else { return false };
    use crate::engine::aura_helpers::{has_any_rank, DEMORALIZING_SHOUT_RANKS};
    !has_any_rank(ctx.interface, target, DEMORALIZING_SHOUT_RANKS)
}
