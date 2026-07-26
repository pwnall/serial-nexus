//! Phase 9 — the **filesystem half of the §10 authorization model**, from review 26
//! (`docs/26-claude-opus-code-review.md` findings CP-3, SEC-4/RV-6, SEC-2, SEC-8).
//!
//! `docs/security.md` calls socket permissions "the whole authorization model": there
//! is no per-verb authentication, so whoever can `connect(2)` owns every console the
//! daemon carries. That makes a *file mode* a security control, and the review found
//! four places where nothing checked it end to end:
//!
//! * **CP-3** — `--socket-group`, the only sanctioned *widening* of that model, had no
//!   test of group resolution, the chgrp, the 0660 mode, or the "group not found" hard
//!   error. A flag that silently no-ops leaves the operator believing their group has
//!   access; a flag that silently widens *more* than asked is worse.
//! * **SEC-4/RV-6** — the state file and log files inherited the umask; **0664 was
//!   observed live** (§7 reproduction 10). The state file carries exec-codec argv and
//!   environment and names every console; log files are console bytes, "frequently root
//!   shells" in the design's own words.
//! * **SEC-2** — a `transport = "unix"` leg listener bound at **0775**, and the v1 wire
//!   has no authentication of its own (§9: SSH is the confidentiality and authentication
//!   layer), so any local user could dial it and write into every console bound to that
//!   leg — straight past the 0600 control socket.
//! * **SEC-8** — a `listen`+`unix` leg unconditionally unlinked its configured address
//!   before binding. Pointed at an operator's file, it deleted it.
//!
//! Each of those has a unit test in the crate that owns it. What was missing is the
//! *end-to-end* statement: the modes a **running daemon** actually leaves on disk. A
//! unit test proves `apply_socket_perms` computes 0600; only this proves the daemon
//! calls it, on the real socket, on the path the operator configured. So every
//! assertion here reads `std::fs::metadata` on a file a live `serialnexusd` created.
//!
//! **Platform:** no serial device anywhere in this file — pty, log, leg and the control
//! plane run on every platform (§5), so nothing here skips for want of hardware. The one
//! skip is CP-3's success case, which needs the test user to be in a secondary group;
//! a machine with none is a valid skip, announced.

use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use nexus_itest::{Daemon, TempRun, bin, daemon_answers, sha256_hex, wait_until};
use serde_json::Value;

/// The permission bits of `path` (`& 0o777`). Panics if it does not exist — a missing
/// file is a broken test, not a passing one (the anti-tautology rule, §5).
fn mode_of(path: &Path) -> u32 {
    std::fs::metadata(path)
        .unwrap_or_else(|e| panic!("stat {}: {e}", path.display()))
        .permissions()
        .mode()
        & 0o777
}

/// The permission bits of `path`, or `None` if it does not exist yet — the
/// non-panicking form, for use inside a [`wait_until`] predicate.
fn try_mode_of(path: &Path) -> Option<u32> {
    Some(std::fs::metadata(path).ok()?.permissions().mode() & 0o777)
}

/// The owning gid of `path`.
fn gid_of(path: &Path) -> u32 {
    std::fs::metadata(path)
        .unwrap_or_else(|e| panic!("stat {}: {e}", path.display()))
        .gid()
}

/// The mode an ordinary `File::create` (which requests 0666) lands on in `dir` — i.e.
/// what the ambient umask alone would give a file the daemon did *not* narrow.
///
/// This is the honesty check behind the SEC-4 assertions. The regression they guard is
/// "the daemon inherited the umask"; on a box whose umask is already 0077, an
/// un-narrowed file comes out 0600 anyway and the assertions would pass vacuously. So
/// the test announces that rather than pretending, in the same spirit as a self-skip
/// (§5): a gate that has quietly stopped discriminating is the failure mode this whole
/// review round is about.
fn umask_reference_mode(dir: &Path) -> u32 {
    let probe = dir.join(".umask-probe");
    let _ = std::fs::File::create(&probe).expect("create the umask probe file");
    let mode = mode_of(&probe);
    let _ = std::fs::remove_file(&probe);
    mode
}

