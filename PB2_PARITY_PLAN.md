# PB2 → Rust Behaviour Parity Plan

Tracking doc for getting the Rust bot AI to full PB2 parity **and actually working in-game**.

- **Reference (port FROM):** `/home/cg/Code/gitea/Karatefylla/mangos/classic/source/src/modules/PB2/playerbot/strategy/`
- **Target:** `playerbot-rs/crates/playerbot/src/` (AI) + `cpp_wrapper/` (FFI glue)
- Fork is **classic → `vanilla`**.

> **Hard-won lesson:** "code exists" ≠ "works". The AI is *largely ported*, but real behaviour
> was blocked by (1) **runtime/wiring bugs**, (2) **C++ FFI stubs** (empty functions that silently
> no-op), and (3) **depth gaps** in otherwise-present rotations. Surveys/agents **over-report
> MISSING** by mistaking `cmangos/src/world.rs` trait *defaults* (return false) for the real
> `BotBridge.cpp`/`real.rs` impls. **Verify every claim against the actual code, and test in-game.**

> **Deploy gotcha (recurring):** fixes live in two layers — Rust (`cargo`) and C++ wrapper
> (`cmake`). A `cargo build` never includes C++ changes. Always do a **full** `cmake --build` +
> `cmake --install` + restart. Confirm the new binary is live via the startup log line
> `Playerbots: loaded N random-bot account(s)`.

---

## 0. START HERE (fresh session, no prior context)

**MASTER LIST = `PB2_PARITY_CHECKLIST.md`** — **4040 items** = every class in the PB2 playerbot
module (1243 strategies / 967 actions / 573 triggers / 364 values / 19 multipliers / 19 filters /
509 infra) + file-based subsystems (TravelMgr/world-nav, ChatHelper/Broadcast, ReactionEngine) +
**315 config-driven realism knobs** (random jump, idle wander, walking-RPG, teleports, distances,
timings — the human-like layer) + the movement/travel engine + an explicit realism list (walk-vs-run,
swim, ground-vs-flying-mount, anti-stuck, mob-avoid, rest-at-inn…). All `[ ]` UNVERIFIED. **This plan groups/prioritises; the checklist
is ground truth.** Rule: never say "on par" — report "X of 3697 verified"; mark `[x]` ONLY with a
Rust file:line/test. Per-spec×context strategies collapse to StrategyFlags+spec trees (verify the
matrix). Context/Mgr/Helper = infra (verify the system, not each class). Now also includes **23 ai_playerbot_* data tables**
(travel-node graph, text/broadcast content, weightscales, enchants…), **103 chat-command strings**
(verify semantics — the `leave` bug class), and a **BLIND SPOTS** section. **RESIDUAL LIMIT (cannot be
listed, only verified per-unit):** strategy COMPOSITION (which units each strategy wires + PRIORITIES),
multiplier weight values, hardcoded numeric tuning, free-function/event-hook logic, AHBot. The list
prevents FORGETTING; it does not prove BEHAVIOUR — that needs per-unit logic verification + in-game test.


**Read first:** `CLAUDE.md` (build/arch/conventions) and `RUST_MIGRATION.md` (phase log; factory/
login/random-mgr/itempool are ported there — treat character-generation parity as covered unless a
gear/talent gap is observed in-game).

**Repo layout:** three-crate Rust workspace under `playerbot-rs/crates/`:
`cmangos-sys` (raw FFI), `cmangos` (safe `World` trait — `world.rs` is the trait with `false`/empty
DEFAULTS; the REAL impl is `real.rs` → C++ `cpp_wrapper/BotBridge.cpp` `CB_*`; `mock.rs` = test
fake), `playerbot` (all AI). Behaviour = the `Bt` enum in `engine/bt.rs`, assembled in `bot/init.rs`.

**To add a behaviour:** new `Bt` variant + its `tick_*` handler + any condition leaf in
`engine/bt.rs`, then wire into `build_combat_tree`/`build_world_tree`/encounter/maintenance in
`bot/init.rs`. **New game query** = callback in `cpp_wrapper/botffi.h` + `World` trait method +
`OwnedList` alias + `VtableWorld` impl (`real.rs`) + `MockWorld` impl (`mock.rs`) + a test. Spell
constants live in `data/spells/vanilla.rs` (+ tbc/wotlk).

**Build / test / deploy:**
```
cd playerbot-rs && cargo test --workspace --features vanilla && cargo clippy --workspace --features vanilla -- -D warnings
# full server (from CMaNGOS core root) — REQUIRED for any C++ change:
cd /home/cg/Code/gitea/Karatefylla/mangos/classic/source
cmake --build bin/builddir --config Release -- -j8 && cmake --install bin/builddir   # then restart mangosd
```
Confirm the new binary is live: startup log line `Playerbots: loaded N random-bot account(s)`.

**Git state:** all §1 fixes are **uncommitted in the working tree** on branch `rrir` (not yet
built into the running server). **FIRST ACTION of the new session:** full cmake rebuild + restart,
then verify §1 in-game (rogue stealths + melees, follow is smooth, random bots populate). Do NOT
assume §1 works until that rebuild is confirmed. Consider committing §1 before new work.

**Standing rule:** never mark anything DONE from a survey/agent — verify against real
`CastOn*`/`CB_*`/wiring code (see Methodology). Port everything, no scope-cutting (user decision).

---

## 1. Runtime / wiring fixes (this session — DONE, pending full rebuild)

These were the actual reasons bots "had no behaviour". All verified (cargo/clippy) but need the
C++ wrapper rebuilt to take effect.

| Fix | Layer | What was broken |
|---|---|---|
| Stale binary | deploy | running server predated the current branch — nothing ran |
| Random-bot allow-list | C++ | `randomBotAccounts` never populated → every random bot kicked ("not allowed bot") |
| Login starvation | C++ | login pool trusted stale `characters.online` → `total_space≈1` → 0 logins |
| Melee never hit | C++/Rust | `auto_attack` enable was a stub (comment, no code) → engage toggled forever |
| Melee auto-shot | Rust | engage auto-shot for any ranged weapon → rogues shot crossbow instead of meleeing |
| Follow stagger | C++ | `CB_Follow` exact-float dedup → re-issued MoveFollow every tick while moving |
| `leave` = guild | Rust | mapped to GuildLeave instead of leave-group |
| `CanEnterArea` | C++ | returned false → bots blocked from raids past attunement |
| `TellPlayerNoFacing` | C++ | empty → bots couldn't whisper on that path |
| Duel accept | C++ | `CB_AcceptDuel` returned false (stub) |
| Rogue stealth | Rust | no stealth-cast/opener; added Stealth→Garrote approach (all specs) |

