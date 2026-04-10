//! Mount initialization — ports `PlayerbotFactory::InitMounts`.
//!
//! The C++ version is a ~140-line per-race switch that builds four mount
//! lists (slow ground, fast ground, slow flying, fast flying), gates them on
//! level thresholds that differ per expansion, and `urand`-picks one from
//! each eligible tier before calling `Player::learnSpell`.
//!
//! In Rust the race switch collapses to a static table and the policy is a
//! loop over `(threshold, pool)` pairs. The flying tiers are compiled out on
//! Classic (`default` feature set) because no Classic race has flying mounts
//! and the level cap is 60.

use super::FactoryTransaction;

// ── Race IDs (mirrors CMaNGOS `Races` enum) ───────────────────────────────
const RACE_HUMAN: u8 = 1;
const RACE_ORC: u8 = 2;
const RACE_DWARF: u8 = 3;
const RACE_NIGHTELF: u8 = 4;
const RACE_UNDEAD: u8 = 5;
const RACE_TAUREN: u8 = 6;
const RACE_GNOME: u8 = 7;
const RACE_TROLL: u8 = 8;
#[cfg(any(feature = "tbc", feature = "wotlk"))]
const RACE_BLOODELF: u8 = 10;
#[cfg(any(feature = "tbc", feature = "wotlk"))]
const RACE_DRAENEI: u8 = 11;

// ── Team IDs (match `BotWorldSnapshot.team`; 0 = alliance, 1 = horde) ─────
#[cfg(any(feature = "tbc", feature = "wotlk"))]
const TEAM_ALLIANCE: u8 = 0;

// ── Level thresholds ──────────────────────────────────────────────────────
// Tier 0 = slow ground, 1 = fast ground, 2 = slow flying, 3 = fast flying.
// Each expansion shifted the requirements down.

#[cfg(not(any(feature = "tbc", feature = "wotlk")))]
const FIRST: u32 = 40;
#[cfg(not(any(feature = "tbc", feature = "wotlk")))]
const SECOND: u32 = 60;

#[cfg(all(feature = "tbc", not(feature = "wotlk")))]
const FIRST: u32 = 30;
#[cfg(all(feature = "tbc", not(feature = "wotlk")))]
const SECOND: u32 = 60;
#[cfg(all(feature = "tbc", not(feature = "wotlk")))]
const THIRD: u32 = 70;
#[cfg(all(feature = "tbc", not(feature = "wotlk")))]
const FOURTH: u32 = 70;

#[cfg(feature = "wotlk")]
const FIRST: u32 = 20;
#[cfg(feature = "wotlk")]
const SECOND: u32 = 40;
#[cfg(feature = "wotlk")]
const THIRD: u32 = 60;
#[cfg(feature = "wotlk")]
const FOURTH: u32 = 70;

// ── Race → (slow pool, fast pool) ─────────────────────────────────────────

/// Slow and fast ground-mount spell pools for `race`, or `None` for an
/// unknown race (bot race was not one the factory handles).
fn race_pools(race: u8) -> Option<(&'static [u32], &'static [u32])> {
    match race {
        RACE_HUMAN => Some((&[470, 6648, 458, 472], &[23228, 23227, 23229])),
        RACE_ORC => Some((&[6654, 6653, 580], &[23250, 23252, 23251])),
        RACE_DWARF => Some((&[6899, 6777, 6898], &[23238, 23239, 23240])),
        RACE_NIGHTELF => Some((&[10789, 8394, 10793], &[23221, 23219, 23338])),
        RACE_UNDEAD => Some((&[17463, 17464, 17462], &[17465, 23246])),
        RACE_TAUREN => Some((&[18990, 18989], &[23249, 23248, 23247])),
        RACE_GNOME => Some((&[10969, 17453, 10873, 17454], &[23225, 23223, 23222])),
        RACE_TROLL => Some((&[8395, 10796, 10799], &[23241, 23242, 23243])),
        #[cfg(any(feature = "tbc", feature = "wotlk"))]
        RACE_DRAENEI => Some((&[34406, 35711, 35710], &[35713, 35712, 35714])),
        #[cfg(any(feature = "tbc", feature = "wotlk"))]
        RACE_BLOODELF => Some((&[33660, 35020, 35022, 35018], &[35025, 35025, 35027])),
        _ => None,
    }
}

/// Slow and fast flying-mount spell pools for `team`.
/// Only compiled on expansions that have flying.
#[cfg(any(feature = "tbc", feature = "wotlk"))]
fn flying_for_team(team: u8) -> (&'static [u32], &'static [u32]) {
    if team == TEAM_ALLIANCE {
        (&[32235, 32239, 32240], &[32242, 32289, 32290, 32292])
    } else {
        (&[32244, 32245, 32243], &[32295, 32297, 32246, 32296])
    }
}

