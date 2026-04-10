use crate::engine::bt::Bt;
use crate::{Sel, Seq};

/// Consumables strategy — use buff/utility potions when available.
/// Gates on the potion cooldown and bag availability checks so the
/// BT short-circuits cheaply in the common "nothing to use" case.
///
/// PB2: `ConsumablesStrategy` / `PotionsStrategy`.
pub fn build() -> Bt {
    Bt::throttle(
        5_000,
        Sel!(
            Seq!(
                Bt::HasBuffPotionAvailable,
                Bt::PotionCooldownReady,
                Bt::UseBuffPotion,
            ),
            Seq!(Bt::PotionCooldownReady, Bt::UseUtilityPotion,),
        ),
    )
}
