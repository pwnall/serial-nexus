# serial-nexus-doctor — capability checker

`serial-nexus-doctor` is the shipping diagnostic for serial_nexus (design §15.17). It
consolidates every kernel-behavior probe the design depends on, plus environment
checks, into one binary that emits a copy-pasteable **Markdown** report (the
expected first attachment on any support request) with a **`--json`** twin for
CI. It supersedes the throwaway per-spike binaries of the v1 plan.

```
serial-nexus-doctor              # Markdown report on stdout (default)
serial-nexus-doctor --json       # JSON twin for CI: serial-nexus-doctor --json | jq -e -f expectations/linux.jq
serial-nexus-doctor --port /dev/ttyUSB0   # opt a real port into P3 (serial fit), P5 (rig) and P11 (counters)
serial-nexus-doctor --port /dev/ttyUSB0 --port /dev/ttyUSB1   # repeatable: P5 classifies the set
serial-nexus-doctor --dev-root ./fixtures # fixture /dev + /sys tree (test seam, §3; sys_root is <dev-root>/sys)
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
| **P10** | pty buffer depth: how many bytes a pty accepts in each direction before it would block with nothing draining the far end, **how many of those a reader can actually get back**, and **which line discipline the measured slave was in**. (§5 boundary policy, §15.19) | `supported` → the depth every backpressure argument in §5 rests on, in numbers, measured on the raw baseline the daemon runs. `degraded` → the measured slave was **not** raw, so the depths are some other configuration's and must not be diffed against a run that was (the observation names the mode). `skipped` → the probe could not run. Read `bytes_recovered_by_peer` beside `total_bytes_accepted` and `slave_termios_mode` beside both: acceptance is not delivery, and a depth without its discipline is two measurements wearing one name (notes §3.34). **Read `peer_pending_input_trust` before `peer_pending_input_bytes`**: a `contradicted-empty` there means the *ioctl* is wrong, not the queue — `FIONREAD` reported empty and the very next `read(2)` returned bytes, with both fills finished in `EAGAIN` and no writer in between. Darwin reads exactly that targetward (`0` beside 1024 recovered, 6 of 6 captures); Linux reads `undercounts`, saturating at 4095 with the rest still recoverable, which is a documented staging cap and **not** a fault. The classification is taken from `peer_pending_input_bytes_at_drain`, the sample immediately before the drain, because the older mid-measurement sample always had an innocent reading. `bytes_recovered_by_peer` is unaffected — it is a drain, not an ioctl, and it is the field to size anything against. The status deliberately does **not** move for this: an auxiliary ioctl fault folded into `degraded` would make one platform permanently yellow and would lose which direction is affected (notes §3.50). |
| **P11** | Real-port line-state counters: do `TIOCGICOUNT` (driver error/edge counters) and `TIOCMGET` (modem lines) answer on a real port, and what do they read? (§5, §7.1) | `supported` / `degraded` (counters absent — macOS has no `TIOCGICOUNT`) / `skipped` (no `--port`). **Opt-in for the same reason as P3/P5: opening a port toggles DTR.** Its consequence text reads P5's certified-pair count: the deliberate baud mismatch transmits only over a cross-wired pair, so on a rig without one P11 reports that the item did not run rather than offering it as the reason a `frame` count is nonzero. |
| **P12** | Session-boundary **edge** on a pty master: does an edge latch report a collapsed client session that left *nothing readable*, and does it stay silent while idle? (§6 detach-release, §15.39) | **P7's sibling — read the two together.** They ask the same question of the two different mechanisms that can carry detach-release, and the answer differs by platform: Linux keeps a readable packet (P7's subject) and this is `skipped` there by design; Darwin destroys it at last close, so the *edge* is the only mechanism and this is what carries §6. `supported` → the termios-only shape posts an edge **and** an idle hung-up master posts none in 200 reader-shaped passes. `degraded` → either the shape posts nothing (read P7: if it is `supported`, the packet route is carrying it), or — the dangerous direction, reported first — an idle master posts edges, which re-fires the last-close handler on a pair no client touched and releases a lock the operator took. `idle_edges_in_200_passes` is the anti-spin number; `a_open_close_edge` is reported because Darwin covers a shape Linux deliberately does not. **Three windows and a control, and the reading rule turns on the control.** `idle_window_tight` is the historical 200 back-to-back passes, unchanged in key, count and shape (six committed artifacts carry it) but now carrying its wall clock — in **microseconds**, because those passes cost 134–175 µs on Linux 7.0.0-29 and a millisecond field would print `0`. `idle_window_paced` asks the same question 64 times at the daemon's own 5 ms `IDLE_POLL`, which is the loop `nodes/pty.rs` actually runs; an edge in only one of the two is time-driven versus syscall-driven. `live_session_window` is the negative control — the same window with a client **attached**, where an edge would fire §6 detach-release mid-session and hand away the write lock of a client that never left. Then the rule: **`control_session_edge: false` makes every idle count `unmeasured`, not `quiet`.** After the windows the probe closes a slave on the same master through the same latch; if that boundary posts nothing the instrument is inert and `supported` is refused, because `EV_CLEAR` on a master already hung up at `watch()` time is exactly the shape where "0 edges" could be structurally guaranteed rather than measured. **P12 now carries observations while `skipped` on Linux, and that is deliberate:** the windows run on every platform, so a Linux report's `control_session_edge: false` beside a full set of executed passes is the inert arm proving itself inert — the negative control the kernel that *depends* on this mechanism cannot provide for itself, and a Linux `true` there would mean the latch had grown a second implementation nobody measured (notes §3.50). |
| **P13** | Disposition of unread client bytes at a pts **last close**: when a client writes bytes the master has not read and then closes, does this kernel **retain** them, **discard** them, or **block the close** waiting for the reader? Three shapes — no reader, reader-drains-first, and a no-reader `O_NONBLOCK` slave (XNU's `ttylclose` branches on exactly that flag). (§5's accounting, §7.2's drain-before-close ordering, notes §3.29) | **P7/P12's third sibling, and the one that separates the two answers they cannot.** P7 asks what a collapsed session leaves *readable* against a master nobody drains — a yes/no that `discards` and `waits-then-discards` answer identically, because an undrained wait times out and looks exactly like a flush. `close_microseconds` tells them apart: microseconds means the kernel decided immediately, hundreds of milliseconds means it waited for a reader that never came. Always `supported` when it measures (a probe error `degrades`): every policy is legitimate and the daemon is correct under each. Measured `retains`, 20 µs, 64/64 recovered on Linux 7.0.0-29 (`docs/doctor/linux-7.0-2026-08-05-tier3.json`, binary `71fc5a815852`, probe set `a131e1f4b46d6c83`, 2026-08-05; the no-reader shape's close is a handful of microseconds run to run — 20, 10 and 13 across the three committed captures — so read the *scale*, microseconds, which is the quantity that separates the policies, not the digit), and **`waits-then-discards` on Darwin 24.6.0 / macOS 15.7.8** — `close_waits_for_reader: true`, 600104 µs with 0 of 64 recovered against no reader, 23 µs with 64 of 64 when the master drains first, and 29 µs with 0 of 64 for an `O_NONBLOCK` slave (`docs/doctor/macos-24.6.0-2026-08-05-tier3.json`, binary `fa4b12d6f529`, probe set `a131e1f4b46d6c83`, 2026-08-05). The `waits-then-*` arm is therefore measured, not hypothetical, and the two kernels differ by ~86000× in `close_microseconds` — which is the field that exists to separate them. Both cautions this row used to carry are **discharged**: the Linux figures are artifact-backed (three captures committed 2026-08-05) and both sides carry probe set `a131e1f4b46d6c83`, so this is a lawful field-by-field diff rather than a recorded reading. One caution replaces them, and it is narrower: a shared fingerprint certifies the two runs asked the same *questions*, not that they asked them of the same *configuration* — see P10's `slave_termios_mode`. The reason it exists: under `discards` a lost byte is a lost microsecond race, while under `waits-then-discards` it means a reader stalled for the *whole timeout* — a daemon-side event, not a kernel one — and `docs/macos.md` (2026-08-04) records a macOS CI failure whose competing readings differ on precisely that. |

### Every report says what produced it

Both renderings open with a **Build** block, and a cross-kernel diff should read it
before anything else:

| Field | What it answers |
|---|---|
| `commit` | Which tree the binary was built from — `<short sha>`, `<short sha>-dirty`, or `unknown` where git could not answer (a source tarball, a container without git, a `cargo package` staging tree). Override it with `SNX_BUILD_COMMIT=…` for a vendored or reproducible build. |
| `probe set` | A 16-hex-char digest over the deduplicated, sorted set of every probe's **`(id, question)`** — *not* its title. **Equal fingerprints mean the two runs asked the same questions of their kernels**; unequal means the probe set moved and a field-by-field diff is reading two different instruments. The title is excluded on purpose: P3's embeds the device path and P3 is emitted once per `--port`, so folding it in would make a one-port box and a two-port box of the same binary disagree — printing "not comparable" over exactly the cross-kernel diff this field exists to underwrite. |
| `generated` | The run's UTC timestamp. |

**What `probe_set` does not cover, stated because it bit (2026-08-05).** The digest is
over `(id, question)`, so a change to a probe's *body* — what it configures, what it
measures, what it reports — keeps the fingerprint identical. Equal fingerprints certify
that two runs asked the same **questions**; they do not certify that the runs asked them
of the same **configuration**. P10's baseline repair and its two new observations moved
the instrument and left `a131e1f4b46d6c83` unchanged on both sides. So when a probe body
changes without its question changing, say so in `docs/doctor/README.md` and in the
report's own observations, because the fingerprint will not. The alternative — folding the
implementation into the digest — was not taken: it would report every prose fix and every
refactor as "not comparable", which is the failure mode the title exclusion above already
exists to avoid. The cost is that the reader is told, rather than the tool refusing.

**The environment block names its own kernel, as of `71fc5a815852`.** `kernel` and `os`
came from `/proc/sys/kernel/osrelease` and `/etc/os-release`, both Linux-only, so every
macOS report recorded `""` and `unknown` — and marked them `supported` — which forced the
one fact every cross-kernel claim rests on to be typed into `docs/doctor/README.md`'s index
by hand. §16.13 says provenance is *recorded, never asserted*. Both fields now come from
`uname(2)`: `kernel` is the release (`7.0.0-29-generic`, `24.6.0`) and `os` falls back to
`<sysname> <release> (<machine>)` where no distribution publishes a `PRETTY_NAME`.
`nodename` is deliberately not read — it is the machine's hostname and nothing here needs
it. The two *older* committed macOS artifacts still read empty: they are frozen records of what
the tool printed on their date and are not rewritten. **The fix has since been observed working
on the platform it was written for**: `docs/doctor/macos-24.6.0-2026-08-05-7ead470-tier3{,-2,-3}.json`
report `kernel: "24.6.0"` and `os: "Darwin 24.6.0 (x86_64)"` from `uname(2)`, so a macOS report
now names its own kernel instead of needing the index to do it by hand.

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

**P6–P13 exist to be *diffed between kernels*, which changes how their verdicts
read.** Each emits its raw measurements as structured JSON, not just a verdict,
and a kernel that *differs* is reported `degraded` **with the observation named**,
never `unsupported` — because a different number is a fact to carry into the
design argument, not a capability the design lost. (`expectations/linux.jq` and
`serial-nexus-itest`'s `meta_gates` both gate on `unsupported`, so this distinction is
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
  item would be running uncertified; everything else is certified — and the
  verdict names the **tier** discovery found even here, plus, for `icounter` and
  `deliberate_mismatch` off Linux, the mechanism: those two read `TIOCGICOUNT`,
  which does not exist there, so no rig can certify them and re-seating a cable is
  the wrong instruction.
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
only. The verdict says which, and says what that
tier did **not** run, because §15.21 makes this certificate the precondition a
tiered run starts from: an unqualified "certified" over a Tier-1 rig invites a
Tier-2/3 run to start from it. That is not hypothetical — it is what the
2026-07-27 6.18 report said, verbatim, over one dangling adapter. Guard:
`probes::tests::the_certificate_names_its_tier_and_what_that_tier_did_not_run`.

**No tier receives a break, and the Tier-1 line used to imply otherwise.** The
per-port `break` item is `set_break(true).is_ok() && set_break(false).is_ok()` on a
port the doctor holds open alone — local ioctl acceptance — and `p5_certify_pair`
transmits a rate ladder and a bulk baud-mismatch pattern and no break at all. So
**the doctor raises no `brk` on any port, at any tier, on any kernel**, and the
Tier-1 verdict's old tail, "and no break was received by anything", was true of
Tier 1 and equally true of Tier 3 while reading as though the tier were the reason.
The two 2026-07-29 Tier-3 reports carry the assertion half on both boxes:
`break_ok: true` on both ports and `break=true` in both certificates, on 6.18 and
on 7.0 alike.

*(Said as "`brk` stays 0 at every tier on every kernel" this would be a claim about
the counter rather than about the doctor, and the 7.0 Tier-3 report falsifies that
reading: `brk=2` on `/dev/ttyUSB1` there, against `brk=0` on both ports of the 6.18
one. Which is the parenthetical below working as designed — the counter accumulates
from driver bind, so a nonzero `brk` records a break something *other* than the
doctor put on that line, and what did is not established by the report. The
disambiguation is the one this page already prescribes for the same class of
reading: replug the adapter, which rebinds the driver and zeroes the counters, and
re-run.)* Break *reception* is a job for
`p12_serial_exclusivity::a_break_straddled_by_a_replace_leaves_the_line_transmitting`,
not for a probe.

The negative-control ritual therefore means what it says: pull one wire, re-run
P5, and the asymmetry is named at discovery *and* whatever it broke in the
certificate is named in the verdict.

## Kernel-of-record report (Linux 7.0.0-28-generic and -29-generic, x86_64)

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
  daemon issues it on the raw fd (`serial_nexus_sys::set_exclusive`); **serial2-tokio
  hides the fd**, so the serial node opens `serial2` blocking and drives
  readiness with its own `poll(2)` (§13 fallback). Two shapes, per §15.19: the
  hostward reader parks in a **blocking `poll(2)` on a dedicated thread**
  (`serial_nexus_sys::poll_blocking`), and the low-rate async side uses a
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

### All eleven probes, 2026-07-27 — the baseline the *first* 6.18 diff was taken against

HEAD *of that day* (`a2d3b96`), Ubuntu 26.04, two FTDI FT232R adapters cross-wired
as a null modem (`usb:0403:6001:BH00LL8O:00` on `/dev/ttyUSB0`,
`usb:0403:6001:BH00L4KU:00` on `/dev/ttyUSB1`). **21 supported · 0 degraded ·
0 unsupported · 0 skipped** — 21 = 9 environment rows + 12 probe entries, P3 running
once per named port. A **passive** run on the same box reported **17 supported ·
3 skipped** (P3/P5/P11 skip without `--port`); both figures are real and neither
supersedes the other. **That pair of adapters left this box and came back** — it
moved to the 6.18 production box on 2026-07-29 and returned later the same day, so
the *hardware* is here again while neither *figure* is reproducible: what moved
under them is the instrument, not the rig (P12 arrived, P4 and P5 were rewritten —
see the next paragraph but one). The two 7.0 figures that are in the tree today are
**21 · 0 · 0 · 1** on the same cross-wired pair and **13 · 0 · 0 · 6** passive, both
below.

The raw numbers are not transcribed here — they change per box and per run, so
the report itself is the record. Capture them with `serial-nexus-doctor --json >
doctor-7.0.json` and diff against the target kernel; the per-probe "Consequence"
paragraphs say what each number licenses. What the 2026-07-27 run adds beyond the
2026-07-19 four is a real-hardware P3 on this kernel, a **paired** P5 certificate
(`rate_ladder=true deliberate_mismatch_observed=true`), and the P6–P11
measurements that were the whole point of the six new probes.

**That run is history, not the baseline any more.** `doctor/src/probes.rs`
moved 702 lines between `a2d3b96` and `85699d6` — P12 arrived (§15.39), P4 and P5
were rewritten — so a field-by-field diff against a HEAD run would be comparing two
instruments. It cannot even *say* so itself: `a2d3b96` predates the Build block, so
that artifact carries no `probe_set` at all, and "no fingerprint" is the one answer
the comparability rule has to treat as *not comparable* rather than as agreement.
Nothing about the run was wrong; the instrument moved under it, which is the case
the fingerprint was added to catch — catching itself on its first outing.

What the fingerprint deliberately does **not** move on is report *text*. Correcting
a `Consequence` paragraph or a doc comment changes what an operator reads and leaves
`(id, question)` alone, so archived artifacts stay comparable across such an edit —
verified after the 2026-07-29 corrections below by rebuilding and re-running: still
`01b257ece8c48470`.

### All thirteen probes, 2026-08-05 — the current 7.0 baseline, and the first lawful P13-era diff

**Tier 3, and it is the 7.0 run to read now.** `71fc5a815852` binary, probe set
**`a131e1f4b46d6c83`**, committed as three sequential runs on an idle box:
[`linux-7.0-2026-08-05-tier3.json`](doctor/linux-7.0-2026-08-05-tier3.json) and its `-2` /
`-3` siblings. **22 supported · 0 degraded · 0 unsupported · 1 skipped** — 22 = 9
environment rows + 13 of the 14 probe rows (thirteen probes, P3 running once per named
port, so fourteen rows); the fourteenth is P12, inert on Linux by design (§15.39), and it
remains the only non-supported row anywhere in the report. The same cross-wired FT232R pair
(`usb:0403:6001:BH00L4KU:00` ↔ `usb:0403:6001:BH00LL8O:00`) is attached, so P5 certifies at
**Tier 3** and P3/P11 have two real ports.

**Why three runs and not one.** P10's depths move run to run as the tty's asynchronous flip
work lands, and a single capture cannot show a reader which differences are noise. The three
agree on everything that matters: `slave_termios_mode: "raw"` and every accepted byte
recoverable in both directions, P13 `retains` with 64/64 in all three shapes, and the
no-reader close at 20/10/13 µs — a spread that is itself the point, since the policy
classifier keys on the microseconds-versus-hundreds-of-milliseconds gap and never on a digit.

**This is the first pair in `docs/doctor/` that the directory's own comparability rule
permits to be diffed field by field in the P13 era**, because it carries the same
fingerprint as `macos-24.6.0-2026-08-05-tier3.json`. At that equal fingerprint:

| | Linux 7.0.0-29 | Darwin 24.6.0 |
|---|---|---|
| P13 policy / no-reader close | `retains`, 20 µs, 64/64 | `waits-then-discards`, 600104 µs, 0/64 |
| P9 zero-timeout poll, median | 170 ns | 23122 ns |
| P6 after last close | `POLLHUP` only, `EIO`, 0 POLLIN passes | `POLLIN\|POLLHUP` 64/64, `eof` |
| P7 collapsed session | 0 / 1 / 2 bytes readable | 0 / 0 / 0, `degraded` |
| P2 master is a terminal | yes | no (`ENOTTY`) |

**P10 is deliberately absent from that table, and the reason generalises.** Its committed
Darwin block predates the baseline repair, so those depths were measured on a *cooked* pty —
`apply_pty_baseline` sets the baseline through a slave it immediately closes wherever the
master is not a terminal, which P2's row above is exactly the measurement of, and BSD does not
carry that to the next open. Do not diff P10 across this pair until a macOS capture at
`71fc5a815852` or later reports `slave_termios_mode: "raw"` on both directions.

<!-- ANNOTATION 2026-08-05 (§5). That capture landed —
     `docs/doctor/macos-24.6.0-2026-08-05-7ead470-tier3{,-2,-3}.json`, `slave_termios_mode: "raw"`
     on both directions in all three runs — so P10 takes its place in the table above, and the row
     is the one added below. Two wording notes carried from the annotation on notes §3.34: the word
     "cooked" in this paragraph is an inference (the pre-repair artifact has no
     `slave_termios_mode` field to testify with), and the pre-repair hostward figure was never a
     depth, because `ceiling_hit: true` means the fill never reached a blocking point. The
     *sibling* caution in the next paragraph is NOT discharged and must not be swept with this
     one. -->

| | Linux 7.0.0-29 | Darwin 24.6.0 |
|---|---|---|
| **P10 depth, targetward** (`slave_termios_mode: raw` both sides) | 15360 accepted, 15360 recovered, 0 unrecoverable | 1024 accepted, 1024 recovered, 0 unrecoverable |
| **P10 depth, hostward** | 15360 / 15360 / 0 | 1022 / 1022 / 0 |

**Read that row as the kernel difference it now is.** Both sides report `raw`, both report
`bytes_unrecoverable: 0` and `ceiling_hit: false`, both were produced by the same probe code, and
neither moves run to run — Darwin's P10 block is byte-identical across its three captures, Linux's
across its three. So the ~15x is signal by the probe's own stated rule, and **Linux is the deeper
kernel**. Before the repair the same comparison read 4194304 hostward on Darwin and appeared to put
Darwin >=273x *deeper*; that number was a floor at P10's 4 MiB backstop, in a discipline the report
could not name. The instruction "check `slave_termios_mode` agrees on both sides first" is what
separates those two readings, and it is now a measured rule rather than a proposed one.

**One field in that row is unexplained.** Darwin takes 1024 targetward but 1022 hostward, from a
single 4096-byte write in each direction, reproducibly, where Linux varies by direction and by run. No probe currently asks
why, and this page does not guess (§7).

**The same caution has not been discharged for P10's siblings, and that is not an oversight.**
P6, P7, P9, P12 and P13 take the same fallback, so their Darwin rows above are not *known* to
have been measured on the daemon's line discipline either. They are not thereby wrong — a
readability question or a targetward write survives a cooked discipline far better than a
buffer *depth* does, and P8 never runs on Darwin at all — but "not obviously affected" is not
"measured". P7 is the one to watch: its `degraded` arm is what `docs/macos.md` cites as an
operator-visible macOS defect, and `set_baseline`'s own doc comment warns that measuring P7
without EXTPROC in the baseline reports a false `degraded`. Settle it by measuring on a Mac
(§7), not by reasoning from here (notes §3.34).

### All twelve probes, 2026-07-29 — the two 7.0 baselines that are in the tree

**Tier 3, and it is the 7.0 run to read.** `da290c616631` binary, probe set
`01b257ece8c48470`, committed as
[`docs/doctor/linux-7.0-2026-07-29-tier3.json`](doctor/linux-7.0-2026-07-29-tier3.json).
**21 supported · 0 degraded · 0 unsupported · 1 skipped** — 21 = 9 environment rows
+ 12 of the 13 probe rows (P3 runs once per named port, so twelve probes make
thirteen rows); the thirteenth is P12, inert on Linux by design (§15.39), and it is
the only non-supported row anywhere in the report. The cross-wired FT232R pair
(`usb:0403:6001:BH00LL8O:00`, `usb:0403:6001:BH00L4KU:00`) came back to this box
later on 2026-07-29 after its stay on the 6.18 production box, so P3/P5/P11 have two
real ports again and P5 certifies at **Tier 3**: `rate_ladder=true
deliberate_mismatch_observed=true`, with `custom_baud`/`break`/`icounter` true on
both ports. Its `probe set` equals the 6.18 Tier-3 report's, which is this
repository's stated comparability rule (`docs/doctor/README.md`) — the two commits
differ, and the fingerprint, not the commit, is what licenses a field-by-field diff.

**Three sequential passive runs, and a Tier-3 run does not replace them.** The same
`85699d66c5a5` binary, the same probe set, committed as
[`docs/doctor/linux-7.0-2026-07-29-passive-{1,2,3}.json`](doctor/). **13 supported ·
0 degraded · 0 unsupported · 6 skipped**, and `jq -e -f expectations/linux.jq`
**executes** clean against them (exit 0) — which is what proves that probe set and
that `linux.jq` actually agree, a thing no amount of reading either file settles.

Passive, and six skips, because they were taken **while the pair was away on the
6.18 box** — which is how that box became Tier 3 — not because this box is a
hardware-less one. So in those three runs P3/P5/P11 skip for want of a `--port`, P4
skips with "no serial device visible", and the `/dev/serial/by-id` environment row
exercises its **third arm** — `absent, and no serial device visible through sysfs,
by-path or cu.* either` — which is the RES-2 rewrite's skip case observed rather
than reasoned about. That arm is reachable *only* while the box is bare, so those
three runs remain the only in-tree observation of it. P12 skips on Linux by design
(§15.39). None of it costs the cross-kernel diff anything either way: P1, P2, P6,
P7, P8, P9 and P10 need no hardware and are the whole kernel-diff set.

**Three runs, because one sample of a varying quantity is indistinguishable from a
cross-kernel difference.** On one quiet box (load 0.44, sequential, nothing else
running) P9's 1 ms median moved 1066–1068 µs and its zero-timeout cost 264–287 ns,
while P10 produced **three different shapes** — `11776/3584/15360/3` (runs 1 and 3),
`13824/0/13824/4` (run 2), and a hostward `15360/0/15360/4` in run 1. Read that
before attributing any P10 or P9 delta to a kernel. The Tier-3 run is a fourth
sample of the same quantities on the same box and lands inside both spreads — P9
1068 µs at 1 ms and 262 ns at zero timeout (a hair under the passive band's 264),
P10 `11776/3584/15360/3` in *both* directions, the runs-1-and-3 shape — so it
widens the variance record rather than settling anything, which is the point.

## Confirmed on Linux 6.18 — P1–P4 (2026-07-19), P1–P11 (2026-07-27), all twelve at HEAD on a Tier-3 rig (2026-07-29)

serial_nexus must run on **Linux 6.18**, older than the 7.0 dev box. Three runs
exist, all on `6.18.14-1rodete4-amd64` (Debian GNU/Linux rodete):

1. **2026-07-19** (`e93149d`, FTDI FT232R `usb:0403:6001:ABSCDGL6:00`) — P1–P4,
   the only probes that existed. P5 landed in `aef797f` two days later and
   P6–P11 a week later.
2. **2026-07-27** (FTDI FT232R `usb:0403:6001:ABSCDJ6O:00` on `/dev/ttyUSB12`) —
   **all eleven probes**, `19 supported · 0 degraded · 0 unsupported · 0 skipped`
   (19 = 8 environment rows + 11 probes). This is the diff P6–P11 were added for.
3. **2026-07-29** (`85699d66c5a5` = HEAD, probe set `01b257ece8c48470`, the FT232R
   pair `BH00LL8O` ↔ `BH00L4KU` cross-wired on `/dev/ttyUSB0` and `/dev/ttyUSB1`) —
   **all twelve probes on a Tier-3 rig**, `21 supported · 0 degraded ·
   0 unsupported · 1 skipped` — 21 = 9 environment rows + 12 of the 13 probe rows
   (P3 runs once per named port, so twelve probes make thirteen rows); the
   thirteenth is P12, inert on Linux by design, and it is the *only* non-supported
   row anywhere in the report. **This
   is the run to read**, and the artifact is in the tree:
   [`docs/doctor/linux-6.18-2026-07-29-tier3.md`](doctor/linux-6.18-2026-07-29-tier3.md).

Adapters travel between the boxes and identities follow them, which is §12 working
as designed — `ABSCDJ6O` appears as the 7.0 adapter on 2026-07-19 and the 6.18
adapter on 2026-07-27, and the `BH00LL8O`/`BH00L4KU` pair was the 7.0 box's rig on
2026-07-27 and is the 6.18 box's rig on 2026-07-29. Confirmed, not transcription
errors. The *machines* never move, so "the same 6.18 kernel measured 605 ns on
2026-07-19" below compares like with like.

### The 2026-07-29 run — a HEAD binary on a Tier-3 rig, and which gaps it closes

Its `probe set` is **`01b257ece8c48470`, equal to the 7.0 baseline's**, so for the
first time the two kernels are comparable by the repository's own stated rule rather
than by an after-the-fact `git diff` of `probes.rs`. Both sides of that diff are
committed under [`docs/doctor/`](doctor/). Against the four limits the 2026-07-27 run
left open:

1. **Binary vintage — closed, and this is the gap where the field names promise
   more than the numbers deliver.** HEAD's P4 ran on 6.18 and reported
   `by_id_tree: present, count: 2, sysfs_only: 0, other_candidates: 0` with a
   canonical identity for each adapter. It is the *first* HEAD-vintage P4 ever run
   there, not a diff — the `fe1c52c` block had none of those three fields, only
   `count` — so one adapter then and two now is a hardware change, not a resolver
   one, and the three new fields have no 6.18 predecessor to compare against.
   What it genuinely buys: **the sysfs ancestor walk works on 6.18**, because
   `discover_adapters` derives each identity through `sysfs_lookup` and the two
   printed identities are its output — `usb:0403:6001:BH00LL8O:00` /
   `usb:0403:6001:BH00L4KU:00`, stopping at the FTDI node rather than a root hub,
   and agreeing with the serial and interface udev independently encoded in the link
   names. Adjacent positive: those serials are *populated and distinct* in 6.18's
   FTDI sysfs, where a blank one would have minted `usb:0403:6001:-:00` for both and
   made RES-1's hazard live on ordinary hardware.
   What it does **not** buy, and the distinction is sharper than it looks:
   `sysfs_only: 0` means no enumerated candidate both lacked a by-id link *and* was
   identified as USB — and with `other_candidates: 0` beside it, the no-by-id
   population is empty outright, so the population RES-2 exists for was not there.
   `enumerate_ports` merges its sysfs pass with `or_insert`,
   so a `sysfs_usb_devices()` returning nothing at all would have printed a
   byte-identical block. The `<sys>/class/tty` listing therefore **ran but is not
   witnessed** by this output, the no-udev fallback (by-id absent, identity from
   sysfs alone) is unexercised on **either** kernel, and the `/dev/serial/by-id`
   environment row's `degraded` middle arm — the actual RES-2 arm — has fired on
   neither, only under a fixture unit test. Nothing here speaks to RES-1 either:
   `find_usb`'s decline arm is on the *resolution* side, which no doctor code path
   reaches, and the two adapters carry different serials so the predicate could not
   fire anyway. **Closing the sysfs-only arm on 6.18 needs no hardware**: `--dev-root`
   reroots `/dev` *and* `/sys` (`sys_root = dev_root.join("sys")`), so a fixture tree
   carrying a USB tty with no by-id link fires arm 2 on that box directly.
2. **Rig tier — closed, and it relocates `brk = 0` to a different cause.** Tier 3:
   the pair verified in both directions, `rate_ladder=true` (9600, 115200 and the
   nonstandard rate, certified each way), `deliberate_mismatch_observed=true`, and
   per-port `custom_baud`/`break`/`icounter` plus `tiocexcl_refuses_second_open` on
   *two* real ports. The counters corroborate the shape rather than merely agreeing
   with it: the mismatch transmits one direction only, and P11 duly reads
   `tx=1452, frame=0` on `/dev/ttyUSB0` against `rx=197, frame=5` on `/dev/ttyUSB1`
   — `ttyUSB0` the 115200 transmitter, `ttyUSB1` the 9600 receiver that owns the
   frame errors. It is also the **first real-hardware rendering** of the 2026-07-28
   P11 fix: what an operator reads there is the `mismatch_pairs > 0` branch, the one
   `p11_blames_the_baud_mismatch_only_when_a_pair_was_certified` pins, and it could
   not have appeared before — the fix landed after both 2026-07-27 rig runs, macOS
   cannot certify a pair, and the 7.0 box had no adapter at that moment. (It got the
   pair back later the same day, so
   [`linux-7.0-2026-07-29-tier3.json`](doctor/linux-7.0-2026-07-29-tier3.json) is the
   second such rendering; this one is still the first.)
   **A Tier-3 certificate is not the whole of Tier 3.** Three items design §15.21
   and the plan's tiered checklist put at that tier are *not* discharged by it:
   break **reception** (see below), far-side **modem-line signalling** (the modem
   map is read one port at a time, peer closed), and **parity**-error observation —
   only a *baud* mismatch is implemented, where §15.21 says "baud and parity". Those
   belong to the checklist run this certificate is the precondition *for*, not to
   the certificate.
   One incidental confirmation worth having, because it is §12's whole premise
   happening by itself on real hardware: the pair's identity→path mapping on this
   box is the **reverse** of the 7.0 record above — `BH00LL8O` was `/dev/ttyUSB0`
   there and is the other node here. The paths swapped when the hardware moved
   between machines; the identities did not, and every node configured against one
   would still bind.
   **And `brk = 0` on both ports, where the earlier note blamed the tier for it.**
   It is structural. `p5_certify_port` computes `break_ok` as
   `set_break(true).is_ok() && set_break(false).is_ok()` — local ioctl acceptance —
   and `p5_certify_pair` transmits a rate ladder and a bulk mismatch pattern and
   **no break at all** — and the two phases that hold both ports open at once,
   discovery and the pair certificate, are exactly the two that never assert one. So
   nothing the doctor itself does can raise `brk`, at any tier, on any kernel.
   (The counter is still worth reading: it accumulates from driver bind, so a
   *nonzero* `brk` means something other than the doctor put a break on that line.)
   What Tier 3 did buy is break **assertion** confirmed on real 6.18 silicon.
   Break **reception**
   there is still unobserved, and what would observe it is a test rather than a
   probe — `p12_serial_exclusivity::a_break_straddled_by_a_replace_leaves_the_line_transmitting`,
   which needs `crossover_ports()`. That item therefore moved out of gap 2 and into
   gap 4 — and **attaching the rig was necessary but is not sufficient**: on Linux
   `crossover_ports()` reads `SNX_CROSSOVER_A`/`_B` and nothing else (the
   auto-detecting arm is `#[cfg(target_os = "macos")]`), so on the upgraded box that
   test still self-skips until those two variables are set.
   (The certificate's `modem[cts=false dsr=false dcd=false ri=false]` is a raw
   `TIOCMGET` read, printed and deliberately **not** folded into the verdict —
   `fail_if` covers only `custom_baud`, `break` and `icounter` — which is why P5 is
   `supported` with all four false. Do **not** read all-false as "a three-wire
   crossover with the handshake lines unwired": every modem read in the doctor
   happens with the *peer* port closed, so a low CTS is equally consistent with an
   unwired pair and with a peer whose DTR/RTS are low because nothing holds it open,
   and P11's own nonzero `cts` edge counters — 8 and 3 — make the unwired reading
   actively unsafe. The rig's handshake wiring is simply not established by this
   report.)
