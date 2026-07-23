//! Codec node (design §7.5): the interior demux/remux protocol transform. The
//! compiled-in codec registry that instantiates these (§8/§15.26) lives in
//! [`crate::registry`]; this module is the running node.
//!
//! **Orientation.** Phase 5 implements the **demultiplexer** (`faces = target`):
//! the multiplexed side is the node's default endpoint, facing the device across
//! a serial; N channel endpoints face host consumers. Hostward, raw multiplexed
//! bytes are `demux`ed into per-channel events and fanned out; targetward,
//! per-channel writes are `mux`ed back into the multiplexed stream and forwarded
//! to the device. The **re-multiplexer** (`faces = host`) is the mirror, driven
//! by a leg (phase 6); a codec configured that way comes up faulted with a clear
//! reason, so the config stays loadable and the gap is visible in state (§15.8).
//!
//! **Interior contract (§5).** The codec holds only parser state (a partial
//! frame, bounded by the frame size) — no queues. It runs on the async runtime;
//! the synchronous `demux`/`mux` transforms execute in the task's context, and
//! the bounded mpsc channels to its serial and PTY neighbours are *their* boundary
//! buffers, not the codec's.
//!
//! **The held lock (§6).** The demultiplexer's edge to the serial holds that
//! endpoint's write lock permanently (any other writer would corrupt the mux
//! framing). A `send --steal` at the serial ousts the codec transiently; each
//! channel's targetward task then parks its one framed chunk and re-acquires the
//! lock (FIFO) once the stealer releases — the §6 stall, with commands delayed,
//! never dropped.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use codec_api::{Codec, Event, EventKind, MAX_FRAME_SIZE};
use nexus_core::Chunk;
use nexus_core::NodeStatus;
use nexus_core::config::NodeConfig;
use nexus_core::graph::{EndpointAddr, Facing};
use nexus_core::lock::OriginId;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::task::JoinHandle;

use crate::cell::CriticalCell;
use crate::runtime::{DropCounters, HostwardSink, SharedLock, Wiring, reacquire_held};

/// Per-channel observed counters (§7.5). All access is on the one runtime thread,
/// so `Cell` suffices.
#[derive(Default)]
struct ChannelStat {
    /// Bytes handed hostward to this channel's consumers (device → consumers). A
    /// per-consumer slow-buffer drop is counted separately at that boundary (§5).
    delivered_hostward: Cell<u64>,
    /// Bytes discarded because this channel is configured but has no consumer bound
    /// — a §5 loss counted where it happens, not silently dropped.
    discarded_unattached: Cell<u64>,
    /// Channel bytes forwarded targetward to the device. Freezes while the codec
    /// does not hold the serial's write lock — the observable §6 stall on a stolen
    /// held lock (item 6).
    accepted_targetward: Cell<u64>,
    /// Channel bytes that could not be framed targetward and were therefore dropped
    /// — a §5 loss counted where it happens. Unreachable for the envelope codec
    /// (each oversize chunk is fragmented so every piece provably fits a frame); a
    /// defensive count for a custom transform whose `mux` refuses a piece.
    discarded_targetward: Cell<u64>,
    /// Whether the channel has been seen active (an `open`, or any `data`).
    active: Cell<bool>,
}

pub struct CodecNode {
    pub name: String,
    codec_name: String,
    faces: Facing,
    channels: Vec<String>,
    /// The transform, shared between the hostward demux task and each channel's
    /// targetward mux task; borrowed only synchronously, never across an await.
    codec: Rc<CriticalCell<Box<dyn Codec>>>,
    stats: Rc<HashMap<String, Rc<ChannelStat>>>,
    /// Hostward drops the serial reader counted because this codec's multiplexed
    /// side fell behind (its bounded intake was full) — a §5 loss, surfaced so it
    /// stays located and attributable. Claimed from the wiring at start.
    mux_counters: Option<Arc<DropCounters>>,
    tasks: Vec<JoinHandle<()>>,
    status: NodeStatus,
}

impl CodecNode {
    /// Create the node from configuration and a pre-built codec (the registry
    /// validated the name and attributes at instantiate time, §8/§11).
    pub fn create(config: &NodeConfig, codec: Box<dyn Codec>) -> CodecNode {
        let NodeConfig::Codec {
            name,
            codec: codec_name,
            faces,
            channels,
            ..
        } = config
        else {
            unreachable!("CodecNode::create called with non-Codec config");
        };
        let stats = channels
            .iter()
            .map(|c| (c.clone(), Rc::new(ChannelStat::default())))
            .collect();
        CodecNode {
            name: name.clone(),
            codec_name: codec_name.clone(),
            faces: *faces,
            channels: channels.clone(),
            codec: Rc::new(CriticalCell::new(codec)),
            stats: Rc::new(stats),
            mux_counters: None,
            tasks: Vec::new(),
            status: NodeStatus::Active,
        }
    }

