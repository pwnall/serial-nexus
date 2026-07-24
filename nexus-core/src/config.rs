//! Configuration types — the operator-owned, round-trippable half of the strict
//! configuration/state split (design §15.8). Everything here is *desired* state
//! that survives `dump`→`load`; nothing observed lives in these types (that is
//! [`crate::state`]). The split is enforced mechanically: state fields simply do
//! not exist on configuration types.
//!
//! Phase 1 models the graph container and the first three boundary node kinds
//! (serial, pty, log). Later phases extend [`NodeConfig`] with codec, leg,
//! exec, and existing-terminal kinds; the format is designed to grow additively
//! (§15.16). Node kinds are internally tagged by `type` with inline fields, so
//! they serialize cleanly to TOML without `flatten`.

use serde::{Deserialize, Serialize};

use crate::graph::{
    Arbitration, DEFAULT_ENDPOINT, EdgeSpec, EndpointAddr, EndpointSpec, Facing, GraphModel,
    NodeShape, ValidationError, WriteMode,
};
use crate::map::Mapping;

/// The reserved local name of a map node's target-facing (unmapped, upstream-side)
/// endpoint (§7.8). A map's host-facing side is its *default* endpoint (addressed by
/// the bare node name, carrying the standard lock/fan-out/tap/ring machinery); its
/// target-facing side — the edge into the upstream endpoint whose bytes it maps — is
/// addressed as `node/raw`. A map has no channel list, so this reserved name cannot
/// collide with an operator-declared identity.
pub const MAP_RAW_ENDPOINT: &str = "raw";

/// A complete graph configuration: the exact shape `dump` emits and `load`
/// accepts (§11).
///
/// `Eq` is deliberately absent: a codec node's opaque attribute table
/// ([`toml::Table`]) may carry floats, which are only `PartialEq`. Config
/// equality (round-trip tests) needs only `PartialEq`; nothing keys a map on a
/// config, so the `Eq` marker was never load-bearing.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GraphConfig {
    #[serde(default, rename = "node", skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<NodeConfig>,
    #[serde(default, rename = "edge", skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<EdgeConfig>,
}

impl GraphConfig {
    /// Build the topological [`GraphModel`] this configuration describes, for
    /// structural validation (§4). Node shapes are derived from each kind and
    /// its attributes (e.g. `faces`).
    pub fn to_model(&self) -> GraphModel {
        let mut model = GraphModel::new();
        for node in &self.nodes {
            model.add_node(node.name().to_owned(), node.shape());
        }
        for edge in &self.edges {
            model.add_edge(EdgeSpec {
                a: edge.a.clone(),
                b: edge.b.clone(),
                write_mode: edge.write_mode,
            });
        }
        model
    }

    /// Full structural validation (§4, §11): duplicate node names (checked on the
    /// node *list*, before the model's shape map collapses them) plus the three
    /// graph rules and name checks. An empty vector means the graph is valid. This
    /// is what `load` runs — [`GraphModel::validate`] alone would miss a duplicate
    /// node name, since the model is keyed by name.
    pub fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for node in &self.nodes {
            if !seen.insert(node.name()) {
                errors.push(ValidationError::DuplicateNodeName {
                    node: node.name().to_owned(),
                });
            }
            // A zero-depth hostward buffer builds a rendezvous channel that drops
            // nearly all hostward output even for a fast, fully-present consumer
            // (§5, §7.1/§7.2): the fan-out / writer bridge can admit a chunk only in
            // the instant a consumer is blocked in recv. Give the bounded buffer a
            // floor of one chunk. Only serial and pty carry the tunable.
            if let NodeConfig::Serial {
                name,
                hostward_buffer,
                ..
            }
            | NodeConfig::Pty {
                name,
                hostward_buffer,
                ..
            } = node
                && *hostward_buffer == 0
            {
                errors.push(ValidationError::ZeroHostwardBuffer { node: name.clone() });
            }
            // Leg-specific config-level checks the shape/topology model cannot make
            // (it sees only endpoints, not transport/address, and cannot tell a
            // leg's illegitimate empty channel from a codec's legitimate default
            // endpoint, §7.4).
            if let NodeConfig::Leg {
                name,
                transport,
                address,
                insecure_bind,
                channels,
                ..
            } = node
            {
                // Loopback-only unless insecure_bind; unix is inherently local (§9).
                if *transport == Transport::Tcp && !*insecure_bind && !is_loopback_addr(address) {
                    errors.push(ValidationError::NonLoopbackBind {
                        node: name.clone(),
                        address: address.clone(),
                    });
                }
                // A leg has no default endpoint, so an empty channel identity is a
                // real channel with a reserved name — forbidden (§3). (A codec's
                // empty channel is instead caught as a collision with its default
                // endpoint by the model.)
                if channels.iter().any(|ch| ch.is_empty()) {
                    errors.push(ValidationError::DuplicateEndpoint {
                        node: name.clone(),
                        endpoint: String::new(),
                    });
                }
                // A leg is the only node kind that can shape to zero endpoints; an
                // empty channel list is a degenerate transport that carries nothing
                // and would otherwise load and report "connected" while dead (§7.4).
                if channels.is_empty() {
                    errors.push(ValidationError::EmptyLeg { node: name.clone() });
                }
            }
            // A map's mapping lists are opaque strings the topology model never sees,
            // so — like the leg checks above and the codec attribute schema — an
            // unknown mapping name is validated here at the config level (§7.8). This
            // runs before any teardown in `load --replace`, so a bad name can never
            // destroy a good graph, matching the codec precheck's guarantee.
            if let NodeConfig::Map {
                name,
                hostward,
                targetward,
                ..
            } = node
            {
                for mapping in hostward.iter().chain(targetward) {
                    if Mapping::from_name(mapping).is_none() {
                        errors.push(ValidationError::UnknownMapping {
                            node: name.clone(),
                            mapping: mapping.clone(),
                        });
                    }
                }
            }
        }
        errors.extend(self.to_model().validate());
        errors
    }
}

/// Whether an address's host is loopback (§7.4/§9 loopback-only rule). Accepts a
/// `host:port` (tcp) form; a bare host also works. A host that parses as an IP
/// uses [`std::net::IpAddr::is_loopback`]; the literal `localhost` is loopback;
/// everything else — other hostnames and the wildcard binds `0.0.0.0` / `::` —
/// is treated as non-loopback, so a remote exposure needs the explicit
/// `insecure_bind` confession.
fn is_loopback_addr(address: &str) -> bool {
    // Split off the port. Bracketed IPv6 (`[::1]:port`) and bare IPv6 both need
    // care: rsplit_once(':') on a bare `::1` would wrongly split the address.
    let host = if let Some(rest) = address.strip_prefix('[') {
        // `[ipv6]` or `[ipv6]:port`
        match rest.split_once(']') {
            Some((inner, _)) => inner,
            None => rest,
        }
    } else if let Ok(ip) = address.parse::<std::net::IpAddr>() {
        // A bare IP (including bare IPv6 with no port) parses directly.
        return ip.is_loopback();
    } else {
        // `host:port` (host is a name or IPv4) — take everything before the port.
        address.rsplit_once(':').map(|(h, _)| h).unwrap_or(address)
    };
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        ip.is_loopback()
    } else {
        host.eq_ignore_ascii_case("localhost")
    }
}

