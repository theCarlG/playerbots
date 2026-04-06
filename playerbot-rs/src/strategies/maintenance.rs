use crate::{Sel, Seq};
use crate::engine::bt::Bt;

/// Maintenance strategy — vendor sell, repair, and loot on throttle.
/// PB2: `MaintenanceStrategy` — low-priority upkeep.
pub fn build() -> Bt {
    Sel!(
        Bt::throttle(10_000, Bt::VendorSellGrey),
        Bt::throttle(10_000, Bt::RepairEquipment),
        Bt::throttle(2_000, Bt::LootNearest),
    )
}
