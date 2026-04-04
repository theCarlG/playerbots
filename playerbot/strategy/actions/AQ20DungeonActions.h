#pragma once
#include "DungeonActions.h"
#include "ChangeStrategyAction.h"

namespace ai
{
    class AQ20EnableDungeonStrategyAction : public ChangeAllStrategyAction
    {
    public:
        AQ20EnableDungeonStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable aq20 strategy", "+aq20") {}
    };
    class AQ20DisableDungeonStrategyAction : public ChangeAllStrategyAction
    {
    public:
        AQ20DisableDungeonStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable aq20 strategy", "-aq20") {}
    };

    class KurinnaxxEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        KurinnaxxEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable kurinnaxx fight strategy", "+kurinnaxx") {}
    };
    class KurinnaxxDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        KurinnaxxDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable kurinnaxx fight strategy", "-kurinnaxx") {}
    };
    class KurinnaxxMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        KurinnaxxMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from kurinnaxx", 15348, 31.0f) {}
    };

    class RajaxxEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        RajaxxEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable rajaxx fight strategy", "+rajaxx") {}
    };
    class RajaxxDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        RajaxxDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable rajaxx fight strategy", "-rajaxx") {}
    };
    class RajaxxMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        RajaxxMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from rajaxx", 15341, 31.0f) {}
    };

    class MoamEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        MoamEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable moam fight strategy", "+moam") {}
    };
    class MoamDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        MoamDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable moam fight strategy", "-moam") {}
    };

    class BuruEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        BuruEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable buru fight strategy", "+buru") {}
    };
    class BuruDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        BuruDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable buru fight strategy", "-buru") {}
    };

    class AyamissEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        AyamissEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable ayamiss fight strategy", "+ayamiss") {}
    };
    class AyamissDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        AyamissDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable ayamiss fight strategy", "-ayamiss") {}
    };
    class AyamissMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        AyamissMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from ayamiss", 15375, 31.0f) {}
    };

    class OssirianEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        OssirianEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable ossirian fight strategy", "+ossirian") {}
    };
    class OssirianDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        OssirianDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable ossirian fight strategy", "-ossirian") {}
    };
    class OssirianMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        OssirianMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from ossirian", 15339, 31.0f) {}
    };
}
