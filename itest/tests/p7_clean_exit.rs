#![forbid(unsafe_code)]

//! The daemon's **clean-exit seam** (design §10 control socket, §7.2 pty teardown,
//! §11/§15.9 state persistence) — the path review 37 found nothing observing
//! (37-SEAM-1).
//!
//! `serve`'s loop has three exits — the `shutdown` verb, SIGINT, SIGTERM — and one
//! epilogue: `teardown_all()` (which unlinks every pty symlink and drops every port)
//! followed by `remove_file(socket)`. No test had ever seen it run. The harness's own
//! `Daemon::drop` issues `shutdown` and then immediately SIGKILLs, so the process was
//! always gone before the epilogue could be observed, and **deleting the two signal
//! arms passed the entire suite** — a daemon that ignored SIGTERM, left its socket
//! inode behind for the next start's stale-socket dance and left a symlink pointing
//! into a dead pts would have shipped green.
//!
//! Four properties per exit path, chosen because each is a different subsystem's
//! promise and each fails independently:
//!
//! 1. **the process exits**, and with a success status — the signal was handled, not
//!    merely fatal (a daemon with no handler dies of SIGTERM's default disposition,
//!    which is the exact regression this file exists for);
//! 2. **the control socket is unlinked** — §10's stale-socket dance is what the next
//!    daemon would otherwise have to survive;
//! 3. **the pty symlinks are unlinked**, asserted through `symlink_metadata` rather
//!    than `exists()`. That distinction is the whole assertion: once the master
//!    closes, the symlink *dangles*, and `exists()` follows it and answers `false`
//!    whether it was removed or not, so a guard written the easy way holds either way.
//!    Scope, measured rather than assumed: deleting the epilogue's `teardown_all()`
//!    alone leaves this green, because `PtyNode::drop` delegates to the same
//!    `teardown` as the process unwinds. What it discriminates is a daemon that never
//!    got to unwind — killed by a signal it does not handle, which is exactly the
//!    regression above;
//! 4. **the state file survives, with its graph** — shutdown is not `teardown`
//!    (§11/§15.9: the snapshot is what the next start loads, and losing it silently
//!    discards every node built by incremental surgery).
//!
//! Runs everywhere: a pty node needs no serial device.

use std::path::Path;
use std::time::Duration;

use serial_nexus_itest::{Daemon, TempRun, wait_until};

/// A graph whose teardown is visible from outside the process: two `pty` nodes, each
/// owning a symlink in the run directory. Two rather than one because the epilogue
/// tears down a *list* — a loop that released only its first entry would pass with one.
/// No target-facing node, and therefore no edges: every target here would be a serial
/// device, and the seam under test has nothing to do with one.
fn cfg(run: &TempRun) -> String {
    format!(
        r#"
[[node]]
type = "pty"
name = "console"
path = "{console}"
[[node]]
type = "pty"
name = "aux"
path = "{aux}"
"#,
        console = run.join("console").display(),
        aux = run.join("aux").display(),
    )
}

/// The pty symlinks [`cfg`] installs, which are also its node names.
const SYMLINKS: [&str; 2] = ["console", "aux"];

/// Does a symlink (or anything else) exist *at* `p`, without following it?
///
/// `Path::exists()` is the wrong tool here and quietly so: a symlink into a closed
/// pts dangles, so `exists()` reports `false` for a symlink the daemon never removed.
fn path_present(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok()
}

/// Boot a daemon on [`cfg`] and wait until every observable is in place, returning it
/// with the console symlink's path. Asserting the *before* state is what makes the
/// *after* state evidence rather than coincidence.
fn started_daemon() -> Daemon {
    let d = Daemon::start();
    let cfg = cfg(d.run());
    d.rpc().load_toml(&cfg, false).expect("load the graph");
    assert!(
        d.rpc()
            .wait_status("console", "active", Duration::from_secs(10)),
        "console pty never reached active: {:?}",
        d.rpc().node("console")
    );

    for name in SYMLINKS {
        let link = d.run().join(name);
        assert!(
            wait_until(Duration::from_secs(5), || path_present(&link)),
            "the {name} pty symlink never appeared at {}",
            link.display()
        );
    }
    assert!(
        d.socket().exists(),
        "the control socket is not there to remove"
    );
    assert!(
        wait_until(Duration::from_secs(5), || d.run().state_file().exists()),
        "the state file was never written, so `preserved` below would be vacuous"
    );
    d
}

/// The shared body: bring a daemon up, end it the named way, and assert the four
/// epilogue properties. `end` returns the exit status the daemon left behind.
fn assert_clean_exit(how: &str, end: impl FnOnce(&mut Daemon) -> Option<std::process::ExitStatus>) {
    let mut d = started_daemon();
    let socket = d.socket();
    let state_file = d.run().state_file();
    let before = std::fs::read_to_string(&state_file).expect("read the state file");

    // (1) It exits, and it exits *cleanly*.
    let status = end(&mut d).unwrap_or_else(|| {
        panic!(
            "the daemon was still running 10s after {how} — `serve`'s loop has no arm \
             for it, or the epilogue is wedged"
        )
    });
    assert!(
        status.success(),
        "the daemon exited {status} after {how} — a success status is what separates \
         'the signal was handled' from 'the default disposition killed it', and only \
         the first runs the teardown asserted below"
    );

    // (2) The control socket is unlinked (§10): the next daemon must not have to
    //     survive a stale-socket dance this one could have avoided.
    assert!(
        !path_present(&socket),
        "the control socket {} survived {how}",
        socket.display()
    );

    // (3) Every pty symlink is unlinked (§7.2) — `symlink_metadata`, because a
    //     dangling symlink is invisible to `exists()`, which would hold here whether
    //     the link was removed or merely orphaned.
    for name in SYMLINKS {
        let link = d.run().join(name);
        assert!(
            !path_present(&link),
            "the {name} pty symlink {} survived {how} — it now points into a closed \
             pts, which `Path::exists()` cannot tell from a removed link",
            link.display()
        );
    }

    // (4) The persisted graph survives (§11/§15.9). Shutdown is not `teardown`: the
    //     snapshot is what the next start loads, and clearing it here would silently
    //     discard every node built by incremental surgery.
    assert!(
        state_file.exists(),
        "the state file {} was removed by {how} — the next start would come up empty",
        state_file.display()
    );
    let after = std::fs::read_to_string(&state_file).expect("read the state file");
    assert_eq!(
        after, before,
        "{how} rewrote the state file; it must be left exactly as the last mutation \
         wrote it"
    );
    for node in SYMLINKS {
        assert!(
            after.contains(node),
            "the preserved state file no longer names `{node}` after {how}: {after}"
        );
    }
}

#[test]
fn sigterm_exits_cleanly_and_releases_the_node_environment() {
    assert_clean_exit("SIGTERM", |d| {
        d.signal_and_wait("TERM", Duration::from_secs(10))
    });
}

#[test]
fn sigint_exits_cleanly_and_releases_the_node_environment() {
    assert_clean_exit("SIGINT", |d| {
        d.signal_and_wait("INT", Duration::from_secs(10))
    });
}

// The third exit from the same loop. Included here rather than left to the RPC tests
// because the property under test is the *epilogue*, which is one piece of code all
// three arms fall through to: a regression in it is a regression in all three, and a
// regression in one arm alone is invisible from the others.
#[test]
fn the_shutdown_verb_exits_cleanly_and_releases_the_node_environment() {
    assert_clean_exit("the `shutdown` verb", |d| {
        d.rpc().shutdown();
        d.wait_for_exit(Duration::from_secs(10))
    });
}
