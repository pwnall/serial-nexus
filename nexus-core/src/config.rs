//! Configuration types — the operator-owned, round-trippable half of the strict
//! configuration/state split (design §15.8). Everything here is *desired* state
//! that survives `dump`→`load`; nothing observed lives in these types (that is
//! [`crate::state`]). The split is enforced mechanically: state fields simply do
//! not exist on configuration types.
//!
//! The shipped node kinds are exactly [`NodeConfig`]'s variants — `serial`,
//! `pty`, `log` (the boundary kinds), `codec`, `leg` and `map` (§7.1–§7.6, §7.8).
//! Note two things this list does *not* contain. The exec codec (§7.6) is not a
//! variant: it is an ordinary [`NodeConfig::Codec`] selected by `codec = "exec"`,
//! with its child process configured in the opaque `attributes` table (see that
//! variant's docs). And `existing-terminal` (§7.7) is design-specified but
//! deliberately unimplemented (§14), so the daemon answers an `existing-terminal`
//! node with serde's unknown-variant error. The format is designed to grow
//! additively (§15.16); node kinds are internally tagged by `type` with inline
//! fields, so they serialize cleanly to TOML without `flatten`.

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
///
/// `deny_unknown_fields` (§11, "the entire file is validated … before anything is
/// created"): a mis-typed top-level table — `[[nodez]]` for `[[node]]` — used to
/// parse to an *empty* graph, and since `load --replace` composes teardown-then-load
/// the typo became an unannounced `teardown` reporting success. Silence is the one
/// thing configuration parsing may not do.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphConfig {
    #[serde(default, rename = "node", skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<NodeConfig>,
    #[serde(default, rename = "edge", skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<EdgeConfig>,
}

/// The largest accepted `replay_ring` depth, in bytes (§5, §15.32). The ring is a
/// bounded scrollback buffer sized in tens of kilobytes by design; 16 MiB is already
/// two orders of magnitude past its rationale, and the cap exists because the ring
/// allocates lazily on the first hostward byte — an unbounded value loads cleanly and
/// then *aborts the process* out of the allocator, on a configuration `load` has
/// already persisted, so the daemon crash-loops on restart (§11, §15.26).
pub const MAX_REPLAY_RING: usize = 16 * 1024 * 1024;

/// The largest accepted `hostward_buffer` depth, in chunks (§5, §7.1/§7.2). The
/// buffer absorbs a slow consumer's backlog before dropping-with-counters; 65536
/// chunks is far beyond any consumer worth waiting for. The cap exists because the
/// depth is handed straight to a bounded tokio channel, which *panics* above its
/// permit ceiling — and `load --replace` tears the running graph down before the
/// wiring is built, so the panic would arrive too late to save it (§11, §15.26).
pub const MAX_HOSTWARD_BUFFER: usize = 65_536;

/// The largest accepted value for a leg's millisecond timers — reconnect backoff and
/// idle release (§7.4, §6). One hour: a leg that waits longer than that to retry, or
/// to release an idle implicit lock, is indistinguishable from a dead one, and the
/// operator who typed an extra three digits learns it at load rather than by watching
/// a console that never comes back.
pub const MAX_TIMER_MS: u64 = 3_600_000;

