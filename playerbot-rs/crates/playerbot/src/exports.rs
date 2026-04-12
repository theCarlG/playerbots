//! `extern "C"` FFI surface for the C++ playerbot wrapper.
//!
//! Every public symbol is declared in `cpp_wrapper/botffi.h`. The functions
//! here are the *only* `unsafe` code in `playerbot`; everything else compiles
//! under `#![deny(unsafe_code)]`.

use cmangos::{BotCallbacks, BotHandle, SpellId, UnitHandle, VtableWorld, World};

use crate::bot;
use crate::bot::state::BotState;
use crate::commands;
use crate::factory;
use crate::log_error;
use crate::logging;

// ── Lifecycle ────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn playerbot_init() {}

/// Install (or clear, with null) the global log sink that bridges Rust log
/// calls into `CMaNGOS` `sLog`. Safe to call before `playerbot_init`.
///
/// # Safety
/// `sink` must either be null or a valid `extern "C" fn(u8, *const c_char)`
/// that remains callable for the lifetime of the process.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_set_log_sink(sink: Option<logging::LogSinkFn>) {
    logging::set_sink(sink);
}

// Config loading (the old `playerbot_set_config` forwarder) is gone.
// The C++ shim at `cpp_wrapper/BotConfig.{h,cpp}` calls into
// `crate::config::ffi::playerbot_config_load` directly during server
// startup; all other fields are pulled via the `playerbot_config_get_*`
// family exported from that module.

#[unsafe(no_mangle)]
pub extern "C" fn playerbot_shutdown() {}

// ── Per-bot exports ──────────────────────────────────────────────────────

/// Create AI state for one bot. Returns an opaque Box<BotState> as *mut ().
///
/// # Safety
/// `cbs` must point to a valid, fully-initialized `BotCallbacks` that remains
/// valid for the entire lifetime of the returned state object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_create(
    bot_handle: BotHandle,
    cbs: *const BotCallbacks,
) -> *mut () {
    assert!(
        !cbs.is_null(),
        "playerbot_create: null BotCallbacks pointer"
    );

    let interface: Box<dyn World> = Box::new(unsafe { VtableWorld::new(bot_handle, *cbs) });

    let snap = interface.get_snapshot();
    let class = class_from_id(snap.self_.class_id);
    // Derive spec from the bot's actual talent point investment, mirroring
    // PB2's AiFactory::GetPlayerSpecTab.  This is authoritative regardless
    // of how talents were assigned (addon UI premade spec, manual, random).
    let spec_tab = interface.bot_get_spec_tab();
    let spec = spec_from_class_and_tab(class, spec_tab);

    let state = bot::init::create_bot(bot_handle, interface, class, spec);
    Box::into_raw(state).cast()
}

/// Set (or clear) the bot's master. `guid = 0` clears.
///
/// Called from the C++ shim whenever `PlayerbotRust::SetMaster` runs —
/// including the per-tick master auto-claim, explicit master assignment via
/// `PlayerbotMgr`, and random-bot cleanup when the previous master logs out.
/// The guid is the raw `ObjectGuid` value of the master Player.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_set_master(state: *mut (), guid: u64) {
    if state.is_null() {
        return;
    }
    let bot = unsafe { &mut *state.cast::<BotState>() };
    bot.set_master(if guid == 0 { None } else { Some(guid) });
}

/// Read the current master guid (0 = no master).
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_get_master(state: *const ()) -> u64 {
    if state.is_null() {
        return 0;
    }
    let bot = unsafe { &*state.cast::<BotState>() };
    bot.master_guid.unwrap_or(0)
}

/// Reset all per-bot strategy/cache state (pending commands, blackboard,
/// cooldown throttles, encounter FSM). Called from the C++ shim when the
/// master changes or when `PlayerbotMgr` decides a full reinit is needed.
/// Equivalent to PB2's `PlayerbotAI::ResetStrategies`.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_reset_strategies(state: *mut ()) {
    if state.is_null() {
        return;
    }
    let bot = unsafe { &mut *state.cast::<BotState>() };
    bot.reset_strategies();
}

