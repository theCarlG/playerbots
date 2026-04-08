use crate::bot::settings::StrategyFlags;
use crate::engine::bt::Bt;
use crate::ffi::SpellId;
use crate::{Sel, Seq};

// Well-known racial ability spell IDs (Classic).
const STONEFORM: SpellId = SpellId(20594);
const ESCAPE_ARTIST: SpellId = SpellId(20589);
const PERCEPTION: SpellId = SpellId(20600);
const SHADOWMELD: SpellId = SpellId(20580);
const BERSERKING: SpellId = SpellId(20554);
const WAR_STOMP: SpellId = SpellId(20549);
const WILL_OF_FORSAKEN: SpellId = SpellId(7744);
const BLOOD_FURY: SpellId = SpellId(20572);

/// Racials strategy — try each racial in priority order. Each
/// `UseRacial` leaf gates on `knows_spell` internally, so only the
/// bot's actual race fires.
///
/// PB2: `RacialsStrategy` — gated on the `racials` strategy flag.
pub fn build() -> Bt {
    Seq!(
        Bt::StrategyEnabled(StrategyFlags::RACIALS),
        Bt::InCombat,
        Bt::throttle(
            30_000,
            Sel!(
                Bt::UseRacial(STONEFORM),
                Bt::UseRacial(BERSERKING),
                Bt::UseRacial(BLOOD_FURY),
                Bt::UseRacial(WAR_STOMP),
                Bt::UseRacial(WILL_OF_FORSAKEN),
                Bt::UseRacial(ESCAPE_ARTIST),
                Bt::UseRacial(PERCEPTION),
                Bt::UseRacial(SHADOWMELD),
            ),
        ),
    )
}