/// An edge in configuration form: two endpoint addresses and a write mode (§6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeConfig {
    pub a: EndpointAddr,
    pub b: EndpointAddr,
    /// Write-arbitration mode for this edge (§6). Two runtime overrides exist, both
    /// applied in the daemon's `Wiring::build`: on an edge whose target is an
    /// inherently read-only node (a log, §7.3) the effective mode is forced to
    /// `never` (the log gets no targetward path and no lock handle); and on a map's
    /// raw edge (target = `node/raw`, §7.8) an omitted or `on-demand` mode is promoted
    /// to `held`, since a map owns the console's writes and a held-origin pump cannot
    /// run on-demand — an explicit `never` there instead makes a read-only map. The
    /// configured value still round-trips verbatim through `dump`/`load`, so a
    /// persisted value that an override supersedes is cosmetic and does not reflect
    /// runtime behavior.
    #[serde(default)]
    pub write_mode: WriteMode,
}

/// A node configuration. Internally tagged by `type`; each variant carries a
/// `name` (operator-chosen, §3) plus its kind-specific attributes.
///
/// `Eq` is deliberately absent (see [`GraphConfig`]): the codec variant's
/// [`toml::Table`] attribute table is only `PartialEq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum NodeConfig {
    /// Serial port node (§7.1).
    Serial {
        name: String,
        /// Device identity in resolver form or a raw `/dev` path (the resolver
        /// upgrade lands in phase 7 without a format change).
        device: String,
        #[serde(default = "default_baud")]
        baud: u32,
        #[serde(default)]
        data_bits: DataBits,
        #[serde(default)]
        parity: Parity,
        #[serde(default)]
        stop_bits: StopBits,
        #[serde(default)]
        flow_control: FlowControl,
        /// Faces host in the normal role; target when used as an output leg
        /// toward another machine's tools (§7.1).
        #[serde(default = "default_faces_host")]
        faces: Facing,
        /// Write-arbitration policy for this node's host-facing endpoint (§6).
        /// Defaults to exclusive; `free-for-all` is the escape hatch for
        /// machine-to-machine links coordinated elsewhere.
        #[serde(default)]
        arbitration: Arbitration,
        /// Hostward-consumer drop policy (§5, §7.1): the bounded buffer depth (in
        /// chunks) the serial's fan-out to each consumer absorbs before
        /// dropping-with-counters — a slow spy costs only itself, never faults.
        /// Defaults to the built-in depth.
        #[serde(default = "default_serial_hostward_buffer")]
        hostward_buffer: usize,
        /// On faulted-and-wait reconnect (the device reappearing after an
        /// unplug/power-cycle), discard the targetward backlog buffered during
        /// the outage rather than firing minutes-old commands into a booting
        /// device (§7.1). Default on; the one sanctioned drain of the otherwise
        /// never-drop targetward path, always counted.
        #[serde(default = "default_true")]
        purge_on_reconnect: bool,
        /// Replay ring depth in bytes for this serial node's host-facing endpoint
        /// (§5): a bounded ring of the most recent hostward bytes, retained so a
        /// late tap (§17) sees what just happened. A feature buffer, not flow
        /// control — it never backpressures. Defaults on at 64 KiB (§15.32); set
        /// `0` to opt out.
        #[serde(default = "default_replay_ring")]
        replay_ring: usize,
        /// Initial modem-line assertions applied at open (§7.1). Declared last so
        /// it serializes after the scalar fields (a nested table must follow them
        /// in TOML's array-of-tables syntax); omitted when unset.
        #[serde(default, skip_serializing_if = "ModemLines::is_unset")]
        modem: ModemLines,
    },
    /// PTY node (§7.2). Always faces target.
    Pty {
        name: String,
        /// Symlink path to the allocated pts node (required).
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        owner: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<u32>,
        /// Cosmetic baud reported to clients via tcgetattr (§7.2).
        #[serde(default = "default_baud")]
        advertised_baud: u32,
        /// Hostward drop policy (§5, §7.2): the bounded buffer depth (in chunks)
        /// this PTY's writer bridge absorbs before dropping-with-counters when a
        /// slow client cannot keep up — never faults (a slow client costs only
        /// itself). Defaults to the built-in depth.
        #[serde(default = "default_pty_hostward_buffer")]
        hostward_buffer: usize,
    },
    /// Log node (§7.3). Always faces target; write mode is inherently `never`.
    Log {
        name: String,
        directory: String,
        filename: String,
        #[serde(default)]
        overflow: OverflowPolicy,
        #[serde(default = "default_rotation_padding")]
        rotation_padding: u8,
    },
    /// Codec node (§7.5, §7.6): an interior protocol transform with one
    /// multiplexed-side endpoint (the node's default endpoint) and N channel
    /// endpoints (named by their channel identities). `faces` orients the
    /// multiplexed side — `target` for a demultiplexer (channels face host), the
    /// mirror for re-multiplexing — so one implementation serves both (§8).
    ///
    /// The exec codec (§7.6) is selected by `codec = "exec"`; its child process,
    /// argv, environment, and restart backoff live in `attributes`, the opaque
    /// codec-validated table.
    Codec {
        name: String,
        /// Registry name selecting a compiled-in codec (§8 match-on-name).
        codec: String,
        /// Orientation of the multiplexed side. Demultiplexer (default): the
        /// multiplexed side faces target and channels face host (§7.5).
        #[serde(default = "default_faces_target")]
        faces: Facing,
        /// Channel identities (§8 static channels); each is a channel endpoint.
        /// A `/` in any identity is a structural error (§3), caught by the
        /// graph validator alongside node names.
        channels: Vec<String>,
        /// Arbitration policy for this codec's host-facing endpoints (§6): the
        /// channels in a demultiplexer, the multiplexed side in a re-multiplexer.
        #[serde(default)]
        arbitration: Arbitration,
        /// Replay-ring depth in bytes applied to every host-facing channel endpoint
        /// of this codec (§5, §15.32): a demultiplexer's channels each keep an
        /// independent ring of their most recent hostward bytes. Defaults on at
        /// 64 KiB; set `0` to opt every channel out. A re-multiplexer's host-facing
        /// side (its multiplexed endpoint) rings the same way.
        #[serde(default = "default_replay_ring")]
        replay_ring: usize,
        /// The opaque, codec-validated attribute table (§8). The codec
        /// deserializes it into its own types; a schema failure is structural
        /// and fails the load (§11). Empty for the reference framing codec, which
        /// needs no attributes. Declared last so it serializes after the scalar
        /// fields, which TOML's table syntax requires.
        #[serde(default, skip_serializing_if = "toml::Table::is_empty")]
        attributes: toml::Table,
    },
    /// Leg node (§7.4): the cross-daemon transport. A socket carrying all of its
    /// channels multiplexed by the built-in link codec (§8, §9). One endpoint per
    /// configured channel identity; all channel endpoints face target on the
    /// sending side (`faces = "target"`, computer A: the leg consumes local
    /// channels) or host on the receiving side (`faces = "host"`, computer B: the
    /// leg offers arriving channels). There is no multiplexed-side default
    /// endpoint — the socket is off-graph.
    Leg {
        name: String,
        /// Orientation of every channel endpoint. Required (no default): `target`
        /// consumes local channels for transport; `host` offers arriving channels
        /// to local consumers. A wrong orientation must not be silent.
        faces: Facing,
        /// Socket substrate. `unix` is inherently local; `tcp` is loopback-only
        /// unless `insecure_bind` (§9).
        #[serde(default)]
        transport: Transport,
        /// `listen` binds and accepts one peer; `connect` dials with backoff.
        /// Required (no default).
        role: LegRole,
        /// The bind (listen) or dial (connect) address: `host:port` for tcp, a
        /// filesystem path for unix.
        address: String,
        /// A non-loopback tcp bind/dial requires this deliberately ugly flag
        /// (§9): a named footgun beats a patched binary.
        #[serde(default, skip_serializing_if = "is_false")]
        insecure_bind: bool,
        /// Reconnect backoff for the `connect` role: initial and maximum delay in
        /// milliseconds (exponential in between).
        #[serde(default = "default_reconnect_initial_ms")]
        reconnect_initial_ms: u64,
        #[serde(default = "default_reconnect_max_ms")]
        reconnect_max_ms: u64,
        /// Idle-release interval (ms) for implicit lock acquisition on the sending
        /// side's targetward writes (§6, §7.4).
        #[serde(default = "default_idle_release_ms")]
        idle_release_ms: u64,
        /// Discard outage-era targetward backlogs on reconnect (default on, §7.4).
        #[serde(default = "default_true")]
        purge_on_reconnect: bool,
        /// Arbitration policy for host-facing channel endpoints (`faces = "host"`),
        /// as for a codec's host-facing endpoints (§6).
        #[serde(default)]
        arbitration: Arbitration,
        /// Replay-ring depth in bytes applied to every host-facing channel of this
        /// leg (§5, §15.32) — the arriving channels on the receiving side
        /// (`faces = "host"`). Each channel keeps its own ring. Defaults on at
        /// 64 KiB; `0` opts out. Ignored on a sending leg (`faces = "target"`),
        /// whose channels face target and have no hostward stream to ring.
        #[serde(default = "default_replay_ring")]
        replay_ring: usize,
        /// The channel identities carried over this leg; each is a channel
        /// endpoint. A `/` in any identity — or an empty identity — is a
        /// structural error (§3). Declared last so it serializes after the scalar
        /// fields, which TOML's array-of-tables syntax requires.
        channels: Vec<String>,
    },
    /// Map node (§7.8, §15.33): a stateless per-console character-mapping transform.
    /// One host-facing default endpoint (the *mapped* side, addressed by the bare
    /// node name and carrying the standard write-lock / fan-out / tap / replay-ring
    /// machinery) and one target-facing endpoint (the *raw*, unmapped upstream side,
    /// addressed as `node/raw` — [`MAP_RAW_ENDPOINT`]). Deliberately **not** a codec:
    /// no channels, no frames, no resync — just §5's interior contract at its
    /// simplest, so both a raw view (the upstream endpoint's ring) and a mapped view
    /// (this node's ring) exist by default.
    ///
    /// The map's edge into the upstream endpoint **defaults to `held`** (§7.8) — the
    /// demux's pattern with softer stakes: bypassing a map is not corruption, merely
    /// unmapped, so steal-to-bypass is a legitimate, visible act. `send` at this
    /// node's endpoint speaks mapped; `send` at the upstream endpoint, after a steal,
    /// speaks raw. Because the generic edge default is `on-demand` — which a
    /// held-origin interior pump cannot drive — an omitted or `on-demand` raw edge is
    /// treated as `held` at runtime ([`crate::config`] → the daemon's `Wiring::build`);
    /// an explicit `never` makes a read-only/display map with no targetward path.
    Map {
        name: String,
        /// Ordered mappings applied to **hostward** bytes (device → consumers) —
        /// picocom's `--imap`. First match per input byte wins; an unknown name is a
        /// structural error (§7.8). An empty list is the identity.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        hostward: Vec<String>,
        /// Ordered mappings applied to **targetward** bytes (consumers → device) —
        /// picocom's `--omap`. Same rules as [`Self::Map::hostward`].
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        targetward: Vec<String>,
        /// Arbitration policy for the mapped (host-facing) endpoint (§6). Defaults to
        /// exclusive, as for every host-facing endpoint.
        #[serde(default)]
        arbitration: Arbitration,
        /// Replay-ring depth in bytes for the mapped (host-facing) endpoint (§5,
        /// §15.32). Defaults on at 64 KiB; `0` opts out. The raw upstream view has its
        /// own ring on the upstream endpoint, so a map yields a raw and a mapped
        /// scrollback by default (§7.8).
        #[serde(default = "default_replay_ring")]
        replay_ring: usize,
    },
}

