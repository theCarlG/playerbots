//! Bot configuration module — Rust owns the `.conf` parser.
//!
//! Phase D of the C++ → Rust migration replaced `playerbot/PlayerbotAIConfig.cpp`
//! (1,016 LOC) with a native Rust parser. The C++ side keeps a thin
//! compatibility shim at `cpp_wrapper/BotConfig.{h,cpp}` that calls into
//! this module via the FFI surface in [`ffi`].
//!
//! ## Architecture
//!
//! * [`raw::RawConfig`] — low-level key/value store with `CMaNGOS` `.conf`
//!   parsing semantics (line comments, section headers, env-var
//!   overrides, typed getters with defaults, CSV list parsing).
//! * [`typed::BotConfig`] — strongly-typed view derived from a
//!   `RawConfig` via [`typed::BotConfig::from_raw`]. Contains every
//!   field the C++ `PlayerbotAIConfig` class used to expose, plus a
//!   handful of convenience mirrors that match the original 9-field
//!   stub so the Rust hot path (`bot/tick.rs`) needs no changes.
//! * [`ffi`] — `extern "C"` surface: parse/load, by-key scalar and list
//!   getters, typed accessors for the pre-derived array fields, and
//!   dynamic-key iteration helpers.
//!
//! ## Singleton
//!
//! The parsed config lives in an `OnceLock<ArcSwap<ConfigState>>`. The
//! `OnceLock` lets the first `config_load` call install the state; the
//! `ArcSwap` allows a `/bot reload` GM command to atomically swap the
//! state without blocking hot-path readers. Reading via [`get`] is
//! lock-free.
//!
//! `ConfigState` bundles both the raw map and the derived typed view so
//! the FFI can answer by-key queries (used by the C++ shim) and the
//! Rust hot path can read `cfg.nearby_scan_range` in one atomic load.

use std::sync::OnceLock;

use arc_swap::ArcSwap;

pub mod ffi;
pub mod raw;
pub mod typed;

pub use raw::RawConfig;
pub use typed::{BotConfig, MAX_CLASSES, MAX_GEAR_PROGRESSION_LEVEL, MAX_RACES, WorldBuff};

/// Bundled state held by the global singleton.
///
/// Both `raw` and `typed` are derived from the same parse pass: `raw`
/// is the low-level key/value store, `typed` is the strongly-typed view
/// via [`BotConfig::from_raw`].
#[derive(Debug, Default, Clone)]
pub struct ConfigState {
    pub raw: RawConfig,
    pub typed: BotConfig,
}

impl ConfigState {
    /// Build a `ConfigState` from an already-parsed `RawConfig`.
    pub fn from_raw(raw: RawConfig) -> Self {
        let typed = BotConfig::from_raw(&raw);
        Self { raw, typed }
    }
}

static STATE: OnceLock<ArcSwap<ConfigState>> = OnceLock::new();

fn state() -> &'static ArcSwap<ConfigState> {
    STATE.get_or_init(|| ArcSwap::from_pointee(ConfigState::default()))
}

/// Install a fresh `ConfigState`, atomically replacing the current one.
///
/// Called by [`ffi::playerbot_config_load`] after a file parse and by
/// tests that want to inject a known configuration.
pub fn install(state_in: ConfigState) {
    state().store(std::sync::Arc::new(state_in));
}

/// Snapshot of the typed config.
///
/// Returns a `BotConfigGuard` that derefs to `BotConfig`. The guard
/// holds an `Arc<ConfigState>` — cheap to clone, lock-free, and
/// independent of subsequent `/bot reload` calls.
pub fn get() -> BotConfigGuard {
    BotConfigGuard {
        inner: state().load_full(),
    }
}

/// RAII guard that exposes a snapshot of the typed [`BotConfig`].
///
/// Implements `Deref<Target = BotConfig>` so existing call sites that
/// read `cfg.field` keep working unchanged. Holding the guard prevents
/// the underlying `ConfigState` from being freed while in use, but does
/// not block other readers or `install` calls.
#[derive(Debug, Clone)]
pub struct BotConfigGuard {
    inner: std::sync::Arc<ConfigState>,
}

impl std::ops::Deref for BotConfigGuard {
    type Target = BotConfig;

    fn deref(&self) -> &Self::Target {
        &self.inner.typed
    }
}

impl BotConfigGuard {
    /// Access the underlying raw config store — for FFI by-key lookups
    /// and tests.
    pub fn raw(&self) -> &RawConfig {
        &self.inner.raw
    }
}

/// Shared test mutex used by all tests that mutate the global singleton
/// (both `mod.rs` tests and `ffi.rs` tests). Without serialisation, parallel
/// test execution races `install` calls against each other and produces
/// spurious failures.
#[cfg(test)]
pub(crate) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_guard_is_sane() {
        let _g = TEST_LOCK.lock().unwrap();
        install(ConfigState::default());
        let cfg = get();
        // Default `BotConfig` still satisfies the old stub invariants.
        assert!(cfg.react_delay_ms > 0);
        assert!(cfg.eat_hp_threshold > 0.0 && cfg.eat_hp_threshold <= 1.0);
    }

    #[test]
    fn install_replaces_state() {
        let _g = TEST_LOCK.lock().unwrap();
        let raw = RawConfig::parse_str("AiPlayerbot.ReactDelay = 250");
        install(ConfigState::from_raw(raw));
        let cfg = get();
        assert_eq!(cfg.react_delay, 250);
        assert_eq!(cfg.react_delay_ms, 250);

        // Restore defaults so the test doesn't bleed into sibling tests.
        install(ConfigState::default());
    }

    #[test]
    fn guard_exposes_raw_for_ffi() {
        let _g = TEST_LOCK.lock().unwrap();
        let raw = RawConfig::parse_str("Custom.Key = 42");
        install(ConfigState::from_raw(raw));
        let cfg = get();
        assert_eq!(cfg.raw().get_u32("Custom.Key", 0), 42);
        install(ConfigState::default());
    }
}
