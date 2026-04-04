#pragma once
#include "playerbot/strategy/Strategy.h"

namespace ai
{
    class NaxxramasDungeonStrategy : public Strategy
    {
    public:
        NaxxramasDungeonStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "naxxramas"; }

    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
    };

    // Spider Wing
    class AnubRekhanFightStrategy : public Strategy
    {
    public:
        AnubRekhanFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "anub'rekhan"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
    };

    class FaerlinaFightStrategy : public Strategy
    {
    public:
        FaerlinaFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "faerlina"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
    };

    class MaexxnaFightStrategy : public Strategy
    {
    public:
        MaexxnaFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "maexxna"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
        void InitCombatMultipliers(std::list<Multiplier*>& multipliers) override;
    };

    // Plague Wing
    class NothFightStrategy : public Strategy
    {
    public:
        NothFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "noth"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
        void InitCombatMultipliers(std::list<Multiplier*>& multipliers) override;
    };

    class HeiganFightStrategy : public Strategy
    {
    public:
        HeiganFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "heigan"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
    };

    class LoathebFightStrategy : public Strategy
    {
    public:
        LoathebFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "loatheb"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
    };

    // Military Quarter
    class RazuviousFightStrategy : public Strategy
    {
    public:
        RazuviousFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "razuvious"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
    };

    class GothikFightStrategy : public Strategy
    {
    public:
        GothikFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "gothik"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
    };

    class FourHorsemanFightStrategy : public Strategy
    {
    public:
        FourHorsemanFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "four horseman"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
    };

    // Construct Quarter
    class PatchwerkFightStrategy : public Strategy
    {
    public:
        PatchwerkFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "patchwerk"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
    };

    class GrobbullusFightStrategy : public Strategy
    {
    public:
        GrobbullusFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "grobbulus"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
    };

    class GluthFightStrategy : public Strategy
    {
    public:
        GluthFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "gluth"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
        void InitCombatMultipliers(std::list<Multiplier*>& multipliers) override;
    };

    class ThaddiusFightStrategy : public Strategy
    {
    public:
        ThaddiusFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "thaddius"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
    };

    // Frostwyrm Lair
    class SapphironFightStrategy : public Strategy
    {
    public:
        SapphironFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "sapphiron"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
        void InitCombatMultipliers(std::list<Multiplier*>& multipliers) override;
    };

    class KelThuzadFightStrategy : public Strategy
    {
    public:
        KelThuzadFightStrategy(PlayerbotAI* ai) : Strategy(ai) {}
        std::string getName() override { return "kel'thuzad"; }
    private:
        void InitCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitNonCombatTriggers(std::list<TriggerNode*>& triggers) override;
        void InitDeadTriggers(std::list<TriggerNode*>& triggers) override;
        void InitCombatMultipliers(std::list<Multiplier*>& multipliers) override;
    };
}