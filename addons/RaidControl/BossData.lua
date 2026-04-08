-- BossData.lua
-- Raid Control Configuration
--
-- BUTTON FORMAT:
-- {
--     label = "Button Text",
--     command = "command to send",      -- Single command
--     target = "raid",                   -- Target (see below)
-- }
--
-- MULTI-COMMAND FORMAT (with optional delays):
-- {
--     label = "Button Text",
--     commands = {
--         {command = "first", target = "mt"},
--         {command = "second", target = "raid", delay = 0.5},
--     }
-- }
--
-- VARIABLE SUBSTITUTION (replaced with player names):
--   {mt}       - Main Tank
--   {ot1}      - Off Tank 1
--   {ot2}      - Off Tank 2
--   {puller}   - Puller
--   {mth}      - MT's dedicated healer
--   {ot1h}     - OT1's dedicated healer
--   {ot2h}     - OT2's dedicated healer
--   {player}   - Your character name
--   {target}   - Your current target's name
--   {affected} - Player affected by trigger (triggers only)
--
-- TARGETS:
--   "raid"     - /raid (or /party)
--   "party"    - /party
--   "guild"    - /guild
--   "mt"       - whisper Main Tank
--   "ot1"      - whisper Off Tank 1
--   "ot2"      - whisper Off Tank 2
--   "puller"   - whisper Puller
--   "mth"      - whisper MT's healer
--   "ot1h"     - whisper OT1's healer
--   "ot2h"     - whisper OT2's healer
--   "tanks"    - whisper all tanks
--   "healers"  - whisper all tank healers
--   "target"   - whisper your current target
--   "w:Name"   - whisper specific player (e.g. "w:Legolas")
--   "affected" - whisper the player affected by a trigger (triggers only)
--   "spell"    - cast a spell (e.g. { command = "aedm", target = "spell" })
--   "console"  - run addon slash commands locally
--
-- MACRO USAGE:
--   /rc setup           - Run raid setup
--   /rc btn Attack!     - Execute button labeled "Attack!"
--   /rc triggers        - Toggle combat log triggers
--   /rc help            - Show all commands
--
-- DELAY (for multi-commands):
--   delay = 0.5  -- seconds to wait AFTER this command
--
-- ============================================================================
-- COMBAT LOG TRIGGERS
-- ============================================================================
-- Automatically send commands when combat events occur (e.g. debuffs).
-- Add a "triggers" table to any boss definition:
--
-- TRIGGER FORMAT:
-- {
--     event = "SPELL_AURA_APPLIED",     -- Combat log event type
--     spellId = 20475,                   -- Match by spell ID (preferred)
--     -- OR --
--     spellName = "Living Bomb",         -- Match by spell name
--
--     -- Optional filters:
--     sourceName = "Baron Geddon",       -- Only from this source
--     destIsPlayer = true,               -- Only affects players
--     destIsGroupMember = true,          -- Only affects raid/party members
--     cooldown = 2.0,                    -- Seconds before trigger can fire again (default: 2)
--     debug = true,                      -- Print debug messages
--
--     -- Commands to execute:
--     commands = {
--         { command = "runaway", target = "affected" },  -- "affected" = player who got debuff
--         { command = "stay", target = "affected", delay = 8 },
--     }
-- }
--
-- SUPPORTED EVENTS:
--   "SPELL_AURA_APPLIED"    - Debuff/buff applied to someone
--   "SPELL_AURA_REMOVED"    - Debuff/buff removed from someone
--   "SPELL_CAST_START"      - Enemy starts casting (interruptible)
--   "SPELL_CAST_SUCCESS"    - Spell successfully cast
--   "SPELL_DAMAGE"          - Spell deals damage
--   "UNIT_DIED"             - Unit dies
--
-- EXAMPLE - Baron Geddon Living Bomb:
-- {
--     name = "Baron Geddon",
--     triggers = {
--         {
--             event = "SPELL_AURA_APPLIED",
--             spellId = 20475,  -- Living Bomb
--             destIsGroupMember = true,
--             commands = {
--                 { command = "runaway", target = "affected" },
--             }
--         },
--         {
--             event = "SPELL_AURA_REMOVED",
--             spellId = 20475,
--             destIsGroupMember = true,
--             commands = {
--                 { command = "follow", target = "affected" },
--             }
--         },
--     },
--     categories = { ... }
-- }
--
-- EXAMPLE - Lucifron Mind Control:
-- {
--     name = "Lucifron",
--     triggers = {
--         {
--             event = "SPELL_AURA_APPLIED",
--             spellName = "Dominate Mind",
--             destIsGroupMember = true,
--             commands = {
--                 { command = "cc {affected}", target = "raid" },
--             }
--         },
--     },
-- }
--
-- ============================================================================
--
-- GLOBAL CATEGORIES (RaidControl_Categories):
--   Shown for all bosses. Define as ordered array:
--   RaidControl_Categories = {
--       {
--           name = "Combat",
--           collapsible = false,  -- Optional: if true, can click to hide
--           buttons = { ... }
--       },
--   }
--
-- BOSS CATEGORIES:
--   Each boss can have its own categories for organization:
--   {
--       name = "Ragnaros",
--       categories = {
--           {
--               name = "Setup",
--               collapsible = true,  -- Can hide during combat
--               buttons = { ... }
--           },
--           {
--               name = "Phase 1",
--               buttons = { ... }
--           },
--       },
--   }

-- Reusable button categories (shown globally for all bosses)
-- Categories are in an ordered list for consistent display

