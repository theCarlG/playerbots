/// GOAP world state — 64-bit bitfield of boolean facts.
///
/// Each bit is a predicate about the world. GOAP actions declare which
/// bits they require (preconditions) and which bits they set/clear
/// (effects). The A* planner searches from the current state to the
/// goal state using these masks.
///
/// 64 bits gives us room for ~64 atoms. This is plenty — GOAP plans
/// are intentionally abstract (5-8 steps), not spell-level granular.

/// A single world-state predicate.
///
/// Each variant maps to a bit position in `WorldState`. Keep in sync
/// with the `bit()` method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Atom {
    // ── Target state ────────────────────────────────────────
    HasTarget = 0,
    TargetInMeleeRange = 1,
    TargetInSpellRange = 2,
    TargetDead = 3,

    // ── Self state ──────────────────────────────────────────
    SelfAlive = 4,
    SelfHealthy = 5,
    SelfHasMana = 6,
    SelfMounted = 7,

    // ── Combat state ────────────────────────────────────────
    InCombat = 8,
    ThreatSafe = 9,
    CcApplied = 10,
    InterruptReady = 11,

    // ── Group state ─────────────────────────────────────────
    GroupHealthy = 12,
    GroupBuffed = 13,
    GroupDeadRezzed = 14,
    GroupDebuffsCleansed = 15,

    // ── Positioning ─────────────────────────────────────────
    Positioned = 16,
    BossTargeted = 17,
    AddControlled = 18,
    AllyProtected = 19,

    // ── Maintenance ─────────────────────────────────────────
    ResourcesRecovered = 20,
    GearRepaired = 21,

    // ── Navigation ──────────────────────────────────────────
    FollowingLeader = 22,
    AtTravelDest = 23,

    // ── Encounter ───────────────────────────────────────────
    EncounterDutyDone = 24,
    MechanicPositioned = 25,
}

/// 64-bit bitfield of boolean world-state facts.
///
/// Zero-cost to copy, hash, and compare. The A* planner operates
/// entirely on these bitfields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct WorldState(pub u64);

impl WorldState {
    /// Create a world state with a single atom set.
    pub const fn with(atom: Atom) -> Self {
        Self(1u64 << atom as u8)
    }

    /// Set an atom in the world state.
    pub fn set(&mut self, atom: Atom) {
        self.0 |= 1u64 << atom as u8;
    }

    /// Clear an atom in the world state.
    pub fn clear(&mut self, atom: Atom) {
        self.0 &= !(1u64 << atom as u8);
    }

    /// Test if an atom is set.
    pub fn has(self, atom: Atom) -> bool {
        self.0 & (1u64 << atom as u8) != 0
    }

    /// Count how many bits differ between self and goal (unsatisfied goals).
    /// Used as the A* heuristic.
    pub fn unsatisfied_count(self, goal: Self) -> u32 {
        // Bits that are set in goal but not in self
        let missing = goal.0 & !self.0;
        missing.count_ones()
    }

    /// Apply effects: set `to_set` bits, clear `to_clear` bits.
    pub fn apply(self, to_set: u64, to_clear: u64) -> Self {
        Self((self.0 | to_set) & !to_clear)
    }

    /// Check if all precondition bits are satisfied.
    /// `required_set`: bits that must be 1. `required_clear`: bits that must be 0.
    pub fn satisfies(self, required_set: u64, required_clear: u64) -> bool {
        (self.0 & required_set) == required_set && (self.0 & required_clear) == 0
    }
}

impl std::ops::BitOr for WorldState {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// Build a `WorldState` from the current `BeliefSet`.
///
/// This translates structured beliefs into the flat bitfield that
/// GOAP operates on. Only called when GOAP needs to plan (not every tick).
pub fn from_beliefs(beliefs: &crate::bdi::beliefs::BeliefSet) -> WorldState {
    let mut ws = WorldState::default();

    if beliefs.alive {
        ws.set(Atom::SelfAlive);
    }
    if beliefs.hp_pct > 50 {
        ws.set(Atom::SelfHealthy);
    }
    if beliefs.mana_pct > 20 {
        ws.set(Atom::SelfHasMana);
    }
    if beliefs.mounted {
        ws.set(Atom::SelfMounted);
    }
    if beliefs.in_combat {
        ws.set(Atom::InCombat);
    }
    if beliefs.has_target {
        ws.set(Atom::HasTarget);
    }
    if beliefs.group_injured_count == 0 && beliefs.group_size > 0 {
        ws.set(Atom::GroupHealthy);
    }
    if beliefs.group_dead_count == 0 {
        ws.set(Atom::GroupDeadRezzed);
    }
    if beliefs.threat_level as u8 <= crate::bdi::beliefs::ThreatLevel::Low as u8 {
        ws.set(Atom::ThreatSafe);
    }

    ws
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_atom() {
        let ws = WorldState::with(Atom::SelfAlive);
        assert!(ws.has(Atom::SelfAlive));
        assert!(!ws.has(Atom::InCombat));
    }

    #[test]
    fn set_and_clear() {
        let mut ws = WorldState::default();
        ws.set(Atom::HasTarget);
        assert!(ws.has(Atom::HasTarget));
        ws.clear(Atom::HasTarget);
        assert!(!ws.has(Atom::HasTarget));
    }

    #[test]
    fn unsatisfied_count() {
        let current = WorldState::with(Atom::SelfAlive);
        let goal = WorldState::with(Atom::SelfAlive) | WorldState::with(Atom::TargetDead);
        assert_eq!(current.unsatisfied_count(goal), 1);
    }

    #[test]
    fn apply_effects() {
        let ws = WorldState::with(Atom::SelfAlive);
        let result = ws.apply(
            1u64 << Atom::HasTarget as u8,   // set HasTarget
            1u64 << Atom::SelfAlive as u8,   // clear SelfAlive
        );
        assert!(result.has(Atom::HasTarget));
        assert!(!result.has(Atom::SelfAlive));
    }

    #[test]
    fn satisfies_preconditions() {
        let ws = WorldState::with(Atom::SelfAlive) | WorldState::with(Atom::HasTarget);
        let req_set = (1u64 << Atom::SelfAlive as u8) | (1u64 << Atom::HasTarget as u8);
        let req_clear = 1u64 << Atom::InCombat as u8;
        assert!(ws.satisfies(req_set, req_clear));
    }
}
