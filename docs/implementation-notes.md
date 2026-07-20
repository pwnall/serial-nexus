# serial_nexus — implementation notes & handoff

**As of:** 2026-07-20 (phase 0-3 done and audited against v4). **Branch:**
`implementation` (off `main`).
**Normative docs:** `docs/09-design-claude-fable-v4.md` (design) and
`docs/10-implementation-plan-claude-fable-v4.md` (plan). v1/v2/v3 docs (03–08) are
in `docs/historical/`. Section references (§) point at the v4 design.

**v3 revision (2026-07-20).** The v3 docs folded the refinements below (§3.1–3.10)
into the design text and added two new normative requirements that phase 0-2 code
was realigned to satisfy: (a) design §3 now makes a node name or channel identity
containing `/` a **structural validation error** — enforced in
`nexus_core::graph::GraphModel::validate` (`ValidationError::InvalidName`); and
(b) plan §2 now requires **`Cargo.lock` committed** (the cargo-deny gate is only as
strong as the committed graph) — `Cargo.lock` was un-gitignored and checked in. The
lingering `serial2-tokio` workspace-dependency declaration was also dropped (§13,
§15.1), matching the design narrative that it was removed during implementation.

**v4 revision + audit (2026-07-20).** The v4 docs are v3 plus one substantive
change: the phase-3 hybrid data plane (§3.11 below) was folded into design §5 and
recorded as a new ADR **§15.19** ("The benchmark cashed the escape hatch: a hybrid
data plane"), with **§15.18** now carrying a "(Superseded in part by §15.19)" note.
The split is now clean: §15.18 owns only the *poll(2)-not-epoll / `AsyncFd`-
prohibition* decision, while §15.19 owns the *dedicated blocking threads for the
hot hostward paths* (serial reader, PTY master writer) and the *adaptive
active-to-idle backoff* for the cold async paths. Phase 0-3 was then re-audited
against v4 (multi-agent + adversarial verify). Two genuine deviations were found
and fixed: (a) the PTY node re-asserted the baseline termios on last close only
when the close was observed via POLLHUP, skipping it when the read path saw
EOF/EIO first (§7.2) — `nodes/pty.rs` now does a swap-guarded reset on all three
paths, and the reconciliation backstop is gated on live presence; (b)
`scripts/validate/phase3/subscribe.sh` used a bare `sleep 0.3` to await
subscription registration, against plan §3 — now a bounded `wait-for` on the first
snapshot. Code comments that cited §15.18 for the thread/backoff decision were
repointed to §15.19. No other phase 0-3 deviations surfaced.

This document records where the implementation stands and every place the code
deviates from — or refines — the design. The rule from plan §1 holds: where
implementation reality disagrees with the design, the design gets amended first;
the items below are refinements consistent with the design, none contradict it.

---

## 1. Status at a glance

| Phase | Scope | Status |
|-------|-------|--------|
| 0 | Doctor + scaffolding | **done** — `nexus-doctor`, CI, cargo-deny gate |
| 1 | Contracts in the small | **done** — nexus-core, codec-api, nexus-sim |
| 2 | Walking skeleton | **done** — control plane + node lifecycle + data plane (serial↔PTY byte flow, presence gating, backpressure) |
| 3 | Boundaries & logging | **done** — drop counters, log node, `rotate`/`subscribe`, client-termios, high-throughput data plane + benchmark (§3.11) |
| 4 | Arbitration | **in progress** — slice A done: per-endpoint exclusive write lock, `lock`/`unlock`, the `may_write` gate on the PTY targetward drain, lock state in `state` (§3.12). Slices B (purge/detach-release/free-for-all) and C (send/steal/lease/wait/notifications) pending |
| 5–8 | Codecs, wire, identity, hardening | not started |

**Quality gates (all green):** `cargo fmt --all --check`, `cargo clippy
--workspace --all-targets --locked -- -D warnings`, `cargo test --workspace`,
and `bash scripts/validate/all.sh --through 3` (phase 3 adds `counters.sh`,
`log.sh`, `log-enospc.sh`, `subscribe.sh`, `firehose.sh`, `exact-loss.sh`,
`benchmark.sh`).

**Kernel matrix:** every kernel-behavior probe is `supported` on **Linux 7.0.0**
(dev box, Ubuntu 26.04) and **Linux 6.18.14** (Debian rodete) with **zero
deltas** — see `docs/nexus-doctor.md`. The kernel-sensitive PTY/serial mechanics
are de-risked across the support matrix.

---

## 2. Where the code lives

| Crate | Role | State |
|-------|------|-------|
| `codec-api` | codec trait, event vocabulary, envelope frame codec + golden vectors (§8/§9) | done for phase 1; reference codec is phase 5 |
| `nexus-core` | graph model + 3-rule validator (§4), data-plane deliver contracts + holdover (§5), config/state split (§15.8) | done for phase 1 |
| `nexus-rpc` | JSON-RPC 2.0 wire types — the stable §15.16 surface | done |
| `nexus-sim` | test double: `pty`/`client` modes (§3) | phase 1 modes done + `client --report-termios`; `mux`/`envelope`/`wire`/`tcp-proxy` later |
| `nexus-doctor` | shipping capability checker: probes P1–P4 + env checks (§15.17) | done |
| `serialnexusd` | the daemon | control plane + node lifecycle + data plane done |
| `serialnexusctl` | the CLI (thin RPC client + `--json`) | `load`/`dump`/`state`/`teardown`/`shutdown` done |

`serialnexusd` modules: `main.rs` (runtime, socket policy, shutdown),
`control.rs` (JSON-RPC over UDS), `daemon.rs` (graph state + method impls),
`runtime.rs` (data-plane wiring + poll-based I/O helpers), `nodes/{mod,pty,serial}.rs`
(node runtimes), `sys.rs` (the single unsafe-bearing module: `TIOCEXCL`/`TIOCPKT`
ioctls, raw `read`/`write`/`fcntl`, and the non-blocking `poll_ready`).

Validation scripts are the canonical exit criteria (plan §3):
`scripts/validate/phaseN/*.sh`, each self-judging with a JSON verdict and exit
code. Helpers in `scripts/lib/` (`wait-for.sh`, `semantic-diff.sh`).

---

## 3. Deviations & refinements from the design

These are implementation decisions the design does not spell out, or where a
kernel/library reality shaped the approach. None contradict the design.

### 3.1 Serial node uses blocking `serial2` + poll-based readiness, not `serial2-tokio`
**Design:** §13 lists `serial2`/`serial2-tokio` for "concurrent async read/write."
**Reality (nexus-doctor P3 research):** `serial2-tokio` 0.1.24 exposes **no
accessor for the inner fd**, and `serial2` **does not take `TIOCEXCL`** (only
`O_NOCTTY`). The daemon needs the raw fd for `TIOCEXCL` (§7.1) and later
`TIOCGICOUNT` (§5).
**Decision:** open a blocking `serial2::SerialPort` (settings, modem lines,
break, and the raw ioctls via `as_raw_fd`), set it non-blocking, and drive async
I/O with poll-based readiness (see §3.10) — rather than `serial2-tokio`.
Consistent with §13's "raw termios via nix/rustix as the fallback." `TIOCEXCL` is
issued by the daemon itself (`nodes/serial.rs`). `serial2-tokio` is now an unused
dependency and was dropped from `serialnexusd/Cargo.toml` — and, in the v3
realignment, from the root `Cargo.toml` `[workspace.dependencies]` as well, so the
design's "dropped during implementation" (§13, §15.1) is literally true of the
manifest.

### 3.2 PTY slave is *primed* at creation (POLLHUP never-opened refinement)
**Design:** §7.2 detects presence via the master's HUP condition.
**Reality (nexus-doctor P2):** a master whose slave was **never opened** does
**not** report `POLLHUP`; HUP only appears after the first open→close. So HUP
alone cannot represent the initial no-client state.
**Decision:** at PTY node creation, open and immediately close the slave once
(`prime_slave` in `nodes/pty.rs`). This forces the "absent" HUP state, so
presence detection via POLLHUP is uniform from the start. This step is not in the
design text; it is a faithful refinement of §7.2's model, confirmed identical on
7.0 and 6.18.

### 3.3 Data-plane holdover needs an explicit `flush` on resume
**Design:** §5 — a transform that has emitted output when downstream refuses
"parks it in its holdover slot."
**Refinement:** a chunk parked on the *last* offer would be stranded if the
runtime only retries on new origin input. `nexus-core::data::TargetwardSink` has
a `flush()` method the runtime calls when a boundary becomes writable,
independent of new input, draining parked holdovers in order. Caught by a
property test (`prop_targetward_no_loss_bounded_interior`). v4 §5 now names this
explicitly ("boundaries announce writability, and the runtime drains parked
holdover frames on that signal, independent of any new origin input").

### 3.4 `EndpointAddr` serializes as its display string
**Design:** §3/§15.12 — display form is `node/channel`; neither part contains `/`.
**Decision:** in configuration, an endpoint address serializes as that **string**
(`"usb0"` or `"mux/console"`), not a nested `{node, endpoint}` table. This keeps
edges all-scalar and TOML-clean and makes configs read the way operators write
them. The design does not specify the on-disk encoding of an address; this is a
presentation choice. (`nexus-core::graph::EndpointAddr`.)

### 3.5 JSON-RPC `id: null` and result-XOR-error validation
**Design:** §10 — hand-rolled JSON-RPC 2.0.
**Refinement (from an adversarial review):** `nexus-rpc` now has an `Id::Null`
variant and `Response::error_without_id`, so a parse-error / invalid-request
reply carries the spec-mandated `id: null` (JSON-RPC 2.0 §5) and never desyncs a
client's read stream; and `Response`'s deserializer enforces exactly-one-of
`result`/`error` (distinguishing a present `result: null` from an absent one).
Completes §10's contract; not a deviation.

### 3.6 `load` RPC carries the config as JSON, not TOML text
**Design:** §10 — "Configuration files are TOML; the RPC carries JSON."
**Decision:** `serialnexusctl` reads the `.toml` file, parses it to
`GraphConfig`, and sends `{"config": <GraphConfig as JSON>}` in the `load`
params; `dump` returns the config as JSON and the CLI renders TOML. The CLI owns
the TOML↔JSON conversion (presentation, §15.16); the daemon speaks only JSON.

### 3.7 Daemon-specific error codes
`load` on a non-empty graph → `-32001`; a structural validation failure →
`-32002` (with all offenders in `error.data.errors`). Both in the reserved
application range `[-32099, -32000]` (§10). `nexus-rpc::error_codes` unchanged.

### 3.8 `advertised_baud` maps to standard rates only
PTY `advertised_baud` is cosmetic (§7.2). nix on Linux sets termios speed via a
`BaudRate` enum (standard rates only), so a non-standard advertised baud is
skipped rather than approximated. (`nodes/pty.rs::standard_baud`.)

### 3.9 Unimplemented node kinds were a structural load error (resolved in phase 3)
Before phase 3, a configuration containing a **log** node was rejected at load
(`node <name>: log nodes land in phase 3`), nothing created — a build-stage
limitation, not a design position. Phase 3 (slice B) implemented the log node and
removed the rejection; a log node now loads normally. Kept here only as a record.

### 3.10 Data-plane readiness is poll-based, not `tokio::AsyncFd` (the pty-master spin)
**Design:** §5 — a single-threaded async data plane; the design does not name a
readiness mechanism.
**Reality (found while wiring slice 2):** `tokio::io::unix::AsyncFd` (epoll)
**spuriously and persistently reports a pty master readable** once an external
client is attached — `readable()` returns ready every poll while `read(2)` gives
`EAGAIN` and a direct `poll(2)` reports *no* readiness (epoll disagrees with
`poll`). Because `readable()` completing synchronously never yields, this
busy-loops and **starves the entire current-thread runtime** (every other task,
including the control plane, freezes until an unrelated I/O event — e.g. the
client disconnecting — breaks the loop). Reproduced in isolation; independent of
packet mode, the sync presence poll, shared-vs-dup fds, and `select!`. It is a
genuine epoll/pty-master quirk, and `AsyncFd` is unsuitable for these fds.
**Decision:** drive readiness with a **non-blocking `poll(2)`** (`sys::poll_ready`,
zero timeout — returns immediately, never blocks the thread) plus a short async
`tokio::time::sleep` (`runtime::IDLE_POLL`, 5 ms) only when idle. During an active
transfer a task re-polls immediately after each full drain, so the interval
bounds idle latency (and idle CPU — measured ~1%), never throughput (1 MiB
echo round-trips in ~0.5 s). Reads: `poll(POLLIN|POLLHUP)` → drain to `WouldBlock`.
Writes: `write(2)` then, on `WouldBlock`, `poll(POLLOUT|POLLHUP)` + sleep. This
applies to **both** node types uniformly (`runtime.rs`, `nodes/{pty,serial}.rs`);
a real UART tolerates epoll but the daemon must also drive the PTY master and
(in tests) pts-backed "devices", so one poll-based path is simplest.
**Future:** idle CPU is a fixed ~1%/idle-fd today; a longer or adaptive idle
interval, or a `spawn_blocking` reader thread for high-baud serial, is a phase-3
optimization if the throughput benchmark demands it. `AsyncFd` is *not* the
answer for pty masters.

### 3.11 The phase-3 benchmark demanded §15.18's thread escape hatch (both axes)
**Design:** §15.18 frames the poll(2) readiness as bounding "idle latency, never
throughput" (re-poll immediately during active transfer), with `spawn_blocking`
reader threads as an escape hatch *if the benchmark demands it*, and idle CPU as
the named concern (~1%/idle-fd).
**Reality (phase-3 benchmark):** on the current-thread runtime the "re-poll
immediately" intuition does **not** hold for a peer in a *separate process* — a
`yield_now` spin returns instantly (no other runnable task), so no wall-clock
passes and the peer never refills; the wait therefore always pays the ~1 ms tokio
timer floor per buffer cycle, capping hostward throughput at **~1 MB/s** (measured
1.2 MiB/s serial→log). That is below even one 3 Mbaud port for a fast consumer —
so the escape hatch was **required**, not optional, exactly as §15.18 reserved.
**Decision:** the two high-throughput paths — the **serial hostward reader** and
the **PTY hostward writer** — run on **dedicated blocking threads** doing a
*blocking* `poll(2)` (`sys::poll_blocking`), which the kernel wakes the instant
the fd is ready. Result: ~185 MiB/s, lossless, and **zero** CPU while parked
(a blocked poll costs nothing — this also dissolves the idle-CPU concern for
these fds). Cross-thread counters became atomics (`Rc`→`Arc`, `Cell`→`Atomic*`);
the PTY writer is fed by an async pump through a **bounded** bridge so the buffer
stays bounded and full-buffer drops are counted. Low-rate paths (targetward
PTY→serial, PTY presence/termios) stay async poll-based, now with an
`ACTIVE_POLL`→`IDLE_POLL` adaptive backoff → **~0.06%/idle-fd** (2% total for 32
idle PTYs, well under budget; the §15.18 idle-CPU concern, resolved).
**Recorded:** `docs/benchmarks/phase3.json` (throughput + idle axes);
`scripts/validate/phase3/{firehose,exact-loss,benchmark}.sh`. **Folded into the
design in v4:** this decision is now ADR **§15.19** and §5's "hybrid" paragraph,
and §15.18's "never throughput" claim is corrected there (it held only until the
hot hostward path moved to a blocking thread). The design pass this section asked
for is done; the code comments were repointed from §15.18 to §15.19 to match.

### 3.12 Arbitration addressing: `lock`/`unlock` name the origin, not the endpoint
**Design:** §6 shows `serialnexusctl lock <node/channel>` and `send <node/channel>`
without pinning down whether `<node/channel>` is the origin acquiring the lock or
the host-facing endpoint being locked.
**Decision (phase 4, slice A):** the lock lives on a **host-facing endpoint** (the
serial node), but the RPC `lock`/`unlock` name the **origin** — the target-facing
writer (a PTY) that acquires it. The daemon resolves the origin to the unique
endpoint it feeds (a target-facing endpoint has exactly one edge, §4). This is what
makes the reference workflow coherent: `lock ptya` grants *ptya* the write lock so
its operator can type, while other origins on the same serial are locked out. The
later `send` verb (slice C) instead names the **target** endpoint, since the CLI is
itself the transient origin. This is a presentation/RPC-shape choice the design
leaves open (§15.16); the state machine (`nexus_core::lock`) is addressing-agnostic
(it keys on an opaque `OriginId`), so a future spelling change costs only the daemon
glue. **Architecture:** the lock is a pure state machine in `nexus_core::lock`
(property-tested); the daemon shares one `Rc<RefCell<EndpointLock>>` per endpoint
(all tasks are on the one runtime thread) between the control-plane methods that
mutate it and each origin's PTY read task, which consults `may_write` before
draining targetward. A non-holder is *not read from* (its bytes stay in the kernel
buffer — backpressure, never dropped), so arbitration reuses the §5 pause machinery
and adds no data path, exactly as §6 requires. The serial node's host endpoint
carries a new `arbitration = exclusive | free-for-all` config attribute (§6).
**Purge-on-acquire runs synchronously in the daemon's `lock` at grant time**
(draining the origin's master fd via `Node::purge_origin` before the grant reply
returns), *not* lazily in the reader task — a lazy drain would race a correct
acquire-before-write client's first command and discard it (caught by an
adversarial review; guarded by `phase4/purge.sh` check 3).

