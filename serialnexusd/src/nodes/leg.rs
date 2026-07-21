//! Leg node (design §7.4): the cross-daemon transport. A socket (tcp|unix)
//! carrying all of its channels multiplexed by the built-in **link codec** — the
//! shared envelope frame format (`codec-api`), opened by a `hello` frame (§9).
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
//! halves run as **concurrently-polled** futures in one `select!`, so a
//! backpressured targetward write never starves the hostward read half. Every task
//! is aborted on teardown and Drop; a `RefCell` borrow never crosses an `.await`
//! (§15.20).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::task::{Context, Poll};
use std::time::Duration;

use codec_api::{
    Event, EventKind, FrameDecoder, Hello, MAX_FRAME_SIZE, WIRE_VERSION, encode, encode_hello,
    try_decode_hello,
};
use nexus_core::config::{LegRole, NodeConfig, Transport};
use nexus_core::graph::{EndpointAddr, Facing};
use nexus_core::lock::{Acquire, OriginId};
use nexus_core::{Chunk, NodeStatus};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UnixListener, UnixStream};
use tokio::sync::Notify;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::task::JoinHandle;

use crate::runtime::{CHANNEL_CAP, HostwardSink, READ_BUF, SharedLock, Wiring};

/// How long to wait for the peer's hello before treating the connection as dead.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// A boxed duplex byte stream, abstracting over tcp and unix sockets so the pump
/// is transport-agnostic. Tasks run on the single-threaded `LocalSet`, so no
/// `Send` bound is needed.
trait DuplexStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin> DuplexStream for T {}

/// A bound listener for the `listen` role.
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
    /// full (faces=host) — a §5 loss counted where it happens.
    discarded_hostward: Cell<u64>,
    /// Targetward bytes discarded on reconnect because they were outage-era stale
    /// (§7.4 purge-on-reconnect).
    purged_on_reconnect: Cell<u64>,
    /// Whether the peer announced this configured channel (`bound`), else `waiting`.
    bound: Cell<bool>,
    /// Whether any data has crossed the channel since connect.
    active: Cell<bool>,
}

