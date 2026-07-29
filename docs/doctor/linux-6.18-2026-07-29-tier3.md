# nexus-doctor report

`nexus-doctor` v0.2.0 — paste this whole report into a support request.

## Build

| Field | Value |
|---|---|
| commit | `85699d66c5a5` |
| probe set | `01b257ece8c48470` |
| generated | 2026-07-29T00:15:16Z |

**Diffing this against another kernel?** Compare `probe set` first — an unequal fingerprint means the two runs do not ask the same questions, and the numbers below are not comparable field by field (§13).

## Environment

| Check | Value | Verdict |
|---|---|---|
| kernel | 6.18.14-1rodete4-amd64 | ✅ supported |
| os | Debian GNU/Linux rodete | ✅ supported |
| XDG_RUNTIME_DIR | /run/user/57251 | ✅ supported |
| /dev/serial/by-id | present (2 adapter(s)) | ✅ supported |
| user | costan | ✅ supported |
| group:dialout | member | ✅ supported |
| group:plugdev | member | ✅ supported |
| access:/dev/ttyUSB1 | read+write | ✅ supported |
| access:/dev/ttyUSB0 | read+write | ✅ supported |

## Probes

### P1 — EXTPROC / TIOCPKT signaling — ✅ supported

**Question:** Does a client tcsetattr surface as a TIOCPKT_IOCTL packet on the master; does clearing EXTPROC emit a final packet; can the master re-assert EXTPROC?

**Observed:**

- `ioctl_packet_on_tcsetattr`: true
- `clear_extproc_produces_packet`: true
- `reassert_extproc_via_master`: true

**Consequence:** EXTPROC packet-mode observation is primary; the §7.2 reconciliation poll is only a backstop.

### P2 — PTY presence / POLLHUP semantics — ✅ supported

**Question:** Does the master report POLLHUP only when no client holds the slave; does HUP clear on reopen; is termios settable with no slave open?

**Observed:**

- `hup_when_never_opened`: false
- `hup_while_open`: false
- `hup_after_close`: true
- `hup_after_reopen`: false
- `termios_settable_without_slave`: true
- `zero_timeout_poll_ns_median`: 526

**Consequence:** POLLHUP presence detection works; the master is a terminal (baseline applied natively), and the node primes the slave (open+close at creation) for the never-opened case.

### P4 — device identity resolution — ✅ supported

**Question:** Does the resolver's one source — the <sys>/class/tty listing plus a dependency-free sysfs walk, with /dev/serial/by-id as a fast path over it — yield the canonical usb:vid:pid:serial:iface identity (§12)?

**Observed:**

- `by_id_tree`: present
- `count`: 2
- `sysfs_only`: 0
- `other_candidates`: 0
- `usb-FTDI_FT232R_USB_UART_BH00LL8O-if00-port0`: usb:0403:6001:BH00LL8O:00
- `usb-FTDI_FT232R_USB_UART_BH00L4KU-if00-port0`: usb:0403:6001:BH00L4KU:00

**Consequence:** Resolver produces canonical identities; configs survive replug and cold start (§12).

### P3 — serial-port fit (/dev/ttyUSB0) — ✅ supported

**Question:** Custom baud acceptance, TIOCEXCL exclusivity, modem-line set/read, and break toggling on a real port (§7.1).

**Observed:**

- `requested_baud`: 250000
- `baud_readback`: 250000
- `custom_baud_ok`: true
- `tiocexcl_refuses_second_open`: true
- `modem_calls_ok`: true
- `break_ok`: true
- `tiocgicount_supported`: true

**Consequence:** serial2 fit confirmed; the daemon issues TIOCEXCL on the raw fd (serial2 sets O_NOCTTY only).

### P3 — serial-port fit (/dev/ttyUSB1) — ✅ supported

**Question:** Custom baud acceptance, TIOCEXCL exclusivity, modem-line set/read, and break toggling on a real port (§7.1).

