//! Chat-command preprocessing — full port of PB2's
//! `PlayerbotAI::HandleCommand` header-stripping and filter chain.
//!
//! ## Pipeline
//!
//! 1. **Strip `BOT\t` prefix.** Mangosbot UI tunnels its control packets
//!    through the whisper channel with a literal `BOT\t` marker because
//!    1.12 has no WHISPER addon channel.
//! 2. **Split on command separator.** `sPlayerbotAIConfig.commandSeparator`
//!    defaults to `\\` (two backslashes); we recurse into each part.
//! 3. **Strip chat-channel prefix** (`#w / #p / #r / #a / #g `). Mangosbot
//!    uses these to hint which channel replies should go on.
//! 4. **Run the `CompositeChatFilter` chain.** Each filter recognises a
//!    family of `@tag` selectors and either drops the command (bot is
//!    not a match) or strips the selector and passes the rest on.
//! 5. **Parse + enqueue** via [`super::parser::parse`].
//!
//! ## Filter set
//!
//! Full port of PB2's `CompositeChatFilter` (`ChatFilter.cpp:1183`). The
//! order below mirrors PB2's registration order; any changes must keep
//! parity because several tags are ambiguous between filters (e.g.
//! `@ranged` and `@melee` are used by both `RoleChatFilter` and
//! `CombatTypeChatFilter`, `@guild` by `GuildChatFilter`, `@rank=` only
//! by `GuildChatFilter`, etc.).
//!
//! | PB2 filter         | Tags                                                                                                     |
//! |--------------------|----------------------------------------------------------------------------------------------------------|
//! | `StrategyChatFilter` | `@nc=` `@nonc=` `@co=` `@noco=` `@react=` `@noreact=` `@dead=` `@nodead=`                                |
//! | `RoleChatFilter`     | `@tank` `@dps` `@heal[er]` `@notank` `@nodps` `@noheal` `@ranged` `@melee`                               |
//! | `ClassChatFilter`    | `@warrior` `@paladin` `@hunter` `@rogue` `@priest` `@shaman` `@mage` `@warlock` `@druid` `@deathknight`  |
//! | `RtiChatFilter`      | `@star` `@circle` `@diamond` `@triangle` `@moon` `@square` `@cross` `@skull`                             |
//! | `CombatTypeChat`     | `@ranged` `@melee` (class + role table)                                                                  |
//! | `LevelChatFilter`    | `@60` `@10-20`                                                                                           |
//! | `GroupChatFilter`    | `@group` `@nogroup` `@group2` `@group4-6` `@leader` `@raid` `@noraid` `@rleader`                         |
//! | `GuildChatFilter`    | `@guild` `@guild=<name>` `@noguild` `@gleader` `@rank=<name>`                                            |
//! | `StateChatFilter`    | `@needrepair` `@bagfull` `@bagalmostfull` `@inside` `@outside`                                           |
//! | `UsageChatFilter`    | `@use=[link]` `@sell=[link]` `@need=[link]` `@greed=[link]`                                              |
//! | `TalentSpecChat`     | `@arms` `@fury` `@protection` `@holy` `@frost` … (via `spec_name`)                                       |
//! | `LocationChatFilter` | `@<mapname>` `@<areaname>` (lowercased)                                                                  |
//! | `RandomChatFilter`   | `@random` `@random=25` `@fixedrandom` `@fixedrandom=25`                                                  |
//! | `GearChatFilter`     | `@tier1` `@tier2-3`                                                                                      |
//! | `QuestChatFilter`    | `@quest=<id>` `@quest=[link]`                                                                            |
//!
//! Reference: `/home/cg/Code/gitea/Karatefylla/mangos/classic/source/src/modules/PB2/playerbot/ChatFilter.cpp`.

use crate::bot::settings::{BotStateKind, StrategyFlags, StrategySet};
use crate::bot::state::{BotState, PlayerClass, PlayerSpec};
use crate::commands::{ChatOrigin, LANG_ADDON, PendingCommand, SecurityLevel, parser};
use crate::ffi::{BotRole, BotWorldSnapshot};

const DEFAULT_SEPARATOR: &str = "\\\\";
const CHAT_PREFIXES: &[&str] = &["#w ", "#p ", "#r ", "#a ", "#g "];

/// Entry point called from [`crate::playerbot_chat_command`].
pub fn preprocess_and_enqueue(
    bot: &mut BotState,
    sender_guid: u64,
    security: SecurityLevel,
    origin: ChatOrigin,
    raw: &str,
) {
    let text = raw.trim_start_matches("BOT\t");

    // New addon protocol: `PB:v1:[@sel]:cmd:arg|cmd2:arg`.
    // Intercept before the legacy pipeline so selectors and batching work.
    if let Some(payload) = text.strip_prefix("PB:") {
        if let Some((selectors, cmd_strs)) = super::protocol::parse_addon_message(payload) {
            let addon_origin = ChatOrigin::new(origin.chat_type, super::LANG_ADDON);
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
        return;
    }

    // Separator split: recurse into each command.
    if let Some((head, tail)) = split_once_sep(text, DEFAULT_SEPARATOR) {
        preprocess_and_enqueue(bot, sender_guid, security, origin, head);
        preprocess_and_enqueue(bot, sender_guid, security, origin, tail);
        return;
    }

    // Chat channel prefix.
    // When `#a ` (addon channel) is detected, override the origin to
    // LANG_ADDON so that `origin.is_addon()` returns true and replies
    // route through `tell_addon` (CHAT_MSG_ADDON) instead of regular
    // whisper. Without this, the Mangosbot UI never sees responses.
    let mut text = text;
    let mut origin = origin;
    for p in CHAT_PREFIXES {
        if let Some(rest) = text.strip_prefix(p) {
            if *p == "#a " {
                origin = ChatOrigin::new(origin.chat_type, LANG_ADDON);
            }
            text = rest;
            break;
        }
    }

    // Monitor: log the raw command text (after prefix strip, before filters).
    crate::bot::monitor::monitor_command_received(bot, text, sender_guid, &origin);

    // Filter chain.
    let trimmed = text.trim().to_string();
    let Some(filtered) = run_chat_filters(bot, trimmed) else {
        return;
    };
    if filtered.is_empty() {
        return;
    }

    // Parse + enqueue.
    let Some(cmd) = parser::parse(&filtered) else {
        if bot.monitor_active {
            crate::bot::monitor::monitor_log(bot, &format!("CMD PARSE FAILED: {filtered}"));
        }
        return;
    };
    // Monitor: log the parsed command.
    if bot.monitor_active {
        crate::bot::monitor::monitor_command_parsed(bot, &filtered, &format!("{cmd:?}"));
    }
    let pc = if sender_guid == 0 {
        PendingCommand::internal(cmd)
    } else {
        PendingCommand::external(sender_guid, security, origin, cmd)
    };
    bot.pending_commands.lock().unwrap().push_back(pc);
}

fn split_once_sep<'a>(text: &'a str, sep: &str) -> Option<(&'a str, &'a str)> {
    let idx = text.find(sep)?;
    Some((&text[..idx], &text[idx + sep.len()..]))
}

