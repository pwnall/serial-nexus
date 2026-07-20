//! Write arbitration: the per-host-facing-endpoint exclusive write lock (§6).
//!
//! Reading is never arbitrated — the one-producer invariant (§4) makes hostward
//! flow unambiguous and every attachment may watch. Writing *targetward* through
//! a host-facing endpoint is: among all origins attached to it, at most one holds
//! the exclusive write lock, and only the holder's bytes are read targetward. The
//! lock is a gate on the §5 pause machinery — a non-holder is simply not read
//! from — so arbitration adds no new data path.
//!
//! This module is the pure state machine. It decides who may write
//! ([`EndpointLock::may_write`]), records the holder and per-origin purge
//! accounting, and applies the acquire/release/detach transitions. It performs no
//! I/O: the daemon wraps it in an `Rc<RefCell<_>>` shared (on the one runtime
//! thread) between the control-plane methods that mutate it and the origin read
//! tasks that consult it. Steal, lease, waiters, and the atomic `send` build on
//! this in later slices; the shape here leaves room for them without disturbing
//! the exclusivity core.

use std::collections::BTreeMap;

use serde::Serialize;

pub use crate::graph::{Arbitration, WriteMode};

/// Stable identifier for one origin contending for a single endpoint's write
/// lock. An origin is a hostward boundary through which bytes enter travelling
/// targetward — a PTY with a client attached, an accepted socket connection, the
/// CLI `send` verb, a remote daemon's leg (§3). Assigned by the data-plane wiring
/// and unique within one endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OriginId(pub u64);

/// The outcome of an explicit acquire attempt (§6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acquire {
    /// A fresh grant: the lock was free and the caller now holds it. This is the
    /// transition on which purge-on-acquire fires (the daemon drains and discards
    /// anything the origin buffered before the grant, §6).
    Granted,
    /// The caller already held the lock; an idempotent no-op (no purge).
    AlreadyHeld,
    /// Held by another origin. A plain acquire fails; `--wait` queues; `--steal`
    /// overrides. The daemon maps `held_by` to a label via [`EndpointLock::label`].
    Denied { held_by: OriginId },
    /// The origin's edge is `write = never` (a log edge, a spy PTY): it cannot
    /// contend for the lock at all (§6).
    ReadOnly,
}

/// One origin's participation in an endpoint's arbitration.
#[derive(Debug, Clone)]
struct Origin {
    /// Operator-facing label (the daemon assigns the origin's node name), for
    /// state reporting.
    label: String,
    write_mode: WriteMode,
    /// Bytes purged from this origin — pre-grant bytes discarded on explicit
    /// acquire, or a backlog discarded on detach without the lock (§6). Counted
    /// so the stale-command hazard is always visible.
    purged: u64,
}

/// The exclusive write lock for one host-facing endpoint (§6). Holds the origins
/// attached to the endpoint, which one (if any) holds the lock, and each origin's
/// purge accounting.
#[derive(Debug, Clone)]
pub struct EndpointLock {
    arbitration: Arbitration,
    origins: BTreeMap<OriginId, Origin>,
    holder: Option<OriginId>,
}

impl EndpointLock {
    pub fn new(arbitration: Arbitration) -> Self {
        EndpointLock {
            arbitration,
            origins: BTreeMap::new(),
            holder: None,
        }
    }

    pub fn arbitration(&self) -> Arbitration {
        self.arbitration
    }

    /// Register an origin attaching to this endpoint. A `held`-mode origin
    /// acquires the lock on attach if it is free (the demux codec's permanent
    /// hold, §6); under free-for-all there is no lock to take.
    pub fn register(&mut self, id: OriginId, label: impl Into<String>, write_mode: WriteMode) {
        self.origins.insert(
            id,
            Origin {
                label: label.into(),
                write_mode,
                purged: 0,
            },
        );
        if write_mode == WriteMode::Held
            && self.arbitration == Arbitration::Exclusive
            && self.holder.is_none()
        {
            self.holder = Some(id);
        }
    }

    /// Remove an origin (its client detached, the node was removed). If it held
    /// the lock, the lock releases automatically (detach-release, §6). Returns
    /// whether the lock was released as a result.
    pub fn unregister(&mut self, id: OriginId) -> bool {
        self.origins.remove(&id);
        if self.holder == Some(id) {
            self.holder = None;
            true
        } else {
            false
        }
    }

    /// Whether an origin currently registered here is the writer whose bytes are
    /// read targetward — the gate the data plane consults every cycle. A
    /// `write = never` origin never writes; under free-for-all every writer may;
    /// under the exclusive default only the lock holder may (§6).
    pub fn may_write(&self, id: OriginId) -> bool {
        let Some(origin) = self.origins.get(&id) else {
            return false;
        };
        if origin.write_mode == WriteMode::Never {
            return false;
        }
        match self.arbitration {
            Arbitration::FreeForAll => true,
            Arbitration::Exclusive => self.holder == Some(id),
        }
    }

