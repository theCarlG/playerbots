/// Per-bot, class-specific preferences — the slot where player-selectable
/// state that the BT *reads* (never owns) lives.
///
/// Today this covers rogue weapon poisons and shaman totem loadouts; the
/// enum grows one variant per class as future needs appear (hunter pet
/// aggression, warlock soulstone target, druid form preference, etc.).
///
/// The `Bt` tree itself stays pure data — `Bt::ApplyPoisons` and
/// `Bt::DropConfiguredTotems` read `TickContext::settings.class_prefs`
/// and apply the configured choice.
use crate::bot::state::{PlayerClass, PlayerSpec};
use crate::ffi::SpellId;

/// A rogue weapon-poison *kind*. The tick handler picks the highest-ranked
/// spell the bot knows for this kind at apply time — player picks the kind,
/// not the rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoisonKind {
    /// Instant Poison — scales with `AP`, standard main-hand default.
    Instant,
    /// Deadly Poison — `DoT`, standard off-hand default.
    Deadly,
    /// Wound Poison — mortal-strike effect (reduces healing).
    Wound,
    /// Crippling Poison — movement slow.
    Crippling,
    /// Mind-numbing Poison — cast-time slow.
    MindNumbing,
    /// Anesthetic Poison — dispels enrage (WotLK+).
    Anesthetic,
}

impl PoisonKind {
    /// Rank list (lowest → highest) for this poison kind. The tick handler
    /// walks the slice in reverse and casts the first rank the bot knows.
    pub fn ranks(self) -> &'static [SpellId] {
        match self {
            Self::Instant => INSTANT_POISON_RANKS,
            Self::Deadly => DEADLY_POISON_RANKS,
            Self::Wound => WOUND_POISON_RANKS,
            Self::Crippling => CRIPPLING_POISON_RANKS,
            Self::MindNumbing => MIND_NUMBING_POISON_RANKS,
            Self::Anesthetic => ANESTHETIC_POISON_RANKS,
        }
    }

    /// Parse a poison kind from a chat-command token. Matches the
    /// `Mangosbot` addon vocabulary (`poison main instant`, etc).
    pub fn from_token(tok: &str) -> Option<Self> {
        match tok {
            "instant" | "ip" => Some(Self::Instant),
            "deadly" | "dp" => Some(Self::Deadly),
            "wound" | "wp" => Some(Self::Wound),
            "crippling" | "cp" => Some(Self::Crippling),
            "mindnumbing" | "mind-numbing" | "mind" | "mnp" => Some(Self::MindNumbing),
            "anesthetic" | "anaesthetic" => Some(Self::Anesthetic),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Instant => "instant",
            Self::Deadly => "deadly",
            Self::Wound => "wound",
            Self::Crippling => "crippling",
            Self::MindNumbing => "mind",
            Self::Anesthetic => "anesthetic",
        }
    }
}

/// A shaman totem *role* — the semantic slot the player wants filled.
/// Each variant maps to a single totem school (earth/fire/water/air); the
/// tick handler rejects mismatched placement (e.g. `Searing` in the earth
/// slot).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TotemRole {
    // Earth
    StrengthOfEarth,
    Stoneskin,
    Stoneclaw,
    Tremor,
    Earthbind,
    // Fire
    Searing,
    Magma,
    FireNova,
    Flametongue,
    FireResistance,
    // Water
    HealingStream,
    ManaSpring,
    PoisonCleansing,
    DiseaseCleansing,
    FrostResistance,
    // Air
    Windfury,
    GraceOfAir,
    WrathOfAir,
    WindWall,
    Tranquil,
    Grounding,
    NatureResistance,
}

impl TotemRole {
    pub fn slot(self) -> TotemSlot {
        use TotemRole as R;
        match self {
            R::StrengthOfEarth | R::Stoneskin | R::Stoneclaw | R::Tremor | R::Earthbind => {
                TotemSlot::Earth
            }
            R::Searing | R::Magma | R::FireNova | R::Flametongue | R::FireResistance => {
                TotemSlot::Fire
            }
            R::HealingStream
            | R::ManaSpring
            | R::PoisonCleansing
            | R::DiseaseCleansing
            | R::FrostResistance => TotemSlot::Water,
            R::Windfury
            | R::GraceOfAir
            | R::WrathOfAir
            | R::WindWall
            | R::Tranquil
            | R::Grounding
            | R::NatureResistance => TotemSlot::Air,
        }
    }

