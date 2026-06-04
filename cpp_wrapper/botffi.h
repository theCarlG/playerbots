/**
 * botffi.h — The complete extern "C" contract between CMaNGOS C++ and the Rust bot module.
 *
 * Rules:
 *   - No raw C++ pointers cross this boundary. Ever.
 *   - All CMaNGOS objects are referred to by their ObjectGuid value (uint64_t handle).
 *   - All game state passed to Rust is plain-data structs (no vtables, no destructors).
 *   - The C++ side resolves handles back to live pointers on every callback call.
 *   - Rust exports are the entry points CMaNGOS calls; BotCallbacks are what Rust calls back.
 */

#pragma once

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ── Opaque handles ──────────────────────────────────────────────────────── */

typedef uint64_t BotHandle;   /* ObjectGuid of a bot Player */
typedef uint64_t UnitHandle;  /* ObjectGuid of any Unit (Player, Creature) — 0 = none */

/* ── Plain-data structs (safe to copy across FFI) ────────────────────────── */

typedef struct {
    float    x, y, z, o;
    uint32_t map_id;
} BotPosition;

typedef struct {
    uint32_t health, max_health;
    uint32_t mana, max_mana;
    uint8_t  power_type;        /* POWER_MANA=0, POWER_RAGE=1, POWER_ENERGY=3, etc. */
    uint8_t  class_id;          /* CLASS_WARRIOR=1 ... CLASS_DRUID=11 */
    uint8_t  race_id;
    uint8_t  level;
    uint8_t  team;              /* 0=Alliance, 1=Horde */
    uint8_t  role;              /* 0=none, 1=tank, 2=heal, 4=dps (bitmask) */
    uint8_t  combo_points;      /* rogue/feral combo points on current combo target (0 otherwise) */
    uint8_t  shapeshift_form;   /* Unit::GetShapeshiftForm() — druid forms, warrior stances, DK ghoul, etc. 0=none. */
    BotPosition pos;
    bool     is_alive;
    bool     is_ghost;          /* Player has released spirit (ghost form) */
    bool     on_taxi;           /* Player is on a taxi/flight path */
    bool     in_combat;
    bool     is_casting;
    bool     is_moving;
    bool     is_channeling;
    uint32_t casting_spell_id;  /* 0 if not casting */
    float    casting_progress;  /* 0.0–1.0, only valid when is_casting */
    UnitHandle current_target;  /* 0 = no target */
    uint32_t  aura_state_mask;  /* CMaNGOS AuraState bitmask for quick checks */
    uint32_t  npc_entry;        /* Creature::GetEntry() for NPCs, 0 for players */
    bool      is_mounted;       /* Unit::IsMounted() */
} BotUnitSnapshot;

typedef struct {
    BotUnitSnapshot self;
    UnitHandle      group_members[40];  /* 0-terminated; includes self */
    uint8_t         group_size;         /* 0 = not in group */
    uint32_t        instance_id;        /* 0 = not in instance */
    uint32_t        zone_id;
    uint32_t        area_id;
    uint64_t        server_time_ms;     /* GetMSTime() at snapshot */
    bool            is_leader;          /* true if this bot is group/raid leader */

    /* ── Chat-filter state ──────────────────────────────────────────
     * These fields back the `CompositeChatFilter` port. They're cheap
     * to fill (just player getters) and the filter code on the Rust
     * side reads them synchronously from the snapshot, so it never has
     * to round-trip back into C++ to decide whether to drop a chat
     * command. */
    uint8_t  subgroup;                  /* 1..8 subgroup index, 0 = not in group */
    bool     is_raid_group;             /* bot is in a raid (vs. party or solo) */
    bool     in_guild;                  /* bot is a guild member */
    bool     is_guild_leader;           /* bot is the guild leader */
    uint32_t guild_id;                  /* 0 = no guild */
    uint8_t  durability_pct;            /* 0..100, lowest equipped-slot durability */
    uint8_t  bag_space_pct;             /* 0..100, percent of inventory slots *used* */
    uint32_t equip_gear_score;          /* average ilvl of equipped gear */
    bool     is_overworld;              /* true for maps 0/1/530/571, false inside instances */

    /* ── Rti chat filter state (3b) ─────────────────────────────────
     * ObjectGuid per raid target icon, indexed [0..7] matching PB2's
     * `RtiTargetValue::GetRtiIndex`: 0=star, 1=circle, 2=diamond,
     * 3=triangle, 4=moon, 5=square, 6=cross, 7=skull. Zero when the
     * slot is unset or the bot is not in a group.
     * Populated from `Group::GetTargetIcon(i)` (Groups/Group.h:292). */
    uint64_t group_raid_target_icons[8];

    /* ── Location chat filter state (3d) ────────────────────────────
     * Map name and top-level zone/area name, lowercased in C++ so the
     * Rust filter does byte-exact prefix matches without a per-tick
     * case fold. Buffers fit every known CMaNGOS map/area name with
     * room to spare; truncated with a trailing '\0' if ever too long.
     * Not escaped / not normalized — PB2 calls `std::tolower` and
     * `std::string::find` so the snapshot mirrors that byte-for-byte. */
    char map_name_lower[32];
    char area_name_lower[64];

    /* ── Guild chat filter state (3f) ───────────────────────────────
     * Full guild name and the bot's current rank name, NUL-terminated.
     * CMaNGOS limits are 24 chars (guild) and 16 chars (rank); buffers
     * are oversized for the NUL. Empty string when not in a guild. Not
     * lowercased — PB2 compares raw guild/rank names with
     * `std::string::find`. */
    char guild_name[32];
    char guild_rank_name[24];

    /* ── Quest chat filter state (3e) ───────────────────────────────
     * Active quest IDs in the bot's quest log. `current_quest_count`
     * is the populated prefix length; trailing slots are zero.
     * MAX_QUEST_LOG_SIZE is 25 in CMaNGOS (see Player.h); the buffer
     * is sized to that. Populated from
     * `Player::GetQuestSlotQuestId(slot)` — same loop PB2 uses in
     * `PlayerbotAI::GetAllCurrentQuestIds`. */
    uint32_t current_quest_ids[25];
    uint8_t  current_quest_count;
} BotWorldSnapshot;

typedef struct {
    uint32_t spell_id;
    uint32_t duration_ms;
    uint32_t max_duration_ms;
    uint8_t  stacks;
    bool     is_mine;           /* caster is this bot */
    bool     is_harmful;
    bool     is_passive;
} BotAuraInfo;

typedef struct {
    UnitHandle unit;
    float      threat;
    bool       is_online;       /* false = temporarily out of range/invisible */
    bool       is_taunted;
} BotThreatEntry;

typedef struct {
    float x, y, z;
    bool  found;                /* false = no reachable position found */
} BotSafePosition;

typedef struct {
    uint32_t quest_id;
    bool     complete;
} BotQuestInfo;

/* One overworld taxi node entry — a subset of `TaxiNodesEntry`. Returned in
 * batches by `get_overworld_taxi_nodes`. */
typedef struct {
    uint32_t index;    /* row index in `sTaxiNodesStore` */
    uint32_t map_id;   /* continent id: 0=EK, 1=Kalimdor, 530=Outland, 571=Northrend */
} BotTaxiNode;

/* One talent row entry — a subset of `TalentEntry`. `rank_ids[i]` is the
 * spell id the bot learns for rank `i`, or 0 if that rank is unused. */
typedef struct {
    uint32_t row;              /* talent row within its tab */
    uint32_t rank_ids[5];      /* MAX_TALENT_RANK spell IDs */
} BotTalentEntry;

/* One reputation entry — the bot's standing with a single faction. Returned
 * in batches by `bot_get_reputation_list`. `standing` is the enum-style tier
 * (0 = hated .. 7 = exalted). `value` is the raw standing points. */
typedef struct {
    uint32_t faction_id;
    int32_t  value;
    uint8_t  standing;
} BotReputationEntry;

/* One learned skill — id, current value, max value. Returned in batches by
 * `bot_get_learned_skills`. */
typedef struct {
    uint32_t skill_id;
    uint32_t value;
    uint32_t max;
} BotSkillEntry;

/* Aggregate snapshot of the bot's mailbox — returned by `bot_mail_summary`.
 * All counts include read and unread mail. `total_money` is the sum of
 * attached copper across all mails (not the bot's personal gold). */
typedef struct {
    uint32_t total_mails;
    uint32_t mails_with_money;
    uint32_t mails_with_items;
    uint32_t total_money;
} BotMailSummary;

/* One item from the bot's inventory, bank, equipment, or mailbox.
 * Returned in batches by the `bot_get_*` enumeration callbacks.
 * The Rust side formats the WoW item-link string from these fields. */
typedef struct {
    uint32_t item_id;
    uint32_t count;           /* stack count (1 for non-stackable) */
    uint32_t enchant_id;      /* permanent enchantment id, 0 if none */
    int32_t  random_property; /* GetItemRandomPropertyId(), 0 if none */
    uint32_t suffix_factor;   /* GetItemSuffixFactor(), 0 if none */
    uint8_t  quality;         /* 0=Poor .. 5=Legendary */
    uint8_t  soulbound;       /* 1 if the item is soulbound */
    uint32_t mail_index;      /* 1-based mail index; 0 for non-mail queries */
    char     name[80];        /* null-terminated item name */
} BotInventoryItem;

typedef struct {
    UnitHandle unit;
    uint32_t   spell_id;        /* debuff spell ID */
    bool       found;           /* false = no dispellable target */
} BotDispelTarget;

/* Travel destination returned by the travel FFI. Represents one location the
 * bot could travel to (vendor, repair NPC, quest giver, grind spot, etc.).
 * `entry` is a creature entry (positive) or GO entry (negative). */
typedef struct {
    int32_t  entry;      /* creature (>0) or GO (<0) entry, 0 = position-only */
    uint32_t quest_id;   /* associated quest id, 0 = none */
    uint32_t purpose;    /* TravelPurpose bitflags */
    uint32_t map_id;
    float    x, y, z;
} BotTravelDest;

/* Subset of CMaNGOS SpellEntry — just the fields the bot factory / AI needs.
 * Array sizes match the DBC constants: MAX_EFFECT_INDEX=3, MAX_SPELL_TOTEMS=2,
 * MAX_SPELL_REAGENTS=8. Fields are 0 when not applicable; `is_valid` is false
 * if the spell id is not in the spell store. */
typedef struct {
    uint32_t id;
    bool     is_valid;
    bool     is_passive;

    uint32_t attributes;
    uint32_t attributes_ex;
    uint32_t spell_level;
    uint32_t base_level;
    uint32_t max_level;
    uint32_t spell_family_name;

    /* Per-effect slots (3 entries). Effect=0 means "unused slot". */
    uint32_t effect[3];
    uint32_t effect_item_type[3];
    int32_t  effect_misc_value[3];
    uint32_t effect_apply_aura_name[3];

    /* Shaman totem item requirements (2 slots, 0 = none). */
    uint32_t totem[2];

    /* Spell reagents (8 slots, reagent[i] <= 0 means "unused"). */
    int32_t  reagent[8];
    uint32_t reagent_count[8];

    /* Equipment gating. equipped_item_class == -1 means "no requirement". */
    int32_t  equipped_item_class;
    int32_t  equipped_item_subclass_mask;
    int32_t  equipped_item_inventory_type_mask;
} BotSpellInfo;

/* ── Callback table (C++ → Rust query/command interface) ─────────────────── */

