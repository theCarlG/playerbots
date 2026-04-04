/// Bot configuration — populated once from C++ at init time.
///
/// Stored in a `OnceLock` singleton so any Rust code can read config values
/// without calling into C++. Zero-cost after initialization.
///
/// C++ calls `playerbot_set_config()` during server startup (before any bots
/// are created). Rust code reads via `config::get()`.
use std::sync::OnceLock;

static CONFIG: OnceLock<BotConfig> = OnceLock::new();

/// All bot configuration values that Rust AI code may need.
///
/// Populated from C++ `PlayerbotAIConfig` at startup.
/// All fields have sensible defaults matching the C++ config defaults.
#[derive(Debug, Clone)]
pub struct BotConfig {
    /// Base AI tick interval in milliseconds (default: 100).
    pub react_delay_ms: u32,

    /// Maximum delay before a bot is considered inactive (default: 5000).
    pub max_wait_for_move_ms: u32,

    /// How often to scan for hostile nearby units (default: 500).
    pub attacker_refresh_ms: u64,

    /// How often to scan for all nearby units (default: 1000).
    pub nearby_refresh_ms: u64,

    /// Distance to search for hostile units (default: 40.0).
    pub attacker_scan_range: f32,

    /// Distance to search for all nearby units (default: 60.0).
    pub nearby_scan_range: f32,

    /// HP% threshold below which bots should eat (default: 0.75).
    pub eat_hp_threshold: f32,

    /// Mana% threshold below which bots should drink (default: 0.75).
    pub drink_mana_threshold: f32,

    /// Whether debug logging is enabled (default: false).
    pub debug: bool,
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            react_delay_ms: 100,
            max_wait_for_move_ms: 5000,
            attacker_refresh_ms: 500,
            nearby_refresh_ms: 1000,
            attacker_scan_range: 40.0,
            nearby_scan_range: 60.0,
            eat_hp_threshold: 0.75,
            drink_mana_threshold: 0.75,
            debug: false,
        }
    }
}

/// Get the global config. Returns defaults if `set()` was never called.
pub fn get() -> &'static BotConfig {
    CONFIG.get_or_init(BotConfig::default)
}

/// Set the global config. Called once from `playerbot_set_config()`.
/// Returns `Err` if already set (idempotent — ignored).
pub fn set(config: BotConfig) -> Result<(), BotConfig> {
    CONFIG.set(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_sane() {
        let c = BotConfig::default();
        assert_eq!(c.react_delay_ms, 100);
        assert!(c.eat_hp_threshold > 0.0 && c.eat_hp_threshold <= 1.0);
    }

    #[test]
    fn get_returns_defaults() {
        // In tests, CONFIG may or may not be initialized already.
        let c = get();
        assert!(c.react_delay_ms > 0);
    }
}