/// Destroy AI state for one bot.
///
/// # Safety
/// `state` must be a pointer from `playerbot_create` for this bot handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_destroy(state: *mut ()) {
    if state.is_null() {
        return;
    }
    unsafe { drop(Box::from_raw(state.cast::<BotState>())) };
}

/// Main AI tick.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_update(state: *mut (), elapsed_ms: u32, minimal: bool) {
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        bot::tick::tick(bot, elapsed_ms, minimal);
    }));
    if let Err(e) = result {
        let msg = if let Some(s) = e.downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = e.downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".to_string()
        };
        log_error!("playerbot_update panic: {msg}");
    }
}

// ── Packet events ─────────────────────────────────────────────────────────

/// # Safety: state valid, data readable for len bytes (or null/0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_packet_in(
    state: *mut (),
    opcode: u16,
    data: *const u8,
    len: u32,
) {
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let bytes = unsafe { packet_bytes(data, len) };
    bot.events
        .lock()
        .unwrap()
        .push_back(bot::events::BotEvent::PacketIn {
            opcode,
            data: bytes,
        });
}

/// # Safety: state valid, data readable for len bytes (or null/0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_packet_out(
    state: *mut (),
    opcode: u16,
    data: *const u8,
    len: u32,
) {
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let bytes = unsafe { packet_bytes(data, len) };
    bot.events
        .lock()
        .unwrap()
        .push_back(bot::events::BotEvent::PacketOut {
            opcode,
            data: bytes,
        });
}

// ── Push combat events ────────────────────────────────────────────────────

/// # Safety: state valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_unit_spell_cast(
    state: *mut (),
    caster: UnitHandle,
    spell_id: u32,
    target: UnitHandle,
    success: bool,
) {
    let bot = unsafe { &mut *state.cast::<BotState>() };
    bot.events
        .lock()
        .unwrap()
        .push_back(bot::events::BotEvent::UnitSpellCast {
            caster,
            spell_id: SpellId(spell_id),
            target,
            success,
        });
}

/// RTSC spell position — called when spell 30758 is cast on ground by the master.
/// The C++ side extracts the destination position from `SpellCastTargets` and calls this.
///
/// # Safety: state valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_rtsc_spell(state: *mut (), x: f32, y: f32, z: f32) {
    let bot = unsafe { &mut *state.cast::<BotState>() };
    bot.pending_commands
        .lock()
        .unwrap()
        .push_back(commands::PendingCommand::internal(
            commands::BotCommand::RtscSpellPosition(x, y, z),
        ));
}

/// # Safety: state valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_aura_changed(
    state: *mut (),
    unit: UnitHandle,
    spell_id: u32,
    applied: bool,
    stacks: u8,
) {
    let bot = unsafe { &mut *state.cast::<BotState>() };
    bot.events
        .lock()
        .unwrap()
        .push_back(bot::events::BotEvent::AuraChanged {
            unit,
            spell_id: SpellId(spell_id),
            applied,
            stacks,
        });
}

/// # Safety: state valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_unit_died(
    state: *mut (),
    victim: UnitHandle,
    killer: UnitHandle,
) {
    let bot = unsafe { &mut *state.cast::<BotState>() };
    bot.events
        .lock()
        .unwrap()
        .push_back(bot::events::BotEvent::UnitDied { victim, killer });
}

/// # Safety: state valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_damage_taken(
    state: *mut (),
    damage: u32,
    spell_id: u32,
    dealer: UnitHandle,
) {
    let bot = unsafe { &mut *state.cast::<BotState>() };
    bot.events
        .lock()
        .unwrap()
        .push_back(bot::events::BotEvent::DamageTaken {
            damage,
            spell_id: SpellId(spell_id),
            dealer,
        });
}

// ── Chat command injection ────────────────────────────────────────────────

