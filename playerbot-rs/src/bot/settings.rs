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
    /// Use offensive cooldowns/trinkets during combat ("burst DPS"). Read
    /// by class rotations to decide whether to pop Blood Fury / Berserking /
    /// Blade Flurry / trinkets / Bloodlust etc. Alias: `i` (Mangosbot icon).
    pub const BOOST: Self = Self(1 << 14);
    /// Position behind the target (melee dps). Matches the
    /// `RaidControl` `+behind/-behind` hint.
    pub const BEHIND: Self = Self(1 << 15);
    /// Hold off damage until the tank has a few seconds of threat.
    /// Multi-word name: `wait for attack`.
    pub const WAIT_FOR_ATTACK: Self = Self(1 << 16);
    /// Prefer crowd-control spells (mage sheep, priest shackle, hunter trap,
    /// druid roots, warlock banish, rogue sap). `RaidControl` sends
    /// `@mage co +cc` before pulls with multiple caster targets.
    pub const CC: Self = Self(1 << 17);
    /// Fight at ranged distance (default for casters/hunters). The
    /// `RaidControl` `+range` / `+ranged` alias.
    pub const RANGED: Self = Self(1 << 18);
    /// Role flag: prefer healing-centric rotation. `@priest co +heal` etc.
    pub const HEAL: Self = Self(1 << 19);
    /// Shaman restoration spec hint (`@shaman co +restoration`). Distinct
    /// from `HEAL` in that it implies totem layouts + water-shield usage.
    pub const RESTORATION: Self = Self(1 << 20);
    /// Prefer stealthed opening / shadowmeld on engage.
    pub const STEALTH: Self = Self(1 << 21);
    /// Druid feral form hint (`@druid co +feral`, `+tank feral`,
    /// `+dps feral`). Decides bear vs cat form on engage.
    pub const FERAL: Self = Self(1 << 22);
    /// Druid main-tank feral (bear-form tank). Multi-word: `tank feral`.
    pub const TANK_FERAL: Self = Self(1 << 23);
    /// Druid dps feral (cat-form dps). Multi-word: `dps feral`.
    pub const DPS_FERAL: Self = Self(1 << 24);

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
    /// forms ("tank assist", "dps assist", "pull back", "wait for attack",
    /// "tank feral", "dps feral"). Returns `(flag, words_consumed)`.
    ///
    /// Greedy: always tries the longest match first (3-word → 2-word →
    /// 1-word) so `wait for attack` is parsed as a single flag rather than
    /// three bad words.
    pub fn parse_flag(tokens: &[&str]) -> Option<(Self, usize)> {
        let first = *tokens.first()?;
        // 3-word: `wait for attack`.
        if let (Some(b), Some(c)) = (tokens.get(1).copied(), tokens.get(2).copied())
            && (first, b, c) == ("wait", "for", "attack")
        {
            return Some((Self::WAIT_FOR_ATTACK, 3));
        }
        // 2-word greedy.
        if let Some(second) = tokens.get(1).copied() {
            let pair: Self = match (first, second) {
                ("tank", "assist") => Self::TANK_ASSIST,
                ("dps", "assist") => Self::DPS_ASSIST,
                ("pull", "back") => Self::PULL_BACK,
                ("tank", "feral") => Self::TANK_FERAL,
                ("dps", "feral") => Self::DPS_FERAL,
                _ => Self::NONE,
            };
            if !pair.is_empty() {
                return Some((pair, 2));
            }
        }
        // 1-word fallback. Aliases and typo-tolerance live here.
        let single: Self = match first {
            "tank" => Self::TANK,
            "assist" => Self::ASSIST,
            "protect" => Self::PROTECT,
            "pull" => Self::PULL,
            "threat" | "threath" => Self::THREAT, // RaidControl typo
            "passive" => Self::PASSIVE,
            "fury" => Self::FURY,
            "dps" => Self::DPS,
            "close" => Self::CLOSE,
            "aoe" => Self::AOE,
            "grind" => Self::GRIND,
            "boost" | "i" => Self::BOOST, // Mangosbot keybind alias
            "behind" => Self::BEHIND,
            "cc" => Self::CC,
            "range" | "ranged" => Self::RANGED,
            "heal" | "healer" => Self::HEAL,
            "restoration" | "resto" => Self::RESTORATION,
            "stealth" => Self::STEALTH,
            "feral" => Self::FERAL,
            _ => return None,
        };
        Some((single, 1))
    }

    /// Render as a stable string for query responses. Order and punctuation
    /// mirror what the addons expect to see echoed back.
    pub fn describe(self) -> String {
        if self.is_empty() {
            return "none".to_string();
        }
        let mut parts: Vec<&str> = Vec::new();
        let pairs: &[(Self, &str)] = &[
            (Self::TANK, "tank"),
            (Self::ASSIST, "assist"),
            (Self::PROTECT, "protect"),
            (Self::PULL, "pull"),
            (Self::THREAT, "threat"),
            (Self::PASSIVE, "passive"),
            (Self::FURY, "fury"),
            (Self::DPS, "dps"),
            (Self::CLOSE, "close"),
            (Self::AOE, "aoe"),
            (Self::GRIND, "grind"),
            (Self::TANK_ASSIST, "tank assist"),
            (Self::DPS_ASSIST, "dps assist"),
            (Self::PULL_BACK, "pull back"),
            (Self::BOOST, "boost"),
            (Self::BEHIND, "behind"),
            (Self::WAIT_FOR_ATTACK, "wait for attack"),
            (Self::CC, "cc"),
            (Self::RANGED, "ranged"),
            (Self::HEAL, "heal"),
            (Self::RESTORATION, "restoration"),
            (Self::STEALTH, "stealth"),
            (Self::FERAL, "feral"),
            (Self::TANK_FERAL, "tank feral"),
            (Self::DPS_FERAL, "dps feral"),
        ];
        for (flag, name) in pairs {
            if self.contains(*flag) {
                parts.push(name);
            }
        }
        parts.join(", ")
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

/// PB2 has four independent strategy engines per bot, one per `BotState`.
/// Each engine owns its own strategy list, toggled by a separate chat
/// command: `co` → combat, `nc` → non-combat, `react` → reaction, `de` →
/// dead. The `StrategyChatFilter` and every `HasStrategy(name, state)`
/// call key on this enum.
///
/// Reference: PB2 `PlayerbotAI.h` `BotState` enum and
/// `PlayerbotAI::HasStrategy(const string&, BotState)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BotStateKind {
    /// Strategies that run while the bot is fighting. Addon command: `co`.
    Combat = 0,
    /// Strategies that run while the bot is out of combat. Addon command: `nc`.
    NonCombat = 1,
    /// Reactive / always-on strategies layered over the active state engine.
    /// Addon command: `react` (when used with signed `+/-` args).
    Reaction = 2,
    /// Strategies that run while the bot is dead. Addon command: `de`.
    Dead = 3,
}