**Observed:**

- `requested_baud`: 250000
- `baud_readback`: 250000
- `custom_baud_ok`: true
- `tiocexcl_refuses_second_open`: true
- `modem_calls_ok`: true
- `break_ok`: true
- `tiocgicount_supported`: true

**Consequence:** serial2 fit confirmed; the daemon issues TIOCEXCL on the raw fd (serial2 sets O_NOCTTY only).

### P5 — rig discovery and certification — ✅ supported

**Question:** Classify each named port (dangling/loopback/paired, both directions) and certify the rig for a tiered checklist run (§13, §15.21).

**Observed:**

- `usb:0403:6001:BH00L4KU:00`: paired with usb:0403:6001:BH00LL8O:00
- `usb:0403:6001:BH00LL8O:00`: paired with usb:0403:6001:BH00L4KU:00
- `usb:0403:6001:BH00L4KU:00 cert`: custom_baud=true break=true modem[cts=false dsr=false dcd=false ri=false] icounter=true
- `usb:0403:6001:BH00LL8O:00 cert`: custom_baud=true break=true modem[cts=false dsr=false dcd=false ri=false] icounter=true
- `usb:0403:6001:BH00L4KU:00 ↔ usb:0403:6001:BH00LL8O:00 cert`: rate_ladder=true deliberate_mismatch_observed=true

**Consequence:** Rig discovered and certified at **Tier 3** — 1 cross-wired pair, independent clocks, so the rate ladder and the deliberate baud mismatch ran. A tiered checklist run starts from this certificate (§15.21).

### P6 — pty-master readiness after the last slave closes — ✅ supported

**Question:** Once a pty's last slave fd closes, does the master keep asserting POLLIN with nothing to read (the shape that spins a close-triggered poll loop)?

**Observed:**

- `after_last_close`: bytes_read=0, elapsed_ms=132, passes=64, pollhup_passes=64, pollin_passes=0, pollin_with_no_data_passes=0, read_outcomes=[EIO=64], revents_seen=[POLLHUP=64]
- `handler_reset_applied`: true
- `handler_reset_readable_bytes`: 1
- `after_handler_termios_reset`: bytes_read=1, elapsed_ms=132, passes=64, pollhup_passes=64, pollin_passes=1, pollin_with_no_data_passes=0, read_outcomes=[EIO=63 bytes=1], revents_seen=[POLLHUP=63 POLLIN\|POLLHUP=1]

**Consequence:** POLLIN goes quiet after the last close on this kernel (64 passes, 0 with POLLIN, none readable-with-nothing-to-read): an ungated `closed`-only last-close arm would NOT spin on the hangup alone here, so pty.rs's `saw_session` latch is not what holds the anti-spin argument up on this kernel. The node's own last-close termios reset then re-armed readability 1 time(s) (1 byte(s)), so the drain in `pty.rs` that consumes that packet stays load-bearing regardless: without it the handler re-arms itself and the runaway returns by that route rather than through a stuck POLLIN. This is a per-kernel reading — §13 forbids acting on it until the production kernel (6.18) reports the same numbers, so diff this block before simplifying anything.

### P7 — evidence a collapsed client session leaves on the master — ✅ supported

**Question:** After a pty client hangs up, which session shapes (bare open/close, tcsetattr-only, one byte written) leave a readable packet on the packet-mode master?

**Observed:**

- `a_open_close`: bytes_readable_after_close=0, data_packet_seen=false, ioctl_bit_set=false, leading_bytes_hex=(none), reads=0, terminal_read=EIO
- `b_open_tcsetattr_close`: bytes_readable_after_close=1, data_packet_seen=false, ioctl_bit_set=true, leading_bytes_hex=[0x40], reads=1, terminal_read=EIO
- `c_open_write_close`: bytes_readable_after_close=2, data_packet_seen=true, ioctl_bit_set=false, leading_bytes_hex=[0x00], reads=1, terminal_read=EIO
- `latch_covers_termios_only_session`: true
- `latch_covers_data_session`: true

