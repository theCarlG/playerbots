//! `ItemWorld` — the module-level interface the Rust item pool / random
//! gear subsystem uses to talk to `CMaNGOS`.
//!
//! Unlike [`World`](crate::World) which is per-bot, `ItemWorld` is shared
//! across the whole process: it owns the bulk `sItemStorage` /
//! `sSpellTemplate` / `sFactionStore` / quest-reward / vendor-source /
//! weight-scale / random-enchant / gem / rarity queries that the legacy
//! `RandomItemMgr` ran during `Init()`, plus the small per-player queries
//! the runtime `GetLiveStatWeight` path needs. Separating it from `World`
//! keeps the per-bot trait focused on per-bot semantics.
//!
//! Two implementations exist, mirroring the `World` / `LoginWorld` split:
//!
//! * [`VtableItemWorld`] — production impl, wraps a `*const ItemCallbacks`
//!   from the C++ side. Built once during `playerbot_itempool_init`.
//! * [`MockItemWorld`] — feature-gated under `mock`; fixture-backed pure
//!   Rust impl used by the `crates/playerbot` itempool tests.
//!
//! Memory ownership follows the same pattern as `LoginWorld::query_candidates`:
//! every `query_*` returns a freshly-allocated array the C bridge owns
//! until the matching `free_*` is called. Rust copies rows it needs into
//! its own Vec during init and releases the raw arrays before returning.

use crate::{
    BotEnchantInfo, BotGemPropertiesRow, BotItemEnchantmentRow, BotItemPrototype,
    BotItemRarityRow, BotPlayerItemCtx, BotQuestItemRow, BotSpellEntryInfo, BotVendorItemRow,
    BotWeightScaleRow, BotWeightScaleStatRow, ItemCallbacks,
};

/// Bitflags used by [`ItemWorld::player_quest_status`]. Mirrors the
/// `PLAYERBOT_QUEST_*` constants from `botffi.h`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct QuestStatus(pub u32);

impl QuestStatus {
    pub const NONE: Self = Self(0);
    pub const SATISFIES_CLASS: Self = Self(cmangos_sys::PLAYERBOT_QUEST_SATISFIES_CLASS);
    pub const SATISFIES_RACE: Self = Self(cmangos_sys::PLAYERBOT_QUEST_SATISFIES_RACE);
    pub const SATISFIES_LEVEL: Self = Self(cmangos_sys::PLAYERBOT_QUEST_SATISFIES_LEVEL);
    pub const IS_ACTIVE: Self = Self(cmangos_sys::PLAYERBOT_QUEST_IS_ACTIVE);
    pub const IS_COMPLETED: Self = Self(cmangos_sys::PLAYERBOT_QUEST_IS_COMPLETED);
    pub const IS_REWARDED: Self = Self(cmangos_sys::PLAYERBOT_QUEST_IS_REWARDED);

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// Information returned by [`ItemWorld::player_talent_tab`].
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct TalentTabInfo {
    /// Best talent tab index (0..2). 0 = no points spent.
    pub best_tab: u32,
    /// Rank of the priest's Meditation talent (used to disambiguate
    /// discipline vs holy). 0 = no ranks.
    pub meditation_rank: u32,
}

/// Abstract interface the item pool uses to reach `CMaNGOS`.
///
/// Every method is callable from whichever thread installed the vtable
/// (main thread during `playerbot_itempool_init`, factory-owning thread
/// afterwards). The underlying C++ reads are thread-safe for the
/// operations exposed here: `sItemStorage`/`sSpellTemplate` are immutable
/// after `World::LoadDBCStores`, and the per-player queries lock the
/// `ObjectAccessor`.
pub trait ItemWorld: Send + Sync {
    /// Scan every row of `sItemStorage` and return a freshly-allocated
    /// vec of prototypes in id order. Called exactly once during init.
    fn query_item_prototypes(&self) -> Vec<BotItemPrototype>;

    /// Resolve a spell id against `sSpellTemplate`. Returns `None` for
    /// unknown ids.
    fn lookup_spell_entry(&self, spell_id: u32) -> Option<BotSpellEntryInfo>;

