use cmangos::SpellId;
/// Per-bot combat timers: GCD and per-spell cooldowns.
///
/// Tracked in Rust so BT nodes can check cooldowns without calling into C++.
/// Populated by BT action nodes when spells are cast successfully.
use std::collections::HashMap;

/// GCD duration in milliseconds (1500ms standard).
// const GCD_MS: u64 = 1500;
/// Off-GCD margin: spells with cast time > 0 don't trigger full GCD.
/// We always track full GCD for simplicity; real off-GCD handling is per-spell.
const INSTANT_GCD_MS: u64 = 1500;

#[derive(Debug, Default)]
pub struct BotTimers {
    /// `spell_id` → `server_time_ms` when this spell is next usable
    cooldowns: HashMap<SpellId, u64>,
    /// `server_time_ms` when GCD expires
    gcd_ready_at: u64,
}

impl BotTimers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if the spell is currently on cooldown.
    pub fn spell_on_cooldown(&self, spell_id: SpellId, now_ms: u64) -> bool {
        self.cooldowns
            .get(&spell_id)
            .is_some_and(|&ready_at| now_ms < ready_at)
    }

    /// Milliseconds remaining on a spell cooldown (0 if ready).
    pub fn cooldown_remaining_ms(&self, spell_id: SpellId, now_ms: u64) -> u32 {
        self.cooldowns
            .get(&spell_id)
            .map_or(0, |&ready_at| ready_at.saturating_sub(now_ms) as u32)
    }

    /// Returns true if GCD is active.
    pub fn gcd_active(&self, now_ms: u64) -> bool {
        now_ms < self.gcd_ready_at
    }

    /// Milliseconds remaining on the GCD (0 if ready).
    pub fn gcd_remaining_ms(&self, now_ms: u64) -> u64 {
        self.gcd_ready_at.saturating_sub(now_ms)
    }

    /// Called by action nodes when a spell is successfully cast.
    /// Records the GCD and the spell's own cooldown.
    pub fn on_spell_cast(&mut self, _spell_id: SpellId, now_ms: u64) {
        self.gcd_ready_at = now_ms + INSTANT_GCD_MS;
    }

    /// Called when we know the exact cooldown duration (from C++ `spell_cooldown_ms`).
    pub fn set_cooldown(&mut self, spell_id: SpellId, duration_ms: u32, now_ms: u64) {
        if duration_ms > 0 {
            self.cooldowns.insert(spell_id, now_ms + duration_ms as u64);
        }
    }

    /// Advance timers by `elapsed_ms`. Cleans up expired entries.
    pub fn advance(&mut self, now_ms: u64) {
        self.cooldowns.retain(|_, ready_at| *ready_at > now_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spell_not_on_cooldown_initially() {
        let t = BotTimers::new();
        assert!(!t.spell_on_cooldown(SpellId(1), 1000));
    }

    #[test]
    fn spell_on_cooldown_after_set() {
        let mut t = BotTimers::new();
        t.set_cooldown(SpellId(1), 5000, 1000); // ready at 6000
        assert!(t.spell_on_cooldown(SpellId(1), 3000));
        assert!(!t.spell_on_cooldown(SpellId(1), 6001));
    }

    #[test]
    fn gcd_active_after_cast() {
        let mut t = BotTimers::new();
        t.on_spell_cast(SpellId(1), 1000); // gcd_ready_at = 2500
        assert!(t.gcd_active(2000));
        assert!(!t.gcd_active(2501));
    }

    #[test]
    fn advance_clears_expired_cooldowns() {
        let mut t = BotTimers::new();
        t.set_cooldown(SpellId(1), 100, 1000); // expires at 1100
        t.advance(1200);
        assert!(!t.spell_on_cooldown(SpellId(1), 1200));
    }
}
