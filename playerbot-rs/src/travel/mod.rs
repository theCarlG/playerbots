/// Travel subsystem — destination selection, pathfinding, and navigation.
///
/// PB2: `TravelMgr` + `TravelDestination` + `TravelNode` — handles long-distance
/// bot movement including flight paths, boats, and zone transitions.
///
/// The Rust port layers this on top of the existing blackboard-based
/// `TravelToBlackboard` BT leaf. The planner writes coordinates to the
/// blackboard, and the existing travel leaf handles the actual movement.
pub mod destination;
pub mod graph;
pub mod planner;
pub mod target;
pub mod world_buff;
