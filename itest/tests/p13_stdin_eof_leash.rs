#![forbid(unsafe_code)]

//! **The stdin-EOF orphan leash, once, against all three binaries that carry one**
//! (design §15.43; plan §18 item 79).
//!
//! The watch was implemented three times: `daemon/src/lib.rs` and `web/src/main.rs`
//! held it byte-for-byte — same loop, same arms, same thread name, same log line —
//! and the test double held a third variant. It is now
//! `serial_nexus_rpc::leash`, one implementation, and this file is the half of the
//! repair that keeps it one.
//!
//! # The instrument, and why it is not a proxy
//!
//! Every test here hands the process under test a **file** as stdin and reads the
//! offset of its own duplicate of that file descriptor afterwards. `dup` and `fork`
//! share the open file *description*, so the offset the child moves is the offset the
//! parent reads — the same POSIX sharing that makes a shell's `>>` work. That makes
//! "did this process read its stdin, and how much of it" directly observable from
//! outside the process, on both kernels, with no instrumentation inside the product.
//!
//! Measured on this box before anything was written, driving a stand-in child:
//! read-to-EOF → 524288, one-byte read → 1, no read at all → 0. Three separable
//! readings, which is what makes the assertions below discriminate rather than merely
//! pass.
//!
//! Each binary gets both arms, because either alone proves nothing:
//!
//! * **leashed** — the process drains its stdin to EOF and *then* stops. The offset is
//!   what says it drained: a watch that read the close out of the first byte would stop
//!   the process just the same, and only the offset separates them.
//! * **unleashed** — the same binary, the same stdin, no flag: still running, and the
//!   offset still **0**. §15.43 clause 2 is the whole reason the flag is opt-in, and
//!   a leash nobody armed must not consume a stdin that may carry data.
//!
//! # Fail-first, measured against the shared implementation
//!
//! Three plants in `rpc/src/leash.rs`, each a single arm, each reddening a guard on
//! **every** side — which is the hoist's own validation (plan §18 item 79, the shape
//! item 59(d) used) and not merely a nice property: it is what says the three binaries
//! are on one implementation rather than three that agree today.
//!
//! * `Ok(_) => break` — the loop stops on the first byte. All three leashed tests fail
//!   on the offset (8192, std's `StdinLock` buffer, against 524288) while still
//!   exiting, so the "it stopped" half alone would have gone green.
//! * `Ok(0) => continue` — the close is never recognised. All three leashed tests fail
//!   on the deadline.
//! * `stdin_eof_signal`'s unarmed arm calling `stdin_eof_watch()` — all three
//!   *unleashed* tests fail on the offset.
//!
//! The last one is the reason the unleashed arm reads the offset rather than only
//! checking that the process is still up: an unarmed leash that quietly drains stdin
//! changes no lifetime at all, and every existing guard stays green.

use std::fs::File;
use std::io::Seek;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serial_nexus_itest::{KillOnDrop, TempRun, bin, wait_daemon_ready, wait_until};

/// How much a leashed process must read before it sees the close.
///
/// Large enough that no single buffered `read` can account for it: `std`'s `StdinLock`
/// is a `BufReader`, so the *first* call pulls a whole buffer off the fd whatever the
/// loop does next. Measured on this box — a loop planted with `Ok(_) => break` leaves
/// the offset at **8192**, the shipped loop at **524288** — so the two are separated by
/// a factor of 64 rather than by a byte, and the assertion does not rest on `std`'s
/// buffer being any particular size.
const FEED_BYTES: u64 = 512 * 1024;

/// How long a leashed process gets to notice the close. Generous on purpose: this box
/// runs other work, and the quantity under test is "does it stop at all", not how fast.
const STOP_DEADLINE: Duration = Duration::from_secs(20);

/// A file of [`FEED_BYTES`] noise bytes, to be handed to a process as its stdin.
///
/// Deliberately **not** zeros: a byte that reads as `0` in the buffer must not be
/// confusable with `Ok(0)`, which is the close. Nothing in the loop can confuse them,
/// and that is the point — a feed of zeros would make the test unable to say so.
fn feed(run: &TempRun, name: &str) -> PathBuf {
    let path = run.join(name);
    let bytes: Vec<u8> = (0..FEED_BYTES).map(|i| (i % 251) as u8 + 1).collect();
    std::fs::write(&path, &bytes).expect("write the stdin feed file");
    path
}