---

## 2. C++ no-stubs sweep (cpp_wrapper) — DONE (2026-06-05)

User mandate: **no stubs**. Triaged + completed (see Progress log for evidence).

- **Fixed (§1):** melee `auto_attack`, `CanEnterArea`, `TellPlayerNoFacing`, `CB_AcceptDuel`.
- **Implemented (§2):**
  - ✅ `JoinChatChannels` (`MgrBridge.cpp`) — bots join World + General/Trade/LocalDefense/WorldDefense/LFG.
  - ✅ `EnchantItemT` (`PlayerbotRust.cpp`) — now applies enchants for real; factory enchant pass live.
  - ✅ Outfit (`commands/outfit.rs`) — full named-gear-set port + DB persistence; dead `apply_outfit`
    /`CB_ApplyOutfit` wiring removed (was an orphan; outfit is command-driven via equip loop).
  - ✅ `BotConfig::logEvent` — dead (0 callers), removed; host file slated for Phase-M deletion.
- **Removed dead stubs:** `GetLevelFloat`, `AllowActivity`, `CastSpell`, `GetUnit` (0 callers each).
- **Legit no-ops — keep:** Classic-absent features (gems, arena teams pre-TBC), defensive `unreachable!`.
- **Still need human-client pass:** bots chatting in /general; enchant glow on new bots; `outfit` across logout.

---

## 3. Combat depth (evidence-based audit of actual rotations)

Every DPS spec has its **core rotation**; healers heal correctly via `HealLowest`/`HealInjuredParty`.
The gaps are **utility/CC/cooldowns/AoE**, consistent across classes.

| Class | Core | Real gaps to port |
|---|---|---|
| Warrior | ✅ | Shield Bash + Pummel (interrupts), Hamstring, Death Wish/Recklessness, Disarm, Mocking Blow |
| Paladin | ✅ heals real | Hammer of Justice (stun/interrupt), Cleanse/dispel, Blessing of Protection/Freedom/Sacrifice, Repentance |
| Hunter | ✅ | pet Call/Revive (only Mend Pet), aspect switching (only Hawk), Concussive/Rapid Fire, trap usage, Disengage |
| Rogue | ✅ +stealth | Kidney Shot, Cheap Shot, Sap, Blind, Expose Armor, Sprint, Distract |
| Priest | ✅ heals real | Smite/Holy Fire filler, Dispel Magic, Shackle Undead, Mana Burn, Fear Ward |
| Shaman | ✅ heals real | totem situational logic, Elemental Mastery, Grounding/Tremor/Poison-Cleansing totem use, Earth/Water Shield |
| Mage | ✅ | **Polymorph (CC)**, AoE (Blizzard/Flamestrike), Frost/Fire Ward, Combustion/Arcane Power, Conjure food/water, Mana Shield |
| Warlock | ✅ destro best | **Fear (CC)**, pet summon/command/sacrifice, Banish, Rain of Fire (AoE), Drain Soul, Soulstone, Howl of Terror, curse variety |
| Druid | ✅ | **Prowl (feral stealth)**, Entangling Roots/Hibernate (CC), Mark of the Wild, Bash (interrupt), Tiger's Fury, Hurricane (AoE) |
| Death Knight | ✅ | Mind Freeze (interrupt), Horn of Winter, Raise Dead/Army, Anti-Magic Zone |

### Per-spec missing abilities (cross-checked against actual `CastOn*` inventory)
Accurate — derived from the real rotation casts, NOT the surveys (which falsely reported
mage/paladin/shaman/warlock as "0% casting"; they all cast their cores).

- **Warrior** (have: stances, shouts, MS/BT/WW/Cleave/Execute/HS/Overpower/Rend/Sunder/Charge/Intercept, prot Shield Slam/Revenge/Shield Block/Shield Wall/Last Stand/Taunt/Thunder Clap/Demo Shout):
  missing **Shield Bash + Pummel (interrupts)**, Hamstring, Disarm, Mocking Blow, Sweeping Strikes, Retaliation.
- **Paladin** (have: seals SoR/SoC, Judgement, Consecration, Exorcism, HoW, Holy Shock, Holy Shield, Righteous Fury, Divine Shield, holy heals Holy Light/Flash/Lay on Hands):
  missing **Hammer of Justice (stun/interrupt)**, **Cleanse/Purify**, extra blessings (Protection/Freedom/Salvation/Sacrifice), Repentance, seal/aura variety.
