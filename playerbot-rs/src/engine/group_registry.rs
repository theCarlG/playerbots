//! Global registry of shared `GroupState` instances.
//!
//! Every bot in the same group needs to see the same `GroupState` — that's
//! how the encounter coordinator (run by the leader bot) publishes role
//! assignments that every other bot reads. C++ PB2 achieved this by storing
//! a pointer on the `Group*` object; on the Rust side we don't have access
//! to the server's `Group` pointer across the FFI, so we key the registry
//! by a stable identifier derived from the group membership itself: the
//! **minimum non-zero member GUID**. Every member of a group computes the
//! same key regardless of membership order, so each bot that ticks will
//! resolve the same `GroupHandle`.
//!
//! ## RAII
//!
//! `acquire(key)` returns a `GroupHandle` that owns an `Arc<RwLock<GroupState>>`.
//! When the last `GroupHandle` for a key is dropped, its `Drop` impl removes
//! the matching entry from the registry map under the same lock that `acquire`
//! uses — no lazy housekeeping, no leaked map slots.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, RwLock, Weak};

use crate::engine::group_state::GroupState;

type Registry = Mutex<HashMap<u64, Weak<RwLock<GroupState>>>>;

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// RAII handle for a shared `GroupState`.
///
/// Holds a strong reference to the underlying `Arc<RwLock<GroupState>>` so
/// the state stays alive for as long as any bot in the group carries a
/// handle. When the *last* handle for a given key is dropped, the `Drop`
/// impl removes the entry from the registry — exact, prompt cleanup.
///
/// Clone-able: dropping a clone doesn't remove the registry entry as long
/// as another clone (or the original) still exists. The registry cleanup
/// only fires when `Arc::strong_count` drops to 1 inside `Drop`, meaning
/// this handle is the *only* remaining strong reference.
#[derive(Debug)]
pub struct GroupHandle {
    arc: Arc<RwLock<GroupState>>,
    key: u64,
}

impl GroupHandle {
    /// The stable group key (minimum non-zero member GUID) this handle
    /// belongs to. Used by callers to detect group changes without having
    /// to re-`acquire`.
    #[inline]
    pub fn key(&self) -> u64 {
        self.key
    }

    /// Access the shared `GroupState` for reading or writing. Callers use
    /// `handle.state().try_read()` / `.write()` to obtain the RAII guards.
    #[inline]
    pub fn state(&self) -> &Arc<RwLock<GroupState>> {
        &self.arc
    }
}

impl Clone for GroupHandle {
    fn clone(&self) -> Self {
        Self {
            arc: Arc::clone(&self.arc),
            key: self.key,
        }
    }
}

impl Drop for GroupHandle {
    fn drop(&mut self) {
        // If we're the last strong reference, the registry's Weak is about
        // to become invalid — remove the entry now so the map doesn't
        // accumulate dead slots. Holding the registry lock across the
        // `strong_count` check prevents a concurrent `acquire` from racing
        // in between the check and the removal: any such `acquire` would
        // block on the mutex, and once it runs it will find the entry gone
        // and re-insert a fresh one.
        if Arc::strong_count(&self.arc) == 1
            && let Ok(mut map) = registry().lock() {
                // Defensive: only remove if the entry's weak points at our
                // arc. If someone else already replaced it, leave their
                // entry alone.
                if let Some(weak) = map.get(&self.key)
                    && weak.as_ptr() == Arc::as_ptr(&self.arc)
                {
                    map.remove(&self.key);
                }
            }
    }
}

/// Compute the stable group key from a snapshot's `group_members` slice.
///
/// Returns `None` when the bot is not in a group (or the slice is empty).
/// Otherwise returns the minimum non-zero GUID in the slice — a value every
/// member of the group will compute the same way.
pub fn group_key(members: &[u64]) -> Option<u64> {
    members.iter().copied().filter(|g| *g != 0).min()
}

/// Look up (or create) the shared `GroupState` for `key`, returning a
/// `GroupHandle` that will automatically deregister itself when the last
/// strong reference is dropped.
pub fn acquire(key: u64) -> GroupHandle {
    let mut map = registry().lock().expect("group registry poisoned");

    if let Some(weak) = map.get(&key)
        && let Some(arc) = weak.upgrade()
    {
        return GroupHandle { arc, key };
    }

    let arc = Arc::new(RwLock::new(GroupState::default()));
    map.insert(key, Arc::downgrade(&arc));
    GroupHandle { arc, key }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_contains(key: u64) -> bool {
        registry().lock().unwrap().contains_key(&key)
    }

    #[test]
    fn key_is_symmetric() {
        assert_eq!(group_key(&[3, 1, 2, 0, 0]), Some(1));
        assert_eq!(group_key(&[2, 3, 1]), Some(1));
        assert_eq!(group_key(&[0, 0, 0]), None);
        assert_eq!(group_key(&[]), None);
    }

    #[test]
    fn same_key_returns_same_arc() {
        let key = 0xDEAD_BEEF_0001;
        let a = acquire(key);
        let b = acquire(key);
        assert!(Arc::ptr_eq(a.state(), b.state()));
        assert_eq!(a.key(), key);
        assert_eq!(b.key(), key);
    }

    #[test]
    fn different_keys_return_different_arcs() {
        let a = acquire(0xAAAA_0001);
        let b = acquire(0xBBBB_0001);
        assert!(!Arc::ptr_eq(a.state(), b.state()));
    }

    #[test]
    fn entry_removed_promptly_on_last_drop() {
        let key = 0xCAFE_0001;
        {
            let _handle = acquire(key);
            assert!(registry_contains(key));
        }
        // No intermediate `acquire` needed — Drop took care of it.
        assert!(!registry_contains(key));
    }

    #[test]
    fn entry_kept_while_other_handles_exist() {
        let key = 0xCAFE_0002;
        let a = acquire(key);
        let b = acquire(key); // second handle on the same key
        drop(a);
        assert!(
            registry_contains(key),
            "entry must stay while `b` is still alive"
        );
        drop(b);
        assert!(
            !registry_contains(key),
            "entry must be gone after the last handle drops"
        );
    }
}