/// Leg socket substrate (§7.4). `unix` is inherently local; `tcp` is
/// loopback-only unless `insecure_bind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    #[default]
    Tcp,
    Unix,
}

/// Leg connection role (§7.4). `listen` binds and accepts one peer; `connect`
/// dials and retries with backoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LegRole {
    Listen,
    Connect,
}

impl NodeConfig {
    pub fn name(&self) -> &str {
        match self {
            NodeConfig::Serial { name, .. }
            | NodeConfig::Pty { name, .. }
            | NodeConfig::Log { name, .. }
            | NodeConfig::Codec { name, .. }
            | NodeConfig::Leg { name, .. }
            | NodeConfig::Map { name, .. } => name,
        }
    }

    /// The configured replay-ring depth (in bytes) applied to each of this node's
    /// host-facing endpoints (§5, §15.32). `None` for node types that own no
    /// host-facing endpoint of their own (PTY, log — both face target). The wiring
    /// applies the value only to endpoints that actually face host, so a value on a
    /// serial/codec/leg oriented entirely toward target is inert.
    pub fn replay_ring(&self) -> Option<usize> {
        match self {
            NodeConfig::Serial { replay_ring, .. }
            | NodeConfig::Codec { replay_ring, .. }
            | NodeConfig::Leg { replay_ring, .. }
            // A map's mapped side is its host-facing default endpoint, which carries
            // the ring (§7.8); the raw target-facing endpoint gets none, inert.
            | NodeConfig::Map { replay_ring, .. } => Some(*replay_ring),
            NodeConfig::Pty { .. } | NodeConfig::Log { .. } => None,
        }
    }

