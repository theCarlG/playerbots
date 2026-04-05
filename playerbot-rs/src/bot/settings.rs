/// Per-bot runtime settings — modified by chat commands, read by the BT.
///
/// Stored on `BotState`, passed into `TickContext` as `&BotSettings`.
/// Commands mutate settings between ticks (never during BT execution).
use std::collections::{HashMap, HashSet};

use crate::bot::class_prefs::ClassPrefs;
use crate::bot::encounter_prefs::EncounterPrefs;
use crate::ffi::{ItemId, SpellId, UnitHandle};

/// What the bot does when not given a specific order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BehaviorMode {
    /// Follow the master, assist in combat.
    #[default]
    Follow,
    /// Stay at current position, fight if attacked.
    Stay,
    /// Kill nearby mobs autonomously.
    Grind,
    /// Work on active quests.
    Quest,
    /// Do nothing unless directly commanded.
    Passive,
    /// Stay near a position, fight hostiles in range.
    Guard,
    /// Wander town, interact with NPCs.
    Rpg,
    /// Battleground autonomous behavior.
    Bg,
}

/// How the bot selects combat targets. Bitflags-style — multiple flags may
/// coexist (e.g. `TANK | ASSIST`, `TANK_ASSIST | DPS_ASSIST`).
///
/// Matches the C++ bitfield semantics the `RaidControl` addon drives:
/// `co +tank`, `co -threat`, `co +tank assist,+dps assist`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CombatOrder(pub u32);

impl CombatOrder {
    pub const NONE: Self = Self(0);
    pub const TANK: Self = Self(1 << 0);
    pub const ASSIST: Self = Self(1 << 1);
    pub const PROTECT: Self = Self(1 << 2);
    pub const PULL: Self = Self(1 << 3);
    pub const THREAT: Self = Self(1 << 4);
    pub const PASSIVE: Self = Self(1 << 5);
    pub const FURY: Self = Self(1 << 6);
    pub const DPS: Self = Self(1 << 7);
    pub const CLOSE: Self = Self(1 << 8);
    pub const AOE: Self = Self(1 << 9);
    pub const GRIND: Self = Self(1 << 10);
    pub const TANK_ASSIST: Self = Self(1 << 11);
    pub const DPS_ASSIST: Self = Self(1 << 12);
    pub const PULL_BACK: Self = Self(1 << 13);

    /// True if all bits in `other` are set in `self`.
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }
    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Parse a flag name from the addon vocabulary. Supports multi-word
    /// forms ("tank assist", "dps assist", "pull back").
    ///
    /// Consumes 1 or 2 tokens from `tokens` — returns `(flag, consumed)`.
    pub fn parse_flag(tokens: &[&str]) -> Option<(Self, usize)> {
        let first = *tokens.first()?;
        if let Some(second) = tokens.get(1).copied() {
            // Try 2-word match first (greedy).
            let pair: Self = match (first, second) {
                ("tank", "assist") => Self::TANK_ASSIST,
                ("dps", "assist") => Self::DPS_ASSIST,
                ("pull", "back") => Self::PULL_BACK,
                _ => Self::NONE,
            };
            if !pair.is_empty() {
                return Some((pair, 2));
            }
        }
        let single: Self = match first {
            "tank" => Self::TANK,
            "assist" => Self::ASSIST,
            "protect" => Self::PROTECT,
            "pull" => Self::PULL,
            "threat" => Self::THREAT,
            "passive" => Self::PASSIVE,
            "fury" => Self::FURY,
            "dps" => Self::DPS,
            "close" => Self::CLOSE,
            "aoe" => Self::AOE,
            "grind" => Self::GRIND,
            _ => return None,
        };
        Some((single, 1))
    }
}

impl Default for CombatOrder {
    fn default() -> Self {
        Self::ASSIST
    }
}

impl std::ops::BitOr for CombatOrder {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for CombatOrder {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// Named strategies the bot can toggle at runtime via `nc +x,-y` commands.
///
/// Mirrors the C++ "strategies" vocabulary the `RaidControl` addon sends.
/// Each flag gates one optional subtree in the root BT. Core behavior
/// (combat rotations, reactive layer, mode dispatch) is NOT a strategy —
/// strategies are the knobs a raid leader turns during a pull.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StrategyFlags(pub u32);

impl StrategyFlags {
    pub const NONE: Self = Self(0);
    pub const RPG: Self = Self(1 << 0);
    pub const RPG_BG: Self = Self(1 << 1);
    pub const RPG_EXPLORE: Self = Self(1 << 2);
    pub const RPG_GUILD: Self = Self(1 << 3);
    pub const RPG_MAINTENANCE: Self = Self(1 << 4);
    pub const RPG_PLAYER: Self = Self(1 << 5);
    pub const RPG_QUEST: Self = Self(1 << 6);
    pub const RPG_VENDOR: Self = Self(1 << 7);
    pub const RTSC: Self = Self(1 << 8);
    pub const WBUFF: Self = Self(1 << 9);
    pub const GRIND: Self = Self(1 << 10);
    pub const FLEE: Self = Self(1 << 11);
    pub const EMOTE: Self = Self(1 << 12);
    pub const CC: Self = Self(1 << 13);

