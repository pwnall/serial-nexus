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
//! ([`EndpointLock::may_write`]), records the holder, the FIFO waiter queue, the
//! grant generation, per-origin purge accounting, and the most recent steal. It
//! performs no I/O and knows nothing of async: the daemon wraps it in a
//! `LockCell` (an `Rc<RefCell<_>>` plus a `Notify`) shared, on the one runtime
//! thread, between the control-plane verbs that mutate it and the origin read
//! tasks that consult it. The two-lane control plane (§15.20) lives entirely in
//! the daemon glue; this state machine only provides the fair, generation-guarded
//! primitives it drives — `acquire` grants only to the FIFO head, `enqueue`/
//! `dequeue` manage waiters cancel-safely, `steal` bypasses the queue without
//! destroying it, and `generation` guards a lease against a stale timer.

use std::collections::{BTreeMap, VecDeque};

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
    /// A fresh grant: the lock was free (and the caller was the FIFO head or the
    /// queue was empty), and the caller now holds it. This is the transition on
    /// which purge-on-acquire fires (the daemon drains and discards anything the
    /// origin buffered before the grant, §6) and on which the grant generation
    /// advances (guarding leases, §6).
    Granted,
    /// The caller already held the lock; an idempotent no-op (no purge).
    AlreadyHeld,
    /// Held by another origin, or — while the lock is momentarily free — barred by
    /// an earlier waiter at the head of the queue (FIFO fairness, §6). A plain
    /// acquire fails; `--wait` queues behind `held_by`; `--steal` overrides. The
    /// daemon maps `held_by` to a label via [`EndpointLock::label`].
    Denied { held_by: OriginId },
    /// The origin's edge is `write = never` (a log edge, a spy PTY): it cannot
    /// contend for the lock at all (§6).
    ReadOnly,
}

/// The outcome of a steal (`lock --steal`, `send --steal`, §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Steal {
    /// The caller now holds the lock; `previous` is the origin it was taken from
    /// (recorded in state so the ousted holder sees what happened), or `None` if
    /// the lock was free or the endpoint is free-for-all.
    Stolen { previous: Option<OriginId> },
    /// The origin is `write = never` and cannot hold the lock at all.
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
/// attached to the endpoint, which one (if any) holds the lock, the FIFO queue of
/// origins waiting for it, the grant generation, each origin's purge accounting,
/// and the most recent steal.
#[derive(Debug, Clone)]
pub struct EndpointLock {
    arbitration: Arbitration,
    origins: BTreeMap<OriginId, Origin>,
    holder: Option<OriginId>,
    /// FIFO queue of origins waiting for the lock (`lock --wait`, `send`). Grants
    /// pass to the front on every release path; a plain `acquire` while the queue
    /// is non-empty is denied unless the caller is the head (§6 fairness).
    waiters: VecDeque<OriginId>,
    /// Advances on every fresh grant (a free→held transition via `acquire` or
    /// `steal`). A lease timer captures the generation at grant and only fires if
    /// it still matches, so a stale timer can never release a later grant (§6).
    generation: u64,
    /// The most recent steal `(from, by)`, surfaced in state so an ousted holder
    /// can see it (§6). Best-effort: dropped from the snapshot once either party
    /// detaches.
    stolen: Option<(OriginId, OriginId)>,
}

impl EndpointLock {
    pub fn new(arbitration: Arbitration) -> Self {
        EndpointLock {
            arbitration,
            origins: BTreeMap::new(),
            holder: None,
            waiters: VecDeque::new(),
            generation: 0,
            stolen: None,
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
            self.generation = self.generation.wrapping_add(1);
        }
    }

