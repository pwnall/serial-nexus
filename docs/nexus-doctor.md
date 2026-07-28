# nexus-doctor — capability checker

`nexus-doctor` is the shipping diagnostic for serial_nexus (design §15.17). It
consolidates every kernel-behavior probe the design depends on, plus environment
checks, into one binary that emits a copy-pasteable **Markdown** report (the
expected first attachment on any support request) with a **`--json`** twin for
CI. It supersedes the throwaway per-spike binaries of the v1 plan.

```
nexus-doctor              # Markdown report on stdout (default)
nexus-doctor --json       # JSON twin for CI: nexus-doctor --json | jq -e -f expectations/linux.jq
nexus-doctor --port /dev/ttyUSB0   # opt a real port into P3 (serial fit), P5 (rig) and P11 (counters)
nexus-doctor --port /dev/ttyUSB0 --port /dev/ttyUSB1   # repeatable: P5 classifies the set
nexus-doctor --dev-root ./fixtures # fixture /dev + /sys tree (test seam, §3; sys_root is <dev-root>/sys)
```

**The daemon never consumes this output.** Its degradation paths (e.g. §7.2's
reconciliation poll) are unconditional, so a wrong probe can mislead a developer
but never the data plane. Probes are **passive by default**: any probe that
opens a real serial port requires that port to be named with `--port`, because a
listed port could be wired to live equipment.

## Probes

