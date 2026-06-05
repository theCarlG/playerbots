//! In-memory `World` implementation for tests and offline simulation.
//!
//! `MockWorld` is the single replacement for the legacy `NullInterface`,
//! `TestInterface`, and ~18 per-module `MockIface` impls. It owns a
//! [`MockState`] (snapshots, learned spells, inventories, skills, …) and
//! records every command issued through it as a [`MockEvent`].
//!
//! Construction is via the [`MockWorldBuilder`]:
//!
//! ```ignore
//! use cmangos::mock::MockWorld;
//! let world = MockWorld::builder()
//!     .knows_spell(133)         // fireball
//!     .item_in_bags(6948, 1)    // hearthstone
//!     .build();
//! ```
//!
//! Tests call methods on the `World` trait through `&world` and then
//! inspect `world.events()` / `world.last_event()` to assert what the AI
//! actually did. Mutators (`bot_learn_spell`, `bot_set_skill`, …) update
//! both the underlying `MockState` and the event log so tests can verify
//! either way.

#![cfg(any(test, feature = "mock"))]
#![forbid(unsafe_code)]
#![allow(clippy::missing_const_for_fn, clippy::too_many_lines)]

use core::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::string::{String, ToString};
use std::vec::Vec;

use crate::owned::OwnedList;
use crate::{
    AuraList, BotAuraInfo, BotInventoryItem, BotMailSummary, BotPosition, BotQuestInfo,
    BotReputationEntry, BotRole, BotSkillEntry, BotSpellInfo, BotSpellList, BotTalentEntry,
    BotTaxiNode, BotThreatEntry, BotTravelDest, BotUnitSnapshot, BotWorldSnapshot,
    GatherableList, InventoryList, ItemId, QuestLog, ReputationList, SkillList, SpellId,
    TalentList, TaxiNodeList, ThreatList, TravelDestList, UnitHandle, UnitList, World,
};

/* ── Event log ───────────────────────────────────────────────────────────── */

/// One observable side effect produced by an AI action. Tests assert on
/// these via [`MockWorld::events`] / [`MockWorld::last_event`].
#[derive(Debug, Clone, PartialEq)]
pub enum MockEvent {
    CastSpell { spell: SpellId, target: UnitHandle },
    CastSpellPos { spell: SpellId, x: f32, y: f32, z: f32 },
    MoveTo { x: f32, y: f32, z: f32 },
    Follow { target: UnitHandle, dist: f32, angle: f32 },
    StopMoving,
    Attack(UnitHandle),
    PetAttack(UnitHandle),
    AutoAttack(bool),
    Say { msg: String, lang: u32 },
    Whisper { target: u64, msg: String },
    TellPlayer { target: u64, msg: String },
    TellAddon { target: u64, msg: String },
    UseItem { item: ItemId, target: UnitHandle },
    Taunt(UnitHandle),
    LearnSpell(u32),
    RemoveSpell(u32),
    LearnDefaultSpells,
    LearnClassLevelSpells { include_quest_rewards: bool },
    ResetSpells,
    SetSkill { skill: u32, value: u32, max: u32 },
    ClearSkill(u32),
    UpdateSkillsForLevel,
    SetReputation { faction: u32, value: i32 },
    SetTaxiNode(u32),
    SetAmmo(u32),
    SaveToDbIfNotBusy,
    ResurrectFull,
    CombatStop,
    SetLevelAndResetXp(u32),
    SetPlayerFlag { flag: u32, set: bool },
    RewardQuestComplete(u32),
    InventoryAddItem { item: ItemId, count: u32 },
    StoreInBestSlots { item: ItemId, count: u32 },
    DestroyEquippedAndBags,
    DestroyAll,
    RemoveAllAuras,
    RemoveAura(SpellId),
    UpdateFreeTalentPoints,
    PickSpecNo { incremental: bool },
    ResetAllQuests,
    WriteLogFile { name: String, body: String },
    AppendLogFile { name: String, line: String },
    /// Bot attempted to join `guild_id` at `rank`. Produced by
    /// `factory_guild_add_member`. The mock treats this as an always-success
    /// add; tests that want a failure path can pre-seed `guild_summaries`
    /// to omit the target guild.
    GuildAddMember { guild_id: u32, rank: u32 },
    /// A `factory_kv_set_u32(key, value)` call — used by
    /// `PlayerbotFactory::InitTradeSkills` to cache the two professions
    /// assigned to a bot. Tests can assert on the exact key/value pairs.
    FactoryKvSet { key: String, value: u32 },
    /// A `factory_learn_tradeskill_recipes()` call — the opaque trainer
    /// iteration delegated back to C++. The mock only records that the
    /// callback fired since there is no spell-loop to simulate.
    LearnTradeskillRecipes,
    /// A `factory_destroy_all_equipped_items()` call — the factory
    /// dropping every equipped item before rerolling gear. The mock
    /// clears `equipped_item_in_slot` alongside the recorded event.
    DestroyAllEquippedItems,
    /// A `factory_equip_new_item_in_slot(slot, item, …)` call — InitEquipment
    /// handing the target slot a new item id. Includes any random-enchant id
    /// and whether to apply the socket/enchant pass.
    EquipNewItemInSlot {
        slot: u8,
        item: u32,
        random_enchant: u32,
        apply_enchants: bool,
    },
    /// A `factory_init_stats_for_level_and_update()` call — the factory's
    /// tail `InitStatsForLevel(true); UpdateAllStats()` pair that recomputes
    /// derived stats after equipment changes.
    InitStatsForLevelAndUpdate,
    /// A `factory_tell_master(msg)` call — the factory announcing the
    /// old/new gear-score pair to the synced master.
    TellMaster(String),
    /// A `factory_create_hunter_pet(entry)` call — InitPet picking a
    /// creature from the tameable-creatures list. The mock stamps
    /// `pet_entry` / `pet_family` based on the matching [`PetCreature`]
    /// entry before recording the event.
    CreateHunterPet(u32),
    /// A `factory_pet_refresh_stats()` call — InitPet's tail that
    /// re-runs the pet's stat / level / happiness / REACT_DEFENSIVE
    /// pass after creation.
    PetRefreshStats,
    /// A `factory_pet_learn_spell(spell_id)` call — InitPet or
    /// InitPetSpells teaching a new spell to the pet.
    PetLearnSpell(u32),
    /// A `factory_pet_toggle_autocast(spell_id, enable)` call —
    /// flipping the autocast bit on a pet spell. Both InitPet's mass-
    /// on loop and InitPetSpells' per-spell Cower toggle go through
    /// this path.
    PetToggleAutocast { spell: u32, enable: bool },
    /// A `factory_pet_force_dismiss()` call — InitPet's "fix the
    /// missing flags" `SetDeathState(JUST_DIED)` workaround.
    PetForceDismiss,
    /// A `factory_load_enchant_container()` call — the legacy
    /// `PlayerbotFactory::LoadEnchantContainer` that pulls per-bot enchant
    /// templates out of the world DB. Delegated to C++ because the container
    /// lives on the (deleted) factory object.
    LoadEnchantContainer,
    /// A `bot_reset_talents()` call — `Randomize`'s non-incremental random
    /// bot branch wiping talents before the fresh learn pass.
    ResetTalents,
    /// A `bot_learn_quest_rewarded_spells()` call — `Randomize` folding quest
    /// reward spells into the spellbook after rewarding the special quest
    /// list on a real random bot.
    LearnQuestRewardedSpells,
    /// A `bot_set_money(amount)` call — `Randomize`'s tail stipend. The mock
    /// also stores the new balance in `money` so follow-up `bot_get_money`
    /// queries observe it.
    SetMoney(u32),
    /// A `factory_init_all_gems()` call — the atomic TBC/WotLK gem fan-out
    /// delegated to C++. No-op on Classic bridges.
    InitAllGems,
    /// A `factory_enchant_all_equipment()` call — the atomic per-slot enchant
    /// template dispatch delegated to C++.
    EnchantAllEquipment,
}

/// Descriptor for a tameable creature in [`MockState::tameable_creatures`].
/// Pairs the creature id with the `CreatureInfo::Family` value the mock
/// should stamp onto `pet_family` when the create callback picks this id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PetCreature {
    pub entry: u32,
    pub family: u32,
}

/* ── State ───────────────────────────────────────────────────────────────── */

/// In-memory game state. Every field has a sensible empty default; tests
/// fill in only what they need via [`MockWorldBuilder`] or by mutating the
/// state directly through the helper methods on [`MockWorld`].
pub struct MockState {
    /* snapshots */
    pub world_snap: BotWorldSnapshot,
    pub units: HashMap<UnitHandle, BotUnitSnapshot>,

    /* aura / threat */
    pub auras: HashMap<UnitHandle, Vec<BotAuraInfo>>,
    /// Unit-agnostic aura set: any `has_aura(_, spell)` lookup against an
    /// id in this set returns `true`. Mirrors the legacy `TestInterface`'s
    /// "this aura is on whoever you ask about" semantics.
    pub global_auras: HashSet<u32>,
    pub threat: HashMap<UnitHandle, Vec<BotThreatEntry>>,

    /* nearby */
    pub nearby_hostile: Vec<UnitHandle>,
    pub nearby_friendly: Vec<UnitHandle>,
    pub attackers: Vec<UnitHandle>,
    pub nearby_lootable: Vec<UnitHandle>,
    pub nearby_npcs: Vec<UnitHandle>,
    pub nearby_enemies: Vec<UnitHandle>,
    pub nearby_gossip_npcs: Vec<UnitHandle>,
    pub nearby_gatherables: Vec<u64>,

