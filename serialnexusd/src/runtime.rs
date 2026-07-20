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
//! Both directions run as `spawn_local` tasks on the current-thread runtime
//! (plan §2), sharing each kernel object through an `Rc<_>`.
//!
//! Readiness is driven by a non-blocking `poll(2)` (`sys::poll_ready`) plus a
//! short async sleep when idle — *not* `tokio::io::unix::AsyncFd`. On a pty
//! master, `AsyncFd`'s epoll readiness spuriously and persistently fires
//! "readable" and busy-loops the single-threaded runtime (see `sys::poll_ready`
//! and implementation notes §3.10). `poll(2)` with a zero timeout reports the
//! true state and never blocks the thread. During an active transfer a task
//! re-polls immediately after draining (no sleep), so [`IDLE_POLL`] bounds idle
//! latency, not throughput.

use std::collections::HashMap;
use std::os::fd::RawFd;
use std::time::Duration;

use nexus_core::Chunk;
use nexus_core::config::{GraphConfig, NodeConfig};
use nexus_core::graph::Facing;
use nix::poll::PollFlags;
use tokio::sync::mpsc;

use crate::sys;

/// Read-buffer size for one `read(2)` on a boundary fd. A PTY packet-mode read
/// spends one byte on the control marker, leaving the rest for data.
pub const READ_BUF: usize = 8192;

/// Bounded channel depth, in chunks. This is the boundary's buffer: hostward it
/// caps how much a slow PTY buffers before drops begin; targetward it caps how
/// far a producer runs ahead before backpressure suspends the origin.
pub const CHANNEL_CAP: usize = 256;

/// How long a boundary task sleeps between readiness polls when there is nothing
/// to do. During an active transfer the task re-polls immediately after each
/// drain, so this bounds idle latency (and idle CPU), never throughput. Well
/// under the §7.2 sub-second presence requirement.
pub const IDLE_POLL: Duration = Duration::from_millis(5);

/// The channels the data plane hands to each node's `start`, keyed by node name.
/// Built once from the loaded configuration; each node removes its own entries.
#[derive(Default)]
pub struct Wiring {
    /// serial node → one hostward sender per attached PTY (fan-out, §4 rule 2).
    pub serial_hostward: HashMap<String, Vec<mpsc::Sender<Chunk>>>,
    /// serial node → the single targetward receiver (all attached PTYs feed it).
    pub serial_targetward: HashMap<String, mpsc::Receiver<Chunk>>,
    /// PTY node → its hostward receiver (from its serial).
    pub pty_hostward: HashMap<String, mpsc::Receiver<Chunk>>,
    /// PTY node → its targetward sender (into its serial).
    pub pty_targetward: HashMap<String, mpsc::Sender<Chunk>>,
}

impl Wiring {
    /// Build the channel plan for the phase-2 topology: serial (host-facing)
    /// endpoints fanning out to PTY (target-facing) endpoints. The graph is
    /// already structurally valid here (load validates first, §11), so each edge
    /// joins exactly one host and one target endpoint.
    pub fn build(config: &GraphConfig) -> Wiring {
        // Facing of each node's sole endpoint (phase 2: serial=host|target by
        // `faces`, pty/log=target).
        let mut facing: HashMap<&str, Facing> = HashMap::new();
        for n in &config.nodes {
            let f = match n {
                NodeConfig::Serial { faces, .. } => *faces,
                NodeConfig::Pty { .. } | NodeConfig::Log { .. } => Facing::Target,
            };
            facing.insert(n.name(), f);
        }

        let mut wiring = Wiring::default();
        // One targetward sender per serial, cloned to each attached PTY.
        let mut serial_targetward_tx: HashMap<String, mpsc::Sender<Chunk>> = HashMap::new();

        for edge in &config.edges {
            let a = facing.get(edge.a.node.as_str()).copied();
            let b = facing.get(edge.b.node.as_str()).copied();
            // Identify the host (serial) and target (pty) ends. Same-facing or
            // dangling edges can't occur post-validation; skip defensively.
            let (host, target) = match (a, b) {
                (Some(Facing::Host), Some(Facing::Target)) => (&edge.a.node, &edge.b.node),
                (Some(Facing::Target), Some(Facing::Host)) => (&edge.b.node, &edge.a.node),
                _ => continue,
            };

            // Targetward: create the serial's receiver lazily on first edge, and
            // hand every attached PTY a clone of the sender.
            let ttx = serial_targetward_tx
                .entry(host.clone())
                .or_insert_with(|| {
                    let (tx, rx) = mpsc::channel(CHANNEL_CAP);
                    wiring.serial_targetward.insert(host.clone(), rx);
                    tx
                })
                .clone();
            wiring.pty_targetward.insert(target.clone(), ttx);

            // Hostward: one dedicated channel per (serial, pty) edge, so a slow
            // PTY's drops are isolated to its own channel (§5).
            let (htx, hrx) = mpsc::channel(CHANNEL_CAP);
            wiring
                .serial_hostward
                .entry(host.clone())
                .or_default()
                .push(htx);
            wiring.pty_hostward.insert(target.clone(), hrx);
        }

        wiring
    }
}

/// Write every byte of `data` to a boundary fd, waiting for writability via a
/// non-blocking `poll(2)` between partial writes. This is the boundary draining
/// at its own pace: upstream buffering (and any drops) happen in the feeding
/// channel, never here. `Err` means the peer hung up.
pub async fn write_all(fd: RawFd, mut data: &[u8]) -> std::io::Result<()> {
    while !data.is_empty() {
        match sys::write_fd(fd, data) {
            Ok(0) => return Err(std::io::ErrorKind::WriteZero.into()),
            Ok(n) => data = &data[n..],
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // Wait for writability without blocking the runtime.
                loop {
                    let re = sys::poll_ready(fd, PollFlags::POLLOUT | PollFlags::POLLHUP);
                    if re.contains(PollFlags::POLLOUT) {
                        break;
                    }
                    if re.contains(PollFlags::POLLHUP) {
                        return Err(std::io::ErrorKind::BrokenPipe.into());
                    }
                    tokio::time::sleep(IDLE_POLL).await;
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
