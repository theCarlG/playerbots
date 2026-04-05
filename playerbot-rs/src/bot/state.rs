/// `BotState` — the per-bot AI state object.
///
/// This is the opaque `void*` returned by `playerbot_create` and stored by
/// the C++ `PlayerbotRust` class. It owns everything the AI needs.
use std::collections::VecDeque;

use crate::{
    bot::class_prefs::ClassPrefs,
    bot::events::BotEvent,
    bot::settings::BotSettings,
    commands::PendingCommand,
    encounters::EncounterFsm,
    engine::{
        blackboard::Blackboard,
        bt::Bt,
        group_registry::{self, GroupHandle},
        throttles::Throttles,
        timers::BotTimers,
    },
    ffi::{BotRole, BotWorldSnapshot, UnitHandle, interface::BotInterface},
};

/// Which `WoW` class this bot is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PlayerClass {
    Warrior = 1,
    Paladin = 2,
    Hunter = 3,
    Rogue = 4,
    Priest = 5,
    DeathKnight = 6,
    Shaman = 7,
    Mage = 8,
    Warlock = 9,
    Druid = 11,
}

/// Which specialization / role this bot plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerSpec {
    // Warrior
    WarriorArms,
    WarriorFury,
    WarriorProtection,
    // Paladin
    PaladinHoly,
    PaladinProtection,
    PaladinRetribution,
    // Priest
    PriestHoly,
    PriestDiscipline,
    PriestShadow,
    // Druid
    DruidBalance,
    DruidFeral,
    DruidRestoration,
    // Hunter
    HunterBeastMastery,
    HunterMarksmanship,
    HunterSurvival,
    // Mage
    MageArcane,
    MageFire,
    MageFrost,
    // Rogue
    RogueAssassination,
    RogueCombat,
    RogueSubtlety,
    // Shaman
    ShamanElemental,
    ShamanEnhancement,
    ShamanRestoration,
    // Warlock
    WarlockAffliction,
    WarlockDemonology,
    WarlockDestruction,
    // Death Knight
    DeathKnightBlood,
    DeathKnightFrost,
    DeathKnightUnholy,
}

/// The complete per-bot AI state.
pub struct BotState {
    /// `CMaNGOS` `ObjectGuid` value for this bot's Player.
    pub handle: u64,

    /// Interface to the game (production: `RealInterface`, tests: `MockInterface`).
    pub interface: Box<dyn BotInterface>,

    /// Current tick's world snapshot. Refreshed at the start of each tick.
    pub snap: BotWorldSnapshot,

    /// Hostile nearby units. Refreshed every 500ms.
    pub attackers: Vec<UnitHandle>,

    /// All nearby units (hostile + friendly). Refreshed every 1000ms.
    pub nearby_units: Vec<UnitHandle>,

    /// Per-spell cooldown tracking and GCD.
    pub timers: BotTimers,

    /// Per-call-site last-fire timestamps for `Bt::Throttle` nodes.
    /// Lives on the bot so the behavior tree itself stays stateless and
    /// shareable (see `engine::throttles`).
    pub throttles: Throttles,

    /// Push events from C++ (spell casts, aura changes, deaths, damage).
    /// Processed before the BT runs each tick.
    pub events: VecDeque<BotEvent>,

    /// Per-bot typed key-value store.
    pub blackboard: Blackboard,

    /// Shared group/encounter assignments. `None` if the bot is solo.
    ///
    /// This is a RAII handle — dropping it (by assigning `None`, or because
    /// the `BotState` itself is dropped) automatically deregisters the bot's
    /// reference from the process-wide group registry. When the last
    /// handle for a group is dropped, the registry entry is removed on the
    /// spot; no lazy housekeeping pass is required.
    pub group_state: Option<GroupHandle>,

    /// Raw `ObjectGuid` of the player that currently commands this bot.
    ///
    /// Mirrors PB2's `PlayerbotAI::m_master`. Set from C++ via
    /// `playerbot_set_master` when `PlayerbotRust::SetMaster` is called or
    /// when the per-tick master auto-claim logic finds a real player in the
    /// bot's group. `None` means "solo / unclaimed" — the bot's default
    /// behaviour mode is treated as `Rpg` rather than `Follow` in that case.
    pub master_guid: Option<u64>,

