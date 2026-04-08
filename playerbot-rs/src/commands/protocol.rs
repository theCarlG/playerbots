//! Dual command API — transport abstraction, target selectors, addon protocol.
//!
//! Both chat and addon channels feed into the same `BotCommand` enum.
//! This module provides:
//! - `Transport`: which channel the command arrived on
//! - `TargetSelector`: who should execute the command (`@warrior`, `@healers`, etc.)
//! - `IncomingCommand`: wraps `BotCommand` with transport + selector metadata
//! - Addon protocol parser: `PB:v1:cmd:arg1:arg2|cmd2:arg1`

use std::ops::RangeInclusive;

use crate::bot::state::{PlayerClass, PlayerSpec};
use crate::ffi::BotRole;

/// Which transport a command arrived on. Determines how responses are routed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// `SendAddonMessage` on a registered prefix — silent, structured, batchable.
    Addon,
    /// /whisper from a player.
    Whisper,
    /// /party chat.
    Party,
    /// /raid chat.
    Raid,
    /// /say (only works within range).
    Say,
    /// Internal / system-injected (RTSC spell, tests). Not a real transport.
    Internal,
}

impl Transport {
    /// Derive transport from a `ChatOrigin`.
    pub fn from_chat_origin(origin: &super::ChatOrigin) -> Self {
        if origin.is_addon() {
            return Self::Addon;
        }
        match origin.chat_type {
            0x06 => Self::Whisper, // CHAT_MSG_WHISPER
            0x02 => Self::Party,   // CHAT_MSG_PARTY
            0x03 => Self::Raid,    // CHAT_MSG_RAID
            0x01 => Self::Say,     // CHAT_MSG_SAY
            0 if origin.lang == 0 => Self::Internal,
            _ => Self::Whisper, // fallback
        }
    }
}

/// Target selector — determines which bots in the group should execute a command.
///
/// Commands can be addressed to subsets of bots using `@`-prefixed selectors
/// in both chat and addon channels. Multiple selectors combine with AND logic
/// (e.g. `@warrior @group1` = warriors in group 1).
#[derive(Debug, Clone, PartialEq)]
pub enum TargetSelector {
    /// All bots in the group.
    All,
    /// A specific bot by name (whisper targeting).
    ByName(String),
    /// All bots of a class: `@warrior`, `@priest`, `@mage`, etc.
    ByClass(PlayerClass),
    /// All bots with a role: `@tanks`, `@healers`, `@dps`.
    ByRole(BotRole),
    /// All bots of a spec: `@arms`, `@holy`, `@feral`, etc.
    BySpec(PlayerSpec),
    /// Bots in raid group(s): `@group1`, `@group1-3`.
    ByGroup(RangeInclusive<u8>),
    /// Main tank: `@mt`.
    MainTank,
    /// Off tank(s): `@ot`.
    OffTank,
}

impl TargetSelector {
    /// Parse a single `@`-prefixed selector token.
    ///
    /// Returns `None` if the token doesn't start with `@` or is unrecognized.
    pub fn parse(token: &str) -> Option<Self> {
        let tag = token.strip_prefix('@')?;
        let lower = tag.to_ascii_lowercase();

        // Role selectors
        match lower.as_str() {
            "all" => return Some(Self::All),
            "tanks" | "tank" => return Some(Self::ByRole(BotRole::TANK)),
            "healers" | "healer" | "heal" => return Some(Self::ByRole(BotRole::HEAL)),
            "dps" | "damage" => return Some(Self::ByRole(BotRole::DPS)),
            "mt" => return Some(Self::MainTank),
            "ot" => return Some(Self::OffTank),
            _ => {}
        }

        // Class selectors
        if let Some(class) = parse_class(&lower) {
            return Some(Self::ByClass(class));
        }

        // Spec selectors
        if let Some(spec) = parse_spec(&lower) {
            return Some(Self::BySpec(spec));
        }

        // Group selectors: @group1, @group1-3
        if let Some(rest) = lower.strip_prefix("group")
            && let Some(range) = parse_group_range(rest) {
                return Some(Self::ByGroup(range));
            }

        None
    }