// ── Public entry point ────────────────────────────────────────────────────

/// Teach a bot one mount spell per eligible tier.
///
/// Inputs are read from the world snapshot by the caller (`run_misc`) — this
/// function stays pure policy so unit tests can drive it with a `MockIface`.
pub fn init_mounts(tx: &mut FactoryTransaction<'_>, level: u32, race: u8, team: u8) {
    if level < FIRST {
        return;
    }
    let Some((slow, fast)) = race_pools(race) else {
        return;
    };

    learn_one_of(tx, slow); // tier 0 (gated above)
    if level >= SECOND {
        learn_one_of(tx, fast); // tier 1
    }

    #[cfg(any(feature = "tbc", feature = "wotlk"))]
    {
        let (fslow, ffast) = flying_for_team(team);
        if level >= THIRD {
            learn_one_of(tx, fslow); // tier 2
        }
        if level >= FOURTH {
            learn_one_of(tx, ffast); // tier 3
        }
    }

    // Suppress unused-parameter warning on Classic.
    #[cfg(not(any(feature = "tbc", feature = "wotlk")))]
    let _ = team;
}

/// Pick one spell uniformly from `pool` and teach it to the bot.
fn learn_one_of(tx: &mut FactoryTransaction<'_>, pool: &[u32]) {
    if pool.is_empty() {
        return;
    }
    let idx = tx.random_u32(0, (pool.len() as u32) - 1) as usize;
    let spell = pool[idx.min(pool.len() - 1)];
    tx.bot_learn_spell(spell);
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::with_tx;
    use cmangos::{MockEvent, MockWorld};

    fn world(rng_pick: u32) -> MockWorld {
        MockWorld::builder().random_default(rng_pick).build()
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

    #[test]
    fn below_first_threshold_is_noop() {
        let mut w = world(0);
        with_tx(&mut w, |tx| init_mounts(tx, FIRST - 1, RACE_HUMAN, 0));
        assert!(learned(&w).is_empty());
    }

    #[test]
    fn first_threshold_learns_one_slow_ground_mount() {
        let mut w = world(0);
        with_tx(&mut w, |tx| init_mounts(tx, FIRST, RACE_HUMAN, 0));
        let l = learned(&w);
        assert_eq!(l.len(), 1);
        // Human slow pool starts with 470.
        assert_eq!(l[0], 470);
    }

    #[test]
    fn second_threshold_learns_slow_and_fast_ground_mount() {
        let mut w = world(0);
        with_tx(&mut w, |tx| init_mounts(tx, SECOND, RACE_ORC, 1));
        let l = learned(&w);
        // On Classic that's exactly 2 (no flying). On TBC/WotLK SECOND is
        // still below THIRD/FOURTH, so still exactly 2.
        assert_eq!(l.len(), 2);
        assert_eq!(l[0], 6654); // orc slow[0]
        assert_eq!(l[1], 23250); // orc fast[0]
    }

    #[test]
    fn rng_index_selects_correct_pool_entry() {
        let mut w = world(2);
        with_tx(&mut w, |tx| init_mounts(tx, FIRST, RACE_DWARF, 0));
        // Dwarf slow pool: [6899, 6777, 6898], index 2 → 6898.
        assert_eq!(learned(&w)[0], 6898);
    }

    #[test]
    fn unknown_race_is_noop() {
        let mut w = world(0);
        with_tx(&mut w, |tx| init_mounts(tx, SECOND, 99, 0));
        assert!(learned(&w).is_empty());
    }

    #[cfg(any(feature = "tbc", feature = "wotlk"))]
    #[test]
    fn horde_team_gets_horde_flying_pool() {
        let mut w = world(0);
        with_tx(&mut w, |tx| init_mounts(tx, FOURTH, RACE_ORC, 1 /* horde */));
        let l = learned(&w);
        // Expect 4 learns: slow, fast, flying-slow, flying-fast.
        assert_eq!(l.len(), 4);
        // flying-slow horde pool starts with 32244.
        assert_eq!(l[2], 32244);
        // flying-fast horde pool starts with 32295.
        assert_eq!(l[3], 32295);
    }

    #[cfg(any(feature = "tbc", feature = "wotlk"))]
    #[test]
    fn alliance_team_gets_alliance_flying_pool() {
        let mut w = world(0);
        with_tx(&mut w, |tx| {
            init_mounts(tx, FOURTH, RACE_HUMAN, 0 /* alliance */)
        });
        let l = learned(&w);
        assert_eq!(l.len(), 4);
        assert_eq!(l[2], 32235);
        assert_eq!(l[3], 32242);
    }
}
