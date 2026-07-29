//! Daemon control-socket helpers (§10). The per-browser bridge (`bridge.rs`) opens
//! and drives its own connection directly; this module holds only the shared
//! socket-path policy so the web console reaches the same socket the CLI does.

use std::path::PathBuf;

/// Mirror the daemon's §10 socket-path policy: `$XDG_RUNTIME_DIR/serial-nexus-daemon.sock`
/// for a normal user, else `/run/serial-nexus-daemon.sock`. `--socket` overrides it.
///
/// Falls back to the pre-§15.40 spelling only when the current one does not exist
/// (plan §17.3, one release) — the same order the CLI uses, so a browser and a terminal
/// pointed at the same machine always reach the same daemon.
pub fn resolve_socket(override_path: Option<PathBuf>) -> PathBuf {
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
fn default_socket(name: &str) -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR")
        && !dir.is_empty()
    {
        return PathBuf::from(dir).join(format!("{name}.sock"));
    }
    PathBuf::from(format!("/run/{name}.sock"))
}
