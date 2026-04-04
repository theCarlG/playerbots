/// Fury Warrior behavior tree (Classic / Vanilla).
///
/// Priority: execute → bloodthirst → whirlwind → cleave (AoE) → heroic strike
use crate::{
    data::spells::vanilla::warrior::*,
    engine::bt_nodes::{BtNode, cast_on_current_target, cond, sel, seq},
};

pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // Emergency: Intimidating Shout when very low HP
        seq(vec![
            cond(|ctx| ctx.self_hp_pct() < 0.15),
            cast_on_current_target(INTIMIDATING_SHOUT),
        ]),

        // Close gap via Intercept (Berserker Stance) or Charge
        cast_on_current_target(INTERCEPT),
        cast_on_current_target(CHARGE),

        // Combat priority
        seq(vec![
            cond(|ctx| ctx.in_combat()),
            sel(vec![
                // Execute at <20% target HP
                seq(vec![
                    cond(target_below_20pct),
                    cast_on_current_target(EXECUTE),
                ]),
                // Bloodthirst — main damage source (talent)
                cast_on_current_target(BLOODTHIRST),
                // Whirlwind
                cast_on_current_target(WHIRLWIND),
                // Heroic Strike — rage dump
                cast_on_current_target(HEROIC_STRIKE),
                // Cleave — if multiple targets (same slot as heroic strike)
                cast_on_current_target(CLEAVE),
            ]),
        ]),
    ])
}

fn target_below_20pct(ctx: &crate::engine::context::TickContext<'_>) -> bool {
    let Some(target) = ctx.current_target() else { return false };
    let snap = ctx.interface.get_unit_snapshot(target);
    snap.max_health > 0 && (snap.health as f32 / snap.max_health as f32) < 0.20
}