/// Inject a chat command into a bot's pending command queue.
///
/// Called from C++ when a player sends chat to the bot (whisper, party,
/// say, etc.). `sender_guid` is the `ObjectGuid` raw value of the
/// commanding player (0 = internal/system, bypasses gating). `security`
/// is a `commands::SecurityLevel` byte computed by `PlayerbotRust::
/// ComputeSenderSecurity` (`DENY_ALL/TALK/INVITE/ALLOW_ALL`); each
/// `BotCommand` declares the minimum level it requires.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
/// `text` must be a valid null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_chat_command(
    state: *mut (),
    sender_guid: u64,
    security: u8,
    chat_type: u32,
    lang: u32,
    text: *const std::os::raw::c_char,
) {
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let c_str = unsafe { std::ffi::CStr::from_ptr(text) };
    if let Ok(text) = c_str.to_str() {
        let sec = commands::SecurityLevel::from_raw(security);
        let origin = commands::ChatOrigin::new(chat_type, lang);
        commands::preprocess::preprocess_and_enqueue(bot, sender_guid, sec, origin, text);
    }
}

// ── Monitor toggle ──────────────────────────────────────────────────────

/// Toggle per-bot debug monitor. Returns `true` if now ON, `false` if OFF.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_toggle_monitor(state: *mut ()) -> bool {
    if state.is_null() {
        return false;
    }
    let bot = unsafe { &mut *state.cast::<BotState>() };
    if bot.monitor_active {
        // Log the disable message while still active, then turn off.
        bot::monitor::monitor_log(bot, "=== MONITOR DISABLED ===");
        bot.monitor_active = false;
    } else {
        bot.monitor_active = true;
        bot::monitor::monitor_dump_settings(bot);
    }
    bot.monitor_active
}

// ── Global coordination tick ──────────────────────────────────────────────

/// Called from sRandomPlayerbotMgr.UpdateAI (world thread, existing `CMaNGOS` hook).
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_world_update(_elapsed_ms: u32) {
    // Future: flush stale GroupState entries, update activity metrics.
}

// ── Factory entry points ──────────────────────────────────────────────────

/// Clear bot inventory. Called from C++ `PlayerbotFactory::ClearInventory` /
/// `ClearAllItems` via the bot's Rust state handle.
///
/// `mode`: 0 = equipped + carried bags (bank intact), 1 = everything.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_factory_clear_inventory(state: *mut (), mode: u8) {
    if state.is_null() {
        return;
    }
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let mut tx = factory::FactoryTransaction::new(bot.interface.as_mut());
    factory::clear_inventory(&mut tx, factory::ClearScope::from_mode(mode));
    tx.commit();
}

/// Initialize consumables on a bot via the Rust factory module.
///
/// `kind`: 0 = potions, 1 = food, 2 = reagents, 3 = additional (class-specific
/// weapon oils / stones / poisons). Unknown values are ignored.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_factory_init_consumables(state: *mut (), kind: u8) {
    if state.is_null() {
        return;
    }
    let Some(k) = factory::ConsumableKind::from_kind(kind) else {
        return;
    };
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let mut tx = factory::FactoryTransaction::new(bot.interface.as_mut());
    factory::init_consumables(&mut tx, k);
    tx.commit();
}

/// Wipe a slice of the bot's progression (trade skills, spellbook, quest log).
///
/// `kind`: 0 = trade skills, 1 = spells, 2 = quests. Unknown values are ignored.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_factory_reset_progression(state: *mut (), kind: u8) {
    if state.is_null() {
        return;
    }
    let Some(k) = factory::ProgressionKind::from_kind(kind) else {
        return;
    };
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let mut tx = factory::FactoryTransaction::new(bot.interface.as_mut());
    factory::reset_progression(&mut tx, k);
    tx.commit();
}

/// Miscellaneous factory step (cancel auras, hand out trade-skill tool kit).
///
/// `kind`: 0 = cancel auras, 1 = init skill tool kit. Unknown values are ignored.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_factory_misc(state: *mut (), kind: u8) {
    if state.is_null() {
        return;
    }
    let Some(k) = factory::MiscKind::from_kind(kind) else {
        return;
    };
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let mut tx = factory::FactoryTransaction::new(bot.interface.as_mut());
    factory::run_misc(&mut tx, k);
    tx.commit();
}

