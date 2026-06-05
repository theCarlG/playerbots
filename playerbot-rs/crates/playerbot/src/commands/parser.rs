/// Chat command parser — converts text into `BotCommand`.
///
/// Commands arrive as whispers from the master player. The C++ side routes
/// them through `playerbot_chat_command()` which calls this parser.
///
/// Design: ~20 clean commands replace the old 70+ redundant C++ commands.
/// Each command maps to exactly one `BotCommand` variant.
use crate::bot::class_prefs::{
    HunterAspect, HunterTrap, PaladinAura, PaladinBlessing, PoisonKind, ShamanImbue,
    TotemRole, TotemSlot, WarlockCurse, WarriorStance, WeaponHand,
};
use crate::bot::settings::{
    BehaviorMode, BotStateKind, ChatChannel, FollowFormation, LootPolicy, PositionStance,
    Reactivity, StrategyFlags,
};
use crate::commands::BotCommand;
use crate::data::spells::lookup_spell_by_name;
use cmangos::{ItemId, SpellId};

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
    // -- Reactivity --
    CommandSpec {
        names: &["react"],
        parse: |_, a| parse_reactivity(a),
    },
    // -- Spec selection --
    CommandSpec {
        names: &["spec"],
        parse: |_, a| parse_spec_cmd(a),
    },
    // -- Unified strategy toggles --
    CommandSpec {
        names: &["strat"],
        parse: |_, a| parse_strat_cmd(a),
    },
    CommandSpec {
        names: &["ll"],
        parse: |_, a| parse_loot_policy(a),
    },
    // Mangosbot sends the two-word form `save mana [?|on|off]`.
    CommandSpec {
        names: &["save"],
        parse: |_, a| parse_save(a),
    },
    // -- Targeting --
    CommandSpec {
        names: &["focus"],
        parse: |_, _| Some(BotCommand::Focus(None)),
    },
    CommandSpec {
        names: &["attack", "attack my target", "queue attack"],
        parse: |_, a| parse_attack(a),
    },
    CommandSpec {
        names: &["pull"],
        parse: |_, a| parse_pull(a),
    },
    CommandSpec {
        names: &["cc"],
        parse: |_, a| {
            // "cc ?" — query CC assignments.
            if a.first().map(|s| &**s) == Some("?") {
                return Some(BotCommand::CcQuery);
            }
            // "cc skull" or "cc 8" — assign this bot to CC the marked mob.
            if a.is_empty() {
                return Some(BotCommand::Unknown("cc: need target icon".into()));
            }
            let icon = match parse_rti(a[0]) {
                Some(i) => i,
                None => return Some(BotCommand::Unknown(format!("cc: unknown icon '{}'", a[0]))),
            };
            Some(BotCommand::AssignCc { icon, spell: None })
        },
    },
    CommandSpec {
        names: &["uncc"],
        parse: |_, a| {
            if a.is_empty() {
                Some(BotCommand::UnassignCc(None))
            } else {
                let icon = parse_rti(a[0]);
                Some(BotCommand::UnassignCc(icon))
            }
        },
    },
    // -- Movement --
    CommandSpec {
        names: &["come", "c"],
        parse: |_, _| Some(BotCommand::ComeToMe),
    },
    CommandSpec {
        names: &["guard"],
        parse: |_, _| Some(BotCommand::Guard),
    },
    CommandSpec {
        names: &["go"],
        parse: |_, a| parse_go(a),
    },
    // -- RTSC (Real-Time Strategy Control) --
    CommandSpec {
        names: &["rtsc"],
        parse: |_, a| parse_rtsc(a),
    },
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
    CommandSpec {
        names: &["repair"],
        parse: |_, _| Some(BotCommand::Repair),
    },
    CommandSpec {
        names: &["vendor", "sell"],
        parse: |_, _| Some(BotCommand::Vendor),
    },
    // -- Healing --
    CommandSpec {
        names: &["heal"],
        parse: |_, a| parse_heal_threshold(a),
    },
    // -- Information --
    CommandSpec {
        names: &["status", "stats", "who"],
        parse: |_, _| Some(BotCommand::Status),
    },
    CommandSpec {
        names: &["settings"],
        parse: |_, _| Some(BotCommand::ListSettings),
    },
    CommandSpec {
        names: &["where", "position", "pos"],
        parse: |_, _| Some(BotCommand::Where),
    },
    CommandSpec {
        names: &["help", "commands"],
        parse: |_, _| Some(BotCommand::Help),
    },
    CommandSpec {
        names: &["ready"],
        parse: |_, _| Some(BotCommand::Ready),
    },
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
    CommandSpec {
        names: &["mount", "dismount"],
        parse: |_, _| Some(BotCommand::Mount),
    },
    CommandSpec {
        names: &["rez", "resurrect"],
        parse: |_, _| Some(BotCommand::Resurrect),
    },
    // -- Panic / aliases --
    CommandSpec {
        names: &["flee", "runaway", "panic"],
        parse: |_, _| Some(BotCommand::Flee),
    },
    CommandSpec {
        names: &["free"],
        parse: |_, _| Some(BotCommand::Free),
    },
    CommandSpec {
        names: &["summon"],
        parse: |_, _| Some(BotCommand::Summon),
    },
    // -- Cast a named spell once (addon sends `cast Taunt`). --
    CommandSpec {
        names: &["cast"],
        parse: |_, a| parse_cast(a),
    },
    // RaidControl sends bare spell names without `cast` prefix.
    CommandSpec {
        names: &["remove curse"],
        parse: |_, _| parse_cast(&["remove", "curse"]),
    },
    CommandSpec {
        names: &["dispel magic"],
        parse: |_, _| parse_cast(&["dispel", "magic"]),
    },
    // -- Formation --
    CommandSpec {
        names: &["formation"],
        parse: |_, a| parse_formation(a),
    },
    // -- Named-location travel --
    CommandSpec {
        names: &["travel", "goto"],
        parse: |_, a| parse_travel(a),
    },
    // -- Tunables / PB2 Wave 1 --
    CommandSpec {
        names: &["range", "ra"],
        parse: |_, a| parse_range(a),
    },
    // `stop` — stop current action / go passive.
    CommandSpec {
        names: &["stop"],
        parse: |_, _| Some(BotCommand::Stop),
    },
    // `u <item name>` / `use <item name>` — use an item by name.
    CommandSpec {
        names: &["u", "use"],
        parse: |_, a| parse_use_item(a),
    },
    // `e <item name>` / `equip <item name>` — equip an item by name.
    CommandSpec {
        names: &["e", "equip"],
        parse: |_, a| parse_equip_item(a),
    },
    CommandSpec {
        names: &["stance"],
        parse: |_, a| parse_stance(a),
    },
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
    CommandSpec {
        names: &["cheat"],
        parse: |_, a| parse_cheat(a),
    },
    CommandSpec {
        names: &["keep"],
        parse: |_, a| parse_keep(a, true),
    },
    CommandSpec {
        names: &["unkeep"],
        parse: |_, a| parse_keep(a, false),
    },
    CommandSpec {
        names: &["chat"],
        parse: |_, a| parse_chat(a),
    },
    CommandSpec {
        names: &["rti"],
        parse: |_, a| parse_rti_cmd(a),
    },
    CommandSpec {
        names: &["emote"],
        parse: |_, a| parse_emote(a),
    },
    CommandSpec {
        names: &["debug", "cdebug"],
        parse: |_, a| match a.first().map(|s| &**s) {
            Some("fsm") => Some(BotCommand::DebugFsm),
            Some("claims" | "claim") => Some(BotCommand::DebugClaims),
            Some("coord" | "coordination" | "group") => Some(BotCommand::DebugCoord),
            Some("bt") => Some(BotCommand::DebugBt),
            _ => Some(BotCommand::Debug),
        },
    },
    // -- Wave 2 info queries (reuse existing FFI) --
    CommandSpec {
        names: &["los"],
        parse: |_, _| Some(BotCommand::CheckLos),
    },
    CommandSpec {
        names: &["quests", "q"],
        parse: |_, _| Some(BotCommand::ListQuests),
    },
    CommandSpec {
        names: &["talents"],
        parse: |_, _| Some(BotCommand::ListTalents),
    },
    CommandSpec {
        names: &["spells"],
        parse: |_, _| Some(BotCommand::ListSpells),
    },
    CommandSpec {
        names: &["inventory", "inv", "count"],
        parse: |_, _| Some(BotCommand::ListInventory),
    },
    CommandSpec {
        names: &["release"],
        parse: |_, _| Some(BotCommand::ReleaseSpirit),
    },
    CommandSpec {
        names: &["revive"],
        parse: |_, _| Some(BotCommand::AcceptRevive),
    },
    CommandSpec {
        names: &["jump"],
        parse: |_, _| Some(BotCommand::Jump),
    },
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
            Some("take" | "takeall" | "all") => {
                // `mail take <N>` — take specific mail by 1-based index.
                if let Some(idx_str) = a.get(1)
                    && let Ok(idx) = idx_str.parse::<u32>() {
                        return Some(BotCommand::MailTakeIndex(idx));
                    }
                Some(BotCommand::MailTakeAll)
            }
            Some("?") => Some(BotCommand::ListMailItems),
            _ => Some(BotCommand::MailSummary),
        },
    },
    CommandSpec {
        names: &["leave"],
        parse: |_, _| Some(BotCommand::LeaveGroup),
    },
    // -- Class preferences --
    CommandSpec {
        names: &["poison", "poisons"],
        parse: |_, a| parse_poison(a),
    },
    CommandSpec {
        names: &["totem", "totems"],
        parse: |_, a| parse_totem(a),
    },
    CommandSpec {
        names: &["imbue", "imbues"],
        parse: |_, a| parse_imbue(a),
    },
    CommandSpec {
        names: &["aura", "auras"],
        parse: |_, a| parse_aura(a),
    },
    CommandSpec {
        names: &["blessing", "blessings"],
        parse: |_, a| parse_blessing(a),
    },
    CommandSpec {
        names: &["aspect", "aspects"],
        parse: |_, a| parse_aspect(a),
    },
    CommandSpec {
        names: &["trap", "traps"],
        parse: |_, a| parse_trap(a),
    },
    CommandSpec {
        names: &["curse", "curses"],
        parse: |_, a| parse_curse(a),
    },
    CommandSpec {
        names: &["forcestance", "stancelock"],
        parse: |_, a| parse_forcestance(a),
    },
    CommandSpec {
        names: &["suppression"],
        parse: |_, a| parse_duty(a, DutyKind::Suppression),
    },
    CommandSpec {
        names: &["douse"],
        parse: |_, a| parse_duty(a, DutyKind::Douse),
    },
    // -- BDI personality --
    CommandSpec {
        names: &["personality"],
        parse: |_, a| parse_personality(a),
    },
    // -- PB2 parity: remaining commands --
    // Single-letter shortcuts from PB2
    CommandSpec {
        names: &["s"],
        parse: |_, a| {
            if a.is_empty() {
                Some(BotCommand::Vendor)
            } else {
                Some(BotCommand::SellItemByName(a.join(" ")))
            }
        },
    }, // sell
    CommandSpec {
        names: &["b"],
        parse: |_, _| Some(BotCommand::Buy),
    },
    CommandSpec {
        names: &["bb"],
        parse: |_, _| Some(BotCommand::Buyback),
    },
    CommandSpec {
        names: &["r"],
        parse: |_, _| Some(BotCommand::QuestReward),
    },
    CommandSpec {
        names: &["t", "nt"],
        parse: |_, a| {
            if a.is_empty() {
                return Some(BotCommand::Trade);
            }
            // `t <itemlink> [count]` — add item to trade window.
            // Last arg may be a count (e.g. "6" for all stacks).
            let last = a.last().unwrap();
            let (name_args, count) = if let Ok(n) = last.parse::<u32>() {
                if a.len() > 1 { (&a[..a.len()-1], n) } else { (a, 0u32) }
            } else {
                (a, 0u32)
            };
            Some(BotCommand::TradeAddItem {
                name: name_args.join(" "),
                count,
            })
        },
    },
    CommandSpec {
        names: &["ue"],
        parse: |_, a| Some(BotCommand::UnequipItemByName(a.join(" "))),
    },
    // PB2 targeted commands
    CommandSpec {
        names: &["wait for attack"],
        parse: |_, a| {
            let secs = a.first().and_then(|s| s.parse::<u32>().ok()).unwrap_or(3);
            Some(BotCommand::SetWaitForAttack(secs))
        },
    },
    CommandSpec {
        names: &["tank attack"],
        parse: |_, _| Some(BotCommand::TankAttack),
    },
    CommandSpec {
        names: &["loot", "add all loot"],
        parse: |_, _| Some(BotCommand::Loot),
    },
    CommandSpec {
        names: &["destroy"],
        parse: |_, a| Some(BotCommand::DestroyItem(a.join(" "))),
    },
    CommandSpec {
        names: &["ss"],
        parse: |_, a| Some(BotCommand::SkipSpell(a.join(" "))),
    },
    CommandSpec {
        names: &["roll"],
        parse: |_, a| {
            Some(BotCommand::LootRoll(
                a.first().unwrap_or(&"pass").to_string(),
            ))
        },
    },
    CommandSpec {
        names: &["give leader"],
        parse: |_, _| Some(BotCommand::GiveLeader),
    },
    CommandSpec {
        names: &["invite"],
        parse: |_, a| Some(BotCommand::InvitePlayer(a.join(" "))),
    },
    CommandSpec {
        names: &["pet"],
        parse: |_, a| Some(BotCommand::Pet(a.join(" "))),
    },
    CommandSpec {
        names: &["buff target"],
        parse: |_, a| Some(BotCommand::BuffTarget(a.join(" "))),
    },
    CommandSpec {
        names: &["boost target"],
        parse: |_, a| Some(BotCommand::BoostTarget(a.join(" "))),
    },
    CommandSpec {
        names: &["revive target"],
        parse: |_, a| Some(BotCommand::ReviveTarget(a.join(" "))),
    },
    CommandSpec {
        names: &["follow target"],
        parse: |_, a| Some(BotCommand::FollowTarget(a.join(" "))),
    },
    CommandSpec {
        names: &["focus heal"],
        parse: |_, a| Some(BotCommand::FocusHeal(a.join(" "))),
    },
    CommandSpec {
        names: &["max dps"],
        parse: |_, _| Some(BotCommand::MaxDps),
    },
    CommandSpec {
        names: &["self res"],
        parse: |_, _| Some(BotCommand::ToggleSelfRes),
    },
    // NOTE: "save mana" is handled by the single-word "save" entry which
    // checks args[0]=="mana". We must NOT add a multi-word "save mana" entry
    // here because it would steal args from parse_save().
    CommandSpec {
        names: &["move style"],
        parse: |_, a| {
            Some(BotCommand::MoveStyle(
                a.first().unwrap_or(&"run").to_string(),
            ))
        },
    },
    CommandSpec {
        names: &["talk"],
        parse: |_, _| Some(BotCommand::Talk),
    },
    CommandSpec {
        names: &["trainer"],
        parse: |_, _| Some(BotCommand::Trainer),
    },
    CommandSpec {
        names: &["taxi"],
        parse: |_, _| Some(BotCommand::Taxi),
    },
    CommandSpec {
        names: &["craft"],
        parse: |_, a| Some(BotCommand::Craft(a.join(" "))),
    },
    CommandSpec {
        names: &["outfit"],
        parse: |_, a| Some(BotCommand::Outfit(a.join(" "))),
    },
    CommandSpec {
        names: &["log"],
        parse: |_, a| Some(BotCommand::LogLevel(a.join(" "))),
    },
    CommandSpec {
        names: &["share"],
        parse: |_, _| Some(BotCommand::ShareQuest),
    },
    CommandSpec {
        names: &["doquest"],
        parse: |_, a| Some(BotCommand::DoQuest(a.join(" "))),
    },
    CommandSpec {
        names: &["bank", "gb", "gbank"],
        parse: |_, a| {
            if a.is_empty() {
                return Some(BotCommand::Bank);
            }
            match a.first().copied() {
                Some("?") => Some(BotCommand::ListBankItems),
                Some(first) if first.starts_with('-') => {
                    // bank -<itemlink> = withdraw
                    let mut s = first[1..].to_string();
                    for arg in &a[1..] {
                        s.push(' ');
                        s.push_str(arg);
                    }
                    Some(BotCommand::BankWithdrawItem(s))
                }
                _ => Some(BotCommand::BankDepositItem(a.join(" "))),
            }
        },
    },
    CommandSpec {
        names: &["ah"],
        parse: |_, a| Some(BotCommand::AuctionHouse(a.join(" "))),
    },
    CommandSpec {
        names: &[
            "guild invite",
            "guild join",
            "guild promote",
            "guild demote",
            "guild remove",
            "guild leader",
        ],
        parse: |cmd, _| Some(BotCommand::GuildCommand(cmd.to_string())),
    },
    CommandSpec {
        names: &["bg free"],
        parse: |_, _| Some(BotCommand::BgFree),
    },
    CommandSpec {
        names: &["flag"],
        parse: |_, _| Some(BotCommand::Flag),
    },
    CommandSpec {
        names: &["sendmail"],
        parse: |_, a| Some(BotCommand::SendMail(a.join(" "))),
    },
    CommandSpec {
        names: &["possible attack targets"],
        parse: |_, _| Some(BotCommand::PossibleAttackTargets),
    },
    CommandSpec {
        names: &["attackers"],
        parse: |_, _| Some(BotCommand::ShowAttackers),
    },
    // PB2 commands that map to existing functionality or are no-ops
    CommandSpec {
        names: &["load ai", "list ai", "save ai"],
        parse: |cmd, a| {
            Some(BotCommand::AiProfile(
                format!("{} {}", cmd, a.join(" ")).trim().to_string(),
            ))
        },
    },
    CommandSpec {
        names: &["reset strats"],
        parse: |_, _| Some(BotCommand::ResetStrategies),
    },
    CommandSpec {
        names: &["reset values"],
        parse: |_, _| Some(BotCommand::Reset),
    },
    CommandSpec {
        names: &["cs"],
        parse: |_, a| Some(BotCommand::CustomStrategy(a.join(" "))),
    },
    CommandSpec {
        names: &["wts"],
        parse: |_, _| Some(BotCommand::WhatToSell),
    },
    CommandSpec {
        names: &["teleport"],
        parse: |_, a| Some(BotCommand::Teleport(a.join(" "))),
    },
    CommandSpec {
        names: &["hire"],
        parse: |_, _| Some(BotCommand::Trainer),
    }, // hire = trainer (PB2 alias)
    CommandSpec {
        names: &["glyph"],
        parse: |_, _| Some(BotCommand::Trainer),
    }, // glyph = trainer (PB2 alias for glyph master)
    CommandSpec {
        names: &["faction"],
        parse: |_, _| Some(BotCommand::ShowFaction),
    },
    CommandSpec {
        names: &["set value"],
        parse: |_, a| Some(BotCommand::SetValue(a.join(" "))),
    },
    CommandSpec {
        names: &["speak"],
        parse: |_, a| Some(BotCommand::Speak(a.join(" "))),
    },
    CommandSpec {
        names: &["warning"],
        parse: |_, _| Some(BotCommand::Flee),
    }, // warning = flee
    CommandSpec {
        names: &["runaway"],
        parse: |_, _| Some(BotCommand::Flee),
    }, // alias
    CommandSpec {
        names: &["quest reward", "reward"],
        parse: |_, _| Some(BotCommand::QuestReward),
    },
    CommandSpec {
        names: &["lfg"],
        parse: |_, _| Some(BotCommand::Lfg),
    },
    CommandSpec {
        names: &["set mt", "set maintank"],
        parse: |_, _| Some(BotCommand::SetMainTank),
    },
    CommandSpec {
        names: &["set ot", "set offtank"],
        parse: |_, _| Some(BotCommand::SetOffTank),
    },
    CommandSpec {
        names: &["unset mt", "unset ot", "unset tank"],
        parse: |_, _| Some(BotCommand::UnsetTank),
    },
    CommandSpec {
        names: &["assist"],
        parse: |_, a| {
            if a.is_empty() {
                Some(BotCommand::SetMainAssist(None))
            } else {
                Some(BotCommand::SetMainAssistByName(a.join(" ")))
            }
        },
    },
    CommandSpec {
        names: &["tank"],
        parse: |_, a| {
            // "tank ?" — query tank role and assigned targets.
            if a.first().map(|s| &**s) == Some("?") {
                return Some(BotCommand::TankQuery);
            }
            // "tank skull" or "tank 8" — assign tank focus target.
            if a.is_empty() {
                return Some(BotCommand::Unknown("tank: need target icon".into()));
            }
            let icon = match parse_rti(a[0]) {
                Some(i) => i,
                None => {
                    return Some(BotCommand::Unknown(format!(
                        "tank: unknown icon '{}'",
                        a[0]
                    )));
                }
            };
            Some(BotCommand::AssignTankTarget(icon))
        },
    },
    CommandSpec {
        names: &["untank"],
        parse: |_, a| {
            if a.is_empty() {
                Some(BotCommand::UnassignTankTarget(None))
            } else {
                let icon = parse_rti(a[0]);
                Some(BotCommand::UnassignTankTarget(icon))
            }
        },
    },
    CommandSpec {
        names: &["monitor"],
        parse: |_, a| {
            match a.first().map(|s| &**s) {
                Some("on" | "1" | "true") => Some(BotCommand::SetMonitor(true)),
                Some("off" | "0" | "false") => Some(BotCommand::SetMonitor(false)),
                _ => Some(BotCommand::SetMonitor(true)), // bare "monitor" toggles on
            }
        },
    },
    // -- enable/disable: apply a strategy flag to both Combat and NonCombat --
    CommandSpec {
        names: &["enable"],
        parse: |_, a| {
            let name = a.join(" ");
            let none = StrategyFlags::NONE;
            match StrategyFlags::parse_name(&name) {
                Some(flag) => Some(BotCommand::Batch(vec![
                    BotCommand::ApplyStrategies {
                        state: BotStateKind::Combat,
                        add: flag,
                        remove: none,
                        toggle: none,
                        query: false,
                    },
                    BotCommand::ApplyStrategies {
                        state: BotStateKind::NonCombat,
                        add: flag,
                        remove: none,
                        toggle: none,
                        query: false,
                    },
                ])),
                None => Some(BotCommand::Unknown(format!("enable {name}"))),
            }
        },
    },
    CommandSpec {
        names: &["disable"],
        parse: |_, a| {
            let name = a.join(" ");
            let none = StrategyFlags::NONE;
            match StrategyFlags::parse_name(&name) {
                Some(flag) => Some(BotCommand::Batch(vec![
                    BotCommand::ApplyStrategies {
                        state: BotStateKind::Combat,
                        add: none,
                        remove: flag,
                        toggle: none,
                        query: false,
                    },
                    BotCommand::ApplyStrategies {
                        state: BotStateKind::NonCombat,
                        add: none,
                        remove: flag,
                        toggle: none,
                        query: false,
                    },
                ])),
                None => Some(BotCommand::Unknown(format!("disable {name}"))),
            }
        },
    },
    // -- preset: apply a named strategy profile --
    CommandSpec {
        names: &["preset"],
        parse: |_, a| Some(BotCommand::Preset(a.join(" "))),
    },
    // -- addon subscriptions --
    CommandSpec {
        names: &["subscribe", "sub"],
        parse: |_, a| {
            if a.is_empty() {
                Some(BotCommand::Subscribe("all".to_string()))
            } else {
                Some(BotCommand::Subscribe(a[0].to_string()))
            }
        },
    },
    CommandSpec {
        names: &["unsubscribe", "unsub"],
        parse: |_, a| {
            if a.is_empty() {
                Some(BotCommand::Unsubscribe(None))
            } else {
                Some(BotCommand::Unsubscribe(Some(a[0].to_string())))
            }
        },
    },
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

    // Try multi-word command names first (longest match wins).
    // Check 4-word, 3-word, 2-word, then 1-word prefixes.
    for word_count in (1..=parts.len().min(4)).rev() {
        let candidate = parts[..word_count].join(" ");
        let args = &parts[word_count..];
        for spec in COMMANDS {
            if spec.names.iter().any(|n| *n == candidate) {
                return (spec.parse)(&candidate, args);
            }
        }
    }
    Some(BotCommand::Unknown(text.to_string()))
}

