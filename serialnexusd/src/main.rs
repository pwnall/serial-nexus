#![deny(unsafe_code)]

//! `serialnexusd` — the serial_nexus daemon.
//!
//! Unsafe is denied crate-wide and localized with `#[allow(unsafe_code)]` to the
//! `sys` module, which isolates the raw ioctls nix/serial2 don't wrap (§2). The
//! data plane runs on a current-thread tokio runtime; control-plane connections
//! are tasks on the same runtime, so mutations serialize for free (§5, plan §2).
//!
//! Phase 2 walking skeleton: control socket + JSON-RPC (`load`/`dump`/`state`),
//! serial and PTY node lifecycle. Byte flow and presence gating land next.

mod control;
mod daemon;
mod nodes;
mod runtime;
mod sys;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::Context;
use clap::Parser;
use nexus_core::config::GraphConfig;
use serde_json::json;
use tokio::net::{UnixListener, UnixStream};
use tokio::signal::unix::{SignalKind, signal};

use daemon::Daemon;

/// How often the daemon publishes a state snapshot to `subscribe` streams (§10).
/// Fine-grained enough to observe counter movement, coarse enough to stay cheap.
const SNAPSHOT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

#[derive(Parser)]
#[command(name = "serialnexusd", version, about = "serial_nexus daemon (§10)")]
struct Cli {
    /// Override the control socket path (§10: default /run/serialnexusd.sock as
    /// root, else $XDG_RUNTIME_DIR/serialnexusd.sock).
    #[arg(long)]
    socket: Option<PathBuf>,
    /// TOML configuration file to load at startup (load-on-empty, §11).
    #[arg(long, short)]
    config: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, run(cli))
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let socket_path = resolve_socket(cli.socket);
    prepare_socket(&socket_path).await?;

    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("binding control socket {}", socket_path.display()))?;
    // Socket permissions ARE the authorization model (§10): 0600 by default.
    std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))?;
    tracing::info!(socket = %socket_path.display(), "control socket listening");

    let daemon = Rc::new(Daemon::new());

    // Periodic state snapshots power `subscribe` (§10): status transitions and
    // counter snapshots. The tick no-ops when nobody is subscribed, so it costs
    // nothing on an idle daemon.
    {
        let daemon = daemon.clone();
        tokio::task::spawn_local(async move {
            loop {
                tokio::time::sleep(SNAPSHOT_INTERVAL).await;
                daemon.emit_state_snapshot();
            }
        });
    }

    if let Some(config_path) = &cli.config {
        startup_load(&daemon, config_path)
            .await
            .with_context(|| format!("loading {}", config_path.display()))?;
    }

    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;

    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _addr)) => {
                    tokio::task::spawn_local(control::serve_connection(daemon.clone(), stream));
                }
                Err(e) => tracing::warn!("accept error: {e}"),
            },
            _ = daemon.shutdown.notified() => {
                tracing::info!("shutdown requested over RPC");
                break;
            }
            _ = sigint.recv() => { tracing::info!("SIGINT"); break; }
            _ = sigterm.recv() => { tracing::info!("SIGTERM"); break; }
        }
    }

    // Clean shutdown: release node environment (PTY symlinks, ports) and the
    // control socket (§10).
    daemon.teardown_all();
    let _ = std::fs::remove_file(&socket_path);
    tracing::info!("stopped");
    Ok(())
}

/// The §10 socket path policy: privilege-based default, CLI-overridable.
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

/// The standard stale-socket unlink dance (§10): if the path exists, a live
/// daemon there is an error; a dead one is cleaned up.
async fn prepare_socket(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        match UnixStream::connect(path).await {
            Ok(_) => anyhow::bail!("a daemon is already listening on {}", path.display()),
            Err(_) => {
                std::fs::remove_file(path)
                    .with_context(|| format!("removing stale socket {}", path.display()))?;
            }
        }
    }
    Ok(())
}

async fn startup_load(daemon: &Daemon, config_path: &Path) -> anyhow::Result<()> {
    let text = std::fs::read_to_string(config_path)?;
    let config: GraphConfig = toml::from_str(&text).context("parsing TOML configuration")?;
    let params = json!({ "config": serde_json::to_value(&config)? });
    daemon
        .dispatch("load", Some(params))
        .await
        .map_err(|e| anyhow::anyhow!("load failed: {} (code {})", e.message, e.code))?;
    tracing::info!(nodes = config.nodes.len(), "startup configuration loaded");
    Ok(())
}
