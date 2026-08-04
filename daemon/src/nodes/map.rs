//! Map node (design §7.8, §15.33): the per-console character-mapping transform —
//! picocom's `--imap`/`--omap`, given a home in the graph instead of a flag on
//! every client. The mapping *logic* is the pure, property-tested
//! [`serial_nexus_core::map`]; this module is the running node that wires it into the
//! §5 data plane.
//!
//! **Shape.** One host-facing default endpoint (the *mapped* side, addressed by the
//! bare node name — it carries the standard write-lock / fan-out / tap / replay-ring
//! machinery, so both a raw view on the upstream endpoint and a mapped view here
//! exist by default) and one target-facing `raw` endpoint (the unmapped upstream
//! side, [`serial_nexus_core::config::MAP_RAW_ENDPOINT`]). It slots into the endpoint-keyed
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

use serde_json::{Value, json};
use serial_nexus_core::Chunk;
use serial_nexus_core::config::{MAP_RAW_ENDPOINT, NodeConfig};
use serial_nexus_core::graph::EndpointAddr;
use serial_nexus_core::map::MapDirection;
use serial_nexus_core::state::{NodeState, NodeStatus};

use crate::boundary::TaskSet;
use crate::runtime::{
    DropCounters, EdgeInbox, LossCounter, SharedFanOut, SharedTargetEdge, TargetwardInbox,
    TeardownLoss, Wiring, await_origin, forward_targetward,
};
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
    /// The raw side's live edge binding (§15.35): whether an upstream is attached
    /// and, when it is writable, the targetward sender and lock the map writes
    /// through. `connect`/`disconnect` mutate it under the running pumps.
    raw_edge: Option<SharedTargetEdge>,
    /// Mapped bytes that reached no live consumer of the mapped endpoint — none
    /// bound, or every one cascade-removed. §7.8 gives the map a default replay
    /// ring, so without this counter its bytes would be visible in a tap while
    /// silently absent from the graph: exactly the pairing §5's accounting doctrine
    /// forbids ("a ring can never silently hide loss"). The ring/tap mirror is a spy
    /// *outside* the graph and never suppresses this (DM-3).
    hostward_unattached: Rc<Cell<u64>>,
    /// Targetward bytes discarded because this map has no writable raw edge — the
    /// read-only/display map (`write_mode = "never"` on the raw edge, §7.8) or an
    /// unattached raw side. The receiver is kept alive and drained rather than
    /// dropped, so a writer upstream of the map stays healthy; the bytes are still
    /// loss, so they are counted (§5), mirroring the exec codec's
    /// `mux_discarded_targetward`.
    targetward_discarded: Rc<Cell<u64>>,
    /// Targetward bytes destroyed because this node was torn down while they were
    /// still queued for it (§5, notes §3.31). Distinct from `targetward_discarded`:
    /// that one names bytes this map *decided* to swallow while running, this one
    /// names bytes it never got to look at.
    teardown_loss: TeardownLoss,
    tasks: TaskSet,
    /// The node's observed status *and the moment it entered it* (§7).
    status: NodeState,
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
            raw_edge: None,
            hostward_unattached: Rc::new(Cell::new(0)),
            targetward_discarded: Rc::new(Cell::new(0)),
            teardown_loss: TeardownLoss::default(),
            tasks: TaskSet::default(),
            status: NodeState::new(NodeStatus::Active),
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

        // Hostward source: the raw side's stream of upstream receivers, one per
        // edge over the node's life (§15.35). Without an attached upstream there is
        // no hostward data path (the map waits, reusing faulted-and-wait's state
        // family, §15.8) — but the pump still runs, parked on the inbox, so a later
        // `connect` starts it flowing with no restart.
        let raw_inbox = wiring.target_inbox.remove(&raw);
        self.raw_counters = wiring.target_counters.remove(&raw);
        self.raw_edge = wiring.target_edges.remove(&raw);
        let mapped_fanout = wiring
            .host_fanout
            .remove(&mapped)
            .unwrap_or_else(crate::runtime::FanOutList::new);
        let mapped_feed = wiring.tap_feeds.remove(&mapped);
        if !self.raw_attached() {
            self.status.set(NodeStatus::Waiting {
                reason: "raw side has no attached upstream".to_owned(),
            });
        }
        if let Some(inbox) = raw_inbox {
            self.tasks.push(tokio::task::spawn_local(hostward_map(
                self.hostward.clone(),
                inbox,
                mapped_fanout,
                mapped_feed,
                self.hostward_unattached.clone(),
            )));
        }

        // Targetward: consumer writes arrive at the mapped host endpoint's single
        // arbitrated channel (non-holders self-gate at their own origin, so this
        // channel already carries only the holder's bytes), are mapped, and are
        // forwarded to the upstream via the raw origin — but only while the raw edge
        // can write (held/on-demand fills the origin slot with a targetward sender
        // and a lock handle).
        //
        // One task covers both cases, re-reading the slot per chunk. With no writable
        // raw edge — the read-only/display map (`write_mode = "never"`, §7.8) or an
        // unattached raw side — it drains and counts instead of forwarding. The
        // receiver must NOT be dropped: senders stay live in
        // `GraphState::endpoint_targetward` and in every writer origin attached to the
        // mapped endpoint, and a closed channel makes the next targetward write fail.
        // In the pty case that ends `read_and_poll` outright, taking presence
        // latching, `handle_last_close`, termios reconciliation and detach-release
        // with it — a read-only map silently killing its own writer (MAP-1). Draining
        // makes the map *inert* rather than destructive, and the bytes it swallows are
        // counted (§5).
        if let Some(rx) = wiring.host_targetward_rx.remove(&mapped) {
            let edge = self
                .raw_edge
                .clone()
                .unwrap_or_else(crate::runtime::TargetEdge::new);
            // The node keeps a handle on the queue rather than handing it away
            // outright, so `signal_stop` can count what teardown destroys (§5,
            // notes §3.31).
            self.tasks.push(tokio::task::spawn_local(targetward_map(
                self.targetward.clone(),
                self.teardown_loss.watch(rx),
                edge,
                self.targetward_discarded.clone(),
            )));
        }
    }

    /// Whether the raw side currently has an upstream edge (§15.35).
    fn raw_attached(&self) -> bool {
        self.raw_edge
            .as_ref()
            .is_some_and(|e| e.with(|s| s.attached))
    }

    /// Re-report status after edge surgery on `endpoint` (§15.35). Only the raw
    /// (target-facing) side decides `active`/`waiting`; the mapped side faces host.
    pub fn set_upstream_attached(&mut self, endpoint: &EndpointAddr, attached: bool) {
        if endpoint.node != self.name || endpoint.endpoint != MAP_RAW_ENDPOINT {
            return;
        }
        self.status.set(if attached {
            NodeStatus::Active
        } else {
            NodeStatus::Waiting {
                reason: "raw side has no attached upstream".to_owned(),
            }
        });
    }

    pub fn status(&self) -> NodeState {
        self.status.clone()
    }

    pub fn state_extra(&self) -> Value {
        // Per-direction byte and per-rule substitution counters (§7.8) — the cheap
        // way to discover which quirk a mystery console actually has — plus the two
        // §5 loss counters: mapped bytes that reached no consumer, and targetward
        // bytes a read-only map swallowed. `json!` builds objects, so the inserts
        // below always find one; `as_object_mut` keeps it non-panicking anyway.
        let mut hostward = self.hostward.state();
        if let Some(obj) = hostward.as_object_mut() {
            obj.insert(
                "discarded_unattached".to_owned(),
                json!(self.hostward_unattached.get()),
            );
        }
        let mut targetward = self.targetward.state();
        if let Some(obj) = targetward.as_object_mut() {
            obj.insert(
                "discarded_no_raw_edge".to_owned(),
                json!(self.targetward_discarded.get()),
            );
            obj.insert(
                "discarded_at_teardown".to_owned(),
                json!(self.teardown_loss.bytes()),
            );
        }
        json!({
            "hostward": hostward,
            "targetward": targetward,
            "raw": {
                "dropped_slow_consumer": self.raw_counters.as_ref().map_or(0, |c| c.dropped_full()),
            },
        })
    }

    /// Ask this node's tasks to stop, without waiting (§16.1, BND-1). A map owns no
    /// blocking thread and no environment to release, so signalling *is* its whole
    /// teardown; the method exists so the daemon can signal every node uniformly
    /// before it pays any node's join cost.
    ///
    /// It needs no `impl Drop`: [`TaskSet`] aborts what it holds when the node value
    /// dies, so "a node's tasks die with the node" holds by type rather than by a
    /// hand-written copy of this body (SIMPB-10).
    pub fn signal_stop(&mut self) {
        // Count before aborting, and in that order: `abort_all` is what drops the
        // pump's future, and the queue goes with it. Draining first also stops the
        // pump *gracefully* — an emptied inbox answers `None` — so the abort below
        // is a backstop rather than the mechanism (§5, notes §3.31).
        self.teardown_loss.drain();
        self.tasks.abort_all();
    }

    /// Targetward bytes this node destroyed at teardown, for the verb that removed
    /// it to report (§5: the node is about to stop existing, so `state` cannot be
    /// the only home for its last loss).
    pub fn discarded_at_teardown(&self) -> u64 {
        self.teardown_loss.bytes()
    }

    pub fn teardown(&mut self) {
        self.signal_stop();
    }
}