    /* spells */
    pub knows_spell: HashSet<u32>,
    pub spell_cooldown_ms: HashMap<u32, u32>,
    pub spell_info: HashMap<u32, BotSpellInfo>,
    pub bot_spells: Vec<u32>,
    pub random_bot_spell_ids: Vec<u32>,
    pub can_cast_default: bool,

    /* unit distances */
    pub unit_distance: HashMap<UnitHandle, f32>,
    pub default_unit_distance: f32,
    pub has_los_default: bool,

    /* inventory / items */
    pub bag_items: HashMap<u32, u32>, // item_id -> count
    pub item_max_stack: HashMap<u32, u32>,
    pub item_quality: HashMap<u32, u32>,
    pub item_info: HashMap<u32, (String, u8)>,
    pub equipped_weapon_subclass: HashMap<u8, u32>,
    pub equipped_ranged_subclass: u32,
    pub current_ammo_id: u32,
    pub empty_bag_slots: u32,
    pub inventory_items: Vec<BotInventoryItem>,
    pub equipped_items: Vec<BotInventoryItem>,
    pub bank_items: Vec<BotInventoryItem>,
    pub mail_items: Vec<BotInventoryItem>,

    /* factory refresh */
    pub cheat_mask: u32,

    /* factory prepare / config flags */
    pub disable_random_levels: bool,
    pub random_bot_show_helmet: bool,
    pub random_bot_show_cloak: bool,

    /* factory randomize orchestration */
    /// `factory_is_random_bot` — whether the bot is currently tracked in
    /// the random-bot account list. Default `false`.
    pub is_random_bot: bool,
    /// `factory_has_real_player_master` — whether the bot currently has a
    /// human master assigned. Default `false` (the default "unclaimed"
    /// state).
    pub has_real_player_master: bool,
    /// `factory_is_in_real_guild` — whether the bot is in a guild that
    /// contains at least one real player. Default `false`.
    pub is_in_real_guild: bool,
    /// `factory_config_min_enchanting_bot_level` — cut-off level used by
    /// `Randomize` to gate `LoadEnchantContainer`. Default `0` (always
    /// loads).
    pub min_enchanting_bot_level: u32,
    /// `bot_get_money` backing — mirrored `bot_set_money` writes update
    /// this scalar so follow-up reads observe the new balance.
    pub money: u32,

    /* skills */
    pub skills: HashMap<u32, (u32, u32)>, // skill_id -> (value, max)

    /* talents */
    pub class_talents: HashMap<u8, Vec<BotTalentEntry>>,
    pub free_talent_points: u32,
    pub spec_tab: u32,

    /* taxi */
    pub taxi_nodes: HashMap<u8, Vec<BotTaxiNode>>,

    /* reputation */
    pub reputations: HashMap<u32, BotReputationEntry>,
    pub reputation_rank: HashMap<u32, u8>,

    /* quests */
    pub quest_log: Vec<BotQuestInfo>,
    /// Quest IDs the bot is eligible to turn in via the factory
    /// `quest_is_eligible_for_bot` predicate. Every id *not* present here
    /// makes the predicate return false (matching `PlayerbotFactory::InitQuests`
    /// filtering out class/race/min-level mismatches).
    pub eligible_quests: HashSet<u32>,

    /* arena team */
    /// Owning account id returned by `bot_get_account_id` — the
    /// `WorldSession` account used by `PlayerbotFactory::InitArenaTeam`
    /// to gate random-bot work. Default `0` (the "no session" sentinel).
    pub account_id: u32,

    /* guild */
    /// Bot's current guild id returned by `factory_bot_guild_id`. Default
    /// `0` (the "not in a guild" sentinel).
    pub bot_guild_id: u32,
    /// Guild summaries keyed by guild id, read by
    /// `factory_query_guild_summary`. Unknown ids return `None`.
    pub guild_summaries: HashMap<u32, crate::GuildSummary>,
    /// Rank-name lookup keyed by `(guild_id, rank_id)`, read by
    /// `factory_get_guild_rank_name`. Unknown keys return `None`.
    pub guild_rank_names: HashMap<(u32, u32), String>,

    /* per-bot KV store (backs factory_kv_get_u32 / factory_kv_set_u32) */
    /// In-memory mirror of `sRandomPlayerbotMgr.GetValue(bot, key)` —
    /// the factory persists things like `firstSkill`/`secondSkill` here
    /// so subsequent re-rolls hand out the same professions. Unknown
    /// keys return 0.
    pub kv: HashMap<String, u32>,

    /* factory equipment */
    /// Value returned by `factory_bot_guid_low`. Default `0`. Every
    /// itempool query that identifies the bot by guid-low reads through
    /// this, so tests only have to set it when they care about
    /// per-bot variation (e.g., `has_same_quest_rewards`).
    pub bot_guid_low: u32,
    /// Per-slot current item id read by `factory_bot_equipped_item_in_slot`.
    /// Missing slots return `0` (empty). Writes via
    /// `factory_equip_new_item_in_slot` update this map so back-to-back
    /// factory queries observe the new gear.
    pub equipped_item_in_slot: HashMap<u8, u32>,
    /// Value returned by `factory_master_equip_gear_score`. `None` means
    /// "no master", matching the nullable `GetMaster()` path. Used by the
    /// sync-with-master tail in `InitEquipment`.
    pub master_equip_gear_score: Option<u32>,

    /* factory pet */
    /// `factory_pet_entry` — `0` means the bot has no pet. Writes via
    /// `factory_create_hunter_pet` update this value so subsequent
    /// `factory_bot_has_pet` / `factory_pet_entry` queries observe the
    /// freshly-created pet. Mirrors `bot->GetPet()->GetEntry()`.
    pub pet_entry: u32,
    /// `factory_pet_family` — the `CreatureInfo::Family` bucket for
    /// the current pet. Read per-creature from [`tameable_creatures`]
    /// at create time and cached here.
    pub pet_family: u32,
    /// `factory_pet_level` — the pet's current level. Used by
    /// `InitPetSpells` to gate each spell rank.
    pub pet_level: u32,
    /// `factory_pet_has_spell` — set of spell ids currently in the
    /// pet's spellbook. Writes via `factory_pet_learn_spell` update
    /// this set so test assertions can observe the progression.
    pub pet_spells: HashSet<u32>,
    /// Tracks the autocast bit of each pet spell, so tests can assert
    /// on the order / direction of `factory_pet_toggle_autocast` calls.
    pub pet_autocast: HashMap<u32, bool>,
    /// Answers `factory_pet_autocast_candidate_spells` — non-passive,
    /// non-removed spells from `PetSpellMap` that `InitPet` mass-
    /// toggles autocast on. Kept as a plain `Vec<u32>` so tests can
    /// preserve creation order.
    pub pet_autocast_candidates: Vec<u32>,
    /// Creature entries that count as incomplete quest-kill objectives —
    /// `is_quest_objective_creature` returns true for these.
    pub quest_objective_entries: Vec<u32>,
    /// Result returned by `use_nearby_quest_object`.
    pub use_quest_object_result: bool,
    /// Position returned by `nearest_taxi_node_pos` (None = no taxi network).
    pub nearest_taxi_node: Option<BotPosition>,
    /// Result returned by `take_taxi_toward`.
    pub take_taxi_result: bool,
    /// `(state, dock)` returned by `cross_continent_travel`.
    pub cross_continent: (u8, Option<BotPosition>),
    /// Handle returned by `active_escort_npc` (0 = no active escort).
    pub active_escort_npc: UnitHandle,
    /// Result returned by `bot_broadcast_random`.
    pub bot_broadcast_result: bool,
    /// Result returned by `bot_greet_nearby_player`.
    pub bot_greet_result: bool,
    /// Whether the pet is alive. Gates `factory_pet_force_dismiss`
    /// in the mock (mirrors the `if (pet->IsAlive())` check at the
    /// bottom of `InitPet`). Flipped to `false` when the dismiss is
    /// recorded.
    pub pet_is_alive: bool,
    /// Tameable creature ids returned by
    /// `factory_tameable_creatures_for_bot_level` — already filtered
    /// by the mock's notion of "≤ bot level + tameable". Each
    /// `PetCreature` carries the family the create-callback should
    /// stamp onto `pet_family` when it picks this id.
    pub tameable_creatures: Vec<PetCreature>,

    /* random */
    pub rng_seq: VecDeque<u32>,
    pub rng_default: u32,

    /* factory pickers */
    pub potion_picks: HashMap<(u32, u32), ItemId>, // (level, effect) -> item
    pub food_picks: HashMap<(u32, u32), ItemId>,
    pub trade_picks: HashMap<u32, u32>, // level -> item
    pub ammo_picks: HashMap<(u32, u32), u32>, // (level, subclass) -> item

    /* positioning */
    pub safe_pos: Option<BotPosition>,
    pub random_point_nearby: Option<BotPosition>,
    pub can_reach_default: bool,

    /* groups */
    pub group_tank: Option<UnitHandle>,
    pub group_healer: Option<UnitHandle>,
    pub group_roles: HashMap<UnitHandle, BotRole>,

    /* travel */
    pub travel_dests: Vec<BotTravelDest>,

    /* logs */
    pub log_files: HashMap<String, String>,

    /* recorded events */
    pub events: Vec<MockEvent>,
}

