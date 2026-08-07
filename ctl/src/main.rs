#![forbid(unsafe_code)]

//! `serial-nexus-ctl` — the serial_nexus CLI.
//!
//! A JSON-RPC client plus a rendering layer, nothing else (§15.16). The daemon
//! returns structured JSON; this renders it (a table for `state`, TOML for
//! `dump`). `--json` passes the raw result through, so agents can drive the CLI
//! or speak JSON-RPC to the socket directly. Nothing here is contract — only the
//! RPC surface in `serial-nexus-rpc` is.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use serial_nexus_core::config::GraphConfig;
use serial_nexus_rpc::{Incoming, Request, Response};

#[derive(Parser)]
#[command(
    name = "serial-nexus-ctl",
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
    /// Load a TOML configuration onto an empty graph, or `--replace` a running one
    /// (teardown-then-load, §11).
    Load {
        file: PathBuf,
        /// Tear down any running graph first (§11).
        #[arg(long)]
        replace: bool,
    },
    /// Add one node (no edges) to a running graph (§11). The file is a TOML
    /// configuration containing a single `[[node]]`. For a serial node the device
    /// is resolved and its captured identity echoed back (§12).
    AddNode { file: PathBuf },
    /// Remove one node (§11). Refused while edges are attached unless `--cascade`,
    /// which removes those edges too (flushing a log queue first, §7.3).
    RemoveNode {
        node: String,
        #[arg(long)]
        cascade: bool,
    },
    /// Attach one edge to a running graph (§15.35): `connect <a> <b>`, where the
    /// endpoints are display addresses (`usb0`, `mux/console`, `cons/raw`). The
    /// same structural rules as `load` apply — an illegal edge is refused naming
    /// the rule, with nothing changed.
    Connect {
        /// One endpoint (order does not matter; orientation comes from facings).
        a: String,
        /// The other endpoint.
        b: String,
        /// Write mode for this edge: `never`, `on-demand` (default) or `held` (§6).
        #[arg(long)]
        write_mode: Option<String>,
    },
    /// Remove one edge from a running graph (§15.35). A lock-holding origin
    /// releases the lock and its un-flushed backlog is purged, so the endpoint is
    /// left writable rather than wedged on a writer that is gone (§6).
    Disconnect { a: String, b: String },
    /// Dump the current configuration (TOML by default).
    Dump,
    /// Report observed node state.
    State,
    /// Report the daemon's capability surface (§10/§15.26): its version, the wire
    /// and envelope protocol versions, and the registered codec names — so you can
    /// discover what a possibly-custom daemon supports rather than assume it.
    Info,
    /// List the serial devices this machine has, the identity that would bind each
    /// one, and which graph node already holds it (§12/§15.35). Strictly passive:
    /// nothing is opened, so listing a port never toggles DTR on the board behind
    /// it.
    Ports,
    /// Stream node status and counter snapshots as they change. Prints one JSON
    /// notification per line; exits after `--count` of them (default: run until
    /// the connection closes).
    Subscribe {
        #[arg(long)]
        count: Option<usize>,
    },
    /// Watch a host-facing endpoint's hostward stream over a connection-scoped tap
    /// (§17): the raw decoded bytes are written to stdout as they arrive. With
    /// `--replay` the endpoint's replay ring (§5) is delivered first (ring-then-live,
    /// exact splice). Exits after `--bytes` decoded bytes, when the graph drops the
    /// endpoint (`teardown`, `load --replace`, `remove-node`), or when the connection
    /// closes; a lost byte is reported on stderr, never folded into stdout silently.
    /// Read-only: a tap never writes to the device and never touches config.
    Tap {
        /// The host-facing endpoint to observe (e.g. `usb0` or `mux/ch2`).
        endpoint: String,
        /// Prefix the live stream with the endpoint's replay ring, if configured.
        #[arg(long)]
        replay: bool,
        /// Stop after this many decoded bytes have been written (default: run until
        /// the connection closes).
        #[arg(long)]
        bytes: Option<u64>,
        /// Open the tap but then stop reading for this many milliseconds — a paused
        /// browser tab (§17). The daemon's bounded per-tap queue fills and drops
        /// with a counter, so a slow tab costs only its own tap. Used to exercise
        /// the drop path; the ack is still consumed so the open is confirmed.
        #[arg(long)]
        stall_ms: Option<u64>,
    },
    /// Rotate a log node's file on demand.
    Rotate { node: String },
    /// Assert a serial break on a node for `--ms` milliseconds (§7.1).
    SendBreak {
        node: String,
        #[arg(long, default_value_t = 250)]
        ms: u64,
    },
    /// Drive DTR and/or RTS on a serial node's live port (§7.1). Omitted lines are
    /// left untouched.
    SetModem {
        node: String,
        #[arg(long)]
        dtr: Option<bool>,
        #[arg(long)]
        rts: Option<bool>,
    },
    /// Pulse DTR (the auto-reset toggle, §7.1): drive it to `--assert` for `--ms`
    /// milliseconds, then to the opposite level.
    PulseDtr {
        node: String,
        #[arg(long, default_value_t = 100)]
        ms: u64,
        /// The level to hold during the pulse (then reset to its opposite).
        // `ArgAction::Set` deliberately, not clap's inferred `SetTrue` for a `bool`:
        // the RPC's low-then-high pulse is `assert = false`, and a `SetTrue` flag with
        // `default_value_t = true` can only ever send `true`, making the documented
        // `--assert false` unreachable (review 26, CLI-1). `num_args = 0..=1` plus
        // `default_missing_value` keeps the bare `--assert` spelling working — it was
        // a harmless no-op in operator scripts before, and turning it into a hard
        // error would be a gratuitous break. Kept as a `//` comment, not a doc
        // comment: clap renders `///` verbatim into `--help`, where this rationale
        // is noise to an operator.
        #[arg(
            long,
            action = clap::ArgAction::Set,
            num_args = 0..=1,
            default_missing_value = "true",
            default_value_t = true
        )]
        assert: bool,
    },
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
    let json = cli.json;
    match run(cli) {
        Ok(()) => Ok(()),
        // A *client-side* failure — an unreadable file, a TOML parse error, a socket
        // that will not connect — must be as machine-readable as a daemon error when
        // `--json` is on (review 26, CLI-4). Otherwise an agent driving the documented
        // JSON mode still has to parse human text for half the failures it can hit.
        // These carry no JSON-RPC code, so they take the standard internal-error code
        // and say plainly where they came from; the shape is the same envelope, so a
        // caller has exactly one thing to parse.
        Err(e) if json => {
            let envelope = json!({
                "error": {
                    "code": serial_nexus_rpc::error_codes::INTERNAL_ERROR,
                    "message": format!("{e:#}"),
                    "data": { "origin": "client" },
                }
            });
            println!("{}", serde_json::to_string_pretty(&envelope)?);
            std::process::exit(1);
        }
        Err(e) => Err(e),
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    let socket = resolve_socket(cli.socket.clone());

    // `subscribe` and `tap` are streams, not single request/response — handle them
    // apart from the one-shot verbs below.
    if let Cmd::Subscribe { count } = &cli.cmd {
        return subscribe_stream(&socket, *count);
    }
    if let Cmd::Tap {
        endpoint,
        replay,
        bytes,
        stall_ms,
    } = &cli.cmd
    {
        return tap_stream(&socket, endpoint, *replay, *bytes, *stall_ms);
    }

    let (method, params) = build_request(&cli.cmd)?;
    let response = call(&socket, method, params)?;

    if let Some(err) = response.error {
        if cli.json {
            // `--json` is the machine-readable mode, so its *error* path must be
            // machine-readable too — otherwise an agent driving it has to parse
            // human text to learn what failed (review 26, CLI-4). The daemon's
            // JSON-RPC error object goes to stdout under an `error` key, which
            // cannot collide with the success path: that is a raw pass-through of
            // the daemon `result` (§15.16) and is never printed alongside this.
            // The exit code stays non-zero.
            let envelope = json!({ "error": serde_json::to_value(&err)? });
            println!("{}", serde_json::to_string_pretty(&envelope)?);
        } else {
            eprintln!("error {}: {}", err.code, err.message);
            if let Some(data) = err.data {
                eprintln!("{}", serde_json::to_string_pretty(&data)?);
            }
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
        Cmd::Load { file, replace } => {
            let config = read_config(file)?;
            (
                "load",
                Some(json!({ "config": serde_json::to_value(&config)?, "replace": replace })),
            )
        }
        Cmd::AddNode { file } => {
            // A single-node TOML configuration; take its one node — and refuse
            // anything larger rather than adding the first node and dropping the
            // rest (review 26, CLI-2). Silently discarding configuration the
            // operator wrote is the defect, whether it is a `[[node]]` or an
            // `[[edge]]`; `connect` can add the edge back (§15.35), but only if the
            // operator is told it went missing.
            let config = read_config(file)?;
            let node = single_node(&config, file)?;
            (
                "add-node",
                Some(json!({ "node": serde_json::to_value(node)? })),
            )
        }
        Cmd::RemoveNode { node, cascade } => (
            "remove-node",
            Some(json!({ "node": node, "cascade": cascade })),
        ),
        Cmd::Connect { a, b, write_mode } => (
            "connect",
            Some(json!({ "a": a, "b": b, "write_mode": write_mode })),
        ),
        Cmd::Disconnect { a, b } => ("disconnect", Some(json!({ "a": a, "b": b }))),
        Cmd::Dump => ("dump", None),
        Cmd::State => ("state", None),
        Cmd::Info => ("info", None),
        Cmd::Ports => ("ports", None),
        Cmd::Subscribe { .. } => unreachable!("subscribe is handled before dispatch"),
        Cmd::Tap { .. } => unreachable!("tap is handled before dispatch"),
        Cmd::Rotate { node } => ("rotate", Some(json!({ "node": node }))),
        Cmd::SendBreak { node, ms } => ("send-break", Some(json!({ "node": node, "ms": ms }))),
        Cmd::SetModem { node, dtr, rts } => (
            "set-modem",
            Some(json!({ "node": node, "dtr": dtr, "rts": rts })),
        ),
        Cmd::PulseDtr { node, ms, assert } => (
            "pulse-dtr",
            Some(json!({ "node": node, "ms": ms, "assert": assert })),
        ),
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

/// Read and parse a `GraphConfig` from a TOML file, mapping a parse error to a
/// message that names the file (shared by `load` and `add-node`).
fn read_config(file: &Path) -> anyhow::Result<GraphConfig> {
    // Name the file on the *read* too, not only on the parse below (review 32, CTL-3).
    // Every other client-side I/O failure in this file names its subject — the parse
    // arm, the empty-graph bail, all three `UnixStream::connect` sites — and a bare
    // `No such file or directory (os error 2)` is unusable with several configurations
    // in play, in the human arm and in `--json` alike.
    let text = std::fs::read_to_string(file)
        .map_err(|e| anyhow::anyhow!("reading {}: {e}", file.display()))?;
    let config: GraphConfig =
        toml::from_str(&text).map_err(|e| anyhow::anyhow!("parsing {}: {e}", file.display()))?;
    // A file with content that parses to *nothing* is the one input that turns
    // `load --replace` into an unannounced `teardown` reported as success (review 26,
    // CP-2/CFG-3): a typo'd table name — `[[nodez]]` — yields an empty graph, and
    // §11's "the entire file is validated before anything is created" cannot catch it
    // because there is nothing to validate. Unknown *keys* are now rejected by serde;
    // an unknown *table* has no variant to reject it, so name the mistake here.
    // (An empty graph is legitimate over RPC — `teardown` persists one — so this is a
    // property of "the operator handed me this file", not of the config type.)
    if config.is_empty() && !text.trim().is_empty() {
        anyhow::bail!(
            "{}: parsed to an empty graph although the file is not empty — no [[node]] \
             or [[edge]] table was recognised (a misspelled table name such as \
             `[[nodez]]` parses to nothing). Nothing was sent.",
            file.display()
        );
    }
    Ok(config)
}

/// The one `[[node]]` an `add-node` file may contain — or an error naming what
/// was actually found (review 26, CLI-2).
///
/// `add-node` adds exactly one node and no edges (§11), so a file carrying more
/// than that is an operator mistake, not an instruction to take the first node:
/// the surplus would be dropped silently. `load` / `load --replace` is the
/// multi-node verb, so the message points there — and, since §15.35, at
/// `connect` for wiring nodes added one at a time.
fn single_node<'c>(
    config: &'c GraphConfig,
    file: &Path,
) -> anyhow::Result<&'c serial_nexus_core::config::NodeConfig> {
    let (nodes, edges) = (config.nodes.len(), config.edges.len());
    if nodes == 0 {
        anyhow::bail!("{}: no [[node]] to add", file.display());
    }
    if nodes > 1 || edges > 0 {
        anyhow::bail!(
            "{}: add-node takes a single [[node]] and no [[edge]], but this file has \
             {nodes} node(s) and {edges} edge(s) — nothing was added. Use \
             `serial-nexus-ctl load` (or `load --replace` over a running graph) for a \
             multi-node configuration, or add the nodes one at a time and wire them \
             with `connect`.",
            file.display(),
        );
    }
    Ok(&config.nodes[0])
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
        Cmd::Info => {
            let daemon = result
                .get("daemon_version")
                .and_then(Value::as_str)
                .unwrap_or("?");
            let wire = result.get("wire_version").and_then(Value::as_u64);
            let envelope = result.get("envelope_version").and_then(Value::as_u64);
            let empty = vec![];
            let codecs: Vec<&str> = result
                .get("codecs")
                .and_then(Value::as_array)
                .unwrap_or(&empty)
                .iter()
                .filter_map(Value::as_str)
                .collect();
            println!("daemon {daemon}");
            if let (Some(w), Some(e)) = (wire, envelope) {
                println!("wire v{w}, envelope v{e}");
            }
            println!("codecs: {}", codecs.join(", "));
        }
        Cmd::Connect { .. } => {
            let mode = result
                .get("write_mode")
                .and_then(Value::as_str)
                .unwrap_or("?");
            let pair = result.get("connected");
            let a = pair
                .and_then(|p| p.get("a"))
                .and_then(Value::as_str)
                .unwrap_or("?");
            let b = pair
                .and_then(|p| p.get("b"))
                .and_then(Value::as_str)
                .unwrap_or("?");
            // Host first, then target: the rendering states the orientation the
            // daemon resolved, so an operator sees which end produces (§15.3).
            println!("connected {a} -> {b} ({mode})");
        }
        Cmd::Disconnect { .. } => {
            let pair = result.get("disconnected");
            let a = pair
                .and_then(|p| p.get("a"))
                .and_then(Value::as_str)
                .unwrap_or("?");
            let b = pair
                .and_then(|p| p.get("b"))
                .and_then(Value::as_str)
                .unwrap_or("?");
            println!("disconnected {a} -> {b}");
            if result.get("released_lock").and_then(Value::as_bool) == Some(true) {
                println!("  the removed origin held the write lock; it is released");
            }
            let purged = result
                .get("purged_bytes")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if purged > 0 {
                println!("  purged {purged} un-flushed byte(s) from the removed origin");
            }
        }
        Cmd::Ports => {
            let empty = vec![];
            let ports = result
                .get("ports")
                .and_then(Value::as_array)
                .unwrap_or(&empty);
            if ports.is_empty() {
                println!("(no serial devices found)");
            }
            for p in ports {
                let path = p.get("path").and_then(Value::as_str).unwrap_or("?");
                let identity = p.get("identity").and_then(Value::as_str).unwrap_or("?");
                let desc = p.get("description").and_then(Value::as_str).unwrap_or("");
                // The bound flag comes first on the line: the operator's question is
                // "what can I take", and a free port should be scannable at a glance.
                let bound = match p.get("bound_to").and_then(Value::as_str) {
                    Some(node) => format!("bound {node}"),
                    None => "free".to_owned(),
                };
                println!("{path:<24} {bound:<16} {identity}");
                if !desc.is_empty() {
                    println!("{:<24} {desc}", "");
                }
                // The degraded identity forms carry a documented instability
                // warning (§12); an operator about to bind one should see it here
                // rather than discover it after a replug.
                if let Some(w) = p.get("warning").and_then(Value::as_str) {
                    println!("{:<24} ⚠ {w}", "");
                }
            }
        }
        Cmd::Load { .. } => {
            let n = result.get("loaded").and_then(Value::as_u64).unwrap_or(0);
            println!("loaded {n} node(s)");
        }
        Cmd::AddNode { .. } => {
            let name = result.get("added").and_then(Value::as_str).unwrap_or("?");
            print!("added {name}");
            if let Some(desc) = result.get("description").and_then(Value::as_str) {
                print!(" — bound: {desc}");
            }
            println!();
            if let Some(w) = result.get("warning").and_then(Value::as_str) {
                eprintln!("warning: {w}");
            }
        }
        Cmd::RemoveNode { .. } => {
            let name = result.get("removed").and_then(Value::as_str).unwrap_or("?");
            let edges = result
                .get("cascaded_edges")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if edges > 0 {
                println!("removed {name} (and {edges} edge(s))");
            } else {
                println!("removed {name}");
            }
        }
        Cmd::Rotate { node } => {
            let n = result.get("rotated_to").and_then(Value::as_u64);
            match n {
                Some(n) => println!("{node}: rotating to {n}"),
                None => println!("{node}: rotation requested"),
            }
        }
        Cmd::SendBreak { node, ms } => println!("{node}: break asserted for {ms}ms"),
        Cmd::SetModem { node, .. } => println!("{node}: modem lines set"),
        Cmd::PulseDtr { node, ms, .. } => println!("{node}: DTR pulsed for {ms}ms"),
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
        Cmd::Tap { .. } => unreachable!("tap is handled before dispatch"),
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
    writer.write_all(serial_nexus_rpc::to_line(&Request::new(1, "subscribe", None)).as_bytes())?;
    writer.flush()?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let limit = count.unwrap_or(usize::MAX);
    let mut printed = 0usize;
    let mut stdout = std::io::stdout().lock();

    // Read the subscribe acknowledgement FIRST, exactly as `tap_stream` does, and
    // *examine* it. Swallowing every Response frame swallowed the error reply to this
    // very request too — and a daemon that refuses `subscribe` (a version skew across
    // the §15.16 graceful-degradation boundary) deliberately keeps the connection open,
    // so the loop below then blocked in `read_line` on a socket that would never carry
    // another byte: no output, no exit, no diagnosis (37-TOOL-2). A refusal is now the
    // ordinary error path, naming the code like every other client-side failure.
    //
    // Nothing can be lost by reading it here: the daemon writes the ack *before* it
    // counts the connection as a subscriber, so no notification can precede it. The
    // stray-frame tolerance is defensive only, as it is in `tap_stream`.
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            anyhow::bail!("connection closed before the subscribe acknowledgement");
        }
        if let Ok(Incoming::Response(resp)) = serde_json::from_str::<Incoming>(line.trim()) {
            if let Some(err) = resp.error {
                anyhow::bail!("subscribe failed: {} ({})", err.message, err.code);
            }
            break;
        }
    }

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
            // A later Response on this connection is not ours to print — one
            // subscription per connection, and its ack was read above.
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

