//! The graph model and its three structural rules (design §4, §15.2–§15.4).
//!
//! Nodes expose typed [`EndpointSpec`]s; an [`EdgeSpec`] joins two endpoints.
//! Orientation is a local, per-endpoint property ([`Facing`]) anchored on the
//! system under control, not on silicon (§15.3). The validator works purely on
//! node *shapes* (their endpoints) and edges — it knows nothing about node
//! behavior, which keeps topology validation independent of the boundary
//! implementations that land in later phases.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Orientation of an endpoint along the target–host axis (§3). "Faces" means
/// *looks toward*. A valid edge always joins one host-facing endpoint to one
/// target-facing endpoint. Data flows both ways along every edge regardless.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Facing {
    /// Looks toward the device under control.
    Target,
    /// Looks toward the world of consumers (terminals, logs, sockets).
    Host,
}

impl Facing {
    #[must_use]
    pub fn opposite(self) -> Facing {
        match self {
            Facing::Target => Facing::Host,
            Facing::Host => Facing::Target,
        }
    }
}

impl fmt::Display for Facing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Facing::Target => f.write_str("target"),
            Facing::Host => f.write_str("host"),
        }
    }
}

/// Per-edge write capability (§6). Reading is never arbitrated; only these
/// modes govern who may write targetward through a host-facing endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WriteMode {
    /// The read-only capability (log edges, spy PTYs). Cannot contend for the
    /// lock at all.
    Never,
    /// The default for interactive/programmatic origins. Acquisition is
    /// explicit for named origins, implicit for leg channels.
    #[default]
    OnDemand,
    /// Acquire-on-attach, held indefinitely (the demux codec's edge to the
    /// serial port).
    Held,
}

/// Per-host-facing-endpoint arbitration policy (§6). Defaults to exclusive;
/// free-for-all is the escape hatch for machine-to-machine links coordinated
/// elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Arbitration {
    #[default]
    Exclusive,
    FreeForAll,
}

/// The canonical name of a single-endpoint node's sole endpoint. Codec and leg
/// channels use their channel identity instead; the display form of the sole
/// endpoint is just the node name (§3).
pub const DEFAULT_ENDPOINT: &str = "";

/// Address of an endpoint: a node name plus a local endpoint name. Single
/// endpoint nodes use [`DEFAULT_ENDPOINT`]; codec/leg channels use their
/// channel identity. Neither a node name nor a channel identity contains `/`
/// (§15.12), so the display form `node/channel` parses unambiguously.
///
/// Serializes as its display *string* (`"usb0"` or `"mux/console"`), not a
/// nested `{node, endpoint}` table — so edges are all-scalar and TOML-friendly,
/// and configs read the way operators write them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EndpointAddr {
    pub node: String,
    pub endpoint: String,
}

impl Serialize for EndpointAddr {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for EndpointAddr {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        // FromStr is infallible: every string is a valid address.
        Ok(s.parse().expect("EndpointAddr::from_str is infallible"))
    }
}

impl EndpointAddr {
    pub fn new(node: impl Into<String>, endpoint: impl Into<String>) -> Self {
        EndpointAddr {
            node: node.into(),
            endpoint: endpoint.into(),
        }
    }

    /// The sole/default endpoint of a single-endpoint node.
    pub fn node(node: impl Into<String>) -> Self {
        EndpointAddr {
            node: node.into(),
            endpoint: DEFAULT_ENDPOINT.to_owned(),
        }
    }

    /// A channel endpoint `node/channel`.
    pub fn channel(node: impl Into<String>, channel: impl Into<String>) -> Self {
        EndpointAddr::new(node, channel)
    }

    pub fn is_default(&self) -> bool {
        self.endpoint == DEFAULT_ENDPOINT
    }
}

impl fmt::Display for EndpointAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_default() {
            f.write_str(&self.node)
        } else {
            write!(f, "{}/{}", self.node, self.endpoint)
        }
    }
}

