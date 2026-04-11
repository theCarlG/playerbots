//! Equip-filter predicates — `CanEquipItem`, `CanEquipArmor`,
//! `CanEquipWeapon`, `ShouldEquipArmorForSpec`, `ShouldEquipWeaponForSpec`.
//!
//! Port of the same-named helpers in `RandomItemMgr.cpp` lines 308–853.
//! These run during cache build (`BuildItemInfoCache`) and during the
//! runtime `Query` path. Pure functions over `BotItemPrototype` +
//! class/spec context — no global state, no I/O.

use cmangos::BotItemPrototype;

use super::slots;
use super::types::BotEquipKey;

// ── Class constants ────────────────────────────────────────────────────────

#[allow(dead_code)]
pub mod class {
    pub const WARRIOR: u8 = 1;
    pub const PALADIN: u8 = 2;
    pub const HUNTER: u8 = 3;
    pub const ROGUE: u8 = 4;
    pub const PRIEST: u8 = 5;
    pub const DEATH_KNIGHT: u8 = 6;
    pub const SHAMAN: u8 = 7;
    pub const MAGE: u8 = 8;
    pub const WARLOCK: u8 = 9;
    pub const DRUID: u8 = 11;
}

// ── Item class / subclass constants ────────────────────────────────────────

#[allow(dead_code)]
pub mod item_class {
    pub const CONSUMABLE: u32 = 0;
    pub const CONTAINER: u32 = 1;
    pub const WEAPON: u32 = 2;
    pub const GEM: u32 = 3;
    pub const ARMOR: u32 = 4;
    pub const REAGENT: u32 = 5;
    pub const PROJECTILE: u32 = 6;
    pub const TRADE_GOODS: u32 = 7;
    pub const RECIPE: u32 = 9;
    pub const QUIVER: u32 = 11;
    pub const QUEST: u32 = 12;
    pub const KEY: u32 = 13;
    pub const MISC: u32 = 15;
    pub const GLYPH: u32 = 16;
}

#[allow(dead_code)]
pub mod weapon_sub {
    pub const AXE: u32 = 0;
    pub const AXE2: u32 = 1;
    pub const BOW: u32 = 2;
    pub const GUN: u32 = 3;
    pub const MACE: u32 = 4;
    pub const MACE2: u32 = 5;
    pub const POLEARM: u32 = 6;
    pub const SWORD: u32 = 7;
    pub const SWORD2: u32 = 8;
    pub const STAFF: u32 = 10;
    pub const FIST: u32 = 13;
    pub const DAGGER: u32 = 15;
    pub const THROWN: u32 = 16;
    pub const CROSSBOW: u32 = 18;
    pub const WAND: u32 = 19;
}

#[allow(dead_code)]
pub mod gem_sub {
    /// `ITEM_SUBCLASS_GEM_SIMPLE` — the "simple" meta-less gem that
    /// `GetGemsList` intentionally filters out because it's only used for
    /// the prismatic socket weight calculation and never rolled into the
    /// bot loadout.
    pub const SIMPLE: u32 = 7;
}

#[allow(dead_code)]
pub mod armor_sub {
    pub const MISC: u32 = 0;
    pub const CLOTH: u32 = 1;
    pub const LEATHER: u32 = 2;
    pub const MAIL: u32 = 3;
    pub const PLATE: u32 = 4;
    pub const SHIELD: u32 = 6;
    pub const LIBRAM: u32 = 7;
    pub const IDOL: u32 = 8;
    pub const TOTEM: u32 = 9;
    pub const SIGIL: u32 = 10;
}

// ── Bonding constants ──────────────────────────────────────────────────────

#[allow(dead_code)]
pub mod bonding {
    pub const NO_BIND: u32 = 0;
    pub const BIND_WHEN_PICKED_UP: u32 = 1;
    pub const BIND_WHEN_EQUIPPED: u32 = 2;
    pub const BIND_WHEN_USE: u32 = 3;
    pub const BIND_QUEST_ITEM: u32 = 4;
}

// ── Spec id convention ────────────────────────────────────────────────────