typedef struct BotCallbacks {
    /* ── Core state snapshot (call once at tick start) ────────────────── */
    BotWorldSnapshot (*get_snapshot)(BotHandle bot);
    BotUnitSnapshot  (*get_unit_snapshot)(BotHandle bot, UnitHandle target);

    /* ── Aura queries ────────────────────────────────────────────────── */
    bool         (*has_aura)(BotHandle bot, UnitHandle target, uint32_t spell_id);
    BotAuraInfo  (*get_aura)(BotHandle bot, UnitHandle target, uint32_t spell_id);
    BotAuraInfo* (*get_auras)(BotHandle bot, UnitHandle target, uint32_t* out_count);
    void         (*free_aura_list)(BotAuraInfo* list);

    /* ── Threat queries (CMaNGOS ThreatManager) ──────────────────────── */
    BotThreatEntry* (*get_threat_list)(BotHandle bot, UnitHandle target_unit, uint32_t* out_count);
    void            (*free_threat_list)(BotThreatEntry* list);
    float           (*get_unit_threat)(BotHandle bot, UnitHandle target_unit, UnitHandle from_unit);

    /* ── Unit queries ────────────────────────────────────────────────── */
    float       (*unit_distance)(BotHandle bot, UnitHandle target);
    bool        (*can_cast)(BotHandle bot, uint32_t spell_id, UnitHandle target);
    bool        (*spell_on_cooldown)(BotHandle bot, uint32_t spell_id);
    uint32_t    (*spell_cooldown_ms)(BotHandle bot, uint32_t spell_id);
    bool        (*has_los)(BotHandle bot, UnitHandle target);
    UnitHandle* (*get_nearby_units)(BotHandle bot, float range, bool hostile, uint32_t* out_count);
    /* Units that have this bot on their threat list (actual attackers).
     * Unlike get_nearby_units(hostile=true), this only returns mobs
     * currently fighting the bot — not every hostile in range. */
    UnitHandle* (*get_attackers)(BotHandle bot, uint32_t* out_count);
    void        (*free_unit_list)(UnitHandle* list);
    /* True if the bot is currently positioned in the rear arc of `target`
     * (gates abilities like Backstab that require being behind). */
    bool        (*bot_is_behind)(BotHandle bot, UnitHandle target);
    /* ItemPrototype.SubClass of the weapon in the given equipment slot
     * (0 = main hand, 1 = off hand, 2 = ranged), or UINT32_MAX when empty
     * or holding a non-weapon (e.g. shield). See ItemPrototype.h
     * ITEM_SUBCLASS_WEAPON_*. */
    uint32_t    (*bot_equipped_weapon_subclass)(BotHandle bot, uint8_t slot);
    /* Total count of `item_id` in the bot's inventory (bags + bank is NOT
     * included; mirrors Player::GetItemCount(id, false, nullptr)). Used for
     * warlock shards, mage food, spellstones, etc. */
    uint32_t    (*bot_item_count)(BotHandle bot, uint32_t item_id);
    /* Bitmask of currently active totem slots (shaman). Bit 0 = fire,
     * 1 = earth, 2 = water, 3 = air. 0 on non-shamans. */
    uint8_t     (*bot_active_totem_mask)(BotHandle bot);
    /* True if the weapon in `slot` (0=mainhand, 1=offhand) has a temporary
     * enchant applied (Item::GetEnchantmentId(TEMP_ENCHANTMENT_SLOT) != 0).
     * Shaman weapon buffs, rogue poisons, warrior sharpening stones. */
    bool        (*bot_weapon_enchanted)(BotHandle bot, uint8_t slot);
    /* Bitmask of ready death-knight runes (WotLK only; bits 0–5 for the
     * six rune slots). Returns 0 on Classic/TBC. */
    uint8_t     (*bot_runes_ready_mask)(BotHandle bot);
    /* True if the bot has learned `spell_id` (Player::HasSpell). Used by
     * BT KnowsSpell nodes to gate talented or optional abilities like
     * Hemorrhage, Stormstrike, Metamorphosis, etc. Does NOT check cooldown
     * or power cost — use `can_cast` for full castability. */
    bool        (*bot_knows_spell)(BotHandle bot, uint32_t spell_id);

    /* Return the spell name for a given spell_id. Writes up to buf_len-1
     * characters into buf and null-terminates. Returns the number of
     * characters written (excluding NUL), or 0 if the spell doesn't exist.
     * Used by the debug monitor to log human-readable spell names. */
    uint32_t    (*get_spell_name)(uint32_t spell_id, char* buf, uint32_t buf_len);

    /* ── Pathfinding / positioning ───────────────────────────────────── */
    BotPosition     (*get_behind_position)(BotHandle bot, UnitHandle target, float distance);
    BotSafePosition (*get_safe_position)(BotHandle bot, float search_radius);
    BotPosition     (*get_spread_position)(BotHandle bot, UnitHandle center, float radius,
                                           uint8_t idx, uint8_t total);
    bool            (*can_reach)(BotHandle bot, float x, float y, float z);
    /* Returns true if the bot can reach (x,y,z) via navmesh pathfinding
     * (not just line-of-sight). Uses PathFinder + PATHFIND_NORMAL check. */
    bool            (*can_pathfind_to)(BotHandle bot, float x, float y, float z);

    /* ── Bot commands (all return true on success) ───────────────────── */
    bool (*cast_spell)(BotHandle bot, uint32_t spell_id, UnitHandle target);
    bool (*cast_spell_pos)(BotHandle bot, uint32_t spell_id, float x, float y, float z);
    bool (*move_to)(BotHandle bot, float x, float y, float z);
    bool (*follow)(BotHandle bot, UnitHandle target, float dist, float angle);
    /* Chase a unit in combat — uses MoveChase, which smoothly tracks
     * the target at the given distance and angle without restarting
     * splines every tick (unlike repeated move_to calls). */
    bool (*chase)(BotHandle bot, UnitHandle target, float dist, float angle);
    bool (*stop_moving)(BotHandle bot);
    /* Set the bot's facing angle (radians, 0 = north, clockwise).
     * Stops movement and sends a heartbeat so the server acknowledges
     * the new orientation immediately. Used to face away from a target
     * before casting Blink or other directional abilities. */
    void (*set_facing)(BotHandle bot, float angle);
    bool (*attack)(BotHandle bot, UnitHandle target);
    bool (*auto_attack)(BotHandle bot, bool enable);
    /* Ranged auto-attack / wand-shoot pull. Inspects the bot's ranged slot
     * and fires the appropriate spell (Auto Shot 75 for bow/gun/crossbow,
     * Shoot 5019 for wand) at `target`. Returns false if no ranged weapon
     * is equipped, the bot doesn't know / can't cast the corresponding
     * spell, or the cast fails. Used by the generic `PullTarget` BT leaf
     * as the "shoot pull" branch so cross-class strategies don't need to
     * know which class is driving them. */
    bool (*auto_shoot)(BotHandle bot, UnitHandle target);
    bool (*say)(BotHandle bot, const char* msg, uint32_t lang);
    bool (*whisper)(BotHandle bot, uint64_t target_guid, const char* msg);
    /* PB2-style TellPlayerNoFacing routing: broadcasts to the bot's party/raid
     * when it has a group, otherwise whispers the sender. target_guid is the
     * requester's ObjectGuid (used only for the whisper fallback). */
    bool (*tell_player)(BotHandle bot, uint64_t target_guid, const char* msg);
    bool (*use_item)(BotHandle bot, uint32_t item_id, UnitHandle target);
    /* Find the best food or drink in the bot's bags for its level.
     * category: 11 = food (HP regen), 59 = drink (mana regen).
     * Returns the item_id, or 0 if nothing suitable found. */
    uint32_t (*find_food_drink_in_bags)(BotHandle bot, uint32_t category);
    bool (*taunt)(BotHandle bot, UnitHandle target);
    /* Teleport the bot to (`map_id`, x, y, z, o). When `map_id` matches the
     * bot's current map, wraps `Player::NearTeleportTo`; otherwise wraps
     * `Player::TeleportTo`. Returns false when the bot is dead/offline or
     * the target map cannot be loaded. Used by the `summon` command. */
    bool (*teleport_to)(BotHandle bot, uint32_t map_id, float x, float y, float z, float o);
    /* Resolve a player `ObjectGuid` to their current world position. When the
     * player is online, writes the position into `*out_pos` and returns true;
     * otherwise leaves `*out_pos` untouched and returns false. Used by the
     * `summon` command to find the requester. */
    bool (*get_player_position)(BotHandle bot, uint64_t player_guid, BotPosition* out_pos);
    /* Full-fat summon: mirrors PB2 `SummonAction::Teleport` exactly.
     *   1. Validates that `requester` is online and not being teleported.
     *   2. Iterates angles around the requester's follow offset to pick a
     *      LOS-clear position and snaps Z to ground.
     *   3. If the bot is dead, revives at 100% HP and spawns corpse bones.
     *   4. Interrupts taxi flight, clears the motion master, and teleports.
     *   5. Re-parents to the requester's transport if one is present.
     *   6. Updates any active `stay` / `guard` position entries so the bot
     *      holds its new spot after the summon.
     * Returns true if the bot was successfully moved to a valid position. */
    bool (*summon_to_player)(BotHandle bot, uint64_t requester_guid);

    /* ── Group / raid queries ────────────────────────────────────────── */
    UnitHandle (*group_get_tank)(BotHandle bot);
    UnitHandle (*group_get_healer)(BotHandle bot);
    uint8_t    (*group_get_role)(BotHandle bot, UnitHandle member);
    /* Find the nearest hostile unit marked with raid target icon 1..8.
     * Returns 0 if no such unit exists. */
    UnitHandle (*get_unit_with_raid_icon)(BotHandle bot, uint8_t icon);
    /* Assign raid target icon `icon` (0..7 — star/circle/diamond/triangle/
     * moon/square/cross/skull) to `target`. Wraps `Group::SetTargetIcon`,
     * which broadcasts MSG_RAID_TARGET_UPDATE to the whole group and also
     * clears the icon from any other unit currently wearing it. No-ops and
     * returns false when the bot isn't grouped (solo `SetTargetIcon` is
     * not a thing on this server) or the icon index is out of range.
     * Passing `target = 0` clears the icon. Used by the 11g
     * `MarkRti`/`MarkRtiCc` BT leaves. */
    bool (*group_set_target_icon)(BotHandle bot, UnitHandle target, uint8_t icon);

    /* ── Death / resurrection ────────────────────────────────────────── */
    bool        (*accept_resurrect)(BotHandle bot);
    BotPosition (*get_corpse_position)(BotHandle bot);  /* returns {0,0,0} if N/A */
    bool        (*use_spirit_healer)(BotHandle bot);
    /* In-place self-resurrection used by the `summon` command to mirror
     * PB2's `SummonAction::Teleport` behavior (revive dead bot at full HP,
     * spawn corpse bones, then teleport). Returns false if the bot is
     * already alive or cannot be safely revived. */
    bool        (*resurrect_self)(BotHandle bot);

    /* ── Mount ───────────────────────────────────────────────────────── */
    bool (*is_mounted)(BotHandle bot);
    bool (*mount_up)(BotHandle bot);
    bool (*dismount)(BotHandle bot);
    bool (*is_indoor)(BotHandle bot);

    /* ── Loot ────────────────────────────────────────────────────────── */
    UnitHandle* (*get_nearby_lootable)(BotHandle bot, float range, uint32_t* out_count);
    bool        (*open_loot)(BotHandle bot, UnitHandle target);
    bool        (*take_all_loot)(BotHandle bot);

    /* ── Loot rolling ───────────────────────────────────────────────── */
    /* Get the count of pending loot rolls the bot hasn't voted on yet.
     * Returns 0 if not in a group or no active rolls. */
    uint32_t    (*get_pending_roll_count)(BotHandle bot);
    /* Auto-roll on the next pending item. Applies the same logic as PB2:
     * need for equippable upgrades, greed for tradeable/usable, pass
     * otherwise. Returns true if a vote was cast. */
    bool        (*auto_loot_roll)(BotHandle bot);
    /* Cast a specific roll vote (0=pass, 1=need, 2=greed) on the next
     * pending item. Returns true if a vote was cast. */
    bool        (*cast_loot_roll)(BotHandle bot, uint8_t vote);

    /* ── NPC interaction ─────────────────────────────────────────────── */
    UnitHandle* (*get_nearby_npcs)(BotHandle bot, float range, uint32_t npc_flags,
                                   uint32_t* out_count);
    bool        (*interact_npc)(BotHandle bot, UnitHandle npc);
    bool        (*repair_all)(BotHandle bot);
    bool        (*sell_grey_items)(BotHandle bot);
    bool        (*has_sellable_items)(BotHandle bot);
    float       (*get_durability_pct)(BotHandle bot);

    /* ── Quest ───────────────────────────────────────────────────────── */
    BotQuestInfo* (*get_quest_log)(BotHandle bot, uint32_t* out_count);
    void          (*free_quest_log)(BotQuestInfo* list);
    bool          (*accept_all_quests)(BotHandle bot, UnitHandle npc);
    bool          (*turn_in_quest)(BotHandle bot, UnitHandle npc, uint32_t quest_id);

    /* ── Unit queries (extended) ─────────────────────────────────────── */
    bool    (*is_attackable)(BotHandle bot, UnitHandle target);
    uint8_t (*get_unit_level)(BotHandle bot, UnitHandle target);
    bool    (*is_casting_interruptible)(BotHandle bot, UnitHandle target);
    /* Coarse unit category used by BT condition leaves that need to
     * distinguish players/pets/critters from regular creatures (e.g.
     * pull-target eligibility, CC vocabulary, do-not-attack-critter
     * gates). Return values:
     *   0 = other / unknown (regular creature, GO, empty handle, …)
     *   1 = player (ObjectGuid::IsPlayer)
     *   2 = pet    (ObjectGuid::IsPet)
     *   3 = critter (Creature::IsCritter — requires resolving the unit)
     * Cheaper than `get_unit_snapshot` since players/pets can be
     * decided from the guid high bits alone; only the critter branch
     * resolves the live Creature*. */
    uint8_t (*unit_kind)(BotHandle bot, UnitHandle target);

    /* ── Pet management ──────────────────────────────────────────────── */
    bool    (*has_pet)(BotHandle bot);
    bool    (*pet_is_alive)(BotHandle bot);
    uint8_t (*pet_happiness)(BotHandle bot);
    uint8_t (*pet_health_pct)(BotHandle bot); /* 0..100, pet HP%, 0 if no pet or dead */
    bool    (*summon_pet)(BotHandle bot);
    bool    (*revive_pet)(BotHandle bot);
    bool    (*feed_pet)(BotHandle bot);

    /* ── Dispel / party queries ──────────────────────────────────────── */
    /* Find a nearby group member (or self when solo) with a dispellable
     * debuff that this bot can remove. `dispel_mask` is a bitmask over
     * DispelType values — bit (1 << DISPEL_MAGIC), (1 << DISPEL_CURSE),
     * (1 << DISPEL_DISEASE), (1 << DISPEL_POISON). Passing `0` is
     * interpreted as "any school the bot can dispel". */
    BotDispelTarget (*find_dispellable_target)(BotHandle bot, uint8_t dispel_mask);
    UnitHandle      (*find_dead_party_member)(BotHandle bot);

    /* ── Consumables: potion query ───────────────────────────────────── */
    /* Return the item id of the first usable potion the bot carries in
     * bags matching `category`, or 0 if none. Categories:
     *   0 = buff potion (temporary stat/damage increase — e.g. Elixir of
     *       the Mongoose, Greater Arcane Elixir).
     *   1 = utility potion (free action, invulnerability, swiftness,
     *       living action — consumables that break out of effects).
     * Healing/mana potions use existing `factory_pick_potion_for_level`
     * selection paths and are not covered here. */
    uint32_t        (*find_potion_in_bags)(BotHandle bot, uint8_t category);
    /* True if the bot's shared potion item cooldown (category 4) is
     * ready. Mirrors `Player::HasItemCooldown` on a representative
     * potion — used as a cheap gate before `UseBuffPotion` leaves
     * enter the cast path. */
    bool            (*potion_cooldown_ready)(BotHandle bot);

    /* ── Consumables: trinket activation (11h) ───────────────────────── */
    /* Activate the trinket currently equipped in the requested trinket
     * slot. `slot` is a logical index: 0 = EQUIPMENT_SLOT_TRINKET1,
     * 1 = EQUIPMENT_SLOT_TRINKET2. Looks up the item in that slot,
     * reads its first OnUse spell, gates on `Player::HasSpellCooldown`
     * + `Player::IsSpellReady`, and fires the spell directly (same path
     * as `CB_UseItem`). Returns false when the slot is empty, the item
     * has no OnUse effect, the item is on cooldown, or the cast fails.
     * Used by the `Bt::UseTrinket(slot)` BT leaf. */
    bool            (*use_trinket)(BotHandle bot, uint8_t slot);

    /* ── Social / group actions (11i) ───────────────────────────────── */
    /* Accept a pending group/raid invitation. Returns false if the bot
     * has no pending invite. */
    bool (*accept_group_invite)(BotHandle bot);
    /* Make the bot leave its current group/raid. Returns false when the
     * bot is not grouped. */
    bool (*leave_group)(BotHandle bot);
    /* Accept a pending ready check. Returns false if no ready check is
     * active. */
    bool (*accept_ready_check)(BotHandle bot);
    /* Accept a pending trade window. Returns false if no trade is
     * pending. */
    bool (*accept_trade)(BotHandle bot);
    /* Accept an incoming duel request. Returns false if duel_state != 1
     * or the accept fails. */
    bool (*accept_duel)(BotHandle bot);
    /* Decline an incoming duel request. Returns false if duel_state != 1
     * or the decline fails. */
    bool (*decline_duel)(BotHandle bot);
    /* Accept a pending summon (warlock ritual, meeting stone). Returns
     * false if no summon is pending. */
    bool (*accept_summon)(BotHandle bot);
    /* Interact with the nearest meeting stone within 20y and queue the
     * bot for summoning. Returns false when no stone is in range. */
    bool (*use_meeting_stone)(BotHandle bot);

    /* ── PvP / duel / faction queries (11d) ──────────────────────────── */
    /* `Player::IsPvP()` — true when the bot currently carries the PvP
     * flag (either through a PvP zone or a manual toggle). */
    bool    (*is_pvp_flagged)(BotHandle bot);
    /* Encoded state of `Player::duel`:
     *   0 = no active duel at all,
     *   1 = challenged / countdown (a request is pending — the bot
     *       hasn't started fighting yet, regardless of whether it
     *       sent or received the request),
     *   2 = in progress (bot is currently dueling).
     * `3` (completed) is collapsed into `0` since strategies don't
     * care about the post-duel cleanup window. */
    uint8_t (*duel_state)(BotHandle bot);
    /* The bot's reputation rank with `faction_id`, encoded as the
     * server's `ReputationRank` (0=hated .. 7=exalted). Returns `3`
     * (neutral) when the bot has no record for the faction — this
     * matches how the client displays unfilled factions and lets
     * `RepWithFactionBelow` treat "never touched" as "has not yet
     * earned any standing". Returns `255` when the faction id does
     * not exist in the DBC. */
    uint8_t (*reputation_rank)(BotHandle bot, uint32_t faction_id);

    /* ── Battleground ────────────────────────────────────────────────── */
    bool           (*is_in_battleground)(BotHandle bot);
    uint8_t        (*battleground_type)(BotHandle bot);  /* 1=AV, 2=WSG, 3=AB, 0=none */
    BotSafePosition (*get_bg_objective)(BotHandle bot);  /* .found=false if none */
    bool           (*capture_bg_objective)(BotHandle bot);
    UnitHandle*    (*get_nearby_enemies)(BotHandle bot, float range, uint32_t* out_count);

    /* ── RPG / social ────────────────────────────────────────────────── */
    BotSafePosition (*get_random_point_nearby)(BotHandle bot, float range);
    bool            (*emote)(BotHandle bot, uint32_t emote_id);
    UnitHandle*     (*get_nearby_gossip_npcs)(BotHandle bot, float range, uint32_t* out_count);

    /* ── Gathering (mining, herbalism, skinning) ─────────────────────── */
    bool        (*has_gathering_skill)(BotHandle bot);
    uint64_t*   (*get_nearby_gatherables)(BotHandle bot, float range, uint32_t* out_count);
    void        (*free_gatherable_list)(uint64_t* list);
    bool        (*gather_node)(BotHandle bot, uint64_t handle);
    float       (*gameobject_distance)(BotHandle bot, uint64_t handle);
    BotPosition (*gameobject_position)(BotHandle bot, uint64_t handle);

    /* ── Generic game-object interaction (encounter/instance scripting) ───
     * Find the nearest spawned GameObject with entry `entry` within `range`
     * yards. Returns the packed GUID, or 0 if none. Used by instance FSMs
     * for mechanics like BWL suppression devices and MC rune dousing. */
    uint64_t    (*nearby_gameobject_by_entry)(BotHandle bot, uint32_t entry, float range);
    /* Invoke `GameObject::Use(Player*)` on the GO identified by `handle`.
     * Returns false if the handle no longer resolves. */
    bool        (*use_gameobject)(BotHandle bot, uint64_t handle);

    /* ── Factory: inventory mutation ─────────────────────────────────── */
    /* Destroy every equipped item plus every item in backpack and carried bags.
     * Leaves bank contents intact. */
    void     (*inventory_destroy_equipped_and_bags)(BotHandle bot);
    /* Destroy every item the bot owns (equipped, bags, bank). */
    void     (*inventory_destroy_all)(BotHandle bot);
    /* Return the number of `item_id` in backpack and carried bags (excludes bank). */
    uint32_t (*item_count_in_bags)(BotHandle bot, uint32_t item_id);

    /* Add `count` of `item_id` to the bot's bags (via StoreNewItemInInventorySlot).
     * Returns the number actually added — may be 0 if bags are full. */
    uint32_t (*inventory_add_item)(BotHandle bot, uint32_t item_id, uint32_t count);
    /* Max stack size of the item prototype (1 if unknown). */
    uint32_t (*item_max_stack_size)(BotHandle bot, uint32_t item_id);

    /* ── Factory: consumable selection (wraps RandomItemMgr) ─────────── */
    /* effect: SPELL_EFFECT_HEAL=10, SPELL_EFFECT_ENERGIZE=30. Returns 0 if none. */
    uint32_t (*factory_pick_potion_for_level)(BotHandle bot, uint32_t level, uint32_t effect);
    /* category: 11 = food, 59 = drink. Returns 0 if none. */
    uint32_t (*factory_pick_food_for_level)(BotHandle bot, uint32_t level, uint32_t category);

    /* ── RNG ─────────────────────────────────────────────────────────── */
    /* Uniform random integer in [min, max] (inclusive). Wraps CMaNGOS urand. */
    uint32_t (*random_u32)(BotHandle bot, uint32_t min, uint32_t max);

    /* ── Factory: progression wipe ───────────────────────────────────── */
    /* Set a skill to zero (effectively removing it). Used by ClearSkills. */
    void (*bot_clear_skill)(BotHandle bot, uint32_t skill_id);
    /* Reset all learned spells via Player::resetSpells() (CMaNGOS). */
    void (*bot_reset_spells)(BotHandle bot);
    /* Wipe every quest from the quest log + character_queststatus DB row. */
    void (*bot_reset_all_quests)(BotHandle bot);

    /* ── Factory: misc pre/post init ─────────────────────────────────── */
    /* Remove every aura (buffs & debuffs) currently on the bot. */
    void (*bot_remove_all_auras)(BotHandle bot);
    /* Remove all aura stacks of a specific spell from the bot. */
    void (*bot_remove_aura_by_id)(BotHandle bot, uint32_t spell_id);
    /* True if the bot has `skill_id` learned at any rank. */
    bool (*bot_has_skill)(BotHandle bot, uint32_t skill_id);
    /* Teach the bot a spell (Player::learnSpell with dependent=false).
     * Used by the factory mount / spell initialization steps. */
    void (*bot_learn_spell)(BotHandle bot, uint32_t spell_id);
    /* Remove a learned spell (Player::removeSpell). Used by RTSC `rtsc reset`
     * to unlearn Aedm (spell 30758). Mirrors PB2 RtscAction.cpp:33. */
    void (*bot_remove_spell)(BotHandle bot, uint32_t spell_id);
    /* Wraps `Player::learnDefaultSpells()` — teaches the race/class starter
     * spell set from `playercreateinfo_spell_custom`. Used by the factory
     * InitAvailableSpells step. */
    void (*bot_learn_default_spells)(BotHandle bot);
    /* Wraps `Player::learnClassLevelSpells(include_high_level_quest_rewards)`
     * — teaches every class spell the bot qualifies for at its current
     * level. Used by the factory InitAvailableSpells step. */
    void (*bot_learn_class_level_spells)(BotHandle bot, bool include_quest_rewards);

    /* ── Spell store queries (wraps sSpellTemplate) ──────────────────── */
    /* Look up a subset of SpellEntry fields for `spell_id`. The returned
     * struct has `is_valid=false` when the id is not in the spell store. */
    BotSpellInfo (*get_spell_info)(BotHandle bot, uint32_t spell_id);
    /* List the bot's currently-known (non-removed, non-disabled) spell IDs.
     * Returns a freshly-allocated array; caller must call `free_bot_spells`.
     * `*out_count` receives the number of entries (0 on empty). */
    uint32_t* (*get_bot_spells)(BotHandle bot, uint32_t* out_count);
    void      (*free_bot_spells)(uint32_t* list);

    /* ── Factory: bag slot management ────────────────────────────────── */
    /* Return the number of empty equipped bag slots
     * (INVENTORY_SLOT_BAG_START .. INVENTORY_SLOT_BAG_END). Range 0..=4. */
    uint32_t (*bot_empty_bag_slot_count)(BotHandle bot);
    /* Store `count` of `item_id` using Player::StoreNewItemInBestSlots,
     * which will auto-equip bags into empty bag slots. Returns true on
     * success. Used by factory InitBags. */
    bool     (*bot_store_new_in_best_slots)(BotHandle bot, uint32_t item_id, uint32_t count);

    /* Set the bot's reputation with `faction_id` to `value` standing points.
     * Internally looks up the FactionEntry, checks HasReputation(), and calls
     * ReputationMgr::SetReputation. Returns true when the faction exists and
     * carries reputation, false otherwise. Used by factory InitReputations. */
    bool     (*bot_set_reputation)(BotHandle bot, uint32_t faction_id, int32_t value);

    /* ── Factory: ammo management ────────────────────────────────────── */
    /* Weapon SubClass of the item in EQUIPMENT_SLOT_RANGED, or UINT32_MAX
     * when no ranged item is equipped. See ItemPrototype.h subclass enums. */
    uint32_t (*bot_equipped_ranged_subclass)(BotHandle bot);
    /* Current PLAYER_AMMO_ID field (item entry of the ammo being used), or 0. */
    uint32_t (*bot_current_ammo_id)(BotHandle bot);
    /* Wraps sRandomItemMgr.GetAmmo(level, ammo_subclass). Returns an item id
     * or 0. `ammo_subclass` is ITEM_SUBCLASS_ARROW/BULLET/THROWN. */
    uint32_t (*factory_pick_ammo_for_level)(BotHandle bot, uint32_t level, uint32_t ammo_subclass);
    /* Calls Player::SetAmmo(item_id). */
    void     (*bot_set_ammo)(BotHandle bot, uint32_t item_id);

    /* ── Factory: skills ─────────────────────────────────────────────── */
    /* Current skill value for `skill_id` (0 if the skill is not known). */
    uint32_t (*bot_get_skill_value)(BotHandle bot, uint32_t skill_id);
    /* Player::SetSkill(skill_id, value, max). Grants the skill if not yet
     * known. `step` passes 0 (let the game derive it). */
    void     (*bot_set_skill)(BotHandle bot, uint32_t skill_id, uint32_t value, uint32_t max);
    /* Player::UpdateSkillsForLevel(true). */
    void     (*bot_update_skills_for_level)(BotHandle bot);

    /* ── Factory: refresh (cheat mask + DB save) ─────────────────────── */
    /* Bitmask of cheats currently enabled on this bot (the BotCheatMask
     * enum parsed from `AiPlayerbot.BotCheats`). Bit 5 = item cheat; the
     * Rust factory `refresh` path gates consumable top-ups on this. Returns
     * 0 when no cheats are set. */
    uint32_t (*bot_cheat_mask)(BotHandle bot);
    /* Player::SaveToDB() guarded by
     * `sRandomPlayerbotMgr.GetDatabaseDelay("CharacterDatabase") < 10ms`
     * (mirrors the tail of `PlayerbotFactory::Refresh`). No-op when the
     * CharacterDatabase worker queue is busy. */
    void     (*bot_save_to_db_if_not_busy)(BotHandle bot);

    /* ── Factory: prepare ────────────────────────────────────────────── */
    /* Player::ResurrectPlayer(1.0f, false) followed by
     * Player::SpawnCorpseBones(). Brings a dead bot back to life in one
     * atomic FFI call — the C++ side of `PlayerbotFactory::Prepare` always
     * does both together. */
    void     (*bot_resurrect_full)(BotHandle bot);
    /* Player::CombatStop(true) — drops target, clears threat, stops any
     * in-progress cast. Called by `Prepare` before reshaping the bot. */
    void     (*bot_combat_stop)(BotHandle bot);
    /* Set level to `level`, reset current XP to 0, and set next-level XP
     * to `sObjectMgr.GetXPForLevel(level)`. Atomic wrapper around the
     * three C++ field writes in `PlayerbotFactory::Prepare`. */
    void     (*bot_set_level_and_reset_xp)(BotHandle bot, uint32_t level);
    /* Set (set=true) or clear (set=false) a bit in PLAYER_FLAGS via
     * Player::SetFlag / Player::RemoveFlag. Used by `Prepare` for the
     * helmet / cloak display flags; generalised so future factory steps
     * can reuse it. */
    void     (*bot_set_player_flag)(BotHandle bot, uint32_t flag, bool set);
    /* sPlayerbotAIConfig.disableRandomLevels — when true, factory runs
     * skip the level / XP reset in `Prepare` and bail out of `Randomize`. */
    bool     (*factory_config_disable_random_levels)(BotHandle bot);
    /* sPlayerbotAIConfig.randomBotShowHelmet — when false, bots get the
     * PLAYER_FLAGS_HIDE_HELM flag set during `Prepare`. */
    bool     (*factory_config_random_bot_show_helmet)(BotHandle bot);
    /* sPlayerbotAIConfig.randomBotShowCloak — when false, bots get the
     * PLAYER_FLAGS_HIDE_CLOAK flag set during `Prepare`. */
    bool     (*factory_config_random_bot_show_cloak)(BotHandle bot);

    /* ── Factory: Randomize orchestration ────────────────────────────── */
    /* sRandomPlayerbotMgr.IsRandomBot(bot) — is the bot currently tracked
     * in the random-bot account list? `PlayerbotFactory::Randomize` uses
     * this to decide whether to run the full re-roll pipeline. */
    bool     (*factory_is_random_bot)(BotHandle bot);
    /* ai->HasRealPlayerMaster() — does the bot have a live human master?
     * Part of the "claimed random bot" gate in `Randomize`. */
    bool     (*factory_has_real_player_master)(BotHandle bot);
    /* ai->IsInRealGuild() — is the bot in a guild with at least one real
     * player? Part of the "claimed random bot" gate in `Randomize`. */
    bool     (*factory_is_in_real_guild)(BotHandle bot);
    /* sPlayerbotAIConfig.minEnchantingBotLevel — the cut-off level below
     * which `Randomize` skips `LoadEnchantContainer`. */
    uint32_t (*factory_config_min_enchanting_bot_level)(BotHandle bot);
    /* PlayerbotFactory::LoadEnchantContainer — pull per-bot enchant
     * templates out of the world DB. Called from `Randomize` after the
     * min-enchanting-level gate passes. */
    void     (*factory_load_enchant_container)(BotHandle bot);
    /* bot->resetTalents(true) — wipe every learned talent. Called by
     * `Randomize` in the non-incremental random-bot branch. */
    void     (*bot_reset_talents)(BotHandle bot);
    /* bot->learnQuestRewardedSpells() — fold every quest-reward spell the
     * bot qualifies for into its spellbook in one pass. Called by
     * `Randomize` after rewarding the "special quest list" on a real
     * random bot. */
    void     (*bot_learn_quest_rewarded_spells)(BotHandle bot);
    /* bot->GetMoney() — current copper balance. */
    uint32_t (*bot_get_money)(BotHandle bot);
    /* bot->SetMoney(amount) — set the bot's copper balance. */
    void     (*bot_set_money)(BotHandle bot, uint32_t amount);
    /* PlayerbotFactory::InitGems — socket gem fan-out for TBC / WotLK.
     * Ports the whole per-slot CMSG_SOCKET_GEMS construction loop in one
     * atomic callback. No-op on Classic bridges. */
    void     (*factory_init_all_gems)(BotHandle bot);
    /* PlayerbotFactory::EnchantEquipment — per-slot enchant template
     * dispatch. Atomic "for every equipped item, look up an enchant
     * template and call EnchantItem on it" loop. */
    void     (*factory_enchant_all_equipment)(BotHandle bot);

    /* ── Factory: quests ─────────────────────────────────────────────── */
    /* Combined class/race/min-level eligibility filter from
     * `PlayerbotFactory::InitQuests`. C++ side looks up the quest template,
     * so Rust only needs the id. Returns false when the quest is unknown or
     * any of the three PB2 predicates reject it. */
    bool     (*quest_is_eligible_for_bot)(BotHandle bot, uint32_t quest_id);
    /* Set quest status to complete and hand the bot the reward. Wraps
     * `Player::SetQuestStatus(id, QUEST_STATUS_COMPLETE)` and
     * `Player::RewardQuest(q, 0, bot, false)`. Silent no-op on unknown ids. */
    void     (*bot_reward_quest_complete)(BotHandle bot, uint32_t quest_id);

    /* ── Factory: arena team ─────────────────────────────────────────── */
    /* `bot->GetSession()->GetAccountId()` — account that owns this bot
     * character. Used by `PlayerbotFactory::InitArenaTeam` to gate random-
     * bot factory work against the live random-bot account list. Returns
     * 0 when the session is unavailable. */
    uint32_t (*bot_get_account_id)(BotHandle bot);

    /* ── Factory: guild ─────────────────────────────────────────────────
     * These back `PlayerbotFactory::InitGuild`. The legacy C++ body reads
     * `sPlayerbotAIConfig.randomBotGuilds`, filters to guilds whose leader
     * is on the same team as the bot, picks one at random, and joins it as
     * a random rank between GR_OFFICER and GR_INITIATE. The runtime guild
     * list itself lives in the Rust `random` factory singleton; the
     * callbacks here cover the per-guild lookups and the join action. */

    /* Read-only guild summary: populates `out` with the leader team
     * (0=Alliance, 1=Horde), current member count, the `atoi(GetGINFO())`
     * max-member hint (0 when the GINFO string is not numeric), the guild
     * name, and the "bot is already a member" flag. Returns false when
     * the guild id is unknown. */
    bool     (*factory_query_guild_summary)(BotHandle bot,
                                            uint32_t guild_id,
                                            uint8_t* out_leader_team,
                                            uint32_t* out_member_size,
                                            uint32_t* out_max_members_hint,
                                            char* out_name,
                                            uint32_t name_buf_len);

    /* `guild->AddMember(bot->GetObjectGuid(), rank_id)` — join the bot
     * to an existing guild at the given rank. Returns false when the
     * guild id is unknown or the add failed (bot already in a guild,
     * guild full, etc.). */
    bool     (*factory_guild_add_member)(BotHandle bot,
                                         uint32_t guild_id,
                                         uint32_t rank_id);

    /* `guild->GetRankName(rank_id)` — write the rank name into
     * `out_name` for log output. Returns false when the guild id is
     * unknown or `rank_id` is out of range (no write on failure). */
    bool     (*factory_get_guild_rank_name)(BotHandle bot,
                                            uint32_t guild_id,
                                            uint32_t rank_id,
                                            char* out_name,
                                            uint32_t name_buf_len);

    /* `bot->GetGuildId()` — the bot's currently-joined guild. Returns 0
     * when the bot is not in a guild. Used by `InitGuild` to skip the
     * filter/pick pass when the bot already belongs to one. */
    uint32_t (*factory_bot_guild_id)(BotHandle bot);

    /* ── Factory: per-bot key/value store ────────────────────────────── */
    /* `sRandomPlayerbotMgr.GetValue(bot, key)` / `SetValue(bot, key, value)` —
     * read/write a per-bot scalar in the random_bot_event table. Used by
     * `InitTradeSkills` to cache the two professions assigned to a bot so
     * subsequent re-rolls hand out the same pair. Unknown keys return 0. */
    uint32_t (*factory_kv_get_u32)(BotHandle bot, const char* key);
    void     (*factory_kv_set_u32)(BotHandle bot, const char* key, uint32_t value);

    /* Trainer-iteration tail of `PlayerbotFactory::InitTradeSkills` —
     * walk every creature template, look at its trainer spell list, and
     * `bot->learnSpell` any recipe the bot qualifies for under the
     * "Apprentice + chosen profession or secondary" gate. Delegated to
     * C++ because the loop touches sCreatureStorage + sSpellTemplate +
     * m_trainerSpells. */
    void     (*factory_learn_tradeskill_recipes)(BotHandle bot);

    /* ── Factory: item prototype queries ─────────────────────────────── */
    /* ItemPrototype.Quality (0..7 — poor/common/uncommon/rare/epic/...). */
    uint32_t (*item_prototype_quality)(BotHandle bot, uint32_t item_id);

    /* ── Factory: equipment ──────────────────────────────────────────── */
    /* `bot->GetGUIDLow()` — the low 32 bits of the bot's object guid, used
     * as the per-player cache key by the Rust itempool. Returns 0 when the
     * bot handle is unavailable. */
    uint32_t (*factory_bot_guid_low)(BotHandle bot);

    /* Item id currently equipped in `slot` (see EquipmentSlots / 0..=18),
     * or 0 when the slot is empty. Mirrors
     * `bot->GetItemByPos(INVENTORY_SLOT_BAG_0, slot)->GetProto()->ItemId`
     * as used by `PlayerbotFactory::InitEquipment`. */
    uint32_t (*factory_bot_equipped_item_in_slot)(BotHandle bot, uint8_t slot);

    /* Destroy every equipped item. Mirrors
     * `DestroyItemsVisitor(bot);
     *  ai->InventoryIterateItems(&visitor, ITERATE_ITEMS_IN_EQUIP)`
     * at the top of `InitEquipment`'s non-incremental branch. */
    void     (*factory_destroy_all_equipped_items)(BotHandle bot);

    /* Atomic equip-and-enchant used by the tail of `InitEquipment`'s
     * per-slot loop. Destroys any existing item in `slot`, equips
     * `item_id` via `Player::EquipNewItem`, optionally overwrites its
     * random property with `random_enchant_id` (0 = no rewrite), and
     * optionally runs `PlayerbotFactory::EnchantItem` on the newly
     * equipped item. Returns `true` when the equip succeeded. */
    bool     (*factory_equip_new_item_in_slot)(BotHandle bot,
                                                uint8_t slot,
                                                uint32_t item_id,
                                                uint32_t random_enchant_id,
                                                bool apply_enchants);

    /* `Player::InitStatsForLevel(true)` + `Player::UpdateAllStats()` —
     * used at the tail of `InitEquipment` so newly-equipped items take
     * effect in the bot's stat block. */
    void     (*factory_init_stats_for_level_and_update)(BotHandle bot);

    /* `ai->GetMaster()->GetEquipGearScore(..)` — master's current gear
     * score. Writes the score into `*out_gs` and returns true when the
     * bot has a master, otherwise returns false and leaves `*out_gs`
     * untouched. Used by `InitEquipment`'s `syncWithMaster` branch. */
    bool     (*factory_master_equip_gear_score)(BotHandle bot, uint32_t* out_gs);

    /* `ai->TellPlayerNoFacing(ai->GetMaster(), msg)` — broadcast a
     * user-facing message to the bot's master. Silent no-op when the
     * bot has no master. */
    void     (*factory_tell_master)(BotHandle bot, const char* msg);

    /* ── Factory: pet ────────────────────────────────────────────────── */
    /* `bot->GetPet() != nullptr` — does the bot currently have an active
     * pet? Used by both `InitPet` (to decide whether to create one) and
     * `InitPetSpells` (which short-circuits when there is no pet). */
    bool     (*factory_bot_has_pet)(BotHandle bot);

    /* `bot->GetPet()->GetEntry()` — creature template id for the active
     * pet. Returns 0 when the bot has no pet. Drives the warlock per-
     * pet spell dispatch in `InitPetSpells`. */
    uint32_t (*factory_pet_entry)(BotHandle bot);

    /* `sObjectMgr.GetCreatureTemplate(pet_entry)->Family` — the
     * CreatureFamily bucket (1=Wolf, 2=Cat, 3=Spider, …). Drives the
     * hunter pet spell dispatch on vanilla. Returns 0 when the bot has
     * no pet or the template lookup fails. */
    uint32_t (*factory_pet_family)(BotHandle bot);

    /* `bot->GetPet()->GetLevel()` — the pet's current level (usually
     * matches the bot but re-queried rather than assumed). Returns 0
     * when the bot has no pet. */
    uint32_t (*factory_pet_level)(BotHandle bot);

    /* `pet->HasSpell(spell_id)` — does the pet already know this spell?
     * Used to avoid double-learning in `InitPetSpells`. */
    bool     (*factory_pet_has_spell)(BotHandle bot, uint32_t spell_id);

    /* Non-passive, non-removed spell IDs from the pet's `PetSpellMap`.
     * Used at the end of `InitPet` for the mass-toggle-autocast pass.
     * Returned as a freshly-allocated `malloc` array; caller frees via
     * the existing `free_bot_spells` allocator (same contract as
     * `get_bot_spells`). `*out_count` receives the element count. */
    uint32_t* (*factory_pet_autocast_candidate_spells)(BotHandle bot,
                                                        uint32_t* out_count);

    /* Tameable creature ids whose `MinLevel <= bot->GetLevel()`. One-
     * shot enumeration used by `InitPet` to pick a random hunter pet.
     * On WotLK the C++ side additionally gates on `CanTameExoticPets`.
     * Returned as a freshly-allocated `malloc` array; caller frees via
     * `free_bot_spells` (shared uint32 allocator). */
    uint32_t* (*factory_tameable_creatures_for_bot_level)(BotHandle bot,
                                                           uint32_t* out_count);

    /* Atomic hunter-pet creation — mirrors the body of the 100-
     * iteration retry loop in `PlayerbotFactory::InitPet` minus the
     * retry itself. Runs the full `Pet::Create` → faction/level/
     * happiness setup → `AIM_Initialize` → `InitPetCreateSpells` →
     * `LearnPetPassives` → `CastPetAuras` → `UpdateAllStats` →
     * `SetPet`/`SetPetGuid` → `SavePetToDB` → `PetSpellInitialize`
     * chain in one shot. Returns true on success. */
    bool     (*factory_create_hunter_pet)(BotHandle bot, uint32_t creature_entry);

    /* Re-run the pet refresh block at the tail of `InitPet`:
     *   pet->InitStatsForLevel(bot->GetLevel());
     *   pet->SetLevel(bot->GetLevel());
     *   pet->SetLoyaltyLevel(BEST_FRIEND);   // non-WotLK only
     *   pet->SetPower(POWER_HAPPINESS, HAPPINESS_LEVEL_SIZE * 2);
     *   pet->SetHealth(pet->GetMaxHealth());
     *   pet->SetFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_PLAYER_CONTROLLED);
     *   pet->AI()->SetReactState(REACT_DEFENSIVE);
     * No-op when the bot has no pet. */
    void     (*factory_pet_refresh_stats)(BotHandle bot);

    /* `pet->learnSpell(spell_id)` — add a spell to the pet's spellbook.
     * No-op when the bot has no pet. */
    void     (*factory_pet_learn_spell)(BotHandle bot, uint32_t spell_id);

    /* `pet->ToggleAutocast(spell_id, enable)` — flip the autocast bit
     * on a pet spell. Used by both the mass-on pass at the end of
     * `InitPet` and the per-spell Cower toggle in `InitPetSpells`. */
    void     (*factory_pet_toggle_autocast)(BotHandle bot,
                                             uint32_t spell_id,
                                             bool enable);

    /* `pet->SetDeathState(JUST_DIED)` — the "force dismiss pet to fix
     * missing flags" workaround at the end of `InitPet`. Caller only
     * invokes this when the pet is currently alive. */
    void     (*factory_pet_force_dismiss)(BotHandle bot);

    /* ── Factory: random item picks ──────────────────────────────────── */
    /* Wraps sRandomItemMgr.GetRandomTrade(level). Returns an item id or 0. */
    uint32_t (*factory_pick_trade_for_level)(BotHandle bot, uint32_t level);

    /* ── Factory: config list queries ────────────────────────────────── */
    /* Snapshot of sPlayerbotAIConfig.randomBotSpellIds — the list of spell
     * IDs that `InitSpecialSpells` hands out to every bot. Returned as a
     * freshly-allocated array; caller must free it via `free_bot_spells`. */
    uint32_t* (*get_random_bot_spell_ids)(BotHandle bot, uint32_t* out_count);

    /* ── Factory: taxi nodes ─────────────────────────────────────────── */
    /* Overworld taxi nodes (maps 0, 1, 530, 571) that carry a mount for the
     * given team (0=Alliance, 1=Horde). Returned as a freshly-allocated
     * array; caller must call `free_taxi_nodes`. Matches the pre-filtered
     * `overworldTaxiNodeLevelsA/H` tables on the C++ side. */
    BotTaxiNode* (*get_overworld_taxi_nodes)(BotHandle bot, uint8_t team, uint32_t* out_count);
    void         (*free_taxi_nodes)(BotTaxiNode* list);
    /* Mark a taxi node as discovered (Player::m_taxi.SetTaximaskNode). */
    void         (*bot_set_taxi_node)(BotHandle bot, uint32_t node_index);

    /* ── Factory: talents ────────────────────────────────────────────── */
    /* All `TalentEntry` rows belonging to `spec_no` (0..2 = the three talent
     * tabs) that match the bot's class mask, sorted arbitrarily. Returned
     * as a freshly-allocated array; caller must call `free_class_talents`. */
    BotTalentEntry* (*get_class_talents)(BotHandle bot, uint8_t spec_no, uint32_t* out_count);
    void            (*free_class_talents)(BotTalentEntry* list);
    /* Free talent points the bot has available (wraps Player::GetFreeTalentPoints). */
    uint32_t        (*bot_free_talent_points)(BotHandle bot);
    /* Recompute free talent points after learning a talent spell
     * (wraps Player::UpdateFreeTalentPoints(false)). */
    void            (*bot_update_free_talent_points)(BotHandle bot);

    /* Pick (or recall) which talent tab the bot should spend into.
     * Consolidates the C++ state dance into a single callback:
     *   - Reads cached `specNo` from sRandomPlayerbotMgr.
     *   - If `incremental` is true and a cached value exists, returns the
     *     cached spec (minus 1 to undo the storage offset).
     *   - Otherwise rolls `urand(0,100)` against
     *     sPlayerbotAIConfig.specProbability[class][0..1] to pick a tab
     *     in 0..=2, stores `spec+1` back into sRandomPlayerbotMgr, and
     *     returns the chosen tab.
     * Used by factory InitTalentsTree. */
    uint32_t        (*bot_pick_spec_no)(BotHandle bot, bool incremental);

    /* Determine the bot's dominant talent tab (0/1/2) by examining actual
     * talent points.  Mirrors PB2's AiFactory::GetPlayerSpecTab — iterates
     * the TalentStore, counts invested ranks per tab, returns the tab with
     * the highest total.  This is the authoritative spec derivation used
     * when a bot has already had talents applied (by PB2 or the addon UI).
     * Returns 0 when the bot has no talent points invested. */
    uint32_t        (*bot_get_spec_tab)(BotHandle bot);

    /* ── Chat-command helpers (Wave 2) ───────────────────────────────── */
    /* Trigger a jump action on the bot (vertical movement, no target). */
    bool            (*bot_jump)(BotHandle bot);
    /* Use a hearthstone from the bot's bags, if one is present. Returns
     * false if no hearthstone is owned or the item is on cooldown. */
    bool            (*bot_use_hearthstone)(BotHandle bot);
    /* All factions the bot has a reputation value for. Freshly-allocated
     * array; caller must call `bot_free_reputation_list`. */
    BotReputationEntry* (*bot_get_reputation_list)(BotHandle bot, uint32_t* out_count);
    void                (*bot_free_reputation_list)(BotReputationEntry* list);
    /* All skills the bot has learned (non-zero value). Freshly-allocated
     * array; caller must call `bot_free_skill_list`. */
    BotSkillEntry*  (*bot_get_learned_skills)(BotHandle bot, uint32_t* out_count);
    void            (*bot_free_skill_list)(BotSkillEntry* list);
    /* Accept every quest offered by `npc` (GUID of the nearby quest giver
     * the bot currently has selected). Returns true on success. */
    bool            (*bot_quest_accept_from)(BotHandle bot, UnitHandle npc);
    /* Abandon the quest with id `quest_id` from the bot's log. Returns
     * true if the quest was found and removed. */
    bool            (*bot_quest_abandon)(BotHandle bot, uint32_t quest_id);

    /* ── Chat-command helpers (Wave 3: mail + guild) ────────────────── */
    /* Summary of the bot's current mailbox — totals only, no per-mail
     * details. Useful for the `mail` chat-command reply. */
    BotMailSummary  (*bot_mail_summary)(BotHandle bot);
    /* Take all attached money and items from every mail in the inbox.
     * Requires the bot to be within interaction range of a mailbox
     * GameObject. Returns true if any mail was processed. */
    bool            (*bot_mail_take_all)(BotHandle bot);
    /* Make the bot leave its current guild. Returns false if the bot is
     * not in a guild or is the guild master (leader must disband or
     * transfer leadership first). */
    bool            (*bot_guild_leave)(BotHandle bot);

    /* ── RTSC / file I/O helpers ─────────────────────────────────────── */
    /* Summon a temporary marker creature at (x,y,z,o) in the bot's map.
     * `despawn_ms` is the timed-despawn lifetime; `scale` is applied via
     * SetObjectScale after summoning. Used for RTSC waypoint markers. */
    void            (*bot_summon_marker_creature)(BotHandle bot,
                                                  uint32_t entry,
                                                  float x, float y, float z, float o,
                                                  uint32_t despawn_ms,
                                                  float scale);
    /* Write `body` to a bot-data file under the server logs directory.
     * `name` is a short identifier (no path separators). Returns true on
     * success. Used for `rtsc file save` style commands. */
    bool            (*bot_write_log_file)(BotHandle bot,
                                          const char* name,
                                          const char* body);
    /* Read the contents of a bot-data file previously written via
     * bot_write_log_file. On success, `*out_body` points to a heap
     * allocation that the caller must free via `bot_free_string`. */
    bool            (*bot_read_log_file)(BotHandle bot,
                                         const char* name,
                                         char** out_body);
    /* Free a string previously returned by bot_read_log_file. */
    void            (*bot_free_string)(char* s);

    /* Append `line` to a bot-data file under the server logs directory.
     * Same as bot_write_log_file but opens in append mode (std::ios::app).
     * Used by the per-bot debug monitor. */
    bool            (*bot_append_log_file)(BotHandle bot,
                                           const char* name,
                                           const char* line);

    /* ── Addon-channel reply routing ─────────────────────────────────────
     * PB2 `PlayerbotAI.cpp:3475-3485`: when a command arrived on the addon
     * channel (`#a ` prefix / `SendAddonMessage("BOT", …)` / `debug …`),
     * the bot's reply must go back via CHAT_MSG_ADDON / LANG_ADDON so the
     * Mangosbot UI's addon-message listener receives it and refreshes
     * instead of a whisper leaking into the player's chat frame.
     * The C++ side prepends the `BOT\t` prefix and wraps the packet as
     * CHAT_MSG_PARTY with `lang = LANG_ADDON` (the 1.12 wire form for
     * addon messages — the client splits prefix from body on the tab).
     * `target_guid` is the sender (master) ObjectGuid; the packet is
     * delivered directly to that player so only they receive it. */
    bool            (*bot_tell_addon)(BotHandle bot, uint64_t target_guid, const char* msg);

    /* ── Group addon broadcast ──────────────────────────────────────────
     * Broadcast an addon message to the bot's group/raid. Wire format is
     * "PREFIX\tBODY" sent as CHAT_MSG_PARTY/RAID + LANG_ADDON — identical
     * to what the client's SendAddonMessage() produces. Used for protocols
     * like KLHThreatMeter where the bot must appear as a normal addon
     * user to all group members. */
    bool            (*bot_send_group_addon)(BotHandle bot, const char* prefix, const char* msg);

    /* ── Travel destination queries ─────────────────────────────────────
     * These allow the Rust travel planner to query the server for nearby
     * NPCs/GOs that serve a specific purpose (vendor, repair, quest, etc.).
     * Each returns a freshly-allocated array; caller must call the
     * corresponding free function. */

    /* Find nearby NPC(s) matching a purpose bitmask (TravelPurpose flags).
     * Returns up to `max_results` destinations sorted by distance.
     * `max_range` is the search radius in yards (0 = use default 1000y).
     * The returned array is heap-allocated; free with `bot_free_travel_dests`. */
    BotTravelDest*  (*bot_find_travel_dests)(BotHandle bot,
                                             uint32_t purpose_flags,
                                             float max_range,
                                             uint32_t max_results,
                                             uint32_t* out_count);
    void            (*bot_free_travel_dests)(BotTravelDest* list);

    /* ── Spell name resolution ──────────────────────────────────────── */
    /* Resolve a spell name (case-insensitive) to a spell ID.
     * Iterates sSpellTemplate looking for a matching SpellName[0].
     * If the bot knows one or more ranks, returns the highest known rank.
     * Otherwise returns the highest rank that exists in the spell store.
     * Returns 0 when no spell matches the name at all. */
    uint32_t (*resolve_spell_by_name)(BotHandle bot, const char* name);

    /* ── Item name resolution ──────────────────────────────────────── */
    /* Resolve an item name (case-insensitive) to an item ID.
     * Iterates sItemStorage looking for a matching Name1.
     * Prefers exact-length matches over substring matches.
     * Returns 0 when no item matches the name at all. */
    uint32_t (*resolve_item_by_name)(BotHandle bot, const char* name);

    /* ── Equip item ────────────────────────────────────────────────── */
    /* Equip an item by ID in the bot's best available slot.
     * Returns true if the item was successfully equipped. */
    bool (*equip_item)(BotHandle bot, uint32_t item_id);

    /* ── Group management ──────────────────────────────────────────── */
    /* Transfer group/raid leadership from the bot to `target_guid`.
     * Returns true if the bot was the leader and the transfer succeeded. */
    bool (*give_leader)(BotHandle bot, uint64_t target_guid);

    /* Resolve a player name (case-insensitive) to their ObjectGuid.
     * Searches online players first, then the character database.
     * Returns 0 when no player matches the name. */
    uint64_t (*resolve_player_by_name)(BotHandle bot, const char* name);

    /* Unequip the item in a specific equipment slot, moving it to bags.
     * `item_id` is the item entry to find among equipped slots.
     * Returns true if the item was successfully unequipped. */
    bool (*unequip_item)(BotHandle bot, uint32_t item_id);

    /* Invite a player (by GUID) to the bot's group (or create one).
     * Returns true if the invitation was sent. */
    bool (*invite_to_group)(BotHandle bot, uint64_t target_guid);

    /* Destroy the first stack of `item_id` found in the bot's inventory.
     * Returns true if an item was destroyed. */
    bool (*destroy_item)(BotHandle bot, uint32_t item_id);

    /* Share a quest with the party. `quest_id` = 0 means share the first
     * shareable quest in the log. Returns true if shared. */
    bool (*share_quest)(BotHandle bot, uint32_t quest_id);

    /* Send a text emote (e.g. /wave). `text_emote_id` is the TextEmote enum. */
    bool (*do_text_emote)(BotHandle bot, uint32_t text_emote_id);

    /* ── World buffs ─────────────────────────────────────────────────── */

    /* Directly apply aura `spell_id` to the bot (bypasses normal casting).
     * Used by the wbuff strategy to grant config-driven world buffs. */
    bool (*add_aura)(BotHandle bot, uint32_t spell_id);

    /* Get the list of world buff spell IDs the bot is missing (per config
     * `AiPlayerbot.WorldBuff.*` filtering on faction/class/spec/level).
     * Writes up to `max_out` spell IDs into `out_spells` and returns the
     * count written. */
    uint32_t (*get_needed_world_buffs)(BotHandle bot, uint32_t* out_spells, uint32_t max_out);

    /* ── Heal interrupt ──────────────────────────────────────────────── */

    /* Interrupt the bot's own current cast. Returns true if a cast was
     * actually cancelled. */
    bool (*interrupt_own_cast)(BotHandle bot);

    /* ── NPC interaction (gossip) ────────────────────────────────────── */

    /* Gossip-hello with a nearby NPC matching `entry`. Returns true if
     * the NPC was found and the gossip menu was opened. */
    bool (*gossip_hello)(BotHandle bot, uint32_t npc_entry);

    /* Buy `qty` of `item_id` from a nearby vendor. Returns true on
     * success. */
    bool (*buy_from_vendor)(BotHandle bot, uint32_t item_id, uint32_t qty);

    /* ── Mail ────────────────────────────────────────────────────────── */

    /* Send an item from the bot's bags to the master. Returns true on
     * success. */
    bool (*mail_item_to_master)(BotHandle bot);

    /* ── Bank ────────────────────────────────────────────────────────── */

    /* Deposit excess items into the bank. Returns true if anything was
     * deposited. */
    bool (*bank_deposit)(BotHandle bot);

    /* Withdraw useful items from the bank. Returns true if anything was
     * withdrawn. */
    bool (*bank_withdraw)(BotHandle bot);

    /* ── Auction house ───────────────────────────────────────────────── */

    /* Post items on the auction house. Returns true if anything was
     * posted. */
    bool (*ah_post)(BotHandle bot);

    /* Bid on auction house listings. Returns true if a bid was placed. */
    bool (*ah_bid)(BotHandle bot);

    /* ── Outfit ──────────────────────────────────────────────────────── */

    /* Apply the current saved outfit. Returns true if items were
     * equipped. */
    bool (*apply_outfit)(BotHandle bot);

    /* ── Fishing ─────────────────────────────────────────────────────── */

    /* Start fishing (equip pole + cast). Returns true if cast started. */
    bool (*start_fishing)(BotHandle bot);

    /* ── BG/Arena ────────────────────────────────────────────────────── */

    /* Queue the bot for a random battleground. */
    bool (*queue_bg)(BotHandle bot);

    /* Accept a pending BG invitation. */
    bool (*accept_bg_invite)(BotHandle bot);

    /* Get the position of a BG objective (base/flag).
     * `objective_type`: 0=defend, 1=assault, 2=flag, 3=return_flag.
     * Returns {0,0,0} if no objective found. */
    BotPosition (*get_bg_objective_pos)(BotHandle bot, uint8_t objective_type);

    /* ── LFG (WotLK only) ───────────────────────────────────────────── */

    /* Join the LFG queue. Returns true if queued. */
    bool (*lfg_join)(BotHandle bot);

    /* Accept a pending LFG proposal. Returns true if accepted. */
    bool (*lfg_accept)(BotHandle bot);

    /* ── Dungeon awareness ───────────────────────────────────────────── */

    /* Get the tank's current position for stay-near-tank logic.
     * Returns {0,0,0} if no tank found. */
    BotPosition (*get_tank_position)(BotHandle bot);

    /* Check if the given target has an active CC (polymorph, sap, etc.).
     * Returns true if the target is CC'd. */
    bool (*is_unit_cc)(BotHandle bot, UnitHandle target);

    /* ── Debug ───────────────────────────────────────────────────────── */

    /* Dump debug state. kind: 0=full, 1=strategies, 2=blackboard.
     * Returns true if dump was written. */
    bool (*debug_dump_state)(BotHandle bot, uint8_t kind);

    /* ── EngBags: inventory enumeration ─────────────────────────────── */
    /* Return the bot's bag contents (backpack + equipped bags, excluding
     * equipped gear and bank). Freshly-allocated; free with
     * bot_free_inventory_list. */
    BotInventoryItem* (*bot_get_inventory)(BotHandle bot, uint32_t* out_count);
    /* Return the bot's equipped items (EQUIPMENT_SLOT_START..END).
     * Freshly-allocated; free with bot_free_inventory_list. */
    BotInventoryItem* (*bot_get_equipped)(BotHandle bot, uint32_t* out_count);
    /* Return items in the bot's bank slots. Requires banker proximity.
     * Returns NULL/0 if no banker nearby. */
    BotInventoryItem* (*bot_get_bank_items)(BotHandle bot, uint32_t* out_count);
    /* Return items attached to mails in the bot's inbox.
     * `mail_index` is 1-based (matching EngBags #<index> format). */
    BotInventoryItem* (*bot_get_mail_items)(BotHandle bot, uint32_t* out_count);
    /* Free an array returned by any of the above. */
    void              (*bot_free_inventory_list)(BotInventoryItem* list);

    /* ── EngBags: item actions ──────────────────────────────────────── */
    /* Sell a specific item from bags (by item ID). Adds SellPrice to
     * gold and destroys the item. Returns true on success. */
    bool (*sell_item)(BotHandle bot, uint32_t item_id);
    /* Deposit a specific item (by ID) from bags into bank.
     * Requires banker proximity. */
    bool (*bank_deposit_item)(BotHandle bot, uint32_t item_id);
    /* Withdraw a specific item (by ID) from bank into bags.
     * Requires banker proximity. */
    bool (*bank_withdraw_item)(BotHandle bot, uint32_t item_id);
    /* Take items + money from a specific mail (1-based index).
     * Requires mailbox proximity. */
    bool (*bot_mail_take_index)(BotHandle bot, uint32_t mail_index);
    /* Send a specific item (by ID) from bags to the master via mail. */
    bool (*send_mail_item)(BotHandle bot, uint32_t item_id);
    /* Add an item to the bot's trade window (by item ID, count stacks).
     * count=0 means 1 stack. Returns true on success. */
    bool (*trade_add_item)(BotHandle bot, uint32_t item_id, uint32_t count);
    /* Look up the item ID created by a tradeskill spell.
     * Returns 0 if the spell doesn't create an item. */
    uint32_t (*get_spell_craft_item)(uint32_t spell_id);
    /* Look up basic item info by ID. Writes name (null-terminated, up to
     * name_buf_len) and quality. Returns false if the item doesn't exist. */
    bool (*get_item_info)(uint32_t item_id, char* name_buf, uint32_t name_buf_len,
                          uint8_t* out_quality);
} BotCallbacks;