RaidControl_Categories = {
    {
        name = "RTSC",
        collapsible = true,
        buttons = {
            {
                label = "Follow",
                commands = {
                    { command = "formation near", target = "target" },
                    { command = "follow",         target = "target" },
                }
            },
            {
                label = "Save",
                commands = {
                    { command = "rtsc save here Rag1", target = "target" },
                    { command = "stay",                target = "target" },
                }
            },
            {
                label = "Go",
                commands = {
                    { command = "rtsc cancel",     target = "raid" },
                    { command = "stay",            target = "target" },
                    { command = "rtsc move exact", target = "target" },
                    { command = "aedm",            target = "spell" },
                }
            },
        }
    },
    {
        name = "Setup",
        collapsible = true,
        buttons = {
            {
                label = "Roster1",
                commands = {
                    { command = ".bot add Afokdurg",    target = "raid", delay = 0.5 },
                    { command = ".bot add Ancheiz",     target = "raid", delay = 0.5 },
                    { command = ".bot add Carenok",     target = "raid", delay = 0.5 },
                    { command = ".bot add Chathinhwow", target = "raid", delay = 0.5 },
                    { command = ".bot add Damadam",     target = "raid", delay = 0.5 },
                    { command = ".bot add Dianari",     target = "raid", delay = 0.5 },
                    { command = ".bot add Eildo",       target = "raid", delay = 0.5 },
                    { command = ".bot add Fudanme",     target = "raid", delay = 0.5 },
                    { command = ".bot add Gahno",       target = "raid", delay = 0.5 },
                    { command = ".bot add Ginerine",    target = "raid", delay = 0.5 },
                    { command = ".bot add Givul",       target = "raid", delay = 0.5 },
                    { command = ".bot add Haenu",       target = "raid", delay = 0.5 },
                    { command = ".bot add Hukdezton",   target = "raid", delay = 0.5 },
                    { command = ".bot add Ical",        target = "raid", delay = 0.5 },
                    { command = ".bot add Izhu",        target = "raid", delay = 0.5 },
                    { command = ".bot add Jedzo",       target = "raid", delay = 0.5 },
                    { command = ".bot add Kalorion",    target = "raid", delay = 0.5 },
                    { command = ".bot add Kendro",      target = "raid", delay = 0.5 },
                    { command = ".bot add Kudso",       target = "raid", delay = 0.5 },
                    { command = ".bot add Lorcas",      target = "raid", delay = 0.5 },
                }
            },
            {
                label = "Roster2",
                commands = {
                    { command = ".bot add Micharkon",    target = "raid", delay = 0.5 },
                    { command = ".bot add Odavmirm",     target = "raid", delay = 0.5 },
                    { command = ".bot add Oderina",      target = "raid", delay = 0.5 },
                    { command = ".bot add Omgenk",       target = "raid", delay = 0.5 },
                    { command = ".bot add Revon",        target = "raid", delay = 0.5 },
                    { command = ".bot add Sandeon",      target = "raid", delay = 0.5 },
                    { command = ".bot add Sichak",       target = "raid", delay = 0.5 },
                    { command = ".bot add Surdok",       target = "raid", delay = 0.5 },
                    { command = ".bot add Sustalym",     target = "raid", delay = 0.5 },
                    { command = ".bot add Terineri",     target = "raid", delay = 0.5 },
                    { command = ".bot add Trolleriolas", target = "raid", delay = 0.5 },
                    { command = ".bot add Xahghul",      target = "raid", delay = 0.5 },
                    { command = ".bot add Björnshifts",  target = "raid", delay = 0.5 },
                    { command = ".bot add Furry",        target = "raid", delay = 0.5 },
                    { command = ".bot add Ganked",       target = "raid", delay = 0.5 },
                    { command = ".bot add Hawripawta",   target = "raid", delay = 0.5 },
                    { command = ".bot add Knugen",       target = "raid", delay = 0.5 },
                    { command = ".bot add Snelprest",    target = "raid", delay = 0.5 },
                    { command = ".bot add Sordok",       target = "raid", delay = 0.5 },
                    { command = ".bot add Rickyretardo", target = "raid", delay = 2.0 },
                    { command = "@rank=Raider join",     target = "guild" },
                }
            },
            {
                label = "Reset",
                commands = {
                    { command = "reset ai",                                                                                                   target = "raid" },
                    { command = "nc +rtsc,-rpg,-rpg bg,-rpg explore,-rpg guild,-rpg maintenance,-rpg player,-rpg quest,-rpg vendor,",         target = "raid" },
                    { command = "@group2-3 @warrior de +fury",                                                                                target = "raid" },
                    { command = "@group2-3 @warrior react +fury",                                                                             target = "raid" },
                    { command = "@group2-3 @warrior nc +fury,+dps assist",                                                                    target = "raid" },
                    { command = "@group2-3 @warrior co +fury,+dps assist,-pull,+threath",                                                     target = "raid" },
                    { command = "@shaman de +restoration",                                                                                    target = "raid" },
                    { command = "@shaman react +restoration",                                                                                 target = "raid" },
                    { command = "@shaman nc +restoration",                                                                                    target = "raid", delay = 1.0 },
                    { command = "@shaman co +restoration,+range,-close,+threath",                                                             target = "raid" },
                    { command = "@group1-5 @shaman nc +totem earth strength,+totem fire searing,+totem water resistance,+totem air windfury", target = "raid" },
                    { command = "@group1-5 @shaman co +totem earth strength,+totem fire searing,+totem water resistance,+totem air windfury", target = "raid" },
                    { command = "@group1-7 @priest all +holy",                                                                                target = "raid" },
                    { command = "@mage all +frost",                                                                                           target = "raid" },
                    { command = "@group1 @druid all +dps feral",                                                                              target = "raid" },
                    { command = "@dps co +threat",                                                                                            target = "raid" },
                }
            },
            {
                label = "Rebuff",
                commands = {
                    { command = "nc +wbuff", target = "raid" },
                }
            },
        },
    },
    {
        name = "Movement",
        buttons = {
            { label = "Stay",   command = "stay",   target = "raid" },
            { label = "Follow", command = "follow", target = "raid" },
            { label = "Free",   command = "free",   target = "raid" },
            { label = "Flee",   command = "flee",   target = "raid" },
            { label = "Summon", command = "summon", target = "raid" },
        },
    },
    {
        name = "Healing",
        collapsible = true,
        buttons = {
            { label = "Heal MT",  command = "@healer focus heal +{mt}",  target = "raid" },
            { label = "Heal OT1", command = "@healer focus heal +{ot1}", target = "raid" },
            { label = "Heal OT2", command = "@healer focus heal +{ot2}", target = "raid" },
            {
                label = "Focus Reset",
                commands = {
                    { command = "@healer focus heal none", target = "raid" },
                }
            },
        },
    },
    {
        name = "Tank",
        collapsible = true,
        buttons = {
            { label = "MT Taunt",  command = "cast Taunt", target = "mt" },
            { label = "OT1 Taunt", command = "cast Taunt", target = "ot1" },
            { label = "OT2 Taunt", command = "cast Taunt", target = "ot2" },
            {
                label = "Pull",
                commands = {
                    { command = "co -pull back", target = "puller" },
                    { command = "co +pull",      target = "puller" },
                    { command = "pull rti",      target = "puller" },
                }
            },
            {
                label = "MT Attack",
                commands = {
                    { command = "co -passive",              target = "mt" },
                    { command = "nc -passive",              target = "mt" },
                    { command = "co +tank",                 target = "mt" },
                    { command = "co +tank assist",          target = "mt" },
                    { command = "co +dps assist",           target = "mt" },
                    { command = "co -threat",               target = "mt" },
                    { command = "attack",                   target = "mt" },
                    { command = "@healer attack",           target = "raid" },
                    { command = "@dps co -boost",           target = "raid" },
                    { command = "@dps co -wait for attack", target = "raid" },
                    { command = "@rogue co -passive",       target = "raid" },
                    { command = "@dps attack",              target = "raid" },
                }
            },
            {
                label = "Attack Wait",
                commands = {
                    { command = "co -passive",                  target = "mt" },
                    { command = "co +tank",                     target = "mt" },
                    { command = "co +tank assist",              target = "mt" },
                    { command = "co +dps assist",               target = "mt" },
                    { command = "co +tank assist",              target = "mt" },
                    { command = "co -threat",                   target = "mt" },
                    { command = "attack",                       target = "mt" },
                    { command = "co -passive",                  target = "raid" },
                    { command = "nc -passive",                  target = "raid" },
                    { command = "@dps co -boost",               target = "raid" },
                    { command = "@dps co +wait for attack",     target = "raid" },
                    { command = "@warlock co -wait for attack", target = "raid" },
                    { command = "@warlock cc rti",              target = "raid" },
                    { command = "@tank co -wait for attack",    target = "raid" },
                    { command = "@dps wait for attack 10",      target = "raid" },
                }
            },
            {
                label = "OT1 Attack",
                commands = {
                    { command = "co +tank",          target = "ot1" },
                    { command = "outfit tank equip", target = "ot1" },
                    { command = "co -fury",          target = "ot1" },
                    { command = "co -tank assist",   target = "ot1" },
                    { command = "co -dps assist",    target = "ot1" },
                    { command = "co -threat",        target = "ot1" },
                    { command = "attack rti",        target = "ot1" },
                }
            },
            {
                label = "OT2 Attack",
                commands = {
                    { command = "co +tank",          target = "ot2" },
                    { command = "outfit tank equip", target = "ot2" },
                    { command = "co -fury",          target = "ot2" },
                    { command = "co -tank assist",   target = "ot2" },
                    { command = "co -dps assist",    target = "ot2" },
                    { command = "co -threat",        target = "ot2" },
                    { command = "attack rti",        target = "ot2" },
                }
            },
            -- {
            --     label = "Pull Back",
            --     commands = {
            --         { command = "co +pull back", target = "puller" },
            --         { command = "pull",          target = "puller" },
            --     }
            -- },
        },
    },
    {
        name = "DPS",
        collapsible = true,
        buttons = {
            { label = "Attack",   command = "@dps attack",    target = "raid" },
            { label = "Full DPS", command = "@dps co +boost", target = "raid" },
            {
                label = "Debuffs",
                commands = {
                    { command = "@warrior cast Sunder Armor", target = "raid" },
                }
            },
            {
                label = "AoE",
                commands = {
                    { command = "@mage cast Blizzard",          target = "raid" },
                    { command = "@hunter cast Volley",          target = "raid" },
                    { command = "@hunter cast Multishot",       target = "raid" },
                    { command = "@tank cast Challenging Shout", target = "raid" },
                }
            },
        },
    }

}

