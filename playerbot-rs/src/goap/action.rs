/// GOAP action — a static data record describing what a bot can do.
///
/// Actions have preconditions (world state bits that must be set/clear),
/// effects (bits that get set/clear), a cost, and a mapping to BT
/// strategy flags. The A* planner chains actions to satisfy goals.
///
/// All actions are `const` — no closures, no heap. The global registry
/// is a compile-time array shared by all 1,800 bots.
use crate::bdi::desires::DesireKind;
use crate::bot::settings::StrategyFlags;

/// Newtype for action IDs — indexes into the global action registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ActionId(pub u16);

// ── Role / class filtering constants ─────────────────────────────────
pub const ROLE_ANY: u8 = 0xFF;
pub const ROLE_TANK: u8 = 1;
pub const ROLE_HEAL: u8 = 2;
pub const ROLE_DPS: u8 = 4;
pub const ROLE_TANK_DPS: u8 = ROLE_TANK | ROLE_DPS;
pub const ROLE_HEAL_DPS: u8 = ROLE_HEAL | ROLE_DPS;

pub const CLASS_ANY: u16 = 0xFFFF;

/// Helper: `class_bit(PlayerClass::Mage as u8)` → `1 << 8`.
pub const fn class_bit(c: u8) -> u16 {
    1u16 << c
}

/// A GOAP action definition.
#[derive(Debug, Clone, Copy)]
pub struct GoapAction {
    /// Unique ID (index into `ACTION_REGISTRY`).
    pub id: ActionId,
    /// Human-readable name for debugging/monitor.
    pub name: &'static str,
    /// Bits that must be SET in world state for this action to be usable.
    pub precondition_set: u64,
    /// Bits that must be CLEAR in world state for this action to be usable.
    pub precondition_clear: u64,
    /// Bits that this action SETS in world state when executed.
    pub effect_set: u64,
    /// Bits that this action CLEARS in world state when executed.
    pub effect_clear: u64,
    /// Base cost (lower = preferred by the planner).
    pub cost: u16,
    /// Strategy flags to enable in the BT when this is the current plan step.
    pub bt_flags: StrategyFlags,
    /// Bitmask of `DesireKind` variants this action can help satisfy.
    /// Used for quick filtering: skip actions that can't satisfy the goal.
    pub satisfies: u32,
    /// Role bitmask: bit 0=tank, 1=heal, 2=dps. `ROLE_ANY` (0xFF) = any role.
    pub role_mask: u8,
    /// Class bitmask: bit N = `PlayerClass` discriminant N. `CLASS_ANY` (0xFFFF) = any class.
    pub class_mask: u16,
}

impl GoapAction {
    /// Check if this action's preconditions are met in the given world state.
    pub fn is_applicable(&self, ws: super::world_state::WorldState) -> bool {
        ws.satisfies(self.precondition_set, self.precondition_clear)
    }

    /// Apply this action's effects to a world state, returning the new state.
    pub fn apply(&self, ws: super::world_state::WorldState) -> super::world_state::WorldState {
        ws.apply(self.effect_set, self.effect_clear)
    }

    /// Check if this action can help satisfy the given desire.
    pub fn can_satisfy(&self, desire: DesireKind) -> bool {
        self.satisfies & desire.as_bit() != 0
    }
}

/// Bitmask tracking which actions in the registry are available to a
/// specific bot. Built once at bot init based on class + role.
///
/// 128 bits = room for 128 actions in the registry (plenty).
#[derive(Debug, Clone, Copy, Default)]
pub struct ActionMask(pub u128);

impl ActionMask {
    pub fn set(&mut self, idx: usize) {
        debug_assert!(idx < 128);
        self.0 |= 1u128 << idx;
    }

    pub fn has(self, idx: usize) -> bool {
        debug_assert!(idx < 128);
        self.0 & (1u128 << idx) != 0
    }
}