    /// Wire and start the demultiplexer's data plane, claiming the codec's own
    /// endpoints out of the (endpoint-keyed) wiring plan.
    pub fn start(&mut self, wiring: &mut Wiring) {
        if self.faces != Facing::Target {
            // Re-multiplexer (faces=host): the mirror data path is driven by a leg
            // (phase 6). Come up faulted so the config loads and the gap shows.
            self.status = NodeStatus::Faulted {
                reason:
                    "re-multiplexer orientation (faces=host) lands in phase 6 with the leg node"
                        .to_owned(),
            };
            return;
        }

        // Multiplexed side (the default endpoint, target-facing): raw hostward in,
        // raw targetward out. Without an attached serial there is no data path.
        let mux = EndpointAddr::node(&self.name);
        let Some(mux_hostward_rx) = wiring.target_hostward_rx.remove(&mux) else {
            self.status = NodeStatus::Waiting {
                reason: "multiplexed side has no attached upstream".to_owned(),
            };
            return;
        };
        let mux_targetward_tx = wiring.target_targetward_tx.remove(&mux);
        let serial_lock = wiring.origin_locks.remove(&mux);
        self.mux_counters = wiring.target_counters.remove(&mux);

        // Per-channel hostward fan-out sinks and targetward receivers.
        let mut channel_sinks: HashMap<String, Vec<HostwardSink>> = HashMap::new();
        let mut channel_rxs: Vec<(String, mpsc::Receiver<Chunk>)> = Vec::new();
        for ch in &self.channels {
            let addr = EndpointAddr::channel(&self.name, ch);
            if let Some(sinks) = wiring.host_sinks.remove(&addr) {
                channel_sinks.insert(ch.clone(), sinks);
            }
            if let Some(rx) = wiring.host_targetward_rx.remove(&addr) {
                channel_rxs.push((ch.clone(), rx));
            }
        }

        // Hostward: demux the multiplexed stream and fan each channel out (§5).
        self.tasks.push(tokio::task::spawn_local(hostward_demux(
            self.codec.clone(),
            mux_hostward_rx,
            channel_sinks,
            self.stats.clone(),
        )));

        // Targetward: one task per channel, framing its writes back into the
        // multiplexed stream — only if the multiplexed side can write to the device
        // (its edge is held/on-demand, giving a targetward sender and a lock).
        if let (Some(mux_tx), Some((serial_lock, mux_id))) = (mux_targetward_tx, serial_lock) {
            for (ch, rx) in channel_rxs {
                let Some(stat) = self.stats.get(&ch).cloned() else {
                    continue;
                };
                self.tasks.push(tokio::task::spawn_local(channel_targetward(
                    ch,
                    rx,
                    mux_tx.clone(),
                    self.codec.clone(),
                    serial_lock.clone(),
                    mux_id,
                    stat,
                )));
            }
        }
    }

    pub fn status(&self) -> NodeStatus {
        self.status.clone()
    }

    pub fn state_extra(&self) -> Value {
        // Codec-specific counters (§7.5). Borrow the transform synchronously — no
        // task holds the borrow across an await, so this never contends.
        // `delivered_hostward` counts channel bytes handed to the consumer boundary
        // (a slow consumer's own drops are counted at that boundary, §5);
        // `accepted_targetward` counts channel bytes handed into the serial's
        // targetward channel (the device-write handoff, not device consumption), and
        // freezes while the demux does not hold the serial lock (§6). `status` is
        // `active` once any data has crossed the channel, else `waiting`.
        let framing_errors = self.codec.with(|c| c.resync_count());
        let channels: serde_json::Map<String, Value> = self
            .channels
            .iter()
            .map(|ch| {
                // `self.stats` is built from `self.channels` in `create`, so every
                // channel has a stat — index directly (no Option handling).
                let stat = &self.stats[ch];
                let obj = json!({
                    "status": if stat.active.get() { "active" } else { "waiting" },
                    "delivered_hostward": stat.delivered_hostward.get(),
                    "discarded_unattached": stat.discarded_unattached.get(),
                    "accepted_targetward": stat.accepted_targetward.get(),
                    "discarded_targetward": stat.discarded_targetward.get(),
                });
                (ch.clone(), obj)
            })
            .collect();
        json!({
            "codec": self.codec_name,
            "faces": self.faces.to_string(),
            "framing_errors": framing_errors,
            // The multiplexed side's own hostward drops (the codec falling behind
            // the serial), so the loss stays located and attributable (§5).
            "multiplexed": {
                "dropped_slow_consumer": self.mux_counters.as_ref().map_or(0, |c| c.dropped_full()),
            },
            "channels": channels,
        })
    }

