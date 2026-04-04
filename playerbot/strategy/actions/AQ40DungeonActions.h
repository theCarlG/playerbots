#pragma once
#include "DungeonActions.h"
#include "ChangeStrategyAction.h"

namespace ai
{
    class AQ40EnableDungeonStrategyAction : public ChangeAllStrategyAction
    {
    public:
        AQ40EnableDungeonStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable aq40 strategy", "+aq40") {}
    };
    class AQ40DisableDungeonStrategyAction : public ChangeAllStrategyAction
    {
    public:
        AQ40DisableDungeonStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable aq40 strategy", "-aq40") {}
    };

    class SkeramEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        SkeramEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable skeram fight strategy", "+skeram") {}
    };
    class SkeramDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        SkeramDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable skeram fight strategy", "-skeram") {}
    };
    class SkeramMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        SkeramMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from skeram", 15263, 31.0f) {}
    };

    class BugTrioEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        BugTrioEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable bug trio fight strategy", "+bug trio") {}
    };
    class BugTrioDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        BugTrioDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable bug trio fight strategy", "-bug trio") {}
    };

    class SarturaEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        SarturaEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable sartura fight strategy", "+sartura") {}
    };
    class SarturaDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        SarturaDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable sartura fight strategy", "-sartura") {}
    };
    class SarturaMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        SarturaMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from sartura", 15516, 10.0f) {}
    };

    class FankrissEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        FankrissEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable fankriss fight strategy", "+fankriss") {}
    };
    class FankrissDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        FankrissDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable fankriss fight strategy", "-fankriss") {}
    };

    class ViscidusEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        ViscidusEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable viscidus fight strategy", "+viscidus") {}
    };
    class ViscidusDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        ViscidusDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable viscidus fight strategy", "-viscidus") {}
    };

    class HuhuranEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        HuhuranEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable huhuran fight strategy", "+huhuran") {}
    };
    class HuhuranDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        HuhuranDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable huhuran fight strategy", "-huhuran") {}
    };
    class HuhuranMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        HuhuranMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from huhuran", 15509, 31.0f) {}
    };

    class TwinEmperorsEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        TwinEmperorsEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable twin emperors fight strategy", "+twin emperors") {}
    };
    class TwinEmperorsDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        TwinEmperorsDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable twin emperors fight strategy", "-twin emperors") {}
    };

    class OuroEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        OuroEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable ouro fight strategy", "+ouro") {}
    };
    class OuroDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        OuroDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable ouro fight strategy", "-ouro") {}
    };
    class OuroMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        OuroMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from ouro", 15517, 31.0f) {}
    };

    class CThunEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        CThunEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable c'thun fight strategy", "+c'thun") {}
    };
    class CThunDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        CThunDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable c'thun fight strategy", "-c'thun") {}
    };
    class CThunMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        CThunMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from c'thun", 15727, 31.0f) {}
    };
}