**Known limitation — sub-poll close+reopen (poll-based presence).** Detach-release
and purge-on-detach hinge on observing the PTY's present→absent transition via
level `POLLHUP` (§7.2). If a client closes and a *different* client reopens the
same slave within one poll interval (≤ `IDLE_POLL`, 5 ms for a quiescent origin),
the transition is unobservable — the successor inherits the predecessor's lock
without an explicit re-acquire, and the baseline termios is not re-asserted. This
is inherent to poll-based presence (the §15.18/§15.19 tradeoff), not a logic bug;
it affects only the detach-release path (an explicit `unlock` is unaffected) and
never lets a *different endpoint's* origin write (exclusion still holds). A
per-open generation/epoch would close it if it ever matters; deferred.

---

## 4. Findings carried forward (from nexus-doctor)

Full report: `docs/nexus-doctor.md`. Re-runnable per system with
`cargo run -p nexus-doctor` (Markdown) / `--json | jq -e -f expectations/linux.jq`.

- **P1 EXTPROC/TIOCPKT — supported (7.0 & 6.18).** Packet-mode observation is the
  primary path; the §7.2 reconciliation poll remains an unconditional backstop
  (kept live regardless — do not delete it because a probe passed).
- **P2 PTY presence — supported.** Drives the slave-priming refinement (§3.2).
- **P3 serial fit — supported on real FTDI.** Custom baud (exact), `TIOCEXCL`,
  modem lines, break, `TIOCGICOUNT` all confirmed. Drives §3.1.