/* ─────────────────────────────────────────────────────────────────────
 * LoginCallbacks — module-level vtable used by the Rust login queue
 * worker (Phase E of the C++ → Rust migration).
 *
 * Unlike `BotCallbacks`, none of these are per-bot. They cover the four
 * pieces of functionality the C++ login queue needed but Rust cannot
 * implement directly: candidate enumeration from the CMaNGOS DBs, the
 * async `CharacterDatabase.DelayQueryHolder` lifecycle, the
 * `RandomPlayerbotMgr` KV store (`GetValue`/`SetValue` with TTL), and
 * the actual login/logout dispatch.
 *
 * Thread model: the Rust login worker drives a dedicated background
 * thread and calls these from that thread. `perform_login` /
 * `perform_logout` are the exception: they mutate map state and must
 * run on the main thread, so the worker forwards them as commands via
 * an mpsc channel; the main thread drains the channel from
 * `playerbot_login_update` and executes them.
 *
 * The vtable is installed once at startup via `playerbot_login_init`.
 * ───────────────────────────────────────────────────────────────────── */

/* Candidate row returned by `query_candidates`. Populated from a single
 * row of the CMaNGOS `characters` table joined against the random-bot
 * `account` rows. All fields are raw DB values — the Rust side applies
 * `instant_randomize` / `disable_random_levels` semantics. */
