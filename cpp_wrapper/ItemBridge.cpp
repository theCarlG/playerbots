/**
 * ItemBridge.cpp — CMaNGOS-side implementation of `ItemCallbacks`.
 *
 * See the header for the division of labour: the Rust `itempool` module
 * owns every cache that used to live in `playerbot/RandomItemMgr.cpp`.
 * This file is the narrow surface where the Rust side still needs to
 * reach CMaNGOS — to dump tables during init, do a few per-player
 * queries at runtime, and get an `urand` / debug-log sink.
 *
 * Allocation contract: every `query_*` here returns a
 * `std::malloc`'d array copied from an internal `std::vector`; the Rust
 * side hands it back via the matching `free_*` callback and we release
 * it with `std::free`.
 */

#include "botpch.h"
#include "ItemBridge.h"

#include <cstdlib>
#include <cstring>
#include <vector>

#include "Database/DBCStore.h"
#include "Database/DatabaseEnv.h"
#include "Entities/Creature.h"
#include "Entities/ItemPrototype.h"
#include "Entities/ObjectGuid.h"
#include "Entities/Player.h"
#include "Globals/ObjectMgr.h"
#include "Log/Log.h"
#include "Quests/QuestDef.h"
#include "Server/DBCStores.h"
#include "Server/DBCStructure.h"
#include "Server/SQLStorages.h"
#include "Util/Util.h"

namespace
{
// ── Small helpers ────────────────────────────────────────────────────────

/// Copy a null-terminated C string into a fixed-size buffer, always
/// leaving a terminator. `dst_size` is the total size of the target
/// buffer including the terminator byte.
void CopyCString(char* dst, size_t dst_size, const char* src)
{
    if (!dst || dst_size == 0)
        return;
    if (!src)
    {
        dst[0] = '\0';
        return;
    }
    const size_t n = std::strlen(src);
    const size_t copy = n < dst_size - 1 ? n : dst_size - 1;
    std::memcpy(dst, src, copy);
    dst[copy] = '\0';
}

/// Hand a `std::vector<T>` off to Rust as a `std::malloc`'d raw array.
/// The caller frees via the matching `free_*` callback which uses
/// `std::free`.
template <typename T>
uint32_t HandOff(const std::vector<T>& rows, T** out_rows)
{
    if (!out_rows)
        return 0;
    *out_rows = nullptr;
    if (rows.empty())
        return 0;

    const size_t bytes = rows.size() * sizeof(T);
    auto* buf = static_cast<T*>(std::malloc(bytes));
    if (!buf)
        return 0;
    std::memcpy(buf, rows.data(), bytes);
    *out_rows = buf;
    return static_cast<uint32_t>(rows.size());
}

/// Resolve a logged-in player from the low half of an ObjectGuid.
Player* FindPlayer(uint32_t guid_low)
{
    if (guid_low == 0)
        return nullptr;
    ObjectGuid og(HIGHGUID_PLAYER, guid_low);
    return sObjectMgr.GetPlayer(og, false);
}

/// Walk `sTalentStore`/`sTalentTabStore` to find the talent tab with
/// the most points invested. Mirrors the legacy
/// `GetPlayerSpecTab` helper that lived in RandomItemMgr.cpp.
uint32_t ComputeBestTalentTab(Player* bot)
{
    if (!bot)
        return 0;

    const uint32 classMask = bot->getClassMask();
    int maxCount = 0;
    uint32_t bestTab = 0;

    for (uint32 tab = 0; tab < 3; ++tab)
    {
        int count = 0;
        for (uint32 row = 0; row < sTalentStore.GetNumRows(); ++row)
        {
            TalentEntry const* talent = sTalentStore.LookupEntry(row);
            if (!talent)
                continue;
            TalentTabEntry const* tabInfo =
                sTalentTabStore.LookupEntry(talent->TalentTab);
            if (!tabInfo || tabInfo->tabpage != tab ||
                !(tabInfo->ClassMask & classMask))
                continue;

            for (int rank = MAX_TALENT_RANK - 1; rank >= 0; --rank)
            {
                if (talent->RankID[rank] && bot->HasSpell(talent->RankID[rank]))
                {
                    count += rank + 1;
                    break;
                }
            }
        }
        if (count > maxCount)
        {
            maxCount = count;
            bestTab = tab;
        }
    }
    return bestTab;
}

} // namespace

