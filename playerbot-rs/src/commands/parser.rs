/// Chat command parser — converts text into `BotCommand`.
///
/// Commands arrive as whispers from the master player. The C++ side routes
/// them through `playerbot_chat_command()` which calls this parser.
///
/// Design: ~20 clean commands replace the old 70+ redundant C++ commands.
/// Each command maps to exactly one `BotCommand` variant.
use crate::bot::settings::{BehaviorMode, CombatOrder, FollowFormation, Reactivity, StrategyFlags};
use crate::commands::BotCommand;
use crate::data::spells::lookup_spell_by_name;
use crate::ffi::SpellId;

/// Parse a chat message into a `BotCommand`.
/// Returns `None` if the message is not a recognized command.
pub fn parse(text: &str) -> Option<BotCommand> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    let lower = text.to_lowercase();
    let parts: Vec<&str> = lower.split_whitespace().collect();
    let cmd = parts[0];
    let args = &parts[1..];

    match cmd {
        // -- Behavior modes --
        "follow" | "stay" | "grind" | "quest" | "passive" | "rpg" | "bg" => {
            BehaviorMode::from_str(cmd).map(BotCommand::SetMode)
        }

        // -- Combat orders --
        "co" => parse_combat_order(args),

        // -- Non-combat strategy toggles --
        "nc" => parse_strategies(args),

        // -- Reactivity --
        "react" => parse_reactivity(args),

        // -- Targeting --
        "focus" => {
            if args.first().is_some_and(|a| *a == "clear") {
                Some(BotCommand::Focus(None))
            } else {
                Some(BotCommand::Focus(None)) // target from current target
            }
        }
        "attack" => parse_attack(args),
        "pull" => parse_pull(args),
        "cc" => parse_cc(args),

        // -- Movement --
        "come" | "c" => Some(BotCommand::ComeToMe),
        "guard" => Some(BotCommand::Guard),
        "go" => parse_go(args),

        // -- RTSC (Real-Time Strategy Control) --
        "rtsc" => parse_rtsc(args),

        // -- Spell control --
        "blacklist" => parse_spell_id(args).map(BotCommand::BlacklistSpell),
        "unblacklist" => parse_spell_id(args).map(BotCommand::UnblacklistSpell),

        // -- Economy --
        "repair" => Some(BotCommand::Repair),
        "vendor" | "sell" => Some(BotCommand::Vendor),

        // -- Healing --
        "heal" => parse_heal_threshold(args),

        // -- Information --
        "status" | "stats" => Some(BotCommand::Status),
        "settings" => Some(BotCommand::ListSettings),

        // -- Utility --
        "reset" => {
            if args.first().copied() == Some("ai") {
                Some(BotCommand::ResetStrategies)
            } else {
                Some(BotCommand::Reset)
            }
        }
        "mount" | "dismount" => Some(BotCommand::Mount),
        "rez" | "resurrect" => Some(BotCommand::Resurrect),

        // -- Panic / aliases --
        "flee" | "runaway" | "panic" => Some(BotCommand::Flee),
        "free" => Some(BotCommand::Free),
        "summon" => Some(BotCommand::Summon),

        // -- Cast a named spell once (addon sends `cast Taunt`). --
        "cast" => parse_cast(args),

        // -- Formation --
        "formation" => parse_formation(args),

        // -- Named-location travel (`travel stormwind`, `travel orgrimmar`). --
        "travel" | "goto" => parse_travel(args),

        _ => Some(BotCommand::Unknown(text.to_string())),
    }
}

