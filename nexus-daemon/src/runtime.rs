//! The data-plane runtime (design §5): the endpoint-keyed channel plan
//! ([`Wiring`]) that connects every node's boundary tasks.
//!
//! The §5 boundary policies are realized with bounded `tokio::sync::mpsc`
//! channels between node tasks — the channel *is* the "bounded buffering where
//! configured" a boundary owns:
//!
//! * **Hostward** (a host-facing producer → its consumers) is lossy at the
//!   boundary: the producer `try_send`s a chunk to each attached consumer and
//!   drops on a full channel (a slow consumer costs only itself, §5), counting
//!   the loss in the shared [`DropCounters`] so it stays located and attributable.
//! * **Targetward** (a writing origin → its host endpoint) is backpressured to
//!   the origin: the origin `send().await`s into the host endpoint's single
//!   arbitrated channel; a full channel suspends the origin and nothing is
//!   dropped (§5).
//!
//! The topology is endpoint-keyed (§3, [`Wiring`]), not two-layer: a serial fans
//! out to PTYs and logs, and interior codec/exec/leg nodes (§7.4/§7.5/§7.6) are
//! each a target-facing consumer on their multiplexed side and N host-facing
//! producers on their channels. The pure `nexus_core::data` contracts remain the
//! property-tested spec of these same boundary semantics.
//!
//! Readiness is driven by `poll(2)`, *never* `tokio::io::unix::AsyncFd`: on a pty
//! master, `AsyncFd`'s epoll readiness spuriously and persistently fires
//! "readable" and busy-loops the single-threaded runtime (§15.18). Two shapes,
//! per §15.19 (the hybrid data plane the phase-3 benchmark settled):
//!
//! * Low-rate paths (targetward PTY→serial, PTY presence/termios) stay **async
//!   tasks** using a non-blocking `poll(2)` (`sys::poll_ready`) with an
//!   [`ACTIVE_POLL`]→[`IDLE_POLL`] backoff — quiescent fds settle onto the cheap
//!   5ms poll (~0.06% CPU each), active ones recheck promptly.
//! * High-throughput paths (the serial hostward reader, the PTY hostward writer)
//!   run on **dedicated blocking threads** using a *blocking* `poll(2)`
//!   ([`sys::poll_blocking`]) — the kernel wakes them the instant the fd is ready,
//!   so they move data at line rate (a non-blocking poll-plus-sleep on the
//!   runtime thread capped this at ~1 MB/s) and park at zero CPU otherwise. This
//!   is the hatch §15.18 reserved and §15.19 cashed. Cross-thread counters are
//!   therefore atomic ([`DropCounters`]).

use std::cell::Cell;
use std::collections::HashMap;
use std::os::fd::RawFd;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use codec_api::{Event, MAX_FRAME_SIZE, encode};
use nexus_core::Chunk;
use nexus_core::config::{GraphConfig, NodeConfig};
use nexus_core::graph::{Arbitration, EndpointAddr, Facing, WriteMode};
use nexus_core::lock::{EndpointLock, OriginId};
use nexus_rpc::Notification;
use nix::poll::PollFlags;
use serde_json::json;
use tokio::sync::futures::Notified;
use tokio::sync::{Notify, broadcast, mpsc};

use crate::cell::CriticalCell;
use crate::tap::{SharedTapHub, TAP_FEED_CAP, TapFeed, TapHub};
use nexus_sys as sys;

/// A shared, single-threaded handle to one host-facing endpoint's write lock
/// (§6): the pure [`EndpointLock`] state machine plus the two async signals the
/// two-lane control plane needs (§15.20) — a [`Notify`] that wakes queued waiters
/// to re-attempt, and the `subscribe` broadcast so every lock transition emits an
/// immediate id-less notification (§10). All mutation is on the one runtime
/// thread, so the inner [`CriticalCell`] needs no synchronization; and because its
/// state is reachable only inside a synchronous `with`/`with_mut` closure, a borrow
/// *cannot* cross an `.await` — the §15.20 tripwire is a compile-shape fact, not a
/// review rule (§16.2).
pub struct LockCell {
    endpoint: String,
    lock: CriticalCell<EndpointLock>,
    wake: Notify,
    notifier: broadcast::Sender<Notification>,
    /// Set when the endpoint is torn down or removed while the cell may still be
    /// kept alive by a parked waiter's `Rc` clone (§6/§15.20). A woken waiter that
    /// sees this leaves the queue with a defined error instead of re-parking.
    closed: Cell<bool>,
}

impl LockCell {
    pub fn new(
        endpoint: impl Into<String>,
        lock: EndpointLock,
        notifier: broadcast::Sender<Notification>,
    ) -> Self {
        LockCell {
            endpoint: endpoint.into(),
            lock: CriticalCell::new(lock),
            wake: Notify::new(),
            notifier,
            closed: Cell::new(false),
        }
    }

    /// Mark the cell closed (its endpoint is gone) and wake any parked waiters so
    /// they observe the closure and return the defined teardown error (§6/§15.20).
    pub fn close(&self) {
        self.closed.set(true);
        self.wake.notify_waiters();
    }

    /// Whether the endpoint behind this cell has been torn down or removed.
    pub fn is_closed(&self) -> bool {
        self.closed.get()
    }

    /// Run `f` against the state machine in a synchronous critical section (§16.2):
    /// the borrow cannot escape the closure, so it can never cross an `.await`
    /// (§15.20) — the tripwire is now a compile-shape fact.
    pub fn with<R>(&self, f: impl FnOnce(&EndpointLock) -> R) -> R {
        self.lock.with(f)
    }

    pub fn with_mut<R>(&self, f: impl FnOnce(&mut EndpointLock) -> R) -> R {
        self.lock.with_mut(f)
    }

    /// Wake every suspended waiter so the FIFO head re-attempts `acquire` in a
    /// fresh critical section (§15.20). Called on every release path.
    pub fn wake_waiters(&self) {
        self.wake.notify_waiters();
    }

    /// A future that completes on the next [`Self::wake_waiters`]. The wait loop
    /// enables it *before* the acquire check, so a wake landing between the check
    /// and the await is not lost.
    pub fn notified(&self) -> Notified<'_> {
        self.wake.notified()
    }

    /// Emit an immediate id-less `lock` notification to subscribers on a lock
    /// transition (§10: acquire, release, steal, lease expiry, detach-release). A
    /// no-op when nobody is *connected*. Must be called with no outstanding borrow.
    ///
    /// This deliberately gates on `receiver_count()` — connections — where the 5 Hz
    /// state snapshot gates on the daemon's exact subscriber tally (OBS-1). The two
    /// answer the same question differently on purpose: the snapshot is periodic and
    /// serialises the whole graph, so building it for a merely-connected client was
    /// real waste; a lock transition is human-scale and rare, and the tally lives on
    /// the `Daemon` rather than here, so threading it into every endpoint's lock
    /// would buy nothing. A connected-but-unsubscribed receiver simply lags.
    pub fn emit_change(&self) {
        if self.notifier.receiver_count() == 0 {
            return;
        }
        let snapshot = self.lock.with(|l| l.snapshot());
        let _ = self.notifier.send(Notification::new(
            "lock",
            Some(json!({ "endpoint": self.endpoint, "lock": snapshot })),
        ));
    }
}

/// A shared, single-threaded handle to one endpoint's [`LockCell`].
pub type SharedLock = Rc<LockCell>;

/// Park-and-retry an acquisition on `lock` until `attempt` settles it, or the
/// endpoint is torn down (`None`). **The** implementation of the §15.20 lock-wait
/// discipline, whose five clauses were hand-written three times before SIMPB-2:
///
/// 1. Re-check [`LockCell::is_closed`] at the top of *every* iteration, including
///    right after a wake — teardown calls `close()` (which wakes) before the daemon's
///    maps clear, so a waiter leaves the queue with a defined outcome instead of
///    re-parking on an endpoint that is gone.
/// 2. Create the [`Notified`] and `enable()` it **before** the attempt. This is the
///    subtle clause and the reason this function exists: `Notify::notify_waiters`
///    stores no permit, so a writer that checks first and registers interest second
///    loses a wake landing in between and parks forever on a *free* lock — the
///    stranded-waiter class §15.20 and §15.23 were written to close. Three separate
///    comments used to be all that upheld it.
/// 3. Attempt inside one synchronous `with_mut` critical section. No borrow can cross
///    the await below, because [`CriticalCell`] makes that a compile-shape fact
///    (§16.2, invariant 5) — `attempt` cannot leak the `&mut EndpointLock`.
/// 4. `Some(_)` settles and returns; the caller decides what a *fresh* grant costs
///    (only a fresh one is a lock transition that notifies, §10).
/// 5. `None` parks on the lock's `Notify` and loops.
///
/// A deadline is deliberately *not* a parameter: the one caller that has one races
/// this whole future against a single `sleep_until` and lets its own cancel-safety
/// guard dequeue on the drop, which is both simpler and one timer instead of one per
/// iteration.
pub(crate) async fn await_grant<T>(
    lock: &SharedLock,
    mut attempt: impl FnMut(&mut EndpointLock) -> Option<T>,
) -> Option<T> {
    loop {
        if lock.is_closed() {
            return None;
        }
        let notified = lock.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if let Some(settled) = lock.with_mut(&mut attempt) {
            return Some(settled);
        }
        notified.await;
    }
}

/// How a parked acquisition settled, for the two callers that only need to know
/// whether they may write now (§6). The third caller — the `lock --wait` verb — needs
/// a finer vocabulary (it must tell a timeout from a teardown, and `write = never`
/// from an unregistered origin) and maps [`EndpointLock::acquire`] to its own type
/// through the same [`await_grant`] shell.
pub(crate) enum Grant {
    /// A fresh grant: this origin now holds the lock, and *only* this outcome is a
    /// lock transition, so only it notifies (§10).
    Fresh,
    /// It already held the lock: an idempotent no-op, no notification.
    AlreadyHeld,
    /// It can never be granted, so waiting is pointless. Either the origin is
    /// `write = never`, or it is no longer registered on this endpoint because
    /// `disconnect`/`remove-node` took its edge (§15.35). The second is the clause
    /// whose absence would wedge a pump forever: an unregistered id satisfies neither
    /// `may_write` nor `reclaim_held` nor `acquire`, while the lock itself stays open
    /// (the endpoint is fine — it is *this* origin that left), so the pump would park
    /// holding the chunk it had already taken and a later `connect` could not revive
    /// it, because the new edge gets a new id and nothing wakes the old wait.
    Refused,
}

/// Park until this origin may write, reporting whether it may (§6/§15.20).
///
/// [`await_grant`] plus the shared post-grant step both write-gate pumps need: emit
/// the lock-change notification on a **fresh** grant and on no other outcome. The
/// callers differ only in the one line inside the critical section — held-priority
/// reclaim ([`reacquire_held`]) versus an on-demand `acquire`-or-enqueue (the leg's
/// `ensure_acquired`) — which is exactly the parameter.
pub(crate) async fn await_write_grant(
    lock: &SharedLock,
    attempt: impl FnMut(&mut EndpointLock) -> Option<Grant>,
) -> bool {
    match await_grant(lock, attempt).await {
        Some(Grant::Fresh) => {
            lock.emit_change();
            true
        }
        Some(Grant::AlreadyHeld) => true,
        // Refused outright, or the endpoint was torn down under the wait.
        Some(Grant::Refused) | None => false,
    }
}

/// Ensure `id` holds its endpoint's write lock, re-acquiring through the FIFO
/// queue if a `send --steal` transiently ousted the held origin (§6 held
/// priority). Returns `false` if the endpoint was torn down or this origin's edge
/// went away. The fast path (the normal held case) is a single synchronous borrow;
/// the slow path is the shared [`await_write_grant`] park, which holds no borrow
/// across its await (§15.20). Shared by the in-process codec and the exec codec —
/// the two held-origin targetward gates.
pub(crate) async fn reacquire_held(lock: &SharedLock, id: OriginId) -> bool {
    if lock.with(|g| g.may_write(id)) {
        return true; // already holds it
    }
    // A held origin never enqueues: held priority is a rule *inside* `acquire`
    // /`reclaim_held` (§15.23), so it takes a free lock whatever the queue holds.
    // `reclaim_held` returns a bare bool and so cannot distinguish "denied now" from
    // "never registered"; the explicit `write_mode` probe is how this attempt reports
    // [`Grant::Refused`] where the on-demand callers get it from
    // [`Acquire::Unregistered`](nexus_core::lock::Acquire::Unregistered).
    await_write_grant(lock, |g| {
        if g.write_mode(id).is_none() {
            Some(Grant::Refused)
        } else if g.may_write(id) {
            Some(Grant::AlreadyHeld)
        } else if g.reclaim_held(id) {
            Some(Grant::Fresh)
        } else {
            None
        }
    })
    .await
}