- **P4 by-id resolution — supported.** Canonical `usb:vid:pid:serial:iface` via a
  dependency-free sysfs *ancestor* walk (nearest `bInterfaceNumber` = interface;
  first `idVendor` = device — stop there or you bind the root hub). This is the
  reusable core of the phase-7 resolver.

---

## 5. How to build, test, run

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
bash scripts/validate/all.sh --through 2      # every phase gate so far

# Capability report on this machine (attach to any bug report):
cargo run -p nexus-doctor                      # Markdown
cargo run -p nexus-doctor -- --port /dev/ttyUSB0   # include P3 on a real port

# Drive the daemon by hand (use a SHORT socket dir — see §7):
export XDG_RUNTIME_DIR=$(mktemp -d /tmp/snx.XXXXXX)
./target/debug/serialnexusd &                  # or --config demo.toml, --socket PATH
./target/debug/serialnexusctl load demo.toml
./target/debug/serialnexusctl --json state
./target/debug/serialnexusctl dump
./target/debug/serialnexusctl shutdown
```

A minimal `demo.toml` (serial→PTY fan of one):
```toml
[[node]]
type = "pty"
name = "console"
path = "/tmp/snx/console"
[[node]]
type = "serial"
name = "usb0"
device = "/dev/ttyUSB0"     # or a `nexus-sim pty --echo --link` path
[[edge]]
a = "usb0"
b = "console"
```

---

## 6. Phase 2 slice 2 (the data plane) — DONE

Real bytes flow serial↔PTY through a configured daemon over RPC. As built:

- **Readiness (§3.10):** poll-based, *not* `AsyncFd`. Each boundary fd is set
  `O_NONBLOCK`; a task drains via `sys::poll_ready` + `sys::read_fd`/`write_fd`,
  sleeping `runtime::IDLE_POLL` (5 ms) only when idle.
- **Hostward (serial→PTY):** serial read → `try_send` into each attached PTY's
  bounded channel (drop-on-full = lossy at the boundary, §5); the PTY writer
  drains to the master, **presence-gated** (discard when no client).
- **Targetward (PTY→serial):** PTY read → `send().await` into the serial's bounded
  channel (lossless + backpressure: a full channel pauses the reader; the kernel
  buffers on the client side, §5).
- **Packet mode:** the leading `TIOCPKT` byte is stripped on every master read;
  only `TIOCPKT_DATA` (`sys::TIOCPKT_DATA`) payloads are forwarded. `TIOCPKT_IOCTL`
  (client-termios reconciliation into state) is still `#[allow(dead_code)]` — a
  later phase surfaces client termios; the data plane just drops control packets.
