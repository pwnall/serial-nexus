//! Map node (design §7.8, §15.33): the per-console character-mapping transform —
//! picocom's `--imap`/`--omap`, given a home in the graph instead of a flag on
//! every client. The mapping *logic* is the pure, property-tested
//! [`nexus_core::map`]; this module is the running node that wires it into the
//! §5 data plane.
//!
//! **Shape.** One host-facing default endpoint (the *mapped* side, addressed by the
//! bare node name — it carries the standard write-lock / fan-out / tap / replay-ring
//! machinery, so both a raw view on the upstream endpoint and a mapped view here
//! exist by default) and one target-facing `raw` endpoint (the unmapped upstream
//! side, [`nexus_core::config::MAP_RAW_ENDPOINT`]). It slots into the endpoint-keyed
//! wiring (§15.23) with no new machinery — the first *non-codec* interior transform.
//!
//! **Interior contract (§5).** The map holds no queues and no parser state: each
//! direction is a stateless byte-to-byte-sequence substitution, so chunk boundaries
//! are irrelevant by construction and the output is bounded at `k ×` input (§7.8),
//! keeping the interior memory bound intact. It runs on the async runtime; the
//! synchronous transform executes in the task's context, and the bounded mpsc
//! channels to its neighbours are *their* boundary buffers, not the map's.
//!
//! **The held edge (§6).** The map's edge into the upstream endpoint is normally
//! `held` — the demux's pattern with softer stakes (bypassing a map is merely
//! unmapped, not corrupt). A `send --steal` at the upstream ousts the map
//! transiently; its targetward task parks its mapped chunk and re-acquires (FIFO,
//! held priority) once the stealer releases — delayed, never dropped.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use nexus_core::Chunk;
use nexus_core::NodeStatus;
use nexus_core::config::{MAP_RAW_ENDPOINT, NodeConfig};
use nexus_core::graph::EndpointAddr;
use nexus_core::lock::OriginId;
use nexus_core::map::MapDirection;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::task::JoinHandle;

use crate::runtime::{DropCounters, HostwardSink, SharedLock, Wiring, reacquire_held};
use crate::tap::TapFeed;

/// Per-direction observed counters (§7.8). All access is on the one runtime thread
/// (each direction's task, plus `state_extra`), so `Cell` suffices. `rule_counts`
/// has one entry per configured rule, in list order — the per-rule substitution
/// tallies the design exposes so an operator can discover which quirk a mystery
/// console actually has.
struct DirStat {
    bytes_in: Cell<u64>,
    bytes_out: Cell<u64>,
    rule_counts: Vec<Cell<u64>>,
}

impl DirStat {
    fn new(rules: usize) -> DirStat {
        DirStat {
            bytes_in: Cell::new(0),
            bytes_out: Cell::new(0),
            rule_counts: (0..rules).map(|_| Cell::new(0)).collect(),
        }
    }
}

/// One compiled direction plus its live counters — the unit each pump task owns.
struct Direction {
    map: MapDirection,
    stat: DirStat,
}

impl Direction {
    /// Apply the transform to `input`, returning the mapped bytes and updating this
    /// direction's byte + per-rule counters (§7.8). A fully-deleted chunk yields an
    /// empty `Vec`.
    fn transform(&self, input: &[u8]) -> Vec<u8> {
        // Bounded at k× input (§7.8), so one allocation of the worst case avoids
        // reallocation on the hot path without risking unboundedness.
        let mut out = Vec::with_capacity(input.len() * self.map.max_expansion());
        self.map.apply(input, &mut out, |rule| {
            let c = &self.stat.rule_counts[rule];
            c.set(c.get() + 1);
        });
        self.stat
            .bytes_in
            .set(self.stat.bytes_in.get() + input.len() as u64);
        self.stat
            .bytes_out
            .set(self.stat.bytes_out.get() + out.len() as u64);
        out
    }

