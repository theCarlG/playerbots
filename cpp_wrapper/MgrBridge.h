#ifndef _PLAYERBOTMGR_H
#define _PLAYERBOTMGR_H

#include "Common.h"
#include "PlayerbotAIBase.h"
#include "Entities/ObjectGuid.h"
#include "Database/DatabaseEnv.h"
#include "Globals/SharedDefines.h"


class WorldPacket;
class Player;
class Unit;
class Object;
class Item;

typedef std::map<uint32, Player*> PlayerBotMap;

class PlayerbotHolder : public PlayerbotAIBase
{
public:
    PlayerbotHolder();
    virtual ~PlayerbotHolder();

    void AddPlayerBot(uint32 guid, uint32 masterAccountId);
	void HandlePlayerBotLoginCallback(QueryResult * dummy, SqlQueryHolder * holder);

    void LogoutPlayerBot(uint32 guid);
    void DisablePlayerBot(uint32 guid, bool logOutPlayer = true);
    Player* GetPlayerBot (uint32 guid) const;

    virtual void UpdateAIInternal(uint32 elapsed, bool minimal = false) override;
    void UpdateSessions(uint32 elapsed);

    void ForEachPlayerbot(std::function<void(Player*)> fct) const;

    void LogoutAllBots();
    void JoinChatChannels(Player* bot);
    void OnBotLogin(Player* bot);
    virtual void MovePlayerBot(uint32 guid, PlayerbotHolder* newHolder);

    std::list<std::string> HandlePlayerbotCommand(const std::string args, Player* master = NULL, AccountTypes security = SEC_PLAYER);
    std::string ProcessBotCommand(const std::string cmd, ObjectGuid guid, ObjectGuid masterguid, bool admin, uint32 masterAccountId, uint32 masterGuildId, const std::string param = "");
    uint32 GetAccountId(std::string name);
    std::string ListBots(Player* master, const std::string param);
    PlayerBotMap& GetAllBots() { return playerBots; }
    uint32 GetPlayerbotsAmount() const;

    static std::string GetCommandTexts(const std::string& command);
    static std::unordered_map<std::string, std::string> GetCommandTexts();
protected:
    virtual void OnBotLoginInternal(Player * const bot) = 0;
    void Cleanup();
private:
    typedef std::list<std::string> (PlayerbotHolder::*HolderCommandHandler)(Player* master, const std::string param, AccountTypes security);
    typedef std::string (PlayerbotHolder::*BotCommandHandler)(Player* bot, Player* master, const std::string param);

    ObjectGuid GetSpoofGuid() const { return m_spoofGuid; }


    std::list<std::string> HandleList(Player* master, const std::string param, AccountTypes security);
    std::list<std::string> HandleHelp(Player* master, const std::string param, AccountTypes security);
    std::list<std::string> HandleReload(Player* master, const std::string param, AccountTypes security);
    std::list<std::string> HandleTweak(Player* master, const std::string param, AccountTypes security);
    std::list<std::string> HandleSelf(Player* master, const std::string param, AccountTypes security);
    std::list<std::string> HandleSpoof(Player* master, const std::string param, AccountTypes security);
    std::list<std::string> HandleParty(Player* master, const std::string param, AccountTypes security);
    std::list<std::string> HandleGuild(Player* master, const std::string param, AccountTypes security);
    std::list<std::string> HandleRaid(Player* master, const std::string param, AccountTypes security);
    std::list<std::string> HandleRaidLeader(Player* master, const std::string param, AccountTypes security);


    std::string HandleBotAlways(Player* bot, Player* master, const std::string param);
    std::string HandleBotDebug(Player* bot, Player* master, const std::string param);
    std::string HandleBotMonitor(Player* bot, Player* master, const std::string param);
    std::string HandleBotC(Player* bot, Player* master, const std::string param);
    std::string HandleConsoleWhisper(Player* bot, Player* master, const std::string param);
    std::string HandleConsoleCmd(Player* bot, Player* master, const std::string param);
    std::string HandleBotDo(Player* bot, Player* master, const std::string param);
    std::string HandleBotRecord(Player* bot, Player* master, const std::string param);
    std::string HandleBotRead(Player* bot, Player* master, const std::string param);
    std::string HandleBotClear(Player* bot, Player* master, const std::string param);

    std::string HandleBotAddLogin(Player* bot, Player* master, const std::string param);
    std::string HandleBotRemoveLogout(Player* bot, Player* master, const std::string param);
    std::string HandleBotRefresh(Player* bot, Player* master, const std::string param);
    std::string HandleBotRandom(Player* bot, Player* master, const std::string param);

    PlayerBotMap playerBots;
    std::map<std::string, HolderCommandHandler> m_holderHandlers;
    std::map<std::string, BotCommandHandler> m_botCommandHandlers;
    ObjectGuid m_spoofGuid;
};

class PlayerbotMgr : public PlayerbotHolder
{
public:
    PlayerbotMgr(Player* const master);
    virtual ~PlayerbotMgr() override;

    static bool HandlePlayerbotMgrCommand(ChatHandler* handler, char const* args);
    void HandleMasterIncomingPacket(const WorldPacket& packet);
    void HandleMasterOutgoingPacket(const WorldPacket& packet);
    void HandleCommand(uint32 type, const std::string& text, uint32 lang = LANG_UNIVERSAL);
    void OnPlayerLogin(Player* player);
    void CancelLogout();

    virtual void UpdateAIInternal(uint32 elapsed, bool minimal = false) override;

    Player* GetMaster() const { return master; };

    void SaveToDB();

protected:
    virtual void OnBotLoginInternal(Player * const bot) override;

private:
    Player* const master;
};

#endif
