use crate::Seq;
/// Looting — approach and loot nearby corpses.
use crate::engine::bt::Bt::{self, SettingEnabled, LootNearest};
use crate::engine::bt::Setting;

pub fn loot_subtree() -> Bt {
    Seq!(
        Bt::InCombatFsm.not(),
        SettingEnabled(Setting::AutoLoot),
        Bt::throttle(2_000, LootNearest),
    )
}
