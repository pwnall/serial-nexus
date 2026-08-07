//! The §10 default socket path, in one place.
//!
//! # Why this is a module and not two private functions
//!
//! The policy is three arms — root gets `/run`, an unprivileged process with
//! `XDG_RUNTIME_DIR` set gets that directory, and everyone else gets a per-uid path
//! under `/tmp`. It was implemented **twice**, once in the daemon and once in `ctl`,
//! and the two agreed: they had to, or `ctl` would look for a socket the daemon had
//! not created. Two copies that must agree is a latent defect whether or not they
//! currently do.
//!
//! It was also described a **third** time, in prose, by `serial-nexus-doctor`'s
//! environment block — and that copy was wrong. With `XDG_RUNTIME_DIR` unset it said
//! *"daemon falls back to /run or a --socket override"*, naming an arm that only a
//! root process reaches and omitting the one an unprivileged user actually gets. That
//! is the first field an operator reads when the socket is not where they expected,
//! in the report the design calls the expected first attachment on any support
//! request, and no gate covered it: `expectations/*.jq` assert over `.probes[]`,
//! `.summary` and `.build`, and the environment block is none of those.
//!
//! So the fix is not "make the third copy agree" — it is to delete the third copy.
//! The doctor now *computes* the path with the same function the daemon binds and
//! `ctl` connects to, and prints it. A path that is printed rather than described
//! cannot drift, and [`SocketOrigin`] lets the report say which arm produced it
//! without restating the policy (notes §3.72).
//!
//! macOS is why the wrong description mattered rather than merely being untidy: it
//! sets no `XDG_RUNTIME_DIR` and has no `/run`, so every unprivileged Mac lands on the
//! `/tmp` arm — the one the old sentence did not mention.
//!
//! # Why it lives *here*, and the home that was tried first
//!
//! The three callers are the daemon, `ctl` and the doctor. The obvious shared crate is
//! `serial-nexus-core` — all three already depend on it — and that was tried first. A
//! meta-gate rejected it: `core` may not declare `nix`, because the resolver's
//! enumeration face is passive **by construction**, and probing a port toggles DTR and
//! resets the board behind it. The gate exists for exactly the move that was being
//! made ("just add `nix` to core for one `getuid`"), it names its own reasoning, and it
//! was right — recorded rather than quietly worked around (§5, §9).
//!
//! `serial-nexus-sys` was the next candidate and is declined: it is the platform crate
//! and has `nix` already, but it also links IOKit and CoreFoundation on macOS for the
//! replug backend, and `ctl` does not depend on it today. Reading a uid is not worth
//! making the CLI link two frameworks.
//!
//! So it lands beside [`DAEMON_NAME`](crate::DAEMON_NAME), in the crate that defines
//! the daemon's control interface — "which socket that interface is at" is the same
//! kind of fact as "what the daemon is called". The policy itself
//! ([`socket_path_from`]) is pure `std` and would compile in any of the three; only the
//! two-line wrapper needs `nix` at all.

use std::path::PathBuf;

/// Which arm of the §10 policy produced a path.
///
/// Carried so a diagnostic can explain the answer without re-deriving it. The daemon
/// and `ctl` ignore it; they need the path and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketOrigin {
    /// Running as root: `/run/<name>.sock`. The system-wide location.
    Root,
    /// Unprivileged with `XDG_RUNTIME_DIR` set and non-empty: `$XDG_RUNTIME_DIR/<name>.sock`.
    XdgRuntimeDir,
    /// Unprivileged with no usable `XDG_RUNTIME_DIR`: `/tmp/<name>-<uid>.sock`.
    ///
    /// The uid is in the filename rather than the directory because `/tmp` is shared:
    /// without it, two users on one machine would collide on a path only one of them
    /// can own. This is the arm every unprivileged macOS session takes.
    TmpPerUid,
}

impl SocketOrigin {
    /// A short phrase naming the arm, for a report that has already printed the path.
    pub fn describe(self) -> &'static str {
        match self {
            SocketOrigin::Root => "running as root, so the system-wide /run path",
            SocketOrigin::XdgRuntimeDir => "from XDG_RUNTIME_DIR",
            SocketOrigin::TmpPerUid => {
                "no XDG_RUNTIME_DIR and not root, so the per-uid /tmp fallback"
            }
        }
    }
}

