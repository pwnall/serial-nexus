#![forbid(unsafe_code)]

//! `nexus-daemon` — the serial_nexus daemon as an **embeddable library** (§15.26).
//!
//! The daemon is a library with a thin binary. `serialnexusd` (the in-tree binary)
//! parses flags, installs a tracing subscriber, and calls [`run`] with the built-in
//! codec [`Registry`]; a *closed-source* daemon does the same dozen lines but
//! registers its own codecs first — source-level composition, no dynamic loading
//! (§15.11/§15.26). Everything else in the ecosystem — `serialnexusctl`,
//! `nexus-sim`, `nexus-doctor`, the validation scripts — works against either
//! binary unchanged, because they speak the RPC surface and the envelope, never the
//! codec list (§15.16).
//!
//! **The entry surface is all of it.** [`run`], [`RunOptions`], the codec
//! [`Registry`] (+ [`CodecFactory`], [`RegistryError`]), and the version constants
//! ([`VERSION`], [`WIRE_VERSION`], [`ENVELOPE_VERSION`]) are the only public API;
//! every internal module is private, so the daemon's guts are free to change
//! without breaking an embedder. That narrowness is the second, deliberately tiny
//! stability surface §15.26 accepts (the first being `codec-api`).
//!
//! Unsafe is *forbidden* crate-wide: every raw ioctl/`ptsname`/`poll(2)` wrapper
//! lives in the shared `nexus-sys` crate (§16.3). The data plane runs on a
//! current-thread tokio runtime; control-plane connections are tasks on the same
//! runtime, so mutations serialize for free (§5, plan §2).

mod boundary;
mod cell;
mod control;
mod daemon;
mod nodes;
mod registry;
mod runtime;
mod tap;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::Context;
use nexus_core::config::GraphConfig;
use serde_json::json;
use tokio::net::{UnixListener, UnixStream};
use tokio::signal::unix::{SignalKind, signal};

use daemon::Daemon;

pub use registry::{CodecFactory, Registry, RegistryError};

/// The daemon (library) version, reported by the `info` verb (§10/§15.26). A
/// custom binary embedding this library reports *this* engine version, which is
/// what determines wire and behavior compatibility.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The daemon-to-daemon wire protocol version (§9), re-exported so an embedder and
/// the `info` verb name one constant. Versioned independently of the envelope.
pub use codec_api::WIRE_VERSION;

/// The exec-codec envelope version (§8/§15.15), re-exported for the `info` verb.
/// A codec author pins against this; wire evolution must never break it.
pub use codec_api::ENVELOPE_VERSION;

/// How often the daemon publishes a state snapshot to `subscribe` streams (§10).
/// Fine-grained enough to observe counter movement, coarse enough to stay cheap.
const SNAPSHOT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

/// The options a daemon binary hands to [`run`] — the library twin of the CLI
/// flags (§10). Flag parsing itself stays in the binary (§15.16/§15.26), so an
/// embedder is free to expose a different flag surface, a config file, or none.
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Override the control socket path (§10). `None` selects the privilege-based
    /// default (`/run/serialnexusd.sock` as root, else `$XDG_RUNTIME_DIR/…`).
    pub socket: Option<PathBuf>,
    /// A TOML configuration file to load at startup (load-on-empty, §11), used only
    /// when no persisted state file is present.
    pub config: Option<PathBuf>,
    /// Widen the control socket to a group (§10): chgrp + relax the mode to 0660.
    /// `None` keeps the 0600 owner-only default.
    pub socket_group: Option<String>,
    /// Root prefix for device-identity resolution (§12) — a fixture seam in tests
    /// (`sys_root` is `<dev-root>/sys`), `/` in production.
    pub dev_root: PathBuf,
    /// Configuration snapshot path (§11). `None` derives it from the socket path
    /// (`<socket>.state.toml`); an explicit path buys reboot durability.
    pub state_file: Option<PathBuf>,
}

impl Default for RunOptions {
    /// `dev_root` defaults to `/` (the production root), not `PathBuf::default()`
    /// (empty): an embedder calling `RunOptions::default()` gets a resolver rooted at
    /// the real filesystem, not one silently rooted at the daemon's CWD (§12/§15.26).
    fn default() -> Self {
        RunOptions {
            socket: None,
            config: None,
            socket_group: None,
            dev_root: PathBuf::from("/"),
            state_file: None,
        }
    }
}

