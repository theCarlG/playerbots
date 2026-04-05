/// Reactive combat behaviors — interrupt, dispel, resurrect, flee, threat.
///
/// These wrap around class-specific rotations in the root BT.
/// They fire based on conditions and have higher priority than
/// normal rotation abilities.
///
/// The class rotation itself is a `Box<dyn BtNode>` (closure-based),
/// so the combat wrapper is built in `bot::init` as a `Box<dyn BtNode>`
/// selector containing both `Bt` enum nodes and the class rotation.
use crate::bot::state::PlayerClass;
use crate::engine::bt::{Bt::{self, *}, Op::*, Resource::*};
use crate::{Seq, Sel};

/// Flee at critically low HP.
pub fn flee_subtree() -> Bt {
    Seq!(
        Cmp(SelfHealthPct, Below(0)), // flee_hp_pct checked via the dynamic threshold
        FleeToSafe(20.0),
    )
}

/// Interrupt enemy casts (class-appropriate).
pub fn interrupt_subtree() -> Bt {
    Bt::throttle(500, Seq!(TargetCastingInterruptible, Interrupt))
}

/// Dispel party debuffs (healer/dispel classes only).
pub fn dispel_subtree() -> Bt {
    Seq!(
        Sel!(
            IsClass(PlayerClass::Priest),
            IsClass(PlayerClass::Paladin),
            IsClass(PlayerClass::Druid),
            IsClass(PlayerClass::Mage),
            IsClass(PlayerClass::Shaman),
        ),
        Bt::throttle(1_000, DispelParty),
    )
}

/// Resurrect dead party members (class-appropriate).
pub fn resurrect_subtree() -> Bt {
    Seq!(
        Sel!(
            IsClass(PlayerClass::Priest),
            IsClass(PlayerClass::Paladin),
            IsClass(PlayerClass::Druid),
            IsClass(PlayerClass::Shaman),
        ),
        Bt::throttle(5_000, ResurrectParty),
    )
}

/// Threat dump when DPS is about to pull aggro.
pub fn threat_subtree() -> Bt {
    Seq!(IsTank.not(), InCombat, PullingAggro, ThreatDump)
}

/// Target selection based on combat order and settings.
pub fn targeting_subtree() -> Bt {
    Sel!(
        // Focus target override.
        Seq!(HasFocusTarget, FocusAttack),
        // Tank: pick up loose adds.
        Seq!(
            CombatOrderHas(crate::bot::settings::CombatOrder::TANK),
            Bt::throttle(1_000, TankPickupAdds),
        ),
        // Assist: attack leader/tank's target.
        Seq!(
            CombatOrderHas(crate::bot::settings::CombatOrder::ASSIST),
            AssistLeader,
        ),
        // Protect: attack what attacks protect target.
        Seq!(
            CombatOrderHas(crate::bot::settings::CombatOrder::PROTECT),
            ProtectAttacker,
        ),
        // Aggressive: attack nearest hostile.
        Seq!(
            ReactivityIs(crate::bot::settings::Reactivity::Aggressive),
            AttackNearest,
        ),
    )
}
