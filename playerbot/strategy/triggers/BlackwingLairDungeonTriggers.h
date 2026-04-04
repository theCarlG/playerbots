#pragma once
#include "DungeonTriggers.h"
#include "GenericTriggers.h"

namespace ai
{
    class BlackwingLairEnterDungeonTrigger : public EnterDungeonTrigger
    {
    public:
        BlackwingLairEnterDungeonTrigger(PlayerbotAI* ai) : EnterDungeonTrigger(ai, "enter blackwing lair", "blackwing lair", 469) {}
    };

    class BlackwingLairLeaveDungeonTrigger : public LeaveDungeonTrigger
    {
    public:
        BlackwingLairLeaveDungeonTrigger(PlayerbotAI* ai) : LeaveDungeonTrigger(ai, "leave blackwing lair", "blackwing lair", 469) {}
    };

    class SuppressionDeviceNeedStealthTrigger : public Trigger
    {
    public:
        SuppressionDeviceNeedStealthTrigger(PlayerbotAI* ai) : Trigger(ai, "suppression device need stealth", 1) {}

        bool IsActive() override
        {
            if (bot->getClass() != CLASS_ROGUE)
                return false;

            if (ai->HasAura("stealth", bot))
                return false;

            std::list<GuidPosition> gos = AI_VALUE(std::list<GuidPosition>, "go usable filter::go trapped filter::entry filter::{gos in sight,suppression devices}");
            return !gos.empty();
        }
    };

    class SuppressionDeviceInSightTrigger : public Trigger
    {
    public:
        SuppressionDeviceInSightTrigger(PlayerbotAI* ai) : Trigger(ai, "suppression device in sight", 1) {}

        bool IsActive() override
        {
            if (bot->getClass() != CLASS_ROGUE)
                return false;

            std::list<GuidPosition> gosInSight = AI_VALUE(std::list<GuidPosition>, "go usable filter::go trapped filter::entry filter::{gos in sight,suppression devices}");
            std::list<GuidPosition> gosClose = AI_VALUE(std::list<GuidPosition>, "entry filter::{gos close,suppression devices}");
            
            return !gosInSight.empty() && gosClose.empty();
        }
    };

    class SuppressionDeviceCloseTrigger : public Trigger
    {
    public:
        SuppressionDeviceCloseTrigger(PlayerbotAI* ai) : Trigger(ai, "suppression device close", 1) {}

        bool IsActive() override
        {
            if (bot->getClass() != CLASS_ROGUE)
                return false;

            std::list<GuidPosition> gos = AI_VALUE(std::list<GuidPosition>, "go usable filter::go trapped filter::entry filter::{gos close,suppression devices}");
            return !gos.empty();
        }
    };

    // Razorgore the Untamed (entry 12435)
    class RazorgoreStartFightTrigger : public StartBossFightTrigger
    {
    public:
        RazorgoreStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start razorgore fight", "razorgore", 12435) {}
    };
    class RazorgoreEndFightTrigger : public EndBossFightTrigger
    {
    public:
        RazorgoreEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end razorgore fight", "razorgore", 12435) {}
    };

    // Vaelastrasz the Corrupt (entry 13020)
    class VaelastraszStartFightTrigger : public StartBossFightTrigger
    {
    public:
        VaelastraszStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start vaelastrasz fight", "vaelastrasz", 13020) {}
    };
    class VaelastraszEndFightTrigger : public EndBossFightTrigger
    {
    public:
        VaelastraszEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end vaelastrasz fight", "vaelastrasz", 13020) {}
    };
    // Burning Adrenaline: random player must run away before it explodes
    class HasBurningAdrenalineTrigger : public Trigger
    {
    public:
        HasBurningAdrenalineTrigger(PlayerbotAI* ai) : Trigger(ai, "has burning adrenaline", 1) {}
        bool IsActive() override { return ai->HasAura("burning adrenaline", bot); }
    };

    // Broodlord Lashlayer (entry 12017)
    class BroodlordStartFightTrigger : public StartBossFightTrigger
    {
    public:
        BroodlordStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start broodlord fight", "broodlord", 12017) {}
    };
    class BroodlordEndFightTrigger : public EndBossFightTrigger
    {
    public:
        BroodlordEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end broodlord fight", "broodlord", 12017) {}
    };

    // Firemaw (entry 11983)
    class FiremawStartFightTrigger : public StartBossFightTrigger
    {
    public:
        FiremawStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start firemaw fight", "firemaw", 11983) {}
    };
    class FiremawEndFightTrigger : public EndBossFightTrigger
    {
    public:
        FiremawEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end firemaw fight", "firemaw", 11983) {}
    };
    class FiremawTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        FiremawTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "firemaw too close", 11983, 30.0f) {}
    };

    // Ebonroc (entry 14601)
    class EbonrocStartFightTrigger : public StartBossFightTrigger
    {
    public:
        EbonrocStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start ebonroc fight", "ebonroc", 14601) {}
    };
    class EbonrocEndFightTrigger : public EndBossFightTrigger
    {
    public:
        EbonrocEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end ebonroc fight", "ebonroc", 14601) {}
    };
    class EbonrocTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        EbonrocTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "ebonroc too close", 14601, 30.0f) {}
    };

    // Flamegor (entry 11981)
    class FlamegorStartFightTrigger : public StartBossFightTrigger
    {
    public:
        FlamegorStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start flamegor fight", "flamegor", 11981) {}
    };
    class FlamegorEndFightTrigger : public EndBossFightTrigger
    {
    public:
        FlamegorEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end flamegor fight", "flamegor", 11981) {}
    };
    class FlamegorTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        FlamegorTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "flamegor too close", 11981, 30.0f) {}
    };

    // Chromaggus (entry 14020)
    class ChromaggusStartFightTrigger : public StartBossFightTrigger
    {
    public:
        ChromaggusStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start chromaggus fight", "chromaggus", 14020) {}
    };
    class ChromaggusEndFightTrigger : public EndBossFightTrigger
    {
    public:
        ChromaggusEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end chromaggus fight", "chromaggus", 14020) {}
    };
    class ChromaggusTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        ChromaggusTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "chromaggus too close", 14020, 30.0f) {}
    };

    // Nefarian (entry 11583)
    class NefarianStartFightTrigger : public StartBossFightTrigger
    {
    public:
        NefarianStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start nefarian fight", "nefarian", 11583) {}
    };
    class NefarianEndFightTrigger : public EndBossFightTrigger
    {
    public:
        NefarianEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end nefarian fight", "nefarian", 11583) {}
    };
    class NefarianTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        NefarianTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "nefarian too close", 11583, 30.0f) {}
    };
}