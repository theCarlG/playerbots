//! Desire taxonomy — what a bot can want.
//!
//! Each `DesireKind` represents a high-level goal the bot can pursue.
//! The BDI layer scores all applicable desires based on beliefs, role,
//! personality, and context, then selects the highest-urgency one as
//! the current intention.
//!
//! Desires map 1:1 to GOAP goal states via `goap::desire_to_goal()`.

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
    /// `AoE` danger, boss mechanic, overwhelmed — run away.
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

    /// Short human-readable name for status/monitor output.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Survive => "survive",
            Self::FleeFromDanger => "flee",
            Self::HealGroup => "heal_group",
            Self::ResurrectDead => "resurrect",
            Self::DispelDebuffs => "dispel",
            Self::ProtectAlly => "protect",
            Self::TankBoss => "tank",
            Self::KillTarget => "kill",
            Self::CrowdControl => "cc",
            Self::InterruptCast => "interrupt",
            Self::ManageThreat => "threat",
            Self::PullMobs => "pull",
            Self::PositionForMechanic => "position",
            Self::ExecuteEncounterDuty => "encounter_duty",
            Self::RecoverResources => "recover",
            Self::BuffGroup => "buff",
            Self::RepairGear => "repair",
            Self::FollowLeader => "follow",
            Self::GrindMobs => "grind",
            Self::Travel => "travel",
            Self::Idle => "idle",
        }
    }
}

impl std::fmt::Display for DesireKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
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

/// Group desire counts for coordination — prevents duplicate roles.
///
/// Passed into `score_desires` so healers seeing others already healing
/// can fall through to dispel/rez, etc.
#[derive(Debug, Clone, Copy, Default)]
pub struct GroupDesireCounts {
    /// How many other bots currently have each desire active.
    /// Indexed by `DesireKind as usize`.
    pub counts: [u8; DesireKind::COUNT],
}

