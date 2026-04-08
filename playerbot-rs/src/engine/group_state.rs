/// Shared group/encounter state — one instance per group, shared via Arc<`RwLock`<>>.
///
/// The group's "coordinator" (leader bot) writes this once per world tick.
/// All other bots in the group read it (lock-free via `try_read` with stale fallback).
use crate::engine::claim::ClaimTable;
use crate::ffi::{SpellId, UnitHandle};

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

/// Shared encounter state visible to every bot in the group.
///
/// The per-bot `EncounterFsm` drives boss phase transitions locally.
/// This shared state is for **coordination** — making sure bots don't
/// duplicate work (via `ClaimTable`) and can read group-wide encounter
/// metadata without polling each other.
#[derive(Debug, Default)]
pub struct SharedEncounterState {
    /// NPC entry ID of the primary boss, if known. 0 = no boss detected.
    pub boss_entry: u32,
    /// Current phase ID (interpretation is per-encounter). 0 = pre-pull.
    pub phase_id: u32,
    /// True when a boss fight is in progress.
    pub active: bool,
    /// Claim table — prevents duplicate heals, CC, rune dousing, etc.
    pub claims: ClaimTable,
}

/// Group-wide coordination data — targeting, tank order, CC assignments.
///
/// This is the "group blackboard" that any bot can read to make coordinated
/// decisions. Written by commands (`@botname set mt`, `@healers assign ...`)
/// and by the encounter coordinator.
#[derive(Debug, Default)]
pub struct GroupCoordination {
    /// The group's main-assist target (all DPS focus this mob).
    /// Set by command or auto-detected from tank's target.
    pub main_assist_target: Option<UnitHandle>,

    /// CC assignments: (spell to use, caster bot, target mob).
    /// Up to 8 concurrent CC assignments (practical raid max).
    /// `None` spell means "use your default CC".
    pub cc_assignments: [(Option<SpellId>, UnitHandle, UnitHandle); 8],

    /// Blessing coordination: which paladin covers which blessing.
    /// (`paladin_handle`, `blessing_spell_id`). Up to 4 paladins.
    pub paladin_blessings: [(UnitHandle, SpellId); 4],

    /// Tank focus target assignments: (bot handle, RTI icon 1..=8).
    /// Up to 8 entries. 0 values = unassigned slot.
    pub tank_focus_targets: [(UnitHandle, u8); 8],

    /// Ordered tank list: [MT, OT1, OT2, OT3]. 0 = unassigned slot.
    /// `tank_order[0]` is always the main tank.
    pub tank_order: [UnitHandle; 4],

    /// Index into `tank_order` for the currently active tank (for tank swaps).
    pub active_tank_idx: u8,

    /// Heal priority list — healers check this to avoid overhealing the same
    /// target. Ordered from highest to lowest priority.
    pub heal_priority: [UnitHandle; 8],
}

impl GroupCoordination {
    /// Returns the main tank (first entry in `tank_order`), if assigned.
    pub fn main_tank(&self) -> Option<UnitHandle> {
        let h = self.tank_order[0];
        if h != 0 { Some(h) } else { None }
    }

    /// Returns the currently active tank (for tank-swap aware logic).
    pub fn active_tank(&self) -> Option<UnitHandle> {
        let idx = self.active_tank_idx as usize;
        if idx < self.tank_order.len() {
            let h = self.tank_order[idx];
            if h != 0 { Some(h) } else { None }
        } else {
            None
        }
    }

