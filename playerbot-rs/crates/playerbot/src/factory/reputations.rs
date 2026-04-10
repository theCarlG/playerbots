//! Factory reputation initialization — mirrors
//! `PlayerbotFactory::InitReputations`. At level 60+ the bot is granted
//! "honored"-cap standing (42000) with a fixed list of neutral and `PvP`
//! factions, split by team. TBC/WotLK builds add an extra pack of outland
//! factions behind the expansion feature gate.
//!
//! Pure policy: the only side effect is calling `bot_set_reputation` on
//! the interface.

use super::FactoryTransaction;

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
pub fn init_reputations(tx: &mut FactoryTransaction<'_>, level: u32, team: u8) {
    if level < MIN_LEVEL {
        return;
    }
    let is_alliance = team == TEAM_ALLIANCE;

    for &f in NEUTRAL {
        tx.bot_set_reputation(f, REP_FRIENDLY_MAX);
    }
    let pvp = if is_alliance { PVP_ALLIANCE } else { PVP_HORDE };
    for &f in pvp {
        tx.bot_set_reputation(f, REP_FRIENDLY_MAX);
    }

    #[cfg(not(feature = "vanilla"))]
    {
        for &f in TBC_NEUTRAL {
            tx.bot_set_reputation(f, REP_FRIENDLY_MAX);
        }
        let tbc_pvp = if is_alliance { TBC_ALLIANCE } else { TBC_HORDE };
        for &f in tbc_pvp {
            tx.bot_set_reputation(f, REP_FRIENDLY_MAX);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::with_tx;
    use cmangos::{MockEvent, MockWorld};
    use std::collections::HashSet;

    fn rep_calls(w: &MockWorld) -> Vec<(u32, i32)> {
        w.events()
            .iter()
            .filter_map(|e| match e {
                MockEvent::SetReputation { faction, value } => Some((*faction, *value)),
                _ => None,
            })
            .collect()
    }

    fn factions_called(w: &MockWorld) -> HashSet<u32> {
        rep_calls(w).iter().map(|(f, _)| *f).collect()
    }

    #[test]
    fn noop_below_level_60() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_reputations(tx, 59, 0));
        assert!(rep_calls(&w).is_empty());
    }

    #[test]
    fn alliance_level_60_sets_neutral_plus_alliance_pvp() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_reputations(tx, 60, 0));
        let set = factions_called(&w);
        for &f in NEUTRAL {
            assert!(set.contains(&f), "missing neutral faction {f}");
        }
        for &f in PVP_ALLIANCE {
            assert!(set.contains(&f), "missing alliance faction {f}");
        }
        // Values must all be at the honored cap.
        assert!(rep_calls(&w).iter().all(|(_, v)| *v == REP_FRIENDLY_MAX));
    }

    #[test]
    fn horde_level_60_sets_neutral_plus_horde_pvp() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_reputations(tx, 60, 1));
        let set = factions_called(&w);
        for &f in NEUTRAL {
            assert!(set.contains(&f));
        }
        for &f in PVP_HORDE {
            assert!(set.contains(&f));
        }
    }

    #[test]
    fn no_cross_faction_pvp() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_reputations(tx, 60, 1));
        let set = factions_called(&w);
        for &f in PVP_ALLIANCE {
            assert!(!set.contains(&f), "horde bot got alliance faction {f}");
        }

        let mut w2 = MockWorld::default();
        with_tx(&mut w2, |tx| init_reputations(tx, 60, 0));
        let set2 = factions_called(&w2);
        for &f in PVP_HORDE {
            assert!(!set2.contains(&f), "alliance bot got horde faction {f}");
        }
    }
}