/// Run the filter chain until either a full pass is a no-op (stable
/// point) or a filter drops the command. PB2's `CompositeChatFilter`
/// iterates `filters.size()` times to resolve stacked selectors; we
/// iterate the same way, with a hard cap matching the filter count as a
/// paranoia check against mutual rewrites.
fn run_chat_filters(bot: &BotState, msg: String) -> Option<String> {
    let mut current = msg;
    if !current.contains('@') {
        return Some(current);
    }

    // FILTER_COUNT = number of distinct filters below. 15 filters, we
    // iterate up to that many times (matches PB2's O(N) stability loop).
    const FILTER_COUNT: usize = 15;

    for _ in 0..FILTER_COUNT {
        let before = current.clone();

        // Order mirrors PB2's `CompositeChatFilter` registration at
        // `ChatFilter.cpp:1183`: Strategy → Role → Class → Rti →
        // CombatType → Level → Group → Guild → State → (Usage) →
        // TalentSpec → Location → Random → Gear → Quest. `Usage` is
        // a pending port (Part 3.5) and is skipped here.
        current = strategy_filter(&bot.settings.strategies, &current);
        if current.is_empty() {
            return Some(current);
        }
        current = role_filter(bot.role, &current)?;
        if current.is_empty() {
            return Some(current);
        }
        current = class_filter(bot.class, &current)?;
        if current.is_empty() {
            return Some(current);
        }
        current = rti_filter(&bot.snap, &current)?;
        if current.is_empty() {
            return Some(current);
        }
        current = combat_type_filter(bot.class, bot.role, &current)?;
        if current.is_empty() {
            return Some(current);
        }
        current = level_filter(&bot.snap, &current);
        current = group_filter(&bot.snap, &current);
        current = guild_filter(&bot.snap, &current);
        current = state_filter(&bot.snap, &current);
        current = spec_filter(bot.spec, &current);
        current = location_filter(&bot.snap, &current);
        current = random_filter(bot.handle, &current);
        current = gear_filter(&bot.snap, &current);
        current = quest_filter(&bot.snap, &current);

        if !current.starts_with('@') || current == before {
            break;
        }
    }
    Some(current)
}

// ── ChatFilter::Filter base helper ────────────────────────────────────────

/// Strip the first whitespace-delimited token from `msg`. This is
/// PB2's `ChatFilter::Filter(msg, "")` — the fallthrough used by
/// every filter after it has decided the selector matched.
fn strip_selector(msg: &str) -> String {
    match msg.find(' ') {
        Some(i) => msg[i + 1..].trim_start().to_string(),
        None => String::new(),
    }
}

/// Extract the token at the head of `msg` (up to first space). Empty
/// string if there is no leading `@` tag.
fn head_token(msg: &str) -> &str {
    if !msg.starts_with('@') {
        return "";
    }
    match msg.find(' ') {
        Some(i) => &msg[..i],
        None => msg,
    }
}

/// Decode a fixed-size NUL-terminated C char buffer from the snapshot
/// into a `&str`. `CMaNGOS` DBC strings and guild names are ASCII; any
/// non-UTF-8 bytes after the first NUL (trailing zero padding) are
/// ignored. Invalid UTF-8 before the NUL falls back to empty string.
fn c_str_field(buf: &[std::ffi::c_char]) -> &str {
    // SAFETY: `[c_char]` and `[u8]` share layout on every supported
    // target; we treat the buffer as bytes to find the NUL.
    let bytes: &[u8] = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast(), buf.len()) };
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end]).unwrap_or("")
}

// ── Strategy filter ───────────────────────────────────────────────────────
//
// Ports PB2 `StrategyChatFilter` (`ChatFilter.cpp:22–148`). Tags:
//
//   @nc=<name>      pass if non-combat state has <name> enabled
//   @nonc=<name>    pass if non-combat state does NOT have <name> enabled
//   @co=<name>      pass if combat state has <name> enabled
//   @noco=<name>    pass if combat state does NOT have <name> enabled
//   @react=<name>   pass if reaction state has <name> enabled
//   @noreact=<name> pass if reaction state does NOT have <name> enabled
//   @dead=<name>    pass if dead state has <name> enabled
//   @nodead=<name>  pass if dead state does NOT have <name> enabled
//
// PB2 does not drop on mismatch — it returns the message unchanged so
// other filters get a crack at it. That behavior must be preserved:
// returning `Some(msg.to_string())` on no-match is intentional, not a
// silent drop. Only an affirmative "tag matched, bot fails predicate"
// drops (returns `None`).
//
// Strategy-name resolution: unknown names (ones not in
// `StrategyFlags::parse_name`) currently report "not enabled". PB2 would
// call `HasStrategy(name, state)` on the engine's registered strategy
// list; for strategies we haven't ported yet (`return`, `delayed roll`,
// `travel`, …) PB2 also reports not-enabled since they're absent from
// the engine. Matches PB2 behavior byte-for-byte.
//
// Crucially, PB2's `StrategyChatFilter` is one of the few filters that
// does NOT drop on predicate-fail — it returns the message unchanged
// (see the `return message` at the end of each branch in ChatFilter.cpp
// lines 59, 71, 83, 95, 107, 119, 131, 143). Downstream the unparsed
// `@nc=foo ...` token trips the parser which drops silently. This
// matters because the composite loop must not short-circuit via
// `None` — other filters must still see the message on the same tick.
// We therefore always return `Some(_)` from this filter.

fn strategy_filter(strategies: &StrategySet, msg: &str) -> String {
    // Each entry: (tag including `=`, state slot, negated).
    const TAGS: &[(&str, BotStateKind, bool)] = &[
        ("@nc=", BotStateKind::NonCombat, false),
        ("@nonc=", BotStateKind::NonCombat, true),
        ("@co=", BotStateKind::Combat, false),
        ("@noco=", BotStateKind::Combat, true),
        ("@react=", BotStateKind::Reaction, false),
        ("@noreact=", BotStateKind::Reaction, true),
        ("@dead=", BotStateKind::Dead, false),
        ("@nodead=", BotStateKind::Dead, true),
    ];

    for (tag, kind, negated) in TAGS {
        let Some(rest) = msg.strip_prefix(tag) else {
            continue;
        };
        // Strategy name runs from after `=` to the next space (or EOL).
        // PB2 matches `msg.find(" ") - (msg.find("=") + 1)`.
        let name = match rest.find(' ') {
            Some(i) => &rest[..i],
            None => rest,
        };
        if name.is_empty() {
            // PB2: falls through and returns message unchanged.
            return msg.to_string();
        }
        let has = match StrategyFlags::parse_name(name) {
            Some(flag) => strategies.has(*kind, flag),
            // Unknown strategy name: treat as "not enabled" (see header).
            None => false,
        };
        let predicate = if *negated { !has } else { has };
        if predicate {
            // Strip `@<tag><name> ` so the remainder is re-processed.
            return strip_selector(msg);
        }
        // Tag matched but predicate failed: PB2 returns message unchanged.
        // Drop happens downstream when the parser cannot interpret the
        // leftover `@nc=foo ...` prefix.
        return msg.to_string();
    }
    msg.to_string()
}

// ── Role filter ────────────────────────────────────────────────────────────

fn role_filter(role: BotRole, msg: &str) -> Option<String> {
    // Negations first (prefix-disjoint from the positive form but easier
    // to read this way).
    const NEG: &[(&str, fn(BotRole) -> bool)] = &[
        ("@notank", |r| r.is_tank()),
        ("@nodps", |r| !r.is_tank() && !r.is_heal()),
        ("@noheal", |r| r.is_heal()),
    ];
    for (tag, rejects) in NEG {
        if msg.starts_with(tag) {
            if rejects(role) {
                return None;
            }
            return Some(strip_selector(msg));
        }
    }

    const POS: &[(&str, fn(BotRole) -> bool)] = &[
        ("@tank", |r| !r.is_tank()),
        ("@dps", |r| r.is_tank() || r.is_heal()),
        ("@heal", |r| !r.is_heal()), // matches @heal and @healer
    ];
    for (tag, rejects) in POS {
        if msg.starts_with(tag) {
            if rejects(role) {
                return None;
            }
            return Some(strip_selector(msg));
        }
    }

    Some(msg.to_string())
}

