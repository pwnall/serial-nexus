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
        }
        errors.extend(self.to_model().validate());
        errors
    }
}

/// An edge in configuration form: two endpoint addresses and a write mode (§6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeConfig {
    pub a: EndpointAddr,
    pub b: EndpointAddr,
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
        /// The opaque, codec-validated attribute table (§8). The codec
        /// deserializes it into its own types; a schema failure is structural
        /// and fails the load (§11). Empty for the reference framing codec, which
        /// needs no attributes. Declared last so it serializes after the scalar
        /// fields, which TOML's table syntax requires.
        #[serde(default, skip_serializing_if = "toml::Table::is_empty")]
        attributes: toml::Table,
    },
}

impl NodeConfig {
    pub fn name(&self) -> &str {
        match self {
            NodeConfig::Serial { name, .. }
            | NodeConfig::Pty { name, .. }
            | NodeConfig::Log { name, .. }
            | NodeConfig::Codec { name, .. } => name,
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
        }
    }

    /// The write-arbitration policy of this node's host-facing endpoint(s) (§6),
    /// or the default for node kinds without one. A codec's policy applies to all
    /// of its host-facing endpoints uniformly.
    pub fn arbitration(&self) -> Arbitration {
        match self {
            NodeConfig::Serial { arbitration, .. } | NodeConfig::Codec { arbitration, .. } => {
                *arbitration
            }
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

fn default_rotation_padding() -> u8 {
    3
}

fn default_faces_host() -> Facing {
    Facing::Host
}

fn default_faces_target() -> Facing {
    Facing::Target
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
                },
                NodeConfig::Pty {
                    name: "console".into(),
                    path: "/run/serial_nexus/console".into(),
                    owner: None,
                    group: Some("dialout".into()),
                    mode: Some(0o660),
                    advertised_baud: 115_200,
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
                },
                NodeConfig::Codec {
                    name: "mux".into(),
                    codec: "reference".into(),
                    faces: Facing::Target,
                    channels: vec!["console".into(), "trace".into()],
                    arbitration: Arbitration::Exclusive,
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
    fn any_arbitration() -> impl Strategy<Value = Arbitration> {
        prop_oneof![Just(Arbitration::Exclusive), Just(Arbitration::FreeForAll)]
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
            )
                .prop_map(|(name, path, owner, group, mode, advertised_baud)| {
                    NodeConfig::Pty {
                        name,
                        path,
                        owner,
                        group,
                        mode,
                        advertised_baud,
                    }
                }),
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
            )
                .prop_map(|(name, codec, faces, channels, arbitration)| {
                    NodeConfig::Codec {
                        name,
                        codec,
                        faces,
                        channels,
                        arbitration,
                        // The attribute table's TOML round-trip is covered by the
                        // explicit demux test; keep the proptest table empty so the
                        // arbitrary structural shapes stay TOML-clean.
                        attributes: toml::Table::new(),
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
