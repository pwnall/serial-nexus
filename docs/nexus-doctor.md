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
| **P6** | pty-master readiness after the last slave closes: once the last slave fd closes, does the master keep asserting POLLIN with nothing to read — the shape that spins a close-triggered poll loop? Also whether the node's own last-close termios reset re-arms readability. (§7.2, §15.36) | `supported` → the numbers are recorded; a kernel that reads differently is `degraded` **with the observation named**. Diff this block before simplifying `pty.rs`'s `saw_session` latch or its last-close drain. |
| **P7** | Evidence a collapsed client session leaves on the master: which session shapes (bare open/close, `tcsetattr`-only, one byte written) leave a readable packet, and whether the presence latch covers each. (§7.2's session-evidence rule, §15.36 F4) | `supported` → the latch's premise holds here. The `latch_covers_*` observations are the ones to compare across kernels. |
| **P8** | epoll vs `read(2)` on a pty master: does epoll report the master readable while `read` returns EAGAIN — the busy-loop shape that put the data plane on `poll(2)`? Probed with **raw epoll**, never `AsyncFd`. (invariant 1, §15.18) | `supported` → invariant 1's premise measured, not assumed (`spin_ratio`, `busy_loop_reproduced`, `epoll_agrees_with_poll2`). |
| **P9** | `poll(2)` timeout granularity: for a never-ready tty fd, what a requested 0/1/5/10 ms timeout actually costs (min/median/max µs, and the overshoot). (§15.19's timer floor) | `supported` → the adaptive backoff's constants have measured ground under them on this kernel. |
| **P10** | pty buffer depth: how many bytes a pty accepts in each direction before it would block with nothing draining the far end. (§5 boundary policy, §15.19) | `supported` → the depth every backpressure argument in §5 rests on, in numbers. |
| **P11** | Real-port line-state counters: do `TIOCGICOUNT` (driver error/edge counters) and `TIOCMGET` (modem lines) answer on a real port, and what do they read? (§5, §7.1) | `supported` / `degraded` (counters absent — macOS has no `TIOCGICOUNT`) / `skipped` (no `--port`). **Opt-in for the same reason as P3/P5: opening a port toggles DTR.** |

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
- **`supported`** — discovered and certified. On a non-UART (the CI pts sim)
  characterization reports `skipped (not a UART)` and records **no** failure, by
  §15.21's design, so P5's logic never waits for a bench.

The negative-control ritual therefore means what it says: pull one wire, re-run
P5, and the asymmetry is named at discovery *and* whatever it broke in the
certificate is named in the verdict.

## Kernel-of-record report (Linux 7.0.0-28-generic, x86_64) — P1–P4 as of 2026-07-19

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

**P6–P11 on this kernel are not transcribed here** — they emit raw numbers that
change per box and per run, so the report itself is the record. Capture them with
`nexus-doctor --json > doctor-7.0.json` and diff against the target kernel; the
per-probe "Consequence" paragraphs in the report say what each number licenses.

## Confirmed on Linux 6.18 (P1–P4 only, 2026-07-19)

serial_nexus must run on **Linux 6.18**, older than the 7.0 dev box. Confirmed
2026-07-19 on `6.18.14-1rodete4-amd64` (Debian GNU/Linux rodete), FTDI FT232R
`usb:0403:6001:ABSCDGL6:00`:

**P1–P4 all `supported` — 12 supported · 0 degraded · 0 unsupported · 0 skipped
across that probe set; zero deltas from 7.0.**

> **P5–P11 have NOT been run on 6.18** — that is *seven of the binary's eleven
> probes*, and the totals above do not correspond to any run of the current
> binary (a 7.0 run today reports 17 supported · 3 skipped). P5 did not exist
> yet: this run is `e93149d` (2026-07-19) and rig discovery/certification landed
> in `aef797f` two days later, so a reader concluding that the rate ladder, break
> reception and per-port `TIOCGICOUNT` support are confirmed on the production
> kernel would be reading a claim nobody made. P6–P11 came a week later
> (2026-07-26/27), added *specifically* so this diff could be taken. The
> "kernel-of-record (7.0)" section above is likewise P1–P4.
>
> This matters because P6 and P7 are exactly the probes whose output says "diff
> this block before simplifying anything", and AGENTS.md §7 requires a pause
> before any one-way decision resting on a kernel ability confirmed only on 7.0.
> Close it with, on a 6.18 box:
>
> ```sh
> nexus-doctor --json > doctor-6.18.json     # then diff the P6–P11 observation
> nexus-doctor --json | jq -e -f expectations/linux.jq   # blocks on `unsupported`
> ```
>
> and replace this note with the numbers.

- **P1 supported** — EXTPROC packet-mode signaling behaves identically; the
  primary observation path works, poll stays a backstop.
- **P2 supported** — HUP semantics byte-identical, including
  `hup_when_never_opened == false` (so the slave-priming refinement transfers);
  zero-timeout poll ≈ 605 ns.
- **P4 supported** — the sysfs ancestor-walk resolves the canonical identity on
  Debian too.
- **P3 supported** — custom baud (exact), `TIOCEXCL`, modem lines, break, and
  `TIOCGICOUNT` all confirmed on real hardware.

So the kernel-sensitive mechanics (EXTPROC observation, POLLHUP presence) are
de-risked across the matrix; the design's fallbacks remain live regardless. Re-run
`nexus-doctor --json | jq -e -f expectations/linux.jq` on any new target — if P1
ever reports `degraded` that is fine (poll backstop), but a P2 `unsupported`
would be a real stop condition.