- **Presence:** per-PTY `client_present: Rc<Cell<bool>>` driven by the same 5 ms
  `POLLHUP` poll; reads are gated on presence, and on last close the baseline
  termios is re-asserted (§7.2). Feeds `PtyNode::state_extra`.
- **Wiring:** `runtime::Wiring::build` derives the channels from the validated
  edges; `daemon::load` starts each node via `Node::start` (`spawn_local` on the
  `LocalSet`). Teardown/Drop abort the tasks and close the fds.
- **`nexus-sim`:** `client --report-termios` opens the daemon's PTY and reports
  its termios *without* disturbing it (verifies the §7.2 baseline end to end).
- **Validated by `scripts/validate/phase2/data-path.sh`:** 64 KiB seeded echo
  round-trip (checksums intact), both nodes `active`, baseline termios from the
  client's side (raw/echo-off/EXTPROC), and `client_present` true↔false
  transitions. Measured ad-hoc: ~1% idle CPU, 1 MiB echo in ~0.5 s.

## 6a. Phase 3 (boundaries and logging) — DONE

Built in four committed slices: **A** boundary drop counters + serial discard +
`TIOCGICOUNT` (`e064025`); **B** the log node — bounded queue, dedicated blocking
writer thread, on-demand `rotate`, counter recovery by directory scan, ENOSPC
fault; removed the §3.9 log-load rejection (`04b394d`); **C** `subscribe`
(broadcast + periodic snapshot) and client-termios surfacing via the
`TIOCPKT_IOCTL` path (`86ff94c`); **D** the high-throughput data plane (§3.11:
serial reader + PTY writer on dedicated blocking threads) with the firehose,
exact-loss, and throughput/idle benchmark (`c4d0e64`). All validated by
self-judging `scripts/validate/phase3/*.sh`; `docs/benchmarks/phase3.json` records
the throughput (~185 MiB/s) and idle (2% for 32 fds) axes.

