use crate::Seq;
/// Vendor behavior — sell grey/white items and restock ammo at nearby vendors.
use crate::engine::bt::Bt::{
    self, HasSellableItems, InCombat, RestockAmmo, SettingEnabled, VendorSellGrey,
};
use crate::engine::bt::Setting;

pub fn vendor_subtree() -> Bt {
    Seq!(
        InCombat.not(),
        SettingEnabled(Setting::AutoVendor),
        HasSellableItems,
        Bt::throttle(60_000, VendorSellGrey),
    )
}

/// Restock ammo at a nearby vendor when low. Gated on the same `AutoVendor`
/// setting as selling — both are "interact with vendors" behaviors. The
/// `RestockAmmo` leaf is a no-op for non-ammo classes and when already
/// well-stocked, so this is cheap to tick.
pub fn ammo_restock_subtree() -> Bt {
    Seq!(
        InCombat.not(),
        SettingEnabled(Setting::AutoVendor),
        Bt::throttle(60_000, RestockAmmo),
    )
}