/// The maximum envelope payload per frame carrying `channel`: [`MAX_FRAME_SIZE`]
/// minus the envelope header (1 type byte + 2 channel-length bytes + the channel
/// id), floored at 1 so a pathologically long channel id still makes progress.
pub(crate) fn frame_payload_cap(channel: &str) -> usize {
    MAX_FRAME_SIZE.saturating_sub(3 + channel.len()).max(1)
}

/// Split `total` bytes into consecutive `(off, end)` payload ranges, each at most
/// [`frame_payload_cap`] wide (§15.24). This is the **one** shared helper where the
/// targetward fragmentation boundary is computed — the error-prone off-by-one /
/// underflow logic §15.27 moved into a single place so every targetward framer
/// fragments on identical boundaries: the envelope framers via [`data_frames`]
/// (which encodes each range through the fixed envelope), and the in-process codec
/// via its pluggable-`mux` loop (which frames each range through the configured
/// transform, then parks and re-acquires the held lock per piece). A chunk larger
/// than one frame — a full device read (`READ_BUF == MAX_FRAME_SIZE` overflows the
/// header) or an arbitrary-length `send` line — is thereby fragmented rather than
/// dropped (§5 no-drop / all-loss-counted).
pub(crate) fn frame_ranges(
    channel: &str,
    total: usize,
) -> impl Iterator<Item = (usize, usize)> + use<> {
    let cap = frame_payload_cap(channel);
    let mut off = 0usize;
    std::iter::from_fn(move || {
        if off >= total {
            return None;
        }
        let start = off;
        let end = (off + cap).min(total);
        off = end;
        Some((start, end))
    })
}

/// One item of the [`data_frames`] fragmentation stream.
///
/// The enum exists so the *residual* cannot be dropped on the floor: a caller
/// matching on this has to say what happens to [`Self::Residual`], where the
/// previous `map_while` shape simply ended the iteration and truncated the chunk
/// in silence (RV-9). Invariant 3's stated shape is "fragment, never
/// skip-on-error, **count any residual**" — this is the type that enforces the
/// third clause.
pub(crate) enum DataFrame {
    /// A framed piece: `(payload_len, frame_bytes)`. `payload_len` is the number
    /// of *source* bytes this frame carries, for the caller's throughput counters.
    Piece(usize, Vec<u8>),
    /// The unframable tail, in source bytes: `encode` refused a piece, so this
    /// many bytes of the chunk were never framed. Terminal — the iterator ends
    /// after yielding it — and the caller must count it (§5 all-loss-counted).
    Residual(usize),
}

/// Split `bytes` into consecutive envelope [`Event::data`] frames on `channel` —
/// the envelope-framing wrapper over the shared [`frame_ranges`] boundary helper
/// (§15.24). The peer/child reassembles per channel. Shared by the leg's write half
/// and the exec codec's stdin feed (§5 no-drop / all-loss-counted).
///
/// `encode` is infallible for any sane channel id (each range provably fits the
/// frame bound once the header is added); it can only refuse when the channel
/// identity itself is long enough that `frame_payload_cap`'s floor-at-1 still
/// overflows [`MAX_FRAME_SIZE`]. That is unreachable through configuration —
/// `nexus_core::graph::MAX_NAME_LEN` = 256 bounds every channel identity, on every
/// graph-creating path (§3) — but the arm is kept because invariant 3's third
/// clause forbids a silent truncation if that ever changes, so the tail is
/// reported as [`DataFrame::Residual`] instead of vanishing.
pub(crate) fn data_frames<'a>(
    channel: &'a str,
    bytes: &'a Chunk,
) -> impl Iterator<Item = DataFrame> + 'a {
    let total = bytes.len();
    let mut ranges = frame_ranges(channel, total);
    let mut done = false;
    std::iter::from_fn(move || {
        if done {
            return None;
        }
        let Some((off, end)) = ranges.next() else {
            done = true;
            return None;
        };
        let mut frame = Vec::new();
        if encode(&Event::data(channel, bytes.slice(off..end)), &mut frame).is_err() {
            // Stop fragmenting this chunk, but hand the caller the exact number of
            // source bytes that never reached a frame so it can be attributed.
            done = true;
            return Some(DataFrame::Residual(total - off));
        }
        Some(DataFrame::Piece(end - off, frame))
    })
}

/// A monotonic byte counter the data plane charges lost bytes to — the daemon's
/// **one** "charge `n` bytes of loss" verb, in both directions (§5: all loss is
/// counted where it happens).
///
/// Two shapes exist in the daemon and both must work through one helper: the serial
/// reader's `Arc<AtomicU64>` (its producer runs on a dedicated blocking thread,
/// §15.19, so the counter has to be `Send`/`Sync`) and every interior node's
/// single-threaded `Cell<u64>` (reached through an `Rc` by deref, so a node's
/// `Rc<Cell<u64>>` needs no impl of its own). Taking the accounting as a trait
/// object is what lets the five former hand-rolled fan-out loops collapse into one
/// (§16, F1) without either side reshaping its counter.
///
/// **Use `counter.add(n)`, never `counter.set(counter.get() + n)`** (SIMP-2). The
/// longhand had been written out at ~20 targetward sites across five pumps while
/// this trait already existed for the hostward half; a rule spelled out per site is
/// a rule a new exit can silently omit, which is the shape of LEG-1, LEG-2, LEGD-2
/// and LOGQ-1. *Credit* counters (`delivered_hostward`, `accepted_targetward`) stay
/// explicit on purpose: a missed credit under-reports throughput, while a missed
/// charge hides data loss, and only the second is worth a type.
pub(crate) trait LossCounter {
    fn add(&self, n: u64);
}

impl LossCounter for AtomicU64 {
    fn add(&self, n: u64) {
        self.fetch_add(n, Ordering::Relaxed);
    }
}

impl LossCounter for Cell<u64> {
    fn add(&self, n: u64) {
        self.set(self.get() + n);
    }
}

/// One attached hostward consumer, tagged by the **target endpoint** the edge
/// reaches (§15.35).
///
/// The tag is what makes an edge removable: §4 rule 2 gives a target-facing
/// endpoint at most one edge, so a target address names exactly one sink in a
/// producer's fan-out and `disconnect` needs no separate edge identity.
pub(crate) struct AttachedSink {
    pub target: EndpointAddr,
    pub tx: mpsc::Sender<Chunk>,
    pub counters: Arc<DropCounters>,
}

/// One host-facing endpoint's **live** fan-out list (§15.35).
///
/// A producer holds this for its whole life and re-reads it per chunk, so
/// `connect` and `disconnect` join and leave a running fan-out without restarting
/// the producer — which matters because restarting a serial node would drop
/// `TIOCEXCL` and toggle DTR on the board behind it.
///
/// `Mutex` rather than a single-threaded cell because the serial reader runs on a
/// dedicated blocking thread (§15.19, invariant 2) while the control plane runs on
/// the runtime thread. The cost is one uncontended lock per *chunk* — a 64 KiB
/// device read, not a byte — which is noise beside the read it accompanies; the
/// firehose guard (`p3_firehose`) is the standing proof.
pub struct FanOutList {
    sinks: std::sync::Mutex<Vec<AttachedSink>>,
}

/// A shared handle to one host-facing endpoint's [`FanOutList`].
pub type SharedFanOut = Arc<FanOutList>;

impl FanOutList {
    pub(crate) fn new() -> SharedFanOut {
        Arc::new(FanOutList {
            sinks: std::sync::Mutex::new(Vec::new()),
        })
    }

    /// Add one consumer. Called from `Wiring::build` at load and from `connect` on
    /// a running graph — the same call, so a mid-stream connect joins the fan-out
    /// exactly where a loaded one would have.
    pub(crate) fn attach(&self, sink: AttachedSink) {
        self.lock().push(sink);
    }

    /// Remove the consumer reached through `target`, returning whether one went.
    ///
    /// Dropping the sink drops the only sender on that edge's hostward channel, so
    /// the consumer's pump drains what is buffered and then sees the channel close
    /// — the detach costs no bytes and needs no second signal.
    pub(crate) fn detach(&self, target: &EndpointAddr) -> bool {
        let mut sinks = self.lock();
        let before = sinks.len();
        sinks.retain(|s| &s.target != target);
        sinks.len() != before
    }

    /// Whether any consumer is attached — the cheap check a producer makes before
    /// deciding a read is worth doing at all.
    pub(crate) fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// Broadcast one hostward chunk to every attached consumer, accounting for all
    /// loss (§5). The single hostward fan-out: see [`fan_out`].
    pub(crate) fn broadcast(&self, chunk: &Chunk, unattached: &dyn LossCounter) -> FanOut {
        fan_out(chunk, &self.lock(), unattached)
    }

    /// The sink list. A poisoned mutex cannot lose data here — every operation is a
    /// `Vec` push/retain with no invariant spanning them — so the guard is taken
    /// through the poison rather than panicking a data-plane thread with it.
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<AttachedSink>> {
        self.sinks.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// One target-facing endpoint's **live** edge binding (§15.35).
///
/// Every target-facing endpoint owns one for its whole life; `connect` fills it
/// and `disconnect` clears it, and the endpoint's already-running task re-reads it
/// each iteration. Nothing is spawned or aborted, which is the point: the
/// alternative — restarting the task — would drop the targetward *receiver* out
/// from under senders that are still live in `GraphState::endpoint_targetward` and
/// in every writer origin, which is MAP-1's chain exactly (§15.8, notes §3.x).
#[derive(Default)]
pub struct TargetEdge {
    /// The host endpoint's lock and the id this origin is registered under (§6).
    /// Present for **every** attached edge, `never` included — the lock registers
    /// every origin so `state` can list it, so every one of them must be
    /// unregisterable. Tracking this only for writable edges is how a `never`
    /// origin outlives the edge that created it and lingers in `state` as a
    /// phantom.
    pub registered: Option<(SharedLock, OriginId)>,
    /// The targetward path into the host endpoint. Present only when the effective
    /// write mode is not `never`, so a read-only edge is registered but has no
    /// writer.
    pub writer: Option<mpsc::Sender<Chunk>>,
    /// Whether an edge is attached at all. Distinct from `writer.is_some()`
    /// because a `never` edge is a real attachment that simply cannot write, and
    /// it is *this* flag — not the writer — that decides whether an interior node
    /// reports `active` or `waiting` for want of an upstream (§7.5, §15.8).
    pub attached: bool,
}

/// One target-facing endpoint's edge binding plus the signal that it changed
/// (§15.35).
///
/// The signal is what lets an *unattached* endpoint apply §5's targetward
/// contract properly. Three states, three behaviours, and the difference between
/// the last two is why `attached` exists separately from `origin`:
///
/// * **Attached and writable** — forward, gated on the lock (§6).
/// * **Attached, not writable** (`write_mode = "never"`, the declared read-only
///   edge) — drain and count. Parking would wedge a writer forever on a
///   configuration that says, permanently, that its bytes go nowhere; the node
///   must be *inert*, not destructive (MAP-1, §15.8).
/// * **Not attached** (`disconnect`, or never connected) — **park**, consuming
///   nothing, so the endpoint's bounded targetward channel fills and its writers
///   backpressure. Targetward is the direction §5 forbids dropping on, so an
///   operator detaching an edge must stall the writers, not shed their bytes —
///   the same thing a steal does to a held origin. [`Self::changed`] wakes the
///   pump when `connect` fills the slot again.
pub struct EdgeSlot {
    state: CriticalCell<TargetEdge>,
    changed: Notify,
}

/// A shared handle to one target-facing endpoint's [`EdgeSlot`].
pub type SharedTargetEdge = Rc<EdgeSlot>;

impl EdgeSlot {
    pub fn with<R>(&self, f: impl FnOnce(&TargetEdge) -> R) -> R {
        self.state.with(f)
    }

    /// The writable binding — sender, lock, origin id — or `None` when this edge
    /// cannot write (read-only, or nothing attached). The one place the three parts
    /// are assembled, so a caller cannot pair a writer with the wrong id.
    pub fn origin(&self) -> Option<WritableOrigin> {
        self.state.with(|e| match (&e.writer, &e.registered) {
            (Some(tx), Some((lock, id))) => Some((tx.clone(), lock.clone(), *id)),
            _ => None,
        })
    }

    /// Mutate the binding and wake whoever is parked on it. Every writer goes
    /// through here, so a fill can never be missed by a parked pump.
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut TargetEdge) -> R) -> R {
        let out = self.state.with_mut(f);
        self.changed.notify_waiters();
        out
    }