impl BotStateKind {
    /// The four slots in canonical order, used for per-state iteration.
    pub const ALL: [Self; 4] = [
        Self::Combat,
        Self::NonCombat,
        Self::Reaction,
        Self::Dead,
    ];

    /// Short name used in the addon command vocabulary.
    pub fn addon_command(self) -> &'static str {
        match self {
            Self::Combat => "co",
            Self::NonCombat => "nc",
            Self::Reaction => "react",
            Self::Dead => "de",
        }
    }

    /// Human label used in reply strings. Mangosbot's `OnWhisper` parser
    /// keys on these exact prefixes (`Mangosbot.lua:3358, 3362, 3366` …).
    pub fn reply_prefix(self) -> &'static str {
        match self {
            Self::Combat => "Combat Strategies",
            Self::NonCombat => "Non Combat Strategies",
            Self::Reaction => "Reaction Strategies",
            Self::Dead => "Dead Strategies",
        }
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

    /// Render as a comma-separated string for query responses.
    pub fn describe(self) -> String {
        if self.0 == 0 {
            return "none".to_string();
        }
        let mut parts: Vec<&str> = Vec::new();
        let pairs: &[(Self, &str)] = &[
            (Self::RPG, "rpg"),
            (Self::RPG_BG, "rpg bg"),
            (Self::RPG_EXPLORE, "rpg explore"),
            (Self::RPG_GUILD, "rpg guild"),
            (Self::RPG_MAINTENANCE, "rpg maintenance"),
            (Self::RPG_PLAYER, "rpg player"),
            (Self::RPG_QUEST, "rpg quest"),
            (Self::RPG_VENDOR, "rpg vendor"),
            (Self::RTSC, "rtsc"),
            (Self::WBUFF, "wbuff"),
            (Self::GRIND, "grind"),
            (Self::FLEE, "flee"),
            (Self::EMOTE, "emote"),
            (Self::CC, "cc"),
        ];
        for (flag, name) in pairs {
            if self.contains(*flag) {
                parts.push(name);
            }
        }
        parts.join(", ")
    }
}

