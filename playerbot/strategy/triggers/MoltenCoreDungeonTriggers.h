#pragma once
#include "DungeonTriggers.h"
#include "GenericTriggers.h"

namespace ai
{
    class MoltenCoreEnterDungeonTrigger : public EnterDungeonTrigger
    {
    public:
        MoltenCoreEnterDungeonTrigger(PlayerbotAI* ai) : EnterDungeonTrigger(ai, "enter molten core", "molten core", 409) {}
    };

    class MoltenCoreLeaveDungeonTrigger : public LeaveDungeonTrigger
    {
    public:
        MoltenCoreLeaveDungeonTrigger(PlayerbotAI* ai) : LeaveDungeonTrigger(ai, "leave molten core", "molten core", 409) {}
    };

    class MagmadarStartFightTrigger : public StartBossFightTrigger
    {
    public:
        MagmadarStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start magmadar fight", "magmadar", 11982) {}
    };

    class MagmadarEndFightTrigger : public EndBossFightTrigger
    {
    public:
        MagmadarEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end magmadar fight", "magmadar", 11982) {}
    };

    class MagmadarLavaBombTrigger : public CloseToGameObjectHazardTrigger
    {
    public:
        MagmadarLavaBombTrigger(PlayerbotAI* ai) : CloseToGameObjectHazardTrigger(ai, "magmadar lava bomb", 177704, 5.0f, 60) {}
    };

