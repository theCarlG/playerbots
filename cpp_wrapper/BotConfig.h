#pragma once

/**
 * cpp_wrapper/BotConfig.h — Phase D compatibility shim.
 *
 * Rust (crate `playerbot`, module `config`) now owns the `.conf` parser.
 * This file is a thin data mirror whose `PlayerbotAIConfig::Initialize()`
 * implementation (see `BotConfig.cpp`) calls `playerbot_config_load()` once
 * at startup and then pulls every field over the FFI via strongly-typed
 * accessors declared in `cpp_wrapper/botffi.h`.
 *
 * The symbol names (`class PlayerbotAIConfig`, `sPlayerbotAIConfig`) are
 * preserved verbatim so the 9 consumer files scheduled for port in phases
 * E–J only need their `#include` path updated. This shim disappears as
 * those consumers move to Rust.
 *
 * DO NOT add new parser logic here. If a field needs a different shape,
 * add the derivation to the Rust `config::typed::BotConfig::from_raw`
 * function and expose it via a new FFI accessor.
 */

#include "Globals/SharedDefines.h"
#include "SystemConfig.h"

#include <cstdint>
#include <cstdio>
#include <list>
#include <map>
#include <mutex>
#include <set>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

class Player;
class PlayerbotAI;
class PlayerbotMgr;
class ChatHandler;

// ── Stub for deleted TalentSpec system — talent spec management lives in Rust.
struct ClassSpecs
{
    ClassSpecs() = default;
    ClassSpecs(uint32 /*classMask*/) {}

    struct TalentPath
    {
        uint32 id;
        std::string name;
        int probability;
        TalentPath(uint32 i, std::string n, int p)
            : id(i), name(std::move(n)), probability(p) {}
    };

    std::vector<TalentPath> talentPath;
};

#if PLATFORM == PLATFORM_WINDOWS
inline std::string _D_AIPLAYERBOT_CONFIG = "aiplayerbot.conf";
#else
inline std::string _D_AIPLAYERBOT_CONFIG = SYSCONFDIR "aiplayerbot.conf";
#endif

enum class BotCheatMask : uint32
{
    none        = 0,
    taxi        = 1 << 0,
    gold        = 1 << 1,
    health      = 1 << 2,
    mana        = 1 << 3,
    power       = 1 << 4,
    item        = 1 << 5,
    cooldown    = 1 << 6,
    repair      = 1 << 7,
    movespeed   = 1 << 8,
    attackspeed = 1 << 9,
    breath      = 1 << 10,
    glyph       = 1 << 11,
    quest       = 1 << 12,
    maxMask     = 1 << 13,
};

enum class BotAutoLogin : uint32
{
    DISABLED                 = 0,
    LOGIN_ALL_WITH_MASTER    = 1,
    LOGIN_ONLY_ALWAYS_ACTIVE = 2,
};

enum class BotSelfBotLevel : uint32
{
    DISABLED          = 0,
    GM_ONLY           = 1,
    ACTIVE_BY_COMMAND = 2,
    ALWAYS_ALLOWED    = 3,
    ACTIVE_BY_LOGIN   = 4,
    ALWAYS_ACTIVE     = 5,
};

enum class BotAlwaysOnline : uint32
{
    DISABLED            = 0,
    ACTIVE              = 1,
    DISABLED_BY_COMMAND = 2,
};

enum class BotLoginCriteriaType : uint8
{
    RACECLASS          = 0,
    LEVEL              = 1,
    RANGE_TO_PLAYER    = 2,
    MAX_LOGIN_CRITERIA = 3,
};

#define MAX_GEAR_PROGRESSION_LEVEL 6

struct ParsedUrl
{
    std::string hostname;
    std::string path;
    int  port = 0;
    bool https = false;
};

// GlyphPrioritySpecMap[specId][level] = {{glyphItemId, prereqTalentSpell}};
using GlyphPriority          = std::pair<uint32, uint32>;
using GlyphPriorityList      = std::vector<GlyphPriority>;
using GlyphPriorityLevelMap  = std::unordered_map<uint32, GlyphPriorityList>;
using GlyphPrioritySpecMap   = std::unordered_map<uint32, GlyphPriorityLevelMap>;

class PlayerbotAIConfig
{
public:
    PlayerbotAIConfig();

    static PlayerbotAIConfig& instance()
    {
        static PlayerbotAIConfig instance;
        return instance;
    }

