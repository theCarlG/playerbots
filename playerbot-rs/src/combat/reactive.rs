/// Reactive combat behaviors — interrupt, dispel, resurrect, flee, threat,
/// pull-back, positioning, and more.
///
/// These wrap around class-specific rotations in the root BT.
/// They fire based on conditions and have higher priority than
/// normal rotation abilities.
///
/// The class rotation itself is a `Box<dyn BtNode>` (closure-based),
/// so the combat wrapper is built in `bot::init` as a `Box<dyn BtNode>`
/// selector containing both `Bt` enum nodes and the class rotation.
use crate::bot::settings::StrategyFlags;
use crate::bot::state::PlayerClass;
use crate::engine::bt::Bt::{self, ShouldFlee, FleeToSafe, TargetCastingInterruptible, Interrupt, IsClass, DispelParty, ResurrectParty, IsTank, InCombat, PullingAggro, ThreatDump, StrategyEnabled, PullBack, PreHeal, HealInterrupt, KiteFromTarget, CloseToTarget, MaintainRange, MoveBehind, MarkRtiPreferred, HasFocusTarget, FocusAttack, TankPickupAdds, AttackRtiPriority, AssistLeader, ProtectAttacker, ReactivityIs, AttackNearest, HasAttackers};
use crate::{Sel, Seq};

