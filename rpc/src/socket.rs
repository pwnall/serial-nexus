//! The §10 default socket path — and the client-side choice of *which* socket to
//! drive — in one place.
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
//! The four callers are the daemon, `ctl`, the doctor and the web console — the last
//! of which arrived here in 2026-08-12 by having its *own* two-arm copy deleted
//! (notes §3.75). The obvious shared crate is
//! `serial-nexus-core` — all four already depend on it — and that was tried first. A
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
//!
//! # The rename window rides here for the same reason
//!
//! [`resolve_client_socket`] is the second half: a client with no `--socket` takes the
//! current default when it exists and the pre-§15.40 one only when it does not (plan
//! §17.3). That policy was also implemented **twice** — `ctl` and the web console
//! carried byte-duplicate copies — which is the same shape as the paragraph above, one
//! layer up: two clients that must agree about *which* daemon a terminal and a browser
//! on one machine reach. It is one implementation here now (plan §18 item 51), so
//! retiring the client fallback edits this crate rather than three.
//!
//! The web console is the worked example of why "agrees today" is not a defence. Its
//! deleted `default_socket` copy had **two** arms where the policy has three: it never
//! asked whether the process was root, so it read `XDG_RUNTIME_DIR` first and handed
//! everyone without a usable one the *root* arm `/run/<name>.sock`. An unprivileged
//! console with the variable unset or exported empty therefore pointed at a path only a
//! root daemon ever binds, while the daemon beside it was at `/tmp/<name>-<uid>.sock` —
//! and it said so through the 503 the bridge answers an upgrade with, naming a socket
//! nothing on that machine will ever create, under a `--help` line promising the
//! daemon's defaults. That is every unprivileged macOS session (no `XDG_RUNTIME_DIR`,
//! no `/run`) and any stripped service environment that exports the variable empty
//! (notes §3.75).

use std::path::{Path, PathBuf};

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

/// The §17.3 rename-window policy as a **pure function of the two candidate paths and
/// an existence predicate**.
///
/// Split out from [`resolve_client_socket`] for the same reason [`socket_path_from`] is
/// split out from [`default_socket_path`], and then one more: the discriminators here
/// are two *files*, and a test that arranged them for real would have to create them at
/// the paths this process's own environment derives — which is a fixed pair per box, so
/// two such tests running in parallel would fight over the same two inodes and one
/// would see the other's socket. The predicate makes both arms reachable from any
/// environment, in either order, with nothing on disk.
fn resolve_client_socket_from(
    override_path: Option<PathBuf>,
    current: PathBuf,
    legacy: PathBuf,
    exists: impl Fn(&Path) -> bool,
) -> PathBuf {
    // An explicit override is taken **as written**, without asking whether anything is
    // there. Falling back from a path the operator named would send them to a different
    // daemon than the one they asked for; a connection error naming their own path is
    // the correct answer, and the only one they can act on.
    if let Some(p) = override_path {
        return p;
    }
    // The current name wins whenever it exists, so a fresh daemon is never passed over
    // in favour of a stale socket inode left by an older one that died without
    // unlinking. The fallback only rescues the other order: a pre-rename daemon still
    // running under an operator who has just installed the new client.
    if !exists(&current) && exists(&legacy) {
        return legacy;
    }
    // Neither present: still the current name, so the connection error names the path a
    // daemon started now would actually bind.
    current
}

/// The control socket a **client** should drive: an explicit override when given, else
/// the §10 default, falling back to the pre-§15.40 default when — and only when — there
/// is nothing at the current one (plan §17.3, one release).
///
/// One implementation for both clients (plan §18 item 51), because a browser and a
/// terminal pointed at the same machine must reach the same daemon — and because the
/// *client* half of the window then closes here, in one crate, rather than in three.
/// (The window's other half is the daemon adopting a pre-rename state snapshot, and it
/// stays in the daemon: that is about a file this daemon owns, not about which socket a
/// client dials. Both halves read [`LEGACY_DAEMON_NAME`](crate::LEGACY_DAEMON_NAME),
/// which is why the constant lives one level up.)
///
/// **"Client" is in the name because the daemon must not call this.** The daemon binds
/// [`DAEMON_NAME`](crate::DAEMON_NAME) unconditionally: adopting the fallback there
/// would make a daemon *create* a socket under the retired name whenever a stale inode
/// from an older release happened to be lying around, which is the opposite of a window
/// that closes — nothing ever writes that spelling again.
///
/// Both candidate paths are computed up front rather than lazily. `default_socket_path`
/// reads process state and has no side effects, so the extra call costs a `getuid` and
/// an environment read on the override arm and buys the policy as one expression.
pub fn resolve_client_socket(override_path: Option<PathBuf>) -> PathBuf {
    let (current, legacy) = client_socket_candidates();
    resolve_client_socket_from(override_path, current, legacy, |p| p.exists())
}

