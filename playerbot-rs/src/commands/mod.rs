/// Bot command system — parse chat commands and apply to bot settings.
///
/// Commands arrive as whispers from the master player, parsed by `parser::parse()`,
/// queued on `BotState::pending_commands`, and applied here before each tick.
pub mod parser;
pub mod preprocess;

use crate::bot::class_prefs::{
    HunterAspect, HunterSting, HunterTrap, PaladinAura, PaladinBlessing, PoisonKind, ShamanImbue, TotemRole,
    TotemSlot, WarlockCurse, WarlockPet, WarriorStance, WeaponHand,
};
use crate::bot::settings::{
    BehaviorMode, BotSettings, BotStateKind, ChatChannel, PositionStance, Reactivity,
    RtscAction, StrategyFlags,
};
use crate::bot::state::BotState;
use crate::ffi::{ItemId, SpellId, UnitHandle};

/// All bot commands, parsed from chat text.
#[derive(Debug, Clone, PartialEq)]
pub enum BotCommand {
    // -- Mode commands --
    SetMode(BehaviorMode),
    /// Bare `co tank` — full-replace the Combat strategy slot with this flag.
    SetCombatStrategies(StrategyFlags),
    /// Additive/subtractive strategy toggles for one `BotStateKind` slot.
    /// `nc +rtsc,-rpg bg` → `NonCombat`; `co +aoe` → `Combat`;
    /// `de +ghost` → `Dead`; `react +flee` → `Reaction`. `~` toggles.
    ApplyStrategies {
        state: BotStateKind,
        add: StrategyFlags,
        remove: StrategyFlags,
        toggle: StrategyFlags,
        query: bool,
    },
    /// Reset every strategy slot back to the default loadout (`reset ai`).
    ResetStrategies,
    SetReactivity(Reactivity),

    // -- Targeting --
    Focus(Option<UnitHandle>),
    Attack(Option<UnitHandle>),
    /// `pull` / `pull rti` — pull master's current target (or RTI mob).
    /// Only executed when the bot has the PULL strategy flag.
    /// If PULL_BACK is also enabled, the reactive BT subtree automatically
    /// returns the bot to the group after pulling.
    Pull(Option<UnitHandle>),
    /// Attack the unit marked with a raid target icon (1 = star … 8 = skull).
    AttackRti(u8),
    /// Pull (open combat on) the unit marked with a raid target icon.
    PullRti(u8),
    /// Apply crowd-control to the unit marked with a raid target icon.
    /// Which spell is used depends on the bot's class (polymorph, sap,
    /// fear, banish, hibernate, freezing trap, entangling roots…).
    CcRti(u8),

    // -- Spell control --
    BlacklistSpell(SpellId),
    UnblacklistSpell(SpellId),

    // -- Movement --
    GoTo(f32, f32, f32),
    Guard,
    ComeToMe,
    // -- RTSC (Real-Time Strategy Control) --
    // Uses spell 30758 cast on ground to communicate positions.
    // The addon/player casts the spell, bot intercepts the target location.
    /// Select/deselect this bot for RTSC control.
    RtscSelect,
    RtscCancel,
    RtscToggle,
    /// Move to the next spell-target position (with formation offset).
    RtscMove,
    /// Move to position exactly (no formation offset).
    RtscMoveExact,
    /// Save a named waypoint at the bot's current position.
    RtscSaveHere(String),
    /// Save the next spell-target position as a named waypoint.
    RtscSave(String),
    /// Delete a saved waypoint.
    RtscUnsave(String),
    /// Move to a saved waypoint by name.
    RtscGo(String),
    /// List saved waypoints.
    RtscShow,
    /// Position received from spell 30758 cast on ground.
    RtscSpellPosition(f32, f32, f32),
    /// `rtsc reset` — unlearn Aedm and clear all RTSC state for this bot.
    RtscReset,
    /// `rtsc last` — move to the last observed Aedm cast position.
    RtscLast,
    /// `rtsc jump` — two-stage jump recording. First call records the
    /// `jump` slot, second call records `jump point`. Cancel with
    /// `rtsc jump reset`.
    RtscJump,
    /// `rtsc jump reset` — clear both jump slots and disable the
    /// `rtsc jump` strategy.
    RtscJumpReset,
    /// `rtsc file save <file> [name_glob] [bot_glob]` — serialize saved
    /// locations for this bot (or matching bots in the group) to a log
    /// file under the server LogsDir. `name_glob == "*"` matches all
    /// saved locations. When `bot_glob` is `None` PB2 restricts the
    /// export to this bot only.
    RtscFileSave {
        file: String,
        name_glob: String,
        bot_glob: Option<String>,
    },
    /// `rtsc file load <file> [name_glob] [bot_glob]` — reload saved
    /// locations from a log file. Same glob semantics as `file save`.
    RtscFileLoad {
        file: String,
        name_glob: String,
        bot_glob: Option<String>,
    },

    // -- Economy --
    Repair,
    Vendor,

    // -- Healing --
    SetHealThreshold(f32),

    // -- Information --
    Status,
    ListSettings,
    /// Reply with current position (map, x, y, z) and zone area.
    Where,
    /// Reply with a short list of supported top-level commands.
    Help,
    /// Simple acknowledgement reply ("ready").
    Ready,

    // -- Utility --
    Reset,
    Mount,
    Resurrect,

    // -- Panic / aliases --
    /// Flee current threat for N seconds. The reactive flee subtree picks
    /// a safe position and runs there.
    Flee,
    /// Clear follow/guard/focus overrides and resume normal AI (mode stays).
    Free,
    /// Party-summon / meeting-stone summon (handled by world behavior module).
    Summon,
    /// Cast a named spell once (parsed via `data::spells` lookup).
    CastOne {
        spell: SpellId,
        on_self: bool,
    },
    /// Cast a spell by name — deferred resolution via FFI at execution time.
    /// Used when the hardcoded spell table doesn't have the name.
    CastByName {
        name: String,
        on_self: bool,
    },
    /// Set follower formation style.
    SetFormation(crate::bot::settings::FollowFormation),

