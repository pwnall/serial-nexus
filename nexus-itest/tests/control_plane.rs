//! Phase 2 control-plane slice, ported from `scripts/validate/phase2/control-plane.sh`
//! (design §10/§11): boot, socket perms, structural atomicity, truthful state,
//! dump→load round-trip, JSON-RPC hygiene. Needs no serial *device*, so it runs on
//! every platform — the macOS replacement for the `stat -c`/`nc -q`-riddled bash.

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use nexus_itest::{Daemon, TempRun};
use serde_json::Value;

/// A minimal valid graph: an active pty and a `waiting` serial node (device absent).
fn demo_cfg(run: &TempRun) -> String {
    format!(
        r#"
[[node]]
type = "pty"
name = "console"
path = "{console}"
[[node]]
type = "serial"
name = "usb0"
device = "{absent}"
[[edge]]
a = "usb0"
b = "console"
"#,
        console = run.join("console").display(),
        absent = run.join("absent-device").display(),
    )
}

#[test]
fn socket_permissions_are_0600() {
    let d = Daemon::start();
    // The socket permissions ARE the authorization model (§10): 0600.
    let mode = std::fs::metadata(d.socket()).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "control socket perms are {mode:o}, want 600");
}

#[test]
fn structurally_invalid_config_is_atomically_rejected() {
    let d = Daemon::start();
    let rpc = d.rpc();
    // A host↔host edge (two serial nodes) is structurally invalid; the load must be
    // rejected and leave nothing behind (§11 structural atomicity).
    let broken = r#"
[[node]]
type = "serial"
name = "a"
device = "/nonexistent/x"
[[node]]
type = "serial"
name = "b"
device = "/nonexistent/y"
[[edge]]
a = "a"
b = "b"
"#;
    assert!(
        rpc.load_toml(broken, false).is_err(),
        "structurally-invalid config was accepted"
    );
    let n = rpc.state()["nodes"].as_array().unwrap().len();
    assert_eq!(n, 0, "rejected load left {n} nodes behind");
}

#[test]
fn valid_load_reports_truthful_state_and_refuses_second_load() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let run = TempRun::new();
    let console = run.join("console");
    rpc.load_toml(&demo_cfg(&run), false).expect("valid load");

    assert!(
        rpc.wait_status("console", "active", Duration::from_secs(5)),
        "console not active: {:?}",
        rpc.node("console")
    );
    assert_eq!(
        rpc.node_status("usb0"),
        "waiting",
        "usb0 should be waiting (device absent)"
    );
    assert!(
        std::fs::symlink_metadata(&console)
            .unwrap()
            .file_type()
            .is_symlink()
    );

    // Load-on-empty: a second load on a non-empty graph is refused (§11).
    assert!(
        rpc.load_toml(&demo_cfg(&run), false).is_err(),
        "second load accepted on non-empty graph"
    );
}

#[test]
fn json_rpc_method_not_found_is_minus_32601() {
    let d = Daemon::start();
    let err = d.rpc().call("bogus", Value::Null).unwrap_err();
    assert_eq!(err.code, -32601, "method-not-found returned {}", err.code);
}

#[test]
fn dump_load_dump_round_trips() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let run = TempRun::new();
    rpc.load_toml(&demo_cfg(&run), false).expect("load");

    let dump1 = rpc.dump();
    rpc.teardown();
    assert_eq!(
        rpc.state()["nodes"].as_array().unwrap().len(),
        0,
        "teardown left nodes"
    );

    // dump returns the config as JSON — feed it straight back to load and it must
    // round-trip byte-for-byte at the config level (§11).
    rpc.load_config(dump1.clone(), false)
        .expect("reload of dump");
    let dump2 = rpc.dump();
    assert_eq!(dump1, dump2, "dump→load→dump config mismatch");
}