    /// Returns all assigned off-tanks (`tank_order`[1..] where non-zero).
    pub fn off_tanks(&self) -> impl Iterator<Item = UnitHandle> + '_ {
        self.tank_order[1..].iter().copied().filter(|h| *h != 0)
    }

    /// Check if a unit is assigned as CC target by anyone.
    pub fn is_cc_target(&self, target: UnitHandle) -> bool {
        self.cc_assignments
            .iter()
            .any(|(_, _, t)| *t == target && target != 0)
    }

    /// Set the main tank (slot 0). Removes the handle from other slots first.
    pub fn set_main_tank(&mut self, handle: UnitHandle) {
        self.remove_tank(handle);
        self.tank_order[0] = handle;
        self.active_tank_idx = 0;
    }

    /// Add an off-tank in the first available slot (1..3).
    /// Removes the handle from other slots first to avoid duplicates.
    pub fn add_off_tank(&mut self, handle: UnitHandle) {
        self.remove_tank(handle);
        for slot in &mut self.tank_order[1..] {
            if *slot == 0 {
                *slot = handle;
                return;
            }
        }
    }

    /// Remove a handle from all tank slots.
    pub fn remove_tank(&mut self, handle: UnitHandle) {
        for slot in &mut self.tank_order {
            if *slot == handle {
                *slot = 0;
            }
        }
    }

    /// Find CC assignment for a specific caster bot.
    pub fn cc_for_bot(&self, bot: UnitHandle) -> Option<(Option<SpellId>, UnitHandle)> {
        self.cc_assignments
            .iter()
            .find(|(_, caster, _)| *caster == bot && *caster != 0)
            .map(|(spell, _, target)| (*spell, *target))
    }

    /// Assign a CC duty: bot `caster` should CC the mob marked with RTI `icon`.
    /// Stores the icon as the target field (the encounter BT resolves it to
    /// a real `UnitHandle` at runtime since `get_rti_target` FFI is not yet
    /// available). Reuses the bot's existing slot if present, otherwise takes
    /// the first empty slot.
    pub fn assign_cc(&mut self, caster: UnitHandle, icon: u8, spell: Option<SpellId>) {
        // Icon is stored in the target field as a small sentinel (1..=8).
        let target = icon as UnitHandle;
        // Reuse existing slot for this caster.
        if let Some(slot) = self
            .cc_assignments
            .iter_mut()
            .find(|(_, c, _)| *c == caster && *c != 0)
        {
            *slot = (spell, caster, target);
            return;
        }
        // Find first empty slot.
        if let Some(slot) = self.cc_assignments.iter_mut().find(|(_, c, _)| *c == 0) {
            *slot = (spell, caster, target);
        }
    }

    /// Remove CC assignment(s) for `caster`. If `icon` is Some, only remove
    /// the assignment matching that icon; otherwise remove all for this caster.
    pub fn unassign_cc(&mut self, caster: UnitHandle, icon: Option<u8>) {
        for slot in &mut self.cc_assignments {
            if slot.1 == caster && caster != 0 {
                if let Some(i) = icon
                    && slot.2 != i as UnitHandle {
                        continue;
                    }
                *slot = (None, 0, 0);
            }
        }
    }

    /// Assign a tank focus target: bot `tank` should tank the mob marked with
    /// RTI `icon`. Reuses the bot's existing slot for that icon, otherwise
    /// takes the first empty slot.
    pub fn assign_tank_target(&mut self, tank: UnitHandle, icon: u8) {
        // Check if this bot already has this icon assigned.
        if self
            .tank_focus_targets
            .iter()
            .any(|(h, i)| *h == tank && *i == icon && *h != 0)
        {
            return; // already assigned
        }
        // Find first empty slot.
        if let Some(slot) = self.tank_focus_targets.iter_mut().find(|(h, _)| *h == 0) {
            *slot = (tank, icon);
        }
    }

    /// Remove tank focus target(s) for `tank`. If `icon` is Some, only remove
    /// the entry for that icon; otherwise remove all entries for this tank.
    pub fn unassign_tank_target(&mut self, tank: UnitHandle, icon: Option<u8>) {
        for slot in &mut self.tank_focus_targets {
            if slot.0 == tank && tank != 0 {
                if let Some(i) = icon
                    && slot.1 != i {
                        continue;
                    }
                *slot = (0, 0);
            }
        }
    }

    /// Set a paladin's blessing in the coordination table. Reuses the
    /// paladin's existing slot or takes the first empty one.
    pub fn set_paladin_blessing(&mut self, paladin: UnitHandle, spell: SpellId) {
        if let Some(slot) = self
            .paladin_blessings
            .iter_mut()
            .find(|(h, _)| *h == paladin)
        {
            slot.1 = spell;
            return;
        }
        if let Some(slot) = self.paladin_blessings.iter_mut().find(|(h, _)| *h == 0) {
            *slot = (paladin, spell);
        }
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
    /// Shared encounter coordination state (claims, boss tracking).
    pub encounter: SharedEncounterState,
    /// Group-wide coordination (targeting, tank order, CC, blessings).
    pub coordination: GroupCoordination,
}

