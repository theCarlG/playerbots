//! Typed `BotConfig` struct populated from `RawConfig`.
//!
//! Mirrors every field of the C++ `PlayerbotAIConfig` class (see
//! `playerbot/PlayerbotAIConfig.h`) and uses the same default values as
//! the C++ `Initialize()` method. The field naming follows Rust snake-
//! case; the C++ shim (`cpp_wrapper/BotConfig.{h,cpp}`) keeps the old
//! camelCase identifiers by pulling via the FFI getters in `config::ffi`.
//!
//! `BotConfig::from_raw` is the single entry point that maps parsed
//! `.conf` keys onto struct fields. When porting new options, add the
//! field here *and* the line in `from_raw`, nowhere else.
//!
//! Hot-path consumers in the Rust AI crate read via
//! `crate::config::get()`; they see the same field names they read today
//! (`react_delay_ms`, `attacker_scan_range`, `eat_hp_threshold`, …).

use std::collections::BTreeMap;

use super::raw::RawConfig;

/// Number of gear progression phases (mirrors `MAX_GEAR_PROGRESSION_LEVEL`
/// in the C++ header).
pub const MAX_GEAR_PROGRESSION_LEVEL: usize = 6;

/// `CMaNGOS` `MAX_CLASSES` is 12 across all expansions (11 player classes
/// plus 0 as a sentinel; death knight is slot 6 and is only valid on
/// wotlk but the table size is the same everywhere).
pub const MAX_CLASSES: usize = 12;

/// `CMaNGOS` `MAX_RACES` is 12 (used as array size even though some races
/// are expansion-gated).
pub const MAX_RACES: usize = 12;

/// Upper bound on player level (`DEFAULT_MAX_LEVEL` in the core is 80
/// on wotlk, 70 on tbc, 60 on classic). We use the max across all
/// expansions as the array size; the C++ shim indexes into it using the
/// runtime-selected max level, and entries past that are never read.
pub const MAX_LEVEL_PROBABILITY_SLOTS: usize = 81;

/// One entry in the world-buff table. Matches the `worldBuff` struct
/// in the C++ header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldBuff {
    pub spell_id: u32,
    pub faction_id: u32,
    pub class_id: u32,
    pub spec_id: u32,
    pub min_level: u32,
    pub max_level: u32,
    pub event_id: u32,
}

/// Parsed URL for the LLM endpoint (`ParsedUrl` in the C++ header).
#[derive(Debug, Default, Clone)]
pub struct ParsedLlmUrl {
    pub hostname: String,
    pub path: String,
    pub port: u16,
    pub https: bool,
}

