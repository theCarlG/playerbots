/// Bot command system — parse chat commands and apply to bot settings.
///
/// Commands arrive as whispers from the master player, parsed by `parser::parse()`,
/// queued on `BotState::pending_commands`, and applied here before each tick.

pub mod parser;

use crate::bot::settings::{
    BehaviorMode, BotSettings, CombatOrder, Reactivity, RtscAction, StrategyFlags,
};
use crate::bot::state::BotState;
use crate::ffi::{SpellId, UnitHandle};

/// All bot commands, parsed from chat text.
#[derive(Debug, Clone, PartialEq)]
pub enum BotCommand {
    // -- Mode commands --
    SetMode(BehaviorMode),
    SetCombatOrder(CombatOrder),
    /// Additive/subtractive combat order edit (`co +tank -fury`).
    ApplyCombatOrder { add: CombatOrder, remove: CombatOrder },
    /// Additive/subtractive strategy toggles (`nc +rtsc,-rpg bg`).
    ApplyStrategies  { add: StrategyFlags, remove: StrategyFlags },
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
    CastOne { spell: SpellId, on_self: bool },
    /// Set follower formation style.
    SetFormation(crate::bot::settings::FollowFormation),

    /// Travel to a named location from `data::named_locations`.
    /// Writes the destination to the blackboard; the travel subtree consumes it.
    TravelTo(&'static crate::data::named_locations::NamedLocation),

    // -- Unknown --
    Unknown(String),
}

/// A command queued for execution, tagged with its sender and trust level.
///
/// `sender` is the ObjectGuid of the player who issued the whisper, or
/// `None` for internal/system-injected commands (RTSC spell positions,
/// tests). `privileged` is true if the sender is the bot's owner, party
/// leader, or a GM (decided C++-side in `PlayerbotRust::HandleCommand`).
/// Non-privileged senders are silently ignored.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingCommand {
    pub sender:     Option<u64>,
    pub privileged: bool,
    pub command:    BotCommand,
}

impl PendingCommand {
    /// Internal/system command — always allowed, no whisper reply possible.
    pub fn internal(command: BotCommand) -> Self {
        Self { sender: None, privileged: true, command }
    }

    /// Command from a specific player with a trust level.
    pub fn external(sender: u64, privileged: bool, command: BotCommand) -> Self {
        Self { sender: Some(sender), privileged, command }
    }
}

/// Process all pending commands on a bot, mutating its settings.
/// Called once per tick before the BT runs.
pub fn process_commands(bot: &mut BotState) {
    while let Some(pc) = bot.pending_commands.pop_front() {
        if !pc.privileged {
            // Silently drop non-owner commands. A verbose reply would
            // create a whisper-spam vector from non-owners.
            continue;
        }
        apply_command(bot, &pc);
    }
}

/// The crowd-control spell this class has available for `cc {icon}` commands.
///
/// The spell is always *attempted* — `can_cast` handles rank, LoS, range,
/// creature-type immunity, already-CC'd, etc. Returns `None` for classes
/// with no direct single-target CC (warrior, DK, rogue uses stealth-only
/// sap so it's conditional).
fn class_cc_spell(class: crate::bot::state::PlayerClass) -> Option<SpellId> {
    use crate::bot::state::PlayerClass::*;
    Some(match class {
        Mage    => SpellId(118),    // polymorph
        Warlock => SpellId(710),    // banish (works on demons/elementals)
        Priest  => SpellId(605),    // mind control — in practice shackle undead
        Druid   => SpellId(2637),   // hibernate (beast/dragonkin only; fall back silently)
        Hunter  => SpellId(1499),   // freezing trap
        Paladin => SpellId(20066),  // repentance (retri talent; may fail silently)
        Shaman  => SpellId(8034),   // frostbrand … no single-target CC; hex is wotlk
        Rogue   => SpellId(6770),   // sap (stealth-only; can_cast will refuse otherwise)
        Warrior | DeathKnight => return None,
    })
}

/// Reply to the sender of `pc` — whisper if external, say if internal.
fn reply(bot: &BotState, pc: &PendingCommand, msg: &str) {
    match pc.sender {
        Some(guid) => { bot.interface.whisper(guid, msg); }
        None       => { bot.interface.say(msg, 0); }
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
            if let Some(unit) = bot.interface.get_unit_with_raid_icon(*icon) {
                if let Some(spell) = class_cc_spell(bot.class) {
                    if bot.interface.can_cast(spell, unit) {
                        bot.interface.cast_spell(spell, unit);
                    }
                }
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
        BotCommand::Repair | BotCommand::Vendor => {
            // These set a one-shot action flag. World behavior modules
            // will check for it. For now, just acknowledge.
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
                reply(bot, pc, &format!(
                    "Cannot travel to {} from this map (need map {}).",
                    loc.name, loc.map
                ));
            }
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
    use crate::engine::context::tests::NullInterface;
    use crate::bot::state::{PlayerClass, PlayerSpec};
    use crate::ffi::BotRole;
    use crate::engine::bt::Bt;

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
        bot.pending_commands.push_back(PendingCommand::internal(BotCommand::SetMode(BehaviorMode::Grind)));
        process_commands(&mut bot);
        assert_eq!(bot.settings.mode, BehaviorMode::Grind);
    }

    #[test]
    fn blacklist_spell() {
        let mut bot = test_bot();
        let spell = SpellId(100);
        bot.pending_commands.push_back(PendingCommand::internal(BotCommand::BlacklistSpell(spell)));
        process_commands(&mut bot);
        assert!(bot.settings.spell_blacklist.contains(&spell));

        bot.pending_commands.push_back(PendingCommand::internal(BotCommand::UnblacklistSpell(spell)));
        process_commands(&mut bot);
        assert!(!bot.settings.spell_blacklist.contains(&spell));
    }

    #[test]
    fn guard_sets_position_and_mode() {
        let mut bot = test_bot();
        bot.snap.self_.pos.x = 10.0;
        bot.snap.self_.pos.y = 20.0;
        bot.snap.self_.pos.z = 30.0;
        bot.pending_commands.push_back(PendingCommand::internal(BotCommand::Guard));
        process_commands(&mut bot);
        assert_eq!(bot.settings.mode, BehaviorMode::Guard);
        assert_eq!(bot.settings.guard_position, Some((10.0, 20.0, 30.0)));
    }

    #[test]
    fn reset_restores_defaults() {
        let mut bot = test_bot();
        bot.settings.mode = BehaviorMode::Grind;
        bot.settings.flee_hp_pct = 0.5;
        bot.pending_commands.push_back(PendingCommand::internal(BotCommand::Reset));
        process_commands(&mut bot);
        assert_eq!(bot.settings.mode, BehaviorMode::Follow);
        assert_eq!(bot.settings.flee_hp_pct, 0.0);
    }

    #[test]
    fn heal_threshold_set() {
        let mut bot = test_bot();
        bot.pending_commands.push_back(PendingCommand::internal(BotCommand::SetHealThreshold(0.70)));
        process_commands(&mut bot);
        assert!((bot.settings.heal_party_threshold - 0.70).abs() < f32::EPSILON);
    }
}
