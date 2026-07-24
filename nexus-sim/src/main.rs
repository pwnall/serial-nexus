#![forbid(unsafe_code)]

//! `nexus-sim` — the in-workspace test double (design plan §3).
//!
//! A purpose-built double that uses the *same permissive PTY and socket calls
//! as the daemon* — so validating with it exercises those calls twice. Every
//! mode is deterministic under `--seed`, prints a single JSON verdict line on
//! exit, and exits 0 only on pass.
//!
//! Phase 1 lands the `pty` and `client` modes plus the verdict plumbing, so the
//! judges exist before anything they will judge. `mux`/`envelope` arrive in
//! phase 5. Phase 6 adds `wire` (the §9 conformance driver / hostile-or-conforming
//! peer for a daemon leg) and `tcp-proxy` (a link-outage injector between two
//! daemons).

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::os::fd::AsFd;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc as std_mpsc;
use std::thread;
use std::time::{Duration, Instant};

use bytes::Bytes;
use clap::{Args, Parser, Subcommand};
use codec_api::{
    Event, EventKind, FrameDecoder, Hello, MAX_FRAME_SIZE, WIRE_MAGIC, WIRE_VERSION, encode,
    encode_hello, try_decode_hello,
};
use nix::fcntl::OFlag;
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::pty::{PtyMaster, grantpt, posix_openpt, unlockpt};
use nix::sys::termios::{
    BaudRate, LocalFlags, SetArg, cfgetospeed, cfmakeraw, cfsetspeed, tcgetattr, tcsetattr,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

// The sim resolves a pty master's slave path via the shared `nexus_sys::ptsname`
// (§16.3), so it holds no `unsafe` of its own and `#![forbid]`s it. `ptsname` here
// is a thin alias to keep the call sites unchanged.
use nexus_sys::ptsname;

#[derive(Parser)]
#[command(name = "nexus-sim", about = "serial_nexus test double (§3)")]
struct Cli {
    #[command(subcommand)]
    mode: Mode,
}

#[derive(Subcommand)]
enum Mode {
    /// Create a PTY pair and run a device-side behavior on the master, standing
    /// exactly where `/dev/ttyUSB0` will.
    Pty(PtyArgs),
    /// Open an existing PTY like an operator would and drive it.
    Client(ClientArgs),
    /// Emit (or manifest) reference-framed multichannel streams for a demux node
    /// to split, with per-channel checksums and a computed expected-loss set under
    /// `--corrupt-every` (§8, phase 5).
    Mux(MuxArgs),
    /// Drive an external codec child through the golden-vector envelope battery,
    /// proving any-language envelope conformance (§8, phase 5).
    Envelope(EnvelopeArgs),
    /// Drive an external codec child through the full exec-conformance battery
    /// (§15.26 / plan §10.5): golden vectors, full-duplex liveness (the §15.22
    /// deadlock class), fragmented-frame reassembly, and kill-and-restart
    /// cleanliness. The closed-repo CI entry point for an exec codec.
    #[command(name = "exec-conformance")]
    ExecConformance(ExecConformanceArgs),
    /// Speak the v1 wire protocol to a daemon leg as a hostile-or-conforming peer
    /// — the §9 conformance driver (crafted hellos, bad magic, oversize/unknown
    /// frames, unbound channels) and an echo peer for a single-daemon round-trip.
    Wire(WireArgs),
    /// Sit between two daemons on loopback and forward both directions, with
    /// `--drop-after`/`--restore-after` link-outage injection (§7.4, phase 6).
    TcpProxy(TcpProxyArgs),
    /// Bridge two PTY pairs in-process as a software null modem (`--link-a` ↔
    /// `--link-b`): a crossed pair with no hardware, for CI-testing doctor P5's
    /// discovery/classification without a bench (§3, plan validation item 7).
    #[command(name = "nullmodem")]
    NullModem(NullModemArgs),
}

#[derive(Args)]
struct NullModemArgs {
    /// Stable symlink to the first pts (one "port" of the crossed pair).
    #[arg(long)]
    link_a: PathBuf,
    /// Stable symlink to the second pts (the other "port").
    #[arg(long)]
    link_b: PathBuf,
    /// Exit after this many milliseconds with no traffic in either direction.
    #[arg(long, default_value_t = 10_000)]
    timeout_ms: u64,
}

#[derive(Args)]
struct PtyArgs {
    /// Maintain a stable symlink to the pts node (a device path stand-in).
    #[arg(long)]
    link: Option<PathBuf>,
    /// Echo everything read on the master back to it.
    #[arg(long)]
    echo: bool,
    /// Emit a seeded stream of `--bytes` bytes, then exit.
    #[arg(long)]
    source: bool,
    /// Consume and checksum up to `--bytes` bytes, then exit.
    #[arg(long)]
    sink: bool,
    /// Report the termios the far side applied to the pair, then exit.
    #[arg(long)]
    report_termios: bool,
    /// Hold the master open but never read it — the targetward input buffer fills
    /// and the daemon's writer backpressures (a stalled target, for the §9
    /// head-of-line property). Stays present until `--timeout-ms`.
    #[arg(long)]
    stall: bool,
    #[arg(long, default_value_t = 0)]
    seed: u64,
    /// Size for source/sink, e.g. `1MiB`, `64KiB`, `512`.
    #[arg(long)]
    bytes: Option<String>,
    /// Pace `--source` to at most this many bytes/second (default: unpaced, line
    /// rate). A paced source keeps a slow consumer present *while* it sheds, so
    /// drops are attributable to backpressure rather than absence.
    #[arg(long)]
    rate: Option<u64>,
    /// Gate `--source`: wait until this file exists before writing the payload
    /// (plan §3, presence != readiness). The harness creates it only once every
    /// consumer — a co-attached client and an open tap — is present and draining, so
    /// the seeded stream cannot outrun a not-yet-ready consumer and a byte-exact
    /// comparison is deterministic. Waits up to `--timeout-ms`.
    #[arg(long)]
    wait_file: Option<PathBuf>,
    #[arg(long, default_value_t = 5000)]
    timeout_ms: u64,
    /// After the exchange completes, keep the master (and thus the pts) open this
    /// long (ms) before exiting, so the device reads as still "plugged in" — a
    /// real serial port stays present when it stops transmitting (§7.1).
    #[arg(long)]
    hold_ms: Option<u64>,
}

#[derive(Args)]
struct ClientArgs {
    /// Path to the PTY to open (a symlink or pts node).
    #[arg(long)]
    path: PathBuf,
    /// What to send, e.g. `seeded:1MiB`.
    #[arg(long)]
    send: Option<String>,
    /// Expectation to verify: `echo` compares the returned stream to what was
    /// sent.
    #[arg(long, default_value = "")]
    expect: String,
    /// Report the termios the daemon applied to the PTY (from the client's side
    /// of the slave), then exit — without disturbing it. Verifies the §7.2
    /// baseline (raw, echo off, EXTPROC) end to end.
    #[arg(long)]
    report_termios: bool,
    /// Set the slave's baud (a standard rate) when opening, so the daemon's
    /// packet-mode termios observation (§7.2) has a distinctive value to report.
    #[arg(long)]
    set_baud: Option<u32>,
    /// After any exchange, keep the slave open this long (ms) before exiting, so
    /// a subscriber can observe the client-present/termios state.
    #[arg(long)]
    hold_ms: Option<u64>,
    /// Receive exactly this many hostward bytes (e.g. `512MiB`), checksum them
    /// incrementally, and report — the fast sink for the firehose test. Does not
    /// send. Mutually exclusive with `--drain`.
    #[arg(long)]
    recv: Option<String>,
    /// Handshake for the demux burst (plan §3, presence != readiness): read and
    /// DISCARD this many leading "primer" bytes before counting/checksumming the
    /// `--recv` payload. Paired with the mux's `--prime-bytes`. 0 = no primer.
    #[arg(long, default_value_t = 0)]
    skip: usize,
    /// Handshake: create this file the instant the read loop reads its first byte
    /// (the primer) — proof the client is actively draining, not merely present. The
    /// harness gates the payload burst on this, closing the presence-vs-readiness
    /// race that made the initial burst outrun a not-yet-reading client.
    #[arg(long)]
    ready_file: Option<PathBuf>,
    /// Read the hostward stream until it goes quiet (no bytes for `--quiet-ms`),
    /// counting every byte — the fully-draining reader for exact loss accounting.
    /// Combine with `--read-rate` to be a slow consumer. Does not send.
    #[arg(long)]
    drain: bool,
    /// Throttle reads to at most this many bytes/second (a slow consumer). Applies
    /// to `--recv`/`--drain`.
    #[arg(long)]
    read_rate: Option<u64>,
    /// For `--drain`: stop after this many ms with no new bytes (default 1000).
    #[arg(long)]
    quiet_ms: Option<u64>,
    #[arg(long, default_value_t = 0)]
    seed: u64,
    #[arg(long, default_value_t = 10_000)]
    timeout_ms: u64,
}

#[derive(Args)]
struct MuxArgs {
    /// Channel identity to emit (repeatable). Must match the demux node's
    /// configured channel list, in the same order.
    #[arg(long = "channel")]
    channels: Vec<String>,
    /// Bytes of seeded data per channel (e.g. `8MiB`).
    #[arg(long, default_value = "1MiB")]
    bytes: String,
    #[arg(long, default_value_t = 7)]
    seed: u64,
    /// Corrupt one in every N emitted frames (0 = none) by mangling its type byte,
    /// keeping the length prefix intact so the decoder resyncs exactly (§7.5).
    #[arg(long, default_value_t = 0)]
    corrupt_every: u64,
    /// Maximum data payload per frame.
    #[arg(long, default_value_t = 4096)]
    frame_size: usize,
    /// Compute and print the per-channel manifest (expected delivered bytes and
    /// checksums) without touching a PTY, then exit — the deterministic oracle the
    /// channel clients check against.
    #[arg(long)]
    manifest: bool,
    /// Feed mode: create a PTY pair, maintain this symlink to the pts node (the
    /// device path the demux's serial opens), write the framed stream, then hold
    /// the device present until `--timeout-ms`.
    #[arg(long)]
    link: Option<PathBuf>,
    /// Feed mode: before writing the framed stream, wait until this file exists
    /// (up to `--timeout-ms`). Lets a harness attach the hostward channel clients
    /// first, so the presence-gated PTYs do not discard the initial burst.
    #[arg(long)]
    wait_file: Option<PathBuf>,
    /// Feed handshake (plan §3, presence != readiness): once this file exists (the
    /// clients are present), send a small `--prime-bytes` primer per channel BEFORE
    /// waiting on `--wait-file`. A primer small enough never to overflow a
    /// presence-gated PTY reliably reaches each client and lets it prove it is
    /// draining (its `--ready-file`), so the payload burst (gated on `--wait-file`)
    /// cannot outrun a not-yet-reading client. Requires `--prime-bytes > 0`.
    #[arg(long)]
    prime_file: Option<PathBuf>,
    /// Bytes of primer to send per channel before the payload (0 = no primer).
    #[arg(long, default_value_t = 0)]
    prime_bytes: usize,
    #[arg(long, default_value_t = 10_000)]
    timeout_ms: u64,
}

#[derive(Args)]
struct EnvelopeArgs {
    /// The external codec child to drive, as a shell command
    /// (e.g. `python3 tests/ext-codec/passthrough.py`). It must read envelope
    /// frames on stdin and re-emit them on stdout (a passthrough codec).
    #[arg(long)]
    exec: String,
    #[arg(long, default_value_t = 5_000)]
    timeout_ms: u64,
}

#[derive(Args)]
struct ExecConformanceArgs {
    /// The external codec child to drive, as a shell command
    /// (e.g. `python3 tests/ext-codec/passthrough.py`). It must speak the envelope
    /// on stdin/stdout and echo each frame it reads (a passthrough).
    #[arg(long)]
    exec: String,
    /// Per-frame echo timeout (ms). The full-duplex liveness check requires each
    /// frame's echo within this bound, so a half-duplex child (reads all before
    /// writing) trips it — the §15.22 deadlock class, made a test.
    #[arg(long, default_value_t = 2_000)]
    frame_timeout_ms: u64,
    /// How many frames the full-duplex liveness check interleaves.
    #[arg(long, default_value_t = 64)]
    liveness_frames: usize,
}

#[derive(Args)]
struct WireArgs {
    /// `tcp` or `unix`.
    #[arg(long, default_value = "tcp")]
    transport: String,
    /// The daemon leg's listen address to dial (`host:port` or a unix path).
    #[arg(long)]
    address: String,
    /// Channel identity to announce in our hello (repeatable).
    #[arg(long = "announce")]
    announce: Vec<String>,
    /// The wire version to claim (default the real one). `--hello-version 999`
    /// drives the §9 clause-6 version-mismatch refusal.
    #[arg(long, default_value_t = WIRE_VERSION)]
    hello_version: u16,
    /// Send a hello with a wrong magic number (a not-our-protocol peer).
    #[arg(long)]
    bad_magic: bool,
    /// The capability bitset to advertise.
    #[arg(long, default_value_t = 0)]
    capabilities: u32,
    /// After the handshake, send a frame whose length prefix exceeds the maximum
    /// (§9 clause 4) — the daemon must refuse cleanly.
    #[arg(long)]
    oversize_frame: bool,
    /// After the handshake, send a frame with an unknown type byte — the daemon
    /// must refuse cleanly (§9 clause 6).
    #[arg(long)]
    unknown_type: bool,
    /// Echo every targetward `data` frame back hostward on the same channel — a
    /// device stand-in for a single-daemon round-trip through a leg.
    #[arg(long)]
    echo: bool,
    /// After the handshake, stream sustained seeded hostward data on the announced
    /// channels while NEVER reading the socket — the peer's targetward backs up
    /// into the socket buffer (the §9 head-of-line stall), yet hostward keeps
    /// flowing. Holds for `--hold-ms`.
    #[arg(long)]
    stall: bool,
    /// Send `<channel>=<size>` seeded hostward data after the handshake
    /// (repeatable), e.g. `--send c0=64KiB`.
    #[arg(long = "send")]
    send: Vec<String>,
    #[arg(long, default_value_t = 0)]
    seed: u64,
    /// Stay connected this long (ms) after the scripted actions, so the harness can
    /// inspect leg state and drain the data path.
    #[arg(long, default_value_t = 1_500)]
    hold_ms: u64,
    #[arg(long, default_value_t = 10_000)]
    timeout_ms: u64,
}

#[derive(Args)]
struct TcpProxyArgs {
    /// Bind here; the `connect`-role daemon dials this (`host:port`).
    #[arg(long)]
    listen: String,
    /// Dial here; the `listen`-role daemon is bound here (`host:port`).
    #[arg(long)]
    connect: String,
    /// Sever the link after forwarding this many bytes from the dialing daemon,
    /// injecting an outage (e.g. `256KiB`).
    #[arg(long)]
    drop_after: Option<String>,
    /// After severing, wait this long (ms) before re-establishing the link.
    #[arg(long, default_value_t = 500)]
    restore_after_ms: u64,
    #[arg(long, default_value_t = 30_000)]
    timeout_ms: u64,
}

fn main() {
    let cli = Cli::parse();
    let verdict = match cli.mode {
        Mode::Pty(a) => run_pty(a),
        Mode::Client(a) => run_client(a),
        Mode::Mux(a) => run_mux(a),
        Mode::Envelope(a) => run_envelope(a),
        Mode::ExecConformance(a) => run_exec_conformance(a),
        Mode::Wire(a) => run_wire(a),
        Mode::TcpProxy(a) => run_tcp_proxy(a),
        Mode::NullModem(a) => run_nullmodem(a),
    };
    println!("{verdict}");
    let pass = verdict
        .get("pass")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    std::process::exit(if pass { 0 } else { 1 });
}

// --- shared helpers --------------------------------------------------------

/// Deterministic byte stream from a seed (splitmix64). Source and sink agree on
/// the stream from the same seed, so "no bytes lost/duplicated/reordered" is a
/// checksum comparison, not a judgment call (§3).
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

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Parse a human size like `1MiB`, `64KiB`, `10M`, `512`.
fn parse_size(s: &str) -> anyhow::Result<usize> {
    let s = s.trim();
    const UNITS: &[(&str, u64)] = &[
        ("GiB", 1 << 30),
        ("MiB", 1 << 20),
        ("KiB", 1 << 10),
        ("G", 1_000_000_000),
        ("M", 1_000_000),
        ("K", 1_000),
        ("B", 1),
    ];
    for (suffix, mult) in UNITS {
        if let Some(num) = s.strip_suffix(suffix) {
            let n: u64 = num.trim().parse()?;
            return Ok((n * mult) as usize);
        }
    }
    Ok(s.parse::<u64>()? as usize)
}

fn err_verdict(mode: &str, e: &anyhow::Error) -> Value {
    json!({"tool": "nexus-sim", "mode": mode, "error": e.to_string(), "pass": false})
}

/// Set a fd to raw termios (no echo, no translation) — binary-transparent. Only the
/// Linux `apply_raw_pair` uses it (BSD/macOS leaves termios to the consumer).
#[cfg(target_os = "linux")]
fn set_raw<F: AsFd>(fd: &F) -> anyhow::Result<()> {
    set_raw_baud(fd, None)
}

/// Apply raw termios to a freshly-created pty pair per platform: Linux through the
/// **master** (a terminal there); BSD/macOS through a momentarily-opened **slave**
/// (the master is not a terminal — `tc*attr` → `ENOTTY`). Mirrors the daemon's PTY
/// node (`nodes::pty::with_termios_fd`). On BSD the slave's termios resets on
/// last-close, but the consumer (the daemon's serial node, or a client) reconfigures
/// on open; the double only needs a clean, non-faulting setup.
#[cfg(target_os = "linux")]
fn apply_raw_pair(master: &PtyMaster, _pts: &str) -> anyhow::Result<()> {
    set_raw(master)
}
#[cfg(not(target_os = "linux"))]
fn apply_raw_pair(_master: &PtyMaster, _pts: &str) -> anyhow::Result<()> {
    // BSD/macOS: the master is not a terminal (ENOTTY), and opening the slave to set
    // termios would prime POLLHUP on the master — which the echo/source/sink loops
    // read as "client hung up" and exit early. Leave the pair at its default termios
    // and let the CONSUMER configure it: the daemon's serial node opens the slave
    // through serial2 (raw 8N1), and the sim `client` sets raw on its own slave fd.
    // A never-opened master does not HUP here, so the loops wait correctly meanwhile.
    Ok(())
}

/// Read a pty pair's termios per platform (master on Linux, a slave open on BSD).
#[cfg(target_os = "linux")]
fn termios_of_pair(master: &PtyMaster, _pts: &str) -> anyhow::Result<nix::sys::termios::Termios> {
    Ok(tcgetattr(master)?)
}
#[cfg(not(target_os = "linux"))]
fn termios_of_pair(_master: &PtyMaster, pts: &str) -> anyhow::Result<nix::sys::termios::Termios> {
    Ok(tcgetattr(&open_slave(pts)?)?)
}

/// Open the pty slave read-write (`O_NOCTTY`) for BSD/macOS termios operations.
#[cfg(not(target_os = "linux"))]
fn open_slave(pts: &str) -> anyhow::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    Ok(std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(nix::libc::O_NOCTTY)
        .open(pts)?)
}