/// A secondary group of the test user — `(name, gid)` — or `None` to **skip**.
///
/// `--socket-group` chgrps the socket, which the kernel permits only to a group the
/// process belongs to (or for root). The *primary* group would make the chgrp a no-op
/// and the assertion vacuous (the socket is already owned by it), so only a group with
/// a gid other than the effective one counts. Read from POSIX `id`, which behaves the
/// same on Linux and macOS — unlike the `stat`/`getent` flags §5 retired the bash rig
/// over. A purely numeric entry means the name did not resolve, and `Group::from_name`
/// in the daemon would not resolve it either, so those are filtered out.
fn secondary_group() -> Option<(String, u32)> {
    let out = |args: &[&str]| -> Option<String> {
        let o = Command::new("id").args(args).output().ok()?;
        o.status
            .success()
            .then(|| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    };
    let gids: Vec<u32> = out(&["-G"])?
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();
    let names: Vec<String> = out(&["-Gn"])?
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    if gids.len() != names.len() {
        return None; // the two listings must correspond positionally
    }
    let primary: u32 = out(&["-g"])?.parse().ok()?;
    names
        .into_iter()
        .zip(gids)
        .find(|(name, gid)| *gid != primary && !name.chars().all(|c| c.is_ascii_digit()))
}

/// A `serialnexusd` booted with **extra flags** — the harness [`Daemon`] deliberately
/// boots one plain shape, and `--socket-group` is the whole point here. Same temp
/// runtime dir discipline (short `/tmp` path, `SUN_LEN`), same kill-and-clean on `Drop`,
/// so a panicking assertion never leaks a daemon.
struct FlaggedDaemon {
    child: Child,
    run: TempRun,
}

impl FlaggedDaemon {
    /// Spawn with `extra` appended to the standard `--socket`/`--state-file` flags.
    /// Does **not** wait for readiness: a test here may be asserting that it never
    /// becomes ready.
    fn spawn(extra: &[&str]) -> FlaggedDaemon {
        let run = TempRun::new();
        let child = Command::new(bin("serialnexusd"))
            .arg("--socket")
            .arg(run.socket())
            .arg("--state-file")
            .arg(run.state_file())
            .args(extra)
            .env("XDG_RUNTIME_DIR", run.path())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn serialnexusd");
        FlaggedDaemon { child, run }
    }

    fn socket(&self) -> PathBuf {
        self.run.socket()
    }

    /// Wait until the daemon answers an `info` round trip — the same readiness
    /// definition [`Daemon::start`] uses (T7), not merely "the inode appeared".
    fn wait_ready(&self) -> bool {
        let socket = self.socket();
        wait_until(Duration::from_secs(10), || {
            socket.exists() && daemon_answers(&socket)
        })
    }

    /// Wait for the process to exit within `timeout`, returning its exit code and
    /// stderr. `None` means it was still running (which for a "must refuse" case is
    /// the failure).
    fn wait_exit(mut self, timeout: Duration) -> Option<(Option<i32>, String)> {
        let mut status = None;
        wait_until(timeout, || {
            status = self.child.try_wait().expect("try_wait");
            status.is_some()
        });
        let status = status?;
        let mut stderr = String::new();
        if let Some(mut e) = self.child.stderr.take() {
            let _ = e.read_to_string(&mut stderr);
        }
        Some((status.code(), stderr))
    }
}

impl Drop for FlaggedDaemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ============================================================================
// SEC-4/RV-6 — the files a running daemon writes are not umask-wide.
// ============================================================================

/// The control socket is 0600, the configuration snapshot is 0600, and a log file is
/// no wider than 0640 — asserted on the artifacts of a **live daemon**, not on the
/// helpers that compute the modes.
///
/// The observed regression was 0664 on the state file: world-readable exec-codec argv,
/// environment, and the name of every console a reader could then go and tap. The
/// packaged unit's `StateDirectoryMode=0700` and `$XDG_RUNTIME_DIR`'s 0700 are the
/// outer layers; this is each file's own.
#[test]
fn a_running_daemon_writes_its_socket_state_and_logs_owner_only() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let run = d.run();

    // 0. Is this box's umask loose enough for the assertions below to discriminate?
    //    If not, say so — a vacuous pass is the thing under review here.
    let umask_ref = umask_reference_mode(run.path());
    if umask_ref & !0o600 == 0 {
        eprintln!(
            "NOTE a_running_daemon_writes_its_socket_state_and_logs_owner_only: this \
             box's umask already narrows a plain file to {umask_ref:o}, so the SEC-4 \
             assertions below cannot distinguish an explicitly-narrowed file from an \
             inherited one. They still run; they just prove less here than on a \
             default (0022) box, where an un-narrowed file lands 0644."
        );
    }

    // 1. The control socket: 0600 with no --socket-group. This is the file
    //    `docs/security.md` calls the whole authorization model.
    assert_eq!(
        mode_of(&d.socket()),
        0o600,
        "control socket is not owner-only (§10): whoever can open it owns every console"
    );

    // 2. A graph with a log node. `LogNode::create` opens the file, so no data has to
    //    flow for the mode to be observable — and `load` is a config mutation, so it
    //    also triggers the state-file snapshot (§11/§15.9).
    let logdir = run.join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log directory");
    // `usb0`'s device is deliberately absent: an edge must join one host-facing to one
    // target-facing endpoint, and the log's mode is set when `LogNode::create` opens the
    // file — neither needs the device to exist, so this runs on every platform (§5).
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
[[node]]
type = "log"
name = "logx"
directory = "{logdir}"
filename = "console.log"
[[edge]]
a = "usb0"
b = "logx"
"#,
        dev = run.join("absent-device").display(),
        logdir = logdir.display(),
    );
    rpc.load_toml(&cfg, false).expect("load serial+log graph");
    assert!(
        rpc.wait_status("logx", "active", Duration::from_secs(5)),
        "logx did not come up active: {:?}",
        rpc.node("logx")
    );

    // 3. The configuration snapshot: exactly 0600. `set_permissions` is not masked by
    //    the umask, so this is an equality, not a ceiling.
    let state_file = run.state_file();
    assert!(
        wait_until(Duration::from_secs(5), || state_file.exists()),
        "the state file was never written to {}",
        state_file.display()
    );
    assert_eq!(
        mode_of(&state_file),
        0o600,
        "state file is not owner-only (SEC-4: 0664 was observed) — it carries \
         exec-codec argv/environment and names every console"
    );

    // 4. The log file: 0640 is requested through `open(2)`'s mode argument, which the
    //    process umask *masks*, so the invariant is a ceiling — no bit outside 0640 —
    //    plus the floor that the owner can still read and write it.
    let logfile = logdir.join("console.log");
    assert!(
        wait_until(Duration::from_secs(5), || logfile.exists()),
        "the log file was never created at {}",
        logfile.display()
    );
    let mode = mode_of(&logfile);
    assert_eq!(
        mode & !0o640,
        0,
        "log file mode {mode:o} is wider than 0640 (SEC-4) — console bytes are \
         frequently root shells"
    );
    assert_eq!(
        mode & 0o600,
        0o600,
        "log file mode {mode:o} is not owner-readable/writable — the daemon cannot \
         use its own log"
    );
    // Spelled out because it is the property that actually matters: no other-user bits.
    assert_eq!(
        mode & 0o007,
        0,
        "log file mode {mode:o} grants `other` access"
    );
}