**Consequence:** A collapsed termios-only session leaves 1 byte(s) readable past the hangup (leading 0x40, ioctl bit true): pty.rs's widened last-close latch arms on it, so an `stty`/health-check/scripted client that opens, reconfigures and closes inside one poll gap still runs detach-release (§6). Diff this against the production kernel (6.18) before trusting the coverage there.

### P12 — session-boundary edge on a pty master — ⏭️ skipped (nexus-sys's SessionLatch is inert on this platform)

**Question:** Does an edge latch report a collapsed client session that left nothing readable on the master, and does it stay silent while idle?

**Consequence:** The session boundary is carried by the retained `TIOCPKT_IOCTL` packet here, which P7 measures — nothing is untested, only unmeasurable by this route (§15.39, §13).

### P8 — epoll vs read(2) on a pty master — ✅ supported

**Question:** Does epoll report a pty master readable while read(2) returns EAGAIN — the busy-loop shape that made the data plane use poll(2) instead (invariant 1, §15.18)?

**Observed:**

- `slave_open_idle`: bytes_read=0, elapsed_ms=136, epoll_events=0, epoll_flags_seen=[], epoll_ready_waits=0, epoll_wait_timeout_ms=1, epoll_waits=64, fionread_max=0, poll2_pollin_passes=0, read_outcomes=[EAGAIN=64], ready_then_eagain=0, ready_then_no_data=0, registration=level-triggered EPOLLIN, spin_ratio=0.0
- `after_slave_close`: bytes_read=0, elapsed_ms=69, epoll_events=64, epoll_flags_seen=[EPOLLHUP=64], epoll_ready_waits=64, epoll_wait_timeout_ms=1, epoll_waits=64, fionread_max=0, poll2_pollin_passes=0, read_outcomes=[EIO=64], ready_then_eagain=0, ready_then_no_data=64, registration=level-triggered EPOLLIN, spin_ratio=0.0
- `busy_loop_reproduced`: false
- `epoll_agrees_with_poll2`: true

**Consequence:** NOT reproduced at this layer: a bare level-triggered EPOLLIN registration agreed with poll(2) on this kernel (0 of 64 waits ready, poll(2) POLLIN on 0 passes, 0 reads answering EAGAIN after a ready report). Read that as scoped, not as a refutation — the starvation §15.18 records is a property of tokio's readiness guard (registration lifecycle + a synchronously-completing ready future), not of epoll_ctl alone, so invariant 1 stands and nothing here licenses putting epoll back in the data plane. After the last slave closed, the level-triggered set reported an event on 64 of 64 waits (64 of them with no bytes to read) — persistent readiness on a hung-up fd is expected and is why the PTY reader branches on POLLHUP rather than looping on readability. Diff both blocks against the production kernel (6.18) before drawing any conclusion from either.

### P9 — poll(2) timeout granularity — ✅ supported

**Question:** For a never-ready tty fd, what does a requested poll(2) timeout of 0/1/5/10 ms actually cost (min/median/max, µs)?

**Observed:**

- `poll_timeout_0ms`: max_us=2, median_ns=1323, median_us=1, min_us=1, overshoot_median_us=1, ready_passes=0, requested_ms=0, requested_us=0, samples=16
- `poll_timeout_1ms`: max_us=1062, median_ns=1057682, median_us=1057, min_us=1057, overshoot_median_us=57, ready_passes=0, requested_ms=1, requested_us=1000, samples=16
- `poll_timeout_5ms`: max_us=5068, median_ns=5059156, median_us=5059, min_us=5056, overshoot_median_us=59, ready_passes=0, requested_ms=5, requested_us=5000, samples=16
- `poll_timeout_10ms`: max_us=10087, median_ns=10064492, median_us=10064, min_us=10058, overshoot_median_us=64, ready_passes=0, requested_ms=10, requested_us=10000, samples=16
- `median_us_for_1ms_request`: 1057
- `median_ns_for_0ms_request`: 1323
- `ready_passes_total`: 0