    /// Load the `ai_playerbot_weight_scales` table. Empty if the table
    /// does not exist in the server's `characters` DB.
    fn query_weight_scales(&self) -> Vec<BotWeightScaleRow>;

    /// Load the `ai_playerbot_weight_scales_stats` table.
    fn query_weight_scale_stats(&self) -> Vec<BotWeightScaleStatRow>;

    /// Enumerate every `(quest, reward item)` pair from the quest store.
    /// One row per quest × reward item; if a quest rewards 4 items,
    /// 4 rows are emitted. Covers both fixed rewards and choice rewards
    /// (distinguished by the `is_choice` field).
    fn query_quest_items(&self) -> Vec<BotQuestItemRow>;

    /// Enumerate every `(vendor, item)` pair from `npc_vendor`, with
    /// the vendor's faction and any pre-resolved reputation-rank
    /// requirement.
    fn query_vendor_items(&self) -> Vec<BotVendorItemRow>;

    /// Load the `item_enchantment_template` table (weighted enchant
    /// roll pool, one row per `(entry, ench)` pair).
    fn query_item_enchantment_template(&self) -> Vec<BotItemEnchantmentRow>;

    /// On-demand enchant lookup — `sSpellItemEnchantmentStore` row for
    /// `enchant_id`.
    fn lookup_enchant_info(&self, enchant_id: u32) -> Option<BotEnchantInfo>;

    /// Resolve the four enchant slots of a random-property entry.
    /// Returns `None` if the random property id is unknown.
    fn lookup_random_property_enchants(&self, property_id: i32) -> Option<[u32; 4]>;

    /// Optional `ai_playerbot_rarity_cache` rows. An empty vec means
    /// the cache table was not created; the Rust side will fall back
    /// to its computed rarity heuristic.
    fn query_rarity_rows(&self) -> Vec<BotItemRarityRow>;

    /// `sGemPropertiesStore` contents. Empty on Classic.
    fn query_gem_properties(&self) -> Vec<BotGemPropertiesRow>;

    /// Per-player context snapshot. Returns `None` when the guid is
    /// not currently a logged-in player.
    fn player_item_ctx(&self, guid_low: u32) -> Option<BotPlayerItemCtx>;

    /// Player's current reputation rank for `faction_id`. `-1` = hated,
    /// `7` = exalted; unknown faction returns `0` (neutral).
    fn player_reputation_rank(&self, guid_low: u32, faction_id: u32) -> i32;

    /// Player's current skill value for `skill_id`. `0` = not trained.
    fn player_skill_value(&self, guid_low: u32, skill_id: u32) -> u32;

    /// Quest status bitmask for `(player, quest)`.
    fn player_quest_status(&self, guid_low: u32, quest_id: u32) -> QuestStatus;

    /// `true` when the player currently holds at least one copy of
    /// `item_id` in bags or bank. Mirrors
    /// `player->HasItemCount(item_id, 1, true)` in the legacy C++.
    fn player_has_item(&self, guid_low: u32, item_id: u32) -> bool;

    /// Player's best talent tab and meditation rank.
    fn player_talent_tab(&self, guid_low: u32) -> TalentTabInfo;

    /// Wrap the `CMaNGOS` `urand(min, max)` helper (inclusive).
    fn urand_range(&self, min: u32, max: u32) -> u32;

