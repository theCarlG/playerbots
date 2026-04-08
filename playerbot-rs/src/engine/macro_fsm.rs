/// Top-level macro FSM states.
///
/// Three mutually exclusive states that determine which behavior tree runs.
/// The FSM is evaluated at the top of each tick before the BT. Encounters
/// and classes receive the active FSM as a parameter to branch on.

/// The three top-level bot states. Mutually exclusive — exactly one is
/// active at any time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ActiveFsm {
    /// Out-of-combat: following, grinding, questing, resting, RPG, etc.
    World = 0,
    /// In combat: rotations, reactive abilities, encounter mechanics.
    Combat = 1,
    /// Dead: corpse run, accept resurrect, spirit healer.
    Dead = 2,
}

impl ActiveFsm {
    /// Determine which FSM should be active based on current game state.
    ///
    /// Priority order: Dead > Combat > World.
    #[inline]
    pub fn determine(alive: bool, in_combat: bool, has_attackers: bool) -> Self {
        if !alive {
            Self::Dead
        } else if in_combat || has_attackers {
            Self::Combat
        } else {
            Self::World
        }
    }
}

/// World FSM sub-states.
///
/// When `ActiveFsm::World` is active, this enum tracks what the bot is
/// doing out of combat. Derived from `BehaviorMode` (the command-driven
/// setting) plus runtime conditions (e.g. resting when low HP/mana).
///
/// The World BT's `mode_dispatch()` already branches on `BehaviorMode`.
/// `WorldSub` formalizes this as an observable state with proper transitions
/// so encounters, the monitor, and coordination can reason about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum WorldSub {
    /// Following master / tank / group member.
    #[default]
    Follow = 0,
    /// Autonomous mob grinding.
    Grind = 1,
    /// Working on quests.
    Quest = 2,
    /// Eating/drinking to recover HP/mana.
    Rest = 3,
    /// Holding position, fighting hostiles in range.
    Guard = 4,
    /// Town wandering, NPC interaction.
    Rpg = 5,
    /// Staying put, do nothing unless commanded.
    Stay = 6,
    /// Battleground autonomous behavior.
    Bg = 7,
    /// Traveling to a distant destination (vendor, repair, world buff, ...).
    Travel = 8,
}

impl WorldSub {
    /// Derive `WorldSub` from the current `BehaviorMode` and runtime state.
    ///
    /// `is_resting` overrides the mode — if the bot is eating/drinking, it's
    /// in Rest regardless of what mode it's in (except Passive/Stay).
    pub fn derive(mode: crate::bot::settings::BehaviorMode, is_resting: bool) -> Self {
        use crate::bot::settings::BehaviorMode;
        if is_resting {
            return Self::Rest;
        }
        match mode {
            BehaviorMode::Follow => Self::Follow,
            BehaviorMode::Stay => Self::Stay,
            BehaviorMode::Grind => Self::Grind,
            BehaviorMode::Quest => Self::Quest,
            BehaviorMode::Passive => Self::Stay,
            BehaviorMode::Guard => Self::Guard,
            BehaviorMode::Rpg => Self::Rpg,
            BehaviorMode::Bg => Self::Bg,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dead_takes_priority() {
        assert_eq!(ActiveFsm::determine(false, true, true), ActiveFsm::Dead);
    }

    #[test]
    fn combat_when_in_combat() {
        assert_eq!(ActiveFsm::determine(true, true, false), ActiveFsm::Combat);
    }

    #[test]
    fn combat_when_has_attackers() {
        assert_eq!(ActiveFsm::determine(true, false, true), ActiveFsm::Combat);
    }

    #[test]
    fn world_when_peaceful() {
        assert_eq!(ActiveFsm::determine(true, false, false), ActiveFsm::World);
    }

    #[test]
    fn world_sub_from_mode() {
        use crate::bot::settings::BehaviorMode;
        assert_eq!(
            WorldSub::derive(BehaviorMode::Follow, false),
            WorldSub::Follow
        );
        assert_eq!(
            WorldSub::derive(BehaviorMode::Grind, false),
            WorldSub::Grind
        );
        assert_eq!(WorldSub::derive(BehaviorMode::Stay, false), WorldSub::Stay);
        assert_eq!(WorldSub::derive(BehaviorMode::Bg, false), WorldSub::Bg);
    }

    #[test]
    fn resting_overrides_mode() {
        use crate::bot::settings::BehaviorMode;
        assert_eq!(WorldSub::derive(BehaviorMode::Follow, true), WorldSub::Rest);
        assert_eq!(WorldSub::derive(BehaviorMode::Grind, true), WorldSub::Rest);
    }
}
