//! `extern "C"` surface for the Rust item pool.
//!
//! The C++ shim at `cpp_wrapper/ItemBridge.{h,cpp}` owns the
//! `ItemCallbacks` vtable and calls into this module via:
//!
//! * [`playerbot_itempool_init`] — install the vtable, query all DB
//!   rows, and build every cache.
//! * [`playerbot_itempool_shutdown`] — drop the cached state.
//! * [`playerbot_itempool_query`] — equip-cache lookup returning an
//!   owned `u32` slice.
//! * [`playerbot_itempool_get_min_level`],
//!   [`playerbot_itempool_get_stat_weight`],
//!   [`playerbot_itempool_get_item_rarity`] — cheap metadata lookups.
//! * [`playerbot_itempool_get_ammo`],
//!   [`playerbot_itempool_get_random_potion`],
//!   [`playerbot_itempool_get_random_food`],
//!   [`playerbot_itempool_get_random_trade`] — consumable pool helpers.
//! * [`playerbot_itempool_calculate_enchant_weight`] and
//!   [`playerbot_itempool_calculate_best_random_enchant_id`] — enchant
//!   scoring.
//! * [`playerbot_itempool_free_u32_list`] — allocator pair for the
//!   u32 slices returned by `query` / `get_gems`.
//!
//! State is a single `OnceLock<Mutex<Option<ItemPoolState>>>`. Init
//! installs a fresh state; shutdown clears it. Every other entry point
//! acquires the mutex, reads from the manager, and releases it before
//! returning — none of them hold the lock across a callback back into
//! C++.

use std::sync::{Arc, Mutex, OnceLock};

use cmangos::{BotItemPrototype, ItemCallbacks, ItemWorld, VtableItemWorld};

use crate::itempool::equip_cache::EquipCacheRow;
use crate::itempool::manager::{ItemPoolManager, ManagerInputs, PreloadedCaches};
use crate::itempool::random_cache::RandomCacheRow;
use crate::log_error;

/// Singleton state owned by the FFI layer. `None` until
/// [`playerbot_itempool_init`] runs.
struct ItemPoolState {
    /// Cached itempool — built once during init.
    manager: ItemPoolManager,
    /// Live vtable used by runtime queries (`urand_range`, enchant
    /// lookups, …). Shared behind `Arc` so we can pass a `&dyn
    /// ItemWorld` into the manager methods without cloning the whole
    /// vtable.
    world: Arc<dyn ItemWorld>,
}

fn state() -> &'static Mutex<Option<ItemPoolState>> {
    static STATE: OnceLock<Mutex<Option<ItemPoolState>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

fn with_state<R>(default: R, f: impl FnOnce(&ItemPoolState) -> R) -> R {
    let guard = match state().lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    match guard.as_ref() {
        Some(st) => f(st),
        None => default,
    }
}

// ── Lifecycle ─────────────────────────────────────────────────────────────

/// Install the item vtable and build every cache. A null pointer is a
/// safe no-op. A second call tears down the previous state before
/// installing the new one.
///
/// # Safety
///
/// `cbs` must either be null or a valid, fully-initialised
/// `ItemCallbacks` whose function pointers remain valid until the
/// next [`playerbot_itempool_shutdown`] call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_itempool_init(cbs: *const ItemCallbacks) {
    if cbs.is_null() {
        return;
    }

    // Copy the vtable by value — it's a PoD struct of fn pointers.
    // Safety: caller guarantees `cbs` is fully initialised.
    let cbs_copy = unsafe { *cbs };

    // Safety: caller promises the function pointers stay valid until
    // shutdown; `VtableItemWorld` is Send+Sync for the operations
    // exposed through the trait.
    let world: Arc<dyn ItemWorld> =
        Arc::new(unsafe { VtableItemWorld::new(cbs_copy) });

    // Pull every table the manager wants to snapshot during init.
    let inputs = ManagerInputs {
        quest_items: world.query_quest_items(),
        vendor_items: world.query_vendor_items(),
        enchant_rows: world.query_item_enchantment_template(),
        preloaded: PreloadedCaches {
            rarity_rows: world.query_rarity_rows(),
            // Load the pre-built equip / random-item caches from the DB
            // (`ai_playerbot_equip_cache` / `ai_playerbot_rnditem_cache`).
            // When these are non-empty the manager loads them directly;
            // an empty vec forces a full rebuild — a multi-billion-
            // iteration scan that runs synchronously on the startup
            // thread and stalls server boot for minutes.
            equip_rows: world
                .query_equip_cache_rows()
                .iter()
                .map(|r| EquipCacheRow {
                    clazz: r.clazz as u8,
                    spec: r.spec as u8,
                    level: r.level,
                    slot: r.slot as u8,
                    quality: r.quality,
                    item_id: r.item_id,
                })
                .collect(),
            random_rows: world
                .query_random_cache_rows()
                .iter()
                .map(|r| RandomCacheRow {
                    level_bucket: r.level_bucket,
                    kind_raw: r.kind,
                    item_id: r.item_id,
                })
                .collect(),
        },
    };

    // Pull the `randomBotMaxLevel` from the already-loaded config. If
    // the config hasn't been loaded yet (tests), fall back to `60`.
    let max_level = crate::config::get().random_bot_max_level.max(1);

    let mut manager = ItemPoolManager::new();
    manager.init(world.as_ref(), max_level, &inputs);

    let mut guard = match state().lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    *guard = Some(ItemPoolState { manager, world });
}