/// Flee on an explicit `flee` command or when HP drops below the bot's
/// configured `flee_hp_pct` threshold (with `StrategyFlags::FLEE` enabled).
/// `ShouldFlee` reads live settings so the raid can re-tune thresholds at
/// runtime without rebuilding the tree.
pub fn flee_subtree() -> Bt {
    Seq!(ShouldFlee, FleeToSafe(20.0))
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

/// Pull phase — hold position and keep using ranged pull until the mob
/// arrives. Gated on `IsPulling` (set by the `pull` command, cleared
/// when any attacker reaches melee range).
///
/// When `PULL_BACK` is enabled, the bot first returns to its pre-pull
/// position (saved by the pull command). Otherwise, it holds its current
/// position and retries `PullTarget` (auto-shoot / taunt) each tick.
///
/// If the bot has no ranged pull ability (PullTarget fails), the Seq
/// fails and normal combat positioning takes over — the bot walks to
/// the target instead of standing still doing nothing.
pub fn pull_phase_subtree() -> Bt {
    Seq!(
        Bt::IsPulling,
        Sel!(
            // If PULL_BACK is enabled, move to pre-pull position first.
            // PullBack returns Running while moving, Failure when arrived.
            Seq!(StrategyEnabled(StrategyFlags::PULL_BACK), PullBack),
            // At position (or no PULL_BACK): retry ranged pull.
            // If PullTarget fails (no ranged weapon, taunt on CD),
            // the whole subtree fails and normal combat takes over.
            Bt::PullTarget,
        ),
    )
}

/// Wait-for-attack: bots hold off engaging until the pull target is
/// within melee range (mob has arrived). Gated on the `WAIT_FOR_ATTACK`
/// strategy flag. When active, this fires in the reactive layer and
/// short-circuits the combat pipeline, preventing bots from charging
/// into the pull. Can be set on any role including tanks (e.g. a tank
/// waiting for mobs to arrive at a chokepoint after pulling back).
pub fn wait_for_attack_subtree() -> Bt {
    Seq!(
        StrategyEnabled(StrategyFlags::WAIT_FOR_ATTACK),
        Bt::WaitForAttack,
    )
}

/// Pre-heal: healers cast a heal on an injured party member as combat
/// starts. Gated on the HEAL strategy flag. The generic leaf returns
/// Failure; class files provide the real implementation via higher-priority
/// HealLowest/CastOnLowestAlly leaves.
pub fn preheal_subtree() -> Bt {
    Bt::throttle(2_000, PreHeal)
}

/// Interrupt own cast when the heal target is no longer injured
/// (overheal prevention). Uses the `interrupt_own_cast` FFI callback.
pub fn heal_interrupt_subtree() -> Bt {
    HealInterrupt
}

/// Kite: ranged DPS move away when an attacker enters melee range.
/// Gated on RANGED strategy flag — only ranged classes should kite.
pub fn kite_subtree() -> Bt {
    Seq!(
        StrategyEnabled(StrategyFlags::RANGED),
        Bt::throttle(1_000, KiteFromTarget(8.0)),
    )
}

/// Close to melee range. Gated on the CLOSE strategy flag.
pub fn close_subtree() -> Bt {
    Seq!(StrategyEnabled(StrategyFlags::CLOSE), CloseToTarget(5.0),)
}

/// Maintain ranged distance. Gated on the RANGED strategy flag.
/// Two behaviors: flee when too close (< 8y), chase when too far (> 30y).
pub fn ranged_subtree() -> Bt {
    Seq!(
        StrategyEnabled(StrategyFlags::RANGED),
        Sel!(MaintainRange(8.0), CloseToTarget(30.0),),
    )
}

/// Stay behind the target. Gated on the BEHIND strategy flag AND
/// `InCombatFsm` so the MoveBehind chase command does NOT fire
/// out-of-combat. Without this gate the bot issues combat chase()
/// calls while the world tree tries to Follow, causing rogues to
/// "glide away" from the group as the two movement systems fight.
pub fn behind_subtree() -> Bt {
    Seq!(
        StrategyEnabled(StrategyFlags::BEHIND),
        Bt::InCombatFsm,
        Bt::throttle(1_000, MoveBehind(2.0)),
    )
}

/// Mark the current target with the bot's preferred raid target icon.
/// Gated on `MARK_RTI` — the Mangosbot addon's "Mark current target"
/// button toggles this flag via `strat ~mark rti`. When
/// `preferred_rti_icon` is `None`, the subtree does nothing (the user
/// can clear it with `rti clear` to stop marking entirely).
pub fn mark_rti_subtree() -> Bt {
    Seq!(
        StrategyEnabled(StrategyFlags::MARK_RTI),
        Bt::throttle(3_000, MarkRtiPreferred),
    )
}

/// Target selection based on combat order and settings.
///
/// Focus targets, RTI assignments, and tank duties always run (command
/// overrides). The generic assist/protect/aggressive arms are suppressed
/// when GOAP has an active plan — GOAP drives target acquisition through
/// its own actions. Attack-back fallback always runs as a safety net.
pub fn targeting_subtree() -> Bt {
    Sel!(
        // Focus target override — explicit "attack this" command. Always runs.
        Seq!(HasFocusTarget, FocusAttack),
        // Unified RTI priority for every class/role. This leaf handles
        // three phases in order: (a) the bot's assigned RTI icons from
        // `GroupCoordination::tank_focus_targets` walked in canonical
        // kill order — tank-flagged bots also run cooperation/taunt
        // logic so multiple tanks don't steal threat from each other;
        // (b) the bot's `preferred_rti_icon` as a fallback when it has
        // no explicit assignments; (c) the canonical kill order
        // (skull → cross → square → moon → triangle → diamond → circle
        // → star) to engage the first marked mob. Runs BEFORE the
        // generic assist/protect/aggressive arms so raid marks always
        // outrank a leader's arbitrary current selection.
        AttackRtiPriority,
        // Tank: pick up loose adds targeting non-tanks. Always runs.
        Seq!(
            StrategyEnabled(StrategyFlags::TANK),
            Bt::throttle(1_000, TankPickupAdds),
        ),
        // GOAP-gated: assist/protect/aggressive only when GOAP has no plan.
        // When GOAP is active, it drives target selection through acquire_target.
        Seq!(
            Bt::GoapHasPlan.not(),
            Sel!(
                Seq!(StrategyEnabled(StrategyFlags::ASSIST), AssistLeader),
                Seq!(StrategyEnabled(StrategyFlags::PROTECT), ProtectAttacker),
                Seq!(
                    ReactivityIs(crate::bot::settings::Reactivity::Aggressive),
                    AttackNearest,
                ),
            ),
        ),
        // Default attack-back fallback — always runs as safety net.
        Seq!(HasAttackers, AttackNearest),
    )
}
