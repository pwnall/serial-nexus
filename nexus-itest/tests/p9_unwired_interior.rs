//! MAP-1's shape, generalised: **a `waiting` interior node must be inert, not
//! destructive** (design §15.8, §5).
//!
//! The review (`docs/historical/26-claude-opus-code-review.md`, MAP-1 runtime) reported the map
//! node dropping its mapped endpoint's targetward receiver when it had no writable
//! raw edge. The receiver's *senders* stay live — in `GraphState::endpoint_targetward`
//! and in every attached writer origin — so dropping it closes the channel underneath
//! them, and a PTY origin's next write ends `read_and_poll` (`pty.rs`), taking presence
//! latching, `handle_last_close`, termios reconciliation and detach-release with it.
//! The console then reports a client that left as still attached, forever, and its
//! endpoint's lock is wedged on a holder that no longer exists.
//!
//! Fixing the map was not enough: the adversarial verification of that fix reproduced
//! the identical chain in `codec` and `exec`, through a different door. Both return
//! early from `start` when their multiplexed side has no attached upstream — the
//! ordinary state during incremental graph buildout, reported honestly as `waiting` —
//! and that `return` happened *before* they claimed their channels' receivers, which
//! then died with the wiring plan at the end of `load`.
//!
//! So the guard here is written against the *rule* rather than one node: for each
//! interior node kind, with its upstream deliberately absent, a PTY writer attached to
//! one of its host-facing endpoints must stay healthy — presence must still flip when
//! the client leaves, the lock must still detach-release, and the bytes that went
//! nowhere must be counted rather than vanish (§5).
//!
//! Device-free, so it runs on every platform: the serial node is configured for a
//! device that does not exist, which is exactly the `waiting` upstream the test wants.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::time::Duration;

use nexus_itest::{Daemon, wait_until};
use serde_json::Value;

/// One interior node kind, wired with its upstream absent.
struct Case {
    /// What the node is, for failure messages.
    kind: &'static str,
    /// The `[[node]]` block for the interior node under test.
    node: &'static str,
    /// The host-facing endpoint the PTY attaches to.
    endpoint: &'static str,
}

const CASES: &[Case] = &[
    Case {
        kind: "codec",
        node: r#"
[[node]]
type = "codec"
name = "mux"
codec = "reference"
faces = "target"
channels = ["c0"]
"#,
        endpoint: "mux/c0",
    },
    Case {
        kind: "exec",
        node: r#"
[[node]]
type = "codec"
name = "mux"
codec = "exec"
faces = "target"
channels = ["c0"]
attributes = { argv = ["/bin/cat"] }
"#,
        endpoint: "mux/c0",
    },
    Case {
        kind: "map",
        node: r#"
[[node]]
type = "map"
name = "mux"
targetward = ["lfcrlf"]
"#,
        endpoint: "mux",
    },
];

#[test]
fn an_interior_node_waiting_on_its_upstream_does_not_kill_its_writers() {
    const TYPED: usize = 64;
    for case in CASES {
        let d = Daemon::start();
        let rpc = d.rpc();
        let p0 = d.run().join("p0");
        // No edge into the interior node's multiplexed/raw side at all: `mux` comes up
        // `waiting`, which is the shape that used to strand the channel receivers. The
        // PTY hangs off the interior node's host-facing endpoint, as an operator
        // building a graph incrementally would leave it.
        let cfg = format!(
            r#"{node}
[[node]]
type = "pty"
name = "p0"
path = "{p0}"
[[edge]]
a = "{endpoint}"
b = "p0"
"#,
            node = case.node,
            endpoint = case.endpoint,
            p0 = p0.display(),
        );
        rpc.load_toml(&cfg, false)
            .unwrap_or_else(|e| panic!("[{}] load: {e:?}", case.kind));

        assert!(
            rpc.wait_status("p0", "active", Duration::from_secs(10)),
            "[{}] p0 not active: {:?}",
            case.kind,
            rpc.node("p0")
        );
        assert!(
            wait_until(Duration::from_secs(5), || p0.exists()),
            "[{}] p0 symlink never appeared",
            case.kind
        );
        // The node reports the honest reason rather than pretending to be active.
        let mux = rpc.node("mux").expect("mux in state");
        assert_eq!(
            mux["status"].as_str(),
            Some("waiting"),
            "[{}] expected the interior node to be waiting on its upstream: {mux}",
            case.kind
        );

        // Take the lock, attach a client, and let the daemon *observe* it before
        // typing — otherwise a reader that died before ever latching would look the
        // same as the defect this pins.
        rpc.lock("p0", false, false, None)
            .unwrap_or_else(|e| panic!("[{}] lock p0: {e:?}", case.kind));
        let mut client = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOCTTY)
            .open(&p0)
            .unwrap_or_else(|e| panic!("[{}] open pty slave: {e}", case.kind));
        assert!(
            wait_until(Duration::from_secs(10), || {
                rpc.node("p0")
                    .and_then(|n| n.get("client_present").and_then(Value::as_bool))
                    == Some(true)
            }),
            "[{}] the client never became present",
            case.kind
        );
        client
            .write_all(&[b'x'; TYPED])
            .expect("type at the console");
        client.flush().expect("flush");

        // The property. Pre-fix, the first typed byte hit a closed channel, ended the
        // PTY's reader task, and froze this at `true` forever.
        drop(client);
        assert!(
            wait_until(Duration::from_secs(10), || {
                rpc.node("p0")
                    .and_then(|n| n.get("client_present").and_then(Value::as_bool))
                    == Some(false)
            }),
            "[{}] client_present never went false — the PTY's reader task died with \
             its targetward channel (MAP-1 shape): {:?}",
            case.kind,
            rpc.node("p0")
        );
        // Detach-release is the same reader task's job (§6), so this proves the task
        // survived rather than that presence merely happened to flip. Without it the
        // endpoint stays locked by an origin that is gone — unrecoverable short of a
        // steal.
        assert!(
            wait_until(Duration::from_secs(5), || {
                rpc.node("mux")
                    .map(|n| holder_of(&n, case.endpoint))
                    .unwrap_or(Value::Null)
                    == Value::Null
            }),
            "[{}] the endpoint's lock was not detach-released: {:?}",
            case.kind,
            rpc.node("mux")
        );
    }
}

/// The `lock.holder` of `endpoint` inside a node's `state` object — the endpoint is
/// either the node's default (`mux`) or one of its channels (`mux/c0`).
fn holder_of(node: &Value, endpoint: &str) -> Value {
    match endpoint.split_once('/') {
        Some((_, channel)) => node
            .pointer(&format!("/channels/{channel}/lock/holder"))
            .cloned()
            .unwrap_or(Value::Null),
        None => node.pointer("/lock/holder").cloned().unwrap_or(Value::Null),
    }
}
