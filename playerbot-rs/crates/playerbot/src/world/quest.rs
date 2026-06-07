/// Quest behavior — accept, work on, and turn in quests.
use crate::engine::bt::Bt::{
    self, AcceptQuests, AttackQuestMob, EscortQuestNpc, InCombat, LootNearest, SettingEnabled,
    TalkToQuestNpc, TurnInQuest, UseQuestObject,
};
use crate::engine::bt::Setting;
use crate::{Sel, Seq};

pub fn quest_subtree() -> Bt {
    Seq!(
        InCombat.not(),
        Sel!(
            // Active escort — keep pace with the escort NPC (this is
            // time-sensitive, so it comes first; combat handles ambushes).
            EscortQuestNpc,
            // Turn in completed quests.
            Bt::throttle(5_000, TurnInQuest),
            // Drop grey (no-XP) quests so the bot stops carrying / working
            // trivial low-level quests after it out-levels a zone.
            Bt::throttle(15_000, Bt::AbandonGreyQuests),
            // Accept new quests from nearby quest givers.
            Seq!(
                SettingEnabled(Setting::AutoAcceptQuest),
                Bt::throttle(10_000, AcceptQuests),
            ),
            // Work on quest objectives — use any nearby "use object" target,
            // talk to a "speak to" objective NPC, then kill quest mobs.
            // (UseQuestObject/TalkToQuestNpc are throttled; they scan the log.)
            Bt::throttle(3_000, UseQuestObject),
            Bt::throttle(3_000, TalkToQuestNpc),
            // Loot gameobject containers (boxes/chests) for "collect item from
            // container" objectives, e.g. Scavenging Deathknell's Equipment
            // Boxes. Self-fails when none are nearby.
            Bt::LootQuestContainer,
            // Buy still-needed quest items a nearby vendor sells (Coarse Thread,
            // etc.). Self-fails when no vendor in range stocks a needed item.
            Bt::throttle(3_000, Bt::BuyQuestItems),
            // Loot our kills BEFORE engaging the next mob, so the bot clears
            // every corpse it has rights to (a caster's ranged kills land well
            // away from it). LootNearest self-fails when nothing is lootable,
            // so it's transparent once corpses are cleared.
            LootNearest,
            AttackQuestMob,
        ),
    )
}
