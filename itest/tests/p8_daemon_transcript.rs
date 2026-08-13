#![forbid(unsafe_code)]

//! **Golden transcripts of the daemon boundary** (plan §18 item 36).
//!
//! The golden vectors (`serial-nexus-sim envelope`, `exec-conformance`) pin single
//! *frames*: given this event, these bytes. Nothing gave a codec author the
//! **conversation** — what actually arrives on their child's stdin when an operator
//! writes to a channel, when a chunk is too large for one frame, when the device
//! speaks — so an author's mental model of the boundary came from prose and from
//! reading `daemon/src/nodes/exec.rs`.
//!
//! This file produces that conversation as a committed, replayable byte transcript,
//! and the load-bearing clause of the item is *how*: the transcripts are **generated
//! by this test against a live daemon**, never typed beside the code. A hand-written
//! example is exactly the artifact that drifts — it is right on the day it is
//! written and silently wrong afterwards, with nothing to notice. Here the daemon
//! writes its own half every run, and the run fails if the committed file disagrees.
//!
//! **Three phases per transcript.**
//!
//! 1. *Record.* A daemon drives `serial-nexus-sim transcript --record` as its exec
//!    codec child. The child emits a scripted half (built here from real [`Event`]s
//!    through the real [`encode`], so even that half is not typed hex) and writes
//!    down every byte the daemon sends it, one record per decoded frame.
//! 2. *Compare.* The recording is compared with the committed file, byte for byte.
//!    `SNX_REGENERATE_TRANSCRIPTS=1` rewrites the committed file instead — which is
//!    how a reviewer tells a legitimate regeneration from a silent one: a
//!    regeneration is a *diff in the commit*, on a file nothing else touches, next
//!    to the change that moved the boundary. There is no path by which a transcript
//!    changes without showing up in `git diff`.
//! 3. *Replay.* A second daemon drives `serial-nexus-sim transcript --verdict`
//!    against the committed file. It emits the same scripted half and compares the
//!    daemon→child stream byte for byte, failing at the first byte that differs. A
//!    third daemon replays a copy with **one byte flipped**, and the verdict must
//!    name that byte's offset — the item's own validation, run as a permanent guard
//!    rather than a one-off proof (§9: a guard asserts the property, and this one's
//!    fail-first is built into it).
//!
//! **Why nothing here can flake.** A transcript is a function of bytes and protocol,
//! never of scheduling:
//!
//! * The two directions are independent pipes polled concurrently (§15.22), so the
//!   file records two ordered streams and asserts no interleaving between them.
//! * The daemon→child stream is one merge queue fed by one forwarder per channel, so
//!   frames from *different* channels have no defined order. Every targetward write
//!   in the host conversation therefore goes to **one** channel, where the order is
//!   a single FIFO; the device conversation has one device and one sentinel, and
//!   sequences the second against the first by reading the recorder's `.partial`
//!   file, never by sleeping.
//! * The conversation ends on a **sentinel frame**, not on a quiet period. A quiet
//!   period is a timer, and a timer in a golden is a flake with a delay fuse.
//! * Nothing timestamped, counted or scheduled is written down. Only bytes.
//!
//! **What the transcripts do not establish** is written into their own headers
//! (AGENTS §8), so a reader who has the artifact and not this file still gets it.
//! The one thing bytes cannot show — that those bytes landed in the loss counters
//! §5 promises — is asserted here beside the comparison.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::Value;
use serial_nexus_codec_api::{Event, MAX_FRAME_SIZE, encode};
use serial_nexus_itest::{Daemon, Rpc, TempRun, bin, wait_until};

/// The end-of-conversation sentinel. Matches `serial-nexus-sim transcript`'s
/// `--end-marker` default; spelled once here and passed nowhere, so the two can only
/// disagree by someone changing the sim's default, which reddens every test below.
const END_MARKER: &str = "__snx_transcript_end__";

/// Set to regenerate the committed transcripts instead of comparing against them.
///
/// Deliberately **not** spelled `SNX_BLESS_*`: `scripts/bless` already means
/// "build + install + setcap" in this tree, and one word for two unrelated
/// operations is how an operator runs the wrong one.
const REGENERATE_ENV: &str = "SNX_REGENERATE_TRANSCRIPTS";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("serial-nexus-itest has a parent (the workspace root)")
        .to_path_buf()
}

/// A committed transcript's path. `tests/transcripts/` at the workspace root — the
/// same `tests/` that holds `ext-codec/`, which is likewise data for the harness
/// rather than a cargo test target (the root manifest is a virtual workspace).
fn golden_path(name: &str) -> PathBuf {
    workspace_root()
        .join("tests")
        .join("transcripts")
        .join(name)
}

// --- transcript file plumbing ----------------------------------------------