/// Hostward pump: drain raw bytes from the upstream, apply the hostward mapping, and
/// fan the mapped bytes out to the mapped endpoint's consumers (lossy `try_send` at
/// each consuming boundary, §5), mirroring to the tap hub for taps and the replay
/// ring (§17). A fully-deleted chunk is dropped silently — deletion is the
/// operator's explicit intent, not a loss.
async fn hostward_map(
    dir: Rc<Direction>,
    mut inbox: EdgeInbox,
    sinks: SharedFanOut,
    feed: Option<TapFeed>,
    unattached: Rc<Cell<u64>>,
) {
    while let Some(mut rx) = inbox.recv().await {
        while let Some(chunk) = rx.recv().await {
            let mapped = dir.transform(&chunk);
            if mapped.is_empty() {
                continue;
            }
            let mapped = Chunk::from(mapped);
            // Mirror to the tap hub for taps and the replay ring (§17), independent of
            // whether a graph consumer is bound — a tapped-but-unconsumed map still
            // reaches its observer. Deliberately *outside* the fan-out's accounting: the
            // ring is a spy out of the graph, so it must never suppress the unattached
            // count below (§5, AGENTS.md invariant 9).
            if let Some(feed) = &feed {
                feed.mirror(&mapped);
            }
            // The one shared hostward fan-out (§5, F1). It charges a slow consumer's
            // full-buffer drop to that consumer, and — the case the map used to ignore
            // entirely (DM-3) — an empty or all-`Closed` sink set to `unattached`.
            sinks.broadcast(&mapped, &*unattached);
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
    rx: TargetwardInbox,
    edge: SharedTargetEdge,
    discarded: Rc<Cell<u64>>,
) {
    while let Some(chunk) = rx.recv().await {
        // Re-read the live raw edge per chunk (§15.35), so `connect`/`disconnect`
        // switch this pump between forwarding, draining and parking with no task
        // churn. An *unattached* raw side parks inside `await_origin` — the mapped
        // endpoint's channel then fills and its writers backpressure, which is §5's
        // targetward contract and exactly what a steal does. An attached but
        // read-only one (`write_mode = "never"`) returns `None` and is drained:
        // inert, not destructive, and never wedging a writer on a configuration
        // that will never become writable (MAP-1).
        let cost = chunk.len() as u64;
        let Some(origin) = await_origin(&edge).await else {
            // Count the *arriving* bytes; the transform is deliberately not run, so
            // a read-only map's per-rule counters do not claim substitutions on
            // bytes that never left the node.
            discarded.add(cost);
            continue;
        };
        // The shared send-and-charge step (SIMP-2). It gates on holding the
        // upstream's write lock (the map's held origin) *before* running the
        // transform, which is this node's ordering requirement: the per-rule
        // substitution counters are bumped inside `transform`, so running it on a
        // chunk that then turns out to have nowhere to go would have the map claim
        // substitutions on bytes that never left the node — the same reason the
        // no-origin arm above counts without transforming. Both of its loss exits
        // charge the *arriving* byte count for that same reason. A fully deleted
        // chunk yields `None`: nothing to write and nothing lost. And every failure
        // keeps draining rather than returning — the receiver's senders are still
        // live, and a `connect` may yet give this map a new upstream.
        forward_targetward(&origin, cost, || {
            let mapped = dir.transform(&chunk);
            (!mapped.is_empty()).then(|| Chunk::from(mapped))
        })
        .await
        .charge(&*discarded);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::DropCounters;
    use tokio::sync::mpsc;

    /// Wrap one receiver as an [`EdgeInbox`] carrying exactly that edge, then
    /// closed — so a pump under test drains it and returns rather than parking.
    fn one_edge(rx: mpsc::Receiver<Chunk>) -> EdgeInbox {
        let (tx, inbox) = mpsc::channel(1);
        tx.try_send(rx).expect("fresh inbox");
        inbox
    }

    fn identity_direction() -> Rc<Direction> {
        // No rules: a pure pass-through, so the tests assert on the fan-out and the
        // drain rather than on the (separately property-tested) transform.
        let map = MapDirection::parse(&[]).expect("empty mapping list parses");
        Rc::new(Direction {
            stat: DirStat::new(map.rule_count()),
            map,
        })
    }

    /// DM-3/MAP-UNATTACHED-LOSS: the map is a hostward producer like any other, so
    /// mapped bytes that reach no live consumer must be counted, not swallowed.
    /// §7.8 gives it a default replay ring, so an uncounted drop would be visible in
    /// a tap while silently absent from the graph (§5).
    #[tokio::test]
    async fn hostward_map_counts_bytes_that_reach_no_consumer() {
        // Two shapes of "no consumer": no sinks at all, and one permanently Closed.
        for closed_sink in [false, true] {
            let (in_tx, in_rx) = mpsc::channel::<Chunk>(4);
            let sinks = crate::runtime::FanOutList::new();
            if closed_sink {
                let (tx, rx) = mpsc::channel::<Chunk>(4);
                drop(rx); // consumer cascade-removed: the sink is permanently Closed
                sinks.attach(crate::runtime::AttachedSink {
                    target: "consumer".parse().expect("address parses"),
                    tx,
                    counters: Arc::new(DropCounters::default()),
                });
            }
            let unattached = Rc::new(Cell::new(0u64));

            in_tx
                .send(Chunk::copy_from_slice(b"hello"))
                .await
                .expect("queued");
            drop(in_tx); // close the source so the pump drains and returns

            hostward_map(
                identity_direction(),
                one_edge(in_rx),
                sinks,
                None,
                unattached.clone(),
            )
            .await;

            assert_eq!(
                unattached.get(),
                5,
                "closed_sink = {closed_sink}: unconsumed mapped bytes must be counted"
            );
        }
    }

    #[tokio::test]
    async fn hostward_map_does_not_count_bytes_a_live_consumer_took() {
        let (in_tx, in_rx) = mpsc::channel::<Chunk>(4);
        let (tx, mut rx) = mpsc::channel::<Chunk>(4);
        let sinks = crate::runtime::FanOutList::new();
        sinks.attach(crate::runtime::AttachedSink {
            target: "consumer".parse().expect("address parses"),
            tx,
            counters: Arc::new(DropCounters::default()),
        });
        let unattached = Rc::new(Cell::new(0u64));

        in_tx
            .send(Chunk::copy_from_slice(b"hello"))
            .await
            .expect("queued");
        drop(in_tx);

        hostward_map(
            identity_direction(),
            one_edge(in_rx),
            sinks,
            None,
            unattached.clone(),
        )
        .await;

        assert_eq!(unattached.get(), 0);
        let got = rx.try_recv().expect("the live consumer received the chunk");
        assert_eq!(&got[..], b"hello");
    }

    /// MAP-1 (runtime): a read-only map must keep its mapped endpoint's targetward
    /// receiver alive. The regression is silent and destructive — a dropped receiver
    /// closes the channel under every writer origin, and a pty writer's task dies
    /// with it — so assert the observable half: the sender stays usable and the
    /// bytes are counted as loss (§5).
    #[test]
    fn targetward_drain_keeps_the_channel_open_and_counts_the_loss() {
        // A current-thread runtime + LocalSet mirrors the daemon: the drain runs as
        // a spawned local task while a "writer" sends into the same channel.
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async {
            let (tx, rx) = mpsc::channel::<Chunk>(4);
            let discarded = Rc::new(Cell::new(0u64));
            // A read-only map: the raw edge is attached with `write_mode = "never"`,
            // so the pump drains and counts. (An unattached raw side parks instead —
            // §5 forbids dropping targetward, and a detached edge must stall its
            // writers exactly as a steal does.)
            let handle = tokio::task::spawn_local(targetward_map(
                identity_direction(),
                TargetwardInbox::new(rx),
                crate::runtime::TargetEdge::read_only(),
                discarded.clone(),
            ));
            tx.send(Chunk::copy_from_slice(b"abc"))
                .await
                .expect("a live drain keeps the channel open");
            tx.send(Chunk::copy_from_slice(b"de"))
                .await
                .expect("still open on the second write");
            drop(tx);
            handle.await.expect("the drain task ends with its source");
            assert_eq!(discarded.get(), 5);
        });
    }
}