    /// Forward a debug line to the C++ log sink. The default
    /// implementation does nothing so mocks don't have to care.
    fn log_debug(&self, _msg: &str) {}
}

/// Production impl — wraps the C `ItemCallbacks` vtable.
///
/// The C pointer is stored as a raw function pointer because
/// `ItemCallbacks` is `Copy`; the struct is filled once on the C++ side
/// and never mutated again. `VtableItemWorld` is `Send + Sync` because
/// the underlying pointers refer to global C++ state that is thread-safe
/// for the operations exposed here.
pub struct VtableItemWorld {
    cbs: ItemCallbacks,
}

#[allow(unsafe_code)]
// Safety: every field on ItemCallbacks is a plain function pointer
// whose implementation is thread-safe per the invariants above.
unsafe impl Send for VtableItemWorld {}
#[allow(unsafe_code)]
unsafe impl Sync for VtableItemWorld {}

impl VtableItemWorld {
    /// # Safety
    /// `cbs` must be a fully initialised `ItemCallbacks` whose function
    /// pointers remain valid for the entire lifetime of the returned
    /// object (in practice: until `playerbot_itempool_shutdown`).
    #[must_use]
    #[allow(unsafe_code)]
    pub unsafe fn new(cbs: ItemCallbacks) -> Self {
        Self { cbs }
    }
}

/// Helper: copy a C-allocated array into a `Vec<T>` and free it via
/// the matching `free_fn`. Returns an empty vec if either function
/// pointer is null or the query returns zero rows.
#[allow(unsafe_code)]
fn drain_list<T, QueryFn, FreeFn>(
    query: Option<QueryFn>,
    free: Option<FreeFn>,
) -> Vec<T>
where
    T: Copy,
    QueryFn: FnOnce(&mut *mut T) -> u32,
    FreeFn: FnOnce(*mut T, u32),
{
    let (Some(query), Some(free)) = (query, free) else {
        return Vec::new();
    };

    let mut ptr: *mut T = core::ptr::null_mut();
    let count = query(&mut ptr);
    if ptr.is_null() || count == 0 {
        return Vec::new();
    }

    // Safety: the C side guarantees `ptr..ptr+count` is readable for
    // `count` elements of `T` until the matching `free` is invoked.
    let slice = unsafe { core::slice::from_raw_parts(ptr, count as usize) };
    let out = slice.to_vec();
    free(ptr, count);
    out
}

#[allow(unsafe_code)]
impl ItemWorld for VtableItemWorld {
    fn query_item_prototypes(&self) -> Vec<BotItemPrototype> {
        drain_list(
            self.cbs.query_item_prototypes.map(|f| {
                move |out_ptr: &mut *mut BotItemPrototype| unsafe { f(out_ptr) }
            }),
            self.cbs.free_item_prototype_list.map(|f| {
                move |ptr: *mut BotItemPrototype, count: u32| unsafe { f(ptr, count) }
            }),
        )
    }

    fn lookup_spell_entry(&self, spell_id: u32) -> Option<BotSpellEntryInfo> {
        let f = self.cbs.lookup_spell_entry?;
        let mut out = BotSpellEntryInfo::default();
        // Safety: `out` is a live out-pointer; the callback only writes
        // if it returns true.
        if unsafe { f(spell_id, &mut out) } {
            Some(out)
        } else {
            None
        }
    }

    fn query_weight_scales(&self) -> Vec<BotWeightScaleRow> {
        drain_list(
            self.cbs.query_weight_scales.map(|f| {
                move |out_ptr: &mut *mut BotWeightScaleRow| unsafe { f(out_ptr) }
            }),
            self.cbs.free_weight_scale_list.map(|f| {
                move |ptr: *mut BotWeightScaleRow, count: u32| unsafe { f(ptr, count) }
            }),
        )
    }

    fn query_weight_scale_stats(&self) -> Vec<BotWeightScaleStatRow> {
        drain_list(
            self.cbs.query_weight_scale_stats.map(|f| {
                move |out_ptr: &mut *mut BotWeightScaleStatRow| unsafe { f(out_ptr) }
            }),
            self.cbs.free_weight_scale_stat_list.map(|f| {
                move |ptr: *mut BotWeightScaleStatRow, count: u32| unsafe { f(ptr, count) }
            }),
        )
    }

    fn query_quest_items(&self) -> Vec<BotQuestItemRow> {
        drain_list(
            self.cbs.query_quest_items.map(|f| {
                move |out_ptr: &mut *mut BotQuestItemRow| unsafe { f(out_ptr) }
            }),
            self.cbs.free_quest_item_list.map(|f| {
                move |ptr: *mut BotQuestItemRow, count: u32| unsafe { f(ptr, count) }
            }),
        )
    }

    fn query_vendor_items(&self) -> Vec<BotVendorItemRow> {
        drain_list(
            self.cbs.query_vendor_items.map(|f| {
                move |out_ptr: &mut *mut BotVendorItemRow| unsafe { f(out_ptr) }
            }),
            self.cbs.free_vendor_item_list.map(|f| {
                move |ptr: *mut BotVendorItemRow, count: u32| unsafe { f(ptr, count) }
            }),
        )
    }