    /// A future that completes on the next binding change. Enable it *before*
    /// re-reading the slot, or a `connect` landing in between is lost and the pump
    /// parks forever on an endpoint that is now wired.
    pub fn changed(&self) -> Notified<'_> {
        self.changed.notified()
    }
}

impl TargetEdge {
    /// An edge slot that is **attached but not writable** — the declared read-only
    /// edge (`write_mode = "never"`, §7.3/§7.8). Distinct from [`Self::new`], which
    /// is *unattached* and makes its pump park: the two states behave differently
    /// on purpose, so a test that means one must not reach for the other.
    #[cfg(test)]
    pub(crate) fn read_only() -> SharedTargetEdge {
        let slot = TargetEdge::new();
        slot.with_mut(|e| e.attached = true);
        slot
    }

    // `TargetEdge` is only ever handed out behind an `Rc<EdgeSlot>` — the whole point
    // is that producer and control plane share one — so a bare `Self` would have no
    // caller.
    #[allow(clippy::new_ret_no_self)]
    pub(crate) fn new() -> SharedTargetEdge {
        Rc::new(EdgeSlot {
            state: CriticalCell::new(TargetEdge::default()),
            changed: Notify::new(),
        })
    }
}

/// A writable targetward binding: the host endpoint's sender, its write lock, and
/// the [`OriginId`] this origin is registered under (§6/§15.35).
///
/// Named because it is assembled in exactly one place ([`EdgeSlot::origin`], and so
/// [`await_origin`]) and consumed by the shared send helpers below — a caller that
/// never takes the three apart cannot pair a writer with the wrong id.
pub type WritableOrigin = (mpsc::Sender<Chunk>, SharedLock, OriginId);

/// Wait for `edge` to carry a writable origin, returning it (§15.35).
///
/// The shared park used by every interior targetward pump. `None` means the edge
/// is attached but read-only — the caller drains and counts rather than waiting,
/// because that configuration will never become writable on its own.
///
/// Structurally a sibling of [`await_grant`] — the same enable-before-read
/// lost-wakeup discipline — but over a *different* signal: the edge slot's
/// `changed`, not the endpoint lock's `wake`. It stays separate because it has no
/// `is_closed` clause and no critical section to attempt inside; the two are not one
/// function, but a change to the discipline belongs in both.
pub(crate) async fn await_origin(edge: &SharedTargetEdge) -> Option<WritableOrigin> {
    loop {
        // Enable the wake before reading, so a `connect` between the read and the
        // await is not lost.
        let changed = edge.changed();
        tokio::pin!(changed);
        changed.as_mut().enable();
        let origin = edge.origin();
        if origin.is_some() || edge.with(|e| e.attached) {
            return origin;
        }
        changed.await;
    }
}

/// Source bytes a targetward step did **not** deliver, which the caller has to
/// charge to a [`LossCounter`] (§5, invariant 3's third clause).
///
/// The type exists for the same reason [`DataFrame::Residual`] does. Targetward is
/// the direction §5 forbids dropping on, so *every* non-delivery exit of *every*
/// pump has to reach a counter — and "has to" was a review convention rather than a
/// compiler rule, hand-written at ~20 sites across five pumps and twice shipped with
/// an exit that charged nothing (CODEXEC-2's `reacquire_held` arm; the pty's
/// held-payload handoff). `#[must_use]` plus a single consuming method makes a
/// forgotten charge a warning at the site that forgot it, which is what
/// [`DataFrame`] already does for the framing half (SIMP-2).
#[must_use = "targetward loss must be charged where it happened (§5): call `charge`"]
pub(crate) struct TargetwardLoss(u64);

impl TargetwardLoss {
    /// Everything arrived (or there was nothing to send).
    fn none() -> TargetwardLoss {
        TargetwardLoss(0)
    }

    /// `n` source bytes never reached the endpoint.
    fn of(n: u64) -> TargetwardLoss {
        TargetwardLoss(n)
    }

    /// Charge whatever did not arrive to `lost`, reporting whether the chunk was
    /// delivered in full. The only way to consume the value, so `#[must_use]` above
    /// cannot be satisfied without naming a counter.
    pub(crate) fn charge(self, lost: &dyn LossCounter) -> bool {
        if self.0 == 0 {
            return true;
        }
        lost.add(self.0);
        false
    }
}

/// The shared targetward **send-and-charge** step: gate on the origin's write lock
/// (§6), build the payload, hand it to the host endpoint, and report what did not
/// arrive (SIMP-2).
///
/// Deliberately *not* a whole pump. The three states an edge can be in — forward /
/// drain-and-count / park — are policy each node owns and must keep owning
/// (invariant 14): an interior node parks on an unattached edge so its writers
/// backpressure; the exec codec never parks, because its single stdout decode loop
/// would stall every other channel behind one detached device edge; a leg drains,
/// because a parked channel head-of-line blocks a whole cross-machine link (§9).
/// What all of them share is this tail — and the tail is where exits get forgotten:
/// the `reacquire_held`-failed arm is exactly the one CODEXEC-2 returned through
/// without counting.
///
/// `cost` is a parameter rather than `chunk.len()` because the two are not always
/// the same number: the map charges the bytes that *arrived* while sending the
/// *mapped* bytes, so a substituting or deleting rule cannot make its loss count
/// drift from its input. `payload` runs **after** the lock is held, which is also the
/// map's ordering requirement — its per-rule substitution counters are bumped inside
/// the transform, so a chunk that turns out to have nowhere to go must never have
/// been transformed at all — and it may return `None` for "nothing left to write",
/// which is not a loss (a fully deleted targetward chunk is the operator's explicit
/// intent, §7.8).
pub(crate) async fn forward_targetward(
    origin: &WritableOrigin,
    cost: u64,
    payload: impl FnOnce() -> Option<Chunk>,
) -> TargetwardLoss {
    let (tx, lock, id) = origin;
    // A `send --steal` transiently ousts a held origin; re-acquire FIFO once the
    // stealer releases, so the chunk is delayed, never dropped (§6). `false` means
    // the endpoint was torn down (or this origin cannot write): the bytes have
    // nowhere left to go.
    if !reacquire_held(lock, *id).await {
        return TargetwardLoss::of(cost);
    }
    let Some(chunk) = payload() else {
        return TargetwardLoss::none();
    };
    // The targetward channel closed between the grant and the send — its node was
    // removed, or the graph was replaced.
    if tx.send(chunk).await.is_err() {
        return TargetwardLoss::of(cost);
    }
    TargetwardLoss::none()
}

/// What a non-blocking targetward handoff did with its chunk
/// ([`try_forward_targetward`]).
///
/// `#[must_use]` for the same reason [`TargetwardLoss`] is, and it is the other half
/// of that enforcement (SIMP-2): [`Self::Full`] hands the *undelivered chunk* back,
/// so a call site written as the bare statement `try_forward_targetward(o, c, &l);`
/// drops those bytes on the floor with no counter — targetward, the one direction §5
/// forbids dropping on — and compiles clean. The blocking sibling could not be
/// written that way; without this attribute the non-blocking one could, which left
/// SIMP-2's stated failure scenario reachable on exactly the path the helper was
/// introduced to make safe. Every variant has to be looked at: `Full` must be parked
/// and retried, `Lost` is already charged, `Taken` is done.
#[must_use = "a targetward handoff must be acted on: `Full` carries the undelivered \
              chunk back and dropping it loses bytes silently (§5)"]
pub(crate) enum Handoff {
    /// The endpoint took it.
    Taken,
    /// The endpoint's bounded channel is full, so the chunk comes **back**: the
    /// caller holds it and stops reading its source. Targetward backpressure, never
    /// a drop (§5).
    Full(Chunk),
    /// The endpoint is gone. The bytes are lost and have already been charged.
    Lost,
}

/// Hand one chunk targetward without ever suspending the caller (CONC-1) — the
/// non-blocking sibling of [`forward_targetward`], with the same charge rule.
///
/// The pty reader is the one targetward producer that cannot `await` its send: the
/// same loop owns presence latching, termios reconciliation and last-close handling
/// (§7.2/§6), so parking it inside a full endpoint would freeze the console's whole
/// lifecycle for as long as the endpoint stayed full. It `try_send`s and holds the
/// refused chunk itself instead. The lock is *not* touched here — a held-over
/// payload was read while this origin could write, and a steal landing afterwards
/// must not strand it — so arbitration stays the caller's decision, exactly as
/// today.
pub(crate) fn try_forward_targetward(
    origin: &WritableOrigin,
    chunk: Chunk,
    lost: &dyn LossCounter,
) -> Handoff {
    let n = chunk.len() as u64;
    // The lock is bound and deliberately unused: see the note above.
    let (tx, _lock, _id) = origin;
    match tx.try_send(chunk) {
        Ok(()) => Handoff::Taken,
        Err(mpsc::error::TrySendError::Full(chunk)) => Handoff::Full(chunk),
        Err(mpsc::error::TrySendError::Closed(_)) => {
            lost.add(n);
            Handoff::Lost
        }
    }
}

/// The stream of hostward receivers a target-facing endpoint is given over its
/// life — one per edge it is connected with (§15.35).
///
/// A consuming task owns the receiving half forever and loops
/// `while let Some(rx) = inbox.recv().await { while let Some(chunk) = rx.recv().await { … } }`:
/// the inner loop ends when `disconnect` drops the producer's sink and the edge
/// channel closes (after draining whatever it still held), and the outer loop
/// parks until the next `connect`. One task, no aborts, no extra per-chunk hop.
pub type EdgeInbox = mpsc::Receiver<mpsc::Receiver<Chunk>>;

/// The sending half of an [`EdgeInbox`], kept by the daemon so `connect` can hand
/// a running node its new edge.
pub type EdgeInboxTx = mpsc::Sender<mpsc::Receiver<Chunk>>;

/// Depth of an [`EdgeInbox`]. An endpoint has at most one edge at a time (§4 rule
/// 2), so this only ever buffers a connect that lands while the consumer is
/// draining the previous edge's tail.
pub(crate) const EDGE_INBOX_CAP: usize = 4;

/// What one hostward [`fan_out`] did with its chunk, for the producer's own
/// per-node/per-channel bookkeeping.
pub(crate) struct FanOut {
    /// Whether at least one *live* sink took the chunk (or was charged a
    /// full-buffer drop for it). `false` means the chunk reached no live graph
    /// consumer at all — the empty-sinks and all-`Closed` cases — and its bytes
    /// have already been charged to the caller's unattached counter.
    ///
    /// **Not a delivery signal** — see [`Self::delivered`]. `live` answers "is
    /// anything still attached here?", which is the question the unattached charge
    /// turns on, and a consumer whose bounded buffer is full is very much attached.
    pub live: bool,
    /// Bytes at least one sink actually **took**: `chunk.len()` when any sink
    /// accepted it, `0` otherwise.
    ///
    /// A separate field from [`Self::live`] because the two answer different
    /// questions and sharing one of them has now been wrong in both directions
    /// (LEGD-2). Crediting delivery on `live` counted bytes a full sink never
    /// received; the correction `n - dropped_full` then under-counted, because
    /// `dropped_full` accumulates *per sink*, so a `[Ok, Full]` fan-out reported
    /// `dropped_full == n` and credited **zero** delivered for a chunk a live
    /// consumer had received in full. A mixed chunk is legitimately both delivered
    /// (to one consumer) and discarded (at another): the two counters measure
    /// different consumers, and only the chunk **no** sink accepted is unambiguously
    /// undelivered. So delivery is decided here, from the sends themselves, and no
    /// caller subtracts one counter from the other.
    pub delivered: u64,
    /// Bytes dropped at a sink whose bounded buffer was full, **summed over sinks**
    /// — so with several slow consumers it can exceed `chunk.len()`, and it is not a
    /// quantity any per-chunk arithmetic may subtract from. Already charged to each
    /// sink's own [`DropCounters`] (§5: loss is counted at the boundary that drops
    /// it); returned so a producer that *also* mirrors full-buffer loss in a
    /// per-channel counter (the leg's `discarded_hostward`) needs no second loop.
    pub dropped_full: u64,
}

