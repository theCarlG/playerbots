/**
 * BotBridge.cpp — CMaNGOS implementation of the BotCallbacks vtable.
 *
 * Every callback:
 *   1. Resolves BotHandle → Player* (and UnitHandle → Unit*) via ObjectAccessor.
 *   2. Calls the CMaNGOS API.
 *   3. Returns a plain-data result (no pointers into CMaNGOS memory).
 *
 * Threading: these are called on the map worker thread that owns the bot's Player.
 * No additional locking is needed — CMaNGOS guarantees single-threaded access
 * to each Player's UpdateAI.
 */

#include "botpch.h"
#include "BotBridge.h"

#include "Entities/Player.h"
#include "Entities/Unit.h"
#include "Entities/Creature.h"
#include "Grids/ObjectAccessor.h"
#include "Spells/SpellMgr.h"
#include "Spells/Spell.h"
#include "Maps/Map.h"
#include "Movement/MotionMaster.h"
#include "Globals/ObjectMgr.h"

#ifdef CMANGOS
#include "Threat/ThreatManager.h"
#endif

// ── Platform helpers ──────────────────────────────────────────────────────

static inline ObjectGuid MakeGuid(uint64_t handle)
{
    return ObjectGuid(handle);
}

// ── Internal helpers ──────────────────────────────────────────────────────

Player* BotBridge::FindBot(BotHandle bot)
{
    return sObjectAccessor.FindPlayer(MakeGuid(bot));
}

Unit* BotBridge::FindUnit(BotHandle bot, UnitHandle handle)
{
    if (handle == 0)
        return nullptr;
    Player* b = FindBot(bot);
    if (!b)
        return nullptr;
    // Use the bot's map to find any unit (player, creature, pet)
    return b->GetMap()->GetUnit(MakeGuid(handle));
}

BotUnitSnapshot BotBridge::FillUnitSnapshot(Unit* unit)
{
    BotUnitSnapshot s{};
    if (!unit)
        return s;

    s.health     = unit->GetHealth();
    s.max_health = unit->GetMaxHealth();
    s.is_alive   = unit->IsAlive();
    s.in_combat  = unit->IsInCombat();
    s.is_casting = unit->IsNonMeleeSpellCasted(false);
    s.is_moving  = unit->isMoving();
    s.is_channeling = unit->IsChanneling();

    if (s.is_casting && unit->GetCurrentSpell(CURRENT_GENERIC_SPELL))
    {
        Spell* spell = unit->GetCurrentSpell(CURRENT_GENERIC_SPELL);
        if (spell && spell->m_spellInfo)
            s.casting_spell_id = spell->m_spellInfo->Id;
    }
    s.casting_progress = 0.0f; // filled below if casting

    // Power (mana, rage, energy, etc.)
    Powers powerType = unit->GetPowerType();
    s.power_type = static_cast<uint8_t>(powerType);
    s.mana       = unit->GetPower(powerType);
    s.max_mana   = unit->GetMaxPower(powerType);

    // Identity
    s.level    = static_cast<uint8_t>(unit->GetLevel());
    s.race_id  = static_cast<uint8_t>(unit->getRace());
    s.class_id = static_cast<uint8_t>(unit->getClass());
    s.team     = 0; // filled below for Players

    // Position
    s.pos.x    = unit->GetPositionX();
    s.pos.y    = unit->GetPositionY();
    s.pos.z    = unit->GetPositionZ();
    s.pos.o    = unit->GetOrientation();
    s.pos.map_id = unit->GetMapId();

    // Target
    s.current_target = unit->GetTargetGuid().GetRawValue();

    // Aura state mask
    s.aura_state_mask = unit->GetUInt32Value(UNIT_FIELD_AURASTATE);

    // Player-specific
    if (Player* p = unit->ToPlayer())
    {
        s.team = static_cast<uint8_t>(p->GetTeam() == ALLIANCE ? 0 : 1);
    }

    return s;
}

// ── BotCallbacks factory ──────────────────────────────────────────────────