RaidControl_BossData = {
    -- Global buttons shown for every boss
    raids = {
        {
            name = "Molten Core",
            setup = {
                -- Formation: tight for MC corridors, default ranges
                { command = "formation raid",     target = "raid" },
                { command = "range follow 0",     target = "raid" },
                { command = "range followraid 0", target = "raid" },
                { command = "range attack 0",     target = "raid" },
                -- Kill target via skull marker
                { command = "rti skull",          target = "raid" },
                -- DPS hold threat — no aggro pulling on trash
                { command = "@dps co +threat",    target = "raid" },
                -- Enable MC dungeon strategy: bots auto-move to and douse nearby runes
                { command = "nc +molten core",    target = "raid" },
                -- Assign healers to their tanks
                { command = "focus heal +{mt}",   target = "mth" },
                { command = "focus heal +{ot1}",  target = "ot1h" },
                { command = "focus heal +{ot2}",  target = "ot2h" },
            },
            bosses = {
                {
                    name = "Lucifron",
                    triggers = {
                        {
                            event = "SPELL_AURA_APPLIED",
                            spellName = "Dominate Mind",
                            destIsGroupMember = true,
                            cooldown = 2.0,
                            commands = {
                                { command = "MC! {affected}", target = "raid" },
                            }
                        },
                    },
                    categories = {
                        {
                            name = "Setup",
                            collapsible = true,
                            buttons = {
                                {
                                    label = "MT",
                                    commands = {
                                        { command = "rtsc cancel",             target = "raid" },
                                        { command = "rtsc save exact MCLuci1", target = "mt" },
                                        { command = "aedm",                    target = "spell" },
                                    }
                                },
                                {
                                    label = "OT",
                                    commands = {
                                        { command = "rtsc cancel",             target = "raid" },
                                        { command = "rtsc save exact MCLuci1", target = "ot1" },
                                        { command = "aedm",                    target = "spell" },
                                    }
                                },
                                {
                                    label = "Ranged",
                                    commands = {
                                        { command = "rtsc cancel",               target = "raid" },
                                        { command = "@ranged rtsc select",       target = "raid" },
                                        { command = "@ranged rtsc save MCLuci1", target = "raid" },
                                        { command = "aedm",                      target = "spell" },
                                    }
                                },
                                {
                                    -- Lucifron + 2 Flamewaker Protectors: all 3 tanks need dedicated healers
                                    label = "Healers",
                                    commands = {
                                        { command = "focus heal +{mt}",  target = "mth"  },
                                        { command = "focus heal +{ot1}", target = "ot1h" },
                                        { command = "focus heal +{ot2}", target = "ot2h" },
                                    }
                                },
                                {
                                    label = "Douse Rune",
                                    commands = {
                                        { command = "u Eternal Quintessence", target = "raid" },
                                        { command = "u Aqual Quintessence",   target = "raid" },
                                    }
                                },
                            }
                        },
                        {
                            name = "Combat",
                            buttons = {
                                {
                                    label = "Go 1",
                                    commands = {
                                        { command = "rtsc go MCLuci1",    target = "raid" },
                                        { command = "stay",               target = "raid" },
                                        { command = "@melee @dps follow", target = "raid" },
                                        { command = "stay",               target = "ot1" },
                                    }
                                },
                                {
                                    label = "Dispel",
                                    commands = {
                                        { command = "@mage @druid remove curse", target = "raid" },
                                        { command = "@priest dispel magic",      target = "raid" },
                                    }
                                },
                            }
                        },
                    },
                },
                {
                    name = "Magmadar",
                    triggers = {
                        {
                            event = "SPELL_AURA_APPLIED",
                            spellName = "Frenzy",
                            sourceName = "Magmadar",
                            cooldown = 30.0,
                            commands = {
                                { command = "@hunter cast Tranquilizing Shot", target = "raid" },
                            }
                        },
                    },
                    categories = {
                        {
                            -- Magmadar: single tank. OT healers freed for raid healing.
                            name = "Setup",
                            collapsible = true,
                            buttons = {
                                {
                                    label = "Healers",
                                    commands = {
                                        { command = "focus heal +{mt}", target = "mth"  },
                                        { command = "focus heal none",  target = "ot1h" },
                                        { command = "focus heal none",  target = "ot2h" },
                                    }
                                },
                                {
                                    -- OTs have no tank role here — send them to DPS
                                    label = "OT DPS",
                                    commands = {
                                        { command = "co -tank assist,+dps assist", target = "ot1" },
                                        { command = "co -tank assist,+dps assist", target = "ot2" },
                                    }
                                },
                                {
                                    label = "Douse Rune",
                                    commands = {
                                        { command = "u Eternal Quintessence", target = "raid" },
                                        { command = "u Aqual Quintessence",   target = "raid" },
                                    }
                                },
                            }
                        },
                        {
                            name = "Mechanics",
                            buttons = {
                                { label = "Fear Ward",  command = "buff target +{mt}",               target = "mth" },
                                { label = "Tranq Shot", command = "@hunter cast Tranquilizing Shot", target = "raid" },
                            }
                        },
                    },
                },
                {
                    name = "Gehennas",
                    categories = {
                        {
                            -- Gehennas + 2 Flamewaker Healer adds: 3 tanks needed.
                            name = "Setup",
                            collapsible = true,
                            buttons = {
                                {
                                    label = "Healers",
                                    commands = {
                                        { command = "focus heal +{mt}",  target = "mth"  },
                                        { command = "focus heal +{ot1}", target = "ot1h" },
                                        { command = "focus heal +{ot2}", target = "ot2h" },
                                    }
                                },
                                {
                                    label = "Douse Rune",
                                    commands = {
                                        { command = "u Eternal Quintessence", target = "raid" },
                                        { command = "u Aqual Quintessence",   target = "raid" },
                                    }
                                },
                            }
                        },
                        {
                            name = "Mechanics",
                            buttons = {
                                { label = "Decurse", command = "@mage @druid remove curse", target = "raid" },
                                { label = "Kick",    command = "@rogue cast kick",          target = "raid" },
                            }
                        },
                    },
                },
                {
                    name = "Garr",
                    categories = {
                        {
                            name = "Setup",
                            collapsible = true,
                            buttons = {
                                {
                                    label = "RTSC MT",
                                    commands = {
                                        { command = "rtsc cancel", target = "raid" },
                                        { command = "rtsc select", target = "mt" },
                                    }
                                },
                                {
                                    label = "OT1",
                                    commands = {
                                        { command = "rtsc cancel", target = "raid" },
                                        { command = "rtsc select", target = "ot1" },
                                    }
                                },
                                {
                                    label = "OT2",
                                    commands = {
                                        { command = "rtsc cancel", target = "raid" },
                                        { command = "rtsc select", target = "ot2" },
                                    }
                                },
                                {
                                    label = "RTSC Ranged",
                                    commands = {
                                        { command = "rtsc cancel",         target = "raid" },
                                        { command = "@ranged rtsc select", target = "raid" },
                                    }
                                },
                                {
                                    label = "Save Safe",
                                    commands = {
                                        { command = "rtsc save GarrSafe", target = "raid" },
                                        { command = "aedm",               target = "spell" },
                                    }
                                },
                                {
                                    label = "Save Spot",
                                    commands = {
                                        { command = "rtsc save selected Garr1", target = "raid" },
                                        { command = "aedm",                     target = "spell" },
                                    }
                                },
                                {
                                    -- Garr + 8 Firesworn: all tanks active simultaneously
                                    label = "Healers",
                                    commands = {
                                        { command = "focus heal +{mt}",  target = "mth"  },
                                        { command = "focus heal +{ot1}", target = "ot1h" },
                                        { command = "focus heal +{ot2}", target = "ot2h" },
                                    }
                                },
                                {
                                    label = "Douse Rune",
                                    commands = {
                                        { command = "u Eternal Quintessence", target = "raid" },
                                        { command = "u Aqual Quintessence",   target = "raid" },
                                    }
                                },
                            }
                        },
                        {
                            name = "Combat",
                            buttons = {
                                {
                                    label = "Go Spots",
                                    commands = {
                                        { command = "@tank stay",          target = "raid" },
                                        { command = "@tank rtsc go Garr1", target = "raid" },
                                    }
                                },
                                {
                                    label = "Go Safe",
                                    commands = {
                                        { command = "@melee stay",             target = "raid" },
                                        { command = "@melee rtsc go GarrSafe", target = "raid" },
                                    }
                                },
                                {
                                    label = "Warlock CC",
                                    commands = {
                                        { command = "@warlock CC", target = "raid" },
                                    }
                                },
                            }
                        },
                    },
                },
                {
                    name = "Shazzrah",
                    categories = {
                        {
                            -- Shazzrah: single tank. Blinks to random players — keep everyone spread.
                            name = "Setup",
                            collapsible = true,
                            buttons = {
                                {
                                    label = "Healers",
                                    commands = {
                                        { command = "focus heal +{mt}", target = "mth"  },
                                        { command = "focus heal none",  target = "ot1h" },
                                        { command = "focus heal none",  target = "ot2h" },
                                    }
                                },
                                {
                                    label = "OT DPS",
                                    commands = {
                                        { command = "co -tank assist,+dps assist", target = "ot1" },
                                        { command = "co -tank assist,+dps assist", target = "ot2" },
                                    }
                                },
                                {
                                    label = "Douse Rune",
                                    commands = {
                                        { command = "u Eternal Quintessence", target = "raid" },
                                        { command = "u Aqual Quintessence",   target = "raid" },
                                    }
                                },
                            }
                        },
                        {
                            name = "Mechanics",
                            buttons = {
                                {
                                    label = "Decurse",
                                    commands = {
                                        { command = "@mage remove curse",  target = "raid" },
                                        { command = "@druid remove curse", target = "raid" },
                                    }
                                },
                            }
                        },
                    },
                },
                {
                    name = "Baron Geddon",
                    triggers = {
                        -- Living Bomb
                        {
                            event = "SPELL_AURA_APPLIED",
                            debug = true,
                            spellId = 20475, -- Living Bomb
                            destIsGroupMember = false,
                            cooldown = 1.0,
                            commands = {
                                { command = "stay",              target = "affected" },
                                { command = "rtsc go BaronBomb", target = "affected" },
                            }
                        },
                        {
                            event = "SPELL_AURA_REMOVED",
                            spellId = 20475,
                            destIsGroupMember = false,
                            cooldown = 1.0,
                            commands = {
                                { command = "@ranged rtsc go BaronSafe", target = "affected" },
                                { command = "@melee @dps follow",        target = "affected" },
                                { command = "@dps attack rti",           target = "affected" },
                            }
                        },
                        -- Inferno
                        {
                            event = "SPELL_CAST_START",
                            spellId = 19695, -- Inferno
                            cooldown = 10.0,
                            commands = {
                                { command = "stay",                     target = "raid" },
                                { command = "@melee rtsc go BaronSafe", target = "raid" },
                                { command = "free",                     target = "mt",  delay = 10 }, -- Return after Inferno ends (8s + buffer)
                            }
                        },
                        -- Ignite Mana - alert for dispel (uncomment commands you want)
                        -- {
                        --     event = "SPELL_AURA_APPLIED",
                        --     spellId = 19659,  -- Ignite Mana
                        --     destIsGroupMember = true,
                        --     cooldown = 2.0,
                        --     commands = {
                        -- Option 1: Announce in raid (uncomment to use)
                        -- { command = "DISPEL {affected} - Ignite Mana!", target = "raid" },
                        -- Option 2: Tell healers to dispel (uncomment to use)
                        -- { command = "dispel {affected}", target = "healers" },
                        --     }
                        -- },
                    },
                    categories = {
                        {
                            name = "Setup",
                            collapsible = true,
                            buttons = {
                                {
                                    label = "SafeSpot",
                                    commands = {
                                        { command = "rtsc save exact BaronSafe", target = "raid" },
                                        { command = "aedm",                      target = "spell" },
                                    }
                                },
                                {
                                    label = "TankSpot",
                                    commands = {
                                        { command = "rtsc cancel",               target = "raid" },
                                        { command = "rtsc select",               target = "mt" },
                                        { command = "rtsc save exact BaronTank", target = "mt" },
                                        { command = "aedm",                      target = "spell" },
                                    }
                                },
                                {
                                    label = "MeleeSpot",
                                    commands = {
                                        { command = "@melee rtsc cancel",            target = "raid" },
                                        { command = "@melee rtsc select",            target = "mt" },
                                        { command = "rtsc save selected BaronMelee", target = "raid" },
                                        { command = "aedm",                          target = "spell" },
                                    }
                                },
                                {
                                    label = "BombSpot",
                                    commands = {
                                        { command = "rtsc save exact BaronBomb", target = "raid" },
                                        { command = "aedm",                      target = "spell" },
                                    }
                                },
                                {
                                    -- Baron Geddon: single tank. OT healers free for raid healing.
                                    label = "Healers",
                                    commands = {
                                        { command = "focus heal +{mt}", target = "mth"  },
                                        { command = "focus heal none",  target = "ot1h" },
                                        { command = "focus heal none",  target = "ot2h" },
                                    }
                                },
                                {
                                    label = "Douse Rune",
                                    commands = {
                                        { command = "u Eternal Quintessence", target = "raid" },
                                        { command = "u Aqual Quintessence",   target = "raid" },
                                    }
                                },
                            }
                        },
                        {
                            name = "Combat",
                            buttons = {
                                {
                                    label = "Go To Spots",
                                    commands = {
                                        { command = "free",                      target = "raid" },
                                        { command = "rtsc go BaronTank",         target = "mt" },
                                        { command = "@notank rtsc go BaronSafe", target = "raid" },
                                    }
                                },
                                {
                                    label = "Tank Follow",
                                    commands = {
                                        { command = "follow", target = "mt" },
                                    }
                                },

                                {
                                    label = "Attack Wait",
                                    commands = {
                                        { command = "co -passive",               target = "mt" },
                                        { command = "co +tank",                  target = "mt" },
                                        { command = "co +tank assist",           target = "mt" },
                                        { command = "co +dps assist",            target = "mt" },
                                        { command = "co +tank assist",           target = "mt" },
                                        { command = "co -threat",                target = "mt" },
                                        { command = "attack",                    target = "mt" },
                                        { command = "co -passive",               target = "raid" },
                                        { command = "nc -passive",               target = "raid" },
                                        { command = "@dps co -boost",            target = "raid" },
                                        { command = "@dps co +wait for attack",  target = "raid" },
                                        { command = "@tank co -wait for attack", target = "raid" },
                                        { command = "@dps wait for attack 15",   target = "raid" },
                                    }
                                },
                                {
                                    label = "Melee Go",
                                    commands = {
                                        { command = "@melee @dps follow", target = "raid" },
                                        { command = "@dps attack rti",    target = "raid" },
                                    }
                                },
                                {
                                    label = "Go Safe",
                                    commands = {
                                        { command = "free",              target = "raid" },
                                        { command = "rtsc go BaronSafe", target = "raid" },
                                    }
                                },
                            }
                        },
                    },
                },
                {
                    name = "Golemagg",
                    categories = {
                        {
                            name = "Setup",
                            collapsible = true,
                            buttons = {
                                {
                                    label = "Save Safe",
                                    commands = {
                                        { command = "rtsc save GoleSafe", target = "raid" },
                                        { command = "aedm",               target = "spell" },
                                    }
                                },

                                {
                                    label = "Select MT",
                                    commands = {
                                        { command = "rtsc cancel", target = "raid" },
                                        { command = "rtsc select", target = "mt" },
                                    }
                                },

                                {
                                    label = "Select OT1",
                                    commands = {
                                        { command = "rtsc cancel", target = "raid" },
                                        { command = "rtsc select", target = "ot1" },
                                    }
                                },

                                {
                                    label = "Select OT2",
                                    commands = {
                                        { command = "rtsc cancel", target = "raid" },
                                        { command = "rtsc select", target = "ot2" },
                                    }
                                },

                                {
                                    label = "Save TankSpot",
                                    commands = {
                                        { command = "rtsc save selected GoleTank", target = "raid" },
                                        { command = "aedm",                        target = "spell" },
                                    }
                                },
                                {
                                    -- Golemagg + 2 Core Rager adds: MT + OT1 + OT2 all active
                                    label = "Healers",
                                    commands = {
                                        { command = "focus heal +{mt}",  target = "mth"  },
                                        { command = "focus heal +{ot1}", target = "ot1h" },
                                        { command = "focus heal +{ot2}", target = "ot2h" },
                                    }
                                },
                                {
                                    label = "Douse Rune",
                                    commands = {
                                        { command = "u Eternal Quintessence", target = "raid" },
                                        { command = "u Aqual Quintessence",   target = "raid" },
                                    }
                                },
                            }
                        },
                        {
                            name = "Combat",
                            buttons = {
                                {
                                    label = "Go",
                                    commands = {
                                        { command = "@dps stay",                 target = "raid" },
                                        { command = "@ranged stay",              target = "raid" },
                                        { command = "@ranged rtsc go GoleSafe",  target = "raid" },
                                        { command = "@tank pull rti",            target = "raid" },
                                        { command = "@tank attack rti",          target = "raid" },
                                        { command = "@tank co -wait for attack", target = "raid" },
                                        { command = "@healer attack",            target = "raid" },
                                        { command = "@dps co -boost",            target = "raid" },
                                        { command = "@dps co +wait for attack",  target = "raid" },
                                        { command = "@dps attack",               target = "raid" },
                                        { command = "@dps wait for attack 10",   target = "raid" },
                                    }
                                },
                                {
                                    label = "Tank Go",
                                    commands = {
                                        { command = "@tank rtsc go GoleTank", target = "raid" },
                                    }
                                },
                                {
                                    label = "Melee Go",
                                    commands = {
                                        { command = "@melee @dps guard", target = "raid" },
                                        { command = "@dps attack rti",   target = "raid" },
                                        { command = "@ranged attack rti", target = "raid" },
                                    }
                                },
                                {
                                    label = "Melee Safe",
                                    commands = {
                                        { command = "stay",                    target = "raid", delay = 0.5 },
                                        { command = "@melee rtsc go GoleSafe", target = "raid" },
                                    }
                                },
                            }
                        },
                    },
                },
                {
                    name = "Sulfuron",
                    categories = {
                        {
                            -- Sulfuron + 4 Flamewaker Priest adds: MT on boss, OTs on adds
                            name = "Setup",
                            collapsible = true,
                            buttons = {
                                {
                                    label = "Healers",
                                    commands = {
                                        { command = "focus heal +{mt}",  target = "mth"  },
                                        { command = "focus heal +{ot1}", target = "ot1h" },
                                        { command = "focus heal +{ot2}", target = "ot2h" },
                                    }
                                },
                            }
                        },
                        {
                            name = "Combat",
                            buttons = {
                                {
                                    label = "Kill Healers",
                                    commands = {
                                        { command = "rti skull",        target = "raid" },
                                        { command = "@dps attack rti",  target = "raid" },
                                    }
                                },
                                {
                                    label = "Interrupt",
                                    commands = {
                                        { command = "@rogue cast Kick",         target = "raid" },
                                        { command = "@warrior cast Pummel",     target = "raid" },
                                        { command = "@shaman cast Earth Shock", target = "raid" },
                                    }
                                },
                                { label = "Kill Boss", command = "@dps attack rti", target = "raid" },
                            }
                        },
                    },
                },
                {
                    name = "Majordomo",
                    categories = {
                        {
                            name = "Setup",
                            collapsible = true,
                            buttons = {
                                {
                                    label = "Tank Pos",
                                    commands = {
                                        { command = "rtsc cancel",               target = "raid" },
                                        { command = "rtsc save exact MajorTank", target = "target" },
                                        { command = "aedm",                      target = "spell" },
                                    }
                                },
                                {
                                    label = "Safe Pos",
                                    commands = {
                                        { command = "rtsc cancel",                 target = "raid" },
                                        { command = "@ranged rtsc select",         target = "raid" },
                                        { command = "@ranged rtsc save MajorSafe", target = "raid" },
                                        { command = "aedm",                        target = "spell" },
                                    }
                                },
                                {
                                    -- Majordomo: 4 Flamewaker Elites need tanks; all 3 healers on tanks
                                    label = "Healers",
                                    commands = {
                                        { command = "focus heal +{mt}",  target = "mth"  },
                                        { command = "focus heal +{ot1}", target = "ot1h" },
                                        { command = "focus heal +{ot2}", target = "ot2h" },
                                    }
                                },
                            }
                        },
                        {
                            name = "Combat",
                            buttons = {
                                {
                                    label = "Tanks Taunt",
                                    commands = {
                                        { command = "@tank cast Taunt", target = "raid" },
                                    }
                                },
                                {
                                    label = "Attack",
                                    commands = {
                                        { command = "nc -aoe",                  target = "raid" },
                                        { command = "co -aoe",                  target = "raid" },
                                        { command = "@tank attack rti",         target = "raid" },
                                        { command = "@healer attack",           target = "raid" },
                                        { command = "@mage nc +cc",             target = "raid" },
                                        { command = "@mage co +cc",             target = "raid" },
                                        { command = "@dps co +boost",           target = "raid" },
                                        { command = "@dps co +wait for attack", target = "raid" },
                                        { command = "@dps wait for attack 10",  target = "raid" },
                                        { command = "@dps attack rti",          target = "raid" },
                                    }
                                },
                                {
                                    label = "Off-attack",
                                    commands = {
                                        { command = "attack", target = "w:Sordok" },
                                    }
                                },
                                {
                                    label = "Off-taunt",
                                    commands = {
                                        { command = "cast Taunt", target = "w:Sordok" },
                                    }
                                },
                                {
                                    label = "Pull",
                                    commands = {
                                        { command = "@ranged rtsc go MajorSafe", target = "raid" },
                                        { command = "@tank rtsc go MajorTank",   target = "raid" },
                                        { command = "@tank co -wait for attack", target = "raid" },
                                        { command = "@tank pull rti",            target = "raid" },
                                        { command = "@tank attack rti",          target = "raid" },
                                    }
                                },
                                {
                                    label = "Tank Go Pos",
                                    commands = {
                                        { command = "@tank guard",             target = "raid" },
                                        { command = "@tank rtsc go MajorTank", target = "raid" },
                                    }
                                },
                                {
                                    label = "Go Pos",
                                    commands = {
                                        { command = "@tank guard",               target = "raid" },
                                        { command = "@tank rtsc go MajorTank",   target = "raid" },
                                        { command = "@ranged stay",              target = "raid" },
                                        { command = "@ranged rtsc go MajorSafe", target = "raid" },
                                    }
                                },
                            },
                        },
                    },
                },

                {
                    name = "Ragnaros",
                    categories = {
                        {
                            name = "Setup",
                            collapsible = true,
                            buttons = {
                                {
                                    label = "Ranged Pos",
                                    commands = {
                                        { command = "rtsc cancel",               target = "raid" },
                                        { command = "@ranged @dps rtsc select",  target = "raid" },
                                        { command = "@ranged @dps rtsc save Rag1", target = "raid" },
                                        { command = "aedm",                      target = "spell" },
                                    }
                                },
                                {
                                    label = "Melee Pos",
                                    commands = {
                                        { command = "rtsc cancel",                 target = "raid" },
                                        { command = "@melee @dps rtsc select",     target = "raid" },
                                        { command = "@melee @dps rtsc save Rag1",  target = "raid" },
                                        { command = "aedm",                        target = "spell" },
                                    }
                                },
                                {
                                    label = "Melee Out",
                                    commands = {
                                        { command = "rtsc cancel",                             target = "raid" },
                                        { command = "@melee @dps rtsc select",                 target = "raid" },
                                        { command = "@melee @dps rtsc save exact RagMeleeOut", target = "raid" },
                                        { command = "aedm",                                    target = "spell" },
                                    }
                                },
                                {
                                    label = "Tank Pos",
                                    commands = {
                                        { command = "rtsc cancel",                target = "raid" },
                                        { command = "@tank rtsc save exact Rag1", target = "target" },
                                        { command = "aedm",                       target = "spell" },
                                    }
                                },
                                {
                                    -- Ragnaros: MT + OT1 swap on knockback. OT2 healer free for raid.
                                    label = "Healers",
                                    commands = {
                                        { command = "focus heal +{mt}",  target = "mth"  },
                                        { command = "focus heal +{ot1}", target = "ot1h" },
                                        { command = "focus heal none",   target = "ot2h" },
                                    }
                                },
                            }
                        },
                        {
                            name = "Combat",
                            collapsible = false,
                            buttons = {
                                {
                                    label = "Go Pos",
                                    commands = {
                                        { command = "rtsc go Rag1", target = "raid" },
                                    }
                                },
                                {
                                    label = "MT Out",
                                    commands = {
                                        { command = "rtsc go RagTankOut",              target = "mt" },
                                        { command = "@priest focus heal none",         target = "raid" },
                                        { command = "@priest focus heal +{ot1}",       target = "raid" },
                                        { command = "@melee @dps rtsc go RagMeleeOut", target = "raid" },
                                    }
                                },
                                {
                                    label = "OT Out",
                                    commands = {
                                        { command = "rtsc go RagTankOut",              target = "ot1" },
                                        { command = "@priest focus heal none",         target = "raid" },
                                        { command = "@priest focus heal +{mt}",        target = "raid" },
                                        { command = "@melee @dps rtsc go RagMeleeOut", target = "raid" },
                                    }
                                },
                                {
                                    label = "MT Attack",
                                    commands = {
                                        { command = "attack rti",                                 target = "mt" },
                                        { command = "@priest focus +{mt}",                        target = "raid" },
                                        { command = "co -threat -dps assist -close +tank assist", target = "mt" },
                                        { command = "co +threat +dps assist +close -tank assist", target = "ot1" },
                                        { command = "@melee @dps rtsc go RagMeleeOut",            target = "raid" },
                                    }
                                },
                                {
                                    label = "OT Attack",
                                    commands = {
                                        { command = "attack rti",                                 target = "ot1" },
                                        { command = "co -threat -dps assist -close +tank assist", target = "ot1" },
                                        { command = "co +threat +dps assist +close -tank assist", target = "mt" },
                                        { command = "@melee @dps rtsc go RagMeleeOut",            target = "raid" },
                                    }
                                },
                                {
                                    label = "Sons",
                                    commands = {
                                        { command = "stay",        target = "raid" },
                                        { command = "@dps attack", target = "raid" },
                                    }
                                },
                            }
                        },
                    },
                },
            },
        },
        {
            name = "Blackwing Lair",
            bosses = {
                {
                    name = "Razorgore",
                    categories = {
                        {
                            name = "Phase 1 (Orb)",
                            collapsible = true,
                            buttons = {
                                { label = "Kite", command = ".bot kite", target = "ot1" },
                            }
                        },
                        {
                            name = "Phase 2",
                            buttons = {
                                { label = "P2 Boss", command = ".bot attack boss", target = "raid" },
                            }
                        },
                    },
                },
                {
                    name = "Vaelastrasz",
                    categories = {
                        {
                            name = "Mechanics",
                            buttons = {
                                { label = "BA Run!",     command = ".bot spread 50",    target = "raid" },
                                { label = "Threat Dump", command = ".bot threat reset", target = "raid" },
                                { label = "Full DPS",    command = ".bot attack full",  target = "raid" },
                            }
                        },
                    },
                },
                {
                    name = "Broodlord",
                    categories = {
                        {
                            name = "Combat",
                            buttons = {
                                { label = "Clear Traps", command = ".bot attack trap", target = "raid" },
                            }
                        },
                    },
                },
                {
                    name = "Firemaw",
                    categories = {
                        {
                            name = "Combat",
                            buttons = {
                                { label = "LoS Reset", command = ".bot los reset", target = "raid" },
                            }
                        },
                    },
                },
                {
                    name = "Ebonroc",
                    categories = {},
                },
                {
                    name = "Flamegor",
                    categories = {
                        {
                            name = "Combat",
                            buttons = {
                                { label = "Tranq Shot", command = ".bot cast tranquilizing shot", target = "raid" },
                            }
                        },
                    },
                },
                {
                    name = "Chromaggus",
                    categories = {
                        {
                            name = "Mechanics",
                            buttons = {
                                { label = "Hourglass",  command = ".bot use [Hourglass Sand]",    target = "raid" },
                                { label = "Tranq Shot", command = ".bot cast tranquilizing shot", target = "raid" },
                                { label = "Dispel",     command = ".bot cast dispel magic",       target = "raid" },
                            }
                        },
                    },
                },
                {
                    name = "Nefarian",
                    categories = {
                        {
                            name = "Phase 1 (Adds)",
                            collapsible = true,
                            buttons = {
                                { label = "P1 Adds", command = ".bot attack add", target = "raid" },
                            }
                        },
                        {
                            name = "Phase 2",
                            buttons = {
                                { label = "P2 Boss",    command = ".bot attack boss", target = "raid" },
                                { label = "Class Call", command = ".bot class call",  target = "raid" },
                            }
                        },
                    },
                },
            }
        },
        {
            name = "Onyxia",
            setup = {
                -- Formation: tight initially, ranged spread out per phase via RTSC buttons
                { command = "formation raid",                 target = "raid" },
                { command = "range follow 0",                 target = "raid" },
                { command = "range followraid 0",             target = "raid" },
                -- Ranged DPS spread out from each other to avoid Wing Buffet chain
                { command = "@ranged @dps range follow 10",     target = "raid" },
                { command = "@ranged @dps range followraid 10", target = "raid" },
                -- Shamans: tremor totems in case of fear effects
                { command = "@shaman nc +totem earth tremor", target = "raid" },
                { command = "@shaman co +totem earth tremor", target = "raid" },
                -- Kill target via skull marker, DPS hold threat
                { command = "rti skull",                      target = "raid" },
                { command = "@dps co +threat",                target = "raid" },
                -- MT healer: focus heal on MT and apply Fear Ward
                { command = "focus heal +{mt}",               target = "mth" },
                { command = "buff target +{mt}",              target = "mth" },
            },
            bosses = {
                {
                    name = "Onyxia",
                    categories = {
                        {
                            name = "Setup",
                            collapsible = true,
                            buttons = {
                                {
                                    label = "MT P1",
                                    commands = {
                                        { command = "rtsc cancel",              target = "raid" },
                                        { command = "rtsc save exact OnySpot1", target = "mt" },
                                        { command = "aedm",                     target = "spell" },
                                    }
                                },
                                {
                                    label = "MT P2",
                                    commands = {
                                        { command = "rtsc cancel",              target = "raid" },
                                        { command = "rtsc save exact OnySpot2", target = "mt" },
                                        { command = "aedm",                     target = "spell" },
                                    }
                                },
                                {
                                    label = "Raid P2",
                                    commands = {
                                        { command = "rtsc cancel",        target = "raid" },
                                        { command = "rtsc save OnySpot2", target = "raid" },
                                        { command = "aedm",               target = "spell" },
                                    }
                                },
                                {
                                    label = "Melee P1",
                                    commands = {
                                        { command = "@melee @dps range followraid 1",  target = "raid" },
                                        { command = "@melee @dps formation near",      target = "raid" },
                                        { command = "@melee @dps rtsc save OnySpot1",  target = "raid" },
                                        { command = "aedm",                            target = "spell" },
                                    }
                                },
                                {
                                    label = "Ranged P1",
                                    commands = {
                                        { command = "@ranged @dps range follow 10",     target = "raid" },
                                        { command = "@ranged @dps range followraid 10", target = "raid" },
                                        { command = "@ranged @dps formation far",        target = "raid" },
                                        { command = "@ranged @dps rtsc save OnySpot1",   target = "raid" },
                                        { command = "aedm",                              target = "spell" },
                                    }
                                },
                                {
                                    label = "P2-1",
                                    commands = {
                                        { command = "rtsc cancel",         target = "raid" },
                                        { command = "rtsc save OnySpot21", target = "raid" },
                                        { command = "aedm",                target = "spell" },
                                    }
                                },
                                {
                                    label = "P2-2",
                                    commands = {
                                        { command = "rtsc cancel",         target = "raid" },
                                        { command = "rtsc save OnySpot22", target = "raid" },
                                        { command = "aedm",                target = "spell" },
                                    }
                                },
                                {
                                    label = "P2-3",
                                    commands = {
                                        { command = "rtsc cancel",         target = "raid" },
                                        { command = "rtsc save OnySpot23", target = "raid" },
                                        { command = "aedm",                target = "spell" },
                                    }
                                },
                                {
                                    label = "P2-4",
                                    commands = {
                                        { command = "rtsc cancel",         target = "raid" },
                                        { command = "rtsc save OnySpot24", target = "raid" },
                                        { command = "aedm",                target = "spell" },
                                    }
                                },
                            },
                        },
                        {
                            name = "Phase 1",
                            buttons = {
                                {
                                    label = "Go spot",
                                    commands = {
                                        { command = "rtsc go OnySpot1", target = "raid" },
                                    }
                                },
                                {
                                    label = "Tank DPS",
                                    commands = {
                                        { command = "co -passive,+grind",       target = "mt" },
                                        { command = "nc -passive,+grind",       target = "mt" },
                                        { command = "@tank attack Onyxia",      target = "raid" },
                                        { command = "@dps co +wait for attack", target = "raid" },
                                        { command = "@dps wait for attack 15",  target = "raid" },
                                    }
                                },
                                {
                                    label = "Stop DPS",
                                    commands = {
                                        { command = "@dps co +passive,-grind", target = "raid" },
                                        { command = "@dps nc +passive,-grind", target = "raid" },
                                        { command = "@dps stop attack",        target = "raid" },
                                    }
                                },
                                {
                                    label = "Slow DPS",
                                    commands = {
                                        { command = "@dps stance near",                       target = "raid" },
                                        { command = "@dps co -boost,-passive,+grind,-behind", target = "raid" },
                                        { command = "@dps nc -passive,+grind",                target = "raid" },
                                        { command = "@mage ss Frostbolt",                     target = "raid" },
                                        { command = "@warrior ss Death Wish",                 target = "raid" },
                                        { command = "@hunter ss Aimed Shot",                  target = "raid" },
                                        { command = "@rogue ss Sinister Strike",              target = "raid" },
                                        { command = "@druid ss Shred",                        target = "raid" },
                                        { command = "@dps attack Onyxia",                     target = "raid" },
                                    }
                                },
                                {
                                    label = "High DPS",
                                    commands = {
                                        { command = "@dps stance near",                       target = "raid" },
                                        { command = "@dps co +boost,-passive,+grind,-behind", target = "raid" },
                                        { command = "@dps nc -passive,+grind",                target = "raid" },
                                        { command = "@mage ss -Frostbolt",                    target = "raid" },
                                        { command = "@warrior ss -Death Wish",                target = "raid" },
                                        { command = "@hunter ss -Aimed Shot",                 target = "raid" },
                                        { command = "@rogue ss -Sinister Strike",             target = "raid" },
                                        { command = "@druid ss -Shred",                       target = "raid" },
                                        { command = "@dps attack Onyxia",                     target = "raid" },
                                    }
                                },
                            }
                        },
                        {
                            name = "Phase 2",
                            buttons = {
                                {
                                    label = "Go spots",
                                    commands = {
                                        { command = "rtsc go OnySpot2",   target = "raid" },
                                        { command = "@ranged attack rti", target = "raid" },
                                    }
                                },
                                {
                                    label = "Go 1",
                                    commands = {
                                        { command = "rtsc go OnySpot21", target = "raid" },
                                    }
                                },
                                {
                                    label = "Go 2",
                                    commands = {
                                        { command = "rtsc go OnySpot22", target = "raid" },
                                    }
                                },
                                {
                                    label = "Go 3",
                                    commands = {
                                        { command = "rtsc go OnySpot23", target = "raid" },
                                    }
                                },
                                {
                                    label = "Go 4",
                                    commands = {
                                        { command = "rtsc go OnySpot24", target = "raid" },
                                    }
                                },
                                {
                                    label = "Taunt",
                                    commands = {
                                        { command = "rtsc go OnySpot2",             target = "raid" },
                                        { command = "@tank cast Challenging Shout", target = "raid" },
                                    }
                                },
                            }
                        },
                        {
                            name = "Phase 3",
                            buttons = {
                                {
                                    label = "Go spots",
                                    commands = {
                                        { command = "rtsc go OnySpot1", target = "raid" },
                                    }
                                },
                                {
                                    label = "Full DPS",
                                    commands = {
                                        { command = "@mage ss -Frostbolt",                    target = "raid" },
                                        { command = "@warrior ss -Death Wish",                target = "raid" },
                                        { command = "@hunter ss -Aimed Shot",                 target = "raid" },
                                        { command = "@rogue ss -Sinister Strike",             target = "raid" },
                                        { command = "@druid ss -Shred",                       target = "raid" },
                                        { command = "@dps co +boost,-passive,+grind,-behind", target = "raid" },
                                        { command = "@dps nc -passive,+grind",                target = "raid" },
                                        { command = "@dps attack Onyxia",                     target = "raid" },
                                    }
                                },
                                {
                                    label = "Whelps",
                                    commands = {
                                        { command = "@tank cast Challenging Shout", target = "raid" },
                                        { command = "@dps attack",                  target = "raid" },
                                    }
                                },
                            }
                        },
                    },
                }
            },
        },
        {
            name = "Zul'Gurub",
            bosses = {
                {
                    name = "Hakkar",
                    categories = {
                        {
                            name = "Combat",
                            buttons = {
                                { label = "Kill Sons", command = ".bot attack add",          target = "raid" },
                                { label = "Stack",     command = ".bot stack",               target = "raid" },
                                { label = "Cleanse",   command = ".bot cast abolish poison", target = "raid" },
                            }
                        },
                    },
                },
            }
        },
        {
            name = "AQ40",
            bosses = {
                {
                    name = "Skeram",
                    categories = {
                        {
                            name = "Combat",
                            buttons = {
                                { label = "Kill Clone", command = ".bot attack clone", target = "raid" },
                            }
                        },
                    },
                },
                {
                    name = "Twin Emperors",
                    categories = {
                        {
                            name = "Combat",
                            buttons = {
                                { label = "Teleport!", command = ".bot swap tanks", target = "tanks" },
                            }
                        },
                    },
                },
                {
                    name = "C'Thun",
                    categories = {
                        {
                            name = "Phase 1 (Eye)",
                            collapsible = true,
                            buttons = {
                                { label = "Eye Beam", command = ".bot spread 10", target = "raid" },
                            }
                        },
                        {
                            name = "Phase 2",
                            buttons = {
                                { label = "Stomach",  command = ".bot attack tentacle", target = "raid" },
                                { label = "Weakened", command = ".bot attack full",     target = "raid" },
                            }
                        },
                    },
                },
            }
        },
        {
            name = "Naxxramas",
            bosses = {
                {
                    name = "Patchwerk",
                    categories = {
                        {
                            name = "Combat",
                            buttons = {
                                { label = "Hateful OT", command = ".bot hateful", target = "ot1" },
                            }
                        },
                    },
                },
                {
                    name = "Four Horsemen",
                    categories = {
                        {
                            name = "Combat",
                            buttons = {
                                { label = "Rotate!",   command = ".bot rotate",     target = "tanks" },
                                { label = "Mark Swap", command = ".bot swap marks", target = "raid" },
                            }
                        },
                    },
                },
            }
        },
    }
}


