use crate::{Sel, Seq};
use crate::bot::settings::StrategyFlags;
use crate::engine::bt::Bt;

/// RPG strategy — idle wandering, NPC interaction, emotes.
/// PB2: `RpgStrategy` — gated on the `rpg` strategy flag.
pub fn build() -> Bt {
    Seq!(
        Bt::StrategyEnabled(StrategyFlags::RPG),
        Sel!(
            Bt::throttle(10_000, Bt::RpgInteractNpc),
            Bt::throttle(30_000, Bt::RpgEmote),
            Bt::throttle(5_000, Bt::RpgWander),
        ),
    )
}
