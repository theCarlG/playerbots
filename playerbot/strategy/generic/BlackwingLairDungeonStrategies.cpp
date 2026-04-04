#include "playerbot/playerbot.h"
#include "BlackwingLairDungeonStrategies.h"
#include "DungeonMultipliers.h"

using namespace ai;

void BlackwingLairDungeonStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "suppression device close",
        NextAction::array(0, new NextAction("disarm suppression device", 80.0f), NULL)));

    triggers.push_back(new TriggerNode(
        "start razorgore fight",
        NextAction::array(0, new NextAction("enable razorgore fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start vaelastrasz fight",
        NextAction::array(0, new NextAction("enable vaelastrasz fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start broodlord fight",
        NextAction::array(0, new NextAction("enable broodlord fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start firemaw fight",
        NextAction::array(0, new NextAction("enable firemaw fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start ebonroc fight",
        NextAction::array(0, new NextAction("enable ebonroc fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start flamegor fight",
        NextAction::array(0, new NextAction("enable flamegor fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start chromaggus fight",
        NextAction::array(0, new NextAction("enable chromaggus fight strategy", 100.0f), NULL)));
    triggers.push_back(new TriggerNode(
        "start nefarian fight",
        NextAction::array(0, new NextAction("enable nefarian fight strategy", 100.0f), NULL)));
}

void BlackwingLairDungeonStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "suppression device need stealth",
        NextAction::array(0, new NextAction("stealth for suppression device", ACTION_HIGH + 3), NULL)));

    triggers.push_back(new TriggerNode(
        "suppression device in sight",
        NextAction::array(0, new NextAction("move to suppression device", ACTION_HIGH + 2), NULL)));

    triggers.push_back(new TriggerNode(
        "suppression device close",
        NextAction::array(0, new NextAction("disarm suppression device", ACTION_HIGH + 4), NULL)));
}

class SuppressionRoomPassiveMultiplier : public Multiplier
{
public:
    SuppressionRoomPassiveMultiplier(PlayerbotAI* ai) : Multiplier(ai, "suppression room passive") {}

    float GetValue(Action* action) override
    {
        if (!action)
            return 1.0f;

        if (ai->GetBot()->getClass() != CLASS_ROGUE)
            return 1.0f;

        const std::string& name = action->getName();

        // Enable only the following strats for suppression room to avoid regular combat breaking logic
        if (name == "stealth for suppression device" ||
            name == "move to suppression device" ||
            name == "disarm suppression device" ||
            name == "deactivate suppression device")
        {
            return 1.0f;
        }

        if (name == "stealth" ||
            name == "unstealth" ||
            name == "check stealth" ||
            name == "sprint" ||
            name == "vanish")
        {
            return 1.0f;
        }

        if (name == "co" ||
            name == "nc" ||
            name == "load ai" ||
            name == "save ai" ||
            name == "list ai" ||
            name == "reset ai" ||
            name == "reset strats" ||
            name == "reset values" ||
            name == "check mount state" ||
            name == "accept invitation" ||
            name == "set combat state" ||
            name == "set non combat state" ||
            name == "set dead state" ||
            name == "update pvp strats" ||
            name == "update pve strats" ||
            name == "update raid strats" ||
            name == "loot roll" ||
            name == "auto loot roll" ||
            name == "follow" ||
            name == "stay" ||
            name == "food" ||
            name == "drink")
        {
            return 1.0f;
        }

        return 0.0f;
    }
};

void SuppressionRoomStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "suppression device need stealth",
        NextAction::array(0, new NextAction("vanish", ACTION_EMERGENCY + 1), NULL)));

    triggers.push_back(new TriggerNode(
        "suppression device in sight",
        NextAction::array(0, new NextAction("move to suppression device", ACTION_HIGH + 8), NULL)));

    triggers.push_back(new TriggerNode(
        "suppression device close",
        NextAction::array(0, new NextAction("disarm suppression device", 90.0f), NULL)));
}

void SuppressionRoomStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "suppression device need stealth",
        NextAction::array(0, new NextAction("stealth for suppression device", ACTION_MOVE), NULL)));

    triggers.push_back(new TriggerNode(
        "suppression device in sight",
        NextAction::array(0, new NextAction("move to suppression device", ACTION_HIGH + 8), NULL)));

    triggers.push_back(new TriggerNode(
        "suppression device close",
        NextAction::array(0, new NextAction("disarm suppression device", ACTION_MOVE + 2), NULL)));
}

void SuppressionRoomStrategy::InitCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    multipliers.push_back(new SuppressionRoomPassiveMultiplier(ai));
}

void SuppressionRoomStrategy::InitNonCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    multipliers.push_back(new SuppressionRoomPassiveMultiplier(ai));
}

void SuppressionRoomStrategy::OnStrategyAdded(BotState state)
{
    if (ai->GetBot()->getClass() == CLASS_ROGUE)
    {
        ai->ChangeStrategy("-avoid aoe", BotState::BOT_STATE_COMBAT);
        ai->ChangeStrategy("-avoid aoe", BotState::BOT_STATE_NON_COMBAT);
        ai->ChangeStrategy("-avoid aoe", BotState::BOT_STATE_REACTION);
        ai->ChangeStrategy("-avoid mobs", BotState::BOT_STATE_COMBAT);
        ai->ChangeStrategy("-avoid mobs", BotState::BOT_STATE_NON_COMBAT);
        ai->ChangeStrategy("-avoid mobs", BotState::BOT_STATE_REACTION);
    }
}

// ----- Razorgore the Untamed -----
void RazorgoreFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
}

void RazorgoreFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end razorgore fight",
        NextAction::array(0, new NextAction("disable razorgore fight strategy", 100.0f), NULL)));
}

void RazorgoreFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end razorgore fight",
        NextAction::array(0, new NextAction("disable razorgore fight strategy", 100.0f), NULL)));
}

// ----- Vaelastrasz the Corrupt -----
void VaelastraszFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    // Burning Adrenaline: affected bot must immediately run away before the explosion
    triggers.push_back(new TriggerNode(
        "has burning adrenaline",
        NextAction::array(0, new NextAction("vaelastrasz run away", 150.0f), NULL)));
}

void VaelastraszFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end vaelastrasz fight",
        NextAction::array(0, new NextAction("disable vaelastrasz fight strategy", 100.0f), NULL)));
}

void VaelastraszFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end vaelastrasz fight",
        NextAction::array(0, new NextAction("disable vaelastrasz fight strategy", 100.0f), NULL)));
}

// ----- Broodlord Lashlayer -----
void BroodlordFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (!ai->IsRanged(bot) && !ai->IsHeal(bot))
    {
        // Broodlord has Cleave — melee attack from behind
        triggers.push_back(new TriggerNode(
            "not behind target",
            NextAction::array(0, new NextAction("set behind", ACTION_NORMAL + 2), NULL)));
    }
}

void BroodlordFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end broodlord fight",
        NextAction::array(0, new NextAction("disable broodlord fight strategy", 100.0f), NULL)));
}

void BroodlordFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end broodlord fight",
        NextAction::array(0, new NextAction("disable broodlord fight strategy", 100.0f), NULL)));
}

// ----- Firemaw -----
void FiremawFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
    {
        // Wing Buffet knocks back players in melee range
        triggers.push_back(new TriggerNode(
            "firemaw too close",
            NextAction::array(0, new NextAction("move away from firemaw", 100.0f), NULL)));
    }
}

void FiremawFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end firemaw fight",
        NextAction::array(0, new NextAction("disable firemaw fight strategy", 100.0f), NULL)));
}

void FiremawFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end firemaw fight",
        NextAction::array(0, new NextAction("disable firemaw fight strategy", 100.0f), NULL)));
}

void FiremawFightStrategy::InitCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
        multipliers.push_back(new PreventMoveAwayFromCreatureOnReachToCastMultiplier(ai));
}

// ----- Ebonroc -----
void EbonrocFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
    {
        triggers.push_back(new TriggerNode(
            "ebonroc too close",
            NextAction::array(0, new NextAction("move away from ebonroc", 100.0f), NULL)));
    }
}

void EbonrocFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end ebonroc fight",
        NextAction::array(0, new NextAction("disable ebonroc fight strategy", 100.0f), NULL)));
}

void EbonrocFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end ebonroc fight",
        NextAction::array(0, new NextAction("disable ebonroc fight strategy", 100.0f), NULL)));
}

void EbonrocFightStrategy::InitCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
        multipliers.push_back(new PreventMoveAwayFromCreatureOnReachToCastMultiplier(ai));
}

// ----- Flamegor -----
void FlamegorFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
    {
        triggers.push_back(new TriggerNode(
            "flamegor too close",
            NextAction::array(0, new NextAction("move away from flamegor", 100.0f), NULL)));
    }
}

void FlamegorFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end flamegor fight",
        NextAction::array(0, new NextAction("disable flamegor fight strategy", 100.0f), NULL)));
}

void FlamegorFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end flamegor fight",
        NextAction::array(0, new NextAction("disable flamegor fight strategy", 100.0f), NULL)));
}

void FlamegorFightStrategy::InitCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
        multipliers.push_back(new PreventMoveAwayFromCreatureOnReachToCastMultiplier(ai));
}

// ----- Chromaggus -----
void ChromaggusFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
    {
        // Chromaggus has heavy AoE breath attacks — stay at range
        triggers.push_back(new TriggerNode(
            "chromaggus too close",
            NextAction::array(0, new NextAction("move away from chromaggus", 100.0f), NULL)));
    }
}

void ChromaggusFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end chromaggus fight",
        NextAction::array(0, new NextAction("disable chromaggus fight strategy", 100.0f), NULL)));
}

void ChromaggusFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end chromaggus fight",
        NextAction::array(0, new NextAction("disable chromaggus fight strategy", 100.0f), NULL)));
}

void ChromaggusFightStrategy::InitCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
        multipliers.push_back(new PreventMoveAwayFromCreatureOnReachToCastMultiplier(ai));
}

// ----- Nefarian -----
void NefarianFightStrategy::InitCombatTriggers(std::list<TriggerNode*>& triggers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
    {
        // Nefarian's Cleave and Tail Sweep — ranged stay out of melee range
        triggers.push_back(new TriggerNode(
            "nefarian too close",
            NextAction::array(0, new NextAction("move away from nefarian", 100.0f), NULL)));
    }
    else
    {
        // Melee attack from behind to avoid Cleave
        triggers.push_back(new TriggerNode(
            "not behind target",
            NextAction::array(0, new NextAction("set behind", ACTION_NORMAL + 2), NULL)));
    }
}

void NefarianFightStrategy::InitNonCombatTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end nefarian fight",
        NextAction::array(0, new NextAction("disable nefarian fight strategy", 100.0f), NULL)));
}

void NefarianFightStrategy::InitDeadTriggers(std::list<TriggerNode*>& triggers)
{
    triggers.push_back(new TriggerNode(
        "end nefarian fight",
        NextAction::array(0, new NextAction("disable nefarian fight strategy", 100.0f), NULL)));
}

void NefarianFightStrategy::InitCombatMultipliers(std::list<Multiplier*>& multipliers)
{
    Player* bot = ai->GetBot();
    if (ai->IsRanged(bot) || ai->IsHeal(bot))
        multipliers.push_back(new PreventMoveAwayFromCreatureOnReachToCastMultiplier(ai));
}