/// Raw termios with an optional standard baud, applied in one `tcsetattr` so the
/// daemon's packet-mode observation sees a single change (§7.2).
fn set_raw_baud<F: AsFd>(fd: &F, baud: Option<u32>) -> anyhow::Result<()> {
    let mut t = tcgetattr(fd)?;
    cfmakeraw(&mut t);
    t.local_flags.remove(LocalFlags::ECHO);
    if let Some(rate) = baud {
        let br = baud_rate(rate)
            .ok_or_else(|| anyhow::anyhow!("--set-baud {rate} is not a standard rate"))?;
        cfsetspeed(&mut t, br)?;
    }
    tcsetattr(fd, SetArg::TCSANOW, &t)?;
    Ok(())
}

/// Map a numeric baud to a standard `BaudRate` (the rates a PTY can carry).
fn baud_rate(rate: u32) -> Option<BaudRate> {
    Some(match rate {
        9600 => BaudRate::B9600,
        19200 => BaudRate::B19200,
        38400 => BaudRate::B38400,
        57600 => BaudRate::B57600,
        115200 => BaudRate::B115200,
        230400 => BaudRate::B230400,
        _ => return None,
    })
}

/// Poll a fd for readability (or hangup) up to `ms`. Returns the revents.
fn wait_readable<F: AsFd>(fd: &F, ms: u16) -> anyhow::Result<PollFlags> {
    let borrowed = fd.as_fd();
    let mut fds = [PollFd::new(
        borrowed,
        PollFlags::POLLIN | PollFlags::POLLHUP,
    )];
    let n = poll(&mut fds, PollTimeout::from(ms))?;
    if n == 0 {
        Ok(PollFlags::empty())
    } else {
        Ok(fds[0].revents().unwrap_or_else(PollFlags::empty))
    }
}

