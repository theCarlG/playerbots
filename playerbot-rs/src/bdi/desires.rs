/// Desire taxonomy — what a bot can want.
///
/// Each `DesireKind` represents a high-level goal the bot can pursue.
/// The BDI layer scores all applicable desires based on beliefs, role,
/// personality, and context, then selects the highest-urgency one as
/// the current intention.
///
/// Desires map 1:1 to GOAP goal states via `goap::desire_to_goal()`.

/// All possible bot desires, ordered roughly by typical priority band.
///
/// Compact enum — fits in a u8. Used as an index into scoring tables
/// and as a bitmask key for GOAP action filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum DesireKind {
    // ── Survival (highest priority band) ──────────────────
    /// Self HP critical — need immediate self-preservation.
    Survive = 0,
    /// AoE danger, boss mechanic, overwhelmed — run away.
    FleeFromDanger = 1,

    // ── Group support ─────────────────────────────────────
    /// Party members injured — heal them.
    HealGroup = 2,
    /// Party members dead — resurrect them.
    ResurrectDead = 3,
    /// Party members debuffed — dispel them.
    DispelDebuffs = 4,
    /// Protect a specific ally taking damage.
    ProtectAlly = 5,

    // ── Combat ────────────────────────────────────────────
    /// Main tank duty — hold boss aggro.
    TankBoss = 6,
    /// DPS the current/assigned target.
    KillTarget = 7,
    /// Apply crowd control to an add or marked target.
    CrowdControl = 8,
    /// Interrupt a dangerous enemy cast.
    InterruptCast = 9,
    /// Threat dump or hold aggro as appropriate.
    ManageThreat = 10,
    /// Initiate combat by pulling the next pack.
    PullMobs = 11,

    // ── Tactical ──────────────────────────────────────────
    /// Move to a boss-mechanic-specific position.
    PositionForMechanic = 12,
    /// Execute a zone-specific duty (rune douse, suppression, etc.).
    ExecuteEncounterDuty = 13,

    // ── Maintenance ───────────────────────────────────────
    /// Eat/drink/bandage to recover HP/mana.
    RecoverResources = 14,
    /// Apply missing buffs to group.
    BuffGroup = 15,
    /// Repair gear at a vendor.
    RepairGear = 16,

    // ── World ─────────────────────────────────────────────
    /// Follow the master or group leader.
    FollowLeader = 17,
    /// Autonomously grind nearby mobs.
    GrindMobs = 18,
    /// Travel to a destination.
    Travel = 19,
    /// Nothing to do — idle at current location.
    #[default]
    Idle = 20,
}

impl DesireKind {
    /// Total number of desire variants. Used for fixed-size arrays.
    pub const COUNT: usize = 21;

    /// All variants in declaration order.
    pub const ALL: [Self; Self::COUNT] = [
        Self::Survive,
        Self::FleeFromDanger,
        Self::HealGroup,
        Self::ResurrectDead,
        Self::DispelDebuffs,
        Self::ProtectAlly,
        Self::TankBoss,
        Self::KillTarget,
        Self::CrowdControl,
        Self::InterruptCast,
        Self::ManageThreat,
        Self::PullMobs,
        Self::PositionForMechanic,
        Self::ExecuteEncounterDuty,
        Self::RecoverResources,
        Self::BuffGroup,
        Self::RepairGear,
        Self::FollowLeader,
        Self::GrindMobs,
        Self::Travel,
        Self::Idle,
    ];

    /// Minimum intention hold time for this desire kind (milliseconds).
    /// Prevents bots from switching goals every tick.
    pub fn min_hold_ms(self) -> u64 {
        match self {
            // Survival — short hold, situation changes fast
            Self::Survive | Self::FleeFromDanger => 1_000,
            // Combat — moderate hold
            Self::TankBoss
            | Self::KillTarget
            | Self::CrowdControl
            | Self::InterruptCast
            | Self::ManageThreat
            | Self::PullMobs => 2_000,
            // Group support — moderate hold
            Self::HealGroup
            | Self::ResurrectDead
            | Self::DispelDebuffs
            | Self::ProtectAlly => 2_000,
            // Tactical — hold until mechanic resolves
            Self::PositionForMechanic | Self::ExecuteEncounterDuty => 3_000,
            // Maintenance — longer hold, don't interrupt eating
            Self::RecoverResources | Self::BuffGroup | Self::RepairGear => 10_000,
            // World — long hold, travel takes time
            Self::FollowLeader | Self::GrindMobs | Self::Travel => 5_000,
            // Idle — hold indefinitely until something happens
            Self::Idle => 30_000,
        }
    }

