/// Aura rank helpers — check if a unit has any rank of a multi-rank spell.
///
/// Many `WoW` spells have multiple ranks (e.g. Rend rank 1-6).  When checking
/// if a DoT/buff is already on a target, we need to check all ranks.
/// This module provides `has_any_rank()` and spell rank tables.
use crate::ffi::{SpellId, UnitHandle, interface::BotInterface};

/// Check if `unit` has any of the given spell IDs as an aura.
#[inline]
pub fn has_any_rank(iface: &dyn BotInterface, unit: UnitHandle, ranks: &[SpellId]) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::context::tests::NullInterface;

    #[test]
    fn has_any_rank_returns_false_when_no_aura() {
        let iface = NullInterface;
        assert!(!has_any_rank(&iface, 1, REND_RANKS));
    }
}
