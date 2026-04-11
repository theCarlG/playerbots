//! Console / chat command surface for the random-playerbot manager.
//!
//! Ported from the C++ `HandlePlayerbotConsoleCommand` dispatch table
//! plus the pair-wise per-command handlers (`HandleConsoleReset`,
//! `HandleRandomizeFirst`, …). The Rust version splits the two
//! sub-tables into [`ConsoleCommand`] (manager-scoped) and
//! [`BotCommand`] (per-bot-scoped) and surfaces a [`CommandOutput`]
//! that the FFI layer hands back to the C++ handler to be written into
//! the chat log / RA session.
//!
//! The parser itself is intentionally loose — it matches the legacy
//! C++ `cmd.find(prefix) != 0` behaviour, so command names with
//! trailing arguments are tolerated the same way the old code tolerated
//! them.

use cmangos::RandomMgrWorld;

use super::events::EventCache;
use super::state::RandomMgrState;
use super::stats::print_stats;

/// Manager-scoped console command. Matches the outer `handlers` map in
/// `RandomPlayerbotMgr::HandlePlayerbotConsoleCommand`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ConsoleCommand {
    /// `help` / `help commands` / `help <subcommand>`.
    Help,
    /// `reset` — wipe events and in-memory state.
    Reset,
    /// `stats <guid>` — print bot population stats.
    Stats,
    /// `update` — force an `UpdateAIInternal(0)` pass.
    Update,
    /// `pid p i d` — re-tune the PID controller.
    Pid,
    /// `diff` / `diff p e` — show or set the PID diff setpoints.
    Diff,
    /// `clean map` — unload & reload every map (C++-only side effect,
    /// Rust just emits the acknowledgement).
    CleanMap,
    /// `login debug` — toggle the login worker's debug flag.
    LoginDebug,
}

/// Per-bot console command. Matches the inner `playerHandlers` map in
/// `HandlePlayerbotConsoleCommand`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BotCommand {
    /// `init <name>` — `RandomizeFirst`.
    Init,
    /// `upgrade <name>` — `UpdateGearSpells`.
    Upgrade,
    /// `refresh <name>` — `Refresh`.
    Refresh,
    /// `teleport <name>` — `RandomTeleportForLevel`.
    Teleport,
    /// `rpg <name>` — `RandomTeleportForRpg`.
    Rpg,
    /// `revive <name>` — `Revive`.
    Revive,
    /// `grind <name>` — `RandomTeleport`.
    Grind,
    /// `change_strategy <name>` — `ChangeStrategy`.
    ChangeStrategy,
    /// `remove <name>` — `Remove`.
    Remove,
}

/// Parsed command.
#[derive(Clone, Debug, PartialEq)]
pub enum ParsedCommand {
    /// A console command with the raw argument tail (empty string when
    /// no args were supplied).
    Console { cmd: ConsoleCommand, args: String },
    /// A per-bot command. `name_prefix` is the bot name prefix the C++
    /// code uses (`%` means "all bots").
    Bot {
        cmd: BotCommand,
        name_prefix: String,
        params: String,
    },
    /// The input did not match any known command.
    Unknown,
}

/// Output the FFI layer drains after a command runs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandOutput {
    /// Lines to write back to the caller (chat handler / CLI).
    pub messages: Vec<String>,
}

impl CommandOutput {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push<S: Into<String>>(&mut self, msg: S) {
        self.messages.push(msg.into());
    }
}