    /// This direction's state object: byte totals plus a per-rule-name substitution
    /// map. A repeated rule name (degenerate — the shadowed copy never fires) sums,
    /// which is exact since the shadowed count is always zero.
    fn state(&self) -> Value {
        let mut rules = serde_json::Map::new();
        for (i, count) in self.stat.rule_counts.iter().enumerate() {
            let name = self.map.rule_name(i);
            let entry = rules.entry(name.to_owned()).or_insert(json!(0));
            let prev = entry.as_u64().unwrap_or(0);
            *entry = json!(prev + count.get());
        }
        json!({
            "bytes_in": self.stat.bytes_in.get(),
            "bytes_out": self.stat.bytes_out.get(),
            "rules": rules,
        })
    }
}

pub struct MapNode {
    pub name: String,
    /// Hostward (device → consumers, picocom `--imap`) and targetward (consumers →
    /// device, `--omap`) transforms, each owned by its pump task after `start`.
    /// Shared by `Rc` so `state_extra` can read the counters on the runtime thread.
    hostward: Rc<Direction>,
    targetward: Rc<Direction>,
    /// Hostward drops the upstream producer counted because this map's raw-side
    /// intake was full — a §5 loss surfaced so it stays located and attributable
    /// (the map's mirror of the codec's multiplexed-side drop count). Claimed from
    /// the wiring at start.
    raw_counters: Option<Arc<DropCounters>>,
    tasks: Vec<JoinHandle<()>>,
    status: NodeStatus,
}

impl MapNode {
    /// Create the node from configuration, compiling both mapping directions. The
    /// mapping names were already validated structurally (`GraphConfig::validate`,
    /// §7.8) before any teardown, so a parse failure here is unreachable in practice;
    /// it is still surfaced as a structural `Err` (never a panic), belt-and-suspenders.
    pub fn create(config: &NodeConfig) -> Result<MapNode, String> {
        let NodeConfig::Map {
            name,
            hostward,
            targetward,
            ..
        } = config
        else {
            unreachable!("MapNode::create called with non-Map config");
        };
        let hostward_dir = MapDirection::parse(hostward)
            .map_err(|bad| format!("unknown hostward mapping {bad:?} (§7.8)"))?;
        let targetward_dir = MapDirection::parse(targetward)
            .map_err(|bad| format!("unknown targetward mapping {bad:?} (§7.8)"))?;
        let hostward = Rc::new(Direction {
            stat: DirStat::new(hostward_dir.rule_count()),
            map: hostward_dir,
        });
        let targetward = Rc::new(Direction {
            stat: DirStat::new(targetward_dir.rule_count()),
            map: targetward_dir,
        });
        Ok(MapNode {
            name: name.clone(),
            hostward,
            targetward,
            raw_counters: None,
            tasks: Vec::new(),
            status: NodeStatus::Active,
        })
    }

    /// Wire and start the map's data plane, claiming its own endpoints out of the
    /// endpoint-keyed wiring plan (§15.23). Hostward: raw bytes arrive at the `raw`
    /// target endpoint, are mapped, and fan out at the mapped host endpoint (with a
    /// tap/ring mirror). Targetward: consumer bytes arrive at the mapped host
    /// endpoint, are mapped, and are written to the upstream via the `raw` origin's
    /// held lock.
    pub fn start(&mut self, wiring: &mut Wiring) {
        let mapped = EndpointAddr::node(&self.name);
        let raw = EndpointAddr::channel(&self.name, MAP_RAW_ENDPOINT);

        // Hostward source: the raw side's receiver from its upstream host endpoint.
        // Without an attached upstream there is no hostward data path (the map waits,
        // reusing faulted-and-wait's state family, §15.8).
        let raw_hostward_rx = wiring.target_hostward_rx.remove(&raw);
        self.raw_counters = wiring.target_counters.remove(&raw);
        let mapped_sinks = wiring.host_sinks.remove(&mapped).unwrap_or_default();
        let mapped_feed = wiring.tap_feeds.remove(&mapped);

        match raw_hostward_rx {
            Some(rx) => self.tasks.push(tokio::task::spawn_local(hostward_map(
                self.hostward.clone(),
                rx,
                mapped_sinks,
                mapped_feed,
            ))),
            None => {
                self.status = NodeStatus::Waiting {
                    reason: "raw side has no attached upstream".to_owned(),
                };
            }
        }

        // Targetward: consumer writes arrive at the mapped host endpoint's single
        // arbitrated channel (non-holders self-gate at their own origin, so this
        // channel already carries only the holder's bytes), are mapped, and are
        // forwarded to the upstream via the raw origin — but only if the raw edge can
        // write (held/on-demand gives a targetward sender and a lock handle).
        let mapped_targetward_rx = wiring.host_targetward_rx.remove(&mapped);
        let raw_targetward_tx = wiring.target_targetward_tx.remove(&raw);
        let raw_lock = wiring.origin_locks.remove(&raw);
        if let (Some(rx), Some(up_tx), Some((lock, id))) =
            (mapped_targetward_rx, raw_targetward_tx, raw_lock)
        {
            self.tasks.push(tokio::task::spawn_local(targetward_map(
                self.targetward.clone(),
                rx,
                up_tx,
                lock,
                id,
            )));
        }
    }