/// A spec slot id. The legacy code uses `spec` as both a talent-tab
/// index (0..2) *and* as an id into the `m_weightScales` map (1..=32).
/// Functions in this file that take a spec name reflect the latter use;
/// the manager resolves the scale-id once per call.
pub type SpecName<'a> = &'a str;

// ── CanEquipItem ───────────────────────────────────────────────────────────

/// Pre-check used by `Query` / `BuildEquipCache` — the item's class,
/// bonding, duration, quality, slot viability and recorded
/// min-level must all line up.
///
/// `min_level` is the cached minimum level (may be 0 if
/// `proto.required_level` was not set) — callers supply it from the
/// item-info cache.
#[must_use]
pub fn can_equip_item(key: BotEquipKey, proto: &BotItemPrototype, min_level: u32) -> bool {
    if (proto.duration & 0x8000_0000) != 0 {
        return false;
    }
    if proto.quality != key.quality {
        return false;
    }
    if proto.bonding == bonding::BIND_QUEST_ITEM || proto.bonding == bonding::BIND_WHEN_USE {
        return false;
    }
    if proto.class_id == item_class::CONTAINER {
        return true;
    }
    if !slots::slot_accepts(key.slot, proto.inventory_type as u8) {
        return false;
    }

    let required = if proto.required_level == 0 {
        min_level
    } else {
        proto.required_level
    };
    required != 0
}

/// Looser variant used by `BuildItemInfoCache` when it scans items
/// without committing to a specific (level, class, spec, slot, quality)
/// combination. Only rejects containers and obviously-unequippable
/// items.
#[must_use]
pub fn can_equip_item_new(proto: &BotItemPrototype) -> bool {
    if (proto.duration & 0x8000_0000) != 0 {
        return false;
    }
    if proto.bonding == bonding::BIND_QUEST_ITEM || proto.bonding == bonding::BIND_WHEN_USE {
        return false;
    }
    if proto.class_id == item_class::CONTAINER {
        return false;
    }
    slots::any_slot_accepts(proto.inventory_type as u8)
}

// ── CanEquipArmor ──────────────────────────────────────────────────────────

const INVTYPE_TABARD: u32 = 19;
const INVTYPE_CLOAK: u32 = 16;

/// Port of `RandomItemMgr::CanEquipArmor`. `spec` here is the numeric
/// scale id (1..=32), not a spec name.
#[must_use]
pub fn can_equip_armor(clazz: u8, spec: u32, level: u32, proto: &BotItemPrototype) -> bool {
    if proto.inventory_type == INVTYPE_TABARD {
        return true;
    }

    // Shields are always equippable for the "can block" classes.
    if (clazz == class::WARRIOR || clazz == class::PALADIN || clazz == class::SHAMAN)
        && proto.sub_class == armor_sub::SHIELD
    {
        return true;
    }

    // Warriors / prot-pallys at 40+ need plate (or a cloak).
    if (clazz == class::WARRIOR || (clazz == class::PALADIN && spec != 4))
        && level >= 40
        && proto.sub_class != armor_sub::PLATE
        && proto.inventory_type != INVTYPE_CLOAK
    {
        return false;
    }

    // Mail tier: warriors/prot-pallys <40, hunters/enh-shamans 40+.
    if (((clazz == class::WARRIOR || (clazz == class::PALADIN && spec != 4)) && level < 40)
        || ((clazz == class::HUNTER || (clazz == class::SHAMAN && spec != 21)) && level >= 40))
        && proto.sub_class != armor_sub::MAIL
        && proto.inventory_type != INVTYPE_CLOAK
    {
        // Spec-22 (enhancement shaman variant?) may equip leather/cloth as fallback.
        if spec == 22
            && (proto.sub_class == armor_sub::LEATHER || proto.sub_class == armor_sub::CLOTH)
        {
            return true;
        }
        return false;
    }

    // Leather tier: hunters/shamans <40, druids (non-balance/resto),
    // rogues.
    if (((clazz == class::HUNTER || clazz == class::SHAMAN) && level < 40)
        || ((clazz == class::DRUID && !(spec == 29 || spec == 31)) || clazz == class::ROGUE))
        && proto.sub_class != armor_sub::LEATHER
        && proto.inventory_type != INVTYPE_CLOAK
    {
        return false;
    }

    true
}