// ── Item prototype enumeration ───────────────────────────────────────────

uint32_t ItemBridge::CB_QueryItemPrototypes(BotItemPrototype** out_rows)
{
    if (!out_rows)
        return 0;
    *out_rows = nullptr;

    std::vector<BotItemPrototype> rows;
    rows.reserve(sItemStorage.GetRecordCount());

    const uint32 maxEntry = sItemStorage.GetMaxEntry();
    for (uint32 id = 0; id < maxEntry; ++id)
    {
        ItemPrototype const* proto = sItemStorage.LookupEntry<ItemPrototype>(id);
        if (!proto)
            continue;

        BotItemPrototype dst{};
        dst.item_id                     = proto->ItemId;
        dst.class_id                    = proto->Class;
        dst.sub_class                   = proto->SubClass;
        dst.unk0                        = 0;
        dst.quality                     = proto->Quality;
        dst.flags                       = proto->Flags;
        dst.flags2                      = 0;
        dst.buy_count                   = proto->BuyCount;
        dst.buy_price                   = static_cast<int32_t>(proto->BuyPrice);
        dst.sell_price                  = proto->SellPrice;
        dst.inventory_type              = proto->InventoryType;
        dst.allowable_class             = static_cast<int32_t>(proto->AllowableClass);
        dst.allowable_race              = static_cast<int32_t>(proto->AllowableRace);
        dst.item_level                  = proto->ItemLevel;
        dst.required_level              = proto->RequiredLevel;
        dst.required_skill              = proto->RequiredSkill;
        dst.required_skill_rank         = proto->RequiredSkillRank;
        dst.required_spell              = proto->RequiredSpell;
        dst.required_honor_rank         = proto->RequiredHonorRank;
        dst.required_city_rank          = proto->RequiredCityRank;
        dst.required_reputation_faction = proto->RequiredReputationFaction;
        dst.required_reputation_rank    = proto->RequiredReputationRank;
        dst.max_count                   = proto->MaxCount;
        dst.stackable                   = proto->Stackable;
        dst.container_slots             = proto->ContainerSlots;
        dst.stats_count                 = MAX_ITEM_PROTO_STATS;
        dst.scaling_stat_distribution   = 0;
        dst.scaling_stat_value          = 0;
        dst.armor                       = proto->Armor;
        dst.holy_res                    = proto->HolyRes;
        dst.fire_res                    = proto->FireRes;
        dst.nature_res                  = proto->NatureRes;
        dst.frost_res                   = proto->FrostRes;
        dst.shadow_res                  = proto->ShadowRes;
        dst.arcane_res                  = proto->ArcaneRes;
        dst.delay                       = proto->Delay;
        dst.ammo_type                   = proto->AmmoType;
        dst.ranged_mod_range            = proto->RangedModRange;
        dst.bonding                     = proto->Bonding;
        dst.page_text                   = static_cast<int32_t>(proto->PageText);
        dst.lang                        = proto->LanguageID;
        dst.page_material               = proto->PageMaterial;
        dst.start_quest                 = proto->StartQuest;
        dst.lock_id                     = proto->LockID;
        dst.material                    = static_cast<int32_t>(proto->Material);
        dst.sheath                      = proto->Sheath;
        dst.random_property             = static_cast<int32_t>(proto->RandomProperty);
        dst.random_suffix               = 0;
        dst.block                       = proto->Block;
        dst.itemset                     = proto->ItemSet;
        dst.max_durability              = proto->MaxDurability;
        dst.area                        = proto->Area;
        dst.map                         = static_cast<int32_t>(proto->Map);
        dst.bag_family                  = proto->BagFamily;
        dst.totem_category              = 0;
        dst.socket_bonus                = 0;
        dst.gem_properties              = 0;
        dst.required_disenchant_skill   = -1;
        dst.armor_damage_modifier       = 0.0f;
        dst.duration                    = proto->Duration;
        dst.item_limit_category         = 0;
        dst.holiday_id                  = 0;

        CopyCString(dst.name, sizeof(dst.name), proto->Name1);

        for (uint32 i = 0; i < MAX_ITEM_PROTO_STATS; ++i)
        {
            dst.stats[i].stat_type  = proto->ItemStat[i].ItemStatType;
            dst.stats[i].stat_value = proto->ItemStat[i].ItemStatValue;
        }
        for (uint32 i = 0; i < MAX_ITEM_PROTO_DAMAGES; ++i)
        {
            dst.damages[i].damage_min  = proto->Damage[i].DamageMin;
            dst.damages[i].damage_max  = proto->Damage[i].DamageMax;
            dst.damages[i].damage_type = proto->Damage[i].DamageType;
        }
        // No sockets on Classic — leave dst.sockets zero-initialised.
        for (uint32 i = 0; i < MAX_ITEM_PROTO_SPELLS; ++i)
        {
            dst.spells[i].spell_id                = static_cast<int32_t>(proto->Spells[i].SpellId);
            dst.spells[i].spell_trigger           = proto->Spells[i].SpellTrigger;
            dst.spells[i].spell_charges           = proto->Spells[i].SpellCharges;
            dst.spells[i].spell_ppm_rate          = proto->Spells[i].SpellPPMRate;
            dst.spells[i].spell_cooldown          = proto->Spells[i].SpellCooldown;
            dst.spells[i].spell_category          = proto->Spells[i].SpellCategory;
            dst.spells[i].spell_category_cooldown = proto->Spells[i].SpellCategoryCooldown;
        }

        rows.push_back(dst);
    }

    return HandOff(rows, out_rows);
}

