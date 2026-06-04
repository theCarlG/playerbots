//! Pet initialization — port of `PlayerbotFactory::InitPet` and
//! `PlayerbotFactory::InitPetSpells` (playerbot/PlayerbotFactory.cpp:314-1280).
//!
//! There are two entry points:
//!
//!   * [`init_pet`] — mirrors `PlayerbotFactory::InitPet`. Hunter-only.
//!     If the bot has no pet it walks the tameable creature list, picks
//!     one at random (up to 100 retries), and calls the atomic
//!     `factory_create_hunter_pet` callback to run the full `CMaNGOS`
//!     creation sequence. Finishes by refreshing pet stats, mass-
//!     toggling autocast on every non-passive pet spell, and force-
//!     dismissing the pet to clear a missing-flags bug in the legacy
//!     C++ code.
//!
//!   * [`init_pet_spells`] — mirrors `PlayerbotFactory::InitPetSpells`.
//!     Vanilla hunter path dispatches on `CreatureInfo::Family` to a
//!     per-pet-type spell table, then learns the rank-appropriate
//!     Growl / Natural Armor / Great Stamina, and (≥ level 20) every
//!     resistance spell. Vanilla+TBC warlock path dispatches on the
//!     pet's creature template entry (`PET_IMP` et al.) and learns
//!     the level-gated spell list from the hard-coded warlock table.
//!
//! Everything is pure data + policy — no `CMaNGOS` glue. The FFI layer
//! hands us `factory_tameable_creatures_for_bot_level` /
//! `factory_create_hunter_pet` /
//! `factory_pet_autocast_candidate_spells` as one-shot callbacks and
//! lets Rust own the retry/dispatch policy.

use super::FactoryTransaction;
use crate::bot::state::PlayerClass;

/* ── Hunter pet types ─────────────────────────────────────────────────── */

/// Internal enum for the hunter pet family dispatch. Mirrors the
/// `HunterPetType` enum inlined inside `PlayerbotFactory::InitPetSpells`.
#[cfg(feature = "vanilla")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum HunterPetType {
    Wolf,
    Cat,
    Spider,
    Bear,
    Boar,
    Crocolisk,
    CarrionBird,
    Crab,
    Gorilla,
    Raptor,
    Tallstrider,
    Scorpid,
    Turtle,
    Bat,
    Hyena,
    Owl,
    WindSerpent,
}

/// Decode the vanilla `CreatureInfo::Family` value into a
/// [`HunterPetType`]. Mirrors the `GetHunterPetTypeFromEntry` lambda in
/// `PlayerbotFactory.cpp:927-953`.
#[cfg(feature = "vanilla")]
const fn hunter_pet_type_from_family(family: u32) -> Option<HunterPetType> {
    match family {
        1 => Some(HunterPetType::Wolf),
        2 => Some(HunterPetType::Cat),
        3 => Some(HunterPetType::Spider),
        4 => Some(HunterPetType::Bear),
        5 => Some(HunterPetType::Boar),
        6 => Some(HunterPetType::Crocolisk),
        7 => Some(HunterPetType::CarrionBird),
        8 => Some(HunterPetType::Crab),
        9 => Some(HunterPetType::Gorilla),
        11 => Some(HunterPetType::Raptor),
        12 => Some(HunterPetType::Tallstrider),
        20 => Some(HunterPetType::Scorpid),
        21 => Some(HunterPetType::Turtle),
        24 => Some(HunterPetType::Bat),
        25 => Some(HunterPetType::Hyena),
        26 => Some(HunterPetType::Owl),
        27 => Some(HunterPetType::WindSerpent),
        _ => None,
    }
}

/* ── Hunter pet spell tables ──────────────────────────────────────────── */

/// Shared `Bite` rank table for every hunter pet with a bite. Mirrors
/// the inline `{1, 17253}, ..., {56, 17261}` sequence repeated in
/// `PlayerbotFactory.cpp` for every pet type that learns Bite.
#[cfg(feature = "vanilla")]
const BITE_RANKS: &[(u32, u32)] = &[
    (1, 17253),
    (8, 17255),
    (16, 17256),
    (24, 17257),
    (32, 17258),
    (40, 17259),
    (48, 17260),
    (56, 17261),
];