/// Broadcast one hostward chunk to every attached sink, accounting for all loss
/// (§5) — the **one** shared hostward fan-out, previously hand-rolled once per
/// producing node (serial, codec, exec, leg, map) with only the serial copy
/// counting the all-sinks-closed case (F1/DM-3, design §16).
///
/// Delivery is lossy at the consuming boundary: a full bounded buffer costs that
/// consumer its bytes and nobody else (`try_send`, never `await`), which is what
/// keeps a slow spy from backpressuring the device. Three outcomes, each accounted:
///
/// * **Delivered** — the sink took it; nothing to count, and [`FanOut::delivered`]
///   reports the chunk length so the caller never has to infer delivery from
///   liveness or by subtracting the drops (LEGD-2).
/// * **Full** — the consumer has fallen behind; the drop is charged to *its*
///   [`DropCounters`] and the consumer is still live.
/// * **Closed** — the receiver is gone (whole-node teardown, or a consumer
///   cascade-removed while this producer survives, which leaves a permanently
///   `Closed` sink in the producer's snapshot). Counted only if *no* sink took the
///   chunk, so a live neighbour is not charged for a dead one.
///
/// Taking `unattached` by parameter — rather than returning "nobody took it" and
/// trusting each caller to remember — is the point: the all-sinks-closed case is
/// charged here, by construction, before the caller sees the result. The tap/ring
/// mirror is deliberately *not* part of this: it is a spy outside the graph with its
/// own accounting (§5, AGENTS.md invariant 9), so it must never suppress the
/// unattached count. Mirror before calling this, and pass the graph sinks only.
pub(crate) fn fan_out(
    chunk: &Chunk,
    sinks: &[AttachedSink],
    unattached: &dyn LossCounter,
) -> FanOut {
    let n = chunk.len() as u64;
    let mut out = FanOut {
        live: false,
        delivered: 0,
        dropped_full: 0,
    };
    for AttachedSink { tx, counters, .. } in sinks {
        match tx.try_send(chunk.clone()) {
            // Delivered to a live consumer. One sink taking the chunk is the whole
            // chunk delivered — the same bytes going to a second consumer are not a
            // second delivery — so this assigns rather than accumulates.
            Ok(()) => {
                out.live = true;
                out.delivered = n;
            }
            // Slow consumer: its bounded buffer is full — the drop is counted
            // against it, and it is still live.
            Err(mpsc::error::TrySendError::Full(_)) => {
                counters.add_full(n);
                out.dropped_full += n;
                out.live = true;
            }
            // Receiver gone; attributed below only if no sink took the chunk.
            Err(mpsc::error::TrySendError::Closed(_)) => {}
        }
    }
    if !out.live {
        // No live graph consumer took the chunk (empty or all-`Closed` sinks): count
        // it where it was lost so §5's "loss is always visible and attributable"
        // holds — independent of whether a tap or ring mirrored a copy.
        unattached.add(n);
    }
    out
}

/// The per-channel hostward accounting a multi-channel producer applies to one
/// decoded `data` event, as a trait so [`route_channel_data`] can own the
/// arithmetic once (SIMP-1).
///
/// Three producers decode channel events and hand them to local consumers — the
/// in-process codec, the exec codec and the leg — and each keeps its own
/// `ChannelStat` with its own field names, deliberately: a leg's
/// `discarded_hostward` is documented as covering *both* the full-buffer drop and
/// the no-consumer case, while a codec's narrower `discarded_unattached` covers only
/// the second and leaves full-buffer loss to the consuming boundary's own
/// [`DropCounters`]. The structs stay separate; only the *rule* — which counter gets
/// which bytes — is shared. That one real difference is [`Self::add_dropped_full`],
/// so it is now a visible property of one trait impl instead of a divergence between
/// three hand-written copies of the same block that nothing would flag.
pub(crate) trait HostwardChannelStat {
    /// Latch the channel active: bytes have crossed it.
    fn set_active(&self);
    /// Credit bytes a live consumer actually took.
    fn add_delivered(&self, n: u64);
    /// Where bytes that reached no live consumer at all are charged.
    fn unattached(&self) -> &dyn LossCounter;
    /// Mirror the bytes a slow consumer's full buffer cost this channel. The two
    /// codecs leave that loss to the consuming boundary's `DropCounters` and do
    /// nothing here; the leg also reports it per channel, so it overrides.
    fn add_dropped_full(&self, _n: u64) {}
}

/// Route one decoded channel `data` event hostward — the **one** per-channel
/// hostward routing block, previously a verbatim clone between the codec and the
/// exec codec and a third, differently-specified instance in the leg (SIMP-1,
/// design §16).
///
/// The order is invariant 9's and is load-bearing: latch active, mirror to the
/// tap/replay ring **first and outside the accounting** — the ring is a spy outside
/// the graph, so it must never suppress the unattached count — then fan out to the
/// graph sinks alone through [`fan_out`].
///
/// `stat` is `None` only for a channel identity the node was never configured for.
/// There is no per-channel counter to charge and mis-charging a neighbour would be
/// worse than not counting, so those bytes go to a local scratch counter; the caller
/// has already surfaced the identity itself, which is the diagnosis that matters
/// (`unconfigured` / `unbound` state, §8: an announcement never grows the graph).
pub(crate) fn route_channel_data(
    bytes: &Chunk,
    feed: Option<&TapFeed>,
    sinks: Option<&SharedFanOut>,
    stat: Option<&dyn HostwardChannelStat>,
) {
    let n = bytes.len() as u64;
    if let Some(s) = stat {
        s.set_active();
    }
    if let Some(feed) = feed {
        feed.mirror(bytes);
    }
    let scratch = Cell::new(0u64);
    let unattached: &dyn LossCounter = match stat {
        Some(s) => s.unattached(),
        None => &scratch,
    };
    let Some(sinks) = sinks else {
        // The channel has no fan-out at all: nothing is bound, so nothing took it.
        unattached.add(n);
        return;
    };
    let fan = sinks.broadcast(bytes, unattached);
    let Some(s) = stat else {
        return;
    };
    // Credit exactly what a sink took (`0` when none did), never what liveness
    // suggests and never `n - dropped_full` (LEGD-2, both directions — see
    // [`FanOut::delivered`]). With one consumer the two counters still partition the
    // stream exactly, which is the reconciliation an operator does; with several,
    // a chunk one consumer took while another's buffer was full is deliberately
    // counted in both, because they measure different consumers.
    s.add_delivered(fan.delivered);
    // A no-op for the codecs; the leg's per-channel mirror of the full-buffer loss
    // the sinks' own `DropCounters` already carry (§5).
    s.add_dropped_full(fan.dropped_full);
}

/// Hostward drop counters for one consuming boundary (§5). All hostward loss is
/// counted at the boundary that drops it, so it is always located, counted, and
/// attributable — a slow spy costs itself data, never its neighbors. One instance
/// is shared (via `Arc`) between the producing serial reader — which counts
/// full-buffer drops and, since the high-throughput reader runs on a dedicated
/// blocking thread (§15.19), needs the counters to be `Send`/`Sync`, hence
/// atomics — and the consuming boundary, which counts presence-gated discards and
/// reports both in state. `Relaxed` suffices: counters are monotonic and read
/// only for reporting, never to synchronize other memory.
#[derive(Default)]
pub struct DropCounters {
    /// Bytes dropped because the boundary's bounded buffer was full — a slow
    /// consumer that has fallen behind line rate (§5).
    dropped_full: AtomicU64,
    /// Bytes discarded because no consumer was present to receive them — a PTY
    /// with no client holding the slave open (§7.2 presence gating).
    discarded_absent: AtomicU64,
}

impl DropCounters {
    pub fn add_full(&self, n: u64) {
        self.dropped_full.fetch_add(n, Ordering::Relaxed);
    }

    pub fn add_absent(&self, n: u64) {
        self.discarded_absent.fetch_add(n, Ordering::Relaxed);
    }

    pub fn dropped_full(&self) -> u64 {
        self.dropped_full.load(Ordering::Relaxed)
    }

    pub fn discarded_absent(&self) -> u64 {
        self.discarded_absent.load(Ordering::Relaxed)
    }
}

/// Read-buffer size for one `read(2)` on a boundary fd. A PTY packet-mode read
/// spends one byte on the control marker, leaving the rest for data. Sized so a
/// draining boundary reads many kilobytes per wakeup, keeping throughput well
/// clear of the readiness cadence (§15.18): fewer, larger reads per idle gap.
pub const READ_BUF: usize = 64 * 1024;

/// Bounded channel depth, in chunks. This is the boundary's buffer: hostward it
/// caps how much a slow consumer buffers before drops begin; targetward it caps
/// how far a producer runs ahead before backpressure suspends the origin. Sized
/// to absorb the dedicated reader thread's bursts across a runtime-scheduling gap
/// before a keep-up consumer (e.g. the log pump) drains them.
pub const CHANNEL_CAP: usize = 256;

/// How long a boundary task sleeps between readiness polls when there is nothing
/// to do. During an active transfer the task re-polls immediately after each
/// drain, so this bounds idle latency (and idle CPU), never throughput. Well
/// under the §7.2 sub-second presence requirement.
pub const IDLE_POLL: Duration = Duration::from_millis(5);

/// The hostward channel depth for a consumer fed by the node named `host_node`.
///
/// A serial node's `hostward_buffer` is its *consumers'* drop policy (§5, §7.1) —
/// how much each of them may buffer before drops begin — so the depth belongs to
/// the producing node, not the consuming one. Shared by `Wiring::build` and by
/// `connect`, which creates the same channel on a running graph: an edge added
/// mid-stream must buffer exactly as one loaded from configuration would.
pub(crate) fn hostward_depth(config: &GraphConfig, host_node: &str) -> usize {
    config
        .nodes
        .iter()
        .find_map(|n| match n {
            NodeConfig::Serial {
                name,
                hostward_buffer,
                ..
            } if name == host_node => Some(*hostward_buffer),
            _ => None,
        })
        .unwrap_or(CHANNEL_CAP)
}

/// The live handles one edge attachment binds together, plus the two identities it
/// is registered under (§15.35). Consumed by [`attach_edge`].
///
/// Both attach paths already hold exactly these — `Wiring::build` reads them out of
/// the plan it just made, `Daemon::connect` out of the display-keyed state maps
/// `absorb_wiring` re-keyed them into — so bundling them is what lets one function
/// be the single implementation of "attach one edge".
pub(crate) struct EdgeParts {
    /// The consuming (target-facing) endpoint's address. It is the origin's *label*
    /// on the lock, the fan-out tag `disconnect` detaches by (§4 rule 2 makes it
    /// unique), and the key both callers file the registration under.
    pub target: EndpointAddr,
    /// The id this attachment registers under on the host endpoint's lock. Allocated
    /// by the caller on purpose: the two paths count from different places and must
    /// not collide — `Wiring::build` counts up from 0 and publishes its ceiling as
    /// [`Wiring::next_origin`], and `connect` allocates strictly above that.
    pub origin: OriginId,
    /// The host endpoint's write lock (§6). `None` only for a host-facing endpoint
    /// that has none, which cannot occur post-validation; the edge then attaches
    /// unarbitrated rather than not at all.
    pub lock: Option<SharedLock>,
    /// A targetward sender into the host endpoint — the same arbitrated channel the
    /// `send` verb injects into. `None` under the same unreachable condition as
    /// `lock`, with which it is populated and pruned in lockstep.
    pub host_targetward: Option<mpsc::Sender<Chunk>>,
    /// The host endpoint's live fan-out list, which this edge's hostward sink joins.
    pub fanout: SharedFanOut,
    /// The consuming endpoint's edge inbox: how its *already-running* pump is handed
    /// this edge's hostward receiver, so nothing is spawned or aborted (invariant 14).
    pub inbox: EdgeInboxTx,
    /// The consuming endpoint's live edge binding, filled here.
    pub slot: SharedTargetEdge,
    /// The consuming endpoint's drop counters, shared with the hostward sink so the
    /// producer's full-buffer drops and the consumer's own discards land in one place.
    pub counters: Arc<DropCounters>,
}

