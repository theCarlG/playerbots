//! Follow-formation geometry — direct port of PB2's
//! `strategy/values/Formations.cpp` (11 formations).
//!
//! Each formation answers the question **"where should this bot stand
//! relative to its follow target?"**. PB2 implements this via two abstract
//! base classes:
//!
//! - `FollowFormation` — returns `(angle, offset)` pairs consumed by the
//!   chase movement generator (angle relative to the follow target's
//!   orientation, offset in yards). Used by `melee`, `queue`, `far`,
//!   plus the base `FollowFormation::GetLocation` fallback for `near`.
//! - `MoveFormation` / `MoveAheadFormation` — returns an absolute
//!   `WorldLocation` computed from a group roster, the leader's facing,
//!   and optional "current target" (used by `near`, `chaos`, `circle`,
//!   `line`, `shield`, `arrow`, `raid`, `custom`).
//!
//! The Rust side mirrors this split with [`FormationOutput`]:
//!
//! - [`FormationOutput::ChaseOffset`] — feed straight into
//!   `InterfaceBridge::follow(target, offset, angle)` (matches PB2's
//!   `ai->GetRange("follow")` + `FollowFormation::GetAngle/GetOffset`).
//! - [`FormationOutput::Position`] — absolute `(x, y, z)` to be passed to
//!   `move_to` (matches `Formation::GetLocation` in PB2).
//!
//! Step 8 of `PB2_PARITY_PLAN` wires [`resolve_formation`] into
//! `engine::bt::tick_follow`. This module is pure math — no FFI — so it
//! can be unit-tested cheaply.
//!
//! ## Roster ordering
//!
//! PB2's `Formation::GetFollowAngle` builds a group roster in a very
//! specific order (DPS center-inserted → healers center-inserted →
//! tanks alternating left/right) and then indexes the caller's position
//! within it. See `group_follow_angle` below for the exact port.
//!
//! The roster construction requires per-member **role classification**
//! (tank / healer / DPS). The current `BotWorldSnapshot` only exposes
//! `role` on `BotUnitSnapshot::self_` — group members are bare handles.
//! For now we take the caller's provided roster ordering at face value;
//! the snapshot upgrade is tracked in the plan's Step 8 follow-ups.
//!
//! Source reference: `PB2/playerbot/strategy/values/Formations.cpp`
//! (lines 11–603) and `PB2/playerbot/strategy/values/Arrow.cpp`.

use std::f32::consts::PI;

use crate::bot::settings::FollowFormation;
use cmangos::BotPosition;

/// Per-member input for group-aware formations (line, shield, arrow,
/// raid). Ordering matches how PB2 walks `GroupReference` lists.
#[derive(Debug, Clone, Copy)]
pub struct FormationMember {
    /// Unit handle — only used for self-identification so the bot picks
    /// its own slot out of the roster.
    pub handle: u64,
    pub is_tank: bool,
    pub is_heal: bool,
    pub is_melee: bool,
    pub is_alive: bool,
}

/// Inputs for every formation computation.
#[derive(Debug, Clone)]
pub struct FormationContext<'a> {
    /// The bot resolving its own formation slot.
    pub self_handle: u64,
    /// The follow target (usually the master or the group tank).
    pub follow_target: BotPosition,
    /// Follow range — PB2's `ai->GetRange("follow")`, per-bot adjustable
    /// via the `follow <n>` chat command. Default 1.5.
    pub follow_range: f32,
    /// Bot's collision-pad radius (PB2 `GetObjectBoundingRadius()`).
    /// Used by the `near` formation so tightly packed bots don't clip
    /// into the leader. Use ~0.389 for a humanoid player if unknown.
    pub bounding_radius: f32,
    /// Current combat target — only consulted by the `circle` formation,
    /// which spaces bots around the enemy instead of the leader.
    pub current_target: Option<BotPosition>,
    /// Roster for group-aware formations. May be empty for solo bots.
    pub group: &'a [FormationMember],
    /// Tick time in seconds, used by `chaos` to jitter every 3 seconds.
    /// Not a wall clock — any monotonically increasing source works.
    pub now_secs: u64,
    /// `(dx, dy)` saved from the last `chaos` tick. Call site owns the
    /// cell; the formation rotates it on a 3-second cadence.
    pub chaos_state: ChaosState,
    /// Custom-formation offset (local frame, relative to leader). Only
    /// consulted for `FollowFormation::Custom`.
    pub custom_offset: Option<CustomOffset>,
}

