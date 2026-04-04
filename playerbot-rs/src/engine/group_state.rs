/// Shared group/encounter state — one instance per group, shared via Arc<`RwLock`<>>.
///
/// The group's "coordinator" (leader bot) writes this once per world tick.
/// All other bots in the group read it (lock-free via `try_read` with stale fallback).
use crate::ffi::UnitHandle;

/// Bot role bitmask — mirrors the C-side role field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BotRole(pub u8);

impl BotRole {
    pub const NONE: Self = Self(0);
    pub const TANK: Self = Self(1);
    pub const HEAL: Self = Self(2);
    pub const DPS: Self = Self(4);

    pub fn is_tank(self) -> bool {
        self.0 & 1 != 0
    }
    pub fn is_heal(self) -> bool {
        self.0 & 2 != 0
    }
    pub fn is_dps(self) -> bool {
        self.0 & 4 != 0
    }
}

/// Encounter-specific role assignments computed by `GroupCoordinator`.
#[derive(Debug, Clone, Default)]
pub struct EncounterAssignments {
    pub main_tank: Option<UnitHandle>,
    pub off_tank: Option<UnitHandle>,
    pub healers: Vec<UnitHandle>,
    pub ranged_dps: Vec<UnitHandle>,
    pub melee_dps: Vec<UnitHandle>,
    /// Special roles keyed by an encounter-specific string (e.g. "`mc_breaker`", "`polarity_switch`")
    pub special: Vec<(String, UnitHandle)>,
}

impl EncounterAssignments {
    pub fn get_special(&self, role: &str) -> Option<UnitHandle> {
        self.special
            .iter()
            .find(|(r, _)| r == role)
            .map(|(_, h)| *h)
    }
}

/// State shared across all bots in one group/raid.
#[derive(Debug, Default)]
pub struct GroupState {
    pub assignments: EncounterAssignments,
    /// Server time (ms) when assignments were last recomputed.
    pub last_computed_ms: u64,
    /// Whether the group is currently in an active encounter (boss pulled).
    pub encounter_active: bool,
}

impl GroupState {
    pub fn is_stale(&self, now_ms: u64, threshold_ms: u64) -> bool {
        now_ms.saturating_sub(self.last_computed_ms) > threshold_ms
    }
}
