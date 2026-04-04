/// Non-combat behavior — follow, buff, eat/drink, loot.
///
/// These subtrees run when the bot is not in combat.  They are composed
/// into a single Selector and plugged at the top of the root tree (above
/// the class combat subtree) in `bot::init`.
pub mod buffing;
pub mod consumables;
pub mod follow;
pub mod looting;

pub use buffing::{BuffTarget, GroupBuff};

use crate::engine::bt_nodes::{BtNode, sel};

/// Compose the complete non-combat Selector.
///
/// Priority (highest first):
///   1. Eat/drink — stop and recover HP/mana.
///   2. Buff group — apply missing buffs to party/self.
///   3. Loot — open nearby lootable corpses (stub).
///   4. Follow leader — keep up with the group.
///
/// `buffs` is the spec-specific list of buffs to maintain.
/// Pass an empty Vec for specs that don't contribute group buffs (rogues, etc.).
pub fn build_noncombat_subtree(buffs: Vec<GroupBuff>) -> Box<dyn BtNode> {
    sel(vec![
        consumables::build_consumables_subtree(),
        buffing::build_buff_subtree(buffs),
        looting::build_loot_subtree(),
        follow::build_follow_subtree(),
    ])
}

// ── Buff lists per class / spec ───────────────────────────────────────────

/// Warrior buffs — Battle Shout for the whole party.
pub fn warrior_buffs() -> Vec<GroupBuff> {
    use crate::data::spells::vanilla::warrior::*;
    vec![
        GroupBuff::on_party(BATTLE_SHOUT),
    ]
}

/// Paladin buffs — spec-appropriate blessing + appropriate aura.
pub fn paladin_retribution_buffs() -> Vec<GroupBuff> {
    use crate::data::spells::vanilla::paladin::*;
    vec![
        GroupBuff::on_party(BLESSING_OF_MIGHT),
        GroupBuff::on_self(RETRIBUTION_AURA),
    ]
}

pub fn paladin_holy_buffs() -> Vec<GroupBuff> {
    use crate::data::spells::vanilla::paladin::*;
    vec![
        GroupBuff::on_party(BLESSING_OF_WISDOM),
        GroupBuff::on_self(DEVOTION_AURA),
    ]
}

pub fn paladin_protection_buffs() -> Vec<GroupBuff> {
    use crate::data::spells::vanilla::paladin::*;
    vec![
        GroupBuff::on_party(BLESSING_OF_KINGS),
        GroupBuff::on_self(DEVOTION_AURA),
    ]
}

/// Priest buffs — Power Word: Fortitude for everyone.
pub fn priest_buffs() -> Vec<GroupBuff> {
    use crate::data::spells::vanilla::priest::*;
    vec![
        GroupBuff::on_party(POWER_WORD_FORTITUDE),
        GroupBuff::on_self(INNER_FIRE),
    ]
}

/// Druid buffs — Mark of the Wild for everyone.
pub fn druid_buffs() -> Vec<GroupBuff> {
    use crate::data::spells::vanilla::druid::*;
    vec![
        // Mark of the Wild: spell 9885 (rank 6 vanilla)
        GroupBuff { spell_id: crate::ffi::SpellId(9885), aura_id: crate::ffi::SpellId(9885), target: BuffTarget::AnyMember },
    ]
}

/// Mage buffs — Arcane Intellect / Arcane Brilliance for the party.
pub fn mage_buffs() -> Vec<GroupBuff> {
    use crate::data::spells::vanilla::mage::*;
    vec![
        // Use Arcane Brilliance when in a group, Arcane Intellect otherwise.
        // Both share the same aura (10157 = highest-rank Arcane Intellect aura).
        GroupBuff::on_party_aura(ARCANE_BRILLIANCE, ARCANE_INTELLECT),
    ]
}

/// Shaman buffs — appropriate totem set.
/// Actual totem drop is handled in the combat rotation; this just
/// ensures Windfury is up for melee or Grace of Air for casters.
pub fn shaman_enhancement_buffs() -> Vec<GroupBuff> {
    use crate::data::spells::vanilla::shaman::*;
    vec![
        GroupBuff::on_self(LIGHTNING_SHIELD),
    ]
}

pub fn shaman_elemental_buffs() -> Vec<GroupBuff> { vec![] }
pub fn shaman_restoration_buffs() -> Vec<GroupBuff> { vec![] }

/// Warlock — Demon Armor on self; summon pet if it's dead.
pub fn warlock_buffs() -> Vec<GroupBuff> {
    use crate::data::spells::vanilla::warlock::*;
    vec![
        GroupBuff::on_self(DEMON_ARMOR),
    ]
}

/// Specs with no persistent group buffs.
pub fn no_buffs() -> Vec<GroupBuff> { vec![] }
