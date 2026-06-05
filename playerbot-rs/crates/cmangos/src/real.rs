//! `VtableWorld` — production `World` impl backed by the C `BotCallbacks`
//! function pointer table. Every getter/command method is a thin wrapper
//! that unwraps the matching function pointer and forwards the call.
//!
//! Imported verbatim from the legacy `RealInterface` impl, with two
//! mechanical changes:
//!
//! 1. Each list-returning method constructs an `OwnedList` directly from
//!    the raw FFI pointer instead of copying through `collect_ffi_list`.
//! 2. The `bot_read_log_file` string-return path uses `bot_free_string`
//!    behind a guarded `OwnedCString`.

#![allow(clippy::doc_markdown, clippy::too_many_arguments)]

use crate::{
    BotAuraInfo, BotCallbacks, BotHandle, BotMailSummary, BotPosition, BotRole, BotSpellInfo,
    BotUnitSnapshot, BotWorldSnapshot, GuildSummary, ItemId, SpellId, UnitHandle, World,
    owned::{
        AuraList, BotSpellList, Free, GatherableList, InventoryList, OwnedList, QuestLog,
        ReputationList, SkillList, TalentList, TaxiNodeList, ThreatList, TravelDestList,
        UnitList,
    },
};

/// Convert a raw `UnitHandle` (0 = none) into `Option<UnitHandle>`.
#[inline(always)]
const fn handle_option(h: UnitHandle) -> Option<UnitHandle> {
    if h == 0 { None } else { Some(h) }
}

/// Build an `OwnedList<T>` from a `(ptr, count)` pair returned by an FFI
/// getter and the matching `free_*` callback.
///
/// # Safety
/// `ptr` must either be null or point to a buffer of `count` elements
/// allocated by the C++ side; `free` must be the matching deallocator.
#[inline(always)]
unsafe fn ffi_list<T>(
    ptr: *mut T,
    count: u32,
    free: unsafe extern "C" fn(*mut T),
) -> OwnedList<'static, T> {
    unsafe { OwnedList::from_raw(ptr, count as usize, Free::Ffi(free)) }
}

/// Wraps the C `BotCallbacks` function-pointer table.
/// `cbs` is valid for the lifetime of this struct (it points into C++ memory
/// that outlives the bot session).
pub struct VtableWorld {
    handle: BotHandle,
    cbs: BotCallbacks,
}

impl VtableWorld {
    /// # Safety
    /// `cbs` must be a fully-initialized `BotCallbacks` with all function pointers set.
    /// The struct must remain valid for the lifetime of this `VtableWorld`.
    pub fn new(handle: BotHandle, cbs: BotCallbacks) -> Self {
        Self { handle, cbs }
    }
}

impl World for VtableWorld {
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

