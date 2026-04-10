//! RAII guard around C-allocated arrays returned from `BotCallbacks` getters.
//!
//! Every list-returning method on the `World` trait hands back an
//! `OwnedList<'a, T>` (or one of the named type aliases below). The list
//! borrows nothing from `&self` other than the lifetime stamp; the underlying
//! storage is owned and freed in `Drop` according to the `Free` strategy
//! recorded at construction.
//!
//! There are three free strategies:
//!
//! * **Ffi** — buffer was allocated by the C++ side via the `BotCallbacks`
//!   vtable; on drop we call back through the matching `free_*` function
//!   pointer (signature `fn(*mut T)` per `cpp_wrapper/botffi.h`).
//! * **Boxed** — buffer was allocated by Rust as `Box<[T]>` (used by
//!   `MockWorld`); on drop we rebuild the box and let it deallocate.
//! * **Empty** — null buffer, length zero, nothing to free. Used as the
//!   default value for trait methods that aren't overridden.
//!
//! The struct itself is *not* generic over a closure type — every
//! `OwnedList<'a, T>` has the same Rust type, which means callers can name
//! it without `impl Trait` games and the 13 type aliases below are bare
//! `pub type` declarations.

use core::ffi::c_char;
use core::marker::PhantomData;
use core::ops::Deref;
use core::ptr::NonNull;
use core::slice;

use crate::{
    BotAuraInfo, BotInventoryItem, BotQuestInfo, BotReputationEntry, BotSkillEntry, BotTalentEntry,
    BotTaxiNode, BotThreatEntry, BotTravelDest, UnitHandle,
};

/// How an `OwnedList`'s buffer should be released on drop.
pub enum Free<T> {
    /// Buffer was returned by the C++ side; release it via the matching
    /// `free_*` callback (single-pointer signature, no length).
    Ffi(unsafe extern "C" fn(*mut T)),
    /// Buffer was allocated as a Rust `Box<[T]>`. On drop we rebuild the box
    /// from `(ptr, len)` and let `Drop` deallocate.
    Boxed,
    /// Sentinel: buffer is null, length is zero, nothing to free.
    Empty,
}

pub struct OwnedList<'a, T> {
    ptr: Option<NonNull<T>>,
    len: usize,
    free: Free<T>,
    _marker: PhantomData<&'a T>,
}

// Safety: OwnedList owns its buffer outright; sharing across threads is fine
// as long as the element type is itself Send/Sync. We do not actually
// implement these blanket impls because the element types are FFI POD that
// are bytewise Send already, and we want test fixtures to be free to use
// non-Send types like Rc inside the buffer if they ever need to.

impl<'a, T> OwnedList<'a, T> {
    /// Construct an `OwnedList` from a raw pointer + length + free strategy.
    ///
    /// # Safety
    ///
    /// * If `free` is `Ffi(f)`, then `ptr` must have been allocated by the
    ///   C++ side and `f` must be the matching `free_*` function pointer
    ///   from the same `BotCallbacks` table.
    /// * If `free` is `Boxed`, then `ptr` and `len` must have come from
    ///   `Box::into_raw` on a `Box<[T]>` of exactly that length.
    /// * If `free` is `Empty`, `ptr` is ignored; the list is empty.
    /// * The caller must not free the buffer through any other path.
    #[inline]
    pub unsafe fn from_raw(ptr: *mut T, len: usize, free: Free<T>) -> Self {
        let (ptr, len) = match free {
            Free::Empty => (None, 0),
            _ => (NonNull::new(ptr), if ptr.is_null() { 0 } else { len }),
        };
        Self {
            ptr,
            len,
            free,
            _marker: PhantomData,
        }
    }

    /// An empty list. Free strategy is `Empty`, so dropping it is a no-op.
    #[inline]
    #[must_use]
    pub fn empty() -> Self {
        Self {
            ptr: None,
            len: 0,
            free: Free::Empty,
            _marker: PhantomData,
        }
    }

    /// Wrap a `Box<[T]>` into an `OwnedList` whose `Drop` will rebuild and
    /// release the box. Used by `MockWorld` to satisfy the `OwnedList`
    /// contract from heap-allocated test fixtures.
    #[inline]
    #[must_use]
    pub fn from_boxed_slice(boxed: alloc::boxed::Box<[T]>) -> Self {
        let len = boxed.len();
        let ptr = alloc::boxed::Box::into_raw(boxed).cast::<T>();
        Self {
            ptr: NonNull::new(ptr),
            len,
            free: Free::Boxed,
            _marker: PhantomData,
        }
    }

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        match self.ptr {
            // Safety: invariants enforced at construction; the buffer is
            // valid for `self.len` reads of `T` until `Drop`.
            Some(p) => unsafe { slice::from_raw_parts(p.as_ptr(), self.len) },
            None => &[],
        }
    }
}

