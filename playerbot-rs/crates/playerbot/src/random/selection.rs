//! Probability-based class/race selection and name postfix helpers.
//!
//! Ports the three pure functions that `RandomPlayerbotFactory` used to
//! pick a class/race combination for a new random bot, plus the
//! `GetNamePostFix` bijective-base-26 helper used by
//! `CreateRandomBots`' name-pool expansion pass.
//!
//! The functions take an explicit [`FactoryRng`] so tests can drive them
//! deterministically; the production path uses [`VtableRng`] which
//! forwards to the C++ `urand_range` callback.

use crate::config::typed::{BotConfig, MAX_CLASSES, MAX_RACES};
use crate::random::races::first_available_race;

/// RNG abstraction matching the `urand(min, max)` helper in `CMaNGOS` —
/// both bounds are **inclusive**.
///
/// Production code uses [`VtableRng`] which forwards to the
/// `RandomFactoryCallbacks::urand_range` function pointer. Tests use
/// [`SequentialRng`] which replays a pre-built queue of return values
/// and falls back to `min` once exhausted.
pub trait FactoryRng {
    /// Return a pseudo-random value in `[min, max]` inclusive.
    fn urand_range(&mut self, min: u32, max: u32) -> u32;
}

/// Replays a prepared sequence of values; once the sequence is drained
/// every subsequent call returns `min`. Used exclusively by unit tests
/// to make selection behaviour deterministic.
#[derive(Default)]
pub struct SequentialRng {
    values: Vec<u32>,
    cursor: usize,
}

impl SequentialRng {
    /// Build a [`SequentialRng`] that will return `values[0]` then
    /// `values[1]` and so on, wrapping to `min` after the sequence is
    /// exhausted.
    #[must_use]
    pub fn new(values: impl Into<Vec<u32>>) -> Self {
        Self {
            values: values.into(),
            cursor: 0,
        }
    }
}

impl FactoryRng for SequentialRng {
    fn urand_range(&mut self, min: u32, _max: u32) -> u32 {
        if let Some(v) = self.values.get(self.cursor).copied() {
            self.cursor += 1;
            v
        } else {
            min
        }
    }
}

/// Port of `RandomPlayerbotFactory::GetRandomClass`.
///
/// The class is picked by summing the per-class total of
/// `class_race_probability[cls][1..MAX_RACES]`, then rolling against
/// `class_race_probability_total` (which is the sum over every
/// `[cls][race]` slot, matching the C++ field). The rolling loop
/// subtracts each class bucket in turn and returns the first class
/// whose bucket the random roll lands in; if the roll falls through the
/// entire pass (possible because the total includes unreachable slots),
/// the fallback loop returns the first class with any non-zero bucket,
/// defaulting to `CLASS_WARRIOR` (1) as a last resort.
///
/// The subtract-through-the-bucket pattern mirrors the C++ uint32 wrap
/// semantics exactly — no branches have been simplified.
#[must_use]
#[allow(clippy::needless_range_loop)] // index-driven loop mirrors the C++ source verbatim
pub fn get_random_class(cfg: &BotConfig, rng: &mut impl FactoryRng) -> u8 {
    let mut class_prob = [0u32; MAX_CLASSES];
    for race in 1..MAX_RACES {
        for cls in 1..MAX_CLASSES {
            class_prob[cls] = class_prob[cls].wrapping_add(cfg.class_race_probability[cls][race]);
        }
    }

    let mut random_prob = rng.urand_range(0, cfg.class_race_probability_total);

    for cls in 1..MAX_CLASSES {
        if class_prob[cls] > 0 && random_prob < class_prob[cls] {
            return cls as u8;
        }
        random_prob = random_prob.wrapping_sub(class_prob[cls]);
    }

    for cls in 1..MAX_CLASSES {
        if class_prob[cls] > 0 {
            return cls as u8;
        }
    }

    1
}