    /// Bitmask representation for this desire (1 << discriminant).
    /// Used by GOAP actions to declare which desires they can satisfy.
    pub fn as_bit(self) -> u32 {
        1u32 << (self as u8)
    }
}

/// A scored desire — a `DesireKind` with an urgency value.
///
/// Urgency is 0.0 (irrelevant) to 1.0 (critical). The BDI layer picks
/// the desire with the highest urgency as the intention, subject to
/// persistence and hysteresis rules.
#[derive(Debug, Clone, Copy)]
pub struct ScoredDesire {
    pub kind: DesireKind,
    pub urgency: f32,
}

/// Score all desires given current beliefs, role, personality, and mode.
///
/// Returns a fixed-size array of scored desires. Urgency 0.0 means
/// "not applicable right now." The caller picks the highest.
pub fn score_desires(
    beliefs: &super::beliefs::BeliefSet,
    role: crate::ffi::BotRole,
    personality: &super::personality::Personality,
    encounter_active: bool,
    mode: crate::bot::settings::BehaviorMode,
) -> [ScoredDesire; DesireKind::COUNT] {
    use crate::bot::settings::BehaviorMode;

    let mut scores = [ScoredDesire {
        kind: DesireKind::Idle,
        urgency: 0.0,
    }; DesireKind::COUNT];

    // Initialize each slot with its kind
    for (i, kind) in DesireKind::ALL.iter().enumerate() {
        scores[i].kind = *kind;
    }

    if !beliefs.alive {
        // Dead — only desire is idle (dead tree handles resurrection)
        scores[DesireKind::Idle as usize].urgency = 1.0;
        return scores;
    }

    // Survival — universal, scales with danger
    if beliefs.hp_pct < 20 {
        scores[DesireKind::Survive as usize].urgency =
            0.9 * personality.caution;
    }
    if beliefs.threat_level as u8 >= super::beliefs::ThreatLevel::Critical as u8 {
        scores[DesireKind::FleeFromDanger as usize].urgency =
            0.85 * personality.caution;
    }

    if beliefs.in_combat {
        // Combat desires — mode doesn't suppress combat behavior
        if role.is_tank() {
            scores[DesireKind::TankBoss as usize].urgency = 0.8;
            scores[DesireKind::ManageThreat as usize].urgency = 0.5;
        } else if role.is_heal() {
            scores[DesireKind::HealGroup as usize].urgency = 0.8 * personality.helpfulness;
            if beliefs.party_needs_rez {
                scores[DesireKind::ResurrectDead as usize].urgency =
                    0.7 * personality.helpfulness;
            }
        } else {
            // DPS
            scores[DesireKind::KillTarget as usize].urgency =
                0.7 * personality.aggression;
        }
        // Interrupt is universal in combat
        scores[DesireKind::InterruptCast as usize].urgency = 0.6;
    } else {
        // Out of combat — mode drives which world-behavior desires dominate
        if beliefs.hp_pct < 80 || beliefs.mana_pct < 50 {
            scores[DesireKind::RecoverResources as usize].urgency = 0.6;
        }
        scores[DesireKind::BuffGroup as usize].urgency = 0.3;

        match mode {
            BehaviorMode::Follow => {
                scores[DesireKind::FollowLeader as usize].urgency = 0.5;
            }
            BehaviorMode::Grind => {
                scores[DesireKind::GrindMobs as usize].urgency =
                    0.5 * personality.aggression;
                scores[DesireKind::FollowLeader as usize].urgency = 0.2;
            }
            BehaviorMode::Stay | BehaviorMode::Guard => {
                // Stay put — idle is dominant, but still buff/recover
                scores[DesireKind::Idle as usize].urgency = 0.4;
            }
            BehaviorMode::Passive => {
                // Do nothing unless commanded
                scores[DesireKind::Idle as usize].urgency = 0.9;
            }
            _ => {
                // Quest, Rpg, Bg — default follow-like behavior for now
                scores[DesireKind::FollowLeader as usize].urgency = 0.4;
            }
        }
    }

    // Encounter duties
    if encounter_active {
        scores[DesireKind::PositionForMechanic as usize].urgency = 0.75;
        scores[DesireKind::ExecuteEncounterDuty as usize].urgency = 0.7;
    }

    // Idle is always available as a fallback
    if scores[DesireKind::Idle as usize].urgency < 0.01 {
        scores[DesireKind::Idle as usize].urgency = 0.01;
    }

    scores
}