    /// Rank list (lowest → highest) for this totem role. The tick handler
    /// picks the highest rank the bot knows.
    pub fn ranks(self) -> &'static [SpellId] {
        use TotemRole as R;
        match self {
            R::StrengthOfEarth => STRENGTH_OF_EARTH_RANKS,
            R::Stoneskin => STONESKIN_RANKS,
            R::Stoneclaw => STONECLAW_RANKS,
            R::Tremor => TREMOR_RANKS,
            R::Earthbind => EARTHBIND_RANKS,
            R::Searing => SEARING_RANKS,
            R::Magma => MAGMA_RANKS,
            R::FireNova => FIRE_NOVA_RANKS,
            R::Flametongue => FLAMETONGUE_RANKS,
            R::FireResistance => FIRE_RESISTANCE_RANKS,
            R::HealingStream => HEALING_STREAM_RANKS,
            R::ManaSpring => MANA_SPRING_RANKS,
            R::PoisonCleansing => POISON_CLEANSING_RANKS,
            R::DiseaseCleansing => DISEASE_CLEANSING_RANKS,
            R::FrostResistance => FROST_RESISTANCE_RANKS,
            R::Windfury => WINDFURY_RANKS,
            R::GraceOfAir => GRACE_OF_AIR_RANKS,
            R::WrathOfAir => WRATH_OF_AIR_RANKS,
            R::WindWall => WIND_WALL_RANKS,
            R::Tranquil => TRANQUIL_AIR_RANKS,
            R::Grounding => GROUNDING_RANKS,
            R::NatureResistance => NATURE_RESISTANCE_RANKS,
        }
    }

    /// Parse a totem role from a chat-command token. Case-insensitive,
    /// accepts short forms.
    /// Parse a totem role from a chat-command token. Aliases match the
    /// `Mangosbot` addon vocabulary (`totem earth strength` → `StrengthOfEarth`).
    pub fn from_token(tok: &str) -> Option<Self> {
        use TotemRole as R;
        Some(match tok {
            // Earth
            "strength" | "strengthofearth" | "soe" => R::StrengthOfEarth,
            "stoneskin" | "ss" => R::Stoneskin,
            "stoneclaw" | "claw" => R::Stoneclaw,
            "tremor" => R::Tremor,
            "earthbind" | "eb" => R::Earthbind,
            // Fire
            "searing" | "sear" => R::Searing,
            "magma" => R::Magma,
            "nova" | "firenova" => R::FireNova,
            "flametongue" | "ft" => R::Flametongue,
            // Water
            "healing" | "healingstream" | "hs" => R::HealingStream,
            "mana" | "manaspring" | "ms" => R::ManaSpring,
            "poison" | "poisoncleansing" => R::PoisonCleansing,
            "disease" | "diseasecleansing" => R::DiseaseCleansing,
            // Air
            "windfury" | "wf" => R::Windfury,
            "grace" | "graceofair" | "goa" => R::GraceOfAir,
            "wrath" | "wrathofair" | "woa" => R::WrathOfAir,
            "windwall" | "ww" => R::WindWall,
            "tranquil" | "tranquilair" => R::Tranquil,
            "grounding" => R::Grounding,
            // Resistance totems are school-scoped — caller must pick
            // the right slot; here we parse the school from the name.
            "fireresistance" | "fireres" => R::FireResistance,
            "frostresistance" | "frostres" => R::FrostResistance,
            "natureresistance" | "natureres" | "nr" => R::NatureResistance,
            // "resistance" alone is ambiguous — handled by slot in parser.
            _ => return None,
        })
    }

    /// Resolve a "resistance" shorthand to the correct role for a slot.
    /// Used by the parser when the player writes `totem fire resistance`.
    pub fn resistance_for(slot: TotemSlot) -> Option<Self> {
        Some(match slot {
            TotemSlot::Fire => Self::FireResistance,
            TotemSlot::Water => Self::FrostResistance,
            TotemSlot::Air => Self::NatureResistance,
            TotemSlot::Earth => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        use TotemRole as R;
        match self {
            R::StrengthOfEarth => "strength",
            R::Stoneskin => "stoneskin",
            R::Stoneclaw => "stoneclaw",
            R::Tremor => "tremor",
            R::Earthbind => "earthbind",
            R::Searing => "searing",
            R::Magma => "magma",
            R::FireNova => "nova",
            R::Flametongue => "flametongue",
            R::FireResistance => "fireresistance",
            R::HealingStream => "healing",
            R::ManaSpring => "mana",
            R::PoisonCleansing => "poison",
            R::DiseaseCleansing => "disease",
            R::FrostResistance => "frostresistance",
            R::Windfury => "windfury",
            R::GraceOfAir => "grace",
            R::WrathOfAir => "wrath",
            R::WindWall => "windwall",
            R::Tranquil => "tranquil",
            R::Grounding => "grounding",
            R::NatureResistance => "natureresistance",
        }
    }
}

/// Shaman totem school — one slot each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TotemSlot {
    Earth,
    Fire,
    Water,
    Air,
}

impl TotemSlot {
    pub fn from_token(tok: &str) -> Option<Self> {
        Some(match tok {
            "earth" | "e" => Self::Earth,
            "fire" | "f" => Self::Fire,
            "water" | "w" => Self::Water,
            "air" | "a" => Self::Air,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Earth => "earth",
            Self::Fire => "fire",
            Self::Water => "water",
            Self::Air => "air",
        }
    }
}

/// Which weapon hand a poison applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponHand {
    MainHand,
    OffHand,
}

impl WeaponHand {
    pub fn from_token(tok: &str) -> Option<Self> {
        Some(match tok {
            "mh" | "main" | "mainhand" | "main-hand" => Self::MainHand,
            "oh" | "off" | "offhand" | "off-hand" => Self::OffHand,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::MainHand => "mh",
            Self::OffHand => "oh",
        }
    }

    /// Slot index matching `BotInterface::bot_weapon_enchanted` / the
    /// `ITEM_SLOT_*` layout — 0=main-hand, 1=off-hand.
    pub fn slot_index(self) -> u8 {
        match self {
            Self::MainHand => 0,
            Self::OffHand => 1,
        }
    }
}

/// Rogue player preferences — which poison the bot applies to each weapon.
/// `None` means "leave that hand alone".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RoguePrefs {
    pub mh: Option<PoisonKind>,
    pub oh: Option<PoisonKind>,
}

/// Shaman weapon-imbue kind. Imbues are *self-cast* spells that apply a
/// temporary enchant to the equipped weapon; mutually exclusive with other
/// temporary enchants, including rogue poisons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShamanImbue {
    /// Rockbiter Weapon — flat damage + threat.
    Rockbiter,
    /// Flametongue Weapon — on-hit fire damage.
    Flametongue,
    /// Frostbrand Weapon — slow proc.
    Frostbrand,
    /// Windfury Weapon — extra-swing proc (enhancement signature).
    Windfury,
    /// Earthliving Weapon — healing proc (WotLK+).
    Earthliving,
}

impl ShamanImbue {
    pub fn ranks(self) -> &'static [SpellId] {
        match self {
            Self::Rockbiter => ROCKBITER_WEAPON_RANKS,
            Self::Flametongue => FLAMETONGUE_WEAPON_RANKS,
            Self::Frostbrand => FROSTBRAND_WEAPON_RANKS,
            Self::Windfury => WINDFURY_WEAPON_RANKS,
            Self::Earthliving => EARTHLIVING_WEAPON_RANKS,
        }
    }

    pub fn from_token(tok: &str) -> Option<Self> {
        Some(match tok {
            "rockbiter" | "rb" => Self::Rockbiter,
            "flametongue" | "ft" => Self::Flametongue,
            "frostbrand" | "fb" => Self::Frostbrand,
            "windfury" | "wf" => Self::Windfury,
            "earthliving" | "el" => Self::Earthliving,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rockbiter => "rockbiter",
            Self::Flametongue => "flametongue",
            Self::Frostbrand => "frostbrand",
            Self::Windfury => "windfury",
            Self::Earthliving => "earthliving",
        }
    }
}

/// Shaman player preferences — totems per slot plus weapon imbues.
/// `None` on any field means "leave that slot alone".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShamanPrefs {
    pub earth: Option<TotemRole>,
    pub fire: Option<TotemRole>,
    pub water: Option<TotemRole>,
    pub air: Option<TotemRole>,
    pub mh_imbue: Option<ShamanImbue>,
    pub oh_imbue: Option<ShamanImbue>,
}