impl Default for MockState {
    fn default() -> Self {
        Self {
            world_snap: BotWorldSnapshot::default(),
            units: HashMap::new(),
            auras: HashMap::new(),
            global_auras: HashSet::new(),
            threat: HashMap::new(),
            nearby_hostile: Vec::new(),
            nearby_friendly: Vec::new(),
            attackers: Vec::new(),
            nearby_lootable: Vec::new(),
            nearby_npcs: Vec::new(),
            nearby_enemies: Vec::new(),
            nearby_gossip_npcs: Vec::new(),
            nearby_gatherables: Vec::new(),
            knows_spell: HashSet::new(),
            spell_cooldown_ms: HashMap::new(),
            spell_info: HashMap::new(),
            bot_spells: Vec::new(),
            random_bot_spell_ids: Vec::new(),
            can_cast_default: true,
            unit_distance: HashMap::new(),
            default_unit_distance: 0.0,
            has_los_default: true,
            bag_items: HashMap::new(),
            item_max_stack: HashMap::new(),
            item_quality: HashMap::new(),
            item_info: HashMap::new(),
            equipped_weapon_subclass: HashMap::new(),
            equipped_ranged_subclass: u32::MAX,
            current_ammo_id: 0,
            empty_bag_slots: 0,
            inventory_items: Vec::new(),
            equipped_items: Vec::new(),
            bank_items: Vec::new(),
            mail_items: Vec::new(),
            cheat_mask: 0,
            disable_random_levels: false,
            random_bot_show_helmet: true,
            random_bot_show_cloak: true,
            is_random_bot: false,
            has_real_player_master: false,
            is_in_real_guild: false,
            min_enchanting_bot_level: 0,
            money: 0,
            skills: HashMap::new(),
            class_talents: HashMap::new(),
            free_talent_points: 0,
            spec_tab: 0,
            taxi_nodes: HashMap::new(),
            reputations: HashMap::new(),
            reputation_rank: HashMap::new(),
            quest_log: Vec::new(),
            eligible_quests: HashSet::new(),
            account_id: 0,
            bot_guild_id: 0,
            guild_summaries: HashMap::new(),
            guild_rank_names: HashMap::new(),
            kv: HashMap::new(),
            bot_guid_low: 0,
            equipped_item_in_slot: HashMap::new(),
            master_equip_gear_score: None,
            pet_entry: 0,
            pet_family: 0,
            pet_level: 0,
            pet_spells: HashSet::new(),
            pet_autocast: HashMap::new(),
            pet_autocast_candidates: Vec::new(),
            quest_objective_entries: Vec::new(),
            use_quest_object_result: false,
            nearest_taxi_node: None,
            take_taxi_result: false,
            cross_continent: (0, None),
            active_escort_npc: 0,
            bot_broadcast_result: false,
            bot_greet_result: false,
            pet_is_alive: false,
            tameable_creatures: Vec::new(),
            rng_seq: VecDeque::new(),
            rng_default: 0,
            potion_picks: HashMap::new(),
            food_picks: HashMap::new(),
            trade_picks: HashMap::new(),
            ammo_picks: HashMap::new(),
            safe_pos: None,
            random_point_nearby: None,
            can_reach_default: true,
            group_tank: None,
            group_healer: None,
            group_roles: HashMap::new(),
            travel_dests: Vec::new(),
            log_files: HashMap::new(),
            events: Vec::new(),
        }
    }
}

/* ── MockWorld ───────────────────────────────────────────────────────────── */

/// In-memory `World` implementation. Wraps a `MockState` in a `RefCell`
/// so trait methods (`&self`) can mutate state and append events.
pub struct MockWorld(RefCell<MockState>);

impl Default for MockWorld {
    fn default() -> Self {
        Self(RefCell::new(MockState::default()))
    }
}

impl MockWorld {
    /// Start building a `MockWorld` with the default empty state.
    #[must_use]
    pub fn builder() -> MockWorldBuilder {
        MockWorldBuilder::default()
    }

    /// Convenience constructor: a `MockWorld` with the default empty state.
    /// Mirrors the legacy `TestInterface::new()` / `NullInterface` API used
    /// throughout the encounter and BT tests.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /* ── Legacy `TestInterface` chainable convenience methods ────────── */

    /// Mark `spell_id` as a unit-agnostic aura — `has_aura(_, spell_id)`
    /// returns `true` for any unit. Matches the legacy
    /// `TestInterface::with_aura` semantics.
    #[must_use]
    pub fn with_aura(self, spell_id: SpellId) -> Self {
        self.0.borrow_mut().global_auras.insert(spell_id.0);
        self
    }

    /// Set a non-empty safe position so `get_safe_position` returns `Some`.
    /// The exact coordinates match the legacy `TestInterface` defaults.
    #[must_use]
    pub fn with_safe_pos(self) -> Self {
        self.0.borrow_mut().safe_pos = Some(BotPosition {
            x: 1.0,
            y: 2.0,
            z: 3.0,
            o: 0.0,
            map_id: 0,
        });
        self
    }

    /// Set the default `unit_distance` returned for any unit not explicitly
    /// configured.
    #[must_use]
    pub fn with_unit_dist(self, dist: f32) -> Self {
        self.0.borrow_mut().default_unit_distance = dist;
        self
    }

    /// Configure `get_random_point_nearby` to return the given coordinates.
    #[must_use]
    pub fn with_wander_point(self, x: f32, y: f32, z: f32) -> Self {
        self.0.borrow_mut().random_point_nearby = Some(BotPosition {
            x,
            y,
            z,
            o: 0.0,
            map_id: 0,
        });
        self
    }

    /// Configure the taxi mock: `node` is the nearest flight master position
    /// (`None` = no taxi network), `can_fly` is the `take_taxi_toward` result.
    #[must_use]
    pub fn with_taxi(self, node: Option<BotPosition>, can_fly: bool) -> Self {
        {
            let mut s = self.0.borrow_mut();
            s.nearest_taxi_node = node;
            s.take_taxi_result = can_fly;
        }
        self
    }

    /// Configure the `cross_continent_travel` mock result `(state, dock)`.
    #[must_use]
    pub fn with_cross_continent(self, state: u8, dock: Option<BotPosition>) -> Self {
        self.0.borrow_mut().cross_continent = (state, dock);
        self
    }

    /// Configure the handle returned by `active_escort_npc` (0 = no escort).
    #[must_use]
    pub fn with_escort_npc(self, handle: UnitHandle) -> Self {
        self.0.borrow_mut().active_escort_npc = handle;
        self
    }

    /// Configure the `bot_broadcast_random` result.
    #[must_use]
    pub fn with_broadcast(self, broadcast: bool) -> Self {
        self.0.borrow_mut().bot_broadcast_result = broadcast;
        self
    }

    /// Configure the `bot_greet_nearby_player` result.
    #[must_use]
    pub fn with_greet(self, greet: bool) -> Self {
        self.0.borrow_mut().bot_greet_result = greet;
        self
    }

    /// Borrow the underlying `MockState` mutably for ad-hoc setup or
    /// inspection from a test body.
    pub fn with_state<R>(&self, f: impl FnOnce(&mut MockState) -> R) -> R {
        f(&mut self.0.borrow_mut())
    }

    /// Snapshot of every event recorded so far (oldest first).
    pub fn events(&self) -> Vec<MockEvent> {
        self.0.borrow().events.clone()
    }

    /// The most recent recorded event, if any.
    pub fn last_event(&self) -> Option<MockEvent> {
        self.0.borrow().events.last().cloned()
    }

    /// Clear the event log without touching any other state.
    pub fn clear_events(&self) {
        self.0.borrow_mut().events.clear();
    }

    /// Advance simulated game time by `ms`. Decays per-spell cooldowns.
    pub fn tick(&self, ms: u32) {
        let mut s = self.0.borrow_mut();
        s.world_snap.server_time_ms = s.world_snap.server_time_ms.saturating_add(u64::from(ms));
        for cd in s.spell_cooldown_ms.values_mut() {
            *cd = cd.saturating_sub(ms);
        }
    }

    /// Push an aura onto a unit's aura list.
    pub fn inject_aura(&self, target: UnitHandle, aura: BotAuraInfo) {
        self.0.borrow_mut().auras.entry(target).or_default().push(aura);
    }

    /// Set the bot's current target on the world snapshot.
    pub fn set_self_target(&self, target: UnitHandle) {
        self.0.borrow_mut().world_snap.self_.current_target = target;
    }
}

fn boxed<'a, T>(v: Vec<T>) -> OwnedList<'a, T> {
    OwnedList::from_boxed_slice(v.into_boxed_slice())
}

fn record(state: &RefCell<MockState>, event: MockEvent) {
    state.borrow_mut().events.push(event);
}

/* ── Builder ─────────────────────────────────────────────────────────────── */

/// Fluent builder for [`MockWorld`]. Every setter mutates an in-progress
/// `MockState` and returns `self` so calls can be chained.
#[derive(Default)]
pub struct MockWorldBuilder {
    state: MockState,
}

impl MockWorldBuilder {
    #[must_use]
    pub fn build(self) -> MockWorld {
        MockWorld(RefCell::new(self.state))
    }

    /* snapshots */
    #[must_use]
    pub fn world_snap(mut self, snap: BotWorldSnapshot) -> Self {
        self.state.world_snap = snap;
        self
    }
    #[must_use]
    pub fn self_snap(mut self, snap: BotUnitSnapshot) -> Self {
        self.state.world_snap.self_ = snap;
        self
    }
    #[must_use]
    pub fn unit(mut self, h: UnitHandle, snap: BotUnitSnapshot) -> Self {
        self.state.units.insert(h, snap);
        self
    }
    #[must_use]
    pub fn unit_distance(mut self, h: UnitHandle, dist: f32) -> Self {
        self.state.unit_distance.insert(h, dist);
        self
    }
    #[must_use]
    pub fn default_unit_distance(mut self, dist: f32) -> Self {
        self.state.default_unit_distance = dist;
        self
    }