/// What one [`attach_edge`] did, for its caller to finish (§15.35).
pub(crate) struct AttachOutcome {
    /// The lock and id this edge registered under, when the host endpoint has a lock.
    /// Present for **every** effective mode, `never` included — the lock registers
    /// every origin so `state` can list it, so every one of them must be
    /// unregisterable (see [`TargetEdge::registered`]).
    pub registered: Option<(SharedLock, OriginId)>,
    /// Whether this edge is addressable by `lock`/`unlock`, i.e. whether the caller
    /// should file [`Self::registered`] in its `origin_locks` map. Returned rather
    /// than re-derived because the two callers spelled the condition differently
    /// (`mode != never` in the plan, "a writer was installed" live) and only coincided
    /// because `endpoint_locks` and the targetward senders are populated in lockstep
    /// — a latent divergence of exactly the kind one-rule-one-place exists to remove.
    pub addressable: bool,
    /// Whether [`EndpointLock::register`] granted the lock to this origin outright — a
    /// `held` origin on a free exclusive endpoint. That is a **fresh grant**, which §6
    /// says runs purge-on-acquire before the origin's bytes flow: vacuous at load
    /// (nothing has written yet), and the reason `connect` needs to know.
    pub granted: bool,
    /// Whether the consuming endpoint actually took the hostward receiver. `false`
    /// means it has no pump at all — a pty whose setup faulted, or a `faces = "host"`
    /// codec/exec channel whose driver is deferred (§14) — a dead edge that `load`
    /// would have produced just the same, so it is reported, never refused (§15.8).
    pub consumer_live: bool,
}

/// Attach one edge: the **single** implementation of it (§16 one-rule-one-place),
/// shared by `Wiring::build`'s edge loop at load and by `Daemon::connect` on a
/// running graph.
///
/// Its mirror was consolidated first — `GraphState::detach_edge_runtime` is shared by
/// `disconnect` and `remove-node --cascade` because "two implementations of leaving
/// the lock cleanly" is how the phantom holder came back once (§15.27) — and this is
/// the attach half of the same argument (SIMPB-1). Assembling an edge is six ordered
/// steps over five long-lived structures, and a change to any of them (an
/// [`EdgeSlot`] field, a second counter, a per-edge epoch) must not be able to reach
/// the loaded path and miss the live one.
///
/// `mode` is the **effective** write mode, which every caller must obtain from
/// `GraphConfig::effective_write_mode` — the one implementation of the log⇒`never`
/// and map-`raw`⇒`held` promotions, shared with the validator so the two halves
/// cannot disagree about what a graph does (invariant 12). It is a parameter rather
/// than derived here because the two callers hold different configurations at this
/// point (the loaded graph; the validated *candidate*).
///
/// Nothing is spawned, aborted or restarted: the fan-out, the inbox and the edge slot
/// all outlive individual edges, which is what makes a mid-stream `connect` join the
/// stream instead of toggling DTR on the board behind a serial node (invariant 14).
pub(crate) fn attach_edge(parts: EdgeParts, mode: WriteMode, depth: usize) -> AttachOutcome {
    let EdgeParts {
        target,
        origin,
        lock,
        host_targetward,
        fanout,
        inbox,
        slot,
        counters,
    } = parts;

    // 1. Register the attachment as an origin on the host endpoint's lock (§6),
    //    labelled by the target's address so `lock`/`unlock` can name it. First,
    //    before any byte can flow, so arbitration is decided from the first chunk.
    let registered = lock.map(|cell| {
        cell.with_mut(|l| l.register(origin, target.to_string(), mode));
        (cell, origin)
    });
    let granted = registered
        .as_ref()
        .is_some_and(|(cell, id)| cell.with(|l| l.holder() == Some(*id)));

    // 2. Fill the consuming endpoint's live edge slot. `attached` is set whatever the
    //    mode — a `never` edge is a real attachment that simply cannot write, and it
    //    is that flag, not the writer, that decides `active` vs `waiting` (§7.5,
    //    §15.8) — while the targetward path exists only for a writable mode.
    let writer = (mode != WriteMode::Never)
        .then_some(host_targetward)
        .flatten();
    let addressable = writer.is_some();
    slot.with_mut(|e| {
        e.attached = true;
        e.registered = registered.clone();
        e.writer = writer;
    });

    // 3. Hostward: one dedicated channel per (host, target) edge at the producing
    //    node's configured depth (§5/§7.1), so a slow consumer's drops are isolated
    //    to its own channel.
    let (htx, hrx) = mpsc::channel(depth);
    let sink = AttachedSink {
        target,
        tx: htx,
        counters,
    };
    fanout.attach(sink);

    // 4. Hand the consuming endpoint its receiver through the inbox its pump is
    //    already parked on. A *full* inbox would mean that endpoint already has an
    //    unconsumed edge, which §4 rule 2 has been validated against on both paths; a
    //    *closed* one is the no-pump case [`AttachOutcome::consumer_live`] reports.
    let consumer_live = inbox.try_send(hrx).is_ok();

    AttachOutcome {
        registered,
        addressable,
        granted,
        consumer_live,
    }
}

/// The channels the data plane hands to each node's `start`, keyed by **endpoint
/// address** (`node` or `node/channel`, §3). Built once from the loaded
/// configuration; each node removes its own endpoints' entries at start.
///
/// The topology is no longer two-layer (serial→consumer): an interior codec node
/// (§7.5) is a *target*-facing consumer on its multiplexed side and N *host*-facing
/// producers on its channels, so a single node may claim entries from both halves.
/// Keying by endpoint rather than by node makes that uniform — every host-facing
/// endpoint (a serial, a codec channel) fans out and arbitrates; every
/// target-facing endpoint (a PTY, a log, a codec's multiplexed side) is a single
/// producer that may also write back.
#[derive(Default)]
pub struct Wiring {
    // --- host-facing endpoints (serial sole endpoint, codec channels) ---
    /// Host-facing endpoint → its write lock (§6). The daemon keeps a clone for
    /// `lock`/`unlock`/`send` and for reporting lock state.
    pub endpoint_locks: HashMap<EndpointAddr, SharedLock>,
    /// Host-facing endpoint → its live fan-out list (§4 rule 2, §15.35). One per
    /// host-facing endpoint whether or not it has consumers today: the producer
    /// keeps the handle for its whole life and `connect`/`disconnect` mutate it in
    /// place.
    pub host_fanout: HashMap<EndpointAddr, SharedFanOut>,
    /// Host-facing endpoint → the single targetward receiver it drains (all its
    /// writing origins feed this one channel, arbitrated by the lock).
    pub host_targetward_rx: HashMap<EndpointAddr, mpsc::Receiver<Chunk>>,
    /// Host-facing endpoint → a targetward sender into it, so the `send` verb can
    /// inject a line as a transient origin even with no writer attached (§6).
    pub host_targetward_tx: HashMap<EndpointAddr, mpsc::Sender<Chunk>>,
    // --- target-facing endpoints (PTY, log, codec multiplexed side) ---
    /// Target-facing endpoint → the stream of hostward receivers it will be given,
    /// one per edge over its life (§15.35). The consuming task takes this at
    /// `start` and keeps it forever; the daemon keeps the sending half.
    pub target_inbox: HashMap<EndpointAddr, EdgeInbox>,
    /// Target-facing endpoint → the sending half of its [`EdgeInbox`], which
    /// `connect` uses to hand a running node its new edge.
    pub target_inbox_tx: HashMap<EndpointAddr, EdgeInboxTx>,
    /// Target-facing endpoint → its [`DropCounters`] (shared with whatever host
    /// sink currently feeds it), for drop/discard counts and state reporting (§5,
    /// §7.2, §7.3). One per *endpoint*, not per edge: the counters describe the
    /// consuming boundary, so a reconnect does not reset an operator's loss history.
    pub target_counters: HashMap<EndpointAddr, Arc<DropCounters>>,
    /// Target-facing endpoint → its live edge binding (§15.35): the targetward
    /// sender and lock handle while an edge with a writable mode is attached.
    pub target_edges: HashMap<EndpointAddr, SharedTargetEdge>,
    /// Writing target-facing endpoint → (its host endpoint's lock, its origin id),
    /// so the control plane can resolve a `lock`/`unlock` by origin name (§6). Only
    /// origins that can write (mode ≠ never) appear — a `never` origin is still
    /// *registered* on the lock, but it is not addressable by `lock`/`unlock`, and
    /// its id lives in the target endpoint's [`TargetEdge::registered`].
    pub origin_locks: HashMap<EndpointAddr, (SharedLock, OriginId)>,
    /// One past the highest origin id this plan assigned (§15.35). The daemon keeps
    /// its live-`connect` allocator above this, so an edge added later can never
    /// collide with one the plan registered. Derived from the counter rather than
    /// from `origin_locks`, which omits `never` origins — a graph whose only edge is
    /// `never` would otherwise leave the floor at 0 and hand `connect` an id already
    /// registered on that very lock.
    pub next_origin: u64,
    // --- taps and the replay ring (§5 ring, §17 taps) ---
    /// Host-facing endpoint → the producer-side tap feed it mirrors hostward bytes
    /// into (only while a tap or ring wants them). Each producer claims its own
    /// endpoints' feeds at `start`.
    pub tap_feeds: HashMap<EndpointAddr, TapFeed>,
    /// Host-facing endpoint → its tap hub plus the feed receiver a hub task drains
    /// (§17). The daemon consumes this to spawn the hub tasks and keep the hub
    /// handles for `tap.open`/`tap.close`/`state`.
    pub tap_hub_setup: HashMap<EndpointAddr, TapHubSetup>,
}

/// The daemon's per-endpoint tap-hub startup bundle: the shared hub handle it keeps
/// (for registering taps and reporting state) and the feed receiver a spawned hub
/// task drains into [`TapHub::ingest`] (§17).
pub struct TapHubSetup {
    pub hub: SharedTapHub,
    pub feed_rx: mpsc::Receiver<Chunk>,
}

