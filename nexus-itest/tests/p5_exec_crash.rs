//! Phase 5 exec-codec crash-containment slice, ported from
//! `scripts/validate/phase5/exec-crash.sh` (design §7.6 exec lifecycle, §15.22 the
//! child-pipe boundary + concurrent pump).
//!
//! An exec codec — a child process speaking the shared envelope on stdin/stdout —
//! sits between a `serial` and a channel `pty`, echoing a full round-trip. The test
//! proves crash containment: a `kill -9` of the child mid-life faults the node and
//! restarts it within the backoff (observed via the flattened `restart_count` state);
//! a fresh round-trip afterward checksums clean (the restarted child resumed); and a
//! concurrent echo on an *unrelated* serial keeps passing throughout (the data plane
//! never wedged, §15.22).
//!
//! Deviations from the bash, and why (each preserves the original assertions):
//! * The two `nexus-sim pty --echo` "devices" become two [`serial_echo`] doubles.
//!   Both are Linux-only (a pty cannot be a `serial` device on macOS — `serial2` →
//!   `ENOTTY`, see the crate docs), so — like `p3_log`'s serial-fed checks — this
//!   test **skips** where no software serial device exists. The exec codec's
//!   crash-and-resume property is demonstrated *through* the serial round-trip (that
//!   is what proves "the data plane did not wedge"), so it genuinely needs the device.
//! * `pkill -9 -f "$CHILD"` becomes a `/proc` scan (Linux, where this test runs) that
//!   SIGKILLs the one process whose argv carries this run's **unique** child-script
//!   copy — the faithful, shell-free equivalent of the bash's per-run copy + `pkill`.
//! * Every claim pins to structured RPC state (`.status`/`.codec`/`.restart_count`,
//!   all flattened onto the node by the daemon) or a byte-exact `nexus-sim client`
//!   `pass`/checksum verdict — never parsed CLI text (§5).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use nexus_itest::{Daemon, Rpc, Sim, serial_echo, wait_until};
use serde_json::Value;

/// Absolute path to a fixture under the workspace's `tests/ext-codec/`, derived from
/// this crate's compile-time manifest dir — the portable replacement for the bash's
/// `REPO_ROOT` dance (same pattern as `p5_exec_conformance`).
fn ext_codec(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("nexus-itest has a parent (the workspace root)")
        .join("tests")
        .join("ext-codec")
        .join(name)
}

/// Whether `python3` is invocable — the exec child is a Python script. Absent ⇒ skip
/// (an environmental prerequisite, like a missing serial device).
fn have_python3() -> bool {
    Command::new("python3")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

/// The node's flattened `restart_count` from `state` (0 if the field/node is absent) —
/// the portable replacement for the bash `jq -r '…|.restart_count'`.
fn restart_count(rpc: &Rpc, node: &str) -> u64 {
    rpc.node(node)
        .and_then(|n| n.get("restart_count").and_then(Value::as_u64))
        .unwrap_or(0)
}

/// SIGKILL every process whose raw `/proc/<pid>/cmdline` contains `needle`, returning
/// how many were signalled. `needle` is this run's unique child-script path, which only
/// the exec codec's child carries — so this never touches the daemon or the test binary
/// (whose cmdlines do not contain it). The shell-free stand-in for `pkill -9 -f`.
///
/// `/proc` is Linux-only; on other platforms `read_dir` fails and this returns 0. The
/// test never reaches it off Linux (it skips earlier for want of a serial device), so
/// the function only needs to *compile* everywhere, which it does.
fn kill_procs_matching(needle: &str) -> usize {
    let mut killed = 0usize;
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return 0;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(pid) = file_name.to_str() else {
            continue;
        };
        if pid.is_empty() || !pid.bytes().all(|b| b.is_ascii_digit()) {
            continue; // only numeric pid directories
        }
        let Ok(cmdline) = std::fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        // cmdline is NUL-separated argv; match the needle as a raw byte substring.
        let hit = cmdline
            .windows(needle.len())
            .any(|w| w == needle.as_bytes());
        if hit {
            let _ = Command::new("kill").arg("-9").arg(pid).status();
            killed += 1;
        }
    }
    killed
}

/// One seeded echo probe on the *unrelated* serial path (client → `tty2` → usb1 → echo
/// device → back), sized `size` bytes with `seed == size`, exactly as the bash `probe`.
/// Returns the `nexus-sim client` verdict (`pass`/checksum ground truth).
fn probe(tty: &Path, size: u64) -> Value {
    let path = tty.to_string_lossy().into_owned();
    let send = format!("seeded:{size}");
    let seed = size.to_string();
    Sim::client(&[
        "--path",
        &path,
        "--send",
        &send,
        "--expect",
        "echo",
        "--seed",
        &seed,
        "--timeout-ms",
        "20000",
    ])
}

/// One 256 KiB round-trip through the exec codec (client → `tty-c0` → c0 → child →
/// serial → echo device → back), seeded `seed`, exactly as the bash `roundtrip`. 256
/// KiB overfills the child's 64 KiB pipes many times, also proving the pump does not
/// deadlock under sustained flow (the two directions polled concurrently, §15.22).
fn roundtrip(tty: &Path, seed: u64) -> Value {
    let path = tty.to_string_lossy().into_owned();
    let seed = seed.to_string();
    Sim::client(&[
        "--path",
        &path,
        "--send",
        "seeded:256KiB",
        "--expect",
        "echo",
        "--seed",
        &seed,
        "--timeout-ms",
        "30000",
    ])
}

