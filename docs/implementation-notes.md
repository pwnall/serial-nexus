# serial_nexus — implementation notes & handoff

**As of:** 2026-07-21 (phase 0-5 done; **phase 6 COMPLETE** — the leg node + v1 wire
protocol, all six validation items green, audited against v5 with 17 findings fixed).
**Next: phase 7** (identity & resilience). **Branch:** `implementation` (off `main`).
**Normative docs:** `docs/11-design-claude-fable-v5.md` (design) and
`docs/12-implementation-plan-claude-fable-v5.md` (plan). v1–v4 docs (03–10) are in
`docs/historical/`. Section references (§) point at the v5 design.

**Phase 6 (2026-07-21).** The cross-daemon transport (§7.4/§9): a new **leg node**
(`nodes/leg.rs`) carrying N channels multiplexed over a tcp|unix socket by the
built-in **link codec** (the shared envelope, §8). `codec-api` grew the **v1 wire
hello** (`WIRE_MAGIC` "SNXL", `WIRE_VERSION` distinct from `ENVELOPE_VERSION`, a `u32`
capability bitset with `CAP_LOCK_RELAY` reserved, `Hello`/`encode_hello`/
`try_decode_hello`, `WireError`) — a distinct wire construct, not a fifth event kind,
so the four golden vectors stay frozen. `nexus-core` gained the `NodeConfig::Leg`
variant (+ `Transport`/`LegRole`), the leg `shape()` (N channel endpoints, no default
endpoint), and config-level validation (loopback-only unless `insecure_bind`, empty
channel/list rejection → new `ValidationError::{NonLoopbackBind,EmptyLeg}`). The leg
plugs into the §15.23 endpoint-keyed `Wiring` with **zero `Wiring::build` change** —
purely via `shape()`. `nexus-sim` grew `wire` (hostile-or-conforming peer / §9
conformance driver) and `tcp-proxy` (outage injection) modes, plus `pty --stall`. One
new ADR landed — **§15.24** (the leg node, the hello frame, fragmentation-not-drop,
faulted-and-wait); §7.5/§15.23/§14 were touched for the re-multiplexer scoping. A
multi-agent adversarial audit of the built phase 6 found **17 confirmed issues, all
fixed** — most importantly a **critical §5/§9 targetward-no-drop violation** (the leg's
write half `continue`d on an oversize-frame encode error, silently dropping any chunk
whose framed size exceeded `MAX_FRAME_SIZE` — reachable because `READ_BUF ==
MAX_FRAME_SIZE` and the `send` verb line is uncapped; **fixed** by fragmenting oversize
chunks across consecutive `data` frames, verified with a 100 001-byte `send`
round-trip) and a **stale-status wedge** (a `faces=target` leg whose local producers
closed returned `SourceClosed` and left status `Active`/"connected" forever; **fixed**
by parking the write half so the independent read direction and the wire stay live).
See §6d below.

**Phase 5 (2026-07-21).** The codec runtime (§7.5/§7.6/§8): a new `codecs/reference`
crate (the v1 envelope framing as a `Codec`, with length-guided resync); the
interior **codec node** (`nodes/codec.rs`) and **exec codec node** (`nodes/exec.rs`)
on a **generalized endpoint-keyed data-plane wiring** (interior nodes have N+1
endpoints — the first non-two-layer topology); `nexus-sim` grew `mux`/`envelope`
modes; two new ADRs landed — **§15.22** (exec child protocol: the multiplexed side
is a reserved empty channel; the exec codec is a child-pipe boundary, not a pure §5
interior node) and **§15.23** (endpoint-keyed wiring, length-guided resync,
held-priority reclaim); §3/§7.5/§7.6 were touched. A multi-agent adversarial audit of
the built phase 5 found **14 confirmed issues, all fixed** — most importantly a
**critical exec-pump deadlock** (the single `select!` coupled stdin-write and
stdout-read; under sustained flow the child filled its stdout pipe and blocked on
stdin while the daemon blocked writing stdin — fixed by running the two directions as
*concurrently-polled* futures) and **held-lock re-acquire** (was FIFO, letting a
non-held `--wait` waiter inherit the mux lock; now a `reclaim_held` primitive with
priority over on-demand waiters). See §6c below.

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