/// Full bot configuration. Fields are laid out in the same order the
/// C++ `Initialize()` method assigns them.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct BotConfig {
    // ── master switch ────────────────────────────────────────────────
    pub enabled: bool,
    pub allow_guild_bots: bool,
    pub allow_multi_account_alt_bots: bool,

    // ── timing scalars ───────────────────────────────────────────────
    pub global_cool_down: u32,
    pub react_delay: u32,
    pub max_wait_for_move: u32,
    pub expire_action_time: u32,
    pub dispel_aura_duration: u32,
    pub passive_delay: u32,
    pub repeat_delay: u32,
    pub error_delay: u32,
    pub rpg_delay: u32,
    pub sit_delay: u32,
    pub return_delay: u32,
    pub loot_delay: u32,

    // ── distance / range scalars ─────────────────────────────────────
    pub sight_distance: f32,
    pub spell_distance: f32,
    pub react_distance: f32,
    pub grind_distance: f32,
    pub loot_distance: f32,
    pub group_member_loot_distance: f32,
    pub group_member_loot_distance_with_active_master: f32,
    pub gathering_distance: f32,
    pub group_member_gathering_distance: f32,
    pub group_member_gathering_distance_with_active_master: f32,
    pub shoot_distance: f32,
    pub flee_distance: f32,
    pub too_close_distance: f32,
    pub melee_distance: f32,
    pub follow_distance: f32,
    pub raid_follow_distance: f32,
    pub wander_min_distance: f32,
    pub wander_max_distance: f32,
    pub whisper_distance: f32,
    pub contact_distance: f32,
    pub aoe_radius: f32,
    pub rpg_distance: f32,
    pub target_pos_recalc_distance: f32,
    pub far_distance: f32,
    pub heal_distance: f32,
    pub aggro_distance: f32,
    pub proximity_distance: f32,
    pub max_free_move_distance: f32,
    pub free_move_delay: f32,

    // ── health / mana thresholds ─────────────────────────────────────
    pub critical_health: u32,
    pub low_health: u32,
    pub medium_health: u32,
    pub almost_full_health: u32,
    pub low_mana: u32,
    pub medium_mana: u32,

    // ── random bots: identity / population ───────────────────────────
    pub open_go_spell: u32,
    pub random_bot_autologin: bool,
    pub bot_autologin: u32, // BotAutoLogin enum
    pub random_bot_maps_as_string: String,
    pub random_bot_maps: Vec<u32>,
    pub random_bot_quest_items: Vec<u32>,
    pub random_bot_accounts: Vec<u32>,
    pub random_bot_spell_ids: Vec<u32>,
    pub random_bot_quest_ids: Vec<u32>,
    pub immune_spell_ids: Vec<u32>,
    /// Populated at runtime by `cpp_wrapper/BotConfig::loadFreeAltBotAccounts()`
    /// — the Rust parser leaves this empty because the data lives in the
    /// login/character DBs, which are not yet accessible from Rust.
    pub free_alt_bots: Vec<(u32, u32)>,
    pub toggle_always_online_accounts: Vec<String>,
    pub toggle_always_online_chars: Vec<String>,
    pub enable_random_teleports: bool,
    pub enable_minimal_move: bool,
    pub random_bot_teleport_distance: u32,
    pub random_bot_teleport_near_player: bool,
    pub transport_teleport_type: u32,
    pub random_bot_teleport_near_player_max_amount: u32,
    pub random_bot_teleport_near_player_max_amount_radius: f32,
    pub random_bot_teleport_min_interval: u32,
    pub random_bot_teleport_max_interval: u32,

    // ── random gear ──────────────────────────────────────────────────
    pub random_gear_max_level: u32,
    pub random_gear_max_diff: u32,
    pub random_gear_upgrade_enabled: bool,
    pub random_gear_tabards: bool,
    pub random_gear_tabards_replace_guild: bool,
    pub random_gear_tabards_unobtainable: bool,
    pub random_gear_tabards_chance: f32,
    pub random_gear_blacklist: Vec<u32>,
    pub random_gear_whitelist: Vec<u32>,
    pub random_gear_progression: bool,
    pub random_gear_lowering_chance: f32,
    pub roll_bad_items_with_player: bool,
    pub random_bot_max_level_chance: f32,
    pub random_bot_rpg_chance: f32,
    pub use_potion_chance: f32,
    pub attack_emote_chance: f32,
    pub random_bot_auto_create: bool,
    pub min_random_bots: u32,
    pub max_random_bots: u32,
    pub random_bot_update_interval: u32,
    pub random_bot_count_change_min_interval: u32,
    pub random_bot_count_change_max_interval: u32,
    pub random_bot_timed_logout: bool,
    pub random_bot_timed_offline: bool,
    pub min_random_bot_in_world_time: u32,
    pub max_random_bot_in_world_time: u32,
    pub min_random_bot_randomize_time: u32,
    pub max_random_bot_randomize_time: u32,
    pub min_random_bot_change_strategy_time: u32,
    pub max_random_bot_change_strategy_time: u32,
    pub min_random_bot_revive_time: u32,
    pub max_random_bot_revive_time: u32,
    pub min_random_bot_pvp_time: u32,
    pub max_random_bot_pvp_time: u32,
    pub random_bots_max_logins_per_interval: u32,
    pub random_bots_per_interval: u32,
    pub min_random_bots_price_change_interval: u32,
    pub max_random_bots_price_change_interval: u32,

    // ── auction house ────────────────────────────────────────────────
    pub should_query_ah_listings_outside_of_ah: bool,
    pub ah_over_vendor_item_ids: Vec<u32>,
    pub vendor_over_ah_item_ids: Vec<u32>,
    pub bot_check_all_auction_listings: bool,
    pub bots_save_epics: bool,

    // ── LFG / BG / PvP ───────────────────────────────────────────────
    pub random_bot_join_lfg: bool,
    pub log_random_bot_join_lfg: bool,
    pub random_bot_join_bg: bool,
    pub random_bot_auto_join_bg: bool,
    pub random_bot_bracket_count: u32,
    pub random_bot_login_at_startup: bool,
    pub random_bot_tele_level: u32,
    pub log_in_group_only: bool,
    pub log_values_per_tick: bool,
    pub fleeing_enabled: bool,
    pub summon_at_innkeepers_enabled: bool,
    pub combat_strategies: String,
    pub non_combat_strategies: String,
    pub react_strategies: String,
    pub dead_strategies: String,
    pub random_bot_combat_strategies: String,
    pub random_bot_non_combat_strategies: String,
    pub random_bot_react_strategies: String,
    pub random_bot_dead_strategies: String,
    pub random_bot_min_level: u32,
    pub random_bot_max_level: u32,
    pub random_change_multiplier: f32,

    // ── class/race/spec tables ───────────────────────────────────────
    /// `[class][spec]` spec probability; size `MAX_CLASSES × 10` to
    /// match the C++ table.
    pub spec_probability: Box<[[u32; 10]; MAX_CLASSES]>,
    /// `[class][spec][level]` premade spec strings.
    pub premade_level_spec: Box<[[[String; 91]; 10]; MAX_CLASSES]>,
    pub class_race_probability_total: u32,
    pub class_race_probability: Box<[[u32; MAX_RACES]; MAX_CLASSES]>,
    pub use_fixed_class_race_counts: bool,
    pub fixed_class_race_counts: BTreeMap<(u8, u8), u32>,
    pub level_probability: Box<[u32; MAX_LEVEL_PROBABILITY_SLOTS]>,

    // ── gear progression ─────────────────────────────────────────────
    pub gear_progression_system_enabled: bool,
    /// `[phase][0|1]` → min/max item level.
    pub gear_progression_system_item_levels: Box<[[u32; 2]; MAX_GEAR_PROGRESSION_LEVEL]>,
    /// `[phase][class][spec][slot]` → item entry id (or `-1` for unset).
    /// `slot` is `SLOT_EMPTY = 19` (max across all expansions).
    pub gear_progression_system_items:
        Box<[[[[i32; 19]; 4]; MAX_CLASSES]; MAX_GEAR_PROGRESSION_LEVEL]>,

    // ── commands / chat ──────────────────────────────────────────────
    pub command_prefix: String,
    pub command_separator: String,
    pub random_bot_account_prefix: String,
    pub random_bot_account_count: u32,
    pub delete_random_bot_accounts: bool,
    pub random_bot_guild_count: u32,
    pub delete_random_bot_guilds: bool,
    pub random_bot_arena_team_count: u32,
    pub delete_random_bot_arena_teams: bool,
    pub random_bot_arena_teams: Vec<u32>,
    pub randombots_walking_rpg: bool,
    pub randombots_walking_rpg_in_doors: bool,
    pub boost_follow: bool,
    pub turn_in_rpg: bool,
    pub global_sound_effects: bool,
    pub share_targets: bool,
    pub random_bot_guilds: Vec<u32>,
    pub pvp_prohibited_zone_ids: Vec<u32>,
    pub enable_greet: bool,
    pub random_bot_show_helmet: bool,
    pub random_bot_show_cloak: bool,
    pub disable_random_levels: bool,
    pub instant_randomize: bool,
    pub gearscorecheck: bool,
    pub level_check: i32,
    pub random_bot_pre_quests: bool,
    pub playerbots_xp_rate: f32,
    pub disable_bot_optimizations: bool,
    pub disable_activity_priorities: bool,
    pub force_active_when_near_player: bool,
    pub limit_combat_activity: bool,
    pub guild_order_always_active: bool,
    pub bot_active_alone: u32,
    pub diff_with_player: u32,
    pub diff_empty: u32,
    pub min_enchanting_bot_level: u32,
    pub randombot_starting_level: u32,
    pub random_bot_say_without_master: bool,
    pub random_bot_invite_player: bool,
    pub random_bot_group_nearby: bool,
    pub random_bot_raid_nearby: bool,
    pub random_bot_guild_nearby: bool,
    pub random_bot_form_guild: bool,
    pub random_bot_random_password: bool,
    pub invite_chat: bool,
    pub enable_off_spec_strategies: bool,
    pub use_wander_as_default_follow_strategy: bool,
    pub default_formation: String,

    pub guild_max_bot_limit: u32,

    // ── broadcast switches / chances ─────────────────────────────────
    pub enable_broadcasts: bool,
    pub broadcast_chance_max_value: u32,

    pub broadcast_to_guild_global_chance: u32,
    pub broadcast_to_world_global_chance: u32,
    pub broadcast_to_general_global_chance: u32,
    pub broadcast_to_trade_global_chance: u32,
    pub broadcast_to_lfg_global_chance: u32,
    pub broadcast_to_local_defense_global_chance: u32,
    pub broadcast_to_world_defense_global_chance: u32,
    pub broadcast_to_guild_recruitment_global_chance: u32,
    pub broadcast_to_say_global_chance: u32,
    pub broadcast_to_yell_global_chance: u32,

    pub broadcast_chance_looting_item_poor: u32,
    pub broadcast_chance_looting_item_normal: u32,
    pub broadcast_chance_looting_item_uncommon: u32,
    pub broadcast_chance_looting_item_rare: u32,
    pub broadcast_chance_looting_item_epic: u32,
    pub broadcast_chance_looting_item_legendary: u32,
    pub broadcast_chance_looting_item_artifact: u32,

    pub broadcast_chance_quest_accepted: u32,
    pub broadcast_chance_quest_update_objective_completed: u32,
    pub broadcast_chance_quest_update_objective_progress: u32,
    pub broadcast_chance_quest_update_failed_timer: u32,
    pub broadcast_chance_quest_update_complete: u32,
    pub broadcast_chance_quest_turned_in: u32,

    pub broadcast_chance_kill_normal: u32,
    pub broadcast_chance_kill_elite: u32,
    pub broadcast_chance_kill_rareelite: u32,
    pub broadcast_chance_kill_worldboss: u32,
    pub broadcast_chance_kill_rare: u32,
    pub broadcast_chance_kill_unknown: u32,
    pub broadcast_chance_kill_pet: u32,
    pub broadcast_chance_kill_player: u32,

    pub broadcast_chance_levelup_generic: u32,
    pub broadcast_chance_levelup_ten_x: u32,
    pub broadcast_chance_levelup_max_level: u32,

    pub broadcast_chance_suggest_instance: u32,
    pub broadcast_chance_suggest_quest: u32,
    pub broadcast_chance_suggest_grind_materials: u32,
    pub broadcast_chance_suggest_grind_reputation: u32,
    pub broadcast_chance_suggest_sell: u32,
    pub broadcast_chance_suggest_something: u32,
    pub broadcast_chance_suggest_something_toxic: u32,

    pub broadcast_chance_suggest_toxic_links: u32,
    pub toxic_links_prefix: String,
    pub toxic_links_replies_chance: u32,

    pub broadcast_chance_suggest_thunderfury: u32,
    pub thunderfury_replies_chance: u32,

    pub broadcast_chance_guild_management: u32,

    pub guild_replies_rate: u32,

    pub bot_accept_duel_minimum_level: u32,

    pub talents_in_public_note: bool,
    pub non_gm_free_summon: bool,

    pub self_bot_level: u32,  // BotSelfBotLevel enum
    pub iterations_per_tick: u32,

    pub auto_pick_reward: String,
    pub auto_equip_upgrade_loot: bool,
    pub sync_quest_with_player: bool,
    pub sync_quest_for_player: bool,
    pub auto_train_spells: String,
    pub auto_pick_talents: String,
    pub auto_learn_trainer_spells: bool,
    pub auto_learn_quest_spells: bool,
    pub auto_learn_dropped_spells: bool,
    pub auto_do_quests: bool,
    pub sync_level_with_players: bool,
    pub sync_level_max_above: u32,
    pub sync_level_no_player: u32,
    pub tweak_value: u32,
    pub respawn_mod_neutral: f32,
    pub respawn_mod_hostile: f32,
    pub respawn_mod_threshold: u32,
    pub respawn_mod_max: u32,
    pub respawn_mod_for_player_bots: bool,
    pub respawn_mod_for_instances: bool,

    pub random_bot_login_with_player: bool,
    pub async_bot_login: bool,
    pub preload_holders: bool,
    pub free_room_for_non_spare_bots: u32,
    pub login_bots_near_player_range: u32,
    pub default_login_criteria: Vec<String>,
    pub login_criteria: Vec<Vec<String>>,

    // ── jump tuning ──────────────────────────────────────────────────
    pub jump_in_bg: bool,
    pub jump_with_player: bool,
    pub jump_follow: bool,
    pub jump_chase: bool,
    pub use_knockback: bool,
    pub jump_no_combat_chance: f32,
    pub jump_melee_in_combat_chance: f32,
    pub jump_random_chance: f32,
    pub jump_in_place_chance: f32,
    pub jump_backward_chance: f32,
    pub jump_height_limit: f32,
    pub jump_v_speed: f32,
    pub jump_h_speed: f32,

    // ── logging ──────────────────────────────────────────────────────
    pub allowed_log_files: Vec<String>,
    pub debug_filter: Vec<String>,

    pub bot_cheat_mask: u32,
    pub rnd_bot_cheat_mask: u32,

    pub world_buffs: Vec<WorldBuff>,

    pub command_server_port: i32,
    pub perf_mon_enabled: bool,
    pub explicit_db_store_save: bool,

    // ── LLM ─────────────────────────────────────────────────────────
    pub llm_api_endpoint: String,
    pub llm_api_key: String,
    pub llm_api_json: String,
    pub llm_pre_prompt: String,
    pub llm_pre_rpg_prompt: String,
    pub llm_prompt: String,
    pub llm_post_prompt: String,
    pub llm_response_start_pattern: String,
    pub llm_response_end_pattern: String,
    pub llm_response_delete_pattern: String,
    pub llm_response_split_pattern: String,
    pub llm_enabled: u32,
    pub llm_context_length: u32,
    pub llm_bot_to_bot_chat_chance: u32,
    pub llm_generation_timeout: u32,
    pub llm_max_simultanious_generations: u32,
    pub llm_rpg_ai_chat_chance: u32,
    pub llm_global_context: bool,
    pub llm_end_point_url: ParsedLlmUrl,

    // ── Eat/drink tuning ─────────────────────────────────────────────
    pub eat_drink_min_distance: u32,
    pub eat_drink_max_distance: u32,

    // ── Rust-side convenience mirrors ────────────────────────────────
    /// Alias of `react_delay` in ms (kept for the hot path that reads
    /// `cfg.react_delay_ms`).
    pub react_delay_ms: u32,
    /// Alias of `max_wait_for_move` in ms.
    pub max_wait_for_move_ms: u32,
    /// How often to refresh the attacker scan (ms). Tracks
    /// `react_delay_ms * 5` matching the PB2 cadence unless overridden.
    pub attacker_refresh_ms: u64,
    /// How often to refresh the nearby scan (ms).
    pub nearby_refresh_ms: u64,
    /// Hostile-unit scan distance (yards); mirrors `aggro_distance`.
    pub attacker_scan_range: f32,
    /// All-nearby-unit scan distance (yards); mirrors `sight_distance`.
    pub nearby_scan_range: f32,
    /// HP% threshold for bots to eat (fraction 0.0–1.0).
    pub eat_hp_threshold: f32,
    /// Mana% threshold for bots to drink (fraction 0.0–1.0).
    pub drink_mana_threshold: f32,
    /// Rust-side debug flag (derived from `!debug_filter.is_empty()`).
    pub debug: bool,
}