    /* nearby */
    #[must_use]
    pub fn nearby_hostile(mut self, units: Vec<UnitHandle>) -> Self {
        self.state.nearby_hostile = units;
        self
    }
    #[must_use]
    pub fn nearby_friendly(mut self, units: Vec<UnitHandle>) -> Self {
        self.state.nearby_friendly = units;
        self
    }
    #[must_use]
    pub fn attackers(mut self, units: Vec<UnitHandle>) -> Self {
        self.state.attackers = units;
        self
    }
    #[must_use]
    pub fn nearby_lootable(mut self, units: Vec<UnitHandle>) -> Self {
        self.state.nearby_lootable = units;
        self
    }
    #[must_use]
    pub fn nearby_npcs(mut self, units: Vec<UnitHandle>) -> Self {
        self.state.nearby_npcs = units;
        self
    }
    #[must_use]
    pub fn nearby_enemies(mut self, units: Vec<UnitHandle>) -> Self {
        self.state.nearby_enemies = units;
        self
    }
    #[must_use]
    pub fn nearby_gossip_npcs(mut self, units: Vec<UnitHandle>) -> Self {
        self.state.nearby_gossip_npcs = units;
        self
    }
    #[must_use]
    pub fn nearby_gatherables(mut self, gos: Vec<u64>) -> Self {
        self.state.nearby_gatherables = gos;
        self
    }

    /* auras / threat */
    #[must_use]
    pub fn global_aura(mut self, spell_id: SpellId) -> Self {
        self.state.global_auras.insert(spell_id.0);
        self
    }
    #[must_use]
    pub fn aura(mut self, unit: UnitHandle, aura: BotAuraInfo) -> Self {
        self.state.auras.entry(unit).or_default().push(aura);
        self
    }
    #[must_use]
    pub fn auras(mut self, unit: UnitHandle, auras: Vec<BotAuraInfo>) -> Self {
        self.state.auras.insert(unit, auras);
        self
    }
    #[must_use]
    pub fn threat(mut self, target: UnitHandle, list: Vec<BotThreatEntry>) -> Self {
        self.state.threat.insert(target, list);
        self
    }

    /* spells */
    #[must_use]
    pub fn knows_spell(mut self, spell_id: u32) -> Self {
        self.state.knows_spell.insert(spell_id);
        self
    }
    #[must_use]
    pub fn spell_cooldown(mut self, spell_id: u32, ms: u32) -> Self {
        self.state.spell_cooldown_ms.insert(spell_id, ms);
        self
    }
    #[must_use]
    pub fn spell_info(mut self, spell_id: u32, info: BotSpellInfo) -> Self {
        self.state.spell_info.insert(spell_id, info);
        self
    }
    #[must_use]
    pub fn bot_spells(mut self, spells: Vec<u32>) -> Self {
        self.state.bot_spells = spells;
        self
    }
    #[must_use]
    pub fn random_bot_spell_ids(mut self, ids: Vec<u32>) -> Self {
        self.state.random_bot_spell_ids = ids;
        self
    }
    #[must_use]
    pub fn can_cast_default(mut self, ok: bool) -> Self {
        self.state.can_cast_default = ok;
        self
    }

    /* inventory / items */
    #[must_use]
    pub fn item_in_bags(mut self, item_id: u32, count: u32) -> Self {
        self.state.bag_items.insert(item_id, count);
        self
    }
    #[must_use]
    pub fn item_max_stack(mut self, item_id: u32, stack: u32) -> Self {
        self.state.item_max_stack.insert(item_id, stack);
        self
    }
    #[must_use]
    pub fn item_quality(mut self, item_id: u32, quality: u32) -> Self {
        self.state.item_quality.insert(item_id, quality);
        self
    }
    #[must_use]
    pub fn item_info(mut self, item_id: u32, name: impl Into<String>, quality: u8) -> Self {
        self.state.item_info.insert(item_id, (name.into(), quality));
        self
    }
    #[must_use]
    pub fn equipped_weapon_subclass(mut self, slot: u8, subclass: u32) -> Self {
        self.state.equipped_weapon_subclass.insert(slot, subclass);
        self
    }
    #[must_use]
    pub fn equipped_ranged_subclass(mut self, subclass: u32) -> Self {
        self.state.equipped_ranged_subclass = subclass;
        self
    }
    #[must_use]
    pub fn current_ammo_id(mut self, ammo: u32) -> Self {
        self.state.current_ammo_id = ammo;
        self
    }
    #[must_use]
    pub fn empty_bag_slots(mut self, slots: u32) -> Self {
        self.state.empty_bag_slots = slots;
        self
    }
    #[must_use]
    pub fn inventory_items(mut self, items: Vec<BotInventoryItem>) -> Self {
        self.state.inventory_items = items;
        self
    }
    #[must_use]
    pub fn equipped_items(mut self, items: Vec<BotInventoryItem>) -> Self {
        self.state.equipped_items = items;
        self
    }
    #[must_use]
    pub fn bank_items(mut self, items: Vec<BotInventoryItem>) -> Self {
        self.state.bank_items = items;
        self
    }
    #[must_use]
    pub fn mail_items(mut self, items: Vec<BotInventoryItem>) -> Self {
        self.state.mail_items = items;
        self
    }

    /* factory refresh */
    #[must_use]
    pub fn cheat_mask(mut self, mask: u32) -> Self {
        self.state.cheat_mask = mask;
        self
    }

    /* factory prepare / config flags */
    #[must_use]
    pub fn disable_random_levels(mut self, disabled: bool) -> Self {
        self.state.disable_random_levels = disabled;
        self
    }
    #[must_use]
    pub fn random_bot_show_helmet(mut self, show: bool) -> Self {
        self.state.random_bot_show_helmet = show;
        self
    }
    #[must_use]
    pub fn random_bot_show_cloak(mut self, show: bool) -> Self {
        self.state.random_bot_show_cloak = show;
        self
    }

    /* factory randomize orchestration */
    #[must_use]
    pub fn is_random_bot(mut self, v: bool) -> Self {
        self.state.is_random_bot = v;
        self
    }
    #[must_use]
    pub fn has_real_player_master(mut self, v: bool) -> Self {
        self.state.has_real_player_master = v;
        self
    }
    #[must_use]
    pub fn is_in_real_guild(mut self, v: bool) -> Self {
        self.state.is_in_real_guild = v;
        self
    }
    #[must_use]
    pub fn min_enchanting_bot_level(mut self, level: u32) -> Self {
        self.state.min_enchanting_bot_level = level;
        self
    }
    #[must_use]
    pub fn money(mut self, amount: u32) -> Self {
        self.state.money = amount;
        self
    }

    /* skills */
    #[must_use]
    pub fn skill(mut self, skill_id: u32, value: u32, max: u32) -> Self {
        self.state.skills.insert(skill_id, (value, max));
        self
    }
    #[must_use]
    pub fn skills(mut self, skills: &[u32]) -> Self {
        for &id in skills {
            self.state.skills.entry(id).or_insert((1, 1));
        }
        self
    }

    /* talents */
    #[must_use]
    pub fn class_talents(mut self, spec_no: u8, talents: Vec<BotTalentEntry>) -> Self {
        self.state.class_talents.insert(spec_no, talents);
        self
    }
    #[must_use]
    pub fn free_talent_points(mut self, n: u32) -> Self {
        self.state.free_talent_points = n;
        self
    }
    #[must_use]
    pub fn spec_tab(mut self, tab: u32) -> Self {
        self.state.spec_tab = tab;
        self
    }

    /* taxi */
    #[must_use]
    pub fn taxi_nodes(mut self, team: u8, nodes: Vec<BotTaxiNode>) -> Self {
        self.state.taxi_nodes.insert(team, nodes);
        self
    }

    /* reputation */
    #[must_use]
    pub fn reputation(mut self, faction_id: u32, value: i32, standing: u8) -> Self {
        self.state
            .reputations
            .insert(faction_id, BotReputationEntry { faction_id, value, standing });
        self.state.reputation_rank.insert(faction_id, standing);
        self
    }

    /* quests */
    #[must_use]
    pub fn quest(mut self, quest_id: u32, complete: bool) -> Self {
        self.state.quest_log.push(BotQuestInfo { quest_id, complete });
        self
    }
    /// Mark `quest_id` as eligible for the bot — the factory
    /// `quest_is_eligible_for_bot` predicate returns true for anything in this
    /// set.
    #[must_use]
    pub fn eligible_quest(mut self, quest_id: u32) -> Self {
        self.state.eligible_quests.insert(quest_id);
        self
    }

    /// Set the owning account id returned by `bot_get_account_id`.
    #[must_use]
    pub fn account_id(mut self, id: u32) -> Self {
        self.state.account_id = id;
        self
    }

    /// Set the bot's current guild id returned by `factory_bot_guild_id`.
    /// `0` = "not in a guild" (the default).
    #[must_use]
    pub fn bot_guild_id(mut self, id: u32) -> Self {
        self.state.bot_guild_id = id;
        self
    }

    /// Register a guild summary returned by `factory_query_guild_summary`
    /// for `guild_id`.
    #[must_use]
    pub fn guild_summary(mut self, guild_id: u32, summary: crate::GuildSummary) -> Self {
        self.state.guild_summaries.insert(guild_id, summary);
        self
    }

    /// Register a rank-name lookup for `(guild_id, rank_id)` returned by
    /// `factory_get_guild_rank_name`.
    #[must_use]
    pub fn guild_rank_name(
        mut self,
        guild_id: u32,
        rank_id: u32,
        name: impl Into<String>,
    ) -> Self {
        self.state
            .guild_rank_names
            .insert((guild_id, rank_id), name.into());
        self
    }

    /// Pre-seed the per-bot KV store read by `factory_kv_get_u32`. Used
    /// by trade-skill tests to stash `firstSkill`/`secondSkill` and
    /// verify the cache-hit branch.
    #[must_use]
    pub fn kv(mut self, key: impl Into<String>, value: u32) -> Self {
        self.state.kv.insert(key.into(), value);
        self
    }

    /* factory equipment */

