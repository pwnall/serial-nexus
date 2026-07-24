# macOS support status

macOS is a **best-effort** tier (design §13). Linux is the required platform and
the one every mechanism is specified against; macOS is supported where plain
POSIX carries the design, and degrades — never crashes, never silently misbehaves
— everywhere the design leans on a Linux-only facility.

## Update — 2026-07-24 hands-on pass (macOS 15.7.8 / Darwin 24.6.0, x86_64, real FTDI crossover rig)

The first hands-on pass on real hardware ran, and settled the open questions. Two
things below that this page previously marked *"expected"* / *"needs a Mac"* turned
out to be **real defects, now fixed**; the rest is confirmed. See
`docs/implementation-notes.md` (2026-07-24 session) for the mechanism.

- **Build / test / lint:** all clean; **156 tests pass, same count as Linux.**
- **Serial data plane over the real crossover cable:** **32 KiB byte-exact both
  directions**, `send` verb reaches hardware, **TIOCEXCL enforced**, driver counters
  gracefully absent. **verified.**
- **PTY nodes — FIXED.** They previously *faulted* on macOS (`tcgetattr: ENOTTY`): the
  §7.2 baseline termios was applied through the pty **master**, which BSD rejects. Now
  cfg-gated to apply through the **slave** on non-Linux (`nodes/pty.rs::with_termios_fd`),
  re-asserted on the client's presence rising edge (the macOS slave termios resets on
  last-close). Presence tracking works; the full client→pty→serial→crossover path is
  **verified byte-exact.** Linux path unchanged.
- **Doctor P1 → `degraded`** (EXTPROC/packet-mode notifications don't surface; §7.2 runs
  poll-only — benign, as designed). **Doctor P2 → `degraded`** (was `unsupported`): POLLHUP
  presence works via priming + slave-termios, so the probe now says `degraded`, not
  `unsupported`. **`expectations/macos.jq` now PASSES** (0 unsupported).
- **`nexus-sim` PTY doubles — FIXED** for the same master-termios reason (BSD leaves termios
  to the consumer).

### macOS test-infrastructure limitation: a pty is not a usable serial device

`serial2::SerialPort::open` on a macOS **pts** returns `ENOTTY` (it sets baud via a
macOS-specific ioctl a pty rejects). So the Linux "no-target doctrine" — a pty standing in
for a serial device — **does not work on macOS**. Serial-*device* tests on macOS use a
**real crossover rig** or **skip**; the product's real-UART path is unaffected (proven
byte-exact above). The Rust harness (`nexus-itest`) encodes exactly this: `serial_rig()`
yields the real rig on macOS, a sim pty on Linux, or `None` → skip.

The validation harness was fully migrated from the bash `scripts/validate/**` (which used
`stat -c`, `nc -q`, `sha256sum`, `timeout`, `/dev/serial/by-id` — none macOS-portable) to
the cross-platform **`nexus-itest`** crate; `scripts/` is gone entirely (v10 §16.11).
macOS-verified: control-plane + the hardware crossover byte-exact test.

The feature matrix below is the original Phase-8 *predicted* table, kept for reference; where
this update block and the table disagree, **this block is the observed truth.**

**What Phase 8 actually delivered:** the whole workspace now *compiles* for
`*-apple-darwin` and *degrades gracefully* at every Linux-specific edge. That is
verified by a clean cross-compile —

```
cargo check --target x86_64-apple-darwin --workspace   # Finished, no errors
```

— and by reading the platform gates in the source (below). It is **not yet
runtime-verified on a real Mac.** Confirming actual kernel and device behavior is
the job of the macOS CI lane and a future hands-on pass; until then, everything
that depends on live macOS behavior is marked *unverified* here and stays that
way honestly.

## How to read the verdicts

| Marker | Meaning |
|---|---|
| **cross-checked** | Verified against the source and a clean `cargo check --target x86_64-apple-darwin`. Compile-time and code-path facts. |
| **expected** | Follows deterministically from the code and design, but has not been exercised on a Mac. Should hold; unproven. |
| **unverified** | Depends on real macOS kernel/device behavior we cannot predict from Linux. **Needs a Mac** — this is what the CI lane and a hands-on pass exist to settle. |

## Feature matrix