    /**
     * Parses the AI Playerbot `.conf` via the Rust crate and pulls every
     * legacy field from the Rust singleton over the FFI. Returns false if
     * parsing fails or AI Playerbot is disabled in the config.
     *
     * Called from `PlayerbotMgr::LoadConfig()` / `RandomPlayerbotMgr::Init()`
     * at server startup, as it was before Phase D.
     */
    bool Initialize();

    // ── Predicate helpers (pure std::find wrappers over the mirrored lists) ──
    bool IsInRandomAccountList(uint32 id);
    bool IsFreeAltBot(uint32 guid);
    bool IsFreeAltBot(Player* player);
    bool IsInRandomQuestItemList(uint32 id);
    bool IsInPvpProhibitedZone(uint32 id);

    // ── Scalars (flags / delays / distances / thresholds) ────────────────────
    bool enabled = false;
    bool allowGuildBots = true;
    bool allowMultiAccountAltBots = true;

    uint32 globalCoolDown = 0, reactDelay = 0, maxWaitForMove = 0,
           expireActionTime = 0, dispelAuraDuration = 0, passiveDelay = 0,
           repeatDelay = 0, errorDelay = 0, rpgDelay = 0, sitDelay = 0,
           returnDelay = 0, lootDelay = 0;

    float sightDistance = 0.0f, spellDistance = 0.0f, reactDistance = 0.0f,
          grindDistance = 0.0f, lootDistance = 0.0f,
          groupMemberLootDistance = 0.0f, groupMemberLootDistanceWithActiveMaster = 0.0f,
          gatheringDistance = 0.0f, groupMemberGatheringDistance = 0.0f,
          groupMemberGatheringDistanceWithActiveMaster = 0.0f,
          shootDistance = 0.0f, fleeDistance = 0.0f, tooCloseDistance = 0.0f,
          meleeDistance = 0.0f, followDistance = 0.0f, raidFollowDistance = 0.0f,
          wanderMinDistance = 0.0f, wanderMaxDistance = 0.0f,
          whisperDistance = 0.0f, contactDistance = 0.0f, aoeRadius = 0.0f,
          rpgDistance = 0.0f, targetPosRecalcDistance = 0.0f, farDistance = 0.0f,
          healDistance = 0.0f, aggroDistance = 0.0f, proximityDistance = 0.0f,
          maxFreeMoveDistance = 0.0f, freeMoveDelay = 0.0f;

    uint32 criticalHealth = 0, lowHealth = 0, mediumHealth = 0, almostFullHealth = 0;
    uint32 lowMana = 0, mediumMana = 0;

    uint32 openGoSpell = 0;

    // ── Random-bot manager state ────────────────────────────────────────────
    bool randomBotAutologin = false;
    BotAutoLogin botAutologin = BotAutoLogin::DISABLED;
    std::string randomBotMapsAsString;
    std::vector<uint32> randomBotMaps;
    std::list<uint32> randomBotQuestItems;
    std::list<uint32> randomBotAccounts;
    std::list<uint32> randomBotSpellIds;
    std::list<uint32> randomBotQuestIds;
    std::list<uint32> immuneSpellIds;
    std::list<std::pair<uint32, uint32>> freeAltBots;
    std::list<std::string> toggleAlwaysOnlineAccounts;
    std::list<std::string> toggleAlwaysOnlineChars;

    bool enableRandomTeleports = false;
    bool enableMinimalMove = false;
    uint32 randomBotTeleportDistance = 0;
    bool randomBotTeleportNearPlayer = false;
    uint32 transportTeleportType = 0;
    uint32 randomBotTeleportNearPlayerMaxAmount = 0;
    float randomBotTeleportNearPlayerMaxAmountRadius = 0.0f;
    uint32 randomBotTeleportMinInterval = 0, randomBotTeleportMaxInterval = 0;

    uint32 randomGearMaxLevel = 0;
    uint32 randomGearMaxDiff = 0;
    bool randomGearUpgradeEnabled = false;
    bool randomGearTabards = false;
    bool randomGearTabardsReplaceGuild = false;
    bool randomGearTabardsUnobtainable = false;
    float randomGearTabardsChance = 0.0f;
    std::list<uint32> randomGearBlacklist;
    std::list<uint32> randomGearWhitelist;
    bool randomGearProgression = false;
    float randomGearLoweringChance = 0.0f;
    bool rollBadItemsWithPlayer = false;
    float randomBotMaxLevelChance = 0.0f;
    float randomBotRpgChance = 0.0f;
    float usePotionChance = 0.0f;
    float attackEmoteChance = 0.0f;
    bool randomBotAutoCreate = false;