-- {
--     name = "All",
--     collapsible = true,
--     buttons = {
--         {
--             label = "Bot Prep",
--             commands = {
--                 { command = ".bot learn", target = "raid" },
--                 { command = ".bot train", target = "raid" },
--             }
--         },
--         {
--             label = "Bags",
--             commands = {
--                 { command = ".additem 14156", target = "mt", delay = 1.0 },
--                 { command = ".additem 14156", target = "mt", delay = 1.0 },
--                 { command = ".additem 14156", target = "mt", delay = 1.0 },
--                 { command = ".additem 14156", target = "mt", delay = 1.0 },
--                 -- { command = "e 14156",        target = "raid", delay = 1.0 },
--             }
--         },
--     },
-- },
-- {
--     name = "Fury Warrior",
--     collapsible = true,
--     buttons = {
--         {
--             label = "Keep Items",
--             commands = {
--                 { command = "keep 13404", target = "raid", delay = 1.0 },
--                 { command = "keep 15411", target = "raid", delay = 1.0 },
--                 { command = "keep 12927", target = "raid", delay = 1.0 },
--                 { command = "keep 13340", target = "raid", delay = 1.0 },
--                 { command = "keep 11726", target = "raid", delay = 1.0 },
--                 { command = "keep 12936", target = "raid", delay = 1.0 },
--                 { command = "keep 15063", target = "raid", delay = 1.0 },
--                 { command = "keep 13959", target = "raid", delay = 1.0 },
--                 { command = "keep 15062", target = "raid", delay = 1.0 },
--                 { command = "keep 12555", target = "raid", delay = 1.0 },
--                 { command = "keep 13098", target = "raid", delay = 1.0 },
--                 { command = "keep 18500", target = "raid", delay = 1.0 },
--                 { command = "keep 11815", target = "raid", delay = 1.0 },
--                 { command = "keep 13965", target = "raid", delay = 1.0 },
--                 { command = "keep 12940", target = "raid", delay = 1.0 },
--                 { command = "keep 12939", target = "raid", delay = 1.0 },
--                 { command = "keep 18323", target = "raid", delay = 1.0 },
--             }
--         },
--         {
--             label = "Give Items",
--             commands = {
--                 { command = ".additem 13404", target = "mt", delay = 1.0 },
--                 { command = ".additem 15411", target = "mt", delay = 1.0 },
--                 { command = ".additem 12927", target = "mt", delay = 1.0 },
--                 { command = ".additem 13340", target = "mt", delay = 1.0 },
--                 { command = ".additem 11726", target = "mt", delay = 1.0 },
--                 { command = ".additem 12936", target = "mt", delay = 1.0 },
--                 { command = ".additem 15063", target = "mt", delay = 1.0 },
--                 { command = ".additem 13959", target = "mt", delay = 1.0 },
--                 { command = ".additem 15062", target = "mt", delay = 1.0 },
--                 { command = ".additem 12555", target = "mt", delay = 1.0 },
--                 { command = ".additem 13098", target = "mt", delay = 1.0 },
--                 { command = ".additem 18500", target = "mt", delay = 1.0 },
--                 { command = ".additem 11815", target = "mt", delay = 1.0 },
--                 { command = ".additem 13965", target = "mt", delay = 1.0 },
--                 { command = ".additem 12940", target = "mt", delay = 1.0 },
--                 { command = ".additem 12939", target = "mt", delay = 1.0 },
--                 { command = ".additem 18323", target = "mt", delay = 1.0 },
--             }
--         },
--         {
--             label = "Equip Items",
--             commands = {
--                 { command = "e 13404", target = "raid", delay = 1.0 },
--                 { command = "e 15411", target = "raid", delay = 1.0 },
--                 { command = "e 12927", target = "raid", delay = 1.0 },
--                 { command = "e 13340", target = "raid", delay = 1.0 },
--                 { command = "e 11726", target = "raid", delay = 1.0 },
--                 { command = "e 12936", target = "raid", delay = 1.0 },
--                 { command = "e 15063", target = "raid", delay = 1.0 },
--                 { command = "e 13959", target = "raid", delay = 1.0 },
--                 { command = "e 15062", target = "raid", delay = 1.0 },
--                 { command = "e 12555", target = "raid", delay = 1.0 },
--                 { command = "e 13098", target = "raid", delay = 1.0 },
--                 { command = "e 18500", target = "raid", delay = 1.0 },
--                 { command = "e 11815", target = "raid", delay = 1.0 },
--                 { command = "e 13965", target = "raid", delay = 1.0 },
--                 { command = "e 12940", target = "raid", delay = 1.0 },
--                 { command = "e 12939", target = "raid", delay = 1.0 },
--                 { command = "e 18323", target = "raid", delay = 1.0 },
--             }
--         },
--     },
-- },
-- {
--     name = "Tank Warrior",
--     collapsible = true,
--     buttons = {
--         {
--             label = "Keep Items",
--             commands = {
--                 { command = "keep 12952", target = "raid" },
--                 { command = "keep 13091", target = "raid" },
--                 { command = "keep 14552", target = "raid" },
--                 { command = "keep 18413", target = "raid" },
--                 { command = "keep 14624", target = "raid" },
--                 { command = "keep 12550", target = "raid" },
--                 { command = "keep 14525", target = "raid" },
--                 { command = "keep 14620", target = "raid", delay = 1.0 },
--                 { command = "keep 11927", target = "raid" },
--                 { command = "keep 14621", target = "raid" },
--                 { command = "keep 11669", target = "raid" },
--                 { command = "keep 22331", target = "raid" },
--                 { command = "keep 11810", target = "raid" },
--                 { command = "keep 10779", target = "raid" },
--                 { command = "keep 15806", target = "raid" },
--                 { command = "keep 12602", target = "raid" },
--                 { command = "keep 12651", target = "raid" },
--
--             }
--         },
--         {
--             label = "Give Items",
--             commands = {
--                 { command = ".additem 12952", target = "mt", delay = 1.0 },
--                 { command = ".additem 13091", target = "mt", delay = 1.0 },
--                 { command = ".additem 14552", target = "mt", delay = 1.0 },
--                 { command = ".additem 18413", target = "mt", delay = 1.0 },
--                 { command = ".additem 14624", target = "mt", delay = 1.0 },
--                 { command = ".additem 12550", target = "mt", delay = 1.0 },
--                 { command = ".additem 14525", target = "mt", delay = 1.0 },
--                 { command = ".additem 14620", target = "mt", delay = 1.0 },
--                 { command = ".additem 11927", target = "mt", delay = 1.0 },
--                 { command = ".additem 14621", target = "mt", delay = 1.0 },
--                 { command = ".additem 11669", target = "mt", delay = 1.0 },
--                 { command = ".additem 22331", target = "mt", delay = 1.0 },
--                 { command = ".additem 11810", target = "mt", delay = 1.0 },
--                 { command = ".additem 10779", target = "mt", delay = 1.0 },
--                 { command = ".additem 15806", target = "mt", delay = 1.0 },
--                 { command = ".additem 12602", target = "mt", delay = 1.0 },
--                 { command = ".additem 12651", target = "mt", delay = 1.0 },
--             }
--         },
--         {
--             label = "Equip Items",
--             commands = {
--                 { command = "e 12952", target = "raid", delay = 1.0 },
--                 { command = "e 13091", target = "raid", delay = 1.0 },
--                 { command = "e 14552", target = "raid", delay = 1.0 },
--                 { command = "e 18413", target = "raid", delay = 1.0 },
--                 { command = "e 14624", target = "raid", delay = 1.0 },
--                 { command = "e 12550", target = "raid", delay = 1.0 },
--                 { command = "e 14525", target = "raid", delay = 1.0 },
--                 { command = "e 14620", target = "raid", delay = 1.0 },
--                 { command = "e 11927", target = "raid", delay = 1.0 },
--                 { command = "e 14621", target = "raid", delay = 1.0 },
--                 { command = "e 11669", target = "raid", delay = 1.0 },
--                 { command = "e 22331", target = "raid", delay = 1.0 },
--                 { command = "e 11810", target = "raid", delay = 1.0 },
--                 { command = "e 10779", target = "raid", delay = 1.0 },
--                 { command = "e 15806", target = "raid", delay = 1.0 },
--                 { command = "e 12602", target = "raid", delay = 1.0 },
--                 { command = "e 12651", target = "raid", delay = 1.0 },
--             }
--         },
--     },
-- },
--
-- {
--     name = "Priest",
--     collapsible = true,
--     buttons = {
--         {
--             label = "Give Items",
--             commands = {
--                 { command = ".additem 13102", target = "mt", delay = 1.0 },
--                 { command = ".additem 13141", target = "mt", delay = 1.0 },
--                 { command = ".additem 13013", target = "mt", delay = 1.0 },
--                 { command = ".additem 13386", target = "mt", delay = 1.0 },
--                 { command = ".additem 14154", target = "mt", delay = 1.0 },
--                 { command = ".additem 13107", target = "mt", delay = 1.0 },
--                 { command = ".additem 12554", target = "mt", delay = 1.0 },
--                 { command = ".additem 14143", target = "mt", delay = 1.0 },
--                 { command = ".additem 11841", target = "mt", delay = 1.0 },
--                 { command = ".additem 11822", target = "mt", delay = 1.0 },
--                 { command = ".additem 16058", target = "mt", delay = 1.0 },
--                 { command = ".additem 13178", target = "mt", delay = 1.0 },
--                 { command = ".additem 11819", target = "mt", delay = 1.0 },
--                 { command = ".additem 12930", target = "mt", delay = 1.0 },
--                 { command = ".additem 11923", target = "mt", delay = 1.0 },
--                 { command = ".additem 11928", target = "mt", delay = 1.0 },
--                 { command = ".additem 16997", target = "mt", delay = 1.0 },
--             }
--         },
--         {
--             label = "Equip Items",
--             commands = {
--                 { command = "@priest e 13102", target = "raid", delay = 1.0 },
--                 { command = "@priest e 13141", target = "raid", delay = 1.0 },
--                 { command = "@priest e 13013", target = "raid", delay = 1.0 },
--                 { command = "@priest e 13386", target = "raid", delay = 1.0 },
--                 { command = "@priest e 14154", target = "raid", delay = 1.0 },
--                 { command = "@priest e 13107", target = "raid", delay = 1.0 },
--                 { command = "@priest e 12554", target = "raid", delay = 1.0 },
--                 { command = "@priest e 14143", target = "raid", delay = 1.0 },
--                 { command = "@priest e 11841", target = "raid", delay = 1.0 },
--                 { command = "@priest e 11822", target = "raid", delay = 1.0 },
--                 { command = "@priest e 16058", target = "raid", delay = 1.0 },
--                 { command = "@priest e 13178", target = "raid", delay = 1.0 },
--                 { command = "@priest e 11819", target = "raid", delay = 1.0 },
--                 { command = "@priest e 12930", target = "raid", delay = 1.0 },
--                 { command = "@priest e 11923", target = "raid", delay = 1.0 },
--                 { command = "@priest e 11928", target = "raid", delay = 1.0 },
--                 { command = "@priest e 16997", target = "raid", delay = 1.0 },
--             }
--         },
--     },
-- },
--
-- {
--     name = "Resto Druid",
--     collapsible = true,
--     buttons = {
--         {
--             label = "Give Items",
--             commands = {
--                 { command = ".additem 13102", target = "mt", delay = 1.0 },
--                 { command = ".additem 13141", target = "mt", delay = 1.0 },
--                 { command = ".additem 15061", target = "mt", delay = 1.0 },
--                 { command = ".additem 13386", target = "mt", delay = 1.0 },
--                 { command = ".additem 13346", target = "mt", delay = 1.0 },
--                 { command = ".additem 13208", target = "mt", delay = 1.0 },
--                 { command = ".additem 12554", target = "mt", delay = 1.0 },
--                 { command = ".additem 14553", target = "mt", delay = 1.0 },
--                 { command = ".additem 15060", target = "mt", delay = 1.0 },
--                 { command = ".additem 13954", target = "mt", delay = 1.0 },
--                 { command = ".additem 13178", target = "mt", delay = 1.0 },
--                 { command = ".additem 16058", target = "mt", delay = 1.0 },
--                 { command = ".additem 12930", target = "mt", delay = 1.0 },
--                 { command = ".additem 11819", target = "mt", delay = 1.0 },
--                 { command = ".additem 11923", target = "mt", delay = 1.0 },
--                 { command = ".additem 11928", target = "mt", delay = 1.0 },
--                 { command = ".additem 13396", target = "mt", delay = 1.0 },
--                 { command = ".additem 23197", target = "mt", delay = 1.0 },
--             }
--         },
--         {
--             label = "Equip Items",
--             commands = {
--                 { command = "@druid e 13102", target = "raid", delay = 1.0 },
--                 { command = "@druid e 13141", target = "raid", delay = 1.0 },
--                 { command = "@druid e 15061", target = "raid", delay = 1.0 },
--                 { command = "@druid e 13386", target = "raid", delay = 1.0 },
--                 { command = "@druid e 13346", target = "raid", delay = 1.0 },
--                 { command = "@druid e 13208", target = "raid", delay = 1.0 },
--                 { command = "@druid e 12554", target = "raid", delay = 1.0 },
--                 { command = "@druid e 14553", target = "raid", delay = 1.0 },
--                 { command = "@druid e 15060", target = "raid", delay = 1.0 },
--                 { command = "@druid e 13954", target = "raid", delay = 1.0 },
--                 { command = "@druid e 13178", target = "raid", delay = 1.0 },
--                 { command = "@druid e 16058", target = "raid", delay = 1.0 },
--                 { command = "@druid e 12930", target = "raid", delay = 1.0 },
--                 { command = "@druid e 11819", target = "raid", delay = 1.0 },
--                 { command = "@druid e 11923", target = "raid", delay = 1.0 },
--                 { command = "@druid e 11928", target = "raid", delay = 1.0 },
--                 { command = "@druid e 13396", target = "raid", delay = 1.0 },
--                 { command = "@druid e 23197", target = "raid", delay = 1.0 },
--             }
--         },
--     },
-- },
--
-- {
--     name = "All",
--     collapsible = true,
--     buttons = {
--         {
--             label = "Bot Prep",
--             commands = {
--                 { command = ".bot learn", target = "raid" },
--                 { command = ".bot train", target = "raid" },
--             }
--         },
--         {
--             label = "Bags",
--             commands = {
--                 { command = ".additem 14156", target = "mt", delay = 1.0 },
--                 { command = ".additem 14156", target = "mt", delay = 1.0 },
--                 { command = ".additem 14156", target = "mt", delay = 1.0 },
--                 { command = ".additem 14156", target = "mt", delay = 1.0 },
--                 -- { command = "e 14156",        target = "raid", delay = 1.0 },
--             }
--         },
--     },
-- },
-- {
--     name = "Resto Shaman",
--     collapsible = true,
--     buttons = {
--         {
--             label = "Give Items",
--             commands = {
--                 { command = ".additem 13102", target = "mt", delay = 1.0 },
--                 { command = ".additem 18723", target = "mt", delay = 1.0 },
--                 { command = ".additem 14112", target = "mt", delay = 1.0 },
--                 { command = ".additem 18389", target = "mt", delay = 1.0 },
--                 { command = ".additem 13346", target = "mt", delay = 1.0 },
--                 { command = ".additem 16696", target = "mt", delay = 1.0 },
--                 { command = ".additem 12554", target = "mt", delay = 1.0 },
--                 { command = ".additem 11662", target = "mt", delay = 1.0 },
--                 { command = ".additem 13170", target = "mt", delay = 1.0 },
--                 { command = ".additem 13954", target = "mt", delay = 1.0 },
--                 { command = ".additem 13178", target = "mt", delay = 1.0 },
--                 { command = ".additem 11819", target = "mt", delay = 1.0 },
--                 { command = ".additem 12930", target = "mt", delay = 1.0 },
--                 { command = ".additem 13968", target = "mt", delay = 1.0 },
--                 { command = ".additem 11932", target = "mt", delay = 1.0 },
--                 { command = ".additem 23200", target = "mt", delay = 1.0 },
--             }
--         },
--         {
--             label = "Equip Items",
--             commands = {
--                 { command = "@shaman e 13102", target = "raid", delay = 1.0 },
--                 { command = "@shaman e 18723", target = "raid", delay = 1.0 },
--                 { command = "@shaman e 14112", target = "raid", delay = 1.0 },
--                 { command = "@shaman e 18389", target = "raid", delay = 1.0 },
--                 { command = "@shaman e 13346", target = "raid", delay = 1.0 },
--                 { command = "@shaman e 16696", target = "raid", delay = 1.0 },
--                 { command = "@shaman e 12554", target = "raid", delay = 1.0 },
--                 { command = "@shaman e 11662", target = "raid", delay = 1.0 },
--                 { command = "@shaman e 13170", target = "raid", delay = 1.0 },
--                 { command = "@shaman e 13954", target = "raid", delay = 1.0 },
--                 { command = "@shaman e 13178", target = "raid", delay = 1.0 },
--                 { command = "@shaman e 11819", target = "raid", delay = 1.0 },
--                 { command = "@shaman e 12930", target = "raid", delay = 1.0 },
--                 { command = "@shaman e 13968", target = "raid", delay = 1.0 },
--                 { command = "@shaman e 11932", target = "raid", delay = 1.0 },
--                 { command = "@shaman e 23200", target = "raid", delay = 1.0 },
--             }
--         },
--     },
-- },
--
-- {
--     name = "Tank Warrior",
--     collapsible = true,
--     buttons = {
--         {
--             label = "Keep Items",
--             commands = {
--                 { command = "keep 12952", target = "raid" },
--                 { command = "keep 13091", target = "raid" },
--                 { command = "keep 14552", target = "raid" },
--                 { command = "keep 18413", target = "raid" },
--                 { command = "keep 14624", target = "raid" },
--                 { command = "keep 12550", target = "raid" },
--                 { command = "keep 14525", target = "raid" },
--                 { command = "keep 14620", target = "raid", delay = 1.0 },
--                 { command = "keep 11927", target = "raid" },
--                 { command = "keep 14621", target = "raid" },
--                 { command = "keep 11669", target = "raid" },
--                 { command = "keep 22331", target = "raid" },
--                 { command = "keep 11810", target = "raid" },
--                 { command = "keep 10779", target = "raid" },
--                 { command = "keep 15806", target = "raid" },
--                 { command = "keep 12602", target = "raid" },
--                 { command = "keep 12651", target = "raid" },
--
--             }
--         },
--         {
--             label = "Give Items",
--             commands = {
--                 { command = ".additem 12952", target = "mt", delay = 1.0 },
--                 { command = ".additem 13091", target = "mt", delay = 1.0 },
--                 { command = ".additem 14552", target = "mt", delay = 1.0 },
--                 { command = ".additem 18413", target = "mt", delay = 1.0 },
--                 { command = ".additem 14624", target = "mt", delay = 1.0 },
--                 { command = ".additem 12550", target = "mt", delay = 1.0 },
--                 { command = ".additem 14525", target = "mt", delay = 1.0 },
--                 { command = ".additem 14620", target = "mt", delay = 1.0 },
--                 { command = ".additem 11927", target = "mt", delay = 1.0 },
--                 { command = ".additem 14621", target = "mt", delay = 1.0 },
--                 { command = ".additem 11669", target = "mt", delay = 1.0 },
--                 { command = ".additem 22331", target = "mt", delay = 1.0 },
--                 { command = ".additem 11810", target = "mt", delay = 1.0 },
--                 { command = ".additem 10779", target = "mt", delay = 1.0 },
--                 { command = ".additem 15806", target = "mt", delay = 1.0 },
--                 { command = ".additem 12602", target = "mt", delay = 1.0 },
--                 { command = ".additem 12651", target = "mt", delay = 1.0 },
--             }
--         },
--         {
--             label = "Equip Items",
--             commands = {
--                 { command = "e 12952", target = "raid", delay = 1.0 },
--                 { command = "e 13091", target = "raid", delay = 1.0 },
--                 { command = "e 14552", target = "raid", delay = 1.0 },
--                 { command = "e 18413", target = "raid", delay = 1.0 },
--                 { command = "e 14624", target = "raid", delay = 1.0 },
--                 { command = "e 12550", target = "raid", delay = 1.0 },
--                 { command = "e 14525", target = "raid", delay = 1.0 },
--                 { command = "e 14620", target = "raid", delay = 1.0 },
--                 { command = "e 11927", target = "raid", delay = 1.0 },
--                 { command = "e 14621", target = "raid", delay = 1.0 },
--                 { command = "e 11669", target = "raid", delay = 1.0 },
--                 { command = "e 22331", target = "raid", delay = 1.0 },
--                 { command = "e 11810", target = "raid", delay = 1.0 },
--                 { command = "e 10779", target = "raid", delay = 1.0 },
--                 { command = "e 15806", target = "raid", delay = 1.0 },
--                 { command = "e 12602", target = "raid", delay = 1.0 },
--                 { command = "e 12651", target = "raid", delay = 1.0 },
--             }
--         },
--     },
-- }