/// Parse `co` arguments — routes through the Combat strategy slot.
///
/// Bare form (no sign): `co tank` → full replace with that flag.
/// Signed form: `co +tank -fury`, `co +tank assist,+dps assist` → additive/subtractive edit.
/// Query form: `co ?` → report current combat strategies.
///
/// This is the unified path: `co` now reads/writes the same `StrategyFlags`
/// bitfield that the BT's `StrategyEnabled` checks use, eliminating the old
/// `CombatOrder` ↔ `StrategyFlags` split.
/// `react passive|defensive|aggressive` — set bot reactivity level.
fn parse_reactivity(args: &[&str]) -> Option<BotCommand> {
    match args.first().copied() {
        None | Some("?") => Some(BotCommand::Unknown(
            "react: need level (passive/defensive/aggressive)".into(),
        )),
        Some("passive") => Some(BotCommand::SetReactivity(Reactivity::Passive)),
        Some("defensive") => Some(BotCommand::SetReactivity(Reactivity::Defensive)),
        Some("aggressive") => Some(BotCommand::SetReactivity(Reactivity::Aggressive)),
        _ => Some(BotCommand::Unknown(
            "react: unknown level (passive/defensive/aggressive)".into(),
        )),
    }
}

/// `spec <name>` — set the bot's specialization.
/// Examples: `spec arms`, `spec holy`, `spec feral`, `spec protection`
///
/// The spec name is stored as a raw string and resolved at execution time
/// using the bot's class, so ambiguous names like "holy" (priest/paladin)
/// or "protection" (warrior/paladin) work correctly for any class.
fn parse_spec_cmd(args: &[&str]) -> Option<BotCommand> {
    if args.is_empty() || args == ["?"] {
        return Some(BotCommand::Unknown("spec: need a spec name".into()));
    }
    let name = args.join(" ").to_lowercase();
    Some(BotCommand::SetSpec(name))
}