/// Drop the cached state. Safe to call more than once; the second call
/// is a no-op.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_itempool_shutdown() {
    let mut guard = match state().lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    drop(guard.take());
}

// ── Item metadata ─────────────────────────────────────────────────────────

/// Minimum required level for `item_id`, or `0` when the item is not
/// in the info cache.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_itempool_get_min_level(item_id: u32) -> u32 {
    with_state(0, |st| st.manager.min_level(item_id))
}

/// Resolve the spec id for a logged-in player via the bridge's talent
/// tab snapshot. Returns `0` when the bridge has no context, the spec
/// name can't be derived, or no scale matches.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_itempool_get_player_spec_id(guid_low: u32) -> u32 {
    with_state(0, |st| st.manager.player_spec_id(st.world.as_ref(), guid_low))
}

/// Live (filtered) stat weight for `(guid_low, item_id, spec_id)`.
/// When `spec_id == 0`, falls back to the talent-tab lookup. Mirrors
/// `RandomItemMgr::GetLiveStatWeight`.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_itempool_get_live_stat_weight(
    guid_low: u32,
    item_id: u32,
    spec_id: u32,
) -> u32 {
    with_state(0, |st| {
        st.manager
            .live_stat_weight(st.world.as_ref(), guid_low, item_id, spec_id)
    })
}

/// Best enchant score rollable on `item_id` under `spec_id`.
/// Mirrors `RandomItemMgr::GetBestRandomEnchantStatWeight`.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_itempool_get_best_random_enchant_stat_weight(
    item_id: u32,
    spec_id: u32,
) -> u32 {
    with_state(0, |st| {
        st.manager
            .best_random_enchant_stat_weight(st.world.as_ref(), item_id, spec_id)
    })
}

/// `true` when the player already holds a reward item from any quest
/// that can grant `item_id`. Mirrors
/// `RandomItemMgr::HasSameQuestRewards`.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_itempool_has_same_quest_rewards(
    guid_low: u32,
    item_id: u32,
) -> bool {
    with_state(false, |st| {
        st.manager
            .has_same_quest_rewards(st.world.as_ref(), guid_low, item_id)
    })
}

/// Owned copy of the full gem list. Empty on vanilla; allocated as a
/// `Box<[u32]>` on TBC/WotLK and freed via
/// [`playerbot_itempool_free_u32_list`].
///
/// * Empty result is `(NULL, 0)` with a `true` return value.
/// * `false` is returned only when `out_ptr` or `out_len` is null.
///
/// # Safety
///
/// `out_ptr` and `out_len` must both be valid, writable pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_itempool_get_gems(
    out_ptr: *mut *mut u32,
    out_len: *mut usize,
) -> bool {
    if out_ptr.is_null() || out_len.is_null() {
        log_error!("playerbot_itempool_get_gems: null out pointer");
        return false;
    }
    let gems: Vec<u32> = with_state(Vec::new(), |st| st.manager.gems_list().to_vec());
    // Safety: caller guarantees `out_ptr` / `out_len` are valid.
    unsafe {
        let (ptr, len) = into_raw_u32_slice(gems);
        out_ptr.write(ptr);
        out_len.write(len);
    }
    true
}

/// Rarity for `item_id`. Returns the DB-cached value if present,
/// otherwise a heuristic derived from the item quality. Returns `0.0`
/// when the item is not in the info cache.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_itempool_get_item_rarity(item_id: u32) -> f32 {
    with_state(0.0_f32, |st| st.manager.item_rarity(item_id))
}