    class MagmadarTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        MagmadarTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "magmadar too close", 11982, 30.0f) {}
    };

    class FireProtectionPotionReadyTrigger : public ItemBuffReadyTrigger
    {
    public:
        FireProtectionPotionReadyTrigger(PlayerbotAI* ai) : ItemBuffReadyTrigger(ai, "fire protection potion ready", 13457, 17543) {}
    };

    class MCRuneInSightTrigger : public ValueTrigger
    {
    public:
        MCRuneInSightTrigger(PlayerbotAI* ai) : ValueTrigger(ai, "mc rune in sight", 1)
        {
            qualifier = "and::{"
                "or::{action possible::use id::17333,action possible::use id::22754},"
                "has object::go usable filter::go trapped filter::entry filter::{gos in sight,mc runes},"
                "not::has object::entry filter::{gos close,mc runes}"
                "}";
        }
    };

    class MCRuneCloseTrigger : public ValueTrigger
    {
    public:
        MCRuneCloseTrigger(PlayerbotAI* ai) : ValueTrigger(ai, "mc rune close", 1) { qualifier = "has object::go usable filter::entry filter::{gos close,mc runes}"; }
    };

    class BaronGeddonStartFightTrigger : public StartBossFightTrigger
    {
    public:
        BaronGeddonStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start baron geddon fight", "baron geddon", 12056) {}
    };

    class BaronGeddonEndFightTrigger : public EndBossFightTrigger
    {
    public:
        BaronGeddonEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end baron geddon fight", "baron geddon", 12056) {}
    };

    // Fires when this bot has Baron Geddon's Living Bomb debuff (must run away from group).
    // Uses a distinct name to avoid conflict with the Mage class's own "living bomb" trigger.
    class HasLivingBombDebuffTrigger : public Trigger
    {
    public:
        HasLivingBombDebuffTrigger(PlayerbotAI* ai) : Trigger(ai, "has living bomb debuff", 1) {}
        bool IsActive() override { return ai->HasAura("living bomb", bot); }
    };

    // Lucifron (entry 12118)
    class LucifronStartFightTrigger : public StartBossFightTrigger
    {
    public:
        LucifronStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start lucifron fight", "lucifron", 12118) {}
    };
    class LucifronEndFightTrigger : public EndBossFightTrigger
    {
    public:
        LucifronEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end lucifron fight", "lucifron", 12118) {}
    };

    // Gehennas (entry 12259)
    class GehennasStartFightTrigger : public StartBossFightTrigger
    {
    public:
        GehennasStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start gehennas fight", "gehennas", 12259) {}
    };
    class GehennasEndFightTrigger : public EndBossFightTrigger
    {
    public:
        GehennasEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end gehennas fight", "gehennas", 12259) {}
    };
    // Gehennas: ranged/healers maintain distance
    class GehennasTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        GehennasTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "gehennas too close", 12259, 30.0f) {}
    };

    // Garr (entry 12057) + Firesworn adds (entry 12099)
    class GarrStartFightTrigger : public StartBossFightTrigger
    {
    public:
        GarrStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start garr fight", "garr", 12057) {}
    };
    class GarrEndFightTrigger : public EndBossFightTrigger
    {
    public:
        GarrEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end garr fight", "garr", 12057) {}
    };
    // Firesworn explode on death — avoid packs within 10 yards
    class FireswornHazardTrigger : public CloseToCreatureHazardTrigger
    {
    public:
        FireswornHazardTrigger(PlayerbotAI* ai) : CloseToCreatureHazardTrigger(ai, "firesworn hazard", 12099, 10.0f, 30) {}
    };

    // Shazzrah (entry 12264) — Arcane Explosion hits all nearby; all bots stay at distance
    class ShazzrahStartFightTrigger : public StartBossFightTrigger
    {
    public:
        ShazzrahStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start shazzrah fight", "shazzrah", 12264) {}
    };
    class ShazzrahEndFightTrigger : public EndBossFightTrigger
    {
    public:
        ShazzrahEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end shazzrah fight", "shazzrah", 12264) {}
    };
    class ShazzrahTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        ShazzrahTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "shazzrah too close", 12264, 15.0f) {}
    };

    // Sulfuron Harbinger (entry 12098)
    class SulfuronStartFightTrigger : public StartBossFightTrigger
    {
    public:
        SulfuronStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start sulfuron fight", "sulfuron", 12098) {}
    };
    class SulfuronEndFightTrigger : public EndBossFightTrigger
    {
    public:
        SulfuronEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end sulfuron fight", "sulfuron", 12098) {}
    };

    // Golemagg the Incinerator (entry 11988)
    class GolemaggStartFightTrigger : public StartBossFightTrigger
    {
    public:
        GolemaggStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start golemagg fight", "golemagg", 11988) {}
    };
    class GolemaggEndFightTrigger : public EndBossFightTrigger
    {
    public:
        GolemaggEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end golemagg fight", "golemagg", 11988) {}
    };

    // Majordomo Executus (entry 12018)
    class MajordomoStartFightTrigger : public StartBossFightTrigger
    {
    public:
        MajordomoStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start majordomo fight", "majordomo", 12018) {}
    };
    class MajordomoEndFightTrigger : public EndBossFightTrigger
    {
    public:
        MajordomoEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end majordomo fight", "majordomo", 12018) {}
    };

    // Ragnaros (entry 11502)
    class RagnarosStartFightTrigger : public StartBossFightTrigger
    {
    public:
        RagnarosStartFightTrigger(PlayerbotAI* ai) : StartBossFightTrigger(ai, "start ragnaros fight", "ragnaros", 11502) {}
    };
    class RagnarosEndFightTrigger : public EndBossFightTrigger
    {
    public:
        RagnarosEndFightTrigger(PlayerbotAI* ai) : EndBossFightTrigger(ai, "end ragnaros fight", "ragnaros", 11502) {}
    };
    // Ragnaros: ranged/healers maintain 30+ yards (he hits hard in melee)
    class RagnarosTooCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        RagnarosTooCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "ragnaros too close", 11502, 30.0f) {}
    };
    // Sons of Flame (entry 12099) spawn during submerge — avoid them
    class SonsOfFlameHazardTrigger : public CloseToCreatureHazardTrigger
    {
    public:
        SonsOfFlameHazardTrigger(PlayerbotAI* ai) : CloseToCreatureHazardTrigger(ai, "sons of flame hazard", 12100, 8.0f, 30) {}
    };

    // Flamewaker Imps (entry 11666): fast trash before Lucifron; melee DPS back off so ranged handles them
    class FlamewakerimpsCloseTrigger : public CloseToCreatureTrigger
    {
    public:
        FlamewakerimpsCloseTrigger(PlayerbotAI* ai) : CloseToCreatureTrigger(ai, "flamewaker imps too close", 11666, 15.0f) {}
    };

    // Stone Elemental / Lava Surger trash (entry 12076): Surge charges random distant targets
    // Everyone stacks in melee range to remove valid distant targets
    class StoneElementalTooFarTrigger : public TooFarFromCreatureTrigger
    {
    public:
        StoneElementalTooFarTrigger(PlayerbotAI* ai) : TooFarFromCreatureTrigger(ai, "stone elemental too far", 12076, 6.0f) {}
    };

    // Core Hound (entry 11671): directional Flame Breath — tank drags it away from raid
    // Fires when bot is a tank, current target is a Core Hound, and group members are close
    class TankFightingCoreHoundNearGroupTrigger : public Trigger
    {
    public:
        TankFightingCoreHoundNearGroupTrigger(PlayerbotAI* ai) : Trigger(ai, "tank fighting core hound near group", 2) {}
        bool IsActive() override;
    };

    // Baron Geddon Inferno: channeled AoE ring ~10 yards — all bots must flee
    // Fires when Baron Geddon (entry 12056) nearby has the Inferno aura active
    class BaronGeddonInfernoActiveTrigger : public Trigger
    {
    public:
        BaronGeddonInfernoActiveTrigger(PlayerbotAI* ai) : Trigger(ai, "baron geddon inferno active", 1) {}
        bool IsActive() override;
    };
}