    /// Explicit acquisition by a named origin (§6). Under free-for-all there is
    /// no lock, so any writer "acquires" trivially. Under the exclusive default,
    /// a free lock is granted, the same holder re-acquiring is a no-op, and a
    /// lock held by another is denied.
    pub fn acquire(&mut self, id: OriginId) -> Acquire {
        match self.origins.get(&id) {
            None => return Acquire::ReadOnly,
            Some(o) if o.write_mode == WriteMode::Never => return Acquire::ReadOnly,
            Some(_) => {}
        }
        if self.arbitration == Arbitration::FreeForAll {
            return Acquire::Granted;
        }
        match self.holder {
            None => {
                self.holder = Some(id);
                Acquire::Granted
            }
            Some(h) if h == id => Acquire::AlreadyHeld,
            Some(h) => Acquire::Denied { held_by: h },
        }
    }

    /// Release the lock if `id` holds it. Returns true if a release happened.
    pub fn release(&mut self, id: OriginId) -> bool {
        if self.holder == Some(id) {
            self.holder = None;
            true
        } else {
            false
        }
    }

    pub fn holder(&self) -> Option<OriginId> {
        self.holder
    }

    /// The label of a registered origin (for mapping a `Denied { held_by }` or
    /// the current holder to an operator-facing name).
    pub fn label(&self, id: OriginId) -> Option<&str> {
        self.origins.get(&id).map(|o| o.label.as_str())
    }

    /// Record `bytes` purged from an origin's targetward backlog (§6): pre-grant
    /// discard on explicit acquire, or backlog discard on detach without the lock.
    pub fn record_purge(&mut self, id: OriginId, bytes: u64) {
        if let Some(o) = self.origins.get_mut(&id) {
            o.purged = o.purged.saturating_add(bytes);
        }
    }

    pub fn purged(&self, id: OriginId) -> u64 {
        self.origins.get(&id).map_or(0, |o| o.purged)
    }

    /// A reportable snapshot for the `state` verb (§6: holder and per-origin
    /// purge counters per endpoint). Waiters arrive with `--wait` in a later
    /// slice; the field is present and empty so the shape is stable.
    pub fn snapshot(&self) -> LockSnapshot {
        LockSnapshot {
            arbitration: self.arbitration,
            holder: self.holder.and_then(|h| self.label(h)).map(str::to_owned),
            origins: self
                .origins
                .values()
                .map(|o| OriginState {
                    origin: o.label.clone(),
                    write_mode: o.write_mode,
                    holds_lock: self
                        .holder
                        .and_then(|h| self.label(h))
                        .is_some_and(|l| l == o.label),
                    purged: o.purged,
                })
                .collect(),
            waiters: Vec::new(),
        }
    }
}

