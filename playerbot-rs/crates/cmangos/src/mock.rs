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
            skills: HashMap::new(),
            class_talents: HashMap::new(),
            free_talent_points: 0,
            spec_tab: 0,
            taxi_nodes: HashMap::new(),
            reputations: HashMap::new(),
            reputation_rank: HashMap::new(),
            quest_log: Vec::new(),
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