fn is_eio(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(nix::libc::EIO)
}

// --- pty mode --------------------------------------------------------------

fn run_pty(a: PtyArgs) -> Value {
    match run_pty_inner(&a) {
        Ok(v) => v,
        Err(e) => {
            if let Some(link) = &a.link {
                let _ = std::fs::remove_file(link);
            }
            err_verdict("pty", &e)
        }
    }
}

fn run_pty_inner(a: &PtyArgs) -> anyhow::Result<Value> {
    let mut master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY)?;
    grantpt(&master)?;
    unlockpt(&master)?;
    let pts = ptsname(&master)?;

    // Raw baseline on the pair. On Linux this goes through the master (so we never
    // open the slave ourselves — per S2, opening+closing it would prime POLLHUP); on
    // BSD/macOS the master is not a terminal, so it goes through a momentary slave
    // open (see `apply_raw_pair`).
    apply_raw_pair(&master, &pts)?;

    if let Some(link) = &a.link {
        let _ = std::fs::remove_file(link);
        std::os::unix::fs::symlink(&pts, link)?;
    }

    let result = if a.echo {
        pty_echo(&mut master, a.timeout_ms)
    } else if a.source {
        let n = parse_size(a.bytes.as_deref().unwrap_or("0"))?;
        pty_source(
            &mut master,
            a.seed,
            n,
            a.rate,
            a.wait_file.as_deref(),
            a.timeout_ms,
        )
    } else if a.sink {
        let n = parse_size(a.bytes.as_deref().unwrap_or("0"))?;
        pty_sink(&mut master, n, a.timeout_ms)
    } else if a.report_termios {
        pty_report_termios(&master, &pts)
    } else if a.stall {
        pty_stall(&master, a.timeout_ms)
    } else {
        anyhow::bail!("pty: pick one of --echo/--source/--sink/--report-termios/--stall")
    };

    // Keep the master (and thus the pts) open so the device reads as still
    // "plugged in" after a finite source/sink completes — a real serial device
    // stays present when it stops transmitting; only an unplug closes it, which
    // the daemon now reads as faulted-and-wait (§7.1). Tests that want the unplug
    // simply omit --hold-ms (or kill the sim).
    if let Some(ms) = a.hold_ms {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }

    if let Some(link) = &a.link {
        let _ = std::fs::remove_file(link);
    }
    result
}

fn run_nullmodem(a: NullModemArgs) -> Value {
    let out = run_nullmodem_inner(&a);
    let _ = std::fs::remove_file(&a.link_a);
    let _ = std::fs::remove_file(&a.link_b);
    match out {
        Ok(v) => v,
        Err(e) => err_verdict("nullmodem", &e),
    }
}

/// Open a raw PTY master and publish its pts under `link`. Returns the master.
fn nullmodem_master(link: &std::path::Path) -> anyhow::Result<PtyMaster> {
    let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY)?;
    grantpt(&master)?;
    unlockpt(&master)?;
    let pts = ptsname(&master)?;
    apply_raw_pair(&master, &pts)?;
    let _ = std::fs::remove_file(link);
    std::os::unix::fs::symlink(&pts, link)?;
    Ok(master)
}

/// Bridge two PTY masters: bytes written to either pts slave are forwarded to the
/// other, so the two slaves behave as a cross-wired null modem (a device with no
/// hardware). Exits after `timeout_ms` of no traffic in either direction. Slave
/// closures are tolerated (a client may reopen), so a probe that opens, exchanges,
/// and closes each side leaves the bridge available for the next.
fn run_nullmodem_inner(a: &NullModemArgs) -> anyhow::Result<Value> {
    let mut ma = nullmodem_master(&a.link_a)?;
    let mut mb = nullmodem_master(&a.link_b)?;
    let mut a_to_b: u64 = 0;
    let mut b_to_a: u64 = 0;
    let mut buf = [0u8; 8192];
    let mut last_activity = Instant::now();

    while last_activity.elapsed() < Duration::from_millis(a.timeout_ms) {
        let mut fds = [
            PollFd::new(ma.as_fd(), PollFlags::POLLIN),
            PollFd::new(mb.as_fd(), PollFlags::POLLIN),
        ];
        // A short poll timeout bounds how quickly the idle-exit check fires.
        let _ = poll(&mut fds, PollTimeout::from(100u16));
        let a_ready = fds[0]
            .revents()
            .map(|r| r.contains(PollFlags::POLLIN))
            .unwrap_or(false);
        let b_ready = fds[1]
            .revents()
            .map(|r| r.contains(PollFlags::POLLIN))
            .unwrap_or(false);

        if a_ready {
            match ma.read(&mut buf) {
                Ok(0) => {}
                Ok(n) => {
                    // A slave may not be open; ignore a transient write error.
                    let _ = mb.write_all(&buf[..n]);
                    let _ = mb.flush();
                    a_to_b += n as u64;
                    last_activity = Instant::now();
                }
                Err(e) if is_eio(&e) => {} // slave (client) closed; tolerate
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.into()),
            }
        }
        if b_ready {
            match mb.read(&mut buf) {
                Ok(0) => {}
                Ok(n) => {
                    let _ = ma.write_all(&buf[..n]);
                    let _ = ma.flush();
                    b_to_a += n as u64;
                    last_activity = Instant::now();
                }
                Err(e) if is_eio(&e) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.into()),
            }
        }
    }

    Ok(json!({
        "tool": "nexus-sim",
        "mode": "nullmodem",
        "pass": true,
        "a_to_b": a_to_b,
        "b_to_a": b_to_a,
    }))
}

fn pty_echo(master: &mut PtyMaster, timeout_ms: u64) -> anyhow::Result<Value> {
    let mut echoed: u64 = 0;
    let mut buf = [0u8; 8192];
    loop {
        let re = wait_readable(master, timeout_ms.min(u16::MAX as u64) as u16)?;
        if re.is_empty() {
            break; // idle past the timeout — client done or never arrived
        }
        if re.contains(PollFlags::POLLIN) {
            match master.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    master.write_all(&buf[..n])?;
                    echoed += n as u64;
                }
                Err(e) if is_eio(&e) => break, // slave (client) closed
                Err(e) => return Err(e.into()),
            }
        } else if re.contains(PollFlags::POLLHUP) {
            break;
        }
    }
    Ok(
        json!({"tool": "nexus-sim", "mode": "pty", "behavior": "echo", "bytes_echoed": echoed, "pass": true}),
    )
}

