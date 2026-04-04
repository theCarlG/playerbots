
#include "playerbot/playerbot.h"
#include "AQ40DungeonStrategies.h"
#include "DungeonMultipliers.h"

using namespace ai;

void AQ40DungeonStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "start skeram fight",
        NextAction::array(0, new NextAction("enable skeram fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start bug trio fight",
        NextAction::array(0, new NextAction("enable bug trio fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start sartura fight",
        NextAction::array(0, new NextAction("enable sartura fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start fankriss fight",
        NextAction::array(0, new NextAction("enable fankriss fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start viscidus fight",
        NextAction::array(0, new NextAction("enable viscidus fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start huhuran fight",
        NextAction::array(0, new NextAction("enable huhuran fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start twin emperors fight",
        NextAction::array(0, new NextAction("enable twin emperors fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start ouro fight",
        NextAction::array(0, new NextAction("enable ouro fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start c'thun fight",
        NextAction::array(0, new NextAction("enable c'thun fight strategy", 100.0f), NULL)));
}

// ----- The Prophet Skeram -----
void SkeramFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
    {
        // Skeram teleports around; ranged maintain safe distance to avoid melee hits
        triggers.push_back(new TriggerNode(
            "skeram too close",
            NextAction::array(0, new NextAction("move away from skeram", 100.0f), NULL)));
    }
}

void SkeramFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end skeram fight",
        NextAction::array(0, new NextAction("disable skeram fight strategy", 100.0f), NULL)));
}

void SkeramFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end skeram fight",
        NextAction::array(0, new NextAction("disable skeram fight strategy", 100.0f), NULL)));
}

void SkeramFightStrategy::InitCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
        multipliers.push_back(new PreventMoveAwayFromCreatureOnReachToCastMultiplier(ai));
}

// ----- Bug Trio -----
void BugTrioFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
}

void BugTrioFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end bug trio fight",
        NextAction::array(0, new NextAction("disable bug trio fight strategy", 100.0f), NULL)));
}

void BugTrioFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end bug trio fight",
        NextAction::array(0, new NextAction("disable bug trio fight strategy", 100.0f), NULL)));
}

// ----- Battleguard Sartura -----
void SarturaFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    // Whirlwind — all bots maintain distance (8 yard trigger, move to 10 yards)
    triggers.push_back(new TriggerNode(
        "sartura too close",
        NextAction::array(0, new NextAction("move away from sartura", 100.0f), NULL)));
}

void SarturaFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end sartura fight",
        NextAction::array(0, new NextAction("disable sartura fight strategy", 100.0f), NULL)));
}

void SarturaFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end sartura fight",
        NextAction::array(0, new NextAction("disable sartura fight strategy", 100.0f), NULL)));
}

void SarturaFightStrategy::InitCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    multipliers.push_back(new PreventMoveAwayFromCreatureOnReachToCastMultiplier(ai));
}

// ----- Fankriss the Unyielding -----
void FankrissFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
}

void FankrissFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end fankriss fight",
        NextAction::array(0, new NextAction("disable fankriss fight strategy", 100.0f), NULL)));
}

void FankrissFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end fankriss fight",
        NextAction::array(0, new NextAction("disable fankriss fight strategy", 100.0f), NULL)));
}

// ----- Viscidus -----
void ViscidusFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
}

void ViscidusFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end viscidus fight",
        NextAction::array(0, new NextAction("disable viscidus fight strategy", 100.0f), NULL)));
}

void ViscidusFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end viscidus fight",
        NextAction::array(0, new NextAction("disable viscidus fight strategy", 100.0f), NULL)));
}

// ----- Princess Huhuran -----
void HuhuranFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
    {
        // Wyvern Sting and Frenzy — ranged maintain distance
        triggers.push_back(new TriggerNode(
            "huhuran too close",
            NextAction::array(0, new NextAction("move away from huhuran", 100.0f), NULL)));
    }
}

void HuhuranFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end huhuran fight",
        NextAction::array(0, new NextAction("disable huhuran fight strategy", 100.0f), NULL)));
}

void HuhuranFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end huhuran fight",
        NextAction::array(0, new NextAction("disable huhuran fight strategy", 100.0f), NULL)));
}

void HuhuranFightStrategy::InitCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
        multipliers.push_back(new PreventMoveAwayFromCreatureOnReachToCastMultiplier(ai));
}

// ----- Twin Emperors -----
void TwinEmperorsFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
}

void TwinEmperorsFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end twin emperors fight",
        NextAction::array(0, new NextAction("disable twin emperors fight strategy", 100.0f), NULL)));
}

void TwinEmperorsFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end twin emperors fight",
        NextAction::array(0, new NextAction("disable twin emperors fight strategy", 100.0f), NULL)));
}

// ----- Ouro -----
void OuroFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
    {
        // Ground Slam knocks back players in front — ranged maintain distance
        triggers.push_back(new TriggerNode(
            "ouro too close",
            NextAction::array(0, new NextAction("move away from ouro", 100.0f), NULL)));
    }
}

void OuroFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end ouro fight",
        NextAction::array(0, new NextAction("disable ouro fight strategy", 100.0f), NULL)));
}

void OuroFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end ouro fight",
        NextAction::array(0, new NextAction("disable ouro fight strategy", 100.0f), NULL)));
}

void OuroFightStrategy::InitCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
        multipliers.push_back(new PreventMoveAwayFromCreatureOnReachToCastMultiplier(ai));
}

// ----- C'Thun -----
void CThunFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
    {
        // Eye Beam and tentacle attacks — ranged stay spread and at distance
        triggers.push_back(new TriggerNode(
            "c'thun too close",
            NextAction::array(0, new NextAction("move away from c'thun", 100.0f), NULL)));
    }
}

void CThunFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end c'thun fight",
        NextAction::array(0, new NextAction("disable c'thun fight strategy", 100.0f), NULL)));
}

void CThunFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end c'thun fight",
        NextAction::array(0, new NextAction("disable c'thun fight strategy", 100.0f), NULL)));
}

void CThunFightStrategy::InitCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
        multipliers.push_back(new PreventMoveAwayFromCreatureOnReachToCastMultiplier(ai));
}