/// The offset-continuity tracker for one tap stream (review 32, CTL-2).
///
/// A `tap.data` carries two *independent* discontinuity signals, and this CLI — the
/// one first-party consumer that writes a capture file — used to read neither, so
/// `serial-nexus-ctl tap console > incident.bin` produced a holed stream with nothing
/// said on stdout or stderr:
///
/// * `gap_before` — bytes lost at the endpoint's producer→hub feed, the one hop §5
///   sanctions losing on. The offset space deliberately cannot express it (folding it
///   in would leave `from_offset` naming an offset the live stream never uses — see
///   "the offset contract" in `docs/rpc/observation.md`), so offsets stay contiguous
///   *across* the hole and this field is its only signal.
/// * an `offset` above the expected `previous offset + previous length` — how a
///   per-tap queue drop surfaces instead: "a gap it can see, never a silent shift".
///
/// Both notices go to **stderr**, beside the `tap opened:` line: stdout is the capture
/// and stays a clean byte stream (§15.16 lets the rendering move, not the bytes).
struct TapContinuity {
    /// The offset the next chunk should carry — seeded from `tap.open`'s `from_offset`
    /// and advanced by each chunk's length. `None` only if the daemon reported no
    /// offsets at all, which disables the offset check alone; `gap_before` still reports.
    expected: Option<u64>,
}

