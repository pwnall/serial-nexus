//! Phase 8 tap-drops slice, ported from `scripts/validate/phase8/tap-drops.sh`
//! (design §5 / §17): a slow tap costs only itself. An unread ("paused browser tab")
//! tap's bounded per-connection queue fills and drops-with-a-counter, while a
//! co-attached `log` consumer of the *same* hostward endpoint stays byte-exact and
//! complete — §5's "a slow spy costs itself data, never its neighbors", in its
//! dynamic-attachment (§17 tap) form.
//!
//! Two properties, both preserved from the bash:
//!
//! 1. The unread tap's own `dropped` counter climbs above zero while it is open
//!    (`state.taps[0].dropped > 0`).
//! 2. The co-attached log reaches the full sourced size and matches the source
//!    byte-for-byte (`sha256`), and drops nothing — the slow tap never starved or
//!    corrupted its neighbor.
//!
//! Ground truth for the data-plane claim is a byte-exact SHA-256 (`sha256_hex`), never
//! a judgement (§5): the log's bytes must equal the sourced stream's `sha256_sent`.
//!
//! Deviations from the bash, and why (each preserves the original *assertions*):
//!
//! * The bash sourced 8 MiB hostward with a `pty --source` device and read the source
//!   checksum from that sim's stdout `.sha256`. `Sim::spawn` discards a sim's stdout,
//!   so — exactly as `p3_log.rs` does — this drives an **echo** device (`serial_echo`)
//!   with a seeded `client` batch and uses the client verdict's `sha256_sent` as the
//!   hostward stream's ground truth (the echo returns exactly what was sent). This
//!   needs a serial *device*, so the test **skips** where none exists (macOS).
//! * The "paused tab" is a raw `tap.open` [`Subscription`] this test opens and then
//!   never reads: its OS socket buffer fills, the daemon's async write to that
//!   connection parks (never blocking the shared runtime — the whole §15.20 point),
//!   that connection's bounded tap queue (`TAP_QUEUE_CAP` = 128 chunks) fills, and the
//!   hub drops-with-counter. This is precisely the stalled-tab condition the bash
//!   produced via `serialnexusctl tap … --stall-ms`.
//! * 16 MiB (up from the bash's 8 MiB): the tap queue is 128 chunks of `READ_BUF`
//!   (64 KiB) = up to 8 MiB, plus the two socket buffers, so 8 MiB is marginal for
//!   *guaranteeing* a drop; 16 MiB comfortably overflows the tap boundary while the
//!   deep (16384-chunk) `hostward_buffer` keeps the co-attached log lossless.

use std::path::Path;
use std::time::Duration;

use nexus_itest::{Daemon, Rpc, Sim, serial_echo, sha256_hex, wait_until};
use serde_json::{Value, json};

/// The sourced hostward volume — 16 MiB, comfortably over the tap queue + socket
/// buffer bound so the unread tap is guaranteed to drop (see the module note).
const N: u64 = 16 * 1024 * 1024;
/// Seed for the seeded echo batch (matches the bash's `SEED=11`).
const SEED: u64 = 11;