void ItemBridge::CB_FreeItemPrototypeList(BotItemPrototype* rows, uint32_t /*count*/)
{
    std::free(rows);
}

// ── Spell entry lookup ───────────────────────────────────────────────────

bool ItemBridge::CB_LookupSpellEntry(uint32_t spell_id, BotSpellEntryInfo* out)
{
    if (!out)
        return false;

    SpellEntry const* spell = sSpellTemplate.LookupEntry<SpellEntry>(spell_id);
    if (!spell)
        return false;

    std::memset(out, 0, sizeof(*out));
    out->id = spell->Id;
    for (uint32 i = 0; i < MAX_EFFECT_INDEX; ++i)
    {
        out->effect[i]                 = spell->Effect[i];
        out->effect_apply_aura_name[i] = spell->EffectApplyAuraName[i];
        out->effect_base_points[i]     = spell->EffectBasePoints[i];
        out->effect_misc_value[i]      = spell->EffectMiscValue[i];
        out->effect_misc_value_b[i]    = 0; // No EffectMiscValueB on Classic.
        out->effect_die_sides[i]       = static_cast<uint32_t>(spell->EffectDieSides[i]);
    }

    int32_t duration_ms = 0;
    if (spell->DurationIndex)
    {
        if (SpellDurationEntry const* dur =
                sSpellDurationStore.LookupEntry(spell->DurationIndex))
        {
            duration_ms = dur->Duration[0];
        }
    }
    out->duration_ms     = duration_ms;
    out->recovery_time_ms = spell->RecoveryTime;

    // Classic stores procChance as uint32; use -1 sentinel for "not a
    // proc spell" so the Rust side can tell a 0% proc roll from a
    // non-proc ability.
    out->proc_chance = spell->procFlags != 0
        ? static_cast<int32_t>(spell->procChance)
        : -1;

    // Classic stores School as a single index; the Rust side expects a
    // mask of the same shape as `SpellEntry::GetSchoolMask` (1 << School).
    out->school_mask = 1u << spell->School;

    CopyCString(out->name, sizeof(out->name),
                spell->SpellName[0] ? spell->SpellName[0] : "");
    return true;
}

// ── Weight scales (custom playerbot tables) ──────────────────────────────

