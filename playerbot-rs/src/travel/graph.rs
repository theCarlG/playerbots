/// Travel graph — pathfinding between zones/maps.
///
/// PB2's `TravelNodeMap` maintains a precomputed graph of flight paths, portals,
/// zone connections, and boats/zeppelins. This module provides the same
/// abstraction. For now it operates as a simple same-map distance check;
/// cross-map pathing (flight paths, boats) will be added incrementally.
///
/// **Context for Part 5.8+:** PB2's TravelNode.cpp (3500 lines) implements
/// A* routing over a DB-stored node graph with walk/portal/transport/flight
/// path types. The Rust port will add `TravelNodeMap` when encounter/raid
/// travel is implemented. The existing same-map check is sufficient for
/// the destination selection logic in Part 5.6.
use crate::travel::destination::TravelDestination;

/// A travel graph node — a location that connects to other nodes.
#[derive(Debug, Clone, Copy)]
pub struct TravelNode {
    pub map: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Check if a destination is reachable from the current position.
/// For now: same-map = reachable, cross-map = unreachable.
pub fn is_reachable(current_map: u32, dest: &TravelDestination) -> bool {
    current_map == dest.map
}

/// Estimate travel time in seconds (crude: distance / run speed).
/// Average run speed is ~7 yards/sec, mounted ~14 yards/sec.
pub fn estimate_travel_time(
    from_x: f32,
    from_y: f32,
    dest: &TravelDestination,
    mounted: bool,
) -> f32 {
    let dist = dest.dist_sq_2d(from_x, from_y).sqrt();
    let speed = if mounted { 14.0 } else { 7.0 };
    dist / speed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::travel::destination::{TravelDestination, TravelKind, TravelPurpose};

    #[test]
    fn same_map_is_reachable() {
        let dest = TravelDestination::new(
            TravelKind::NamedLocation,
            TravelPurpose::NONE,
            0,
            100.0,
            200.0,
            0.0,
        );
        assert!(is_reachable(0, &dest));
    }

    #[test]
    fn cross_map_not_reachable() {
        let dest = TravelDestination::new(
            TravelKind::NamedLocation,
            TravelPurpose::NONE,
            1,
            100.0,
            200.0,
            0.0,
        );
        assert!(!is_reachable(0, &dest));
    }

    #[test]
    fn travel_time_estimate() {
        let dest = TravelDestination::new(
            TravelKind::NamedLocation,
            TravelPurpose::NONE,
            0,
            70.0,
            0.0,
            0.0,
        );
        let time = estimate_travel_time(0.0, 0.0, &dest, false);
        assert!((time - 10.0).abs() < 0.1); // 70 / 7 = 10
    }
}