/// Shared `Claw` rank table for every hunter pet that gets Claw.
#[cfg(feature = "vanilla")]
const CLAW_RANKS: &[(u32, u32)] = &[
    (1, 16827),
    (8, 16828),
    (15, 16829),
    (22, 16830),
    (29, 16831),
    (36, 16832),
    (48, 3010),
    (56, 3009),
];

/// Shared `Cower` rank table. Every vanilla hunter pet learns Cower.
/// Autocast is toggled *off* per-spell-id via [`COWER_SPELL_IDS`]
/// below.
#[cfg(feature = "vanilla")]
const COWER_RANKS: &[(u32, u32)] = &[
    (5, 1742),
    (15, 1753),
    (25, 1754),
    (35, 1755),
    (45, 1756),
    (55, 16697),
];

/// Shared `Dash` rank table (Wolf/Boar/Cat/Hyena/Tallstrider).
#[cfg(feature = "vanilla")]
const DASH_RANKS: &[(u32, u32)] = &[(30, 23099), (40, 23109), (50, 23110)];

/// Shared `Dive` rank table (Bat/Carrion Bird/Owl/Wind Serpent).
#[cfg(feature = "vanilla")]
const DIVE_RANKS: &[(u32, u32)] = &[(30, 23145), (40, 23146), (50, 23147)];

/// Shared `Screech` rank table for Bat/Carrion Bird — Carrion Bird's
/// rank 4 is `27051` (different from Owl's `24579`).
#[cfg(feature = "vanilla")]
const SCREECH_RANKS_CARRION: &[(u32, u32)] =
    &[(8, 24423), (24, 24577), (40, 24578), (56, 27051)];

/// `Screech` rank table for Owl — rank 4 is `24579`.
#[cfg(feature = "vanilla")]
const SCREECH_RANKS_OWL: &[(u32, u32)] = &[(8, 24423), (24, 24577), (40, 24578), (56, 24579)];

/// Cower spell IDs — autocast is set *off* by default after the mass
/// `ToggleAutocast(true)` pass. Matches the `cowerSpellIds`
/// `unordered_set` at `PlayerbotFactory.cpp:961`.
#[cfg(feature = "vanilla")]
const COWER_SPELL_IDS: &[u32] = &[1742, 1753, 1754, 1755, 1756, 16697];

