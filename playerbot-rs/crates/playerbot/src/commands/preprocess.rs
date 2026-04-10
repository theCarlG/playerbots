//! Chat-command preprocessing.
//!
//! All commands flow through one of two paths:
//!
//! 1. **PB:v1 protocol** — structured addon messages from Mangosbot/RaidControl.
//!    Format: `PB:v1:[@selector]:cmd:arg|cmd2:arg`.
//!    Colons within commands are converted to spaces before parsing.
//!
//! 2. **Direct text** — whisper/party/raid commands typed by the player.
//!    Parsed directly by [`super::parser::parse`]. No legacy filter chain,
//!    no separator splitting, no `@tag` selectors.
//!
//! The old PB2 filter chain (`CompositeChatFilter`) and `BOT\t` / `\\`
//! separator / `#channel` prefix pipeline have been removed. All addon
//! communication uses the PB:v1 protocol.

use crate::bot::state::BotState;
use crate::commands::{ChatOrigin, LANG_ADDON, PendingCommand, SecurityLevel, parser};

/// Entry point called from [`crate::playerbot_chat_command`].
pub fn preprocess_and_enqueue(
    bot: &mut BotState,
    sender_guid: u64,
    security: SecurityLevel,
    origin: ChatOrigin,
    raw: &str,
) {
    // Strip legacy BOT\t prefix (Mangosbot still wraps PB:v1 messages in it
    // for the whisper channel because 1.12 has no WHISPER addon channel).
    let text = raw.trim_start_matches("BOT\t");

    if text.is_empty() || is_addon_noise(text) {
        return;
    }

    // PB:v1 protocol — addon structured messages.
    if let Some(payload) = text.strip_prefix("PB:") {
        handle_pb_protocol(bot, sender_guid, security, origin, payload);
        return;
    }

    // Direct text command — parse immediately, no filter chain.
    handle_direct_text(bot, sender_guid, security, origin, text);
}

/// Handle a `PB:v1:...` addon protocol message.
fn handle_pb_protocol(
    bot: &mut BotState,
    sender_guid: u64,
    security: SecurityLevel,
    origin: ChatOrigin,
    payload: &str,
) {
    let Some((selectors, cmd_strs)) = super::protocol::parse_addon_message(payload) else {
        return;
    };
    let addon_origin = ChatOrigin::new(origin.chat_type, LANG_ADDON);
    for cmd_str in cmd_strs {
        // Colon-separated args → space-separated for the text parser.
        let normalized = cmd_str.replace(':', " ");
        if let Some(cmd) = parser::parse(&normalized) {
            let pc = PendingCommand::with_selectors(
                sender_guid,
                security,
                addon_origin,
                cmd,
                selectors.clone(),
            );
            bot.pending_commands.lock().unwrap().push_back(pc);
        }
    }
}

/// Handle a direct text command (whisper/party/raid typed by a player).
fn handle_direct_text(
    bot: &mut BotState,
    sender_guid: u64,
    security: SecurityLevel,
    origin: ChatOrigin,
    text: &str,
) {
    // Strip chat-channel prefix if present (legacy Mangosbot routing hint).
    // When `#a ` (addon channel) is detected, override the origin so
    // replies route through `tell_addon` (CHAT_MSG_ADDON).
    let (text, origin) = strip_channel_prefix(text, origin);

    crate::bot::monitor::monitor_command_received(bot, text, sender_guid, &origin);

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }

    // Extract @selectors from the text (e.g. "@dps attack" → selectors=[@dps], rest="attack").
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let (selectors, rest_tokens) = super::protocol::extract_chat_selectors(&tokens);
    let rest = rest_tokens.join(" ");
    if rest.is_empty() {
        return;
    }

    let Some(cmd) = parser::parse(&rest) else {
        if bot.monitor_active {
            crate::bot::monitor::monitor_log(bot, &format!("CMD PARSE FAILED: {rest}"));
        }
        return;
    };
    if bot.monitor_active {
        crate::bot::monitor::monitor_command_parsed(bot, &rest, &format!("{cmd:?}"));
    }
    let pc = if sender_guid == 0 {
        PendingCommand::internal(cmd)
    } else if selectors.is_empty() {
        PendingCommand::external(sender_guid, security, origin, cmd)
    } else {
        PendingCommand::with_selectors(sender_guid, security, origin, cmd, selectors)
    };
    bot.pending_commands.lock().unwrap().push_back(pc);
}

const CHAT_PREFIXES: &[(&str, bool)] = &[
    ("#a ", true),  // addon channel — sets LANG_ADDON
    ("#w ", false),  // whisper
    ("#p ", false),  // party
    ("#r ", false),  // raid
    ("#g ", false),  // guild
];

fn strip_channel_prefix(text: &str, origin: ChatOrigin) -> (&str, ChatOrigin) {
    for &(prefix, is_addon) in CHAT_PREFIXES {
        if let Some(rest) = text.strip_prefix(prefix) {
            let new_origin = if is_addon {
                ChatOrigin::new(origin.chat_type, LANG_ADDON)
            } else {
                origin
            };
            return (rest, new_origin);
        }
    }
    (text, origin)
}

/// Non-command messages that arrive through the bot command channel —
/// `WoW` client events and addon chatter that are not bot commands.
const IGNORED_PATTERNS: &[&str] = &[
    "X-Perl",
    "HealBot",
    "HealComm",
    "LOOT_OPENED",
    "CTRA",
];

fn is_addon_noise(text: &str) -> bool {
    if text.starts_with("GathX") {
        return true;
    }
    IGNORED_PATTERNS
        .iter()
        .any(|pattern| text.contains(pattern))
}