impl GroupState {
    pub fn is_stale(&self, now_ms: u64, threshold_ms: u64) -> bool {
        now_ms.saturating_sub(self.last_computed_ms) > threshold_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_main_tank() {
        let mut c = GroupCoordination::default();
        c.set_main_tank(100);
        assert_eq!(c.main_tank(), Some(100));
        assert_eq!(c.active_tank(), Some(100));
    }

    #[test]
    fn set_main_tank_clears_from_ot_slots() {
        let mut c = GroupCoordination::default();
        c.add_off_tank(100);
        assert!(c.off_tanks().any(|h| h == 100));

        // Promoting to MT should clear the OT slot.
        c.set_main_tank(100);
        assert_eq!(c.main_tank(), Some(100));
        assert!(!c.off_tanks().any(|h| h == 100));
    }

    #[test]
    fn add_off_tanks_fills_slots() {
        let mut c = GroupCoordination::default();
        c.add_off_tank(200);
        c.add_off_tank(300);
        c.add_off_tank(400);
        let ots: Vec<_> = c.off_tanks().collect();
        assert_eq!(ots, vec![200, 300, 400]);
    }

    #[test]
    fn add_off_tank_no_duplicate() {
        let mut c = GroupCoordination::default();
        c.add_off_tank(200);
        c.add_off_tank(200); // should not create duplicate
        let ots: Vec<_> = c.off_tanks().collect();
        assert_eq!(ots, vec![200]);
    }

    #[test]
    fn off_tank_slots_full() {
        let mut c = GroupCoordination::default();
        c.add_off_tank(200);
        c.add_off_tank(300);
        c.add_off_tank(400);
        c.add_off_tank(500); // no room — silently ignored
        let ots: Vec<_> = c.off_tanks().collect();
        assert_eq!(ots, vec![200, 300, 400]);
    }

    #[test]
    fn remove_tank() {
        let mut c = GroupCoordination::default();
        c.set_main_tank(100);
        c.add_off_tank(200);
        c.remove_tank(100);
        assert_eq!(c.main_tank(), None);
        assert!(c.off_tanks().any(|h| h == 200));
    }

    #[test]
    fn cc_assignment_lookup() {
        let mut c = GroupCoordination::default();
        c.cc_assignments[0] = (Some(SpellId(118)), 50, 999); // mage 50 CCs mob 999
        assert!(c.is_cc_target(999));
        assert!(!c.is_cc_target(888));
        assert_eq!(c.cc_for_bot(50), Some((Some(SpellId(118)), 999)));
        assert_eq!(c.cc_for_bot(60), None);
    }

    #[test]
    fn assign_cc_via_method() {
        let mut c = GroupCoordination::default();
        c.assign_cc(50, 8, Some(SpellId(118))); // mage 50 CCs icon 8 (skull)
        // Icon stored as UnitHandle sentinel.
        assert_eq!(c.cc_for_bot(50), Some((Some(SpellId(118)), 8)));
        // Reassign same bot → reuses slot.
        c.assign_cc(50, 5, None); // now CC icon 5 (moon)
        assert_eq!(c.cc_for_bot(50), Some((None, 5)));
        // A second bot takes a new slot.
        c.assign_cc(60, 3, None);
        assert_eq!(c.cc_for_bot(60), Some((None, 3)));
    }

    #[test]
    fn unassign_cc_all() {
        let mut c = GroupCoordination::default();
        c.assign_cc(50, 8, None);
        c.unassign_cc(50, None); // clear all
        assert_eq!(c.cc_for_bot(50), None);
    }

    #[test]
    fn unassign_cc_specific_icon() {
        let mut c = GroupCoordination::default();
        c.assign_cc(50, 8, None);
        c.unassign_cc(50, Some(5)); // icon mismatch — should not clear
        assert!(c.cc_for_bot(50).is_some());
        c.unassign_cc(50, Some(8)); // icon matches — should clear
        assert_eq!(c.cc_for_bot(50), None);
    }

    #[test]
    fn assign_tank_target() {
        let mut c = GroupCoordination::default();
        c.assign_tank_target(100, 8);
        assert_eq!(c.tank_focus_targets[0], (100, 8));
        // Duplicate assignment is a no-op.
        c.assign_tank_target(100, 8);
        assert_eq!(c.tank_focus_targets[1], (0, 0));
        // Same bot, different icon → new slot.
        c.assign_tank_target(100, 5);
        assert_eq!(c.tank_focus_targets[1], (100, 5));
    }

    #[test]
    fn unassign_tank_target_all() {
        let mut c = GroupCoordination::default();
        c.assign_tank_target(100, 8);
        c.assign_tank_target(100, 5);
        c.unassign_tank_target(100, None);
        assert_eq!(c.tank_focus_targets[0], (0, 0));
        assert_eq!(c.tank_focus_targets[1], (0, 0));
    }

    #[test]
    fn unassign_tank_target_specific() {
        let mut c = GroupCoordination::default();
        c.assign_tank_target(100, 8);
        c.assign_tank_target(100, 5);
        c.unassign_tank_target(100, Some(8));
        // Only icon 8 cleared; icon 5 remains.
        assert_eq!(c.tank_focus_targets[0], (0, 0));
        assert_eq!(c.tank_focus_targets[1], (100, 5));
    }

    #[test]
    fn set_paladin_blessing() {
        let mut c = GroupCoordination::default();
        c.set_paladin_blessing(10, SpellId(19740)); // BoM
        assert_eq!(c.paladin_blessings[0], (10, SpellId(19740)));
        // Update same paladin → reuses slot.
        c.set_paladin_blessing(10, SpellId(19742)); // BoW
        assert_eq!(c.paladin_blessings[0], (10, SpellId(19742)));
        // Second paladin → new slot.
        c.set_paladin_blessing(20, SpellId(19740));
        assert_eq!(c.paladin_blessings[1], (20, SpellId(19740)));
    }

    #[test]
    fn heal_priority_rotate() {
        let mut c = GroupCoordination::default();
        c.heal_priority = [10, 20, 30, 0, 0, 0, 0, 0];
        c.set_main_tank(99);
        // After setting MT, heal_priority should put MT first
        // (done by sync_encounter_to_group, tested separately).
        // But we can test the rotate logic directly:
        c.heal_priority.rotate_right(1);
        c.heal_priority[0] = 99;
        assert_eq!(c.heal_priority[0], 99);
        assert_eq!(c.heal_priority[1], 10);
        assert_eq!(c.heal_priority[2], 20);
    }
}