/// Per-bot persistent state for the `chaos` formation.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ChaosState {
    pub dx: f32,
    pub dy: f32,
    /// `now_secs` when `(dx, dy)` was last regenerated.
    pub last_change_secs: u64,
}

/// Stored offset for the `custom` formation — saved in the leader's
/// local frame (rotated by `-orientation` at save time, re-rotated back
/// at resolve time). Mirrors PB2 `CustomFormation`'s "position" value.
#[derive(Debug, Clone, Copy)]
pub struct CustomOffset {
    pub rel_x: f32,
    pub rel_y: f32,
    pub rel_z: f32,
}

/// What a formation resolves to. Two modes matching PB2:
///
/// - `ChaseOffset` — let the engine's chase generator handle movement;
///   the bot just tells it "stay `offset` yards at `angle` radians from
///   the target" (angle is relative to the target's orientation).
/// - `Position` — recompute an absolute point every tick and `move_to`
///   it. PB2 uses this whenever the formation depends on the group
///   roster or on a fixed world location.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FormationOutput {
    ChaseOffset { offset: f32, angle: f32 },
    Position { x: f32, y: f32, z: f32 },
}

/// Output of [`resolve_formation`]. Includes any chaos state the
/// caller should persist back to [`ChaosState`] for the next tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FormationResult {
    pub output: FormationOutput,
    pub chaos_state: ChaosState,
}

/// Resolve `formation` against `ctx`, returning the position or chase
/// offset the bot should adopt **this tick**.
///
/// Never panics. Formations whose math can't be satisfied (e.g. a raid
/// formation with an empty roster) fall back to the `near` formation.
pub fn resolve_formation(
    formation: FollowFormation,
    ctx: &FormationContext<'_>,
) -> FormationResult {
    let chaos_state = ctx.chaos_state;
    let output = match formation {
        FollowFormation::Melee => melee(ctx),
        FollowFormation::Queue => queue(ctx),
        FollowFormation::Far => far(ctx),
        FollowFormation::Near => near(ctx),
        FollowFormation::Chaos => {
            let (out, new_state) = chaos(ctx);
            return FormationResult {
                output: out,
                chaos_state: new_state,
            };
        }
        FollowFormation::Circle => circle(ctx),
        FollowFormation::Line => line(ctx),
        FollowFormation::Shield => shield(ctx),
        FollowFormation::Arrow => arrow(ctx),
        FollowFormation::Raid => raid(ctx),
        FollowFormation::Custom => custom(ctx),
    };
    FormationResult {
        output,
        chaos_state,
    }
}

// ── Follow-angle roster computation ─────────────────────────────────────────