    /// Active raid/dungeon encounter FSM. None outside of known instances.
    /// Created by `encounters::coordinator::encounter_for_zone` when the bot
    /// enters a known zone; updated each tick before the BT runs.
    pub encounter: Option<Box<dyn EncounterFsm>>,

    /// The root behavior tree. Built once at bot init, never reallocated.
    pub root_tree: Bt,

    /// Bot's class and spec.
    pub class: PlayerClass,
    pub spec: PlayerSpec,
    pub role: BotRole,

    /// Per-bot runtime settings (modified by chat commands).
    pub settings: BotSettings,

    /// Pending commands from chat, processed at tick start.
    pub pending_commands: VecDeque<PendingCommand>,

    // ── Throttle timestamps ──────────────────────────────────────────────
    pub last_attackers_refresh_ms: u64,
    pub last_nearby_refresh_ms: u64,
}

impl BotState {
    pub fn new(
        handle: u64,
        interface: Box<dyn BotInterface>,
        class: PlayerClass,
        spec: PlayerSpec,
        role: BotRole,
        root_tree: Bt,
    ) -> Self {
        let settings = BotSettings {
            class_prefs: ClassPrefs::default_for(class, spec),
            ..BotSettings::default()
        };
        Self {
            handle,
            interface,
            snap: BotWorldSnapshot::default(),
            attackers: Vec::new(),
            nearby_units: Vec::new(),
            timers: BotTimers::new(),
            throttles: Throttles::new(),
            events: VecDeque::new(),
            blackboard: Blackboard::default(),
            group_state: None,
            master_guid: None,
            encounter: None,
            root_tree,
            class,
            spec,
            role,
            settings,
            pending_commands: VecDeque::new(),
            last_attackers_refresh_ms: 0,
            last_nearby_refresh_ms: 0,
        }
    }

    /// Reconcile `self.group_state` with the current snapshot.
    ///
    /// Called once at the top of every tick. Reads the snapshot's
    /// `group_members` array, computes the stable group key
    /// (`group_registry::group_key`), and makes sure we hold a `GroupHandle`
    /// for that key. Dropping the previous handle (via reassignment or via
    /// `None`) is what releases the registry entry — no explicit cleanup.
    pub fn refresh_group_membership(&mut self) {
        let size = self.snap.group_size as usize;
        let cap = self.snap.group_members.len();
        let members = &self.snap.group_members[..size.min(cap)];
        match group_registry::group_key(members) {
            None => {
                // Solo or left the group — drop the handle so the registry
                // entry can be reclaimed by `GroupHandle::drop`.
                self.group_state = None;
            }
            Some(key) => {
                // Keep the existing handle if it already points at this
                // group; otherwise swap. Assignment drops the old handle
                // first, which is exactly how we deregister from the old
                // group before joining the new one.
                let same = self
                    .group_state
                    .as_ref()
                    .is_some_and(|h| h.key() == key);
                if !same {
                    self.group_state = Some(group_registry::acquire(key));
                }
            }
        }
    }

    /// Set (or clear) this bot's master. `None` means solo / unclaimed.
    ///
    /// Called through the FFI from `PlayerbotRust::SetMaster` in the C++
    /// shim. Kept as a typed method (rather than a raw field write in
    /// `lib.rs`) so any future invariant that needs to fire on master
    /// change has one place to live.
    pub fn set_master(&mut self, guid: Option<u64>) {
        self.master_guid = guid;
    }

    /// Clear all cached per-bot strategy state.
    ///
    /// Called by the C++ shim whenever the master changes or the core
    /// decides a full reinit is needed (PB2 parity: `ResetStrategies`).
    /// Living next to the field declarations means adding a new cache
    /// field to `BotState` makes the necessary reset clear at the point
    /// where the field is introduced, instead of hiding it in a scattered
    /// `extern "C"` wrapper.
    pub fn reset_strategies(&mut self) {
        self.pending_commands.clear();
        self.events.clear();
        self.blackboard = Blackboard::default();
        self.throttles = Throttles::new();
        self.timers = BotTimers::new();
        self.encounter = None;
    }
}