// ── Stat weights ──────────────────────────────────────────────────────────

/// Pre-computed base stat weight for `(item_id, spec_id)`. Returns `0`
/// when either id is zero or the item is not in the info cache.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_itempool_get_stat_weight(item_id: u32, spec_id: u32) -> u32 {
    with_state(0, |st| st.manager.stat_weight(item_id, spec_id))
}

// ── Pool queries ──────────────────────────────────────────────────────────

/// Query the equip pool for `(level, class, spec, slot, quality)`.
///
/// Allocates a fresh `Box<[u32]>` and hands it to the caller; the
/// caller must free it with [`playerbot_itempool_free_u32_list`].
///
/// * Empty result is `(NULL, 0)` with a `true` return value.
/// * `false` is returned only when `out_ptr` or `out_len` is null.
///
/// # Safety
///
/// `out_ptr` and `out_len` must both be valid, writable pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_itempool_query(
    level: u32,
    class_id: u8,
    spec_id: u8,
    slot: u8,
    quality: u32,
    out_ptr: *mut *mut u32,
    out_len: *mut usize,
) -> bool {
    if out_ptr.is_null() || out_len.is_null() {
        log_error!("playerbot_itempool_query: null out pointer");
        return false;
    }

    let items: Vec<u32> = with_state(Vec::new(), |st| {
        st.manager
            .query_equip(level, class_id, spec_id, slot, quality)
            .to_vec()
    });

    // Safety: caller guarantees out_ptr and out_len are valid. `write`
    // is used so we don't read the uninitialised destination.
    unsafe {
        let (ptr, len) = into_raw_u32_slice(items);
        out_ptr.write(ptr);
        out_len.write(len);
    }
    true
}

/// Free a `(ptr, len)` pair returned by any FFI entry point in this
/// module that documents this function as its free path. A
/// `(NULL, 0)` pair is a safe no-op.
///
/// # Safety
///
/// `ptr` must have been produced by a call into this module (never
/// `malloc`'d on the C side), and `len` must match the length
/// originally returned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_itempool_free_u32_list(ptr: *mut u32, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    // Safety: caller guarantees (ptr, len) came from a
    // `Box::into_raw` in this module. Reconstruct the box and let the
    // Drop impl release the allocation.
    unsafe {
        let slice = std::slice::from_raw_parts_mut(ptr, len);
        drop(Box::<[u32]>::from_raw(slice as *mut [u32]));
    }
}

// ── Consumable pools ──────────────────────────────────────────────────────

/// Pre-selected ammo item id for `(level, sub_class)`. Returns `0` on
/// miss.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_itempool_get_ammo(level: u32, ammo_subclass: u32) -> u32 {
    with_state(0, |st| st.manager.ammo(level, ammo_subclass))
}

/// Random potion pick from the cache. Returns `0` when the bucket is
/// empty.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_itempool_get_random_potion(level: u32, effect: u32) -> u32 {
    with_state(0, |st| {
        st.manager.random_potion(st.world.as_ref(), level, effect)
    })
}

/// Random food pick from the cache. Returns `0` when the bucket is
/// empty.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_itempool_get_random_food(level: u32, category: u32) -> u32 {
    with_state(0, |st| {
        st.manager.random_food(st.world.as_ref(), level, category)
    })
}

/// Equivalent to `RandomItemMgr::GetFood(level, category)`. The legacy
/// `GetFood` and `GetRandomFood` both resolve to the same random pick
/// in the post-port implementation, so this forwards unchanged.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_itempool_get_food(level: u32, category: u32) -> u32 {
    playerbot_itempool_get_random_food(level, category)
}

/// Random trade-goods pick from the cache. Returns `0` when the
/// bucket is empty.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_itempool_get_random_trade(level: u32) -> u32 {
    with_state(0, |st| st.manager.random_trade(st.world.as_ref(), level))
}

// ── Enchant scoring ───────────────────────────────────────────────────────

/// Score an enchant id for `(class, spec)`. Returns `0` when the spec
/// scale does not exist or the enchant is not in the store.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_itempool_calculate_enchant_weight(
    class_id: u8,
    spec_id: u8,
    enchant_id: u32,
) -> u32 {
    with_state(0, |st| {
        st.manager.calculate_enchant_weight(
            st.world.as_ref(),
            class_id,
            u32::from(spec_id),
            enchant_id,
        )
    })
}

