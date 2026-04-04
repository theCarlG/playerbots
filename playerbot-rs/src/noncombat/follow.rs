/// Follow — keep the bot close to the group leader when out of combat.
///
/// Priority: follow tank → follow healer → follow any group member.
/// If no group exists (solo bot), does nothing.
/// Throttled: only issues a new follow command every 2 seconds to avoid
/// spamming the motion master.
use crate::engine::bt_nodes::{BtNode, BtResult, action, cond, sel, seq, throttle};

/// Follow distance in yards (stay behind leader, not on top of them).
const FOLLOW_DIST: f32 = 3.0;
/// Minimum yards before a new follow command is issued (dead-zone avoidance).
const REFOLLOW_THRESHOLD: f32 = 8.0;
/// Re-issue follow at most once per 2 seconds.
const FOLLOW_THROTTLE_MS: u64 = 2_000;

/// Build the follow subtree.
///
/// Plugs into the root Selector below the combat tree. Returns Failure
/// immediately when in combat, so the combat tree has priority.
pub fn build_follow_subtree() -> Box<dyn BtNode> {
    seq(vec![
        // Only follow when not in combat.
        cond(|ctx| !ctx.in_combat()),

        // Find a leader and follow them if far enough away.
        throttle(FOLLOW_THROTTLE_MS, sel(vec![
            // Prefer following the designated tank.
            seq(vec![
                cond(|ctx| {
                    ctx.interface.group_get_tank()
                        .map_or(false, |t| ctx.interface.unit_distance(t) > REFOLLOW_THRESHOLD)
                }),
                action(|ctx| {
                    if let Some(tank) = ctx.interface.group_get_tank() {
                        if ctx.interface.follow(tank, FOLLOW_DIST, 0.0) {
                            return BtResult::Success;
                        }
                    }
                    BtResult::Failure
                }),
            ]),

            // Fall back to following any group member.
            action(|ctx| {
                let member = ctx.snap.group_members[..ctx.snap.group_size as usize]
                    .iter()
                    .copied()
                    .filter(|&h| h != 0 && h != ctx.bot_handle)
                    .find(|&h| ctx.interface.unit_distance(h) > REFOLLOW_THRESHOLD);

                if let Some(target) = member {
                    if ctx.interface.follow(target, FOLLOW_DIST, 0.0) {
                        return BtResult::Success;
                    }
                }
                BtResult::Failure
            }),
        ])),
    ])
}
