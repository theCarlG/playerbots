/// Belief set — structured summary of "what the bot thinks the world is."
///
/// Updated from `BotWorldSnapshot` every tick. Fixed-size, stack-allocated,
/// zero heap allocation. The BDI layer reads beliefs to score desires.
use cmangos::{BotWorldSnapshot, UnitHandle};

/// How threatened the bot currently is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum ThreatLevel {
    /// No enemies, no danger.
    #[default]
    Safe = 0,
    /// In combat but not taking significant damage.
    Low = 1,
    /// Taking moderate damage or multiple attackers.
    Medium = 2,
    /// Health dropping fast, outnumbered.
    High = 3,
    /// About to die without intervention.
    Critical = 4,
}

/// Compact belief set. Updated from `BotWorldSnapshot` + attacker list each tick.
///
/// All fields are plain data — no references, no heap. This is cheap to copy
/// and diff. The `BeliefDiff` bitfield tracks which *categories* changed since
/// the previous tick, so desire evaluation can skip unchanged domains.
#[derive(Debug, Clone, Copy)]
pub struct BeliefSet {
    // ── Self state ──────────────────────────────────────────
    pub hp_pct: u8,
    pub mana_pct: u8,
    pub alive: bool,
    pub in_combat: bool,
    pub mounted: bool,

    // ── Threat / engagement ─────────────────────────────────
    pub threat_level: ThreatLevel,
    pub attacker_count: u8,
    pub has_target: bool,
    pub target_hp_pct: u8,

    // ── Group state ─────────────────────────────────────────
    pub group_size: u8,
    pub group_hp_min_pct: u8,
    pub group_injured_count: u8,
    pub group_dead_count: u8,
    pub tank_alive: bool,
    pub healer_alive: bool,
    pub tank_hp_pct: u8,

    // ── Environment ─────────────────────────────────────────
    pub zone_id: u32,
    pub in_instance: bool,
    pub encounter_active: bool,
    /// Boss HP percent (0..=100). 100 when no encounter or boss unresolved.
    /// Populated from `resolve_boss_handle()` before `update()` runs; read by
    /// execute-phase desires and by `from_beliefs()` for the GOAP `BossBelow*`
    /// atoms.
    pub boss_hp_pct: u8,

    // ── Resources ───────────────────────────────────────────
    pub has_food: bool,
    pub has_drink: bool,
    pub durability_low: bool,
    pub bags_full: bool,

    // ── Derived / inferred ──────────────────────────────────
    pub party_needs_heals: bool,
    pub party_needs_rez: bool,
}

impl Default for BeliefSet {
    fn default() -> Self {
        Self {
            hp_pct: 100,
            mana_pct: 100,
            alive: true,
            in_combat: false,
            mounted: false,
            threat_level: ThreatLevel::Safe,
            attacker_count: 0,
            has_target: false,
            target_hp_pct: 0,
            group_size: 0,
            group_hp_min_pct: 100,
            group_injured_count: 0,
            group_dead_count: 0,
            tank_alive: true,
            healer_alive: true,
            tank_hp_pct: 100,
            zone_id: 0,
            in_instance: false,
            encounter_active: false,
            boss_hp_pct: 100,
            has_food: false,
            has_drink: false,
            durability_low: false,
            bags_full: false,
            party_needs_heals: false,
            party_needs_rez: false,
        }
    }
}

/// Bitfield tracking which belief categories changed since last tick.
/// Only set bits trigger desire re-evaluation for that domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BeliefDiff(pub u16);