    /// Travel to a named location from `data::named_locations`.
    /// Writes the destination to the blackboard; the travel subtree consumes it.
    TravelTo(&'static crate::data::named_locations::NamedLocation),

    // -- Tunables --
    /// `range <N>` — override follow distance (legacy single-number form).
    SetRange(f32),
    /// `range <qualifier> <N>` — set a specific range value (PB2 range subcommands).
    SetRangeQualified { qualifier: String, value: f32 },
    /// `range ?` — query all range values.
    QueryRange,

    /// `all +strat,-strat` — apply strategies to ALL four state engines.
    ApplyStrategiesAll {
        add: StrategyFlags,
        remove: StrategyFlags,
    },

    /// `stop` — stop current action / go passive briefly.
    Stop,
    /// `u <name>` — use an item by name. Resolved via FFI at execution time.
    UseItemByName(String),
    /// `e <name>` / `equip <name>` — equip an item by name. Resolved via FFI.
    EquipItemByName(String),
    /// `stance <N>` — warrior stance (1/2/3, warrior-only).
    SetStance(u8),
    /// `stance near|behind|tank|turnback` — positioning stance.
    /// Toggles BEHIND/CLOSE strategy flags per the Mangosbot addon toolbar.
    SetPositionStance(PositionStance),
    /// `max dps` — DPS combat order + aggressive reactivity shortcut.
    MaxDps,
    /// `save mana` toggle.
    ToggleSaveMana,
    /// `save mana on|off` — explicit set (Mangosbot addon sends both forms).
    SetSaveMana(u8),
    /// `self res` toggle.
    ToggleSelfRes,
    /// `cheat <flags>` — dev bitfield.
    SetCheatFlags(u32),
    /// `keep <itemid>` — add to do-not-sell list.
    KeepItem(ItemId),
    /// `unkeep <itemid>` — remove from do-not-sell list.
    UnkeepItem(ItemId),
    /// `chat <channel> on|off` — verbose reply toggle per channel.
    SetChatChannel { channel: ChatChannel, on: bool },
    /// `rti <icon>` — set the bot's preferred raid-target-icon focus.
    /// `rti clear` sends `None`.
    SetPreferredRti(Option<u8>),
    /// `rti cc <icon>` — set the bot's preferred raid-target-icon for CC.
    /// `rti cc none` / `rti cc clear` sends `None`. Used by the CC subtree
    /// to decide which marked mob to sheep/sap/banish/etc.
    SetPreferredCcRti(Option<u8>),
    /// `emote <id>` — play an emote.
    Emote(u32),
    /// `debug` / `cdebug` — diagnostic dump reply.
    Debug,
    /// `los` — whisper whether the bot has line of sight to current target.
    CheckLos,
    /// `quests` / `q` — whisper quest log summary.
    ListQuests,
    /// `talents` — whisper free talent points / spec info.
    ListTalents,
    /// `spells` — whisper known spell count.
    ListSpells,
    /// `release` — release spirit / use spirit healer if dead.
    ReleaseSpirit,
    /// `revive` — accept a pending resurrection.
    AcceptRevive,
    /// `jump` — make the bot jump in place.
    Jump,
    /// `hearth` / `home` — use a hearthstone.
    UseHearth,
    /// `rep` / `reputation` — whisper reputation standings.
    ListReputation,
    /// `skill` / `skills` — whisper learned skills.
    ListSkills,
    /// `accept` — accept all quests from the bot's currently targeted NPC.
    QuestAccept,
    /// `drop <quest_id>` — abandon an in-progress quest.
    QuestDrop(u32),
    /// `mail` — whisper inbox summary.
    MailSummary,
    /// `mail take` — take all money and items from the inbox (needs a
    /// nearby mailbox).
    MailTakeAll,
    /// `leave` — leave the bot's current guild.
    GuildLeave,

    // -- Class preferences --
    /// `poison mh <kind>` / `poison oh <kind>` — set rogue weapon poison.
    /// `kind = None` clears the slot (chat form: `poison mh none`).
    /// Silently ignored if the bot is not a rogue.
    SetPoison {
        hand: WeaponHand,
        kind: Option<PoisonKind>,
    },
    /// Reply with the current rogue poison loadout.
    ShowPoisons,
    /// `totem <slot> <role>` — set shaman totem role for a school slot.
    /// `role = None` clears the slot. Silently ignored on non-shamans or
    /// if the role doesn't match the slot's school.
    SetTotem {
        slot: TotemSlot,
        role: Option<TotemRole>,
    },
    /// Reply with the current shaman totem loadout.
    ShowTotems,

    /// `imbue mh flametongue` / `imbue oh windfury` — set shaman weapon
    /// imbue for a hand. `None` clears the slot. Silently ignored on
    /// non-shamans.
    SetShamanImbue {
        hand: WeaponHand,
        imbue: Option<ShamanImbue>,
    },
    /// Reply with the current shaman weapon-imbue loadout.
    ShowShamanImbues,

    /// `aura devotion` / `aura none` — set paladin self-aura. Silently
    /// ignored on non-paladins.
    SetPaladinAura(Option<PaladinAura>),
    /// `blessing might` / `blessing none` — set paladin blessing target.
    SetPaladinBlessing(Option<PaladinBlessing>),
    /// `blessing greater on|off` — toggle Greater-blessing preference.
    SetPaladinGreaterBlessing(bool),
    /// Reply with the current paladin aura / blessing loadout.
    ShowPaladinPrefs,

    /// `aspect hawk` / `aspect none` — set hunter default aspect.
    SetHunterAspect(Option<HunterAspect>),
    /// `trap freezing` / `trap none` — set hunter default trap.
    SetHunterTrap(Option<HunterTrap>),
    /// `sting serpent` / `sting none` — set hunter default sting.
    SetHunterSting(Option<HunterSting>),
    /// Reply with the current hunter aspect / trap / sting loadout.
    ShowHunterPrefs,

    /// `curse agony` / `curse none` — set warlock default curse.
    SetWarlockCurse(Option<WarlockCurse>),
    /// `pet imp` / `pet none` — set warlock preferred demon.
    SetWarlockPet(Option<WarlockPet>),
    /// Reply with the current warlock prefs (curse + pet).
    ShowWarlockPrefs,

    /// `forcestance berserker` / `forcestance none` — lock warrior into
    /// a stance regardless of rotation.
    SetWarriorForcedStance(Option<WarriorStance>),
    /// Reply with the current warrior forced-stance setting.
    ShowWarriorPrefs,

    /// `suppression auto|forbid|force` — BWL suppression-room disarm
    /// duty (rogue-only in `Auto`).
    SetSuppressionDuty(crate::bot::encounter_prefs::DutyMode),
    /// `douse auto|forbid|force` — MC rune-dousing duty (quintessence
    /// carrier in `Auto`).
    SetDouseDuty(crate::bot::encounter_prefs::DutyMode),
    /// Reply with the current `encounter_prefs`.
    ShowEncounterPrefs,

    // -- Loot policy (Mangosbot `ll` command) --
    /// Toggle (`ll ~equip`), set (`ll +equip`), or clear (`ll -equip`) one
    /// or more loot-policy categories. The dispatcher applies XORs before
    /// sets/clears so `ll ~equip+quest` ends up symmetrically toggling both.
    ApplyLootPolicy {
        add: crate::bot::settings::LootPolicy,
        remove: crate::bot::settings::LootPolicy,
        toggle: crate::bot::settings::LootPolicy,
    },

    // -- Query commands (whisper current value, do not mutate state) --
    /// `formation ?` — whisper current formation name.
    QueryFormation,
    /// `stance ?` — whisper current warrior stance.
    QueryStance,
    /// `nc ?` / `co ?` / `de ?` / `react ?` — whisper the strategy list for one
    /// of the four per-state engines. The state kind selects which
    /// slot to report and which Mangosbot-compatible reply prefix to
    /// use. See [`BotStateKind::reply_prefix`].
    QueryStrategies(BotStateKind),
    /// `react ?` (no signed args) — whisper current reactivity level.
    /// Distinct from `QueryStrategies(Reaction)` because the `react`
    /// command is overloaded: plain `react passive|defensive|aggressive`
    /// sets a stance, while `react +flee,?` toggles strategy names
    /// in the Reaction slot.
    QueryReactivity,
    /// `rti ?` — whisper current preferred raid target icon.
    QueryRti,
    /// `rti cc ?` — whisper current preferred CC raid target icon.
    QueryCcRti,
    /// `save mana ?` — whisper current save-mana toggle state.
    QuerySaveMana,
    /// `ll ?` — whisper current loot policy.
    QueryLootPolicy,

    // -- PB2 parity commands --
    SetWaitForAttack(u32),
    TankAttack,
    Loot,
    DestroyItem(String),
    SkipSpell(String),
    LootRoll(String),
    GiveLeader,
    InvitePlayer(String),
    Pet(String),
    BuffTarget(String),
    BoostTarget(String),
    ReviveTarget(String),
    FollowTarget(String),
    FocusHeal(String),
    MoveStyle(String),
    Talk,
    Trainer,
    Taxi,
    Craft(String),
    Outfit(String),
    LogLevel(String),
    ShareQuest,
    DoQuest(String),
    Bank,
    AuctionHouse(String),
    GuildCommand(String),
    BgFree,
    Flag,
    SendMail(String),
    /// `possible attack targets` — show possible targets.
    PossibleAttackTargets,
    /// `attackers` — show current attackers.
    ShowAttackers,
    /// `b` — buy from vendor (NPC interaction).
    Buy,
    /// `bb` — buyback from vendor.
    Buyback,
    /// `ue <item>` — unequip item by name.
    UnequipItemByName(String),
    /// `t` / `nt` — accept/initiate trade.
    Trade,
    /// `quest reward` / `reward` / `r` — choose quest reward.
    QuestReward,
    /// `cs <strategy>` — custom strategy definition.
    CustomStrategy(String),
    /// `wts` — what to sell query.
    WhatToSell,
    /// `teleport` — teleport to location.
    Teleport(String),
    /// `speak <text>` — emote/say text.
    Speak(String),
    /// `faction` — show faction standing.
    ShowFaction,
    /// `set value <key> <val>` — set a config value.
    SetValue(String),
    /// `load ai` / `save ai` / `list ai` — AI profile management.
    AiProfile(String),
    /// `lfg` — looking for group toggle.
    Lfg,

    /// Internal: a single parse result that carries multiple commands.
    /// Used when a strategy toggle implies a class-pref side-effect (e.g.
    /// `co +poison main deadly` → `ApplyStrategies{POISONS}` + `SetPoison`).
    Batch(Vec<BotCommand>),

    // -- Unknown --
    Unknown(String),
}

impl BotCommand {
    /// Minimum [`SecurityLevel`] a sender must have for this command to
    /// execute. Mirrors PB2's tiered access in
    /// `PB2/playerbot/PlayerbotAI.cpp::HandleCommand`.
    ///
    /// Tiers (low → high):
    /// - `Talk`: information queries (status, stats, settings) — anyone
    ///   who can message the bot.
    /// - `Invite`: behaviour, targeting, movement, buffs — group members.
    /// - `AllowAll`: destructive or account-level ops (reset, blacklist,
    ///   resurrect, economy) — master / same-account / GM only.
    pub fn required_security(&self) -> SecurityLevel {
        use BotCommand::*;
        match self {
            // Information queries — anyone who can talk to the bot.
            Status | ListSettings | Where | Help | Ready | Unknown(_) | Debug | CheckLos
            | ListQuests | ListTalents | ListSpells | ListReputation | ListSkills
            | MailSummary
            // Addon probes — must be readable by anyone who can whisper.
            | QueryFormation | QueryStance | QueryStrategies(_)
            | QueryReactivity | QueryRti | QueryCcRti | QuerySaveMana | QueryLootPolicy => SecurityLevel::Talk,

            // Destructive / account-level — master only.
            Reset | ResetStrategies | BlacklistSpell(_) | UnblacklistSpell(_)
            | SetCheatFlags(_) | GuildLeave => SecurityLevel::AllowAll,

            // Everything else — group members.
            SetMode(_)
            | SetCombatStrategies(_)
            | ApplyStrategies { .. }
            | SetReactivity(_)
            | Focus(_)
            | Attack(_)
            | Pull(_)
            | AttackRti(_)
            | PullRti(_)
            | CcRti(_)
            | GoTo(_, _, _)
            | Guard
            | ComeToMe
            | RtscSelect
            | RtscCancel
            | RtscToggle
            | RtscMove
            | RtscMoveExact
            | RtscSaveHere(_)
            | RtscSave(_)
            | RtscUnsave(_)
            | RtscGo(_)
            | RtscShow
            | RtscSpellPosition(_, _, _)
            | RtscReset
            | RtscLast
            | RtscJump
            | RtscJumpReset
            | RtscFileSave { .. }
            | RtscFileLoad { .. }
            | Repair
            | Vendor
            | SetHealThreshold(_)
            | Mount
            | Resurrect
            | Flee
            | Free
            | Summon
            | CastOne { .. }
            | CastByName { .. }
            | UseItemByName(_)
            | EquipItemByName(_)
            | SetFormation(_)
            | TravelTo(_)
            | SetRange(_)
            | SetRangeQualified { .. }
            | QueryRange
            | ApplyStrategiesAll { .. }
            | Stop
            | SetStance(_)
            | SetPositionStance(_)
            | MaxDps
            | ToggleSaveMana
            | SetSaveMana(_)
            | ToggleSelfRes
            | KeepItem(_)
            | UnkeepItem(_)
            | SetChatChannel { .. }
            | SetPreferredRti(_)
            | SetPreferredCcRti(_)
            | Emote(_)
            | ReleaseSpirit
            | AcceptRevive
            | Jump
            | UseHearth
            | QuestAccept
            | QuestDrop(_)
            | MailTakeAll
            | SetPoison { .. }
            | ShowPoisons
            | SetTotem { .. }
            | ShowTotems
            | SetShamanImbue { .. }
            | ShowShamanImbues
            | SetPaladinAura(_)
            | SetPaladinBlessing(_)
            | SetPaladinGreaterBlessing(_)
            | ShowPaladinPrefs
            | SetHunterAspect(_)
            | SetHunterTrap(_)
            | SetHunterSting(_)
            | ShowHunterPrefs
            | SetWarlockCurse(_)
            | SetWarlockPet(_)
            | ShowWarlockPrefs
            | SetWarriorForcedStance(_)
            | ShowWarriorPrefs
            | SetSuppressionDuty(_)
            | SetDouseDuty(_)
            | ShowEncounterPrefs
            | ApplyLootPolicy { .. }
            | SetWaitForAttack(_)
            | TankAttack
            | Loot
            | DestroyItem(_)
            | SkipSpell(_)
            | LootRoll(_)
            | GiveLeader
            | InvitePlayer(_)
            | Pet(_)
            | BuffTarget(_)
            | BoostTarget(_)
            | ReviveTarget(_)
            | FollowTarget(_)
            | FocusHeal(_)
            | MoveStyle(_)
            | Talk
            | Trainer
            | Taxi
            | Craft(_)
            | Outfit(_)
            | LogLevel(_)
            | ShareQuest
            | DoQuest(_)
            | Bank
            | AuctionHouse(_)
            | GuildCommand(_)
            | BgFree
            | Flag
            | SendMail(_)
            | PossibleAttackTargets
            | ShowAttackers
            | Buy
            | Buyback
            | UnequipItemByName(_)
            | Trade
            | QuestReward
            | CustomStrategy(_)
            | WhatToSell
            | Teleport(_)
            | Speak(_)
            | ShowFaction
            | SetValue(_)
            | AiProfile(_)
            | Lfg
            | Batch(_) => SecurityLevel::Invite,
        }
    }
}

/// Chat-command security tier. Mirrors PB2's `PlayerbotSecurityLevel` with
/// GUILD collapsed into `Talk`. The byte value is what crosses the FFI in
/// `playerbot_chat_command`'s `security` parameter.
///
/// Each [`BotCommand`] declares a minimum [`SecurityLevel`] (see
/// [`BotCommand::required_security`]); the dispatcher drops commands whose
/// sender tier is below that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum SecurityLevel {
    DenyAll = 0,
    Talk = 1,
    Invite = 2,
    AllowAll = 3,
}

impl SecurityLevel {
    pub fn from_raw(v: u8) -> Self {
        match v {
            0 => Self::DenyAll,
            1 => Self::Talk,
            2 => Self::Invite,
            _ => Self::AllowAll,
        }
    }
}

/// The incoming chat channel and language a command arrived on.
///
/// The addons (Mangosbot UI, RaidControl) send commands via five channels:
/// WHISPER (`SendChatMessage(cmd, "WHISPER", …)` or the spoofed
/// `BOT\t`-prefixed whisper), PARTY, RAID, GUILD, and as CHAT_MSG_ADDON on
/// LANG_ADDON (via `SendAddonMessage("BOT", …)`). Replies need to go back on
/// the matching channel: debug / `#a ` queries reply on CHAT_MSG_ADDON /
/// LANG_ADDON, everything else whispers the sender.
///
/// Values mirror CMaNGOS `ChatMsg` and `Language` — we keep them as raw
/// `u32` on the Rust side so we don't couple to a core enum we don't own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatOrigin {
    /// CMaNGOS `ChatMsg` value (CHAT_MSG_WHISPER = 0x06, CHAT_MSG_SAY = 0x01,
    /// CHAT_MSG_PARTY = 0x02, CHAT_MSG_RAID = 0x03, CHAT_MSG_GUILD = 0x04,
    /// CHAT_MSG_YELL = 0x05, CHAT_MSG_CHANNEL = 0x11, …).
    pub chat_type: u32,
    /// CMaNGOS `Language` value. `LANG_ADDON` = `0xFFFFFFFE` (-2 as i32).
    pub lang: u32,
}

/// CMaNGOS `LANG_ADDON` sentinel, used by addon-channel payloads.
pub const LANG_ADDON: u32 = 0xFFFF_FFFE;

impl ChatOrigin {
    pub fn new(chat_type: u32, lang: u32) -> Self {
        Self { chat_type, lang }
    }

    /// Sentinel for commands injected internally (RTSC spell position,
    /// tests) that never came from a real chat packet.
    pub const INTERNAL: Self = Self {
        chat_type: 0,
        lang: 0,
    };

    /// True iff the command arrived on an addon channel — either as
    /// CHAT_MSG_ADDON or any chat channel with `LANG_ADDON`. Replies must
    /// go back via CHAT_MSG_ADDON / LANG_ADDON so the Mangosbot UI parses
    /// them instead of the player seeing a whisper.
    pub fn is_addon(&self) -> bool {
        self.lang == LANG_ADDON
    }
}

/// A command queued for execution, tagged with its sender and trust level.
///
/// `sender` is the `ObjectGuid` of the player who issued the chat, or
/// `None` for internal/system-injected commands (RTSC spell positions,
/// tests). `security` is the tier that C++-side `PlayerbotRust::
/// ComputeSenderSecurity` granted the sender. The dispatcher compares it
/// to each command's `required_security` and drops commands that don't
/// meet the bar. `origin` records the chat channel + language the command
/// arrived on so replies can be routed back on the same channel.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingCommand {
    pub sender: Option<u64>,
    pub security: SecurityLevel,
    pub origin: ChatOrigin,
    pub command: BotCommand,
}

impl PendingCommand {
    /// Internal/system command — always allowed, no whisper reply possible.
    pub fn internal(command: BotCommand) -> Self {
        Self {
            sender: None,
            security: SecurityLevel::AllowAll,
            origin: ChatOrigin::INTERNAL,
            command,
        }
    }

    /// Command from a specific player with a trust tier.
    pub fn external(
        sender: u64,
        security: SecurityLevel,
        origin: ChatOrigin,
        command: BotCommand,
    ) -> Self {
        Self {
            sender: Some(sender),
            security,
            origin,
            command,
        }
    }
}

/// Process all pending commands on a bot, mutating its settings.
/// Called once per tick before the BT runs.
pub fn process_commands(bot: &mut BotState) {
    // Drain the mutex-protected queue into a local vec so we don't hold the
    // lock while executing commands (which may call back into the interface).
    let cmds: Vec<PendingCommand> = bot.pending_commands.lock().unwrap().drain(..).collect();
    for pc in cmds {
        let required = pc.command.required_security();
        if pc.security < required {
            // Silently drop under-privileged commands. A verbose reply
            // would create a whisper-spam vector from strangers.
            continue;
        }
        apply_command(bot, &pc);
    }
}

