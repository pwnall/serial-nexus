# AGENTS.md — working notes for serial_nexus

Orientation for an AI agent (or human) picking this repo up cold. It captures what
the code *is*, how to build/verify it, and the hard-won invariants you must not
regress. When this file and the design disagree, **the design wins** — see
`docs/22-design-claude-fable-v10.md`.

---

## 1. What this is

**serial_nexus** is a permissively-licensed (`MIT OR Apache-2.0`) daemon
(`serialnexusd`) + control CLI (`serialnexusctl`) that manages serial ports as an
explicit, inspectable **directed acyclic graph** of data-routing nodes under one
operator-owned configuration. It exists because embedded serial work looks trivial
(`open /dev/ttyUSB0`, run a terminal) until the realities collide: one UART carries
several multiplexed logical streams; each stream has several simultaneous consumers
that must not interfere; streams must cross machines; concurrent writers corrupt
line/packet protocols so writing needs an exclusive lock with a steal escape hatch;
and USB adapters come and go under changing `/dev` paths, so operator intent must
survive replug/restart/power-cycle. `ser2net`/`socat`/`conserver` each solve a slice
and all three are copyleft; none *compose* demux + PTY fan-out + per-stream logging +
re-mux + cross-machine forwarding under one config.

The stable contract is a **JSON-RPC 2.0 method set over a Unix socket** (design §10);
`serialnexusctl` is an unstable presentation layer over it. Everything is debuggable
with `socat` and `jq`.

**Node types** (design §7): `serial` (owns the physical port, `TIOCEXCL`, reconnect to
same identity), `pty` (interactive pseudo-terminal + stable symlink), `log`
(append-only, on-demand rotation, always read-only toward the device), `codec`
(interior demux/re-mux, framing stays inside the node), `exec`-codec (a `codec` running
an external child speaking the envelope protocol on stdin/stdout — the any-language
escape hatch), `leg` (cross-daemon transport, every channel multiplexed over one
TCP/Unix socket, loopback-only unless opted out). `existing-terminal` (§7.7) is
*design-specified but not implemented*.

## 2. Current status (read this first)

- **Branch:** `implementation` (off `main`). Version **0.2.0** (annotated tag `0.2.0`
  at the phase-8 release mark). Pre-1.0, lab-usable on Linux.
- **Baseline that must stay green:** `cargo test --workspace --locked` — **unit/property
  tests + the `nexus-itest` integration harness (§5)**, now the *only* validation suite
  (the last three bash scripts were folded into `nexus-itest` in v10 §16.11, so `scripts/`
  is gone); `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --locked --
  -D warnings` (+ the minimal-daemon clippy); `cargo deny check`. **The whole suite runs on
  macOS too** (serial-*device* tests self-skip there — §7 — and the real crossover-hardware
  test runs when a rig is attached).
- **All planned phases 0–8 are done**, plus four post-1.0 tracks: the simplification
  track (design §16 / plan §9), the out-of-tree-codec extension track (design §15.26 /
  plan §10), the web console track (design §17 / plan §11.1–§11.6), and the **v10 track**
  (design §15.32 / plan §11.7–§11.9 + §16.11): **default-on replay rings** (64 KiB on every
  host-facing endpoint, per-channel on codec/exec/leg, opt out with `replay_ring = 0`),
  **tap byte offsets** (`tap.data.offset`, `tap.open`'s `from_offset`, `info.instance`
  nonce), **browser-side OPFS history** in `serialnexusweb`, and the bash retirement.
- **Deferred / not implemented on purpose:** design §14 items, and RPC verbs `connect` /
  `disconnect` / `set-attribute` (they return `-32601`). `existing-terminal` node (§7.7).
- **Open review items:** the Opus comprehensive code review is `docs/19-claude-opus-code-review.md`;
  its remediation was committed (`b9d8a50`) and folded into v9. If you touch that area,
  check the review's action table for anything still marked open before assuming it's clean.

## 3. Workspace layout

