/// `BotInterface` — the Rust abstraction over the C `BotCallbacks` vtable.
///
/// Production code uses `RealInterface` (wraps the C function pointer table).
/// Tests use `MockInterface` (in-memory mock that records all commands issued).
///
/// `BtNode` and `TickContext` use `&dyn BotInterface` so they work in both contexts
/// without any conditional compilation.
use super::{
    BotAuraInfo, BotCallbacks, BotHandle, BotPosition, BotThreatEntry,
    BotUnitSnapshot, BotWorldSnapshot, UnitHandle,
    types::{BotRole, ItemId, SpellId},
};

/// The complete interface a bot has to the game world.
/// All queries return owned data — no lifetimes tied to C++ pointers.
pub trait BotInterface: Send {
    /* ── State snapshot ──────────────────────────────────────────────── */

    /// Read the full bot+group snapshot for this tick. Call once per tick.
    fn get_snapshot(&self) -> BotWorldSnapshot;

    /// Read a specific unit's snapshot (group member, nearby enemy, boss).
    fn get_unit_snapshot(&self, target: UnitHandle) -> BotUnitSnapshot;

    /* ── Aura queries ────────────────────────────────────────────────── */

    fn has_aura(&self, unit: UnitHandle, spell_id: SpellId) -> bool;
    fn get_aura(&self, unit: UnitHandle, spell_id: SpellId) -> Option<BotAuraInfo>;
    /// All auras on `unit`. Used for encounter phase detection and debuff tracking.
    fn get_auras(&self, unit: UnitHandle) -> Vec<BotAuraInfo>;

    /* ── Threat queries ──────────────────────────────────────────────── */

    /// Full threat list on `target_unit` (e.g. boss), ordered highest→lowest.
    fn get_threat_list(&self, target_unit: UnitHandle) -> Vec<BotThreatEntry>;
    /// Threat that `from_unit` has on `target_unit`.
    fn get_unit_threat(&self, target_unit: UnitHandle, from_unit: UnitHandle) -> f32;

    /* ── Unit queries ────────────────────────────────────────────────── */

    fn unit_distance(&self, target: UnitHandle) -> f32;
    fn can_cast(&self, spell_id: SpellId, target: UnitHandle) -> bool;
    fn spell_cooldown_ms(&self, spell_id: SpellId) -> u32;
    fn has_los(&self, target: UnitHandle) -> bool;
    fn get_nearby_units(&self, range: f32, hostile: bool) -> Vec<UnitHandle>;

    /* ── Pathfinding / positioning ───────────────────────────────────── */

    /// Position directly behind `target` at `distance` yards (cleave avoidance).
    fn get_behind_position(&self, target: UnitHandle, distance: f32) -> BotPosition;
    /// Nearest reachable position not in a ground hazard within `search_radius` yards.
    fn get_safe_position(&self, search_radius: f32) -> Option<BotPosition>;
    /// Spread position: this bot is index `idx` of `total` bots spreading at `radius` around `center`.
    fn get_spread_position(
        &self,
        center: UnitHandle,
        radius: f32,
        idx: u8,
        total: u8,
    ) -> BotPosition;
    /// Returns true if the bot can pathfind to (x, y, z).
    fn can_reach(&self, x: f32, y: f32, z: f32) -> bool;

    /* ── Commands ────────────────────────────────────────────────────── */

    fn cast_spell(&self, spell_id: SpellId, target: UnitHandle) -> bool;
    fn cast_spell_pos(&self, spell_id: SpellId, x: f32, y: f32, z: f32) -> bool;
    fn move_to(&self, x: f32, y: f32, z: f32) -> bool;
    fn follow(&self, target: UnitHandle, dist: f32, angle: f32) -> bool;
    fn stop_moving(&self) -> bool;
    fn attack(&self, target: UnitHandle) -> bool;
    fn auto_attack(&self, enable: bool) -> bool;
    fn say(&self, msg: &str, lang: u32) -> bool;
    /// Whisper a message directly to a specific player (`target_guid`).
    /// Used for per-command replies to the sender.
    fn whisper(&self, _target_guid: u64, _msg: &str) -> bool {
        false
    }
    fn use_item(&self, item_id: ItemId, target: UnitHandle) -> bool;
    fn taunt(&self, target: UnitHandle) -> bool;