    /// Check if this selector matches a bot with the given attributes.
    pub fn matches(
        &self,
        name: &str,
        class: PlayerClass,
        spec: PlayerSpec,
        role: BotRole,
        raid_group: u8,
        is_main_tank: bool,
        is_off_tank: bool,
    ) -> bool {
        match self {
            Self::All => true,
            Self::ByName(n) => n.eq_ignore_ascii_case(name),
            Self::ByClass(c) => class == *c,
            Self::ByRole(r) => role == *r,
            Self::BySpec(s) => spec == *s,
            Self::ByGroup(range) => range.contains(&raid_group),
            Self::MainTank => is_main_tank,
            Self::OffTank => is_off_tank,
        }
    }
}

fn parse_class(s: &str) -> Option<PlayerClass> {
    match s {
        "warrior" | "warriors" => Some(PlayerClass::Warrior),
        "paladin" | "paladins" => Some(PlayerClass::Paladin),
        "priest" | "priests" => Some(PlayerClass::Priest),
        "druid" | "druids" => Some(PlayerClass::Druid),
        "hunter" | "hunters" => Some(PlayerClass::Hunter),
        "mage" | "mages" => Some(PlayerClass::Mage),
        "rogue" | "rogues" => Some(PlayerClass::Rogue),
        "shaman" | "shamans" => Some(PlayerClass::Shaman),
        "warlock" | "warlocks" => Some(PlayerClass::Warlock),
        "deathknight" | "deathknights" | "dk" | "dks" => Some(PlayerClass::DeathKnight),
        _ => None,
    }
}

fn parse_spec(s: &str) -> Option<PlayerSpec> {
    match s {
        // Warrior
        "arms" => Some(PlayerSpec::WarriorArms),
        "fury" => Some(PlayerSpec::WarriorFury),
        "protection" | "prot" => None, // ambiguous (warrior/paladin) — use @class + @spec
        // Paladin
        "retribution" | "ret" => Some(PlayerSpec::PaladinRetribution),
        // Priest
        "discipline" | "disc" => Some(PlayerSpec::PriestDiscipline),
        "shadow" => Some(PlayerSpec::PriestShadow),
        "holy" => None, // ambiguous (priest/paladin)
        // Druid
        "balance" | "boomkin" | "moonkin" => Some(PlayerSpec::DruidBalance),
        "feral" => Some(PlayerSpec::DruidFeral),
        "restoration" | "resto" => None, // ambiguous (druid/shaman)
        // Hunter
        "beastmastery" | "bm" => Some(PlayerSpec::HunterBeastMastery),
        "marksmanship" | "marks" | "mm" => Some(PlayerSpec::HunterMarksmanship),
        "survival" | "surv" => Some(PlayerSpec::HunterSurvival),
        // Mage
        "arcane" => Some(PlayerSpec::MageArcane),
        "fire" => Some(PlayerSpec::MageFire),
        "frost" => None, // ambiguous (mage/dk)
        // Rogue
        "assassination" | "assa" => Some(PlayerSpec::RogueAssassination),
        "combat" => Some(PlayerSpec::RogueCombat),
        "subtlety" | "sub" => Some(PlayerSpec::RogueSubtlety),
        // Shaman
        "elemental" | "ele" => Some(PlayerSpec::ShamanElemental),
        "enhancement" | "enh" => Some(PlayerSpec::ShamanEnhancement),
        // Warlock
        "affliction" | "aff" => Some(PlayerSpec::WarlockAffliction),
        "demonology" | "demo" => Some(PlayerSpec::WarlockDemonology),
        "destruction" | "destro" => Some(PlayerSpec::WarlockDestruction),
        // Death Knight
        "blood" => Some(PlayerSpec::DeathKnightBlood),
        "unholy" => Some(PlayerSpec::DeathKnightUnholy),
        _ => None,
    }
}

fn parse_group_range(s: &str) -> Option<RangeInclusive<u8>> {
    if let Some((start, end)) = s.split_once('-') {
        let start: u8 = start.parse().ok()?;
        let end: u8 = end.parse().ok()?;
        if start >= 1 && end <= 8 && start <= end {
            Some(start..=end)
        } else {
            None
        }
    } else {
        let n: u8 = s.parse().ok()?;
        if (1..=8).contains(&n) { Some(n..=n) } else { None }
    }
}

