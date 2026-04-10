//! Integration tests for the config parser golden-file fixtures.
//!
//! These exercise the same path the C++ shim hits through FFI:
//! `RawConfig::parse_file` → `BotConfig::from_raw`. Unit-level corner cases
//! for the parser live in `src/config/raw.rs` (parser primitives) and
//! `src/config/typed.rs` (field assignment). This file covers the
//! end-to-end pipeline against real `.conf` fixtures so CI catches anyone
//! quietly breaking the defaults or field wiring.

use std::path::PathBuf;

use playerbot_rs::config::{BotConfig, RawConfig};

fn fixture_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures/config");
    p.push(name);
    p
}

#[test]
fn minimal_parses_to_defaults() {
    let raw = RawConfig::parse_file(&fixture_path("minimal.conf"))
        .expect("fixture should be readable");
    let cfg = BotConfig::from_raw(&raw);
    let dflt = BotConfig::default();

    // Master switches.
    assert_eq!(cfg.enabled, dflt.enabled);
    assert_eq!(cfg.allow_guild_bots, dflt.allow_guild_bots);
    assert_eq!(
        cfg.allow_multi_account_alt_bots,
        dflt.allow_multi_account_alt_bots
    );

    // Timing scalars — sample a handful that have historically drifted.
    assert_eq!(cfg.global_cool_down, 500);
    assert_eq!(cfg.react_delay, 100);
    assert_eq!(cfg.max_wait_for_move, 3000);
    assert_eq!(cfg.loot_delay, 750);

    // Distance scalars.
    assert!((cfg.sight_distance - 75.0).abs() < 0.01);
    assert!((cfg.heal_distance - 125.0).abs() < 0.01);
    assert!((cfg.aggro_distance - 22.0).abs() < 0.01);

    // Health/mana thresholds.
    assert_eq!(cfg.critical_health, 20);
    assert_eq!(cfg.low_health, 50);
    assert_eq!(cfg.medium_health, 70);
    assert_eq!(cfg.almost_full_health, 90);
    assert_eq!(cfg.low_mana, 15);
    assert_eq!(cfg.medium_mana, 40);

    // Lists with documented defaults.
    assert_eq!(
        cfg.random_bot_quest_items,
        vec![6948, 5175, 5176, 5177, 5178, 16309, 12382, 13704, 11000, 22754]
    );
    assert_eq!(cfg.random_bot_spell_ids, vec![54197]);

    // Broadcast gate — `EnableBroadcasts` defaults to true, so the max
    // chance value is the enabled sentinel (30000) even in the minimal file.
    assert!(cfg.enable_broadcasts);
    assert_eq!(cfg.broadcast_chance_max_value, 30000);

    // Rust-side mirrors.
    assert_eq!(cfg.react_delay_ms, cfg.react_delay);
    assert_eq!(cfg.max_wait_for_move_ms, cfg.max_wait_for_move);
    // `debug = !debug_filter.is_empty()`, and the default filter is
    // non-empty, so `cfg.debug` is true even in the minimal fixture.
    assert!(cfg.debug);

    // Dynamic-key defaults: no world buffs, 7 default login criteria.
    assert!(cfg.world_buffs.is_empty());
    assert_eq!(cfg.login_criteria.len(), 7);
}