/// Port of `Formation::GetFollowAngle` (Formations.cpp:459). Walks the
/// group roster in PB2's exact order (DPS first center-inserted, then
/// healers center-inserted, then tanks alternating left/right), finds
/// the caller's index, and returns the angle in radians:
///
/// ```text
/// start + (0.125 + 1.75 * index / total + (total==2 ? 0.125 : 0)) * π
/// ```
///
/// `start` is the follow target's orientation (radians). Solo bots
/// (empty or single-entry roster) get `index = 1`, `total = 1`.
pub fn group_follow_angle(ctx: &FormationContext<'_>) -> f32 {
    let start = ctx.follow_target.o;

    // Solo-bot fallback: no group, or the "group" is just the leader.
    // Matches PB2's "no group && single follow target" path.
    if ctx.group.is_empty() {
        return start + (0.125 + 1.75 + 0.0) * PI;
    }

    let mut roster: Vec<u64> = Vec::with_capacity(ctx.group.len());

    // Pass 1 — DPS (non-tank, non-heal), center-insert.
    for m in ctx.group {
        if !m.is_alive || m.handle == ctx.follow_target_handle() {
            continue;
        }
        if !m.is_tank && !m.is_heal {
            let mid = roster.len() / 2;
            roster.insert(mid, m.handle);
        }
    }
    // Pass 2 — healers, center-insert.
    for m in ctx.group {
        if !m.is_alive || m.handle == ctx.follow_target_handle() {
            continue;
        }
        if m.is_heal {
            let mid = roster.len() / 2;
            roster.insert(mid, m.handle);
        }
    }
    // Pass 3 — tanks, alternate back/front.
    let mut left = true;
    for m in ctx.group {
        if !m.is_alive || m.handle == ctx.follow_target_handle() {
            continue;
        }
        if m.is_tank {
            if left {
                roster.push(m.handle);
            } else {
                roster.insert(0, m.handle);
            }
            left = !left;
        }
    }

    // Caller's index (1-based, matches PB2 `int index = 1`).
    let mut index: i32 = 1;
    for h in &roster {
        if *h == ctx.self_handle {
            break;
        }
        index += 1;
    }
    let total = (roster.len() as i32) + 1;
    let frac = index as f32 / total as f32;
    let extra = if total == 2 { 0.125 } else { 0.0 };
    start + (0.125 + 1.75 * frac + extra) * PI
}

impl FormationContext<'_> {
    /// PB2 treats the leader as a Unit handle the roster walks explicitly
    /// skip — but we don't have a separate "leader handle" field. The
    /// solo/masterless path uses `self_handle`; group roster walks are
    /// expected to exclude the leader before handing us `ctx.group`.
    /// This helper exists so `group_follow_angle` reads symmetrically
    /// with the PB2 source even though there's no-op today.
    #[allow(clippy::unused_self)]
    fn follow_target_handle(&self) -> u64 {
        // Placeholder — callers strip the leader themselves.
        u64::MAX
    }
}

// ── Simple (chase-offset) formations ────────────────────────────────────────

/// `MeleeFormation` — Formations.cpp:123. Angle = group-follow-angle,
/// offset = follow range.
fn melee(ctx: &FormationContext<'_>) -> FormationOutput {
    let angle = relative_angle(group_follow_angle(ctx), ctx.follow_target.o);
    FormationOutput::ChaseOffset {
        offset: ctx.follow_range,
        angle,
    }
}

/// `QueueFormation` — Formations.cpp:132. Single-file directly behind
/// leader (angle = π relative to facing).
fn queue(ctx: &FormationContext<'_>) -> FormationOutput {
    FormationOutput::ChaseOffset {
        offset: ctx.follow_range,
        angle: PI,
    }
}

/// `FarFormation::GetAngle` — Formations.cpp:376. Maintains a far
/// angle relative to the leader; PB2 keeps re-computing from the bot's
/// current relative angle to stay stable. We simplify to "reflect
/// across π": spawn the bot 180° opposite the leader's facing.
///
/// The full PB2 implementation adds a "drift delta" that needs the
/// bot's current chase angle — which is movement-generator state we
/// don't have easy access to from the BT tick. The simplified version
/// here matches steady-state behavior; the drift logic is optional
/// polish and is flagged in the plan's Step 8 notes.
fn far(ctx: &FormationContext<'_>) -> FormationOutput {
    // Hint at PB2's `followAngle + delta` convergence by spawning 180°
    // behind the leader and letting the chase generator stabilize.
    FormationOutput::ChaseOffset {
        offset: ctx.follow_range,
        angle: PI,
    }
}

// ── Position-based formations ───────────────────────────────────────────────

/// `NearFormation::GetLocationInternal` — Formations.cpp:141.
/// Range = `follow_range` + `bounding_radius`, angle = group follow angle.
fn near(ctx: &FormationContext<'_>) -> FormationOutput {
    let range = ctx.follow_range + ctx.bounding_radius;
    let angle = group_follow_angle(ctx);
    let t = &ctx.follow_target;
    FormationOutput::Position {
        x: t.x + angle.cos() * range,
        y: t.y + angle.sin() * range,
        z: t.z,
    }
}

