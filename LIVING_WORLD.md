# Living World — making random bots feel like real players

> Design vision captured 2026-06-06. The north star for out-of-combat bot
> behaviour. Not yet implemented beyond the "Current state" section; this doc
> exists so the idea isn't lost and can drive future work.

## North star

The single most important goal: **bots should feel alive — like real players
with a purpose**, not NPCs walking around doing nothing. When a player flies
over a zone or stands in a city, the bots they see should *read as people doing
their own thing*. It's mostly about **looking** purposeful; perfect mechanics
matter less than the impression of intent.

## Core principle: attributes drive a *stable* purpose

Every random bot already has a randomized class, race, and professions. Use
those to pick a **default out-of-combat "purpose"**, and have the bot **stick
with it for a good while** — purpose should change slowly, not flip every few
seconds. Frequent switching looks robotic; a bot that spends, say, 20–40 minutes
"being a herbalist" looks natural.

So: `(class, race, professions, level)` → a default world behaviour the bot
commits to, with occasional natural transitions.

## Profession-driven behaviours

- **Mining (Miner / Blacksmith):** roam and mine ore nodes. In town, walk up to
  an **anvil** and play the blacksmithing/use animation (for show — doesn't need
  to craft anything real). Maybe craft something basic occasionally.
- **Herbalism (e.g. a Druid herbalist):** run around gathering herb nodes —
  ideally in **Cat Form / Travel Form** for druids, which looks great and very
  player-like.
- **Skinning / Leatherworking:** hunt and **skin appropriate mobs** — and
  crucially fight **level-appropriate** targets, not running around slaughtering
  trivial low-level mobs (a recurring "looks dumb" failure mode).
- **Fishing (everyone — it's a universal secondary skill):** stand at water and
  fish. All bots can do this as a fallback "idle but doing something" activity.
- General: a bot's profession set should bias *where it goes* and *what it
  fights/gathers*, so different bots naturally spread across different content.

## Town / social flavour

- **Auction House & vendors:** bots stand at the AH or a vendor and *pretend* to
  buy/sell (mostly just be present in the right spot, maybe open the window).
  Cities should look populated with people doing errands.
- **High-level "waiting for group":** some level-60s hang out **outside a
  high-level dungeon/raid entrance**, as if forming or waiting for their group.
- (Already partly built — see Current state — bots run real town errands:
  repair, sell, hand in quests, visit NPCs.)

## Out in the world

- Bots roaming the world should **look like they're questing**. The strong
  preference is that they **actually do quests and level up** (real progression),
  not just mime it — but at minimum the *movement and activity* should read as
  questing, not aimless wandering.

## Character progression — NOT re-randomization (hard requirement)

The old bots re-rolled **random gear on (almost) every spawn**. The user
explicitly dislikes this. A bot character must feel like a **player who
progresses**: stable identity, gear that improves over time, persistent level /
skills / inventory. A returning player should recognise a bot they saw before.

**Implication:** bot generation should be a *one-time* seed (or rare top-up),
and subsequent sessions should **load and continue** a bot's persisted state,
letting gear/level/skills advance — never wipe and re-randomize an existing bot.
Audit the factory/randomize path (`factory/`, `random_mgr` refresh) for any
place that re-randomizes gear/stats on respawn and gate it so existing
characters only progress, never reset.

## How this fits the architecture

- Purpose selection belongs in the Rust AI (`playerbot-rs`), keyed off the
  snapshot's class/race/level + a professions query, producing a sticky
  "world purpose" state (held on the blackboard / BDI intention with a long
  minimum dwell time).
- It slots in where the idle/RPG behaviour currently lives: the `rpg_subtree`
  and the masterless Follow-fallback in `bot/init.rs`. Today that subtree does
  errands → visit NPCs → fish → wander; the vision replaces the generic tail
  with a *profession/identity-driven* purpose.
- Gathering (mining/herb/skinning) and the "level-appropriate target band"
  logic already partly exist (`world/gather.rs`, `world/grind.rs`) and can be
  reused.

## Current state (2026-06-06) — foundation already in place

Working and live-validated (1h soak + repeated runs, stable):

- **Mounted travel** — masterless bots mount and ride around.
- **Town errands** — walk to vendor / repairer / quest-giver and do real
  business (repair, sell junk, hand in finished quests). Enabled by fixing the
  vanilla `UNIT_NPC_FLAG_*` layout bug (`crate::npc_flags`).
- **Visit NPCs** — stroll between town NPCs and chat (look busy).
- **Grind + loot** — engage nearby level-banded mobs.
- **Continuous, mounted movement** — no more "tapping W" stop-start wander.

Open / not yet working:

- **Fishing skill-up:** bots equip a pole and cast, but the bobber rarely/never
  spawns — the fishing spell's fishing-spot water check (`SpellEffects.cpp`,
  target type 39) is strict and few bots are near genuinely fishable water.
  Failed casts are cheap (give up in ~4s, no idle), so it doesn't hurt "feel
  alive," but it doesn't level the skill yet. Needs: align the water-direction
  scan with the spell's bobber distance, and/or route fishers to real fishing
  spots (the `ai_playerbot_named_location` `FISH_LOCATION_*` rows already exist).
- **Everything in the sections above** — the profession/identity-driven purpose
  system is the next big piece.
