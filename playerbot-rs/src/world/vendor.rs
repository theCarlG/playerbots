/// Vendor behavior — sell grey/white items at nearby vendor NPCs.
use crate::engine::bt::Bt::{self, *};
use crate::Seq;
use crate::engine::bt::Setting;

pub fn vendor_subtree() -> Bt {
    Seq!(
        InCombat.not(),
        SettingEnabled(Setting::AutoVendor),
        HasSellableItems,
        Bt::throttle(60_000, VendorSellGrey),
    )
}
