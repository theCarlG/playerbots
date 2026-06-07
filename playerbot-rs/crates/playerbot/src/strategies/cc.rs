use crate::bot::settings::StrategyFlags;
use crate::engine::bt::{Bt, Op::AtLeast, Resource::AttackerCount};
use cmangos::SpellId;
use crate::{Sel, Seq};

/// CC strategy — crowd-control nearby enemies using the bot's CC spell.
///
/// Two paths, in priority order:
/// 1. Cast CC on the RTI-marked CC target (if a mob wears the CC icon).
/// 2. Cast CC on the nearest non-current-target mob.
///
/// Callers pass the class-appropriate CC spell (e.g. Polymorph 118,
/// Shackle 10955, Banish 710). Class files wrap this with their own
/// spell pick.
///
/// PB2: `CcStrategy` — gated on the `cc` strategy flag.
pub fn build(cc_spell: SpellId) -> Bt {
    Seq!(
        Bt::StrategyEnabled(StrategyFlags::CC),
        // Only CC when genuinely fighting MORE than one enemy. CC is for EXTRA
        // adds — never poly a lone mob, and never re-poly the LAST remaining mob.
        // Without this gate, after the first of two mobs dies the survivor became
        // both the kill target and (briefly, while focus_target was mid-switch) a
        // CC candidate, so the bot looped frostbolt → poly → frostbolt → poly and
        // burned itself to OOM (the reported 2-mob bug). With ≥2 attackers the CC
        // leaf targets the add (kill target + focus are excluded there); drop to
        // one attacker and CC stops, so the bot just kills what's left.
        Bt::Cmp(AttackerCount, AtLeast(2)),
        Bt::throttle(
            2_000,
            Sel!(Bt::CcCastOnRti(cc_spell), Bt::CcCastOnNearest(cc_spell),),
        ),
    )
}