    /// Set the value returned by `factory_bot_guid_low`. Tests that need
    /// per-bot itempool queries to disambiguate (`has_same_quest_rewards`)
    /// should set this to a non-zero id.
    #[must_use]
    pub fn bot_guid_low(mut self, guid_low: u32) -> Self {
        self.state.bot_guid_low = guid_low;
        self
    }
    /// Pre-seed the equipped item id for a single slot. Returned by
    /// `factory_bot_equipped_item_in_slot`. Tests for the incremental
    /// `InitEquipment` branch use this to place the "old" item the
    /// factory must beat.
    #[must_use]
    pub fn equipped_item_in_slot(mut self, slot: u8, item_id: u32) -> Self {
        self.state.equipped_item_in_slot.insert(slot, item_id);
        self
    }
    /// Pre-seed the value returned by `factory_master_equip_gear_score`.
    /// `Some(gs)` makes the sync-with-master tail log; `None` matches the
    /// "no master" default.
    #[must_use]
    pub fn master_equip_gear_score(mut self, gs: u32) -> Self {
        self.state.master_equip_gear_score = Some(gs);
        self
    }

    /* factory pet */

    /// Pre-seed the bot with an existing pet. Matches the state the
    /// mock would be in after a successful `factory_create_hunter_pet`
    /// call. `level` is the pet's level; pass `0` to reuse the bot's
    /// current level from `self_snap`.
    #[must_use]
    pub fn pet(mut self, entry: u32, family: u32, level: u32) -> Self {
        self.state.pet_entry = entry;
        self.state.pet_family = family;
        self.state.pet_level = if level == 0 {
            u32::from(self.state.world_snap.self_.level)
        } else {
            level
        };
        self.state.pet_is_alive = true;
        self
    }

    /// Pre-seed the pet's autocast-candidate spell list returned by
    /// `factory_pet_autocast_candidate_spells`. Used by tests of
    /// `InitPet`'s mass-toggle pass.
    #[must_use]
    pub fn pet_autocast_candidates(mut self, candidates: Vec<u32>) -> Self {
        self.state.pet_autocast_candidates = candidates;
        self
    }

    /// Pre-seed the tameable-creatures list returned by
    /// `factory_tameable_creatures_for_bot_level`.
    #[must_use]
    pub fn tameable_creatures(mut self, creatures: Vec<PetCreature>) -> Self {
        self.state.tameable_creatures = creatures;
        self
    }

    /* random */
    #[must_use]
    pub fn random_seq(mut self, seq: Vec<u32>) -> Self {
        self.state.rng_seq = seq.into();
        self
    }
    #[must_use]
    pub fn random_default(mut self, n: u32) -> Self {
        self.state.rng_default = n;
        self
    }

    /* factory pickers */
    #[must_use]
    pub fn potion_pick(mut self, level: u32, effect: u32, item: ItemId) -> Self {
        self.state.potion_picks.insert((level, effect), item);
        self
    }
    #[must_use]
    pub fn food_pick(mut self, level: u32, category: u32, item: ItemId) -> Self {
        self.state.food_picks.insert((level, category), item);
        self
    }
    #[must_use]
    pub fn trade_pick(mut self, level: u32, item: u32) -> Self {
        self.state.trade_picks.insert(level, item);
        self
    }
    #[must_use]
    pub fn ammo_pick(mut self, level: u32, subclass: u32, item: u32) -> Self {
        self.state.ammo_picks.insert((level, subclass), item);
        self
    }

    /* positioning */
    #[must_use]
    pub fn safe_pos(mut self, pos: BotPosition) -> Self {
        self.state.safe_pos = Some(pos);
        self
    }
    #[must_use]
    pub fn wander_point(mut self, x: f32, y: f32, z: f32) -> Self {
        self.state.random_point_nearby = Some(BotPosition {
            x,
            y,
            z,
            o: 0.0,
            map_id: 0,
        });
        self
    }
    #[must_use]
    pub fn can_reach_default(mut self, ok: bool) -> Self {
        self.state.can_reach_default = ok;
        self
    }

    /* groups */
    #[must_use]
    pub fn group_tank(mut self, h: UnitHandle) -> Self {
        self.state.group_tank = Some(h);
        self
    }
    #[must_use]
    pub fn group_healer(mut self, h: UnitHandle) -> Self {
        self.state.group_healer = Some(h);
        self
    }
    #[must_use]
    pub fn group_role(mut self, h: UnitHandle, role: BotRole) -> Self {
        self.state.group_roles.insert(h, role);
        self
    }

    /* travel */
    #[must_use]
    pub fn travel_dests(mut self, dests: Vec<BotTravelDest>) -> Self {
        self.state.travel_dests = dests;
        self
    }
}

/* ── World impl ──────────────────────────────────────────────────────────── */

impl World for MockWorld {
    /* ── Snapshots ───────────────────────────────────────────────────── */

    fn get_snapshot(&self) -> BotWorldSnapshot {
        self.0.borrow().world_snap
    }

    fn get_unit_snapshot(&self, target: UnitHandle) -> BotUnitSnapshot {
        self.0
            .borrow()
            .units
            .get(&target)
            .copied()
            .unwrap_or_default()
    }

    /* ── Auras ───────────────────────────────────────────────────────── */

    fn has_aura(&self, unit: UnitHandle, spell_id: SpellId) -> bool {
        let s = self.0.borrow();
        if s.global_auras.contains(&spell_id.0) {
            return true;
        }
        s.auras
            .get(&unit)
            .is_some_and(|v| v.iter().any(|a| a.spell_id == spell_id.0))
    }

    fn get_aura(&self, unit: UnitHandle, spell_id: SpellId) -> Option<BotAuraInfo> {
        self.0
            .borrow()
            .auras
            .get(&unit)
            .and_then(|v| v.iter().find(|a| a.spell_id == spell_id.0).copied())
    }