uint32_t ItemBridge::CB_QueryWeightScales(BotWeightScaleRow** out_rows)
{
    if (!out_rows)
        return 0;
    *out_rows = nullptr;

    std::vector<BotWeightScaleRow> rows;

    auto result = WorldDatabase.PQuery(
        "SELECT id, name, class FROM ai_playerbot_weightscales");
    if (!result)
        return 0;

    do
    {
        Field* f = result->Fetch();
        BotWeightScaleRow r{};
        r.id       = f[0].GetUInt32();
        r.class_id = f[2].GetUInt32();
        CopyCString(r.name, sizeof(r.name), f[1].GetString());
        rows.push_back(r);
    } while (result->NextRow());

    return HandOff(rows, out_rows);
}

void ItemBridge::CB_FreeWeightScaleList(BotWeightScaleRow* rows, uint32_t /*count*/)
{
    std::free(rows);
}

uint32_t ItemBridge::CB_QueryWeightScaleStats(BotWeightScaleStatRow** out_rows)
{
    if (!out_rows)
        return 0;
    *out_rows = nullptr;

    std::vector<BotWeightScaleStatRow> rows;

    auto result = WorldDatabase.PQuery(
        "SELECT id, field, val FROM ai_playerbot_weightscale_data");
    if (!result)
        return 0;

    do
    {
        Field* f = result->Fetch();
        BotWeightScaleStatRow r{};
        r.scale_id = f[0].GetUInt32();
        r.weight   = static_cast<int32_t>(f[2].GetUInt32());
        CopyCString(r.stat, sizeof(r.stat), f[1].GetString());
        rows.push_back(r);
    } while (result->NextRow());

    return HandOff(rows, out_rows);
}

void ItemBridge::CB_FreeWeightScaleStatList(BotWeightScaleStatRow* rows, uint32_t /*count*/)
{
    std::free(rows);
}

// ── Quest → reward item rows ─────────────────────────────────────────────

uint32_t ItemBridge::CB_QueryQuestItems(BotQuestItemRow** out_rows)
{
    if (!out_rows)
        return 0;
    *out_rows = nullptr;

    std::vector<BotQuestItemRow> rows;
    rows.reserve(4096);

    const ObjectMgr::QuestMap& quests = sObjectMgr.GetQuestTemplates();
    for (const auto& kv : quests)
    {
        Quest const* q = kv.second.get();
        if (!q)
            continue;

        BotQuestItemRow base{};
        base.quest_id            = q->GetQuestId();
        base.min_level           = q->GetMinLevel();
        base.required_classes    = q->GetRequiredClasses();
        base.required_races      = q->GetRequiredRaces();
        base.zone_or_sort        = static_cast<uint32_t>(q->GetZoneOrSort());
        base.reward_rep_faction  = q->RewRepFaction[0];
        base.reward_rep_value    = q->RewRepValue[0];

        for (uint32 i = 0; i < QUEST_REWARDS_COUNT; ++i)
        {
            if (q->RewItemId[i] == 0)
                continue;
            BotQuestItemRow r = base;
            r.item_id   = q->RewItemId[i];
            r.is_choice = false;
            rows.push_back(r);
        }
        for (uint32 i = 0; i < QUEST_REWARD_CHOICES_COUNT; ++i)
        {
            if (q->RewChoiceItemId[i] == 0)
                continue;
            BotQuestItemRow r = base;
            r.item_id   = q->RewChoiceItemId[i];
            r.is_choice = true;
            rows.push_back(r);
        }
    }

    return HandOff(rows, out_rows);
}

void ItemBridge::CB_FreeQuestItemList(BotQuestItemRow* rows, uint32_t /*count*/)
{
    std::free(rows);
}

// ── Vendor sources ───────────────────────────────────────────────────────

