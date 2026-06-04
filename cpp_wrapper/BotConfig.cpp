/**
 * cpp_wrapper/BotConfig.cpp — Phase D compatibility shim implementation.
 *
 * The Rust `playerbot::config` module now owns the .conf parser. This file
 * simply calls `playerbot_config_load()` once and then copies every field
 * into the C++ `sPlayerbotAIConfig` mirror via the typed FFI accessors
 * declared in `cpp_wrapper/botffi.h`.
 *
 * The runtime helpers that have nothing to do with parsing — DB queries
 * for `freeAltBots`, file logging, `GetValue`/`SetValue` dispatch — stay
 * here as verbatim ports of the old `playerbot/PlayerbotAIConfig.cpp`
 * implementations. They'll move to Rust in later phases (DB in Phase F,
 * logging in Phase K).
 */

#include "botpch.h"
#include "BotConfig.h"

#include "botffi.h"

#include "Config/Config.h"
#include "Database/DatabaseEnv.h"
#include "Log/Log.h"
#include "Policies/Singleton.h"
#include "RandomPlayerbotMgr.h"

#include <algorithm>
#include <cctype>
#include <cstdarg>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <ctime>
#include <sstream>
#include <utility>

INSTANTIATE_SINGLETON_1(PlayerbotAIConfig);

PlayerbotAIConfig::PlayerbotAIConfig() = default;

// ── Small helpers that turn FFI results into std containers ─────────────

namespace
{

// Pull a u32 list from Rust and copy into a generic container.
template <class Container>
void PullU32List(const char* key, Container& out)
{
    out.clear();
    uint32_t* ptr = nullptr;
    size_t len = 0;
    if (!playerbot_config_get_u32_list_dup(key, &ptr, &len))
        return;
    for (size_t i = 0; i < len; ++i)
        out.push_back(ptr[i]);
    playerbot_config_free_u32_list(ptr, len);
}

// Pull a string list from Rust and copy into a generic container.
template <class Container>
void PullStringList(const char* key, Container& out)
{
    out.clear();
    char** ptr = nullptr;
    size_t len = 0;
    if (!playerbot_config_get_string_list_dup(key, &ptr, &len))
        return;
    for (size_t i = 0; i < len; ++i)
        out.push_back(ptr[i] ? std::string(ptr[i]) : std::string());
    playerbot_config_free_string_list(ptr, len);
}

std::string PullString(const char* key, const char* fallback = "")
{
    char* dup = playerbot_config_get_string_dup(key, fallback);
    std::string result = dup ? std::string(dup) : std::string();
    playerbot_config_free_cstr(dup);
    return result;
}

} // namespace

