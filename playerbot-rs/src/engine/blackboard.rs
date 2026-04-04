/// Per-bot typed key-value store.
///
/// All state that the BT writes and reads across ticks lives here.
/// O(1) access via enum discriminant index — no string lookups, no HashMap.
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
pub struct Blackboard([Value; BOARD_SIZE]);

impl Default for Blackboard {
    fn default() -> Self {
        Self([Value::None; BOARD_SIZE])
    }
}

impl Blackboard {
    pub fn get(&self, key: Key) -> Value {
        self.0[key as usize]
    }

    pub fn set(&mut self, key: Key, value: Value) {
        self.0[key as usize] = value;
    }

    pub fn clear(&mut self, key: Key) {
        self.0[key as usize] = Value::None;
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
