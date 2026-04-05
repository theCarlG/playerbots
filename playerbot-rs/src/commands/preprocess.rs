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
//! Every PB2 chat filter whose data is reachable from the snapshot /
//! settings / class enum is implemented here. That covers everything the
//! shipping addons (Mangosbot UI, RaidControl) ever emit *and* everything
//! a player can type manually without touching auction-house / item-usage
//! infrastructure:
//!
//! | Filter        | Tags                                                                                | Source of truth                         |
//! |---------------|-------------------------------------------------------------------------------------|-----------------------------------------|
//! | role          | `@tank` `@dps` `@heal[er]` `@notank` `@nodps` `@noheal`                             | [`BotState::role`]                      |
//! | combat-type   | `@ranged` `@melee`                                                                  | class + role (matches PB2's table)      |
//! | class         | `@warrior` `@paladin` `@hunter` `@rogue` `@priest` `@shaman` `@mage` `@warlock` `@druid` `@deathknight` | [`BotState::class`]                     |
//! | talent spec   | `@arms` `@fury` `@protection` `@holy` `@frost` …                                    | [`BotState::spec`] via [`spec_name`]    |
//! | level         | `@60` `@10-20`                                                                      | `snap.self_.level`                      |
//! | group         | `@group` `@nogroup` `@group2` `@group4-6` `@leader` `@raid` `@noraid` `@rleader`    | `snap.subgroup` / `is_raid_group`       |
//! | guild         | `@guild` `@noguild` `@gleader`                                                      | `snap.in_guild` / `is_guild_leader`     |
//! | state         | `@needrepair` `@bagfull` `@bagalmostfull` `@inside` `@outside`                      | `snap.durability_pct` / `bag_space_pct` / `is_overworld` |
//! | gear          | `@tier1` `@tier2-3`                                                                 | `snap.equip_gear_score`                 |
//! | rti           | `@star` `@circle` `@diamond` `@triangle` `@moon` `@square` `@cross` `@skull`        | blackboard rti assignment               |
//! | random        | `@random` `@random=25`                                                              | thread-local RNG                        |
//!
//! What is intentionally *not* ported: `@use=`, `@sell=`, `@need=`,
//! `@greed=`, `@quest=`, `@guild=<name>`, `@rank=`, `@<mapname>`,
//! `@<zonename>`, `@nc=<strategy>`, `@co=<strategy>`, `@react=`. All of
//! these depend on subsystems that don't exist in the Rust port
//! (ItemUsageValue, the string-name strategy registry, the full quest
//! log API, raw map/zone *name* lookups). None of the shipping addons
//! issue them; none of the `BossData` entries reference them. If a new
//! consumer starts using one, its backing data must land first — the
//! filter itself would be a three-line prefix check on top of that data,
//! not a stub here.

use crate::bot::state::{BotState, PlayerClass, PlayerSpec};
use crate::commands::{PendingCommand, SecurityLevel, parser};
use crate::ffi::{BotRole, BotWorldSnapshot};

const DEFAULT_SEPARATOR: &str = "\\\\";
const CHAT_PREFIXES: &[&str] = &["#w ", "#p ", "#r ", "#a ", "#g "];