/// Open `path` twice over one open file description: the handle to hand the child as
/// its stdin, and our own duplicate of it, whose offset *is* the child's.
fn shared_stdin(path: &Path) -> (Stdio, File) {
    let theirs = File::open(path).expect("open the stdin feed file");
    let ours = theirs.try_clone().expect("dup the stdin feed file");
    (Stdio::from(theirs), ours)
}

/// How far into its stdin the process has read, in bytes.
fn consumed(ours: &mut File) -> u64 {
    ours.stream_position().expect("read the shared file offset")
}

/// The complaint an unleashed process earns by touching a stdin it was never given a
/// reason to read.
fn unleashed_read_anyway(who: &str, got: u64) -> String {
    format!(
        "an unleashed {who} consumed {got} of {FEED_BYTES} bytes of its stdin. \
         §15.43's leash is opt-in, and the process that did not opt in must leave its \
         stdin alone: a mode whose stdin carries data — a codec child's envelope pipe \
         — would have that data eaten, and nothing about the process's *lifetime* \
         would change, so no existing guard would see it."
    )
}

/// The complaint a leashed process earns by stopping before it reached the close.
fn leashed_short_read(who: &str, got: u64) -> String {
    format!(
        "a leashed {who} stopped with the shared stdin offset at {got}, not \
         {FEED_BYTES}: it did not read its stdin to EOF, so it took something other \
         than the close as the signal to stop. Bytes on stdin are noise, not a \
         protocol (§15.43) — a supervisor that logs into the pipe by accident must not \
         end the process it is supervising."
    )
}

fn spawned(mut cmd: Command) -> KillOnDrop {
    KillOnDrop::new(cmd.spawn().expect("spawn the process under test"))
}

fn daemon_cmd(run: &TempRun, socket: &Path, leashed: bool, stdin: Stdio) -> Command {
    let mut cmd = Command::new(bin("serial-nexus-daemon"));
    cmd.arg("--socket")
        .arg(socket)
        .arg("--state-file")
        .arg(run.state_file());
    if leashed {
        cmd.arg("--exit-on-stdin-eof");
    }
    cmd.env("XDG_RUNTIME_DIR", run.path())
        .stdin(stdin)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd
}

/// **A leashed daemon reads its stdin to the end and then stops.**
#[test]
fn a_leashed_daemon_drains_its_stdin_to_eof_and_then_stops() {
    let run = TempRun::new();
    let socket = run.socket();
    let (stdin, mut ours) = shared_stdin(&feed(&run, "daemon.feed"));
    let mut child = spawned(daemon_cmd(&run, &socket, true, stdin));

    let status = child.wait_exit(STOP_DEADLINE);
    assert!(
        status.is_some(),
        "a leashed serial-nexus-daemon was still running {STOP_DEADLINE:?} after its \
         stdin reached EOF. It holds a control socket nothing will dial again and, if \
         its graph opened any, real devices under TIOCEXCL (§15.43)."
    );
    assert!(
        status.expect("the daemon exited").success(),
        "a leashed serial-nexus-daemon stopped on the close, but not through its \
         normal shutdown path: §15.43 promises no more residue than a SIGTERM, and a \
         non-zero exit is the shape that leaves the control socket behind"
    );
    let got = consumed(&mut ours);
    assert_eq!(got, FEED_BYTES, "{}", leashed_short_read("daemon", got));
}

/// **…and an unleashed daemon does not touch that same stdin.** The control, and the
/// half no lifetime assertion can make.
#[test]
fn an_unleashed_daemon_never_reads_a_byte_of_its_stdin() {
    let run = TempRun::new();
    let socket = run.socket();
    let (stdin, mut ours) = shared_stdin(&feed(&run, "daemon.feed"));
    let mut child = spawned(daemon_cmd(&run, &socket, false, stdin));

    assert!(
        wait_daemon_ready(&socket),
        "an unleashed serial-nexus-daemon never became ready, so the offset below \
         would be measuring a daemon that never got as far as arming anything"
    );
    let got = consumed(&mut ours);
    assert_eq!(got, 0, "{}", unleashed_read_anyway("daemon", got));
    assert!(
        child.try_wait().is_none(),
        "an unleashed serial-nexus-daemon exited on a stdin it was never asked to \
         watch (§15.43 clause 2)"
    );
}

