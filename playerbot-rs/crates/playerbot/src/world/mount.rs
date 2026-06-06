/// Mount/dismount behavior.
///
/// A bot with a human master mirrors the master's mount state. A masterless
/// bot (random/solo) mounts on its own initiative when travelling — outdoors
/// and out of combat — like a real player, rather than only ever running.
use crate::engine::bt::Bt::{
    self, Dismount, HasMaster, InCombat, IsIndoor, IsMounted, MasterIsMounted, MountUp,
    SettingEnabled,
};
use crate::{Sel, Seq};

pub fn mount_subtree() -> Bt {
    Sel!(
        // Dismount when mounted and we shouldn't be: in combat or indoors
        // (anyone), or — only for a mastered bot — the master has dismounted.
        // The master-dismount clause is gated on HasMaster so a masterless bot
        // isn't instantly dismounted by `MasterIsMounted.not()` (always true
        // when there's no master).
        Seq!(
            IsMounted,
            Sel!(InCombat, IsIndoor, Seq!(HasMaster, MasterIsMounted.not())),
            Dismount,
        ),
        // Mastered bot: mount when the master mounts.
        Seq!(
            IsMounted.not(),
            InCombat.not(),
            IsIndoor.not(),
            MasterIsMounted,
            SettingEnabled(crate::engine::bt::Setting::AutoMount),
            MountUp,
        ),
        // Masterless bot: mount up to get around, like a real player. Outdoors,
        // out of combat, AutoMount on. `MountUp` no-ops (Failure) if the bot
        // has no mount (e.g. too low level), so low-level bots just run.
        Seq!(
            IsMounted.not(),
            HasMaster.not(),
            InCombat.not(),
            IsIndoor.not(),
            SettingEnabled(crate::engine::bt::Setting::AutoMount),
            MountUp,
        ),
    )
}