- **Hunter** (have: Hawk, Hunter's Mark, Serpent Sting, Aimed/Arcane/Multi Shot, Raptor Strike, Wing Clip, Mend Pet, Feign Death, Scatter Shot, traps):
  missing **Call Pet/Revive Pet**, **aspect switching (Monkey/Cheetah/Viper context)**, Concussive Shot, Distracting Shot, Volley (AoE), Kill Command, Freezing/Frost Trap.
- **Rogue** (have: stealth+Garrote opener, Backstab/Eviscerate/Hemorrhage/Kick/Rupture/SS/SnD/Vanish/Ambush/Gouge/Blade Flurry/Evasion/Riposte):
  missing **Kidney Shot (stun finisher)**, Cheap Shot, Sap, Blind, Expose Armor, Sprint, Distract, Preparation.
- **Priest** (have: shadow SW:Pain/Mind Blast/Mind Flay/Devouring Plague/Shadowform/Vampiric Embrace/Psychic Scream/Fade, holy/disc heals Flash/Greater/Renew + PW:Shield + Inner Fire):
  missing **Smite/Holy Fire (healer DPS)**, **Dispel Magic/Cure Disease**, Shackle Undead (CC), Mana Burn, Fear Ward, Power Infusion.
- **Shaman** (have: Lightning Bolt/Chain Lightning/Earth+Flame+Frost Shock, Stormstrike, Lightning Shield, heals Chain Heal/Healing Wave/LHW, totems.rs, imbues.rs):
  missing **Wind Shear / Earth Shock interrupt usage**, situational totems (Grounding/Tremor/Poison-Cleansing/Earthbind), Elemental Mastery, Earth/Water Shield, Cleanse Spirit, Fire Nova/Searing totem DPS.
- **Mage** (have: Frostbolt/Fireball/Scorch/Fire Blast/Frost Nova/Cone of Cold/Blink/Arcane Missiles/Arcane Explosion/Counterspell/Evocation/Ice Block):
  missing **Polymorph (CC)**, **Blizzard/Flamestrike (AoE)**, Frost/Fire Ward, Mana Shield, Combustion/Arcane Power/Icy Veins/Cold Snap/Presence of Mind, Conjure Food/Water, Remove Curse, Pyroblast.
- **Warlock** (have: Shadow Bolt/Corruption/CoA/Immolate/Conflagrate/Curse of Elements/Shadowburn/Drain Life/Life Tap/Demon Armor):
  missing **Fear (CC)**, **pet summon/command/sacrifice** (Imp/Voidwalker/Succubus/Felhunter), Banish, **Rain of Fire (AoE)**, Drain Soul, Soulstone/Healthstone, curse variety (Tongues/Weakness/Doom), Spell Lock (pet interrupt).
- **Druid** (have: bear/cat forms, Claw/Maul/Shred/Rake/Rip/Ferocious Bite/Swipe/Growl/Demo Roar/Frenzied Regen, balance Wrath/Starfire/Moonfire/Insect Swarm, resto heals/Innervate/Tranquility/Barkskin):
  missing **Prowl (feral stealth + Pounce/Ravage openers)**, **Entangling Roots/Hibernate (CC)**, **Bash (feral interrupt)**, Thorns, Cower, Faerie Fire (non-feral), Hurricane (AoE), Cure Poison/Remove Curse, Rebirth (combat rez).
- **Death Knight** (wotlk; have: diseases, strikes, presences, Death Grip/Death Coil/D&D/Blood Boil/CDs):
  missing **Mind Freeze (interrupt)**, Horn of Winter, Raise Dead/Army, Anti-Magic Zone, Death Pact.

### Cross-class systems (build once, wire per class)
1. **Interrupt framework** — Shield Bash/Pummel/Mind Freeze/Bash/Wind Shear/Counterspell(have)/Kick(have)/Earth Shock(have). Generalise the "target casting → interrupt" leaf per class spell.
2. **CC framework** — Polymorph/Fear/Sap/Entangling Roots/Hammer of Justice/Banish/Hibernate/Shackle. `AutoCc` exists; per-class CC casts + RTI integration are thin.
3. **Offensive cooldowns** — Death Wish/Recklessness/Combustion/Arcane Power/Bestial Wrath(have). Burst on `co +boost`.
4. **AoE rotations** — multi-target thresholds → Blizzard/Rain of Fire/Hurricane/Flamestrike/Consecration(have)/Whirlwind(have).
5. **Pet management** — hunter/warlock: summon, revive, command (attack/follow/stay), feed/happiness, sacrifice. Only Mend Pet present.
6. **Dispels/cleanse** — Cleanse/Dispel Magic/Abolish Poison/Purge(have)/Remove Curse.
7. **Class buffs** — Mark of the Wild, Arcane Brilliance, Prayer of Fortitude/Spirit, Thorns (single-target `buffs()` exist; group versions + smart per-member blessings thin).

---

## 4. Group / raid coordination depth
- Threat meters (PB2 `ThreatValue`/`MyThreatValue`/`TankThreatValue`) — Rust has only `PullingAggro`(>90%).
- Kill-order / focus / main-assist following beyond RTI.
- Heal assignment (tank-priority, overheal avoidance), tank assignment, CC assignment.
- Ready-check, role auto-detection, formation nuance (RaiderRole/GroupFighting).

## 5. Non-combat world loop — VERIFIED status (corrected; was over-stated)
Audited the actual world subtrees + wiring. Three buckets:

**A. Autonomous & wired (bot does these on its own — real):**
- Check mail `bot/init.rs:503`, learn trainer spells `:505`, consumables/eat/drink `:268`
- Repair `strategies/maintenance.rs:9`, sell-greys `maintenance.rs:8`
- Quest accept→kill-mob→turn-in `world/quest.rs`, gather `world/gather.rs`, loot `world/loot.rs`
- RPG wander/emote/interact `world/rpg.rs`, mount, grind, guard, stay, corpse-run/death

**B. Real C++ callback but COMMAND-ONLY (NOT wired into any autonomous mode — bot won't do it
unless told):** bank deposit/withdraw, buy-from-vendor, send-mail-to-master. → port into the
autonomous maintenance/RPG loop (auto-restock food/reagents/ammo, auto-bank when bags full, etc.).

**C. Thin (present but shallow) — port full depth:**
- **Quest**: accept/kill/turn-in only — no objective-type routing (gather/explore/escort/use-item),
  no travel-to-objective, no reward choice, no quest-giver discovery, no quest chains.
- **Vendor**: sell-greys only — no sell-by-quality/policy, no buyback, no auto-buy (food/water/
  reagents/ammo/poisons).
- **Gather**: single `GatherNode` — no fishing bobber/loot loop, no node-route planning, no skill-up.
- **Travel**: no taxi/flight-master routing, no meeting-stone.
- **RPG/social depth**: town routing, inn/hearth, NPC-class discovery (vendor/trainer/repair seek),
  greetings/broadcasts (see §8).
- **Profession/craft mode**: `Craft` command exists; autonomous craft/disenchant/enchant loop thin.

## 6. Encounters — structure done, mechanics thin (per-boss)
- **DONE (real FSMs):** Molten Core (all 10 + rune-dousing + Magmadar tranq-shot + trash: Flamewaker-heal
  interrupt, Core-Hound tank-away, Lava-Surger stack), BWL (all 8 + suppression device), Onyxia (3-phase).
  Ragnaros: submerge phase NUKES the Sons of Flame (focus nearest entry 12143); Ground/P2 melee stack
  *behind* the boss (`MoveBehind`, Wrath of Ragnaros is a frontal knockback) and ranged spread to distinct
  points around him (`get_spread_position` by group index, so Elemental Fire can't chain). MC remaining
  (complex boss-signature, deferred — risk of half-scripting): Shazzrah Gate teleport handling (run-to-tank +
  spread for Arcane Explosion), Ragnaros tank "knocked up the pillar" spot (precise world pos + knockback
  timing), Garr Firesworn / Golemagg Core Rager add control. Base positioning + reactive
  dispel/interrupt/flee cover the rest.
- **Naxxramas:** real = Heigan, Grobbulus, Thaddius, Kel'Thuzad. **SimpleFsm (need mechanics):**
  Anub'Rekhan, Faerlina, Maexxna, Noth, Loatheb, Razuvious, Gothik, Four Horsemen (tank-swap), Patchwerk, Gluth, Sapphiron.
- **Zul'Gurub:** all 9 SimpleFsm (Jeklik bat phase, Venoxis snake, Mar'li spiders, Mandokir charge/gaze, Thekal trio, Arlokk vanish, Jin'do, Hakkar blood-siphon, Gahz'ranka).
- **AQ20:** all 6 SimpleFsm (Kurinnaxx sand-traps, Rajaxx waves, Moam mana-drain, Buru eggs, Ayamiss air phase, Ossirian crystals).
- **AQ40:** all 9 SimpleFsm (Skeram illusions, Bug Trio, Sartura whirlwind, Fankriss, Viscidus freeze/shatter, Huhuran frenzy, Twin Emperors tank-swap, Ouro burrow, C'thun tentacles/eye).
- **Karazhan (TBC):** SimpleFsm + Netherspite/Prince thin + Nightbane missing.

## 6b. Value/decision systems (PARTIAL — verify each before porting)
- **Threat:** reactive `ThreatDump` exists (`combat/reactive.rs:58`, uses real threat list). Missing:
  graded *proactive* threat throttling (DPS soft-cap below tank).
- **Target selection:** `combat/targeting.rs` has heal-target picking. VERIFY role-aware DPS/CC/
  enemy-healer focus selection — may be implicit.
- **Loot categorisation:** generic loot works; per-item vendor/craft/keep policy is thin.
- **Quest objective/reward tracking:** accept/turn-in work; objective-type routing + reward choice thin.
- **Professions/crafting:** `Craft` command exists; autonomous craft/disenchant/enchant *mode* thin.
> NOTE: these came from the survey pass and are only partially trustworthy — **verify against
> `combat/`, `engine/bt.rs`, `commands/mod.rs` before acting.**

## 7. PvP / Battlegrounds
- AV faction node objectives + NPC turn-ins + boss rush, AB 5-node routing/priority, WSG flag-carry
  safeguarding. Currently FFI stubs / delegated.
- Arena positioning, duel target selection (duel *accept* now works).

## 8. Social / "alive" features
- **JoinChatChannels** (stub — bots silent in /general,/trade).
- Broadcasts/greetings (PB2 `EnableBroadcasts`, `EnableGreet`), AI-chat replies.
- Town wander/inn (`random_mgr/teleport_cache.rs:18` flags RPG+inn caches TODO), emote variety.
- **Guild ops:** `factory/guild.rs` handles join at generation. VERIFY in-game guild create/invite/
  promote/demote/guild-bank/tabard (PB2 `RpgGuild*`) — likely thin/command-only.
- **Auction house:** bot-driven AH selling/buying (PB2 auction values). NOTE: the standalone AHBot
  C++ subsystem is **out of scope** per `CLAUDE.md`; only *bot* AH behaviour is in scope here.

## 8b. RPG autonomous sub-behaviours — FULL PB2 enumeration (`actions/RpgSubActions.h`)
The complete set of things a PB2 bot does on its own in the world (`RpgStrategy` drives these via
`ChooseRpgTargetAction` → `MoveToRpgTargetAction` → the sub-action). **Audit each against Rust
`world/rpg.rs` (currently only wander/emote/interact) — most are NOT yet wired.**
- RpgStartQuest, RpgEndQuest (full quest pickup/turn-in at RPG NPCs) · RpgGossipTalk · RpgHeal
- RpgBuy, RpgSell, RpgRepair, RpgTrain, RpgGetMail (vendor/trainer/mailbox visits)
- RpgBankDeposit, RpgBankWithdraw, RpgGuildBankDeposit, RpgGuildBankWithdraw
- RpgCraft, RpgEnchant, RpgWork (professions: do work, craft, enchant) · RpgItem, RpgUse, RpgSpell, RpgSpellClick (use items/objects/clickable spells)
- RpgDiscover (explore/discover areas) · RpgHomeBind (set hearth at innkeeper) · RpgTaxi (use flight master)
- RpgTradeUseful (trade useful items to nearby players) · RpgAIChat (AI chat replies) · RpgEmote
- RpgDuel · RpgQueueBg · RpgBuyPetition (guild charter) · RpgAHBuy, RpgAHSell (auction) · RpgStay, RpgCancel

## 12. Utility / misc actions — FULL PB2 enumeration (`actions/*Action.h`)
Audit each vs Rust `commands/`, `world/`, `engine/bt.rs`; many are command-only or unported:
- **Movement/state:** Unstuck, Teleport, RememberTaxi, UseMeetingStone, MoveStyle, Position, Range, SetHome, Go, Flag, Vehicle (N/A-vanilla mostly)
- **Items:** UpdateGear (in-game upgrade → §11), Equip/Unequip, Outfit (§2), DestroyItem, KeepItem, GiveItem, UseConsumable, UseItem, UseTrinket, Imbue, QueryItemUsage, TellItemCount
- **Quest:** TalkToQuestGiver, QuestReward/Reward, AutoCompleteQuest, DropQuest, ListQuests, QueryQuest, ShareQuest
- **Social/info:** Greet, Say, TellMaster, TellReputation, TellLos, TellCastFailed, Faction, Help, Stats, SuggestWhatToDo, Hire
- **Group/raid:** AcceptInvitation, InviteToGroup, LeaveGroup, PassLeadership, ReadyCheck, Lfg
- **Skills/spells:** AutoLearnSpell, Skill, Glyph (N/A-vanilla), ChangeTalents, SkipSpellsList, SetCraft, RemoveAura
- **Economy/guild:** Ah, GuildCreate/Management/Accept/Bank/CraftOrder/ShareItem/ShareAhBuy/AcceptQuestOrder, BuyGuildBankTab, PetitionSign, Trade/TradeStatus
- **World/system:** SetAvoidArea (hazard avoidance), ResetInstances, HonorGain, AreaTrigger, ReleaseSpirit, ReviveFromCorpse, RevealGatheringItem, Cheat, SecurityCheck, LogLevel, ResetAi, SetValue/CheckValues, RandomBotUpdate, Debug
- **RPG values to back the above:** PossibleRpgTargets, NearestNpcs/GameObjects/Corpses/FriendlyPlayers/NonBotPlayers, TravelValues, WorldBuffTravelValues.

## 9. Random-bot lifecycle
- `create_random_bots` FFI has **no caller** → no *new* random bots are created (existing ones work
  after the login fix). Wire `rndbot init`/refresh to it.

## 10. Commands / RTSC
- Breadth OK: **144 Rust command specs vs 86 PB2 supported** (`commands/parser.rs`). Do NOT chase
  "missing commands" — instead **audit command SEMANTICS** against PB2: the `leave`→GuildLeave bug
  proves a correctly-named command can do the wrong thing. Spot-check each command's action vs PB2's
  equivalent (`strategy/actions/*` / `ChatActionContext.h`).
- RTSC (real-time strategy channel) waypoints/jumps — verify `rtsc.rs` against PB2 `SeeSpellAction`.

## 11. Gear / item management (in-game, autonomous)
- Equip-upgrade exists ONLY at character generation (`factory/mod.rs` `PARTIAL_UPGRADE`). PB2 bots
  **re-equip upgrades in-game** (when they loot/buy/quest-reward a better item) — MISSING autonomous
  loop. Also: gem/enchant on new gear (vanilla: enchant only), bag management, disenchant.

---

## Priority order (highest player-visible value)
1. **Verify the runtime fixes** in-game (rebuild → stealth + smooth follow + melee). [§0/§1]
2. **Finish the C++ no-stubs sweep** (chat channels, enchant, outfit). [§2]
3. **Interrupts + CC** cross-class (biggest combat-feel gap). [§3]
4. **Pet management** (hunter/warlock) + **AoE rotations** + **offensive cooldowns**. [§3]
5. **Non-combat depth:** wire command-only actions (bank/buy/mail) into the autonomous loop +
   full quest/vendor/gear-upgrade. [§5/§11]
6. **Encounter mechanics** (ZG/AQ/Naxx/Kara) + **group threat/assignment**. [§6/§4]
7. **Command semantics audit** [§10], **PvP/BG** [§7], **social/alive + guild** [§8].

## Domain coverage checklist (so nothing is forgotten)
Combat rotations §3 · interrupts/CC/cooldowns/AoE/pets/dispels/buffs §3 · threat/target/value systems
§4/§6b · non-combat maintenance+quest+vendor+gather+travel §5 · encounters §6 · PvP/BG/arena/duel §7 ·
social/chat/guild/AH §8 · lifecycle §9 · commands/RTSC §10 · gear/item §11 · stubs §2 · char-gen →
`RUST_MIGRATION.md`. **If a PB2 behaviour isn't placeable in one of these, the plan is incomplete — add it.**

## Methodology
- Per item: re-verify against real code (not surveys) → implement → `cargo test` + `cargo clippy -D warnings` → **full cmake rebuild** → test in-game → log below.
- Drive discovery from **observed in-game behaviour**, not source claims.
- **LIVING DOCUMENT — add anything new on sight.** If during migration you encounter ANY PB2
  strategy / action / trigger / value / multiplier / command / config knob / data table / behaviour
  (or an in-game gap) that is **not already in `PB2_PARITY_CHECKLIST.md`**, ADD it immediately as a
  `[ ]` item under the right section, with where you found it (PB2 file or in-game). This includes
  units that only emerge from strategy *composition*, from reading PB2 source, or from watching a
  bot. The checklist is only as complete as we keep it — **when in doubt, add it; never assume it's
  already covered.** Re-run the source-enumeration greps (see checklist header) if a whole category
  seems missing.
- **Surveys/LLM agents are NOT reliable for "is it implemented?"** — a 2026-06-05 exhaustive audit
  falsely reported Mage/Paladin/Shaman/Warlock as "0% casting", healing as "MISSING", professions
  as "0%", and threat as "binary only" — all contradicted by the actual code (they cast their
  cores; heals via `HealLowest`; `Craft` command exists; `ThreatDump` uses a real threat list).
  Trust only claims cross-checked against the real `CastOn*`/`CB_*` code. When in doubt, grep.

## Progress log
- 2026-06-05 · MOLTEN CORE trash positioning — Core Hound tank-away + Lava Surger stack. `zone_wide_bt` now
  also: (tank) when the current target is a Core Hound (11671/11673), `MoveAwayFromRaid(20)` to drag the
  cleaving/reviving pack off the raid; (non-tank) when a Lava Surger (12101, the charging elemental) is within
  30y, `STACK_ON_TANK` (move to within 5y of `group_tank`) so the charge + cleave hit a grouped raid instead
  of scattering people into other packs. New leaves `TARGET_IS_CORE_HOUND` / `LAVA_SURGER_NEARBY` /
  `STACK_ON_TANK`, gated on InCombat + role. cargo test 10/10 + clippy green; build+install; smoke clean
  (531 bots, 0 crashes). Live verification = a client raid.
- 2026-06-05 · MOLTEN CORE mechanics — kept all existing boss FSMs (which are real), added the gaps.
  Magmadar: hunters now **Tranquilizing Shot** (19801) the boss's Frenzy enrage (aura 19451) — top-priority
  branch in `magmadar.rs::phase_bt`; the signature mechanic that was missing. MC TRASH: new
  `INTERRUPT_FLAMEWAKER` zone-wide leaf — scans nearby hostiles for a casting Flamewaker Priest (11662) or
  Healer (11663) and fires the bot's class interrupt at it (an OFF-target interrupt the reactive
  current-target interrupt can't do), so the trash packs don't heal through the raid. Extracted
  `class_interrupt_spells(class)` from `tick_interrupt` so the reactive interrupt and the trash interrupt
  share one per-class table. Runs zone-wide via the MC FSM's `zone_wide_bt` (same hook as rune dousing).
  cargo test 10/10 + clippy green; full mangosd build+install; smoke test clean (458 bots, 0 crashes). MC
  mechanics only fire inside the instance, so live verification is a client raid.
- 2026-06-05 · GREETING — bots `/say` a hello to nearby real players, completing the social pair with broadcast.
  `CB_BotGreetNearbyPlayer`: `PlayerListSearcher` within 25y, skip bots (`GetPlayerbotAI()`) and group members
  (`IsInGroup`), per-bot per-target 10-min cooldown so nobody is greeted twice, name-filled hardcoded greeting
  via `/say`. `Bt::GreetNearbyPlayer` throttled 20s in the maintenance loop. 1 unit test; cargo test 10/10 +
  clippy green; full mangosd build+install; smoke test clean (506 bots, 0 crashes). Greeting only fires for a
  real player nearby (none in a headless run), so live verification is a client pass.
- 2026-06-05 · BROADCAST / IDLE CHATTER — gave the §2 chat channels a voice. `CB_BotBroadcastRandom` says a
  random suggestion from `ai_playerbot_texts` (cached at first use) in the bot's General channel (reconstructs
  the zone-qualified name like §2's JoinChatChannels; `Channel::Say`, `/say` fallback). Placeholders filled
  (`%my_level`/`%my_name`/`%my_role`/`%category`) and any text with an unfillable `%` is skipped, so no broken
  output; a low per-call chance (~8%) + the caller's 180s throttle keep it a trickle across hundreds of bots.
  `Bt::BroadcastChatter` wired into the world maintenance loop. 2 unit tests; cargo test 10/10 + clippy green;
  full mangosd build+install; smoke test clean (737 bots, 0 crashes, no SQL errors). Chat isn't in Server.log
  so live chatter verification is a client pass. Deferred: per-event broadcasts (loot/quest/levelup/kill),
  per-category channels (Trade/LFG), and proximity GREETING (no greet-text category exists).
- 2026-06-05 · ESCORT QUESTS — the last quest objective type; the quest system is now functionally COMPLETE.
  `CB_GetActiveEscortNpc` finds a nearby creature whose escort AI is in `STATE_ESCORT_ESCORTING` while the bot
  holds an incomplete `QUEST_TYPE_ESCORT` quest (the escorted-player is private, but quest + escorting-state is
  a reliable id), via `dynamic_cast<npc_escortAI*>` (links cleanly — `-I src/game` covers ScriptDevAI).
  `Bt::EscortQuestNpc` (first in the quest subtree, time-sensitive) follows the NPC with `follow(npc,5,0)`;
  the reactive combat subtree handles ambushes; the escort script completes the quest on arrival. All four
  objective types now handled (kill/use-GO/collect/escort). 2 unit tests; cargo test 10/10 + clippy green;
  full mangosd build+install; smoke test clean (652 bots, 0 crashes). Deferred: gossip-started escorts (only
  accept-started ones auto-detect). Live escort verification pending a client pass.
- 2026-06-05 · BOAT / ZEPPELIN CROSS-CONTINENT TRAVEL — bots can now reach ANY continent (completes the travel
  story with taxi). `Bt::CrossContinentTravel` (wired ahead of taxi in `strategies/travel.rs`): for a travel
  dest on a different map, `CB_CrossContinentTravel` finds a transport on the bot's map whose keyframes reach
  the dest map, sends the bot to the nearest dock (a stop keyframe, `delay>0`), boards when the boat is in
  range (`Transport::AddPassenger`), and disembarks on arrival — the core auto-teleports player passengers
  across the map boundary (`Transport::TeleportTransport`), so no fragile timing/stranding. Made
  `tick_choose_travel_target` allow cross-map destinations (new `TravelDestMap` blackboard key) so
  cross-continent objectives are no longer filtered out. 4 unit tests; cargo test 10/10 + clippy green; full
  mangosd build+install; smoke test clean (534 bots, 0 crashes). Live boarding verification pending a client
  pass. Deferred: reaching a dock/flight-master that's out of walk range.
- 2026-06-05 · QUEST ITEM-COLLECT ROUTING — the last quest objective type. `CB_BotFindTravelDests` now also
  routes `ReqItemId` objectives: a cached `creature_loot_template`/`creature_template` reverse lookup
  (`ItemDropSources`) maps the required item → creatures that drop it → route to a spawn; existing kill+loot
  finishes it. All three objective types (kill / use-GO / collect) now route. Smoke test clean (580 bots, 0
  crashes). Quest objective routing is now COMPLETE except cross-continent (needs boats) and escort (FSM).
- 2026-06-05 · TAXI / FLIGHT-MASTER ROUTING — implemented. Bots now FLY to far (>600y) same-continent travel
  destinations instead of walking through dangerous zones. `Bt::TakeTaxi` (wired into `strategies/travel.rs`
  ahead of the walk action): for a far blackboard dest, walk to the nearest flight master
  (`nearest_taxi_node_pos`), then take off (`take_taxi_toward`). C++ `CB_TakeTaxiToward` computes the multi-hop
  node route via BFS over `sTaxiPathSetBySource` (a single {src,dest} pair only works for directly-connected
  nodes), discovers the route nodes on the bot's taxi mask, and calls `Player::ActivateTaxiPathTo(route,
  flightMaster)`; it validates the bot is standing at the source flight master (2*INTERACTION_DISTANCE).
  Cross-CONTINENT needs boats (not taxi) — out of scope. 4 unit tests (near-skip / walk-to-master / take-off /
  no-network), cargo test 10/10 + clippy vanilla&wotlk green; full mangosd build+install; smoke test clean
  (572 bots, 0 crashes, normal loop times). Live flight verification (watch a bot fly) pending a client pass.
- 2026-06-05 · QUEST OBJECTIVE ROUTING — FINISHED (per "don't leave a thing half-done if not blocked"):
  extended the creature-kill v1 to full objective coverage. C++ `CB_BotFindTravelDests` now also routes to
  gameobject-USE objectives (`ReqCreatureOrGOId<0` via `FindGOData`). New FFI: `use_nearby_quest_object`
  (`go->Use` on a nearby quest GO → `Bt::UseQuestObject`, wired in `world/quest.rs`) and
  `is_quest_objective_creature` (so `tick_attack_quest_mob` kills the SPECIFIC objective mob via the unit
  snapshot's `npc_entry`, not just the nearest). Turn-in now picks the best REWARD CHOICE
  (`PickBestRewardChoice`: prefer usable items by item level, else highest sell value) instead of hardcoded
  index 0. Genuinely deferred because BLOCKED on separate subsystems: item-collect-from-mob (needs a
  loot-source reverse index — PB2 precomputes item/quest relations), cross-map objectives (needs taxi
  routing), escort objectives (follow/protect FSM). cargo test (vanilla 10/10) + clippy green; full mangosd
  build+install; smoke test clean (553 bots, 0 crashes, normal loop times).
- 2026-06-05 · QUEST OBJECTIVE ROUTING — implemented (the biggest single non-combat gap). Before: bots
  routed to quest GIVERS (accept) and TAKERS (turn in) but never to quest OBJECTIVE locations — after
  accepting, they just ground whatever was nearby (`tick_attack_quest_mob` attacks any nearby unit). The
  travel system already had the scaffolding (`TravelPurpose::QUEST_OBJECTIVE1..4`, `TravelKind::QuestObjective`,
  `find_travel_dests`) but `evaluate_needs` never emitted an objective need and the C++ provider only searched
  by NPC flag. Fixed end to end:
  - C++ `CB_BotFindTravelDests` (`BotBridge.cpp`): for the bot's INCOMPLETE quests, read each quest template's
    `ReqCreatureOrGOId`/`ReqCreatureOrGOCount`, skip objectives already met (`GetReqKillOrCastCurrentCount`),
    and resolve each remaining creature-kill objective to a spawn location via `FindCreatureData`+`DoCreatureData`
    (map-aware, not range-limited) → emit a QUEST_OBJECTIVE destination.
  - Rust `evaluate_needs` (`travel/planner.rs`): split quest state into complete (→QUEST_TAKER, prio 6.85) vs
    incomplete (→QUEST_ALL_OBJ, prio 6.83, above grind); caller computes both from the quest log; purpose→kind
    map adds QUEST_OBJECTIVE→`QuestObjective`. The expensive `DoCreatureData` scan only runs after cheaper
    needs (vendor/taker/giver) find nothing, so it's naturally rate-limited.
  - Scope: creature-KILL objectives, same-map. Deferred (checklist): GO objectives (`ReqCreatureOrGOId<0`),
    item-collect (need loot-source), cross-map (needs taxi), and precomputing objective locations (PB2's
    travelnode graph) instead of a per-query `DoCreatureData` scan for scale.
  - Verified: cargo test (vanilla, +1 new) + clippy green; full mangosd build+install; smoke test clean
    (452 bots, 0 crashes, loop times normal). Behavioural confirmation (watch a bot walk to its quest mobs)
    pending a human-client pass.
- 2026-06-05 · #5 NON-COMBAT DEPTH — VERIFIED mostly already done; one pet-polish fix landed. Reading
  the real code (not the plan): vendor-sell-grey + repair + loot are autonomous (`strategies/maintenance.rs`);
  consumables eat/drink wired (`tick_consumables`) with factory food/drink refill; **random bots auto-upgrade
  gear via the scheduler re-running `randomize` (factory re-equip) every 10–60 min** — so §11's "gear upgrade
  MISSING" is largely covered for random bots. PB2's `UpdateGearAction` is the niche config-driven
  GearProgressionSystem (max-level bots grouped with a real player), not a loot-equip loop. Landed: warlock
  demon SELECTION — `CB_SummonPet` now prefers Voidwalker (survivable solo pet) → Felhunter → Succubus → Imp
  (HasSpell still falls back to Imp for low-level locks), instead of always summoning the fragile Imp first.
  Smoke test clean (645 bots, 0 crashes).
  GENUINE remaining gaps (real, but each a focused effort — added to checklist): quest objective-type routing
  (gather/use-item/explore/escort — currently kill-only); broadcast/greeting social system (PB2 BroadcastHelper
  — bots are now IN chat channels via §2 but say nothing); taxi/flight-master routing; GearProgressionSystem
  command; vendor sell-by-quality (needs ItemUsage policy); autonomous ammo/food RESTOCK (buy FFI exists but
  needs vendor-stock routing; mitigated by periodic randomize). None are quick "stub fills" — they're features.
- 2026-06-05 · #4 PETS / AoE / COOLDOWNS — mostly already done; ONE real gap fixed. Verified by reading:
  AoE (`CastAoEOnTarget`) is wired across warrior/druid/mage/shaman/DK/rogue specs; offensive cooldowns
  (`boost()` per class — mage Combustion/Arcane Power/Presence of Mind, warrior Recklessness/Death Wish,
  hunter Rapid Fire/Bestial Wrath, etc.) all present. Pet LIFECYCLE already wired (`world/pet.rs`:
  summon/revive/feed + hunter Mend Pet in rotations). The plan's "only Mend Pet present" was wrong.
  REAL GAP fixed: **pet attack-on-engage**. Pets summon `REACT_DEFENSIVE`, so a mob pulled at range was
  ignored by the pet. Added `pet_attack` FFI (`CB_PetAttack` → `pet->AI()->AttackStart`, skips re-issue
  when already on victim, honours passive) + World/real/mock + `Bt::PetAttack` leaf, wired as an
  `Optional` side-effect step in `combat_wrapper` (B.1b, throttled 1s) so it commands the pet without
  blocking the rotation. PB2 `AttackAction` parity. cargo test (vanilla 10/10, +2 pet tests) + clippy
  green; full mangosd build+install; smoke test clean (521 bots, 0 crashes). Deferred (checklist):
  warlock demon SELECTION (always summons Imp first) + in-combat hunter pet revive — minor.
- 2026-06-05 · §3 INTERRUPTS + CC — CORRECTED + EXTENDED. **The plan over-stated this as "the biggest
  gap"; it was already ~80% built and wired.** Verified by reading: `combat/reactive.rs::interrupt_subtree`
  (`throttle(500, Seq!(TargetCastingInterruptible, Interrupt))`) is wrapped into EVERY class's combat
  tree at `bot/init.rs:325`; `bot/init.rs` wires `throttle(2_000, AutoCc)`. `tick_interrupt` already
  covered Rogue/Warrior(Shield Bash+Pummel)/Mage/Shaman/Druid; `tick_auto_cc` already covered
  Mage/Warlock/Priest/Druid/Paladin/Rogue. Extended both (`engine/bt.rs`, Rust-only, no FFI):
  - Interrupt: + Paladin Hammer of Justice (PB2 `HammerOfJusticeInterruptSpellTrigger`), + DeathKnight
    Mind Freeze (wotlk), + Shaman Wind Shear (wotlk, preferred over Earth Shock), + Druid Bash (in-melee,
    before Feral Charge). Raw-id consts; vanilla bots lacking wotlk spells fall through via can_cast.
  - CC: converted `tick_auto_cc` to per-class spell LISTS tried in order. Key insight — `CB_CastSpell`
    runs `CheckCast` and returns false on BAD_TARGETS, so type-restricted CC routes itself with NO
    creature-type FFI: Warlock [Banish→Fear], Druid [Hibernate→Entangling Roots] (PB2
    `CastEntanglingRootsCcAction`). Warlock now CCs non-demon adds (Fear); caster druids CC non-beasts.
  - STILL a gap (added to checklist): Warlock pet Spell Lock (Felhunter) interrupt — needs pet-ability
    dispatch, deferred. Hunter/Priest have no reliable vanilla interrupt (correct to omit).
  - Verified: cargo test (vanilla 10/10) + clippy vanilla&wotlk green; full mangosd build+install;
    smoke test clean (480 bots, many in combat, 0 crashes). Behavioural confirmation (watching a
    paladin HoJ a caster, a warlock Fear a beast add) pending a human-client pass.
- 2026-06-05 · §2 C++ NO-STUBS SWEEP — DONE (cargo+clippy green, full mangosd build+install, headless
  smoke test clean: boot OK, 674+ bots populated, JoinChatChannels ran per-login with zero crashes).
  Committed on `rrir`. Items:
  - §2a `JoinChatChannels` IMPLEMENTED (`cpp_wrapper/MgrBridge.cpp` `PlayerbotHolder::JoinChatChannels`):
    bots join World (gated: lvl≥10, free, non-solo) + built-in General/Trade/LocalDefense/WorldDefense/
    LookingForGroup via `sChatChannelsStore` + `channelMgr`/`GetJoinChannel`/`Channel::Join`. Faithful
    port of PB2's obfuscated `PlayerbotHolder::n`.
  - §2b `EnchantItemT` IMPLEMENTED for real (`cpp_wrapper/PlayerbotRust.cpp`): resolves the enchant
    spell→enchant-id and applies it (`ApplyEnchantment`/`SetEnchantment`). Both enchant "paths"
    (Rust `factory_enchant_equipment` and C++ `DoEnchantItem`) converged on this previously-no-op stub;
    now the factory enchant pass actually enchants. Stale "no-op stub" comments in BotBridge.cpp fixed.
  - §2c OUTFIT FULL PORT (user decision): new `commands/outfit.rs` — named gear sets with
    `?`/`name=ids`/`+items`/`-items`/`equip`/`replace`/`reset`/`update`, item-link+id parsing
    (port of `ChatHelper::parseItemsUnordered`), DB-backed persistence via the existing event KV
    store (`random_mgr/ffi.rs::kv_get_str/kv_set_str`, key `"outfit list"`, PB2 `^`/`name=ids`
    format — mutually readable). Removed dead `apply_outfit` wiring (orphan `Bt::ApplyOutfit`,
    `apply_outfit` FFI callback + World method + `CB_ApplyOutfit`). 4 unit tests.
  - §2d DEAD STUBS REMOVED (`PlayerbotRust.h`): `GetLevelFloat`, `AllowActivity`, `CastSpell`,
    `GetUnit` — all 0 callers, confirmed not virtual/core-called.
  - §2e `BotConfig::logEvent` (dead, 0 callers; host file slated for Phase-M deletion) removed rather
    than left as a misleading "will be re-implemented" stub. `CanLogAction` kept (has callers).
  - STILL NEEDS human-client pass: bots visibly chatting in /general; freshly-generated bots have
    enchant glow on gear; `outfit` save/equip/list across a logout.
- 2026-06-05 · §1 SERVER-SIDE VERIFIED LIVE · the §1 working-tree changes were already compiled
  into the installed `run/bin/mangosd` (Jun-4 23:48; `.o`/`.a` mtimes newer than sources — the
  "not in any running build" note was stale). Confirmed the full mangosd build+link now WORKS
  (Eluna/GCC16 block resolved). Headless run: `Playerbots: loaded 200 random-bot account(s)
  (prefix 'RNDBOT')`; online chars 0→989 in ~50s; zero "not allowed bot"/login-fail; stable, no
  crash. Proves the allow-list + login-starvation fixes. **Still need a human-client pass** for
  the behavioural §1 checks (rogue stealth+melee, smooth follow, duel accept, leave=group,
  CanEnterArea). §1 still uncommitted on `rrir`.
- 2026-06-04 · runtime fixes batch (table §1) · cpp_wrapper + bt.rs/rogue · cargo+clippy green, pending rebuild
- 2026-06-04 · `leave`→group · commands/parser.rs,mod.rs · test
- 2026-06-05 · rogue stealth approach+Garrote opener (all specs) · classes/rogue/mod.rs · cargo+clippy
- 2026-06-05 · plan completeness pass: added §0 start-here, §10 commands, §11 gear, guild/AH, domain
  checklist; corrected non-combat §5 (3 buckets); discarded unreliable survey audit. Ready for fresh session.