    /// Default set enabled at bot creation. Matches the C++ "defaults"
    /// loadout: reactive flee, RTSC accepting, basic RPG.
    pub const fn defaults() -> Self {
        Self(Self::RTSC.0 | Self::FLEE.0 | Self::RPG.0 | Self::RPG_MAINTENANCE.0)
    }

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }
    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }

    /// Look up a flag by the name the addon sends. Multi-word names are
    /// joined with a space ("rpg bg", "rpg maintenance").
    pub fn parse_name(name: &str) -> Option<Self> {
        Some(match name.trim() {
            "rpg" => Self::RPG,
            "rpg bg" => Self::RPG_BG,
            "rpg explore" => Self::RPG_EXPLORE,
            "rpg guild" => Self::RPG_GUILD,
            "rpg maintenance" => Self::RPG_MAINTENANCE,
            "rpg player" => Self::RPG_PLAYER,
            "rpg quest" => Self::RPG_QUEST,
            "rpg vendor" => Self::RPG_VENDOR,
            "rtsc" => Self::RTSC,
            "wbuff" => Self::WBUFF,
            "grind" => Self::GRIND,
            "flee" => Self::FLEE,
            "emote" => Self::EMOTE,
            "cc" => Self::CC,
            _ => return None,
        })
    }
}

impl Default for StrategyFlags {
    fn default() -> Self {
        Self::defaults()
    }
}

impl std::ops::BitOr for StrategyFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// How followers arrange themselves around the master when in Follow mode.
/// Mirrors the C++ formation vocabulary the `RaidControl` addon sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FollowFormation {
    /// Cluster tightly near the leader (default).
    #[default]
    Near,
    /// Side-by-side line.
    Line,
    /// Circle around the leader.
    Circle,
    /// Random-ish scatter.
    Chaos,
    /// 3x3 box.
    Box,
    /// Single-file queue behind leader.
    Queue,
    /// Arrow / V shape.
    Arrow,
    /// Wedge shape (inverted arrow).
    Wedge,
    /// Paired buddies.
    Pairs,
}

impl FollowFormation {
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "near" => Self::Near,
            "line" => Self::Line,
            "circle" => Self::Circle,
            "chaos" => Self::Chaos,
            "box" => Self::Box,
            "queue" => Self::Queue,
            "arrow" => Self::Arrow,
            "wedge" => Self::Wedge,
            "pairs" => Self::Pairs,
            _ => return None,
        })
    }
}

/// Reactivity level for autonomous combat engagement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Reactivity {
    /// Never engage unless ordered.
    Passive,
    /// Fight back when attacked.
    #[default]
    Defensive,
    /// Engage any hostile in range.
    Aggressive,
}

/// Per-bot runtime settings. Modified by chat commands.
#[derive(Debug, Clone)]
pub struct BotSettings {
    // -- Behavior --
    pub mode: BehaviorMode,
    pub combat_order: CombatOrder,
    pub reactivity: Reactivity,
    pub strategies: StrategyFlags,

    // -- Combat tuning --
    pub focus_target: Option<UnitHandle>,
    pub protect_target: Option<UnitHandle>,
    pub spell_blacklist: HashSet<SpellId>,
    pub max_combat_range: f32,
    pub flee_hp_pct: f32,

    // -- Healing priorities --
    pub heal_self_threshold: f32,
    pub heal_party_threshold: f32,

    // -- Movement --
    pub follow_distance: f32,
    pub follow_formation: FollowFormation,
    pub guard_position: Option<(f32, f32, f32)>,
    /// Set when a `flee` / `runaway` / `panic` command arrives. Cleared
    /// when the flee reaches its destination or after ~5 seconds.
    pub flee_override_until_ms: u64,

    // -- Economy --
    pub auto_repair: bool,
    pub auto_vendor: bool,

    // -- Features --
    pub auto_loot: bool,
    pub auto_mount: bool,
    pub auto_resurrect: bool,
    pub auto_accept_quest: bool,
    pub verbose: bool,

    // -- RTSC (Real-Time Strategy Control) --
    /// Whether this bot is currently selected for RTSC commands.
    pub rtsc_selected: bool,
    /// Pending action for the next spell-target position.
    pub rtsc_pending_action: Option<RtscAction>,
    /// Named waypoints saved via RTSC.
    pub rtsc_waypoints: HashMap<String, (f32, f32, f32)>,

