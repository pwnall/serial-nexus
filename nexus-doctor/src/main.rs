#![deny(unsafe_code)]

//! `nexus-doctor` — the serial_nexus capability checker (design §15.17).
//!
//! One consolidated binary that runs every kernel-behavior probe the design
//! depends on (P1 EXTPROC/TIOCPKT, P2 PTY presence, P3 serial fit, P4 by-id
//! resolution) plus environment checks, and emits a copy-pasteable Markdown
//! report — the expected first attachment on any support request — with a
//! `--json` twin for CI.
//!
//! The daemon never consumes this output: its degradation paths (e.g. §7.2's
//! reconciliation poll) are unconditional, so a wrong probe can mislead a
//! developer but never the data plane. Passive by default — any probe that
//! opens a real serial port requires that port to be named with `--port`,
//! because a listed port could be wired to live equipment.

mod probes;
mod report;
mod sys;

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;

use report::Report;

#[derive(Parser)]
#[command(
    name = "nexus-doctor",
    version,
    about = "serial_nexus capability checker (§15.17)"
)]
struct Cli {
    /// Emit JSON instead of Markdown (for CI: `nexus-doctor --json | jq -e ...`).
    #[arg(long)]
    json: bool,
    /// Emit Markdown (the default).
    #[arg(long)]
    markdown: bool,
    /// A serial port to include in P3 (repeatable). Required to open any real
    /// port — passive by default (§3).
    #[arg(long = "port")]
    ports: Vec<PathBuf>,
    /// Root prefix for /dev resolution — a test seam for fixture by-id trees
    /// (§3). Defaults to `/`.
    #[arg(long, default_value = "/")]
    dev_root: PathBuf,
}

fn main() {
    let cli = Cli::parse();
    let sys_root = PathBuf::from("/sys");

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
        probe_list.push(report::Probe::new(
            "P3",
            "serial-port fit",
            "Custom baud, TIOCEXCL exclusivity, modem lines, and break on a real port (§7.1).",
        ).verdict(
            report::Status::skipped("no --port named"),
            "Re-run with --port /dev/ttyUSB0 (a dangling converter is enough — no target device needed, §13).",
        ));
    } else {
        for port in &cli.ports {
            probe_list.push(probes::p3_serial(port));
        }
    }

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