/// Learn talents for the given spec tab (0..2). Matches the old
/// `PlayerbotFactory::InitTalents` policy — randomly invests 5 points per
/// row until the bot's free-talent-points budget is spent.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_factory_init_talents(state: *mut (), spec_no: u32) {
    if state.is_null() {
        return;
    }
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let mut tx = factory::FactoryTransaction::new(bot.interface.as_mut());
    factory::talents::init_talents(&mut tx, spec_no);
    tx.commit();
}

/// Pick (or recall) a talent spec for the bot and spend all of its talent
/// points, matching the old `PlayerbotFactory::InitTalentsTree` policy.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_factory_init_talents_tree(state: *mut (), incremental: bool) {
    if state.is_null() {
        return;
    }
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let mut tx = factory::FactoryTransaction::new(bot.interface.as_mut());
    factory::talents::init_talents_tree(&mut tx, incremental);
    tx.commit();
}

/// Top up ammo / food / potions / reagents / weapon consumables on a bot
/// and optionally save to DB. Ports `PlayerbotFactory::Refresh`.
///
/// Gated on the bot's item cheat bit: no-op when the bot does not have the
/// item cheat enabled.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_factory_refresh(state: *mut ()) {
    if state.is_null() {
        return;
    }
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let mut tx = factory::FactoryTransaction::new(bot.interface.as_mut());
    factory::refresh(&mut tx);
    tx.commit();
}

/// Reset the chassis before a factory re-roll: resurrect, combat-stop, set
/// level + XP (unless random levels are disabled), apply helmet / cloak
/// display flags. Ports `PlayerbotFactory::Prepare`.
///
/// `level` is the target level handed to the C++ `PlayerbotFactory` ctor —
/// it is not read from the bot because the caller may want to adjust the bot
/// before doing anything else.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_factory_prepare(state: *mut (), level: u32) {
    if state.is_null() {
        return;
    }
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let mut tx = factory::FactoryTransaction::new(bot.interface.as_mut());
    factory::prepare(&mut tx, level);
    tx.commit();
}

/// Reward every eligible quest from a caller-owned list. Ports
/// `PlayerbotFactory::InitQuests(std::list<uint32>&)`; the C++ side passes
/// `specialQuestIds` (built in `PlayerbotFactory::Init`) as a contiguous
/// array. `ids`/`len` describe that buffer — when `len == 0` the call is a
/// no-op even if `ids` is null.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`, and `ids` must
/// point at `len` contiguous `u32` values for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_factory_init_quests(
    state: *mut (),
    ids: *const u32,
    len: usize,
) {
    if state.is_null() {
        return;
    }
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let quest_ids: &[u32] = if len == 0 || ids.is_null() {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(ids, len) }
    };
    let mut tx = factory::FactoryTransaction::new(bot.interface.as_mut());
    factory::init_quests(&mut tx, quest_ids);
    tx.commit();
}

/// Kick off a random arena-team creation pass when the bot qualifies.
/// Ports `PlayerbotFactory::InitArenaTeam`: gated on the bot's owning
/// account being in the live random-bot account list and the current
/// tracked arena-team count being below `AiPlayerbot.RandomBotArenaTeamCount`.
/// On Classic this is a compile-time no-op.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_factory_init_arena_team(state: *mut ()) {
    if state.is_null() {
        return;
    }
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let mut tx = factory::FactoryTransaction::new(bot.interface.as_mut());
    factory::init_arena_team(&mut tx);
    tx.commit();
}

/// Top up a guild tabard on an already-guilded bot, otherwise filter the
/// random-bot guild pool by team and add the bot at a random rank. Ports
/// `PlayerbotFactory::InitGuild`. When the tracked guild pool is below
/// `AiPlayerbot.RandomBotGuildCount` this call also kicks off a fresh
/// `playerbot_random_factory_create_guilds()` pass before attempting the
/// join.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_factory_init_guild(state: *mut ()) {
    if state.is_null() {
        return;
    }
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let mut tx = factory::FactoryTransaction::new(bot.interface.as_mut());
    factory::init_guild(&mut tx);
    tx.commit();
}