impl Default for BotConfig {
    fn default() -> Self {
        BotConfig::from_raw(&RawConfig::default())
    }
}

impl BotConfig {
    /// Populate a fresh `BotConfig` from a parsed `RawConfig`.
    ///
    /// This is the single source of truth for default values — every
    /// key the C++ `PlayerbotAIConfig::Initialize()` supported must
    /// appear here with the same default.
    #[allow(clippy::too_many_lines)]
    pub fn from_raw(raw: &RawConfig) -> Self {
        let mut cfg = Self {
            enabled: raw.get_bool("AiPlayerbot.Enabled", false),
            allow_guild_bots: raw.get_bool("AiPlayerbot.AllowGuildBots", true),
            allow_multi_account_alt_bots: raw.get_bool("AiPlayerbot.AllowMultiAccountAltBots", true),

            global_cool_down: raw.get_u32("AiPlayerbot.GlobalCooldown", 500),
            react_delay: raw.get_u32("AiPlayerbot.ReactDelay", 100),
            max_wait_for_move: raw.get_u32("AiPlayerbot.MaxWaitForMove", 3000),
            expire_action_time: raw.get_u32("AiPlayerbot.ExpireActionTime", 5000),
            dispel_aura_duration: raw.get_u32("AiPlayerbot.DispelAuraDuration", 2000),
            passive_delay: raw.get_u32("AiPlayerbot.PassiveDelay", 4000),
            repeat_delay: raw.get_u32("AiPlayerbot.RepeatDelay", 5000),
            error_delay: raw.get_u32("AiPlayerbot.ErrorDelay", 5000),
            rpg_delay: raw.get_u32("AiPlayerbot.RpgDelay", 3000),
            sit_delay: raw.get_u32("AiPlayerbot.SitDelay", 30000),
            return_delay: raw.get_u32("AiPlayerbot.ReturnDelay", 7000),
            loot_delay: raw.get_u32("AiPlayerbot.LootDelayDelay", 750),

            sight_distance: raw.get_f32("AiPlayerbot.SightDistance", 75.0),
            spell_distance: raw.get_f32("AiPlayerbot.SpellDistance", 25.0),
            react_distance: raw.get_f32("AiPlayerbot.ReactDistance", 150.0),
            grind_distance: raw.get_f32("AiPlayerbot.GrindDistance", 75.0),
            loot_distance: raw.get_f32("AiPlayerbot.LootDistance", 25.0),
            group_member_loot_distance: raw
                .get_f32("AiPlayerbot.GroupMemberLootDistance", 15.0),
            group_member_loot_distance_with_active_master: raw.get_f32(
                "AiPlayerbot.GroupMemberLootDistanceWithActiveMaster",
                10.0,
            ),
            gathering_distance: raw.get_f32("AiPlayerbot.GatheringDistance", 15.0),
            group_member_gathering_distance: raw
                .get_f32("AiPlayerbot.GroupMemberGatheringDistance", 10.0),
            group_member_gathering_distance_with_active_master: raw.get_f32(
                "AiPlayerbot.GroupMemberGatheringDistanceWithActiveMaster",
                5.0,
            ),
            shoot_distance: raw.get_f32("AiPlayerbot.ShootDistance", 25.0),
            flee_distance: raw.get_f32("AiPlayerbot.FleeDistance", 8.0),
            too_close_distance: raw.get_f32("AiPlayerbot.TooCloseDistance", 5.0),
            melee_distance: raw.get_f32("AiPlayerbot.MeleeDistance", 1.5),
            follow_distance: raw.get_f32("AiPlayerbot.FollowDistance", 1.5),
            raid_follow_distance: raw.get_f32("AiPlayerbot.RaidFollowDistance", 5.0),
            wander_min_distance: raw.get_f32("AiPlayerbot.WanderMinDistance", 5.0),
            wander_max_distance: raw.get_f32("AiPlayerbot.WanderMaxDistance", 50.0),
            whisper_distance: raw.get_f32("AiPlayerbot.WhisperDistance", 6000.0),
            contact_distance: raw.get_f32("AiPlayerbot.ContactDistance", 0.5),
            aoe_radius: raw.get_f32("AiPlayerbot.AoeRadius", 5.0),
            rpg_distance: raw.get_f32("AiPlayerbot.RpgDistance", 80.0),
            target_pos_recalc_distance: raw
                .get_f32("AiPlayerbot.TargetPosRecalcDistance", 0.1),
            far_distance: raw.get_f32("AiPlayerbot.FarDistance", 20.0),
            heal_distance: raw.get_f32("AiPlayerbot.HealDistance", 125.0),
            aggro_distance: raw.get_f32("AiPlayerbot.AggroDistance", 22.0),
            proximity_distance: raw.get_f32("AiPlayerbot.ProximityDistance", 20.0),
            max_free_move_distance: raw.get_f32("AiPlayerbot.MaxFreeMoveDistance", 150.0),
            free_move_delay: raw.get_f32("AiPlayerbot.FreeMoveDelay", 30.0),

            critical_health: raw.get_u32("AiPlayerbot.CriticalHealth", 20),
            low_health: raw.get_u32("AiPlayerbot.LowHealth", 50),
            medium_health: raw.get_u32("AiPlayerbot.MediumHealth", 70),
            almost_full_health: raw.get_u32("AiPlayerbot.AlmostFullHealth", 90),
            low_mana: raw.get_u32("AiPlayerbot.LowMana", 15),
            medium_mana: raw.get_u32("AiPlayerbot.MediumMana", 40),

            open_go_spell: raw.get_u32("AiPlayerbot.OpenGoSpell", 6477),
            random_bot_autologin: raw.get_bool("AiPlayerbot.RandomBotAutologin", true),
            bot_autologin: raw.get_u32("AiPlayerbot.BotAutologin", 0),
            random_bot_maps_as_string: String::new(), // assigned below
            random_bot_maps: Vec::new(),              // assigned below
            random_bot_quest_items: raw.get_u32_list_default(
                "AiPlayerbot.RandomBotQuestItems",
                "6948,5175,5176,5177,5178,16309,12382,13704,11000,22754",
            ),
            random_bot_accounts: Vec::new(),
            random_bot_spell_ids: raw
                .get_u32_list_default("AiPlayerbot.RandomBotSpellIds", "54197"),
            random_bot_quest_ids: raw.get_u32_list_default(
                "AiPlayerbot.RandomBotQuestIds",
                "7848,3802,5505,6502,7761,9378",
            ),
            immune_spell_ids: raw.get_u32_list("AiPlayerbot.ImmuneSpellIds"),
            free_alt_bots: Vec::new(),
            toggle_always_online_accounts: Vec::new(), // assigned below
            toggle_always_online_chars: Vec::new(),    // assigned below
            enable_random_teleports: raw.get_bool("AiPlayerbot.EnableRandomTeleports", true),
            enable_minimal_move: raw.get_bool("AiPlayerbot.EnableMinimalMove", true),
            random_bot_teleport_distance: raw
                .get_u32("AiPlayerbot.RandomBotTeleportDistance", 1000),
            random_bot_teleport_near_player: raw
                .get_bool("AiPlayerbot.RandomBotTeleportNearPlayer", false),
            transport_teleport_type: raw.get_u32("AiPlayerbot.TransportTeleportType", 2),
            random_bot_teleport_near_player_max_amount: raw
                .get_u32("AiPlayerbot.RandomBotTeleportNearPlayerMaxAmount", 0),
            random_bot_teleport_near_player_max_amount_radius: raw
                .get_f32("AiPlayerbot.RandomBotTeleportNearPlayerMaxAmountRadius", 0.0),
            random_bot_teleport_min_interval: raw
                .get_u32("AiPlayerbot.RandomBotTeleportTeleportMinInterval", 2 * 3600),
            random_bot_teleport_max_interval: raw
                .get_u32("AiPlayerbot.RandomBotTeleportTeleportMaxInterval", 48 * 3600),

            random_gear_max_level: raw.get_u32("AiPlayerbot.RandomGearMaxLevel", 500),
            random_gear_max_diff: raw.get_u32("AiPlayerbot.RandomGearMaxDiff", 9),
            random_gear_upgrade_enabled: raw
                .get_bool("AiPlayerbot.RandomGearUpgradeEnabled", false),
            random_gear_tabards: raw.get_bool("AiPlayerbot.RandomGearTabards", false),
            random_gear_tabards_replace_guild: raw
                .get_bool("AiPlayerbot.RandomGearTabardsReplaceGuild", false),
            random_gear_tabards_unobtainable: raw
                .get_bool("AiPlayerbot.RandomGearTabardsUnobtainable", false),
            random_gear_tabards_chance: raw
                .get_f32("AiPlayerbot.RandomGearTabardsChance", 0.1),
            random_gear_blacklist: raw.get_u32_list("AiPlayerbot.RandomGearBlacklist"),
            random_gear_whitelist: raw.get_u32_list("AiPlayerbot.RandomGearWhitelist"),
            random_gear_progression: raw.get_bool("AiPlayerbot.RandomGearProgression", true),
            random_gear_lowering_chance: raw
                .get_f32("AiPlayerbot.RandomGearLoweringChance", 0.15),
            roll_bad_items_with_player: raw
                .get_bool("AiPlayerbot.RollBadItemsWithPlayer", false),
            random_bot_max_level_chance: raw
                .get_f32("AiPlayerbot.RandomBotMaxLevelChance", 0.15),
            random_bot_rpg_chance: raw.get_f32("AiPlayerbot.RandomBotRpgChance", 0.35),
            use_potion_chance: raw.get_f32("AiPlayerbot.UsePotionChance", 1.0),
            attack_emote_chance: raw.get_f32("AiPlayerbot.AttackEmoteChance", 0.0),
            random_bot_auto_create: raw.get_bool("AiPlayerbot.RandomBotAutoCreate", true),
            min_random_bots: raw.get_u32("AiPlayerbot.MinRandomBots", 50),
            max_random_bots: raw.get_u32("AiPlayerbot.MaxRandomBots", 200),
            random_bot_update_interval: raw
                .get_u32("AiPlayerbot.RandomBotUpdateInterval", 1000),
            random_bot_count_change_min_interval: raw
                .get_u32("AiPlayerbot.RandomBotCountChangeMinInterval", 1800),
            random_bot_count_change_max_interval: raw
                .get_u32("AiPlayerbot.RandomBotCountChangeMaxInterval", 2 * 3600),
            random_bot_timed_logout: raw.get_bool("AiPlayerbot.RandomBotTimedLogout", true),
            random_bot_timed_offline: raw.get_bool("AiPlayerbot.RandomBotTimedOffline", false),
            min_random_bot_in_world_time: raw
                .get_u32("AiPlayerbot.MinRandomBotInWorldTime", 1800),
            max_random_bot_in_world_time: raw
                .get_u32("AiPlayerbot.MaxRandomBotInWorldTime", 6 * 3600),
            min_random_bot_randomize_time: raw
                .get_u32("AiPlayerbot.MinRandomBotRandomizeTime", 6 * 3600),
            max_random_bot_randomize_time: raw
                .get_u32("AiPlayerbot.MaxRandomRandomizeTime", 24 * 3600),
            min_random_bot_change_strategy_time: raw
                .get_u32("AiPlayerbot.MinRandomBotChangeStrategyTime", 1800),
            max_random_bot_change_strategy_time: raw
                .get_u32("AiPlayerbot.MaxRandomBotChangeStrategyTime", 2 * 3600),
            min_random_bot_revive_time: raw
                .get_u32("AiPlayerbot.MinRandomBotReviveTime", 60),
            max_random_bot_revive_time: raw.get_u32("AiPlayerbot.MaxRandomReviveTime", 300),
            min_random_bot_pvp_time: raw.get_u32("AiPlayerbot.MinRandomBotPvpTime", 0),
            max_random_bot_pvp_time: raw.get_u32("AiPlayerbot.MaxRandomBotPvpTime", 0),
            random_bots_max_logins_per_interval: raw
                .get_u32("AiPlayerbot.RandomBotsMaxLoginsPerInterval", 10),
            random_bots_per_interval: raw.get_u32("AiPlayerbot.RandomBotsPerInterval", 0),
            min_random_bots_price_change_interval: raw
                .get_u32("AiPlayerbot.MinRandomBotsPriceChangeInterval", 2 * 3600),
            max_random_bots_price_change_interval: raw
                .get_u32("AiPlayerbot.MaxRandomBotsPriceChangeInterval", 48 * 3600),

            should_query_ah_listings_outside_of_ah: raw
                .get_bool("AiPlayerbot.ShouldQueryAHListingsOutsideOfAH", true),
            ah_over_vendor_item_ids: raw.get_u32_list("AiPlayerbot.AhOverVendorItemIds"),
            vendor_over_ah_item_ids: raw.get_u32_list("AiPlayerbot.VendorOverAHItemIds"),
            bot_check_all_auction_listings: raw
                .get_bool("AiPlayerbot.BotCheckAllAuctionListings", false),
            bots_save_epics: raw.get_bool("AiPlayerbot.BotsSaveEpics", true),

            random_bot_join_lfg: raw.get_bool("AiPlayerbot.RandomBotJoinLfg", true),
            log_random_bot_join_lfg: raw.get_bool("AiPlayerbot.LogRandomBotJoinLfg", false),
            random_bot_join_bg: raw.get_bool("AiPlayerbot.RandomBotJoinBG", true),
            random_bot_auto_join_bg: raw.get_bool("AiPlayerbot.RandomBotAutoJoinBG", false),
            random_bot_bracket_count: raw.get_u32("AiPlayerbot.RandomBotBracketCount", 3),
            random_bot_login_at_startup: raw
                .get_bool("AiPlayerbot.RandomBotLoginAtStartup", true),
            random_bot_tele_level: raw.get_u32("AiPlayerbot.RandomBotTeleLevel", 5),
            log_in_group_only: raw.get_bool("AiPlayerbot.LogInGroupOnly", true),
            log_values_per_tick: raw.get_bool("AiPlayerbot.LogValuesPerTick", false),
            fleeing_enabled: raw.get_bool("AiPlayerbot.FleeingEnabled", true),
            summon_at_innkeepers_enabled: raw
                .get_bool("AiPlayerbot.SummonAtInnkeepersEnabled", true),
            combat_strategies: raw.get_string("AiPlayerbot.CombatStrategies", "").to_string(),
            non_combat_strategies: raw
                .get_string("AiPlayerbot.NonCombatStrategies", "+return,+delayed roll")
                .to_string(),
            react_strategies: raw.get_string("AiPlayerbot.ReactStrategies", "").to_string(),
            dead_strategies: raw.get_string("AiPlayerbot.DeadStrategies", "").to_string(),
            random_bot_combat_strategies: raw
                .get_string(
                    "AiPlayerbot.RandomBotCombatStrategies",
                    "-threat,+custom::say",
                )
                .to_string(),
            random_bot_non_combat_strategies: raw
                .get_string("AiPlayerbot.RandomBotNonCombatStrategies", "+custom::say")
                .to_string(),
            random_bot_react_strategies: raw
                .get_string("AiPlayerbot.RandomBotReactStrategies", "")
                .to_string(),
            random_bot_dead_strategies: raw
                .get_string("AiPlayerbot.RandomBotDeadStrategies", "")
                .to_string(),
            random_bot_min_level: raw.get_u32("AiPlayerbot.RandomBotMinLevel", 1),
            random_bot_max_level: raw.get_u32("AiPlayerbot.RandomBotMaxLevel", 255),
            random_change_multiplier: raw.get_f32("AiPlayerbot.RandomChangeMultiplier", 1.0),

            spec_probability: Box::new([[0; 10]; MAX_CLASSES]),
            premade_level_spec: Box::new(std::array::from_fn(|_| {
                std::array::from_fn(|_| std::array::from_fn(|_| String::new()))
            })),
            class_race_probability_total: 0, // assigned below
            class_race_probability: Box::new([[0; MAX_RACES]; MAX_CLASSES]),
            use_fixed_class_race_counts: raw
                .get_bool("AiPlayerbot.ClassRace.UseFixedClassRaceCounts", false),
            fixed_class_race_counts: BTreeMap::new(),
            level_probability: Box::new([100; MAX_LEVEL_PROBABILITY_SLOTS]),

            gear_progression_system_enabled: raw
                .get_bool("AiPlayerbot.GearProgressionSystem.Enable", false),
            gear_progression_system_item_levels: Box::new(
                [[9_999_999; 2]; MAX_GEAR_PROGRESSION_LEVEL],
            ),
            gear_progression_system_items: Box::new(std::array::from_fn(|_| {
                std::array::from_fn(|_| std::array::from_fn(|_| [-1_i32; 19]))
            })),

            command_prefix: raw.get_string("AiPlayerbot.CommandPrefix", "").to_string(),
            command_separator: raw
                .get_string("AiPlayerbot.CommandSeparator", "\\\\")
                .to_string(),
            random_bot_account_prefix: raw
                .get_string("AiPlayerbot.RandomBotAccountPrefix", "rndbot")
                .to_string(),
            random_bot_account_count: raw.get_u32("AiPlayerbot.RandomBotAccountCount", 50),
            delete_random_bot_accounts: raw
                .get_bool("AiPlayerbot.DeleteRandomBotAccounts", false),
            random_bot_guild_count: raw.get_u32("AiPlayerbot.RandomBotGuildCount", 20),
            delete_random_bot_guilds: raw.get_bool("AiPlayerbot.DeleteRandomBotGuilds", false),
            random_bot_arena_team_count: raw.get_u32("AiPlayerbot.RandomBotArenaTeamCount", 20),
            delete_random_bot_arena_teams: raw
                .get_bool("AiPlayerbot.DeleteRandomBotArenaTeams", false),
            random_bot_arena_teams: Vec::new(),
            randombots_walking_rpg: raw.get_bool("AiPlayerbot.RandombotsWalkingRPG", false),
            randombots_walking_rpg_in_doors: raw
                .get_bool("AiPlayerbot.RandombotsWalkingRPG.InDoors", false),
            boost_follow: raw.get_bool("AiPlayerbot.BoostFollow", false),
            turn_in_rpg: raw.get_bool("AiPlayerbot.TurnInRpg", false),
            global_sound_effects: raw.get_bool("AiPlayerbot.GlobalSoundEffects", false),
            share_targets: raw.get_bool("AiPlayerbot.ShareTargets", true),
            random_bot_guilds: Vec::new(),
            pvp_prohibited_zone_ids: raw.get_u32_list_default(
                "AiPlayerbot.PvpProhibitedZoneIds",
                "2255,656,2361,2362,2363,976,35,2268,3425,392,541,1446,3828,3712,3738,3565,3539,3623,4152,3988,4658,4284,4418,4436,4275,4323",
            ),
            enable_greet: raw.get_bool("AiPlayerbot.EnableGreet", false),
            random_bot_show_helmet: raw.get_bool("AiPlayerbot.RandomBotShowHelmet", false),
            random_bot_show_cloak: raw.get_bool("AiPlayerbot.RandomBotShowCloak", false),
            disable_random_levels: raw.get_bool("AiPlayerbot.DisableRandomLevels", false),
            instant_randomize: raw.get_bool("AiPlayerbot.InstantRandomize", true),
            gearscorecheck: raw.get_bool("AiPlayerbot.GearScoreCheck", false),
            level_check: raw.get_i32("AiPlayerbot.LevelCheck", 30),
            random_bot_pre_quests: raw.get_bool("AiPlayerbot.PreQuests", true),
            playerbots_xp_rate: raw.get_f32("AiPlayerbot.XPRate", 1.0),
            disable_bot_optimizations: raw
                .get_bool("AiPlayerbot.DisableBotOptimizations", false),
            disable_activity_priorities: raw
                .get_bool("AiPlayerbot.DisableActivityPriorities", false),
            force_active_when_near_player: raw
                .get_bool("AiPlayerbot.ForceActiveWhenNearPlayer", false),
            limit_combat_activity: raw.get_bool("AiPlayerbot.LimitCombatActivity", false),
            guild_order_always_active: raw
                .get_bool("AiPlayerbot.GuildOrderAlwaysActive", true),
            bot_active_alone: raw.get_u32("AiPlayerbot.botActiveAlone", 10),
            diff_with_player: raw.get_u32("AiPlayerbot.DiffWithPlayer", 100),
            diff_empty: raw.get_u32("AiPlayerbot.DiffEmpty", 200),
            min_enchanting_bot_level: raw.get_u32("AiPlayerbot.minEnchantingBotLevel", 60),
            randombot_starting_level: raw.get_u32("AiPlayerbot.randombotStartingLevel", 5),
            random_bot_say_without_master: raw
                .get_bool("AiPlayerbot.RandomBotSayWithoutMaster", false),
            random_bot_invite_player: raw.get_bool("AiPlayerbot.RandomBotInvitePlayer", true),
            random_bot_group_nearby: raw.get_bool("AiPlayerbot.RandomBotGroupNearby", true),
            random_bot_raid_nearby: raw.get_bool("AiPlayerbot.RandomBotRaidNearby", true),
            random_bot_guild_nearby: raw.get_bool("AiPlayerbot.RandomBotGuildNearby", true),
            random_bot_form_guild: raw.get_bool("AiPlayerbot.RandomBotFormGuild", true),
            random_bot_random_password: raw
                .get_bool("AiPlayerbot.RandomBotRandomPassword", true),
            invite_chat: raw.get_bool("AiPlayerbot.InviteChat", true),
            enable_off_spec_strategies: raw.get_bool("AiPlayerbot.EnableOffSpecStrategies", true),
            use_wander_as_default_follow_strategy: raw
                .get_bool("AiPlayerbot.UseWanderAsDefaultFollowStrategy", true),
            default_formation: raw
                .get_string("AiPlayerbot.DefaultFormation", "near")
                .to_string(),

            guild_max_bot_limit: raw.get_u32("AiPlayerbot.GuildMaxBotLimit", 1000),

            enable_broadcasts: raw.get_bool("AiPlayerbot.EnableBroadcasts", true),
            broadcast_chance_max_value: 0, // assigned below

            broadcast_to_guild_global_chance: raw
                .get_u32("AiPlayerbot.BroadcastToGuildGlobalChance", 30000),
            broadcast_to_world_global_chance: raw
                .get_u32("AiPlayerbot.BroadcastToWorldGlobalChance", 30000),
            broadcast_to_general_global_chance: raw
                .get_u32("AiPlayerbot.BroadcastToGeneralGlobalChance", 30000),
            broadcast_to_trade_global_chance: raw
                .get_u32("AiPlayerbot.BroadcastToTradeGlobalChance", 30000),
            broadcast_to_lfg_global_chance: raw
                .get_u32("AiPlayerbot.BroadcastToLFGGlobalChance", 30000),
            broadcast_to_local_defense_global_chance: raw
                .get_u32("AiPlayerbot.BroadcastToLocalDefenseGlobalChance", 30000),
            broadcast_to_world_defense_global_chance: raw
                .get_u32("AiPlayerbot.BroadcastToWorldDefenseGlobalChance", 30000),
            broadcast_to_guild_recruitment_global_chance: raw
                .get_u32("AiPlayerbot.BroadcastToGuildRecruitmentGlobalChance", 30000),
            broadcast_to_say_global_chance: raw
                .get_u32("AiPlayerbot.BroadcastToSayGlobalChance", 30000),
            broadcast_to_yell_global_chance: raw
                .get_u32("AiPlayerbot.BroadcastToYellGlobalChance", 30000),

            broadcast_chance_looting_item_poor: raw
                .get_u32("AiPlayerbot.BroadcastChanceLootingItemPoor", 30),
            broadcast_chance_looting_item_normal: raw
                .get_u32("AiPlayerbot.BroadcastChanceLootingItemNormal", 300),
            broadcast_chance_looting_item_uncommon: raw
                .get_u32("AiPlayerbot.BroadcastChanceLootingItemUncommon", 10000),
            broadcast_chance_looting_item_rare: raw
                .get_u32("AiPlayerbot.BroadcastChanceLootingItemRare", 20000),
            broadcast_chance_looting_item_epic: raw
                .get_u32("AiPlayerbot.BroadcastChanceLootingItemEpic", 30000),
            broadcast_chance_looting_item_legendary: raw
                .get_u32("AiPlayerbot.BroadcastChanceLootingItemLegendary", 30000),
            broadcast_chance_looting_item_artifact: raw
                .get_u32("AiPlayerbot.BroadcastChanceLootingItemArtifact", 30000),

            broadcast_chance_quest_accepted: raw
                .get_u32("AiPlayerbot.BroadcastChanceQuestAccepted", 6000),
            broadcast_chance_quest_update_objective_completed: raw
                .get_u32("AiPlayerbot.BroadcastChanceQuestUpdateObjectiveCompleted", 300),
            broadcast_chance_quest_update_objective_progress: raw
                .get_u32("AiPlayerbot.BroadcastChanceQuestUpdateObjectiveProgress", 300),
            broadcast_chance_quest_update_failed_timer: raw
                .get_u32("AiPlayerbot.BroadcastChanceQuestUpdateFailedTimer", 300),
            broadcast_chance_quest_update_complete: raw
                .get_u32("AiPlayerbot.BroadcastChanceQuestUpdateComplete", 1000),
            broadcast_chance_quest_turned_in: raw
                .get_u32("AiPlayerbot.BroadcastChanceQuestTurnedIn", 10000),

            broadcast_chance_kill_normal: raw
                .get_u32("AiPlayerbot.BroadcastChanceKillNormal", 30),
            broadcast_chance_kill_elite: raw
                .get_u32("AiPlayerbot.BroadcastChanceKillElite", 300),
            broadcast_chance_kill_rareelite: raw
                .get_u32("AiPlayerbot.BroadcastChanceKillRareelite", 3000),
            broadcast_chance_kill_worldboss: raw
                .get_u32("AiPlayerbot.BroadcastChanceKillWorldboss", 20000),
            broadcast_chance_kill_rare: raw
                .get_u32("AiPlayerbot.BroadcastChanceKillRare", 10000),
            broadcast_chance_kill_unknown: raw
                .get_u32("AiPlayerbot.BroadcastChanceKillUnknown", 100),
            broadcast_chance_kill_pet: raw.get_u32("AiPlayerbot.BroadcastChanceKillPet", 10),
            broadcast_chance_kill_player: raw
                .get_u32("AiPlayerbot.BroadcastChanceKillPlayer", 30),

            broadcast_chance_levelup_generic: raw
                .get_u32("AiPlayerbot.BroadcastChanceLevelupGeneric", 20000),
            broadcast_chance_levelup_ten_x: raw
                .get_u32("AiPlayerbot.BroadcastChanceLevelupTenX", 30000),
            broadcast_chance_levelup_max_level: raw
                .get_u32("AiPlayerbot.BroadcastChanceLevelupMaxLevel", 30000),

            broadcast_chance_suggest_instance: raw
                .get_u32("AiPlayerbot.BroadcastChanceSuggestInstance", 5000),
            broadcast_chance_suggest_quest: raw
                .get_u32("AiPlayerbot.BroadcastChanceSuggestQuest", 10000),
            broadcast_chance_suggest_grind_materials: raw
                .get_u32("AiPlayerbot.BroadcastChanceSuggestGrindMaterials", 5000),
            broadcast_chance_suggest_grind_reputation: raw
                .get_u32("AiPlayerbot.BroadcastChanceSuggestGrindReputation", 5000),
            broadcast_chance_suggest_sell: raw
                .get_u32("AiPlayerbot.BroadcastChanceSuggestSell", 300),
            broadcast_chance_suggest_something: raw
                .get_u32("AiPlayerbot.BroadcastChanceSuggestSomething", 30000),
            broadcast_chance_suggest_something_toxic: raw
                .get_u32("AiPlayerbot.BroadcastChanceSuggestSomethingToxic", 0),

            broadcast_chance_suggest_toxic_links: raw
                .get_u32("AiPlayerbot.BroadcastChanceSuggestToxicLinks", 0),
            toxic_links_prefix: raw
                .get_string("AiPlayerbot.ToxicLinksPrefix", "gnomes")
                .to_string(),
            toxic_links_replies_chance: raw.get_u32("AiPlayerbot.ToxicLinksRepliesChance", 30),

            broadcast_chance_suggest_thunderfury: raw
                .get_u32("AiPlayerbot.BroadcastChanceSuggestThunderfury", 1),
            thunderfury_replies_chance: raw.get_u32("AiPlayerbot.ThunderfuryRepliesChance", 40),

            broadcast_chance_guild_management: raw
                .get_u32("AiPlayerbot.BroadcastChanceGuildManagement", 30000),

            guild_replies_rate: raw.get_u32("AiPlayerbot.GuildRepliesRate", 100),

            bot_accept_duel_minimum_level: raw
                .get_u32("AiPlayerbot.BotAcceptDuelMinimumLevel", 10),

            talents_in_public_note: raw.get_bool("AiPlayerbot.TalentsInPublicNote", false),
            non_gm_free_summon: raw.get_bool("AiPlayerbot.NonGmFreeSummon", false),

            self_bot_level: raw.get_u32("AiPlayerbot.SelfBotLevel", 1), // GM_ONLY = 1
            iterations_per_tick: raw.get_u32("AiPlayerbot.IterationsPerTick", 100),

            auto_pick_reward: raw.get_string("AiPlayerbot.AutoPickReward", "no").to_string(),
            auto_equip_upgrade_loot: raw.get_bool("AiPlayerbot.AutoEquipUpgradeLoot", false),
            sync_quest_with_player: raw.get_bool("AiPlayerbot.SyncQuestWithPlayer", false),
            sync_quest_for_player: raw.get_bool("AiPlayerbot.SyncQuestForPlayer", false),
            auto_train_spells: raw
                .get_string("AiPlayerbot.AutoTrainSpells", "no")
                .to_string(),
            auto_pick_talents: raw
                .get_string("AiPlayerbot.AutoPickTalents", "no")
                .to_string(),
            auto_learn_trainer_spells: raw.get_bool("AiPlayerbot.AutoLearnTrainerSpells", false),
            auto_learn_quest_spells: raw.get_bool("AiPlayerbot.AutoLearnQuestSpells", false),
            auto_learn_dropped_spells: raw.get_bool("AiPlayerbot.AutoLearnDroppedSpells", false),
            auto_do_quests: raw.get_bool("AiPlayerbot.AutoDoQuests", true),
            sync_level_with_players: raw.get_bool("AiPlayerbot.SyncLevelWithPlayers", false),
            sync_level_max_above: raw.get_u32("AiPlayerbot.SyncLevelMaxAbove", 5),
            sync_level_no_player: raw
                .get_u32("AiPlayerbot.SyncLevelNoPlayer", 5), // patched below
            tweak_value: raw.get_u32("AiPlayerbot.TweakValue", 0),
            respawn_mod_neutral: raw.get_f32("AiPlayerbot.RespawnModNeutral", 10.0),
            respawn_mod_hostile: raw.get_f32("AiPlayerbot.RespawnModHostile", 5.0),
            respawn_mod_threshold: raw.get_u32("AiPlayerbot.RespawnModThreshold", 10),
            respawn_mod_max: raw.get_u32("AiPlayerbot.RespawnModMax", 18),
            respawn_mod_for_player_bots: raw.get_bool("AiPlayerbot.RespawnModForPlayerBots", false),
            respawn_mod_for_instances: raw.get_bool("AiPlayerbot.RespawnModForInstances", false),

            random_bot_login_with_player: raw
                .get_bool("AiPlayerbot.RandomBotLoginWithPlayer", false),
            async_bot_login: raw.get_bool("AiPlayerbot.AsyncBotLogin", false),
            preload_holders: raw.get_bool("AiPlayerbot.PreloadHolders", false),
            free_room_for_non_spare_bots: raw.get_u32("AiPlayerbot.FreeRoomForNonSpareBots", 1),
            login_bots_near_player_range: raw
                .get_u32("AiPlayerbot.LoginBotsNearPlayerRange", 1000),
            default_login_criteria: raw.get_string_list_default(
                "AiPlayerbot.DefaultLoginCriteria",
                "maxbots,spareroom,offline",
            ),
            login_criteria: Vec::new(), // assigned below

            jump_in_bg: raw.get_bool("AiPlayerbot.JumpInBg", false),
            jump_with_player: raw.get_bool("AiPlayerbot.JumpWithPlayer", false),
            jump_follow: raw.get_bool("AiPlayerbot.JumpFollow", true),
            jump_chase: raw.get_bool("AiPlayerbot.JumpChase", true),
            use_knockback: raw.get_bool("AiPlayerbot.UseKnockback", true),
            jump_no_combat_chance: raw.get_f32("AiPlayerbot.JumpNoCombatChance", 0.5),
            jump_melee_in_combat_chance: raw
                .get_f32("AiPlayerbot.JumpMeleeInCombatChance", 0.5),
            jump_random_chance: raw.get_f32("AiPlayerbot.JumpRandomChance", 0.20),
            jump_in_place_chance: raw.get_f32("AiPlayerbot.JumpInPlaceChance", 0.50),
            jump_backward_chance: raw.get_f32("AiPlayerbot.JumpBackwardChance", 0.10),
            jump_height_limit: raw.get_f32("AiPlayerbot.JumpHeightLimit", 60.0),
            jump_v_speed: raw.get_f32("AiPlayerbot.JumpVSpeed", 7.96),
            jump_h_speed: raw.get_f32("AiPlayerbot.JumpHSpeed", 7.0),

            allowed_log_files: raw.get_string_list("AiPlayerbot.AllowedLogFiles"),
            debug_filter: raw.get_string_list_default(
                "AiPlayerbot.DebugFilter",
                "add gathering loot,check values,emote,check mount state,jump",
            ),

            bot_cheat_mask: parse_cheat_mask(raw.get_string(
                "AiPlayerbot.BotCheats",
                "taxi,item,breath",
            )),
            rnd_bot_cheat_mask: parse_cheat_mask(raw.get_string(
                "AiPlayerbot.RndBotCheats",
                "taxi,item,breath",
            )),

            world_buffs: Vec::new(), // assigned below

            command_server_port: raw.get_i32("AiPlayerbot.CommandServerPort", 0),
            perf_mon_enabled: raw.get_bool("AiPlayerbot.PerfMonEnabled", false),
            explicit_db_store_save: raw.get_bool("AiPlayerbot.ExplicitDbStoreSave", false),

            llm_api_endpoint: raw
                .get_string(
                    "AiPlayerbot.LLMApiEndpoint",
                    "http://127.0.0.1:5001/api/v1/generate",
                )
                .to_string(),
            llm_api_key: raw.get_string("AiPlayerbot.LLMApiKey", "").to_string(),
            llm_api_json: raw
                .get_string(
                    "AiPlayerbot.LLMApiJson",
                    "{ \"max_length\": 100, \"prompt\": \"[<pre prompt>]<context> <prompt> <post prompt>\"}",
                )
                .to_string(),
            llm_pre_prompt: raw
                .get_string(
                    "AiPlayerbot.LLMPrePrompt",
                    "You are a roleplaying character in World of Warcraft: <expansion name>. Your name is <bot name>. The <other type> <other name> is speaking to you <channel name> and is an <other gender> <other race> <other class> of level <other level>. You are level <bot level> and play as a <bot gender> <bot race> <bot class> that is currently in <bot subzone> <bot zone>. Answer as a roleplaying character. Limit responses to 100 characters.",
                )
                .to_string(),
            llm_pre_rpg_prompt: raw
                .get_string(
                    "AiPlayerbot.LLMRpgPrompt",
                    "In World of Warcraft: <expansion name> in <bot zone> <bot subzone> stands <bot type> <bot name> a level <bot level> <bot gender> <bot race> <bot class>. Standing nearby is <unit type> <unit name> <unit subname> a level <unit level> <unit gender> <unit race> <unit faction> <unit class>. Answer as a roleplaying character. Limit responses to 100 characters.",
                )
                .to_string(),
            llm_prompt: raw
                .get_string("AiPlayerbot.LLMPrompt", "<receiver name>:<initial message>")
                .to_string(),
            llm_post_prompt: raw
                .get_string("AiPlayerbot.LLMPostPrompt", "<sender name>:")
                .to_string(),
            llm_response_start_pattern: raw
                .get_string(
                    "AiPlayerbot.LLMResponseStartPattern",
                    r#"("text":\s*")"#,
                )
                .to_string(),
            llm_response_end_pattern: raw
                .get_string(
                    "AiPlayerbot.LLMResponseEndPattern",
                    r#"("|\b(?!<sender name>\b)(\w+):)"#,
                )
                .to_string(),
            llm_response_delete_pattern: raw
                .get_string(
                    "AiPlayerbot.LLMResponseDeletePattern",
                    r"(\\n|<sender name>:|\\[^ ]+)",
                )
                .to_string(),
            llm_response_split_pattern: raw
                .get_string(
                    "AiPlayerbot.LLMResponseSplitPattern",
                    r"(\*.*?\*)|(\[.*?\])|(\'.*\')|([^\*\[\] ][^\*\[\]]+?[.?!])",
                )
                .to_string(),
            llm_enabled: raw.get_u32("AiPlayerbot.LLMEnabled", 1),
            llm_context_length: raw.get_u32("AiPlayerbot.LLMContextLength", 4096),
            llm_bot_to_bot_chat_chance: raw.get_u32("AiPlayerbot.LLMBotToBotChatChance", 0),
            llm_generation_timeout: raw.get_u32("AiPlayerbot.LLMGenerationTimeout", 600),
            llm_max_simultanious_generations: raw
                .get_u32("AiPlayerbot.LLMMaxSimultaniousGenerations", 100),
            llm_rpg_ai_chat_chance: raw.get_u32("AiPlayerbot.LLMRpgAIChatChance", 100),
            llm_global_context: raw.get_bool("AiPlayerbot.LLMGlobalContext", false),
            llm_end_point_url: ParsedLlmUrl::default(),

            eat_drink_min_distance: 5,
            eat_drink_max_distance: 1000,

            // Convenience mirrors — populated from the canonical fields below.
            react_delay_ms: 0,
            max_wait_for_move_ms: 0,
            attacker_refresh_ms: 0,
            nearby_refresh_ms: 0,
            attacker_scan_range: 0.0,
            nearby_scan_range: 0.0,
            eat_hp_threshold: 0.0,
            drink_mana_threshold: 0.0,
            debug: false,
        };

        // ── Derived / cross-cutting passes ────────────────────────────

        // Random bot maps: default list and parsed vec.
        cfg.random_bot_maps_as_string = raw
            .get_string("AiPlayerbot.RandomBotMaps", "0,1,530,571")
            .to_string();
        cfg.random_bot_maps = parse_u32_csv(&cfg.random_bot_maps_as_string);

        // Toggle-always-online lists get case-normalised as in the C++
        // original: account names uppercased, character names
        // lowercased with the first letter capitalised.
        cfg.toggle_always_online_accounts = raw
            .get_string_list("AiPlayerbot.ToggleAlwaysOnlineAccounts")
            .into_iter()
            .map(|s| s.to_uppercase())
            .collect();
        cfg.toggle_always_online_chars = raw
            .get_string_list("AiPlayerbot.ToggleAlwaysOnlineChars")
            .into_iter()
            .map(|s| {
                let mut lc = s.to_lowercase();
                if let Some(first) = lc.get_mut(0..1) {
                    first.make_ascii_uppercase();
                }
                lc
            })
            .collect();

        // sync_level_no_player defaults to randombot_starting_level (not a fixed constant).
        cfg.sync_level_no_player = raw
            .get_u32("AiPlayerbot.SyncLevelNoPlayer", cfg.randombot_starting_level);

        // broadcast_chance_max_value gates all broadcast chances to 30000 when enabled, else 0.
        cfg.broadcast_chance_max_value = if cfg.enable_broadcasts { 30000 } else { 0 };

        // Level probability: per-level key overrides, default 100.
        for level in 1..MAX_LEVEL_PROBABILITY_SLOTS {
            let key = format!("AiPlayerbot.LevelProbability.{level}");
            cfg.level_probability[level] = raw.get_u32(&key, 100);
        }

        // Class/Race probability matrix — defaults first, overrides last.
        // Race-only keys: AiPlayerbot.ClassRaceProb.0.<race>
        for race in 1..MAX_RACES {
            let rprob = raw.get_i32(
                &format!("AiPlayerbot.ClassRaceProb.0.{race}"),
                100,
            );
            if rprob >= 0 {
                for cls in 1..MAX_CLASSES {
                    cfg.class_race_probability[cls][race] = rprob as u32;
                }
            }
        }
        // Class-only keys: AiPlayerbot.ClassRaceProb.<cls>
        for cls in 1..MAX_CLASSES {
            let cprob = raw.get_i32(&format!("AiPlayerbot.ClassRaceProb.{cls}"), -1);
            if cprob >= 0 {
                for race in 1..MAX_RACES {
                    cfg.class_race_probability[cls][race] = cprob as u32;
                }
            }
        }
        // Race/class overrides: AiPlayerbot.ClassRaceProb.<cls>.<race>
        for race in 1..MAX_RACES {
            for cls in 1..MAX_CLASSES {
                let rcprob = raw.get_i32(
                    &format!("AiPlayerbot.ClassRaceProb.{cls}.{race}"),
                    -1,
                );
                if rcprob >= 0 {
                    cfg.class_race_probability[cls][race] = rcprob as u32;
                }
            }
        }
        // NOTE: `factory.isAvailableRace(cls, race)` zeroing the entries
        // for invalid class/race combinations happens in the C++ shim
        // which has access to the `RandomPlayerbotFactory` runtime data.
        // The Rust parser leaves the matrix as-loaded; the shim masks
        // unavailable combinations and computes
        // `class_race_probability_total` after masking.
        // For the Rust hot path this field is currently unused, but it's
        // kept for future parity and so the data is available via FFI.
        cfg.class_race_probability_total = cfg
            .class_race_probability
            .iter()
            .flat_map(|row| row.iter().copied())
            .sum();

        // Fixed class/race counts (opt-in).
        if cfg.use_fixed_class_race_counts {
            for cls in 1..MAX_CLASSES {
                for race in 1..MAX_RACES {
                    let key = format!("AiPlayerbot.ClassRaceProb.{cls}.{race}");
                    let count = raw.get_i32(&key, -1);
                    if count >= 0 {
                        cfg.fixed_class_race_counts
                            .insert((cls as u8, race as u8), count as u32);
                    }
                }
            }
        }

        // World buffs: enumerate all `AiPlayerbot.WorldBuff*` keys.
        for key in raw.keys_containing("AiPlayerbot.WorldBuff") {
            let ids: Vec<&str> = key.split('.').collect();
            let mut params = [0u32; 6];
            for (i, slot) in params.iter_mut().enumerate() {
                if let Some(p) = ids.get(i + 2) {
                    *slot = p.parse::<u32>().unwrap_or(0);
                }
            }
            let buffs = raw.get_u32_list(&key);
            for buff in buffs {
                cfg.world_buffs.push(WorldBuff {
                    spell_id: buff,
                    faction_id: params[0],
                    class_id: params[1],
                    spec_id: params[2],
                    min_level: params[3],
                    max_level: params[4],
                    event_id: params[5],
                });
            }
        }

        // Login criteria: enumerate all `AiPlayerbot.LoginCriteria*` keys
        // (sorted).
        for key in raw.keys_containing("AiPlayerbot.LoginCriteria") {
            let csv = raw.get_string(&key, "");
            cfg.login_criteria
                .push(csv.split(',').map(str::trim).map(str::to_string).collect());
        }
        if cfg.login_criteria.is_empty() {
            cfg.login_criteria = vec![
                vec!["group".to_string()],
                vec!["arena".to_string()],
                vec!["bg".to_string()],
                vec!["guild".to_string()],
                vec!["logoff,classrace,level,online".to_string()],
                vec!["logoff,classrace,level".to_string()],
                vec!["logoff,classrace".to_string()],
            ];
        }

        // Gear progression: phases × classes × specs × slots.
        for phase in 0..MAX_GEAR_PROGRESSION_LEVEL {
            cfg.gear_progression_system_item_levels[phase][0] = raw.get_u32(
                &format!("AiPlayerbot.GearProgressionSystem.{phase}.MinItemLevel"),
                9_999_999,
            );
            cfg.gear_progression_system_item_levels[phase][1] = raw.get_u32(
                &format!("AiPlayerbot.GearProgressionSystem.{phase}.MaxItemLevel"),
                9_999_999,
            );

            for cls in 1..MAX_CLASSES {
                for spec in 0..4 {
                    for slot in 0..19 {
                        let key = format!(
                            "AiPlayerbot.GearProgressionSystem.{phase}.{cls}.{spec}.{slot}"
                        );
                        cfg.gear_progression_system_items[phase][cls][spec][slot] =
                            raw.get_i32(&key, -1);
                    }
                }
            }
        }

        // Parse the LLM URL out of `llm_api_endpoint`.
        cfg.llm_end_point_url = parse_llm_url(&cfg.llm_api_endpoint);

        // ── Convenience mirror fields for the Rust hot path ───────────
        cfg.react_delay_ms = cfg.react_delay;
        cfg.max_wait_for_move_ms = cfg.max_wait_for_move;
        cfg.attacker_refresh_ms = u64::from(cfg.react_delay) * 5;
        cfg.nearby_refresh_ms = u64::from(cfg.react_delay) * 10;
        cfg.attacker_scan_range = cfg.aggro_distance;
        cfg.nearby_scan_range = cfg.sight_distance;
        cfg.eat_hp_threshold = if cfg.low_health > 0 {
            cfg.low_health as f32 / 100.0
        } else {
            0.75
        };
        cfg.drink_mana_threshold = if cfg.medium_mana > 0 {
            cfg.medium_mana as f32 / 100.0
        } else {
            0.75
        };
        cfg.debug = !cfg.debug_filter.is_empty();

        cfg
    }
}