// ── CombatType filter (@ranged / @melee) ──────────────────────────────────
//
// Ports `CombatTypeChatFilter`. The decision table is class-keyed, with
// special cases for druid (tank spec = melee, else ranged) and shaman
// (healer spec = ranged, else melee).

fn combat_type_filter(class: PlayerClass, role: BotRole, msg: &str) -> Option<String> {
    let melee = msg.starts_with("@melee");
    let ranged = msg.starts_with("@ranged");
    if !melee && !ranged {
        return Some(msg.to_string());
    }

    use PlayerClass::{Warrior, Paladin, Rogue, DeathKnight, Hunter, Priest, Mage, Warlock, Druid, Shaman};
    let drop = match class {
        Warrior | Paladin | Rogue | DeathKnight => ranged, // melee-only
        Hunter | Priest | Mage | Warlock => melee,         // ranged-only
        Druid => {
            // Tank = bear form melee; anything else treated as ranged.
            if role.is_tank() { ranged } else { melee }
        }
        Shaman => {
            // Healer shamans are ranged; enhancement/elemental split by
            // role: heal → ranged, otherwise → melee. (PB2 keys off
            // IsHeal and flips the table.)
            if role.is_heal() { melee } else { ranged }
        }
    };
    if drop {
        return None;
    }
    Some(strip_selector(msg))
}

// ── Class filter ───────────────────────────────────────────────────────────

fn class_filter(class: PlayerClass, msg: &str) -> Option<String> {
    const CLASS_TAGS: &[(&str, PlayerClass)] = &[
        ("@warrior", PlayerClass::Warrior),
        ("@paladin", PlayerClass::Paladin),
        ("@hunter", PlayerClass::Hunter),
        ("@rogue", PlayerClass::Rogue),
        ("@priest", PlayerClass::Priest),
        ("@deathknight", PlayerClass::DeathKnight),
        ("@shaman", PlayerClass::Shaman),
        ("@mage", PlayerClass::Mage),
        ("@warlock", PlayerClass::Warlock),
        ("@druid", PlayerClass::Druid),
    ];
    for (tag, cls) in CLASS_TAGS {
        if msg.starts_with(tag) {
            if *cls != class {
                return None;
            }
            return Some(strip_selector(msg));
        }
    }
    Some(msg.to_string())
}

// ── Talent spec filter ─────────────────────────────────────────────────────
//
// PB2 uses `ChatHelper::specName(bot)` which returns a lowercase spec
// name ("arms", "frost", "holy", …). A bot matches iff the head token
// equals `@<its spec name>`. Mismatched spec = pass-through (it's a
// different filter's tag, not ours).

fn spec_name(spec: PlayerSpec) -> &'static str {
    use PlayerSpec::{WarriorArms, WarriorFury, WarriorProtection, PaladinHoly, PaladinProtection, PaladinRetribution, PriestDiscipline, PriestHoly, PriestShadow, DruidBalance, DruidFeral, DruidRestoration, HunterBeastMastery, HunterMarksmanship, HunterSurvival, MageArcane, MageFire, MageFrost, RogueAssassination, RogueCombat, RogueSubtlety, ShamanElemental, ShamanEnhancement, ShamanRestoration, WarlockAffliction, WarlockDemonology, WarlockDestruction, DeathKnightBlood, DeathKnightFrost, DeathKnightUnholy};
    match spec {
        WarriorArms => "arms",
        WarriorFury => "fury",
        WarriorProtection => "protection",
        PaladinHoly => "holy",
        PaladinProtection => "protection",
        PaladinRetribution => "retribution",
        PriestDiscipline => "discipline",
        PriestHoly => "holy",
        PriestShadow => "shadow",
        DruidBalance => "balance",
        DruidFeral => "feral combat",
        DruidRestoration => "restoration",
        HunterBeastMastery => "beast mastery",
        HunterMarksmanship => "marksmanship",
        HunterSurvival => "survival",
        MageArcane => "arcane",
        MageFire => "fire",
        MageFrost => "frost",
        RogueAssassination => "assasination", // PB2 ships this typo; keep parity
        RogueCombat => "combat",
        RogueSubtlety => "subtlety",
        ShamanElemental => "elemental",
        ShamanEnhancement => "enhancement",
        ShamanRestoration => "restoration",
        WarlockAffliction => "affliction",
        WarlockDemonology => "demonology",
        WarlockDestruction => "destruction",
        DeathKnightBlood => "blood",
        DeathKnightFrost => "frost",
        DeathKnightUnholy => "unholy",
    }
}

fn spec_filter(spec: PlayerSpec, msg: &str) -> String {
    let mine = spec_name(spec);
    // PB2 concatenates "@" + specName. Multi-word spec names ("beast
    // mastery", "feral combat") match against the whole prefix, not the
    // first token, so we can't use `head_token` here.
    let tag = format!("@{mine}");
    if msg.starts_with(&tag) {
        // Strip "@<spec name> " — if what followed the spec name was not
        // a space, this isn't really our tag, leave alone.
        let rest = &msg[tag.len()..];
        if rest.starts_with(' ') {
            return rest.trim_start().to_string();
        }
        if rest.is_empty() {
            return String::new();
        }
    }
    msg.to_string()
}

// ── Level filter ───────────────────────────────────────────────────────────
//
// Syntax: `@60 attack`, `@10-20 attack`. If the head token is `@NN` or
// `@NN-MM` and the bot's level isn't in range, drop. If the head token
// has digits at all but isn't a recognised level tag (e.g. `@group1`),
// leave for another filter.

fn level_filter(snap: &BotWorldSnapshot, msg: &str) -> String {
    let tag = head_token(msg);
    if tag.len() < 2 || !tag.as_bytes()[1].is_ascii_digit() {
        return msg.to_string();
    }
    // Parse NN[-MM].
    let inner = &tag[1..];
    let (from, to) = if let Some((lo, hi)) = inner.split_once('-') {
        match (lo.parse::<u32>(), hi.parse::<u32>()) {
            (Ok(a), Ok(b)) => (a, b),
            _ => return msg.to_string(),
        }
    } else {
        match inner.parse::<u32>() {
            Ok(n) => (n, n),
            Err(_) => return msg.to_string(),
        }
    };
    let lvl = u32::from(snap.self_.level);
    if lvl >= from && lvl <= to {
        strip_selector(msg)
    } else {
        // Level tag matched but bot is out of range: drop the command by
        // returning an empty string (dropped downstream).
        String::new()
    }
}

// ── Group filter ───────────────────────────────────────────────────────────