/// Parse `co` arguments.
///
/// Bare form (no sign): `co tank` → full replace with that flag.
/// Signed form: `co +tank -fury`, `co +tank assist,+dps assist` → additive/subtractive edit.
/// Flags are comma- or space-separated; multi-word names (`tank assist`,
/// `dps assist`, `pull back`) are matched greedily.
fn parse_combat_order(args: &[&str]) -> Option<BotCommand> {
    if args.is_empty() {
        return Some(BotCommand::Unknown("co: missing order".into()));
    }

    // Re-join so we can split on commas (the addon sends `co x,y` as one arg chain).
    let joined = args.join(" ");
    let signed = joined.contains('+') || joined.contains('-');

    // Bare form: single flag, full replacement.
    if !signed {
        let tokens: Vec<&str> = joined.split_whitespace().collect();
        return match CombatOrder::parse_flag(&tokens) {
            Some((flag, _)) => Some(BotCommand::SetCombatOrder(flag)),
            None => Some(BotCommand::Unknown(format!("co: unknown flag `{joined}`"))),
        };
    }

    // Signed form: walk tokens, each token starts with +/-, followed by flag name(s).
    let mut add = CombatOrder::NONE;
    let mut remove = CombatOrder::NONE;

    for chunk in joined.split(',') {
        let tokens: Vec<&str> = chunk.split_whitespace().collect();
        let mut i = 0;
        while i < tokens.len() {
            let tok = tokens[i];
            let (sign, rest) = match tok.chars().next() {
                Some('+') => (1i8, &tok[1..]),
                Some('-') => (-1i8, &tok[1..]),
                _ => {
                    return Some(BotCommand::Unknown(format!(
                        "co: expected +/- prefix at `{tok}`"
                    )));
                }
            };
            // Build a small slice starting with the unsigned first word.
            let mut window: Vec<&str> = Vec::with_capacity(2);
            if !rest.is_empty() {
                window.push(rest);
            }
            if let Some(next) = tokens.get(i + 1) {
                // Include the next token only if it doesn't start a new signed flag.
                if !next.starts_with('+') && !next.starts_with('-') {
                    window.push(next);
                }
            }
            match CombatOrder::parse_flag(&window) {
                Some((flag, consumed)) => {
                    if sign > 0 {
                        add.insert(flag);
                    } else {
                        remove.insert(flag);
                    }
                    // `consumed` counts words from `window`. The first word came
                    // from the signed token itself (same `i`); any additional
                    // consumed words came from tokens[i+1..].
                    i += 1 + consumed.saturating_sub(1);
                }
                None => {
                    return Some(BotCommand::Unknown(format!(
                        "co: unknown flag `{}`",
                        window.join(" ")
                    )));
                }
            }
        }
    }

    Some(BotCommand::ApplyCombatOrder { add, remove })
}

fn parse_reactivity(args: &[&str]) -> Option<BotCommand> {
    match args.first().copied() {
        Some("passive") => Some(BotCommand::SetReactivity(Reactivity::Passive)),
        Some("defensive") => Some(BotCommand::SetReactivity(Reactivity::Defensive)),
        Some("aggressive") => Some(BotCommand::SetReactivity(Reactivity::Aggressive)),
        _ => Some(BotCommand::Unknown(
            "react: missing level (passive/defensive/aggressive)".into(),
        )),
    }
}

fn parse_go(args: &[&str]) -> Option<BotCommand> {
    if args.len() >= 3 {
        let x = args[0].parse::<f32>().ok()?;
        let y = args[1].parse::<f32>().ok()?;
        let z = args[2].parse::<f32>().ok()?;
        Some(BotCommand::GoTo(x, y, z))
    } else {
        Some(BotCommand::Unknown(
            "go: need 3 coordinates (go <x> <y> <z>)".into(),
        ))
    }
}

