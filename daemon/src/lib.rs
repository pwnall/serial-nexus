#![forbid(unsafe_code)]

//! `serial-nexus-daemon` — the serial_nexus daemon as an **embeddable library** (§15.26).
//!
//! The daemon is a library with a thin binary. `serial-nexus-daemon` (the in-tree binary)
//! parses flags, installs a tracing subscriber, and calls [`run`] with the built-in
//! codec [`Registry`]; an out-of-tree daemon does the same dozen lines but
//! registers its own codecs first — source-level composition, no dynamic loading
//! (§15.11/§15.26). Everything else in the ecosystem — `serial-nexus-ctl`,
//! `serial-nexus-sim`, `serial-nexus-doctor`, the validation scripts — works against either
//! binary unchanged, because they speak the RPC surface and the envelope, never the
//! codec list (§15.16).
//!
//! **The entry surface is all of it.** [`run`], [`RunOptions`], the codec
//! [`Registry`] (+ [`CodecFactory`], [`RegistryError`]), and the version constants
//! ([`VERSION`], [`WIRE_VERSION`], [`ENVELOPE_VERSION`]) are the only public API;
//! every internal module is private, so the daemon's guts are free to change
//! without breaking an embedder. That narrowness is the second, deliberately tiny
//! stability surface §15.26 accepts (the first being `serial-nexus-codec-api`).
//!
//! Unsafe is *forbidden* crate-wide: every raw ioctl/`ptsname`/`poll(2)` wrapper
//! lives in the shared `serial-nexus-sys` crate (§16.3). The data plane runs on a
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

use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::Context;
use serde_json::json;
use serial_nexus_core::config::GraphConfig;
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
pub use serial_nexus_codec_api::WIRE_VERSION;

/// The exec-codec envelope version (§8/§15.15), re-exported for the `info` verb.
/// A codec author pins against this; wire evolution must never break it.
pub use serial_nexus_codec_api::ENVELOPE_VERSION;

/// **Not part of this crate's supported API. Exposed only so the fuzz harness can
/// reach it, and free to change or disappear in any release, including a patch.**
///
/// §15.26 defines the embeddable surface as exactly `run`/[`RunOptions`]/[`Registry`]
/// plus the version constants, and says everything else stays private. This module is
/// a deliberate, named exception rather than an erosion of that rule — see
/// `docs/implementation-notes.md` §3.19 for the reasoning and the terms.
///
/// The problem it solves: review 26's SEC-7 observed that every fuzz target sat on the
/// `serial-nexus-codec-api` layer, so the parsers an ordinary client reaches over the 0600 control
/// socket — with no leg, no peer, no child process — were unfuzzed. The
/// control-socket line reader is the daemon's front door: it frames *every* byte a
/// client writes, before any JSON is parsed and before any verb is dispatched, and it
/// is the one place a hostile client controls both the content and the chunking. Unit
/// tests cover its contract; they cannot cover its input space.
///
/// If you are an embedder (§15.26): do not use this. Nothing here is semver'd, nothing
/// here is documented as behaviour, and a future release may empty the module.
pub mod unstable_fuzz_api {
    /// The control plane's newline-delimited request framer, generic over its byte
    /// source so a fuzzer can drive it from a `&[u8]` exactly as the socket does.
    ///
    /// Properties worth asserting from a target: it never yields a line longer than
    /// [`MAX_REQUEST_LINE`]; a partial line survives the future being dropped
    /// mid-read (cancel-safety — §15.20 relies on it, and it is what keeps a
    /// pipelined request from being truncated); and it terminates on any input.
    pub use crate::control::{LineRead, MAX_REQUEST_LINE, RequestLines};
}

/// How often the daemon publishes a state snapshot to `subscribe` streams (§10).
/// Fine-grained enough to observe counter movement, coarse enough to stay cheap.
const SNAPSHOT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

