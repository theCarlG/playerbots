/// Consumables — eat and drink to recover HP/mana between pulls.
///
/// When HP < 70 % or mana < 40 % and the bot is out of combat, it stops
/// moving and "rests" until thresholds are met.  Actual item usage is left
/// to the `CMaNGOS` side (the C++ wrapper can inject food/drink on the bot
/// character).  The Rust side simply holds the bot still and blocks other
/// actions (follow, buffing) until recovery completes.
///
/// Mana-free classes (warriors, rogues, death knights) skip the mana check.
use crate::engine::bt_nodes::{BtNode, BtResult, action, cond, seq};

pub const HP_EAT_THRESHOLD: f32 = 0.70; // start eating below this HP%
pub const HP_FULL_THRESHOLD: f32 = 0.90; // stop eating above this HP%
pub const MANA_DRINK_THRESHOLD: f32 = 0.40;
pub const MANA_FULL_THRESHOLD: f32 = 0.80;

/// Returns true if the class uses mana (needs a drink recovery check).
fn uses_mana(ctx: &crate::engine::context::TickContext<'_>) -> bool {
    // power_type 0 = mana; 1 = rage; 3 = energy; 6 = runic power
    ctx.snap.self_.power_type == 0
}

/// Build the consumables recovery subtree.
///
/// Plugged above the class combat tree in the root Selector.
/// Returns Running while recovering (blocks follow/buffing until done).
/// Returns Failure immediately when in combat or already at full resources.
pub fn build_consumables_subtree() -> Box<dyn BtNode> {
    seq(vec![
        // Only recover out of combat.
        cond(|ctx| !ctx.in_combat()),
        // At least one resource is below threshold.
        cond(|ctx| {
            let hp_low = ctx.self_hp_pct() < HP_EAT_THRESHOLD;
            let mana_low = uses_mana(ctx) && ctx.self_mana_pct() < MANA_DRINK_THRESHOLD;
            hp_low || mana_low
        }),
        // Stop moving and wait until both resources are recovered.
        action(|ctx| {
            let hp_full = ctx.self_hp_pct() >= HP_FULL_THRESHOLD;
            let mana_full = !uses_mana(ctx) || ctx.self_mana_pct() >= MANA_FULL_THRESHOLD;

            if hp_full && mana_full {
                // Done recovering — let the bot resume normal behavior.
                return BtResult::Success;
            }

            // Keep the bot stationary while recovering.
            ctx.interface.stop_moving();
            BtResult::Running
        }),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{bt_nodes::BtResult, context::tests::make_test_ctx};
    use cmangos::BotWorldSnapshot;

    #[test]
    fn consumables_returns_failure_when_hp_full() {
        let tree = build_consumables_subtree();
        let mut owned = make_test_ctx();
        owned.snap.self_.health = 1000;
        owned.snap.self_.max_health = 1000;
        owned.snap.self_.mana = 1000;
        owned.snap.self_.max_mana = 1000;
        owned.snap.self_.power_type = 0; // mana user
        // Out of combat + full HP/mana → Failure (nothing to do)
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Failure);
    }

    #[test]
    fn consumables_runs_when_out_of_combat_and_low_hp() {
        let tree = build_consumables_subtree();
        let mut owned = make_test_ctx();
        owned.snap.self_.health = 500;
        owned.snap.self_.max_health = 1000; // 50% < 70% threshold
        owned.snap.self_.mana = 1000;
        owned.snap.self_.max_mana = 1000;
        owned.snap.self_.power_type = 0;
        owned.snap.self_.in_combat = false;
        // Low HP → Running (recovering)
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Running);
    }

    #[test]
    fn consumables_returns_failure_when_in_combat() {
        let tree = build_consumables_subtree();
        let mut owned = make_test_ctx();
        owned.snap.self_.health = 200;
        owned.snap.self_.max_health = 1000; // 20% HP
        owned.snap.self_.in_combat = true;
        // In combat → Failure even though HP is low
        assert_eq!(tree.tick(&mut owned.ctx()), BtResult::Failure);
    }
}
