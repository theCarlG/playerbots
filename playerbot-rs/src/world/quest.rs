/// Quest behavior — accept, work on, and turn in quests.
use crate::engine::bt::Bt::{self, Seq, InCombat, Sel, TurnInQuest, SettingEnabled, AcceptQuests, AttackQuestMob};
use crate::engine::bt::Setting;

pub fn quest_subtree() -> Bt {
    Seq(vec![
        InCombat.not(),
        Sel(vec![
            // Turn in completed quests.
            Bt::throttle(5_000, TurnInQuest),
            // Accept new quests from nearby quest givers.
            Seq(vec![
                SettingEnabled(Setting::AutoAcceptQuest),
                Bt::throttle(10_000, AcceptQuests),
            ]),
            // Work on quest objectives — kill quest mobs.
            AttackQuestMob,
        ]),
    ])
}