    fn get_auras(&self, unit: UnitHandle) -> AuraList<'_> {
        let mut count: u32 = 0;
        let ptr = unsafe { (self.cbs.get_auras.unwrap())(self.handle, unit, &mut count) };
        unsafe { ffi_list(ptr, count, self.cbs.free_aura_list.unwrap()) }
    }

    fn get_threat_list(&self, target_unit: UnitHandle) -> ThreatList<'_> {
        let mut count: u32 = 0;
        let ptr =
            unsafe { (self.cbs.get_threat_list.unwrap())(self.handle, target_unit, &mut count) };
        unsafe { ffi_list(ptr, count, self.cbs.free_threat_list.unwrap()) }
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

    fn get_nearby_units(&self, range: f32, hostile: bool) -> UnitList<'_> {
        let mut count: u32 = 0;
        let ptr = unsafe {
            (self.cbs.get_nearby_units.unwrap())(self.handle, range, hostile, &mut count)
        };
        unsafe { ffi_list(ptr, count, self.cbs.free_unit_list.unwrap()) }
    }

    fn get_attackers(&self) -> UnitList<'_> {
        let mut count: u32 = 0;
        let ptr = unsafe { (self.cbs.get_attackers.unwrap())(self.handle, &mut count) };
        unsafe { ffi_list(ptr, count, self.cbs.free_unit_list.unwrap()) }
    }

    fn bot_is_behind(&self, target: UnitHandle) -> bool {
        unsafe { (self.cbs.bot_is_behind.unwrap())(self.handle, target) }
    }

    fn bot_equipped_weapon_subclass(&self, slot: u8) -> u32 {
        unsafe { (self.cbs.bot_equipped_weapon_subclass.unwrap())(self.handle, slot) }
    }

    fn bot_item_count(&self, item_id: ItemId) -> u32 {
        unsafe { (self.cbs.bot_item_count.unwrap())(self.handle, item_id.raw()) }
    }

    fn bot_active_totem_mask(&self) -> u8 {
        unsafe { (self.cbs.bot_active_totem_mask.unwrap())(self.handle) }
    }

    fn bot_weapon_enchanted(&self, slot: u8) -> bool {
        unsafe { (self.cbs.bot_weapon_enchanted.unwrap())(self.handle, slot) }
    }

    fn bot_runes_ready_mask(&self) -> u8 {
        unsafe { (self.cbs.bot_runes_ready_mask.unwrap())(self.handle) }
    }

    fn knows_spell(&self, spell_id: SpellId) -> bool {
        unsafe { (self.cbs.bot_knows_spell.unwrap())(self.handle, spell_id.raw()) }
    }

    fn get_spell_name(&self, spell_id: SpellId) -> String {
        let mut buf = [0u8; 128];
        let len = unsafe {
            (self.cbs.get_spell_name.unwrap())(
                spell_id.raw(),
                buf.as_mut_ptr().cast::<i8>(),
                buf.len() as u32,
            )
        };
        if len == 0 {
            return format!("#{}", spell_id.raw());
        }
        String::from_utf8_lossy(&buf[..len as usize]).into_owned()
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

    fn can_pathfind_to(&self, x: f32, y: f32, z: f32) -> bool {
        unsafe { (self.cbs.can_pathfind_to.unwrap())(self.handle, x, y, z) }
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

    fn chase(&self, target: UnitHandle, dist: f32, angle: f32) -> bool {
        unsafe { (self.cbs.chase.unwrap())(self.handle, target, dist, angle) }
    }

    fn stop_moving(&self) -> bool {
        unsafe { (self.cbs.stop_moving.unwrap())(self.handle) }
    }

    fn set_facing(&self, angle: f32) {
        unsafe { (self.cbs.set_facing.unwrap())(self.handle, angle) }
    }

    fn attack(&self, target: UnitHandle) -> bool {
        unsafe { (self.cbs.attack.unwrap())(self.handle, target) }
    }

    fn auto_shoot(&self, target: UnitHandle) -> bool {
        unsafe { (self.cbs.auto_shoot.unwrap())(self.handle, target) }
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

    fn tell_player(&self, target_guid: u64, msg: &str) -> bool {
        let c_str = std::ffi::CString::new(msg).unwrap_or_default();
        unsafe { (self.cbs.tell_player.unwrap())(self.handle, target_guid, c_str.as_ptr()) }
    }

    fn tell_addon(&self, target_guid: u64, msg: &str) -> bool {
        let c_str = std::ffi::CString::new(msg).unwrap_or_default();
        unsafe { (self.cbs.bot_tell_addon.unwrap())(self.handle, target_guid, c_str.as_ptr()) }
    }

    fn send_group_addon(&self, prefix: &str, msg: &str) -> bool {
        let c_prefix = std::ffi::CString::new(prefix).unwrap_or_default();
        let c_msg = std::ffi::CString::new(msg).unwrap_or_default();
        unsafe {
            (self.cbs.bot_send_group_addon.unwrap())(
                self.handle,
                c_prefix.as_ptr(),
                c_msg.as_ptr(),
            )
        }
    }

    fn use_item(&self, item_id: ItemId, target: UnitHandle) -> bool {
        unsafe { (self.cbs.use_item.unwrap())(self.handle, item_id.raw(), target) }
    }

    fn taunt(&self, target: UnitHandle) -> bool {
        unsafe { (self.cbs.taunt.unwrap())(self.handle, target) }
    }

    fn teleport_to(&self, map_id: u32, x: f32, y: f32, z: f32, o: f32) -> bool {
        unsafe { (self.cbs.teleport_to.unwrap())(self.handle, map_id, x, y, z, o) }
    }

    fn get_player_position(&self, player_guid: u64) -> Option<BotPosition> {
        let mut out: BotPosition = unsafe { std::mem::zeroed() };
        let ok =
            unsafe { (self.cbs.get_player_position.unwrap())(self.handle, player_guid, &mut out) };
        if ok { Some(out) } else { None }
    }

    fn summon_to_player(&self, requester_guid: u64) -> bool {
        unsafe { (self.cbs.summon_to_player.unwrap())(self.handle, requester_guid) }
    }

    fn group_get_tank(&self) -> Option<UnitHandle> {
        handle_option(unsafe { (self.cbs.group_get_tank.unwrap())(self.handle) })
    }

    fn group_get_healer(&self) -> Option<UnitHandle> {
        handle_option(unsafe { (self.cbs.group_get_healer.unwrap())(self.handle) })
    }

    fn group_get_role(&self, member: UnitHandle) -> BotRole {
        BotRole(unsafe { (self.cbs.group_get_role.unwrap())(self.handle, member) })
    }

    fn get_unit_with_raid_icon(&self, icon: u8) -> Option<UnitHandle> {
        handle_option(unsafe { (self.cbs.get_unit_with_raid_icon.unwrap())(self.handle, icon) })
    }

    fn group_set_target_icon(&self, target: UnitHandle, icon: u8) -> bool {
        unsafe { (self.cbs.group_set_target_icon.unwrap())(self.handle, target, icon) }
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

    fn resurrect_self(&self) -> bool {
        unsafe { (self.cbs.resurrect_self.unwrap())(self.handle) }
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

    fn get_nearby_lootable(&self, range: f32) -> UnitList<'_> {
        let mut count: u32 = 0;
        let ptr =
            unsafe { (self.cbs.get_nearby_lootable.unwrap())(self.handle, range, &mut count) };
        unsafe { ffi_list(ptr, count, self.cbs.free_unit_list.unwrap()) }
    }

    fn open_loot(&self, target: UnitHandle) -> bool {
        unsafe { (self.cbs.open_loot.unwrap())(self.handle, target) }
    }

    fn take_all_loot(&self) -> bool {
        unsafe { (self.cbs.take_all_loot.unwrap())(self.handle) }
    }

    /* ── NPC interaction ────────────────────────────────────────────── */

    fn get_nearby_npcs(&self, range: f32, npc_flags: u32) -> UnitList<'_> {
        let mut count: u32 = 0;
        let ptr = unsafe {
            (self.cbs.get_nearby_npcs.unwrap())(self.handle, range, npc_flags, &mut count)
        };
        unsafe { ffi_list(ptr, count, self.cbs.free_unit_list.unwrap()) }
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

    fn get_quest_log(&self) -> QuestLog<'_> {
        let mut count: u32 = 0;
        let ptr = unsafe { (self.cbs.get_quest_log.unwrap())(self.handle, &mut count) };
        unsafe { ffi_list(ptr, count, self.cbs.free_quest_log.unwrap()) }
    }

    fn accept_all_quests(&self, npc: UnitHandle) -> bool {
        unsafe { (self.cbs.accept_all_quests.unwrap())(self.handle, npc) }
    }

    fn turn_in_quest(&self, npc: UnitHandle, quest_id: u32) -> bool {
        unsafe { (self.cbs.turn_in_quest.unwrap())(self.handle, npc, quest_id) }
    }

    fn use_nearby_quest_object(&self, range: f32) -> bool {
        unsafe { (self.cbs.use_nearby_quest_object.unwrap())(self.handle, range) }
    }

    fn is_quest_objective_creature(&self, entry: u32) -> bool {
        unsafe { (self.cbs.is_quest_objective_creature.unwrap())(self.handle, entry) }
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

    fn unit_kind(&self, target: UnitHandle) -> u8 {
        unsafe { (self.cbs.unit_kind.unwrap())(self.handle, target) }
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

    fn pet_health_pct(&self) -> u8 {
        unsafe { (self.cbs.pet_health_pct.unwrap())(self.handle) }
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

    fn pet_attack(&self, target: UnitHandle) -> bool {
        unsafe { (self.cbs.pet_attack.unwrap())(self.handle, target) }
    }

    /* ── Dispel / party queries ─────────────────────────────────────── */

    fn find_dispellable_target(&self, dispel_mask: u8) -> Option<(UnitHandle, SpellId)> {
        let result =
            unsafe { (self.cbs.find_dispellable_target.unwrap())(self.handle, dispel_mask) };
        if result.found {
            Some((result.unit, SpellId(result.spell_id)))
        } else {
            None
        }
    }

    fn find_dead_party_member(&self) -> Option<UnitHandle> {
        handle_option(unsafe { (self.cbs.find_dead_party_member.unwrap())(self.handle) })
    }

    fn find_potion_in_bags(&self, category: u8) -> ItemId {
        let id = unsafe { (self.cbs.find_potion_in_bags.unwrap())(self.handle, category) };
        ItemId(id)
    }

    fn find_food_drink_in_bags(&self, category: u32) -> ItemId {
        let id = unsafe { (self.cbs.find_food_drink_in_bags.unwrap())(self.handle, category) };
        ItemId(id)
    }

    fn potion_cooldown_ready(&self) -> bool {
        unsafe { (self.cbs.potion_cooldown_ready.unwrap())(self.handle) }
    }

    fn use_trinket(&self, slot: u8) -> bool {
        unsafe { (self.cbs.use_trinket.unwrap())(self.handle, slot) }
    }

    fn accept_group_invite(&self) -> bool {
        unsafe { (self.cbs.accept_group_invite.unwrap())(self.handle) }
    }
    fn leave_group(&self) -> bool {
        unsafe { (self.cbs.leave_group.unwrap())(self.handle) }
    }
    fn accept_ready_check(&self) -> bool {
        unsafe { (self.cbs.accept_ready_check.unwrap())(self.handle) }
    }
    fn accept_trade(&self) -> bool {
        unsafe { (self.cbs.accept_trade.unwrap())(self.handle) }
    }
    fn accept_duel(&self) -> bool {
        unsafe { (self.cbs.accept_duel.unwrap())(self.handle) }
    }
    fn decline_duel(&self) -> bool {
        unsafe { (self.cbs.decline_duel.unwrap())(self.handle) }
    }
    fn accept_summon(&self) -> bool {
        match self.cbs.accept_summon {
            Some(f) => unsafe { f(self.handle) },
            None => false,
        }
    }
    fn use_meeting_stone(&self) -> bool {
        match self.cbs.use_meeting_stone {
            Some(f) => unsafe { f(self.handle) },
            None => false,
        }
    }

    fn is_pvp_flagged(&self) -> bool {
        unsafe { (self.cbs.is_pvp_flagged.unwrap())(self.handle) }
    }

    fn duel_state(&self) -> u8 {
        unsafe { (self.cbs.duel_state.unwrap())(self.handle) }
    }

    fn reputation_rank(&self, faction_id: u32) -> u8 {
        unsafe { (self.cbs.reputation_rank.unwrap())(self.handle, faction_id) }
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

    fn get_nearby_enemies(&self, range: f32) -> UnitList<'_> {
        let mut count: u32 = 0;
        let ptr = unsafe { (self.cbs.get_nearby_enemies.unwrap())(self.handle, range, &mut count) };
        unsafe { ffi_list(ptr, count, self.cbs.free_unit_list.unwrap()) }
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

    fn get_nearby_gossip_npcs(&self, range: f32) -> UnitList<'_> {
        let mut count: u32 = 0;
        let ptr =
            unsafe { (self.cbs.get_nearby_gossip_npcs.unwrap())(self.handle, range, &mut count) };
        unsafe { ffi_list(ptr, count, self.cbs.free_unit_list.unwrap()) }
    }

    /* ── Gathering ──────────────────────────────────────────────────── */

    fn has_gathering_skill(&self) -> bool {
        unsafe { (self.cbs.has_gathering_skill.unwrap())(self.handle) }
    }

    fn get_nearby_gatherables(&self, range: f32) -> GatherableList<'_> {
        let mut count: u32 = 0;
        let ptr =
            unsafe { (self.cbs.get_nearby_gatherables.unwrap())(self.handle, range, &mut count) };
        unsafe { ffi_list(ptr, count, self.cbs.free_gatherable_list.unwrap()) }
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

    fn nearby_gameobject_by_entry(&self, entry: u32, range: f32) -> Option<u64> {
        let h =
            unsafe { (self.cbs.nearby_gameobject_by_entry.unwrap())(self.handle, entry, range) };
        if h == 0 { None } else { Some(h) }
    }

    fn use_gameobject(&self, handle: u64) -> bool {
        unsafe { (self.cbs.use_gameobject.unwrap())(self.handle, handle) }
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

    fn remove_aura(&self, spell_id: SpellId) {
        unsafe { (self.cbs.bot_remove_aura_by_id.unwrap())(self.handle, spell_id.raw()) }
    }

    fn bot_has_skill(&self, skill_id: u32) -> bool {
        unsafe { (self.cbs.bot_has_skill.unwrap())(self.handle, skill_id) }
    }

    fn bot_learn_spell(&self, spell_id: u32) {
        unsafe { (self.cbs.bot_learn_spell.unwrap())(self.handle, spell_id) }
    }

    fn bot_remove_spell(&self, spell_id: u32) {
        unsafe { (self.cbs.bot_remove_spell.unwrap())(self.handle, spell_id) }
    }

    fn bot_learn_default_spells(&self) {
        unsafe { (self.cbs.bot_learn_default_spells.unwrap())(self.handle) }
    }

    fn bot_learn_class_level_spells(&self, include_quest_rewards: bool) {
        unsafe {
            (self.cbs.bot_learn_class_level_spells.unwrap())(self.handle, include_quest_rewards);
        }
    }

    fn get_spell_info(&self, spell_id: u32) -> Option<BotSpellInfo> {
        let info = unsafe { (self.cbs.get_spell_info.unwrap())(self.handle, spell_id) };
        if info.is_valid { Some(info) } else { None }
    }

    fn get_bot_spells(&self) -> BotSpellList<'_> {
        let mut count: u32 = 0;
        let ptr = unsafe { (self.cbs.get_bot_spells.unwrap())(self.handle, &mut count) };
        unsafe { ffi_list(ptr, count, self.cbs.free_bot_spells.unwrap()) }
    }

    fn resolve_spell_by_name(&self, name: &str) -> u32 {
        let c_name = std::ffi::CString::new(name).unwrap_or_default();
        unsafe { (self.cbs.resolve_spell_by_name.unwrap())(self.handle, c_name.as_ptr()) }
    }

    fn resolve_item_by_name(&self, name: &str) -> u32 {
        let c_name = std::ffi::CString::new(name).unwrap_or_default();
        unsafe { (self.cbs.resolve_item_by_name.unwrap())(self.handle, c_name.as_ptr()) }
    }

    fn equip_item(&self, item_id: ItemId) -> bool {
        unsafe { (self.cbs.equip_item.unwrap())(self.handle, item_id.raw()) }
    }

    fn give_leader(&self, target_guid: UnitHandle) -> bool {
        unsafe { (self.cbs.give_leader.unwrap())(self.handle, target_guid) }
    }

    fn resolve_player_by_name(&self, name: &str) -> UnitHandle {
        let c_name = std::ffi::CString::new(name).unwrap_or_default();
        unsafe { (self.cbs.resolve_player_by_name.unwrap())(self.handle, c_name.as_ptr()) }
    }

    fn unequip_item(&self, item_id: ItemId) -> bool {
        unsafe { (self.cbs.unequip_item.unwrap())(self.handle, item_id.raw()) }
    }

    fn invite_to_group(&self, target_guid: UnitHandle) -> bool {
        unsafe { (self.cbs.invite_to_group.unwrap())(self.handle, target_guid) }
    }

    fn destroy_item(&self, item_id: ItemId) -> bool {
        unsafe { (self.cbs.destroy_item.unwrap())(self.handle, item_id.raw()) }
    }

    fn share_quest(&self, quest_id: u32) -> bool {
        unsafe { (self.cbs.share_quest.unwrap())(self.handle, quest_id) }
    }

    fn do_text_emote(&self, emote_id: u32) -> bool {
        unsafe { (self.cbs.do_text_emote.unwrap())(self.handle, emote_id) }
    }

    fn bot_empty_bag_slot_count(&self) -> u32 {
        unsafe { (self.cbs.bot_empty_bag_slot_count.unwrap())(self.handle) }
    }

    fn bot_store_new_in_best_slots(&self, item_id: ItemId, count: u32) -> bool {
        unsafe {
            (self.cbs.bot_store_new_in_best_slots.unwrap())(self.handle, item_id.raw(), count)
        }
    }

    fn bot_set_reputation(&self, faction_id: u32, value: i32) -> bool {
        unsafe { (self.cbs.bot_set_reputation.unwrap())(self.handle, faction_id, value) }
    }

    fn bot_equipped_ranged_subclass(&self) -> u32 {
        unsafe { (self.cbs.bot_equipped_ranged_subclass.unwrap())(self.handle) }
    }

    fn bot_current_ammo_id(&self) -> u32 {
        unsafe { (self.cbs.bot_current_ammo_id.unwrap())(self.handle) }
    }

    fn factory_pick_ammo_for_level(&self, level: u32, ammo_subclass: u32) -> u32 {
        unsafe {
            (self.cbs.factory_pick_ammo_for_level.unwrap())(self.handle, level, ammo_subclass)
        }
    }

    fn bot_set_ammo(&self, item_id: u32) {
        unsafe { (self.cbs.bot_set_ammo.unwrap())(self.handle, item_id) }
    }

    fn bot_get_skill_value(&self, skill_id: u32) -> u32 {
        unsafe { (self.cbs.bot_get_skill_value.unwrap())(self.handle, skill_id) }
    }

    fn bot_set_skill(&self, skill_id: u32, value: u32, max: u32) {
        unsafe { (self.cbs.bot_set_skill.unwrap())(self.handle, skill_id, value, max) }
    }

    fn bot_update_skills_for_level(&self) {
        unsafe { (self.cbs.bot_update_skills_for_level.unwrap())(self.handle) }
    }

    fn bot_cheat_mask(&self) -> u32 {
        unsafe { (self.cbs.bot_cheat_mask.unwrap())(self.handle) }
    }

    fn bot_save_to_db_if_not_busy(&self) {
        unsafe { (self.cbs.bot_save_to_db_if_not_busy.unwrap())(self.handle) }
    }

    fn bot_resurrect_full(&self) {
        unsafe { (self.cbs.bot_resurrect_full.unwrap())(self.handle) }
    }

    fn bot_combat_stop(&self) {
        unsafe { (self.cbs.bot_combat_stop.unwrap())(self.handle) }
    }

    fn bot_set_level_and_reset_xp(&self, level: u32) {
        unsafe { (self.cbs.bot_set_level_and_reset_xp.unwrap())(self.handle, level) }
    }

    fn bot_set_player_flag(&self, flag: u32, set: bool) {
        unsafe { (self.cbs.bot_set_player_flag.unwrap())(self.handle, flag, set) }
    }

    fn factory_config_disable_random_levels(&self) -> bool {
        unsafe { (self.cbs.factory_config_disable_random_levels.unwrap())(self.handle) }
    }

    fn factory_config_random_bot_show_helmet(&self) -> bool {
        unsafe { (self.cbs.factory_config_random_bot_show_helmet.unwrap())(self.handle) }
    }

    fn factory_config_random_bot_show_cloak(&self) -> bool {
        unsafe { (self.cbs.factory_config_random_bot_show_cloak.unwrap())(self.handle) }
    }

    fn factory_is_random_bot(&self) -> bool {
        unsafe { (self.cbs.factory_is_random_bot.unwrap())(self.handle) }
    }

    fn factory_has_real_player_master(&self) -> bool {
        unsafe { (self.cbs.factory_has_real_player_master.unwrap())(self.handle) }
    }

    fn factory_is_in_real_guild(&self) -> bool {
        unsafe { (self.cbs.factory_is_in_real_guild.unwrap())(self.handle) }
    }

    fn factory_config_min_enchanting_bot_level(&self) -> u32 {
        unsafe { (self.cbs.factory_config_min_enchanting_bot_level.unwrap())(self.handle) }
    }

    fn factory_load_enchant_container(&self) {
        unsafe { (self.cbs.factory_load_enchant_container.unwrap())(self.handle) }
    }

    fn bot_reset_talents(&self) {
        unsafe { (self.cbs.bot_reset_talents.unwrap())(self.handle) }
    }

    fn bot_learn_quest_rewarded_spells(&self) {
        unsafe { (self.cbs.bot_learn_quest_rewarded_spells.unwrap())(self.handle) }
    }

    fn bot_get_money(&self) -> u32 {
        unsafe { (self.cbs.bot_get_money.unwrap())(self.handle) }
    }

    fn bot_set_money(&self, amount: u32) {
        unsafe { (self.cbs.bot_set_money.unwrap())(self.handle, amount) }
    }

    fn factory_init_all_gems(&self) {
        unsafe { (self.cbs.factory_init_all_gems.unwrap())(self.handle) }
    }

    fn factory_enchant_all_equipment(&self) {
        unsafe { (self.cbs.factory_enchant_all_equipment.unwrap())(self.handle) }
    }

    fn quest_is_eligible_for_bot(&self, quest_id: u32) -> bool {
        unsafe { (self.cbs.quest_is_eligible_for_bot.unwrap())(self.handle, quest_id) }
    }

    fn bot_reward_quest_complete(&self, quest_id: u32) {
        unsafe { (self.cbs.bot_reward_quest_complete.unwrap())(self.handle, quest_id) }
    }

    fn bot_get_account_id(&self) -> u32 {
        unsafe { (self.cbs.bot_get_account_id.unwrap())(self.handle) }
    }

    fn factory_bot_guild_id(&self) -> u32 {
        unsafe { (self.cbs.factory_bot_guild_id.unwrap())(self.handle) }
    }

    fn factory_query_guild_summary(&self, guild_id: u32) -> Option<GuildSummary> {
        let mut leader_team: u8 = 0;
        let mut member_size: u32 = 0;
        let mut max_members_hint: u32 = 0;
        let mut name_buf = [0u8; 128];
        let ok = unsafe {
            (self.cbs.factory_query_guild_summary.unwrap())(
                self.handle,
                guild_id,
                &mut leader_team,
                &mut member_size,
                &mut max_members_hint,
                name_buf.as_mut_ptr().cast::<core::ffi::c_char>(),
                name_buf.len() as u32,
            )
        };
        if !ok {
            return None;
        }
        let end = name_buf.iter().position(|&b| b == 0).unwrap_or(name_buf.len());
        let name = core::str::from_utf8(&name_buf[..end]).unwrap_or("?").to_string();
        Some(GuildSummary {
            leader_team,
            member_size,
            max_members_hint,
            name,
        })
    }

    fn factory_guild_add_member(&self, guild_id: u32, rank: u32) -> bool {
        unsafe { (self.cbs.factory_guild_add_member.unwrap())(self.handle, guild_id, rank) }
    }

    fn factory_get_guild_rank_name(&self, guild_id: u32, rank: u32) -> Option<String> {
        let mut buf = [0u8; 64];
        let ok = unsafe {
            (self.cbs.factory_get_guild_rank_name.unwrap())(
                self.handle,
                guild_id,
                rank,
                buf.as_mut_ptr().cast::<core::ffi::c_char>(),
                buf.len() as u32,
            )
        };
        if !ok {
            return None;
        }
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Some(core::str::from_utf8(&buf[..end]).unwrap_or("?").to_string())
    }

    fn factory_kv_get_u32(&self, key: &str) -> u32 {
        // NUL-terminated key for the C side. Small fixed stack buffer keeps
        // the common case alloc-free; overlong keys fall back to a heap
        // allocation via CString.
        use std::ffi::CString;
        let Ok(c_key) = CString::new(key) else {
            return 0;
        };
        unsafe { (self.cbs.factory_kv_get_u32.unwrap())(self.handle, c_key.as_ptr()) }
    }

    fn factory_kv_set_u32(&self, key: &str, value: u32) {
        use std::ffi::CString;
        let Ok(c_key) = CString::new(key) else {
            return;
        };
        unsafe { (self.cbs.factory_kv_set_u32.unwrap())(self.handle, c_key.as_ptr(), value) }
    }

    fn factory_learn_tradeskill_recipes(&self) {
        unsafe { (self.cbs.factory_learn_tradeskill_recipes.unwrap())(self.handle) }
    }

    fn item_prototype_quality(&self, item_id: u32) -> u32 {
        unsafe { (self.cbs.item_prototype_quality.unwrap())(self.handle, item_id) }
    }

    fn factory_bot_guid_low(&self) -> u32 {
        unsafe { (self.cbs.factory_bot_guid_low.unwrap())(self.handle) }
    }

    fn factory_bot_equipped_item_in_slot(&self, slot: u8) -> u32 {
        unsafe { (self.cbs.factory_bot_equipped_item_in_slot.unwrap())(self.handle, slot) }
    }

    fn factory_destroy_all_equipped_items(&self) {
        unsafe { (self.cbs.factory_destroy_all_equipped_items.unwrap())(self.handle) }
    }

    fn factory_equip_new_item_in_slot(
        &self,
        slot: u8,
        item_id: u32,
        random_enchant_id: u32,
        apply_enchants: bool,
    ) -> bool {
        unsafe {
            (self.cbs.factory_equip_new_item_in_slot.unwrap())(
                self.handle,
                slot,
                item_id,
                random_enchant_id,
                apply_enchants,
            )
        }
    }

    fn factory_init_stats_for_level_and_update(&self) {
        unsafe { (self.cbs.factory_init_stats_for_level_and_update.unwrap())(self.handle) }
    }

    fn factory_master_equip_gear_score(&self) -> Option<u32> {
        let mut gs: u32 = 0;
        let ok = unsafe {
            (self.cbs.factory_master_equip_gear_score.unwrap())(self.handle, &mut gs as *mut u32)
        };
        if ok {
            Some(gs)
        } else {
            None
        }
    }

    fn factory_tell_master(&self, msg: &str) {
        let c_msg = std::ffi::CString::new(msg).unwrap_or_default();
        unsafe { (self.cbs.factory_tell_master.unwrap())(self.handle, c_msg.as_ptr()) }
    }

    fn factory_bot_has_pet(&self) -> bool {
        unsafe { (self.cbs.factory_bot_has_pet.unwrap())(self.handle) }
    }

    fn factory_pet_entry(&self) -> u32 {
        unsafe { (self.cbs.factory_pet_entry.unwrap())(self.handle) }
    }

    fn factory_pet_family(&self) -> u32 {
        unsafe { (self.cbs.factory_pet_family.unwrap())(self.handle) }
    }

    fn factory_pet_level(&self) -> u32 {
        unsafe { (self.cbs.factory_pet_level.unwrap())(self.handle) }
    }

    fn factory_pet_has_spell(&self, spell_id: u32) -> bool {
        unsafe { (self.cbs.factory_pet_has_spell.unwrap())(self.handle, spell_id) }
    }

    fn factory_pet_autocast_candidate_spells(&self) -> BotSpellList<'_> {
        let mut count: u32 = 0;
        let ptr = unsafe {
            (self.cbs.factory_pet_autocast_candidate_spells.unwrap())(self.handle, &mut count)
        };
        // Reuses free_bot_spells — same malloc/free contract (uint32_t array).
        unsafe { ffi_list(ptr, count, self.cbs.free_bot_spells.unwrap()) }
    }

    fn factory_tameable_creatures_for_bot_level(&self) -> BotSpellList<'_> {
        let mut count: u32 = 0;
        let ptr = unsafe {
            (self.cbs.factory_tameable_creatures_for_bot_level.unwrap())(self.handle, &mut count)
        };
        // Reuses free_bot_spells — same malloc/free contract (uint32_t array).
        unsafe { ffi_list(ptr, count, self.cbs.free_bot_spells.unwrap()) }
    }

    fn factory_create_hunter_pet(&self, creature_entry: u32) -> bool {
        unsafe { (self.cbs.factory_create_hunter_pet.unwrap())(self.handle, creature_entry) }
    }

    fn factory_pet_refresh_stats(&self) {
        unsafe { (self.cbs.factory_pet_refresh_stats.unwrap())(self.handle) }
    }

    fn factory_pet_learn_spell(&self, spell_id: u32) {
        unsafe { (self.cbs.factory_pet_learn_spell.unwrap())(self.handle, spell_id) }
    }

    fn factory_pet_toggle_autocast(&self, spell_id: u32, enable: bool) {
        unsafe {
            (self.cbs.factory_pet_toggle_autocast.unwrap())(self.handle, spell_id, enable);
        }
    }

    fn factory_pet_force_dismiss(&self) {
        unsafe { (self.cbs.factory_pet_force_dismiss.unwrap())(self.handle) }
    }

    fn factory_pick_trade_for_level(&self, level: u32) -> u32 {
        unsafe { (self.cbs.factory_pick_trade_for_level.unwrap())(self.handle, level) }
    }

    fn get_random_bot_spell_ids(&self) -> BotSpellList<'_> {
        let mut count: u32 = 0;
        let ptr = unsafe { (self.cbs.get_random_bot_spell_ids.unwrap())(self.handle, &mut count) };
        // Reuses free_bot_spells — same malloc/free contract (uint32_t array).
        unsafe { ffi_list(ptr, count, self.cbs.free_bot_spells.unwrap()) }
    }

    fn get_overworld_taxi_nodes(&self, team: u8) -> TaxiNodeList<'_> {
        let mut count: u32 = 0;
        let ptr =
            unsafe { (self.cbs.get_overworld_taxi_nodes.unwrap())(self.handle, team, &mut count) };
        unsafe { ffi_list(ptr, count, self.cbs.free_taxi_nodes.unwrap()) }
    }

    fn bot_set_taxi_node(&self, node_index: u32) {
        unsafe { (self.cbs.bot_set_taxi_node.unwrap())(self.handle, node_index) };
    }

    fn nearest_taxi_node_pos(&self) -> Option<BotPosition> {
        let mut out: BotPosition = unsafe { std::mem::zeroed() };
        let ok = unsafe { (self.cbs.nearest_taxi_node_pos.unwrap())(self.handle, &mut out) };
        if ok { Some(out) } else { None }
    }

    fn take_taxi_toward(&self, dest_map: u32, x: f32, y: f32, z: f32) -> bool {
        unsafe { (self.cbs.take_taxi_toward.unwrap())(self.handle, dest_map, x, y, z) }
    }

    fn cross_continent_travel(&self, dest_map: u32) -> (u8, Option<BotPosition>) {
        let mut dock: BotPosition = unsafe { std::mem::zeroed() };
        let code =
            unsafe { (self.cbs.cross_continent_travel.unwrap())(self.handle, dest_map, &mut dock) };
        let pos = if code == 4 { Some(dock) } else { None };
        (code, pos)
    }

    fn get_class_talents(&self, spec_no: u8) -> TalentList<'_> {
        let mut count: u32 = 0;
        let ptr =
            unsafe { (self.cbs.get_class_talents.unwrap())(self.handle, spec_no, &mut count) };
        unsafe { ffi_list(ptr, count, self.cbs.free_class_talents.unwrap()) }
    }

    fn bot_free_talent_points(&self) -> u32 {
        unsafe { (self.cbs.bot_free_talent_points.unwrap())(self.handle) }
    }

    fn bot_update_free_talent_points(&self) {
        unsafe { (self.cbs.bot_update_free_talent_points.unwrap())(self.handle) };
    }

    fn bot_pick_spec_no(&self, incremental: bool) -> u32 {
        unsafe { (self.cbs.bot_pick_spec_no.unwrap())(self.handle, incremental) }
    }

    fn bot_get_spec_tab(&self) -> u32 {
        unsafe { (self.cbs.bot_get_spec_tab.unwrap())(self.handle) }
    }

    /* ── Chat-command helpers (Wave 2) ───────────────────────────────── */

    fn bot_jump(&self) -> bool {
        unsafe { (self.cbs.bot_jump.unwrap())(self.handle) }
    }

    fn bot_use_hearthstone(&self) -> bool {
        unsafe { (self.cbs.bot_use_hearthstone.unwrap())(self.handle) }
    }

    fn bot_get_reputation_list(&self) -> ReputationList<'_> {
        let mut count: u32 = 0;
        let ptr = unsafe { (self.cbs.bot_get_reputation_list.unwrap())(self.handle, &mut count) };
        unsafe { ffi_list(ptr, count, self.cbs.bot_free_reputation_list.unwrap()) }
    }

    fn bot_get_learned_skills(&self) -> SkillList<'_> {
        let mut count: u32 = 0;
        let ptr = unsafe { (self.cbs.bot_get_learned_skills.unwrap())(self.handle, &mut count) };
        unsafe { ffi_list(ptr, count, self.cbs.bot_free_skill_list.unwrap()) }
    }

    fn bot_quest_accept_from(&self, npc: UnitHandle) -> bool {
        unsafe { (self.cbs.bot_quest_accept_from.unwrap())(self.handle, npc) }
    }

    fn bot_quest_abandon(&self, quest_id: u32) -> bool {
        unsafe { (self.cbs.bot_quest_abandon.unwrap())(self.handle, quest_id) }
    }

    /* ── Chat-command helpers (Wave 3: mail + guild) ─────────────────── */

    fn bot_mail_summary(&self) -> BotMailSummary {
        unsafe { (self.cbs.bot_mail_summary.unwrap())(self.handle) }
    }

    fn bot_mail_take_all(&self) -> bool {
        unsafe { (self.cbs.bot_mail_take_all.unwrap())(self.handle) }
    }

    fn bot_guild_leave(&self) -> bool {
        unsafe { (self.cbs.bot_guild_leave.unwrap())(self.handle) }
    }

    /* ── RTSC / file I/O helpers ─────────────────────────────────────── */

    fn bot_summon_marker_creature(
        &self,
        entry: u32,
        x: f32,
        y: f32,
        z: f32,
        o: f32,
        despawn_ms: u32,
        scale: f32,
    ) {
        unsafe {
            (self.cbs.bot_summon_marker_creature.unwrap())(
                self.handle,
                entry,
                x,
                y,
                z,
                o,
                despawn_ms,
                scale,
            );
        }
    }

    fn bot_write_log_file(&self, name: &str, body: &str) -> bool {
        let Ok(cname) = std::ffi::CString::new(name) else {
            return false;
        };
        let Ok(cbody) = std::ffi::CString::new(body) else {
            return false;
        };
        unsafe {
            (self.cbs.bot_write_log_file.unwrap())(self.handle, cname.as_ptr(), cbody.as_ptr())
        }
    }

    fn bot_append_log_file(&self, name: &str, line: &str) -> bool {
        let Ok(cname) = std::ffi::CString::new(name) else {
            return false;
        };
        let Ok(cline) = std::ffi::CString::new(line) else {
            return false;
        };
        unsafe {
            (self.cbs.bot_append_log_file.unwrap())(self.handle, cname.as_ptr(), cline.as_ptr())
        }
    }

    fn bot_read_log_file(&self, name: &str) -> Option<String> {
        let cname = std::ffi::CString::new(name).ok()?;
        let mut out_ptr: *mut std::os::raw::c_char = std::ptr::null_mut();
        let ok = unsafe {
            (self.cbs.bot_read_log_file.unwrap())(self.handle, cname.as_ptr(), &mut out_ptr)
        };
        if !ok || out_ptr.is_null() {
            return None;
        }
        let s = unsafe { std::ffi::CStr::from_ptr(out_ptr) }
            .to_string_lossy()
            .into_owned();
        unsafe { (self.cbs.bot_free_string.unwrap())(out_ptr) };
        Some(s)
    }

    fn get_pending_roll_count(&self) -> u32 {
        match self.cbs.get_pending_roll_count {
            Some(f) => unsafe { f(self.handle) },
            None => 0,
        }
    }

    fn auto_loot_roll(&self) -> bool {
        match self.cbs.auto_loot_roll {
            Some(f) => unsafe { f(self.handle) },
            None => false,
        }
    }

    fn cast_loot_roll(&self, vote: u8) -> bool {
        match self.cbs.cast_loot_roll {
            Some(f) => unsafe { f(self.handle, vote) },
            None => false,
        }
    }

    fn find_travel_dests(
        &self,
        purpose_flags: u32,
        max_range: f32,
        max_results: u32,
    ) -> TravelDestList<'_> {
        let mut count: u32 = 0;
        let ptr = unsafe {
            (self.cbs.bot_find_travel_dests.unwrap())(
                self.handle,
                purpose_flags,
                max_range,
                max_results,
                &mut count,
            )
        };
        unsafe { ffi_list(ptr, count, self.cbs.bot_free_travel_dests.unwrap()) }
    }

    fn add_aura(&self, spell_id: u32) -> bool {
        unsafe { (self.cbs.add_aura.unwrap())(self.handle, spell_id) }
    }

    fn get_needed_world_buffs(&self) -> Vec<u32> {
        let mut buf = [0u32; 64];
        let count = unsafe {
            (self.cbs.get_needed_world_buffs.unwrap())(
                self.handle,
                buf.as_mut_ptr(),
                buf.len() as u32,
            )
        };
        buf[..count as usize].to_vec()
    }

    fn interrupt_own_cast(&self) -> bool {
        unsafe { (self.cbs.interrupt_own_cast.unwrap())(self.handle) }
    }

    fn gossip_hello(&self, npc_entry: u32) -> bool {
        unsafe { (self.cbs.gossip_hello.unwrap())(self.handle, npc_entry) }
    }

    fn buy_from_vendor(&self, item_id: u32, qty: u32) -> bool {
        unsafe { (self.cbs.buy_from_vendor.unwrap())(self.handle, item_id, qty) }
    }

    fn mail_item_to_master(&self) -> bool {
        unsafe { (self.cbs.mail_item_to_master.unwrap())(self.handle) }
    }

    fn bank_deposit(&self) -> bool {
        unsafe { (self.cbs.bank_deposit.unwrap())(self.handle) }
    }

    fn bank_withdraw(&self) -> bool {
        unsafe { (self.cbs.bank_withdraw.unwrap())(self.handle) }
    }

    fn bot_get_inventory(&self) -> InventoryList<'_> {
        let mut count = 0u32;
        let ptr = unsafe { (self.cbs.bot_get_inventory.unwrap())(self.handle, &mut count) };
        unsafe { ffi_list(ptr, count, self.cbs.bot_free_inventory_list.unwrap()) }
    }

    fn bot_get_equipped(&self) -> InventoryList<'_> {
        let mut count = 0u32;
        let ptr = unsafe { (self.cbs.bot_get_equipped.unwrap())(self.handle, &mut count) };
        unsafe { ffi_list(ptr, count, self.cbs.bot_free_inventory_list.unwrap()) }
    }

    fn bot_get_bank_items(&self) -> InventoryList<'_> {
        let mut count = 0u32;
        let ptr = unsafe { (self.cbs.bot_get_bank_items.unwrap())(self.handle, &mut count) };
        unsafe { ffi_list(ptr, count, self.cbs.bot_free_inventory_list.unwrap()) }
    }

    fn bot_get_mail_items(&self) -> InventoryList<'_> {
        let mut count = 0u32;
        let ptr = unsafe { (self.cbs.bot_get_mail_items.unwrap())(self.handle, &mut count) };
        unsafe { ffi_list(ptr, count, self.cbs.bot_free_inventory_list.unwrap()) }
    }

    fn sell_item(&self, item_id: ItemId) -> bool {
        unsafe { (self.cbs.sell_item.unwrap())(self.handle, item_id.0) }
    }

    fn bank_deposit_item(&self, item_id: ItemId) -> bool {
        unsafe { (self.cbs.bank_deposit_item.unwrap())(self.handle, item_id.0) }
    }

    fn bank_withdraw_item(&self, item_id: ItemId) -> bool {
        unsafe { (self.cbs.bank_withdraw_item.unwrap())(self.handle, item_id.0) }
    }

    fn bot_mail_take_index(&self, mail_index: u32) -> bool {
        unsafe { (self.cbs.bot_mail_take_index.unwrap())(self.handle, mail_index) }
    }

    fn send_mail_item(&self, item_id: ItemId) -> bool {
        unsafe { (self.cbs.send_mail_item.unwrap())(self.handle, item_id.0) }
    }

    fn trade_add_item(&self, item_id: ItemId, count: u32) -> bool {
        unsafe { (self.cbs.trade_add_item.unwrap())(self.handle, item_id.0, count) }
    }

    fn get_spell_craft_item(&self, spell_id: u32) -> u32 {
        unsafe { (self.cbs.get_spell_craft_item.unwrap())(spell_id) }
    }

    fn get_item_info(&self, item_id: u32) -> Option<(String, u8)> {
        let mut name_buf = [0u8; 80];
        let mut quality = 0u8;
        let ok = unsafe {
            (self.cbs.get_item_info.unwrap())(
                item_id,
                name_buf.as_mut_ptr().cast::<core::ffi::c_char>(),
                name_buf.len() as u32,
                &mut quality,
            )
        };
        if !ok {
            return None;
        }
        let end = name_buf.iter().position(|&b| b == 0).unwrap_or(name_buf.len());
        let name = core::str::from_utf8(&name_buf[..end]).unwrap_or("?").to_string();
        Some((name, quality))
    }

    fn ah_post(&self) -> bool {
        unsafe { (self.cbs.ah_post.unwrap())(self.handle) }
    }

    fn ah_bid(&self) -> bool {
        unsafe { (self.cbs.ah_bid.unwrap())(self.handle) }
    }

    fn start_fishing(&self) -> bool {
        unsafe { (self.cbs.start_fishing.unwrap())(self.handle) }
    }

    fn queue_bg(&self) -> bool {
        unsafe { (self.cbs.queue_bg.unwrap())(self.handle) }
    }

    fn accept_bg_invite(&self) -> bool {
        unsafe { (self.cbs.accept_bg_invite.unwrap())(self.handle) }
    }

    fn get_bg_objective_pos(&self, objective_type: u8) -> BotPosition {
        unsafe { (self.cbs.get_bg_objective_pos.unwrap())(self.handle, objective_type) }
    }

    fn lfg_join(&self) -> bool {
        unsafe { (self.cbs.lfg_join.unwrap())(self.handle) }
    }

    fn lfg_accept(&self) -> bool {
        unsafe { (self.cbs.lfg_accept.unwrap())(self.handle) }
    }

    fn get_tank_position(&self) -> BotPosition {
        unsafe { (self.cbs.get_tank_position.unwrap())(self.handle) }
    }

    fn is_unit_cc(&self, target: UnitHandle) -> bool {
        unsafe { (self.cbs.is_unit_cc.unwrap())(self.handle, target) }
    }

    fn debug_dump_state(&self, kind: u8) -> bool {
        unsafe { (self.cbs.debug_dump_state.unwrap())(self.handle, kind) }
    }
}
