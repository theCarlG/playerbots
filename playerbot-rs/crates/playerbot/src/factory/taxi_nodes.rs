//! Factory `InitTaxiNodes` — mirrors `PlayerbotFactory::InitTaxiNodes`.
//!
//! The C++ source iterates `PlayerbotFactory::overworldTaxiNodeLevelsA/H`,
//! static tables populated at startup from `sTaxiNodesStore` and filtered
//! to the four overworld continents (EK, Kalimdor, Outland, Northrend).
//! Each entry's `Level` is simplified to `1`, so the level-gating `urand`
//! filters mostly degenerate — we drop them entirely on the Rust side
//! except for the cross-map <21 clause, and keep only the hard map gates.
//!
//! The team filter + continent filter lives in
//! `BotBridge::CB_GetOverworldTaxiNodes`; this module walks the returned
//! `(index, map_id)` pairs, applies the remaining gates, and flags the
//! surviving nodes on the bot's taxi mask.

use super::FactoryTransaction;

/// Map id of Outland; Blood Elves (Alliance) and Draenei (Horde) start here,
/// which means their "home continent" should be treated as EK/Kalimdor for
/// the cross-map filter — matching the C++ override.
const MAP_OUTLAND: u32 = 530;
/// Map id of Northrend. Locked until level 66.
const MAP_NORTHREND: u32 = 571;
/// Map id of Eastern Kingdoms.
const MAP_EK: u32 = 0;
/// Map id of Kalimdor.
const MAP_KALIMDOR: u32 = 1;

const NORTHREND_MIN_LEVEL: u32 = 66;
const OUTLAND_MIN_LEVEL: u32 = 58;
/// Cross-map filter from the C++ source: other continents with
/// `Level + 20 > botLevel` are randomly skipped (`urand(0, 4)` — 4 / 5 odds).
/// Since `Level` is hard-coded to 1, this degenerates to `botLevel < 21`.
const CROSS_MAP_LOW_LEVEL: u32 = 21;

const TEAM_ALLIANCE: u8 = 0;

/// Flag level-appropriate taxi nodes on the bot. `start_map` is the bot's
/// current map id and mirrors `bot->GetMapId()` in the C++ source. The
/// Outland override mirrors the classic behaviour where BE (Horde) start
/// on EK and Draenei (Alliance) on Kalimdor.
pub fn init_taxi_nodes(tx: &mut FactoryTransaction<'_>, level: u32, team: u8, start_map: u32) {
    // Outland override: BE / Draenei characters start on map 530 but their
    // "home" for the cross-map filter is their racial continent.
    let effective_start_map = if start_map == MAP_OUTLAND {
        if team == TEAM_ALLIANCE {
            MAP_KALIMDOR
        } else {
            MAP_EK
        }
    } else {
        start_map
    };

    let nodes = tx.get_overworld_taxi_nodes(team);
    for node in &nodes {
        if node.map_id == MAP_NORTHREND && level < NORTHREND_MIN_LEVEL {
            continue;
        }
        if node.map_id == MAP_OUTLAND && level < OUTLAND_MIN_LEVEL {
            continue;
        }
        // Cross-map low-level filter (Level is simplified to 1 upstream).
        // `urand(0, 4)` is non-zero 4/5 of the time; we match the semantic
        // via `random_u32(0, 4) != 0`.
        if node.map_id != effective_start_map
            && level < CROSS_MAP_LOW_LEVEL
            && tx.random_u32(0, 4) != 0
        {
            continue;
        }
        tx.bot_set_taxi_node(node.index);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::with_tx;
    use cmangos::{BotTaxiNode, MockEvent, MockWorld};

    fn node(index: u32, map_id: u32) -> BotTaxiNode {
        BotTaxiNode { index, map_id }
    }

    fn world(nodes: Vec<BotTaxiNode>, rand_fixed: u32) -> MockWorld {
        MockWorld::builder()
            .taxi_nodes(0, nodes.clone())
            .taxi_nodes(1, nodes)
            .random_default(rand_fixed)
            .build()
    }

    fn flagged(world: &MockWorld) -> Vec<u32> {
        world
            .events()
            .iter()
            .filter_map(|e| match e {
                MockEvent::SetTaxiNode(idx) => Some(*idx),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn northrend_locked_before_66() {
        let mut w = world(vec![node(10, MAP_NORTHREND), node(11, MAP_EK)], 0);
        with_tx(&mut w, |tx| init_taxi_nodes(tx, 65, 0, MAP_EK));
        assert_eq!(flagged(&w), vec![11]);
    }

    #[test]
    fn northrend_unlocked_at_66() {
        let mut w = world(vec![node(10, MAP_NORTHREND)], 0);
        with_tx(&mut w, |tx| init_taxi_nodes(tx, 66, 0, MAP_EK));
        assert_eq!(flagged(&w), vec![10]);
    }

    #[test]
    fn outland_locked_before_58() {
        let mut w = world(vec![node(20, MAP_OUTLAND), node(21, MAP_KALIMDOR)], 0);
        with_tx(&mut w, |tx| init_taxi_nodes(tx, 57, 1, MAP_KALIMDOR));
        assert_eq!(flagged(&w), vec![21]);
    }

    #[test]
    fn outland_unlocked_at_58() {
        let mut w = world(vec![node(20, MAP_OUTLAND)], 0);
        with_tx(&mut w, |tx| init_taxi_nodes(tx, 58, 0, MAP_EK));
        assert_eq!(flagged(&w), vec![20]);
    }

    #[test]
    fn outland_start_maps_to_home_continent() {
        // Draenei (alliance) start on Outland — treat Kalimdor as home.
        // With rand_fixed=1 (non-zero), a level 10 bot's cross-map filter
        // fires for any node not on the home continent.
        let mut w = world(vec![node(1, MAP_KALIMDOR), node(2, MAP_EK)], 1);
        with_tx(&mut w, |tx| {
            init_taxi_nodes(tx, 10, TEAM_ALLIANCE, MAP_OUTLAND)
        });
        // Kalimdor is the effective home → kept. EK → skipped.
        assert_eq!(flagged(&w), vec![1]);
    }

    #[test]
    fn high_level_bypasses_cross_map_filter() {
        let mut w = world(vec![node(1, MAP_EK), node(2, MAP_KALIMDOR)], 999);
        with_tx(&mut w, |tx| init_taxi_nodes(tx, 30, 0, MAP_EK));
        assert_eq!(flagged(&w), vec![1, 2]);
    }
}
