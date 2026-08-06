//! Leg node (design §7.4): the cross-daemon transport. A socket (tcp|unix)
//! carrying all of its channels multiplexed by the built-in **link codec** — the
//! shared envelope frame format (`serial-nexus-codec-api`), opened by a `hello` frame (§9).
//!
//! **Orientation.** All of a leg's endpoints are its channels (there is no
//! multiplexed-side default endpoint — the socket is off-graph). `faces = target`
//! (computer A, the sending side) consumes local channels: it forwards their
//! hostward device data onto the wire and writes wire-arriving commands targetward
//! into the local graph. `faces = host` (computer B, the receiving side) offers
//! arriving channels: it fans wire-arriving device data out to local consumers and
//! forwards their targetward commands onto the wire. Per leg, one socket direction
//! is purely hostward, the other purely targetward.
//!
//! **The wire (§9).** On every (re)connect both peers send a `hello` (magic,
//! version, capabilities, channel announcements), then read the peer's. A version
//! mismatch or a malformed frame is refused cleanly, faulting the leg with the
//! reason in state (§9 clause 6). Over the reliable transport the link codec never
//! resyncs — a decode error is a protocol violation, handled like the exec child's
//! malformed frame (§7.6): tear the connection down and reconnect.
//!
//! **Binding (§8).** Announcements never grow the graph. A configured channel the
//! peer announces is `bound`; configured-but-unannounced is `waiting`
//! (faulted-and-wait — its targetward writers backpressure, their bytes never sent);
//! announced-but-unconfigured is `unbound` — visible state only, its arriving bytes
//! dropped (a configured-but-unattached channel instead counts the drop, §5). All
//! three are leg-internal state in `state_extra`, never a graph or wiring mutation.
//!
//! **Lifecycle (§7.4).** One active peer per leg; the listen role rejects a
//! concurrent second connection. An outage is faulted-and-wait: the connect role
//! retries with backoff; while disconnected the leg parks its wiring channels, so
//! targetward writers backpressure and hostward data drops-and-counts at the
//! existing boundaries (§5). On reconnect, purge-on-reconnect (default on) discards
//! the outage-era targetward backlog with a counter, so stale commands never fire
//! into a device that rebooted (§6).
//!
//! **Concurrency.** Like the exec codec (§15.22), the leg's socket read and write
//! halves run as **concurrently-polled** futures via the boundary-supervisor
//! library's [`boundary::race3`] (§16.1), so a backpressured targetward write never
//! starves the hostward read half. Reconnect backoff is [`boundary::Backoff`], and
//! an exhausted send half [`boundary::park`]s rather than tearing the wire down (the
//! §15.24 stale-status fix, now structural). Every task is aborted on teardown and
//! Drop; a lock borrow never crosses an `.await` — now a compile-shape fact via
//! [`CriticalCell`] rather than a review rule (§15.20, §16.2).
//!
//! **The listen role's filesystem artifact.** A `listen`+`unix` leg narrows its
//! socket to **0600** ([`bind_listener`]): §9's posture is that SSH supplies
//! confidentiality and authentication *between machines*, which says nothing about
//! the local users who can reach the path — and the v1 wire has no authentication
//! of its own, so a bearer of that path writes into every console bound to the leg.
//! That is precisely the trust set the control socket's 0600 mode governs (§10), and
//! §15.29 already settled the principle for the web console: a bearer of a local
//! channel is not the same trust set as a 0600 socket. The address is also treated
//! as an operator file until proven otherwise — a non-socket is never unlinked, a
//! socket with a live peer is never stolen, and teardown removes only the inode this
//! node actually created.

use std::borrow::Cow;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use serde_json::{Value, json};
use serial_nexus_codec_api::{
    Event, EventKind, FrameDecoder, Hello, WIRE_VERSION, encode_hello, try_decode_hello,
};
use serial_nexus_core::config::{LegRole, NodeConfig, Transport};
use serial_nexus_core::graph::{EndpointAddr, Facing};
use serial_nexus_core::lock::{Acquire, OriginId};
use serial_nexus_core::{Chunk, NodeState, NodeStatus};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UnixListener, UnixStream};
use tokio::sync::Notify;
use tokio::sync::mpsc;

use crate::boundary::{self, TaskSet};
use crate::cell::CriticalCell;
use crate::runtime::{
    CHANNEL_CAP, DataFrame, DropCounters, Grant, HostwardChannelStat, LossCounter, READ_BUF,
    SharedFanOut, SharedLock, SharedTargetEdge, TargetwardInbox, TeardownBytes, TeardownLoss,
    Wiring, await_write_grant, data_frames, route_channel_data,
};
use crate::tap::TapFeed;

/// How long to wait for the peer's hello before treating the connection as dead.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a pre-bind dial at a `listen`+`unix` leg's address may take before the
/// existing socket is treated as live. A socket nobody listens on refuses
/// *immediately* (ECONNREFUSED), so anything slower is somebody answering — and the
/// safe reading of an ambiguous answer is "not mine to unlink" (SEC-8).
const PEER_PROBE_TIMEOUT: Duration = Duration::from_millis(250);

/// How many peer-announced-but-unconfigured identities one leg remembers (§8).
/// The list exists to prompt an operator ("the peer offers `console-c`, you have not
/// configured it"), and a few hundred is far past the point where a human reads it —
/// but a peer streaming data frames with fresh channel ids would otherwise grow it
/// without limit, on the single runtime thread (LEG-2). Hostile peers are in scope
/// (`p6_hostility`), and a `listen`+`unix` leg is dialable by anyone who can reach
/// its path.
const MAX_UNBOUND: usize = 256;

/// How much of a peer-supplied identity is remembered. Channel identities an
/// operator writes are short; the wire admits far longer ones, and `state` needs
/// only enough to recognize what the peer offered (LEG-2).
const MAX_UNBOUND_ID_LEN: usize = 64;

/// Appended to an identity [`MAX_UNBOUND_ID_LEN`] truncated, so `state` never
/// implies the peer sent the shorter name.
const TRUNCATION_MARKER: &str = "…(truncated)";

/// A boxed duplex byte stream, abstracting over tcp and unix sockets so the pump
/// is transport-agnostic. Tasks run on the single-threaded `LocalSet`, so no
/// `Send` bound is needed.
trait DuplexStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin> DuplexStream for T {}

/// A bound listener for the `listen` role.
#[derive(Debug)]
enum Listener {
    Tcp(TcpListener),
    Unix(UnixListener),
}

impl Listener {
    async fn accept(&self) -> std::io::Result<(Box<dyn DuplexStream>, String)> {
        match self {
            Listener::Tcp(l) => {
                let (s, addr) = l.accept().await?;
                let _ = s.set_nodelay(true);
                Ok((Box::new(s), addr.to_string()))
            }
            Listener::Unix(l) => {
                let (s, _) = l.accept().await?;
                Ok((Box::new(s), "unix".to_owned()))
            }
        }
    }
}

/// Per-channel observed counters and binding (§7.4). Single runtime thread, so
/// `Cell` suffices.
#[derive(Default)]
struct ChannelStat {
    /// Hostward bytes this leg forwarded (faces=host: to local consumers;
    /// faces=target: onto the wire).
    delivered_hostward: Cell<u64>,
    /// Targetward bytes this leg forwarded (faces=host: onto the wire;
    /// faces=target: into the local graph, once the device-write handoff accepts).
    accepted_targetward: Cell<u64>,
    /// Hostward bytes dropped at this leg because a local consumer's buffer was
    /// full, or because a configured channel has no consumer bound at all
    /// (faces=host) — a §5 loss counted where it happens.
    discarded_hostward: Cell<u64>,
    /// Targetward bytes dropped at this leg because wire data arrived for a
    /// configured channel with no writable local edge behind it, or whose local
    /// writer task is gone (faces=target). Charging these to `discarded_hostward`
    /// was LEG-4: the loss is real, the direction was wrong.
    discarded_targetward: Cell<u64>,
    /// Bytes of a chunk the wire framer refused to encode. Defensively unreachable
    /// for the fixed envelope — every fragment provably fits [`data_frames`]' bound
    /// — but §5/invariant 3's shape is "fragment, never skip-on-error, **count any
    /// residual**", and `data_frames` stops fragmenting on an encode error, so an
    /// uncounted short framing would be a silent truncation (RV-9). The in-process
    /// codec counts the same case as `discarded_targetward`; the leg's send half
    /// serves whichever direction the facing implies, so it gets its own name.
    discarded_unframable: Cell<u64>,
    /// Bytes of a chunk the socket write half had already taken from its bounded
    /// receiver when the peer went away, and never put on the wire (LEG-2). Its own
    /// name rather than a fold into `discarded_unframable`: that counter means "this
    /// channel identity is pathological", a defensively-unreachable case, and firing
    /// it on every ordinary disconnect would destroy the one signal it carries. §5
    /// wants loss attributable, which means one cause per counter.
    discarded_peer_gone: Cell<u64>,
    /// Targetward bytes discarded on reconnect because they were outage-era stale
    /// (§7.4 purge-on-reconnect). Counted on both sides of the wire: the sending
    /// side's local backlog, and — since LEG-3 — the receiving side's too.
    purged_on_reconnect: Cell<u64>,
    /// Whether the peer announced this configured channel (`bound`), else `waiting`.
    bound: Cell<bool>,
    /// Whether any data has crossed the channel since connect.
    active: Cell<bool>,
}

/// The §5 hostward accounting rule, shared with both codecs through the one
/// [`route_channel_data`] implementation (SIMP-1).
///
/// The leg is the *differently specified* instance of that rule, not a drifted copy
/// of the codecs', and this impl is where the difference now lives in one visible
/// place rather than being inferable only by diffing three routing blocks:
/// `discarded_hostward` is documented above as covering **both** the full-buffer drop
/// and the no-consumer-bound case, so [`Self::add_dropped_full`] folds the sinks'
/// full-buffer loss in where the codecs' narrower `discarded_unattached` leaves it to
/// the consuming boundary's own `DropCounters`.
impl HostwardChannelStat for ChannelStat {
    fn set_active(&self) {
        self.active.set(true);
    }

    fn add_delivered(&self, n: u64) {
        self.delivered_hostward
            .set(self.delivered_hostward.get() + n);
    }

    fn unattached(&self) -> &dyn LossCounter {
        &self.discarded_hostward
    }

    fn add_dropped_full(&self, n: u64) {
        self.discarded_hostward.add(n);
    }
}

/// Peer-announced identities this configuration does not declare — visible state
/// awaiting an operator, never an endpoint (§8). Bounded in both count and
/// per-identity length: a peer streaming data frames with fresh channel ids would
/// otherwise grow an uncapped `Vec` without limit and turn its linear dedup scan
/// into O(n²) work on the single runtime thread (LEG-2). Insertion order is what
/// `state` reports, so two snapshots diff meaningfully; the parallel set makes the
/// per-frame membership test O(1) rather than a scan of the cap.
#[derive(Default)]
struct UnboundSet {
    order: Vec<String>,
    seen: HashSet<String>,
    /// Occurrences the cap refused to record — *not* distinct identities, which
    /// cannot be known without remembering them, which is the thing being bounded.
    /// A repeat of an already-recorded identity is not an overflow.
    overflow: u64,
}

impl UnboundSet {
    /// Record a peer-supplied identity, truncated and capped.
    fn insert(&mut self, id: &str) {
        let id = truncate_identity(id);
        if self.seen.contains(id.as_ref()) {
            return;
        }
        if self.order.len() >= MAX_UNBOUND {
            self.overflow += 1;
            return;
        }
        let id = id.into_owned();
        self.seen.insert(id.clone());
        self.order.push(id);
    }

    /// Forget everything: binding state is per-connection (§8), so it is cleared at
    /// hello reconciliation and when a connection drops — the overflow count with
    /// it, since it describes that connection's peer.
    fn clear(&mut self) {
        self.order.clear();
        self.seen.clear();
        self.overflow = 0;
    }
}

/// Bound the stored length of a peer-supplied identity, marking a truncation
/// explicitly (LEG-2). Truncation lands on a `char` boundary: the identity is
/// peer-supplied UTF-8 and a split code point would not survive JSON.
fn truncate_identity(id: &str) -> Cow<'_, str> {
    if id.len() <= MAX_UNBOUND_ID_LEN {
        return Cow::Borrowed(id);
    }
    let mut end = MAX_UNBOUND_ID_LEN;
    while end > 0 && !id.is_char_boundary(end) {
        end -= 1;
    }
    Cow::Owned(format!("{}{TRUNCATION_MARKER}", &id[..end]))
}

