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

use crate::ffi::interface::BotInterface;

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
pub fn init_mounts(iface: &dyn BotInterface, level: u32, race: u8, team: u8) {
    if level < FIRST {
        return;
    }
    let Some((slow, fast)) = race_pools(race) else {
        return;
    };

    learn_one_of(iface, slow); // tier 0 (gated above)
    if level >= SECOND {
        learn_one_of(iface, fast); // tier 1
    }

    #[cfg(any(feature = "tbc", feature = "wotlk"))]
    {
        let (fslow, ffast) = flying_for_team(team);
        if level >= THIRD {
            learn_one_of(iface, fslow); // tier 2
        }
        if level >= FOURTH {
            learn_one_of(iface, ffast); // tier 3
        }
    }

    // Suppress unused-parameter warning on Classic.
    #[cfg(not(any(feature = "tbc", feature = "wotlk")))]
    let _ = team;
}

/// Pick one spell uniformly from `pool` and teach it to the bot.
fn learn_one_of(iface: &dyn BotInterface, pool: &[u32]) {
    if pool.is_empty() {
        return;
    }
    let idx = iface.random_u32(0, (pool.len() as u32) - 1) as usize;
    let spell = pool[idx.min(pool.len() - 1)];
    iface.bot_learn_spell(spell);
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::interface::BotInterface;
    use crate::ffi::types::{BotRole, ItemId, SpellId};
    use std::cell::RefCell;

    #[derive(Default)]
    struct MockIface {
        learned: RefCell<Vec<u32>>,
        /// Deterministic RNG: always return `rng_pick` (clamped by callee).
        rng_pick: u32,
    }

    // Single-threaded test use; BotInterface requires Send.
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
        fn group_get_role(&self, _: crate::ffi::UnitHandle) -> BotRole {
            BotRole::NONE
        }

        fn random_u32(&self, min: u32, max: u32) -> u32 {
            self.rng_pick.clamp(min, max)
        }
        fn bot_learn_spell(&self, spell_id: u32) {
            self.learned.borrow_mut().push(spell_id);
        }
    }

    #[test]
    fn below_first_threshold_is_noop() {
        let m = MockIface::default();
        init_mounts(&m, FIRST - 1, RACE_HUMAN, 0);
        assert!(m.learned.borrow().is_empty());
    }

    #[test]
    fn first_threshold_learns_one_slow_ground_mount() {
        let m = MockIface::default(); // rng_pick = 0 → first entry of each pool
        init_mounts(&m, FIRST, RACE_HUMAN, 0);
        let learned = m.learned.borrow();
        assert_eq!(learned.len(), 1);
        // Human slow pool starts with 470.
        assert_eq!(learned[0], 470);
    }

    #[test]
    fn second_threshold_learns_slow_and_fast_ground_mount() {
        let m = MockIface::default();
        init_mounts(&m, SECOND, RACE_ORC, 1);
        let learned = m.learned.borrow();
        // On Classic that's exactly 2 (no flying). On TBC/WotLK SECOND is
        // still below THIRD/FOURTH, so still exactly 2.
        assert_eq!(learned.len(), 2);
        assert_eq!(learned[0], 6654); // orc slow[0]
        assert_eq!(learned[1], 23250); // orc fast[0]
    }

    #[test]
    fn rng_index_selects_correct_pool_entry() {
        let m = MockIface {
            rng_pick: 2,
            ..Default::default()
        };
        init_mounts(&m, FIRST, RACE_DWARF, 0);
        // Dwarf slow pool: [6899, 6777, 6898], index 2 → 6898.
        assert_eq!(m.learned.borrow()[0], 6898);
    }

    #[test]
    fn unknown_race_is_noop() {
        let m = MockIface::default();
        init_mounts(&m, SECOND, 99, 0);
        assert!(m.learned.borrow().is_empty());
    }

    #[cfg(any(feature = "tbc", feature = "wotlk"))]
    #[test]
    fn horde_team_gets_horde_flying_pool() {
        let m = MockIface::default();
        init_mounts(&m, FOURTH, RACE_ORC, 1 /* horde */);
        let learned = m.learned.borrow();
        // Expect 4 learns: slow, fast, flying-slow, flying-fast.
        assert_eq!(learned.len(), 4);
        // flying-slow horde pool starts with 32244.
        assert_eq!(learned[2], 32244);
        // flying-fast horde pool starts with 32295.
        assert_eq!(learned[3], 32295);
    }

    #[cfg(any(feature = "tbc", feature = "wotlk"))]
    #[test]
    fn alliance_team_gets_alliance_flying_pool() {
        let m = MockIface::default();
        init_mounts(&m, FOURTH, RACE_HUMAN, 0 /* alliance */);
        let learned = m.learned.borrow();
        assert_eq!(learned.len(), 4);
        assert_eq!(learned[2], 32235);
        assert_eq!(learned[3], 32242);
    }
}
