/// Aura rank helpers — check if a unit has any rank of a multi-rank spell.
///
/// Many `WoW` spells have multiple ranks (e.g. Rend rank 1-6).  When checking
/// if a DoT/buff is already on a target, we need to check all ranks.
/// This module provides `has_any_rank()` and spell rank tables.
use cmangos::{SpellId, UnitHandle, World};

/// Check if `unit` has any of the given spell IDs as an aura.
#[inline]
pub fn has_any_rank(iface: &dyn World, unit: UnitHandle, ranks: &[SpellId]) -> bool {
    ranks.iter().any(|&id| iface.has_aura(unit, id))
}

// ── Spell rank tables ────────────────────────────────────────────────────
// Each array lists all ranks of a spell, lowest to highest.
// Only spells that are actually checked in multi-rank patterns need entries.

/// Rend (Warrior): ranks 1-6
pub const REND_RANKS: &[SpellId] = &[
    SpellId(772),
    SpellId(6546),
    SpellId(6547),
    SpellId(6548),
    SpellId(11572),
    SpellId(11574),
];

/// Battle Shout (Warrior): ranks 1-7
pub const BATTLE_SHOUT_RANKS: &[SpellId] = &[
    SpellId(6673),
    SpellId(5242),
    SpellId(6192),
    SpellId(11549),
    SpellId(11550),
    SpellId(11551),
    SpellId(25289),
];

/// Demoralizing Shout (Warrior): ranks 1-6
pub const DEMORALIZING_SHOUT_RANKS: &[SpellId] = &[
    SpellId(1160),
    SpellId(6190),
    SpellId(11554),
    SpellId(11555),
    SpellId(11556),
];

/// Faerie Fire (Druid, all forms): feral + caster ranks
pub const FAERIE_FIRE_RANKS: &[SpellId] = &[
    SpellId(770),
    SpellId(778),
    SpellId(9749),
    SpellId(9907),
    SpellId(16857), // Faerie Fire (Feral)
];

/// Serpent Sting (Hunter): ranks 1-7
pub const SERPENT_STING_RANKS: &[SpellId] = &[
    SpellId(1978),
    SpellId(13549),
    SpellId(13550),
    SpellId(13551),
    SpellId(13552),
    SpellId(13553),
    SpellId(13555),
];

/// Hunter's Mark: ranks 1-4
pub const HUNTERS_MARK_RANKS: &[SpellId] = &[
    SpellId(1130),
    SpellId(14323),
    SpellId(14324),
    SpellId(14325),
];

/// Moonfire (Druid): ranks 1-10
pub const MOONFIRE_RANKS: &[SpellId] = &[
    SpellId(8921),
    SpellId(8924),
    SpellId(8925),
    SpellId(8926),
    SpellId(8927),
    SpellId(8928),
    SpellId(8929),
    SpellId(9833),
    SpellId(9834),
    SpellId(26987),
];

/// Insect Swarm (Druid): ranks 1-5
pub const INSECT_SWARM_RANKS: &[SpellId] = &[
    SpellId(5570),
    SpellId(24974),
    SpellId(24975),
    SpellId(24976),
    SpellId(24977),
];

/// Rupture (Rogue): ranks 1-6
pub const RUPTURE_RANKS: &[SpellId] = &[
    SpellId(1943),
    SpellId(8639),
    SpellId(8640),
    SpellId(11273),
    SpellId(11274),
    SpellId(11275),
];

/// Rake (Druid): ranks 1-4
pub const RAKE_RANKS: &[SpellId] = &[SpellId(1822), SpellId(1823), SpellId(1824), SpellId(9904)];

/// Rip (Druid): ranks 1-7
pub const RIP_RANKS: &[SpellId] = &[
    SpellId(1079),
    SpellId(9492),
    SpellId(9493),
    SpellId(9752),
    SpellId(9894),
    SpellId(9896),
];

/// Demoralizing Roar (Druid): ranks 1-5
pub const DEMO_ROAR_RANKS: &[SpellId] = &[
    SpellId(99),
    SpellId(1735),
    SpellId(9490),
    SpellId(9747),
    SpellId(26998),
];

// ── Group buff rank tables ──────────────────────────────────────────────
// Used by the maintenance buff system so that any rank of a buff counts
// as "already present" and prevents infinite rebuffing.

