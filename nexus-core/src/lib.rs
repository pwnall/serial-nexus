#![forbid(unsafe_code)]

//! `nexus-core` — the graph model and data-plane contracts (design §3–§5).
//!
//! Pure logic, no kernel calls. This crate is the load-bearing foundation the
//! rest of the system builds on:
//!
//! * [`graph`] — endpoint/facing/edge types and the three structural rules
//!   (§4, §15.2–§15.4), validated on load and every incremental operation.
//! * [`data`] — the two `deliver` contracts and the single-chunk holdover slot
//!   (§5, §15.5), with mock boundaries for property testing.
//! * [`lock`] — the per-endpoint write-arbitration state machine (§6): who may
//!   write targetward, with holder and purge accounting. Pure; the daemon shares
//!   it between the control plane and the origin read tasks.
//! * [`config`] / [`state`] — the strict configuration/state split (§15.8),
//!   enforced by the type system: state fields do not exist in configuration
//!   types.

pub mod config;
pub mod data;
pub mod graph;
pub mod lock;
pub mod state;

pub use config::GraphConfig;
pub use data::{Chunk, Delivery};
pub use graph::{
    Arbitration, EdgeSpec, EndpointAddr, EndpointSpec, Facing, GraphModel, NodeShape,
    ValidationError, WriteMode,
};
pub use lock::{Acquire, EndpointLock, LockSnapshot, OriginId};
pub use state::{NodeState, NodeStatus};
