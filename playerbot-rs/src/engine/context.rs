/// TickContext — everything a BT node needs for one tick.
///
/// Built once at the start of each `playerbot_update` call and passed
/// down through the entire tree. Immutable game state is borrowed;
/// mutable bot state (timers, blackboard, target) is mutably borrowed.
use crate::{
    engine::{
        blackboard::Blackboard,
        group_state::GroupState,
        snapshot::{UnitSnapshotExt, WorldSnapshotExt},
        timers::BotTimers,
    },
    ffi::{interface::BotInterface, BotWorldSnapshot, UnitHandle},
};

pub struct TickContext<'a> {
    // ── Immutable game state (read-only, refreshed at tick start) ──────
    pub snap:        &'a BotWorldSnapshot,
    pub nearby:      &'a [UnitHandle],   // hostile nearby units (refreshed every 500ms)
    pub attackers:   &'a [UnitHandle],   // units attacking this bot (refreshed every 500ms)
    pub group_state: Option<&'a GroupState>,

    // ── Mutable bot state ───────────────────────────────────────────────
    pub interface:   &'a dyn BotInterface,
    pub blackboard:  &'a mut Blackboard,
    pub timers:      &'a mut BotTimers,

    // ── Tick metadata ───────────────────────────────────────────────────
    pub server_time_ms: u64,
    pub elapsed_ms:     u32,
    pub minimal:        bool,   // true = throttled tick, skip expensive queries

    // ── Bot identity ────────────────────────────────────────────────────
    /// This bot's own UnitHandle (ObjectGuid value). Use as a target for
    /// self-buffs, self-casts, or any operation that needs "cast on myself".
    pub bot_handle:  UnitHandle,
}

impl<'a> TickContext<'a> {
    /// Convenience: self HP as a fraction [0.0, 1.0].
    pub fn self_hp_pct(&self) -> f32 {
        self.snap.self_hp_pct()
    }

    /// Convenience: self mana as a fraction [0.0, 1.0].
    pub fn self_mana_pct(&self) -> f32 {
        self.snap.self_mana_pct()
    }

    /// Convenience: self current target handle (None if no target).
    pub fn current_target(&self) -> Option<UnitHandle> {
        let h = self.snap.self_unit().current_target;
        if h == 0 { None } else { Some(h) }
    }

    /// True if this bot is currently in combat.
    pub fn in_combat(&self) -> bool {
        self.snap.self_unit().in_combat
    }

    /// True if this bot is inside a raid/dungeon instance.
    pub fn in_instance(&self) -> bool {
        self.snap.in_instance()
    }
}

