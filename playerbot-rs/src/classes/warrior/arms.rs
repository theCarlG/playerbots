/// Arms Warrior behavior tree.
///
/// Combat priority (Classic / Vanilla):
///   1. Intimidating Shout  — emergency, self HP < 15%
///   2. Execute             — target HP < 20% (rage dump)
///   3. Overpower           — available proc (target dodged, very high priority)
///   4. Mortal Strike       — 6s CD, core arms damage
///   5. Whirlwind           — 10s CD (talent required)
///   6. Heroic Strike       — filler, uses queued rage on next swing
///   7. Rend                — maintain bleed if not on target
///   8. Battle Shout        — keep buff up (non-combat + combat rebuff)
///
/// Conditions that use interface calls (e.g. get_unit_snapshot, has_aura) are
/// explicitly opt-in and are skipped on minimal ticks where applicable.
use crate::{
    data::spells::vanilla::warrior::*,
    engine::{
        bt_nodes::{BtNode, BtResult, action, cast_on_current_target, cd_gate, cond, gcd_gate, not, sel, seq},
        context::TickContext,
    },
};

/// Returns the Arms Warrior root tree.
pub fn build_tree() -> Box<dyn BtNode> {
    sel(vec![
        // ── Safety: charge to target if out of melee range ──────────────
        // charge() handles its own range check via can_cast
        cast_on_current_target(CHARGE),

        // ── Combat selector ─────────────────────────────────────────────
        seq(vec![
            cond(|ctx| ctx.in_combat()),
            combat_selector(),
        ]),

        // ── Non-combat: maintain Battle Shout buff ───────────────────────
        seq(vec![
            cond(|ctx| !ctx.in_combat()),
            noncombat_selector(),
        ]),
    ])
}

/// All in-combat decisions for Arms Warrior.
fn combat_selector() -> Box<dyn BtNode> {
    sel(vec![
        // 1. Emergency: Intimidating Shout when very low HP
        seq(vec![
            cond(|ctx| ctx.self_hp_pct() < 0.15),
            cast_on_current_target(INTIMIDATING_SHOUT),
        ]),

        // 2. Execute at target HP < 20%
        seq(vec![
            cond(target_hp_below_20pct),
            cast_on_current_target(EXECUTE),
        ]),

        // 3. Overpower — only available after target dodges (server tracks the proc)
        cast_on_current_target(OVERPOWER),

        // 4. Mortal Strike — core damage, 6s CD
        cast_on_current_target(MORTAL_STRIKE),

        // 5. Whirlwind — 10s CD (requires talent)
        cast_on_current_target(WHIRLWIND),

        // 6. Heroic Strike — rage dump / filler
        cast_on_current_target(HEROIC_STRIKE),

        // 7. Rend — maintain bleed on target if not already present
        seq(vec![
            cond(target_needs_rend),
            cast_on_current_target(REND),
        ]),
    ])
}

fn noncombat_selector() -> Box<dyn BtNode> {
    sel(vec![
        // Rebuff Battle Shout if missing
        seq(vec![
            cond(needs_battle_shout),
            gcd_gate(cd_gate(BATTLE_SHOUT, action(|ctx| {
                let me = ctx.bot_handle;
                if me != 0 && ctx.interface.cast_spell(BATTLE_SHOUT, me) {
                    ctx.timers.on_spell_cast(BATTLE_SHOUT, ctx.server_time_ms);
                    BtResult::Success
                } else {
                    BtResult::Failure
                }
            }))),
        ]),
    ])
}

// ── Condition helpers ─────────────────────────────────────────────────────

fn target_hp_below_20pct(ctx: &TickContext<'_>) -> bool {
    let Some(target) = ctx.current_target() else { return false };
    let snap = ctx.interface.get_unit_snapshot(target);
    snap.max_health > 0 && (snap.health as f32 / snap.max_health as f32) < 0.20
}

fn target_needs_rend(ctx: &TickContext<'_>) -> bool {
    let Some(target) = ctx.current_target() else { return false };
    use crate::engine::aura_helpers::{has_any_rank, REND_RANKS};
    !has_any_rank(ctx.interface, target, REND_RANKS)
}

fn needs_battle_shout(ctx: &TickContext<'_>) -> bool {
    let me = ctx.bot_handle;
    if me == 0 { return false; }
    use crate::engine::aura_helpers::{has_any_rank, BATTLE_SHOUT_RANKS};
    !has_any_rank(ctx.interface, me, BATTLE_SHOUT_RANKS)
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::context::tests::make_test_ctx;

    #[test]
    fn tree_builds_without_panic() {
        let _tree = build_tree();
    }

    #[test]
    fn tree_returns_failure_when_no_target_out_of_combat() {
        use crate::engine::bt_nodes::BtResult;
        let tree = build_tree();
        let mut owned = make_test_ctx();
        // Default context: no combat, no target → should return Failure (nothing to do)
        let result = tree.tick(&mut owned.ctx());
        assert_eq!(result, BtResult::Failure);
    }
}