bool PlayerbotAIConfig::Initialize()
{
    sLog.outString("Initializing AI Playerbot by Claude?, based on the original Playerbot by ike3 and blueboy");

    if (!playerbot_config_load(_D_AIPLAYERBOT_CONFIG.c_str()))
    {
        sLog.outString("AI Playerbot is Disabled. Unable to open configuration file aiplayerbot.conf");
        return false;
    }

    enabled = playerbot_config_get_bool("AiPlayerbot.Enabled", false);
    if (!enabled)
    {
        sLog.outString("AI Playerbot is Disabled in aiplayerbot.conf");
        return false;
    }

    BarGoLink::SetOutputState(playerbot_config_get_bool("AiPlayerbot.ShowProgressBars", false));

    // ── Delays ──────────────────────────────────────────────────────────
    globalCoolDown     = playerbot_config_get_u32("AiPlayerbot.GlobalCooldown", 500);
    maxWaitForMove     = playerbot_config_get_u32("AiPlayerbot.MaxWaitForMove", 3000);
    expireActionTime   = playerbot_config_get_u32("AiPlayerbot.ExpireActionTime", 5000);
    dispelAuraDuration = playerbot_config_get_u32("AiPlayerbot.DispelAuraDuration", 2000);
    reactDelay         = playerbot_config_get_u32("AiPlayerbot.ReactDelay", 100);
    passiveDelay       = playerbot_config_get_u32("AiPlayerbot.PassiveDelay", 4000);
    repeatDelay        = playerbot_config_get_u32("AiPlayerbot.RepeatDelay", 5000);
    errorDelay         = playerbot_config_get_u32("AiPlayerbot.ErrorDelay", 5000);
    rpgDelay           = playerbot_config_get_u32("AiPlayerbot.RpgDelay", 3000);
    sitDelay           = playerbot_config_get_u32("AiPlayerbot.SitDelay", 30000);
    returnDelay        = playerbot_config_get_u32("AiPlayerbot.ReturnDelay", 7000);
    lootDelay          = playerbot_config_get_u32("AiPlayerbot.LootDelayDelay", 750);

    // ── Distances ───────────────────────────────────────────────────────
    farDistance                                    = playerbot_config_get_f32("AiPlayerbot.FarDistance", 20.0f);
    sightDistance                                  = playerbot_config_get_f32("AiPlayerbot.SightDistance", 75.0f);
    spellDistance                                  = playerbot_config_get_f32("AiPlayerbot.SpellDistance", 25.0f);
    shootDistance                                  = playerbot_config_get_f32("AiPlayerbot.ShootDistance", 25.0f);
    healDistance                                   = playerbot_config_get_f32("AiPlayerbot.HealDistance", 125.0f);
    reactDistance                                  = playerbot_config_get_f32("AiPlayerbot.ReactDistance", 150.0f);
    maxFreeMoveDistance                            = playerbot_config_get_f32("AiPlayerbot.MaxFreeMoveDistance", 150.0f);
    freeMoveDelay                                  = playerbot_config_get_f32("AiPlayerbot.FreeMoveDelay", 30.0f);
    grindDistance                                  = playerbot_config_get_f32("AiPlayerbot.GrindDistance", 75.0f);
    aggroDistance                                  = playerbot_config_get_f32("AiPlayerbot.AggroDistance", 22.0f);
    lootDistance                                   = playerbot_config_get_f32("AiPlayerbot.LootDistance", 25.0f);
    groupMemberLootDistance                        = playerbot_config_get_f32("AiPlayerbot.GroupMemberLootDistance", 15.0f);
    groupMemberLootDistanceWithActiveMaster        = playerbot_config_get_f32("AiPlayerbot.GroupMemberLootDistanceWithActiveMaster", 10.0f);
    gatheringDistance                              = playerbot_config_get_f32("AiPlayerbot.GatheringDistance", 15.0f);
    groupMemberGatheringDistance                   = playerbot_config_get_f32("AiPlayerbot.GroupMemberGatheringDistance", 10.0f);
    groupMemberGatheringDistanceWithActiveMaster   = playerbot_config_get_f32("AiPlayerbot.GroupMemberGatheringDistanceWithActiveMaster", 5.0f);
    fleeDistance                                   = playerbot_config_get_f32("AiPlayerbot.FleeDistance", 8.0f);
    tooCloseDistance                               = playerbot_config_get_f32("AiPlayerbot.TooCloseDistance", 5.0f);
    meleeDistance                                  = playerbot_config_get_f32("AiPlayerbot.MeleeDistance", 1.5f);
    followDistance                                 = playerbot_config_get_f32("AiPlayerbot.FollowDistance", 1.5f);
    raidFollowDistance                             = playerbot_config_get_f32("AiPlayerbot.RaidFollowDistance", 5.0f);
    wanderMinDistance                              = playerbot_config_get_f32("AiPlayerbot.WanderMinDistance", 5.0f);
    wanderMaxDistance                              = playerbot_config_get_f32("AiPlayerbot.WanderMaxDistance", 50.0f);
    whisperDistance                                = playerbot_config_get_f32("AiPlayerbot.WhisperDistance", 6000.0f);
    contactDistance                                = playerbot_config_get_f32("AiPlayerbot.ContactDistance", 0.5f);
    aoeRadius                                      = playerbot_config_get_f32("AiPlayerbot.AoeRadius", 5.0f);
    rpgDistance                                    = playerbot_config_get_f32("AiPlayerbot.RpgDistance", 80.0f);
    proximityDistance                              = playerbot_config_get_f32("AiPlayerbot.ProximityDistance", 20.0f);

    criticalHealth   = playerbot_config_get_u32("AiPlayerbot.CriticalHealth", 20);
    lowHealth        = playerbot_config_get_u32("AiPlayerbot.LowHealth", 50);
    mediumHealth     = playerbot_config_get_u32("AiPlayerbot.MediumHealth", 70);
    almostFullHealth = playerbot_config_get_u32("AiPlayerbot.AlmostFullHealth", 90);
    lowMana          = playerbot_config_get_u32("AiPlayerbot.LowMana", 15);
    mediumMana       = playerbot_config_get_u32("AiPlayerbot.MediumMana", 40);

    // ── Random gear ─────────────────────────────────────────────────────
    randomGearMaxLevel            = playerbot_config_get_u32("AiPlayerbot.RandomGearMaxLevel", 500);
    randomGearMaxDiff             = playerbot_config_get_u32("AiPlayerbot.RandomGearMaxDiff", 9);
    randomGearUpgradeEnabled      = playerbot_config_get_bool("AiPlayerbot.RandomGearUpgradeEnabled", false);
    randomGearTabards             = playerbot_config_get_bool("AiPlayerbot.RandomGearTabards", false);
    randomGearTabardsChance       = playerbot_config_get_f32("AiPlayerbot.RandomGearTabardsChance", 0.1f);
    randomGearTabardsReplaceGuild = playerbot_config_get_bool("AiPlayerbot.RandomGearTabardsReplaceGuild", false);
    randomGearTabardsUnobtainable = playerbot_config_get_bool("AiPlayerbot.RandomGearTabardsUnobtainable", false);
    PullU32List("AiPlayerbot.RandomGearBlacklist", randomGearBlacklist);
    PullU32List("AiPlayerbot.RandomGearWhitelist", randomGearWhitelist);
    randomGearProgression    = playerbot_config_get_bool("AiPlayerbot.RandomGearProgression", true);
    randomGearLoweringChance = playerbot_config_get_f32("AiPlayerbot.RandomGearLoweringChance", 0.15f);
    randomBotMaxLevelChance  = playerbot_config_get_f32("AiPlayerbot.RandomBotMaxLevelChance", 0.15f);
    rollBadItemsWithPlayer   = playerbot_config_get_bool("AiPlayerbot.RollBadItemsWithPlayer", false);
    randomBotRpgChance       = playerbot_config_get_f32("AiPlayerbot.RandomBotRpgChance", 0.35f);
    usePotionChance          = playerbot_config_get_f32("AiPlayerbot.UsePotionChance", 1.0f);
    attackEmoteChance        = playerbot_config_get_f32("AiPlayerbot.AttackEmoteChance", 0.0f);

    // ── Jumping ─────────────────────────────────────────────────────────
    jumpNoCombatChance      = playerbot_config_get_f32("AiPlayerbot.JumpNoCombatChance", 0.5f);
    jumpMeleeInCombatChance = playerbot_config_get_f32("AiPlayerbot.JumpMeleeInCombatChance", 0.5f);
    jumpRandomChance        = playerbot_config_get_f32("AiPlayerbot.JumpRandomChance", 0.20f);
    jumpInPlaceChance       = playerbot_config_get_f32("AiPlayerbot.JumpInPlaceChance", 0.50f);
    jumpBackwardChance      = playerbot_config_get_f32("AiPlayerbot.JumpBackwardChance", 0.10f);
    jumpHeightLimit         = playerbot_config_get_f32("AiPlayerbot.JumpHeightLimit", 60.0f);
    jumpVSpeed              = playerbot_config_get_f32("AiPlayerbot.JumpVSpeed", 7.96f);
    jumpHSpeed              = playerbot_config_get_f32("AiPlayerbot.JumpHSpeed", 7.0f);
    jumpInBg                = playerbot_config_get_bool("AiPlayerbot.JumpInBg", false);
    jumpWithPlayer          = playerbot_config_get_bool("AiPlayerbot.JumpWithPlayer", false);
    jumpFollow              = playerbot_config_get_bool("AiPlayerbot.JumpFollow", true);
    jumpChase               = playerbot_config_get_bool("AiPlayerbot.JumpChase", true);
    useKnockback            = playerbot_config_get_bool("AiPlayerbot.UseKnockback", true);

    iterationsPerTick = playerbot_config_get_u32("AiPlayerbot.IterationsPerTick", 100);

    allowGuildBots           = playerbot_config_get_bool("AiPlayerbot.AllowGuildBots", true);
    allowMultiAccountAltBots = playerbot_config_get_bool("AiPlayerbot.AllowMultiAccountAltBots", true);

    randomBotMapsAsString = PullString("AiPlayerbot.RandomBotMaps", "0,1,530,571");
    {
        // Re-parse the string list into the std::vector<uint32> the consumers expect.
        randomBotMaps.clear();
        const char* src = randomBotMapsAsString.c_str();
        while (*src)
        {
            while (*src == ' ' || *src == ',') ++src;
            if (!*src) break;
            char* end = nullptr;
            unsigned long v = std::strtoul(src, &end, 10);
            if (end == src) break;
            randomBotMaps.push_back(static_cast<uint32>(v));
            src = end;
        }
    }
    PullU32List("AiPlayerbot.RandomBotQuestItems", randomBotQuestItems);
    PullU32List("AiPlayerbot.RandomBotSpellIds", randomBotSpellIds);
    PullU32List("AiPlayerbot.PvpProhibitedZoneIds", pvpProhibitedZoneIds);

    PullU32List("AiPlayerbot.RandomBotQuestIds", randomBotQuestIds);
    PullU32List("AiPlayerbot.ImmuneSpellIds", immuneSpellIds);

    botAutologin            = BotAutoLogin(playerbot_config_get_u32("AiPlayerbot.BotAutologin", 0));
    randomBotAutologin      = playerbot_config_get_bool("AiPlayerbot.RandomBotAutologin", true);
    randomBotAutoCreate     = playerbot_config_get_bool("AiPlayerbot.RandomBotAutoCreate", true);
    minRandomBots           = playerbot_config_get_u32("AiPlayerbot.MinRandomBots", 50);
    maxRandomBots           = playerbot_config_get_u32("AiPlayerbot.MaxRandomBots", 200);
    randomBotUpdateInterval = playerbot_config_get_u32("AiPlayerbot.RandomBotUpdateInterval", 1 * 1000);
    randomBotCountChangeMinInterval = playerbot_config_get_u32("AiPlayerbot.RandomBotCountChangeMinInterval", 1 * 1800);
    randomBotCountChangeMaxInterval = playerbot_config_get_u32("AiPlayerbot.RandomBotCountChangeMaxInterval", 2 * 3600);
    randomBotTimedLogout    = playerbot_config_get_bool("AiPlayerbot.RandomBotTimedLogout", true);
    randomBotTimedOffline   = playerbot_config_get_bool("AiPlayerbot.RandomBotTimedOffline", false);
    minRandomBotInWorldTime = playerbot_config_get_u32("AiPlayerbot.MinRandomBotInWorldTime", 1 * 1800);
    maxRandomBotInWorldTime = playerbot_config_get_u32("AiPlayerbot.MaxRandomBotInWorldTime", 6 * 3600);
    minRandomBotRandomizeTime = playerbot_config_get_u32("AiPlayerbot.MinRandomBotRandomizeTime", 6 * 3600);
    maxRandomBotRandomizeTime = playerbot_config_get_u32("AiPlayerbot.MaxRandomRandomizeTime", 24 * 3600);
    minRandomBotChangeStrategyTime = playerbot_config_get_u32("AiPlayerbot.MinRandomBotChangeStrategyTime", 1800);
    maxRandomBotChangeStrategyTime = playerbot_config_get_u32("AiPlayerbot.MaxRandomBotChangeStrategyTime", 2 * 3600);
    minRandomBotReviveTime  = playerbot_config_get_u32("AiPlayerbot.MinRandomBotReviveTime", 60);
    maxRandomBotReviveTime  = playerbot_config_get_u32("AiPlayerbot.MaxRandomReviveTime", 300);
    enableRandomTeleports   = playerbot_config_get_bool("AiPlayerbot.EnableRandomTeleports", true);
    enableMinimalMove       = playerbot_config_get_bool("AiPlayerbot.EnableMinimalMove", true);

    randomBotTeleportDistance                  = playerbot_config_get_u32("AiPlayerbot.RandomBotTeleportDistance", 1000);
    transportTeleportType                      = playerbot_config_get_u32("AiPlayerbot.TransportTeleportType", 2);
    randomBotTeleportNearPlayer                = playerbot_config_get_bool("AiPlayerbot.RandomBotTeleportNearPlayer", false);
    randomBotTeleportNearPlayerMaxAmount       = playerbot_config_get_u32("AiPlayerbot.RandomBotTeleportNearPlayerMaxAmount", 0);
    randomBotTeleportNearPlayerMaxAmountRadius = playerbot_config_get_f32("AiPlayerbot.RandomBotTeleportNearPlayerMaxAmountRadius", 0.0f);
    randomBotTeleportMinInterval               = playerbot_config_get_u32("AiPlayerbot.RandomBotTeleportTeleportMinInterval", 2 * 3600);
    randomBotTeleportMaxInterval               = playerbot_config_get_u32("AiPlayerbot.RandomBotTeleportTeleportMaxInterval", 48 * 3600);
    randomBotsMaxLoginsPerInterval             = playerbot_config_get_u32("AiPlayerbot.RandomBotsMaxLoginsPerInterval", 10);
    randomBotsPerInterval                      = playerbot_config_get_u32("AiPlayerbot.RandomBotsPerInterval", 0);
    minRandomBotsPriceChangeInterval           = playerbot_config_get_u32("AiPlayerbot.MinRandomBotsPriceChangeInterval", 2 * 3600);
    maxRandomBotsPriceChangeInterval           = playerbot_config_get_u32("AiPlayerbot.MaxRandomBotsPriceChangeInterval", 48 * 3600);

    // ── AH settings ─────────────────────────────────────────────────────
    shouldQueryAHListingsOutsideOfAH = playerbot_config_get_bool("AiPlayerbot.ShouldQueryAHListingsOutsideOfAH", true);
    PullU32List("AiPlayerbot.AhOverVendorItemIds", ahOverVendorItemIds);
    PullU32List("AiPlayerbot.VendorOverAHItemIds", vendorOverAHItemIds);
    botCheckAllAuctionListings = playerbot_config_get_bool("AiPlayerbot.BotCheckAllAuctionListings", false);
    botsSaveEpics              = playerbot_config_get_bool("AiPlayerbot.BotsSaveEpics", true);

    randomBotJoinLfg          = playerbot_config_get_bool("AiPlayerbot.RandomBotJoinLfg", true);
    logRandomBotJoinLfg       = playerbot_config_get_bool("AiPlayerbot.LogRandomBotJoinLfg", false);
    randomBotJoinBG           = playerbot_config_get_bool("AiPlayerbot.RandomBotJoinBG", true);
    randomBotAutoJoinBG       = playerbot_config_get_bool("AiPlayerbot.RandomBotAutoJoinBG", false);
    randomBotBracketCount     = playerbot_config_get_u32("AiPlayerbot.RandomBotBracketCount", 3);
    logInGroupOnly            = playerbot_config_get_bool("AiPlayerbot.LogInGroupOnly", true);
    logValuesPerTick          = playerbot_config_get_bool("AiPlayerbot.LogValuesPerTick", false);
    fleeingEnabled            = playerbot_config_get_bool("AiPlayerbot.FleeingEnabled", true);
    summonAtInnkeepersEnabled = playerbot_config_get_bool("AiPlayerbot.SummonAtInnkeepersEnabled", true);
    randomBotMinLevel         = playerbot_config_get_u32("AiPlayerbot.RandomBotMinLevel", 1);
    randomBotMaxLevel         = playerbot_config_get_u32("AiPlayerbot.RandomBotMaxLevel", 255);
    randomBotLoginAtStartup   = playerbot_config_get_bool("AiPlayerbot.RandomBotLoginAtStartup", true);
    randomBotTeleLevel        = playerbot_config_get_u32("AiPlayerbot.RandomBotTeleLevel", 5);
    openGoSpell               = playerbot_config_get_u32("AiPlayerbot.OpenGoSpell", 6477);

    randomChangeMultiplier = playerbot_config_get_f32("AiPlayerbot.RandomChangeMultiplier", 1.0f);

    randomBotCombatStrategies    = PullString("AiPlayerbot.RandomBotCombatStrategies", "-threat,+custom::say");
    randomBotNonCombatStrategies = PullString("AiPlayerbot.RandomBotNonCombatStrategies", "+custom::say");
    randomBotReactStrategies     = PullString("AiPlayerbot.RandomBotReactStrategies", "");
    randomBotDeadStrategies      = PullString("AiPlayerbot.RandomBotDeadStrategies", "");
    combatStrategies             = PullString("AiPlayerbot.CombatStrategies", "");
    nonCombatStrategies          = PullString("AiPlayerbot.NonCombatStrategies", "+return,+delayed roll");
    reactStrategies              = PullString("AiPlayerbot.ReactStrategies", "");
    deadStrategies               = PullString("AiPlayerbot.DeadStrategies", "");

    commandPrefix    = PullString("AiPlayerbot.CommandPrefix", "");
    commandSeparator = PullString("AiPlayerbot.CommandSeparator", "\\\\");

    commandServerPort    = static_cast<int>(playerbot_config_get_i32("AiPlayerbot.CommandServerPort", 0));
    perfMonEnabled       = playerbot_config_get_bool("AiPlayerbot.PerfMonEnabled", false);
    bExplicitDbStoreSave = playerbot_config_get_bool("AiPlayerbot.ExplicitDbStoreSave", false);

    randomBotLoginWithPlayer = playerbot_config_get_bool("AiPlayerbot.RandomBotLoginWithPlayer", false);
    asyncBotLogin            = playerbot_config_get_bool("AiPlayerbot.AsyncBotLogin", false);
    preloadHolders           = playerbot_config_get_bool("AiPlayerbot.PreloadHolders", false);

    freeRoomForNonSpareBots  = playerbot_config_get_u32("AiPlayerbot.FreeRoomForNonSpareBots", 1);
    loginBotsNearPlayerRange = playerbot_config_get_u32("AiPlayerbot.LoginBotsNearPlayerRange", 1000);

    // ── Login criteria (dynamic) ────────────────────────────────────────
    {
        defaultLoginCriteria.clear();
        char** ptr = nullptr;
        size_t len = 0;
        if (playerbot_config_default_login_criteria_dup(&ptr, &len))
        {
            for (size_t i = 0; i < len; ++i)
                defaultLoginCriteria.emplace_back(ptr[i] ? ptr[i] : "");
            playerbot_config_free_string_list(ptr, len);
        }
    }
    {
        loginCriteria.clear();
        size_t outer = playerbot_config_login_criteria_len();
        for (size_t i = 0; i < outer; ++i)
        {
            std::vector<std::string> row;
            char** ptr = nullptr;
            size_t len = 0;
            if (playerbot_config_login_criteria_at(i, &ptr, &len))
            {
                for (size_t j = 0; j < len; ++j)
                    row.emplace_back(ptr[j] ? ptr[j] : "");
                playerbot_config_free_string_list(ptr, len);
            }
            loginCriteria.push_back(std::move(row));
        }
    }

    // ── Level probabilities ─────────────────────────────────────────────
    for (uint32 level = 0; level <= DEFAULT_MAX_LEVEL; ++level)
        levelProbability[level] = playerbot_config_get_level_probability(level);

    // ── Class/race probability tables ───────────────────────────────────
    useFixedClassRaceCounts = playerbot_config_get_bool("AiPlayerbot.ClassRace.UseFixedClassRaceCounts", false);
    classRaceProbabilityTotal = playerbot_config_get_class_race_probability_total();
    for (uint32 cls = 0; cls < MAX_CLASSES; ++cls)
        for (uint32 race = 0; race < MAX_RACES; ++race)
            classRaceProbability[cls][race] = playerbot_config_get_class_race_probability(cls, race);

    if (useFixedClassRaceCounts)
    {
        fixedClassRaceCounts.clear();
        for (uint32 cls = 1; cls < MAX_CLASSES; ++cls)
        {
            for (uint32 race = 1; race < MAX_RACES; ++race)
            {
                int64_t count = playerbot_config_get_fixed_class_race_count(cls, race);
                if (count >= 0)
                    fixedClassRaceCounts[{static_cast<uint8>(cls), static_cast<uint8>(race)}] =
                        static_cast<uint32>(count);
            }
        }
    }

    // ── Cheat masks ─────────────────────────────────────────────────────
    botCheatMask    = playerbot_config_get_u32("AiPlayerbot.BotCheats", 0);
    rndBotCheatMask = playerbot_config_get_u32("AiPlayerbot.RndBotCheats", 0);

    PullStringList("AiPlayerbot.AllowedLogFiles", allowedLogFiles);
    PullStringList("AiPlayerbot.DebugFilter", debugFilter);

    // ── World buffs (dynamic keys) ──────────────────────────────────────
    worldBuffs.clear();
    {
        const size_t n = playerbot_config_world_buffs_len();
        worldBuffs.reserve(n);
        for (size_t i = 0; i < n; ++i)
        {
            worldBuff wb{};
            uint32_t spell = 0, faction = 0, cls = 0, spec = 0, minLv = 0, maxLv = 0, evt = 0;
            if (playerbot_config_world_buff_at(i, &spell, &faction, &cls, &spec, &minLv, &maxLv, &evt))
            {
                wb.spellId   = spell;
                wb.factionId = faction;
                wb.classId   = cls;
                wb.specId    = spec;
                wb.minLevel  = minLv;
                wb.maxLevel  = maxLv;
                wb.eventId   = evt;
                worldBuffs.push_back(wb);
            }
        }
    }

    randomBotAccountPrefix  = PullString("AiPlayerbot.RandomBotAccountPrefix", "rndbot");
    randomBotAccountCount   = playerbot_config_get_u32("AiPlayerbot.RandomBotAccountCount", 50);
    deleteRandomBotAccounts = playerbot_config_get_bool("AiPlayerbot.DeleteRandomBotAccounts", false);
    randomBotGuildCount     = playerbot_config_get_u32("AiPlayerbot.RandomBotGuildCount", 20);
    deleteRandomBotGuilds   = playerbot_config_get_bool("AiPlayerbot.DeleteRandomBotGuilds", false);

    randomBotArenaTeamCount   = playerbot_config_get_u32("AiPlayerbot.RandomBotArenaTeamCount", 20);
    deleteRandomBotArenaTeams = playerbot_config_get_bool("AiPlayerbot.DeleteRandomBotArenaTeams", false);

    randomBotShowCloak  = playerbot_config_get_bool("AiPlayerbot.RandomBotShowCloak", false);
    randomBotShowHelmet = playerbot_config_get_bool("AiPlayerbot.RandomBotShowHelmet", false);

    enableGreet              = playerbot_config_get_bool("AiPlayerbot.EnableGreet", false);
    disableRandomLevels      = playerbot_config_get_bool("AiPlayerbot.DisableRandomLevels", false);
    instantRandomize         = playerbot_config_get_bool("AiPlayerbot.InstantRandomize", true);
    randomBotRandomPassword  = playerbot_config_get_bool("AiPlayerbot.RandomBotRandomPassword", true);
    playerbotsXPrate         = playerbot_config_get_f32("AiPlayerbot.XPRate", 1.0f);
    disableBotOptimizations  = playerbot_config_get_bool("AiPlayerbot.DisableBotOptimizations", false);
    disableActivityPriorities = playerbot_config_get_bool("AiPlayerbot.DisableActivityPriorities", false);
    forceActiveWhenNearPlayer = playerbot_config_get_bool("AiPlayerbot.ForceActiveWhenNearPlayer", false);
    limitCombatActivity      = playerbot_config_get_bool("AiPlayerbot.LimitCombatActivity", false);
    guildOrderAlwaysActive   = playerbot_config_get_bool("AiPlayerbot.GuildOrderAlwaysActive", true);
    botActiveAlone           = playerbot_config_get_u32("AiPlayerbot.botActiveAlone", 10);
    diffWithPlayer           = playerbot_config_get_u32("AiPlayerbot.DiffWithPlayer", 100);
    diffEmpty                = playerbot_config_get_u32("AiPlayerbot.DiffEmpty", 200);
    RandombotsWalkingRPG         = playerbot_config_get_bool("AiPlayerbot.RandombotsWalkingRPG", false);
    RandombotsWalkingRPGInDoors  = playerbot_config_get_bool("AiPlayerbot.RandombotsWalkingRPG.InDoors", false);
    minEnchantingBotLevel    = playerbot_config_get_u32("AiPlayerbot.minEnchantingBotLevel", 60);
    randombotStartingLevel   = playerbot_config_get_u32("AiPlayerbot.randombotStartingLevel", 5);
    gearscorecheck           = playerbot_config_get_bool("AiPlayerbot.GearScoreCheck", false);
    levelCheck               = playerbot_config_get_i32("AiPlayerbot.LevelCheck", 30);
    randomBotPreQuests       = playerbot_config_get_bool("AiPlayerbot.PreQuests", true);
    randomBotSayWithoutMaster = playerbot_config_get_bool("AiPlayerbot.RandomBotSayWithoutMaster", false);
    randomBotInvitePlayer    = playerbot_config_get_bool("AiPlayerbot.RandomBotInvitePlayer", true);
    randomBotGroupNearby     = playerbot_config_get_bool("AiPlayerbot.RandomBotGroupNearby", true);
    randomBotRaidNearby      = playerbot_config_get_bool("AiPlayerbot.RandomBotRaidNearby", true);
    randomBotGuildNearby     = playerbot_config_get_bool("AiPlayerbot.RandomBotGuildNearby", true);
    inviteChat               = playerbot_config_get_bool("AiPlayerbot.InviteChat", true);
    enableOffSpecStrategies  = playerbot_config_get_bool("AiPlayerbot.EnableOffSpecStrategies", true);
    useWanderAsDefaultFollowStrategy = playerbot_config_get_bool("AiPlayerbot.UseWanderAsDefaultFollowStrategy", true);
    defaultFormation         = PullString("AiPlayerbot.DefaultFormation", "near");

    guildMaxBotLimit = playerbot_config_get_u32("AiPlayerbot.GuildMaxBotLimit", 1000);

    // ── Broadcasts ──────────────────────────────────────────────────────
    enableBroadcasts        = playerbot_config_get_bool("AiPlayerbot.EnableBroadcasts", true);
    broadcastChanceMaxValue = enableBroadcasts ? 30000 : 0;

    broadcastToGuildGlobalChance            = playerbot_config_get_u32("AiPlayerbot.BroadcastToGuildGlobalChance", 30000);
    broadcastToWorldGlobalChance            = playerbot_config_get_u32("AiPlayerbot.BroadcastToWorldGlobalChance", 30000);
    broadcastToGeneralGlobalChance          = playerbot_config_get_u32("AiPlayerbot.BroadcastToGeneralGlobalChance", 30000);
    broadcastToTradeGlobalChance            = playerbot_config_get_u32("AiPlayerbot.BroadcastToTradeGlobalChance", 30000);
    broadcastToLFGGlobalChance              = playerbot_config_get_u32("AiPlayerbot.BroadcastToLFGGlobalChance", 30000);
    broadcastToLocalDefenseGlobalChance     = playerbot_config_get_u32("AiPlayerbot.BroadcastToLocalDefenseGlobalChance", 30000);
    broadcastToWorldDefenseGlobalChance     = playerbot_config_get_u32("AiPlayerbot.BroadcastToWorldDefenseGlobalChance", 30000);
    broadcastToGuildRecruitmentGlobalChance = playerbot_config_get_u32("AiPlayerbot.BroadcastToGuildRecruitmentGlobalChance", 30000);
    broadcastToSayGlobalChance              = playerbot_config_get_u32("AiPlayerbot.BroadcastToSayGlobalChance", 30000);
    broadcastToYellGlobalChance             = playerbot_config_get_u32("AiPlayerbot.BroadcastToYellGlobalChance", 30000);

    broadcastChanceLootingItemPoor      = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceLootingItemPoor", 30);
    broadcastChanceLootingItemNormal    = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceLootingItemNormal", 300);
    broadcastChanceLootingItemUncommon  = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceLootingItemUncommon", 10000);
    broadcastChanceLootingItemRare      = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceLootingItemRare", 20000);
    broadcastChanceLootingItemEpic      = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceLootingItemEpic", 30000);
    broadcastChanceLootingItemLegendary = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceLootingItemLegendary", 30000);
    broadcastChanceLootingItemArtifact  = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceLootingItemArtifact", 30000);

    broadcastChanceQuestAccepted                 = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceQuestAccepted", 6000);
    broadcastChanceQuestUpdateObjectiveCompleted = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceQuestUpdateObjectiveCompleted", 300);
    broadcastChanceQuestUpdateObjectiveProgress  = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceQuestUpdateObjectiveProgress", 300);
    broadcastChanceQuestUpdateFailedTimer        = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceQuestUpdateFailedTimer", 300);
    broadcastChanceQuestUpdateComplete           = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceQuestUpdateComplete", 1000);
    broadcastChanceQuestTurnedIn                 = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceQuestTurnedIn", 10000);

    broadcastChanceKillNormal    = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceKillNormal", 30);
    broadcastChanceKillElite     = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceKillElite", 300);
    broadcastChanceKillRareelite = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceKillRareelite", 3000);
    broadcastChanceKillWorldboss = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceKillWorldboss", 20000);
    broadcastChanceKillRare      = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceKillRare", 10000);
    broadcastChanceKillUnknown   = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceKillUnknown", 100);
    broadcastChanceKillPet       = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceKillPet", 10);
    broadcastChanceKillPlayer    = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceKillPlayer", 30);

    broadcastChanceLevelupGeneric  = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceLevelupGeneric", 20000);
    broadcastChanceLevelupTenX     = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceLevelupTenX", 30000);
    broadcastChanceLevelupMaxLevel = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceLevelupMaxLevel", 30000);

    broadcastChanceSuggestInstance        = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceSuggestInstance", 5000);
    broadcastChanceSuggestQuest           = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceSuggestQuest", 10000);
    broadcastChanceSuggestGrindMaterials  = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceSuggestGrindMaterials", 5000);
    broadcastChanceSuggestGrindReputation = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceSuggestGrindReputation", 5000);
    broadcastChanceSuggestSell            = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceSuggestSell", 300);
    broadcastChanceSuggestSomething       = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceSuggestSomething", 30000);

    broadcastChanceSuggestSomethingToxic = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceSuggestSomethingToxic", 0);

    broadcastChanceSuggestToxicLinks = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceSuggestToxicLinks", 0);
    toxicLinksPrefix                 = PullString("AiPlayerbot.ToxicLinksPrefix", "gnomes");

    broadcastChanceSuggestThunderfury = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceSuggestThunderfury", 1);

    broadcastChanceGuildManagement = playerbot_config_get_u32("AiPlayerbot.BroadcastChanceGuildManagement", 30000);

    toxicLinksRepliesChance = playerbot_config_get_u32("AiPlayerbot.ToxicLinksRepliesChance", 30);
    thunderfuryRepliesChance = playerbot_config_get_u32("AiPlayerbot.ThunderfuryRepliesChance", 40);
    guildRepliesRate        = playerbot_config_get_u32("AiPlayerbot.GuildRepliesRate", 100);

    botAcceptDuelMinimumLevel = playerbot_config_get_u32("AiPlayerbot.BotAcceptDuelMinimumLevel", 10);

    randomBotFormGuild  = playerbot_config_get_bool("AiPlayerbot.RandomBotFormGuild", true);
    boostFollow         = playerbot_config_get_bool("AiPlayerbot.BoostFollow", false);
    turnInRpg           = playerbot_config_get_bool("AiPlayerbot.TurnInRpg", false);
    shareTargets        = playerbot_config_get_bool("AiPlayerbot.ShareTargets", true);
    globalSoundEffects  = playerbot_config_get_bool("AiPlayerbot.GlobalSoundEffects", false);
    nonGmFreeSummon     = playerbot_config_get_bool("AiPlayerbot.NonGmFreeSummon", false);

    autoPickReward         = PullString("AiPlayerbot.AutoPickReward", "no");
    autoEquipUpgradeLoot   = playerbot_config_get_bool("AiPlayerbot.AutoEquipUpgradeLoot", false);
    syncQuestWithPlayer    = playerbot_config_get_bool("AiPlayerbot.SyncQuestWithPlayer", false);
    syncQuestForPlayer     = playerbot_config_get_bool("AiPlayerbot.SyncQuestForPlayer", false);
    autoTrainSpells        = PullString("AiPlayerbot.AutoTrainSpells", "no");
    autoPickTalents        = PullString("AiPlayerbot.AutoPickTalents", "no");
    autoLearnTrainerSpells = playerbot_config_get_bool("AiPlayerbot.AutoLearnTrainerSpells", false);
    autoLearnQuestSpells   = playerbot_config_get_bool("AiPlayerbot.AutoLearnQuestSpells", false);
    autoLearnDroppedSpells = playerbot_config_get_bool("AiPlayerbot.AutoLearnDroppedSpells", false);
    autoDoQuests           = playerbot_config_get_bool("AiPlayerbot.AutoDoQuests", true);
    syncLevelWithPlayers   = playerbot_config_get_bool("AiPlayerbot.SyncLevelWithPlayers", false);
    syncLevelMaxAbove      = playerbot_config_get_u32("AiPlayerbot.SyncLevelMaxAbove", 5);
    syncLevelNoPlayer      = playerbot_config_get_u32("AiPlayerbot.SyncLevelNoPlayer", randombotStartingLevel);
    tweakValue             = playerbot_config_get_u32("AiPlayerbot.TweakValue", 0);
    talentsInPublicNote    = playerbot_config_get_bool("AiPlayerbot.TalentsInPublicNote", false);
    respawnModNeutral      = playerbot_config_get_f32("AiPlayerbot.RespawnModNeutral", 10.0f);
    respawnModHostile      = playerbot_config_get_f32("AiPlayerbot.RespawnModHostile", 5.0f);
    respawnModThreshold    = playerbot_config_get_u32("AiPlayerbot.RespawnModThreshold", 10);
    respawnModMax          = playerbot_config_get_u32("AiPlayerbot.RespawnModMax", 18);
    respawnModForPlayerBots = playerbot_config_get_bool("AiPlayerbot.RespawnModForPlayerBots", false);
    respawnModForInstances  = playerbot_config_get_bool("AiPlayerbot.RespawnModForInstances", false);

    // ── LLM ─────────────────────────────────────────────────────────────
    llmEnabled          = playerbot_config_get_u32("AiPlayerbot.LLMEnabled", 1);
    llmApiEndpoint      = PullString("AiPlayerbot.LLMApiEndpoint", "http://127.0.0.1:5001/api/v1/generate");
    // Rust already parsed and validated the URL; pull the parts back.
    llmEndPointUrl.hostname = PullString("AiPlayerbot.LLMApiEndpoint.Hostname", "");
    llmEndPointUrl.path     = PullString("AiPlayerbot.LLMApiEndpoint.Path", "/");
    llmEndPointUrl.port     = static_cast<int>(playerbot_config_get_i32("AiPlayerbot.LLMApiEndpoint.Port", 80));
    llmEndPointUrl.https    = playerbot_config_get_bool("AiPlayerbot.LLMApiEndpoint.Https", false);

    llmApiKey                     = PullString("AiPlayerbot.LLMApiKey", "");
    llmApiJson                    = PullString("AiPlayerbot.LLMApiJson",
                                                 "{ \"max_length\": 100, \"prompt\": \"[<pre prompt>]<context> <prompt> <post prompt>\"}");
    llmContextLength              = playerbot_config_get_u32("AiPlayerbot.LLMContextLength", 4096);
    llmGenerationTimeout          = playerbot_config_get_u32("AiPlayerbot.LLMGenerationTimeout", 600);
    llmMaxSimultaniousGenerations = playerbot_config_get_u32("AiPlayerbot.LLMMaxSimultaniousGenerations", 100);

    llmPrePrompt    = PullString("AiPlayerbot.LLMPrePrompt",
        "You are a roleplaying character in World of Warcraft: <expansion name>. Your name is <bot name>. The <other type> <other name> is speaking to you <channel name> and is an <other gender> <other race> <other class> of level <other level>. You are level <bot level> and play as a <bot gender> <bot race> <bot class> that is currently in <bot subzone> <bot zone>. Answer as a roleplaying character. Limit responses to 100 characters.");
    llmPreRpgPrompt = PullString("AiPlayerbot.LLMRpgPrompt",
        "In World of Warcraft: <expansion name> in <bot zone> <bot subzone> stands <bot type> <bot name> a level <bot level> <bot gender> <bot race> <bot class>. Standing nearby is <unit type> <unit name> <unit subname> a level <unit level> <unit gender> <unit race> <unit faction> <unit class>. Answer as a roleplaying character. Limit responses to 100 characters.");
    llmPrompt       = PullString("AiPlayerbot.LLMPrompt", "<receiver name>:<initial message>");
    llmPostPrompt   = PullString("AiPlayerbot.LLMPostPrompt", "<sender name>:");

    llmResponseStartPattern  = PullString("AiPlayerbot.LLMResponseStartPattern",  R"(("text":\s*"))");
    llmResponseEndPattern    = PullString("AiPlayerbot.LLMResponseEndPattern",    R"(("|\b(?!<sender name>\b)(\w+):))");
    llmResponseDeletePattern = PullString("AiPlayerbot.LLMResponseDeletePattern", R"((\\n|<sender name>:|\\[^ ]+))");
    llmResponseSplitPattern  = PullString("AiPlayerbot.LLMResponseSplitPattern",  R"((\*.*?\*)|(\[.*?\])|(\'.*\')|([^\*\[\] ][^\*\[\]]+?[.?!]))");

    llmGlobalContext      = playerbot_config_get_bool("AiPlayerbot.LLMGlobalContext", false);
    llmBotToBotChatChance = playerbot_config_get_u32("AiPlayerbot.LLMBotToBotChatChance", 0);
    llmRpgAIChatChance    = playerbot_config_get_u32("AiPlayerbot.LLMRpgAIChatChance", 100);

    // ── Gear progression (pre-derived arrays) ───────────────────────────
    gearProgressionSystemEnabled = playerbot_config_get_bool("AiPlayerbot.GearProgressionSystem.Enable", false);
    for (uint8 phase = 0; phase < MAX_GEAR_PROGRESSION_LEVEL; ++phase)
    {
        gearProgressionSystemItemLevels[phase][0] = playerbot_config_get_gear_min_item_level(phase);
        gearProgressionSystemItemLevels[phase][1] = playerbot_config_get_gear_max_item_level(phase);
        for (uint8 cls = 0; cls < MAX_CLASSES; ++cls)
            for (uint8 spec = 0; spec < 4; ++spec)
                for (uint8 slot = 0; slot < SLOT_EMPTY; ++slot)
                    gearProgressionSystemItems[phase][cls][spec][slot] =
                        playerbot_config_get_gear_item(phase, cls, spec, slot);
    }

    sLog.outString("Loading free bots.");
    selfBotLevel = BotSelfBotLevel(playerbot_config_get_u32("AiPlayerbot.SelfBotLevel",
                                                             static_cast<uint32>(BotSelfBotLevel::GM_ONLY)));
    PullStringList("AiPlayerbot.ToggleAlwaysOnlineAccounts", toggleAlwaysOnlineAccounts);
    PullStringList("AiPlayerbot.ToggleAlwaysOnlineChars", toggleAlwaysOnlineChars);

    for (std::string& nm : toggleAlwaysOnlineAccounts)
        std::transform(nm.begin(), nm.end(), nm.begin(),
                       [](unsigned char c) { return static_cast<char>(std::toupper(c)); });
    for (std::string& nm : toggleAlwaysOnlineChars)
    {
        std::transform(nm.begin(), nm.end(), nm.begin(),
                       [](unsigned char c) { return static_cast<char>(std::tolower(c)); });
        if (!nm.empty())
            nm[0] = static_cast<char>(std::toupper(static_cast<unsigned char>(nm[0])));
    }

    loadFreeAltBotAccounts();

    targetPosRecalcDistance = playerbot_config_get_f32("AiPlayerbot.TargetPosRecalcDistance", 0.1f);

    sLog.outString("---------------------------------------");
    sLog.outString("        AI Playerbot initialized       ");
    sLog.outString("---------------------------------------");
    sLog.outString();

    return true;
}