impl<T> Default for OwnedList<'_, T> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<T> Deref for OwnedList<'_, T> {
    type Target = [T];
    #[inline]
    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<'a, 'b, T> IntoIterator for &'b OwnedList<'a, T> {
    type Item = &'b T;
    type IntoIter = core::slice::Iter<'b, T>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for OwnedList<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.as_slice()).finish()
    }
}

impl<T> Drop for OwnedList<'_, T> {
    fn drop(&mut self) {
        let Some(p) = self.ptr.take() else { return };
        match self.free {
            Free::Ffi(f) => {
                // Safety: precondition of `from_raw` — `f` matches the
                // allocator that produced `p`.
                unsafe { f(p.as_ptr()) };
            }
            Free::Boxed => {
                // Safety: `Boxed` was set by `from_boxed_slice` (or an
                // equivalent `from_raw` caller); rebuild the box and drop.
                let slice_ptr =
                    core::ptr::slice_from_raw_parts_mut(p.as_ptr(), self.len);
                drop(unsafe { alloc::boxed::Box::from_raw(slice_ptr) });
            }
            Free::Empty => {}
        }
    }
}

extern crate alloc;

// ── Type aliases ─────────────────────────────────────────────────────────────

pub type AuraList<'a> = OwnedList<'a, BotAuraInfo>;
pub type ThreatList<'a> = OwnedList<'a, BotThreatEntry>;
pub type UnitList<'a> = OwnedList<'a, UnitHandle>;
pub type QuestLog<'a> = OwnedList<'a, BotQuestInfo>;
pub type GatherableList<'a> = OwnedList<'a, u64>;
pub type BotSpellList<'a> = OwnedList<'a, u32>;
pub type TaxiNodeList<'a> = OwnedList<'a, BotTaxiNode>;
pub type TalentList<'a> = OwnedList<'a, BotTalentEntry>;
pub type ReputationList<'a> = OwnedList<'a, BotReputationEntry>;
pub type SkillList<'a> = OwnedList<'a, BotSkillEntry>;
pub type TravelDestList<'a> = OwnedList<'a, BotTravelDest>;
pub type InventoryList<'a> = OwnedList<'a, BotInventoryItem>;
pub type OwnedCString<'a> = OwnedList<'a, c_char>;

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicUsize, Ordering};

    static FREE_CALLS: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "C" fn fake_free(_: *mut u32) {
        FREE_CALLS.fetch_add(1, Ordering::SeqCst);
    }

    #[test]
    fn empty_list_is_empty() {
        let l: OwnedList<'_, u32> = OwnedList::empty();
        assert_eq!(l.len(), 0);
        assert!(l.is_empty());
        assert_eq!(l.as_slice(), &[]);
    }

    #[test]
    fn boxed_list_drops_storage() {
        let boxed: alloc::boxed::Box<[u32]> =
            alloc::vec![1u32, 2, 3].into_boxed_slice();
        let l = OwnedList::from_boxed_slice(boxed);
        assert_eq!(l.len(), 3);
        assert_eq!(&*l, &[1, 2, 3]);
        // Drop runs here without leaking; miri would catch otherwise.
    }

    #[test]
    fn ffi_free_called_exactly_once() {
        FREE_CALLS.store(0, Ordering::SeqCst);
        // Allocate via Box so the test owns the storage; we use a no-op
        // "free" callback that just bumps the counter and lets the box leak
        // (the test cleans up by re-acquiring the pointer below).
        let boxed: alloc::boxed::Box<[u32]> = alloc::vec![10u32, 20].into_boxed_slice();
        let len = boxed.len();
        let raw = alloc::boxed::Box::into_raw(boxed).cast::<u32>();
        let l = unsafe { OwnedList::from_raw(raw, len, Free::Ffi(fake_free)) };
        assert_eq!(l.len(), 2);
        assert_eq!(&*l, &[10, 20]);
        drop(l);
        assert_eq!(FREE_CALLS.load(Ordering::SeqCst), 1);
        // Reclaim the leaked box so the test doesn't leak.
        let _reclaim = unsafe {
            alloc::boxed::Box::from_raw(core::ptr::slice_from_raw_parts_mut(raw, len))
        };
    }

    #[test]
    fn iter_via_ref() {
        let l = OwnedList::from_boxed_slice(alloc::vec![1u32, 2, 3].into_boxed_slice());
        let sum: u32 = (&l).into_iter().sum();
        assert_eq!(sum, 6);
    }
}