| Feature | Linux | macOS | Notes |
|---|---|---|---|
| Workspace build | ✅ | ✅ | Compiles for `*-apple-darwin`. **cross-checked.** |
| Data plane (PTY pair, `read`/`write`/`poll(2)`) | ✅ | ✅ (expected) | Plain POSIX; no Linux-only syscalls on the hot path. **expected.** |
| PTY client-termios observation (EXTPROC + `TIOCPKT`) | ✅ | ❓ | Packet-mode signaling is **unverified** on macOS (§7.2). **needs a Mac.** |
| Reconciliation poll (termios backstop) | ✅ | ✅ | Unconditional; becomes the *sole* observation path if EXTPROC misbehaves. **cross-checked** (code) / macOS timing **expected.** |
| Driver error counters (`TIOCGICOUNT`: overrun/framing/parity) | ✅ | ➖ omitted | `TIOCGICOUNT` is Linux-only; the binding is gated, the reader stubs to `ENOTSUP`, counters are simply absent — exactly as a pts behaves on Linux. **cross-checked.** |
| Modem-line read/set, break (`TIOCMGET`, DTR/RTS) | ✅ | ✅ (expected) | Not gated; serial2 + the shared `sys` ioctls are cross-platform. **expected.** |
| Advertised PTY baud ≥ 460800 (`B460800`/`B921600`) | ✅ | ➖ capped | macOS termios tops out at `B230400`; the high arms are gated out. Advertised baud is cosmetic on a PTY, so it falls through to "unset." **cross-checked.** |
| Identity: `usb:` / `by-path:` resolve to a live path | ✅ | ✖ inert | No `/dev/serial/by-id`, no `by-path` tree, no `/sys`. A node configured this way resolves to nothing and stays **`waiting`** forever. **expected.** |
| Identity: add by present raw path | ✅ | ✅ (expected) | Captures a `raw:<path>` identity with the standard instability warning. **expected** (runtime path **needs a Mac**). |
| Identity: add by bare serial number | ✅ | ✖ unsupported | Needs the deferred IOKit resolver backend (§14). **cross-checked** (falls through to an empty adapter scan). |
| Device-node convention | `/dev/ttyUSB*`, `/dev/ttyACM*` | `/dev/cu.*` | Use the **call-out** (`cu.*`) nodes, **not** `tty.*` (those block on carrier detect). **expected** (macOS convention). |
| Root control socket | `/run/serialnexusd.sock` | `/run/serialnexusd.sock` | `/run` exists on macOS (symlink to `/var/run`). **expected.** |
| Non-root control socket | `$XDG_RUNTIME_DIR/…` | `/tmp/serialnexusd-<uid>.sock` | `XDG_RUNTIME_DIR` is conventionally unset on macOS, so the fallback applies. **cross-checked** (code) / convention **expected.** |
| Stale PTY symlink auto-recovery after a crash | ✅ | ✖ faults | Recovery is keyed on `/dev/pts` (Linux devpts); macOS pts nodes are `/dev/ttys###`, so a stale symlink is **not** reclaimed — the node faults instead. Minor degradation. **cross-checked.** |
| Doctor P1 (EXTPROC/`TIOCPKT`) | ✅ | ❓ | Reports the real delta on a given Mac; a `degraded` verdict means §7.2 runs poll-only. **needs a Mac.** |
| Doctor P2 (PTY presence / `POLLHUP`) | ✅ | ❓ | Presence detection is POSIX but the exact `POLLHUP` timing is **unverified.** **needs a Mac.** |
| Doctor P3 (serial-port fit / UART cert) | ✅ | ➖ degrades | The `TIOCGICOUNT` clause reports unsupported; other clauses need a `--port`. **expected.** |
| Doctor P4 (by-id resolution) | ✅ | ➖ skipped | No by-id tree → `skipped ("no adapter")`. **cross-checked.** |
| Doctor P5 (rig certification) | ✅ | ➖ skipped | Depends on `TIOCGICOUNT` for the error-counter clause; opt-in and rig-bound anyway. **expected.** |
| Doctor env: `dialout`/`plugdev` membership | ✅ | ➖ skipped | `getgroups` is unavailable in nix on Apple, so supplementary membership is reported **unknown/skipped**. macOS serial access is governed by device-node ownership, not these groups. **cross-checked.** |
| Doctor env: device-node access check | ✅ | ✅ (expected) | `access(2)` on the node path is cross-platform. **expected.** |

