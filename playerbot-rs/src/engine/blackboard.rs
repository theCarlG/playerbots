/// Per-bot typed key-value store.
///
/// All state that the BT writes and reads across ticks lives here.
/// O(1) access via enum discriminant index — no string lookups, no `HashMap`.
use crate::ffi::UnitHandle;

/// All keys a BT node can read or write.
/// Each variant is an index into a fixed-size array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Key {
    LastAttackTarget = 0,
    CombatStartMs,
    CurrentEncounterPhase, // u32 encoding of the phase enum for the active encounter
    AssignedRoleOverride,  // BotRole override from group coordinator
    InProgressActionId,    // id of a Running action (movement, casting)
    InProgressActionData,  // context data for the in-progress action
    FollowTargetHandle,
    LastSafePositionX,
    LastSafePositionY,
    LastSafePositionZ,
    LastFleeMs,        // server_time_ms of last flee movement
    ThreatResetNeeded, // bool: bot lost aggro, should stop attacking
    PullTimeMs,        // server_time_ms when the current boss was pulled
    AddCount,          // number of active adds (for encounter scripts)
    EncounterSafeZone, // u32: Heigan safe zone (1-4), 0 if not applicable

    // ── World behavior keys ─────────────────────────────────────────
    TravelDestX, // f32: travel destination
    TravelDestY,
    TravelDestZ,
    GrindTargetHandle, // handle: current grind target
    LastVendorVisitMs, // u64: when we last vendored
    LastRepairMs,      // u64: when we last repaired

    // ── RPG mode keys ───────────────────────────────────────────────
    RpgWanderDestX, // f32: current wander destination, held until arrival
    RpgWanderDestY,
    RpgWanderDestZ,

    // ── Formation (chaos) persistence ───────────────────────────────
    // Per-bot state for the `chaos` follow-formation jitter. Rerolled
    // every 3 seconds by `tick_follow`; other formations leave these
    // untouched. See `bot::formation::ChaosState`.
    ChaosDx,              // f32
    ChaosDy,              // f32
    ChaosLastChangeSecs,  // u64

    // ── Death behavior ───────────────────────────────────────────────
    DeathTimestampMs, // u64: server_time_ms when the bot died (0 = alive)

    // ── Follow throttle ──────────────────────────────────────────────
    LastFollowMs, // u64: last time position-based follow issued move_to

    // ── RTSC move queue ─────────────────────────────────────────────
    RtscMoveX, // f32: next RTSC move waypoint
    RtscMoveY,
    RtscMoveZ,

    // Add new keys above this line. Keep count accurate.
    _Count,
}

const BOARD_SIZE: usize = Key::_Count as usize;

/// Value stored in a blackboard slot.
#[derive(Debug, Clone, Copy, Default)]
pub enum Value {
    #[default]
    None,
    Handle(UnitHandle),
    U32(u32),
    U64(u64),
    F32(f32),
    Bool(bool),
}

/// The blackboard itself — a fixed-size array of Values.
#[derive(Debug)]
pub struct Blackboard {
    slots: [Value; BOARD_SIZE],
    /// Temporary monitor log lines accumulated during a tick.
    /// Flushed after the BT tick when monitoring is active.
    monitor_lines: Vec<String>,
}

impl Default for Blackboard {
    fn default() -> Self {
        Self {
            slots: [Value::None; BOARD_SIZE],
            monitor_lines: Vec::new(),
        }
    }
}

impl Blackboard {
    pub fn get(&self, key: Key) -> Value {
        self.slots[key as usize]
    }

    pub fn set(&mut self, key: Key, value: Value) {
        self.slots[key as usize] = value;
    }

    pub fn clear(&mut self, key: Key) {
        self.slots[key as usize] = Value::None;
    }

    /// Push a monitor log line (flushed after the BT tick).
    pub fn push_monitor_line(&mut self, line: String) {
        self.monitor_lines.push(line);
    }

    /// Drain all accumulated monitor lines.
    pub fn drain_monitor_lines(&mut self) -> std::vec::Drain<'_, String> {
        self.monitor_lines.drain(..)
    }

    pub fn get_u32(&self, key: Key) -> Option<u32> {
        match self.get(key) {
            Value::U32(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_u64(&self, key: Key) -> Option<u64> {
        match self.get(key) {
            Value::U64(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_f32(&self, key: Key) -> Option<f32> {
        match self.get(key) {
            Value::F32(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_bool(&self, key: Key) -> Option<bool> {
        match self.get(key) {
            Value::Bool(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_handle(&self, key: Key) -> Option<UnitHandle> {
        match self.get(key) {
            Value::Handle(v) if v != 0 => Some(v),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_u32() {
        let mut bb = Blackboard::default();
        bb.set(Key::AddCount, Value::U32(3));
        assert_eq!(bb.get_u32(Key::AddCount), Some(3));
    }

    #[test]
    fn roundtrip_handle() {
        let mut bb = Blackboard::default();
        let h: UnitHandle = 0xDEAD_BEEF_0000_0001;
        bb.set(Key::LastAttackTarget, Value::Handle(h));
        assert_eq!(bb.get_handle(Key::LastAttackTarget), Some(h));
    }

    #[test]
    fn zero_handle_returns_none() {
        let mut bb = Blackboard::default();
        bb.set(Key::LastAttackTarget, Value::Handle(0));
        assert_eq!(bb.get_handle(Key::LastAttackTarget), None);
    }

    #[test]
    fn clear_resets_to_none() {
        let mut bb = Blackboard::default();
        bb.set(Key::AddCount, Value::U32(5));
        bb.clear(Key::AddCount);
        assert!(matches!(bb.get(Key::AddCount), Value::None));
    }
}
