//! Factory reputation initialization — mirrors
//! `PlayerbotFactory::InitReputations`. At level 60+ the bot is granted
//! "honored"-cap standing (42000) with a fixed list of neutral and `PvP`
//! factions, split by team. TBC/WotLK builds add an extra pack of outland
//! factions behind the expansion feature gate.
//!
//! Pure policy: the only side effect is calling `bot_set_reputation` on
//! the interface.

use crate::ffi::interface::BotInterface;

/// Minimum level before any reputation is granted. Matches the C++ source,
/// which gated both the neutral and `PvP` factions behind `level >= 60`.
const MIN_LEVEL: u32 = 60;

/// Hard-coded "honored" cap value used by the C++ source. Preserved verbatim.
const REP_FRIENDLY_MAX: i32 = 42000;

/// `Player::GetTeam()` values mirror Classic: `ALLIANCE = 469`, `HORDE = 67`,
/// but `BotWorldSnapshot.self.team` is normalized to 0 / 1. See the snapshot
/// fill site in `BotBridge.cpp`.
const TEAM_ALLIANCE: u8 = 0;

// ── Vanilla (always compiled) ─────────────────────────────────────────────

// Neutral: Nozdormu, Hydraxian Waterlords, Argent Dawn.
const NEUTRAL: &[u32] = &[910, 749, 529];

// PvP — Alliance: Silverwing Sentinels, Stormpike Guard, The League of Arathor.
const PVP_ALLIANCE: &[u32] = &[890, 730, 509];

// PvP — Horde: Frostwolf Clan, The Defilers, Warsong Outriders.
const PVP_HORDE: &[u32] = &[729, 510, 889];

// ── TBC / WotLK additions ─────────────────────────────────────────────────
//
// C++ source gated these behind `#ifndef MANGOSBOT_ZERO`. On Rust we mirror
// via the `vanilla` feature: vanilla off → TBC or WotLK build.

#[cfg(not(feature = "vanilla"))]
const TBC_NEUTRAL: &[u32] = &[
    942,  // Cenarion Expedition
    935,  // The Sha'tar
    1011, // Lower City
    989,  // Keepers of Time
    967,  // The Violet Eye
    1015, // Netherwing
    1077, // Shattered Sun Offensive
    1012, // Ashtongue Deathsworn
    970,  // Sporeggar
    933,  // The Consortium
    1031, // Sha'tari Skyguard
];

#[cfg(not(feature = "vanilla"))]
const TBC_ALLIANCE: &[u32] = &[
    946, // Honor Hold
    978, // Kurenai
];

#[cfg(not(feature = "vanilla"))]
const TBC_HORDE: &[u32] = &[
    947, // Thrallmar
    941, // The Mag'har
    922, // Tranquillien
];

// ── Policy ────────────────────────────────────────────────────────────────

/// Grant the bot honored standing with every faction that matches its
/// level, team, and build expansion.
///
/// `team` matches `BotWorldSnapshot.self.team` — 0 = Alliance, 1 = Horde.
pub fn init_reputations(iface: &dyn BotInterface, level: u32, team: u8) {
    if level < MIN_LEVEL {
        return;
    }
    let is_alliance = team == TEAM_ALLIANCE;

    for &f in NEUTRAL {
        iface.bot_set_reputation(f, REP_FRIENDLY_MAX);
    }
    let pvp = if is_alliance { PVP_ALLIANCE } else { PVP_HORDE };
    for &f in pvp {
        iface.bot_set_reputation(f, REP_FRIENDLY_MAX);
    }

    #[cfg(not(feature = "vanilla"))]
    {
        for &f in TBC_NEUTRAL {
            iface.bot_set_reputation(f, REP_FRIENDLY_MAX);
        }
        let tbc_pvp = if is_alliance { TBC_ALLIANCE } else { TBC_HORDE };
        for &f in tbc_pvp {
            iface.bot_set_reputation(f, REP_FRIENDLY_MAX);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::interface::BotInterface;
    use crate::ffi::types::ItemId;
    use std::cell::RefCell;
    use std::collections::HashSet;

    #[derive(Default)]
    struct MockIface {
        rep_calls: RefCell<Vec<(u32, i32)>>,
    }

    // Safety: RefCell is !Sync but only touched single-threaded in tests.
    unsafe impl Send for MockIface {}

    impl BotInterface for MockIface {
        fn get_snapshot(&self) -> crate::ffi::BotWorldSnapshot {
            unsafe { std::mem::zeroed() }
        }
        fn get_unit_snapshot(&self, _: crate::ffi::UnitHandle) -> crate::ffi::BotUnitSnapshot {
            unsafe { std::mem::zeroed() }
        }
        fn has_aura(&self, _: crate::ffi::UnitHandle, _: crate::ffi::SpellId) -> bool {
            false
        }
        fn get_aura(
            &self,
            _: crate::ffi::UnitHandle,
            _: crate::ffi::SpellId,
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
        fn can_cast(&self, _: crate::ffi::SpellId, _: crate::ffi::UnitHandle) -> bool {
            false
        }
        fn spell_cooldown_ms(&self, _: crate::ffi::SpellId) -> u32 {
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
        fn cast_spell(&self, _: crate::ffi::SpellId, _: crate::ffi::UnitHandle) -> bool {
            false
        }
        fn cast_spell_pos(&self, _: crate::ffi::SpellId, _: f32, _: f32, _: f32) -> bool {
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

        fn bot_set_reputation(&self, faction_id: u32, value: i32) -> bool {
            self.rep_calls.borrow_mut().push((faction_id, value));
            true
        }
    }

    fn factions_called(m: &MockIface) -> HashSet<u32> {
        m.rep_calls.borrow().iter().map(|(f, _)| *f).collect()
    }

    #[test]
    fn noop_below_level_60() {
        let m = MockIface::default();
        init_reputations(&m, 59, 0);
        assert!(m.rep_calls.borrow().is_empty());
    }

    #[test]
    fn alliance_level_60_sets_neutral_plus_alliance_pvp() {
        let m = MockIface::default();
        init_reputations(&m, 60, 0);
        let set = factions_called(&m);
        for &f in NEUTRAL {
            assert!(set.contains(&f), "missing neutral faction {f}");
        }
        for &f in PVP_ALLIANCE {
            assert!(set.contains(&f), "missing alliance faction {f}");
        }
        // Values must all be at the honored cap.
        assert!(
            m.rep_calls
                .borrow()
                .iter()
                .all(|(_, v)| *v == REP_FRIENDLY_MAX)
        );
    }

    #[test]
    fn horde_level_60_sets_neutral_plus_horde_pvp() {
        let m = MockIface::default();
        init_reputations(&m, 60, 1);
        let set = factions_called(&m);
        for &f in NEUTRAL {
            assert!(set.contains(&f));
        }
        for &f in PVP_HORDE {
            assert!(set.contains(&f));
        }
    }

    #[test]
    fn no_cross_faction_pvp() {
        let m = MockIface::default();
        init_reputations(&m, 60, 1);
        let set = factions_called(&m);
        for &f in PVP_ALLIANCE {
            assert!(!set.contains(&f), "horde bot got alliance faction {f}");
        }

        let m2 = MockIface::default();
        init_reputations(&m2, 60, 0);
        let set2 = factions_called(&m2);
        for &f in PVP_HORDE {
            assert!(!set2.contains(&f), "alliance bot got horde faction {f}");
        }
    }
}