    fn query_item_enchantment_template(&self) -> Vec<BotItemEnchantmentRow> {
        drain_list(
            self.cbs.query_item_enchantment_template.map(|f| {
                move |out_ptr: &mut *mut BotItemEnchantmentRow| unsafe { f(out_ptr) }
            }),
            self.cbs.free_item_enchantment_list.map(|f| {
                move |ptr: *mut BotItemEnchantmentRow, count: u32| unsafe { f(ptr, count) }
            }),
        )
    }

    fn lookup_enchant_info(&self, enchant_id: u32) -> Option<BotEnchantInfo> {
        let f = self.cbs.lookup_enchant_info?;
        let mut out = BotEnchantInfo::default();
        // Safety: `out` is a live out-pointer; the callback only writes
        // if it returns true.
        if unsafe { f(enchant_id, &mut out) } {
            Some(out)
        } else {
            None
        }
    }

    fn lookup_random_property_enchants(&self, property_id: i32) -> Option<[u32; 4]> {
        let f = self.cbs.lookup_random_property_enchants?;
        let mut out = [0u32; 4];
        // Safety: `out` is a live out-pointer of the exact length the
        // callback writes into.
        if unsafe { f(property_id, out.as_mut_ptr()) } {
            Some(out)
        } else {
            None
        }
    }

    fn query_rarity_rows(&self) -> Vec<BotItemRarityRow> {
        drain_list(
            self.cbs.query_rarity_rows.map(|f| {
                move |out_ptr: &mut *mut BotItemRarityRow| unsafe { f(out_ptr) }
            }),
            self.cbs.free_rarity_list.map(|f| {
                move |ptr: *mut BotItemRarityRow, count: u32| unsafe { f(ptr, count) }
            }),
        )
    }

    fn query_gem_properties(&self) -> Vec<BotGemPropertiesRow> {
        drain_list(
            self.cbs.query_gem_properties.map(|f| {
                move |out_ptr: &mut *mut BotGemPropertiesRow| unsafe { f(out_ptr) }
            }),
            self.cbs.free_gem_list.map(|f| {
                move |ptr: *mut BotGemPropertiesRow, count: u32| unsafe { f(ptr, count) }
            }),
        )
    }

    fn player_item_ctx(&self, guid_low: u32) -> Option<BotPlayerItemCtx> {
        let f = self.cbs.get_player_item_ctx?;
        let mut out = BotPlayerItemCtx::default();
        // Safety: `out` is a live out-pointer; the callback only writes
        // if it returns true.
        if unsafe { f(guid_low, &mut out) } {
            Some(out)
        } else {
            None
        }
    }

    fn player_reputation_rank(&self, guid_low: u32, faction_id: u32) -> i32 {
        self.cbs
            .get_player_reputation_rank
            .map_or(0, |f| unsafe { f(guid_low, faction_id) })
    }

    fn player_skill_value(&self, guid_low: u32, skill_id: u32) -> u32 {
        self.cbs
            .get_player_skill_value
            .map_or(0, |f| unsafe { f(guid_low, skill_id) })
    }

    fn player_quest_status(&self, guid_low: u32, quest_id: u32) -> QuestStatus {
        QuestStatus(
            self.cbs
                .player_quest_status
                .map_or(0, |f| unsafe { f(guid_low, quest_id) }),
        )
    }

    fn player_has_item(&self, guid_low: u32, item_id: u32) -> bool {
        self.cbs
            .player_has_item
            .is_some_and(|f| unsafe { f(guid_low, item_id) })
    }

    fn player_talent_tab(&self, guid_low: u32) -> TalentTabInfo {
        let Some(f) = self.cbs.get_player_talent_tab else {
            return TalentTabInfo::default();
        };
        let mut meditation_rank: u32 = 0;
        // Safety: `meditation_rank` is a live out-pointer.
        let best_tab = unsafe { f(guid_low, &mut meditation_rank) };
        TalentTabInfo {
            best_tab,
            meditation_rank,
        }
    }

