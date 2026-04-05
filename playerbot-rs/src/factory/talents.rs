//! Factory InitTalents — mirrors `PlayerbotFactory::InitTalents(specNo)`.
//!
//! The C++ source walks `sTalentStore`, keeps rows whose `TalentTab` matches
//! `spec_no` and whose `ClassMask` covers the bot's class, groups them by
//! `Row`, then for each row tries up to 3 random picks: each pick is a
//! `TalentEntry` whose ranks are learned in order until the bot has spent 5
//! points (or run out of free talent points). After every `learnSpell` the
//! C++ code calls `Player::UpdateFreeTalentPoints(false)` to recompute the
//! remaining budget.
//!
//! The DBC walk and the two counter helpers (`GetFreeTalentPoints`,
//! `UpdateFreeTalentPoints(false)`) live in `BotBridge.cpp`; this module
//! owns the grouping, random selection, and per-row spend loop.

use crate::ffi::BotTalentEntry;
use crate::ffi::interface::BotInterface;
use std::collections::BTreeMap;

/// Points spent per row budget (matches the C++ source's inner guard
/// `freePoints - GetFreeTalentPoints() < 5`).
const POINTS_PER_ROW: u32 = 5;
/// Per-row retry budget before moving on (matches C++ `attemptCount < 3`).
const ATTEMPTS_PER_ROW: u32 = 3;

/// Fill the bot's `spec_no` talent tab with randomly-selected talents until
/// free talent points are exhausted.
pub fn init_talents(iface: &dyn BotInterface, spec_no: u32) {
    // `spec_no` is bounded to 0..=2 by the caller, but the C++ source and the
    // FFI both take it as u32 / u8. Clamp defensively.
    let spec = spec_no.min(u8::MAX as u32) as u8;

    let talents = iface.get_class_talents(spec);
    if talents.is_empty() {
        return;
    }

    // Group by row so we can drain rows independently.
    let mut rows: BTreeMap<u32, Vec<BotTalentEntry>> = BTreeMap::new();
    for t in talents {
        rows.entry(t.row).or_default().push(t);
    }

    // The C++ source caches `freePoints` before each row to determine when
    // it has spent 5 points inside that row. We mirror the same loop.
    let mut free_points = iface.bot_free_talent_points();
    for (_row, mut pool) in rows {
        if free_points == 0 {
            break;
        }
        let start_points = free_points;
        let mut attempts: u32 = 0;
        while !pool.is_empty()
            && start_points.saturating_sub(free_points) < POINTS_PER_ROW
            && attempts < ATTEMPTS_PER_ROW
            && free_points > 0
        {
            attempts += 1;
            let idx = iface.random_u32(0, (pool.len() - 1) as u32) as usize;
            let talent = pool.swap_remove(idx);

            for &spell_id in &talent.rank_ids {
                if spell_id == 0 {
                    continue;
                }
                if iface.bot_free_talent_points() == 0 {
                    break;
                }
                iface.bot_learn_spell(spell_id);
                iface.bot_update_free_talent_points();
            }

            free_points = iface.bot_free_talent_points();
        }
    }
}

