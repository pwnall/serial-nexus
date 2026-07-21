#![forbid(unsafe_code)]

//! `serialnexusctl` — the serial_nexus CLI.
//!
//! A JSON-RPC client plus a rendering layer, nothing else (§15.16). The daemon
//! returns structured JSON; this renders it (a table for `state`, TOML for
//! `dump`). `--json` passes the raw result through, so agents can drive the CLI
//! or speak JSON-RPC to the socket directly. Nothing here is contract — only the
//! RPC surface in `nexus-rpc` is.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use nexus_core::config::GraphConfig;
use nexus_rpc::{Incoming, Request, Response};
use serde_json::{Value, json};

#[derive(Parser)]
#[command(
    name = "serialnexusctl",
    version,
    about = "serial_nexus control CLI (§10)"
)]
struct Cli {
    /// Override the control socket path (defaults match the daemon, §10).
    #[arg(long)]
    socket: Option<PathBuf>,
    /// Print the raw JSON result instead of rendered output.
    #[arg(long)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Load a TOML configuration onto an empty graph.
    Load { file: PathBuf },
    /// Dump the current configuration (TOML by default).
    Dump,
    /// Report observed node state.
    State,
    /// Stream node status and counter snapshots as they change. Prints one JSON
    /// notification per line; exits after `--count` of them (default: run until
    /// the connection closes).
    Subscribe {
        #[arg(long)]
        count: Option<usize>,
    },
    /// Rotate a log node's file on demand.
    Rotate { node: String },
    /// Acquire the exclusive write lock for an origin (§6): only its bytes are
    /// then read targetward through the endpoint it feeds. A plain contended
    /// acquire fails fast; `--wait` joins the FIFO queue; `--steal` takes the lock
    /// from the current holder; `--lease-ms` auto-releases after a duration.
    Lock {
        origin: String,
        /// Take the lock from whoever holds it (recorded in state, §6).
        #[arg(long)]
        steal: bool,
        /// Block until the lock is granted instead of failing fast.
        #[arg(long)]
        wait: bool,
        /// Auto-release the lock this many milliseconds after the grant.
        #[arg(long)]
        lease_ms: Option<u64>,
    },
    /// Release the write lock held by an origin.
    Unlock { origin: String },
    /// Send one line targetward through an endpoint (§6): the CLI acquires the
    /// endpoint's write lock (with a timeout), writes the line, and releases —
    /// one atomic operation. `--steal` takes the lock rather than waiting.
    Send {
        /// The host-facing endpoint to write to (e.g. `usb0` or `mux/ch2`).
        endpoint: String,
        /// The line to send (a trailing newline is appended).
        #[arg(long)]
        line: String,
        /// Give up with the locked error after this long if the lock is held.
        #[arg(long)]
        timeout_ms: Option<u64>,
        /// Take the lock from the current holder instead of waiting.
        #[arg(long)]
        steal: bool,
    },
    /// Tear down the whole graph.
    Teardown,
    /// Ask the daemon to shut down.
    Shutdown,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let socket = resolve_socket(cli.socket.clone());

    // `subscribe` is a stream, not a single request/response — handle it apart
    // from the one-shot verbs below.
    if let Cmd::Subscribe { count } = &cli.cmd {
        return subscribe_stream(&socket, *count);
    }

    let (method, params) = build_request(&cli.cmd)?;
    let response = call(&socket, method, params)?;

    if let Some(err) = response.error {
        eprintln!("error {}: {}", err.code, err.message);
        if let Some(data) = err.data {
            eprintln!("{}", serde_json::to_string_pretty(&data)?);
        }
        std::process::exit(1);
    }
    let result = response.result.unwrap_or(Value::Null);

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        render(&cli.cmd, &result)?;
    }
    Ok(())
}

fn build_request(cmd: &Cmd) -> anyhow::Result<(&'static str, Option<Value>)> {
    Ok(match cmd {
        Cmd::Load { file } => {
            let text = std::fs::read_to_string(file)?;
            let config: GraphConfig = toml::from_str(&text)
                .map_err(|e| anyhow::anyhow!("parsing {}: {e}", file.display()))?;
            (
                "load",
                Some(json!({ "config": serde_json::to_value(&config)? })),
            )
        }
        Cmd::Dump => ("dump", None),
        Cmd::State => ("state", None),
        Cmd::Subscribe { .. } => unreachable!("subscribe is handled before dispatch"),
        Cmd::Rotate { node } => ("rotate", Some(json!({ "node": node }))),
        Cmd::Lock {
            origin,
            steal,
            wait,
            lease_ms,
        } => (
            "lock",
            Some(json!({
                "origin": origin,
                "steal": steal,
                "wait": wait,
                "lease_ms": lease_ms,
            })),
        ),
        Cmd::Unlock { origin } => ("unlock", Some(json!({ "origin": origin }))),
        Cmd::Send {
            endpoint,
            line,
            timeout_ms,
            steal,
        } => (
            "send",
            Some(json!({
                "endpoint": endpoint,
                "line": line,
                "timeout_ms": timeout_ms,
                "steal": steal,
            })),
        ),
        Cmd::Teardown => ("teardown", None),
        Cmd::Shutdown => ("shutdown", None),
    })
}

