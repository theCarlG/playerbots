//! `extern "C"` FFI surface for [`super`].
//!
//! All functions in this module are called from the C++ compat shim
//! at `cpp_wrapper/BotConfig.{h,cpp}`. They provide:
//!
//! 1. Lifecycle: [`playerbot_config_load`] parses a file and installs
//!    the resulting state as the global config.
//! 2. By-key scalar getters: `playerbot_config_get_bool` / `_u32` / `_i32`
//!    / `_f32` / `_string_dup`. These consult the stored `RawConfig`
//!    and apply the default passed in by the C++ caller — matching the
//!    `Config::GetBoolDefault` / `GetIntDefault` / `GetStringDefault`
//!    semantics the old C++ `Initialize()` used.
//! 3. By-key list getters: `_get_u32_list_dup` / `_get_string_list_dup`
//!    return ownership of a freshly-allocated array that the caller
//!    must free via the paired `_free` functions.
//! 4. Typed accessors for the pre-derived array fields: class/race
//!    probability, level probability, gear progression tables, world
//!    buffs, login criteria. The derivation logic (default → override
//!    → specific) lives in [`super::typed::BotConfig::from_raw`] so the
//!    C++ shim just pulls already-computed values.
//! 5. Dynamic-key iteration: [`playerbot_config_keys_containing`] mirrors
//!    `CMaNGOS` `ConfigAccess::GetValues` (substring match against stored
//!    keys, sorted output). Used by the shim to enumerate
//!    `AiPlayerbot.WorldBuff*` / `AiPlayerbot.LoginCriteria*` dynamic
//!    keys — although both of those are now also available via the
//!    pre-derived accessors, the raw iteration is kept for flexibility
//!    and tests.
//!
//! All functions are `#[no_mangle] extern "C"` and the whole module
//! opts in to `unsafe_code` because of the C pointer bookkeeping.
//! Safe-Rust-only crate-level linting resumes outside this file.

#![allow(unsafe_code)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::Path;
use std::ptr;

use crate::log_error;

use super::raw::RawConfig;
use super::typed::{MAX_CLASSES, MAX_GEAR_PROGRESSION_LEVEL, MAX_LEVEL_PROBABILITY_SLOTS, MAX_RACES};
use super::{ConfigState, install, state};

// ── Helpers ──────────────────────────────────────────────────────────────

/// Convert a possibly-null C string to a borrowed `&str`, returning
/// `None` on null / invalid UTF-8.
unsafe fn cstr_to_str<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().ok()
}

/// Allocate a fresh owned C string for hand-off to the caller. Returns
/// null on embedded-NUL (should never happen for config values but
/// defensive).
fn into_owned_cstr(s: &str) -> *mut c_char {
    match CString::new(s) {
        Ok(c) => c.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

/// Reclaim an owned-by-Rust `Box<[T]>` from raw parts and drop it.
///
/// # Safety
/// `ptr` must have been produced by a `Box::into_raw` that returned
/// exactly `len` elements. Called from the matching `_free_*` functions.
unsafe fn drop_boxed_slice<T>(ptr: *mut T, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    let slice = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
    let boxed: Box<[T]> = unsafe { Box::from_raw(slice as *mut [T]) };
    drop(boxed);
}

// ── Lifecycle ────────────────────────────────────────────────────────────

/// Parse a `.conf` file and install the result as the global config.
///
/// After this call succeeds, subsequent scalar / list / typed getters
/// observe the new state atomically. Environment-variable overrides
/// (prefixed with `PlayerBots_`) are applied on top of the file.
///
/// Returns `true` on success, `false` on parse or I/O failure. On
/// failure, the existing global state is left untouched.
///
/// # Safety
/// `path` must be null or a valid NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_config_load(path: *const c_char) -> bool {
    let Some(path_str) = (unsafe { cstr_to_str(path) }) else {
        log_error!("playerbot_config_load: null or invalid path");
        return false;
    };
    let mut raw = match RawConfig::parse_file(Path::new(path_str)) {
        Ok(r) => r,
        Err(e) => {
            log_error!("playerbot_config_load: failed to read '{path_str}': {e}");
            return false;
        }
    };
    raw.apply_env_overrides("PlayerBots_");
    install(ConfigState::from_raw(raw));
    true
}

/// Install an empty config with only defaults. Useful for tests that
/// run in-process without calling `playerbot_config_load`.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_config_reset_to_defaults() {
    install(ConfigState::default());
}

