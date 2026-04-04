#pragma once
#include "DungeonTriggers.h"
#include "GenericTriggers.h"

namespace ai
{
    class AQ40EnterDungeonTrigger : public EnterDungeonTrigger
    {
    public:
        AQ40EnterDungeonTrigger(PlayerbotAI* ai) : EnterDungeonTrigger(ai, "enter aq40", "aq40", 531) {}
    };

    class AQ40LeaveDungeonTrigger : public LeaveDungeonTrigger
    {
    public:
        AQ40LeaveDungeonTrigger(PlayerbotAI* ai) : LeaveDungeonTrigger(ai, "leave aq40", "aq40", 531) {}
    };

    // The Prophet Skeram (entry 15263)
    class SkeramStartFightTrigger : public StartBossFightTrigger
    {
    public:
        SkeramStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start skeram fight", "skeram", 15263) {}
    };
    class SkeramEndFightTrigger : public EndBossFightTrigger
    {
    public:
        SkeramEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end skeram fight", "skeram", 15263) {}
    };
    class SkeramTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        SkeramTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "skeram too close", 15263, 30.0f) {}
    };

    // Bug Trio — Vem (15544) used as group trigger
    class BugTrioStartFightTrigger : public StartBossFightTrigger
    {
    public:
        BugTrioStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start bug trio fight", "bug trio", 15544) {}
    };
    class BugTrioEndFightTrigger : public EndBossFightTrigger
    {
    public:
        BugTrioEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end bug trio fight", "bug trio", 15544) {}
    };

    // Battleguard Sartura (entry 15516)
    class SarturaStartFightTrigger : public StartBossFightTrigger
    {
    public:
        SarturaStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start sartura fight", "sartura", 15516) {}
    };
    class SarturaEndFightTrigger : public EndBossFightTrigger
    {
    public:
        SarturaEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end sartura fight", "sartura", 15516) {}
    };
    // Whirlwind — all bots maintain distance
    class SarturaTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        SarturaTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "sartura too close", 15516, 8.0f) {}
    };

    // Fankriss the Unyielding (entry 15510)
    class FankrissStartFightTrigger : public StartBossFightTrigger
    {
    public:
        FankrissStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start fankriss fight", "fankriss", 15510) {}
    };
    class FankrissEndFightTrigger : public EndBossFightTrigger
    {
    public:
        FankrissEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end fankriss fight", "fankriss", 15510) {}
    };

    // Viscidus (entry 15299)
    class ViscidusStartFightTrigger : public StartBossFightTrigger
    {
    public:
        ViscidusStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start viscidus fight", "viscidus", 15299) {}
    };
    class ViscidusEndFightTrigger : public EndBossFightTrigger
    {
    public:
        ViscidusEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end viscidus fight", "viscidus", 15299) {}
    };

    // Princess Huhuran (entry 15509)
    class HuhuranStartFightTrigger : public StartBossFightTrigger
    {
    public:
        HuhuranStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start huhuran fight", "huhuran", 15509) {}
    };
    class HuhuranEndFightTrigger : public EndBossFightTrigger
    {
    public:
        HuhuranEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end huhuran fight", "huhuran", 15509) {}
    };
    class HuhuranTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        HuhuranTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "huhuran too close", 15509, 30.0f) {}
    };

    // Twin Emperors — Vek'nilash (entry 15275) used as group trigger
    class TwinEmperorsStartFightTrigger : public StartBossFightTrigger
    {
    public:
        TwinEmperorsStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start twin emperors fight", "twin emperors", 15275) {}
    };
    class TwinEmperorsEndFightTrigger : public EndBossFightTrigger
    {
    public:
        TwinEmperorsEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end twin emperors fight", "twin emperors", 15275) {}
    };

    // Ouro (entry 15517)
    class OuroStartFightTrigger : public StartBossFightTrigger
    {
    public:
        OuroStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start ouro fight", "ouro", 15517) {}
    };
    class OuroEndFightTrigger : public EndBossFightTrigger
    {
    public:
        OuroEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end ouro fight", "ouro", 15517) {}
    };
    class OuroTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        OuroTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "ouro too close", 15517, 30.0f) {}
    };

    // C'Thun (entry 15727)
    class CThunStartFightTrigger : public StartBossFightTrigger
    {
    public:
        CThunStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start c'thun fight", "c'thun", 15727) {}
    };
    class CThunEndFightTrigger : public EndBossFightTrigger
    {
    public:
        CThunEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end c'thun fight", "c'thun", 15727) {}
    };
    class CThunTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        CThunTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "c'thun too close", 15727, 30.0f) {}
    };
}