/// Parse a raw command string. Matches the C++ dispatch logic:
///
/// 1. Try every console prefix (`help`, `reset`, `stats`, `update`,
///    `pid `, `diff`, `clean map`, `login debug`). Order matters because
///    `HandlePlayerbotConsoleCommand` walks the map in insertion order
///    and returns on the first match.
/// 2. Try every per-bot prefix (`init`, `upgrade`, `refresh`, `teleport`,
///    `rpg`, `revive`, `grind`, `change_strategy`, `remove`).
/// 3. Fall through to [`ParsedCommand::Unknown`].
#[must_use]
pub fn parse(input: &str) -> ParsedCommand {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return ParsedCommand::Console {
            cmd: ConsoleCommand::Help,
            args: String::new(),
        };
    }

    // Check two-word prefixes first so `login debug` doesn't get
    // swallowed by `login`.
    let two_word: &[(&str, ConsoleCommand)] = &[
        ("clean map", ConsoleCommand::CleanMap),
        ("login debug", ConsoleCommand::LoginDebug),
    ];
    for (prefix, cmd) in two_word {
        if let Some(rest) = match_prefix(trimmed, prefix) {
            return ParsedCommand::Console {
                cmd: *cmd,
                args: rest.to_string(),
            };
        }
    }

    let console: &[(&str, ConsoleCommand)] = &[
        ("help", ConsoleCommand::Help),
        ("reset", ConsoleCommand::Reset),
        ("stats", ConsoleCommand::Stats),
        ("update", ConsoleCommand::Update),
        ("pid", ConsoleCommand::Pid),
        ("diff", ConsoleCommand::Diff),
    ];
    for (prefix, cmd) in console {
        if let Some(rest) = match_prefix(trimmed, prefix) {
            return ParsedCommand::Console {
                cmd: *cmd,
                args: rest.to_string(),
            };
        }
    }

    let bot: &[(&str, BotCommand)] = &[
        ("init", BotCommand::Init),
        ("upgrade", BotCommand::Upgrade),
        ("refresh", BotCommand::Refresh),
        ("teleport", BotCommand::Teleport),
        ("rpg", BotCommand::Rpg),
        ("revive", BotCommand::Revive),
        ("grind", BotCommand::Grind),
        ("change_strategy", BotCommand::ChangeStrategy),
        ("remove", BotCommand::Remove),
    ];
    for (prefix, cmd) in bot {
        if let Some(rest) = match_prefix(trimmed, prefix) {
            let (name, params) = split_name_params(rest);
            return ParsedCommand::Bot {
                cmd: *cmd,
                name_prefix: name,
                params,
            };
        }
    }

    ParsedCommand::Unknown
}

/// Extract the caller-supplied epoch-seconds value from the stats
/// command args tail, or return `0` when none was supplied. The C++
/// version read this from `time(nullptr)` on the main thread — the
/// Rust FFI layer passes it in via the args tail so the cache-hit
/// paths in `print_stats` use a consistent "now" value.
fn now_epoch_s_from_args(args: &str) -> u32 {
    args.split_ascii_whitespace()
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0)
}

/// Match `prefix` against the front of `input`, accepting either exact
/// equality or a prefix followed by a space. Returns the remaining
/// slice (possibly empty) on success.
fn match_prefix<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    if input == prefix {
        return Some("");
    }
    let with_space = format!("{prefix} ");
    if let Some(rest) = input.strip_prefix(&with_space) {
        return Some(rest);
    }
    None
}

/// Split `rest` into `(name, params)` the same way the C++ code does:
/// the first whitespace-separated token is the name, everything after
/// is the parameter tail. An empty `rest` produces `("%", "")`.
fn split_name_params(rest: &str) -> (String, String) {
    let trimmed = rest.trim();
    if trimmed.is_empty() {
        return ("%".into(), String::new());
    }
    match trimmed.split_once(' ') {
        Some((name, params)) => (name.into(), params.trim().into()),
        None => (trimmed.into(), String::new()),
    }
}

/// Returns the help table — mirrors `RandomPlayerbotMgr::GetCommandTexts`.
#[must_use]
pub fn help_table() -> &'static [(&'static str, &'static str)] {
    &[
        ("init", "Randomize the first available bot.\nUsage: init"),
        ("upgrade", "Update gear and spells for all random bots.\nUsage: upgrade"),
        ("refresh", "Log out and log in all random bots to refresh their status.\nUsage: refresh"),
        ("teleport", "Teleport all random bots to a location suitable for their level.\nUsage: teleport"),
        ("rpg", "Teleport all random bots to a location for RPG activities.\nUsage: rpg"),
        ("revive", "Revive all dead random bots.\nUsage: revive"),
        ("grind", "Teleport all random bots to a grinding location.\nUsage: grind"),
        ("change_strategy", "Change the AI strategy for random bots.\nUsage: change_strategy <botname> <strategy>"),
        ("remove", "Remove a random bot from the server.\nUsage: remove <botname>"),
        ("reset", "Reset all random bots and clear event cache.\nUsage: reset"),
        ("diff", "Show server performance metrics.\nUsage: diff [player_diff] [empty_diff]"),
        ("stats", "Print bot statistics.\nUsage: stats"),
        ("update", "Trigger immediate bot AI update.\nUsage: update"),
        ("pid", "Adjust PID controller values.\nUsage: pid p i d"),
        ("clean map", "Unload and reload map files.\nUsage: clean map"),
        ("login debug", "Toggle login debug mode.\nUsage: login debug"),
        ("help", "Show help for commands.\nUsage: help [command]"),
    ]
}