**v5 revision + phase 0-4 alignment audit (2026-07-20).** The v5 docs are v4 plus
the slice-C/P5 specification: design §6 gained a "Waiting and fairness" paragraph
(the FIFO waiter queue), lease generation-guarding, and the poll-sampled-presence
blind-spot note; §10 gained a "Waiting verbs" paragraph; §13/§14 gained P5 (doctor
rig certification) and the deferred per-open PTY epoch; and two new ADRs landed —
**§15.20** ("Waiting verbs: the two-lane control plane") and **§15.21** ("The rig
is a fixture, so the doctor certifies it"). A multi-agent adversarial audit of the
**built** phase 0-4 code against v5 found **one genuine deviation, fixed**: a
`waiting`/`faulted` serial node (device absent — a reachable startup state) drained
and silently discarded every targetward chunk (`while rx.recv().await.is_some(){}`),
violating §5's never-drop-targetward invariant. `nodes/serial.rs` now **parks the
targetward receiver unread** (field `parked_targetward`), so the bounded channel
fills and backpressures the origin (commands delayed, never dropped); only the
phase-7 reopen/heal is deferred, not the invariant. Everything else in phases 0-3 +
slice A/B verified faithful to v5.

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
| 4 | Arbitration | **done** — slices A & B (exclusive write lock, `lock`/`unlock`, `may_write` gate, purge-on-acquire/-detach, detach-release, held, free-for-all) plus **slice C**: the FIFO waiter queue + two-lane async dispatch, `send`, `--steal`/`--wait`/`--lease-ms`, lease generation-guard, immediate lock notifications (§3.12, §6b, §15.20) |
| 5 | Codecs | **done** — codec runtime + registry (§8), the `codecs/reference` framing codec (resync), the interior codec node + exec codec (§7.5/§7.6), endpoint-keyed wiring, `nexus-sim` `mux`/`envelope`; audited (§6c, §15.22, §15.23) |
| 6 | The wire | **done** — leg node (§7.4) + v1 wire hello (§9), fragmentation, binding, faulted-and-wait/purge-on-reconnect, `nexus-sim` `wire`/`tcp-proxy`, §9 conformance scripts; audited (§6d, §15.24) |
| 7–8 | Identity, hardening | not started |

**Quality gates (all green):** `cargo fmt --all --check`, `cargo clippy
--workspace --all-targets --locked -- -D warnings`, `cargo test --workspace` (76
pass), and `bash scripts/validate/all.sh --through 6` (**32 pass, 0 fail**). Phase 6
scripts: `phase6/{reference,binding,hostility,insecure-bind,outage,head-of-line}.sh`;
phase 5 scripts: `phase5/{envelope,demux,resync,held,bad-attributes,exec-crash}.sh`;
phase 4 scripts: `phase4/{exclusivity,purge,free-for-all,held,send,steal-lease,waiting}.sh`;
phase 3 added `counters.sh`, `log.sh`, `log-enospc.sh`, `subscribe.sh`,
`firehose.sh`, `exact-loss.sh`, `benchmark.sh`.

**Kernel matrix:** every kernel-behavior probe is `supported` on **Linux 7.0.0**
(dev box, Ubuntu 26.04) and **Linux 6.18.14** (Debian rodete) with **zero
deltas** — see `docs/nexus-doctor.md`. The kernel-sensitive PTY/serial mechanics
are de-risked across the support matrix.

---

## 2. Where the code lives

| Crate | Role | State |
|-------|------|-------|
| `codec-api` | codec trait (+ `resync_count`), event vocabulary, envelope frame codec + golden vectors, **v1 wire hello** (`WIRE_MAGIC`/`WIRE_VERSION`/`Hello`/`WireError`) (§8/§9) | done |
| `codecs/reference` (`codec-reference`) | the v1 envelope framing as a `Codec`, with length-guided resync (§7.5/§9) | done (phase 5) |
| `nexus-core` | graph model + validator (§4: 3 rules + name/duplicate + leg loopback/empty checks), data-plane deliver contracts + holdover (§5), lock state machine incl. `reclaim_held` (§6), config/state split incl. `NodeConfig::Leg` (§15.8) | done |
| `nexus-rpc` | JSON-RPC 2.0 wire types — the stable §15.16 surface | done |
| `nexus-sim` | test double: `pty`/`client`/`mux`/`envelope`/`wire`/`tcp-proxy` modes (§3) | done through phase 6 |
| `nexus-doctor` | shipping capability checker: probes P1–P4 + env checks (§15.17) | done |
| `serialnexusd` | the daemon | control plane + node lifecycle + data plane + codecs + leg/wire done |
| `serialnexusctl` | the CLI (thin RPC client + `--json`) | `load`/`dump`/`state`/`subscribe`/`rotate`/`lock`/`unlock`/`send`/`teardown`/`shutdown` |

`serialnexusd` modules: `main.rs` (runtime, socket policy, shutdown),
`control.rs` (JSON-RPC over UDS), `daemon.rs` (graph state + method impls),
`runtime.rs` (endpoint-keyed data-plane `Wiring` + `LockCell` + poll-based I/O helpers),
`nodes/{mod,serial,pty,log,codec,exec,leg}.rs` (node runtimes; `codec` = the in-process
demux/remux + registry, `exec` = the child-process codec, `leg` = the cross-daemon
socket transport + link codec, §15.24), `sys.rs` (the single unsafe-bearing module:
`TIOCEXCL`/`TIOCPKT` ioctls, raw `read`/`write`/`fcntl`, and the non-blocking
`poll_ready`).

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

## 6b. Phase 4 (arbitration) — COMPLETE

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
  Purge-on-acquire runs **synchronously in `daemon::lock`** at grant time
  (exclusive only), draining+counting the pre-grant backlog via `Node::purge_origin`
  *before the grant reply returns*; a held holder keeps the lock across a client
  detach. Purge counters surface in `state` as `.lock.origins[].purged`. Bugs caught
  & fixed during build-out and an adversarial multi-agent review (details in §3.12):
  a closing free-for-all writer's residual was purged not forwarded; a lingering
  `TIOCPKT_IOCTL` re-populated `client_termios` after last-close; the purge-on-acquire
  drain was initially in the async reader and raced a correct acquire-then-write
  client (moved to the daemon); a held holder was wrongly detach-released. Validated
  by `phase4/{purge,free-for-all,held}.sh` (purge counts exact, post-grant survives,
  two free-for-all writers both reach the device, held keeps its lock);
  `exclusivity.sh` now also asserts detach-release.

- **Slice C DONE: waiting verbs + the two-lane control plane (§6, §10, §15.20).**
  - **Pure lock (`nexus_core::lock`):** `EndpointLock` gained a FIFO `waiters`
    queue, a grant `generation`, `steal`, and `renew`. `acquire` is now queue-aware
    — it grants a free lock **only to the FIFO head** (barge prevention), naming an
    earlier waiter in `Denied { held_by }`. New pure API: `enqueue`/`dequeue`/`steal`
    /`renew`/`generation`/`waiters`; `snapshot()` now carries `waiters` and
    `last_steal`. 14 unit/property tests (the invariant proptest gained enqueue/
    dequeue/steal ops, generation-monotonicity, and holder-never-queued).
  - **`LockCell` (`runtime.rs`):** `SharedLock` is now `Rc<LockCell>` =
    `RefCell<EndpointLock>` + a `tokio::sync::Notify` (wakes queued waiters) + the
    `subscribe` broadcast sender + a `closed` flag. `wake_waiters`/`notified`/
    `emit_change`/`close`/`is_closed`. `Wiring::build` takes the notifier and creates
    **one targetward channel per host endpoint up front** (`endpoint_targetward`), so
    `send` works even with no PTY writer attached.
  - **Two-lane dispatch (`daemon.rs`, §15.20):** `Daemon::dispatch` is now `async`;
    `lock`/`send` are async, `unlock` stays sync. `wait_for_grant` is the waiting
    lane — it enables the `Notify` future **before** the acquire check (lost-wakeup-
    free), enqueues on `Denied`, and suspends on `notified`/deadline holding no
    borrow. The **`RefCell` borrow never crosses an `.await`** tripwire holds
    throughout (every borrow is a `{}` block dropped before the await; purge drains
    the fd synchronously). `WaiterGuard`/`TransientOrigin` are `Drop` guards that
    dequeue/unregister on cancellation. Immediate id-less `lock` notifications fire
    on every transition (acquire/release/steal/lease-expiry/detach-release) via
    `LockCell::emit_change`; the 200 ms snapshot is only an observability floor.
  - **`send <endpoint> --line`:** names the **endpoint**; the CLI is a transient
    origin (synthetic id from `SEND_ORIGIN_BASE = 1<<40`). register → acquire-with-
    timeout (default 2000 ms, `--timeout-ms`) or `--steal` → write `line + "\n"`
    targetward → release + unregister. Always cleaned up (guard) on timeout or a
    dropped connection.
  - **`control.rs` cancel-on-disconnect:** `serve_connection` races the (maybe-
    waiting) dispatch future against a second `lines.next_line()` in a
    `tokio::select! { biased; … }` — `biased` so a ready fast verb is never pre-
    empted by a spuriously-read next request, and a dropped connection cancels a
    `--wait` (dropping the dispatch future runs the guard).
  - **`nodes/pty.rs`:** `handle_last_close` now `wake_waiters()` + `emit_change()`
    after a detach-release / purge-on-detach, so a queued `--wait` waiter is granted
    on the detach-release path.
  - **Three bugs caught by the slice-C adversarial review & fixed** (regression-
    covered): (1) **lease re-arm** — a second `lock --lease-ms` hit `AlreadyHeld`
    without advancing the generation, so the *earlier* (shorter) timer still fired
    and released the grant; now `renew` bumps the generation on re-arm, invalidating
    the prior timer (`steal-lease.sh` check 4). (2) **teardown stranded parked
    waiters** — a deadline-less `lock --wait` hung forever when its endpoint was
    torn down; `teardown` now `close()`s every lock cell (which wakes waiters) and
    `wait_for_grant` returns a defined `Closed` error (`waiting.sh` sub-check D).
    (3) **steal didn't wake a same-origin `--wait`** — a `lock X --steal` from one
    connection left a `lock X --wait` on another parked; both steal paths now
    `wake_waiters()`.
  - **Validated:** `phase4/{send,steal-lease,waiting}.sh` (plan items 5, 4, 7):
    send LOCKED-then-steal byte-exact; steal record + immediate notification; lease
    auto-release, stale-timer-never-fires, and renewal-extends; FIFO across an unlock
    **and** a detach-release with byte-exact purge-on-acquire on the queued grant,
    kill-waiter-dequeues, deadline-send-queue-intact, teardown-wakes-waiter.

## 6c. Phase 5 (codecs) — COMPLETE

The interior codec node — the first node with more than one endpoint and the first
non-two-layer topology. Built in three slices, then an adversarial audit fixed 14
findings.

- **Slice A — pure contracts + reference codec + sim modes.** `nexus-core` gained the
  `NodeConfig::Codec` variant (codec name, `faces`, channel list, opaque `attributes`
  table; multiplexed side = the default/empty endpoint, channels = identities) and the
  shape/validation; `Eq` was dropped from `GraphConfig`/`NodeConfig` (a `toml::Table`
  carries floats — only `PartialEq`; nothing needed `Eq`). New crate
  **`codecs/reference`** (`codec-reference`): the v1 envelope framing as a `Codec`,
  with **length-guided resync** — on a body-decode error with an intact length prefix,
  skip exactly `4 + body_len` and count one framing error; only a mangled length prefix
  is unrecoverable, and the reliable-transport link codec (phase 6) never hits it, so
  §8's one shared frame format holds. `nexus-sim` grew **`mux`** (round-robin
  seeded per-channel data → reference frames, `--corrupt-every`, a deterministic
  `--manifest` oracle, and a `--wait-file` feed gate so presence-gated channel PTYs
  don't miss the burst) and **`envelope`** (drives an external codec child through the
  golden-vector battery). Fixture: `tests/ext-codec/passthrough.py`. Validated by
  `phase5/envelope.sh` (item 3). The graph validator gained `DuplicateEndpoint`
  (empty/duplicate channel identity) — a slice-A adversarial review found the codec was
  the first node that could hit it.
