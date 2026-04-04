/// BotInterface — the Rust abstraction over the C BotCallbacks vtable.
///
/// Production code uses RealInterface (wraps the C function pointer table).
/// Tests use MockInterface (in-memory mock that records all commands issued).
///
/// BtNode and TickContext use `&dyn BotInterface` so they work in both contexts
/// without any conditional compilation.
use super::{
    BotAuraInfo, BotCallbacks, BotHandle, BotPosition, BotSafePosition, BotThreatEntry,
    BotUnitSnapshot, BotWorldSnapshot, UnitHandle,
    types::{BotRole, SpellId, ItemId},
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
    fn get_spread_position(&self, center: UnitHandle, radius: f32, idx: u8, total: u8) -> BotPosition;
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
    fn use_item(&self, item_id: ItemId, target: UnitHandle) -> bool;
    fn taunt(&self, target: UnitHandle) -> bool;

    /* ── Group / raid ────────────────────────────────────────────────── */

    fn group_get_tank(&self) -> Option<UnitHandle>;
    fn group_get_healer(&self) -> Option<UnitHandle>;
    fn group_get_role(&self, member: UnitHandle) -> BotRole;
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
        if ptr.is_null() || count == 0 { return Vec::new(); }
        let vec = unsafe { std::slice::from_raw_parts(ptr, count as usize).to_vec() };
        unsafe { (self.cbs.free_aura_list.unwrap())(ptr) };
        vec
    }

    fn get_threat_list(&self, target_unit: UnitHandle) -> Vec<BotThreatEntry> {
        let mut count: u32 = 0;
        let ptr = unsafe { (self.cbs.get_threat_list.unwrap())(self.handle, target_unit, &mut count) };
        if ptr.is_null() || count == 0 { return Vec::new(); }
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
        let ptr = unsafe { (self.cbs.get_nearby_units.unwrap())(self.handle, range, hostile, &mut count) };
        if ptr.is_null() || count == 0 { return Vec::new(); }
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
            Some(BotPosition { x: result.x, y: result.y, z: result.z, o: 0.0, map_id: 0 })
        } else {
            None
        }
    }

    fn get_spread_position(&self, center: UnitHandle, radius: f32, idx: u8, total: u8) -> BotPosition {
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
}