/// Run `InitSkills` (weapons/armor/riding) followed by `InitTradeSkills`
/// (professions + first aid/fishing/cooking + the TBC+ armorsmith chain +
/// the opaque trainer-iteration loop). Ports
/// `PlayerbotFactory::InitAllSkills`. The profession pair is cached in
/// the per-bot KV store so re-rolls keep the same two professions.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_factory_init_all_skills(state: *mut ()) {
    if state.is_null() {
        return;
    }
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let mut tx = factory::FactoryTransaction::new(bot.interface.as_mut());
    factory::init_all_skills(&mut tx);
    tx.commit();
}

/// Run only the tradeskill half of `InitAllSkills` (profession
/// assignment + the three universal secondaries + the TBC+ armorsmith
/// chain + the opaque trainer iteration), skipping the weapons / armor
/// / riding pass. Ports `PlayerbotFactory::InitTradeSkills`.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_factory_init_trade_skills(state: *mut ()) {
    if state.is_null() {
        return;
    }
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let mut tx = factory::FactoryTransaction::new(bot.interface.as_mut());
    factory::init_trade_skills_only(&mut tx);
    tx.commit();
}

/// Re-roll the bot's equipped gear from the itempool cache. Ports
/// `PlayerbotFactory::InitEquipment`. `flags` packs the four legacy
/// boolean parameters via [`factory::equipment_flags`]:
///
///   * bit 0 = `incremental`
///   * bit 1 = `syncWithMaster`
///   * bit 2 = `progressive`
///   * bit 3 = `partialUpgrade`
///
/// `item_quality` pins the search quality tier when non-zero (legacy
/// `.py equip <quality>` chat command path); `0` means "follow the
/// progressive/incremental ladder".
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_factory_init_equipment(
    state: *mut (),
    flags: u32,
    item_quality: u32,
) {
    if state.is_null() {
        return;
    }
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let mut tx = factory::FactoryTransaction::new(bot.interface.as_mut());
    factory::init_equipment(&mut tx, flags, item_quality);
    tx.commit();
}

/// Hunter-only pet creation + stat refresh + mass-autocast + force-
/// dismiss pass. Ports `PlayerbotFactory::InitPet`. Every other class
/// is a no-op.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_factory_init_pet(state: *mut ()) {
    if state.is_null() {
        return;
    }
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let mut tx = factory::FactoryTransaction::new(bot.interface.as_mut());
    factory::init_pet(&mut tx);
    tx.commit();
}

/// Per-class / per-expansion pet spell learning. Ports
/// `PlayerbotFactory::InitPetSpells`. Hunter + warlock only — every
/// other class is a no-op.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_factory_init_pet_spells(state: *mut ()) {
    if state.is_null() {
        return;
    }
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let mut tx = factory::FactoryTransaction::new(bot.interface.as_mut());
    factory::init_pet_spells(&mut tx);
    tx.commit();
}

/// Top-level "re-roll this bot from scratch" pass. Ports
/// `PlayerbotFactory::Randomize(bool incremental, bool syncWithMaster)` —
/// chains `Prepare`, the two random-bot gates, and every other `Init*`
/// step into a single call. See [`factory::randomize::randomize`] for
/// the full control flow.
///
/// `level` is the target level from the PlayerbotFactory ctor in C++;
/// `incremental` and `sync_with_master` are the two boolean params on
/// the PB2 method; `item_quality` is the `.py equip <quality>` override
/// (0 = follow the progressive ladder).
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_factory_randomize(
    state: *mut (),
    level: u32,
    incremental: bool,
    sync_with_master: bool,
    item_quality: u32,
) {
    if state.is_null() {
        return;
    }
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let mut tx = factory::FactoryTransaction::new(bot.interface.as_mut());
    factory::randomize(&mut tx, level, incremental, sync_with_master, item_quality);
    tx.commit();
}

