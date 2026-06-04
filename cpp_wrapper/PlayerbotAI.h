#pragma once
// PlayerbotAI — trivial subclass of PlayerbotRust for backwards compatibility.
// The core's Player.h forward-declares `class PlayerbotAI;` and code casts
// GetPlayerbotAI() to it.

#include "PlayerbotRust.h"

class PlayerbotAI : public PlayerbotRust
{
public:
    using PlayerbotRust::PlayerbotRust;
};