/// Node-level observed state shared with the supervisor task (which flips it as
/// the connection comes and goes).
struct LegShared {
    status: CriticalCell<NodeState>,
    peer_address: CriticalCell<Option<String>>,
    peer_version: Cell<Option<u16>>,
    peer_capabilities: Cell<u32>,
    reconnect_count: Cell<u64>,
    /// Peer-announced-but-unconfigured identities (§8), bounded per LEG-2.
    unbound: CriticalCell<UnboundSet>,
    /// The unix socket path the `listen` role actually bound, so teardown unlinks
    /// its own artifact and never an operator file this node merely found (SEC-8).
    /// `None` for tcp, for the connect role, and before a successful bind.
    bound_unix_path: CriticalCell<Option<String>>,
    /// §7.4 purge-on-reconnect. Read by the supervisor (the sending side's local
    /// backlog) *and* by every faces=target channel task (the receiving side's,
    /// LEG-3), which is what makes it shared rather than a supervisor argument.
    purge_on_reconnect: bool,
    /// Pulsed by the supervisor when a connection drops, so each faces=target
    /// channel task promptly releases its on-demand write lock (§7.1: release on
    /// idle *or* peer disconnect), rather than holding the local floor until idle.
    disconnect: Notify,
    /// Incremented alongside every `disconnect` pulse. Because `notify_waiters`
    /// stores no permit, a channel task blocked in a backpressured local write
    /// misses the pulse; it compares this counter against the one its chunk was
    /// *enqueued* under, releasing the lock promptly when it changed (§7.1, LEG-4)
    /// and purging the chunk when the connection that delivered it is the one that
    /// went away (§6, 37-LEG-1 — see [`Inbound`]).
    disconnect_epoch: Cell<u64>,
    /// Set once, by the supervisor, when the `listen` role's first bind attempt has
    /// resolved — either way (§15.42). The `connect` role sets it on its first
    /// supervisor turn, having no inbound artifact for anyone to race.
    ///
    /// This is what lets `load` and `add-node` decline to reply before the socket
    /// their reply implies exists. Before it, the reply was a proxy in time (§9):
    /// `start` only *spawns* this supervisor, so the address was created some
    /// microseconds afterwards — a p50 of 14 µs on an idle Linux 7.0.0-29 box, and
    /// lost to the caller in 40.5% of 1200 trials under an 8-way load (notes §3.38).
    listen_attempted: Cell<bool>,
    /// Woken when [`Self::listen_attempted`] is set. `notify_waiters` stores no
    /// permit, so the flag — not the notification — is the state, and the wait loop
    /// re-reads it. Correct without a lock because both sides live on the one
    /// current-thread `LocalSet`: nothing can run between the loop's read of the flag
    /// and its registration on the `Notified`, because neither of those is an
    /// `.await`.
    listen_attempt_notify: Notify,
}

impl LegShared {
    fn new(purge_on_reconnect: bool) -> LegShared {
        LegShared {
            status: CriticalCell::new(NodeState::new(NodeStatus::Waiting {
                reason: "no peer connected yet".to_owned(),
            })),
            peer_address: CriticalCell::new(None),
            peer_version: Cell::new(None),
            peer_capabilities: Cell::new(0),
            reconnect_count: Cell::new(0),
            unbound: CriticalCell::new(UnboundSet::default()),
            bound_unix_path: CriticalCell::new(None),
            purge_on_reconnect,
            disconnect: Notify::new(),
            disconnect_epoch: Cell::new(0),
            listen_attempted: Cell::new(false),
            listen_attempt_notify: Notify::new(),
        }
    }

    /// Record that the first bind attempt has resolved, and release anyone waiting.
    /// Idempotent by design: the supervisor calls it on every turn of its loop and
    /// only the first is a transition.
    fn mark_listen_attempted(&self) {
        if !self.listen_attempted.replace(true) {
            self.listen_attempt_notify.notify_waiters();
        }
    }
}

/// A handle on one `role = "listen"` leg's **first bind attempt**, handed to the
/// config verb that created the node so that verb's reply cannot precede the socket
/// it implies (§15.42).
///
/// Attempt, not success. A refused bind resolves this too, having already faulted the
/// node with the reason in `state` — §15.8 says an environmental failure changes state
/// and never the reply, so waiting for success here would turn a faulted node into a
/// failed `load`, which is that rule inverted.
pub struct ListenBarrier(Rc<LegShared>);

impl ListenBarrier {
    /// Resolve when the leg's first bind attempt has finished, however it finished.
    ///
    /// The flag is read *before* awaiting, which is what makes this correct when the
    /// attempt already happened while an earlier barrier in the same batch was being
    /// awaited: `notify_waiters` wakes only waiters already registered, so a barrier
    /// that arrives late must be able to see the state rather than the edge.
    pub async fn wait(self) {
        while !self.0.listen_attempted.get() {
            self.0.listen_attempt_notify.notified().await;
        }
    }
}

pub struct LegNode {
    pub name: String,
    faces: Facing,
    transport: Transport,
    role: LegRole,
    address: String,
    insecure_bind: bool,
    reconnect_initial_ms: u64,
    reconnect_max_ms: u64,
    idle_release_ms: u64,
    channels: Vec<String>,
    stats: Rc<HashMap<String, Rc<ChannelStat>>>,
    /// Each `faces = target` channel's target-facing endpoint [`DropCounters`] — the
    /// handle the producing node's `AttachedSink` charges a full-buffer drop to
    /// (`runtime::fan_out`), and the *only* record of what this leg sheds at its own
    /// intake. `start` used to drop it on the floor (`let _ = …remove(&addr)`), which
    /// made the leg the one consuming node kind that never reported it: a peerless
    /// leg could shed tens of megabytes with every counter in `state` reading zero
    /// (LEG-1). Kept exactly as `map.rs`, `codec.rs` and `exec.rs` keep theirs.
    channel_counters: HashMap<String, Arc<DropCounters>>,
    /// Per channel, the node's handle on that channel's §5-targetward queue and the
    /// tally of what teardown destroyed in it (§5, §15.50, notes §3.31/§3.55).
    ///
    /// **Per channel, not per node**, because a leg's whole reporting idiom is per
    /// channel and §5 asks for loss that is *attributable*: an operator whose
    /// `remove-node` reports 400 KiB destroyed has a very different next question if it
    /// all came from one wedged channel. The node-level figure the removal reply
    /// carries ([`Self::discarded_at_teardown`]) is the sum.
    ///
    /// **Which queue that is depends on which way the leg faces**, and both are
    /// targetward in §5's sense — bytes headed for a device, on the direction the
    /// design forbids dropping:
    ///
    /// * `faces = host`: the arbitrated `mpsc::Receiver<Chunk>` every local writing
    ///   origin on that channel's host-facing endpoint feeds. Identical in shape to the
    ///   queue §15.50 already charges on `map`/`codec`/`exec`.
    /// * `faces = target`: the `mpsc::Receiver<Inbound>` of wire-arriving chunks
    ///   [`channel_targetward`] hands into the local graph. A different item type — it
    ///   carries 37-LEG-1's provenance tag — which is why the ledger's inbox is generic
    ///   over its item rather than over `Chunk` alone.
    ///
    /// What is deliberately **not** here: a `faces = target` leg's per-channel relay
    /// (local hostward device data on its way to the wire). Those bytes are hostward,
    /// whose §5 policy is drop-and-count at the consuming boundary, and charging them
    /// to a counter named `discarded_at_teardown` would report hostward loss under a
    /// targetward name.
    channel_teardown: HashMap<String, TeardownLoss>,
    shared: Rc<LegShared>,
    tasks: TaskSet,
}

impl LegNode {
    pub fn create(config: &NodeConfig) -> LegNode {
        let NodeConfig::Leg {
            name,
            faces,
            transport,
            role,
            address,
            insecure_bind,
            reconnect_initial_ms,
            reconnect_max_ms,
            idle_release_ms,
            purge_on_reconnect,
            channels,
            ..
        } = config
        else {
            unreachable!("LegNode::create called with non-Leg config");
        };
        let stats = channels
            .iter()
            .map(|c| (c.clone(), Rc::new(ChannelStat::default())))
            .collect();
        LegNode {
            name: name.clone(),
            faces: *faces,
            transport: *transport,
            role: *role,
            address: address.clone(),
            insecure_bind: *insecure_bind,
            reconnect_initial_ms: *reconnect_initial_ms,
            reconnect_max_ms: *reconnect_max_ms,
            idle_release_ms: *idle_release_ms,
            channels: channels.clone(),
            stats: Rc::new(stats),
            channel_counters: HashMap::new(),
            channel_teardown: channels
                .iter()
                .map(|c| (c.clone(), TeardownLoss::default()))
                .collect(),
            shared: Rc::new(LegShared::new(*purge_on_reconnect)),
            tasks: TaskSet::default(),
        }
    }

    /// Claim this leg's per-channel endpoints out of the endpoint-keyed wiring and
    /// start the supervisor (§7.4). A `faces = host` leg claims each channel's
    /// host-facing maps (fan-out sinks + the arbitrated targetward receiver); a
    /// `faces = target` leg claims each channel's target-facing maps (the local
    /// hostward stream + a targetward sender and lock into the local graph).
    /// The readiness handle for this leg's inbound artifact, or `None` if it has none
    /// (§15.42). Call it *after* [`Self::start`]: before that the supervisor has not
    /// been spawned and nothing will ever resolve it.
    ///
    /// Only the `listen` role gets one. The `connect` role's readiness is the *peer's*,
    /// which no reply can promise and no caller should be made to wait for — blocking
    /// `load` on a dial would make it as slow, and as unreliable, as the far end.
    pub fn listen_barrier(&self) -> Option<ListenBarrier> {
        (self.role == LegRole::Listen).then(|| ListenBarrier(self.shared.clone()))
    }