    fn urand_range(&self, min: u32, max: u32) -> u32 {
        self.cbs.urand_range.map_or(min, |f| unsafe { f(min, max) })
    }

    fn log_debug(&self, msg: &str) {
        let Some(f) = self.cbs.log_debug else {
            return;
        };
        let mut buf = [0u8; 512];
        let bytes = msg.as_bytes();
        let n = bytes.len().min(buf.len() - 1);
        buf[..n].copy_from_slice(&bytes[..n]);
        buf[n] = 0;
        // Safety: `buf` is nul-terminated within its own length.
        unsafe { f(buf.as_ptr().cast()) };
    }
}

/// In-memory mock used by the `crates/playerbot` itempool tests.
///
/// `MockItemWorld` is feature-gated under `mock` (same as
/// [`MockWorld`](crate::MockWorld)) and every method is fully
/// implemented — no `unimplemented!` paths. Tests drive it by
/// pre-seeding prototype rows, weight scales, vendor rows etc, then
/// assert on cache contents after running the itempool init.
#[cfg(any(test, feature = "mock"))]
pub use mock_impl::{MockItemEvent, MockItemWorld};

#[cfg(any(test, feature = "mock"))]
mod mock_impl {
    use super::{
        BotEnchantInfo, BotGemPropertiesRow, BotItemEnchantmentRow, BotItemPrototype,
        BotItemRarityRow, BotPlayerItemCtx, BotQuestItemRow, BotSpellEntryInfo,
        BotVendorItemRow, BotWeightScaleRow, BotWeightScaleStatRow, ItemWorld, QuestStatus,
        TalentTabInfo,
    };
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Event recorded by [`MockItemWorld`]. Tests assert on the event
    /// list after running the itempool init to verify which callbacks
    /// fired.
    #[derive(Clone, Debug, PartialEq)]
    pub enum MockItemEvent {
        QueryItemPrototypes,
        LookupSpell(u32),
        LookupEnchant(u32),
        LookupRandomProperty(i32),
        PlayerItemCtx(u32),
        PlayerReputation { guid: u32, faction: u32 },
        PlayerSkill { guid: u32, skill: u32 },
        PlayerQuestStatus { guid: u32, quest: u32 },
        PlayerHasItem { guid: u32, item: u32 },
        PlayerTalentTab(u32),
        UrandRange { min: u32, max: u32, result: u32 },
        Debug(String),
    }

    #[derive(Default)]
    struct MockState {
        prototypes: Vec<BotItemPrototype>,
        spell_entries: HashMap<u32, BotSpellEntryInfo>,
        weight_scales: Vec<BotWeightScaleRow>,
        weight_scale_stats: Vec<BotWeightScaleStatRow>,
        quest_items: Vec<BotQuestItemRow>,
        vendor_items: Vec<BotVendorItemRow>,
        item_enchantment_template: Vec<BotItemEnchantmentRow>,
        enchants: HashMap<u32, BotEnchantInfo>,
        random_property_enchants: HashMap<i32, [u32; 4]>,
        rarity_rows: Vec<BotItemRarityRow>,
        gem_properties: Vec<BotGemPropertiesRow>,
        player_ctx: HashMap<u32, BotPlayerItemCtx>,
        reputation: HashMap<(u32, u32), i32>,
        skill: HashMap<(u32, u32), u32>,
        quest_status: HashMap<(u32, u32), QuestStatus>,
        has_item: HashMap<(u32, u32), bool>,
        talent_tab: HashMap<u32, TalentTabInfo>,
        urand_next: Vec<u32>,
        urand_deterministic: bool,
        events: Vec<MockItemEvent>,
    }

    /// Fixture-backed [`ItemWorld`] impl for unit/integration tests.
    ///
    /// Thread-safe (`Send + Sync`) by internal `Mutex`; tests that spawn
    /// multiple threads can share a single `Arc<MockItemWorld>`.
    #[derive(Default)]
    pub struct MockItemWorld {
        state: Mutex<MockState>,
    }

    impl MockItemWorld {
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        /// Replace the fixture prototype list.
        pub fn set_prototypes(&self, rows: Vec<BotItemPrototype>) {
            self.state.lock().unwrap().prototypes = rows;
        }

