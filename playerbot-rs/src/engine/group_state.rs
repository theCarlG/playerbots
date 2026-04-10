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

/// Encounter-specific role assignments — a per-tick snapshot of every
/// group member classified by role and (for DPS) by range category.
///
/// Populated in `bot::tick::sync_encounter_to_group` by iterating
/// `snap.group_members` and asking the interface for each member's
/// role + class. Every bot in the group runs this rebuild; because
/// the input is the same for everyone, they converge on the same
/// roster without coordination.
///
/// Rosters contain members regardless of alive status — the
/// classification is a stable property of each bot, and consumers
/// that need live/dead filtering (e.g., `tick_resurrect` picking who
/// to rez first) check `is_alive` at read time.
#[derive(Debug, Clone, Default)]
pub struct EncounterAssignments {
    pub main_tank: Option<UnitHandle>,
    pub off_tank: Option<UnitHandle>,
    pub healers: Vec<UnitHandle>,
    pub ranged_dps: Vec<UnitHandle>,
    pub melee_dps: Vec<UnitHandle>,
    /// Special roles keyed by an encounter-specific string (e.g. "`mc_breaker`", "`polarity_switch`").
    /// Command-driven — `rebuild` does NOT touch this field.
    pub special: Vec<(String, UnitHandle)>,
}

impl EncounterAssignments {
    pub fn get_special(&self, role: &str) -> Option<UnitHandle> {
        self.special
            .iter()
            .find(|(r, _)| r == role)
            .map(|(_, h)| *h)
    }

    /// Set or update a special encounter-specific assignment.
    /// Used by command handlers (e.g. `@bot assign mc_breaker`).
    pub fn set_special(&mut self, role: &str, handle: UnitHandle) {
        if let Some(slot) = self.special.iter_mut().find(|(r, _)| r == role) {
            slot.1 = handle;
            return;
        }
        self.special.push((role.to_string(), handle));
    }

    /// Clear a specific special-role assignment.
    pub fn clear_special(&mut self, role: &str) {
        self.special.retain(|(r, _)| r != role);
    }

    /// Rebuild the tank/heal/dps rosters from an iterator of
    /// `(handle, class_id, role_bits)` tuples. `special` is left
    /// untouched — it is command-driven and persists across ticks.
    ///
    /// The first tank encountered becomes `main_tank`; the second
    /// becomes `off_tank`. Additional tanks are ignored (raid setups
    /// with > 2 tanks use the `special` slot or the per-bot
    /// `tank_focus_targets` multi-select).
    ///
    /// DPS members are split by class: `Hunter`, `Priest`, `Mage`,
    /// `Warlock` go into `ranged_dps`; everyone else goes into
    /// `melee_dps`. The split is deliberately coarse — hybrid classes
    /// like Shaman/Druid can be either, but we don't know the spec
    /// at this layer, so we default to melee for ambiguous cases.
    pub fn rebuild<I>(&mut self, members: I)
    where
        I: IntoIterator<Item = (UnitHandle, u8, u8)>,
    {
        self.main_tank = None;
        self.off_tank = None;
        self.healers.clear();
        self.ranged_dps.clear();
        self.melee_dps.clear();

        for (handle, class_id, role_bits) in members {
            if handle == 0 {
                continue;
            }
            let role = BotRole(role_bits);
            if role.is_tank() {
                if self.main_tank.is_none() {
                    self.main_tank = Some(handle);
                } else if self.off_tank.is_none() && self.main_tank != Some(handle) {
                    self.off_tank = Some(handle);
                }
                // Fall through — a tank/offspec bot with both tank
                // and DPS bits can appear in melee_dps too, but we
                // skip that here to keep rosters clean.
                continue;
            }
            if role.is_heal() {
                self.healers.push(handle);
                continue;
            }
            if role.is_dps() {
                if is_ranged_dps_class(class_id) {
                    self.ranged_dps.push(handle);
                } else {
                    self.melee_dps.push(handle);
                }
            }
        }
    }

    /// Iterate every classified member in rez priority order:
    /// healers first (a dead healer is the most urgent rez target
    /// because everyone else will die without heals), then the
    /// main tank and off-tank, then ranged DPS, then melee DPS.
    /// `special` assignments are not included — those are covered
    /// by the canonical rosters.
    ///
    /// Dead-vs-alive filtering is the caller's responsibility; the
    /// roster contains every classified member regardless of state
    /// so rez logic can hit `get_unit_snapshot` and pick the first
    /// actually-dead entry.
    pub fn rez_priority_order(&self) -> impl Iterator<Item = UnitHandle> + '_ {
        self.healers
            .iter()
            .copied()
            .chain(self.main_tank.into_iter())
            .chain(self.off_tank.into_iter())
            .chain(self.ranged_dps.iter().copied())
            .chain(self.melee_dps.iter().copied())
    }
}

