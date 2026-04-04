#pragma once
#include "DungeonActions.h"
#include "ChangeStrategyAction.h"
#include "UseItemAction.h"

namespace ai
{
    class NaxxramasEnableDungeonStrategyAction : public ChangeAllStrategyAction
    {
    public:
        NaxxramasEnableDungeonStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable naxxramas strategy", "+naxxramas") {}
    };

    class NaxxramasDisableDungeonStrategyAction : public ChangeAllStrategyAction
    {
    public:
        NaxxramasDisableDungeonStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable naxxramas strategy", "-naxxramas") {}
    };

    // Spider Wing
    class AnubRekhanEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        AnubRekhanEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable anub'rekhan fight strategy", "+anub'rekhan") {}
    };
    class AnubRekhanDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        AnubRekhanDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable anub'rekhan fight strategy", "-anub'rekhan") {}
    };

    class FaerlinaEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        FaerlinaEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable faerlina fight strategy", "+faerlina") {}
    };
    class FaerlinaDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        FaerlinaDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable faerlina fight strategy", "-faerlina") {}
    };

    class MaexxnaEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        MaexxnaEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable maexxna fight strategy", "+maexxna") {}
    };
    class MaexxnaDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        MaexxnaDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable maexxna fight strategy", "-maexxna") {}
    };
    class MaexxnaMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        MaexxnaMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from maexxna", 15952, 31.0f) {}
    };

    // Plague Wing
    class NothEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        NothEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable noth fight strategy", "+noth") {}
    };
    class NothDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        NothDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable noth fight strategy", "-noth") {}
    };
    class NothMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        NothMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from noth", 15954, 31.0f) {}
    };

    class HeiganEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        HeiganEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable heigan fight strategy", "+heigan") {}
    };
    class HeiganDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        HeiganDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable heigan fight strategy", "-heigan") {}
    };

    class LoathebEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        LoathebEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable loatheb fight strategy", "+loatheb") {}
    };
    class LoathebDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        LoathebDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable loatheb fight strategy", "-loatheb") {}
    };

    // Military Quarter
    class RazuviousEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        RazuviousEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable razuvious fight strategy", "+razuvious") {}
    };
    class RazuviousDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        RazuviousDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable razuvious fight strategy", "-razuvious") {}
    };

    class GothikEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        GothikEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable gothik fight strategy", "+gothik") {}
    };
    class GothikDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        GothikDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable gothik fight strategy", "-gothik") {}
    };

    class FourHorsemanEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        FourHorsemanEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable four horseman fight strategy", "+four horseman") {}
    };
    class FourHorsemanDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        FourHorsemanDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable four horseman fight strategy", "-four horseman") {}
    };

    // Construct Quarter
    class PatchwerkEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        PatchwerkEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable patchwerk fight strategy", "+patchwerk") {}
    };
    class PatchwerkDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        PatchwerkDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable patchwerk fight strategy", "-patchwerk") {}
    };

    class GrobbullusEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        GrobbullusEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable grobbulus fight strategy", "+grobbulus") {}
    };
    class GrobbullusDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        GrobbullusDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable grobbulus fight strategy", "-grobbulus") {}
    };
    class GrobbullusRunAwayAction : public RunAwayFromGroupAction
    {
    public:
        GrobbullusRunAwayAction(PlayerbotAI* ai) : RunAwayFromGroupAction(ai, "grobbulus run away") {}
    };

    class GluthEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        GluthEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable gluth fight strategy", "+gluth") {}
    };
    class GluthDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        GluthDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable gluth fight strategy", "-gluth") {}
    };
    class GluthMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        GluthMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from gluth", 16029, 31.0f) {}
    };

    class ThaddiusEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        ThaddiusEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable thaddius fight strategy", "+thaddius") {}
    };
    class ThaddiusDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        ThaddiusDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable thaddius fight strategy", "-thaddius") {}
    };

    // Frostwyrm Lair
    class SapphironEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        SapphironEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable sapphiron fight strategy", "+sapphiron") {}
    };
    class SapphironDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        SapphironDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable sapphiron fight strategy", "-sapphiron") {}
    };
    class SapphironMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        SapphironMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from sapphiron", 15989, 31.0f) {}
    };

    class KelThuzadEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        KelThuzadEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable kel'thuzad fight strategy", "+kel'thuzad") {}
    };
    class KelThuzadDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        KelThuzadDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable kel'thuzad fight strategy", "-kel'thuzad") {}
    };
    class KelThuzadMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        KelThuzadMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from kel'thuzad", 15990, 31.0f) {}
    };
}