/// Node-level observed state shared with the supervisor task (which flips it as
/// the connection comes and goes).
struct LegShared {
    status: RefCell<NodeStatus>,
    peer_address: RefCell<Option<String>>,
    peer_version: Cell<Option<u16>>,
    peer_capabilities: Cell<u32>,
    reconnect_count: Cell<u64>,
    /// Peer-announced identities this configuration does not declare — visible
    /// state awaiting an operator, never an endpoint (§8).
    unbound: RefCell<Vec<String>>,
    /// Pulsed by the supervisor when a connection drops, so each faces=target
    /// channel task promptly releases its on-demand write lock (§7.1: release on
    /// idle *or* peer disconnect), rather than holding the local floor until idle.
    disconnect: Notify,
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
    purge_on_reconnect: bool,
    channels: Vec<String>,
    stats: Rc<HashMap<String, Rc<ChannelStat>>>,
    shared: Rc<LegShared>,
    tasks: Vec<JoinHandle<()>>,
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
            purge_on_reconnect: *purge_on_reconnect,
            channels: channels.clone(),
            stats: Rc::new(stats),
            shared: Rc::new(LegShared {
                status: RefCell::new(NodeStatus::Waiting {
                    reason: "no peer connected yet".to_owned(),
                }),
                peer_address: RefCell::new(None),
                peer_version: Cell::new(None),
                peer_capabilities: Cell::new(0),
                reconnect_count: Cell::new(0),
                unbound: RefCell::new(Vec::new()),
                disconnect: Notify::new(),
            }),
            tasks: Vec::new(),
        }
    }

    /// Claim this leg's per-channel endpoints out of the endpoint-keyed wiring and
    /// start the supervisor (§7.4). A `faces = host` leg claims each channel's
    /// host-facing maps (fan-out sinks + the arbitrated targetward receiver); a
    /// `faces = target` leg claims each channel's target-facing maps (the local
    /// hostward stream + a targetward sender and lock into the local graph).
    pub fn start(&mut self, wiring: &mut Wiring) {
        // The socket send source: the per-channel receivers the pump multiplexes
        // onto the wire. faces=host: the arbitrated targetward stream (host writers
        // → wire). faces=target: the local hostward stream (device → wire).
        let mut send_receivers: Vec<SendReceiver> = Vec::new();
        let stat_for = |ch: &str| self.stats.get(ch).cloned().unwrap_or_default();
        // How the pump routes decoded wire events back into the local graph.
        let recv_route: RecvRoute = match self.faces {
            Facing::Host => {
                let mut sinks: HashMap<String, Vec<HostwardSink>> = HashMap::new();
                for ch in &self.channels {
                    let addr = EndpointAddr::channel(&self.name, ch);
                    if let Some(s) = wiring.host_sinks.remove(&addr) {
                        sinks.insert(ch.clone(), s);
                    }
                    if let Some(rx) = wiring.host_targetward_rx.remove(&addr) {
                        send_receivers.push((ch.clone(), rx, stat_for(ch)));
                    }
                }
                RecvRoute::Host(sinks)
            }
            Facing::Target => {
                let mut inbound_txs: HashMap<String, mpsc::Sender<Chunk>> = HashMap::new();
                for ch in &self.channels {
                    let addr = EndpointAddr::channel(&self.name, ch);
                    if let Some(rx) = wiring.target_hostward_rx.remove(&addr) {
                        send_receivers.push((ch.clone(), rx, stat_for(ch)));
                    }
                    let _ = wiring.target_counters.remove(&addr);
                    // Targetward into the local graph, gated on this leg's on-demand
                    // origin lock. One task per channel does the acquire, the
                    // idle-release, and the (backpressured) write; the pump feeds it
                    // through a bounded per-channel queue so a stalled channel
                    // backpressures the whole connection (§9 head-of-line).
                    let target_tx = wiring.target_targetward_tx.remove(&addr);
                    let origin = wiring.origin_locks.remove(&addr);
                    if let (Some(target_tx), Some((lock, id))) = (target_tx, origin) {
                        let (inbound_tx, inbound_rx) = mpsc::channel::<Chunk>(CHANNEL_CAP);
                        inbound_txs.insert(ch.clone(), inbound_tx);
                        let stat = self.stats.get(ch).cloned().unwrap_or_default();
                        let idle = Duration::from_millis(self.idle_release_ms);
                        self.tasks.push(tokio::task::spawn_local(channel_targetward(
                            inbound_rx,
                            target_tx,
                            lock,
                            id,
                            idle,
                            stat,
                            self.shared.clone(),
                        )));
                    }
                }
                RecvRoute::Target(inbound_txs)
            }
        };

        self.tasks
            .push(tokio::task::spawn_local(supervise(SuperviseArgs {
                faces: self.faces,
                transport: self.transport,
                role: self.role,
                address: self.address.clone(),
                reconnect_initial_ms: self.reconnect_initial_ms,
                reconnect_max_ms: self.reconnect_max_ms,
                purge_on_reconnect: self.purge_on_reconnect,
                channels: self.channels.clone(),
                send_receivers,
                recv_route,
                stats: self.stats.clone(),
                shared: self.shared.clone(),
            })));
    }

    pub fn status(&self) -> NodeStatus {
        self.shared.status.borrow().clone()
    }

    pub fn state_extra(&self) -> Value {
        let channels: serde_json::Map<String, Value> = self
            .channels
            .iter()
            .map(|ch| {
                let stat = self.stats.get(ch);
                let bound = stat.is_some_and(|s| s.bound.get());
                let obj = json!({
                    "binding": if bound { "bound" } else { "waiting" },
                    "active": stat.is_some_and(|s| s.active.get()),
                    "delivered_hostward": stat.map_or(0, |s| s.delivered_hostward.get()),
                    "accepted_targetward": stat.map_or(0, |s| s.accepted_targetward.get()),
                    "discarded_hostward": stat.map_or(0, |s| s.discarded_hostward.get()),
                    "purged_on_reconnect": stat.map_or(0, |s| s.purged_on_reconnect.get()),
                });
                (ch.clone(), obj)
            })
            .collect();
        // Announced-but-unconfigured identities: visible state, no endpoint (§8).
        let mut channels = channels;
        for id in self.shared.unbound.borrow().iter() {
            channels.insert(id.clone(), json!({ "binding": "unbound" }));
        }
        let mut obj = json!({
            "role": role_str(self.role),
            "transport": transport_str(self.transport),
            "faces": self.faces.to_string(),
            "connection": connection_str(&self.shared.status.borrow()),
            "peer_address": *self.shared.peer_address.borrow(),
            "protocol_version": self.shared.peer_version.get(),
            "capabilities": self.shared.peer_capabilities.get(),
            "reconnect_count": self.shared.reconnect_count.get(),
            "channels": channels,
        });
        // The §9 named footgun: surface it as a visible, greppable confession in
        // `state` when a non-loopback bind was opted into (§15.12).
        if self.insecure_bind {
            obj["insecure_bind"] = json!(true);
        }
        obj
    }

    pub fn teardown(&mut self) {
        for t in self.tasks.drain(..) {
            t.abort();
        }
    }
}

