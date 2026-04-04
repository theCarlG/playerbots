/// Looting — approach and loot nearby corpses.
use crate::engine::bt::Bt::{self, *};
use crate::engine::bt::Setting;

pub fn loot_subtree() -> Bt {
    Seq(vec![
        InCombat.not(),
        SettingEnabled(Setting::AutoLoot),
        Bt::throttle(2_000, LootNearest),
    ])
}
