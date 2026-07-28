//! The **sim doubles' idle CPU** guard (design §15.36 Decision; plan §3 rule 2) —
//! the assertion the `NO_SLAVE_PAUSE` fix shipped without (review 32, `ITEST-5`).
//!
//! `fe1c52c` fixed two apparatus loops that free-ran on a full core once their pty
//! slaves closed: `nexus-sim`'s `pty --echo` and its `nullmodem` bridge. On a pty
//! master `POLLHUP` is *level*-triggered and set for as long as no slave is open, and
//! requesting `POLLIN` alone does not mask it — so a loop that (correctly) tolerates
//! the hangup and does not pause wakes immediately, forever. Measured at the time:
//! **74.4% of a core**, three seconds after one open+close of each slave.
//!
//! The *functional* half of that fix has a guard
//! (`p7_p5::a_probe_that_opens_and_closes_the_port_leaves_the_loopback_alive` — the
//! double survives a close). The CPU half had none, in a tree whose design says it
//! must: §15.36's Decision paragraph — "sim doubles modeling stateless hardware
//! tolerate hangup forever, pause rather than spin, **and get their idle CPU
//! asserted**" — and plan §3 rule 2, verbatim. Deleting the `thread::sleep` would
//! therefore have been silent: no test measures a `nexus-sim` process's CPU, and the
//! only symptom is renewed flakiness in unrelated tests, which is precisely the
//! misdiagnosis loop the flake session paid for.
//!
//! Three things are established per double, and none of them is decoration:
//!
//! 0. **the hangup this guard is about is actually being asserted by this kernel** —
//!    `nexus-doctor`'s P2 `hup_after_close`, read as a precondition. Without it the
//!    measurement is silently vacuous: a kernel that stopped level-triggering `POLLHUP`
//!    on a slaveless master leaves both doubles polling a quiet fd, which scores zero
//!    ticks *with or without* the `NO_SLAVE_PAUSE` the test exists to protect. The
//!    review-32 audit caught that hole; the premise is now checked rather than assumed,
//!    and a kernel that has lost it gets a loud skip instead of a green tick;
//! 1. the double burns almost no CPU across an idle window after its slave has been
//!    opened and closed once (the priming step — a *never*-opened master does not
//!    HUP, so a test that skips it measures nothing);
//! 2. it still relays a probe afterwards. A CPU budget alone cannot distinguish
//!    *paused* from *dead*, and a double that exited on the hangup — the very
//!    regression `p7_p5` exists for — would score zero ticks and pass.
//!
//! **What it still does not establish**, stated rather than implied: it measures the
//! *shipped* `nexus-sim` binary's idle cost, so it catches a deleted or weakened
//! `NO_SLAVE_PAUSE`, but it cannot attribute a passing figure to that specific
//! mechanism — any other change that made the loop cheap would also pass. And the
//! ceiling is a budget, not a proof of sleeping: a loop that woke ten times a second
//! doing nothing measurable would clear it.
//!
//! Linux-only: per-process CPU sampling needs `/proc/<pid>/stat`. Self-skips
//! elsewhere, exactly as `p9_pty_collapse`'s sampler does (whose `cpu_ticks` this
//! deliberately mirrors, field indices and all).

#[cfg(target_os = "linux")]
use std::io::{Read, Write};
#[cfg(target_os = "linux")]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::process::{Child, Command, Stdio};
#[cfg(target_os = "linux")]
use std::sync::mpsc;
#[cfg(target_os = "linux")]
use std::time::Duration;

#[cfg(target_os = "linux")]
use nexus_itest::{TempRun, bin, wait_until};

/// The measurement window, and the ceiling on what a *paused* double may burn
/// inside it.
///
/// Ticks are USER_HZ (100/s on Linux), so a whole core for this window is 300 and
/// the busy spin being guarded against — 74.4% of a core, the figure recorded in
/// `nexus-sim`'s `NO_SLAVE_PAUSE` doc — is ~223. Both doubles measured **1** tick
/// here after one open+close of each slave. The ceiling of 30 (10% of a core) is
/// therefore 30x the observed cost and 7x below the regression it must catch.
/// Deliberately not tighter: the number has to survive a loaded shared runner, and
/// the failure it exists for is two orders of magnitude away, not two ticks.
#[cfg(target_os = "linux")]
const WINDOW: Duration = Duration::from_secs(3);
#[cfg(target_os = "linux")]
const MAX_TICKS: u64 = 30;

/// How long the doubles are told to live. Comfortably past the window plus setup, so
/// the sampler never measures a process that idled itself out mid-window (a dead
/// process reads as beautifully quiet).
#[cfg(target_os = "linux")]
const DOUBLE_TIMEOUT_MS: &str = "60000";

