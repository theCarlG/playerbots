/// Per-bot debug monitor — appends timestamped entries to a log file.
///
/// Activated via `.bot monitor <name>` or whisper `log on`/`log off`.
/// When active, every command received, parsed result, reply, settings
/// mutation, and BT path is appended to `{LogsDir}/playerbot_monitor.txt`.
use crate::bot::state::BotState;
use crate::commands::ChatOrigin;
use crate::engine::snapshot::WorldSnapshotExt;

/// Build the per-bot monitor filename: `monitor_<HANDLE>`.
/// Called once when monitoring is enabled; the result is cached in
/// `BotState::monitor_file_name`.
pub fn make_monitor_file_name(handle: u64) -> String {
    format!("monitor_{handle:X}")
}

/// Write a line to the bot's monitor log file.
/// No-op if `bot.monitor_active` is false.
pub fn monitor_log(bot: &BotState, entry: &str) {
    if !bot.monitor_active {
        return;
    }
    let ts = bot.snap.server_time_ms;
    let line = format!("[{ts}] {entry}\n");
    bot.interface
        .bot_append_log_file(&bot.monitor_file_name, &line);
}

/// Dump full settings snapshot to the monitor log.
pub fn monitor_dump_settings(bot: &BotState) {
    let s = &bot.settings;
    monitor_log(
        bot,
        &format!("=== MONITOR ENABLED (handle=0x{:X}) ===", bot.handle),
    );
    monitor_log(
        bot,
        &format!(
            "CLASS: {:?}  SPEC: {:?}  ROLE: {:?}",
            bot.class, bot.spec, bot.role
        ),
    );
    monitor_log(bot, &format!("MODE: {:?}", s.mode));
    {
        use crate::bot::settings::BotStateKind;
        for kind in [
            BotStateKind::Combat,
            BotStateKind::NonCombat,
            BotStateKind::Reaction,
            BotStateKind::Dead,
        ] {
            let slot = s.strategies.get(kind);
            let desc = slot.describe();
            if !desc.is_empty() {
                monitor_log(bot, &format!("  {}: [{}]", kind.reply_prefix(), desc));
            }
        }
    }
    monitor_log(bot, &format!("REACTIVITY: {:?}", s.reactivity));
    monitor_log(bot, &format!("FORMATION: {:?}", s.follow_formation));
    monitor_log(bot, &format!("STANCE: {}", s.stance));
    monitor_log(bot, &format!("SAVE MANA: {}", s.save_mana));
    monitor_log(bot, &format!("LOOT POLICY: {:?}", s.loot_policy));
    monitor_log(
        bot,
        &format!(
            "FOLLOW DIST: {:.1} / RAID: {:.1}",
            s.follow_distance, s.follow_distance_raid
        ),
    );
    monitor_log(
        bot,
        &format!(
            "RTI: {:?}  CC RTI: {:?}",
            s.preferred_rti_icon, s.preferred_cc_rti_icon
        ),
    );
    monitor_log(bot, &format!("VERBOSE: {}", s.verbose));
    monitor_log(
        bot,
        &format!(
            "MASTER: {:?}  ALIVE: {}  COMBAT: {}",
            bot.master_guid, bot.snap.self_.is_alive, bot.snap.self_.in_combat
        ),
    );
    monitor_log(bot, "=== END SETTINGS DUMP ===");
}

/// Log an incoming raw command text with channel info.
pub fn monitor_command_received(bot: &BotState, raw: &str, sender_guid: u64, origin: &ChatOrigin) {
    let channel = match origin.chat_type {
        0 => "SAY",
        1 => "PARTY",
        2 => "RAID",
        4 => "GUILD",
        6 => "WHISPER",
        _ => "OTHER",
    };
    let addon = if origin.is_addon() { " [ADDON]" } else { "" };
    monitor_log(
        bot,
        &format!("CMD RECV [{channel}{addon}] from 0x{sender_guid:X}: {raw}"),
    );
}

/// Log the parsed `BotCommand`.
pub fn monitor_command_parsed(bot: &BotState, raw: &str, cmd: &str) {
    monitor_log(bot, &format!("CMD PARSED: {raw} -> {cmd}"));
}

/// Log a reply being sent.
pub fn monitor_reply(bot: &BotState, target_guid: u64, msg: &str, is_addon: bool) {
    let channel = if is_addon { "ADDON" } else { "WHISPER" };
    monitor_log(
        bot,
        &format!("REPLY [{channel}] to 0x{target_guid:X}: {msg}"),
    );
}