// ── Test helpers ──────────────────────────────────────────────────────────
#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::ffi::{BotUnitSnapshot, BotWorldSnapshot, SpellId, ItemId};
    use crate::engine::{blackboard::Blackboard, timers::BotTimers};

    /// Minimal mock interface for unit tests of BT node logic.
    pub struct NullInterface;
    impl BotInterface for NullInterface {
        fn get_snapshot(&self) -> BotWorldSnapshot { BotWorldSnapshot::default() }
        fn get_unit_snapshot(&self, _: UnitHandle) -> BotUnitSnapshot { BotUnitSnapshot::default() }
        fn has_aura(&self, _: UnitHandle, _: SpellId) -> bool { false }
        fn get_aura(&self, _: UnitHandle, _: SpellId) -> Option<crate::ffi::BotAuraInfo> { None }
        fn get_auras(&self, _: UnitHandle) -> Vec<crate::ffi::BotAuraInfo> { vec![] }
        fn get_threat_list(&self, _: UnitHandle) -> Vec<crate::ffi::BotThreatEntry> { vec![] }
        fn get_unit_threat(&self, _: UnitHandle, _: UnitHandle) -> f32 { 0.0 }
        fn unit_distance(&self, _: UnitHandle) -> f32 { 0.0 }
        fn can_cast(&self, _: SpellId, _: UnitHandle) -> bool { true }
        fn spell_cooldown_ms(&self, _: SpellId) -> u32 { 0 }
        fn has_los(&self, _: UnitHandle) -> bool { true }
        fn get_nearby_units(&self, _: f32, _: bool) -> Vec<UnitHandle> { vec![] }
        fn get_behind_position(&self, _: UnitHandle, _: f32) -> crate::ffi::BotPosition { Default::default() }
        fn get_safe_position(&self, _: f32) -> Option<crate::ffi::BotPosition> { None }
        fn get_spread_position(&self, _: UnitHandle, _: f32, _: u8, _: u8) -> crate::ffi::BotPosition { Default::default() }
        fn can_reach(&self, _: f32, _: f32, _: f32) -> bool { true }
        fn cast_spell(&self, _: SpellId, _: UnitHandle) -> bool { true }
        fn cast_spell_pos(&self, _: SpellId, _: f32, _: f32, _: f32) -> bool { true }
        fn move_to(&self, _: f32, _: f32, _: f32) -> bool { true }
        fn follow(&self, _: UnitHandle, _: f32, _: f32) -> bool { true }
        fn stop_moving(&self) -> bool { true }
        fn attack(&self, _: UnitHandle) -> bool { true }
        fn auto_attack(&self, _: bool) -> bool { true }
        fn say(&self, _: &str, _: u32) -> bool { true }
        fn use_item(&self, _: ItemId, _: UnitHandle) -> bool { true }
        fn taunt(&self, _: UnitHandle) -> bool { true }
        fn group_get_tank(&self) -> Option<UnitHandle> { None }
        fn group_get_healer(&self) -> Option<UnitHandle> { None }
        fn group_get_role(&self, _: UnitHandle) -> crate::ffi::BotRole { Default::default() }
    }

    /// Build a minimal TickContext for use in unit tests.
    pub fn make_test_ctx_with<'a>(
        snap: &'a BotWorldSnapshot,
        nearby: &'a [UnitHandle],
        attackers: &'a [UnitHandle],
        interface: &'a dyn BotInterface,
        blackboard: &'a mut Blackboard,
        timers: &'a mut BotTimers,
    ) -> TickContext<'a> {
        TickContext {
            snap,
            nearby,
            attackers,
            group_state: None,
            interface,
            blackboard,
            timers,
            server_time_ms: 10_000,
            elapsed_ms: 100,
            minimal: false,
            bot_handle: 0,
        }
    }

    // Owned versions of test state for convenience in tests that don't need
    // to inspect them afterward.
    thread_local! {
        static NULL_IFACE: NullInterface = NullInterface;
    }

    pub struct TestCtxOwned {
        pub snap:       BotWorldSnapshot,
        pub nearby:     Vec<UnitHandle>,
        pub attackers:  Vec<UnitHandle>,
        pub interface:  NullInterface,
        pub blackboard: Blackboard,
        pub timers:     BotTimers,
        pub time_ms:    u64,
    }

    impl TestCtxOwned {
        pub fn new() -> Self {
            Self {
                snap: BotWorldSnapshot::default(),
                nearby: vec![],
                attackers: vec![],
                interface: NullInterface,
                blackboard: Blackboard::default(),
                timers: BotTimers::new(),
                time_ms: 10_000,
            }
        }

        pub fn ctx(&mut self) -> TickContext<'_> {
            TickContext {
                snap:           &self.snap,
                nearby:         &self.nearby,
                attackers:      &self.attackers,
                group_state:    None,
                interface:      &self.interface,
                blackboard:     &mut self.blackboard,
                timers:         &mut self.timers,
                server_time_ms: self.time_ms,
                elapsed_ms:     100,
                minimal:        false,
                bot_handle:     0,  // test default: no real handle
            }
        }
    }

    /// Convenience for tests that just need any valid context.
    pub fn make_test_ctx() -> TestCtxOwned {
        TestCtxOwned::new()
    }

    // Re-export so bt_nodes tests can use make_test_ctx() directly
    impl Default for TestCtxOwned {
        fn default() -> Self { Self::new() }
    }
}

// Allow bt_nodes tests to call make_test_ctx() and get a TickContext
#[cfg(test)]
impl crate::engine::context::tests::TestCtxOwned {
    pub fn as_ctx(&mut self) -> TickContext<'_> {
        self.ctx()
    }
}