    pub fn teardown(&mut self) {
        for t in self.tasks.drain(..) {
            t.abort();
        }
    }
}

impl Drop for CodecNode {
    fn drop(&mut self) {
        for t in self.tasks.drain(..) {
            t.abort();
        }
    }
}

/// Hostward demux task: drain the multiplexed stream, decode per-channel events,
/// and fan each channel's data out to its consumers (lossy `try_send` at the
/// consuming boundary, §5). The codec borrow is synchronous and dropped before the
/// fan-out and before the next `recv().await`.
async fn hostward_demux(
    codec: Rc<CriticalCell<Box<dyn Codec>>>,
    mut mux_rx: mpsc::Receiver<Chunk>,
    channel_sinks: HashMap<String, Vec<HostwardSink>>,
    stats: Rc<HashMap<String, Rc<ChannelStat>>>,
) {
    while let Some(chunk) = mux_rx.recv().await {
        let mut events = Vec::new();
        codec.with_mut(|c| {
            if let Err(e) = c.demux(&chunk, &mut |ev| events.push(ev)) {
                tracing::warn!("codec demux error: {e}");
            }
        });
        for ev in events {
            let stat = stats.get(ev.channel.as_str());
            match ev.kind {
                EventKind::Data(bytes) => {
                    let n = bytes.len() as u64;
                    if let Some(s) = stat {
                        s.active.set(true);
                    }
                    // Fan out to this channel's consumers. A configured channel with
                    // no consumer bound discards-with-count (§5); data on an
                    // unconfigured channel (no stat) is noise from the mux and simply
                    // dropped — announced-but-unbound is a phase-6 leg concern.
                    match channel_sinks.get(ev.channel.as_str()) {
                        Some(sinks) => {
                            if let Some(s) = stat {
                                s.delivered_hostward.set(s.delivered_hostward.get() + n);
                            }
                            for (tx, counters) in sinks {
                                match tx.try_send(bytes.clone()) {
                                    Ok(()) => {}
                                    Err(TrySendError::Full(_)) => counters.add_full(n),
                                    Err(TrySendError::Closed(_)) => {}
                                }
                            }
                        }
                        None => {
                            if let Some(s) = stat {
                                s.discarded_unattached.set(s.discarded_unattached.get() + n);
                            }
                        }
                    }
                }
                EventKind::Open => {
                    if let Some(s) = stat {
                        s.active.set(true);
                    }
                }
                EventKind::Close => {
                    if let Some(s) = stat {
                        s.active.set(false);
                    }
                }
                EventKind::Error(msg) => {
                    tracing::debug!(channel = %ev.channel, "codec channel error: {msg}");
                }
            }
        }
    }
}