- **Slice B — endpoint-keyed wiring + codec node (demux/resync/held).** `Wiring` was
  generalized from node-keyed (serial→consumer) to **endpoint-keyed** (`EndpointAddr`):
  every host-facing endpoint gets a lock + fan-out + one arbitrated targetward channel;
  every target-facing endpoint is a single-producer consumer that may write back. Only
  the `Node::start` dispatcher and `Wiring` changed — serial/pty/log `start` signatures
  are untouched. The daemon converts the endpoint-keyed maps to display-string keys for
  the RPC surface (`usb0`, `mux/console`) and reports each host endpoint's lock as
  `.lock` (serial) or `.channels[ch].lock` (codec). `nodes/codec.rs`: a hostward demux
  task (raw → per-channel `data` → fan-out) and one targetward mux task per channel
  (frame → serial, gated on the codec holding the serial lock). The demux edge holds
  the serial lock (`held`); a steal ousts it, and the channel task **reclaims with
  priority** once the stealer releases. Registry `build_codec` (match-on-name behind a
  `codec-reference` Cargo feature); a bad codec name / attribute schema is structural
  (aborts the load, nothing created). Validated by `phase5/{demux,resync,held,
  bad-attributes}.sh` (items 1, 2, 6, 5). **Remux (`faces = host`) is deferred to
  phase 6** — it needs a leg to drive; such a node loads and comes up faulted.