    /// Find the nearest hostile unit currently marked with the given raid
    /// target icon (1 = star, 2 = circle, 3 = diamond, 4 = triangle,
    /// 5 = moon, 6 = square, 7 = cross, 8 = skull). Returns `None` if
    /// no unit is marked with that icon in range.
    fn get_unit_with_raid_icon(&self, icon: u8) -> Option<UnitHandle> {
        let _ = icon;
        None
    }

    /* ── Group / raid ────────────────────────────────────────────────── */

    fn group_get_tank(&self) -> Option<UnitHandle>;
    fn group_get_healer(&self) -> Option<UnitHandle>;
    fn group_get_role(&self, member: UnitHandle) -> BotRole;

    /* ── Death / resurrection ───────────────────────────────────────── */

    fn accept_resurrect(&self) -> bool {
        false
    }
    fn get_corpse_position(&self) -> Option<BotPosition> {
        None
    }
    fn use_spirit_healer(&self) -> bool {
        false
    }

    /* ── Mount ──────────────────────────────────────────────────────── */

    fn is_mounted(&self) -> bool {
        false
    }
    fn mount_up(&self) -> bool {
        false
    }
    fn dismount(&self) -> bool {
        false
    }
    fn is_indoor(&self) -> bool {
        false
    }

    /* ── Loot ───────────────────────────────────────────────────────── */

    fn get_nearby_lootable(&self, _range: f32) -> Vec<UnitHandle> {
        vec![]
    }
    fn open_loot(&self, _target: UnitHandle) -> bool {
        false
    }
    fn take_all_loot(&self) -> bool {
        false
    }

    /* ── NPC interaction ────────────────────────────────────────────── */

    fn get_nearby_npcs(&self, _range: f32, _npc_flags: u32) -> Vec<UnitHandle> {
        vec![]
    }
    fn interact_npc(&self, _npc: UnitHandle) -> bool {
        false
    }
    fn repair_all(&self) -> bool {
        false
    }
    fn sell_grey_items(&self) -> bool {
        false
    }
    fn has_sellable_items(&self) -> bool {
        false
    }
    fn get_durability_pct(&self) -> f32 {
        1.0
    }

    /* ── Quest ──────────────────────────────────────────────────────── */

    fn get_quest_log(&self) -> Vec<QuestInfo> {
        vec![]
    }
    fn accept_all_quests(&self, _npc: UnitHandle) -> bool {
        false
    }
    fn turn_in_quest(&self, _npc: UnitHandle, _quest_id: u32) -> bool {
        false
    }

    /* ── Unit queries (extended) ────────────────────────────────────── */

    fn is_attackable(&self, _target: UnitHandle) -> bool {
        false
    }
    fn get_unit_level(&self, _target: UnitHandle) -> u8 {
        0
    }
    fn is_casting_interruptible(&self, _target: UnitHandle) -> bool {
        false
    }

    /* ── Pet management ─────────────────────────────────────────────── */

    fn has_pet(&self) -> bool {
        false
    }
    fn pet_is_alive(&self) -> bool {
        false
    }
    fn pet_happiness(&self) -> u8 {
        3
    } // 1=unhappy, 2=content, 3=happy
    fn summon_pet(&self) -> bool {
        false
    }
    fn revive_pet(&self) -> bool {
        false
    }
    fn feed_pet(&self) -> bool {
        false
    }

    /* ── Dispel / party aura queries ────────────────────────────────── */

    /// Find a group member with a dispellable debuff that this bot can remove.
    /// Returns (`member_handle`, `debuff_spell_id`).
    fn find_dispellable_target(&self) -> Option<(UnitHandle, SpellId)> {
        None
    }

    /// Find a dead group member that can be resurrected.
    fn find_dead_party_member(&self) -> Option<UnitHandle> {
        None
    }

    /* ── Battleground ───────────────────────────────────────────────── */

