#pragma once
#include "MovementActions.h"
#include "playerbot/strategy/values/HazardsValue.h"

namespace ai
{
    class MoveAwayFromHazard : public MovementAction
    {
    public:
        MoveAwayFromHazard(PlayerbotAI* ai, std::string name = "move away from hazard") : MovementAction(ai, name) {}
        bool Execute(Event& event) override;
        bool isPossible() override;

#ifdef GenerateBotHelp
        virtual std::string GetHelpName() { return "move away from hazard"; }
        virtual std::string GetHelpDescription()
        {
            return "This action makes the bot move away from hazardous areas in dungeons.\n"
                   "It identifies dangerous positions and navigates to a safer location.";
        }
        virtual std::vector<std::string> GetUsedActions() { return {}; }
        virtual std::vector<std::string> GetUsedValues() { return {"hazards"}; }
#endif 

    private:
        bool IsHazardNearby(const WorldPosition& point, const std::list<HazardPosition>& hazards) const;
    };

    // Base action for any "run away from the group" debuff mechanic (Living Bomb, Burning Adrenaline, etc.)
    // Moves the bot ~25 yards away from the average position of party members,
    // using LOS + pathfinding validation to avoid lava/hazards.
    class RunAwayFromGroupAction : public MovementAction
    {
    public:
        RunAwayFromGroupAction(PlayerbotAI* ai, std::string name = "run away from group") : MovementAction(ai, name) {}
        bool Execute(Event& event) override;
        bool isPossible() override;

    private:
        bool IsHazardNearby(const WorldPosition& point, const std::list<HazardPosition>& hazards) const;
    };

    // Moves the bot to melee range of the nearest alive creature of the given entry.
    // Used for "stack on mob" mechanics where everyone must be in melee range.
    class MoveCloseToCreature : public MovementAction
    {
    public:
        MoveCloseToCreature(PlayerbotAI* ai, std::string name, uint32 creatureID, float targetRange)
        : MovementAction(ai, name), creatureID(creatureID), targetRange(targetRange) {}
        bool Execute(Event& event) override;
        bool isPossible() override;

    protected:
        uint32 creatureID;
        float targetRange;
    };

    class MoveAwayFromCreature : public MovementAction
    {
    public:
        MoveAwayFromCreature(PlayerbotAI* ai, std::string name, uint32 creatureID, float range) : MovementAction(ai, name), creatureID(creatureID), range(range) {}
        bool Execute(Event& event) override;
        bool isPossible() override;

#ifdef GenerateBotHelp
        virtual std::string GetHelpName() { return "move away from creature"; }
        virtual std::string GetHelpDescription()
        {
            return "This action makes the bot move away from a specific creature in dungeons.\n"
                   "It maintains a safe distance from the specified creature ID within a defined range.";
        }
        virtual std::vector<std::string> GetUsedActions() { return {}; }
        virtual std::vector<std::string> GetUsedValues() { return {"hazards"}; }
#endif 

    private:
        bool IsValidPoint(const WorldPosition& point, const std::list<Creature*>& creatures, const std::list<HazardPosition>& hazards);
        bool HasCreaturesNearby(const WorldPosition& point, const std::list<Creature*>& creatures) const;
        bool IsHazardNearby(const WorldPosition& point, const std::list<HazardPosition>& hazards) const;

    private:
        uint32 creatureID;
        float range;
    };
}