    pub fn start(&mut self, wiring: &mut Wiring) {
        // The socket send source: the per-channel receivers the pump multiplexes
        // onto the wire. faces=host: the arbitrated targetward stream (host writers
        // → wire). faces=target: the local hostward stream (device → wire).
        let mut send_receivers: Vec<SendReceiver> = Vec::new();
        // Infallible by construction — `create` keys `stats` off the very `channels`
        // this loop walks — and a fallback default here would be worse than a panic:
        // the orphan stat it minted would count bytes `state_extra` never reads,
        // turning a broken invariant into a silent accounting gap (37-LEG-5).
        let stat_for = |ch: &str| {
            self.stats
                .get(ch)
                .cloned()
                .expect("stats is keyed by self.channels")
        };
        // Taken out for the duration of the wiring walk and put back at the end, because
        // `TeardownLoss::watch` needs `&mut` on the entry while `self.stats` and
        // `self.channels` are still being read. Same panic discipline as `stat_for`
        // (37-LEG-5): `create` keys this map off the very `channels` the loops below
        // walk, so a missing entry is a broken invariant, and a defaulted one would
        // drop a channel out of the teardown ledger — the accounting gap this whole
        // change exists to close.
        let mut teardown = std::mem::take(&mut self.channel_teardown);
        // How the pump routes decoded wire events back into the local graph.
        let recv_route: RecvRoute = match self.faces {
            Facing::Host => {
                let mut sinks: HashMap<String, SharedFanOut> = HashMap::new();
                let mut feeds: HashMap<String, TapFeed> = HashMap::new();
                for ch in &self.channels {
                    let addr = EndpointAddr::channel(&self.name, ch);
                    if let Some(s) = wiring.host_fanout.remove(&addr) {
                        sinks.insert(ch.clone(), s);
                    }
                    if let Some(feed) = wiring.tap_feeds.remove(&addr) {
                        feeds.insert(ch.clone(), feed);
                    }
                    if let Some(rx) = wiring.host_targetward_rx.remove(&addr) {
                        // Watched: this is the §5-targetward queue local writers feed,
                        // the shape §15.50 charges. Before this it lived inside the
                        // supervisor's future, so `TaskSet::abort_all` destroyed
                        // whatever a peerless leg had accumulated in it and no counter
                        // in the daemon had ever seen those bytes.
                        let rx = teardown
                            .get_mut(ch)
                            .expect("channel_teardown is keyed by self.channels")
                            .watch(rx);
                        send_receivers.push((ch.clone(), rx, stat_for(ch)));
                    }
                }
                RecvRoute::Host { sinks, feeds }
            }
            Facing::Target => {
                let mut inbound_txs: HashMap<String, mpsc::Sender<Inbound>> = HashMap::new();
                for ch in &self.channels {
                    let addr = EndpointAddr::channel(&self.name, ch);
                    // The local hostward stream reaches the wire pump through a
                    // permanent per-channel relay, so the pump's `select!` over
                    // receivers is untouched by edge surgery: the relay parks on the
                    // endpoint's inbox, drains an edge until `disconnect` closes it,
                    // and parks again (§15.35).
                    if let Some(mut inbox) = wiring.target_inbox.remove(&addr) {
                        let (relay_tx, relay_rx) = mpsc::channel::<Chunk>(CHANNEL_CAP);
                        // Deliberately **not** watched. The relay carries local
                        // *hostward* device data on its way to the wire; §5 governs
                        // that direction with drop-and-count at the consuming boundary
                        // (`dropped_slow_consumer` below), so charging it to
                        // `discarded_at_teardown` would report a hostward loss under a
                        // targetward name. The inbox wrapper is still used, because it
                        // is what `next_send` polls — being *watched* is what makes a
                        // queue teardown-charged, not being an inbox.
                        let relay_rx = TargetwardInbox::new(relay_rx);
                        send_receivers.push((ch.clone(), relay_rx, stat_for(ch)));
                        self.tasks.push(tokio::task::spawn_local(async move {
                            while let Some(mut rx) = inbox.recv().await {
                                while let Some(chunk) = rx.recv().await {
                                    if relay_tx.send(chunk).await.is_err() {
                                        return;
                                    }
                                }
                            }
                        }));
                    }
                    // Keep this endpoint's drop counters rather than discarding them
                    // (LEG-1): they are what the upstream producer charges when this
                    // leg's intake is full — the *only* record of a peerless or
                    // slower-than-the-device leg shedding hostward bytes — and
                    // `state_extra` reports them per channel, as every other
                    // consuming node kind does.
                    if let Some(counters) = wiring.target_counters.remove(&addr) {
                        self.channel_counters.insert(ch.clone(), counters);
                    }
                    // Targetward into the local graph, gated on this leg's on-demand
                    // origin lock. One task per channel does the acquire, the
                    // idle-release, and the (backpressured) write; the pump feeds it
                    // through a bounded per-channel queue so a stalled channel
                    // backpressures the whole connection (§9 head-of-line). The task
                    // exists whether or not an edge is attached and re-reads the live
                    // binding per chunk (§15.35).
                    let edge = wiring
                        .target_edges
                        .remove(&addr)
                        .unwrap_or_else(crate::runtime::TargetEdge::new);
                    let (inbound_tx, inbound_rx) = mpsc::channel::<Inbound>(CHANNEL_CAP);
                    // Watched: wire-arriving bytes headed into the local graph are
                    // targetward, so a teardown that destroys this queue owes §5 the
                    // same number a `map`'s does. The item type differs — every chunk
                    // carries 37-LEG-1's provenance tag — which is the reason the
                    // ledger's inbox is generic over its item.
                    let inbound_rx = teardown
                        .get_mut(ch)
                        .expect("channel_teardown is keyed by self.channels")
                        .watch(inbound_rx);
                    inbound_txs.insert(ch.clone(), inbound_tx);
                    let stat = stat_for(ch);
                    let idle = Duration::from_millis(self.idle_release_ms);
                    self.tasks.push(tokio::task::spawn_local(channel_targetward(
                        inbound_rx,
                        edge,
                        idle,
                        stat,
                        self.shared.clone(),
                    )));
                }
                RecvRoute::Target(inbound_txs)
            }
        };

        self.channel_teardown = teardown;

        self.tasks
            .push(tokio::task::spawn_local(supervise(SuperviseArgs {
                faces: self.faces,
                transport: self.transport,
                role: self.role,
                address: self.address.clone(),
                reconnect_initial_ms: self.reconnect_initial_ms,
                reconnect_max_ms: self.reconnect_max_ms,
                channels: self.channels.clone(),
                send_receivers,
                recv_route,
                stats: self.stats.clone(),
                shared: self.shared.clone(),
            })));
    }

    /// The node's status *and* the moment it was entered (§7 "with reason and
    /// timestamp"), so an operator watching a flapping peer reads the age of the
    /// condition rather than the age of the last poll (STATE-1).
    pub fn status(&self) -> NodeState {
        self.shared.status.with(|s| s.clone())
    }

    pub fn state_extra(&self) -> Value {
        let channels: serde_json::Map<String, Value> = self
            .channels
            .iter()
            .map(|ch| {
                let stat = &self.stats[ch];
                let obj = json!({
                    "binding": if stat.bound.get() { "bound" } else { "waiting" },
                    "active": stat.active.get(),
                    "delivered_hostward": stat.delivered_hostward.get(),
                    "accepted_targetward": stat.accepted_targetward.get(),
                    "discarded_hostward": stat.discarded_hostward.get(),
                    "discarded_targetward": stat.discarded_targetward.get(),
                    "discarded_unframable": stat.discarded_unframable.get(),
                    "discarded_peer_gone": stat.discarded_peer_gone.get(),
                    "purged_on_reconnect": stat.purged_on_reconnect.get(),
                    // What the *upstream* producer shed because this channel's
                    // intake was full — a `faces = target` leg's own consuming
                    // boundary (§5). Structurally zero for `faces = host`, whose
                    // channel endpoints are host-facing and have no such boundary.
                    "dropped_slow_consumer": self
                        .channel_counters
                        .get(ch)
                        .map_or(0, |c| c.dropped_full()),
                    // Targetward bytes this channel's own queue was still holding when
                    // the node stopped (§15.50). It reads `0` for the whole of a
                    // channel's working life and moves exactly once, at `signal_stop`,
                    // so on a leg you can still see in `state` it is always `0` and the
                    // queue behind it is backlog rather than loss — a peerless
                    // `faces = host` leg is *designed* to accumulate one (§5
                    // backpressure), and that backlog is delivered the moment a peer
                    // arrives. The figure that matters is the one the `remove-node`
                    // reply carries.
                    "discarded_at_teardown": self
                        .channel_teardown
                        .get(ch)
                        .map_or(0, TeardownLoss::bytes),
                });
                (ch.clone(), obj)
            })
            .collect();
        // Announced-but-unconfigured identities: visible state, no endpoint (§8),
        // in the bounded insertion order LEG-2 established.
        let mut channels = channels;
        let unbound_overflow = self.shared.unbound.with(|u| {
            for id in &u.order {
                channels.insert(id.clone(), json!({ "binding": "unbound" }));
            }
            u.overflow
        });
        let mut obj = json!({
            "role": role_str(self.role),
            "transport": transport_str(self.transport),
            "faces": self.faces.to_string(),
            "connection": self.shared.status.with(connection_str),
            "peer_address": self.shared.peer_address.with(|p| p.clone()),
            "protocol_version": self.shared.peer_version.get(),
            "capabilities": self.shared.peer_capabilities.get(),
            "reconnect_count": self.shared.reconnect_count.get(),
            // Wire frames whose unconfigured identity the LEG-2 cap refused to
            // record: a peer inventing identities is visible rather than silent.
            "unbound_overflow": unbound_overflow,
            // The node-level sum of the per-channel figure above, present for the same
            // reason `map`/`codec`/`exec` carry one: it is the number the `remove-node`
            // and `teardown` replies quote, and reading the two in one place is what
            // makes the reply checkable against `state` before the node is gone.
            "discarded_at_teardown": self.discarded_at_teardown(),
            "channels": channels,
        });
        // The §9 named footgun: surface it as a visible, greppable confession in
        // `state` when a non-loopback bind was opted into (§15.12).
        if self.insecure_bind {
            obj["insecure_bind"] = json!(true);
        }
        obj
    }

    /// Remove the `listen`+`unix` socket inode — our filesystem artifact — so a
    /// torn-down or removed leg leaves no orphan, mirroring the PTY symlink cleanup
    /// (§7.2) and the control-socket removal (§10).
    ///
    /// Only the path this node *actually bound* is unlinked, and the record is taken
    /// so the unlink happens exactly once (teardown then Drop). The former code
    /// unlinked the configured address for every `listen`+`unix` leg, bound or not,
    /// so a leg configured with `address = "/home/me/notes.txt"` deleted that file on
    /// teardown even though `bind_listener` had refused it (SEC-8).
    fn unlink_listen_socket(&self) {
        if let Some(path) = self.shared.bound_unix_path.with_mut(Option::take) {
            let _ = std::fs::remove_file(path);
        }
    }

    /// The cheap, non-blocking half of teardown: abort every data-plane task (which
    /// drops the listener and the socket with them) and release the filesystem
    /// artifact, returning immediately. `daemon.rs` signals every node before paying
    /// any join cost, so a slow node cannot stall the runtime thread on behalf of
    /// its neighbours during `remove-node` / `load --replace` / shutdown (BND-1).
    /// The leg has nothing to join, so [`Self::teardown`] is exactly this; the split
    /// exists so the two-phase shape is uniform across node kinds. Idempotent.
    pub fn signal_stop(&mut self) {
        // Count before aborting, and in that order: `abort_all` is what drops the
        // futures the per-channel targetward queues live in, and every chunk queued in
        // them goes with it (§5, §15.50). Draining first also ends the pumps
        // *gracefully* — an emptied inbox answers `None` — so the abort is a backstop
        // rather than the mechanism.
        for loss in self.channel_teardown.values() {
            loss.drain();
        }
        self.tasks.abort_all();
        self.unlink_listen_socket();
    }

    /// Targetward bytes this node destroyed at teardown, summed over its channels, for
    /// the verb that removed it to report (§5: the node is about to stop existing, so
    /// `state` cannot be the only home for its last loss). The per-channel split — the
    /// attributable half of the same fact — stays in [`Self::state_extra`].
    pub fn discarded_at_teardown(&self) -> u64 {
        self.channel_teardown
            .values()
            .map(TeardownLoss::bytes)
            .fold(0u64, u64::saturating_add)
    }

    pub fn teardown(&mut self) {
        self.signal_stop();
    }
}

impl Drop for LegNode {
    fn drop(&mut self) {
        // Kept after SIMPB-10 moved the task half into [`TaskSet`]: a leg's teardown is
        // more than aborting tasks — it must also unlink the listening socket it
        // created, which is filesystem state no field drop releases.
        self.signal_stop();
    }
}

fn role_str(role: LegRole) -> &'static str {
    match role {
        LegRole::Listen => "listen",
        LegRole::Connect => "connect",
    }
}

fn transport_str(t: Transport) -> &'static str {
    match t {
        Transport::Tcp => "tcp",
        Transport::Unix => "unix",
    }
}

fn connection_str(state: &NodeState) -> &'static str {
    match state.status() {
        NodeStatus::Active => "connected",
        NodeStatus::Waiting { .. } => "waiting",
        NodeStatus::Faulted { .. } => "faulted",
    }
}

/// One socket-send source: a channel identity, its bounded receiver, and its
/// stat (for the `bound` gate — a `waiting` channel is not drained onto the wire,
/// so its writers backpressure per faulted-and-wait rather than have their bytes
/// dropped at the unconfigured peer).
type SendReceiver = (String, TargetwardInbox, Rc<ChannelStat>);

/// How the pump routes a decoded wire event into the local graph.
enum RecvRoute {
    /// faces=host: fan each channel's hostward data out to local consumers, and
    /// mirror it to the channel's tap hub for taps and the replay ring (§17).
    Host {
        sinks: HashMap<String, SharedFanOut>,
        feeds: HashMap<String, TapFeed>,
    },
    /// faces=target: hand each channel's targetward data to its per-channel task.
    Target(HashMap<String, mpsc::Sender<Inbound>>),
}

/// One wire-arriving targetward chunk, tagged with the connection that delivered it.
///
/// The tag is [`LegShared::disconnect_epoch`] read at **enqueue**, on the pump's read
/// half, because that is the only moment at which the chunk's provenance is known.
/// Reading it after `rx.recv()` returns instead attributes the chunk to whichever
/// connection is live when [`channel_targetward`] is next polled, and a peer whose
/// last data frames and FIN are readable together — one that sends its commands and
/// disconnects in one breath — is enqueued, EOF'd and epoch-bumped inside a single
/// poll of the supervise task. Every queued chunk then snapshotted the *post*-bump
/// epoch, both purge checks compared equal, and the dead connection's whole backlog
/// fired into the device the §6 purge exists to protect (37-LEG-1). Provenance
/// carried with the chunk is decidable whenever the channel task gets round to it.
struct Inbound {
    /// The disconnect epoch current when the pump enqueued this chunk. A tag that
    /// differs from the live epoch means the connection that delivered it is gone.
    epoch: u64,
    bytes: Chunk,
}

impl TeardownBytes for Inbound {
    /// The payload only. The provenance tag is bookkeeping this node added on the way
    /// in, not something a peer sent or a device would have received, so counting it
    /// would inflate a §5 loss figure with the daemon's own overhead.
    fn teardown_bytes(&self) -> u64 {
        self.bytes.len() as u64
    }
}