/// An incoming command with full metadata for both chat and addon channels.
///
/// This is the unified wrapper that both transports produce. The existing
/// `PendingCommand` struct is kept for the internal queue (it's what crosses
/// the thread boundary via Mutex). `IncomingCommand` is the richer form used
/// during dispatch to evaluate target selectors and route responses.
#[derive(Debug, Clone)]
pub struct IncomingCommand {
    /// Who sent this command (player GUID). `None` for system-injected.
    pub sender: Option<u64>,
    /// Security tier granted to the sender.
    pub security: super::SecurityLevel,
    /// Which channel the command arrived on.
    pub transport: Transport,
    /// Who should execute it. Empty = the directly-addressed bot only.
    pub selectors: Vec<TargetSelector>,
    /// The command(s). Addon channel can batch multiple commands per message.
    pub commands: Vec<super::BotCommand>,
}

/// Parse an addon channel message in the `PB:v1:...` protocol.
///
/// Format: `PB:v1:[@selector]:cmd:arg1:arg2|cmd2:arg1`
/// - `PB` is the addon prefix (already stripped before this function).
/// - `v1` is the protocol version.
/// - Optional `@selector` tokens appear before the first command.
/// - `|` separates batched commands.
/// - `:` separates command name and arguments within each command.
///
/// Returns parsed selectors and raw command strings for further parsing
/// by the text command parser.
pub fn parse_addon_message(payload: &str) -> Option<(Vec<TargetSelector>, Vec<&str>)> {
    // Expect "v1:" prefix.
    let rest = payload.strip_prefix("v1:")?;

    let mut selectors = Vec::new();
    let mut command_start = rest;

    // Scan for leading @selector tokens (colon-separated).
    loop {
        let (token, remainder) = match command_start.split_once(':') {
            Some(pair) => pair,
            None => (command_start, ""),
        };

        if let Some(sel) = TargetSelector::parse(token) {
            selectors.push(sel);
            command_start = remainder;
        } else {
            break;
        }
    }

    // Split remaining on `|` for batched commands.
    let commands: Vec<&str> = command_start.split('|').filter(|s| !s.is_empty()).collect();

    if commands.is_empty() {
        return None;
    }

    Some((selectors, commands))
}

/// Extract `@`-prefixed selectors from chat command tokens and return the
/// remaining tokens.
///
/// Example: `["@warrior", "@group1", "follow", "me"]`
/// → selectors: `[ByClass(Warrior), ByGroup(1..=1)]`, rest: `["follow", "me"]`
pub fn extract_chat_selectors<'a>(tokens: &'a [&'a str]) -> (Vec<TargetSelector>, Vec<&'a str>) {
    let mut selectors = Vec::new();
    let mut rest = Vec::new();
    for &token in tokens {
        if token.starts_with('@')
            && let Some(sel) = TargetSelector::parse(token) {
                selectors.push(sel);
                continue;
            }
        rest.push(token);
    }
    (selectors, rest)
}

// ── Structured Addon Responses ──────────────────────────────────────────────

/// State update categories that addons can subscribe to.
///
/// Each category maps to a kind of periodic or event-driven state push
/// that the bot sends back over the addon channel. Addons register which
/// categories they care about to avoid flooding the channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum StateCategory {
    /// FSM state + `WorldSub` (changes only).
    Fsm = 0,
    /// Health/mana percentages (throttled, every ~2s).
    Vitals = 1,
    /// Encounter phase + boss info (changes only).
    Encounter = 2,
    /// Active strategies summary (changes only).
    Strategies = 3,
    /// Group coordination snapshot (MT, MA, claims).
    Coordination = 4,
    /// BT execution path (changes only).
    BtPath = 5,
}

impl StateCategory {
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "fsm" => Some(Self::Fsm),
            "vitals" | "health" | "hp" => Some(Self::Vitals),
            "encounter" | "enc" => Some(Self::Encounter),
            "strategies" | "strats" => Some(Self::Strategies),
            "coordination" | "coord" => Some(Self::Coordination),
            "bt" | "btpath" => Some(Self::BtPath),
            _ => None,
        }
    }
}

/// Subscription set — which state categories a player wants updates for.
///
/// Stored per-subscriber (player GUID) on the bot. When no subscriptions
/// exist, no state pushes are sent (zero overhead).
#[derive(Debug, Clone, Default)]
pub struct AddonSubscriptions {
    /// (subscriber GUID, bitmask of `StateCategory` values).
    entries: Vec<(u64, u8)>,
}

