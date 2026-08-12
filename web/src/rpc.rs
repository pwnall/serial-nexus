//! Daemon control-socket helpers (§10). The per-browser bridge (`bridge.rs`) opens
//! and drives its own connection directly; this module holds only the choice of *which*
//! socket to drive — the `--socket` override, the §17.3 rename window, and a one-line
//! delegation to the single socket-path policy — so the web console reaches the same
//! socket the CLI does.

use std::path::PathBuf;

/// The control socket the console will drive: an explicit `--socket` when given, else
/// the §10 default, falling back to the pre-§15.40 default when — and only when — there
/// is nothing at the current one (plan §17.3, one release).
///
/// The order matters: the current name wins whenever it exists, so a fresh daemon is
/// never passed over in favour of a stale socket left by an older one. It is the same
/// order `ctl` uses, so a browser and a terminal pointed at the same machine always
/// reach the same daemon.
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
///
/// One line, because a console looking somewhere the daemon did not bind is the whole
/// failure this policy can produce — so it is computed by the same function the daemon
/// binds through (`serial_nexus_rpc::socket`), not by a second copy that agrees today
/// (notes §3.72).
///
/// The copy that used to live here is why that rule is absolute rather than stylistic.
/// It had **two** arms where the policy has three: it never asked whether the process
/// was root, so it read `XDG_RUNTIME_DIR` first and handed everyone without a usable one
/// the *root* arm `/run/<name>.sock`. An unprivileged console with the variable unset or
/// exported empty therefore pointed at a path only a root daemon ever binds, while the
/// daemon beside it was at `/tmp/<name>-<uid>.sock` — and it said so through the 503 the
/// bridge answers an upgrade with, naming a socket nothing on that machine will ever
/// create, under a `--help` line promising the daemon's defaults. That is every
/// unprivileged macOS session (no `XDG_RUNTIME_DIR`, no `/run`) and any stripped service
/// environment that exports the variable empty.
fn default_socket(name: &str) -> PathBuf {
    serial_nexus_rpc::default_socket_path(name).0
}