fn pty_source(
    master: &mut PtyMaster,
    seed: u64,
    n: usize,
    rate: Option<u64>,
    wait_file: Option<&std::path::Path>,
    timeout_ms: u64,
) -> anyhow::Result<Value> {
    // Gate on the harness's readiness file so the payload never outruns a
    // not-yet-draining consumer (plan §3, presence != readiness).
    if let Some(gate) = wait_file {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        while !gate.exists() {
            if Instant::now() >= deadline {
                anyhow::bail!("wait-file {} never appeared", gate.display());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    let payload = seeded_bytes(seed, n);
    match rate.filter(|r| *r > 0) {
        // Unpaced: one write_all at line rate.
        None => master.write_all(&payload)?,
        // Paced: write in blocks, sleeping to hold the overall byte rate.
        Some(bps) => {
            let start = Instant::now();
            let block = 65536.min(payload.len().max(1));
            let mut written = 0usize;
            while written < payload.len() {
                let end = (written + block).min(payload.len());
                master.write_all(&payload[written..end])?;
                written = end;
                let expected = Duration::from_secs_f64(written as f64 / bps as f64);
                if start.elapsed() < expected {
                    thread::sleep(expected - start.elapsed());
                }
            }
        }
    }
    master.flush()?;
    Ok(json!({
        "tool": "nexus-sim", "mode": "pty", "behavior": "source",
        "sent": n, "seed": seed, "sha256": sha256_hex(&payload), "pass": true
    }))
}

fn pty_sink(master: &mut PtyMaster, n: usize, timeout_ms: u64) -> anyhow::Result<Value> {
    let got = read_until(master, n, timeout_ms)?;
    Ok(json!({
        "tool": "nexus-sim", "mode": "pty", "behavior": "sink",
        "received": got.len(), "sha256": sha256_hex(&got), "pass": got.len() == n
    }))
}

/// Hold the master open without ever reading it: the targetward (master input)
/// buffer fills and the daemon's writer backpressures — a stalled target for the
/// §9 head-of-line-blocking property. Stays present (no HUP) until the timeout.
fn pty_stall(_master: &PtyMaster, timeout_ms: u64) -> anyhow::Result<Value> {
    // Hold the master fd open (present) but never read it, so the targetward input
    // buffer fills and stays full; just sleep until the timeout.
    thread::sleep(Duration::from_millis(timeout_ms));
    Ok(json!({"tool": "nexus-sim", "mode": "pty", "behavior": "stall", "pass": true}))
}

fn pty_report_termios(master: &PtyMaster, pts: &str) -> anyhow::Result<Value> {
    let t = termios_of_pair(master, pts)?;
    // On Linux nix reports the output speed as a `BaudRate` enum of standard
    // rates (e.g. `B115200`); its debug form is stable and comparable, and baud
    // is cosmetic on a PTY anyway (§7.2).
    let baud = format!("{:?}", cfgetospeed(&t));
    Ok(json!({
        "tool": "nexus-sim", "mode": "pty", "behavior": "report-termios",
        "baud": baud,
        "echo": t.local_flags.contains(LocalFlags::ECHO),
        "icanon": t.local_flags.contains(LocalFlags::ICANON),
        "extproc": t.local_flags.contains(LocalFlags::EXTPROC),
        "opost": t.output_flags.contains(nix::sys::termios::OutputFlags::OPOST),
        "pass": true
    }))
}

// --- client mode -----------------------------------------------------------

fn run_client(a: ClientArgs) -> Value {
    match run_client_inner(&a) {
        Ok(v) => v,
        Err(e) => err_verdict("client", &e),
    }
}

fn run_client_inner(a: &ClientArgs) -> anyhow::Result<Value> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(nix::libc::O_NOCTTY)
        .open(&a.path)?;

    // Observe-only: read the termios the daemon set on the pair, without the
    // set_raw below that would overwrite it.
    if a.report_termios {
        let t = tcgetattr(&file)?;
        return Ok(json!({
            "tool": "nexus-sim", "mode": "client", "behavior": "report-termios",
            "baud": format!("{:?}", cfgetospeed(&t)),
            "echo": t.local_flags.contains(LocalFlags::ECHO),
            "icanon": t.local_flags.contains(LocalFlags::ICANON),
            "extproc": t.local_flags.contains(LocalFlags::EXTPROC),
            "opost": t.output_flags.contains(nix::sys::termios::OutputFlags::OPOST),
            "pass": true
        }));
    }
    set_raw_baud(&file, a.set_baud)?;

    // Receive-only modes: a fixed-size sink (`--recv`) or a fully-draining reader
    // (`--drain`). Neither sends; both keep the slave open until done.
    if a.recv.is_some() || a.drain {
        let target = match a.recv.as_deref() {
            Some(s) => Some(parse_size(s)?),
            None => None,
        };
        let (received, sha) = recv_loop(
            &file,
            target,
            a.read_rate,
            if a.drain {
                Some(a.quiet_ms.unwrap_or(1000))
            } else {
                None
            },
            a.timeout_ms,
            a.skip,
            a.ready_file.as_deref(),
        )?;
        let pass = target.is_none_or(|t| received as usize == t);
        return Ok(json!({
            "tool": "nexus-sim", "mode": "client",
            "behavior": if a.drain { "drain" } else { "recv" },
            "received": received, "sha256": sha, "pass": pass
        }));
    }

    let payload = match a.send.as_deref() {
        Some(s) => {
            let size = s
                .strip_prefix("seeded:")
                .ok_or_else(|| anyhow::anyhow!("--send expects seeded:SIZE"))?;
            seeded_bytes(a.seed, parse_size(size)?)
        }
        None => Vec::new(),
    };
    let n = payload.len();

    // Read echoes concurrently with writing, so a full-duplex path never
    // deadlocks against a bounded PTY buffer.
    let reader = file.try_clone()?;
    let expect_echo = a.expect == "echo";
    let want = if expect_echo { n } else { 0 };
    let timeout_ms = a.timeout_ms;
    let read_handle = thread::spawn(move || read_until(&reader, want, timeout_ms));

    {
        let mut w = &file;
        w.write_all(&payload)?;
        w.flush()?;
    }

    let received = read_handle
        .join()
        .map_err(|_| anyhow::anyhow!("reader thread panicked"))??;

    // Keep the slave open so a subscriber can observe our presence/termios.
    if let Some(ms) = a.hold_ms {
        thread::sleep(Duration::from_millis(ms));
    }

    let sent_hash = sha256_hex(&payload);
    let recv_hash = sha256_hex(&received);
    let pass = if expect_echo {
        received.len() == n && sent_hash == recv_hash
    } else {
        true
    };

    Ok(json!({
        "tool": "nexus-sim", "mode": "client",
        "sent": n, "received": received.len(),
        "sha256_sent": sent_hash, "sha256_received": recv_hash,
        "expect": a.expect, "pass": pass
    }))
}

/// Read from a fd until `n` bytes are collected or the timeout elapses. `n == 0`
/// returns immediately with nothing (used when no echo is expected).
fn read_until<F: AsFd + Read>(mut fd: F, n: usize, timeout_ms: u64) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(n);
    if n == 0 {
        return Ok(out);
    }
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut buf = [0u8; 8192];
    while out.len() < n {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            break;
        };
        let ms = remaining.as_millis().min(1000) as u16;
        let re = wait_readable(&fd, ms)?;
        if re.contains(PollFlags::POLLIN) {
            match fd.read(&mut buf) {
                Ok(0) => break,
                Ok(k) => out.extend_from_slice(&buf[..k]),
                Err(e) if is_eio(&e) => break,
                Err(e) => return Err(e.into()),
            }
        } else if re.contains(PollFlags::POLLHUP) {
            // Drain any last readable bytes, then stop.
            if !re.contains(PollFlags::POLLIN) {
                break;
            }
        }
    }
    Ok(out)
}

/// Receive hostward bytes, counting and checksumming them incrementally (so a
/// multi-hundred-MiB firehose never buffers in the sink). Stops on a byte
/// `target` (the fixed-size sink) or, when `quiet_ms` is set, after that long
/// with no new bytes (the fully-draining reader — draining to quiet guarantees
/// no daemon-delivered byte is left unread, which is what makes drop accounting
/// exact). `read_rate` paces overall throughput to model a slow consumer.
fn recv_loop<F: AsFd + Read>(
    mut fd: F,
    target: Option<usize>,
    read_rate: Option<u64>,
    quiet_ms: Option<u64>,
    timeout_ms: u64,
    skip: usize,
    ready_file: Option<&Path>,
) -> anyhow::Result<(u64, String)> {
    let start = Instant::now();
    let deadline = start + Duration::from_millis(timeout_ms);
    // `received`/`hasher` cover the PAYLOAD only; the leading `to_skip` primer bytes
    // are read and discarded (the presence-vs-readiness handshake, plan §3).
    let mut received: u64 = 0;
    let mut to_skip = skip;
    let mut ready_signalled = false;
    let mut hasher = Sha256::new();
    // A generous read buffer so a fast, well-buffered hostward stream drains in few
    // syscalls — under heavy CPU contention a per-frame (4 KiB) read cadence is
    // scheduling-bound, so grabbing up to 64 KiB per read keeps a demux burst moving.
    let mut buf = vec![0u8; 64 * 1024];
    let mut last_data = Instant::now();
    loop {
        if target.is_some_and(|t| received as usize >= t) {
            break;
        }
        if Instant::now() >= deadline {
            break;
        }
        if quiet_ms.is_some_and(|q| last_data.elapsed() >= Duration::from_millis(q)) {
            break;
        }
        // Throttle: hold overall throughput at or below `read_rate`.
        if let Some(rate) = read_rate.filter(|r| *r > 0) {
            let expected = Duration::from_secs_f64(received as f64 / rate as f64);
            if start.elapsed() < expected {
                thread::sleep(expected - start.elapsed());
            }
        }
        // Short poll timeout so the quiet/deadline checks keep making progress.
        let re = wait_readable(&fd, 200)?;
        if re.contains(PollFlags::POLLIN) {
            // While still skipping the primer, read a full buffer (we must consume
            // the whole primer, which may straddle a read into the payload); once
            // past it, never overshoot the target.
            let cap = if to_skip > 0 {
                buf.len()
            } else {
                target.map_or(buf.len(), |t| (t - received as usize).min(buf.len()))
            };
            match fd.read(&mut buf[..cap]) {
                Ok(0) => break,
                Ok(k) => {
                    // The first byte back proves the read loop is live and draining —
                    // signal readiness so the harness can release the payload burst.
                    if !ready_signalled {
                        if let Some(rf) = ready_file {
                            std::fs::File::create(rf)?;
                        }
                        ready_signalled = true;
                    }
                    // Discard up to `to_skip` primer bytes; the remainder is payload.
                    let d = to_skip.min(k);
                    to_skip -= d;
                    let payload = &buf[d..k];
                    let take = payload
                        .len()
                        .min(target.map_or(usize::MAX, |t| t - received as usize));
                    hasher.update(&payload[..take]);
                    received += take as u64;
                    last_data = Instant::now();
                }
                Err(e) if is_eio(&e) => break,
                Err(e) => return Err(e.into()),
            }
        } else if re.contains(PollFlags::POLLHUP) {
            break;
        }
    }
    Ok((
        received,
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    ))
}