## 6b. Phase 4 (arbitration) — IN PROGRESS

Per plan §Phase 4, built in slices; test topology needs no codec (PTYs on one
serial endpoint is a legal §4 fan-out).

- **Slice A DONE: the exclusive write lock.** `nexus_core::lock::EndpointLock` —
  the pure, property-tested state machine (holder, per-origin write modes, purge
  accounting; `may_write` is the gate). Serial node gains an `arbitration` config
  attribute (§6). `Wiring::build` creates one `Rc<RefCell<EndpointLock>>` per
  host-facing endpoint and registers every edge as an origin (a log/`never` edge is
  a non-writer). Each writing PTY's read task gates its targetward drain on
  `may_write` — a non-holder is **not read from** (backpressure, no drop, §5/§6).
  `lock`/`unlock` RPC (address the **origin**, §3.12) with `-32003` LOCKED for a
  contended acquire; the host endpoint reports `.lock` (arbitration, holder,
  origins, purge) in `state`. CLI `lock`/`unlock`. Validated by
  `phase4/exclusivity.sh` (byte-exact: only the holder's stream reaches the sink;
  a locked-out present writer and a `write=never` spy leak nothing — verified with
  a negative control that a disabled gate makes the test fail).
  - **Consequence — exclusive is the default (§6), so a lone PTY needs a lock to
    write.** `only the holder's bytes are read targetward` holds even with one
    origin: an on-demand PTY that has not acquired the lock is not read from. This
    (correctly) broke five pre-arbitration phase-2/3 tests that wrote targetward
    (`--expect echo`) or changed termios without locking. They now set
    `arbitration = "free-for-all"` on their serial node — §6's documented opt-out
    — to keep testing the data plane / logging / termios (their actual subject)
    without arbitration ceremony; the exclusive-lock path is covered by
    `phase4/exclusivity.sh`. Real single-console operators have the same choice:
    `free-for-all`, or the "grab, write, release" flow.
