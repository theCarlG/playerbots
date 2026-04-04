
#include "playerbot/playerbot.h"
#include "AQ20DungeonStrategies.h"
#include "DungeonMultipliers.h"

using namespace ai;

void AQ20DungeonStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "start kurinnaxx fight",
        NextAction::array(0, new NextAction("enable kurinnaxx fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start rajaxx fight",
        NextAction::array(0, new NextAction("enable rajaxx fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start moam fight",
        NextAction::array(0, new NextAction("enable moam fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start buru fight",
        NextAction::array(0, new NextAction("enable buru fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start ayamiss fight",
        NextAction::array(0, new NextAction("enable ayamiss fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start ossirian fight",
        NextAction::array(0, new NextAction("enable ossirian fight strategy", 100.0f), NULL)));
}

// ----- Kurinnaxx -----
void KurinnaxxFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
    {
        // Mortal Wound and Sand Trap — ranged stay at distance
        triggers.push_back(new TriggerNode(
            "kurinnaxx too close",
            NextAction::array(0, new NextAction("move away from kurinnaxx", 100.0f), NULL)));
    }
}

void KurinnaxxFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end kurinnaxx fight",
        NextAction::array(0, new NextAction("disable kurinnaxx fight strategy", 100.0f), NULL)));
}

void KurinnaxxFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end kurinnaxx fight",
        NextAction::array(0, new NextAction("disable kurinnaxx fight strategy", 100.0f), NULL)));
}

void KurinnaxxFightStrategy::InitCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
        multipliers.push_back(new PreventMoveAwayFromCreatureOnReachToCastMultiplier(ai));
}

// ----- General Rajaxx -----
void RajaxxFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
    {
        // Thunderclap and Chain Lightning — ranged stay back
        triggers.push_back(new TriggerNode(
            "rajaxx too close",
            NextAction::array(0, new NextAction("move away from rajaxx", 100.0f), NULL)));
    }
}

void RajaxxFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end rajaxx fight",
        NextAction::array(0, new NextAction("disable rajaxx fight strategy", 100.0f), NULL)));
}

void RajaxxFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end rajaxx fight",
        NextAction::array(0, new NextAction("disable rajaxx fight strategy", 100.0f), NULL)));
}

void RajaxxFightStrategy::InitCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
        multipliers.push_back(new PreventMoveAwayFromCreatureOnReachToCastMultiplier(ai));
}

// ----- Moam -----
void MoamFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
}

void MoamFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end moam fight",
        NextAction::array(0, new NextAction("disable moam fight strategy", 100.0f), NULL)));
}

void MoamFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end moam fight",
        NextAction::array(0, new NextAction("disable moam fight strategy", 100.0f), NULL)));
}

// ----- Buru the Gorger -----
void BuruFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
}

void BuruFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end buru fight",
        NextAction::array(0, new NextAction("disable buru fight strategy", 100.0f), NULL)));
}

void BuruFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end buru fight",
        NextAction::array(0, new NextAction("disable buru fight strategy", 100.0f), NULL)));
}

// ----- Ayamiss the Hunter -----
void AyamissFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
    {
        // Paralyze targets melee range players — ranged stay back
        triggers.push_back(new TriggerNode(
            "ayamiss too close",
            NextAction::array(0, new NextAction("move away from ayamiss", 100.0f), NULL)));
    }
}

void AyamissFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end ayamiss fight",
        NextAction::array(0, new NextAction("disable ayamiss fight strategy", 100.0f), NULL)));
}

void AyamissFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end ayamiss fight",
        NextAction::array(0, new NextAction("disable ayamiss fight strategy", 100.0f), NULL)));
}

void AyamissFightStrategy::InitCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
        multipliers.push_back(new PreventMoveAwayFromCreatureOnReachToCastMultiplier(ai));
}

// ----- Ossirian the Unscarred -----
void OssirianFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
    {
        // Supremacy of the Unscarred enrage — ranged stay at distance
        triggers.push_back(new TriggerNode(
            "ossirian too close",
            NextAction::array(0, new NextAction("move away from ossirian", 100.0f), NULL)));
    }
}

void OssirianFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end ossirian fight",
        NextAction::array(0, new NextAction("disable ossirian fight strategy", 100.0f), NULL)));
}

void OssirianFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end ossirian fight",
        NextAction::array(0, new NextAction("disable ossirian fight strategy", 100.0f), NULL)));
}

void OssirianFightStrategy::InitCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
        multipliers.push_back(new PreventMoveAwayFromCreatureOnReachToCastMultiplier(ai));
}