/// Return the full `(level, spell_id)` table for a given hunter pet
/// type. Order matches `PlayerbotFactory::InitPetSpells` — stable so
/// the autocast dispatch order is deterministic.
#[cfg(feature = "vanilla")]
fn hunter_pet_spell_table(pet: HunterPetType) -> Vec<(u32, u32)> {
    // Seed with the rank tables that apply to this pet type, then
    // append the per-pet extras. Using a Vec keeps the signature tidy
    // and avoids a monster const-slice tree.
    let mut out = Vec::new();
    let push_many = |out: &mut Vec<(u32, u32)>, table: &[(u32, u32)]| out.extend_from_slice(table);

    match pet {
        HunterPetType::Bat => {
            push_many(&mut out, BITE_RANKS);
            push_many(&mut out, COWER_RANKS);
            push_many(&mut out, DIVE_RANKS);
            push_many(&mut out, SCREECH_RANKS_CARRION);
        }
        HunterPetType::Bear => {
            push_many(&mut out, BITE_RANKS);
            push_many(&mut out, CLAW_RANKS);
            push_many(&mut out, COWER_RANKS);
        }
        HunterPetType::Boar => {
            push_many(&mut out, BITE_RANKS);
            // Charge
            push_many(
                &mut out,
                &[
                    (1, 7371),
                    (12, 26177),
                    (24, 26178),
                    (36, 26179),
                    (48, 26201),
                    (60, 27685),
                ],
            );
            push_many(&mut out, COWER_RANKS);
            push_many(&mut out, DASH_RANKS);
        }
        HunterPetType::CarrionBird => {
            push_many(&mut out, BITE_RANKS);
            push_many(&mut out, CLAW_RANKS);
            push_many(&mut out, COWER_RANKS);
            push_many(&mut out, DIVE_RANKS);
            push_many(&mut out, SCREECH_RANKS_CARRION);
        }
        HunterPetType::Cat => {
            push_many(&mut out, BITE_RANKS);
            push_many(&mut out, CLAW_RANKS);
            push_many(&mut out, COWER_RANKS);
            push_many(&mut out, DASH_RANKS);
            // Prowl
            push_many(&mut out, &[(30, 24450), (40, 24452), (50, 24453)]);
        }
        HunterPetType::Crab => {
            push_many(&mut out, CLAW_RANKS);
            push_many(&mut out, COWER_RANKS);
        }
        HunterPetType::Crocolisk => {
            push_many(&mut out, BITE_RANKS);
            push_many(&mut out, COWER_RANKS);
        }
        HunterPetType::Gorilla => {
            push_many(&mut out, BITE_RANKS);
            push_many(&mut out, COWER_RANKS);
            // Thunderstomp
            push_many(&mut out, &[(30, 26090), (40, 26187), (50, 26188)]);
        }
        HunterPetType::Hyena => {
            push_many(&mut out, BITE_RANKS);
            push_many(&mut out, COWER_RANKS);
            push_many(&mut out, DASH_RANKS);
        }
        HunterPetType::Owl => {
            push_many(&mut out, CLAW_RANKS);
            push_many(&mut out, COWER_RANKS);
            push_many(&mut out, DIVE_RANKS);
            push_many(&mut out, SCREECH_RANKS_OWL);
        }
        HunterPetType::Raptor => {
            push_many(&mut out, BITE_RANKS);
            push_many(&mut out, CLAW_RANKS);
            push_many(&mut out, COWER_RANKS);
        }
        HunterPetType::Scorpid => {
            push_many(&mut out, CLAW_RANKS);
            push_many(&mut out, COWER_RANKS);
            // Scorpid Poison
            push_many(
                &mut out,
                &[(8, 24640), (24, 24583), (40, 24586), (56, 24587)],
            );
        }
        HunterPetType::Spider => {
            push_many(&mut out, BITE_RANKS);
            push_many(&mut out, COWER_RANKS);
        }
        HunterPetType::Tallstrider => {
            push_many(&mut out, BITE_RANKS);
            push_many(&mut out, COWER_RANKS);
            push_many(&mut out, DASH_RANKS);
        }
        HunterPetType::Turtle => {
            push_many(&mut out, BITE_RANKS);
            push_many(&mut out, COWER_RANKS);
            // Shell Shield
            push_many(&mut out, &[(20, 26064)]);
        }
        HunterPetType::WindSerpent => {
            push_many(&mut out, BITE_RANKS);
            push_many(&mut out, COWER_RANKS);
            push_many(&mut out, DIVE_RANKS);
            // Lightning Breath
            push_many(
                &mut out,
                &[
                    (1, 24844),
                    (12, 25008),
                    (24, 25009),
                    (36, 25010),
                    (48, 25011),
                    (60, 25012),
                ],
            );
        }
        HunterPetType::Wolf => {
            push_many(&mut out, BITE_RANKS);
            push_many(&mut out, COWER_RANKS);
            push_many(&mut out, DASH_RANKS);
            // Furious Howl
            push_many(&mut out, &[(10, 24604), (20, 24605), (30, 24603), (40, 24597)]);
        }
    }
    out
}

/// Growl ranks learned by every hunter pet. Highest rank whose
/// `min_level <= pet_level` wins.
#[cfg(feature = "vanilla")]
const GROWL_RANKS: &[(u32, u32)] = &[
    (1, 2649),
    (10, 14916),
    (20, 14917),
    (30, 14918),
    (40, 14919),
    (50, 14920),
    (60, 14921),
];

/// Natural Armor passive ranks.
#[cfg(feature = "vanilla")]
const NATURAL_ARMOR_RANKS: &[(u32, u32)] = &[(1, 24545), (12, 24549), (18, 24550), (24, 24551)];

