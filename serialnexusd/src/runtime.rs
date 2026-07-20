//! The data-plane runtime (design §5). Slice 2 wires real bytes serial↔PTY.
//!
//! The §5 boundary policies are realized with bounded `tokio::sync::mpsc`
//! channels between node tasks — the channel *is* the "bounded buffering where
//! configured" a boundary owns:
//!
//! * **Hostward** (serial → PTYs) is lossy at the boundary: the serial reader
//!   `try_send`s a chunk to each attached PTY and drops on a full channel (a
//!   slow consumer costs only itself, §5). Counters land in phase 3.
//! * **Targetward** (PTY → serial) is backpressured to the origin: the PTY
//!   reader `send().await`s into the serial's bounded channel; a full channel
//!   suspends the reader, the kernel buffers on the client's side of the PTY,
//!   and nothing is dropped (§5).
//!
//! The pure `nexus_core::data` contracts remain the property-tested spec of the
//! same semantics; the interior holdover they model is exercised when codec
//! (interior) nodes arrive in phase 5. Phase 2 has no interior nodes, so the two
//! boundaries connect directly through these channels.
//!
//! Readiness is driven by `poll(2)`, *never* `tokio::io::unix::AsyncFd`: on a pty
//! master, `AsyncFd`'s epoll readiness spuriously and persistently fires
//! "readable" and busy-loops the single-threaded runtime (§15.18). Two shapes,
//! per §15.18:
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
//!   is §15.18's "spawn_blocking reader threads" escape hatch. Cross-thread
//!   counters are therefore atomic ([`DropCounters`]).

use std::collections::HashMap;
use std::os::fd::RawFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use nexus_core::Chunk;
use nexus_core::config::{GraphConfig, NodeConfig};
use nexus_core::graph::Facing;
use nix::poll::PollFlags;
use tokio::sync::mpsc;

use crate::sys;

/// Hostward drop counters for one consuming boundary (§5). All hostward loss is
/// counted at the boundary that drops it, so it is always located, counted, and
/// attributable — a slow spy costs itself data, never its neighbors. One instance
/// is shared (via `Arc`) between the producing serial reader — which counts
/// full-buffer drops and, since the high-throughput reader runs on a dedicated
/// blocking thread (§15.18), needs the counters to be `Send`/`Sync`, hence
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

/// The channels the data plane hands to each node's `start`, keyed by node name.
/// Built once from the loaded configuration; each node removes its own entries.
#[derive(Default)]
pub struct Wiring {
    /// serial node → one hostward sink per attached consumer (fan-out, §4 rule 2).
    pub serial_hostward: HashMap<String, Vec<HostwardSink>>,
    /// serial node → the single targetward receiver (all writing consumers feed it).
    pub serial_targetward: HashMap<String, mpsc::Receiver<Chunk>>,
    /// consumer node (PTY or log) → its hostward receiver (from its serial).
    pub consumer_hostward: HashMap<String, mpsc::Receiver<Chunk>>,
    /// consumer node → its [`DropCounters`] (shared with the serial hostward
    /// sink), for drop/discard counts and state reporting (§5, §7.2, §7.3).
    pub consumer_counters: HashMap<String, Arc<DropCounters>>,
    /// PTY node → its targetward sender (into its serial). Only targetward-writing
    /// consumers appear here; a log node's write mode is inherently `never` (§7.3).
    pub pty_targetward: HashMap<String, mpsc::Sender<Chunk>>,
}

impl Wiring {
    /// Build the channel plan for the phase-2/3 topology: serial (host-facing)
    /// endpoints fanning out to target-facing consumers (PTY and log). The graph
    /// is already structurally valid here (load validates first, §11), so each
    /// edge joins exactly one host and one target endpoint. Every consumer gets a
    /// hostward channel; only targetward-writing consumers (PTY) get a targetward
    /// sender — a log's write mode is inherently `never` (§7.3).
    pub fn build(config: &GraphConfig) -> Wiring {
        let mut facing: HashMap<&str, Facing> = HashMap::new();
        let mut writes_targetward: HashMap<&str, bool> = HashMap::new();
        for n in &config.nodes {
            let f = match n {
                NodeConfig::Serial { faces, .. } => *faces,
                NodeConfig::Pty { .. } | NodeConfig::Log { .. } => Facing::Target,
            };
            facing.insert(n.name(), f);
            // Log nodes never write targetward; PTYs (and serials) can.
            writes_targetward.insert(n.name(), !matches!(n, NodeConfig::Log { .. }));
        }

        let mut wiring = Wiring::default();
        // One targetward sender per serial, cloned to each writing consumer.
        let mut serial_targetward_tx: HashMap<String, mpsc::Sender<Chunk>> = HashMap::new();

        for edge in &config.edges {
            let a = facing.get(edge.a.node.as_str()).copied();
            let b = facing.get(edge.b.node.as_str()).copied();
            // Identify the host (serial) and target (consumer) ends. Same-facing
            // or dangling edges can't occur post-validation; skip defensively.
            let (host, target) = match (a, b) {
                (Some(Facing::Host), Some(Facing::Target)) => (&edge.a.node, &edge.b.node),
                (Some(Facing::Target), Some(Facing::Host)) => (&edge.b.node, &edge.a.node),
                _ => continue,
            };

            // Targetward: only for consumers that write back (PTY). Create the
            // serial's receiver lazily on the first such edge.
            if writes_targetward
                .get(target.as_str())
                .copied()
                .unwrap_or(true)
            {
                let ttx = serial_targetward_tx
                    .entry(host.clone())
                    .or_insert_with(|| {
                        let (tx, rx) = mpsc::channel(CHANNEL_CAP);
                        wiring.serial_targetward.insert(host.clone(), rx);
                        tx
                    })
                    .clone();
                wiring.pty_targetward.insert(target.clone(), ttx);
            }

            // Hostward: one dedicated channel per (serial, consumer) edge, so a
            // slow consumer's drops are isolated to its own channel (§5). One
            // shared DropCounters rides with both ends — the serial reader counts
            // full-buffer drops, the consumer counts its own boundary discards.
            let (htx, hrx) = mpsc::channel(CHANNEL_CAP);
            let counters = Arc::new(DropCounters::default());
            wiring
                .serial_hostward
                .entry(host.clone())
                .or_default()
                .push((htx, counters.clone()));
            wiring.consumer_hostward.insert(target.clone(), hrx);
            wiring.consumer_counters.insert(target.clone(), counters);
        }

        wiring
    }
}

/// The readiness-poll interval during an *active* transfer: short, so a momentary
/// empty/full buffer mid-stream is rechecked in ~1ms (the tokio timer floor)
/// rather than the 5ms [`IDLE_POLL`] — the difference between ~1 MB/s and tens of
/// MB/s. A boundary resets its wait to this on every byte of progress, then lets
/// it back off toward [`IDLE_POLL`] (§15.18: bound idle latency, never
/// throughput; a `yield_now` spin does nothing here because the peer is a
/// separate process that only advances as real wall-clock passes).
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
/// at full rate (§15.18).
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
