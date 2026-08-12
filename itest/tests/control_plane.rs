#![forbid(unsafe_code)]

//! Phase 2 control-plane slice, ported from `scripts/validate/phase2/control-plane.sh`
//! (design §10/§11): boot, socket perms, structural atomicity, truthful state,
//! dump→load round-trip, JSON-RPC hygiene. Needs no serial *device*, so it runs on
//! every platform — the macOS replacement for the `stat -c`/`nc -q`-riddled bash.
//!
//! One addition from the Opus review (CTRL-1's other side): a connection that carries
//! **several requests** is normal and must keep working. The refusal the daemon now
//! sends is scoped to a request pipelined behind an *in-flight waiting verb*
//! (`p9_control_pipelining.rs`); it must never widen into "one request per connection",
//! which would break the §17 console's single multiplexed connection just as thoroughly
//! as the bug it replaced.

use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use serial_nexus_itest::{Daemon, TempRun};

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

/// Several requests on **one** connection are all answered, in order, each carrying its
/// own id — including two written back-to-back before either is read (the §17 console's
/// shape, and what `serial-nexus-ctl` deliberately does not do). This is the boundary of
/// the CTRL-1 refusal: only a request pipelined behind a *waiting* verb is refused
/// (`p9_control_pipelining.rs`); ordinary multiplexing must stay free.
#[test]
fn several_requests_on_one_connection_are_all_answered() {
    let d = Daemon::start();
    let mut stream = UnixStream::connect(d.socket()).expect("connect control socket");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();

    // Two fast verbs written in a single write, neither read yet.
    let batch = format!(
        "{}\n{}\n",
        json!({ "jsonrpc": "2.0", "id": 1, "method": "info" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "state" }),
    );
    stream.write_all(batch.as_bytes()).expect("write requests");
    stream.flush().expect("flush");

    // …then a third, after the first two have been served.
    let mut lines = Vec::new();
    let mut buf = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut tmp = [0u8; 8192];
    while lines.len() < 3 && Instant::now() < deadline {
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            let v: Value = serde_json::from_slice(&line[..line.len() - 1])
                .expect("each reply is one JSON line");
            lines.push(v);
            if lines.len() == 2 {
                stream
                    .write_all(
                        format!(
                            "{}\n",
                            json!({ "jsonrpc": "2.0", "id": 3, "method": "dump" })
                        )
                        .as_bytes(),
                    )
                    .expect("write the third request");
                stream.flush().expect("flush");
            }
        }
        if lines.len() >= 3 {
            break;
        }
        match stream.read(&mut tmp) {
            Ok(0) => panic!(
                "the daemon closed the connection after {} replies",
                lines.len()
            ),
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(e) => panic!("read reply {}: {e}", lines.len() + 1),
        }
    }

    assert_eq!(lines.len(), 3, "not every request was answered: {lines:?}");
    for (i, want_id) in [1i64, 2, 3].into_iter().enumerate() {
        assert_eq!(
            lines[i].get("id").and_then(Value::as_i64),
            Some(want_id),
            "reply {i} is out of order or misidentified: {}",
            lines[i]
        );
        assert!(
            lines[i].get("result").is_some(),
            "request {want_id} on a shared connection was refused: {}",
            lines[i]
        );
    }
}