    // -- Misc tunables driven by chat commands --
    /// Warrior stance (0=none, 1=battle, 2=defensive, 3=berserker).
    /// Ignored by non-warrior classes.
    pub stance: u8,
    /// `save mana` toggle — when true, the bot prefers cheap casts and avoids
    /// full-cost rotation spells until mana is topped up.
    pub save_mana: bool,
    /// `self res` toggle — when true, the bot will use a soulstone / ankh /
    /// reincarnation when it dies, instead of running back from graveyard.
    pub self_res: bool,
    /// `cheat <flags>` — dev-only bitfield. Specific flags are interpreted by
    /// the BT/world modules; zero means no cheats active.
    pub cheat_flags: u32,
    /// Items the bot should never sell, destroy, or disenchant. Populated by
    /// the `keep <itemid>` command.
    pub keep_items: HashSet<ItemId>,
    /// Which chat channels the bot should send verbose replies on. Bitfield
    /// matching [`ChatChannel`]. Default is none (silent).
    pub chat_channels: u32,
    /// Persistent raid-target-icon preference set by `rti <icon>`. When set,
    /// world/combat modules may use it as the bot's default focus icon.
    pub preferred_rti_icon: Option<u8>,

    /// Class-specific preferences (rogue weapon poisons, shaman totem
    /// loadout, etc). Only the variant matching the bot's class is ever
    /// populated — seeded at `BotState::new` time via
    /// `ClassPrefs::default_for`, mutated by chat commands.
    pub class_prefs: ClassPrefs,

    /// Cross-boss / instance-wide duty preferences (BWL suppression
    /// disarm, MC rune dousing). Class-agnostic — a raid leader
    /// whispers bots individually to designate carriers or opt bots
    /// out. Default: every field is [`DutyMode::Auto`], so any
    /// eligible bot participates.
    pub encounter_prefs: EncounterPrefs,
}

/// Chat channel bitfield for `BotSettings::chat_channels`. Mirrors the PB2
/// `chat` command verbosity toggles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ChatChannel {
    Say = 1 << 0,
    Party = 1 << 1,
    Raid = 1 << 2,
    Guild = 1 << 3,
    Whisper = 1 << 4,
}

impl ChatChannel {
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "say" | "s" => Self::Say,
            "party" | "p" => Self::Party,
            "raid" | "r" => Self::Raid,
            "guild" | "g" => Self::Guild,
            "whisper" | "w" => Self::Whisper,
            _ => return None,
        })
    }
}

/// What to do with the next RTSC spell-target position.
#[derive(Debug, Clone, PartialEq)]
pub enum RtscAction {
    /// Move to the position. `exact` = skip formation offset.
    Move { exact: bool },
    /// Save the position as a named waypoint.
    Save { name: String },
}

impl Default for BotSettings {
    fn default() -> Self {
        Self {
            mode: BehaviorMode::Follow,
            combat_order: CombatOrder::ASSIST,
            reactivity: Reactivity::Defensive,
            strategies: StrategyFlags::defaults(),
            focus_target: None,
            protect_target: None,
            spell_blacklist: HashSet::new(),
            max_combat_range: 40.0,
            flee_hp_pct: 0.0,
            heal_self_threshold: 0.60,
            heal_party_threshold: 0.80,
            follow_distance: 3.0,
            follow_formation: FollowFormation::Near,
            guard_position: None,
            flee_override_until_ms: 0,
            auto_repair: true,
            auto_vendor: true,
            auto_loot: true,
            auto_mount: true,
            auto_resurrect: true,
            auto_accept_quest: true,
            verbose: false,
            rtsc_selected: false,
            rtsc_pending_action: None,
            rtsc_waypoints: HashMap::new(),
            stance: 0,
            save_mana: false,
            self_res: false,
            cheat_flags: 0,
            keep_items: HashSet::new(),
            chat_channels: 0,
            preferred_rti_icon: None,
            class_prefs: ClassPrefs::None,
            encounter_prefs: EncounterPrefs::default(),
        }
    }
}

impl BehaviorMode {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "follow" => Some(Self::Follow),
            "stay" => Some(Self::Stay),
            "grind" => Some(Self::Grind),
            "quest" => Some(Self::Quest),
            "passive" => Some(Self::Passive),
            "guard" => Some(Self::Guard),
            "rpg" | "wander" => Some(Self::Rpg),
            "bg" => Some(Self::Bg),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Follow => "follow",
            Self::Stay => "stay",
            Self::Grind => "grind",
            Self::Quest => "quest",
            Self::Passive => "passive",
            Self::Guard => "guard",
            Self::Rpg => "rpg",
            Self::Bg => "bg",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_are_sane() {
        let s = BotSettings::default();
        assert_eq!(s.mode, BehaviorMode::Follow);
        assert_eq!(s.combat_order, CombatOrder::ASSIST);
        assert_eq!(s.reactivity, Reactivity::Defensive);
        assert!(s.spell_blacklist.is_empty());
        assert!(s.auto_loot);
        assert!(s.auto_repair);
    }

    #[test]
    fn behavior_mode_roundtrip() {
        for mode in [
            BehaviorMode::Follow,
            BehaviorMode::Stay,
            BehaviorMode::Grind,
            BehaviorMode::Quest,
            BehaviorMode::Passive,
            BehaviorMode::Guard,
            BehaviorMode::Rpg,
            BehaviorMode::Bg,
        ] {
            assert_eq!(BehaviorMode::from_str(mode.as_str()), Some(mode));
        }
    }
}