/// The largest accepted log `rotation_padding`, in digits (§7.3). The rotation
/// counter is a `u64`, so at most twenty digits can ever be significant; beyond that
/// the padding is pure filename noise.
pub const MAX_ROTATION_PADDING: u8 = 20;

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

    /// Whether this configuration declares nothing at all.
    ///
    /// An empty graph is perfectly legal — `teardown` persists one deliberately —
    /// so this is *not* a validation failure. It exists for the `load` path, which
    /// is the only caller that also knows whether the source text was non-empty: an
    /// empty parse of a non-empty file (a comment-only file, or a table name serde
    /// now rejects outright) turns `load --replace` into an unannounced `teardown`
    /// that reports success, which is the one input that breaks §15.8's
    /// operators-own-the-graph guarantee. That caller pairs this with
    /// [`ValidationError::EmptyConfig`].
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.edges.is_empty()
    }

    /// The node declared under `name`, if any.
    fn node_named(&self, name: &str) -> Option<&NodeConfig> {
        self.nodes.iter().find(|n| n.name() == name)
    }

    /// The orientation of an endpoint address, resolved through the declaring
    /// node's shape (§4). `None` for a dangling address — the topology model
    /// reports those as `UnknownNode`/`UnknownEndpoint`.
    fn endpoint_facing(&self, addr: &EndpointAddr) -> Option<Facing> {
        self.node_named(&addr.node)?
            .shape()
            .endpoints
            .iter()
            .find(|e| e.name == addr.endpoint)
            .map(|e| e.facing)
    }

    /// An edge's `(host-facing end, target-facing end)`, or `None` when it is
    /// same-facing or dangling — the cases graph rule 1 and reference integrity
    /// already report, and the cases the data-plane wiring skips.
    fn edge_ends<'e>(&self, edge: &'e EdgeConfig) -> Option<(&'e EndpointAddr, &'e EndpointAddr)> {
        match (
            self.endpoint_facing(&edge.a)?,
            self.endpoint_facing(&edge.b)?,
        ) {
            (Facing::Host, Facing::Target) => Some((&edge.a, &edge.b)),
            (Facing::Target, Facing::Host) => Some((&edge.b, &edge.a)),
            _ => None,
        }
    }

    /// An edge's `(host-facing end, target-facing end)`, public for the daemon's
    /// edge-surgery verbs (§15.35): `connect` and `disconnect` need to know which
    /// end produces and which consumes, and re-deriving that from facings at the
    /// call site is exactly the kind of second implementation §16 exists to
    /// prevent. `None` for a same-facing or dangling edge, which validation
    /// already reports.
    pub fn edge_ends_of<'e>(
        &self,
        edge: &'e EdgeConfig,
    ) -> Option<(&'e EndpointAddr, &'e EndpointAddr)> {
        self.edge_ends(edge)
    }

    /// Whether `addr` is a codec (or exec) node's *multiplexed* endpoint — its
    /// default, empty-named endpoint (§7.5/§7.6, §15.22). The exec codec is an
    /// ordinary codec node selected by `codec = "exec"`, so one test covers both.
    fn is_multiplexed_endpoint(&self, addr: &EndpointAddr) -> bool {
        addr.is_default() && matches!(self.node_named(&addr.node), Some(NodeConfig::Codec { .. }))
    }

    /// Whether the host-facing endpoint at `addr` **arbitrates at all** (§6) — i.e.
    /// whether there is a write lock on it for an edge rule to reason about.
    ///
    /// A `free-for-all` endpoint has none: [`crate::lock::EndpointLock::may_write`]
    /// returns true for every registered origin whose mode is not `never`, so no
    /// origin can park waiting for a grant and no origin can hold the endpoint to
    /// another's exclusion. Both kind-dependent edge rules in [`Self::validate`]
    /// exist to prevent a lock hazard, so both are exempt there — and both said so
    /// in their own words until one grew the exemption and the other did not
    /// (review 32 `CORE-2`). One predicate, consulted twice (§16 "one rule, one
    /// place").
    ///
    /// An unresolvable node is treated as arbitrating: the topology model reports
    /// the dangling reference, and [`Self::edge_ends`] has already filtered such an
    /// edge out before either rule runs.
    fn endpoint_arbitrates(&self, addr: &EndpointAddr) -> bool {
        self.node_named(&addr.node).map(NodeConfig::arbitration) != Some(Arbitration::FreeForAll)
    }

    /// The runtime-effective write mode of `edge` (§6) — what the data plane will
    /// actually register, as opposed to what the operator declared.
    ///
    /// **This is the single source of truth for the two configuration-to-runtime
    /// write-mode promotions** (§16's "one rule, one place"); the daemon's wiring
    /// consumes it rather than re-deriving them:
    ///
    /// 1. **A log target forces `never`.** A log node is inherently read-only
    ///    (§7.3): it gets no targetward path and no lock handle, whatever the edge
    ///    says. A configured value round-trips through `dump`/`load` cosmetically.
    /// 2. **A map's `raw` endpoint promotes `on-demand` to `held`.** A map owns its
    ///    console's writes (§7.8), and a held-origin interior pump cannot run
    ///    on-demand — the lock only ever grants a *held* origin that reclaim path,
    ///    so an on-demand origin would park forever. An omitted `write_mode` is the
    ///    generic `on-demand` default, so it is promoted here; an explicit `never`
    ///    is preserved and makes a read-only/display map.
    ///
    /// Everything else is the declared mode, including a same-facing or dangling
    /// edge (which is never wired at all, so the declared value is the honest
    /// answer).
    pub fn effective_write_mode(&self, edge: &EdgeConfig) -> WriteMode {
        let Some((_host, target)) = self.edge_ends(edge) else {
            return edge.write_mode;
        };
        if matches!(self.node_named(&target.node), Some(NodeConfig::Log { .. })) {
            WriteMode::Never
        } else if edge.write_mode == WriteMode::OnDemand
            && target.endpoint == MAP_RAW_ENDPOINT
            && matches!(self.node_named(&target.node), Some(NodeConfig::Map { .. }))
        {
            WriteMode::Held
        } else {
            edge.write_mode
        }
    }

    /// Full structural validation (§4, §11): duplicate node names (checked on the
    /// node *list*, before the model's shape map collapses them) plus the three
    /// graph rules and name checks. An empty vector means the graph is valid. This
    /// is what `load` runs — [`GraphModel::validate`] alone would miss a duplicate
    /// node name, since the model is keyed by name.
    ///
    /// Everything the topology model cannot see lives here, because §11 wants the
    /// *entire* file judged before anything is created — and, under `load --replace`,
    /// before the running graph is torn down (§15.26). That is three families: the
    /// per-kind attribute checks (leg transport/address, map mapping names, serial
    /// facing), the numeric ranges, and the two edge rules that depend on node kinds
    /// and write modes rather than on endpoint facings.
    pub fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for node in &self.nodes {
            if !seen.insert(node.name()) {
                errors.push(ValidationError::DuplicateNodeName {
                    node: node.name().to_owned(),
                });
            }
            // Every numeric knob is range-checked here, at validation time — before
            // anything is created and, under `load --replace`, before the running
            // graph is torn down (§11 load atomicity, §15.26: a structurally-invalid
            // config must never destroy a good running graph). Two of these were
            // reachable process-killers rather than merely silly values, which is
            // why the whole class moved here instead of one field at a time.
            if let Some(cap) = node.replay_ring() {
                errors.extend(range_error(
                    node.name(),
                    "replay_ring",
                    cap as u64,
                    0,
                    MAX_REPLAY_RING as u64,
                ));
            }
            // A zero-depth hostward buffer builds a rendezvous channel that drops
            // nearly all hostward output even for a fast, fully-present consumer
            // (§5, §7.1/§7.2): the fan-out / writer bridge can admit a chunk only in
            // the instant a consumer is blocked in recv. Give the bounded buffer a
            // floor of one chunk, and a ceiling too — the depth is handed straight to
            // a bounded tokio channel, which panics above its permit ceiling. Only
            // serial and pty carry the tunable.
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
            {
                if *hostward_buffer == 0 {
                    errors.push(ValidationError::ZeroHostwardBuffer { node: name.clone() });
                } else {
                    errors.extend(range_error(
                        name,
                        "hostward_buffer",
                        *hostward_buffer as u64,
                        1,
                        MAX_HOSTWARD_BUFFER as u64,
                    ));
                }
            }
            // Serial-specific config-level checks (§7.1).
            if let NodeConfig::Serial {
                name, faces, baud, ..
            } = node
            {
                // §7.1 describes the output-leg role (`faces = "target"`), but no
                // wiring for it exists: the node would open the device, take
                // TIOCEXCL, and be attached to nothing — a port held hostage with no
                // data path and no diagnostic. Refuse it honestly rather than let it
                // load; implementing the role is deferred work (§14).
                if *faces == Facing::Target {
                    errors.push(ValidationError::SerialFacesTarget { node: name.clone() });
                }
                // Baud has no upper bound on purpose — §7.1/§13 buy custom and
                // nonstandard rates deliberately — but zero is not a rate: on Linux
                // B0 means *hang up the line*, which would quietly contradict §7.1's
                // promise that line states are deterministic.
                errors.extend(range_error(name, "baud", *baud as u64, 1, u32::MAX as u64));
            }
            // A pty's slave permission bits (§7.2). Two rules coincide on one
            // inclusive window, so one `range_error` states both:
            //
            //   * **Ceiling `0o777`** — a permission mode is nine bits. TOML rejects
            //     leading-zero integers, so an operator must write `0o600`; writing
            //     `mode = 666`, the octal spelling read as decimal, means 0o1232 —
            //     owner `-w-`. Every three-digit decimal typo of an octal mode
            //     exceeds 0o777 = 511, so the ceiling names the whole family.
            //   * **Floor `0o600`** — the daemon's own access. `setup` chmods the
            //     slave and then *opens* it to prime the session (`prime_slave`), so
            //     a mode that denies the owner read+write faults the node by
            //     construction, with `prime open /dev/pts/N: EACCES` — a message
            //     that never mentions the mode and sends the operator hunting a
            //     devpts problem. Inside the ceiling, "the owner has rw" is exactly
            //     `mode >= 0o600`, which is why one range carries both rules; and it
            //     catches what the ceiling misses by arithmetic accident (`mode =
            //     154` is 0o232 — no owner read — and passes a 0o777 cap).
            //
            // The floor costs no working configuration: only a root daemon can open
            // a slave whose owner bits deny it, and only root can chown the node to
            // some *other* owner in the first place — a root daemon declaring a mode
            // that locks its own owner out is confessing the typo this rejects.
            if let NodeConfig::Pty {
                name,
                mode: Some(mode),
                ..
            } = node
            {
                errors.extend(range_error(name, "mode", *mode as u64, 0o600, 0o777));
            }
            // A log's rotation-suffix padding (§7.3).
            if let NodeConfig::Log {
                name,
                rotation_padding,
                ..
            } = node
            {
                errors.extend(range_error(
                    name,
                    "rotation_padding",
                    *rotation_padding as u64,
                    0,
                    MAX_ROTATION_PADDING as u64,
                ));
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
                reconnect_initial_ms,
                reconnect_max_ms,
                idle_release_ms,
                channels,
                ..
            } = node
            {
                // The leg's millisecond timers (§7.4, §6). Nothing overflows at the
                // top — `Duration::from_millis(u64::MAX)` is a legal, geological
                // wait — which is exactly the problem: the leg would load, report
                // itself connecting, and never retry. Cap them where "slow" stops
                // being distinguishable from "dead".
                for (field, value) in [
                    ("reconnect_initial_ms", *reconnect_initial_ms),
                    ("reconnect_max_ms", *reconnect_max_ms),
                    ("idle_release_ms", *idle_release_ms),
                ] {
                    errors.extend(range_error(name, field, value, 0, MAX_TIMER_MS));
                }
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

        // Edge-level rules the topology model cannot make: it knows endpoint facings
        // and nothing about node kinds or write modes (§4 keeps it that way on
        // purpose), so both live here, alongside the leg/codec/map config checks.
        let mut held_origin: std::collections::HashMap<&EndpointAddr, &EndpointAddr> =
            std::collections::HashMap::new();
        for (i, edge) in self.edges.iter().enumerate() {
            let Some((host, target)) = self.edge_ends(edge) else {
                continue;
            };
            // A codec's (or exec's) multiplexed side takes its bytes through a
            // held-origin pump: the lock's held-reclaim path only ever grants a
            // `Held` origin, so an `on-demand` origin parks on its first chunk
            // forever while `send` cheerfully reports success — targetward bytes
            // accepted, acknowledged, and lost, which §5 forbids outright. `held` is
            // the working mode and `never` is the deliberate read-only one; anything
            // else is an operator error worth naming, not a silent stall (§7.5/§7.6).
            // Only the *target-facing* multiplexed side is constrained: a
            // re-multiplexer's mux endpoint faces host and is an ordinary arbitrated
            // endpoint, and a codec's channel endpoints are ordinary origins. And
            // the whole hazard presumes a lock upstream: on a `free-for-all` host
            // endpoint `held` and `on-demand` are behaviourally identical, so the
            // refusal would reject a graph that runs, citing a stall that cannot
            // happen there (§6, review 32 `CORE-2`).
            if self.is_multiplexed_endpoint(target)
                && self.endpoint_arbitrates(host)
                && !matches!(edge.write_mode, WriteMode::Held | WriteMode::Never)
            {
                errors.push(ValidationError::MultiplexedEdgeNotHeld {
                    edge: i,
                    origin: host.clone(),
                    mux: target.clone(),
                    write_mode: edge.write_mode,
                });
            }
            // At most one *effectively* held origin per host-facing endpoint (§6).
            // "Held indefinitely" is a promise only one origin can be given: the lock
            // picks a winner arbitrarily, and the loser can never write, never joins
            // the waiter queue, and appears nowhere in `state` as blocked. Going
            // through `effective_write_mode` (rather than the declared value) is what
            // catches the reachable shape — two maps attached to one upstream
            // endpoint with `write_mode` written nowhere, both promoted to held.
            if self.effective_write_mode(edge) == WriteMode::Held
                // A `free-for-all` endpoint has no lock at all (§6), so two held
                // origins there are the operator's explicit, working choice — the
                // same exemption the mux rule above takes, through the same
                // predicate so it cannot be applied to one and forgotten on the
                // other again.
                && self.endpoint_arbitrates(host)
            {
                match held_origin.entry(host) {
                    std::collections::hash_map::Entry::Occupied(prior) => {
                        errors.push(ValidationError::ConflictingHeldEdges {
                            endpoint: host.clone(),
                            first: (*prior.get()).clone(),
                            second: target.clone(),
                        });
                    }
                    std::collections::hash_map::Entry::Vacant(slot) => {
                        slot.insert(target);
                    }
                }
            }
        }

        errors.extend(self.to_model().validate());
        errors
    }
}