/// `strat [+/-/~]flag,flag,...[,?]` — unified strategy toggle.
/// Applies to all state slots (combat, noncombat, reaction, dead).
/// Examples: `strat ~aoe,?`, `strat +cc`, `strat -stealth,+close`
fn parse_strat_cmd(args: &[&str]) -> Option<BotCommand> {
    if args.is_empty() || args == ["?"] {
        return Some(BotCommand::QueryAllStrategies);
    }
    let joined = args.join(" ");
    let normalized = joined.replace(',', " ");
    let tokens: Vec<&str> = normalized.split_whitespace().collect();

    let mut add = StrategyFlags::NONE;
    let mut remove = StrategyFlags::NONE;
    let mut toggle = StrategyFlags::NONE;
    let mut query = false;

    let mut i = 0;
    while i < tokens.len() {
        let tok = tokens[i];
        if tok == "?" {
            query = true;
            i += 1;
            continue;
        }
        let (sign, first_word) = match tok.as_bytes().first() {
            Some(b'+') => (1i8, &tok[1..]),
            Some(b'-') => (-1i8, &tok[1..]),
            Some(b'~') => (0i8, &tok[1..]),
            _ => {
                i += 1;
                continue;
            }
        };
        if first_word.is_empty() {
            i += 1;
            continue;
        }
        // Greedy multi-word match: try 3-word, 2-word, 1-word.
        let mut matched = false;
        for window_len in (1..=3).rev() {
            if i + window_len > tokens.len() {
                continue;
            }
            let mut name_parts = vec![first_word];
            for j in 1..window_len {
                let next = tokens[i + j];
                if next.starts_with('+')
                    || next.starts_with('-')
                    || next.starts_with('~')
                    || next == "?"
                {
                    break;
                }
                name_parts.push(next);
            }
            if name_parts.len() < window_len {
                continue;
            }
            let candidate = name_parts.join(" ");
            if let Some(flag) = StrategyFlags::parse_name(&candidate) {
                match sign {
                    1 => add.insert(flag),
                    -1 => remove.insert(flag),
                    _ => toggle.insert(flag),
                }
                i += window_len;
                matched = true;
                break;
            }
        }
        if !matched {
            i += 1;
        }
    }

    Some(BotCommand::ToggleStrategies { add, remove, toggle, query })
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
        Some(BotCommand::ApplyLootPolicy {
            add,
            remove,
            toggle,
        })
    }
}