/// A `nexus-sim` child SIGKILLed and reaped on drop, so a panicking test never leaks
/// one. Hand-managed rather than `Sim` because this test needs the double's **pid**
/// to sample its CPU, which `Sim` does not expose.
#[cfg(target_os = "linux")]
struct KillOnDrop(Child);
#[cfg(target_os = "linux")]
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// `utime + stime` of `pid` in clock ticks, read from `/proc/<pid>/stat`. The two
/// fields are 14 and 15 (1-based) *after* the parenthesised comm field, which may
/// itself contain spaces — so the split starts past the last `)`.
#[cfg(target_os = "linux")]
fn cpu_ticks(pid: u32) -> u64 {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .unwrap_or_else(|e| panic!("read /proc/{pid}/stat: {e}"));
    let tail = &stat[stat.rfind(')').expect("comm field is parenthesised") + 1..];
    let fields: Vec<&str> = tail.split_whitespace().collect();
    let utime: u64 = fields[11].parse().expect("utime");
    let stime: u64 = fields[12].parse().expect("stime");
    utime + stime
}

/// Spawn `nexus-sim` with `args`, returning the reaped-on-drop child and its pid.
#[cfg(target_os = "linux")]
fn spawn_double(args: &[&str]) -> (KillOnDrop, u32) {
    let child = Command::new(bin("nexus-sim"))
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn nexus-sim");
    let pid = child.id();
    (KillOnDrop(child), pid)
}

/// Open a pts the way a client does (never adopting it as this process's
/// controlling terminal).
#[cfg(target_os = "linux")]
fn attach(path: &Path) -> std::fs::File {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOCTTY)
        .open(path)
        .unwrap_or_else(|e| panic!("open pts {}: {e}", path.display()))
}

/// Does this kernel still assert `POLLHUP` on a pty master after its last slave
/// closes? `None` when the question could not be put.
///
/// Read from the shipped capability checker rather than re-implemented here: P2's
/// `hup_after_close` observation is exactly this question, it is already the tree's
/// authority on it (AGENTS §7, `expectations/linux.jq`), and `nexus-doctor` owns the
/// `unsafe` a pty pair needs — this crate is `#![forbid(unsafe_code)]` and could not ask
/// the kernel directly without growing a seam for it.
///
/// The reader was proved by pointing it at P2's neighbouring `hup_when_never_opened`,
/// which this kernel reports `false`: both tests then skipped with the message below,
/// rather than measuring anything.
#[cfg(target_os = "linux")]
fn pollhup_after_last_close() -> Option<bool> {
    let out = Command::new(bin("nexus-doctor"))
        .arg("--json")
        .output()
        .ok()?;
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    report
        .get("probes")?
        .as_array()?
        .iter()
        .find(|p| p.get("id").and_then(|v| v.as_str()) == Some("P2"))?
        .get("observations")?
        .as_array()?
        .iter()
        .find(|o| o.get("key").and_then(|v| v.as_str()) == Some("hup_after_close"))?
        .get("value")?
        .as_bool()
}

/// The precondition gate: `true` to go on, `false` after printing the skip.
#[cfg(target_os = "linux")]
fn premise_holds(test: &str) -> bool {
    match pollhup_after_last_close() {
        Some(true) => true,
        Some(false) => {
            eprintln!(
                "SKIP {test}: this kernel no longer reports POLLHUP on a pty master \
                 after its last slave closes (nexus-doctor P2 hup_after_close = false), \
                 so a peerless double has nothing to spin on and this measurement would \
                 pass whatever the loop does. The busy-spin class is gone with the \
                 premise — see AGENTS §7 and expectations/linux.jq, which gate on P2."
            );
            false
        }
        None => {
            eprintln!(
                "SKIP {test}: could not read nexus-doctor P2, so the premise this \
                 measurement rests on is unverified (run `cargo build --workspace`)"
            );
            false
        }
    }
}

/// Open and immediately close each pts once — the *priming* step. A pty master that
/// has never had a slave does not report `POLLHUP` at all (AGENTS.md §7 / doctor P2),
/// so without this the double is polling an entirely quiet fd and the measurement
/// would pass no matter what the loop does.
#[cfg(target_os = "linux")]
fn prime(links: &[&PathBuf]) {
    for link in links {
        assert!(
            wait_until(Duration::from_secs(5), || link.exists()),
            "sim link never appeared at {}",
            link.display()
        );
        drop(attach(link));
    }
}

/// The double's `utime + stime` over [`WINDOW`], in clock ticks.
#[cfg(target_os = "linux")]
fn idle_ticks(pid: u32) -> u64 {
    let before = cpu_ticks(pid);
    // A *measurement window*, not a readiness wait: the claim under test is about
    // how little CPU passes while nothing happens, so the window has to elapse.
    std::thread::sleep(WINDOW);
    cpu_ticks(pid).saturating_sub(before)
}