fn parse_rtsc(args: &[&str]) -> Option<BotCommand> {
    match args.first().copied() {
        Some("select") => Some(BotCommand::RtscSelect),
        Some("cancel") => Some(BotCommand::RtscCancel),
        Some("toggle") => Some(BotCommand::RtscToggle),
        Some("move") => {
            if args.get(1).copied() == Some("exact") {
                Some(BotCommand::RtscMoveExact)
            } else {
                Some(BotCommand::RtscMove)
            }
        }
        Some("save") => match args.get(1).copied() {
            Some("here") => {
                let name = args.get(2).unwrap_or(&"default").to_string();
                Some(BotCommand::RtscSaveHere(name))
            }
            Some("exact") => {
                let name = args.get(2).unwrap_or(&"default").to_string();
                Some(BotCommand::RtscSave(name))
            }
            Some(name) => Some(BotCommand::RtscSave(name.to_string())),
            None => Some(BotCommand::RtscSave("default".into())),
        },
        Some("unsave") => {
            let name = args.get(1).unwrap_or(&"default").to_string();
            Some(BotCommand::RtscUnsave(name))
        }
        Some("go") => {
            let name = args.get(1).unwrap_or(&"default").to_string();
            Some(BotCommand::RtscGo(name))
        }
        Some("show") => Some(BotCommand::RtscShow),
        _ => Some(BotCommand::Unknown(
            "rtsc: select/cancel/toggle/move/save/go/show".into(),
        )),
    }
}

/// Parse a raid target icon name/number into a 1..=8 index. Accepts both
/// the canonical name (`star`, `skull`) and the numeric form (`rti1`..`rti8`).
fn parse_rti(token: &str) -> Option<u8> {
    // Numeric: rti1, rti8, or bare "1".."8".
    if let Some(rest) = token.strip_prefix("rti") {
        return rest.parse::<u8>().ok().filter(|&n| (1..=8).contains(&n));
    }
    if let Ok(n) = token.parse::<u8>()
        && (1..=8).contains(&n) {
            return Some(n);
        }
    Some(match token {
        "star" => 1,
        "circle" => 2,
        "diamond" => 3,
        "triangle" => 4,
        "moon" => 5,
        "square" => 6,
        "cross" | "x" => 7,
        "skull" => 8,
        _ => return None,
    })
}

fn parse_attack(args: &[&str]) -> Option<BotCommand> {
    match args.first().copied() {
        // `attack rti`, `attack skull`, `attack rti8`, `attack 8`
        Some(first) => {
            if first == "rti" {
                // Legacy RaidControl form: `attack rti` with implicit skull.
                return Some(BotCommand::AttackRti(8));
            }
            if let Some(icon) = parse_rti(first) {
                return Some(BotCommand::AttackRti(icon));
            }
            Some(BotCommand::Attack(None))
        }
        None => Some(BotCommand::Attack(None)),
    }
}

fn parse_pull(args: &[&str]) -> Option<BotCommand> {
    match args.first().copied() {
        Some(first) => {
            if first == "rti" {
                return Some(BotCommand::PullRti(8));
            }
            if let Some(icon) = parse_rti(first) {
                return Some(BotCommand::PullRti(icon));
            }
            Some(BotCommand::Unknown(
                "pull: need raid target (e.g. `pull skull`)".into(),
            ))
        }
        None => Some(BotCommand::Unknown("pull: need raid target".into())),
    }
}

fn parse_cc(args: &[&str]) -> Option<BotCommand> {
    match args.first().copied() {
        Some(first) => {
            if let Some(icon) = parse_rti(first) {
                return Some(BotCommand::CcRti(icon));
            }
            Some(BotCommand::Unknown(format!("cc: unknown target `{first}`")))
        }
        None => Some(BotCommand::Unknown("cc: need raid target".into())),
    }
}

/// `nc +a,-b c,+d e` — comma-separated list of ±strategy names. Multi-word
/// names ("rpg bg", "rpg maintenance") are one token per chunk.
fn parse_strategies(args: &[&str]) -> Option<BotCommand> {
    if args.is_empty() {
        return Some(BotCommand::Unknown("nc: missing strategy list".into()));
    }
    let joined = args.join(" ");
    let mut add = StrategyFlags::NONE;
    let mut remove = StrategyFlags::NONE;

    for chunk in joined.split(',') {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }

        let (sign, name): (i8, &str) = match chunk.chars().next() {
            Some('+') => (1, chunk[1..].trim()),
            Some('-') => (-1, chunk[1..].trim()),
            _ => {
                return Some(BotCommand::Unknown(format!(
                    "nc: expected +/- prefix on `{chunk}`"
                )));
            }
        };

        match StrategyFlags::parse_name(name) {
            Some(flag) => {
                if sign > 0 {
                    add.insert(flag);
                } else {
                    remove.insert(flag);
                }
            }
            None => {
                return Some(BotCommand::Unknown(format!(
                    "nc: unknown strategy `{name}`"
                )));
            }
        }
    }

    Some(BotCommand::ApplyStrategies { add, remove })
}

