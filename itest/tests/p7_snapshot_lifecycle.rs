//! The state snapshot across the *lifecycle* verbs (design §11/§15.9): what
//! `remove-node`, `teardown` and a clean shutdown leave on disk for the next boot.
//!
//! `p7_crash_recovery` proves the additive half — a graph built by `load` +
//! `add-node` comes back after `kill -9`. The subtractive half had no guard at all:
//! nothing pinned that a *removal* is snapshotted, or that `teardown` persists the
//! empty graph §11 promises it does. Both verbs earn their snapshot from one
//! `is_config_mutation` match arm, and a regression dropping either from it would
//! pass the whole suite while resurrecting removed nodes on the next restart —
//! configuration silently diverging from `dump`, which is the fail-*silent* shape
//! the arm's own comment was written against. It is the defect class the rename
//! track's `p13_legacy_defaults` guards fail-first-proved for adoption; this is the
//! same property for the verbs on the other side of the graph's life.
//!
//! Three cases, each asserting **both** halves — the bytes on disk and the graph the
//! next daemon comes up with, because either one alone can be right while the other
//! is wrong:
//!
//! 1. `remove-node` is snapshotted: the node is gone from the state file, and the
//!    restart does not resurrect it.
//! 2. `teardown` persists the *empty* graph, and the restart comes up empty — not
//!    back on the pre-teardown configuration.
//! 3. A clean `shutdown` **preserves** the snapshot: it is not a config mutation, so
//!    the last mutation's file is what the next boot reads, byte for byte. (The
//!    signal-handling half of clean exit — SIGTERM/SIGINT — is 37-SEAM-1's; this is
//!    the state-file half, and it is deliberately about the file rather than about
//!    how the process was asked to stop.)
//!
//! Device-free: `pty` nodes need no hardware, so all three run on every platform.
//! The daemon lifecycle is hand-managed (a fixed socket + state file across two
//! spawns), which `Daemon::start` cannot express — the `p7_crash_recovery` pattern.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::Value;
use serial_nexus_itest::{Rpc, TempRun, bin, daemon_answers, wait_until};