Rust **edition 2024**, `resolver = "2"`, **MSRV `1.97`** (see §6 — the MSRV is load-bearing).
Cargo workspace; `fuzz/` and `examples/external-codec/` are deliberately **excluded**
(separate toolchain / built from a consumer's position).

| Crate | Kind | Role |
|-------|------|------|
| `codec-api` | lib | Dependency-free codec contract (§8): the multi-channel `Codec` trait, the `Event` vocabulary (data/open/close/error), the versioned **envelope** + daemon-to-daemon **wire frame** (`Hello`, `WIRE_MAGIC`) encode/decode. Has a feature-gated `test_support` conformance kit. |
| `codecs/reference` | lib | `codec-reference`: the reference framing codec over the v1 length-prefixed envelope; doubles as the first demux/re-mux codec and the link codec core; adds **length-guided resync** past corrupt frames (§7.5/§9). |
| `nexus-core` | lib | Pure foundation: `graph` model + 3 structural rules, `data` deliver contracts + holdover, `lock` write-arbitration state machine, `config`/`state` split, `resolver` (dependency-free `/dev`+sysfs device-identity resolution, §12). Property-tested; no I/O. |
| `nexus-rpc` | lib | Thin, stable JSON-RPC 2.0 framing (§10/§15.16): request/response/notification wire types over NDJSON, method params/results left as opaque `serde_json::Value`. Owns the single **`AppError` error-code registry** (§16.8) and dependency-free base64 for `tap.data`. |
| `nexus-sys` | lib | **The workspace's only crate with `unsafe`** (§16.3). Centralizes every ioctl / `ptsname` / nonblocking read-write / `poll(2)` wrapper: `read_icounts` (TIOCGICOUNT), `set_exclusive` (TIOCEXCL), `set_packet_mode` (TIOCPKT), `read_modem_bits` (TIOCMGET), `poll_ready`/`poll_blocking` (**deliberately not tokio `AsyncFd`**, §15.18). Every other crate `#![forbid(unsafe_code)]`. |
| `nexus-daemon` | lib | The daemon as an **embeddable library**: `run`/`RunOptions`/`Registry` entry surface. Wires boundary nodes, the single-thread tokio data-plane runtime, the JSON-RPC control plane, the persisted state file, and the compiled-in codec registry. Largest crate; see §4 for its modules. |
| `serialnexusd` | bin | Deliberately thin binary: parse flags, install tracing, call `nexus_daemon::run` with `Registry::with_builtins()`. All logic lives in `nexus-daemon`. |
| `serialnexusctl` | bin | Thin JSON-RPC client CLI. Subcommands → requests over the Unix socket; renders structured replies; `--json` is a raw pass-through of the daemon `result`. |
| `serialnexusweb` | bin | Standalone loopback HTTP+WebSocket console that is a **pure RPC client** of the daemon (the daemon gains no HTTP). Filtering JSON-RPC proxy; enforces per-session token + Host validation; **refuses graph/lifecycle verbs** (§17). Hand-rolled HTTP on tokio; `tokio-tungstenite` WS; TLS via `rustls`+`rcgen` pinned to the **ring** backend. |
| `nexus-sim` | bin | Deterministic **test double** (plan §3): PTY doubles, client drivers, in-process null-modem, TCP link-outage proxy, wire/envelope/exec conformance batteries. Emits one machine-readable JSON verdict line per run. Uses the daemon's own permissive PTY/socket calls. `publish = false`. |
| `nexus-doctor` | bin | Shipping **capability checker** (§15.17). Passive kernel probes P1 (EXTPROC/TIOCPKT), P2 (PTY POLLHUP presence), P4 (by-id resolver) + opt-in real-port P3 (serial fit) and P5 (rig cert). Markdown or `--json`. **Attach its output to any bug report.** |
| `nexus-itest` | lib+tests | The **cross-platform integration harness** (§5), which replaced the bash `scripts/validate/**`. `src/lib.rs`: boots `serialnexusd` on a temp socket, an in-Rust JSON-RPC client (`Rpc`), a streaming `Subscription` (`subscribe`/`tap`), `nexus-sim` subprocess doubles, `serial_pair`/`serial_echo` (Linux sim) / `crossover_ports` (real HW) providers with self-skip, and `sha256_hex`. `tests/*.rs`: one file per former phase script. `publish = false`. |

Dependency direction: `nexus-daemon` → {`nexus-core`, `nexus-rpc`, `nexus-sys`,
`codec-api`, `codec-reference`}; both client bins → {`nexus-rpc`, `nexus-core`};
`nexus-sim`/`nexus-doctor` → `nexus-sys` (+ `codec-api` / `nexus-core`).

### Key files inside `nexus-daemon/src/`
- `lib.rs` — public API (`run`/`RunOptions`/`Registry`); socket + state-file path policy; startup load.
- `daemon.rs` — graph state + all RPC verb impls; the two-lane control plane (§15.20). Largest file.
- `control.rs` — JSON-RPC 2.0 over NDJSON on the Unix socket; one task per connection; cancel-safe waiting.
- `runtime.rs` — data-plane runtime: endpoint-keyed mpsc wiring, `poll(2)` readiness, and the shared **`frame_ranges`/`frame_payload_cap`** targetward-fragmentation helper (§5/§15.19/§15.27).
- `boundary.rs` — shared boundary-supervisor primitives (park / race3 / `BlockingReader` / `Backoff`), property-tested (§16.1).
- `cell.rs` — `CriticalCell`, the `RefCell` wrapper that makes "a borrow never crosses `.await`" a compile-shape fact (§16.2).
- `registry.rs` — codec `Registry` value (`with_builtins`/`register`); **no dynamic loading** (§8/§15.26).
- `tap.rs` — connection-scoped taps + per-endpoint replay ring (§5/§6/§17).
- `nodes/` — `Node` enum + per-node runtimes: `serial`, `pty`, `log`, `codec`, `exec`, `leg`.

## 4. Build / test / lint (exact commands)

```sh
cargo build --workspace --locked
# The one suite: unit/property tests + the nexus-itest integration harness. It builds
# every binary first (serialnexusd/nexus-sim/serialnexusweb/nexus-doctor) so the harness's
# bin() lookups resolve; the exec/envelope codec tests need python3, and the folded
# license-gate/external-codec/web-history tests shell out to cargo-deny/cargo/node and
# self-skip when the tool is absent.
cargo test  --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
# The minimal daemon (no built-in codecs) must ALSO be warning-clean:
cargo clippy -p serialnexusd -p nexus-daemon --no-default-features --locked -- -D warnings
# macOS portability gate (no Mac needed — it type-checks the cfg resolution). NOTE: the
# `ring` crate (serialnexusweb's TLS dep) cannot cross-build from Linux, so exclude it —
# the real macOS gate is `cargo test --workspace` on a Mac runner:
cargo check --target x86_64-apple-darwin --workspace --exclude serialnexusweb
# Licensing gate (permissive-only), proven not assumed. The second command is the folded
# gate that plants a banned crate and asserts cargo-deny rejects it (self-skips without it):
cargo deny check licenses bans sources
cargo test -p nexus-itest --test p0_license_gate --locked
# Run one test file, or the #[ignore]d endurance soak:
cargo test -p nexus-itest --test p4_steal_lease
cargo test -p nexus-itest --test p8_soak -- --ignored
```

`--locked` everywhere: **`Cargo.lock` is committed** (plan §2). CI (`.github/workflows/ci.yml`)
runs per-push jobs `check` (fmt + clippy ×2 + `cargo test --workspace`, which now carries
the whole integration suite), `license-gate`, `doctor`, `external-codec`, and **`macos`**
(the same `cargo test --workspace` — serial-device tests self-skip — plus the now-gating
`macos.jq` doctor check); `soak-nightly` / `sweep-nightly` (`--include-ignored`) /
`fuzz-nightly` are `schedule`-only. CI toolchain is pinned to **1.97** for the `check` job.

## 5. The validation harness (how "done" is proven)

The harness is the **`nexus-itest` crate** — portable Rust integration tests, run by
`cargo test` like any other. It **replaced the bash `scripts/validate/**` maze** (2026-07-24),
which was not macOS-portable: `stat -c`, `nc -q`, `sha256sum`, `timeout`, and
`/dev/serial/by-id` all diverge across Linux/macOS. Each former phase script became a
`nexus-itest/tests/<name>.rs` (e.g. `p4_steal_lease.rs`, `p6_outage.rs`, `p8_web.rs`);
`src/lib.rs` is the shared foundation. A test that cannot run on a platform **self-skips**
(`eprintln!("SKIP …"); return`) — the same skip-is-valid discipline the bash rig had.

**Iron conventions — follow them when adding tests:**
- **Assert on structured RPC results / byte-exact SHA-256, never CLI text.** Drive the
  daemon via the in-Rust `Rpc` client (`d.rpc().call(method, json!({…}))` and helpers
  like `send`/`lock`/`load_toml`/`state`/`wait_status`); ground-truth for data-plane
  claims is `sha256_hex(bytes)` or a `nexus-sim`-reported checksum, never a judgement.
- **Serial *device* tests skip off Linux.** A pty **cannot be a serial device on macOS**
  (`serial2` → `ENOTTY`), so `serial_echo()` (single echo device) and `serial_pair()`
  (lossless cross-wired null modem) are **Linux-only** (sim-backed) and return `None`
  elsewhere → the test skips. Nodes that need no serial device (pty, log, codec, exec,
  leg, tap, control-plane) run on **every** platform. The real macOS serial path is the
  dedicated `serial_hardware.rs` test via `crossover_ports()` — it reads through the
  daemon's own fast, lossless reader (a flow-control-less UART drops bytes under a raw
  high-volume read, so *that* is where hardware byte-exactness is asserted).