/// `cast <spell name>` or `cast self <spell name>`. Uses the spell-name
/// table in `data::spells` — anything not there returns `Unknown`.
fn parse_cast(args: &[&str]) -> Option<BotCommand> {
    if args.is_empty() {
        return Some(BotCommand::Unknown("cast: missing spell name".into()));
    }
    let (on_self, name_tokens): (bool, &[&str]) = if args[0] == "self" || args[0] == "me" {
        (true, &args[1..])
    } else {
        (false, args)
    };
    if name_tokens.is_empty() {
        return Some(BotCommand::Unknown("cast: missing spell name".into()));
    }
    let name = name_tokens.join(" ");
    match lookup_spell_by_name(&name) {
        Some(spell) => Some(BotCommand::CastOne { spell, on_self }),
        None => Some(BotCommand::Unknown(format!("cast: unknown spell `{name}`"))),
    }
}

fn parse_formation(args: &[&str]) -> Option<BotCommand> {
    let Some(first) = args.first().copied() else {
        return Some(BotCommand::Unknown(
            "formation: need type (near/line/circle/chaos/box/queue/arrow/wedge/pairs)".into(),
        ));
    };
    match FollowFormation::from_str(first) {
        Some(f) => Some(BotCommand::SetFormation(f)),
        None => Some(BotCommand::Unknown(format!("formation: unknown `{first}`"))),
    }
}

fn parse_travel(args: &[&str]) -> Option<BotCommand> {
    let Some(name) = args.first().copied() else {
        return Some(BotCommand::Unknown("travel: need a location name".into()));
    };
    match crate::data::named_locations::lookup(name) {
        Some(loc) => Some(BotCommand::TravelTo(loc)),
        None => Some(BotCommand::Unknown(format!(
            "travel: unknown location `{name}`"
        ))),
    }
}

fn parse_spell_id(args: &[&str]) -> Option<SpellId> {
    args.first()
        .and_then(|s| s.parse::<u32>().ok())
        .map(SpellId)
}