// ── Scalar getters ───────────────────────────────────────────────────────

/// Look up a boolean config key.
///
/// # Safety
/// `key` must be null or a valid NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_config_get_bool(key: *const c_char, default: bool) -> bool {
    let Some(k) = (unsafe { cstr_to_str(key) }) else {
        return default;
    };
    state().load().raw.get_bool(k, default)
}

/// Look up a `u32` config key.
///
/// # Safety
/// `key` must be null or a valid NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_config_get_u32(key: *const c_char, default: u32) -> u32 {
    let Some(k) = (unsafe { cstr_to_str(key) }) else {
        return default;
    };
    state().load().raw.get_u32(k, default)
}

/// Look up an `i32` config key.
///
/// # Safety
/// `key` must be null or a valid NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_config_get_i32(key: *const c_char, default: i32) -> i32 {
    let Some(k) = (unsafe { cstr_to_str(key) }) else {
        return default;
    };
    state().load().raw.get_i32(k, default)
}

/// Look up an `f32` config key.
///
/// # Safety
/// `key` must be null or a valid NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_config_get_f32(key: *const c_char, default: f32) -> f32 {
    let Some(k) = (unsafe { cstr_to_str(key) }) else {
        return default;
    };
    state().load().raw.get_f32(k, default)
}

/// Look up a string config key and return a fresh owned C string.
///
/// The returned pointer must be freed via [`playerbot_config_free_cstr`].
/// On null-key or missing-default, a copy of `default` is returned
/// (so the caller always owns the result and can free it uniformly).
/// If `default` is also null, the empty string is returned.
///
/// # Safety
/// Both `key` and `default` must be null or valid NUL-terminated UTF-8
/// C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_config_get_string_dup(
    key: *const c_char,
    default: *const c_char,
) -> *mut c_char {
    let default_str = unsafe { cstr_to_str(default) }.unwrap_or("");
    let Some(k) = (unsafe { cstr_to_str(key) }) else {
        return into_owned_cstr(default_str);
    };
    let guard = state().load();
    let value = guard.raw.get_string(k, default_str);
    into_owned_cstr(value)
}

/// Free a C string previously returned by any `*_dup` getter.
///
/// # Safety
/// `ptr` must be null or a pointer returned by one of this module's
/// `_dup` getters.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_config_free_cstr(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    // Reclaim ownership so the CString's allocation is freed.
    drop(unsafe { CString::from_raw(ptr) });
}

// ── List getters ─────────────────────────────────────────────────────────

/// Look up a CSV u32 list, writing the element pointer and length into
/// the out-params. Returns `true` if the call succeeded (the key was
/// present and produced zero or more valid entries) and `false` on
/// invalid parameters. Empty lists still set `*out_len = 0` and
/// `*out_ptr = null`.
///
/// Always pair with [`playerbot_config_free_u32_list`].
///
/// # Safety
/// `key` must be a valid UTF-8 C string. `out_ptr` and `out_len` must
/// point to writable storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_config_get_u32_list_dup(
    key: *const c_char,
    out_ptr: *mut *mut u32,
    out_len: *mut usize,
) -> bool {
    if out_ptr.is_null() || out_len.is_null() {
        return false;
    }
    unsafe {
        *out_ptr = ptr::null_mut();
        *out_len = 0;
    }
    let Some(k) = (unsafe { cstr_to_str(key) }) else {
        return false;
    };
    let list = state().load().raw.get_u32_list(k);
    if list.is_empty() {
        return true;
    }
    let boxed: Box<[u32]> = list.into_boxed_slice();
    let len = boxed.len();
    let raw = Box::into_raw(boxed);
    unsafe {
        *out_ptr = raw.cast::<u32>();
        *out_len = len;
    }
    true
}

/// Free a u32 list previously returned by [`playerbot_config_get_u32_list_dup`].
///
/// # Safety
/// `ptr` and `len` must come from a prior successful call to the
/// matching `_dup` function. Must not be called twice on the same pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_config_free_u32_list(ptr: *mut u32, len: usize) {
    unsafe { drop_boxed_slice(ptr, len) };
}

