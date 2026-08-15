//! Observed state — the environment-owned half of the strict split (§15.8).
//! Reportable by the `state` verb, never persisted, and by construction absent
//! from every configuration type.
//!
//! This module is the status vocabulary and the split, and deliberately nothing
//! else: the `active | waiting | faulted` family every node kind reports (§7),
//! and the stamp that says when it last *changed* rather than when it was last
//! polled. The per-boundary counters §5 asks for ("loss is always visible and
//! attributable") live where the bytes cross — each node kind's own stats,
//! surfaced through its `state_extra` object — because what a counter is called
//! is part of what that boundary observed, and one struct here would have to give
//! a single name to losses that are deliberately not the same loss.
//!
//! (This header used to frame that as construction phases — "Fleshed out with
//! counters per boundary in phase 3; phase 1 establishes only the status
//! vocabulary and the split". Retired for `daemon/src/nodes/mod.rs`'s reason,
//! plan §18 item 59(b): the counters landed long ago, and they landed at the
//! boundaries rather than here, so the sentence had become a map of a tree that
//! never existed.)

use std::time::{SystemTime, UNIX_EPOCH};

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

/// A node's status together with the moment it last *changed* — §7's "a status
/// of `active | waiting | faulted` with reason and timestamp (state)", common to
/// every node type. This is the holder a node keeps in place of a bare
/// [`NodeStatus`]: [`Self::set`] re-stamps only on a real transition, so the
/// timestamp answers "since when has it been faulted?" rather than "when was it
/// last polled?" — the difference an operator watching a flapping adapter needs.
/// A repeat of the same status *and reason* is not a transition; a new reason is.
///
/// It serializes flat, so a node's `state` object carries `since_unix_ms` beside
/// the `status`/`reason` it already reported:
/// `{"status":"faulted","reason":"ENOENT: /dev/ttyUSB0","since_unix_ms":…}`.
/// The stamp is wall clock in milliseconds since the Unix epoch, which is what
/// makes it comparable with the daemon's own log lines and with whatever else
/// the operator correlates against; a clock step moves it like every other
/// wall-clock time on the box. (`serial-nexus-core` is dependency-free — `std::time` is
/// the whole toolbox — and a monotonic `Instant` cannot be reported as an
/// absolute time at all.) Observed state: never persisted, and re-stamped to
/// "now" whenever a node is rebuilt, since a rebuilt node has no history (§15.8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeState {
    #[serde(flatten)]
    status: NodeStatus,
    since_unix_ms: u64,
}

impl NodeState {
    /// A node entering `status` now.
    pub fn new(status: NodeStatus) -> Self {
        Self::new_at(status, now_unix_ms())
    }

    /// Record `status`, stamping the transition. Returns whether this actually
    /// *was* a transition — a re-report of the identical status keeps the
    /// original stamp, so the reported age is the age of the condition, not of
    /// the last poll.
    pub fn set(&mut self, status: NodeStatus) -> bool {
        self.set_at(status, now_unix_ms())
    }

    pub fn status(&self) -> &NodeStatus {
        &self.status
    }

    /// When the current status was entered, in milliseconds since the Unix epoch.
    pub fn since_unix_ms(&self) -> u64 {
        self.since_unix_ms
    }

    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }

    pub fn reason(&self) -> Option<&str> {
        self.status.reason()
    }

    // The clock is injected on these two so the transition rule is testable
    // without sleeping (the suite has no bare sleeps, §5).
    fn new_at(status: NodeStatus, unix_ms: u64) -> Self {
        NodeState {
            status,
            since_unix_ms: unix_ms,
        }
    }

    fn set_at(&mut self, status: NodeStatus, unix_ms: u64) -> bool {
        if self.status == status {
            return false;
        }
        self.status = status;
        self.since_unix_ms = unix_ms;
        true
    }
}

/// Wall clock in milliseconds since the Unix epoch, saturating to 0 on a clock
/// set before 1970 (a broken RTC must not panic a data-plane node).
fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
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

    #[test]
    fn state_serializes_status_reason_and_timestamp_flat() {
        // §7: every node reports a status with reason *and timestamp*. The three
        // are siblings in one object, so the `state` verb's per-node rendering
        // gains the stamp without restructuring anything.
        let st = NodeState::new_at(
            NodeStatus::Waiting {
                reason: "device absent".into(),
            },
            1_753_400_000_000,
        );
        let j = serde_json::to_value(&st).unwrap();
        assert_eq!(j["status"], "waiting");
        assert_eq!(j["reason"], "device absent");
        assert_eq!(j["since_unix_ms"], 1_753_400_000_000u64);
        assert!(!st.is_active());
        assert_eq!(st.reason(), Some("device absent"));
    }

    #[test]
    fn set_restamps_only_on_a_real_transition() {
        let mut st = NodeState::new_at(NodeStatus::Active, 1_000);
        assert!(st.is_active());
        assert_eq!(st.since_unix_ms(), 1_000);

        // A re-report of the identical status is not a transition: the stamp
        // still says when the node *became* active, not when it was last polled.
        assert!(!st.set_at(NodeStatus::Active, 2_000));
        assert_eq!(st.since_unix_ms(), 1_000);

        // A real change stamps.
        let faulted = NodeStatus::Faulted {
            reason: "EIO".into(),
        };
        assert!(st.set_at(faulted.clone(), 3_000));
        assert_eq!(st.since_unix_ms(), 3_000);
        assert_eq!(st.status(), &faulted);

        // The same fault re-reported by the recovery poll is not a transition.
        assert!(!st.set_at(faulted, 4_000));
        assert_eq!(st.since_unix_ms(), 3_000);

        // Same variant, *different* reason: the environment changed, so it is a
        // transition and the stamp moves.
        assert!(st.set_at(
            NodeStatus::Faulted {
                reason: "ENXIO".into()
            },
            5_000
        ));
        assert_eq!(st.since_unix_ms(), 5_000);
    }

    #[test]
    fn new_stamps_the_current_wall_clock_in_milliseconds() {
        let st = NodeState::new(NodeStatus::Active);
        // Milliseconds, not seconds: any plausible present is past this (2020-09).
        assert!(
            st.since_unix_ms() > 1_600_000_000_000,
            "stamp {} is not milliseconds since the epoch",
            st.since_unix_ms()
        );
    }
}
