#pragma once
#include "DungeonActions.h"
#include "ChangeStrategyAction.h"
#include "UseItemAction.h"

namespace ai
{
    class MoltenCoreEnableDungeonStrategyAction : public ChangeAllStrategyAction
    {
    public:
        MoltenCoreEnableDungeonStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable molten core strategy", "+molten core") {}
    };

    class MoltenCoreDisableDungeonStrategyAction : public ChangeAllStrategyAction
    {
    public:
        MoltenCoreDisableDungeonStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable molten core strategy", "-molten core") {}
    };

    class MagmadarEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        MagmadarEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable magmadar fight strategy", "+magmadar") {}
    };

    class MagmadarDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        MagmadarDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable magmadar fight strategy", "-magmadar") {}
    };

    class MagmadarMoveAwayFromLavaBombAction : public MoveAwayFromHazard
    {
    public:
        MagmadarMoveAwayFromLavaBombAction(PlayerbotAI* ai) : MoveAwayFromHazard(ai, "move away from magmadar lava bomb") {}
    };

    class MagmadarMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        MagmadarMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from magmadar", 11982, 31.0f) {}
    };

    class MoveToMCRuneAction : public MoveToAction
    {
    public:
        MoveToMCRuneAction(PlayerbotAI* ai) : MoveToAction(ai, "move to mc rune") { qualifier = "entry filter::{gos in sight,mc runes}"; }
    };

    class DouseMCRuneActionAqual : public UseItemIdAction
    {
    public:
        DouseMCRuneActionAqual(PlayerbotAI* ai) : UseItemIdAction(ai, "douse mc rune aqual") { qualifier = "{17333,entry filter::{gos close,mc runes}}"; }
    };

    class DouseMCRuneActionEternal : public UseItemIdAction
    {
    public:
        DouseMCRuneActionEternal(PlayerbotAI* ai) : UseItemIdAction(ai, "douse mc rune eternal") { qualifier = "{22754,entry filter::{gos close,mc runes}}"; }
    };

    class BaronGeddonEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        BaronGeddonEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable baron geddon fight strategy", "+baron geddon") {}
    };

    class BaronGeddonDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        BaronGeddonDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable baron geddon fight strategy", "-baron geddon") {}
    };

    class BaronGeddonRunAwayAction : public RunAwayFromGroupAction
    {
    public:
        BaronGeddonRunAwayAction(PlayerbotAI* ai) : RunAwayFromGroupAction(ai, "baron geddon run away") {}
    };

    // Flamewaker Imps (entry 11666): melee DPS back away so ranged handles them
    class FlamewakeriampsMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        FlamewakeriampsMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from flamewaker imps", 11666, 16.0f) {}
    };

    // Stone Elemental / Lava Surger (entry 12076): all bots stack in melee to prevent Surge
    class MoveCloseToStoneElementalAction : public MoveCloseToCreature
    {
    public:
        MoveCloseToStoneElementalAction(PlayerbotAI* ai) : MoveCloseToCreature(ai, "move close to stone elemental", 12076, 4.0f) {}
    };

    // Core Hound (entry 11671): tank drags it away from group to avoid cleaving the raid
    class TankRunCoreHoundFromGroupAction : public RunAwayFromGroupAction
    {
    public:
        TankRunCoreHoundFromGroupAction(PlayerbotAI* ai) : RunAwayFromGroupAction(ai, "tank run core hound from group") {}
    };

    // Baron Geddon Inferno: all bots flee while Inferno channels
    class BaronGeddonInfernoFleeAction : public MoveAwayFromCreature
    {
    public:
        BaronGeddonInfernoFleeAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "baron geddon inferno flee", 12056, 15.0f) {}
    };

    // Lucifron
    class LucifronEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        LucifronEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable lucifron fight strategy", "+lucifron") {}
    };
    class LucifronDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        LucifronDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable lucifron fight strategy", "-lucifron") {}
    };

    // Gehennas
    class GehennasEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        GehennasEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable gehennas fight strategy", "+gehennas") {}
    };
    class GehennasDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        GehennasDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable gehennas fight strategy", "-gehennas") {}
    };
    class GehennasMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        GehennasMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from gehennas", 12259, 31.0f) {}
    };

    // Garr
    class GarrEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        GarrEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable garr fight strategy", "+garr") {}
    };
    class GarrDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        GarrDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable garr fight strategy", "-garr") {}
    };

    // Shazzrah
    class ShazzrahEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        ShazzrahEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable shazzrah fight strategy", "+shazzrah") {}
    };
    class ShazzrahDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        ShazzrahDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable shazzrah fight strategy", "-shazzrah") {}
    };
    class ShazzrahMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        ShazzrahMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from shazzrah", 12264, 16.0f) {}
    };

    // Sulfuron Harbinger
    class SulfuronEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        SulfuronEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable sulfuron fight strategy", "+sulfuron") {}
    };
    class SulfuronDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        SulfuronDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable sulfuron fight strategy", "-sulfuron") {}
    };

    // Golemagg
    class GolemaggEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        GolemaggEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable golemagg fight strategy", "+golemagg") {}
    };
    class GolemaggDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        GolemaggDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable golemagg fight strategy", "-golemagg") {}
    };

    // Majordomo Executus
    class MajordomoEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        MajordomoEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable majordomo fight strategy", "+majordomo") {}
    };
    class MajordomoDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        MajordomoDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable majordomo fight strategy", "-majordomo") {}
    };

    // Ragnaros
    class RagnarosEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        RagnarosEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable ragnaros fight strategy", "+ragnaros") {}
    };
    class RagnarosDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        RagnarosDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable ragnaros fight strategy", "-ragnaros") {}
    };
    class RagnarosMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        RagnarosMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from ragnaros", 11502, 31.0f) {}
    };
}