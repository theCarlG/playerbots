// Precompiled header for playerbots module.
// Includes CMaNGOS core headers used across multiple files.
#pragma once

#include "Server/WorldSocket.h"
#include "Common.h"
#include "Maps/MapManager.h"
#include "Log/Log.h"
#include "Globals/ObjectAccessor.h"
#include "Entities/ObjectGuid.h"
#include "Server/SQLStorages.h"
#include "Server/Opcodes.h"
#include "Globals/SharedDefines.h"
#include "Guilds/GuildMgr.h"
#include "Globals/ObjectMgr.h"
#include "DBScripts/ScriptMgr.h"
#include "Spells/Spell.h"
#include "Spells/SpellMgr.h"
#include "Spells/SpellAuras.h"
#include "Server/WorldPacket.h"
#include "Loot/LootMgr.h"
#include "Entities/GossipDef.h"
#include "Chat/Chat.h"
#include "World/World.h"
#include "Entities/Unit.h"
#include "MotionGenerators/MotionMaster.h"
#include "Guilds/Guild.h"
#include "Entities/Player.h"
#include "Groups/Group.h"
#include "Database/DatabaseEnv.h"
#include "Grids/GridNotifiers.h"
#include "Grids/GridNotifiersImpl.h"
#include "Grids/CellImpl.h"

#include <algorithm>
#include <functional>
#include <memory>
#include <numeric>
#include <stack>
#include <regex>

#include "playerbot/playerbot.h"