`➖` = a design fallback engages (a feature is omitted or skipped, by design, with
no fault). `✖` = the feature does not function on macOS today.

## The concrete deltas

### 1. Build: what is gated, and why it is safe

The tree compiles for `*-apple-darwin` because four Linux-only touch-points are
gated behind `cfg`, each onto a fallback the design already had:

- **`TIOCGICOUNT`** (driver overrun/framing/parity counters). libc exports the
  request code only under `target_os = "linux"/"android"`, so the ioctl binding —
  and only the binding — is Linux-gated. Off Linux, `read_icounts`/`read_icounter`
  return `ENOTSUP`, which callers already map to "driver counters unsupported →
  omit them." That is the *same* graceful path a pts takes on Linux (a pts has no
  such counters either), so the code path is well-worn, not new. See
  `serialnexusd/src/sys.rs` and `nexus-doctor/src/sys.rs`.
- **`ptsname_r(3)`** (the reentrant slave-name resolver, a glibc extension). It
  does not exist on macOS, so a localized wrapper uses the static-buffer
  `ptsname(3)` there, copying the `String` out before returning. One wrapper hides
  the split; every caller stays platform-agnostic. Present in the daemon
  (`sys.rs`), the doctor (`sys.rs`), and the sim (`nexus-sim/src/main.rs`).
- **High-baud `BaudRate` arms** (`B460800`, `B921600`). macOS termios caps
  standard speeds at `B230400`, and nix gates those arms out on Apple. The PTY's
  advertised baud is cosmetic anyway, so an out-of-range value simply falls
  through to "unset" rather than being approximated
  (`serialnexusd/src/nodes/pty.rs::standard_baud`).
- **`getgroups`** is unavailable in nix on Apple, so the doctor's `dialout`/
  `plugdev` membership check reports *unknown/skipped* rather than a false verdict
  (`nexus-doctor/src/probes.rs::is_group_member`).

None of these change the design; each is the platform arm of a fallback §13
already promised. **cross-checked.**

### 2. Device identity and the `cu.*` convention

macOS has **no `/dev/serial/by-id` tree, no `by-path` tree, and no `/sys`.** The
resolver's Linux backend reads exactly those, so on macOS it enumerates nothing.
The consequences are concrete:

- **`usb:` and `by-path:` identities are inert at runtime.** A node configured with
  one resolves to no path and stays permanently **`waiting`** (the faulted-and-wait
  posture of §15.25). This is not an error — it is the honest state for "I cannot
  find this device" — but it means the squatter-safe identity forms do not
  function until an IOKit backend lands.
- **Operators use raw call-out paths: `/dev/cu.*`.** Use the **`cu.*`** (call-out)
  nodes, **never** `/dev/tty.*` — the `tty.*` nodes block on carrier-detect and
  will hang an open. A *present* `cu.*` device added by path captures a `raw:`
  identity and carries the standard instability warning (the escape hatch of §12):
  a `raw:` path is "whatever is at this path now," with no squatter protection.
- **Bare serial-number adds are unsupported** until the deferred IOKit resolver
  backend (§14). On macOS the bare-serial branch scans an empty adapter list and
  finds nothing. That backend slots *behind* the existing `Resolver` API with no
  design change (§12) — the `usb:`/`by-path:`/`raw:` fallback chain and the
  identity-vs-path split are already in place; only the discovery source changes.

Adding by raw path still requires the device present at that moment (identity must
be captured); adding or loading by identity never does — but on macOS the only
capturable identity today is `raw:`. **expected** (the code paths are
cross-checked; the live `cu.*` capture **needs a Mac**).

### 3. PTY observation runs the poll-only path

The design observes client termios two ways: promptly, via EXTPROC + packet-mode
(`TIOCPKT`) control packets; and, as an unconditional backstop, via a slow
reconciliation poll (one ioctl every few seconds, effectively free). **EXTPROC/
packet-mode observation is unverified on macOS** (§7.2 says so explicitly). If it
misbehaves there, the reconciliation poll becomes the *sole* mechanism — i.e.
macOS runs the **poll-only observation path**, and the only thing that degrades is
client-termios *latency*; nothing in the data plane depends on the fast path. The
daemon never consults a probe to decide this — the poll is always running.