    /// True if the bot is currently in a battleground instance.
    fn is_in_battleground(&self) -> bool {
        false
    }
    /// BG type: 1=AV, 2=WSG, 3=AB. 0 if not in BG.
    fn battleground_type(&self) -> u8 {
        0
    }
    /// Get the position of a nearby capturable BG objective (flag, base).
    fn get_bg_objective(&self) -> Option<BotPosition> {
        None
    }
    /// Interact with a BG objective (pick up flag, capture base).
    fn capture_bg_objective(&self) -> bool {
        false
    }
    /// Get nearby enemy players within range.
    fn get_nearby_enemies(&self, _range: f32) -> Vec<UnitHandle> {
        vec![]
    }

    /* ── RPG / social ───────────────────────────────────────────────── */

    /// Get a random navigable point near the bot within range.
    fn get_random_point_nearby(&self, _range: f32) -> Option<BotPosition> {
        None
    }
    /// Play an emote (wave, bow, dance, etc.).
    fn emote(&self, _emote_id: u32) -> bool {
        false
    }
    /// Get nearby friendly NPCs to gossip with (non-vendor, non-quest).
    fn get_nearby_gossip_npcs(&self, _range: f32) -> Vec<UnitHandle> {
        vec![]
    }

    /* ── Gathering (mining, herbalism, skinning) ────────────────────── */

    /// True if the bot has any gathering profession (mining, herbalism, skinning).
    fn has_gathering_skill(&self) -> bool {
        false
    }
    /// Get nearby gatherable game objects (ore veins, herb nodes) or skinnable corpses.
    fn get_nearby_gatherables(&self, _range: f32) -> Vec<u64> {
        vec![]
    }
    /// Interact with a gatherable node/corpse to gather it.
    fn gather_node(&self, _handle: u64) -> bool {
        false
    }
    /// Distance to a game object handle.
    fn gameobject_distance(&self, _handle: u64) -> f32 {
        f32::MAX
    }
    /// Position of a game object handle.
    fn gameobject_position(&self, _handle: u64) -> BotPosition {
        BotPosition::default()
    }

    /* ── Factory: inventory mutation ─────────────────────────────────── */

    /// Destroy every equipped item plus every item in backpack + carried bags.
    /// Bank contents are left intact. Used by the factory before re-rolling gear.
    fn inventory_destroy_equipped_and_bags(&self) {}

    /// Destroy every item the bot owns (equipped, bags, and bank).
    fn inventory_destroy_all(&self) {}

    /// Return how many of `item_id` the bot holds in backpack + carried bags
    /// (excludes bank). Used by factory restock checks.
    fn item_count_in_bags(&self, _item_id: ItemId) -> u32 {
        0
    }

    /// Add `count` of `item_id` to the bot's bags. Returns the number actually
    /// added (may be less than requested if bags are full).
    fn inventory_add_item(&self, _item_id: ItemId, _count: u32) -> u32 {
        0
    }

    /// Max stack size for `item_id` (1 if the item prototype is unknown).
    fn item_max_stack_size(&self, _item_id: ItemId) -> u32 {
        1
    }

    /* ── Factory: consumable selection ────────────────────────────────── */

    /// Pick a potion item ID appropriate for `level` and spell effect
    /// (10 = `SPELL_EFFECT_HEAL`, 30 = `SPELL_EFFECT_ENERGIZE`). Returns 0 if
    /// no suitable potion exists for that level/effect.
    fn factory_pick_potion_for_level(&self, _level: u32, _effect: u32) -> ItemId {
        ItemId::NONE
    }

    /// Pick a food item ID appropriate for `level` and food category
    /// (11 = food, 59 = drink). Returns 0 if none.
    fn factory_pick_food_for_level(&self, _level: u32, _category: u32) -> ItemId {
        ItemId::NONE
    }

    /* ── RNG ─────────────────────────────────────────────────────────── */

    /// Uniform random integer in `[min, max]` (inclusive).
    fn random_u32(&self, _min: u32, _max: u32) -> u32 {
        0
    }

    /* ── Factory: progression wipe ───────────────────────────────────── */

    /// Zero out one trade skill on the bot. Called from the factory when
    /// re-rolling a bot's trade professions.
    fn bot_clear_skill(&self, _skill_id: u32) {}

    /// Reset the bot's spellbook to the class's starter spells
    /// (removes all learned spells except the defaults).
    fn bot_reset_spells(&self) {}

