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
    bool     in_combat;
    bool     is_casting;
    bool     is_moving;
    bool     is_channeling;
    uint32_t casting_spell_id;  /* 0 if not casting */
    float    casting_progress;  /* 0.0–1.0, only valid when is_casting */
    UnitHandle current_target;  /* 0 = no target */
    uint32_t  aura_state_mask;  /* CMaNGOS AuraState bitmask for quick checks */
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

typedef struct {
    UnitHandle unit;
    uint32_t   spell_id;        /* debuff spell ID */
    bool       found;           /* false = no dispellable target */
} BotDispelTarget;

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

    /* ── Pathfinding / positioning ───────────────────────────────────── */
    BotPosition     (*get_behind_position)(BotHandle bot, UnitHandle target, float distance);
    BotSafePosition (*get_safe_position)(BotHandle bot, float search_radius);
    BotPosition     (*get_spread_position)(BotHandle bot, UnitHandle center, float radius,
                                           uint8_t idx, uint8_t total);
    bool            (*can_reach)(BotHandle bot, float x, float y, float z);

    /* ── Bot commands (all return true on success) ───────────────────── */
    bool (*cast_spell)(BotHandle bot, uint32_t spell_id, UnitHandle target);
    bool (*cast_spell_pos)(BotHandle bot, uint32_t spell_id, float x, float y, float z);
    bool (*move_to)(BotHandle bot, float x, float y, float z);
    bool (*follow)(BotHandle bot, UnitHandle target, float dist, float angle);
    bool (*stop_moving)(BotHandle bot);
    bool (*attack)(BotHandle bot, UnitHandle target);
    bool (*auto_attack)(BotHandle bot, bool enable);
    bool (*say)(BotHandle bot, const char* msg, uint32_t lang);
    bool (*whisper)(BotHandle bot, uint64_t target_guid, const char* msg);
    /* PB2-style TellPlayerNoFacing routing: broadcasts to the bot's party/raid
     * when it has a group, otherwise whispers the sender. target_guid is the
     * requester's ObjectGuid (used only for the whisper fallback). */
    bool (*tell_player)(BotHandle bot, uint64_t target_guid, const char* msg);
    bool (*use_item)(BotHandle bot, uint32_t item_id, UnitHandle target);
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

    /* ── Pet management ──────────────────────────────────────────────── */
    bool    (*has_pet)(BotHandle bot);
    bool    (*pet_is_alive)(BotHandle bot);
    uint8_t (*pet_happiness)(BotHandle bot);
    bool    (*summon_pet)(BotHandle bot);
    bool    (*revive_pet)(BotHandle bot);
    bool    (*feed_pet)(BotHandle bot);

    /* ── Dispel / party queries ──────────────────────────────────────── */
    BotDispelTarget (*find_dispellable_target)(BotHandle bot);
    UnitHandle      (*find_dead_party_member)(BotHandle bot);

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
    /* True if the bot has `skill_id` learned at any rank. */
    bool (*bot_has_skill)(BotHandle bot, uint32_t skill_id);
    /* Teach the bot a spell (Player::learnSpell with dependent=false).
     * Used by the factory mount / spell initialization steps. */
    void (*bot_learn_spell)(BotHandle bot, uint32_t spell_id);
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

    /* ── Factory: item prototype queries ─────────────────────────────── */
    /* ItemPrototype.Quality (0..7 — poor/common/uncommon/rare/epic/...). */
    uint32_t (*item_prototype_quality)(BotHandle bot, uint32_t item_id);

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
} BotCallbacks;

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

/**
 * Set bot configuration. Must be called after playerbot_init() and before
 * any bots are created. Values of 0/0.0/false use built-in defaults.
 */
void playerbot_set_config(uint32_t react_delay_ms, uint32_t max_wait_for_move_ms,
                          float eat_hp_pct, float drink_mana_pct, bool debug);

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

#ifdef __cplusplus
} /* extern "C" */
#endif
