#pragma once
// Redirect — real definition lives in cpp_wrapper/BotConfig.h.
// Kept because the mangos-classic core includes "playerbot/PlayerbotAIConfig.h"
// from CharacterHandler.cpp, Player.cpp, World.cpp and Chat.cpp. Providing this
// shim keeps the core's includes identical to the original PB2 bots so the
// module stays drop-in swappable without modifying the core.
#include "../cpp_wrapper/BotConfig.h"