/// `save <subcommand>` — currently only `save mana [?|on|off]`.
fn parse_save(args: &[&str]) -> Option<BotCommand> {
    match args.first().copied() {
        Some("mana") => match args.get(1).copied() {
            None => Some(BotCommand::ToggleSaveMana),
            Some("?") => Some(BotCommand::QuerySaveMana),
            Some("on" | "yes" | "true") => Some(BotCommand::SetSaveMana(1)),
            Some("off" | "0" | "no" | "false") => {
                Some(BotCommand::SetSaveMana(0))
            }
            Some("toggle" | "~") => Some(BotCommand::ToggleSaveMana),
            Some(n) => {
                // Accept numeric levels 1-5.
                match n.parse::<u8>() {
                    Ok(level @ 1..=5) => Some(BotCommand::SetSaveMana(level)),
                    _ => Some(BotCommand::SetSaveMana(1)),
                }
            }
        },
        _ => Some(BotCommand::Unknown(
            "save: expected `save mana [?|on|off|1-5]`".into(),
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
        Some("reset") => Some(BotCommand::RtscReset),
        Some("last") => Some(BotCommand::RtscLast),
        Some("jump") => {
            if args.get(1).copied() == Some("reset") {
                Some(BotCommand::RtscJumpReset)
            } else {
                Some(BotCommand::RtscJump)
            }
        }
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
            Some("exact" | "selected") => {
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
        Some("file") => parse_rtsc_file(&args[1..]),
        _ => Some(BotCommand::Unknown(
            "rtsc: select/cancel/toggle/reset/move/save/unsave/go/show/last/jump/file".into(),
        )),
    }
}

/// Parse the tail of `rtsc file save|load <file> [name_glob] [bot_glob]`.
///
/// PB2 reference: `RtscAction.cpp:104-270`. The PB2 parser requires at
/// least two arguments after `file` (the verb + filename); if
/// `name_glob` is missing it defaults to `"*"`. When `bot_glob` is
/// absent PB2 restricts the export/import to the executing bot only —
/// we model that by leaving `bot_glob` as `None`.
fn parse_rtsc_file(args: &[&str]) -> Option<BotCommand> {
    let verb = args.first().copied()?;
    let file = match args.get(1) {
        Some(f) if !f.is_empty() => (*f).to_string(),
        _ => {
            return Some(BotCommand::Unknown(
                "rtsc file needs at least 2 parameters: \
                 rtsc file save|load <file> [name_glob] [bot_glob]"
                    .into(),
            ));
        }
    };
    let name_glob = args.get(2).copied().unwrap_or("*").to_string();
    let bot_glob = args.get(3).map(|s| (*s).to_string());
    match verb {
        "save" => Some(BotCommand::RtscFileSave {
            file,
            name_glob,
            bot_glob,
        }),
        "load" => Some(BotCommand::RtscFileLoad {
            file,
            name_glob,
            bot_glob,
        }),
        _ => Some(BotCommand::Unknown("rtsc file: expected save|load".into())),
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
        && (1..=8).contains(&n)
    {
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
        Some("rti") => Some(BotCommand::PullRti(8)),
        Some(first) => {
            if let Some(icon) = parse_rti(first) {
                return Some(BotCommand::PullRti(icon));
            }
            // Unknown arg — treat as bare pull (master's target).
            Some(BotCommand::Pull(None))
        }
        // Bare `pull` — pull master's current target.
        None => Some(BotCommand::Pull(None)),
    }
}


/// `cast <spell name>` or `cast self <spell name>`. Uses the spell-name
/// table in `data::spells` — anything not there falls through to FFI.
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
        None => Some(BotCommand::CastByName { name, on_self }),
    }
}

fn parse_use_item(args: &[&str]) -> Option<BotCommand> {
    if args.is_empty() {
        return Some(BotCommand::Unknown("use: missing item name".into()));
    }
    let name = args.join(" ");
    Some(BotCommand::UseItemByName(name))
}

fn parse_equip_item(args: &[&str]) -> Option<BotCommand> {
    if args.is_empty() {
        return Some(BotCommand::Unknown("equip: missing item name".into()));
    }
    if args == ["?"] {
        return Some(BotCommand::ListEquipment);
    }
    let name = args.join(" ");
    Some(BotCommand::EquipItemByName(name))
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
    match args {
        ["?"] => Some(BotCommand::QueryRange),
        [qualifier, val] => {
            if *qualifier == "?" {
                return Some(BotCommand::QueryRange);
            }
            match val.parse::<f32>() {
                Ok(d) => Some(BotCommand::SetRangeQualified {
                    qualifier: qualifier.to_string(),
                    value: d,
                }),
                Err(_) if *val == "?" => Some(BotCommand::QueryRange),
                _ => Some(BotCommand::Unknown(format!("range: invalid value `{val}`"))),
            }
        }
        [val] => match val.parse::<f32>() {
            Ok(d) if (0.0..=100.0).contains(&d) => Some(BotCommand::SetRange(d)),
            _ => Some(BotCommand::Unknown("range: need yards (0-100)".into())),
        },
        [] => Some(BotCommand::QueryRange),
        _ => Some(BotCommand::Unknown("range: bad arguments".into())),
    }
}

fn parse_stance(args: &[&str]) -> Option<BotCommand> {
    // `stance` has two roles in PB2's vocabulary:
    //
    //  1. Warrior combat stance: `stance battle`, `stance defensive`,
    //     `stance berserker`, `stance 1`/`2`/`3`.
    //
    //  2. Positioning stance (Mangosbot addon toolbar): `stance near`,
    //     `stance behind`, `stance tank`, `stance turnback`. These
    //     toggle strategy flags that gate reactive positioning subtrees.
    //
    // The parser tries positioning keywords first, then warrior stances.
    let Some(first) = args.first().copied() else {
        return Some(BotCommand::QueryStance);
    };
    if first == "?" {
        return Some(BotCommand::QueryStance);
    }
    // Positioning stances.
    if let Some(ps) = PositionStance::from_str(first) {
        return Some(BotCommand::SetPositionStance(ps));
    }
    // Warrior combat stances.
    let st: u8 = match first {
        "0" => 0,
        "battle" | "1" => 1,
        "defensive" | "def" | "2" => 2,
        "berserker" | "zerk" | "3" => 3,
        _ => {
            return Some(BotCommand::Unknown(format!("stance: unknown `{first}`")));
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
    let on = !matches!(args.get(1).copied(), Some("off" | "no" | "0"));
    Some(BotCommand::SetChatChannel { channel, on })
}

fn parse_rti_cmd(args: &[&str]) -> Option<BotCommand> {
    // `rti cc ...` — CC raid-target preference (Mangosbot's per-bot CC mark).
    if args.first().copied() == Some("cc") {
        return match args.get(1).copied() {
            Some("?") => Some(BotCommand::QueryCcRti),
            None | Some("clear" | "none") => Some(BotCommand::SetPreferredCcRti(None)),
            Some(tok) => match parse_rti(tok) {
                Some(icon) => Some(BotCommand::SetPreferredCcRti(Some(icon))),
                None => Some(BotCommand::Unknown(format!("rti cc: unknown `{tok}`"))),
            },
        };
    }
    match args.first().copied() {
        Some("?") => Some(BotCommand::QueryRti),
        None | Some("clear" | "none") => Some(BotCommand::SetPreferredRti(None)),
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
/// `personality ?`                         → `ShowPersonality`
/// `personality aggression 0.8`            → `SetPersonality { dimension, value }`
fn parse_personality(args: &[&str]) -> Option<BotCommand> {
    match args {
        [] | ["?"] => Some(BotCommand::ShowPersonality),
        [dim, val] => {
            if let Ok(v) = val.parse::<f32>() {
                Some(BotCommand::SetPersonality {
                    dimension: dim.to_lowercase(),
                    value: v.clamp(0.0, 1.0),
                })
            } else {
                Some(BotCommand::Unknown(format!("personality: invalid value '{val}'")))
            }
        }
        _ => Some(BotCommand::Unknown("personality: usage: personality <dimension> <value>".into())),
    }
}

/// `douse auto|forbid|force`              → `SetDouseDuty(..)`
fn parse_duty(args: &[&str], kind: DutyKind) -> Option<BotCommand> {
    use crate::bot::encounter_prefs::DutyMode;
    let Some(&tok) = args.first() else {
        return Some(BotCommand::ShowEncounterPrefs);
    };
    if let Some(mode) = DutyMode::from_word(tok) { Some(match kind {
        DutyKind::Suppression => BotCommand::SetSuppressionDuty(mode),
        DutyKind::Douse => BotCommand::SetDouseDuty(mode),
    }) } else {
        let label = match kind {
            DutyKind::Suppression => "suppression",
            DutyKind::Douse => "douse",
        };
        Some(BotCommand::Unknown(format!("{label}: unknown '{tok}'")))
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

/// Try to parse a PB2 compound strategy name as a class-pref side-effect
/// command. The Mangosbot addon sends class preferences as strategy toggles
/// (e.g. `co +poison main deadly,?`). PB2 had dedicated Strategy classes for
/// each; the Rust port uses `ClassPrefs` + `StrategyFlags`. This bridge maps
/// the compound name to both:
///   - the base strategy flag (POISONS, TOTEMS, CURSE, etc.) — returned
///   - an optional class-pref `BotCommand` (`SetPoison`, `SetTotem`, etc.) — returned
///
/// When the name matches, the caller should:
///   1. Insert the flag into add/remove/toggle as appropriate.
///   2. Emit the class-pref command alongside the `ApplyStrategies`.

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
    fn spec_command() {
        // SetSpec now carries the raw name string; resolution is deferred to
        // execution time using the bot's class.
        assert_eq!(parse("spec arms"), Some(BotCommand::SetSpec("arms".into())));
        assert_eq!(parse("spec fury"), Some(BotCommand::SetSpec("fury".into())));
        assert_eq!(parse("spec feral"), Some(BotCommand::SetSpec("feral".into())));
        assert_eq!(parse("spec shadow"), Some(BotCommand::SetSpec("shadow".into())));
        assert_eq!(parse("spec beast mastery"), Some(BotCommand::SetSpec("beast mastery".into())));
        // Even unknown names pass through — the resolution happens at execution time.
        assert_eq!(parse("spec bogus"), Some(BotCommand::SetSpec("bogus".into())));
    }

    #[test]
    fn strat_command() {
        use crate::bot::settings::StrategyFlags as F;
        assert_eq!(
            parse("strat +aoe"),
            Some(BotCommand::ToggleStrategies {
                add: F::AOE,
                remove: F::NONE,
                toggle: F::NONE,
                query: false,
            }),
        );
        assert_eq!(
            parse("strat ~threat,?"),
            Some(BotCommand::ToggleStrategies {
                add: F::NONE,
                remove: F::NONE,
                toggle: F::THREAT,
                query: true,
            }),
        );
        assert_eq!(
            parse("strat -close,+ranged"),
            Some(BotCommand::ToggleStrategies {
                add: F::RANGED,
                remove: F::CLOSE,
                toggle: F::NONE,
                query: false,
            }),
        );
        assert_eq!(parse("strat ?"), Some(BotCommand::QueryAllStrategies));
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
    fn parse_rtsc_reset_last_jump() {
        assert_eq!(parse("rtsc reset"), Some(BotCommand::RtscReset));
        assert_eq!(parse("rtsc last"), Some(BotCommand::RtscLast));
        assert_eq!(parse("rtsc jump"), Some(BotCommand::RtscJump));
        assert_eq!(parse("rtsc jump reset"), Some(BotCommand::RtscJumpReset));
    }

    #[test]
    fn parse_rtsc_file_commands() {
        // Minimum form: save/load + filename → name_glob defaults to
        // "*", bot_glob stays None (PB2: bot-only scope).
        assert_eq!(
            parse("rtsc file save mc_positions"),
            Some(BotCommand::RtscFileSave {
                file: "mc_positions".into(),
                name_glob: "*".into(),
                bot_glob: None,
            })
        );
        assert_eq!(
            parse("rtsc file load mc_positions"),
            Some(BotCommand::RtscFileLoad {
                file: "mc_positions".into(),
                name_glob: "*".into(),
                bot_glob: None,
            })
        );

        // With name glob.
        assert_eq!(
            parse("rtsc file save mc magmadar"),
            Some(BotCommand::RtscFileSave {
                file: "mc".into(),
                name_glob: "magmadar".into(),
                bot_glob: None,
            })
        );

        // With name glob + bot glob — PB2 widens the scope to all
        // matching group bots when the 4th arg is present. Note that
        // `parse()` lowercases the whole command line (parser.rs:221),
        // so the PB2 sentinel `BOTNAME` arrives as `botname` here.
        // Step 10's file writer must do a case-insensitive replacement
        // of `botname` with the bot's actual name when serializing.
        assert_eq!(
            parse("rtsc file save mc * BOTNAME"),
            Some(BotCommand::RtscFileSave {
                file: "mc".into(),
                name_glob: "*".into(),
                bot_glob: Some("botname".into()),
            })
        );
    }

    #[test]
    fn parse_rtsc_file_rejects_missing_filename() {
        assert!(matches!(
            parse("rtsc file save"),
            Some(BotCommand::Unknown(_))
        ));
        assert!(matches!(
            parse("rtsc file load"),
            Some(BotCommand::Unknown(_))
        ));
    }

    #[test]
    fn parse_rtsc_file_rejects_bad_verb() {
        assert!(matches!(
            parse("rtsc file wibble foo"),
            Some(BotCommand::Unknown(_))
        ));
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(
            parse("FOLLOW"),
            Some(BotCommand::SetMode(BehaviorMode::Follow))
        );
        assert_eq!(
            parse("React Passive"),
            Some(BotCommand::SetReactivity(Reactivity::Passive))
        );
    }

    #[test]
    fn strat_multi_word_and_toggle() {
        use crate::bot::settings::StrategyFlags as F;
        // Multi-word flag.
        assert_eq!(
            parse("strat +tank assist"),
            Some(BotCommand::ToggleStrategies {
                add: F::TANK_ASSIST,
                remove: F::NONE,
                toggle: F::NONE,
                query: false,
            }),
        );
        // Comma-separated mixed.
        assert_eq!(
            parse("strat -tank assist,+dps assist"),
            Some(BotCommand::ToggleStrategies {
                add: F::DPS_ASSIST,
                remove: F::TANK_ASSIST,
                toggle: F::NONE,
                query: false,
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
        // Unknown spell names fall through to CastByName for FFI resolution
        assert!(matches!(
            parse("cast xyzzy"),
            Some(BotCommand::CastByName { .. })
        ));
        assert!(matches!(parse("cast"), Some(BotCommand::Unknown(_))));
    }

    #[test]
    fn formation_command() {
        assert_eq!(
            parse("formation near"),
            Some(BotCommand::SetFormation(FollowFormation::Near)),
        );
        assert_eq!(
            parse("formation arrow"),
            Some(BotCommand::SetFormation(FollowFormation::Arrow)),
        );
        assert_eq!(
            parse("formation melee"),
            Some(BotCommand::SetFormation(FollowFormation::Melee)),
        );
        assert_eq!(
            parse("formation raid"),
            Some(BotCommand::SetFormation(FollowFormation::Raid)),
        );
        // PB2 accepts `default` as an alias for `near`.
        assert_eq!(
            parse("formation default"),
            Some(BotCommand::SetFormation(FollowFormation::Near)),
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
        assert!(matches!(
            parse("stance bogus"),
            Some(BotCommand::Unknown(_))
        ));

        // Positioning stances (Mangosbot addon toolbar).
        assert_eq!(
            parse("stance behind"),
            Some(BotCommand::SetPositionStance(PositionStance::Behind))
        );
        assert_eq!(
            parse("stance near"),
            Some(BotCommand::SetPositionStance(PositionStance::Near))
        );
        assert_eq!(
            parse("stance tank"),
            Some(BotCommand::SetPositionStance(PositionStance::Tank))
        );
        assert_eq!(
            parse("stance turnback"),
            Some(BotCommand::SetPositionStance(PositionStance::Turnback))
        );

        assert_eq!(parse("max-dps"), Some(BotCommand::MaxDps));
        assert_eq!(parse("maxdps"), Some(BotCommand::MaxDps));
        assert_eq!(parse("save-mana"), Some(BotCommand::ToggleSaveMana));
        assert_eq!(parse("self-res"), Some(BotCommand::ToggleSelfRes));

        assert_eq!(parse("cheat 0x3"), Some(BotCommand::SetCheatFlags(3)));
        assert_eq!(parse("cheat 7"), Some(BotCommand::SetCheatFlags(7)));
        assert_eq!(parse("cheat off"), Some(BotCommand::SetCheatFlags(0)));

        assert_eq!(
            parse("keep 12345"),
            Some(BotCommand::KeepItem(ItemId(12345)))
        );
        assert_eq!(
            parse("unkeep 12345"),
            Some(BotCommand::UnkeepItem(ItemId(12345))),
        );

        assert_eq!(
            parse("chat party"),
            Some(BotCommand::SetChatChannel {
                channel: ChatChannel::Party,
                on: true
            }),
        );
        assert_eq!(
            parse("chat guild off"),
            Some(BotCommand::SetChatChannel {
                channel: ChatChannel::Guild,
                on: false
            }),
        );

        assert_eq!(
            parse("rti skull"),
            Some(BotCommand::SetPreferredRti(Some(8)))
        );
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
        assert_eq!(parse("leave"), Some(BotCommand::LeaveGroup));
    }

    #[test]
    fn probe_query_commands() {
        // Addon probes every setting via the `?` query operator.
        assert_eq!(parse("formation ?"), Some(BotCommand::QueryFormation));
        assert_eq!(parse("stance ?"), Some(BotCommand::QueryStance));
        assert_eq!(parse("strat ?"), Some(BotCommand::QueryAllStrategies));
        assert_eq!(parse("rti ?"), Some(BotCommand::QueryRti));
        assert_eq!(parse("ll ?"), Some(BotCommand::QueryLootPolicy));
        assert_eq!(parse("save mana ?"), Some(BotCommand::QuerySaveMana));
    }

    #[test]
    fn save_mana_explicit_set() {
        assert_eq!(parse("save mana on"), Some(BotCommand::SetSaveMana(1)));
        assert_eq!(parse("save mana off"), Some(BotCommand::SetSaveMana(0)));
        assert_eq!(parse("save mana"), Some(BotCommand::ToggleSaveMana));
        assert_eq!(parse("save mana 3"), Some(BotCommand::SetSaveMana(3)));
        assert_eq!(parse("save mana 5"), Some(BotCommand::SetSaveMana(5)));
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
    fn strat_boost_flag_parses() {
        use crate::bot::settings::StrategyFlags as F;
        // RaidControl burst-cooldown keyword.
        assert_eq!(
            parse("strat +boost"),
            Some(BotCommand::ToggleStrategies {
                add: F::BOOST,
                remove: F::NONE,
                toggle: F::NONE,
                query: false,
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

    #[test]
    fn debug_sub_commands() {
        assert_eq!(parse("debug"), Some(BotCommand::Debug));
        assert_eq!(parse("debug fsm"), Some(BotCommand::DebugFsm));
        assert_eq!(parse("debug claims"), Some(BotCommand::DebugClaims));
        assert_eq!(parse("debug claim"), Some(BotCommand::DebugClaims));
        assert_eq!(parse("debug coord"), Some(BotCommand::DebugCoord));
        assert_eq!(parse("debug coordination"), Some(BotCommand::DebugCoord));
        assert_eq!(parse("debug group"), Some(BotCommand::DebugCoord));
        // Unknown sub-command falls back to generic debug.
        assert_eq!(parse("debug blah"), Some(BotCommand::Debug));
    }

    #[test]
    fn group_coordination_commands() {
        assert_eq!(parse("set mt"), Some(BotCommand::SetMainTank));
        assert_eq!(parse("set maintank"), Some(BotCommand::SetMainTank));
        assert_eq!(parse("set ot"), Some(BotCommand::SetOffTank));
        assert_eq!(parse("set offtank"), Some(BotCommand::SetOffTank));
        assert_eq!(parse("unset mt"), Some(BotCommand::UnsetTank));
        assert_eq!(parse("unset ot"), Some(BotCommand::UnsetTank));
        assert_eq!(parse("unset tank"), Some(BotCommand::UnsetTank));
        assert_eq!(parse("assist"), Some(BotCommand::SetMainAssist(None)));
    }

    #[test]
    fn cc_assign_commands() {
        // cc with icon name
        assert_eq!(
            parse("cc skull"),
            Some(BotCommand::AssignCc {
                icon: 8,
                spell: None
            })
        );
        assert_eq!(
            parse("cc moon"),
            Some(BotCommand::AssignCc {
                icon: 5,
                spell: None
            })
        );
        // cc with numeric icon
        assert_eq!(
            parse("cc 3"),
            Some(BotCommand::AssignCc {
                icon: 3,
                spell: None
            })
        );
        // cc with no args → error
        assert_eq!(
            parse("cc"),
            Some(BotCommand::Unknown("cc: need target icon".into()))
        );
        // cc with unknown icon → error
        assert!(matches!(parse("cc banana"), Some(BotCommand::Unknown(_))));
    }

    #[test]
    fn uncc_commands() {
        // bare uncc → clear all
        assert_eq!(parse("uncc"), Some(BotCommand::UnassignCc(None)));
        // uncc with icon → clear specific
        assert_eq!(parse("uncc skull"), Some(BotCommand::UnassignCc(Some(8))));
        assert_eq!(parse("uncc 5"), Some(BotCommand::UnassignCc(Some(5))));
        // uncc with unknown → None icon
        assert_eq!(parse("uncc banana"), Some(BotCommand::UnassignCc(None)));
    }

    #[test]
    fn tank_target_commands() {
        // tank with icon name
        assert_eq!(parse("tank skull"), Some(BotCommand::AssignTankTarget(8)));
        assert_eq!(parse("tank star"), Some(BotCommand::AssignTankTarget(1)));
        // tank with numeric icon
        assert_eq!(parse("tank 7"), Some(BotCommand::AssignTankTarget(7)));
        // tank with no args → error
        assert_eq!(
            parse("tank"),
            Some(BotCommand::Unknown("tank: need target icon".into()))
        );
        // tank with unknown icon → error
        assert!(matches!(parse("tank banana"), Some(BotCommand::Unknown(_))));
    }

    #[test]
    fn untank_commands() {
        // bare untank → clear all
        assert_eq!(parse("untank"), Some(BotCommand::UnassignTankTarget(None)));
        // untank with icon → clear specific
        assert_eq!(
            parse("untank skull"),
            Some(BotCommand::UnassignTankTarget(Some(8)))
        );
        assert_eq!(
            parse("untank 3"),
            Some(BotCommand::UnassignTankTarget(Some(3)))
        );
        // untank with unknown → None icon
        assert_eq!(
            parse("untank banana"),
            Some(BotCommand::UnassignTankTarget(None))
        );
    }

    #[test]
    fn monitor_command() {
        assert_eq!(parse("monitor on"), Some(BotCommand::SetMonitor(true)));
        assert_eq!(parse("monitor off"), Some(BotCommand::SetMonitor(false)));
        assert_eq!(parse("monitor 1"), Some(BotCommand::SetMonitor(true)));
        assert_eq!(parse("monitor 0"), Some(BotCommand::SetMonitor(false)));
        assert_eq!(parse("monitor"), Some(BotCommand::SetMonitor(true)));
    }

    #[test]
    fn enable_disable_commands() {
        // "enable aoe" should parse to a Batch of two ApplyStrategies
        let cmd = parse("enable aoe").unwrap();
        assert!(matches!(cmd, BotCommand::Batch(_)));
        let cmd = parse("disable aoe").unwrap();
        assert!(matches!(cmd, BotCommand::Batch(_)));
        // Unknown flag
        let cmd = parse("enable nonexistent").unwrap();
        assert!(matches!(cmd, BotCommand::Unknown(_)));
    }

    #[test]
    fn preset_command() {
        assert_eq!(
            parse("preset boss"),
            Some(BotCommand::Preset("boss".to_string()))
        );
        assert_eq!(
            parse("preset raid-clear"),
            Some(BotCommand::Preset("raid-clear".to_string()))
        );
    }

    #[test]
    fn assist_with_name() {
        assert_eq!(parse("assist"), Some(BotCommand::SetMainAssist(None)));
        assert_eq!(
            parse("assist playerone"),
            Some(BotCommand::SetMainAssistByName("playerone".to_string()))
        );
    }
}