/// Handle a parsed console command against the provided state / world.
///
/// This is the Rust equivalent of the `HandleConsole*` C++ helpers,
/// folded into a single dispatch function. Side effects that touch
/// global C++ state (the `update` force-tick, the login worker debug
/// flag, the map reload) are delegated back to the caller via the
/// output messages — the FFI layer reads the messages and performs
/// the matching side effect on the main thread.
pub fn run_console_command(
    cmd: ConsoleCommand,
    args: &str,
    state: &mut RandomMgrState,
    world: &dyn RandomMgrWorld,
) -> CommandOutput {
    let mut out = CommandOutput::new();
    match cmd {
        ConsoleCommand::Help => {
            let trimmed = args.trim();
            if trimmed.is_empty() {
                out.push("Type 'help commands' for all available commands.");
                out.push("Type 'help <command>' for more information on a specific command.");
                return out;
            }
            if trimmed == "commands" {
                let mut joined = String::from("Commands: ");
                let parts: Vec<&str> = help_table().iter().map(|(n, _)| *n).collect();
                joined.push_str(&parts.join(", "));
                out.push(joined);
                return out;
            }
            if let Some((_, text)) = help_table().iter().find(|(n, _)| *n == trimmed) {
                out.push(*text);
            }
        }
        ConsoleCommand::Reset => {
            world.delete_all_events();
            state.reset_in_memory();
            out.push("Random bots were reset for all players. Please restart the Server.");
        }
        ConsoleCommand::Stats => {
            // Pull a fresh per-bot snapshot from the world and fold it
            // through `stats::print_stats`. The C++ counterpart also
            // echoed a requester-only line ahead of the histogram — we
            // leave that to the FFI layer because it owns the session
            // handle.
            let rows = world.query_bot_stats();
            let now = now_epoch_s_from_args(args);
            let max_level = world.world_max_level();
            let stats_out = print_stats(&rows, &mut state.events, world, now, max_level);
            out.messages.extend(stats_out.messages);
        }
        ConsoleCommand::Update => {
            // The worker thread picks this up via a `Tick` request —
            // the FFI layer inspects the output and triggers it.
            out.push("Playerbot update triggered.");
        }
        ConsoleCommand::Pid => {
            let parts: Vec<&str> = args.split_ascii_whitespace().collect();
            let p = parts.first().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
            let i = parts.get(1).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
            let d = parts.get(2).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
            state.pid.adjust(p, i, d);
            out.push(format!("Pid set to p:{p:.6} i:{i:.6} d:{d:.6}"));
        }
        ConsoleCommand::Diff => {
            let trimmed = args.trim();
            if trimmed.is_empty() {
                let sample = world.world_diff_sample();
                let db = world.database_delay_ms("CharacterDatabase");
                let bots_online = world.owned_bot_guids().len();
                out.push(format!(
                    "Avg diff: {}\nMax diff: {}\nchar db ping: {}\nBots online: {}",
                    sample.average_diff_ms, sample.max_diff_ms, db, bots_online,
                ));
            } else if trimmed.contains(' ') {
                // `diff <player> <empty>` — these values live on the
                // global BotConfig which Rust owns. The FFI layer
                // reads them back via `playerbot_config_*` entries;
                // here we just echo the new values the caller should
                // install.
                let parts: Vec<&str> = trimmed.split_ascii_whitespace().collect();
                let player = parts.first().and_then(|s| s.parse::<u32>().ok()).unwrap_or(100);
                let empty = parts.get(1).and_then(|s| s.parse::<u32>().ok()).unwrap_or(player);
                out.push(format!(
                    "Diff set to {player} (player), {empty} (empty)"
                ));
            }
        }
        ConsoleCommand::CleanMap => {
            out.push("Map cleaning initiated.");
        }
        ConsoleCommand::LoginDebug => {
            out.push("Login debug toggled.");
        }
    }
    out
}