/// The options a daemon binary hands to [`run`] — the library twin of the CLI
/// flags (§10). Flag parsing itself stays in the binary (§15.16/§15.26), so an
/// embedder is free to expose a different flag surface, a config file, or none.
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Override the control socket path (§10). `None` selects the privilege-based
    /// default (`/run/serial-nexus-daemon.sock` as root, else `$XDG_RUNTIME_DIR/…`).
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
    /// Configuration snapshot path (§11). `None` derives it from the socket path's
    /// **stem** (`<socket-stem>.state.toml`, so `/run/serial-nexus-daemon.sock` yields
    /// `/run/serial-nexus-daemon.state.toml` — see [`resolve_state_file`]); an explicit path
    /// buys reboot durability.
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
///     let registry = serial_nexus_daemon::Registry::with_builtins();
///     let options = serial_nexus_daemon::RunOptions {
///         dev_root: parse_dev_root(),
///         ..Default::default()
///     };
///     serial_nexus_daemon::run(options, registry)
/// }
/// ```
///
/// An out-of-tree daemon inserts one line — `let registry =
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
    // Held for the whole life of the daemon: it is what makes the probe-unlink-bind
    // dance below indivisible against a second daemon on this path (SEAM-3).
    let socket_claim = prepare_socket(&socket_path).await?;

    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("binding control socket {}", socket_path.display()))?;
    // Socket permissions ARE the authorization model (§10): 0600 by default,
    // widened to a group by --socket-group. The parent runtime dir (0700) bounds
    // the brief post-bind window before this applies (v4 audit).
    apply_socket_perms(&socket_path, options.socket_group.as_deref())?;
    tracing::info!(socket = %socket_path.display(), "control socket listening");

    let state_file = resolve_state_file(options.state_file.clone(), &socket_path);
    let daemon = Rc::new(Daemon::new(
        serial_nexus_core::Resolver::new(options.dev_root.clone()),
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
    //
    // **The two sources fail differently, on purpose** (RV-10). A `--config` file is
    // one the operator *named* on the command line, so a bad one is fail-fast: they
    // are standing there and will fix it. The state file is daemon-owned and nobody
    // asked for it — a version-skewed or hand-edited one that refuses to parse would
    // otherwise leave the operator with no daemon and no consoles for a file they
    // never chose to load. §15.8's spirit is that trouble faults a node rather than
    // removing the graph; here it is louder still (nothing loads), but it must not
    // cost the daemon its life. So: log at `error` with the path and the parse error,
    // note that the file is preserved untouched for inspection, and come up with an
    // empty graph the operator can `load` into.
    // …with one addition the rename forced (plan §17.3). §15.40 moved the default
    // state file with the daemon's name, and a snapshot is not something an operator
    // can be asked to notice: an upgraded daemon would have found nothing at the new
    // path, taken the `else` branch, and come up empty — silently discarding every
    // node built by incremental surgery, which is the one thing the snapshot exists to
    // prevent. So a legacy-named snapshot is *adopted* on read for one release. The
    // daemon still writes at `state_file`, so the very first mutation rewrites it under
    // the new name and this branch never runs again; the old file is left untouched
    // rather than deleted, because a file this daemon did not create in this release is
    // not its to remove — a downgrade should still find it.
    //
    // Adoption covers an explicit `--state-file` too (PKG-1). It once did not, and that
    // exempted the one deployment shape the packaging ships: the unit names its snapshot
    // under `/var/lib`, so every systemd install upgraded per `packaging/README.md`
    // would have taken the `--config` first-boot seed and discarded its surgery — the
    // defect this whole branch exists to prevent, in the shape most likely to meet it.
    let startup_state = if state_file.exists() {
        Some(state_file.clone())
    } else if let Some(legacy) = legacy_state_file(options.state_file.as_deref(), &socket_path)
        && legacy.exists()
    {
        tracing::warn!(
            legacy = %legacy.display(),
            current = %state_file.display(),
            "adopting the pre-rename state file; the next mutation writes the current \
             one and the legacy file can then be removed (plan §17.3, one release only)"
        );
        Some(legacy)
    } else {
        None
    };

    if let Some(startup_state) = startup_state {
        if let Err(e) = startup_load(&daemon, &startup_state, StartupSource::StateFile).await {
            tracing::error!(
                path = %startup_state.display(),
                "state file could not be loaded ({e:#}); starting with an EMPTY graph. \
                 The file is preserved untouched — inspect or remove it, then `load` \
                 your configuration."
            );
        }
    } else if let Some(config_path) = &options.config {
        startup_load(&daemon, config_path, StartupSource::Config)
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
    // control socket (§10). The claim is still held here, which is what makes the
    // unlink safe: no other daemon can have run the dance on this path since we
    // bound, so the inode at `socket_path` is still the one we created (SEAM-3).
    daemon.teardown_all();
    let _ = std::fs::remove_file(&socket_path);
    drop(socket_claim);
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
    let name = serial_nexus_rpc::DAEMON_NAME;
    if nix::unistd::geteuid().is_root() {
        return PathBuf::from(format!("/run/{name}.sock"));
    }
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR")
        && !dir.is_empty()
    {
        return PathBuf::from(dir).join(format!("{name}.sock"));
    }
    PathBuf::from(format!("/tmp/{name}-{}.sock", nix::unistd::getuid()))
}

/// Where a pre-§15.40 daemon would have kept *this* daemon's state snapshot, or `None`
/// when this deployment has no pre-rename shape to migrate from (plan §17.3).
///
/// The rename moved the snapshot in two different ways, so there are two arms:
///
/// * **Derived** (`explicit` is `None`) — the default is `<socket-stem>.state.toml`, so
///   the file moved with the socket's *name*: [`legacy_socket_sibling`].
/// * **Explicit** (`--state-file`) — the packaged unit names
///   `/var/lib/<daemon name>/state.toml`, so the file moved with its *directory* and the
///   derived arm never looks there: [`legacy_renamed_path`].
///
/// Both arms answer `None` for anything the rename could not itself have produced, which
/// is the property that keeps adoption from reaching a file that merely happens to be
/// nearby — see each one for the specific restraint it applies.
fn legacy_state_file(explicit: Option<&Path>, socket_path: &Path) -> Option<PathBuf> {
    match explicit {
        Some(state_file) => legacy_renamed_path(state_file),
        None => legacy_socket_sibling(socket_path),
    }
}

/// The pre-§15.40 default state-file path a *default* socket path would have derived,
/// or `None` when this socket is not one of the defaults.
///
/// The `None` case is the important one: an operator who passed `--socket` chose that
/// name themselves, so its stem never changed and there is nothing to migrate from —
/// inventing a legacy sibling there would let a daemon adopt an unrelated file that
/// merely happens to sit next to its socket. Only the two shapes
/// [`resolve_socket`] actually mints are recognised.
fn legacy_socket_sibling(socket_path: &Path) -> Option<PathBuf> {
    let file = socket_path.file_name()?.to_str()?;
    let current = serial_nexus_rpc::DAEMON_NAME;
    let legacy = serial_nexus_rpc::LEGACY_DAEMON_NAME;
    let legacy_stem = if file == format!("{current}.sock") {
        legacy.to_owned()
    } else {
        let uid = file
            .strip_prefix(&format!("{current}-"))?
            .strip_suffix(".sock")?;
        format!("{legacy}-{uid}")
    };
    Some(socket_path.with_file_name(format!("{legacy_stem}.state.toml")))
}

/// `path` with §15.40's rename **undone**, or `None` when the path never spelled the
/// daemon's name and so has nothing to undo (PKG-1).
///
/// An explicit `--state-file` is a path the operator wrote, which is why the derived
/// arm's reasoning does not carry over: they did not choose the *content* of the name,
/// they copied whatever the packaging told them, and the packaging renamed a directory
/// under them. `/var/lib/serial-nexus-daemon/state.toml` and its pre-rename spelling are
/// the same operator intent written twice.
///
/// The restraint is that only the daemon's own name is respelled, and only where a whole
/// path component *is* that name or is that name plus an extension. A component that
/// merely contains it is left alone, and a path with no such component yields `None`
/// rather than a guess — so the daemon only ever looks at a path the rename itself could
/// have produced, which is the same promise [`legacy_socket_sibling`] makes.
fn legacy_renamed_path(path: &Path) -> Option<PathBuf> {
    let current = serial_nexus_rpc::DAEMON_NAME;
    let legacy = serial_nexus_rpc::LEGACY_DAEMON_NAME;
    let mut renamed = false;
    let mut out = PathBuf::new();
    for component in path.components() {
        let raw = component.as_os_str();
        // `<name>` itself, or `<name>.state.toml` and friends — the extension, if any,
        // rides along unchanged.
        let respelled = raw.to_str().and_then(|name| {
            let rest = name.strip_prefix(current)?;
            (rest.is_empty() || rest.starts_with('.')).then(|| format!("{legacy}{rest}"))
        });
        match respelled {
            Some(name) => {
                renamed = true;
                out.push(name);
            }
            None => out.push(raw),
        }
    }
    renamed.then_some(out)
}

/// The §11/§15.9 state-file path policy: CLI-overridable, else derived from the
/// control-socket path so it shares the socket's uniqueness (one daemon per
/// socket) and lifecycle. The daemon owns this writable path and prefers it over
/// `--config` at startup, so a restart recovers incremental surgery. A deployment
/// that wants the snapshot to survive a *reboot* (the runtime dir is cleared then)
/// passes `--state-file` pointing at a persistent path (e.g. /var/lib).
///
/// **The derived name is `<socket-stem>.state.toml`, not `<socket>.state.toml`**
/// (RV-6): `/run/serial-nexus-daemon.sock` yields `/run/serial-nexus-daemon.state.toml`, not
/// `/run/serial-nexus-daemon.sock.state.toml`. Say *stem* wherever this is documented —
/// an operator deriving the path from the wrong spelling looks for a file that has
/// never existed. The known cost of the stem is that two sockets differing only in
/// extension (`a.sock`, `a.socket`) derive one state file; the tempting one-line
/// repair — [`Path::file_name`] instead of [`Path::file_stem`] — is **not** taken
/// here, because it would silently orphan every deployed daemon's existing
/// `serial-nexus-daemon.state.toml` and come up with an empty graph after an upgrade. A
/// deployment that really runs two daemons out of one directory gives one of them
/// an explicit `--state-file`.
fn resolve_state_file(override_path: Option<PathBuf>, socket_path: &Path) -> PathBuf {
    if let Some(p) = override_path {
        return p;
    }
    let stem = socket_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("serial-nexus-daemon");
    socket_path.with_file_name(format!("{stem}.state.toml"))
}

/// How long a pre-bind dial at the control-socket path may take before the existing
/// socket is read as live. A socket nobody listens on refuses *immediately*
/// (`ECONNREFUSED`), so anything slower is somebody answering, and the safe reading of
/// an ambiguous answer is "not mine to unlink" — the leg's SEC-8 budget, applied to the
/// control socket (SEAM-2).
const SOCKET_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(250);

/// One daemon's exclusive claim on one control-socket path, held from before the
/// stale-socket probe until after the shutdown unlink (SEAM-3).
///
/// §10's "standard stale-socket unlink dance" is three steps — probe, unlink, bind — and
/// two daemons starting from a common stale inode can interleave them: both probe, both
/// see it dead, and the loser's `remove_file` lands *after* the winner's `bind`,
/// unlinking a live socket. Neither reports an error. They then load the same state file
/// and contend for the same devices under `TIOCEXCL`, with the unreachable one typically
/// winning the opens because nothing is talking to it — an operator sees a daemon that
/// answers and whose every port is faulted, and a daemon that answers nothing.
///
/// The mutual exclusion is `flock(2)` on a sibling `<socket>.lock`. `flock` rather than
/// the lock file's mere existence, because the kernel drops it however the holder dies:
/// a SIGKILLed daemon leaves nothing to clean up, where a presence-means-locked file
/// would reintroduce the same stale-artifact problem one level down. For that reason the
/// lock file is never unlinked either — unlinking it races a third daemon that has
/// already opened the inode. It is a zero-byte file with the socket's lifetime; the
/// packaged unit's `RuntimeDirectory=` clears it with the directory.
struct SocketClaim {
    /// Held only for its lifetime: `flock` is released when this file closes, which is
    /// on drop, or on process exit however that exit happens.
    _lock: nix::fcntl::Flock<std::fs::File>,
}

impl SocketClaim {
    fn acquire(socket_path: &Path) -> anyhow::Result<SocketClaim> {
        let mut name = socket_path.as_os_str().to_owned();
        name.push(".lock");
        let path = PathBuf::from(name);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .with_context(|| format!("opening control-socket lock {}", path.display()))?;
        match nix::fcntl::Flock::lock(file, nix::fcntl::FlockArg::LockExclusiveNonblock) {
            Ok(lock) => Ok(SocketClaim { _lock: lock }),
            Err((_, e)) if e == nix::errno::Errno::EWOULDBLOCK => anyhow::bail!(
                "another daemon already claims the control socket {} (its lock {} is \
                 held); one daemon per socket path",
                socket_path.display(),
                path.display()
            ),
            Err((_, e)) => {
                Err(anyhow::Error::new(e).context(format!("locking {}", path.display())))
            }
        }
    }
}

/// Claim `path`, then run §10's stale-socket unlink dance on it — or refuse.
///
/// Three conditions, each one a way the naive dance was wrong:
///
/// 1. **It is a socket.** Checked with `symlink_metadata`, so a symlink is refused as
///    itself rather than followed to whatever it names. The former code unlinked on any
///    connect error, so `--socket /home/me/notes.txt` deleted that file — the same shape
///    the daemon's own unix leg was hardened against (review 26 SEC-8), which the control
///    socket had kept (SEAM-2).
/// 2. **Nobody else is mid-dance.** [`SocketClaim`] makes probe-unlink-bind indivisible
///    against another daemon on this path (SEAM-3).
/// 3. **Nobody is listening on it.** A stale socket refuses a dial immediately
///    (`ECONNREFUSED`); a successful dial means a live daemon owns the path. An answer
///    that is neither prompt nor a refusal is read as live, because that is the reading
///    that fails safe.
///
/// Condition 1 runs *before* the claim on purpose. It is read-only, it races nothing
/// (the only thing another daemon can leave at this path is a socket), and putting it
/// first is what keeps the refusal total: taking the claim would drop a sibling
/// `<path>.lock` next to an operator's file on the way to deciding not to touch it.
async fn prepare_socket(path: &Path) -> anyhow::Result<SocketClaim> {
    let present = match std::fs::symlink_metadata(path) {
        Ok(md) if md.file_type().is_socket() => true,
        Ok(_) => anyhow::bail!(
            "{} exists and is not a unix socket; refusing to unlink it",
            path.display()
        ),
        // Nothing there: the ordinary first-bind path.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => {
            return Err(anyhow::Error::new(e)
                .context(format!("inspecting control socket {}", path.display())));
        }
    };

    // Everything from here — the probe, the unlink, and the caller's `bind` — happens
    // under the claim, which is the whole of what makes those steps indivisible.
    let claim = SocketClaim::acquire(path)?;
    if !present {
        return Ok(claim);
    }
    match tokio::time::timeout(SOCKET_PROBE_TIMEOUT, UnixStream::connect(path)).await {
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
            std::fs::remove_file(path)
                .with_context(|| format!("removing stale socket {}", path.display()))?;
            Ok(claim)
        }
        Ok(Ok(_live)) => anyhow::bail!("a daemon is already listening on {}", path.display()),
        Ok(Err(e)) => anyhow::bail!(
            "probing the existing socket {}: {e}; refusing to unlink it",
            path.display()
        ),
        Err(_) => anyhow::bail!(
            "the existing socket {} did not refuse a dial promptly; refusing to unlink it",
            path.display()
        ),
    }
}