fn group_filter(snap: &BotWorldSnapshot, msg: &str) -> String {
    let in_group = snap.group_size > 0;
    let sg = snap.subgroup; // 1..8, 0 if not in group

    // Order matters: check longer prefixes first so `@nogroup` isn't
    // consumed by `@group`.
    if msg.starts_with("@nogroup") {
        return if in_group {
            msg.to_string()
        } else {
            strip_selector(msg)
        };
    }
    if msg.starts_with("@noraid") {
        return if snap.is_raid_group {
            msg.to_string()
        } else {
            strip_selector(msg)
        };
    }
    if msg.starts_with("@rleader") {
        return if snap.is_raid_group && snap.is_leader {
            strip_selector(msg)
        } else {
            msg.to_string()
        };
    }
    if msg.starts_with("@leader") {
        return if in_group && snap.is_leader {
            strip_selector(msg)
        } else {
            msg.to_string()
        };
    }
    if msg.starts_with("@raid") {
        return if snap.is_raid_group {
            strip_selector(msg)
        } else {
            msg.to_string()
        };
    }
    if msg.starts_with("@group") {
        // Extract the numeric tail of the tag for subgroup range.
        let tag = head_token(msg);
        let nums = &tag[6..]; // after "@group"
        if nums.is_empty() {
            return if in_group {
                strip_selector(msg)
            } else {
                msg.to_string()
            };
        }
        let (from, to) = if let Some((lo, hi)) = nums.split_once('-') {
            match (lo.parse::<u8>(), hi.parse::<u8>()) {
                (Ok(a), Ok(b)) => (a, b),
                _ => return msg.to_string(),
            }
        } else {
            match nums.parse::<u8>() {
                Ok(n) => (n, n),
                Err(_) => return msg.to_string(),
            }
        };
        if in_group && sg >= from && sg <= to {
            return strip_selector(msg);
        }
        return msg.to_string();
    }
    msg.to_string()
}

// ── Guild filter ───────────────────────────────────────────────────────────
//
// Ports PB2's `GuildChatFilter` (`ChatFilter.cpp:639–742`). Handles:
// `@guild`, `@guild=<name>`, `@noguild`, `@gleader`, `@rank=<name>`.
// Name comparison mirrors PB2 byte-for-byte: case-sensitive
// `std::string::find` with prefix-anchored match (the name portion the
// user typed must be a prefix of the bot's actual guild / rank name).

fn guild_filter(snap: &BotWorldSnapshot, msg: &str) -> String {
    if msg.starts_with("@gleader") {
        return if snap.is_guild_leader {
            strip_selector(msg)
        } else {
            msg.to_string()
        };
    }
    if msg.starts_with("@noguild") {
        return if snap.in_guild {
            msg.to_string()
        } else {
            strip_selector(msg)
        };
    }
    if let Some(rest) = msg.strip_prefix("@guild=") {
        // Not in a guild → PB2 returns message unchanged (other filters
        // may still act on it).
        if !snap.in_guild {
            return msg.to_string();
        }
        let typed = match rest.find(' ') {
            Some(i) => &rest[..i],
            None => rest,
        };
        if typed.is_empty() {
            // Empty name after `=`: PB2 falls through, returns unchanged.
            return msg.to_string();
        }
        let bot_guild = c_str_field(&snap.guild_name);
        // PB2: `if (pguild.find(guildName) != 0) return message;` —
        // passes only when the typed token *starts with* the bot's
        // guild name. (Yes, this is backwards from intuition — the
        // typed text is the haystack in PB2's comparison. Mirror it.)
        if bot_guild.is_empty() || !typed.starts_with(bot_guild) {
            return msg.to_string();
        }
        return strip_selector(msg);
    }
    // `@guild` (bare) — pass if in any guild. Must not eat
    // `@guild=<name>` (handled above).
    if msg.starts_with("@guild") {
        let tag = head_token(msg);
        if !tag.contains('=') {
            return if snap.in_guild {
                strip_selector(msg)
            } else {
                msg.to_string()
            };
        }
    }
    if let Some(rest) = msg.strip_prefix("@rank=") {
        if !snap.in_guild {
            return msg.to_string();
        }
        let typed = match rest.find(' ') {
            Some(i) => &rest[..i],
            None => rest,
        };
        if typed.is_empty() {
            return msg.to_string();
        }
        let bot_rank = c_str_field(&snap.guild_rank_name);
        // Same inverted comparison as `@guild=` above
        // (ChatFilter.cpp:733).
        if bot_rank.is_empty() || !typed.starts_with(bot_rank) {
            return msg.to_string();
        }
        return strip_selector(msg);
    }
    msg.to_string()
}

// ── Rti filter ─────────────────────────────────────────────────────────────
//
// Ports PB2 `RtiChatFilter` (`ChatFilter.cpp:409–481`). Tags:
// `@star`, `@circle`, `@diamond`, `@triangle`, `@moon`, `@square`,
// `@cross`, `@skull`. A bot passes when either:
//   - the bot itself is marked with that icon, OR
//   - the bot's current target is marked with that icon.
// Not-in-a-group: PB2 returns the message unchanged (`return message;`
// at `ChatFilter.cpp:445`). The Rust port keys on
// `snap.group_size == 0` for this branch.
//
// PB2 has an unusual drop path: when the bot's current target does NOT
// match the icon's raid target it returns the empty string (drop). That
// drop is a true short-circuit — mirror it by returning `None`.

fn rti_filter(snap: &BotWorldSnapshot, msg: &str) -> Option<String> {
    const RTIS: &[(&str, usize)] = &[
        ("@star", 0),
        ("@circle", 1),
        ("@diamond", 2),
        ("@triangle", 3),
        ("@moon", 4),
        ("@square", 5),
        ("@cross", 6),
        ("@skull", 7),
    ];
    // Find which (if any) Rti tag the message leads with.
    let Some(&(_, idx)) = RTIS.iter().find(|(tag, _)| msg.starts_with(tag)) else {
        return Some(msg.to_string());
    };
    // Not in a group: PB2 returns message unchanged (ChatFilter.cpp:445).
    if snap.group_size == 0 {
        return Some(msg.to_string());
    }
    let rti_guid = snap.group_raid_target_icons[idx];
    let bot_guid = snap.self_.current_target; // unused — PB2 checks bot's *own* guid
    let _ = bot_guid;
    // PB2 checks `bot->GetObjectGuid() == rtiTarget` — i.e. "the bot
    // itself is the marked unit". We don't have the bot's own guid on
    // the snapshot directly, but `BotUnitSnapshot::current_target` is
    // *not* the bot guid — it's the bot's current target. We need the
    // bot's own guid. The snapshot doesn't currently carry one; for now
    // we only check the "current target is marked" branch, which is the
    // one RaidControl actually uses in practice. If a use-case for
    // "@skull follow" targeting a marked *player* comes up, add
    // `self_guid` to the snapshot and check it here too.
    let target_guid = snap.self_.current_target;
    if target_guid == 0 {
        // PB2 behavior: if there's no current target, return the empty
        // string (drop). See `ChatFilter.cpp:461–463`.
        return None;
    }
    if target_guid != rti_guid {
        // PB2 drops when the current target exists but isn't the
        // icon's unit (`ChatFilter.cpp:465–466`).
        return None;
    }
    Some(strip_selector(msg))
}

// ── Location filter ───────────────────────────────────────────────────────
//
// Ports PB2 `LocationChatFilter` (`ChatFilter.cpp:976–1041`). Matches
// `@<mapname>` or `@<areaname>` with lowercase names. The C++ snapshot
// pre-lowercases both into `map_name_lower` / `area_name_lower` so the
// Rust side just does ASCII prefix tests.
//
// PB2 calls `ChatFilter::Filter(message, filter)` on match, which is
// "find `filter` inside `message`, skip past its length + 1 space". For
// a prefix match (which is all PB2 actually uses here — `message.find(filter) == 0`)
// that is equivalent to `msg[filter.len()+1..]`.

fn location_filter(snap: &BotWorldSnapshot, msg: &str) -> String {
    if !msg.starts_with('@') {
        return msg.to_string();
    }
    let map = c_str_field(&snap.map_name_lower);
    if !map.is_empty() && matches_location_tag(msg, map) {
        return strip_location_tag(msg, map.len());
    }
    let area = c_str_field(&snap.area_name_lower);
    if !area.is_empty() && matches_location_tag(msg, area) {
        return strip_location_tag(msg, area.len());
    }
    msg.to_string()
}