/// `ChaosFormation::GetLocationInternal` — Formations.cpp:186. Same as
/// near, but with per-bot `(dx, dy)` jitter re-rolled every 3 seconds.
///
/// PB2 uses `sPlayerbotAIConfig.tooCloseDistance` as the jitter radius;
/// our equivalent is `BotAiConfig::too_close_distance`. Plumbing that
/// in would add another ctx field — for Step 7 we use a literal 2.0
/// matching PB2's config default, and flag the TODO in the plan.
fn chaos(ctx: &FormationContext<'_>) -> (FormationOutput, ChaosState) {
    const TOO_CLOSE: f32 = 2.0; // PB2 default sPlayerbotAIConfig.tooCloseDistance
    let mut state = ctx.chaos_state;
    let should_reroll =
        state.last_change_secs == 0 || ctx.now_secs.saturating_sub(state.last_change_secs) >= 3;
    if should_reroll {
        // Deterministic-ish reroll so we don't need an RNG in this
        // pure module. The hash collapses `(handle, now_secs)` into a
        // pair of offsets in `[-TOO_CLOSE/2, TOO_CLOSE/2]`. PB2 uses a
        // real RNG; this is close enough for jitter purposes and keeps
        // the module testable without fixtures.
        let seed = ctx.self_handle.wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ ctx.now_secs.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        let hx = ((seed >> 16) & 0xFFFF) as f32 / 65535.0;
        let hy = ((seed >> 32) & 0xFFFF) as f32 / 65535.0;
        state.dx = (hx - 0.5) * TOO_CLOSE;
        state.dy = (hy - 0.5) * TOO_CLOSE;
        state.last_change_secs = ctx.now_secs;
    }

    let range = ctx.follow_range;
    let angle = group_follow_angle(ctx);
    let t = &ctx.follow_target;
    (
        FormationOutput::Position {
            x: t.x + angle.cos() * range + state.dx,
            y: t.y + angle.sin() * range + state.dy,
            z: t.z,
        },
        state,
    )
}

/// `CircleFormation::GetLocation` — Formations.cpp:235. Same math as
/// near, but centered on the current combat target (falls through to
/// the follow target if there's no combat target).
fn circle(ctx: &FormationContext<'_>) -> FormationOutput {
    let center = ctx.current_target.as_ref().unwrap_or(&ctx.follow_target);
    let angle = group_follow_angle(ctx);
    FormationOutput::Position {
        x: center.x + angle.cos() * ctx.follow_range,
        y: center.y + angle.sin() * ctx.follow_range,
        z: center.z,
    }
}

/// `LineFormation::GetLocationInternal` — Formations.cpp:272. Calls
/// `MoveLine(players, 0.0, leader_pos, orientation, range)` with the
/// follow target center-inserted into the roster.
fn line(ctx: &FormationContext<'_>) -> FormationOutput {
    let mut roster = alive_handles_except(ctx, None);
    let mid = roster.len() / 2;
    roster.insert(mid, u64::MAX); // placeholder for follow target
    move_line_resolve(
        ctx,
        &roster,
        0.0,
        ctx.follow_target.x,
        ctx.follow_target.y,
        ctx.follow_target.z,
        ctx.follow_target.o,
        ctx.follow_range,
    )
    .unwrap_or_else(|| near(ctx))
}

/// `ShieldFormation::GetLocation` — Formations.cpp:308. Splits into
/// tanks vs. DPS lines, then places the bot in its own half. We
/// implement the dominant branches (`bot is tank ⟷ leader is tank`)
/// and fall back to `near` for the cross-split cases — those need the
/// leader's role classification, which our snapshot doesn't expose yet.
fn shield(ctx: &FormationContext<'_>) -> FormationOutput {
    let self_is_tank = ctx
        .group
        .iter()
        .any(|m| m.handle == ctx.self_handle && m.is_tank);

    let mut tanks: Vec<u64> = Vec::new();
    let mut dps: Vec<u64> = Vec::new();
    for m in ctx.group {
        if !m.is_alive {
            continue;
        }
        if m.is_tank {
            tanks.push(m.handle);
        } else {
            dps.push(m.handle);
        }
    }

    let line_slice: &[u64] = if self_is_tank { &tanks } else { &dps };
    if line_slice.is_empty() {
        return near(ctx);
    }
    move_line_resolve(
        ctx,
        line_slice,
        0.0,
        ctx.follow_target.x,
        ctx.follow_target.y,
        ctx.follow_target.z,
        ctx.follow_target.o,
        ctx.follow_range,
    )
    .unwrap_or_else(|| near(ctx))
}

