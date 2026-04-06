/// Cross-class strategy builders.
///
/// Each sub-module exports `pub fn build() -> Bt` (or a small set of
/// parameterized builders) that assembles a BT subtree from the shared
/// `Bt` variants in `engine/bt.rs`. Class-specific behavior is NEVER
/// placed here — it lives in `classes/<class>/<spec>.rs` and is composed
/// with these cross-class strategies by `bot/init.rs`.
///
/// Strategies are referenced by name in `StrategyFlags` and enabled/
/// disabled via chat commands (`@nc=+pull`, `@co=-kite`, etc.).
pub mod behind;
pub mod cc;
pub mod close;
pub mod consumables;
pub mod guard;
pub mod kite;
pub mod loot;
pub mod maintenance;
pub mod mark_rti;
pub mod mount;
pub mod pull;
pub mod racials;
pub mod ranged;
pub mod rpg;
pub mod stay;
pub mod travel;
pub mod wander;
pub mod wbuff;