impl AddonSubscriptions {
    /// Subscribe a player to a category. Idempotent.
    pub fn subscribe(&mut self, guid: u64, cat: StateCategory) {
        let bit = 1u8 << (cat as u8);
        if let Some(entry) = self.entries.iter_mut().find(|(g, _)| *g == guid) {
            entry.1 |= bit;
        } else {
            self.entries.push((guid, bit));
        }
    }

    /// Unsubscribe a player from a category.
    pub fn unsubscribe(&mut self, guid: u64, cat: StateCategory) {
        let bit = 1u8 << (cat as u8);
        if let Some(entry) = self.entries.iter_mut().find(|(g, _)| *g == guid) {
            entry.1 &= !bit;
        }
        self.entries.retain(|(_, mask)| *mask != 0);
    }

    /// Unsubscribe a player from all categories.
    pub fn unsubscribe_all(&mut self, guid: u64) {
        self.entries.retain(|(g, _)| *g != guid);
    }

    /// Returns all GUIDs subscribed to a given category.
    pub fn subscribers(&self, cat: StateCategory) -> impl Iterator<Item = u64> + '_ {
        let bit = 1u8 << (cat as u8);
        self.entries
            .iter()
            .filter(move |(_, mask)| mask & bit != 0)
            .map(|(g, _)| *g)
    }

    /// True if anyone is subscribed to this category.
    pub fn has_subscribers(&self, cat: StateCategory) -> bool {
        let bit = 1u8 << (cat as u8);
        self.entries.iter().any(|(_, mask)| mask & bit != 0)
    }

    /// True if no subscriptions exist at all.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Format a structured addon response message.
