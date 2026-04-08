use crate::bot::settings::StrategyFlags;
use crate::engine::bt::Bt;
use crate::ffi::SpellId;
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
        Bt::throttle(
            2_000,
            Sel!(Bt::CcCastOnRti(cc_spell), Bt::CcCastOnNearest(cc_spell),),
        ),
    )
}