struct SuperviseArgs {
    faces: Facing,
    transport: Transport,
    role: LegRole,
    address: String,
    reconnect_initial_ms: u64,
    reconnect_max_ms: u64,
    channels: Vec<String>,
    send_receivers: Vec<SendReceiver>,
    recv_route: RecvRoute,
    stats: Rc<HashMap<String, Rc<ChannelStat>>>,
    shared: Rc<LegShared>,
}

/// Why a connection's pump ended.
enum PumpEnd {
    /// The socket closed or errored — reconnect (faulted-and-wait).
    PeerGone,
    /// The peer sent a malformed frame — a §9 clause-6 protocol violation.
    Protocol(String),
}

/// Supervise the socket: (re)establish the connection, handshake, then pump both
/// directions until it drops, then fault, back off, and retry (§7.4). The send
/// receivers, the recv route, and the per-channel targetward tasks persist across
/// reconnects; only the socket and the pump are per-connection.
async fn supervise(a: SuperviseArgs) {
    // Exponential reconnect backoff (§7.4), reset on a good connection. The listen
    // role's bind shares the schedule with the connect role's dial: both are the same
    // environmental retry.
    let mut backoff = boundary::Backoff::exponential(a.reconnect_initial_ms, a.reconnect_max_ms);
    let mut connected_before = false;
    // The listen role's socket, taken on the first successful bind and then kept for
    // every peer after it. `None` for the connect role, which dials instead.
    let mut listener: Option<Listener> = None;

    loop {
        // Bind (listen role) — *inside* the retry loop, because a bind fails for the
        // same environmental reasons a dial does: an address whose interface is not up
        // yet, a port a departing process still holds, a descriptor table momentarily
        // exhausted. §11 says environmental failures leave nodes "visible in state,
        // healing on their own", generalized to every boundary type by §15.8, and a
        // one-shot bind made this the daemon's one fault that could only be cleared by
        // remove-and-re-add (37-LEG-2). The SEC-8 stale-socket check re-runs on every
        // attempt with it, which is what makes the common case — a predecessor's inode
        // outliving the peer that was still answering on it — heal at all.
        if a.role == LegRole::Listen && listener.is_none() {
            match bind_listener(a.transport, &a.address).await {
                Ok(l) => {
                    // Record the inode this node actually created, so teardown unlinks
                    // its own artifact and nothing else (SEC-8).
                    if a.transport == Transport::Unix {
                        let path = a.address.clone();
                        a.shared.bound_unix_path.with_mut(|p| *p = Some(path));
                    }
                    listener = Some(l);
                    // A leg that is listening is waiting for a peer, not faulted. On
                    // the first bind this is the status the node already carries, so
                    // `NodeState::set` leaves the stamp alone (STATE-1); after a
                    // refused bind it is the transition an operator watches for — a
                    // heal nobody can observe is not one (§7).
                    set_status(
                        &a.shared,
                        NodeStatus::Waiting {
                            reason: "no peer connected yet".to_owned(),
                        },
                    );
                    // The config verb's reply is held until this line (§15.42): from
                    // here on the address is accepting, so a caller that dials the
                    // instant its reply lands cannot be told the socket is not there.
                    a.shared.mark_listen_attempted();
                }
                Err(e) => {
                    set_status(
                        &a.shared,
                        NodeStatus::Faulted {
                            reason: format!("bind {:?}: {e}", a.address),
                        },
                    );
                    // Release the config verb on a *failed* bind too. The node is
                    // faulted with the reason in `state`, which is where §15.8 puts an
                    // environmental failure; holding the reply here would stall `load`
                    // for the whole backoff schedule of an address that may never bind.
                    a.shared.mark_listen_attempted();
                    backoff.sleep().await;
                    continue;
                }
            }
        }
        // The `connect` role, and every turn after the first: nothing inbound for a
        // caller to race, so nobody is kept waiting.
        a.shared.mark_listen_attempted();

        // Establish a connection.
        let established = match &listener {
            Some(l) => l.accept().await.map(|(s, addr)| (s, Some(addr))),
            None => connect_stream(a.transport, &a.address)
                .await
                .map(|s| (s, None)),
        };
        let (mut stream, peer_addr) = match established {
            Ok(v) => v,
            Err(e) => {
                set_status(
                    &a.shared,
                    NodeStatus::Faulted {
                        reason: format!("connect {:?}: {e}", a.address),
                    },
                );
                backoff.sleep().await;
                continue;
            }
        };

        // Handshake: send our hello, read the peer's, validate and bind (§9). The
        // whole exchange is bounded by one overall deadline (not just per-read), so
        // a trickling or silent peer cannot wedge the supervisor — critical for the
        // listen role, whose reject-extras arm only runs *after* the handshake, so a
        // stalled handshake would otherwise stall every other peer.
        let hs = tokio::time::timeout(
            HANDSHAKE_TIMEOUT,
            handshake(&mut stream, &a.channels, &a.shared),
        )
        .await;
        let leftover = match hs {
            Ok(Ok((hello, leftover))) => {
                bind_channels(&a.channels, &hello, &a.stats, &a.shared);
                // Listen learns the peer from `accept()`; connect dialed a known
                // endpoint, so report the dialed address (§7.4 peer address, LEG-2).
                let addr = peer_addr.unwrap_or_else(|| a.address.clone());
                a.shared.peer_address.with_mut(|p| *p = Some(addr));
                set_status(&a.shared, NodeStatus::Active);
                backoff.reset(); // a good connection resets backoff
                leftover
            }
            Ok(Err(reason)) => {
                set_status(&a.shared, NodeStatus::Faulted { reason });
                if a.role == LegRole::Connect {
                    backoff.sleep().await;
                }
                continue;
            }
            Err(_) => {
                set_status(
                    &a.shared,
                    NodeStatus::Faulted {
                        reason: "handshake deadline exceeded".to_owned(),
                    },
                );
                if a.role == LegRole::Connect {
                    backoff.sleep().await;
                }
                continue;
            }
        };

        // Purge-on-reconnect, sending side: on a reconnect (not the first
        // connection) this leg's local targetward backlog is outage-era stale
        // (§6/§7.4), so it must not fire into a device that rebooted. Only the
        // faces=host side has one *here* — a faces=host leg's send source is the
        // arbitrated targetward stream of local writers, while a faces=target leg's
        // send source is hostward device data, which §5 governs with drop-and-count
        // at boundaries, not with purge. The faces=target side's targetward backlog
        // arrives from the wire and is purged in `channel_targetward`, which owns it
        // (LEG-3).
        //
        // The drain runs to *quiescence* on the shared inbox — drain, yield, redrain,
        // bounded rounds — because §6 names the case a single `try_recv` pass misses:
        // "including a chunk held by a producer suspended mid-send" (DM-2/LEG-1). It is
        // the same method the serial node's purge calls, so the two instances of the one
        // purge rule cannot drift again; and because it drains *through* the node's slot
        // rather than borrowing the receiver out of it, a `remove-node` landing on one of
        // its yields still finds the queue where the node can count it (§15.50).
        //
        // The counter is handed *to* the drain rather than charged from a return value,
        // which is the other half of the same race and was measured on the serial node's
        // identical caller (notes §3.59): a tally accumulated in this future's frame dies
        // with the frame when `abort_all` fires, and by then the rounds have already
        // emptied the queue the ledger would otherwise have charged. The leg's exposure
        // is per channel and therefore N-fold — every `send_receivers` entry the loop has
        // already visited is a queue that is drained, and a tally that is not yet
        // anywhere. `ChannelStat` lives on the node, not in this task, so a charge landing
        // mid-purge survives the abort.
        if connected_before && a.shared.purge_on_reconnect && a.faces == Facing::Host {
            for (_ch, rx, stat) in &a.send_receivers {
                rx.purge_to_quiescence(&|n| stat.purged_on_reconnect.add(n))
                    .await;
            }
        }
        connected_before = true;

        // Pump both directions until the connection drops.
        let (read_half, write_half) = tokio::io::split(stream);
        let send_is_hostward = a.faces == Facing::Target;
        let end = pump(
            read_half,
            write_half,
            leftover,
            &a.send_receivers,
            send_is_hostward,
            &a.recv_route,
            &a.stats,
            &a.shared,
            listener.as_ref(),
        )
        .await;

        clear_connection_state(&a.stats, &a.shared);

        a.shared
            .reconnect_count
            .set(a.shared.reconnect_count.get() + 1);
        match end {
            PumpEnd::PeerGone => set_status(
                &a.shared,
                NodeStatus::Waiting {
                    reason: "peer disconnected; awaiting reconnect".to_owned(),
                },
            ),
            PumpEnd::Protocol(reason) => set_status(&a.shared, NodeStatus::Faulted { reason }),
        }
        if a.role == LegRole::Connect {
            backoff.sleep().await;
        }
    }
}

/// The connection dropped: forget everything that describes *that peer*, and pulse
/// the disconnect signal.
///
/// Binding is per-connection (§8), so the node parks its channels until the next
/// peer arrives (faulted-and-wait). The handshake-derived fields go with it: a
/// peerless leg reporting the departed peer's `protocol_version` and `capabilities`
/// reads as a live handshake, which is exactly the stale state LEG-5 named — every
/// field here is re-established by the next `handshake`/`bind_channels` pair.
///
/// The epoch is bumped *before* the pulse: `notify_waiters` stores no permit, so a
/// channel task blocked in a backpressured local write misses the pulse and detects
/// the drop by re-reading the epoch after its write instead (§7.1, LEG-4).
fn clear_connection_state(stats: &Rc<HashMap<String, Rc<ChannelStat>>>, shared: &Rc<LegShared>) {
    for stat in stats.values() {
        stat.bound.set(false);
        stat.active.set(false);
    }
    shared.unbound.with_mut(UnboundSet::clear);
    shared.peer_address.with_mut(|p| *p = None);
    shared.peer_version.set(None);
    shared.peer_capabilities.set(0);
    shared
        .disconnect_epoch
        .set(shared.disconnect_epoch.get() + 1);
    shared.disconnect.notify_waiters();
}

