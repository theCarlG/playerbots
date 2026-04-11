//! Factory `InitTradeSkills` — assigns two profession trade skills to a
//! bot, seeds the three universal secondary skills (first aid, fishing,
//! cooking), learns the armorsmith spell chain for TBC+ plate classes,
//! and then delegates the trainer-iteration loop back to C++.
//!
//! Mirrors `PlayerbotFactory::InitTradeSkills` in
//! `playerbot/PlayerbotFactory.cpp`. The policy layer here is:
//!
//!   1. Look up the cached `(firstSkill, secondSkill)` pair from the
//!      per-bot KV store. When either side is missing, pick a new pair
//!      using the class-based initial pools, falling back to a 4-case
//!      generic pool for classes with no class-specific preference, and
//!      persist the result.
//!   2. Call `SetRandomSkill` for first aid, fishing, cooking, and the
//!      two professions. This uses the exact same upgrade-or-seed
//!      logic as `skills::init_skills` so weapon/armor and profession
//!      policies share one code path.
//!   3. Teach the armorsmith/weaponsmith/hammersmith/swordsmith/
//!      axesmith spell chain to plate-class bots (TBC+ only).
//!   4. Dispatch `factory_learn_tradeskill_recipes()` — the opaque
//!      trainer-iteration callback that walks sCreatureStorage and
//!      teaches any recipe the bot currently qualifies for. Kept in
//!      C++ because that loop touches several game-data surfaces
//!      (`sSpellTemplate`, `sSkillLineStore`, `TrainerSpellData`) that are
//!      not plumbed through the vtable.
//!
//! `init_all_skills` is the public entry point for the combined
//! `InitSkills + InitTradeSkills` call that `PlayerbotFactory::
//! InitAllSkills` used to run. The two halves stay separate because
//! `InitSkills` (weapons/armor/riding) is class-pure while
//! `InitTradeSkills` needs KV lookups + the opaque trainer loop.

use super::{FactoryTransaction, skills};

// ── Skill IDs (mirror SharedDefines.h) ────────────────────────────────────

const SKILL_FIRST_AID: u32 = 129;
const SKILL_BLACKSMITHING: u32 = 164;
const SKILL_LEATHERWORKING: u32 = 165;
const SKILL_ALCHEMY: u32 = 171;
const SKILL_HERBALISM: u32 = 182;
const SKILL_COOKING: u32 = 185;
const SKILL_MINING: u32 = 186;
const SKILL_ENGINEERING: u32 = 202;
const SKILL_FISHING: u32 = 356;
const SKILL_SKINNING: u32 = 393;
#[cfg(not(feature = "vanilla"))]
const SKILL_JEWELCRAFTING: u32 = 755;

// ── Class IDs ─────────────────────────────────────────────────────────────

const CLASS_WARRIOR: u8 = 1;
const CLASS_PALADIN: u8 = 2;
const CLASS_HUNTER: u8 = 3;
const CLASS_ROGUE: u8 = 4;
#[cfg(feature = "wotlk")]
const CLASS_DEATH_KNIGHT: u8 = 6;
const CLASS_SHAMAN: u8 = 7;
const CLASS_DRUID: u8 = 11;

// ── KV keys (match the C++ side) ──────────────────────────────────────────

const KV_FIRST_SKILL: &str = "firstSkill";
const KV_SECOND_SKILL: &str = "secondSkill";

// ── TBC+ armorsmith spell chain ───────────────────────────────────────────
//
// Learned by plate classes once `InitTradeSkills` has assigned their
// professions. The 9788/9787/17040/17039/17041 ids match the C++ source
// verbatim (note that 9788 is listed twice there — a bug preserved for
// behavioural parity).
#[cfg(not(feature = "vanilla"))]
const ARMORSMITH_CHAIN: &[u32] = &[
    9788,  // armorsmith
    9788,  // armorsmith (duplicated in C++)
    9787,  // weaponsmith
    17040, // hammersmith
    17039, // swordsmith
    17041, // axesmith
];