    uint32 minRandomBots = 0, maxRandomBots = 0;
    uint32 randomBotUpdateInterval = 0, randomBotCountChangeMinInterval = 0,
           randomBotCountChangeMaxInterval = 0;
    bool randomBotTimedLogout = false, randomBotTimedOffline = false;
    uint32 minRandomBotInWorldTime = 0, maxRandomBotInWorldTime = 0;
    uint32 minRandomBotRandomizeTime = 0, maxRandomBotRandomizeTime = 0;
    uint32 minRandomBotChangeStrategyTime = 0, maxRandomBotChangeStrategyTime = 0;
    uint32 minRandomBotReviveTime = 0, maxRandomBotReviveTime = 0;
    uint32 minRandomBotPvpTime = 0, maxRandomBotPvpTime = 0;
    uint32 randomBotsMaxLoginsPerInterval = 0;
    uint32 randomBotsPerInterval = 0;
    uint32 minRandomBotsPriceChangeInterval = 0, maxRandomBotsPriceChangeInterval = 0;

    // ── Auction house settings ──────────────────────────────────────────────
    bool shouldQueryAHListingsOutsideOfAH = false;
    std::list<uint32> ahOverVendorItemIds;
    std::list<uint32> vendorOverAHItemIds;
    bool botCheckAllAuctionListings = false;
    bool botsSaveEpics = false;

    bool randomBotJoinLfg = false;
    bool logRandomBotJoinLfg = false;
    bool randomBotJoinBG = false;
    bool randomBotAutoJoinBG = false;
    uint32 randomBotBracketCount = 0;
    bool randomBotLoginAtStartup = false;
    uint32 randomBotTeleLevel = 0;
    bool logInGroupOnly = false, logValuesPerTick = false;
    bool fleeingEnabled = false;
    bool summonAtInnkeepersEnabled = false;

    std::string combatStrategies, nonCombatStrategies, reactStrategies, deadStrategies;
    std::string randomBotCombatStrategies, randomBotNonCombatStrategies,
                randomBotReactStrategies, randomBotDeadStrategies;

    uint32 randomBotMinLevel = 0, randomBotMaxLevel = 0;
    float randomChangeMultiplier = 0.0f;

    // ── Class/race tables (pre-derived by Rust) ─────────────────────────────
    uint32 specProbability[MAX_CLASSES][10] = {};
    std::string premadeLevelSpec[MAX_CLASSES][10][91] = {}; // level 10..100
    uint32 classRaceProbabilityTotal = 0;
    uint32 classRaceProbability[MAX_CLASSES][MAX_RACES] = {};
    bool useFixedClassRaceCounts = false;
    using ClassRacePair = std::pair<uint8, uint8>;
    std::map<ClassRacePair, uint32> fixedClassRaceCounts;
    uint32 levelProbability[DEFAULT_MAX_LEVEL + 1] = {};
    ClassSpecs classSpecs[MAX_CLASSES];
    GlyphPrioritySpecMap glyphPriorityMap[MAX_CLASSES];

    // ── Gear progression ────────────────────────────────────────────────────
    bool gearProgressionSystemEnabled = false;
    uint32 gearProgressionSystemItemLevels[MAX_GEAR_PROGRESSION_LEVEL][2] = {};
    int32 gearProgressionSystemItems[MAX_GEAR_PROGRESSION_LEVEL][MAX_CLASSES][4][SLOT_EMPTY] = {};

    std::string commandPrefix, commandSeparator;
    std::string randomBotAccountPrefix;
    uint32 randomBotAccountCount = 0;
    bool deleteRandomBotAccounts = false;
    uint32 randomBotGuildCount = 0;
    bool deleteRandomBotGuilds = false;
    uint32 randomBotArenaTeamCount = 0;
    bool deleteRandomBotArenaTeams = false;
    std::list<uint32> randomBotArenaTeams;

    bool RandombotsWalkingRPG = false;
    bool RandombotsWalkingRPGInDoors = false;
    bool boostFollow = false;
    bool turnInRpg = false;
    bool globalSoundEffects = false;
    bool shareTargets = false;
    std::list<uint32> randomBotGuilds;
    std::list<uint32> pvpProhibitedZoneIds;