- **Slice B DONE: purge + detach-release + free-for-all e2e.** The PTY reader
  (`nodes/pty.rs::read_and_poll`) was restructured: it now drains available data
  for any `may_write` writer **regardless of a simultaneous `POLLHUP`** (so a
  closing writer's residual is forwarded, not lost), and the present→absent
  transition is handled once, post-drain, by `handle_last_close` — the holder
  releases (detach-release), an **exclusive non-holder's** buffered backlog is
  drained+counted (purge-on-detach), and a free-for-all writer keeps its bytes.
  Purge-on-acquire fires on the `may_write` false→true transition (exclusive only),
  draining+counting the pre-grant backlog via `drain_and_discard`. Purge counters
  surface in `state` as `.lock.origins[].purged`. Two subtle bugs were caught in
  build-out and fixed: a closing free-for-all writer's residual was purged instead
  of forwarded (fixed by the drain-regardless-of-POLLHUP restructure + purge-only-
  exclusive-non-holder); and a lingering `TIOCPKT_IOCTL` packet re-populated
  `client_termios` after last-close cleared it (fixed by gating the termios
  reconcile on `now`). Validated by `phase4/purge.sh` (purge-on-detach and
  purge-on-acquire count exactly, device receives nothing) and
  `phase4/free-for-all.sh` (two writers both reach the device — stable over many
  runs after the fix); `exclusivity.sh` now also asserts detach-release.
- **Slice C PENDING:** the atomic `send` verb, `--steal`/`--lease`/`--wait` (async
  dispatch), and lock-change `subscribe` notifications (periodic snapshots already
  surface `.lock`, so changes are visible at 200 ms granularity today).

---

## 7. Environment & operational notes

- **Unix socket path length:** paths are bounded by `SUN_LEN` (~108 bytes). The
  daemon errors clearly on overflow. Real deployments use `/run` or
  `$XDG_RUNTIME_DIR`; **test harnesses must use a short dir** (`mktemp -d
  /tmp/snx.XXXXXX`), not the long scratchpad path.
- **Serial device access:** the daemon runs as its own user and needs r/w on the
  device node. On the dev box `/dev/ttyUSB0` was `root:dialout 660`; a udev rule
  `SUBSYSTEM=="tty", SUBSYSTEMS=="usb", ATTRS{idVendor}=="0403", GROUP="plugdev",
  MODE="0660"` (or dialout membership) grants it. `nexus-doctor`'s env checks
  report `group:*` membership and `access:<dev>`.
- **`Cargo.lock` is committed** (v3 plan §2): this is a binary workspace, and the
  cargo-deny gate is only as strong as the graph it inspects — an uncommitted lock
  would gate a freshly resolved, potentially different graph on every CI run. It was
  removed from `.gitignore` in the v3 realignment.
- **Licensing gate** (`deny.toml`) is proven in CI (rejects `serialport`); keep
  all new deps permissive (MIT/Apache/BSD/ISC/Zlib/Unicode), §13.
- **`nexus-doctor` never gates the daemon:** runtime degradation paths (e.g.
  §7.2's poll) are unconditional, so a wrong probe misleads a developer but never
  the data plane. Keep it that way.