/// Great Stamina passive ranks.
#[cfg(feature = "vanilla")]
const GREAT_STAMINA_RANKS: &[(u32, u32)] = &[
    (1, 4187),
    (12, 4188),
    (18, 4189),
    (24, 4190),
    (30, 4191),
    (36, 4192),
    (42, 4193),
    (48, 4194),
    (54, 5041),
    (60, 5042),
];

/// Resistance spells learned at pet level ≥ 20:
/// Arcane / Fire / Frost / Nature / Shadow.
#[cfg(feature = "vanilla")]
const PET_RESISTANCE_SPELLS: &[u32] = &[24493, 23992, 24446, 24492, 24488];

/* ── Warlock pet entries + spell tables ───────────────────────────────── */

#[cfg(any(feature = "vanilla", feature = "tbc"))]
const PET_IMP: u32 = 416;
#[cfg(any(feature = "vanilla", feature = "tbc"))]
const PET_FELHUNTER: u32 = 417;
#[cfg(any(feature = "vanilla", feature = "tbc"))]
const PET_VOIDWALKER: u32 = 1860;
#[cfg(any(feature = "vanilla", feature = "tbc"))]
const PET_SUCCUBUS: u32 = 1863;
#[cfg(any(feature = "vanilla", feature = "tbc"))]
const PET_FELGUARD: u32 = 17252;

/// Return the `(level, spell_id)` list for a given warlock pet entry.
/// Mirrors the `spellList` map population in
/// `PlayerbotFactory.cpp:1100-1260`. Empty for any entry that isn't a
/// known warlock pet.
#[cfg(any(feature = "vanilla", feature = "tbc"))]
fn warlock_pet_spell_table(pet_entry: u32) -> &'static [(u32, u32)] {
    match pet_entry {
        PET_IMP => &[
            // Blood Pact
            (4, 6307),
            (14, 7804),
            (26, 7805),
            (38, 11766),
            (50, 11767),
            (62, 27268),
            (74, 47982),
            // Fire Shield
            (14, 2947),
            (24, 8316),
            (34, 8317),
            (44, 11770),
            (54, 11771),
            (64, 27269),
            (76, 47983),
            // Firebolt
            (1, 3110),
            (8, 7799),
            (18, 7800),
            (28, 7801),
            (38, 7802),
            (48, 11762),
            (58, 11763),
            (68, 27267),
            (78, 47964),
            // Phase Shift
            (12, 4511),
        ],
        PET_FELHUNTER => &[
            // Devour Magic
            (30, 19505),
            (38, 19731),
            (46, 19734),
            (54, 19736),
            (62, 27276),
            (70, 27277),
            (77, 48011),
            // Paranoia
            (42, 19480),
            // Spell Lock
            (36, 19244),
            (52, 19647),
            // Tainted Blood
            (32, 19478),
            (40, 19655),
            (48, 19656),
            (56, 19660),
            (64, 27280),
        ],
        PET_VOIDWALKER => &[
            // Consume Shadows
            (18, 17767),
            (26, 17850),
            (34, 17851),
            (42, 17852),
            (50, 17853),
            (58, 17854),
            (66, 27272),
            (73, 47987),
            (78, 47988),
            // Sacrifice
            (16, 7812),
            (24, 19438),
            (32, 19440),
            (40, 19441),
            (48, 19442),
            (56, 19443),
            (64, 27273),
            (72, 47985),
            (79, 47986),
            // Suffering
            (24, 17735),
            (36, 17750),
            (48, 17751),
            (60, 17752),
            (63, 27271),
            (69, 33701),
            (75, 47989),
            (80, 47990),
            // Torment
            (10, 3716),
            (20, 7809),
            (30, 7810),
            (40, 7811),
            (50, 11774),
            (60, 11775),
            (70, 27270),
            (80, 47984),
        ],
        PET_SUCCUBUS => &[
            // Lash of Pain
            (20, 7814),
            (28, 7815),
            (36, 7816),
            (44, 11778),
            (52, 11779),
            (60, 11780),
            (68, 27274),
            (74, 47991),
            (80, 47992),
            // Lesser Invisibility
            (32, 7870),
            // Seduction
            (26, 6358),
            // Soothing Kiss
            (22, 6360),
            (34, 7813),
            (46, 11784),
            (58, 11785),
            (70, 27275),
        ],
        PET_FELGUARD => &[
            // Anguish
            (50, 33698),
            (60, 33699),
            (69, 33700),
            (78, 47993),
            // Avoidance
            (60, 32233),
            // Cleave
            (50, 30213),
            (60, 30219),
            (68, 30223),
            (76, 47994),
            // Demonic Frenzy
            (56, 32850),
            // Intercept
            (52, 30151),
            (61, 30194),
            (69, 30198),
            (79, 47996),
        ],
        _ => &[],
    }
}