/// Look up a CSV string list, writing an array of C string pointers
/// into the out-params. Each element is an independently-owned C string
/// allocation — free the whole thing via
/// [`playerbot_config_free_string_list`], which calls
/// [`playerbot_config_free_cstr`] on each entry before releasing the
/// outer array.
///
/// # Safety
/// `key` must be a valid UTF-8 C string. `out_ptr` and `out_len` must
/// point to writable storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_config_get_string_list_dup(
    key: *const c_char,
    out_ptr: *mut *mut *mut c_char,
    out_len: *mut usize,
) -> bool {
    if out_ptr.is_null() || out_len.is_null() {
        return false;
    }
    unsafe {
        *out_ptr = ptr::null_mut();
        *out_len = 0;
    }
    let Some(k) = (unsafe { cstr_to_str(key) }) else {
        return false;
    };
    let list = state().load().raw.get_string_list(k);
    write_cstr_array(list, out_ptr, out_len)
}

/// Free an array of C strings previously returned by
/// [`playerbot_config_get_string_list_dup`] or
/// [`playerbot_config_keys_containing`].
///
/// # Safety
/// `ptr` / `len` must come from a prior successful call to a matching
/// `_dup` / keys function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_config_free_string_list(ptr: *mut *mut c_char, len: usize) {
    if ptr.is_null() {
        return;
    }
    let slice = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
    for entry in slice.iter_mut() {
        if !entry.is_null() {
            drop(unsafe { CString::from_raw(*entry) });
            *entry = ptr::null_mut();
        }
    }
    unsafe { drop_boxed_slice(ptr, len) };
}

// ── Dynamic key iteration ────────────────────────────────────────────────

/// Enumerate all config keys that contain `needle` as a substring
/// (case-insensitive, sorted). Matches the semantics of the `CMaNGOS`
/// `ConfigAccess::GetValues(name)` helper the old `.cpp` used.
///
/// Each element is an owned C string; free the array via
/// [`playerbot_config_free_string_list`].
///
/// # Safety
/// `needle` must be a valid UTF-8 C string. `out_ptr` and `out_len`
/// must point to writable storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_config_keys_containing(
    needle: *const c_char,
    out_ptr: *mut *mut *mut c_char,
    out_len: *mut usize,
) -> bool {
    if out_ptr.is_null() || out_len.is_null() {
        return false;
    }
    unsafe {
        *out_ptr = ptr::null_mut();
        *out_len = 0;
    }
    let Some(n) = (unsafe { cstr_to_str(needle) }) else {
        return false;
    };
    let keys = state().load().raw.keys_containing(n);
    write_cstr_array(keys, out_ptr, out_len)
}

// ── Typed accessors ──────────────────────────────────────────────────────

/// Read a cell from the class/race probability matrix that was derived
/// by `BotConfig::from_raw`. Returns 0 for out-of-range indices.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_config_get_class_race_probability(cls: u32, race: u32) -> u32 {
    let c = cls as usize;
    let r = race as usize;
    if c >= MAX_CLASSES || r >= MAX_RACES {
        return 0;
    }
    state().load().typed.class_race_probability[c][r]
}

/// Read the pre-computed sum of all class/race probability cells.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_config_get_class_race_probability_total() -> u32 {
    state().load().typed.class_race_probability_total
}

/// Named typed config getters — the `BotConfig.{h,cpp}` C++ mirror is
/// being deleted (Phase M); every C++ `sPlayerbotAIConfig.<field>` reader
/// migrates to one of these instead of a string-keyed lookup, honouring
/// the "no string-keyed dispatch at the boundary" rule. Each returns the
/// live value off the typed config so `/bot reload` is reflected.
macro_rules! config_scalar_getter {
    ($name:ident, $ret:ty, $field:ident) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name() -> $ret {
            state().load().typed.$field
        }
    };
}