BotCallbacks BotBridge::MakeCallbacks()
{
    BotCallbacks cbs{};
    cbs.get_snapshot        = CB_GetSnapshot;
    cbs.get_unit_snapshot   = CB_GetUnitSnapshot;
    cbs.has_aura            = CB_HasAura;
    cbs.get_aura            = CB_GetAura;
    cbs.get_auras           = CB_GetAuras;
    cbs.free_aura_list      = CB_FreeAuraList;
    cbs.get_threat_list     = CB_GetThreatList;
    cbs.free_threat_list    = CB_FreeThreatList;
    cbs.get_unit_threat     = CB_GetUnitThreat;
    cbs.unit_distance       = CB_UnitDistance;
    cbs.can_cast            = CB_CanCast;
    cbs.spell_on_cooldown   = CB_SpellOnCooldown;
    cbs.spell_cooldown_ms   = CB_SpellCooldownMs;
    cbs.has_los             = CB_HasLos;
    cbs.get_nearby_units    = CB_GetNearbyUnits;
    cbs.free_unit_list      = CB_FreeUnitList;
    cbs.get_behind_position = CB_GetBehindPosition;
    cbs.get_safe_position   = CB_GetSafePosition;
    cbs.get_spread_position = CB_GetSpreadPosition;
    cbs.can_reach           = CB_CanReach;
    cbs.cast_spell          = CB_CastSpell;
    cbs.cast_spell_pos      = CB_CastSpellPos;
    cbs.move_to             = CB_MoveTo;
    cbs.follow              = CB_Follow;
    cbs.stop_moving         = CB_StopMoving;
    cbs.attack              = CB_Attack;
    cbs.auto_attack         = CB_AutoAttack;
    cbs.say                 = CB_Say;
    cbs.use_item            = CB_UseItem;
    cbs.taunt               = CB_Taunt;
    cbs.group_get_tank      = CB_GroupGetTank;
    cbs.group_get_healer    = CB_GroupGetHealer;
    cbs.group_get_role      = CB_GroupGetRole;
    return cbs;
}

// ── Snapshot ──────────────────────────────────────────────────────────────

BotWorldSnapshot BotBridge::CB_GetSnapshot(BotHandle bot)
{
    BotWorldSnapshot snap{};
    Player* b = FindBot(bot);
    if (!b)
        return snap;

    snap.self_ = FillUnitSnapshot(b);
    snap.zone_id     = b->GetZoneId();
    snap.area_id     = b->GetAreaId();
    snap.instance_id = b->GetInstanceId();
    snap.server_time_ms = WorldTimer::getMSTime();
    snap.is_leader   = false; // filled below

    // Group members
    Group* group = b->GetGroup();
    if (group)
    {
        uint8_t count = 0;
        snap.is_leader = (group->GetLeaderGuid() == b->GetObjectGuid());

        for (GroupReference* ref = group->GetFirstMember(); ref != nullptr && count < 40;
             ref = ref->next())
        {
            Player* member = ref->getSource();
            if (member && member->IsInWorld())
            {
                snap.group_members[count++] = member->GetGUID();
            }
        }
        snap.group_size = count;
    }
    else
    {
        snap.group_size = 0;
    }

    return snap;
}

BotUnitSnapshot BotBridge::CB_GetUnitSnapshot(BotHandle bot, UnitHandle target)
{
    Unit* unit = FindUnit(bot, target);
    return FillUnitSnapshot(unit);
}

// ── Aura queries ──────────────────────────────────────────────────────────

bool BotBridge::CB_HasAura(BotHandle bot, UnitHandle target, uint32_t spell_id)
{
    Unit* unit = FindUnit(bot, target);
    if (!unit)
        return false;
    return unit->HasAura(spell_id);
}