impl TapContinuity {
    fn new(from_offset: Option<u64>) -> Self {
        TapContinuity {
            expected: from_offset,
        }
    }

    /// Observe one `tap.data` and return the notices it warrants (empty = contiguous).
    fn observe(&mut self, offset: Option<u64>, gap_before: u64, len: usize) -> Vec<String> {
        // `Vec::new` does not allocate, so a clean stream — the overwhelming case —
        // costs nothing on this per-chunk path.
        let mut notices = Vec::new();
        if gap_before > 0 {
            notices.push(match offset {
                Some(o) => {
                    format!("tap gap: {gap_before} bytes lost before offset {o} (daemon feed)")
                }
                None => format!("tap gap: {gap_before} bytes lost (daemon feed)"),
            });
        }
        if let (Some(o), Some(e)) = (offset, self.expected) {
            if o > e {
                notices.push(format!(
                    "tap gap: {} bytes dropped before offset {o} (this tap's queue)",
                    o - e
                ));
            } else if o < e {
                notices.push(format!(
                    "tap warning: offset went backwards to {o}, expected {e}"
                ));
            }
        }
        self.expected = Some(offset.unwrap_or(self.expected.unwrap_or(0)) + len as u64);
        notices
    }
}

/// Handle the terminal `tap.closed` notification (§17) — the exit path `tap_stream`
/// used to lack (review 32, CTL-1).
///
/// The daemon sends this when the graph drops the tapped endpoint (`teardown`,
/// `load --replace`, `remove-node --cascade`) and then deliberately *keeps the
/// connection alive* for the connection's other taps and its subscription. A client
/// that ignores it therefore blocks in `read_line` forever on a connection that will
/// never carry another byte — the exact hang this notification was introduced to end,
/// and a permanent one: the hub is gone and a re-`load` does not revive that tap id.
///
/// **Exit status** is the deliberate part. The bytes already written are intact and
/// flushed, so with no `--bytes` budget outstanding this is an orderly end of stream
/// and the CLI exits **0**, naming the endpoint and the `reason` on stderr beside the
/// `tap opened:` line. With `--bytes N` still outstanding it is a short read — the
/// operator asked for N bytes and the stream ended at fewer — so it exits **1** through
/// the ordinary error path, which `--json` renders as a client-origin error envelope
/// like every other client-side failure (review 26, CLI-4).
fn tap_closed(params: Option<&Value>, written: u64, stop_bytes: Option<u64>) -> anyhow::Result<()> {
    let field = |key: &str| {
        params
            .and_then(|p| p.get(key))
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_owned()
    };
    let (endpoint, reason) = (field("endpoint"), field("reason"));
    if let Some(limit) = stop_bytes {
        return Err(short_read(
            &format!("tap closed: {endpoint} ({reason})"),
            written,
            limit,
        ));
    }
    eprintln!("tap closed: {endpoint} ({reason}) — {written} byte(s) received");
    Ok(())
}