impl BeliefDiff {
    pub const NONE: Self = Self(0);
    pub const SELF_VITALS: Self = Self(1 << 0);
    pub const THREAT_CHANGED: Self = Self(1 << 1);
    pub const GROUP_CHANGED: Self = Self(1 << 2);
    pub const COMBAT_CHANGED: Self = Self(1 << 3);
    pub const ENCOUNTER_CHANGED: Self = Self(1 << 4);
    pub const ENVIRONMENT: Self = Self(1 << 5);
    pub const RESOURCES: Self = Self(1 << 6);

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for BeliefDiff {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for BeliefDiff {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// Update beliefs from the current world snapshot and attacker list.
///
/// This runs every tick and must be very cheap (~80ns). It reads fields
/// already loaded on the snapshot — no FFI calls, no heap allocation.
pub fn update(beliefs: &mut BeliefSet, snap: &BotWorldSnapshot, attackers: &[UnitHandle]) {
    let self_ = &snap.self_;
    let max_hp = self_.max_health.max(1);
    let max_mana = self_.max_mana.max(1);

    beliefs.hp_pct = ((self_.health as u64 * 100) / max_hp as u64).min(100) as u8;
    beliefs.mana_pct = ((self_.mana as u64 * 100) / max_mana as u64).min(100) as u8;
    beliefs.alive = self_.is_alive;
    beliefs.in_combat = self_.in_combat;
    beliefs.mounted = self_.is_mounted;

    // Threat assessment based on attacker count and HP
    beliefs.attacker_count = attackers.len().min(255) as u8;
    beliefs.threat_level = if !self_.in_combat || attackers.is_empty() {
        ThreatLevel::Safe
    } else if beliefs.hp_pct < 20 || beliefs.attacker_count > 4 {
        ThreatLevel::Critical
    } else if beliefs.hp_pct < 40 || beliefs.attacker_count > 2 {
        ThreatLevel::High
    } else if beliefs.attacker_count > 1 {
        ThreatLevel::Medium
    } else {
        ThreatLevel::Low
    };

    // Target state
    beliefs.has_target = snap.self_.current_target != 0;
    // target_hp_pct is populated by populate_group_beliefs() in tick.rs
    // which runs before this function and has access to the World.

    // Group size from snapshot; per-member fields (group_hp_min_pct,
    // group_injured_count, group_dead_count, tank_alive, healer_alive)
    // are populated by populate_group_beliefs() in tick.rs.
    beliefs.group_size = snap.group_size;

    // Environment
    beliefs.zone_id = snap.zone_id;
    beliefs.in_instance = !snap.is_overworld;
    // encounter_active is set externally by tick.rs based on encounter FSM

    // Resources — durability and bag space from snapshot
    beliefs.durability_low = snap.durability_pct < 25;
    beliefs.bags_full = snap.bag_space_pct > 90;

    // Derived
    beliefs.party_needs_heals = beliefs.group_injured_count > 0;
    beliefs.party_needs_rez = beliefs.group_dead_count > 0;
}

/// Compute which belief categories changed between two belief snapshots.
///
/// Returns a `BeliefDiff` bitfield. The BDI evaluator only re-scores
/// desires for categories that changed.
pub fn diff(current: &BeliefSet, prev: &BeliefSet) -> BeliefDiff {
    let mut d = BeliefDiff::NONE;

    // Significant HP/mana change (>5% delta)
    if current.hp_pct.abs_diff(prev.hp_pct) > 5
        || current.mana_pct.abs_diff(prev.mana_pct) > 5
        || current.alive != prev.alive
    {
        d |= BeliefDiff::SELF_VITALS;
    }

    if current.threat_level != prev.threat_level
        || current.attacker_count != prev.attacker_count
    {
        d |= BeliefDiff::THREAT_CHANGED;
    }

    if current.group_hp_min_pct.abs_diff(prev.group_hp_min_pct) > 5
        || current.group_injured_count != prev.group_injured_count
        || current.group_dead_count != prev.group_dead_count
        || current.group_size != prev.group_size
        || current.tank_alive != prev.tank_alive
    {
        d |= BeliefDiff::GROUP_CHANGED;
    }

    if current.in_combat != prev.in_combat {
        d |= BeliefDiff::COMBAT_CHANGED;
    }

    if current.encounter_active != prev.encounter_active
        || current.boss_hp_pct.abs_diff(prev.boss_hp_pct) > 5
    {
        d |= BeliefDiff::ENCOUNTER_CHANGED;
    }

    if current.zone_id != prev.zone_id || current.in_instance != prev.in_instance {
        d |= BeliefDiff::ENVIRONMENT;
    }

    if current.durability_low != prev.durability_low
        || current.bags_full != prev.bags_full
        || current.has_food != prev.has_food
        || current.has_drink != prev.has_drink
    {
        d |= BeliefDiff::RESOURCES;
    }

    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_beliefs_are_safe() {
        let b = BeliefSet::default();
        assert_eq!(b.threat_level, ThreatLevel::Safe);
        assert_eq!(b.hp_pct, 100);
        assert!(b.alive);
    }

    #[test]
    fn diff_detects_hp_change() {
        let a = BeliefSet::default();
        let mut b = BeliefSet::default();
        b.hp_pct = 97; // within 5% threshold
        assert!(diff(&a, &b).is_empty());

        b.hp_pct = 80; // exceeds 5% threshold (delta = 20)
        let d = diff(&a, &b);
        assert!(d.contains(BeliefDiff::SELF_VITALS));
    }

    #[test]
    fn diff_detects_combat_change() {
        let mut a = BeliefSet::default();
        let b = BeliefSet::default();
        a.in_combat = true;
        let d = diff(&a, &b);
        assert!(d.contains(BeliefDiff::COMBAT_CHANGED));
    }

    #[test]
    fn empty_diff_when_identical() {
        let a = BeliefSet::default();
        assert!(diff(&a, &a).is_empty());
    }
}