impl Wiring {
    /// Build the channel plan from the validated graph (load validates first,
    /// §11), keyed by endpoint. Every host-facing endpoint gets a lock, a fan-out
    /// sink list, and one arbitrated targetward channel; every edge wires one
    /// host↔target pair with the mode
    /// [`GraphConfig::effective_write_mode`](nexus_core::config::GraphConfig::effective_write_mode)
    /// reports — the single source of truth for the log⇒`never` and
    /// map-`raw`⇒`held` promotions, shared with the validator so the two halves
    /// cannot disagree about what a graph actually does (§16).
    pub fn build(config: &GraphConfig, notifier: &broadcast::Sender<Notification>) -> Wiring {
        // Every endpoint's facing + arbitration, keyed by its address (§4). Derived
        // from each node's shape, so codec channels and multiplexed sides appear
        // alongside single-endpoint boundary nodes.
        let mut facing: HashMap<EndpointAddr, (Facing, Arbitration)> = HashMap::new();
        // A host-facing endpoint's configured replay-ring depth in bytes (§5, §15.32).
        // Every host-facing endpoint carries the attribute now — a serial node's
        // single endpoint and every host-facing channel of a codec or leg — each
        // defaulting to 64 KiB (config layer) and opt-out with `0`. Every host
        // endpoint gets a hub regardless (a tap can attach to any of them); this map
        // sizes each one's ring.
        let mut host_ring_cap: HashMap<EndpointAddr, usize> = HashMap::new();
        for n in &config.nodes {
            let node_ring = n.replay_ring();
            for ep in &n.shape().endpoints {
                let addr = EndpointAddr::new(n.name(), ep.name.clone());
                facing.insert(addr.clone(), (ep.facing, ep.arbitration));
                // Ring only a host-facing endpoint; the node's value is inert on any
                // target-facing endpoint (a serial output leg, a sending leg's
                // channels, a demux's multiplexed side).
                if ep.facing == Facing::Host
                    && let Some(cap) = node_ring
                {
                    host_ring_cap.insert(addr, cap);
                }
            }
        }

        let mut wiring = Wiring::default();
        // One write lock + one arbitrated targetward channel per host-facing
        // endpoint (§6). The daemon keeps a sender clone so `send` works even with
        // no writer attached; each writer gets its own clone below.
        for (addr, (f, arb)) in &facing {
            if *f == Facing::Host {
                wiring.endpoint_locks.insert(
                    addr.clone(),
                    Rc::new(LockCell::new(
                        addr.to_string(),
                        EndpointLock::new(*arb),
                        notifier.clone(),
                    )),
                );
                let (tx, rx) = mpsc::channel(CHANNEL_CAP);
                wiring.host_targetward_rx.insert(addr.clone(), rx);
                wiring.host_targetward_tx.insert(addr.clone(), tx);
                wiring.host_fanout.insert(addr.clone(), FanOutList::new());

                // One tap hub per host-facing endpoint (§17): a tap can attach to
                // any of them. The hub owns the endpoint's replay ring (§5) — sized
                // from config, 0 = off — and its `active` flag gates the producer's
                // mirror so an untapped, ring-less endpoint pays only an atomic load.
                let ring_cap = host_ring_cap.get(addr).copied().unwrap_or(0);
                let (hub, active, feed_dropped) = TapHub::new(addr.to_string(), ring_cap);
                let (feed_tx, feed_rx) = mpsc::channel(TAP_FEED_CAP);
                wiring.tap_feeds.insert(
                    addr.clone(),
                    TapFeed {
                        tx: feed_tx,
                        active,
                        feed_dropped,
                    },
                );
                wiring
                    .tap_hub_setup
                    .insert(addr.clone(), TapHubSetup { hub, feed_rx });
            } else {
                // Every *target*-facing endpoint gets its edge machinery whether or
                // not configuration wires it today (§15.35). Building these per
                // endpoint rather than per edge is what makes `connect` a plain
                // mutation of a running graph: the consuming task, its counters and
                // its origin slot already exist, so an edge only has to arrive.
                let (inbox_tx, inbox_rx) = mpsc::channel(EDGE_INBOX_CAP);
                wiring.target_inbox.insert(addr.clone(), inbox_rx);
                wiring.target_inbox_tx.insert(addr.clone(), inbox_tx);
                wiring
                    .target_counters
                    .insert(addr.clone(), Arc::new(DropCounters::default()));
                wiring.target_edges.insert(addr.clone(), TargetEdge::new());
            }
        }

        let mut next_origin = 0u64;
        for edge in &config.edges {
            let fa = facing.get(&edge.a).map(|(f, _)| *f);
            let fb = facing.get(&edge.b).map(|(f, _)| *f);
            // Identify the host and target ends. Same-facing or dangling edges
            // can't occur post-validation; skip defensively.
            let (host, target) = match (fa, fb) {
                (Some(Facing::Host), Some(Facing::Target)) => (&edge.a, &edge.b),
                (Some(Facing::Target), Some(Facing::Host)) => (&edge.b, &edge.a),
                _ => continue,
            };

            // The three per-endpoint structures this edge joins. The facing loop
            // above made one of each for *every* endpoint, so post-validation all
            // three are present; the skip is the same defensive arm as the facing
            // match, and skipping wholesale beats attaching an edge by halves.
            let (Some(fanout), Some(inbox), Some(slot)) = (
                wiring.host_fanout.get(host).cloned(),
                wiring.target_inbox_tx.get(target).cloned(),
                wiring.target_edges.get(target).cloned(),
            ) else {
                continue;
            };

            // The two configuration-to-runtime promotions — a log target forces
            // `never` (§7.3), a map's `raw` endpoint promotes `on-demand` to `held`
            // (§7.8) — live in `GraphConfig::effective_write_mode`, not here: the
            // validator reasons about the same effective modes the wiring registers,
            // so the two can never drift (§16 "one rule, one place", invariant 12).
            let mode = config.effective_write_mode(edge);
            let origin_id = OriginId(next_origin);
            next_origin += 1;
            // The one implementation of "attach one edge" (SIMPB-1), shared with
            // `Daemon::connect` so a live edge is assembled exactly as a loaded one.
            // Depth is the producing serial's configured hostward buffer (§7.1).
            let attached = attach_edge(
                EdgeParts {
                    target: target.clone(),
                    origin: origin_id,
                    lock: wiring.endpoint_locks.get(host).cloned(),
                    host_targetward: wiring.host_targetward_tx.get(host).cloned(),
                    fanout,
                    inbox,
                    slot,
                    counters: wiring
                        .target_counters
                        .get(target)
                        .cloned()
                        .unwrap_or_default(),
                },
                mode,
                hostward_depth(config, &host.node),
            );
            // Only a writable origin is addressable by `lock`/`unlock`; the read-only
            // ones stay registered on the lock and reachable through the target
            // endpoint's `TargetEdge::registered`, which is what `disconnect` undoes.
            if attached.addressable
                && let Some(registration) = attached.registered
            {
                wiring.origin_locks.insert(target.clone(), registration);
            }
        }

        wiring.next_origin = next_origin;
        wiring
    }
}

/// The readiness-poll interval during an *active* transfer: short, so a momentary
/// empty/full buffer mid-stream is rechecked in ~1ms (the tokio timer floor)
/// rather than the 5ms [`IDLE_POLL`] — the difference between ~1 MB/s and tens of
/// MB/s. A boundary resets its wait to this on every byte of progress, then lets
/// it back off toward [`IDLE_POLL`] (§15.19: a `yield_now` spin does nothing
/// here because the peer is a separate process that only advances as real
/// wall-clock passes — the finding that retired §15.18's "never throughput"
/// claim once the hot path moved to a blocking thread).
pub const ACTIVE_POLL: Duration = Duration::from_micros(200);

/// Grow a readiness wait toward [`IDLE_POLL`]: doubles `*wait`, capped. Callers
/// reset `*wait = ACTIVE_POLL` on progress, so an active fd stays near
/// [`ACTIVE_POLL`] and only a genuinely idle one settles onto [`IDLE_POLL`].
pub fn back_off(wait: &mut Duration) {
    *wait = (*wait * 2).min(IDLE_POLL);
}

