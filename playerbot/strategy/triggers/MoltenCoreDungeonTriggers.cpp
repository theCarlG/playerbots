
#include "playerbot/playerbot.h"
#include "MoltenCoreDungeonTriggers.h"
#include "playerbot/strategy/AiObjectContext.h"
#include "Grids/GridNotifiers.h"
#include "Grids/GridNotifiersImpl.h"
#include "Grids/CellImpl.h"
#include "Groups/Group.h"

using namespace ai;

bool TankFightingCoreHoundNearGroupTrigger::IsActive()
{
    if (!bot->IsInWorld() || bot->IsBeingTeleported())
        return false;

    if (!ai->IsTank(bot))
        return false;

    Unit* target = AI_VALUE(Unit*, "current target");
    if (!target)
        return false;

    Creature* creature = dynamic_cast<Creature*>(target);
    if (!creature || creature->GetEntry() != 11671)
        return false;

    // Trigger only if raid members are still nearby (need to drag the Core Hound away)
    Group* group = bot->GetGroup();
    if (!group)
        return false;

    for (GroupReference* ref = group->GetFirstMember(); ref; ref = ref->next())
    {
        Player* member = ref->getSource();
        if (member && member != bot && member->IsAlive() && member->GetMapId() == bot->GetMapId())
        {
            if (bot->GetDistance(member) < 15.0f)
                return true;
        }
    }

    return false;
}

bool BaronGeddonInfernoActiveTrigger::IsActive()
{
    if (!bot->IsInWorld() || bot->IsBeingTeleported())
        return false;

    const float searchRadius = 60.0f;
    std::list<Unit*> units;
    MaNGOS::AllCreaturesOfEntryInRangeCheck u_check(bot, 12056, searchRadius);
    MaNGOS::UnitListSearcher<MaNGOS::AllCreaturesOfEntryInRangeCheck> searcher(units, u_check);
    Cell::VisitAllObjects(bot, searcher, searchRadius);

    for (Unit* unit : units)
    {
        Creature* geddon = dynamic_cast<Creature*>(unit);
        if (geddon && geddon->IsAlive() && ai->HasAura("inferno", geddon))
            return true;
    }

    return false;
}