/// Scan a `(min_level, spell_id)` rank table and return the highest
/// spell id whose `min_level <= level`. Returns `0` when no row
/// qualifies. Mirrors the rolling `growlSpellId = rank.spellId` loop.
#[cfg(feature = "vanilla")]
fn highest_rank(ranks: &[(u32, u32)], level: u32) -> u32 {
    let mut chosen = 0u32;
    for &(min_level, spell_id) in ranks {
        if level >= min_level {
            chosen = spell_id;
        }
    }
    chosen
}

/* ── Entry points ─────────────────────────────────────────────────────── */

/// Port of `PlayerbotFactory::InitPet` — the whole method body.
///
/// Hunters walk the tameable-creatures list and try up to 100 random
/// picks to hand the C++ side one that successfully creates a Pet.
/// Every other class is a no-op.
pub fn init_pet(tx: &mut FactoryTransaction<'_>) {
    let snap = tx.get_snapshot();
    if PlayerClass::from_class_id(snap.self_.class_id) != Some(PlayerClass::Hunter) {
        return;
    }

    // If the bot already has a pet, skip creation and jump straight to
    // the refresh / autocast / force-dismiss tail.
    let had_pet_before = tx.factory_bot_has_pet();

    if !had_pet_before {
        let candidates: Vec<u32> = tx
            .factory_tameable_creatures_for_bot_level()
            .iter()
            .copied()
            .collect();

        if candidates.is_empty() {
            // Mirrors `sLog.outError("No pets available for bot ...")`;
            // the C++ path returns here, leaving the bot petless.
            return;
        }

        // Up to 100 attempts matches `PlayerbotFactory.cpp:353`. The
        // C++ source does `urand(0, ids.size() - 1)` so `max` is
        // inclusive; `random_u32` matches that contract.
        let max_index = (candidates.len() - 1) as u32;
        for _ in 0..100 {
            let index = tx.random_u32(0, max_index) as usize;
            let entry = candidates[index];
            if tx.factory_create_hunter_pet(entry) {
                break;
            }
        }
    }

    // Whether the pet already existed or was just created, re-run the
    // refresh block. If creation still failed (no pet) bail out just
    // like the C++ `sLog.outError("Cannot create pet ...")` branch.
    if !tx.factory_bot_has_pet() {
        return;
    }

    tx.factory_pet_refresh_stats();

    // Mass-autocast ON for every non-passive, non-removed pet spell.
    // The C++ side filters via `IsPassiveSpell` + `PETSPELL_REMOVED`;
    // the bridge mirrors those filters so the list we get here is
    // already clean.
    let autocast_spells: Vec<u32> = tx
        .factory_pet_autocast_candidate_spells()
        .iter()
        .copied()
        .collect();
    for spell in autocast_spells {
        tx.factory_pet_toggle_autocast(spell, true);
    }

    // Force dismiss the pet to clear the missing-flags bug that
    // `PlayerbotFactory::InitPet` works around with
    // `pet->SetDeathState(JUST_DIED)`. The bridge is responsible for
    // checking `pet->IsAlive()` at the C++ side, so we call
    // unconditionally.
    tx.factory_pet_force_dismiss();
}