fn parse_heal_threshold(args: &[&str]) -> Option<BotCommand> {
    match args.first().and_then(|s| s.parse::<f32>().ok()) {
        Some(pct) if (0.0..=1.0).contains(&pct) => Some(BotCommand::SetHealThreshold(pct)),
        Some(pct) if (1.0..=100.0).contains(&pct) => {
            Some(BotCommand::SetHealThreshold(pct / 100.0))
        }
        _ => Some(BotCommand::Unknown(
            "heal: need percentage (0-100 or 0.0-1.0)".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mode_commands() {
        assert_eq!(
            parse("follow"),
            Some(BotCommand::SetMode(BehaviorMode::Follow))
        );
        assert_eq!(parse("stay"), Some(BotCommand::SetMode(BehaviorMode::Stay)));
        assert_eq!(
            parse("grind"),
            Some(BotCommand::SetMode(BehaviorMode::Grind))
        );
        assert_eq!(
            parse("quest"),
            Some(BotCommand::SetMode(BehaviorMode::Quest))
        );
        assert_eq!(
            parse("passive"),
            Some(BotCommand::SetMode(BehaviorMode::Passive))
        );
    }

    #[test]
    fn parse_combat_orders_bare() {
        assert_eq!(
            parse("co tank"),
            Some(BotCommand::SetCombatOrder(CombatOrder::TANK))
        );
        assert_eq!(
            parse("co assist"),
            Some(BotCommand::SetCombatOrder(CombatOrder::ASSIST))
        );
        assert_eq!(
            parse("co protect"),
            Some(BotCommand::SetCombatOrder(CombatOrder::PROTECT))
        );
        assert_eq!(
            parse("co pull"),
            Some(BotCommand::SetCombatOrder(CombatOrder::PULL))
        );
    }

    #[test]
    fn parse_combat_orders_signed() {
        // Simple additive.
        assert_eq!(
            parse("co +tank"),
            Some(BotCommand::ApplyCombatOrder {
                add: CombatOrder::TANK,
                remove: CombatOrder::NONE,
            }),
        );
        // Subtractive.
        assert_eq!(
            parse("co -threat"),
            Some(BotCommand::ApplyCombatOrder {
                add: CombatOrder::NONE,
                remove: CombatOrder::THREAT,
            }),
        );
        // Multi-word flag.
        assert_eq!(
            parse("co +tank assist"),
            Some(BotCommand::ApplyCombatOrder {
                add: CombatOrder::TANK_ASSIST,
                remove: CombatOrder::NONE,
            }),
        );
        // Comma-separated mixed.
        assert_eq!(
            parse("co -tank assist,+dps assist"),
            Some(BotCommand::ApplyCombatOrder {
                add: CombatOrder::DPS_ASSIST,
                remove: CombatOrder::TANK_ASSIST,
            }),
        );
        // Space-separated mixed, multi-flag.
        assert_eq!(
            parse("co -threat -dps assist -close +tank assist"),
            Some(BotCommand::ApplyCombatOrder {
                add: CombatOrder::TANK_ASSIST,
                remove: CombatOrder::THREAT | CombatOrder::DPS_ASSIST | CombatOrder::CLOSE,
            }),
        );
        // pull back — two-word flag in subtractive form.
        assert_eq!(
            parse("co -pull back"),
            Some(BotCommand::ApplyCombatOrder {
                add: CombatOrder::NONE,
                remove: CombatOrder::PULL_BACK,
            }),
        );
    }

    #[test]
    fn parse_reactivity_commands() {
        assert_eq!(
            parse("react passive"),
            Some(BotCommand::SetReactivity(Reactivity::Passive))
        );
        assert_eq!(
            parse("react aggressive"),
            Some(BotCommand::SetReactivity(Reactivity::Aggressive))
        );
    }

    #[test]
    fn parse_go_coordinates() {
        assert_eq!(
            parse("go 1.0 2.0 3.0"),
            Some(BotCommand::GoTo(1.0, 2.0, 3.0))
        );
        assert!(matches!(parse("go 1.0"), Some(BotCommand::Unknown(_))));
    }

    #[test]
    fn parse_blacklist() {
        assert_eq!(
            parse("blacklist 12345"),
            Some(BotCommand::BlacklistSpell(SpellId(12345)))
        );
        assert_eq!(parse("blacklist"), None);
    }

    #[test]
    fn parse_heal_threshold() {
        assert_eq!(parse("heal 80"), Some(BotCommand::SetHealThreshold(0.80)));
        assert_eq!(parse("heal 0.5"), Some(BotCommand::SetHealThreshold(0.5)));
    }

    #[test]
    fn empty_returns_none() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("  "), None);
    }

    #[test]
    fn unknown_command() {
        assert!(matches!(parse("xyzzy"), Some(BotCommand::Unknown(_))));
    }

    #[test]
    fn parse_rtsc_commands() {
        assert_eq!(parse("rtsc select"), Some(BotCommand::RtscSelect));
        assert_eq!(parse("rtsc cancel"), Some(BotCommand::RtscCancel));
        assert_eq!(parse("rtsc toggle"), Some(BotCommand::RtscToggle));
        assert_eq!(parse("rtsc move"), Some(BotCommand::RtscMove));
        assert_eq!(parse("rtsc move exact"), Some(BotCommand::RtscMoveExact));
        assert_eq!(
            parse("rtsc save here myspot"),
            Some(BotCommand::RtscSaveHere("myspot".into()))
        );
        assert_eq!(
            parse("rtsc save tankpos"),
            Some(BotCommand::RtscSave("tankpos".into()))
        );
        assert_eq!(
            parse("rtsc unsave tankpos"),
            Some(BotCommand::RtscUnsave("tankpos".into()))
        );
        assert_eq!(
            parse("rtsc go tankpos"),
            Some(BotCommand::RtscGo("tankpos".into()))
        );
        assert_eq!(parse("rtsc show"), Some(BotCommand::RtscShow));
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(
            parse("FOLLOW"),
            Some(BotCommand::SetMode(BehaviorMode::Follow))
        );
        assert_eq!(
            parse("Co Tank"),
            Some(BotCommand::SetCombatOrder(CombatOrder::TANK))
        );
    }

    #[test]
    fn nc_strategy_toggles() {
        assert_eq!(
            parse("nc +rtsc"),
            Some(BotCommand::ApplyStrategies {
                add: StrategyFlags::RTSC,
                remove: StrategyFlags::NONE,
            }),
        );
        assert_eq!(
            parse("nc -rpg bg"),
            Some(BotCommand::ApplyStrategies {
                add: StrategyFlags::NONE,
                remove: StrategyFlags::RPG_BG,
            }),
        );
        assert_eq!(
            parse("nc +rtsc,-rpg,-rpg bg,-rpg explore"),
            Some(BotCommand::ApplyStrategies {
                add: StrategyFlags::RTSC,
                remove: StrategyFlags::RPG | StrategyFlags::RPG_BG | StrategyFlags::RPG_EXPLORE,
            }),
        );
        assert!(matches!(parse("nc +bogus"), Some(BotCommand::Unknown(_))));
    }

    #[test]
    fn reset_ai_alias() {
        assert_eq!(parse("reset ai"), Some(BotCommand::ResetStrategies));
        assert_eq!(parse("reset"), Some(BotCommand::Reset));
    }

    #[test]
    fn panic_aliases() {
        assert_eq!(parse("flee"), Some(BotCommand::Flee));
        assert_eq!(parse("runaway"), Some(BotCommand::Flee));
        assert_eq!(parse("panic"), Some(BotCommand::Flee));
        assert_eq!(parse("free"), Some(BotCommand::Free));
        assert_eq!(parse("summon"), Some(BotCommand::Summon));
    }

    #[test]
    fn cast_named_spell() {
        assert_eq!(
            parse("cast taunt"),
            Some(BotCommand::CastOne {
                spell: SpellId(355),
                on_self: false
            }),
        );
        assert_eq!(
            parse("cast self bubble"),
            Some(BotCommand::CastOne {
                spell: SpellId(642),
                on_self: true
            }),
        );
        assert!(matches!(parse("cast xyzzy"), Some(BotCommand::Unknown(_))));
        assert!(matches!(parse("cast"), Some(BotCommand::Unknown(_))));
    }

    #[test]
    fn formation_command() {
        assert_eq!(
            parse("formation near"),
            Some(BotCommand::SetFormation(FollowFormation::Near)),
        );
        assert_eq!(
            parse("formation wedge"),
            Some(BotCommand::SetFormation(FollowFormation::Wedge)),
        );
        assert!(matches!(
            parse("formation bogus"),
            Some(BotCommand::Unknown(_))
        ));
    }

    #[test]
    fn utility_commands() {
        assert_eq!(parse("come"), Some(BotCommand::ComeToMe));
        assert_eq!(parse("c"), Some(BotCommand::ComeToMe));
        assert_eq!(parse("attack"), Some(BotCommand::Attack(None)));
        assert_eq!(parse("repair"), Some(BotCommand::Repair));
        assert_eq!(parse("vendor"), Some(BotCommand::Vendor));
        assert_eq!(parse("status"), Some(BotCommand::Status));
        assert_eq!(parse("reset"), Some(BotCommand::Reset));
        assert_eq!(parse("mount"), Some(BotCommand::Mount));
        assert_eq!(parse("rez"), Some(BotCommand::Resurrect));
    }
}