fn web_cmd(run: &TempRun, leashed: bool, stdin: Stdio) -> Command {
    let mut cmd = Command::new(bin("serial-nexus-web"));
    cmd.args([
        "--bind",
        "127.0.0.1:0",
        "--token",
        "leash-offset-token",
        "--socket",
        &run.socket().to_string_lossy(),
    ]);
    if leashed {
        cmd.arg("--exit-on-stdin-eof");
    }
    cmd.env("XDG_RUNTIME_DIR", run.path())
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    cmd
}

/// Collect a child's stdout lines in the background, so a wait for the console's
/// bound-URL line cannot deadlock on a full pipe.
fn tail_stdout(child: &mut Child) -> std::sync::Arc<std::sync::Mutex<Vec<String>>> {
    let stdout = child.stdout.take().expect("piped stdout");
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
    lines
}

/// **A leashed web console reads its stdin to the end and then stops.**
///
/// No daemon: the console binds and prints its URL before it dials anything, and the
/// subject here is its *lifetime* and its stdin, not its bridge.
#[test]
fn a_leashed_web_console_drains_its_stdin_to_eof_and_then_stops() {
    let run = TempRun::new();
    let (stdin, mut ours) = shared_stdin(&feed(&run, "web.feed"));
    let mut child = web_cmd(&run, true, stdin)
        .spawn()
        .expect("spawn serial-nexus-web");
    let _lines = tail_stdout(&mut child);
    let mut child = KillOnDrop::new(child);

    let status = child.wait_exit(STOP_DEADLINE);
    assert!(
        status.is_some(),
        "a leashed serial-nexus-web was still running {STOP_DEADLINE:?} after its \
         stdin reached EOF; it keeps its port bound and its taps open on the daemon \
         for as long as the box stays up (§15.43)"
    );
    assert!(
        status.expect("the console exited").success(),
        "a leashed serial-nexus-web stopped on the close, but not cleanly"
    );
    let got = consumed(&mut ours);
    assert_eq!(
        got,
        FEED_BYTES,
        "{}",
        leashed_short_read("web console", got)
    );
}

/// **…and an unleashed web console does not touch that same stdin.**
#[test]
fn an_unleashed_web_console_never_reads_a_byte_of_its_stdin() {
    let run = TempRun::new();
    let (stdin, mut ours) = shared_stdin(&feed(&run, "web.feed"));
    let mut child = web_cmd(&run, false, stdin)
        .spawn()
        .expect("spawn serial-nexus-web");
    let lines = tail_stdout(&mut child);
    let mut child = KillOnDrop::new(child);

    // "Up" is asserted as *bound and printing its URL*, not as "the process exists":
    // a process can survive its own failure, and the offset below has to be read off a
    // console that actually got started.
    let bound = wait_until(Duration::from_secs(15), || {
        lines.lock().unwrap().iter().any(|l| l.contains("http://"))
    });
    assert!(
        bound,
        "an unleashed serial-nexus-web never printed its bound URL; saw {:?}",
        lines.lock().unwrap()
    );
    let got = consumed(&mut ours);
    assert_eq!(got, 0, "{}", unleashed_read_anyway("web console", got));
    assert!(
        child.try_wait().is_none(),
        "an unleashed serial-nexus-web exited on a stdin it was never asked to watch \
         (§15.43 clause 2)"
    );
}

fn sim_cmd(link: &Path, leashed: bool, stdin: Stdio) -> Command {
    let mut cmd = Command::new(bin("serial-nexus-sim"));
    // The flag must precede the subcommand: it is a top-level option (§15.43's arm of
    // the double), not a `pty` one.
    if leashed {
        cmd.arg("--exit-on-stdin-eof");
    }
    cmd.args(["pty", "--echo", "--timeout-ms", "600000", "--link"])
        .arg(link)
        .stdin(stdin)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd
}

/// **A leashed double reads its stdin to the end and then stops.**
///
/// The double is the variant that differs for a reason — one reader, two waiter shapes
/// (`--exit-on-stdin-eof` and `client --hold-stdin-eof`) — and it is on the same read
/// loop as the other two, which is what this asserts.
///
/// What is deliberately *not* asserted here: that the published symlink is gone. The
/// leash fires from another thread and the double publishes its link from `main`, so
/// with a stdin already at EOF the two race; the unpublish contract is exercised where
/// the leash arrives after a running double, not here.
#[test]
fn a_leashed_double_drains_its_stdin_to_eof_and_then_stops() {
    let run = TempRun::new();
    let (stdin, mut ours) = shared_stdin(&feed(&run, "sim.feed"));
    let mut child = spawned(sim_cmd(&run.join("sim.link"), true, stdin));

    let status = child.wait_exit(STOP_DEADLINE);
    assert!(
        status.is_some(),
        "a leashed serial-nexus-sim was still running {STOP_DEADLINE:?} after its \
         stdin reached EOF; it holds a pty master and its --timeout-ms is ten minutes \
         away (§15.43)"
    );
    let got = consumed(&mut ours);
    assert_eq!(got, FEED_BYTES, "{}", leashed_short_read("double", got));
}

