//! Per-bot throttle state for `Bt::Throttle` nodes.
//!
//! The behavior tree is stateless and may be shared between bots (e.g. encounter
//! `phase_bt()` returns a borrowed reference). Time-based throttling therefore
//! cannot live inside the tree — it has to be keyed by call site and stored on
//! the bot.
//!
//! Keys are captured automatically at tree construction via `#[track_caller]`
//! in [`crate::engine::bt::Bt::throttle`], so callers don't need to invent
//! names. `(file, line)` is stable per source location and unique enough for
//! our purposes (collisions would only happen if two throttles were declared
//! on the same line, which is not something we do).

use std::collections::HashMap;

/// Stable identifier for a single `Bt::Throttle` call site.
///
/// Captured by `Bt::throttle()` via `std::panic::Location::caller()`. The
/// `file` pointer is a `&'static str` into the compiler-generated string
/// table, so comparing and hashing by `(file, line)` is cheap and stable for
/// the lifetime of the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThrottleKey {
    pub file: &'static str,
    pub line: u32,
}

/// Per-bot last-fire timestamps for every `Bt::Throttle` node that has been
/// entered at least once.
///
/// Lives on `BotState` and is reached through `TickContext::throttles`.
#[derive(Debug, Default)]
pub struct Throttles {
    last_fire_ms: HashMap<ThrottleKey, u64>,
}

impl Throttles {
    pub fn new() -> Self {
        Self::default()
    }

    /// Last time this throttle fired, or 0 if it has never fired.
    pub fn last_fire(&self, key: ThrottleKey) -> u64 {
        self.last_fire_ms.get(&key).copied().unwrap_or(0)
    }

    /// Record that this throttle has just fired at `now_ms`.
    pub fn mark_fired(&mut self, key: ThrottleKey, now_ms: u64) {
        self.last_fire_ms.insert(key, now_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_key_returns_zero() {
        let t = Throttles::new();
        let k = ThrottleKey { file: "a.rs", line: 1 };
        assert_eq!(t.last_fire(k), 0);
    }

    #[test]
    fn mark_and_read() {
        let mut t = Throttles::new();
        let k = ThrottleKey { file: "a.rs", line: 1 };
        t.mark_fired(k, 1234);
        assert_eq!(t.last_fire(k), 1234);
    }

    #[test]
    fn distinct_keys_independent() {
        let mut t = Throttles::new();
        let k1 = ThrottleKey { file: "a.rs", line: 1 };
        let k2 = ThrottleKey { file: "a.rs", line: 2 };
        t.mark_fired(k1, 100);
        t.mark_fired(k2, 200);
        assert_eq!(t.last_fire(k1), 100);
        assert_eq!(t.last_fire(k2), 200);
    }
}