/// Which file a startup load came from — the two differ in exactly two rules
/// (see [`startup_load`] and the `serve` call site).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupSource {
    /// The operator's `--config` file.
    Config,
    /// The daemon-owned `<socket-stem>.state.toml` snapshot (§11/§15.9) — the
    /// socket's *stem*, not the whole socket path ([`resolve_state_file`]).
    StateFile,
}

async fn startup_load(
    daemon: &Daemon,
    config_path: &Path,
    source: StartupSource,
) -> anyhow::Result<()> {
    let text = std::fs::read_to_string(config_path)?;
    let config: GraphConfig = toml::from_str(&text).context("parsing TOML configuration")?;
    // An operator file that parses to *nothing* while its source text says otherwise
    // is a typo, not an intent (CP-2/CFG-3): a mis-typed table name (`[[nodez]]`)
    // yields an empty graph that loads with `{"loaded": 0}` and exit 0, and with
    // `--replace` that is an unannounced `teardown` reported as success — the one
    // input that breaks §15.8's operators-own-the-graph guarantee. Name the real
    // mistake instead.
    //
    // **Never for the state file.** `teardown` persists an empty graph deliberately
    // (§11), and an empty graph is a legal graph; the check exists only where a
    // human wrote the file and a non-empty source proves they meant something.
    if source == StartupSource::Config && config.is_empty() && !text.trim().is_empty() {
        anyhow::bail!("{}", serial_nexus_core::graph::ValidationError::EmptyConfig);
    }
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

    fn scratch(name: &str) -> PathBuf {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("snx-startup-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d.join(name)
    }

    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, f)
    }

    // CP-2/CFG-3 (the second half): a file with content that nonetheless parses to
    // an *empty* graph — everything commented out, or (before `deny_unknown_fields`
    // caught it at the parser) a typo'd table name. From an operator's `--config`
    // file that must be an error naming the mistake: with `--replace` the same input
    // is an unannounced `teardown` reported as success (§11, §15.8).
    #[test]
    fn a_config_file_that_parses_to_nothing_is_refused() {
        let path = scratch("typo.toml");
        std::fs::write(
            &path,
            "# [[node]]\n# type = \"log\"\n# name = \"x\"\n# path = \"/tmp/x.log\"\n",
        )
        .unwrap();
        let daemon = Daemon::default();
        let err = block_on(startup_load(&daemon, &path, StartupSource::Config))
            .expect_err("an empty parse of a non-empty operator file must be refused");
        assert!(
            format!("{err:#}").contains("declares no nodes and no edges"),
            "the error must name the real mistake, got: {err:#}"
        );
        std::fs::remove_file(&path).ok();
    }

    // …and the same emptiness from the *state file* is legal: `teardown` persists an
    // empty graph on purpose (§11), so the rule must not be applied there.
    #[test]
    fn an_empty_state_file_load_is_legal() {
        let path = scratch("state.toml");
        std::fs::write(&path, "# torn down\n").unwrap();
        let daemon = Daemon::default();
        block_on(startup_load(&daemon, &path, StartupSource::StateFile))
            .expect("an empty state file is a legal empty graph");
        std::fs::remove_file(&path).ok();
    }

    // PKG-1: an explicit `--state-file` — the packaged unit's shape — resolves to the
    // same path with §15.40's rename undone, which is where a pre-rename daemon
    // installed from the same packaging actually wrote it. The retired spelling is read
    // from its single definition, never typed here (§15.40's meta-gate).
    #[test]
    fn an_explicit_state_file_maps_to_its_pre_rename_directory() {
        let current = serial_nexus_rpc::DAEMON_NAME;
        let legacy = serial_nexus_rpc::LEGACY_DAEMON_NAME;
        let explicit = PathBuf::from(format!("/var/lib/{current}/state.toml"));
        assert_eq!(
            legacy_state_file(Some(&explicit), Path::new("/run/unused.sock")),
            Some(PathBuf::from(format!("/var/lib/{legacy}/state.toml"))),
            "the packaged --state-file must map to the directory the pre-rename unit \
             wrote, or adoption is dead in the deployment the packaging ships"
        );
    }

    // …and the restraint that makes that safe: only the daemon's own name is respelled,
    // and only where a whole component *is* it. Anything else has no legacy twin, so the
    // daemon never goes looking beside a path an operator chose for their own reasons.
    #[test]
    fn an_explicit_state_file_that_never_spelled_the_name_has_no_legacy_twin() {
        let current = serial_nexus_rpc::DAEMON_NAME;
        for unrelated in [
            "/srv/lab/graph.toml".to_owned(),
            // Contains the name, but is not it: a sibling install directory.
            format!("/var/lib/{current}-standby/state.toml"),
        ] {
            assert_eq!(
                legacy_state_file(Some(Path::new(&unrelated)), Path::new("/run/unused.sock")),
                None,
                "{unrelated} was given a pre-rename twin it never had"
            );
        }
    }

    // A comment-only `--config` file is the same shape as the typo and gets the same
    // answer; a genuinely empty file is not a mistake anyone can name, so it loads.
    #[test]
    fn a_wholly_empty_config_file_still_loads() {
        let path = scratch("empty.toml");
        std::fs::write(&path, "   \n\n").unwrap();
        let daemon = Daemon::default();
        block_on(startup_load(&daemon, &path, StartupSource::Config))
            .expect("an empty file declares nothing and means nothing");
        std::fs::remove_file(&path).ok();
    }
}