// ── CanEquipWeapon ─────────────────────────────────────────────────────────

/// Port of `RandomItemMgr::CanEquipWeapon` — rejects weapon subclasses
/// the class is not trained in. Does not check spec.
#[must_use]
pub fn can_equip_weapon(clazz: u8, proto: &BotItemPrototype) -> bool {
    use weapon_sub::*;
    let sub = proto.sub_class;
    match clazz {
        class::PRIEST => sub == STAFF || sub == WAND || sub == MACE,
        class::MAGE | class::WARLOCK => sub == STAFF || sub == WAND || sub == SWORD,
        class::WARRIOR => matches!(
            sub,
            MACE2 | SWORD2 | MACE | SWORD | GUN | CROSSBOW | BOW | AXE | AXE2 | THROWN
        ),
        class::PALADIN => matches!(sub, MACE2 | SWORD2 | MACE | SWORD),
        class::SHAMAN => matches!(sub, MACE | MACE2 | STAFF),
        class::DRUID => matches!(sub, MACE | MACE2 | DAGGER | STAFF),
        class::HUNTER => matches!(sub, AXE2 | SWORD2 | GUN | CROSSBOW | BOW),
        class::ROGUE => matches!(
            sub,
            DAGGER | SWORD | MACE | GUN | CROSSBOW | BOW | THROWN
        ),
        #[cfg(feature = "wotlk")]
        class::DEATH_KNIGHT => matches!(sub, MACE2 | SWORD2 | MACE | SWORD | AXE | AXE2),
        _ => true,
    }
}

// ── ShouldEquipArmorForSpec ────────────────────────────────────────────────

const INVTYPE_HOLDABLE: u32 = 23;

/// Port of `RandomItemMgr::ShouldEquipArmorForSpec`. `spec_name` is the
/// weight-scale name (e.g. "arms", "holy", "feraldps"). `has_scale`
/// must be `true` when the manager has a weight-scale entry for that
/// spec (mirrors `m_weightScales[spec].info.id != 0`).
#[must_use]
pub fn should_equip_armor_for_spec(
    player_class: u8,
    spec_name: SpecName<'_>,
    has_scale: bool,
    proto: &BotItemPrototype,
) -> bool {
    if proto.inventory_type == INVTYPE_TABARD {
        return true;
    }
    if !has_scale {
        return false;
    }

    let sub = proto.sub_class;
    let inv = proto.inventory_type;

    // Helper: check sub in a small set.
    let in_set = |set: &[u32]| set.contains(&sub);

    let allowed: &[u32] = match player_class {
        class::WARRIOR => {
            if inv == INVTYPE_HOLDABLE {
                return false;
            }
            if spec_name == "arms" || spec_name == "fury" {
                &[armor_sub::LEATHER, armor_sub::MAIL, armor_sub::PLATE]
            } else {
                &[armor_sub::MAIL, armor_sub::PLATE]
            }
        }
        #[cfg(feature = "wotlk")]
        class::DEATH_KNIGHT => {
            if inv == INVTYPE_HOLDABLE {
                return false;
            }
            &[armor_sub::SIGIL, armor_sub::PLATE]
        }
        class::PALADIN => {
            if spec_name != "holy" && inv == INVTYPE_HOLDABLE {
                return false;
            }
            if spec_name == "holy" {
                &[
                    armor_sub::CLOTH,
                    armor_sub::LEATHER,
                    armor_sub::MAIL,
                    armor_sub::PLATE,
                    armor_sub::LIBRAM,
                ]
            } else {
                &[armor_sub::MAIL, armor_sub::PLATE, armor_sub::LIBRAM]
            }
        }
        class::HUNTER => {
            if inv == INVTYPE_HOLDABLE {
                return false;
            }
            &[armor_sub::CLOTH, armor_sub::LEATHER, armor_sub::MAIL]
        }
        class::ROGUE => {
            if inv == INVTYPE_HOLDABLE {
                return false;
            }
            &[armor_sub::CLOTH, armor_sub::LEATHER]
        }
        class::PRIEST => &[armor_sub::CLOTH],
        class::SHAMAN => {
            if spec_name == "enhance" && inv == INVTYPE_HOLDABLE {
                return false;
            }
            #[cfg(feature = "vanilla")]
            {
                if spec_name == "enhance" {
                    &[armor_sub::TOTEM, armor_sub::LEATHER, armor_sub::MAIL]
                } else {
                    &[
                        armor_sub::TOTEM,
                        armor_sub::CLOTH,
                        armor_sub::LEATHER,
                        armor_sub::MAIL,
                    ]
                }
            }
            #[cfg(not(feature = "vanilla"))]
            {
                &[armor_sub::TOTEM, armor_sub::LEATHER, armor_sub::MAIL]
            }
        }
        class::MAGE | class::WARLOCK => &[armor_sub::CLOTH],
        class::DRUID => {
            if (spec_name == "feraltank" || spec_name == "feraldps") && inv == INVTYPE_HOLDABLE {
                return false;
            }
            if spec_name == "feraltank" || spec_name == "feraldps" {
                &[armor_sub::IDOL, armor_sub::LEATHER]
            } else {
                &[armor_sub::IDOL, armor_sub::CLOTH, armor_sub::LEATHER]
            }
        }
        _ => &[armor_sub::CLOTH],
    };

    in_set(allowed)
}

