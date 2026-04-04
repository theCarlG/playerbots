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
pub mod ffi;
pub mod noncombat;
pub mod world;

use bot::state::BotState;
use ffi::{
    interface::{BotInterface, RealInterface},
    BotCallbacks, BotHandle, SpellId, UnitHandle,
};

// ── Lifecycle ────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn playerbot_init() {}

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
    if react_delay_ms > 0       { cfg.react_delay_ms = react_delay_ms; }
    if max_wait_for_move_ms > 0 { cfg.max_wait_for_move_ms = max_wait_for_move_ms; }
    if eat_hp_pct > 0.0         { cfg.eat_hp_threshold = eat_hp_pct; }
    if drink_mana_pct > 0.0     { cfg.drink_mana_threshold = drink_mana_pct; }
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
    assert!(!cbs.is_null(), "playerbot_create: null BotCallbacks pointer");

    let interface: Box<dyn BotInterface> = Box::new(unsafe {
        RealInterface::new(bot_handle, *cbs)
    });

    let snap = interface.get_snapshot();
    let (class, spec) = class_spec_from_snapshot(snap.self_.class_id);

    let state = bot::init::create_bot(bot_handle, interface, class, spec);
    Box::into_raw(state).cast()
}

/// Destroy AI state for one bot.
///
/// # Safety
/// `state` must be a pointer from `playerbot_create` for this bot handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_destroy(state: *mut ()) {
    if state.is_null() { return; }
    unsafe { drop(Box::from_raw(state.cast::<BotState>())) };
}

/// Main AI tick.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_update(
    state: *mut (),
    elapsed_ms: u32,
    minimal: bool,
) {
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
    bot.events.push_back(bot::events::BotEvent::PacketIn { opcode, data: bytes });
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
    bot.events.push_back(bot::events::BotEvent::PacketOut { opcode, data: bytes });
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
    bot.events.push_back(bot::events::BotEvent::UnitSpellCast { caster, spell_id: SpellId(spell_id), target, success });
}

/// RTSC spell position — called when spell 30758 is cast on ground by the master.
/// The C++ side extracts the destination position from SpellCastTargets and calls this.
///
/// # Safety: state valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_rtsc_spell(
    state: *mut (),
    x: f32,
    y: f32,
    z: f32,
) {
    let bot = unsafe { &mut *state.cast::<BotState>() };
    bot.pending_commands.push_back(
        commands::PendingCommand::internal(commands::BotCommand::RtscSpellPosition(x, y, z)),
    );
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
    bot.events.push_back(bot::events::BotEvent::AuraChanged { unit, spell_id: SpellId(spell_id), applied, stacks });
}

/// # Safety: state valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_unit_died(
    state: *mut (),
    victim: UnitHandle,
    killer: UnitHandle,
) {
    let bot = unsafe { &mut *state.cast::<BotState>() };
    bot.events.push_back(bot::events::BotEvent::UnitDied { victim, killer });
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
    bot.events.push_back(bot::events::BotEvent::DamageTaken { damage, spell_id: SpellId(spell_id), dealer });
}

// ── Chat command injection ────────────────────────────────────────────────

/// Inject a chat command into a bot's pending command queue.
///
/// Called from C++ when a player whispers a command to the bot.
/// `sender_guid` is the ObjectGuid raw value of the commanding player
/// (0 = internal/system). `privileged` is non-zero if the sender is
/// owner/party-leader/GM — unprivileged commands are silently dropped.
///
/// # Safety
/// `state` must be a valid pointer from `playerbot_create`.
/// `text` must be a valid null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_chat_command(
    state: *mut (),
    sender_guid: u64,
    privileged: u8,
    text: *const std::os::raw::c_char,
) {
    let bot = unsafe { &mut *state.cast::<BotState>() };
    let c_str = unsafe { std::ffi::CStr::from_ptr(text) };
    if let Ok(text) = c_str.to_str() {
        if let Some(cmd) = commands::parser::parse(text) {
            let pc = if sender_guid == 0 {
                commands::PendingCommand::internal(cmd)
            } else {
                commands::PendingCommand::external(sender_guid, privileged != 0, cmd)
            };
            bot.pending_commands.push_back(pc);
        }
    }
}

// ── Global coordination tick ──────────────────────────────────────────────

/// Called from sRandomPlayerbotMgr.UpdateAI (world thread, existing CMaNGOS hook).
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_world_update(_elapsed_ms: u32) {
    // Future: flush stale GroupState entries, update activity metrics.
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
    use bot::state::{PlayerClass::*, PlayerSpec::*};
    match class_id {
        1  => (Warrior,      WarriorArms),
        2  => (Paladin,      PaladinRetribution),
        3  => (Hunter,       HunterMarksmanship),
        4  => (Rogue,        RogueCombat),
        5  => (Priest,       PriestHoly),
        6  => (DeathKnight,  DeathKnightFrost),
        7  => (Shaman,       ShamanEnhancement),
        8  => (Mage,         MageFrost),
        9  => (Warlock,      WarlockDestruction),
        11 => (Druid,        DruidRestoration),
        _  => (Warrior,      WarriorArms),
    }
}
