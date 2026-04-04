// PlayerbotAI.cpp — stub.
//
// The old PlayerbotAI implementation has been removed.
// The AI is now provided by the Rust module via PlayerbotRust
// (cpp_wrapper/PlayerbotRust.cpp).
//
// This file intentionally left empty.  Remove this file along with
// PlayerbotAI.h once all callers have been updated to PlayerbotRust.

#include "PlayerbotAI.h"

bool IsAlliance(uint8 race) {
    return race == RACE_HUMAN || race == RACE_DWARF || race == RACE_NIGHTELF
        || race == RACE_GNOME
#ifdef MANGOSBOT_ONE
        || race == RACE_DRAENEI
#endif
#ifdef MANGOSBOT_TWO
        || race == RACE_DRAENEI
#endif
        ;
}
