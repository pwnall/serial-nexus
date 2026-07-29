//! The §15.40 rename's compatibility window (plan §17.3): the pre-rename default
//! socket and state file are **accepted on read for one release**.
//!
//! Why this is a guard and not a footnote. §15.40 changed the daemon's name, and every
//! default path in §10 and §11 is derived from it — so an upgrade silently moved both
//! the control socket and the state snapshot. The snapshot is the dangerous half: it is
//! daemon-owned, nobody looks at it, and the daemon's startup rule is "state file if
//! present, else `--config`, else empty". An upgraded daemon would therefore have found
//! nothing at the new path and come up with an **empty graph**, discarding every node
//! built by `add-node`/`connect` since the last `load` — the exact loss the snapshot
//! exists to prevent, delivered by a rename.
//!
//! Three properties, one per test:
//!
//! 1. a legacy snapshot beside a default socket is adopted, and the next mutation
//!    rewrites under the *current* name while the legacy file is left untouched;
//! 2. a client with no `--socket` finds a pre-rename daemon's socket;
//! 3. …but only when there is no current one — a live daemon is never passed over in
//!    favour of a stale socket from an older release.
//!
//! Fail-first (plan §3 rule 4): test 1 was proved against the unfixed tree by reverting
//! the adoption block in `daemon/src/lib.rs` in place — it fails with an empty graph,
//! which is precisely the defect. Tests 2 and 3 fail the same way with the fallback
//! removed from `ctl`'s `resolve_socket`.
//!
//! When the window closes, this file and `serial_nexus_rpc::LEGACY_DAEMON_NAME` are
//! deleted together, and the `meta_names` allowance that names them both goes with them.

use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serial_nexus_itest::{Rpc, TempRun, bin, daemon_answers, wait_until};

/// The pre-rename spelling, from its single definition — never repeated here, so this
/// file costs the `retired_names_appear_only_where_history_lives` allowance one
/// occurrence (the planted file name below) rather than a dozen.
const LEGACY: &str = serial_nexus_rpc::LEGACY_DAEMON_NAME;
const CURRENT: &str = serial_nexus_rpc::DAEMON_NAME;

/// A daemon child killed on drop. The harness's `Daemon` always passes explicit
/// `--socket` and `--state-file`, which is exactly what these tests must *not* do:
/// the whole subject is what the daemon derives when it is given neither.
struct Bare(Child);