config_scalar_getter!(playerbot_config_react_delay, u32, react_delay);
config_scalar_getter!(playerbot_config_max_wait_for_move, u32, max_wait_for_move);
config_scalar_getter!(playerbot_config_level_check, i32, level_check);
config_scalar_getter!(
    playerbot_config_min_random_bot_in_world_time,
    u32,
    min_random_bot_in_world_time
);
config_scalar_getter!(
    playerbot_config_max_random_bot_in_world_time,
    u32,
    max_random_bot_in_world_time
);
config_scalar_getter!(
    playerbot_config_random_bot_timed_logout,
    bool,
    random_bot_timed_logout
);
config_scalar_getter!(
    playerbot_config_random_bot_timed_offline,
    bool,
    random_bot_timed_offline
);
config_scalar_getter!(playerbot_config_enabled, bool, enabled);
config_scalar_getter!(playerbot_config_random_bot_max_level, u32, random_bot_max_level);
config_scalar_getter!(playerbot_config_random_bot_min_level, u32, random_bot_min_level);
config_scalar_getter!(playerbot_config_grind_distance, f32, grind_distance);
config_scalar_getter!(playerbot_config_sync_level_max_above, u32, sync_level_max_above);
config_scalar_getter!(playerbot_config_sync_level_with_players, bool, sync_level_with_players);
config_scalar_getter!(playerbot_config_randombot_starting_level, u32, randombot_starting_level);
config_scalar_getter!(playerbot_config_random_gear_upgrade_enabled, bool, random_gear_upgrade_enabled);
config_scalar_getter!(playerbot_config_random_bot_update_interval, u32, random_bot_update_interval);
config_scalar_getter!(playerbot_config_random_bot_autologin, bool, random_bot_autologin);
config_scalar_getter!(playerbot_config_disable_random_levels, bool, disable_random_levels);
config_scalar_getter!(playerbot_config_random_bot_tele_level, u32, random_bot_tele_level);
config_scalar_getter!(playerbot_config_random_bot_rpg_chance, f32, random_bot_rpg_chance);
config_scalar_getter!(playerbot_config_random_bot_max_level_chance, f32, random_bot_max_level_chance);
config_scalar_getter!(playerbot_config_min_random_bot_randomize_time, u32, min_random_bot_randomize_time);
config_scalar_getter!(playerbot_config_max_random_bot_randomize_time, u32, max_random_bot_randomize_time);
config_scalar_getter!(playerbot_config_min_random_bot_revive_time, u32, min_random_bot_revive_time);
config_scalar_getter!(playerbot_config_max_random_bot_revive_time, u32, max_random_bot_revive_time);
config_scalar_getter!(playerbot_config_random_bot_teleport_near_player, bool, random_bot_teleport_near_player);
config_scalar_getter!(
    playerbot_config_random_bot_teleport_near_player_max_amount,
    u32,
    random_bot_teleport_near_player_max_amount
);
config_scalar_getter!(
    playerbot_config_random_bot_teleport_near_player_max_amount_radius,
    f32,
    random_bot_teleport_near_player_max_amount_radius
);
config_scalar_getter!(
    playerbot_config_random_bot_teleport_min_interval,
    u32,
    random_bot_teleport_min_interval
);
config_scalar_getter!(
    playerbot_config_random_bot_teleport_max_interval,
    u32,
    random_bot_teleport_max_interval
);
// self_bot_level / bot_autologin are stored as u32 (the C++ side casts to
// its BotSelfBotLevel / BotAutoLogin enums).
config_scalar_getter!(playerbot_config_self_bot_level, u32, self_bot_level);
config_scalar_getter!(playerbot_config_bot_autologin, u32, bot_autologin);
config_scalar_getter!(playerbot_config_random_gear_progression, bool, random_gear_progression);
config_scalar_getter!(playerbot_config_instant_randomize, bool, instant_randomize);
config_scalar_getter!(playerbot_config_allow_multi_account_alt_bots, bool, allow_multi_account_alt_bots);
config_scalar_getter!(playerbot_config_allow_guild_bots, bool, allow_guild_bots);
config_scalar_getter!(playerbot_config_min_enchanting_bot_level, u32, min_enchanting_bot_level);
config_scalar_getter!(playerbot_config_follow_distance, f32, follow_distance);
config_scalar_getter!(playerbot_config_rnd_bot_cheat_mask, u32, rnd_bot_cheat_mask);
config_scalar_getter!(playerbot_config_bot_cheat_mask, u32, bot_cheat_mask);
config_scalar_getter!(playerbot_config_random_bot_show_helmet, bool, random_bot_show_helmet);
config_scalar_getter!(playerbot_config_random_bot_show_cloak, bool, random_bot_show_cloak);
config_scalar_getter!(playerbot_config_tweak_value, u32, tweak_value);

/// Named typed `String` config getters — return a freshly allocated C
/// string (free with `playerbot_config_free_cstr`).
macro_rules! config_string_getter {
    ($name:ident, $field:ident) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name() -> *mut c_char {
            into_owned_cstr(&state().load().typed.$field)
        }
    };
}

