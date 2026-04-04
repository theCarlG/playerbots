
#include "playerbot/playerbot.h"
#include "NaxxramasDungeonStrategies.h"
#include "DungeonMultipliers.h"

using namespace ai;

void NaxxramasDungeonStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "start four horseman fight",
        NextAction::array(0, new NextAction("enable four horseman fight strategy", 100.0f), NULL)));

    triggers.push_back(new TriggerNode(
        "start anub'rekhan fight",
        NextAction::array(0, new NextAction("enable anub'rekhan fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start faerlina fight",
        NextAction::array(0, new NextAction("enable faerlina fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start maexxna fight",
        NextAction::array(0, new NextAction("enable maexxna fight strategy", 100.0f), NULL)));

    triggers.push_back(new TriggerNode(
        "start noth fight",
        NextAction::array(0, new NextAction("enable noth fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start heigan fight",
        NextAction::array(0, new NextAction("enable heigan fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start loatheb fight",
        NextAction::array(0, new NextAction("enable loatheb fight strategy", 100.0f), NULL)));

    triggers.push_back(new TriggerNode(
        "start razuvious fight",
        NextAction::array(0, new NextAction("enable razuvious fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start gothik fight",
        NextAction::array(0, new NextAction("enable gothik fight strategy", 100.0f), NULL)));

    triggers.push_back(new TriggerNode(
        "start patchwerk fight",
        NextAction::array(0, new NextAction("enable patchwerk fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start grobbulus fight",
        NextAction::array(0, new NextAction("enable grobbulus fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start gluth fight",
        NextAction::array(0, new NextAction("enable gluth fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start thaddius fight",
        NextAction::array(0, new NextAction("enable thaddius fight strategy", 100.0f), NULL)));

    triggers.push_back(new TriggerNode(
        "start sapphiron fight",
        NextAction::array(0, new NextAction("enable sapphiron fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start kel'thuzad fight",
        NextAction::array(0, new NextAction("enable kel'thuzad fight strategy", 100.0f), NULL)));
}

// ----- Anub'Rekhan -----
void AnubRekhanFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (!ai->IsRanged(bot) && !ai->IsHeal(bot))
    {
        // Cleave — melee attack from behind
        triggers.push_back(new TriggerNode(
            "not behind target",
            NextAction::array(0, new NextAction("set behind", ACTION_NORMAL + 2), NULL)));
    }
}

void AnubRekhanFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end anub'rekhan fight",
        NextAction::array(0, new NextAction("disable anub'rekhan fight strategy", 100.0f), NULL)));
}

void AnubRekhanFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end anub'rekhan fight",
        NextAction::array(0, new NextAction("disable anub'rekhan fight strategy", 100.0f), NULL)));
}

// ----- Grand Widow Faerlina -----
void FaerlinaFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (!ai->IsRanged(bot) && !ai->IsHeal(bot))
    {
        // Faerlina has Cleave — melee attack from behind
        triggers.push_back(new TriggerNode(
            "not behind target",
            NextAction::array(0, new NextAction("set behind", ACTION_NORMAL + 2), NULL)));
    }
}

void FaerlinaFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end faerlina fight",
        NextAction::array(0, new NextAction("disable faerlina fight strategy", 100.0f), NULL)));
}

void FaerlinaFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end faerlina fight",
        NextAction::array(0, new NextAction("disable faerlina fight strategy", 100.0f), NULL)));
}

// ----- Maexxna -----
void MaexxnaFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
    {
        // Web Spray hits melee range — ranged stay back
        triggers.push_back(new TriggerNode(
            "maexxna too close",
            NextAction::array(0, new NextAction("move away from maexxna", 100.0f), NULL)));
    }
}

void MaexxnaFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end maexxna fight",
        NextAction::array(0, new NextAction("disable maexxna fight strategy", 100.0f), NULL)));
}

void MaexxnaFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end maexxna fight",
        NextAction::array(0, new NextAction("disable maexxna fight strategy", 100.0f), NULL)));
}

void MaexxnaFightStrategy::InitCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
        multipliers.push_back(new PreventMoveAwayFromCreatureOnReachToCastMultiplier(ai));
}

// ----- Noth the Plaguebringer -----
void NothFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
    {
        // Noth has Cripple and Curse of the Plaguebringer — ranged maintain distance
        triggers.push_back(new TriggerNode(
            "noth too close",
            NextAction::array(0, new NextAction("move away from noth", 100.0f), NULL)));
    }
}

void NothFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end noth fight",
        NextAction::array(0, new NextAction("disable noth fight strategy", 100.0f), NULL)));
}

void NothFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end noth fight",
        NextAction::array(0, new NextAction("disable noth fight strategy", 100.0f), NULL)));
}

void NothFightStrategy::InitCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
        multipliers.push_back(new PreventMoveAwayFromCreatureOnReachToCastMultiplier(ai));
}

// ----- Heigan the Unclean -----
void HeiganFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
}

void HeiganFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end heigan fight",
        NextAction::array(0, new NextAction("disable heigan fight strategy", 100.0f), NULL)));
}

void HeiganFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end heigan fight",
        NextAction::array(0, new NextAction("disable heigan fight strategy", 100.0f), NULL)));
}

