/// Bot command system — parse chat commands and apply to bot settings.
///
/// Commands arrive as whispers from the master player, parsed by `parser::parse()`,
/// queued on `BotState::pending_commands`, and applied here before each tick.
pub mod parser;
pub mod preprocess;

use crate::bot::class_prefs::{
    HunterAspect, HunterTrap, PaladinAura, PaladinBlessing, PoisonKind, ShamanImbue, TotemRole,
    TotemSlot, WarlockCurse, WarriorStance, WeaponHand,
};
use crate::bot::settings::{
    BehaviorMode, BotSettings, ChatChannel, CombatOrder, Reactivity, RtscAction, StrategyFlags,
};
use crate::bot::state::BotState;
use crate::ffi::{ItemId, SpellId, UnitHandle};

/// All bot commands, parsed from chat text.
#[derive(Debug, Clone, PartialEq)]
pub enum BotCommand {
    // -- Mode commands --
    SetMode(BehaviorMode),
    SetCombatOrder(CombatOrder),
    /// Additive/subtractive combat order edit (`co +tank -fury`).
    /// `query` is `true` when the original command ended with `,?` —
    /// Mangosbot uses that form to both apply *and* re-query the flags in
    /// one round-trip, so the handler whispers the current state after
    /// applying.
    ApplyCombatOrder {
        add: CombatOrder,
        remove: CombatOrder,
        query: bool,
    },
    /// Additive/subtractive strategy toggles (`nc +rtsc,-rpg bg`). See
    /// [`ApplyCombatOrder::query`] for the `,?` trailing-query semantics.
    ApplyStrategies {
        add: StrategyFlags,
        remove: StrategyFlags,
        query: bool,
    },
    /// Reset strategies back to the default loadout (`reset ai`).
    ResetStrategies,
    SetReactivity(Reactivity),

    // -- Targeting --
    Focus(Option<UnitHandle>),
    Attack(Option<UnitHandle>),
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
    /// Set follower formation style.
    SetFormation(crate::bot::settings::FollowFormation),

    /// Travel to a named location from `data::named_locations`.
    /// Writes the destination to the blackboard; the travel subtree consumes it.
    TravelTo(&'static crate::data::named_locations::NamedLocation),

    // -- Tunables --
    /// `range <N>` — override follow distance.
    SetRange(f32),
    /// `stance <N>` — warrior stance (1/2/3, warrior-only).
    SetStance(u8),
    /// `max dps` — DPS combat order + aggressive reactivity shortcut.
    MaxDps,
    /// `save mana` toggle.
    ToggleSaveMana,
    /// `save mana on|off` — explicit set (Mangosbot addon sends both forms).
    SetSaveMana(bool),
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
    /// Reply with the current hunter aspect / trap loadout.
    ShowHunterPrefs,

    /// `curse agony` / `curse none` — set warlock default curse.
    SetWarlockCurse(Option<WarlockCurse>),
    /// Reply with the current warlock curse.
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
    /// `co ?` — whisper current combat-order flags.
    QueryCombatOrder,
    /// `nc ?` — whisper current strategy flags.
    QueryStrategies,
    /// `react ?` — whisper current reactivity level.
    QueryReactivity,
    /// `rti ?` — whisper current preferred raid target icon.
    QueryRti,
    /// `rti cc ?` — whisper current preferred CC raid target icon.
    QueryCcRti,
    /// `save mana ?` — whisper current save-mana toggle state.
    QuerySaveMana,
    /// `ll ?` — whisper current loot policy.
    QueryLootPolicy,

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
            | QueryFormation | QueryStance | QueryCombatOrder | QueryStrategies
            | QueryReactivity | QueryRti | QueryCcRti | QuerySaveMana | QueryLootPolicy => SecurityLevel::Talk,

            // Destructive / account-level — master only.
            Reset | ResetStrategies | BlacklistSpell(_) | UnblacklistSpell(_)
            | SetCheatFlags(_) | GuildLeave => SecurityLevel::AllowAll,

            // Everything else — group members.
            SetMode(_)
            | SetCombatOrder(_)
            | ApplyCombatOrder { .. }
            | ApplyStrategies { .. }
            | SetReactivity(_)
            | Focus(_)
            | Attack(_)
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
            | Repair
            | Vendor
            | SetHealThreshold(_)
            | Mount
            | Resurrect
            | Flee
            | Free
            | Summon
            | CastOne { .. }
            | SetFormation(_)
            | TravelTo(_)
            | SetRange(_)
            | SetStance(_)
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
            | ShowHunterPrefs
            | SetWarlockCurse(_)
            | ShowWarlockPrefs
            | SetWarriorForcedStance(_)
            | ShowWarriorPrefs
            | SetSuppressionDuty(_)
            | SetDouseDuty(_)
            | ShowEncounterPrefs
            | ApplyLootPolicy { .. } => SecurityLevel::Invite,
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

/// A command queued for execution, tagged with its sender and trust level.
///
/// `sender` is the `ObjectGuid` of the player who issued the chat, or
/// `None` for internal/system-injected commands (RTSC spell positions,
/// tests). `security` is the tier that C++-side `PlayerbotRust::
/// ComputeSenderSecurity` granted the sender. The dispatcher compares it
/// to each command's `required_security` and drops commands that don't
/// meet the bar.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingCommand {
    pub sender: Option<u64>,
    pub security: SecurityLevel,
    pub command: BotCommand,
}

impl PendingCommand {
    /// Internal/system command — always allowed, no whisper reply possible.
    pub fn internal(command: BotCommand) -> Self {
        Self {
            sender: None,
            security: SecurityLevel::AllowAll,
            command,
        }
    }