// --- mux mode --------------------------------------------------------------

/// One channel's slice of the manifest: how many bytes the demux node should
/// deliver on it (the source stream minus any corrupted frames' payloads) and the
/// checksum of exactly those bytes — the oracle a channel client checks against.
struct ChannelManifest {
    id: String,
    delivered: u64,
    sha256: String,
}

/// The full framed stream plus the manifest it satisfies. Built deterministically
/// from `(seed, channels, bytes, corrupt_every, frame_size)`, so `--manifest`
/// (pure) and the feed path agree byte for byte.
struct MuxPlan {
    wire: Vec<u8>,
    channels: Vec<ChannelManifest>,
    frames: u64,
    corrupted: u64,
}

/// Round-robin seeded per-channel data into reference-framed envelope frames,
/// corrupting one in every `corrupt_every` frames (0 = none) by mangling its type
/// byte while leaving the length prefix intact — so the demux codec resyncs by
/// frame length and drops exactly the corrupt frame (§7.5). Tracks per channel the
/// bytes that survive (are delivered) and their checksum.
fn build_mux(
    channels: &[String],
    bytes_per_channel: usize,
    seed: u64,
    corrupt_every: u64,
    frame_size: usize,
) -> anyhow::Result<MuxPlan> {
    if channels.is_empty() {
        anyhow::bail!("mux: at least one --channel is required");
    }
    let frame_size = frame_size.max(1);
    let streams: Vec<Vec<u8>> = (0..channels.len())
        .map(|i| seeded_bytes(seed.wrapping_add(i as u64), bytes_per_channel))
        .collect();
    let mut cursors = vec![0usize; channels.len()];
    let mut delivered = vec![Vec::<u8>::new(); channels.len()];
    let mut wire = Vec::new();
    let mut frame_no: u64 = 0;
    let mut corrupted: u64 = 0;

    // Emit one frame per channel per round until every stream is exhausted.
    loop {
        let mut any = false;
        for (i, chan) in channels.iter().enumerate() {
            if cursors[i] >= streams[i].len() {
                continue;
            }
            any = true;
            let end = (cursors[i] + frame_size).min(streams[i].len());
            let payload = streams[i][cursors[i]..end].to_vec();
            cursors[i] = end;

            let frame_start = wire.len();
            let ev = Event::data(chan.as_str(), Bytes::from(payload.clone()));
            encode(&ev, &mut wire).map_err(|e| anyhow::anyhow!("encode: {e}"))?;

            if corrupt_every > 0 && (frame_no + 1).is_multiple_of(corrupt_every) {
                // The type byte is the first body byte, at frame_start + 4.
                wire[frame_start + 4] = 0xFF;
                corrupted += 1;
            } else {
                delivered[i].extend_from_slice(&payload);
            }
            frame_no += 1;
        }
        if !any {
            break;
        }
    }

    let channels = channels
        .iter()
        .enumerate()
        .map(|(i, id)| ChannelManifest {
            id: id.clone(),
            delivered: delivered[i].len() as u64,
            sha256: sha256_hex(&delivered[i]),
        })
        .collect();
    Ok(MuxPlan {
        wire,
        channels,
        frames: frame_no,
        corrupted,
    })
}

/// Poll until `path` exists or `deadline` passes (the feed handshake gates, §3).
fn wait_for_file(path: &Path, deadline: Instant) -> anyhow::Result<()> {
    while !path.exists() {
        if Instant::now() >= deadline {
            anyhow::bail!("wait-file {} never appeared", path.display());
        }
        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

fn manifest_json(plan: &MuxPlan, behavior: &str, extra: Value) -> Value {
    let channels: Vec<Value> = plan
        .channels
        .iter()
        .map(|c| json!({"id": c.id, "delivered": c.delivered, "sha256": c.sha256}))
        .collect();
    let mut obj = json!({
        "tool": "nexus-sim", "mode": "mux", "behavior": behavior,
        "channels": channels, "frames": plan.frames, "corrupted": plan.corrupted,
        "pass": true,
    });
    if let (Value::Object(o), Value::Object(e)) = (&mut obj, extra) {
        o.extend(e);
    }
    obj
}

fn run_mux(a: MuxArgs) -> Value {
    match run_mux_inner(&a) {
        Ok(v) => v,
        Err(e) => {
            if let Some(link) = &a.link {
                let _ = std::fs::remove_file(link);
            }
            err_verdict("mux", &e)
        }
    }
}

fn run_mux_inner(a: &MuxArgs) -> anyhow::Result<Value> {
    let bytes_per_channel = parse_size(&a.bytes)?;
    let plan = build_mux(
        &a.channels,
        bytes_per_channel,
        a.seed,
        a.corrupt_every,
        a.frame_size,
    )?;

    // Manifest mode: print the oracle and exit, without a PTY.
    if a.manifest {
        return Ok(manifest_json(&plan, "manifest", json!({})));
    }

    // Feed mode: create the PTY, write the framed stream, hold the device present.
    let link = a
        .link
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("mux feed needs --link (or use --manifest)"))?;
    let mut master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY)?;
    grantpt(&master)?;
    unlockpt(&master)?;
    let pts = ptsname(&master)?;
    apply_raw_pair(&master, &pts)?;
    let _ = std::fs::remove_file(link);
    std::os::unix::fs::symlink(&pts, link)?;

    // Two-phase feed handshake (plan §3 — presence is not readiness). When a primer
    // is configured, first wait for the clients to be present (`--prime-file`) and
    // send a small primer per channel: small enough that a present-but-not-yet-
    // draining PTY buffers rather than drops it, so every client reads a byte and
    // proves it is draining (creating its `--ready-file`). Only then does the harness
    // release the real burst via `--wait-file`, which can no longer outrun a reader
    // that is already parked in its read loop.
    if a.prime_bytes > 0 {
        let deadline = Instant::now() + Duration::from_millis(a.timeout_ms);
        if let Some(prime) = &a.prime_file {
            wait_for_file(prime, deadline)?;
        }
        let mut primer = Vec::new();
        for ch in &a.channels {
            let ev = Event::data(ch.as_str(), Bytes::from(vec![0u8; a.prime_bytes]));
            encode(&ev, &mut primer).map_err(|e| anyhow::anyhow!("encode primer: {e}"))?;
        }
        master.write_all(&primer)?;
        master.flush()?;
    }
    if let Some(go) = &a.wait_file {
        let deadline = Instant::now() + Duration::from_millis(a.timeout_ms);
        wait_for_file(go, deadline)?;
    }

    // Write the whole framed stream. A blocking master write backpressures against
    // the daemon's serial reader, so this returns only once the daemon has drained
    // all but the last kernel-buffer-full.
    master.write_all(&plan.wire)?;
    master.flush()?;

    // Hold the device present (draining and discarding any targetward bytes so the
    // daemon's serial writer never blocks) until the timeout, then unlink.
    let deadline = Instant::now() + Duration::from_millis(a.timeout_ms);
    let mut buf = [0u8; 8192];
    while Instant::now() < deadline {
        let re = wait_readable(&master, 200)?;
        if re.contains(PollFlags::POLLIN) {
            match master.read(&mut buf) {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) if is_eio(&e) => break,
                Err(e) => return Err(e.into()),
            }
        } else if re.contains(PollFlags::POLLHUP) {
            break;
        }
    }
    let _ = std::fs::remove_file(link);
    Ok(manifest_json(
        &plan,
        "feed",
        json!({"bytes_written": plan.wire.len()}),
    ))
}

// --- envelope mode ---------------------------------------------------------

