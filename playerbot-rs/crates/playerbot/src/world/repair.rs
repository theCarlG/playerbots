use crate::Seq;
/// Repair behavior — find repair NPC and repair all equipment.
use crate::engine::bt::Bt::{self, InCombat, SettingEnabled, DurabilityBelow, RepairEquipment};
use crate::engine::bt::Setting;

pub fn repair_subtree() -> Bt {
    Seq!(
        InCombat.not(),
        SettingEnabled(Setting::AutoRepair),
        DurabilityBelow(0.30),
        Bt::throttle(60_000, RepairEquipment),
    )
}