/// The §10 policy as a **pure function of the process state it reads**.
///
/// Split out from [`default_socket_path`] so every arm is reachable from a test
/// without touching the environment. That is not a seam invented for a test: the two
/// discriminators are `geteuid` and one variable, and a test that mutated either would
/// have to mutate *process-global* state — which in Rust 2024 means `unsafe`, and this
/// crate is `#![forbid(unsafe_code)]` like every crate but `serial-nexus-sys` (§16.3).
/// That is the invariant catching a bad test rather than obstructing a good one: an
/// env-mutating test is also racy under the parallel runner, since a sibling reading
/// `XDG_RUNTIME_DIR` would see whichever write won. The alternative to this split is a
/// test that can only ever exercise the arm the suite happens to run under, which for
/// the root arm is never — and an untested arm here is a daemon binding somewhere
/// `ctl` does not look.
///
/// `xdg` is the raw variable, `None` when unset. An **empty** value is deliberately
/// not the same as a set one: an exported-but-empty `XDG_RUNTIME_DIR` is common in
/// stripped service environments, and joining onto it yields a relative path the
/// daemon would bind in its working directory.
fn socket_path_from(
    is_root: bool,
    xdg: Option<&str>,
    uid: u32,
    name: &str,
) -> (PathBuf, SocketOrigin) {
    if is_root {
        return (
            PathBuf::from(format!("/run/{name}.sock")),
            SocketOrigin::Root,
        );
    }
    if let Some(dir) = xdg.filter(|d| !d.is_empty()) {
        return (
            PathBuf::from(dir).join(format!("{name}.sock")),
            SocketOrigin::XdgRuntimeDir,
        );
    }
    (
        PathBuf::from(format!("/tmp/{name}-{uid}.sock")),
        SocketOrigin::TmpPerUid,
    )
}

/// The §10 default socket path for a daemon spelled `name`, and which arm produced it.
///
/// Callers that only need the path use `.0`. There is deliberately no way to ask for
/// the path *without* being able to ask where it came from: the pair is what stops a
/// fourth prose copy appearing.
pub fn default_socket_path(name: &str) -> (PathBuf, SocketOrigin) {
    let xdg = std::env::var("XDG_RUNTIME_DIR").ok();
    socket_path_from(
        nix::unistd::geteuid().is_root(),
        xdg.as_deref(),
        nix::unistd::getuid().as_raw(),
        name,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: &str = "serial-nexus-daemon";

    /// All three arms, including the one the suite can never run under.
    #[test]
    fn each_arm_of_the_socket_policy_produces_its_own_path() {
        // Root ignores XDG entirely — asserted with XDG *set*, so a bug that checked
        // the variable before the privilege would fail here rather than pass by
        // accident on a box where the variable happens to be unset.
        assert_eq!(
            socket_path_from(true, Some("/run/user/0"), 0, D),
            (
                PathBuf::from("/run/serial-nexus-daemon.sock"),
                SocketOrigin::Root
            )
        );

        assert_eq!(
            socket_path_from(false, Some("/run/user/4242"), 4242, D),
            (
                PathBuf::from("/run/user/4242/serial-nexus-daemon.sock"),
                SocketOrigin::XdgRuntimeDir
            )
        );

        // The macOS shape: no XDG at all.
        assert_eq!(
            socket_path_from(false, None, 501, D),
            (
                PathBuf::from("/tmp/serial-nexus-daemon-501.sock"),
                SocketOrigin::TmpPerUid
            )
        );
    }

    /// **An exported-but-empty `XDG_RUNTIME_DIR` is not a set one.** Treating it as
    /// set yields `serial-nexus-daemon.sock` — a *relative* path, bound in whatever
    /// working directory the daemon started in, which `ctl` would never find.
    #[test]
    fn an_empty_xdg_runtime_dir_falls_through_to_the_tmp_arm() {
        let (p, o) = socket_path_from(false, Some(""), 501, D);
        assert_eq!(o, SocketOrigin::TmpPerUid);
        assert!(
            p.is_absolute(),
            "an empty XDG must not yield a relative path: {p:?}"
        );
    }

    /// The uid must reach the `/tmp` filename. `/tmp` is shared, so without it two
    /// users on one machine collide on a path only one of them can own — and the
    /// second gets a permission error naming a file they never chose.
    #[test]
    fn the_tmp_arm_separates_users_by_uid() {
        let (a, _) = socket_path_from(false, None, 501, D);
        let (b, _) = socket_path_from(false, None, 502, D);
        assert_ne!(a, b, "two uids must not share a /tmp socket path");
    }

    /// The wrapper reads the real process state and agrees with the pure function it
    /// delegates to. Without this the split above could drift: a wrapper that passed
    /// `is_root` for `xdg`, or read a differently-spelled variable, would leave every
    /// test above green while the daemon bound somewhere else.
    #[test]
    fn the_public_entry_point_agrees_with_the_policy_for_this_process() {
        let xdg = std::env::var("XDG_RUNTIME_DIR").ok();
        assert_eq!(
            default_socket_path(D),
            socket_path_from(
                nix::unistd::geteuid().is_root(),
                xdg.as_deref(),
                nix::unistd::getuid().as_raw(),
                D
            )
        );
    }

    /// Every origin describes itself. A `describe` that returned `""` for an arm
    /// would let the doctor print a path with no explanation beside it, which is the
    /// shape this module exists to prevent.
    #[test]
    fn every_origin_names_itself() {
        for o in [
            SocketOrigin::Root,
            SocketOrigin::XdgRuntimeDir,
            SocketOrigin::TmpPerUid,
        ] {
            assert!(!o.describe().is_empty(), "{o:?} has no description");
        }
    }
}