/// The byte length of the frame one record line carries (`H:` hex plus any `R:`
/// run). The harness needs this to sequence a replay on `matched_bytes` and to
/// place the planted mutation; it is the only piece of the format this file parses.
///
/// **Linux-gated because its only caller is.** The device-side transcript needs a
/// real hostward producer, which inside one daemon is a `serial` node over a pty —
/// the pty-as-serial doctrine, Linux-only. An ungated helper whose one caller is
/// `#[cfg(target_os = "linux")]` is dead code off Linux, which `cargo build` never
/// compiles and the Linux lints never see: the exact class plan §18 item 54's Darwin
/// clippy cross-check exists for, and the first live instance it caught.
#[cfg(target_os = "linux")]
fn record_len(line: &str) -> usize {
    let mut n = 0;
    for field in line.split_whitespace().skip(1) {
        if let Some(hex) = field.strip_prefix("H:") {
            n += hex.len() / 2;
        } else if let Some(run) = field.strip_prefix("R:") {
            let (_, count) = run
                .split_once('*')
                .expect("an R: field is <hexbyte>*<count>");
            n += count.parse::<usize>().expect("an R: count is a number");
        }
    }
    n
}

/// How many daemon→child records a (possibly partial) transcript holds so far.
///
/// Linux-gated for the reason [`record_len`] states.
#[cfg(target_os = "linux")]
fn daemon_records(path: &Path) -> usize {
    std::fs::read_to_string(path)
        .map(|t| t.lines().filter(|l| l.starts_with("> ")).count())
        .unwrap_or(0)
}