3. **The `jq` re-gate — narrowed, not closed.** Markdown again, no `--json`, so
   `serial-nexus-doctor --json | jq -e -f expectations/linux.jq` still has never been
   *executed* on 6.18. What changed is that the inference transfers again: every
   clause is decidable by eye from this report and every one holds, including the
   `.build.probe_set` and `.build.commit` clauses the `fe1c52c` artifact could not
   have answered — which is exactly why that older artifact would now fail a gate it
   once passed. One `--json` capture on that box closes this.
4. **`cargo test --workspace` on 6.18 — untouched.** Still zero executed tests
   there. It is now the only one of the four fully open, and it has acquired the
   break-reception item from gap 2.

### What the 2026-07-29 diff established

Against the three passive 7.0 runs in [`docs/doctor/`](doctor/) — same binary, same
commit, same fingerprint on both sides. (The 7.0 Tier-3 run came later the same day
and is a *different* binary at the same fingerprint; it is what supplies the
port-facing counterpart the last bullet used to say did not exist.)

- **P6 is byte-identical to all three 7.0 runs on every measured field**:
  `passes 64 / pollhup 64 / pollin 0 / bytes_read 0 / [EIO=64]`,
  `handler_reset_applied: true`, `handler_reset_readable_bytes: 1`, and the
  post-reset pass reading exactly 1 byte with
  `revents [POLLHUP=63, POLLIN|POLLHUP=1]`.