/// Four independent `StrategyFlags` slots — one per PB2 `BotState`.
///
/// Each slot is toggled by its own chat command (`co`/`nc`/`react`/`de`)
/// and queried independently. The `StrategyChatFilter` at PB2
/// `ChatFilter.cpp:22–148` resolves `@nc=<name>`, `@co=<name>`,
/// `@react=<name>`, `@dead=<name>` (plus negated `@noco=` etc.) against
/// whichever slot the filter names. See
/// `PlayerbotAI::HasStrategy(name, BotState)`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StrategySet {
    slots: [StrategyFlags; 4],
}

impl StrategySet {
    /// Per-state defaults matching PB2 `AiFactory.cpp` / `PlayerbotAIConfig`:
    ///
    /// - `combatStrategies       = ""`
    /// - `nonCombatStrategies    = "+return,+delayed roll"`
    /// - `reactStrategies        = ""`
    /// - `deadStrategies         = ""`
    ///
    /// Until the `return` and `delayed roll` strategy flags exist in the
    /// Rust port, the non-combat slot ships with the existing Rust
    /// defaults (RTSC + FLEE + RPG + RPG_MAINTENANCE) so current
    /// behavior is preserved. Replace with PB2's exact list as those
    /// strategies are ported (tracked under Part 5 Step 5).
    pub fn pb2_defaults() -> Self {
        let mut s = Self::default();
        s.slots[BotStateKind::NonCombat as usize] = StrategyFlags::defaults();
        s
    }

    pub fn get(&self, kind: BotStateKind) -> StrategyFlags {
        self.slots[kind as usize]
    }

    pub fn get_mut(&mut self, kind: BotStateKind) -> &mut StrategyFlags {
        &mut self.slots[kind as usize]
    }

    pub fn set(&mut self, kind: BotStateKind, flags: StrategyFlags) {
        self.slots[kind as usize] = flags;
    }

    /// `HasStrategy(name, state)` equivalent — does the named state's
    /// engine have this flag set?
    pub fn has(&self, kind: BotStateKind, flag: StrategyFlags) -> bool {
        self.slots[kind as usize].contains(flag)
    }

    /// True if any state slot has `flag` set. Used when the caller does
    /// not care which engine owns the flag (cross-state UI queries,
    /// blanket runtime gates).
    pub fn has_any(&self, flag: StrategyFlags) -> bool {
        self.slots.iter().any(|s| s.contains(flag))
    }

    /// Reset every slot to PB2 defaults (`reset ai` / `reset strats`).
    pub fn reset_to_defaults(&mut self) {
        *self = Self::pb2_defaults();
    }

    /// Reset a single slot back to its PB2 default.
    pub fn reset_slot(&mut self, kind: BotStateKind) {
        let defaults = Self::pb2_defaults();
        self.slots[kind as usize] = defaults.slots[kind as usize];
    }
}

/// Loot-policy bitfield driven by the Mangosbot `ll` command. Each flag is
/// a class of items the bot is allowed to auto-loot; the autoloot module
/// consults the active set on kills and on inventory sorting.
///
/// Addon vocabulary: `ll +equip`, `ll -equip`, `ll ~equip` (toggle),
/// `ll ?` (query).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LootPolicy(pub u32);

