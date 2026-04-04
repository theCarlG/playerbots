#pragma once

#include <string>
#include <vector>

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

    bool isBg() const { return false; }
    bool isArena() const { return false; }
    bool isOverworld() const { return true; }
};

/// Minimal Qualified stub — replaces deleted Qualified helper used by
/// legacy command parsing. Provides just the static helpers referenced by
/// the stripped management layer.
struct Qualified {
    static std::vector<std::string> getMultiQualifiers(const std::string& text,
                                                       const std::string& sep) {
        std::vector<std::string> out;
        size_t start = 0;
        while (start <= text.size()) {
            size_t pos = text.find(sep, start);
            if (pos == std::string::npos) {
                out.push_back(text.substr(start));
                break;
            }
            out.push_back(text.substr(start, pos - start));
            start = pos + sep.size();
        }
        return out;
    }
    static bool isValidNumberString(const std::string& s) {
        if (s.empty()) return false;
        for (char c : s) if (c < '0' || c > '9') return false;
        return true;
    }
};

/// Minimal ChatHelper stub — replaces deleted ChatHelper class.
struct ChatHelper {
    ChatHelper() = default;
    explicit ChatHelper(void* /*ai*/) {}
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