- **P7 is byte-identical**, all three session shapes, and **both `latch_covers_*`
  true on both kernels**.
- **P8 matches on every measured field too**, and its wall clock now agrees to
  within a millisecond — 136 idle / 69 hung-up on 6.18 against 136 / 68–69 across
  the three 7.0 runs, where the 2026-07-27 pair sat 10–25 ms apart. Putting the same
  binary on both sides collapsed that spread, so the "wall clock, not a kernel
  property" diagnosis was right. Do **not** promote `elapsed_ms` to evidence in
  either probe on the strength of that: it is 64 passes × a fixed pause, structurally
  forced on any box that completes them, and it still differs by 1 ms between two 7.0
  runs of one binary.
- **P1's three booleans and P2's five presence booleans are identical**, including
  `hup_when_never_opened: false` — §3.2's slave priming stays *mandatory* on 6.18.
- **P9's timed floor is *tighter* on 6.18 than on 7.0, on every row**: 1057 µs
  against 1066–1068 at 1 ms, 5059 against 5074–5082 at 5 ms, 10064 against
  10081–10174 at 10 ms; overshoot medians 57/59/64 against 66–68/74–82/81–174.
  `ready_passes_total: 0` on both, so neither measurement is contaminated.
- **P10 is settled, and the answer is "not a kernel property".** The two kernels
  **swapped shapes**: 6.18 now reads `11776 / 3584 / 15360 / 3` in both directions
  (the late-flip case, which was 7.0's reading in the previous diff) while 7.0 run 2
  reads `13824 / 0 / 13824 / 4` (the mid-fill case, which was 6.18's). Three
  sequential 7.0 runs produced both, plus a hostward `15360 / 0 / 15360 / 4`. A
  cross-kernel P10 difference is a scheduling artifact until several runs a side say
  otherwise — which is what the probe's own text says, now with the evidence to back
  it. *(It also corrected the probe's own text, which had printed "7.0 measured
  11776–13824 first-pass" — narrower on the first-pass side than what 7.0 has now
  been observed to do — while its `settled_bytes` doc comment blamed the 13824 case
  on "several doctors running concurrently", which run 2, sequential on an idle box,
  did not need. Both now report what has been measured. Neither is in the probe-set
  fingerprint, so archived artifacts stay comparable across the edit — checked.)*
- **P3, P4, P5 and P11 have no counterpart in *this* diff — and now have one beside
  it.** All four skip in the three passive runs, so the equal fingerprint makes the
  passive probes comparable and not these. The gap closed later the same day: the
  7.0 **Tier-3** report has all four on the same cross-wired pair at the same probe
  set, and it agrees. **P3 is field-for-field identical on both ports and both
  kernels** (`requested_baud`/`baud_readback` 250000, `custom_baud_ok`,
  `tiocexcl_refuses_second_open`, `modem_calls_ok`, `break_ok`,
  `tiocgicount_supported` all true). **P4 is identical** — `by_id_tree: present,
  count: 2, sysfs_only: 0, other_candidates: 0` and the same two canonical
  identities, so the sysfs ancestor walk derives the same answers on both. **P5's
  observation lines are identical**, pairing verified both ways with
  `rate_ladder=true deliberate_mismatch_observed=true` and the same per-port
  certificate string. **P11 agrees on ioctl availability and field set** and differs
  in absolute counts by construction — they accumulate from driver bind, so two
  boxes that have driven the same adapters for different lengths of time must
  differ. Note what this pair does *not* establish, for the same reason the 6.18
  report's gap 1 does not: both boxes see the by-id tree, so the no-udev fallback
  and the `/dev/serial/by-id` row's `degraded` middle arm remain unexercised on
  either kernel.

**The sub-microsecond outlier persists, and its named confounder is now excluded.**
The zero-timeout `poll(2)` cost reads 526 ns (P2, 4096 samples) and 1323 ns (P9, 16
samples) on 6.18, against 175–268 ns and 264–287 ns on the 7.0 box. The 2026-07-28
note offered "the 6.18 binary's build profile" as the one unexcluded explanation —
a debug build inflating exactly the sub-microsecond loops. **The 7.0 numbers above
are themselves a `target/debug` build**, so a debug-vs-release gap cannot be the
mechanism: whichever profile 6.18 was built with, it is being compared against
debug. What survives is a persistent ~2–5× difference in the bare cost of *asking*,
between two physically different machines, on top of a within-6.18 spread of
605 / 1162 / 526 ns for the same P2 code across the three runs. Box, most likely,
and not separable from a kernel effect without running both kernels on one machine.
It stays load-bearing on nothing: at `ACTIVE_POLL` = 200 µs even 1.323 µs per ask is
~0.7 % of a core per fd, and every shipped decision reads the 1 ms floor, where 6.18
agrees to within 1 %.

### Scope of the 2026-07-27 run — three limits a totals line does not show

*Historical.* All three were closed or narrowed by the 2026-07-29 run above; this
records what they were and why the Build block exists. **Do not read it as the
current state of the 6.18 evidence base** — read the two sections above for that.

1. **Binary vintage.** The run used a **`fe1c52c`-vintage** binary, not HEAD. Its
   P4 block is the pre-`RES-2` "by-id resolution ground truth" shape with no
   `by_id_tree` / `sysfs_only` / `other_candidates`, which is how the vintage was
   established at all — by eye, from a section title, after the fact. That is the
   accident the **Build** block above now removes: a report from either box would
   state its `commit` and its `probe set`, and two unequal fingerprints would have
   said "not comparable" in one glance. `git diff fe1c52c a2d3b96 -- doctor/src/probes.rs`
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
   `serial-nexus-doctor --json | jq -e -f expectations/linux.jq` has never been
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

*(Two of the four arrived on 2026-07-29 — a HEAD binary and the cross-wired pair.
`--json` and the suite did not. See "which gaps it closes" above.)*

### What the diff established

*Historical — the 2026-07-27 pair.* Superseded field by field by "What the
2026-07-29 diff established" above, which re-took every one of these against a
same-fingerprint baseline. Kept because it is where the P10 flip-scheduling reading
and the sub-microsecond outlier were first worked out.

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
  directions — which answers `serial-nexus-sys`'s standing "does this kernel account for
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

Unchanged by the 2026-07-29 re-run, which re-confirmed every one of these on a HEAD
binary against a same-fingerprint 7.0 baseline. Two kernels agreeing twice is still
not an argument for deleting anything.

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
the design's fallbacks remain live regardless. Re-run `serial-nexus-doctor --json | jq -e
-f expectations/linux.jq` on any new target — if P1 ever reports `degraded` that
is fine (poll backstop), but a P2 `unsupported` would be a real stop condition.

> **A caution for the next reader.** Each probe's per-run "Consequence" paragraph
> hardcodes 6.18 as the production kernel, so a report generated *on* 6.18 tells
> you to "diff this against the production kernel (6.18)" — i.e. against itself.
> Those strings are correct to leave alone (they are unconditional by design);
> read them against the `kernel` row of the report's own Environment table.