| ID | What it checks | Verdict → design consequence |
|----|----------------|------------------------------|
| **P1** | EXTPROC/TIOCPKT: does a client `tcsetattr` surface as a `TIOCPKT_IOCTL` packet; does clearing EXTPROC emit a final packet; can the master re-assert EXTPROC? (§7.2, §15.14) | `supported` → packet-mode observation is primary. `degraded` → §7.2 runs poll-only; only observation latency degrades. |
| **P2** | PTY presence: POLLHUP only when no client holds the slave; HUP clears on reopen; termios settable with no slave open. (§7.2) | `supported` → presence-gated output works. `unsupported` → no fallback; stop condition. |
| **P3** | Serial fit: custom baud, `TIOCEXCL` exclusivity, modem-line set/read, break, `TIOCGICOUNT`. (§7.1, §13) | `supported`/`degraded` (apply missing control via the `sys` module) / `skipped` (no `--port`). |
| **P4** | device identity resolution: does the resolver's one source — the `<sys>/class/tty` listing plus a dependency-free sysfs walk, with `/dev/serial/by-id` as a fast path over it — yield `usb:vid:pid:serial:iface`? (§12) | `supported` / `degraded` (by-path only, no serial) / `skipped` (no serial device visible at all). Observations: `by_id_tree` (`present`/`absent`), `count` (by-id adapters), `sysfs_only` (USB devices udev never named — the population §12's fallback exists for) and `other_candidates` (by-path-only nodes and BSD `cu.*`, counted, never judged), plus one line per device naming the identity it resolves to. |
| **P5** | Rig discovery **and certification**: classify every named port (dangling / loopback / paired, both directions) and certify it — break, custom baud, counter support per port; a rate ladder including a nonstandard rate and a deliberate baud mismatch per pair. (§13, §15.21) | `supported` (discovered and certified) / `degraded` (miswired, or an uncharacterized item) / `unsupported` (data-integrity failure) / `skipped` (no `--port`). |
| **P6** | pty-master readiness after the last slave closes: once the last slave fd closes, does the master keep asserting POLLIN with nothing to read — the shape that spins a close-triggered poll loop? Also whether the node's own last-close termios reset re-arms readability. (§7.2, §15.36) | `supported` → the numbers are recorded; a kernel that reads differently is `degraded` **with the observation named**. Diff this block before simplifying `pty.rs`'s `saw_session` latch or its last-close drain. **That diff has been taken — 7.0 vs 6.18, byte-identical (see below) — and it licenses neither simplification:** `handler_reset_readable_bytes: 1` on *both* kernels confirms the drain load-bearing rather than removable, and the latch is barred by AGENTS invariant 16 rule (3), a write-lock-leak property no probe measures in either direction. |
| **P7** | Evidence a collapsed client session leaves on the master: which session shapes (bare open/close, `tcsetattr`-only, one byte written) leave a readable packet, and whether the presence latch covers each. (§7.2's session-evidence rule, §15.36 F4) | `supported` → the latch's premise holds here. The `latch_covers_*` observations are the ones to compare across kernels; both read `true` on 7.0 and 6.18 alike, which retires this probe's named risk (a 6.18 that left nothing readable would have failed the widened latch silently). |
| **P8** | epoll vs `read(2)` on a pty master: does epoll report the master readable while `read` returns EAGAIN — the busy-loop shape that put the data plane on `poll(2)`? Probed with **raw epoll**, never `AsyncFd`. (invariant 1, §15.18) | `supported` → invariant 1's premise measured, not assumed (`spin_ratio`, `busy_loop_reproduced`, `epoll_agrees_with_poll2`). |
| **P9** | `poll(2)` timeout granularity: for a never-ready tty fd, what a requested 0/1/5/10 ms timeout actually costs (min/median/max µs, and the overshoot). (§15.19's timer floor) | `supported` → the adaptive backoff's constants have measured ground under them on this kernel. |
| **P10** | pty buffer depth: how many bytes a pty accepts in each direction before it would block with nothing draining the far end. (§5 boundary policy, §15.19) | `supported` → the depth every backpressure argument in §5 rests on, in numbers. |
| **P11** | Real-port line-state counters: do `TIOCGICOUNT` (driver error/edge counters) and `TIOCMGET` (modem lines) answer on a real port, and what do they read? (§5, §7.1) | `supported` / `degraded` (counters absent — macOS has no `TIOCGICOUNT`) / `skipped` (no `--port`). **Opt-in for the same reason as P3/P5: opening a port toggles DTR.** Its consequence text reads P5's certified-pair count: the deliberate baud mismatch transmits only over a cross-wired pair, so on a rig without one P11 reports that the item did not run rather than offering it as the reason a `frame` count is nonzero. |
| **P12** | Session-boundary **edge** on a pty master: does an edge latch report a collapsed client session that left *nothing readable*, and does it stay silent while idle? (§6 detach-release, §15.39) | **P7's sibling — read the two together.** They ask the same question of the two different mechanisms that can carry detach-release, and the answer differs by platform: Linux keeps a readable packet (P7's subject) and this is `skipped` there by design; Darwin destroys it at last close, so the *edge* is the only mechanism and this is what carries §6. `supported` → the termios-only shape posts an edge **and** an idle hung-up master posts none in 200 reader-shaped passes. `degraded` → either the shape posts nothing (read P7: if it is `supported`, the packet route is carrying it), or — the dangerous direction, reported first — an idle master posts edges, which re-fires the last-close handler on a pair no client touched and releases a lock the operator took. `idle_edges_in_200_passes` is the anti-spin number; `a_open_close_edge` is reported because Darwin covers a shape Linux deliberately does not. |

### Every report says what produced it

Both renderings open with a **Build** block, and a cross-kernel diff should read it
before anything else:

| Field | What it answers |
|---|---|
| `commit` | Which tree the binary was built from — `<short sha>`, `<short sha>-dirty`, or `unknown` where git could not answer (a source tarball, a container without git, a `cargo package` staging tree). Override it with `SNX_BUILD_COMMIT=…` for a vendored or reproducible build. |
| `probe set` | A 16-hex-char digest over every probe's `(id, title, question)`. **Equal fingerprints mean the two runs asked the same questions of their kernels**; unequal means the probe set moved and a field-by-field diff is reading two different instruments. |
| `generated` | The run's UTC timestamp. |

`probe_set` is the load-bearing one, because it answers the question a diff
actually needs and answers it with no repository access — where a commit hash
requires the reader to work out what changed between two commits. It digests
*identity and question only*, never observations or verdicts: those are the
measurements the diff exists to compare, and two healthy boxes differ in them by
design, so folding them in would report every real cross-kernel pair as
incomparable.

This exists because it was missing exactly once and cost real work. The 2026-07-27
Linux 6.18 report came from a `fe1c52c`-vintage binary rather than HEAD, and the
only reason anyone noticed was that its P4 section still carried the pre-`RES-2`
*title*, read by eye after the fact — while `expectations/linux.jq`, whose stated
job is proving the artifact "diffable field by field", had no clause that could
see it. It has one now (`.build.probe_set`, `.build.commit`), asserting **presence**
rather than a particular value: a build that genuinely cannot know its commit must
not redden a healthy box, which is the same rule P4's clause already follows. The
Markdown twin also carried no date at all before this, so a report committed beside
a design note was datable only by its commit.

**P6–P11 exist to be *diffed between kernels*, which changes how their verdicts
read.** Each emits its raw measurements as structured JSON, not just a verdict,
and a kernel that *differs* is reported `degraded` **with the observation named**,
never `unsupported` — because a different number is a fact to carry into the
design argument, not a capability the design lost. (`expectations/linux.jq` and
`nexus-itest`'s `meta_gates` both gate on `unsupported`, so this distinction is
load-bearing rather than editorial.) When a probe block tells you to diff it
before simplifying something, that is §13's "new kernels get diffed, not assumed"
in its operational form.

A probe verdict of `unsupported` fails the process (exit 1) — a stop condition:
surface the report for a design amendment rather than coding around it (plan §1).
`skipped` and `degraded` exit 0. Hardware tiers (dangling converter → TX/RX
jumper → cross-wired null modem) are the §13 no-target doctrine; Tier 1 (a
dangling converter, no receiver) already exercises identity, exclusivity, and
lifecycle.

### P4 asks about devices, not about the by-id tree

P4 and the `/dev/serial/by-id` environment check both used to gate on that tree
being a directory, and reported `skipped (no /dev/serial/by-id tree)` and
`absent (no USB-serial adapter)` when it was not. That is precisely the
environment §12's resolver fallback exists for — `/sys` mounted and no udev
`60-serial.rules`, i.e. a container handed a bare `--device=/dev/ttyUSB0`, a
busybox-mdev image, or macOS, which has no such tree at all — so the report
AGENTS §3 makes the first attachment on every bug report was contradicting a
daemon working fine beside it, and pointing the reader away from the code that
had just learned to handle the case (review 32, `RES-2`). The tree's absence is
not the adapter's absence.

P4 therefore enumerates through the resolver's own passive enumeration face, the
same source capture reads (§12), so what it reports is what `add-node` would
store — and it stays **`supported`** in a no-udev environment by design: identity
resolution works there, it merely works through the `<sys>/class/tty` listing
rather than the readlink fast path, and the absence is named in the probe's
consequence text instead of in its verdict. That is not cosmetic — a `degraded`
verdict here would fail `expectations/linux.jq`, which admits only `supported` or
`skipped` for P4, reddening a box the daemon is happy on.

The *environment* difference is carried by the `/dev/serial/by-id` environment
check instead, which now has three arms rather than two: `supported` (tree
present, adapters counted), `degraded` (tree absent, but N serial devices visible
another way — what the operator has lost is udev's stable naming, not the
device), and `skipped` (absent, and nothing visible through sysfs, by-path or
`cu.*` either). §13's rule applied to an environment rather than a kernel: a
difference is `degraded` with the observation named, never `unsupported`.

### P5's verdict folds the certificate in

§15.21 makes the rig certificate the **precondition every tiered checklist run
starts from**, "so a tier failure is attributable to serial_nexus rather than a
loose jumper" — which only means anything if the precondition can fail. It can:
each certification item's outcome is folded into P5's verdict, not merely printed
as report text, and the failing item is **named in the verdict line** (review 26,
DOC-1b). The precedence is worst-first:

- **`unsupported`** — a *data-integrity* failure: the rig did not deliver the
  bytes it was handed. Today that is the paired **rate ladder** (9600, 115200 and
  a nonstandard rate, certified in *both* directions), and it exits 1. A tiered
  run started from this certificate would misattribute the rig's own loss to
  serial_nexus.
- **`degraded`** — the rig carries data but is not fully characterized. Miswiring
  found at discovery (a half-crossed/asymmetric pair) lands here, and so does any
  uncharacterized item: per-port `custom_baud`, `break`, `icounter`; per-pair
  `deliberate_mismatch` (TX at 115200 into an RX at 9600 must corrupt the nonce
  *and* raise the frame-error counter — proof the counters are observable); or a
  port that would not reopen for characterization at all. A tier leaning on that
  item would be running uncertified; everything else is certified.
- **`supported`** — discovered and certified, **at a named tier**. On a non-UART
  (the CI pts sim) characterization reports `skipped (not a UART)` and records
  **no** failure, by §15.21's design, so P5's logic never waits for a bench.
  **Off Linux it always skips, and says so differently.** The UART predicate is
  `TIOCGICOUNT`, which is Linux-only, so it answers "no" for every port on macOS —
  a real FTDI adapter included. Measured 2026-07-28 on macOS 15.7.8 with two
  genuine adapters cross-wired: discovery named the pair correctly in both
  directions, and characterization reported `skipped (not characterizable here
  (TIOCGICOUNT is Linux-only))` rather than calling the operator's hardware a
  non-UART. So on a Mac, **read P5 for discovery and pairing only** — the
  certificate a tiered checklist run starts from has to come from a Linux box.

`supported` names the tier because the tiers certify different things and the word
alone does not distinguish them: **Tier 3** (a cross-wired pair) is the only one
where the rate ladder and the deliberate baud mismatch transmit at all, **Tier 2**
(a TX↔RX jumper) is a real driver data path on one clock, and **Tier 1** (a
dangling converter — §13's *baseline*, not a corner case) certifies per-port items
only and receives no break anywhere. The verdict says which, and says what that
tier did **not** run, because §15.21 makes this certificate the precondition a
tiered run starts from: an unqualified "certified" over a Tier-1 rig invites a
Tier-2/3 run to start from it. That is not hypothetical — it is what the
2026-07-27 6.18 report said, verbatim, over one dangling adapter. Guard:
`probes::tests::the_certificate_names_its_tier_and_what_that_tier_did_not_run`.

The negative-control ritual therefore means what it says: pull one wire, re-run
P5, and the asymmetry is named at discovery *and* whatever it broke in the
certificate is named in the verdict.

## Kernel-of-record report (Linux 7.0.0-28-generic, x86_64)

### P1–P4 as of 2026-07-19

Rust 1.97.1, edition 2024. Adapter: FTDI FT232R `usb:0403:6001:ABSCDJ6O:00`.

- **P1 — supported.** `ioctl_packet_on_tcsetattr`, `clear_extproc_produces_packet`,
  `reassert_extproc_via_master` all true. EXTPROC observation is primary; poll is
  the backstop.
- **P2 — supported.** HUP absent while open, present after close, clears on
  reopen; termios settable with no slave; zero-timeout poll ≈ sub-µs.
  **Refinement:** a master whose slave was *never opened* does **not** report
  POLLHUP — at PTY node creation, open+close the slave once to prime it.
- **P3 — skipped.** P3 opens nothing unless a port is named, so a bare run reports
  `skipped (no --port named)`; re-run with `--port` (and, if the open is refused,
  grant device access via a udev `GROUP=plugdev` rule or `dialout` membership).
  The real-hardware P3 result is in the 6.18 section below.
  Verified from source: **serial2 sets `O_NOCTTY` but not `TIOCEXCL`**, so the
  daemon issues it on the raw fd (`nexus_sys::set_exclusive`); **serial2-tokio
  hides the fd**, so the serial node opens `serial2` blocking and drives
  readiness with its own `poll(2)` (§13 fallback). Two shapes, per §15.19: the
  hostward reader parks in a **blocking `poll(2)` on a dedicated thread**
  (`nexus_sys::poll_blocking`), and the low-rate async side uses a
  **non-blocking `poll(2)` with an active→idle backoff** (`poll_ready`,
  `ACTIVE_POLL`→`IDLE_POLL`). `tokio::io::unix::AsyncFd` is ruled out
  outright — its epoll readiness busy-loops on a pty master (§15.18), and the
  phase-3 benchmark capped the async poll loop at ~1 MB/s where the
  blocking-thread reader runs at line rate (§15.19).
- **P4 — supported.** Yields `usb:0403:6001:ABSCDJ6O:00` via the sysfs
  ancestor-walk (nearest `bInterfaceNumber` = interface; first `idVendor` =
  device — stop there or you bind the root hub).

None of these contradict the design; two implementation notes (P2 priming, P3
serial-node fd strategy) are carried into phases 2 and 7.

### All eleven probes, 2026-07-27 — the baseline the 6.18 diff was taken against

HEAD (`a2d3b96`), Ubuntu 26.04, two FTDI FT232R adapters cross-wired as a null
modem (`usb:0403:6001:BH00LL8O:00` on `/dev/ttyUSB0`, `usb:0403:6001:BH00L4KU:00`
on `/dev/ttyUSB1`). **21 supported · 0 degraded · 0 unsupported · 0 skipped** —
21 = 9 environment rows + 12 probe entries, P3 running once per named port. A
**passive** run on the same box reports **17 supported · 3 skipped** (P3/P5/P11
skip without `--port`); both figures are real and neither supersedes the other.

The raw numbers are not transcribed here — they change per box and per run, so
the report itself is the record. Capture them with `nexus-doctor --json >
doctor-7.0.json` and diff against the target kernel; the per-probe "Consequence"
paragraphs say what each number licenses. What the 2026-07-27 run adds beyond the
2026-07-19 four is a real-hardware P3 on this kernel, a **paired** P5 certificate
(`rate_ladder=true deliberate_mismatch_observed=true`), and the P6–P11
measurements that were the whole point of the six new probes.

## Confirmed on Linux 6.18 — P1–P4 (2026-07-19) and P1–P11 (2026-07-27)

serial_nexus must run on **Linux 6.18**, older than the 7.0 dev box. Two runs
exist, both on `6.18.14-1rodete4-amd64` (Debian GNU/Linux rodete):

1. **2026-07-19** (`e93149d`, FTDI FT232R `usb:0403:6001:ABSCDGL6:00`) — P1–P4,
   the only probes that existed. P5 landed in `aef797f` two days later and
   P6–P11 a week later.
2. **2026-07-27** (FTDI FT232R `usb:0403:6001:ABSCDJ6O:00` on `/dev/ttyUSB12`) —
   **all eleven probes**, `19 supported · 0 degraded · 0 unsupported · 0 skipped`
   (19 = 8 environment rows + 11 probes). This is the diff P6–P11 were added for.

`ABSCDJ6O` appears under *both* kernels above — as the 7.0 adapter on 2026-07-19
and the 6.18 adapter on 2026-07-27 — because the owner physically moved it between
the boxes. Confirmed, not a transcription error; adapters travel and identities
follow them, which is §12 working as designed. The *machines* did not move, so
"the same 6.18 kernel measured 605 ns on 2026-07-19" below compares like with like.

### Scope of the 2026-07-27 run — three limits a totals line does not show

1. **Binary vintage.** The run used a **`fe1c52c`-vintage** binary, not HEAD. Its
   P4 block is the pre-`RES-2` "by-id resolution ground truth" shape with no
   `by_id_tree` / `sysfs_only` / `other_candidates`, which is how the vintage was
   established at all — by eye, from a section title, after the fact. That is the
   accident the **Build** block above now removes: a report from either box would
   state its `commit` and its `probe set`, and two unequal fingerprints would have
   said "not comparable" in one glance. `git diff fe1c52c a2d3b96 -- nexus-doctor/src/probes.rs`
   touches only `p4_resolver`, `environment()` and tests — so **P1–P3 and P6–P11
   are validly diffable and P4 is not**. `environment()` changed too, which the
   headline arithmetic below silently depends on, so state the reason rather than
   assume it: HEAD's rewrite added a third arm for a *missing* by-id tree and
   unioned the access rows with `resolver.enumerate_ports()`. Neither moves here —
   the tree is present, and the box's one by-id adapter, its one named port and
   its one enumerable device are the same `/dev/ttyUSB12`. The control is the 7.0
   box, where a HEAD run emits exactly two access rows for its two adapters and
   none for the `/dev/ttyS*` nodes beside them, `enumerate_ports()` covering by-id,
   sysfs-USB and by-path rather than every tty.
2. **Rig tier.** The box is **Tier 1** — one *dangling* FT232R — so P5 emitted a
   per-port certificate and no pair certificate. The paired rate ladder (the sole
   route to a P5 `unsupported`) and the deliberate baud mismatch never executed,
   no break was ever *observed* (`brk = 0`), and P3/P11 covered one port instead
   of two. **"P5 supported" therefore means strictly less on 6.18 than the same
   word on 7.0**, and every `crossover_ports()`-gated test self-skips there —
   including `p12_serial_exclusivity::a_break_straddled_by_a_replace_leaves_the_line_transmitting`,
   the guard for invariant 15's break clause.
3. **Markdown only, and the gate has since moved.** No `--json` was captured, so
   `nexus-doctor --json | jq -e -f expectations/linux.jq` has never been
   **executed** on 6.18 — its content satisfied every clause of `linux.jq` *as
   that file stood at the `fe1c52c` vintage*, which was byte-identical to HEAD's
   at the time, but inference is not execution. **That inference no longer
   transfers**: the 2026-07-28 provenance work added `.build.probe_set` and
   `.build.commit` clauses, and a `fe1c52c` binary emits no `build` object at all,
   so the 2026-07-27 artifact would now *fail* the gate. Nothing about the kernel
   changed — the artifact predates the field — but the re-gate genuinely requires
   a HEAD-binary re-run rather than a re-reading.

Beyond the doctor: **`cargo test --workspace` has never been run on 6.18.** CI is
`ubuntu-latest` + `macos-latest`. The production kernel's evidence base is eleven
probes and zero executed tests, including the `p12_pty_setup` guards whose
premises this run just confirmed. Closing all four costs one visit: a HEAD binary,
both adapters cross-wired, `--json`, and a `cargo test --workspace --locked`.

### What the diff established

- **P6 and P7 are byte-identical to 7.0**, every field of every observation group.
  P6: `passes 64 / pollhup 64 / pollin 0 / bytes_read 0 / [EIO=64]`,
  `handler_reset_applied: true`, `handler_reset_readable_bytes: 1`, and the
  post-reset pass reading exactly 1 byte. P7: shape (a) 0 bytes, shape (b) 1 byte
  leading `0x40` with the ioctl bit set, shape (c) 2 bytes with a data packet,
  and **both `latch_covers_*` true**. (Do *not* cite P6's matching `elapsed_ms`
  as evidence — it is 64 passes × a 2 ms pause under a 200 ms window, structurally
  forced on any box that completes the passes.)
- **P8** is identical on every semantic field — `busy_loop_reproduced: false`,
  `epoll_agrees_with_poll2: true`, idle `epoll_ready_waits 0 / [EAGAIN=64] /
  spin_ratio 0.0`, hung-up `epoll_events 64 / [EPOLLHUP=64] / [EIO=64] /
  ready_then_no_data 64`. Only `elapsed_ms` moves (135/68 on 6.18 against 146/82
  and 160/78 in two 7.0 runs) — and those two 7.0 runs are the *same binary on the
  same box*, spreading 14 ms on the idle block, which is the scale of the
  cross-kernel gap rather than smaller than it. Wall clock, not a kernel property.
- **P1's three booleans, P2's five presence booleans and P3's seven serial-fit
  observations are identical**, including `hup_when_never_opened: false` (so
  §3.2's slave priming is *mandatory* on 6.18, not droppable) and
  `tiocexcl_refuses_second_open: true`.
- **P9's timer floor agrees**: 1057 / 5060 / 10059 µs against 7.0's 1065–1070 /
  5069–5070 / 10067–10076 — 8–17 µs apart, i.e. 1.3 % / 0.20 % / 0.17 % of the
  requested 1 / 5 / 10 ms — with 6.18's overshoot medians *tighter* (57/60/59 µs
  vs 65–76). `ready_passes_total: 0` on both, so neither measurement is
  contaminated.
- **P10 differs by one flip-scheduling case, which the probe itself diagnoses**:
  6.18 reads 13824 first-pass / `settled_extra` 0 / 13824 total / 4 writes, 7.0
  reads 11776 / 3584 / 15360 / 3. A first pass short by a chunk with a matching
  `settled_extra` *is* the late-flip case, so 7.0 is the late flip and 6.18 the
  mid-fill; both land inside the band the probe declares for 7.0 against itself.
  Every structural field matches, including `pending_output_bytes: 0` in both
  directions — which answers `nexus-sys`'s standing "does this kernel account for
  a pty here at all" hedge with *no*, the same as 7.0.
- **P11's ioctl availability and field set are identical**; its absolute counts
  differ by construction (they accumulate from driver bind).

**Two numbers moved, and neither is a kernel property.** The zero-timeout `poll(2)`
cost reads 1162 ns (P2, 4096 samples) and 1753 ns (P9, 16 samples) on 6.18 against
266–278 / 207–265 ns on the 7.0 box. Four things argue it is the box, not 6.18:
the same 6.18 kernel measured **605 ns** for the identical P2 code on 2026-07-19,
a 1.9× spread within one kernel; the two 7.0 runs disagree with *themselves* on
both fields (278 vs 266, 265 vs 207); P2 and P9 measure different things (a
4096-sample `poll(POLLHUP, 0)` on a hung-up master versus a 16-sample
`poll(POLLIN, 0)` with the slave held open), so their mutual 1.5× disagreement is
partly methodological; and 6.18 is at or below 7.0 on every *timed* median. **One
confounder nobody has excluded: the 6.18 binary's build profile** — a debug build
inflates exactly the sub-microsecond loops and leaves every sleep-dominated row
untouched, which is the observed pattern. It is load-bearing on no constant either
way: at `ACTIVE_POLL` = 200 µs even 1.75 µs per ask is ~0.9 % of a core per fd,
and the shipped decisions read the 1 ms floor, which agrees.

**One number is unexplained and benign — and it caught a defect in this report.**
The dangling 6.18 port reads `frame=4` and `rx=138` with nothing wired to it. P11
told the operator that was "usually P5's deliberate baud-mismatch item", which it
**cannot** be: that item transmits only over a discovered *pair*, and this box has
none. P11 now reads P5's certified-pair count and says so instead (fixed
2026-07-28, guard
`probes::tests::p11_blames_the_baud_mismatch_only_when_a_pair_was_certified`). The
remaining candidates are cumulative history since `ftdi_sio` bound the device, or
crosstalk into a floating RX during P3's 250000-baud transmit. Disambiguate in a
minute if it ever matters: replug the adapter — which rebinds the driver and zeroes
the counters — and re-run. 0/0 is history, a repeat is crosstalk.

### What it licenses: nothing

The two probes whose own output says "diff this block before simplifying
anything" are answered, and the answer is that **neither simplification is
licensed.**

- P6 agreeing on both kernels does mean an ungated `closed`-only last-close arm
  would not spin *on the hangup* on 6.18 either. That is not permission to
  simplify. `pty.rs` already disclaims dependence on it in so many words ("the
  anti-spin argument needs no assumption about POLLIN going quiet after a
  hangup"), and the `saw_session` latch's live justification is **invariant 16
  rule (3)** — a collapsed-session write-lock leak measured five of five under a
  saturated endpoint, a correctness property about a drain that ends early, which
  no probe measures on any kernel.
- P6's `handler_reset_readable_bytes: 1` reading identically on 6.18 confirms the
  last-close drain **load-bearing on the production kernel**, not removable.
- P8 licenses nothing about `AsyncFd`: the non-reproduction is scoped to a layer
  below the starvation invariant 1 records (tokio's readiness *guard*, not
  `epoll_ctl`), on both kernels alike.
- P1/P2 release neither the §7.2 reconciliation backstop nor slave priming — the
  degradation paths are unconditional by construction, and nothing in the daemon
  reads a kernel version or a probe verdict.

So the kernel-sensitive mechanics (EXTPROC observation, POLLHUP presence,
last-close readiness, collapsed-session evidence) are de-risked across the matrix;
the design's fallbacks remain live regardless. Re-run `nexus-doctor --json | jq -e
-f expectations/linux.jq` on any new target — if P1 ever reports `degraded` that
is fine (poll backstop), but a P2 `unsupported` would be a real stop condition.

> **A caution for the next reader.** Each probe's per-run "Consequence" paragraph
> hardcodes 6.18 as the production kernel, so a report generated *on* 6.18 tells
> you to "diff this against the production kernel (6.18)" — i.e. against itself.
> Those strings are correct to leave alone (they are unconditional by design);
> read them against the `kernel` row of the report's own Environment table.