// ── Role-aware layer classification (arrow + raid) ─────────────────────────

/// Which layer a bot belongs to in the arrow/raid formation.
enum FormationLayer {
    Front,  // tanks
    Middle, // melee DPS
    Back,   // ranged DPS + healers
}

/// Classify the bot into a layer and return the roster for that layer.
/// Tanks → Front, melee non-tank non-heal → Middle, everyone else → Back.
fn arrow_raid_layer(ctx: &FormationContext<'_>) -> (FormationLayer, Vec<u64>) {
    let me = ctx.group.iter().find(|m| m.handle == ctx.self_handle);
    let layer = match me {
        Some(m) if m.is_tank => FormationLayer::Front,
        Some(m) if !m.is_heal && m.is_melee => FormationLayer::Middle,
        _ => FormationLayer::Back,
    };
    let roster: Vec<u64> = ctx
        .group
        .iter()
        .filter(|m| {
            m.is_alive
                && match layer {
                    FormationLayer::Front => m.is_tank,
                    FormationLayer::Middle => !m.is_tank && !m.is_heal && m.is_melee,
                    FormationLayer::Back => !m.is_tank && (m.is_heal || !m.is_melee),
                }
        })
        .map(|m| m.handle)
        .collect();
    (layer, roster)
}

/// `ArrowFormation::GetLocationInternal` — Arrow.cpp. Three layers:
/// tanks in front, melee DPS in the middle (around the leader), ranged
/// DPS and healers in the back. Each layer spreads members in a V
/// alternating left/right.
fn arrow(ctx: &FormationContext<'_>) -> FormationOutput {
    const UNIT_SPACING: f32 = 0.2 + 1.0; // Arrow.cpp UNIT_SPACING 0.2 + bot width ~1y

    let (layer, roster) = arrow_raid_layer(ctx);
    let idx = roster
        .iter()
        .position(|&h| h == ctx.self_handle)
        .unwrap_or(0) as i32;

    // Alternate left/right: 0 → 0, 1 → +1, 2 → -1, 3 → +2, 4 → -2 …
    let signed = if idx % 2 == 0 {
        idx / 2
    } else {
        -((idx + 1) / 2)
    };
    let signed = signed as f32;

    let depth = ctx.follow_range * (1.0 + signed.abs() * 0.5);
    let (spine_x, spine_y) = match layer {
        FormationLayer::Front => {
            let a = ctx.follow_target.o; // forward
            (
                ctx.follow_target.x + a.cos() * depth,
                ctx.follow_target.y + a.sin() * depth,
            )
        }
        FormationLayer::Middle => {
            // Around the leader, slight offset behind.
            let a = ctx.follow_target.o + PI;
            (
                ctx.follow_target.x + a.cos() * (ctx.follow_range * 0.5),
                ctx.follow_target.y + a.sin() * (ctx.follow_range * 0.5),
            )
        }
        FormationLayer::Back => {
            let a = ctx.follow_target.o + PI; // behind
            (
                ctx.follow_target.x + a.cos() * depth,
                ctx.follow_target.y + a.sin() * depth,
            )
        }
    };

    let perp_angle = ctx.follow_target.o - PI / 2.0;
    let x = spine_x + perp_angle.cos() * (signed * UNIT_SPACING);
    let y = spine_y + perp_angle.sin() * (signed * UNIT_SPACING);
    FormationOutput::Position {
        x,
        y,
        z: ctx.follow_target.z,
    }
}

