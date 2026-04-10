/// Intention — a committed desire the bot is actively pursuing.
///
/// The key requirement is "human persistence" — bots should not switch
/// goals every tick. An intention is held for a minimum duration and
/// only preempted by significantly more urgent desires.
use super::desires::{DesireKind, ScoredDesire};
use cmangos::UnitHandle;

/// A committed intention the bot is currently pursuing.
#[derive(Debug, Clone, Copy)]
pub struct Intention {
    /// Which desire this intention satisfies.
    pub desire: DesireKind,
    /// Server time (ms) when this intention was adopted.
    pub adopted_at_ms: u64,
    /// Minimum time before reconsidering (from `DesireKind::min_hold_ms`).
    pub min_hold_ms: u64,
    /// Urgency score when this intention was adopted.
    pub urgency_at_adoption: f32,
}

impl Default for Intention {
    fn default() -> Self {
        Self {
            desire: DesireKind::Idle,
            adopted_at_ms: 0,
            min_hold_ms: DesireKind::Idle.min_hold_ms(),
            urgency_at_adoption: 0.01,
        }
    }
}

/// A forced intention from a player command (attack, pull, focus, cc rti,
/// RTSC move/jump).
///
/// When present, this overrides the normal BDI desire evaluation. The
/// bot obeys commands first, thinks for itself second.
///
/// The variants split into two families:
/// - [`ForcedIntention::Desire`] reuses the autonomous-BDI machinery —
///   GOAP plans for the desire and the BT executes the plan.
/// - [`ForcedIntention::MoveToRtsc`] / [`ForcedIntention::JumpRtsc`] are
///   positional, not goal-shaped, so they bypass GOAP entirely. The
///   tick loop's RTSC consumer (`bot::tick::tick_rtsc_forced_intention`)
///   drives `move_to` every tick until arrival, then clears the
///   intention. See Gap #10 / Gap #13 in `playerbot-rs/GAPS.md`.
#[derive(Debug, Clone, Copy)]
pub enum ForcedIntention {
    /// Player command satisfied by an autonomous-style desire (kill,
    /// pull, cc, focus). GOAP plans for `desire`, BT executes.
    Desire {
        desire: DesireKind,
        target: Option<UnitHandle>,
    },
    /// Move to an RTSC waypoint until arrival. `exact` is reserved for
    /// the PB2 distinction between formation-offset moves and absolute
    /// moves; the consumer treats both the same way today.
    MoveToRtsc {
        x: f32,
        y: f32,
        z: f32,
        exact: bool,
    },
    /// Two-stage RTSC jump: first walk to `stage1`, then trigger the
    /// jump action toward `stage2`. The consumer flips `at_stage_two`
    /// once stage one is reached so the second tick fires the jump.
    /// PB2 reference: `RtscAction.cpp:315-344`.
    JumpRtsc {
        stage1: (f32, f32, f32),
        stage2: (f32, f32, f32),
        at_stage_two: bool,
    },
}

impl ForcedIntention {
    /// The autonomous-style desire this intention satisfies, for BDI
    /// scoring/monitor reporting. RTSC variants surface as `Travel`
    /// because that is the closest existing kind — they do not
    /// participate in GOAP scoring directly.
    pub fn desire(&self) -> DesireKind {
        match self {
            Self::Desire { desire, .. } => *desire,
            Self::MoveToRtsc { .. } | Self::JumpRtsc { .. } => DesireKind::Travel,
        }
    }

    /// The unit target, if any. RTSC variants are positional and have
    /// no target.
    pub fn target(&self) -> Option<UnitHandle> {
        match self {
            Self::Desire { target, .. } => *target,
            _ => None,
        }
    }

    /// True for the positional RTSC variants — used by the tick loop
    /// to skip the BDI replan path and route to the RTSC consumer.
    pub fn is_rtsc_positional(&self) -> bool {
        matches!(self, Self::MoveToRtsc { .. } | Self::JumpRtsc { .. })
    }
}

/// Urgency delta threshold for preempting the current intention early.
/// A new desire must exceed the current by this amount to override
/// before `min_hold_ms` has elapsed.
const PREEMPTION_THRESHOLD: f32 = 0.3;

/// Anti-oscillation cooldown entry. Recently-abandoned desires get a
/// penalty to prevent flip-flopping.
#[derive(Debug, Clone, Copy, Default)]
pub struct CooldownEntry {
    pub desire: DesireKind,
    pub until_ms: u64,
}

/// Maximum number of anti-oscillation cooldown slots.
const MAX_COOLDOWNS: usize = 4;

