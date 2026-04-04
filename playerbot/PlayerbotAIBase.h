#pragma once

#include "Common.h"

class Player;
class PlayerbotMgr;
class ChatHandler;

/// Base class for bot AI.  CMaNGOS calls UpdateAI() on the map-worker thread.
class PlayerbotAIBase
{
public:
    PlayerbotAIBase();
    virtual ~PlayerbotAIBase() = default;

    bool IsActive() const;
    virtual void UpdateAI(uint32 elapsed);

    uint32 GetAIInternalUpdateDelay() const { return aiInternalUpdateDelay; }

protected:
    virtual void UpdateAIInternal(uint32 elapsed, bool minimal = false);
    bool CanUpdateAIInternal() const { return aiInternalUpdateDelay < 100U; }
    void SetAIInternalUpdateDelay(uint32 delay);
    void ResetAIInternalUpdateDelay() { aiInternalUpdateDelay = 0U; }
    void IncreaseAIInternalUpdateDelay(uint32 delay);
    void YieldAIInternalThread(bool minimal = false);

protected:
    uint32 aiInternalUpdateDelay;
};