config_string_getter!(playerbot_config_command_separator, command_separator);
config_string_getter!(playerbot_config_random_bot_maps_as_string, random_bot_maps_as_string);

/// `specProbability[cls][idx]` — spec-pick probability cell. 0 for
/// out-of-range indices.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_config_spec_probability(cls: u32, idx: u32) -> u32 {
    let guard = state().load();
    guard
        .typed
        .spec_probability
        .get(cls as usize)
        .and_then(|row| row.get(idx as usize))
        .copied()
        .unwrap_or(0)
}

/// Membership test for `randomBotMaps` (the maps bots may be teleported to).
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_config_is_random_bot_map(map_id: u32) -> bool {
    state().load().typed.random_bot_maps.contains(&map_id)
}

/// `randomBotSpellIds` list — length + per-index accessor (mirrors the
/// world-buffs accessor style; no allocation crosses the boundary).
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_config_random_bot_spell_ids_len() -> usize {
    state().load().typed.random_bot_spell_ids.len()
}

#[unsafe(no_mangle)]
pub extern "C" fn playerbot_config_random_bot_spell_ids_at(idx: usize) -> u32 {
    state()
        .load()
        .typed
        .random_bot_spell_ids
        .get(idx)
        .copied()
        .unwrap_or(0)
}

/// Read a fixed class/race count. Returns `-1` if `use_fixed_class_race_counts`
/// is disabled or the entry is absent; otherwise the stored count.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_config_get_fixed_class_race_count(cls: u32, race: u32) -> i64 {
    let guard = state().load();
    if !guard.typed.use_fixed_class_race_counts {
        return -1;
    }
    guard
        .typed
        .fixed_class_race_counts
        .get(&(cls as u8, race as u8))
        .map_or(-1, |v| i64::from(*v))
}

/// Read a per-level spawn probability (`AiPlayerbot.LevelProbability.N`).
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_config_get_level_probability(level: u32) -> u32 {
    let idx = level as usize;
    if idx >= MAX_LEVEL_PROBABILITY_SLOTS {
        return 0;
    }
    state().load().typed.level_probability[idx]
}

/// Read gear progression min-item-level for a phase.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_config_get_gear_min_item_level(phase: u32) -> u32 {
    let p = phase as usize;
    if p >= MAX_GEAR_PROGRESSION_LEVEL {
        return 0;
    }
    state().load().typed.gear_progression_system_item_levels[p][0]
}

/// Read gear progression max-item-level for a phase.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_config_get_gear_max_item_level(phase: u32) -> u32 {
    let p = phase as usize;
    if p >= MAX_GEAR_PROGRESSION_LEVEL {
        return 0;
    }
    state().load().typed.gear_progression_system_item_levels[p][1]
}

/// Read a gear progression item id. Returns `-1` for out-of-range
/// indices or unset entries.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_config_get_gear_item(
    phase: u32,
    cls: u32,
    spec: u32,
    slot: u32,
) -> i32 {
    let p = phase as usize;
    let c = cls as usize;
    let s = spec as usize;
    let sl = slot as usize;
    if p >= MAX_GEAR_PROGRESSION_LEVEL || c >= MAX_CLASSES || s >= 4 || sl >= 19 {
        return -1;
    }
    state().load().typed.gear_progression_system_items[p][c][s][sl]
}

/// Number of world-buff entries derived by the parser.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_config_world_buffs_len() -> usize {
    state().load().typed.world_buffs.len()
}

/// Copy the `idx`th world-buff entry into the caller-provided out-params.
/// Returns `true` on success, `false` if `idx` is out of range or any
/// out-param pointer is null.
///
/// # Safety
/// All out-param pointers must point to writable `u32` storage.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn playerbot_config_world_buff_at(
    idx: usize,
    out_spell_id: *mut u32,
    out_faction_id: *mut u32,
    out_class_id: *mut u32,
    out_spec_id: *mut u32,
    out_min_level: *mut u32,
    out_max_level: *mut u32,
    out_event_id: *mut u32,
) -> bool {
    if out_spell_id.is_null()
        || out_faction_id.is_null()
        || out_class_id.is_null()
        || out_spec_id.is_null()
        || out_min_level.is_null()
        || out_max_level.is_null()
        || out_event_id.is_null()
    {
        return false;
    }
    let guard = state().load();
    let Some(buff) = guard.typed.world_buffs.get(idx) else {
        return false;
    };
    unsafe {
        *out_spell_id = buff.spell_id;
        *out_faction_id = buff.faction_id;
        *out_class_id = buff.class_id;
        *out_spec_id = buff.spec_id;
        *out_min_level = buff.min_level;
        *out_max_level = buff.max_level;
        *out_event_id = buff.event_id;
    }
    true
}