/// Render a successful result for humans (the `--json` path bypasses this).
fn render(cmd: &Cmd, result: &Value) -> anyhow::Result<()> {
    match cmd {
        Cmd::Dump => {
            let config: GraphConfig = serde_json::from_value(result.clone())?;
            print!("{}", toml::to_string(&config)?);
        }
        Cmd::State => {
            let empty = vec![];
            let nodes = result
                .get("nodes")
                .and_then(Value::as_array)
                .unwrap_or(&empty);
            if nodes.is_empty() {
                println!("(empty graph)");
            }
            for n in nodes {
                let name = n.get("name").and_then(Value::as_str).unwrap_or("?");
                let status = n.get("status").and_then(Value::as_str).unwrap_or("?");
                let reason = n
                    .get("reason")
                    .and_then(Value::as_str)
                    .map(|r| format!(" ({r})"))
                    .unwrap_or_default();
                println!("{name:<16} {status}{reason}");
            }
        }
        Cmd::Load { .. } => {
            let n = result.get("loaded").and_then(Value::as_u64).unwrap_or(0);
            println!("loaded {n} node(s)");
        }
        Cmd::Rotate { node } => {
            let n = result.get("rotated_to").and_then(Value::as_u64);
            match n {
                Some(n) => println!("{node}: rotating to {n}"),
                None => println!("{node}: rotation requested"),
            }
        }
        Cmd::Lock { origin, .. } => {
            let acquired = result
                .get("acquired")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let held = result.get("held").and_then(Value::as_bool).unwrap_or(false);
            let msg = if acquired {
                "lock acquired"
            } else if held {
                "already holds the lock"
            } else {
                "not held"
            };
            let stole = result
                .get("stole_from")
                .and_then(Value::as_str)
                .map(|f| format!(" (stolen from {f})"))
                .unwrap_or_default();
            println!("{origin}: {msg}{stole}");
        }
        Cmd::Send { endpoint, .. } => {
            let sent = result.get("sent").and_then(Value::as_u64).unwrap_or(0);
            let delivered = result
                .get("delivered")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if delivered {
                println!("{endpoint}: sent {sent} byte(s)");
            } else {
                println!("{endpoint}: not delivered");
            }
        }
        Cmd::Unlock { origin } => {
            let released = result
                .get("released")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            println!(
                "{origin}: {}",
                if released {
                    "unlocked"
                } else {
                    "was not holding the lock"
                }
            );
        }
        Cmd::Teardown => {
            let n = result.get("torn_down").and_then(Value::as_u64).unwrap_or(0);
            println!("tore down {n} node(s)");
        }
        Cmd::Shutdown => println!("shutdown requested"),
        Cmd::Subscribe { .. } => unreachable!("subscribe is handled before dispatch"),
    }
    Ok(())
}

/// Open the socket, subscribe, and print one JSON notification per line as they
/// arrive (§10). Exits after `count` notifications, or when the daemon closes
/// the connection. The subscribe acknowledgement is consumed, not printed, so
/// the output is a clean stream of notification objects for `jq`.
fn subscribe_stream(socket: &Path, count: Option<usize>) -> anyhow::Result<()> {
    let stream = UnixStream::connect(socket)
        .map_err(|e| anyhow::anyhow!("connecting to {}: {e}", socket.display()))?;
    let mut writer = stream.try_clone()?;
    writer.write_all(nexus_rpc::to_line(&Request::new(1, "subscribe", None)).as_bytes())?;
    writer.flush()?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let limit = count.unwrap_or(usize::MAX);
    let mut printed = 0usize;
    let mut stdout = std::io::stdout().lock();
    while printed < limit {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break; // daemon closed the connection
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<Incoming>(trimmed) {
            // The ack for the subscribe request itself — swallow it.
            Ok(Incoming::Response(_)) => {}
            Ok(Incoming::Notification(note)) => {
                writeln!(stdout, "{}", serde_json::to_string(&note)?)?;
                stdout.flush()?;
                printed += 1;
            }
            // Unrecognized frame: pass it through so nothing is silently lost.
            Err(_) => {
                writeln!(stdout, "{trimmed}")?;
                stdout.flush()?;
                printed += 1;
            }
        }
    }
    Ok(())
}

fn call(socket: &Path, method: &str, params: Option<Value>) -> anyhow::Result<Response> {
    let stream = UnixStream::connect(socket)
        .map_err(|e| anyhow::anyhow!("connecting to {}: {e}", socket.display()))?;
    let mut writer = stream.try_clone()?;
    let request = Request::new(1, method, params);
    writer.write_all(nexus_rpc::to_line(&request).as_bytes())?;
    writer.flush()?;

    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line)?;
    if line.trim().is_empty() {
        anyhow::bail!("daemon closed the connection without replying");
    }
    Ok(serde_json::from_str(line.trim())?)
}

/// Mirror the daemon's §10 socket-path policy exactly.
fn resolve_socket(override_path: Option<PathBuf>) -> PathBuf {
    if let Some(p) = override_path {
        return p;
    }
    if nix::unistd::geteuid().is_root() {
        return PathBuf::from("/run/serialnexusd.sock");
    }
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir).join("serialnexusd.sock");
        }
    }
    PathBuf::from(format!("/tmp/serialnexusd-{}.sock", nix::unistd::getuid()))
}
