#pragma once
#include "DungeonTriggers.h"
#include "GenericTriggers.h"

namespace ai
{
    class AQ20EnterDungeonTrigger : public EnterDungeonTrigger
    {
    public:
        AQ20EnterDungeonTrigger(PlayerbotAI* ai) : EnterDungeonTrigger(ai, "enter aq20", "aq20", 509) {}
    };

    class AQ20LeaveDungeonTrigger : public LeaveDungeonTrigger
    {
    public:
        AQ20LeaveDungeonTrigger(PlayerbotAI* ai) : LeaveDungeonTrigger(ai, "leave aq20", "aq20", 509) {}
    };

    // Kurinnaxx (entry 15348)
    class KurinnaxxStartFightTrigger : public StartBossFightTrigger
    {
    public:
        KurinnaxxStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start kurinnaxx fight", "kurinnaxx", 15348) {}
    };
    class KurinnaxxEndFightTrigger : public EndBossFightTrigger
    {
    public:
        KurinnaxxEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end kurinnaxx fight", "kurinnaxx", 15348) {}
    };
    class KurinnaxxTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        KurinnaxxTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "kurinnaxx too close", 15348, 30.0f) {}
    };

    // General Rajaxx (entry 15341)
    class RajaxxStartFightTrigger : public StartBossFightTrigger
    {
    public:
        RajaxxStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start rajaxx fight", "rajaxx", 15341) {}
    };
    class RajaxxEndFightTrigger : public EndBossFightTrigger
    {
    public:
        RajaxxEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end rajaxx fight", "rajaxx", 15341) {}
    };
    class RajaxxTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        RajaxxTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "rajaxx too close", 15341, 30.0f) {}
    };

    // Moam (entry 15340)
    class MoamStartFightTrigger : public StartBossFightTrigger
    {
    public:
        MoamStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start moam fight", "moam", 15340) {}
    };
    class MoamEndFightTrigger : public EndBossFightTrigger
    {
    public:
        MoamEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end moam fight", "moam", 15340) {}
    };

    // Buru the Gorger (entry 15370)
    class BuruStartFightTrigger : public StartBossFightTrigger
    {
    public:
        BuruStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start buru fight", "buru", 15370) {}
    };
    class BuruEndFightTrigger : public EndBossFightTrigger
    {
    public:
        BuruEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end buru fight", "buru", 15370) {}
    };

    // Ayamiss the Hunter (entry 15375)
    class AyamissStartFightTrigger : public StartBossFightTrigger
    {
    public:
        AyamissStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start ayamiss fight", "ayamiss", 15375) {}
    };
    class AyamissEndFightTrigger : public EndBossFightTrigger
    {
    public:
        AyamissEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end ayamiss fight", "ayamiss", 15375) {}
    };
    class AyamissTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        AyamissTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "ayamiss too close", 15375, 30.0f) {}
    };

    // Ossirian the Unscarred (entry 15339)
    class OssirianStartFightTrigger : public StartBossFightTrigger
    {
    public:
        OssirianStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start ossirian fight", "ossirian", 15339) {}
    };
    class OssirianEndFightTrigger : public EndBossFightTrigger
    {
    public:
        OssirianEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end ossirian fight", "ossirian", 15339) {}
    };
    class OssirianTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        OssirianTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "ossirian too close", 15339, 30.0f) {}
    };
}
