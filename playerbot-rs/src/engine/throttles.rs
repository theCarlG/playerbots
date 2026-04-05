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
//!
//! ## Running-transparency
//!
//! A throttle that wraps a long-running action (e.g. `RpgWander` → `move_to`
//! → `BtResult::Running`) must *not* interrupt that action when the cooldown
//! window has not yet elapsed. Before running-transparency, the sequence was:
//!
//! 1. tick N: child picks a target, calls `move_to`, returns Running. Throttle
//!    records `last_fire = N`.
//! 2. tick N+1: throttle sees `now - last_fire < interval`, returns Failure
//!    **without** ticking the child. The parent `Sel` falls through to the
//!    next arm — typically `StopMoving` — and halts the bot mid-movement.
//!
//! We fix this by tracking a "running" flag per throttle key. If the child
//! returned Running on the previous tick, the throttle bypasses its cooldown
//! check and ticks the child directly, letting the in-progress action
//! continue. The flag clears as soon as the child returns Success or Failure.

use std::collections::{HashMap, HashSet};

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
    running: HashSet<ThrottleKey>,
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

    /// Is the child of this throttle currently in a Running phase (i.e. its
    /// last tick returned [`crate::engine::bt::BtResult::Running`])?
    pub fn is_running(&self, key: ThrottleKey) -> bool {
        self.running.contains(&key)
    }

    /// Set or clear the running flag for a throttle key.
    pub fn set_running(&mut self, key: ThrottleKey, running: bool) {
        if running {
            self.running.insert(key);
        } else {
            self.running.remove(&key);
        }
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

    #[test]
    fn running_flag_set_and_cleared() {
        let mut t = Throttles::new();
        let k = ThrottleKey { file: "a.rs", line: 1 };
        assert!(!t.is_running(k));
        t.set_running(k, true);
        assert!(t.is_running(k));
        t.set_running(k, false);
        assert!(!t.is_running(k));
    }
}