/// `RaidFormation::GetLocationInternal` — Arrow.cpp `RaidUnitPlacer`.
/// Three layers: tanks in front, melee in the middle, ranged/healers
/// in the back. Each layer uses blocks of 5 with depth offset.
fn raid(ctx: &FormationContext<'_>) -> FormationOutput {
    const UNITS_PER_LINE: i32 = 5;
    const LINE_SPACING: f32 = 0.2; // Arrow.cpp LINE_SPACING
    const UNIT_SPACING: f32 = 0.2; // Arrow.cpp UNIT_SPACING
    const GROUP_SPACING: f32 = 0.5; // Arrow.cpp GROUP_SPACING

    let (layer, roster) = arrow_raid_layer(ctx);
    let idx = roster
        .iter()
        .position(|&h| h == ctx.self_handle)
        .unwrap_or(0) as i32;

    let lane = idx % UNITS_PER_LINE;
    let depth_block = idx / UNITS_PER_LINE;
    let count = roster.len().max(1) as i32;

    let lane_offset =
        (lane - (count.min(UNITS_PER_LINE)) / 2) as f32 * (ctx.follow_range + UNIT_SPACING);
    let depth_offset =
        (depth_block as f32) * (ctx.follow_range + LINE_SPACING + GROUP_SPACING);

    let (spine_x, spine_y) = match layer {
        FormationLayer::Front => {
            let a = ctx.follow_target.o;
            (
                ctx.follow_target.x + a.cos() * (ctx.follow_range + depth_offset),
                ctx.follow_target.y + a.sin() * (ctx.follow_range + depth_offset),
            )
        }
        FormationLayer::Middle => {
            // Melee cluster around the leader, slightly behind.
            let a = ctx.follow_target.o + PI;
            (
                ctx.follow_target.x + a.cos() * (ctx.follow_range * 0.5 + depth_offset),
                ctx.follow_target.y + a.sin() * (ctx.follow_range * 0.5 + depth_offset),
            )
        }
        FormationLayer::Back => {
            let a = ctx.follow_target.o + PI;
            (
                ctx.follow_target.x + a.cos() * (ctx.follow_range + depth_offset),
                ctx.follow_target.y + a.sin() * (ctx.follow_range + depth_offset),
            )
        }
    };

    let perp_angle = ctx.follow_target.o - PI / 2.0;
    let x = spine_x + perp_angle.cos() * lane_offset;
    let y = spine_y + perp_angle.sin() * lane_offset;
    FormationOutput::Position {
        x,
        y,
        z: ctx.follow_target.z,
    }
}