/// Parse the `BotCheats` / `RndBotCheats` comma-separated name list
/// into a bitmask. Matches the inline lambda in the C++ `Initialize()`.
fn parse_cheat_mask(names: &str) -> u32 {
    const NAMES: &[&str] = &[
        "taxi",
        "gold",
        "health",
        "mana",
        "power",
        "item",
        "cooldown",
        "repair",
        "movespeed",
        "attackspeed",
        "breath",
        "glyph",
        "quest",
        "maxMask",
    ];
    let mut mask = 0u32;
    for name in names.split(',') {
        let t = name.trim();
        if t.is_empty() {
            continue;
        }
        for (i, candidate) in NAMES.iter().enumerate() {
            if *candidate == t {
                mask |= 1 << i;
                break;
            }
        }
    }
    mask
}

fn parse_u32_csv(csv: &str) -> Vec<u32> {
    csv.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<u32>().ok())
        .collect()
}

/// Parse `http(s)://host[:port][/path]` into a `ParsedLlmUrl`. Matches
/// the regex-based `parseUrl` helper in the original `.cpp` file,
/// though this implementation uses explicit parsing instead of a
/// regex dep.
fn parse_llm_url(url: &str) -> ParsedLlmUrl {
    let (scheme, rest) = if let Some(s) = url.strip_prefix("https://") {
        ("https", s)
    } else if let Some(s) = url.strip_prefix("http://") {
        ("http", s)
    } else {
        return ParsedLlmUrl::default();
    };
    let https = scheme == "https";

    // Split host[:port] from path.
    let (host_part, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };
    let (hostname, port) = match host_part.find(':') {
        Some(idx) => {
            let host = &host_part[..idx];
            let port = host_part[idx + 1..].parse::<u16>().unwrap_or(if https {
                443
            } else {
                80
            });
            (host.to_string(), port)
        }
        None => (host_part.to_string(), if https { 443 } else { 80 }),
    };
    ParsedLlmUrl {
        hostname,
        path: path.to_string(),
        port,
        https,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_legacy_rust_fields() {
        let cfg = BotConfig::default();
        assert_eq!(cfg.react_delay, 100);
        assert_eq!(cfg.max_wait_for_move, 3000);
        assert_eq!(cfg.react_delay_ms, 100);
        assert_eq!(cfg.max_wait_for_move_ms, 3000);
        assert!((cfg.attacker_scan_range - 22.0).abs() < 0.01);
        assert!((cfg.nearby_scan_range - 75.0).abs() < 0.01);
        assert!((cfg.eat_hp_threshold - 0.50).abs() < 0.01);
        assert!((cfg.drink_mana_threshold - 0.40).abs() < 0.01);
    }

    #[test]
    fn parsing_turns_on_fields() {
        let raw = RawConfig::parse_str(
            "AiPlayerbot.Enabled = 1\n\
             AiPlayerbot.ReactDelay = 200\n\
             AiPlayerbot.MaxWaitForMove = 5000\n\
             AiPlayerbot.LowHealth = 40\n\
             AiPlayerbot.MediumMana = 30\n\
             AiPlayerbot.DebugFilter = x\n",
        );
        let cfg = BotConfig::from_raw(&raw);
        assert!(cfg.enabled);
        assert_eq!(cfg.react_delay, 200);
        assert_eq!(cfg.react_delay_ms, 200);
        assert_eq!(cfg.max_wait_for_move, 5000);
        assert_eq!(cfg.attacker_refresh_ms, 1000);
        assert_eq!(cfg.nearby_refresh_ms, 2000);
        assert!((cfg.eat_hp_threshold - 0.40).abs() < 0.01);
        assert!((cfg.drink_mana_threshold - 0.30).abs() < 0.01);
        assert!(cfg.debug);
    }

    #[test]
    fn parses_world_buffs_dynamic_keys() {
        let raw = RawConfig::parse_str(
            "AiPlayerbot.WorldBuff.0.0 = 100,200\n\
             AiPlayerbot.WorldBuff.1.0.0 = 300\n",
        );
        let cfg = BotConfig::from_raw(&raw);
        // Three total entries: 100 + 200 + 300.
        assert_eq!(cfg.world_buffs.len(), 3);
        let spell_ids: Vec<u32> = cfg.world_buffs.iter().map(|b| b.spell_id).collect();
        assert!(spell_ids.contains(&100));
        assert!(spell_ids.contains(&200));
        assert!(spell_ids.contains(&300));
    }

    #[test]
    fn parses_login_criteria_dynamic_keys() {
        let raw = RawConfig::parse_str(
            "AiPlayerbot.LoginCriteria1 = group\n\
             AiPlayerbot.LoginCriteria2 = arena,bg\n",
        );
        let cfg = BotConfig::from_raw(&raw);
        assert_eq!(cfg.login_criteria.len(), 2);
    }

    #[test]
    fn login_criteria_falls_back_to_defaults() {
        let cfg = BotConfig::default();
        assert_eq!(cfg.login_criteria.len(), 7);
        assert_eq!(cfg.login_criteria[0], vec!["group".to_string()]);
    }

    #[test]
    fn parses_cheat_mask() {
        assert_eq!(parse_cheat_mask("taxi"), 1);
        assert_eq!(parse_cheat_mask("taxi,item,breath"), 1 | (1 << 5) | (1 << 10));
        assert_eq!(parse_cheat_mask(""), 0);
        assert_eq!(parse_cheat_mask("unknown"), 0);
    }

    #[test]
    fn parses_llm_url() {
        let url = parse_llm_url("http://localhost:5001/api/v1/generate");
        assert_eq!(url.hostname, "localhost");
        assert_eq!(url.port, 5001);
        assert_eq!(url.path, "/api/v1/generate");
        assert!(!url.https);

        let url = parse_llm_url("https://api.example.com/v1");
        assert_eq!(url.hostname, "api.example.com");
        assert_eq!(url.port, 443);
        assert_eq!(url.path, "/v1");
        assert!(url.https);

        let url = parse_llm_url("http://host");
        assert_eq!(url.hostname, "host");
        assert_eq!(url.port, 80);
        assert_eq!(url.path, "/");
    }

    #[test]
    fn toggle_always_online_case_normalised() {
        let raw = RawConfig::parse_str(
            "AiPlayerbot.ToggleAlwaysOnlineAccounts = admin,User\n\
             AiPlayerbot.ToggleAlwaysOnlineChars = boss,thrall\n",
        );
        let cfg = BotConfig::from_raw(&raw);
        assert_eq!(cfg.toggle_always_online_accounts, vec!["ADMIN", "USER"]);
        assert_eq!(cfg.toggle_always_online_chars, vec!["Boss", "Thrall"]);
    }

    #[test]
    fn broadcast_max_value_gated_by_enable_flag() {
        let on = BotConfig::from_raw(&RawConfig::parse_str("AiPlayerbot.EnableBroadcasts = 1"));
        assert_eq!(on.broadcast_chance_max_value, 30000);
        let off = BotConfig::from_raw(&RawConfig::parse_str("AiPlayerbot.EnableBroadcasts = 0"));
        assert_eq!(off.broadcast_chance_max_value, 0);
    }

    #[test]
    fn sync_level_no_player_inherits_starting_level() {
        let raw = RawConfig::parse_str("AiPlayerbot.randombotStartingLevel = 25");
        let cfg = BotConfig::from_raw(&raw);
        assert_eq!(cfg.sync_level_no_player, 25);
    }

    #[test]
    fn sync_level_no_player_can_be_overridden() {
        let raw = RawConfig::parse_str(
            "AiPlayerbot.randombotStartingLevel = 25\n\
             AiPlayerbot.SyncLevelNoPlayer = 50\n",
        );
        let cfg = BotConfig::from_raw(&raw);
        assert_eq!(cfg.sync_level_no_player, 50);
    }

    #[test]
    fn class_race_probability_levels_stack() {
        let raw = RawConfig::parse_str(
            "AiPlayerbot.ClassRaceProb.0.1 = 80\n\
             AiPlayerbot.ClassRaceProb.2 = 50\n\
             AiPlayerbot.ClassRaceProb.2.3 = 25\n",
        );
        let cfg = BotConfig::from_raw(&raw);
        // Race 1 defaults: all classes get 80.
        assert_eq!(cfg.class_race_probability[1][1], 80);
        assert_eq!(cfg.class_race_probability[4][1], 80);
        // Class 2 override: race 2 becomes 50.
        assert_eq!(cfg.class_race_probability[2][2], 50);
        // Class 2 Race 3 specific: 25.
        assert_eq!(cfg.class_race_probability[2][3], 25);
    }

    #[test]
    fn gear_progression_defaults() {
        let cfg = BotConfig::default();
        for phase in 0..MAX_GEAR_PROGRESSION_LEVEL {
            assert_eq!(cfg.gear_progression_system_item_levels[phase][0], 9_999_999);
            assert_eq!(cfg.gear_progression_system_item_levels[phase][1], 9_999_999);
        }
        // All slot entries default to -1.
        assert_eq!(cfg.gear_progression_system_items[0][1][0][0], -1);
    }
}