/// Pump one connection: the socket write half drains the send source and the read
/// half decodes and routes, run as **concurrently-polled** futures so a
/// backpressured write never starves the read half (§15.22). For the listen role a
/// third arm rejects concurrent second connections (§7.4).
#[allow(clippy::too_many_arguments)]
async fn pump(
    mut read_half: tokio::io::ReadHalf<Box<dyn DuplexStream>>,
    mut write_half: tokio::io::WriteHalf<Box<dyn DuplexStream>>,
    leftover: Vec<u8>,
    // Shared, not exclusive: an inbox's receive goes through its shared slot, which is
    // the whole point of the §15.50 ledger — the node keeps a handle on the queue this
    // pump is draining, so aborting the pump no longer takes the queue with it.
    send_receivers: &[SendReceiver],
    send_is_hostward: bool,
    recv_route: &RecvRoute,
    stats: &Rc<HashMap<String, Rc<ChannelStat>>>,
    shared: &Rc<LegShared>,
    listener: Option<&Listener>,
) -> PumpEnd {
    let mut send_start = 0usize;
    // The chunk the write half has taken out of its bounded receiver but not yet put
    // on the wire in full. It lives in *this* frame rather than the write future's
    // because the peer can take it away by either of two routes (LEG-2): `write_all`
    // fails, or the read half returns `PeerGone` first and `select!` drops the write
    // future while it is suspended inside `write_all`, taking its stack frame — and
    // the popped chunk — with it. A residual charged only on the error arm would miss
    // the second exit entirely, so ownership moves out to whoever survives the race.
    let in_flight: Cell<Option<InFlight>> = Cell::new(None);
    // Write half: multiplex the send source onto the wire. A chunk larger than a
    // single frame is fragmented into consecutive Data frames on the same channel
    // (the peer reassembles transparently) — never dropped, since READ_BUF ==
    // MAX_FRAME_SIZE means a full read always overflows the header, and the `send`
    // verb accepts arbitrary-length lines (§5 no-drop / all-loss-counted, §9 clause 5).
    let write = async {
        loop {
            match next_send(send_receivers, &mut send_start).await {
                Some((ch, bytes)) => {
                    if let Some(stat) = stats.get(&ch) {
                        stat.active.set(true);
                    }
                    // Take custody of the whole chunk before the first `.await` that
                    // could lose it. There is no suspension point between `next_send`
                    // returning and this line, so a chunk is never off both the
                    // receiver and this slot at once (LEG-2).
                    in_flight.set(Some(InFlight {
                        channel: ch.clone(),
                        remaining: bytes.len() as u64,
                    }));
                    // Fragment an over-large chunk into consecutive Data frames
                    // rather than drop it (§15.24); the peer reassembles per channel.
                    for item in data_frames(ch.as_str(), &bytes) {
                        match item {
                            DataFrame::Piece(piece_len, frame) => {
                                if write_half.write_all(&frame).await.is_err() {
                                    return PumpEnd::PeerGone;
                                }
                                // On the wire and out of danger: shrink the custody
                                // slot *before* crediting throughput, so the two can
                                // never both claim the same bytes.
                                let n = piece_len as u64;
                                discharge(&in_flight, n);
                                if let Some(stat) = stats.get(&ch) {
                                    if send_is_hostward {
                                        stat.delivered_hostward
                                            .set(stat.delivered_hostward.get() + n);
                                    } else {
                                        stat.accepted_targetward
                                            .set(stat.accepted_targetward.get() + n);
                                    }
                                }
                            }
                            // The framer refused a piece and handed back the exact
                            // source-byte tail that never reached the wire. Counting
                            // it is the half of invariant 3 that says "count any
                            // residual" — the old `map_while` shape truncated the
                            // chunk in silence (RV-9).
                            DataFrame::Residual(residual) => {
                                discharge(&in_flight, residual as u64);
                                if let Some(stat) = stats.get(&ch) {
                                    stat.discarded_unframable.add(residual as u64);
                                }
                            }
                        }
                    }
                    // Fully accounted — framed onto the wire, or charged as an
                    // unframable residual. Release custody before parking on the next
                    // `next_send`, so a peer that dies while this half waits charges
                    // nothing (LEG-2).
                    in_flight.set(None);
                }
                // Every send source has closed (a faces=target leg whose local
                // producers are all gone). Park the write half so the independent
                // read/targetward direction stays alive (§16.1 park-don't-teardown) —
                // teardown aborts the task.
                None => boundary::park().await,
            }
        }
    };
    // Read half: decode envelope frames and route them into the local graph.
    let read = async {
        let mut decoder = FrameDecoder::new();
        decoder.push(&leftover);
        let mut readbuf = vec![0u8; READ_BUF];
        loop {
            loop {
                match decoder.next_event() {
                    Ok(Some(ev)) => route_recv(ev, recv_route, stats, shared).await,
                    Ok(None) => break,
                    Err(e) => return PumpEnd::Protocol(e.to_string()),
                }
            }
            match read_half.read(&mut readbuf).await {
                Ok(0) | Err(_) => return PumpEnd::PeerGone,
                Ok(k) => decoder.push(&readbuf[..k]),
            }
        }
    };
    // Third arm: never ends the pump, so it ends only via the write/read halves
    // above. The listen role actively rejects a concurrent second peer (§7.4); the
    // connect role has no listener, so it parks (§16.1 park-don't-teardown).
    let reject_extra = async {
        match listener {
            Some(l) => reject_extra_peers(move || l.accept()).await,
            None => boundary::park().await,
        }
    };
    // Concurrently-polled halves (§15.22): a backpressured write never starves the
    // read half.
    let end = boundary::race3(write, read, reject_extra).await;
    // Whatever the write half still had custody of is gone with the connection —
    // via the `write_all` error arm or via `select!` dropping the suspended write
    // future, indistinguishable from here and deliberately so (LEG-2). The pieces
    // already on the wire were discharged as they went, so `remaining` is the exact
    // untransmitted tail: §5's "loss is always visible and attributable", and
    // invariant 3's third clause, applied to the sending side of the wire.
    if let Some(f) = in_flight.take()
        && f.remaining > 0
        && let Some(stat) = stats.get(&f.channel)
    {
        stat.discarded_peer_gone.add(f.remaining);
    }
    end
}

/// A chunk the socket write half has taken out of its bounded receiver but not yet
/// put on the wire in full — [`pump`]'s custody slot (LEG-2).
struct InFlight {
    /// The channel whose `ChannelStat` the untransmitted tail is charged to.
    channel: String,
    /// Source bytes of the chunk that have not reached the wire.
    remaining: u64,
}

/// Account `n` source bytes of the in-flight chunk as no longer at risk (framed onto
/// the wire, or charged as an unframable residual).
fn discharge(slot: &Cell<Option<InFlight>>, n: u64) {
    if let Some(mut f) = slot.take() {
        f.remaining = f.remaining.saturating_sub(n);
        slot.set(Some(f));
    }
}

/// How long the reject-a-second-peer loop waits after its first failing `accept`,
/// and the cap it doubles toward (LEG-3). The same exponential shape the supervisor's
/// own accept uses, with a smaller floor because the common case here — a real second
/// peer knocking — must still be refused promptly.
const REJECT_BACKOFF_INITIAL_MS: u64 = 5;
const REJECT_BACKOFF_MAX_MS: u64 = 1_000;

/// The `listen` role's reject-a-second-peer arm (§7.4), parameterized over the accept
/// so a test can hand it one that always fails.
///
/// An `Err` from `accept(2)` is **not** a readiness event: under EMFILE/ENFILE it
/// returns immediately, so the former `if let Ok(…)` shape re-called it at once and
/// spun the daemon's single runtime thread at ~54,000 accepts per second — silently,
/// because this arm alone logged nothing (LEG-3, the §15.36 "busy-spun core" class).
/// Errors now warn and back off toward a one-second cap; a good accept resets the
/// schedule, so a burst of legitimate second peers is still refused at full speed.
/// Never returns: the pump ends only through its two data halves (§16.1).
async fn reject_extra_peers<S, F, Fut>(mut accept: F) -> PumpEnd
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = std::io::Result<S>>,
{
    let mut backoff =
        boundary::Backoff::exponential(REJECT_BACKOFF_INITIAL_MS, REJECT_BACKOFF_MAX_MS);
    loop {
        match accept().await {
            Ok(extra) => {
                drop(extra); // close it immediately
                backoff.reset();
            }
            Err(e) => {
                tracing::warn!(target: "leg", "refusing a concurrent second peer failed: {e}");
                backoff.sleep().await;
            }
        }
    }
}

/// Route one decoded wire event into the local graph. Hostward fan-out is lossy at
/// the consuming boundary (`try_send` + counters); targetward is backpressured
/// (`send().await`), which propagates whole-connection head-of-line blocking (§9).
async fn route_recv(
    ev: Event,
    route: &RecvRoute,
    stats: &Rc<HashMap<String, Rc<ChannelStat>>>,
    shared: &Rc<LegShared>,
) {
    let ch = ev.channel.as_str();
    match ev.kind {
        EventKind::Data(bytes) => {
            let n = bytes.len() as u64;
            let stat = stats.get(ch);
            if let Some(s) = stat {
                s.active.set(true);
            }
            match route {
                RecvRoute::Host { sinks, feeds } => match sinks.get(ch) {
                    // The one shared per-channel hostward routing block (SIMP-1). It
                    // mirrors to this channel's tap hub for taps and the replay ring
                    // (§17) independent of whether a local consumer is bound, and
                    // *outside* the fan-out's accounting; it charges the
                    // all-sinks-closed case to `unattached` itself, which is exactly
                    // the case this loop used to swallow — an announced channel whose
                    // local consumer was cascade-removed while the leg lived on; it
                    // credits `FanOut::delivered` — what a sink actually took — never
                    // `live` (a *full* sink is live) and never `n - dropped_full`
                    // (which credited zero for a chunk one consumer received in full
                    // while another's buffer was full), LEGD-2 in both directions; and
                    // it mirrors the full-buffer loss into `discarded_hostward`
                    // through this node's
                    // own `add_dropped_full`, which is the one place the leg's rule
                    // differs from the codecs'. An unconfigured identity has no
                    // `ChannelStat` to charge, so the helper absorbs its bytes and the
                    // identity is reported as `unbound` state instead (§8:
                    // announcements never grow the graph).
                    Some(chsinks) => route_channel_data(
                        &bytes,
                        feeds.get(ch),
                        Some(chsinks),
                        stat.map(|s| &**s as &dyn HostwardChannelStat),
                    ),
                    None => {
                        // Not routed through the helper: an unconfigured identity here
                        // is `unbound` *state*, not a scratch discard, and the leg
                        // mirrors even an unbound channel to its tap feed.
                        if let Some(feed) = feeds.get(ch) {
                            feed.mirror(&bytes);
                        }
                        note_undeliverable(ch, stat, n, shared, Undeliverable::Hostward);
                    }
                },
                RecvRoute::Target(txs) => match txs.get(ch) {
                    // The per-channel task counts accepted_targetward once the local
                    // device-write handoff accepts; here we just backpressure.
                    Some(tx) => {
                        // Tag the chunk with the connection delivering it (37-LEG-1):
                        // this is the last moment that fact is knowable, and the read
                        // half of a connection that is about to end is still *this*
                        // connection.
                        let inbound = Inbound {
                            epoch: shared.disconnect_epoch.get(),
                            bytes,
                        };
                        // A closed queue means this channel's writer task is gone
                        // (torn down). The bytes are lost either way; §5 wants the
                        // loss counted where it happens rather than swallowed.
                        if tx.send(inbound).await.is_err()
                            && let Some(s) = stat
                        {
                            s.discarded_targetward.add(n);
                        }
                    }
                    None => note_undeliverable(ch, stat, n, shared, Undeliverable::Targetward),
                },
            }
        }
        EventKind::Open => {
            if let Some(s) = stats.get(ch) {
                s.active.set(true);
            }
        }
        EventKind::Close => {
            if let Some(s) = stats.get(ch) {
                s.active.set(false);
            }
        }
        EventKind::Error(msg) => {
            tracing::debug!(target: "leg", channel = %ev.channel, "peer channel error: {msg}");
        }
    }
}

/// Which way the undeliverable wire data was travelling, and therefore which
/// counter it is charged to. A `faces = host` leg's arriving data is hostward
/// (device → local consumers); a `faces = target` leg's is targetward (remote
/// operator → local graph). Charging both to `discarded_hostward` was LEG-4: the
/// loss was counted, the direction was a lie.
#[derive(Clone, Copy, Debug)]
enum Undeliverable {
    Hostward,
    Targetward,
}

/// Handle wire data for a channel with no local edge behind it. A *configured*
/// channel with no consumer bound is a §5 boundary drop — counted in its direction
/// (like the serial node's discard-when-unattached). An *unconfigured* identity is
/// `unbound` state — its bytes are dropped (§8: announcements never grow the graph)
/// and the identity is surfaced, bounded, for an operator (LEG-2).
fn note_undeliverable(
    ch: &str,
    stat: Option<&Rc<ChannelStat>>,
    n: u64,
    shared: &Rc<LegShared>,
    direction: Undeliverable,
) {
    if let Some(s) = stat {
        // Configured but unattached: dropped and counted, not "unbound".
        let counter = match direction {
            Undeliverable::Hostward => &s.discarded_hostward,
            Undeliverable::Targetward => &s.discarded_targetward,
        };
        counter.add(n);
        return;
    }
    shared.unbound.with_mut(|unbound| unbound.insert(ch));
}