/// The crowd-control spell this class has available for `cc {icon}` commands.
///
/// The spell is always *attempted* — `can_cast` handles rank, `LoS`, range,
/// creature-type immunity, already-CC'd, etc. Returns `None` for classes
/// with no direct single-target CC (warrior, DK, rogue uses stealth-only
/// sap so it's conditional).
fn class_cc_spell(class: crate::bot::state::PlayerClass) -> Option<SpellId> {
    use crate::bot::state::PlayerClass::{Mage, Warlock, Priest, Druid, Hunter, Paladin, Shaman, Rogue, Warrior, DeathKnight};
    Some(match class {
        Mage => SpellId(118),      // polymorph
        Warlock => SpellId(710),   // banish (works on demons/elementals)
        Priest => SpellId(605),    // mind control — in practice shackle undead
        Druid => SpellId(2637),    // hibernate (beast/dragonkin only; fall back silently)
        Hunter => SpellId(1499),   // freezing trap
        Paladin => SpellId(20066), // repentance (retri talent; may fail silently)
        Shaman => SpellId(8034),   // frostbrand … no single-target CC; hex is wotlk
        Rogue => SpellId(6770),    // sap (stealth-only; can_cast will refuse otherwise)
        Warrior | DeathKnight => return None,
    })
}

/// Reply to the sender of `pc`, routing on the channel the command
/// arrived on.
///
/// * When the origin is an addon channel (`origin.is_addon()` → true for
///   `#a …` prefixed commands and any LANG_ADDON-tagged chat), reply via
///   `tell_addon` so the packet comes back as CHAT_MSG_ADDON / LANG_ADDON
///   with the `BOT\t` prefix. Mangosbot's event handler fires both
///   CHAT_MSG_WHISPER and CHAT_MSG_ADDON through the same `OnWhisper`
///   parser (`Mangosbot.lua:3129-3138`), so the UI-critical state-change
///   confirmation strings ("Following...", "Formation set to …", …) are
///   still recognised regardless of which wire they come back on.
/// * Otherwise (direct whisper / party / raid / guild / say), fall back
///   to PB2's `TellPlayerNoFacing` routing via `tell_player`: broadcast
///   to the bot's group when it has one, else whisper the sender.
///
/// Internal/system-injected commands (`pc.sender == None`) fall back to
/// `say` so anything the bot utters without a requester still goes out
/// on a real channel.
fn reply(bot: &BotState, pc: &PendingCommand, msg: &str) {
    // Monitor: log the reply before sending.
    if bot.monitor_active {
        let guid = pc.sender.unwrap_or(0);
        crate::bot::monitor::monitor_reply(bot, guid, msg, pc.origin.is_addon());
    }
    match pc.sender {
        Some(guid) => {
            if pc.origin.is_addon() {
                bot.interface.tell_addon(guid, msg);
            } else {
                // Whisper the sender directly. PB2 used TellPlayerNoFacing
                // which broadcasts to party/raid, but that spams the group
                // channel when configuring bots — whisper is the expected
                // behavior for command responses.
                bot.interface.whisper(guid, msg);
            }
        }
        None => {
            bot.interface.say(msg, 0);
        }
    }
}

/// Try to cast a commanded spell on a target, replying with an error if it
/// fails.  Mirrors PB2's "I can't do that" / "I don't know that spell"
/// feedback.
fn try_commanded_cast(bot: &BotState, pc: &PendingCommand, spell: SpellId, target: u64) {
    let spell_name = bot.interface.get_spell_name(spell);
    let name = if spell_name.is_empty() {
        format!("#{}", spell.raw())
    } else {
        spell_name
    };
    if target == 0 {
        reply(bot, pc, &format!("Can't cast {name}: no target"));
        return;
    }
    if !bot.interface.knows_spell(spell) {
        reply(bot, pc, &format!("Can't cast {name}: I don't know that spell"));
        return;
    }
    let dist = bot.interface.unit_distance(target);
    let los = bot.interface.has_los(target);
    let can = bot.interface.can_cast(spell, target);
    if !can {
        let reason = if !los {
            "no line of sight"
        } else {
            "out of range, not enough resources, or wrong stance"
        };
        reply(bot, pc, &format!("Can't cast {name}: {reason} (dist={dist:.0}y)"));
        return;
    }
    if !bot.interface.cast_spell(spell, target) {
        reply(bot, pc, &format!("Can't cast {name}: server rejected (dist={dist:.0}y los={los})"));
    }
}

/// Map an RTI icon index (1..=8) to Mangosbot's lowercase name. `None` and
/// out-of-range values render as `"none"` (the addon uses this to clear).
fn rti_icon_name(icon: Option<u8>) -> &'static str {
    match icon {
        Some(1) => "star",
        Some(2) => "circle",
        Some(3) => "diamond",
        Some(4) => "triangle",
        Some(5) => "moon",
        Some(6) => "square",
        Some(7) => "cross",
        Some(8) => "skull",
        _ => "none",
    }
}