// ── Predicates (pure std::find wrappers over mirrored lists) ────────────

bool PlayerbotAIConfig::IsInRandomAccountList(uint32 id)
{
    return std::find(randomBotAccounts.begin(), randomBotAccounts.end(), id)
           != randomBotAccounts.end();
}

bool PlayerbotAIConfig::IsFreeAltBot(uint32 guid)
{
    for (auto const& bot : freeAltBots)
        if (bot.second == guid)
            return true;
    return false;
}

bool PlayerbotAIConfig::IsFreeAltBot(Player* player)
{
    return player && IsFreeAltBot(player->GetGUIDLow());
}

bool PlayerbotAIConfig::IsInRandomQuestItemList(uint32 id)
{
    return std::find(randomBotQuestItems.begin(), randomBotQuestItems.end(), id)
           != randomBotQuestItems.end();
}

bool PlayerbotAIConfig::IsInPvpProhibitedZone(uint32 id)
{
    return std::find(pvpProhibitedZoneIds.begin(), pvpProhibitedZoneIds.end(), id)
           != pvpProhibitedZoneIds.end();
}

// ── GetValue / SetValue (verbatim port — small dispatch table) ──────────

std::string PlayerbotAIConfig::GetValue(std::string name)
{
    std::ostringstream out;

    if (name == "GlobalCooldown") out << globalCoolDown;
    else if (name == "ReactDelay") out << reactDelay;
    else if (name == "SightDistance") out << sightDistance;
    else if (name == "SpellDistance") out << spellDistance;
    else if (name == "ReactDistance") out << reactDistance;
    else if (name == "GrindDistance") out << grindDistance;
    else if (name == "LootDistance") out << lootDistance;
    else if (name == "FleeDistance") out << fleeDistance;
    else if (name == "CriticalHealth") out << criticalHealth;
    else if (name == "LowHealth") out << lowHealth;
    else if (name == "MediumHealth") out << mediumHealth;
    else if (name == "AlmostFullHealth") out << almostFullHealth;
    else if (name == "LowMana") out << lowMana;
    else if (name == "IterationsPerTick") out << iterationsPerTick;

    return out.str();
}