/// Write a probe on `writer` and wait for it to come back on `reader` — proof the
/// double is **paused**, not dead. The read runs on its own thread with a bounded
/// wait, so a broken double fails this in five seconds instead of hanging the suite.
#[cfg(target_os = "linux")]
fn still_relays(writer: &Path, reader: &Path) -> bool {
    const PROBE: &[u8] = b"ping";
    // The reading end is opened *first*, so the relayed bytes land in a pts whose
    // slave is already open rather than being written to a master with no reader.
    let mut r = attach(reader);
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; PROBE.len()];
        let got = r.read_exact(&mut buf).map(|()| buf);
        let _ = tx.send(got);
    });
    let mut w = attach(writer);
    w.write_all(PROBE).expect("write the probe");
    w.flush().expect("flush the probe");
    matches!(rx.recv_timeout(Duration::from_secs(5)), Ok(Ok(b)) if b == PROBE)
}

// ============================================================================
// 1 — `pty --echo`: the software TX<->RX jumper.
// ============================================================================
#[test]
#[cfg(target_os = "linux")]
fn the_echo_double_pauses_rather_than_spinning_while_peerless() {
    if !premise_holds("the_echo_double_pauses_rather_than_spinning_while_peerless") {
        return;
    }
    let run = TempRun::new();
    let link = run.join("echo-dev");
    let (_double, pid) = spawn_double(&[
        "pty",
        "--echo",
        "--link",
        &link.to_string_lossy(),
        "--timeout-ms",
        DOUBLE_TIMEOUT_MS,
    ]);
    prime(&[&link]);

    let spent = idle_ticks(pid);
    assert!(
        spent <= MAX_TICKS,
        "the `pty --echo` double burned {spent} clock ticks in {WINDOW:?} while \
         peerless (ceiling {MAX_TICKS}, a whole core is 300) — the level-triggered \
         POLLHUP is free-running its loop: restore `NO_SLAVE_PAUSE` in the Ok(0) / \
         EIO / POLLHUP arms of `nexus-sim`'s `pty_echo` (§15.36, plan §3 rule 2)"
    );
    assert!(
        still_relays(&link, &link),
        "the echo double stopped reflecting after a close — a jumper is stateless \
         and must tolerate the hangup forever (p7_p5's defect), and a double that \
         exits instead of pausing would pass the CPU budget above"
    );
}

// ============================================================================
// 2 — `nullmodem`: the crossed pair with no hardware.
// ============================================================================
#[test]
#[cfg(target_os = "linux")]
fn the_nullmodem_double_pauses_rather_than_spinning_while_peerless() {
    if !premise_holds("the_nullmodem_double_pauses_rather_than_spinning_while_peerless") {
        return;
    }
    let run = TempRun::new();
    let (a, b) = (run.join("nm-a"), run.join("nm-b"));
    let (_double, pid) = spawn_double(&[
        "nullmodem",
        "--link-a",
        &a.to_string_lossy(),
        "--link-b",
        &b.to_string_lossy(),
        "--timeout-ms",
        DOUBLE_TIMEOUT_MS,
    ]);
    // Both ends are primed: the bridge polls two masters, and one HUPing end is
    // enough to free-run the shared loop.
    prime(&[&a, &b]);

    let spent = idle_ticks(pid);
    assert!(
        spent <= MAX_TICKS,
        "the `nullmodem` double burned {spent} clock ticks in {WINDOW:?} while \
         peerless (ceiling {MAX_TICKS}, a whole core is 300) — the level-triggered \
         POLLHUP is free-running its loop: restore the `NO_SLAVE_PAUSE` on the \
         woke-but-forwarded-nothing arm of `run_nullmodem_inner` (§15.36, plan §3 \
         rule 2)"
    );
    assert!(
        still_relays(&a, &b),
        "the null modem stopped forwarding a -> b after a close — the bridge must \
         stay usable for the next opener, and a double that exits instead of \
         pausing would pass the CPU budget above"
    );
}

#[test]
#[cfg(not(target_os = "linux"))]
fn the_echo_double_pauses_rather_than_spinning_while_peerless() {
    eprintln!(
        "SKIP the_echo_double_pauses_rather_than_spinning_while_peerless: \
         per-process CPU sampling needs /proc/<pid>/stat (Linux)"
    );
}

#[test]
#[cfg(not(target_os = "linux"))]
fn the_nullmodem_double_pauses_rather_than_spinning_while_peerless() {
    eprintln!(
        "SKIP the_nullmodem_double_pauses_rather_than_spinning_while_peerless: \
         per-process CPU sampling needs /proc/<pid>/stat (Linux)"
    );
}