    bool enableGreet = false;
    bool randomBotShowHelmet = false;
    bool randomBotShowCloak = false;
    bool disableRandomLevels = false;
    bool instantRandomize = false;
    bool gearscorecheck = false;
    int32 levelCheck = 0;
    bool randomBotPreQuests = false;
    float playerbotsXPrate = 1.0f;
    bool disableBotOptimizations = false;
    bool disableActivityPriorities = false;
    bool forceActiveWhenNearPlayer = false;
    bool limitCombatActivity = false;
    bool guildOrderAlwaysActive = false;
    uint32 botActiveAlone = 0;
    uint32 diffWithPlayer = 0;
    uint32 diffEmpty = 0;
    uint32 minEnchantingBotLevel = 0;
    uint32 randombotStartingLevel = 0;
    bool randomBotSayWithoutMaster = false;
    bool randomBotInvitePlayer = false;
    bool randomBotGroupNearby = false;
    bool randomBotRaidNearby = false;
    bool randomBotGuildNearby = false;
    bool randomBotFormGuild = false;
    bool randomBotRandomPassword = false;
    bool inviteChat = false;
    bool enableOffSpecStrategies = false;
    bool useWanderAsDefaultFollowStrategy = false;
    std::string defaultFormation;

    uint32 guildMaxBotLimit = 0;

    // ── Broadcast chances ───────────────────────────────────────────────────
    bool enableBroadcasts = false;
    uint32 broadcastChanceMaxValue = 0;
    uint32 broadcastToGuildGlobalChance = 0;
    uint32 broadcastToWorldGlobalChance = 0;
    uint32 broadcastToGeneralGlobalChance = 0;
    uint32 broadcastToTradeGlobalChance = 0;
    uint32 broadcastToLFGGlobalChance = 0;
    uint32 broadcastToLocalDefenseGlobalChance = 0;
    uint32 broadcastToWorldDefenseGlobalChance = 0;
    uint32 broadcastToGuildRecruitmentGlobalChance = 0;
    uint32 broadcastToSayGlobalChance = 0;
    uint32 broadcastToYellGlobalChance = 0;

    uint32 broadcastChanceLootingItemPoor = 0;
    uint32 broadcastChanceLootingItemNormal = 0;
    uint32 broadcastChanceLootingItemUncommon = 0;
    uint32 broadcastChanceLootingItemRare = 0;
    uint32 broadcastChanceLootingItemEpic = 0;
    uint32 broadcastChanceLootingItemLegendary = 0;
    uint32 broadcastChanceLootingItemArtifact = 0;

    uint32 broadcastChanceQuestAccepted = 0;
    uint32 broadcastChanceQuestUpdateObjectiveCompleted = 0;
    uint32 broadcastChanceQuestUpdateObjectiveProgress = 0;
    uint32 broadcastChanceQuestUpdateFailedTimer = 0;
    uint32 broadcastChanceQuestUpdateComplete = 0;
    uint32 broadcastChanceQuestTurnedIn = 0;

    uint32 broadcastChanceKillNormal = 0;
    uint32 broadcastChanceKillElite = 0;
    uint32 broadcastChanceKillRareelite = 0;
    uint32 broadcastChanceKillWorldboss = 0;
    uint32 broadcastChanceKillRare = 0;
    uint32 broadcastChanceKillUnknown = 0;
    uint32 broadcastChanceKillPet = 0;
    uint32 broadcastChanceKillPlayer = 0;

    uint32 broadcastChanceLevelupGeneric = 0;
    uint32 broadcastChanceLevelupTenX = 0;
    uint32 broadcastChanceLevelupMaxLevel = 0;

    uint32 broadcastChanceSuggestInstance = 0;
    uint32 broadcastChanceSuggestQuest = 0;
    uint32 broadcastChanceSuggestGrindMaterials = 0;
    uint32 broadcastChanceSuggestGrindReputation = 0;
    uint32 broadcastChanceSuggestSell = 0;
    uint32 broadcastChanceSuggestSomething = 0;

    uint32 broadcastChanceSuggestSomethingToxic = 0;

    uint32 broadcastChanceSuggestToxicLinks = 0;
    std::string toxicLinksPrefix;
    uint32 toxicLinksRepliesChance = 0;

    uint32 broadcastChanceSuggestThunderfury = 0;
    uint32 thunderfuryRepliesChance = 0;

    uint32 broadcastChanceGuildManagement = 0;

    uint32 guildRepliesRate = 0;

    uint32 botAcceptDuelMinimumLevel = 0;

    bool talentsInPublicNote = false;
    bool nonGmFreeSummon = false;

    BotSelfBotLevel selfBotLevel = BotSelfBotLevel::GM_ONLY;
    uint32 iterationsPerTick = 0;

