//! Phase 3 log-node slice, ported from `scripts/validate/phase3/log.sh`
//! (design §7.3). Three properties:
//!
//! 1. A `log` captures the whole hostward stream with no loss (`dropped_bytes == 0`).
//! 2. On-demand `rotate` loses nothing and numbers higher-is-newer: each batch lands
//!    in exactly its own file (`.000`, `.001`, then the live file), split at no chunk
//!    boundary.
//! 3. The rotation counter is recovered by a directory scan across a hard daemon
//!    restart (never persisted); the next rotation numbers `.002`, never a clobbering
//!    `.000`, and existing rotations survive untouched.
//! 4. **Rotation is ordered against the queue** (review LOG-3): bytes accepted after
//!    a `rotate` request can never land in the pre-rotation file — asserted while the
//!    writer is *deliberately stuck mid-write*, which is the only state in which the
//!    pre-fix "rotate between batches" shape misfiled them.
//!
//! Ground truth for every data-plane claim is a byte-exact SHA-256 (`sha256_hex`) or
//! the sim's reported `sha256_sent`, never a judgement (§5).
//!
//! Deviations from the bash, and why (each preserves the original *assertions*):
//! * The bash sourced the hostward stream with a `pty --source` device and compared
//!   the log to the source's `.sha256`. `nexus-sim pty --source` writes that checksum
//!   only to stdout, which the harness's `Sim::spawn` discards, so checks 1/2 instead
//!   drive an **echo** device (`serial_echo`) with a seeded `client` batch and use the
//!   client verdict's `sha256_sent` as ground truth — the identical
//!   "log captures the hostward stream byte-exact, zero drops" property, over the
//!   sanctioned single-device helper. Checks 1/2 need a serial device, so they skip
//!   where none exists (macOS).
//! * Check 3's directory-scan recovery is a pure log-node property independent of any
//!   serial device, so it runs **everywhere** over a lone `log` node whose empty
//!   rotations exercise scan recovery + no-clobber exactly as the bash's content-laden
//!   ones did (the sha-stability assertion holds regardless of file contents).

use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use nexus_itest::{Daemon, Rpc, Sim, TempRun, bin, serial_echo, sha256_hex, wait_until};
use serde_json::Value;

const SIZE_256K: u64 = 256 * 1024;
const SIZE_32K: u64 = 32 * 1024;

/// Current on-disk length of `p` (0 if absent) — the portable replacement for
/// `stat -c %s … || echo 0`.
fn file_len(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

/// Drive one seeded batch through an echo device and verify the full round trip:
/// write `send_spec` (e.g. `seeded:32KiB`) into `tty`, read the echo back, and return
/// the `client` verdict (whose `sha256_sent` is the batch's byte-exact ground truth).
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
        "30000",
    ])
}

/// Wait until the log node's observed `rotation` counter equals `want` (§7.3 state,
/// never persisted). Bounded poll on structured RPC state — no bare sleep.
fn wait_rotation(rpc: &Rpc, node: &str, want: u64, timeout: Duration) -> bool {
    wait_until(timeout, || {
        rpc.node(node)
            .and_then(|n| n.get("rotation").and_then(Value::as_u64))
            == Some(want)
    })
}

// ---- Check 1: the log captures the whole hostward stream, no loss (§7.3) --------