/// Mirror of `PlayerbotFactory::InitTalentsTree`: pick (or recall) the spec
/// the bot should invest into, spend its points there, and if any points
/// remain, dump them into the complementary tab. The spec-picking state
/// management (sRandomPlayerbotMgr get/set + config roll) is bundled into
/// the single `bot_pick_spec_no` FFI call.
pub fn init_talents_tree(iface: &dyn BotInterface, incremental: bool) {
    let spec_no = iface.bot_pick_spec_no(incremental);
    init_talents(iface, spec_no);

    if iface.bot_free_talent_points() > 0 {
        // Dump any leftover points into the complementary tab. The C++
        // source computes `2 - specNo`; `saturating_sub` keeps us safe if
        // the picker ever returns a value > 2.
        let other = 2u32.saturating_sub(spec_no);
        init_talents(iface, other);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::BotTalentEntry;
    use crate::ffi::interface::BotInterface;
    use crate::ffi::types::{ItemId, SpellId};
    use std::cell::RefCell;

    struct MockIface {
        talents: Vec<BotTalentEntry>,
        free_points: RefCell<u32>,
        // Rank IDs that decrement a talent point when learned.
        talent_spell_ids: std::collections::HashSet<u32>,
        learned: RefCell<Vec<u32>>,
        rand_fixed: u32,
        // Spec the picker should return; `None` leaves the trait default (0).
        picked_spec: Option<u32>,
        pick_calls: RefCell<Vec<bool>>,
    }

    unsafe impl Send for MockIface {}

    impl BotInterface for MockIface {
        fn get_snapshot(&self) -> crate::ffi::BotWorldSnapshot {
            unsafe { std::mem::zeroed() }
        }
        fn get_unit_snapshot(&self, _: crate::ffi::UnitHandle) -> crate::ffi::BotUnitSnapshot {
            unsafe { std::mem::zeroed() }
        }
        fn has_aura(&self, _: crate::ffi::UnitHandle, _: SpellId) -> bool {
            false
        }
        fn get_aura(
            &self,
            _: crate::ffi::UnitHandle,
            _: SpellId,
        ) -> Option<crate::ffi::BotAuraInfo> {
            None
        }
        fn get_auras(&self, _: crate::ffi::UnitHandle) -> Vec<crate::ffi::BotAuraInfo> {
            vec![]
        }
        fn get_threat_list(&self, _: crate::ffi::UnitHandle) -> Vec<crate::ffi::BotThreatEntry> {
            vec![]
        }
        fn get_unit_threat(&self, _: crate::ffi::UnitHandle, _: crate::ffi::UnitHandle) -> f32 {
            0.0
        }
        fn unit_distance(&self, _: crate::ffi::UnitHandle) -> f32 {
            0.0
        }
        fn can_cast(&self, _: SpellId, _: crate::ffi::UnitHandle) -> bool {
            false
        }
        fn spell_cooldown_ms(&self, _: SpellId) -> u32 {
            0
        }
        fn has_los(&self, _: crate::ffi::UnitHandle) -> bool {
            false
        }
        fn get_nearby_units(&self, _: f32, _: bool) -> Vec<crate::ffi::UnitHandle> {
            vec![]
        }
        fn get_behind_position(
            &self,
            _: crate::ffi::UnitHandle,
            _: f32,
        ) -> crate::ffi::BotPosition {
            unsafe { std::mem::zeroed() }
        }
        fn get_safe_position(&self, _: f32) -> Option<crate::ffi::BotPosition> {
            None
        }
        fn get_spread_position(
            &self,
            _: crate::ffi::UnitHandle,
            _: f32,
            _: u8,
            _: u8,
        ) -> crate::ffi::BotPosition {
            unsafe { std::mem::zeroed() }
        }
        fn can_reach(&self, _: f32, _: f32, _: f32) -> bool {
            false
        }
        fn cast_spell(&self, _: SpellId, _: crate::ffi::UnitHandle) -> bool {
            false
        }
        fn cast_spell_pos(&self, _: SpellId, _: f32, _: f32, _: f32) -> bool {
            false
        }
        fn move_to(&self, _: f32, _: f32, _: f32) -> bool {
            false
        }
        fn follow(&self, _: crate::ffi::UnitHandle, _: f32, _: f32) -> bool {
            false
        }
        fn stop_moving(&self) -> bool {
            false
        }
        fn attack(&self, _: crate::ffi::UnitHandle) -> bool {
            false
        }
        fn auto_attack(&self, _: bool) -> bool {
            false
        }
        fn say(&self, _: &str, _: u32) -> bool {
            false
        }
        fn use_item(&self, _: ItemId, _: crate::ffi::UnitHandle) -> bool {
            false
        }
        fn taunt(&self, _: crate::ffi::UnitHandle) -> bool {
            false
        }
        fn group_get_tank(&self) -> Option<crate::ffi::UnitHandle> {
            None
        }
        fn group_get_healer(&self) -> Option<crate::ffi::UnitHandle> {
            None
        }
        fn group_get_role(&self, _: crate::ffi::UnitHandle) -> crate::ffi::BotRole {
            crate::ffi::BotRole::NONE
        }

        fn random_u32(&self, _: u32, _: u32) -> u32 {
            self.rand_fixed
        }
        fn bot_learn_spell(&self, spell_id: u32) {
            self.learned.borrow_mut().push(spell_id);
        }
        fn get_class_talents(&self, _: u8) -> Vec<BotTalentEntry> {
            self.talents.clone()
        }
        fn bot_free_talent_points(&self) -> u32 {
            *self.free_points.borrow()
        }
        fn bot_pick_spec_no(&self, incremental: bool) -> u32 {
            self.pick_calls.borrow_mut().push(incremental);
            self.picked_spec.unwrap_or(0)
        }
        fn bot_update_free_talent_points(&self) {
            // A learned talent rank consumes one point.
            let last = self.learned.borrow().last().copied();
            if let Some(id) = last {
                if self.talent_spell_ids.contains(&id) {
                    let mut fp = self.free_points.borrow_mut();
                    *fp = fp.saturating_sub(1);
                }
            }
        }
    }

    fn talent(row: u32, ranks: [u32; 5]) -> BotTalentEntry {
        BotTalentEntry { row, rank_ids: ranks }
    }

    #[test]
    fn no_talents_is_noop() {
        let m = MockIface {
            talents: vec![],
            free_points: RefCell::new(5),
            talent_spell_ids: Default::default(),
            learned: RefCell::new(Vec::new()),
            rand_fixed: 0,
            picked_spec: None,
            pick_calls: RefCell::new(Vec::new()),
        };
        init_talents(&m, 0);
        assert!(m.learned.borrow().is_empty());
        assert_eq!(*m.free_points.borrow(), 5);
    }

    #[test]
    fn spends_all_points_in_one_row() {
        // Single row with a 5-rank talent; free points = 5 → every rank learned.
        let ranks = [100u32, 101, 102, 103, 104];
        let m = MockIface {
            talents: vec![talent(0, ranks)],
            free_points: RefCell::new(5),
            talent_spell_ids: ranks.iter().copied().collect(),
            learned: RefCell::new(Vec::new()),
            rand_fixed: 0,
            picked_spec: None,
            pick_calls: RefCell::new(Vec::new()),
        };
        init_talents(&m, 0);
        assert_eq!(m.learned.borrow().as_slice(), &ranks);
        assert_eq!(*m.free_points.borrow(), 0);
    }

    #[test]
    fn stops_learning_when_budget_zero() {
        // Two rows with 5-rank talents, but only 3 points available.
        let row0 = [200u32, 201, 202, 203, 204];
        let row1 = [300u32, 301, 302, 303, 304];
        let mut ids: std::collections::HashSet<u32> = row0.iter().copied().collect();
        ids.extend(row1);
        let m = MockIface {
            talents: vec![talent(0, row0), talent(1, row1)],
            free_points: RefCell::new(3),
            talent_spell_ids: ids,
            learned: RefCell::new(Vec::new()),
            rand_fixed: 0,
            picked_spec: None,
            pick_calls: RefCell::new(Vec::new()),
        };
        init_talents(&m, 0);
        let learned = m.learned.borrow();
        assert_eq!(learned.len(), 3);
        // All learned ranks must come from the first row (budget exhausted
        // before we reach row 1).
        for id in learned.iter() {
            assert!(row0.contains(id), "spent {id} outside row 0");
        }
        assert_eq!(*m.free_points.borrow(), 0);
    }

    #[test]
    fn skips_zero_rank_ids() {
        // Talent with only 2 ranks (the rest are 0) — should stop at 2.
        let ranks = [400u32, 401, 0, 0, 0];
        let m = MockIface {
            talents: vec![talent(0, ranks)],
            free_points: RefCell::new(5),
            talent_spell_ids: [400u32, 401].into_iter().collect(),
            learned: RefCell::new(Vec::new()),
            rand_fixed: 0,
            picked_spec: None,
            pick_calls: RefCell::new(Vec::new()),
        };
        init_talents(&m, 0);
        assert_eq!(m.learned.borrow().as_slice(), &[400u32, 401]);
        assert_eq!(*m.free_points.borrow(), 3);
    }

    #[test]
    fn visits_multiple_rows_when_budget_allows() {
        // Two rows, 10 points → both should see spending.
        let row0 = [500u32, 501, 502, 503, 504];
        let row1 = [600u32, 601, 602, 603, 604];
        let mut ids: std::collections::HashSet<u32> = row0.iter().copied().collect();
        ids.extend(row1);
        let m = MockIface {
            talents: vec![talent(0, row0), talent(1, row1)],
            free_points: RefCell::new(10),
            talent_spell_ids: ids,
            learned: RefCell::new(Vec::new()),
            rand_fixed: 0,
            picked_spec: None,
            pick_calls: RefCell::new(Vec::new()),
        };
        init_talents(&m, 0);
        let learned = m.learned.borrow();
        assert_eq!(learned.len(), 10);
        assert!(learned.iter().any(|id| row0.contains(id)));
        assert!(learned.iter().any(|id| row1.contains(id)));
        assert_eq!(*m.free_points.borrow(), 0);
    }

    #[test]
    fn tree_spends_in_picked_spec_only_when_budget_fits() {
        // Picker returns spec 1. Budget = 5, row in picked spec soaks it all,
        // so no fallback into the complementary tab happens.
        let row0 = [700u32, 701, 702, 703, 704];
        let m = MockIface {
            talents: vec![talent(0, row0)],
            free_points: RefCell::new(5),
            talent_spell_ids: row0.iter().copied().collect(),
            learned: RefCell::new(Vec::new()),
            rand_fixed: 0,
            picked_spec: Some(1),
            pick_calls: RefCell::new(Vec::new()),
        };
        init_talents_tree(&m, false);
        assert_eq!(m.learned.borrow().len(), 5);
        assert_eq!(m.pick_calls.borrow().as_slice(), &[false]);
        assert_eq!(*m.free_points.borrow(), 0);
    }

    #[test]
    fn tree_propagates_incremental_flag() {
        let m = MockIface {
            talents: vec![],
            free_points: RefCell::new(0),
            talent_spell_ids: Default::default(),
            learned: RefCell::new(Vec::new()),
            rand_fixed: 0,
            picked_spec: Some(2),
            pick_calls: RefCell::new(Vec::new()),
        };
        init_talents_tree(&m, true);
        assert_eq!(m.pick_calls.borrow().as_slice(), &[true]);
    }
}
