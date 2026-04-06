/// World buff locations — coordinates for collecting world buffs.
///
/// PB2: `WBuffStrategy` sends bots to collect Onyxia/Nefarian head turn-in
/// buffs, Dire Maul tribute, Songflower, etc. Each buff has one or more
/// collection points (the NPC or object location).
use crate::ffi::SpellId;
use crate::travel::destination::{TravelDestination, TravelKind, TravelPurpose};

/// Known world buff locations. Each entry is a (spell_id, map, x, y, z).
/// Coordinates are approximate (near the NPC / object that grants the buff).
const WORLD_BUFF_LOCATIONS: &[(u32, u32, f32, f32, f32)] = &[
    // Rallying Cry of the Dragonslayer (Onyxia head, Stormwind)
    (22888, 0, -8514.0, 339.0, 121.0),
    // Rallying Cry of the Dragonslayer (Onyxia head, Orgrimmar)
    (22888, 1, 1556.0, -4420.0, 8.0),
    // Warchief's Blessing (Nefarian head, Orgrimmar)
    (16609, 1, 1556.0, -4420.0, 8.0),
    // Rallying Cry of the Dragonslayer (Nefarian head, Stormwind)
    (22888, 0, -8514.0, 339.0, 121.0),
    // Spirit of Zandalar (Heart of Hakkar, Booty Bay)
    (24425, 0, -14373.0, 441.0, 22.0),
    // Songflower Serenade (Felwood)
    (15366, 1, 6418.0, -1431.0, 437.0),
    // Dire Maul Tribute (Dire Maul North)
    (22818, 429, 194.0, -61.0, -7.0),
    // Sayge's Dark Fortune (Darkmoon Faire, Elwynn)
    (23768, 0, -9552.0, 107.0, 58.0),
];

/// Find the nearest world buff location for a given spell on a given map.
pub fn find_world_buff_location(buff_id: SpellId, map: u32) -> Option<TravelDestination> {
    WORLD_BUFF_LOCATIONS
        .iter()
        .find(|(spell, m, _, _, _)| *spell == buff_id.0 && *m == map)
        .map(|(_, m, x, y, z)| {
            TravelDestination::new(TravelKind::WorldBuff(buff_id), TravelPurpose::NONE, *m, *x, *y, *z)
        })
}

/// Find any world buff location for the spell (cross-map).
pub fn find_world_buff_any_map(buff_id: SpellId) -> Option<TravelDestination> {
    WORLD_BUFF_LOCATIONS
        .iter()
        .find(|(spell, _, _, _, _)| *spell == buff_id.0)
        .map(|(_, m, x, y, z)| {
            TravelDestination::new(TravelKind::WorldBuff(buff_id), TravelPurpose::NONE, *m, *x, *y, *z)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_onyxia_buff_stormwind() {
        let dest = find_world_buff_location(SpellId(22888), 0);
        assert!(dest.is_some());
        let d = dest.unwrap();
        assert_eq!(d.map, 0);
        assert_eq!(d.kind, TravelKind::WorldBuff(SpellId(22888)));
    }

    #[test]
    fn find_unknown_buff_returns_none() {
        assert!(find_world_buff_location(SpellId(99999), 0).is_none());
    }

    #[test]
    fn find_any_map_returns_first_match() {
        let dest = find_world_buff_any_map(SpellId(22888));
        assert!(dest.is_some());
    }
}