#[test]
fn log_captures_hostward_stream_without_loss() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP log_captures_hostward_stream_without_loss: no serial device on this platform"
        );
        return;
    };
    let d = Daemon::start();
    let rpc = d.rpc();
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log directory");
    let console = d.run().join("console");

    // A free-for-all serial node feeds every hostward byte to a capturing log; a pty
    // console injects a 256 KiB seeded batch that the echo device returns hostward.
    let cfg = format!(
        r#"
[[node]]
type = "pty"
name = "console"
path = "{console}"
[[node]]
type = "serial"
name = "usb0"
arbitration = "free-for-all"
device = "{dev}"
[[node]]
type = "log"
name = "cap"
directory = "{logdir}"
filename = "cap.log"
[[edge]]
a = "usb0"
b = "console"
[[edge]]
a = "usb0"
b = "cap"
"#,
        console = console.display(),
        dev = echo.device().display(),
        logdir = logdir.display(),
    );
    rpc.load_toml(&cfg, false).expect("load capture config");
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

    let v = echo_send(&console, "seeded:256KiB", 7);
    assert_eq!(
        v["pass"].as_bool(),
        Some(true),
        "256 KiB echo did not round-trip: {v}"
    );
    assert_eq!(
        v["received"].as_u64(),
        Some(SIZE_256K),
        "echo received != 256 KiB: {v}"
    );
    let sent_sha = v["sha256_sent"]
        .as_str()
        .expect("client reported sha256_sent")
        .to_owned();

    // The log must reach the full sourced size, then match the source byte-for-byte.
    let cap = logdir.join("cap.log");
    assert!(
        wait_until(Duration::from_secs(15), || file_len(&cap) >= SIZE_256K),
        "log never reached the sourced size (queued={:?})",
        rpc.node("cap").map(|n| n["queued_bytes"].clone())
    );
    let data = std::fs::read(&cap).expect("read cap.log");
    assert_eq!(
        data.len() as u64,
        SIZE_256K,
        "cap.log length != 256 KiB (captured {} bytes)",
        data.len()
    );
    assert_eq!(
        sha256_hex(&data),
        sent_sha,
        "log checksum != source checksum (lossy capture)"
    );

    let dropped = rpc.node("cap").expect("cap node")["dropped_bytes"]
        .as_u64()
        .expect("dropped_bytes present");
    assert_eq!(
        dropped, 0,
        "log dropped_bytes should be 0 for a keep-up disk"
    );
}

// ---- Check 2: rotation loses nothing; each batch lands in its own file (§7.3) ----

