/// Battleground encounter modules.
///
/// Each BG has its own file owning the node/flag/base logic.
/// BG "encounters" are not boss FSMs — they track the strategic state
/// of the battleground (flag positions, base ownership, timers) and
/// provide BT overrides for flag-carrying, base defense, etc.
///
/// Currently scaffolded as stubs — the BT leaves (`DefendBase`,
/// `CaptureFlag`, `ReturnFlag`, `AssaultBase`, `QueueBg`,
/// `AcceptBgInvite`) exist in `engine/bt.rs` but return `Failure`.
/// These modules will be fleshed out when BG FFI and map-awareness
/// primitives land.
pub mod warsong;
pub mod arathi;
pub mod alterac;