/// Port of `PlayerbotFactory::InitAllSkills` — runs `InitSkills` then
/// `InitTradeSkills`. The two halves are kept as separate functions so
/// the `MiscKind::InitSkills` dispatcher can still drive `InitSkills`
/// on its own.
pub fn init_all_skills(tx: &mut FactoryTransaction<'_>, class_id: u8, level: u32) {
    skills::init_skills(tx, class_id, level);
    init_trade_skills(tx, class_id, level);
}

/// Port of `PlayerbotFactory::InitTradeSkills`. Takes `class_id` and
/// `level` from the caller because the dispatcher already snapshots
/// them once at entry.
pub fn init_trade_skills(tx: &mut FactoryTransaction<'_>, class_id: u8, level: u32) {
    let (first_skill, second_skill) = pick_professions(tx, class_id);

    // Five universal/role skills: three "everyone gets these" secondaries
    // plus the two professions picked above.
    skills::set_random_skill(tx, SKILL_FIRST_AID, level);
    skills::set_random_skill(tx, SKILL_FISHING, level);
    skills::set_random_skill(tx, SKILL_COOKING, level);
    skills::set_random_skill(tx, first_skill, level);
    skills::set_random_skill(tx, second_skill, level);

    // TBC+ armorsmith spell chain — taught unconditionally to plate
    // classes. Vanilla has no equivalent path (the specialisations were
    // added in TBC), hence the cfg gate.
    #[cfg(not(feature = "vanilla"))]
    {
        if is_plate_class(class_id) {
            for &spell_id in ARMORSMITH_CHAIN {
                tx.bot_learn_spell(spell_id);
            }
        }
    }
    // `class_id` is only consulted by the TBC+ block above, so the
    // vanilla build warns about an unused param without this hint.
    #[cfg(feature = "vanilla")]
    let _ = class_id;

    // Opaque trainer loop: walk sCreatureStorage and learn every
    // tradeskill recipe the bot currently qualifies for. See
    // `BotBridge::CB_FactoryLearnTradeskillRecipes` for the body.
    tx.factory_learn_tradeskill_recipes();
}

/// Look up (or pick + persist) the two professions assigned to this bot.
///
/// Mirrors the cache-first branch in `PlayerbotFactory::InitTradeSkills`:
/// when the KV store already holds both ids the caller reuses them
/// unmodified. Otherwise a new pair is rolled, persisted via
/// `factory_kv_set_u32`, and returned.
fn pick_professions(tx: &mut FactoryTransaction<'_>, class_id: u8) -> (u32, u32) {
    let cached_first = tx.factory_kv_get_u32(KV_FIRST_SKILL);
    let cached_second = tx.factory_kv_get_u32(KV_SECOND_SKILL);

    // Matches the C++ `if (!firstSkill || !secondSkill)` gate — both
    // have to be non-zero for the cache to count as populated.
    if cached_first != 0 && cached_second != 0 {
        return (cached_first, cached_second);
    }

    let (first, second) = roll_new_profession_pair(tx, class_id);

    // Persist back so subsequent re-rolls of this bot pick the same
    // pair. The mock records the set as a `FactoryKvSet` event.
    tx.factory_kv_set_u32(KV_FIRST_SKILL, first);
    tx.factory_kv_set_u32(KV_SECOND_SKILL, second);

    (first, second)
}