#[test]
fn rotation_loses_nothing_each_batch_in_its_own_file() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP rotation_loses_nothing_each_batch_in_its_own_file: no serial device on this platform"
        );
        return;
    };
    let d = Daemon::start();
    let rpc = d.rpc();
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log directory");
    let console = d.run().join("console");

    let cfg = format!(
        r#"
[[node]]
type = "pty"
name = "console"
path = "{console}"
[[node]]
type = "serial"
name = "usb0"
arbitration = "free-for-all"
device = "{dev}"
[[node]]
type = "log"
name = "rot"
directory = "{logdir}"
filename = "rot.log"
[[edge]]
a = "usb0"
b = "console"
[[edge]]
a = "usb0"
b = "rot"
"#,
        console = console.display(),
        dev = echo.device().display(),
        logdir = logdir.display(),
    );
    rpc.load_toml(&cfg, false).expect("load rotation config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );
    assert!(
        rpc.wait_status("console", "active", Duration::from_secs(10)),
        "console not active"
    );
    assert!(
        wait_until(Duration::from_secs(5), || console.exists()),
        "console pty symlink never appeared"
    );

    let rot_log = logdir.join("rot.log");

    // Batch A -> current file; rotate -> rot.log.000 must equal exactly A.
    let a = echo_send(&console, "seeded:32KiB", 1);
    assert_eq!(a["pass"].as_bool(), Some(true), "batch A echo failed: {a}");
    assert_eq!(a["received"].as_u64(), Some(SIZE_32K), "batch A short: {a}");
    let a_sha = a["sha256_sent"].as_str().expect("A sha256_sent").to_owned();
    assert!(
        wait_until(Duration::from_secs(10), || file_len(&rot_log) >= SIZE_32K),
        "batch A not logged"
    );
    rpc.rotate("rot").expect("rotate 1");
    assert!(
        wait_rotation(rpc, "rot", 0, Duration::from_secs(5)),
        "rotation did not reach 0"
    );
    let f000 = logdir.join("rot.log.000");
    assert_eq!(
        sha256_hex(&std::fs::read(&f000).expect("read rot.log.000")),
        a_sha,
        "rot.log.000 != batch A"
    );

    // Batch B -> fresh current file; rotate -> rot.log.001 must equal exactly B.
    let b = echo_send(&console, "seeded:32KiB", 2);
    assert_eq!(b["pass"].as_bool(), Some(true), "batch B echo failed: {b}");
    assert_eq!(b["received"].as_u64(), Some(SIZE_32K), "batch B short: {b}");
    let b_sha = b["sha256_sent"].as_str().expect("B sha256_sent").to_owned();
    assert!(
        wait_until(Duration::from_secs(10), || file_len(&rot_log) >= SIZE_32K),
        "batch B not logged"
    );
    rpc.rotate("rot").expect("rotate 2");
    assert!(
        wait_rotation(rpc, "rot", 1, Duration::from_secs(5)),
        "rotation did not reach 1"
    );
    let f001 = logdir.join("rot.log.001");
    assert_eq!(
        sha256_hex(&std::fs::read(&f001).expect("read rot.log.001")),
        b_sha,
        "rot.log.001 != batch B"
    );

    // Batch C stays in the live file. Each batch landed in exactly its own file with a
    // matching checksum (A->.000, B->.001, C->live), so rotation lost nothing and split
    // no chunk across a boundary.
    let c = echo_send(&console, "seeded:32KiB", 3);
    assert_eq!(c["pass"].as_bool(), Some(true), "batch C echo failed: {c}");
    assert_eq!(c["received"].as_u64(), Some(SIZE_32K), "batch C short: {c}");
    let c_sha = c["sha256_sent"].as_str().expect("C sha256_sent").to_owned();
    assert!(
        wait_until(Duration::from_secs(10), || file_len(&rot_log) >= SIZE_32K),
        "batch C not logged"
    );
    assert_eq!(
        sha256_hex(&std::fs::read(&rot_log).expect("read live rot.log")),
        c_sha,
        "live rot.log != batch C"
    );
}

// ---- Check 3: rotation counter recovered by directory scan on restart (§7.3) ----