/// The golden-vector battery: every event kind plus edge cases (empty payload,
/// binary payload, a long channel id, back-to-back frames on one channel). A
/// conforming child re-emits exactly this sequence.
fn golden_battery() -> Vec<Event> {
    vec![
        Event::open("console"),
        Event::data("console", Bytes::from_static(b"hi")),
        Event::data("console", Bytes::from_static(b"")),
        Event::data("bin", Bytes::from_static(b"\x00\x01\xff\xfe binary\n")),
        Event::data("a-long-channel-identity-name", Bytes::from_static(b"x")),
        Event::error("c0", "framing error: resync"),
        Event::close("console"),
        Event::data("t", Bytes::from_static(b"1")),
        Event::data("t", Bytes::from_static(b"22")),
        Event::data("t", Bytes::from_static(b"333")),
    ]
}

fn run_envelope(a: EnvelopeArgs) -> Value {
    match run_envelope_inner(&a) {
        Ok(v) => v,
        Err(e) => err_verdict("envelope", &e),
    }
}

fn run_envelope_inner(a: &EnvelopeArgs) -> anyhow::Result<Value> {
    let battery = golden_battery();
    let mut input = Vec::new();
    for ev in &battery {
        encode(ev, &mut input).map_err(|e| anyhow::anyhow!("encode: {e}"))?;
    }

    // Drive the child as a shell command so `--exec "python3 x.py"` works verbatim.
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&a.exec)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| anyhow::anyhow!("spawn {:?}: {e}", a.exec))?;

    // Feed stdin from a thread and close it (EOF), so a child that writes as it
    // reads cannot deadlock against a full stdout pipe.
    let mut stdin = child.stdin.take().expect("piped stdin");
    let writer = thread::spawn(move || {
        let _ = stdin.write_all(&input);
        // Dropping `stdin` here closes the child's stdin.
    });
    let mut stdout = child.stdout.take().expect("piped stdout");

    // Read stdout to EOF on a thread, bounded by the timeout.
    let (tx, rx) = std_mpsc::channel();
    thread::spawn(move || {
        let mut out = Vec::new();
        let res = stdout.read_to_end(&mut out).map(|_| out);
        let _ = tx.send(res);
    });
    let output = match rx.recv_timeout(Duration::from_millis(a.timeout_ms)) {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            let _ = child.kill();
            anyhow::bail!("reading child stdout: {e}");
        }
        Err(_) => {
            let _ = child.kill();
            anyhow::bail!("child did not complete within {} ms", a.timeout_ms);
        }
    };
    let _ = writer.join();
    let _ = child.wait();

    // Decode what the child re-emitted and compare to the battery.
    let mut dec = FrameDecoder::new();
    dec.push(&output);
    let mut got = Vec::new();
    loop {
        match dec.next_event() {
            Ok(Some(ev)) => got.push(ev),
            Ok(None) => break,
            Err(e) => anyhow::bail!("child emitted a malformed frame: {e}"),
        }
    }
    let trailing = dec.buffered();
    let pass = got == battery && trailing == 0;
    Ok(json!({
        "tool": "nexus-sim", "mode": "envelope",
        "sent_frames": battery.len(), "received_frames": got.len(),
        "trailing_bytes": trailing, "pass": pass,
    }))
}

// --- exec conformance (§15.26 / plan §10.5) --------------------------------

/// A spawned external codec child, with concurrent stdin feeding and a
/// frame-decoded stdout stream — the harness's half of the §7.6 child-pipe
/// boundary. stdout is drained on a thread and decoded as frames arrive, so a
/// blocked stdin write can never starve stdout (the very coupling §15.22 forbids);
/// requiring the *child* to interleave is what the liveness check probes.
struct ExecChild {
    child: std::process::Child,
    stdin: Option<std::process::ChildStdin>,
    frames: std_mpsc::Receiver<FrameMsg>,
}

/// One decoded item from the child's stdout stream (or a terminal condition).
enum FrameMsg {
    Event(Event),
    Malformed(String),
    Closed,
}

/// The outcome of waiting for one echoed frame.
enum Recv {
    Event(Event),
    Timeout,
    Closed,
    Malformed,
}

impl ExecChild {
    fn spawn(exec: &str) -> anyhow::Result<ExecChild> {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(exec)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| anyhow::anyhow!("spawn {exec:?}: {e}"))?;
        let stdin = child.stdin.take().expect("piped stdin");
        let mut stdout = child.stdout.take().expect("piped stdout");
        let (tx, rx) = std_mpsc::channel();
        thread::spawn(move || {
            let mut dec = FrameDecoder::new();
            let mut buf = [0u8; 8192];
            loop {
                match stdout.read(&mut buf) {
                    Ok(0) => {
                        let _ = tx.send(FrameMsg::Closed);
                        break;
                    }
                    Ok(n) => {
                        dec.push(&buf[..n]);
                        loop {
                            match dec.next_event() {
                                Ok(Some(ev)) => {
                                    if tx.send(FrameMsg::Event(ev)).is_err() {
                                        return;
                                    }
                                }
                                Ok(None) => break,
                                Err(e) => {
                                    let _ = tx.send(FrameMsg::Malformed(e.to_string()));
                                    return;
                                }
                            }
                        }
                    }
                    Err(_) => {
                        let _ = tx.send(FrameMsg::Closed);
                        break;
                    }
                }
            }
        });
        Ok(ExecChild {
            child,
            stdin: Some(stdin),
            frames: rx,
        })
    }

    /// Encode and write one event frame to the child's stdin, flushed immediately.
    fn send(&mut self, event: &Event) -> anyhow::Result<()> {
        let mut buf = Vec::new();
        encode(event, &mut buf).map_err(|e| anyhow::anyhow!("encode: {e}"))?;
        self.write_raw(&buf)
    }

    /// Write raw bytes to the child's stdin (used to deliver a frame in pieces).
    fn write_raw(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("stdin already closed"))?;
        stdin.write_all(bytes)?;
        stdin.flush()?;
        Ok(())
    }

    /// Close the child's stdin (EOF), so a child that batches until end-of-input
    /// flushes. Idempotent.
    fn close_stdin(&mut self) {
        self.stdin = None;
    }

    /// Wait for the next echoed frame, bounded by `timeout`.
    fn recv(&self, timeout: Duration) -> Recv {
        match self.frames.recv_timeout(timeout) {
            Ok(FrameMsg::Event(ev)) => Recv::Event(ev),
            Ok(FrameMsg::Malformed(reason)) => {
                eprintln!("exec-conformance: child emitted a malformed frame: {reason}");
                Recv::Malformed
            }
            Ok(FrameMsg::Closed) => Recv::Closed,
            Err(std_mpsc::RecvTimeoutError::Timeout) => Recv::Timeout,
            Err(std_mpsc::RecvTimeoutError::Disconnected) => Recv::Closed,
        }
    }
}