/// Top up ranged-weapon ammo for warrior/rogue/hunter bots. Ports
/// `PlayerbotFactory::InitAmmo`. Reads class + level from a snapshot on
/// the Rust side so C++ callers don't need to marshal them.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_factory_init_ammo(state: *mut ()) {
    if state.is_null() {
        return;
    }
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let mut tx = factory::FactoryTransaction::new(bot.interface.as_mut());
    let snap = tx.get_snapshot();
    let class = snap.self_.class_id;
    let level = u32::from(snap.self_.level);
    factory::ammo::init_ammo(&mut tx, class, level);
    tx.commit();
}

/// Enchant every equipped slot on the bot, matching PB2's
/// `PlayerbotFactory::EnchantEquipment`. The full body — enchant-container
/// load, per-slot iteration, spec-tab lookup, and the actual item
/// enchantment calls — lives on the C++ bridge side because it still
/// touches live `Item*` pointers. The Rust side is a thin wrapper that
/// forwards into `factory_enchant_all_equipment`.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_factory_enchant_equipment(state: *mut ()) {
    if state.is_null() {
        return;
    }
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let tx = factory::FactoryTransaction::new(bot.interface.as_mut());
    tx.factory_enchant_all_equipment();
    tx.commit();
}

/// Socket a random gem into every empty socket on the bot's equipment.
/// Ports `PlayerbotFactory::InitGems`. The full body — socket iteration,
/// `sRandomItemMgr::GetGemsList`, stat-weight check, `CMSG_SOCKET_GEMS`
/// dispatch — lives on the C++ bridge side. Classic is a compile-time
/// no-op because the game has no gems pre-TBC.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_factory_init_gems(state: *mut ()) {
    if state.is_null() {
        return;
    }
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let tx = factory::FactoryTransaction::new(bot.interface.as_mut());
    tx.factory_init_all_gems();
    tx.commit();
}

// ── Manager command dispatch ─────────────────────────────────────────────

/// Try to handle a `.bot <name> <cmd> [param]` command on the Rust side.
///
/// Returns a heap-allocated C string if the command was handled (caller must
/// free with [`playerbot_free_string`]), or null if the C++ side should
/// process the command instead.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
/// `cmd` and `param` must be valid null-terminated C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_mgr_bot_command(
    state: *mut (),
    cmd: *const std::os::raw::c_char,
    param: *const std::os::raw::c_char,
    master_level: u32,
) -> *mut std::os::raw::c_char {
    if state.is_null() || cmd.is_null() || param.is_null() {
        return std::ptr::null_mut();
    }
    let cmd_str = unsafe { std::ffi::CStr::from_ptr(cmd) };
    let param_str = unsafe { std::ffi::CStr::from_ptr(param) };
    let (Ok(cmd_str), Ok(param_str)) = (cmd_str.to_str(), param_str.to_str()) else {
        return std::ptr::null_mut();
    };

    let bot = unsafe { &mut *state.cast::<BotState>() };
    match crate::manager::dispatch_bot_command(
        bot.interface.as_mut(),
        cmd_str,
        param_str,
        master_level,
    ) {
        crate::manager::BotCommandResult::Handled(msg) => {
            match std::ffi::CString::new(msg) {
                Ok(cs) => cs.into_raw(),
                Err(_) => std::ptr::null_mut(),
            }
        }
        crate::manager::BotCommandResult::NotHandled => std::ptr::null_mut(),
    }
}

/// Free a string previously returned by [`playerbot_mgr_bot_command`].
///
/// # Safety
/// `ptr` must be null or a pointer returned by `playerbot_mgr_bot_command`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_free_string(ptr: *mut std::os::raw::c_char) {
    if !ptr.is_null() {
        drop(unsafe { std::ffi::CString::from_raw(ptr) });
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

unsafe fn packet_bytes(data: *const u8, len: u32) -> Vec<u8> {
    if data.is_null() || len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(data, len as usize).to_vec() }
    }
}

