#pragma once

#define CAST_ANGLE_IN_FRONT (2 * M_PI_F / 3)
#define EMOTE_ANGLE_IN_FRONT (2 * M_PI_F / 6)

// Stub for deleted bot text system
#define BOT_TEXT(x) std::string(x)

/// Minimal WorldPosition stub — replaces deleted WorldPosition class.
/// Only provides storage and distance calculation for login-manager use.
struct WorldPosition {
    float x = 0, y = 0, z = 0, o = 0;
    uint32 mapid = 0;

    WorldPosition() = default;
    WorldPosition(float x_, float y_, float z_, float o_, float map)
        : x(x_), y(y_), z(z_), o(o_), mapid(static_cast<uint32>(map)) {}
    WorldPosition(Player* p);  // defined in .cpp
    WorldPosition(const WorldPosition& other) = default;

    float sqDistance(const WorldPosition& other) const {
        float dx = x - other.x, dy = y - other.y, dz = z - other.z;
        return dx*dx + dy*dy + dz*dz;
    }

    static void unloadMapAndVMaps(uint32 /*mapId*/) {}  // stub
};

/// Minimal ChatHelper stub — replaces deleted ChatHelper class.
struct ChatHelper {
    static std::string formatRace(uint8 race) { return "Race" + std::to_string(race); }
    static std::string formatClass(uint8 cls) { return "Class" + std::to_string(cls); }
    static std::string formatMoney(uint32 copper) {
        return std::to_string(copper / 10000) + "g " + std::to_string((copper / 100) % 100) + "s";
    }
    template<typename T>
    static std::string formatItem(T /*item*/, uint32 count = 1) {
        return "[item]x" + std::to_string(count);
    }
};