uint32_t ItemBridge::CB_QueryVendorItems(BotVendorItemRow** out_rows)
{
    if (!out_rows)
        return 0;
    *out_rows = nullptr;

    std::vector<BotVendorItemRow> rows;

    auto result = WorldDatabase.Query("SELECT item, entry FROM npc_vendor");
    if (!result)
        return 0;

    do
    {
        Field* f = result->Fetch();
        const uint32 item_id     = f[0].GetUInt32();
        const uint32 vendor_entry = f[1].GetUInt32();
        if (!item_id || !vendor_entry)
            continue;

        CreatureInfo const* ci =
            sCreatureStorage.LookupEntry<CreatureInfo>(vendor_entry);
        if (!ci)
            continue;

        BotVendorItemRow r{};
        r.vendor_entry            = vendor_entry;
        r.item_id                 = item_id;
        r.vendor_faction_template = ci->Faction;

        if (FactionTemplateEntry const* ft =
                sFactionTemplateStore.LookupEntry(ci->Faction))
        {
            r.vendor_faction = ft->faction;
        }

        // Condition resolution is driven by the per-vendor item's
        // `conditionId`. Pull it from the cached vendor item data.
        if (VendorItemData const* vd =
                sObjectMgr.GetNpcVendorItemList(vendor_entry))
        {
            if (VendorItem const* vi = vd->FindItem(item_id))
                r.condition_id = vi->conditionId;
        }

        // Classic `npc_vendor` does not store explicit rep-rank gates
        // per vendor row; leave required_reputation_rank/faction zero
        // so the Rust side treats the vendor as unconditionally
        // reachable (matching the legacy behaviour).
        rows.push_back(r);
    } while (result->NextRow());

    return HandOff(rows, out_rows);
}

void ItemBridge::CB_FreeVendorItemList(BotVendorItemRow* rows, uint32_t /*count*/)
{
    std::free(rows);
}

// ── Item enchantment template ────────────────────────────────────────────

uint32_t ItemBridge::CB_QueryItemEnchantmentTemplate(BotItemEnchantmentRow** out_rows)
{
    if (!out_rows)
        return 0;
    *out_rows = nullptr;

    std::vector<BotItemEnchantmentRow> rows;

    auto result = WorldDatabase.Query(
        "SELECT entry, ench, chance FROM item_enchantment_template");
    if (!result)
        return 0;

    do
    {
        Field* f = result->Fetch();
        BotItemEnchantmentRow r{};
        r.entry  = f[0].GetUInt32();
        r.ench   = f[1].GetUInt32();
        r.chance = f[2].GetFloat();
        rows.push_back(r);
    } while (result->NextRow());

    return HandOff(rows, out_rows);
}

void ItemBridge::CB_FreeItemEnchantmentList(BotItemEnchantmentRow* rows, uint32_t /*count*/)
{
    std::free(rows);
}

// ── Enchant lookups ──────────────────────────────────────────────────────

bool ItemBridge::CB_LookupEnchantInfo(uint32_t enchant_id, BotEnchantInfo* out)
{
    if (!out)
        return false;

    SpellItemEnchantmentEntry const* ench =
        sSpellItemEnchantmentStore.LookupEntry(enchant_id);
    if (!ench)
    {
        std::memset(out, 0, sizeof(*out));
        out->is_valid = false;
        return false;
    }

    std::memset(out, 0, sizeof(*out));
    out->id = ench->ID;
    for (uint32 i = 0; i < 3; ++i)
    {
        out->type[i]    = ench->type[i];
        out->amount[i]  = static_cast<int32_t>(ench->amount[i]);
        out->spell_id[i] = ench->spellid[i];
    }
    CopyCString(out->description, sizeof(out->description),
                ench->description[0] ? ench->description[0] : "");
    out->is_valid = true;
    return true;
}

bool ItemBridge::CB_LookupRandomPropertyEnchants(int32_t property_id,
                                                 uint32_t out_enchant_ids[4])
{
    if (!out_enchant_ids)
        return false;

    const uint32 id = static_cast<uint32>(property_id < 0 ? -property_id : property_id);
    ItemRandomPropertiesEntry const* entry =
        sItemRandomPropertiesStore.LookupEntry(id);
    if (!entry)
    {
        for (uint32 i = 0; i < 4; ++i)
            out_enchant_ids[i] = 0;
        return false;
    }

    // Classic only has 3 enchant slots; zero-fill the 4th.
    out_enchant_ids[0] = entry->enchant_id[0];
    out_enchant_ids[1] = entry->enchant_id[1];
    out_enchant_ids[2] = entry->enchant_id[2];
    out_enchant_ids[3] = 0;
    return true;
}