    fn get_auras(&self, unit: UnitHandle) -> AuraList<'_> {
        let v = self
            .0
            .borrow()
            .auras
            .get(&unit)
            .cloned()
            .unwrap_or_default();
        boxed(v)
    }

    /* ── Threat ──────────────────────────────────────────────────────── */

    fn get_threat_list(&self, target_unit: UnitHandle) -> ThreatList<'_> {
        let v = self
            .0
            .borrow()
            .threat
            .get(&target_unit)
            .cloned()
            .unwrap_or_default();
        boxed(v)
    }

    fn get_unit_threat(&self, target_unit: UnitHandle, from_unit: UnitHandle) -> f32 {
        self.0
            .borrow()
            .threat
            .get(&target_unit)
            .and_then(|v| v.iter().find(|t| t.unit == from_unit).map(|t| t.threat))
            .unwrap_or(0.0)
    }

    /* ── Units ───────────────────────────────────────────────────────── */

    fn unit_distance(&self, target: UnitHandle) -> f32 {
        let s = self.0.borrow();
        s.unit_distance
            .get(&target)
            .copied()
            .unwrap_or(s.default_unit_distance)
    }

    fn can_cast(&self, spell_id: SpellId, _target: UnitHandle) -> bool {
        let s = self.0.borrow();
        if s.spell_cooldown_ms.get(&spell_id.0).copied().unwrap_or(0) > 0 {
            return false;
        }
        s.can_cast_default
    }

    fn spell_cooldown_ms(&self, spell_id: SpellId) -> u32 {
        self.0
            .borrow()
            .spell_cooldown_ms
            .get(&spell_id.0)
            .copied()
            .unwrap_or(0)
    }

    fn has_los(&self, _target: UnitHandle) -> bool {
        self.0.borrow().has_los_default
    }

    fn get_nearby_units(&self, _range: f32, hostile: bool) -> UnitList<'_> {
        let s = self.0.borrow();
        let v = if hostile {
            s.nearby_hostile.clone()
        } else {
            s.nearby_friendly.clone()
        };
        boxed(v)
    }

    fn get_attackers(&self) -> UnitList<'_> {
        boxed(self.0.borrow().attackers.clone())
    }

    fn bot_equipped_weapon_subclass(&self, slot: u8) -> u32 {
        self.0
            .borrow()
            .equipped_weapon_subclass
            .get(&slot)
            .copied()
            .unwrap_or(u32::MAX)
    }

    fn bot_item_count(&self, item_id: ItemId) -> u32 {
        self.0
            .borrow()
            .bag_items
            .get(&item_id.0)
            .copied()
            .unwrap_or(0)
    }

    fn knows_spell(&self, spell_id: SpellId) -> bool {
        self.0.borrow().knows_spell.contains(&spell_id.0)
    }

    /* ── Pathfinding / positioning ───────────────────────────────────── */

    fn get_behind_position(&self, _target: UnitHandle, _distance: f32) -> BotPosition {
        BotPosition::default()
    }

    fn get_safe_position(&self, _search_radius: f32) -> Option<BotPosition> {
        self.0.borrow().safe_pos
    }

    fn get_spread_position(
        &self,
        _center: UnitHandle,
        _radius: f32,
        _idx: u8,
        _total: u8,
    ) -> BotPosition {
        BotPosition::default()
    }

    fn can_reach(&self, _x: f32, _y: f32, _z: f32) -> bool {
        self.0.borrow().can_reach_default
    }

    /* ── Commands ────────────────────────────────────────────────────── */

    fn cast_spell(&self, spell_id: SpellId, target: UnitHandle) -> bool {
        record(&self.0, MockEvent::CastSpell { spell: spell_id, target });
        true
    }

    fn cast_spell_pos(&self, spell_id: SpellId, x: f32, y: f32, z: f32) -> bool {
        record(
            &self.0,
            MockEvent::CastSpellPos { spell: spell_id, x, y, z },
        );
        true
    }

    fn move_to(&self, x: f32, y: f32, z: f32) -> bool {
        record(&self.0, MockEvent::MoveTo { x, y, z });
        true
    }

    fn follow(&self, target: UnitHandle, dist: f32, angle: f32) -> bool {
        record(&self.0, MockEvent::Follow { target, dist, angle });
        true
    }

    fn stop_moving(&self) -> bool {
        record(&self.0, MockEvent::StopMoving);
        true
    }

    fn attack(&self, target: UnitHandle) -> bool {
        record(&self.0, MockEvent::Attack(target));
        true
    }

    fn pet_attack(&self, target: UnitHandle) -> bool {
        record(&self.0, MockEvent::PetAttack(target));
        true
    }

    fn is_quest_objective_creature(&self, entry: u32) -> bool {
        self.0.borrow().quest_objective_entries.contains(&entry)
    }

    fn use_nearby_quest_object(&self, _range: f32) -> bool {
        self.0.borrow().use_quest_object_result
    }

    fn nearest_taxi_node_pos(&self) -> Option<BotPosition> {
        self.0.borrow().nearest_taxi_node
    }

    fn take_taxi_toward(&self, _dest_map: u32, _x: f32, _y: f32, _z: f32) -> bool {
        self.0.borrow().take_taxi_result
    }

    fn cross_continent_travel(&self, _dest_map: u32) -> (u8, Option<BotPosition>) {
        self.0.borrow().cross_continent
    }

    fn active_escort_npc(&self) -> UnitHandle {
        self.0.borrow().active_escort_npc
    }

    fn bot_broadcast_random(&self) -> bool {
        self.0.borrow().bot_broadcast_result
    }

    fn bot_greet_nearby_player(&self) -> bool {
        self.0.borrow().bot_greet_result
    }

    fn auto_attack(&self, enable: bool) -> bool {
        record(&self.0, MockEvent::AutoAttack(enable));
        true
    }

    fn say(&self, msg: &str, lang: u32) -> bool {
        record(
            &self.0,
            MockEvent::Say {
                msg: msg.to_string(),
                lang,
            },
        );
        true
    }

    fn whisper(&self, target_guid: u64, msg: &str) -> bool {
        record(
            &self.0,
            MockEvent::Whisper {
                target: target_guid,
                msg: msg.to_string(),
            },
        );
        true
    }

    fn tell_player(&self, target_guid: u64, msg: &str) -> bool {
        record(
            &self.0,
            MockEvent::TellPlayer {
                target: target_guid,
                msg: msg.to_string(),
            },
        );
        true
    }

    fn tell_addon(&self, target_guid: u64, msg: &str) -> bool {
        record(
            &self.0,
            MockEvent::TellAddon {
                target: target_guid,
                msg: msg.to_string(),
            },
        );
        true
    }

    fn use_item(&self, item_id: ItemId, target: UnitHandle) -> bool {
        record(&self.0, MockEvent::UseItem { item: item_id, target });
        true
    }

    fn taunt(&self, target: UnitHandle) -> bool {
        record(&self.0, MockEvent::Taunt(target));
        true
    }

    /* ── Group ───────────────────────────────────────────────────────── */

    fn group_get_tank(&self) -> Option<UnitHandle> {
        self.0.borrow().group_tank
    }

    fn group_get_healer(&self) -> Option<UnitHandle> {
        self.0.borrow().group_healer
    }

    fn group_get_role(&self, member: UnitHandle) -> BotRole {
        self.0
            .borrow()
            .group_roles
            .get(&member)
            .copied()
            .unwrap_or_default()
    }

    /* ── Loot / NPC / quest list-getters ─────────────────────────────── */

    fn get_nearby_lootable(&self, _range: f32) -> UnitList<'_> {
        boxed(self.0.borrow().nearby_lootable.clone())
    }

    fn get_nearby_npcs(&self, _range: f32, _flags: u32) -> UnitList<'_> {
        boxed(self.0.borrow().nearby_npcs.clone())
    }

    fn get_nearby_enemies(&self, _range: f32) -> UnitList<'_> {
        boxed(self.0.borrow().nearby_enemies.clone())
    }

    fn get_nearby_gossip_npcs(&self, _range: f32) -> UnitList<'_> {
        boxed(self.0.borrow().nearby_gossip_npcs.clone())
    }

    fn get_nearby_gatherables(&self, _range: f32) -> GatherableList<'_> {
        boxed(self.0.borrow().nearby_gatherables.clone())
    }

    fn get_quest_log(&self) -> QuestLog<'_> {
        boxed(self.0.borrow().quest_log.clone())
    }

    fn get_random_point_nearby(&self, _range: f32) -> Option<BotPosition> {
        self.0.borrow().random_point_nearby
    }

    /* ── Spell store / inventory queries ─────────────────────────────── */

    fn get_spell_info(&self, spell_id: u32) -> Option<BotSpellInfo> {
        self.0.borrow().spell_info.get(&spell_id).copied()
    }

    fn get_bot_spells(&self) -> BotSpellList<'_> {
        boxed(self.0.borrow().bot_spells.clone())
    }

    fn get_random_bot_spell_ids(&self) -> BotSpellList<'_> {
        boxed(self.0.borrow().random_bot_spell_ids.clone())
    }

    fn item_count_in_bags(&self, item_id: ItemId) -> u32 {
        self.0
            .borrow()
            .bag_items
            .get(&item_id.0)
            .copied()
            .unwrap_or(0)
    }

    fn item_max_stack_size(&self, item_id: ItemId) -> u32 {
        self.0
            .borrow()
            .item_max_stack
            .get(&item_id.0)
            .copied()
            .unwrap_or(1)
    }

    fn item_prototype_quality(&self, item_id: u32) -> u32 {
        self.0
            .borrow()
            .item_quality
            .get(&item_id)
            .copied()
            .unwrap_or(0)
    }

    fn get_item_info(&self, item_id: u32) -> Option<(String, u8)> {
        self.0.borrow().item_info.get(&item_id).cloned()
    }

    fn bot_get_inventory(&self) -> InventoryList<'_> {
        boxed(self.0.borrow().inventory_items.clone())
    }

    fn bot_get_equipped(&self) -> InventoryList<'_> {
        boxed(self.0.borrow().equipped_items.clone())
    }

    fn bot_get_bank_items(&self) -> InventoryList<'_> {
        boxed(self.0.borrow().bank_items.clone())
    }

    fn bot_get_mail_items(&self) -> InventoryList<'_> {
        boxed(self.0.borrow().mail_items.clone())
    }

    fn bot_empty_bag_slot_count(&self) -> u32 {
        self.0.borrow().empty_bag_slots
    }

    /* ── Skills ──────────────────────────────────────────────────────── */

    fn bot_has_skill(&self, skill_id: u32) -> bool {
        self.0.borrow().skills.contains_key(&skill_id)
    }

    fn bot_get_skill_value(&self, skill_id: u32) -> u32 {
        self.0
            .borrow()
            .skills
            .get(&skill_id)
            .map_or(0, |(v, _)| *v)
    }

    fn bot_get_learned_skills(&self) -> SkillList<'_> {
        let v: Vec<BotSkillEntry> = self
            .0
            .borrow()
            .skills
            .iter()
            .map(|(&skill_id, &(value, max))| BotSkillEntry { skill_id, value, max })
            .collect();
        boxed(v)
    }

    /* ── Talents ─────────────────────────────────────────────────────── */

    fn get_class_talents(&self, spec_no: u8) -> TalentList<'_> {
        let v = self
            .0
            .borrow()
            .class_talents
            .get(&spec_no)
            .cloned()
            .unwrap_or_default();
        boxed(v)
    }

    fn bot_free_talent_points(&self) -> u32 {
        self.0.borrow().free_talent_points
    }

    fn bot_get_spec_tab(&self) -> u32 {
        self.0.borrow().spec_tab
    }

    fn bot_pick_spec_no(&self, incremental: bool) -> u32 {
        let spec = self.0.borrow().spec_tab;
        record(&self.0, MockEvent::PickSpecNo { incremental });
        spec
    }

    /* ── Taxi ────────────────────────────────────────────────────────── */

    fn get_overworld_taxi_nodes(&self, team: u8) -> TaxiNodeList<'_> {
        let v = self
            .0
            .borrow()
            .taxi_nodes
            .get(&team)
            .cloned()
            .unwrap_or_default();
        boxed(v)
    }

    /* ── Reputation ──────────────────────────────────────────────────── */

    fn bot_get_reputation_list(&self) -> ReputationList<'_> {
        let v: Vec<BotReputationEntry> = self.0.borrow().reputations.values().copied().collect();
        boxed(v)
    }

    fn reputation_rank(&self, faction_id: u32) -> u8 {
        self.0
            .borrow()
            .reputation_rank
            .get(&faction_id)
            .copied()
            .unwrap_or(3)
    }

    /* ── Travel ──────────────────────────────────────────────────────── */

    fn find_travel_dests(
        &self,
        _purpose_flags: u32,
        _max_range: f32,
        _max_results: u32,
    ) -> TravelDestList<'_> {
        boxed(self.0.borrow().travel_dests.clone())
    }

    /* ── Random ──────────────────────────────────────────────────────── */

    fn random_u32(&self, min: u32, _max: u32) -> u32 {
        let mut s = self.0.borrow_mut();
        if let Some(v) = s.rng_seq.pop_front() {
            v
        } else if s.rng_default != 0 {
            s.rng_default
        } else {
            min
        }
    }

    /* ── Factory pickers ─────────────────────────────────────────────── */

    fn factory_pick_potion_for_level(&self, level: u32, effect: u32) -> ItemId {
        self.0
            .borrow()
            .potion_picks
            .get(&(level, effect))
            .copied()
            .unwrap_or(ItemId::NONE)
    }

    fn factory_pick_food_for_level(&self, level: u32, category: u32) -> ItemId {
        self.0
            .borrow()
            .food_picks
            .get(&(level, category))
            .copied()
            .unwrap_or(ItemId::NONE)
    }

    fn factory_pick_trade_for_level(&self, level: u32) -> u32 {
        self.0
            .borrow()
            .trade_picks
            .get(&level)
            .copied()
            .unwrap_or(0)
    }

    fn factory_pick_ammo_for_level(&self, level: u32, ammo_subclass: u32) -> u32 {
        self.0
            .borrow()
            .ammo_picks
            .get(&(level, ammo_subclass))
            .copied()
            .unwrap_or(0)
    }

    /* ── Ammo ────────────────────────────────────────────────────────── */

    fn bot_equipped_ranged_subclass(&self) -> u32 {
        self.0.borrow().equipped_ranged_subclass
    }

    fn bot_current_ammo_id(&self) -> u32 {
        self.0.borrow().current_ammo_id
    }

    fn bot_set_ammo(&self, item_id: u32) {
        self.0.borrow_mut().current_ammo_id = item_id;
        record(&self.0, MockEvent::SetAmmo(item_id));
    }

    /* ── Mail ────────────────────────────────────────────────────────── */

    fn bot_mail_summary(&self) -> BotMailSummary {
        let s = self.0.borrow();
        let mails_with_items = s.mail_items.len() as u32;
        BotMailSummary {
            total_mails: mails_with_items,
            mails_with_money: 0,
            mails_with_items,
            total_money: 0,
        }
    }

    /* ── Mutators that record + mutate state ─────────────────────────── */

    fn bot_learn_spell(&self, spell_id: u32) {
        self.0.borrow_mut().knows_spell.insert(spell_id);
        record(&self.0, MockEvent::LearnSpell(spell_id));
    }

    fn bot_remove_spell(&self, spell_id: u32) {
        self.0.borrow_mut().knows_spell.remove(&spell_id);
        record(&self.0, MockEvent::RemoveSpell(spell_id));
    }

    fn bot_learn_default_spells(&self) {
        record(&self.0, MockEvent::LearnDefaultSpells);
    }

    fn bot_learn_class_level_spells(&self, include_quest_rewards: bool) {
        record(
            &self.0,
            MockEvent::LearnClassLevelSpells { include_quest_rewards },
        );
    }

    fn bot_reset_spells(&self) {
        self.0.borrow_mut().knows_spell.clear();
        record(&self.0, MockEvent::ResetSpells);
    }

    fn bot_set_skill(&self, skill_id: u32, value: u32, max: u32) {
        self.0.borrow_mut().skills.insert(skill_id, (value, max));
        record(&self.0, MockEvent::SetSkill { skill: skill_id, value, max });
    }

    fn bot_clear_skill(&self, skill_id: u32) {
        self.0.borrow_mut().skills.remove(&skill_id);
        record(&self.0, MockEvent::ClearSkill(skill_id));
    }

    fn bot_update_skills_for_level(&self) {
        record(&self.0, MockEvent::UpdateSkillsForLevel);
    }

    fn bot_cheat_mask(&self) -> u32 {
        self.0.borrow().cheat_mask
    }

    fn bot_save_to_db_if_not_busy(&self) {
        record(&self.0, MockEvent::SaveToDbIfNotBusy);
    }

    fn bot_resurrect_full(&self) {
        record(&self.0, MockEvent::ResurrectFull);
    }

    fn bot_combat_stop(&self) {
        record(&self.0, MockEvent::CombatStop);
    }

    fn bot_set_level_and_reset_xp(&self, level: u32) {
        record(&self.0, MockEvent::SetLevelAndResetXp(level));
    }

    fn bot_set_player_flag(&self, flag: u32, set: bool) {
        record(&self.0, MockEvent::SetPlayerFlag { flag, set });
    }

    fn factory_config_disable_random_levels(&self) -> bool {
        self.0.borrow().disable_random_levels
    }

    fn factory_config_random_bot_show_helmet(&self) -> bool {
        self.0.borrow().random_bot_show_helmet
    }

    fn factory_config_random_bot_show_cloak(&self) -> bool {
        self.0.borrow().random_bot_show_cloak
    }

    fn factory_is_random_bot(&self) -> bool {
        self.0.borrow().is_random_bot
    }

    fn factory_has_real_player_master(&self) -> bool {
        self.0.borrow().has_real_player_master
    }

    fn factory_is_in_real_guild(&self) -> bool {
        self.0.borrow().is_in_real_guild
    }

    fn factory_config_min_enchanting_bot_level(&self) -> u32 {
        self.0.borrow().min_enchanting_bot_level
    }

    fn factory_load_enchant_container(&self) {
        record(&self.0, MockEvent::LoadEnchantContainer);
    }

    fn bot_reset_talents(&self) {
        record(&self.0, MockEvent::ResetTalents);
    }

    fn bot_learn_quest_rewarded_spells(&self) {
        record(&self.0, MockEvent::LearnQuestRewardedSpells);
    }

    fn bot_get_money(&self) -> u32 {
        self.0.borrow().money
    }

    fn bot_set_money(&self, amount: u32) {
        self.0.borrow_mut().money = amount;
        record(&self.0, MockEvent::SetMoney(amount));
    }

    fn factory_init_all_gems(&self) {
        record(&self.0, MockEvent::InitAllGems);
    }

    fn factory_enchant_all_equipment(&self) {
        record(&self.0, MockEvent::EnchantAllEquipment);
    }

    fn quest_is_eligible_for_bot(&self, quest_id: u32) -> bool {
        self.0.borrow().eligible_quests.contains(&quest_id)
    }

    fn bot_reward_quest_complete(&self, quest_id: u32) {
        record(&self.0, MockEvent::RewardQuestComplete(quest_id));
    }

    fn bot_get_account_id(&self) -> u32 {
        self.0.borrow().account_id
    }

    fn factory_bot_guild_id(&self) -> u32 {
        self.0.borrow().bot_guild_id
    }

    fn factory_query_guild_summary(&self, guild_id: u32) -> Option<crate::GuildSummary> {
        self.0.borrow().guild_summaries.get(&guild_id).cloned()
    }

    fn factory_guild_add_member(&self, guild_id: u32, rank: u32) -> bool {
        // Mirror the FFI contract: an unknown guild id is a failure.
        if !self.0.borrow().guild_summaries.contains_key(&guild_id) {
            return false;
        }
        // Flip the bot's guild id so a follow-up `factory_bot_guild_id`
        // call sees the join — matches the C++ side where `AddMember`
        // updates `Player::m_guildId` synchronously.
        {
            let mut s = self.0.borrow_mut();
            s.bot_guild_id = guild_id;
            if let Some(summary) = s.guild_summaries.get_mut(&guild_id) {
                summary.member_size = summary.member_size.saturating_add(1);
            }
        }
        record(&self.0, MockEvent::GuildAddMember { guild_id, rank });
        true
    }

    fn factory_get_guild_rank_name(&self, guild_id: u32, rank: u32) -> Option<String> {
        self.0
            .borrow()
            .guild_rank_names
            .get(&(guild_id, rank))
            .cloned()
    }

    fn factory_kv_get_u32(&self, key: &str) -> u32 {
        self.0.borrow().kv.get(key).copied().unwrap_or(0)
    }

    fn factory_kv_set_u32(&self, key: &str, value: u32) {
        self.0.borrow_mut().kv.insert(key.to_string(), value);
        record(
            &self.0,
            MockEvent::FactoryKvSet { key: key.to_string(), value },
        );
    }

    fn factory_learn_tradeskill_recipes(&self) {
        record(&self.0, MockEvent::LearnTradeskillRecipes);
    }

    fn factory_bot_guid_low(&self) -> u32 {
        self.0.borrow().bot_guid_low
    }

    fn factory_bot_equipped_item_in_slot(&self, slot: u8) -> u32 {
        self.0
            .borrow()
            .equipped_item_in_slot
            .get(&slot)
            .copied()
            .unwrap_or(0)
    }

    fn factory_destroy_all_equipped_items(&self) {
        self.0.borrow_mut().equipped_item_in_slot.clear();
        record(&self.0, MockEvent::DestroyAllEquippedItems);
    }

    fn factory_equip_new_item_in_slot(
        &self,
        slot: u8,
        item_id: u32,
        random_enchant_id: u32,
        apply_enchants: bool,
    ) -> bool {
        self.0
            .borrow_mut()
            .equipped_item_in_slot
            .insert(slot, item_id);
        record(
            &self.0,
            MockEvent::EquipNewItemInSlot {
                slot,
                item: item_id,
                random_enchant: random_enchant_id,
                apply_enchants,
            },
        );
        true
    }

    fn factory_init_stats_for_level_and_update(&self) {
        record(&self.0, MockEvent::InitStatsForLevelAndUpdate);
    }

    fn factory_master_equip_gear_score(&self) -> Option<u32> {
        self.0.borrow().master_equip_gear_score
    }

    fn factory_tell_master(&self, msg: &str) {
        record(&self.0, MockEvent::TellMaster(msg.to_string()));
    }

    fn factory_bot_has_pet(&self) -> bool {
        self.0.borrow().pet_entry != 0
    }

    fn factory_pet_entry(&self) -> u32 {
        self.0.borrow().pet_entry
    }

    fn factory_pet_family(&self) -> u32 {
        self.0.borrow().pet_family
    }

    fn factory_pet_level(&self) -> u32 {
        self.0.borrow().pet_level
    }

    fn factory_pet_has_spell(&self, spell_id: u32) -> bool {
        self.0.borrow().pet_spells.contains(&spell_id)
    }

    fn factory_pet_autocast_candidate_spells(&self) -> BotSpellList<'_> {
        let s = self.0.borrow();
        let candidates = s.pet_autocast_candidates.clone();
        drop(s);
        OwnedList::from_boxed_slice(candidates.into_boxed_slice())
    }

    fn factory_tameable_creatures_for_bot_level(&self) -> BotSpellList<'_> {
        let s = self.0.borrow();
        let ids: Vec<u32> = s.tameable_creatures.iter().map(|c| c.entry).collect();
        drop(s);
        OwnedList::from_boxed_slice(ids.into_boxed_slice())
    }

    fn factory_create_hunter_pet(&self, creature_entry: u32) -> bool {
        let mut s = self.0.borrow_mut();
        let Some(creature) = s
            .tameable_creatures
            .iter()
            .find(|c| c.entry == creature_entry)
            .copied()
        else {
            return false;
        };
        s.pet_entry = creature.entry;
        s.pet_family = creature.family;
        // InitPet sets the pet level to the bot level right before
        // refreshing stats — mirror that here so the subsequent spell
        // dispatch in `init_pet_spells` sees a non-zero level.
        s.pet_level = u32::from(s.world_snap.self_.level);
        s.pet_is_alive = true;
        drop(s);
        record(&self.0, MockEvent::CreateHunterPet(creature_entry));
        true
    }

    fn factory_pet_refresh_stats(&self) {
        {
            let mut s = self.0.borrow_mut();
            if s.pet_entry != 0 {
                s.pet_level = u32::from(s.world_snap.self_.level);
                s.pet_is_alive = true;
            }
        }
        record(&self.0, MockEvent::PetRefreshStats);
    }

    fn factory_pet_learn_spell(&self, spell_id: u32) {
        {
            let mut s = self.0.borrow_mut();
            if s.pet_entry != 0 {
                s.pet_spells.insert(spell_id);
            }
        }
        record(&self.0, MockEvent::PetLearnSpell(spell_id));
    }

    fn factory_pet_toggle_autocast(&self, spell_id: u32, enable: bool) {
        {
            let mut s = self.0.borrow_mut();
            if s.pet_entry != 0 {
                s.pet_autocast.insert(spell_id, enable);
            }
        }
        record(&self.0, MockEvent::PetToggleAutocast { spell: spell_id, enable });
    }

    fn factory_pet_force_dismiss(&self) {
        {
            let mut s = self.0.borrow_mut();
            s.pet_is_alive = false;
        }
        record(&self.0, MockEvent::PetForceDismiss);
    }

    fn bot_set_reputation(&self, faction_id: u32, value: i32) -> bool {
        let mut s = self.0.borrow_mut();
        s.reputations.insert(
            faction_id,
            BotReputationEntry { faction_id, value, standing: 0 },
        );
        drop(s);
        record(&self.0, MockEvent::SetReputation { faction: faction_id, value });
        true
    }

    fn bot_set_taxi_node(&self, node_index: u32) {
        record(&self.0, MockEvent::SetTaxiNode(node_index));
    }

    fn bot_update_free_talent_points(&self) {
        // Mirrors a learned-rank decrement: each call shaves one point off the
        // budget, saturating at zero. Tests that drive the talent allocator
        // rely on this so the inner loop terminates.
        {
            let mut s = self.0.borrow_mut();
            s.free_talent_points = s.free_talent_points.saturating_sub(1);
        }
        record(&self.0, MockEvent::UpdateFreeTalentPoints);
    }

    fn bot_remove_all_auras(&self) {
        self.0.borrow_mut().auras.clear();
        record(&self.0, MockEvent::RemoveAllAuras);
    }

    fn remove_aura(&self, spell_id: SpellId) {
        for v in self.0.borrow_mut().auras.values_mut() {
            v.retain(|a| a.spell_id != spell_id.0);
        }
        record(&self.0, MockEvent::RemoveAura(spell_id));
    }

    fn bot_reset_all_quests(&self) {
        self.0.borrow_mut().quest_log.clear();
        record(&self.0, MockEvent::ResetAllQuests);
    }

    fn inventory_destroy_equipped_and_bags(&self) {
        let mut s = self.0.borrow_mut();
        s.equipped_items.clear();
        s.inventory_items.clear();
        s.bag_items.clear();
        drop(s);
        record(&self.0, MockEvent::DestroyEquippedAndBags);
    }

    fn inventory_destroy_all(&self) {
        let mut s = self.0.borrow_mut();
        s.equipped_items.clear();
        s.inventory_items.clear();
        s.bag_items.clear();
        s.bank_items.clear();
        drop(s);
        record(&self.0, MockEvent::DestroyAll);
    }

    fn inventory_add_item(&self, item_id: ItemId, count: u32) -> u32 {
        *self.0.borrow_mut().bag_items.entry(item_id.0).or_insert(0) += count;
        record(
            &self.0,
            MockEvent::InventoryAddItem { item: item_id, count },
        );
        count
    }

    fn bot_store_new_in_best_slots(&self, item_id: ItemId, count: u32) -> bool {
        *self.0.borrow_mut().bag_items.entry(item_id.0).or_insert(0) += count;
        record(
            &self.0,
            MockEvent::StoreInBestSlots { item: item_id, count },
        );
        true
    }

    /* ── Log files ───────────────────────────────────────────────────── */

    fn bot_write_log_file(&self, name: &str, body: &str) -> bool {
        self.0
            .borrow_mut()
            .log_files
            .insert(name.to_string(), body.to_string());
        record(
            &self.0,
            MockEvent::WriteLogFile {
                name: name.to_string(),
                body: body.to_string(),
            },
        );
        true
    }

    fn bot_append_log_file(&self, name: &str, line: &str) -> bool {
        self.0
            .borrow_mut()
            .log_files
            .entry(name.to_string())
            .or_default()
            .push_str(line);
        record(
            &self.0,
            MockEvent::AppendLogFile {
                name: name.to_string(),
                line: line.to_string(),
            },
        );
        true
    }

    fn bot_read_log_file(&self, name: &str) -> Option<String> {
        self.0.borrow().log_files.get(name).cloned()
    }
}

