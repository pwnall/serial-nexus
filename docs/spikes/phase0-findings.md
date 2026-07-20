# Phase 0 — Spike findings

**Kernel of record:** `7.0.0-28-generic` (x86_64). **Rust:** 1.97.1, edition 2024.
**Adapter of record:** FTDI FT232R, `usb:0403:6001:ABSCDJ6O:00`.

> **Kernel support target: Linux 6.18.** These verdicts are from kernel 7.0.0.
> Every mechanism relied on is long-stable Linux (EXTPROC/TIOCPKT since ~3.8,
> pty POLLHUP semantics, `ptsname_r`, `TIOCGICOUNT`, serial2 `termios2`/`BOTHER`
> custom baud), so 6.18 is expected to behave identically — but the spikes are
> re-runnable and record the kernel, so **re-run S1/S2/S4 on the 6.18 box and
> compare** before any kernel-dependent one-way decision (phases 2 and 7). The
> design's fallbacks (§7.2 reconciliation poll; S2 slave-priming) are kept live
> rather than deleted on the strength of a 7.0 pass.

Each spike is a self-judging binary under `spikes/` that prints one JSON verdict
line and exits nonzero on a mismatch with the design (plan §1: a nonzero spike
is a stop condition prompting a design amendment). Re-run any with
`cargo run -p spikes --bin <name>`.

**Bottom line:** every design assumption held or was refined without contradiction.
No `§15` amendment is required; two implementation notes (below) are carried into
phases 2 and 7.

---

## S1 — EXTPROC / TIOCPKT (§7.2, §15.14) — PASS

The design's single most-flagged mechanism. On the kernel of record:

- A client `tcsetattr` on the slave, with `EXTPROC` set and packet mode
  (`TIOCPKT`) on the master, **does** surface as a `TIOCPKT_IOCTL` (0x40)
  control packet on the master read (`ioctl_packet_on_tcsetattr: true`).
- Clearing `EXTPROC` produces a final control packet (`true`).
- The daemon **can** re-assert `EXTPROC` through the master fd
  (`reassert_extproc_via_master: true`).

⇒ The observe-client-termios mechanism works as designed; the §7.2 reconciliation
poll remains only a backstop, not the primary path. `TIOCPKT_IOCTL` is **not**
exported by libc 0.2.186 (only the `TIOCPKT` request code is) — the constant
`64` is spelled out in a localized `sys` module.

## S2 — PTY presence / POLLHUP (§7.2) — PASS (with one refinement)

- Slave open ⇒ master **not** HUP (`hup_while_open: false`, "present").
- Last close ⇒ master **HUP** (`hup_after_close: true`, "absent").
- Reopen ⇒ HUP clears (`hup_after_reopen: false`).
- Termios is settable through the master with **no slave open**
  (`termios_settable_without_slave: true`) — the last-close baseline reset works.
- Zero-timeout HUP poll costs **~305 ns median** — the design's "effectively
  free" is confirmed.

**Refinement (→ phase 2):** a master whose slave was **never opened** does *not*
report `POLLHUP` (`hup_when_never_opened: false`); HUP only appears after the
first open→close. So HUP alone cannot represent the initial no-client state.
**Implementation note:** at PTY node creation, open then immediately close the
slave once to *prime* the HUP state to "absent". Presence detection then becomes
uniform (HUP set = absent, HUP clear = a real opener), matching §7.2's model.
This refines, but does not contradict, the design.

## S3 — serial2 fit (§7.1, §13) — WRITTEN; hardware run pending access

Compiles and runs; against `/dev/ttyUSB0` it currently **skips** with
`permission denied` because `pwnall` is not in `dialout` and the node is
`root:dialout 660`. Grant access (udev `GROUP=plugdev`, or add to `dialout`) and
re-run `cargo run -p spikes --bin s3_serial2` — optionally `--loopback` with a
TX↔RX jumper, or `--watch-unplug` to capture the physical-removal error surface.

Verified from source (research) and encoded in the spike:

- **serial2 sets `O_NOCTTY` + `O_NONBLOCK` but NOT `TIOCEXCL`.** The daemon must
  issue `TIOCEXCL` itself on the raw fd; the spike then proves a second open is
  refused with `EBUSY`.
- **serial2-tokio 0.1.24 exposes no accessor for the inner fd**, so raw ioctls
  (`TIOCEXCL`, `TIOCGICOUNT`, `TIOCPKT`) can't be issued on the async wrapper.
  Custom baud works on Linux (termios2/`BOTHER`) via `Settings::set_baud_rate`.

**Architecture note (→ phase 2):** the serial node will open a **blocking
`serial2::SerialPort`** (for settings, modem lines, break, and raw ioctls via
`as_raw_fd`) and drive async I/O by registering the fd with
`tokio::io::unix::AsyncFd`, rather than using serial2-tokio — precisely because
serial2-tokio hides the fd we need for `TIOCEXCL`/`TIOCGICOUNT`. Consistent with
§13's "raw termios via nix/rustix as the fallback."

## S4 — resolver ground truth (§12) — PASS

Against the real adapter, the resolver yields the exact canonical identity
**`usb:0403:6001:ABSCDJ6O:00`**.

- `/dev/serial/by-id` enumerates adapters and resolves the `/dev/tty*` path; its
  **name is ambiguous to parse** (vendor/model strings contain underscores).
- The **authoritative numeric identity** comes from a dependency-free sysfs
  *ancestor walk* from `/sys/class/tty/<dev>/device`: the nearest ancestor with
  `bInterfaceNumber` is the interface, the first ancestor with `idVendor` is the
  device (stop there, or you bind the root hub). Nesting depth differs between
  ttyUSB and ttyACM, so the walk must not assume a fixed number of parents.
- `by-path` provides the topology fallback (`pci-…-port0`) for the §12 fallback
  chain.

The `--dev-root` seam (§3) is present for fixture trees. This ancestor-walk is
the reusable core of the phase-7 resolver.

## S5 — RPC skeleton (§10) — PASS

Newline-delimited JSON-RPC 2.0 over a Unix socket pair round-trips a
request/response and a `subscribe`-style notification, and rejects batch arrays
with `-32600`. This **fixes the `nexus-rpc` type shapes** (`Request`,
`Response`, `Notification`, `RpcError`, `Id`, `V2`, `parse_incoming_request`),
now the stable §15.16 surface with unit tests.
