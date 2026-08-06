#![forbid(unsafe_code)]

//! `serial-nexus-doctor` — the serial_nexus capability checker (design §15.17).
//!
//! One consolidated binary that runs every kernel-behavior probe the design
//! depends on (P1 EXTPROC/TIOCPKT, P2 PTY presence, P3 serial fit, P4 device
//! identity resolution, P5 rig certification, P6 post-hangup pty readiness, P7 collapsed
//! client-session evidence, P8 epoll vs read(2) on a pty master, P9 poll(2)
//! timeout granularity, P10 pty buffer depth, P11 real-port line-state counters,
//! P12 the session-boundary edge P7's packet route substitutes for on Linux)
//! plus environment checks, and emits a copy-pasteable Markdown report — the
//! expected first attachment on any support request — with a `--json` twin for
//! CI.
//!
//! The `--json` twin is also the **cross-kernel diff artifact**: production runs
//! Linux 6.18 and this tree is developed on 7.0, so §13 forbids a one-way
//! decision resting on 7.0-only evidence. Every probe emits its raw measurements
//! (counts, flags, bytes in hex, microseconds, byte depths) beside its verdict so
//! two runs can be diffed numerically — P6..P12 exist for exactly that, and each
//! one measures a premise some shipped decision currently rests on.
//!
//! The daemon never consumes this output: its degradation paths (e.g. §7.2's
//! reconciliation poll) are unconditional, so a wrong probe can mislead a
//! developer but never the data plane. Passive by default — any probe that
//! opens a real serial port requires that port to be named with `--port`,
//! because a listed port could be wired to live equipment.

mod probes;
mod report;

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;

use report::Report;

#[derive(Parser)]
#[command(
    name = "serial-nexus-doctor",
    version,
    about = "serial_nexus capability checker (§15.17)"
)]
struct Cli {
    /// Emit JSON instead of Markdown (for CI: `serial-nexus-doctor --json | jq -e ...`).
    #[arg(long)]
    json: bool,
    /// Emit Markdown (the default).
    #[arg(long)]
    markdown: bool,
    /// A serial port to include in P3, P5, P11 and P14 (repeatable). Required to
    /// open any real port — passive by default (§3), because opening a port
    /// toggles DTR on equipment that may be live. P14 additionally *transmits at
    /// every rate it tries*, up to the adapter's ceiling, so naming two ports is
    /// a statement that they are wired to each other and to nothing else.
    #[arg(long = "port")]
    ports: Vec<PathBuf>,
    /// Root prefix for /dev and /sys resolution — a test seam for fixture by-id
    /// and sysfs trees (§3); `sys_root` is derived as `<dev-root>/sys`, so a
    /// single flag selects a self-contained fixture. Defaults to `/`.
    #[arg(long, default_value = "/")]
    dev_root: PathBuf,
    /// Recompute the observation **field-set** digest of an already-captured
    /// report and print it, then exit. The digest is a pure function of
    /// `.probes[].observations`, so it is computable for artifacts captured
    /// before the field existed — which is the only way `docs/doctor/` can be
    /// indexed without editing frozen output (§16.13).
    #[arg(long = "field-set", value_name = "REPORT_JSON")]
    field_set_of: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();

