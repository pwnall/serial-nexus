//! Phase 8 agent-task slice, ported from `scripts/validate/phase8/agent-task.sh`
//! (design §6, §7.3, §10). The full operator scenario driven purely through
//! structured RPC (the Rust stand-in for §15.16's `serialnexusctl --json | jq`
//! feedback loop): inspect state, lock a channel, send a command, verify the device
//! received exactly it, rotate its log, verify cross-rotation continuity, unlock.
//!
//! Topology (no hardware, §15.17):
//!
//! ```text
//!   [ log "cap" ]──edge(never)──┐
//!                               ├─[ serial "usb0" exclusive ] ─▶ nexus-sim pty --echo
//!   [ pty "console" ]──edge─────┘        (device double)            (echoes hostward)
//! ```
//!
//! Every command is a KNOWN line, so a byte-exact SHA-256 is an independent oracle
//! (§5): the echo device reflects each command hostward, the `log` captures it, and
//! the concatenation of rotated + live log files must equal the exact command
//! transcript — proving exclusivity (a locked-out `send` leaks nothing) and lossless
//! rotation. Needs a serial *device*, so it skips where none exists (macOS): the echo
//! double comes from [`serial_echo`].

use std::path::{Path, PathBuf};
use std::time::Duration;

use nexus_itest::{Daemon, Rpc, serial_echo, sha256_hex, wait_until};
use serde_json::Value;

/// The daemon's application error code for a contended `lock`/`send` (§6): the
/// `AppError::Locked` variant, `APP_ERROR_BASE (-32000) - 3`. Hard-coded because the
/// harness pulls in only `nexus_itest` + std + `serde_json` (no `nexus-rpc` dep).
const LOCKED_CODE: i64 = -32003;

/// The combined bytes of a log's rotated (`<base>.NNN`, chronological) then live
/// (`<base>`) files — the portable replacement for `cat "$DIR"/console.log.* "$DIR"/console.log`.
/// Zero-padded suffixes make a lexical sort chronological (higher is newer, §7.3).
fn combined_log_bytes(logdir: &Path, base: &str) -> Vec<u8> {
    let prefix = format!("{base}.");
    let mut rotated: Vec<PathBuf> = std::fs::read_dir(logdir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.starts_with(&prefix))
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    rotated.sort();

    let mut out = Vec::new();
    for f in &rotated {
        if let Ok(b) = std::fs::read(f) {
            out.extend_from_slice(&b);
        }
    }
    if let Ok(b) = std::fs::read(logdir.join(base)) {
        out.extend_from_slice(&b);
    }
    out
}

/// Wait until the combined (rotated + live) log length equals `want` — a bounded poll
/// on the byte-exact on-disk length, never a bare sleep (§5).
fn wait_combined_len(logdir: &Path, base: &str, want: usize, timeout: Duration) -> bool {
    wait_until(timeout, || combined_log_bytes(logdir, base).len() == want)
}

