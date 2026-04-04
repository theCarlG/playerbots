#pragma once
#include "DungeonTriggers.h"
#include "GenericTriggers.h"

namespace ai
{
    class NaxxramasEnterDungeonTrigger : public EnterDungeonTrigger
    {
    public:
        NaxxramasEnterDungeonTrigger(PlayerbotAI* ai) : EnterDungeonTrigger(ai, "enter naxxramas", "naxxramas", 533) {}
    };

    class NaxxramasLeaveDungeonTrigger : public LeaveDungeonTrigger
    {
    public:
        NaxxramasLeaveDungeonTrigger(PlayerbotAI* ai) : LeaveDungeonTrigger(ai, "leave naxxramas", "naxxramas", 533) {}
    };

    // Spider Wing
    // Anub'Rekhan (entry 15956)
    class AnubRekhanStartFightTrigger : public StartBossFightTrigger
    {
    public:
        AnubRekhanStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start anub'rekhan fight", "anub'rekhan", 15956) {}
    };
    class AnubRekhanEndFightTrigger : public EndBossFightTrigger
    {
    public:
        AnubRekhanEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end anub'rekhan fight", "anub'rekhan", 15956) {}
    };

    // Grand Widow Faerlina (entry 15953)
    class FaerlinaStartFightTrigger : public StartBossFightTrigger
    {
    public:
        FaerlinaStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start faerlina fight", "faerlina", 15953) {}
    };
    class FaerlinaEndFightTrigger : public EndBossFightTrigger
    {
    public:
        FaerlinaEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end faerlina fight", "faerlina", 15953) {}
    };

    // Maexxna (entry 15952)
    class MaexxnaStartFightTrigger : public StartBossFightTrigger
    {
    public:
        MaexxnaStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start maexxna fight", "maexxna", 15952) {}
    };
    class MaexxnaEndFightTrigger : public EndBossFightTrigger
    {
    public:
        MaexxnaEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end maexxna fight", "maexxna", 15952) {}
    };
    class MaexxnaTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        MaexxnaTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "maexxna too close", 15952, 30.0f) {}
    };

    // Plague Wing
    // Noth the Plaguebringer (entry 15954)
    class NothStartFightTrigger : public StartBossFightTrigger
    {
    public:
        NothStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start noth fight", "noth", 15954) {}
    };
    class NothEndFightTrigger : public EndBossFightTrigger
    {
    public:
        NothEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end noth fight", "noth", 15954) {}
    };
    class NothTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        NothTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "noth too close", 15954, 30.0f) {}
    };

    // Heigan the Unclean (entry 15936)
    class HeiganStartFightTrigger : public StartBossFightTrigger
    {
    public:
        HeiganStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start heigan fight", "heigan", 15936) {}
    };
    class HeiganEndFightTrigger : public EndBossFightTrigger
    {
    public:
        HeiganEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end heigan fight", "heigan", 15936) {}
    };

    // Loatheb (entry 16011)
    class LoathebStartFightTrigger : public StartBossFightTrigger
    {
    public:
        LoathebStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start loatheb fight", "loatheb", 16011) {}
    };
    class LoathebEndFightTrigger : public EndBossFightTrigger
    {
    public:
        LoathebEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end loatheb fight", "loatheb", 16011) {}
    };

    // Military Quarter
    // Instructor Razuvious (entry 16061)
    class RazuviousStartFightTrigger : public StartBossFightTrigger
    {
    public:
        RazuviousStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start razuvious fight", "razuvious", 16061) {}
    };
    class RazuviousEndFightTrigger : public EndBossFightTrigger
    {
    public:
        RazuviousEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end razuvious fight", "razuvious", 16061) {}
    };

    // Gothik the Harvester (entry 16060)
    class GothikStartFightTrigger : public StartBossFightTrigger
    {
    public:
        GothikStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start gothik fight", "gothik", 16060) {}
    };
    class GothikEndFightTrigger : public EndBossFightTrigger
    {
    public:
        GothikEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end gothik fight", "gothik", 16060) {}
    };

    // The Four Horsemen (Lady Blaumeux entry 16062 used as group trigger)
    class FourHorsemanStartFightTrigger : public StartBossFightTrigger
    {
    public:
        FourHorsemanStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start four horseman fight", "four horseman", 16062) {}
    };
    class FourHorsemanEndFightTrigger : public EndBossFightTrigger
    {
    public:
        FourHorsemanEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end four horseman fight", "four horseman", 16062) {}
    };

    // Construct Quarter
    // Patchwerk (entry 16028)
    class PatchwerkStartFightTrigger : public StartBossFightTrigger
    {
    public:
        PatchwerkStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start patchwerk fight", "patchwerk", 16028) {}
    };
    class PatchwerkEndFightTrigger : public EndBossFightTrigger
    {
    public:
        PatchwerkEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end patchwerk fight", "patchwerk", 16028) {}
    };

    // Grobbulus (entry 15931) — Mutating Injection: affected bot runs away from group
    class GrobbullusStartFightTrigger : public StartBossFightTrigger
    {
    public:
        GrobbullusStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start grobbulus fight", "grobbulus", 15931) {}
    };
    class GrobbullusEndFightTrigger : public EndBossFightTrigger
    {
    public:
        GrobbullusEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end grobbulus fight", "grobbulus", 15931) {}
    };
    class HasMutatingInjectionTrigger : public Trigger
    {
    public:
        HasMutatingInjectionTrigger(PlayerbotAI* ai) : Trigger(ai, "has mutating injection", 1) {}
        bool IsActive() override { return ai->HasAura("mutating injection", bot); }
    };

    // Gluth (entry 16029)
    class GluthStartFightTrigger : public StartBossFightTrigger
    {
    public:
        GluthStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start gluth fight", "gluth", 16029) {}
    };
    class GluthEndFightTrigger : public EndBossFightTrigger
    {
    public:
        GluthEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end gluth fight", "gluth", 16029) {}
    };
    class GluthTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        GluthTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "gluth too close", 16029, 30.0f) {}
    };

    // Thaddius (entry 15928)
    class ThaddiusStartFightTrigger : public StartBossFightTrigger
    {
    public:
        ThaddiusStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start thaddius fight", "thaddius", 15928) {}
    };
    class ThaddiusEndFightTrigger : public EndBossFightTrigger
    {
    public:
        ThaddiusEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end thaddius fight", "thaddius", 15928) {}
    };

    // Frostwyrm Lair
    // Sapphiron (entry 15989)
    class SapphironStartFightTrigger : public StartBossFightTrigger
    {
    public:
        SapphironStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start sapphiron fight", "sapphiron", 15989) {}
    };
    class SapphironEndFightTrigger : public EndBossFightTrigger
    {
    public:
        SapphironEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end sapphiron fight", "sapphiron", 15989) {}
    };
    class SapphironTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        SapphironTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "sapphiron too close", 15989, 30.0f) {}
    };

    // Kel'Thuzad (entry 15990)
    class KelThuzadStartFightTrigger : public StartBossFightTrigger
    {
    public:
        KelThuzadStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start kel'thuzad fight", "kel'thuzad", 15990) {}
    };
    class KelThuzadEndFightTrigger : public EndBossFightTrigger
    {
    public:
        KelThuzadEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end kel'thuzad fight", "kel'thuzad", 15990) {}
    };
    class KelThuzadTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        KelThuzadTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "kel'thuzad too close", 15990, 30.0f) {}
    };
}