typedef struct {
    uint32_t account_id;
    uint32_t guid;                   /* low guid, matches ObjectGuid::GetCounter() */
    uint8_t  race;
    uint8_t  cls;
    uint32_t level;
    bool     is_online;              /* characters.online != 0 */
    uint32_t total_played_time;      /* characters.totaltime */
    uint32_t map_id;
    float    x, y, z, o;
    uint32_t guild_id;               /* 0 = no guild */
    uint32_t group_id;               /* snapshotted at load time; 0 = no group */
} BotCandidateInfo;

/* Snapshot of a real (non-bot) player passed to the login queue each
 * update tick. The worker uses these to evaluate distance / map / group
 * / guild gates on bot candidates. */
typedef struct {
    uint32_t guid;
    uint32_t map_id;
    float    x, y, z;
    uint32_t group_id;               /* 0 = not in a group */
    uint32_t guild_id;               /* 0 = no guild */
} BotRealPlayerInfo;

/* HolderState matches the C++ enum: 0=empty, 1=sent, 2=received. */
#define PLAYERBOT_HOLDER_EMPTY    0
#define PLAYERBOT_HOLDER_SENT     1
#define PLAYERBOT_HOLDER_RECEIVED 2

typedef struct LoginCallbacks {
    /* ── Candidate enumeration ──────────────────────────────────────────
     * Scan the random-bot accounts (accounts whose username begins with
     * `account_prefix` case-insensitively) and fill `out_rows` with every
     * matching character. The callee allocates; the caller frees via
     * `free_candidate_list`. Returns the number of rows written to
     * `*out_rows`. */
    uint32_t (*query_candidates)(const char* account_prefix, BotCandidateInfo** out_rows);
    void     (*free_candidate_list)(BotCandidateInfo* rows, uint32_t count);

    /* ── Async holder lifecycle ─────────────────────────────────────────
     * `send_holder` kicks off the CMaNGOS `CharacterDatabase.DelayQueryHolder`
     * that the legacy C++ `PlayerLoginInfo::SendHolder` issued. Returns
     * true if the holder was queued, false on immediate failure. The C++
     * side tracks the holder internally keyed on `guid`; Rust polls
     * `get_holder_state(guid)` to learn when the holder has returned. */
    bool    (*send_holder)(uint32_t account, uint32_t guid);
    /* Returns the current HolderState for `guid` — one of PLAYERBOT_HOLDER_*. */
    uint8_t (*get_holder_state)(uint32_t guid);
    /* Discard the holder for `guid` (e.g. on state reset). Safe to call
     * when there is no holder. */
    void    (*clear_holder)(uint32_t guid);

    /* ── RandomPlayerbotMgr KV store ────────────────────────────────────
     * Thin FFI wrappers around `sRandomPlayerbotMgr.{GetValue,SetValue}`.
     * `key` is an ASCII identifier like "add", "logout", "bot_count".
     * `valid_in_s` is the TTL in seconds; -1 means "use default". */
    uint32_t (*random_mgr_get_value)(uint32_t guid, const char* key);
    void     (*random_mgr_set_value)(uint32_t guid, const char* key, uint32_t value,
                                     int32_t valid_in_s);
    uint32_t (*random_mgr_get_players_level)(void);
    uint32_t (*random_mgr_get_max_online_bot_count)(void);
    uint32_t (*random_mgr_get_world_max_level)(void);
    uint32_t (*random_mgr_database_delay_ms)(void);
    void     (*random_mgr_database_ping)(void);

    /* ── Login / logout dispatch ────────────────────────────────────────
     * Issue the actual login or logout for `guid`. Called on the main
     * thread only (never on the login worker). Returns true if the
     * operation was accepted, false if the guid is unknown or the bot
     * is already in the requested state. */
    bool (*perform_login)(uint32_t guid);
    bool (*perform_logout)(uint32_t guid);

    /* ── Debug logging ──────────────────────────────────────────────────
     * Called when the worker wants to surface a state transition via
     * `sLog.outError` (keeping parity with the original C++ debug path
     * behind `ToggleDebug`). */
    void (*log_debug)(const char* msg);
} LoginCallbacks;

/* ─────────────────────────────────────────────────────────────────────
 * ItemCallbacks — module-level vtable used by the Rust item pool /
 * random-gear subsystem (Phase F of the C++ → Rust migration).
 *
 * Unlike `BotCallbacks`, none of these are per-bot. They cover the data
 * the C++ `RandomItemMgr` previously pulled directly from
 * `sObjectMgr` / `sSpellTemplate` / `sFactionTemplateStore` /
 * `CharacterDatabase` at init time: item prototypes, spell entries,
 * weight scale rows, quest-reward relationships, vendor sources,
 * item-enchantment templates, and a small set of per-player queries the
 * runtime `GetLiveStatWeight` path needs (reputation rank, skill value,
 * quest status, pvp rank).
 *
 * Memory ownership: every `query_*` returns a freshly-allocated array
 * the C++ bridge owns until the matching `free_*` is called. Rust copies
 * the rows it needs into its own caches during `playerbot_itempool_init`
 * and releases the raw arrays before returning.
 *
 * Thread model: every callback is invoked from the main thread during
 * init and from whatever thread the factory runs on at runtime. All
 * underlying reads are thread-safe for the operations exposed here
 * (`sObjectMgr`/`sSpellTemplate` are immutable post-init; `sRandomItemMgr`
 * does not exist anymore).
 *
 * The vtable is installed once at startup via `playerbot_itempool_init`.
 * ───────────────────────────────────────────────────────────────────── */

/* One entry of `ItemPrototype::ItemStat[]`. Max 10 entries per proto
 * (MAX_ITEM_PROTO_STATS). Empty slots have `stat_value == 0`. */
typedef struct {
    uint32_t stat_type;       /* `ItemModType` enum: ITEM_MOD_POWER/AGI/... */
    int32_t  stat_value;
} BotItemProtoStat;

/* One entry of `ItemPrototype::Damage[]`. Max 5 entries per proto
 * (MAX_ITEM_PROTO_DAMAGES). Empty slots have `damage_max == 0`. */
typedef struct {
    float    damage_min;
    float    damage_max;
    uint32_t damage_type;     /* `SpellSchools` enum: 0=normal .. 6=arcane */
} BotItemProtoDamage;

/* One entry of `ItemPrototype::Socket[]`. WotLK/TBC only; zeroed on
 * Classic. Max 3 entries per proto (MAX_ITEM_PROTO_SOCKETS). */
typedef struct {
    uint32_t color;           /* `SocketColor` enum: 1=meta, 2=red, 4=yellow, 8=blue */
    uint32_t content;         /* unused at proto level; always 0 */
} BotItemProtoSocket;

/* One entry of `ItemPrototype::Spells[]`. Max 5 entries per proto
 * (MAX_ITEM_PROTO_SPELLS). Empty slots have `spell_id == 0`. */
typedef struct {
    int32_t  spell_id;
    uint32_t spell_trigger;            /* `ItemSpelltriggerType`: 0=on_use, 1=on_equip, 2=on_hit, ... */
    int32_t  spell_charges;
    float    spell_ppm_rate;
    int32_t  spell_cooldown;           /* milliseconds, -1 = none */
    uint32_t spell_category;
    int32_t  spell_category_cooldown;  /* milliseconds */
} BotItemProtoSpellRef;

/* Subset of CMaNGOS `ItemPrototype` carrying every field the Rust item
 * pool needs. Populated by the C++ bridge from `sItemStorage` during
 * `query_item_prototypes`. Fields absent on a given expansion (flags2,
 * scaling stats, sockets, random_suffix, totem_category, item_limit_*,
 * holiday_id) are zeroed on that build. */
