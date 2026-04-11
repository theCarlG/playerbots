//! Quest bulk-rewarder — port of `PlayerbotFactory::InitQuests`.
//!
//! The C++ body iterates a list of quest IDs, filters each by class/race/
//! min-level eligibility, then marks the quest complete and hands the bot
//! the reward. The policy here is that exact loop; the per-quest predicate
//! (`quest_is_eligible_for_bot`) and mutation (`bot_reward_quest_complete`)
//! live behind FFI callbacks so the quest template lookup stays on the C++
//! side.

use super::FactoryTransaction;

/// Reward every quest in `quest_ids` that the bot is eligible for.
///
/// Mirrors `PlayerbotFactory::InitQuests(std::list<uint32>&)`:
///
/// ```text
/// for (questId in questMap) {
///     Quest const* q = sObjectMgr.GetQuestTemplate(questId);
///     if (!bot->SatisfyQuestClass(q,false) ||
///         q->GetMinLevel() > bot->GetLevel() ||
///         !bot->SatisfyQuestRace(q,false))
///         continue;
///     bot->SetQuestStatus(questId, QUEST_STATUS_COMPLETE);
///     bot->RewardQuest(q, 0, bot, false);
/// }
/// ```
///
/// The three predicate checks collapse into the single
/// `quest_is_eligible_for_bot` callback (the C++ bridge does the lookup and
/// the boolean conjunction); the two-call mutation folds into
/// `bot_reward_quest_complete`. No error handling is needed — unknown or
/// ineligible quests are silently skipped.
pub fn init_quests(tx: &mut FactoryTransaction<'_>, quest_ids: &[u32]) {
    for &quest_id in quest_ids {
        if tx.quest_is_eligible_for_bot(quest_id) {
            tx.bot_reward_quest_complete(quest_id);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::with_tx;
    use cmangos::{MockEvent, MockWorld};

    /// Collect every quest id the factory rewarded.
    fn rewarded(w: &MockWorld) -> Vec<u32> {
        w.events()
            .iter()
            .filter_map(|e| match e {
                MockEvent::RewardQuestComplete(id) => Some(*id),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn init_quests_rewards_every_eligible_quest() {
        let mut w = MockWorld::builder()
            .eligible_quest(1)
            .eligible_quest(2)
            .eligible_quest(3)
            .build();

        with_tx(&mut w, |tx| init_quests(tx, &[1, 2, 3]));

        assert_eq!(rewarded(&w), vec![1, 2, 3]);
    }

    #[test]
    fn init_quests_skips_ineligible_entries() {
        // Quest 2 is not in the eligibility set — it must be skipped while
        // quests 1 and 3 (both eligible) are still rewarded.
        let mut w = MockWorld::builder()
            .eligible_quest(1)
            .eligible_quest(3)
            .build();

        with_tx(&mut w, |tx| init_quests(tx, &[1, 2, 3]));

        assert_eq!(rewarded(&w), vec![1, 3]);
    }

    #[test]
    fn init_quests_is_noop_on_empty_input() {
        let mut w = MockWorld::builder().eligible_quest(1).build();
        with_tx(&mut w, |tx| init_quests(tx, &[]));
        assert!(rewarded(&w).is_empty());
    }

    #[test]
    fn init_quests_is_noop_when_nothing_eligible() {
        // Default MockWorld has no eligible quests → every predicate returns
        // false → no RewardQuestComplete events.
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_quests(tx, &[10, 20, 30]));
        assert!(rewarded(&w).is_empty());
    }

    #[test]
    fn init_quests_preserves_input_order() {
        // Order matters because the C++ list is pre-sorted by dependency via
        // AddPrevQuests; the Rust loop must not reorder.
        let mut w = MockWorld::builder()
            .eligible_quest(7)
            .eligible_quest(4)
            .eligible_quest(9)
            .build();

        with_tx(&mut w, |tx| init_quests(tx, &[9, 7, 4]));

        assert_eq!(rewarded(&w), vec![9, 7, 4]);
    }
}
