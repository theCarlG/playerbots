use crate::Seq;
/// Vendor behavior — sell grey/white items at nearby vendor NPCs.
use crate::engine::bt::Bt::{self, *};
use crate::engine::bt::Setting;

pub fn vendor_subtree() -> Bt {
    Seq!(
        InCombat.not(),
        SettingEnabled(Setting::AutoVendor),
        HasSellableItems,
        Bt::throttle(60_000, VendorSellGrey),
    )
}