///
/// Format: `PB:v1:cat:key=value:key=value`
/// This is the bot→addon direction. The addon parses these to update its UI.
pub fn format_state_update(cat: StateCategory, kvs: &[(&str, &str)]) -> String {
    let cat_name = match cat {
        StateCategory::Fsm => "fsm",
        StateCategory::Vitals => "vitals",
        StateCategory::Encounter => "enc",
        StateCategory::Strategies => "strats",
        StateCategory::Coordination => "coord",
        StateCategory::BtPath => "bt",
    };
    let mut msg = format!("PB:v1:{cat_name}");
    for (k, v) in kvs {
        msg.push(':');
        msg.push_str(k);
        msg.push('=');
        msg.push_str(v);
    }
    msg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_role_selectors() {
        assert_eq!(
            TargetSelector::parse("@tanks"),
            Some(TargetSelector::ByRole(BotRole::TANK))
        );
        assert_eq!(
            TargetSelector::parse("@healers"),
            Some(TargetSelector::ByRole(BotRole::HEAL))
        );
        assert_eq!(
            TargetSelector::parse("@dps"),
            Some(TargetSelector::ByRole(BotRole::DPS))
        );
    }

    #[test]
    fn parse_class_selectors() {
        assert_eq!(
            TargetSelector::parse("@warrior"),
            Some(TargetSelector::ByClass(PlayerClass::Warrior))
        );
        assert_eq!(
            TargetSelector::parse("@Mage"),
            Some(TargetSelector::ByClass(PlayerClass::Mage))
        );
    }

    #[test]
    fn parse_spec_selectors() {
        assert_eq!(
            TargetSelector::parse("@arms"),
            Some(TargetSelector::BySpec(PlayerSpec::WarriorArms))
        );
        assert_eq!(
            TargetSelector::parse("@bm"),
            Some(TargetSelector::BySpec(PlayerSpec::HunterBeastMastery))
        );
    }

    #[test]
    fn parse_group_selectors() {
        assert_eq!(
            TargetSelector::parse("@group1"),
            Some(TargetSelector::ByGroup(1..=1))
        );
        assert_eq!(
            TargetSelector::parse("@group1-3"),
            Some(TargetSelector::ByGroup(1..=3))
        );
        assert!(TargetSelector::parse("@group0").is_none());
        assert!(TargetSelector::parse("@group9").is_none());
    }

    #[test]
    fn parse_special_selectors() {
        assert_eq!(TargetSelector::parse("@all"), Some(TargetSelector::All));
        assert_eq!(TargetSelector::parse("@mt"), Some(TargetSelector::MainTank));
        assert_eq!(TargetSelector::parse("@ot"), Some(TargetSelector::OffTank));
    }

    #[test]
    fn unknown_selector_returns_none() {
        assert!(TargetSelector::parse("@blahblah").is_none());
        assert!(TargetSelector::parse("warrior").is_none()); // no @
    }

    #[test]
    fn addon_protocol_basic() {
        let (sels, cmds) = parse_addon_message("v1:follow").unwrap();
        assert!(sels.is_empty());
        assert_eq!(cmds, vec!["follow"]);
    }

    #[test]
    fn addon_protocol_with_selectors() {
        let (sels, cmds) = parse_addon_message("v1:@warrior:@group1:co +aoe").unwrap();
        assert_eq!(sels.len(), 2);
        assert_eq!(sels[0], TargetSelector::ByClass(PlayerClass::Warrior));
        assert_eq!(sels[1], TargetSelector::ByGroup(1..=1));
        assert_eq!(cmds, vec!["co +aoe"]);
    }

    #[test]
    fn addon_protocol_batched() {
        let (sels, cmds) = parse_addon_message("v1:@healers:follow|co +heal").unwrap();
        assert_eq!(sels.len(), 1);
        assert_eq!(cmds, vec!["follow", "co +heal"]);
    }

    #[test]
    fn addon_protocol_no_version_fails() {
        assert!(parse_addon_message("v2:follow").is_none());
        assert!(parse_addon_message("follow").is_none());
    }

    #[test]
    fn extract_chat_selectors_works() {
        let tokens = vec!["@warrior", "@group1", "follow", "me"];
        let (sels, rest) = extract_chat_selectors(&tokens);
        assert_eq!(sels.len(), 2);
        assert_eq!(rest, vec!["follow", "me"]);
    }

    #[test]
    fn selector_matches_class() {
        let sel = TargetSelector::ByClass(PlayerClass::Warrior);
        assert!(sel.matches(
            "Bot1",
            PlayerClass::Warrior,
            PlayerSpec::WarriorFury,
            BotRole::DPS,
            1,
            false,
            false,
        ));
        assert!(!sel.matches(
            "Bot2",
            PlayerClass::Mage,
            PlayerSpec::MageFrost,
            BotRole::DPS,
            1,
            false,
            false,
        ));
    }

    #[test]
    fn selector_matches_group_range() {
        let sel = TargetSelector::ByGroup(1..=3);
        assert!(sel.matches(
            "Bot",
            PlayerClass::Warrior,
            PlayerSpec::WarriorFury,
            BotRole::DPS,
            2,
            false,
            false
        ));
        assert!(!sel.matches(
            "Bot",
            PlayerClass::Warrior,
            PlayerSpec::WarriorFury,
            BotRole::DPS,
            5,
            false,
            false
        ));
    }

    #[test]
    fn selector_matches_mt_ot() {
        let mt = TargetSelector::MainTank;
        let ot = TargetSelector::OffTank;
        assert!(mt.matches(
            "Bot",
            PlayerClass::Warrior,
            PlayerSpec::WarriorFury,
            BotRole::TANK,
            1,
            true,
            false
        ));
        assert!(!mt.matches(
            "Bot",
            PlayerClass::Warrior,
            PlayerSpec::WarriorFury,
            BotRole::TANK,
            1,
            false,
            true
        ));
        assert!(ot.matches(
            "Bot",
            PlayerClass::Warrior,
            PlayerSpec::WarriorFury,
            BotRole::TANK,
            1,
            false,
            true
        ));
        assert!(!ot.matches(
            "Bot",
            PlayerClass::Warrior,
            PlayerSpec::WarriorFury,
            BotRole::TANK,
            1,
            true,
            false
        ));
    }

    #[test]
    fn addon_protocol_colon_args_normalize() {
        // Addon protocol uses colons as separators within commands.
        // The ingestion code normalizes them to spaces before parsing.
        let (sels, cmds) = parse_addon_message("v1:set mt").unwrap();
        assert!(sels.is_empty());
        assert_eq!(cmds, vec!["set mt"]);

        // With selector and batched commands.
        let (sels, cmds) = parse_addon_message("v1:@tanks:follow|set mt").unwrap();
        assert_eq!(sels.len(), 1);
        assert_eq!(sels[0], TargetSelector::ByRole(BotRole::TANK));
        assert_eq!(cmds, vec!["follow", "set mt"]);
    }

    #[test]
    fn selector_and_logic() {
        // @warrior @group1 should match a warrior in group 1 but not group 2.
        let sels = vec![
            TargetSelector::ByClass(PlayerClass::Warrior),
            TargetSelector::ByGroup(1..=1),
        ];
        assert!(sels.iter().all(|s| s.matches(
            "Bot",
            PlayerClass::Warrior,
            PlayerSpec::WarriorFury,
            BotRole::DPS,
            1,
            false,
            false
        )));
        assert!(!sels.iter().all(|s| s.matches(
            "Bot",
            PlayerClass::Warrior,
            PlayerSpec::WarriorFury,
            BotRole::DPS,
            2,
            false,
            false
        )));
        assert!(!sels.iter().all(|s| s.matches(
            "Bot",
            PlayerClass::Mage,
            PlayerSpec::MageFrost,
            BotRole::DPS,
            1,
            false,
            false
        )));
    }

    #[test]
    fn addon_subscriptions_basic() {
        let mut subs = AddonSubscriptions::default();
        assert!(subs.is_empty());

        subs.subscribe(100, StateCategory::Fsm);
        subs.subscribe(100, StateCategory::Vitals);
        assert!(!subs.is_empty());
        assert!(subs.has_subscribers(StateCategory::Fsm));
        assert!(subs.has_subscribers(StateCategory::Vitals));
        assert!(!subs.has_subscribers(StateCategory::Encounter));

        let fsm_subs: Vec<_> = subs.subscribers(StateCategory::Fsm).collect();
        assert_eq!(fsm_subs, vec![100]);
    }

    #[test]
    fn addon_subscriptions_multi_player() {
        let mut subs = AddonSubscriptions::default();
        subs.subscribe(100, StateCategory::Fsm);
        subs.subscribe(200, StateCategory::Fsm);
        subs.subscribe(200, StateCategory::Vitals);

        let fsm_subs: Vec<_> = subs.subscribers(StateCategory::Fsm).collect();
        assert_eq!(fsm_subs, vec![100, 200]);

        let vital_subs: Vec<_> = subs.subscribers(StateCategory::Vitals).collect();
        assert_eq!(vital_subs, vec![200]);
    }

    #[test]
    fn addon_subscriptions_unsubscribe() {
        let mut subs = AddonSubscriptions::default();
        subs.subscribe(100, StateCategory::Fsm);
        subs.subscribe(100, StateCategory::Vitals);
        subs.unsubscribe(100, StateCategory::Fsm);
        assert!(!subs.has_subscribers(StateCategory::Fsm));
        assert!(subs.has_subscribers(StateCategory::Vitals));

        subs.unsubscribe_all(100);
        assert!(subs.is_empty());
    }

    #[test]
    fn addon_subscriptions_idempotent() {
        let mut subs = AddonSubscriptions::default();
        subs.subscribe(100, StateCategory::Fsm);
        subs.subscribe(100, StateCategory::Fsm); // duplicate
        assert_eq!(subs.entries.len(), 1);
    }

    #[test]
    fn format_state_update_message() {
        let msg = format_state_update(StateCategory::Fsm, &[("state", "Combat"), ("sub", "n/a")]);
        assert_eq!(msg, "PB:v1:fsm:state=Combat:sub=n/a");
    }

    #[test]
    fn state_category_from_name() {
        assert_eq!(StateCategory::from_name("fsm"), Some(StateCategory::Fsm));
        assert_eq!(
            StateCategory::from_name("vitals"),
            Some(StateCategory::Vitals)
        );
        assert_eq!(StateCategory::from_name("hp"), Some(StateCategory::Vitals));
        assert_eq!(
            StateCategory::from_name("enc"),
            Some(StateCategory::Encounter)
        );
        assert_eq!(
            StateCategory::from_name("coord"),
            Some(StateCategory::Coordination)
        );
        assert_eq!(StateCategory::from_name("bt"), Some(StateCategory::BtPath));
        assert_eq!(StateCategory::from_name("unknown"), None);
    }
}
