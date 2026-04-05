/// Bot command system — parse chat commands and apply to bot settings.
///
/// Commands arrive as whispers from the master player, parsed by `parser::parse()`,
/// queued on `BotState::pending_commands`, and applied here before each tick.
pub mod parser;

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
    ApplyCombatOrder {
        add: CombatOrder,
        remove: CombatOrder,
    },
    /// Additive/subtractive strategy toggles (`nc +rtsc,-rpg bg`).
    ApplyStrategies {
        add: StrategyFlags,
        remove: StrategyFlags,
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
            | ListQuests | ListTalents | ListSpells | ListReputation | ListSkills => {
                SecurityLevel::Talk
            }

            // Destructive / account-level — master only.
            Reset | ResetStrategies | BlacklistSpell(_) | UnblacklistSpell(_)
            | SetCheatFlags(_) => SecurityLevel::AllowAll,

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
            | ToggleSelfRes
            | KeepItem(_)
            | UnkeepItem(_)
            | SetChatChannel { .. }
            | SetPreferredRti(_)
            | Emote(_)
            | ReleaseSpirit
            | AcceptRevive
            | Jump
            | UseHearth
            | QuestAccept
            | QuestDrop(_) => SecurityLevel::Invite,
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

/// Reply to the sender of `pc` — whisper if external, say if internal.
fn reply(bot: &BotState, pc: &PendingCommand, msg: &str) {
    match pc.sender {
        Some(guid) => {
            bot.interface.whisper(guid, msg);
        }
        None => {
            bot.interface.say(msg, 0);
        }
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
        BotCommand::ApplyCombatOrder { add, remove } => {
            s.combat_order.remove(*remove);
            s.combat_order.insert(*add);
        }
        BotCommand::ApplyStrategies { add, remove } => {
            s.strategies.remove(*remove);
            s.strategies.insert(*add);
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
        BotCommand::Mount | BotCommand::Resurrect => {
            // Handled by world behavior modules when they exist.
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
            // Meeting-stone summon — world behavior module accepts the summon
            // dialog. No immediate action needed here.
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
                reply(bot, pc, if now { "save mana: on" } else { "save mana: off" });
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
            let msg = format!("Reputations tracked: {}", list.len());
            reply(bot, pc, &msg);
        }
        BotCommand::ListSkills => {
            let list = bot.interface.bot_get_learned_skills();
            let msg = format!("Skills learned: {}", list.len());
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
    use crate::engine::bt::Bt;
    use crate::engine::context::tests::NullInterface;
    use crate::ffi::BotRole;

    fn test_bot() -> BotState {
        BotState::new(
            1,
            Box::new(NullInterface),
            PlayerClass::Warrior,
            PlayerSpec::WarriorArms,
            BotRole::DPS,
            Bt::Sel(vec![]), // dummy empty tree
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
