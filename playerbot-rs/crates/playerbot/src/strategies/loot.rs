use crate::engine::bt::Bt;

/// Loot strategy — find and loot nearby corpses.
/// PB2: `LootStrategy` — runs the loot action on a throttle.
pub fn build() -> Bt {
    Bt::throttle(2_000, Bt::LootNearest)
}