    /// Command from a specific player with a trust tier.
    pub fn external(sender: u64, security: SecurityLevel, command: BotCommand) -> Self {
        Self {
            sender: Some(sender),
            security,
            command,
        }
    }
}

/// Process all pending commands on a bot, mutating its settings.
/// Called once per tick before the BT runs.
pub fn process_commands(bot: &mut BotState) {
    while let Some(pc) = bot.pending_commands.pop_front() {
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

/// Reply to the sender of `pc`, matching PB2's `TellPlayerNoFacing`
/// routing: if the bot is in a group, broadcast to PARTY/RAID so every
/// group member sees the response; otherwise whisper the sender. The
/// C++ bridge's `tell_player` callback encapsulates that rule.
///
/// Internal/system-injected commands (`pc.sender == None`) fall back to
/// `say` so anything the bot utters without a requester still goes out
/// on a real channel.
fn reply(bot: &BotState, pc: &PendingCommand, msg: &str) {
    match pc.sender {
        Some(guid) => {
            bot.interface.tell_player(guid, msg);
        }
        None => {
            bot.interface.say(msg, 0);
        }
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
    let cmd = &pc.command;
    let s = &mut bot.settings;
    match cmd {
        BotCommand::SetMode(mode) => {
            s.mode = *mode;
            let verbose = s.verbose;
            let label = mode.as_str();
            if verbose {
                reply(bot, pc, &format!("Mode: {label}"));
            }
        }
        BotCommand::SetCombatOrder(order) => {
            s.combat_order = *order;
        }
        BotCommand::ApplyCombatOrder { add, remove, query } => {
            s.combat_order.remove(*remove);
            s.combat_order.insert(*add);
            if *query {
                let msg = format!("Combat Strategies: {}", s.combat_order.describe());
                reply(bot, pc, &msg);
            }
        }
        BotCommand::ApplyStrategies { add, remove, query } => {
            s.strategies.remove(*remove);
            s.strategies.insert(*add);
            if *query {
                let msg = format!("Non Combat Strategies: {}", s.strategies.describe());
                reply(bot, pc, &msg);
            }
        }
        BotCommand::ResetStrategies => {
            s.strategies = StrategyFlags::defaults();
        }
        BotCommand::SetReactivity(level) => {
            s.reactivity = *level;
        }
        BotCommand::Focus(target) => {
            // If no target provided, use current target from snapshot.
            s.focus_target = target.or_else(|| {
                let t = bot.snap.self_.current_target;
                if t != 0 { Some(t) } else { None }
            });
        }
        BotCommand::Attack(target) => {
            let t = target.or_else(|| {
                let t = bot.snap.self_.current_target;
                if t != 0 { Some(t) } else { None }
            });
            if let Some(unit) = t {
                bot.interface.attack(unit);
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

        // -- RTSC commands --
        BotCommand::RtscSelect => {
            s.rtsc_selected = true;
            s.rtsc_pending_action = None;
        }
        BotCommand::RtscCancel => {
            s.rtsc_selected = false;
            s.rtsc_pending_action = None;
        }
        BotCommand::RtscToggle => {
            s.rtsc_selected = !s.rtsc_selected;
            if !s.rtsc_selected {
                s.rtsc_pending_action = None;
            }
        }
        BotCommand::RtscMove => {
            s.rtsc_pending_action = Some(RtscAction::Move { exact: false });
        }
        BotCommand::RtscMoveExact => {
            s.rtsc_pending_action = Some(RtscAction::Move { exact: true });
        }
        BotCommand::RtscSaveHere(name) => {
            let pos = bot.snap.self_.pos;
            s.rtsc_waypoints.insert(name.clone(), (pos.x, pos.y, pos.z));
        }
        BotCommand::RtscSave(name) => {
            s.rtsc_pending_action = Some(RtscAction::Save { name: name.clone() });
        }
        BotCommand::RtscUnsave(name) => {
            s.rtsc_waypoints.remove(name);
        }
        BotCommand::RtscGo(name) => {
            if let Some(&(x, y, z)) = s.rtsc_waypoints.get(name) {
                bot.interface.move_to(x, y, z);
                s.guard_position = Some((x, y, z));
                s.mode = BehaviorMode::Guard;
            }
        }
        BotCommand::RtscShow => {
            let msg = if s.rtsc_waypoints.is_empty() {
                "No saved waypoints.".to_string()
            } else {
                let names: Vec<&str> = s.rtsc_waypoints.keys().map(|s| s.as_str()).collect();
                format!("Waypoints: {}", names.join(", "))
            };
            reply(bot, pc, &msg);
        }
        BotCommand::RtscSpellPosition(x, y, z) => {
            // This is triggered when spell 30758 lands on a position.
            // What happens depends on the pending RTSC action.
            match s.rtsc_pending_action.take() {
                Some(RtscAction::Move { exact: _ }) => {
                    bot.interface.move_to(*x, *y, *z);
                    s.guard_position = Some((*x, *y, *z));
                    s.mode = BehaviorMode::Guard;
                }
                Some(RtscAction::Save { name }) => {
                    s.rtsc_waypoints.insert(name, (*x, *y, *z));
                }
                None => {
                    // No pending action — if selected, default to move.
                    if s.rtsc_selected {
                        bot.interface.move_to(*x, *y, *z);
                        s.guard_position = Some((*x, *y, *z));
                        s.mode = BehaviorMode::Guard;
                    }
                }
            }
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
                "Mode:{} CO:{:#x} React:{:?} HP:{:.0}% MP:{:.0}%",
                s.mode.as_str(),
                s.combat_order.0,
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
            *s = BotSettings::default();
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
                bot.snap.self_.current_target
            };
            if target != 0 {
                bot.interface.cast_spell(*spell, target);
            }
        }
        BotCommand::SetFormation(f) => {
            s.follow_formation = *f;
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
        BotCommand::SetStance(st) => {
            s.stance = *st;
        }
        BotCommand::MaxDps => {
            s.combat_order = CombatOrder::DPS;
            s.reactivity = Reactivity::Aggressive;
        }
        BotCommand::ToggleSaveMana => {
            s.save_mana = !s.save_mana;
            let verbose = s.verbose;
            let now = s.save_mana;
            if verbose {
                // Mangosbot parses `Mana save level set: <val>` (line 3397).
                reply(
                    bot,
                    pc,
                    if now {
                        "Mana save level set: on"
                    } else {
                        "Mana save level set: off"
                    },
                );
            }
        }
        BotCommand::SetSaveMana(on) => {
            s.save_mana = *on;
            let verbose = s.verbose;
            let now = s.save_mana;
            if verbose {
                // Mangosbot parses `Mana save level set: <val>` (line 3397).
                reply(
                    bot,
                    pc,
                    if now {
                        "Mana save level set: on"
                    } else {
                        "Mana save level set: off"
                    },
                );
            }
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
                "DBG mode={} co={:#x} react={:?} strats={:#x} cheat={:#x}",
                s.mode.as_str(),
                s.combat_order.0,
                s.reactivity,
                s.strategies.0,
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
                if bot.settings.verbose {
                    let label = kind.map_or("cleared", |k| k.as_str());
                    reply(bot, pc, &format!("poison {}: {label}", hand.as_str()));
                }
            } else if bot.settings.verbose {
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
                if bot.settings.verbose {
                    reply(
                        bot,
                        pc,
                        &format!("totem: {} is not a {} totem", r.as_str(), slot.as_str()),
                    );
                }
            } else if let Some(sh) = s.class_prefs.as_shaman_mut() {
                sh.set(*slot, *role);
                if bot.settings.verbose {
                    let label = role.map_or("cleared", |r| r.as_str());
                    reply(bot, pc, &format!("totem {}: {label}", slot.as_str()));
                }
            } else if bot.settings.verbose {
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
                if bot.settings.verbose {
                    let label = imbue.map_or("cleared", |i| i.as_str());
                    reply(bot, pc, &format!("imbue {}: {label}", hand.as_str()));
                }
            } else if bot.settings.verbose {
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
                if bot.settings.verbose {
                    let label = aura.map_or("cleared", |a| a.as_str());
                    reply(bot, pc, &format!("aura: {label}"));
                }
            } else if bot.settings.verbose {
                reply(bot, pc, "aura: not a paladin");
            }
        }
        BotCommand::SetPaladinBlessing(blessing) => {
            if let Some(p) = s.class_prefs.as_paladin_mut() {
                p.blessing = *blessing;
                if bot.settings.verbose {
                    let label = blessing.map_or("cleared", |b| b.as_str());
                    reply(bot, pc, &format!("blessing: {label}"));
                }
            } else if bot.settings.verbose {
                reply(bot, pc, "blessing: not a paladin");
            }
        }
        BotCommand::SetPaladinGreaterBlessing(flag) => {
            if let Some(p) = s.class_prefs.as_paladin_mut() {
                p.use_greater = *flag;
                if bot.settings.verbose {
                    reply(
                        bot,
                        pc,
                        &format!("greater blessing: {}", if *flag { "on" } else { "off" }),
                    );
                }
            } else if bot.settings.verbose {
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
                if bot.settings.verbose {
                    let label = aspect.map_or("cleared", |a| a.as_str());
                    reply(bot, pc, &format!("aspect: {label}"));
                }
            } else if bot.settings.verbose {
                reply(bot, pc, "aspect: not a hunter");
            }
        }
        BotCommand::SetHunterTrap(trap) => {
            if let Some(h) = s.class_prefs.as_hunter_mut() {
                h.trap = *trap;
                if bot.settings.verbose {
                    let label = trap.map_or("cleared", |t| t.as_str());
                    reply(bot, pc, &format!("trap: {label}"));
                }
            } else if bot.settings.verbose {
                reply(bot, pc, "trap: not a hunter");
            }
        }
        BotCommand::ShowHunterPrefs => {
            let msg = match s.class_prefs.as_hunter() {
                Some(h) => format!(
                    "hunter: aspect={} trap={}",
                    h.aspect.map_or("none", |a| a.as_str()),
                    h.trap.map_or("none", |t| t.as_str()),
                ),
                None => "hunter: not a hunter".into(),
            };
            reply(bot, pc, &msg);
        }

        BotCommand::SetWarlockCurse(curse) => {
            if let Some(w) = s.class_prefs.as_warlock_mut() {
                w.curse = *curse;
                if bot.settings.verbose {
                    let label = curse.map_or("cleared", |c| c.as_str());
                    reply(bot, pc, &format!("curse: {label}"));
                }
            } else if bot.settings.verbose {
                reply(bot, pc, "curse: not a warlock");
            }
        }
        BotCommand::ShowWarlockPrefs => {
            let msg = match s.class_prefs.as_warlock() {
                Some(w) => format!("warlock: curse={}", w.curse.map_or("none", |c| c.as_str())),
                None => "warlock: not a warlock".into(),
            };
            reply(bot, pc, &msg);
        }

        BotCommand::SetWarriorForcedStance(stance) => {
            if let Some(w) = s.class_prefs.as_warrior_mut() {
                w.forced_stance = *stance;
                if bot.settings.verbose {
                    let label = stance.map_or("cleared", |st| st.as_str());
                    reply(bot, pc, &format!("forcestance: {label}"));
                }
            } else if bot.settings.verbose {
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
            if bot.settings.verbose {
                reply(bot, pc, &format!("suppression: {}", mode.as_word()));
            }
        }
        BotCommand::SetDouseDuty(mode) => {
            bot.settings.encounter_prefs.douse_duty = *mode;
            if bot.settings.verbose {
                reply(bot, pc, &format!("douse: {}", mode.as_word()));
            }
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
        }

        // -- Query replies ---------------------------------------------
        BotCommand::QueryFormation => {
            // Mangosbot parses `Formation: <name>` (line 3391 in Mangosbot.lua).
            let msg = format!("Formation: {}", s.follow_formation.as_str());
            reply(bot, pc, &msg);
        }
        BotCommand::QueryStance => {
            let name = match s.stance {
                1 => "battle",
                2 => "defensive",
                3 => "berserker",
                _ => "none",
            };
            // Mangosbot parses `Stance: <name>` (line 3394).
            let msg = format!("Stance: {name}");
            reply(bot, pc, &msg);
        }
        BotCommand::QueryCombatOrder => {
            // Mangosbot parses `Combat Strategies: ...` (line 3358, trim=19).
            let msg = format!("Combat Strategies: {}", s.combat_order.describe());
            reply(bot, pc, &msg);
        }
        BotCommand::QueryStrategies => {
            // Mangosbot parses `Non Combat Strategies: ...` (line 3362, trim=23).
            let msg = format!("Non Combat Strategies: {}", s.strategies.describe());
            reply(bot, pc, &msg);
        }
        BotCommand::QueryReactivity => {
            let msg = format!("react: {}", s.reactivity.as_str());
            reply(bot, pc, &msg);
        }
        BotCommand::QueryRti => {
            let msg = format!("rti: {}", rti_icon_name(s.preferred_rti_icon));
            reply(bot, pc, &msg);
        }
        BotCommand::QueryCcRti => {
            let msg = format!("rti cc: {}", rti_icon_name(s.preferred_cc_rti_icon));
            reply(bot, pc, &msg);
        }
        BotCommand::QuerySaveMana => {
            // Mangosbot parses `Mana save level: <val>` (line 3400).
            let msg = if s.save_mana {
                "Mana save level: on"
            } else {
                "Mana save level: off"
            };
            reply(bot, pc, msg);
        }
        BotCommand::QueryLootPolicy => {
            // Mangosbot parses `Loot strategy: <val>` (line 3403).
            let msg = format!("Loot strategy: {}", s.loot_policy.describe());
            reply(bot, pc, &msg);
        }

        BotCommand::Unknown(text) => {
            let msg = format!("Unknown command: {text}");
            reply(bot, pc, &msg);
        }
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

    #[test]
    fn set_mode_via_command() {
        let mut bot = test_bot();
        bot.pending_commands
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
        bot.pending_commands
            .push_back(PendingCommand::internal(BotCommand::BlacklistSpell(spell)));
        process_commands(&mut bot);
        assert!(bot.settings.spell_blacklist.contains(&spell));

        bot.pending_commands
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
        bot.pending_commands
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
        bot.pending_commands
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
            bot.pending_commands
                .push_back(PendingCommand::internal(cmd));
        }
        process_commands(&mut bot);

        assert!((bot.settings.follow_distance - 7.5).abs() < f32::EPSILON);
        assert_eq!(bot.settings.stance, 2);
        assert_eq!(bot.settings.combat_order, CombatOrder::DPS);
        assert_eq!(bot.settings.reactivity, Reactivity::Aggressive);
        assert!(bot.settings.save_mana);
        assert!(bot.settings.self_res);
        assert_eq!(bot.settings.cheat_flags, 0xF);
        assert!(bot.settings.keep_items.contains(&ItemId(42)));
        assert_eq!(bot.settings.chat_channels, ChatChannel::Party as u32);
        assert_eq!(bot.settings.preferred_rti_icon, Some(8));

        // Unkeep should remove from keep_items.
        bot.pending_commands
            .push_back(PendingCommand::internal(BotCommand::UnkeepItem(ItemId(42))));
        process_commands(&mut bot);
        assert!(!bot.settings.keep_items.contains(&ItemId(42)));

        // Toggling chat channel off clears the bit.
        bot.pending_commands
            .push_back(PendingCommand::internal(BotCommand::SetChatChannel {
                channel: ChatChannel::Party,
                on: false,
            }));
        process_commands(&mut bot);
        assert_eq!(bot.settings.chat_channels, 0);
    }

    #[test]
    fn heal_threshold_set() {
        let mut bot = test_bot();
        bot.pending_commands
            .push_back(PendingCommand::internal(BotCommand::SetHealThreshold(0.70)));
        process_commands(&mut bot);
        assert!((bot.settings.heal_party_threshold - 0.70).abs() < f32::EPSILON);
    }
}