// ── Rarity cache ─────────────────────────────────────────────────────────

uint32_t ItemBridge::CB_QueryRarityRows(BotItemRarityRow** out_rows)
{
    if (!out_rows)
        return 0;
    *out_rows = nullptr;

    std::vector<BotItemRarityRow> rows;

    auto result = CharacterDatabase.Query(
        "SELECT item, rarity FROM ai_playerbot_rarity_cache");
    if (!result)
        return 0;

    do
    {
        Field* f = result->Fetch();
        BotItemRarityRow r{};
        r.item_id = f[0].GetUInt32();
        r.rarity  = f[1].GetFloat();
        rows.push_back(r);
    } while (result->NextRow());

    return HandOff(rows, out_rows);
}

void ItemBridge::CB_FreeRarityList(BotItemRarityRow* rows, uint32_t /*count*/)
{
    std::free(rows);
}

// ── Gem properties (empty on Classic) ────────────────────────────────────

uint32_t ItemBridge::CB_QueryGemProperties(BotGemPropertiesRow** out_rows)
{
    if (out_rows)
        *out_rows = nullptr;
    return 0;
}

void ItemBridge::CB_FreeGemList(BotGemPropertiesRow* rows, uint32_t /*count*/)
{
    std::free(rows);
}

// ── Per-player runtime queries ───────────────────────────────────────────

bool ItemBridge::CB_GetPlayerItemCtx(uint32_t guid_low, BotPlayerItemCtx* out)
{
    if (!out)
        return false;

    Player* p = FindPlayer(guid_low);
    if (!p)
        return false;

    std::memset(out, 0, sizeof(*out));
    out->guid_low = guid_low;
    out->level    = p->GetLevel();
    out->class_id = p->getClass();
    out->race_id  = p->getRace();
    out->team     = (p->GetTeam() == HORDE) ? 1u : 0u;
    out->pvp_rank = p->GetHonorHighestRankInfo().rank;
    out->map_id   = p->GetMapId();
    out->in_world = p->IsInWorld();
    return true;
}

int32_t ItemBridge::CB_GetPlayerReputationRank(uint32_t guid_low, uint32_t faction_id)
{
    Player* p = FindPlayer(guid_low);
    if (!p)
        return 0;
    return static_cast<int32_t>(p->GetReputationRank(faction_id));
}

uint32_t ItemBridge::CB_GetPlayerSkillValue(uint32_t guid_low, uint32_t skill_id)
{
    Player* p = FindPlayer(guid_low);
    if (!p)
        return 0;
    return p->GetSkillValue(static_cast<uint16>(skill_id));
}

uint32_t ItemBridge::CB_PlayerQuestStatus(uint32_t guid_low, uint32_t quest_id)
{
    Player* p = FindPlayer(guid_low);
    if (!p)
        return 0;

    Quest const* quest = sObjectMgr.GetQuestTemplate(quest_id);
    if (!quest)
        return 0;

    uint32_t flags = 0;
    if (p->SatisfyQuestClass(quest, false))
        flags |= PLAYERBOT_QUEST_SATISFIES_CLASS;
    if (p->SatisfyQuestRace(quest, false))
        flags |= PLAYERBOT_QUEST_SATISFIES_RACE;
    if (p->SatisfyQuestLevel(quest, false))
        flags |= PLAYERBOT_QUEST_SATISFIES_LEVEL;

    if (p->IsActiveQuest(quest_id))
        flags |= PLAYERBOT_QUEST_IS_ACTIVE;

    const QuestStatus status = p->GetQuestStatus(quest_id);
    if (status == QUEST_STATUS_COMPLETE)
        flags |= PLAYERBOT_QUEST_IS_COMPLETED;

    // Rewarded state lives on the quest-status data directly.
    const QuestStatusMap& qs = p->getQuestStatusMap();
    auto it = qs.find(quest_id);
    if (it != qs.end() && it->second.m_rewarded)
        flags |= PLAYERBOT_QUEST_IS_REWARDED;

    return flags;
}

