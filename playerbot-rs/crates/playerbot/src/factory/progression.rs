//! Progression wipe — clears trade skills, the spellbook, and the quest log
//! so the factory can re-roll a bot's character progression from scratch.
//!
//! Corresponds to the C++ `PlayerbotFactory::ClearSkills`, `ClearSpells`,
//! and `ResetQuests` methods. The policy here is the fixed list of trade
//! skills to wipe; the actual game-state mutations are delegated back
//! through the `World` callbacks.

use super::FactoryTransaction;

// ── Skill IDs ─────────────────────────────────────────────────────────────
//
// Values mirror `SharedDefines.h` in the CMaNGOS core. Kept here as plain
// constants so the policy layer stays self-contained and testable.

const SKILL_FIRST_AID: u32 = 129;
const SKILL_BLACKSMITHING: u32 = 164;
const SKILL_LEATHERWORKING: u32 = 165;
const SKILL_ALCHEMY: u32 = 171;
const SKILL_HERBALISM: u32 = 182;
const SKILL_COOKING: u32 = 185;
const SKILL_MINING: u32 = 186;
const SKILL_TAILORING: u32 = 197;
const SKILL_ENGINEERING: u32 = 202;
const SKILL_ENCHANTING: u32 = 333;
const SKILL_FISHING: u32 = 356;
const SKILL_SKINNING: u32 = 393;
#[cfg(not(feature = "vanilla"))]
const SKILL_JEWELCRAFTING: u32 = 755;

/// Trade skills cleared by the factory before re-rolling professions.
///
/// Kept as a `fn` rather than a `const` so the `cfg`-gated TBC/WotLK
/// jewelcrafting entry can be appended without a `#[cfg]` inside a
/// const array literal.
pub fn trade_skills() -> &'static [u32] {
    #[cfg(feature = "vanilla")]
    {
        &[
            SKILL_ALCHEMY,
            SKILL_ENCHANTING,
            SKILL_SKINNING,
            SKILL_TAILORING,
            SKILL_LEATHERWORKING,
            SKILL_ENGINEERING,
            SKILL_HERBALISM,
            SKILL_MINING,
            SKILL_BLACKSMITHING,
            SKILL_COOKING,
            SKILL_FIRST_AID,
            SKILL_FISHING,
        ]
    }
    #[cfg(not(feature = "vanilla"))]
    {
        &[
            SKILL_ALCHEMY,
            SKILL_ENCHANTING,
            SKILL_SKINNING,
            SKILL_TAILORING,
            SKILL_LEATHERWORKING,
            SKILL_ENGINEERING,
            SKILL_HERBALISM,
            SKILL_MINING,
            SKILL_BLACKSMITHING,
            SKILL_COOKING,
            SKILL_FIRST_AID,
            SKILL_FISHING,
            SKILL_JEWELCRAFTING,
        ]
    }
}

/// Clear every trade skill the factory knows about.
///
/// Mirrors `PlayerbotFactory::ClearSkills`. The two `PLAYER_SKILL_INDEX(0/1)`
/// slot resets from the original are intentionally omitted — the original
/// call site is commented out upstream, and those writes are a `WoW` internal
/// detail that does not belong on the Rust side of the FFI.
pub fn clear_trade_skills(tx: &mut FactoryTransaction<'_>) {
    for &skill_id in trade_skills() {
        tx.bot_clear_skill(skill_id);
    }
}

/// Reset the bot's spellbook to class defaults.
///
/// Mirrors `PlayerbotFactory::ClearSpells`. Delegates straight to `CMaNGOS`'
/// `Player::resetSpells()` via the callback — there is no Rust-side policy.
pub fn clear_spells(tx: &mut FactoryTransaction<'_>) {
    tx.bot_reset_spells();
}

/// Drop every quest in the bot's log and wipe the rewarded-quest state.
///
/// Mirrors `PlayerbotFactory::ResetQuests`. Again a pure delegation: the
/// iteration over `sObjectMgr.GetQuestTemplates()` and the DB delete happen
/// on the C++ side because that data is not available to Rust.
pub fn reset_all_quests(tx: &mut FactoryTransaction<'_>) {
    tx.bot_reset_all_quests();
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::with_tx;
    use cmangos::{MockEvent, MockWorld};

    fn cleared(w: &MockWorld) -> Vec<u32> {
        w.events()
            .iter()
            .filter_map(|e| match e {
                MockEvent::ClearSkill(id) => Some(*id),
                _ => None,
            })
            .collect()
    }

    fn count(w: &MockWorld, target: MockEvent) -> u32 {
        w.events().iter().filter(|e| **e == target).count() as u32
    }

    #[test]
    fn clear_trade_skills_calls_every_skill_exactly_once() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| clear_trade_skills(tx));

        let c = cleared(&w);
        assert_eq!(c.len(), trade_skills().len());
        for &skill in trade_skills() {
            assert!(c.contains(&skill), "missing clear for skill {skill}");
        }
    }

    #[test]
    fn trade_skills_list_is_unique() {
        let list = trade_skills();
        let mut sorted: Vec<u32> = list.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            list.len(),
            "duplicate skill id in trade_skills()"
        );
    }

    #[test]
    fn trade_skills_includes_core_professions() {
        let list = trade_skills();
        assert!(list.contains(&SKILL_ALCHEMY));
        assert!(list.contains(&SKILL_ENCHANTING));
        assert!(list.contains(&SKILL_FIRST_AID));
        assert!(list.contains(&SKILL_FISHING));
    }

    #[test]
    fn clear_spells_delegates_once() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| {
            clear_spells(tx);
            clear_spells(tx);
        });
        assert_eq!(count(&w, MockEvent::ResetSpells), 2);
    }

    #[test]
    fn reset_all_quests_delegates_once() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| reset_all_quests(tx));
        assert_eq!(count(&w, MockEvent::ResetAllQuests), 1);
    }
}