impl ShamanPrefs {
    pub fn get(&self, slot: TotemSlot) -> Option<TotemRole> {
        match slot {
            TotemSlot::Earth => self.earth,
            TotemSlot::Fire => self.fire,
            TotemSlot::Water => self.water,
            TotemSlot::Air => self.air,
        }
    }

    pub fn set(&mut self, slot: TotemSlot, role: Option<TotemRole>) {
        // Reject role/slot mismatch silently — the command layer validates
        // before calling, but belt-and-suspenders for future callers.
        if let Some(r) = role
            && r.slot() != slot
        {
            return;
        }
        match slot {
            TotemSlot::Earth => self.earth = role,
            TotemSlot::Fire => self.fire = role,
            TotemSlot::Water => self.water = role,
            TotemSlot::Air => self.air = role,
        }
    }
}

// ── Paladin ─────────────────────────────────────────────────────────────────

/// Paladin self-aura. Only one aura can be active at a time, so this is a
/// single-choice preference rather than a per-slot assignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaladinAura {
    Devotion,
    Retribution,
    Concentration,
    ShadowResistance,
    FrostResistance,
    FireResistance,
    /// Ret talent — not every paladin knows it; `knows_spell` will filter.
    Sanctity,
    /// Mounted-only movement aura (TBC+).
    Crusader,
}

impl PaladinAura {
    pub fn ranks(self) -> &'static [SpellId] {
        match self {
            Self::Devotion => DEVOTION_AURA_RANKS,
            Self::Retribution => RETRIBUTION_AURA_RANKS,
            Self::Concentration => CONCENTRATION_AURA_RANKS,
            Self::ShadowResistance => SHADOW_RESISTANCE_AURA_RANKS,
            Self::FrostResistance => FROST_RESISTANCE_AURA_RANKS,
            Self::FireResistance => FIRE_RESISTANCE_AURA_RANKS,
            Self::Sanctity => SANCTITY_AURA_RANKS,
            Self::Crusader => CRUSADER_AURA_RANKS,
        }
    }

    pub fn from_token(tok: &str) -> Option<Self> {
        Some(match tok {
            "devotion" | "dev" => Self::Devotion,
            "retribution" | "ret" => Self::Retribution,
            "concentration" | "conc" => Self::Concentration,
            "shadow" | "shadowres" => Self::ShadowResistance,
            "frost" | "frostres" => Self::FrostResistance,
            "fire" | "fireres" => Self::FireResistance,
            "sanctity" => Self::Sanctity,
            "crusader" => Self::Crusader,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Devotion => "devotion",
            Self::Retribution => "retribution",
            Self::Concentration => "concentration",
            Self::ShadowResistance => "shadow",
            Self::FrostResistance => "frost",
            Self::FireResistance => "fire",
            Self::Sanctity => "sanctity",
            Self::Crusader => "crusader",
        }
    }
}

/// Paladin group blessing. `Greater*` versions are cast as long-duration
/// group buffs when the bot has the Greater Blessings spells learned;
/// ranks are resolved at cast time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaladinBlessing {
    Might,
    Wisdom,
    Kings,
    Sanctuary,
    Light,
    Salvation,
}

impl PaladinBlessing {
    /// Single-target blessing ranks (lowest → highest).
    pub fn ranks(self) -> &'static [SpellId] {
        match self {
            Self::Might => BLESSING_OF_MIGHT_RANKS,
            Self::Wisdom => BLESSING_OF_WISDOM_RANKS,
            Self::Kings => BLESSING_OF_KINGS_RANKS,
            Self::Sanctuary => BLESSING_OF_SANCTUARY_RANKS,
            Self::Light => BLESSING_OF_LIGHT_RANKS,
            Self::Salvation => BLESSING_OF_SALVATION_RANKS,
        }
    }

    /// Greater (group / long-duration) blessing ranks.
    pub fn greater_ranks(self) -> &'static [SpellId] {
        match self {
            Self::Might => GREATER_BLESSING_OF_MIGHT_RANKS,
            Self::Wisdom => GREATER_BLESSING_OF_WISDOM_RANKS,
            Self::Kings => GREATER_BLESSING_OF_KINGS_RANKS,
            Self::Sanctuary => GREATER_BLESSING_OF_SANCTUARY_RANKS,
            Self::Light => GREATER_BLESSING_OF_LIGHT_RANKS,
            Self::Salvation => GREATER_BLESSING_OF_SALVATION_RANKS,
        }
    }

    pub fn from_token(tok: &str) -> Option<Self> {
        Some(match tok {
            "might" => Self::Might,
            "wisdom" => Self::Wisdom,
            "kings" => Self::Kings,
            "sanctuary" => Self::Sanctuary,
            "light" => Self::Light,
            "salvation" | "sal" => Self::Salvation,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Might => "might",
            Self::Wisdom => "wisdom",
            Self::Kings => "kings",
            Self::Sanctuary => "sanctuary",
            Self::Light => "light",
            Self::Salvation => "salvation",
        }
    }
}

/// Paladin player preferences.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PaladinPrefs {
    /// Self-aura to keep active. `None` = don't maintain any aura.
    pub aura: Option<PaladinAura>,
    /// Blessing to keep on party/raid members. `None` = no auto-blessing.
    pub blessing: Option<PaladinBlessing>,
    /// If true, prefer Greater Blessings (group, long duration) over the
    /// single-target versions when the bot knows the greater rank. Defaults
    /// to true because it's the common raid choice.
    pub use_greater: bool,
}

// ── Hunter ──────────────────────────────────────────────────────────────────

/// Hunter self-aspect. Only one active at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HunterAspect {
    Hawk,
    Monkey,
    Cheetah,
    Pack,
    Beast,
    Wild,
    /// Mana-regen aspect (TBC+).
    Viper,
    /// WotLK replacement for Hawk combined with melee bonus.
    Dragonhawk,
}

