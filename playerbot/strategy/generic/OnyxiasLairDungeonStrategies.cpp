
#include "playerbot/playerbot.h"
#include "OnyxiasLairDungeonStrategies.h"
#include "DungeonMultipliers.h"

using namespace ai;

void OnyxiasLairDungeonStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "start onyxia fight",
        NextAction::array(0, new NextAction("enable onyxia fight strategy", 100.0f), NULL)));
}

void OnyxiaFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
    {
        // Ranged and healers maintain safe distance from Onyxia
        triggers.push_back(new TriggerNode(
            "onyxia too close",
            NextAction::array(0, new NextAction("move away from onyxia", 100.0f), NULL)));
    }
    else
    {
        // Melee DPS attack from behind to avoid frontal Cleave
        triggers.push_back(new TriggerNode(
            "not behind target",
            NextAction::array(0, new NextAction("set behind", ACTION_NORMAL + 2), NULL)));
    }
}

void OnyxiaFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end onyxia fight",
        NextAction::array(0, new NextAction("disable onyxia fight strategy", 100.0f), NULL)));
}

void OnyxiaFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end onyxia fight",
        NextAction::array(0, new NextAction("disable onyxia fight strategy", 100.0f), NULL)));
}

void OnyxiaFightStrategy::InitReactionTriggers(std::list<TriggerNode*>& triggers)
{
    // Phase 2: Onyxian Whelps swarm the cave — step away from nearby packs
    triggers.push_back(new TriggerNode(
        "onyxian whelp hazard",
        NextAction::array(0, new NextAction("move away from hazard", 100.0f), NULL)));
}

void OnyxiaFightStrategy::InitCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
    {
        // Prevent movement from interrupting casts while at safe range
        multipliers.push_back(new PreventMoveAwayFromCreatureOnReachToCastMultiplier(ai));
    }
}