/// Handle a parsed per-bot command.
///
/// `bots` is the guid list already filtered against `name_prefix` by
/// the caller (the FFI side walks the world's playerbot roster, applies
/// the prefix filter, and passes the matching guids here).
pub fn run_bot_command(
    cmd: BotCommand,
    bots: &[u32],
    params: &str,
    _events: &mut EventCache,
    world: &dyn RandomMgrWorld,
) -> CommandOutput {
    let _ = params; // change_strategy currently ignores its body — the strategy
    // set lives on the C++ side. We keep the field in case the
    // port folds that in later.

    let mut out = CommandOutput::new();
    if bots.is_empty() {
        out.push("No matching random bot.");
        return out;
    }
    for &bot in bots {
        match cmd {
            BotCommand::Init => {
                world.dispatch_randomize_first(bot);
                out.push(format!("init applied to bot #{bot}"));
            }
            BotCommand::Upgrade => {
                world.dispatch_update_gear_spells(bot);
                out.push(format!("upgrade applied to bot #{bot}"));
            }
            BotCommand::Refresh => {
                world.dispatch_refresh(bot);
                out.push(format!("refresh applied to bot #{bot}"));
            }
            BotCommand::Teleport => {
                let ok = world.dispatch_random_teleport_for_level(bot, false);
                if ok {
                    out.push(format!("teleport applied to bot #{bot}"));
                } else {
                    out.push(format!("teleport had no cached location for bot #{bot}"));
                }
            }
            BotCommand::Rpg => {
                let ok = world.dispatch_random_teleport_for_rpg(bot, false);
                if ok {
                    out.push(format!("rpg applied to bot #{bot}"));
                } else {
                    out.push(format!("rpg had no cached location for bot #{bot}"));
                }
            }
            BotCommand::Revive => {
                world.dispatch_revive(bot);
                out.push(format!("revive applied to bot #{bot}"));
            }
            BotCommand::Grind => {
                world.dispatch_random_teleport(bot);
                out.push(format!("grind applied to bot #{bot}"));
            }
            BotCommand::ChangeStrategy => {
                world.dispatch_change_strategy(bot);
                out.push(format!("change_strategy applied to bot #{bot}"));
            }
            BotCommand::Remove => {
                world.dispatch_remove(bot);
                out.push(format!("remove applied to bot #{bot}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmangos::{MockRandomMgrEvent, MockRandomMgrWorld};

    #[test]
    fn parses_console_commands_with_and_without_args() {
        assert!(matches!(
            parse("help"),
            ParsedCommand::Console { cmd: ConsoleCommand::Help, .. }
        ));
        assert!(matches!(
            parse("help commands"),
            ParsedCommand::Console { cmd: ConsoleCommand::Help, .. }
        ));
        if let ParsedCommand::Console { cmd, args } = parse("pid 0.1 0.01 0.5") {
            assert_eq!(cmd, ConsoleCommand::Pid);
            assert_eq!(args, "0.1 0.01 0.5");
        } else {
            panic!("expected Pid");
        }
    }

    #[test]
    fn parses_two_word_commands_before_single_word_ones() {
        assert!(matches!(
            parse("clean map"),
            ParsedCommand::Console { cmd: ConsoleCommand::CleanMap, .. }
        ));
        assert!(matches!(
            parse("login debug"),
            ParsedCommand::Console { cmd: ConsoleCommand::LoginDebug, .. }
        ));
    }

    #[test]
    fn parses_bot_commands_with_name_and_params() {
        if let ParsedCommand::Bot { cmd, name_prefix, params } = parse("change_strategy BobTheBot +tank") {
            assert_eq!(cmd, BotCommand::ChangeStrategy);
            assert_eq!(name_prefix, "BobTheBot");
            assert_eq!(params, "+tank");
        } else {
            panic!("expected Bot");
        }

        if let ParsedCommand::Bot { cmd, name_prefix, params } = parse("remove Alice") {
            assert_eq!(cmd, BotCommand::Remove);
            assert_eq!(name_prefix, "Alice");
            assert_eq!(params, "");
        } else {
            panic!("expected Bot");
        }
    }

    #[test]
    fn parses_bot_command_with_no_name_to_wildcard() {
        if let ParsedCommand::Bot { cmd, name_prefix, params } = parse("refresh") {
            assert_eq!(cmd, BotCommand::Refresh);
            assert_eq!(name_prefix, "%");
            assert_eq!(params, "");
        } else {
            panic!("expected Bot");
        }
    }

    #[test]
    fn empty_input_returns_help() {
        assert!(matches!(
            parse(""),
            ParsedCommand::Console { cmd: ConsoleCommand::Help, .. }
        ));
    }

    #[test]
    fn reset_wipes_state_and_emits_message() {
        let world = MockRandomMgrWorld::new();
        let mut state = RandomMgrState::new();
        state.players_level = 42;
        state.process_ticks = 10;

        let out = run_console_command(ConsoleCommand::Reset, "", &mut state, &world);
        assert_eq!(out.messages.len(), 1);
        assert!(out.messages[0].contains("Random bots were reset"));
        assert_eq!(state.players_level, 0);
        assert_eq!(state.process_ticks, 0);

        let ev = world.events();
        assert!(ev
            .iter()
            .any(|e| matches!(e, MockRandomMgrEvent::DeleteAllEvents)));
    }

    #[test]
    fn pid_adjusts_pid_controller_gains() {
        let world = MockRandomMgrWorld::new();
        let mut state = RandomMgrState::new();
        let _ = run_console_command(
            ConsoleCommand::Pid,
            "0.2 0.02 0.3",
            &mut state,
            &world,
        );
        // The pid controller uses the gains on the next calculate()
        // call — we assert by running a non-trivial calculate().
        let out = state.pid.calculate(100.0, 50.0);
        assert!(out.abs() > 0.0);
    }

    #[test]
    fn diff_empty_args_reports_world_state() {
        let world = MockRandomMgrWorld::new();
        world.set_world_diff_sample(cmangos::WorldDiffSample {
            current_diff_ms: 10.0,
            average_diff_ms: 20.0,
            max_diff_ms: 30.0,
        });
        let mut state = RandomMgrState::new();
        let out = run_console_command(ConsoleCommand::Diff, "", &mut state, &world);
        assert_eq!(out.messages.len(), 1);
        let msg = &out.messages[0];
        assert!(msg.contains("Avg diff: 20"));
        assert!(msg.contains("Max diff: 30"));
    }

    #[test]
    fn help_commands_lists_every_command() {
        let world = MockRandomMgrWorld::new();
        let mut state = RandomMgrState::new();
        let out = run_console_command(ConsoleCommand::Help, "commands", &mut state, &world);
        assert_eq!(out.messages.len(), 1);
        assert!(out.messages[0].contains("init"));
        assert!(out.messages[0].contains("change_strategy"));
        assert!(out.messages[0].contains("clean map"));
    }

    #[test]
    fn bot_command_dispatches_for_each_guid() {
        let world = MockRandomMgrWorld::new();
        let mut events = EventCache::new();
        let out = run_bot_command(
            BotCommand::Refresh,
            &[7, 9],
            "",
            &mut events,
            &world,
        );
        assert_eq!(out.messages.len(), 2);
        let ev = world.events();
        let refresh_count = ev
            .iter()
            .filter(|e| matches!(e, MockRandomMgrEvent::Refresh(_)))
            .count();
        assert_eq!(refresh_count, 2);
    }

    #[test]
    fn bot_command_with_empty_list_reports_no_match() {
        let world = MockRandomMgrWorld::new();
        let mut events = EventCache::new();
        let out = run_bot_command(BotCommand::Init, &[], "", &mut events, &world);
        assert_eq!(out.messages.len(), 1);
        assert!(out.messages[0].contains("No matching"));
    }

    #[test]
    fn stats_command_drains_snapshot_and_emits_histogram() {
        use cmangos::{BotStatsFlags, BotStatsSnapshotRow};
        let world = MockRandomMgrWorld::new();
        world.set_bot_stats(vec![
            BotStatsSnapshotRow {
                guid: 1,
                race: 1,
                class_id: 1,
                team: 0,
                level: 20,
                flags: BotStatsFlags::ACTIVE,
            },
            BotStatsSnapshotRow {
                guid: 2,
                race: 2,
                class_id: 5,
                team: 1,
                level: 20,
                flags: BotStatsFlags::ACTIVE,
            },
        ]);
        let mut state = RandomMgrState::new();
        let out = run_console_command(ConsoleCommand::Stats, "0", &mut state, &world);
        // Header line + level bucket + race/class/role/status lines
        // should all be present. We only assert on the anchor lines.
        let joined = out.messages.join("\n");
        assert!(joined.contains("2 Random Bots online"));
        assert!(joined.contains("20..29: 1 alliance, 1 horde"));
        assert!(joined.contains("Active: 2"));
    }
}