impl HunterAspect {
    pub fn ranks(self) -> &'static [SpellId] {
        match self {
            Self::Hawk => ASPECT_HAWK_RANKS,
            Self::Monkey => ASPECT_MONKEY_RANKS,
            Self::Cheetah => ASPECT_CHEETAH_RANKS,
            Self::Pack => ASPECT_PACK_RANKS,
            Self::Beast => ASPECT_BEAST_RANKS,
            Self::Wild => ASPECT_WILD_RANKS,
            Self::Viper => ASPECT_VIPER_RANKS,
            Self::Dragonhawk => ASPECT_DRAGONHAWK_RANKS,
        }
    }

    pub fn from_token(tok: &str) -> Option<Self> {
        Some(match tok {
            "hawk" => Self::Hawk,
            "monkey" => Self::Monkey,
            "cheetah" => Self::Cheetah,
            "pack" => Self::Pack,
            "beast" => Self::Beast,
            "wild" => Self::Wild,
            "viper" => Self::Viper,
            "dragonhawk" | "dh" => Self::Dragonhawk,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hawk => "hawk",
            Self::Monkey => "monkey",
            Self::Cheetah => "cheetah",
            Self::Pack => "pack",
            Self::Beast => "beast",
            Self::Wild => "wild",
            Self::Viper => "viper",
            Self::Dragonhawk => "dragonhawk",
        }
    }
}

/// Hunter trap kind — the trap the bot drops when a trap-action fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HunterTrap {
    Freezing,
    Explosive,
    Immolation,
    Frost,
    Snake,
}

impl HunterTrap {
    pub fn ranks(self) -> &'static [SpellId] {
        match self {
            Self::Freezing => FREEZING_TRAP_RANKS,
            Self::Explosive => EXPLOSIVE_TRAP_RANKS,
            Self::Immolation => IMMOLATION_TRAP_RANKS,
            Self::Frost => FROST_TRAP_RANKS,
            Self::Snake => SNAKE_TRAP_RANKS,
        }
    }

    pub fn from_token(tok: &str) -> Option<Self> {
        Some(match tok {
            "freezing" | "freeze" => Self::Freezing,
            "explosive" | "boom" => Self::Explosive,
            "immolation" | "immo" => Self::Immolation,
            "frost" => Self::Frost,
            "snake" => Self::Snake,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Freezing => "freezing",
            Self::Explosive => "explosive",
            Self::Immolation => "immolation",
            Self::Frost => "frost",
            Self::Snake => "snake",
        }
    }
}

/// Hunter player preferences.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HunterPrefs {
    pub aspect: Option<HunterAspect>,
    pub trap: Option<HunterTrap>,
}

// ── Warlock ─────────────────────────────────────────────────────────────────

/// Warlock curse — only one curse can be active on a target at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarlockCurse {
    Agony,
    Doom,
    Elements,
    Recklessness,
    Weakness,
    Tongues,
    Shadow,
    Exhaustion,
}

impl WarlockCurse {
    pub fn ranks(self) -> &'static [SpellId] {
        match self {
            Self::Agony => CURSE_OF_AGONY_RANKS,
            Self::Doom => CURSE_OF_DOOM_RANKS,
            Self::Elements => CURSE_OF_ELEMENTS_RANKS,
            Self::Recklessness => CURSE_OF_RECKLESSNESS_RANKS,
            Self::Weakness => CURSE_OF_WEAKNESS_RANKS,
            Self::Tongues => CURSE_OF_TONGUES_RANKS,
            Self::Shadow => CURSE_OF_SHADOW_RANKS,
            Self::Exhaustion => CURSE_OF_EXHAUSTION_RANKS,
        }
    }

    pub fn from_token(tok: &str) -> Option<Self> {
        Some(match tok {
            "agony" => Self::Agony,
            "doom" => Self::Doom,
            "elements" | "coe" => Self::Elements,
            "recklessness" | "reckless" | "cor" => Self::Recklessness,
            "weakness" | "cow" => Self::Weakness,
            "tongues" => Self::Tongues,
            "shadow" | "cos" => Self::Shadow,
            "exhaustion" => Self::Exhaustion,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agony => "agony",
            Self::Doom => "doom",
            Self::Elements => "elements",
            Self::Recklessness => "recklessness",
            Self::Weakness => "weakness",
            Self::Tongues => "tongues",
            Self::Shadow => "shadow",
            Self::Exhaustion => "exhaustion",
        }
    }
}

/// Warlock player preferences.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WarlockPrefs {
    /// The curse the bot keeps up on its current target.
    pub curse: Option<WarlockCurse>,
}

// ── Warrior ─────────────────────────────────────────────────────────────────

/// Warrior combat stance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarriorStance {
    Battle,
    Defensive,
    Berserker,
}

impl WarriorStance {
    pub fn spell(self) -> SpellId {
        match self {
            Self::Battle => SpellId(2457),
            Self::Defensive => SpellId(71),
            Self::Berserker => SpellId(2458),
        }
    }

    pub fn from_token(tok: &str) -> Option<Self> {
        Some(match tok {
            "battle" | "bt" => Self::Battle,
            "defensive" | "def" => Self::Defensive,
            "berserker" | "berserk" | "zerk" => Self::Berserker,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Battle => "battle",
            Self::Defensive => "defensive",
            Self::Berserker => "berserker",
        }
    }
}

/// Warrior player preferences.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WarriorPrefs {
    /// Force the bot into a specific stance regardless of what the rotation
    /// would pick. `None` = rotation decides (default).
    pub forced_stance: Option<WarriorStance>,
}

// ── ClassPrefs dispatcher ───────────────────────────────────────────────────

/// Per-class mutable preferences, attached to `BotSettings`. Only the variant
/// matching the bot's class is ever populated; the rest are unreachable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ClassPrefs {
    /// No class-specific prefs for this bot.
    #[default]
    None,
    Rogue(RoguePrefs),
    Shaman(ShamanPrefs),
    Paladin(PaladinPrefs),
    Hunter(HunterPrefs),
    Warlock(WarlockPrefs),
    Warrior(WarriorPrefs),
}

