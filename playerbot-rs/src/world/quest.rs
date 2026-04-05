/// Quest behavior — accept, work on, and turn in quests.
use crate::engine::bt::Bt::{self, *};
use crate::{Seq, Sel};
use crate::engine::bt::Setting;

pub fn quest_subtree() -> Bt {
    Seq!(
        InCombat.not(),
        Sel!(
            // Turn in completed quests.
            Bt::throttle(5_000, TurnInQuest),
            // Accept new quests from nearby quest givers.
            Seq!(
                SettingEnabled(Setting::AutoAcceptQuest),
                Bt::throttle(10_000, AcceptQuests),
            ),
            // Work on quest objectives — kill quest mobs.
            AttackQuestMob,
        ),
    )
}
