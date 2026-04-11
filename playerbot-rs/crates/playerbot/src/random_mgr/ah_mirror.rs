//! Auction-house mirror rebuild — ports `RandomPlayerbotMgr::MirrorAh`.
//!
//! The C++ original at `RandomPlayerbotMgr.cpp:3750` walks every
//! `AuctionHouseObject` (the three faction houses), pulls each
//! `AuctionEntry`, and groups them by item template into
//! `ahMirror[item_id] → Vec<AuctionEntry>`. Entries without a buyout
//! price or a zero item count are skipped — the mirror is only used
//! by the AH-trading strategy to cross-reference "is this item already
//! listed and at what price", so rows that can't actually be purchased
//! are useless to the caller.
//!
//! The Rust version pulls the already-flattened rows from
//! [`RandomMgrWorld::query_ah_rows`] (which on the production impl
//! walks the same three houses) and rebuilds [`RandomMgrState::ah_mirror`]
//! in one pass.

use cmangos::{AhMirrorRow, RandomMgrWorld};

use super::state::{AhMirrorEntry, RandomMgrState};

/// Rebuild the AH mirror. Always runs — the C++ caller gates this on
/// the AH update timer, not on a cadence inside the function itself.
/// Returns the number of rows written into the mirror (useful for
/// stats logging).
pub fn mirror_ah(state: &mut RandomMgrState, world: &dyn RandomMgrWorld) -> usize {
    state.ah_mirror.clear();
    let rows = world.query_ah_rows();
    let mut inserted = 0;
    for row in rows {
        if let Some(entry) = into_mirror_entry(&row) {
            state
                .ah_mirror
                .entry(row.item_id)
                .or_default()
                .push(entry);
            inserted += 1;
        }
    }
    inserted
}

/// Convert a raw trait row into the Rust [`AhMirrorEntry`] shape.
/// Returns `None` for rows that would be filtered out by the C++
/// `buyout == 0 || itemCount == 0` guards.
#[must_use]
pub fn into_mirror_entry(row: &AhMirrorRow) -> Option<AhMirrorEntry> {
    if row.buyout == 0 || row.count == 0 {
        return None;
    }
    Some(AhMirrorEntry {
        item_id: row.item_id,
        buyout: row.buyout,
        count: row.count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmangos::MockRandomMgrWorld;

    #[test]
    fn empty_world_leaves_mirror_empty() {
        let mut state = RandomMgrState::new();
        let world = MockRandomMgrWorld::new();
        let n = mirror_ah(&mut state, &world);
        assert_eq!(n, 0);
        assert!(state.ah_mirror.is_empty());
    }

    #[test]
    fn rows_are_grouped_by_item_id() {
        let mut state = RandomMgrState::new();
        let world = MockRandomMgrWorld::new();
        world.set_ah_rows(vec![
            AhMirrorRow {
                item_id: 6948,
                buyout: 100,
                count: 1,
            },
            AhMirrorRow {
                item_id: 6948,
                buyout: 150,
                count: 2,
            },
            AhMirrorRow {
                item_id: 2589,
                buyout: 50,
                count: 20,
            },
        ]);
        let n = mirror_ah(&mut state, &world);
        assert_eq!(n, 3);

        let bag = state.ah_mirror.get(&6948).unwrap();
        assert_eq!(bag.len(), 2);
        assert_eq!(bag[0].buyout, 100);
        assert_eq!(bag[1].buyout, 150);

        let linen = state.ah_mirror.get(&2589).unwrap();
        assert_eq!(linen.len(), 1);
        assert_eq!(linen[0].count, 20);
    }

    #[test]
    fn rows_without_buyout_or_count_are_filtered() {
        let mut state = RandomMgrState::new();
        let world = MockRandomMgrWorld::new();
        world.set_ah_rows(vec![
            AhMirrorRow {
                item_id: 1,
                buyout: 0,
                count: 5,
            },
            AhMirrorRow {
                item_id: 2,
                buyout: 100,
                count: 0,
            },
            AhMirrorRow {
                item_id: 3,
                buyout: 100,
                count: 5,
            },
        ]);
        let n = mirror_ah(&mut state, &world);
        assert_eq!(n, 1);
        assert!(state.ah_mirror.contains_key(&3));
        assert!(!state.ah_mirror.contains_key(&1));
        assert!(!state.ah_mirror.contains_key(&2));
    }

    #[test]
    fn rebuild_drops_previous_contents() {
        let mut state = RandomMgrState::new();
        state
            .ah_mirror
            .entry(999)
            .or_default()
            .push(AhMirrorEntry {
                item_id: 999,
                buyout: 1,
                count: 1,
            });
        let world = MockRandomMgrWorld::new();
        let _ = mirror_ah(&mut state, &world);
        assert!(state.ah_mirror.is_empty());
    }
}