/// Number of login-criteria entries.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_config_login_criteria_len() -> usize {
    state().load().typed.login_criteria.len()
}

/// Copy the `idx`th login-criteria entry into a freshly-allocated
/// string-array. Paired with [`playerbot_config_free_string_list`].
///
/// # Safety
/// `out_ptr` and `out_len` must point to writable storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_config_login_criteria_at(
    idx: usize,
    out_ptr: *mut *mut *mut c_char,
    out_len: *mut usize,
) -> bool {
    if out_ptr.is_null() || out_len.is_null() {
        return false;
    }
    unsafe {
        *out_ptr = ptr::null_mut();
        *out_len = 0;
    }
    let guard = state().load();
    let Some(entry) = guard.typed.login_criteria.get(idx) else {
        return false;
    };
    write_cstr_array(entry.clone(), out_ptr, out_len)
}

/// Number of entries in the default login-criteria list (used when
/// no `AiPlayerbot.LoginCriteria*` keys were supplied in the file).
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_config_default_login_criteria_len() -> usize {
    state().load().typed.default_login_criteria.len()
}

/// Copy the default login-criteria list into a freshly-allocated
/// string-array. Paired with [`playerbot_config_free_string_list`].
///
/// # Safety
/// `out_ptr` and `out_len` must point to writable storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_config_default_login_criteria_dup(
    out_ptr: *mut *mut *mut c_char,
    out_len: *mut usize,
) -> bool {
    if out_ptr.is_null() || out_len.is_null() {
        return false;
    }
    unsafe {
        *out_ptr = ptr::null_mut();
        *out_len = 0;
    }
    let guard = state().load();
    write_cstr_array(guard.typed.default_login_criteria.clone(), out_ptr, out_len)
}

// ── Internals ────────────────────────────────────────────────────────────