- **Slice C — exec codec.** `nodes/exec.rs`: a child process speaking the envelope on
  stdin/stdout, the multiplexed side on the **reserved empty channel** (ADR §15.22). A
  supervisor spawns the child, pumps both directions, and restarts with backoff on
  crash (restart count is observable); stderr → tracing. Validated by
  `phase5/exec-crash.sh` (item 4): a 256 KiB echo round-trip through the codec, `kill
  -9`, restart, clean resume, with an unrelated serial echo healthy throughout.
- **⚠️ Audit fixes (14 confirmed; do NOT regress).** (1) **CRITICAL exec-pump
  deadlock** — the single `select!` coupled stdin-write and stdout-read; under
  sustained flow (>64 KB) the child filled stdout and blocked on stdin while the daemon
  blocked writing stdin. **Fixed:** `pump_child` runs stdin-feeding and stdout-reading
  (and stderr) as **concurrently-polled** futures in one `select!`, so a blocked
  `write_all` never starves the stdout reader. The 256 KiB round-trip in `exec-crash.sh`
  is the regression guard — do NOT collapse the two directions back into one branch.
  (2) **Held re-acquire was FIFO** — a non-held `--wait` waiter could inherit the mux
  lock and corrupt framing. **Fixed:** `EndpointLock::reclaim_held` grants a held origin
  the free lock ahead of on-demand waiters (§6 "held indefinitely"); `ensure_holds`
  uses it. (3) **Duplicate node names** silently collapsed in the shape map →
  `ValidationError::DuplicateNodeName` + `GraphConfig::validate()` (checks the node
  *list* before the model's HashMap collapses it; `load` calls it). (4) Mux-side
  hostward drop counter now surfaced as `.multiplexed.dropped_slow_consumer` (§5 loss
  attribution). (5) A configured-but-unattached channel discards-with-count
  (`discarded_unattached`) instead of over-counting `delivered_hostward`. Plus the
  exec teardown-vs-crash discriminator is now an explicit `PumpEnd` outcome (not a
  `src_rx.is_closed()` heuristic), the stderr reader is a pump future (no leaked task),
  and doc corrections (§3 default endpoint, §15.22/§15.23, `daemon.rs`/`codec.rs`
  docstrings). Two audit findings were **rejected** on verification (an oversize-mux
  drop that can't be constructed since `MAX_FRAME_SIZE == READ_BUF`, and a
  resync-as-link-codec worry that doesn't apply — the link codec never resyncs).
  **Note:** the phase-6 audit re-examined the first rejection and found the oversize
  drop *is* reachable for a non-codec-bounded producer (the leg's `send` verb, and
  the exec node's raw device stream) — see §6d; both are now fixed by fragmentation.

## 6d. Phase 6 (the wire / leg node) — COMPLETE

The cross-daemon transport (§7.4/§9/§15.24). Built as one coherent slice (config +
wire contracts, then the leg node, then the six validation scripts), then an
adversarial audit fixed 17 findings.

- **Wire contracts (`codec-api`).** The v1 **hello** frame: `WIRE_MAGIC` (`0x534E584C`
  "SNXL"), `WIRE_VERSION = 1` (versioned independently of `ENVELOPE_VERSION`), a `u32`
  capability bitset (`CAP_LOCK_RELAY = 1<<0` reserved, negotiated none in v1),
  `Hello{version,capabilities,channels}`, `encode_hello`/`try_decode_hello`,
  `WireError`. A distinct wire construct (not a fifth `EventKind`), so the four golden
  vectors stay byte-frozen; it reuses the envelope's `u32` length prefix, and its body
  begins with the magic so it never collides with a data frame. `try_decode_hello`
  validates the version-stable magic+version prefix *before* the v1 12-byte header, so
  a version mismatch is always refused as such (audit fix).
- **Config (`nexus-core`).** `NodeConfig::Leg` (+ `Transport`/`LegRole`); `shape()`
  emits one endpoint per channel, all facing `faces`, **no default endpoint** (the
  socket is off-graph); host-facing channels carry the leg's arbitration.
  `GraphConfig::validate` gained the loopback-only check (tcp non-loopback needs
  `insecure_bind`; unix exempt), empty-channel-identity and empty-channel-*list*
  rejection → `ValidationError::{NonLoopbackBind, EmptyLeg}` (+ the existing
  `DuplicateEndpoint` for empty identities). `is_loopback_addr` handles `host:port`,
  bracketed/ bare IPv6, `localhost`, and wildcard binds. The leg plugs into the
  §15.23 endpoint-keyed `Wiring` with **zero `Wiring::build` change** — via `shape()`.
- **The leg node (`nodes/leg.rs`).** A supervisor task (mirroring the exec supervisor)
  does connect-with-backoff / listen-accept-one, the hello handshake (both send then
  read, under one overall deadline), binding, and per-connection pump. The pump runs
  the socket **read and write halves concurrently** (the §15.22 lesson). `faces=target`
  (sender): drains the local hostward stream onto the wire and writes wire-arriving
  targetward as an **on-demand origin** (implicit acquire; release on idle *or*
  disconnect via a shared `Notify`; never `held`, exempt from purge-on-acquire).
  `faces=host` (receiver): fans wire data hostward (lossy `try_send`+counters) and
  drains the arbitrated targetward stream onto the wire. **The link codec fragments,
  never drops** an oversize chunk. Binding: `bound`/`waiting`/`unbound` are
  leg-internal state; a `waiting` channel's targetward writers backpressure (not sent
  to be dropped at the peer). Outage = faulted-and-wait: reconnect backoff, listen
  reject-extras, park the receivers, purge-on-reconnect (faces=host targetward
  backlog), and park the SEND half — not tear down — when local producers close.
- **`nexus-sim`.** `wire` (hostile-or-conforming peer: crafted `--hello-version`,
  `--bad-magic`, `--oversize-frame`, `--unknown-type`, `--echo`, `--send`, `--stall`)
  and `tcp-proxy` (`--drop-after`/`--restore-after` outage injection) modes; `pty
  --stall`.
- **Validated:** `phase6/{reference,binding,hostility,insecure-bind,outage,
  head-of-line}.sh` (plan items 1–6): two-daemon reference topology (per-channel
  bidirectional checksums), bound/waiting/unbound, the §9 clean-refusal battery +
  heal, the loopback gate + insecure marker, tcp-proxy outage + purge-on-reconnect,
  and the whole-connection head-of-line property (targetward freezes together,
  hostward advances).
- **⚠️ Audit fixes (17 confirmed; do NOT regress).** (1) **CRITICAL §5/§9
  targetward-no-drop violation** — the write half `continue`d on an oversize-frame
  encode error, silently dropping (uncounted) any chunk whose framed size exceeded
  `MAX_FRAME_SIZE`; reachable via the uncapped `send` verb and codec-emitted chunks
  (`READ_BUF == MAX_FRAME_SIZE`). **Fixed** by fragmenting oversize chunks across
  consecutive `data` frames in `leg.rs` (and the same idiom in `exec.rs`'s stdin feed
  for the raw device stream); verified with a 100 001-byte `send` round-trip
  (byte-exact, `discarded_hostward == 0`). Do NOT reinstate the `continue`-on-encode-
  error drop. (2) **Stale-status wedge** — a `faces=target` leg whose local producers
  all closed returned `SourceClosed` and left status `Active` forever, killing the
  independent targetward direction; **fixed** by parking the write half (removed
  `PumpEnd::SourceClosed`) so the wire/read half stay live. (3) On-demand lock
  **released on peer disconnect** now, not only after idle (a `Notify` the supervisor
  pulses). (4) Handshake bounded by **one overall deadline** (a trickling peer no
  longer wedges a listen leg). (5) `waiting`-channel targetward is **gated (not
  muxed-then-dropped-at-peer)** — `next_send` skips unbound channels so their writers
  backpressure. Plus: `insecure_bind` surfaced in `state`; configured-but-unattached
  channel drops counted (`discarded_hostward`); empty-channel-list rejected; the
  hello magic/version-first decode order; and test-fidelity fixes (head-of-line
  positive lower bound + honest comment; sim wire hello honors `--timeout-ms`). No
  findings were rejected.

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
- **Known pre-existing flake — `phase5/demux.sh` (~1 in 5 under load).** A timing
  race in the *test*, not the daemon: the mux feed is released (`--wait-file` GO)
  once every channel client reports `client_present==true`, but a client's slave
  can be open (present) a beat before its read loop is draining, so under machine
  load the initial burst can outrun the consumer and the presence-gated PTY drops
  it, failing the byte-exact manifest check. Untouched by phase 6 (the demux path
  is unchanged). A robust fix would gate GO on the clients actually reading (a
  first-byte handshake), not just presence — a phase-5 test-fidelity follow-up.