// ============================================================================
// SEC-2 — a unix leg listener is no wider than the control socket.
// ============================================================================

/// A `transport = "unix"`, `role = "listen"` leg binds its socket **0600**.
///
/// `bind(2)` creates the inode at `0o777 & !umask` — `srwxrwxr-x` on a default box —
/// and the v1 wire has no authentication (§9), so the file mode is the entire gate.
/// The second half of this test matters as much as the first: the narrowed socket must
/// still be *dialable by its owner*, or the fix would have closed the door on the
/// daemon's own peers.
#[test]
fn a_listen_unix_leg_binds_its_socket_owner_only() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let leg_sock = d.run().join("leg.sock");

    let cfg = format!(
        r#"
[[node]]
type = "leg"
name = "downlink"
faces = "host"
transport = "unix"
role = "listen"
address = "{leg}"
channels = ["console"]
"#,
        leg = leg_sock.display(),
    );
    rpc.load_toml(&cfg, false).expect("load listen leg");
    assert!(
        wait_until(Duration::from_secs(10), || leg_sock.exists()),
        "the leg never bound its socket at {} (node={:?})",
        leg_sock.display(),
        rpc.node("downlink")
    );

    // The narrowing is a `chmod` on the line after `bind(2)` — two adjacent syscalls
    // with no `.await` between them, but the *test* is another process and can stat in
    // between, so the mode is polled to its settled value rather than read once. That
    // window is documented and deliberate in `bind_listener` (a umask-guarded bind is
    // process-global and breaks concurrently-created directories); the finding is that
    // the socket used to stay 0775 **forever**, which no amount of polling fixes.
    assert!(
        wait_until(Duration::from_secs(5), || try_mode_of(&leg_sock)
            == Some(0o600)),
        "leg listener socket settled at {:o}, not 0600 (SEC-2: 0775 was observed) — \
         the v1 wire has no authentication, so any local user could write into every \
         console bound to this leg",
        try_mode_of(&leg_sock).unwrap_or(0)
    );

    // A leg with no peer is `waiting`, not faulted: narrowing the mode must not have
    // broken the bind.
    assert_eq!(
        rpc.node_status("downlink"),
        "waiting",
        "leg is not waiting for a peer: {:?}",
        rpc.node("downlink")
    );
    // …and the owner can still reach it, which is what the leg is for.
    std::os::unix::net::UnixStream::connect(&leg_sock)
        .expect("the narrowed leg socket must still accept its owner's connection");
}

