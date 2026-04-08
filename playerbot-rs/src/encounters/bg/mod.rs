pub mod alterac;
pub mod arathi;
/// Battleground encounter modules.
///
/// Each BG has its own file owning the node/flag/base logic.
/// BG "encounters" are not boss FSMs �� they track the strategic state
/// of the battleground (flag positions, base ownership, timers) and
/// provide BT overrides for flag-carrying, base defense, etc.
///
/// The BT leaves (`DefendBase`, `CaptureFlag`, `ReturnFlag`,
/// `AssaultBase`, `QueueBg`, `AcceptBgInvite`) are implemented in
/// `engine/bt.rs` via FFI callbacks (`get_bg_objective_pos`,
/// `capture_bg_objective`, `queue_bg`, `accept_bg_invite`).
pub mod warsong;
