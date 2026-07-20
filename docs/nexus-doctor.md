# nexus-doctor — capability checker

`nexus-doctor` is the shipping diagnostic for serial_nexus (design §15.17). It
consolidates every kernel-behavior probe the design depends on, plus environment
checks, into one binary that emits a copy-pasteable **Markdown** report (the
expected first attachment on any support request) with a **`--json`** twin for
CI. It supersedes the throwaway per-spike binaries of the v1 plan.

```
nexus-doctor              # Markdown report on stdout (default)
nexus-doctor --json       # JSON twin for CI: nexus-doctor --json | jq -e -f expectations/linux.jq
nexus-doctor --port /dev/ttyUSB0   # opt a real port into P3 (serial fit)
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

A probe verdict of `unsupported` fails the process (exit 1) — a stop condition:
surface the report for a design amendment rather than coding around it (plan §1).
`skipped` and `degraded` exit 0. Hardware tiers (dangling converter → TX/RX
jumper → cross-wired null modem) are the §13 no-target doctrine; Tier 1 (a
dangling converter, no receiver) already exercises identity, exclusivity, and
lifecycle.

## Kernel-of-record report (Linux 7.0.0-28-generic, x86_64)

Rust 1.97.1, edition 2024. Adapter: FTDI FT232R `usb:0403:6001:ABSCDJ6O:00`.

- **P1 — supported.** `ioctl_packet_on_tcsetattr`, `clear_extproc_produces_packet`,
  `reassert_extproc_via_master` all true. EXTPROC observation is primary; poll is
  the backstop.
- **P2 — supported.** HUP absent while open, present after close, clears on
  reopen; termios settable with no slave; zero-timeout poll ≈ sub-µs.
  **Refinement:** a master whose slave was *never opened* does **not** report
  POLLHUP — at PTY node creation, open+close the slave once to prime it.
- **P3 — skipped** (device access pending; `pwnall` not in `dialout`). Grant via
  a udev `GROUP=plugdev` rule or dialout membership and re-run with `--port`.
  Verified from source: **serial2 sets `O_NOCTTY` but not `TIOCEXCL`**, so the
  daemon issues it on the raw fd; **serial2-tokio hides the fd**, so the serial
  node uses blocking `serial2` + `tokio AsyncFd` (§13 fallback).
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