    /// Clear every quest from the bot's quest log and drop any
    /// completed/rewarded quest status (also deletes the DB rows).
    fn bot_reset_all_quests(&self) {}

    /* ── Factory: misc pre/post init ─────────────────────────────────── */

    /// Strip every aura (buffs and debuffs) currently on the bot.
    fn bot_remove_all_auras(&self) {}

    /// Whether the bot has the given skill learned (at any rank).
    fn bot_has_skill(&self, _skill_id: u32) -> bool {
        false
    }
}

/// Quest info returned from the FFI.
#[derive(Debug, Clone)]
pub struct QuestInfo {
    pub quest_id: u32,
    pub complete: bool,
}

// ── Production implementation ─────────────────────────────────────────────

/// Wraps the C `BotCallbacks` function-pointer table.
/// `cbs` is valid for the lifetime of this struct (it points into C++ memory
/// that outlives the bot session).
pub struct RealInterface {
    handle: BotHandle,
    cbs: BotCallbacks,
}

impl RealInterface {
    /// # Safety
    /// `cbs` must be a fully-initialized `BotCallbacks` with all function pointers set.
    /// The struct must remain valid for the lifetime of this `RealInterface`.
    pub fn new(handle: BotHandle, cbs: BotCallbacks) -> Self {
        Self { handle, cbs }
    }
}

#[expect(unsafe_code)]
impl BotInterface for RealInterface {
    fn get_snapshot(&self) -> BotWorldSnapshot {
        unsafe { (self.cbs.get_snapshot.unwrap())(self.handle) }
    }

    fn get_unit_snapshot(&self, target: UnitHandle) -> BotUnitSnapshot {
        unsafe { (self.cbs.get_unit_snapshot.unwrap())(self.handle, target) }
    }

    fn has_aura(&self, unit: UnitHandle, spell_id: SpellId) -> bool {
        unsafe { (self.cbs.has_aura.unwrap())(self.handle, unit, spell_id.raw()) }
    }

    fn get_aura(&self, unit: UnitHandle, spell_id: SpellId) -> Option<BotAuraInfo> {
        let info = unsafe { (self.cbs.get_aura.unwrap())(self.handle, unit, spell_id.raw()) };
        if info.spell_id == 0 { None } else { Some(info) }
    }

