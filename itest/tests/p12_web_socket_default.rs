#![forbid(unsafe_code)]

//! **The web console's default control socket is the one §10 implementation's answer.**
//!
//! §10's socket path is a three-arm policy — root gets `/run/<name>.sock`, an
//! unprivileged process with a usable `XDG_RUNTIME_DIR` gets that directory, and
//! everyone else gets `/tmp/<name>-<uid>.sock` — and the design says it has exactly one
//! implementation, `serial_nexus_rpc::default_socket_path`. `serial-nexus-web` carried a
//! second one: **two** arms, no `geteuid`, reading the variable first. An unprivileged
//! console with `XDG_RUNTIME_DIR` unset or exported empty therefore resolved the *root*
//! arm `/run/serial-nexus-daemon.sock` while the daemon beside it bound
//! `/tmp/serial-nexus-daemon-<uid>.sock`, and reported that daemon unreachable at a path
//! nothing on the machine will ever create — under a `--socket` help line promising
//! "defaults match the daemon, §10". That is every unprivileged macOS session (no
//! `XDG_RUNTIME_DIR`, no `/run`) and any stripped service environment that exports the
//! variable empty.
//!
//! # Why this reads two processes instead of calling two functions
//!
//! The policy's discriminators are `geteuid()` and one environment variable, so the arm
//! a test observes is whichever one *its own process* is in. Mutating either in-process
//! is `unsafe` in Rust 2024 — forbidden everywhere but `serial-nexus-sys` (§16.3) — and
//! racy under the parallel runner besides, which is why the rpc crate splits its policy
//! into a pure function and unit-tests all three arms behind that seam. The seam is
//! private, and the property here is not "web's copy is correct" (there is no longer a
//! copy) but "the shipped `serial-nexus-web` and the shipped §10 implementation answer
//! the same thing in the same environment". Both halves are therefore read from real
//! processes, one environment at a time:
//!
//! * the **§10 side** from `serial-nexus-doctor --json`, whose environment block prints
//!   `default_socket_path`'s answer together with the `SocketOrigin` that produced it
//!   (notes §3.72). That row is computed rather than described, so the oracle cannot
//!   drift from the policy it is supposed to be checking against;
//! * the **web side** from the line `serial-nexus-web` logs before it binds anything,
//!   which names the exact `PathBuf` handed to `ServerConfig` — the socket the bridge
//!   dials, not a re-derivation of it.
//!
//! Reading the web side from a *failed* upgrade's 503 body was the alternative and is
//! rejected: that body only names the path when nothing is listening there, and two of
//! the three cases resolve to the shared per-uid `/tmp` path, where a daemon someone
//! left running would silently turn the guard into a 101 and no observation at all.
//!
//! # What is asserted, and what is not
//!
//! The paths must be equal in each environment; that is the finding. The uid arm is not
//! ours to choose, so the guard additionally requires the case set to have *discriminated*
//! — unprivileged, the three cases must land on two different arms of the policy, named
//! with `SocketOrigin`'s own vocabulary rather than with a second copy of the rule. As
//! root all three collapse onto `/run`, which is stated and allowed rather than skipped:
//! the equality is still meaningful there (the deleted copy diverged from it in the
//! `XDG_RUNTIME_DIR`-set case), it is simply a weaker fixture.
//!
//! Fail-first (plan §3 rule 4): with the two-arm copy restored in `web/src/rpc.rs`, the
//! `unset` and `empty` cases fail naming both paths — `/run/serial-nexus-daemon.sock`
//! against `/tmp/serial-nexus-daemon-1000.sock`. The `set` case passes either way, which
//! is precisely why it cannot be the only case.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serial_nexus_itest::{TempRun, bin};
use serial_nexus_rpc::SocketOrigin;

/// The doctor's computed row, by name. Matched exactly: a renamed row must fail loudly
/// here rather than leave the guard scanning for something that no longer exists.
const SOCKET_ROW: &str = "daemon socket (default)";

/// The prefix `serial-nexus-web` logs its resolved socket under.
const WEB_MARKER: &str = "daemon control socket: ";

/// Irrelevant to this test — nothing ever connects — but pinned so the server does not
/// spend entropy on a token no one reads.
const TOKEN: &str = "0123456789abcdef0123456789abcdef";

/// Point a child at `xdg`, where `None` means the variable is **removed** from its
/// environment and `Some("")` means it is exported empty. The policy distinguishes
/// those from a set one and from each other's spelling, so the fixture must be able to
/// produce all three rather than approximate one with another.
fn with_xdg(cmd: &mut Command, xdg: Option<&str>) {
    match xdg {
        Some(dir) => cmd.env("XDG_RUNTIME_DIR", dir),
        None => cmd.env_remove("XDG_RUNTIME_DIR"),
    };
}

/// Start `serial-nexus-doctor --json` under `xdg`.
///
/// Spawned rather than run to completion because the doctor runs its whole probe
/// battery (~4 s on the box of record) and the three cases are independent: they race
/// each other instead of the suite's clock.
fn spawn_doctor(xdg: Option<&str>) -> Child {
    let mut cmd = Command::new(bin("serial-nexus-doctor"));
    cmd.arg("--json")
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    with_xdg(&mut cmd, xdg);
    cmd.spawn().expect("spawn serial-nexus-doctor")
}

