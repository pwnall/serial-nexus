# serial_nexus — Implementation Plan

**Status:** Executed — phases 0–8, the simplification track (§9), the extension track (§10), and web-console items §11.1–§11.6 are complete, audited, and green; macOS is runtime-verified on real hardware and the validation suite is the Rust `nexus-itest` crate (§15.30, §15.31). Open work: §11.7–§11.9 (default rings, tap offsets, browser history) and §16.11.
**Companion:** `serial_nexus-design.md` — section references (§) below point there. The design is normative; where implementation reality disagrees with it, the design gets a new §15 entry before the code diverges.
**Shape:** Nine phases (0–8), each with a goal, scope traced to design sections, key tasks, testable exit criteria, and an agent-validation block of concrete commands with expected outcomes. Sizes are relative (S/M/L) because calendar mapping depends on availability, not on the work.

## 1. Approach

Five principles order everything below.

**Retire risk before writing architecture.** The design flags exactly one mechanism for early verification (§15.14: EXTPROC + TIOCPKT), and several others rest on kernel behaviors worth confirming on real systems (PTY presence detection, serial2's exclusivity and unplug semantics, by-id resolution on awkward adapters). serial_nexus must hold on a matrix of systems, so phase 0 answers these with `nexus-doctor` — one consolidated capability checker run per system, not spike binaries run one by one (§15.17) — and anything its report contradicts in the design amends the design first.

**Walking skeleton, then muscles.** Phase 2 produces the thinnest end-to-end system — config file in, daemon up, real bytes flowing device↔PTY, CLI talking JSON-RPC — before any feature depth. Every later phase extends a working system, so integration risk is paid once, early.

**Tests pin to the RPC surface, never to CLI output.** Per §15.16 the CLI shape will churn on human and agent feedback; integration tests therefore drive `serialnexusd` over JSON-RPC directly (or via `serialnexusctl --json`, which is a pass-through). CLI iteration stays free.

**Every check is a command whose exit code is the verdict.** This plan will be executed with an AI coding agent in the loop, so validation cannot live in prose. Each phase's exit criteria map one-to-one to scripts under `scripts/validate/phaseN/`; each script is a single command, idempotent in a temp directory, emitting machine-readable JSON on stdout, diagnostics on stderr, and exiting 0 only on pass. An agent finishes a task, runs the phase's scripts, and either proceeds or has a concrete failure to fix. Data-integrity assertions use seeded pseudo-random streams and checksums, so "no bytes lost, duplicated, or reordered" is a single comparison rather than a judgment call.

**Test doubles are in-workspace and permissive.** No mainstream permissively-licensed tool covers socat's PTY-plumbing role (§3), and external plumbing can't emit verdicts anyway. The workspace therefore ships `nexus-sim`, a purpose-built test double using the same permissive PTY and socket calls as the daemon — and validating with it exercises those calls twice.

## 2. Workspace and toolchain

**Crates.** One Cargo workspace, kept deliberately small until a second consumer forces a split:

- `codec-api` — the codec trait, per-channel event vocabulary, and envelope frame types (§8). Depends on nothing project-internal; codecs and the daemon depend on it. This crate carries the envelope's public versioning promise (§15.15).
- `nexus-core` — graph model (endpoints, facing, the three validation rules), the data-plane contracts (hostward infallible deliver, targetward Accepted/Busy, holdover slot), node trait, configuration/state type split, resolver types. Pure logic; no kernel calls, fully testable in-memory.
- `nexus-rpc` — the JSON-RPC request/response/notification serde types shared by daemon and CLI. This is the stable surface of §15.16, in crate form.
- `nexus-sim` — the test-double binary (§3). Ships with the repository, never with releases.
- `serialnexusd` — the daemon binary: boundary node implementations (serial, PTY, log, leg, exec host, existing-terminal), the data-plane runtime, control plane, state file. Node implementations live here until something else needs them; premature crate splits are churn.
- `codecs/*` — one crate per codec (§8), behind Cargo features, registered in one explicit match in the daemon. Phase 5 adds the first two: the reference framing codec and the exec codec.
- `serialnexusctl` — the CLI binary: an RPC client plus a rendering layer, nothing else. `--json` (raw result pass-through) exists from its first commit, which also makes it agent-usable immediately.
- `nexus-itest` — the integration harness (§15.31): the former 58-script bash suite as one workspace test crate — in-Rust RPC client, subprocess sim doubles, SHA-256 assertions, and `serial_rig()`/`crossover_ports()` providers with visible self-skip; CI is `cargo test --workspace` on both platforms. Three shell scripts survive as external-tool wrappers pending §16.11.
- `nexus-doctor` — the capability checker (§3, design §15.17): every kernel-behavior probe the design depends on plus environment checks, one binary, one copy-pasteable Markdown report (`--json` twin for CI). Ships with releases, because it is the support tool — a user on an unlisted system runs it and attaches the output to a bug report.

**Concurrency architecture (per design §5, §15.18, §15.19 — settled by the phase 3 benchmark).** Control, coordination, and low-rate data paths run on a current-thread tokio runtime — the synchronous deliver contract needs no locks there, and mutation serialization falls out for free — with tty-family readiness via non-blocking `poll(2)` under adaptive idle backoff; `AsyncFd` remains prohibited for pty masters (design §15.18). High-rate hostward paths — each serial port's reader and each PTY master's writer — run on dedicated blocking threads parked in blocking `poll(2)`: zero cost while parked, kernel-instant wake, roughly 185 MiB/s measured against the ~1 MB/s tokio-timer-floor ceiling of the all-async version (design §15.19). Cross-thread counters are atomics; file writers get dedicated blocking writer threads (§5's regular-file rule); socket boundaries use tokio's native socket types. Control verbs that must wait run on the design-§15.20 two-lane model — every state transition is a synchronous critical section, and waiting verbs suspend between transitions holding nothing — with the companion review tripwire to the `AsyncFd` rule: a `RefCell` borrow never crosses an `.await`. The data-plane wiring is keyed by endpoint address, not by node: every host-facing endpoint owns its lock, fan-out, and arbitrated targetward channel, so a new node shape plugs in through `shape()` alone — the leg node landed with zero wiring changes (design §15.23).

**Licensing enforcement is CI, not vigilance.** `cargo-deny` runs on every push with the permissive allowlist (MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib, Unicode-DFS) and an explicit ban list naming the known landmines from §13 — `serialport`, `mio-serial`, `tokio-serial`, and any libudev binding — so they cannot re-enter transitively. Phase 0 proves the gate by adding `serialport` on a scratch branch and watching CI fail. `Cargo.lock` is committed: this is a binary workspace, and the deny gate is only as strong as the graph it inspects — an uncommitted lockfile means every CI run gates a freshly resolved, potentially different dependency graph.

**Rust hygiene.** Edition 2024; MSRV pinned at **1.97** in CI (matching the deployment infrastructure; raised deliberately, never by drift); rustfmt and clippy with warnings denied; `#![forbid(unsafe_code)]` everywhere except a single small `sys` module in `serialnexusd` isolating the raw ioctls that nix/rustix don't wrap (TIOCGICOUNT is the known candidate). `tracing` wired from phase 2 so debugging never starts from printf. Configuration files are TOML; the RPC carries JSON — both through serde, so the attribute-table pattern (§8) is uniform.

## 3. Validation toolkit

**The external-tool question, answered.** socat is the canonical plumbing tool and does exactly the needed trick — generating a PTY whose slave side another process opens like a serial line, with a `link` option for a stable symlink — but it is GPL-2.0, and no mainstream permissive equivalent covers the PTY side (openbsd-netcat, BSD-licensed, covers only sockets). Under the §13 policy socat may still *run* beside the project, so it remains an optional manual cross-check; but no validation script requires it, for two better reasons than license comfort: an external relay cannot judge outcomes, and a purpose-built double can.

**`nexus-sim`: one binary, several doubles.** Every mode is deterministic under `--seed`, prints a single JSON verdict line on exit (`{"tool":"nexus-sim","mode":...,"pass":...,"sent":...,"received":...,"sha256":...}`), and exits 0 only on pass. Modes, introduced as phases need them:

- `pty` — create a PTY pair via the same permissive calls the daemon uses; maintain `--link PATH` (a stable symlink to the pts node, standing in for a device path or a by-id entry); run a behavior: `--echo`, `--source` (seeded generator at `--rate`, `--bytes`), `--sink` (count and checksum), `--script` (expect/send exchanges), `--stall-read-after N` (stop draining, for Busy and head-of-line tests), `--hup-after`/`--reopen-after` (client-presence fault injection), `--report-termios` (observe what the daemon applied to the pair — validates the §7.1 reopen ritual and §7.2 baseline from the far side).
- `client` — open the *daemon's* PTY like an operator would: send seeded data, verify echoes or expected streams, throttle with `--read-rate` (slow-consumer tests), report attach/HUP observations.
- `nullmodem` — two PTY pairs bridged in-process (`--link-a`/`--link-b`): a software crossed pair for CI-testing P5's discovery and classification (whose characterization there correctly reports `skipped(not a UART)`), and for any harness that wants a two-port rig without hardware.
- `mux` — emit and verify reference-framed multichannel streams with per-channel manifests (seed, byte counts, checksums), plus `--corrupt-every N` with a computed expected-loss manifest for resynchronization tests (phase 5).
- `envelope` — drive an external codec process through the golden-vector battery (phase 5).
- `wire` — speak just enough v1 protocol to be a hostile or conforming peer: crafted hellos (`--hello-version`, bad magic), oversize and truncated frames, unknown-channel data. This mode *is* the driver for the §9 conformance suite (phase 6).
- `tcp-proxy` — sit between two daemons with `--drop-after`/`--restore-after` for unprivileged link-outage injection (phase 6).

**The resolver seam.** The resolver takes a root prefix (`--dev-root`, default `/`), making `/dev/serial/by-id` a fixture directory in tests: symlink trees pointing at `nexus-sim` pts nodes reproduce normal adapters, no-serial clones (by-path only), FT4232-style multi-interface devices, and identity squatters — the whole §12 matrix, unprivileged, no hardware (phase 7). This is a documented, first-class test seam, not a hidden hook.

**`nexus-doctor`: every probe, one report.** The design's kernel-behavior assumptions must hold across the support matrix, so they are checked by one consolidated binary rather than per-spike one-offs. The probe set: **P1** EXTPROC/TIOCPKT signaling (does tcsetattr surface as TIOCPKT_IOCTL; does clearing EXTPROC emit a final packet; does re-assertion via the master work), **P2** PTY presence semantics (POLLHUP with zero openers in both histories, absence of an un-HUP event, termios reset through the master, zero-timeout check cost), **P3** serial-port fit (custom baud acceptance, modem-line set/readback, TIOCEXCL exposure, the exact unplug error surface), **P4** by-id resolution ground truth (normal, no-serial, multi-interface, collision behavior), **P5** rig discovery and characterization (opt-in, since it transmits): nonce-based classification of each named port as dangling, loopback, or cross-paired with bidirectional verification, then the rig certificate — rate-ladder integrity including a nonstandard rate, deliberate baud and parity mismatches proving TIOCGICOUNT error observability, break reception, and a modem-line map — plus environment checks: kernel and distro, `/dev` permissions and group membership, by-id tree presence, `$XDG_RUNTIME_DIR`. Output is Markdown on stdout, written to be pasted whole into an issue thread; every probe is self-judging with a verdict line — question, observed behavior, `supported`/`degraded`/`unsupported`/`skipped(reason)`, and the one-line design consequence ("EXTPROC notify absent → §7.2 runs poll-only"). Passive by default: any probe that would transmit on a serial port requires that port to be named on the command line, because a listed port could be wired to live equipment. The daemon never reads doctor output — runtime degradation paths are unconditional — so a wrong probe can mislead a developer but never the data plane.

**Hardware tiers: no target device, ever.** Every hardware-dependent check is designed around USB-serial converters wired to nothing. **Tier 1 — a dangling converter:** enumeration and identity (by-id, multi-interface, no-serial fallback), TIOCEXCL exclusivity against a second opener, termios acceptance including custom bauds, DTR/RTS set-and-readback, unplug error surfaces, replug during write-side traffic, and squatter swaps — the whole §12 and faulted-and-wait matrix on real silicon, with no receiver required. **Tier 2 — one converter, TX jumpered to RX:** a true driver-level data path for seeded round-trips and RX-overrun counters under load; caveat, a single port cannot detect baud *inaccuracy*, since TX and RX share one clock. **Tier 3 — two converters cross-wired as a null modem** (three jumper wires): independently clocked ends, so baud accuracy, parity and framing-error observation via TIOCGICOUNT, break reception, and modem-line signaling all become assertable — and the pair doubles as a physical instance of the design's symmetric configuration, driven end to end through two daemon serial nodes. Tier detection and certification is probe P5: the doctor classifies opted-in ports by nonce (dangling, loopback, or paired — pairs verified in both directions so half-crossed wiring is named, not mysterious), characterizes the rig into the certificate the tiered checklist requires before any serial_nexus code is blamed, and stops there — the doctor certifies the rig, it never drives the daemon through it. Flow-control behavior with floating CTS is driver-dependent, so it is reported, never assumed.

**Assertion conventions** *(historical for the bash era; the normative harness is now `nexus-itest`, §15.31 — structured Rust assertions, byte-exact SHA-256 oracles, bounded `wait_for` helpers instead of sleeps, providers that self-skip visibly when a platform lacks the fixture, and sim doubles kept as subprocesses for cross-process fidelity. The conventions below survive only inside the three remaining tool-wrapper scripts, and the presence-is-not-readiness rule lives on as the sim's `--prime-*`/`--ready-file`/`--wait-file` gating primitives).** State and RPC assertions use `serialnexusctl --json ... | jq -e '<predicate>'` (jq is MIT-licensed; `-e` turns the predicate into the exit code). Time-dependent conditions use `scripts/lib/wait-for.sh '<command>' <timeout>` — bounded polling on state, never bare sleeps. `scripts/lib/semantic-diff.sh` compares two configuration dumps after normalization. Raw-protocol pokes may use openbsd-netcat (`nc -U`) or `serialnexusctl raw`. `scripts/validate/all.sh --through N` runs every script up to phase N; CI runs a lean deterministic unprivileged lane on every push, a privileged lane (tmpfs disk-full variant) where runners allow it, and the nightly lane — which carries the E2E scenarios, the soak, the fuzz targets, and the full deterministic `--through 8` sweep (§9.5): the heavy gates may be too slow for every push, but they are never local-only. Control-socket paths in harnesses are bounded by `SUN_LEN` (about 108 bytes): every script creates its runtime dir with `mktemp -d /tmp/snx.XXXXXX`, never under a long scratch path — the daemon diagnoses the overflow clearly, but a script should never trigger it. And presence is not readiness: a harness that releases a data burst on `client_present` races the client's read loop, because a slave can be open a beat before anyone drains it — feed gates handshake on an actual first byte read back, never on presence alone (the once-flaky `phase5/demux.sh` was exactly this race in the test, not the daemon; its first-byte retrofit — sim `--prime-*`/`--ready-file` handshakes — is done and holds at 0 failures in 35 runs under full CPU saturation).

## 4. Phases

### Phase 0 — Doctor and scaffolding (M)

*Goal:* every kernel-behavior assumption in the design is confirmed or corrected, per supported system, by one tool; the repository enforces its own rules.
*Implements:* preconditions for §7.1, §7.2, §12, §13, §15.14; design §15.17.

Scaffolding: workspace, CI (build, test, fmt, clippy, cargo-deny), the ban-list proof above. Then `nexus-doctor` with probes P1–P4 and the environment checks as specified in §3 — the former spike questions, now permanent, self-judging, and emitted as one report. One plain prototyping task rides along without a binary of its own: newline-delimited JSON-RPC over a UDS with serde, fixing the `nexus-rpc` type shapes.

*Agent validation:*

1. `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace` exits 0.
2. `scripts/validate/phase0/license-gate.sh` injects `serialport = "*"` into a scratch copy of the manifest, asserts `cargo deny check` fails there, and asserts it passes on the clean tree — the gate is proven, not assumed.
3. `nexus-doctor --json | jq -e -f expectations/<platform>.jq` — per-platform expectation files encode what each supported system must report; CI runs the doctor across the matrix (kernel-of-record Linux runners plus the macOS lane) and archives `nexus-doctor --markdown` as a build artifact, so every capability claim in the design has a dated report behind it.
4. On adapter-equipped machines, the tier probes (§3) run against explicitly named ports and land in the same report; `skipped(no adapter)` is a valid CI verdict, a failing probe is not.
5. **A probe contradicting the design is a stop condition:** the agent surfaces the report for a design amendment (§1) rather than coding around it — verdict lines are written to make that diff obvious.

### Phase 1 — Contracts in the small (M)

*Goal:* the design's load-bearing abstractions exist as pure, property-tested code before any kernel object is touched.
*Implements:* §3 (types), §4, §5 (contract, not boundaries), §8 (trait and envelope types).

Build `nexus-core` and `codec-api`: endpoint/facing/edge types, the three-rule validator, configuration and state type families (the split enforced by the type system — state fields do not exist in configuration types), the deliver contracts with mock nodes, the holdover slot, and envelope encode/decode with golden vectors. Land the `nexus-sim` skeleton — `pty` (echo/source/sink/link) and `client` modes plus the verdict plumbing — so the doubles exist before anything they will judge.

*Agent validation:*

1. `cargo test -p nexus-core -p codec-api` passes, with property tests (proptest) covering: hostward broadcast reaches all N attached mocks with per-consumer loss isolation; targetward Busy pauses exactly the offering origin; no interior state exceeds parser-state-plus-one-frame; the validator rejects each illegal shape (host↔host edge, target↔target edge, second edge on a target-facing endpoint, any cycle) with a structural error naming the offender.
2. Round-trip property: arbitrary valid configurations survive TOML→types→TOML unchanged under `semantic-diff.sh`.
3. Envelope golden vectors: `cargo test -p codec-api golden` fails if any vector drifts; regeneration requires the explicit `regen-golden` feature and a written rationale in the commit.
4. `scripts/validate/phase1/sim-selftest.sh`: `nexus-sim pty --echo --link $TMP/dut` piped against `nexus-sim client --path $TMP/dut --send seeded:1MiB --expect echo` passes with matching checksums — the judges are calibrated before they judge.
5. Optional CI floor: `cargo llvm-cov -p nexus-core --fail-under-lines 90`.

### Phase 2 — Walking skeleton (L)

*Goal:* real bytes flow device↔PTY through a configured daemon controlled over RPC.
*Implements:* §7.1 (core behavior, raw-path identity only), §7.2 (creation, baseline termios, symlink, presence-gated output), §10 (socket, hand-rolled JSON-RPC, `load`/`dump`/`state`), §11 (load-on-empty, structural atomicity).

The daemon binary: current-thread runtime, control socket with the §10 path policy and permissions, load-on-empty with full structural validation, serial node on blocking serial2 with poll-based readiness (design §15.18; device given as a raw path for now — the resolver upgrade lands in phase 7 without config-format changes, because identity strings were designed for this), PTY node with the phase-0-verified mechanics, and `serialnexusctl` with `load`, `dump`, `state`, and `--json`. The fake device is `nexus-sim pty --echo --link`, standing exactly where `/dev/ttyUSB0` will.

*Agent validation (`scripts/validate/phase2/e2e.sh` and siblings):*

1. Boot sequence: sim device up, daemon up in a temp `$XDG_RUNTIME_DIR`, `serialnexusctl --json load demo.toml | jq -e '.result'` — structural errors in a deliberately broken sibling config are rejected with the offending node named and nothing created (`state` shows an empty graph).
2. Data path: `nexus-sim client --path $TMP/tty/usb0 --send seeded:64KiB --expect echo` passes — bytes traverse client→daemon→device→daemon→client with checksums intact.
3. Presence: with the client attached, `wait-for` `state | jq -e '..client_present==true'`; after client exit, false within one second; baseline termios confirmed from the device side via sim `--report-termios` (raw, echo off, EXTPROC).
4. Round-trip: `dump` → fresh daemon → `load` → `dump`; `semantic-diff.sh` exits 0.
5. Protocol hygiene: an unknown method over `nc -U` yields JSON-RPC error `-32601` (`jq -e '.error.code==-32601'`); `stat -c %a` on the socket prints `600`; everything ran unprivileged.

### Phase 3 — Boundaries and logging (M)

*Goal:* every §5 policy exists, measured, with counters in state.
*Implements:* §5 (boundary policies, discard-when-unattached, counters), §7.3 entirely.

Drop policies with counters on PTY and (later-reused by leg) socket boundaries; serial discard-when-unattached plus TIOCGICOUNT surfacing; the log node's bounded queue, writer task, on-demand rotation, counter recovery by directory scan, and flush-on-removal; `rotate` verb; `subscribe` notifications for counters and node status; and client-termios surfacing into PTY state through the TIOCPKT_IOCTL reconciliation path (wired in phase 2's data plane, dead code until here).

*Agent validation:*

1. Firehose integrity: sim `--source --rate max --bytes 512MiB --seed 42` through the daemon to a fast sink client — sink checksum equals source checksum; daemon `VmRSS` sampled before/during/after stays within a fixed bound (no interior accumulation).
2. Loss accounting is exact: with the client throttled (`--read-rate 10k`), the PTY boundary's drop counter equals `source_sent − client_received` to the byte, while the log node's file checksum remains complete — loss is located, counted, and isolated.
3. Rotation loses nothing: rotate five times mid-stream; `cat serial.log.00* serial.log | sha256sum` equals the source's reported checksum, and the recovered rotation counter after a daemon restart continues from the directory scan, not from memory.
4. ENOSPC without root: point a log node's file at `/dev/full` — the node faults with an ENOSPC reason while the port and its other consumers keep passing the echo probe; the tmpfs full-disk variant runs in the privileged CI lane.
5. `subscribe` streams the counter and status transitions above (`serialnexusctl --json subscribe` with a timeout, piped to jq predicates), including a client-termios update when a sim client changes settings on the slave; discard-when-unattached counters increase while no client is attached and freeze when one is.
6. The benchmark writes `docs/benchmarks/phase3.json` with two axes: throughput (asserting headroom of at least 10× over 8 ports at 3 Mbaud, making §5's single-thread assumption a recorded fact) and idle cost (total daemon CPU with 32 idle tty fds, asserted under a stated budget). Exceeding the idle budget selects a design-§15.18 escape hatch — adaptive idle backoff or `spawn_blocking` readers — as a phase task, never a return to epoll.

### Phase 4 — Arbitration (M)

*Goal:* the §6 lock machinery, end to end, including its failure etiquette.
*Implements:* §6 entirely; extends §10 with `lock`/`unlock`/`send` and lock-state notifications.

Write modes on edges; the per-endpoint lock as a gate on the pause machinery; explicit acquire/release/steal/lease over RPC on the design-§15.20 two-lane dispatch with its FIFO waiter queue; the atomic `send`; purge-on-acquire and purge-on-detach with counters; `free-for-all` opt-out. The test topology needs no codec: two PTY nodes attached to one serial endpoint is a legal §4 fan-out, with the sim device recording exactly what reaches "hardware."

*Agent validation:*

1. Exclusivity is byte-exact: client A locks and sends seeded-A; client B sends seeded-B without the lock; the sim device's received checksum equals seeded-A exactly.
2. The 3-a.m. regression: after A unlocks, a bounded wait shows the device checksum unchanged (B's buffered bytes did not fire); when B detaches, the purge counter increases by exactly `len(B)`.
3. Purge-on-acquire: B writes before locking, then locks — pre-grant bytes are purged and counted; bytes sent after the grant arrive intact.
4. Steal and lease: `lock --steal` transfers the lock, records the steal in state, and emits an *immediate* `subscribe` notification (event-driven — asserted faster than the snapshot cadence); an expired lease releases a silent holder within the configured bound (`wait-for`); and a stale lease timer never fires across grants — unlock, re-lock, let the old timer elapse, assert the new grant survives.
5. `send` semantics: while A holds, plain `send` fails with the documented locked error; `send --steal` delivers the line exactly once (device-side count).
6. A `write = never` spy receives the complete hostward checksum and its write attempts change nothing (device checksum stable, no lock contention visible in state).
7. Waiting is FIFO and cancel-safe: two `--wait` clients are granted in arrival order across an unlock and a detach-release, each grant running purge-on-acquire first; killing the first waiter mid-wait dequeues it and the second is granted next (`state` shows the queue shrink); and a deadline `send` against a stubborn holder returns the locked error at its deadline with the queue intact.

### Phase 5 — Codecs (L)

*Goal:* the codec runtime, the registry, and both first codecs — one internal, one external-facing.
*Implements:* §7.5, §7.6, §8 (runtime, registry, attribute tables); depends on phase 4 (the demux edge's `held` mode).

Codec node hosting with orientation from `faces`; the explicit match-on-name registry behind Cargo features; attribute tables deserialized and validated by the codec (structural on failure); the reference framing codec (the v1 frame format itself, §9 — doing double duty as the first real codec and the link codec's core); the exec codec with envelope over stdin/stdout, stderr passthrough to tracing, restart-with-backoff, and child lifecycle tied to faulted-and-wait. `nexus-sim` grows `mux` and `envelope` modes.

*Agent validation:*

1. Deterministic demultiplexing: `nexus-sim mux --channels 4 --bytes 8MiB --seed 7` behind a demux node — each channel client's checksum matches the sim's per-channel manifest.
2. Resynchronization is accounted, not approximate: with `--corrupt-every 1000`, the codec's framing-error counter equals the manifest's corruption count and each channel's received bytes equal the manifest's computed expected-loss set — recovery after garbage is provable.
3. Any-language envelope: `nexus-sim envelope --exec "python3 tests/ext-codec/passthrough.py"` passes the full golden-vector battery in CI (Python stdlib only — no GPL, no dependencies).
4. Crash containment: `kill -9` of the exec child mid-stream faults the node, restarts it within the configured backoff (`wait-for` on `restart_count`), resumes clean checksums afterward — and a concurrent echo probe on an unrelated serial node shows no latency spike (the data plane never wedged).
5. Bad attribute tables are structural: a config with a misspelled codec attribute is rejected at load with the codec's own schema error, nothing created.
6. The held lock tells the truth: raw `send` at the serial endpoint fails while the demux holds it; `send --steal` succeeds and, for the theft's duration, every channel client's accepted-bytes counter freezes (§6's stall, observed).

### Phase 6 — The wire (L)

*Goal:* two daemons, one reference topology, over loopback.
*Implements:* §7.4, §9 entirely (contract conformance suite plus the v1 protocol); extends §11's waiting states across machines.

The leg node: listen/connect roles, single-peer policy, loopback-only enforcement with `insecure_bind`, reconnect backoff, faulted-and-wait on outage, purge-on-reconnect; the v1 protocol: hello (magic, version, announcements, capabilities), identity binding into `bound`/`waiting`/`unbound` state, bounded frame size. `nexus-sim` grows `wire` and `tcp-proxy` modes, and the §9 contract becomes an executable conformance suite — six clauses as `wire`-driven cases parameterized over the framing implementation — which is what keeps §15.15's substrate-swap promise honest.

*Agent validation:*

1. The reference topology (§2 of the design), scripted: two daemons in separate temp roots, joined through `nexus-sim tcp-proxy` on loopback; per-channel checksums verify both directions device↔remote-clients end to end.
2. Binding never mutates the graph: an extra configured channel on B reads `waiting`; an extra announced channel from A reads `unbound`; node counts before and after connection are equal (`jq` length equality).
3. Version and hostility handling: `nexus-sim wire --hello-version 999` leaves the leg faulted with the version named in its reason; the conformance battery (bad magic, oversize frame, truncated frame, unknown-channel data, no-drop accounting, announcement round-trip) passes against the v1 framing — and the suite runs against *any* framing by construction.
4. Outage semantics: proxy `--drop-after 1MiB --restore-after 3s` — during the gap, remote writers' accepted counters freeze (paused, not dropped); after restore, `purge_on_reconnect` counters match the manifest of outage-era writes and post-restore checksums are clean.
5. Head-of-line, documented by test: with the device double stalled (`--stall-read-after`), channel 1's *and* channel 2's targetward counters freeze together while hostward checksums keep advancing — the §9 v1 property, pinned so a future per-channel-flow-control substrate visibly changes it.
6. `insecure_bind` gate: a non-loopback address without the flag is a structural load error; with the flag it loads and `state` carries the insecure marker.

### Phase 7 — Identity and resilience (M/L)

*Goal:* the daemon survives the real world: replugs, restarts, crashes, wrong adapters.
*Implements:* §12 entirely, §7.1 faulted-and-wait with the reopen ritual, §11 (state file, `--replace`, cascade), §10 (`teardown`, serial-signal verbs).

The resolver with its fallback chain and add-time echo-back; identity-form versus path-form add semantics; polling-based reappearance detection; reopen ritual (termios, TIOCEXCL, modem lines, purge); the state file written after each successful mutation and preferred at startup; `remove-node --cascade` with log flushing; `send-break`/`set-modem`/`pulse-dtr`; and doctor probe P5 (rig discovery plus certificate), landing here so the 0.1.0 checklist's rigs are certified before first use. All of §12's hardware matrix runs unprivileged against fixture by-id trees over `--dev-root` (§3), with real adapters reserved for the tiered checklist (§5) — which by design needs only dangling converters and jumper wires, never a target device.

*Agent validation:*

1. Unplug keeps clients alive: kill the sim device and remove its fixture symlink — the node reaches `waiting` within the poll interval, and the attached client sim reports its fd still open, no HUP.
2. Replug heals and reapplies: recreate the sim at the same identity — node `active`, purge-on-reconnect counters set, and sim `--report-termios` shows the configured termios reapplied (the reopen ritual, observed from the device side).
3. Squatters are refused: recreate the fixture symlink pointing at a *different* sim identity — the node stays `waiting` and the squatter sim's received-byte count is zero.
4. The §12 matrix on fixtures: an FT4232-style tree yields four independently bound nodes with independent checksum streams; a no-serial clone binds by-path with the documented warning present in the add-time RPC result (`jq -e '.result.warning'`); path-form add with the device absent fails as designed, identity-form add succeeds into `waiting`.
5. Crash recovery is exact: add a log node incrementally, `kill -9` the daemon, restart — `dump` semantic-diffs equal against the pre-kill dump; PTY symlinks are recreated; a fresh client attaches and passes the echo probe (the HUP across daemon death is the documented, unavoidable §7.2 behavior).
6. Signal verbs reach the wire: `pulse-dtr` and `send-break` are observed and reported by the sim device's verdict (master-side observation), and `remove-node --cascade` flushes the log queue — file checksum complete — before the node disappears.
7. P5 without a bench: against a `nexus-sim nullmodem` pair and a sim loopback, discovery classifies dangling, loopback, and paired ports correctly (pairs verified both directions) and characterization reports `skipped(not a UART)`; on adapter-equipped machines the full certificate populates, and `skipped(no adapter)` remains a valid CI verdict while a failing probe is not.

### Phase 8 — Hardening and release (M)

*Goal:* something other people (and agents) can run.
*Implements:* §13 macOS pass, §14 stays deferred, packaging and docs.

macOS: build, `cu.*` resolution interim, PTY observation degrading to poll-only if S1's findings don't transfer; a 24-hour soak of the reference topology under synthetic load; docs — README with the five-minute path, the security page stating "serial consoles are root shells" in §9's words, codec-author guide against the envelope, man-style pages for the RPC verbs; systemd unit and packaging; fuzzing of frame and envelope parsers (cargo-fuzz, optional nightly job) on top of the existing proptest coverage.

*Agent validation:*

1. `scripts/validate/phase8/quickstart.sh` on a clean container reaches a passing end-to-end echo verdict in under five minutes, wall-clocked by the script.
2. The nightly soak asserts hourly: bounded RSS, no counter outside an allowlist growing, zero unexplained faulted nodes; the final per-channel checksums reconcile with the generators' manifests.
3. The macOS lane builds and passes the phase 2 e2e script on the poll-only observation path, with deltas written to `docs/macos.md`.
4. `scripts/validate/phase8/agent-task.sh` performs the full operator scenario purely through `serialnexusctl --json` — inspect state, lock a channel, send a command, verify the device received it, rotate its log, verify continuity, unlock — as the scripted stand-in for §15.16's feedback loop; before `0.2.0`, run the same task with a live coding agent and file its friction notes as CLI-iteration input.

**Release marks.** Tag `0.1.0` at the end of phase 7 (lab-usable on Linux); `0.2.0` at the end of phase 8.

**Continuous track — CLI iteration.** From phase 2 onward, run real tasks through the CLI with humans and agents and collect friction notes; reshape subcommands freely between phases. §15.16 makes this deliberately cheap: tests and agents pin to RPC, so CLI churn costs one crate's diff — and the validation scripts, pinned to `--json` and raw RPC, never notice.

## 5. Testing strategy

The pyramid, mapped to this system:

**Unit and property tests (many, pure, in `nexus-core`/`codec-api`).** The §5 contracts, graph validation, lock state machine, purge accounting, resolver identity parsing, envelope and frame codecs — all with proptest generators for graph shapes, chunk sequences, and interleavings. These encode the design's invariants; a failing property test means the design or the code is wrong, and §1's rule says which document changes.

**Integration tests (some, kernel-facing, Linux CI).** Real PTYs and fixtures via `nexus-sim` and `--dev-root`; a harness that boots `serialnexusd` in a temp `$XDG_RUNTIME_DIR`, drives it over raw JSON-RPC, and asserts on state and counters through jq predicates. The `scripts/validate/phaseN/` scripts are the canonical form of every exit criterion — prose in this plan is commentary on those scripts, not the other way around. Timing-sensitive assertions use `wait-for` bounds and `subscribe` events, never bare sleeps.

**End-to-end scenarios (few, slow, high confidence).** The two-daemon reference topology; the crash-recovery sequence; the soak. Nightly rather than per-push.

**Conformance and compatibility suites (contract tests).** The §9 six-clause conformance suite, driven by `nexus-sim wire` and parameterized over framings; the envelope golden-vector corpus with the in-CI Python codec as the external consumer. These two suites are the executable form of §15.15's decoupling and must never be weakened to make a protocol change pass.

**Tiered hardware checklist (manual, release-gating).** What fixtures cannot fully prove runs on the §3 tiers — never on a target device. Tier 1 (dangling converters, including a no-serial clone and an FT4232): physical replug during write traffic, adapter-swap squatter refusal, exclusivity against a second process. Tier 2 (jumpered converter): seeded round-trips through the real driver and RX-overrun counters under deliberate throttling. Tier 3 (cross-wired pair): baud accuracy across independent clocks, framing and parity error observation, break and modem-line verbs asserted from the far side, and the symmetric null-modem configuration through two daemon serial nodes. The first step of every checklist run is `nexus-doctor --markdown` on that machine, now including the P5 rig certificate, which must be clean before any tier item runs — so a tier failure is attributable to serial_nexus rather than a loose jumper — and the checklist's own negative control is physical: pull one wire, re-run P5, confirm the asymmetry is named. The report is attached to the release notes; the macOS pass rides the same tiers. The scripted end-to-end companion is `scripts/validate/hardware/crossover-rig.sh` — P5-certificate first, then the daemon driven through the physical pair, SKIP-with-exit-0 when no rig is wired — and the checklist carries the doctrine design §16.7 generalizes from the project's one real-hardware bug: any behavior the sim structurally cannot exercise either appears here or is marked *unverified* in the doctor's report, never silently untested. One page, checked at `0.1.0` and `0.2.0` — and each run recalibrates the fixtures, the sim, and the doctor against real adapters (see risks).

**What is deliberately not tested.** Rendering details of `serialnexusctl` beyond `--json` correctness (per §15.16); throughput beyond the documented headroom benchmark (this is a control-and-observation tool at serial rates, not a data mover).

## 6. Risks and mitigations

**EXTPROC behaves differently across kernels or not at all (P1 fails).** The design already contains the fallback — §7.2's reconciliation poll becomes the only mechanism, and only observation latency degrades. Decision point at end of phase 0, recorded as a §15 amendment either way.

**serial2 lacks a needed control (P3 finds a gap).** The `sys` module applies the missing ioctl on serial2's raw fd; the full fallback (rustix termios, §13) exists but is not expected to be needed. Watch item, not a blocker.

**Test doubles or probes drift from real hardware.** `nexus-sim` behaviors and `nexus-doctor` verdict logic are written from phase 0 findings, not assumptions, and every tiered-checklist run re-validates both against real adapters; a divergence is treated like a failing probe — design, doctor, or sim amended before code trusts any of them.

**Single-thread data plane hits a ceiling.** Phase 3's benchmark makes this empirical early and records the headroom in-tree. The §5 contract permits sharding whole subgraphs per thread later without changing node code; nothing in v1 should need it.

**Protocol churn during phases 5–6.** Contained by construction: the conformance suite and the envelope corpus are written before the second framing change is entertained, and §15.15's two-contract split means wire changes cannot break external codecs.

**Scope creep via the CLI.** §15.16 channels it: shape changes are free, but new *capability* requests route through the RPC surface and get a design-section home first.

## 7. Out of scope for v1

Restating §14 as a refusal list so the plan stays honest: no configuration diffing (load-on-empty stands), no native termios propagation (observe-only, with the subscribe-plus-RPC experiment path), no TLS or non-loopback legs, no uevent hotplug (polling stands), no IOKit resolver, no yamux substrate, no replay ring, no combiner node, no systemd socket activation. Each has a design-section home when its time comes.

## 8. Start here

The first concrete steps, in order: initialize the workspace with the crates and CI (fmt, clippy, test, cargo-deny with the allowlist and the §13 ban list); prove the license gate on a scratch branch; build `nexus-doctor` with the software probes (P1, P2) and run it on the kernel-of-record; add the hardware probes (P3, P4) and run against whatever adapters are on hand — ordering an FT4232, a no-serial clone, and a bag of jumper wires if the drawer lacks them; prototype the `nexus-rpc` types over a UDS; then open phase 1 with the endpoint types, the validator, and the `nexus-sim` skeleton — whose self-test (§4, phase 1) is the first validation script in the repository, so the agent's ability to check its own work exists before the first feature does.

## 9. Post-1.0 simplification track

Design §16 is the rationale; this section is the work, in priority order, each item agent-validated like everything else. *Executed in full — seven commits, each adversarially re-audited; the implementation notes carry the record.*

1. **Boundary-supervisor library (§16.1).** Extract the concurrent-halves / park-don't-teardown / notify-on-loss / join-then-transition lifecycle into one abstraction; rebase serial, exec, and leg onto it. *Validation:* `all.sh --through 8` and the hardware rig script pass unchanged; the pump property tests move into the library; the exec-deadlock and stale-status regression tests now exercise library code paths.
2. **Critical-section cell (§16.2).** Replace raw `RefCell` daemon state with a closure-only cell; add `disallowed-types` for `std::cell::RefCell` in `serialnexusd`'s clippy config. *Validation:* clippy gate fails on a planted `RefCell`; the phase 4 waiting suite passes unchanged.
3. **`nexus-sys` crate (§16.3).** One internal crate for every unsafe wrapper (ioctls, `ptsname`, `poll_ready`/`poll_blocking`); `#![forbid(unsafe_code)]` everywhere else including the sim. *Validation:* a grep gate proves `unsafe` appears only under `nexus-sys/`; `cargo check --target x86_64-apple-darwin` stays clean with the cfg-gating written once.
4. **Harness hardening (§16.5).** Shared assertion helpers with their own tests (the soak jq-tautology becomes a helper regression test); shellcheck and a jq-lint pass added to the per-push gates. *Validation:* the planted-tautology test fails the helper suite; shellcheck gate green across `scripts/`.
5. **Nightly full sweep (§16.5).** The nightly CI lane runs `all.sh --through 8` plus soak and fuzz. *Validation:* the nightly job manifest exists and its artifacts include the sweep's 42-verdict JSON.
6. **State-file fsync (§16.6).** Fsync the temp file and directory around the snapshot rename. *Validation:* existing crash-recovery script unchanged and green; a comment-pinned unit test asserts the fsync calls (mock or strace-based spot check acceptable).
7. **Error-code registry (§16.8).** Codes defined once in `nexus-rpc`; the `docs/rpc` table rendered or asserted from it. *Validation:* a test fails when a daemon-emitted code is missing from the registry.

Explicitly not on this track, with reasons recorded in design §16.9–16.10: readiness unification (rejected — it moves lock consultation across threads), and the standing §14 deferrals (unchanged urgency).

## 10. Extension track: out-of-tree codecs

Design §15.26 is the rationale; these items make a separate closed-source codec repository a supported, CI-proven pattern. Priority-ordered, agent-validated. *Executed in full, including the audit's `precheck_codecs` hardening: `load --replace` validates codec names and attribute schemas before teardown, so a bad table can no longer destroy a good graph.*

1. **Library/binary split plus registry-as-value.** `serialnexusd` becomes a thin binary over a `nexus-daemon` library: run options in the library, flag parsing and the tracing subscriber in the binary; the codec registry becomes `Registry::with_builtins().register(name, factory)` with startup-time collision errors; the entry surface (`run`, registry, version constants) is the only public API, everything else `pub(crate)` or `#[doc(hidden)]`. *Validation:* `all.sh --through 8` green unchanged; a doc-tested example in the library compiles the twelve-line `main`; `cargo doc` shows exactly the intended public items.
2. **The `info` verb.** Daemon version, wire and envelope versions, registered codec names over RPC; unknown-codec load errors include the available list; `serialnexusctl info` renders it. *Validation:* `info | jq -e '.codecs | index("reference")'`; a config naming a bogus codec fails structurally with the list present in the error payload.
3. **External-consumer template.** `examples/external-codec/` (workspace-excluded): a trivial codec crate against `codec-api` and a custom daemon crate against `nexus-daemon`, standing in for the closed repository. *Validation:* a CI job builds it from the consumer's position (its own manifest, path dependencies standing in for tags), boots the custom binary, loads a config naming the example codec, and asserts `info` lists it — the pattern is proven per push, not promised.
4. **`codec-api` conformance kit (`test-support` feature).** Generic suites any `Codec` implementation instantiates in its own crate's tests: resync termination on garbage, bounded parser state, per-channel mux↔demux round-trip identity where both orientations exist, fragmentation tolerance. *Validation:* the reference codec's tests migrate onto the kit; a deliberately broken toy codec fails each suite.
5. **Exec conformance harness.** A `nexus-sim` mode driving an external codec child through the golden vectors plus the behavioral battery: sustained simultaneous full-duplex flow (the §15.22 deadlock class, as a test), oversize-chunk fragmentation, kill-and-restart cleanliness, and round-trip identity when the child supports both orientations; standard JSON verdict. *Validation:* the Python passthrough fixture passes; a deliberately half-duplex-coupled fixture fails the liveness case; `docs/codec-authors.md` documents the harness as the closed-repo CI entry point.

The recommendation order is deliberate: items 1–3 make the Rust path the easy path, which the design prefers; items 4–5 serve both forms and keep the exec route honest for everyone else.

## 11. Web console track

Design §17 and §15.28 are the rationale; the daemon work is deliberately two small features, and everything else is a client. Priority-ordered, agent-validated.

1. **The tap.** `tap.open <endpoint> [--replay]` / `tap.close` on the control plane: a connection-scoped, read-only dynamic attachment at a host-facing endpoint, streaming `tap.data` notifications (base64 chunks) with a bounded per-tap queue and drop counters; detaches on connection drop. *Validation:* a sim source through a tapped endpoint — the tap-side checksum equals a co-attached sim client's; a deliberately unread tap accumulates counted drops while the sim client stays byte-exact; dropping the tap's connection detaches it (`state` shows zero taps); `dump` is byte-identical before and after tapping, because taps never touch configuration.
2. **The replay ring.** Per-host-facing-endpoint `replay_ring = <bytes>` (default off); `tap.open --replay` delivers ring-then-live under the exact-splice guarantee. *Validation:* stream a seeded source, open a replay tap mid-stream, assert the concatenated replay-plus-live checksum equals the source from ring-depth onward — no gap, no duplication — under load; a ring-off endpoint answers `--replay` with an explicit empty-replay marker; the attribute round-trips through dump/load.
3. **`serialnexusweb` scaffold.** New crate: RPC client plus HTTP/WS server under the §15.29 three-tier bind policy — loopback+token default, `--tls`+token for non-loopback, `--insecure-bind` as the token-mandatory named footgun; per-session bearer token printed as a bootstrap URL, moving to a header off loopback; Host validation covering the configured names. *Validation:* a request without the token gets 401 in every tier; a wrong Host is rejected; a plain non-loopback `--bind` exits with the documented error while the same bind succeeds under `--tls` or `--insecure-bind` (the latter printing its forfeiture warning verbatim); the WS byte stream for a tapped console checksums against the sim source end to end through the browser-facing protocol.
4. **The console UI.** Left rail from `state` plus `subscribe` (status and lock badges live), right terminal pane per selected console (replay marker, tap drop counter surfaced when nonzero), bottom-right `send` line with holder-named LOCKED handling and an explicit steal affordance. Static embedded assets, no JS build toolchain; the renderer vendored and permissive. *Validation:* API-level — the UI's data endpoints asserted with jq (console list matches `state`, the lock badge flips on a scripted steal, send round-trips through the echo oracle); rendering itself stays presentation per §15.16.
5. **Docs and the security page.** `docs/security.md` gains the web section: the local-user delta, the token model, the §15.29 three-tier bind policy with its token-is-not-TLS rationale, and SSH forwarding as the zero-new-code remote path. *Validation:* the docs suite links it; `serialnexusweb --help` states the token behavior and the insecure-tier forfeitures verbatim.
6. **The TLS tier.** `--tls` via rustls (permissive; cert and key paths, plus a generate-self-signed-on-first-run convenience for lab use). *Validation:* a `curl --cacert` round-trip over a non-loopback bind passes with the token and fails without it; cargo-deny stays green with the rustls tree; the plaintext-refusal case from item 3 still holds when `--tls` is absent.

Dependency note for the §13 gate: the HTTP/WS crates and any vendored frontend component clear cargo-deny's permissive allowlist like everything else — as built: tokio-tungstenite (post-handshake framing only), rustls+rcgen pinned to the `ring` backend, hand-rolled HTTP per the §15.13 ethos.

*Items 1–6: executed, adversarially audited, green — including the hardenings the audit forced (the spy-outside-the-graph accounting so a ring never hides `discarded_unattached`; the counted `feed_dropped`; the cookie-carried token; the bridge's graph-verb denylist). Items 7–9 below are the §15.32 follow-on.*

7. **Default rings everywhere.** Flip `replay_ring` to default-on (64 KiB) and implement per-channel rings on codec, exec, and leg host-facing channels, superseding the serial-only scoped deferral. *Validation:* a fresh daemon with an unannotated config serves a non-empty replay on every console including a codec channel; `replay_ring = 0` opts out and restores the empty-replay marker; the splice-exactness test passes on a codec channel; the `active_tap_feed_does_not_hide_unattached_loss` regression stays green with rings defaulted; the §15.19 benchmark re-runs with rings on and stays within its recorded bounds.
8. **Tap stream offsets.** `tap.data` carries the endpoint's monotonic hostward byte offset, replay responses carry `from_offset`, and `info` exposes the per-boot `instance` nonce. *Validation:* a `wsclient` reconnect mid-stream reconstructs the source exactly once by offset-trimming its second replay (byte-exact against the sim manifest); a daemon restart changes `instance` and the client detects the reset instead of splicing across it.
9. **Browser-side history (OPFS).** Per-console append-only history keyed by socket path, endpoint, and instance; 16 MiB trim-oldest cap; `navigator.storage.persist()` requested with granted-status surfaced; export and clear controls; stable default port; memory-only fallback with a visible indicator. *Validation:* the offset-splice and retention logic lives in a pure JS module with its own CI-run tests; the OPFS adapter itself is thin and rides the hardware/manual checklist per §16.7, with its persisted/best-effort status observable in the UI rather than assumed.