/// A daemon child that is SIGKILLed and reaped on drop, so a panicking test never
/// leaks a daemon.
struct KillOnDrop(Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Spawn `serialnexusd` on `run`'s socket + state file (the persisted-config path
/// policy, §11/§15.9). Reusing the same paths across two spawns is how the restart is
/// exercised: the stale-socket dance reclaims the leftover socket and the persisted
/// state file is recovered at startup (§10).
fn spawn_daemon(run: &TempRun) -> Child {
    Command::new(bin("serialnexusd"))
        .arg("--socket")
        .arg(run.socket())
        .arg("--state-file")
        .arg(run.state_file())
        .env("XDG_RUNTIME_DIR", run.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn serialnexusd")
}

/// Wait until a daemon is actually listening on `sock` (a bound listener accepts the
/// connection; a leftover stale socket file refuses it). Bounded poll, no panic — this
/// is the restart-safe replacement for `test -S`, which would spuriously match the
/// stale socket file left by the hard kill.
fn wait_socket(sock: &Path) -> bool {
    wait_until(Duration::from_secs(10), || {
        UnixStream::connect(sock).is_ok()
    })
}

#[test]
fn rotation_counter_recovered_by_directory_scan_on_restart() {
    // Hand-managed daemon lifecycle: this test needs a hard kill + restart on the SAME
    // socket/state-file/log-directory, which `Daemon::start` (fresh temp dir each call)
    // cannot express. Needs no serial device, so it runs on every platform.
    let run = TempRun::new();
    let logdir = run.join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log directory");

    let mut d1 = KillOnDrop(spawn_daemon(&run));
    assert!(
        wait_socket(&run.socket()),
        "daemon 1 control socket never appeared"
    );
    let rpc = Rpc::new(run.socket());

    // A lone log node: the directory-scan recovery is independent of any producer.
    let cfg = format!(
        r#"
[[node]]
type = "log"
name = "rot"
directory = "{logdir}"
filename = "rot.log"
"#,
        logdir = logdir.display(),
    );
    rpc.load_toml(&cfg, false).expect("load lone-log config");
    assert!(
        rpc.wait_status("rot", "active", Duration::from_secs(5)),
        "rot not active: {:?}",
        rpc.node("rot")
    );

    // Two rotations produce rot.log.000 and rot.log.001 (numbers; higher is newer).
    rpc.rotate("rot").expect("rotate 1");
    assert!(
        wait_rotation(&rpc, "rot", 0, Duration::from_secs(5)),
        "rotation did not reach 0"
    );
    rpc.rotate("rot").expect("rotate 2");
    assert!(
        wait_rotation(&rpc, "rot", 1, Duration::from_secs(5)),
        "rotation did not reach 1"
    );
    let f000 = logdir.join("rot.log.000");
    let f001 = logdir.join("rot.log.001");
    assert!(
        f000.exists() && f001.exists(),
        "the two rotations did not produce rot.log.000 and rot.log.001"
    );

    // Hard kill (SIGKILL) skips the clean-shutdown socket unlink and never persists the
    // rotation counter — the next daemon must recover both from the environment (§7.3).
    d1.0.kill().expect("SIGKILL daemon 1");
    d1.0.wait().expect("reap daemon 1");

    // A fresh daemon reclaims the stale socket (§10) and recovers config from the
    // persisted state file; its log node rescans the directory itself (§7.3).
    let _d2 = KillOnDrop(spawn_daemon(&run));
    assert!(
        wait_socket(&run.socket()),
        "daemon 2 control socket never came back"
    );

    // Existing rotations are .000 and .001, so the recovered counter must read 1 — not
    // a restart at 000.
    assert!(
        wait_rotation(&rpc, "rot", 1, Duration::from_secs(10)),
        "rotation counter not recovered from directory scan (got {:?})",
        rpc.node("rot").map(|n| n["rotation"].clone())
    );

    // The next rotation must number .002, never a clobbering .000; the earlier
    // rotation must survive untouched (higher-is-newer, no cascade).
    let a_before = sha256_hex(&std::fs::read(&f000).expect("read rot.log.000 before"));
    rpc.rotate("rot").expect("post-restart rotate");
    let f002 = logdir.join("rot.log.002");
    assert!(
        wait_until(Duration::from_secs(5), || f002.exists()),
        "post-restart rotation did not produce rot.log.002"
    );
    let a_after = sha256_hex(&std::fs::read(&f000).expect("read rot.log.000 after"));
    assert_eq!(a_before, a_after, "rotation cascaded/clobbered rot.log.000");
}

// ---- Check 4: rotation is ordered against the queue (§7.3, review LOG-3) --------

/// The sim's deterministic SplitMix64 payload — reimplemented so this test owns the
/// ground truth for *where* each batch's bytes landed, not just their checksum
/// (identical to `nexus-sim`'s; `len` a multiple of 8).
fn seeded_bytes(seed: u64, len: usize) -> Vec<u8> {
    let mut s = seed;
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        s = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = s;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        out.extend_from_slice(&z.to_le_bytes());
    }
    out.truncate(len);
    out
}

/// Create a FIFO at `path` with `mkfifo(1)`. `false` when the tool is missing or
/// fails, which makes the caller **skip** — POSIX guarantees the utility, but the
/// harness never assumes an external tool exists (§5).
fn mkfifo(path: &Path) -> bool {
    Command::new("mkfifo")
        .arg(path)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Read everything the FIFO gives us until it stays silent for `quiet` (or the
/// deadline passes), appending to `out`. The read end is non-blocking, so this is a
/// bounded poll rather than a blocking read that could wedge the test: draining is
/// also what *unblocks* the daemon's writer, so the loop is the fixture's motor.
fn drain_fifo(reader: &mut std::fs::File, out: &mut Vec<u8>, quiet: Duration, deadline: Instant) {
    let mut buf = vec![0u8; 64 * 1024];
    let mut last_byte_at = Instant::now();
    while Instant::now() < deadline {
        match reader.read(&mut buf) {
            Ok(0) => {}
            Ok(n) => {
                out.extend_from_slice(&buf[..n]);
                last_byte_at = Instant::now();
                continue;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => panic!("read the log FIFO: {e}"),
        }
        if last_byte_at.elapsed() >= quiet {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_batch_accepted_after_rotate_never_lands_in_the_old_file() {
    // LOG-3. `rotate` used to be a side flag the writer honoured *between* write
    // batches, with the pending count sampled when a batch was drained — so bytes
    // that arrived while the writer was busy were written to the pre-rotation file
    // and only then was the file rotated. At HEAD the request is a `QueueItem::Rotate`
    // marker that takes its place in the byte stream.
    //
    // Distinguishing the two needs the writer to be **stuck in a write** when the
    // operator rotates, which a keeping-up disk never is. So the log's file is a
    // **FIFO** this test owns the read end of: after ~64 KiB (the pipe buffer) the
    // writer thread blocks in `write(2)` and the rest of the batch backs up in the
    // node's queue, which `state.queued_bytes` proves before the `rotate` is issued.
    // Draining the FIFO afterwards yields, byte for byte, exactly what the
    // pre-rotation file received.
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP a_batch_accepted_after_rotate_never_lands_in_the_old_file: \
             no serial device on this platform"
        );
        return;
    };

    const A_LEN: usize = 128 * 1024; // > the 64 KiB pipe buffer, so the writer blocks
    const B_LEN: usize = 16 * 1024;
    let batch_a = seeded_bytes(11, A_LEN);
    let batch_b = seeded_bytes(22, B_LEN);

    let d = Daemon::start();
    let rpc = d.rpc();
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log directory");
    let console = d.run().join("console");
    let live = logdir.join("rot.log");

    if !mkfifo(&live) {
        eprintln!(
            "SKIP a_batch_accepted_after_rotate_never_lands_in_the_old_file: mkfifo unavailable"
        );
        return;
    }
    // O_RDWR keeps the FIFO open from both ends (so a read never sees EOF and the
    // daemon's write-only open never blocks); O_NONBLOCK keeps the drain a poll.
    let mut fifo = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(&live)
        .expect("open the log FIFO read end");

    let cfg = format!(
        r#"
[[node]]
type = "pty"
name = "console"
path = "{console}"
[[node]]
type = "serial"
name = "usb0"
arbitration = "free-for-all"
device = "{dev}"
[[node]]
type = "log"
name = "rot"
directory = "{logdir}"
filename = "rot.log"
[[edge]]
a = "usb0"
b = "console"
[[edge]]
a = "usb0"
b = "rot"
"#,
        console = console.display(),
        dev = echo.device().display(),
        logdir = logdir.display(),
    );
    rpc.load_toml(&cfg, false).expect("load ordering config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20))
            && rpc.wait_status("rot", "active", Duration::from_secs(10)),
        "nodes not active: usb0={:?} rot={:?}",
        rpc.node("usb0"),
        rpc.node("rot")
    );
    assert!(
        wait_until(Duration::from_secs(5), || console.exists()),
        "console pty symlink never appeared"
    );

    // Batch A: 128 KiB through the echo device and back hostward into the log. The
    // writer takes the first ~64 KiB into the pipe and then blocks.
    let a = echo_send(&console, "seeded:128KiB", 11);
    assert_eq!(a["pass"].as_bool(), Some(true), "batch A echo failed: {a}");
    assert_eq!(
        a["sha256_sent"].as_str(),
        Some(sha256_hex(&batch_a).as_str()),
        "the sim's batch A is not the payload this test reconstructed: {a}"
    );

    // The precondition that makes this test discriminating, asserted rather than
    // assumed: the writer is stuck mid-batch with bytes still queued behind it.
    assert!(
        wait_until(Duration::from_secs(10), || {
            rpc.node("rot")
                .and_then(|n| n["queued_bytes"].as_u64())
                .unwrap_or(0)
                > 0
        }),
        "the log writer never backed up on the FIFO, so this run could not tell an \
         ordered rotation from an unordered one: {:?}",
        rpc.node("rot")
    );

    // The operator rotates *now*, while the writer is blocked…
    rpc.rotate("rot").expect("rotate");
    // …and only then does batch B enter the daemon at all: every one of its bytes is
    // accepted strictly after the rotation request, so §7.3 places all of them in the
    // new file.
    let b = echo_send(&console, "seeded:16KiB", 22);
    assert_eq!(b["pass"].as_bool(), Some(true), "batch B echo failed: {b}");
    assert_eq!(
        b["sha256_sent"].as_str(),
        Some(sha256_hex(&batch_b).as_str()),
        "the sim's batch B is not the payload this test reconstructed: {b}"
    );

    // Drain the FIFO: this both unblocks the writer and collects, byte for byte,
    // everything the pre-rotation file was given.
    let mut old = Vec::new();
    drain_fifo(
        &mut fifo,
        &mut old,
        Duration::from_millis(750),
        Instant::now() + Duration::from_secs(30),
    );
    // 1. The old file holds a **prefix of A and nothing else**. This is the LOG-3
    //    assertion: an unordered rotation appends B's bytes here (the reviewer's
    //    "bytes accepted after `rotate` can land in the pre-rotation file"). The
    //    split point itself is not pinned — bytes of A still in flight when the
    //    marker was queued legitimately belong to the new file — but it can never
    //    fall inside B.
    assert!(
        old.len() <= A_LEN,
        "the pre-rotation file received {} bytes, more than batch A's {A_LEN} — \
         bytes accepted after `rotate` landed in the old file (LOG-3)",
        old.len()
    );
    assert_eq!(
        sha256_hex(&old),
        sha256_hex(&batch_a[..old.len()]),
        "the pre-rotation file is not a prefix of batch A — the rotation was not \
         ordered against the queue (LOG-3)"
    );
    // 2. …and the new file — the FIFO renamed away, a fresh regular file in its
    //    place — is exactly the remainder of A followed by all of B, so the rotation
    //    lost nothing and reordered nothing (§7.3).
    let rotated = logdir.join("rot.log.000");
    assert!(
        wait_until(Duration::from_secs(10), || rotated.exists()
            && live.is_file()),
        "rotation did not rename the old file and reopen a fresh one"
    );
    let want_new = [&batch_a[old.len()..], &batch_b[..]].concat();
    assert!(
        wait_until(Duration::from_secs(10), || {
            std::fs::read(&live).map(|v| v.len()).unwrap_or(0) >= want_new.len()
        }),
        "the new file never received batch B (got {} bytes, want {})",
        std::fs::read(&live).map(|v| v.len()).unwrap_or(0),
        want_new.len()
    );
    let new = std::fs::read(&live).expect("read the post-rotation file");
    assert_eq!(
        sha256_hex(&new),
        sha256_hex(&want_new),
        "the post-rotation file is not A's remainder followed by batch B \
         (old={} new={} want={})",
        old.len(),
        new.len(),
        want_new.len()
    );
    // 3. Nothing was dropped along the way: the split is a split, not a loss.
    assert_eq!(
        rpc.node("rot").expect("rot node")["dropped_bytes"].as_u64(),
        Some(0),
        "rotation under a blocked writer dropped bytes: {:?}",
        rpc.node("rot")
    );
}