/// Render the child's scripted half as `<` records, through the *real* envelope
/// encoder. The recorder canonicalises whatever spelling it is handed, so this
/// writes plain `H:` hex and the committed file still comes out canonical.
fn write_script(path: &Path, events: &[Event]) {
    let mut text = String::from(
        "# The codec child's scripted half, written by \
         itest/tests/p8_daemon_transcript.rs\n# through serial_nexus_codec_api::encode. \
         The recorder re-renders it canonically.\n",
    );
    for ev in events {
        let mut frame = Vec::new();
        encode(ev, &mut frame).expect("encode a scripted child frame");
        let hex: String = frame.iter().map(|b| format!("{b:02x}")).collect();
        text.push_str(&format!("< H:{hex}\n"));
    }
    std::fs::write(path, text).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

/// A TOML array literal for an argv, with the two characters a TOML basic string
/// cares about escaped. Paths here are harness-built, but a quoted path assembled
/// by hand is the kind of thing that works until someone's checkout has a space or
/// a backslash in it (design §15.26's "quoted exec paths").
fn toml_argv(argv: &[String]) -> String {
    let items: Vec<String> = argv
        .iter()
        .map(|a| format!("\"{}\"", a.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect();
    format!("[{}]", items.join(", "))
}

/// The `serial-nexus-sim transcript` argv for a recording run.
fn recorder_argv(script: &Path, out: &Path, notes: &[&str]) -> Vec<String> {
    let mut argv = vec![
        bin("serial-nexus-sim").display().to_string(),
        "transcript".to_owned(),
        "--transcript".to_owned(),
        script.display().to_string(),
        "--record".to_owned(),
        out.display().to_string(),
    ];
    for note in notes {
        argv.push("--note".to_owned());
        argv.push((*note).to_owned());
    }
    argv
}

/// The `serial-nexus-sim transcript` argv for a replay run.
fn replayer_argv(transcript: &Path, verdict: &Path) -> Vec<String> {
    vec![
        bin("serial-nexus-sim").display().to_string(),
        "transcript".to_owned(),
        "--transcript".to_owned(),
        transcript.display().to_string(),
        "--verdict".to_owned(),
        verdict.display().to_string(),
    ]
}

/// Wait for the recorder's final file, then read it. On timeout, the partial
/// recording is printed: a recorder that stopped mid-conversation has already
/// written down everything it saw, and that is the diagnosis.
fn read_recording(path: &Path) -> String {
    if !wait_until(Duration::from_secs(60), || path.exists()) {
        let mut partial = path.as_os_str().to_owned();
        partial.push(".partial");
        let partial = std::fs::read_to_string(PathBuf::from(partial))
            .unwrap_or_else(|e| format!("<no partial recording: {e}>"));
        panic!(
            "the recorder never finished the conversation at {}.\nPartial recording:\n{partial}",
            path.display()
        );
    }
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Compare a recording with its committed transcript, or regenerate it.
///
/// The failure message carries the *first differing line* of each side rather than
/// the whole file: a 64 KiB fragment's record is one very long line, and a diff that
/// dumps both copies of it is unreadable at exactly the moment it matters.
fn compare_or_regenerate(golden: &Path, observed: &str) {
    if std::env::var_os(REGENERATE_ENV).is_some() {
        let dir = golden.parent().expect("a transcript path has a directory");
        std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("mkdir {}: {e}", dir.display()));
        std::fs::write(golden, observed)
            .unwrap_or_else(|e| panic!("write {}: {e}", golden.display()));
        eprintln!(
            "REGENERATED {} — review the diff before committing it",
            golden.display()
        );
        return;
    }
    let want = std::fs::read_to_string(golden).unwrap_or_else(|e| {
        panic!(
            "{}: {e}\nThis transcript is generated, never typed: run\n  \
             {REGENERATE_ENV}=1 cargo test -p serial-nexus-itest --test p8_daemon_transcript\n\
             and commit the result.",
            golden.display()
        )
    });
    if want == observed {
        return;
    }
    let (mut wl, mut ol) = (want.lines(), observed.lines());
    let mut lineno = 0usize;
    loop {
        lineno += 1;
        match (wl.next(), ol.next()) {
            (None, None) => break,
            (w, o) if w == o => continue,
            (w, o) => panic!(
                "{} disagrees with the live daemon at line {lineno}.\n  \
                 committed: {}\n  observed:  {}\n\nIf the boundary legitimately changed, \
                 regenerate with\n  {REGENERATE_ENV}=1 cargo test -p serial-nexus-itest \
                 --test p8_daemon_transcript\nand commit the diff; otherwise the daemon \
                 changed the bytes it sends a codec child.",
                golden.display(),
                w.map(elide).unwrap_or_else(|| "<end of file>".to_owned()),
                o.map(elide).unwrap_or_else(|| "<end of file>".to_owned()),
            ),
        }
    }
}

/// A line, shortened in the middle so a 130 KB record still reads as one line.
fn elide(line: &str) -> String {
    if line.len() <= 160 {
        return line.to_owned();
    }
    format!(
        "{}…[{} more chars]…{}",
        &line[..100],
        line.len() - 140,
        &line[line.len() - 40..]
    )
}

/// Read the replayer's verdict once it exists.
fn read_verdict(path: &Path) -> Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read the replay verdict {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!(
            "parse the replay verdict {}: {e}; text={text:?}",
            path.display()
        )
    })
}

/// Wait for a replay verdict satisfying `cond`, or panic with what it did say.
///
/// A settled **mismatch** ends the wait immediately even when `cond` is still false:
/// once the replayer has refused a byte it will never match another, so waiting the
/// rest of the timeout out only delays the diagnosis it has already written down.
/// (Measured while proving this guard: the wait cost 60 s and reported the same
/// verdict it had at 0.1 s.)
fn wait_verdict(path: &Path, what: &str, cond: impl Fn(&Value) -> bool) -> Value {
    let ok = wait_until(Duration::from_secs(60), || {
        path.exists() && {
            let v = read_verdict(path);
            cond(&v) || !v["mismatch_offset"].is_null()
        }
    }) && cond(&read_verdict(path));
    assert!(
        ok,
        "the replay never {what}; last verdict: {}",
        if path.exists() {
            read_verdict(path).to_string()
        } else {
            "<no verdict file>".to_owned()
        }
    );
    read_verdict(path)
}

/// Flip one bit of one byte of the first daemon→child record, returning the mutated
/// transcript and the offset/expected/observed triple the replayer must report.
///
/// The *first* record and its *last* byte, so the offset within the whole
/// daemon→child stream is `frame_len - 1` and the assertion can be exact rather than
/// merely "something differed". The record must be pure `H:` hex for that to hold —
/// with an `R:` tail the last hex byte is not the last frame byte — which is checked
/// rather than assumed.
fn mutate_one_byte(golden: &str) -> (String, usize, u8, u8) {
    let mut out = String::with_capacity(golden.len());
    let mut done: Option<(usize, u8, u8)> = None;
    for line in golden.lines() {
        if done.is_none() && line.starts_with("> ") {
            assert!(
                !line.contains(" R:"),
                "the first daemon→child record must be pure hex for the planted \
                 mutation to have a known offset; got: {}",
                elide(line)
            );
            let hex = line
                .split_whitespace()
                .nth(1)
                .and_then(|f| f.strip_prefix("H:"))
                .expect("the first daemon→child record has an H: field");
            assert!(hex.len() >= 2, "an empty first record cannot be mutated");
            let last = u8::from_str_radix(&hex[hex.len() - 2..], 16).expect("hex byte");
            let flipped = last ^ 0x01;
            out.push_str(&format!("> H:{}{flipped:02x}\n", &hex[..hex.len() - 2]));
            done = Some((hex.len() / 2 - 1, last, flipped));
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    let (off, want, got) = done.expect("the transcript has at least one daemon→child record");
    (out, off, want, got)
}

// --- the host-side conversation --------------------------------------------

/// The codec child's scripted half of the host conversation.
///
/// Seven frames covering the whole §8 event vocabulary plus the two identities that
/// are not ordinary channels: the reserved empty one (the multiplexed side, §15.22)
/// and one the node was never configured for (CODEC-1). Built as [`Event`]s and run
/// through [`encode`] so this half is the real envelope too, not a transcription of
/// it.
fn host_child_script() -> Vec<Event> {
    vec![
        // open/close, first half.
        Event::open("console"),
        // Ordinary hostward channel data. Nothing is bound to `console`, so §5 says
        // these nine bytes are charged to `discarded_unattached` — asserted below.
        Event::data("console", b"boot ok\r\n".to_vec()),
        // The reserved empty identity, child→daemon: a demux's remux output, headed
        // for the device. This graph has no device, so §5 says the fifteen bytes are
        // charged to `multiplexed.discarded_targetward` rather than vanishing.
        Event::data("", b"\x01\x02device-bound\xff".to_vec()),
        // An identity the node has no channel for: eleven bytes counted and the
        // identity named (CODEC-1), which is the only thing separating a typo from a
        // device multiplexing a stream nobody enumerated.
        Event::data("gps", b"$GPGGA,1234".to_vec()),
        // …and an announcement on it, which records the identity with no bytes.
        Event::open("gps"),
        // The fourth event kind, on a configured channel.
        Event::error("telemetry", "framing error: resync"),
        // open/close, second half: `console` goes back to `waiting`.
        Event::close("console"),
    ]
}

/// The daemon→child half: every targetward write the harness makes, in order, all on
/// **one** channel (see the module comment — two forwarders into one merge queue
/// have no defined order, and pinning one would pin a scheduling artifact).
fn drive_host_conversation(rpc: &Rpc) {
    rpc.send("mux/console", "ping", false, 10_000)
        .expect("send a short line to the codec channel");
    // Comfortably larger than one frame, expressed against the *public*
    // `MAX_FRAME_SIZE` rather than against the daemon's private payload cap: the
    // test says "too big for one frame" and the transcript pins where the daemon
    // actually splits it (§15.24, invariant 3). Nothing here knows the cap.
    let oversize = "A".repeat(MAX_FRAME_SIZE + 64);
    rpc.send("mux/console", &oversize, false, 30_000)
        .expect("send a line larger than one envelope frame");
    rpc.send("mux/console", END_MARKER, false, 10_000)
        .expect("send the end-of-conversation sentinel");
}

/// The `[[node]]` block for the host conversation's exec codec node.
///
/// `restart_backoff_ms` is pinned at the schema maximum on purpose: a child that
/// exits is a crash to the daemon (§7.6) and gets respawned, and a respawned
/// recorder would overwrite a complete recording with a second, half-observed one —
/// the evidence-destroying shape. An hour is longer than any run here.
fn host_config(argv: &[String]) -> String {
    format!(
        r#"
[[node]]
type = "codec"
name = "mux"
codec = "exec"
faces = "target"
channels = ["console", "telemetry"]
[node.attributes]
argv = {argv}
restart_backoff_ms = 3600000
"#,
        argv = toml_argv(argv),
    )
}

/// The honesty block written into the host transcript's own header (AGENTS §8).
const HOST_NOTES: &[&str] = &[
    "Generated by itest/tests/p8_daemon_transcript.rs against a live serial-nexus-daemon.",
    "Never hand-written. Regenerate with SNX_REGENERATE_TRANSCRIPTS=1 and commit the diff.",
    "",
    "Scope: an `exec` codec node, faces=target, channels [console, telemetry], with NO",
    "upstream device attached and nothing bound to either channel. Every targetward",
    "write is on `console`.",
    "",
    "Establishes: the exact bytes the daemon frames a targetward channel write into,",
    "including where it splits a chunk larger than one frame (design §15.24); and the",
    "exact bytes a conforming child emits for data/open/close/error, for the reserved",
    "multiplexed identity, and for an identity the node is not configured for.",
    "",
    "Does NOT establish: any interleaving between the two directions (two pipes polled",
    "concurrently, §15.22); any order between frames on different channels (one merge",
    "queue, one forwarder per channel); timing, backpressure, restart or teardown",
    "behaviour; and the hostward device stream, which this graph has none of — see",
    "exec-demux-device.transcript, which does, and is Linux-only for it.",
    "",
    "Bytes alone also cannot show that these frames landed in the §5 loss counters they",
    "are supposed to. The generating test asserts that beside the comparison.",
];

/// The loss and lifecycle counters the host conversation's child half must produce.
/// Bytes are the transcript's subject; these are what those bytes *did*, which no
/// byte comparison can see (§5: loss is always visible and attributable).
fn assert_host_counters(rpc: &Rpc, phase: &str) {
    let settled = wait_until(Duration::from_secs(20), || {
        rpc.node("mux").is_some_and(|m| {
            m["discarded_unconfigured_channel"].as_u64() == Some(11)
                && m["multiplexed"]["discarded_targetward"].as_u64() == Some(15)
                && m["channels"]["console"]["discarded_unattached"].as_u64() == Some(9)
                && m["channels"]["console"]["status"].as_str() == Some("waiting")
        })
    });
    let mux = rpc.node("mux").expect("the exec codec node is in state");
    assert!(
        settled,
        "[{phase}] the codec node's counters never settled: {mux}"
    );
    assert_eq!(
        mux["unconfigured_channels"],
        serde_json::json!(["gps"]),
        "[{phase}] the identity actually on the wire is named (CODEC-1): {mux}"
    );
    assert_eq!(
        mux["channels"]["console"]["delivered_hostward"].as_u64(),
        Some(0),
        "[{phase}] nothing is bound to `console`, so nothing was delivered: {mux}"
    );
    assert_eq!(
        mux["discarded_unframable"].as_u64(),
        Some(0),
        "[{phase}] every fragment of the oversize write fit a frame (§15.24): {mux}"
    );
}

/// Phase 1: the daemon writes its own half of the conversation, and the committed
/// transcript must equal it byte for byte.
#[test]
fn the_host_side_of_the_exec_boundary_matches_its_golden_transcript() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let script = d.run().join("host-script.transcript");
    let observed = d.run().join("host-observed.transcript");
    write_script(&script, &host_child_script());

    rpc.load_toml(
        &host_config(&recorder_argv(&script, &observed, HOST_NOTES)),
        false,
    )
    .expect("load the recording graph");

    drive_host_conversation(rpc);
    let recording = read_recording(&observed);
    compare_or_regenerate(&golden_path("exec-demux-host.transcript"), &recording);
    assert_host_counters(rpc, "record");
}

/// Phases 2 and 3: the committed transcript replays clean against a live daemon, and
/// a one-byte mutation of it is caught with the offset named — the item's own
/// validation, kept as a guard so the comparison can never quietly stop comparing.
#[test]
fn replaying_the_host_transcript_passes_and_a_one_byte_mutation_fails_it() {
    let golden = golden_path("exec-demux-host.transcript");
    let text = std::fs::read_to_string(&golden).unwrap_or_else(|e| {
        panic!(
            "{}: {e}\nGenerate it first:\n  {REGENERATE_ENV}=1 cargo test -p \
             serial-nexus-itest --test p8_daemon_transcript",
            golden.display()
        )
    });

    // --- phase 2: the committed transcript is what the daemon really sends ---
    {
        let d = Daemon::start();
        let rpc = d.rpc();
        let verdict = d.run().join("replay.json");
        rpc.load_toml(&host_config(&replayer_argv(&golden, &verdict)), false)
            .expect("load the replay graph");
        drive_host_conversation(rpc);
        let v = wait_verdict(&verdict, "reproduced the transcript", |v| {
            v["pass"].as_bool() == Some(true)
        });
        assert_eq!(
            v["matched_bytes"], v["expected_bytes"],
            "a passing replay matched the whole daemon→child stream: {v}"
        );
        assert_eq!(
            v["trailing_bytes"].as_u64(),
            Some(0),
            "the daemon sent nothing the transcript does not account for: {v}"
        );
        // The child half of the committed file is the child half of the recording,
        // so the same counters must come out of a replay run. That is what makes the
        // `<` records load-bearing rather than decorative: mutate one and this fails
        // even though the `>` comparison still passes.
        assert_host_counters(rpc, "replay");
    }

    // --- phase 3: one flipped byte, and the replay says exactly where ---
    let (mutated, offset, want, got) = mutate_one_byte(&text);
    let d = Daemon::start();
    let rpc = d.rpc();
    let path = d.run().join("mutated.transcript");
    let verdict = d.run().join("mutated-verdict.json");
    std::fs::write(&path, &mutated).expect("write the mutated transcript");
    rpc.load_toml(&host_config(&replayer_argv(&path, &verdict)), false)
        .expect("load the mutated-replay graph");
    drive_host_conversation(rpc);
    let v = wait_verdict(&verdict, "rejected the mutated transcript", |v| {
        !v["mismatch_offset"].is_null()
    });
    assert_eq!(
        v["pass"].as_bool(),
        Some(false),
        "a mutated transcript must not pass: {v}"
    );
    assert_eq!(
        v["mismatch_offset"].as_u64(),
        Some(offset as u64),
        "the replay names the byte it refused: {v}"
    );
    assert_eq!(
        v["expected_byte"].as_str(),
        Some(format!("0x{got:02x}").as_str()),
        "the transcript (mutated) asked for the flipped byte: {v}"
    );
    assert_eq!(
        v["observed_byte"].as_str(),
        Some(format!("0x{want:02x}").as_str()),
        "and the daemon sent the original: {v}"
    );
}

// --- the park's own promise --------------------------------------------------

/// **A parked transcript child stops when its stdin closes** — every one of the three
/// places it parks.
///
/// The sweep below catches the *class* from the daemon's side; this names the property
/// the product promises, at the boundary that promises it, with no daemon in the way.
/// It is the cheaper of the two and the one whose failure message points at the sim.
///
/// The defect it guards: `park_transcript` used to call `std::io::stdin().lock()` for
/// itself, and two of its three call sites are inside a function whose `StdinLock` is
/// still alive. `Stdin` is a plain `Mutex`, not the `ReentrantLock` behind `Stdout` and
/// `Stderr`, so that second lock was a same-thread self-deadlock: `futex(FUTEX_WAIT…)`,
/// forever, *before* the first `read`. The EOF that is §15.43's whole leash arrived at a
/// process that had stopped listening for it.
///
/// Each case closes the child's stdin and asserts an exit, which is exactly what the
/// daemon's teardown does. Deliberately not a `--record`-file assertion: the file
/// appeared fine while the process stayed forever.
#[test]
fn a_parked_transcript_child_exits_when_its_stdin_closes() {
    let dir = TempRun::new();
    let script = dir.join("script.transcript");
    // One daemon→child record and no child half: enough to give the replay something to
    // disagree with, and nothing for the child to write.
    std::fs::write(&script, "> H:aa\n").expect("write the script");

    /// One parking call site: how it is reached, and what (if anything) must be fed
    /// before the EOF for the child to be sitting in the park when it arrives.
    struct ParkCase {
        what: &'static str,
        args: Vec<String>,
        feed: Option<&'static [u8]>,
    }

    // The three call sites, in the order they appear in `sim/src/main.rs`.
    let cases = vec![
        ParkCase {
            what: "the error path (an unreadable transcript)",
            args: vec![
                "--transcript".to_owned(),
                dir.join("does-not-exist").display().to_string(),
                "--record".to_owned(),
                dir.join("never.transcript").display().to_string(),
            ],
            feed: None,
        },
        ParkCase {
            what: "the recorder, after its conversation ended",
            args: vec![
                "--transcript".to_owned(),
                script.display().to_string(),
                "--record".to_owned(),
                dir.join("observed.transcript").display().to_string(),
            ],
            feed: None,
        },
        ParkCase {
            what: "the replayer, parked on a mismatch it has already written down",
            args: vec![
                "--transcript".to_owned(),
                script.display().to_string(),
                "--verdict".to_owned(),
                dir.join("verdict.json").display().to_string(),
            ],
            // One byte that is not the 0xaa the transcript asks for: the replayer
            // records the mismatch and parks rather than exiting (a child that exits is
            // a crash to the daemon, which would restart it over the diagnosis).
            feed: Some(b"\xbb"),
        },
    ];

    for ParkCase { what, args, feed } in cases {
        let mut child = Command::new(bin("serial-nexus-sim"))
            .arg("transcript")
            .args(&args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn serial-nexus-sim transcript");
        {
            let mut stdin = child.stdin.take().expect("piped stdin");
            if let Some(bytes) = feed {
                use std::io::Write;
                stdin.write_all(bytes).expect("feed the replayer");
                stdin.flush().expect("flush");
                // Let it reach the park before the EOF, so this tests the park and not
                // a read that had not started.
                std::thread::sleep(Duration::from_millis(200));
            }
            // …and the EOF, which is the only thing that may stop it.
        }
        let mut status = None;
        let exited = wait_until(Duration::from_secs(15), || {
            status = child.try_wait().expect("try_wait");
            status.is_some()
        });
        if !exited {
            let pid = child.id();
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "{what}: `serial-nexus-sim transcript` was still running 15 s after its \
                 stdin closed (pid {pid}, killed). A parked codec child's only leash is \
                 that EOF (§15.43); one that misses it outlives every run that spawns it."
            );
        }
    }
}

// --- the sweep that would have caught the recorder ---------------------------

/// The orphan sweep's own proof, which is what stops it being a gate that asserts
/// nothing (AGENTS §3, plan §3 rule 22).
///
/// Every [`Daemon`] in this suite now sweeps its process group on drop and fails if the
/// daemon left anything running. That check is silent when it passes — its output is
/// identical to its not-running output — so on its own it is exactly the shape §3 warns
/// about. This test supplies the other half by **planting** the violation: a codec child
/// that never reads its stdin, which is precisely how the recorder leaked (it
/// self-deadlocked on `std::io::Stdin`'s non-reentrant mutex before its first `read`, so
/// the EOF that is §15.43's whole leash arrived at a process that was never going to
/// look). The planted child is `sleep`, with nothing serial_nexus about it, because the
/// sweep matches on the daemon's **process group** and not on a name — the matcher and
/// the walker, both exercised.
///
/// Then it kills the plant and watches the sweep go quiet, so a sweep that had rotted
/// into "always reports something" fails here too.
#[test]
fn the_orphan_sweep_sees_a_codec_child_that_ignores_its_stdin() {
    let d = Daemon::start();
    // `sh -c 'exec sleep 300'` rather than a bare path: `sleep` is found on `PATH` on
    // both platforms of record, and `exec` leaves exactly one process to find.
    let argv: Vec<String> = ["/bin/sh", "-c", "exec sleep 300"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    d.rpc()
        .load_toml(&host_config(&argv), false)
        .expect("load a graph whose codec child ignores its stdin");

    let planted = {
        let mut found = Vec::new();
        assert!(
            wait_until(Duration::from_secs(20), || {
                found = d.survivors();
                !found.is_empty()
            }),
            "the sweep never saw the codec child the daemon spawned into its process \
             group; a sweep that cannot see a live child cannot see a leaked one"
        );
        found
    };
    assert_eq!(
        planted.len(),
        1,
        "the graph has exactly one codec child: {planted:?}"
    );
    let (pid, argv_seen) = planted.into_iter().next().expect("one planted child");
    assert!(
        argv_seen.contains("sleep"),
        "the sweep reports the survivor's argv, which is the whole diagnosis a leak \
         report carries; got {argv_seen:?}"
    );

    // …and it reports a *clean* group as clean. Killing the plant is also what keeps
    // this test's own `Daemon` drop from failing on it — the daemon's restart backoff
    // is pinned at an hour, so nothing respawns.
    assert!(
        Command::new("kill")
            .arg("-9")
            .arg(pid.to_string())
            .status()
            .expect("run kill(1)")
            .success(),
        "kill the planted child (pid {pid})"
    );
    assert!(
        wait_until(Duration::from_secs(20), || d.survivors().is_empty()),
        "the sweep still reports survivors after the only one was killed: {:?}",
        d.survivors()
    );
}

// --- the device-side conversation ------------------------------------------

/// The one shape the host conversation structurally cannot carry: the raw device
/// stream arriving at the child's stdin on the reserved empty identity.
///
/// It needs a real hostward producer, and the only host-facing node that is one
/// without a second daemon is `serial` — over a pty, which is the pty-as-serial
/// doctrine and Linux-only (`serial2` rejects a pty on macOS: `ENOTTY`), exactly as
/// `p5_demux` is.
#[cfg(target_os = "linux")]
mod device {
    use std::time::Duration;

    use serial_nexus_itest::{Daemon, Sim, wait_until};

    use super::{
        END_MARKER, compare_or_regenerate, daemon_records, golden_path, read_recording, record_len,
        recorder_argv, replayer_argv, toml_argv, wait_verdict, write_script,
    };

    /// A single 32-byte `write()` from the device stand-in. Small on purpose: one
    /// completed write into an idle raw pty is delivered to the daemon's blocked
    /// `read` in one piece, so the frame boundary is a property of the write, not of
    /// the scheduler. The note in the transcript's header says so, because a *large*
    /// device stream's chunking genuinely is a kernel property and this file must not
    /// be read as a promise about it.
    const DEVICE_BYTES: &str = "32";

    const NOTES: &[&str] = &[
        "Generated by itest/tests/p8_daemon_transcript.rs against a live serial-nexus-daemon.",
        "Never hand-written. Regenerate with SNX_REGENERATE_TRANSCRIPTS=1 and commit the diff.",
        "",
        "Scope: a `serial` node over a pty device stand-in, feeding a held edge into an",
        "`exec` codec node (faces=target, channels [console]). Linux-only: a pty as a",
        "serial device is the pty-as-serial doctrine, which macOS rejects (ENOTTY).",
        "",
        "Establishes: the exact bytes the daemon frames a hostward *device* read into —",
        "the reserved empty channel identity, daemon->child (§15.22) — followed by the",
        "targetward sentinel, so both sources of a child's stdin appear in one file.",
        "",
        "Does NOT establish how a kernel chunks a device read. The device record here is",
        "one completed 32-byte write() on an idle raw pty, read whole; a large or",
        "continuous stream is split wherever the kernel splits it, and the daemon frames",
        "each chunk it is handed. This file pins the framing, not the chunking.",
        "",
        "The device record is sequenced before the sentinel by reading the recorder's",
        ".partial file, never by waiting a while.",
    ];

    /// The graph: pty device -> held edge -> exec codec with one channel.
    fn config(dev: &std::path::Path, argv: &[String]) -> String {
        format!(
            r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
[[node]]
type = "codec"
name = "mux"
codec = "exec"
faces = "target"
channels = ["console"]
[node.attributes]
argv = {argv}
restart_backoff_ms = 3600000
[[edge]]
a = "usb0"
b = "mux"
write_mode = "held"
"#,
            dev = dev.display(),
            argv = toml_argv(argv),
        )
    }

    /// Bring the graph up with a `serial-nexus-sim pty --source` device stand-in
    /// gated on `go`, and return the pieces the caller drives.
    fn boot(d: &Daemon, argv: &[String]) -> (Sim, std::path::PathBuf) {
        let dev = d.run().join("dev");
        let go = d.run().join("go");
        // `--hold-ms` keeps the pts present after the burst: a real port stays
        // plugged in when it stops transmitting (§7.1), and an unplug here would
        // fault the serial node mid-conversation.
        let sim = Sim::spawn(
            &[
                "pty",
                "--link",
                &dev.to_string_lossy(),
                "--source",
                "--bytes",
                DEVICE_BYTES,
                "--seed",
                "3",
                "--wait-file",
                &go.to_string_lossy(),
                "--hold-ms",
                "120000",
                "--timeout-ms",
                "120000",
            ],
            Some(&dev),
        );
        let rpc = d.rpc();
        rpc.load_toml(&config(&dev, argv), false)
            .expect("load the device graph");
        assert!(
            rpc.wait_status("usb0", "active", Duration::from_secs(20)),
            "the serial node over the pty never went active: {:?}",
            rpc.node("usb0")
        );
        assert!(
            rpc.wait_status("mux", "active", Duration::from_secs(20)),
            "the exec codec node never went active: {:?}",
            rpc.node("mux")
        );
        (sim, go)
    }

    /// Record the device conversation, compare it with its committed transcript, and
    /// replay that transcript against a second daemon.
    pub fn run() {
        let golden = golden_path("exec-demux-device.transcript");

        // ---- record ----
        let recording = {
            let d = Daemon::start();
            let rpc = d.rpc();
            let script = d.run().join("device-script.transcript");
            let observed = d.run().join("device-observed.transcript");
            // No scripted child half at all: this transcript's subject is the
            // daemon->child direction, and a child emission here would race the
            // held-lock grant on the mux edge for no coverage the host transcript
            // does not already have deterministically.
            write_script(&script, &[]);
            let (_sim, go) = boot(&d, &recorder_argv(&script, &observed, NOTES));

            let mut partial = observed.as_os_str().to_owned();
            partial.push(".partial");
            let partial = std::path::PathBuf::from(partial);

            std::fs::File::create(&go).expect("release the device burst");
            assert!(
                wait_until(Duration::from_secs(30), || daemon_records(&partial) >= 1),
                "the device read never reached the codec child; partial recording: {}",
                std::fs::read_to_string(&partial).unwrap_or_default()
            );
            // Only now: the two sources feed one merge queue, so the sentinel is
            // ordered after the device read by *observing* the device read, not by
            // hoping.
            rpc.send("mux/console", END_MARKER, false, 10_000)
                .expect("send the end-of-conversation sentinel");
            let recording = read_recording(&observed);
            assert_eq!(
                rpc.node("mux")
                    .map(|m| m["multiplexed"]["discarded_targetward"].clone()),
                Some(serde_json::json!(0)),
                "nothing was lost on the multiplexed side of a healthy device graph"
            );
            recording
        };
        compare_or_regenerate(&golden, &recording);

        // ---- replay ----
        let text = std::fs::read_to_string(&golden)
            .unwrap_or_else(|e| panic!("read {}: {e}", golden.display()));
        let device_frame = text
            .lines()
            .find(|l| l.starts_with("> "))
            .map(record_len)
            .expect("the device transcript has a daemon→child record");

        let d = Daemon::start();
        let rpc = d.rpc();
        let verdict = d.run().join("replay.json");
        let (_sim, go) = boot(&d, &replayer_argv(&golden, &verdict));
        std::fs::File::create(&go).expect("release the device burst");
        // The replayer publishes progress on every read, so the same ordering the
        // recording had is reproduced by watching `matched_bytes` rather than by a
        // sleep — and the wait itself proves the device frame replayed byte-exactly.
        wait_verdict(&verdict, "matched the device frame", |v| {
            v["matched_bytes"].as_u64().unwrap_or(0) >= device_frame as u64
        });
        rpc.send("mux/console", END_MARKER, false, 10_000)
            .expect("send the end-of-conversation sentinel");
        let v = wait_verdict(&verdict, "reproduced the transcript", |v| {
            v["pass"].as_bool() == Some(true)
        });
        assert_eq!(
            v["trailing_bytes"].as_u64(),
            Some(0),
            "the daemon sent nothing the device transcript does not account for: {v}"
        );
    }
}

/// The device half of "empty-channel in both directions": the raw device stream
/// arriving at a codec child's stdin, recorded, compared, and replayed.
#[test]
fn the_device_side_of_the_exec_boundary_matches_its_golden_transcript() {
    #[cfg(target_os = "linux")]
    device::run();
    #[cfg(not(target_os = "linux"))]
    eprintln!(
        "SKIP the_device_side_of_the_exec_boundary_matches_its_golden_transcript: needs a \
         Linux pty-as-serial device (serial2 rejects a pty on macOS: ENOTTY); the host-side \
         transcript runs everywhere"
    );
}
