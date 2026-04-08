/// Mount/dismount behavior.
///
/// Mount when traveling long distance out of combat.
/// Dismount when entering combat or going indoors.
use crate::engine::bt::Bt::{self, IsMounted, InCombat, IsIndoor, Dismount, SettingEnabled, MountUp};
use crate::{Sel, Seq};

pub fn mount_subtree() -> Bt {
    Sel!(
        // Dismount if mounted and shouldn't be.
        Seq!(IsMounted, Sel!(InCombat, IsIndoor), Dismount),
        // Mount if not mounted, not in combat, outdoors, and setting enabled.
        Seq!(
            IsMounted.not(),
            InCombat.not(),
            IsIndoor.not(),
            SettingEnabled(crate::engine::bt::Setting::AutoMount),
            MountUp,
        ),
    )
}
