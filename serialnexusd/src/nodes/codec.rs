//! Codec node (design §7.5): the interior demux/remux protocol transform, and
//! the compiled-in codec registry (§8).
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

use codec_api::{Codec, Event, EventKind};
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
use crate::runtime::{DropCounters, HostwardSink, SharedLock, Wiring};

/// Instantiate a compiled-in codec by registry name (§8 match-on-name — no
/// linker-magic auto-registration). Attribute-schema validation is the codec's
/// own, and a failure here is structural: it aborts the load, nothing created
/// (§8, §11). The reference framing codec takes no attributes. The exec codec
/// (§7.6) is not a [`Codec`] transform — it is a child process — so it is hosted
/// separately (phase 5 slice C), not built here.
pub fn build_codec(name: &str, attributes: &toml::Table) -> Result<Box<dyn Codec>, String> {
    match name {
        #[cfg(feature = "codec-reference")]
        "reference" => {
            if !attributes.is_empty() {
                let keys: Vec<&String> = attributes.keys().collect();
                return Err(format!(
                    "codec \"reference\" takes no attributes; got {keys:?}"
                ));
            }
            Ok(Box::new(codec_reference::ReferenceCodec::new()))
        }
        // "exec" is not an in-process transform (it is a child process); it is
        // routed to the exec node at instantiate and never reaches here.
        other => Err(format!("unknown codec {other:?}")),
    }
}

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
                let stat = self.stats.get(ch);
                let obj = json!({
                    "status": if stat.is_some_and(|s| s.active.get()) { "active" } else { "waiting" },
                    "delivered_hostward": stat.map_or(0, |s| s.delivered_hostward.get()),
                    "discarded_unattached": stat.map_or(0, |s| s.discarded_unattached.get()),
                    "accepted_targetward": stat.map_or(0, |s| s.accepted_targetward.get()),
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
/// lock (§6). The framed chunk parks here (bounded: one chunk) while the lock is
/// stolen, and is delivered once the codec re-acquires — delayed, never dropped.
async fn channel_targetward(
    channel: String,
    mut rx: mpsc::Receiver<Chunk>,
    mux_tx: mpsc::Sender<Chunk>,
    codec: Rc<CriticalCell<Box<dyn Codec>>>,
    serial_lock: SharedLock,
    mux_id: OriginId,
    stat: Rc<ChannelStat>,
) {
    while let Some(bytes) = rx.recv().await {
        let n = bytes.len() as u64;
        let mut framed = Vec::new();
        let muxed = codec.with_mut(|c| {
            c.mux(&Event::data(channel.as_str(), bytes), &mut framed)
                .is_ok()
        });
        if !muxed {
            continue; // a mux error drops this chunk (unreachable for reference)
        }
        // Gate on holding the serial's write lock (the codec's held origin). A
        // `send --steal` transiently ousts it; re-acquire FIFO once the stealer
        // releases. The framed chunk is parked in `framed` meanwhile.
        if !ensure_holds(&serial_lock, mux_id).await {
            return; // the serial endpoint was torn down
        }
        if mux_tx.send(Chunk::from(framed)).await.is_err() {
            return; // serial gone
        }
        stat.accepted_targetward
            .set(stat.accepted_targetward.get() + n);
    }
}

/// Ensure the codec holds `id`'s serial write lock, re-acquiring through the FIFO
/// queue if a steal ousted it (§6). Returns `false` if the endpoint was torn down.
/// The fast path (the normal held case) is a single borrow; the slow path parks on
/// the lock's `Notify`, holding no borrow across the await (§15.20).
async fn ensure_holds(lock: &SharedLock, id: OriginId) -> bool {
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