/// Port of `RandomPlayerbotFactory::GetRandomRace`.
///
/// Uses the per-class row of `class_race_probability`; rolls against
/// that row's sum and returns the race whose bucket the roll lands in.
/// Falls back to [`first_available_race`] when the row is all zero —
/// which in turn mirrors the C++ insertion order of
/// `availableRaces[cls]`, *not* numeric race id order.
#[must_use]
pub fn get_random_race(cfg: &BotConfig, cls: u8, rng: &mut impl FactoryRng) -> u8 {
    let cls_idx = cls as usize;
    if cls_idx >= MAX_CLASSES {
        return first_available_race(cls);
    }

    let mut total_class_prob = 0u32;
    for race in 1..MAX_RACES {
        total_class_prob = total_class_prob.wrapping_add(cfg.class_race_probability[cls_idx][race]);
    }

    let mut random_prob = rng.urand_range(0, total_class_prob);

    for race in 1..MAX_RACES {
        let p = cfg.class_race_probability[cls_idx][race];
        if p > 0 && random_prob < p {
            return race as u8;
        }
        random_prob = random_prob.wrapping_sub(p);
    }

    first_available_race(cls)
}

/// Bijective base-26 postfix generator. Mirrors the inline
/// `GetNamePostFix(int32 nr)` helper in
/// `playerbot/RandomPlayerbotFactory.cpp`.
///
/// Maps `0 → "a"`, `1 → "b"`, …, `25 → "z"`, `26 → "aa"`, `27 → "ab"`, …,
/// `701 → "zz"`, `702 → "aaa"`. Negative inputs return an empty string,
/// matching the C++ loop condition `while (nr >= 0)`.
#[must_use]
pub fn name_postfix(mut nr: i32) -> String {
    let mut ret = String::new();
    while nr >= 0 {
        let idx = (nr % 26) as u8;
        ret.insert(0, (b'a' + idx) as char);
        nr /= 26;
        nr -= 1;
    }
    ret
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::typed::BotConfig;

    fn make_cfg() -> BotConfig {
        // Only fields the selection routines touch matter here; everything
        // else uses the `Default` derive in `typed.rs`.
        let mut cfg = BotConfig::default();
        cfg.class_race_probability = Box::new([[0u32; MAX_RACES]; MAX_CLASSES]);
        cfg
    }

    fn set_prob(cfg: &mut BotConfig, cls: u8, race: u8, value: u32) {
        cfg.class_race_probability[cls as usize][race as usize] = value;
        cfg.class_race_probability_total = cfg
            .class_race_probability
            .iter()
            .flat_map(|row| row.iter().copied())
            .sum();
    }

    #[test]
    fn name_postfix_base_alphabet() {
        assert_eq!(name_postfix(0), "a");
        assert_eq!(name_postfix(1), "b");
        assert_eq!(name_postfix(25), "z");
    }

    #[test]
    fn name_postfix_wraps_to_two_letter() {
        assert_eq!(name_postfix(26), "aa");
        assert_eq!(name_postfix(27), "ab");
        assert_eq!(name_postfix(51), "az");
        assert_eq!(name_postfix(52), "ba");
    }

    #[test]
    fn name_postfix_wraps_to_three_letter() {
        // 701 is the last two-letter postfix (zz); 702 is the first
        // three-letter (aaa).
        assert_eq!(name_postfix(701), "zz");
        assert_eq!(name_postfix(702), "aaa");
    }

    #[test]
    fn name_postfix_negative_is_empty() {
        assert_eq!(name_postfix(-1), "");
        assert_eq!(name_postfix(-100), "");
    }

    #[test]
    fn get_random_class_returns_the_only_populated_class() {
        use crate::random::races::{CLASS_WARRIOR, RACE_HUMAN};
        let mut cfg = make_cfg();
        set_prob(&mut cfg, CLASS_WARRIOR, RACE_HUMAN, 100);

        // urand(0, 100) -> 0 lands inside warrior's bucket.
        let mut rng = SequentialRng::new([0]);
        assert_eq!(get_random_class(&cfg, &mut rng), CLASS_WARRIOR);
    }

    #[test]
    fn get_random_class_falls_through_to_second_class_when_first_is_zero() {
        use crate::random::races::{CLASS_PALADIN, CLASS_WARRIOR, RACE_HUMAN};
        let mut cfg = make_cfg();
        set_prob(&mut cfg, CLASS_WARRIOR, RACE_HUMAN, 0);
        set_prob(&mut cfg, CLASS_PALADIN, RACE_HUMAN, 10);

        // Roll lands in paladin's bucket (warrior has 0 so the loop
        // skips it and paladin is the only non-zero entry).
        let mut rng = SequentialRng::new([0]);
        assert_eq!(get_random_class(&cfg, &mut rng), CLASS_PALADIN);
    }

    #[test]
    fn get_random_class_fallback_when_roll_exceeds_total() {
        use crate::random::races::{CLASS_MAGE, RACE_HUMAN};
        let mut cfg = make_cfg();
        set_prob(&mut cfg, CLASS_MAGE, RACE_HUMAN, 10);

        // Force a roll larger than the total so the first loop never
        // returns; the fallback loop picks mage (the only non-zero).
        let mut rng = SequentialRng::new([u32::MAX]);
        assert_eq!(get_random_class(&cfg, &mut rng), CLASS_MAGE);
    }

    #[test]
    fn get_random_class_last_resort_returns_warrior() {
        // Empty table — both loops fall through, returns 1 (warrior).
        let cfg = make_cfg();
        let mut rng = SequentialRng::new([0]);
        assert_eq!(get_random_class(&cfg, &mut rng), 1);
    }

    #[test]
    fn get_random_race_returns_the_only_populated_race() {
        use crate::random::races::{CLASS_WARRIOR, RACE_HUMAN};
        let mut cfg = make_cfg();
        set_prob(&mut cfg, CLASS_WARRIOR, RACE_HUMAN, 50);

        let mut rng = SequentialRng::new([0]);
        assert_eq!(get_random_race(&cfg, CLASS_WARRIOR, &mut rng), RACE_HUMAN);
    }

    #[test]
    fn get_random_race_fallback_uses_cpp_first_race_order() {
        use crate::random::races::{CLASS_HUNTER, RACE_DWARF};
        let cfg = make_cfg();
        // Empty row → fallback. Hunter's C++ first-inserted race is DWARF,
        // which differs from the numeric-ascending walk (which would pick
        // ORC).
        let mut rng = SequentialRng::new([0]);
        assert_eq!(get_random_race(&cfg, CLASS_HUNTER, &mut rng), RACE_DWARF);
    }

    #[test]
    fn get_random_race_unknown_class_falls_back_to_human() {
        let cfg = make_cfg();
        let mut rng = SequentialRng::new([0]);
        assert_eq!(get_random_race(&cfg, 99, &mut rng), 1); // RACE_HUMAN
    }

    #[test]
    fn get_random_race_distributes_between_two_races() {
        use crate::random::races::{CLASS_PALADIN, RACE_DWARF, RACE_HUMAN};
        let mut cfg = make_cfg();
        set_prob(&mut cfg, CLASS_PALADIN, RACE_HUMAN, 30);
        set_prob(&mut cfg, CLASS_PALADIN, RACE_DWARF, 70);

        // Roll 0 should hit human (bucket [0..30) in race-id order).
        let mut rng = SequentialRng::new([0]);
        assert_eq!(get_random_race(&cfg, CLASS_PALADIN, &mut rng), RACE_HUMAN);
        // Roll 40 should hit dwarf.
        let mut rng = SequentialRng::new([40]);
        assert_eq!(get_random_race(&cfg, CLASS_PALADIN, &mut rng), RACE_DWARF);
    }
}