/// One inclusive range check, as a [`ValidationError::NumericOutOfRange`] or
/// nothing. A free function rather than a closure so the per-node checks in
/// [`GraphConfig::validate`] can keep pushing into the same error vector.
fn range_error(
    node: &str,
    field: &'static str,
    value: u64,
    min: u64,
    max: u64,
) -> Option<ValidationError> {
    (value < min || value > max).then(|| ValidationError::NumericOutOfRange {
        node: node.to_owned(),
        field,
        value,
        min,
        max,
    })
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
/// `deny_unknown_fields`, as everywhere in this file: §11 validates the entire file
/// before anything is created, and a key nobody reads is a validation that silently
/// did not happen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
///
/// `deny_unknown_fields` is a *container* attribute and applies to every variant's
/// field set, so `advertized_baud = 9600` is now a load error naming the key instead
/// of a typo the daemon ignores while `dump` shows the unchanged default (§11). The
/// `type` tag itself is stripped before a variant is deserialized, so internal
/// tagging and the denial coexist — pinned by `unknown_node_field_is_rejected`. The
/// codec's `attributes` table stays open by construction (§8): it is one opaque
/// field the codec validates against its own schema, not a field set we know.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
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
///
/// Kebab-case is canonical, so `dump` round-trips unchanged, and §7.1 now says so.
/// The unhyphenated `xonxoff` / `rtscts` stay accepted as aliases because that is
/// what §7.1 used to spell and what operators wrote against it: without them the
/// design's own spellings failed to deserialize, and because that is a parse error
/// it rejected the *entire* configuration file (CFG-1). Both spellings are
/// accepted; only one is emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FlowControl {
    #[default]
    None,
    #[serde(alias = "xonxoff")]
    XonXoff,
    #[serde(alias = "rtscts")]
    RtsCts,
}

