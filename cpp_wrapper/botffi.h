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

/* ── Callback table (C++ → Rust query/command interface) ─────────────────── */

typedef struct BotCallbacks {
    /* -- Core state snapshot (call once at tick start) -- */
    BotWorldSnapshot (*get_snapshot)(BotHandle bot);
    BotUnitSnapshot  (*get_unit_snapshot)(BotHandle bot, UnitHandle target);

    /* -- Aura queries -- */
    bool         (*has_aura)(BotHandle bot, UnitHandle target, uint32_t spell_id);
    BotAuraInfo  (*get_aura)(BotHandle bot, UnitHandle target, uint32_t spell_id);
    /* get_auras: fills *out_count and returns heap-allocated array; caller must call free_aura_list */
    BotAuraInfo* (*get_auras)(BotHandle bot, UnitHandle target, uint32_t* out_count);
    void         (*free_aura_list)(BotAuraInfo* list);

    /* -- Threat queries (CMaNGOS ThreatManager) -- */
    /* get_threat_list: ordered highest→lowest; caller must call free_threat_list */
    BotThreatEntry* (*get_threat_list)(BotHandle bot, UnitHandle target_unit, uint32_t* out_count);
    void            (*free_threat_list)(BotThreatEntry* list);
    float           (*get_unit_threat)(BotHandle bot, UnitHandle target_unit, UnitHandle from_unit);

    /* -- Unit queries -- */
    float       (*unit_distance)(BotHandle bot, UnitHandle target);
    bool        (*can_cast)(BotHandle bot, uint32_t spell_id, UnitHandle target);
    bool        (*spell_on_cooldown)(BotHandle bot, uint32_t spell_id);
    uint32_t    (*spell_cooldown_ms)(BotHandle bot, uint32_t spell_id);
    bool        (*has_los)(BotHandle bot, UnitHandle target);
    /* get_nearby_units: caller must call free_unit_list */
    UnitHandle* (*get_nearby_units)(BotHandle bot, float range, bool hostile, uint32_t* out_count);
    void        (*free_unit_list)(UnitHandle* list);

    /* -- Pathfinding / positioning (wraps CMaNGOS PathFinder + Detour) -- */
    /* Position at `distance` directly behind target (avoids cleave) */
    BotPosition    (*get_behind_position)(BotHandle bot, UnitHandle target, float distance);
    /* Nearest reachable position not in any ground effect within search_radius yards */
    BotSafePosition (*get_safe_position)(BotHandle bot, float search_radius);
    /* Evenly-spread position: bot index idx of total bots in a circle of radius around center */
    BotPosition    (*get_spread_position)(BotHandle bot, UnitHandle center, float radius,
                                          uint8_t idx, uint8_t total);
    /* Check if bot can path to (x,y,z) — no movement issued */
    bool           (*can_reach)(BotHandle bot, float x, float y, float z);

    /* -- Bot commands (all return true on success) -- */
    bool (*cast_spell)(BotHandle bot, uint32_t spell_id, UnitHandle target);
    bool (*cast_spell_pos)(BotHandle bot, uint32_t spell_id, float x, float y, float z);
    bool (*move_to)(BotHandle bot, float x, float y, float z);
    bool (*follow)(BotHandle bot, UnitHandle target, float dist, float angle);
    bool (*stop_moving)(BotHandle bot);
    bool (*attack)(BotHandle bot, UnitHandle target);
    bool (*auto_attack)(BotHandle bot, bool enable);
    bool (*say)(BotHandle bot, const char* msg, uint32_t lang);
    bool (*use_item)(BotHandle bot, uint32_t item_id, UnitHandle target);
    bool (*taunt)(BotHandle bot, UnitHandle target);   /* issues bot's taunt spell */

    /* -- Group / raid queries -- */
    UnitHandle (*group_get_tank)(BotHandle bot);       /* 0 if no tank assigned */
    UnitHandle (*group_get_healer)(BotHandle bot);     /* 0 if no healer assigned */
    uint8_t    (*group_get_role)(BotHandle bot, UnitHandle member); /* role bitmask */
} BotCallbacks;

/* ── Rust exports (entry points CMaNGOS calls into Rust) ─────────────────── */

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
 * Main AI tick. Called from Player::UpdateAI on the map worker thread.
 * elapsed_ms: milliseconds since last call.
 * minimal: true when bot activity is throttled (e.g. empty zone, server load).
 */
void playerbot_update(void* state, uint32_t elapsed_ms, bool minimal);

/**
 * Incoming network packet (master player → server).
 * Called from WorldSession::HandleLoggedInState (existing CMaNGOS hook).
 */
void playerbot_packet_in(void* state, uint16_t opcode, const uint8_t* data, uint32_t len);

/**
 * Outgoing network packet (server → master player).
 * Called from WorldSession::SendPacket (existing CMaNGOS hook).
 */
void playerbot_packet_out(void* state, uint16_t opcode, const uint8_t* data, uint32_t len);

/* ── Push combat events (CMaNGOS notifies Rust immediately when events fire) ─
 * Hooked in BotBridge.cpp via:
 *   - SMSG_SPELL_GO / SMSG_SPELL_START packet observer (spell casts)
 *   - SMSG_AURA_UPDATE packet observer (aura changes)
 *   - PlayerbotAIBase::DamageTaken() override
 *   - PlayerbotAIBase::KilledUnit() / JustDied() hooks
 * These are queued in BotState::push_events and processed at next tick start.
 */

/**
 * A unit visible to this bot cast (or failed to cast) a spell.
 * success=false means the cast was interrupted or resisted.
 */
void playerbot_unit_spell_cast(void* state, UnitHandle caster, uint32_t spell_id,
                               UnitHandle target, bool success);

/**
 * An aura was applied to or removed from a unit visible to this bot.
 */
void playerbot_aura_changed(void* state, UnitHandle unit, uint32_t spell_id,
                            bool applied, uint8_t stacks);

/**
 * A unit visible to this bot died.
 */
void playerbot_unit_died(void* state, UnitHandle victim, UnitHandle killer);

/**
 * This bot took damage (for immediate flee/reaction logic).
 * spell_id=0 for melee damage.
 */
void playerbot_damage_taken(void* state, uint32_t damage, uint32_t spell_id,
                            UnitHandle dealer);

/**
 * Global coordination tick — called from sRandomPlayerbotMgr.UpdateAI (world thread).
 * Used for cross-bot bookkeeping: group state cleanup, activity metrics.
 * This hook already exists in CMaNGOS — no modification required.
 */
void playerbot_world_update(uint32_t elapsed_ms);

#ifdef __cplusplus
} /* extern "C" */
#endif
