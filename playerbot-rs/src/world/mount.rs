/// Mount/dismount behavior.
///
/// Mount when traveling long distance out of combat.
/// Dismount when entering combat or going indoors.
use crate::engine::bt::Bt::{self, *};

pub fn mount_subtree() -> Bt {
    Sel(vec![
        // Dismount if mounted and shouldn't be.
        Seq(vec![IsMounted, Sel(vec![InCombat, IsIndoor]), Dismount]),
        // Mount if not mounted, not in combat, outdoors, and setting enabled.
        Seq(vec![
            IsMounted.not(),
            InCombat.not(),
            IsIndoor.not(),
            SettingEnabled(crate::engine::bt::Setting::AutoMount),
            MountUp,
        ]),
    ])
}
