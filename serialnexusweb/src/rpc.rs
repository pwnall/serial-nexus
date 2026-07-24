//! Daemon control-socket helpers (§10). The per-browser bridge (`bridge.rs`) opens
//! and drives its own connection directly; this module holds only the shared
//! socket-path policy so the web console reaches the same socket the CLI does.

use std::path::PathBuf;

/// Mirror the daemon's §10 socket-path policy: `$XDG_RUNTIME_DIR/serialnexusd.sock`
/// for a normal user, else `/run/serialnexusd.sock`. `--socket` overrides it.
pub fn resolve_socket(override_path: Option<PathBuf>) -> PathBuf {
    if let Some(p) = override_path {
        return p;
    }
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR")
        && !dir.is_empty()
    {
        return PathBuf::from(dir).join("serialnexusd.sock");
    }
    PathBuf::from("/run/serialnexusd.sock")
}
