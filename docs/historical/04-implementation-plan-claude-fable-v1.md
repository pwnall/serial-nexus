# serial_nexus — Implementation Plan

**Status:** Ready to execute.
**Companion:** `serial_nexus-design.md` — section references (§) below point there. The design is normative; where implementation reality disagrees with it, the design gets a new §15 entry before the code diverges.
**Shape:** Nine phases (0–8), each with a goal, scope traced to design sections, key tasks, testable exit criteria, and an agent-validation block of concrete commands with expected outcomes. Sizes are relative (S/M/L) because calendar mapping depends on availability, not on the work.

## 1. Approach

Five principles order everything below.

**Retire risk before writing architecture.** The design flags exactly one mechanism for early verification (§15.14: EXTPROC + TIOCPKT), and several others rest on kernel behaviors worth confirming on real systems (PTY presence detection, serial2's exclusivity and unplug semantics, by-id resolution on awkward adapters). Phase 0 answers these as throwaway spikes with written findings; anything that contradicts the design amends the design first.

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
- `spikes` — phase 0 throwaway binaries, excluded from release artifacts, kept in-tree because their findings are documentation.

**Concurrency architecture (decided here, per §5).** The data plane runs on a current-thread tokio runtime: nodes are plain structs owned by one thread, the synchronous deliver contract needs no locks, and serial-rate throughput fits with room to spare (benchmarked in phase 3, not assumed). Control-plane connections run as tasks on the same runtime — mutation serialization falls out for free. File writers are the one exception, on the blocking pool (§5's regular-file rule).

**Licensing enforcement is CI, not vigilance.** `cargo-deny` runs on every push with the permissive allowlist (MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib, Unicode-DFS) and an explicit ban list naming the known landmines from §13 — `serialport`, `mio-serial`, `tokio-serial`, and any libudev binding — so they cannot re-enter transitively. Phase 0 proves the gate by adding `serialport` on a scratch branch and watching CI fail.

**Rust hygiene.** Edition 2024; MSRV pinned in CI to whatever tokio + serial2 require; rustfmt and clippy with warnings denied; `#![forbid(unsafe_code)]` everywhere except a single small `sys` module in `serialnexusd` isolating the raw ioctls that nix/rustix don't wrap (TIOCGICOUNT is the known candidate). `tracing` wired from phase 2 so debugging never starts from printf. Configuration files are TOML; the RPC carries JSON — both through serde, so the attribute-table pattern (§8) is uniform.

## 3. Validation toolkit

**The external-tool question, answered.** socat is the canonical plumbing tool and does exactly the needed trick — generating a PTY whose slave side another process opens like a serial line, with a `link` option for a stable symlink — but it is GPL-2.0, and no mainstream permissive equivalent covers the PTY side (openbsd-netcat, BSD-licensed, covers only sockets). Under the §13 policy socat may still *run* beside the project, so it remains an optional manual cross-check; but no validation script requires it, for two better reasons than license comfort: an external relay cannot judge outcomes, and a purpose-built double can.

**`nexus-sim`: one binary, several doubles.** Every mode is deterministic under `--seed`, prints a single JSON verdict line on exit (`{"tool":"nexus-sim","mode":...,"pass":...,"sent":...,"received":...,"sha256":...}`), and exits 0 only on pass. Modes, introduced as phases need them:

- `pty` — create a PTY pair via the same permissive calls the daemon uses; maintain `--link PATH` (a stable symlink to the pts node, standing in for a device path or a by-id entry); run a behavior: `--echo`, `--source` (seeded generator at `--rate`, `--bytes`), `--sink` (count and checksum), `--script` (expect/send exchanges), `--stall-read-after N` (stop draining, for Busy and head-of-line tests), `--hup-after`/`--reopen-after` (client-presence fault injection), `--report-termios` (observe what the daemon applied to the pair — validates the §7.1 reopen ritual and §7.2 baseline from the far side).
- `client` — open the *daemon's* PTY like an operator would: send seeded data, verify echoes or expected streams, throttle with `--read-rate` (slow-consumer tests), report attach/HUP observations.
- `mux` — emit and verify reference-framed multichannel streams with per-channel manifests (seed, byte counts, checksums), plus `--corrupt-every N` with a computed expected-loss manifest for resynchronization tests (phase 5).
- `envelope` — drive an external codec process through the golden-vector battery (phase 5).
- `wire` — speak just enough v1 protocol to be a hostile or conforming peer: crafted hellos (`--hello-version`, bad magic), oversize and truncated frames, unknown-channel data. This mode *is* the driver for the §9 conformance suite (phase 6).
- `tcp-proxy` — sit between two daemons with `--drop-after`/`--restore-after` for unprivileged link-outage injection (phase 6).

**The resolver seam.** The resolver takes a root prefix (`--dev-root`, default `/`), making `/dev/serial/by-id` a fixture directory in tests: symlink trees pointing at `nexus-sim` pts nodes reproduce normal adapters, no-serial clones (by-path only), FT4232-style multi-interface devices, and identity squatters — the whole §12 matrix, unprivileged, no hardware (phase 7). This is a documented, first-class test seam, not a hidden hook.

**Assertion conventions.** State and RPC assertions use `serialnexusctl --json ... | jq -e '<predicate>'` (jq is MIT-licensed; `-e` turns the predicate into the exit code). Time-dependent conditions use `scripts/lib/wait-for.sh '<command>' <timeout>` — bounded polling on state, never bare sleeps. `scripts/lib/semantic-diff.sh` compares two configuration dumps after normalization. Raw-protocol pokes may use openbsd-netcat (`nc -U`) or `serialnexusctl raw`. `scripts/validate/all.sh --through N` runs every script up to phase N; CI runs the unprivileged lane on every push, a privileged lane (tmpfs disk-full variant) where runners allow it, and the nightly lane (E2E scenarios, soak).

## 4. Phases

### Phase 0 — Spikes and scaffolding (M)

*Goal:* every kernel-behavior assumption in the design is confirmed or corrected on real systems; the repository enforces its own rules.
*Implements:* preconditions for §7.1, §7.2, §12, §13, §15.14.

Scaffolding: workspace, CI (build, test, fmt, clippy, cargo-deny), the ban-list proof above. Spikes, each a small binary answering a written question:

1. **S1 — EXTPROC/TIOCPKT (§7.2, §15.14).** With EXTPROC set on the slave, does tcsetattr produce a TIOCPKT_IOCTL packet on the master? Does *clearing* EXTPROC produce a final packet, and does re-asserting it via the master work with a client attached? Record kernel versions tested.
2. **S2 — PTY presence (§7.2 HUP handling).** Confirm POLLHUP semantics with zero slave openers (both never-opened and closed-after-open), confirm no un-HUP event exists, confirm termios reset through the master with no slave open, and measure the zero-timeout HUP-check cost.
3. **S3 — serial2 fit (§7.1, §13).** Custom baud on a real adapter; modem-line get/set for the control verbs; whether TIOCEXCL/O_NOCTTY are exposed or need the `sys` module on the raw fd; the exact error surface on physical unplug (feeds faulted-and-wait detection).
4. **S4 — Resolver ground truth (§12).** readlink parsing of `/dev/serial/by-id` across a normal adapter, a no-serial clone, and a multi-interface FT4232; the by-path fallback's actual shape; behavior of the tree when identities collide.
5. **S5 — RPC skeleton (§10).** Half a day: newline-delimited JSON-RPC over a UDS with serde — request, result, error, notification — fixing the `nexus-rpc` type shapes.

*Agent validation:*

1. `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace` exits 0.
2. `scripts/validate/phase0/license-gate.sh` injects `serialport = "*"` into a scratch copy of the manifest, asserts `cargo deny check` fails there, and asserts it passes on the clean tree — the gate is proven, not assumed.
3. Each spike is self-judging: `cargo run -p spikes --bin s1_extproc` prints one JSON verdict line encoding observed-vs-designed behavior and exits nonzero on mismatch. **A nonzero spike is a stop condition:** the agent surfaces the findings for a design amendment (§1) instead of coding around them. S3 and S4 verdicts against real hardware run under the hardware checklist (§5) when no adapter is attached to CI.

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

The daemon binary: current-thread runtime, control socket with the §10 path policy and permissions, load-on-empty with full structural validation, serial node on serial2-tokio (device given as a raw path for now — the resolver upgrade lands in phase 7 without config-format changes, because identity strings were designed for this), PTY node with the phase-0-verified mechanics, and `serialnexusctl` with `load`, `dump`, `state`, and `--json`. The fake device is `nexus-sim pty --echo --link`, standing exactly where `/dev/ttyUSB0` will.

*Agent validation (`scripts/validate/phase2/e2e.sh` and siblings):*

1. Boot sequence: sim device up, daemon up in a temp `$XDG_RUNTIME_DIR`, `serialnexusctl --json load demo.toml | jq -e '.result'` — structural errors in a deliberately broken sibling config are rejected with the offending node named and nothing created (`state` shows an empty graph).
2. Data path: `nexus-sim client --path $TMP/tty/usb0 --send seeded:64KiB --expect echo` passes — bytes traverse client→daemon→device→daemon→client with checksums intact.
3. Presence: with the client attached, `wait-for` `state | jq -e '..client_present==true'`; after client exit, false within one second; baseline termios confirmed from the device side via sim `--report-termios` (raw, echo off, EXTPROC).
4. Round-trip: `dump` → fresh daemon → `load` → `dump`; `semantic-diff.sh` exits 0.
5. Protocol hygiene: an unknown method over `nc -U` yields JSON-RPC error `-32601` (`jq -e '.error.code==-32601'`); `stat -c %a` on the socket prints `600`; everything ran unprivileged.

### Phase 3 — Boundaries and logging (M)

*Goal:* every §5 policy exists, measured, with counters in state.
*Implements:* §5 (boundary policies, discard-when-unattached, counters), §7.3 entirely.

Drop policies with counters on PTY and (later-reused by leg) socket boundaries; serial discard-when-unattached plus TIOCGICOUNT surfacing; the log node's bounded queue, writer task, on-demand rotation, counter recovery by directory scan, and flush-on-removal; `rotate` verb; `subscribe` notifications for counters and node status.

*Agent validation:*

1. Firehose integrity: sim `--source --rate max --bytes 512MiB --seed 42` through the daemon to a fast sink client — sink checksum equals source checksum; daemon `VmRSS` sampled before/during/after stays within a fixed bound (no interior accumulation).
2. Loss accounting is exact: with the client throttled (`--read-rate 10k`), the PTY boundary's drop counter equals `source_sent − client_received` to the byte, while the log node's file checksum remains complete — loss is located, counted, and isolated.
3. Rotation loses nothing: rotate five times mid-stream; `cat serial.log.00* serial.log | sha256sum` equals the source's reported checksum, and the recovered rotation counter after a daemon restart continues from the directory scan, not from memory.
4. ENOSPC without root: point a log node's file at `/dev/full` — the node faults with an ENOSPC reason while the port and its other consumers keep passing the echo probe; the tmpfs full-disk variant runs in the privileged CI lane.
5. `subscribe` streams the counter and status transitions above (`serialnexusctl --json subscribe` with a timeout, piped to jq predicates); discard-when-unattached counters increase while no client is attached and freeze when one is.
6. The throughput benchmark writes `docs/benchmarks/phase3.json`; the script asserts headroom of at least 10× over 8 ports at 3 Mbaud, making §5's single-thread assumption a recorded fact.

### Phase 4 — Arbitration (M)

*Goal:* the §6 lock machinery, end to end, including its failure etiquette.
*Implements:* §6 entirely; extends §10 with `lock`/`unlock`/`send` and lock-state notifications.

Write modes on edges; the per-endpoint lock as a gate on the pause machinery; explicit acquire/release/steal/lease over RPC; the atomic `send`; purge-on-acquire and purge-on-detach with counters; `free-for-all` opt-out. The test topology needs no codec: two PTY nodes attached to one serial endpoint is a legal §4 fan-out, with the sim device recording exactly what reaches "hardware."

*Agent validation:*

1. Exclusivity is byte-exact: client A locks and sends seeded-A; client B sends seeded-B without the lock; the sim device's received checksum equals seeded-A exactly.
2. The 3-a.m. regression: after A unlocks, a bounded wait shows the device checksum unchanged (B's buffered bytes did not fire); when B detaches, the purge counter increases by exactly `len(B)`.
3. Purge-on-acquire: B writes before locking, then locks — pre-grant bytes are purged and counted; bytes sent after the grant arrive intact.
4. Steal and lease: `lock --steal` transfers the lock, records the steal in state, and emits a `subscribe` notification; an expired lease releases a silent holder within the configured bound (`wait-for`).
5. `send` semantics: while A holds, plain `send` fails with the documented locked error; `send --steal` delivers the line exactly once (device-side count).
6. A `write = never` spy receives the complete hostward checksum and its write attempts change nothing (device checksum stable, no lock contention visible in state).

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

The resolver with its fallback chain and add-time echo-back; identity-form versus path-form add semantics; polling-based reappearance detection; reopen ritual (termios, TIOCEXCL, modem lines, purge); the state file written after each successful mutation and preferred at startup; `remove-node --cascade` with log flushing; `send-break`/`set-modem`/`pulse-dtr`. All of §12's hardware matrix runs unprivileged against fixture by-id trees over `--dev-root` (§3), with real adapters reserved for the checklist.

*Agent validation:*

1. Unplug keeps clients alive: kill the sim device and remove its fixture symlink — the node reaches `waiting` within the poll interval, and the attached client sim reports its fd still open, no HUP.
2. Replug heals and reapplies: recreate the sim at the same identity — node `active`, purge-on-reconnect counters set, and sim `--report-termios` shows the configured termios reapplied (the reopen ritual, observed from the device side).
3. Squatters are refused: recreate the fixture symlink pointing at a *different* sim identity — the node stays `waiting` and the squatter sim's received-byte count is zero.
4. The §12 matrix on fixtures: an FT4232-style tree yields four independently bound nodes with independent checksum streams; a no-serial clone binds by-path with the documented warning present in the add-time RPC result (`jq -e '.result.warning'`); path-form add with the device absent fails as designed, identity-form add succeeds into `waiting`.
5. Crash recovery is exact: add a log node incrementally, `kill -9` the daemon, restart — `dump` semantic-diffs equal against the pre-kill dump; PTY symlinks are recreated; a fresh client attaches and passes the echo probe (the HUP across daemon death is the documented, unavoidable §7.2 behavior).
6. Signal verbs reach the wire: `pulse-dtr` and `send-break` are observed and reported by the sim device's verdict (master-side observation), and `remove-node --cascade` flushes the log queue — file checksum complete — before the node disappears.

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

**Hardware checklist (manual, release-gating).** The things fixtures cannot fully prove: real USB adapters including a no-serial clone and an FT4232; physical replug during traffic; an adapter swap exercising wrong-identity refusal; a real 3-wire target for the break/modem verbs; the macOS pass. One page, checked at `0.1.0` and `0.2.0` — and each checklist run doubles as calibration for the fixtures and sim behaviors (see risks).

**What is deliberately not tested.** Rendering details of `serialnexusctl` beyond `--json` correctness (per §15.16); throughput beyond the documented headroom benchmark (this is a control-and-observation tool at serial rates, not a data mover).

## 6. Risks and mitigations

**EXTPROC behaves differently across kernels or not at all (S1 fails).** The design already contains the fallback — §7.2's reconciliation poll becomes the only mechanism, and only observation latency degrades. Decision point at end of phase 0, recorded as a §15 amendment either way.

**serial2 lacks a needed control (S3 finds a gap).** The `sys` module applies the missing ioctl on serial2's raw fd; the full fallback (rustix termios, §13) exists but is not expected to be needed. Watch item, not a blocker.

**Test doubles drift from real hardware.** `nexus-sim` behaviors are written from the phase 0 spike findings, not from assumptions, and every hardware-checklist run re-validates the doubles against real adapters; a divergence is treated like a failing spike — design or sim amended before code trusts either.

**Single-thread data plane hits a ceiling.** Phase 3's benchmark makes this empirical early and records the headroom in-tree. The §5 contract permits sharding whole subgraphs per thread later without changing node code; nothing in v1 should need it.

**Protocol churn during phases 5–6.** Contained by construction: the conformance suite and the envelope corpus are written before the second framing change is entertained, and §15.15's two-contract split means wire changes cannot break external codecs.

**Scope creep via the CLI.** §15.16 channels it: shape changes are free, but new *capability* requests route through the RPC surface and get a design-section home first.

## 7. Out of scope for v1

Restating §14 as a refusal list so the plan stays honest: no configuration diffing (load-on-empty stands), no native termios propagation (observe-only, with the subscribe-plus-RPC experiment path), no TLS or non-loopback legs, no uevent hotplug (polling stands), no IOKit resolver, no yamux substrate, no replay ring, no combiner node, no systemd socket activation. Each has a design-section home when its time comes.

## 8. Start here

The first concrete steps, in order: initialize the workspace with the crates and CI (fmt, clippy, test, cargo-deny with the allowlist and the §13 ban list); prove the license gate on a scratch branch; write S1 and S2 against a kernel-of-record and record findings; write S3/S4 against whatever adapters are on hand, ordering an FT4232 and a no-serial clone if the drawer lacks them; land `nexus-rpc` types from S5; then open phase 1 with the endpoint types, the validator, and the `nexus-sim` skeleton — whose self-test (§4, phase 1) is the first validation script in the repository, so the agent's ability to check its own work exists before the first feature does.