**Consequence:** A zero timeout costs 1323 ns median (the cost of asking) and a requested 1 ms costs 1057 µs median on this kernel — that is the floor §15.19's hybrid data plane was built around and the floor poll_ready's idle backoff steps against. 16 samples per timeout: enough to see the floor, not enough to characterize a tail. Diff these against the production kernel (6.18) before tuning any backoff step or timer against them.

### P10 — pty buffer depth — ✅ supported

**Question:** How many bytes does a pty accept in each direction before it would block, with nothing draining the other end?

**Observed:**

- `master_to_slave_targetward`: bytes_accepted_before_eagain=11776, ceiling_bytes=4194304, ceiling_hit=false, chunk_bytes=4096, peer_pending_input_bytes=4095, pending_output_bytes=0, settle_ms=20, settled_extra_bytes=3584, settled_extra_writes=1, terminal_write=EAGAIN, terminal_write_after_settle=EAGAIN, total_bytes_accepted=15360, writes=3
- `slave_to_master_hostward`: bytes_accepted_before_eagain=11776, ceiling_bytes=4194304, ceiling_hit=false, chunk_bytes=4096, peer_pending_input_bytes=4095, pending_output_bytes=0, settle_ms=20, settled_extra_bytes=3584, settled_extra_writes=1, terminal_write=EAGAIN, terminal_write_after_settle=EAGAIN, total_bytes_accepted=15360, writes=3

**Consequence:** This kernel's pty accepts 11776 byte(s) master→slave (targetward, what a pty node writes to its client) and 11776 byte(s) slave→master (hostward, what the node reads) before answering EAGAIN, reaching 15360 and 15360 in total once a short pause has let the tty's asynchronous flip work run. Read the daemon's `hostward_buffer` defaults against the SCALE of these, not their last byte: the pty default is 32 chunks, and a queue far larger than the kernel pipe below it only defers the same backpressure. Both figures move by a chunk or two run to run depending on when that flip work lands (7.0 measured 11776–13824 first-pass, 13824–15360 total), so across kernels a one-chunk difference is noise and only an order-of-magnitude one is signal. Numbers, not a verdict — diff them against the production kernel (6.18) before changing a default.

### P11 — real-port line-state counters — ✅ supported

**Question:** Do TIOCGICOUNT (driver error/edge counters) and TIOCMGET (modem lines) answer on a real port, and what do they currently read (§5, §7.1)?

**Observed:**

- `/dev/ttyUSB0`: counters=[brk=0 buf_overrun=0 cts=8 dcd=0 dsr=0 frame=0 overrun=0 parity=0 rng=0 rx=164 tx=1452], modem_bits_hex=0x0006, modem_lines_asserted=[DTR RTS], tiocgicount_available=true, tiocmget_available=true
- `/dev/ttyUSB1`: counters=[brk=0 buf_overrun=0 cts=3 dcd=0 dsr=0 frame=5 overrun=0 parity=0 rng=0 rx=197 tx=164], modem_bits_hex=0x0006, modem_lines_asserted=[DTR RTS], tiocgicount_available=true, tiocmget_available=true

**Consequence:** Both line-state ioctls answer on 2 of 2 named port(s): the driver counters (§5, §7.1) and the modem lines are readable, so serial state carries real error/overrun accounting rather than omitting it. Read the counts as a snapshot of a cumulative total, not as a measurement of this run — they count since the driver bound the device, and P3/P5 transmit on these same ports earlier in the same invocation (a nonzero `frame` here is usually P5's deliberate baud-mismatch item, not a fault). Across kernels, diff the ioctl *availability* and the field set; the absolute counts differ by construction.

## Summary

21 supported · 0 degraded · 0 unsupported · 1 skipped
