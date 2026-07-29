//! **One daemon per control-socket path** (37-SEAM-3; design §10).
//!
//! §10 says "the standard stale-socket unlink dance runs at startup". The dance is three
//! steps — probe, unlink, bind — and two daemons starting from a common stale inode can
//! interleave them: both probe, both read it dead, and the loser's unlink lands *after*
//! the winner's bind. Neither reports an error. The winner keeps accepting on an inode
//! no client can reach; the loser owns the path. Both then load the same state file and
//! race for the same devices under `TIOCEXCL`, and the unreachable one usually wins the
//! opens, because nothing is talking to it — the operator sees one daemon that answers
//! with every port `faulted` and one daemon that answers nothing.
//!
//! **What this test can and cannot do.** The interleaving itself is not reproducible from
//! outside: it needs a scheduler to stop one daemon between two adjacent syscalls, and a
//! test that merely started two daemons would pass on the *old* code too, because a live
//! daemon's socket answers the probe and the second daemon refuses for that reason alone.
//! So the property asserted here is the fix rather than the symptom: the path is
//! **claimed** — an `flock` on `<socket>.lock` held for the daemon's whole life — so the
//! dance cannot begin while another daemon is inside it. The fixture removes a live
//! daemon's socket out from under it, which is exactly what the loser of the race does,
//! and then asks a second daemon to take the path. With the claim there is something
//! left to refuse it with; without one the path is simply free.
//!
//! Fail-first (AGENTS.md §9): proved against the unfixed tree by reverting the claim in
//! `daemon/src/lib.rs` in place — the second daemon binds, never exits, and the test
//! fails at `expect("the second daemon kept running…")`.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serial_nexus_itest::{TempRun, bin, daemon_answers, wait_until};

/// A daemon child killed and reaped on drop, so a panicking assertion never leaks one.
struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Spawn `serial-nexus-daemon` on `run`'s socket and state file, stderr piped so a
/// refusal's diagnostic is readable. Does not wait for readiness: the second daemon here
/// is expected never to become ready.
fn spawn(run: &TempRun) -> KillOnDrop {
    KillOnDrop(
        Command::new(bin("serial-nexus-daemon"))
            .arg("--socket")
            .arg(run.socket())
            .arg("--state-file")
            .arg(run.state_file())
            .env("XDG_RUNTIME_DIR", run.path())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn serial-nexus-daemon"),
    )
}

/// Wait for `child` to exit within `timeout`, returning its exit code and stderr.
/// `None` means it was still running — which, for a daemon that must refuse, is the
/// failure this file is about.
fn wait_exit(child: &mut KillOnDrop, timeout: Duration) -> Option<(Option<i32>, String)> {
    let mut status = None;
    wait_until(timeout, || {
        status = child.0.try_wait().expect("try_wait");
        status.is_some()
    });
    let status = status?;
    let mut stderr = String::new();
    if let Some(mut e) = child.0.stderr.take() {
        let _ = e.read_to_string(&mut stderr);
    }
    Some((status.code(), stderr))
}

/// **A second daemon refuses a claimed path even when the socket is gone from it.**
#[test]
fn a_claimed_socket_path_refuses_a_second_daemon_after_its_socket_is_unlinked() {
    let run = TempRun::new();
    let socket = run.socket();

    let mut first = spawn(&run);
    assert!(
        wait_until(Duration::from_secs(10), || socket.exists()
            && daemon_answers(&socket)),
        "the first daemon never answered on {}",
        socket.display()
    );

    // Exactly what the loser of the SEAM-3 race does to the winner: unlink a live
    // daemon's socket. The daemon is still running and still accepting — on an inode
    // nothing can reach any more.
    std::fs::remove_file(&socket).expect("unlink the live daemon's socket");
    assert!(!socket.exists(), "the socket path should now be free");

    // Nothing about the *path* now says it is taken: this is the state in which the
    // naive dance binds and two daemons end up owning one socket and one state file.
    let mut second = spawn(&run);
    let (code, stderr) = wait_exit(&mut second, Duration::from_secs(10)).expect(
        "the second daemon kept running: it took a path a live daemon still owns, so \
         both now serve one state file and contend for the same devices",
    );
    assert_ne!(
        code,
        Some(0),
        "the second daemon exited successfully on a claimed path; stderr={stderr:?}"
    );
    // An operator has to be able to tell this apart from a crash, so the diagnostic
    // names the path that is spoken for.
    assert!(
        stderr.contains(&socket.display().to_string()),
        "the refusal does not name the socket path it refused: {stderr:?}"
    );

    // …and it took nothing on its way out: the path is still free for whoever the first
    // daemon hands it to when it stops.
    assert!(
        !socket.exists(),
        "the refused daemon bound the path anyway at {}",
        socket.display()
    );

    // The first daemon is untouched by all of this — it is still the owner.
    assert_eq!(
        first.0.try_wait().expect("try_wait on the first daemon"),
        None,
        "the first daemon died while the second was refused"
    );
}