/// Initial modem-line assertions for a serial node (§7.1). Applied at open and
/// (in phase 7) re-applied on reopen, so line states are deterministic against
/// auto-reset adapters. Each line may be asserted (`true`), deasserted (`false`),
/// or left untouched (omitted — the default, which keeps the driver's power-on
/// state). `set-modem`/`pulse-dtr` control verbs (§7.1) act on the live port
/// later; these are the *initial* states configuration owns and round-trips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

    /// A serial node with everything at its default but the named overrides — the
    /// building block for the range tests below.
    fn serial(name: &str) -> NodeConfig {
        NodeConfig::Serial {
            name: name.into(),
            device: "/dev/ttyUSB0".into(),
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
        }
    }

    /// [`serial`] with one field bent out of shape — the range tests all want a
    /// valid node with exactly one offender in it.
    fn serial_with(name: &str, bend: impl FnOnce(&mut NodeConfig)) -> NodeConfig {
        let mut node = serial(name);
        bend(&mut node);
        node
    }

    fn errors_of(nodes: Vec<NodeConfig>, edges: Vec<EdgeConfig>) -> Vec<ValidationError> {
        GraphConfig { nodes, edges }.validate()
    }

    #[test]
    fn replay_ring_is_range_checked() {
        // §11/§15.26: an absurd ring depth must fail *validation*, not the allocator.
        // Left unchecked it loads cleanly, aborts the process on the first hostward
        // byte (the ring allocates lazily), and — since `load` persists config —
        // crash-loops on every restart. Every node kind that carries the attribute
        // is covered, via NodeConfig::replay_ring().
        let over = MAX_REPLAY_RING + 1;
        let too_big = |node: NodeConfig| {
            let name = node.name().to_owned();
            errors_of(vec![node], vec![]).iter().any(|e| {
                matches!(e, ValidationError::NumericOutOfRange { node, field: "replay_ring", value, .. }
                    if *node == name && *value == over as u64)
            })
        };
        assert!(too_big(serial_with("usb0", |n| {
            if let NodeConfig::Serial { replay_ring, .. } = n {
                *replay_ring = over;
            }
        })));
        assert!(too_big(NodeConfig::Codec {
            name: "mux".into(),
            codec: "reference".into(),
            faces: Facing::Target,
            channels: vec!["c0".into()],
            arbitration: Arbitration::Exclusive,
            replay_ring: over,
            attributes: toml::Table::new(),
        }));
        assert!(too_big(NodeConfig::Map {
            name: "m".into(),
            hostward: vec![],
            targetward: vec![],
            arbitration: Arbitration::Exclusive,
            replay_ring: over,
        }));
        assert!(too_big(NodeConfig::Leg {
            name: "uplink".into(),
            faces: Facing::Host,
            transport: Transport::Unix,
            role: LegRole::Listen,
            address: "/run/snx/leg.sock".into(),
            insecure_bind: false,
            reconnect_initial_ms: 200,
            reconnect_max_ms: 5_000,
            idle_release_ms: 1_000,
            purge_on_reconnect: true,
            arbitration: Arbitration::Exclusive,
            replay_ring: over,
            channels: vec!["c0".into()],
        }));

        // The two ends of the accepted range: `0` is the documented opt-out and the
        // cap itself is legal, so neither is reported.
        for cap in [0, MAX_REPLAY_RING] {
            let node = serial_with("usb0", |n| {
                if let NodeConfig::Serial { replay_ring, .. } = n {
                    *replay_ring = cap;
                }
            });
            assert!(
                !errors_of(vec![node], vec![])
                    .iter()
                    .any(|e| matches!(e, ValidationError::NumericOutOfRange { .. })),
                "replay_ring = {cap} must be accepted"
            );
        }
    }

    #[test]
    fn oversize_hostward_buffer_is_rejected() {
        // The zero case keeps its own message (ZeroHostwardBuffer); the ceiling is
        // new. Unbounded, the depth panics inside tokio's bounded channel — and under
        // `load --replace` that panic lands *after* the good graph is gone (§15.26).
        let pty = |depth: usize| NodeConfig::Pty {
            name: "console".into(),
            path: "/run/serial_nexus/console".into(),
            owner: None,
            group: None,
            mode: None,
            advertised_baud: 115_200,
            hostward_buffer: depth,
        };
        let over = MAX_HOSTWARD_BUFFER + 1;
        assert!(
            errors_of(vec![pty(over)], vec![]).iter().any(|e| {
                matches!(e, ValidationError::NumericOutOfRange { field: "hostward_buffer", value, max, .. }
                    if *value == over as u64 && *max == MAX_HOSTWARD_BUFFER as u64)
            }),
            "an oversize hostward_buffer must be structural"
        );
        // The cap itself is accepted, and the zero case still reports the dedicated
        // message rather than the generic range one.
        assert!(
            errors_of(vec![pty(MAX_HOSTWARD_BUFFER)], vec![]).is_empty(),
            "hostward_buffer = MAX is accepted"
        );
        let zero = errors_of(vec![pty(0)], vec![]);
        assert!(
            zero.iter()
                .any(|e| matches!(e, ValidationError::ZeroHostwardBuffer { .. }))
                && !zero
                    .iter()
                    .any(|e| matches!(e, ValidationError::NumericOutOfRange { .. })),
            "zero keeps its own message: {zero:?}"
        );
    }

    #[test]
    fn leg_timers_and_log_padding_and_baud_are_range_checked() {
        // The rest of the numeric surface (§7.4, §7.3, §7.1). None of these can crash
        // the daemon; all of them load a graph that is quietly useless, which §11's
        // "validated before anything is created" exists to prevent.
        let leg = |initial: u64, max: u64, idle: u64| NodeConfig::Leg {
            name: "uplink".into(),
            faces: Facing::Target,
            transport: Transport::Unix,
            role: LegRole::Connect,
            address: "/run/snx/leg.sock".into(),
            insecure_bind: false,
            reconnect_initial_ms: initial,
            reconnect_max_ms: max,
            idle_release_ms: idle,
            purge_on_reconnect: true,
            arbitration: Arbitration::Exclusive,
            replay_ring: DEFAULT_REPLAY_RING,
            channels: vec!["c0".into()],
        };
        let field_reported = |node: NodeConfig, field: &str| {
            errors_of(vec![node], vec![]).iter().any(
                |e| matches!(e, ValidationError::NumericOutOfRange { field: f, .. } if *f == field),
            )
        };
        assert!(field_reported(
            leg(MAX_TIMER_MS + 1, 5_000, 1_000),
            "reconnect_initial_ms"
        ));
        assert!(field_reported(
            leg(200, u64::MAX, 1_000),
            "reconnect_max_ms"
        ));
        assert!(field_reported(
            leg(200, 5_000, MAX_TIMER_MS + 1),
            "idle_release_ms"
        ));
        assert!(
            errors_of(vec![leg(0, MAX_TIMER_MS, 0)], vec![]).is_empty(),
            "the range ends are legal (0 is a valid immediate release)"
        );

        // A log's rotation padding: a u64 counter is at most twenty digits wide.
        let log = |padding: u8| NodeConfig::Log {
            name: "cap".into(),
            directory: "/tmp".into(),
            filename: "c.log".into(),
            overflow: OverflowPolicy::DropOldest,
            rotation_padding: padding,
        };
        assert!(field_reported(
            log(MAX_ROTATION_PADDING + 1),
            "rotation_padding"
        ));
        assert!(errors_of(vec![log(MAX_ROTATION_PADDING)], vec![]).is_empty());

        // Baud: no ceiling (§7.1/§13 buy nonstandard rates deliberately), but B0
        // means "hang up the line" on Linux, not "zero symbols per second".
        let zero_baud = serial_with("usb0", |n| {
            if let NodeConfig::Serial { baud, .. } = n {
                *baud = 0;
            }
        });
        assert!(field_reported(zero_baud, "baud"));
        // A wildly nonstandard but positive rate stays legal.
        assert!(errors_of(vec![serial("usb0")], vec![]).is_empty());
    }

    #[test]
    fn serial_faces_target_is_rejected_as_unimplemented() {
        // §7.1 describes the output-leg role; no wiring for it exists, so such a node
        // would seize the port with TIOCEXCL and be attached to nothing. Refused
        // structurally and named, with the deferral (§14) stated in the message.
        let node = serial_with("outleg", |n| {
            if let NodeConfig::Serial { faces, .. } = n {
                *faces = Facing::Target;
            }
        });
        let errs = errors_of(vec![node], vec![]);
        assert!(
            errs.iter().any(
                |e| matches!(e, ValidationError::SerialFacesTarget { node } if node == "outleg")
            ),
            "expected SerialFacesTarget, got {errs:?}"
        );
        let msg = errs
            .iter()
            .find(|e| matches!(e, ValidationError::SerialFacesTarget { .. }))
            .unwrap()
            .to_string();
        assert!(
            msg.contains("not implemented") && msg.contains("§14"),
            "the message must be honest about the deferral: {msg}"
        );

        // A codec's and a leg's `faces = "target"` are ordinary and must keep working.
        assert!(
            errors_of(
                vec![NodeConfig::Codec {
                    name: "mux".into(),
                    codec: "reference".into(),
                    faces: Facing::Target,
                    channels: vec!["c0".into()],
                    arbitration: Arbitration::Exclusive,
                    replay_ring: DEFAULT_REPLAY_RING,
                    attributes: toml::Table::new(),
                }],
                vec![]
            )
            .is_empty(),
            "a demux codec faces target normally"
        );
    }

    #[test]
    fn codec_multiplexed_edge_must_be_held_or_never() {
        // §5's no-drop invariant, made structural: the multiplexed side's targetward
        // pump is only ever granted the lock as a *held* origin, so an on-demand
        // origin parks on its first chunk forever while `send` reports success.
        let nodes = || {
            vec![
                serial("usb0"),
                NodeConfig::Codec {
                    name: "mux".into(),
                    codec: "reference".into(),
                    faces: Facing::Target,
                    channels: vec!["c0".into()],
                    arbitration: Arbitration::Exclusive,
                    replay_ring: DEFAULT_REPLAY_RING,
                    attributes: toml::Table::new(),
                },
            ]
        };
        let mux_edge = |write_mode: WriteMode| EdgeConfig {
            a: EndpointAddr::node("usb0"),
            b: EndpointAddr::node("mux"),
            write_mode,
        };
        // The reported shape: an omitted write_mode is the generic on-demand default.
        assert_eq!(
            WriteMode::default(),
            WriteMode::OnDemand,
            "the premise: an omitted write_mode is on-demand"
        );
        let errs = errors_of(nodes(), vec![mux_edge(WriteMode::OnDemand)]);
        assert!(
            errs.iter().any(|e| matches!(
                e,
                ValidationError::MultiplexedEdgeNotHeld { edge: 0, origin, mux, write_mode: WriteMode::OnDemand }
                    if origin.to_string() == "usb0" && mux.to_string() == "mux"
            )),
            "expected MultiplexedEdgeNotHeld naming both endpoints, got {errs:?}"
        );

        // `held` is the working mode; `never` is the deliberate read-only one.
        for mode in [WriteMode::Held, WriteMode::Never] {
            assert!(
                errors_of(nodes(), vec![mux_edge(mode)]).is_empty(),
                "write_mode = {mode} must be accepted on a multiplexed edge"
            );
        }

        // The rule is about the *multiplexed* endpoint only: a codec's channel
        // endpoints are ordinary host-facing endpoints and stay on-demand-able.
        let mut nodes = nodes();
        nodes.push(NodeConfig::Pty {
            name: "con".into(),
            path: "/run/serial_nexus/con".into(),
            owner: None,
            group: None,
            mode: None,
            advertised_baud: 115_200,
            hostward_buffer: 32,
        });
        assert!(
            errors_of(
                nodes,
                vec![
                    mux_edge(WriteMode::Held),
                    EdgeConfig {
                        a: EndpointAddr::channel("mux", "c0"),
                        b: EndpointAddr::node("con"),
                        write_mode: WriteMode::OnDemand,
                    },
                ],
            )
            .is_empty(),
            "a codec channel edge is an ordinary on-demand origin"
        );
    }

    /// CORE-2: both kind-dependent edge rules are exemptions from the *same*
    /// condition — "does this host endpoint arbitrate at all?" — and the mux rule
    /// used to ignore it while the held-uniqueness rule ten lines below took it.
    #[test]
    fn a_free_for_all_host_endpoint_exempts_both_edge_rules() {
        // On a `free-for-all` endpoint there is no lock: `EndpointLock::may_write`
        // is true for every origin that is not `never`, so an on-demand mux origin
        // cannot park and two held origins cannot starve each other. The refusal
        // would reject a graph that runs, with a stated reason that is false for it
        // (§6, design §6's "both corollaries are conditioned on the endpoint
        // arbitrating at all").
        let nodes = || {
            vec![
                serial_with("usb0", |n| {
                    if let NodeConfig::Serial { arbitration, .. } = n {
                        *arbitration = Arbitration::FreeForAll;
                    }
                }),
                NodeConfig::Codec {
                    name: "mux".into(),
                    codec: "reference".into(),
                    faces: Facing::Target,
                    channels: vec!["c0".into()],
                    arbitration: Arbitration::Exclusive,
                    replay_ring: DEFAULT_REPLAY_RING,
                    attributes: toml::Table::new(),
                },
                NodeConfig::Codec {
                    name: "mux2".into(),
                    codec: "reference".into(),
                    faces: Facing::Target,
                    channels: vec!["c0".into()],
                    arbitration: Arbitration::Exclusive,
                    replay_ring: DEFAULT_REPLAY_RING,
                    attributes: toml::Table::new(),
                },
            ]
        };
        let mux_edge = |mux: &str, write_mode: WriteMode| EdgeConfig {
            a: EndpointAddr::node("usb0"),
            b: EndpointAddr::node(mux),
            write_mode,
        };
        // The rule under repair: an omitted (on-demand) mux write mode.
        assert!(
            errors_of(nodes(), vec![mux_edge("mux", WriteMode::OnDemand)]).is_empty(),
            "an on-demand mux edge on a lock-less endpoint must be accepted"
        );
        // The sibling rule's exemption, unchanged — pinned here so the two stay
        // visibly on one predicate.
        assert!(
            errors_of(
                nodes(),
                vec![
                    mux_edge("mux", WriteMode::Held),
                    mux_edge("mux2", WriteMode::Held),
                ],
            )
            .is_empty(),
            "two held origins on a lock-less endpoint are the operator's choice"
        );
        // And the exemption is *only* about arbitration: the same graph under the
        // exclusive default is still refused, by both rules.
        let exclusive = || {
            let mut nodes = nodes();
            if let Some(NodeConfig::Serial { arbitration, .. }) = nodes.first_mut() {
                *arbitration = Arbitration::Exclusive;
            }
            nodes
        };
        assert!(
            errors_of(exclusive(), vec![mux_edge("mux", WriteMode::OnDemand)])
                .iter()
                .any(|e| matches!(e, ValidationError::MultiplexedEdgeNotHeld { .. })),
            "an arbitrated endpoint still refuses an on-demand mux edge"
        );
        assert!(
            errors_of(
                exclusive(),
                vec![
                    mux_edge("mux", WriteMode::Held),
                    mux_edge("mux2", WriteMode::Held),
                ],
            )
            .iter()
            .any(|e| matches!(e, ValidationError::ConflictingHeldEdges { .. })),
            "an arbitrated endpoint still refuses two held origins"
        );
    }

    /// RV-4: a pty `mode` is range-checked structurally, so the octal/decimal typo
    /// is named at `load` instead of faulting the node with an EACCES that never
    /// mentions the mode.
    #[test]
    fn pty_mode_is_range_checked() {
        let pty = |mode: Option<u32>| NodeConfig::Pty {
            name: "con".into(),
            path: "/run/serial_nexus/con".into(),
            owner: None,
            group: None,
            mode,
            advertised_baud: 115_200,
            hostward_buffer: 32,
        };
        let rejected = |mode: u32| {
            errors_of(vec![pty(Some(mode))], vec![]).iter().any(|e| {
                matches!(e, ValidationError::NumericOutOfRange { node, field: "mode", value, .. }
                    if node == "con" && *value == mode as u64)
            })
        };
        // The reported typo: `0o600` written as `600`… and as `666`, `644`, `640`.
        // Every three-digit decimal spelling of an octal mode exceeds 0o777.
        for typo in [600, 666, 644, 640, 660, 777] {
            assert!(
                rejected(typo),
                "mode = {typo} (decimal octal typo) must fail"
            );
        }
        // What the ceiling alone would miss (the verifier's case): 154 = 0o232,
        // owner `-w-`, which the daemon's own `prime_slave` open cannot use.
        assert!(
            rejected(154),
            "a mode denying the owner read+write must fail"
        );
        assert!(rejected(0o477), "owner without write must fail");
        assert!(rejected(0o277), "owner without read must fail");
        // The window, and the two defaults `PtyNode::create` applies when `mode` is
        // omitted (0o600 alone, 0o660 with a group) — neither may be refusable.
        for mode in [0o600, 0o660, 0o666, 0o700, 0o777] {
            assert!(
                !errors_of(vec![pty(Some(mode))], vec![])
                    .iter()
                    .any(|e| matches!(e, ValidationError::NumericOutOfRange { .. })),
                "mode = {mode:o} must be accepted"
            );
        }
        // An omitted mode is not a number and cannot be out of range.
        assert!(
            !errors_of(vec![pty(None)], vec![])
                .iter()
                .any(|e| matches!(e, ValidationError::NumericOutOfRange { .. })),
            "an omitted mode must be accepted"
        );
    }

    #[test]
    fn effective_write_mode_reproduces_the_runtime_promotions() {
        // The single source of truth for the two promotions the data plane applies
        // (§16, one rule one place). A log target forces `never`; a map's raw
        // endpoint promotes an omitted/on-demand mode to `held`; everything else is
        // the declared mode verbatim.
        let cfg = GraphConfig {
            nodes: vec![
                serial("usb0"),
                NodeConfig::Log {
                    name: "cap".into(),
                    directory: "/tmp".into(),
                    filename: "c.log".into(),
                    overflow: OverflowPolicy::DropOldest,
                    rotation_padding: 3,
                },
                NodeConfig::Map {
                    name: "m".into(),
                    hostward: vec![],
                    targetward: vec![],
                    arbitration: Arbitration::Exclusive,
                    replay_ring: DEFAULT_REPLAY_RING,
                },
                NodeConfig::Pty {
                    name: "con".into(),
                    path: "/run/serial_nexus/con".into(),
                    owner: None,
                    group: None,
                    mode: None,
                    advertised_baud: 115_200,
                    hostward_buffer: 32,
                },
            ],
            edges: vec![],
        };
        let mode = |a: EndpointAddr, b: EndpointAddr, declared: WriteMode| {
            cfg.effective_write_mode(&EdgeConfig {
                a,
                b,
                write_mode: declared,
            })
        };

        // 1. A log target is forced to `never`, whatever the edge declares.
        for declared in [WriteMode::Never, WriteMode::OnDemand, WriteMode::Held] {
            assert_eq!(
                mode(
                    EndpointAddr::node("usb0"),
                    EndpointAddr::node("cap"),
                    declared
                ),
                WriteMode::Never,
                "a log target forces never (declared {declared})"
            );
        }
        // 2. A map's raw endpoint promotes on-demand (the omitted default) to held,
        //    passes an explicit held through, and preserves `never` (a read-only map).
        let raw = || EndpointAddr::channel("m", MAP_RAW_ENDPOINT);
        assert_eq!(
            mode(EndpointAddr::node("usb0"), raw(), WriteMode::OnDemand),
            WriteMode::Held
        );
        assert_eq!(
            mode(EndpointAddr::node("usb0"), raw(), WriteMode::Held),
            WriteMode::Held
        );
        assert_eq!(
            mode(EndpointAddr::node("usb0"), raw(), WriteMode::Never),
            WriteMode::Never
        );
        // 3. Everything else is the declared mode — including the map's *mapped*
        //    (host-facing, default) endpoint, which is an ordinary origin.
        for declared in [WriteMode::Never, WriteMode::OnDemand, WriteMode::Held] {
            assert_eq!(
                mode(EndpointAddr::node("m"), EndpointAddr::node("con"), declared),
                declared,
                "an ordinary edge keeps its declared mode"
            );
        }
        // 4. A dangling edge is never wired at all, so the declared value is honest.
        assert_eq!(
            mode(
                EndpointAddr::node("ghost"),
                EndpointAddr::node("con"),
                WriteMode::OnDemand
            ),
            WriteMode::OnDemand
        );
    }

    #[test]
    fn two_held_origins_on_one_endpoint_are_rejected() {
        // §6: "held" is acquire-on-attach and held *indefinitely*, which two origins
        // cannot both have — the lock picks a winner arbitrarily and the loser can
        // never write, never queues as a waiter, and shows up nowhere in `state`.
        let map = |name: &str| NodeConfig::Map {
            name: name.into(),
            hostward: vec![],
            targetward: vec![],
            arbitration: Arbitration::Exclusive,
            replay_ring: DEFAULT_REPLAY_RING,
        };
        // The reachable shape: two maps on one upstream endpoint with `write_mode`
        // written NOWHERE — both promoted to held by the §7.8 rule. This is why the
        // rule consults effective_write_mode rather than the declared value.
        let edges = vec![
            EdgeConfig {
                a: EndpointAddr::node("usb0"),
                b: EndpointAddr::channel("m1", MAP_RAW_ENDPOINT),
                write_mode: WriteMode::default(),
            },
            EdgeConfig {
                a: EndpointAddr::node("usb0"),
                b: EndpointAddr::channel("m2", MAP_RAW_ENDPOINT),
                write_mode: WriteMode::default(),
            },
        ];
        let errs = errors_of(vec![serial("usb0"), map("m1"), map("m2")], edges.clone());
        assert!(
            errs.iter().any(|e| matches!(
                e,
                ValidationError::ConflictingHeldEdges { endpoint, first, second }
                    if endpoint.to_string() == "usb0"
                        && first.to_string() == "m1/raw"
                        && second.to_string() == "m2/raw"
            )),
            "expected ConflictingHeldEdges naming BOTH offenders, got {errs:?}"
        );

        // A free-for-all endpoint has no lock at all (§6), so two held origins there
        // are the operator's explicit, working choice — exempt.
        let ffa = serial_with("usb0", |n| {
            if let NodeConfig::Serial { arbitration, .. } = n {
                *arbitration = Arbitration::FreeForAll;
            }
        });
        assert!(
            !errors_of(vec![ffa, map("m1"), map("m2")], edges)
                .iter()
                .any(|e| matches!(e, ValidationError::ConflictingHeldEdges { .. })),
            "a free-for-all endpoint is exempt"
        );

        // One held origin alongside any number of on-demand/never ones is the normal,
        // legal shape (the demux topology) and must stay silent.
        assert!(
            errors_of(
                vec![
                    serial("usb0"),
                    map("m1"),
                    NodeConfig::Pty {
                        name: "spy".into(),
                        path: "/run/serial_nexus/spy".into(),
                        owner: None,
                        group: None,
                        mode: None,
                        advertised_baud: 115_200,
                        hostward_buffer: 32,
                    },
                ],
                vec![
                    EdgeConfig {
                        a: EndpointAddr::node("usb0"),
                        b: EndpointAddr::channel("m1", MAP_RAW_ENDPOINT),
                        write_mode: WriteMode::Held,
                    },
                    EdgeConfig {
                        a: EndpointAddr::node("usb0"),
                        b: EndpointAddr::node("spy"),
                        write_mode: WriteMode::OnDemand,
                    },
                ],
            )
            .is_empty(),
            "one held origin plus on-demand fan-out is the normal shape"
        );
    }

    #[test]
    fn unknown_node_field_is_rejected() {
        // CP-2/CFG-3 (§11): a key nobody reads is a validation that silently did not
        // happen — and `[[nodez]]` used to parse to an empty graph, which turns
        // `load --replace` into an unannounced teardown reporting success.
        //
        // (a) A valid config still parses, tag and all.
        let ok: GraphConfig = toml::from_str(
            r#"
            [[node]]
            type = "pty"
            name = "console"
            path = "/tmp/c"
            advertised_baud = 9600
            [[edge]]
            a = "usb0"
            b = "console"
        "#,
        )
        .expect("a valid config must still parse (the `type` tag is accepted)");
        assert_eq!(ok.nodes.len(), 1);
        assert_eq!(ok.edges.len(), 1);

        // (b) A misspelled field is rejected, naming the offending key.
        let err = toml::from_str::<GraphConfig>(
            r#"
            [[node]]
            type = "pty"
            name = "console"
            path = "/tmp/c"
            advertized_baud = 9600
        "#,
        )
        .expect_err("a misspelled field must be rejected");
        assert!(
            err.to_string().contains("advertized_baud"),
            "the parse error must name the offending key: {err}"
        );

        // (c) A misspelled top-level table is rejected too, rather than parsing to
        //     the empty graph that made `--replace` a silent teardown.
        let err = toml::from_str::<GraphConfig>(
            r#"
            [[nodez]]
            type = "log"
            name = "x"
        "#,
        )
        .expect_err("a misspelled table name must be rejected");
        assert!(
            err.to_string().contains("nodez"),
            "the parse error must name the offending table: {err}"
        );

        // An unknown edge key is caught the same way.
        assert!(
            toml::from_str::<GraphConfig>(
                r#"
                [[edge]]
                a = "x"
                b = "y"
                write_moad = "held"
            "#,
            )
            .is_err(),
            "an unknown edge key must be rejected"
        );

        // A codec's `attributes` table stays OPEN by construction (§8): the codec
        // validates it against its own schema, so arbitrary keys must survive.
        let codec: GraphConfig = toml::from_str(
            r#"
            [[node]]
            type = "codec"
            name = "mux"
            codec = "exec"
            channels = ["c0"]
            [node.attributes]
            argv = ["/usr/bin/thing", "--flag"]
            anything_at_all = 7
        "#,
        )
        .expect("codec attributes are opaque and stay open (§8)");
        match &codec.nodes[0] {
            NodeConfig::Codec { attributes, .. } => {
                assert!(attributes.contains_key("anything_at_all"))
            }
            other => panic!("expected codec, got {other:?}"),
        }
    }

    #[test]
    fn empty_parse_is_detectable() {
        // The other half of CP-2/CFG-3: a comment-only file parses to an empty graph,
        // which `load --replace` would turn into a silent teardown. An empty graph is
        // legal (teardown persists one), so this is a query the load path pairs with
        // "was the source text non-empty?" — not a validation failure.
        let commented: GraphConfig = toml::from_str("# nothing here\n").expect("parse");
        assert!(commented.is_empty());
        assert!(
            commented.validate().is_empty(),
            "an empty graph is structurally valid; only `load` judges it"
        );
        assert!(
            !GraphConfig {
                nodes: vec![serial("usb0")],
                edges: vec![],
            }
            .is_empty()
        );
    }

    #[test]
    fn flow_control_accepts_the_designs_spellings_and_dumps_kebab_case() {
        // §7.1's original spellings were `xonxoff` and `rtscts`; the canonical serde
        // form — and what §7.1 says today — is kebab-case. Both parse, so a config
        // written against either reads, and `dump` still emits the canonical
        // spelling so round-trips are unchanged.
        let parse = |spelling: &str| -> FlowControl {
            let cfg: GraphConfig = toml::from_str(&format!(
                r#"
                [[node]]
                type = "serial"
                name = "usb0"
                device = "/dev/ttyUSB0"
                flow_control = "{spelling}"
            "#
            ))
            .unwrap_or_else(|e| panic!("flow_control = {spelling:?} must parse: {e}"));
            match &cfg.nodes[0] {
                NodeConfig::Serial { flow_control, .. } => *flow_control,
                other => panic!("expected serial, got {other:?}"),
            }
        };
        assert_eq!(parse("xon-xoff"), FlowControl::XonXoff);
        assert_eq!(
            parse("xonxoff"),
            FlowControl::XonXoff,
            "the retired spelling"
        );
        assert_eq!(parse("rts-cts"), FlowControl::RtsCts);
        assert_eq!(parse("rtscts"), FlowControl::RtsCts, "§7.1's spelling");
        assert_eq!(parse("none"), FlowControl::None);

        // Dump canonicalisation: whichever spelling was loaded, `dump` emits the
        // kebab-case one, so a dump→load cycle is stable.
        let dumped = toml::to_string(&GraphConfig {
            nodes: vec![NodeConfig::Serial {
                name: "usb0".into(),
                device: "/dev/ttyUSB0".into(),
                baud: 115_200,
                data_bits: DataBits::Eight,
                parity: Parity::None,
                stop_bits: StopBits::One,
                flow_control: FlowControl::XonXoff,
                faces: Facing::Host,
                arbitration: Arbitration::Exclusive,
                hostward_buffer: 256,
                purge_on_reconnect: true,
                replay_ring: DEFAULT_REPLAY_RING,
                modem: ModemLines::default(),
            }],
            edges: vec![],
        })
        .expect("serialize");
        assert!(
            dumped.contains("flow_control = \"xon-xoff\""),
            "dump stays kebab-case: {dumped}"
        );
    }

    #[test]
    fn whitespace_only_channel_identity_is_rejected() {
        // AGENTS §6 invariant 7 / §12: a whitespace-only identity is the empty one in
        // disguise. Config-level proof for a leg's operator-declared channel list;
        // the node-name and codec-channel cases live in graph.rs's model tests.
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
                channels: vec![" ".into()],
            }],
            edges: vec![],
        };
        assert!(
            cfg.validate().iter().any(|e| matches!(
                e,
                ValidationError::BlankName { node, endpoint: Some(c) }
                    if node == "uplink" && c == " "
            )),
            "expected BlankName for a whitespace channel identity, got {:?}",
            cfg.validate()
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
        /// The guard the review asked for by name: run the numeric surface over its
        /// whole domain — including the values that abort the allocator and panic
        /// tokio — and pin the accept/reject split *exactly*, boundaries included.
        /// A range check that is merely "present" is not the property; a range check
        /// that fires on precisely the out-of-range values is (§11).
        #[test]
        fn prop_numeric_ranges_are_enforced_exactly(
            ring in prop_oneof![0usize..=MAX_REPLAY_RING, (MAX_REPLAY_RING + 1)..=usize::MAX],
            depth in prop_oneof![0usize..=MAX_HOSTWARD_BUFFER, (MAX_HOSTWARD_BUFFER + 1)..=usize::MAX],
            timer in prop_oneof![0u64..=MAX_TIMER_MS, (MAX_TIMER_MS + 1)..=u64::MAX],
            mode in prop_oneof![0u32..0o600, 0o600u32..=0o777, 0o1000u32..=u32::MAX],
        ) {
            let cfg = GraphConfig {
                nodes: vec![
                    serial_with("usb0", |n| {
                        if let NodeConfig::Serial { replay_ring, hostward_buffer, .. } = n {
                            *replay_ring = ring;
                            *hostward_buffer = depth;
                        }
                    }),
                    NodeConfig::Leg {
                        name: "uplink".into(),
                        faces: Facing::Target,
                        transport: Transport::Unix,
                        role: LegRole::Connect,
                        address: "/run/snx/leg.sock".into(),
                        insecure_bind: false,
                        reconnect_initial_ms: timer,
                        reconnect_max_ms: timer,
                        idle_release_ms: timer,
                        purge_on_reconnect: true,
                        arbitration: Arbitration::Exclusive,
                        replay_ring: DEFAULT_REPLAY_RING,
                        channels: vec!["c0".into()],
                    },
                    NodeConfig::Pty {
                        name: "con".into(),
                        path: "/run/serial_nexus/con".into(),
                        owner: None,
                        group: None,
                        mode: Some(mode),
                        advertised_baud: 115_200,
                        hostward_buffer: 32,
                    },
                ],
                edges: vec![],
            };
            let errors = cfg.validate();
            let reported = |field: &str| {
                errors.iter().any(|e| matches!(
                    e,
                    ValidationError::NumericOutOfRange { field: f, .. } if *f == field
                ))
            };
            prop_assert_eq!(reported("replay_ring"), ring > MAX_REPLAY_RING);
            prop_assert_eq!(reported("idle_release_ms"), timer > MAX_TIMER_MS);
            // The zero depth keeps its own, more specific message, so the generic
            // range error must NOT also fire there.
            prop_assert_eq!(reported("hostward_buffer"), depth > MAX_HOSTWARD_BUFFER);
            prop_assert_eq!(
                errors.iter().any(|e| matches!(e, ValidationError::ZeroHostwardBuffer { .. })),
                depth == 0
            );
            // RV-4: the accepted window is `0o600 ..= 0o777` — the ceiling is a
            // permission mode's bit width (which catches the decimal-octal typo
            // family) and the floor is the daemon's own `prime_slave` open.
            prop_assert_eq!(reported("mode"), !(0o600..=0o777).contains(&mode));
        }

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