/// Current on-disk length of `p` (0 if absent) — the portable replacement for the
/// bash's `stat -c %s … || echo 0`.
fn file_len(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

/// Number of open taps reported in `state` (§17).
fn taps_len(rpc: &Rpc) -> usize {
    rpc.state()["taps"].as_array().map_or(0, |a| a.len())
}

/// The first open tap's own drop counter (§5 per-tap `dropped`, not the endpoint's
/// `feed_dropped`), 0 if absent.
fn tap0_dropped(rpc: &Rpc) -> u64 {
    rpc.state()["taps"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|t| t.get("dropped"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

/// Drive one seeded batch through an echo device: write `send_spec` (e.g.
/// `seeded:16MiB`) into `tty`, read the echo back concurrently, and return the
/// `client` verdict (whose `sha256_sent` is the batch's byte-exact ground truth —
/// the echo returns exactly what was sent). Mirrors `p3_log.rs`'s `echo_send`.
fn echo_send(tty: &Path, send_spec: &str, seed: u64) -> Value {
    let path = tty.to_string_lossy().into_owned();
    let seed = seed.to_string();
    Sim::client(&[
        "--path",
        &path,
        "--send",
        send_spec,
        "--expect",
        "echo",
        "--seed",
        &seed,
        "--timeout-ms",
        "60000",
    ])
}

#[test]
fn slow_tap_drops_while_coattached_log_stays_byte_exact() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP slow_tap_drops_while_coattached_log_stays_byte_exact: no serial device on this platform"
        );
        return;
    };
    let d = Daemon::start();
    let rpc = d.rpc();
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log directory");
    let console = d.run().join("console");

    // A free-for-all serial node whose hostward stream fans out to a capturing log and
    // an injecting pty console (the console lets a `client` write targetward so the
    // echo device returns the same bytes hostward — the sourced stream). The deep
    // `hostward_buffer` keeps the log's fan-out lossless; the tap uses its own separate
    // bounded queue.
    let cfg = format!(
        r#"
[[node]]
type = "pty"
name = "console"
path = "{console}"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
arbitration = "free-for-all"
hostward_buffer = 16384
[[node]]
type = "log"
name = "logx"
directory = "{logdir}"
filename = "serial.log"
[[edge]]
a = "usb0"
b = "console"
[[edge]]
a = "usb0"
b = "logx"
"#,
        console = console.display(),
        dev = echo.device().display(),
        logdir = logdir.display(),
    );
    rpc.load_toml(&cfg, false).expect("load tap-drops config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );
    assert!(
        rpc.wait_status("console", "active", Duration::from_secs(10)),
        "console not active: {:?}",
        rpc.node("console")
    );
    assert!(
        wait_until(Duration::from_secs(5), || console.exists()),
        "console pty symlink never appeared"
    );

    // A paused tab (§17): open a tap on usb0 and then NEVER read it. The stream's OS
    // socket buffer fills, the daemon's async write to that connection parks, the
    // connection's bounded tap queue fills, and the hub drops-with-counter — a slow spy
    // costing only itself. Holding `stalled_tap` in scope keeps the tap registered.
    let stalled_tap = rpc.stream("tap.open", json!({ "endpoint": "usb0" }));
    assert!(
        wait_until(Duration::from_secs(5), || taps_len(rpc) == 1),
        "stalled tap did not register (taps={})",
        rpc.state()["taps"]
    );

    // Release the source: 16 MiB flows hostward to the log (fast, byte-exact) and to
    // the stalled tap (queue fills → drops). The echo returns exactly what was sent, so
    // the client verdict's `sha256_sent` is the hostward stream's ground truth.
    let v = echo_send(&console, "seeded:16MiB", SEED);
    assert_eq!(
        v["pass"].as_bool(),
        Some(true),
        "16 MiB echo did not round-trip: {v}"
    );
    assert_eq!(
        v["received"].as_u64(),
        Some(N),
        "echo received != 16 MiB: {v}"
    );
    let sent_sha = v["sha256_sent"]
        .as_str()
        .expect("client reported sha256_sent")
        .to_owned();

    // Property 1: while the stalled tap is still open, its own drop counter climbs
    // above zero (bounded poll on structured state — no bare sleep).
    assert!(
        wait_until(Duration::from_secs(15), || tap0_dropped(rpc) > 0),
        "the unread tap recorded no drops (state={})",
        rpc.state()
    );
    let dropped = tap0_dropped(rpc);

    // Property 2: the co-attached log reaches the full sourced size and matches the
    // source byte-for-byte, despite the tap dropping — a slow spy never starves or
    // corrupts its neighbor.
    let logfile = logdir.join("serial.log");
    assert!(
        wait_until(Duration::from_secs(30), || file_len(&logfile) >= N),
        "log did not reach {N} bytes — a slow tap starved a neighbor (len={})",
        file_len(&logfile)
    );
    let data = std::fs::read(&logfile).expect("read serial.log");
    assert_eq!(
        data.len() as u64,
        N,
        "serial.log length != 16 MiB (captured {} bytes)",
        data.len()
    );
    assert_eq!(
        sha256_hex(&data),
        sent_sha,
        "log checksum != source — the slow tap corrupted a neighbor's stream"
    );

    // The neighbor's own drop counter confirms completeness (§5 all-loss-counted).
    let log_dropped = rpc.node("logx").expect("logx node")["dropped_bytes"]
        .as_u64()
        .expect("dropped_bytes present");
    assert_eq!(
        log_dropped, 0,
        "the co-attached log dropped bytes — a slow tap starved a neighbor"
    );

    eprintln!(
        "tap dropped {dropped} bytes while the co-attached log stayed byte-exact and complete"
    );

    // Clean up the stalled tap: dropping the subscription closes its connection, which
    // detaches the tap from its hub.
    drop(stalled_tap);
}
