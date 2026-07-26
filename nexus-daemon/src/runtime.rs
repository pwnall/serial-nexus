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

/// Ensure `id` holds its endpoint's write lock, re-acquiring through the FIFO
/// queue if a `send --steal` transiently ousted the held origin (§6 held
/// priority). Returns `false` if the endpoint was torn down. The fast path (the
/// normal held case) is a single synchronous borrow; the slow path parks on the
/// lock's `Notify`, holding no borrow across the await (§15.20). Shared by the
/// in-process codec and the exec codec — the two held-origin targetward gates.
pub(crate) async fn reacquire_held(lock: &SharedLock, id: OriginId) -> bool {
    if lock.with(|g| g.may_write(id)) {
        return true; // already holds it
    }
    loop {
        if lock.is_closed() {
            return false;
        }
        // Enable the wake future before the reclaim attempt (lost-wakeup-free).
        let notified = lock.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();

        // Already holds (re-granted), or reclaim as a held origin ahead of any
        // on-demand waiter (§6 held priority). Only a fresh reclaim emits a change.
        let outcome = lock.with_mut(|g| {
            if g.may_write(id) {
                Some(false)
            } else if g.reclaim_held(id) {
                Some(true)
            } else {
                None
            }
        });
        match outcome {
            Some(fresh) => {
                if fresh {
                    lock.emit_change();
                }
                return true;
            }
            None => notified.await,
        }
    }
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
/// overflows [`MAX_FRAME_SIZE`]. That is pathological rather than impossible —
/// nothing bounds identity length structurally today — so the tail is reported as
/// [`DataFrame::Residual`] instead of vanishing.
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

/// A monotonic byte counter a hostward [`fan_out`] charges its unattached loss to.
///
/// Two shapes exist in the daemon and both must work through one helper: the serial
/// reader's `Arc<AtomicU64>` (its producer runs on a dedicated blocking thread,
/// §15.19, so the counter has to be `Send`/`Sync`) and every interior node's
/// single-threaded `Cell<u64>`. Taking the accounting as a trait object is what lets
/// the five former hand-rolled fan-out loops collapse into one (§16, F1) without
/// either side reshaping its counter.
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

/// What one hostward [`fan_out`] did with its chunk, for the producer's own
/// per-node/per-channel bookkeeping.
pub(crate) struct FanOut {
    /// Whether at least one *live* sink took the chunk (or was charged a
    /// full-buffer drop for it). `false` means the chunk reached no live graph
    /// consumer at all — the empty-sinks and all-`Closed` cases — and its bytes
    /// have already been charged to the caller's unattached counter.
    pub live: bool,
    /// Bytes dropped at a sink whose bounded buffer was full. Already charged to
    /// that sink's own [`DropCounters`] (§5: loss is counted at the boundary that
    /// drops it); returned so a producer that *also* mirrors full-buffer loss in a
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
/// * **Delivered** — the sink took it; nothing to count.
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
    sinks: &[HostwardSink],
    unattached: &dyn LossCounter,
) -> FanOut {
    let n = chunk.len() as u64;
    let mut out = FanOut {
        live: false,
        dropped_full: 0,
    };
    for (tx, counters) in sinks {
        match tx.try_send(chunk.clone()) {
            // Delivered to a live consumer.
            Ok(()) => out.live = true,
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

/// A hostward fan-out target: a bounded sender into one consuming boundary,
/// paired with that boundary's [`DropCounters`] so a full-buffer drop is counted
/// where it happens (§5).
pub type HostwardSink = (mpsc::Sender<Chunk>, Arc<DropCounters>);

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
    /// Host-facing endpoint → one hostward sink per attached consumer (fan-out,
    /// §4 rule 2).
    pub host_sinks: HashMap<EndpointAddr, Vec<HostwardSink>>,
    /// Host-facing endpoint → the single targetward receiver it drains (all its
    /// writing origins feed this one channel, arbitrated by the lock).
    pub host_targetward_rx: HashMap<EndpointAddr, mpsc::Receiver<Chunk>>,
    /// Host-facing endpoint → a targetward sender into it, so the `send` verb can
    /// inject a line as a transient origin even with no writer attached (§6).
    pub host_targetward_tx: HashMap<EndpointAddr, mpsc::Sender<Chunk>>,
    // --- target-facing endpoints (PTY, log, codec multiplexed side) ---
    /// Target-facing endpoint → its hostward receiver (from its one host endpoint).
    pub target_hostward_rx: HashMap<EndpointAddr, mpsc::Receiver<Chunk>>,
    /// Target-facing endpoint → its [`DropCounters`] (shared with the host sink),
    /// for drop/discard counts and state reporting (§5, §7.2, §7.3).
    pub target_counters: HashMap<EndpointAddr, Arc<DropCounters>>,
    /// Writing target-facing endpoint → its targetward sender into its host
    /// endpoint. Only origins that can write (mode ≠ never) appear here.
    pub target_targetward_tx: HashMap<EndpointAddr, mpsc::Sender<Chunk>>,
    /// Writing target-facing endpoint → (its host endpoint's lock, its origin id).
    /// The origin gates its targetward drain on this (§6); only writers appear.
    pub origin_locks: HashMap<EndpointAddr, (SharedLock, OriginId)>,
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
        // A serial node's configured hostward-consumer drop policy (§5, §7.1): the
        // fan-out buffer depth to each of its consumers. Other producers (codec
        // channels) use the built-in default.
        let mut host_hostward_depth: HashMap<&str, usize> = HashMap::new();
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
            if let NodeConfig::Serial {
                hostward_buffer, ..
            } = n
            {
                host_hostward_depth.insert(n.name(), *hostward_buffer);
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

            // Register this attachment as an origin on the host endpoint's lock
            // (§6), labelled by the target's address so `lock`/`unlock` can name
            // it. The two configuration-to-runtime promotions — a log target forces
            // `never` (§7.3), a map's `raw` endpoint promotes `on-demand` to `held`
            // (§7.8) — live in `GraphConfig::effective_write_mode`, not here: the
            // validator reasons about the same effective modes the wiring registers,
            // so the two can never drift (§16 "one rule, one place"). The origin's
            // label is its display address.
            let mode = config.effective_write_mode(edge);
            let origin_id = OriginId(next_origin);
            next_origin += 1;
            if let Some(lock) = wiring.endpoint_locks.get(host) {
                lock.with_mut(|l| l.register(origin_id, target.to_string(), mode));
            }

            // Targetward: only origins that can write (mode ≠ never) get a path to
            // the host endpoint and a lock handle to gate their drain (§6).
            if mode != WriteMode::Never {
                if let Some(ttx) = wiring.host_targetward_tx.get(host) {
                    wiring
                        .target_targetward_tx
                        .insert(target.clone(), ttx.clone());
                }
                if let Some(lock) = wiring.endpoint_locks.get(host) {
                    wiring
                        .origin_locks
                        .insert(target.clone(), (lock.clone(), origin_id));
                }
            }

            // Hostward: one dedicated channel per (host, target) edge, so a slow
            // consumer's drops are isolated to its own channel (§5). One shared
            // DropCounters rides with both ends — the producer counts full-buffer
            // drops, the consumer counts its own boundary discards. Depth is the
            // producing serial's configured hostward buffer (§7.1), else default.
            let depth = host_hostward_depth
                .get(host.node.as_str())
                .copied()
                .unwrap_or(CHANNEL_CAP);
            let (htx, hrx) = mpsc::channel(depth);
            let counters = Arc::new(DropCounters::default());
            wiring
                .host_sinks
                .entry(host.clone())
                .or_default()
                .push((htx, counters.clone()));
            wiring.target_hostward_rx.insert(target.clone(), hrx);
            wiring.target_counters.insert(target.clone(), counters);
        }

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

    fn sink(cap: usize) -> (HostwardSink, mpsc::Receiver<Chunk>, Arc<DropCounters>) {
        let (tx, rx) = mpsc::channel::<Chunk>(cap);
        let counters = Arc::new(DropCounters::default());
        ((tx, counters.clone()), rx, counters)
    }

    #[test]
    fn fan_out_with_no_sinks_counts_the_whole_chunk_as_unattached() {
        let unattached = Cell::new(0u64);
        let chunk = Chunk::copy_from_slice(b"hello");
        let out = fan_out(&chunk, &[], &unattached);
        assert!(!out.live);
        assert_eq!(out.dropped_full, 0);
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
            .0
            .try_send(Chunk::copy_from_slice(b"x"))
            .expect("fill the slow consumer's buffer");
        let (live_sink, mut live_rx, live_counters) = sink(4);

        let unattached = Cell::new(0u64);
        let chunk = Chunk::copy_from_slice(b"hello");
        let out = fan_out(&chunk, &[full_sink, live_sink], &unattached);

        assert!(out.live);
        assert_eq!(out.dropped_full, 5, "the full sink's drop is reported back");
        assert_eq!(full_counters.dropped_full(), 5, "charged where it dropped");
        assert_eq!(live_counters.dropped_full(), 0);
        assert_eq!(unattached.get(), 0, "a live consumer took it");
        assert_eq!(&live_rx.try_recv().expect("delivered")[..], b"hello");
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
}