    // The recompute arm runs before anything else and probes nothing: it reads a
    // captured report and prints one digest, which is what lets a frozen artifact
    // be indexed without being edited (§16.13).
    if let Some(path) = &cli.field_set_of {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("{}: {e}", path.display());
            std::process::exit(2);
        });
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| {
            eprintln!("{}: not JSON: {e}", path.display());
            std::process::exit(2);
        });
        match report::field_set_of_report_json(&parsed) {
            Some(f) => {
                println!("{f}");
                return;
            }
            None => {
                eprintln!(
                    "{}: not a doctor report (no .probes[].observations)",
                    path.display()
                );
                std::process::exit(2);
            }
        }
    }

    let sys_root = cli.dev_root.join("sys");

    let generated_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64);

    let environment = probes::environment(&cli.dev_root, &sys_root, &cli.ports);

    let mut probe_list = vec![
        probes::p1_extproc(),
        probes::p2_presence(),
        probes::p4_resolver(&cli.dev_root, &sys_root),
    ];
    if cli.ports.is_empty() {
        // The question must be `probes::P3_QUESTION` verbatim, not a paraphrase:
        // it feeds the probe-set fingerprint, so a placeholder that words the
        // question differently makes a passive run and a rig run of the *same
        // binary* report themselves as incomparable.
        probe_list.push(report::Probe::new(
            "P3",
            "serial-port fit",
            probes::P3_QUESTION,
        ).verdict(
            report::Status::skipped("no --port named"),
            "Re-run with --port /dev/ttyUSB0 (a dangling converter is enough — no target device needed, §13).",
        ));
    } else {
        for port in &cli.ports {
            probe_list.push(probes::p3_serial(port));
        }
    }

    // P5 rig discovery + certification (§15.21). Opt-in like P3 — it transmits, so
    // it runs only on explicitly named ports; it classifies them collectively.
    //
    // What it discovers outlives the probe: P11 below reads the same ports and
    // must not explain their counters with an item P5 never transmitted. A skipped
    // P5 leaves the default — no certified pair, which is exactly true.
    let mut rig = probes::RigFacts::default();
    // The pairs discovery verified, carried by path. `RigFacts` answers "what
    // kind of rig is this" for a verdict; P14 has to transmit on the pair, so it
    // needs the ports themselves (§15.51).
    let mut pairs: Vec<probes::VerifiedPair> = Vec::new();
    if cli.ports.is_empty() {
        probe_list.push(
            report::Probe::new(
                "P5",
                "rig discovery and certification",
                probes::P5_QUESTION,
            )
            .verdict(
                report::Status::skipped("no --port named"),
                "Re-run with the rig's --ports (dangling converters and jumper wires suffice — no target device, §13).",
            ),
        );
    } else {
        let resolver = serial_nexus_core::Resolver::with_roots(&cli.dev_root, &sys_root);
        let (p5, facts, verified) = probes::p5_rig(&cli.ports, &resolver);
        probe_list.push(p5);
        rig = facts;
        pairs = verified;
    }

    // P6/P7 — the two pty last-close measurements (§6 detach-release, §7.2). Both
    // are pty-only, so they are passive and need no `--port`, exactly like P1/P2.
    // They run *after* P5 deliberately: P5's discovery window is wall-clock timed
    // against live peers (and `p7_p5`'s hot-unplug test times a sim against it), so
    // nothing new may be inserted ahead of it.
    probe_list.push(probes::p6_last_close_readiness());
    probe_list.push(probes::p7_collapsed_session());
    // P12 is P7's sibling and belongs beside it: the same question asked of the
    // *edge* mechanism rather than the readable-packet one, so the two are read
    // together to see which of them carries detach-release on this kernel (§15.39).
    // Numbered 12 rather than 8 because the probe set is append-only — an id is a
    // diff key across kernels, and renumbering P8..P11 would silently invalidate
    // every archived report.
    probe_list.push(probes::p12_session_edge());
    // P13 completes the trio: P7 asks what a collapsed session leaves *readable*,
    // P12 whether the boundary arrives as an *edge*, and P13 what the kernel does
    // with client bytes the master never read — retain, discard, or block the close
    // waiting for the reader. The first two cannot separate the last two answers,
    // because a master that never drains makes a drain-wait time out and look
    // exactly like a flush; P13's `close_microseconds` is what tells them apart
    // (notes §3.29, `docs/macos.md` 2026-08-04). Same append-only numbering rule as
    // P12: an id is a diff key across kernels, so new questions take new numbers.
    probe_list.push(probes::p13_last_close_disposition());

    // P8/P9/P10 — the data-plane shape measurements (invariant 1, §15.19's timer
    // floor, the hostward_buffer defaults). Pty- and poll-only, so passive like
    // P1/P2/P6/P7, and behind P5 for the same wall-clock reason. Together they add
    // roughly 0.5 s to a run: P8 ~0.26 s (2 × 64 passes at a 1 ms timeout), P9
    // ~0.26 s (4 timeouts × 16 samples), P10 milliseconds.
    probe_list.push(probes::p8_epoll_readiness());
    probe_list.push(probes::p9_poll_granularity());
    probe_list.push(probes::p10_pty_buffer_depth());

    // P11 — real-port line-state ioctls. Opt-in behind --port exactly like P3/P5:
    // it opens a real device (which toggles DTR), though it transmits nothing and
    // leaves the port's settings untouched. `p11_line_state` reports the skip
    // itself when the list is empty, so there is no placeholder branch here.
    //
    // It is handed P5's `RigFacts` because the counters it prints are only
    // explicable against what P5 actually transmitted: the deliberate baud
    // mismatch runs over a *certified pair* and nowhere else, so on a rig with
    // none (the skip branch above, or a dangling converter) P11 must not offer it
    // as the reason a `frame` count is nonzero.
    probe_list.push(probes::p11_line_state(&cli.ports, rig));

    // P15 — does a named port honour a requested flow-control mode? Opt-in behind
    // `--port` like P3/P5/P11, and placed here rather than beside P11 because it
    // **configures**: P11's contract is that it opens with the port's settings
    // unchanged, and this question cannot be asked without changing them. It runs
    // after P11 so P11 still sees the port as the operator left it, and before P14
    // because P14 re-proves its own baseline anyway and must stay last for the
    // wall-clock reason below. Two `tcsetattr` calls per port, so it costs nothing
    // against P14's rate ladder (notes §3.65 E).
    probe_list.push(probes::p15_flow_control_readback(&cli.ports));

    // P14 — the maximum-rate search (§15.51). **Last, and that is a rule rather
    // than a habit.** The ordering note above P6/P7 says nothing may be inserted
    // ahead of P5 because the passive probes are wall-clock timed; P14 is by far
    // the largest wall-clock consumer in this binary — it climbs a rate ladder
    // with three constant-airtime round-trips per direction per rung — so
    // anywhere but the end it would perturb every timing measurement P6..P13
    // takes. It is opt-in behind `--port` like P3/P5/P11 and additionally needs
    // a cross-paired rig, and it reports its own skips (the `p11_line_state`
    // shape), so there is no placeholder branch and no way for a passive run and
    // a rig run of this binary to word the question differently.
    probe_list.push(probes::p14_max_rate(&cli.ports, &pairs));

    let report = Report::new(generated_unix_ms, environment, probe_list);

    if cli.json {
        println!("{}", report.to_json());
    } else {
        // Markdown is the default; --markdown is accepted explicitly too.
        let _ = cli.markdown;
        print!("{}", report.to_markdown());
    }

    // A probe contradicting the design (unsupported) is a stop condition; a
    // skipped or degraded verdict is not (plan §4).
    std::process::exit(if report.any_unsupported() { 1 } else { 0 });
}