impl Drop for LegNode {
    fn drop(&mut self) {
        for t in self.tasks.drain(..) {
            t.abort();
        }
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

fn connection_str(status: &NodeStatus) -> &'static str {
    match status {
        NodeStatus::Active => "connected",
        NodeStatus::Waiting { .. } => "waiting",
        NodeStatus::Faulted { .. } => "faulted",
    }
}

/// One socket-send source: a channel identity, its bounded receiver, and its
/// stat (for the `bound` gate — a `waiting` channel is not drained onto the wire,
/// so its writers backpressure per faulted-and-wait rather than have their bytes
/// dropped at the unconfigured peer).
type SendReceiver = (String, mpsc::Receiver<Chunk>, Rc<ChannelStat>);

/// How the pump routes a decoded wire event into the local graph.
enum RecvRoute {
    /// faces=host: fan each channel's hostward data out to local consumers.
    Host(HashMap<String, Vec<HostwardSink>>),
    /// faces=target: hand each channel's targetward data to its per-channel task.
    Target(HashMap<String, mpsc::Sender<Chunk>>),
}

struct SuperviseArgs {
    faces: Facing,
    transport: Transport,
    role: LegRole,
    address: String,
    reconnect_initial_ms: u64,
    reconnect_max_ms: u64,
    purge_on_reconnect: bool,
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
async fn supervise(mut a: SuperviseArgs) {
    // The listen role binds once and accepts successive peers; the connect role
    // dials with backoff.
    let listener = match a.role {
        LegRole::Listen => match bind_listener(a.transport, &a.address).await {
            Ok(l) => Some(l),
            Err(e) => {
                set_status(
                    &a.shared,
                    NodeStatus::Faulted {
                        reason: format!("bind {:?}: {e}", a.address),
                    },
                );
                return; // a bind failure does not self-heal
            }
        },
        LegRole::Connect => None,
    };

    let mut backoff = a.reconnect_initial_ms;
    let mut connected_before = false;

    loop {
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
                sleep_backoff(&mut backoff, a.reconnect_max_ms).await;
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
                if let Some(addr) = peer_addr {
                    *a.shared.peer_address.borrow_mut() = Some(addr);
                }
                set_status(&a.shared, NodeStatus::Active);
                backoff = a.reconnect_initial_ms; // a good connection resets backoff
                leftover
            }
            Ok(Err(reason)) => {
                set_status(&a.shared, NodeStatus::Faulted { reason });
                if a.role == LegRole::Connect {
                    sleep_backoff(&mut backoff, a.reconnect_max_ms).await;
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
                    sleep_backoff(&mut backoff, a.reconnect_max_ms).await;
                }
                continue;
            }
        };