/// **…and an unleashed double does not touch that same stdin.**
///
/// This is the arm with a live consequence in this tree: the double's `transcript`
/// mode stands where a codec child stands and its stdin *is* the daemon's envelope
/// pipe (§7.6), which is why `--exit-on-stdin-eof` is refused there. A watch that read
/// stdin without being armed would eat that conversation.
#[test]
fn an_unleashed_double_never_reads_a_byte_of_its_stdin() {
    let run = TempRun::new();
    let link = run.join("sim.link");
    let (stdin, mut ours) = shared_stdin(&feed(&run, "sim.feed"));
    let mut child = spawned(sim_cmd(&link, false, stdin));

    assert!(
        wait_until(Duration::from_secs(15), || link.exists()),
        "an unleashed serial-nexus-sim never published its pty link, so the offset \
         below would be measuring a double that never started"
    );
    let got = consumed(&mut ours);
    assert_eq!(got, 0, "{}", unleashed_read_anyway("double", got));
    assert!(
        child.try_wait().is_none(),
        "an unleashed serial-nexus-sim exited on a stdin it was never asked to watch \
         (§15.43 clause 2)"
    );
}

// --- the source-level half: exactly one implementation ---------------------

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");

/// The one file allowed to hold the leash's own identifiers.
const WATCH_HOME: &str = "rpc/src/leash.rs";

/// This file, which must spell what it bans and is therefore excluded by name.
/// Asserted to exist, so a rename widens the allowance loudly rather than silently.
const THIS_GATE: &str = "itest/tests/p13_stdin_eof_leash.rs";

/// The two things plan §18 item 79 named as copied verbatim across the binaries: the
/// reader thread's name, which is what an operator reading `ps -L` has to recognise,
/// and the line a leashed process logs on its way out.
const MARKERS: [(&str, &str); 2] = [
    ("the reader thread's name", "\"stdin-eof-watch\""),
    (
        "the stopping line",
        "stdin reached EOF under --exit-on-stdin-eof",
    ),
];

/// Join Rust's backslash-newline string continuations, so a marker split across two
/// source lines still matches.
///
/// **Without this the matcher is trivially defeated by `rustfmt`.** Both markers are
/// long enough to be wrapped, and every copy this tree has ever held wrapped them —
/// `daemon/src/lib.rs` broke the stopping line after "supervisor" and
/// `web/src/main.rs` after "holding", in the same sentence.
fn join_continuations(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(i) = rest.find("\\\n") {
        out.push_str(&rest[..i]);
        rest = rest[i + 2..].trim_start_matches([' ', '\t']);
    }
    out.push_str(rest);
    out
}

/// Whether `src` holds `marker`, continuations joined first.
fn holds(src: &str, marker: &str) -> bool {
    join_continuations(src).contains(marker)
}

/// Every `.rs` file under `root`, relative to it. `unreadable` collects directories
/// `read_dir` refused for a reason other than "gone": a directory this walk never
/// reached is a directory whose copy of the watch reads as compliance.
fn rs_files(root: &Path, unreadable: &mut Vec<String>) -> Vec<String> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>, unreadable: &mut Vec<String>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    unreadable.push(format!("{}: {e}", dir.display()));
                }
                return;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy().into_owned();
            if path.is_dir() {
                if matches!(name.as_str(), "target" | ".git" | "node_modules") {
                    continue;
                }
                walk(&path, root, out, unreadable);
            } else if name.ends_with(".rs")
                && let Ok(rel) = path.strip_prefix(root)
            {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out, unreadable);
    out.sort();
    out
}