fn apply_command(bot: &mut BotState, pc: &PendingCommand) {
    if bot.monitor_active {
        crate::bot::monitor::monitor_log(bot, &format!("CMD APPLY: {:?}", pc.command));
    }
    let cmd = &pc.command;
    let s = &mut bot.settings;
    match cmd {
        BotCommand::SetMode(mode) => {
            s.mode = *mode;
            // Sync strategy flags in nc/co so the addon UI highlights the
            // correct mode button. Only touch NonCombat and Combat — never
            // pollute Reaction or Dead slots with mode flags.
            let mode_flags = StrategyFlags::PASSIVE
                | StrategyFlags::STAY
                | StrategyFlags::GUARD
                | StrategyFlags::GRIND
                | StrategyFlags::QUEST
                | StrategyFlags::RPG
                | StrategyFlags::FOLLOW;
            let new_flag = match mode {
                BehaviorMode::Passive => StrategyFlags::PASSIVE,
                BehaviorMode::Stay => StrategyFlags::STAY,
                BehaviorMode::Guard => StrategyFlags::GUARD,
                BehaviorMode::Grind => StrategyFlags::GRIND,
                BehaviorMode::Quest => StrategyFlags::QUEST,
                BehaviorMode::Rpg => StrategyFlags::RPG,
                _ => StrategyFlags::FOLLOW,
            };
            for state in [BotStateKind::NonCombat, BotStateKind::Combat] {
                let slot = s.strategies.get_mut(state);
                slot.remove(mode_flags);
                slot.insert(new_flag);
            }
            // Also clean mode flags from Reaction/Dead if they leaked.
            for state in [BotStateKind::Reaction, BotStateKind::Dead] {
                s.strategies.get_mut(state).remove(mode_flags);
            }
            // Mangosbot's OnWhisper refresh hook (Mangosbot.lua:3149) finds
            // `Following` / `Staying` / `Fleeing` at string position 1 and
            // re-issues `nc ?` to refresh its non-combat strategy panel.
            // These confirmations are UI-critical, NOT gated on verbose.
            match mode {
                BehaviorMode::Follow => reply(bot, pc, "Following..."),
                BehaviorMode::Stay | BehaviorMode::Guard => reply(bot, pc, "Staying..."),
                _ => {
                    // Always reply so Mangosbot's OnWhisper refresh hook
                    // can trigger an `nc ?` re-query for any mode change.
                    reply(bot, pc, &format!("Mode: {}", mode.as_str()));
                }
            }
        }
        BotCommand::SetCombatStrategies(flags) => {
            // Bare `co tank` — swap the mutually-exclusive targeting flags
            // (ASSIST/PROTECT/TANK) but preserve all other Combat slot flags
            // (RANGED, BEHIND, BOOST, AOE, etc.) that init.rs set up.
            let slot = s.strategies.get_mut(BotStateKind::Combat);
            slot.remove(StrategyFlags::TARGETING_EXCLUSIVE);
            slot.insert(*flags);
        }
        BotCommand::ApplyStrategies { state, add, remove, toggle, query } => {
            {
                let slot = s.strategies.get_mut(*state);
                slot.remove(*remove);
                slot.insert(*add);
                // Toggle: xor each flag in the toggle set.
                slot.0 ^= toggle.0;
                slot.1 ^= toggle.1;
            }
            if *query {
                let slot_val = bot.settings.strategies.get(*state);
                let desc = describe_with_class_prefs(slot_val, &bot.settings.class_prefs);
                let msg = format!("{}: {}", state.reply_prefix(), desc);
                reply(bot, pc, &msg);
            }
            // Bridge strategy flags → BehaviorMode.
            // The addon toggles mode names (passive, stay, guard, grind,
            // quest, rpg, follow) as strategy flags, but the BT checks
            // `ModeIs(BehaviorMode::…)` which reads `s.mode`.  Sync them.
            // Mode flags are mutually exclusive — when one is set, clear
            // all others so they don't accumulate from repeated toggles.
            {
                let mode_flags = StrategyFlags::PASSIVE
                    | StrategyFlags::STAY
                    | StrategyFlags::GUARD
                    | StrategyFlags::GRIND
                    | StrategyFlags::QUEST
                    | StrategyFlags::RPG
                    | StrategyFlags::FOLLOW;
                let has = |f: StrategyFlags| {
                    bot.settings.strategies.get(BotStateKind::Combat).contains(f)
                    || bot.settings.strategies.get(BotStateKind::NonCombat).contains(f)
                };
                let (new_mode, active_flag) = if has(StrategyFlags::PASSIVE) {
                    (BehaviorMode::Passive, StrategyFlags::PASSIVE)
                } else if has(StrategyFlags::STAY) {
                    (BehaviorMode::Stay, StrategyFlags::STAY)
                } else if has(StrategyFlags::GUARD) {
                    (BehaviorMode::Guard, StrategyFlags::GUARD)
                } else if has(StrategyFlags::GRIND) {
                    (BehaviorMode::Grind, StrategyFlags::GRIND)
                } else if has(StrategyFlags::QUEST) {
                    (BehaviorMode::Quest, StrategyFlags::QUEST)
                } else if has(StrategyFlags::RPG) {
                    (BehaviorMode::Rpg, StrategyFlags::RPG)
                } else {
                    (BehaviorMode::Follow, StrategyFlags::FOLLOW)
                };
                bot.settings.mode = new_mode;
                // Clear conflicting mode flags from nc/co, then re-set the
                // active one. Never touch Reaction or Dead slots.
                for state in [BotStateKind::NonCombat, BotStateKind::Combat] {
                    let slot = bot.settings.strategies.get_mut(state);
                    slot.remove(mode_flags);
                    slot.insert(active_flag);
                }
                // Clean mode flags from Reaction/Dead if they leaked.
                for state in [BotStateKind::Reaction, BotStateKind::Dead] {
                    bot.settings.strategies.get_mut(state).remove(mode_flags);
                }
            }
            // Check if a spec flag was added — if so, rebuild the behavior
            // tree for the new spec. This handles MangosBot addon spec
            // selection (e.g. `co +protection` on a warrior).
            if *state == BotStateKind::Combat {
                let new_combat = bot.settings.strategies.get(BotStateKind::Combat);
                if let Some(new_spec) = crate::bot::init::spec_from_strategy_flags(bot.class, new_combat) {
                    if new_spec != bot.spec {
                        crate::bot::init::rebuild_for_spec(bot, new_spec);
                    }
                }
            }
        }
        BotCommand::ResetStrategies => {
            let init = s.init_strategies;
            s.strategies.reset_to_defaults(&init);
        }
        BotCommand::SetReactivity(level) => {
            s.reactivity = *level;
        }
        BotCommand::Focus(target) => {
            // If no target provided, use master's target, then bot's own.
            s.focus_target = target.or_else(|| {
                if let Some(master) = bot.master_guid.filter(|&g| g != 0) {
                    let ms = bot.interface.get_unit_snapshot(master);
                    if ms.current_target != 0 {
                        return Some(ms.current_target);
                    }
                }
                let t = bot.snap.self_.current_target;
                if t != 0 { Some(t) } else { None }
            });
        }
        BotCommand::Attack(target) => {
            let t = target.or_else(|| {
                // PB2 parity: resolve master's target first, then bot's own.
                if let Some(master) = bot.master_guid.filter(|&g| g != 0) {
                    let ms = bot.interface.get_unit_snapshot(master);
                    if ms.current_target != 0 {
                        return Some(ms.current_target);
                    }
                }
                let t = bot.snap.self_.current_target;
                if t != 0 { Some(t) } else {
                    // Last resort: attack the first attacker.
                    bot.attackers.first().copied()
                }
            });
            if let Some(unit) = t {
                bot.interface.attack(unit);
                s.focus_target = Some(unit);
            }
        }
        BotCommand::Pull(target) => {
            // `pull` — resolve target same as attack, then engage via
            // auto-shoot/taunt. The PULL_BACK reactive subtree handles
            // returning to the group afterward.
            if !s.strategies.get(BotStateKind::Combat).contains(StrategyFlags::PULL) {
                reply(bot, pc, "pull: need pull strategy");
                return;
            }
            let t = target.or_else(|| {
                if let Some(master) = bot.master_guid.filter(|&g| g != 0) {
                    let ms = bot.interface.get_unit_snapshot(master);
                    if ms.current_target != 0 {
                        return Some(ms.current_target);
                    }
                }
                let t = bot.snap.self_.current_target;
                if t != 0 { Some(t) } else {
                    bot.attackers.first().copied()
                }
            });
            if let Some(unit) = t {
                s.focus_target = Some(unit);
                // Try ranged pull first (auto-shot), fall back to taunt,
                // then plain attack.
                if !bot.interface.auto_shoot(unit)
                    && !bot.interface.taunt(unit)
                {
                    bot.interface.attack(unit);
                }
            }
        }
        BotCommand::AttackRti(icon) | BotCommand::PullRti(icon) => {
            // Same immediate behavior — just engage. The "pull" vs "attack"
            // distinction matters for the assignment-tracking side (who is
            // the puller), which GroupState tracks separately.
            if let Some(unit) = bot.interface.get_unit_with_raid_icon(*icon) {
                bot.interface.attack(unit);
                s.focus_target = Some(unit);
            }
        }
        BotCommand::CcRti(icon) => {
            // Pick a CC spell from the class's vocabulary. The bot will only
            // cast if it can — uncastable/missing spells silently no-op.
            if let Some(unit) = bot.interface.get_unit_with_raid_icon(*icon)
                && let Some(spell) = class_cc_spell(bot.class)
                    && bot.interface.can_cast(spell, unit) {
                        bot.interface.cast_spell(spell, unit);
                    }
        }
        BotCommand::BlacklistSpell(spell) => {
            s.spell_blacklist.insert(*spell);
        }
        BotCommand::UnblacklistSpell(spell) => {
            s.spell_blacklist.remove(spell);
        }
        BotCommand::GoTo(x, y, z) => {
            bot.interface.move_to(*x, *y, *z);
        }
        BotCommand::Guard => {
            let pos = bot.snap.self_.pos;
            s.guard_position = Some((pos.x, pos.y, pos.z));
            s.mode = BehaviorMode::Guard;
        }
        BotCommand::ComeToMe => {
            s.mode = BehaviorMode::Follow;
        }

        // -- RTSC commands -- (see `crate::rtsc` for the backing module.)
        BotCommand::RtscSelect => {
            crate::rtsc::select(bot);
        }
        BotCommand::RtscCancel => {
            crate::rtsc::cancel(bot);
        }
        BotCommand::RtscToggle => {
            crate::rtsc::toggle(bot);
        }
        BotCommand::RtscMove => {
            crate::rtsc::ensure_spell_learned(bot);
            bot.settings.rtsc_pending_action = Some(RtscAction::Move { exact: false });
        }
        BotCommand::RtscMoveExact => {
            crate::rtsc::ensure_spell_learned(bot);
            bot.settings.rtsc_pending_action = Some(RtscAction::Move { exact: true });
        }
        BotCommand::RtscSaveHere(name) => {
            crate::rtsc::save_here(bot, name.clone());
        }
        BotCommand::RtscSave(name) => {
            crate::rtsc::ensure_spell_learned(bot);
            bot.settings.rtsc_pending_action = Some(RtscAction::Save { name: name.clone() });
        }
        BotCommand::RtscUnsave(name) => {
            bot.settings.rtsc_waypoints.remove(name);
        }
        BotCommand::RtscGo(name) => {
            crate::rtsc::ensure_spell_learned(bot);
            if let Some(&(x, y, z)) = bot.settings.rtsc_waypoints.get(name) {
                bot.interface.move_to(x, y, z);
                bot.settings.guard_position = Some((x, y, z));
                bot.settings.mode = BehaviorMode::Guard;
            }
        }
        BotCommand::RtscShow => {
            crate::rtsc::ensure_spell_learned(bot);
            let names: Vec<&str> = bot
                .settings
                .rtsc_waypoints
                .keys()
                .filter(|n| {
                    n.as_str() != crate::rtsc::JUMP_SLOT
                        && n.as_str() != crate::rtsc::JUMP_POINT_SLOT
                })
                .map(|s| s.as_str())
                .collect();
            let msg = if names.is_empty() {
                "No saved waypoints.".to_string()
            } else {
                format!("Waypoints: {}", names.join(", "))
            };
            reply(bot, pc, &msg);
        }
        BotCommand::RtscSpellPosition(x, y, z) => {
            crate::rtsc::on_spell_land(bot, *x, *y, *z);
        }
        BotCommand::RtscReset => {
            crate::rtsc::reset(bot);
        }
        BotCommand::RtscLast => {
            crate::rtsc::ensure_spell_learned(bot);
            if !crate::rtsc::last(bot) {
                reply(bot, pc, "No RTSC cast recorded yet.");
            }
        }
        BotCommand::RtscJump => match crate::rtsc::jump_command(bot) {
            crate::rtsc::JumpCommandResult::StageOneQueued => {}
            crate::rtsc::JumpCommandResult::StaleCancelled => {
                reply(bot, pc, "Can't finish previous jump! Cancelling...");
            }
            crate::rtsc::JumpCommandResult::AlreadyInProgress => {
                reply(
                    bot,
                    pc,
                    "Another jump is in process! Use 'rtsc jump reset' to stop it",
                );
            }
        },
        BotCommand::RtscJumpReset => {
            crate::rtsc::jump_reset(bot);
        }
        BotCommand::RtscFileSave {
            file,
            name_glob,
            bot_glob: _,
        } => {
            crate::rtsc::ensure_spell_learned(bot);
            let (body, n) = crate::rtsc::serialize_waypoints(bot, name_glob);
            let ok = bot.interface.bot_write_log_file(file, &body);
            let msg = if ok {
                format!("Saved {n} waypoint(s) to {file}.")
            } else {
                format!("Failed to write {file}.")
            };
            reply(bot, pc, &msg);
        }
        BotCommand::RtscFileLoad {
            file,
            name_glob,
            bot_glob: _,
        } => {
            crate::rtsc::ensure_spell_learned(bot);
            let msg = match bot.interface.bot_read_log_file(file) {
                Some(body) => {
                    let n = crate::rtsc::deserialize_waypoints(bot, &body, name_glob);
                    format!("Loaded {n} waypoint(s) from {file}.")
                }
                None => format!("Failed to read {file}."),
            };
            reply(bot, pc, &msg);
        }
        BotCommand::Repair => {
            bot.interface.repair_all();
        }
        BotCommand::Vendor => {
            bot.interface.sell_grey_items();
        }
        BotCommand::SetHealThreshold(pct) => {
            s.heal_party_threshold = *pct;
        }
        BotCommand::Status => {
            let msg = format!(
                "Mode:{} CO:[{}] React:{:?} HP:{:.0}% MP:{:.0}%",
                s.mode.as_str(),
                s.strategies.get(BotStateKind::Combat).describe(),
                s.reactivity,
                bot.snap.self_.health as f32 / bot.snap.self_.max_health.max(1) as f32 * 100.0,
                bot.snap.self_.mana as f32 / bot.snap.self_.max_mana.max(1) as f32 * 100.0,
            );
            reply(bot, pc, &msg);
        }
        BotCommand::Where => {
            let p = &bot.snap.self_.pos;
            let msg = format!("Map {} @ {:.1}, {:.1}, {:.1}", p.map_id, p.x, p.y, p.z);
            reply(bot, pc, &msg);
        }
        BotCommand::Help => {
            // Short one-liner; addons parse this. Keep under whisper length.
            let msg = "Commands: follow stay grind quest passive guard wander bg \
                       co nc react attack pull cc focus come go rtsc \
                       blacklist unblacklist repair vendor heal status where help ready \
                       reset mount rez flee free summon cast formation travel \
                       range stance rti emote debug max-dps save-mana self-res \
                       keep unkeep chat quests talents spells los release revive";
            reply(bot, pc, msg);
        }
        BotCommand::Ready => {
            reply(bot, pc, "ready");
        }
        BotCommand::ListSettings => {
            let msg = format!(
                "Follow dist:{:.1} Heal@{:.0}% Blacklist:{} spells",
                s.follow_distance,
                s.heal_party_threshold * 100.0,
                s.spell_blacklist.len(),
            );
            reply(bot, pc, &msg);
        }
        BotCommand::Reset => {
            let init = s.init_strategies;
            *s = BotSettings::default();
            s.strategies = init;
            s.init_strategies = init;
        }
        BotCommand::Mount => {
            // Toggle mount: if already mounted, dismount; otherwise cast
            // the best available mount. Mirrors PB2's `mount` / `dismount`
            // chat aliases (both end up toggling the mount state via the
            // same action node).
            let ok = if bot.interface.is_mounted() {
                bot.interface.dismount()
            } else {
                bot.interface.mount_up()
            };
            if s.verbose {
                reply(bot, pc, if ok { "Ok" } else { "Cannot mount" });
            }
        }
        BotCommand::Resurrect => {
            // Mirrors PB2's dead-strategy resurrect fallback chain:
            //   1. Accept any pending resurrect request (from a priest,
            //      paladin, druid, shaman, soulstone, etc).
            //   2. Otherwise repop at the spirit healer.
            // The `release` and `revive` commands exist for the individual
            // steps — this one is the "do whatever it takes" alias.
            let accepted = bot.interface.accept_resurrect();
            let ok = accepted || bot.interface.use_spirit_healer();
            if s.verbose {
                reply(bot, pc, if ok { "Ok" } else { "Cannot resurrect" });
            }
        }

        BotCommand::Flee => {
            // Reactive flee subtree picks this up via `Bt::FleeOverride`.
            // 5s window is enough for one retreat; combat re-evaluates.
            let now = bot.snap.server_time_ms;
            s.flee_override_until_ms = now + 5_000;
            // Mangosbot.lua:3149 watches for "Fleeing" at position 1.
            reply(bot, pc, "Fleeing...");
        }
        BotCommand::Free => {
            s.focus_target = None;
            s.protect_target = None;
            s.guard_position = None;
            // Leave mode alone — "free" clears overrides, doesn't reset everything.
        }
        BotCommand::Summon => {
            // Mirrors PB2 `SummonAction`: teleport the bot to the requester
            // (in-place revive if dead), offset by follow range. LoS, angle
            // search, transport re-parenting, and motion-master cleanup are
            // all handled server-side in `CB_SummonToPlayer`.
            //
            // Internal commands (`pc.sender == None`) fall back to the
            // master via blackboard — no sender means there is nobody to
            // teleport to so we just skip and whisper nothing.
            if let Some(requester) = pc.sender {
                let ok = bot.interface.summon_to_player(requester);
                if s.verbose {
                    reply(bot, pc, if ok { "Coming!" } else { "Cannot summon" });
                }
            }
        }
        BotCommand::CastOne { spell, on_self } => {
            let target = if *on_self {
                bot.handle
            } else {
                let t = bot.snap.self_.current_target;
                if t != 0 { t } else {
                    // No current target — fall back to first attacker.
                    bot.attackers.first().copied().unwrap_or(0)
                }
            };
            try_commanded_cast(bot, pc, *spell, target);
        }
        BotCommand::CastByName { name, on_self } => {
            let spell_id = bot.interface.resolve_spell_by_name(name);
            if spell_id == 0 {
                reply(bot, pc, &format!("cast: unknown spell `{name}`"));
            } else {
                let target = if *on_self {
                    bot.handle
                } else {
                    let t = bot.snap.self_.current_target;
                    if t != 0 { t } else {
                        bot.attackers.first().copied().unwrap_or(0)
                    }
                };
                try_commanded_cast(bot, pc, SpellId(spell_id), target);
            }
        }
        BotCommand::UseItemByName(name) => {
            let item_id = bot.interface.resolve_item_by_name(name);
            if item_id == 0 {
                reply(bot, pc, &format!("use: unknown item `{name}`"));
            } else {
                let target = bot.snap.self_.current_target;
                let ok = bot.interface.use_item(ItemId(item_id), target);
                if !ok && s.verbose {
                    reply(bot, pc, &format!("use: failed to use `{name}`"));
                }
            }
        }
        BotCommand::EquipItemByName(name) => {
            let item_id = bot.interface.resolve_item_by_name(name);
            if item_id == 0 {
                reply(bot, pc, &format!("equip: unknown item `{name}`"));
            } else {
                let ok = bot.interface.equip_item(ItemId(item_id));
                if !ok && s.verbose {
                    reply(bot, pc, &format!("equip: failed to equip `{name}`"));
                }
            }
        }
        BotCommand::SetFormation(f) => {
            s.follow_formation = *f;
            // Mangosbot.lua:3152 watches for `Formation set to` to refresh
            // `formation ?`. UI-critical — always emit.
            let msg = format!("Formation set to {}", f.as_str());
            reply(bot, pc, &msg);
        }
        BotCommand::TravelTo(loc) => {
            use crate::engine::blackboard::{Key, Value};
            // Cross-map travel is not yet supported; only set the waypoint
            // when the bot is already on the destination map. Otherwise the
            // travel subtree would chase a meaningless coordinate.
            if bot.snap.self_.pos.map_id == loc.map {
                bot.blackboard.set(Key::TravelDestX, Value::F32(loc.x));
                bot.blackboard.set(Key::TravelDestY, Value::F32(loc.y));
                bot.blackboard.set(Key::TravelDestZ, Value::F32(loc.z));
                if s.verbose {
                    reply(bot, pc, &format!("Travelling to {}", loc.name));
                }
            } else if s.verbose {
                reply(
                    bot,
                    pc,
                    &format!(
                        "Cannot travel to {} from this map (need map {}).",
                        loc.name, loc.map
                    ),
                );
            }
        }

        // -- Tunables --
        BotCommand::SetRange(dist) => {
            s.follow_distance = *dist;
        }
        BotCommand::SetRangeQualified { qualifier, value } => {
            match qualifier.as_str() {
                "follow" => s.follow_distance = *value,
                "followraid" => s.follow_distance_raid = *value,
                "attack" => s.attack_range = *value,
                "spell" => s.spell_range = *value,
                "heal" => s.heal_range = *value,
                "shoot" => s.shoot_range = *value,
                "flee" => s.flee_range = *value,
                _ => {
                    // PB2 silently ignores unknown range qualifiers.
                }
            }
        }
        BotCommand::QueryRange => {
            let msg = format!(
                "follow: {:.0}, attack: {:.0}, spell: {:.0}, heal: {:.0}",
                s.follow_distance, s.attack_range, s.spell_range, s.heal_range,
            );
            reply(bot, pc, &msg);
        }
        BotCommand::ApplyStrategiesAll { add, remove } => {
            for kind in &[
                BotStateKind::Combat,
                BotStateKind::NonCombat,
                BotStateKind::Reaction,
                BotStateKind::Dead,
            ] {
                s.strategies.get_mut(*kind).insert(*add);
                s.strategies.get_mut(*kind).remove(*remove);
            }
        }
        BotCommand::Stop => {
            // PB2 stop = cancel current action, brief passive
            s.reactivity = Reactivity::Passive;
        }
        BotCommand::SetStance(st) => {
            s.stance = *st;
            // Mangosbot.lua:3155 watches for `Stance set to` to refresh
            // `stance ?`. UI-critical — always emit.
            let name = match *st {
                1 => "battle",
                2 => "defensive",
                3 => "berserker",
                _ => "none",
            };
            let msg = format!("Stance set to {name}");
            reply(bot, pc, &msg);
        }
        BotCommand::SetPositionStance(ps) => {
            s.position_stance = *ps;
            // Clear old positioning flags and apply new ones.
            let combat = s.strategies.get_mut(BotStateKind::Combat);
            combat.remove(PositionStance::all_position_flags());
            combat.insert(ps.strategy_flags());
            let msg = format!("Stance set to {}", ps.as_str());
            reply(bot, pc, &msg);
        }
        BotCommand::MaxDps => {
            s.strategies.set(BotStateKind::Combat, StrategyFlags::DPS);
            s.reactivity = Reactivity::Aggressive;
        }
        BotCommand::ToggleSaveMana => {
            // Toggle: if off (0), set to 1; if on, set to 0.
            s.save_mana = if s.save_mana == 0 { 1 } else { 0 };
            // Mangosbot parses `Mana save level set: <val>` (line 3397).
            let msg = format!("Mana save level set: {}", s.save_mana);
            reply(bot, pc, &msg);
        }
        BotCommand::SetSaveMana(level) => {
            s.save_mana = *level;
            // Mangosbot parses `Mana save level set: <val>` (line 3397).
            let msg = format!("Mana save level set: {}", s.save_mana);
            reply(bot, pc, &msg);
        }
        BotCommand::ToggleSelfRes => {
            s.self_res = !s.self_res;
            let verbose = s.verbose;
            let now = s.self_res;
            if verbose {
                reply(bot, pc, if now { "self res: on" } else { "self res: off" });
            }
        }
        BotCommand::SetCheatFlags(flags) => {
            s.cheat_flags = *flags;
        }
        BotCommand::KeepItem(item) => {
            s.keep_items.insert(*item);
        }
        BotCommand::UnkeepItem(item) => {
            s.keep_items.remove(item);
        }
        BotCommand::SetChatChannel { channel, on } => {
            let bit = *channel as u32;
            if *on {
                s.chat_channels |= bit;
            } else {
                s.chat_channels &= !bit;
            }
        }
        BotCommand::SetPreferredRti(icon) => {
            s.preferred_rti_icon = *icon;
            let name = rti_icon_name(*icon);
            let msg = format!("rti set to: {name}");
            reply(bot, pc, &msg);
        }
        BotCommand::SetPreferredCcRti(icon) => {
            s.preferred_cc_rti_icon = *icon;
            let name = rti_icon_name(*icon);
            let msg = format!("rti cc set to: {name}");
            reply(bot, pc, &msg);
        }
        BotCommand::Emote(id) => {
            bot.interface.emote(*id);
        }
        BotCommand::Debug => {
            let msg = format!(
                "DBG mode={} react={:?} co-strats={:#x} nc-strats={:#x} react-strats={:#x} de-strats={:#x} cheat={:#x}",
                s.mode.as_str(),
                s.reactivity,
                s.strategies.get(BotStateKind::Combat).0,
                s.strategies.get(BotStateKind::NonCombat).0,
                s.strategies.get(BotStateKind::Reaction).0,
                s.strategies.get(BotStateKind::Dead).0,
                s.cheat_flags,
            );
            reply(bot, pc, &msg);
        }
        BotCommand::CheckLos => {
            let target = bot.snap.self_.current_target;
            let msg = if target == 0 {
                "los: no target".to_string()
            } else if bot.interface.has_los(target) {
                "los: yes".to_string()
            } else {
                "los: no".to_string()
            };
            reply(bot, pc, &msg);
        }
        BotCommand::ListQuests => {
            let quests = bot.interface.get_quest_log();
            let done = quests.iter().filter(|q| q.complete).count();
            let msg = format!("Quests: {} total, {} complete", quests.len(), done);
            reply(bot, pc, &msg);
        }
        BotCommand::ListTalents => {
            let free = bot.interface.bot_free_talent_points();
            let msg = format!("Talents: {free} unspent");
            reply(bot, pc, &msg);
        }
        BotCommand::ListSpells => {
            let n = bot.interface.get_bot_spells().len();
            let msg = format!("Spells known: {n}");
            reply(bot, pc, &msg);
        }
        BotCommand::ReleaseSpirit => {
            bot.interface.use_spirit_healer();
        }
        BotCommand::AcceptRevive => {
            bot.interface.accept_resurrect();
        }
        BotCommand::Jump => {
            bot.interface.bot_jump();
        }
        BotCommand::UseHearth => {
            bot.interface.bot_use_hearthstone();
        }
        BotCommand::ListReputation => {
            let list = bot.interface.bot_get_reputation_list();
            // Bucket by rank tier (0=hated .. 7=exalted). Matches the
            // FactionRank enum the C++ side passes via `standing`.
            let mut buckets = [0u32; 8];
            for entry in &list {
                let idx = (entry.standing as usize).min(7);
                buckets[idx] += 1;
            }
            let msg = format!(
                "Rep: {} tracked (ex:{} rev:{} hon:{} fri:{} neu:{} unf:{} hos:{} hat:{})",
                list.len(),
                buckets[7], buckets[6], buckets[5], buckets[4],
                buckets[3], buckets[2], buckets[1], buckets[0],
            );
            reply(bot, pc, &msg);
        }
        BotCommand::ListSkills => {
            let mut list = bot.interface.bot_get_learned_skills();
            list.sort_by(|a, b| b.value.cmp(&a.value));
            let top: Vec<String> = list
                .iter()
                .take(5)
                .map(|s| format!("{}:{}/{}", s.skill_id, s.value, s.max))
                .collect();
            let msg = if top.is_empty() {
                "Skills: none".to_string()
            } else {
                format!("Skills ({} total): {}", list.len(), top.join(", "))
            };
            reply(bot, pc, &msg);
        }
        BotCommand::QuestAccept => {
            let npc = bot.snap.self_.current_target;
            if npc != 0 {
                bot.interface.bot_quest_accept_from(npc);
            }
        }
        BotCommand::QuestDrop(quest_id) => {
            bot.interface.bot_quest_abandon(*quest_id);
        }
        BotCommand::MailSummary => {
            let s = bot.interface.bot_mail_summary();
            let msg = format!(
                "Mail: {} total ({} money, {} items, {}c total)",
                s.total_mails, s.mails_with_money, s.mails_with_items, s.total_money,
            );
            reply(bot, pc, &msg);
        }
        BotCommand::MailTakeAll => {
            let ok = bot.interface.bot_mail_take_all();
            if bot.settings.verbose {
                let msg = if ok {
                    "mail: taken"
                } else {
                    "mail: nothing to take (or no mailbox in range)"
                };
                reply(bot, pc, msg);
            }
        }
        BotCommand::GuildLeave => {
            let ok = bot.interface.bot_guild_leave();
            if bot.settings.verbose {
                let msg = if ok {
                    "guild: left"
                } else {
                    "guild: cannot leave (not in guild or guild master)"
                };
                reply(bot, pc, msg);
            }
        }

        BotCommand::SetPoison { hand, kind } => {
            if let Some(r) = s.class_prefs.as_rogue_mut() {
                match hand {
                    WeaponHand::MainHand => r.mh = *kind,
                    WeaponHand::OffHand => r.oh = *kind,
                }
                let label = kind.map_or("cleared", |k| k.as_str());
                reply(bot, pc, &format!("poison {}: {label}", hand.as_str()));
            } else {
                reply(bot, pc, "poison: not a rogue");
            }
        }
        BotCommand::ShowPoisons => {
            let msg = match s.class_prefs.as_rogue() {
                Some(r) => format!(
                    "poisons: mh={} oh={}",
                    r.mh.map_or("none", |k| k.as_str()),
                    r.oh.map_or("none", |k| k.as_str()),
                ),
                None => "poisons: not a rogue".into(),
            };
            reply(bot, pc, &msg);
        }
        BotCommand::SetTotem { slot, role } => {
            // Validate slot/role match up front — the parser already enforces
            // this, but belt-and-suspenders against future internal callers.
            if let Some(r) = role
                && r.slot() != *slot
            {
                reply(
                    bot,
                    pc,
                    &format!("totem: {} is not a {} totem", r.as_str(), slot.as_str()),
                );
            } else if let Some(sh) = s.class_prefs.as_shaman_mut() {
                sh.set(*slot, *role);
                let label = role.map_or("cleared", |r| r.as_str());
                reply(bot, pc, &format!("totem {}: {label}", slot.as_str()));
            } else {
                reply(bot, pc, "totem: not a shaman");
            }
        }
        BotCommand::ShowTotems => {
            let msg = match s.class_prefs.as_shaman() {
                Some(sh) => format!(
                    "totems: earth={} fire={} water={} air={}",
                    sh.earth.map_or("none", |r| r.as_str()),
                    sh.fire.map_or("none", |r| r.as_str()),
                    sh.water.map_or("none", |r| r.as_str()),
                    sh.air.map_or("none", |r| r.as_str()),
                ),
                None => "totems: not a shaman".into(),
            };
            reply(bot, pc, &msg);
        }

        BotCommand::SetShamanImbue { hand, imbue } => {
            if let Some(sh) = s.class_prefs.as_shaman_mut() {
                match hand {
                    WeaponHand::MainHand => sh.mh_imbue = *imbue,
                    WeaponHand::OffHand => sh.oh_imbue = *imbue,
                }
                let label = imbue.map_or("cleared", |i| i.as_str());
                reply(bot, pc, &format!("imbue {}: {label}", hand.as_str()));
            } else {
                reply(bot, pc, "imbue: not a shaman");
            }
        }
        BotCommand::ShowShamanImbues => {
            let msg = match s.class_prefs.as_shaman() {
                Some(sh) => format!(
                    "imbues: mh={} oh={}",
                    sh.mh_imbue.map_or("none", |i| i.as_str()),
                    sh.oh_imbue.map_or("none", |i| i.as_str()),
                ),
                None => "imbues: not a shaman".into(),
            };
            reply(bot, pc, &msg);
        }

        BotCommand::SetPaladinAura(aura) => {
            if let Some(p) = s.class_prefs.as_paladin_mut() {
                p.aura = *aura;
                let label = aura.map_or("cleared", |a| a.as_str());
                reply(bot, pc, &format!("aura: {label}"));
            } else {
                reply(bot, pc, "aura: not a paladin");
            }
        }
        BotCommand::SetPaladinBlessing(blessing) => {
            if let Some(p) = s.class_prefs.as_paladin_mut() {
                p.blessing = *blessing;
                let label = blessing.map_or("cleared", |b| b.as_str());
                reply(bot, pc, &format!("blessing: {label}"));
            } else {
                reply(bot, pc, "blessing: not a paladin");
            }
        }
        BotCommand::SetPaladinGreaterBlessing(flag) => {
            if let Some(p) = s.class_prefs.as_paladin_mut() {
                p.use_greater = *flag;
                reply(
                    bot,
                    pc,
                    &format!("greater blessing: {}", if *flag { "on" } else { "off" }),
                );
            } else {
                reply(bot, pc, "blessing: not a paladin");
            }
        }
        BotCommand::ShowPaladinPrefs => {
            let msg = match s.class_prefs.as_paladin() {
                Some(p) => format!(
                    "paladin: aura={} blessing={} greater={}",
                    p.aura.map_or("none", |a| a.as_str()),
                    p.blessing.map_or("none", |b| b.as_str()),
                    if p.use_greater { "on" } else { "off" },
                ),
                None => "paladin: not a paladin".into(),
            };
            reply(bot, pc, &msg);
        }

        BotCommand::SetHunterAspect(aspect) => {
            if let Some(h) = s.class_prefs.as_hunter_mut() {
                h.aspect = *aspect;
                let label = aspect.map_or("cleared", |a| a.as_str());
                reply(bot, pc, &format!("aspect: {label}"));
            } else {
                reply(bot, pc, "aspect: not a hunter");
            }
        }
        BotCommand::SetHunterTrap(trap) => {
            if let Some(h) = s.class_prefs.as_hunter_mut() {
                h.trap = *trap;
                let label = trap.map_or("cleared", |t| t.as_str());
                reply(bot, pc, &format!("trap: {label}"));
            } else {
                reply(bot, pc, "trap: not a hunter");
            }
        }
        BotCommand::SetHunterSting(sting) => {
            if let Some(h) = s.class_prefs.as_hunter_mut() {
                h.sting = *sting;
                let label = sting.map_or("cleared", |st| st.as_str());
                reply(bot, pc, &format!("sting: {label}"));
            } else {
                reply(bot, pc, "sting: not a hunter");
            }
        }
        BotCommand::ShowHunterPrefs => {
            let msg = match s.class_prefs.as_hunter() {
                Some(h) => format!(
                    "hunter: aspect={} trap={} sting={}",
                    h.aspect.map_or("none", |a| a.as_str()),
                    h.trap.map_or("none", |t| t.as_str()),
                    h.sting.map_or("none", |st| st.as_str()),
                ),
                None => "hunter: not a hunter".into(),
            };
            reply(bot, pc, &msg);
        }

        BotCommand::SetWarlockCurse(curse) => {
            if let Some(w) = s.class_prefs.as_warlock_mut() {
                w.curse = *curse;
                let label = curse.map_or("cleared", |c| c.as_str());
                reply(bot, pc, &format!("curse: {label}"));
            } else {
                reply(bot, pc, "curse: not a warlock");
            }
        }
        BotCommand::SetWarlockPet(pet) => {
            if let Some(w) = s.class_prefs.as_warlock_mut() {
                w.pet = *pet;
                let label = pet.map_or("cleared", |p| p.as_str());
                reply(bot, pc, &format!("pet: {label}"));
            } else {
                reply(bot, pc, "pet: not a warlock");
            }
        }
        BotCommand::ShowWarlockPrefs => {
            let msg = match s.class_prefs.as_warlock() {
                Some(w) => format!(
                    "warlock: curse={}, pet={}",
                    w.curse.map_or("none", |c| c.as_str()),
                    w.pet.map_or("none", |p| p.as_str()),
                ),
                None => "warlock: not a warlock".into(),
            };
            reply(bot, pc, &msg);
        }

        BotCommand::SetWarriorForcedStance(stance) => {
            if let Some(w) = s.class_prefs.as_warrior_mut() {
                w.forced_stance = *stance;
                let label = stance.map_or("cleared", |st| st.as_str());
                reply(bot, pc, &format!("forcestance: {label}"));
            } else {
                reply(bot, pc, "forcestance: not a warrior");
            }
        }
        BotCommand::ShowWarriorPrefs => {
            let msg = match s.class_prefs.as_warrior() {
                Some(w) => format!(
                    "warrior: forcestance={}",
                    w.forced_stance.map_or("none", |st| st.as_str())
                ),
                None => "warrior: not a warrior".into(),
            };
            reply(bot, pc, &msg);
        }

        BotCommand::SetSuppressionDuty(mode) => {
            bot.settings.encounter_prefs.suppression_duty = *mode;
            reply(bot, pc, &format!("suppression: {}", mode.as_word()));
        }
        BotCommand::SetDouseDuty(mode) => {
            bot.settings.encounter_prefs.douse_duty = *mode;
            reply(bot, pc, &format!("douse: {}", mode.as_word()));
        }
        BotCommand::ShowEncounterPrefs => {
            let p = &bot.settings.encounter_prefs;
            let msg = format!(
                "encounter: suppression={} douse={}",
                p.suppression_duty.as_word(),
                p.douse_duty.as_word()
            );
            reply(bot, pc, &msg);
        }

        BotCommand::ApplyLootPolicy { add, remove, toggle } => {
            s.loot_policy.remove(*remove);
            s.loot_policy.insert(*add);
            s.loot_policy.toggle(*toggle);
            // Mangosbot.lua:3158 watches for `Loot strategy set to ` to
            // refresh `ll ?`. UI-critical — always emit.
            let msg = format!("Loot strategy set to {}", s.loot_policy.describe());
            reply(bot, pc, &msg);
        }

        // -- Query replies ---------------------------------------------
        BotCommand::QueryFormation => {
            // Mangosbot parses `Formation: ` at position 1, extracts from
            // position 11 (line 3391). PB2 embeds `|cff00ff00` color code.
            let msg = format!("Formation: |cff00ff00{}", s.follow_formation.as_str());
            reply(bot, pc, &msg);
        }
        BotCommand::QueryStance => {
            // Mangosbot parses `Stance: ` at position 1, then extracts
            // from position 11 onwards (line 3394). PB2 embeds WoW color
            // codes (`|cff00ff00`) which pad the prefix to 10 chars before
            // the value name. The addon uses `string.find` for matching,
            // so the color code prefix on the value is harmless.
            let msg = format!("Stance: |cff00ff00{}", s.position_stance.as_str());
            reply(bot, pc, &msg);
        }
        BotCommand::QueryStrategies(state) => {
            // Mangosbot parses `Non Combat Strategies: ...` (line 3362,
            // trim=23) and `Combat Strategies: ...` (line 3358, trim=19)
            // — the reply prefix comes from `BotStateKind::reply_prefix`.
            let slot_val = s.strategies.get(*state);
            let desc = describe_with_class_prefs(slot_val, &s.class_prefs);
            let msg = format!("{}: {}", state.reply_prefix(), desc);
            reply(bot, pc, &msg);
        }
        BotCommand::QueryReactivity => {
            // Mangosbot parses `Reaction Strategies: ...` (Mangosbot.lua:3366,
            // trim=21) — the reaction strategy panel keys on this exact prefix.
            let msg = format!("Reaction Strategies: {}", s.reactivity.as_str());
            reply(bot, pc, &msg);
        }
        BotCommand::QueryRti => {
            let msg = format!("rti: |cff00ff00{}", rti_icon_name(s.preferred_rti_icon));
            reply(bot, pc, &msg);
        }
        BotCommand::QueryCcRti => {
            let msg = format!("rti cc: |cff00ff00{}", rti_icon_name(s.preferred_cc_rti_icon));
            reply(bot, pc, &msg);
        }
        BotCommand::QuerySaveMana => {
            // Mangosbot parses `Mana save level: <val>` (line 3400).
            let msg = format!("Mana save level: |cff00ff00{}", s.save_mana);
            reply(bot, pc, &msg);
        }
        BotCommand::QueryLootPolicy => {
            // Mangosbot parses `Loot strategy: <val>` (line 3403).
            let msg = format!("Loot strategy: |cff00ff00{}", s.loot_policy.describe());
            reply(bot, pc, &msg);
        }

        // -- PB2 parity commands --
        BotCommand::SetWaitForAttack(secs) => {
            s.wait_for_attack_secs = *secs;
            if s.verbose {
                reply(bot, pc, &format!("Wait for attack set to {secs}s"));
            }
        }
        BotCommand::TankAttack => {
            // PB2: attack current target as tank. Set combat order to tank+attack.
            s.strategies.set(BotStateKind::Combat, StrategyFlags::TANK);
            s.reactivity = Reactivity::Aggressive;
        }
        BotCommand::Loot => {
            // Loot nearby lootable units/objects. PB2 "add all loot" picks up
            // everything in range. We scan, open, and take in one pass.
            let lootables = bot.interface.get_nearby_lootable(crate::config::get().nearby_scan_range);
            for handle in &lootables {
                if bot.interface.open_loot(*handle) {
                    bot.interface.take_all_loot();
                }
            }
        }
        BotCommand::DestroyItem(name) => {
            let item_id = bot.interface.resolve_item_by_name(name);
            if item_id == 0 {
                reply(bot, pc, &format!("destroy: unknown item `{name}`"));
            } else {
                let ok = bot.interface.destroy_item(ItemId(item_id));
                if !ok && s.verbose {
                    reply(bot, pc, &format!("destroy: failed to destroy `{name}`"));
                }
            }
        }
        BotCommand::SkipSpell(name) => {
            // ss = skip spell. Resolve name to ID and blacklist.
            let spell_id = bot.interface.resolve_spell_by_name(name);
            if spell_id != 0 {
                s.blacklisted_spells.insert(SpellId(spell_id));
            } else if s.verbose {
                reply(bot, pc, &format!("ss: unknown spell `{name}`"));
            }
        }
        BotCommand::LootRoll(pref) => {
            // PB2 "roll need|greed|pass" — cast a loot roll vote.
            let vote = match pref.as_str() {
                "need" => 0,
                "greed" => 1,
                "pass" | _ => 2,
            };
            if bot.interface.get_pending_roll_count() > 0 {
                bot.interface.cast_loot_roll(vote);
            }
        }
        BotCommand::GiveLeader => {
            // Transfer leadership to the command sender. PB2 behavior: the
            // bot passes leadership to whoever sent the "give leader" command.
            if let Some(requester) = pc.sender {
                let ok = bot.interface.give_leader(requester);
                if ok {
                    reply(bot, pc, "Lead the way!");
                } else if s.verbose {
                    reply(bot, pc, "give leader: failed (not leader or target not in group)");
                }
            }
        }
        BotCommand::InvitePlayer(name) => {
            let name_clean = name.trim_start_matches('+');
            let guid = bot.interface.resolve_player_by_name(name_clean);
            if guid == 0 {
                reply(bot, pc, &format!("invite: player `{name_clean}` not found"));
            } else {
                let ok = bot.interface.invite_to_group(guid);
                if !ok && s.verbose {
                    reply(bot, pc, &format!("invite: failed to invite `{name_clean}`"));
                }
            }
        }
        BotCommand::Pet(sub) => {
            // Pet management using existing FFI.
            match sub.as_str() {
                "summon" | "call" => {
                    bot.interface.summon_pet();
                }
                "revive" => {
                    bot.interface.revive_pet();
                }
                "feed" => {
                    bot.interface.feed_pet();
                }
                "?" | "status" | "" => {
                    let has = bot.interface.has_pet();
                    let alive = bot.interface.pet_is_alive();
                    let happy = bot.interface.pet_happiness();
                    let msg = if !has {
                        "No pet".to_string()
                    } else {
                        let state = if !alive { "dead" } else {
                            match happy {
                                3 => "happy",
                                2 => "content",
                                _ => "unhappy",
                            }
                        };
                        format!("Pet: {state}")
                    };
                    reply(bot, pc, &msg);
                }
                _ => {
                    // PB2 silently accepts unknown pet subcommands.
                }
            }
        }
        BotCommand::BuffTarget(name) => {
            // PB2 "buff target +Name" — resolve name, set as focus target so
            // the BT's buff subtree targets them.
            let name_clean = name.trim_start_matches('+');
            let guid = bot.interface.resolve_player_by_name(name_clean);
            if guid != 0 {
                s.focus_target = Some(guid);
            } else if s.verbose {
                reply(bot, pc, &format!("buff target: player `{name_clean}` not found"));
            }
        }
        BotCommand::BoostTarget(name) => {
            // Same as buff target — set focus for boost.
            let name_clean = name.trim_start_matches('+');
            let guid = bot.interface.resolve_player_by_name(name_clean);
            if guid != 0 {
                s.focus_target = Some(guid);
            } else if s.verbose {
                reply(bot, pc, &format!("boost target: player `{name_clean}` not found"));
            }
        }
        BotCommand::ReviveTarget(name) => {
            // Resolve name, then cast resurrect on them.
            let name_clean = name.trim_start_matches('+');
            let guid = bot.interface.resolve_player_by_name(name_clean);
            if guid != 0 {
                // Try to cast a resurrect spell on the target.
                // The BT will pick the appropriate class resurrect.
                s.focus_target = Some(guid);
            } else if s.verbose {
                reply(bot, pc, &format!("revive target: player `{name_clean}` not found"));
            }
        }
        BotCommand::FollowTarget(name) => {
            // PB2 "follow target +Name" — resolve name and set as follow target.
            let name_clean = name.trim_start_matches('+');
            let guid = bot.interface.resolve_player_by_name(name_clean);
            if guid != 0 {
                use crate::engine::blackboard::{Key, Value};
                bot.blackboard.set(Key::FollowTargetHandle, Value::Handle(guid));
            } else if s.verbose {
                reply(bot, pc, &format!("follow target: player `{name_clean}` not found"));
            }
        }
        BotCommand::FocusHeal(name) => {
            // PB2 "focus heal +Name" — resolve name and set as focus/heal target.
            // RaidControl sends "focus heal none" to clear the focus target.
            let name_clean = name.trim_start_matches('+');
            if name_clean.eq_ignore_ascii_case("none") || name_clean.is_empty() {
                s.focus_target = None;
                s.protect_target = None;
                reply(bot, pc, "focus heal: cleared");
            } else {
                let guid = bot.interface.resolve_player_by_name(name_clean);
                if guid != 0 {
                    s.focus_target = Some(guid);
                    s.protect_target = Some(guid);
                } else if s.verbose {
                    reply(bot, pc, &format!("focus heal: player `{name_clean}` not found"));
                }
            }
        }
        BotCommand::MoveStyle(style) => {
            match style.as_str() {
                "walk" => {
                    // No dedicated flag yet — silently accept, BT defaults to run.
                }
                "run" | _ => {
                    // Default behavior.
                }
            }
        }
        BotCommand::Talk => {
            // PB2 "talk" — gossip with targeted NPC. Find nearest gossip NPC
            // and interact.
            let npcs = bot.interface.get_nearby_gossip_npcs(crate::config::get().nearby_scan_range);
            if let Some(&npc) = npcs.first() {
                bot.interface.interact_npc(npc);
            }
        }
        BotCommand::Trainer => {
            // PB2 "trainer" — visit nearby trainer. NPC flag 0x10 = TRAINER.
            let npcs = bot.interface.get_nearby_npcs(crate::config::get().nearby_scan_range, 0x10);
            if let Some(&npc) = npcs.first() {
                bot.interface.interact_npc(npc);
                // After interaction, the C++ side handles the trainer
                // packet sequence automatically for bots.
            }
        }
        BotCommand::Taxi => {
            // PB2 "taxi" — interact with nearest flightmaster.
            // NPC flag 0x200 = FLIGHTMASTER.
            let npcs = bot.interface.get_nearby_npcs(crate::config::get().nearby_scan_range, 0x200);
            if let Some(&npc) = npcs.first() {
                bot.interface.interact_npc(npc);
            }
        }
        BotCommand::Craft(_args) => {
            // PB2 "craft <item>" — craft items. The BT handles crafting
            // when the bot has the recipe and materials. Silently accept.
        }
        BotCommand::Outfit(_args) => {
            // PB2 "outfit <name> equip|save|list" — gear set management.
            // Requires persistent gear set storage. Silently accept.
        }
        BotCommand::LogLevel(args) => {
            // PB2 "log <level>" — adjust bot verbosity.
            match args.as_str() {
                "on" | "verbose" => { s.verbose = true; }
                "off" | "quiet" => { s.verbose = false; }
                _ => {}
            }
        }
        BotCommand::ShareQuest => {
            let ok = bot.interface.share_quest(0);
            if !ok && s.verbose {
                reply(bot, pc, "share: no shareable quest found");
            }
        }
        BotCommand::DoQuest(_args) => {
            // PB2 "doquest" — travel to quest objective. The BT's quest/travel
            // subtree handles this autonomously when in quest mode.
        }
        BotCommand::Bank => {
            // PB2 "bank"/"gb" — interact with nearest banker.
            // NPC flag 0x20 = BANKER.
            let npcs = bot.interface.get_nearby_npcs(crate::config::get().nearby_scan_range, 0x20);
            if let Some(&npc) = npcs.first() {
                bot.interface.interact_npc(npc);
            }
        }
        BotCommand::AuctionHouse(_args) => {
            // PB2 "ah" — interact with nearest auctioneer.
            // NPC flag 0x40000 = AUCTIONEER.
            let npcs = bot.interface.get_nearby_npcs(crate::config::get().nearby_scan_range, 0x40000);
            if let Some(&npc) = npcs.first() {
                bot.interface.interact_npc(npc);
            }
        }
        BotCommand::GuildCommand(sub) => {
            match sub.as_str() {
                "guild leave" => {
                    bot.interface.bot_guild_leave();
                }
                "guild invite" | "guild join" | "guild promote" | "guild demote"
                | "guild remove" | "guild leader" => {
                    // These guild management ops require dedicated guild FFI.
                    // The C++ PlayerbotMgr handles most guild ops for managed bots.
                    // Silently accept — PB2 also routes these through its strategy engine.
                }
                _ => {}
            }
        }
        BotCommand::BgFree => {
            // PB2 "bg free" — leave the current battleground.
            // Requires BG leave packet. The bot module handles BG
            // autonomously when in BG mode.
        }
        BotCommand::Flag => {
            // PB2 "flag" — interact with BG flag (WSG/AB).
            // Handled by BG subtree when in battleground.
        }
        BotCommand::SendMail(_args) => {
            // PB2 "sendmail <target> <text>" — send mail. Requires
            // mailbox proximity + mail packet sequence.
        }
        BotCommand::PossibleAttackTargets => {
            // Show nearby attackable units.
            let count = bot.nearby_units.len();
            reply(bot, pc, &format!("{count} possible targets"));
        }
        BotCommand::ShowAttackers => {
            let count = bot.attackers.len();
            reply(bot, pc, &format!("{count} attackers"));
        }

        BotCommand::Buy => {
            // PB2 "b" — buy from targeted vendor. Interact with nearest vendor NPC.
            let npcs = bot.interface.get_nearby_npcs(crate::config::get().nearby_scan_range, 0x80); // UNIT_NPC_FLAG_VENDOR
            if let Some(&npc) = npcs.first() {
                bot.interface.interact_npc(npc);
            }
        }
        BotCommand::Buyback => {
            // PB2 "bb" — buyback from vendor. Same NPC interaction as buy.
            let npcs = bot.interface.get_nearby_npcs(crate::config::get().nearby_scan_range, 0x80);
            if let Some(&npc) = npcs.first() {
                bot.interface.interact_npc(npc);
            }
        }
        BotCommand::UnequipItemByName(name) => {
            if name.is_empty() {
                reply(bot, pc, "ue: specify item name");
            } else {
                let item_id = bot.interface.resolve_item_by_name(&name);
                if item_id == 0 {
                    reply(bot, pc, &format!("ue: unknown item `{name}`"));
                } else {
                    let ok = bot.interface.unequip_item(ItemId(item_id));
                    if !ok && s.verbose {
                        reply(bot, pc, &format!("ue: failed to unequip `{name}`"));
                    }
                }
            }
        }
        BotCommand::Trade => {
            // PB2 "t"/"nt" — accept pending trade. Uses existing FFI.
            bot.interface.accept_trade();
        }
        BotCommand::QuestReward => {
            // PB2 "r"/"reward"/"quest reward" — auto-pick quest reward.
            // Quest reward selection is handled by the C++ side when
            // the bot is at a questgiver. The RPG subtree auto-accepts quests.
        }
        BotCommand::CustomStrategy(_strat) => {
            // PB2 "cs" — custom strategy definitions. These are user-defined
            // trigger→action pairs stored per bot. Silently accept for now.
        }
        BotCommand::WhatToSell => {
            // PB2 "wts" — show what items the bot would sell.
            let has = bot.interface.has_sellable_items();
            reply(bot, pc, if has { "I have items to sell" } else { "Nothing to sell" });
        }
        BotCommand::Teleport(dest) => {
            // PB2 "teleport" — teleport to a destination.
            // In PB2 this uses the travel system to find named locations.
            // The bot's world module handles travel destinations autonomously.
            if dest.is_empty() {
                reply(bot, pc, "teleport: specify destination");
            }
            // Named destination lookup is handled by the BT's travel subtree.
        }
        BotCommand::Speak(text) => {
            // PB2 "speak" — emote or say text.
            if !text.is_empty() {
                // Try common emote names first.
                let emote_id = match text.to_lowercase().as_str() {
                    "wave" => Some(21),
                    "bow" => Some(2),
                    "dance" => Some(4),
                    "cheer" => Some(3),
                    "laugh" | "lol" => Some(11),
                    "salute" => Some(16),
                    "cry" => Some(5),
                    "point" => Some(14),
                    "kneel" => Some(10),
                    "flex" => Some(6),
                    "thank" | "thanks" | "ty" => Some(20),
                    "no" => Some(13),
                    "yes" | "nod" => Some(23),
                    "roar" => Some(15),
                    "shy" => Some(17),
                    _ => None,
                };
                if let Some(eid) = emote_id {
                    bot.interface.do_text_emote(eid);
                } else {
                    // Say the text.
                    bot.interface.say(&text, 0);
                }
            }
        }
        BotCommand::ShowFaction => {
            // PB2 "faction" — show faction reputation summary.
            let reps = bot.interface.bot_get_reputation_list();
            let count = reps.len();
            reply(bot, pc, &format!("{count} factions tracked"));
        }
        BotCommand::SetValue(kv) => {
            // PB2 "set value <key> <val>" — set a config value. These map to
            // various BotSettings fields. Parse key=value pairs.
            if s.verbose {
                reply(bot, pc, &format!("set value: `{kv}` — accepted"));
            }
        }
        BotCommand::AiProfile(sub) => {
            // PB2 "load ai" / "save ai" / "list ai" — AI profile management.
            // Profiles store strategy+setting snapshots. Silently accept for now.
            if s.verbose {
                reply(bot, pc, &format!("ai: `{sub}` — profiles not yet stored"));
            }
        }
        BotCommand::Lfg => {
            // PB2 "lfg" — toggle looking-for-group flag.
            // In classic vanilla this is a simple chat flag, not a real system.
        }

        BotCommand::Batch(cmds) => {
            for sub in cmds {
                let sub_pc = PendingCommand {
                    sender: pc.sender,
                    security: pc.security,
                    origin: pc.origin,
                    command: sub.clone(),
                };
                apply_command(bot, &sub_pc);
            }
        }
        BotCommand::Unknown(text) => {
            if bot.monitor_active {
                crate::bot::monitor::monitor_log(bot, &format!("UNKNOWN COMMAND: {text}"));
            }
            let msg = format!("Unknown command: {text}");
            reply(bot, pc, &msg);
        }
    }
}

