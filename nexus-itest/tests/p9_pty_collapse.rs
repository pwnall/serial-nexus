//! The **collapsed PTY client session** guard (design §6 detach-release, §7.2
//! presence) — the regression test the fix at `b8d8ed8` shipped without (review
//! §2 item 1, T2).
//!
//! `b8d8ed8` changed the pty last-close trigger from the bare presence edge
//!
//! ```text
//! if was && !present_now                       // pre-fix
//! ```
//!
//! to
//!
//! ```text
//! if (was && !present_now) || (closed && saw_data)   // at HEAD
//! ```
//!
//! and touched exactly one file. The bug it closed is load-dependent: when the
//! reader task is starved, one poll of the master can observe a client's whole
//! attach → write → detach at once, so `present` never latches `true`, the `was`
//! edge never fires, and an **on-demand holder that streamed and detached keeps
//! the write lock forever** — the endpoint is dead to every other writer until the
//! node is removed. Nothing in the suite would have caught the fix's removal.
//!
//! Two properties, one per half of the fix's argument:
//!
//! 1. [`collapsed_client_sessions_still_release_the_write_lock`] — a session whose
//!    attach, write and close all happen inside **one** of the reader's idle poll
//!    windows must still detach-release. The test does not simulate the collapse:
//!    it *causes* it, by doing open→write→close in a few microseconds while the
//!    idle reader polls every `IDLE_POLL` (5 ms), so the daemon sees the session
//!    only at its EOF. Repeated, because the collapse is a race the test wins
//!    ~99 times in 100 — one unlucky iteration proves nothing, eight prove it.
//! 2. [`a_bare_hangup_leaves_the_daemon_cpu_bounded`] — the fix's *other* claim:
//!    the `saw_data` latch is what keeps the handler from re-firing, so a hangup
//!    with no client data must leave no busy loop behind. Measured as the daemon's
//!    own `utime + stime` over an idle window (Linux-only `/proc/<pid>/stat`;
//!    self-skips elsewhere). See the constant's comment for what planting the
//!    ungated arm actually did on this kernel.
//!
//! Neither needs a serial *device*: the `usb0` node's device is deliberately
//! absent, so it comes up `waiting` while its write lock, origins and targetward
//! receiver exist regardless — structural, exactly as `p4_waiting` establishes. So
//! both run on **every** platform (the CPU sampler self-skips off Linux).

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::Duration;

use nexus_itest::{Daemon, Rpc, TempRun, wait_until};
use serde_json::Value;

// The CPU sampler and its hand-managed daemon exist only where `/proc/<pid>/stat`
// does; what only they need is gated with them so the file stays warning-clean on
// the platforms where the second test is a skip.
#[cfg(target_os = "linux")]
use nexus_itest::bin;
#[cfg(target_os = "linux")]
use std::process::{Child, Command, Stdio};

/// How many collapsed sessions one run drives. Each is an independent trial of a
/// race the test wins with high probability (a ~50 µs client session against a
/// 5 ms idle poll), so the pre-fix code fails at least one with overwhelming
/// probability while the fixed code passes all of them deterministically.
const COLLAPSES: usize = 8;

/// The graph: one PTY writer into one device-absent serial host endpoint. The
/// serial node never opens anything (`waiting`), which is what keeps this test
/// device-free and cross-platform; its lock and the PTY's on-demand origin are
/// built by the wiring regardless (§6).
fn cfg(run: &TempRun) -> String {
    format!(
        r#"
[[node]]
type = "pty"
name = "console"
path = "{console}"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
[[edge]]
a = "usb0"
b = "console"
"#,
        console = run.join("console").display(),
        dev = run.join("absent-device").display(),
    )
}

/// The current holder of `usb0`'s write lock, if any (§6).
fn holder(rpc: &Rpc) -> Option<String> {
    rpc.node("usb0")?
        .pointer("/lock/holder")?
        .as_str()
        .map(str::to_owned)
}

/// Open the PTY slave the way a client does (never adopting it as this process's
/// controlling terminal).
fn attach(path: &Path) -> std::fs::File {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOCTTY)
        .open(path)
        .unwrap_or_else(|e| panic!("open pty slave {}: {e}", path.display()))
}