typedef struct {
    uint32_t item_id;
    uint32_t class_id;                 /* `ItemClass` */
    uint32_t sub_class;                /* `ItemSubclass*` within class */
    int32_t  unk0;
    uint32_t quality;                  /* 0=poor .. 7=heirloom */
    uint32_t flags;
    uint32_t flags2;                   /* WotLK+, 0 earlier */
    uint32_t buy_count;
    int32_t  buy_price;                /* uint in 0.x, int in 2.x+; int is lossless */
    uint32_t sell_price;
    uint32_t inventory_type;           /* `InventoryType` */
    int32_t  allowable_class;          /* class mask; -1 = all */
    int32_t  allowable_race;           /* race mask; -1 = all */
    uint32_t item_level;
    uint32_t required_level;
    uint32_t required_skill;
    uint32_t required_skill_rank;
    uint32_t required_spell;
    uint32_t required_honor_rank;      /* Classic PvP rank requirement */
    uint32_t required_city_rank;
    uint32_t required_reputation_faction;
    uint32_t required_reputation_rank;
    uint32_t max_count;
    uint32_t stackable;
    uint32_t container_slots;
    uint32_t stats_count;              /* populated length of `stats` */
    uint32_t scaling_stat_distribution; /* WotLK heirlooms */
    uint32_t scaling_stat_value;       /* WotLK heirlooms */
    uint32_t armor;
    uint32_t holy_res;
    uint32_t fire_res;
    uint32_t nature_res;
    uint32_t frost_res;
    uint32_t shadow_res;
    uint32_t arcane_res;
    uint32_t delay;                    /* weapon speed in milliseconds */
    uint32_t ammo_type;                /* weapon `ammo_type` column */
    float    ranged_mod_range;
    uint32_t bonding;                  /* `ItemBondingType` */
    int32_t  page_text;
    uint32_t lang;
    uint32_t page_material;
    uint32_t start_quest;
    uint32_t lock_id;
    int32_t  material;
    uint32_t sheath;
    int32_t  random_property;
    int32_t  random_suffix;            /* TBC+ */
    uint32_t block;
    uint32_t itemset;
    uint32_t max_durability;
    uint32_t area;
    int32_t  map;
    uint32_t bag_family;
    uint32_t totem_category;           /* TBC+ */
    int32_t  socket_bonus;             /* TBC+ */
    uint32_t gem_properties;           /* TBC+ */
    int32_t  required_disenchant_skill;
    float    armor_damage_modifier;
    uint32_t duration;                 /* high bit = percentage, else seconds */
    uint32_t item_limit_category;      /* WotLK+ */
    uint32_t holiday_id;               /* WotLK+ */

    char     name[96];                 /* null-terminated, truncated to 95 chars */

    BotItemProtoStat     stats[10];    /* MAX_ITEM_PROTO_STATS */
    BotItemProtoDamage   damages[5];   /* MAX_ITEM_PROTO_DAMAGES */
    BotItemProtoSocket   sockets[3];   /* MAX_ITEM_PROTO_SOCKETS */
    BotItemProtoSpellRef spells[5];    /* MAX_ITEM_PROTO_SPELLS */
} BotItemPrototype;

/* Subset of CMaNGOS `SpellEntry` — the fields `CalculateStatWeight` /
 * `CalculateEnchantWeight` read to interpret an item's on-equip /
 * on-use / proc spell. Filled by `lookup_spell_entry`. */
typedef struct {
    uint32_t id;
    uint32_t effect[3];                /* `SpellEffects` enum, 0 = unused slot */
    uint32_t effect_apply_aura_name[3]; /* `AuraType` enum */
    int32_t  effect_base_points[3];    /* signed, can be negative */
    int32_t  effect_misc_value[3];     /* signed, e.g. negative spell power gaps */
    int32_t  effect_misc_value_b[3];
    uint32_t effect_die_sides[3];      /* used by the basepoints/sides coverage calc */
    int32_t  duration_ms;              /* resolved from sSpellDurationStore; 0 = no duration */
    uint32_t recovery_time_ms;
    int32_t  proc_chance;              /* -1 if not a proc spell */
    uint32_t school_mask;
    char     name[96];                 /* null-terminated; truncated */
} BotSpellEntryInfo;

/* One row of the custom `ai_playerbot_weight_scales` table. Each scale
 * describes a class/spec slot: id is a small integer, name is the
 * internal spec key (e.g. "arms", "holy", "combat"). */
typedef struct {
    uint32_t id;
    uint32_t class_id;                 /* CLASS_WARRIOR..CLASS_DRUID */
    char     name[40];                 /* spec key; null-terminated */
} BotWeightScaleRow;

/* One row of the custom `ai_playerbot_weight_scales_stats` table.
 * (scale_id, stat_name, weight) triples drive `CalculateSingleStatWeight`. */
typedef struct {
    uint32_t scale_id;
    char     stat[32];                 /* stat key: "sta", "splpwr", "mledps", ... */
    int32_t  weight;
} BotWeightScaleStatRow;

/* One (quest_id, reward_item_id) relationship derived from
 * `quest_template` at init time. Used by `GetQuestIdsForItem` to find
 * which quests reward a given item and by the "quest source" detection
 * in `BuildItemInfoCache`. */
typedef struct {
    uint32_t item_id;
    uint32_t quest_id;
    uint32_t min_level;                /* Quest.MinLevel */
    uint32_t required_classes;         /* Quest.RequiredClasses */
    uint32_t required_races;           /* Quest.RequiredRaces */
    uint32_t zone_or_sort;             /* Quest.ZoneOrSort, for PVP-quest heuristics */
    uint32_t reward_rep_faction;       /* Quest.RewRepFaction[0], 0 = none */
    int32_t  reward_rep_value;         /* Quest.RewRepValue[0] */
    bool     is_choice;                /* true = RewardChoiceItemId, false = RewardItemId */
} BotQuestItemRow;

/* One (vendor_entry, item_id) relationship from `npc_vendor`, with the
 * vendor's faction and (if the vendor entry has a condition referring
 * to reputation) the pre-resolved required rep rank + faction. */
typedef struct {
    uint32_t vendor_entry;
    uint32_t item_id;
    uint32_t vendor_faction_template;
    uint32_t vendor_faction;           /* FactionTemplateEntry.faction */
    uint32_t required_reputation_rank; /* 0 = no requirement */
    uint32_t required_reputation_faction;
    uint32_t condition_id;             /* original condition row; 0 = none */
} BotVendorItemRow;

/* One row of `item_enchantment_template` — the CMaNGOS table that maps
 * random property entries to enchant ids with weighted chances. Used
 * by `LoadRandomEnchantments` and `CalculateRandomPropertyWeight`. */
typedef struct {
    uint32_t entry;
    uint32_t ench;
    float    chance;
} BotItemEnchantmentRow;

/* Subset of `SpellItemEnchantmentEntry` carrying the fields
 * `CalculateEnchantWeight` decodes: three (type, amount, spellid)
 * triples plus the aura base id that some types resolve through. */
typedef struct {
    uint32_t id;
    uint32_t type[3];                  /* `ItemEnchantmentType` */
    int32_t  amount[3];
    uint32_t spell_id[3];
    char     description[96];          /* null-terminated; truncated */
    bool     is_valid;
} BotEnchantInfo;

/* Optional row from `ai_playerbot_rarity_cache`. When the cache does
 * not exist the bridge returns zero rows and `GetItemRarity` falls back
 * to a computed value. */
typedef struct {
    uint32_t item_id;
    float    rarity;
} BotItemRarityRow;

/* One row from `ai_playerbot_equip_cache` — a pre-built equip cache
 * keyed by (class, spec, level, slot, quality). When the table is empty
 * the bridge returns 0 rows and Rust rebuilds the cache from scratch
 * (an expensive multi-billion-iteration scan over every item prototype). */
typedef struct {
    uint32_t clazz;
    uint32_t spec;
    uint32_t level;
    uint32_t slot;
    uint32_t quality;
    uint32_t item_id;
} BotEquipCacheRow;

/* One row from `ai_playerbot_rnditem_cache` — a pre-built random-item
 * cache keyed by (level decade, random-item type). Empty → Rust rebuilds. */
typedef struct {
    uint32_t level_bucket;
    uint32_t kind;
    uint32_t item_id;
} BotRandomCacheRow;

/* Subset of `GemProperties.dbc` — the gem metadata the socket-weight
 * calculation needs on TBC/WotLK. Empty on Classic. */
typedef struct {
    uint32_t id;
    uint32_t spell_item_enchantment;
    uint32_t color;                    /* `SocketColor` mask */
    uint32_t required_skill_id;
    uint32_t required_skill_value;
} BotGemPropertiesRow;

/* Per-player context read before runtime stat-weight scoring. Populated
 * by `get_player_item_ctx` and forwarded through to the Rust live
 * weight calculation so the bridge only needs one callback per query. */
typedef struct {
    uint32_t guid_low;
    uint32_t level;
    uint32_t class_id;                 /* 1..11 */
    uint32_t race_id;                  /* 1..11 */
    uint32_t team;                     /* 0 = Alliance, 1 = Horde */
    uint32_t pvp_rank;                 /* Classic: GetHonorHighestRankInfo().rank; TBC+: honor rank index */
    uint32_t map_id;
    bool     in_world;
} BotPlayerItemCtx;

/* Quest status bitflags returned by `player_quest_status`. */
#define PLAYERBOT_QUEST_SATISFIES_CLASS  0x01u
#define PLAYERBOT_QUEST_SATISFIES_RACE   0x02u
#define PLAYERBOT_QUEST_SATISFIES_LEVEL  0x04u
#define PLAYERBOT_QUEST_IS_ACTIVE        0x08u
#define PLAYERBOT_QUEST_IS_COMPLETED     0x10u
#define PLAYERBOT_QUEST_IS_REWARDED      0x20u

typedef struct ItemCallbacks {
    /* ── Item prototype enumeration ─────────────────────────────────
     * Scan every row in `sItemStorage` and return a freshly-allocated
     * array of `BotItemPrototype`. The caller frees via
     * `free_item_prototype_list(ptr, count)`. Returns the number of
     * rows written. */
    uint32_t (*query_item_prototypes)(BotItemPrototype** out_rows);
    void     (*free_item_prototype_list)(BotItemPrototype* rows, uint32_t count);

    /* ── On-demand spell entry lookup ────────────────────────────────
     * Resolve a spell id against `sSpellTemplate` and fill `out` with
     * the subset of fields the stat-weight calculation reads. Returns
     * `false` if the spell is not in the store; `out` is then
     * untouched. */
    bool (*lookup_spell_entry)(uint32_t spell_id, BotSpellEntryInfo* out);

    /* ── Weight scales ──────────────────────────────────────────────
     * Load the full `ai_playerbot_weight_scales` table (scales) and
     * the paired `ai_playerbot_weight_scales_stats` table (per-scale
     * stat weights). If the tables don't exist the bridge returns 0
     * rows for both; Rust will fall back to compile-time defaults. */
    uint32_t (*query_weight_scales)(BotWeightScaleRow** out_rows);
    void     (*free_weight_scale_list)(BotWeightScaleRow* rows, uint32_t count);
    uint32_t (*query_weight_scale_stats)(BotWeightScaleStatRow** out_rows);
    void     (*free_weight_scale_stat_list)(BotWeightScaleStatRow* rows, uint32_t count);

    /* ── Quest rewards ──────────────────────────────────────────────
     * Enumerate every (quest, reward item) pair known to the
     * `ObjectMgr` quest store. One row per (quest × reward item); if a
     * quest rewards 4 items, 4 rows are emitted. */
    uint32_t (*query_quest_items)(BotQuestItemRow** out_rows);
    void     (*free_quest_item_list)(BotQuestItemRow* rows, uint32_t count);

    /* ── Vendor sources ─────────────────────────────────────────────
     * Enumerate every (vendor, sold item) pair from `npc_vendor`,
     * pre-resolving the vendor's faction and (when the vendor row
     * carries a condition referring to reputation) the rep rank
     * requirement. Rows for vendors whose faction lookup fails are
     * skipped. */
    uint32_t (*query_vendor_items)(BotVendorItemRow** out_rows);
    void     (*free_vendor_item_list)(BotVendorItemRow* rows, uint32_t count);

    /* ── Item enchantment template ──────────────────────────────────
     * Load the `item_enchantment_template` table. One row per
     * (entry, ench) pair, carrying the weighted chance. Used to
     * decode random property enchant pools. */
    uint32_t (*query_item_enchantment_template)(BotItemEnchantmentRow** out_rows);
    void     (*free_item_enchantment_list)(BotItemEnchantmentRow* rows, uint32_t count);

    /* ── On-demand enchant lookup ───────────────────────────────────
     * Resolve an enchant id against `sSpellItemEnchantmentStore` and
     * fill `out`. Returns `false` on miss. */
    bool (*lookup_enchant_info)(uint32_t enchant_id, BotEnchantInfo* out);

    /* ── Random-property → enchant slot resolver ────────────────────
     * For a given random property id, fill `out_enchant_ids[4]` with
     * the enchant ids from `sItemRandomPropertiesStore[id].enchant_id`.
     * Returns `false` on miss. */
    bool (*lookup_random_property_enchants)(int32_t property_id, uint32_t out_enchant_ids[4]);

    /* ── Optional rarity cache ──────────────────────────────────────
     * If the `ai_playerbot_rarity_cache` table exists, return its
     * rows. Otherwise return 0 and Rust will fall back to its
     * computed rarity heuristic. */
    uint32_t (*query_rarity_rows)(BotItemRarityRow** out_rows);
    void     (*free_rarity_list)(BotItemRarityRow* rows, uint32_t count);

    /* ── Optional pre-built equip / random-item caches ──────────────
     * If `ai_playerbot_equip_cache` / `ai_playerbot_rnditem_cache`
     * hold rows, return them so the Rust item pool loads instead of
     * rebuilding. Returning 0 rows forces a full rebuild during
     * `playerbot_itempool_init` (very slow — minutes on the main
     * thread). The caller frees via the paired `free_*` callback. */
    uint32_t (*query_equip_cache_rows)(BotEquipCacheRow** out_rows);
    void     (*free_equip_cache_list)(BotEquipCacheRow* rows, uint32_t count);
    uint32_t (*query_random_cache_rows)(BotRandomCacheRow** out_rows);
    void     (*free_random_cache_list)(BotRandomCacheRow* rows, uint32_t count);

    /* ── Gem properties (TBC/WotLK) ─────────────────────────────────
     * Return `sGemPropertiesStore` contents. Returns 0 rows on Classic. */
    uint32_t (*query_gem_properties)(BotGemPropertiesRow** out_rows);
    void     (*free_gem_list)(BotGemPropertiesRow* rows, uint32_t count);

    /* ── Per-player queries ─────────────────────────────────────────
     * `guid_low` is the low part of `ObjectGuid` (what
     * `ObjectGuid::GetCounter()` returns). `get_player_item_ctx`
     * returns `false` if the guid is not a resolvable logged-in
     * player. */
    bool     (*get_player_item_ctx)(uint32_t guid_low, BotPlayerItemCtx* out);
    int32_t  (*get_player_reputation_rank)(uint32_t guid_low, uint32_t faction_id);
    uint32_t (*get_player_skill_value)(uint32_t guid_low, uint32_t skill_id);
    /* Returns the bitmask defined above; 0 = unknown quest. */
    uint32_t (*player_quest_status)(uint32_t guid_low, uint32_t quest_id);
    /* Equivalent to `player->HasItemCount(item_id, 1, true)` — checks
     * bags + bank for at least one copy of the item. Used by
     * `HasSameQuestRewards` to skip duplicate quest rewards during
     * factory gearing. Returns `false` when the guid is not a logged-in
     * player. */
    bool     (*player_has_item)(uint32_t guid_low, uint32_t item_id);
    /* Returns the bot's best talent-tab index (0..2) and sets
     * `*out_meditation_rank` to the rank of the Meditation talent
     * (used by priest disc/holy disambiguation). 0 = no points. */
    uint32_t (*get_player_talent_tab)(uint32_t guid_low, uint32_t* out_meditation_rank);

    /* ── RNG ────────────────────────────────────────────────────────
     * Wraps CMaNGOS `urand(min, max)`. Inclusive. Rust uses this when
     * it would have called the legacy C++ urand path; tests plug in a
     * deterministic impl instead. */
    uint32_t (*urand_range)(uint32_t min, uint32_t max);

    /* ── Debug logging ──────────────────────────────────────────────
     * Optional; used for init-time progress lines so they show up via
     * `sLog` when `itempool debug` is flipped on. */
    void (*log_debug)(const char* msg);
} ItemCallbacks;

/* ─────────────────────────────────────────────────────────────────────
 * RandomFactoryCallbacks — module-level vtable used by the Rust random
 * bot factory (Phase G of the C++ → Rust migration).
 *
 * Unlike `BotCallbacks`, none of these are per-bot. They cover every
 * CMaNGOS-side operation the legacy `playerbot/RandomPlayerbotFactory.cpp`
 * performed: DB scans over accounts/characters/names/guilds/arena teams,
 * the async account-creation fan-out, `Player::Create` + object accessor
 * bookkeeping, and the guild/arena team lifecycle helpers on
 * `sGuildMgr`/`sObjectMgr`. Everything else (race/class availability,
 * probability-based class/race selection, name-pool expansion via the
 * bijective-base-26 postfix helper, orchestration of the three public
 * entry points) is owned by the Rust `random` module and does not need a
 * callback.
 *
 * Memory ownership: every `query_*_list`-shaped callback allocates a
 * freshly-malloc'd array that the C++ bridge owns; Rust releases it via
 * the matching `free_*_list` callback before returning control. Single-
 * row string callbacks (`query_random_bot_name`, `query_random_guild_name`,
 * `query_random_arena_team_name`) instead write into a caller-provided
 * buffer and return `false` on miss.
 *
 * Thread model: all callbacks are invoked from the thread that runs the
 * Rust `create_random_bots` / `create_random_guilds` /
 * `create_random_arena_teams` entry points. The legacy C++ version used
 * `std::async` to fan out account creation and per-character saves; the
 * Rust port runs synchronously and relies on CMaNGOS' own
 * `CharacterDatabase` queue to absorb the burst. The bridge is therefore
 * safe to implement with ordinary blocking DB calls.
 *
 * The vtable is installed once at startup via
 * `playerbot_random_factory_init`.
 * ───────────────────────────────────────────────────────────────────── */

/* Single-row result of `query_bot_delete_event`, mirroring the legacy
 * `select value from ai_playerbot_random_bots where event = 'bot_delete'`
 * shape. When no row is present `scheduled` is false and
 * `delete_friends` is ignored. When the row's value is > 1 the caller
 * removes every random-bot character regardless of friend/guild status;
 * otherwise friends/guild members are preserved. */
typedef struct {
    bool scheduled;
    bool delete_friends;
} BotDeleteEventState;

/* One row from `ai_playerbot_names` left-joined against `characters`.
 * `gender_index` matches the `NameRaceAndGender` enum (0..17).
 * `is_taken` is true iff the joined character row has a non-null guid
 * — used to seed the in-Rust `freeNames` / `allNames` split. */
typedef struct {
    uint8_t gender_index;
    char    name[16];                  /* null-terminated; Blizzard char
                                        * names are max 12 chars */
    bool    is_taken;
} BotNamePoolRow;

/* Character appearance slice picked by the bridge from
 * `sCharSectionMap` for a given (race, gender). Mirrors the local
 * variables the C++ `CreateRandomBot` built before calling
 * `Player::Create`. `success` is false when no matching sections exist
 * (which is the cue for the Rust side to skip the class/race pair). */
typedef struct {
    uint8_t skin_color;                /* `face.second` in the C++ port */
    uint8_t face_id;                   /* `face.first` */
    uint8_t face_color;                /* duplicate of skin_color — the
                                        * legacy code reused `face.second`
                                        * for the skin colour slot */
    uint8_t hair_style;                /* `hair.first` */
    uint8_t hair_color;                /* `hair.second` */
    uint8_t facial_hair;               /* 0 for excluded races/genders
                                        * (tauren, non-nightelf/undead
                                        * females) and always 0 on
                                        * WotLK — see C++ `excludeCheck` */
    bool    success;
} BotCharAppearance;

/* Full parameter bundle passed to `create_random_bot_character`. The
 * bridge is responsible for instantiating `WorldSession` + `Player`,
 * calling `Player::Create`, `setCinematic`, `SetAtLoginFlag`, and
 * `sObjectAccessor.AddObject`. Returns `true` on success. The Rust
 * side does not retain any pointer to the created player. */
typedef struct {
    uint32_t account_id;
    uint8_t  race;
    uint8_t  cls;
    uint8_t  gender;
    char     name[16];                 /* null-terminated */
    uint8_t  skin_color;
    uint8_t  face_id;
    uint8_t  face_color;
    uint8_t  hair_style;
    uint8_t  hair_color;
    uint8_t  facial_hair;
} BotCreateParams;

/* Snapshot of an in-world player read during guild/arena leader
 * selection. `in_world` is false when the guid is not a currently
 * logged-in player — the rest of the fields are then ignored. */
typedef struct {
    bool     in_world;
    uint8_t  level;
    uint32_t guild_id;                 /* 0 = no guild */
    uint32_t team;                     /* 0 = Alliance, 1 = Horde */
    char     name[16];                 /* null-terminated; used in log
                                        * strings to match the legacy
                                        * `sLog.outBasic` format */
} BotPlayerSnapshot;

/* One row of `query_character_accounts` — the `(account, guid)` pair
 * from the `characters` table used to group bot characters by their
 * owning account when building the guild-leader candidate list. */
typedef struct {
    uint32_t account_id;
    uint32_t guid_low;
} BotCharacterAccount;

/* Arena team kind identifiers — values match CMaNGOS `ArenaType`. */
#define PLAYERBOT_ARENA_TYPE_2V2 2
#define PLAYERBOT_ARENA_TYPE_3V3 3
#define PLAYERBOT_ARENA_TYPE_5V5 5

