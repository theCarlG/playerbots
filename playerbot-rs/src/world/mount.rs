/// Mount/dismount behavior.
///
/// Mount when the master mounts (out of combat, outdoors).
/// Dismount when entering combat, going indoors, or master dismounts.
use crate::engine::bt::Bt::{self, IsMounted, InCombat, IsIndoor, Dismount, SettingEnabled, MountUp, MasterIsMounted};
use crate::{Sel, Seq};

pub fn mount_subtree() -> Bt {
    Sel!(
        // Dismount if mounted and shouldn't be (combat, indoors, or master dismounted).
        Seq!(IsMounted, Sel!(InCombat, IsIndoor, MasterIsMounted.not()), Dismount),
        // Mount when master is mounted, not in combat, outdoors, and setting enabled.
        Seq!(
            IsMounted.not(),
            InCombat.not(),
            IsIndoor.not(),
            MasterIsMounted,
            SettingEnabled(crate::engine::bt::Setting::AutoMount),
            MountUp,
        ),
    )
}