/// Mark of the Wild (Druid): ranks 1-7
pub const MARK_OF_THE_WILD_RANKS: &[SpellId] = &[
    SpellId(1126),
    SpellId(5232),
    SpellId(6756),
    SpellId(5234),
    SpellId(8907),
    SpellId(9884),
    SpellId(9885),
];

/// Power Word: Fortitude (Priest): ranks 1-7
pub const PW_FORTITUDE_RANKS: &[SpellId] = &[
    SpellId(1243),
    SpellId(1244),
    SpellId(1245),
    SpellId(2791),
    SpellId(10937),
    SpellId(10938),
    SpellId(21562),
];

/// Inner Fire (Priest): ranks 1-6
pub const INNER_FIRE_RANKS: &[SpellId] = &[
    SpellId(588),
    SpellId(7128),
    SpellId(602),
    SpellId(1006),
    SpellId(10951),
    SpellId(10952),
];

/// Arcane Intellect (Mage): ranks 1-5 + Arcane Brilliance rank 1
pub const ARCANE_INTELLECT_RANKS: &[SpellId] = &[
    SpellId(1459),
    SpellId(1460),
    SpellId(1461),
    SpellId(10156),
    SpellId(10157),
    SpellId(23028), // Arcane Brilliance
];

/// Demon Armor (Warlock): ranks 1-5 + Demon Skin ranks 1-2
pub const DEMON_ARMOR_RANKS: &[SpellId] = &[
    SpellId(687),   // Demon Skin rank 1
    SpellId(696),   // Demon Skin rank 2
    SpellId(706),   // Demon Armor rank 1
    SpellId(1086),  // rank 2
    SpellId(11733), // rank 3
    SpellId(11734), // rank 4
    SpellId(11735), // rank 5
];

/// Mage armor self-buffs — Frost Armor + Ice Armor + Mage Armor ranks. Used to
/// check whether the mage already has ANY armor up before recasting Frost Armor.
pub const MAGE_ARMOR_RANKS: &[SpellId] = &[
    // Frost Armor
    SpellId(168),
    SpellId(7300),
    SpellId(7301),
    // Ice Armor (replaces Frost Armor at higher level)
    SpellId(7302),
    SpellId(7320),
    SpellId(10219),
    SpellId(10220),
    // Mage Armor
    SpellId(6117),
    SpellId(22782),
    SpellId(22783),
];

/// Lightning Shield (Shaman): ranks 1-7
pub const LIGHTNING_SHIELD_RANKS: &[SpellId] = &[
    SpellId(324),
    SpellId(325),
    SpellId(905),
    SpellId(945),
    SpellId(8134),
    SpellId(10431),
    SpellId(10432),
];

/// Blessing of Might (Paladin): ranks 1-7
pub const BLESSING_OF_MIGHT_RANKS: &[SpellId] = &[
    SpellId(19740),
    SpellId(19834),
    SpellId(19835),
    SpellId(19836),
    SpellId(19837),
    SpellId(19838),
    SpellId(25291),
];

/// Blessing of Wisdom (Paladin): ranks 1-6
pub const BLESSING_OF_WISDOM_RANKS: &[SpellId] = &[
    SpellId(19742),
    SpellId(19850),
    SpellId(19852),
    SpellId(19853),
    SpellId(19854),
    SpellId(25290),
];

/// Blessing of Kings (Paladin): rank 1
pub const BLESSING_OF_KINGS_RANKS: &[SpellId] = &[SpellId(20217)];

/// Devotion Aura (Paladin): ranks 1-7
pub const DEVOTION_AURA_RANKS: &[SpellId] = &[
    SpellId(465),
    SpellId(10290),
    SpellId(643),
    SpellId(10291),
    SpellId(1032),
    SpellId(10292),
    SpellId(10293),
];

/// Retribution Aura (Paladin): ranks 1-5
pub const RETRIBUTION_AURA_RANKS: &[SpellId] = &[
    SpellId(7294),
    SpellId(10298),
    SpellId(10299),
    SpellId(10300),
    SpellId(10301),
];

#[cfg(test)]
mod tests {
    use super::*;
    use cmangos::MockWorld;

    #[test]
    fn has_any_rank_returns_false_when_no_aura() {
        let iface = MockWorld::default();
        assert!(!has_any_rank(&iface, 1, REND_RANKS));
    }
}
