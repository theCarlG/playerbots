//! Factory InitTaxiNodes — mirrors `PlayerbotFactory::InitTaxiNodes`.
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

use crate::ffi::interface::BotInterface;

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
pub fn init_taxi_nodes(iface: &dyn BotInterface, level: u32, team: u8, start_map: u32) {
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

    let nodes = iface.get_overworld_taxi_nodes(team);
    for node in nodes {
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
            && iface.random_u32(0, 4) != 0
        {
            continue;
        }
        iface.bot_set_taxi_node(node.index);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::BotTaxiNode;
    use crate::ffi::interface::BotInterface;
    use crate::ffi::types::{ItemId, SpellId};
    use std::cell::RefCell;

    struct MockIface {
        nodes: Vec<BotTaxiNode>,
        rand_fixed: u32,
        flagged: RefCell<Vec<u32>>,
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
        fn get_overworld_taxi_nodes(&self, _: u8) -> Vec<BotTaxiNode> {
            self.nodes.clone()
        }
        fn bot_set_taxi_node(&self, node_index: u32) {
            self.flagged.borrow_mut().push(node_index);
        }
    }

    fn node(index: u32, map_id: u32) -> BotTaxiNode {
        BotTaxiNode { index, map_id }
    }

    fn mock(nodes: Vec<BotTaxiNode>) -> MockIface {
        // rand_fixed = 0 → all `random_u32 != 0` cross-map filters pass
        // (i.e. the cross-map filter never fires).
        MockIface {
            nodes,
            rand_fixed: 0,
            flagged: RefCell::new(Vec::new()),
        }
    }

    #[test]
    fn northrend_locked_before_66() {
        let m = mock(vec![node(10, MAP_NORTHREND), node(11, MAP_EK)]);
        init_taxi_nodes(&m, 65, 0, MAP_EK);
        assert_eq!(m.flagged.borrow().as_slice(), &[11]);
    }

    #[test]
    fn northrend_unlocked_at_66() {
        let m = mock(vec![node(10, MAP_NORTHREND)]);
        init_taxi_nodes(&m, 66, 0, MAP_EK);
        assert_eq!(m.flagged.borrow().as_slice(), &[10]);
    }

    #[test]
    fn outland_locked_before_58() {
        let m = mock(vec![node(20, MAP_OUTLAND), node(21, MAP_KALIMDOR)]);
        init_taxi_nodes(&m, 57, 1, MAP_KALIMDOR);
        assert_eq!(m.flagged.borrow().as_slice(), &[21]);
    }

    #[test]
    fn outland_unlocked_at_58() {
        let m = mock(vec![node(20, MAP_OUTLAND)]);
        init_taxi_nodes(&m, 58, 0, MAP_EK);
        assert_eq!(m.flagged.borrow().as_slice(), &[20]);
    }

    #[test]
    fn outland_start_maps_to_home_continent() {
        // Draenei (alliance) start on Outland — treat Kalimdor as home.
        // With rand_fixed=1 (non-zero), a level 10 bot's cross-map filter
        // fires for any node not on the home continent.
        let m = MockIface {
            nodes: vec![node(1, MAP_KALIMDOR), node(2, MAP_EK)],
            rand_fixed: 1,
            flagged: RefCell::new(Vec::new()),
        };
        init_taxi_nodes(&m, 10, TEAM_ALLIANCE, MAP_OUTLAND);
        // Kalimdor is the effective home → kept. EK → skipped.
        assert_eq!(m.flagged.borrow().as_slice(), &[1]);
    }

    #[test]
    fn high_level_bypasses_cross_map_filter() {
        // Level 30 is past CROSS_MAP_LOW_LEVEL → every non-outland,
        // non-northrend node is flagged regardless of rand.
        let m = MockIface {
            nodes: vec![node(1, MAP_EK), node(2, MAP_KALIMDOR)],
            rand_fixed: 999,
            flagged: RefCell::new(Vec::new()),
        };
        init_taxi_nodes(&m, 30, 0, MAP_EK);
        assert_eq!(m.flagged.borrow().as_slice(), &[1, 2]);
    }
}