        /// Seed a spell entry. `lookup_spell_entry` returns `None` for
        /// unknown ids.
        pub fn set_spell_entry(&self, spell_id: u32, info: BotSpellEntryInfo) {
            self.state
                .lock()
                .unwrap()
                .spell_entries
                .insert(spell_id, info);
        }

        /// Replace the weight-scale row fixture.
        pub fn set_weight_scales(&self, rows: Vec<BotWeightScaleRow>) {
            self.state.lock().unwrap().weight_scales = rows;
        }

        /// Replace the weight-scale-stat row fixture.
        pub fn set_weight_scale_stats(&self, rows: Vec<BotWeightScaleStatRow>) {
            self.state.lock().unwrap().weight_scale_stats = rows;
        }

        /// Replace the quest-item row fixture.
        pub fn set_quest_items(&self, rows: Vec<BotQuestItemRow>) {
            self.state.lock().unwrap().quest_items = rows;
        }

        /// Replace the vendor-item row fixture.
        pub fn set_vendor_items(&self, rows: Vec<BotVendorItemRow>) {
            self.state.lock().unwrap().vendor_items = rows;
        }

        /// Replace the item-enchantment-template fixture.
        pub fn set_item_enchantment_template(&self, rows: Vec<BotItemEnchantmentRow>) {
            self.state.lock().unwrap().item_enchantment_template = rows;
        }

        /// Seed a `sSpellItemEnchantmentStore` row.
        pub fn set_enchant(&self, enchant_id: u32, info: BotEnchantInfo) {
            self.state.lock().unwrap().enchants.insert(enchant_id, info);
        }

        /// Seed the four enchant slots for a random property id.
        pub fn set_random_property_enchants(&self, property_id: i32, enchants: [u32; 4]) {
            self.state
                .lock()
                .unwrap()
                .random_property_enchants
                .insert(property_id, enchants);
        }

        /// Replace the rarity cache fixture.
        pub fn set_rarity_rows(&self, rows: Vec<BotItemRarityRow>) {
            self.state.lock().unwrap().rarity_rows = rows;
        }

        /// Replace the gem properties fixture.
        pub fn set_gem_properties(&self, rows: Vec<BotGemPropertiesRow>) {
            self.state.lock().unwrap().gem_properties = rows;
        }

        /// Seed a player context row.
        pub fn set_player_ctx(&self, guid: u32, ctx: BotPlayerItemCtx) {
            self.state.lock().unwrap().player_ctx.insert(guid, ctx);
        }

        /// Seed a reputation rank.
        pub fn set_player_reputation(&self, guid: u32, faction: u32, rank: i32) {
            self.state
                .lock()
                .unwrap()
                .reputation
                .insert((guid, faction), rank);
        }

        /// Seed a skill value.
        pub fn set_player_skill(&self, guid: u32, skill: u32, value: u32) {
            self.state.lock().unwrap().skill.insert((guid, skill), value);
        }

        /// Seed a quest status bitmask.
        pub fn set_player_quest_status(&self, guid: u32, quest: u32, status: QuestStatus) {
            self.state
                .lock()
                .unwrap()
                .quest_status
                .insert((guid, quest), status);
        }

        /// Seed an inventory-has-item flag. Defaults to `false` for
        /// unseeded keys, matching `player->HasItemCount` on a player
        /// that doesn't hold the item.
        pub fn set_player_has_item(&self, guid: u32, item: u32, has: bool) {
            self.state
                .lock()
                .unwrap()
                .has_item
                .insert((guid, item), has);
        }

        /// Seed a talent tab snapshot.
        pub fn set_player_talent_tab(&self, guid: u32, info: TalentTabInfo) {
            self.state.lock().unwrap().talent_tab.insert(guid, info);
        }

        /// Enqueue a sequence of `urand(min, max)` return values. The
        /// mock pops them in order; when the queue is empty, it falls
        /// back to returning `min`.
        pub fn push_urand_sequence(&self, values: impl IntoIterator<Item = u32>) {
            let mut s = self.state.lock().unwrap();
            s.urand_next.extend(values);
            s.urand_deterministic = true;
        }

