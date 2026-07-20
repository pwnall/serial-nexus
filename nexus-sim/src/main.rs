#![deny(unsafe_code)]

//! `nexus-sim` — the in-workspace test double (design plan §3).
//!
//! A purpose-built double that uses the *same permissive PTY and socket calls
//! as the daemon* — so validating with it exercises those calls twice. Every
//! mode is deterministic under `--seed`, prints a single JSON verdict line on
//! exit, and exits 0 only on pass.
//!
//! Phase 1 lands the `pty` and `client` modes plus the verdict plumbing, so the
//! judges exist before anything they will judge. `mux`/`envelope` arrive in
//! phase 5 and `wire`/`tcp-proxy` in phase 6.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use clap::{Args, Parser, Subcommand};
use nix::fcntl::OFlag;
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::pty::{PtyMaster, grantpt, posix_openpt, ptsname_r, unlockpt};
use nix::sys::termios::{
    BaudRate, LocalFlags, SetArg, cfgetospeed, cfmakeraw, cfsetspeed, tcgetattr, tcsetattr,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

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
    #[arg(long, default_value_t = 5000)]
    timeout_ms: u64,
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

fn main() {
    let cli = Cli::parse();
    let verdict = match cli.mode {
        Mode::Pty(a) => run_pty(a),
        Mode::Client(a) => run_client(a),
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

/// Set a fd to raw termios (no echo, no translation) — binary-transparent.
fn set_raw<F: AsFd>(fd: &F) -> anyhow::Result<()> {
    set_raw_baud(fd, None)
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
    let pts = ptsname_r(&master)?;

    // Raw baseline on the pair, applied through the master so we never open the
    // slave ourselves (per S2: opening+closing the slave would prime POLLHUP).
    set_raw(&master)?;

    if let Some(link) = &a.link {
        let _ = std::fs::remove_file(link);
        std::os::unix::fs::symlink(&pts, link)?;
    }

    let result = if a.echo {
        pty_echo(&mut master, a.timeout_ms)
    } else if a.source {
        let n = parse_size(a.bytes.as_deref().unwrap_or("0"))?;
        pty_source(&mut master, a.seed, n, a.rate)
    } else if a.sink {
        let n = parse_size(a.bytes.as_deref().unwrap_or("0"))?;
        pty_sink(&mut master, n, a.timeout_ms)
    } else if a.report_termios {
        pty_report_termios(&master)
    } else {
        anyhow::bail!("pty: pick one of --echo/--source/--sink/--report-termios")
    };

    if let Some(link) = &a.link {
        let _ = std::fs::remove_file(link);
    }
    result
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
) -> anyhow::Result<Value> {
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

fn pty_report_termios(master: &PtyMaster) -> anyhow::Result<Value> {
    let t = tcgetattr(master)?;
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
) -> anyhow::Result<(u64, String)> {
    let start = Instant::now();
    let deadline = start + Duration::from_millis(timeout_ms);
    let mut received: u64 = 0;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
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
            // Never overshoot a fixed target.
            let cap = target.map_or(buf.len(), |t| (t - received as usize).min(buf.len()));
            match fd.read(&mut buf[..cap]) {
                Ok(0) => break,
                Ok(k) => {
                    hasher.update(&buf[..k]);
                    received += k as u64;
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
