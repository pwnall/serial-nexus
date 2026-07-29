#![forbid(unsafe_code)]

//! `serial-nexus-daemon` — the in-tree serial_nexus daemon binary.
//!
//! Deliberately thin (§15.26): it parses flags, installs a tracing subscriber, and
//! calls [`serial_nexus_daemon::run`] with the built-in codec [`Registry`]. Everything
//! that makes the daemon a daemon — boundary nodes, the data plane, the control
//! plane, the state file, the codec registry — lives in the `serial-nexus-daemon` library,
//! so an out-of-tree daemon can register its own codecs and reuse all of it with
//! the same dozen lines. Nothing here may depend on how those internals work.

use std::path::PathBuf;

use clap::Parser;
use serial_nexus_daemon::{Registry, RunOptions};

#[derive(Parser)]
#[command(
    name = "serial-nexus-daemon",
    version,
    about = "serial_nexus daemon (§10)"
)]
struct Cli {
    /// Override the control socket path (§10: default /run/serial-nexus-daemon.sock as
    /// root, else $XDG_RUNTIME_DIR/serial-nexus-daemon.sock).
    #[arg(long)]
    socket: Option<PathBuf>,
    /// TOML configuration file to load at startup (load-on-empty, §11).
    #[arg(long, short)]
    config: Option<PathBuf>,
    /// Widen the control socket to a group (§10: "flags to widen to a group"):
    /// chgrp the socket to this group and relax its mode to 0660. Unset keeps the
    /// 0600 owner-only default — whoever can open the socket owns every console.
    #[arg(long)]
    socket_group: Option<String>,
    /// Root prefix for device-identity resolution (§12) — a test seam for fixture
    /// `/dev/serial/by-id` and sysfs trees (`sys_root` is `<dev-root>/sys`).
    /// Defaults to `/`.
    #[arg(long, default_value = "/")]
    dev_root: PathBuf,
    /// Configuration snapshot path (§11): the daemon writes the current config here
    /// after each successful config mutation and prefers it at startup, so
    /// incremental surgery survives a restart. Defaults to a file beside the control
    /// socket, named from the socket's *stem* — `/run/serial-nexus-daemon.sock` yields
    /// `/run/serial-nexus-daemon.state.toml`, not `serial-nexus-daemon.sock.state.toml`. It shares
    /// the socket's lifecycle, so under /run or $XDG_RUNTIME_DIR it is cleared on
    /// reboot: pass an explicit path (e.g. /var/lib/serial-nexus-daemon/state.toml) for
    /// reboot-durable persistence, and pass one to each daemon if two sockets differ
    /// only in extension (`a.sock` and `a.socket` derive the same state file).
    #[arg(long)]
    state_file: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let options = RunOptions {
        socket: cli.socket,
        config: cli.config,
        socket_group: cli.socket_group,
        dev_root: cli.dev_root,
        state_file: cli.state_file,
    };
    // The built-in codec registry (§8/§15.26). An out-of-tree daemon replaces this
    // one line with `Registry::with_builtins().register("myproto", factory)?`.
    let registry = Registry::with_builtins();
    serial_nexus_daemon::run(options, registry)
}
