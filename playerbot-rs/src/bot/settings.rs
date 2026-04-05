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
pub struct StrategyFlags(pub u128);

impl StrategyFlags {
    /// Helper: build a single-bit flag at position `n` in the u128 backing
    /// store. `const fn` so every constant below is compile-time.
    const fn bit(n: u32) -> Self {
        Self(1u128 << n)
    }

    pub const NONE: Self = Self(0);

    // ── Bits 0–15: original RPG / RTSC / Grind / Flee / CC set (pre Step 6).
    pub const RPG: Self = Self::bit(0);
    pub const RPG_BG: Self = Self::bit(1);
    pub const RPG_EXPLORE: Self = Self::bit(2);
    pub const RPG_GUILD: Self = Self::bit(3);
    pub const RPG_MAINTENANCE: Self = Self::bit(4);
    pub const RPG_PLAYER: Self = Self::bit(5);
    pub const RPG_QUEST: Self = Self::bit(6);
    pub const RPG_VENDOR: Self = Self::bit(7);
    pub const RTSC: Self = Self::bit(8);
    pub const WBUFF: Self = Self::bit(9);
    pub const GRIND: Self = Self::bit(10);
    pub const FLEE: Self = Self::bit(11);
    pub const EMOTE: Self = Self::bit(12);
    pub const CC: Self = Self::bit(13);
    /// PB2 `ReturnStrategy` — when the bot drifts away from its marked
    /// "return position" (set by `stay` / `guard` or encounter scripts),
    /// walk back. One of PB2's two non-combat defaults from
    /// `PlayerbotAIConfig.cpp` (`nonCombatStrategies = "+return,+delayed roll"`).
    /// The BT leaves that consume this flag land in Part 5 Step 11 —
    /// for now the flag parses/describes so chat filters and `nc ?`
    /// reports are byte-correct.
    pub const RETURN: Self = Self::bit(14);
    /// PB2 `DelayedRollStrategy` — hold Need/Greed/Pass roll decisions
    /// for a short window so master/party policy can override. The
    /// second PB2 non-combat default. Consumer lands in Part 5 Step 16
    /// alongside the loot FSM; the flag is present now so `@nc=delayed roll`
    /// filters and `nc ?` queries match PB2.
    pub const DELAYED_ROLL: Self = Self::bit(15);

    // ── Bits 16–30: PB2 all-bot / generic combat base (§3.1 & §3.3).
    //
    // These were added in Part 5 Step 6 for per-class default parity. Most
    // of them do NOT yet have BT-consumer wiring — they exist so:
    //   1. `@co=mount` / `@co=aoe` / etc. chat filters behave like PB2.
    //   2. `co ?` / `nc ?` query replies list the same names PB2 ships.
    //   3. Future Part 5 Step 11+ consumers can gate on `has(state, flag)`.
    //
    // Overlap with `CombatOrder`: a handful of these (TANK_ASSIST,
    // DPS_ASSIST, PULL, PULL_BACK, CLOSE, AOE, RANGED, BEHIND, BOOST,
    // TANK_FERAL, DPS_FERAL, STEALTH, CC) share their names with
    // `CombatOrder` bits. PB2 stores these as *strategies*; the Rust port
    // historically used `CombatOrder` for `co +<name>` parsing, and still
    // does. The strategy-slot copies populated at bot-create time are for
    // chat-filter/query parity only; rotation gating continues to read
    // `CombatOrder` until the follow-up reconciliation (tracked as the
    // "co → StrategySet" unification in the Step 6 end-notes).
    pub const MOUNT: Self = Self::bit(16);
    pub const AVOID_MOBS: Self = Self::bit(17);
    pub const RACIALS: Self = Self::bit(18);
    pub const DEFAULT: Self = Self::bit(19);
    pub const DUEL: Self = Self::bit(20);
    pub const PVP: Self = Self::bit(21);
    pub const AI_CHAT: Self = Self::bit(22);
    pub const TANK_ASSIST: Self = Self::bit(23);
    pub const DPS_ASSIST: Self = Self::bit(24);
    pub const PULL: Self = Self::bit(25);
    pub const PULL_BACK: Self = Self::bit(26);
    pub const CLOSE: Self = Self::bit(27);
    pub const AOE: Self = Self::bit(28);
    pub const RANGED: Self = Self::bit(29);
    pub const BEHIND: Self = Self::bit(30);
    pub const BUFF: Self = Self::bit(31);
    pub const CURE: Self = Self::bit(32);
    pub const BOOST: Self = Self::bit(33);