/// A faces=target channel's targetward task: hand each wire-arriving chunk into the
/// local graph, gated on this leg's on-demand origin lock (§6). Acquires implicitly
/// on data arrival and releases after `idle` *or* on peer disconnect (§7.1); the
/// framed chunk is backpressured (`send().await`), never dropped — except by
/// purge-on-reconnect, §6's one sanctioned targetward drain.
///
/// **The receiving side's purge (LEG-3).** The sending side of a leg is not the only
/// place outage-era commands accumulate: this task owns a bounded queue of up to
/// [`CHANNEL_CAP`] wire-arriving chunks plus the one it is currently writing, and
/// nothing used to purge them — so §6/§7.4's guarantee ("twenty minutes of buffered
/// commands must not fire into its boot prompt") held on computer A and quietly did
/// not on computer B. The purge runs when the peer drops rather than when it
/// returns: the pump is dead by then, so no fresh byte can be swallowed with the
/// stale ones, and the memory is freed for the duration of the outage. Every drop
/// here is counted into `purged_on_reconnect`, the same counter the sending side
/// reports.
///
/// **Per chunk, not per moment (37-LEG-1).** Which chunks are outage-era is decided
/// from the tag each one carries out of [`Inbound`], never from when this task
/// happened to dequeue it — the sending side's `drain_to_quiescence` approximates
/// provenance by time because a local backlog has no other record of it, and this
/// queue does. Two consequences worth stating: the whole backlog is discarded one
/// attributed chunk per loop turn rather than in one drain, and a chunk a *live*
/// connection put in the queue behind stale ones is delivered rather than swept up
/// with them. A chunk the pump is suspended mid-`send` on — §6's named case — carries
/// the tag it was stamped with before the suspension, so it is attributed correctly
/// whichever side of the disconnect it lands on.
async fn channel_targetward(
    rx: TargetwardInbox<Inbound>,
    edge: SharedTargetEdge,
    idle: Duration,
    stat: Rc<ChannelStat>,
    shared: Rc<LegShared>,
) {
    let purging = shared.purge_on_reconnect;
    // What this task believes it currently holds. Tracked as the binding itself, not
    // a bare flag, because `connect`/`disconnect` can replace the local edge under a
    // running task (§15.35): a binding that changed identity is one this task never
    // acquired, so the old floor must be yielded rather than re-released blindly.
    let mut holding: Option<(SharedLock, OriginId)> = None;
    loop {
        // Re-read the live edge. `disconnect` clears it (and releases the lock on
        // this origin's behalf), so an unbound channel counts and drops what the
        // wire delivers rather than wedging the whole connection behind a local
        // endpoint the operator detached.
        let binding = edge.origin();
        match (&binding, &holding) {
            (Some((_, _, new_id)), Some((old_lock, old_id))) if new_id != old_id => {
                release(old_lock, *old_id);
                holding = None;
            }
            (None, Some((old_lock, old_id))) => {
                release(old_lock, *old_id);
                holding = None;
            }
            _ => {}
        }
        let Some((tx, lock, id)) = binding else {
            // Unattached: **drain and count**, deliberately *not* park.
            //
            // The interior nodes park here, because their targetward pump serves one
            // endpoint and stalling it backpressures exactly that endpoint's writers
            // (§5). A leg is different: these bytes come from a *remote peer* over
            // one shared connection, and a parked channel fills its bounded queue and
            // then head-of-line blocks every other channel on the link (§9). Blocking
            // a whole cross-machine link because one local endpoint was detached is
            // worse than counting, and §7.4 already treats data for a channel with no
            // local endpoint as counted loss ("announced but unbound").
            let Some(inbound) = rx.recv().await else {
                break; // source closed (torn down)
            };
            stat.discarded_targetward.add(inbound.bytes.len() as u64);
            continue;
        };
        let msg = if holding.is_some() {
            tokio::select! {
                v = rx.recv() => v,
                _ = tokio::time::sleep(idle) => {
                    release(&lock, id);
                    holding = None;
                    continue;
                }
                // The peer dropped: yield the local endpoint's floor now, so a local
                // operator is not blocked behind a vanished remote (§7.1). What the
                // dead connection left queued is discarded as the loop dequeues it —
                // every chunk names the connection that delivered it (§6, LEG-3,
                // 37-LEG-1), so nothing here has to be swept up on a deadline.
                _ = shared.disconnect.notified() => {
                    release(&lock, id);
                    holding = None;
                    continue;
                }
            }
        } else {
            rx.recv().await
        };
        let Some(Inbound { epoch, bytes }) = msg else {
            break; // source closed (torn down)
        };
        let n = bytes.len() as u64;
        // The connection that delivered this chunk is already gone: it is outage-era
        // stale before it was ever considered, whether the pulse that ended it was
        // observable from here or not (§6, 37-LEG-1). The check leads the acquire so a
        // dead peer's backlog is never made to queue for a contended local floor
        // first, and takes the floor back if this task still holds it (§7.1).
        if purging && shared.disconnect_epoch.get() != epoch {
            stat.purged_on_reconnect.add(n);
            if let Some((old_lock, old_id)) = holding.take() {
                release(&old_lock, old_id);
            }
            continue;
        }
        if !ensure_acquired(&lock, id).await {
            // Endpoint torn down (or this origin cannot write). Same reasoning as the
            // send-error arm below: count and keep the task, because a later `connect`
            // can hand this channel a live local endpoint (§15.35).
            stat.discarded_targetward.add(n);
            holding = None;
            continue;
        }
        holding = Some((lock.clone(), id));
        // The peer vanished while this chunk queued for a contended local lock: it is
        // outage-era stale before it was ever written, so purge it here rather than
        // firing it from the floor we just took (§6, LEG-3).
        if purging && shared.disconnect_epoch.get() != epoch {
            stat.purged_on_reconnect.add(n);
            release(&lock, id);
            holding = None;
            continue;
        }
        // Race the (backpressured) local write against the peer's disconnect: a chunk
        // still waiting on a full local queue when the peer vanishes is exactly the
        // stale command §6 forbids delivering. `Sender::send` is cancel-safe — on the
        // disconnect arm the chunk was *not* handed to the graph — so the purge count
        // is exact. With purge-on-reconnect off, the operator asked for the backlog to
        // survive the outage, so the write is not raced at all.
        let sent = if purging {
            tokio::select! {
                r = tx.send(bytes) => Some(r),
                _ = shared.disconnect.notified() => None,
            }
        } else {
            Some(tx.send(bytes).await)
        };
        match sent {
            Some(Ok(())) => stat
                .accepted_targetward
                .set(stat.accepted_targetward.get() + n),
            Some(Err(_)) => {
                // The local endpoint's targetward channel closed under us — its node
                // was removed, or the graph was replaced. Count the loss and keep the
                // task: with edge surgery (§15.35) a `connect` may give this channel a
                // new local endpoint, and returning here would leave a leg channel
                // permanently dead with no way back short of a reload.
                stat.discarded_targetward.add(n);
                release(&lock, id);
                holding = None;
                continue;
            }
            None => {
                // The write was cancelled by the disconnect, and `Sender::send` is
                // cancel-safe: this chunk was *not* handed to the graph, so the purge
                // count is exact (§6, LEG-3).
                stat.purged_on_reconnect.add(n);
                release(&lock, id);
                holding = None;
                continue;
            }
        }
        // The chunk is delivered (no targetward drop, §5); if the peer vanished
        // while we acquired or wrote, yield the local floor now rather than holding
        // it behind a vanished remote until the idle interval (§7.1, LEG-4). The
        // outage-era backlog behind it is discarded per chunk as the loop reaches it.
        if shared.disconnect_epoch.get() != epoch {
            release(&lock, id);
            holding = None;
        }
    }
    if let Some((lock, id)) = holding {
        release(&lock, id);
    }
}

/// Acquire `id`'s on-demand write lock, joining the FIFO queue and suspending if
/// contended (§6, §15.20 two-lane). Returns false if the endpoint was torn down or
/// the origin cannot write. Holds no borrow across the await.
///
/// The fast path is a single synchronous borrow; the slow path is the shared
/// [`await_write_grant`] park, which owns the five-clause lost-wakeup discipline this
/// function used to hand-write (§15.20, SIMPB-2) — so all that is left here is the one
/// line inside the critical section.
async fn ensure_acquired(lock: &SharedLock, id: OriginId) -> bool {
    if lock.with(|g| g.may_write(id)) {
        return true;
    }
    await_write_grant(lock, |g| match g.acquire(id) {
        Acquire::Granted => Some(Grant::Fresh),
        Acquire::AlreadyHeld => Some(Grant::AlreadyHeld),
        // `write = never` (a claim about configuration) and an edge that went away
        // under this pump (`disconnect`/`remove-node`, §15.35) are distinct states
        // since CTRL-2 split them, and both are unwaitable: this channel cannot write.
        Acquire::ReadOnly | Acquire::Unregistered => Some(Grant::Refused),
        Acquire::Denied { .. } => {
            g.enqueue(id);
            None
        }
    })
    .await
}

/// Release `id`'s lock if held, waking the next queue head (§6).
fn release(lock: &SharedLock, id: OriginId) {
    let freed = lock.with_mut(|g| g.release(id));
    if freed {
        lock.wake_waiters();
        lock.emit_change();
    }
}

/// Poll every send receiver once (round-robin from `start` for basic fairness),
/// yielding the first available (channel, chunk). A `waiting` (unbound) channel is
/// skipped, not drained, so its bounded receiver fills and the writer backpressures
/// per faulted-and-wait (§7.1/§8) rather than sending bytes the unconfigured peer
/// would drop; a skipped channel counts as open (not closed). `Ready(None)` only
/// when every receiver is closed (all local producers gone) — binding is stable for
/// a pump's lifetime, so a skipped channel never needs its waker re-registered here.
fn next_send<'a>(
    receivers: &'a [SendReceiver],
    start: &'a mut usize,
) -> impl std::future::Future<Output = Option<(String, Chunk)>> + 'a {
    std::future::poll_fn(move |cx: &mut Context<'_>| {
        let n = receivers.len();
        if n == 0 {
            return Poll::Ready(None);
        }
        // Settle every inbox's mid-flight chunk before polling any of them (§15.50).
        // This is the multiplexed analogue of `TargetwardInbox::recv`'s clear-at-the-top:
        // the write half only re-enters `next_send` once the previous chunk has reached
        // the wire or been charged as an unframable residual, and that chunk may have
        // come from *any* of these inboxes. Clearing only the inbox we happen to poll
        // would leave a producer's `held` set for as long as its siblings kept the write
        // half busy, so a teardown would charge a chunk that went out minutes ago.
        for (_ch, rx, _stat) in receivers.iter() {
            rx.settle_held();
        }
        let mut all_closed = true;
        for k in 0..n {
            let i = (*start + k) % n;
            if !receivers[i].2.bound.get() {
                all_closed = false; // waiting: open but deliberately not drained
                continue;
            }
            match receivers[i].1.poll_recv(cx) {
                Poll::Ready(Some(v)) => {
                    *start = (i + 1) % n;
                    return Poll::Ready(Some((receivers[i].0.clone(), v)));
                }
                Poll::Ready(None) => {}
                Poll::Pending => all_closed = false,
            }
        }
        if all_closed {
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    })
}

/// Exchange hellos (§9): send ours, then read the peer's (bounded by
/// [`HANDSHAKE_TIMEOUT`]). Returns the peer's hello plus any leftover bytes already
/// read past it (the start of the envelope stream), or a refusal reason.
async fn handshake<S: DuplexStream>(
    stream: &mut S,
    channels: &[String],
    shared: &Rc<LegShared>,
) -> Result<(Hello, Vec<u8>), String> {
    let ours = Hello {
        version: WIRE_VERSION,
        capabilities: 0,
        channels: channels.iter().map(|c| c.as_str().into()).collect(),
    };
    let mut frame = Vec::new();
    encode_hello(&ours, &mut frame).map_err(|e| format!("encode hello: {e}"))?;
    if stream.write_all(&frame).await.is_err() {
        return Err("peer closed during handshake".to_owned());
    }

    let mut buf = Vec::new();
    let mut tmp = vec![0u8; 4096];
    loop {
        match try_decode_hello(&buf) {
            Ok(Some((hello, consumed))) => {
                shared.peer_version.set(Some(hello.version));
                shared.peer_capabilities.set(hello.capabilities);
                buf.drain(..consumed);
                return Ok((hello, buf));
            }
            Ok(None) => {}
            // §9 clause 6: a bad magic / unsupported version / malformed hello is a
            // clean refusal with the reason surfaced in leg state.
            Err(e) => return Err(e.to_string()),
        }
        // The whole handshake is bounded by an overall deadline at the call site, so
        // a plain read suffices here (a trickling peer trips the outer timeout).
        match stream.read(&mut tmp).await {
            Ok(0) => return Err("peer closed before sending a hello".to_owned()),
            Ok(k) => buf.extend_from_slice(&tmp[..k]),
            Err(e) => return Err(format!("read hello: {e}")),
        }
    }
}

/// Reconcile the peer's announcements against configured channels into
/// bound/waiting/unbound (§8). Never grows the graph.
fn bind_channels(
    channels: &[String],
    hello: &Hello,
    stats: &Rc<HashMap<String, Rc<ChannelStat>>>,
    shared: &Rc<LegShared>,
) {
    let announced: std::collections::HashSet<&str> =
        hello.channels.iter().map(|c| c.as_str()).collect();
    for ch in channels {
        if let Some(stat) = stats.get(ch) {
            stat.bound.set(announced.contains(ch.as_str()));
        }
    }
    let configured: std::collections::HashSet<&str> = channels.iter().map(String::as_str).collect();
    shared.unbound.with_mut(|unbound| {
        unbound.clear();
        for id in &hello.channels {
            if !configured.contains(id.as_str()) {
                // Through the same bounded insert as the data path: a hello is
                // peer-supplied too, and announces as many identities as it likes
                // (LEG-2).
                unbound.insert(id.as_str());
            }
        }
    });
}

/// Record a status transition. [`NodeState::set`] re-stamps only on a real change,
/// so a reconnect loop repeating the same fault reason keeps reporting the age of
/// the fault rather than the age of the last retry (§7, STATE-1).
fn set_status(shared: &Rc<LegShared>, status: NodeStatus) {
    shared.status.with_mut(|s| {
        s.set(status);
    });
}

