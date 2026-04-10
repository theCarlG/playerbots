//! Factory `InitSpecialSpells` — mirrors `PlayerbotFactory::InitSpecialSpells`.
//!
//! Iterates the config-driven `randomBotSpellIds` list (default entry on
//! TBC/WotLK: `54197` Cold Weather Flying) and teaches every valid entry
//! to the bot. Entries that do not resolve in the spell store are skipped
//! — matching the C++ guard against `!spellInfo`.

use super::FactoryTransaction;

/// Teach the bot every spell in `sPlayerbotAIConfig.randomBotSpellIds` that
/// resolves to a valid spell entry.
pub fn init_special_spells(tx: &mut FactoryTransaction<'_>) {
    let ids = tx.get_random_bot_spell_ids();
    for &spell_id in &ids {
        if tx.get_spell_info(spell_id).is_none() {
            continue;
        }
        tx.bot_learn_spell(spell_id);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::with_tx;
    use cmangos::{BotSpellInfo, MockEvent, MockWorld};

    fn build_world(config_ids: &[u32], valid_ids: &[u32]) -> MockWorld {
        let mut b = MockWorld::builder().random_bot_spell_ids(config_ids.to_vec());
        for &id in valid_ids {
            let mut info = BotSpellInfo::default();
            info.id = id;
            info.is_valid = true;
            b = b.spell_info(id, info);
        }
        b.build()
    }

    fn learned(world: &MockWorld) -> Vec<u32> {
        world
            .events()
            .iter()
            .filter_map(|e| match e {
                MockEvent::LearnSpell(id) => Some(*id),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn empty_config_is_noop() {
        let mut w = build_world(&[], &[]);
        with_tx(&mut w, |tx| init_special_spells(tx));
        assert!(learned(&w).is_empty());
    }

    #[test]
    fn learns_only_valid_entries() {
        let mut w = build_world(&[1, 2, 3], &[1, 3]);
        with_tx(&mut w, |tx| init_special_spells(tx));
        assert_eq!(learned(&w), vec![1, 3]);
    }

    #[test]
    fn single_entry_config() {
        let mut w = build_world(&[54197], &[54197]);
        with_tx(&mut w, |tx| init_special_spells(tx));
        assert_eq!(learned(&w), vec![54197]);
    }
}
