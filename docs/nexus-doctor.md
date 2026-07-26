# nexus-doctor — capability checker

`nexus-doctor` is the shipping diagnostic for serial_nexus (design §15.17). It
consolidates every kernel-behavior probe the design depends on, plus environment
checks, into one binary that emits a copy-pasteable **Markdown** report (the
expected first attachment on any support request) with a **`--json`** twin for
CI. It supersedes the throwaway per-spike binaries of the v1 plan.

```
nexus-doctor              # Markdown report on stdout (default)
nexus-doctor --json       # JSON twin for CI: nexus-doctor --json | jq -e -f expectations/linux.jq
nexus-doctor --port /dev/ttyUSB0   # opt a real port into P3 (serial fit) and P5 (rig)
nexus-doctor --port /dev/ttyUSB0 --port /dev/ttyUSB1   # repeatable: P5 classifies the set
nexus-doctor --dev-root ./fixtures # fixture by-id tree (test seam, §3)
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
| **P4** | by-id resolution: does `/dev/serial/by-id` + a dependency-free sysfs walk yield `usb:vid:pid:serial:iface`? (§12) | `supported` / `degraded` (by-path only, no serial) / `skipped` (no adapter). |
| **P5** | Rig discovery **and certification**: classify every named port (dangling / loopback / paired, both directions) and certify it — break, custom baud, counter support per port; a rate ladder including a nonstandard rate and a deliberate baud mismatch per pair. (§13, §15.21) | `supported` (discovered and certified) / `degraded` (miswired, or an uncharacterized item) / `unsupported` (data-integrity failure) / `skipped` (no `--port`). |

A probe verdict of `unsupported` fails the process (exit 1) — a stop condition:
surface the report for a design amendment rather than coding around it (plan §1).
`skipped` and `degraded` exit 0. Hardware tiers (dangling converter → TX/RX
jumper → cross-wired null modem) are the §13 no-target doctrine; Tier 1 (a
dangling converter, no receiver) already exercises identity, exclusivity, and
lifecycle.

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

## Kernel-of-record report (Linux 7.0.0-28-generic, x86_64)

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

## Confirmed on Linux 6.18 (Debian rodete)

serial_nexus must run on **Linux 6.18**, older than the 7.0 dev box. Confirmed
2026-07-19 on `6.18.14-1rodete4-amd64` (Debian GNU/Linux rodete), FTDI FT232R
`usb:0403:6001:ABSCDGL6:00`:

**All probes `supported` — 12 supported · 0 degraded · 0 unsupported · 0 skipped;
zero deltas from 7.0.**

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