typedef struct RandomFactoryCallbacks {
    /* ── RNG ────────────────────────────────────────────────────────
     * Wraps CMaNGOS `urand(min, max)`. Inclusive on both ends. Rust
     * calls this wherever the legacy C++ path would have called
     * `urand` directly. Tests plug in a deterministic `SequentialRng`
     * instead. */
    uint32_t (*urand_range)(uint32_t min, uint32_t max);

    /* ── Bot delete event ──────────────────────────────────────────
     * Read `ai_playerbot_random_bots WHERE event = 'bot_delete'`.
     * `out` is always written — on miss it is zeroed. */
    void (*query_bot_delete_event)(BotDeleteEventState* out);

    /* ── Account scan / create / delete ─────────────────────────────
     * Look up an account id by exact username match. Returns 0 when
     * the account does not exist. */
    uint32_t (*query_account_id_by_name)(const char* name);

    /* Scan every account whose username begins with `prefix` (case-
     * insensitively) and allocate an id array into `*out_ids`. The
     * return value is the array length; caller frees via
     * `free_u32_list`. */
    uint32_t (*query_account_ids_like_prefix)(const char* prefix, uint32_t** out_ids);
    void     (*free_u32_list)(uint32_t* ptr, uint32_t count);

    /* Create an account (synchronous). `password` may be empty,
     * meaning "use the account name as the password". `max_expansion`
     * is `MAX_EXPANSION` on TBC/WotLK and 0 on Classic. */
    void (*create_account)(const char* name, const char* password, uint8_t max_expansion);
    /* Delete an account and every character on it. */
    void (*delete_account)(uint32_t account_id);

    /* ── Character count / query / friend list ──────────────────────
     * `sAccountMgr.GetCharactersCount(account_id)`. */
    uint32_t (*get_character_count)(uint32_t account_id);

    /* Enumerate every character guid on `account_id`. Caller frees
     * via `free_u32_list`. */
    uint32_t (*query_characters_by_account)(uint32_t account_id, uint32_t** out_guids);

    /* Load every friend guid from `character_social` where
     * `flags=SOCIAL_FLAG_FRIEND`. Caller frees via `free_u32_list`. */
    uint32_t (*query_friend_guids)(uint32_t** out_guids);

    /* `sRandomPlayerbotMgr.OnPlayerLoginError(guid)` — invoked before
     * a character row is deleted so the login queue drops any
     * pending state for it. */
    void (*on_player_login_error)(uint32_t guid_low);

    /* `Player::DeleteFromDB(ObjectGuid, account, false, true)` —
     * removes the character row and its associated tables. */
    void (*delete_character_from_db)(uint32_t guid_low, uint32_t account_id);

    /* `CharacterDatabase.Execute("DELETE FROM ai_playerbot_random_bots
     * WHERE bot NOT IN (SELECT guid FROM characters)")` — invoked once
     * at the end of the delete pass. */
    void (*prune_random_bots_table)(void);

    /* ── Name pool ─────────────────────────────────────────────────
     * Load every row of `ai_playerbot_names` joined against
     * `characters` so the Rust side can decide which names are free
     * to use. One row per name; `is_taken` encodes the join result.
     * Caller frees via `free_name_pool_list`. An empty result means
     * the name DB has not been loaded and Rust falls back to the
     * legacy character-names path. */
    uint32_t (*query_name_pool)(BotNamePoolRow** out_rows);
    void     (*free_name_pool_list)(BotNamePoolRow* rows, uint32_t count);

    /* Fallback load used when `ai_playerbot_names` is unpopulated —
     * returns every existing character name with `gender_index = 0`
     * and `is_taken = true`. Rust then builds the postfix-expanded
     * pool on top of those seeds. */
    uint32_t (*query_existing_character_names)(BotNamePoolRow** out_rows);

    /* Single-row fallback used by `CreateRandomBotName` when the Rust
     * name pool is empty for a given race/gender. `out_buf` is
     * caller-provided; writes a null-terminated name and returns
     * true on success. */
    bool (*query_random_bot_name)(uint8_t race_and_gender, char* out_buf, uint32_t buf_len);

    /* ── Character creation ────────────────────────────────────────
     * Resolve the appearance options for `(race, gender)` by scanning
     * `sCharSectionMap`. On success fills `out` and returns true;
     * returns false if no sections match. */
    bool (*query_char_appearance)(uint8_t race, uint8_t gender, BotCharAppearance* out);

    /* Create a single random-bot character using the supplied
     * appearance/class/race/name. Wraps `WorldSession` allocation,
     * `Player::Create`, `SetAtLoginFlag`, and
     * `sObjectAccessor.AddObject`. Returns true on success. */
    bool (*create_random_bot_character)(const BotCreateParams* params);

    /* Save every currently-in-world player (random bot session) and
     * tear them down — `SaveToDB` + `LogoutPlayer` + delete-session
     * delete-player loop. Called once at the end of `CreateRandomBots`
     * to release the sessions/players spawned during the pass. */
    void (*save_and_logout_all_online)(void);

    /* ── Guild enumeration / creation ──────────────────────────────
     * Scan every `(account, guid)` pair from the `characters` table
     * (used to build the per-account guid buckets for guild leader
     * selection). Caller frees via `free_character_account_list`. */
    uint32_t (*query_character_accounts)(BotCharacterAccount** out_rows);
    void     (*free_character_account_list)(BotCharacterAccount* rows, uint32_t count);

    /* `sGuildMgr.GetGuildByLeader(guid)->GetId()` — returns 0 when
     * the guid does not lead a guild. */
    uint32_t (*get_guild_id_by_leader)(uint32_t leader_guid_low);
    /* `sObjectMgr.GetPlayerAccountIdByGUID(guild->GetLeaderGuid())`
     * — used when deciding whether a random bot guild's leader is
     * itself a random bot account. Returns 0 on miss. */
    uint32_t (*get_guild_leader_account_id)(uint32_t guild_id);
    /* `guild->Disband()`. */
    void (*disband_guild)(uint32_t guild_id);

    /* `sObjectMgr.GetPlayer(guid)` wrapper; returns false when the
     * guid is not a currently logged-in player. Fills `out` on
     * success. */
    bool (*get_player_snapshot)(uint32_t guid_low, BotPlayerSnapshot* out);

    /* Single-row query against `ai_playerbot_guild_names`. Writes a
     * null-terminated name into `out_buf` on success; returns false
     * when no row is available. */
    bool (*query_random_guild_name)(char* out_buf, uint32_t buf_len);

    /* `Guild::Create(player, name)` + `sGuildMgr.AddGuild` — returns
     * the new guild id, or 0 on failure. */
    uint32_t (*create_random_guild)(uint32_t leader_guid_low, const char* name);

    /* `guild->SetEmblem(st, cl, br, bc, bg)`. Matches the legacy
     * argument order verbatim. */
    void (*set_guild_emblem)(uint32_t guild_id,
                             uint32_t style, uint32_t color,
                             uint32_t border_style, uint32_t border_color,
                             uint32_t background_color);
    /* `guild->SetGINFO(info)` — used to stamp a random age label. */
    void (*set_guild_info)(uint32_t guild_id, const char* info);

    /* ── Arena team (wotlk only — may be NULL on Classic/TBC) ──────
     * Read `ai_playerbot_random_bots WHERE event = 'add'` — used as
     * the captain candidate pool in `CreateRandomArenaTeams`. Caller
     * frees via `free_u32_list`. */
    uint32_t (*query_random_bot_add_event)(uint32_t** out_guids);

    /* `sObjectMgr.GetArenaTeamByCaptain(guid)`; fills out params and
     * returns true when a team exists. */
    bool (*get_arena_team_by_captain)(uint32_t captain_guid_low,
                                      uint32_t* out_team_id, uint8_t* out_type);
    /* `arenateam->Disband(NULL)`. */
    void (*disband_arena_team)(uint32_t team_id);
    /* `player->GetArenaTeamId(ArenaTeam::GetSlotByType(team_type))`.
     * Returns 0 when the player is not on an arena team of that
     * type. */
    uint32_t (*get_player_arena_team_id_by_type)(uint32_t guid_low, uint8_t team_type);

    /* Single-row query against `ai_playerbot_arena_team_names` —
     * returns both the name and the `type` column (one of
     * PLAYERBOT_ARENA_TYPE_{2V2,3V3,5V5}). Writes `out_name` as a
     * null-terminated string and sets `*out_type` on success. */
    bool (*query_random_arena_team_name)(char* out_name, uint32_t buf_len, uint8_t* out_type);

    /* `ArenaTeam::Create` + `SetCaptain` + `sObjectMgr.AddArenaTeam`
     * — returns the new team id, or 0 on failure. */
    uint32_t (*create_arena_team)(uint32_t captain_guid_low, uint8_t team_type, const char* name);

    /* `arenateam->AddMember(member_guid)`. Returns true on success. */
    bool (*add_arena_team_member)(uint32_t team_id, uint32_t member_guid_low);
    /* `arenateam->GetMembersSize()`. */
    uint32_t (*get_arena_team_members_size)(uint32_t team_id);
    /* `arenateam->SetEmblem(background_color, style, color,
     * border_style, border_color)`. Argument order matches the C++
     * signature. */
    void (*set_arena_team_emblem)(uint32_t team_id,
                                   uint32_t background_color, uint32_t style,
                                   uint32_t color, uint32_t border_style,
                                   uint32_t border_color);
    /* `arenateam->SetRatingForAll(rating)`. */
    void (*set_arena_team_rating)(uint32_t team_id, uint32_t rating);
    /* `arenateam->SaveToDB()`. */
    void (*save_arena_team)(uint32_t team_id);

    /* ── Debug logging ─────────────────────────────────────────────
     * Optional; routed to `sLog.outBasic`. */
    void (*log_debug)(const char* msg);
} RandomFactoryCallbacks;

/* ─────────────────────────────────────────────────────────────────────
 * RandomMgrCallbacks — module-level vtable used by the Rust random
 * playerbot manager (Phase H of the C++ → Rust migration).
 *
 * Phase H moves `playerbot/RandomPlayerbotMgr.cpp` (≈4100 LOC) into the
 * Rust `playerbot` crate's `random_mgr` module. These callbacks cover
 * every CMaNGOS-side operation the legacy class did: DB I/O against the
 * `ai_playerbot_random_bots` event store, teleport-cache maintenance,
 * named-location loading, per-bot action dispatch (Randomize / Revive /
 * teleport / etc.), BG / LFG / arena queue inspection, auction-house
 * mirroring, and the world-diff / DB-delay sampling that feeds the PID
 * controller.
 *
 * Thread model: the Rust random-mgr worker thread owns a handle to this
 * vtable and performs DB reads from that thread. Per-bot action dispatch
 * (`dispatch_*`) is forwarded back to the main thread by the worker
 * through its command channel; `playerbot_random_mgr_update` drains
 * that channel on the main tick so the actual mutation happens under
 * the CMaNGOS world lock.
 *
 * The vtable is installed once at startup via `playerbot_random_mgr_init`.
 * ───────────────────────────────────────────────────────────────────── */

/* One row of `ai_playerbot_random_bots`. Strings are inline char
 * buffers so the whole struct is POD and bindgen-safe. */
typedef struct {
    uint32_t bot;
    char     event[32];
    uint32_t value;
    uint32_t last_change_time;       /* unix epoch seconds */
    uint32_t valid_in;               /* TTL in seconds; 0 = no TTL */
    char     data[256];
} BotEventRow;

/* Snapshot of a world-diff sample — the three counters
 * `sWorld.GetCurrentDiff()`, `sWorld.GetAverageDiff()`, and
 * `sWorld.GetMaxDiff()` the legacy `ScaleBotActivity` fed into the PID
 * controller. */
typedef struct {
    float current_diff_ms;
    float average_diff_ms;
    float max_diff_ms;
} BotWorldDiffSample;

/* One row of the random-bot teleport cache. */
typedef struct {
    uint32_t level;
    uint32_t map_id;
    float    x;
    float    y;
    float    z;
} BotTeleportRow;

/* One row of `ai_playerbot_named_location`. */
typedef struct {
    char     name[64];
    uint32_t map_id;
    float    x;
    float    y;
    float    z;
    float    orientation;
} BotNamedLocationRow;

/* One row in the BG / arena queue snapshot the worker consumes per
 * tick to rebuild the `BgPlayers`/`BgBots`/`ArenaBots` bucket matrix.
 * `arena_type == 0` marks a non-arena BG row. */
typedef struct {
    uint32_t queue_type;
    uint32_t bracket_id;
    uint32_t team_id;                /* 0=Alliance, 1=Horde */
    bool     is_bot;
    uint32_t level;
    uint32_t map_id;
    uint32_t arena_rating;
    uint8_t  arena_type;             /* 0=non-arena, 2=2v2, 3=3v3, 5=5v5 */
} BotBgQueueEntry;

/* One row in the LFG queue snapshot. Classic/TBC leaves this empty. */
typedef struct {
    uint8_t  team_id;                /* 0=Alliance, 1=Horde, 2=both */
    uint32_t dungeon_id;
    uint32_t role_mask;
} BotLfgQueueEntry;

/* One row of the auction house mirror (per currently-listed auction). */
typedef struct {
    uint32_t item_id;
    uint32_t buyout;
    uint32_t count;
} BotAhMirrorRow;

/* Real-player level summary used by `CheckPlayers` to drive the
 * `playersLevel` running average. */
typedef struct {
    uint32_t level;
    uint32_t total_time;
} BotRealPlayerLevel;

/* Per-bot snapshot used by `PrintStats` to render the /bot stats
 * console output. One row per currently-owned bot. The `flags` field
 * is a bitmask of `PLAYERBOT_STATS_FLAG_*` entries below. */
typedef struct {
    uint32_t guid;
    uint8_t  race;     /* Mangos `Races` enum value */
    uint8_t  class_id; /* Mangos `Classes` enum value */
    uint8_t  team;     /* 0 = Alliance, 1 = Horde */
    uint32_t level;
    uint32_t flags;
} BotStatsRow;

/* Bitflags for `BotStatsRow::flags` — keep in sync with
 * `crates/cmangos/src/random_mgr_world.rs`. */
#define PLAYERBOT_STATS_FLAG_ACTIVE  (1u << 0)
#define PLAYERBOT_STATS_FLAG_DEAD    (1u << 1)
#define PLAYERBOT_STATS_FLAG_COMBAT  (1u << 2)
#define PLAYERBOT_STATS_FLAG_TAXI    (1u << 3)
#define PLAYERBOT_STATS_FLAG_MOVING  (1u << 4)
#define PLAYERBOT_STATS_FLAG_MOUNTED (1u << 5)
#define PLAYERBOT_STATS_FLAG_AFK     (1u << 6)

typedef struct RandomMgrCallbacks {
    /* ── RNG ──────────────────────────────────────────────────────── */
    uint32_t (*urand_range)(uint32_t min, uint32_t max);

    /* ── World globals ───────────────────────────────────────────── */
    uint32_t (*current_ms_time)(void);
    bool     (*is_shutting_down)(void);
    uint32_t (*world_max_level)(void);
    void     (*world_diff_sample)(BotWorldDiffSample* out);
    void     (*database_ping)(void);
    uint32_t (*database_delay_ms)(const char* db);

    /* ── Event KV store (ai_playerbot_random_bots) ────────────────── */
    uint32_t (*query_all_events)(uint32_t /*unused*/, BotEventRow** out_rows);
    uint32_t (*query_events_for_bot)(uint32_t bot, BotEventRow** out_rows);
    void     (*free_event_row_list)(BotEventRow* rows, uint32_t count);
    void     (*upsert_event)(const BotEventRow* row);
    void     (*delete_event)(uint32_t bot, const char* event);
    void     (*delete_all_events_for_bot)(uint32_t bot);
    void     (*delete_all_events)(void);
    void     (*bump_event_times)(uint32_t delta_seconds);

    /* ── Bot roster / per-bot actions ─────────────────────────────── */
    uint32_t (*owned_bot_guids)(uint32_t** out_ids);
    void     (*free_u32_list)(uint32_t* ids, uint32_t count);
    bool     (*bot_level)(uint32_t guid, uint32_t* out_level);
    bool     (*has_player_bot)(uint32_t guid);

    void     (*dispatch_randomize)(uint32_t guid);
    void     (*dispatch_insta_randomize)(uint32_t guid);
    void     (*dispatch_randomize_first)(uint32_t guid);
    void     (*dispatch_update_gear_spells)(uint32_t guid);
    void     (*dispatch_refresh)(uint32_t guid);
    bool     (*dispatch_random_teleport_for_level)(uint32_t guid, bool active_only);
    bool     (*dispatch_random_teleport_for_rpg)(uint32_t guid, bool active_only);
    void     (*dispatch_random_teleport)(uint32_t guid);
    void     (*dispatch_revive)(uint32_t guid);
    void     (*dispatch_change_strategy)(uint32_t guid);
    void     (*dispatch_remove)(uint32_t guid);
    void     (*dispatch_logout)(uint32_t guid);

    /* ── Teleport cache (`ai_playerbot_tele_cache`) ───────────────── */
    uint32_t (*query_teleport_cache)(BotTeleportRow** out_rows);
    void     (*free_teleport_row_list)(BotTeleportRow* rows, uint32_t count);
    void     (*rebuild_teleport_cache)(void);

    /* ── Named locations ──────────────────────────────────────────── */
    uint32_t (*query_named_locations)(BotNamedLocationRow** out_rows);
    void     (*free_named_location_list)(BotNamedLocationRow* rows, uint32_t count);

    /* ── BG / LFG / AH / real-player ──────────────────────────────── */
    uint32_t (*query_bg_queue)(BotBgQueueEntry** out_rows);
    void     (*free_bg_queue_list)(BotBgQueueEntry* rows, uint32_t count);
    uint32_t (*query_lfg_queue)(BotLfgQueueEntry** out_rows);
    void     (*free_lfg_queue_list)(BotLfgQueueEntry* rows, uint32_t count);
    uint32_t (*query_ah_rows)(BotAhMirrorRow** out_rows);
    void     (*free_ah_row_list)(BotAhMirrorRow* rows, uint32_t count);
    uint32_t (*query_real_player_levels)(BotRealPlayerLevel** out_rows);
    void     (*free_real_player_level_list)(BotRealPlayerLevel* rows, uint32_t count);

    /* ── Per-bot stats snapshot (used by `PrintStats`) ────────────── */
    uint32_t (*query_bot_stats)(BotStatsRow** out_rows);
    void     (*free_bot_stats_list)(BotStatsRow* rows, uint32_t count);

    /* ── Logging sinks ────────────────────────────────────────────── */
    void     (*log_info)(const char* msg);
    void     (*log_error)(const char* msg);
    void     (*log_file)(const char* file, const char* line);
} RandomMgrCallbacks;

/* ── Rust exports (entry points CMaNGOS calls into Rust) ─────────────────── */

/* ── Logging sink ────────────────────────────────────────────────────────── */

/**
 * Log level values passed to the sink. Mirror a four-level scheme that the
 * CMaNGOS `sLog` front-end (outError / outBasic / outString / outDetail) can
 * map directly onto.
 */
#define PLAYERBOT_LOG_ERROR 0
#define PLAYERBOT_LOG_WARN  1
#define PLAYERBOT_LOG_INFO  2
#define PLAYERBOT_LOG_DEBUG 3

/**
 * Function pointer type for the log sink. `msg` is a null-terminated UTF-8
 * C string, valid only for the duration of the call (the Rust side formats
 * into a temporary buffer). The sink must not retain the pointer.
 */
typedef void (*PlayerbotLogSink)(uint8_t level, const char* msg);

/**
 * Install a log sink. Call once from C++ before `playerbot_init` (or at any
 * point after — any Rust log emitted before install is silently dropped).
 * Passing `NULL` detaches the sink.
 */
void playerbot_set_log_sink(PlayerbotLogSink sink);

/**
 * Called once at server startup (before any bots are created).
 */
void playerbot_init(void);

/**
 * Called once at server shutdown (after all bots are destroyed).
 */
void playerbot_shutdown(void);

/* ─────────────────────────────────────────────────────────────────────
 * Bot configuration (`playerbot_config_*`)
 *
 * Phase D of the C++ → Rust migration moved the `.conf` parser from
 * `playerbot/PlayerbotAIConfig.cpp` into the Rust `playerbot` crate.
 * The C++ side keeps a thin data mirror at `cpp_wrapper/BotConfig.{h,cpp}`
 * that calls `playerbot_config_load()` during server startup and then
 * populates every legacy `sPlayerbotAIConfig.X` field via the getters
 * below. Rust hot-path code reads the same state via
 * `crate::config::get()` (lock-free via arc-swap).
 *
 * Call order:
 *   1. playerbot_set_log_sink(...);        // so parse errors are reported
 *   2. playerbot_config_load("path.conf"); // populates the Rust singleton
 *   3. sPlayerbotAIConfig.Initialize();    // compat-shim pulls values over FFI
 *   4. playerbot_init();                   // remaining AI init
 * ───────────────────────────────────────────────────────────────────── */

/** Parse `path` as a CMaNGOS `.conf` file and install the result as the
 *  global Rust config state. Applies `PlayerBots_*` env-var overrides on
 *  top of the file. Returns true on success, false on parse/I-O failure
 *  (the existing state is left untouched on failure). */
bool playerbot_config_load(const char* path);

/** Reset the Rust config singleton to compile-time defaults. Intended
 *  only for test harnesses that run without `playerbot_config_load`. */
void playerbot_config_reset_to_defaults(void);