    // ── Bits 34–45: class-feature strategies (§3.3 class-feature rows).
    pub const OFFHEAL: Self = Self::bit(34);
    pub const OFFDPS: Self = Self::bit(35);
    pub const POISONS: Self = Self::bit(36);
    pub const STEALTH: Self = Self::bit(37);
    pub const TOTEMS: Self = Self::bit(38);
    pub const AURA: Self = Self::bit(39);
    pub const BLESSING: Self = Self::bit(40);
    pub const ASPECT: Self = Self::bit(41);
    pub const STING: Self = Self::bit(42);
    pub const PET: Self = Self::bit(43);
    pub const CURSE: Self = Self::bit(44);
    pub const DKSQUEST: Self = Self::bit(45);

    // ── Bits 46–47: druid feral hints (also present on `CombatOrder`,
    // see header comment on the `CombatOrder` block for the overlap note).
    pub const TANK_FERAL: Self = Self::bit(46);
    pub const DPS_FERAL: Self = Self::bit(47);

    // ── Bits 48–75: spec-name strategies (§3.2). Names are shared across
    // classes where PB2 reuses the string ("holy" = priest Holy AND
    // paladin Holy; "protection" = warrior AND paladin; "restoration" =
    // druid AND shaman; "frost" = mage AND DK). The consumer (rotation
    // or chat filter) dispatches on the bot's class to interpret the
    // flag — we do NOT duplicate the bit per class.
    pub const ARMS: Self = Self::bit(48);
    pub const FURY: Self = Self::bit(49);
    pub const PROTECTION: Self = Self::bit(50);
    pub const DISCIPLINE: Self = Self::bit(51);
    pub const HOLY: Self = Self::bit(52);
    pub const SHADOW: Self = Self::bit(53);
    pub const ARCANE: Self = Self::bit(54);
    pub const FIRE: Self = Self::bit(55);
    pub const FROST: Self = Self::bit(56);
    pub const AFFLICTION: Self = Self::bit(57);
    pub const DEMONOLOGY: Self = Self::bit(58);
    pub const DESTRUCTION: Self = Self::bit(59);
    pub const RETRIBUTION: Self = Self::bit(60);
    pub const ELEMENTAL: Self = Self::bit(61);
    pub const ENHANCEMENT: Self = Self::bit(62);
    pub const RESTORATION: Self = Self::bit(63);
    pub const BALANCE: Self = Self::bit(64);
    pub const BEAST_MASTERY: Self = Self::bit(65);
    pub const MARKSMANSHIP: Self = Self::bit(66);
    pub const SURVIVAL: Self = Self::bit(67);
    pub const ASSASSINATION: Self = Self::bit(68);
    /// Rogue "combat" spec. Named `ROGUE_COMBAT` in Rust to avoid
    /// collision with the module-level `Combat` `BotStateKind` variant.
    /// The addon name is still `"combat"`; only the Rust identifier
    /// differs.
    pub const ROGUE_COMBAT: Self = Self::bit(69);
    pub const SUBTLETY: Self = Self::bit(70);
    pub const BLOOD: Self = Self::bit(71);
    pub const UNHOLY: Self = Self::bit(72);
    pub const FROST_AOE: Self = Self::bit(73);
    pub const UNHOLY_AOE: Self = Self::bit(74);

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }
    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }

    /// Canonical flag → addon-name table. Single source of truth used by
    /// both `parse_name` (reverse-lookup) and `describe` (forward-lookup)
    /// so the two can never drift out of sync when a new flag is added.
    /// Order here is the order `describe()` emits; match PB2's
    /// `AiFactory`/`aiplayerbot.conf.dist.in` presentation where possible.
    const NAME_TABLE: &'static [(Self, &'static str)] = &[
        // RPG / RTSC / grind family (pre Step 6).
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
        (Self::RETURN, "return"),
        (Self::DELAYED_ROLL, "delayed roll"),
        // All-bot base.
        (Self::MOUNT, "mount"),
        (Self::AVOID_MOBS, "avoid mobs"),
        (Self::RACIALS, "racials"),
        (Self::DEFAULT, "default"),
        (Self::DUEL, "duel"),
        (Self::PVP, "pvp"),
        (Self::AI_CHAT, "ai chat"),
        // Combat-role hints.
        (Self::TANK_ASSIST, "tank assist"),
        (Self::DPS_ASSIST, "dps assist"),
        (Self::PULL, "pull"),
        (Self::PULL_BACK, "pull back"),
        (Self::CLOSE, "close"),
        (Self::AOE, "aoe"),
        (Self::RANGED, "ranged"),
        (Self::BEHIND, "behind"),
        (Self::BUFF, "buff"),
        (Self::CURE, "cure"),
        (Self::BOOST, "boost"),
        // Class-feature strategies.
        (Self::OFFHEAL, "offheal"),
        (Self::OFFDPS, "offdps"),
        (Self::POISONS, "poisons"),
        (Self::STEALTH, "stealth"),
        (Self::TOTEMS, "totems"),
        (Self::AURA, "aura"),
        (Self::BLESSING, "blessing"),
        (Self::ASPECT, "aspect"),
        (Self::STING, "sting"),
        (Self::PET, "pet"),
        (Self::CURSE, "curse"),
        (Self::DKSQUEST, "dksquest"),
        // Druid feral hints (shared with CombatOrder).
        (Self::TANK_FERAL, "tank feral"),
        (Self::DPS_FERAL, "dps feral"),
        // Spec-name strategies.
        (Self::ARMS, "arms"),
        (Self::FURY, "fury"),
        (Self::PROTECTION, "protection"),
        (Self::DISCIPLINE, "discipline"),
        (Self::HOLY, "holy"),
        (Self::SHADOW, "shadow"),
        (Self::ARCANE, "arcane"),
        (Self::FIRE, "fire"),
        (Self::FROST, "frost"),
        (Self::AFFLICTION, "affliction"),
        (Self::DEMONOLOGY, "demonology"),
        (Self::DESTRUCTION, "destruction"),
        (Self::RETRIBUTION, "retribution"),
        (Self::ELEMENTAL, "elemental"),
        (Self::ENHANCEMENT, "enhancement"),
        (Self::RESTORATION, "restoration"),
        (Self::BALANCE, "balance"),
        (Self::BEAST_MASTERY, "beast mastery"),
        (Self::MARKSMANSHIP, "marksmanship"),
        (Self::SURVIVAL, "survival"),
        (Self::ASSASSINATION, "assassination"),
        (Self::ROGUE_COMBAT, "combat"),
        (Self::SUBTLETY, "subtlety"),
        (Self::BLOOD, "blood"),
        (Self::UNHOLY, "unholy"),
        (Self::FROST_AOE, "frost aoe"),
        (Self::UNHOLY_AOE, "unholy aoe"),
    ];

    /// Look up a flag by the name the addon sends. Multi-word names are
    /// joined with a space ("rpg bg", "rpg maintenance", "tank assist").
    pub fn parse_name(name: &str) -> Option<Self> {
        let trimmed = name.trim();
        Self::NAME_TABLE
            .iter()
            .find(|(_, n)| *n == trimmed)
            .map(|(f, _)| *f)
    }

    /// Render as a comma-separated string for query responses.
    pub fn describe(self) -> String {
        if self.0 == 0 {
            return "none".to_string();
        }
        let mut parts: Vec<&str> = Vec::new();
        for (flag, name) in Self::NAME_TABLE {
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
    /// Per-state defaults matching PB2 `PlayerbotAIConfig.cpp` exactly
    /// (the global baseline before `AiFactory::kit` layers per-class
    /// strategies on top):
    ///
    /// - `combatStrategies       = ""`                        → empty
    /// - `nonCombatStrategies    = "+return,+delayed roll"`  → RETURN | DELAYED_ROLL
    /// - `reactStrategies        = ""`                        → empty
    /// - `deadStrategies         = ""`                        → empty
    ///
    /// Per-class layering (warrior+`mount avoid mobs racials default
    /// duel`, priest+`discipline dps assist flee …`, etc., all documented
    /// in PARITY_PLAN §3.2) happens at `AiFactory::kit` time and is NOT
    /// in this baseline. This matches PB2's two-layer composition where
    /// `AiFactory::AddDefaultCombatStrategies` runs after the config
    /// string is parsed.
    ///
    /// Random-bot overrides (`randomBotCombatStrategies = "-threat,+custom::say"`,
    /// `randomBotNonCombatStrategies = "+custom::say"`) are a separate
    /// code path — not applied here because the Rust port doesn't yet
    /// model `RandomPlayerbotMgr`. They will land alongside the random-bot
    /// population module under Part 5 Step 5 follow-ups.
    pub fn pb2_defaults() -> Self {
        let mut s = Self::default();
        s.slots[BotStateKind::NonCombat as usize] =
            StrategyFlags(StrategyFlags::RETURN.0 | StrategyFlags::DELAYED_ROLL.0);
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
        Self::NONE
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
            follow_distance: 1.5,
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

/// Server-global AI tuning constants mirrored from PB2's
/// `PlayerbotAIConfig` (populated from `aiplayerbot*.conf.dist.in`). Every
/// value here is read at runtime by some trigger, value, or action — PB2
/// exposes them as `sPlayerbotAIConfig.xxx`, and individual rotations
/// depend on the exact numbers matching (e.g. `sightDistance = 75` is the
/// sensor horizon, `spellDistance = 25` gates cast-gated kite leaves).
///
/// These are server-wide, not per-bot. The Rust port keeps one instance
/// alongside `BotSettings` for now (constructed via `pb2_defaults()`) and
/// will later be loaded from the same config file. Per-bot overrides
/// (like `follow_distance`) still live on `BotSettings`, which read from
/// this struct at bot-init time.
///
/// All values come straight from PB2 `PlayerbotAIConfig.cpp`
/// defaults — change them together with any PB2 upstream change.
#[derive(Debug, Clone)]
pub struct BotAiConfig {
    // -- Timing (milliseconds) --
    pub global_cool_down_ms: u32,
    pub react_delay_ms: u32,
    pub max_wait_for_move_ms: u32,
    pub passive_delay_ms: u32,
    pub repeat_delay_ms: u32,
    pub error_delay_ms: u32,
    pub rpg_delay_ms: u32,
    pub sit_delay_ms: u32,
    pub return_delay_ms: u32,
    pub loot_delay_ms: u32,
    pub expire_action_time_ms: u32,
    pub dispel_aura_duration_ms: u32,

    // -- Distances (yards) --
    pub sight_distance: f32,
    pub spell_distance: f32,
    pub shoot_distance: f32,
    pub heal_distance: f32,
    pub react_distance: f32,
    pub grind_distance: f32,
    pub aggro_distance: f32,
    pub loot_distance: f32,
    pub group_member_loot_distance: f32,
    pub group_member_loot_distance_active_master: f32,
    pub gathering_distance: f32,
    pub gathering_distance_active_master: f32,
    pub flee_distance: f32,
    pub too_close_distance: f32,
    pub melee_distance: f32,
    pub follow_distance: f32,
    pub raid_follow_distance: f32,
    pub wander_min_distance: f32,
    pub wander_max_distance: f32,
    pub whisper_distance: f32,
    pub contact_distance: f32,
    pub aoe_radius: f32,
    pub rpg_distance: f32,
    pub proximity_distance: f32,
    pub far_distance: f32,
    pub max_free_move_distance: f32,
    pub free_move_delay: f32,

    // -- Health / mana thresholds (% of max, 0..=100) --
    pub critical_health: u8,
    pub low_health: u8,
    pub medium_health: u8,
    pub almost_full_health: u8,
    pub low_mana: u8,
    pub medium_mana: u8,

    // -- Jump mechanics --
    pub jump_no_combat_chance: f32,
    pub jump_melee_in_combat_chance: f32,
    pub jump_random_chance: f32,
    pub jump_in_place_chance: f32,
    pub jump_backward_chance: f32,
    pub jump_height_limit: f32,
    pub jump_v_speed: f32,
    pub jump_h_speed: f32,
    pub jump_in_bg: bool,
    pub jump_with_player: bool,
    pub jump_follow: bool,
    pub jump_chase: bool,

    // -- Formation / movement policy --
    pub default_formation: FollowFormation,
    pub use_wander_as_default_follow_strategy: bool,
}

impl BotAiConfig {
    /// Hard-coded PB2 defaults from `PlayerbotAIConfig.cpp`. These match
    /// `aiplayerbot*.conf.dist.in` — the server can later override by
    /// reading that config, but until the Rust port loads a config file
    /// this is the single source of truth.
    pub const fn pb2_defaults() -> Self {
        Self {
            // Timing.
            global_cool_down_ms: 500,
            react_delay_ms: 100,
            max_wait_for_move_ms: 3000,
            passive_delay_ms: 4000,
            repeat_delay_ms: 5000,
            error_delay_ms: 5000,
            rpg_delay_ms: 3000,
            sit_delay_ms: 30000,
            return_delay_ms: 7000,
            loot_delay_ms: 750,
            expire_action_time_ms: 5000,
            dispel_aura_duration_ms: 2000,

            // Distances.
            sight_distance: 75.0,
            spell_distance: 25.0,
            shoot_distance: 25.0,
            heal_distance: 125.0,
            react_distance: 150.0,
            grind_distance: 75.0,
            aggro_distance: 22.0,
            loot_distance: 25.0,
            group_member_loot_distance: 15.0,
            group_member_loot_distance_active_master: 10.0,
            gathering_distance: 15.0,
            gathering_distance_active_master: 5.0,
            flee_distance: 8.0,
            too_close_distance: 5.0,
            melee_distance: 1.5,
            follow_distance: 1.5,
            raid_follow_distance: 5.0,
            wander_min_distance: 5.0,
            wander_max_distance: 50.0,
            whisper_distance: 6000.0,
            contact_distance: 0.5,
            aoe_radius: 5.0,
            rpg_distance: 80.0,
            proximity_distance: 20.0,
            far_distance: 20.0,
            max_free_move_distance: 150.0,
            free_move_delay: 30.0,

            // Thresholds.
            critical_health: 20,
            low_health: 50,
            medium_health: 70,
            almost_full_health: 90,
            low_mana: 15,
            medium_mana: 40,

            // Jump mechanics.
            jump_no_combat_chance: 0.5,
            jump_melee_in_combat_chance: 0.5,
            jump_random_chance: 0.20,
            jump_in_place_chance: 0.50,
            jump_backward_chance: 0.10,
            jump_height_limit: 60.0,
            jump_v_speed: 7.96,
            jump_h_speed: 7.0,
            jump_in_bg: false,
            jump_with_player: false,
            jump_follow: true,
            jump_chase: true,

            // Formation.
            default_formation: FollowFormation::Near,
            use_wander_as_default_follow_strategy: true,
        }
    }
}

impl Default for BotAiConfig {
    fn default() -> Self {
        Self::pb2_defaults()
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
    fn pb2_default_follow_distance_is_1_5() {
        // PB2 `followDistance = 1.5`; previous Rust default was 3.0,
        // which caused bots to string out too far in raids.
        let s = BotSettings::default();
        assert!((s.follow_distance - 1.5).abs() < f32::EPSILON);
        assert_eq!(s.follow_formation, FollowFormation::Near);
    }

    #[test]
    fn pb2_default_strategies_match_aiconfig_cpp() {
        // PB2 AiFactory defaults: only NonCombat has `+return,+delayed roll`.
        let s = StrategySet::pb2_defaults();
        assert_eq!(s.get(BotStateKind::Combat), StrategyFlags::NONE);
        assert_eq!(
            s.get(BotStateKind::NonCombat),
            StrategyFlags(StrategyFlags::RETURN.0 | StrategyFlags::DELAYED_ROLL.0)
        );
        assert_eq!(s.get(BotStateKind::Reaction), StrategyFlags::NONE);
        assert_eq!(s.get(BotStateKind::Dead), StrategyFlags::NONE);
    }

    #[test]
    fn bot_ai_config_pb2_defaults_match_playerbotaiconfig_cpp() {
        // Spot-check a cross-section of §3.1 values so upstream PB2
        // changes (or accidental local edits) surface loudly.
        let c = BotAiConfig::pb2_defaults();
        // Timing.
        assert_eq!(c.global_cool_down_ms, 500);
        assert_eq!(c.react_delay_ms, 100);
        assert_eq!(c.return_delay_ms, 7000);
        // Distances.
        assert!((c.sight_distance - 75.0).abs() < f32::EPSILON);
        assert!((c.spell_distance - 25.0).abs() < f32::EPSILON);
        assert!((c.flee_distance - 8.0).abs() < f32::EPSILON);
        assert!((c.follow_distance - 1.5).abs() < f32::EPSILON);
        assert!((c.melee_distance - 1.5).abs() < f32::EPSILON);
        // Thresholds.
        assert_eq!(c.critical_health, 20);
        assert_eq!(c.low_health, 50);
        assert_eq!(c.low_mana, 15);
        // Jump mechanics.
        assert!((c.jump_v_speed - 7.96).abs() < f32::EPSILON);
        assert!(c.jump_follow);
        assert!(!c.jump_in_bg);
        // Formation.
        assert_eq!(c.default_formation, FollowFormation::Near);
        assert!(c.use_wander_as_default_follow_strategy);
    }

    #[test]
    fn strategy_flags_fit_in_backing_store() {
        // Every named strategy in NAME_TABLE must be representable in
        // the u128 backing store. If a future edit runs off the end of
        // the `bit(n)` range, this test catches it immediately.
        for (flag, name) in StrategyFlags::NAME_TABLE {
            assert_ne!(
                flag.0, 0,
                "strategy `{name}` has zero bit — off the end of the u128?"
            );
        }
    }

    #[test]
    fn strategy_name_table_is_bijective() {
        // NAME_TABLE is the single source of truth for parse_name /
        // describe. No duplicate flags, no duplicate names.
        let table = StrategyFlags::NAME_TABLE;
        for i in 0..table.len() {
            for j in (i + 1)..table.len() {
                assert_ne!(
                    table[i].0.0, table[j].0.0,
                    "duplicate bit between `{}` and `{}`",
                    table[i].1, table[j].1
                );
                assert_ne!(
                    table[i].1, table[j].1,
                    "duplicate name `{}`",
                    table[i].1
                );
            }
        }
    }

    #[test]
    fn pb2_step6_strategy_names_all_parse() {
        // Every §3.2 per-class / §3.1 all-bot name must round-trip
        // through parse_name → describe. Catches typos in NAME_TABLE
        // that would silently drop chat-filter / query coverage.
        let names = [
            // All-bot.
            "mount", "avoid mobs", "racials", "default", "duel", "pvp", "ai chat", "wbuff",
            // Combat-role hints.
            "tank assist", "dps assist", "pull", "pull back", "close", "aoe", "ranged",
            "behind", "buff", "cure", "boost", "cc", "flee",
            // Class features.
            "offheal", "offdps", "poisons", "stealth", "totems", "aura", "blessing",
            "aspect", "sting", "pet", "curse", "dksquest", "tank feral", "dps feral",
            // Spec names (warrior, priest, mage, warlock, paladin, shaman, druid,
            // hunter, rogue, dk).
            "arms", "fury", "protection", "discipline", "holy", "shadow", "arcane",
            "fire", "frost", "affliction", "demonology", "destruction", "retribution",
            "elemental", "enhancement", "restoration", "balance", "beast mastery",
            "marksmanship", "survival", "assassination", "combat", "subtlety",
            "blood", "unholy", "frost aoe", "unholy aoe",
            // Pre-Step 6 set still resolves.
            "return", "delayed roll", "rpg", "rtsc", "grind", "emote",
        ];
        for n in names {
            let parsed = StrategyFlags::parse_name(n);
            assert!(parsed.is_some(), "parse_name({n:?}) returned None");
            let f = parsed.unwrap();
            assert_eq!(
                f.describe(),
                n,
                "describe round-trip mismatch for `{n}`"
            );
        }
    }

    #[test]
    fn return_and_delayed_roll_parse_and_describe() {
        assert_eq!(
            StrategyFlags::parse_name("return"),
            Some(StrategyFlags::RETURN)
        );
        assert_eq!(
            StrategyFlags::parse_name("delayed roll"),
            Some(StrategyFlags::DELAYED_ROLL)
        );
        let both = StrategyFlags(StrategyFlags::RETURN.0 | StrategyFlags::DELAYED_ROLL.0);
        assert_eq!(both.describe(), "return, delayed roll");
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
