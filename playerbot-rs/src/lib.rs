// playerbot-rs — Rust AI module for CMaNGOS playerbots.
//
// Compiles to a static library (libplayerbot_rs.a) linked by the C++ wrapper.
// All public symbols are extern "C" and declared in cpp_wrapper/botffi.h.
#![allow(unsafe_code)]

pub mod bot;
pub mod classes;
pub mod combat;
pub mod commands;
pub mod config;
pub mod data;
pub mod encounters;
pub mod engine;
pub mod factory;
pub mod ffi;
pub mod logging;
pub mod noncombat;
pub mod rtsc;
pub mod world;

use bot::state::BotState;
use ffi::{
    BotCallbacks, BotHandle, SpellId, UnitHandle,
    interface::{BotInterface, RealInterface},
};

// ── Lifecycle ────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn playerbot_init() {}

/// Install (or clear, with null) the global log sink that bridges Rust log
/// calls into CMaNGOS `sLog`. Safe to call before `playerbot_init`.
///
/// # Safety
/// `sink` must either be null or a valid `extern "C" fn(u8, *const c_char)`
/// that remains callable for the lifetime of the process.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_set_log_sink(sink: Option<logging::LogSinkFn>) {
    logging::set_sink(sink);
}

/// Set bot configuration from C++. Must be called before any bots are created.
/// Values that are 0 / 0.0 / false use defaults.
///
/// # Safety
/// Must be called from a single thread (server startup, before bots spawn).
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_set_config(
    react_delay_ms: u32,
    max_wait_for_move_ms: u32,
    eat_hp_pct: f32,
    drink_mana_pct: f32,
    debug: bool,
) {
    let mut cfg = config::BotConfig::default();
    if react_delay_ms > 0 {
        cfg.react_delay_ms = react_delay_ms;
    }
    if max_wait_for_move_ms > 0 {
        cfg.max_wait_for_move_ms = max_wait_for_move_ms;
    }
    if eat_hp_pct > 0.0 {
        cfg.eat_hp_threshold = eat_hp_pct;
    }
    if drink_mana_pct > 0.0 {
        cfg.drink_mana_threshold = drink_mana_pct;
    }
    cfg.debug = debug;
    let _ = config::set(cfg);
}

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

    let interface: Box<dyn BotInterface> =
        Box::new(unsafe { RealInterface::new(bot_handle, *cbs) });

    let snap = interface.get_snapshot();
    let (class, spec) = class_spec_from_snapshot(snap.self_.class_id);

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
    bot::tick::tick(bot, elapsed_ms, minimal);
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
    bot.events.push_back(bot::events::BotEvent::PacketIn {
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
    bot.events.push_back(bot::events::BotEvent::PacketOut {
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
    bot.events.push_back(bot::events::BotEvent::UnitSpellCast {
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
    bot.events.push_back(bot::events::BotEvent::AuraChanged {
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
    bot.events.push_back(bot::events::BotEvent::DamageTaken {
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
/// ComputeSenderSecurity` (DENY_ALL/TALK/INVITE/ALLOW_ALL); each
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
    let bot = unsafe { &*state.cast::<BotState>() };
    factory::clear_inventory(bot.interface.as_ref(), factory::ClearScope::from_mode(mode));
}

/// Initialize consumables on a bot via the Rust factory module.
///
/// `kind`: 0 = potions, 1 = food, 2 = reagents. Unknown values are ignored.
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
    let bot = unsafe { &*state.cast::<BotState>() };
    factory::init_consumables(bot.interface.as_ref(), k);
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
    let bot = unsafe { &*state.cast::<BotState>() };
    factory::reset_progression(bot.interface.as_ref(), k);
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
    let bot = unsafe { &*state.cast::<BotState>() };
    factory::run_misc(bot.interface.as_ref(), k);
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
    let bot = unsafe { &*state.cast::<BotState>() };
    factory::talents::init_talents(bot.interface.as_ref(), spec_no);
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
    let bot = unsafe { &*state.cast::<BotState>() };
    factory::talents::init_talents_tree(bot.interface.as_ref(), incremental);
}

// ── Helpers ───────────────────────────────────────────────────────────────

unsafe fn packet_bytes(data: *const u8, len: u32) -> Vec<u8> {
    if data.is_null() || len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(data, len as usize).to_vec() }
    }
}

fn class_spec_from_snapshot(class_id: u8) -> (bot::state::PlayerClass, bot::state::PlayerSpec) {
    use bot::state::{PlayerClass::{Warrior, Paladin, Hunter, Rogue, Priest, DeathKnight, Shaman, Mage, Warlock, Druid}, PlayerSpec::{WarriorArms, PaladinRetribution, HunterMarksmanship, RogueCombat, PriestHoly, DeathKnightFrost, ShamanEnhancement, MageFrost, WarlockDestruction, DruidRestoration}};
    match class_id {
        1 => (Warrior, WarriorArms),
        2 => (Paladin, PaladinRetribution),
        3 => (Hunter, HunterMarksmanship),
        4 => (Rogue, RogueCombat),
        5 => (Priest, PriestHoly),
        6 => (DeathKnight, DeathKnightFrost),
        7 => (Shaman, ShamanEnhancement),
        8 => (Mage, MageFrost),
        9 => (Warlock, WarlockDestruction),
        11 => (Druid, DruidRestoration),
        _ => (Warrior, WarriorArms),
    }
}