/* By-key scalar getters. `default` is returned if `key` is null,
 * missing from the parsed state, or fails to parse as the requested
 * type. Matches the old `Config::GetBoolDefault` / `GetIntDefault` /
 * `GetStringDefault` semantics. */
bool     playerbot_config_get_bool(const char* key, bool default_value);
uint32_t playerbot_config_get_u32(const char* key, uint32_t default_value);
int32_t  playerbot_config_get_i32(const char* key, int32_t default_value);
float    playerbot_config_get_f32(const char* key, float default_value);

/** Return an owned C string for `key` (or `default_value`, or the empty
 *  string if both are null). Caller must free via
 *  `playerbot_config_free_cstr`. */
char* playerbot_config_get_string_dup(const char* key, const char* default_value);

/** Free a string returned by any `*_dup` getter. */
void playerbot_config_free_cstr(char* ptr);

/* List getters. Both *_list_dup functions populate `*out_ptr` with a
 * freshly-allocated array and `*out_len` with its length. On an empty
 * result, `*out_ptr` is NULL and `*out_len` is 0. Return false only on
 * invalid parameters (null key / null out-params); a missing key is a
 * successful empty list. Pair with the matching `_free_*` function. */
bool playerbot_config_get_u32_list_dup(const char* key, uint32_t** out_ptr, size_t* out_len);
void playerbot_config_free_u32_list(uint32_t* ptr, size_t len);

bool playerbot_config_get_string_list_dup(const char* key, char*** out_ptr, size_t* out_len);
void playerbot_config_free_string_list(char** ptr, size_t len);

/** Enumerate config keys that contain `needle` as a substring (case-
 *  insensitive, sorted). Used by the shim to port
 *  `ConfigAccess::GetValues("AiPlayerbot.WorldBuff")` and similar
 *  dynamic-key iteration. Free the array with
 *  `playerbot_config_free_string_list`. */
bool playerbot_config_keys_containing(const char* needle, char*** out_ptr, size_t* out_len);

/* Typed accessors for array/map fields. These are pre-derived by the
 * Rust parser (defaults → race-only → class-only → specific overrides),
 * so the C++ shim doesn't re-implement that logic. */
uint32_t playerbot_config_get_class_race_probability(uint32_t cls, uint32_t race);
uint32_t playerbot_config_get_class_race_probability_total(void);
int64_t  playerbot_config_get_fixed_class_race_count(uint32_t cls, uint32_t race);
uint32_t playerbot_config_get_level_probability(uint32_t level);

/* Named typed config getters — replacing string-keyed reads of the
 * deleted `BotConfig` C++ mirror (Phase M). One per field a C++ consumer
 * needs; values are live (reflect `/bot reload`). */
uint32_t playerbot_config_react_delay(void);
uint32_t playerbot_config_max_wait_for_move(void);
int32_t  playerbot_config_level_check(void);
uint32_t playerbot_config_min_random_bot_in_world_time(void);
uint32_t playerbot_config_max_random_bot_in_world_time(void);
bool     playerbot_config_random_bot_timed_logout(void);
bool     playerbot_config_random_bot_timed_offline(void);
bool     playerbot_config_enabled(void);
uint32_t playerbot_config_random_bot_max_level(void);
uint32_t playerbot_config_random_bot_min_level(void);
float    playerbot_config_grind_distance(void);
uint32_t playerbot_config_sync_level_max_above(void);
bool     playerbot_config_sync_level_with_players(void);
uint32_t playerbot_config_randombot_starting_level(void);
bool     playerbot_config_random_gear_upgrade_enabled(void);
uint32_t playerbot_config_random_bot_update_interval(void);
bool     playerbot_config_random_bot_autologin(void);
bool     playerbot_config_disable_random_levels(void);
uint32_t playerbot_config_random_bot_tele_level(void);
float    playerbot_config_random_bot_rpg_chance(void);
float    playerbot_config_random_bot_max_level_chance(void);
uint32_t playerbot_config_min_random_bot_randomize_time(void);
uint32_t playerbot_config_max_random_bot_randomize_time(void);
uint32_t playerbot_config_min_random_bot_revive_time(void);
uint32_t playerbot_config_max_random_bot_revive_time(void);
bool     playerbot_config_random_bot_teleport_near_player(void);
uint32_t playerbot_config_random_bot_teleport_near_player_max_amount(void);
float    playerbot_config_random_bot_teleport_near_player_max_amount_radius(void);
uint32_t playerbot_config_random_bot_teleport_min_interval(void);
uint32_t playerbot_config_random_bot_teleport_max_interval(void);
uint32_t playerbot_config_self_bot_level(void);
uint32_t playerbot_config_bot_autologin(void);
bool     playerbot_config_random_gear_progression(void);
bool     playerbot_config_instant_randomize(void);
bool     playerbot_config_allow_multi_account_alt_bots(void);
bool     playerbot_config_allow_guild_bots(void);
uint32_t playerbot_config_min_enchanting_bot_level(void);
float    playerbot_config_follow_distance(void);
uint32_t playerbot_config_rnd_bot_cheat_mask(void);
uint32_t playerbot_config_bot_cheat_mask(void);
bool     playerbot_config_random_bot_show_helmet(void);
bool     playerbot_config_random_bot_show_cloak(void);
uint32_t playerbot_config_tweak_value(void);
/* String getters — free the result with playerbot_config_free_cstr(). */
char*    playerbot_config_command_separator(void);
char*    playerbot_config_random_bot_maps_as_string(void);
/* Collection getters. */
uint32_t playerbot_config_spec_probability(uint32_t cls, uint32_t idx);
bool     playerbot_config_is_random_bot_map(uint32_t map_id);
size_t   playerbot_config_random_bot_spell_ids_len(void);
uint32_t playerbot_config_random_bot_spell_ids_at(size_t idx);

uint32_t playerbot_config_get_gear_min_item_level(uint32_t phase);
uint32_t playerbot_config_get_gear_max_item_level(uint32_t phase);
int32_t  playerbot_config_get_gear_item(uint32_t phase, uint32_t cls, uint32_t spec, uint32_t slot);

/* World buffs (`AiPlayerbot.WorldBuff.*`): iterate via
 * `playerbot_config_world_buffs_len()` + per-index accessor. */
size_t playerbot_config_world_buffs_len(void);
bool playerbot_config_world_buff_at(size_t idx,
                                    uint32_t* out_spell_id,
                                    uint32_t* out_faction_id,
                                    uint32_t* out_class_id,
                                    uint32_t* out_spec_id,
                                    uint32_t* out_min_level,
                                    uint32_t* out_max_level,
                                    uint32_t* out_event_id);

/* Login criteria (`AiPlayerbot.LoginCriteria*` + default fallback).
 * Each criterion is a comma-separated string list; the `_at` accessor
 * returns the Nth criterion as an owned C string array. */
size_t playerbot_config_login_criteria_len(void);
bool playerbot_config_login_criteria_at(size_t idx, char*** out_ptr, size_t* out_len);
size_t playerbot_config_default_login_criteria_len(void);
bool playerbot_config_default_login_criteria_dup(char*** out_ptr, size_t* out_len);

/**
 * Create AI state for one bot. Called when bot Player logs in.
 * Returns an opaque pointer (BotState*) stored by PlayerbotRust.
 * cbs must remain valid for the lifetime of the returned state.
 */
void* playerbot_create(BotHandle bot, const BotCallbacks* cbs);

/**
 * Destroy AI state for one bot. Called when bot Player logs out.
 * After this call, the state pointer is invalid.
 */
void playerbot_destroy(void* state);

/**
 * Set (or clear) the bot's master — the player that commands this bot.
 * `guid = 0` clears the master.
 *
 * Called whenever `PlayerbotRust::SetMaster` runs: explicit assignment via
 * PlayerbotMgr, per-tick master auto-claim (PB2 DoNextAction parity), or
 * group-disband cleanup. Mirrors PB2's `PlayerbotAI::m_master`.
 */
void playerbot_set_master(void* state, uint64_t guid);

/**
 * Read the current master guid (0 = no master). Mostly used for asserts
 * and diagnostics on the C++ side; the authoritative value for security
 * checks is still `PlayerbotRust::m_masterGuid`.
 */
uint64_t playerbot_get_master(const void* state);

/**
 * Drop all per-bot strategy/cache state (pending commands, blackboard,
 * cooldown throttles, encounter FSM). Called when the master changes or
 * when a full reinit is requested. Equivalent to PB2's
 * `PlayerbotAI::ResetStrategies`.
 */
void playerbot_reset_strategies(void* state);

/**
 * Main AI tick. Called from Player::UpdateAI on the map worker thread.
 * elapsed_ms: milliseconds since last call.
 * minimal: true when bot activity is throttled (e.g. empty zone, server load).
 */
void playerbot_update(void* state, uint32_t elapsed_ms, bool minimal);

/**
 * Incoming network packet (master player → server).
 */
void playerbot_packet_in(void* state, uint16_t opcode, const uint8_t* data, uint32_t len);

/**
 * Outgoing network packet (server → master player).
 */
void playerbot_packet_out(void* state, uint16_t opcode, const uint8_t* data, uint32_t len);

/* ── Push combat events ──────────────────────────────────────────────────── */

void playerbot_unit_spell_cast(void* state, UnitHandle caster, uint32_t spell_id,
                               UnitHandle target, bool success);

void playerbot_aura_changed(void* state, UnitHandle unit, uint32_t spell_id,
                            bool applied, uint8_t stacks);

void playerbot_unit_died(void* state, UnitHandle victim, UnitHandle killer);

void playerbot_damage_taken(void* state, uint32_t damage, uint32_t spell_id,
                            UnitHandle dealer);

/* ── Chat command injection ──────────────────────────────────────────────── */

/**
 * Chat-command security tiers. Mirrors PB2's PlayerbotSecurityLevel with
 * GUILD collapsed into TALK. Each BotCommand in Rust declares the minimum
 * level required to execute it; the dispatcher drops commands below that.
 *
 *   DENY_ALL (0)  — opposite faction / blocked; nothing runs.
 *   TALK     (1)  — strangers / same guild; info queries only.
 *   INVITE   (2)  — group members; behaviour/targeting/movement commands.
 *   ALLOW_ALL (3) — master, same account, or GM; destructive commands.
 */
#define PLAYERBOT_SEC_DENY_ALL  0
#define PLAYERBOT_SEC_TALK      1
#define PLAYERBOT_SEC_INVITE    2
#define PLAYERBOT_SEC_ALLOW_ALL 3

/**
 * Inject a chat command into a bot's pending command queue.
 * Called when a player whispers a command to the bot.
 *
 *   sender_guid  — ObjectGuid raw value of the player issuing the command.
 *                  0 means "internal/system/console" and bypasses gating.
 *   security     — PLAYERBOT_SEC_* tier the sender was granted by the
 *                  C++-side security check.
 *   chat_type    — CMaNGOS `ChatMsg` enum value (CHAT_MSG_WHISPER,
 *                  CHAT_MSG_SAY, CHAT_MSG_YELL, CHAT_MSG_PARTY,
 *                  CHAT_MSG_RAID*, CHAT_MSG_GUILD, CHAT_MSG_CHANNEL, …).
 *                  Needed by the Rust side to choose the correct reply
 *                  channel (whisper vs. CHAT_MSG_ADDON/LANG_ADDON for the
 *                  Mangosbot `#a`/`debug` reply flow).
 *   lang         — CMaNGOS `Language` value. `LANG_ADDON` (0xFFFFFFFE)
 *                  signals an addon-channel payload; replies to such
 *                  commands must go back on LANG_ADDON.
 *   text         — null-terminated command text.
 */
void playerbot_chat_command(void* state, uint64_t sender_guid,
                            uint8_t security, uint32_t chat_type,
                            uint32_t lang, const char* text);

/* ── RTSC (Real-Time Strategy Control) ───────────────────────────────────── */

/**
 * RTSC spell position — called when spell 30758 is cast on the ground by
 * the bot's master. The C++ side extracts the destination position from
 * SpellCastTargets and passes it here. The Rust side applies the pending
 * RTSC action (move, save waypoint, etc.).
 */
void playerbot_rtsc_spell(void* state, float x, float y, float z);

/* ── Global coordination ─────────────────────────────────────────────────── */

/**
 * Global tick — called from sRandomPlayerbotMgr.UpdateAI (world thread).
 */
void playerbot_world_update(uint32_t elapsed_ms);

/* ── Factory entry points (called from PlayerbotFactory C++) ─────────────── */

/**
 * Clear bot inventory via the Rust factory module.
 *
 *   state — pointer returned by playerbot_create for this bot
 *   mode  — 0 = equipped + carried bags; 1 = equipped + bags + bank (everything)
 */
void playerbot_factory_clear_inventory(void* state, uint8_t mode);

/**
 * Initialize consumables for a bot via the Rust factory module.
 *
 *   kind — 0 = potions, 1 = food, 2 = reagents
 */
void playerbot_factory_init_consumables(void* state, uint8_t kind);

/**
 * Wipe bot progression via the Rust factory module.
 *
 *   kind — 0 = trade skills, 1 = spells, 2 = quests
 */
void playerbot_factory_reset_progression(void* state, uint8_t kind);

/**
 * Miscellaneous factory step via the Rust factory module.
 *
 *   kind — 0 = cancel auras, 1 = init skill-tool starter kit,
 *          2 = init mounts (learn race- and level-appropriate mount spells),
 *          3 = init bags (equip starter bags into empty bag slots),
 *          4 = init reputations (grant honored standing with PvP & end-game factions),
 *          5 = init ammo (top up ranged-weapon ammo for warrior/rogue/hunter),
 *          6 = init inventory trade (stock one random trade good),
 *          7 = init skills (armor/weapon/riding proficiencies),
 *          8 = init special spells (teach config-listed spell IDs),
 *          9 = init taxi nodes (flag level-appropriate overworld flight paths),
 *         10 = init available spells (teach default + class-level spellbook)
 */
void playerbot_factory_misc(void* state, uint8_t kind);

/**
 * Learn talents for the given spec tab (0..2 = three talent trees). Walks the
 * class's talent rows for the requested tab and randomly invests points until
 * the bot has spent its free-talent-points budget, matching the policy of the
 * old `PlayerbotFactory::InitTalents`.
 */
void playerbot_factory_init_talents(void* state, uint32_t spec_no);

/**
 * Pick (or recall) a talent spec and spend the bot's talent points across
 * it, matching the policy of the old `PlayerbotFactory::InitTalentsTree`.
 * `incremental` mirrors the original flag: when true, the bot keeps its
 * previously-rolled spec if one was stored; when false, a fresh spec is
 * chosen against the config probability table.
 */
void playerbot_factory_init_talents_tree(void* state, bool incremental);

/**
 * Top up consumables on a bot via the Rust factory module. Ports
 * `PlayerbotFactory::Refresh`: gated on the bot's item-cheat bit, then
 * re-runs Init{Ammo,Food,Potions,Reagents} and AddConsumables, finishing
 * with a SaveToDB when the character DB queue is idle.
 */
void playerbot_factory_refresh(void* state);

/**
 * Reset the bot chassis before a factory re-roll. Ports
 * `PlayerbotFactory::Prepare`: resurrect + combat-stop, set level / reset XP
 * (unless `disableRandomLevels`), apply the helmet / cloak hide flags per
 * config. `level` is the target level the caller wants.
 */
void playerbot_factory_prepare(void* state, uint32_t level);

/**
 * Reward every quest in the caller-supplied list that the bot is eligible
 * for. Ports `PlayerbotFactory::InitQuests(std::list<uint32>&)`: each id is
 * filtered by the combined class/race/min-level predicate, and on success
 * the quest is marked complete and its reward is handed out.
 *
 *   ids — pointer to `len` contiguous u32 quest ids (may be null when len=0)
 *   len — number of quest ids pointed to by `ids`
 */
void playerbot_factory_init_quests(void* state, const uint32_t* ids, size_t len);

/**
 * Port of `PlayerbotFactory::InitArenaTeam`. Gates the call on two
 * conditions: (a) the bot's owning account is in the live
 * `random_bot_accounts` tracking list, and (b) the current tracked
 * arena-team count is below `AiPlayerbot.RandomBotArenaTeamCount`. When
 * both pass the Rust side forwards to
 * `playerbot_random_factory_create_arena_teams`. On Classic the entry
 * point is a silent no-op because arena teams do not exist.
 */
void playerbot_factory_init_arena_team(void* state);

/**
 * Port of `PlayerbotFactory::InitGuild`. Stocks a guild tabard if the
 * bot is already in a guild; otherwise filters the live
 * `random_bot_guilds` list (owned by the Rust random-factory singleton)
 * to guilds whose leader is on the bot's team, picks one at random,
 * checks the current member count against the `atoi(GetGINFO())` hint
 * (fallback to `urand(10,15)`), and joins the guild at a random rank
 * between GR_OFFICER and GR_INITIATE. Stocks a guild tabard on join
 * when the bot is above level 9 and passes a 4/5 probability roll.
 * Silent no-op when the random-factory module has not been initialised
 * or the filtered guild list is empty.
 */
void playerbot_factory_init_guild(void* state);

/**
 * Port of `PlayerbotFactory::InitAllSkills`. Runs `InitSkills`
 * (weapons/armor/riding) followed by `InitTradeSkills` (two professions
 * + first aid + fishing + cooking + the TBC+ armorsmith spell chain for
 * plate classes + the opaque trainer-iteration loop). The profession
 * pair is cached in the per-bot KV store under `firstSkill`/`secondSkill`
 * so re-rolls hand out the same pair.
 */
void playerbot_factory_init_all_skills(void* state);

/**
 * Port of `PlayerbotFactory::InitEquipment`. Drives the per-slot re-roll
 * loop that ports the equip-cache queries, stat-weight comparisons,
 * filter chain (locked legendaries, unavailable id blacklist, config
 * blacklist, quality/level caps, caster/tank weapon gates), random
 * enchant selection, off-hand-vs-main-hand ladder check, and equip +
 * `EnchantItem` tail. Bit-for-bit parity with the C++ source: same
 * quality descent, same shuffle order, same logging points.
 *
 * Modes (flag bits):
 *   * bit 0 = incremental — keep existing items unless the new roll
 *             scores strictly higher (legacy `incremental` arg).
 *   * bit 1 = sync with master — clamp the item-level ceiling to the
 *             master's gear score + config diff, skip progression
 *             scaling, and broadcast a summary message to the master
 *             on completion (legacy `syncWithMaster` arg).
 *   * bit 2 = progressive — rolling quality ladder keyed on bot level
 *             (legacy `progressive` arg).
 *   * bit 3 = partial upgrade — only touch a 1..=4 subset of slots
 *             chosen from empty slots + worst-scored worn items
 *             (legacy `partialUpgrade` arg).
 *   * `item_quality` — when non-zero, pins the quality tier searched in
 *             each slot (legacy `itemQuality` member of the command
 *             struct — used by `gear` chat commands). `0` means
 *             "follow the progressive/incremental ladder".
 */
void playerbot_factory_init_equipment(void* state,
                                       uint32_t flags,
                                       uint32_t item_quality);

/**
 * Port of `PlayerbotFactory::InitPet`. Hunter-only: walks the tameable
 * creature list, picks one at random (up to 100 retries), and calls the
 * atomic `factory_create_hunter_pet` callback to run the full CMaNGOS
 * creation sequence. Finishes by refreshing pet stats, mass-toggling
 * autocast on every non-passive pet spell, and force-dismissing the pet
 * to clear the missing-flags bug from the legacy C++ code. Every other
 * class is a no-op.
 */
void playerbot_factory_init_pet(void* state);

/**
 * Port of `PlayerbotFactory::InitPetSpells`. Per-class / per-expansion
 * pet spell learning: vanilla hunter dispatches on `CreatureInfo::Family`
 * to a per-pet-type spell table and additionally learns rank-appropriate
 * Growl / Natural Armor / Great Stamina plus resistances at pet level
 * ≥ 20. Vanilla+TBC warlock dispatches on the pet's creature template
 * entry and learns the level-gated spell list. Hunter + warlock only.
 */
void playerbot_factory_init_pet_spells(void* state);

/**
 * Port of `PlayerbotFactory::InitTradeSkills`. Runs only the tradeskill
 * half of `InitAllSkills` — profession assignment, first aid / fishing /
 * cooking / the two professions, the TBC+ armorsmith spell chain, and
 * the opaque trainer-iteration loop — skipping the weapons / armor /
 * riding pass of `InitSkills`.
 */
void playerbot_factory_init_trade_skills(void* state);

/**
 * Port of `PlayerbotFactory::Randomize(bool incremental, bool syncWithMaster)`.
 * Top-level "re-roll this bot from scratch" pass — chains Prepare + every
 * other Init* step behind the two random-bot predicates (isRealRandomBot,
 * isRandomBot) and saves to DB at the tail. `level` is the target level the
 * caller wants; `item_quality` is the `.py equip <quality>` override
 * (0 = follow the progressive ladder).
 */
void playerbot_factory_randomize(void* state,
                                 uint32_t level,
                                 bool     incremental,
                                 bool     sync_with_master,
                                 uint32_t item_quality);