/// Turn an owned `Vec<String>` into a heap-allocated `char**` + length,
/// compatible with [`playerbot_config_free_string_list`].
fn write_cstr_array(
    entries: Vec<String>,
    out_ptr: *mut *mut *mut c_char,
    out_len: *mut usize,
) -> bool {
    if entries.is_empty() {
        // Keep out_ptr null / out_len 0 — signals "valid empty list".
        return true;
    }
    let mut owned: Vec<*mut c_char> = Vec::with_capacity(entries.len());
    for entry in entries {
        let raw = into_owned_cstr(&entry);
        if raw.is_null() {
            // On failure, free anything we allocated so far and abort.
            for p in owned {
                if !p.is_null() {
                    drop(unsafe { CString::from_raw(p) });
                }
            }
            return false;
        }
        owned.push(raw);
    }
    let boxed: Box<[*mut c_char]> = owned.into_boxed_slice();
    let len = boxed.len();
    let raw = Box::into_raw(boxed);
    unsafe {
        *out_ptr = raw.cast::<*mut c_char>();
        *out_len = len;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    // Share the lock with the sibling `mod.rs` tests so parallel test
    // runs don't clobber each other's `install` calls.
    use super::super::TEST_LOCK;

    fn install_from_str(content: &str) {
        let raw = RawConfig::parse_str(content);
        install(ConfigState::from_raw(raw));
    }

    fn reset() {
        install(ConfigState::default());
    }

    #[test]
    fn scalar_getters_round_trip() {
        let _g = TEST_LOCK.lock().unwrap();
        install_from_str(
            "AiPlayerbot.Enabled = 1\n\
             AiPlayerbot.ReactDelay = 250\n\
             AiPlayerbot.Pi = 3.14\n\
             AiPlayerbot.Signed = -42\n",
        );

        let key = CString::new("AiPlayerbot.Enabled").unwrap();
        assert!(unsafe { playerbot_config_get_bool(key.as_ptr(), false) });

        let key = CString::new("AiPlayerbot.ReactDelay").unwrap();
        assert_eq!(unsafe { playerbot_config_get_u32(key.as_ptr(), 0) }, 250);

        let key = CString::new("AiPlayerbot.Signed").unwrap();
        assert_eq!(unsafe { playerbot_config_get_i32(key.as_ptr(), 0) }, -42);

        let key = CString::new("AiPlayerbot.Pi").unwrap();
        let pi = unsafe { playerbot_config_get_f32(key.as_ptr(), 0.0) };
        assert!((pi - 3.14).abs() < 0.001);

        reset();
    }

    #[test]
    fn missing_scalar_returns_default() {
        let _g = TEST_LOCK.lock().unwrap();
        reset();
        let key = CString::new("does.not.exist").unwrap();
        assert!(unsafe { playerbot_config_get_bool(key.as_ptr(), true) });
        assert_eq!(unsafe { playerbot_config_get_u32(key.as_ptr(), 99) }, 99);
        assert_eq!(unsafe { playerbot_config_get_i32(key.as_ptr(), -1) }, -1);
        assert!((unsafe { playerbot_config_get_f32(key.as_ptr(), 1.5) } - 1.5).abs() < 0.0001);
    }

    #[test]
    fn null_key_returns_default() {
        let _g = TEST_LOCK.lock().unwrap();
        assert!(unsafe { playerbot_config_get_bool(ptr::null(), true) });
        assert_eq!(unsafe { playerbot_config_get_u32(ptr::null(), 42) }, 42);
    }

    #[test]
    fn string_dup_round_trips() {
        let _g = TEST_LOCK.lock().unwrap();
        install_from_str(r#"AiPlayerbot.Prefix = "hello there""#);
        let key = CString::new("AiPlayerbot.Prefix").unwrap();
        let default = CString::new("fallback").unwrap();
        let dup = unsafe { playerbot_config_get_string_dup(key.as_ptr(), default.as_ptr()) };
        assert!(!dup.is_null());
        let cs = unsafe { CStr::from_ptr(dup) };
        assert_eq!(cs.to_str().unwrap(), "hello there");
        unsafe { playerbot_config_free_cstr(dup) };
        reset();
    }

    #[test]
    fn string_dup_default_path() {
        let _g = TEST_LOCK.lock().unwrap();
        reset();
        let key = CString::new("missing").unwrap();
        let default = CString::new("fallback").unwrap();
        let dup = unsafe { playerbot_config_get_string_dup(key.as_ptr(), default.as_ptr()) };
        assert_eq!(
            unsafe { CStr::from_ptr(dup) }.to_str().unwrap(),
            "fallback"
        );
        unsafe { playerbot_config_free_cstr(dup) };
    }

    #[test]
    fn u32_list_dup_and_free() {
        let _g = TEST_LOCK.lock().unwrap();
        install_from_str("AiPlayerbot.List = 1,2,3,42");
        let key = CString::new("AiPlayerbot.List").unwrap();
        let mut out_ptr: *mut u32 = ptr::null_mut();
        let mut out_len: usize = 0;
        let ok = unsafe {
            playerbot_config_get_u32_list_dup(key.as_ptr(), &mut out_ptr, &mut out_len)
        };
        assert!(ok);
        assert_eq!(out_len, 4);
        let slice = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(slice, &[1, 2, 3, 42]);
        unsafe { playerbot_config_free_u32_list(out_ptr, out_len) };
        reset();
    }

    #[test]
    fn empty_u32_list_returns_valid_empty() {
        let _g = TEST_LOCK.lock().unwrap();
        reset();
        let key = CString::new("missing").unwrap();
        let mut out_ptr: *mut u32 = ptr::null_mut();
        let mut out_len: usize = 1234;
        let ok = unsafe {
            playerbot_config_get_u32_list_dup(key.as_ptr(), &mut out_ptr, &mut out_len)
        };
        assert!(ok);
        assert!(out_ptr.is_null());
        assert_eq!(out_len, 0);
    }

    #[test]
    fn string_list_dup_and_free() {
        let _g = TEST_LOCK.lock().unwrap();
        install_from_str("AiPlayerbot.Names = alice,bob,carol");
        let key = CString::new("AiPlayerbot.Names").unwrap();
        let mut out_ptr: *mut *mut c_char = ptr::null_mut();
        let mut out_len: usize = 0;
        let ok = unsafe {
            playerbot_config_get_string_list_dup(key.as_ptr(), &mut out_ptr, &mut out_len)
        };
        assert!(ok);
        assert_eq!(out_len, 3);
        let slice = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        let got: Vec<String> = slice
            .iter()
            .map(|p| unsafe { CStr::from_ptr(*p) }.to_string_lossy().into_owned())
            .collect();
        assert_eq!(got, vec!["alice", "bob", "carol"]);
        unsafe { playerbot_config_free_string_list(out_ptr, out_len) };
        reset();
    }

    #[test]
    fn keys_containing_dynamic_iteration() {
        let _g = TEST_LOCK.lock().unwrap();
        install_from_str(
            "AiPlayerbot.WorldBuff.1.0.0 = 100\n\
             AiPlayerbot.WorldBuff.2.1.0 = 200\n\
             AiPlayerbot.Other.Key = 300\n",
        );
        let needle = CString::new("AiPlayerbot.WorldBuff").unwrap();
        let mut out_ptr: *mut *mut c_char = ptr::null_mut();
        let mut out_len: usize = 0;
        let ok = unsafe {
            playerbot_config_keys_containing(needle.as_ptr(), &mut out_ptr, &mut out_len)
        };
        assert!(ok);
        assert_eq!(out_len, 2);
        let slice = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        let keys: Vec<String> = slice
            .iter()
            .map(|p| unsafe { CStr::from_ptr(*p) }.to_string_lossy().into_owned())
            .collect();
        assert!(keys[0].contains("worldbuff"));
        assert!(keys[1].contains("worldbuff"));
        unsafe { playerbot_config_free_string_list(out_ptr, out_len) };
        reset();
    }

    #[test]
    fn typed_accessors_use_derived_fields() {
        let _g = TEST_LOCK.lock().unwrap();
        install_from_str(
            "AiPlayerbot.LevelProbability.5 = 42\n\
             AiPlayerbot.GearProgressionSystem.0.MinItemLevel = 50\n\
             AiPlayerbot.GearProgressionSystem.0.MaxItemLevel = 100\n\
             AiPlayerbot.GearProgressionSystem.0.1.0.0 = 12345\n\
             AiPlayerbot.ClassRaceProb.0.1 = 80\n",
        );
        assert_eq!(playerbot_config_get_level_probability(5), 42);
        assert_eq!(playerbot_config_get_gear_min_item_level(0), 50);
        assert_eq!(playerbot_config_get_gear_max_item_level(0), 100);
        assert_eq!(playerbot_config_get_gear_item(0, 1, 0, 0), 12345);
        assert_eq!(playerbot_config_get_gear_item(0, 1, 0, 1), -1);
        assert_eq!(playerbot_config_get_class_race_probability(1, 1), 80);
        assert_eq!(playerbot_config_get_class_race_probability(99, 99), 0);
        reset();
    }

    #[test]
    fn world_buff_accessors() {
        let _g = TEST_LOCK.lock().unwrap();
        install_from_str("AiPlayerbot.WorldBuff.1.0.0 = 12345,67890");
        let len = playerbot_config_world_buffs_len();
        assert_eq!(len, 2);
        let (mut spell, mut faction, mut class, mut spec, mut minl, mut maxl, mut ev) =
            (0u32, 0u32, 0u32, 0u32, 0u32, 0u32, 0u32);
        let ok = unsafe {
            playerbot_config_world_buff_at(
                0,
                &mut spell,
                &mut faction,
                &mut class,
                &mut spec,
                &mut minl,
                &mut maxl,
                &mut ev,
            )
        };
        assert!(ok);
        assert!(spell == 12345 || spell == 67890);
        assert_eq!(faction, 1);
        reset();
    }

    #[test]
    fn config_load_from_file() {
        let _g = TEST_LOCK.lock().unwrap();
        let dir = std::env::temp_dir();
        let file = dir.join("playerbot_config_ffi_test.conf");
        std::fs::write(
            &file,
            "AiPlayerbot.Enabled = 1\nAiPlayerbot.ReactDelay = 333\n",
        )
        .unwrap();
        let path = CString::new(file.to_str().unwrap()).unwrap();
        assert!(unsafe { playerbot_config_load(path.as_ptr()) });

        let key = CString::new("AiPlayerbot.ReactDelay").unwrap();
        assert_eq!(unsafe { playerbot_config_get_u32(key.as_ptr(), 0) }, 333);

        std::fs::remove_file(&file).ok();
        reset();
    }
}
