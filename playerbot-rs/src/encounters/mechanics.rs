/// Generic encounter mechanics — reusable BT nodes for common boss mechanics.
///
/// These nodes are used by individual boss encounter subtrees.  They are
/// composable with the class-specific combat rotation subtrees.
use crate::engine::bt_nodes::{BtNode, BtResult, action, cond, seq, sel};
use crate::ffi::SpellId;

/// Flee to the nearest safe position (away from ground AoEs / hazards).
///
/// Uses `get_safe_position` from the interface.  Returns Running while
/// moving, Success once a safe position was reached or not needed,
/// Failure if no safe position can be found.
pub fn flee_to_safe_position(search_radius: f32) -> Box<dyn BtNode> {
    action(move |ctx| {
        if let Some(pos) = ctx.interface.get_safe_position(search_radius) {
            if ctx.interface.move_to(pos.x, pos.y, pos.z) {
                return BtResult::Running;
            }
        }
        BtResult::Failure
    })
}

/// Run away from the group when the bot has a specific debuff (e.g. Living Bomb,
/// Mutating Injection) — move to a safe position far enough to avoid splash damage.
///
/// `debuff_spell_id`: the aura spell ID to check on self.
/// `run_distance`: how far to search for a safe spot.
pub fn isolate_if_debuffed(debuff_spell_id: SpellId, run_distance: f32) -> Box<dyn BtNode> {
    seq(vec![
        cond(move |ctx| ctx.interface.has_aura(ctx.bot_handle, debuff_spell_id)),
        flee_to_safe_position(run_distance),
    ])
}

/// Move away from the boss/current target to maintain a minimum range.
///
/// Used by ranged and healer bots when a boss has a melee-range cleave.
pub fn maintain_range(min_range: f32) -> Box<dyn BtNode> {
    seq(vec![
        cond(move |ctx| {
            ctx.current_target()
                .map_or(false, |t| ctx.interface.unit_distance(t) < min_range)
        }),
        action(move |ctx| {
            if let Some(pos) = ctx.interface.get_safe_position(min_range * 2.0) {
                if ctx.interface.move_to(pos.x, pos.y, pos.z) {
                    return BtResult::Running;
                }
            }
            BtResult::Failure
        }),
    ])
}

/// Stop attacking and spread out from other raid members.
///
/// Used during spread mechanics (e.g. Heigan eruptions, Thaddius polarity).
pub fn spread_from_group(spread_radius: f32, index: u8, total: u8) -> Box<dyn BtNode> {
    action(move |ctx| {
        let center = match ctx.current_target() {
            Some(t) => t,
            None    => return BtResult::Failure,
        };
        let pos = ctx.interface.get_spread_position(center, spread_radius, index, total);
        if ctx.interface.move_to(pos.x, pos.y, pos.z) {
            BtResult::Running
        } else {
            BtResult::Failure
        }
    })
}

/// Taunt the primary target if our threat is below the tank's threat threshold.
/// Only executed by bots with the TANK role.
pub fn taunt_if_needed() -> Box<dyn BtNode> {
    seq(vec![
        cond(|ctx| {
            // Only tank bots taunt.
            ctx.snap.self_.role & 1 != 0
        }),
        action(|ctx| {
            if let Some(target) = ctx.current_target() {
                if ctx.interface.taunt(target) {
                    return BtResult::Success;
                }
            }
            BtResult::Failure
        }),
    ])
}

/// Stop all movement and hold position.
pub fn hold_position() -> Box<dyn BtNode> {
    action(|ctx| {
        ctx.interface.stop_moving();
        BtResult::Success
    })
}