/// Port of `PlayerbotFactory::InitPetSpells` — per-class / per-expansion
/// pet spell learning.
pub fn init_pet_spells(tx: &mut FactoryTransaction<'_>) {
    if !tx.factory_bot_has_pet() {
        return;
    }

    let snap = tx.get_snapshot();
    let class = PlayerClass::from_class_id(snap.self_.class_id);

    #[cfg(feature = "vanilla")]
    if class == Some(PlayerClass::Hunter) {
        init_hunter_pet_spells(tx);
    }

    // Warlock spells — both vanilla and TBC. WotLK learns them
    // automatically so the C++ side guards on `#ifndef MANGOSBOT_TWO`.
    #[cfg(any(feature = "vanilla", feature = "tbc"))]
    if class == Some(PlayerClass::Warlock) {
        init_warlock_pet_spells(tx);
    }

    // Silence the unused-variable warning on expansions where both
    // branches are compiled out.
    let _ = class;
}

#[cfg(feature = "vanilla")]
fn init_hunter_pet_spells(tx: &mut FactoryTransaction<'_>) {
    let pet_level = tx.factory_pet_level();
    if pet_level == 0 {
        return;
    }

    let family = tx.factory_pet_family();
    if let Some(pet_type) = hunter_pet_type_from_family(family) {
        let table = hunter_pet_spell_table(pet_type);
        for (level_required, spell_id) in table {
            if pet_level < level_required {
                continue;
            }
            if !tx.factory_pet_has_spell(spell_id) {
                tx.factory_pet_learn_spell(spell_id);
            }
            // Non-passive filter happens on the C++ side —
            // `IsPassiveSpell(spellID)` gates toggle autocast there.
            // Cower (and its ranks) is learned but autocast off.
            if COWER_SPELL_IDS.contains(&spell_id) {
                tx.factory_pet_toggle_autocast(spell_id, false);
            } else {
                tx.factory_pet_toggle_autocast(spell_id, true);
            }
        }
    }

    // Growl / Natural Armor / Great Stamina — highest-rank-wins, learn
    // only if the pet does not already know it.
    let growl = highest_rank(GROWL_RANKS, pet_level);
    if growl != 0 && !tx.factory_pet_has_spell(growl) {
        tx.factory_pet_learn_spell(growl);
    }

    let natural_armor = highest_rank(NATURAL_ARMOR_RANKS, pet_level);
    if natural_armor != 0 && !tx.factory_pet_has_spell(natural_armor) {
        tx.factory_pet_learn_spell(natural_armor);
    }

    let great_stamina = highest_rank(GREAT_STAMINA_RANKS, pet_level);
    if great_stamina != 0 && !tx.factory_pet_has_spell(great_stamina) {
        tx.factory_pet_learn_spell(great_stamina);
    }

    // Resistance spells (Arcane/Fire/Frost/Nature/Shadow) at level ≥ 20.
    if pet_level >= 20 {
        for &spell_id in PET_RESISTANCE_SPELLS {
            if !tx.factory_pet_has_spell(spell_id) {
                tx.factory_pet_learn_spell(spell_id);
            }
        }
    }
}

