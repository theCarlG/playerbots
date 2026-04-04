/// Group targeting utilities used by healer and tank specs.
use crate::{
    engine::{
        bt_nodes::{BtNode, BtResult, action, cd_gate, gcd_gate},
        context::TickContext,
    },
    ffi::UnitHandle,
};

/// Find the group member (including self) with lowest HP% below `threshold`.
/// Returns None if everyone is above threshold or no group.
pub fn find_heal_target(ctx: &TickContext<'_>, threshold: f32) -> Option<UnitHandle> {
    let mut best: Option<UnitHandle> = None;
    let mut best_pct = threshold;

    // Check self
    let self_pct = ctx.self_hp_pct();
    if self_pct < best_pct && ctx.bot_handle != 0 {
        best_pct = self_pct;
        best = Some(ctx.bot_handle);
    }

    // Check group members
    let size = ctx.snap.group_size as usize;
    for i in 0..size {
        let h = ctx.snap.group_members[i];
        if h == 0 || h == ctx.bot_handle {
            continue;
        }
        let snap = ctx.interface.get_unit_snapshot(h);
        if !snap.is_alive || snap.max_health == 0 {
            continue;
        }
        let pct = snap.health as f32 / snap.max_health as f32;
        if pct < best_pct {
            best_pct = pct;
            best = Some(h);
        }
    }

    best
}

/// Find a group member (not self) below `threshold`. Prioritises lowest HP.
/// Useful for "heal party member" when self is full.
pub fn find_injured_party_member(ctx: &TickContext<'_>, threshold: f32) -> Option<UnitHandle> {
    let mut best: Option<UnitHandle> = None;
    let mut best_pct = threshold;

    let size = ctx.snap.group_size as usize;
    for i in 0..size {
        let h = ctx.snap.group_members[i];
        if h == 0 || h == ctx.bot_handle {
            continue;
        }
        let snap = ctx.interface.get_unit_snapshot(h);
        if !snap.is_alive || snap.max_health == 0 {
            continue;
        }
        let pct = snap.health as f32 / snap.max_health as f32;
        if pct < best_pct {
            best_pct = pct;
            best = Some(h);
        }
    }
    best
}

/// True if any group member (including self) is below `threshold`.
pub fn any_group_member_below(ctx: &TickContext<'_>, threshold: f32) -> bool {
    find_heal_target(ctx, threshold).is_some()
}

/// True if any party member (not self) is below `threshold`.
pub fn any_party_member_below(ctx: &TickContext<'_>, threshold: f32) -> bool {
    find_injured_party_member(ctx, threshold).is_some()
}

/// Build a heal action: cast `spell_id` on the lowest-HP group member below `threshold`.
/// Wraps in GCD gate + cooldown gate.
pub fn heal_action(spell_id: u32, threshold: f32) -> Box<dyn BtNode> {
    gcd_gate(cd_gate(spell_id, action(move |ctx| {
        if let Some(target) = find_heal_target(ctx, threshold) {
            if ctx.interface.cast_spell(spell_id, target) {
                ctx.timers.on_spell_cast(spell_id, ctx.server_time_ms);
                BtResult::Success
            } else {
                BtResult::Failure
            }
        } else {
            BtResult::Failure
        }
    })))
}

/// Build a heal action that only targets party members (not self).
pub fn heal_party_action(spell_id: u32, threshold: f32) -> Box<dyn BtNode> {
    gcd_gate(cd_gate(spell_id, action(move |ctx| {
        if let Some(target) = find_injured_party_member(ctx, threshold) {
            if ctx.interface.cast_spell(spell_id, target) {
                ctx.timers.on_spell_cast(spell_id, ctx.server_time_ms);
                BtResult::Success
            } else {
                BtResult::Failure
            }
        } else {
            BtResult::Failure
        }
    })))
}

/// Build a self-buff action: cast `spell_id` on self if `check` returns true.
pub fn self_buff_action(spell_id: u32, check: impl Fn(&TickContext<'_>) -> bool + Send + 'static)
    -> Box<dyn BtNode>
{
    gcd_gate(cd_gate(spell_id, action(move |ctx| {
        if !check(ctx) {
            return BtResult::Failure;
        }
        let me = ctx.bot_handle;
        if me == 0 {
            return BtResult::Failure;
        }
        if ctx.interface.cast_spell(spell_id, me) {
            ctx.timers.on_spell_cast(spell_id, ctx.server_time_ms);
            BtResult::Success
        } else {
            BtResult::Failure
        }
    })))
}
