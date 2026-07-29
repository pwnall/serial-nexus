//! The critical-section cell (design §16.2, plan §9.2).
//!
//! `serial-nexus-daemon` runs its state and its per-endpoint locks on one runtime thread,
//! so a `RefCell` serialized them with no locks (plan §2). The load-bearing
//! discipline of the two-lane control plane (§15.20) was a review rule — **a
//! `RefCell` borrow never crosses an `.await`** — upheld by hand and by audit, and
//! three shipped bugs elsewhere came from exactly this class of by-hand invariant.
//!
//! [`CriticalCell`] makes that rule structural. Its state is reachable only inside
//! a *synchronous* closure — [`CriticalCell::with`] / [`CriticalCell::with_mut`] —
//! so the borrow guard is created and dropped inside the closure and can never be
//! held across an `.await` (a non-async closure cannot even contain one). Raw
//! `std::cell::RefCell` is banned by `disallowed-types` in **`daemon/clippy.toml`
//! and `daemon-bin/clippy.toml` alike**; the single sanctioned instance is the one
//! wrapped here, so the tripwire is a compile-shape fact rather than a thing
//! reviewers must catch.
//!
//! Both files are needed, and that is the point of the trap this comment used to fall
//! into: clippy resolves `clippy.toml` from `CARGO_MANIFEST_DIR` upward through
//! *ancestors*, so when daemon state moved from `serial-nexus-daemon` to this sibling crate
//! at the v8 library split, the ban silently stopped covering the code it was written
//! for — proven with a planted `RefCell` plus a lint canary, and unnoticed for a
//! release. A configuration gate disarms without a sound when its crate moves, so the
//! durable half of the fix is the meta-gate test that greps for the type directly
//! (`itest/tests/meta_gates.rs`), which survives the next move.
//!
//! It does **not** change re-entrancy: calling `with_mut` on a cell already
//! borrowed panics, exactly as a nested `borrow_mut` would — that hazard was never
//! the one §15.20 was about. Single-thread only (no `Sync`), like the `RefCell` it
//! replaces.

/// A single-threaded cell whose contents are reachable only inside a synchronous
/// critical section (§16.2). Replaces `std::cell::RefCell` throughout the daemon.
pub struct CriticalCell<T> {
    // The one sanctioned RefCell in the daemon: wrapping it so no `Ref`/`RefMut`
    // guard can escape a synchronous closure is the entire point of this type.
    #[allow(clippy::disallowed_types)]
    inner: std::cell::RefCell<T>,
}

impl<T> CriticalCell<T> {
    /// Wrap `value`.
    pub fn new(value: T) -> Self {
        #[allow(clippy::disallowed_types)]
        CriticalCell {
            inner: std::cell::RefCell::new(value),
        }
    }

    /// Run `f` with shared access inside a synchronous critical section, returning
    /// its result. The borrow lives only for the closure, so it cannot cross an
    /// `.await` (§15.20).
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.inner.borrow())
    }

    /// Run `f` with exclusive access inside a synchronous critical section,
    /// returning its result. As with [`Self::with`], the borrow cannot escape.
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        f(&mut self.inner.borrow_mut())
    }
}

impl<T: Default> Default for CriticalCell<T> {
    fn default() -> Self {
        CriticalCell::new(T::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_and_with_mut_read_and_mutate() {
        let cell = CriticalCell::new(0u32);
        cell.with_mut(|n| *n += 5);
        cell.with_mut(|n| *n *= 2);
        assert_eq!(cell.with(|n| *n), 10);
    }

    #[test]
    fn with_returns_a_computed_value() {
        let cell = CriticalCell::new(vec![1u8, 2, 3]);
        let sum: u8 = cell.with(|v| v.iter().copied().sum());
        assert_eq!(sum, 6);
        // The borrow is confined to the closure; the cell is usable afterward.
        cell.with_mut(|v| v.push(4));
        assert_eq!(cell.with(Vec::len), 4);
    }

    #[test]
    fn default_wraps_the_inner_default() {
        let cell: CriticalCell<Vec<u8>> = CriticalCell::default();
        assert!(cell.with(Vec::is_empty));
    }
}