/**
 * Port of `PlayerbotFactory::InitAmmo`. Top up ranged-weapon ammo for
 * warrior / rogue / hunter bots. Reads the bot's class and level from a
 * snapshot on the Rust side.
 */
void playerbot_factory_init_ammo(void* state);

/**
 * Port of `PlayerbotFactory::EnchantEquipment`. Applies per-slot enchants
 * to every equipped item. The full body (enchant-container load, spec-tab
 * dispatch, per-slot iteration) lives on the C++ bridge side because it
 * still touches live `Item*` pointers — this is a thin wrapper that
 * forwards into `factory_enchant_all_equipment`.
 */
void playerbot_factory_enchant_equipment(void* state);

/**
 * Port of `PlayerbotFactory::InitGems`. Sockets a random gem into every
 * empty socket on the bot's equipment. The full body (socket iteration,
 * random-item-manager gem list, stat-weight check, `CMSG_SOCKET_GEMS`
 * dispatch) lives on the C++ bridge side. Classic is a compile-time
 * no-op because the game has no gems pre-TBC.
 */
void playerbot_factory_init_gems(void* state);

/**
 * Toggle the per-bot debug monitor. Returns true if monitoring is now ON,
 * false if OFF. When toggled on, the Rust side dumps a full settings snapshot
 * to the monitor log file. The log file is written via bot_append_log_file.
 */
bool playerbot_toggle_monitor(void* state);

/* ── Manager command dispatch ─────────────────────────────────────────── */

/**
 * Try to handle a `.bot <name> <cmd> [param]` command in Rust.
 *
 * Returns a heap-allocated C string if the command was handled (caller must
 * free with playerbot_free_string), or NULL if the C++ side should process
 * the command.
 */
char* playerbot_mgr_bot_command(void* state,
                                const char* cmd,
                                const char* param,
                                uint32_t master_level);

/** Free a string returned by playerbot_mgr_bot_command. NULL-safe. */
void playerbot_free_string(char* ptr);

/* ─────────────────────────────────────────────────────────────────────
 * Login queue (`playerbot_login_*`)
 *
 * Phase E of the C++ → Rust migration moved the bot login/logout queue
 * from `playerbot/PlayerbotLoginMgr.cpp` into the Rust `playerbot`
 * crate's `login` module. The C++ side keeps only a thin shim at
 * `cpp_wrapper/LoginBridge.{h,cpp}` that implements `LoginCallbacks`
 * and forwards the few calls left to `RandomPlayerbotMgr` / the DB.
 *
 * Call order:
 *   1. playerbot_config_load(...);            // file parser
 *   2. playerbot_login_init(&login_cbs);      // spawns worker thread
 *   3. playerbot_login_update(real_players)   // every RPBM update tick
 *   4. playerbot_login_shutdown();            // joined on server stop
 * ───────────────────────────────────────────────────────────────────── */

/** Install the login vtable and spawn the background worker thread.
 *  Must be called exactly once, after `playerbot_config_load`. Passing
 *  a null pointer is a no-op. */
void playerbot_login_init(const LoginCallbacks* cbs);

/** Main tick. Called from `RandomPlayerbotMgr::UpdateAIInternal` on
 *  the world thread. `real_players` describes every non-bot player
 *  currently online and is used to evaluate distance / map / group /
 *  guild gates. The worker thread asynchronously drives the DB side;
 *  this call drains any pending `perform_login`/`perform_logout`
 *  commands on the main thread. */
void playerbot_login_update(const BotRealPlayerInfo* real_players, uint32_t count);

/** Flip the login worker's debug logging state. Mirrors the legacy
 *  `PlayerBotLoginMgr::ToggleDebug`. */
void playerbot_login_toggle_debug(void);

/** Join the background worker thread and detach the vtable. Safe to
 *  call more than once; second call is a no-op. */
void playerbot_login_shutdown(void);

/* ─────────────────────────────────────────────────────────────────────
 * Item pool (`playerbot_itempool_*`)
 *
 * Phase F of the C++ → Rust migration moved the full `RandomItemMgr`
 * (item info cache, equip pool, consumable pools, stat-weight scoring,
 * random-enchant resolution, rarity cache, gem list) from
 * `playerbot/RandomItemMgr.cpp` into the Rust `playerbot` crate's
 * `itempool` module. The C++ side keeps a thin shim at
 * `cpp_wrapper/ItemBridge.{h,cpp}` that implements `ItemCallbacks` and
 * re-exposes a `sRandomItemMgr`-compatible façade backed by these
 * functions so that `PlayerbotFactory.cpp` and `ahbot/PricingStrategy.cpp`
 * do not need to change.
 *
 * Call order:
 *   1. playerbot_config_load(...);             // file parser (Phase D)
 *   2. playerbot_itempool_init(&item_cbs);     // snapshots every table
 *   3. playerbot_login_init(&login_cbs);       // login queue (Phase E)
 *   4. factory/ahbot use `playerbot_itempool_*`
 *   5. playerbot_itempool_shutdown();          // drops the cache
 * ───────────────────────────────────────────────────────────────────── */

/** Install the item vtable and build every internal cache. Reads all
 *  prototypes, quest→item maps, vendor→item maps, weight scales, gem
 *  properties, random-enchant template, spell lookups, and rarity
 *  values via the supplied `ItemCallbacks`. Must be called exactly
 *  once, after `playerbot_config_load`. Passing a null pointer is a
 *  no-op (the module stays uninitialized and every query returns
 *  zeroed/empty results). */
void playerbot_itempool_init(const ItemCallbacks* cbs);

/** Drop every cache and detach the vtable. Safe to call more than
 *  once; the second call is a no-op. */
void playerbot_itempool_shutdown(void);

/* ── Item metadata (used by PlayerbotFactory) ────────────────────── */

/** Return the minimum required level derived during
 *  `BuildItemInfoCache`. Returns 0 for items not in the cache. */
uint32_t playerbot_itempool_get_min_level(uint32_t item_id);

/** Return the item rarity used by ahbot pricing. Falls back to a
 *  computed heuristic if the `ai_playerbot_rarity_cache` table was
 *  empty. Returns 0.0 for items not in the store. */
float playerbot_itempool_get_item_rarity(uint32_t item_id);

/** Resolve the player's spec id (0..n) using the same policy as the
 *  legacy `GetPlayerSpecId`: best talent tab plus class-specific
 *  tie-breakers (druid feral tank/dps 50/50, priest disc/holy via
 *  Meditation rank, hunter MM/surv by rank, warrior arms/fury by
 *  two-hand count — all via the `urand_range` / `get_player_talent_tab`
 *  callbacks). Returns 0 when the guid is not a logged-in player. */
uint32_t playerbot_itempool_get_player_spec_id(uint32_t guid_low);

/* ── Pool queries ────────────────────────────────────────────────── */

/** Query the equip pool. Fills `*out_ptr` with a freshly allocated
 *  array of item ids matching `(level, class, spec, slot, quality)`
 *  and `*out_len` with its length. Empty result is `(NULL, 0)` and
 *  still returns `true`. Returns `false` only on invalid out-params.
 *  Caller frees via `playerbot_itempool_free_u32_list`. */
bool playerbot_itempool_query(uint32_t level,
                              uint8_t  class_id,
                              uint8_t  spec_id,
                              uint8_t  slot,
                              uint32_t quality,
                              uint32_t** out_ptr,
                              size_t*    out_len);

/** Free an array returned by `playerbot_itempool_query` (or any other
 *  `_get_*_list` helper that documents this function as its free
 *  path). A `(NULL, 0)` pair is a safe no-op. */
void playerbot_itempool_free_u32_list(uint32_t* ptr, size_t len);

/** Return the full gem list (one item id per gem known to the
 *  item-info cache). Caller frees via `playerbot_itempool_free_u32_list`.
 *  Empty on Classic. */
bool playerbot_itempool_get_gems(uint32_t** out_ptr, size_t* out_len);

/* ── Stat weight scoring ─────────────────────────────────────────── */

/** Equivalent to `RandomItemMgr::GetStatWeight(item_id, spec_id)`.
 *  Returns the pre-computed base stat weight for the given (item,
 *  spec) pair or 0 if the item is not in the info cache. */
uint32_t playerbot_itempool_get_stat_weight(uint32_t item_id, uint32_t spec_id);

/** Equivalent to `RandomItemMgr::GetLiveStatWeight(bot, item_id,
 *  spec_id)`. Runs the full runtime scoring pass on the current
 *  prototype using the player's level, talent tab, and
 *  situational modifiers, combining the base weight with the stat
 *  contributions derived from `ItemPrototype::ItemStat[]`,
 *  weapon DPS, and `ItemPrototype::Spells[]`. Returns 0 when the guid
 *  is not a logged-in player. */
uint32_t playerbot_itempool_get_live_stat_weight(uint32_t guid_low,
                                                 uint32_t item_id,
                                                 uint32_t spec_id);

/** Equivalent to `RandomItemMgr::GetBestRandomEnchantStatWeight`.
 *  Returns the best possible random-enchant weight the item could
 *  roll for the given spec. */
uint32_t playerbot_itempool_get_best_random_enchant_stat_weight(uint32_t item_id,
                                                                uint32_t spec_id);

/* ── Enchants / quest disambiguation ─────────────────────────────── */

/** Equivalent to `RandomItemMgr::CalculateBestRandomEnchantId`.
 *  Picks the highest-weight random enchant id for the given
 *  (class, spec, item) combination; 0 = no viable enchant. */
uint32_t playerbot_itempool_calculate_best_random_enchant_id(uint8_t  class_id,
                                                             uint8_t  spec_id,
                                                             uint32_t item_id);

/** Equivalent to `RandomItemMgr::CalculateEnchantWeight`. */
uint32_t playerbot_itempool_calculate_enchant_weight(uint8_t  class_id,
                                                     uint8_t  spec_id,
                                                     uint32_t enchant_id);

/** Equivalent to `RandomItemMgr::HasSameQuestRewards(bot, item_id)`.
 *  Returns true if the bot has completed (or is currently holding) a
 *  quest whose reward pool contains `item_id`. Used to skip duplicate
 *  quest rewards during factory gearing. */
bool playerbot_itempool_has_same_quest_rewards(uint32_t guid_low, uint32_t item_id);

/* ── Consumable pools (replace BotCallbacks::factory_pick_*) ─────── */

/** Equivalent to `RandomItemMgr::GetAmmo(level, ammo_subclass)`.
 *  `ammo_subclass` matches the weapon's `ammo_type` field (arrows,
 *  bullets, thrown). Returns 0 on no match. */
uint32_t playerbot_itempool_get_ammo(uint32_t level, uint32_t ammo_subclass);

/** Equivalent to `RandomItemMgr::GetRandomPotion(level, effect)`.
 *  `effect` matches the potion family (heal, mana, etc.) as used by
 *  the potion cache. Returns 0 on no match. */
uint32_t playerbot_itempool_get_random_potion(uint32_t level, uint32_t effect);

/** Equivalent to `RandomItemMgr::GetFood(level, category)`. */
uint32_t playerbot_itempool_get_food(uint32_t level, uint32_t category);

/** Equivalent to `RandomItemMgr::GetRandomFood(level, category)`. */
uint32_t playerbot_itempool_get_random_food(uint32_t level, uint32_t category);

/** Equivalent to `RandomItemMgr::GetRandomTrade(level)`. */
uint32_t playerbot_itempool_get_random_trade(uint32_t level);

/* ─────────────────────────────────────────────────────────────────────
 * Random factory (`playerbot_random_factory_*`)
 *
 * Phase G of the C++ → Rust migration moved the full
 * `RandomPlayerbotFactory` class (race/class availability, probability-
 * weighted selection, name-pool expansion, delete/create orchestration,
 * guild + arena-team lifecycle) from `playerbot/RandomPlayerbotFactory.cpp`
 * into the Rust `playerbot` crate's `random` module. The C++ side keeps
 * a thin shim at `cpp_wrapper/RandomFactoryBridge.{h,cpp}` that
 * implements `RandomFactoryCallbacks` and forwards the few DB / game-
 * object calls still needed to CMaNGOS. The legacy C++ class is gone —
 * `RandomPlayerbotMgr` calls `playerbot_random_factory_*` directly.
 *
 * Call order:
 *   1. playerbot_config_load(...);                    // file parser
 *   2. playerbot_random_factory_init(&factory_cbs);   // installs vtable
 *   3. playerbot_random_factory_create_bots();        // from RPBM::Init
 *   4. playerbot_random_factory_create_guilds();      // from RPBM::Init
 *   5. playerbot_random_factory_create_arena_teams(); // wotlk only
 *   6. playerbot_random_factory_shutdown();           // server stop
 * ───────────────────────────────────────────────────────────────────── */

/** Install the random-factory vtable. Must be called exactly once,
 *  after `playerbot_config_load`. Passing a null pointer is a no-op
 *  (the module stays uninitialised and every subsequent call returns
 *  without touching the game state). */
void playerbot_random_factory_init(const RandomFactoryCallbacks* cbs);

/** Drop the vtable. Safe to call more than once; second call is a
 *  no-op. */
void playerbot_random_factory_shutdown(void);

/** Port of `RandomPlayerbotFactory::CreateRandomBots()`. Scans the
 *  existing random-bot accounts, optionally runs the delete pass,
 *  creates any missing accounts, and fills every account with random-
 *  bot characters using the expansion-appropriate class/race probability
 *  tables. Runs synchronously on the calling thread; returns when all
 *  characters have been created and their `WorldSession`s have been
 *  torn down. */
void playerbot_random_factory_create_bots(void);

/** Port of `RandomPlayerbotFactory::CreateRandomGuilds()`. Walks the
 *  current random-bot character list, optionally disbands every
 *  existing random-bot guild, and populates the configured number of
 *  new random-bot guilds. */
void playerbot_random_factory_create_guilds(void);

/** Port of `RandomPlayerbotFactory::CreateRandomArenaTeams()`. TBC/WotLK
 *  only — on Classic the implementation is a no-op. Builds random-bot
 *  2v2/3v3/5v5 arena teams with a 40%/30%/30% split, optionally
 *  disbanding the existing ones first. */
void playerbot_random_factory_create_arena_teams(void);

/** Query the current `random_bot_accounts` runtime list — the ids of
 *  every account the factory tracked during the most recent
 *  `create_bots` pass. Copies into a caller-provided buffer up to
 *  `max_out` ids and returns the total number of ids. Used by
 *  `PlayerbotAIConfig::loadFreeAltBotAccounts` during shim fill-up. */
uint32_t playerbot_random_factory_get_accounts(uint32_t* out_ids, uint32_t max_out);

/** Query the current `random_bot_guilds` runtime list — the ids of
 *  every random-bot guild the factory tracked during the most recent
 *  `create_guilds` pass. Same shape as `get_accounts`. */
uint32_t playerbot_random_factory_get_guilds(uint32_t* out_ids, uint32_t max_out);

/** Query the current `random_bot_arena_teams` runtime list. WotLK/TBC
 *  only; on Classic always returns 0. */
uint32_t playerbot_random_factory_get_arena_teams(uint32_t* out_ids, uint32_t max_out);

/* ─────────────────────────────────────────────────────────────────────
 * Random-playerbot manager (`playerbot_random_mgr_*`)
 *
 * Phase H of the C++ → Rust migration moved `playerbot/RandomPlayerbotMgr.cpp`
 * (≈4100 LOC) into the Rust `playerbot` crate's `random_mgr` module.
 * The C++ side keeps a thin shim at `cpp_wrapper/RandomMgrBridge.{h,cpp}`
 * that implements `RandomMgrCallbacks` and re-exposes the legacy
 * `sRandomPlayerbotMgr` façade so `PlayerbotHolder`-based core hooks
 * still have a singleton to call.
 *
 * Call order:
 *   1. playerbot_config_load(...);                // file parser
 *   2. playerbot_itempool_init(&item_cbs);        // phase F
 *   3. playerbot_login_init(&login_cbs);          // phase E
 *   4. playerbot_random_factory_init(&factory_cbs); // phase G
 *   5. playerbot_random_mgr_init(&random_mgr_cbs);  // phase H
 *   6. playerbot_random_mgr_update(elapsed_ms)      // every world tick
 *   7. playerbot_random_mgr_shutdown();           // server stop
 * ───────────────────────────────────────────────────────────────────── */

/** Install the random-mgr vtable and spawn its background worker.
 *  Must be called exactly once, after `playerbot_random_factory_init`.
 *  Passing a null pointer is a no-op. */
void playerbot_random_mgr_init(const RandomMgrCallbacks* cbs);

/** Main tick. Called from the same world-update hook that used to drive
 *  `RandomPlayerbotMgr::UpdateAIInternal`. Forwards the elapsed time to
 *  the worker and drains any pending command output on the main thread. */
void playerbot_random_mgr_update(uint32_t elapsed_ms);

/** `RandomPlayerbotMgr::GetValue(guid, key)` — thin FFI passthrough
 *  that reads the in-memory event cache the worker maintains. Safe to
 *  call from any thread; returns 0 when the bot / event pair is
 *  unknown. */
uint32_t playerbot_random_mgr_get_value(uint32_t guid, const char* key);

/** `RandomPlayerbotMgr::SetValue(guid, key, value, validIn)` — thin
 *  FFI passthrough that updates both the in-memory cache and the DB
 *  via the worker. `valid_in_s == -1` means "use the configured
 *  default TTL for this event key". */
void playerbot_random_mgr_set_value(uint32_t guid,
                                    const char* key,
                                    uint32_t value,
                                    int32_t valid_in_s);

/** `RandomPlayerbotMgr::SetEventValue(guid, key, value, validIn, data)` —
 *  faithful low-level event write. `valid_in_s` is the raw seconds TTL
 *  with NO `-1` sentinel: 0 means "no TTL", `0xFFFFFFFF` (the C++
 *  `(uint32)-1`) means "effectively never expires". `data` may be "". */
void playerbot_random_mgr_set_event_value(uint32_t guid,
                                          const char* key,
                                          uint32_t value,
                                          uint32_t valid_in_s,
                                          const char* data);

/** `RandomPlayerbotMgr::Remove(bot)` cache half — drop every cached event
 *  row for `guid` (DB delete is issued by the C++ caller). */
void playerbot_random_mgr_drop_bot_events(uint32_t guid);

/** `RandomPlayerbotMgr::HandleConsoleReset` cache half — clear the entire
 *  event cache (DB wipe is issued by the C++ caller). */
void playerbot_random_mgr_clear_event_cache(void);

/** Current average level of the online real players. Mirrors the C++
 *  `sRandomPlayerbotMgr.GetPlayersLevel()`. */
uint32_t playerbot_random_mgr_get_players_level(void);

/** Current target bot population — the `bot_count` event. */
uint32_t playerbot_random_mgr_get_max_online_bot_count(void);

/** `sWorld.getConfig(CONFIG_UINT32_MAX_PLAYER_LEVEL)` cache. Bridged
 *  here so `LoginBridge.cpp` can forward its own `random_mgr_*`
 *  callbacks into Rust instead of keeping state in two places. */
uint32_t playerbot_random_mgr_get_world_max_level(void);

/** Last observed CharacterDatabase round-trip in milliseconds. */
uint32_t playerbot_random_mgr_get_database_delay_ms(void);

/** Issue an async DB ping so the next
 *  `playerbot_random_mgr_get_database_delay_ms` call sees a fresh
 *  value. */
void playerbot_random_mgr_database_ping(void);

/** True iff the given guid is a random bot (present in the manager's
 *  `currentBots` list). */
bool playerbot_random_mgr_is_random_bot(uint32_t guid);

/** Schedule a per-bot `Randomize` in `delay_s` seconds. */
void playerbot_random_mgr_schedule_randomize(uint32_t guid, uint32_t delay_s);

/** Schedule a per-bot teleport in `delay_s` seconds. */
void playerbot_random_mgr_schedule_teleport(uint32_t guid, uint32_t delay_s);

/** Schedule a per-bot strategy change in `delay_s` seconds. */
void playerbot_random_mgr_schedule_change_strategy(uint32_t guid, uint32_t delay_s);

/** Dispatch a `rndbot ...` console / per-bot command. `text` is the full
 *  argument string (command name + params). Returns the newline-joined
 *  output lines as a malloc'd C string (free with `playerbot_free_string`),
 *  or NULL when the command is unrecognised — in which case the caller
 *  falls through to the holder command handler. `*out_flags` receives
 *  PLAYERBOT_CONSOLE_FLAG_* bits for side effects the C++ caller must
 *  perform. Used by `HandlePlayerbotConsoleCommand`. */
#define PLAYERBOT_CONSOLE_FLAG_UPDATE_TICK 0x1u
#define PLAYERBOT_CONSOLE_FLAG_CLEAN_MAP   0x2u
#define PLAYERBOT_CONSOLE_FLAG_LOGIN_DEBUG 0x4u
char* playerbot_random_mgr_console_command(const char* text, uint32_t* out_flags);

/** Print the running stats block into the log sink (mirrors the legacy
 *  `PrintStats` call). */
void playerbot_random_mgr_print_stats(uint32_t requester_guid);

/** Join the worker, flush pending DB writes, and drop the vtable.
 *  Safe to call more than once. */
void playerbot_random_mgr_shutdown(void);

#ifdef __cplusplus
} /* extern "C" */
#endif