        /// Take the recorded event log, clearing it.
        pub fn take_events(&self) -> Vec<MockItemEvent> {
            std::mem::take(&mut self.state.lock().unwrap().events)
        }

        /// Peek at the event log without clearing it.
        pub fn events(&self) -> Vec<MockItemEvent> {
            self.state.lock().unwrap().events.clone()
        }
    }

    impl ItemWorld for MockItemWorld {
        fn query_item_prototypes(&self) -> Vec<BotItemPrototype> {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockItemEvent::QueryItemPrototypes);
            s.prototypes.clone()
        }

        fn lookup_spell_entry(&self, spell_id: u32) -> Option<BotSpellEntryInfo> {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockItemEvent::LookupSpell(spell_id));
            s.spell_entries.get(&spell_id).copied()
        }

        fn query_weight_scales(&self) -> Vec<BotWeightScaleRow> {
            self.state.lock().unwrap().weight_scales.clone()
        }

        fn query_weight_scale_stats(&self) -> Vec<BotWeightScaleStatRow> {
            self.state.lock().unwrap().weight_scale_stats.clone()
        }

        fn query_quest_items(&self) -> Vec<BotQuestItemRow> {
            self.state.lock().unwrap().quest_items.clone()
        }

        fn query_vendor_items(&self) -> Vec<BotVendorItemRow> {
            self.state.lock().unwrap().vendor_items.clone()
        }

        fn query_item_enchantment_template(&self) -> Vec<BotItemEnchantmentRow> {
            self.state.lock().unwrap().item_enchantment_template.clone()
        }