/// Pick the best random enchant id rollable for `item_id` under
/// `(class, spec)`. Returns `0` when the item has no random property
/// entry or every roll scores 0.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_itempool_calculate_best_random_enchant_id(
    class_id: u8,
    spec_id: u8,
    item_id: u32,
) -> u32 {
    with_state(0, |st| {
        st.manager.calculate_best_random_enchant_id(
            st.world.as_ref(),
            class_id,
            u32::from(spec_id),
            item_id,
        )
    })
}

// ── In-process crate API ─────────────────────────────────────────────────
//
// Entry points for Rust code that lives in this crate (factory, random
// managers, …) so the itempool singleton can be queried without round-
// tripping through the C FFI. Every shim holds the mutex only for the
// duration of a single manager call — the `with_state` helper above
// enforces that.

/// Query the equip pool for `(level, class, spec, slot, quality)` as an
/// owned `Vec<u32>`. Empty when the itempool has not been initialised or
/// the bucket is empty. Mirrors `playerbot_itempool_query` but keeps
/// the data in-process for Rust-side consumers.
pub(crate) fn query_equip(
    level: u32,
    class_id: u8,
    spec_id: u8,
    slot: u8,
    quality: u32,
) -> Vec<u32> {
    with_state(Vec::new(), |st| {
        st.manager
            .query_equip(level, class_id, spec_id, slot, quality)
            .to_vec()
    })
}

/// Pre-computed base stat weight for `(item_id, spec_id)`. Returns `0`
/// when the itempool has not been initialised.
pub(crate) fn stat_weight(item_id: u32, spec_id: u32) -> u32 {
    with_state(0, |st| st.manager.stat_weight(item_id, spec_id))
}

/// Cached minimum level for `item_id`. Returns `0` when the item is not
/// in the info cache or the itempool has not been initialised.
pub(crate) fn min_level(item_id: u32) -> u32 {
    with_state(0, |st| st.manager.min_level(item_id))
}

/// Live (filtered) stat weight for `(guid_low, item_id, spec_id)`.
/// Returns `0` when the itempool has not been initialised.
pub(crate) fn live_stat_weight(guid_low: u32, item_id: u32, spec_id: u32) -> u32 {
    with_state(0, |st| {
        st.manager
            .live_stat_weight(st.world.as_ref(), guid_low, item_id, spec_id)
    })
}

/// Best enchant score rollable on `item_id` under `spec_id`. Returns
/// `0` when the itempool has not been initialised.
pub(crate) fn best_random_enchant_stat_weight(item_id: u32, spec_id: u32) -> u32 {
    with_state(0, |st| {
        st.manager
            .best_random_enchant_stat_weight(st.world.as_ref(), item_id, spec_id)
    })
}

/// `true` when the player already holds a reward item from any quest
/// that can grant `item_id`. Returns `false` when the itempool has not
/// been initialised.
pub(crate) fn has_same_quest_rewards(guid_low: u32, item_id: u32) -> bool {
    with_state(false, |st| {
        st.manager
            .has_same_quest_rewards(st.world.as_ref(), guid_low, item_id)
    })
}

/// Pick the best random enchant id rollable for `item_id` under
/// `(class, spec)`. Returns `0` when the itempool has not been
/// initialised or the item has no rollable enchant.
pub(crate) fn calculate_best_random_enchant_id(class_id: u8, spec_id: u32, item_id: u32) -> u32 {
    with_state(0, |st| {
        st.manager
            .calculate_best_random_enchant_id(st.world.as_ref(), class_id, spec_id, item_id)
    })
}

/// Score an enchant id for `(class, spec)`. Returns `0` when the
/// itempool has not been initialised or the scale does not exist.
pub(crate) fn calculate_enchant_weight(class_id: u8, spec_id: u32, enchant_id: u32) -> u32 {
    with_state(0, |st| {
        st.manager
            .calculate_enchant_weight(st.world.as_ref(), class_id, spec_id, enchant_id)
    })
}

/// Look up a prototype by id. Returns an owned copy because the
/// singleton lock is released before the value is returned — callers
/// must not hold the lock while accessing the result. `BotItemPrototype`
/// is `Copy` (POD) so this is a cheap `memcpy` (~648 bytes).
pub(crate) fn prototype(item_id: u32) -> Option<BotItemPrototype> {
    with_state(None, |st| st.manager.prototype(item_id).copied())
}

/// Resolve the bot's talent-derived spec id. Returns `0` when the
/// itempool has not been initialised or the bot's active tree has no
/// registered weight scale.
pub(crate) fn player_spec_id(guid_low: u32) -> u32 {
    with_state(0, |st| st.manager.player_spec_id(st.world.as_ref(), guid_low))
}