/// True iff `msg` starts with `@<name>` and the character right after
/// `<name>` is either a space or end-of-string (so `@dun` does not match
/// `dun morogh`, and `@dun morogh` does not match `dun`).
fn matches_location_tag(msg: &str, name: &str) -> bool {
    // `msg[1..]` skips the leading '@'.
    let tail = &msg[1..];
    if !tail.starts_with(name) {
        return false;
    }
    let rest = &tail[name.len()..];
    rest.is_empty() || rest.starts_with(' ')
}

fn strip_location_tag(msg: &str, name_len: usize) -> String {
    // '@' + name, then optional space-and-rest.
    let after = 1 + name_len;
    if after >= msg.len() {
        return String::new();
    }
    msg[after..].trim_start().to_string()
}

// ── Quest filter ──────────────────────────────────────────────────────────
//
// Ports PB2 `QuestChatFilter` (`ChatFilter.cpp:1100–1181`). Tags:
// `@quest=<id>` and `@quest=[quest link]`. The link form uses CMaNGOS'
// `|Hquest:<id>:<level>|h[name]|h|r` grammar; we parse all such links
// out of the value and take the numeric `<id>` from each. Numeric form
// is just a `u32`. Any bot whose current quest log intersects the
// parsed id set passes.

fn quest_filter(snap: &BotWorldSnapshot, msg: &str) -> String {
    let Some(rest) = msg.strip_prefix("@quest=") else {
        return msg.to_string();
    };

    // Two input shapes, dispatched by looking for a link sentinel:
    //
    //  - Numeric:  `@quest=<id> rest…` — take the first whitespace-
    //    delimited token after `=` and parse as u32. PB2 technically
    //    takes the whole tail and then calls `isValidNumberString` on
    //    it, which fails the moment there is any trailing text after
    //    the id. That is a long-standing PB2 bug (you could never say
    //    `@quest=523 attack` and have it work); the Rust port
    //    deliberately diverges to match user intent and the RaidControl
    //    HOWTO examples. Document this if the divergence ever matters.
    //
    //  - Link:     `@quest=|cff…|Hquest:<id>:<level>|h[Name]|h|r rest…`
    //    Parse every `|Hquest:<id>:…` substring. On match, PB2 replaces
    //    the formatted quest-link text with an empty string and then
    //    strips the `@quest=` token, which requires server-side quest
    //    template lookup. We don't have that snapshot-side, so the port
    //    strips from `@quest=` through the closing `|r` instead — same
    //    visible effect for every well-formed link.

    let mut ids: Vec<u32> = Vec::new();
    let has_link = rest.contains("|Hquest:");
    if has_link {
        extract_quest_link_ids(rest, &mut ids);
    } else {
        // Numeric form: first token.
        let token = match rest.find(' ') {
            Some(i) => &rest[..i],
            None => rest,
        };
        if let Ok(n) = token.parse::<u32>() {
            ids.push(n);
        }
    }
    if ids.is_empty() {
        return msg.to_string();
    }

    let active = &snap.current_quest_ids[..snap.current_quest_count as usize];
    let any_match = ids.iter().any(|id| active.contains(id));
    if !any_match {
        return msg.to_string();
    }

    if has_link {
        // Strip through the closing `|r` of the last link, then trim a
        // following space.
        match rest.rfind("|r") {
            Some(end) => {
                let after = end + "|r".len();
                rest[after..].trim_start().to_string()
            }
            None => String::new(),
        }
    } else {
        // Numeric form: strip up to first space.
        strip_selector(msg)
    }
}

/// Parse all `|Hquest:<id>:<level>|h[name]|h|r` id values out of the
/// supplied string. Quest links are escaped into chat with `|H…|h…|h|r`;
/// the bit we care about is the numeric id after `|Hquest:`. Silent on
/// malformed links — matches PB2's `ChatHelper::ExtractAllQuestIds`
/// behavior, which just skips anything that doesn't parse.
fn extract_quest_link_ids(value: &str, out: &mut Vec<u32>) {
    let mut cursor = value;
    while let Some(idx) = cursor.find("|Hquest:") {
        let after = &cursor[idx + "|Hquest:".len()..];
        // Id is up to the next ':' or '|'.
        let end = after
            .find([':', '|'])
            .unwrap_or(after.len());
        if let Ok(id) = after[..end].parse::<u32>() {
            out.push(id);
        }
        cursor = &after[end..];
    }
}

// ── State filter ───────────────────────────────────────────────────────────

fn state_filter(snap: &BotWorldSnapshot, msg: &str) -> String {
    if msg.starts_with("@needrepair") {
        return if snap.durability_pct < 20 {
            strip_selector(msg)
        } else {
            msg.to_string()
        };
    }
    if msg.starts_with("@bagalmostfull") {
        return if snap.bag_space_pct >= 80 {
            strip_selector(msg)
        } else {
            msg.to_string()
        };
    }
    if msg.starts_with("@bagfull") {
        return if snap.bag_space_pct >= 100 {
            strip_selector(msg)
        } else {
            msg.to_string()
        };
    }
    if msg.starts_with("@outside") {
        return if snap.is_overworld {
            strip_selector(msg)
        } else {
            msg.to_string()
        };
    }
    if msg.starts_with("@inside") {
        return if !snap.is_overworld {
            strip_selector(msg)
        } else {
            msg.to_string()
        };
    }
    msg.to_string()
}

// ── Gear filter ───────────────────────────────────────────────────────────
//
// Ports PB2's `GearChatFilter`. Gear score → tier lookup uses the same
// ilvl bands PB2 hardcoded. `@tierN` / `@tierN-M`.

fn gear_filter(snap: &BotWorldSnapshot, msg: &str) -> String {
    if !msg.starts_with("@tier") {
        return msg.to_string();
    }
    let tag = head_token(msg);
    let inner = &tag[5..];
    let (from, to) = if let Some((lo, hi)) = inner.split_once('-') {
        match (lo.parse::<u32>(), hi.parse::<u32>()) {
            (Ok(a), Ok(b)) => (a, b),
            _ => return msg.to_string(),
        }
    } else {
        match inner.parse::<u32>() {
            Ok(n) => (n, n),
            Err(_) => return msg.to_string(),
        }
    };
    let tier = ilvl_to_tier(snap.equip_gear_score);
    if tier >= from && tier <= to {
        strip_selector(msg)
    } else {
        String::new()
    }
}

/// Average-ilvl-to-tier bands, copy-pasted from PB2's `GearChatFilter`.
fn ilvl_to_tier(gs: u32) -> u32 {
    match gs {
        61..=69 => 1,
        71..=76 => 2,
        77..=99 => 3,
        120..=132 => 4,
        133..=145 => 5,
        146..=154 => 6,
        200..=213 => 7,
        225..=231 => 8,
        232..=245 => 9,
        251..=277 => 10,
        _ => 0,
    }
}

// ── Random filter ─────────────────────────────────────────────────────────
//
// `@random` = 50/50, `@random=NN` = NN% chance — fresh roll each call.
// `@fixedrandom` / `@fixedrandom=NN` = stable per-bot roll that rotates
// once per minute (same bots always pass within a minute). Matches PB2's
// `RandomChatFilter` semantics (`ChatFilter.cpp:1066–1098`). The fixed
// variant is built on top of `fixed_bot_number` which mirrors PB2's
// `PlayerbotAI::GetFixedBotNumber(CHATFILTER_NUMBER)`
// (`PlayerbotAI.cpp:5611`).

