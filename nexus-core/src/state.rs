//! Observed state — the environment-owned half of the strict split (§15.8).
//! Reportable by the `state` verb, never persisted, and by construction absent
//! from every configuration type. Fleshed out with counters per boundary in
//! phase 3; phase 1 establishes only the status vocabulary and the split.

use serde::{Deserialize, Serialize};

/// A node's observed status (§7, common to all node types). `waiting` and
/// `faulted` are the same state family — an environmental failure faults a node
/// without removing it (§15.8), and it heals on its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum NodeStatus {
    /// Environment present and healthy.
    Active,
    /// Configured here but its environment is not yet present (an unplugged
    /// serial device, a leg channel not yet announced by the peer).
    Waiting { reason: String },
    /// An environmental failure occurred; the node polls to recover.
    Faulted { reason: String },
}

impl NodeStatus {
    pub fn is_active(&self) -> bool {
        matches!(self, NodeStatus::Active)
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            NodeStatus::Active => None,
            NodeStatus::Waiting { reason } | NodeStatus::Faulted { reason } => Some(reason),
        }
    }
}

/// Observed per-node state (§7). A monotonic `since_unix_ms` timestamps the last
/// status transition; it is set by the daemon at runtime and never persisted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeState {
    pub name: String,
    #[serde(flatten)]
    pub status: NodeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since_unix_ms: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_serializes_with_a_tag() {
        let s = NodeStatus::Faulted {
            reason: "ENOENT: /dev/ttyUSB0".into(),
        };
        let j = serde_json::to_value(&s).unwrap();
        assert_eq!(j["status"], "faulted");
        assert_eq!(j["reason"], "ENOENT: /dev/ttyUSB0");
        assert_eq!(s.reason(), Some("ENOENT: /dev/ttyUSB0"));
    }
}
