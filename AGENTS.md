# AGENTS.md — working notes for serial_nexus

Orientation for an AI agent (or human) picking this repo up cold. It captures what
the code *is*, how to build/verify it, and the hard-won invariants you must not
regress. When this file and the design disagree, **the design wins** — see
`docs/30-design-claude-fable-v13.md`. One caveat learned twice: each new design
generation so far has been rebased from a stale base and silently *dropped* rules the
code still enforces. Before acting on "the design no longer says X", diff it against its
predecessor in `docs/historical/` sentence by sentence; the fix has both times been to
restore the text, not to delete working code (implementation notes, v12 and v13 tracks).

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
TCP/Unix socket, loopback-only unless opted out), `map` (§7.8 — a stateless per-console
character transform: picocom's `--imap`/`--omap` byte mappings applied once in config; the
first *non-codec* interior transform, host-facing default endpoint + a target-facing `raw`
endpoint, raw edge defaults to `held` with steal-to-bypass). `existing-terminal` (§7.7) is
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
  Current figure: **636 passed / 0 failed / 4 ignored** on Linux; **623 / 0 / 4** on macOS
  15.7.8 with a crossover rig (2026-07-28) — see the macOS bullet below.
  **Run it with `--no-fail-fast` when you are validating a platform rather than a change.**
  `cargo test` stops at the first failing *crate*, and that is how a single `nexus-daemon`
  unit test hid three further macOS failures from CI for six consecutive red pushes: the
  lane never reached the integration harness at all, so "macOS is red" looked like one
  known problem and was four.