/// Bind the `listen` role's socket.
///
/// The unix arm carries the leg's two filesystem obligations.
///
/// **Reclaiming** the address is conditional (SEC-8): the configured path is treated
/// as an operator file until it is proven to be a stale socket of the kind a previous
/// run left behind — see [`clear_stale_socket`].
///
/// **Narrowing** it is `apply_socket_perms`, the same one policy point the control
/// socket uses (§10) — SEC-2: `bind(2)` creates the inode `0o777 & !umask`, which was
/// observed as `srwxrwxr-x`, and the v1 wire has no authentication of its own, so
/// every local user could dial the leg and write into its consoles. There is no group
/// widen because `--socket-group` has no plumbing to a leg node: a leg is 0600, full
/// stop.
///
/// The chmod lands *after* the bind, which leaves a window — a racer who knows the
/// path and connects in the microseconds before it applies lands in the backlog and
/// is accepted later. The alternative, a umask-guarded bind, closes that window and
/// opens a worse one: `umask` is **process-global**, so a file or directory another
/// thread creates inside the guard comes out narrowed too — and a directory created
/// without its execute bits is not "tighter", it is broken. That was measured here,
/// not theorized: under the guard a concurrently-created directory came out 0600 and
/// the next bind inside it failed `EACCES`. The daemon has file-creating blocking
/// threads (§15.19), so this is the same trade the control socket already made, with
/// the same mitigation: the window is bounded by two adjacent synchronous syscalls,
/// with no `.await` between them and the accept loop not yet running. Closing it
/// properly wants `SO_PEERCRED` at accept, which is a policy addition rather than a
/// permission fix.
async fn bind_listener(transport: Transport, address: &str) -> std::io::Result<Listener> {
    match transport {
        Transport::Tcp => Ok(Listener::Tcp(TcpListener::bind(address).await?)),
        Transport::Unix => {
            clear_stale_socket(address).await?;
            let listener = UnixListener::bind(address)?;
            crate::apply_socket_perms(Path::new(address), None).map_err(std::io::Error::other)?;
            Ok(Listener::Unix(listener))
        }
    }
}

/// Make `address` bindable, or refuse — the `listen`+`unix` reclaim rule (SEC-8).
///
/// The former code unconditionally unlinked the configured address before binding,
/// so a leg configured with `address = "/home/me/notes.txt"` deleted that file. Two
/// conditions now gate the unlink, and both must hold:
///
/// 1. **It is a socket.** Checked with `symlink_metadata`, so a symlink is refused
///    as itself rather than followed to whatever it names.
/// 2. **Nobody is listening on it.** A stale socket refuses a dial immediately
///    (`ECONNREFUSED`); a successful dial means another daemon owns this address, and
///    unlinking it would silently steal the name while leaving that daemon accepting
///    on an unreachable inode. An answer that is neither prompt nor a refusal is read
///    as live, because that is the reading that fails safe.
///
/// Refusal is an error, which the supervisor turns into a faulted node with the
/// reason in state — the §15.8 shape: an environmental problem changes state, never
/// the graph.
async fn clear_stale_socket(address: &str) -> std::io::Result<()> {
    let metadata = match std::fs::symlink_metadata(address) {
        Ok(md) => md,
        // Nothing there: the ordinary first-bind path.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if !metadata.file_type().is_socket() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "exists and is not a unix socket; refusing to unlink it",
        ));
    }
    match tokio::time::timeout(PEER_PROBE_TIMEOUT, UnixStream::connect(address)).await {
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
            std::fs::remove_file(address)
        }
        Ok(Ok(_live_peer)) => Err(std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            "a peer is already listening there; refusing to take its address",
        )),
        Ok(Err(e)) => Err(std::io::Error::new(
            e.kind(),
            format!("probing the existing socket: {e}; refusing to unlink it"),
        )),
        Err(_) => Err(std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            "the existing socket did not refuse a dial promptly; refusing to unlink it",
        )),
    }
}

