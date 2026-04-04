#pragma once
#include "DungeonTriggers.h"

namespace ai
{
    class OnyxiasLairEnterDungeonTrigger : public EnterDungeonTrigger
    {
    public:
        OnyxiasLairEnterDungeonTrigger(PlayerbotAI* ai) : EnterDungeonTrigger(ai, "enter onyxia's lair", "onyxia's lair", 249) {}
    };

    class OnyxiasLairLeaveDungeonTrigger : public LeaveDungeonTrigger
    {
    public:
        OnyxiasLairLeaveDungeonTrigger(PlayerbotAI* ai) : LeaveDungeonTrigger(ai, "leave onyxia's lair", "onyxia's lair", 249) {}
    };

    class OnyxiaStartFightTrigger : public StartBossFightTrigger
    {
    public:
        OnyxiaStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start onyxia fight", "onyxia", 10184) {}
    };

    class OnyxiaEndFightTrigger : public EndBossFightTrigger
    {
    public:
        OnyxiaEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end onyxia fight", "onyxia", 10184) {}
    };

    // Ranged/healers should stay at least 30 yards from Onyxia
    class OnyxiaTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        OnyxiaTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "onyxia too close", 10184, 30.0f) {}
    };

    // Onyxian Whelps (entry 10442) swarm during Phase 2 — avoid packs within 8 yards
    class OnyxianWhelpHazardTrigger : public CloseToCreatureHazardTrigger
    {
    public:
        OnyxianWhelpHazardTrigger(PlayerbotAI* ai) : CloseToCreatureHazardTrigger(ai, "onyxian whelp hazard", 10442, 8.0f, 30) {}
    };
}