impl LootPolicy {
    pub const NONE: Self = Self(0);
    /// Items the bot can equip (armor/weapon upgrades).
    pub const EQUIP: Self = Self(1 << 0);
    /// Quest items and objective drops.
    pub const QUEST: Self = Self(1 << 1);
    /// Tradeskill materials the bot's professions can use.
    pub const SKILL: Self = Self(1 << 2);
    /// Items worth disenchanting (for enchanters).
    pub const DISENCHANT: Self = Self(1 << 3);
    /// Consumable / useful items (potions, scrolls, reagents).
    pub const USE: Self = Self(1 << 4);
    /// Grey / junk items worth selling to vendor.
    pub const VENDOR: Self = Self(1 << 5);
    /// Anything else — trash. Bots typically keep this off.
    pub const TRASH: Self = Self(1 << 6);

    /// Default loot policy: the "useful" categories enabled.
    pub const fn defaults() -> Self {
        Self(Self::EQUIP.0 | Self::QUEST.0 | Self::SKILL.0 | Self::USE.0 | Self::VENDOR.0)
    }

    /// All known loot categories (the set of every bit above).
    pub const fn all_categories() -> Self {
        Self(
            Self::EQUIP.0
                | Self::QUEST.0
                | Self::SKILL.0
                | Self::DISENCHANT.0
                | Self::USE.0
                | Self::VENDOR.0
                | Self::TRASH.0,
        )
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
    pub fn toggle(&mut self, other: Self) {
        self.0 ^= other.0;
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn parse_name(name: &str) -> Option<Self> {
        Some(match name.trim() {
            "equip" => Self::EQUIP,
            "quest" => Self::QUEST,
            "skill" => Self::SKILL,
            "disenchant" | "de" => Self::DISENCHANT,
            "use" => Self::USE,
            "vendor" => Self::VENDOR,
            "trash" => Self::TRASH,
            _ => return None,
        })
    }

    pub fn describe(self) -> String {
        if self.is_empty() {
            return "none".to_string();
        }
        let mut parts: Vec<&str> = Vec::new();
        let pairs: &[(Self, &str)] = &[
            (Self::EQUIP, "equip"),
            (Self::QUEST, "quest"),
            (Self::SKILL, "skill"),
            (Self::DISENCHANT, "disenchant"),
            (Self::USE, "use"),
            (Self::VENDOR, "vendor"),
            (Self::TRASH, "trash"),
        ];
        for (flag, name) in pairs {
            if self.contains(*flag) {
                parts.push(name);
            }
        }
        parts.join(", ")
    }
}

impl std::ops::Sub for LootPolicy {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 & !rhs.0)
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

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Near => "near",
            Self::Line => "line",
            Self::Circle => "circle",
            Self::Chaos => "chaos",
            Self::Box => "box",
            Self::Queue => "queue",
            Self::Arrow => "arrow",
            Self::Wedge => "wedge",
            Self::Pairs => "pairs",
        }
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

impl Reactivity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passive => "passive",
            Self::Defensive => "defensive",
            Self::Aggressive => "aggressive",
        }
    }
}

/// Per-bot runtime settings. Modified by chat commands.
#[derive(Debug, Clone)]
pub struct BotSettings {
    // -- Behavior --
    pub mode: BehaviorMode,
    pub combat_order: CombatOrder,
    pub reactivity: Reactivity,
    /// Per-state strategy engines — PB2 has four independent engines
    /// per bot (combat / non-combat / reaction / dead), each with its
    /// own strategy list toggled by `co` / `nc` / `react` / `de`.
    pub strategies: StrategySet,

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
    /// Loot-policy bitfield driven by the Mangosbot `ll` command.
    pub loot_policy: LootPolicy,
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
    /// Persistent CC raid-target-icon preference set by `rti cc <icon>`.
    /// Mangosbot's per-bot CC mark selector. When set, the reactive/CC
    /// subtree targets the mob bearing this icon with the class CC spell.
    pub preferred_cc_rti_icon: Option<u8>,

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
            strategies: StrategySet::pb2_defaults(),
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
            loot_policy: LootPolicy::defaults(),
            self_res: false,
            cheat_flags: 0,
            keep_items: HashSet::new(),
            chat_channels: 0,
            preferred_rti_icon: None,
            preferred_cc_rti_icon: None,
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
