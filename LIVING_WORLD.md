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

Cover **every profession that has a believable presence in the world**. They
fall into a few behaviour archetypes — a bot's purpose comes from whichever of
its professions gives the richest world behaviour.

### Gathering professions — roam the world and harvest (the richest "alive" look)

- **Mining:** roam and mine ore veins.
- **Herbalism:** roam and pick herb nodes. For **druids**, do it in **Cat /
  Travel Form** — looks great and very player-like.
- **Skinning:** kill **level-appropriate** skinnable beasts and skin the
  corpses. Crucially do *not* slaughter trivial low-level mobs — fight in-band
  targets (a recurring "looks dumb" failure mode to avoid).

These should bias *where the bot goes* and *what it fights*, spreading bots
across different content instead of clumping.

### Crafting professions — mostly a town "use the station" show, biased by their gather pair

The actual crafting can be faked (use the station / play the animation) — the
point is presence. Each typically pairs with a gathering prof, which is what
drives the *world* movement:

- **Blacksmithing / Engineering:** behave ~the same — go to a town **anvil /
  workbench** and use it (for show), maybe craft something basic. Both pair with
  **Mining**, so a "miner-smith" / "miner-engineer" mines out in the world and
  smiths in town.
- **Leatherworking:** pairs with **Skinning** — skin mobs in the world, work
  leather in town (for show).
- **Alchemy:** pairs with **Herbalism** — gather herbs in the world, "brew
  potions" in town (for show).
- **Tailoring:** cloth comes from humanoid kills, so a tailor grinds
  level-appropriate humanoids and tailors in town (for show).
- **Enchanting:** **no special world behaviour** (per the user) — it doesn't
  gather or have a worldly activity. Treat as "no profession purpose"; the bot
  falls back to its other profession or to questing/fishing.

### Secondary professions — everyone

- **Fishing (universal):** stand at water and fish — the catch-all "idle but
  doing something" activity any bot can fall back to.
- **Cooking / First Aid:** no strong standalone world behaviour; flavour only
  (e.g. cook at a town fire). Low priority.

### Summary

The lever is the bot's **gathering** profession (mining / herbalism / skinning)
— that's what produces visible, varied world activity. Crafting professions add
a town-station beat; enchanting/cooking/first-aid add little. A bot with no
gathering profession falls back to questing / grinding / fishing.

## Town / social flavour

- **Auction House & vendors:** bots stand at the AH or a vendor and *pretend* to
  buy/sell (mostly just be present in the right spot, maybe open the window).
  Cities should look populated with people doing errands.
- **High-level "waiting for group":** some level-60s hang out **outside a
  high-level dungeon/raid entrance**, as if forming or waiting for their group.
- (Already partly built — see Current state — bots run real town errands:
  repair, sell, hand in quests, visit NPCs.)

## Out in the world

- Bots roaming the world should **look like they're questing**. Strong
  preference: they **actually do quests and level up** (real progression), not
  just mime it — but at minimum the *movement and activity* should read as
  questing, not aimless wandering.
- **The dream: a bot starts level 1 and quests all the way to 60 with no human
  in the loop**, even if inefficiently. Hardest part is **knowing when to move
  on** — pick up a zone's quests, complete what it reasonably can, and **change
  zone** when it outlevels the area / runs dry. Needs a zone-progression model
  (per-level zone suggestions exist in WoW data and in the old bot travel data —
  reuse the `ai_playerbot_*` location/travel tables and the existing travel
  planner). Acceptable if it's slow and imperfect; the point is believable
  organic progression.
- **Realistic mounting while travelling.** When heading to a far quest
  objective / grind spot, **mount up if the destination is far enough** — but
  **don't mount to run 10 metres.** Gate on a sensible travel distance (e.g.
  only mount when the path is beyond ~some tens of yards / leaving the immediate
  area). (Mounting itself already works; this is about *when*.)
- **Group play (like the old bots):** when grouped with a human, **join their
  quests** — accept shareable quests the leader shares / picks up, and help work
  toward the shared objectives. This made the old bots good party members; keep
  it.

## Needs-driven town trips

Realistic self-maintenance should pull a bot back to a town:

- **Full (or nearly full) bags → go to a nearby town and sell/empty.** Bags
  filling up is a natural trigger for a vendor run, just like a real player.
- **Low durability → repair.** **Out of food/drink/reagents/ammo → restock.**
- These interrupt the current purpose, get handled in town, then the bot returns
  to what it was doing. (Repair / sell-junk already work; the missing piece is
  *full-bags* as a trigger and routing back afterwards.)

## Gear & loot management — keep upgrades, never downgrade (hard requirement)

The old bots had two **hated** behaviours that **made raiding impossible**:

1. **They sold good gear** that was given to them / that they had equipped.
2. **They equipped bad gear** (downgrades), throwing away better items.

Requirements:

- A bot must **always equip the best gear it owns** for its class/spec, and
  **never replace a better item with a worse one**.
- The vendor/sell logic must **never sell an item that is an upgrade over what's
  equipped**, and must **never sell currently-equipped gear**. Only sell true
  junk / clear downgrades / vendor-trash.
- **Player-given items are sacrosanct** — never auto-sell something a human
  handed the bot (or at least never sell gear/upgrades).
- This ties directly into [progression](#character-progression--not-re-randomization-hard-requirement):
  a bot that keeps and equips its upgrades is a bot that gets stronger over time
  and can actually raid.

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

Open / not yet working (roughly in priority order):

- **Gear & loot management** — verify/fix that bots never sell upgrades or
  equipped gear and always equip their best gear (the raiding-breaker). Likely
  the highest-value correctness fix here.
- **Progression, not re-randomization** — audit the factory/`random_mgr`
  refresh for respawn-time gear/stat re-rolls and gate them so existing
  characters only progress.
- **Profession/identity-driven sticky purpose** — the big new system (all the
  sections above): pick a purpose from class/race/professions, hold it a long
  while, drive gathering/crafting/questing behaviour from it.
- **Full-bags → town trip** trigger and return-to-purpose afterwards.
- **Realistic mount gating** while travelling (mount only when far enough).
- **Quest-to-60 loop** — accept/complete/turn-in quests, change zone when
  outleveled; and **group quest sharing** when grouped with a human.
- **Fishing skill-up:** bots equip a pole and cast, but the bobber rarely/never
  spawns — the fishing spell's fishing-spot water check (`SpellEffects.cpp`,
  target type 39) is strict and few bots are near genuinely fishable water.
  Failed casts are cheap (give up in ~4s, no idle), so it doesn't hurt "feel
  alive," but it doesn't level the skill yet. Needs: align the water-direction
  scan with the spell's bobber distance, and/or route fishers to real fishing
  spots (the `ai_playerbot_named_location` `FISH_LOCATION_*` rows already exist).