/* ── Tests ───────────────────────────────────────────────────────────────── */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_world_basic_queries() {
        let w = MockWorld::default();
        assert!(w.has_los(0));
        assert!(w.can_reach(0.0, 0.0, 0.0));
        assert!(w.can_cast(SpellId(123), 0));
        assert_eq!(w.spell_cooldown_ms(SpellId(123)), 0);
        assert_eq!(w.unit_distance(0), 0.0);
        assert!(w.events().is_empty());
    }

    #[test]
    fn cast_spell_records_event() {
        let w = MockWorld::default();
        assert!(w.cast_spell(SpellId(133), 42));
        assert_eq!(w.events().len(), 1);
        assert_eq!(
            w.last_event(),
            Some(MockEvent::CastSpell { spell: SpellId(133), target: 42 })
        );
        w.clear_events();
        assert!(w.events().is_empty());
    }

    #[test]
    fn builder_sets_knows_spell() {
        let w = MockWorld::builder().knows_spell(133).build();
        assert!(w.knows_spell(SpellId(133)));
        assert!(!w.knows_spell(SpellId(134)));
    }

    #[test]
    fn aura_lookups() {
        let aura = BotAuraInfo {
            spell_id: 1459,
            duration_ms: 60_000,
            max_duration_ms: 60_000,
            stacks: 1,
            is_mine: true,
            is_harmful: false,
            is_passive: false,
        };
        let w = MockWorld::builder().aura(7, aura).build();
        assert!(w.has_aura(7, SpellId(1459)));
        assert!(!w.has_aura(7, SpellId(1)));
        assert_eq!(w.get_auras(7).len(), 1);
        assert_eq!(w.get_auras(99).len(), 0);
    }

    #[test]
    fn bag_items_round_trip() {
        let w = MockWorld::builder().item_in_bags(6948, 1).build();
        assert_eq!(w.bot_item_count(ItemId(6948)), 1);
        assert_eq!(w.item_count_in_bags(ItemId(6948)), 1);
        assert!(w.has_item(ItemId(6948)));
    }

    #[test]
    fn learn_spell_mutates_and_records() {
        let w = MockWorld::default();
        w.bot_learn_spell(133);
        assert!(w.knows_spell(SpellId(133)));
        assert_eq!(w.last_event(), Some(MockEvent::LearnSpell(133)));
    }

    #[test]
    fn skill_set_and_get() {
        let w = MockWorld::default();
        w.bot_set_skill(164, 300, 300); // blacksmithing 300/300
        assert!(w.bot_has_skill(164));
        assert_eq!(w.bot_get_skill_value(164), 300);
        let skills = w.bot_get_learned_skills();
        assert_eq!(skills.len(), 1);
    }

    #[test]
    fn cooldown_decays_on_tick() {
        let w = MockWorld::builder().spell_cooldown(133, 1500).build();
        assert_eq!(w.spell_cooldown_ms(SpellId(133)), 1500);
        assert!(!w.can_cast(SpellId(133), 0));
        w.tick(1000);
        assert_eq!(w.spell_cooldown_ms(SpellId(133)), 500);
        w.tick(1000);
        assert_eq!(w.spell_cooldown_ms(SpellId(133)), 0);
        assert!(w.can_cast(SpellId(133), 0));
    }

    #[test]
    fn rng_seq_then_default() {
        let w = MockWorld::builder()
            .random_seq(vec![10, 20])
            .random_default(99)
            .build();
        assert_eq!(w.random_u32(0, 100), 10);
        assert_eq!(w.random_u32(0, 100), 20);
        assert_eq!(w.random_u32(0, 100), 99);
    }

    #[test]
    fn log_file_round_trip() {
        let w = MockWorld::default();
        assert!(w.bot_write_log_file("rtsc.log", "hello"));
        assert_eq!(
            w.bot_read_log_file("rtsc.log").as_deref(),
            Some("hello")
        );
    }
}