    /// The topological shape (endpoints + facings) this node exposes (§4).
    pub fn shape(&self) -> NodeShape {
        match self {
            NodeConfig::Serial {
                faces, arbitration, ..
            } => NodeShape::single_arb(*faces, *arbitration),
            // PTY and log look back toward the device: they face target.
            NodeConfig::Pty { .. } | NodeConfig::Log { .. } => NodeShape::single(Facing::Target),
            // A codec exposes its multiplexed side as the default (empty) endpoint
            // and one endpoint per channel identity. `faces` orients the
            // multiplexed side; channels face the opposite (§7.5). Host-facing
            // endpoints carry the node's arbitration policy (§6).
            NodeConfig::Codec {
                faces,
                channels,
                arbitration,
                ..
            } => {
                let arb = |facing: Facing| {
                    if facing == Facing::Host {
                        *arbitration
                    } else {
                        Arbitration::default()
                    }
                };
                let mut endpoints = vec![EndpointSpec {
                    name: DEFAULT_ENDPOINT.to_owned(),
                    facing: *faces,
                    arbitration: arb(*faces),
                }];
                let chan_facing = faces.opposite();
                for ch in channels {
                    endpoints.push(EndpointSpec {
                        name: ch.clone(),
                        facing: chan_facing,
                        arbitration: arb(chan_facing),
                    });
                }
                NodeShape::new(endpoints)
            }
            // A leg exposes one endpoint per channel identity — and no default
            // endpoint (the socket is off-graph, §7.4). All channels face the same
            // direction (`faces`); host-facing channels carry the node's
            // arbitration policy (§6).
            NodeConfig::Leg {
                faces,
                channels,
                arbitration,
                ..
            } => {
                let arb = if *faces == Facing::Host {
                    *arbitration
                } else {
                    Arbitration::default()
                };
                let endpoints = channels
                    .iter()
                    .map(|ch| EndpointSpec {
                        name: ch.clone(),
                        facing: *faces,
                        arbitration: arb,
                    })
                    .collect();
                NodeShape::new(endpoints)
            }
            // A map exposes its mapped side as the host-facing default endpoint
            // (carrying the node's arbitration policy) and its raw side as one
            // target-facing endpoint (§7.8). The target-facing raw endpoint has the
            // default arbitration — it is an origin into the upstream, not an
            // arbitrated host endpoint of its own.
            NodeConfig::Map { arbitration, .. } => NodeShape::new(vec![
                EndpointSpec {
                    name: DEFAULT_ENDPOINT.to_owned(),
                    facing: Facing::Host,
                    arbitration: *arbitration,
                },
                EndpointSpec {
                    name: MAP_RAW_ENDPOINT.to_owned(),
                    facing: Facing::Target,
                    arbitration: Arbitration::default(),
                },
            ]),
        }
    }

    /// The write-arbitration policy of this node's host-facing endpoint(s) (§6),
    /// or the default for node kinds without one. A codec's policy applies to all
    /// of its host-facing endpoints uniformly.
    pub fn arbitration(&self) -> Arbitration {
        match self {
            NodeConfig::Serial { arbitration, .. }
            | NodeConfig::Codec { arbitration, .. }
            | NodeConfig::Leg { arbitration, .. }
            | NodeConfig::Map { arbitration, .. } => *arbitration,
            NodeConfig::Pty { .. } | NodeConfig::Log { .. } => Arbitration::default(),
        }
    }
}

/// Serial data bits (§7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataBits {
    Five,
    Six,
    Seven,
    #[default]
    Eight,
}

/// Serial parity (§7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Parity {
    #[default]
    None,
    Odd,
    Even,
}

/// Serial stop bits (§7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StopBits {
    #[default]
    One,
    Two,
}

/// Serial flow control. `none` is the 3-wire default (§5); the others remain
/// ordinary port attributes (§7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FlowControl {
    #[default]
    None,
    XonXoff,
    RtsCts,
}

/// Initial modem-line assertions for a serial node (§7.1). Applied at open and
/// (in phase 7) re-applied on reopen, so line states are deterministic against
/// auto-reset adapters. Each line may be asserted (`true`), deasserted (`false`),
/// or left untouched (omitted — the default, which keeps the driver's power-on
/// state). `set-modem`/`pulse-dtr` control verbs (§7.1) act on the live port
/// later; these are the *initial* states configuration owns and round-trips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ModemLines {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dtr: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rts: Option<bool>,
}

impl ModemLines {
    /// Whether no line assertion is configured (both untouched) — the default,
    /// skipped in serialization so an unset config stays clean and round-trips.
    pub fn is_unset(&self) -> bool {
        self.dtr.is_none() && self.rts.is_none()
    }
}

/// Boundary overflow policy for bounded queues (log nodes, §7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverflowPolicy {
    /// Drop the oldest queued bytes, with counters.
    #[default]
    DropOldest,
    /// Fault the node instead of dropping.
    Fault,
}

fn default_baud() -> u32 {
    115_200
}

/// Default hostward buffer depth (in chunks) for a serial node's fan-out to each
/// consumer (§5, §7.1). Matches the data plane's built-in `CHANNEL_CAP`, so an
/// unset config keeps today's behavior.
fn default_serial_hostward_buffer() -> usize {
    256
}

/// Default hostward buffer depth (in chunks) for a PTY node's writer bridge
/// (§5, §7.2). Matches the data plane's built-in `WRITER_QUEUE`, so an unset
/// config keeps today's behavior.
fn default_pty_hostward_buffer() -> usize {
    32
}

fn default_rotation_padding() -> u8 {
    3
}

fn default_faces_host() -> Facing {
    Facing::Host
}

fn default_faces_target() -> Facing {
    Facing::Target
}

fn default_reconnect_initial_ms() -> u64 {
    200
}

fn default_reconnect_max_ms() -> u64 {
    5_000
}

fn default_idle_release_ms() -> u64 {
    1_000
}

/// Default replay-ring depth in bytes for every host-facing endpoint (§5, §15.32):
/// 64 KiB of scrollback, on by default so a console never punishes a late attacher.
/// `0` opts out. Applies to serial nodes and to every host-facing channel of a
/// codec or leg node.
pub const DEFAULT_REPLAY_RING: usize = 65536;

fn default_replay_ring() -> usize {
    DEFAULT_REPLAY_RING
}

