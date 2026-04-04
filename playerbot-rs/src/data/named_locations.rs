/// Hard-coded table of travel waypoints the bot can be commanded to reach.
///
/// Covers capitals, major inns, and a handful of levelling hubs. Map id is
/// included for future cross-map travel logic but today the travel subtree
/// only runs within the bot's current map.
///
/// Coordinates are approximate (inn door / capital square), not exact SQL.
/// A future expansion replaces this with flight-path pathing; for now the
/// bot `move_to`'s the destination once the map matches.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NamedLocation {
    pub name: &'static str,
    pub map: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Case-insensitive name lookup. Matches the first entry whose name equals
/// or is a prefix of the query.
pub fn lookup(name: &str) -> Option<&'static NamedLocation> {
    let needle = name.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return None;
    }
    LOCATIONS
        .iter()
        .find(|loc| loc.name.eq_ignore_ascii_case(&needle))
        .or_else(|| {
            LOCATIONS
                .iter()
                .find(|loc| loc.name.to_ascii_lowercase().starts_with(&needle))
        })
}

/// All known named locations. Indexed for O(1) iteration; order matters only
/// for prefix tie-breaks in `lookup`.
pub const LOCATIONS: &[NamedLocation] = &[
    // ── Eastern Kingdoms (map 0) ──────────────────────────────────────────
    NamedLocation {
        name: "stormwind",
        map: 0,
        x: -8833.4,
        y: 625.0,
        z: 94.0,
    },
    NamedLocation {
        name: "ironforge",
        map: 0,
        x: -4981.2,
        y: -881.5,
        z: 502.0,
    },
    NamedLocation {
        name: "undercity",
        map: 0,
        x: 1633.8,
        y: 240.2,
        z: -43.1,
    },
    NamedLocation {
        name: "goldshire",
        map: 0,
        x: -9464.8,
        y: 62.2,
        z: 56.0,
    },
    NamedLocation {
        name: "kharanos",
        map: 0,
        x: -5605.3,
        y: -479.4,
        z: 402.9,
    },
    NamedLocation {
        name: "brill",
        map: 0,
        x: 2266.8,
        y: 276.5,
        z: 34.8,
    },
    // ── Kalimdor (map 1) ──────────────────────────────────────────────────
    NamedLocation {
        name: "orgrimmar",
        map: 1,
        x: 1629.9,
        y: -4373.4,
        z: 31.3,
    },
    NamedLocation {
        name: "thunderbluff",
        map: 1,
        x: -1277.5,
        y: 122.8,
        z: 131.3,
    },
    NamedLocation {
        name: "darnassus",
        map: 1,
        x: 9947.5,
        y: 2482.7,
        z: 1316.2,
    },
    NamedLocation {
        name: "razorhill",
        map: 1,
        x: 326.3,
        y: -4664.3,
        z: 12.8,
    },
    NamedLocation {
        name: "bloodhoof",
        map: 1,
        x: -2357.0,
        y: -343.7,
        z: -8.9,
    },
    NamedLocation {
        name: "dolanaar",
        map: 1,
        x: 9851.4,
        y: 960.9,
        z: 1307.7,
    },
];