    pub fn status(&self) -> NodeStatus {
        self.status.clone()
    }

    pub fn state_extra(&self) -> Value {
        // Per-direction byte and per-rule substitution counters (§7.8) — the cheap
        // way to discover which quirk a mystery console actually has — plus the
        // raw-side intake drop count (the map falling behind the upstream, §5).
        json!({
            "hostward": self.hostward.state(),
            "targetward": self.targetward.state(),
            "raw": {
                "dropped_slow_consumer": self.raw_counters.as_ref().map_or(0, |c| c.dropped_full()),
            },
        })
    }

    pub fn teardown(&mut self) {
        for t in self.tasks.drain(..) {
            t.abort();
        }
    }
}

impl Drop for MapNode {
    fn drop(&mut self) {
        for t in self.tasks.drain(..) {
            t.abort();
        }
    }
}

/// Hostward pump: drain raw bytes from the upstream, apply the hostward mapping, and
/// fan the mapped bytes out to the mapped endpoint's consumers (lossy `try_send` at
/// each consuming boundary, §5), mirroring to the tap hub for taps and the replay
/// ring (§17). A fully-deleted chunk is dropped silently — deletion is the
/// operator's explicit intent, not a loss.
async fn hostward_map(
    dir: Rc<Direction>,
    mut rx: mpsc::Receiver<Chunk>,
    sinks: Vec<HostwardSink>,
    feed: Option<TapFeed>,
) {
    while let Some(chunk) = rx.recv().await {
        let mapped = dir.transform(&chunk);
        if mapped.is_empty() {
            continue;
        }
        let mapped = Chunk::from(mapped);
        // Mirror to the tap hub for taps and the replay ring (§17), independent of
        // whether a graph consumer is bound — a tapped-but-unconsumed map still
        // reaches its observer.
        if let Some(feed) = &feed {
            feed.mirror(&mapped);
        }
        let n = mapped.len() as u64;
        for (tx, counters) in &sinks {
            match tx.try_send(mapped.clone()) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => counters.add_full(n),
                Err(TrySendError::Closed(_)) => {}
            }
        }
    }
}

/// Targetward pump: drain consumer writes at the mapped endpoint, apply the
/// targetward mapping, and forward the mapped bytes to the upstream — gated on the
/// raw origin holding the upstream's write lock (§6). A `send --steal` at the
/// upstream transiently ousts the map; the mapped chunk parks here (bounded: one
/// chunk) and is delivered once the map re-acquires (FIFO, held priority) — delayed,
/// never dropped. No framing, so no fragmentation: the upstream endpoint's own
/// boundary owns any framing (a codec channel) or writes raw (a serial).
async fn targetward_map(
    dir: Rc<Direction>,
    mut rx: mpsc::Receiver<Chunk>,
    up_tx: mpsc::Sender<Chunk>,
    lock: SharedLock,
    id: OriginId,
) {
    while let Some(chunk) = rx.recv().await {
        let mapped = dir.transform(&chunk);
        if mapped.is_empty() {
            continue; // fully deleted targetward: nothing to write
        }
        // Gate on holding the upstream's write lock (the map's held origin). Park the
        // mapped chunk while a stealer holds it; re-acquire FIFO once released.
        if !reacquire_held(&lock, id).await {
            return; // the upstream endpoint was torn down
        }
        if up_tx.send(Chunk::from(mapped)).await.is_err() {
            return; // upstream gone
        }
    }
}