impl ClassPrefs {
    /// Build the default preference block for a class+spec, applied at bot
    /// creation. Gives every bot sensible defaults so they work without any
    /// player commands.
    pub fn default_for(class: PlayerClass, spec: PlayerSpec) -> Self {
        use PlayerSpec as S;
        match class {
            PlayerClass::Rogue => Self::Rogue(RoguePrefs {
                mh: Some(PoisonKind::Instant),
                oh: Some(PoisonKind::Deadly),
            }),
            PlayerClass::Shaman => {
                let mh_imbue = match spec {
                    S::ShamanEnhancement => Some(ShamanImbue::Windfury),
                    S::ShamanRestoration => Some(ShamanImbue::Earthliving),
                    _ => Some(ShamanImbue::Flametongue),
                };
                let oh_imbue = match spec {
                    S::ShamanEnhancement => Some(ShamanImbue::Flametongue),
                    _ => None,
                };
                Self::Shaman(ShamanPrefs {
                    earth: Some(TotemRole::StrengthOfEarth),
                    fire: Some(TotemRole::Searing),
                    water: Some(TotemRole::ManaSpring),
                    air: Some(TotemRole::Windfury),
                    mh_imbue,
                    oh_imbue,
                })
            }
            PlayerClass::Paladin => {
                let (aura, blessing) = match spec {
                    S::PaladinHoly => (PaladinAura::Concentration, PaladinBlessing::Wisdom),
                    S::PaladinProtection => (PaladinAura::Devotion, PaladinBlessing::Kings),
                    _ => (PaladinAura::Retribution, PaladinBlessing::Might),
                };
                Self::Paladin(PaladinPrefs {
                    aura: Some(aura),
                    blessing: Some(blessing),
                    use_greater: true,
                })
            }
            PlayerClass::Hunter => Self::Hunter(HunterPrefs {
                aspect: Some(HunterAspect::Hawk),
                trap: Some(HunterTrap::Freezing),
            }),
            PlayerClass::Warlock => {
                let curse = match spec {
                    S::WarlockAffliction => WarlockCurse::Agony,
                    S::WarlockDemonology => WarlockCurse::Weakness,
                    _ => WarlockCurse::Elements,
                };
                Self::Warlock(WarlockPrefs { curse: Some(curse) })
            }
            PlayerClass::Warrior => Self::Warrior(WarriorPrefs { forced_stance: None }),
            _ => Self::None,
        }
    }

    pub fn as_rogue(&self) -> Option<&RoguePrefs> {
        if let Self::Rogue(r) = self { Some(r) } else { None }
    }
    pub fn as_shaman(&self) -> Option<&ShamanPrefs> {
        if let Self::Shaman(s) = self { Some(s) } else { None }
    }
    pub fn as_paladin(&self) -> Option<&PaladinPrefs> {
        if let Self::Paladin(p) = self { Some(p) } else { None }
    }
    pub fn as_hunter(&self) -> Option<&HunterPrefs> {
        if let Self::Hunter(h) = self { Some(h) } else { None }
    }
    pub fn as_warlock(&self) -> Option<&WarlockPrefs> {
        if let Self::Warlock(w) = self { Some(w) } else { None }
    }
    pub fn as_warrior(&self) -> Option<&WarriorPrefs> {
        if let Self::Warrior(w) = self { Some(w) } else { None }
    }

    pub fn as_rogue_mut(&mut self) -> Option<&mut RoguePrefs> {
        if let Self::Rogue(r) = self { Some(r) } else { None }
    }
    pub fn as_shaman_mut(&mut self) -> Option<&mut ShamanPrefs> {
        if let Self::Shaman(s) = self { Some(s) } else { None }
    }
    pub fn as_paladin_mut(&mut self) -> Option<&mut PaladinPrefs> {
        if let Self::Paladin(p) = self { Some(p) } else { None }
    }
    pub fn as_hunter_mut(&mut self) -> Option<&mut HunterPrefs> {
        if let Self::Hunter(h) = self { Some(h) } else { None }
    }
    pub fn as_warlock_mut(&mut self) -> Option<&mut WarlockPrefs> {
        if let Self::Warlock(w) = self { Some(w) } else { None }
    }
    pub fn as_warrior_mut(&mut self) -> Option<&mut WarriorPrefs> {
        if let Self::Warrior(w) = self { Some(w) } else { None }
    }
}

// ── Rank tables ─────────────────────────────────────────────────────────────
// Lowest → highest. Tick handler walks in reverse and picks the first rank
// the bot has learned. Covers vanilla + TBC ranks; WotLK ranks are added to
// the end where applicable. These live in this module because `class_prefs`
// is where poison/totem *kinds* live — callers shouldn't poke at rank ids.

const INSTANT_POISON_RANKS: &[SpellId] = &[
    SpellId(8680),  // r1
    SpellId(8685),  // r2
    SpellId(8689),  // r3
    SpellId(11335), // r4
    SpellId(11336), // r5
    SpellId(11337), // r6
    SpellId(26890), // r7
];

const DEADLY_POISON_RANKS: &[SpellId] = &[
    SpellId(2823),  // r1
    SpellId(2824),  // r2
    SpellId(11355), // r3
    SpellId(11356), // r4
    SpellId(25349), // r5
    SpellId(26968), // r6
    SpellId(27187), // r7
];

const WOUND_POISON_RANKS: &[SpellId] = &[
    SpellId(13218), // r1
    SpellId(13222), // r2
    SpellId(13223), // r3
    SpellId(13224), // r4
    SpellId(27189), // r5
];

const CRIPPLING_POISON_RANKS: &[SpellId] = &[
    SpellId(3408),  // r1
    SpellId(25349), // r2 (note: same id appears in DP — this is the CP2 spell)
];

const MIND_NUMBING_POISON_RANKS: &[SpellId] = &[SpellId(5761)];

// ── Totem rank tables ───────────────────────────────────────────────────────

const STRENGTH_OF_EARTH_RANKS: &[SpellId] = &[
    SpellId(8075),  // r1
    SpellId(8160),  // r2
    SpellId(8161),  // r3
    SpellId(10442), // r4
    SpellId(25361), // r5 (tbc cap)
];

const STONESKIN_RANKS: &[SpellId] = &[
    SpellId(8071),  // r1
    SpellId(8154),  // r2
    SpellId(8155),  // r3
    SpellId(10406), // r4
    SpellId(10407), // r5
    SpellId(10408), // r6
    SpellId(25508), // r7 (tbc)
];

const TREMOR_RANKS: &[SpellId] = &[SpellId(8143)];

const EARTHBIND_RANKS: &[SpellId] = &[SpellId(2484)];

const SEARING_RANKS: &[SpellId] = &[
    SpellId(3599),  // r1
    SpellId(6363),  // r2
    SpellId(6364),  // r3
    SpellId(6365),  // r4
    SpellId(10437), // r5
    SpellId(10438), // r6
    SpellId(25533), // r7 (tbc)
];

const MAGMA_RANKS: &[SpellId] = &[
    SpellId(8190),  // r1
    SpellId(10585), // r2
    SpellId(10586), // r3
    SpellId(10587), // r4
    SpellId(25552), // r5 (tbc)
];

