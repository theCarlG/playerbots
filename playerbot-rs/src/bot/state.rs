/// BotState — the per-bot AI state object.
///
/// This is the opaque `void*` returned by `playerbot_create` and stored by
/// the C++ `PlayerbotRust` class. It owns everything the AI needs.
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};

use crate::{
    bot::events::BotEvent,
    engine::{
        blackboard::Blackboard,
        bt_nodes::BtNode,
        group_state::GroupState,
        timers::BotTimers,
    },
    ffi::{interface::BotInterface, BotWorldSnapshot, UnitHandle},
};

/// Which WoW class this bot is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PlayerClass {
    Warrior    = 1,
    Paladin    = 2,
    Hunter     = 3,
    Rogue      = 4,
    Priest     = 5,
    DeathKnight = 6,
    Shaman     = 7,
    Mage       = 8,
    Warlock    = 9,
    Druid      = 11,
}

/// Which specialization / role this bot plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerSpec {
    // Warrior
    WarriorArms, WarriorFury, WarriorProtection,
    // Paladin
    PaladinHoly, PaladinProtection, PaladinRetribution,
    // Priest
    PriestHoly, PriestDiscipline, PriestShadow,
    // Druid
    DruidBalance, DruidFeral, DruidRestoration,
    // Hunter
    HunterBeastMastery, HunterMarksmanship, HunterSurvival,
    // Mage
    MageArcane, MageFire, MageFrost,
    // Rogue
    RogueAssassination, RogueCombat, RogueSubtlety,
    // Shaman
    ShamanElemental, ShamanEnhancement, ShamanRestoration,
    // Warlock
    WarlockAffliction, WarlockDemonology, WarlockDestruction,
    // Death Knight
    DeathKnightBlood, DeathKnightFrost, DeathKnightUnholy,
}

/// Role bitmask (mirrors C-side role field in BotUnitSnapshot).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BotRole(pub u8);

impl BotRole {
    pub const NONE: Self = Self(0);
    pub const TANK: Self = Self(1);
    pub const HEAL: Self = Self(2);
    pub const DPS:  Self = Self(4);
}

/// The complete per-bot AI state.
pub struct BotState {
    /// CMaNGOS ObjectGuid value for this bot's Player.
    pub handle: u64,

    /// Interface to the game (production: RealInterface, tests: MockInterface).
    pub interface: Box<dyn BotInterface>,

    /// Current tick's world snapshot. Refreshed at the start of each tick.
    pub snap: BotWorldSnapshot,

    /// Hostile nearby units. Refreshed every 500ms.
    pub attackers: Vec<UnitHandle>,

    /// All nearby units (hostile + friendly). Refreshed every 1000ms.
    pub nearby_units: Vec<UnitHandle>,

    /// Per-spell cooldown tracking and GCD.
    pub timers: BotTimers,

    /// Push events from C++ (spell casts, aura changes, deaths, damage).
    /// Processed before the BT runs each tick.
    pub events: VecDeque<BotEvent>,

    /// Per-bot typed key-value store.
    pub blackboard: Blackboard,

    /// Shared group/encounter assignments. None if not in a group.
    pub group_state: Option<Arc<RwLock<GroupState>>>,

    /// The root behavior tree. Built once at bot init, never reallocated.
    pub root_tree: Box<dyn BtNode>,

    /// Bot's class and spec.
    pub class: PlayerClass,
    pub spec:  PlayerSpec,
    pub role:  BotRole,

    // ── Throttle timestamps ──────────────────────────────────────────────
    pub last_attackers_refresh_ms:  u64,
    pub last_nearby_refresh_ms:     u64,
}

impl BotState {
    pub fn new(
        handle: u64,
        interface: Box<dyn BotInterface>,
        class: PlayerClass,
        spec: PlayerSpec,
        role: BotRole,
        root_tree: Box<dyn BtNode>,
    ) -> Self {
        Self {
            handle,
            interface,
            snap: BotWorldSnapshot::default(),
            attackers: Vec::new(),
            nearby_units: Vec::new(),
            timers: BotTimers::new(),
            events: VecDeque::new(),
            blackboard: Blackboard::default(),
            group_state: None,
            root_tree,
            class,
            spec,
            role,
            last_attackers_refresh_ms: 0,
            last_nearby_refresh_ms: 0,
        }
    }
}
