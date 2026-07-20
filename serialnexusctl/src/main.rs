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
use nexus_rpc::{Request, Response};
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
    /// Tear down the whole graph.
    Teardown,
    /// Ask the daemon to shut down.
    Shutdown,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let socket = resolve_socket(cli.socket.clone());

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
        Cmd::Teardown => {
            let n = result.get("torn_down").and_then(Value::as_u64).unwrap_or(0);
            println!("tore down {n} node(s)");
        }
        Cmd::Shutdown => println!("shutdown requested"),
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
