/// Chat command parser — converts text into `BotCommand`.
///
/// Commands arrive as whispers from the master player. The C++ side routes
/// them through `playerbot_chat_command()` which calls this parser.
///
/// Design: ~20 clean commands replace the old 70+ redundant C++ commands.
/// Each command maps to exactly one `BotCommand` variant.
use crate::bot::class_prefs::{
    HunterAspect, HunterTrap, PaladinAura, PaladinBlessing, PoisonKind, ShamanImbue, TotemRole,
    TotemSlot, WarlockCurse, WarriorStance, WeaponHand,
};
use crate::bot::settings::{
    BehaviorMode, BotStateKind, ChatChannel, CombatOrder, FollowFormation, LootPolicy, Reactivity,
    StrategyFlags,
};
use crate::commands::BotCommand;
use crate::data::spells::lookup_spell_by_name;
use crate::ffi::{ItemId, SpellId};

/// One entry in the command table. `names` lists the keyword plus any aliases
/// (all lowercase, single-word). `parse` is given the matched keyword and the
/// remaining argument tokens.
struct CommandSpec {
    names: &'static [&'static str],
    parse: fn(cmd: &str, args: &[&str]) -> Option<BotCommand>,
}

/// Command vocabulary. Matching is first-hit; ordering within the table has
/// no functional impact but is grouped by theme for readability.
///
/// Simple no-arg commands use an inline closure that discards `cmd`/`args`.
/// Commands with shared parsers (`SetMode`) dispatch on the matched keyword.
/// Complex commands forward to a dedicated helper function below.
const COMMANDS: &[CommandSpec] = &[
    // -- Behavior modes --
    CommandSpec {
        names: &[
            "follow", "stay", "grind", "quest", "passive", "rpg", "wander", "bg",
        ],
        parse: |cmd, _| BehaviorMode::from_str(cmd).map(BotCommand::SetMode),
    },
    // -- Combat orders / strategies / reactivity --
    CommandSpec { names: &["co"], parse: |_, a| parse_combat_order(a) },
    CommandSpec { names: &["nc"], parse: |_, a| parse_strategies(a, BotStateKind::NonCombat) },
    CommandSpec { names: &["react"], parse: |_, a| parse_reactivity(a) },
    // PB2 4-state model: each of `co` / `nc` / `react` / `de` targets
    // its own strategy engine. `de +spec,?` toggles *dead-state*
    // strategies; the other three are handled by their own parsers
    // (`co` routes to `parse_combat_order`, `react` routes to
    // `parse_reactivity`).
    CommandSpec { names: &["de"], parse: |_, a| parse_strategies(a, BotStateKind::Dead) },
    CommandSpec { names: &["ll"], parse: |_, a| parse_loot_policy(a) },
    // Mangosbot sends the two-word form `save mana [?|on|off]`.
    CommandSpec { names: &["save"], parse: |_, a| parse_save(a) },
    // -- Targeting --
    CommandSpec { names: &["focus"], parse: |_, _| Some(BotCommand::Focus(None)) },
    CommandSpec { names: &["attack"], parse: |_, a| parse_attack(a) },
    CommandSpec { names: &["pull"], parse: |_, a| parse_pull(a) },
    CommandSpec { names: &["cc"], parse: |_, a| parse_cc(a) },
    // -- Movement --
    CommandSpec { names: &["come", "c"], parse: |_, _| Some(BotCommand::ComeToMe) },
    CommandSpec { names: &["guard"], parse: |_, _| Some(BotCommand::Guard) },
    CommandSpec { names: &["go"], parse: |_, a| parse_go(a) },
    // -- RTSC (Real-Time Strategy Control) --
    CommandSpec { names: &["rtsc"], parse: |_, a| parse_rtsc(a) },
    // -- Spell control --
    CommandSpec {
        names: &["blacklist"],
        parse: |_, a| parse_spell_id(a).map(BotCommand::BlacklistSpell),
    },
    CommandSpec {
        names: &["unblacklist"],
        parse: |_, a| parse_spell_id(a).map(BotCommand::UnblacklistSpell),
    },
    // -- Economy --
    CommandSpec { names: &["repair"], parse: |_, _| Some(BotCommand::Repair) },
    CommandSpec { names: &["vendor", "sell"], parse: |_, _| Some(BotCommand::Vendor) },
    // -- Healing --
    CommandSpec { names: &["heal"], parse: |_, a| parse_heal_threshold(a) },
    // -- Information --
    CommandSpec {
        names: &["status", "stats", "who"],
        parse: |_, _| Some(BotCommand::Status),
    },
    CommandSpec { names: &["settings"], parse: |_, _| Some(BotCommand::ListSettings) },
    CommandSpec {
        names: &["where", "position", "pos"],
        parse: |_, _| Some(BotCommand::Where),
    },
    CommandSpec { names: &["help", "commands"], parse: |_, _| Some(BotCommand::Help) },
    CommandSpec { names: &["ready"], parse: |_, _| Some(BotCommand::Ready) },
    // -- Utility --
    CommandSpec {
        names: &["reset"],
        parse: |_, a| {
            if a.first().copied() == Some("ai") {
                Some(BotCommand::ResetStrategies)
            } else {
                Some(BotCommand::Reset)
            }
        },
    },
    CommandSpec { names: &["mount", "dismount"], parse: |_, _| Some(BotCommand::Mount) },
    CommandSpec {
        names: &["rez", "resurrect"],
        parse: |_, _| Some(BotCommand::Resurrect),
    },
    // -- Panic / aliases --
    CommandSpec {
        names: &["flee", "runaway", "panic"],
        parse: |_, _| Some(BotCommand::Flee),
    },
    CommandSpec { names: &["free"], parse: |_, _| Some(BotCommand::Free) },
    CommandSpec { names: &["summon"], parse: |_, _| Some(BotCommand::Summon) },
    // -- Cast a named spell once (addon sends `cast Taunt`). --
    CommandSpec { names: &["cast"], parse: |_, a| parse_cast(a) },
    // -- Formation --
    CommandSpec { names: &["formation"], parse: |_, a| parse_formation(a) },
    // -- Named-location travel --
    CommandSpec { names: &["travel", "goto"], parse: |_, a| parse_travel(a) },
    // -- Tunables / PB2 Wave 1 --
    CommandSpec { names: &["range"], parse: |_, a| parse_range(a) },
    CommandSpec { names: &["stance"], parse: |_, a| parse_stance(a) },
    CommandSpec {
        names: &["max-dps", "maxdps"],
        parse: |_, _| Some(BotCommand::MaxDps),
    },
    CommandSpec {
        names: &["save-mana", "savemana"],
        parse: |_, _| Some(BotCommand::ToggleSaveMana),
    },
    CommandSpec {
        names: &["self-res", "selfres"],
        parse: |_, _| Some(BotCommand::ToggleSelfRes),
    },
    CommandSpec { names: &["cheat"], parse: |_, a| parse_cheat(a) },
    CommandSpec { names: &["keep"], parse: |_, a| parse_keep(a, true) },
    CommandSpec { names: &["unkeep"], parse: |_, a| parse_keep(a, false) },
    CommandSpec { names: &["chat"], parse: |_, a| parse_chat(a) },
    CommandSpec { names: &["rti"], parse: |_, a| parse_rti_cmd(a) },
    CommandSpec { names: &["emote"], parse: |_, a| parse_emote(a) },
    CommandSpec {
        names: &["debug", "cdebug"],
        parse: |_, _| Some(BotCommand::Debug),
    },
    // -- Wave 2 info queries (reuse existing FFI) --
    CommandSpec { names: &["los"], parse: |_, _| Some(BotCommand::CheckLos) },
    CommandSpec {
        names: &["quests", "q"],
        parse: |_, _| Some(BotCommand::ListQuests),
    },
    CommandSpec { names: &["talents"], parse: |_, _| Some(BotCommand::ListTalents) },
    CommandSpec { names: &["spells"], parse: |_, _| Some(BotCommand::ListSpells) },
    CommandSpec {
        names: &["release"],
        parse: |_, _| Some(BotCommand::ReleaseSpirit),
    },
    CommandSpec {
        names: &["revive"],
        parse: |_, _| Some(BotCommand::AcceptRevive),
    },
    CommandSpec { names: &["jump"], parse: |_, _| Some(BotCommand::Jump) },
    CommandSpec {
        names: &["hearth", "home"],
        parse: |_, _| Some(BotCommand::UseHearth),
    },
    CommandSpec {
        names: &["rep", "reputation"],
        parse: |_, _| Some(BotCommand::ListReputation),
    },
    CommandSpec {
        names: &["skill", "skills"],
        parse: |_, _| Some(BotCommand::ListSkills),
    },
    CommandSpec {
        names: &["accept"],
        parse: |_, _| Some(BotCommand::QuestAccept),
    },
    CommandSpec {
        names: &["drop"],
        parse: |_, a| {
            a.first()
                .and_then(|s| s.parse::<u32>().ok())
                .map(BotCommand::QuestDrop)
        },
    },
    CommandSpec {
        names: &["mail"],
        parse: |_, a| match a.first().copied() {
            Some("take") | Some("takeall") | Some("all") => Some(BotCommand::MailTakeAll),
            _ => Some(BotCommand::MailSummary),
        },
    },
    CommandSpec {
        names: &["leave"],
        parse: |_, _| Some(BotCommand::GuildLeave),
    },
    // -- Class preferences --
    CommandSpec { names: &["poison", "poisons"], parse: |_, a| parse_poison(a) },
    CommandSpec { names: &["totem", "totems"], parse: |_, a| parse_totem(a) },
    CommandSpec { names: &["imbue", "imbues"], parse: |_, a| parse_imbue(a) },
    CommandSpec { names: &["aura", "auras"], parse: |_, a| parse_aura(a) },
    CommandSpec { names: &["blessing", "blessings"], parse: |_, a| parse_blessing(a) },
    CommandSpec { names: &["aspect", "aspects"], parse: |_, a| parse_aspect(a) },
    CommandSpec { names: &["trap", "traps"], parse: |_, a| parse_trap(a) },
    CommandSpec { names: &["curse", "curses"], parse: |_, a| parse_curse(a) },
    CommandSpec { names: &["forcestance", "stancelock"], parse: |_, a| parse_forcestance(a) },
    CommandSpec { names: &["suppression"], parse: |_, a| parse_duty(a, DutyKind::Suppression) },
    CommandSpec { names: &["douse"], parse: |_, a| parse_duty(a, DutyKind::Douse) },
];