fn default_true() -> bool {
    true
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn reference_config_round_trips_through_toml() {
        let cfg = GraphConfig {
            nodes: vec![
                NodeConfig::Serial {
                    name: "usb0".into(),
                    device: "usb:0403:6001:ABSCDJ6O:00".into(),
                    baud: 115_200,
                    data_bits: DataBits::Eight,
                    parity: Parity::None,
                    stop_bits: StopBits::One,
                    flow_control: FlowControl::None,
                    faces: Facing::Host,
                    arbitration: Arbitration::Exclusive,
                    hostward_buffer: 256,
                    purge_on_reconnect: true,
                    replay_ring: 0,
                    modem: ModemLines::default(),
                },
                NodeConfig::Pty {
                    name: "console".into(),
                    path: "/run/serial_nexus/console".into(),
                    owner: None,
                    group: Some("dialout".into()),
                    mode: Some(0o660),
                    advertised_baud: 115_200,
                    hostward_buffer: 32,
                },
                NodeConfig::Log {
                    name: "log".into(),
                    directory: "/var/log/serial_nexus".into(),
                    filename: "console.log".into(),
                    overflow: OverflowPolicy::DropOldest,
                    rotation_padding: 3,
                },
            ],
            edges: vec![
                EdgeConfig {
                    a: EndpointAddr::node("usb0"),
                    b: EndpointAddr::node("console"),
                    write_mode: WriteMode::OnDemand,
                },
                EdgeConfig {
                    a: EndpointAddr::node("usb0"),
                    b: EndpointAddr::node("log"),
                    write_mode: WriteMode::Never,
                },
            ],
        };

        let toml = toml::to_string(&cfg).expect("serialize");
        let back: GraphConfig = toml::from_str(&toml).expect("deserialize");
        assert_eq!(cfg, back, "config must round-trip through TOML\n{toml}");

        // And the reference config is structurally valid (§4).
        assert!(cfg.to_model().check().is_ok());
    }

    #[test]
    fn demux_config_round_trips_and_validates() {
        // The §2 reference topology in miniature: a serial feeds a demux codec,
        // whose two channels fan out to a PTY and a log. Exercises the codec node
        // config (multiplexed side + channels + an opaque attribute table) through
        // TOML and structural validation.
        let cfg = GraphConfig {
            nodes: vec![
                NodeConfig::Serial {
                    name: "usb0".into(),
                    device: "usb:0403:6001:ABSCDJ6O:00".into(),
                    baud: 115_200,
                    data_bits: DataBits::Eight,
                    parity: Parity::None,
                    stop_bits: StopBits::One,
                    flow_control: FlowControl::None,
                    faces: Facing::Host,
                    arbitration: Arbitration::Exclusive,
                    hostward_buffer: 256,
                    purge_on_reconnect: true,
                    replay_ring: 0,
                    modem: ModemLines::default(),
                },
                NodeConfig::Codec {
                    name: "mux".into(),
                    codec: "reference".into(),
                    faces: Facing::Target,
                    channels: vec!["console".into(), "trace".into()],
                    arbitration: Arbitration::Exclusive,
                    replay_ring: DEFAULT_REPLAY_RING,
                    attributes: {
                        let mut t = toml::Table::new();
                        t.insert("resync".into(), toml::Value::Boolean(true));
                        t
                    },
                },
                NodeConfig::Pty {
                    name: "console-pty".into(),
                    path: "/run/serial_nexus/console".into(),
                    owner: None,
                    group: None,
                    mode: None,
                    advertised_baud: 115_200,
                    hostward_buffer: 32,
                },
                NodeConfig::Log {
                    name: "trace-log".into(),
                    directory: "/var/log/serial_nexus".into(),
                    filename: "trace.log".into(),
                    overflow: OverflowPolicy::DropOldest,
                    rotation_padding: 3,
                },
            ],
            edges: vec![
                // serial(host) -> codec multiplexed side (target, the default
                // endpoint, addressed as the node name); the demux edge holds the
                // serial's write lock (§6).
                EdgeConfig {
                    a: EndpointAddr::node("usb0"),
                    b: EndpointAddr::node("mux"),
                    write_mode: WriteMode::Held,
                },
                // Each channel (host) fans out to a target consumer.
                EdgeConfig {
                    a: EndpointAddr::channel("mux", "console"),
                    b: EndpointAddr::node("console-pty"),
                    write_mode: WriteMode::OnDemand,
                },
                EdgeConfig {
                    a: EndpointAddr::channel("mux", "trace"),
                    b: EndpointAddr::node("trace-log"),
                    write_mode: WriteMode::Never,
                },
            ],
        };

        let toml = toml::to_string(&cfg).expect("serialize");
        let back: GraphConfig = toml::from_str(&toml).expect("deserialize");
        assert_eq!(
            cfg, back,
            "demux config must round-trip through TOML\n{toml}"
        );
        assert!(
            cfg.to_model().check().is_ok(),
            "demux topology must be structurally valid"
        );
    }

    #[test]
    fn duplicate_node_names_are_rejected() {
        // Two nodes named the same collapse in the model's shape map; the config-
        // level validate() catches it (the model alone cannot), naming the offender.
        let cfg = GraphConfig {
            nodes: vec![
                NodeConfig::Pty {
                    name: "dup".into(),
                    path: "/tmp/a".into(),
                    owner: None,
                    group: None,
                    mode: None,
                    advertised_baud: 115_200,
                    hostward_buffer: 32,
                },
                NodeConfig::Log {
                    name: "dup".into(),
                    directory: "/tmp".into(),
                    filename: "l.log".into(),
                    overflow: OverflowPolicy::DropOldest,
                    rotation_padding: 3,
                },
            ],
            edges: vec![],
        };
        assert!(
            cfg.validate()
                .iter()
                .any(|e| matches!(e, ValidationError::DuplicateNodeName { node } if node == "dup")),
            "expected DuplicateNodeName, got {:?}",
            cfg.validate()
        );
    }

    #[test]
    fn empty_node_name_is_rejected() {
        // §11 legality ("no empties"): an empty node name collides with the empty
        // local name reserved for default endpoints (§3), so load must refuse it —
        // symmetric with empty_leg_channel_identity_is_rejected on the identity side.
        let cfg = GraphConfig {
            nodes: vec![NodeConfig::Log {
                name: String::new(),
                directory: "/tmp".into(),
                filename: "l.log".into(),
                overflow: OverflowPolicy::DropOldest,
                rotation_padding: 3,
            }],
            edges: vec![],
        };
        assert!(
            cfg.validate()
                .iter()
                .any(|e| matches!(e, ValidationError::EmptyName { node } if node.is_empty())),
            "expected EmptyName, got {:?}",
            cfg.validate()
        );
    }

    #[test]
    fn serial_and_pty_hostward_and_modem_round_trip() {
        // §7.1/§7.2 config surface: a serial's hostward buffer + initial modem-line
        // assertions and a PTY's hostward buffer round-trip through TOML (the modem
        // table serializes after the scalar fields, like a codec's attributes).
        let cfg = GraphConfig {
            nodes: vec![
                NodeConfig::Serial {
                    name: "usb0".into(),
                    device: "usb:0403:6001:ABSCDJ6O:00".into(),
                    baud: 115_200,
                    data_bits: DataBits::Eight,
                    parity: Parity::None,
                    stop_bits: StopBits::One,
                    flow_control: FlowControl::None,
                    faces: Facing::Host,
                    arbitration: Arbitration::Exclusive,
                    hostward_buffer: 512,
                    purge_on_reconnect: true,
                    replay_ring: 0,
                    modem: ModemLines {
                        dtr: Some(true),
                        rts: Some(false),
                    },
                },
                NodeConfig::Pty {
                    name: "console".into(),
                    path: "/run/serial_nexus/console".into(),
                    owner: None,
                    group: None,
                    mode: None,
                    advertised_baud: 115_200,
                    hostward_buffer: 64,
                },
            ],
            edges: vec![],
        };
        let toml = toml::to_string(&cfg).expect("serialize");
        let back: GraphConfig = toml::from_str(&toml).expect("deserialize");
        assert_eq!(cfg, back, "custom hostward + modem must round-trip\n{toml}");

        // Omitted attributes fall back to the built-in defaults — so today's
        // behavior is preserved for configs that never mention them.
        let minimal = r#"
            [[node]]
            type = "serial"
            name = "usb0"
            device = "/dev/ttyUSB0"
            [[node]]
            type = "pty"
            name = "console"
            path = "/tmp/c"
        "#;
        let parsed: GraphConfig = toml::from_str(minimal).expect("parse minimal");
        match &parsed.nodes[0] {
            NodeConfig::Serial {
                hostward_buffer,
                purge_on_reconnect,
                modem,
                ..
            } => {
                assert_eq!(*hostward_buffer, 256, "serial default hostward buffer");
                assert!(*purge_on_reconnect, "purge_on_reconnect defaults on (§7.1)");
                assert!(
                    modem.is_unset(),
                    "modem defaults to unset (lines untouched)"
                );
            }
            other => panic!("expected serial, got {other:?}"),
        }
        match &parsed.nodes[1] {
            NodeConfig::Pty {
                hostward_buffer, ..
            } => assert_eq!(*hostward_buffer, 32, "pty default hostward buffer"),
            other => panic!("expected pty, got {other:?}"),
        }
    }

    #[test]
    fn leg_config_round_trips_and_validates() {
        // A receiving leg (computer B, §2): two host-facing channels fanning out
        // to local consumers, bound to loopback. Exercises the leg config through
        // TOML and structural validation.
        let cfg = GraphConfig {
            nodes: vec![
                NodeConfig::Leg {
                    name: "downlink".into(),
                    faces: Facing::Host,
                    transport: Transport::Tcp,
                    role: LegRole::Listen,
                    address: "127.0.0.1:7000".into(),
                    insecure_bind: false,
                    reconnect_initial_ms: 200,
                    reconnect_max_ms: 5_000,
                    idle_release_ms: 1_000,
                    purge_on_reconnect: true,
                    arbitration: Arbitration::Exclusive,
                    replay_ring: DEFAULT_REPLAY_RING,
                    channels: vec!["console".into(), "trace".into()],
                },
                NodeConfig::Pty {
                    name: "console-pty".into(),
                    path: "/run/serial_nexus/console".into(),
                    owner: None,
                    group: None,
                    mode: None,
                    advertised_baud: 115_200,
                    hostward_buffer: 32,
                },
            ],
            edges: vec![EdgeConfig {
                a: EndpointAddr::channel("downlink", "console"),
                b: EndpointAddr::node("console-pty"),
                write_mode: WriteMode::OnDemand,
            }],
        };
        let toml = toml::to_string(&cfg).expect("serialize");
        let back: GraphConfig = toml::from_str(&toml).expect("deserialize");
        assert_eq!(cfg, back, "leg config must round-trip through TOML\n{toml}");
        assert!(
            cfg.validate().is_empty(),
            "loopback leg must be structurally valid: {:?}",
            cfg.validate()
        );
    }

    #[test]
    fn non_loopback_leg_without_insecure_bind_is_rejected() {
        let leg = |address: &str, insecure_bind: bool, transport: Transport| NodeConfig::Leg {
            name: "uplink".into(),
            faces: Facing::Target,
            transport,
            role: LegRole::Connect,
            address: address.into(),
            insecure_bind,
            reconnect_initial_ms: 200,
            reconnect_max_ms: 5_000,
            idle_release_ms: 1_000,
            purge_on_reconnect: true,
            arbitration: Arbitration::Exclusive,
            replay_ring: DEFAULT_REPLAY_RING,
            channels: vec!["a".into()],
        };
        let rejected = |node: NodeConfig| {
            GraphConfig {
                nodes: vec![node],
                edges: vec![],
            }
            .validate()
            .iter()
            .any(|e| matches!(e, ValidationError::NonLoopbackBind { .. }))
        };
        // Non-loopback tcp without the flag: rejected.
        assert!(rejected(leg("10.0.0.5:7000", false, Transport::Tcp)));
        assert!(rejected(leg("0.0.0.0:7000", false, Transport::Tcp)));
        assert!(rejected(leg("example.com:7000", false, Transport::Tcp)));
        // With the flag: accepted.
        assert!(!rejected(leg("10.0.0.5:7000", true, Transport::Tcp)));
        // Loopback forms: accepted without the flag.
        assert!(!rejected(leg("127.0.0.1:7000", false, Transport::Tcp)));
        assert!(!rejected(leg("localhost:7000", false, Transport::Tcp)));
        assert!(!rejected(leg("[::1]:7000", false, Transport::Tcp)));
        // Bare IPs with no port hit the direct-parse branch (config.rs:126), the
        // bare-IPv6 case the surrounding comment flags as hazardous. A bare `::1`
        // would be mis-split by rsplit_once(':') without that branch. Loopback bare
        // IPs (v4 and v6): accepted without the flag.
        assert!(!rejected(leg("127.0.0.1", false, Transport::Tcp)));
        assert!(!rejected(leg("::1", false, Transport::Tcp)));
        // Non-loopback bare IPs (v4 and v6): rejected without the flag.
        assert!(rejected(leg("10.0.0.5", false, Transport::Tcp)));
        assert!(rejected(leg("2001:db8::1", false, Transport::Tcp)));
        // Unix transport is inherently local: never a NonLoopbackBind.
        assert!(!rejected(leg("/run/snx/leg.sock", false, Transport::Unix)));
    }

    #[test]
    fn empty_leg_channel_list_is_rejected() {
        let cfg = GraphConfig {
            nodes: vec![NodeConfig::Leg {
                name: "uplink".into(),
                faces: Facing::Target,
                transport: Transport::Unix,
                role: LegRole::Connect,
                address: "/run/snx/leg.sock".into(),
                insecure_bind: false,
                reconnect_initial_ms: 200,
                reconnect_max_ms: 5_000,
                idle_release_ms: 1_000,
                purge_on_reconnect: true,
                arbitration: Arbitration::Exclusive,
                replay_ring: DEFAULT_REPLAY_RING,
                channels: vec![],
            }],
            edges: vec![],
        };
        assert!(
            cfg.validate()
                .iter()
                .any(|e| matches!(e, ValidationError::EmptyLeg { node } if node == "uplink")),
            "expected EmptyLeg, got {:?}",
            cfg.validate()
        );
    }

    #[test]
    fn empty_leg_channel_identity_is_rejected() {
        let cfg = GraphConfig {
            nodes: vec![NodeConfig::Leg {
                name: "uplink".into(),
                faces: Facing::Target,
                transport: Transport::Unix,
                role: LegRole::Connect,
                address: "/run/snx/leg.sock".into(),
                insecure_bind: false,
                reconnect_initial_ms: 200,
                reconnect_max_ms: 5_000,
                idle_release_ms: 1_000,
                purge_on_reconnect: true,
                arbitration: Arbitration::Exclusive,
                replay_ring: DEFAULT_REPLAY_RING,
                channels: vec!["".into()],
            }],
            edges: vec![],
        };
        assert!(
            cfg.validate().iter().any(|e| matches!(
                e,
                ValidationError::DuplicateEndpoint { node, endpoint } if node == "uplink" && endpoint.is_empty()
            )),
            "expected empty-channel rejection, got {:?}",
            cfg.validate()
        );
    }

    #[test]
    fn zero_hostward_buffer_is_rejected() {
        // §5/§7.1/§7.2: a zero-depth hostward buffer builds a rendezvous channel
        // that drops nearly all hostward output even for a fast consumer, so
        // validate() must refuse it for the two node kinds that carry the tunable.
        let serial = GraphConfig {
            nodes: vec![NodeConfig::Serial {
                name: "usb0".into(),
                device: "/dev/ttyUSB0".into(),
                baud: 115_200,
                data_bits: DataBits::Eight,
                parity: Parity::None,
                stop_bits: StopBits::One,
                flow_control: FlowControl::None,
                faces: Facing::Host,
                arbitration: Arbitration::Exclusive,
                hostward_buffer: 0,
                purge_on_reconnect: true,
                replay_ring: 0,
                modem: ModemLines::default(),
            }],
            edges: vec![],
        };
        assert!(
            serial.validate().iter().any(
                |e| matches!(e, ValidationError::ZeroHostwardBuffer { node } if node == "usb0")
            ),
            "expected ZeroHostwardBuffer for serial, got {:?}",
            serial.validate()
        );

        let pty = GraphConfig {
            nodes: vec![NodeConfig::Pty {
                name: "console".into(),
                path: "/run/serial_nexus/console".into(),
                owner: None,
                group: None,
                mode: None,
                advertised_baud: 115_200,
                hostward_buffer: 0,
            }],
            edges: vec![],
        };
        assert!(
            pty.validate().iter().any(
                |e| matches!(e, ValidationError::ZeroHostwardBuffer { node } if node == "console")
            ),
            "expected ZeroHostwardBuffer for pty, got {:?}",
            pty.validate()
        );

        // A depth of 1 is the sane floor — accepted (no ZeroHostwardBuffer).
        let ok = GraphConfig {
            nodes: vec![NodeConfig::Pty {
                name: "console".into(),
                path: "/run/serial_nexus/console".into(),
                owner: None,
                group: None,
                mode: None,
                advertised_baud: 115_200,
                hostward_buffer: 1,
            }],
            edges: vec![],
        };
        assert!(
            !ok.validate()
                .iter()
                .any(|e| matches!(e, ValidationError::ZeroHostwardBuffer { .. })),
            "hostward_buffer = 1 must be accepted, got {:?}",
            ok.validate()
        );
    }

    #[test]
    fn map_config_round_trips_and_validates() {
        // A quirky console (§7.8): a serial feeds a held map, whose mapped side fans
        // out to a PTY. Exercises the map config (ordered hostward/targetward lists)
        // through TOML and structural validation, including the `node/raw` target
        // endpoint addressing.
        let cfg = GraphConfig {
            nodes: vec![
                NodeConfig::Serial {
                    name: "usb0".into(),
                    device: "usb:0403:6001:ABSCDJ6O:00".into(),
                    baud: 115_200,
                    data_bits: DataBits::Eight,
                    parity: Parity::None,
                    stop_bits: StopBits::One,
                    flow_control: FlowControl::None,
                    faces: Facing::Host,
                    arbitration: Arbitration::Exclusive,
                    hostward_buffer: 256,
                    purge_on_reconnect: true,
                    replay_ring: DEFAULT_REPLAY_RING,
                    modem: ModemLines::default(),
                },
                NodeConfig::Map {
                    name: "console".into(),
                    hostward: vec!["crlf".into()],
                    targetward: vec!["lfcr".into()],
                    arbitration: Arbitration::Exclusive,
                    replay_ring: DEFAULT_REPLAY_RING,
                },
                NodeConfig::Pty {
                    name: "console-pty".into(),
                    path: "/run/serial_nexus/console".into(),
                    owner: None,
                    group: None,
                    mode: None,
                    advertised_baud: 115_200,
                    hostward_buffer: 32,
                },
            ],
            edges: vec![
                // serial(host) -> map raw side (target, `node/raw`); the map's edge
                // into the upstream is held, the demux's pattern (§7.8).
                EdgeConfig {
                    a: EndpointAddr::node("usb0"),
                    b: EndpointAddr::channel("console", MAP_RAW_ENDPOINT),
                    write_mode: WriteMode::Held,
                },
                // map's mapped side (host, the default endpoint) -> the PTY.
                EdgeConfig {
                    a: EndpointAddr::node("console"),
                    b: EndpointAddr::node("console-pty"),
                    write_mode: WriteMode::OnDemand,
                },
            ],
        };
        let toml = toml::to_string(&cfg).expect("serialize");
        let back: GraphConfig = toml::from_str(&toml).expect("deserialize");
        assert_eq!(cfg, back, "map config must round-trip through TOML\n{toml}");
        assert!(
            cfg.validate().is_empty(),
            "map topology must be structurally valid: {:?}",
            cfg.validate()
        );

        // The map's shape: mapped host default endpoint + raw target endpoint (§7.8).
        let shape = cfg.nodes[1].shape();
        assert_eq!(shape.endpoints.len(), 2, "a map has exactly two endpoints");
        let mapped = shape.endpoints.iter().find(|e| e.name.is_empty()).unwrap();
        assert_eq!(mapped.facing, Facing::Host, "mapped side faces host");
        let raw = shape
            .endpoints
            .iter()
            .find(|e| e.name == MAP_RAW_ENDPOINT)
            .unwrap();
        assert_eq!(raw.facing, Facing::Target, "raw side faces target");

        // Omitted mapping lists default to the identity (empty) and round-trip clean.
        let minimal: GraphConfig = toml::from_str(
            r#"
            [[node]]
            type = "map"
            name = "m"
        "#,
        )
        .expect("parse minimal map");
        match &minimal.nodes[0] {
            NodeConfig::Map {
                hostward,
                targetward,
                replay_ring,
                ..
            } => {
                assert!(hostward.is_empty() && targetward.is_empty(), "identity map");
                assert_eq!(*replay_ring, DEFAULT_REPLAY_RING, "map ring defaults on");
            }
            other => panic!("expected map, got {other:?}"),
        }
    }

    #[test]
    fn unknown_mapping_is_rejected() {
        // §7.8: an unknown mapping name is a structural error naming the offender, in
        // either direction — caught by validate() before any teardown (so a bad
        // `--replace` config never destroys a good graph).
        let cfg = |hostward: Vec<String>, targetward: Vec<String>| GraphConfig {
            nodes: vec![NodeConfig::Map {
                name: "m".into(),
                hostward,
                targetward,
                arbitration: Arbitration::Exclusive,
                replay_ring: DEFAULT_REPLAY_RING,
            }],
            edges: vec![],
        };
        // Bad name hostward.
        assert!(
            cfg(vec!["crlf".into(), "bogus".into()], vec![])
                .validate()
                .iter()
                .any(
                    |e| matches!(e, ValidationError::UnknownMapping { node, mapping }
                    if node == "m" && mapping == "bogus")
                ),
            "expected UnknownMapping for a bad hostward name"
        );
        // Bad name targetward.
        assert!(
            cfg(vec![], vec!["nope".into()]).validate().iter().any(
                |e| matches!(e, ValidationError::UnknownMapping { mapping, .. }
                    if mapping == "nope")
            ),
            "expected UnknownMapping for a bad targetward name"
        );
        // Every valid picocom name is accepted.
        let all: Vec<String> = crate::map::Mapping::all_names().map(String::from).collect();
        assert!(
            !cfg(all.clone(), all)
                .validate()
                .iter()
                .any(|e| matches!(e, ValidationError::UnknownMapping { .. })),
            "the full picocom vocabulary must validate"
        );
    }

    // Proptest strategies producing well-typed (not necessarily graph-valid)
    // configurations, to prove serde round-trips. Every enum variant, every
    // Some/None option, non-default numerics, and edges are all reachable, so a
    // mis-renamed serde variant or a dropped field fails the property rather
    // than shipping green.
    fn ident() -> impl Strategy<Value = String> {
        "[a-z][a-z0-9_]{0,7}"
    }

    fn any_data_bits() -> impl Strategy<Value = DataBits> {
        prop_oneof![
            Just(DataBits::Five),
            Just(DataBits::Six),
            Just(DataBits::Seven),
            Just(DataBits::Eight),
        ]
    }
    fn any_parity() -> impl Strategy<Value = Parity> {
        prop_oneof![Just(Parity::None), Just(Parity::Odd), Just(Parity::Even)]
    }
    fn any_stop_bits() -> impl Strategy<Value = StopBits> {
        prop_oneof![Just(StopBits::One), Just(StopBits::Two)]
    }
    fn any_flow() -> impl Strategy<Value = FlowControl> {
        prop_oneof![
            Just(FlowControl::None),
            Just(FlowControl::XonXoff),
            Just(FlowControl::RtsCts),
        ]
    }
    fn any_facing() -> impl Strategy<Value = Facing> {
        prop_oneof![Just(Facing::Host), Just(Facing::Target)]
    }
    fn any_mapping_name() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("crlf"),
            Just("crcrlf"),
            Just("igncr"),
            Just("lfcr"),
            Just("lfcrlf"),
            Just("ignlf"),
            Just("bsdel"),
            Just("delbs"),
            Just("spchex"),
            Just("tabhex"),
            Just("crhex"),
            Just("lfhex"),
            Just("8bithex"),
            Just("nrmhex"),
        ]
        .prop_map(String::from)
    }
    fn any_arbitration() -> impl Strategy<Value = Arbitration> {
        prop_oneof![Just(Arbitration::Exclusive), Just(Arbitration::FreeForAll)]
    }
    fn any_transport() -> impl Strategy<Value = Transport> {
        prop_oneof![Just(Transport::Tcp), Just(Transport::Unix)]
    }
    fn any_leg_role() -> impl Strategy<Value = LegRole> {
        prop_oneof![Just(LegRole::Listen), Just(LegRole::Connect)]
    }
    fn any_overflow() -> impl Strategy<Value = OverflowPolicy> {
        prop_oneof![
            Just(OverflowPolicy::DropOldest),
            Just(OverflowPolicy::Fault)
        ]
    }
    fn any_write_mode() -> impl Strategy<Value = WriteMode> {
        prop_oneof![
            Just(WriteMode::Never),
            Just(WriteMode::OnDemand),
            Just(WriteMode::Held),
        ]
    }
    fn any_modem() -> impl Strategy<Value = ModemLines> {
        (
            proptest::option::of(any::<bool>()),
            proptest::option::of(any::<bool>()),
        )
            .prop_map(|(dtr, rts)| ModemLines { dtr, rts })
    }

    fn node_strategy() -> impl Strategy<Value = NodeConfig> {
        prop_oneof![
            (
                ident(),
                ident(),
                1u32..4_000_000,
                any_data_bits(),
                any_parity(),
                any_stop_bits(),
                any_flow(),
                any_facing(),
                any_arbitration(),
                // (hostward_buffer, replay_ring) packed into one tuple slot to stay
                // within proptest's 12-element tuple limit.
                (1usize..2048, 0usize..4096),
                any_modem(),
                any::<bool>(),
            )
                .prop_map(
                    |(
                        name,
                        device,
                        baud,
                        data_bits,
                        parity,
                        stop_bits,
                        flow_control,
                        faces,
                        arbitration,
                        (hostward_buffer, replay_ring),
                        modem,
                        purge_on_reconnect,
                    )| {
                        NodeConfig::Serial {
                            name,
                            device,
                            baud,
                            data_bits,
                            parity,
                            stop_bits,
                            flow_control,
                            faces,
                            arbitration,
                            hostward_buffer,
                            purge_on_reconnect,
                            replay_ring,
                            modem,
                        }
                    }
                ),
            (
                ident(),
                "/[a-z/]{1,16}",
                proptest::option::of(ident()),
                proptest::option::of(ident()),
                proptest::option::of(0u32..0o777),
                1u32..4_000_000,
                1usize..2048,
            )
                .prop_map(
                    |(name, path, owner, group, mode, advertised_baud, hostward_buffer)| {
                        NodeConfig::Pty {
                            name,
                            path,
                            owner,
                            group,
                            mode,
                            advertised_baud,
                            hostward_buffer,
                        }
                    },
                ),
            (ident(), ident(), ident(), any_overflow(), 1u8..9).prop_map(
                |(name, directory, filename, overflow, rotation_padding)| NodeConfig::Log {
                    name,
                    directory,
                    filename,
                    overflow,
                    rotation_padding,
                }
            ),
            (
                ident(),
                ident(),
                any_facing(),
                prop::collection::vec(ident(), 0..4),
                any_arbitration(),
                0usize..131_072,
            )
                .prop_map(
                    |(name, codec, faces, channels, arbitration, replay_ring)| {
                        NodeConfig::Codec {
                            name,
                            codec,
                            faces,
                            channels,
                            arbitration,
                            replay_ring,
                            // The attribute table's TOML round-trip is covered by the
                            // explicit demux test; keep the proptest table empty so the
                            // arbitrary structural shapes stay TOML-clean.
                            attributes: toml::Table::new(),
                        }
                    }
                ),
            (
                ident(),
                any_facing(),
                any_transport(),
                any_leg_role(),
                "127\\.0\\.0\\.1:[0-9]{2,5}",
                any::<bool>(),
                0u64..30_000,
                0u64..30_000,
                0u64..30_000,
                // (purge_on_reconnect, replay_ring) packed into one tuple slot to
                // stay within proptest's 12-tuple Strategy arity.
                (any::<bool>(), 0usize..131_072),
                any_arbitration(),
                prop::collection::vec(ident(), 0..4),
            )
                .prop_map(
                    |(
                        name,
                        faces,
                        transport,
                        role,
                        address,
                        insecure_bind,
                        reconnect_initial_ms,
                        reconnect_max_ms,
                        idle_release_ms,
                        (purge_on_reconnect, replay_ring),
                        arbitration,
                        channels,
                    )| {
                        NodeConfig::Leg {
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
                            replay_ring,
                            arbitration,
                            channels,
                        }
                    },
                ),
            (
                ident(),
                prop::collection::vec(any_mapping_name(), 0..4),
                prop::collection::vec(any_mapping_name(), 0..4),
                any_arbitration(),
                0usize..131_072,
            )
                .prop_map(|(name, hostward, targetward, arbitration, replay_ring)| {
                    NodeConfig::Map {
                        name,
                        hostward,
                        targetward,
                        arbitration,
                        replay_ring,
                    }
                }),
        ]
    }

    fn endpoint_strategy() -> impl Strategy<Value = EndpointAddr> {
        prop_oneof![
            ident().prop_map(EndpointAddr::node),
            (ident(), ident()).prop_map(|(n, c)| EndpointAddr::channel(n, c)),
        ]
    }

    fn edge_strategy() -> impl Strategy<Value = EdgeConfig> {
        (endpoint_strategy(), endpoint_strategy(), any_write_mode())
            .prop_map(|(a, b, write_mode)| EdgeConfig { a, b, write_mode })
    }

    proptest! {
        #[test]
        fn prop_config_round_trips_through_toml(
            nodes in prop::collection::vec(node_strategy(), 0..8),
            edges in prop::collection::vec(edge_strategy(), 0..8),
        ) {
            let cfg = GraphConfig { nodes, edges };
            let toml = toml::to_string(&cfg).expect("serialize");
            let back: GraphConfig = toml::from_str(&toml).expect("deserialize");
            prop_assert_eq!(cfg, back);
        }
    }
}