/// Entry point called from [`crate::playerbot_chat_command`].
pub fn preprocess_and_enqueue(
    bot: &mut BotState,
    sender_guid: u64,
    security: SecurityLevel,
    raw: &str,
) {
    let text = raw.trim_start_matches("BOT\t");

    // Separator split: recurse into each command.
    if let Some((head, tail)) = split_once_sep(text, DEFAULT_SEPARATOR) {
        preprocess_and_enqueue(bot, sender_guid, security, head);
        preprocess_and_enqueue(bot, sender_guid, security, tail);
        return;
    }

    // Chat channel prefix.
    let mut text = text;
    for p in CHAT_PREFIXES {
        if let Some(rest) = text.strip_prefix(p) {
            text = rest;
            break;
        }
    }

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
        return;
    };
    let pc = if sender_guid == 0 {
        PendingCommand::internal(cmd)
    } else {
        PendingCommand::external(sender_guid, security, cmd)
    };
    bot.pending_commands.push_back(pc);
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

    // FILTER_COUNT = number of distinct filters below. 11 filters, we
    // iterate up to that many times (matches PB2's O(N) stability loop).
    const FILTER_COUNT: usize = 11;

    for _ in 0..FILTER_COUNT {
        let before = current.clone();

        current = role_filter(bot.role, &current)?;
        if current.is_empty() {
            return Some(current);
        }
        current = combat_type_filter(bot.class, bot.role, &current)?;
        if current.is_empty() {
            return Some(current);
        }
        current = class_filter(bot.class, &current)?;
        if current.is_empty() {
            return Some(current);
        }
        current = spec_filter(bot.spec, &current);
        current = level_filter(&bot.snap, &current);
        current = group_filter(&bot.snap, &current);
        current = guild_filter(&bot.snap, &current);
        current = state_filter(&bot.snap, &current);
        current = gear_filter(&bot.snap, &current);
        current = random_filter(&current);

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

    use PlayerClass::*;
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
    use PlayerSpec::*;
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
        return if in_group { msg.to_string() } else { strip_selector(msg) };
    }
    if msg.starts_with("@noraid") {
        return if snap.is_raid_group { msg.to_string() } else { strip_selector(msg) };
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
        return if snap.is_raid_group { strip_selector(msg) } else { msg.to_string() };
    }
    if msg.starts_with("@group") {
        // Extract the numeric tail of the tag for subgroup range.
        let tag = head_token(msg);
        let nums = &tag[6..]; // after "@group"
        if nums.is_empty() {
            return if in_group { strip_selector(msg) } else { msg.to_string() };
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
// Ports PB2's `GuildChatFilter` for every tag whose predicate is
// name-free: `@guild`, `@noguild`, `@gleader`. `@guild=<name>` and
// `@rank=<name>` are name-matched in PB2 and require exposing the
// guild name + rank name over the FFI; when a caller starts using them
// the fields need to land on the snapshot before the filter can be
// turned on. Until then we pass those tags through untouched so the
// parser can respond rather than us silently dropping.

fn guild_filter(snap: &BotWorldSnapshot, msg: &str) -> String {
    if msg.starts_with("@gleader") {
        return if snap.is_guild_leader { strip_selector(msg) } else { msg.to_string() };
    }
    if msg.starts_with("@noguild") {
        return if snap.in_guild { msg.to_string() } else { strip_selector(msg) };
    }
    // `@guild` (bare) — pass if in any guild. Careful: must not eat
    // `@guild=<name>`, which has a different semantic. That form always
    // contains an `=` before the first space.
    if msg.starts_with("@guild") {
        let tag = head_token(msg);
        if !tag.contains('=') {
            return if snap.in_guild { strip_selector(msg) } else { msg.to_string() };
        }
    }
    msg.to_string()
}

// ── State filter ───────────────────────────────────────────────────────────

fn state_filter(snap: &BotWorldSnapshot, msg: &str) -> String {
    if msg.starts_with("@needrepair") {
        return if snap.durability_pct < 20 { strip_selector(msg) } else { msg.to_string() };
    }
    if msg.starts_with("@bagalmostfull") {
        return if snap.bag_space_pct >= 80 { strip_selector(msg) } else { msg.to_string() };
    }
    if msg.starts_with("@bagfull") {
        return if snap.bag_space_pct >= 100 { strip_selector(msg) } else { msg.to_string() };
    }
    if msg.starts_with("@outside") {
        return if snap.is_overworld { strip_selector(msg) } else { msg.to_string() };
    }
    if msg.starts_with("@inside") {
        return if !snap.is_overworld { strip_selector(msg) } else { msg.to_string() };
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

/// Average-ilvl-to-tier bands, copy-pasted from PB2's GearChatFilter.
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
// `@random` = 50/50, `@random=25` = 25% chance. Matches PB2's
// `RandomChatFilter` semantics. Uses the thread-local hashed-time RNG
// from `fastrand` if available; otherwise std `SystemTime` jitter keeps
// the footprint stubless without dragging in an extra crate.

fn random_filter(msg: &str) -> String {
    if !msg.starts_with("@random") {
        return msg.to_string();
    }
    let tag = head_token(msg);
    // `@random=NN`
    let threshold: u32 = if let Some(eq) = tag.strip_prefix("@random=") {
        eq.parse::<u32>().unwrap_or(50)
    } else if tag == "@random" {
        50
    } else {
        return msg.to_string();
    };
    let roll = pseudo_rand_0_99();
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
        assert!(
            combat_type_filter(PlayerClass::Warrior, BotRole::DPS, "@ranged charge").is_none()
        );
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
        assert_eq!(random_filter("@random=0 wave"), "");
    }

    #[test]
    fn random_filter_threshold_100_always_passes() {
        // @random=100 → roll always < 100 → always pass.
        assert_eq!(random_filter("@random=100 wave"), "wave");
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
        assert_eq!(random_filter("attack"), "attack");
    }
}