void PlayerbotAIConfig::SetValue(std::string name, std::string value)
{
    std::istringstream in(value, std::istringstream::in);

    if (name == "GlobalCooldown") in >> globalCoolDown;
    else if (name == "ReactDelay") in >> reactDelay;
    else if (name == "SightDistance") in >> sightDistance;
    else if (name == "SpellDistance") in >> spellDistance;
    else if (name == "ReactDistance") in >> reactDistance;
    else if (name == "GrindDistance") in >> grindDistance;
    else if (name == "LootDistance") in >> lootDistance;
    else if (name == "FleeDistance") in >> fleeDistance;
    else if (name == "CriticalHealth") in >> criticalHealth;
    else if (name == "LowHealth") in >> lowHealth;
    else if (name == "MediumHealth") in >> mediumHealth;
    else if (name == "AlmostFullHealth") in >> almostFullHealth;
    else if (name == "LowMana") in >> lowMana;
    else if (name == "IterationsPerTick") in >> iterationsPerTick;
}

// ── DB-backed free-alt-bot load (stays C++ until Phase F provides FFI) ─

void PlayerbotAIConfig::loadFreeAltBotAccounts()
{
    bool allCharsOnline = (selfBotLevel == BotSelfBotLevel::ALWAYS_ACTIVE);

    freeAltBots.clear();

    auto results = LoginDatabase.PQuery(
        "SELECT username, id FROM account where username not like '%s%%'",
        randomBotAccountPrefix.c_str());
    if (!results)
        return;

    do
    {
        bool accountToggle = false;

        Field* fields = results->Fetch();
        std::string accountName = fields[0].GetString();
        uint32 accountId = fields[1].GetUInt32();

        if (std::find(toggleAlwaysOnlineAccounts.begin(), toggleAlwaysOnlineAccounts.end(), accountName)
            != toggleAlwaysOnlineAccounts.end())
            accountToggle = true;

        auto result = CharacterDatabase.PQuery(
            "SELECT name, guid FROM characters WHERE account = '%u'", accountId);
        if (!result)
            continue;

        do
        {
            bool charToggle = false;

            Field* rf = result->Fetch();
            std::string charName = rf[0].GetString();
            uint32 guid = rf[1].GetUInt32();

            BotAlwaysOnline always = BotAlwaysOnline(sRandomPlayerbotMgr.GetValue(guid, "always"));

            if (always == BotAlwaysOnline::DISABLED_BY_COMMAND)
                continue;

            if (std::find(toggleAlwaysOnlineChars.begin(), toggleAlwaysOnlineChars.end(), charName)
                != toggleAlwaysOnlineChars.end())
                charToggle = true;

            bool thisCharAlwaysOnline = allCharsOnline;
            if (accountToggle || charToggle)
                thisCharAlwaysOnline = !thisCharAlwaysOnline;

            if ((thisCharAlwaysOnline && always != BotAlwaysOnline::DISABLED_BY_COMMAND)
                || always == BotAlwaysOnline::ACTIVE)
            {
                sLog.outString("Enabling always online for %s", charName.c_str());
                freeAltBots.push_back(std::make_pair(accountId, guid));
            }
        } while (result->NextRow());
    } while (results->NextRow());
}