/// Run the daemon to completion: bind the control socket, apply the §10 auth
/// policy, honor the startup config/state-file preference (§11/§15.9), serve RPC,
/// and tear down cleanly on `shutdown`/SIGINT/SIGTERM. `registry` is the set of
/// codecs this daemon can instantiate (§8/§15.26).
///
/// The daemon is single-threaded by construction (§5): this builds a current-thread
/// tokio runtime and a `LocalSet` internally, so an embedder's `main` never touches
/// the runtime. The whole in-tree binary is:
///
/// ```no_run
/// # fn parse_dev_root() -> std::path::PathBuf { "/".into() }
/// fn main() -> anyhow::Result<()> {
///     let registry = nexus_daemon::Registry::with_builtins();
///     let options = nexus_daemon::RunOptions {
///         dev_root: parse_dev_root(),
///         ..Default::default()
///     };
///     nexus_daemon::run(options, registry)
/// }
/// ```
///
/// A closed-source daemon inserts one line — `let registry =
/// Registry::with_builtins().register("myproto", my_factory)?;` — and is otherwise
/// identical (§15.26).
pub fn run(options: RunOptions, registry: Registry) -> anyhow::Result<()> {
    // The data plane is single-threaded (§5/§15.19): a current-thread runtime with
    // a LocalSet so every `!Send` node task and the control plane share one thread,
    // and mutations serialize for free (plan §2). Owned here so the binary's `main`
    // never mentions tokio.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, serve(options, registry))
}

async fn serve(options: RunOptions, registry: Registry) -> anyhow::Result<()> {
    let socket_path = resolve_socket(options.socket);
    prepare_socket(&socket_path).await?;

    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("binding control socket {}", socket_path.display()))?;
    // Socket permissions ARE the authorization model (§10): 0600 by default,
    // widened to a group by --socket-group. The parent runtime dir (0700) bounds
    // the brief post-bind window before this applies (v4 audit).
    apply_socket_perms(&socket_path, options.socket_group.as_deref())?;
    tracing::info!(socket = %socket_path.display(), "control socket listening");

    let state_file = resolve_state_file(options.state_file.clone(), &socket_path);
    let daemon = Rc::new(Daemon::new(
        nexus_core::Resolver::new(options.dev_root.clone()),
        Some(state_file.clone()),
        Rc::new(registry),
    ));

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

    // Startup config preference (§11/§15.9): a persisted state file, if present,
    // is the source of truth (it captured incremental surgery a `--config` file
    // never saw); otherwise fall back to the CLI `--config`. Restart, replug, and
    // first boot become one code path.
    if state_file.exists() {
        startup_load(&daemon, &state_file)
            .await
            .with_context(|| format!("loading state file {}", state_file.display()))?;
    } else if let Some(config_path) = &options.config {
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

/// Apply the §10 control-socket authorization policy: mode 0600 by default, or
/// mode 0660 owned by `group` when one is configured (`--socket-group`). The
/// group is resolved first, so a name that cannot be found is a hard error before
/// anything is changed; mirrors the PTY slave's group logic (§7.2).
fn apply_socket_perms(path: &Path, group: Option<&str>) -> anyhow::Result<()> {
    if let Some(group) = group {
        let gid = nix::unistd::Group::from_name(group)
            .ok()
            .flatten()
            .map(|g| g.gid)
            .ok_or_else(|| anyhow::anyhow!("socket group {group:?} not found"))?;
        nix::unistd::chown(path, None, Some(gid))
            .with_context(|| format!("chgrp {} to {group}", path.display()))?;
    }
    let mode = if group.is_some() { 0o660 } else { 0o600 };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .with_context(|| format!("setting mode on control socket {}", path.display()))?;
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
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR")
        && !dir.is_empty()
    {
        return PathBuf::from(dir).join("serialnexusd.sock");
    }
    PathBuf::from(format!("/tmp/serialnexusd-{}.sock", nix::unistd::getuid()))
}

/// The §11/§15.9 state-file path policy: CLI-overridable, else derived from the
/// control-socket path so it shares the socket's uniqueness (one daemon per
/// socket) and lifecycle. The daemon owns this writable path and prefers it over
/// `--config` at startup, so a restart recovers incremental surgery. A deployment
/// that wants the snapshot to survive a *reboot* (the runtime dir is cleared then)
/// passes `--state-file` pointing at a persistent path (e.g. /var/lib).
fn resolve_state_file(override_path: Option<PathBuf>, socket_path: &Path) -> PathBuf {
    if let Some(p) = override_path {
        return p;
    }
    let stem = socket_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("serialnexusd");
    socket_path.with_file_name(format!("{stem}.state.toml"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dev_root_is_the_filesystem_root_not_cwd() {
        // An embedder writing `RunOptions::default()` must get a resolver rooted at
        // `/`, not one silently rooted at the daemon's CWD (CTRL-2 / §12).
        assert_eq!(RunOptions::default().dev_root, PathBuf::from("/"));
    }
}