        fn lookup_enchant_info(&self, enchant_id: u32) -> Option<BotEnchantInfo> {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockItemEvent::LookupEnchant(enchant_id));
            s.enchants.get(&enchant_id).copied()
        }

        fn lookup_random_property_enchants(&self, property_id: i32) -> Option<[u32; 4]> {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockItemEvent::LookupRandomProperty(property_id));
            s.random_property_enchants.get(&property_id).copied()
        }

        fn query_rarity_rows(&self) -> Vec<BotItemRarityRow> {
            self.state.lock().unwrap().rarity_rows.clone()
        }

        fn query_gem_properties(&self) -> Vec<BotGemPropertiesRow> {
            self.state.lock().unwrap().gem_properties.clone()
        }

        fn player_item_ctx(&self, guid_low: u32) -> Option<BotPlayerItemCtx> {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockItemEvent::PlayerItemCtx(guid_low));
            s.player_ctx.get(&guid_low).copied()
        }

        fn player_reputation_rank(&self, guid_low: u32, faction_id: u32) -> i32 {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockItemEvent::PlayerReputation {
                guid: guid_low,
                faction: faction_id,
            });
            s.reputation
                .get(&(guid_low, faction_id))
                .copied()
                .unwrap_or(0)
        }

        fn player_skill_value(&self, guid_low: u32, skill_id: u32) -> u32 {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockItemEvent::PlayerSkill {
                guid: guid_low,
                skill: skill_id,
            });
            s.skill.get(&(guid_low, skill_id)).copied().unwrap_or(0)
        }

        fn player_quest_status(&self, guid_low: u32, quest_id: u32) -> QuestStatus {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockItemEvent::PlayerQuestStatus {
                guid: guid_low,
                quest: quest_id,
            });
            s.quest_status
                .get(&(guid_low, quest_id))
                .copied()
                .unwrap_or(QuestStatus::NONE)
        }

        fn player_has_item(&self, guid_low: u32, item_id: u32) -> bool {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockItemEvent::PlayerHasItem {
                guid: guid_low,
                item: item_id,
            });
            s.has_item
                .get(&(guid_low, item_id))
                .copied()
                .unwrap_or(false)
        }

        fn player_talent_tab(&self, guid_low: u32) -> TalentTabInfo {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockItemEvent::PlayerTalentTab(guid_low));
            s.talent_tab.get(&guid_low).copied().unwrap_or_default()
        }

        fn urand_range(&self, min: u32, max: u32) -> u32 {
            let mut s = self.state.lock().unwrap();
            let result = if s.urand_deterministic && !s.urand_next.is_empty() {
                s.urand_next.remove(0).clamp(min, max)
            } else {
                // Deterministic fallback: always return `min`. Tests that
                // care about the distribution must pre-seed the sequence
                // via `push_urand_sequence`.
                min
            };
            s.events
                .push(MockItemEvent::UrandRange { min, max, result });
            result
        }

        fn log_debug(&self, msg: &str) {
            self.state
                .lock()
                .unwrap()
                .events
                .push(MockItemEvent::Debug(msg.to_string()));
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn empty_world_has_no_data() {
            let m = MockItemWorld::new();
            assert!(m.query_item_prototypes().is_empty());
            assert!(m.query_weight_scales().is_empty());
            assert!(m.query_vendor_items().is_empty());
            assert!(m.lookup_spell_entry(1).is_none());
            assert!(m.lookup_enchant_info(1).is_none());
            assert!(m.lookup_random_property_enchants(1).is_none());
            assert!(m.player_item_ctx(42).is_none());
            assert_eq!(m.player_reputation_rank(42, 1), 0);
            assert_eq!(m.player_skill_value(42, 1), 0);
            assert!(m.player_quest_status(42, 1).is_empty());
        }

        #[test]
        fn prototypes_round_trip() {
            let m = MockItemWorld::new();
            let p = BotItemPrototype {
                item_id: 25,
                class_id: 4,
                quality: 2,
                ..BotItemPrototype::default()
            };
            m.set_prototypes(vec![p]);
            let rows = m.query_item_prototypes();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].item_id, 25);
            assert_eq!(rows[0].quality, 2);
        }

        #[test]
        fn random_property_enchants_round_trip() {
            let m = MockItemWorld::new();
            m.set_random_property_enchants(42, [1, 2, 3, 4]);
            assert_eq!(m.lookup_random_property_enchants(42), Some([1, 2, 3, 4]));
            assert_eq!(m.lookup_random_property_enchants(43), None);
        }

        #[test]
        fn quest_status_round_trip() {
            let m = MockItemWorld::new();
            let status = QuestStatus(
                QuestStatus::SATISFIES_CLASS.0
                    | QuestStatus::SATISFIES_RACE.0
                    | QuestStatus::IS_REWARDED.0,
            );
            m.set_player_quest_status(42, 100, status);
            let read = m.player_quest_status(42, 100);
            assert!(read.contains(QuestStatus::SATISFIES_CLASS));
            assert!(read.contains(QuestStatus::IS_REWARDED));
            assert!(!read.contains(QuestStatus::IS_ACTIVE));
        }

        #[test]
        fn urand_sequence_drains_then_fallbacks() {
            let m = MockItemWorld::new();
            m.push_urand_sequence([5u32, 10, 15]);
            assert_eq!(m.urand_range(0, 100), 5);
            assert_eq!(m.urand_range(0, 100), 10);
            assert_eq!(m.urand_range(0, 100), 15);
            // Sequence drained — fall back to `min`.
            assert_eq!(m.urand_range(3, 100), 3);
        }

        #[test]
        fn talent_tab_defaults_are_zero() {
            let m = MockItemWorld::new();
            let t = m.player_talent_tab(42);
            assert_eq!(t.best_tab, 0);
            assert_eq!(t.meditation_rank, 0);
            m.set_player_talent_tab(
                42,
                TalentTabInfo {
                    best_tab: 1,
                    meditation_rank: 3,
                },
            );
            let t = m.player_talent_tab(42);
            assert_eq!(t.best_tab, 1);
            assert_eq!(t.meditation_rank, 3);
        }

        #[test]
        fn event_log_tracks_calls() {
            let m = MockItemWorld::new();
            let _ = m.query_item_prototypes();
            let _ = m.lookup_spell_entry(42);
            let _ = m.player_item_ctx(100);
            let events = m.take_events();
            assert_eq!(events.len(), 3);
            assert_eq!(events[0], MockItemEvent::QueryItemPrototypes);
            assert_eq!(events[1], MockItemEvent::LookupSpell(42));
            assert_eq!(events[2], MockItemEvent::PlayerItemCtx(100));
        }
    }
}