/// Open the PTY slave, write `data`, and close — as fast as the kernel allows,
/// with no intervening syscall the daemon could interleave with. That speed *is*
/// the test fixture: the whole session lands inside one 5 ms idle poll window of
/// the reader task, which is the shape that used to lose the release.
fn collapsed_session(path: &Path, data: &[u8]) {
    let mut slave = attach(path);
    slave.write_all(data).expect("write to the pty slave");
    slave.flush().expect("flush the pty slave");
    drop(slave); // last close → the daemon observes attach+data+hangup at once
}

// ============================================================================
// 1 — a collapsed session still detach-releases (the `b8d8ed8` latch).
// ============================================================================
#[test]
fn collapsed_client_sessions_still_release_the_write_lock() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let console = d.run().join("console");
    rpc.load_toml(&cfg(d.run()), false).expect("load graph");
    assert!(
        rpc.wait_status("console", "active", Duration::from_secs(10)),
        "console pty not active: {:?}",
        rpc.node("console")
    );
    assert!(
        wait_until(Duration::from_secs(5), || console.exists()),
        "console pty symlink never appeared"
    );

    for i in 0..COLLAPSES {
        // The PTY takes the lock as an on-demand origin, the way an operator's
        // `lock console` (or a `send`) does before typing.
        let r = rpc
            .lock("console", false, false, None)
            .unwrap_or_else(|e| panic!("lock console #{i}: [{}] {}", e.code, e.message));
        assert_eq!(
            r.get("acquired").and_then(Value::as_bool),
            Some(true),
            "lock console #{i} did not acquire: {r}"
        );
        assert_eq!(
            holder(rpc).as_deref(),
            Some("console"),
            "console is not the holder before session #{i}"
        );

        // Attach, type, detach — all inside one idle poll window.
        collapsed_session(&console, b"collapsed\n");

        // The detach must release the on-demand holder's lock (§6). Pre-`b8d8ed8`
        // this hung on "console" forever, because the rising edge was never seen.
        assert!(
            wait_until(Duration::from_secs(10), || holder(rpc).is_none()),
            "collapsed session #{i}: the write lock was NOT released \
             (holder still {:?}) — the last-close latch missed a session whose \
             attach, data and hangup arrived in one poll (§6, b8d8ed8)",
            holder(rpc)
        );
        // The client is gone as far as state is concerned, too (§7.2).
        assert_eq!(
            rpc.node("console")
                .and_then(|n| n.get("client_present").and_then(Value::as_bool)),
            Some(false),
            "client_present stuck true after collapsed session #{i}"
        );
    }
}

// ============================================================================
// 2 — a bare hangup does not spin the runtime (the `saw_data` latch).
// ============================================================================

/// A daemon child SIGKILLed and reaped on drop, so a panicking test never leaks
/// one. Hand-managed because this test needs the daemon's **pid** to sample its
/// CPU, which `Daemon` does not expose.
#[cfg(target_os = "linux")]
struct KillOnDrop(Child);
#[cfg(target_os = "linux")]
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// `utime + stime` of `pid` in clock ticks, read from `/proc/<pid>/stat`. The
/// two fields are 14 and 15 (1-based) *after* the parenthesised comm field, which
/// may itself contain spaces — so the split starts past the last `)`.
#[cfg(target_os = "linux")]
fn cpu_ticks(pid: u32) -> u64 {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .unwrap_or_else(|e| panic!("read /proc/{pid}/stat: {e}"));
    let tail = &stat[stat.rfind(')').expect("comm field is parenthesised") + 1..];
    let fields: Vec<&str> = tail.split_whitespace().collect();
    // `tail` starts at field 3 (state), so utime (14) and stime (15) are at
    // indices 11 and 12.
    let utime: u64 = fields[11].parse().expect("utime");
    let stime: u64 = fields[12].parse().expect("stime");
    utime + stime
}