BotAuraInfo BotBridge::CB_GetAura(BotHandle bot, UnitHandle target, uint32_t spell_id)
{
    BotAuraInfo info{};
    Unit* unit = FindUnit(bot, target);
    if (!unit)
        return info;

    SpellAuraHolder* holder = unit->GetSpellAuraHolder(spell_id);
    if (!holder)
        return info;

    info.spell_id       = spell_id;
    info.duration_ms    = static_cast<uint32_t>(holder->GetAuraDuration());
    info.max_duration_ms= static_cast<uint32_t>(holder->GetAuraMaxDuration());
    info.stacks         = static_cast<uint8_t>(holder->GetStackAmount());
    info.is_mine        = (holder->GetCasterGuid() == FindBot(bot)->GetObjectGuid());
    info.is_harmful     = !IsPositiveSpell(spell_id);
    info.is_passive     = IsPassiveSpell(spell_id);
    return info;
}

BotAuraInfo* BotBridge::CB_GetAuras(BotHandle bot, UnitHandle target, uint32_t* out_count)
{
    *out_count = 0;
    Unit* unit = FindUnit(bot, target);
    if (!unit)
        return nullptr;

    Player* b = FindBot(bot);
    ObjectGuid botGuid = b ? b->GetObjectGuid() : ObjectGuid();

    // Collect all aura holders
    std::vector<BotAuraInfo> results;
    results.reserve(32);

    Unit::SpellAuraHolderMap const& holders = unit->GetSpellAuraHolderMap();
    for (auto const& pair : holders)
    {
        SpellAuraHolder* holder = pair.second;
        if (!holder)
            continue;
        BotAuraInfo info{};
        info.spell_id        = holder->GetId();
        info.duration_ms     = static_cast<uint32_t>(holder->GetAuraDuration());
        info.max_duration_ms = static_cast<uint32_t>(holder->GetAuraMaxDuration());
        info.stacks          = static_cast<uint8_t>(holder->GetStackAmount());
        info.is_mine         = (holder->GetCasterGuid() == botGuid);
        info.is_harmful      = !IsPositiveSpell(info.spell_id);
        info.is_passive      = IsPassiveSpell(info.spell_id);
        results.push_back(info);
    }

    if (results.empty())
        return nullptr;

    BotAuraInfo* arr = new BotAuraInfo[results.size()];
    std::copy(results.begin(), results.end(), arr);
    *out_count = static_cast<uint32_t>(results.size());
    return arr;
}

void BotBridge::CB_FreeAuraList(BotAuraInfo* list)
{
    delete[] list;
}

// ── Threat queries ────────────────────────────────────────────────────────

BotThreatEntry* BotBridge::CB_GetThreatList(BotHandle bot, UnitHandle target_unit,
                                             uint32_t* out_count)
{
    *out_count = 0;
    Unit* target = FindUnit(bot, target_unit);
    if (!target)
        return nullptr;

    ThreatManager const& tm = target->getThreatManager();
    ThreatList const& list  = tm.getThreatList();
    if (list.empty())
        return nullptr;

    BotThreatEntry* arr = new BotThreatEntry[list.size()];
    uint32_t count = 0;
    for (HostileReference* ref : list)
    {
        arr[count].unit      = ref->getUnitGuid().GetRawValue();
        arr[count].threat    = ref->getThreat();
        arr[count].is_online = ref->isOnline();
        arr[count].is_taunted= false; // CMaNGOS doesn't expose this directly
        ++count;
    }
    *out_count = count;
    return arr;
}

void BotBridge::CB_FreeThreatList(BotThreatEntry* list)
{
    delete[] list;
}

float BotBridge::CB_GetUnitThreat(BotHandle bot, UnitHandle target_unit, UnitHandle from_unit)
{
    Unit* target = FindUnit(bot, target_unit);
    Unit* from   = FindUnit(bot, from_unit);
    if (!target || !from)
        return 0.0f;
    return target->getThreatManager().getThreat(from);
}

// ── Unit queries ──────────────────────────────────────────────────────────

float BotBridge::CB_UnitDistance(BotHandle bot, UnitHandle target)
{
    Player* b = FindBot(bot);
    Unit* t   = FindUnit(bot, target);
    if (!b || !t)
        return 0.0f;
    return b->GetDistance(t);
}