/// A reportable view of one endpoint's lock (§6), disjoint from configuration —
/// this is observed state (§15.8).
#[derive(Debug, Clone, Serialize)]
pub struct LockSnapshot {
    pub arbitration: Arbitration,
    /// The origin holding the lock, if any.
    pub holder: Option<String>,
    /// Each attached origin's arbitration participation.
    pub origins: Vec<OriginState>,
    /// Origins waiting for the lock (populated once `--wait` lands).
    pub waiters: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OriginState {
    pub origin: String,
    pub write_mode: WriteMode,
    pub holds_lock: bool,
    pub purged: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn on_demand() -> EndpointLock {
        let mut lock = EndpointLock::new(Arbitration::Exclusive);
        lock.register(OriginId(1), "a", WriteMode::OnDemand);
        lock.register(OriginId(2), "b", WriteMode::OnDemand);
        lock
    }

    #[test]
    fn only_the_holder_may_write_under_exclusive() {
        let mut lock = on_demand();
        // No holder yet: nobody writes (on-demand must acquire first, §6).
        assert!(!lock.may_write(OriginId(1)));
        assert!(!lock.may_write(OriginId(2)));

        assert_eq!(lock.acquire(OriginId(1)), Acquire::Granted);
        assert!(lock.may_write(OriginId(1)));
        assert!(!lock.may_write(OriginId(2)), "a holds it; b is locked out");

        // b cannot acquire while a holds it, and is named the holder.
        assert_eq!(
            lock.acquire(OriginId(2)),
            Acquire::Denied {
                held_by: OriginId(1)
            }
        );
        assert_eq!(lock.label(OriginId(1)), Some("a"));

        // Re-acquiring by the holder is an idempotent no-op (no purge).
        assert_eq!(lock.acquire(OriginId(1)), Acquire::AlreadyHeld);
    }

    #[test]
    fn release_frees_the_lock_but_does_not_grant_a_non_holder() {
        let mut lock = on_demand();
        lock.acquire(OriginId(1));
        assert!(lock.release(OriginId(1)));
        // A free lock does not auto-grant b's buffered writes (the 3 a.m.
        // safety, §6): b still may not write until it explicitly acquires.
        assert!(!lock.may_write(OriginId(1)));
        assert!(!lock.may_write(OriginId(2)));
        // Releasing when you do not hold it is a no-op.
        assert!(!lock.release(OriginId(2)));
    }

    #[test]
    fn detach_releases_the_lock() {
        let mut lock = on_demand();
        lock.acquire(OriginId(1));
        assert!(
            lock.unregister(OriginId(1)),
            "detaching the holder releases"
        );
        assert_eq!(lock.holder(), None);
        // A non-holder detaching does not report a release.
        assert!(!lock.unregister(OriginId(2)));
    }

    #[test]
    fn never_writers_cannot_contend() {
        let mut lock = EndpointLock::new(Arbitration::Exclusive);
        lock.register(OriginId(9), "spy", WriteMode::Never);
        assert!(!lock.may_write(OriginId(9)));
        assert_eq!(lock.acquire(OriginId(9)), Acquire::ReadOnly);
        assert_eq!(lock.holder(), None);
    }

    #[test]
    fn held_mode_acquires_on_attach() {
        let mut lock = EndpointLock::new(Arbitration::Exclusive);
        lock.register(OriginId(1), "demux", WriteMode::Held);
        assert_eq!(lock.holder(), Some(OriginId(1)));
        assert!(lock.may_write(OriginId(1)));
        // A later on-demand origin is locked out by the permanent hold.
        lock.register(OriginId(2), "spy", WriteMode::OnDemand);
        assert_eq!(
            lock.acquire(OriginId(2)),
            Acquire::Denied {
                held_by: OriginId(1)
            }
        );
    }

    #[test]
    fn free_for_all_lets_every_writer_write() {
        let mut lock = EndpointLock::new(Arbitration::FreeForAll);
        lock.register(OriginId(1), "a", WriteMode::OnDemand);
        lock.register(OriginId(2), "b", WriteMode::OnDemand);
        lock.register(OriginId(3), "log", WriteMode::Never);
        assert!(lock.may_write(OriginId(1)));
        assert!(
            lock.may_write(OriginId(2)),
            "no exclusion under free-for-all"
        );
        assert!(!lock.may_write(OriginId(3)), "never is still never");
        assert_eq!(lock.acquire(OriginId(1)), Acquire::Granted);
    }

    #[test]
    fn purge_accounting_accumulates_per_origin() {
        let mut lock = on_demand();
        lock.record_purge(OriginId(2), 100);
        lock.record_purge(OriginId(2), 23);
        assert_eq!(lock.purged(OriginId(2)), 123);
        assert_eq!(lock.purged(OriginId(1)), 0);
    }

    // --- Property: at most one holder, and may_write matches the holder --------

    #[derive(Debug, Clone)]
    enum Op {
        Acquire(u8),
        Release(u8),
        Detach(u8),
        Reattach(u8),
    }

    fn op_strategy() -> impl Strategy<Value = Op> {
        prop_oneof![
            (0u8..4).prop_map(Op::Acquire),
            (0u8..4).prop_map(Op::Release),
            (0u8..4).prop_map(Op::Detach),
            (0u8..4).prop_map(Op::Reattach),
        ]
    }

    proptest! {
        /// Under any interleaving of acquire/release/detach/reattach across four
        /// on-demand origins, the exclusive lock never has two holders, and
        /// `may_write` is true for exactly the current holder (§6).
        #[test]
        fn prop_exclusive_invariants(ops in prop::collection::vec(op_strategy(), 0..64)) {
            let mut lock = EndpointLock::new(Arbitration::Exclusive);
            let mut attached = [true; 4];
            for i in 0..4u8 {
                lock.register(OriginId(i as u64), format!("o{i}"), WriteMode::OnDemand);
            }
            for op in ops {
                match op {
                    Op::Acquire(i) => { lock.acquire(OriginId(i as u64)); }
                    Op::Release(i) => { lock.release(OriginId(i as u64)); }
                    Op::Detach(i) => {
                        lock.unregister(OriginId(i as u64));
                        attached[i as usize] = false;
                    }
                    Op::Reattach(i) => {
                        if !attached[i as usize] {
                            lock.register(OriginId(i as u64), format!("o{i}"), WriteMode::OnDemand);
                            attached[i as usize] = true;
                        }
                    }
                }
                // Invariant 1: at most one origin may write.
                let writers: Vec<u64> = (0..4u64).filter(|&i| lock.may_write(OriginId(i))).collect();
                prop_assert!(writers.len() <= 1, "two writers: {writers:?}");
                // Invariant 2: whoever may write is exactly the holder, and the
                // holder is always a still-attached origin.
                match lock.holder() {
                    Some(h) => {
                        prop_assert_eq!(writers, vec![h.0]);
                        prop_assert!(attached[h.0 as usize], "holder detached but still holds");
                    }
                    None => prop_assert!(writers.is_empty()),
                }
            }
        }
    }
}