/// Score all desires given current beliefs, role, personality, and mode.
///
/// Returns a fixed-size array of scored desires. Urgency 0.0 means
/// "not applicable right now." The caller picks the highest.
pub fn score_desires(
    beliefs: &super::beliefs::BeliefSet,
    role: cmangos::BotRole,
    personality: &super::personality::Personality,
    encounter_active: bool,
    mode: crate::bot::settings::BehaviorMode,
    group_desires: Option<&GroupDesireCounts>,
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

    // Encounter duties. Raid mechanics MUST outrank baseline combat desires
    // (TankBoss 0.80, HealGroup 0.80, KillTarget 0.70*aggression). Boss
    // one-shots don't care that you were mid-rotation — positioning and
    // scripted duties take priority. Caution slightly modulates positioning
    // so aggro personalities are still willing to eat a hit for dps uptime.
    if encounter_active {
        scores[DesireKind::PositionForMechanic as usize].urgency =
            (0.92 * personality.caution).max(0.85);
        scores[DesireKind::ExecuteEncounterDuty as usize].urgency = 0.9;
    }

    // Group coordination: suppress desires that too many groupmates already have.
    // Note: HealGroup is NOT suppressed here — multiple healers should all want
    // to heal. Target deconfliction happens via HealAssignment tracking in
    // GroupState, not at the desire level.
    if let Some(gd) = group_desires {
        // If someone else is already tanking, we don't need to
        if gd.counts[DesireKind::TankBoss as usize] >= 1 && role.is_tank() {
            scores[DesireKind::TankBoss as usize].urgency *= 0.5;
            scores[DesireKind::ManageThreat as usize].urgency *= 1.3;
        }
        // If 2+ others already on CC, reduce ours
        if gd.counts[DesireKind::CrowdControl as usize] >= 2 {
            scores[DesireKind::CrowdControl as usize].urgency *= 0.4;
        }
        // If someone else is already rezzing, we don't need to
        if gd.counts[DesireKind::ResurrectDead as usize] >= 1 {
            scores[DesireKind::ResurrectDead as usize].urgency *= 0.3;
        }
    }

    // Idle is always available as a fallback
    if scores[DesireKind::Idle as usize].urgency < 0.01 {
        scores[DesireKind::Idle as usize].urgency = 0.01;
    }

    // Clamp all urgencies to [0.0, 1.0]
    for s in &mut scores {
        s.urgency = s.urgency.clamp(0.0, 1.0);
    }

    scores
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bdi::beliefs::BeliefSet;
    use crate::bdi::personality::Personality;
    use crate::bot::settings::BehaviorMode;

    fn default_personality() -> Personality {
        Personality {
            aggression: 0.5,
            caution: 0.5,
            helpfulness: 0.5,
            discipline: 0.5,
            initiative: 0.5,
        }
    }

    fn highest(scores: &[ScoredDesire; DesireKind::COUNT]) -> DesireKind {
        scores
            .iter()
            .max_by(|a, b| a.urgency.partial_cmp(&b.urgency).unwrap())
            .unwrap()
            .kind
    }

    #[test]
    fn dead_bot_wants_idle() {
        let mut beliefs = BeliefSet::default();
        beliefs.alive = false;
        let scores = score_desires(
            &beliefs,
            cmangos::BotRole::DPS,
            &default_personality(),
            false,
            BehaviorMode::Follow,
            None,
        );
        assert_eq!(highest(&scores), DesireKind::Idle);
        assert_eq!(scores[DesireKind::Idle as usize].urgency, 1.0);
    }

    #[test]
    fn tank_in_combat_wants_tank_boss() {
        let mut beliefs = BeliefSet::default();
        beliefs.in_combat = true;
        let scores = score_desires(
            &beliefs,
            cmangos::BotRole::TANK,
            &default_personality(),
            false,
            BehaviorMode::Follow,
            None,
        );
        assert_eq!(highest(&scores), DesireKind::TankBoss);
    }

    #[test]
    fn healer_in_combat_wants_heal_group() {
        let mut beliefs = BeliefSet::default();
        beliefs.in_combat = true;
        let scores = score_desires(
            &beliefs,
            cmangos::BotRole::HEAL,
            &Personality { helpfulness: 0.9, ..default_personality() },
            false,
            BehaviorMode::Follow,
            None,
        );
        assert_eq!(highest(&scores), DesireKind::HealGroup);
    }

    #[test]
    fn grind_mode_boosts_grind_mobs() {
        let beliefs = BeliefSet::default();
        let scores = score_desires(
            &beliefs,
            cmangos::BotRole::DPS,
            &Personality { aggression: 1.0, ..default_personality() },
            false,
            BehaviorMode::Grind,
            None,
        );
        assert!(scores[DesireKind::GrindMobs as usize].urgency > 0.4);
        assert!(
            scores[DesireKind::GrindMobs as usize].urgency
                > scores[DesireKind::FollowLeader as usize].urgency
        );
    }

    #[test]
    fn low_hp_boosts_survive() {
        let mut beliefs = BeliefSet::default();
        beliefs.hp_pct = 15;
        beliefs.in_combat = true;
        let scores = score_desires(
            &beliefs,
            cmangos::BotRole::DPS,
            &Personality { caution: 1.0, ..default_personality() },
            false,
            BehaviorMode::Follow,
            None,
        );
        assert_eq!(highest(&scores), DesireKind::Survive);
    }

    #[test]
    fn multiple_healers_all_want_to_heal() {
        // HealGroup should NOT be suppressed at the desire level — all healers
        // should want to heal. Target deconfliction happens via HealAssignment
        // tracking in GroupState, not here.
        let mut beliefs = BeliefSet::default();
        beliefs.in_combat = true;
        let mut gd = GroupDesireCounts::default();
        gd.counts[DesireKind::HealGroup as usize] = 3; // 3 others already healing

        let without = score_desires(
            &beliefs,
            cmangos::BotRole::HEAL,
            &Personality { helpfulness: 0.9, ..default_personality() },
            false,
            BehaviorMode::Follow,
            None,
        );
        let with = score_desires(
            &beliefs,
            cmangos::BotRole::HEAL,
            &Personality { helpfulness: 0.9, ..default_personality() },
            false,
            BehaviorMode::Follow,
            Some(&gd),
        );
        assert_eq!(
            with[DesireKind::HealGroup as usize].urgency,
            without[DesireKind::HealGroup as usize].urgency,
            "heal urgency should be the same regardless of how many others are healing"
        );
    }

    #[test]
    fn passive_mode_wants_idle() {
        let beliefs = BeliefSet::default();
        let scores = score_desires(
            &beliefs,
            cmangos::BotRole::DPS,
            &default_personality(),
            false,
            BehaviorMode::Passive,
            None,
        );
        assert_eq!(highest(&scores), DesireKind::Idle);
    }

    #[test]
    fn display_impl_works() {
        assert_eq!(DesireKind::KillTarget.to_string(), "kill");
        assert_eq!(DesireKind::HealGroup.to_string(), "heal_group");
        assert_eq!(DesireKind::Idle.to_string(), "idle");
    }
}