/// **The leash is spelled in exactly one file** (plan §18 item 79).
///
/// A fourth copy that behaves identically is invisible to every runtime assertion in
/// this suite — a byte-identical copy is byte-identical — which is why the claim moves
/// to the source, the shape plan §18 item 59(c) established.
///
/// **What this gate does not cover, stated rather than implied** (AGENTS §3's
/// weaker-than-its-comment register): a copy that renames its thread *and* rewords its
/// log line escapes it, because the markers are the two identifiers item 79 named, not
/// a parse of the loop. The markers were chosen because they are the operator-visible
/// ones — the name in `ps -L`, the line in the log — so a copy that changes both has
/// already broken the thing the sharing exists to protect and cannot be silent.
#[test]
fn the_stdin_eof_leash_is_spelled_in_exactly_one_file() {
    let root = Path::new(REPO);
    assert!(
        root.join(WATCH_HOME).is_file(),
        "{WATCH_HOME} does not exist: the leash moved and this gate is now guarding \
         nothing (plan §18 item 79)"
    );
    assert!(
        root.join(THIS_GATE).is_file(),
        "{THIS_GATE} does not exist, so this gate's self-exclusion now names a file \
         that is not here — the allowance has widened to nothing, or this file was \
         renamed without moving the constant"
    );

    // --- 1. the matcher, in every spelling it claims to cover -----------------
    for (what, marker) in MARKERS {
        let split_at = marker.len() / 2;
        let wrapped = format!(
            "{}\\\n            {}",
            &marker[..split_at],
            &marker[split_at..]
        );
        assert!(
            holds(&format!("let x = {marker};"), marker),
            "the matcher misses {what} written on one line"
        );
        assert!(
            holds(&format!("let x = {wrapped};"), marker),
            "the matcher misses {what} wrapped across two lines — which is how every \
             copy this tree has held was written"
        );
    }
    // …and the near-misses it must not trip on, all three of them real lines from
    // this tree: the double's *other* thread, its own end-of-stream note, and prose.
    for benign in [
        "        .name(\"stdin-eof-leash\".to_owned())",
        "            \" (stdin reached EOF before the end marker)\"",
        "/// Watch stdin for EOF on a detached thread (the stdin EOF watch thread).",
    ] {
        for (what, marker) in MARKERS {
            assert!(
                !holds(benign, marker),
                "the matcher reads {what} into a line that does not carry it: {benign}"
            );
        }
    }

    // --- 2. the walker, against a planted tree --------------------------------
    let planted = TempRun::new();
    let deep = planted.path().join("a").join("b");
    std::fs::create_dir_all(&deep).expect("plant a nested directory");
    std::fs::write(deep.join("copy.rs"), "fn main() {}").expect("plant a nested file");
    std::fs::create_dir_all(planted.path().join("target")).expect("plant a target dir");
    std::fs::write(
        planted.path().join("target").join("skipped.rs"),
        "fn x() {}",
    )
    .expect("plant a build-output file");
    let mut unreadable = Vec::new();
    let found = rs_files(planted.path(), &mut unreadable);
    assert!(
        found.contains(&"a/b/copy.rs".to_string()),
        "the walker did not reach a file two directories down; it found {found:?}"
    );
    assert!(
        !found.iter().any(|f| f.starts_with("target/")),
        "the walker descended into build output; it found {found:?}"
    );

    // --- 3. the tree itself ---------------------------------------------------
    let mut unreadable = Vec::new();
    let files = rs_files(root, &mut unreadable);
    assert!(
        unreadable.is_empty(),
        "directories this gate could not read, so its verdict covers less than it \
         claims: {unreadable:?}"
    );
    assert!(
        files.len() > 50,
        "the walker found only {} .rs files in the workspace, which is not this tree: \
         a gate that scans nothing passes for the same reason it would if it were \
         green",
        files.len()
    );
    for (what, marker) in MARKERS {
        let mut carriers: Vec<&String> = Vec::new();
        for rel in &files {
            if rel == THIS_GATE {
                continue;
            }
            let Ok(src) = std::fs::read_to_string(root.join(rel)) else {
                continue;
            };
            if holds(&src, marker) {
                carriers.push(rel);
            }
        }
        assert_eq!(
            carriers,
            vec![&WATCH_HOME.to_string()],
            "{what} is spelled in {} files, not one. §15.43's watch is \
             `serial_nexus_rpc::leash` and nothing else: three copies of it — two \
             byte-for-byte — is what plan §18 item 79 exists to have removed, and a \
             fourth would be invisible to every runtime assertion in this suite, \
             because a copy that agrees is a copy that agrees.",
            carriers.len()
        );
    }
}