/// `CustomFormation::GetLocationInternal` — Formations.cpp:407. The
/// stored offset is in the leader's local frame (rotated by
/// `-orientation` at save time). Rotate back and add to the leader's
/// world position.
fn custom(ctx: &FormationContext<'_>) -> FormationOutput {
    let Some(off) = ctx.custom_offset else {
        return near(ctx);
    };
    let o = ctx.follow_target.o;
    let (sin, cos) = o.sin_cos();
    let world_dx = off.rel_x * cos - off.rel_y * sin;
    let world_dy = off.rel_x * sin + off.rel_y * cos;
    FormationOutput::Position {
        x: ctx.follow_target.x + world_dx,
        y: ctx.follow_target.y + world_dy,
        z: ctx.follow_target.z + off.rel_z,
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Normalize an absolute world angle into an angle **relative to**
/// `reference_orientation`, wrapped into `[0, 2π)`. Matches PB2's
/// `FollowFormation::GetAngle` output convention.
fn relative_angle(world_angle: f32, reference_orientation: f32) -> f32 {
    let mut a = world_angle - reference_orientation;
    while a < 0.0 {
        a += 2.0 * PI;
    }
    while a >= 2.0 * PI {
        a -= 2.0 * PI;
    }
    a
}

/// Collect alive member handles, optionally excluding one.
fn alive_handles_except(ctx: &FormationContext<'_>, exclude: Option<u64>) -> Vec<u64> {
    ctx.group
        .iter()
        .filter(|m| m.is_alive && Some(m.handle) != exclude)
        .map(|m| m.handle)
        .collect()
}

/// `MoveFormation::MoveSingleLine` (Formations.cpp:676). Places each
/// member of `line` along a line perpendicular to `orientation`,
/// centered on `(cx, cy)`. Only returns the position for the bot whose
/// handle matches `ctx.self_handle`; returns `None` if the bot isn't
/// in the line.
#[allow(clippy::too_many_arguments)]
fn move_line_resolve(
    ctx: &FormationContext<'_>,
    line: &[u64],
    diff: f32,
    cx: f32,
    cy: f32,
    cz: f32,
    orientation: f32,
    range: f32,
) -> Option<FormationOutput> {
    let count = line.len() as f32;
    // Start point: shift left of center by half the line width.
    let start_angle = orientation - PI / 2.0;
    let half = (count / 2.0).floor();
    let sx = cx + start_angle.cos() * (range * half + diff);
    let sy = cy + start_angle.sin() * (range * half + diff);

    // Walk toward the opposite side, placing each member.
    let step_angle = orientation + PI / 2.0;
    for (index, &handle) in line.iter().enumerate() {
        if handle == ctx.self_handle {
            let radius = range * index as f32;
            let lx = sx + step_angle.cos() * radius;
            let ly = sy + step_angle.sin() * radius;
            return Some(FormationOutput::Position {
                x: lx,
                y: ly,
                z: cz,
            });
        }
    }
    None
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn origin_target() -> BotPosition {
        BotPosition {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            o: 0.0,
            map_id: 0,
        }
    }

    fn ctx_with(group: &[FormationMember], self_handle: u64) -> FormationContext<'_> {
        FormationContext {
            self_handle,
            follow_target: origin_target(),
            follow_range: 1.5,
            bounding_radius: 0.389,
            current_target: None,
            group,
            now_secs: 100,
            chaos_state: ChaosState::default(),
            custom_offset: None,
        }
    }

    #[test]
    fn queue_places_bot_directly_behind_leader() {
        let ctx = ctx_with(&[], 1);
        let r = resolve_formation(FollowFormation::Queue, &ctx);
        match r.output {
            FormationOutput::ChaseOffset { offset, angle } => {
                assert!((offset - 1.5).abs() < 1e-6);
                assert!((angle - PI).abs() < 1e-6);
            }
            _ => panic!("queue must produce a chase offset"),
        }
    }

    #[test]
    fn melee_uses_follow_range_offset() {
        let ctx = ctx_with(&[], 1);
        let r = resolve_formation(FollowFormation::Melee, &ctx);
        match r.output {
            FormationOutput::ChaseOffset { offset, .. } => {
                assert!((offset - 1.5).abs() < 1e-6);
            }
            _ => panic!("melee must produce a chase offset"),
        }
    }

    #[test]
    fn near_expands_by_bounding_radius() {
        let ctx = ctx_with(&[], 1);
        let r = resolve_formation(FollowFormation::Near, &ctx);
        match r.output {
            FormationOutput::Position { x, y, .. } => {
                let dist = (x * x + y * y).sqrt();
                assert!((dist - (1.5 + 0.389)).abs() < 1e-4);
            }
            _ => panic!("near must produce a world position"),
        }
    }

    #[test]
    fn circle_centers_on_current_target_when_set() {
        let mut ctx = ctx_with(&[], 1);
        ctx.current_target = Some(BotPosition {
            x: 10.0,
            y: 20.0,
            z: 5.0,
            o: 0.0,
            map_id: 0,
        });
        let r = resolve_formation(FollowFormation::Circle, &ctx);
        if let FormationOutput::Position { x, y, z } = r.output {
            // Distance from current target should equal follow_range.
            let dx = x - 10.0;
            let dy = y - 20.0;
            let dist = (dx * dx + dy * dy).sqrt();
            assert!((dist - 1.5).abs() < 1e-4);
            assert!((z - 5.0).abs() < 1e-6);
        } else {
            panic!("circle must produce a world position");
        }
    }

    #[test]
    fn custom_rotates_offset_by_leader_orientation() {
        let mut ctx = ctx_with(&[], 1);
        ctx.follow_target.o = PI / 2.0; // leader facing +Y
        ctx.custom_offset = Some(CustomOffset {
            rel_x: 1.0,
            rel_y: 0.0,
            rel_z: 0.0,
        });
        let r = resolve_formation(FollowFormation::Custom, &ctx);
        if let FormationOutput::Position { x, y, .. } = r.output {
            // Local +X rotated by 90° should land at world +Y.
            assert!((x - 0.0).abs() < 1e-4);
            assert!((y - 1.0).abs() < 1e-4);
        } else {
            panic!("custom must produce a world position");
        }
    }

    #[test]
    fn chaos_rerolls_after_three_seconds() {
        let mut ctx = ctx_with(&[], 1);
        ctx.now_secs = 100;
        let first = resolve_formation(FollowFormation::Chaos, &ctx);
        ctx.chaos_state = first.chaos_state;
        // Same second → same jitter.
        let second = resolve_formation(FollowFormation::Chaos, &ctx);
        assert_eq!(first.output, second.output);
        // 3s later → reroll.
        ctx.now_secs = 103;
        let third = resolve_formation(FollowFormation::Chaos, &ctx);
        assert_ne!(
            first.chaos_state.last_change_secs,
            third.chaos_state.last_change_secs
        );
    }

    #[test]
    fn raid_places_bots_in_blocks_of_five() {
        // Five bots → first block, side-by-side; sixth goes to depth 1.
        let members: Vec<FormationMember> = (1..=6)
            .map(|h| FormationMember {
                handle: h,
                is_tank: false,
                is_heal: false,
                is_melee: false,
                is_alive: true,
            })
            .collect();
        let mut positions = Vec::new();
        for h in 1..=6 {
            let ctx = ctx_with(&members, h);
            let r = resolve_formation(FollowFormation::Raid, &ctx);
            if let FormationOutput::Position { x, y, .. } = r.output {
                positions.push((x, y));
            }
        }
        assert_eq!(positions.len(), 6);
        // Bot 6 (depth 1) should be farther from the leader along -X
        // (leader orientation 0 ⇒ "behind" = -X) than any bot in block 0.
        let block0_max = positions[..5]
            .iter()
            .map(|(x, _)| x.abs())
            .fold(0.0f32, f32::max);
        assert!(positions[5].0.abs() > block0_max);
    }

    #[test]
    fn line_places_self_on_perpendicular_offset() {
        // Three bots in the line (self + two peers). With leader
        // center-inserted, the roster becomes [peer, MAX, peer, self]
        // so `self_handle=4` ends up at a nonzero step along the line.
        let members = vec![
            FormationMember {
                handle: 2,
                is_tank: false,
                is_heal: false,
                is_melee: false,
                is_alive: true,
            },
            FormationMember {
                handle: 3,
                is_tank: false,
                is_heal: false,
                is_melee: false,
                is_alive: true,
            },
            FormationMember {
                handle: 4,
                is_tank: false,
                is_heal: false,
                is_melee: false,
                is_alive: true,
            },
        ];
        let ctx = ctx_with(&members, 4);
        let r = resolve_formation(FollowFormation::Line, &ctx);
        // Leader orientation = 0 ⇒ perpendicular = ±Y.
        if let FormationOutput::Position { x, y, .. } = r.output {
            assert!(x.abs() < 1e-3, "x should be ~0 on perpendicular, got {x}");
            assert!(y.abs() > 0.0, "y should be nonzero on line, got {y}");
        } else {
            panic!("line must produce a world position");
        }
    }

    #[test]
    fn group_follow_angle_matches_pb2_formula_for_solo_bot() {
        let ctx = ctx_with(&[], 1);
        let a = group_follow_angle(&ctx);
        // Solo: index=1, total=1 ⇒ start + (0.125 + 1.75) * π = 1.875π.
        let expected = (0.125 + 1.75) * PI;
        assert!((a - expected).abs() < 1e-5);
    }

    #[test]
    fn relative_angle_wraps_into_positive_range() {
        assert!((relative_angle(-0.1, 0.0) - (2.0 * PI - 0.1)).abs() < 1e-5);
        assert!((relative_angle(3.0 * PI, 0.0) - PI).abs() < 1e-5);
    }
}