        // Purge-on-reconnect: on a reconnect (not the first connection), the
        // targetward source's backlog is outage-era stale (§7.4). Only the
        // faces=host side carries a local targetward backlog to purge; the
        // faces=target targetward arrives from the wire, so there is none.
        if connected_before && a.purge_on_reconnect && a.faces == Facing::Host {
            for (_ch, rx, stat) in &mut a.send_receivers {
                let mut purged = 0u64;
                while let Ok(bytes) = rx.try_recv() {
                    purged += bytes.len() as u64;
                }
                if purged > 0 {
                    stat.purged_on_reconnect
                        .set(stat.purged_on_reconnect.get() + purged);
                }
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
            &mut a.send_receivers,
            send_is_hostward,
            &a.recv_route,
            &a.stats,
            &a.shared,
            listener.as_ref(),
        )
        .await;

        // The connection dropped. Clear per-connection binding state (the node parks
        // its channels until the next peer, faulted-and-wait) and pulse the
        // disconnect signal so every faces=target channel task releases its
        // on-demand write lock now rather than after the idle interval (§7.1).
        for stat in a.stats.values() {
            stat.bound.set(false);
        }
        a.shared.unbound.borrow_mut().clear();
        *a.shared.peer_address.borrow_mut() = None;
        a.shared.disconnect.notify_waiters();

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
            sleep_backoff(&mut backoff, a.reconnect_max_ms).await;
        }
    }
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
    send_receivers: &mut [SendReceiver],
    send_is_hostward: bool,
    recv_route: &RecvRoute,
    stats: &Rc<HashMap<String, Rc<ChannelStat>>>,
    shared: &Rc<LegShared>,
    listener: Option<&Listener>,
) -> PumpEnd {
    let mut send_start = 0usize;
    tokio::select! {
        // Write half: multiplex the send source onto the wire. A chunk larger than
        // a single frame is fragmented into consecutive Data frames on the same
        // channel (the peer reassembles transparently) — never dropped, since
        // READ_BUF == MAX_FRAME_SIZE means a full read always overflows the header,
        // and the `send` verb accepts arbitrary-length lines (§5 no-drop / all-loss-
        // counted, §9 clause 5).
        end = async {
            loop {
                match next_send(send_receivers, &mut send_start).await {
                    Some((ch, bytes)) => {
                        if let Some(stat) = stats.get(&ch) {
                            stat.active.set(true);
                        }
                        // Max payload per frame = MAX_FRAME_SIZE minus the envelope
                        // header (1 type + 2 channel-len + channel bytes).
                        let cap = MAX_FRAME_SIZE.saturating_sub(3 + ch.len()).max(1);
                        let total = bytes.len();
                        let mut off = 0;
                        while off < total {
                            let end = (off + cap).min(total);
                            let piece_len = (end - off) as u64;
                            let mut frame = Vec::new();
                            if encode(&Event::data(ch.as_str(), bytes.slice(off..end)), &mut frame)
                                .is_err()
                            {
                                break; // defensive; unreachable for a sane channel id
                            }
                            if write_half.write_all(&frame).await.is_err() {
                                return PumpEnd::PeerGone;
                            }
                            if let Some(stat) = stats.get(&ch) {
                                if send_is_hostward {
                                    stat.delivered_hostward
                                        .set(stat.delivered_hostward.get() + piece_len);
                                } else {
                                    stat.accepted_targetward
                                        .set(stat.accepted_targetward.get() + piece_len);
                                }
                            }
                            off = end;
                        }
                    }
                    // Every send source has closed (a faces=target leg whose local
                    // producers are all gone; a serial reopen is deferred to phase 7).
                    // Park the write half so the independent read/targetward direction
                    // stays alive (§15.22) — teardown aborts the task.
                    None => std::future::pending::<()>().await,
                }
            }
        } => end,
        // Read half: decode envelope frames and route them into the local graph.
        end = async {
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
        } => end,
        // Listen role: actively reject a concurrent second peer (§7.4). Never
        // completes, so the pump ends only via the write/read halves above.
        _ = async {
            match listener {
                Some(l) => loop {
                    if let Ok((extra, _)) = l.accept().await {
                        drop(extra); // close it immediately
                    }
                },
                None => std::future::pending::<()>().await,
            }
        } => PumpEnd::PeerGone,
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
                RecvRoute::Host(sinks) => match sinks.get(ch) {
                    Some(chsinks) => {
                        if let Some(s) = stat {
                            s.delivered_hostward.set(s.delivered_hostward.get() + n);
                        }
                        for (tx, counters) in chsinks {
                            match tx.try_send(bytes.clone()) {
                                Ok(()) => {}
                                Err(TrySendError::Full(_)) => {
                                    counters.add_full(n);
                                    if let Some(s) = stat {
                                        s.discarded_hostward.set(s.discarded_hostward.get() + n);
                                    }
                                }
                                Err(TrySendError::Closed(_)) => {}
                            }
                        }
                    }
                    None => note_unbound(ch, stat, n, shared),
                },
                RecvRoute::Target(txs) => match txs.get(ch) {
                    // The per-channel task counts accepted_targetward once the local
                    // device-write handoff accepts; here we just backpressure.
                    Some(tx) => {
                        let _ = tx.send(bytes).await;
                    }
                    None => note_unbound(ch, stat, n, shared),
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

/// Handle wire data for a channel with no local sink. A *configured* channel with
/// no consumer bound is a §5 boundary drop — counted (like the serial node's
/// discard-when-unattached). An *unconfigured* identity is `unbound` state — its
/// bytes are dropped (§8: announcements never grow the graph) and the identity is
/// surfaced for an operator.
fn note_unbound(ch: &str, stat: Option<&Rc<ChannelStat>>, n: u64, shared: &Rc<LegShared>) {
    if let Some(s) = stat {
        s.discarded_hostward.set(s.discarded_hostward.get() + n);
        return; // configured but unattached: dropped and counted, not "unbound"
    }
    let mut unbound = shared.unbound.borrow_mut();
    if !unbound.iter().any(|u| u == ch) {
        unbound.push(ch.to_owned());
    }
}

/// A faces=target channel's targetward task: hand each wire-arriving chunk into the
/// local graph, gated on this leg's on-demand origin lock (§6). Acquires implicitly
/// on data arrival and releases after `idle` *or* on peer disconnect (§7.1); the
/// framed chunk is backpressured (`send().await`), never dropped.
async fn channel_targetward(
    mut rx: mpsc::Receiver<Chunk>,
    tx: mpsc::Sender<Chunk>,
    lock: SharedLock,
    id: OriginId,
    idle: Duration,
    stat: Rc<ChannelStat>,
    shared: Rc<LegShared>,
) {
    let mut holding = false;
    loop {
        let msg = if holding {
            tokio::select! {
                v = rx.recv() => v,
                _ = tokio::time::sleep(idle) => {
                    release(&lock, id);
                    holding = false;
                    continue;
                }
                // The peer dropped: yield the local endpoint's floor now, so a local
                // operator is not blocked behind a vanished remote (§7.1).
                _ = shared.disconnect.notified() => {
                    release(&lock, id);
                    holding = false;
                    continue;
                }
            }
        } else {
            rx.recv().await
        };
        let Some(bytes) = msg else {
            break; // source closed (torn down)
        };
        let n = bytes.len() as u64;
        if !ensure_acquired(&lock, id).await {
            return; // endpoint torn down or we cannot write
        }
        holding = true;
        if tx.send(bytes).await.is_err() {
            release(&lock, id);
            return; // the local endpoint is gone
        }
        stat.accepted_targetward
            .set(stat.accepted_targetward.get() + n);
    }
    if holding {
        release(&lock, id);
    }
}

/// Acquire `id`'s on-demand write lock, joining the FIFO queue and suspending if
/// contended (§6, §15.20 two-lane). Returns false if the endpoint was torn down or
/// the origin cannot write. Holds no borrow across the await.
async fn ensure_acquired(lock: &SharedLock, id: OriginId) -> bool {
    if lock.borrow().may_write(id) {
        return true;
    }
    loop {
        if lock.is_closed() {
            return false;
        }
        let notified = lock.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let outcome = {
            let mut g = lock.borrow_mut();
            match g.acquire(id) {
                Acquire::Granted => Some(Some(true)), // freshly granted (emit)
                Acquire::AlreadyHeld => Some(Some(false)),
                Acquire::ReadOnly => Some(None), // cannot write
                Acquire::Denied { .. } => {
                    g.enqueue(id);
                    None
                }
            }
        };
        match outcome {
            Some(Some(fresh)) => {
                if fresh {
                    lock.emit_change();
                }
                return true;
            }
            Some(None) => return false,
            None => notified.await,
        }
    }
}

/// Release `id`'s lock if held, waking the next queue head (§6).
fn release(lock: &SharedLock, id: OriginId) {
    let freed = { lock.borrow_mut().release(id) };
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
    receivers: &'a mut [SendReceiver],
    start: &'a mut usize,
) -> impl std::future::Future<Output = Option<(String, Chunk)>> + 'a {
    std::future::poll_fn(move |cx: &mut Context<'_>| {
        let n = receivers.len();
        if n == 0 {
            return Poll::Ready(None);
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
    let mut unbound = shared.unbound.borrow_mut();
    unbound.clear();
    for id in &hello.channels {
        if !configured.contains(id.as_str()) {
            unbound.push(id.0.clone());
        }
    }
}

fn set_status(shared: &Rc<LegShared>, status: NodeStatus) {
    *shared.status.borrow_mut() = status;
}

async fn sleep_backoff(backoff: &mut u64, max: u64) {
    let this = (*backoff).max(1).min(max.max(1));
    tokio::time::sleep(Duration::from_millis(this)).await;
    *backoff = (this.saturating_mul(2)).min(max.max(1));
}

async fn bind_listener(transport: Transport, address: &str) -> std::io::Result<Listener> {
    match transport {
        Transport::Tcp => Ok(Listener::Tcp(TcpListener::bind(address).await?)),
        Transport::Unix => {
            // A stale socket from a previous run would block the bind; unlink it
            // first (the standard dance). A missing file is fine.
            let _ = std::fs::remove_file(address);
            Ok(Listener::Unix(UnixListener::bind(address)?))
        }
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
