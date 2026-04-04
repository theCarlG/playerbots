#pragma once
#include "playerbot/strategy/Strategy.h"

namespace ai
{
    class AQ40DungeonStrategy : public Strategy
    {
    public:
        AQ40DungeonStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "aq40"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
    };

    class SkeramFightStrategy : public Strategy
    {
    public:
        SkeramFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "skeram"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
        void InitCombatMultipliers(std::list<Multiplier*>& multipliers) override;
    };

    class BugTrioFightStrategy : public Strategy
    {
    public:
        BugTrioFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "bug trio"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
    };

    class SarturaFightStrategy : public Strategy
    {
    public:
        SarturaFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "sartura"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
        void InitCombatMultipliers(std::list<Multiplier*>& multipliers) override;
    };

    class FankrissFightStrategy : public Strategy
    {
    public:
        FankrissFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "fankriss"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
    };

    class ViscidusFightStrategy : public Strategy
    {
    public:
        ViscidusFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "viscidus"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
    };

    class HuhuranFightStrategy : public Strategy
    {
    public:
        HuhuranFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "huhuran"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
        void InitCombatMultipliers(std::list<Multiplier*>& multipliers) override;
    };

    class TwinEmperorsFightStrategy : public Strategy
    {
    public:
        TwinEmperorsFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "twin emperors"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
    };

    class OuroFightStrategy : public Strategy
    {
    public:
        OuroFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "ouro"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
        void InitCombatMultipliers(std::list<Multiplier*>& multipliers) override;
    };

    class CThunFightStrategy : public Strategy
    {
    public:
        CThunFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "c'thun"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
        void InitCombatMultipliers(std::list<Multiplier*>& multipliers) override;
    };
}
