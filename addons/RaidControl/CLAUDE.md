# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

RaidControl is a WoW Classic Era (Interface: 11404) addon for controlling playerbot raids. It sends chat commands to playerbots via whisper or raid/party chat, with support for combat log triggers that fire automatically on game events.

There are no build steps, tests, or linters — changes take effect by reloading the WoW UI (`/reload`) or restarting the game client.

## Slash commands (in-game)

```
/rc            - toggle window
/rc show|hide
/rc setup      - run raid setup (sends setup commands for selected boss)
/rc btn <label> - execute a button by its label text
/rc triggers   - toggle combat log triggers on/off
/rc debug      - toggle trigger debug mode (prints all combat log events)
/rc test       - test trigger config for currently selected raid/boss
/rc reset      - reset window position to center
```

## Architecture

**`BossData.lua`** — Pure data file, no logic. Defines two globals:
- `RaidControl_Categories`: array of global button categories shown for every boss
- `RaidControl_BossData`: table with a `raids` array; each raid has a `bosses` array; each boss has `categories` (button groups) and optionally `triggers` (combat log auto-responders)

**`RaidControl.lua`** — All addon logic as methods on the `RaidControl` global table. Key areas:
- `SubstituteVariables()` — replaces `{mt}`, `{ot1}`, `{ot2}`, `{puller}`, `{mth}`, `{ot1h}`, `{ot2h}`, `{player}`, `{target}`, `{affected}` in command strings
- `ExecuteSingleCommand(command, target)` — dispatches a command to the appropriate channel (whisper, /raid, /party, /guild, spell cast, or console slash command)
- Command queue with timer frame — handles `delay` fields in multi-command buttons, processing them sequentially via `OnUpdate`
- Combat trigger system — registers for `COMBAT_LOG_EVENT_UNFILTERED`, fires commands when spell/aura events match trigger definitions on the selected boss
- UI: two tabs ("Assignments" and "Controls"). Assignments tab holds dropdowns for MT/OT1/OT2/MT Healer/OT1 Healer/OT2 Healer/Puller. Controls tab shows scrollable global categories + boss-specific buttons.

**`RaidControl.xml`** — Frame layout only. Tab switching calls `RaidControl:ShowTab()`. Button clicks call `RaidControl:RunRaidSetup()` and category/boss button handlers. The scroll frame uses a manual slider + `OnMouseWheel`.

**`RaidControlDB`** (SavedVariable) — persists role assignments (`mainTank`, `offTank1`, `offTank2`, `puller`, `mtHealer`, `ot1Healer`, `ot2Healer`), selected raid/boss indices, active tab, minimized state, and collapsed category state.

## Adding boss data

Bosses are added in `BossData.lua` inside `RaidControl_BossData.raids[n].bosses`. Each boss entry:

```lua
{
    name = "BossName",
    categories = {
        {
            name = "Category Label",
            collapsible = true,  -- optional
            buttons = {
                { label = "Button", command = "botcmd", target = "raid" },
                -- or multi-command:
                { label = "Button", commands = {
                    { command = "cmd1", target = "mt" },
                    { command = "cmd2", target = "raid", delay = 0.5 },
                }},
            }
        },
    },
    triggers = { ... },  -- optional combat log triggers
}
```

Global categories (shown for all bosses) are added to `RaidControl_Categories` at the top of `BossData.lua`.

## Target values reference

`"raid"`, `"party"`, `"guild"`, `"mt"`, `"ot1"`, `"ot2"`, `"mth"`, `"ot1h"`, `"ot2h"`, `"puller"`, `"tanks"` (all tanks), `"healers"` (all tank healers), `"target"` (current target), `"w:Name"` (specific player), `"spell"` (cast locally), `"console"` (run slash command locally), `"affected"` (trigger-only: the player the event affected)

## Trigger format

```lua
{
    event = "SPELL_AURA_APPLIED",  -- or REMOVED, SPELL_CAST_START/SUCCESS, SPELL_DAMAGE, UNIT_DIED
    spellId = 12345,               -- preferred over spellName
    sourceName = "Boss Name",      -- optional filter
    destIsPlayer = true,           -- optional filter
    destIsGroupMember = true,      -- optional filter
    cooldown = 2.0,                -- seconds before trigger can fire again (default: 2)
    commands = {
        { command = "runaway", target = "affected" },
        { command = "follow",  target = "affected", delay = 8 },
    }
}
```