// ── ShouldEquipWeaponForSpec ──────────────────────────────────────────────

/// Port of `RandomItemMgr::ShouldEquipWeaponForSpec`.
///
/// `has_scale` gates the whole call the same way the legacy code's
/// `m_weightScales[spec].info.id` check does.
#[must_use]
pub fn should_equip_weapon_for_spec(
    player_class: u8,
    spec_name: SpecName<'_>,
    has_scale: bool,
    proto: &BotItemPrototype,
) -> bool {
    use slots::slot as S;
    use weapon_sub::*;

    let inv = proto.inventory_type as u8;

    // Which equip slot(s) could this item go into?
    let fits_mh = slots::slot_accepts(S::MAINHAND, inv);
    let fits_oh = slots::slot_accepts(S::OFFHAND, inv);
    let fits_rh = slots::slot_accepts(S::RANGED, inv);

    if !fits_mh && !fits_oh && !fits_rh {
        return false;
    }

    if !has_scale {
        return false;
    }

    // Per-class (spec-conditional) subclass sets. Returns (mh, oh, rh)
    // as slices over `armor_sub`/`weapon_sub` constants.
    let (mh, oh, rh): (&[u32], &[u32], &[u32]) = match player_class {
        class::WARRIOR => {
            if spec_name == "prot" {
                (
                    &[SWORD, AXE, MACE, DAGGER, FIST],
                    &[armor_sub::SHIELD],
                    &[BOW, CROSSBOW, GUN],
                )
            } else if spec_name == "arms" {
                (
                    &[SWORD2, AXE2, MACE2, POLEARM],
                    &[],
                    &[BOW, CROSSBOW, GUN],
                )
            } else {
                (
                    &[SWORD, AXE, MACE, DAGGER, FIST],
                    &[SWORD, AXE, MACE, DAGGER, FIST],
                    &[BOW, CROSSBOW, GUN],
                )
            }
        }
        class::PALADIN => {
            if spec_name == "prot" {
                (
                    &[SWORD, AXE, MACE],
                    &[armor_sub::SHIELD],
                    &[armor_sub::LIBRAM],
                )
            } else if spec_name == "holy" {
                (
                    &[SWORD, AXE, MACE],
                    &[armor_sub::SHIELD, armor_sub::MISC],
                    &[armor_sub::LIBRAM],
                )
            } else {
                (
                    &[SWORD2, AXE2, MACE2, POLEARM],
                    &[],
                    &[armor_sub::LIBRAM],
                )
            }
        }
        class::HUNTER => (
            &[FIST, DAGGER, SWORD, AXE, SWORD2, AXE2, POLEARM, STAFF],
            &[],
            &[BOW, CROSSBOW, GUN],
        ),
        class::ROGUE => {
            let (mh, oh) = if spec_name == "assas" {
                (&[DAGGER][..], &[DAGGER][..])
            } else if spec_name == "combat" {
                (&[SWORD, MACE][..], &[SWORD, MACE][..])
            } else {
                (
                    &[DAGGER, SWORD, MACE, FIST][..],
                    &[DAGGER, SWORD, MACE, FIST][..],
                )
            };
            (mh, oh, &[THROWN, BOW, CROSSBOW, GUN])
        }
        class::PRIEST => (
            &[STAFF, DAGGER, MACE],
            &[armor_sub::MISC],
            &[WAND],
        ),
        class::SHAMAN => {
            if spec_name == "resto" {
                (
                    &[STAFF, DAGGER, AXE, MACE, FIST],
                    &[armor_sub::MISC, armor_sub::SHIELD],
                    &[armor_sub::TOTEM],
                )
            } else if spec_name == "enhance" {
                #[cfg(feature = "vanilla")]
                {
                    (&[MACE2, AXE2], &[], &[armor_sub::TOTEM])
                }
                #[cfg(not(feature = "vanilla"))]
                {
                    (
                        &[MACE2, AXE2, AXE, MACE, FIST],
                        &[],
                        &[armor_sub::TOTEM],
                    )
                }
            } else {
                (
                    &[STAFF, DAGGER, AXE, MACE, FIST],
                    &[armor_sub::MISC, armor_sub::SHIELD],
                    &[armor_sub::TOTEM],
                )
            }
        }
        class::MAGE | class::WARLOCK => (
            &[STAFF, DAGGER, SWORD],
            &[armor_sub::MISC],
            &[WAND],
        ),
        class::DRUID => {
            if spec_name == "feraltank" {
                (
                    &[STAFF, MACE2, DAGGER, MACE],
                    &[armor_sub::MISC],
                    &[armor_sub::IDOL],
                )
            } else if spec_name == "resto" {
                (
                    &[STAFF, DAGGER, MACE, MACE2],
                    &[armor_sub::MISC],
                    &[armor_sub::IDOL],
                )
            } else if spec_name == "feraldps" {
                (
                    &[STAFF, MACE2, MACE],
                    &[armor_sub::MISC],
                    &[armor_sub::IDOL],
                )
            } else {
                (
                    &[DAGGER, MACE, STAFF, MACE2],
                    &[armor_sub::MISC],
                    &[armor_sub::IDOL],
                )
            }
        }
        #[cfg(feature = "wotlk")]
        class::DEATH_KNIGHT => (
            &[SWORD, SWORD2, AXE, AXE2, MACE, MACE2],
            &[],
            &[armor_sub::SIGIL],
        ),
        _ => (&[], &[], &[]),
    };

    // Pick the first slot that accepted this inventory type. The
    // legacy loop assigns `slot_mh`/`slot_oh`/`slot_rh` independently;
    // when an item fits multiple slots (e.g. a 1-hand sword fits both
    // mh and oh) the legacy code checks `slot_mh` first, then `oh`,
    // then `rh`, so we preserve that ordering here.
    let sub = proto.sub_class;
    if fits_mh {
        return mh.contains(&sub);
    }
    if fits_oh {
        return oh.contains(&sub);
    }
    if fits_rh {
        return rh.contains(&sub);
    }
    false
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn proto(class: u32, sub: u32, inv: u32) -> BotItemPrototype {
        BotItemPrototype {
            class_id: class,
            sub_class: sub,
            inventory_type: inv,
            quality: 1,
            bonding: bonding::NO_BIND,
            ..Default::default()
        }
    }

    #[test]
    fn warrior_arms_rejects_cloth() {
        let p = proto(item_class::ARMOR, armor_sub::CLOTH, 5);
        assert!(!should_equip_armor_for_spec(
            class::WARRIOR, "arms", true, &p
        ));
    }

    #[test]
    fn warrior_arms_accepts_leather_mail_plate() {
        for sub in [armor_sub::LEATHER, armor_sub::MAIL, armor_sub::PLATE] {
            let p = proto(item_class::ARMOR, sub, 5);
            assert!(should_equip_armor_for_spec(
                class::WARRIOR, "arms", true, &p
            ));
        }
    }

    #[test]
    fn holy_paladin_accepts_all_armor_and_libram() {
        for sub in [
            armor_sub::CLOTH,
            armor_sub::LEATHER,
            armor_sub::MAIL,
            armor_sub::PLATE,
            armor_sub::LIBRAM,
        ] {
            let p = proto(item_class::ARMOR, sub, 5);
            assert!(should_equip_armor_for_spec(
                class::PALADIN, "holy", true, &p
            ));
        }
    }

    #[test]
    fn priest_only_equips_cloth_armor() {
        let cloth = proto(item_class::ARMOR, armor_sub::CLOTH, 5);
        let leather = proto(item_class::ARMOR, armor_sub::LEATHER, 5);
        assert!(should_equip_armor_for_spec(
            class::PRIEST, "holy", true, &cloth
        ));
        assert!(!should_equip_armor_for_spec(
            class::PRIEST, "holy", true, &leather
        ));
    }

    #[test]
    fn tabard_always_equippable() {
        let p = proto(item_class::ARMOR, armor_sub::CLOTH, INVTYPE_TABARD);
        assert!(should_equip_armor_for_spec(
            class::WARRIOR, "prot", false, &p
        ));
    }

    #[test]
    fn missing_scale_rejects_non_tabard() {
        let p = proto(item_class::ARMOR, armor_sub::PLATE, 5);
        assert!(!should_equip_armor_for_spec(
            class::WARRIOR, "prot", false, &p
        ));
    }

    #[test]
    fn rogue_assas_only_daggers() {
        let dagger = proto(item_class::WEAPON, weapon_sub::DAGGER, 13);
        let sword = proto(item_class::WEAPON, weapon_sub::SWORD, 13);
        assert!(should_equip_weapon_for_spec(
            class::ROGUE, "assas", true, &dagger
        ));
        assert!(!should_equip_weapon_for_spec(
            class::ROGUE, "assas", true, &sword
        ));
    }

    #[test]
    fn priest_weapon_subclass_whitelist() {
        let staff = proto(item_class::WEAPON, weapon_sub::STAFF, 17);
        let wand = proto(item_class::WEAPON, weapon_sub::WAND, 15);
        let mace = proto(item_class::WEAPON, weapon_sub::MACE, 13);
        let sword = proto(item_class::WEAPON, weapon_sub::SWORD, 13);
        assert!(can_equip_weapon(class::PRIEST, &staff));
        assert!(can_equip_weapon(class::PRIEST, &wand));
        assert!(can_equip_weapon(class::PRIEST, &mace));
        assert!(!can_equip_weapon(class::PRIEST, &sword));
    }

    #[test]
    fn can_equip_armor_tabard_bypasses_class_filter() {
        let p = proto(item_class::ARMOR, armor_sub::CLOTH, INVTYPE_TABARD);
        assert!(can_equip_armor(class::WARRIOR, 1, 60, &p));
    }

    #[test]
    fn can_equip_armor_shield_for_warriors() {
        let p = proto(item_class::ARMOR, armor_sub::SHIELD, 14);
        assert!(can_equip_armor(class::WARRIOR, 1, 40, &p));
        assert!(can_equip_armor(class::PALADIN, 2, 40, &p));
        assert!(can_equip_armor(class::SHAMAN, 21, 40, &p));
        assert!(!can_equip_armor(class::ROGUE, 4, 40, &p));
    }

    #[test]
    fn can_equip_item_rejects_flagged_duration() {
        let mut p = proto(item_class::ARMOR, armor_sub::CLOTH, 5);
        p.duration = 0x8000_0001;
        p.required_level = 1;
        let key = BotEquipKey::new(60, 1, 1, slots::slot::CHEST, 1);
        assert!(!can_equip_item(key, &p, 0));
    }

    #[test]
    fn can_equip_item_requires_matching_quality() {
        let mut p = proto(item_class::ARMOR, armor_sub::CLOTH, 5);
        p.required_level = 1;
        let key = BotEquipKey::new(60, 1, 1, slots::slot::CHEST, 2);
        assert!(!can_equip_item(key, &p, 0));
    }
}