impl Drop for Bare {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Spawn `serial-nexus-daemon` with `XDG_RUNTIME_DIR` pointed at `run` and whatever
/// `extra` flags the test needs, then wait until the socket at `sock_name` answers.
fn spawn(run: &TempRun, sock_name: &str, extra: &[&str]) -> (Bare, Rpc) {
    let socket = run.join(sock_name);
    let child = Command::new(bin(CURRENT))
        .args(extra)
        .env("XDG_RUNTIME_DIR", run.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn daemon");
    let ready = wait_until(Duration::from_secs(10), || {
        socket.exists() && daemon_answers(&socket)
    });
    assert!(
        ready,
        "daemon never answered on {} (present: {})",
        socket.display(),
        socket.exists()
    );
    (Bare(child), Rpc::new(socket))
}

/// A pty-only graph: no device, no hardware, and it survives a dump/load round trip.
fn console_config(run: &TempRun, name: &str) -> String {
    format!(
        "[[node]]\ntype = \"pty\"\nname = \"{name}\"\npath = \"{}\"\n",
        run.join(name).display()
    )
}

/// **A legacy snapshot is adopted, and the next mutation moves it forward.**
///
/// The legacy file is produced the way a real one would be — by a daemon of the old
/// spelling writing its own snapshot — rather than hand-authored, so the test cannot
/// pass against a format this daemon would never have written.
#[test]
fn a_pre_rename_state_file_is_adopted_and_rewritten_under_the_current_name() {
    let run = TempRun::new();
    let legacy_state = run.join(&format!("{LEGACY}.state.toml"));
    let current_state = run.join(&format!("{CURRENT}.state.toml"));

    // 1. An "old release" daemon: both defaults spelled the old way, which is what
    //    `--socket <dir>/<legacy>.sock` reproduces exactly (the state file is derived
    //    from the socket's stem, §11).
    {
        let legacy_sock = run.join(&format!("{LEGACY}.sock"));
        let (_d, rpc) = spawn(
            &run,
            &format!("{LEGACY}.sock"),
            &["--socket", legacy_sock.to_str().unwrap()],
        );
        rpc.load_toml(&console_config(&run, "before"), false)
            .expect("load");
        assert!(
            wait_until(Duration::from_secs(5), || legacy_state.exists()),
            "the old-spelling daemon never wrote {}",
            legacy_state.display()
        );
        rpc.shutdown();
    }
    let legacy_bytes = std::fs::read(&legacy_state).expect("read legacy state");
    assert!(
        !current_state.exists(),
        "nothing should have written the current-name state file yet"
    );

    // 2. The upgraded daemon, given neither flag — the case the rename broke.
    let (_d, rpc) = spawn(&run, &format!("{CURRENT}.sock"), &[]);
    let state = rpc.state();
    let names: Vec<String> = state["nodes"]
        .as_array()
        .expect("nodes array")
        .iter()
        .map(|n| n["name"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(
        names.iter().any(|n| n == "before"),
        "the upgraded daemon came up with {names:?} instead of adopting the pre-rename \
         snapshot — this is the empty-graph defect the rename would otherwise have \
         shipped"
    );

    // 3. The first mutation writes forward, under the current name…
    rpc.add_node_toml(&console_config(&run, "after"))
        .expect("add-node");
    assert!(
        wait_until(Duration::from_secs(5), || current_state.exists()),
        "the adopted graph was never rewritten to {}",
        current_state.display()
    );
    let written = std::fs::read_to_string(&current_state).expect("read current state");
    assert!(
        written.contains("before") && written.contains("after"),
        "the current-name snapshot should carry the adopted graph plus the new node, \
         got:\n{written}"
    );

    // 4. …and the legacy file is left exactly as it was. A file this release did not
    //    create is not this release's to delete: a downgrade must still find it.
    assert_eq!(
        std::fs::read(&legacy_state).expect("re-read legacy state"),
        legacy_bytes,
        "the legacy state file was modified; adoption is read-only"
    );
}

/// **A client with no `--socket` finds a pre-rename daemon.**
#[test]
fn a_client_falls_back_to_the_pre_rename_socket_when_there_is_no_current_one() {
    let run = TempRun::new();
    let legacy_sock = run.join(&format!("{LEGACY}.sock"));
    let (_d, _rpc) = spawn(
        &run,
        &format!("{LEGACY}.sock"),
        &["--socket", legacy_sock.to_str().unwrap()],
    );
    assert!(
        !run.join(&format!("{CURRENT}.sock")).exists(),
        "no current-name socket should exist in this fixture"
    );

    let out = Command::new(bin("serial-nexus-ctl"))
        .args(["--json", "info"])
        .env("XDG_RUNTIME_DIR", run.path())
        .output()
        .expect("run serial-nexus-ctl");
    assert!(
        out.status.success(),
        "the CLI could not reach the pre-rename daemon: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let info: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("info is JSON on stdout");
    assert!(
        info["daemon_version"].is_string(),
        "reached something, but it did not answer `info`: {info}"
    );
}

/// **…but a live current-name daemon is never passed over.**
///
/// The failure this pins is the tempting cheap ordering — try legacy first, or prefer
/// whichever exists — which would send every client on an upgraded box to a stale
/// socket file left behind by an older daemon that never got to unlink it.
#[test]
fn a_current_socket_wins_over_a_stale_pre_rename_one() {
    let run = TempRun::new();

    // A stale socket inode from an "old" daemon that died without unlinking. It has no
    // listener, so anything that connects to it fails — which is the point.
    let stale = run.join(&format!("{LEGACY}.sock"));
    std::os::unix::net::UnixListener::bind(&stale).expect("bind stale socket");
    // Dropping the listener leaves the inode behind, exactly like a killed daemon.
    assert!(stale.exists());

    let (_d, rpc) = spawn(&run, &format!("{CURRENT}.sock"), &[]);
    rpc.load_toml(&console_config(&run, "live"), false)
        .expect("load");

    let out = Command::new(bin("serial-nexus-ctl"))
        .args(["--json", "state"])
        .env("XDG_RUNTIME_DIR", run.path())
        .output()
        .expect("run serial-nexus-ctl");
    assert!(
        out.status.success(),
        "the CLI chose the stale pre-rename socket over the live current one: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let state: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("state is JSON on stdout");
    let names: Vec<&str> = state["nodes"]
        .as_array()
        .expect("nodes array")
        .iter()
        .filter_map(|n| n["name"].as_str())
        .collect();
    assert_eq!(
        names,
        vec!["live"],
        "the CLI reached the wrong daemon: {state}"
    );
}