#[test]
fn exec_codec_crash_faults_restarts_and_data_plane_survives() {
    // Two echo "devices": the exec path (usb0) and the unrelated probe (usb1). Both are
    // Linux-only software serial; absent ⇒ skip (the round-trip needs a real device).
    let Some(echo0) = serial_echo() else {
        eprintln!(
            "SKIP exec_codec_crash_faults_restarts_and_data_plane_survives: no serial device"
        );
        return;
    };
    let Some(echo1) = serial_echo() else {
        eprintln!(
            "SKIP exec_codec_crash_faults_restarts_and_data_plane_survives: no serial device"
        );
        return;
    };
    if !have_python3() {
        eprintln!(
            "SKIP exec_codec_crash_faults_restarts_and_data_plane_survives: python3 not found"
        );
        return;
    }

    let d = Daemon::start();
    let rpc = d.rpc();

    // A per-run copy of the child so the /proc match targets ONLY this run's child (the
    // bash copied it under $TMPD for the same reason). Its unique path is the needle.
    let child = d.run().join("passthrough-codec.py");
    std::fs::copy(ext_codec("passthrough-codec.py"), &child).expect("copy child codec");
    let child_path = child.to_string_lossy().into_owned();

    let tty_c0 = d.run().join("tty-c0");
    let tty2 = d.run().join("tty2");

    // usb0 → exec codec (held) → con-c0 (the exec path); usb1 → con2 (the unrelated
    // echo probe). The exec codec's channel is free-for-all so the client writes freely.
    // `[node.attributes]` is a dotted sub-table (equivalent to the bash's inline table)
    // to avoid `{`/`}` colliding with `format!`.
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev0}"

[[node]]
type = "codec"
name = "mux"
codec = "exec"
faces = "target"
channels = ["c0"]
arbitration = "free-for-all"
[node.attributes]
argv = ["python3", "{child}", "c0"]
restart_backoff_ms = 150

[[node]]
type = "pty"
name = "con-c0"
path = "{tty_c0}"

[[node]]
type = "serial"
name = "usb1"
device = "{dev1}"
arbitration = "free-for-all"

[[node]]
type = "pty"
name = "con2"
path = "{tty2}"

[[edge]]
a = "usb0"
b = "mux"
write_mode = "held"

[[edge]]
a = "mux/c0"
b = "con-c0"

[[edge]]
a = "usb1"
b = "con2"
"#,
        dev0 = echo0.device().display(),
        dev1 = echo1.device().display(),
        child = child_path,
        tty_c0 = tty_c0.display(),
        tty2 = tty2.display(),
    );
    rpc.load_toml(&cfg, false).expect("load exec-crash graph");

    // The exec codec starts a child and reports active + codec=="exec" (bash line 63).
    let mux_ready = wait_until(Duration::from_secs(8), || {
        rpc.node("mux").is_some_and(|n| {
            n.get("status").and_then(Value::as_str) == Some("active")
                && n.get("codec").and_then(Value::as_str) == Some("exec")
        })
    });
    assert!(
        mux_ready,
        "exec codec never became active: {:?}",
        rpc.node("mux")
    );

    // The boundary nodes come up and their PTY symlinks appear before we drive bytes.
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );
    assert!(
        rpc.wait_status("usb1", "active", Duration::from_secs(20)),
        "usb1 not active: {:?}",
        rpc.node("usb1")
    );
    assert!(
        rpc.wait_status("con-c0", "active", Duration::from_secs(10)),
        "con-c0 not active: {:?}",
        rpc.node("con-c0")
    );
    assert!(
        rpc.wait_status("con2", "active", Duration::from_secs(10)),
        "con2 not active: {:?}",
        rpc.node("con2")
    );
    assert!(
        wait_until(Duration::from_secs(5), || tty_c0.exists() && tty2.exists()),
        "console PTY symlinks never appeared"
    );

    // The unrelated echo probe and the exec-codec round-trip both pass BEFORE the crash.
    let p = probe(&tty2, 4096);
    assert_eq!(
        p["pass"].as_bool(),
        Some(true),
        "unrelated echo probe failed before the crash: {p}"
    );
    let r = roundtrip(&tty_c0, 11);
    assert_eq!(
        r["pass"].as_bool(),
        Some(true),
        "exec-codec round-trip failed before the crash: {r}"
    );

    // Kill the child mid-life; the node must fault and restart within the backoff, so
    // restart_count strictly increases and the node returns to active.
    let before = restart_count(rpc, "mux");
    let killed = kill_procs_matching(&child_path);
    assert!(killed >= 1, "could not find the exec child to kill");

    let restarted = wait_until(Duration::from_secs(8), || {
        restart_count(rpc, "mux") > before
    });
    assert!(
        restarted,
        "restart_count did not increase after kill -9 (before={before}, now={})",
        restart_count(rpc, "mux")
    );
    assert!(
        rpc.wait_status("mux", "active", Duration::from_secs(8)),
        "exec codec did not return to active after restart: {:?}",
        rpc.node("mux")
    );

    // The unrelated echo probe still passes — the data plane never wedged.
    let p2 = probe(&tty2, 5121);
    assert_eq!(
        p2["pass"].as_bool(),
        Some(true),
        "unrelated echo probe failed after the crash (data plane wedged): {p2}"
    );

    // A fresh round-trip through the restarted child checksums clean (resumed).
    let r2 = roundtrip(&tty_c0, 22);
    assert_eq!(
        r2["pass"].as_bool(),
        Some(true),
        "exec-codec round-trip failed after restart (did not resume clean): {r2}"
    );

    let after = restart_count(rpc, "mux");
    assert!(
        after > before,
        "restart_count did not advance across the crash (before={before}, after={after})"
    );
}