#[cfg(any(feature = "vanilla", feature = "tbc"))]
fn init_warlock_pet_spells(tx: &mut FactoryTransaction<'_>) {
    let pet_level = tx.factory_pet_level();
    if pet_level == 0 {
        return;
    }

    let entry = tx.factory_pet_entry();
    let table = warlock_pet_spell_table(entry);
    for &(level_required, spell_id) in table {
        if pet_level >= level_required {
            tx.factory_pet_learn_spell(spell_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::with_tx;
    use cmangos::{BotUnitSnapshot, MockEvent, MockWorld, PetCreature};

    fn hunter_world(level: u8) -> MockWorld {
        MockWorld::builder()
            .self_snap(BotUnitSnapshot {
                class_id: 3, // CLASS_HUNTER
                level,
                power_type: 2, // POWER_FOCUS
                ..Default::default()
            })
            .build()
    }

    /// Hunter bot with a pre-existing pet of the requested family.
    fn hunter_with_pet(level: u8, family: u32) -> MockWorld {
        MockWorld::builder()
            .self_snap(BotUnitSnapshot {
                class_id: 3,
                level,
                power_type: 2,
                ..Default::default()
            })
            .pet(42, family, u32::from(level))
            .build()
    }

    fn warlock_world(level: u8) -> MockWorld {
        MockWorld::builder()
            .self_snap(BotUnitSnapshot {
                class_id: 9, // CLASS_WARLOCK
                level,
                power_type: 0, // POWER_MANA
                ..Default::default()
            })
            .build()
    }

    /// Warlock bot with a pre-existing pet of the requested creature
    /// entry.
    fn warlock_with_pet(level: u8, pet_entry: u32) -> MockWorld {
        MockWorld::builder()
            .self_snap(BotUnitSnapshot {
                class_id: 9,
                level,
                power_type: 0,
                ..Default::default()
            })
            .pet(pet_entry, 0, u32::from(level))
            .build()
    }

    #[test]
    fn non_hunter_init_pet_is_noop() {
        let mut world = MockWorld::builder()
            .self_snap(BotUnitSnapshot {
                class_id: 1, // CLASS_WARRIOR
                level: 40,
                ..Default::default()
            })
            .build();
        with_tx(&mut world, init_pet);
        assert!(
            !world
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::CreateHunterPet(_))),
            "non-hunter classes should never create a pet"
        );
    }

    #[test]
    fn init_pet_without_tameables_is_noop() {
        let mut world = hunter_world(40);
        with_tx(&mut world, init_pet);
        assert!(
            !world
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::CreateHunterPet(_))),
            "empty tameable list should leave bot petless"
        );
    }

    #[test]
    fn init_pet_creates_refreshes_autocasts_and_dismisses() {
        let mut world = MockWorld::builder()
            .self_snap(BotUnitSnapshot {
                class_id: 3,
                level: 40,
                power_type: 2,
                ..Default::default()
            })
            .tameable_creatures(vec![PetCreature { entry: 42, family: 1 }])
            // After creation the autocast candidates list would
            // normally be populated by the C++ side from
            // pet->m_spells — simulate that with a pre-seeded list.
            .pet_autocast_candidates(vec![17253, 2649])
            .build();

        with_tx(&mut world, init_pet);

        let events = world.events();
        // Creation callback fired for the pre-seeded entry.
        assert!(
            events
                .iter()
                .any(|e| matches!(e, MockEvent::CreateHunterPet(42))),
            "init_pet should create a hunter pet from the tameable list"
        );
        // Refresh stats always runs after (successful) creation.
        assert!(
            events.iter().any(|e| matches!(e, MockEvent::PetRefreshStats)),
            "init_pet should refresh pet stats"
        );
        // Autocast toggle fired once per candidate spell.
        let autocast_on_count = events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    MockEvent::PetToggleAutocast {
                        spell: _, enable: true
                    }
                )
            })
            .count();
        assert!(
            autocast_on_count >= 2,
            "expected at least one ToggleAutocast(true) per candidate, got {autocast_on_count}"
        );
        // Force dismiss at the tail.
        assert!(
            events.iter().any(|e| matches!(e, MockEvent::PetForceDismiss)),
            "init_pet should force-dismiss the pet at the tail"
        );
    }

    #[test]
    fn init_pet_skips_creation_when_bot_already_has_pet() {
        let mut world = MockWorld::builder()
            .self_snap(BotUnitSnapshot {
                class_id: 3,
                level: 40,
                power_type: 2,
                ..Default::default()
            })
            // Seed the bot as already having a pet via the `.pet()`
            // builder helper (MockWorldBuilder sets pet_entry + family
            // + level so factory_bot_has_pet returns true).
            .pet(42, 1, 40)
            .build();

        with_tx(&mut world, init_pet);

        let events = world.events();
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, MockEvent::CreateHunterPet(_))),
            "init_pet should skip creation when the bot already has a pet"
        );
        assert!(
            events.iter().any(|e| matches!(e, MockEvent::PetRefreshStats)),
            "init_pet should still refresh stats for the existing pet"
        );
        assert!(
            events.iter().any(|e| matches!(e, MockEvent::PetForceDismiss)),
            "init_pet should still force-dismiss the existing pet"
        );
    }

    #[cfg(feature = "vanilla")]
    #[test]
    fn init_pet_spells_hunter_learns_growl_rank_5_at_level_40() {
        // Wolf pet (family 1) at level 40.
        let mut world = hunter_with_pet(40, 1);

        with_tx(&mut world, init_pet_spells);

        // Growl rank 5 is spell 14919.
        assert!(
            world
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::PetLearnSpell(14919))),
            "pet at level 40 should learn Growl rank 5"
        );
        // Rank 6 is level 50 — should NOT have been learned.
        assert!(
            !world
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::PetLearnSpell(14920))),
            "pet at level 40 must not yet learn Growl rank 6"
        );
    }

    #[cfg(feature = "vanilla")]
    #[test]
    fn init_pet_spells_hunter_cower_autocasts_off() {
        // Boar pet (family 5) at level 60 → learns every Cower rank.
        let mut world = hunter_with_pet(60, 5);

        with_tx(&mut world, init_pet_spells);

        let events = world.events();
        // Every Cower rank should have been toggled autocast OFF
        // (enable = false) at least once.
        let cower_off = events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    MockEvent::PetToggleAutocast { spell, enable: false }
                        if COWER_SPELL_IDS.contains(spell)
                )
            })
            .count();
        assert!(
            cower_off > 0,
            "expected Cower autocast to be toggled OFF, got 0 events"
        );
        // And the highest-rank Cower spell (16697) should also be in
        // the pet's spellbook.
        assert!(
            events
                .iter()
                .any(|e| matches!(e, MockEvent::PetLearnSpell(16697))),
            "pet at level 60 should learn Cower rank 6"
        );
    }

    #[cfg(feature = "vanilla")]
    #[test]
    fn init_pet_spells_hunter_learns_resistances_at_level_20() {
        let mut world = hunter_with_pet(20, 1);

        with_tx(&mut world, init_pet_spells);

        let events = world.events();
        // All five resistance spells must be learned once.
        for &spell in PET_RESISTANCE_SPELLS {
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, MockEvent::PetLearnSpell(s) if *s == spell)),
                "pet at level 20 should learn resistance spell {spell}"
            );
        }
    }

    #[cfg(feature = "vanilla")]
    #[test]
    fn init_pet_spells_hunter_skips_resistances_below_level_20() {
        let mut world = hunter_with_pet(19, 1);

        with_tx(&mut world, init_pet_spells);

        let events = world.events();
        for &spell in PET_RESISTANCE_SPELLS {
            assert!(
                !events
                    .iter()
                    .any(|e| matches!(e, MockEvent::PetLearnSpell(s) if *s == spell)),
                "pet below level 20 must not learn resistance spell {spell}"
            );
        }
    }

    #[cfg(any(feature = "vanilla", feature = "tbc"))]
    #[test]
    fn init_pet_spells_warlock_imp_learns_firebolt_ranks() {
        let mut world = warlock_with_pet(40, PET_IMP);

        with_tx(&mut world, init_pet_spells);

        let events = world.events();
        // Imp Firebolt ranks 1-5 are learned at levels 1/8/18/28/38.
        // At level 40 the rank 5 (7802) should have been learned and
        // the rank 6 (11762, level 48) should not.
        assert!(
            events
                .iter()
                .any(|e| matches!(e, MockEvent::PetLearnSpell(7802))),
            "imp at level 40 should learn Firebolt rank 5 (7802)"
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, MockEvent::PetLearnSpell(11762))),
            "imp at level 40 must not yet learn Firebolt rank 6 (11762)"
        );
    }

    #[cfg(any(feature = "vanilla", feature = "tbc"))]
    #[test]
    fn init_pet_spells_warlock_no_pet_noop() {
        let mut world = warlock_world(40);
        with_tx(&mut world, init_pet_spells);
        assert!(
            !world
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::PetLearnSpell(_))),
            "warlock without a pet should never learn pet spells"
        );
    }
}