impl Drop for ExecChild {
    fn drop(&mut self) {
        // Kill and reap: the harness owns the child's lifetime, and the
        // kill-and-restart check depends on a killed child actually dying.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn run_exec_conformance(a: ExecConformanceArgs) -> Value {
    match run_exec_conformance_inner(&a) {
        Ok(v) => v,
        Err(e) => err_verdict("exec-conformance", &e),
    }
}

fn run_exec_conformance_inner(a: &ExecConformanceArgs) -> anyhow::Result<Value> {
    let per_frame = Duration::from_millis(a.frame_timeout_ms);
    let golden = check_golden(&a.exec, per_frame)?;
    let liveness = check_liveness(&a.exec, per_frame, a.liveness_frames)?;
    let fragmentation = check_fragmentation(&a.exec, per_frame)?;
    let restart = check_restart(&a.exec, per_frame)?;
    let pass = golden && liveness && fragmentation && restart;
    Ok(json!({
        "tool": "nexus-sim", "mode": "exec-conformance",
        "checks": {
            "golden": golden,
            "liveness": liveness,
            "fragmentation": fragmentation,
            "restart": restart,
        },
        "pass": pass,
    }))
}

/// Golden vectors: the child re-emits the whole battery byte-for-byte (correctness,
/// independent of timing — stdin is closed so a batching child still flushes).
fn check_golden(exec: &str, timeout: Duration) -> anyhow::Result<bool> {
    let battery = golden_battery();
    let mut child = ExecChild::spawn(exec)?;
    for ev in &battery {
        child.send(ev)?;
    }
    child.close_stdin();
    let mut got = Vec::new();
    loop {
        match child.recv(timeout) {
            Recv::Event(ev) => got.push(ev),
            Recv::Closed | Recv::Timeout => break,
            Recv::Malformed => return Ok(false),
        }
    }
    Ok(got == battery)
}

/// Full-duplex liveness (the §15.22 deadlock class). Sends a sustained pipeline of
/// N frames WITHOUT closing stdin, then requires that a healthy majority of echoes
/// flow back *before* EOF — proving the child interleaves read and write. A child
/// that reads all input before writing (the half-duplex antipattern) emits nothing
/// until EOF and fails this. A *correct* codec that lags its output a bounded few
/// frames behind its input still passes: it echoes continuously and the tail it
/// holds is drained after stdin closes. Only read-all-first / hoarding shapes fail
/// (§15.26; see docs/codec-authors.md). This is NOT a lock-step ping-pong — that
/// would false-fail any legitimately buffering codec.
fn check_liveness(exec: &str, per_frame: Duration, frames: usize) -> anyhow::Result<bool> {
    let mut child = ExecChild::spawn(exec)?;
    let sent: Vec<Event> = (0..frames)
        .map(|i| Event::data("live", Bytes::from(seeded_bytes(1000 + i as u64, 512))))
        .collect();
    for ev in &sent {
        child.send(ev)?;
    }

    // Phase 1 — interleaving proof: with stdin still open, collect whatever the
    // child emits. A full-duplex child echoes (most of) the pipeline; a half-duplex
    // one emits nothing until EOF (which has not come).
    let mut received: Vec<Event> = Vec::new();
    while received.len() < frames {
        match child.recv(per_frame) {
            Recv::Event(ev) => received.push(ev),
            _ => break, // nothing more arrives without EOF
        }
    }
    // Fewer than half the frames echoed before EOF ⇒ the child is not interleaving
    // (read-all-first, or hoarding more than a bounded lag). Liveness fails.
    if received.len() * 2 < frames {
        return Ok(false);
    }

    // Phase 2 — completeness: close stdin so a bounded-lag child flushes its held
    // tail, then drain the rest and require an exact, in-order match.
    child.close_stdin();
    while received.len() < frames {
        match child.recv(per_frame) {
            Recv::Event(ev) => received.push(ev),
            _ => break,
        }
    }
    Ok(received == sent)
}

/// Fragmentation: a large frame whose wire bytes are delivered to stdin in small
/// pieces must still be reassembled and echoed whole. A child that assumes a whole
/// frame arrives per `read()` fails.
fn check_fragmentation(exec: &str, timeout: Duration) -> anyhow::Result<bool> {
    // A near-maximum frame, so the wire bytes span many pipe reads.
    let payload = seeded_bytes(7, MAX_FRAME_SIZE - 128);
    let sent = Event::data("frag", Bytes::from(payload));
    let mut wire = Vec::new();
    encode(&sent, &mut wire).map_err(|e| anyhow::anyhow!("encode: {e}"))?;

    let mut child = ExecChild::spawn(exec)?;
    for piece in wire.chunks(97) {
        child.write_raw(piece)?;
    }
    child.close_stdin();
    // Generous total bound for a big frame; scale off the per-frame timeout.
    let bound = timeout.max(Duration::from_secs(5));
    match child.recv(bound) {
        Recv::Event(got) => Ok(got == sent),
        _ => Ok(false),
    }
}

/// Kill-and-restart cleanliness: a running child is killed mid-session and a fresh
/// one is spawned; the new child echoes cleanly, proving no reliance on state that
/// a `kill -9` would strand (the §7.6 restart-with-backoff contract, child side).
/// stdin is closed before the echo is required, so a batching or bounded-lag child
/// flushes at EOF — this check tests restart cleanliness, not liveness.
fn check_restart(exec: &str, timeout: Duration) -> anyhow::Result<bool> {
    let probe = Event::data("c0", Bytes::from_static(b"probe"));

    // First child: prove it echoes (stdin closed → it flushes), then drop it (Drop
    // kills and reaps).
    {
        let mut first = ExecChild::spawn(exec)?;
        first.send(&probe)?;
        first.close_stdin();
        if !matches!(first.recv(timeout), Recv::Event(ref got) if *got == probe) {
            return Ok(false);
        }
    }

    // A freshly spawned child must echo cleanly with no shared state.
    let mut second = ExecChild::spawn(exec)?;
    second.send(&probe)?;
    second.close_stdin();
    Ok(matches!(second.recv(timeout), Recv::Event(got) if got == probe))
}

// --- wire / tcp-proxy sockets ----------------------------------------------

/// A blocking duplex socket over tcp or unix, so the wire/proxy modes exercise
/// the same OS calls the daemon's leg uses (§3), transport-agnostically.
enum SimStream {
    Tcp(TcpStream),
    Unix(UnixStream),
}

impl SimStream {
    fn connect(transport: &str, address: &str) -> anyhow::Result<SimStream> {
        match transport {
            "tcp" => Ok(SimStream::Tcp(TcpStream::connect(address)?)),
            "unix" => Ok(SimStream::Unix(UnixStream::connect(address)?)),
            other => anyhow::bail!("unknown transport {other:?}"),
        }
    }
    fn set_read_timeout(&self, d: Option<Duration>) -> anyhow::Result<()> {
        match self {
            SimStream::Tcp(s) => s.set_read_timeout(d)?,
            SimStream::Unix(s) => s.set_read_timeout(d)?,
        }
        Ok(())
    }
}

impl Read for SimStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            SimStream::Tcp(s) => s.read(buf),
            SimStream::Unix(s) => s.read(buf),
        }
    }
}

impl Write for SimStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            SimStream::Tcp(s) => s.write(buf),
            SimStream::Unix(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            SimStream::Tcp(s) => s.flush(),
            SimStream::Unix(s) => s.flush(),
        }
    }
}

/// Whether a read error is a timeout (the socket read deadline elapsed) rather
/// than a real failure.
fn is_timeout(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}

// --- wire mode -------------------------------------------------------------

fn run_wire(a: WireArgs) -> Value {
    match run_wire_inner(&a) {
        Ok(v) => v,
        Err(e) => err_verdict("wire", &e),
    }
}

fn run_wire_inner(a: &WireArgs) -> anyhow::Result<Value> {
    let mut stream = SimStream::connect(&a.transport, &a.address)?;
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;

    // Send our hello (§9). A hostile peer sends a bad magic or a mismatched
    // version; both must be refused cleanly by the daemon.
    let mut hello_bytes = Vec::new();
    if a.bad_magic {
        write_bad_magic_hello(&mut hello_bytes, &a.announce);
    } else {
        let hello = Hello {
            version: a.hello_version,
            capabilities: a.capabilities,
            channels: a.announce.iter().map(|c| c.as_str().into()).collect(),
        };
        // Force the requested version even when it differs from ours (the
        // struct carries it, so encode_hello emits it verbatim).
        encode_hello(&hello, &mut hello_bytes).map_err(|e| anyhow::anyhow!("encode hello: {e}"))?;
    }
    stream.write_all(&hello_bytes)?;
    stream.flush()?;

    // Read the daemon's hello (it speaks first on accept).
    let deadline = Instant::now() + Duration::from_millis(a.timeout_ms);
    let mut inbuf: Vec<u8> = Vec::new();
    let mut peer_hello: Option<Hello> = None;
    let mut peer_closed = false;
    'hello: loop {
        match try_decode_hello(&inbuf) {
            Ok(Some((h, consumed))) => {
                inbuf.drain(..consumed);
                peer_hello = Some(h);
                break 'hello;
            }
            Ok(None) => {}
            Err(e) => anyhow::bail!("daemon sent a malformed hello: {e}"),
        }
        match read_more(&mut stream, &mut inbuf, deadline)? {
            ReadOutcome::Bytes => {}
            ReadOutcome::Eof => {
                peer_closed = true;
                break 'hello;
            }
            // A socket-level read timeout is not terminal — keep waiting until the
            // configured --timeout-ms deadline actually elapses (read_more's own
            // guard returns Timeout again once it does).
            ReadOutcome::Timeout => {
                if Instant::now() >= deadline {
                    break 'hello;
                }
            }
        }
    }

    // Post-handshake hostility: an oversize length prefix or an unknown type byte.
    if a.oversize_frame {
        let mut f = ((MAX_FRAME_SIZE + 1) as u32).to_be_bytes().to_vec();
        f.extend_from_slice(&[0, 0, 0]); // a few body bytes; the daemon refuses on the prefix
        stream.write_all(&f)?;
        stream.flush()?;
    }
    if a.unknown_type {
        // A well-framed body whose type byte is 9 (>3): a clean-refusal case.
        let body: &[u8] = &[9, 0, 1, b'x'];
        let mut f = (body.len() as u32).to_be_bytes().to_vec();
        f.extend_from_slice(body);
        stream.write_all(&f)?;
        stream.flush()?;
    }

    // Stall mode: stream sustained hostward on the announced channels but never
    // read, so the peer's whole-connection targetward backs up (§9 head-of-line)
    // while hostward keeps advancing. Holds the connection open the whole time.
    if a.stall {
        let deadline = Instant::now() + Duration::from_millis(a.hold_ms);
        let mut streamed: u64 = 0;
        let mut round: u64 = 0;
        let block = seeded_bytes(a.seed, 16 * 1024);
        while Instant::now() < deadline {
            for ch in &a.announce {
                let mut frame = Vec::new();
                if encode(
                    &Event::data(ch.as_str(), Bytes::from(block.clone())),
                    &mut frame,
                )
                .is_ok()
                    && stream.write_all(&frame).is_ok()
                {
                    streamed += block.len() as u64;
                }
            }
            round += 1;
            // Do NOT read the socket. A tiny pause bounds the hostward rate.
            thread::sleep(Duration::from_millis(5));
        }
        let _ = round;
        return Ok(json!({
            "tool": "nexus-sim", "mode": "wire",
            "behavior": "stall",
            "peer_version": peer_hello.as_ref().map(|h| h.version),
            "streamed_hostward": streamed,
            "pass": peer_hello.is_some(),
        }));
    }

    // Send seeded hostward data on the requested channels.
    let mut sent: u64 = 0;
    for spec in &a.send {
        let (chan, size) = parse_chan_size(spec)?;
        let payload = seeded_bytes(a.seed, size);
        let mut frame = Vec::new();
        encode(
            &Event::data(chan.as_str(), Bytes::from(payload)),
            &mut frame,
        )
        .map_err(|e| anyhow::anyhow!("encode data: {e}"))?;
        stream.write_all(&frame)?;
        sent += size as u64;
    }
    if !a.send.is_empty() {
        stream.flush()?;
    }

    // Echo / hold: drain frames until the hold window elapses, echoing targetward
    // data back hostward when requested. Also notices a peer close (refusal).
    let mut echoed: u64 = 0;
    let mut decoder = FrameDecoder::new();
    decoder.push(&inbuf);
    let hold_deadline = Instant::now() + Duration::from_millis(a.hold_ms);
    let run_deadline = if a.echo { deadline } else { hold_deadline };
    loop {
        loop {
            match decoder.next_event() {
                Ok(Some(ev)) => {
                    if a.echo
                        && let EventKind::Data(bytes) = &ev.kind
                    {
                        let n = bytes.len() as u64;
                        let mut frame = Vec::new();
                        if encode(&Event::data(ev.channel.as_str(), bytes.clone()), &mut frame)
                            .is_ok()
                            && stream.write_all(&frame).is_ok()
                        {
                            echoed += n;
                        }
                    }
                }
                Ok(None) => break,
                Err(e) => anyhow::bail!("daemon emitted a malformed frame: {e}"),
            }
        }
        if Instant::now() >= run_deadline {
            break;
        }
        let mut chunk = Vec::new();
        match read_more(&mut stream, &mut chunk, run_deadline)? {
            ReadOutcome::Bytes => decoder.push(&chunk),
            ReadOutcome::Eof => {
                peer_closed = true;
                break;
            }
            ReadOutcome::Timeout => {
                if !a.echo {
                    break;
                }
            }
        }
    }

    // A hostile peer expects a refusal (the daemon closes); a conforming peer
    // expects a valid hello. The script adds the daemon-side state assertion.
    let hostile =
        a.bad_magic || a.hello_version != WIRE_VERSION || a.oversize_frame || a.unknown_type;
    let pass = if hostile {
        peer_closed
    } else {
        peer_hello.is_some()
    };
    Ok(json!({
        "tool": "nexus-sim", "mode": "wire",
        "peer_version": peer_hello.as_ref().map(|h| h.version),
        "peer_channels": peer_hello.as_ref().map(|h| h.channels.iter().map(|c| c.0.clone()).collect::<Vec<_>>()),
        "peer_closed": peer_closed,
        "sent": sent, "echoed": echoed,
        "pass": pass,
    }))
}