/// The `(path, origin)` pair the doctor computed: §10's answer for that child's
/// environment, and the arm that produced it.
///
/// The exit status is deliberately not asserted. The doctor grades the *machine* — an
/// absent adapter or a degraded kernel moves its verdict — and none of that has any
/// bearing on a path it computes from a uid and a variable.
fn doctor_socket(child: Child) -> (PathBuf, String) {
    let out = child
        .wait_with_output()
        .expect("serial-nexus-doctor runs to completion");
    let report: serde_json::Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("serial-nexus-doctor --json did not emit JSON: {e}"));
    let row = report["environment"]
        .as_array()
        .expect("the report has an environment block")
        .iter()
        .find(|c| c["name"] == SOCKET_ROW)
        .unwrap_or_else(|| {
            panic!("no {SOCKET_ROW:?} row in the doctor's environment block: {report}")
        });
    let value = row["value"].as_str().expect("the row's value is a string");
    // `"<path> — <origin>"`, the shape `probes.rs` prints. No arm's description
    // contains the separator, so one split is unambiguous.
    let (path, origin) = value
        .split_once(" — ")
        .unwrap_or_else(|| panic!("the computed socket row is not `<path> — <origin>`: {value:?}"));
    (PathBuf::from(path), origin.to_owned())
}

/// The socket `serial-nexus-web` resolves under `xdg`, read from its startup line.
///
/// No `--socket`: the default is the whole subject. `--bind 127.0.0.1:0` so the test
/// never competes for a port, and `RUST_LOG=info` because the line is a tracing
/// diagnostic and the developer's own filter must not decide whether this guard can see
/// it.
fn web_socket(xdg: Option<&str>) -> PathBuf {
    let mut cmd = Command::new(bin("serial-nexus-web"));
    cmd.args(["--bind", "127.0.0.1:0", "--token", TOKEN])
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    with_xdg(&mut cmd, xdg);
    let mut child = cmd.spawn().expect("spawn serial-nexus-web");

    // Drained on a thread: the server is long-lived and its stdout must not be left to
    // fill a pipe buffer while this thread blocks on a line that has already arrived.
    let stdout = child.stdout.take().expect("piped serial-nexus-web stdout");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(i) = line.find(WEB_MARKER) {
                let _ = tx.send(line[i + WEB_MARKER.len()..].trim().to_owned());
                return;
            }
        }
    });
    let found = rx.recv_timeout(Duration::from_secs(20));
    let _ = child.kill();
    let _ = child.wait();
    PathBuf::from(found.unwrap_or_else(|_| {
        panic!(
            "serial-nexus-web never named the control socket it resolved (looked for \
             {WEB_MARKER:?} on stdout, with XDG_RUNTIME_DIR {})",
            match xdg {
                Some("") => "exported empty".to_owned(),
                Some(dir) => format!("set to {dir}"),
                None => "unset".to_owned(),
            }
        )
    }))
}

#[test]
fn the_web_console_defaults_to_the_socket_the_one_policy_names() {
    let run = TempRun::new();
    let set = run.path().to_string_lossy().into_owned();
    // The three environments §10 discriminates on. An exported-but-empty value is a
    // separate case from an absent one on purpose: it is the shape a stripped service
    // environment produces, and the arm it must *not* take is the one that joins onto it.
    let cases: [(&str, Option<&str>); 3] = [
        ("XDG_RUNTIME_DIR set", Some(set.as_str())),
        ("XDG_RUNTIME_DIR unset", None),
        ("XDG_RUNTIME_DIR exported empty", Some("")),
    ];

    let doctors: Vec<Child> = cases.iter().map(|(_, xdg)| spawn_doctor(*xdg)).collect();
    let web: Vec<PathBuf> = cases.iter().map(|(_, xdg)| web_socket(*xdg)).collect();
    let policy: Vec<(PathBuf, String)> = doctors.into_iter().map(doctor_socket).collect();

    for (i, (label, _)) in cases.iter().enumerate() {
        let (expected, origin) = &policy[i];
        assert_eq!(
            &web[i],
            expected,
            "with {label}, the web console defaults to {} while the one §10 \
             implementation names {} ({origin}). A console that dials a socket no daemon \
             on this machine binds reports every console as unreachable, and its --socket \
             help line promises the daemon's defaults.",
            web[i].display(),
            expected.display()
        );
    }

    // Equal answers prove nothing if every case asked the same question, so the fixture
    // states which arms it reached — in the policy's own words, never re-derived here.
    let origins: Vec<&str> = policy.iter().map(|(_, o)| o.as_str()).collect();
    if origins[0] == SocketOrigin::XdgRuntimeDir.describe() {
        for i in [1, 2] {
            assert_eq!(
                origins[i],
                SocketOrigin::TmpPerUid.describe(),
                "{} reached {:?}, not the per-uid /tmp arm: with the XDG case on the \
                 XDG arm this process is unprivileged, so the two cases without a usable \
                 XDG_RUNTIME_DIR are exactly the arm the duplicate implementation got \
                 wrong — if they no longer reach it, this guard has stopped covering the \
                 defect it exists for",
                cases[i].0,
                origins[i]
            );
        }
    } else {
        // Root: one arm answers all three, `/run`. Weaker, and said so rather than
        // skipped — the equality above still discriminates here, because the deleted
        // copy would have handed the XDG-set case its variable instead of `/run`.
        for (i, (label, _)) in cases.iter().enumerate() {
            assert_eq!(
                origins[i],
                SocketOrigin::Root.describe(),
                "{label} reported {:?}, which is neither the unprivileged shape (the XDG \
                 case would then be on the XDG arm) nor the root one — the policy has \
                 grown an arm this guard does not know about",
                origins[i]
            );
        }
    }
}