- **All planned phases 0–8 are done**, plus five post-1.0 tracks: the simplification
  track (design §16 / plan §9), the out-of-tree-codec extension track (design §15.26 /
  plan §10), the web console track (design §17 / plan §11.1–§11.6), the **v10 track**
  (design §15.32 / plan §11.7–§11.9 + §16.11): **default-on replay rings** (64 KiB on every
  host-facing endpoint, per-channel on codec/exec/leg, opt out with `replay_ring = 0`),
  **tap byte offsets** (`tap.data.offset`, `tap.open`'s `from_offset`, `info.instance`
  nonce), **browser-side OPFS history** in `serialnexusweb`, and the bash retirement; and
  the **v11 console-map track** (design §7.8/§15.33 / plan §12): the **`map` node** — a
  stateless per-console character transform (picocom's byte mappings), the first non-codec
  interior transform, slotting into the endpoint-keyed wiring with no `Wiring::build`
  structural change. Its raw edge defaults to `held` (an omitted/on-demand map raw edge is
  promoted to `held` by `GraphConfig::effective_write_mode`, mirroring the log→never override;
  `never` makes a read-only map, which is now inert rather than destructive — the mapped
  endpoint's targetward receiver is drained-and-counted instead of dropped); and the
  **v12 graph-editing track** (design §15.35 / plan §14): the passive **`ports`** verb
  (the resolver's enumeration face — identity, current path, bound-status, built from
  by-id/by-path readlinks, sysfs and a BSD `cu.*` scan, and **never `open(2)`**, because
  probing a port toggles DTR), the **`connect`/`disconnect`** edge-surgery verbs (live
  edge add/remove under the same critical-section validation as `load`), and the web
  console's **graph and editor pages** with the bridge allowlist widened to the
  graph-editing verbs; and the **v13 browser-UI track** (design §15.37/§15.38 / plan §15):
  a pinned **Playwright suite** (`serialnexusweb/ui-tests/`, Chromium only) driven by the
  `nexus-itest` gate `p8_web_ui.rs`, which took the web console's browser half off the
  §16.7 manual checklist. It found three real defects on its first run — see §2's
  "What the browser suite found" below.
- **Deferred / not implemented on purpose:** design §14 items, and the RPC verb
  `set-attribute` alone (it returns `-32601`; remove-and-re-add covers it). `connect` and
  `disconnect` **left that list in v12** (§15.35) and are implemented.
  `existing-terminal` node (§7.7). A
  serial node's `faces = "target"` output-leg role (§7.1) is *described* by the design and has
  no wiring, so it is now **refused structurally** rather than loading a port it seizes with
  `TIOCEXCL` and attaches to nothing; a standalone re-multiplexing codec (`faces = "host"`)
  now comes up `waiting` with a §14 reason, which is what §7.5 promised, instead of `faulted`.
- **macOS full-suite pass on a crossover rig (2026-07-28), and the one real defect it left
  open.** The first run of the *whole* suite on a Mac — CI's macOS lane had been red on six
  consecutive pushes, always the same `nexus-daemon` unit test, and because `cargo test`
  fail-fasts per crate it had **never once reached the integration harness**. Four failures,
  not one. All four hardware-rig tests pass (byte-exact both directions at 115200 and
  250000, `send`, `TIOCEXCL`, the signal verbs, the v11 `map` node over the physical
  crossover), as do `fmt`, both clippy gates, `cargo deny` and `expectations/macos.jq`.
  **Three of the four were guards asserting a Linux-specific proxy for a property the
  daemon satisfies portably**, and each is now written against the property itself: (1) the
  §7.2 flush test asserted `!POLLIN` on the master, where a Darwin master with no slave
  reports `POLLIN|POLLHUP` *unconditionally* and answers `read` with 0/EOF (Linux: EIO) —
  it now asserts on the **read**, which is what `read_and_poll`'s `saw_session` actually
  latches on (`Ok(n) if n >= 1`), and which is strictly stronger on Linux; (2) the two
  LEG-2 guards required `accepted_targetward` to settle *nonzero*, which silently assumed
  the AF_UNIX buffer is at least one 64 KiB wire frame wide — macOS's is **8192 bytes**
  against Linux's ~208 KiB, so the counter correctly pins at 0 there (it credits per
  *completed* frame) — the predicate is now "frozen while bytes are still owed", the
  portable form `p6_fragmentation` already used and stricter on Linux besides (the old one
  also accepted a plateau at `== sent`, a fully drained peer, which is not the parked state
  at all). A fourth, pre-existing and unmasked once those two stopped burning 30 s
  timeouts: `WirePeer::dial` raced the leg's `listen(2)`, because the callers' readiness
  proxy is `sock.exists()` and **`bind(2)` creates the socket file one syscall before
  `listen(2)` stops `connect(2)` answering ECONNREFUSED**; the dial now retries to a
  bounded deadline. **The remaining one is a genuine, operator-visible macOS defect and is
  NOT fixed** — see §7's macOS arm; its guard skips there rather than being retired, and
  the skip names the gap.
- **CI flake remediation + 6.18 probes (2026-07-26/27):** CI had been red on four of the last six
  pushes, on three *different* tests. All of it is fixed; see `docs/implementation-notes.md` for the
  mechanisms. In short: the `p8_web` RFC 6455 test client was not frame-atomic w.r.t. its deadline
  and desynced (**the obvious `pending`-buffer fix is insufficient — it was measured**; the shape is
  fill-then-commit); `nexus-sim`'s echo double died on a bare pty `POLLHUP` and its null modem
  busy-spun a core; nine test files asserted byte-exactness at the lossy console boundary (see the
  new rule in §5); and — a **real product defect** found beside them — a collapsed pty session
  carrying no data byte leaked the exclusive write lock, fixed kernel-independently rather than with
  the `|| closed` arm §7 forbade deciding on 7.0-only evidence. (Two different constructs, so do not
  merge them: that `|| closed` *disjunct* was rejected on the evidence rule, while invariant 16
  rule (3) separately bars the `closed` **conjunct** for a reason no kernel touches. §7's rule is
  discharged for P6/P7 now — see §7 — and rule (3) is not, because no probe speaks to it.) `nexus-doctor` gained **P6–P11** to settle the
  remaining kernel questions on 6.18 by diffing two runs. **That diff was taken on 2026-07-27** and
  is recorded in §7 and `docs/nexus-doctor.md`; it changed nothing in the daemon and licensed no
  simplification, but it did surface **two over-claims in the doctor's own report text**, both fixed
  on 2026-07-28 and both about a *Tier-1* rig, which §13 makes the baseline: P5 said "Rig discovered
  and **certified**" for any UART rig, so a dangling converter's certificate invited a Tier-2/3 run
  to start from it, and P11 offered "P5's deliberate baud-mismatch item" as the usual cause of a
  nonzero `frame` on a rig where no pair existed to mismatch. `p5_rig` now returns `RigFacts`
  (certified pairs, loopbacks, `tier()`) beside its `Probe`; P5 names the tier and what that tier did
  **not** run, and P11 reads the same value rather than guessing — `named >= 2` is not the same
  question, two dangling ports being two named ports and no pair. **And every report now says what
  produced it**: both renderers open with a `commit` / `probe set` / `generated` (UTC) block, because
  establishing that the 6.18 run predated HEAD took reading a *section title* by eye. The probe-set
  fingerprint is the load-bearing half — it digests each probe's `(id, title, question)` and *not* its
  measurements, so equal fingerprints mean "these two runs asked the same questions" without needing
  the repository, where a commit hash needs the reader to diff two commits. `expectations/*.jq` gained
  a **presence** clause for both fields (a tarball build legitimately reports `commit: unknown`, and
  reddening it would be the false negative P4's clause already refuses). No new dependency: `build.rs`
  shells to `git` with `std` alone and the UTC rendering is hand-rolled, the doctor's dependency list
  being part of the licensing gate.
  Suite at the close of *that* track was 480 passed / 0 failed / 4 ignored (the current figure is in
  §2's opening bullet).
- **What the browser suite found (2026-07-27), all fixed:** (1) the web client's
  `offsetSpaceReset` inferred an offset-space restart from `from_offset < frontier`, which
  is *also* what an ordinary reload with a replay ring looks like — so every reload
  re-rendered and re-stored the ring, growing stored scrollback by one copy each time. The
  daemon now reports a per-hub **`epoch`** on `tap.open` and the client re-anchors on that
  (invariant 10), which also closes the old "`instance` does not rotate on a hub rebuild"
  open issue. (2) `load --replace` opened a serial node's replacement device *before*
  releasing the outgoing fd, so the daemon EBUSY'd against itself: on real hardware a
  one-second `faulted` flap during which an accepted `send` was purged rather than
  written. `SerialNode::teardown` released `TIOCEXCL` — which was one of several paths that let a
  port go, so review 32 moved the release into the port itself (invariant 15); guard
  `p11_replace_atomicity.rs`, fail-first proved. (3) The same collision is *permanent* on
  any pty-backed serial device (see §8). Suite is 485 passed / 0 failed / 4 ignored.
  **Review 32's browser-history cluster is remediated on top of that:** the client now *marks* a
  replay ring that rotated past its stored scrollback (it used to splice across the hole while
  counting it into a `history.dropped` nothing read), sweeps the OPFS records of prior daemon boots
  rather than orphaning up to 16 MiB per console per restart, keeps the offset-space epoch **inside**
  the history object so the key/buffer/epoch triple cannot be adopted apart, escapes storage keys
  injectively (two legally-named consoles used to share one file and overwrite each other), and
  cancels the debounced save before a `clear` so a snapshot taken pre-confirmation cannot re-create
  the record. The rendered scrollback is also capped at 256 KiB of characters while the retained
  buffer stays at `history.cap` — an unbounded `<pre>` does not merely grow, its render throughput
  decays with the size of the pane (measured 34.9 → 14.4 KiB/s over the first 2.9 MB, against a flat
  121–144 KiB/s bounded), so the screen is a window onto what `export` hands back, not a copy of it.
- **Review remediation:** three full-workspace Opus reviews exist.
  `docs/historical/19-claude-opus-code-review.md` was remediated in `b9d8a50` and folded into v9.
  `docs/historical/26-claude-opus-code-review.md` (2026-07-25, 93 surviving findings, 20 refuted) was
  remediated before the v13 track, with its ledger at
  `docs/historical/27-review-26-remediation-ledger.md`. **Every review file in this repository is a
  frozen record of the review as delivered** — each still reads "nothing is fixed yet", because each was
  written before the remediation that answered it — so **read the ledger and
  `docs/implementation-notes.md`, never the review, for what is true now**. Its §6 lists the 20 refutations, which need no action and should not be
  re-filed. Two dispositions the remediation deliberately kept rather than changed:
  `nexus_core::data` stays the executable *specification* of §5 rather than being rebased onto
  the shipped boundaries (notes §3.18 — its module doc now says so outright, so do not read a
  green property test there as coverage of the data plane), and `arbitration` stays a per-node
  attribute applied to each host-facing endpoint (notes §3.16). Notes **§3.15 was withdrawn**:
  the `flow_control` spelling was a real defect, not a justified deviation, and `xonxoff` /
  `rtscts` are now accepted as serde aliases beside the canonical kebab-case. The third,
  `docs/32-claude-opus-code-review.md`, gets its own bullet below.
- **Review 32 remediation (2026-07-27):** `docs/32-claude-opus-code-review.md` — 87 of 99 candidate
  findings survived independent verification (**80 unique**, once the pairs two finders filed
  independently are merged), 10 were refuted — is remediated, and the per-finding record is
  `docs/33-review-32-remediation-ledger.md`. As with review 26 the review file is a **frozen**
  historical record that still reads as though nothing is fixed, so read the ledger for what is true
  now; the review's §6 tabulates the refutations, which need no action and should not be re-filed.
  The suite went **485 → 630 passed / 0 failed / 4 ignored**, and the guards are the `p12_*` family
  (fifteen files under `nexus-itest/tests/`, one per defect area — the `p9_*` convention, §5). Four
  clusters, stated as what a future session must not undo. (1) **The resolver's two §12 directions
  read one source:** `Resolver::sysfs_usb_devices` lists `<sys_root>/class/tty` and *both* capture
  and resolution go through it, with `/dev/serial/by-id` kept as a fast path **over** it rather than
  as the only source — a `usb:` identity minted where `/sys` exists and udev's serial links do not (a
  container handed `--device=/dev/ttyUSB0`, an image without `60-serial.rules`) resolved back to
  nothing while `add-node` reported success and the node waited forever for a device that was right
  there. Ambiguity is counted over **devices**, never over by-id entries — udev derives the link name
  from the very fields that make an identity ambiguous, so two clones sharing a serial number collide
  on one link and a link count could never exceed 1 — and `capture_from_path` canonicalizes a
  symlinked `/dev` path before deriving the device name, so the most canonical input an operator has
  stops degrading to `raw:<link name>` under a "not stable across reboots" warning that is precisely
  backwards for a by-id path. (2) **`TIOCEXCL` is owned by the port, not by one exit path** —
  invariant 15, which is where the reasoning lives. (3) **A parked waiting verb no longer starves its
  own connection (`CTRLW-1`):** `serve_connection`'s `select!` carries the in-flight verb as a lane
  instead of awaiting it inline, so that connection's `tap.data` and `subscribe` keep being delivered
  for the whole wait — `subscribe` traffic was merely late, but tap bytes were really *lost* — and
  `send`'s `timeout_ms` now bounds the **whole** operation, the acquire *and* the targetward write,
  so a backpressured endpoint cannot hold the write lock past the deadline the operator asked for.
  (4) **The §5 accounting holes are closed at four boundaries:** a log node whose writer has stopped
  (a fatal write under `overflow = "fault"`, a failed rotation) counts every byte handed to it
  afterwards in `dropped_bytes` instead of parking it in a `queued_bytes` nothing will ever drain; a
  `Codec::demux` refusal **faults** the node and shows up as `demux_errors` / `last_demux_error` /
  `multiplexed.discarded_hostward` rather than only in the log, so §7.5's sanctioned never-resync
  policy is visible in `state`; data decoded onto a channel identity the node is not configured for
  is still dropped (§8 — an announcement never grows the graph) but is counted as
  `discarded_unconfigured_channel`, with the identities named in a bounded `unconfigured_channels`
  list and the occurrences past its 256-entry cap in `unconfigured_overflow`; and a `leg` counts what
  an upstream producer shed at a `faces = "target"` channel's intake (`dropped_slow_consumer`) and
  the untransmitted tail its write half had already taken off the bounded receiver when the peer went
  away (`discarded_peer_gone`), with `delivered_hostward` no longer inflated by the bytes a full sink
  refused. Beside the four: the **browser-history cluster** (see the v13 bullet above); **three gates
  that could not fail** now can — the `TIOCEXCL` guard whose only assertion (`pass == false`) was
  equally satisfied by the daemon's own reader eating the echo, the licence gate that took *any*
  non-zero `cargo deny` exit (including a `cargo metadata` fetch failure on an offline runner) as
  proof the ban list fired, and the browser suite's spec-count floor, which sat at the *device-free*
  spec count and so let every device-bearing spec above it vanish in silence — each now asserting the
  **reason** rather than the outcome; and **`serialnexusweb --tls` treats its cert and key as one
  atomic pair**: both present is a load, *neither* present generates the lab self-signed pair, and a
  **half-present pair is refused at startup**, naming the file that exists and the one that does not.
  Do not "simplify" that back to `cert.exists() && key.exists()`: generating writes both paths, so a
  half-present pair used to truncate the operator's private key — unrecoverable, and the ordinary CA
  workflow of installing a key first walks straight into it — while the server came up green and the
  single log line named only the cert. Presence is decided with `symlink_metadata`, not `exists()` (a
  dangling planted symlink is *present*), and both files go through `create_new(true)`, with the key
  additionally narrowed to 0600 by an explicit `set_permissions` because `OpenOptions::mode()`
  applies only at creation. Guards: `serialnexusweb/src/tls.rs`'s unit tests and
  `nexus-itest/tests/p12_web_tls.rs`. Two apparatus contracts tightened alongside: a `nexus-sim
  client --recv/--drain` verdict now carries `timed_out`, and a run the wall clock ended never
  reports `pass: true` (a `--drain` that expired mid-stream used to exit 0 with the checksum of a
  truncated stream — verdict JSON byte-identical to a peer that genuinely lost bytes); and every
  write `exec-conformance` makes to the child is bounded by a five-second *idle* deadline, so a codec
  that stops reading its stdin fails that one check with `child did not drain stdin within 5000 ms`
  instead of parking the whole battery in `anon_pipe_write` forever with no verdict line and no exit.
- **The remediation was then audited, and the audit found regressions the remediation itself had
  introduced (2026-07-27).** Six agents got the review and a *frozen* tree, never the implementers'
  reports (§9), and were asked to refute "this is fixed". The per-finding verdicts, the five
  regressions, and the one suite failure that turned out not to be ours are in
  `docs/33-review-32-remediation-ledger.md` — read them there rather than re-deriving them. Three of
  its outcomes are settled rules now rather than history: **break** joined `TIOCEXCL` as a claim the
  *port* owns (invariant 15); the pty reader's **held payload** got the three rules that keep it from
  becoming a hiding place (invariant 16); and `serialnexusweb`'s pre-auth cap is enforced by
  **eviction**, never by refusing an accept (invariant 11). Two further outcomes change what `state`
  shows, so an operator meeting them recognises them. A pty now reports
  **`discarded_at_last_close`** — device bytes the kernel still held for a client
  that never read them, discarded when that client detaches so the next session starts on an empty
  pair. §7.2's last-close reset had only ever settled how the *next* session's bytes are framed,
  never *which* bytes it sees, so a fresh `picocom` opened onto the previous operator's scrollback,
  and nothing counted the discard when the pair was finally reused — `state` read
  `discarded_no_client: 0` on a boundary shedding kilobytes (guard
  `p12_pty_setup::a_fresh_console_session_does_not_inherit_the_previous_sessions_bytes`). And a
  console's *un-delivered* backlog, drained when its client detaches, is charged to §6's per-origin
  **`purged`** rather than to `discarded_targetward`: the latter stays reserved for bytes an endpoint
  that went away could never take, which is loss, while these were discarded on purpose at the moment
  the write floor settled. A reader watching a pty's `discarded_targetward` for detach-time backlog
  will now always see 0.
- **Configurations that used to load and now do not** (all refused *structurally*, before
  anything is created and before a `--replace` teardown): an unknown key anywhere outside a
  codec's opaque `attributes` table (`deny_unknown_fields`); a numeric field out of range
  (invariant 13); an edge into a codec/exec multiplexed endpoint whose mode is neither `held`
  nor `never`; two effectively-`held` edges on one arbitrated host-facing endpoint; a serial
  `faces = "target"`; a whitespace-only or over-256-byte name; and an operator `load` whose
  non-empty source text parses to an empty graph (a mis-typed `[[nodez]]` used to make
  `--replace` an unannounced `teardown` reporting success). Each names the offender.
  **Since v12 these are not load-only:** `connect` validates the candidate graph with the
  same `GraphConfig::validate` call, so an illegal edge added to a *running* graph is
  refused by the same rule with the same message and nothing changed.
  **Review 32 added two numbers to the list** (invariant 13 applied where it had not been): a pty
  `mode` outside `0o600 ..= 0o777` — the ceiling because a permission mode is nine bits, which is
  what rejects every three-digit *decimal* spelling of an octal one (`mode = 666` is 0o1232, owner
  `-w-`, and used to load and then fault the node with an EACCES that never mentioned the mode), and
  the floor because a pty's setup chmods the slave and then opens it `O_RDWR` itself to prime the
  session; and an exec codec's `restart_backoff_ms` above `MAX_TIMER_MS` (3600000), checked in
  `exec::parse_attributes`, which both `load`'s codec precheck and `add-node` run before anything is
  created — it was the one millisecond timer in the schema with no cap, so three slipped digits
  loaded clean and then never respawned the crashed child again for the life of the daemon. Two
  siblings outside the graph refuse on the same principle: `send-break`/`pulse-dtr` reject an `ms`
  above `Daemon::MAX_SIGNAL_MS` (60000) before the port is resolved and before any line is asserted,
  and `serialnexusweb --tls` refuses a half-present cert/key pair at startup. **One thing that used
  to be refused and now loads:** a codec/exec multiplexed edge whose origin endpoint is
  `arbitration = "free-for-all"` may be `on-demand`. The rule exists because an `on-demand` origin
  into a held-origin pump parks on its first chunk forever, and that hazard presumes a lock upstream;
  a `free-for-all` endpoint has none, so `held` and `on-demand` are behaviourally identical there and
  the refusal was rejecting a graph that runs. Both edge rules now consult the one
  `endpoint_arbitrates` predicate, so the exemption cannot be applied to one and forgotten on the
  other again (§6, review 32 `CORE-2`).
- **Configurations that used to *bind* and now *wait*** — the sibling list, and the one behaviour
  change here that no `load` error announces. A `device = "usb:vid:pid:SERIAL:iface"` identity that
  **two present devices answer to** now binds nothing: `Resolver::find_usb` declines rather than
  returning the first match, so the node stays `waiting`. It used to bind whichever clone sorted
  first, which meant two nodes carrying that identity both drove one adapter while the other was
  unreachable by any node (review 32 `RES-1`). §15.10's ambiguity rule binds **resolution** as well
  as capture now, and `resolve_current_path` hands back a bare `Option<PathBuf>` — declining is the
  only way that signature can say "this identity pins no device", and "bind and warn" would be
  indistinguishable, at every open and every 1 Hz recheck, from binding the right one. The identity
  is still *accepted* (§12's asymmetry: identity-form input never requires the device present, which
  is why `dump` emits identities and cold starts work), and `add-node` echoes a `warning` naming
  every device that answers plus the by-path fix. This reaches an **upgrader with no action on their
  part**: a pre-remediation daemon captured and `dump`ed the ambiguous identity, so restarting the
  fixed daemon on an existing state file is enough to reproduce it — with no `warning` anywhere,
  because startup takes the resolution path, not the `add-node` one. The recovery is the by-path
  identity `ports` lists for each adapter, which does pin them one each.
- **One behaviour change to existing working configs:** `spchex` now maps picocom's
  control-byte class (DEL plus `0x00..=0x1f` except TAB/LF/CR) instead of SPACE, so a graph
  that enabled it emits different bytes — see the v11 track entry in
  `docs/implementation-notes.md`.

## 3. Workspace layout

Rust **edition 2024**, `resolver = "2"`, **MSRV `1.97`** (see §6 — the MSRV is load-bearing).
Cargo workspace; `fuzz/` and `examples/external-codec/` are deliberately **excluded**
(separate toolchain / built from a consumer's position).

| Crate | Kind | Role |
|-------|------|------|
| `codec-api` | lib | Dependency-free codec contract (§8): the multi-channel `Codec` trait, the `Event` vocabulary (data/open/close/error), the versioned **envelope** + daemon-to-daemon **wire frame** (`Hello`, `WIRE_MAGIC`) encode/decode. Has a feature-gated `test_support` conformance kit. |
| `codecs/reference` | lib | `codec-reference`: the reference framing codec over the v1 length-prefixed envelope; doubles as the first demux/re-mux codec and the link codec core; adds **length-guided resync** past corrupt frames (§7.5/§9). |
| `nexus-core` | lib | Pure foundation: `graph` model + the 3 topological rules and the `ValidationError` vocabulary, `data` (the §5 contracts as an executable **specification**, not the shipped path — invariant 9/notes §3.18), `lock` write-arbitration state machine, `config`/`state` split (`GraphConfig::validate` owns everything the topology model cannot see: per-kind attributes, numeric ranges, and the two edge rules that depend on node kinds), `map` (the picocom transform engine, §7.8), `resolver` (dependency-free `/dev`+sysfs device-identity resolution, §12). Property-tested; no I/O. |
| `nexus-rpc` | lib | Thin, stable JSON-RPC 2.0 framing (§10/§15.16): request/response/notification wire types over NDJSON, method params/results left as opaque `serde_json::Value`. Owns the single **`AppError` error-code registry** (§16.8) and dependency-free base64 for `tap.data`. |
| `nexus-sys` | lib | **The workspace's only crate with `unsafe`** (§16.3). Centralizes every ioctl / `ptsname` / nonblocking read-write / `poll(2)` wrapper: `read_icounts` (TIOCGICOUNT), `set_exclusive` (TIOCEXCL), `set_packet_mode` (TIOCPKT), `read_modem_bits` (TIOCMGET), `poll_ready`/`poll_blocking` (**deliberately not tokio `AsyncFd`**, §15.18). Every other crate `#![forbid(unsafe_code)]`. |
| `nexus-daemon` | lib | The daemon as an **embeddable library**: `run`/`RunOptions`/`Registry` entry surface. Wires boundary nodes, the single-thread tokio data-plane runtime, the JSON-RPC control plane, the persisted state file, and the compiled-in codec registry. Largest crate; see §4 for its modules. |
| `serialnexusd` | bin | Deliberately thin binary: parse flags, install tracing, call `nexus_daemon::run` with `Registry::with_builtins()`. All logic lives in `nexus-daemon`. |
| `serialnexusctl` | bin | Thin JSON-RPC client CLI. Subcommands → requests over the Unix socket; renders structured replies; `--json` is a raw pass-through of the daemon `result`. |
| `serialnexusweb` | bin | Standalone loopback HTTP+WebSocket console that is a **pure RPC client** of the daemon (the daemon gains no HTTP). Filtering JSON-RPC proxy; enforces per-session token + Host validation; **allowlisted verb bridge — observation + arbitration + graph editing; `load`/`teardown`/`shutdown` stay off the browser wire** (§17/§15.35, invariant 11). Hand-rolled HTTP on tokio; `tokio-tungstenite` WS; TLS via `rustls`+`rcgen` pinned to the **ring** backend (cert and key are one atomic pair — §2). Frontend assets are hand-written ES modules with **no bundler**, each served by `assets.rs` and unit-tested under `node --test` where it is pure: `history.mjs`/`saver.mjs`/`opfs.mjs` (the browser-side offset splice, the debounced writer, OPFS storage), `graph.mjs`/`editor.mjs` (the two §15.35 pages) and **`ansi.mjs`** (§17's minimal-ANSI-subset arm — a resumable state machine, because an escape sequence split across two `tap.data` notifications is the ordinary case at serial rates). |
| `nexus-sim` | bin | Deterministic **test double** (plan §3): PTY doubles, client drivers, in-process null-modem, TCP link-outage proxy, wire/envelope/exec conformance batteries. Emits one machine-readable JSON verdict line per run. Uses the daemon's own permissive PTY/socket calls. `publish = false`. |
| `nexus-doctor` | bin | Shipping **capability checker** (§15.17). Passive kernel probes P1 (EXTPROC/TIOCPKT), P2 (PTY POLLHUP presence), P4 (device identity resolution — the
`<sys>/class/tty` listing the resolver reads, with `/dev/serial/by-id` as a fast path *over* it, so
the probe answers for the environments §12 handles and not only for udev's), **P6** (pty-master readiness after last-slave close), **P7** (evidence a collapsed session leaves), **P8** (epoll-vs-`read` on a pty master — invariant 1's premise, probed with raw epoll, *never* `AsyncFd`), **P9** (poll timeout granularity), **P10** (pty buffer depth) + opt-in real-port P3 (serial fit), P5 (rig cert) and **P11** (TIOCGICOUNT/TIOCMGET). Markdown or `--json`. **Attach its output to any bug report.** P6–P11 exist to be **diffed between kernels** — every one emits raw numbers in `--json`, and a differing kernel is `degraded` with the observation named, never `unsupported` (which `linux.jq` and `meta_gates` both gate on). |
| `nexus-itest` | lib+tests | The **cross-platform integration harness** (§5), which replaced the bash `scripts/validate/**`. `src/lib.rs`: boots `serialnexusd` on a temp socket, an in-Rust JSON-RPC client (`Rpc`), a streaming `Subscription` (`subscribe`/`tap`), `nexus-sim` subprocess doubles, `serial_pair`/`serial_echo` (Linux sim) / `crossover_ports` (real HW) providers with self-skip, and `sha256_hex`. `tests/*.rs`: one file per former phase script. `publish = false`. |
| `serialnexusweb/ui-tests` | npm | **Not a crate.** The pinned Playwright suite (design §15.37): `@playwright/test` 1.62.0 + lockfile, Chromium only, dev-time only, nothing ships. Run *only* through `cargo test -p nexus-itest --test p8_web_ui`, which boots the fixture and passes the bootstrap URL; running `npx playwright test` by hand fails loudly by design. |

Dependency direction: `nexus-daemon` → {`nexus-core`, `nexus-rpc`, `nexus-sys`,
`codec-api`, `codec-reference`}; both client bins → {`nexus-rpc`, `nexus-core`};
`nexus-sim`/`nexus-doctor` → `nexus-sys` (+ `codec-api` / `nexus-core`).

### Key files inside `nexus-daemon/src/`
- `lib.rs` — public API (`run`/`RunOptions`/`Registry`); socket + state-file path policy; startup load.
- `daemon.rs` — graph state + all RPC verb impls; the two-lane control plane (§15.20). Largest file.
- `control.rs` — JSON-RPC 2.0 over NDJSON on the Unix socket; one task per connection; cancel-safe waiting. Its `select!` has four **`biased`** lanes — the in-flight verb, the request half, the `subscribe` broadcast, the tap channel — and the order is load-bearing: a waiting verb is an *arm*, never an inline `.await`, so it cannot black out its own connection's notification lanes (§2, CTRLW-1).
- `runtime.rs` — data-plane runtime: endpoint-keyed mpsc wiring, `poll(2)` readiness, the shared **`frame_ranges`/`frame_payload_cap`/`data_frames`** targetward-fragmentation helpers (§5/§15.19/§15.27, invariant 3) and the shared hostward **`fan_out`** (invariant 9).
- `boundary.rs` — shared boundary-supervisor primitives (park / race3 / `BlockingReader` / `Backoff`), property-tested (§16.1).
- `cell.rs` — `CriticalCell`, the `RefCell` wrapper that makes "a borrow never crosses `.await`" a compile-shape fact (§16.2).
- `registry.rs` — codec `Registry` value (`with_builtins`/`register`); **no dynamic loading** (§8/§15.26).
- `tap.rs` — connection-scoped taps + per-endpoint replay ring (§5/§6/§17).
- `nodes/` — `Node` enum + per-node runtimes: `serial`, `pty`, `log`, `codec`, `exec`, `leg`, `map`.
  (`map.rs` is the §7.8 character-map node; its pure transform engine is `nexus-core/src/map.rs`.)

## 4. Build / test / lint (exact commands)

```sh
# `cargo build --workspace` is NOT optional before the suite: the nexus-itest harness
# boots the plain `target/debug/{serialnexusd,nexus-sim,serialnexusweb,nexus-doctor}`
# artifacts, and only `cargo build` emits those — `cargo test` builds the
# test-instrumented bins under `deps/`, not the plain artifact — so `cargo test` alone
# on a clean tree fails every itest with "binary not found" (CI runs the build step
# first for exactly this reason; see .github/workflows/ci.yml).
cargo build --workspace --locked
# The one suite: unit/property tests + the nexus-itest integration harness (needs the
# binaries built above). The exec/envelope codec tests need python3, and the folded
# license-gate/external-codec/web-history tests shell out to cargo-deny/cargo/node and
# self-skip when the tool is absent.
cargo test  --workspace --locked
cargo fmt --all --check
# The `disallowed-types` RefCell ban lives in BOTH `serialnexusd/clippy.toml` and
# `nexus-daemon/clippy.toml` — clippy reads it from the crate manifest dir upward, and
# the two are siblings, so one copy covers only one crate (invariant 5).
cargo clippy --workspace --all-targets --locked -- -D warnings
# The minimal daemon (no built-in codecs) must ALSO be warning-clean:
cargo clippy -p serialnexusd -p nexus-daemon --no-default-features --locked -- -D warnings
# macOS portability gate (no Mac needed — it type-checks the cfg resolution). NOTE: the
# `ring` crate (serialnexusweb's TLS dep) cannot cross-build from Linux, so exclude it —
# the real macOS gate is `cargo test --workspace` on a Mac runner:
cargo check --target x86_64-apple-darwin --workspace --exclude serialnexusweb
# Licensing gate (permissive-only), proven not assumed. The second command is the folded
# gate that plants a banned crate and asserts cargo-deny rejects it *by name*
# (`error[banned]`). It has TWO preconditions and self-skips on either: cargo-deny absent,
# and a runner that cannot resolve the banned crate from the index (an air-gapped or
# throttled box — probed with `cargo metadata`, not by reading cargo-deny's prose).
# `SNX_LICENSE_GATE=required` turns either skip into a failure, the `SNX_WEB_UI` shape
# below — set it wherever the runner is expected to provide both.
cargo deny check licenses bans sources
cargo test -p nexus-itest --test p0_license_gate --locked
SNX_LICENSE_GATE=required cargo test -p nexus-itest --test p0_license_gate --locked
# The browser suite (design §15.37). Self-skips without node or the pinned Chromium;
# the CI job sets SNX_WEB_UI=required so a skip there is a failure. First-time setup:
#   (cd serialnexusweb/ui-tests && npm ci && npx playwright install --with-deps chromium)
cargo test -p nexus-itest --test p8_web_ui
SNX_UI_SLOW=1 cargo test -p nexus-itest --test p8_web_ui   # + the nightly @slow specs
SNX_UI_GREP='reload splices' cargo test -p nexus-itest --test p8_web_ui  # one spec
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
(`eprintln!("SKIP …"); return`) — the same skip-is-valid discipline the bash rig had. The
`p9_*` files are the regression guards for `docs/historical/26-claude-opus-code-review.md` — "phase 9"
by convention only, since the numbered phases ended at 8 — one file per defect area, each
module doc naming the finding IDs it pins and the reviewer's live reproduction it replays.
**Put a new review-driven guard there** rather than in a `p0`–`p8` file. A *feature* track
gets its own family instead, so the two never blur: the v12 graph-editing track is `p10_*`
(`p10_ports.rs`, `p10_edge_surgery.rs`) and the v13 browser-UI track is `p11_*`
(`p11_replace_atomicity.rs`) plus the gate `p8_web_ui.rs`. **`p12_*` is review 32's family** —
fifteen files on the same one-file-per-defect-area rule, each module doc naming the finding IDs it
pins. One of them is not a regression guard for a defect at all: `p12_sim_idle_cpu.rs` asserts a
`nexus-sim` double's **idle CPU**, which design §15.36's Decision and plan §3 rule 2 both required in
so many words ("pause rather than spin, and get their idle CPU asserted") and nothing implemented —
so the `NO_SLAVE_PAUSE` fix for a measured 74.4%-of-a-core busy spin could have been deleted in
silence, its only symptom renewed flakiness in unrelated tests. It primes each double's pts first (a
*never*-opened master does not HUP, so a test that skips that step measures nothing) and it must keep
its probe-relay assertion: a double that *exited* on the hangup burns zero CPU and would pass a
budget alone. The web console is the one deliberate exception —
its §15.35 graph/editor tests went into `p8_web.rs`, because that file already carries
several hundred lines of web-server harness (`WebServer`, a raw RFC 6455 client,
`wsclient_rpc`) and a fourth copy of it would cost more than a slightly over-full file.
Review 32 added a second exception for the opposite reason: `WIRER-3`'s guard is
`p6_fragmentation.rs`, in the *leg* family, because §15.24 has promised a leg guard by number since
v6 ("a 100 001-byte `send` round-trip as its regression guard") and never had one — the file
completes the family §15.24 names rather than parking a leg round-trip inside `p12_*`.

**A test must not assert byte-exactness at a boundary the design makes lossy.** A `pty` node's
hostward path is a bounded bridge whose overflow is **dropped and counted** (`pty.rs`'s
`sync_channel(hostward_buffer)` → `dropped_slow_consumer`), default depth 32 chunks, and design
§5/§15.19 sanction that loss: "a slow spy costs itself data, never its neighbors." A test that
streams ≥64 KiB through a console and asserts it arrives *complete* is therefore asserting more
than the design guarantees, and it flakes on a loaded runner — `received + dropped_slow_consumer`
equals `sent` to the byte, while the lossless sink (a `log`) stays byte-exact in the same run. Give
that console pty an explicit `hostward_buffer = 8192` and cite §5/§15.19 in a comment. Raising the
*serial* node's depth does **not** help: the pty pump drops rather than awaits, so it never
backpressures upstream and the pty's own depth is the only buffer in the path. The exception that
must not be "fixed": a node whose **loss is the subject** (a deliberately slow tap) keeps its
default — deepening it silently guts the test.

**Assert the property, not a platform's spelling of it — and derive a readiness wait from
what the syscall promises, not from what you can see.** Three of the four failures the
2026-07-28 macOS pass found (§2) were one mistake in three costumes: a guard that named a
Linux-shaped *proxy* for a portable property, on a daemon that satisfied the property
everywhere. `!POLLIN` on a hung-up pty master stood in for "no session evidence" — but the
product latches on a **read** (`Ok(n) if n >= 1`), and Darwin sets `POLLIN` unconditionally
there while answering `read` with 0. A *nonzero* `accepted_targetward` stood in for "the leg's
write half is parked mid-chunk" — but the counter credits per **completed frame**, so the
proxy silently required a socket buffer at least one frame wide, which macOS's 8 KiB is not.
In both cases the portable form is **stricter** on Linux, not looser, which is the tell that
you have found the real property rather than weakened the guard. The fourth is the same error
about *time*: `sock.exists()` stood in for "the leg accepts connections", and `bind(2)` creates
that file one syscall before `listen(2)` stops `connect(2)` answering ECONNREFUSED — so wait
on the operation you actually need to succeed, not on a side effect that precedes it. Ask what
the daemon *promises*; a quantity you measured on one kernel is a fixture, not a contract.

**A test that measures recovery after a leg outage must start from a known-quiet console.** A
reconnect releases *two* backlogs and purges only one. Purge-on-reconnect is §6's one sanctioned
**targetward** drain and `leg.rs` gates it on `faces == Facing::Host`; the outage-era **hostward**
backlog is untouched, crosses the restored link, and is written into whatever console is attached —
specified behaviour (§5/§7.4), not a defect, and not something to "fix" in the product. `p6_outage`
step 7 used to attach its round-trip client inside that ~20–30 ms window and read the flood as its
own echo (~1 failure in 10 unloaded runs). Gate on observables, never a sleep, and use two of them:
drain the console to quiet — only a reader can clear bytes already sitting in the pts buffer, no RPC
counter can see those — *and* require the receiving daemon's hostward accounting to stop moving,
which catches the converse case of a drain that finished before the flood ever started arriving.
**The generalisable half is in §8: when a flake is *suppressed* by CPU load rather than caused by it,
a green loaded re-run is evidence of nothing.**

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
- **A guard must be able to fail on the device it runs on.** A pts has no `break_ctl` —
  `TIOCSBRK`/`TIOCCBRK` succeed on it and change nothing observable — and `nexus-sim`'s null modem is
  a byte-copy loop that models no line condition at all, so *any* line-state assertion over
  `serial_echo()`/`serial_pair()` is vacuous — and reads to the next session as proof. That is how
  review 32's `SERX-2` remediation shipped a **permanently** stuck break past a green suite: its unit
  guard installed the successor on a second, different pts, which removes the shared-tty condition
  that is the entire mechanism, and measured fds where the defect was a line state. Line-state claims
  belong in a `crossover_ports()`-gated test that self-skips without a rig
  (`p12_serial_exclusivity::a_break_straddled_by_a_replace_leaves_the_line_transmitting`), and say in
  the doc which device the claim needs rather than implying a proof.
- **A sim verdict distinguishes a *short* stream from a *long* one; say which you mean.**
  `nexus-sim`'s `client` and `pty --sink` verdicts carry `timed_out` — the wall clock ended the run,
  which a `--drain` that expired mid-stream used to render as `pass: true` over the checksum of a
  truncated stream, byte-identical to a peer that genuinely lost bytes — and `overshoot`, bytes that
  arrived *beyond* the budget, i.e. contamination. `pass` now requires `overshoot == 0` beside the
  checksum. A test pinning "nothing foreign reached this console" asserts `overshoot == 0`; one
  pinning "nothing was lost" checks `timed_out` before believing a short count.
- **Meta-gates are proven, not assumed.** `tests/meta_gates.rs` scans the tree and asserts
  `unsafe` is confined to `nexus-sys/`, that the `RefCell` ban covers every crate holding
  daemon state (invariant 5), and that no `AsyncFd` appears in code anywhere (invariant 1); it
  also asserts `nexus-doctor` reports no `unsupported` capability. **Each scanning gate plants
  a synthetic violation first and proves its own detector fires** — and, for the two that must
  read past prose legitimately naming the banned token, that it does *not* trip on a comment. A
  gate whose detector silently stopped detecting is precisely the failure invariant 5 suffered.
  The licensing gate is
  `tests/p0_license_gate.rs` (folded from bash in v10 §16.11): it plants a banned crate and
  asserts cargo-deny rejects it, under the three-outcome discipline the next bullet
  describes. `p8_external_codec.rs`
  builds the out-of-tree template from a consumer's position, and `p8_web_history.rs` runs
  the browser history module's `node --test` (self-skip without node). **No bash remains.**
- **A gate whose precondition can fail needs three outcomes, not two** — pass, fail, and *could not
  evaluate* — and it must answer the third with its own mechanism rather than by matching a tool's
  prose. `p0_license_gate` is the worked example on both counts: it asserts cargo-deny's
  `error[banned]` diagnostic, because taking *any* non-zero exit as proof of a ban made a
  `cargo metadata` fetch failure pass the gate; and it decides evaluability with its own
  `cargo metadata` probe on the scratch crate, so an air-gapped or index-throttled runner **skips**
  rather than reporting that this tree has grown a banned dependency. Both skips honour
  `SNX_LICENSE_GATE=required` (§4) — the `SNX_WEB_UI` shape, optional on a laptop and mandatory
  wherever the runner is provisioned to answer.
- **`nexus-doctor` is a test *precondition* source, not only a CI check.** `p12_sim_idle_cpu` reads
  P2's `hup_after_close` out of `--json` and skips loudly when the kernel premise its measurement
  rests on is gone — without it the test measures a quiet fd and passes whether or not the fix it
  guards is present. Any test whose premise a probe already answers can do the same; the probe is
  this tree's authority on that question (§7, `expectations/linux.jq`), and re-deriving it in the
  harness only creates a second answer to keep in sync.
- **The browser half runs in a real browser** (§15.37). `p8_web_ui.rs` boots a daemon, sim
  doubles and `serialnexusweb`, then hands a pinned Playwright suite the bootstrap URL. Its
  three environmental prerequisites self-skip *with the command that fixes them*, and
  `SNX_WEB_UI=required` turns every skip into a failure — prefer that shape for any gate
  whose prerequisites are provisioned in CI but optional locally, because a gate that can
  skip silently is a gate CI passes over a hole. It also asserts a **floor on the spec
  count**, so a filter typo cannot shrink the suite quietly. Specs that cost real browser
  time are tagged `@slow` and excluded per push (`--grep-invert @slow`) — Playwright's
  spelling of `#[ignore]` + the nightly sweep. The suite is **22 specs**, two of them `@slow`
  (the forced tap shed and the tap grace interval, both of which are a minute of real time by
  construction — the interval *is* the subject, so shortening it would assert a timer the product
  does not ship). **The floor is a function of the fixture the gate built** (`SPECS_WITH_DEVICE`
  = 20 per push, `SPECS_DEVICE_FREE` = 10) and, with a device present, the gate additionally
  requires that **nothing skipped**. One constant used to do both jobs, sitting at the device-free
  count — so on `ubuntu-latest`, the only platform the `web-ui` job runs on and where the echo device
  is unconditional, any six specs could vanish under a rename or a `--grep` mistake and the gate
  stayed green while its own message promised it would trip. A floor sees arithmetic, never coverage:
  pair it with the skip assertion, and re-measure both numbers when you add a spec.
  **A spec that mutates the shared graph restores it in a `finally`** — see §8.
- **No bare sleeps.** Use `wait_until(Duration, || cond)` / `rpc.wait_status(…)` /
  `Subscription::wait_for(…)`. `Daemon`/`Sim`/`TempRun` clean up on `Drop` (kill children,
  remove the temp dir), so a panicking test never leaks a daemon or a socket.
- **Heavy/endurance tests are `#[ignore]`d** (e.g. `p8_soak::soak_endurance`, SOAK_*-env
  parameterized) and run in the nightly `--include-ignored` sweep, not per push.
- **Fuzzing is a separate toolchain and a separate answer to "which parsers".** `fuzz/` is
  excluded from the workspace and runs `fuzz-nightly`. Its targets were all on the `codec-api`
  layer — `envelope_decode`, `frame_decoder`, `wire_hello`, `reference_demux` — which left
  every parser reachable *without a leg* uncovered (review 26 SEC-7). Three more now close
  that: `rpc_request_line` (the daemon's front door — every byte written to the control
  socket), `rpc_base64` (the hand-rolled codec carrying console bytes inside `tap.data`), and
  `config_load` (`GraphConfig` deserialization + `validate`, the parser the two worst
  configuration defects walked through). SEC-7's last two — the daemon's `RequestLines` and
  `serialnexusweb`'s HTTP head parser — are now covered too, by `control_request_lines` and
  `web_http_head`. A new parser on an externally-reachable surface gets a target.
- **`unstable_fuzz_api` is a real exception to §15.26, with a rule attached.** Reaching those
  last two meant widening two crates: `nexus-daemon` and `serialnexusweb` each expose a
  `pub mod unstable_fuzz_api` re-exporting the parser (and `serialnexusweb` gained a library
  beside its binary). The design says "everything else stays private"; this is an amendment,
  not a drift — design §15.26 and `docs/implementation-notes.md` §3.19 carry the reasoning and
  the terms. **The rule that keeps it from eroding: an item re-exported there must have a
  target in `fuzz/` driving it.** The parent modules (`control`, `server`) stay private, so the
  named re-export is the only door, and each module's first doc line disclaims stability. If
  you are an embedder, do not use them; if you are adding to them, add the target first.

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
   for pty/tty — or anywhere: `meta_gates::no_asyncfd_is_used_anywhere_in_the_workspace`
   greps for it in *code* (the tree's only occurrences are prose explaining the ban), with an
   empty allowlist and a planted-violation self-proof.
2. **High-rate hostward paths run on dedicated blocking threads.** The serial reader and
   the PTY writer park in **blocking `poll(2)`** (`nexus-sys::poll_blocking`,
   `nexus-daemon/src/boundary.rs`): ~185 MiB/s, lossless, ~0 CPU idle. The async poll loop
   caps at ~1 MB/s — do **not** "simplify" the reader/writer back onto it (§15.19).
3. **Never silently drop targetward bytes: fragment, never skip on an encode error, and
   count any residual.** An oversize producer chunk is **fragmented** across frames via the
   one shared helper `runtime::frame_ranges` — never skipped. This skip-on-error bug shipped
   three times in three framers before review caught it (§5/§15.27). The in-process codec,
   `leg`, and `exec` all fragment on that one helper (the latter two through its envelope
   wrapper `data_frames`). The *third* clause is newer and was the quieter bug: `data_frames`
   used `map_while`, so a refused piece simply ended the iteration and truncated the chunk
   without a trace. It now yields a `DataFrame::Piece`/`DataFrame::Residual` enum a caller has
   to match, and **every writer charges the tail** — the codec to the channel's
   `discarded_targetward`, `leg` and `exec` to `discarded_unframable`. Guards:
   `targetward_oversize_chunk_is_fragmented_never_dropped`,
   `data_frames_reports_the_residual_instead_of_truncating_in_silence`.
4. **All `unsafe` lives in `nexus-sys`.** Every other crate is `#![forbid(unsafe_code)]`;
   `nexus-itest/tests/meta_gates.rs` (`unsafe_is_confined_to_nexus_sys`) proves the confinement.
5. **No `std::cell::RefCell` in the daemon — and the ban needs a `clippy.toml` in *every*
   crate it covers.** Daemon state lives in `nexus_daemon::cell::CriticalCell`, whose contents
   are reachable only inside a synchronous `with`/`with_mut` closure, so a borrow **cannot
   cross an `.await`** (§16.2). (`CriticalCell`'s own internal `RefCell` carries a localized
   `#[allow]`.) The `disallowed-types` entry now lives in **both** `serialnexusd/clippy.toml`
   **and** `nexus-daemon/clippy.toml`, deliberately duplicated. **The trap, which is the
   reusable lesson: a `clippy.toml` disarms silently when the code moves.** Clippy resolves it
   from `CARGO_MANIFEST_DIR` upward through *ancestors*; the file lived only in `serialnexusd/`,
   and the v8 library split (§15.26) moved every line of daemon state into the *sibling* crate
   `nexus-daemon/`, which is not a descendant. The ban stopped covering the code it was written
   for and nothing said so until review 26 (INV5-CLIPPY-SCOPE), which proved it with a planted
   `RefCell` plus a `len_zero` canary — only the canary was reported. If you move a
   crate, move its lint configuration with it. The durable half of the fix is the meta-gate
   `meta_gates::refcell_ban_covers_every_crate_that_holds_daemon_state`, which asserts the ban
   crates hold no raw `RefCell`, that each really carries a `clippy.toml`, and — the clause that
   would have caught the original break — that **every** crate whose sources use `CriticalCell`
   is on the ban list. Prefer that shape for any lint-enforced invariant: it survives a crate
   move, a `clippy.toml` does not.
6. **MSRV 1.97 is a two-way constraint.** The code uses **let-chains** (need ≥1.88) and
   clippy 0.1.97's `collapsible_if` *requires* collapsing nested `if { if let }` **into**
   let-chains. 1.85 and 1.97 clippy are mutually incompatible here — do **not** lower MSRV
   without `#[allow]` churn.
7. **Config vs state split.** Configuration is operator-owned, round-trippable, and only
   fails on *structural* invalidity; state is environment-owned and never persisted.
   Environmental failure (missing device, unwritable dir) changes a node's *state*, never
   the graph. A node name or channel identity may not contain `/` (`InvalidName`), may not be
   whitespace-only (`BlankName` — the reserved *empty* default-endpoint name stays legal, so
   the check is `!is_empty() && trim().is_empty()`), and may not exceed
   `graph::MAX_NAME_LEN` = 256 **bytes** (`NameTooLong`); a node name may not be empty
   (`EmptyName`). All are structural validation errors naming the offender (§3/§11/§12,
   `nexus-core/src/graph.rs`). The length cap is not cosmetic: a channel identity rides in
   every envelope frame header, so an oversize one shrinks the per-frame payload to nothing
   and makes invariant 3's residual reachable rather than pathological.
8. **Arbitration default is `exclusive`.** Only the write-lock holder's bytes are read
   targetward (non-holders are simply not read = backpressure, no drop). A lone PTY needs
   an explicit `lock` to write, or the node set to `arbitration = "free-for-all"`. The
   `send` verb self-acquires the lock. Do not weaken the gate to "fix" a test.
9. **The replay ring is bulk-memcpy, and default-on (64 KiB); the hostward fan-out is one
   helper.** Since v10 §15.32 every host-facing endpoint carries a `replay_ring` (default
   65536, opt out with `0`), so its hostward mirror + hub run on the hot path of *every*
   endpoint. `tap::ReplayRing` MUST stay a fixed circular `Vec<u8>` written with
   `copy_from_slice` — a byte-at-a-time `VecDeque` `drain`+`extend` starved the runtime thread
   and collapsed the 256 MiB firehose from 2.5 s to ~1.9 MB/s (measured, then fixed). Guard:
   `p3_firehose` completes well under its 60 s bound. Every producing node — serial, codec,
   exec, leg, map — now broadcasts through the single `runtime::fan_out(chunk, sinks,
   unattached)`, which charges the no-live-sink case (empty **or** all-`Closed` sinks) to the
   producer's unattached counter *inside the helper*, before the caller sees the result. That
   consolidation is not cosmetic: the loop was hand-rolled five times and only the serial copy
   counted it, which is exactly how the map shipped as the one hostward producer that never
   counted consumer absence (F1/DM-3). Mirror to the tap/ring **before** calling `fan_out` and
   pass it the graph sinks only: `discarded_unattached`/`discarded_no_client` accounting stays
   independent of the mirror (the ring is a spy *outside* the graph, §5) — guard
   `active_tap_feed_does_not_hide_unattached_loss`. **Delivery is reported by the helper, never
   inferred by its caller.** `fan_out` returns `FanOut::delivered` — the bytes at least one sink
   actually took — beside `FanOut::live`, which answers only "is anything still attached here?".
   Crediting delivery from `live` counted bytes a full sink never received; deriving it as
   `n - dropped_full` then under-counted, because `dropped_full` accumulates *per sink*, so a
   `[Ok, Full]` fan-out reported `dropped_full == n` and credited **zero** delivered for a chunk a
   live consumer had received in full (review 32 `LEGD-2`, whose first fix was the subtraction and
   whose second is this field). With several consumers a chunk is legitimately **both** delivered and
   discarded — the two counters measure different consumers — so "delivered + discarded partitions
   the stream" is a single-sink property, and only the chunk *no* sink accepted is unambiguously
   undelivered. Per-channel hostward routing credits in exactly one place,
   `runtime::route_channel_data`; guard
   `route_channel_data_credits_a_mixed_fan_out_to_the_consumer_that_took_it`.
10. **`tap.data` offsets are the *delivered-bytes* space, and loss beside it is signalled, not
   folded in.** Every `ingest` both pushes to the ring and advances `ingested`, so the ring
   holds `≤ ingested` bytes by construction and `from_offset = ingested − ring.len()` cannot
   underflow (`nexus-daemon/src/tap.rs`, §11.8). Do not stamp an offset *after* advancing
   `ingested`, or splice-exactness breaks. The other half of that contract used to disagree
   with it: bytes dropped at the lossy `TapFeed::mirror` hop left the offset space *contiguous
   across a real hole*, so a browser splicing by offset silently concatenated a holed stream.
   Folding those drops into `ingested` is the wrong fix — it makes `ingested − ring.len()` no
   longer the ring's true base offset, which is a silent corruption strictly worse than an
   invisible gap. The hole is instead reported as **`gap_before`** on the first chunk drained
   after it (from the shared `feed_dropped` atomic, against the hub's `feed_dropped_seen`
   watermark), with `tap.open`'s `feed_dropped` as the client's baseline; ring-replay pieces
   carry `gap_before: 0` because the ring is contiguous by construction. Review 32 sharpened three
   clauses of that, and all three are the difference between a number an operator can add up and one
   that double-counts or hides. The **baseline is the hub's already-charged `feed_dropped_seen`
   watermark, not the raw atomic** (TAP-4): loss recorded since the hub's last chunk is delivered to
   this tap as its first `gap_before`, so reporting the running counter at open counted the same
   bytes twice and `feed_dropped + Σgap_before` overshot the endpoint's true loss. The **same counter
   also carries the bytes an *inactive* feed never mirrored** (TAP-2) — no ring and no tap, which on
   a `replay_ring = 0` endpoint is the whole window between one `tap.close` and the next `tap.open` —
   because those were a third category invariant 10 has no room for: neither delivered nor signalled,
   so a client spliced across an arbitrary hole with no `gap_before`, no counter and no epoch change.
   Their position is *exact*, unlike an overflow's, the feed being empty by definition while nothing
   is listening. And a **replay too large for the connection's bounded tap channel is trimmed at its
   head, never its tail** (TAP-1), with the shortfall reported as `replay_truncated`: the newest
   bytes are the ones delivered, so `from_offset + replay_bytes` still lands exactly on the live edge
   and the splice stays exact. And a producer that takes `TapFeed::wanted()`'s permission to skip
   building the `Chunk` at all must charge those bytes to **`TapFeed::skipped`**: `wanted()` and
   `skipped()` are a pair. The one producer that takes the shortcut is `serial.rs`'s read-and-discard
   arm (`hostward.is_empty() && !tap_wanted`), and because it never reaches `mirror`, a ring-less dark
   window on a serial endpoint with no graph consumer landed on `discarded_unattached` and nowhere
   else — not delivered, so correctly absent from the offset space, but not on `feed_dropped` either,
   so absent from `gap_before` too, and a returning client spliced across the whole window with no
   marker, no counter and no epoch change (the half of TAP-2 the first fix documented and did not
   write). Both counters are right at once: the bytes are unattached *and* they are a gap in the
   feed. Guard `p12_tap_replay.rs`. `info.instance` is a
   per-boot nonce so a client detects the offset reset across a restart; it does **not** rotate
   on a hub rebuild (`load --replace`), and it is not supposed to — **`tap.open` reports a
   per-endpoint-hub `epoch`** for that (§15.38), unique within a process and never reused. A
   client persists the epoch beside its stored offset and re-anchors exactly when it changes.
   **It must not infer a reset from `from_offset < frontier`**: that is *also* what an ordinary
   reload with a replay ring looks like, and the heuristic duplicated stored scrollback once per
   reload until the browser suite caught it. The epoch closed the old "`instance` does not
   rotate" open issue — do not rebuild the heuristic.
11. **The web bridge is an allowlist, and it screens a *parsed* value and forwards that value
   re-serialized.** Both halves are settled by a reproduced bypass (review 26 WEB-1/SEC-1).
   The daemon's control socket is NDJSON, so one WebSocket frame carrying
   `{…"method":"info"}\n{…"method":"teardown"}` used to split into two requests on the far
   side of which only the first was screened — `teardown` and `shutdown` both executed from
   the browser. Re-serializing one parsed object emits exactly one line, so what was screened
   is exactly what is sent. And `bridge::ALLOWED` enumerates the verbs the console may invoke
   rather than the ones it may not: a denylist fails **open** on every verb §10 grows
   afterwards, which made the boundary depend on someone remembering to extend it. Do
   not invert either half, and do not forward raw frame text. **v12 widened the list**
   (§15.35): `add-node`, `remove-node`, `connect`, `disconnect` and `ports` are now
   forwarded — a token holder already commands every configured console, so withholding
   graph edits protected little. Read that as the allowlist working, not eroding: widening
   it is a deliberate act with a design section behind it, which is exactly what an
   inverted list would have given away. **`load`, `teardown` and `shutdown` stay off the
   browser wire**, pinned by `bridge::the_allowlist_admits_graph_editing_and_no_lifecycle_verb`
   and end to end by `p8_web::the_editor_verbs_pass_the_bridge_and_lifecycle_verbs_still_do_not`.
   **The console's other boundary — admission — is bounded by *eviction*, never by refusing an
   accept.** `MAX_CONNECTIONS` (128) is taken at the **token gate**, so over the cap an
   *authenticated* request gets a 503; the population that has not shown a cookie yet is capped
   separately (`MAX_PRE_AUTH_CONNECTIONS`, 32) by evicting its oldest member, and registration never
   fails. Taking that permit at `accept` instead is what the audit reproduced against the shipped
   binary: the cookie cannot be read before the head is, so a gate closed on unauthenticated peers is
   closed on the operator too — 32 silent sockets denied every new connection, valid session cookie
   included, on `/app.js` and `/ws` alike, which made the lockout four times cheaper than the
   128-socket flood it replaced. **A reserve you cannot classify into at admission time is not a
   reserve**; the eviction form bounds how *long* an unauthenticated peer may sit rather than
   *whether* it may connect, which leaves the newest arrival — always the operator's browser, and the
   one with no reconnect path but a reload — the last candidate for eviction instead of the
   structural victim. Guards: `server::the_pre_auth_population_is_a_fraction_of_the_connection_pool`
   and `p12_web_session::a_pre_authentication_flood_cannot_deny_an_authenticated_client_a_new_connection`.
12. **Write-mode promotion has exactly one implementation.** `GraphConfig::effective_write_mode`
   (`nexus-core/src/config.rs`) is the single source of truth for the two
   configuration-to-runtime promotions — a log target forced to `never`, and a map's `raw`
   endpoint promoted from `on-demand` to `held` (§7.8) — and **both** the validator
   (`GraphConfig::validate`, for the one-`held`-origin-per-endpoint rule) and the data plane
   (`runtime::Wiring::build`) call it — and, since v12, `Daemon::connect`, which registers a
   live edge's origin with the same effective mode a loaded one would get. Re-deriving a
   promotion in any of the three is how the
   validator and the runtime come to disagree about what a graph actually does: the reachable
   shape is two maps on one upstream endpoint with `write_mode` written nowhere, both silently
   promoted to `held`, one of them starved forever and invisible in `state` (§16, review 26
   RV-4). The declared value is what `dump` round-trips; the effective value is what runs
   (notes §3.17).
13. **Numeric configuration fields are range-validated *structurally*, in
   `GraphConfig::validate`.** A bad number must be refused before anything is created and —
   under `load --replace`, which composes teardown-then-load — before the running graph is torn
   down (§11 atomicity, §15.26). This is not tidiness: `replay_ring` allocates lazily, so an
   unbounded value **loaded cleanly and then aborted the process** out of the allocator on the
   first hostward byte, on a configuration `load` had already persisted — a crash loop across
   restarts; and an unbounded `hostward_buffer` panicked inside tokio's bounded channel *after*
   `--replace` had already destroyed the good graph. The caps live beside the fields they bound
   (`MAX_REPLAY_RING` 16 MiB, `MAX_HOSTWARD_BUFFER` 65536 chunks, `MAX_TIMER_MS` one hour,
   `MAX_ROTATION_PADDING` 20 digits), every check goes through the one `range_error` helper,
   and a proptest sweeps extreme values. A new numeric knob gets a range here on the day it is
   added.

14. **Edge surgery mutates *shared* wiring; it never restarts a node.** `connect` and
   `disconnect` change a running graph through three long-lived structures, and the reason
   each exists is a bug it prevents (§15.35, `nexus-daemon/src/runtime.rs`). A host-facing
   endpoint's fan-out is a `FanOutList` (an `Arc<Mutex<Vec<AttachedSink>>>`, shared with the
   serial reader's blocking thread — invariant 2 — at a cost of one uncontended lock per
   64 KiB chunk). A target-facing endpoint owns an `EdgeInbox`, a stream of hostward
   receivers its pump loops over, so the pump outlives every individual edge. And its
   `EdgeSlot` carries the targetward sender + lock, re-read per chunk. **Do not "simplify"
   any of this into restarting the node's tasks:** aborting a task drops the *targetward*
   receiver out from under senders that stay live in `GraphState::endpoint_targetward` and in
   every writer origin, which is MAP-1's chain — a pty origin's next write fails, its reader
   ends, and presence latching, last-close handling and detach-release go with it. Three
   states, three behaviours, and the middle one is the subtle one: attached-and-writable
   forwards; **attached but read-only** (`write_mode = "never"`) drains-and-counts, because
   parking would wedge a writer forever on a configuration that will never become writable;
   **not attached** parks (`runtime::await_origin`), because targetward is the direction §5
   forbids dropping on — a detached edge must stall its writers exactly as a steal does.
   Guards: `p10_edge_surgery.rs`, and `p9_unwired_interior.rs` still pins the read-only arm.

15. **Every tty-level assertion is owned by the port, and given back at most once.** Exclusivity was
   only the first of them. Each is a claim a serial node made on a *tty* it shares with whoever
   opens the device next, so the node gives it back when it stops — but "when it stops" is several
   places, and writing the release at one of them is how this was wrong twice. `ExclusivePort`
   (`nexus-daemon/src/nodes/serial.rs`) wraps the fd with a `Cell<bool>` claim flag; `release_port`
   is the **one ordered discard** every path takes (`teardown`, `set_waiting`, `fault`), going
   through `SerialShared::set_port`, the single point a node's port changes; and `Drop` is the
   backstop for the paths no successor can reach (an error inside `open_port`, a port never stored).
   The flag is what makes the two agree: an ordered release must **not** be undone by the late `Drop`
   of a lingering `Rc` clone, because `TIOCEXCL` is *tty* state and stripping it then strips it off
   the **successor's** port. The failure that settled all of this is not a flap but a brick: the flag
   clears only at the tty's last close, which a pts whose master is held open (`nexus-sim pty`, socat
   `PTY,link=`, QEMU `-serial pty`) never reaches — so an ordinary `[node.modem] dtr = true`, which
   is `ENOTTY` on such a device and dropped the port through `open_port`'s error arm, left every
   unprivileged process on the machine locked out of it permanently, surviving `teardown` and daemon
   exit, while the node's reported reason flipped from the true cause to a self-inflicted `Device or
   resource busy` that sends the operator hunting a squatter that does not exist. The same ownership
   settles the signal verbs: `send-break`/`pulse-dtr` hold a `SignalHandle` carrying **no** strong
   port reference and re-check a generation through the one `with_scoped_port` predicate the verb and
   its `RestoreGuard` share, so a signal acts on the port it was issued against and declines with
   `node was removed while signalling` rather than following the node — reproduced on real hardware
   as `pulse-dtr --ms 12000` flipping DTR on a `load --replace` successor's line twelve seconds
   later, with a break half that needs no race at all because break is tty state.
   **And that is exactly why the claim, not only the restore, has to be given back.** Scoping the
   `RestoreGuard` alone was the regression the audit caught on the bench rig: it made the guard
   decline once the generation moved, which removed the only `TIOCCBRK` in the tree and converted an
   `ms`-bounded break into an unbounded one — a `send-break` straddled by `load --replace` left the
   successor reporting `active`/`open: true`, `send` reporting bytes accepted and `driver_counters.tx`
   climbing, with nothing on the wire, indefinitely, recoverable only by a `teardown` that destroys
   the tty or by another `send-break`. So `ExclusivePort::release_claims` — the method the whole type
   exists for — issues `TIOCCBRK` and *then* drops `TIOCEXCL`, in that order, so the line is
   transmitting normally before anyone else can open it, and under the same at-most-once flag because
   clearing a break the *successor* just asserted is the same bug in the other direction. DTR and RTS
   are deliberately **not** released: driving them on the way out is a reset pulse on every
   auto-reset board (§7.1), the unrequested edge this whole area exists to prevent, and unlike break
   they self-heal — the successor's own `open(2)` re-raises DTR and `open_port` reapplies the
   configured `[node.modem]` levels.
   **The same ownership has a second site, and its rule is "publish before you await" (§15.38 D2).**
   `serial.rs::reopen` adopts the reopened port into `SerialShared` *before* `purge_on_reconnect`, so
   `release_port` can see it; the reconnect's single generation bump moved from `set_active` to
   `adopt_port` with it, and the function re-checks the generation after the purge rather than
   arming a reader on a port it no longer owns. The order the code does not use — open, purge, arm,
   publish — leaves the node holding `TIOCEXCL` on a device the ordered discard cannot reach for as
   long as the purge yields, and a `load --replace` landing there releases nothing while the aborted
   supervisor's future holds the only `Rc<ExclusivePort>` until the `LocalSet` regains the thread: the
   successor's `open(2)` EBUSYs against the daemon's own claim. Two consequences are accepted rather
   than worked around, and both are more honest than the alternative: the node reports `waiting` with
   `open: true` for the width of the purge — the daemon really does hold the device, and a third
   party's `open(2)` already says so — and a signal verb is accepted in that window, where the
   generation bump keeps it valid into `active` instead of orphaning it there.
   Guards: `p12_serial_exclusivity.rs` — including
   `a_break_straddled_by_a_replace_leaves_the_line_transmitting`, which needs a `crossover_ports()`
   rig and self-skips without one, because a pts cannot observe a break at all (§5) — and
   `p11_replace_atomicity.rs`.

16. **A pty reader that cannot hand its chunk on *holds* it — registered on the endpoint, and never
   across a session boundary.** The reader task's lifecycle half — the presence swap,
   `handle_last_close` (§7.2's baseline reset, §6's detach-release and purge-on-detach) and the
   `RECONCILE_INTERVAL` backstop — must run within a bounded time whatever the targetward channel is
   doing, which is why it no longer sits *after* an unbounded `tx.send().await`: a console wired to a
   serial node `waiting` for an absent device reported `client_present: true` forever after its client
   exited and kept an on-demand write lock held by an origin that was gone (review 32 `CONC-1`). A
   payload the endpoint cannot take is parked in `pending` instead and the master goes unread, so the
   client backpressures through the kernel buffer — §5 forbids *dropping* targetward, not delaying it.
   **Three rules keep that hold from becoming a hiding place, and each is a defect the first version
   of the fix shipped** (`nexus-daemon/src/nodes/pty.rs::read_and_poll`). (1) *The hold is registered
   on the endpoint, not on a timer.* While a payload is held the loop's one await is
   `timeout(wait, tx.reserve())`, so the task is a producer "suspended mid-send" in the precise sense
   §6's purge uses and `boundary::drain_to_quiescence` still reaches it. Parked on a bare `sleep` the
   payload sat *outside* the pipeline, and the operator's pre-outage console input fired into the
   device's boot prompt the moment it came back while `purged_on_reconnect` reported success —
   measured at 5634 bytes on the audit's rig. (2) *A held payload never crosses a session boundary.*
   The close block offers it once more and then purges it as §6's detach instance. The retry at the
   top of a pass deliberately does not consult the lock, so a carried chunk would be written by an
   origin that no longer holds the floor — after detach-release, after the next holder's
   purge-on-acquire, into the middle of the line that holder had just queued. (3) *Lifecycle
   observation does not depend on the data slot being empty.* The close trigger is
   `!present_now && (was || saw_session)`; re-adding the `closed` conjunct re-opens the
   collapsed-session leak, because it silently required the drain to *reach* EOF/EIO and the drain
   ends early whenever the endpoint refuses a payload — a session that opened and closed inside one
   poll gap against a saturated endpoint then leaked its lock for as long as the saturation lasted
   (the audit measured five of five, with another origin's `send` failing on the locked endpoint
   afterwards). Guard: `p12_pty_setup.rs`, five tests.

For the deeper code-level invariants (purge-on-acquire runs synchronously at grant time;
the exec pump polls stdin/stdout/stderr concurrently to avoid deadlock; serial
faulted-and-wait parks receivers unread rather than draining; etc.) see
`docs/implementation-notes.md` (§3.x deviations, §6a–§6f per-phase writeups) — it is the
running engineering log and the authoritative "why the code looks like this" record.

## 7. Platform & kernel constraints

- **Linux is required** and is the kernel of record. **Production target is Linux 6.18;
  the dev box runs 7.0.** You can run code on 6.18 (the user can; an agent here cannot).
  `nexus-doctor` has been run on 6.18 **twice**: P1–P4 on 2026-07-19 (`e93149d`), and **all eleven
  probes on 2026-07-27**, both on `6.18.14-1rodete4-amd64` (Debian rodete). Everything reported
  `supported` (19 · 0 · 0 · 0). **P6 and P7 came back byte-identical to 7.0** — including P6's
  `handler_reset_readable_bytes: 1` and P7's `latch_covers_termios_only_session: true` — P8 agrees on
  every semantic field, P1/P2/P3's booleans are identical, P9's 1/5/10 ms floor agrees within
  8–17 µs, and P10 lands inside the band the probe declares for 7.0 against *itself*. So the two
  probes whose own output says "diff this block before simplifying anything" are answered, and the
  answer is that **nothing may be simplified**: P6 confirms the last-close drain load-bearing on the
  production kernel rather than removable, and the `saw_session` latch is barred by invariant 16
  rule (3) — a write-lock leak measured five of five — which no probe speaks to in either direction.
  `docs/nexus-doctor.md`'s 6.18 section carries the numbers and what the run still does **not** cover:
  it used a **`fe1c52c`-vintage binary**, so HEAD's P4 and the `environment()` by-id arm rewritten
  beside it (review 32, `RES-2`) have no 6.18 evidence; that box is **Tier 1** — one dangling adapter —
  so P5's paired rate ladder, its deliberate baud mismatch and every `crossover_ports()`-gated test
  never ran there, and `brk = 0` means a break has never been *observed* on 6.18; only Markdown was
  captured, so the re-gate command below has never been *executed* there — its content satisfied every
  clause of `linux.jq` **as that file stood at the `fe1c52c` vintage**, and no longer would, the
  2026-07-28 provenance work having added `.build.*` clauses a binary of that vintage cannot answer;
  and **`cargo test --workspace` has never run on 6.18 at all** — CI is
  `ubuntu-latest` + `macos-latest`, so the production kernel's evidence base is eleven probes and zero
  executed tests. **Pause and check with the user before any one-way (hard-to-reverse) decision that
  depends on a kernel ability confirmed only on 7.0** — the rule is a predicate over a set that
  shrinks as probes get answered, not a fixed list — and keep the design's fallbacks live (the §7.2
  termios reconciliation-poll backstop; P2 slave-priming for presence, which 6.18's
  `hup_when_never_opened: false` makes *mandatory* there rather than droppable). Re-gate on 6.18 with
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
  - **A collapsed termios-only pty session used to leak its write lock on macOS; it is
    fixed by an *edge* latch, and the reasoning is design §15.39 — read it before touching
    `read_and_poll`.** A client that opens the pts, calls `tcsetattr` and closes **inside
    one 5 ms reader poll gap** — a scripted probe, a health check, a bare `stty` — used to
    leave `usb0.lock.holder` set forever while `client_present` read `false`, with another
    origin's `send` failing `-32003 … is locked` (20 of 20, past 30 s). Cause: XNU's
    `ptsclose` → `ttyclose` flushes both tty queues at the slave's last close, destroying
    the packet `saw_session` arms on (invariant 16); `was` is false because no poll landed
    during the ~53 µs session; and every *level*-triggered observable — poll revents,
    `FIONREAD`, `TIOCOUTQ`, `TIOCGPGRP`, `TIOCMGET`, `TIOCGWINSZ`, the pts inode's
    timestamps — is byte-identical to no session at all. **Level state cannot carry an
    edge**, which is why looking harder at those was never going to work:
    `nexus_sys::SessionLatch` (a `kqueue` `EVFILT_READ | EV_CLEAR` knote on Darwin, inert
    elsewhere) now supplies the boundary and `p9_pty_collapse` runs unskipped on both
    platforms. Four things bind a future editor. (1) **Do not widen the predicate instead**:
    an ungated `|| closed` arm fires on *every* pass here (a Darwin master with no slave
    reports `POLLIN|POLLHUP` and `read → 0` forever — doctor P6, 64/64), releasing a lock
    an operator took with no client attached. (2) **The latch must never mark the pass
    productive** — an edge is not data, so `did` stays unset and the idle backoff is
    unaffected; measured cost 1.62% → 1.75% of a core idle. (3) **The daemon forges these
    edges itself** — the baseline re-assert, the last-close flush, the reconciliation
    backstop — so `watch` swallows its own registration edge and the close block discards
    after running; delete either and the handler re-fires on its own footsteps, which
    `collapsed_client_sessions_still_release_the_write_lock` catches. (4) **Invariant 1 is
    intact**: its ban is on `AsyncFd`/epoll as a *readiness* source, and readiness is still
    `poll(2)` alone. `nexus-doctor` **P7** measures the packet mechanism and **P12** the
    edge one, so a reader can always tell which carries detach-release on the kernel in
    front of them. Treat P7's sibling `latch_covers_data_session` with care — it reads
    `false` on Darwin for a shape the daemon demonstrably covers, because the probe's
    harness has no master reader and BSD `ttywait` then blocks the close ~600 ms, where
    under the live daemon the same close takes ~1 ms and the read arms the latch.
  - **Doctor P1 on macOS is `degraded`, and the mechanism is now measured** (2026-07-28,
    15.7.8): a client `tcsetattr` *does* produce a packet, but Darwin's leading byte is
    `0x20` (`TIOCPKT_DOSTOP`), not `0x40` (`TIOCPKT_IOCTL`), so `read_and_poll`'s
    `buf[0] & sys::TIOCPKT_IOCTL != 0` arm never matches and termios reconciliation runs
    entirely off the `RECONCILE_INTERVAL` backstop — the §7.2 fallback the design keeps
    live for exactly this. Only client-termios *latency* degrades. Relatedly,
    `discarded_at_last_close` is structurally always **0** on macOS: the kernel destroys
    the pts's undelivered hostward queue at last close before the daemon can count it, so
    §7.2's guarantee holds there for free and the counter naming the discard has nothing
    to name.
  - **The AF_UNIX socket buffer is 8192 bytes on macOS** (`net.local.stream.sendspace`)
    against Linux's ~208 KiB — *narrower than one 64 KiB wire frame*. Nothing in the
    product depends on the size (a full buffer is backpressure, which §5 sanctions
    targetward), but a test that derives a predicate from what Linux happens to buffer
    will hang here; see §5's rule and `docs/macos.md` delta 4.
  - **The macOS local cross-check must exclude `serialnexusweb`:** `cargo check --target
    x86_64-apple-darwin --workspace --exclude serialnexusweb` — the `ring` crate (its TLS dep)
    cannot cross-build from Linux. The real macOS gate is `cargo test --workspace` *on the Mac*,
    where `ring` builds natively and this exclusion is unnecessary.
  - **Serial-*device* itest tests self-skip on macOS** (a pts can't be a serial device there —
    `serial2` → `ENOTTY`); the real macOS serial path is `serial_hardware.rs` via
    `crossover_ports()` (`/dev/cu.usbserial-*`, or `SNX_CROSSOVER_A`/`_B`), self-skipping with
    no rig. `p8_web_history.rs` runs the browser history module under `node --test` and
    self-skips without `node`.
- **`TIOCEXCL` on a pts outlives the fd that set it**, because the flag lives on the tty
  and clears only at the tty's *last* close — which a pts whose master is held open (every
  `nexus-sim pty` double, socat `PTY,link=`, QEMU `-serial pty`) never reaches. So a
  reopen of the same pts by anyone, daemon included, is `EBUSY` forever. The daemon now
  releases exclusivity in `ExclusivePort`, which every discard path goes through — the ordered
  half is `release_port` (`teardown`, `set_waiting`, `fault`), `Drop` is the backstop, and the
  claim is given back at most once (invariant 15). That repairs it; if you see a
  permanently `faulted` serial node with `Device or resource busy` over a sim device,
  check that release before blaming the sim.
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
- **Before concluding anything from a failing test on the dev box, check the box.** `uptime` and
  `pgrep -x yes`. Reproducing a load-sensitive flake usually means spawning CPU hogs, and hogs get
  leaked: 16 stray `yes` processes once held this 8-core laptop at load ~19 for ten hours, which made
  `p3_firehose`, `p5_resync`, `p8_tap_drops` and `p5_exec_crash` fail and cost a round of false
  "did we regress?" investigation. Killed, `p3_firehose` ran in **2.53 s** — the healthy figure
  invariant 9 records — and the suite went green. **Clean up every hog you start**, and prefer
  bounding them with `timeout`. This is a shared machine the user is working on.
- **…but load does not only *reveal* races; it also hides them, and when it does, the advice above is
  exactly wrong.** `p6_outage`'s step-7 flake (§5) is the counter-example that cost a round of
  investigation: the failure needs the round-trip client to attach inside a ~20–30 ms flood window,
  and CPU load widens client-spawn latency *past* it — so every loaded re-run came back green and not
  one of them was evidence of anything. What settled "not ours" was a throwaway `git worktree` at the
  pre-remediation commit reproducing the same mechanism, with hit rates on the sensitive probe
  statistically indistinguishable (1 in 22 there, 1 in 20 here). If a hypothesis predicts that load
  makes a failure *more* likely and the loaded runs are green, consider that you have suppressed it:
  reproduce the window deterministically rather than turning the dial harder.
- **A pts hands out at most 4095 bytes per read** (`N_TTY_BUF_SIZE - 1` — a property of the line
  discipline, not a daemon or sim constant, and you will meet it again). So "exactly 2×" in a
  pty-backed verdict is much more likely read geometry than duplication: `received: 8190` against
  `sent: 4096` read as a doubled stream for a while and was 4095 + 4095 — a *contaminated* stream,
  counted by an accumulator that appended whole reads past its own budget. Suspect the oracle's
  counting rule before the data plane, and read `overshoot` (§5).
- **A Playwright spec that mutates the shared graph must restore it in a `finally`.** The browser
  suite's fixture is one daemon and one graph for the whole run, so a spec that leaves an edge behind
  changes what a *later* spec measures — and the symptom is a test that fails in the suite and passes
  in isolation, which reads as flakiness and gets chased as one. `graph-editor.spec.mjs` built a log
  node onto the echo console and left it attached; a subsequent spec used that node's
  `discarded_unattached` as its "did the device emit?" oracle and the counter could never move again,
  because the bytes now had somewhere to go. The restore is one `ctl("remove-node", name,
  "--cascade")` in a `finally` — cascade so the edge goes with the node — and it belongs there
  whether or not the assertions above it passed.
- **`git stash` is as destructive as `git checkout --` when the tree holds a large uncommitted
  change set**, which is the normal state mid-track here. To answer "is this failure ours?", add a
  throwaway `git worktree` at the last commit, build there, and compare — it touches nothing.
- **`.claude/settings.json` is tracked, and its permission allowlist is deliberately narrow — the
  omissions are the point.** It pre-approves the build/test/lint loop (`cargo build|test|check|clippy|
  fmt|deny`), read-only git (`status`/`diff`/`log`/`show`/`ls-files`/`blame`) and read-only shell
  (`rg`, `grep`, `sed -n`, `jq`, `ls`, `wc`, `head`, `tail`, `pgrep`). It does **not** carry
  `Bash(git *)` or `Bash(cargo *)`, and must not grow them: a blanket `git` rule re-enables
  `checkout --`, `stash`, `reset --hard` and `push` — three of which are the hazards listed
  immediately above — and a blanket `cargo` rule covers `cargo run`/`install`. `pkill` is absent for
  the §8 reason at the top of this list, and `find` for `-delete`/`-exec rm`. `nexus-doctor` is
  allowed only in its **passive** forms, so a `--port` run still prompts: opening a port toggles DTR
  on equipment that may be live, which is the same doctrine that makes those probes opt-in (§7.1,
  design §15.17). `.claude/scheduled_tasks.lock` and `settings.local.json` are gitignored — machine
  state and personal overrides, respectively.

## 9. How work has been done here (the working rhythm)

- **Design/plan pairs are version-suffixed and monotonic.** The newest pair lives in
  `docs/` (currently **v13**: `30-design-claude-fable-v13.md` +
  `31-implementation-plan-claude-fable-v13.md`); superseded generations move to
  `docs/historical/`. `§N` always means the *current* normative design. **This parenthetical
  and `README.md`'s documentation index are the two places that name the pair by filename, so
  both must be bumped in the commit that lands a generation** — they have gone stale on three
  consecutive generations now (review 19 DOC-5, review 26 DOC-5, review 32 DOCR-1/DOCR-5), and
  `meta_gates` now covers both halves of that: `entry_point_doc_links_resolve` asserts every relative
  doc *link* in `README.md` and `AGENTS.md` resolves, and `entry_point_design_and_plan_names_resolve`
  asserts every **backticked** design/plan filename in those two files names a file that exists —
  which is what this parenthetical is, prose rather than a link, and exactly the shape a link checker
  could not see. ADRs are numbered subsections under design **§15.x** (plus §16
  post-completion review, §17 web console). The RPC method-by-method reference is
  `docs/rpc/` (README + one page per verb group); design §10 is its normative source.
- **Every phase/track has ended with a multi-agent adversarial audit** (per-area finders +
  independent verifiers; each finding verified before it's accepted, then fixed by aligning
  code to design). This is the expected bar for substantial changes — find, verify, fix,
  add a regression guard. **Two rules the audits themselves cost us:** a verifier gets the
  finding and the tree, *never* the report (review 26, §15.34) — and the tree must not move
  under the verifier (the v12 audit, which fixed findings while the skeptics read and got 35
  of 43 back as "not real" for defects that were real when filed). Freeze the worktree for
  the verification pass, or pin the verifiers to the commit the finders read. When in doubt,
  the thing that actually settles a finding is a test that fails without the fix. `docs/implementation-notes.md` records the confirmed/refuted
  counts per phase.
- **Commit discipline:** work happens on `implementation`; the user reviews before commit
  and before any `main` merge. Do not push or merge to `main` without being asked. Commit
  messages here are section-scoped (e.g. "v9 §11.3-6: the web console client").
- **Before asserting any file:line as fact, re-read it** — much of the surrounding
  knowledge was captured point-in-time and the code moves.
