/// Looting — approach and loot nearby corpses.
///
/// Phase 4 stub.  The actual loot interaction requires a CMaNGOS-side API
/// (opening a loot window, iterating items, confirming rolls) that isn't yet
/// exposed in BotCallbacks.  This module provides the tree node that will be
/// filled in during Phase 5/6 once the FFI surface is extended.
///
/// For now the subtree always returns Failure so it falls through to follow.
use crate::engine::bt_nodes::{BtNode, BtResult, action};

/// Build the looting subtree.
pub fn build_loot_subtree() -> Box<dyn BtNode> {
    // TODO(phase5): implement loot scan + approach + open loot window
    // when `get_nearby_lootable_objects` is added to BotCallbacks.
    action(|_ctx| BtResult::Failure)
}