    std::string autoPickReward;
    bool autoEquipUpgradeLoot = false;
    bool syncQuestWithPlayer = false;
    bool syncQuestForPlayer = false;
    std::string autoTrainSpells;
    std::string autoPickTalents;
    bool autoLearnTrainerSpells = false;
    bool autoLearnQuestSpells = false;
    bool autoLearnDroppedSpells = false;
    bool autoDoQuests = false;
    bool syncLevelWithPlayers = false;
    uint32 syncLevelMaxAbove = 0, syncLevelNoPlayer = 0;
    uint32 tweakValue = 0;
    float respawnModNeutral = 0.0f, respawnModHostile = 0.0f;
    uint32 respawnModThreshold = 0, respawnModMax = 0;
    bool respawnModForPlayerBots = false, respawnModForInstances = false;

    bool randomBotLoginWithPlayer = false;
    bool asyncBotLogin = false, preloadHolders = false;
    uint32 freeRoomForNonSpareBots = 0;
    uint32 loginBotsNearPlayerRange = 0;
    std::vector<std::string> defaultLoginCriteria;
    std::vector<std::vector<std::string>> loginCriteria;

    bool jumpInBg = false;
    bool jumpWithPlayer = false;
    bool jumpFollow = false;
    bool jumpChase = false;
    bool useKnockback = false;
    float jumpNoCombatChance = 0.0f;
    float jumpMeleeInCombatChance = 0.0f;
    float jumpRandomChance = 0.0f;
    float jumpInPlaceChance = 0.0f;
    float jumpBackwardChance = 0.0f;
    float jumpHeightLimit = 0.0f;
    float jumpVSpeed = 0.0f;
    float jumpHSpeed = 0.0f;

    // ── Logging / debug (stays C++ until Phase K) ───────────────────────────
    std::mutex m_logMtx;

    std::list<std::string> allowedLogFiles;
    std::list<std::string> debugFilter;

    std::unordered_map<std::string, std::pair<FILE*, bool>> logFiles;

    uint32 botCheatMask = 0;
    uint32 rndBotCheatMask = 0;

    std::vector<std::string> BotCheatMaskName = {
        "taxi", "gold", "health", "mana", "power", "item", "cooldown",
        "repair", "movespeed", "attackspeed", "breath", "glyph", "quest",
        "maxMask",
    };

    struct worldBuff
    {
        uint32 spellId;
        uint32 factionId = 0;
        uint32 classId   = 0;
        uint32 specId    = 0;
        uint32 minLevel  = 0;
        uint32 maxLevel  = 0;
        uint32 eventId   = 0;
    };

    std::vector<worldBuff> worldBuffs;

    int  commandServerPort = 0;
    bool perfMonEnabled = false;
    bool bExplicitDbStoreSave = false;

    // ── LLM ────────────────────────────────────────────────────────────────
    std::string llmApiEndpoint, llmApiKey, llmApiJson, llmPrePrompt, llmPreRpgPrompt,
                llmPrompt, llmPostPrompt, llmResponseStartPattern, llmResponseEndPattern,
                llmResponseDeletePattern, llmResponseSplitPattern;
    uint32 llmEnabled = 0, llmContextLength = 0, llmBotToBotChatChance = 0,
           llmGenerationTimeout = 0, llmMaxSimultaniousGenerations = 0,
           llmRpgAIChatChance = 0;
    bool llmGlobalContext = false;
    ParsedUrl llmEndPointUrl;
    std::set<uint32> llmBlockedReplyChannels;

    uint32 EatDrinkMinDistance = 5;
    uint32 EatDrinkMaxDistance = 1000;

    // ── Debug-channel dispatch (stays C++ — small, local to shim) ───────────
    std::string GetValue(std::string name);
    void SetValue(std::string name, std::string value);

    // ── DB-backed load (stays C++ — Phase F will port DB access) ───────────
    void loadFreeAltBotAccounts();

    // ── File logging (stays C++ — Phase K moves the log sinks to Rust) ──────
    std::string GetTimestampStr();

    bool hasLog(std::string fileName)
    {
        return std::find(allowedLogFiles.begin(), allowedLogFiles.end(), fileName)
               != allowedLogFiles.end();
    }
    bool openLog(std::string fileName, char const* mode = "a", bool haslog = false);
    bool isLogOpen(std::string fileName)
    {
        auto it = logFiles.find(fileName);
        return it != logFiles.end() && it->second.second;
    }
    void log(std::string fileName, const char* str, ...);

    void logEvent(PlayerbotAI* ai, std::string eventName,
                  std::string info1 = "", std::string info2 = "");
    void logEvent(PlayerbotAI* ai, std::string eventName,
                  ObjectGuid guid, std::string info2);

    bool CanLogAction(PlayerbotAI* ai, std::string actionName,
                      bool isExecute, std::string lastActionName);
};

#define sPlayerbotAIConfig MaNGOS::Singleton<PlayerbotAIConfig>::Instance()
