# serial_nexus — implementation notes & handoff

**As of:** 2026-07-20. **Branch:** `implementation` (off `main`).
**Normative docs:** `docs/05-design-claude-fable-v2.md` (design) and
`docs/06-implementation-plan-claude-fable-v2.md` (plan). v1 docs are in
`docs/historical/`. Section references (§) point at the v2 design.

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
| 2 | Walking skeleton | **slice 1 done** (control plane + node lifecycle); **slice 2 pending** (data plane) |
| 3–8 | Boundaries/log, arbitration, codecs, wire, identity, hardening | not started |

**Quality gates (all green):** `cargo fmt --all --check`, `cargo clippy
--workspace --all-targets -- -D warnings`, `cargo test --workspace` (36 tests),
and `bash scripts/validate/all.sh --through 2`.

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
| `nexus-sim` | test double: `pty`/`client` modes (§3) | phase 1 modes done; `mux`/`envelope`/`wire`/`tcp-proxy` later |
| `nexus-doctor` | shipping capability checker: probes P1–P4 + env checks (§15.17) | done |
| `serialnexusd` | the daemon | control plane + node lifecycle done; data plane pending |
| `serialnexusctl` | the CLI (thin RPC client + `--json`) | `load`/`dump`/`state`/`teardown`/`shutdown` done |

`serialnexusd` modules: `main.rs` (runtime, socket policy, shutdown),
`control.rs` (JSON-RPC over UDS), `daemon.rs` (graph state + method impls),
`nodes/{mod,pty,serial}.rs` (node runtimes), `sys.rs` (the single unsafe-bearing
module: `TIOCEXCL`/`TIOCPKT` ioctls).

Validation scripts are the canonical exit criteria (plan §3):
`scripts/validate/phaseN/*.sh`, each self-judging with a JSON verdict and exit
code. Helpers in `scripts/lib/` (`wait-for.sh`, `semantic-diff.sh`).

---

## 3. Deviations & refinements from the design

These are implementation decisions the design does not spell out, or where a
kernel/library reality shaped the approach. None contradict the design.

### 3.1 Serial node uses blocking `serial2` + `tokio::AsyncFd`, not `serial2-tokio`
**Design:** §13 lists `serial2`/`serial2-tokio` for "concurrent async read/write."
**Reality (nexus-doctor P3 research):** `serial2-tokio` 0.1.24 exposes **no
accessor for the inner fd**, and `serial2` **does not take `TIOCEXCL`** (only
`O_NOCTTY`). The daemon needs the raw fd for `TIOCEXCL` (§7.1) and later
`TIOCGICOUNT` (§5).
**Decision:** open a blocking `serial2::SerialPort` (settings, modem lines,
break, and the raw ioctls via `as_raw_fd`) and drive async I/O by registering
the fd with `tokio::io::unix::AsyncFd` in slice 2 — rather than `serial2-tokio`.
Consistent with §13's "raw termios via nix/rustix as the fallback." `TIOCEXCL` is
issued by the daemon itself (`nodes/serial.rs`).

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
property test (`prop_targetward_no_loss_bounded_interior`). This is a runtime
detail §5 implies but does not name.

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

### 3.9 Unimplemented node kinds are a structural load error (temporary)
A configuration containing a **log** node (phase 3) is rejected at load with
`node <name>: log nodes land in phase 3`, nothing created. This is a
build-stage limitation, not a design position; it disappears when phase 3 lands.

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

## 6. Next up — Phase 2 slice 2 (the data plane)

The only thing between here and a full walking skeleton is real bytes flowing
serial↔PTY. Design settled:

- **Async I/O:** wrap the serial fd and PTY-master fd in `tokio::io::unix::AsyncFd`
  (blocking serial2 + AsyncFd, §3.1); `try_clone`/dup for concurrent read+write
  tasks. Set `O_NONBLOCK` on the PTY master (posix_openpt doesn't).
- **Hostward (serial→PTY):** serial read → `try_send` into each attached PTY's
  bounded channel (drop-on-full = lossy at the boundary, §5); the PTY write task
  drains to the master, **presence-gated** (discard when no client).
- **Targetward (PTY→serial):** PTY read → `send().await` into the serial's bounded
  channel (lossless + backpressure: a full channel pauses the reader; the kernel
  buffers on the client side, §5).
- **Packet mode:** strip the leading `TIOCPKT` control byte on every master read;
  forward only `TIOCPKT_DATA` payloads (else the echo stream corrupts). The
  `TIOCPKT_IOCTL` constant in `sys.rs` is `#[allow(dead_code)]` awaiting this.
- **Presence:** per-PTY `client_present: Rc<Cell<bool>>` driven by a ~100 ms
  zero-timeout `POLLHUP` poll; the readable branch is gated on presence to avoid
  the HUP busy-loop. Feeds `PtyNode::state_extra` (`client_present` is currently
  hard-coded `false`).
- **Wiring:** a `runtime`/`DataPlane` module builds the channels from the loaded
  edges and hands each node its endpoints; called from `daemon::load` after
  instantiation. Nodes gain a `start()` that moves their kernel objects into
  tasks (`spawn_local` on the `LocalSet`).
- **Validation:** `scripts/validate/phase2/data-path.sh` — `nexus-sim client
  --send seeded:64KiB --expect echo` through the daemon (checksums intact), plus
  presence transitions and device-side `--report-termios` (raw/echo-off/EXTPROC).

Integration risk already retired: **serial2 opens a `nexus-sim` pts and sets
baud** (the serial node goes `active`), so the plan's e2e topology works.

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
- **`Cargo.lock`** is gitignored (per the repo's original `.gitignore`); the
  build resolves deps fresh. Reconsider committing it before release for
  reproducible CI + cargo-deny.
- **Licensing gate** (`deny.toml`) is proven in CI (rejects `serialport`); keep
  all new deps permissive (MIT/Apache/BSD/ISC/Zlib/Unicode), §13.
- **`nexus-doctor` never gates the daemon:** runtime degradation paths (e.g.
  §7.2's poll) are unconditional, so a wrong probe misleads a developer but never
  the data plane. Keep it that way.