const FIRE_NOVA_RANKS: &[SpellId] = &[
    SpellId(1535),  // r1
    SpellId(8498),  // r2
    SpellId(8499),  // r3
    SpellId(11314), // r4
    SpellId(11315), // r5
    SpellId(25546), // r6 (tbc)
];

const FLAMETONGUE_RANKS: &[SpellId] = &[
    SpellId(8227),  // r1
    SpellId(8249),  // r2
    SpellId(10526), // r3
    SpellId(16387), // r4
    SpellId(25557), // r5 (tbc)
];

const FIRE_RESISTANCE_RANKS: &[SpellId] = &[
    SpellId(8184),  // r1
    SpellId(10537), // r2
    SpellId(10538), // r3
    SpellId(25563), // r4 (tbc)
];

const HEALING_STREAM_RANKS: &[SpellId] = &[
    SpellId(5394),  // r1
    SpellId(6375),  // r2
    SpellId(6377),  // r3
    SpellId(10462), // r4
    SpellId(10463), // r5
    SpellId(25567), // r6 (tbc)
];

const MANA_SPRING_RANKS: &[SpellId] = &[
    SpellId(5675),  // r1
    SpellId(10495), // r2
    SpellId(10496), // r3
    SpellId(10497), // r4
    SpellId(25570), // r5 (tbc)
];

const POISON_CLEANSING_RANKS: &[SpellId] = &[SpellId(8166)];
const DISEASE_CLEANSING_RANKS: &[SpellId] = &[SpellId(8170)];

const FROST_RESISTANCE_RANKS: &[SpellId] = &[
    SpellId(8181),  // r1
    SpellId(10478), // r2
    SpellId(10479), // r3
    SpellId(25559), // r4 (tbc)
];

const WINDFURY_RANKS: &[SpellId] = &[
    SpellId(8512),  // r1
    SpellId(10613), // r2
    SpellId(10614), // r3
    SpellId(25585), // r4 (tbc)
    SpellId(25587), // r5 (tbc)
];

const GRACE_OF_AIR_RANKS: &[SpellId] = &[
    SpellId(8835),  // r1
    SpellId(10627), // r2
    SpellId(25359), // r3 (tbc)
];

const WIND_WALL_RANKS: &[SpellId] = &[
    SpellId(15107), // r1
    SpellId(15111), // r2
    SpellId(15112), // r3
];

const NATURE_RESISTANCE_RANKS: &[SpellId] = &[
    SpellId(10595), // r1
    SpellId(10600), // r2
    SpellId(10601), // r3
    SpellId(25573), // r4 (tbc)
];

const STONECLAW_RANKS: &[SpellId] = &[
    SpellId(5730),  // r1
    SpellId(6390),  // r2
    SpellId(6391),  // r3
    SpellId(6392),  // r4
    SpellId(10427), // r5
    SpellId(10428), // r6
    SpellId(25525), // r7 (tbc)
];

const WRATH_OF_AIR_RANKS: &[SpellId] = &[SpellId(3738), SpellId(25587)];
const TRANQUIL_AIR_RANKS: &[SpellId] = &[SpellId(25908)];
const GROUNDING_RANKS: &[SpellId] = &[SpellId(8177)];

const ANESTHETIC_POISON_RANKS: &[SpellId] = &[SpellId(57666)]; // WotLK+

// ── Shaman weapon imbues ────────────────────────────────────────────────────

const ROCKBITER_WEAPON_RANKS: &[SpellId] = &[
    SpellId(8017),  // r1
    SpellId(8018),  // r2
    SpellId(8019),  // r3
    SpellId(10399), // r4
    SpellId(16314), // r5
    SpellId(16315), // r6
    SpellId(16316), // r7
    SpellId(25479), // r8 (tbc)
];

const FLAMETONGUE_WEAPON_RANKS: &[SpellId] = &[
    SpellId(8024),  // r1
    SpellId(8027),  // r2
    SpellId(8030),  // r3
    SpellId(16339), // r4
    SpellId(16341), // r5
    SpellId(16342), // r6
    SpellId(25489), // r7 (tbc)
];

const FROSTBRAND_WEAPON_RANKS: &[SpellId] = &[
    SpellId(8033),  // r1
    SpellId(8038),  // r2
    SpellId(10456), // r3
    SpellId(16355), // r4
    SpellId(16356), // r5
    SpellId(25500), // r6 (tbc)
];

const WINDFURY_WEAPON_RANKS: &[SpellId] = &[
    SpellId(8232),  // r1
    SpellId(8235),  // r2
    SpellId(10486), // r3
    SpellId(16362), // r4
    SpellId(25505), // r5 (tbc)
];

const EARTHLIVING_WEAPON_RANKS: &[SpellId] = &[
    SpellId(51730), // r1 (WotLK)
    SpellId(51988), // r2
    SpellId(52004), // r3
    SpellId(52005), // r4
    SpellId(52006), // r5
];

// ── Paladin auras ───────────────────────────────────────────────────────────

const DEVOTION_AURA_RANKS: &[SpellId] = &[
    SpellId(465),   // r1
    SpellId(10290), // r2
    SpellId(643),   // r3
    SpellId(10291), // r4
    SpellId(1032),  // r5
    SpellId(10292), // r6
    SpellId(10293), // r7
    SpellId(27149), // r8 (tbc)
];

const RETRIBUTION_AURA_RANKS: &[SpellId] = &[
    SpellId(7294),  // r1
    SpellId(10298), // r2
    SpellId(10299), // r3
    SpellId(10300), // r4
    SpellId(10301), // r5
    SpellId(27150), // r6 (tbc)
];

const CONCENTRATION_AURA_RANKS: &[SpellId] = &[SpellId(19746)];

const SHADOW_RESISTANCE_AURA_RANKS: &[SpellId] = &[
    SpellId(19876), // r1
    SpellId(19895), // r2
    SpellId(19896), // r3
    SpellId(27151), // r4 (tbc)
];

const FROST_RESISTANCE_AURA_RANKS: &[SpellId] = &[
    SpellId(19888), // r1
    SpellId(19897), // r2
    SpellId(19898), // r3
    SpellId(27152), // r4 (tbc)
];

const FIRE_RESISTANCE_AURA_RANKS: &[SpellId] = &[
    SpellId(19891), // r1
    SpellId(19899), // r2
    SpellId(19900), // r3
    SpellId(27153), // r4 (tbc)
];