/// The short-read error every "the stream ended before the `--bytes` budget did" path
/// raises: `what` names how the stream ended, the tail names the shortfall.
///
/// One helper because there are **two** such paths and they used to disagree. The
/// terminal `tap.closed` errored per the documented rule, while a plain connection EOF
/// — a daemon crash, an abrupt close — simply broke the read loop and fell through to
/// `Ok(())`: exit 0 over a silently truncated capture, which is the one outcome
/// `--bytes N` exists to make impossible (37-TOOL-1). The condition is identical, so
/// the status and the sentence are now identical too.
fn short_read(what: &str, written: u64, limit: u64) -> anyhow::Error {
    anyhow::anyhow!("{what} — {written} of {limit} requested byte(s) received")
}

/// Open the socket, `tap.open` the endpoint, and write each `tap.data`
/// notification's decoded bytes to stdout as they arrive (§17). The connection's
/// write half stays open for the tap's lifetime (so the daemon does not treat it
/// as a dropped waiter, §15.20). Exits after `stop_bytes` decoded bytes, on the
/// terminal `tap.closed` ([`tap_closed`], which owns the exit status), or when the
/// daemon closes the connection. The `tap.open` acknowledgement (carrying the tap
/// id and `replay_bytes`) is reported on stderr, keeping stdout a clean byte stream —
/// as are the discontinuity notices [`TapContinuity`] raises.
fn tap_stream(
    socket: &Path,
    endpoint: &str,
    replay: bool,
    stop_bytes: Option<u64>,
    stall_ms: Option<u64>,
) -> anyhow::Result<()> {
    let stream = UnixStream::connect(socket)
        .map_err(|e| anyhow::anyhow!("connecting to {}: {e}", socket.display()))?;
    // Hold the write half open for the whole tap so the daemon keeps the connection
    // alive (a half-close reads as a dropped connection, §15.20).
    let mut writer = stream.try_clone()?;
    let params = json!({ "endpoint": endpoint, "replay": replay });
    writer.write_all(
        serial_nexus_rpc::to_line(&Request::new(1, "tap.open", Some(params))).as_bytes(),
    )?;
    writer.flush()?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let mut written = 0u64;
    let limit = stop_bytes.unwrap_or(u64::MAX);
    let mut stdout = std::io::stdout().lock();

    // Read the tap.open acknowledgement FIRST — before any byte loop — so a failed
    // open (unknown or non-host-facing endpoint) exits non-zero, and `--bytes 0` is a
    // clean confirmed no-op rather than a silent success (audit finding). The daemon
    // replies to tap.open before it streams any tap.data, so the ack is the first
    // line; tolerate a stray notification ahead of it defensively.
    let mut continuity = loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            anyhow::bail!("connection closed before the tap.open acknowledgement");
        }
        if let Ok(Incoming::Response(resp)) = serde_json::from_str::<Incoming>(line.trim()) {
            if let Some(err) = resp.error {
                anyhow::bail!("tap.open failed: {} ({})", err.message, err.code);
            }
            let ack = resp.result.unwrap_or(Value::Null);
            eprintln!("tap opened: {ack}");
            // `from_offset` is where this tap's stream begins (plan §11.8) — the anchor the
            // offset-continuity check below counts from, so a drop before the very
            // first chunk is visible too (review 32, CTL-2).
            break TapContinuity::new(ack.get("from_offset").and_then(Value::as_u64));
        }
    };

    // A paused tab (§17): hold the tap open without reading for the stall window so
    // the daemon's bounded queue fills and drops with a counter, then exit.
    if let Some(ms) = stall_ms {
        std::thread::sleep(std::time::Duration::from_millis(ms));
        return Ok(());
    }

    // Stream tap.data (base64) to stdout until the byte limit or the connection close.
    while written < limit {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            // Clean EOF: the daemon closed the connection without a `tap.closed` —
            // it crashed, was killed, or dropped us. With a budget outstanding that is
            // the same short read `tap_closed` errors on, so it takes the same exit
            // (37-TOOL-1); unbounded, it is an orderly end of stream and exits 0.
            if let Some(l) = stop_bytes {
                return Err(short_read(
                    &format!("tap stream ended: {endpoint} (connection closed)"),
                    written,
                    l,
                ));
            }
            break; // daemon closed the connection
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(Incoming::Notification(note)) = serde_json::from_str::<Incoming>(trimmed) {
            // The terminal event for this tap — and the connection stays open after it,
            // so ignoring it is an unbounded hang, not a wait (review 32, CTL-1). A CLI
            // tap owns exactly one tap on its own connection, so any `tap.closed` here
            // is ours; `tap_closed` owns the exit status.
            if note.method == "tap.closed" {
                return tap_closed(note.params.as_ref(), written, stop_bytes);
            }
            if note.method != "tap.data" {
                continue; // ignore other id-less notifications
            }
            let params = note.params.as_ref();
            let data = params
                .and_then(|p| p.get("data"))
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("tap.data missing 'data' field"))?;
            let bytes = serial_nexus_rpc::base64_decode(data)
                .ok_or_else(|| anyhow::anyhow!("tap.data 'data' is not valid base64"))?;
            // Announce a hole before writing the bytes that follow it, so the stderr
            // notice orders correctly against a stdout capture (review 32, CTL-2).
            let offset = params.and_then(|p| p.get("offset")).and_then(Value::as_u64);
            let gap_before = params
                .and_then(|p| p.get("gap_before"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            for notice in continuity.observe(offset, gap_before, bytes.len()) {
                eprintln!("{notice}");
            }
            let take = ((limit - written) as usize).min(bytes.len());
            stdout.write_all(&bytes[..take])?;
            stdout.flush()?;
            written += take as u64;
        }
    }
    Ok(())
}