fn class_from_id(class_id: u8) -> bot::state::PlayerClass {
    use bot::state::PlayerClass::{
        DeathKnight, Druid, Hunter, Mage, Paladin, Priest, Rogue, Shaman, Warlock, Warrior,
    };
    match class_id {
        1 => Warrior,
        2 => Paladin,
        3 => Hunter,
        4 => Rogue,
        5 => Priest,
        6 => DeathKnight,
        7 => Shaman,
        8 => Mage,
        9 => Warlock,
        11 => Druid,
        _ => Warrior,
    }
}

/// Map a (class, talent-tab-index) pair to the concrete `PlayerSpec`.
///
/// Talent tab indices (0/1/2) follow the `WoW` DBC `TalentTab` ordering.
/// The stored `specNo` from `sRandomPlayerbotMgr` uses the same numbering.
fn spec_from_class_and_tab(class: bot::state::PlayerClass, tab: u32) -> bot::state::PlayerSpec {
    use bot::state::PlayerClass::{
        DeathKnight, Druid, Hunter, Mage, Paladin, Priest, Rogue, Shaman, Warlock, Warrior,
    };
    use bot::state::PlayerSpec::{
        DeathKnightBlood, DeathKnightFrost, DeathKnightUnholy, DruidBalance, DruidFeral,
        DruidRestoration, HunterBeastMastery, HunterMarksmanship, HunterSurvival, MageArcane,
        MageFire, MageFrost, PaladinHoly, PaladinProtection, PaladinRetribution, PriestDiscipline,
        PriestHoly, PriestShadow, RogueAssassination, RogueCombat, RogueSubtlety, ShamanElemental,
        ShamanEnhancement, ShamanRestoration, WarlockAffliction, WarlockDemonology,
        WarlockDestruction, WarriorArms, WarriorFury, WarriorProtection,
    };
    match (class, tab) {
        (Warrior, 0) => WarriorArms,
        (Warrior, 1) => WarriorFury,
        (Warrior, 2) => WarriorProtection,
        (Paladin, 0) => PaladinHoly,
        (Paladin, 1) => PaladinProtection,
        (Paladin, 2) => PaladinRetribution,
        (Hunter, 0) => HunterBeastMastery,
        (Hunter, 1) => HunterMarksmanship,
        (Hunter, 2) => HunterSurvival,
        (Rogue, 0) => RogueAssassination,
        (Rogue, 1) => RogueCombat,
        (Rogue, 2) => RogueSubtlety,
        (Priest, 0) => PriestDiscipline,
        (Priest, 1) => PriestHoly,
        (Priest, 2) => PriestShadow,
        (DeathKnight, 0) => DeathKnightBlood,
        (DeathKnight, 1) => DeathKnightFrost,
        (DeathKnight, 2) => DeathKnightUnholy,
        (Shaman, 0) => ShamanElemental,
        (Shaman, 1) => ShamanEnhancement,
        (Shaman, 2) => ShamanRestoration,
        (Mage, 0) => MageArcane,
        (Mage, 1) => MageFire,
        (Mage, 2) => MageFrost,
        (Warlock, 0) => WarlockAffliction,
        (Warlock, 1) => WarlockDemonology,
        (Warlock, 2) => WarlockDestruction,
        (Druid, 0) => DruidBalance,
        (Druid, 1) => DruidFeral,
        (Druid, 2) => DruidRestoration,
        // Fallback for unknown tab values — use the class default.
        (Warrior, _) => WarriorArms,
        (Paladin, _) => PaladinRetribution,
        (Hunter, _) => HunterMarksmanship,
        (Rogue, _) => RogueCombat,
        (Priest, _) => PriestHoly,
        (DeathKnight, _) => DeathKnightFrost,
        (Shaman, _) => ShamanEnhancement,
        (Mage, _) => MageFrost,
        (Warlock, _) => WarlockDestruction,
        (Druid, _) => DruidRestoration,
    }
}