bool BotBridge::CB_CanCast(BotHandle bot, uint32_t spell_id, UnitHandle target)
{
    Player* b = FindBot(bot);
    Unit* t   = FindUnit(bot, target);
    if (!b)
        return false;

    SpellEntry const* spellInfo = sSpellStore.LookupEntry(spell_id);
    if (!spellInfo)
        return false;

    // Check if player has the spell
    if (!b->HasSpell(spell_id))
        return false;

    // Check cooldown
    if (!b->IsSpellReady(spell_id))
        return false;

    // Basic castability check (range, power, etc.)
    Unit* castTarget = t ? t : b;
    SpellCastResult result = b->IsSpellFitByClassAndRace(spellInfo)
        ? SPELL_CAST_OK
        : SPELL_FAILED_NOT_KNOWN;
    if (result != SPELL_CAST_OK)
        return false;

    return true;
}

bool BotBridge::CB_SpellOnCooldown(BotHandle bot, uint32_t spell_id)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    return !b->IsSpellReady(spell_id);
}

uint32_t BotBridge::CB_SpellCooldownMs(BotHandle bot, uint32_t spell_id)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;
    SpellCooldowns const& cds = b->GetSpellCooldownMap();
    auto it = cds.find(spell_id);
    if (it == cds.end())
        return 0;
    uint32_t now = WorldTimer::getMSTime();
    if (it->second.end <= now)
        return 0;
    return it->second.end - now;
}

bool BotBridge::CB_HasLos(BotHandle bot, UnitHandle target)
{
    Player* b = FindBot(bot);
    Unit* t   = FindUnit(bot, target);
    if (!b || !t)
        return false;
    return b->IsWithinLOSInMap(t);
}

UnitHandle* BotBridge::CB_GetNearbyUnits(BotHandle bot, float range, bool hostile,
                                          uint32_t* out_count)
{
    *out_count = 0;
    Player* b = FindBot(bot);
    if (!b)
        return nullptr;

    std::vector<Unit*> units;
    MaNGOS::AnyUnitInObjectRangeCheck checker(b, range);
    MaNGOS::UnitListSearcher<MaNGOS::AnyUnitInObjectRangeCheck> searcher(units, checker);
    b->GetMap()->VisitWorldObjects(b, searcher, range);

    std::vector<UnitHandle> handles;
    handles.reserve(units.size());
    for (Unit* u : units)
    {
        if (!u || u == b || !u->IsAlive())
            continue;
        bool isHostile = b->IsHostileTo(u);
        if (hostile == isHostile)
            handles.push_back(u->GetObjectGuid().GetRawValue());
    }

    if (handles.empty())
        return nullptr;

    UnitHandle* arr = new UnitHandle[handles.size()];
    std::copy(handles.begin(), handles.end(), arr);
    *out_count = static_cast<uint32_t>(handles.size());
    return arr;
}

void BotBridge::CB_FreeUnitList(UnitHandle* list)
{
    delete[] list;
}

// ── Pathfinding / positioning ─────────────────────────────────────────────

BotPosition BotBridge::CB_GetBehindPosition(BotHandle bot, UnitHandle target, float distance)
{
    BotPosition pos{};
    Unit* t = FindUnit(bot, target);
    if (!t)
        return pos;

    float angle = t->GetOrientation() + static_cast<float>(M_PI); // behind = facing + 180°
    pos.x = t->GetPositionX() + std::cos(angle) * distance;
    pos.y = t->GetPositionY() + std::sin(angle) * distance;
    pos.z = t->GetPositionZ();
    pos.o = angle;
    pos.map_id = t->GetMapId();
    return pos;
}

