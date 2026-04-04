/// World behaviors — everything bots do outside of combat rotations.
///
/// Each module exports a function returning a `Bt` enum value.
/// These are composed into the mode-aware root BT in `bot::init`.
pub mod bg;
pub mod death;
pub mod gather;
pub mod grind;
pub mod guard;
pub mod loot;
pub mod mount;
pub mod pet;
pub mod quest;
pub mod repair;
pub mod rpg;
pub mod stay;
pub mod travel;
pub mod vendor;