/// Parse a chat message into a `BotCommand`.
/// Returns `None` if the message is empty. Unrecognised keywords yield
/// `Some(BotCommand::Unknown(..))` so the dispatcher can reply.
pub fn parse(text: &str) -> Option<BotCommand> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    let lower = text.to_lowercase();
    let parts: Vec<&str> = lower.split_whitespace().collect();
    let cmd = parts[0];
    let args = &parts[1..];

    for spec in COMMANDS {
        if spec.names.contains(&cmd) {
            return (spec.parse)(cmd, args);
        }
    }
    Some(BotCommand::Unknown(text.to_string()))
}

/// Parse `co` arguments.
///
/// Bare form (no sign): `co tank` → full replace with that flag.
/// Signed form: `co +tank -fury`, `co +tank assist,+dps assist` → additive/subtractive edit.
/// Flags are comma- or space-separated; multi-word names (`tank assist`,
/// `dps assist`, `pull back`) are matched greedily.
fn parse_combat_order(args: &[&str]) -> Option<BotCommand> {
    if args.is_empty() || args == ["?"] {
        return Some(BotCommand::QueryCombatOrder);
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
    // A bare `?` chunk (e.g. `co +tank assist,?`) is Mangosbot's request to
    // apply *and* re-query the flags in one round-trip — strip it and set
    // `query=true` on the emitted command.
    let mut add = CombatOrder::NONE;
    let mut remove = CombatOrder::NONE;
    let mut query = false;

    for chunk in joined.split(',') {
        let chunk_trim = chunk.trim();
        if chunk_trim == "?" {
            query = true;
            continue;
        }
        let tokens: Vec<&str> = chunk_trim.split_whitespace().collect();
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

    Some(BotCommand::ApplyCombatOrder { add, remove, query })
}

fn parse_reactivity(args: &[&str]) -> Option<BotCommand> {
    // `react` has two totally disjoint forms in the addon vocabulary:
    //
    //   * plain level setter — `react passive|defensive|aggressive` — adjusts
    //     the bot's passive/defensive/aggressive stance (Rust's native
    //     `Reactivity` setting).
    //
    //   * PB2 signed strategy list — `react +tank feral,?` — toggles
    //     strategies in the `Reaction` BotState slot. PB2 uses a dedicated
    //     reaction engine per bot; the Rust port now mirrors that with
    //     `BotStateKind::Reaction`, so a signed `react` command routes to
    //     `parse_strategies` with the `Reaction` slot, and emits
    //     `Reaction Strategies: ...` when trailing `,?`.
    //
    // Dispatch on the first token's sign prefix.
    if let Some(first) = args.first().copied() {
        if first.starts_with('+') || first.starts_with('-') {
            return parse_strategies(args, BotStateKind::Reaction);
        }
    }
    match args.first().copied() {
        None | Some("?") => Some(BotCommand::QueryReactivity),
        Some("passive") => Some(BotCommand::SetReactivity(Reactivity::Passive)),
        Some("defensive") => Some(BotCommand::SetReactivity(Reactivity::Defensive)),
        Some("aggressive") => Some(BotCommand::SetReactivity(Reactivity::Aggressive)),
        _ => Some(BotCommand::Unknown(
            "react: missing level (passive/defensive/aggressive)".into(),
        )),
    }
}

/// `ll` — loot list / policy.
///   `ll` or `ll ?`      → query current policy
///   `ll <cat>`          → full replacement (single category)
///   `ll +cat,-cat,~cat` → signed apply (add / remove / toggle)
fn parse_loot_policy(args: &[&str]) -> Option<BotCommand> {
    if args.is_empty() || args == ["?"] {
        return Some(BotCommand::QueryLootPolicy);
    }
    let joined = args.join(" ");
    let signed = joined.contains('+') || joined.contains('-') || joined.contains('~');

    if !signed {
        // Bare single category: full replacement.
        let name = joined.trim();
        match LootPolicy::parse_name(name) {
            Some(flag) => Some(BotCommand::ApplyLootPolicy {
                add: flag,
                remove: LootPolicy::all_categories() - flag,
                toggle: LootPolicy::NONE,
            }),
            None => Some(BotCommand::Unknown(format!(
                "ll: unknown category `{name}`"
            ))),
        }
    } else {
        let mut add = LootPolicy::NONE;
        let mut remove = LootPolicy::NONE;
        let mut toggle = LootPolicy::NONE;

        for chunk in joined.split(',') {
            let chunk = chunk.trim();
            if chunk.is_empty() || chunk == "?" {
                continue;
            }
            let (sign, name): (u8, &str) = match chunk.chars().next() {
                Some('+') => (1, chunk[1..].trim()),
                Some('-') => (2, chunk[1..].trim()),
                Some('~') => (3, chunk[1..].trim()),
                _ => {
                    return Some(BotCommand::Unknown(format!(
                        "ll: expected +/-/~ prefix on `{chunk}`"
                    )));
                }
            };
            match LootPolicy::parse_name(name) {
                Some(flag) => match sign {
                    1 => add.insert(flag),
                    2 => remove.insert(flag),
                    _ => toggle.insert(flag),
                },
                None => {
                    return Some(BotCommand::Unknown(format!(
                        "ll: unknown category `{name}`"
                    )));
                }
            }
        }
        Some(BotCommand::ApplyLootPolicy { add, remove, toggle })
    }
}

/// `save <subcommand>` — currently only `save mana [?|on|off]`.
fn parse_save(args: &[&str]) -> Option<BotCommand> {
    match args.first().copied() {
        Some("mana") => match args.get(1).copied() {
            None => Some(BotCommand::ToggleSaveMana),
            Some("?") => Some(BotCommand::QuerySaveMana),
            Some("on") | Some("1") | Some("yes") | Some("true") => {
                Some(BotCommand::SetSaveMana(true))
            }
            Some("off") | Some("0") | Some("no") | Some("false") => {
                Some(BotCommand::SetSaveMana(false))
            }
            Some("toggle") | Some("~") => Some(BotCommand::ToggleSaveMana),
            Some(other) => Some(BotCommand::Unknown(format!(
                "save mana: unknown `{other}` (expected ?/on/off/toggle)"
            ))),
        },
        _ => Some(BotCommand::Unknown(
            "save: expected `save mana [?|on|off]`".into(),
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
///
/// A bare `?` chunk (`nc +dps assist,?`) is Mangosbot's apply-and-query
/// shorthand — stripped here and surfaced as `query=true` on the emitted
/// command so the handler whispers the new state after applying.
fn parse_strategies(args: &[&str], state: BotStateKind) -> Option<BotCommand> {
    let cmd_name = state.addon_command();
    if args.is_empty() || args == ["?"] {
        return Some(BotCommand::QueryStrategies(state));
    }
    let joined = args.join(" ");
    let mut add = StrategyFlags::NONE;
    let mut remove = StrategyFlags::NONE;
    let mut query = false;

    for chunk in joined.split(',') {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        if chunk == "?" {
            query = true;
            continue;
        }

        let (sign, name): (i8, &str) = match chunk.chars().next() {
            Some('+') => (1, chunk[1..].trim()),
            Some('-') => (-1, chunk[1..].trim()),
            _ => {
                return Some(BotCommand::Unknown(format!(
                    "{cmd_name}: expected +/- prefix on `{chunk}`"
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
                    "{cmd_name}: unknown strategy `{name}`"
                )));
            }
        }
    }

    Some(BotCommand::ApplyStrategies { state, add, remove, query })
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
        return Some(BotCommand::QueryFormation);
    };
    if first == "?" {
        return Some(BotCommand::QueryFormation);
    }
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

fn parse_range(args: &[&str]) -> Option<BotCommand> {
    match args.first().and_then(|s| s.parse::<f32>().ok()) {
        Some(d) if (0.5..=40.0).contains(&d) => Some(BotCommand::SetRange(d)),
        _ => Some(BotCommand::Unknown("range: need yards (0.5-40)".into())),
    }
}

fn parse_stance(args: &[&str]) -> Option<BotCommand> {
    // Accept numeric 0..=3 or named battle/defensive/berserker.
    let Some(first) = args.first().copied() else {
        return Some(BotCommand::QueryStance);
    };
    if first == "?" {
        return Some(BotCommand::QueryStance);
    }
    let st: u8 = match first {
        "none" | "0" => 0,
        "battle" | "1" => 1,
        "defensive" | "def" | "2" => 2,
        "berserker" | "zerk" | "3" => 3,
        _ => {
            return Some(BotCommand::Unknown(format!(
                "stance: unknown `{first}`"
            )));
        }
    };
    Some(BotCommand::SetStance(st))
}

fn parse_cheat(args: &[&str]) -> Option<BotCommand> {
    // `cheat <flags>` — accept decimal or 0x-prefixed hex. `cheat off`/`0`
    // clears the flags.
    let Some(first) = args.first().copied() else {
        return Some(BotCommand::Unknown("cheat: need flags or `off`".into()));
    };
    if first == "off" || first == "none" {
        return Some(BotCommand::SetCheatFlags(0));
    }
    let parsed = if let Some(hex) = first.strip_prefix("0x") {
        u32::from_str_radix(hex, 16).ok()
    } else {
        first.parse::<u32>().ok()
    };
    match parsed {
        Some(flags) => Some(BotCommand::SetCheatFlags(flags)),
        None => Some(BotCommand::Unknown(format!("cheat: bad flags `{first}`"))),
    }
}

fn parse_keep(args: &[&str], keep: bool) -> Option<BotCommand> {
    match args.first().and_then(|s| s.parse::<u32>().ok()) {
        Some(id) => Some(if keep {
            BotCommand::KeepItem(ItemId(id))
        } else {
            BotCommand::UnkeepItem(ItemId(id))
        }),
        None => Some(BotCommand::Unknown(
            "keep/unkeep: need numeric item id".into(),
        )),
    }
}

fn parse_chat(args: &[&str]) -> Option<BotCommand> {
    // `chat <channel> [on|off]` — omitting state defaults to `on`.
    let Some(channel_name) = args.first().copied() else {
        return Some(BotCommand::Unknown(
            "chat: need channel (say/party/raid/guild/whisper)".into(),
        ));
    };
    let Some(channel) = ChatChannel::from_name(channel_name) else {
        return Some(BotCommand::Unknown(format!(
            "chat: unknown channel `{channel_name}`"
        )));
    };
    let on = match args.get(1).copied() {
        Some("off") | Some("no") | Some("0") => false,
        _ => true,
    };
    Some(BotCommand::SetChatChannel { channel, on })
}

fn parse_rti_cmd(args: &[&str]) -> Option<BotCommand> {
    // `rti cc ...` — CC raid-target preference (Mangosbot's per-bot CC mark).
    if args.first().copied() == Some("cc") {
        return match args.get(1).copied() {
            Some("?") => Some(BotCommand::QueryCcRti),
            None | Some("clear") | Some("none") => Some(BotCommand::SetPreferredCcRti(None)),
            Some(tok) => match parse_rti(tok) {
                Some(icon) => Some(BotCommand::SetPreferredCcRti(Some(icon))),
                None => Some(BotCommand::Unknown(format!("rti cc: unknown `{tok}`"))),
            },
        };
    }
    match args.first().copied() {
        Some("?") => Some(BotCommand::QueryRti),
        None | Some("clear") | Some("none") => Some(BotCommand::SetPreferredRti(None)),
        Some(tok) => match parse_rti(tok) {
            Some(icon) => Some(BotCommand::SetPreferredRti(Some(icon))),
            None => Some(BotCommand::Unknown(format!("rti: unknown `{tok}`"))),
        },
    }
}

fn parse_emote(args: &[&str]) -> Option<BotCommand> {
    match args.first().and_then(|s| s.parse::<u32>().ok()) {
        Some(id) => Some(BotCommand::Emote(id)),
        None => Some(BotCommand::Unknown("emote: need numeric emote id".into())),
    }
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

/// `poison` → `ShowPoisons`
/// `poison mh instant` / `poison oh deadly` → `SetPoison { hand, kind: Some(..) }`
/// `poison mh none` → `SetPoison { hand, kind: None }`
fn parse_poison(args: &[&str]) -> Option<BotCommand> {
    if args.is_empty() {
        return Some(BotCommand::ShowPoisons);
    }
    let Some(hand) = WeaponHand::from_token(args[0]) else {
        return Some(BotCommand::Unknown(
            "poison: expected mh|oh <kind|none>".into(),
        ));
    };
    let kind_tok = args.get(1).copied().unwrap_or("none");
    let kind = if kind_tok == "none" || kind_tok == "clear" || kind_tok == "off" {
        None
    } else {
        let Some(k) = PoisonKind::from_token(kind_tok) else {
            return Some(BotCommand::Unknown(format!(
                "poison: unknown kind '{kind_tok}'"
            )));
        };
        Some(k)
    };
    Some(BotCommand::SetPoison { hand, kind })
}

/// `totem` → `ShowTotems`
/// `totem earth strengthofearth` → `SetTotem { slot, role: Some(..) }`
/// `totem fire none` → `SetTotem { slot, role: None }`
fn parse_totem(args: &[&str]) -> Option<BotCommand> {
    if args.is_empty() {
        return Some(BotCommand::ShowTotems);
    }
    let Some(slot) = TotemSlot::from_token(args[0]) else {
        return Some(BotCommand::Unknown(
            "totem: expected earth|fire|water|air <role|none>".into(),
        ));
    };
    let role_tok = args.get(1).copied().unwrap_or("none");
    let role = if role_tok == "none" || role_tok == "clear" || role_tok == "off" {
        None
    } else {
        let Some(r) = TotemRole::from_token(role_tok) else {
            return Some(BotCommand::Unknown(format!(
                "totem: unknown role '{role_tok}'"
            )));
        };
        if r.slot() != slot {
            return Some(BotCommand::Unknown(format!(
                "totem: {role_tok} is not a {} totem",
                slot.as_str()
            )));
        }
        Some(r)
    };
    Some(BotCommand::SetTotem { slot, role })
}

/// `imbue` → `ShowShamanImbues`
/// `imbue mh flametongue` → `SetShamanImbue { hand, imbue: Some(..) }`
/// `imbue oh none` → `SetShamanImbue { hand, imbue: None }`
fn parse_imbue(args: &[&str]) -> Option<BotCommand> {
    if args.is_empty() {
        return Some(BotCommand::ShowShamanImbues);
    }
    let Some(hand) = WeaponHand::from_token(args[0]) else {
        return Some(BotCommand::Unknown(
            "imbue: expected mh|oh <kind|none>".into(),
        ));
    };
    let tok = args.get(1).copied().unwrap_or("none");
    let imbue = if tok == "none" || tok == "clear" || tok == "off" {
        None
    } else {
        let Some(i) = ShamanImbue::from_token(tok) else {
            return Some(BotCommand::Unknown(format!("imbue: unknown kind '{tok}'")));
        };
        Some(i)
    };
    Some(BotCommand::SetShamanImbue { hand, imbue })
}

/// `aura` → `ShowPaladinPrefs`
/// `aura devotion` → `SetPaladinAura(Some(..))`
/// `aura none` → `SetPaladinAura(None)`
fn parse_aura(args: &[&str]) -> Option<BotCommand> {
    let Some(&tok) = args.first() else {
        return Some(BotCommand::ShowPaladinPrefs);
    };
    if tok == "none" || tok == "clear" || tok == "off" {
        return Some(BotCommand::SetPaladinAura(None));
    }
    match PaladinAura::from_token(tok) {
        Some(a) => Some(BotCommand::SetPaladinAura(Some(a))),
        None => Some(BotCommand::Unknown(format!("aura: unknown '{tok}'"))),
    }
}

/// `blessing` → `ShowPaladinPrefs`
/// `blessing might` → `SetPaladinBlessing(Some(..))`
/// `blessing none` → `SetPaladinBlessing(None)`
/// `blessing greater on|off` → `SetPaladinGreaterBlessing(bool)`
fn parse_blessing(args: &[&str]) -> Option<BotCommand> {
    let Some(&tok) = args.first() else {
        return Some(BotCommand::ShowPaladinPrefs);
    };
    if tok == "greater" || tok == "gb" {
        let flag_tok = args.get(1).copied().unwrap_or("on");
        let flag = match flag_tok {
            "on" | "true" | "yes" | "1" => true,
            "off" | "false" | "no" | "0" => false,
            _ => {
                return Some(BotCommand::Unknown(format!(
                    "blessing greater: expected on|off, got '{flag_tok}'"
                )));
            }
        };
        return Some(BotCommand::SetPaladinGreaterBlessing(flag));
    }
    if tok == "none" || tok == "clear" || tok == "off" {
        return Some(BotCommand::SetPaladinBlessing(None));
    }
    match PaladinBlessing::from_token(tok) {
        Some(b) => Some(BotCommand::SetPaladinBlessing(Some(b))),
        None => Some(BotCommand::Unknown(format!("blessing: unknown '{tok}'"))),
    }
}

/// `aspect` → `ShowHunterPrefs`
/// `aspect hawk` → `SetHunterAspect(Some(..))`
/// `aspect none` → `SetHunterAspect(None)`
fn parse_aspect(args: &[&str]) -> Option<BotCommand> {
    let Some(&tok) = args.first() else {
        return Some(BotCommand::ShowHunterPrefs);
    };
    if tok == "none" || tok == "clear" || tok == "off" {
        return Some(BotCommand::SetHunterAspect(None));
    }
    match HunterAspect::from_token(tok) {
        Some(a) => Some(BotCommand::SetHunterAspect(Some(a))),
        None => Some(BotCommand::Unknown(format!("aspect: unknown '{tok}'"))),
    }
}

/// `trap` → `ShowHunterPrefs`
/// `trap freezing` → `SetHunterTrap(Some(..))`
/// `trap none` → `SetHunterTrap(None)`
fn parse_trap(args: &[&str]) -> Option<BotCommand> {
    let Some(&tok) = args.first() else {
        return Some(BotCommand::ShowHunterPrefs);
    };
    if tok == "none" || tok == "clear" || tok == "off" {
        return Some(BotCommand::SetHunterTrap(None));
    }
    match HunterTrap::from_token(tok) {
        Some(t) => Some(BotCommand::SetHunterTrap(Some(t))),
        None => Some(BotCommand::Unknown(format!("trap: unknown '{tok}'"))),
    }
}

/// `curse` → `ShowWarlockPrefs`
/// `curse agony` → `SetWarlockCurse(Some(..))`
/// `curse none` → `SetWarlockCurse(None)`
fn parse_curse(args: &[&str]) -> Option<BotCommand> {
    let Some(&tok) = args.first() else {
        return Some(BotCommand::ShowWarlockPrefs);
    };
    if tok == "none" || tok == "clear" || tok == "off" {
        return Some(BotCommand::SetWarlockCurse(None));
    }
    match WarlockCurse::from_token(tok) {
        Some(c) => Some(BotCommand::SetWarlockCurse(Some(c))),
        None => Some(BotCommand::Unknown(format!("curse: unknown '{tok}'"))),
    }
}

/// Which encounter duty a `parse_duty` call is parsing for.
enum DutyKind {
    Suppression,
    Douse,
}

/// `suppression` / `douse` (no args)      → `ShowEncounterPrefs`
/// `suppression auto|forbid|force`        → `SetSuppressionDuty(..)`
/// `douse auto|forbid|force`              → `SetDouseDuty(..)`
fn parse_duty(args: &[&str], kind: DutyKind) -> Option<BotCommand> {
    use crate::bot::encounter_prefs::DutyMode;
    let Some(&tok) = args.first() else {
        return Some(BotCommand::ShowEncounterPrefs);
    };
    match DutyMode::from_word(tok) {
        Some(mode) => Some(match kind {
            DutyKind::Suppression => BotCommand::SetSuppressionDuty(mode),
            DutyKind::Douse => BotCommand::SetDouseDuty(mode),
        }),
        None => {
            let label = match kind {
                DutyKind::Suppression => "suppression",
                DutyKind::Douse => "douse",
            };
            Some(BotCommand::Unknown(format!("{label}: unknown '{tok}'")))
        }
    }
}

/// `forcestance` → `ShowWarriorPrefs`
/// `forcestance berserker` → `SetWarriorForcedStance(Some(..))`
/// `forcestance none` → `SetWarriorForcedStance(None)`
fn parse_forcestance(args: &[&str]) -> Option<BotCommand> {
    let Some(&tok) = args.first() else {
        return Some(BotCommand::ShowWarriorPrefs);
    };
    if tok == "none" || tok == "clear" || tok == "off" {
        return Some(BotCommand::SetWarriorForcedStance(None));
    }
    match WarriorStance::from_token(tok) {
        Some(st) => Some(BotCommand::SetWarriorForcedStance(Some(st))),
        None => Some(BotCommand::Unknown(format!("forcestance: unknown '{tok}'"))),
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
            Some(BotCommand::ApplyCombatOrder { query: false,
                add: CombatOrder::TANK,
                remove: CombatOrder::NONE,
            }),
        );
        // Subtractive.
        assert_eq!(
            parse("co -threat"),
            Some(BotCommand::ApplyCombatOrder { query: false,
                add: CombatOrder::NONE,
                remove: CombatOrder::THREAT,
            }),
        );
        // Multi-word flag.
        assert_eq!(
            parse("co +tank assist"),
            Some(BotCommand::ApplyCombatOrder { query: false,
                add: CombatOrder::TANK_ASSIST,
                remove: CombatOrder::NONE,
            }),
        );
        // Comma-separated mixed.
        assert_eq!(
            parse("co -tank assist,+dps assist"),
            Some(BotCommand::ApplyCombatOrder { query: false,
                add: CombatOrder::DPS_ASSIST,
                remove: CombatOrder::TANK_ASSIST,
            }),
        );
        // Space-separated mixed, multi-flag.
        assert_eq!(
            parse("co -threat -dps assist -close +tank assist"),
            Some(BotCommand::ApplyCombatOrder { query: false,
                add: CombatOrder::TANK_ASSIST,
                remove: CombatOrder::THREAT | CombatOrder::DPS_ASSIST | CombatOrder::CLOSE,
            }),
        );
        // pull back — two-word flag in subtractive form.
        assert_eq!(
            parse("co -pull back"),
            Some(BotCommand::ApplyCombatOrder { query: false,
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
                state: BotStateKind::NonCombat,
                query: false,
                add: StrategyFlags::RTSC,
                remove: StrategyFlags::NONE,
            }),
        );
        assert_eq!(
            parse("nc -rpg bg"),
            Some(BotCommand::ApplyStrategies {
                state: BotStateKind::NonCombat,
                query: false,
                add: StrategyFlags::NONE,
                remove: StrategyFlags::RPG_BG,
            }),
        );
        assert_eq!(
            parse("nc +rtsc,-rpg,-rpg bg,-rpg explore"),
            Some(BotCommand::ApplyStrategies {
                state: BotStateKind::NonCombat,
                query: false,
                add: StrategyFlags::RTSC,
                remove: StrategyFlags::RPG | StrategyFlags::RPG_BG | StrategyFlags::RPG_EXPLORE,
            }),
        );
        assert!(matches!(parse("nc +bogus"), Some(BotCommand::Unknown(_))));
        // `de` targets the Dead-state engine in PB2's 4-state model.
        assert_eq!(
            parse("de +rtsc"),
            Some(BotCommand::ApplyStrategies {
                state: BotStateKind::Dead,
                query: false,
                add: StrategyFlags::RTSC,
                remove: StrategyFlags::NONE,
            }),
        );
        // `react +flee` routes to the Reaction slot (PB2 parity).
        assert_eq!(
            parse("react +flee"),
            Some(BotCommand::ApplyStrategies {
                state: BotStateKind::Reaction,
                query: false,
                add: StrategyFlags::FLEE,
                remove: StrategyFlags::NONE,
            }),
        );
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
    fn info_command_aliases() {
        // `who` maps onto status; `where`/`position`/`pos` onto Where;
        // `help`/`commands` onto Help; `ready` onto Ready.
        assert_eq!(parse("who"), Some(BotCommand::Status));
        assert_eq!(parse("where"), Some(BotCommand::Where));
        assert_eq!(parse("position"), Some(BotCommand::Where));
        assert_eq!(parse("pos"), Some(BotCommand::Where));
        assert_eq!(parse("help"), Some(BotCommand::Help));
        assert_eq!(parse("commands"), Some(BotCommand::Help));
        assert_eq!(parse("ready"), Some(BotCommand::Ready));
    }

    #[test]
    fn wander_is_rpg_mode() {
        assert_eq!(
            parse("wander"),
            Some(BotCommand::SetMode(BehaviorMode::Rpg))
        );
    }

    #[test]
    fn tunable_commands() {
        assert_eq!(parse("range 5"), Some(BotCommand::SetRange(5.0)));
        assert!(matches!(parse("range 999"), Some(BotCommand::Unknown(_))));

        assert_eq!(parse("stance battle"), Some(BotCommand::SetStance(1)));
        assert_eq!(parse("stance 3"), Some(BotCommand::SetStance(3)));
        assert_eq!(parse("stance def"), Some(BotCommand::SetStance(2)));
        assert!(matches!(parse("stance bogus"), Some(BotCommand::Unknown(_))));

        assert_eq!(parse("max-dps"), Some(BotCommand::MaxDps));
        assert_eq!(parse("maxdps"), Some(BotCommand::MaxDps));
        assert_eq!(parse("save-mana"), Some(BotCommand::ToggleSaveMana));
        assert_eq!(parse("self-res"), Some(BotCommand::ToggleSelfRes));

        assert_eq!(parse("cheat 0x3"), Some(BotCommand::SetCheatFlags(3)));
        assert_eq!(parse("cheat 7"), Some(BotCommand::SetCheatFlags(7)));
        assert_eq!(parse("cheat off"), Some(BotCommand::SetCheatFlags(0)));

        assert_eq!(parse("keep 12345"), Some(BotCommand::KeepItem(ItemId(12345))));
        assert_eq!(
            parse("unkeep 12345"),
            Some(BotCommand::UnkeepItem(ItemId(12345))),
        );

        assert_eq!(
            parse("chat party"),
            Some(BotCommand::SetChatChannel { channel: ChatChannel::Party, on: true }),
        );
        assert_eq!(
            parse("chat guild off"),
            Some(BotCommand::SetChatChannel { channel: ChatChannel::Guild, on: false }),
        );

        assert_eq!(parse("rti skull"), Some(BotCommand::SetPreferredRti(Some(8))));
        assert_eq!(parse("rti 3"), Some(BotCommand::SetPreferredRti(Some(3))));
        assert_eq!(parse("rti clear"), Some(BotCommand::SetPreferredRti(None)));
        assert_eq!(parse("rti"), Some(BotCommand::SetPreferredRti(None)));

        assert_eq!(parse("emote 4"), Some(BotCommand::Emote(4)));
        assert_eq!(parse("debug"), Some(BotCommand::Debug));
        assert_eq!(parse("cdebug"), Some(BotCommand::Debug));
    }

    #[test]
    fn info_query_commands() {
        assert_eq!(parse("los"), Some(BotCommand::CheckLos));
        assert_eq!(parse("quests"), Some(BotCommand::ListQuests));
        assert_eq!(parse("q"), Some(BotCommand::ListQuests));
        assert_eq!(parse("talents"), Some(BotCommand::ListTalents));
        assert_eq!(parse("spells"), Some(BotCommand::ListSpells));
        assert_eq!(parse("release"), Some(BotCommand::ReleaseSpirit));
        assert_eq!(parse("revive"), Some(BotCommand::AcceptRevive));
    }

    #[test]
    fn wave2_ffi_commands() {
        assert_eq!(parse("jump"), Some(BotCommand::Jump));
        assert_eq!(parse("hearth"), Some(BotCommand::UseHearth));
        assert_eq!(parse("home"), Some(BotCommand::UseHearth));
        assert_eq!(parse("rep"), Some(BotCommand::ListReputation));
        assert_eq!(parse("reputation"), Some(BotCommand::ListReputation));
        assert_eq!(parse("skill"), Some(BotCommand::ListSkills));
        assert_eq!(parse("skills"), Some(BotCommand::ListSkills));
        assert_eq!(parse("accept"), Some(BotCommand::QuestAccept));
        assert_eq!(parse("drop 1234"), Some(BotCommand::QuestDrop(1234)));
    }

    #[test]
    fn wave3_mail_guild_commands() {
        assert_eq!(parse("mail"), Some(BotCommand::MailSummary));
        assert_eq!(parse("mail take"), Some(BotCommand::MailTakeAll));
        assert_eq!(parse("mail takeall"), Some(BotCommand::MailTakeAll));
        assert_eq!(parse("mail all"), Some(BotCommand::MailTakeAll));
        assert_eq!(parse("leave"), Some(BotCommand::GuildLeave));
    }

    #[test]
    fn mangosbot_probe_query_commands() {
        // Mangosbot addon probes every setting via the `?` query operator.
        // Each of these must yield a Query* variant (never Unknown) so the
        // addon's probe loop sees a valid response.
        assert_eq!(parse("formation ?"), Some(BotCommand::QueryFormation));
        assert_eq!(parse("stance ?"), Some(BotCommand::QueryStance));
        assert_eq!(parse("co ?"), Some(BotCommand::QueryCombatOrder));
        assert_eq!(
            parse("nc ?"),
            Some(BotCommand::QueryStrategies(BotStateKind::NonCombat))
        );
        assert_eq!(
            parse("de ?"),
            Some(BotCommand::QueryStrategies(BotStateKind::Dead))
        );
        assert_eq!(parse("react ?"), Some(BotCommand::QueryReactivity));
        assert_eq!(parse("rti ?"), Some(BotCommand::QueryRti));
        assert_eq!(parse("ll ?"), Some(BotCommand::QueryLootPolicy));
        assert_eq!(parse("save mana ?"), Some(BotCommand::QuerySaveMana));
    }

    #[test]
    fn save_mana_explicit_set() {
        assert_eq!(parse("save mana on"), Some(BotCommand::SetSaveMana(true)));
        assert_eq!(parse("save mana off"), Some(BotCommand::SetSaveMana(false)));
        assert_eq!(parse("save mana"), Some(BotCommand::ToggleSaveMana));
    }

    #[test]
    fn loot_policy_parse_forms() {
        // Bare category: full replacement — only that one category is kept.
        let all = LootPolicy::all_categories();
        assert_eq!(
            parse("ll equip"),
            Some(BotCommand::ApplyLootPolicy {
                add: LootPolicy::EQUIP,
                remove: all - LootPolicy::EQUIP,
                toggle: LootPolicy::NONE,
            }),
        );
        // Signed: add quest, remove vendor.
        assert_eq!(
            parse("ll +quest,-vendor"),
            Some(BotCommand::ApplyLootPolicy {
                add: LootPolicy::QUEST,
                remove: LootPolicy::VENDOR,
                toggle: LootPolicy::NONE,
            }),
        );
        // Toggle with ~.
        assert_eq!(
            parse("ll ~equip"),
            Some(BotCommand::ApplyLootPolicy {
                add: LootPolicy::NONE,
                remove: LootPolicy::NONE,
                toggle: LootPolicy::EQUIP,
            }),
        );
    }

    #[test]
    fn co_boost_flag_parses() {
        // RaidControl burst-cooldown keyword.
        assert_eq!(
            parse("co +boost"),
            Some(BotCommand::ApplyCombatOrder { query: false,
                add: CombatOrder::BOOST,
                remove: CombatOrder::NONE,
            }),
        );
        // Mangosbot uses `i` as alias for boost.
        assert_eq!(
            parse("co +i"),
            Some(BotCommand::ApplyCombatOrder { query: false,
                add: CombatOrder::BOOST,
                remove: CombatOrder::NONE,
            }),
        );
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