/// Log a settings change.
pub fn monitor_setting_changed(bot: &BotState, what: &str, old: &str, new: &str) {
    monitor_log(bot, &format!("SETTING CHANGED: {what}: {old} -> {new}"));
}

/// Log the BT path (full root-to-leaf trace).
pub fn monitor_bt_path(bot: &BotState, path: &str) {
    monitor_log(bot, &format!("BT PATH: {path}"));
}

/// Log a tick summary with full game state context.
pub fn monitor_tick_summary(bot: &BotState) {
    let u = &bot.snap.self_;
    let hp_pct = (bot.snap.self_hp_pct() * 100.0) as u32;
    let mana_pct = (bot.snap.self_mana_pct() * 100.0) as u32;
    let power_name = match u.power_type {
        0 => "mana",
        1 => "rage",
        3 => "energy",
        6 => "runic",
        _ => "?",
    };
    let now = bot.snap.server_time_ms;
    let gcd_left = bot.timers.gcd_remaining_ms(now);
    monitor_log(
        bot,
        &format!(
            "TICK: alive={} combat={} hp={}% {}={}/{}({}%) casting={} moving={} form={} gcd={}ms target=0x{:X} attackers={} nearby={}",
            u.is_alive,
            u.in_combat,
            hp_pct,
            power_name,
            u.mana,
            u.max_mana,
            mana_pct,
            u.is_casting,
            u.is_moving,
            u.shapeshift_form,
            gcd_left,
            u.current_target,
            bot.attackers.len(),
            bot.nearby_units.len(),
        ),
    );
    // Log current target info if one exists.
    if u.current_target != 0 {
        let ts = bot.interface.get_unit_snapshot(u.current_target);
        let t_hp = if ts.max_health > 0 {
            (ts.health as f32 / ts.max_health as f32 * 100.0) as u32
        } else {
            0
        };
        let dist = bot.interface.unit_distance(u.current_target);
        let behind = bot.interface.bot_is_behind(u.current_target);
        let los = bot.interface.has_los(u.current_target);
        monitor_log(
            bot,
            &format!(
                "  TARGET: 0x{:X} hp={}% dist={:.1}y casting={} lvl={} behind={} los={}",
                u.current_target, t_hp, dist, ts.is_casting, ts.level, behind, los,
            ),
        );
    }
    // Log attackers list when non-empty.
    if !bot.attackers.is_empty() {
        let atk_str: Vec<String> = bot
            .attackers
            .iter()
            .take(8)
            .map(|a| format!("0x{a:X}"))
            .collect();
        let suffix = if bot.attackers.len() > 8 {
            format!(" (+{})", bot.attackers.len() - 8)
        } else {
            String::new()
        };
        monitor_log(
            bot,
            &format!("  ATTACKERS: [{}]{suffix}", atk_str.join(", ")),
        );
    }
    // Log group members.
    if bot.snap.group_size > 0 {
        let tank = bot.interface.group_get_tank();
        let healer = bot.interface.group_get_healer();
        monitor_log(
            bot,
            &format!(
                "  GROUP: size={} tank={} healer={}",
                bot.snap.group_size,
                tank.map_or("none".to_string(), |t| format!("0x{t:X}")),
                healer.map_or("none".to_string(), |h| format!("0x{h:X}")),
            ),
        );
    }
    // Log current mode, combat order, and strategies (human-readable).
    monitor_log(
        bot,
        &format!(
            "  STATE: mode={:?} react={:?} role={:?} master={}",
            bot.settings.mode,
            bot.settings.reactivity,
            bot.role,
            bot.master_guid
                .map_or("none".to_string(), |m| format!("0x{m:X}")),
        ),
    );
    // Log each strategy slot with human-readable names.
    use crate::bot::settings::BotStateKind;
    for kind in [
        BotStateKind::Combat,
        BotStateKind::NonCombat,
        BotStateKind::Reaction,
        BotStateKind::Dead,
    ] {
        let slot = bot.settings.strategies.get(kind);
        let desc = slot.describe();
        if !desc.is_empty() {
            monitor_log(bot, &format!("  {}: [{}]", kind.reply_prefix(), desc));
        }
    }
    // Log focus/protect targets if set.
    if let Some(f) = bot.settings.focus_target {
        monitor_log(bot, &format!("  FOCUS: 0x{f:X}"));
    }
    if let Some(p) = bot.settings.protect_target {
        monitor_log(bot, &format!("  PROTECT: 0x{p:X}"));
    }
}
