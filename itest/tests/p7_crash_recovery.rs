#![forbid(unsafe_code)]

//! Phase 7 crash-recovery slice, ported from `scripts/validate/phase7/crash-recovery.sh`
//! (plan §Phase 7 item 5; design §11/§15.9): crash recovery is exact. Restart, replug,
//! and first boot are one code path.
//!
//! The flow, and the assertions preserved from the bash:
//!   1. A serial (`free-for-all`) + pty graph is loaded, then a `log` node is *added
//!      incrementally* — the add is persisted to the state file (§11/§15.9).
//!   2. The pre-kill `dump` contains the added log node.
//!   3. A hard `kill -9` (SIGKILL) then a restart on the SAME socket + state file
//!      auto-recovers the whole graph from the persisted file — no manual reload.
//!   4. The recovered `dump` is semantically equal to the pre-kill `dump`.
//!   5. The pty symlink is recreated on restart.
//!   6. A fresh client passes the echo probe — the data plane healed end to end.
//!
//! Deviations from the bash, and why (each preserves the original *assertions*):
//! * **Semantic diff → structured value equality.** The bash normalized both dumps to
//!   canonical key-sorted JSON and `diff`ed them (`scripts/lib/semantic-diff.sh`).
//!   `dump` returns JSON here, and `serde_json::Value` equality is exactly that
//!   normalization: object comparison is key-order-insensitive, array comparison is
//!   order-sensitive — identical semantics to the bash's `sort_keys=True` normalize.
//!   This is the same round-trip check `control_plane::dump_load_dump_round_trips` uses.
//! * **Needs a serial device**, so it gates on [`serial_echo`] and **skips on macOS**
//!   (no software serial loopback there — a pty rejects `serial2`'s ioctl). The pure
//!   log-recovery-across-restart property already runs everywhere in
//!   `p3_log::rotation_counter_recovered_by_directory_scan_on_restart`; this test's
//!   distinct contribution is the integrated serial+pty+log recovery plus the echo
//!   probe, which need a device.
//! * **Hand-managed daemon lifecycle.** A hard kill + restart on the SAME
//!   socket/state-file cannot be expressed with `Daemon::start` (fresh temp dir per
//!   call), so — like `p3_log` — the daemon is a `RawDaemon` on fixed paths, killed on
//!   drop, whose readiness wait is a bounded connect-and-answer probe (not `test -S`,
//!   which would spuriously match the stale socket the hard kill leaves).
//! * **The device sim is restarted fresh** across the daemon kill (as the bash does):
//!   a real adapter releases its fd on the daemon's death, but the sim's held pty
//!   master would keep the crashed daemon's `TIOCEXCL` alive on the pts — a sim-only
//!   artifact — so the old sim is killed and a fresh one is spawned at the same path.

use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use serial_nexus_itest::{RawDaemon, Rpc, Sim, TempRun, serial_echo, wait_until};

/// Spawn a `serial-nexus-sim pty --echo` software serial device at the fixed `path` and
/// wait for its device symlink to appear. The stand-in for a plugged-in echo adapter.
fn spawn_device(path: &Path) -> Sim {
    Sim::spawn(
        &[
            "pty",
            "--echo",
            "--link",
            &path.to_string_lossy(),
            "--timeout-ms",
            "600000",
        ],
        Some(path),
    )
}

/// The sorted node names from `state` (the portable replacement for
/// `jq '[.nodes[].name]|sort'`).
fn sorted_node_names(rpc: &Rpc) -> Vec<String> {
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

/// Whether a `dump` (the config JSON, `{ node: [...], edge: [...] }`) contains a node
/// named `name` — the portable replacement for `grep -q 'name = "cap"'`.
fn dump_has_node(dump: &Value, name: &str) -> bool {
    dump.get("node")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .any(|n| n.get("name").and_then(Value::as_str) == Some(name))
        })
        .unwrap_or(false)
}

#[test]
fn kill_dash_9_then_restart_auto_recovers_graph_and_data_plane() {
    // Needs a software serial device (Linux); skip where there is none (macOS).
    if serial_echo().is_none() {
        eprintln!(
            "SKIP kill_dash_9_then_restart_auto_recovers_graph_and_data_plane: \
             no software serial device on this platform"
        );
        return;
    }

    let run = TempRun::new();
    let dev = run.join("dev1");
    let con = run.join("con");
    // The bash uses $TMPD itself as the log directory (filename = cap.log).
    let logdir = run.path().to_path_buf();

    // ---- boot: device + daemon, then load the serial+pty graph ------------------
    let device = spawn_device(&dev);
    let mut daemon = RawDaemon::start(&run);
    let rpc = Rpc::new(run.socket());

    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
arbitration = "free-for-all"
[[node]]
type = "pty"
name = "con"
path = "{con}"
[[edge]]
a = "usb0"
b = "con"
"#,
        dev = dev.display(),
        con = con.display(),
    );
    rpc.load_toml(&cfg, false).expect("load serial+pty graph");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(10)),
        "serial never active before crash: {:?}",
        rpc.node("usb0")
    );

    // ---- incrementally add a log node (persisted to the state file) -------------
    let logcfg = format!(
        r#"
[[node]]
type = "log"
name = "cap"
directory = "{dir}"
filename = "cap.log"
"#,
        dir = logdir.display(),
    );
    rpc.add_node_toml(&logcfg).expect("add-node log");

    // The pre-kill dump must carry the added log node (persistence took effect).
    let pre = rpc.dump();
    assert!(
        dump_has_node(&pre, "cap"),
        "added log node not in pre-kill dump: {pre}"
    );

    // ---- kill -9 (SIGKILL) and restart -----------------------------------------
    daemon.child().kill().expect("SIGKILL daemon 1");
    daemon.child().wait().expect("reap daemon 1");

    // The device stays "plugged in" across a daemon restart, but the sim's held master
    // would keep the crashed daemon's TIOCEXCL alive on the pts (a sim-only artifact),
    // so restart the sim fresh at the same path.
    drop(device);
    let _ = std::fs::remove_file(&dev);
    let _ = std::fs::remove_file(&con);
    let _device2 = spawn_device(&dev);
    let _daemon2 = RawDaemon::start(&run);

    // ---- the graph auto-recovered from the persisted state file (no reload) ------
    assert!(
        wait_until(Duration::from_secs(10), || sorted_node_names(&rpc)
            == ["cap", "con", "usb0"]),
        "graph did not auto-recover from the state file: {:?}",
        sorted_node_names(&rpc)
    );

    // The recovered dump is semantically equal to the pre-kill dump (§11 round-trip):
    // object keys order-insensitive, arrays order-sensitive — the semantic-diff rule.
    let post = rpc.dump();
    assert_eq!(pre, post, "recovered dump differs from pre-kill dump");

    // The pty symlink is recreated on restart.
    assert!(
        wait_until(Duration::from_secs(5), || std::fs::symlink_metadata(&con)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)),
        "PTY symlink not recreated on restart"
    );

    // The serial node is active again and the data plane healed: a fresh client's
    // seeded echo probe round-trips byte-exact (the sim verdict is the ground truth).
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(10)),
        "serial not active after restart: {:?}",
        rpc.node("usb0")
    );
    let verdict = Sim::client(&[
        "--path",
        &con.to_string_lossy(),
        "--send",
        "seeded:2KiB",
        "--expect",
        "echo",
        "--seed",
        "3",
        "--timeout-ms",
        "8000",
    ]);
    assert_eq!(
        verdict["pass"].as_bool(),
        Some(true),
        "post-restart echo probe failed: {verdict}"
    );
}