impl std::str::FromStr for EndpointAddr {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.split_once('/') {
            Some((node, endpoint)) => EndpointAddr::new(node, endpoint),
            None => EndpointAddr::node(s),
        })
    }
}

/// One endpoint a node exposes: its local name, orientation, and (for
/// host-facing endpoints) arbitration policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointSpec {
    pub name: String,
    pub facing: Facing,
    pub arbitration: Arbitration,
}

impl EndpointSpec {
    pub fn host(name: impl Into<String>) -> Self {
        EndpointSpec {
            name: name.into(),
            facing: Facing::Host,
            arbitration: Arbitration::Exclusive,
        }
    }

    pub fn target(name: impl Into<String>) -> Self {
        EndpointSpec {
            name: name.into(),
            facing: Facing::Target,
            arbitration: Arbitration::Exclusive,
        }
    }
}

/// The topological shape of a node: the endpoints it exposes. Derived from a
/// node's configuration (kind + attributes such as channel lists and `faces`);
/// the validator consumes only this, never node behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeShape {
    pub endpoints: Vec<EndpointSpec>,
}

impl NodeShape {
    pub fn new(endpoints: Vec<EndpointSpec>) -> Self {
        NodeShape { endpoints }
    }

    /// A node with a single endpoint of the given facing (serial, pty, log), with
    /// the default exclusive arbitration.
    pub fn single(facing: Facing) -> Self {
        Self::single_arb(facing, Arbitration::Exclusive)
    }

    /// A single-endpoint node with an explicit arbitration policy (§6) — used for
    /// the serial node's host-facing endpoint, whose policy is configurable.
    pub fn single_arb(facing: Facing, arbitration: Arbitration) -> Self {
        NodeShape {
            endpoints: vec![EndpointSpec {
                name: DEFAULT_ENDPOINT.to_owned(),
                facing,
                arbitration,
            }],
        }
    }

    fn endpoint(&self, name: &str) -> Option<&EndpointSpec> {
        self.endpoints.iter().find(|e| e.name == name)
    }
}

/// An edge joining two endpoints, carrying a write mode (§6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeSpec {
    pub a: EndpointAddr,
    pub b: EndpointAddr,
    pub write_mode: WriteMode,
}

impl EdgeSpec {
    pub fn new(a: EndpointAddr, b: EndpointAddr) -> Self {
        EdgeSpec {
            a,
            b,
            write_mode: WriteMode::default(),
        }
    }

    pub fn with_mode(mut self, mode: WriteMode) -> Self {
        self.write_mode = mode;
        self
    }
}