/// Write every byte of `data` to a boundary fd. The boundary drains at its own
/// pace: upstream buffering (and any drops) happen in the feeding channel, never
/// here. `Err` means the peer hung up. On `WouldBlock` the writability wait polls
/// with the [`ACTIVE_POLL`]→[`IDLE_POLL`] backoff, so a fast consumer is drained
/// at full rate (§15.19's adaptive active-to-idle backoff).
pub async fn write_all(fd: RawFd, mut data: &[u8]) -> std::io::Result<()> {
    let mut wait = ACTIVE_POLL;
    while !data.is_empty() {
        match sys::write_fd(fd, data) {
            Ok(0) => return Err(std::io::ErrorKind::WriteZero.into()),
            Ok(n) => {
                data = &data[n..];
                wait = ACTIVE_POLL; // made progress: recheck promptly
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                let re = sys::poll_ready(fd, PollFlags::POLLOUT | PollFlags::POLLHUP);
                if re.contains(PollFlags::POLLOUT) {
                    continue;
                }
                if re.contains(PollFlags::POLLHUP) {
                    return Err(std::io::ErrorKind::BrokenPipe.into());
                }
                tokio::time::sleep(wait).await;
                back_off(&mut wait);
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- fragmentation boundary (invariant 3's single shared helper) ------------

    #[test]
    fn frame_payload_cap_reserves_the_envelope_header() {
        // 1 type byte + 2 channel-length bytes + the channel id (`codec_api::encode`).
        assert_eq!(frame_payload_cap(""), MAX_FRAME_SIZE - 3);
        assert_eq!(frame_payload_cap("console"), MAX_FRAME_SIZE - 3 - 7);
    }

    #[test]
    fn frame_payload_cap_is_floored_at_one_for_a_pathological_channel_id() {
        // A channel id long enough to consume the whole frame would give a cap of 0
        // and an iterator that never advances. The floor keeps `frame_ranges`
        // productive; whether the resulting frame *encodes* is `data_frames`'
        // problem, and it reports the residual rather than truncating silently.
        let huge = "c".repeat(MAX_FRAME_SIZE * 2);
        assert_eq!(frame_payload_cap(&huge), 1);
        let exact = "c".repeat(MAX_FRAME_SIZE - 3);
        assert_eq!(
            frame_payload_cap(&exact),
            1,
            "0 would be the unfloored value"
        );
    }

    #[test]
    fn frame_ranges_covers_every_byte_exactly_once() {
        let cap = frame_payload_cap("ch");
        for total in [0usize, 1, cap - 1, cap, cap + 1, 2 * cap, 2 * cap + 7] {
            let ranges: Vec<(usize, usize)> = frame_ranges("ch", total).collect();
            // Contiguous, non-overlapping, in order, and never wider than the cap.
            let mut expect_off = 0usize;
            for (off, end) in &ranges {
                assert_eq!(
                    *off, expect_off,
                    "ranges must be contiguous (total {total})"
                );
                assert!(end > off, "no empty range (total {total})");
                assert!(end - off <= cap, "no range exceeds the cap (total {total})");
                expect_off = *end;
            }
            assert_eq!(
                expect_off, total,
                "ranges must cover the chunk (total {total})"
            );
            assert_eq!(
                ranges.len(),
                total.div_ceil(cap),
                "piece count (total {total})"
            );
        }
    }

    #[test]
    fn frame_ranges_of_an_empty_chunk_yields_nothing() {
        assert_eq!(frame_ranges("ch", 0).count(), 0);
    }

    // --- data_frames: fragment, never skip-on-error, count any residual ---------

    /// Reassemble a `data_frames` stream, returning `(payload, residual)`.
    fn drive(channel: &str, bytes: &Chunk) -> (Vec<u8>, usize) {
        let mut payload = Vec::new();
        let mut residual = 0usize;
        for item in data_frames(channel, bytes) {
            match item {
                DataFrame::Piece(len, frame) => {
                    // The frame is `len` source bytes plus a 4-byte length prefix and
                    // the 3 + channel.len() envelope header.
                    assert_eq!(frame.len(), 4 + 3 + channel.len() + len);
                    let start = frame.len() - len;
                    payload.extend_from_slice(&frame[start..]);
                }
                DataFrame::Residual(n) => residual += n,
            }
        }
        (payload, residual)
    }

    #[test]
    fn data_frames_fragments_byte_exactly_with_no_residual() {
        let cap = frame_payload_cap("console");
        for total in [0usize, 1, cap, cap + 1, 3 * cap + 11] {
            let src: Vec<u8> = (0..total).map(|i| (i % 251) as u8).collect();
            let chunk = Chunk::from(src.clone());
            let (payload, residual) = drive("console", &chunk);
            assert_eq!(
                payload, src,
                "reassembly must be byte-exact (total {total})"
            );
            assert_eq!(
                residual, 0,
                "a sane channel id never residuals (total {total})"
            );
        }
    }

    #[test]
    fn data_frames_reports_the_residual_instead_of_truncating_in_silence() {
        // RV-9: with a channel identity long enough that even a 1-byte payload
        // overflows MAX_FRAME_SIZE, `encode` refuses every piece. The old `map_while`
        // shape ended the iteration and lost the chunk without a trace; the residual
        // is the whole chunk, and the caller is forced to see it.
        let channel = "c".repeat(MAX_FRAME_SIZE);
        let chunk = Chunk::copy_from_slice(b"twelve bytes");
        let (payload, residual) = drive(&channel, &chunk);
        assert!(payload.is_empty(), "nothing could be framed");
        assert_eq!(residual, chunk.len(), "the whole chunk is reported as loss");
    }

    #[test]
    fn data_frames_residual_is_terminal() {
        let channel = "c".repeat(MAX_FRAME_SIZE);
        let chunk = Chunk::copy_from_slice(b"twelve bytes");
        let items: Vec<DataFrame> = data_frames(&channel, &chunk).collect();
        assert_eq!(items.len(), 1, "the residual ends the stream");
        assert!(matches!(items[0], DataFrame::Residual(12)));
    }

    // --- the shared hostward fan-out (F1) --------------------------------------

    fn sink(cap: usize) -> (AttachedSink, mpsc::Receiver<Chunk>, Arc<DropCounters>) {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel::<Chunk>(cap);
        let counters = Arc::new(DropCounters::default());
        (
            AttachedSink {
                target: format!("consumer{n}").parse().expect("address parses"),
                tx,
                counters: counters.clone(),
            },
            rx,
            counters,
        )
    }

    #[test]
    fn fan_out_with_no_sinks_counts_the_whole_chunk_as_unattached() {
        let unattached = Cell::new(0u64);
        let chunk = Chunk::copy_from_slice(b"hello");
        let out = fan_out(&chunk, &[], &unattached);
        assert!(!out.live);
        assert_eq!(out.dropped_full, 0);
        assert_eq!(out.delivered, 0);
        assert_eq!(unattached.get(), 5);
    }

    #[test]
    fn fan_out_with_all_sinks_closed_counts_unattached_not_full() {
        // The consumer-cascade-removed case: a permanently `Closed` sink is not a
        // slow consumer, so it must not be charged a full-buffer drop — and the
        // chunk reached nobody, so it is unattached loss (§5).
        let (s1, rx1, c1) = sink(4);
        let (s2, rx2, c2) = sink(4);
        drop(rx1);
        drop(rx2);
        let unattached = Cell::new(0u64);
        let chunk = Chunk::copy_from_slice(b"hello");
        let out = fan_out(&chunk, &[s1, s2], &unattached);
        assert!(!out.live);
        assert_eq!(unattached.get(), 5);
        assert_eq!(c1.dropped_full(), 0);
        assert_eq!(c2.dropped_full(), 0);
    }

    #[test]
    fn fan_out_charges_a_full_sink_and_still_delivers_to_a_live_one() {
        // One consumer has fallen behind (its bounded buffer is full) and one keeps
        // up. The slow one is charged, the fast one is served, and nothing is
        // unattached — a slow spy costs only itself (§5).
        let (full_sink, _full_rx, full_counters) = sink(1);
        full_sink
            .tx
            .try_send(Chunk::copy_from_slice(b"x"))
            .expect("fill the slow consumer's buffer");
        let (live_sink, mut live_rx, live_counters) = sink(4);

        let unattached = Cell::new(0u64);
        let chunk = Chunk::copy_from_slice(b"hello");
        let out = fan_out(&chunk, &[full_sink, live_sink], &unattached);

        assert!(out.live);
        assert_eq!(out.dropped_full, 5, "the full sink's drop is reported back");
        assert_eq!(
            out.delivered, 5,
            "a live consumer received the whole chunk, so it *was* delivered (LEGD-2)"
        );
        assert_eq!(full_counters.dropped_full(), 5, "charged where it dropped");
        assert_eq!(live_counters.dropped_full(), 0);
        assert_eq!(unattached.get(), 0, "a live consumer took it");
        assert_eq!(&live_rx.try_recv().expect("delivered")[..], b"hello");
    }

    #[test]
    fn fan_out_reports_no_delivery_when_every_live_sink_was_full() {
        // The other side of the same distinction: `live` is true (a full consumer is
        // attached) while `delivered` is 0, so a caller reading `live` as delivery
        // credits bytes nobody received.
        let (a, _a_rx, _ac) = sink(1);
        let (b, _b_rx, _bc) = sink(1);
        for s in [&a, &b] {
            s.tx.try_send(Chunk::copy_from_slice(b"x"))
                .expect("fill the slow consumer's buffer");
        }
        let unattached = Cell::new(0u64);
        let out = fan_out(&Chunk::copy_from_slice(b"hello"), &[a, b], &unattached);
        assert!(out.live);
        assert_eq!(out.delivered, 0, "no sink took it");
        assert_eq!(
            out.dropped_full, 10,
            "`dropped_full` sums over sinks and can exceed the chunk — which is why \
             it is not something delivery may be derived from"
        );
        assert_eq!(unattached.get(), 0, "full is not unattached");
    }

    #[test]
    fn fan_out_counts_a_closed_sink_only_when_no_live_sink_took_the_chunk() {
        // A dead neighbour must not be charged against a healthy one: with one live
        // sink present, the closed sink contributes nothing to the unattached count.
        let (dead_sink, dead_rx, _dead_counters) = sink(4);
        drop(dead_rx);
        let (live_sink, mut live_rx, _live_counters) = sink(4);
        let unattached = Cell::new(0u64);
        let chunk = Chunk::copy_from_slice(b"hello");
        let out = fan_out(&chunk, &[dead_sink, live_sink], &unattached);
        assert!(out.live);
        assert_eq!(unattached.get(), 0);
        assert!(live_rx.try_recv().is_ok());
    }

    #[test]
    fn fan_out_charges_an_atomic_counter_from_the_blocking_reader_shape() {
        // The serial reader runs on a dedicated blocking thread (§15.19), so its
        // counter is an `Arc<AtomicU64>` rather than a `Cell`. Both must work
        // through the one helper (F1).
        let unattached = Arc::new(AtomicU64::new(0));
        let chunk = Chunk::copy_from_slice(b"hello");
        assert!(!fan_out(&chunk, &[], &*unattached).live);
        assert_eq!(unattached.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn fan_out_of_an_empty_chunk_still_reports_liveness_without_counting_bytes() {
        let unattached = Cell::new(0u64);
        let out = fan_out(&Chunk::new(), &[], &unattached);
        assert!(!out.live);
        assert_eq!(
            unattached.get(),
            0,
            "a zero-length chunk is zero bytes of loss"
        );
    }

    // --- the shared per-channel hostward routing block (SIMP-1) -----------------

    /// A `ChannelStat` stand-in with the codecs' rule: full-buffer loss belongs to
    /// the consuming boundary, so `add_dropped_full` stays at its default.
    #[derive(Default)]
    struct CodecStat {
        active: Cell<bool>,
        delivered: Cell<u64>,
        unattached: Cell<u64>,
    }

    impl HostwardChannelStat for CodecStat {
        fn set_active(&self) {
            self.active.set(true);
        }
        fn add_delivered(&self, n: u64) {
            self.delivered.set(self.delivered.get() + n);
        }
        fn unattached(&self) -> &dyn LossCounter {
            &self.unattached
        }
    }

    /// The leg's rule: one counter covers both the full-buffer drop and the
    /// no-consumer case, so this impl folds `dropped_full` in. The *only* legitimate
    /// difference between the three producers, and now a property of one impl.
    #[derive(Default)]
    struct LegStat {
        active: Cell<bool>,
        delivered: Cell<u64>,
        discarded: Cell<u64>,
    }

    impl HostwardChannelStat for LegStat {
        fn set_active(&self) {
            self.active.set(true);
        }
        fn add_delivered(&self, n: u64) {
            self.delivered.set(self.delivered.get() + n);
        }
        fn unattached(&self) -> &dyn LossCounter {
            &self.discarded
        }
        fn add_dropped_full(&self, n: u64) {
            self.discarded.add(n);
        }
    }

    #[test]
    fn route_channel_data_credits_only_what_a_live_sink_took() {
        let (live, mut rx, _c) = sink(4);
        let sinks = FanOutList::new();
        sinks.attach(live);
        let stat = CodecStat::default();
        route_channel_data(
            &Chunk::copy_from_slice(b"hello"),
            None,
            Some(&sinks),
            Some(&stat),
        );
        assert!(stat.active.get());
        assert_eq!(stat.delivered.get(), 5);
        assert_eq!(stat.unattached.get(), 0);
        assert_eq!(&rx.try_recv().expect("delivered")[..], b"hello");
    }

    #[test]
    fn route_channel_data_charges_a_channel_with_no_fan_out_at_all() {
        let stat = CodecStat::default();
        route_channel_data(&Chunk::copy_from_slice(b"hello"), None, None, Some(&stat));
        assert_eq!(stat.delivered.get(), 0);
        assert_eq!(stat.unattached.get(), 5, "nothing was bound to take it");
    }

    #[test]
    fn route_channel_data_charges_an_all_closed_sink_set_as_unattached() {
        let (dead, rx, _c) = sink(4);
        drop(rx);
        let sinks = FanOutList::new();
        sinks.attach(dead);
        let stat = CodecStat::default();
        route_channel_data(
            &Chunk::copy_from_slice(b"hello"),
            None,
            Some(&sinks),
            Some(&stat),
        );
        assert_eq!(stat.delivered.get(), 0);
        assert_eq!(stat.unattached.get(), 5);
    }

    #[test]
    fn route_channel_data_does_not_credit_what_a_full_sink_dropped() {
        // LEGD-2, now proved once for every producer rather than once per node: a
        // full sink is still `live`, so crediting the whole chunk would report bytes
        // delivered that no consumer received.
        let (full, _full_rx, counters) = sink(1);
        full.tx
            .try_send(Chunk::copy_from_slice(b"x"))
            .expect("fill the slow consumer");
        let sinks = FanOutList::new();
        sinks.attach(full);

        let codec = CodecStat::default();
        route_channel_data(
            &Chunk::copy_from_slice(b"hello"),
            None,
            Some(&sinks),
            Some(&codec),
        );
        assert_eq!(codec.delivered.get(), 0, "the full sink took nothing");
        assert_eq!(
            codec.unattached.get(),
            0,
            "a full sink is live, so this is not unattached loss"
        );
        assert_eq!(counters.dropped_full(), 5, "charged at the boundary (§5)");

        // Same traffic, the leg's rule: the full-buffer loss is mirrored per channel.
        let leg = LegStat::default();
        route_channel_data(
            &Chunk::copy_from_slice(b"hello"),
            None,
            Some(&sinks),
            Some(&leg),
        );
        assert_eq!(leg.delivered.get(), 0);
        assert_eq!(
            leg.discarded.get(),
            5,
            "the leg's counter covers the full-buffer drop too"
        );
    }

    #[test]
    fn route_channel_data_credits_a_mixed_fan_out_to_the_consumer_that_took_it() {
        // LEGD-2's *second* direction, and the one its first fix introduced: with
        // sinks [Ok, Full] the per-sink `dropped_full` equals the chunk length, so
        // `n - dropped_full` credited **zero** delivered for a chunk a live consumer
        // received in full. A mixed chunk is legitimately both — the two counters
        // measure different consumers — and only the chunk no sink accepted is
        // undelivered. Fails against `add_delivered(n.saturating_sub(dropped_full))`.
        let (full, _full_rx, full_counters) = sink(1);
        full.tx
            .try_send(Chunk::copy_from_slice(b"x"))
            .expect("fill the slow consumer");
        let (live, mut live_rx, live_counters) = sink(4);
        let sinks = FanOutList::new();
        sinks.attach(full);
        sinks.attach(live);

        let codec = CodecStat::default();
        route_channel_data(
            &Chunk::copy_from_slice(b"hello"),
            None,
            Some(&sinks),
            Some(&codec),
        );
        assert_eq!(
            &live_rx.try_recv().expect("the fast consumer got it")[..],
            b"hello",
            "the premise: one consumer really did receive the whole chunk"
        );
        assert_eq!(
            codec.delivered.get(),
            5,
            "a chunk a live consumer received in full is a delivery (LEGD-2)"
        );
        assert_eq!(codec.unattached.get(), 0);
        assert_eq!(
            full_counters.dropped_full(),
            5,
            "charged at the boundary (§5)"
        );
        assert_eq!(live_counters.dropped_full(), 0);

        // The leg mirrors the slow consumer's loss per channel, so the same chunk is
        // reported as delivered *and* discarded. That is not double-counting: it is
        // two consumers, and the alternative is under-reporting one of them.
        let leg = LegStat::default();
        route_channel_data(
            &Chunk::copy_from_slice(b"hello"),
            None,
            Some(&sinks),
            Some(&leg),
        );
        assert_eq!(leg.delivered.get(), 5);
        assert_eq!(leg.discarded.get(), 5, "the slow consumer's own loss");
    }

    #[test]
    fn route_channel_data_mirrors_to_the_feed_without_masking_unattached_loss() {
        // Invariant 9: the ring is a spy *outside* the graph, so a tapped chunk that
        // reached no graph consumer is still charged.
        let (hub, active, feed_dropped) = TapHub::new("ep", 64);
        let (tx, mut feed_rx) = mpsc::channel(TAP_FEED_CAP);
        let feed = TapFeed {
            tx,
            active,
            feed_dropped,
        };
        let stat = CodecStat::default();
        route_channel_data(
            &Chunk::copy_from_slice(b"hello"),
            Some(&feed),
            None,
            Some(&stat),
        );
        assert_eq!(&feed_rx.try_recv().expect("mirrored")[..], b"hello");
        assert_eq!(
            stat.unattached.get(),
            5,
            "a spy outside the graph must not suppress the loss"
        );
        drop(hub);
    }

    #[test]
    fn route_channel_data_absorbs_an_unconfigured_identity_without_a_stat() {
        // No `ChannelStat` exists for an identity the node was never configured for;
        // the caller reports the identity itself and this must not panic or
        // mis-charge a neighbour.
        route_channel_data(&Chunk::copy_from_slice(b"hello"), None, None, None);
    }

    // --- the shared §15.20 park-and-retry shell (SIMPB-2) -----------------------

    /// An exclusive endpoint lock with no origins registered.
    fn empty_lock() -> SharedLock {
        let (notifier, _rx) = broadcast::channel(16);
        Rc::new(LockCell::new(
            "ep",
            EndpointLock::new(Arbitration::Exclusive),
            notifier,
        ))
    }

    #[tokio::test]
    async fn await_grant_leaves_a_closed_endpoint_instead_of_parking() {
        // Clause (1): a torn-down endpoint ends the wait with a defined outcome rather
        // than re-parking forever (§6/§15.20).
        let lock = empty_lock();
        lock.close();
        let out: Option<()> = await_grant(&lock, |_| None).await;
        assert!(out.is_none(), "a closed endpoint must not park");
    }

    #[tokio::test]
    async fn await_grant_does_not_lose_a_wake_that_lands_during_the_attempt() {
        // Clause (2), the whole reason this shell exists: the `Notified` is created and
        // `enable()`d *before* the attempt, so a `wake_waiters` landing between the
        // attempt and the park is not lost. `Notify::notify_waiters` stores no permit,
        // so a shell that enabled *after* its attempt would park forever right here —
        // the stranded-waiter class §15.20/§15.23 exist to close. The timeout is what
        // turns that regression into a failure instead of a hang.
        let lock = empty_lock();
        let attempts = Cell::new(0u32);
        let out = tokio::time::timeout(
            Duration::from_secs(5),
            await_grant(&lock, |_| {
                attempts.set(attempts.get() + 1);
                if attempts.get() == 1 {
                    // A release landing mid-attempt: the wake fires while this
                    // critical section is still running.
                    lock.wake_waiters();
                    None
                } else {
                    Some(())
                }
            }),
        )
        .await
        .expect("a wake landing during the attempt must not be lost");
        assert_eq!(out, Some(()));
        assert_eq!(attempts.get(), 2, "the waiter re-attempted after the wake");
    }

    #[tokio::test]
    async fn reacquire_held_exits_when_its_edge_was_disconnected() {
        // `disconnect` unregisters the origin (§15.35), and an id absent from `origins`
        // satisfies neither `may_write` nor `reclaim_held` while the endpoint itself
        // stays open — so without the `Grant::Refused` clause the pump parks forever
        // holding the chunk it had already taken, and a later `connect` cannot revive
        // it (the new edge gets a new id).
        let lock = empty_lock();
        let id = OriginId(3);
        lock.with_mut(|g| g.register(id, "target", WriteMode::Held));
        assert!(
            reacquire_held(&lock, id).await,
            "a held origin holds the lock"
        );
        lock.with_mut(|g| g.unregister(id)); // the edge goes away under the pump
        let still_writable =
            tokio::time::timeout(Duration::from_secs(5), reacquire_held(&lock, id))
                .await
                .expect("an unregistered origin must not park forever");
        assert!(
            !still_writable,
            "an unregistered origin can never be granted"
        );
    }

    // --- the shared edge attachment (SIMPB-1) -----------------------------------

    /// One attached edge and everything a test inspects it through.
    struct Attached {
        out: AttachOutcome,
        lock: SharedLock,
        slot: SharedTargetEdge,
        inbox: mpsc::Receiver<mpsc::Receiver<Chunk>>,
        /// The host endpoint's targetward receiver, held so its channel stays open.
        _host_rx: mpsc::Receiver<Chunk>,
    }

    /// Attach one edge with `mode` through the shared helper both callers use.
    fn attach_one(mode: WriteMode) -> Attached {
        let lock = empty_lock();
        let (host_tx, host_rx) = mpsc::channel::<Chunk>(4);
        let (inbox_tx, inbox_rx) = mpsc::channel(EDGE_INBOX_CAP);
        let slot = TargetEdge::new();
        let out = attach_edge(
            EdgeParts {
                target: "logger".parse().expect("address parses"),
                origin: OriginId(9),
                lock: Some(lock.clone()),
                host_targetward: Some(host_tx),
                fanout: FanOutList::new(),
                inbox: inbox_tx,
                slot: slot.clone(),
                counters: Arc::new(DropCounters::default()),
            },
            mode,
            4,
        );
        Attached {
            out,
            lock,
            slot,
            inbox: inbox_rx,
            _host_rx: host_rx,
        }
    }

    #[tokio::test]
    async fn attach_edge_registers_a_read_only_edge_without_making_it_addressable() {
        // The distinction both callers now take from one place: a `never` edge is a
        // *real* attachment — registered on the lock so `state` lists it and
        // `disconnect` can unregister it — that has no writer and is not addressable
        // by `lock`/`unlock`.
        let mut a = attach_one(WriteMode::Never);
        let (out, lock, slot) = (&a.out, &a.lock, &a.slot);
        assert!(
            out.registered.is_some(),
            "every mode registers on the lock, `never` included"
        );
        assert!(!out.addressable, "a `never` edge is not lock-addressable");
        assert!(!out.granted, "a `never` origin cannot hold the lock");
        assert!(out.consumer_live, "the inbox took the hostward receiver");
        assert!(slot.with(|e| e.attached), "attached, but not writable");
        assert!(slot.origin().is_none(), "no writable origin");
        assert_eq!(
            lock.with(|g| g.write_mode(OriginId(9))),
            Some(WriteMode::Never)
        );
        assert!(a.inbox.recv().await.is_some());
    }

    #[tokio::test]
    async fn attach_edge_grants_a_held_edge_on_a_free_exclusive_endpoint() {
        // The `granted` flag `connect` runs purge-on-acquire on (§6): `register`
        // itself grants a `held` origin a free exclusive lock, and that is a *fresh*
        // grant. At load it is vacuous; on a running graph it is the stale-command
        // hazard, which is why the outcome reports it instead of the caller re-deriving
        // `holder() == Some(id)`.
        let a = attach_one(WriteMode::Held);
        let (out, lock, slot) = (&a.out, &a.lock, &a.slot);
        assert!(out.granted, "a held origin takes a free exclusive lock");
        assert!(out.addressable, "and is addressable by `lock`/`unlock`");
        assert_eq!(lock.with(|g| g.holder()), Some(OriginId(9)));
        assert!(slot.origin().is_some(), "a writable origin was installed");
    }

    // --- the shared targetward send-and-charge step (SIMP-2) --------------------

    #[test]
    fn targetward_loss_charges_the_counter_and_reports_non_delivery() {
        let lost = Cell::new(0u64);
        assert!(TargetwardLoss::none().charge(&lost));
        assert_eq!(lost.get(), 0);
        assert!(!TargetwardLoss::of(7).charge(&lost));
        assert_eq!(lost.get(), 7);
    }

    /// A held origin bound to `tx` — the shape [`EdgeSlot::origin`] hands out.
    fn held_origin(tx: mpsc::Sender<Chunk>) -> WritableOrigin {
        let id = OriginId(1);
        let mut lock = EndpointLock::new(Arbitration::Exclusive);
        lock.register(id, "origin", WriteMode::Held); // acquires on attach
        let (notifier, _rx) = broadcast::channel(16);
        (tx, Rc::new(LockCell::new("ep", lock, notifier)), id)
    }

    #[tokio::test]
    async fn forward_targetward_delivers_and_charges_nothing() {
        let (tx, mut rx) = mpsc::channel::<Chunk>(4);
        let origin = held_origin(tx);
        let lost = Cell::new(0u64);
        let delivered = forward_targetward(&origin, 5, || Some(Chunk::copy_from_slice(b"hello")))
            .await
            .charge(&lost);
        assert!(delivered);
        assert_eq!(lost.get(), 0);
        assert_eq!(&rx.recv().await.expect("delivered")[..], b"hello");
    }

    #[tokio::test]
    async fn forward_targetward_charges_the_cost_when_the_endpoint_is_gone() {
        // The exit CODEXEC-2 returned through without counting, now unreachable
        // without naming a counter.
        let (tx, rx) = mpsc::channel::<Chunk>(4);
        drop(rx);
        let origin = held_origin(tx);
        let lost = Cell::new(0u64);
        let delivered = forward_targetward(&origin, 5, || Some(Chunk::copy_from_slice(b"hello")))
            .await
            .charge(&lost);
        assert!(!delivered);
        assert_eq!(lost.get(), 5);
    }

    #[tokio::test]
    async fn forward_targetward_charges_the_cost_not_the_payload_length() {
        // The map's case: it charges the bytes that *arrived* while sending the
        // mapped bytes, so a substituting rule cannot make the loss count drift.
        let (tx, rx) = mpsc::channel::<Chunk>(4);
        drop(rx);
        let origin = held_origin(tx);
        let lost = Cell::new(0u64);
        forward_targetward(&origin, 40, || Some(Chunk::copy_from_slice(b"xy")))
            .await
            .charge(&lost);
        assert_eq!(
            lost.get(),
            40,
            "the arriving byte count, not the mapped one"
        );
    }

    #[tokio::test]
    async fn forward_targetward_treats_an_empty_payload_as_no_loss() {
        // A fully deleted targetward chunk is the operator's explicit intent (§7.8),
        // not a §5 loss.
        let (tx, mut rx) = mpsc::channel::<Chunk>(4);
        let origin = held_origin(tx);
        let lost = Cell::new(0u64);
        assert!(forward_targetward(&origin, 9, || None).await.charge(&lost));
        assert_eq!(lost.get(), 0);
        assert!(rx.try_recv().is_err(), "nothing was written");
    }

    #[test]
    fn try_forward_targetward_hands_a_full_endpoint_its_chunk_back_uncharged() {
        // CONC-1: a full endpoint must not cost a byte — the caller holds the chunk
        // and stops reading, which backpressures the client instead.
        let (tx, _rx) = mpsc::channel::<Chunk>(1);
        tx.try_send(Chunk::copy_from_slice(b"x")).expect("fill it");
        let origin = held_origin(tx);
        let lost = Cell::new(0u64);
        match try_forward_targetward(&origin, Chunk::copy_from_slice(b"hello"), &lost) {
            Handoff::Full(back) => assert_eq!(&back[..], b"hello", "handed back intact"),
            _ => panic!("a full endpoint must return the chunk"),
        }
        assert_eq!(lost.get(), 0, "delayed, not dropped (§5)");
    }

    #[test]
    fn try_forward_targetward_charges_a_closed_endpoint() {
        let (tx, rx) = mpsc::channel::<Chunk>(1);
        drop(rx);
        let origin = held_origin(tx);
        let lost = Cell::new(0u64);
        assert!(matches!(
            try_forward_targetward(&origin, Chunk::copy_from_slice(b"hello"), &lost),
            Handoff::Lost
        ));
        assert_eq!(lost.get(), 5);
    }

    #[test]
    fn try_forward_targetward_delivers_without_touching_the_lock() {
        // The pty's held-payload retry is deliberately not gated on arbitration: the
        // bytes are already out of the master and were read while this origin could
        // write, so a steal landing in between must not strand them.
        let (tx, mut rx) = mpsc::channel::<Chunk>(4);
        let origin = held_origin(tx);
        let thief = OriginId(2);
        origin.1.with_mut(|g| {
            g.register(thief, "stealer", WriteMode::OnDemand);
            g.steal(thief);
        });
        assert!(!origin.1.with(|g| g.may_write(origin.2)), "ousted");
        let lost = Cell::new(0u64);
        assert!(matches!(
            try_forward_targetward(&origin, Chunk::copy_from_slice(b"hello"), &lost),
            Handoff::Taken
        ));
        assert_eq!(lost.get(), 0);
        assert_eq!(&rx.try_recv().expect("delivered")[..], b"hello");
    }
}