    fn get_auras(&self, unit: UnitHandle) -> Vec<BotAuraInfo> {
        let mut count: u32 = 0;
        let ptr = unsafe { (self.cbs.get_auras.unwrap())(self.handle, unit, &mut count) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let vec = unsafe { std::slice::from_raw_parts(ptr, count as usize).to_vec() };
        unsafe { (self.cbs.free_aura_list.unwrap())(ptr) };
        vec
    }

    fn get_threat_list(&self, target_unit: UnitHandle) -> Vec<BotThreatEntry> {
        let mut count: u32 = 0;
        let ptr =
            unsafe { (self.cbs.get_threat_list.unwrap())(self.handle, target_unit, &mut count) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let vec = unsafe { std::slice::from_raw_parts(ptr, count as usize).to_vec() };
        unsafe { (self.cbs.free_threat_list.unwrap())(ptr) };
        vec
    }

    fn get_unit_threat(&self, target_unit: UnitHandle, from_unit: UnitHandle) -> f32 {
        unsafe { (self.cbs.get_unit_threat.unwrap())(self.handle, target_unit, from_unit) }
    }

    fn unit_distance(&self, target: UnitHandle) -> f32 {
        unsafe { (self.cbs.unit_distance.unwrap())(self.handle, target) }
    }

    fn can_cast(&self, spell_id: SpellId, target: UnitHandle) -> bool {
        unsafe { (self.cbs.can_cast.unwrap())(self.handle, spell_id.raw(), target) }
    }

    fn spell_cooldown_ms(&self, spell_id: SpellId) -> u32 {
        unsafe { (self.cbs.spell_cooldown_ms.unwrap())(self.handle, spell_id.raw()) }
    }

    fn has_los(&self, target: UnitHandle) -> bool {
        unsafe { (self.cbs.has_los.unwrap())(self.handle, target) }
    }

    fn get_nearby_units(&self, range: f32, hostile: bool) -> Vec<UnitHandle> {
        let mut count: u32 = 0;
        let ptr = unsafe {
            (self.cbs.get_nearby_units.unwrap())(self.handle, range, hostile, &mut count)
        };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let vec = unsafe { std::slice::from_raw_parts(ptr, count as usize).to_vec() };
        unsafe { (self.cbs.free_unit_list.unwrap())(ptr) };
        vec
    }

    fn get_behind_position(&self, target: UnitHandle, distance: f32) -> BotPosition {
        unsafe { (self.cbs.get_behind_position.unwrap())(self.handle, target, distance) }
    }

    fn get_safe_position(&self, search_radius: f32) -> Option<BotPosition> {
        let result = unsafe { (self.cbs.get_safe_position.unwrap())(self.handle, search_radius) };
        if result.found {
            Some(BotPosition {
                x: result.x,
                y: result.y,
                z: result.z,
                o: 0.0,
                map_id: 0,
            })
        } else {
            None
        }
    }

    fn get_spread_position(
        &self,
        center: UnitHandle,
        radius: f32,
        idx: u8,
        total: u8,
    ) -> BotPosition {
        unsafe { (self.cbs.get_spread_position.unwrap())(self.handle, center, radius, idx, total) }
    }

    fn can_reach(&self, x: f32, y: f32, z: f32) -> bool {
        unsafe { (self.cbs.can_reach.unwrap())(self.handle, x, y, z) }
    }

    fn cast_spell(&self, spell_id: SpellId, target: UnitHandle) -> bool {
        unsafe { (self.cbs.cast_spell.unwrap())(self.handle, spell_id.raw(), target) }
    }

    fn cast_spell_pos(&self, spell_id: SpellId, x: f32, y: f32, z: f32) -> bool {
        unsafe { (self.cbs.cast_spell_pos.unwrap())(self.handle, spell_id.raw(), x, y, z) }
    }

    fn move_to(&self, x: f32, y: f32, z: f32) -> bool {
        unsafe { (self.cbs.move_to.unwrap())(self.handle, x, y, z) }
    }

    fn follow(&self, target: UnitHandle, dist: f32, angle: f32) -> bool {
        unsafe { (self.cbs.follow.unwrap())(self.handle, target, dist, angle) }
    }

    fn stop_moving(&self) -> bool {
        unsafe { (self.cbs.stop_moving.unwrap())(self.handle) }
    }

    fn attack(&self, target: UnitHandle) -> bool {
        unsafe { (self.cbs.attack.unwrap())(self.handle, target) }
    }

    fn auto_attack(&self, enable: bool) -> bool {
        unsafe { (self.cbs.auto_attack.unwrap())(self.handle, enable) }
    }

    fn say(&self, msg: &str, lang: u32) -> bool {
        let c_str = std::ffi::CString::new(msg).unwrap_or_default();
        unsafe { (self.cbs.say.unwrap())(self.handle, c_str.as_ptr(), lang) }
    }

    fn whisper(&self, target_guid: u64, msg: &str) -> bool {
        let c_str = std::ffi::CString::new(msg).unwrap_or_default();
        unsafe { (self.cbs.whisper.unwrap())(self.handle, target_guid, c_str.as_ptr()) }
    }

    fn use_item(&self, item_id: ItemId, target: UnitHandle) -> bool {
        unsafe { (self.cbs.use_item.unwrap())(self.handle, item_id.raw(), target) }
    }

    fn taunt(&self, target: UnitHandle) -> bool {
        unsafe { (self.cbs.taunt.unwrap())(self.handle, target) }
    }

    fn group_get_tank(&self) -> Option<UnitHandle> {
        let h = unsafe { (self.cbs.group_get_tank.unwrap())(self.handle) };
        if h == 0 { None } else { Some(h) }
    }

    fn group_get_healer(&self) -> Option<UnitHandle> {
        let h = unsafe { (self.cbs.group_get_healer.unwrap())(self.handle) };
        if h == 0 { None } else { Some(h) }
    }

    fn group_get_role(&self, member: UnitHandle) -> BotRole {
        BotRole(unsafe { (self.cbs.group_get_role.unwrap())(self.handle, member) })
    }

    fn get_unit_with_raid_icon(&self, icon: u8) -> Option<UnitHandle> {
        let h = unsafe { (self.cbs.get_unit_with_raid_icon.unwrap())(self.handle, icon) };
        if h == 0 { None } else { Some(h) }
    }

    /* ── Death / resurrection ───────────────────────────────────────── */

    fn accept_resurrect(&self) -> bool {
        unsafe { (self.cbs.accept_resurrect.unwrap())(self.handle) }
    }

    fn get_corpse_position(&self) -> Option<BotPosition> {
        let pos = unsafe { (self.cbs.get_corpse_position.unwrap())(self.handle) };
        if pos.x == 0.0 && pos.y == 0.0 && pos.z == 0.0 {
            None
        } else {
            Some(pos)
        }
    }

    fn use_spirit_healer(&self) -> bool {
        unsafe { (self.cbs.use_spirit_healer.unwrap())(self.handle) }
    }

    /* ── Mount ──────────────────────────────────────────────────────── */

    fn is_mounted(&self) -> bool {
        unsafe { (self.cbs.is_mounted.unwrap())(self.handle) }
    }

    fn mount_up(&self) -> bool {
        unsafe { (self.cbs.mount_up.unwrap())(self.handle) }
    }

    fn dismount(&self) -> bool {
        unsafe { (self.cbs.dismount.unwrap())(self.handle) }
    }

    fn is_indoor(&self) -> bool {
        unsafe { (self.cbs.is_indoor.unwrap())(self.handle) }
    }

    /* ── Loot ───────────────────────────────────────────────────────── */

    fn get_nearby_lootable(&self, range: f32) -> Vec<UnitHandle> {
        let mut count: u32 = 0;
        let ptr =
            unsafe { (self.cbs.get_nearby_lootable.unwrap())(self.handle, range, &mut count) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let vec = unsafe { std::slice::from_raw_parts(ptr, count as usize).to_vec() };
        unsafe { (self.cbs.free_unit_list.unwrap())(ptr) };
        vec
    }

    fn open_loot(&self, target: UnitHandle) -> bool {
        unsafe { (self.cbs.open_loot.unwrap())(self.handle, target) }
    }

    fn take_all_loot(&self) -> bool {
        unsafe { (self.cbs.take_all_loot.unwrap())(self.handle) }
    }

    /* ── NPC interaction ────────────────────────────────────────────── */

    fn get_nearby_npcs(&self, range: f32, npc_flags: u32) -> Vec<UnitHandle> {
        let mut count: u32 = 0;
        let ptr = unsafe {
            (self.cbs.get_nearby_npcs.unwrap())(self.handle, range, npc_flags, &mut count)
        };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let vec = unsafe { std::slice::from_raw_parts(ptr, count as usize).to_vec() };
        unsafe { (self.cbs.free_unit_list.unwrap())(ptr) };
        vec
    }

    fn interact_npc(&self, npc: UnitHandle) -> bool {
        unsafe { (self.cbs.interact_npc.unwrap())(self.handle, npc) }
    }

    fn repair_all(&self) -> bool {
        unsafe { (self.cbs.repair_all.unwrap())(self.handle) }
    }

    fn sell_grey_items(&self) -> bool {
        unsafe { (self.cbs.sell_grey_items.unwrap())(self.handle) }
    }

    fn has_sellable_items(&self) -> bool {
        unsafe { (self.cbs.has_sellable_items.unwrap())(self.handle) }
    }

    fn get_durability_pct(&self) -> f32 {
        unsafe { (self.cbs.get_durability_pct.unwrap())(self.handle) }
    }

    /* ── Quest ──────────────────────────────────────────────────────── */

    fn get_quest_log(&self) -> Vec<QuestInfo> {
        let mut count: u32 = 0;
        let ptr = unsafe { (self.cbs.get_quest_log.unwrap())(self.handle, &mut count) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let slice = unsafe { std::slice::from_raw_parts(ptr, count as usize) };
        let vec = slice
            .iter()
            .map(|q| QuestInfo {
                quest_id: q.quest_id,
                complete: q.complete,
            })
            .collect();
        unsafe { (self.cbs.free_quest_log.unwrap())(ptr) };
        vec
    }

    fn accept_all_quests(&self, npc: UnitHandle) -> bool {
        unsafe { (self.cbs.accept_all_quests.unwrap())(self.handle, npc) }
    }

    fn turn_in_quest(&self, npc: UnitHandle, quest_id: u32) -> bool {
        unsafe { (self.cbs.turn_in_quest.unwrap())(self.handle, npc, quest_id) }
    }

    /* ── Unit queries (extended) ────────────────────────────────────── */

    fn is_attackable(&self, target: UnitHandle) -> bool {
        unsafe { (self.cbs.is_attackable.unwrap())(self.handle, target) }
    }

    fn get_unit_level(&self, target: UnitHandle) -> u8 {
        unsafe { (self.cbs.get_unit_level.unwrap())(self.handle, target) }
    }

    fn is_casting_interruptible(&self, target: UnitHandle) -> bool {
        unsafe { (self.cbs.is_casting_interruptible.unwrap())(self.handle, target) }
    }

    /* ── Pet management ─────────────────────────────────────────────── */

    fn has_pet(&self) -> bool {
        unsafe { (self.cbs.has_pet.unwrap())(self.handle) }
    }

    fn pet_is_alive(&self) -> bool {
        unsafe { (self.cbs.pet_is_alive.unwrap())(self.handle) }
    }

    fn pet_happiness(&self) -> u8 {
        unsafe { (self.cbs.pet_happiness.unwrap())(self.handle) }
    }

    fn summon_pet(&self) -> bool {
        unsafe { (self.cbs.summon_pet.unwrap())(self.handle) }
    }

    fn revive_pet(&self) -> bool {
        unsafe { (self.cbs.revive_pet.unwrap())(self.handle) }
    }

    fn feed_pet(&self) -> bool {
        unsafe { (self.cbs.feed_pet.unwrap())(self.handle) }
    }

    /* ── Dispel / party queries ─────────────────────────────────────── */

    fn find_dispellable_target(&self) -> Option<(UnitHandle, SpellId)> {
        let result = unsafe { (self.cbs.find_dispellable_target.unwrap())(self.handle) };
        if result.found {
            Some((result.unit, SpellId(result.spell_id)))
        } else {
            None
        }
    }

    fn find_dead_party_member(&self) -> Option<UnitHandle> {
        let h = unsafe { (self.cbs.find_dead_party_member.unwrap())(self.handle) };
        if h == 0 { None } else { Some(h) }
    }

    /* ── Battleground ───────────────────────────────────────────────── */

    fn is_in_battleground(&self) -> bool {
        unsafe { (self.cbs.is_in_battleground.unwrap())(self.handle) }
    }

    fn battleground_type(&self) -> u8 {
        unsafe { (self.cbs.battleground_type.unwrap())(self.handle) }
    }

    fn get_bg_objective(&self) -> Option<BotPosition> {
        let result = unsafe { (self.cbs.get_bg_objective.unwrap())(self.handle) };
        if result.found {
            Some(BotPosition {
                x: result.x,
                y: result.y,
                z: result.z,
                o: 0.0,
                map_id: 0,
            })
        } else {
            None
        }
    }

    fn capture_bg_objective(&self) -> bool {
        unsafe { (self.cbs.capture_bg_objective.unwrap())(self.handle) }
    }

    fn get_nearby_enemies(&self, range: f32) -> Vec<UnitHandle> {
        let mut count: u32 = 0;
        let ptr = unsafe { (self.cbs.get_nearby_enemies.unwrap())(self.handle, range, &mut count) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let vec = unsafe { std::slice::from_raw_parts(ptr, count as usize).to_vec() };
        unsafe { (self.cbs.free_unit_list.unwrap())(ptr) };
        vec
    }

    /* ── RPG / social ───────────────────────────────────────────────── */

    fn get_random_point_nearby(&self, range: f32) -> Option<BotPosition> {
        let result = unsafe { (self.cbs.get_random_point_nearby.unwrap())(self.handle, range) };
        if result.found {
            Some(BotPosition {
                x: result.x,
                y: result.y,
                z: result.z,
                o: 0.0,
                map_id: 0,
            })
        } else {
            None
        }
    }

    fn emote(&self, emote_id: u32) -> bool {
        unsafe { (self.cbs.emote.unwrap())(self.handle, emote_id) }
    }

    fn get_nearby_gossip_npcs(&self, range: f32) -> Vec<UnitHandle> {
        let mut count: u32 = 0;
        let ptr =
            unsafe { (self.cbs.get_nearby_gossip_npcs.unwrap())(self.handle, range, &mut count) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let vec = unsafe { std::slice::from_raw_parts(ptr, count as usize).to_vec() };
        unsafe { (self.cbs.free_unit_list.unwrap())(ptr) };
        vec
    }

    /* ── Gathering ──────────────────────────────────────────────────── */

    fn has_gathering_skill(&self) -> bool {
        unsafe { (self.cbs.has_gathering_skill.unwrap())(self.handle) }
    }

    fn get_nearby_gatherables(&self, range: f32) -> Vec<u64> {
        let mut count: u32 = 0;
        let ptr =
            unsafe { (self.cbs.get_nearby_gatherables.unwrap())(self.handle, range, &mut count) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let vec = unsafe { std::slice::from_raw_parts(ptr, count as usize).to_vec() };
        unsafe { (self.cbs.free_gatherable_list.unwrap())(ptr) };
        vec
    }

    fn gather_node(&self, handle: u64) -> bool {
        unsafe { (self.cbs.gather_node.unwrap())(self.handle, handle) }
    }

    fn gameobject_distance(&self, handle: u64) -> f32 {
        unsafe { (self.cbs.gameobject_distance.unwrap())(self.handle, handle) }
    }

    fn gameobject_position(&self, handle: u64) -> BotPosition {
        unsafe { (self.cbs.gameobject_position.unwrap())(self.handle, handle) }
    }

    /* ── Factory: inventory mutation ─────────────────────────────────── */

    fn inventory_destroy_equipped_and_bags(&self) {
        unsafe { (self.cbs.inventory_destroy_equipped_and_bags.unwrap())(self.handle) }
    }

    fn inventory_destroy_all(&self) {
        unsafe { (self.cbs.inventory_destroy_all.unwrap())(self.handle) }
    }

    fn item_count_in_bags(&self, item_id: ItemId) -> u32 {
        unsafe { (self.cbs.item_count_in_bags.unwrap())(self.handle, item_id.raw()) }
    }

    fn inventory_add_item(&self, item_id: ItemId, count: u32) -> u32 {
        unsafe { (self.cbs.inventory_add_item.unwrap())(self.handle, item_id.raw(), count) }
    }

    fn item_max_stack_size(&self, item_id: ItemId) -> u32 {
        unsafe { (self.cbs.item_max_stack_size.unwrap())(self.handle, item_id.raw()) }
    }

    fn factory_pick_potion_for_level(&self, level: u32, effect: u32) -> ItemId {
        ItemId(unsafe {
            (self.cbs.factory_pick_potion_for_level.unwrap())(self.handle, level, effect)
        })
    }

    fn factory_pick_food_for_level(&self, level: u32, category: u32) -> ItemId {
        ItemId(unsafe {
            (self.cbs.factory_pick_food_for_level.unwrap())(self.handle, level, category)
        })
    }

    fn random_u32(&self, min: u32, max: u32) -> u32 {
        unsafe { (self.cbs.random_u32.unwrap())(self.handle, min, max) }
    }

    /* ── Factory: progression wipe ───────────────────────────────────── */

    fn bot_clear_skill(&self, skill_id: u32) {
        unsafe { (self.cbs.bot_clear_skill.unwrap())(self.handle, skill_id) }
    }

    fn bot_reset_spells(&self) {
        unsafe { (self.cbs.bot_reset_spells.unwrap())(self.handle) }
    }

    fn bot_reset_all_quests(&self) {
        unsafe { (self.cbs.bot_reset_all_quests.unwrap())(self.handle) }
    }

    /* ── Factory: misc pre/post init ─────────────────────────────────── */

    fn bot_remove_all_auras(&self) {
        unsafe { (self.cbs.bot_remove_all_auras.unwrap())(self.handle) }
    }

    fn bot_has_skill(&self, skill_id: u32) -> bool {
        unsafe { (self.cbs.bot_has_skill.unwrap())(self.handle, skill_id) }
    }
}
