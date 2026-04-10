/// Spell ID constants introduced in The Burning Crusade (patch 2.x).
///
/// Vanilla ranks live under `spells::vanilla::<class>` and remain valid IDs
/// in the TBC DBC, so class trees don't need to swap imports — they just
/// gain the new-spell constants from this module when the `tbc` feature is
/// enabled. Only genuinely new abilities (or new top ranks that matter for
/// rotation decisions) are listed here.
use cmangos::SpellId;

// ── Shaman ──────────────────────────────────────────────────────────────
// Earth Shield — new in TBC, single-target reactive HoT for the enhancement
// and restoration trees. Rotation code checks target for this aura and keeps
// it applied; we only need the base rank.
pub const EARTH_SHIELD: SpellId = SpellId(32594);

// Chain Heal — new high-rank in TBC (rank 5).
pub const CHAIN_HEAL_TBC: SpellId = SpellId(33642);

// ── Paladin ─────────────────────────────────────────────────────────────
// Avenging Wrath — new cooldown in TBC.
pub const AVENGING_WRATH: SpellId = SpellId(31884);
// Crusader Strike — new retri baseline in TBC.
pub const CRUSADER_STRIKE_TBC: SpellId = SpellId(35395);

// ── Warlock ─────────────────────────────────────────────────────────────
// Felguard — new demonology pet ability.
pub const SUMMON_FELGUARD: SpellId = SpellId(30146);

// ── Mage ────────────────────────────────────────────────────────────────
// Ice Lance — new frost spell in TBC.
pub const ICE_LANCE: SpellId = SpellId(30455);

// ── Priest ──────────────────────────────────────────────────────────────
// Mass Dispel — new group utility in TBC.
pub const MASS_DISPEL: SpellId = SpellId(32375);
