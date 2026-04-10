//! Factory `InitTalents` — mirrors `PlayerbotFactory::InitTalents(specNo)`.
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

use super::FactoryTransaction;
use cmangos::BotTalentEntry;
use std::collections::BTreeMap;

/// Points spent per row budget (matches the C++ source's inner guard
/// `freePoints - GetFreeTalentPoints() < 5`).
const POINTS_PER_ROW: u32 = 5;
/// Per-row retry budget before moving on (matches C++ `attemptCount < 3`).
const ATTEMPTS_PER_ROW: u32 = 3;

/// Fill the bot's `spec_no` talent tab with randomly-selected talents until
/// free talent points are exhausted.
pub fn init_talents(tx: &mut FactoryTransaction<'_>, spec_no: u32) {
    // `spec_no` is bounded to 0..=2 by the caller, but the C++ source and the
    // FFI both take it as u32 / u8. Clamp defensively.
    let spec = spec_no.min(u8::MAX as u32) as u8;

    let talents = tx.get_class_talents(spec);
    if talents.is_empty() {
        return;
    }

    // Group by row so we can drain rows independently.
    let mut rows: BTreeMap<u32, Vec<BotTalentEntry>> = BTreeMap::new();
    for &t in &talents {
        rows.entry(t.row).or_default().push(t);
    }

    // The C++ source caches `freePoints` before each row to determine when
    // it has spent 5 points inside that row. We mirror the same loop.
    let mut free_points = tx.bot_free_talent_points();
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
            let idx = tx.random_u32(0, (pool.len() - 1) as u32) as usize;
            let talent = pool.swap_remove(idx);

            for &spell_id in &talent.rank_ids {
                if spell_id == 0 {
                    continue;
                }
                if tx.bot_free_talent_points() == 0 {
                    break;
                }
                tx.bot_learn_spell(spell_id);
                tx.bot_update_free_talent_points();
            }

            free_points = tx.bot_free_talent_points();
        }
    }
}

/// Mirror of `PlayerbotFactory::InitTalentsTree`: pick (or recall) the spec
/// the bot should invest into, spend its points there, and if any points
/// remain, dump them into the complementary tab. The spec-picking state
/// management (sRandomPlayerbotMgr get/set + config roll) is bundled into
/// the single `bot_pick_spec_no` FFI call.
pub fn init_talents_tree(tx: &mut FactoryTransaction<'_>, incremental: bool) {
    let spec_no = tx.bot_pick_spec_no(incremental);
    init_talents(tx, spec_no);

    if tx.bot_free_talent_points() > 0 {
        // Dump any leftover points into the complementary tab. The C++
        // source computes `2 - specNo`; `saturating_sub` keeps us safe if
        // the picker ever returns a value > 2.
        let other = 2u32.saturating_sub(spec_no);
        init_talents(tx, other);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::with_tx;
    use cmangos::{BotTalentEntry, MockEvent, MockWorld, World};

    fn talent(row: u32, ranks: [u32; 5]) -> BotTalentEntry {
        BotTalentEntry { row, rank_ids: ranks }
    }

    fn learned(w: &MockWorld) -> Vec<u32> {
        w.events()
            .iter()
            .filter_map(|e| match e {
                MockEvent::LearnSpell(id) => Some(*id),
                _ => None,
            })
            .collect()
    }

    fn pick_calls(w: &MockWorld) -> Vec<bool> {
        w.events()
            .iter()
            .filter_map(|e| match e {
                MockEvent::PickSpecNo { incremental } => Some(*incremental),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn no_talents_is_noop() {
        let mut w = MockWorld::builder().free_talent_points(5).build();
        with_tx(&mut w, |tx| init_talents(tx, 0));
        assert!(learned(&w).is_empty());
        assert_eq!(w.bot_free_talent_points(), 5);
    }

    #[test]
    fn spends_all_points_in_one_row() {
        // Single row with a 5-rank talent; free points = 5 → every rank learned.
        let ranks = [100u32, 101, 102, 103, 104];
        let mut w = MockWorld::builder()
            .class_talents(0, vec![talent(0, ranks)])
            .free_talent_points(5)
            .build();
        with_tx(&mut w, |tx| init_talents(tx, 0));
        assert_eq!(learned(&w).as_slice(), &ranks);
        assert_eq!(w.bot_free_talent_points(), 0);
    }

    #[test]
    fn stops_learning_when_budget_zero() {
        // Two rows with 5-rank talents, but only 3 points available.
        let row0 = [200u32, 201, 202, 203, 204];
        let row1 = [300u32, 301, 302, 303, 304];
        let mut w = MockWorld::builder()
            .class_talents(0, vec![talent(0, row0), talent(1, row1)])
            .free_talent_points(3)
            .build();
        with_tx(&mut w, |tx| init_talents(tx, 0));
        let l = learned(&w);
        assert_eq!(l.len(), 3);
        // All learned ranks must come from the first row (budget exhausted
        // before we reach row 1).
        for id in l.iter() {
            assert!(row0.contains(id), "spent {id} outside row 0");
        }
        assert_eq!(w.bot_free_talent_points(), 0);
    }

    #[test]
    fn skips_zero_rank_ids() {
        // Talent with only 2 ranks (the rest are 0) — should stop at 2.
        let ranks = [400u32, 401, 0, 0, 0];
        let mut w = MockWorld::builder()
            .class_talents(0, vec![talent(0, ranks)])
            .free_talent_points(5)
            .build();
        with_tx(&mut w, |tx| init_talents(tx, 0));
        assert_eq!(learned(&w).as_slice(), &[400u32, 401]);
        assert_eq!(w.bot_free_talent_points(), 3);
    }

    #[test]
    fn visits_multiple_rows_when_budget_allows() {
        // Two rows, 10 points → both should see spending.
        let row0 = [500u32, 501, 502, 503, 504];
        let row1 = [600u32, 601, 602, 603, 604];
        let mut w = MockWorld::builder()
            .class_talents(0, vec![talent(0, row0), talent(1, row1)])
            .free_talent_points(10)
            .build();
        with_tx(&mut w, |tx| init_talents(tx, 0));
        let l = learned(&w);
        assert_eq!(l.len(), 10);
        assert!(l.iter().any(|id| row0.contains(id)));
        assert!(l.iter().any(|id| row1.contains(id)));
        assert_eq!(w.bot_free_talent_points(), 0);
    }

    #[test]
    fn tree_spends_in_picked_spec_only_when_budget_fits() {
        // Picker returns spec 1. Budget = 5, row in picked spec soaks it all,
        // so no fallback into the complementary tab happens.
        let row0 = [700u32, 701, 702, 703, 704];
        let mut w = MockWorld::builder()
            .class_talents(1, vec![talent(0, row0)])
            .free_talent_points(5)
            .spec_tab(1)
            .build();
        with_tx(&mut w, |tx| init_talents_tree(tx, false));
        assert_eq!(learned(&w).len(), 5);
        assert_eq!(pick_calls(&w).as_slice(), &[false]);
        assert_eq!(w.bot_free_talent_points(), 0);
    }

    #[test]
    fn tree_propagates_incremental_flag() {
        let mut w = MockWorld::builder().spec_tab(2).build();
        with_tx(&mut w, |tx| init_talents_tree(tx, true));
        assert_eq!(pick_calls(&w).as_slice(), &[true]);
    }
}
