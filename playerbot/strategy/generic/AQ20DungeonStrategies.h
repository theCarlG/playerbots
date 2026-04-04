#pragma once
#include "playerbot/strategy/Strategy.h"

namespace ai
{
    class AQ20DungeonStrategy : public Strategy
    {
    public:
        AQ20DungeonStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "aq20"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
    };

    class KurinnaxxFightStrategy : public Strategy
    {
    public:
        KurinnaxxFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "kurinnaxx"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
        void InitCombatMultipliers(std::list<Multiplier*>& multipliers) override;
    };

    class RajaxxFightStrategy : public Strategy
    {
    public:
        RajaxxFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "rajaxx"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
        void InitCombatMultipliers(std::list<Multiplier*>& multipliers) override;
    };

    class MoamFightStrategy : public Strategy
    {
    public:
        MoamFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "moam"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
    };

    class BuruFightStrategy : public Strategy
    {
    public:
        BuruFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "buru"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
    };

    class AyamissFightStrategy : public Strategy
    {
    public:
        AyamissFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "ayamiss"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
        void InitCombatMultipliers(std::list<Multiplier*>& multipliers) override;
    };

    class OssirianFightStrategy : public Strategy
    {
    public:
        OssirianFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "ossirian"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
        void InitCombatMultipliers(std::list<Multiplier*>& multipliers) override;
    };
}
