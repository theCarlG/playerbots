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

/// Zero any trade skill whose current value is exactly 1 — the "learned but
/// never trained" state the factory does not want to carry forward.
///
/// Ports `PlayerbotFactory::UpdateTradeSkills`. The C++ body is
///
/// ```text
/// for (skill in tradeSkills)
///     if (bot->GetSkillValue(skill) == 1)
///         bot->SetSkill(skill, 0, 0, 0);
/// ```
///
/// `SetSkill(id, 0, 0, 0)` with the fourth `step` arg defaulted to 0 is
/// identical to `SetSkill(id, 0, 0)` — which is what our existing
/// `bot_set_skill` FFI already calls — so no new FFI surface is needed.
pub fn update_trade_skills(tx: &mut FactoryTransaction<'_>) {
    for &skill_id in trade_skills() {
        if tx.bot_get_skill_value(skill_id) == 1 {
            tx.bot_set_skill(skill_id, 0, 0);
        }
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

    /* ── update_trade_skills ────────────────────────────────────────── */

    /// Collect every `SetSkill` event whose value is 0 — those are the
    /// "zeroed" skills that `update_trade_skills` is expected to emit.
    fn zeroed(w: &MockWorld) -> Vec<u32> {
        w.events()
            .iter()
            .filter_map(|e| match e {
                MockEvent::SetSkill { skill, value: 0, max: 0 } => Some(*skill),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn update_trade_skills_zeros_value_one_entries() {
        // Seed two trade skills at value 1 (Alchemy + Fishing) and one at a
        // trained value (Blacksmithing at 150). The factory should zero only
        // the two "learned but never trained" entries.
        let mut w = MockWorld::builder()
            .skill(SKILL_ALCHEMY, 1, 1)
            .skill(SKILL_FISHING, 1, 1)
            .skill(SKILL_BLACKSMITHING, 150, 225)
            .build();

        with_tx(&mut w, update_trade_skills);

        let z = zeroed(&w);
        assert_eq!(z.len(), 2, "expected two skills to be zeroed, got {z:?}");
        assert!(z.contains(&SKILL_ALCHEMY));
        assert!(z.contains(&SKILL_FISHING));
        assert!(
            !z.contains(&SKILL_BLACKSMITHING),
            "trained skills must not be reset"
        );
    }

    #[test]
    fn update_trade_skills_is_noop_when_no_skills_are_at_one() {
        // Default MockWorld has empty skills → every GetSkillValue returns 0
        // → no SetSkill events should fire.
        let mut w = MockWorld::default();
        with_tx(&mut w, update_trade_skills);

        assert!(
            zeroed(&w).is_empty(),
            "update_trade_skills must not emit events when no skill is at value 1"
        );
    }

    #[test]
    fn update_trade_skills_leaves_zero_valued_skills_alone() {
        // A skill already at value 0 (or absent) should not be rewritten.
        // GetSkillValue returns 0 for unknown skills and the guard is `== 1`,
        // so nothing fires.
        let mut w = MockWorld::builder().skill(SKILL_ALCHEMY, 0, 0).build();
        with_tx(&mut w, update_trade_skills);

        assert!(zeroed(&w).is_empty());
    }
}
