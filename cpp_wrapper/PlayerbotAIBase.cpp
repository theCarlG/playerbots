
#include "PlayerbotAIBase.h"
#include "botffi.h"
#include "playerbotDefs.h"
#include "Entities/Player.h"
#include "Globals/SharedDefines.h"

WorldPosition::WorldPosition(Player* p)
    : x(p->GetPositionX()), y(p->GetPositionY()), z(p->GetPositionZ()),
      o(p->GetOrientation()), mapid(p->GetMapId()) {}

PlayerbotAIBase::PlayerbotAIBase() : aiInternalUpdateDelay(0)
{
}

void PlayerbotAIBase::UpdateAIInternal(uint32 /*elapsed*/, bool /*minimal*/)
{
}

void PlayerbotAIBase::UpdateAI(uint32 elapsed)
{
    if (aiInternalUpdateDelay > elapsed)
        aiInternalUpdateDelay -= elapsed;
    else
        aiInternalUpdateDelay = 0;

    if (!CanUpdateAIInternal())
        return;

    UpdateAIInternal(elapsed);
    YieldAIInternalThread();
}

void PlayerbotAIBase::SetAIInternalUpdateDelay(uint32 delay)
{
    aiInternalUpdateDelay = delay;
}

void PlayerbotAIBase::IncreaseAIInternalUpdateDelay(uint32 delay)
{
    aiInternalUpdateDelay += delay;
}

void PlayerbotAIBase::YieldAIInternalThread(bool minimal)
{
    uint32 reactDelay = playerbot_config_react_delay();
    if (aiInternalUpdateDelay < reactDelay)
        aiInternalUpdateDelay = minimal ? reactDelay * 10 : reactDelay;
}

bool PlayerbotAIBase::IsActive() const
{
    return (int)aiInternalUpdateDelay < (int)playerbot_config_max_wait_for_move();
}

bool IsAlliance(uint8 race) {
    return race == RACE_HUMAN || race == RACE_DWARF || race == RACE_NIGHTELF
        || race == RACE_GNOME
#ifdef MANGOSBOT_ONE
        || race == RACE_DRAENEI
#endif
#ifdef MANGOSBOT_TWO
        || race == RACE_DRAENEI
#endif
        ;
}