// ============================================================================
// SEC-8 — the configured address is an operator file until proven a stale socket.
// ============================================================================

/// A `listen`+`unix` leg pointed at an existing **non-socket** file refuses to bind and
/// leaves the file byte-identical.
///
/// The former code unconditionally unlinked the configured address, so
/// `address = "/home/me/notes.txt"` deleted it — silently, as a side effect of loading
/// a config. The refusal must land the §15.8 way: an environmental problem changes the
/// node's *state* (faulted, with the reason in `state`), never the graph, and never the
/// operator's data. The file's SHA-256 is checked after the fault **and** after
/// `teardown`, because the unlink had a second site on the teardown path.
#[test]
fn a_leg_pointed_at_an_existing_file_refuses_and_leaves_it_intact() {
    let d = Daemon::start();
    let rpc = d.rpc();

    let notes = d.run().join("notes.txt");
    let content = b"nexus-itest: an operator file a leg must never unlink\n";
    std::fs::write(&notes, content).expect("write the operator file");
    let before = sha256_hex(content);

    let cfg = format!(
        r#"
[[node]]
type = "leg"
name = "downlink"
faces = "host"
transport = "unix"
role = "listen"
address = "{addr}"
channels = ["console"]
"#,
        addr = notes.display(),
    );
    // The load itself succeeds — the config is structurally valid; it is the
    // environment that refuses (§15.8).
    rpc.load_toml(&cfg, false)
        .expect("load leg on an operator file");

    assert!(
        rpc.wait_status("downlink", "faulted", Duration::from_secs(10)),
        "the leg did not fault on a non-socket address: {:?}",
        rpc.node("downlink")
    );
    let reason = rpc
        .node("downlink")
        .and_then(|n| {
            n.get("reason")
                .and_then(Value::as_str)
                .map(str::to_ascii_lowercase)
        })
        .unwrap_or_default();
    assert!(
        reason.contains("not a unix socket") && reason.contains("refusing to unlink"),
        "the fault reason does not name what happened, so an operator cannot act on \
         it: {reason:?}"
    );

    // The file is untouched — existence is not enough; the bytes are the claim.
    assert!(
        notes.exists(),
        "the leg unlinked the operator's file (SEC-8)"
    );
    let after = sha256_hex(&std::fs::read(&notes).expect("read the operator file"));
    assert_eq!(
        after, before,
        "the operator's file changed under a refused leg bind"
    );

    // The teardown path had its own unconditional unlink: a node that never bound
    // must remove nothing.
    rpc.teardown();
    assert!(
        notes.exists(),
        "teardown unlinked a file the leg never bound (SEC-8, teardown half)"
    );
    let after_teardown = sha256_hex(&std::fs::read(&notes).expect("read after teardown"));
    assert_eq!(
        after_teardown, before,
        "the operator's file changed during teardown"
    );
}