/// A structural validation failure, always naming the offender (§4, §11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// A node name or channel identity contains `/` — forbidden because the
    /// display form `node/channel` and the on-disk address encoding depend on
    /// its absence (§3, §15.12). `endpoint` is `None` when the offending name is
    /// the node's own name, `Some` when it is one of its channel identities.
    InvalidName {
        node: String,
        endpoint: Option<String>,
    },
    /// Two of a node's endpoints share a local name — a multi-endpoint node (a
    /// codec) with a duplicate channel identity, or a channel identity colliding
    /// with the reserved multiplexed-side default endpoint (an empty identity,
    /// §3). The second endpoint would be permanently shadowed (endpoint resolution
    /// returns the first match), so it is a structural error naming the offender.
    DuplicateEndpoint { node: String, endpoint: String },
    /// Two nodes share a name. Node names key the graph (and the endpoint-addressed
    /// data-plane wiring), so a duplicate would silently collapse one node into the
    /// other — a structural error naming the offender (§3, §4).
    DuplicateNodeName { node: String },
    /// An edge references a node that does not exist.
    UnknownNode { edge: usize, node: String },
    /// An edge references an endpoint the node does not expose.
    UnknownEndpoint { edge: usize, addr: EndpointAddr },
    /// An edge joins two same-facing endpoints (host↔host or target↔target) —
    /// rule 1.
    SameFacingEdge { edge: usize, facing: Facing },
    /// More than one edge attached to a target-facing endpoint — rule 2, the
    /// one-producer invariant (§15.4).
    TargetEndpointOversubscribed { addr: EndpointAddr, count: usize },
    /// A directed cycle through the graph — rule 3.
    Cycle { nodes: Vec<String> },
    /// A leg node binds or dials a non-loopback address without `insecure_bind`
    /// (§7.4, §9): loopback-only is the v1 security posture, and a remote bind
    /// requires a visible, greppable confession. Checked at the config level
    /// (the model sees only shapes, not transport/address), naming the offender.
    NonLoopbackBind { node: String, address: String },
    /// A leg node declares no channels — a degenerate, zero-endpoint transport that
    /// can carry nothing (§7.4). A leg must declare at least one channel. Checked at
    /// the config level, since a leg is the only node kind that can shape to zero
    /// endpoints.
    EmptyLeg { node: String },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationError::InvalidName {
                node,
                endpoint: None,
            } => {
                write!(
                    f,
                    "node name {node:?} contains '/', which names and channel identities may not (§3)"
                )
            }
            ValidationError::InvalidName {
                node,
                endpoint: Some(channel),
            } => {
                write!(
                    f,
                    "channel identity {channel:?} on node {node:?} contains '/', which names and channel identities may not (§3)"
                )
            }
            ValidationError::DuplicateNodeName { node } => {
                write!(
                    f,
                    "node name {node:?} is declared more than once (node names must be unique, §4)"
                )
            }
            ValidationError::DuplicateEndpoint { node, endpoint } => {
                if endpoint.is_empty() {
                    write!(
                        f,
                        "node {node:?} declares an empty channel identity, which is reserved for a default endpoint and forbidden as a real channel (§3)"
                    )
                } else {
                    write!(
                        f,
                        "node {node:?} declares endpoint {endpoint:?} more than once (a duplicate channel identity would be shadowed, §3)"
                    )
                }
            }
            ValidationError::UnknownNode { edge, node } => {
                write!(f, "edge {edge} references unknown node {node:?}")
            }
            ValidationError::UnknownEndpoint { edge, addr } => {
                write!(f, "edge {edge} references unknown endpoint {addr}")
            }
            ValidationError::SameFacingEdge { edge, facing } => {
                write!(
                    f,
                    "edge {edge} joins two {facing}-facing endpoints (must join one host to one target)"
                )
            }
            ValidationError::TargetEndpointOversubscribed { addr, count } => {
                write!(
                    f,
                    "target-facing endpoint {addr} has {count} edges (must have at most one — the one-producer invariant)"
                )
            }
            ValidationError::Cycle { nodes } => {
                write!(f, "graph contains a cycle through {}", nodes.join(" -> "))
            }
            ValidationError::NonLoopbackBind { node, address } => {
                write!(
                    f,
                    "leg node {node:?} binds/dials non-loopback address {address:?} without insecure_bind=true (§7.4)"
                )
            }
            ValidationError::EmptyLeg { node } => {
                write!(
                    f,
                    "leg node {node:?} declares no channels (a leg must carry at least one, §7.4)"
                )
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// A set of node shapes and edges, checkable against the three structural rules
/// (§4). Built from configuration at load and on every incremental operation.
#[derive(Debug, Clone, Default)]
pub struct GraphModel {
    shapes: HashMap<String, NodeShape>,
    edges: Vec<EdgeSpec>,
}

impl GraphModel {
    pub fn new() -> Self {
        GraphModel::default()
    }

    pub fn add_node(&mut self, name: impl Into<String>, shape: NodeShape) {
        self.shapes.insert(name.into(), shape);
    }

    pub fn add_edge(&mut self, edge: EdgeSpec) {
        self.edges.push(edge);
    }

    fn resolve<'a>(&'a self, addr: &EndpointAddr) -> Result<&'a EndpointSpec, ResolveErr> {
        let shape = self.shapes.get(&addr.node).ok_or(ResolveErr::Node)?;
        shape.endpoint(&addr.endpoint).ok_or(ResolveErr::Endpoint)
    }

    /// Validate against all three rules, returning every violation found (load
    /// is atomic, so surfacing all offenders at once is friendlier than one at
    /// a time). An empty vector means the graph is structurally valid.
    pub fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        // §3/§15.12: a node name or channel identity containing `/` breaks the
        // `node/channel` display form and the on-disk address encoding, so it is
        // a structural error. Checked on declared names (shape keys and endpoint
        // names); malformed *references* in edges surface as UnknownNode/Endpoint.
        for (node, shape) in &self.shapes {
            if node.contains('/') {
                errors.push(ValidationError::InvalidName {
                    node: node.clone(),
                    endpoint: None,
                });
            }
            // Endpoint names must be locally unique: a codec is the first node kind
            // with multiple endpoints, so a duplicate channel identity — or a
            // channel identity colliding with the reserved multiplexed-side default
            // endpoint (an empty identity) — would leave the second endpoint dead
            // (resolution returns the first match). Report it, naming the offender.
            let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
            for ep in &shape.endpoints {
                if ep.name.contains('/') {
                    errors.push(ValidationError::InvalidName {
                        node: node.clone(),
                        endpoint: Some(ep.name.clone()),
                    });
                }
                if !seen.insert(ep.name.as_str()) {
                    errors.push(ValidationError::DuplicateEndpoint {
                        node: node.clone(),
                        endpoint: ep.name.clone(),
                    });
                }
            }
        }

        // Rule 1 + reference integrity, and tally target-facing edge counts for
        // rule 2. Directed arcs (host-node -> target-node) feed rule 3.
        let mut target_edge_count: HashMap<EndpointAddr, usize> = HashMap::new();
        let mut arcs: Vec<(String, String)> = Vec::new();

        for (i, edge) in self.edges.iter().enumerate() {
            let (a, b) = match (self.resolve(&edge.a), self.resolve(&edge.b)) {
                (Ok(a), Ok(b)) => (a, b),
                (Err(e), _) => {
                    errors.push(reference_error(i, &edge.a, e));
                    continue;
                }
                (_, Err(e)) => {
                    errors.push(reference_error(i, &edge.b, e));
                    continue;
                }
            };

            // Rule 1: exactly one host and one target.
            match (a.facing, b.facing) {
                (Facing::Host, Facing::Target) | (Facing::Target, Facing::Host) => {}
                (facing, _) => {
                    errors.push(ValidationError::SameFacingEdge { edge: i, facing });
                    continue;
                }
            }

            // Identify the target-facing side for rule 2 and orient the arc for
            // rule 3 (data flows host-node -> target-node).
            let (host_addr, target_addr) = if a.facing == Facing::Host {
                (&edge.a, &edge.b)
            } else {
                (&edge.b, &edge.a)
            };
            *target_edge_count.entry(target_addr.clone()).or_insert(0) += 1;
            arcs.push((host_addr.node.clone(), target_addr.node.clone()));
        }

        // Rule 2: at most one edge per target-facing endpoint.
        for (addr, count) in &target_edge_count {
            if *count > 1 {
                errors.push(ValidationError::TargetEndpointOversubscribed {
                    addr: addr.clone(),
                    count: *count,
                });
            }
        }

        // Rule 3: acyclic.
        if let Some(cycle) = find_cycle(&self.shapes, &arcs) {
            errors.push(ValidationError::Cycle { nodes: cycle });
        }

        errors
    }

    /// Convenience: `Ok(())` iff [`Self::validate`] is empty.
    pub fn check(&self) -> Result<(), Vec<ValidationError>> {
        let errors = self.validate();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[derive(Clone, Copy)]
enum ResolveErr {
    Node,
    Endpoint,
}

fn reference_error(edge: usize, addr: &EndpointAddr, e: ResolveErr) -> ValidationError {
    match e {
        ResolveErr::Node => ValidationError::UnknownNode {
            edge,
            node: addr.node.clone(),
        },
        ResolveErr::Endpoint => ValidationError::UnknownEndpoint {
            edge,
            addr: addr.clone(),
        },
    }
}

/// Depth-first cycle detection over the directed node graph. Returns the nodes
/// on a detected cycle (in order) or `None` if acyclic.
fn find_cycle(
    shapes: &HashMap<String, NodeShape>,
    arcs: &[(String, String)],
) -> Option<Vec<String>> {
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for name in shapes.keys() {
        adj.entry(name.as_str()).or_default();
    }
    for (from, to) in arcs {
        adj.entry(from.as_str()).or_default().push(to.as_str());
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }
    let mut color: HashMap<&str, Color> = adj.keys().map(|k| (*k, Color::White)).collect();

    // Iterative DFS carrying the gray path so a back-edge yields the cycle.
    for &start in adj.keys() {
        if color[start] != Color::White {
            continue;
        }
        let mut stack: Vec<(&str, usize)> = vec![(start, 0)];
        let mut path: Vec<&str> = vec![start];
        color.insert(start, Color::Gray);
        while let Some(&mut (node, ref mut idx)) = stack.last_mut() {
            let neighbors = &adj[node];
            if *idx < neighbors.len() {
                let next = neighbors[*idx];
                *idx += 1;
                match color[next] {
                    Color::White => {
                        color.insert(next, Color::Gray);
                        stack.push((next, 0));
                        path.push(next);
                    }
                    Color::Gray => {
                        // Back-edge: the cycle is the path from `next` onward.
                        let at = path.iter().position(|n| *n == next).unwrap_or(0);
                        let mut cycle: Vec<String> =
                            path[at..].iter().map(|s| s.to_string()).collect();
                        cycle.push(next.to_string());
                        return Some(cycle);
                    }
                    Color::Black => {}
                }
            } else {
                color.insert(node, Color::Black);
                stack.pop();
                path.pop();
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A serial(host) → pty(target) + log(target) fan-out: the canonical legal
    /// shape (§4 rule 2, fan-out at a host-facing endpoint).
    fn fanout_graph() -> GraphModel {
        let mut g = GraphModel::new();
        g.add_node("usb0", NodeShape::single(Facing::Host));
        g.add_node("console", NodeShape::single(Facing::Target));
        g.add_node("log", NodeShape::single(Facing::Target));
        g.add_edge(EdgeSpec::new(
            EndpointAddr::node("usb0"),
            EndpointAddr::node("console"),
        ));
        g.add_edge(
            EdgeSpec::new(EndpointAddr::node("usb0"), EndpointAddr::node("log"))
                .with_mode(WriteMode::Never),
        );
        g
    }

    #[test]
    fn legal_fanout_validates() {
        assert!(fanout_graph().check().is_ok());
    }

    #[test]
    fn host_to_host_edge_is_rejected() {
        let mut g = GraphModel::new();
        g.add_node("a", NodeShape::single(Facing::Host));
        g.add_node("b", NodeShape::single(Facing::Host));
        g.add_edge(EdgeSpec::new(
            EndpointAddr::node("a"),
            EndpointAddr::node("b"),
        ));
        let errs = g.validate();
        assert!(matches!(
            errs.as_slice(),
            [ValidationError::SameFacingEdge {
                facing: Facing::Host,
                ..
            }]
        ));
    }

    #[test]
    fn target_to_target_edge_is_rejected() {
        let mut g = GraphModel::new();
        g.add_node("a", NodeShape::single(Facing::Target));
        g.add_node("b", NodeShape::single(Facing::Target));
        g.add_edge(EdgeSpec::new(
            EndpointAddr::node("a"),
            EndpointAddr::node("b"),
        ));
        let errs = g.validate();
        assert!(matches!(
            errs.as_slice(),
            [ValidationError::SameFacingEdge {
                facing: Facing::Target,
                ..
            }]
        ));
    }

    #[test]
    fn second_edge_on_target_endpoint_is_rejected() {
        // Two host sources into one target-facing consumer violates the
        // one-producer invariant (§15.4).
        let mut g = GraphModel::new();
        g.add_node("src1", NodeShape::single(Facing::Host));
        g.add_node("src2", NodeShape::single(Facing::Host));
        g.add_node("sink", NodeShape::single(Facing::Target));
        g.add_edge(EdgeSpec::new(
            EndpointAddr::node("src1"),
            EndpointAddr::node("sink"),
        ));
        g.add_edge(EdgeSpec::new(
            EndpointAddr::node("src2"),
            EndpointAddr::node("sink"),
        ));
        let errs = g.validate();
        assert!(
            errs.iter().any(|e| matches!(
                e,
                ValidationError::TargetEndpointOversubscribed { count: 2, .. }
            )),
            "expected oversubscription error, got {errs:?}"
        );
    }

    #[test]
    fn host_endpoint_fanout_is_unlimited() {
        // One host source into three target consumers is legal (fan-out).
        let mut g = GraphModel::new();
        g.add_node("src", NodeShape::single(Facing::Host));
        for name in ["s1", "s2", "s3"] {
            g.add_node(name, NodeShape::single(Facing::Target));
            g.add_edge(EdgeSpec::new(
                EndpointAddr::node("src"),
                EndpointAddr::node(name),
            ));
        }
        assert!(g.check().is_ok());
    }

    #[test]
    fn cycle_is_rejected() {
        // A codec-like node with both a host and a target endpoint, wired into
        // a loop, must be caught. mux: target(in) + host(out).
        let mut g = GraphModel::new();
        g.add_node(
            "codec1",
            NodeShape::new(vec![EndpointSpec::target("in"), EndpointSpec::host("out")]),
        );
        g.add_node(
            "codec2",
            NodeShape::new(vec![EndpointSpec::target("in"), EndpointSpec::host("out")]),
        );
        // codec1/out(host) -> codec2/in(target); codec2/out(host) -> codec1/in(target)
        g.add_edge(EdgeSpec::new(
            EndpointAddr::channel("codec1", "out"),
            EndpointAddr::channel("codec2", "in"),
        ));
        g.add_edge(EdgeSpec::new(
            EndpointAddr::channel("codec2", "out"),
            EndpointAddr::channel("codec1", "in"),
        ));
        let errs = g.validate();
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::Cycle { .. })),
            "expected a cycle error, got {errs:?}"
        );
    }

    #[test]
    fn unknown_node_and_endpoint_are_named() {
        let mut g = GraphModel::new();
        g.add_node("real", NodeShape::single(Facing::Host));
        g.add_edge(EdgeSpec::new(
            EndpointAddr::node("real"),
            EndpointAddr::node("ghost"),
        ));
        let errs = g.validate();
        assert!(matches!(
            errs.as_slice(),
            [ValidationError::UnknownNode { node, .. }] if node == "ghost"
        ));
    }

    #[test]
    fn slash_in_node_name_is_rejected() {
        // §3/§15.12: a '/' in a node name breaks `node/channel` addressing, so it
        // is a structural validation error naming the offender.
        let mut g = GraphModel::new();
        g.add_node("bad/name", NodeShape::single(Facing::Host));
        let errs = g.validate();
        assert!(
            errs.iter().any(|e| matches!(
                e,
                ValidationError::InvalidName { node, endpoint: None } if node == "bad/name"
            )),
            "expected InvalidName for a slashed node name, got {errs:?}"
        );
    }

    #[test]
    fn slash_in_channel_identity_is_rejected() {
        // A codec/leg channel identity containing '/' is equally forbidden (§3).
        let mut g = GraphModel::new();
        g.add_node("mux", NodeShape::new(vec![EndpointSpec::host("con/sole")]));
        let errs = g.validate();
        assert!(
            errs.iter().any(|e| matches!(
                e,
                ValidationError::InvalidName { node, endpoint: Some(c) }
                    if node == "mux" && c == "con/sole"
            )),
            "expected InvalidName for a slashed channel identity, got {errs:?}"
        );
    }

    #[test]
    fn duplicate_channel_identity_is_rejected() {
        // A codec (the first multi-endpoint node kind) with two channels named the
        // same: the second would be shadowed, so it is a structural error.
        let mut g = GraphModel::new();
        g.add_node(
            "mux",
            NodeShape::new(vec![
                EndpointSpec::target(DEFAULT_ENDPOINT),
                EndpointSpec::host("con"),
                EndpointSpec::host("con"),
            ]),
        );
        let errs = g.validate();
        assert!(
            errs.iter().any(|e| matches!(
                e,
                ValidationError::DuplicateEndpoint { node, endpoint }
                    if node == "mux" && endpoint == "con"
            )),
            "expected DuplicateEndpoint for a repeated channel identity, got {errs:?}"
        );
    }

    #[test]
    fn empty_channel_identity_collides_with_the_mux_default_endpoint() {
        // A codec's multiplexed side is the default (empty) endpoint; an empty
        // channel identity collides with it and must be rejected, naming the node.
        let mut g = GraphModel::new();
        g.add_node(
            "mux",
            NodeShape::new(vec![
                EndpointSpec::target(DEFAULT_ENDPOINT),
                EndpointSpec::host(DEFAULT_ENDPOINT),
            ]),
        );
        let errs = g.validate();
        assert!(
            errs.iter().any(|e| matches!(
                e,
                ValidationError::DuplicateEndpoint { node, endpoint }
                    if node == "mux" && endpoint.is_empty()
            )),
            "expected DuplicateEndpoint for an empty channel identity, got {errs:?}"
        );
    }

    #[test]
    fn distinct_codec_endpoints_pass() {
        // The legitimate demux shape — mux default endpoint plus distinct named
        // channels — must not trip the duplicate check.
        let mut g = GraphModel::new();
        g.add_node(
            "mux",
            NodeShape::new(vec![
                EndpointSpec::target(DEFAULT_ENDPOINT),
                EndpointSpec::host("console"),
                EndpointSpec::host("trace"),
            ]),
        );
        let errs = g.validate();
        assert!(
            !errs
                .iter()
                .any(|e| matches!(e, ValidationError::DuplicateEndpoint { .. })),
            "distinct codec endpoints must not trip the duplicate check, got {errs:?}"
        );
    }

    #[test]
    fn legal_names_pass_the_slash_check() {
        // The canonical fan-out (plain names, empty default endpoints) has no '/'
        // offenders — the check must not fire on legal graphs.
        let errs = fanout_graph().validate();
        assert!(
            !errs
                .iter()
                .any(|e| matches!(e, ValidationError::InvalidName { .. })),
            "legal names must not trip the slash check, got {errs:?}"
        );
    }

    #[test]
    fn endpoint_addr_display_and_parse_round_trip() {
        use std::str::FromStr;
        let plain = EndpointAddr::node("usb0");
        assert_eq!(plain.to_string(), "usb0");
        assert_eq!(EndpointAddr::from_str("usb0").unwrap(), plain);

        let chan = EndpointAddr::channel("mux", "console");
        assert_eq!(chan.to_string(), "mux/console");
        assert_eq!(EndpointAddr::from_str("mux/console").unwrap(), chan);
    }
}