const SANCTITY_AURA_RANKS: &[SpellId] = &[SpellId(20218)];
const CRUSADER_AURA_RANKS: &[SpellId] = &[SpellId(32223)];

// ── Paladin blessings ───────────────────────────────────────────────────────

const BLESSING_OF_MIGHT_RANKS: &[SpellId] = &[
    SpellId(19740), // r1
    SpellId(19834), // r2
    SpellId(19835), // r3
    SpellId(19836), // r4
    SpellId(19837), // r5
    SpellId(19838), // r6
    SpellId(25291), // r7 (tbc)
    SpellId(27140), // r8 (tbc)
];

const BLESSING_OF_WISDOM_RANKS: &[SpellId] = &[
    SpellId(19742), // r1
    SpellId(19850), // r2
    SpellId(19852), // r3
    SpellId(19853), // r4
    SpellId(19854), // r5
    SpellId(25290), // r6 (tbc)
    SpellId(27142), // r7 (tbc)
];

const BLESSING_OF_KINGS_RANKS: &[SpellId] = &[SpellId(20217)];

const BLESSING_OF_SANCTUARY_RANKS: &[SpellId] = &[
    SpellId(20911), // r1
    SpellId(20912), // r2
    SpellId(20913), // r3
    SpellId(20914), // r4
    SpellId(25899), // r5 (tbc)
    SpellId(27168), // r6 (tbc)
];

const BLESSING_OF_LIGHT_RANKS: &[SpellId] = &[
    SpellId(19977), // r1
    SpellId(19978), // r2
    SpellId(19979), // r3
    SpellId(27144), // r4 (tbc)
];

const BLESSING_OF_SALVATION_RANKS: &[SpellId] = &[SpellId(1038)];

// Greater Blessings — group, 15-minute duration.
const GREATER_BLESSING_OF_MIGHT_RANKS: &[SpellId] = &[
    SpellId(25782), // r1
    SpellId(25916), // r2
    SpellId(27141), // r3 (tbc)
];

const GREATER_BLESSING_OF_WISDOM_RANKS: &[SpellId] = &[
    SpellId(25894), // r1
    SpellId(25918), // r2
    SpellId(27143), // r3 (tbc)
];

const GREATER_BLESSING_OF_KINGS_RANKS: &[SpellId] = &[SpellId(25898)];

const GREATER_BLESSING_OF_SANCTUARY_RANKS: &[SpellId] = &[
    SpellId(25899), // r1
    SpellId(27169), // r2 (tbc)
];

const GREATER_BLESSING_OF_LIGHT_RANKS: &[SpellId] = &[
    SpellId(25890), // r1
    SpellId(27145), // r2 (tbc)
];

const GREATER_BLESSING_OF_SALVATION_RANKS: &[SpellId] = &[SpellId(25895)];

// ── Hunter aspects ──────────────────────────────────────────────────────────

const ASPECT_HAWK_RANKS: &[SpellId] = &[
    SpellId(13165), // r1
    SpellId(14318), // r2
    SpellId(14319), // r3
    SpellId(14320), // r4
    SpellId(14321), // r5
    SpellId(14322), // r6
    SpellId(25296), // r7 (tbc)
    SpellId(27044), // r8 (tbc)
];

const ASPECT_MONKEY_RANKS: &[SpellId] = &[SpellId(13163)];
const ASPECT_CHEETAH_RANKS: &[SpellId] = &[SpellId(5118)];
const ASPECT_PACK_RANKS: &[SpellId] = &[SpellId(13159)];
const ASPECT_BEAST_RANKS: &[SpellId] = &[SpellId(13161)];

const ASPECT_WILD_RANKS: &[SpellId] = &[
    SpellId(20043), // r1
    SpellId(20190), // r2
    SpellId(27045), // r3 (tbc)
];

const ASPECT_VIPER_RANKS: &[SpellId] = &[SpellId(34074)];
const ASPECT_DRAGONHAWK_RANKS: &[SpellId] = &[SpellId(61846), SpellId(61847)]; // WotLK

// ── Hunter traps ────────────────────────────────────────────────────────────

const FREEZING_TRAP_RANKS: &[SpellId] = &[
    SpellId(1499),  // r1
    SpellId(14310), // r2
    SpellId(14311), // r3
    SpellId(60192), // r4 (wotlk)
];

const EXPLOSIVE_TRAP_RANKS: &[SpellId] = &[
    SpellId(13813), // r1
    SpellId(14316), // r2
    SpellId(14317), // r3
    SpellId(27025), // r4 (tbc)
];

const IMMOLATION_TRAP_RANKS: &[SpellId] = &[
    SpellId(13795), // r1
    SpellId(14302), // r2
    SpellId(14303), // r3
    SpellId(14304), // r4
    SpellId(14305), // r5
    SpellId(27023), // r6 (tbc)
];

const FROST_TRAP_RANKS: &[SpellId] = &[SpellId(13809)];
const SNAKE_TRAP_RANKS: &[SpellId] = &[SpellId(34600)];

// ── Warlock curses ──────────────────────────────────────────────────────────

const CURSE_OF_AGONY_RANKS: &[SpellId] = &[
    SpellId(980),   // r1
    SpellId(1014),  // r2
    SpellId(6217),  // r3
    SpellId(11711), // r4
    SpellId(11712), // r5
    SpellId(11713), // r6
    SpellId(27218), // r7 (tbc)
];

const CURSE_OF_DOOM_RANKS: &[SpellId] = &[SpellId(603), SpellId(30910)];

const CURSE_OF_ELEMENTS_RANKS: &[SpellId] = &[
    SpellId(1490),  // r1
    SpellId(11721), // r2
    SpellId(11722), // r3
    SpellId(27228), // r4 (tbc)
];

const CURSE_OF_RECKLESSNESS_RANKS: &[SpellId] = &[
    SpellId(704),   // r1
    SpellId(7658),  // r2
    SpellId(7659),  // r3
    SpellId(11717), // r4
];

const CURSE_OF_WEAKNESS_RANKS: &[SpellId] = &[
    SpellId(702),   // r1
    SpellId(1108),  // r2
    SpellId(2580),  // r3
    SpellId(11707), // r4
    SpellId(11708), // r5
    SpellId(27224), // r6 (tbc)
];

const CURSE_OF_TONGUES_RANKS: &[SpellId] = &[SpellId(1714), SpellId(11719)];

const CURSE_OF_SHADOW_RANKS: &[SpellId] = &[
    SpellId(17862), // r1
    SpellId(17937), // r2
    SpellId(27226), // r3 (tbc)
];