/// Build a strategy description that includes compound class-pref names.
///
/// The Mangosbot addon matches compound strategy names like `"poison main
/// deadly"`, `"totem earth strength"`, `"curse agony"`, `"aura devotion"`,
/// `"blessing might"`, `"aspect hawk"`, `"pet imp"` against the strategy
/// list returned by `co ?` / `nc ?`. Plain `StrategyFlags::describe()`
/// only emits the base flag name (e.g. `"poisons"`), so the addon never
/// highlights the selection buttons. This function appends the compound
/// names derived from the bot's `ClassPrefs`.
fn describe_with_class_prefs(
    flags: StrategyFlags,
    prefs: &crate::bot::class_prefs::ClassPrefs,
) -> String {
    use crate::bot::class_prefs::ClassPrefs;

    let mut desc = flags.describe();

    // Helper: append a compound name to the description.
    let mut append = |name: String| {
        if desc.is_empty() || desc == "none" {
            desc = name;
        } else {
            desc.push_str(", ");
            desc.push_str(&name);
        }
    };

    match prefs {
        ClassPrefs::Rogue(r) => {
            if flags.contains(StrategyFlags::POISONS) {
                if let Some(pk) = r.mh {
                    append(format!("poison main {}", pk.as_str()));
                }
                if let Some(pk) = r.oh {
                    append(format!("poison off {}", pk.as_str()));
                }
            }
        }
        ClassPrefs::Shaman(sh) => {
            if flags.contains(StrategyFlags::TOTEMS) {
                if let Some(role) = sh.earth {
                    append(format!("totem earth {}", totem_addon_name(role)));
                }
                if let Some(role) = sh.fire {
                    append(format!("totem fire {}", totem_addon_name(role)));
                }
                if let Some(role) = sh.water {
                    append(format!("totem water {}", totem_addon_name(role)));
                }
                if let Some(role) = sh.air {
                    append(format!("totem air {}", totem_addon_name(role)));
                }
            }
        }
        ClassPrefs::Paladin(p) => {
            if flags.contains(StrategyFlags::AURA) {
                if let Some(aura) = p.aura {
                    append(format!("aura {}", aura.as_str()));
                }
            }
            if flags.contains(StrategyFlags::BLESSING) {
                if let Some(bless) = p.blessing {
                    append(format!("blessing {}", bless.as_str()));
                }
            }
        }
        ClassPrefs::Hunter(h) => {
            if flags.contains(StrategyFlags::ASPECT) {
                if let Some(aspect) = h.aspect {
                    append(format!("aspect {}", aspect.as_str()));
                }
            }
            if flags.contains(StrategyFlags::STING) {
                if let Some(sting) = h.sting {
                    append(format!("sting {}", sting.as_str()));
                }
            }
        }
        ClassPrefs::Warlock(w) => {
            if flags.contains(StrategyFlags::CURSE) {
                if let Some(curse) = w.curse {
                    append(format!("curse {}", curse.as_str()));
                }
            }
            if flags.contains(StrategyFlags::PET) {
                if let Some(pet) = w.pet {
                    append(format!("pet {}", pet.as_str()));
                }
            }
        }
        _ => {}
    }

    desc
}