/// Evaluate whether the current intention should be kept, replaced, or
/// a new one adopted.
///
/// Returns `Some(new_intention)` if the intention should change,
/// `None` if the current intention should be maintained.
pub fn evaluate_intention(
    current: &Intention,
    scored: &[ScoredDesire],
    cooldowns: &[CooldownEntry; MAX_COOLDOWNS],
    server_time_ms: u64,
    discipline: f32,
) -> Option<Intention> {
    // Find the highest-urgency scored desire
    let best = scored
        .iter()
        .filter(|s| s.urgency > 0.0)
        .max_by(|a, b| a.urgency.partial_cmp(&b.urgency).unwrap_or(std::cmp::Ordering::Equal))?;

    // If it's the same desire we already have, keep it
    if best.kind == current.desire {
        return None;
    }

    // Check if current intention's hold time has elapsed
    let held_ms = server_time_ms.saturating_sub(current.adopted_at_ms);
    // Discipline personality scales the hold time (higher = more persistent)
    let effective_hold_ms = (current.min_hold_ms as f32 * (0.5 + discipline)) as u64;

    if held_ms < effective_hold_ms {
        // Still within hold period — only preempt for much more urgent desires
        let delta = best.urgency - current.urgency_at_adoption;
        if delta < PREEMPTION_THRESHOLD {
            return None;
        }
    }

    // Check anti-oscillation cooldown on the candidate
    for cd in cooldowns {
        if cd.desire == best.kind && server_time_ms < cd.until_ms {
            return None;
        }
    }

    // Current intention's preconditions may have become impossible
    // (handled by the caller checking if the current desire's urgency is 0)
    let current_urgency = scored
        .iter()
        .find(|s| s.kind == current.desire)
        .map_or(0.0, |s| s.urgency);

    // If current desire is still viable and we're within hold time, keep it
    if current_urgency > 0.0 && held_ms < effective_hold_ms {
        let delta = best.urgency - current_urgency;
        if delta < PREEMPTION_THRESHOLD {
            return None;
        }
    }

    // Adopt the new intention
    Some(Intention {
        desire: best.kind,
        adopted_at_ms: server_time_ms,
        min_hold_ms: best.kind.min_hold_ms(),
        urgency_at_adoption: best.urgency,
    })
}

/// Record an anti-oscillation cooldown for a desire that was just abandoned.
/// The abandoned desire gets a 3-second penalty.
pub fn record_cooldown(
    cooldowns: &mut [CooldownEntry; MAX_COOLDOWNS],
    desire: DesireKind,
    server_time_ms: u64,
) {
    const COOLDOWN_MS: u64 = 3_000;

    // Find an expired slot or the oldest slot
    let slot = cooldowns
        .iter()
        .position(|cd| server_time_ms >= cd.until_ms)
        .unwrap_or_else(|| {
            // All slots active — evict the one expiring soonest
            cooldowns
                .iter()
                .enumerate()
                .min_by_key(|(_, cd)| cd.until_ms)
                .map_or(0, |(i, _)| i)
        });

    cooldowns[slot] = CooldownEntry {
        desire,
        until_ms: server_time_ms + COOLDOWN_MS,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_scores(pairs: &[(DesireKind, f32)]) -> Vec<ScoredDesire> {
        pairs
            .iter()
            .map(|&(kind, urgency)| ScoredDesire { kind, urgency })
            .collect()
    }

    #[test]
    fn keeps_current_within_hold_time() {
        let current = Intention {
            desire: DesireKind::KillTarget,
            adopted_at_ms: 1000,
            min_hold_ms: 2000,
            urgency_at_adoption: 0.7,
        };
        let scores = make_scores(&[
            (DesireKind::KillTarget, 0.7),
            (DesireKind::HealGroup, 0.75), // slightly higher but within threshold
        ]);
        let cooldowns = [CooldownEntry::default(); MAX_COOLDOWNS];
        let result = evaluate_intention(&current, &scores, &cooldowns, 2000, 0.5);
        assert!(result.is_none(), "should keep current intention within hold time");
    }

    #[test]
    fn preempts_for_much_higher_urgency() {
        let current = Intention {
            desire: DesireKind::KillTarget,
            adopted_at_ms: 1000,
            min_hold_ms: 2000,
            urgency_at_adoption: 0.5,
        };
        let scores = make_scores(&[
            (DesireKind::KillTarget, 0.5),
            (DesireKind::Survive, 0.9), // exceeds threshold
        ]);
        let cooldowns = [CooldownEntry::default(); MAX_COOLDOWNS];
        let result = evaluate_intention(&current, &scores, &cooldowns, 1500, 0.5);
        assert!(result.is_some(), "should preempt for survival");
        assert_eq!(result.unwrap().desire, DesireKind::Survive);
    }

    #[test]
    fn switches_after_hold_time() {
        let current = Intention {
            desire: DesireKind::KillTarget,
            adopted_at_ms: 1000,
            min_hold_ms: 2000,
            urgency_at_adoption: 0.5,
        };
        let scores = make_scores(&[
            (DesireKind::KillTarget, 0.3), // dropped
            (DesireKind::RecoverResources, 0.6),
        ]);
        let cooldowns = [CooldownEntry::default(); MAX_COOLDOWNS];
        // After hold time elapsed (discipline 0.5 → effective hold = 2000 * 1.0 = 2000)
        let result = evaluate_intention(&current, &scores, &cooldowns, 3500, 0.5);
        assert!(result.is_some());
        assert_eq!(result.unwrap().desire, DesireKind::RecoverResources);
    }
}
