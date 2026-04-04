#pragma once
#include "DungeonActions.h"
#include "ChangeStrategyAction.h"
#include "UseItemAction.h"
#include "playerbot/strategy/values/HazardsValue.h"

namespace ai
{
    class MoltenCoreEnableDungeonStrategyAction : public ChangeAllStrategyAction
    {
    public:
        MoltenCoreEnableDungeonStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable molten core strategy", "+molten core") {}
    };

    class MoltenCoreDisableDungeonStrategyAction : public ChangeAllStrategyAction
    {
    public:
        MoltenCoreDisableDungeonStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable molten core strategy", "-molten core") {}
    };

    class MagmadarEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        MagmadarEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable magmadar fight strategy", "+magmadar") {}
    };

    class MagmadarDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        MagmadarDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable magmadar fight strategy", "-magmadar") {}
    };

    class MagmadarMoveAwayFromLavaBombAction : public MoveAwayFromHazard
    {
    public:
        MagmadarMoveAwayFromLavaBombAction(PlayerbotAI* ai) : MoveAwayFromHazard(ai, "move away from magmadar lava bomb") {}
    };

    class MagmadarMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        MagmadarMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from magmadar", 11982, 31.0f) {}
    };

    class MoveToMCRuneAction : public MoveToAction
    {
    public:
        MoveToMCRuneAction(PlayerbotAI* ai) : MoveToAction(ai, "move to mc rune") { qualifier = "entry filter::{gos in sight,mc runes}"; }
    };

    class DouseMCRuneActionAqual : public UseItemIdAction
    {
    public:
        DouseMCRuneActionAqual(PlayerbotAI* ai) : UseItemIdAction(ai, "douse mc rune aqual") { qualifier = "{17333,entry filter::{gos close,mc runes}}"; }
    };

    class DouseMCRuneActionEternal : public UseItemIdAction
    {
    public:
        DouseMCRuneActionEternal(PlayerbotAI* ai) : UseItemIdAction(ai, "douse mc rune eternal") { qualifier = "{22754,entry filter::{gos close,mc runes}}"; }
    };

    class BaronGeddonEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        BaronGeddonEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable baron geddon fight strategy", "+baron geddon") {}
    };

    class BaronGeddonDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        BaronGeddonDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable baron geddon fight strategy", "-baron geddon") {}
    };

    // When this bot has the Living Bomb debuff, move away from party members to avoid chain explosion
    class BaronGeddonRunAwayAction : public MovementAction
    {
    public:
        BaronGeddonRunAwayAction(PlayerbotAI* ai) : MovementAction(ai, "baron geddon run away") {}
        bool Execute(Event& event) override;
        bool isPossible() override;

    private:
        bool IsHazardNearby(const WorldPosition& point, const std::list<HazardPosition>& hazards) const;
    };
}