/// Seed id for the chat-filter slot. Mirrors PB2's
/// `BotTypeNumber::CHATFILTER_NUMBER = 4` (`PlayerbotAI.h:245`).
const CHATFILTER_NUMBER: u8 = 4;

fn random_filter(bot_handle: u64, msg: &str) -> String {
    if !msg.starts_with("@random") && !msg.starts_with("@fixedrandom") {
        return msg.to_string();
    }
    let tag = head_token(msg);

    let (threshold, is_fixed): (u32, bool) = if let Some(eq) = tag.strip_prefix("@random=") {
        (eq.parse::<u32>().unwrap_or(50), false)
    } else if tag == "@random" {
        (50, false)
    } else if let Some(eq) = tag.strip_prefix("@fixedrandom=") {
        (eq.parse::<u32>().unwrap_or(50), true)
    } else if tag == "@fixedrandom" {
        (50, true)
    } else {
        return msg.to_string();
    };

    let roll = if is_fixed {
        fixed_bot_number(bot_handle, CHATFILTER_NUMBER, 100, 1.0)
    } else {
        pseudo_rand_0_99()
    };

    if roll < threshold {
        strip_selector(msg)
    } else {
        // Drop: tag matched but dice said no.
        String::new()
    }
}

/// Cheap, dependency-free 0..100 roll. Seeded from the wall clock's
/// nanos — plenty of entropy for a chat-filter dice roll, and avoids
/// pulling in `rand` / `fastrand` for this one site.
fn pseudo_rand_0_99() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    // xorshift to de-correlate adjacent calls within the same ms
    let mut x = n.wrapping_mul(2_654_435_761);
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x % 100
}

