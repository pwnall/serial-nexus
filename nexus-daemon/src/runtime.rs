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
    /// no-op when nobody is subscribed. Must be called with no outstanding borrow.
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

/// Split `bytes` into consecutive envelope [`Event::data`] frames on `channel`,
/// yielding `(piece_len, frame)` per piece — the envelope-framing wrapper over the
/// shared [`frame_ranges`] boundary helper (§15.24). The peer/child reassembles per
/// channel. Shared by the leg's write half and the exec codec's stdin feed (§5
/// no-drop / all-loss-counted).
pub(crate) fn data_frames<'a>(
    channel: &'a str,
    bytes: &'a Chunk,
) -> impl Iterator<Item = (usize, Vec<u8>)> + 'a {
    frame_ranges(channel, bytes.len()).map_while(move |(off, end)| {
        let mut frame = Vec::new();
        // Encode is infallible for a sane channel id (the graph forbids the
        // pathological ones); on the defensive error, stop fragmenting this chunk
        // (`map_while` fuses the range iterator, matching the pre-refactor behavior).
        if encode(&Event::data(channel, bytes.slice(off..end)), &mut frame).is_err() {
            return None;
        }
        Some((end - off, frame))
    })
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
    /// host↔target pair. A log target's write mode is inherently `never` (§7.3),
    /// so it gets no targetward path; every other target keeps its declared mode.
    pub fn build(config: &GraphConfig, notifier: &broadcast::Sender<Notification>) -> Wiring {
        // Every endpoint's facing + arbitration, keyed by its address (§4). Derived
        // from each node's shape, so codec channels and multiplexed sides appear
        // alongside single-endpoint boundary nodes.
        let mut facing: HashMap<EndpointAddr, (Facing, Arbitration)> = HashMap::new();
        let mut is_log: HashMap<&str, bool> = HashMap::new();
        // A serial node's configured hostward-consumer drop policy (§5, §7.1): the
        // fan-out buffer depth to each of its consumers. Other producers (codec
        // channels) use the built-in default.
        let mut host_hostward_depth: HashMap<&str, usize> = HashMap::new();
        // A host-facing endpoint's configured replay-ring depth in bytes (§5, §17).
        // Only a serial node's single host endpoint carries the attribute today;
        // codec/leg host-facing channels default to 0 (their per-channel ring config
        // is deferred work, §14). Every host endpoint still gets a hub — a tap can
        // attach to any of them — but only a configured one keeps a ring.
        let mut host_ring_cap: HashMap<EndpointAddr, usize> = HashMap::new();
        for n in &config.nodes {
            for ep in &n.shape().endpoints {
                facing.insert(
                    EndpointAddr::new(n.name(), ep.name.clone()),
                    (ep.facing, ep.arbitration),
                );
            }
            is_log.insert(n.name(), matches!(n, NodeConfig::Log { .. }));
            if let NodeConfig::Serial {
                hostward_buffer,
                replay_ring,
                ..
            } = n
            {
                host_hostward_depth.insert(n.name(), *hostward_buffer);
                host_ring_cap.insert(EndpointAddr::node(n.name()), *replay_ring);
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
            // it. A log target is inherently `never`; every other edge carries its
            // declared mode. The origin's label is its display address.
            let mode = if is_log.get(target.node.as_str()).copied().unwrap_or(false) {
                WriteMode::Never
            } else {
                edge.write_mode
            };
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