bool ItemBridge::CB_PlayerHasItem(uint32_t guid_low, uint32_t item_id)
{
    Player* p = FindPlayer(guid_low);
    if (!p)
        return false;
    return p->HasItemCount(item_id, 1, true);
}

uint32_t ItemBridge::CB_GetPlayerTalentTab(uint32_t guid_low,
                                           uint32_t* out_meditation_rank)
{
    if (out_meditation_rank)
        *out_meditation_rank = 0;

    Player* p = FindPlayer(guid_low);
    if (!p)
        return 0;
    return ComputeBestTalentTab(p);
}

// ── RNG + debug logging ──────────────────────────────────────────────────

uint32_t ItemBridge::CB_UrandRange(uint32_t min_v, uint32_t max_v)
{
    if (max_v < min_v)
        return min_v;
    return urand(min_v, max_v);
}

void ItemBridge::CB_LogDebug(const char* msg)
{
    if (msg)
        sLog.outError("%s", msg);
}

// ── Vtable builder ───────────────────────────────────────────────────────

ItemCallbacks ItemBridge::MakeCallbacks()
{
    ItemCallbacks cbs{};
    cbs.query_item_prototypes          = &ItemBridge::CB_QueryItemPrototypes;
    cbs.free_item_prototype_list       = &ItemBridge::CB_FreeItemPrototypeList;
    cbs.lookup_spell_entry             = &ItemBridge::CB_LookupSpellEntry;
    cbs.query_weight_scales            = &ItemBridge::CB_QueryWeightScales;
    cbs.free_weight_scale_list         = &ItemBridge::CB_FreeWeightScaleList;
    cbs.query_weight_scale_stats       = &ItemBridge::CB_QueryWeightScaleStats;
    cbs.free_weight_scale_stat_list    = &ItemBridge::CB_FreeWeightScaleStatList;
    cbs.query_quest_items              = &ItemBridge::CB_QueryQuestItems;
    cbs.free_quest_item_list           = &ItemBridge::CB_FreeQuestItemList;
    cbs.query_vendor_items             = &ItemBridge::CB_QueryVendorItems;
    cbs.free_vendor_item_list          = &ItemBridge::CB_FreeVendorItemList;
    cbs.query_item_enchantment_template = &ItemBridge::CB_QueryItemEnchantmentTemplate;
    cbs.free_item_enchantment_list     = &ItemBridge::CB_FreeItemEnchantmentList;
    cbs.lookup_enchant_info            = &ItemBridge::CB_LookupEnchantInfo;
    cbs.lookup_random_property_enchants = &ItemBridge::CB_LookupRandomPropertyEnchants;
    cbs.query_rarity_rows              = &ItemBridge::CB_QueryRarityRows;
    cbs.free_rarity_list               = &ItemBridge::CB_FreeRarityList;
    cbs.query_gem_properties           = &ItemBridge::CB_QueryGemProperties;
    cbs.free_gem_list                  = &ItemBridge::CB_FreeGemList;
    cbs.get_player_item_ctx            = &ItemBridge::CB_GetPlayerItemCtx;
    cbs.get_player_reputation_rank     = &ItemBridge::CB_GetPlayerReputationRank;
    cbs.get_player_skill_value         = &ItemBridge::CB_GetPlayerSkillValue;
    cbs.player_quest_status            = &ItemBridge::CB_PlayerQuestStatus;
    cbs.player_has_item                = &ItemBridge::CB_PlayerHasItem;
    cbs.get_player_talent_tab          = &ItemBridge::CB_GetPlayerTalentTab;
    cbs.urand_range                    = &ItemBridge::CB_UrandRange;
    cbs.log_debug                      = &ItemBridge::CB_LogDebug;
    return cbs;
}