// ----- Loatheb -----
void LoathebFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
}

void LoathebFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end loatheb fight",
        NextAction::array(0, new NextAction("disable loatheb fight strategy", 100.0f), NULL)));
}

void LoathebFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end loatheb fight",
        NextAction::array(0, new NextAction("disable loatheb fight strategy", 100.0f), NULL)));
}

// ----- Instructor Razuvious -----
void RazuviousFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
}

void RazuviousFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end razuvious fight",
        NextAction::array(0, new NextAction("disable razuvious fight strategy", 100.0f), NULL)));
}

void RazuviousFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end razuvious fight",
        NextAction::array(0, new NextAction("disable razuvious fight strategy", 100.0f), NULL)));
}

// ----- Gothik the Harvester -----
void GothikFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
}

void GothikFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end gothik fight",
        NextAction::array(0, new NextAction("disable gothik fight strategy", 100.0f), NULL)));
}

void GothikFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end gothik fight",
        NextAction::array(0, new NextAction("disable gothik fight strategy", 100.0f), NULL)));
}

// ----- The Four Horsemen -----
void FourHorsemanFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "void zone too close",
        NextAction::array(0, new NextAction("move away from void zone", 100.0f), NULL)));
}

void FourHorsemanFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end four horseman fight",
        NextAction::array(0, new NextAction("disable four horseman fight strategy", 100.0f), NULL)));
}

void FourHorsemanFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end four horseman fight",
        NextAction::array(0, new NextAction("disable four horseman fight strategy", 100.0f), NULL)));
}

// ----- Patchwerk -----
void PatchwerkFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
}

void PatchwerkFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end patchwerk fight",
        NextAction::array(0, new NextAction("disable patchwerk fight strategy", 100.0f), NULL)));
}

void PatchwerkFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end patchwerk fight",
        NextAction::array(0, new NextAction("disable patchwerk fight strategy", 100.0f), NULL)));
}

// ----- Grobbulus -----
void GrobbullusFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    // Mutating Injection: affected bot must immediately run away before it explodes
    triggers.push_back(new TriggerNode(
        "has mutating injection",
        NextAction::array(0, new NextAction("grobbulus run away", 150.0f), NULL)));
}

void GrobbullusFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end grobbulus fight",
        NextAction::array(0, new NextAction("disable grobbulus fight strategy", 100.0f), NULL)));
}

void GrobbullusFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end grobbulus fight",
        NextAction::array(0, new NextAction("disable grobbulus fight strategy", 100.0f), NULL)));
}

// ----- Gluth -----
void GluthFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
    {
        // Gluth has a massive Cleave — ranged stay back
        triggers.push_back(new TriggerNode(
            "gluth too close",
            NextAction::array(0, new NextAction("move away from gluth", 100.0f), NULL)));
    }
}

void GluthFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end gluth fight",
        NextAction::array(0, new NextAction("disable gluth fight strategy", 100.0f), NULL)));
}

void GluthFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end gluth fight",
        NextAction::array(0, new NextAction("disable gluth fight strategy", 100.0f), NULL)));
}

void GluthFightStrategy::InitCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
        multipliers.push_back(new PreventMoveAwayFromCreatureOnReachToCastMultiplier(ai));
}

// ----- Thaddius -----
void ThaddiusFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
}

void ThaddiusFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end thaddius fight",
        NextAction::array(0, new NextAction("disable thaddius fight strategy", 100.0f), NULL)));
}

void ThaddiusFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end thaddius fight",
        NextAction::array(0, new NextAction("disable thaddius fight strategy", 100.0f), NULL)));
}

// ----- Sapphiron -----
void SapphironFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
    {
        // Frost Breath and Blizzard — ranged maintain distance
        triggers.push_back(new TriggerNode(
            "sapphiron too close",
            NextAction::array(0, new NextAction("move away from sapphiron", 100.0f), NULL)));
    }
}

void SapphironFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end sapphiron fight",
        NextAction::array(0, new NextAction("disable sapphiron fight strategy", 100.0f), NULL)));
}

void SapphironFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end sapphiron fight",
        NextAction::array(0, new NextAction("disable sapphiron fight strategy", 100.0f), NULL)));
}

void SapphironFightStrategy::InitCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
        multipliers.push_back(new PreventMoveAwayFromCreatureOnReachToCastMultiplier(ai));
}

// ----- Kel'Thuzad -----
void KelThuzadFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
    {
        // Frost Bolt Volley and Void Zone circles — ranged stay at range
        triggers.push_back(new TriggerNode(
            "kel'thuzad too close",
            NextAction::array(0, new NextAction("move away from kel'thuzad", 100.0f), NULL)));
    }
}

void KelThuzadFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end kel'thuzad fight",
        NextAction::array(0, new NextAction("disable kel'thuzad fight strategy", 100.0f), NULL)));
}

void KelThuzadFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end kel'thuzad fight",
        NextAction::array(0, new NextAction("disable kel'thuzad fight strategy", 100.0f), NULL)));
}

void KelThuzadFightStrategy::InitCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
        multipliers.push_back(new PreventMoveAwayFromCreatureOnReachToCastMultiplier(ai));
}