#[test]
fn agent_task_full_operator_scenario() {
    let Some(echo) = serial_echo() else {
        eprintln!("SKIP agent_task_full_operator_scenario: no serial device on this platform");
        return;
    };
    let d = Daemon::start();
    let rpc: &Rpc = d.rpc();

    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log directory");
    let console = d.run().join("console");
    let base = "console.log";

    // The scenario graph: an EXCLUSIVE serial node fronting the echo device, a pty
    // console writer, and a capturing log wired hostward-only (`write_mode = never`).
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
arbitration = "exclusive"
[[node]]
type = "pty"
name = "console"
path = "{console}"
[[node]]
type = "log"
name = "cap"
directory = "{logdir}"
filename = "{base}"
[[edge]]
a = "usb0"
b = "console"
[[edge]]
a = "usb0"
b = "cap"
write_mode = "never"
"#,
        dev = echo.device().display(),
        console = console.display(),
        logdir = logdir.display(),
        base = base,
    );
    rpc.load_toml(&cfg, false).expect("load agent-task config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 never reached active: {:?}",
        rpc.node("usb0")
    );
    assert!(
        rpc.wait_status("console", "active", Duration::from_secs(10)),
        "console never reached active: {:?}",
        rpc.node("console")
    );

    // 1. INSPECT STATE — the graph is healthy and the write lock is free.
    let usb0 = rpc.node("usb0").expect("usb0 in state");
    assert_eq!(
        usb0["status"].as_str(),
        Some("active"),
        "step 1: usb0 not active"
    );
    assert_eq!(
        usb0["lock"]["holder"],
        Value::Null,
        "step 1: usb0 lock not free"
    );

    // 2. LOCK A CHANNEL — the operator grabs the console's write floor.
    let locked = rpc
        .lock("console", false, false, None)
        .expect("step 2: lock console");
    assert_eq!(
        locked["acquired"].as_bool(),
        Some(true),
        "step 2: lock did not acquire: {locked}"
    );
    assert_eq!(
        locked["held"].as_bool(),
        Some(true),
        "step 2: lock not held: {locked}"
    );
    assert_eq!(
        rpc.node("usb0").expect("usb0")["lock"]["holder"].as_str(),
        Some("console"),
        "step 2: state does not show console as the holder"
    );

    // 3. NEGATIVE CONTROL — a competing plain `send` is refused (exclusivity holds).
    let denied = rpc
        .send("usb0", "denied", false, 300)
        .expect_err("step 3: contended send should have failed");
    assert_eq!(
        denied.code, LOCKED_CODE,
        "step 3: send error was not the locked error: [{}] {}",
        denied.code, denied.message
    );
    assert!(
        denied.message.to_lowercase().contains("lock"),
        "step 3: send error message was not the locked error: {}",
        denied.message
    );

    // 4. SEND A COMMAND — the operator escalates with the steal escape hatch and fires
    //    a one-shot command atomically (acquire-write-release, taking the floor).
    let sent = rpc
        .send("usb0", "reboot", true, 5000)
        .expect("step 4: steal send");
    assert_eq!(
        sent["delivered"].as_bool(),
        Some(true),
        "step 4: send --steal did not deliver: {sent}"
    );
    assert_eq!(
        sent["sent"].as_u64(),
        Some(7),
        "step 4: 'reboot\\n' should be 7 bytes: {sent}"
    );

    // 5. VERIFY THE DEVICE RECEIVED IT — the echo device reflects "reboot\n" hostward
    //    and the log captures it. The locked-out "denied" from step 3 left no trace.
    assert!(
        wait_combined_len(&logdir, base, 7, Duration::from_secs(10)),
        "step 5: log never captured the 7-byte echo of 'reboot'"
    );
    assert_eq!(
        sha256_hex(&combined_log_bytes(&logdir, base)),
        sha256_hex(b"reboot\n"),
        "step 5: device did not receive exactly 'reboot' (or 'denied' leaked)"
    );

    // 6. ROTATE ITS LOG + VERIFY CONTINUITY — a stream split across a rotation loses
    //    nothing: rotated + live files concatenate to the exact command transcript.
    rpc.send("usb0", "status", false, 5000)
        .expect("step 6: send 'status'");
    assert!(
        wait_combined_len(&logdir, base, 14, Duration::from_secs(10)),
        "step 6: log did not reach 14 bytes after 'status'"
    );

    rpc.rotate("cap").expect("step 6: rotate");
    let rotated_000 = logdir.join("console.log.000");
    assert!(
        wait_until(Duration::from_secs(5), || rotated_000.exists()),
        "step 6: rotation did not create console.log.000"
    );
    assert!(
        wait_until(Duration::from_secs(5), || {
            rpc.node("cap")
                .and_then(|n| n.get("rotation").and_then(Value::as_u64))
                == Some(0)
        }),
        "step 6: rotation counter did not advance to 0: {:?}",
        rpc.node("cap").map(|n| n["rotation"].clone())
    );

    rpc.send("usb0", "ping", false, 5000)
        .expect("step 6: send 'ping'");
    assert!(
        wait_combined_len(&logdir, base, 19, Duration::from_secs(10)),
        "step 6: combined log did not reach 19 bytes after 'ping'"
    );
    assert_eq!(
        sha256_hex(&combined_log_bytes(&logdir, base)),
        sha256_hex(b"reboot\nstatus\nping\n"),
        "step 6: rotation lost/duplicated bytes (continuity broken)"
    );

    // 7. UNLOCK — re-acquire and explicitly release, leaving the floor free.
    let relock = rpc
        .lock("console", false, false, None)
        .expect("step 7: re-lock");
    assert_eq!(
        relock["acquired"].as_bool(),
        Some(true),
        "step 7: re-lock failed: {relock}"
    );
    let unlocked = rpc.unlock("console").expect("step 7: unlock");
    assert_eq!(
        unlocked["released"].as_bool(),
        Some(true),
        "step 7: unlock did not release: {unlocked}"
    );
    assert_eq!(
        rpc.node("usb0").expect("usb0")["lock"]["holder"],
        Value::Null,
        "step 7: lock still held after unlock"
    );
}