- **Meta-gates are proven, not assumed.** `tests/meta_gates.rs` scans the tree and
  asserts `unsafe` is confined to `nexus-sys/` (with a planted-`unsafe` self-proof), and
  asserts `nexus-doctor` reports no `unsupported` capability. The licensing gate is
  `tests/p0_license_gate.rs` (folded from bash in v10 §16.11): it plants a banned crate and
  asserts cargo-deny rejects it, self-skipping where cargo-deny is absent. `p8_external_codec.rs`
  builds the out-of-tree template from a consumer's position, and `p8_web_history.rs` runs
  the browser history module's `node --test` (self-skip without node). **No bash remains.**
- **No bare sleeps.** Use `wait_until(Duration, || cond)` / `rpc.wait_status(…)` /
  `Subscription::wait_for(…)`. `Daemon`/`Sim`/`TempRun` clean up on `Drop` (kill children,
  remove the temp dir), so a panicking test never leaks a daemon or a socket.
- **Heavy/endurance tests are `#[ignore]`d** (e.g. `p8_soak::soak_endurance`, SOAK_*-env
  parameterized) and run in the nightly `--include-ignored` sweep, not per push.

**Hardware rig:** `serial_hardware.rs` — two USB-serial adapters cross-wired as a null
modem (each is the other's target), auto-detected via `crossover_ports()`
(`/dev/cu.usbserial-*` on macOS, or `SNX_CROSSOVER_A`/`_B`). **Self-skips when absent.**
There are **no shell scripts left** (v10 §16.11): the former tooling wrappers — the
license gate, the external-consumer build, and their `wait-for` helper — are now
`nexus-itest` tests that spawn the same tools directly (`cargo-deny`, `cargo`, `node`),
each self-skipping when its tool is unavailable. Sim doubles stay *subprocesses*
deliberately (cross-process scheduling realism exposed §15.19's timer floor).

## 6. Load-bearing invariants — DO NOT REGRESS

These are settled by real bugs and benchmarks. Each cites where it lives.

1. **No `AsyncFd`/epoll on pty/tty fds.** tokio's `AsyncFd::readable()` busy-loops on a
   pty master (epoll reports ready forever while `read` gives EAGAIN), starving the
   current-thread runtime. Readiness for tty-family fds is non-blocking `poll(2)` with an
   adaptive idle backoff (`nexus-sys::poll_ready`, §15.18). Do not reintroduce `AsyncFd`
   for pty/tty.
2. **High-rate hostward paths run on dedicated blocking threads.** The serial reader and
   the PTY writer park in **blocking `poll(2)`** (`nexus-sys::poll_blocking`,
   `nexus-daemon/src/boundary.rs`): ~185 MiB/s, lossless, ~0 CPU idle. The async poll loop
   caps at ~1 MB/s — do **not** "simplify" the reader/writer back onto it (§15.19).
3. **Never silently drop targetward bytes.** An oversize producer chunk is **fragmented**
   across frames via the one shared helper `runtime::frame_ranges` — never skipped on an
   encode error. This skip-on-error bug shipped three times in three framers before review
   caught it (§5/§15.27). The in-process codec, `leg`, and `exec` all fragment through that
   one helper. Guard test: `targetward_oversize_chunk_is_fragmented_never_dropped`.
4. **All `unsafe` lives in `nexus-sys`.** Every other crate is `#![forbid(unsafe_code)]`;
   `nexus-itest/tests/meta_gates.rs` (`unsafe_is_confined_to_nexus_sys`) proves the confinement.
5. **No `std::cell::RefCell` in the daemon.** `serialnexusd/clippy.toml` bans it via
   `disallowed-types`; daemon state lives in `nexus_daemon::cell::CriticalCell`, whose contents
   are reachable only inside a synchronous `with`/`with_mut` closure, so a borrow **cannot
   cross an `.await`** (§16.2). (`CriticalCell`'s own internal `RefCell` carries a localized
   `#[allow]`.)
6. **MSRV 1.97 is a two-way constraint.** The code uses **let-chains** (need ≥1.88) and
   clippy 0.1.97's `collapsible_if` *requires* collapsing nested `if { if let }` **into**
   let-chains. 1.85 and 1.97 clippy are mutually incompatible here — do **not** lower MSRV
   without `#[allow]` churn.
7. **Config vs state split.** Configuration is operator-owned, round-trippable, and only
   fails on *structural* invalidity; state is environment-owned and never persisted.
   Environmental failure (missing device, unwritable dir) changes a node's *state*, never
   the graph. Node names and channel identities may not contain `/` and may not be
   empty/whitespace-only — structural validation errors (§3/§12).
8. **Arbitration default is `exclusive`.** Only the write-lock holder's bytes are read
   targetward (non-holders are simply not read = backpressure, no drop). A lone PTY needs
   an explicit `lock` to write, or the node set to `arbitration = "free-for-all"`. The
   `send` verb self-acquires the lock. Do not weaken the gate to "fix" a test.
9. **The replay ring is bulk-memcpy, and default-on (64 KiB).** Since v10 §15.32 every
   host-facing endpoint carries a `replay_ring` (default 65536, opt out with `0`), so its
   hostward mirror + hub run on the hot path of *every* endpoint. `tap::ReplayRing` MUST stay
   a fixed circular `Vec<u8>` written with `copy_from_slice` — a byte-at-a-time `VecDeque`
   `drain`+`extend` starved the runtime thread and collapsed the 256 MiB firehose from 2.5 s
   to ~1.9 MB/s (measured, then fixed). Guard: `p3_firehose` completes well under its 60 s
   bound. `discarded_unattached`/`discarded_no_client` accounting stays independent of the
   mirror (the ring is a spy *outside* the graph, §5) — guard `active_tap_feed_does_not_hide_unattached_loss`.
10. **`tap.data` offsets are monotonic per boot; `from_offset = ingested − ring.len()` never
   underflows.** Every `ingest` both pushes to the ring and advances `ingested`, so the ring
   holds `≤ ingested` bytes by construction (`nexus-daemon/src/tap.rs`, §11.8). `info.instance`
   is a per-boot nonce so a client detects the offset reset across a restart. Do not stamp an
   offset *after* advancing `ingested`, or splice-exactness breaks.

For the deeper code-level invariants (purge-on-acquire runs synchronously at grant time;
the exec pump polls stdin/stdout/stderr concurrently to avoid deadlock; serial
faulted-and-wait parks receivers unread rather than draining; etc.) see
`docs/implementation-notes.md` (§3.x deviations, §6a–§6f per-phase writeups) — it is the
running engineering log and the authoritative "why the code looks like this" record.

## 7. Platform & kernel constraints

- **Linux is required** and is the kernel of record. **Production target is Linux 6.18;
  the dev box runs 7.0.** You can run code on 6.18 (the user can; an agent here cannot).
  `nexus-doctor` has been confirmed **all-probes-supported on 6.18** (Debian rodete,
  zero deltas from 7.0). **Pause and check with the user before any one-way
  (hard-to-reverse) decision that depends on a kernel ability confirmed only on 7.0**, and
  keep the design's fallbacks live (the §7.2 termios reconciliation-poll backstop; P2
  slave-priming for presence). Re-gate on 6.18 with
  `nexus-doctor --json | jq -e -f expectations/linux.jq`.
- **macOS is best-effort** (`docs/macos.md`): the tree compiles and degrades gracefully;
  `#[cfg]`-gated blockers are `TIOCGICOUNT` and `ptsname_r` (Linux-only). The gating CI
  deliverable is only that it *builds* + portable tests pass. **Windows is out of scope.**
  - **Doctor P2 on macOS is `degraded`, and that is correct** (`macos.jq` accepts
    supported-or-degraded): the BSD master is not a terminal, so the baseline termios is
    applied via a momentarily-opened slave (§7.2 platform arm). Linux is `supported`. The
    verdict split is `termios_settable`, **not** `never_opened` — a v10 fix (`probes.rs`)
    corrected a regression that had wrongly gated Linux `Supported` on `never_opened` (which
    no Linux satisfies — a never-opened master doesn't HUP, §3.2), demoting native Linux to
    `Degraded`. If a fresh session sees P2 `degraded` on **Linux**, that is a real problem;
    on macOS it is expected.
  - **The macOS local cross-check must exclude `serialnexusweb`:** `cargo check --target
    x86_64-apple-darwin --workspace --exclude serialnexusweb` — the `ring` crate (its TLS dep)
    cannot cross-build from Linux. The real macOS gate is `cargo test --workspace` *on the Mac*,
    where `ring` builds natively and this exclusion is unnecessary.
  - **Serial-*device* itest tests self-skip on macOS** (a pts can't be a serial device there —
    `serial2` → `ENOTTY`); the real macOS serial path is `serial_hardware.rs` via
    `crossover_ports()` (`/dev/cu.usbserial-*`, or `SNX_CROSSOVER_A`/`_B`), self-skipping with
    no rig. `p8_web_history.rs` runs the browser history module under `node --test` and
    self-skips without `node`.
- **serial2, not serialport.** The MPL `serialport`/`mio-serial`/`tokio-serial` stack and
  LGPL `libudev` bindings are **banned in `deny.toml`**. `serial2` is opened blocking and
  driven by the daemon's own poll-based readiness; even `serial2-tokio` was dropped because
  it hides the inner fd that `TIOCEXCL`/`TIOCGICOUNT` need.

## 8. Gotchas that have burned prior sessions

- **`pkill -f serialnexusd` / `pgrep -f serialnexusd` matches the current shell** (its own
  cmdline contains the pattern) → a following `kill` can kill your shell (exit 144, empty
  output — *not* a daemon crash). Use `pgrep -x serialnexusd` (name-only) to find real
  strays, or start the daemon with `nohup … & disown`. Validation scripts kill by explicit
  `$DPID` and are safe.
- **`git checkout -- <file>` reverts ALL uncommitted work in that file.** To remove a
  temporary planted line, use a targeted `Edit` (or commit first) — never `checkout --`.
- **Unix socket paths are bounded (~108 bytes, `SUN_LEN`).** The long scratchpad path
  overflows it. Tests use a short `mktemp -d /tmp/snx-*.XXXXXX` as `XDG_RUNTIME_DIR`; the
  socket is always `$XDG_RUNTIME_DIR/serialnexusd.sock`.
- **Device access:** the dev-box user is in the `dialout` group (both FTDI ports open r/w).
  The old "plugdev-not-dialout / access pending" note is stale.

## 9. How work has been done here (the working rhythm)

- **Design/plan pairs are version-suffixed and monotonic.** The newest pair lives in
  `docs/` (currently v10: `22-design-…-v10.md` + `23-implementation-plan-…-v10.md`);
  superseded generations move to `docs/historical/`. `§N` always means the *current*
  normative design. ADRs are numbered subsections under design **§15.x** (plus §16
  post-completion review, §17 web console). The RPC method-by-method reference is
  `docs/rpc/` (README + one page per verb group); design §10 is its normative source.
- **Every phase/track has ended with a multi-agent adversarial audit** (per-area finders +
  independent verifiers; each finding verified before it's accepted, then fixed by aligning
  code to design). This is the expected bar for substantial changes — find, verify, fix,
  add a regression guard. `docs/implementation-notes.md` records the confirmed/refuted
  counts per phase.
- **Commit discipline:** work happens on `implementation`; the user reviews before commit
  and before any `main` merge. Do not push or merge to `main` without being asked. Commit
  messages here are section-scoped (e.g. "v9 §11.3-6: the web console client").
- **Before asserting any file:line as fact, re-read it** — much of the surrounding
  knowledge was captured point-in-time and the code moves.