#[test]
fn full_parses_to_overrides() {
    let raw = RawConfig::parse_file(&fixture_path("full.conf"))
        .expect("fixture should be readable");
    let cfg = BotConfig::from_raw(&raw);

    // Master switches — all inverted from defaults.
    assert!(cfg.enabled);
    assert!(!cfg.allow_guild_bots);
    assert!(!cfg.allow_multi_account_alt_bots);

    // Timing scalars.
    assert_eq!(cfg.global_cool_down, 750);
    assert_eq!(cfg.react_delay, 200);
    assert_eq!(cfg.max_wait_for_move, 5000);
    assert_eq!(cfg.expire_action_time, 9000);
    assert_eq!(cfg.dispel_aura_duration, 3000);
    assert_eq!(cfg.passive_delay, 6000);
    assert_eq!(cfg.repeat_delay, 7000);
    assert_eq!(cfg.error_delay, 8000);
    assert_eq!(cfg.rpg_delay, 4000);
    assert_eq!(cfg.sit_delay, 45000);
    assert_eq!(cfg.return_delay, 9500);
    assert_eq!(cfg.loot_delay, 900);

    // Distances.
    assert!((cfg.sight_distance - 80.0).abs() < 0.01);
    assert!((cfg.spell_distance - 27.5).abs() < 0.01);
    assert!((cfg.react_distance - 160.0).abs() < 0.01);
    assert!((cfg.grind_distance - 82.0).abs() < 0.01);
    assert!((cfg.loot_distance - 28.0).abs() < 0.01);
    assert!((cfg.heal_distance - 130.0).abs() < 0.01);
    assert!((cfg.aggro_distance - 24.0).abs() < 0.01);

    // Health / mana.
    assert_eq!(cfg.critical_health, 25);
    assert_eq!(cfg.low_health, 55);
    assert_eq!(cfg.medium_health, 75);
    assert_eq!(cfg.almost_full_health, 95);
    assert_eq!(cfg.low_mana, 20);
    assert_eq!(cfg.medium_mana, 45);

    // Random bots.
    assert!(!cfg.random_bot_autologin);
    assert_eq!(cfg.random_bot_maps, vec![0, 1, 530]);
    assert_eq!(cfg.random_bot_quest_items, vec![1234, 5678]);
    assert_eq!(cfg.random_bot_spell_ids, vec![99, 100, 101]);
    assert!(!cfg.enable_random_teleports);
    assert_eq!(cfg.random_bot_teleport_distance, 1500);
    assert_eq!(cfg.random_gear_max_level, 450);
    assert_eq!(cfg.min_random_bots, 10);
    assert_eq!(cfg.max_random_bots, 100);
    assert_eq!(cfg.random_bot_update_interval, 2000);

    // Broadcasts.
    assert_eq!(cfg.broadcast_chance_max_value, 30000);
    assert_eq!(cfg.broadcast_to_guild_global_chance, 25);
    assert_eq!(cfg.broadcast_to_world_global_chance, 30);

    // Commands / chat.
    assert_eq!(cfg.command_prefix, "!");
    assert_eq!(cfg.command_separator, " ");
    assert_eq!(cfg.random_bot_account_prefix, "TEST_");
    assert_eq!(cfg.random_bot_account_count, 40);

    // World buffs — three entries total across the two WorldBuff.* keys.
    assert_eq!(cfg.world_buffs.len(), 3);
    let spell_ids: Vec<u32> = cfg.world_buffs.iter().map(|b| b.spell_id).collect();
    for expected in [100u32, 200, 300] {
        assert!(
            spell_ids.contains(&expected),
            "expected world buff spell {expected} in {spell_ids:?}"
        );
    }

    // Login criteria (only the two `LoginCriteria{1,2}` entries when any
    // LoginCriteria key is present).
    assert_eq!(cfg.login_criteria.len(), 2);
    assert_eq!(cfg.login_criteria[0], vec!["group".to_string()]);
    assert_eq!(
        cfg.login_criteria[1],
        vec!["arena".to_string(), "bg".to_string()]
    );

    // LLM.
    assert_eq!(
        cfg.llm_api_endpoint,
        "http://localhost:5001/api/v1/generate"
    );
    assert_eq!(cfg.llm_enabled, 1);
    assert_eq!(cfg.llm_context_length, 4096);
    assert_eq!(cfg.llm_end_point_url.hostname, "localhost");
    assert_eq!(cfg.llm_end_point_url.port, 5001);
    assert_eq!(cfg.llm_end_point_url.path, "/api/v1/generate");
    assert!(!cfg.llm_end_point_url.https);

    // Cheat masks — indices in `parse_cheat_mask`'s NAMES table:
    //   taxi = 0, gold = 1, item = 5
    // so "taxi,item" = 1 | (1<<5) = 33 and "taxi,gold" = 1 | (1<<1) = 3.
    assert_eq!(cfg.bot_cheat_mask, 1 | (1 << 5));
    assert_eq!(cfg.rnd_bot_cheat_mask, 1 | (1 << 1));

    // Class/race probability matrix — the file sets three overrides; ensure
    // the stacking rules line up with the direct unit test.
    assert_eq!(cfg.class_race_probability[1][1], 80);
    assert_eq!(cfg.class_race_probability[4][1], 80);
    assert_eq!(cfg.class_race_probability[2][2], 50);
    assert_eq!(cfg.class_race_probability[2][3], 25);

    // Debug-filter mirror.
    assert!(cfg.debug);
}