#[test]
#[cfg(target_os = "linux")]
fn a_bare_hangup_leaves_the_daemon_cpu_bounded() {
    // The measurement window, and the ceiling on what the daemon may burn inside
    // it. Ticks are USER_HZ (100/s on Linux), so a whole core for the window is
    // 200 ticks; measured here, a post-hangup daemon spends **1**. The ceiling is
    // therefore 20× the observed cost and still an order of magnitude below a
    // handler that re-runs `tcsetattr` on every poll.
    //
    // Recorded from trying to break it: planting the ungated `|| closed` arm (the
    // shape the `saw_data` latch exists to prevent) did *not* raise this number on
    // Linux 7.0 — once the last slave closes, the master stops reporting `POLLIN`,
    // so `closed` is set only in the poll that drains the final bytes and the
    // handler has no second chance to fire. That is review nit PTY-4 seen from the
    // other side: the anti-spin argument in `pty.rs`'s comment names a mechanism
    // this kernel does not exhibit. The guard is kept because it costs one sample
    // and pins the property the review asked for — a hangup leaves no busy loop
    // behind — not because it reproduces a historical failure.
    const WINDOW: Duration = Duration::from_secs(2);
    const MAX_TICKS: u64 = 20;

    let run = TempRun::new();
    let console = run.join("console");
    let child = Command::new(bin("serialnexusd"))
        .arg("--socket")
        .arg(run.socket())
        .arg("--state-file")
        .arg(run.state_file())
        .env("XDG_RUNTIME_DIR", run.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn serialnexusd");
    let pid = child.id();
    let d = KillOnDrop(child);
    assert!(
        wait_until(Duration::from_secs(10), || run.socket().exists()),
        "daemon control socket never appeared"
    );
    let rpc = Rpc::new(run.socket());
    rpc.load_toml(&cfg(&run), false).expect("load graph");
    assert!(
        rpc.wait_status("console", "active", Duration::from_secs(10)),
        "console pty not active: {:?}",
        rpc.node("console")
    );
    assert!(
        wait_until(Duration::from_secs(5), || console.exists()),
        "console pty symlink never appeared"
    );

    // The lock matters: a non-holder is deliberately *not read from* (§6), so
    // only a holder's master reaches the EOF/EIO the close arm keys on.
    rpc.lock("console", false, false, None)
        .expect("lock console");

    // A bare hangup: a client attaches, is *observed* (so the presence edge fires
    // the handler exactly once, and its `tcsetattr` leaves a control packet
    // readable on the master), and leaves without ever writing a byte. With the
    // `saw_data` gate the handler cannot re-arm on that control packet; ungated,
    // every following poll re-runs it and the reader never backs off again.
    let client = attach(&console);
    assert!(
        wait_until(Duration::from_secs(5), || {
            rpc.node("console")
                .and_then(|n| n.get("client_present").and_then(Value::as_bool))
                == Some(true)
        }),
        "the daemon never observed the attaching client"
    );
    drop(client);
    assert!(
        wait_until(Duration::from_secs(5), || {
            rpc.node("console")
                .and_then(|n| n.get("client_present").and_then(Value::as_bool))
                == Some(false)
        }),
        "client_present did not settle false after the bare hangup"
    );

    let before = cpu_ticks(pid);
    // A *measurement window*, not a readiness wait: there is no condition to poll
    // for here — the claim under test is about how little CPU passes while
    // nothing happens, so the window has to elapse.
    std::thread::sleep(WINDOW);
    let spent = cpu_ticks(pid).saturating_sub(before);

    assert!(
        spent <= MAX_TICKS,
        "the daemon burned {spent} clock ticks in {WINDOW:?} after a bare hangup \
         (ceiling {MAX_TICKS}) — the last-close handler is re-firing every poll \
         (§7.2, the `saw_data` latch of b8d8ed8)"
    );
    // The daemon is still alive and answering after the window (a spin that also
    // wedged the control plane would be a different failure).
    assert_eq!(
        rpc.node_status("console"),
        "active",
        "the console did not survive the idle window"
    );
    drop(d);
}

#[test]
#[cfg(not(target_os = "linux"))]
fn a_bare_hangup_leaves_the_daemon_cpu_bounded() {
    eprintln!(
        "SKIP a_bare_hangup_leaves_the_daemon_cpu_bounded: per-process CPU sampling \
         needs /proc/<pid>/stat (Linux)"
    );
}