/// Classify a class id as ranged DPS for the purpose of splitting
/// the encounter roster. Hunters, Mages, Warlocks and Priests are
/// always ranged; everyone else defaults to melee. Hybrid classes
/// (Shaman, Druid, Paladin) pick their bucket based on spec, but
/// without spec data at this layer we bucket them as melee —
/// encounter BTs that need the real split can query the bot's
/// class prefs or use per-bot positioning.
fn is_ranged_dps_class(class_id: u8) -> bool {
    // PlayerClass discriminants: Hunter=3, Priest=5, Mage=8, Warlock=9.
    matches!(class_id, 3 | 5 | 8 | 9)
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

    /// Returns the RTI icons (0-based, 0..=7) assigned to `tank` as focus targets.
    /// The returned slice is written into `out` and the count is returned.
    pub fn tank_target_icons(&self, tank: UnitHandle, out: &mut [u8; 8]) -> usize {
        let mut n = 0;
        for &(h, icon) in &self.tank_focus_targets {
            if h == tank && h != 0 {
                out[n] = icon;
                n += 1;
            }
        }
        n
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

/// Per-bot desire entry for group coordination.
/// Each bot publishes its current BDI desire so groupmates can avoid
/// duplicating work (e.g., two healers both choosing HealGroup).
#[derive(Debug, Clone, Copy, Default)]
pub struct MemberDesire {
    /// Bot handle (0 = empty slot).
    pub handle: UnitHandle,
    /// Current active desire (DesireKind discriminant as u8).
    pub desire: u8,
}

/// Maximum group members we track desires for.
const MAX_MEMBER_DESIRES: usize = 40;

/// Tracks which healer is attending to which friendly target.
///
/// Soft coordination — multiple healers CAN target the same ally (e.g.,
/// tank taking heavy damage). This informs target selection so healers
/// naturally spread across injured allies instead of all picking the
/// same lowest-HP target.
///
/// Auto-expires after `HEAL_ASSIGNMENT_TTL_MS` if not refreshed.
#[derive(Debug, Clone, Copy, Default)]
pub struct HealAssignment {
    /// Healer bot handle (0 = empty slot).
    pub healer: UnitHandle,
    /// The friendly target being healed.
    pub target: UnitHandle,
    /// Server time (ms) when this was last refreshed.
    pub updated_ms: u64,
}

/// How long a heal assignment lives before auto-expiring (ms).
/// Healers refresh each tick while actively healing. A dead or
/// interrupted healer's assignment expires naturally.
const HEAL_ASSIGNMENT_TTL_MS: u64 = 3_000;

/// Maximum concurrent heal assignments tracked.
/// 10 covers the most healer-heavy 40-man compositions.
const MAX_HEAL_ASSIGNMENTS: usize = 10;

/// State shared across all bots in one group/raid.
#[derive(Debug)]
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
    /// Each bot's current BDI desire. Used by desire scoring to avoid
    /// duplicating roles (e.g., too many healers on HealGroup when one
    /// should DispelDebuffs instead).
    pub member_desires: [MemberDesire; MAX_MEMBER_DESIRES],
    /// Heal target tracking — which healer is attending to which ally.
    /// Healers publish their target here so others spread across
    /// injured allies instead of all picking the same one.
    pub heal_assignments: [HealAssignment; MAX_HEAL_ASSIGNMENTS],
}

impl Default for GroupState {
    fn default() -> Self {
        Self {
            assignments: EncounterAssignments::default(),
            last_computed_ms: 0,
            encounter_active: false,
            encounter: SharedEncounterState::default(),
            coordination: GroupCoordination::default(),
            member_desires: [MemberDesire::default(); MAX_MEMBER_DESIRES],
            heal_assignments: [HealAssignment::default(); MAX_HEAL_ASSIGNMENTS],
        }
    }
}

impl GroupState {
    pub fn is_stale(&self, now_ms: u64, threshold_ms: u64) -> bool {
        now_ms.saturating_sub(self.last_computed_ms) > threshold_ms
    }

    /// Publish this bot's current desire to the shared member_desires table.
    pub fn publish_desire(&mut self, bot_handle: UnitHandle, desire: u8) {
        // Update existing slot or take first empty one.
        if let Some(slot) = self.member_desires.iter_mut().find(|m| m.handle == bot_handle) {
            slot.desire = desire;
            return;
        }
        if let Some(slot) = self.member_desires.iter_mut().find(|m| m.handle == 0) {
            *slot = MemberDesire { handle: bot_handle, desire };
        }
    }

    /// Count how many group members currently have a given desire active.
    pub fn count_with_desire(&self, desire: u8) -> u8 {
        self.member_desires.iter()
            .filter(|m| m.handle != 0 && m.desire == desire)
            .count()
            .min(255) as u8
    }

    // ── Heal target coordination ────────────────────────────

    /// Publish (or refresh) this healer's current heal target.
    ///
    /// Called each tick while a healer is actively healing someone.
    /// If the healer already has a slot, it's updated in place.
    /// Otherwise the first empty or expired slot is taken.
    pub fn publish_heal_target(
        &mut self,
        healer: UnitHandle,
        target: UnitHandle,
        now_ms: u64,
    ) {
        // Update existing slot for this healer.
        if let Some(slot) = self.heal_assignments.iter_mut().find(|a| a.healer == healer) {
            slot.target = target;
            slot.updated_ms = now_ms;
            return;
        }
        // Take first empty slot.
        if let Some(slot) = self.heal_assignments.iter_mut().find(|a| a.healer == 0) {
            *slot = HealAssignment { healer, target, updated_ms: now_ms };
            return;
        }
        // Evict first expired slot.
        if let Some(slot) = self
            .heal_assignments
            .iter_mut()
            .find(|a| now_ms.saturating_sub(a.updated_ms) >= HEAL_ASSIGNMENT_TTL_MS)
        {
            *slot = HealAssignment { healer, target, updated_ms: now_ms };
        }
    }

    /// Clear this healer's heal assignment (stopped healing / switched to DPS / died).
    pub fn clear_heal_target(&mut self, healer: UnitHandle) {
        if let Some(slot) = self.heal_assignments.iter_mut().find(|a| a.healer == healer) {
            *slot = HealAssignment::default();
        }
    }

    /// Count how many other healers (excluding `me`) are actively targeting `target`.
    ///
    /// Used by heal target selection: prefer allies with fewer healers
    /// already assigned. Returns 0 for targets nobody else is healing.
    pub fn healers_on_target(&self, me: UnitHandle, target: UnitHandle, now_ms: u64) -> u8 {
        self.heal_assignments
            .iter()
            .filter(|a| {
                a.healer != 0
                    && a.healer != me
                    && a.target == target
                    && now_ms.saturating_sub(a.updated_ms) < HEAL_ASSIGNMENT_TTL_MS
            })
            .count()
            .min(255) as u8
    }

    /// Check if any other healer (excluding `me`) is already targeting `target`.
    pub fn is_heal_target_covered(&self, me: UnitHandle, target: UnitHandle, now_ms: u64) -> bool {
        self.healers_on_target(me, target, now_ms) > 0
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

    // ── Heal assignment tracking tests ─────────────────────

    const HEALER_1: UnitHandle = 0xEE01;
    const HEALER_2: UnitHandle = 0xEE02;
    const HEALER_3: UnitHandle = 0xEE03;
    const ALLY_1: UnitHandle = 0xAA01;
    const ALLY_2: UnitHandle = 0xAA02;

    #[test]
    fn publish_and_query_heal_target() {
        let mut gs = GroupState::default();
        gs.publish_heal_target(HEALER_1, ALLY_1, 1000);

        assert_eq!(gs.healers_on_target(HEALER_2, ALLY_1, 1000), 1);
        assert_eq!(gs.healers_on_target(HEALER_1, ALLY_1, 1000), 0); // excludes self
        assert!(gs.is_heal_target_covered(HEALER_2, ALLY_1, 1000));
        assert!(!gs.is_heal_target_covered(HEALER_2, ALLY_2, 1000)); // nobody on ALLY_2
    }

    #[test]
    fn multiple_healers_on_same_target() {
        let mut gs = GroupState::default();
        gs.publish_heal_target(HEALER_1, ALLY_1, 1000);
        gs.publish_heal_target(HEALER_2, ALLY_1, 1000);

        // HEALER_3 sees 2 others on ALLY_1
        assert_eq!(gs.healers_on_target(HEALER_3, ALLY_1, 1000), 2);
    }

    #[test]
    fn heal_assignment_expires() {
        let mut gs = GroupState::default();
        gs.publish_heal_target(HEALER_1, ALLY_1, 1000);

        // Still active at 3999ms (1000 + 3000 - 1)
        assert_eq!(gs.healers_on_target(HEALER_2, ALLY_1, 3999), 1);
        // Expired at 4000ms (1000 + HEAL_ASSIGNMENT_TTL_MS)
        assert_eq!(gs.healers_on_target(HEALER_2, ALLY_1, 4000), 0);
    }

    #[test]
    fn refresh_heal_target_extends_lifetime() {
        let mut gs = GroupState::default();
        gs.publish_heal_target(HEALER_1, ALLY_1, 1000);
        // Refresh at 2000ms
        gs.publish_heal_target(HEALER_1, ALLY_1, 2000);

        // Still alive at 4999ms (2000 + 3000 - 1)
        assert_eq!(gs.healers_on_target(HEALER_2, ALLY_1, 4999), 1);
        // Expired at 5000ms
        assert_eq!(gs.healers_on_target(HEALER_2, ALLY_1, 5000), 0);
    }

    #[test]
    fn switch_heal_target() {
        let mut gs = GroupState::default();
        gs.publish_heal_target(HEALER_1, ALLY_1, 1000);
        // Switch to ALLY_2
        gs.publish_heal_target(HEALER_1, ALLY_2, 1500);

        assert_eq!(gs.healers_on_target(HEALER_2, ALLY_1, 1500), 0); // no longer on ALLY_1
        assert_eq!(gs.healers_on_target(HEALER_2, ALLY_2, 1500), 1); // now on ALLY_2
    }

    #[test]
    fn clear_heal_target() {
        let mut gs = GroupState::default();
        gs.publish_heal_target(HEALER_1, ALLY_1, 1000);
        gs.clear_heal_target(HEALER_1);

        assert_eq!(gs.healers_on_target(HEALER_2, ALLY_1, 1000), 0);
    }

    #[test]
    fn expired_slot_reused() {
        let mut gs = GroupState::default();
        // Fill all 10 slots
        for i in 0..10u64 {
            gs.publish_heal_target(0x1000 + i, ALLY_1, 0);
        }
        // All expired at 3000ms — new assignment should evict one
        gs.publish_heal_target(HEALER_1, ALLY_2, 3000);
        assert_eq!(gs.healers_on_target(HEALER_2, ALLY_2, 3000), 1);
    }

    // ── EncounterAssignments rebuild/rez ordering ──────────

    // Class discriminants: Warrior=1, Paladin=2, Hunter=3, Rogue=4,
    // Priest=5, Shaman=7, Mage=8, Warlock=9, Druid=11.
    const TANK_WARRIOR: UnitHandle = 0xB001;
    const TANK_DRUID: UnitHandle = 0xB002;
    const HEALER_PRIEST: UnitHandle = 0xB101;
    const HEALER_PALADIN: UnitHandle = 0xB102;
    const DPS_HUNTER: UnitHandle = 0xB201;
    const DPS_MAGE: UnitHandle = 0xB202;
    const DPS_WARLOCK: UnitHandle = 0xB203;
    const DPS_ROGUE: UnitHandle = 0xB204;
    const DPS_SHAMAN: UnitHandle = 0xB205;

    #[test]
    fn rebuild_classifies_tanks_healers_and_dps_by_range() {
        let mut ea = EncounterAssignments::default();
        ea.rebuild([
            (TANK_WARRIOR, 1, BotRole::TANK.0),
            (HEALER_PRIEST, 5, BotRole::HEAL.0),
            (DPS_HUNTER, 3, BotRole::DPS.0),
            (DPS_MAGE, 8, BotRole::DPS.0),
            (DPS_WARLOCK, 9, BotRole::DPS.0),
            (DPS_ROGUE, 4, BotRole::DPS.0),
            (DPS_SHAMAN, 7, BotRole::DPS.0),
        ]);

        assert_eq!(ea.main_tank, Some(TANK_WARRIOR));
        assert_eq!(ea.off_tank, None);
        assert_eq!(ea.healers, vec![HEALER_PRIEST]);
        // Hunter, Mage, Warlock → ranged. Priest is healer here so not DPS.
        assert_eq!(ea.ranged_dps, vec![DPS_HUNTER, DPS_MAGE, DPS_WARLOCK]);
        // Rogue and Shaman default to melee (shaman is hybrid, defaults melee).
        assert_eq!(ea.melee_dps, vec![DPS_ROGUE, DPS_SHAMAN]);
    }

    #[test]
    fn rebuild_fills_main_then_off_tank() {
        let mut ea = EncounterAssignments::default();
        ea.rebuild([
            (TANK_WARRIOR, 1, BotRole::TANK.0),
            (TANK_DRUID, 11, BotRole::TANK.0),
            // A 3rd tank is ignored (no room).
            (0xB003, 2, BotRole::TANK.0),
        ]);

        assert_eq!(ea.main_tank, Some(TANK_WARRIOR));
        assert_eq!(ea.off_tank, Some(TANK_DRUID));
    }

    #[test]
    fn rebuild_skips_zero_handles() {
        let mut ea = EncounterAssignments::default();
        ea.rebuild([
            (0, 1, BotRole::TANK.0),
            (TANK_WARRIOR, 1, BotRole::TANK.0),
            (0, 5, BotRole::HEAL.0),
            (HEALER_PRIEST, 5, BotRole::HEAL.0),
        ]);

        assert_eq!(ea.main_tank, Some(TANK_WARRIOR));
        assert_eq!(ea.healers, vec![HEALER_PRIEST]);
    }

    #[test]
    fn rebuild_clears_previous_roster() {
        let mut ea = EncounterAssignments::default();
        ea.rebuild([
            (TANK_WARRIOR, 1, BotRole::TANK.0),
            (HEALER_PRIEST, 5, BotRole::HEAL.0),
            (DPS_HUNTER, 3, BotRole::DPS.0),
        ]);
        // Second rebuild with a new roster should fully replace.
        ea.rebuild([
            (TANK_DRUID, 11, BotRole::TANK.0),
            (HEALER_PALADIN, 2, BotRole::HEAL.0),
        ]);

        assert_eq!(ea.main_tank, Some(TANK_DRUID));
        assert_eq!(ea.off_tank, None);
        assert_eq!(ea.healers, vec![HEALER_PALADIN]);
        assert!(ea.ranged_dps.is_empty());
        assert!(ea.melee_dps.is_empty());
    }

    #[test]
    fn rebuild_preserves_special_assignments() {
        let mut ea = EncounterAssignments::default();
        ea.set_special("mc_breaker", 0xBEEF);
        ea.rebuild([(TANK_WARRIOR, 1, BotRole::TANK.0)]);
        assert_eq!(ea.get_special("mc_breaker"), Some(0xBEEF));
    }

    #[test]
    fn rez_priority_order_lists_healers_then_tanks_then_dps() {
        let mut ea = EncounterAssignments::default();
        ea.rebuild([
            (TANK_WARRIOR, 1, BotRole::TANK.0),
            (TANK_DRUID, 11, BotRole::TANK.0),
            (HEALER_PRIEST, 5, BotRole::HEAL.0),
            (HEALER_PALADIN, 2, BotRole::HEAL.0),
            (DPS_HUNTER, 3, BotRole::DPS.0),
            (DPS_ROGUE, 4, BotRole::DPS.0),
        ]);

        let order: Vec<UnitHandle> = ea.rez_priority_order().collect();
        assert_eq!(
            order,
            vec![
                HEALER_PRIEST,  // healers first
                HEALER_PALADIN,
                TANK_WARRIOR,   // then main tank
                TANK_DRUID,     // then off-tank
                DPS_HUNTER,     // ranged DPS
                DPS_ROGUE,      // melee DPS
            ]
        );
    }

    #[test]
    fn rez_priority_order_handles_empty_roster() {
        let ea = EncounterAssignments::default();
        assert_eq!(ea.rez_priority_order().count(), 0);
    }

    #[test]
    fn special_set_get_and_clear() {
        let mut ea = EncounterAssignments::default();
        assert_eq!(ea.get_special("polarity"), None);

        ea.set_special("polarity", 0xAA);
        assert_eq!(ea.get_special("polarity"), Some(0xAA));

        // Update existing role reuses slot instead of appending.
        ea.set_special("polarity", 0xBB);
        assert_eq!(ea.get_special("polarity"), Some(0xBB));
        assert_eq!(ea.special.len(), 1);

        ea.set_special("breaker", 0xCC);
        assert_eq!(ea.special.len(), 2);

        ea.clear_special("polarity");
        assert_eq!(ea.get_special("polarity"), None);
        assert_eq!(ea.get_special("breaker"), Some(0xCC));
    }
}