    /// Remove an origin (its client detached, the node was removed). If it held
    /// the lock, the lock releases automatically (detach-release, §6); it is also
    /// dropped from the waiter queue. Returns whether the lock was released as a
    /// result.
    pub fn unregister(&mut self, id: OriginId) -> bool {
        self.origins.remove(&id);
        self.waiters.retain(|w| *w != id);
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
    /// no lock, so any writer "acquires" trivially. Under the exclusive default a
    /// free lock is granted **only to the FIFO head** (or to anyone when the queue
    /// is empty) — so a plain acquire that would barge past an earlier waiter is
    /// denied, naming that waiter; the same holder re-acquiring is a no-op; and a
    /// lock held by another is denied. A fresh grant removes the caller from the
    /// queue (if it was waiting) and advances the generation.
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
            Some(h) if h == id => Acquire::AlreadyHeld,
            Some(h) => Acquire::Denied { held_by: h },
            None => {
                // Held priority outranks the FIFO queue (§6/§15.23): while the lock
                // is momentarily free (a steal transiently ousted the demux, or it
                // has yet to reclaim), a registered `held` origin reclaims ahead of
                // every on-demand contender — granting a queued waiter a
                // demultiplexer's lock would corrupt the framing the hold protects.
                // Deferring here makes that deterministic instead of a race between
                // the reclaim task and the woken waiter on the shared `Notify`.
                if let Some(held) = self.held_origin_other_than(id) {
                    return Acquire::Denied { held_by: held };
                }
                match self.waiters.front().copied() {
                    // FIFO fairness: while free-but-queued, only the head may take it.
                    Some(front) if front != id => Acquire::Denied { held_by: front },
                    _ => {
                        self.grant_to(id);
                        Acquire::Granted
                    }
                }
            }
        }
    }

    /// Join the FIFO waiter queue (`lock --wait`, `send`). A registered writer
    /// that is neither the current holder nor already queued is appended; anything
    /// else is a no-op. The daemon calls this after a plain [`Self::acquire`]
    /// returns `Denied`, then suspends until woken and re-attempts (§15.20).
    pub fn enqueue(&mut self, id: OriginId) {
        match self.origins.get(&id) {
            Some(o) if o.write_mode != WriteMode::Never => {}
            _ => return,
        }
        if self.holder == Some(id) || self.waiters.contains(&id) {
            return;
        }
        self.waiters.push_back(id);
    }

    /// Remove an origin from the waiter queue — cancellation on a deadline, a
    /// dropped control connection, teardown, or endpoint removal (§6, cancel-safe
    /// waiting). Idempotent.
    pub fn dequeue(&mut self, id: OriginId) {
        self.waiters.retain(|w| *w != id);
    }

    /// Steal the lock for `id` (`lock --steal`, `send --steal`, §6): take it
    /// regardless of the current holder, recording the ousted holder so state can
    /// show it. Steal **bypasses the queue without destroying it** — waiters stay
    /// in line for when the stealer releases. A `write = never` origin cannot
    /// steal; under free-for-all there is no lock, so it trivially succeeds.
    pub fn steal(&mut self, id: OriginId) -> Steal {
        match self.origins.get(&id) {
            None => return Steal::ReadOnly,
            Some(o) if o.write_mode == WriteMode::Never => return Steal::ReadOnly,
            Some(_) => {}
        }
        if self.arbitration == Arbitration::FreeForAll {
            return Steal::Stolen { previous: None };
        }
        let previous = self.holder.filter(|h| *h != id);
        if let Some(prev) = previous {
            self.stolen = Some((prev, id));
        }
        self.waiters.retain(|w| *w != id);
        self.holder = Some(id);
        self.generation = self.generation.wrapping_add(1);
        Steal::Stolen { previous }
    }

    /// Set the holder and advance the generation; used by every fresh grant.
    fn grant_to(&mut self, id: OriginId) {
        self.holder = Some(id);
        self.waiters.retain(|w| *w != id);
        self.generation = self.generation.wrapping_add(1);
    }

    /// The registered `held` origin other than `except`, if any (§6/§15.23): the
    /// demultiplexer whose permanent hold outranks the on-demand FIFO queue. Used
    /// by [`Self::acquire`] to defer an on-demand contender while the lock is
    /// momentarily free, so the held origin's [`Self::reclaim_held`] always wins.
    fn held_origin_other_than(&self, except: OriginId) -> Option<OriginId> {
        self.origins
            .iter()
            .find_map(|(id, o)| (*id != except && o.write_mode == WriteMode::Held).then_some(*id))
    }

    /// A `Held` origin's *priority* reclaim of a free lock (§6). Unlike
    /// [`Self::acquire`], it bypasses the FIFO head check: a held origin holds the
    /// lock indefinitely, so after a `--steal` transiently ousts it, it reclaims
    /// the instant the lock frees — ahead of any on-demand `--wait` waiter, which
    /// by design waits indefinitely behind a held lock (the demux's permanent hold,
    /// so no other writer can corrupt the mux framing). Grants only if `id` is a
    /// registered `Held` origin and the lock is currently free; returns whether it
    /// took (or already effectively has, under free-for-all) the lock.
    pub fn reclaim_held(&mut self, id: OriginId) -> bool {
        match self.origins.get(&id) {
            Some(o) if o.write_mode == WriteMode::Held => {}
            _ => return false,
        }
        if self.arbitration == Arbitration::FreeForAll {
            return true; // no lock to hold; the writer already may write
        }
        if self.holder.is_none() {
            self.grant_to(id);
            true
        } else {
            false // still held (by a stealer); wait
        }
    }

    /// Release the lock if `id` holds it. Returns true if a release happened. Does
    /// not itself grant to the queue — the daemon wakes the waiters, and the head
    /// re-attempts [`Self::acquire`] in a fresh critical section (§15.20).
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

    /// Advance the grant generation for the current holder without changing the
    /// holder — used to *renew* a lease (`lock --lease-ms` on an already-held
    /// grant): the prior lease timer, guarded by the old generation, can then no
    /// longer fire, so the renewed deadline wins (§6). Returns the new generation,
    /// or `None` if `id` does not currently hold the lock.
    pub fn renew(&mut self, id: OriginId) -> Option<u64> {
        if self.holder == Some(id) {
            self.generation = self.generation.wrapping_add(1);
            Some(self.generation)
        } else {
            None
        }
    }

    /// The generation of the current grant — captured by a lease timer so its
    /// firing is guarded against a later grant (§6).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The FIFO waiter queue, front (next to be granted) first.
    pub fn waiters(&self) -> impl Iterator<Item = OriginId> + '_ {
        self.waiters.iter().copied()
    }

    /// The label of a registered origin (for mapping a `Denied { held_by }`, the
    /// current holder, or a waiter to an operator-facing name).
    pub fn label(&self, id: OriginId) -> Option<&str> {
        self.origins.get(&id).map(|o| o.label.as_str())
    }

    /// The write mode of a registered origin. A `Held` holder is held
    /// indefinitely and must not be released by a client detach (§6).
    pub fn write_mode(&self, id: OriginId) -> Option<WriteMode> {
        self.origins.get(&id).map(|o| o.write_mode)
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

    /// A reportable snapshot for the `state` verb (§6: arbitration, holder,
    /// waiters, per-origin purge counters, and the most recent steal). Observed
    /// state, disjoint from configuration (§15.8).
    pub fn snapshot(&self) -> LockSnapshot {
        LockSnapshot {
            arbitration: self.arbitration,
            holder: self.holder.and_then(|h| self.label(h)).map(str::to_owned),
            origins: self
                .origins
                .iter()
                .map(|(id, o)| OriginState {
                    origin: o.label.clone(),
                    write_mode: o.write_mode,
                    // Holder identity is the OriginId (the map key), not the label:
                    // two origins may share a label (concurrent `send`, §6), so a
                    // label compare would mark both as holder (LOCK-1).
                    holds_lock: self.holder == Some(*id),
                    purged: o.purged,
                })
                .collect(),
            waiters: self
                .waiters
                .iter()
                .filter_map(|w| self.label(*w))
                .map(str::to_owned)
                .collect(),
            last_steal: self.stolen.and_then(|(from, by)| {
                Some(StealRecord {
                    from: self.label(from)?.to_owned(),
                    by: self.label(by)?.to_owned(),
                })
            }),
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
    /// Origins waiting for the lock, in FIFO order (front = next to be granted).
    pub waiters: Vec<String>,
    /// The most recent steal, so an ousted holder can see what happened (§6).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_steal: Option<StealRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OriginState {
    pub origin: String,
    pub write_mode: WriteMode,
    pub holds_lock: bool,
    pub purged: u64,
}

/// A recorded steal: the lock was taken `from` one origin `by` another (§6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StealRecord {
    pub from: String,
    pub by: String,
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
        assert_eq!(lock.steal(OriginId(9)), Steal::ReadOnly);
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
        // Steal is a trivial success under free-for-all (there is no lock).
        assert_eq!(lock.steal(OriginId(2)), Steal::Stolen { previous: None });
    }

    #[test]
    fn purge_accounting_accumulates_per_origin() {
        let mut lock = on_demand();
        lock.record_purge(OriginId(2), 100);
        lock.record_purge(OriginId(2), 23);
        assert_eq!(lock.purged(OriginId(2)), 123);
        assert_eq!(lock.purged(OriginId(1)), 0);
    }

    // --- Waiter queue: FIFO grants, barge prevention, cancellation --------------

    #[test]
    fn grants_pass_to_the_queue_head_in_fifo_order() {
        let mut lock = EndpointLock::new(Arbitration::Exclusive);
        for i in 1..=3u64 {
            lock.register(OriginId(i), format!("o{i}"), WriteMode::OnDemand);
        }
        assert_eq!(lock.acquire(OriginId(1)), Acquire::Granted);
        // 2 then 3 join the queue (their plain acquire is denied first).
        assert!(matches!(lock.acquire(OriginId(2)), Acquire::Denied { .. }));
        lock.enqueue(OriginId(2));
        assert!(matches!(lock.acquire(OriginId(3)), Acquire::Denied { .. }));
        lock.enqueue(OriginId(3));
        assert_eq!(
            lock.waiters().collect::<Vec<_>>(),
            vec![OriginId(2), OriginId(3)]
        );

        // Holder releases: the head (2), not the barger (3), takes it next.
        assert!(lock.release(OriginId(1)));
        assert_eq!(
            lock.acquire(OriginId(3)),
            Acquire::Denied {
                held_by: OriginId(2)
            },
            "3 must not barge past the head 2 while the lock is free"
        );
        assert_eq!(lock.acquire(OriginId(2)), Acquire::Granted);
        assert_eq!(lock.waiters().collect::<Vec<_>>(), vec![OriginId(3)]);

        // 2 releases: 3 is now the head and takes it.
        assert!(lock.release(OriginId(2)));
        assert_eq!(lock.acquire(OriginId(3)), Acquire::Granted);
        assert!(lock.waiters().next().is_none());
    }

    #[test]
    fn dequeue_cancels_a_waiter_and_promotes_the_next() {
        let mut lock = EndpointLock::new(Arbitration::Exclusive);
        for i in 1..=3u64 {
            lock.register(OriginId(i), format!("o{i}"), WriteMode::OnDemand);
        }
        lock.acquire(OriginId(1));
        lock.enqueue(OriginId(2));
        lock.enqueue(OriginId(3));
        // The first waiter cancels (deadline / dropped connection).
        lock.dequeue(OriginId(2));
        assert_eq!(lock.waiters().collect::<Vec<_>>(), vec![OriginId(3)]);
        lock.release(OriginId(1));
        // 3 is now the head and is granted directly.
        assert_eq!(lock.acquire(OriginId(3)), Acquire::Granted);
    }

    #[test]
    fn enqueue_is_idempotent_and_skips_holder_and_never() {
        let mut lock = EndpointLock::new(Arbitration::Exclusive);
        lock.register(OriginId(1), "a", WriteMode::OnDemand);
        lock.register(OriginId(2), "spy", WriteMode::Never);
        lock.acquire(OriginId(1));
        lock.enqueue(OriginId(1)); // the holder does not queue behind itself
        lock.enqueue(OriginId(2)); // a never writer cannot wait
        assert!(lock.waiters().next().is_none());
        lock.register(OriginId(3), "c", WriteMode::OnDemand);
        lock.enqueue(OriginId(3));
        lock.enqueue(OriginId(3)); // idempotent
        assert_eq!(lock.waiters().collect::<Vec<_>>(), vec![OriginId(3)]);
    }

    // --- Steal ------------------------------------------------------------------

    #[test]
    fn steal_takes_the_lock_and_records_the_victim_without_touching_the_queue() {
        let mut lock = EndpointLock::new(Arbitration::Exclusive);
        for i in 1..=3u64 {
            lock.register(OriginId(i), format!("o{i}"), WriteMode::OnDemand);
        }
        lock.acquire(OriginId(1));
        lock.enqueue(OriginId(2));
        // 3 steals from 1: holder becomes 3, the queue [2] is untouched.
        assert_eq!(
            lock.steal(OriginId(3)),
            Steal::Stolen {
                previous: Some(OriginId(1))
            }
        );
        assert_eq!(lock.holder(), Some(OriginId(3)));
        assert!(lock.may_write(OriginId(3)));
        assert!(
            !lock.may_write(OriginId(1)),
            "the ousted holder loses write"
        );
        assert_eq!(lock.waiters().collect::<Vec<_>>(), vec![OriginId(2)]);
        let snap = lock.snapshot();
        assert_eq!(
            snap.last_steal,
            Some(StealRecord {
                from: "o1".into(),
                by: "o3".into()
            })
        );
        // When 3 releases, the queued waiter 2 is next (steal preserved the queue).
        assert!(lock.release(OriginId(3)));
        assert_eq!(lock.acquire(OriginId(2)), Acquire::Granted);
    }

    #[test]
    fn held_origin_reclaims_a_free_lock_ahead_of_on_demand_waiters() {
        // §6: the demux's held lock is permanent; a steal ousts it transiently, and
        // it reclaims the instant the lock frees — ahead of an on-demand waiter,
        // which waits indefinitely behind a held lock. reclaim_held bypasses the
        // FIFO head so a non-held writer can never inherit the mux lock.
        let mut lock = EndpointLock::new(Arbitration::Exclusive);
        lock.register(OriginId(1), "demux", WriteMode::Held); // acquires on attach
        lock.register(OriginId(2), "waiter", WriteMode::OnDemand);
        lock.register(OriginId(3), "stealer", WriteMode::OnDemand);
        assert_eq!(lock.holder(), Some(OriginId(1)), "held acquires on attach");

        lock.enqueue(OriginId(2)); // an on-demand origin queues behind the held lock
        lock.steal(OriginId(3)); // a steal ousts the held holder
        assert_eq!(lock.holder(), Some(OriginId(3)));
        assert!(
            !lock.reclaim_held(OriginId(1)),
            "cannot reclaim while the stealer holds it"
        );

        lock.release(OriginId(3)); // the stealer releases
        assert!(
            lock.reclaim_held(OriginId(1)),
            "held reclaims the free lock with priority"
        );
        assert_eq!(
            lock.holder(),
            Some(OriginId(1)),
            "held is back, ahead of the on-demand waiter"
        );
        assert_eq!(
            lock.waiters().collect::<Vec<_>>(),
            vec![OriginId(2)],
            "the on-demand waiter still waits behind the held lock"
        );
        assert!(
            !lock.reclaim_held(OriginId(2)),
            "a non-held origin cannot reclaim_held"
        );
    }

    #[test]
    fn generation_advances_on_grant_and_steal_for_lease_guarding() {
        let mut lock = on_demand();
        let g0 = lock.generation();
        lock.acquire(OriginId(1));
        let g1 = lock.generation();
        assert!(g1 > g0, "a fresh grant advances the generation");
        // Release + re-grant advances it again, so a lease timer captured at g1
        // will not match and cannot release the later grant.
        lock.release(OriginId(1));
        lock.acquire(OriginId(1));
        let g2 = lock.generation();
        assert!(g2 > g1);
        lock.steal(OriginId(2));
        assert!(lock.generation() > g2, "a steal is a fresh grant too");
    }

    #[test]
    fn renew_advances_the_generation_for_the_holder_only() {
        let mut lock = on_demand();
        lock.acquire(OriginId(1));
        let g = lock.generation();
        // Renewing the holder advances the generation (invalidating a prior lease
        // timer), so a re-armed lease's earlier timer can no longer fire (§6).
        assert_eq!(lock.renew(OriginId(1)), Some(g.wrapping_add(1)));
        assert_eq!(lock.generation(), g.wrapping_add(1));
        assert_eq!(lock.holder(), Some(OriginId(1)), "renew keeps the holder");
        // A non-holder cannot renew.
        assert_eq!(lock.renew(OriginId(2)), None);
        assert_eq!(
            lock.generation(),
            g.wrapping_add(1),
            "no bump for a non-holder"
        );
    }

    #[test]
    fn snapshot_marks_exactly_one_holder_with_duplicate_labels() {
        // Two distinct origins sharing a label — the reachable concurrent-`send`
        // case: the daemon registers every transient send origin as "send" with a
        // distinct OriginId (§6). The holder flag must track the id, not the label
        // string, or both same-labelled origins report as holder (LOCK-1).
        let mut lock = EndpointLock::new(Arbitration::Exclusive);
        lock.register(OriginId(1), "send", WriteMode::OnDemand);
        lock.register(OriginId(2), "send", WriteMode::OnDemand);
        assert_eq!(lock.acquire(OriginId(1)), Acquire::Granted);
        // The second same-labelled origin is denied and queues (the FIFO waiter).
        assert!(matches!(lock.acquire(OriginId(2)), Acquire::Denied { .. }));

        let snap = lock.snapshot();
        assert_eq!(snap.holder.as_deref(), Some("send"));
        assert_eq!(
            snap.origins.len(),
            2,
            "both same-labelled origins are reported"
        );
        let holders = snap.origins.iter().filter(|o| o.holds_lock).count();
        assert_eq!(
            holders, 1,
            "exactly one origin holds the lock despite the shared label"
        );
    }

    // --- Property: at most one holder, and may_write matches the holder --------

    #[derive(Debug, Clone)]
    enum Op {
        Acquire(u8),
        Release(u8),
        Detach(u8),
        Reattach(u8),
        Enqueue(u8),
        Dequeue(u8),
        Steal(u8),
    }

    fn op_strategy() -> impl Strategy<Value = Op> {
        prop_oneof![
            (0u8..4).prop_map(Op::Acquire),
            (0u8..4).prop_map(Op::Release),
            (0u8..4).prop_map(Op::Detach),
            (0u8..4).prop_map(Op::Reattach),
            (0u8..4).prop_map(Op::Enqueue),
            (0u8..4).prop_map(Op::Dequeue),
            (0u8..4).prop_map(Op::Steal),
        ]
    }

    proptest! {
        /// Under any interleaving of acquire/release/detach/reattach/enqueue/
        /// dequeue/steal across four on-demand origins, the exclusive lock never
        /// has two holders, `may_write` is true for exactly the current holder, a
        /// waiter is never also the holder, and the generation is monotonic (§6).
        #[test]
        fn prop_exclusive_invariants(ops in prop::collection::vec(op_strategy(), 0..96)) {
            let mut lock = EndpointLock::new(Arbitration::Exclusive);
            let mut attached = [true; 4];
            for i in 0..4u8 {
                lock.register(OriginId(i as u64), format!("o{i}"), WriteMode::OnDemand);
            }
            let mut last_gen = lock.generation();
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
                    Op::Enqueue(i) => { lock.enqueue(OriginId(i as u64)); }
                    Op::Dequeue(i) => { lock.dequeue(OriginId(i as u64)); }
                    Op::Steal(i) => { if attached[i as usize] { lock.steal(OriginId(i as u64)); } }
                }
                // Invariant 1: at most one origin may write.
                let writers: Vec<u64> = (0..4u64).filter(|&i| lock.may_write(OriginId(i))).collect();
                prop_assert!(writers.len() <= 1, "two writers: {writers:?}");
                // Invariant 2: whoever may write is exactly the holder, and the
                // holder is always a still-attached origin.
                match lock.holder() {
                    Some(h) => {
                        prop_assert_eq!(&writers, &vec![h.0]);
                        prop_assert!(attached[h.0 as usize], "holder detached but still holds");
                        // Invariant 3: the holder is never simultaneously queued.
                        prop_assert!(!lock.waiters().any(|w| w == h), "holder is also a waiter");
                    }
                    None => prop_assert!(writers.is_empty()),
                }
                // Invariant 4: the generation never decreases.
                let g = lock.generation();
                prop_assert!(g >= last_gen, "generation went backwards");
                last_gen = g;
            }
        }
    }

    // --- Property: held priority outranks the on-demand FIFO across schedules ---

    #[derive(Debug, Clone)]
    enum HeldOp {
        Acquire(u8),
        Release(u8),
        Detach(u8),
        Reattach(u8),
        Enqueue(u8),
        Dequeue(u8),
        Steal(u8),
        ReclaimHeld(u8),
        Renew(u8),
    }

    fn held_op_strategy() -> impl Strategy<Value = HeldOp> {
        prop_oneof![
            (0u8..4).prop_map(HeldOp::Acquire),
            (0u8..4).prop_map(HeldOp::Release),
            (0u8..4).prop_map(HeldOp::Detach),
            (0u8..4).prop_map(HeldOp::Reattach),
            (0u8..4).prop_map(HeldOp::Enqueue),
            (0u8..4).prop_map(HeldOp::Dequeue),
            (0u8..4).prop_map(HeldOp::Steal),
            (0u8..4).prop_map(HeldOp::ReclaimHeld),
            (0u8..4).prop_map(HeldOp::Renew),
        ]
    }

    proptest! {
        /// A `held` origin (id 0, the demux) mixed with three on-demand origins
        /// under any interleaving of acquire/release/detach/reattach/enqueue/
        /// dequeue/steal/reclaim_held/renew. Alongside the single-holder /
        /// may_write / holder-never-queued / monotonic-generation invariants, this
        /// fuzzes held priority (§6/§15.23): while the held origin is attached, an
        /// on-demand `acquire` can never win the lock — it defers so the demux's
        /// reclaim outranks the FIFO queue, whatever the schedule. It also checks
        /// the snapshot flags exactly the holder by OriginId (LOCK-1).
        #[test]
        fn prop_held_priority_invariants(ops in prop::collection::vec(held_op_strategy(), 0..96)) {
            let mode = |i: u8| if i == 0 { WriteMode::Held } else { WriteMode::OnDemand };
            let mut lock = EndpointLock::new(Arbitration::Exclusive);
            let mut attached = [true; 4];
            for i in 0..4u8 {
                lock.register(OriginId(i as u64), format!("o{i}"), mode(i));
            }
            let mut last_gen = lock.generation();
            for op in ops {
                match op {
                    HeldOp::Acquire(i) => {
                        let result = lock.acquire(OriginId(i as u64));
                        // Held priority: an on-demand acquire never wins while the
                        // held origin (0) is attached — it defers to the demux's
                        // reclaim rather than racing it (daemon-arbitration-1).
                        if i != 0 && attached[0] {
                            prop_assert_ne!(
                                result,
                                Acquire::Granted,
                                "on-demand {} was granted while held origin 0 is attached",
                                i
                            );
                        }
                    }
                    HeldOp::Release(i) => { lock.release(OriginId(i as u64)); }
                    HeldOp::Detach(i) => {
                        lock.unregister(OriginId(i as u64));
                        attached[i as usize] = false;
                    }
                    HeldOp::Reattach(i) => {
                        if !attached[i as usize] {
                            lock.register(OriginId(i as u64), format!("o{i}"), mode(i));
                            attached[i as usize] = true;
                        }
                    }
                    HeldOp::Enqueue(i) => { lock.enqueue(OriginId(i as u64)); }
                    HeldOp::Dequeue(i) => { lock.dequeue(OriginId(i as u64)); }
                    HeldOp::Steal(i) => { if attached[i as usize] { lock.steal(OriginId(i as u64)); } }
                    HeldOp::ReclaimHeld(i) => { lock.reclaim_held(OriginId(i as u64)); }
                    HeldOp::Renew(i) => { lock.renew(OriginId(i as u64)); }
                }

                // Invariant 1: at most one origin may write.
                let writers: Vec<u64> = (0..4u64).filter(|&i| lock.may_write(OriginId(i))).collect();
                prop_assert!(writers.len() <= 1, "two writers: {:?}", writers);
                // Invariant 2: whoever may write is exactly the holder, and the
                // holder is always still attached and never simultaneously queued.
                match lock.holder() {
                    Some(h) => {
                        prop_assert_eq!(&writers, &vec![h.0]);
                        prop_assert!(attached[h.0 as usize], "holder detached but still holds");
                        prop_assert!(!lock.waiters().any(|w| w == h), "holder is also a waiter");
                    }
                    None => prop_assert!(writers.is_empty()),
                }
                // Invariant 3: the snapshot flags exactly the holder by OriginId —
                // one `holds_lock` when held, none when free (LOCK-1).
                let snap = lock.snapshot();
                let flagged = snap.origins.iter().filter(|o| o.holds_lock).count();
                let expected = usize::from(lock.holder().is_some());
                prop_assert_eq!(flagged, expected, "holds_lock count != holder presence");
                // Invariant 4: the generation never decreases.
                let g = lock.generation();
                prop_assert!(g >= last_gen, "generation went backwards");
                last_gen = g;
            }
        }
    }
}