/// Targetward task for one channel: frame each write into the multiplexed stream
/// and forward it to the device, gated on the codec holding the serial's write
/// lock (§6). A write larger than one frame — an uncapped `send` line or a
/// packet-mode PTY read up to READ_BUF == MAX_FRAME_SIZE, which the channel-id
/// header pushes over the frame bound — is fragmented into consecutive data frames
/// rather than dropped, mirroring the leg and the exec codec (§5 no-drop /
/// all-loss-counted, §15.24). Each framed piece parks here (bounded: one chunk)
/// while the lock is stolen, and is delivered once the codec re-acquires — delayed,
/// never dropped.
async fn channel_targetward(
    channel: String,
    mut rx: mpsc::Receiver<Chunk>,
    mux_tx: mpsc::Sender<Chunk>,
    codec: Rc<CriticalCell<Box<dyn Codec>>>,
    serial_lock: SharedLock,
    mux_id: OriginId,
    stat: Rc<ChannelStat>,
) {
    // Max payload per frame = MAX_FRAME_SIZE minus the envelope header (1 type byte
    // + 2 channel-length bytes + the channel id). `channel` is fixed for this task.
    let cap = MAX_FRAME_SIZE.saturating_sub(3 + channel.len()).max(1);
    while let Some(bytes) = rx.recv().await {
        let total = bytes.len();
        let mut off = 0;
        while off < total {
            let end = (off + cap).min(total);
            let piece_len = (end - off) as u64;
            let mut framed = Vec::new();
            let muxed = codec.with_mut(|c| {
                c.mux(
                    &Event::data(channel.as_str(), bytes.slice(off..end)),
                    &mut framed,
                )
                .is_ok()
            });
            if !muxed {
                // Defensive: each fragment provably fits the frame bound for the
                // envelope codec, so this is unreachable there; a custom transform
                // that still refuses a piece must not drop silently — count the
                // undelivered residual (§5 all-loss-is-counted).
                stat.discarded_targetward
                    .set(stat.discarded_targetward.get() + (total - off) as u64);
                break;
            }
            // Gate on holding the serial's write lock (the codec's held origin). A
            // `send --steal` transiently ousts it; re-acquire FIFO once the stealer
            // releases. The framed piece is parked in `framed` meanwhile.
            if !reacquire_held(&serial_lock, mux_id).await {
                return; // the serial endpoint was torn down
            }
            if mux_tx.send(Chunk::from(framed)).await.is_err() {
                return; // serial gone
            }
            stat.accepted_targetward
                .set(stat.accepted_targetward.get() + piece_len);
            off = end;
        }
    }
}

#[cfg(all(test, feature = "codec-reference"))]
mod tests {
    use super::*;
    use nexus_core::lock::{Arbitration, EndpointLock, WriteMode};
    use tokio::sync::broadcast;

    /// A serial lock whose held demux origin already owns the write lock, so the
    /// codec's `reacquire_held` fast path returns immediately (no parking).
    fn held_lock() -> (SharedLock, OriginId) {
        let id = OriginId(1);
        let mut lock = EndpointLock::new(Arbitration::Exclusive);
        lock.register(id, "demux", WriteMode::Held); // acquires the lock on attach
        let (notifier, _rx) = broadcast::channel(16);
        (
            Rc::new(crate::runtime::LockCell::new("mux", lock, notifier)),
            id,
        )
    }

    /// XC-NODROP-1: a targetward chunk larger than one frame (once the channel-id
    /// header is added) is fragmented into consecutive data frames and reassembled
    /// byte-exact by `demux`, with nothing dropped — the codec mirror of the leg's
    /// no-drop round-trip (§5 all-loss-counted, §15.24).
    #[tokio::test]
    async fn targetward_oversize_chunk_is_fragmented_never_dropped() {
        // A 7-byte channel id: the envelope header pushes a READ_BUF-sized read over
        // MAX_FRAME_SIZE, so a single `mux` would fail — the task must fragment.
        let channel = "console".to_owned();
        let payload: Vec<u8> = (0..100_001u32).map(|i| (i % 251) as u8).collect();

        let (in_tx, in_rx) = mpsc::channel::<Chunk>(4);
        let (mux_tx, mut mux_rx) = mpsc::channel::<Chunk>(64);
        let codec: Rc<CriticalCell<Box<dyn Codec>>> = Rc::new(CriticalCell::new(Box::new(
            codec_reference::ReferenceCodec::new(),
        )));
        let (lock, id) = held_lock();
        let stat = Rc::new(ChannelStat::default());

        in_tx.send(Chunk::from(payload.clone())).await.unwrap();
        drop(in_tx); // close the source so the task drains its one chunk and returns

        channel_targetward(
            channel,
            in_rx,
            mux_tx,
            codec.clone(),
            lock,
            id,
            stat.clone(),
        )
        .await;

        // Every framed piece round-trips through `demux` byte-exact, with no loss.
        let mut reassembled: Vec<u8> = Vec::new();
        let mut frames = 0usize;
        while let Ok(frame) = mux_rx.try_recv() {
            frames += 1;
            codec.with_mut(|c| {
                c.demux(&frame, &mut |ev| {
                    assert_eq!(ev.channel.as_str(), "console");
                    if let EventKind::Data(bytes) = ev.kind {
                        reassembled.extend_from_slice(&bytes);
                    }
                })
                .unwrap();
            });
        }
        assert!(
            frames >= 2,
            "an oversize chunk must span multiple frames (got {frames})"
        );
        assert_eq!(
            reassembled, payload,
            "reassembled targetward bytes must be byte-exact"
        );
        assert_eq!(stat.accepted_targetward.get(), payload.len() as u64);
        assert_eq!(stat.discarded_targetward.get(), 0);
    }
}