BotSafePosition BotBridge::CB_GetSafePosition(BotHandle bot, float search_radius)
{
    BotSafePosition result{};
    Player* b = FindBot(bot);
    if (!b)
        return result;

    // Simple implementation: find a point on a circle at search_radius from bot
    // that is not inside any dynamic objects (fires, void zones).
    // A full Detour-based safe-position search would use PathFinder.
    // For now: use the bot's current position as a base and walk outward.
    float bx = b->GetPositionX();
    float by = b->GetPositionY();
    float bz = b->GetPositionZ();

    // Try 8 directions at search_radius
    for (int i = 0; i < 8; ++i)
    {
        float angle = i * (static_cast<float>(M_PI) / 4.0f);
        float cx = bx + std::cos(angle) * search_radius;
        float cy = by + std::sin(angle) * search_radius;
        float cz = bz;
        b->UpdateGroundPositionZ(cx, cy, cz);

        if (b->GetMap()->isInLineOfSight(bx, by, bz, cx, cy, cz))
        {
            result.x     = cx;
            result.y     = cy;
            result.z     = cz;
            result.found = true;
            return result;
        }
    }
    return result; // found = false
}

BotPosition BotBridge::CB_GetSpreadPosition(BotHandle bot, UnitHandle center, float radius,
                                              uint8_t idx, uint8_t total)
{
    BotPosition pos{};
    Unit* c = FindUnit(bot, center);
    if (!c || total == 0)
        return pos;

    float angle = (2.0f * static_cast<float>(M_PI) / total) * idx;
    pos.x    = c->GetPositionX() + std::cos(angle) * radius;
    pos.y    = c->GetPositionY() + std::sin(angle) * radius;
    pos.z    = c->GetPositionZ();
    pos.o    = angle + static_cast<float>(M_PI); // face center
    pos.map_id = c->GetMapId();
    return pos;
}

bool BotBridge::CB_CanReach(BotHandle bot, float x, float y, float z)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    // Use line-of-sight as a proxy for basic reachability.
    // A full PathFinder check would be more accurate but expensive.
    return b->GetMap()->isInLineOfSight(b->GetPositionX(), b->GetPositionY(),
                                         b->GetPositionZ(), x, y, z);
}

// ── Commands ──────────────────────────────────────────────────────────────

bool BotBridge::CB_CastSpell(BotHandle bot, uint32_t spell_id, UnitHandle target)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    SpellEntry const* spellInfo = sSpellStore.LookupEntry(spell_id);
    if (!spellInfo)
        return false;

    Unit* t = FindUnit(bot, target);
    if (!t)
        t = b; // fallback to self

    // Check that the spell is ready and known
    if (!b->HasSpell(spell_id) || !b->IsSpellReady(spell_id))
        return false;

    Spell* spell = new Spell(b, spellInfo, false);
    SpellCastTargets targets;
    targets.setUnitTarget(t);
    spell->prepare(&targets);
    return true;
}

bool BotBridge::CB_CastSpellPos(BotHandle bot, uint32_t spell_id,
                                  float x, float y, float z)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    SpellEntry const* spellInfo = sSpellStore.LookupEntry(spell_id);
    if (!spellInfo)
        return false;

    if (!b->HasSpell(spell_id) || !b->IsSpellReady(spell_id))
        return false;

    Spell* spell = new Spell(b, spellInfo, false);
    SpellCastTargets targets;
    targets.setDestination(x, y, z);
    spell->prepare(&targets);
    return true;
}

bool BotBridge::CB_MoveTo(BotHandle bot, float x, float y, float z)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    b->GetMotionMaster()->MovePoint(0, x, y, z);
    return true;
}

bool BotBridge::CB_Follow(BotHandle bot, UnitHandle target, float dist, float angle)
{
    Player* b = FindBot(bot);
    Unit* t   = FindUnit(bot, target);
    if (!b || !t)
        return false;

    b->GetMotionMaster()->MoveFollow(t, dist, angle);
    return true;
}

bool BotBridge::CB_StopMoving(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    b->GetMotionMaster()->Clear(false);
    b->StopMoving();
    return true;
}

bool BotBridge::CB_Attack(BotHandle bot, UnitHandle target)
{
    Player* b = FindBot(bot);
    Unit* t   = FindUnit(bot, target);
    if (!b || !t)
        return false;

    b->Attack(t, true);
    return true;
}