async fn connect_stream(
    transport: Transport,
    address: &str,
) -> std::io::Result<Box<dyn DuplexStream>> {
    match transport {
        Transport::Tcp => {
            let s = TcpStream::connect(address).await?;
            let _ = s.set_nodelay(true);
            Ok(Box::new(s))
        }
        Transport::Unix => Ok(Box::new(UnixStream::connect(address).await?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A fresh, unique temp directory per call (tests may run in parallel). Kept
    /// short: a Unix socket path is bounded at ~108 bytes (`SUN_LEN`).
    fn unique_dir(tag: &str) -> PathBuf {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("snx-{tag}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn shared() -> Rc<LegShared> {
        Rc::new(LegShared::new(true))
    }

    fn stats_for(channels: &[&str]) -> Rc<HashMap<String, Rc<ChannelStat>>> {
        Rc::new(
            channels
                .iter()
                .map(|c| ((*c).to_owned(), Rc::new(ChannelStat::default())))
                .collect(),
        )
    }

    fn leg_config(name: &str, address: &str) -> NodeConfig {
        toml::from_str(&format!(
            r#"
            type = "leg"
            name = "{name}"
            faces = "host"
            transport = "unix"
            role = "listen"
            address = "{address}"
            channels = ["console"]
            "#
        ))
        .unwrap()
    }

    /// The same leg the other way round: a `faces = target` leg's channel endpoints
    /// are target-facing, so they are the ones that carry a [`DropCounters`] handle.
    fn target_leg_config(name: &str, address: &str) -> NodeConfig {
        toml::from_str(&format!(
            r#"
            type = "leg"
            name = "{name}"
            faces = "target"
            transport = "unix"
            role = "listen"
            address = "{address}"
            channels = ["console"]
            "#
        ))
        .unwrap()
    }

    // LEG-2: an uncapped `Vec` let a peer streaming frames with fresh channel ids
    // grow the leg's memory without limit. The cap holds, the earliest identities
    // (the ones an operator is most likely acting on) are the ones kept, and the
    // refusals are counted rather than silent.
    #[test]
    fn unbound_identities_are_capped_and_the_overflow_counted() {
        let mut set = UnboundSet::default();
        for i in 0..MAX_UNBOUND + 50 {
            set.insert(&format!("ch-{i}"));
        }
        assert_eq!(set.order.len(), MAX_UNBOUND);
        assert_eq!(set.seen.len(), MAX_UNBOUND);
        assert_eq!(set.overflow, 50);
        // Insertion order is stable, so two `state` snapshots diff meaningfully.
        assert_eq!(set.order[0], "ch-0");
        assert_eq!(
            set.order[MAX_UNBOUND - 1],
            format!("ch-{}", MAX_UNBOUND - 1)
        );
    }

    // A repeat of an identity already recorded is not an overflow — the counter
    // reports refusals to *record*, so a peer hammering one unconfigured channel
    // does not look like a peer inventing identities.
    #[test]
    fn a_repeated_unbound_identity_is_neither_duplicated_nor_counted() {
        let mut set = UnboundSet::default();
        for _ in 0..1000 {
            set.insert("ch-a");
        }
        assert_eq!(set.order, vec!["ch-a".to_owned()]);
        assert_eq!(set.overflow, 0);

        // …including once the cap is reached.
        for i in 0..MAX_UNBOUND * 2 {
            set.insert(&format!("ch-{i}"));
        }
        let after = set.overflow;
        set.insert("ch-a");
        assert_eq!(
            set.overflow, after,
            "a recorded identity is never an overflow"
        );
    }

    // LEG-2: the wire admits identities far longer than any operator writes, and
    // `state` must not imply the peer sent the shortened name.
    #[test]
    fn a_long_unbound_identity_is_truncated_with_a_marker() {
        let long = "z".repeat(MAX_UNBOUND_ID_LEN * 4);
        let mut set = UnboundSet::default();
        set.insert(&long);
        assert_eq!(set.order.len(), 1);
        let stored = &set.order[0];
        assert!(stored.ends_with(TRUNCATION_MARKER), "{stored}");
        assert_eq!(
            stored.len(),
            MAX_UNBOUND_ID_LEN + TRUNCATION_MARKER.len(),
            "the stored identity is bounded, not merely marked"
        );
        // Two distinct over-long identities sharing a prefix collapse to one entry —
        // acceptable: the list prompts an operator, it is not a channel registry.
        set.insert(&format!("{long}-and-more"));
        assert_eq!(set.order.len(), 1);
    }

    // Truncation must land on a `char` boundary: the identity is peer-supplied UTF-8
    // and a split code point would not survive JSON.
    #[test]
    fn truncation_lands_on_a_char_boundary() {
        // 3 bytes each, so the cap falls mid-character for some prefix lengths.
        for pad in 0..4 {
            let id = format!("{}{}", "a".repeat(pad), "€".repeat(MAX_UNBOUND_ID_LEN));
            let stored = truncate_identity(&id).into_owned();
            assert!(stored.ends_with(TRUNCATION_MARKER));
            // The mere fact that it is a `String` proves boundary-correctness; assert
            // the body round-trips as the prefix it claims to be.
            let body = stored.strip_suffix(TRUNCATION_MARKER).unwrap();
            assert!(id.starts_with(body), "{body} is not a prefix of the input");
            assert!(body.len() <= MAX_UNBOUND_ID_LEN);
        }
    }

    // A short identity is borrowed untouched — the common case allocates nothing.
    #[test]
    fn a_short_identity_is_not_rewritten() {
        assert!(matches!(truncate_identity("console"), Cow::Borrowed(_)));
    }

    // LEG-4: wire data for a configured channel with no writable local edge was
    // charged to `discarded_hostward` regardless of which way it was travelling. The
    // loss was real; the direction was a lie.
    #[test]
    fn undeliverable_data_is_charged_to_its_own_direction() {
        let shared = shared();
        let stats = stats_for(&["console"]);
        let stat = stats.get("console").cloned();

        note_undeliverable(
            "console",
            stat.as_ref(),
            7,
            &shared,
            Undeliverable::Hostward,
        );
        note_undeliverable(
            "console",
            stat.as_ref(),
            11,
            &shared,
            Undeliverable::Targetward,
        );

        let stat = stat.unwrap();
        assert_eq!(stat.discarded_hostward.get(), 7);
        assert_eq!(stat.discarded_targetward.get(), 11);
        // A configured channel is never "unbound", whichever way its bytes went.
        assert_eq!(shared.unbound.with(|u| u.order.len()), 0);
    }

    // The unconfigured-identity arm records state instead of a counter (§8:
    // announcements never grow the graph) — through the bounded insert.
    #[test]
    fn an_unconfigured_identity_becomes_bounded_unbound_state() {
        let shared = shared();
        for i in 0..MAX_UNBOUND + 3 {
            note_undeliverable(
                &format!("ch-{i}"),
                None,
                4,
                &shared,
                Undeliverable::Hostward,
            );
        }
        shared.unbound.with(|u| {
            assert_eq!(u.order.len(), MAX_UNBOUND);
            assert_eq!(u.overflow, 3);
        });
    }

    // LEG-5: `protocol_version` and `capabilities` outlived the connection that
    // established them, so a peerless leg reported a live handshake. Everything that
    // describes *that peer* goes when the peer does.
    #[test]
    fn a_dropped_connection_clears_the_handshake_state() {
        let shared = shared();
        let stats = stats_for(&["console"]);
        // Simulate a connected, reconciled session.
        shared.peer_version.set(Some(WIRE_VERSION));
        shared.peer_capabilities.set(0b1011);
        shared
            .peer_address
            .with_mut(|p| *p = Some("unix".to_owned()));
        shared.unbound.with_mut(|u| {
            u.insert("ch-they-offered");
            u.overflow = 9;
        });
        stats["console"].bound.set(true);
        stats["console"].active.set(true);
        let epoch = shared.disconnect_epoch.get();

        clear_connection_state(&stats, &shared);

        assert_eq!(shared.peer_version.get(), None);
        assert_eq!(shared.peer_capabilities.get(), 0);
        assert_eq!(shared.peer_address.with(|p| p.clone()), None);
        shared.unbound.with(|u| {
            assert!(u.order.is_empty());
            assert!(u.seen.is_empty());
            assert_eq!(u.overflow, 0);
        });
        assert!(!stats["console"].bound.get());
        assert!(!stats["console"].active.get());
        // The epoch bump is what a channel task blocked in a local write reads
        // (§7.1, LEG-4) — and now also its cue to purge (LEG-3).
        assert_eq!(shared.disconnect_epoch.get(), epoch + 1);
    }

    // STATE-1: §7 wants "a status of active | waiting | faulted with reason and
    // timestamp", and the stamp must age with the *condition*, not with the retry
    // loop that keeps re-reporting it.
    #[test]
    fn a_repeated_fault_reason_does_not_restamp_the_status() {
        let shared = shared();
        set_status(
            &shared,
            NodeStatus::Faulted {
                reason: "connect: refused".to_owned(),
            },
        );
        let first = shared.status.with(NodeState::since_unix_ms);
        set_status(
            &shared,
            NodeStatus::Faulted {
                reason: "connect: refused".to_owned(),
            },
        );
        assert_eq!(shared.status.with(NodeState::since_unix_ms), first);
        assert_eq!(shared.status.with(connection_str), "faulted");
        set_status(&shared, NodeStatus::Active);
        assert_eq!(shared.status.with(connection_str), "connected");
    }

    // The new counters and the overflow are reachable in `state`, which is where an
    // operator (and every itest) reads them — a counter nobody can see is not one.
    #[test]
    fn state_extra_surfaces_the_new_counters() {
        let node = LegNode::create(&leg_config("up", "/tmp/never-bound.sock"));
        node.stats["console"].discarded_targetward.set(3);
        node.stats["console"].discarded_unframable.set(5);
        node.shared.unbound.with_mut(|u| u.overflow = 12);

        let state = node.state_extra();
        assert_eq!(state["unbound_overflow"], json!(12));
        let ch = &state["channels"]["console"];
        assert_eq!(ch["discarded_targetward"], json!(3));
        assert_eq!(ch["discarded_unframable"], json!(5));
        // A leg with no peer reports no handshake (LEG-5's steady state).
        assert_eq!(state["protocol_version"], Value::Null);
    }

    // LEG-1: a `faces = target` leg's `start` did `let _ = wiring.target_counters
    // .remove(&addr)`, so the one handle recording what the upstream producer sheds
    // at this leg's intake was never reported. Measured: ~50 MB shed with every
    // counter in `state` reading zero, beside a `log` on the same producer that
    // accounted for its stream to the byte.
    #[test]
    fn a_target_facing_channel_reports_what_it_sheds_at_its_intake() {
        let mut node = LegNode::create(&target_leg_config("up", "/tmp/never-bound.sock"));
        // A leg that never claimed a counter reports zero rather than nothing — the
        // shape `faces = host` legs keep, whose channel endpoints are host-facing.
        assert_eq!(
            node.state_extra()["channels"]["console"]["dropped_slow_consumer"],
            json!(0)
        );

        // The handle `Wiring::build` hands every target-facing endpoint, and the
        // producing node's `AttachedSink` charges through `runtime::fan_out`.
        let counters = Arc::new(DropCounters::default());
        node.channel_counters
            .insert("console".to_owned(), counters.clone());
        counters.add_full(4096);

        let state = node.state_extra();
        assert_eq!(
            state["channels"]["console"]["dropped_slow_consumer"],
            json!(4096),
            "the intake drop must be visible in state: {state}"
        );
    }

    // LEG-2: the write half pops a chunk off its bounded receiver before framing it,
    // and the peer can take it away by either of two exits — `write_all` failing, or
    // `select!` dropping the suspended write future when the read half returns first.
    // The custody slot is what makes the untransmitted tail charge-able after the
    // race resolves, so it has to survive an over-discharge and an empty slot
    // without inventing bytes.
    #[test]
    fn the_in_flight_slot_holds_the_exact_untransmitted_tail() {
        let slot: Cell<Option<InFlight>> = Cell::new(None);
        // Nothing in custody: a no-op, never a phantom charge.
        discharge(&slot, 10);
        assert!(slot.take().is_none());

        slot.set(Some(InFlight {
            channel: "c0".to_owned(),
            remaining: 100,
        }));
        discharge(&slot, 30);
        discharge(&slot, 30);
        let f = slot.take().expect("still in custody");
        assert_eq!(f.remaining, 40, "the tail is what never reached the wire");
        assert_eq!(f.channel, "c0");

        // Saturating, so a miscount can never become a 2^64-byte loss report.
        slot.set(Some(InFlight {
            channel: "c0".to_owned(),
            remaining: 5,
        }));
        discharge(&slot, 9);
        assert_eq!(slot.take().expect("in custody").remaining, 0);
    }

    // LEG-2's counter is its own, not a fold into `discarded_unframable` — that one
    // means "this channel identity is pathological" and must stay readable as such.
    #[test]
    fn state_extra_reports_the_peer_gone_residual_separately() {
        let node = LegNode::create(&leg_config("up", "/tmp/never-bound.sock"));
        node.stats["console"].discarded_peer_gone.set(60_001);
        let ch = &node.state_extra()["channels"]["console"];
        assert_eq!(ch["discarded_peer_gone"], json!(60_001));
        assert_eq!(ch["discarded_unframable"], json!(0));
    }

    /// The number of attempts past which the injected accept stops answering, and the
    /// ceiling the paced loop must stay under. One constant for both jobs on purpose —
    /// see [`a_persistently_failing_accept_backs_off_instead_of_spinning`].
    const ACCEPT_CAP: u64 = 40;

    // LEG-3: an `Err` from `accept(2)` is not a readiness event — under EMFILE it
    // returns immediately — so the former `if let Ok(…)` shape re-called it at once,
    // pinning the single runtime thread at ~54,000 accepts per second and logging
    // nothing at all. The loop must be paced by a timer, not by the CPU.
    //
    // **Why the injected accept parks after `ACCEPT_CAP`, and why the clock is paused.**
    // The first version of this guard returned a bare `ready(Err(…))` and leaned on
    // `timeout` alone, and against the unfixed loop it did not *report* the defect: an
    // immediately-ready future is not a yield point and consumes no tokio coop budget,
    // so the spinning task never returned to the scheduler, the timeout future was never
    // polled, and the test hung instead of failing its ceiling. A guard that wedges CI
    // stops a regression but diagnoses nothing (review-32 audit).
    //
    // Parking the accept at the ceiling is what closes that: the unpaced loop reaches
    // attempt 41 in microseconds and then has to yield, the timeout fires, and the
    // assertion below prints the count it actually reached. Proved fail-first — against
    // the `if let Ok(…)` shape this test now reports "41 accepts in 300ms" in a fifth of
    // a second, where the old one hung.
    //
    // The clock is real rather than `start_paused`, which would need tokio's `test-util`
    // feature; nothing here depends on virtual time, because the assertion that matters
    // is an *upper* bound — a loaded runner completes fewer sleeps, never more.
    #[tokio::test]
    async fn a_persistently_failing_accept_backs_off_instead_of_spinning() {
        let calls = Rc::new(Cell::new(0u64));
        let counter = calls.clone();
        let arm = reject_extra_peers(move || {
            let counter = counter.clone();
            async move {
                let n = counter.get() + 1;
                counter.set(n);
                if n > ACCEPT_CAP {
                    // Nothing above this line ever awaits, so a paced caller sees the
                    // same immediately-failing accept a real EMFILE gives it.
                    std::future::pending::<()>().await;
                }
                Err::<(), std::io::Error>(std::io::Error::other("EMFILE"))
            }
        });
        // The arm never returns by design (§16.1 park-don't-teardown), so bound it.
        let _ = tokio::time::timeout(Duration::from_millis(300), arm).await;
        let n = calls.get();
        assert!(n >= 2, "the loop stopped retrying after {n} attempts");
        assert!(
            n <= ACCEPT_CAP,
            "the reject arm is spinning rather than backing off: {n} accepts in 300ms \
             (the injected accept stops answering past {ACCEPT_CAP}, so this is a floor \
             on the real count, not the whole of it)"
        );
    }

    // LEGD-2: `fan_out` reports `live = true` for a sink whose bounded buffer is
    // *full* (deliberately — it is still live), so crediting the whole chunk on
    // `live` counted bytes no consumer ever received. With one slow consumer the leg
    // reported the same stream as both delivered and discarded at once.
    //
    // Single-sink deliberately: that is the shape in which the two counters must
    // partition the stream exactly, and the shape an operator reconciles. The mixed
    // fan-out (one sink Ok, one Full) — where the first correction, `n -
    // dropped_full`, then credited zero — is pinned in `runtime::tests`, because the
    // arithmetic is one function shared by all three producers.
    #[tokio::test]
    async fn a_chunk_no_sink_accepted_is_not_credited_as_delivered() {
        let shared = shared();
        let stats = stats_for(&["c0"]);
        let fanout = crate::runtime::FanOutList::new();
        // Depth 1: the first chunk is taken, every one after it finds the sink full.
        let (tx, _rx) = mpsc::channel::<Chunk>(1);
        fanout.attach(crate::runtime::AttachedSink {
            target: EndpointAddr::node("consumer"),
            tx,
            counters: Arc::new(DropCounters::default()),
        });
        let route = RecvRoute::Host {
            sinks: HashMap::from([("c0".to_owned(), fanout)]),
            feeds: HashMap::new(),
        };

        for _ in 0..3 {
            route_recv(
                Event::data("c0", Chunk::from_static(&[7u8; 8])),
                &route,
                &stats,
                &shared,
            )
            .await;
        }

        let s = &stats["c0"];
        assert_eq!(
            s.delivered_hostward.get(),
            8,
            "only the chunk a sink actually took is a delivery"
        );
        assert_eq!(s.discarded_hostward.get(), 16);
        // §5, stated as arithmetic: with one sink the two counters partition the
        // stream exactly, so an operator can reconcile them against the consumer.
        assert_eq!(s.delivered_hostward.get() + s.discarded_hostward.get(), 24);
    }

    // SEC-2: the v1 wire has no authentication (§9), so the path is the whole gate —
    // it must be no wider than the control socket's 0600 (§10).
    #[tokio::test]
    async fn a_listen_unix_leg_binds_its_socket_0600() {
        let dir = unique_dir("legsec2");
        let path = dir.join("leg.sock");
        let address = path.to_str().unwrap();

        let listener = bind_listener(Transport::Unix, address).await.unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "observed {mode:o}");
        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // SEC-8: the configured address is an operator file until proven to be a stale
    // socket. A leg pointed at `notes.txt` used to delete it.
    #[tokio::test]
    async fn a_non_socket_address_is_refused_and_left_alone() {
        let dir = unique_dir("legsec8a");
        let path = dir.join("notes.txt");
        std::fs::write(&path, b"important").unwrap();
        let address = path.to_str().unwrap();

        let err = bind_listener(Transport::Unix, address).await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&path).unwrap(), b"important");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A symlink is refused *as itself* — `symlink_metadata`, not `metadata`, so the
    // check cannot be talked into following a link to something valuable.
    #[tokio::test]
    async fn a_symlink_address_is_refused_without_being_followed() {
        let dir = unique_dir("legsec8b");
        let target = dir.join("target.sock");
        let link = dir.join("link.sock");
        // Even when the link points at a real, dead socket: the address itself is
        // not a socket, so it is not ours to unlink.
        let listener = tokio::net::UnixListener::bind(&target).unwrap();
        drop(listener);
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let err = bind_listener(Transport::Unix, link.to_str().unwrap())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert!(std::fs::symlink_metadata(&link).is_ok(), "link survives");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // SEC-8: a socket somebody is listening on belongs to that somebody. Unlinking
    // it would take the name and leave them accepting on an unreachable inode.
    #[tokio::test]
    async fn a_live_socket_is_not_stolen() {
        let dir = unique_dir("legsec8c");
        let path = dir.join("leg.sock");
        let address = path.to_str().unwrap();
        let incumbent = tokio::net::UnixListener::bind(&path).unwrap();

        let err = bind_listener(Transport::Unix, address).await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);
        // The incumbent is still reachable at its address.
        assert!(tokio::net::UnixStream::connect(&path).await.is_ok());
        drop(incumbent);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // …and the stale-socket case the original unconditional unlink existed to
    // handle still works: a socket nobody listens on refuses a dial, and is cleared.
    #[tokio::test]
    async fn a_stale_socket_is_still_reclaimed() {
        let dir = unique_dir("legsec8d");
        let path = dir.join("leg.sock");
        let address = path.to_str().unwrap();
        drop(tokio::net::UnixListener::bind(&path).unwrap());
        assert!(path.exists(), "neither std nor tokio unlinks on drop");

        let listener = bind_listener(Transport::Unix, address).await.unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // SEC-8, teardown half: the unlink is keyed on what this node actually bound,
    // not on what it was configured with — so a leg that never bound (or that was
    // refused) removes nothing.
    #[test]
    fn teardown_unlinks_only_a_socket_this_node_bound() {
        let dir = unique_dir("legsec8e");
        let path = dir.join("notes.txt");
        std::fs::write(&path, b"important").unwrap();

        let mut node = LegNode::create(&leg_config("up", path.to_str().unwrap()));
        node.teardown();
        assert_eq!(std::fs::read(&path).unwrap(), b"important");

        // Once a bind is recorded, teardown does remove it — exactly once.
        std::fs::write(&path, b"pretend-socket").unwrap();
        let recorded = path.to_str().unwrap().to_owned();
        node.shared
            .bound_unix_path
            .with_mut(|p| *p = Some(recorded.clone()));
        node.teardown();
        assert!(!path.exists());
        std::fs::write(&path, b"recreated by someone else").unwrap();
        node.teardown();
        assert!(path.exists(), "the record is taken, so the unlink is once");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