/// A daemon child killed and reaped on drop, so a panicking test leaks nothing.
struct KillOnDrop(Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Spawn `serial-nexus-daemon` on `run`'s fixed socket + state file and wait until it
/// **answers** RPC. Reusing the same paths across two spawns is what exercises the
/// restart: the second daemon reclaims the socket and recovers the persisted
/// configuration at startup (§10/§11).
fn spawn_daemon(run: &TempRun) -> KillOnDrop {
    let child = Command::new(bin("serial-nexus-daemon"))
        .arg("--socket")
        .arg(run.socket())
        .arg("--state-file")
        .arg(run.state_file())
        .env("XDG_RUNTIME_DIR", run.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn serial-nexus-daemon");
    let socket = run.socket();
    assert!(
        wait_until(Duration::from_secs(10), || daemon_answers(&socket)),
        "daemon never answered on {}",
        socket.display()
    );
    KillOnDrop(child)
}

/// A two-console, edgeless graph. Two so a removal has a survivor to be distinguished
/// from a truncation: a snapshot that lost *everything* would pass a one-node test.
fn two_consoles(run: &TempRun) -> String {
    format!(
        r#"
[[node]]
type = "pty"
name = "keep"
path = "{keep}"
[[node]]
type = "pty"
name = "drop"
path = "{drop}"
"#,
        keep = run.join("keep").display(),
        drop = run.join("drop").display(),
    )
}

/// The node names in `state`, sorted.
fn node_names(rpc: &Rpc) -> Vec<String> {
    let mut names: Vec<String> = rpc.state()["nodes"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|n| n.get("name").and_then(Value::as_str).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// The state file's text, or panic naming the path — an absent snapshot is itself a
/// defect in every case here, never a reason to skip an assertion.
fn snapshot(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read the state file {}: {e}", path.display()))
}

/// Wait for the state file to reach a state the caller recognises. The snapshot is
/// written after the verb's reply is composed, so a bare read immediately after the
/// RPC can legitimately observe the previous one (§11) — a bounded poll, never a
/// sleep (§5).
fn wait_snapshot(path: &Path, mut ok: impl FnMut(&str) -> bool) -> bool {
    wait_until(Duration::from_secs(5), || {
        std::fs::read_to_string(path)
            .map(|t| ok(&t))
            .unwrap_or(false)
    })
}

#[test]
fn remove_node_is_snapshotted_and_the_restart_does_not_resurrect_it() {
    let run = TempRun::new();
    let state = run.state_file();
    let daemon = spawn_daemon(&run);
    let rpc = Rpc::new(run.socket());

    rpc.load_toml(&two_consoles(&run), false).expect("load");
    assert!(
        wait_snapshot(&state, |t| t.contains("drop")),
        "the load was never snapshotted: {}",
        snapshot(&state)
    );

    rpc.remove_node("drop", false).expect("remove-node drop");
    assert!(
        wait_snapshot(&state, |t| !t.contains("drop")),
        "the removal never reached the state file — the next boot would resurrect the \
         node: {}",
        snapshot(&state)
    );
    assert!(
        snapshot(&state).contains("keep"),
        "the removal took the survivor with it: {}",
        snapshot(&state)
    );

    // Stop cleanly and come back on the same paths.
    rpc.shutdown();
    drop(daemon);
    let _daemon2 = spawn_daemon(&run);
    assert_eq!(
        node_names(&rpc),
        vec!["keep".to_string()],
        "the restart recovered a node the operator had removed"
    );
}

#[test]
fn teardown_persists_the_empty_graph_and_the_restart_comes_up_empty() {
    let run = TempRun::new();
    let state = run.state_file();
    let daemon = spawn_daemon(&run);
    let rpc = Rpc::new(run.socket());

    rpc.load_toml(&two_consoles(&run), false).expect("load");
    assert!(
        wait_snapshot(&state, |t| t.contains("keep")),
        "the load was never snapshotted: {}",
        snapshot(&state)
    );

    // §11 states this as an exception worth spelling: `teardown` is the one verb that
    // persists an *empty* graph, and startup deliberately accepts an empty state file
    // (unlike an operator `--config` that parses to nothing).
    rpc.teardown();
    assert!(
        wait_snapshot(&state, |t| !t.contains("keep") && !t.contains("drop")),
        "teardown left the pre-teardown graph in the state file: {}",
        snapshot(&state)
    );
    assert!(node_names(&rpc).is_empty(), "teardown left nodes running");

    rpc.shutdown();
    drop(daemon);
    let _daemon2 = spawn_daemon(&run);
    assert!(
        node_names(&rpc).is_empty(),
        "the restart resurrected a torn-down graph: {:?}",
        node_names(&rpc)
    );
}

#[test]
fn a_clean_shutdown_preserves_the_snapshot_byte_for_byte() {
    let run = TempRun::new();
    let state = run.state_file();
    let daemon = spawn_daemon(&run);
    let rpc = Rpc::new(run.socket());

    rpc.load_toml(&two_consoles(&run), false).expect("load");
    assert!(
        wait_snapshot(&state, |t| t.contains("keep") && t.contains("drop")),
        "the load was never snapshotted: {}",
        snapshot(&state)
    );
    let before = snapshot(&state);

    // `shutdown` is not a config mutation, so it writes nothing: what the next boot
    // reads is the last mutation's file. Asserting the bytes rather than the recovered
    // graph is the point — a shutdown path that rewrote (or truncated) the snapshot
    // could still round-trip a graph while destroying, say, an attribute `dump` had
    // just persisted.
    rpc.shutdown();
    drop(daemon);
    assert_eq!(
        snapshot(&state),
        before,
        "a clean shutdown rewrote the snapshot"
    );

    let _daemon2 = spawn_daemon(&run);
    assert_eq!(
        node_names(&rpc),
        vec!["drop".to_string(), "keep".to_string()],
        "the graph did not survive a clean shutdown"
    );
    assert_eq!(
        snapshot(&state),
        before,
        "the restart rewrote a snapshot it had only read"
    );
}