/// Roll a new `(first_skill, second_skill)` pair for `class_id`.
///
/// The class-specific pools are: warrior/paladin/DK get
/// blacksmithing+engineering, and shaman/druid/hunter/rogue get
/// skinning-or-engineering + leatherworking. Every other class (or any
/// class whose pools are empty) falls through to the generic 4-case
/// fallback — which reproduces the C++ `urand(0, 6)` roll with cases
/// 4..=6 intentionally producing the default `(0, 0)` pair.
fn roll_new_profession_pair(tx: &mut FactoryTransaction<'_>, class_id: u8) -> (u32, u32) {
    let (first_pool, second_pool) = class_profession_pools(class_id);

    if !first_pool.is_empty() && !second_pool.is_empty() {
        let first = first_pool[random_index(tx, first_pool.len())];
        let second = second_pool[random_index(tx, second_pool.len())];
        return (first, second);
    }

    // Generic fallback. The original uses `urand(0, 6)` and silently
    // falls through cases 4..=6, leaving `firstSkill/secondSkill` at 0.
    // The `SetSkill(0, ...)` path that results is safe because
    // `set_random_skill` short-circuits on skill id 0 inside CMaNGOS.
    match tx.random_u32(0, 6) {
        0 => (SKILL_HERBALISM, SKILL_ALCHEMY),
        1 => (SKILL_HERBALISM, SKILL_MINING),
        2 => (SKILL_MINING, SKILL_SKINNING),
        3 => {
            #[cfg(feature = "vanilla")]
            {
                (SKILL_HERBALISM, SKILL_SKINNING)
            }
            #[cfg(not(feature = "vanilla"))]
            {
                (SKILL_JEWELCRAFTING, SKILL_MINING)
            }
        }
        // Cases 4/5/6 in the C++ switch fall through — no assignment
        // happens, so both skills stay at their uninitialised 0.
        _ => (0, 0),
    }
}

/// Class-specific initial profession pools. Returns two empty slices
/// when the class has no preference — the caller then falls back to
/// the generic roll.
fn class_profession_pools(class_id: u8) -> (&'static [u32], &'static [u32]) {
    match class_id {
        CLASS_WARRIOR | CLASS_PALADIN => (&[SKILL_BLACKSMITHING], &[SKILL_ENGINEERING]),

        #[cfg(feature = "wotlk")]
        CLASS_DEATH_KNIGHT => (&[SKILL_BLACKSMITHING], &[SKILL_ENGINEERING]),

        CLASS_SHAMAN | CLASS_DRUID | CLASS_HUNTER | CLASS_ROGUE => {
            (&[SKILL_SKINNING, SKILL_ENGINEERING], &[SKILL_LEATHERWORKING])
        }
        _ => (&[], &[]),
    }
}

/// True when `class_id` is a plate-wearing class that gets the TBC+
/// armorsmith spell chain. Vanilla has no plate specialisations.
#[cfg(not(feature = "vanilla"))]
fn is_plate_class(class_id: u8) -> bool {
    match class_id {
        CLASS_WARRIOR | CLASS_PALADIN => true,
        #[cfg(feature = "wotlk")]
        CLASS_DEATH_KNIGHT => true,
        _ => false,
    }
}