`nexus-doctor` P1 reports the *actual* delta on a given Mac: `supported` means the
fast path works; `degraded` means poll-only. **needs a Mac.**

### 4. Sockets and paths

- **Root:** `/run` exists on macOS (a symlink to `/var/run`), so the default root
  socket `/run/serialnexusd.sock` works unchanged.
- **Non-root:** `XDG_RUNTIME_DIR` is conventionally unset on macOS, so the daemon's
  socket resolver falls through to **`/tmp/serialnexusd-<uid>.sock`** (see
  `serialnexusd/src/main.rs::resolve_socket`). This is short enough for the
  `sockaddr_un` length limit. Pass `--socket` to override.
- **Stale PTY symlink after a crash:** the auto-recovery that silently reclaims a
  symlink dangling into devpts is keyed on the target starting with `/dev/pts`
  (`serialnexusd/src/nodes/pty.rs::install_symlink`). On macOS, pts nodes are
  `/dev/ttys###`, so that predicate is false: a stale PTY symlink left by a crash
  is **not** reclaimed, and the node **faults** on the pre-existing path instead of
  recovering. A minor degradation — the operator removes the stale symlink by hand
  and restarts the node. **cross-checked.**

### 5. Doctor behavior on macOS

- **P3 / P5 (UART certification)** degrade where `TIOCGICOUNT` is absent: the
  error-counter clause reports `unsupported`/`skipped`, and the surviving clauses
  (custom baud, `TIOCEXCL`, modem lines, break) still run against a named `--port`.
- **Environment group checks are Linux-centric.** `dialout`/`plugdev` do not govern
  serial access on macOS — device-node **ownership** (often `wheel`, or the owning
  user) does. With `getgroups` unavailable on Apple, the doctor reports these as
  *unknown/skipped* rather than guessing. The `access:<node>` read+write check is
  the meaningful permission signal on macOS.
- The `kernel` and `os` environment fields read Linux files
  (`/proc/sys/kernel/osrelease`, `/etc/os-release`); on macOS they render empty /
  `unknown`. Cosmetic — the report is still valid and copy-pasteable.

**cross-checked** for the gating; the live verdicts **need a Mac.**

### 6. How to check on a Mac

Run these and attach the output to any macOS bug report:

```
cargo build                              # confirm it builds on the Mac itself
cargo run -p nexus-doctor -- --markdown  # the capability report; attach it
```

The doctor's P1/P2 verdicts and its environment section are the ground truth for
what actually works on that machine — they turn "macOS is different" into a named
delta instead of a mystery (§13, §15.17).

Exercise the control plane, data path, codecs, legs, taps, and the web console with
the portable **`nexus-itest`** harness (the former bash `scripts/validate/**`, now Rust):

```
cargo test --workspace                        # the whole suite; serial-device tests self-skip on macOS
cargo test -p nexus-itest --test control_plane
cargo test -p nexus-itest --test serial_hardware -- --nocapture   # runs when a crossover rig is attached
```

**CI gate.** The Linux lane runs `nexus-doctor --json | jq -e -f
expectations/linux.jq`. The macOS lane gates on an `expectations/macos.jq`
counterpart — the *looser* profile this page describes: nothing may report
`unsupported`, but P1 may be `supported` **or** `degraded` (poll-only is fine),
P3/P4/P5 may be `skipped`, and the `dialout`/`plugdev` checks may be `skipped`.
That gate and the CI lane are standing up as part of Phase 8; the `linux.jq` gate
is the template it is modeled on.

## Roadmap to "verified"

macOS moves from *best-effort, cross-checked* to *verified* when two things land:
the **macOS CI lane** (build + `nexus-doctor --json` against `expectations/macos.jq`
on every change) and a **hands-on pass** on real hardware that settles the
EXTPROC/packet-mode question (§7.2) and the `cu.*` raw-capture path end to end.
The **IOKit-backed resolver** (§14) is the larger follow-on that restores
`usb:`/`by-path:` identities and bare-serial-number adds; it slots behind the
existing `Resolver` API with no design change.