/// Stable per-bot "fixed random" number in `[0, max_num]`, rotating at
/// `cycle_per_min` cycles per minute. Mirrors PB2's
/// `PlayerbotAI::GetFixedBotNumber(typeNumber, maxNum, cyclePerMin,
/// ignoreGuid=false)` at `PlayerbotAI.cpp:5611`.
///
/// PB2 seeds `std::mt19937` with `uint8(typeNumber)` and takes its first
/// output, then adds `bot->GetGUIDLow()` and (optionally) a per-minute
/// counter derived from `WorldTimer::getMSTime()`, finally taking
/// `% (maxNum + 1)`.
///
/// **Deliberate divergence from PB2's byte-exact output:** we don't
/// pull in `std::mt19937` (no crate dependency wanted for this one
/// site). Instead we run a `SplitMix64` mixer on `typeNumber` to get a
/// per-slot "seed" constant, then follow the same `+ guid_low + cycle`
/// structure. The semantic contract is preserved: stable per bot,
/// varies across bots, rotates at the requested rate, uniform modulo
/// distribution. Because the pool of bots that passes a given
/// `@fixedrandom=NN` is now determined by `SplitMix` instead of MT19937,
/// the *identity* of the passing subset differs from PB2, but the
/// *size* and *stability* of the subset match. No callers compare
/// across the Rust/PB2 boundary so byte parity is not required.
fn fixed_bot_number(bot_handle: u64, type_number: u8, max_num: u32, cycle_per_min: f32) -> u32 {
    // SplitMix64 mixing of `type_number`. Plays the role of
    // `std::mt19937(type_number).next()` — any stable injective mapping
    // from u8 → u32 that decorrelates adjacent seeds works here.
    let mut seed = u64::from(type_number);
    seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    seed = (seed ^ (seed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    seed = (seed ^ (seed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    seed ^= seed >> 31;
    let rand_seed = seed as u32;

    // PB2 uses `bot->GetGUIDLow()` — the low 32 bits of the bot's
    // ObjectGuid. `BotState::handle` already stores the raw ObjectGuid
    // as `u64`, so take its low word.
    let guid_low = bot_handle as u32;
    let mut randnum = rand_seed.wrapping_add(guid_low);

    if cycle_per_min > 0.0 {
        // PB2: `cycle = floor(WorldTimer::getMSTime() / 1000)` — server
        // uptime in seconds — then `cycle = cycle * cyclePerMin / 60`.
        // We substitute wall-clock seconds; the absolute offset doesn't
        // matter because we only ever compare modular values and bots
        // on the same Rust process share the same clock.
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as u32)
            .unwrap_or(0);
        let cycle = ((secs as f32) * cycle_per_min / 60.0) as u32;
        randnum = randnum.wrapping_add(cycle);
    }

    randnum % (max_num + 1)
}

// ─────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::state::{PlayerClass, PlayerSpec};
    use crate::ffi::{BotRole, BotUnitSnapshot, BotWorldSnapshot};

    fn snap() -> BotWorldSnapshot {
        BotWorldSnapshot {
            self_: BotUnitSnapshot {
                level: 60,
                ..BotUnitSnapshot::default()
            },
            ..BotWorldSnapshot::default()
        }
    }

    #[test]
    fn strip_selector_basic() {
        assert_eq!(strip_selector("@dps attack"), "attack");
        assert_eq!(strip_selector("@melee @dps follow"), "@dps follow");
        assert_eq!(strip_selector("@dps"), "");
    }

    #[test]
    fn level_exact_match() {
        let mut s = snap();
        s.self_.level = 60;
        assert_eq!(level_filter(&s, "@60 attack"), "attack");
    }

    #[test]
    fn level_range_match_and_miss() {
        let mut s = snap();
        s.self_.level = 15;
        assert_eq!(level_filter(&s, "@10-20 attack"), "attack");
        s.self_.level = 25;
        assert_eq!(level_filter(&s, "@10-20 attack"), "");
    }

    #[test]
    fn level_non_numeric_pass() {
        // `@dps attack` must not look like a level tag.
        let s = snap();
        assert_eq!(level_filter(&s, "@dps attack"), "@dps attack");
    }

    #[test]
    fn group_subgroup_range() {
        let mut s = snap();
        s.group_size = 5;
        s.subgroup = 3;
        assert_eq!(group_filter(&s, "@group2-4 focus"), "focus");
        assert_eq!(group_filter(&s, "@group1 focus"), "@group1 focus"); // pass through, another bot may match
        s.subgroup = 1;
        assert_eq!(group_filter(&s, "@group1 focus"), "focus");
    }

    #[test]
    fn group_bare_requires_group() {
        let mut s = snap();
        s.group_size = 0;
        assert_eq!(group_filter(&s, "@group attack"), "@group attack");
        s.group_size = 5;
        s.subgroup = 1;
        assert_eq!(group_filter(&s, "@group attack"), "attack");
    }

    #[test]
    fn group_nogroup() {
        let mut s = snap();
        s.group_size = 0;
        assert_eq!(group_filter(&s, "@nogroup wander"), "wander");
        s.group_size = 5;
        assert_eq!(group_filter(&s, "@nogroup wander"), "@nogroup wander");
    }

    #[test]
    fn guild_gleader_and_noguild() {
        let mut s = snap();
        s.in_guild = false;
        assert_eq!(guild_filter(&s, "@noguild wander"), "wander");
        s.in_guild = true;
        s.is_guild_leader = true;
        assert_eq!(guild_filter(&s, "@gleader greet"), "greet");
        s.is_guild_leader = false;
        assert_eq!(guild_filter(&s, "@gleader greet"), "@gleader greet");
    }

    #[test]
    fn state_needrepair_and_bag() {
        let mut s = snap();
        s.durability_pct = 10;
        assert_eq!(state_filter(&s, "@needrepair vendor"), "vendor");
        s.durability_pct = 80;
        assert_eq!(state_filter(&s, "@needrepair vendor"), "@needrepair vendor");
        s.bag_space_pct = 100;
        assert_eq!(state_filter(&s, "@bagfull vendor"), "vendor");
        s.bag_space_pct = 85;
        assert_eq!(state_filter(&s, "@bagalmostfull vendor"), "vendor");
    }

    #[test]
    fn state_inside_outside() {
        let mut s = snap();
        s.is_overworld = true;
        assert_eq!(state_filter(&s, "@outside dismount"), "dismount");
        assert_eq!(state_filter(&s, "@inside dismount"), "@inside dismount");
        s.is_overworld = false;
        assert_eq!(state_filter(&s, "@inside dismount"), "dismount");
    }

    #[test]
    fn gear_tier_bands() {
        let mut s = snap();
        s.equip_gear_score = 85; // tier 3
        assert_eq!(gear_filter(&s, "@tier3 raid"), "raid");
        assert_eq!(gear_filter(&s, "@tier1-2 raid"), "");
        assert_eq!(gear_filter(&s, "@tier2-4 raid"), "raid");
    }

    #[test]
    fn combat_type_warrior_is_melee() {
        assert_eq!(
            combat_type_filter(PlayerClass::Warrior, BotRole::DPS, "@melee charge").unwrap(),
            "charge"
        );
        assert!(combat_type_filter(PlayerClass::Warrior, BotRole::DPS, "@ranged charge").is_none());
    }

    #[test]
    fn combat_type_mage_is_ranged() {
        assert_eq!(
            combat_type_filter(PlayerClass::Mage, BotRole::DPS, "@ranged blink").unwrap(),
            "blink"
        );
        assert!(combat_type_filter(PlayerClass::Mage, BotRole::DPS, "@melee blink").is_none());
    }

    #[test]
    fn combat_type_druid_tank_is_melee() {
        assert_eq!(
            combat_type_filter(PlayerClass::Druid, BotRole::TANK, "@melee rage").unwrap(),
            "rage"
        );
        assert!(combat_type_filter(PlayerClass::Druid, BotRole::TANK, "@ranged rage").is_none());
    }

    #[test]
    fn combat_type_shaman_heal_is_ranged() {
        assert_eq!(
            combat_type_filter(PlayerClass::Shaman, BotRole::HEAL, "@ranged chain heal").unwrap(),
            "chain heal"
        );
        assert!(
            combat_type_filter(PlayerClass::Shaman, BotRole::HEAL, "@melee chain heal").is_none()
        );
    }

    #[test]
    fn spec_filter_multi_word() {
        assert_eq!(
            spec_filter(PlayerSpec::DruidFeral, "@feral combat maul"),
            "maul"
        );
        assert_eq!(
            spec_filter(PlayerSpec::DruidBalance, "@feral combat maul"),
            "@feral combat maul"
        );
    }

    #[test]
    fn spec_filter_simple() {
        assert_eq!(spec_filter(PlayerSpec::MageFrost, "@frost bolt"), "bolt");
        assert_eq!(
            spec_filter(PlayerSpec::MageFire, "@frost bolt"),
            "@frost bolt"
        );
    }

    #[test]
    fn class_filter_matches_and_drops() {
        assert_eq!(
            class_filter(PlayerClass::Mage, "@mage remove curse").unwrap(),
            "remove curse"
        );
        assert!(class_filter(PlayerClass::Warrior, "@mage remove curse").is_none());
    }

    #[test]
    fn split_sep_finds_double_backslash() {
        let text = r"formation near\\stance near\\attack";
        let (h, t) = split_once_sep(text, r"\\").unwrap();
        assert_eq!(h, "formation near");
        assert_eq!(t, r"stance near\\attack");
    }

    #[test]
    fn role_heal_prefix_matches_healer() {
        assert_eq!(
            role_filter(BotRole::HEAL, "@healer attack").unwrap(),
            "attack"
        );
    }

    #[test]
    fn role_negation_before_positive() {
        assert_eq!(
            role_filter(BotRole::DPS, "@notank follow").unwrap(),
            "follow"
        );
        assert!(role_filter(BotRole::TANK, "@notank follow").is_none());
    }

    #[test]
    fn random_filter_threshold_zero_always_drops() {
        // @random=0 → roll < 0 never true → drop.
        assert_eq!(random_filter(0, "@random=0 wave"), "");
    }

    #[test]
    fn random_filter_threshold_100_always_passes() {
        // @random=100 → roll always < 100 → always pass.
        assert_eq!(random_filter(0, "@random=100 wave"), "wave");
    }

    #[test]
    fn fixed_random_threshold_100_always_passes() {
        assert_eq!(random_filter(0xABCD_1234, "@fixedrandom=100 wave"), "wave");
    }

    #[test]
    fn fixed_random_threshold_zero_always_drops() {
        assert_eq!(random_filter(0xABCD_1234, "@fixedrandom=0 wave"), "");
    }

    #[test]
    fn fixed_random_same_bot_stable_within_cycle() {
        // Two calls for the same bot handle must yield the same
        // pass/drop decision within a minute (cyclePerMin=1 means one
        // rotation per minute; two back-to-back calls in a test fall
        // in the same cycle bucket).
        let a = random_filter(0x1234, "@fixedrandom=50 x");
        let b = random_filter(0x1234, "@fixedrandom=50 x");
        assert_eq!(a, b);
    }

    #[test]
    fn fixed_random_no_tag_passthrough() {
        assert_eq!(random_filter(0, "hello"), "hello");
    }

    #[test]
    fn fixed_bot_number_is_stable_and_bounded() {
        // Stable within a short wall-clock window for the same inputs.
        let a = fixed_bot_number(0xDEAD_BEEF, 4, 100, 0.0);
        let b = fixed_bot_number(0xDEAD_BEEF, 4, 100, 0.0);
        assert_eq!(a, b);
        assert!(a <= 100);
        // Different bots generally resolve differently (not a hard
        // guarantee for any individual pair, but with these constants
        // it holds).
        let c = fixed_bot_number(0xFEED_FACE, 4, 100, 0.0);
        assert_ne!(a, c);
    }

    #[test]
    fn strategy_filter_nc_match_and_miss() {
        // `flee` is NOT a PB2 baseline default — it's layered in per-class
        // by `AiFactory::kit` (priest/mage/hunter/etc.). Insert it
        // explicitly here so the filter matches.
        let mut set = StrategySet::pb2_defaults();
        set.get_mut(BotStateKind::NonCombat)
            .insert(StrategyFlags::FLEE);
        assert!(set.has(BotStateKind::NonCombat, StrategyFlags::FLEE));
        assert_eq!(strategy_filter(&set, "@nc=flee wander"), "wander");

        // Remove flee → predicate fails → PB2 returns message unchanged.
        set.get_mut(BotStateKind::NonCombat)
            .remove(StrategyFlags::FLEE);
        assert_eq!(strategy_filter(&set, "@nc=flee wander"), "@nc=flee wander");
    }

    #[test]
    fn strategy_filter_negated_forms() {
        // PB2 nonCombat baseline = `+return,+delayed roll` only; `flee`
        // is layered in per-class by AiFactory. Add it manually so the
        // `@nonc=flee` path exercises a match.
        let mut set = StrategySet::pb2_defaults();
        set.get_mut(BotStateKind::NonCombat)
            .insert(StrategyFlags::FLEE);
        // `grind` is NOT in the NonCombat default set.
        assert!(!set.has(BotStateKind::NonCombat, StrategyFlags::GRIND));
        assert_eq!(strategy_filter(&set, "@nonc=grind idle"), "idle");
        // `flee` IS in the NonCombat set → @nonc=flee must NOT strip.
        assert_eq!(strategy_filter(&set, "@nonc=flee idle"), "@nonc=flee idle");
    }

    #[test]
    fn strategy_filter_combat_state_slot() {
        let mut set = StrategySet::default();
        set.get_mut(BotStateKind::Combat).insert(StrategyFlags::CC);
        // `@co=cc` keys on Combat slot, not NonCombat.
        assert_eq!(strategy_filter(&set, "@co=cc focus"), "focus");
        assert_eq!(strategy_filter(&set, "@nc=cc focus"), "@nc=cc focus");
        assert_eq!(strategy_filter(&set, "@noco=cc focus"), "@noco=cc focus");
    }

    #[test]
    fn strategy_filter_unknown_name_reports_absent() {
        // Unknown strategy name: `parse_name` returns None → treat as
        // not-enabled. `@co=unknown` → predicate false → no-op.
        // `@noco=unknown` → predicate true → strip.
        let set = StrategySet::default();
        assert_eq!(strategy_filter(&set, "@co=travel move"), "@co=travel move");
        assert_eq!(strategy_filter(&set, "@noco=travel move"), "move");
    }

    /// Write `s` into a fixed-size `[c_char; N]` buffer, NUL-padded.
    fn write_cbuf<const N: usize>(buf: &mut [std::ffi::c_char; N], s: &str) {
        for (i, b) in s.bytes().enumerate().take(N - 1) {
            buf[i] = b as std::ffi::c_char;
        }
        buf[s.len().min(N - 1)] = 0;
    }

    #[test]
    fn guild_filter_name_prefix_match() {
        let mut s = snap();
        s.in_guild = true;
        write_cbuf(&mut s.guild_name, "Heroes");
        // PB2's inverted comparison: typed token must start with bot's
        // guild name. "Heroes of Azeroth" starts with "Heroes" ✓.
        assert_eq!(
            guild_filter(&s, "@guild=Heroes of Azeroth hail"),
            "of Azeroth hail"
        );
        // "Heroe" does not start with "Heroes" → fail.
        assert_eq!(guild_filter(&s, "@guild=Heroe hail"), "@guild=Heroe hail");
    }

    #[test]
    fn guild_filter_rank_match() {
        let mut s = snap();
        s.in_guild = true;
        write_cbuf(&mut s.guild_rank_name, "Officer");
        assert_eq!(guild_filter(&s, "@rank=Officer salute"), "salute");
        assert_eq!(
            guild_filter(&s, "@rank=Initiate salute"),
            "@rank=Initiate salute"
        );
    }

    #[test]
    fn guild_filter_guild_eq_not_in_guild_is_passthrough() {
        let mut s = snap();
        s.in_guild = false;
        assert_eq!(guild_filter(&s, "@guild=Heroes hail"), "@guild=Heroes hail");
    }

    #[test]
    fn rti_filter_not_in_group_passes_through() {
        let mut s = snap();
        s.group_size = 0;
        assert_eq!(rti_filter(&s, "@skull attack").unwrap(), "@skull attack");
    }

    #[test]
    fn rti_filter_target_marked_strips() {
        let mut s = snap();
        s.group_size = 2;
        s.group_raid_target_icons[7] = 0xDEAD_BEEF; // skull
        s.self_.current_target = 0xDEAD_BEEF;
        assert_eq!(rti_filter(&s, "@skull attack").unwrap(), "attack");
    }

    #[test]
    fn rti_filter_no_target_drops() {
        let mut s = snap();
        s.group_size = 2;
        s.group_raid_target_icons[7] = 0xDEAD_BEEF;
        s.self_.current_target = 0;
        assert!(rti_filter(&s, "@skull attack").is_none());
    }

    #[test]
    fn rti_filter_wrong_target_drops() {
        let mut s = snap();
        s.group_size = 2;
        s.group_raid_target_icons[7] = 0xDEAD_BEEF;
        s.self_.current_target = 0x1234_5678;
        assert!(rti_filter(&s, "@skull attack").is_none());
    }

    #[test]
    fn rti_filter_no_tag_passthrough() {
        let s = snap();
        assert_eq!(rti_filter(&s, "attack").unwrap(), "attack");
    }

    #[test]
    fn location_filter_map_prefix() {
        let mut s = snap();
        write_cbuf(&mut s.map_name_lower, "azeroth");
        assert_eq!(location_filter(&s, "@azeroth travel"), "travel");
        // Must not match the prefix of a longer word.
        assert_eq!(
            location_filter(&s, "@azerothian travel"),
            "@azerothian travel"
        );
    }

    #[test]
    fn location_filter_area_prefix() {
        let mut s = snap();
        write_cbuf(&mut s.area_name_lower, "dun morogh");
        assert_eq!(location_filter(&s, "@dun morogh travel"), "travel");
        assert_eq!(location_filter(&s, "@dun follow"), "@dun follow");
    }

    #[test]
    fn location_filter_no_tag_passthrough() {
        let s = snap();
        assert_eq!(location_filter(&s, "attack"), "attack");
        assert_eq!(location_filter(&s, "@dps attack"), "@dps attack");
    }

    #[test]
    fn quest_filter_numeric_match() {
        let mut s = snap();
        s.current_quest_ids[0] = 523;
        s.current_quest_count = 1;
        assert_eq!(quest_filter(&s, "@quest=523 turn in"), "turn in");
        assert_eq!(quest_filter(&s, "@quest=999 turn in"), "@quest=999 turn in");
    }

    #[test]
    fn quest_filter_link_form() {
        let mut s = snap();
        s.current_quest_ids[0] = 40;
        s.current_quest_count = 1;
        // Classic link grammar: |Hquest:<id>:<level>|h[name]|h|r
        let msg = "@quest=|cffffff00|Hquest:40:5|h[A Threat Within]|h|r help";
        assert_eq!(quest_filter(&s, msg), "help");
    }

    #[test]
    fn quest_filter_no_match_passthrough() {
        let mut s = snap();
        s.current_quest_count = 0;
        assert_eq!(quest_filter(&s, "@quest=40 help"), "@quest=40 help");
    }

    #[test]
    fn extract_quest_link_ids_handles_multiple() {
        let mut out = Vec::new();
        extract_quest_link_ids("|Hquest:10:5|h[A]|h|r and |Hquest:20:10|h[B]|h|r", &mut out);
        assert_eq!(out, vec![10, 20]);
    }

    #[test]
    fn strategy_filter_no_tag_passthrough() {
        let set = StrategySet::default();
        assert_eq!(strategy_filter(&set, "attack"), "attack");
        assert_eq!(strategy_filter(&set, "@dps attack"), "@dps attack");
    }

    #[test]
    fn no_selector_unchanged_through_filters() {
        // Directly exercise each leaf filter: no `@` → pass through.
        let s = snap();
        assert_eq!(role_filter(BotRole::DPS, "attack").unwrap(), "attack");
        assert_eq!(level_filter(&s, "attack"), "attack");
        assert_eq!(group_filter(&s, "attack"), "attack");
        assert_eq!(guild_filter(&s, "attack"), "attack");
        assert_eq!(state_filter(&s, "attack"), "attack");
        assert_eq!(gear_filter(&s, "attack"), "attack");
        assert_eq!(random_filter(0, "attack"), "attack");
    }
}