/// `urand(0, len - 1)` — pick a uniform index into a non-empty slice.
/// Empty slices are rejected at the call site so this is unreachable
/// for `len == 0`, but we still clamp to 0 to avoid a panic in release.
///
/// The modulo on the return value is defensive: the real `CMaNGOS`
/// `urand` never exceeds its `max`, but the test mock's `random_u32`
/// ignores bounds and returns `rng_default` verbatim. Clamping here
/// keeps both production and test paths safe without coupling the
/// policy to the mock's behaviour.
fn random_index(tx: &mut FactoryTransaction<'_>, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let hi = u32::try_from(len - 1).unwrap_or(u32::MAX);
    (tx.random_u32(0, hi) as usize) % len
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::with_tx;
    use cmangos::{MockEvent, MockWorld};

    /* helpers */

    fn set_skill_values(events: &[MockEvent]) -> Vec<(u32, u32, u32)> {
        events
            .iter()
            .filter_map(|e| match e {
                MockEvent::SetSkill { skill, value, max } => Some((*skill, *value, *max)),
                _ => None,
            })
            .collect()
    }

    fn kv_sets(events: &[MockEvent]) -> Vec<(String, u32)> {
        events
            .iter()
            .filter_map(|e| match e {
                MockEvent::FactoryKvSet { key, value } => Some((key.clone(), *value)),
                _ => None,
            })
            .collect()
    }

    fn learned_spells(events: &[MockEvent]) -> Vec<u32> {
        events
            .iter()
            .filter_map(|e| match e {
                MockEvent::LearnSpell(id) => Some(*id),
                _ => None,
            })
            .collect()
    }

    /* pick_professions */

    #[test]
    fn cached_pair_short_circuits_without_writing_kv() {
        let mut w = MockWorld::builder()
            .kv(KV_FIRST_SKILL, SKILL_HERBALISM)
            .kv(KV_SECOND_SKILL, SKILL_ALCHEMY)
            .random_default(0)
            .build();

        let got = with_tx(&mut w, |tx| pick_professions(tx, CLASS_WARRIOR));
        assert_eq!(got, (SKILL_HERBALISM, SKILL_ALCHEMY));

        // No new KV writes — the cache path must not persist anything.
        let sets = kv_sets(&w.events());
        assert!(
            sets.is_empty(),
            "cached pair should not trigger KV writes, got {sets:?}"
        );
    }

    #[test]
    fn missing_cache_picks_and_persists_warrior_pair() {
        let mut w = MockWorld::builder().random_default(0).build();

        let (first, second) = with_tx(&mut w, |tx| pick_professions(tx, CLASS_WARRIOR));
        assert_eq!(first, SKILL_BLACKSMITHING);
        assert_eq!(second, SKILL_ENGINEERING);

        let sets = kv_sets(&w.events());
        assert!(sets.contains(&(KV_FIRST_SKILL.to_string(), SKILL_BLACKSMITHING)));
        assert!(sets.contains(&(KV_SECOND_SKILL.to_string(), SKILL_ENGINEERING)));
    }

    #[test]
    fn partial_cache_still_rerolls_both_sides() {
        // First skill cached, second not: the cache gate is `both != 0`,
        // so we have to roll a fresh pair — not "reuse first, roll only
        // second". This matches the C++ `if (!firstSkill || !secondSkill)`.
        let mut w = MockWorld::builder()
            .kv(KV_FIRST_SKILL, SKILL_HERBALISM)
            .random_default(0)
            .build();

        let (first, second) = with_tx(&mut w, |tx| pick_professions(tx, CLASS_WARRIOR));
        assert_eq!(first, SKILL_BLACKSMITHING);
        assert_eq!(second, SKILL_ENGINEERING);

        // KV must have been rewritten with the new pair.
        let sets = kv_sets(&w.events());
        assert_eq!(sets.len(), 2);
    }

    #[test]
    fn rogue_rolls_leather_pair() {
        let mut w = MockWorld::builder().random_default(0).build();

        let (first, second) = with_tx(&mut w, |tx| pick_professions(tx, CLASS_ROGUE));
        // `random_default(0)` picks index 0 of `[skinning, engineering]`.
        assert_eq!(first, SKILL_SKINNING);
        assert_eq!(second, SKILL_LEATHERWORKING);
    }

    #[test]
    fn mage_falls_back_to_generic_pool_case_zero() {
        // Class 8 = mage → no class-specific pools → falls through to
        // the 4-case generic roll. `random_default(0)` lands on case 0
        // → herbalism + alchemy.
        const CLASS_MAGE: u8 = 8;
        let mut w = MockWorld::builder().random_default(0).build();

        let (first, second) = with_tx(&mut w, |tx| pick_professions(tx, CLASS_MAGE));
        assert_eq!(first, SKILL_HERBALISM);
        assert_eq!(second, SKILL_ALCHEMY);
    }

    #[test]
    fn generic_fallback_case_four_zeros_both_skills() {
        // Case 4 through 6 in the C++ switch intentionally fall through —
        // `firstSkill` and `secondSkill` stay at their 0 initialisation.
        const CLASS_MAGE: u8 = 8;
        let mut w = MockWorld::builder().random_default(4).build();

        let (first, second) = with_tx(&mut w, |tx| pick_professions(tx, CLASS_MAGE));
        assert_eq!(first, 0);
        assert_eq!(second, 0);
    }

    /* init_trade_skills (full path) */

    #[test]
    fn init_trade_skills_sets_five_universal_skills() {
        // Warrior, level 40 → blacksmithing + engineering + the three
        // secondaries. Every `set_random_skill` call must translate
        // into a `SetSkill` event because the mock has no prior values.
        let mut w = MockWorld::builder().random_default(5).build();

        with_tx(&mut w, |tx| init_trade_skills(tx, CLASS_WARRIOR, 40));

        let skills_set: Vec<u32> =
            set_skill_values(&w.events()).into_iter().map(|(s, _, _)| s).collect();
        assert!(skills_set.contains(&SKILL_FIRST_AID));
        assert!(skills_set.contains(&SKILL_FISHING));
        assert!(skills_set.contains(&SKILL_COOKING));
        assert!(skills_set.contains(&SKILL_BLACKSMITHING));
        assert!(skills_set.contains(&SKILL_ENGINEERING));
    }

    #[test]
    fn init_trade_skills_invokes_opaque_trainer_callback() {
        let mut w = MockWorld::builder().random_default(0).build();
        with_tx(&mut w, |tx| init_trade_skills(tx, CLASS_WARRIOR, 40));

        let trainer_calls = w
            .events()
            .iter()
            .filter(|e| matches!(e, MockEvent::LearnTradeskillRecipes))
            .count();
        assert_eq!(
            trainer_calls, 1,
            "init_trade_skills must dispatch the opaque trainer-loop callback exactly once"
        );
    }

    #[cfg(not(feature = "vanilla"))]
    #[test]
    fn tbc_plate_classes_learn_armorsmith_chain() {
        let mut w = MockWorld::builder().random_default(0).build();
        with_tx(&mut w, |tx| init_trade_skills(tx, CLASS_WARRIOR, 60));

        let learned = learned_spells(&w.events());
        // All five distinct ids from the chain must be learned.
        for &expected in &[9788u32, 9787, 17040, 17039, 17041] {
            assert!(
                learned.contains(&expected),
                "plate class should learn spell {expected}, got {learned:?}"
            );
        }
    }

    #[cfg(not(feature = "vanilla"))]
    #[test]
    fn tbc_non_plate_class_skips_armorsmith_chain() {
        const CLASS_MAGE: u8 = 8;
        let mut w = MockWorld::builder().random_default(0).build();
        with_tx(&mut w, |tx| init_trade_skills(tx, CLASS_MAGE, 60));

        let learned = learned_spells(&w.events());
        for &forbidden in &[9788u32, 9787, 17040, 17039, 17041] {
            assert!(
                !learned.contains(&forbidden),
                "non-plate class must not learn armorsmith spell {forbidden}"
            );
        }
    }

    #[cfg(feature = "vanilla")]
    #[test]
    fn vanilla_plate_class_does_not_learn_armorsmith_chain() {
        // Vanilla has no specialisations — the TBC+ block must be cfg'd
        // out entirely and a plate class should learn nothing here.
        let mut w = MockWorld::builder().random_default(0).build();
        with_tx(&mut w, |tx| init_trade_skills(tx, CLASS_WARRIOR, 60));

        let learned = learned_spells(&w.events());
        assert!(
            learned.is_empty(),
            "vanilla init_trade_skills should not learn any spells, got {learned:?}"
        );
    }

    /* init_all_skills */

    #[test]
    fn init_all_skills_runs_init_skills_pass() {
        // `init_skills` ends with an `UpdateSkillsForLevel` tick at the
        // start; asserting on that proves the combined entry point
        // invokes both halves in order.
        let mut w = MockWorld::builder().random_default(0).build();
        with_tx(&mut w, |tx| init_all_skills(tx, CLASS_WARRIOR, 40));

        assert!(
            w.events()
                .iter()
                .any(|e| matches!(e, MockEvent::UpdateSkillsForLevel)),
            "init_all_skills must invoke init_skills' UpdateSkillsForLevel"
        );
        assert!(
            w.events()
                .iter()
                .any(|e| matches!(e, MockEvent::LearnTradeskillRecipes)),
            "init_all_skills must invoke init_trade_skills' trainer callback"
        );
    }
}