/// Test-only helper to install a pre-built manager + world. Used by
/// equipment-factory unit tests that need to drive the real
/// `ItemPoolManager::query_equip` / `stat_weight` / … path without
/// going through the full `playerbot_itempool_init` vtable installation
/// dance. Tests using this helper must run serially — the singleton is
/// process-wide.
#[cfg(test)]
pub(crate) fn install_state_for_test(manager: ItemPoolManager, world: Arc<dyn ItemWorld>) {
    let mut guard = match state().lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    *guard = Some(ItemPoolState { manager, world });
}

// ── Helpers ───────────────────────────────────────────────────────────────

/// Consume a `Vec<u32>` and hand its backing allocation to the caller.
/// Empty vecs are returned as `(null, 0)` so the caller doesn't need
/// a corresponding free call.
fn into_raw_u32_slice(v: Vec<u32>) -> (*mut u32, usize) {
    if v.is_empty() {
        return (std::ptr::null_mut(), 0);
    }
    let boxed: Box<[u32]> = v.into_boxed_slice();
    let len = boxed.len();
    let ptr = Box::into_raw(boxed).cast::<u32>();
    (ptr, len)
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_on_fresh_state_is_noop() {
        // Make sure we start from a clean slate — other tests in this
        // module may have left state around.
        playerbot_itempool_shutdown();
        playerbot_itempool_shutdown();
    }

    #[test]
    fn query_returns_empty_without_init() {
        playerbot_itempool_shutdown();

        let mut ptr: *mut u32 = std::ptr::null_mut();
        let mut len: usize = 0;
        // Safety: pointers are local and valid for the duration of
        // the call.
        let ok = unsafe {
            playerbot_itempool_query(60, 1, 1, 4, 3, &mut ptr as *mut _, &mut len as *mut _)
        };
        assert!(ok);
        assert!(ptr.is_null());
        assert_eq!(len, 0);
    }

    #[test]
    fn metadata_returns_zero_without_init() {
        playerbot_itempool_shutdown();

        assert_eq!(playerbot_itempool_get_min_level(1234), 0);
        assert_eq!(playerbot_itempool_get_stat_weight(1234, 1), 0);
        assert!(playerbot_itempool_get_item_rarity(1234).abs() < f32::EPSILON);
    }

    #[test]
    fn consumables_return_zero_without_init() {
        playerbot_itempool_shutdown();

        assert_eq!(playerbot_itempool_get_ammo(60, 2), 0);
        assert_eq!(playerbot_itempool_get_random_potion(60, 10), 0);
        assert_eq!(playerbot_itempool_get_food(60, 11), 0);
        assert_eq!(playerbot_itempool_get_random_food(60, 11), 0);
        assert_eq!(playerbot_itempool_get_random_trade(60), 0);
    }

    #[test]
    fn enchants_return_zero_without_init() {
        playerbot_itempool_shutdown();

        assert_eq!(playerbot_itempool_calculate_enchant_weight(1, 1, 42), 0);
        assert_eq!(
            playerbot_itempool_calculate_best_random_enchant_id(1, 1, 42),
            0
        );
    }

    #[test]
    fn live_scoring_returns_zero_without_init() {
        playerbot_itempool_shutdown();

        assert_eq!(playerbot_itempool_get_player_spec_id(42), 0);
        assert_eq!(playerbot_itempool_get_live_stat_weight(42, 1234, 1), 0);
        assert_eq!(
            playerbot_itempool_get_best_random_enchant_stat_weight(1234, 1),
            0
        );
        assert!(!playerbot_itempool_has_same_quest_rewards(42, 1234));
    }

    #[test]
    fn get_gems_returns_empty_without_init() {
        playerbot_itempool_shutdown();

        let mut ptr: *mut u32 = std::ptr::null_mut();
        let mut len: usize = 0;
        // Safety: local pointers are valid for the duration of the call.
        let ok = unsafe {
            playerbot_itempool_get_gems(&mut ptr as *mut _, &mut len as *mut _)
        };
        assert!(ok);
        assert!(ptr.is_null());
        assert_eq!(len, 0);
    }

    #[test]
    fn free_u32_list_handles_null_and_zero() {
        // Safety: both valid no-op combinations per the safety
        // contract on the function.
        unsafe {
            playerbot_itempool_free_u32_list(std::ptr::null_mut(), 0);
            playerbot_itempool_free_u32_list(std::ptr::null_mut(), 5);
        }
    }
}