// ── Logging (verbatim port — stays C++ until Phase K moves sinks) ───────

std::string PlayerbotAIConfig::GetTimestampStr()
{
    time_t t = time(nullptr);
    tm* aTm = localtime(&t);
    char buf[20];
    std::strftime(buf, sizeof(buf), "%Y-%m-%d %H:%M:%S", aTm);
    return std::string(buf);
}

bool PlayerbotAIConfig::openLog(std::string fileName, char const* mode, bool haslog)
{
    if (!haslog && !hasLog(fileName))
        return false;

    auto logFileIt = logFiles.find(fileName);
    if (logFileIt == logFiles.end())
    {
        logFiles.insert(std::make_pair(fileName, std::make_pair(nullptr, false)));
        logFileIt = logFiles.find(fileName);
    }

    FILE* file = logFileIt->second.first;
    bool fileOpen = logFileIt->second.second;

    if (fileOpen)
        fclose(file);

    std::string m_logsDir = sConfig.GetStringDefault("LogsDir");
    if (!m_logsDir.empty())
    {
        char last = m_logsDir.back();
        if (last != '/' && last != '\\')
            m_logsDir.append("/");
    }

    file = fopen((m_logsDir + fileName).c_str(), mode);
    fileOpen = true;

    logFileIt->second.first = file;
    logFileIt->second.second = fileOpen;

    return true;
}

void PlayerbotAIConfig::log(std::string fileName, const char* str, ...)
{
    if (!str)
        return;

    std::lock_guard<std::mutex> guard(m_logMtx);

    if (!isLogOpen(fileName))
        if (!openLog(fileName, "a"))
            return;

    FILE* file = logFiles.find(fileName)->second.first;

    va_list ap;
    va_start(ap, str);
    vfprintf(file, str, ap);
    fprintf(file, "\n");
    va_end(ap);
    fflush(file);

    fflush(stdout);
}

void PlayerbotAIConfig::logEvent(PlayerbotAI* /*ai*/, std::string /*eventName*/,
                                  std::string /*info1*/, std::string /*info2*/)
{
    // Stubbed — event logging will be re-implemented in Rust.
}

void PlayerbotAIConfig::logEvent(PlayerbotAI* ai, std::string eventName,
                                  ObjectGuid /*guid*/, std::string info2)
{
    logEvent(ai, eventName, std::string(""), info2);
}

bool PlayerbotAIConfig::CanLogAction(PlayerbotAI* /*ai*/, std::string actionName,
                                      bool /*isExecute*/, std::string /*lastActionName*/)
{
    return std::find(debugFilter.begin(), debugFilter.end(), actionName) == debugFilter.end();
}