enum ReadOutcome {
    Bytes,
    Eof,
    Timeout,
}

/// Read one chunk into `buf`, distinguishing bytes / clean EOF / (deadline)
/// timeout. The socket carries a short read timeout, so a quiet peer surfaces as
/// `Timeout` rather than blocking.
fn read_more(
    stream: &mut SimStream,
    buf: &mut Vec<u8>,
    deadline: Instant,
) -> anyhow::Result<ReadOutcome> {
    if Instant::now() >= deadline {
        return Ok(ReadOutcome::Timeout);
    }
    let mut tmp = [0u8; 8192];
    match stream.read(&mut tmp) {
        Ok(0) => Ok(ReadOutcome::Eof),
        Ok(k) => {
            buf.extend_from_slice(&tmp[..k]);
            Ok(ReadOutcome::Bytes)
        }
        Err(e) if is_timeout(&e) => Ok(ReadOutcome::Timeout),
        Err(e) => Err(e.into()),
    }
}

/// Craft a hello frame with a deliberately wrong magic number (a peer speaking
/// some other protocol), for the §9 clean-refusal case.
fn write_bad_magic_hello(out: &mut Vec<u8>, announce: &[String]) {
    let mut body = Vec::new();
    body.extend_from_slice(&(!WIRE_MAGIC).to_be_bytes()); // definitely not the magic
    body.extend_from_slice(&WIRE_VERSION.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes()); // capabilities
    body.extend_from_slice(&(announce.len() as u16).to_be_bytes());
    for ch in announce {
        body.extend_from_slice(&(ch.len() as u16).to_be_bytes());
        body.extend_from_slice(ch.as_bytes());
    }
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(&body);
}

fn parse_chan_size(spec: &str) -> anyhow::Result<(String, usize)> {
    let (chan, size) = spec
        .split_once('=')
        .ok_or_else(|| anyhow::anyhow!("--send expects <channel>=<size>, got {spec:?}"))?;
    Ok((chan.to_owned(), parse_size(size)?))
}

// --- tcp-proxy mode --------------------------------------------------------

fn run_tcp_proxy(a: TcpProxyArgs) -> Value {
    match run_tcp_proxy_inner(&a) {
        Ok(v) => v,
        Err(e) => err_verdict("tcp-proxy", &e),
    }
}

fn run_tcp_proxy_inner(a: &TcpProxyArgs) -> anyhow::Result<Value> {
    let drop_after = a.drop_after.as_deref().map(parse_size).transpose()?;
    let listener = TcpListener::bind(&a.listen)?;
    let overall_deadline = Instant::now() + Duration::from_millis(a.timeout_ms);

    // Phase 1: forward until the outage trigger (or, with no --drop-after, until
    // the overall deadline), counting the dialing daemon's outward bytes.
    let (before, severed) = proxy_once(&listener, &a.connect, drop_after, overall_deadline)?;

    if severed && a.restore_after_ms > 0 {
        thread::sleep(Duration::from_millis(a.restore_after_ms));
    }

    // Phase 2 (only if we injected an outage): forward cleanly until the deadline,
    // so post-restore traffic reconciles.
    let after = if severed {
        let (n, _) = proxy_once(&listener, &a.connect, None, overall_deadline)?;
        n
    } else {
        0
    };

    Ok(json!({
        "tool": "nexus-sim", "mode": "tcp-proxy",
        "forwarded_before_outage": before, "forwarded_after_restore": after,
        "outage_injected": severed, "pass": true,
    }))
}

/// Accept one dialing daemon, dial the other, and forward both directions until
/// `drop_after` bytes flow outward (then sever, returning `severed = true`) or the
/// deadline passes. Returns (outward bytes forwarded, severed).
fn proxy_once(
    listener: &TcpListener,
    connect_to: &str,
    drop_after: Option<usize>,
    deadline: Instant,
) -> anyhow::Result<(u64, bool)> {
    listener.set_nonblocking(false).ok();
    // Bounded accept so we don't hang forever if no daemon dials.
    let inbound = accept_before(listener, deadline)?;
    let outbound = TcpStream::connect(connect_to)?;
    inbound.set_read_timeout(Some(Duration::from_millis(200)))?;
    outbound.set_read_timeout(Some(Duration::from_millis(200)))?;

    let severed = Arc::new(AtomicBool::new(false));
    let outward = Arc::new(AtomicU64::new(0));

    let in_read = inbound.try_clone()?;
    let out_write = outbound.try_clone()?;
    let in_shut = inbound.try_clone()?;
    let out_shut = outbound.try_clone()?;

    // Dialing-daemon → other daemon, counting bytes and severing at the trigger.
    let sev1 = severed.clone();
    let outw = outward.clone();
    let t1 = thread::spawn(move || {
        pump_dir(in_read, out_write, &sev1, Some(&outw), drop_after, deadline);
        // Whatever ends this direction, sever both so the peers see the outage.
        sev1.store(true, Ordering::SeqCst);
        let _ = in_shut.shutdown(Shutdown::Both);
        let _ = out_shut.shutdown(Shutdown::Both);
    });

    let out_read = outbound.try_clone()?;
    let in_write = inbound.try_clone()?;
    let sev2 = severed.clone();
    let t2 = thread::spawn(move || {
        pump_dir(out_read, in_write, &sev2, None, None, deadline);
    });

    let _ = t1.join();
    let _ = t2.join();
    let did_sever =
        drop_after.is_some() && outward.load(Ordering::SeqCst) >= drop_after.unwrap() as u64;
    Ok((outward.load(Ordering::SeqCst), did_sever))
}

/// Accept a connection before the deadline (the dialing daemon may still be
/// backing off).
fn accept_before(listener: &TcpListener, deadline: Instant) -> anyhow::Result<TcpStream> {
    listener.set_nonblocking(true)?;
    loop {
        match listener.accept() {
            Ok((s, _)) => {
                s.set_nonblocking(false)?;
                return Ok(s);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    anyhow::bail!("no daemon dialed the proxy before the deadline");
                }
                thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// Copy one direction until severed, EOF/error, the byte trigger, or the
/// deadline. Counts bytes into `counter` when given; sets `severed` at the
/// trigger.
fn pump_dir(
    mut src: TcpStream,
    mut dst: TcpStream,
    severed: &AtomicBool,
    counter: Option<&AtomicU64>,
    drop_after: Option<usize>,
    deadline: Instant,
) {
    let mut buf = [0u8; 16384];
    loop {
        if severed.load(Ordering::SeqCst) || Instant::now() >= deadline {
            return;
        }
        match src.read(&mut buf) {
            Ok(0) => return, // EOF
            Ok(k) => {
                if dst.write_all(&buf[..k]).is_err() {
                    return;
                }
                if let Some(c) = counter {
                    let total = c.fetch_add(k as u64, Ordering::SeqCst) + k as u64;
                    if let Some(limit) = drop_after
                        && total >= limit as u64
                    {
                        severed.store(true, Ordering::SeqCst);
                        return; // the trigger: end this direction to sever
                    }
                }
            }
            Err(ref e) if is_timeout(e) => {} // poll again (checks deadline/severed)
            Err(_) => return,
        }
    }
}