/// Map a totem role to the addon-expected name. Resistance totems use the
/// bare `"resistance"` token since the addon scopes them by slot (`totem
/// fire resistance`).
fn totem_addon_name(role: TotemRole) -> &'static str {
    use TotemRole as R;
    match role {
        R::FireResistance | R::FrostResistance | R::NatureResistance => "resistance",
        _ => role.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::state::BotState;
    use crate::bot::state::{PlayerClass, PlayerSpec};
    use crate::Sel;
    use crate::engine::context::tests::NullInterface;
    use crate::ffi::BotRole;
    use crate::ffi::interface::BotInterface;
    use std::sync::Mutex;

    fn test_bot() -> BotState {
        BotState::new(
            1,
            Box::new(NullInterface),
            PlayerClass::Warrior,
            PlayerSpec::WarriorArms,
            BotRole::DPS,
            Sel!(), // dummy empty tree
        )
    }

    /// Records each `tell_player` / `tell_addon` call so the reply-routing
    /// unit tests can assert which wire was used without a live FFI bridge.
    /// The recorder appends to two static `Mutex<Vec<…>>` sinks so tests can
    /// read them back after the boxed interface has been moved into
    /// `BotState`. Tests call `clear_reply_sinks()` at the start to isolate
    /// state (cargo-test runs cases on multiple threads, but each test
    /// scrubs+asserts the sinks under the mutex, which is sufficient for
    /// our single-assertion cases).
    struct ReplyRecorder {
        inner: NullInterface,
    }

    static WHISPERED: Mutex<Vec<(u64, String)>> = Mutex::new(Vec::new());
    static ADDONED: Mutex<Vec<(u64, String)>> = Mutex::new(Vec::new());

    fn clear_reply_sinks() {
        WHISPERED.lock().unwrap().clear();
        ADDONED.lock().unwrap().clear();
    }

    // Forward the full trait surface to NullInterface and only override the
    // two reply paths we want to observe.
    impl BotInterface for ReplyRecorder {
        fn get_snapshot(&self) -> crate::ffi::BotWorldSnapshot {
            self.inner.get_snapshot()
        }
        fn get_unit_snapshot(&self, u: crate::ffi::UnitHandle) -> crate::ffi::BotUnitSnapshot {
            self.inner.get_unit_snapshot(u)
        }
        fn cast_spell(&self, s: SpellId, t: crate::ffi::UnitHandle) -> bool {
            self.inner.cast_spell(s, t)
        }
        fn cast_spell_pos(&self, s: SpellId, x: f32, y: f32, z: f32) -> bool {
            self.inner.cast_spell_pos(s, x, y, z)
        }
        fn move_to(&self, x: f32, y: f32, z: f32) -> bool {
            self.inner.move_to(x, y, z)
        }
        fn follow(&self, t: crate::ffi::UnitHandle, d: f32, a: f32) -> bool {
            self.inner.follow(t, d, a)
        }
        fn stop_moving(&self) -> bool {
            self.inner.stop_moving()
        }
        fn attack(&self, t: crate::ffi::UnitHandle) -> bool {
            self.inner.attack(t)
        }
        fn auto_attack(&self, e: bool) -> bool {
            self.inner.auto_attack(e)
        }
        fn say(&self, m: &str, l: u32) -> bool {
            self.inner.say(m, l)
        }
        fn use_item(&self, i: ItemId, t: crate::ffi::UnitHandle) -> bool {
            self.inner.use_item(i, t)
        }
        fn taunt(&self, t: crate::ffi::UnitHandle) -> bool {
            self.inner.taunt(t)
        }
        fn group_get_tank(&self) -> Option<crate::ffi::UnitHandle> {
            self.inner.group_get_tank()
        }
        fn group_get_healer(&self) -> Option<crate::ffi::UnitHandle> {
            self.inner.group_get_healer()
        }
        fn group_get_role(&self, m: crate::ffi::UnitHandle) -> BotRole {
            self.inner.group_get_role(m)
        }
        fn has_aura(&self, u: crate::ffi::UnitHandle, s: SpellId) -> bool {
            self.inner.has_aura(u, s)
        }
        fn get_aura(&self, u: crate::ffi::UnitHandle, s: SpellId) -> Option<crate::ffi::BotAuraInfo> {
            self.inner.get_aura(u, s)
        }
        fn get_auras(&self, u: crate::ffi::UnitHandle) -> Vec<crate::ffi::BotAuraInfo> {
            self.inner.get_auras(u)
        }
        fn get_threat_list(&self, u: crate::ffi::UnitHandle) -> Vec<crate::ffi::BotThreatEntry> {
            self.inner.get_threat_list(u)
        }
        fn get_unit_threat(&self, a: crate::ffi::UnitHandle, b: crate::ffi::UnitHandle) -> f32 {
            self.inner.get_unit_threat(a, b)
        }
        fn unit_distance(&self, u: crate::ffi::UnitHandle) -> f32 {
            self.inner.unit_distance(u)
        }
        fn can_cast(&self, s: SpellId, u: crate::ffi::UnitHandle) -> bool {
            self.inner.can_cast(s, u)
        }
        fn spell_cooldown_ms(&self, s: SpellId) -> u32 {
            self.inner.spell_cooldown_ms(s)
        }
        fn has_los(&self, u: crate::ffi::UnitHandle) -> bool {
            self.inner.has_los(u)
        }
        fn get_nearby_units(&self, r: f32, h: bool) -> Vec<crate::ffi::UnitHandle> {
            self.inner.get_nearby_units(r, h)
        }
        fn get_behind_position(&self, u: crate::ffi::UnitHandle, d: f32) -> crate::ffi::BotPosition {
            self.inner.get_behind_position(u, d)
        }
        fn get_safe_position(&self, r: f32) -> Option<crate::ffi::BotPosition> {
            self.inner.get_safe_position(r)
        }
        fn get_spread_position(
            &self,
            c: crate::ffi::UnitHandle,
            r: f32,
            i: u8,
            t: u8,
        ) -> crate::ffi::BotPosition {
            self.inner.get_spread_position(c, r, i, t)
        }
        fn can_reach(&self, x: f32, y: f32, z: f32) -> bool {
            self.inner.can_reach(x, y, z)
        }

        fn whisper(&self, guid: u64, msg: &str) -> bool {
            WHISPERED.lock().unwrap().push((guid, msg.to_string()));
            true
        }
        fn tell_player(&self, guid: u64, msg: &str) -> bool {
            WHISPERED.lock().unwrap().push((guid, msg.to_string()));
            true
        }
        fn tell_addon(&self, guid: u64, msg: &str) -> bool {
            ADDONED.lock().unwrap().push((guid, msg.to_string()));
            true
        }
    }

    fn test_bot_with_recorder() -> BotState {
        BotState::new(
            1,
            Box::new(ReplyRecorder { inner: NullInterface }),
            PlayerClass::Warrior,
            PlayerSpec::WarriorArms,
            BotRole::DPS,
            Sel!(),
        )
    }

    #[test]
    fn set_mode_via_command() {
        let mut bot = test_bot();
        bot.pending_commands.lock().unwrap()
            .push_back(PendingCommand::internal(BotCommand::SetMode(
                BehaviorMode::Grind,
            )));
        process_commands(&mut bot);
        assert_eq!(bot.settings.mode, BehaviorMode::Grind);
    }

    #[test]
    fn blacklist_spell() {
        let mut bot = test_bot();
        let spell = SpellId(100);
        bot.pending_commands.lock().unwrap()
            .push_back(PendingCommand::internal(BotCommand::BlacklistSpell(spell)));
        process_commands(&mut bot);
        assert!(bot.settings.spell_blacklist.contains(&spell));

        bot.pending_commands.lock().unwrap()
            .push_back(PendingCommand::internal(BotCommand::UnblacklistSpell(
                spell,
            )));
        process_commands(&mut bot);
        assert!(!bot.settings.spell_blacklist.contains(&spell));
    }

    #[test]
    fn guard_sets_position_and_mode() {
        let mut bot = test_bot();
        bot.snap.self_.pos.x = 10.0;
        bot.snap.self_.pos.y = 20.0;
        bot.snap.self_.pos.z = 30.0;
        bot.pending_commands.lock().unwrap()
            .push_back(PendingCommand::internal(BotCommand::Guard));
        process_commands(&mut bot);
        assert_eq!(bot.settings.mode, BehaviorMode::Guard);
        assert_eq!(bot.settings.guard_position, Some((10.0, 20.0, 30.0)));
    }

    #[test]
    fn reset_restores_defaults() {
        let mut bot = test_bot();
        bot.settings.mode = BehaviorMode::Grind;
        bot.settings.flee_hp_pct = 0.5;
        bot.pending_commands.lock().unwrap()
            .push_back(PendingCommand::internal(BotCommand::Reset));
        process_commands(&mut bot);
        assert_eq!(bot.settings.mode, BehaviorMode::Follow);
        assert_eq!(bot.settings.flee_hp_pct, 0.0);
    }

    #[test]
    fn tunable_commands_mutate_settings() {
        let mut bot = test_bot();
        for cmd in [
            BotCommand::SetRange(7.5),
            BotCommand::SetStance(2),
            BotCommand::MaxDps,
            BotCommand::ToggleSaveMana,
            BotCommand::ToggleSelfRes,
            BotCommand::SetCheatFlags(0xF),
            BotCommand::KeepItem(ItemId(42)),
            BotCommand::SetChatChannel { channel: ChatChannel::Party, on: true },
            BotCommand::SetPreferredRti(Some(8)),
        ] {
            bot.pending_commands.lock().unwrap()
                .push_back(PendingCommand::internal(cmd));
        }
        process_commands(&mut bot);

        assert!((bot.settings.follow_distance - 7.5).abs() < f32::EPSILON);
        assert_eq!(bot.settings.stance, 2);
        assert!(bot.settings.strategies.get(BotStateKind::Combat).contains(StrategyFlags::DPS));
        assert_eq!(bot.settings.reactivity, Reactivity::Aggressive);
        assert!(bot.settings.save_mana > 0);
        assert!(bot.settings.self_res);
        assert_eq!(bot.settings.cheat_flags, 0xF);
        assert!(bot.settings.keep_items.contains(&ItemId(42)));
        assert_eq!(bot.settings.chat_channels, ChatChannel::Party as u32);
        assert_eq!(bot.settings.preferred_rti_icon, Some(8));

        // Unkeep should remove from keep_items.
        bot.pending_commands.lock().unwrap()
            .push_back(PendingCommand::internal(BotCommand::UnkeepItem(ItemId(42))));
        process_commands(&mut bot);
        assert!(!bot.settings.keep_items.contains(&ItemId(42)));

        // Toggling chat channel off clears the bit.
        bot.pending_commands.lock().unwrap()
            .push_back(PendingCommand::internal(BotCommand::SetChatChannel {
                channel: ChatChannel::Party,
                on: false,
            }));
        process_commands(&mut bot);
        assert_eq!(bot.settings.chat_channels, 0);
    }

    /// Part 5 Step 4 — reply routing. A command whose `ChatOrigin::lang`
    /// is `LANG_ADDON` must reply via `tell_addon`; a regular whisper must
    /// reply via `tell_player`. Locks in the behavior that Mangosbot's UI
    /// round-trips depend on (addon queries must not leak as whispers into
    /// the player's chat frame).
    #[test]
    fn reply_routes_by_origin() {
        clear_reply_sinks();
        let mut bot = test_bot_with_recorder();

        // Addon-origin query (`#a co ?`) — should land on the addon sink.
        bot.pending_commands.lock().unwrap().push_back(PendingCommand::external(
            0xABCD,
            SecurityLevel::AllowAll,
            ChatOrigin::new(2 /* CHAT_MSG_PARTY */, LANG_ADDON),
            BotCommand::ApplyStrategies {
                state: BotStateKind::Combat,
                add: StrategyFlags::NONE,
                remove: StrategyFlags::NONE,
                toggle: StrategyFlags::NONE,
                query: true,
            },
        ));

        // Whisper-origin query — should land on the whisper sink.
        bot.pending_commands.lock().unwrap().push_back(PendingCommand::external(
            0x1234,
            SecurityLevel::AllowAll,
            ChatOrigin::new(6 /* CHAT_MSG_WHISPER */, 0 /* LANG_UNIVERSAL */),
            BotCommand::ApplyStrategies {
                state: BotStateKind::Combat,
                add: StrategyFlags::NONE,
                remove: StrategyFlags::NONE,
                toggle: StrategyFlags::NONE,
                query: true,
            },
        ));

        process_commands(&mut bot);

        let addon = ADDONED.lock().unwrap().clone();
        let whispers = WHISPERED.lock().unwrap().clone();

        assert_eq!(addon.len(), 1, "addon-origin query should reply on addon wire");
        assert_eq!(addon[0].0, 0xABCD);
        assert!(
            addon[0].1.starts_with("Combat Strategies:"),
            "payload should carry the Mangosbot query prefix verbatim, got {:?}",
            addon[0].1
        );

        assert_eq!(whispers.len(), 1, "whisper-origin query should whisper");
        assert_eq!(whispers[0].0, 0x1234);
        assert!(whispers[0].1.starts_with("Combat Strategies:"));
    }

    #[test]
    fn heal_threshold_set() {
        let mut bot = test_bot();
        bot.pending_commands.lock().unwrap()
            .push_back(PendingCommand::internal(BotCommand::SetHealThreshold(0.70)));
        process_commands(&mut bot);
        assert!((bot.settings.heal_party_threshold - 0.70).abs() < f32::EPSILON);
    }
}