/// The two candidates [`resolve_client_socket`] hands the policy, **in order**:
/// `(current, legacy)`.
///
/// Split out purely so the order can be pinned by a test that reads no filesystem.
/// The obvious test — compare the wrapper's answer against the policy called with the
/// same two paths — is *vacuous on any box that has a socket at the current name*,
/// which is every developer box with a daemon up and every box carrying a stale inode:
/// with the current name present both the correct order and the swapped one answer the
/// current path, so the assertion passes identically whether the property holds or not
/// (plan §3 rule 22's tell). Measured, with the arguments deliberately swapped:
/// `XDG_RUNTIME_DIR=<dir holding serial-nexus-daemon.sock>` → **ok**; with neither
/// socket present → FAILED. A pair returned before any `exists` call is asked has no
/// such state to depend on.
fn client_socket_candidates() -> (PathBuf, PathBuf) {
    (
        default_socket_path(crate::DAEMON_NAME).0,
        default_socket_path(crate::LEGACY_DAEMON_NAME).0,
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

    // ---- The §17.3 rename window, hoisted out of both clients (plan §18 item 51) ----

    /// The two candidates as one `XDG_RUNTIME_DIR` arm would derive them. Both are
    /// built **from the constants**, never spelled: the retired name has exactly one
    /// live definition and the `retired_names_appear_only_where_history_lives`
    /// meta-gate allows exactly that one occurrence, so a literal here would fail it.
    fn candidates() -> (PathBuf, PathBuf) {
        (
            PathBuf::from(format!("/run/user/4242/{}.sock", crate::DAEMON_NAME)),
            PathBuf::from(format!("/run/user/4242/{}.sock", crate::LEGACY_DAEMON_NAME)),
        )
    }

    /// An existence predicate over a fixed set, standing in for the filesystem.
    fn present(paths: &[&PathBuf]) -> impl Fn(&Path) -> bool {
        let set: Vec<PathBuf> = paths.iter().map(|p| (*p).clone()).collect();
        move |p| set.iter().any(|q| q == p)
    }

    /// **An explicit override is taken as written, unexamined.** Asserted with a path
    /// that does not exist *and* both defaults present, so a version that validated the
    /// override and fell back would have every reason here to answer something else —
    /// which would silently connect the operator to a different daemon than the one
    /// they named on the command line.
    #[test]
    fn an_explicit_socket_override_is_taken_as_written() {
        let (current, legacy) = candidates();
        let chosen = PathBuf::from("/nowhere/the-operators-own.sock");
        assert_eq!(
            resolve_client_socket_from(
                Some(chosen.clone()),
                current.clone(),
                legacy.clone(),
                present(&[&current, &legacy])
            ),
            chosen
        );
    }

    /// **The current name wins whenever it exists** — asserted with the legacy socket
    /// present too, which is the upgraded box. The ordering this pins down is the one
    /// a cheaper implementation gets wrong (try legacy first, or prefer whichever
    /// exists): a stale inode left by an older daemon that died without unlinking would
    /// then capture every client on the machine.
    #[test]
    fn the_current_socket_wins_over_a_stale_pre_rename_one() {
        let (current, legacy) = candidates();
        assert_eq!(
            resolve_client_socket_from(
                None,
                current.clone(),
                legacy.clone(),
                present(&[&current, &legacy])
            ),
            current
        );
    }

    /// **The legacy socket is taken only when the current one is absent** — the case
    /// the window exists for: a pre-rename daemon still running under an operator who
    /// has just installed the new client.
    #[test]
    fn the_pre_rename_socket_is_taken_when_there_is_no_current_one() {
        let (current, legacy) = candidates();
        assert_eq!(
            resolve_client_socket_from(None, current.clone(), legacy.clone(), present(&[&legacy])),
            legacy
        );
    }

    /// **With neither present the client is still pointed at the current name**, so the
    /// connection error names the path a daemon started now would bind — not a retired
    /// one the operator would have to recognise before they could act on it.
    #[test]
    fn with_no_socket_at_all_the_current_name_is_the_answer() {
        let (current, legacy) = candidates();
        assert_eq!(
            resolve_client_socket_from(None, current.clone(), legacy.clone(), present(&[])),
            current
        );
    }

    /// The public entry point feeds the policy the two default paths, in that order.
    /// Without this the split above could drift: the pure function's tests all pass
    /// arguments themselves, so a wrapper that handed the *legacy* path to the
    /// `current` parameter would leave every one of them green while sending both
    /// clients to the retired socket.
    ///
    /// **Asserted on the candidate pair, not on the wrapper's answer**, and that is the
    /// whole point of [`client_socket_candidates`] existing. Comparing
    /// `resolve_client_socket(None)` against the policy called with the same two paths
    /// reads the real filesystem on both sides, so on a box that has a socket at the
    /// current name both orders answer the current path and the assertion asserts
    /// nothing — rule 22's tell, measured in review rather than reasoned about. The
    /// pair is computed before any `exists` call, so this holds on every box in every
    /// state.
    #[test]
    fn the_client_entry_point_applies_the_policy_to_the_two_defaults() {
        let current = default_socket_path(crate::DAEMON_NAME).0;
        let legacy = default_socket_path(crate::LEGACY_DAEMON_NAME).0;
        assert_ne!(
            current, legacy,
            "the two candidates must differ, or the order below is untestable"
        );
        assert_eq!(
            client_socket_candidates(),
            (current.clone(), legacy.clone()),
            "the client entry point must hand the policy (current, legacy) in that \
             order — swapping them sends every client to the retired socket"
        );
        assert_eq!(
            resolve_client_socket(None),
            resolve_client_socket_from(None, current, legacy, |p| p.exists())
        );

        // The override arm through the real entry point, which consults neither the
        // environment nor the filesystem to answer it.
        let chosen = PathBuf::from("/nowhere/the-operators-own.sock");
        assert_eq!(resolve_client_socket(Some(chosen.clone())), chosen);
    }
}