const CURSE_OF_EXHAUSTION_RANKS: &[SpellId] = &[SpellId(18223)];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rogue_assa_gets_instant_and_deadly() {
        let prefs = ClassPrefs::default_for(PlayerClass::Rogue, PlayerSpec::RogueAssassination);
        let r = prefs.as_rogue().expect("rogue variant");
        assert_eq!(r.mh, Some(PoisonKind::Instant));
        assert_eq!(r.oh, Some(PoisonKind::Deadly));
    }

    #[test]
    fn default_enh_shaman_gets_windfury_and_soe() {
        let prefs = ClassPrefs::default_for(PlayerClass::Shaman, PlayerSpec::ShamanEnhancement);
        let s = prefs.as_shaman().expect("shaman variant");
        assert_eq!(s.earth, Some(TotemRole::StrengthOfEarth));
        assert_eq!(s.air, Some(TotemRole::Windfury));
        assert_eq!(s.water, Some(TotemRole::ManaSpring));
        assert_eq!(s.fire, Some(TotemRole::Searing));
    }

    #[test]
    fn default_warrior_has_no_forced_stance() {
        let prefs = ClassPrefs::default_for(PlayerClass::Warrior, PlayerSpec::WarriorArms);
        let w = prefs.as_warrior().expect("warrior variant");
        assert_eq!(w.forced_stance, None);
        assert!(prefs.as_rogue().is_none());
        assert!(prefs.as_shaman().is_none());
    }

    #[test]
    fn default_ret_paladin_gets_retribution_aura_and_might() {
        let prefs =
            ClassPrefs::default_for(PlayerClass::Paladin, PlayerSpec::PaladinRetribution);
        let p = prefs.as_paladin().expect("paladin variant");
        assert_eq!(p.aura, Some(PaladinAura::Retribution));
        assert_eq!(p.blessing, Some(PaladinBlessing::Might));
        assert!(p.use_greater);
    }

    #[test]
    fn default_holy_paladin_gets_concentration_and_wisdom() {
        let prefs = ClassPrefs::default_for(PlayerClass::Paladin, PlayerSpec::PaladinHoly);
        let p = prefs.as_paladin().expect("paladin variant");
        assert_eq!(p.aura, Some(PaladinAura::Concentration));
        assert_eq!(p.blessing, Some(PaladinBlessing::Wisdom));
    }

    #[test]
    fn default_prot_paladin_gets_devotion_and_kings() {
        let prefs =
            ClassPrefs::default_for(PlayerClass::Paladin, PlayerSpec::PaladinProtection);
        let p = prefs.as_paladin().expect("paladin variant");
        assert_eq!(p.aura, Some(PaladinAura::Devotion));
        assert_eq!(p.blessing, Some(PaladinBlessing::Kings));
    }

    #[test]
    fn default_hunter_gets_hawk_and_freezing_trap() {
        let prefs =
            ClassPrefs::default_for(PlayerClass::Hunter, PlayerSpec::HunterMarksmanship);
        let h = prefs.as_hunter().expect("hunter variant");
        assert_eq!(h.aspect, Some(HunterAspect::Hawk));
        assert_eq!(h.trap, Some(HunterTrap::Freezing));
    }

    #[test]
    fn default_affliction_warlock_gets_curse_of_agony() {
        let prefs =
            ClassPrefs::default_for(PlayerClass::Warlock, PlayerSpec::WarlockAffliction);
        let w = prefs.as_warlock().expect("warlock variant");
        assert_eq!(w.curse, Some(WarlockCurse::Agony));
    }

    #[test]
    fn default_destro_warlock_gets_curse_of_elements() {
        let prefs =
            ClassPrefs::default_for(PlayerClass::Warlock, PlayerSpec::WarlockDestruction);
        let w = prefs.as_warlock().expect("warlock variant");
        assert_eq!(w.curse, Some(WarlockCurse::Elements));
    }

    #[test]
    fn default_enh_shaman_gets_windfury_mh_flametongue_oh() {
        let prefs = ClassPrefs::default_for(PlayerClass::Shaman, PlayerSpec::ShamanEnhancement);
        let s = prefs.as_shaman().expect("shaman variant");
        assert_eq!(s.mh_imbue, Some(ShamanImbue::Windfury));
        assert_eq!(s.oh_imbue, Some(ShamanImbue::Flametongue));
    }

    #[test]
    fn default_resto_shaman_gets_earthliving_mh_no_oh() {
        let prefs = ClassPrefs::default_for(PlayerClass::Shaman, PlayerSpec::ShamanRestoration);
        let s = prefs.as_shaman().expect("shaman variant");
        assert_eq!(s.mh_imbue, Some(ShamanImbue::Earthliving));
        assert_eq!(s.oh_imbue, None);
    }

    #[test]
    fn totem_role_slot_matches_schools() {
        assert_eq!(TotemRole::Windfury.slot(), TotemSlot::Air);
        assert_eq!(TotemRole::StrengthOfEarth.slot(), TotemSlot::Earth);
        assert_eq!(TotemRole::ManaSpring.slot(), TotemSlot::Water);
        assert_eq!(TotemRole::Searing.slot(), TotemSlot::Fire);
    }

    #[test]
    fn shaman_prefs_rejects_mismatched_slot() {
        let mut s = ShamanPrefs::default();
        // Try to put Windfury (air) in the earth slot — should be rejected.
        s.set(TotemSlot::Earth, Some(TotemRole::Windfury));
        assert_eq!(s.earth, None);
        // Correct placement works.
        s.set(TotemSlot::Air, Some(TotemRole::Windfury));
        assert_eq!(s.air, Some(TotemRole::Windfury));
    }

    #[test]
    fn poison_kind_token_roundtrip() {
        for k in [
            PoisonKind::Instant,
            PoisonKind::Deadly,
            PoisonKind::Wound,
            PoisonKind::Crippling,
            PoisonKind::MindNumbing,
        ] {
            assert_eq!(PoisonKind::from_token(k.as_str()), Some(k));
        }
    }

    #[test]
    fn totem_role_token_roundtrip() {
        for r in [
            TotemRole::StrengthOfEarth,
            TotemRole::Searing,
            TotemRole::ManaSpring,
            TotemRole::Windfury,
            TotemRole::GraceOfAir,
        ] {
            assert_eq!(TotemRole::from_token(r.as_str()), Some(r));
        }
    }
}
