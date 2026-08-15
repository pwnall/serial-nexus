#![forbid(unsafe_code)]

//! **The web console's lifetime leash** (design §15.43; plan §18 item 65(d)).
//!
//! Daemon ✓, sim ✓, web ✗ — that was the state of §15.43's leash when item 65 was
//! filed, with a live `serial-nexus-web` orphan on the development box to prove the
//! gap (notes §3.91). The console binds a TCP port and holds tap subscriptions on the
//! daemon, so one that outlives its supervisor is not merely a stray process: the next
//! console asking for the same fixed port fails to bind, for a reason nowhere in that
//! run's output.
//!
//! Two tests, because the flag has two halves and only one of them is the interesting
//! one to a reader:
//!
//! * the console **stops** when the pipe closes, asserted through the harness's own
//!   `WebServer` — so what is proven is the thing every web test in this suite relies
//!   on, not a hand-rolled spawn that resembles it;
//! * the console **keeps running** when it is not leashed and its stdin is at EOF from
//!   the first instant, which is §15.43 clause 2's whole reason for the default being
//!   off: under a service manager, or with `< /dev/null`, an always-on leash would stop
//!   the console at startup.
//!
//! **Fail-first, measured** (both against the tree as item 65 found it):
//! removing `--exit-on-stdin-eof` from `WebServer::spawn` leaves the first test's
//! console running through the full 15 s deadline; running the same console *with* the
//! flag and a null stdin exits it before it ever binds, which is the second test's
//! subject.

use std::process::{Command, Stdio};
use std::time::Duration;

use serial_nexus_itest::{TempRun, WebServer, wait_until};

/// **The console the harness spawns stops when the harness stops holding its pipe.**
///
/// The stimulus is the one the kernel delivers when this test process is SIGKILLed,
/// aborted, or killed as a process group: the write end of the console's stdin goes
/// away. Nothing kills the child here — [`WebServer::release_leash`] only drops the fd
/// — so an exit is the console acting on §15.43 and nothing else.
#[test]
fn the_web_console_stops_when_its_supervisor_lets_go_of_the_pipe() {
    let run = TempRun::new();
    let socket = run.socket();
    // No daemon: the console binds and prints its URL before it dials anything, and
    // this test's subject is its *lifetime*, not its bridge. That also keeps it off
    // every daemon-shaped precondition.
    let mut web = WebServer::start("127.0.0.1:0", "leash-token", &socket, run.path(), &[]);
    let pid = web.pid();

    // **Alive at the moment the pipe closes, or its later absence means nothing.**
    // `WebServer::start` waits for the bound URL, which proves the console *started*
    // and not that it is still here; a console that printed its URL and then exited
    // on its own would satisfy the assertion below without a leash existing at all.
    // Same shape as `p5_exec_orphans`'s liveness half, one binary over.
    assert!(
        !web.exited(),
        "serial-nexus-web (pid {pid}) exited on its own between printing its URL and \
         the leash being released, so the exit below is not attributable to the pipe"
    );

    let status = web.release_leash(Duration::from_secs(15));
    assert!(
        status.is_some(),
        "serial-nexus-web (pid {pid}) was still running 15 s after its supervisor closed \
         the write end of its stdin pipe. §15.43's leash is the only thing that stops a \
         console whose supervisor died without unwinding, and a console that survives \
         one keeps its port bound and its taps open on the daemon for as long as the box \
         stays up."
    );
}

/// **…and an unleashed console does not stop on a stdin that was never held** —
/// §15.43 clause 2, the reason the default is off.
///
/// This is the arm that keeps the first test honest. A console that simply exited early
/// — on a null stdin, on a missing daemon, on anything — would satisfy the first test
/// without implementing a leash at all. Here the same binary is given the same null
/// stdin *without* the flag and must still be serving.
///
/// "Still serving" is asserted as *bound and printing its URL*, not as "the process
/// exists": a process can survive its own failure, and the property the console
/// promises is that it is up. The wait is bounded on the URL rather than on a sleep.
#[test]
fn an_unleashed_web_console_ignores_a_stdin_that_is_already_at_eof() {
    let run = TempRun::new();
    let socket = run.socket();
    let mut child = Command::new(serial_nexus_itest::bin("serial-nexus-web"))
        .args([
            "--bind",
            "127.0.0.1:0",
            "--token",
            "unleashed-token",
            "--socket",
            &socket.to_string_lossy(),
        ])
        .env("XDG_RUNTIME_DIR", run.path())
        // No `--exit-on-stdin-eof`, and a stdin at EOF from the first instant: exactly
        // what a service manager hands a daemon it starts.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn serial-nexus-web");
    let stdout = child.stdout.take().expect("piped serial-nexus-web stdout");
    let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let sink = lines.clone();
    std::thread::spawn(move || {
        for line in std::io::BufRead::lines(std::io::BufReader::new(stdout)) {
            match line {
                Ok(l) => sink.lock().unwrap().push(l),
                Err(_) => break,
            }
        }
    });
    let mut web = serial_nexus_itest::KillOnDrop::new(child);

    let bound = wait_until(Duration::from_secs(10), || {
        lines.lock().unwrap().iter().any(|l| l.contains("http://"))
    });
    assert!(
        bound,
        "an unleashed serial-nexus-web never printed its bound URL; saw {:?}",
        lines.lock().unwrap()
    );
    assert!(
        web.try_wait().is_none(),
        "an unleashed serial-nexus-web exited on a stdin that was at EOF from the first \
         instant. §15.43 clause 2 is the whole reason the flag is opt-in: under a \
         service manager stdin is always at EOF, and a leash nobody armed must not be \
         armed by accident."
    );
}
