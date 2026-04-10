//! RAII wrapper around `&mut dyn World` for multi-step factory operations.
//!
//! Every factory multi-step operation (clear inventory → learn spells → equip
//! gear → set talents) takes `&mut FactoryTransaction`. Explicit `.commit()`
//! is required at the end; dropping a non-committed transaction emits a
//! warning via `log_warn!`.
//!
//! # Semantics
//!
//! This is an **immediate-execution** wrapper. World mutations are not
//! journaled or deferred — every method call on `tx` hits the underlying
//! `&mut dyn World` as soon as it runs. Forced by read-modify-write loops in
//! the existing factory code (e.g. `talents::init_talents` reads
//! `bot_free_talent_points()` between `bot_learn_spell` calls).
//!
//! `commit()` is therefore a marker: it consumes `self` and sets `committed =
//! true` so `Drop` exits silently. A transaction dropped without a matching
//! `commit()` means the caller did not explicitly finish the factory unit —
//! we emit a warning so the bug is visible in logs. If the drop happens
//! during a panic unwind we skip the warning to avoid noise.
//!
//! # Phase B deviation
//!
//! `RUST_MIGRATION.md` places `FactoryTransaction` alongside the other RAII
//! guards in the `cmangos` crate. It lives here in `playerbot` instead
//! because the drop-warning uses `log_warn!`, which routes through a `CMaNGOS`
//! log sink owned by this crate. Moving the logging sink into `cmangos` is a
//! Phase K concern (it is prerequisite for tightening `playerbot/src/lib.rs`
//! to `forbid(unsafe_code)`), and widening Phase B to include that move
//! would overflow its scope. Placing the wrapper here keeps Phase B tight.

use crate::log_warn;
use cmangos::World;
use std::ops::{Deref, DerefMut};

/// Exclusive, `commit`-terminated handle to a `World` used by factory
/// multi-step operations.
pub struct FactoryTransaction<'a> {
    world: &'a mut dyn World,
    committed: bool,
}

impl<'a> FactoryTransaction<'a> {
    /// Wrap an exclusive `&mut dyn World` borrow in a transaction.
    #[inline]
    pub fn new(world: &'a mut dyn World) -> Self {
        Self { world, committed: false }
    }

    /// Mark this transaction as successfully committed and release the
    /// exclusive borrow. Consumes `self` so the `Drop` runs with `committed
    /// = true` and emits nothing.
    #[inline]
    pub fn commit(mut self) {
        self.committed = true;
        // Drop runs here; committed = true → no warning.
    }

    /// Read-only access to the underlying world. Use this when the `Deref`
    /// auto-route is unavailable (e.g. when naming the type explicitly or
    /// passing `&dyn World` into a generic function).
    #[inline]
    pub fn world(&self) -> &dyn World {
        self.world
    }

    /// Exclusive access to the underlying world. Same escape hatch as
    /// [`world`](Self::world) but for `&mut` contexts.
    #[inline]
    pub fn world_mut(&mut self) -> &mut dyn World {
        self.world
    }
}

impl<'a> Deref for FactoryTransaction<'a> {
    type Target = dyn World + 'a;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.world
    }
}

impl<'a> DerefMut for FactoryTransaction<'a> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.world
    }
}

impl<'a> Drop for FactoryTransaction<'a> {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        // Skip the warning during unwind so panics do not spam the log sink.
        if std::thread::panicking() {
            return;
        }
        log_warn!("FactoryTransaction dropped without commit");
    }
}

/// Scope helper used in factory unit tests to construct a transaction around
/// a `MockWorld`, run the factory step, and commit. Keeps test bodies free
/// of the `new` / `commit` dance.
///
/// The closure runs with a `&mut FactoryTransaction`. On return, the
/// transaction is committed and the exclusive borrow of `world` is released,
/// so the test can follow up with read-only assertions on the `MockWorld`.
#[cfg(test)]
pub(crate) fn with_tx<R>(
    world: &mut dyn World,
    f: impl FnOnce(&mut FactoryTransaction<'_>) -> R,
) -> R {
    let mut tx = FactoryTransaction::new(world);
    let r = f(&mut tx);
    tx.commit();
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmangos::MockWorld;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn committed_drop_is_silent() {
        let mut w = MockWorld::default();
        let tx = FactoryTransaction::new(&mut w);
        tx.commit();
        // Nothing to assert beyond "did not panic"; no log sink in tests.
    }

    #[test]
    fn uncommitted_drop_does_not_panic() {
        let mut w = MockWorld::default();
        {
            let _tx = FactoryTransaction::new(&mut w);
            // Drop runs here; committed = false, not panicking → warn fires.
            // No log sink installed in tests so the warn is dropped.
        }
        // Reaching this line means the drop exited cleanly.
    }

    #[test]
    fn panic_during_tx_unwinds_without_spurious_warn() {
        let mut w = MockWorld::default();
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _tx = FactoryTransaction::new(&mut w);
            panic!("boom");
        }));
        assert!(result.is_err(), "panic should have been caught");
        // Reaching this line means Drop did not double-panic during the
        // unwind — `std::thread::panicking()` gate short-circuited the
        // warning path.
    }

    #[test]
    fn with_tx_releases_borrow_for_follow_up_reads() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| {
            // Demonstrate that Deref auto-routes method calls onto the
            // underlying World.
            let _snap = tx.get_snapshot();
        });
        // After with_tx returns, `w` is free to borrow again.
        let _snap = w.get_snapshot();
    }
}