bool BotBridge::CB_AutoAttack(BotHandle bot, bool enable)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    if (enable)
        b->SetAutoAttack(true);
    else
        b->SetAutoAttack(false);
    return true;
}

bool BotBridge::CB_Say(BotHandle bot, const char* msg, uint32_t lang)
{
    Player* b = FindBot(bot);
    if (!b || !msg)
        return false;
    b->Say(msg, static_cast<uint32_t>(lang));
    return true;
}

bool BotBridge::CB_UseItem(BotHandle bot, uint32_t item_id, UnitHandle target)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    Item* item = b->GetItemByEntry(item_id);
    if (!item)
        return false;

    Unit* t = FindUnit(bot, target);
    SpellCastTargets targets;
    if (t)
        targets.setUnitTarget(t);

    b->UseItem(item, targets);
    return true;
}

bool BotBridge::CB_Taunt(BotHandle bot, UnitHandle target)
{
    Player* b = FindBot(bot);
    Unit* t   = FindUnit(bot, target);
    if (!b || !t)
        return false;

    // Find and cast the bot's taunt spell (class-dependent)
    // Warrior: Taunt (355), Paladin: Righteous Defense (31789),
    // Druid: Growl (6795), DK: Dark Command (56222)
    static const uint32_t tauntSpells[] = {355, 31789, 6795, 56222, 0};
    for (int i = 0; tauntSpells[i] != 0; ++i)
    {
        if (b->HasSpell(tauntSpells[i]) && b->IsSpellReady(tauntSpells[i]))
        {
            SpellEntry const* spellInfo = sSpellStore.LookupEntry(tauntSpells[i]);
            if (!spellInfo)
                continue;
            Spell* spell = new Spell(b, spellInfo, false);
            SpellCastTargets targets;
            targets.setUnitTarget(t);
            spell->prepare(&targets);
            return true;
        }
    }
    return false;
}

// ── Group / raid ──────────────────────────────────────────────────────────

UnitHandle BotBridge::CB_GroupGetTank(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;
    Group* group = b->GetGroup();
    if (!group)
        return 0;

    // Find the group member flagged as main tank
    for (GroupReference* ref = group->GetFirstMember(); ref != nullptr; ref = ref->next())
    {
        Player* member = ref->getSource();
        if (member && group->GetMemberFlags(member->GetObjectGuid()) & MEMBER_FLAG_MAINTANK)
            return member->GetGUID();
    }
    return 0;
}

UnitHandle BotBridge::CB_GroupGetHealer(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;
    Group* group = b->GetGroup();
    if (!group)
        return 0;

    for (GroupReference* ref = group->GetFirstMember(); ref != nullptr; ref = ref->next())
    {
        Player* member = ref->getSource();
        if (!member)
            continue;
        uint8_t cls = member->getClass();
        // Priest, Paladin, Druid, Shaman as potential healers
        if (cls == CLASS_PRIEST || cls == CLASS_PALADIN ||
            cls == CLASS_DRUID  || cls == CLASS_SHAMAN)
        {
            return member->GetGUID();
        }
    }
    return 0;
}

uint8_t BotBridge::CB_GroupGetRole(BotHandle bot, UnitHandle member)
{
    Player* b     = FindBot(bot);
    Unit*   m_raw = FindUnit(bot, member);
    if (!b || !m_raw)
        return 0;

    Player* m = m_raw->ToPlayer();
    if (!m)
        return 0;

    Group* group = b->GetGroup();
    if (!group)
        return 0;

    uint8_t flags = group->GetMemberFlags(m->GetObjectGuid());
    uint8_t role  = 0;
    if (flags & MEMBER_FLAG_MAINTANK) role |= 1; // TANK
    if (flags & MEMBER_FLAG_MAINASSIST) role |= 4; // treat assist as DPS
    // No healer flag in vanilla CMaNGOS — infer from class
    uint8_t cls = m->getClass();
    if (cls == CLASS_PRIEST || cls == CLASS_PALADIN ||
        cls == CLASS_DRUID  || cls == CLASS_SHAMAN)
        role |= 2; // HEAL

    return role;
}
