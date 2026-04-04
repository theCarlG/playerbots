
#include "playerbot/playerbot.h"
#include "MoltenCoreDungeonActions.h"
#include "playerbot/strategy/AiObjectContext.h"
#include "playerbot/strategy/values/PositionValue.h"

using namespace ai;

bool BaronGeddonRunAwayAction::Execute(Event& event)
{
    // Compute average position of alive party members on the same map (excluding this bot)
    Group* group = bot->GetGroup();
    if (!group)
        return false;

    float sumX = 0.0f, sumY = 0.0f;
    int count = 0;
    for (GroupReference* ref = group->GetFirstMember(); ref; ref = ref->next())
    {
        Player* member = ref->getSource();
        if (member && member != bot && member->IsAlive() && member->GetMapId() == bot->GetMapId())
        {
            sumX += member->GetPositionX();
            sumY += member->GetPositionY();
            ++count;
        }
    }

    if (count == 0)
        return false;

    const float groupCenterX = sumX / count;
    const float groupCenterY = sumY / count;

    // Direction from group center toward the bot (i.e. run away from group)
    float dx = bot->GetPositionX() - groupCenterX;
    float dy = bot->GetPositionY() - groupCenterY;
    const float dist = sqrt(dx * dx + dy * dy);
    if (dist < 0.1f)
    {
        // Bot is on top of group center — pick a random direction
        const float randomAngle = frand(0.0f, static_cast<float>(2.0 * M_PI));
        dx = cos(randomAngle);
        dy = sin(randomAngle);
    }
    else
    {
        dx /= dist;
        dy /= dist;
    }

    const std::list<HazardPosition>& hazards = AI_VALUE(std::list<HazardPosition>, "hazards");
    const WorldPosition botPosition(bot);
    const float runDistance = 25.0f;

    // Base angle away from group
    float angle = atan2(dy, dx);
    const uint8 attempts = 10;
    const float angleIncrement = static_cast<float>(2.0 * M_PI / attempts);

    for (uint8 i = 0; i < attempts; i++)
    {
        WorldPosition candidate(bot->GetMapId(),
            bot->GetPositionX() + runDistance * cos(angle),
            bot->GetPositionY() + runDistance * sin(angle),
            bot->GetPositionZ(), 0.0f);
        candidate.setZ(candidate.getHeight());

        if (!IsHazardNearby(candidate, hazards) &&
            bot->IsWithinLOS(candidate.getX(), candidate.getY(), candidate.getZ() + bot->GetCollisionHeight()) &&
            botPosition.canPathTo(candidate, bot))
        {
            if (MoveTo(bot->GetMapId(), candidate.getX(), candidate.getY(), candidate.getZ(), false, false, false, true))
                return true;
        }

        angle += angleIncrement;
    }

    return false;
}

bool BaronGeddonRunAwayAction::isPossible()
{
    if (MovementAction::isPossible())
        return ai->CanMove();
    return false;
}

bool BaronGeddonRunAwayAction::IsHazardNearby(const WorldPosition& point, const std::list<HazardPosition>& hazards) const
{
    for (const HazardPosition& hazard : hazards)
    {
        if (point.distance(hazard.first) < hazard.second)
            return true;
    }
    return false;
}