fn call(socket: &Path, method: &str, params: Option<Value>) -> anyhow::Result<Response> {
    let stream = UnixStream::connect(socket)
        .map_err(|e| anyhow::anyhow!("connecting to {}: {e}", socket.display()))?;
    let mut writer = stream.try_clone()?;
    let request = Request::new(1, method, params);
    writer.write_all(serial_nexus_rpc::to_line(&request).as_bytes())?;
    writer.flush()?;

    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line)?;
    if line.trim().is_empty() {
        anyhow::bail!("daemon closed the connection without replying");
    }
    Ok(serde_json::from_str(line.trim())?)
}

/// Mirror the daemon's §10 socket-path policy exactly, then fall back to the
/// pre-§15.40 default when — and only when — nothing is listening at the current one
/// (plan §17.3, one release).
///
/// The order matters: the current name wins whenever it exists, so a fresh daemon is
/// never passed over in favour of a stale socket left by an older one. The fallback
/// only rescues the case where a daemon from before the rename is still running and
/// the operator has just installed the new CLI.
fn resolve_socket(override_path: Option<PathBuf>) -> PathBuf {
    if let Some(p) = override_path {
        return p;
    }
    let current = default_socket(serial_nexus_rpc::DAEMON_NAME);
    if !current.exists() {
        let legacy = default_socket(serial_nexus_rpc::LEGACY_DAEMON_NAME);
        if legacy.exists() {
            return legacy;
        }
    }
    current
}