// ============================================================================
// CP-3 — `--socket-group`, the only sanctioned widening of the §10 model.
// ============================================================================

/// `--socket-group <g>` chgrps the control socket to `g` **and** relaxes it to 0660 —
/// both halves, on the socket a running daemon is actually serving.
///
/// Either half alone is a silent failure mode: the chgrp without the mode change leaves
/// the group unable to connect while the operator believes otherwise, and the mode
/// change without the chgrp widens the socket to the *primary* group, which is not what
/// was asked for. Both are asserted, and the daemon is proven to still answer RPC
/// afterwards — the widening must not have broken the socket it widened.
///
/// **Skips** when the test user has no secondary group: the chgrp is only permitted to
/// a group the process belongs to, and using the primary group would assert nothing.
#[test]
fn socket_group_chgrps_the_control_socket_and_widens_it_to_0660() {
    let Some((group, gid)) = secondary_group() else {
        eprintln!(
            "SKIP socket_group_chgrps_the_control_socket_and_widens_it_to_0660: \
             the test user is in no secondary group (nothing to chgrp to)"
        );
        return;
    };

    let d = FlaggedDaemon::spawn(&["--socket-group", &group]);
    assert!(
        d.wait_ready(),
        "the daemon never answered RPC with --socket-group {group}"
    );

    let socket = d.socket();
    assert_eq!(
        gid_of(&socket),
        gid,
        "control socket was not chgrp'd to {group} ({gid}) — the group cannot reach it"
    );
    assert_eq!(
        mode_of(&socket),
        0o660,
        "control socket was not widened to 0660 for {group} — the chgrp alone grants \
         the group nothing"
    );

    // The widening is the *only* one: `other` gets nothing, ever.
    assert_eq!(
        mode_of(&socket) & 0o007,
        0,
        "--socket-group widened the socket to `other` as well"
    );

    // And the daemon still serves on it (a structured RPC result, not CLI text).
    let rpc = nexus_itest::Rpc::new(&socket);
    assert!(
        rpc.info().get("daemon_version").is_some(),
        "the daemon stopped answering `info` after the group widen"
    );
}

/// An unresolvable `--socket-group` is a **hard startup error**, not a silent fallback
/// to 0600.
///
/// The distinction is the whole point: a daemon that came up owner-only after a typo'd
/// group name would look healthy while every intended operator got `EACCES` on connect,
/// and the failure would surface hours later as "the console is broken". `run` resolves
/// the group *before* changing anything and propagates the error out of `main`.
#[test]
fn an_unknown_socket_group_is_a_hard_startup_error() {
    // A name no /etc/group can plausibly hold; pid-suffixed so a parallel run cannot
    // collide with a real group somebody just created.
    let bogus = format!("snx-no-such-group-{}", std::process::id());

    let d = FlaggedDaemon::spawn(&["--socket-group", &bogus]);
    let socket = d.socket();
    let (code, stderr) = d
        .wait_exit(Duration::from_secs(10))
        .expect("the daemon kept running with an unresolvable --socket-group");

    assert_ne!(
        code,
        Some(0),
        "the daemon exited successfully with an unresolvable --socket-group; \
         stderr={stderr:?}"
    );
    // Secondary to the exit status, but an operator has to be able to act on it: the
    // diagnostic must name the group they mistyped.
    assert!(
        stderr.contains(&bogus),
        "the startup error does not name the group that could not be resolved: \
         {stderr:?}"
    );

    // Nothing is left serving on the socket path — a half-configured daemon would be
    // worse than none.
    assert!(
        !daemon_answers(&socket),
        "something is still answering on {} after the startup error",
        socket.display()
    );
}