/// The §10 default socket path for a daemon spelled `name`.
///
/// One line, because `ctl` looking somewhere the daemon did not bind is the whole
/// failure this policy can produce — so it is computed by the same function the daemon
/// binds through (`serial_nexus_rpc::socket`), not by a second copy that agrees today
/// (notes §3.72).
fn default_socket(name: &str) -> PathBuf {
    serial_nexus_rpc::default_socket_path(name).0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(text: &str) -> GraphConfig {
        toml::from_str(text).expect("test configuration parses")
    }

    /// `add-node` adds one node and no edges (§11). A file with more must be
    /// refused, naming the counts — never "add the first node and drop the rest",
    /// because silently dropping configuration the operator wrote is the defect (§15.35 makes a dropped edge recoverable via `connect`, but only if you are told).
    /// Review 26, CLI-2.
    #[test]
    fn add_node_refuses_files_carrying_more_than_one_node_or_any_edge() {
        let file = Path::new("rig.toml");

        let one = cfg(
            "[[node]]\ntype = \"log\"\nname = \"a\"\ndirectory = \"/tmp\"\nfilename = \"a.log\"\n",
        );
        assert!(single_node(&one, file).is_ok());

        let two = cfg(
            "[[node]]\ntype = \"log\"\nname = \"a\"\ndirectory = \"/tmp\"\nfilename = \"a.log\"\n\
             [[node]]\ntype = \"log\"\nname = \"b\"\ndirectory = \"/tmp\"\nfilename = \"b.log\"\n",
        );
        let err = single_node(&two, file).unwrap_err().to_string();
        assert!(err.contains("2 node(s)"), "counts not named: {err}");
        assert!(err.contains("nothing was added"), "no reassurance: {err}");
        assert!(err.contains("load"), "does not point at load: {err}");

        // One node plus an edge: the edge is the part that would vanish unannounced.
        let edged = cfg(
            "[[node]]\ntype = \"log\"\nname = \"a\"\ndirectory = \"/tmp\"\nfilename = \"a.log\"\n\
             [[edge]]\na = \"usb0\"\nb = \"a\"\n",
        );
        let err = single_node(&edged, file).unwrap_err().to_string();
        assert!(err.contains("1 edge(s)"), "edge count not named: {err}");

        let none = cfg("");
        assert!(
            single_node(&none, file)
                .unwrap_err()
                .to_string()
                .contains("no [[node]]")
        );
    }

    /// `--assert false` must reach the RPC as `assert: false` — the documented
    /// low-then-high pulse (§7.1, `docs/rpc/serial-signals.md`). A clap-inferred
    /// `SetTrue` flag could only ever send `true` (review 26, CLI-1/DOC-4).
    #[test]
    fn pulse_dtr_accepts_an_explicit_assert_value_in_both_spellings() {
        for argv in [
            vec!["serial-nexus-ctl", "pulse-dtr", "usb0", "--assert", "false"],
            vec!["serial-nexus-ctl", "pulse-dtr", "usb0", "--assert=false"],
        ] {
            let cli = Cli::try_parse_from(argv.clone()).expect("--assert false parses");
            let (method, params) = build_request(&cli.cmd).unwrap();
            assert_eq!(method, "pulse-dtr");
            assert_eq!(params.unwrap()["assert"], json!(false), "argv {argv:?}");
        }

        // The default is unchanged: a bare `pulse-dtr` still asserts high.
        let cli = Cli::try_parse_from(["serial-nexus-ctl", "pulse-dtr", "usb0"]).unwrap();
        let (_, params) = build_request(&cli.cmd).unwrap();
        let params = params.unwrap();
        assert_eq!(params["assert"], json!(true));
        assert_eq!(params["ms"], json!(100));

        // And a *bare* `--assert` still means `true`. It was a harmless no-op in
        // operator scripts before `ArgAction::Set` landed; making the CLI-1 fix turn
        // it into a hard error would have been a gratuitous break, so `num_args`
        // plus `default_missing_value` keep the spelling alive.
        let cli =
            Cli::try_parse_from(["serial-nexus-ctl", "pulse-dtr", "usb0", "--assert"]).unwrap();
        let (_, params) = build_request(&cli.cmd).unwrap();
        assert_eq!(params.unwrap()["assert"], json!(true), "bare --assert");
    }

    /// `tap.data` carries two independent discontinuity signals and the CLI reported
    /// neither, so a `serial-nexus-ctl tap … > incident.bin` capture was holed with
    /// nothing on stdout or stderr (review 32, CTL-2). Each must raise exactly one
    /// stderr notice naming its size and the offset it precedes, and a contiguous
    /// stream must stay silent — a notice on every chunk would be as useless as none.
    ///
    /// End to end this is not reproducible on demand: `gap_before` needs the
    /// producer→hub feed to overflow and the offset jump needs *this* tap's queue to,
    /// neither of which a promptly-draining CLI can be made to cause deterministically.
    /// So the rule lives in one small type and is pinned here rather than in a timing
    /// race — see `itest/tests/p12_ctl_tap.rs` for the CTL-1 end-to-end guard.
    #[test]
    fn tap_continuity_reports_feed_gaps_and_queue_drops_and_stays_silent_otherwise() {
        let mut c = TapContinuity::new(Some(100));
        assert!(
            c.observe(Some(100), 0, 10).is_empty(),
            "the first chunk at from_offset is contiguous"
        );
        assert!(c.observe(Some(110), 0, 10).is_empty(), "a contiguous chunk");

        // A feed-hop hole: offsets stay contiguous *across* it by design, so
        // `gap_before` is its only signal (docs/rpc/observation.md, offset contract).
        let notes = c.observe(Some(120), 4096, 10);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("4096"), "size not named: {}", notes[0]);
        assert!(
            notes[0].contains("offset 120"),
            "offset not named: {}",
            notes[0]
        );

        // A per-tap queue drop instead: 30 bytes of offset space vanish (130 expected).
        let notes = c.observe(Some(160), 0, 10);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(
            notes[0].contains("30 bytes"),
            "size not named: {}",
            notes[0]
        );
        assert!(notes[0].contains("queue"), "hop not named: {}", notes[0]);

        // The two are independent: both at once are two notices, not one.
        let mut both = TapContinuity::new(Some(0));
        assert_eq!(both.observe(Some(64), 8, 4).len(), 2);

        // A backwards offset is a daemon bug, not loss — say so rather than swallow it.
        let mut back = TapContinuity::new(Some(100));
        let notes = back.observe(Some(40), 0, 4);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("backwards"), "{}", notes[0]);

        // Offsets absent disables the offset check alone; `gap_before` still reports.
        let mut bare = TapContinuity::new(None);
        assert!(bare.observe(None, 0, 4).is_empty());
        assert_eq!(bare.observe(None, 7, 4).len(), 1);
    }

    /// `tap.closed` is terminal for the tap while the connection stays open, so the CLI
    /// must return rather than block in `read_line` (review 32, CTL-1). The documented
    /// exit status: an orderly end of stream is `0` and names the endpoint and reason
    /// on stderr; an outstanding `--bytes` budget is a short read and errors, so a
    /// script cannot mistake a partial capture for the one it asked for.
    #[test]
    fn tap_closed_exits_zero_unbounded_and_errors_on_an_outstanding_byte_budget() {
        let params = json!({ "tap": 0, "endpoint": "console", "reason": "graph replaced" });
        assert!(tap_closed(Some(&params), 4096, None).is_ok());

        let err = tap_closed(Some(&params), 4096, Some(65536))
            .unwrap_err()
            .to_string();
        assert!(err.contains("console"), "endpoint not named: {err}");
        assert!(err.contains("graph replaced"), "reason not named: {err}");
        assert!(err.contains("4096 of 65536"), "counts not named: {err}");

        // A notification with no params at all must still terminate the stream.
        assert!(tap_closed(None, 0, None).is_ok());
    }

    /// The sibling signal verb takes `Option<bool>` values, which clap already
    /// parses as `--dtr false` (and omitting a line leaves it untouched, §7.1).
    /// Pinned so the CLI-1 shape cannot reappear here.
    #[test]
    fn set_modem_takes_explicit_levels_and_leaves_omitted_lines_null() {
        let cli = Cli::try_parse_from(["serial-nexus-ctl", "set-modem", "usb0", "--dtr", "false"])
            .unwrap();
        let (method, params) = build_request(&cli.cmd).unwrap();
        assert_eq!(method, "set-modem");
        let params = params.unwrap();
        assert_eq!(params["dtr"], json!(false));
        assert_eq!(params["rts"], Value::Null);
    }
}
