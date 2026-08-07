# serial-nexus-doctor report

`serial-nexus-doctor` v0.3.0 — paste this whole report into a support request.

## Build

| Field | Value |
|---|---|
| commit | `3e23c524184c` |
| probe set | `82a8e2198e54626a` |
| field set (this run) | `179c9d15c6e450f5` |
| generated | 2026-08-07T00:25:53Z |

**Diffing this against another kernel?** Compare both digests. An unequal `probe set` means the two runs do not ask the same questions and the numbers below are not comparable at all (§13). An **equal** `probe set` is not a green light: it digests the question *text*, not the code that asks it. `field set` is the one that answers "same cells?" — equal means every observation present in one report is present in the other, unequal means diff only their intersection. Neither digest can see a probe body that changed a number without changing a key.

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
- `zero_timeout_poll_ns_median`: 658

**Consequence:** POLLHUP presence detection works; the master is a terminal (baseline applied natively), and the node primes the slave (open+close at creation) for the never-opened case.

### P4 — device identity resolution — ✅ supported

**Question:** Does the resolver's one source — the <sys>/class/tty listing plus a dependency-free sysfs walk, with /dev/serial/by-id as a fast path over it — yield the canonical usb:vid:pid:serial:iface identity (§12)?

**Observed:**

- `by_id_tree`: present
- `sysfs_tty_listing`: present
- `count`: 2
- `sysfs_only`: 0
- `other_candidates`: 0
- `canonical`: 2
- `topology_only`: 0
- `unidentified`: 0
- `usb-FTDI_FT232R_USB_UART_BH00L4KU-if00-port0`: usb:0403:6001:BH00L4KU:00
- `usb-FTDI_FT232R_USB_UART_BH00LL8O-if00-port0`: usb:0403:6001:BH00LL8O:00

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

- `usb:0403:6001:BH00LL8O:00`: paired with usb:0403:6001:BH00L4KU:00
- `usb:0403:6001:BH00L4KU:00`: paired with usb:0403:6001:BH00LL8O:00
- `usb:0403:6001:BH00LL8O:00 cert`: custom_baud=true break=true modem[cts=false dsr=false dcd=false ri=false] icounter=true
- `usb:0403:6001:BH00L4KU:00 cert`: custom_baud=true break=true modem[cts=false dsr=false dcd=false ri=false] icounter=true
- `usb:0403:6001:BH00LL8O:00 ↔ usb:0403:6001:BH00L4KU:00 cert`: rate_ladder=true deliberate_mismatch_observed=true
- `usb:0403:6001:BH00LL8O:00 ↔ usb:0403:6001:BH00L4KU:00 handshake`: 5-wire crossover: RTS/CTS both ways, DTR moves nothing [rts_a_to_cts_b=true rts_b_to_cts_a=true dtr_a_to_dsr_b=false dtr_a_to_dcd_b=false dtr_a_to_ri_b=false dtr_b_to_dsr_a=false]

**Consequence:** Rig discovered and certified at **Tier 3** — 1 cross-wired pair, independent clocks, so the rate ladder and the deliberate baud mismatch ran. A tiered checklist run starts from this certificate (§15.21).

### P6 — pty-master readiness after the last slave closes — ✅ supported

**Question:** Once a pty's last slave fd closes, does the master keep asserting POLLIN with nothing to read (the shape that spins a close-triggered poll loop)?

**Observed:**

- `after_last_close`: bytes_read=0, elapsed_ms=133, passes=64, pollhup_passes=64, pollin_passes=0, pollin_with_no_data_passes=0, read_outcomes=[EIO=64], revents_seen=[POLLHUP=64]
- `client_session_baseline`: baseline_packet_bytes_drained=1, baseline_via_master=true, extproc_set_at_shape=true, reasserted_on_client_slave=true, slave_termios_mode=raw
- `handler_reset_applied`: true
- `handler_reset_extproc_retained`: true
- `handler_reset_path_probe`: master
- `handler_reset_path_node`: master
- `handler_reset_readable_bytes`: 1
- `after_handler_termios_reset`: bytes_read=1, elapsed_ms=132, passes=64, pollhup_passes=64, pollin_passes=1, pollin_with_no_data_passes=0, read_outcomes=[EIO=63 bytes=1], revents_seen=[POLLHUP=63 POLLIN\|POLLHUP=1]

**Consequence:** POLLIN goes quiet after the last close on this kernel (64 passes, 0 with POLLIN, none readable-with-nothing-to-read): an ungated `closed`-only last-close arm would NOT spin on the hangup alone here, so pty.rs's `saw_session` latch is not what holds the anti-spin argument up on this kernel. The node's own last-close termios reset then re-armed readability 1 time(s) (1 byte(s)), so the drain in `pty.rs` that consumes that packet stays load-bearing regardless: without it the handler re-arms itself and the runaway returns by that route rather than through a stuck POLLIN. This is a per-kernel reading — §13 forbids acting on it until the production kernel (6.18) reports the same numbers, so diff this block before simplifying anything. The client session was measured with the pair in `raw` and EXTPROC set — the §7.2 baseline re-asserted on the client's own slave, having reached the pair through the master at setup — so this reading is of the daemon's pty and not of whatever discipline the kernel left behind.

### P7 — evidence a collapsed client session leaves on the master — ✅ supported

**Question:** After a pty client hangs up, which session shapes (bare open/close, tcsetattr-only, one byte written) leave a readable packet on the packet-mode master?

**Observed:**

- `a_open_close`: baseline_packet_bytes_drained=1, baseline_via_master=true, bytes_readable_after_close=0, data_packet_seen=false, extproc_set_at_shape=true, ioctl_bit_set=false, leading_bytes_hex=(none), reads=0, reasserted_on_client_slave=true, slave_termios_mode=raw, terminal_read=EIO
- `b_open_tcsetattr_close`: baseline_packet_bytes_drained=1, baseline_via_master=true, bytes_readable_after_close=1, data_packet_seen=false, extproc_set_at_shape=true, ioctl_bit_set=true, leading_bytes_hex=[0x40], reads=1, reasserted_on_client_slave=true, slave_termios_mode=raw, terminal_read=EIO
- `c_open_write_close`: baseline_packet_bytes_drained=1, baseline_via_master=true, bytes_readable_after_close=2, data_packet_seen=true, extproc_set_at_shape=true, ioctl_bit_set=false, leading_bytes_hex=[0x00], reads=1, reasserted_on_client_slave=true, slave_termios_mode=raw, terminal_read=EIO
- `latch_covers_termios_only_session`: true
- `latch_covers_data_session`: true
- `extproc_retained_at_shape`: true
- `measured_in_daemon_baseline`: true
- `silence_cause`: covered

**Consequence:** A collapsed termios-only session leaves 1 byte(s) readable past the hangup (leading 0x40, ioctl bit true): pty.rs's widened last-close latch arms on it, so an `stty`/health-check/scripted client that opens, reconfigures and closes inside one poll gap still runs detach-release (§6). Diff this against the production kernel (6.18) before trusting the coverage there. Measured with the pair in `raw`, EXTPROC set, the §7.2 baseline re-asserted on the client's own slave (applied) and reaching the pair through the master at setup; the re-assert's own 1 byte(s) were drained before the session ran, so nothing below is the probe's own footprint.

### P12 — session-boundary edge on a pty master — ⏭️ skipped (serial-nexus-sys's SessionLatch is inert on this platform)

**Question:** Does an edge latch report a collapsed client session that left nothing readable on the master, and does it stay silent while idle?

**Observed:**

- `a_open_close_edge`: false
- `b_open_tcsetattr_close_edge`: false
- `c_open_write_close_edge`: false
- `idle_edges_in_200_passes`: 0
- `idle_window_tight`: edges=0, elapsed_us=664, pass_pause_us=0, passes=200, poll_event_passes=200, read_outcomes=[EIO=200]
- `idle_window_paced`: edges=0, elapsed_us=325311, pass_pause_us=5000, passes=64, poll_event_passes=64, read_outcomes=[EIO=64]
- `live_session_window`: edges=0, elapsed_us=325307, pass_pause_us=5000, passes=64, poll_event_passes=0, read_outcomes=[EAGAIN=64]
- `control_session_edge`: false
- `control_session_edge_pass`: null
- `control_session_edge_us`: 81152

**Consequence:** The session boundary is carried by the retained `TIOCPKT_IOCTL` packet here, which P7 measures — nothing is untested, only unmeasurable by this route (§15.39, §13). The windows above ran anyway and are reported: `control_session_edge: false` beside 264 executed passes over 325975 us is this platform's inert arm proving itself inert, and a Linux report where that field read `true` would mean the latch had grown a second implementation nobody measured.

### P13 — disposition of unread client bytes at a pts last close — ✅ supported

**Question:** When a pty client writes bytes the master has not read and then closes, does this kernel retain them, discard them, or block the close waiting for the reader?

**Observed:**

- `a_no_reader_blocking_slave`: baseline_packet_bytes=1, bytes_lost=0, bytes_recovered_after_close=64, bytes_recovered_before_close=0, bytes_recovered_total=64, bytes_written_by_slave=64, close_microseconds=23, slave_termios_mode=raw, terminal_read=EIO
- `b_reader_drains_before_close`: baseline_packet_bytes=1, bytes_lost=0, bytes_recovered_after_close=0, bytes_recovered_before_close=64, bytes_recovered_total=64, bytes_written_by_slave=64, close_microseconds=21, slave_termios_mode=raw, terminal_read=EIO
- `c_no_reader_nonblocking_slave`: baseline_packet_bytes=1, bytes_lost=0, bytes_recovered_after_close=64, bytes_recovered_before_close=0, bytes_recovered_total=64, bytes_written_by_slave=64, close_microseconds=27, slave_termios_mode=raw, terminal_read=EIO
- `d_no_reader_second_fd_held`: baseline_packet_bytes=1, bytes_lost=0, bytes_recovered_after_close=64, bytes_recovered_before_close=0, bytes_recovered_total=64, bytes_written_by_slave=64, close_microseconds=23, slave_termios_mode=raw, terminal_read=EAGAIN
- `policy`: retains
- `close_waits_for_reader`: false

**Consequence:** This kernel **retains** bytes a pts client wrote but the master never read: with no reader, 64 of 64 byte(s) survive the last close and `close(2)` takes 23 µs (terminal read `EIO`). With a master that drains before the close, 64 of 64 byte(s) are recovered and the close takes 21 µs — the healthy-reader case, and the one the daemon is in. Numbers, not a verdict — every policy is legitimate and the daemon is correct under each (§7.2 drains before finalizing a close, §5 accounts what it reads). Read it for two things: a cross-kernel diff, and the reason a harness reads a byte counter while its client is still open rather than after (notes §3.29). A `waits-then-*` kernel additionally means a lost byte implies a reader stalled for the whole timeout, not a lost microsecond race. **The last-close reference count** is measured too, by holding a second fd on the same pts across the writer's close (`d_no_reader_second_fd_held` against `a_no_reader_blocking_slave`): 64 of 64 byte(s) survive with the witness held against 64 without it, and the terminal read is `EAGAIN` against `EIO`. Compare the two rows rather than reading either alone — that pair is the whole measurement, and if *neither* the bytes nor the terminal move on some kernel, a held fd buys nothing there and the harness rule that depends on it (notes §3.56) is resting on nothing.

### P8 — epoll vs read(2) on a pty master — ✅ supported

**Question:** Does epoll report a pty master readable while read(2) returns EAGAIN — the busy-loop shape that made the data plane use poll(2) instead (invariant 1, §15.18)?

**Observed:**

- `slave_open_idle`: bytes_read=0, elapsed_ms=137, epoll_events=0, epoll_flags_seen=[], epoll_ready_waits=0, epoll_wait_timeout_ms=1, epoll_waits=64, fionread_max=0, poll2_pollin_passes=0, read_outcomes=[EAGAIN=64], ready_then_eagain=0, ready_then_no_data=0, registration=level-triggered EPOLLIN, spin_ratio=0.0
- `after_slave_close`: bytes_read=0, elapsed_ms=68, epoll_events=64, epoll_flags_seen=[EPOLLHUP=64], epoll_ready_waits=64, epoll_wait_timeout_ms=1, epoll_waits=64, fionread_max=0, poll2_pollin_passes=0, read_outcomes=[EIO=64], ready_then_eagain=0, ready_then_no_data=64, registration=level-triggered EPOLLIN, spin_ratio=0.0
- `busy_loop_reproduced`: false
- `epoll_agrees_with_poll2`: true

**Consequence:** NOT reproduced at this layer: a bare level-triggered EPOLLIN registration agreed with poll(2) on this kernel (0 of 64 waits ready, poll(2) POLLIN on 0 passes, 0 reads answering EAGAIN after a ready report). Read that as scoped, not as a refutation — the starvation §15.18 records is a property of tokio's readiness guard (registration lifecycle + a synchronously-completing ready future), not of epoll_ctl alone, so invariant 1 stands and nothing here licenses putting epoll back in the data plane. After the last slave closed, the level-triggered set reported an event on 64 of 64 waits (64 of them with no bytes to read) — persistent readiness on a hung-up fd is expected and is why the PTY reader branches on POLLHUP rather than looping on readability. Diff both blocks against the production kernel (6.18) before drawing any conclusion from either.

### P9 — poll(2) timeout granularity — ✅ supported

**Question:** For a never-ready tty fd, what does a requested poll(2) timeout of 0/1/5/10 ms actually cost (min/median/max, µs)?

**Observed:**

- `poll_timeout_0ms`: max_us=1, median_ns=481, median_us=0, min_us=0, overshoot_median_us=0, ready_passes=0, requested_ms=0, requested_us=0, samples=16
- `poll_timeout_1ms`: max_us=1108, median_ns=1057863, median_us=1057, min_us=1053, overshoot_median_us=57, ready_passes=0, requested_ms=1, requested_us=1000, samples=16
- `poll_timeout_5ms`: max_us=5069, median_ns=5059303, median_us=5059, min_us=5055, overshoot_median_us=59, ready_passes=0, requested_ms=5, requested_us=5000, samples=16
- `poll_timeout_10ms`: max_us=10336, median_ns=10134890, median_us=10134, min_us=10061, overshoot_median_us=134, ready_passes=0, requested_ms=10, requested_us=10000, samples=16
- `median_us_for_1ms_request`: 1057
- `median_ns_for_0ms_request`: 481
- `ready_passes_total`: 0
- `zero_timeout_by_fd_state`: hangup_delivered_to_a_mask_that_requested_nothing=true, headline_offset_is=sample count and warmup only: median_ns_for_0ms_request is n=16 taken cold, unready_master_pollin_ns is n=4096 on the same fd, same mask, same wrapper, headline_over_matched_cell_x100=54, isolated_variable=ready-vs-not-ready, mask_role=measured: a poll requesting nothing still received POLLHUP on this kernel, so at a fixed fd state every mask cell observed one kernel state — the cells are replicates, not levels, and the table is a 1x2. The empty-mask cell measures that rather than citing POSIX, and it runs last in each group, so it doubles as a within-group warmup control., mask_spread_not_ready_requesting_x100=211, mask_spread_not_ready_x100=211, mask_spread_ready_requesting_x100=100, mask_spread_ready_x100=102, nonblocking_offset_x100=86, not_ready_cells=[unready_master_pollin_ns unready_master_pollhup_ns unready_master_empty_mask_ns], p2_instrument_blocking_fd_ns=414, p2_instrument_ready_passes=4096, p2_instrument_same_fd_ns=477, p2_instrument_verbatim_is=p2_instrument_blocking_fd_ns, p2_reports_the_shape=ready_hungup_master_pollhup_ns, read_the_mask_spread_from=mask_spread_not_ready_x100 / mask_spread_ready_x100, read_the_separation_from=worst_case_separation_x100, ready_cells=[ready_hungup_master_pollin_ns ready_hungup_master_pollhup_ns ready_hungup_master_empty_mask_ns], ready_hungup_master_empty_mask_ns=474, ready_hungup_master_empty_mask_ready_passes=4096, ready_hungup_master_empty_mask_revents=POLLHUP, ready_hungup_master_pollhup_ns=462, ready_hungup_master_pollhup_ready_passes=4096, ready_hungup_master_pollhup_revents=POLLHUP, ready_hungup_master_pollin_ns=463, ready_hungup_master_pollin_ready_passes=4096, ready_hungup_master_pollin_revents=POLLHUP, ready_passes_on_unready_fd=0, samples_each=4096, shape=1x2, the_data_plane_parks_on=unready_master_pollin_ns, unready_master_empty_mask_ns=418, unready_master_empty_mask_ready_passes=0, unready_master_empty_mask_revents=none, unready_master_pollhup_ns=418, unready_master_pollhup_ready_passes=0, unready_master_pollhup_revents=none, unready_master_pollin_ns=883, unready_master_pollin_ready_passes=0, unready_master_pollin_revents=none, worst_case_separation_requesting_masks_x100=90, worst_case_separation_x100=88, wrapper_offset_x100=103

**Consequence:** A zero timeout costs 481 ns median (the cost of asking) and a requested 1 ms costs 1057 µs median on this kernel — that is the floor §15.19's hybrid data plane was built around and the floor poll_ready's idle backoff steps against. 16 samples per timeout: enough to see the floor, not enough to characterize a tail. `zero_timeout_by_fd_state` is a **1x2** here: the isolated variable is ready-versus-not-ready, and whether the mask column is a control or a second axis is decided by `hangup_delivered_to_a_mask_that_requested_nothing`, which this kernel answers **true** — see `mask_role`. Read `worst_case_separation_x100` against `mask_spread_not_ready_x100 / mask_spread_ready_x100`: the finding survives only where the first exceeds the second, and the two must come from the SAME cell set — `read_the_separation_from` and `read_the_mask_spread_from` name the matching pair, because comparing a figure that drops the empty-mask cell against one that keeps it is not a comparison. `median_ns_for_0ms_request` above is n=16 taken cold and is NOT comparable to P2's headline — `p2_instrument_blocking_fd_ns` is, and `wrapper_offset_x100` / `nonblocking_offset_x100` say how much of any residual is this probe's instrument rather than the kernel's. Diff these against the production kernel (6.18) before tuning any backoff step or timer against them.

### P10 — pty buffer depth — ✅ supported

**Question:** How many bytes does a pty accept in each direction before it would block, with nothing draining the other end?

**Observed:**

- `slave_to_master_targetward`: bytes_accepted_before_eagain=11776, bytes_recovered_by_peer=15360, bytes_unrecoverable=0, ceiling_bytes=4194304, ceiling_hit=false, chunk_bytes=4096, peer_pending_input_bytes=4095, peer_pending_input_bytes_at_drain=4095, peer_pending_input_trust=undercounts, pending_output_bytes=0, recheck=[drained_again_bytes=512 flat_fields_are_the_rung_draining=512 ladder=[[carries_a_watermark_bound=true drain_came_up_short=false drain_requested_bytes=512 drained_bytes=512 occupancy_after_drain_bytes=9216 refill_terminal_write=EAGAIN refilled_from_empty_bytes=9728 refilled_from_empty_writes=3 topped_up_bytes=9728 topped_up_minus_drained=9216 topped_up_writes=9728 topup_ceiling_hit=false topup_terminal_write=EAGAIN] [carries_a_watermark_bound=true drain_came_up_short=false drain_requested_bytes=1 drained_bytes=1 occupancy_after_drain_bytes=13823 refill_terminal_write=EAGAIN refilled_from_empty_bytes=13824 refilled_from_empty_writes=4 topped_up_bytes=9728 topped_up_minus_drained=9727 topped_up_writes=9728 topup_ceiling_hit=false topup_terminal_write=EAGAIN] [carries_a_watermark_bound=true drain_came_up_short=false drain_requested_bytes=128 drained_bytes=128 occupancy_after_drain_bytes=13696 refill_terminal_write=EAGAIN refilled_from_empty_bytes=13824 refilled_from_empty_writes=4 topped_up_bytes=9728 topped_up_minus_drained=9600 topped_up_writes=9728 topup_ceiling_hit=false topup_terminal_write=EAGAIN] [carries_a_watermark_bound=true drain_came_up_short=false drain_requested_bytes=900 drained_bytes=900 occupancy_after_drain_bytes=12924 refill_terminal_write=EAGAIN refilled_from_empty_bytes=13824 refilled_from_empty_writes=4 topped_up_bytes=9728 topped_up_minus_drained=8828 topped_up_writes=9728 topup_ceiling_hit=false topup_terminal_write=EAGAIN] [carries_a_watermark_bound=false drain_came_up_short=false drain_requested_bytes=null drained_bytes=13824 occupancy_after_drain_bytes=0 refill_terminal_write=EAGAIN refilled_from_empty_bytes=13824 refilled_from_empty_writes=4 topped_up_bytes=20480 topped_up_minus_drained=6656 topped_up_writes=20480 topup_ceiling_hit=false topup_terminal_write=EAGAIN]] ladder_reading=[reading=T is the watermark in `writable iff occupancy < T, then accept up to capacity`. A null `watermark_threshold_le` means no rung refused, so no hysteresis was seen at any drain size probed — bounded by the smallest rung, not proof of a pure capacity. A null `watermark_threshold_gt` means no rung topped up at all. Both are null when no rung freed any bytes, which is what a cooked pty does; nothing is inferred from a rung that freed nothing. Where `rungs_refusing` is 0 the `_gt` bound is the largest occupancy the ladder happened to reach and NOT an occupancy the kernel was observed to accept a write at — on a pipeline kernel it moved under the top-up. rungs_carrying_a_bound=4 rungs_refusing=0 rungs_topping_up=4 uniform_shortfall_bytes=null watermark_threshold_gt=13823 watermark_threshold_le=null] refill_reproduced_total=false refill_terminal_write=EAGAIN refilled_from_empty_bytes=9728 refilled_from_empty_writes=3 room_republished_minus_room_freed=9216 topped_up_bytes=9728 topped_up_writes=9728 topup_ceiling_bytes=65536 topup_ceiling_hit=false topup_chunk_bytes=1 topup_terminal_write=EAGAIN], settle_ms=20, settled_extra_bytes=3584, settled_extra_writes=1, slave_termios_mode=raw, terminal_write=EAGAIN, terminal_write_after_settle=EAGAIN, total_bytes_accepted=15360, writer_pending_input_bytes=0, writes=3
- `master_to_slave_hostward`: bytes_accepted_before_eagain=13824, bytes_recovered_by_peer=13824, bytes_unrecoverable=0, ceiling_bytes=4194304, ceiling_hit=false, chunk_bytes=4096, peer_pending_input_bytes=4095, peer_pending_input_bytes_at_drain=4095, peer_pending_input_trust=undercounts, pending_output_bytes=0, recheck=[drained_again_bytes=512 flat_fields_are_the_rung_draining=512 ladder=[[carries_a_watermark_bound=true drain_came_up_short=false drain_requested_bytes=512 drained_bytes=512 occupancy_after_drain_bytes=13312 refill_terminal_write=EAGAIN refilled_from_empty_bytes=13824 refilled_from_empty_writes=4 topped_up_bytes=2560 topped_up_minus_drained=2048 topped_up_writes=2560 topup_ceiling_hit=false topup_terminal_write=EAGAIN] [carries_a_watermark_bound=true drain_came_up_short=false drain_requested_bytes=1 drained_bytes=1 occupancy_after_drain_bytes=13823 refill_terminal_write=EAGAIN refilled_from_empty_bytes=13824 refilled_from_empty_writes=4 topped_up_bytes=2560 topped_up_minus_drained=2559 topped_up_writes=2560 topup_ceiling_hit=false topup_terminal_write=EAGAIN] [carries_a_watermark_bound=true drain_came_up_short=false drain_requested_bytes=128 drained_bytes=128 occupancy_after_drain_bytes=13696 refill_terminal_write=EAGAIN refilled_from_empty_bytes=13824 refilled_from_empty_writes=4 topped_up_bytes=2560 topped_up_minus_drained=2432 topped_up_writes=2560 topup_ceiling_hit=false topup_terminal_write=EAGAIN] [carries_a_watermark_bound=true drain_came_up_short=false drain_requested_bytes=900 drained_bytes=900 occupancy_after_drain_bytes=12924 refill_terminal_write=EAGAIN refilled_from_empty_bytes=13824 refilled_from_empty_writes=4 topped_up_bytes=2560 topped_up_minus_drained=1660 topped_up_writes=2560 topup_ceiling_hit=false topup_terminal_write=EAGAIN] [carries_a_watermark_bound=false drain_came_up_short=false drain_requested_bytes=null drained_bytes=13824 occupancy_after_drain_bytes=0 refill_terminal_write=EAGAIN refilled_from_empty_bytes=13824 refilled_from_empty_writes=4 topped_up_bytes=20480 topped_up_minus_drained=6656 topped_up_writes=20480 topup_ceiling_hit=false topup_terminal_write=EAGAIN]] ladder_reading=[reading=T is the watermark in `writable iff occupancy < T, then accept up to capacity`. A null `watermark_threshold_le` means no rung refused, so no hysteresis was seen at any drain size probed — bounded by the smallest rung, not proof of a pure capacity. A null `watermark_threshold_gt` means no rung topped up at all. Both are null when no rung freed any bytes, which is what a cooked pty does; nothing is inferred from a rung that freed nothing. Where `rungs_refusing` is 0 the `_gt` bound is the largest occupancy the ladder happened to reach and NOT an occupancy the kernel was observed to accept a write at — on a pipeline kernel it moved under the top-up. rungs_carrying_a_bound=4 rungs_refusing=0 rungs_topping_up=4 uniform_shortfall_bytes=null watermark_threshold_gt=13823 watermark_threshold_le=null] refill_reproduced_total=true refill_terminal_write=EAGAIN refilled_from_empty_bytes=13824 refilled_from_empty_writes=4 room_republished_minus_room_freed=2048 topped_up_bytes=2560 topped_up_writes=2560 topup_ceiling_bytes=65536 topup_ceiling_hit=false topup_chunk_bytes=1 topup_terminal_write=EAGAIN], settle_ms=20, settled_extra_bytes=0, settled_extra_writes=0, slave_termios_mode=raw, terminal_write=EAGAIN, terminal_write_after_settle=EAGAIN, total_bytes_accepted=13824, writer_pending_input_bytes=0, writes=4

**Consequence:** This kernel's pty accepted 11776 byte(s) slave→master (**targetward** — a client typing, travelling toward the device, first pass ending in `EAGAIN`) and 13824 byte(s) master→slave (**hostward** — the node delivering device output to its client, ending in `EAGAIN`), reaching 15360 and 13824 in total once a short pause has let the tty's asynchronous flip work run. **Of those, 15360 and 13824 byte(s) were actually recoverable by the peer** (all of it / all of it): acceptance is not delivery, and the two are the same number only on a kernel that queues everything it takes. Read the daemon's `hostward_buffer` defaults against the SCALE of these, not their last byte: the pty default is 32 chunks, and a queue far larger than the kernel pipe below it only defers the same backpressure. Both figures move by a chunk or two run to run depending on when that flip work lands, so a one-chunk difference across kernels is noise; only an order-of-magnitude one is signal, **and only between runs whose `slave_termios_mode` agrees** — a cooked pty and a raw one give different depths on one kernel, and in opposite directions — raw accepts less and returns all of it, cooked accepts more and returns none (measured on Linux 7.0.0-29) — so a mode mismatch explains a gap before any kernel difference does, and the `slave_termios_mode` cell beside each direction is what settles it. The `recheck` block under each direction asks the second question the first cannot, at four drain sizes rather than one: after the peer is drained the pair is refilled from empty and handed back 512, 1, 128 and 900 bytes in turn, and then once from empty entirely. `ladder_reading.watermark_threshold_gt` and `_le` bracket the watermark in "writable iff occupancy < T, then accept up to capacity" — a rung that tops up floors T at its `occupancy_after_drain`, a rung that refuses caps it there. A null `_le` means no rung refused, which bounds T below capacity only down to the smallest rung probed and is **not** proof of a pure capacity; read `_gt` on such a run as the largest occupancy the ladder reached rather than as one the kernel accepted a write at, because on a pipeline kernel it moved under the top-up. `uniform_shortfall_bytes` names a reservation charged per fill episode; the from-empty rung (`drain_requested_bytes: null`) is the one whose top-up starts at occupancy 0, so a reservation charged at the empty→nonempty transition lands inside its number instead of behind it, and comparing its `topped_up_bytes` against the 4 KiB-chunked `refilled_from_empty_bytes` on the same rung says whether write size changes the accounting. The flat fields beside the ladder are the 512-byte rung alone, kept so older reports still diff, and `room_republished_minus_room_freed` there says whether the kernel gave back exactly the room a reader freed (a fixed queue capacity), or more (an asynchronous pipeline that advanced during the settle — Linux 7.0.0-29 reads +2048 or +9216, bimodal, never 0 across 20 samples), or less. `refill_reproduced_total` says whether the depth above is reproducible on the same pair at all; on Linux it usually is not. Numbers, not a verdict — diff them against the production kernel (6.18) before changing a default.

### P11 — real-port line-state counters — ✅ supported

**Question:** Do TIOCGICOUNT (driver error/edge counters) and TIOCMGET (modem lines) answer on a real port, and what do they currently read (§5, §7.1)?

**Observed:**

- `/dev/ttyUSB0`: counters=[brk=0 buf_overrun=0 cts=12 dcd=0 dsr=0 frame=0 overrun=0 parity=0 rng=0 rx=164 tx=932], modem_bits_hex=0x0006, modem_lines_asserted=[DTR RTS], tiocgicount_available=true, tiocmget_available=true
- `/dev/ttyUSB1`: counters=[brk=0 buf_overrun=0 cts=6 dcd=0 dsr=0 frame=5 overrun=0 parity=0 rng=0 rx=197 tx=164], modem_bits_hex=0x0006, modem_lines_asserted=[DTR RTS], tiocgicount_available=true, tiocmget_available=true

**Consequence:** Both line-state ioctls answer on 2 of 2 named port(s): the driver counters (§5, §7.1) and the modem lines are readable, so serial state carries real error/overrun accounting rather than omitting it. Read the counts as a snapshot of a cumulative total, not as a measurement of this run — they count since the driver bound the device, and P3/P5 transmit on these same ports earlier in the same invocation (a nonzero `frame` here is usually P5's deliberate baud-mismatch item, not a fault). Across kernels, diff the ioctl *availability* and the field set; the absolute counts differ by construction.

### P15 — real-port flow-control honouring — ✅ supported

**Question:** Does a named port honour a requested hardware flow-control mode (CRTSCTS) on read-back, or accept the request and silently drop it (§7.1, §15.51)?

**Observed:**

- `/dev/ttyUSB0`: baseline_restored=true, cflag_after_hex=0x90021cb2, cflag_before_hex=0x10021cb2, honoured_on_readback=true, requested=rts-cts (CRTSCTS), shipped_predicate_agrees=true, silently_dropped=false, tcsetattr_error=null, tcsetattr_ok=true
- `/dev/ttyUSB1`: baseline_restored=true, cflag_after_hex=0x90021cb2, cflag_before_hex=0x10021cb2, honoured_on_readback=true, requested=rts-cts (CRTSCTS), shipped_predicate_agrees=true, silently_dropped=false, tcsetattr_error=null, tcsetattr_ok=true

**Consequence:** Every named port (2) honoured `CRTSCTS` on read-back, so a `flow = "rts-cts"` edge configures here and the driver agrees it did. `serial2` verifies settings by reading them back, so this is exactly the check the serial node's open performs.

### P14 — maximum reliable rate — ✅ supported

**Question:** On a P5-verified cross-paired rig, what is the highest baud rate at which a seeded payload still round-trips byte-exact in both directions, and what stopped the search (§15.51)?

**Observed:**

- `max_reliable_baud`: null
- `ceiling_kind`: null
- `ceiling_is_a_floor_over`: nothing yet — the search did not run on this path; the verdict says why.
- `pairs_discovered`: 1
- `structural_max_baud`: 4294967295
- `baseline_baud`: 115200
- `pair`: usb:0403:6001:BH00LL8O:00 ↔ usb:0403:6001:BH00L4KU:00
- `baseline_integrity_from_p5`: true
- `icounts_measurable`: true
- `max_reliable_baud`: 3000000
- `ceiling_kind`: adapter-refused
- `first_unreliable_baud`: 3062500
- `ceiling_is_a_floor_over`: the rates this ladder probed, under this trial policy — three byte-exact constant-airtime round-trips per direction. It is not a promise about rates between rungs, about sustained throughput, about longer cables, or about other temperatures. **It is a REQUESTED rate, not necessarily the wire's**: an adapter may round the ask to its nearest divisor and report the request back unchanged, and the bytes still round-trip because both ends are mis-set identically. Read `achieved_baud_floor` under each direction beside it — a large gap there (the bench rig shows ~0.94 of the ask on a clean rung and 0.70 on a rounded one) means the number above is the number you configure, not the number on the wire.
- `search_stops_at`: the FIRST rung that fails, not the exhaustion of the ladder — so a rate above the reported ceiling was never tried unless refinement reached it, and this number is a floor for that reason too.
- `ladder_body_rungs`: 16
- `rungs_attempted`: 17
- `refinements_used`: 4
- `refinements_max`: 4
- `trials_per_direction`: 3
- `airtime_ms`: 250
- `payload_floor_bytes`: 64
- `payload_cap_bytes`: 65536
- `search_elapsed_ms`: 21967
- `search_budget_exhausted`: false
- `baseline_restored`: true
- `baseline_reproved`: true
- `rungs`: [[ab=[achieved_baud_floor=9537 byte_exact=true bytes_received=720 bytes_sent=720 elapsed_us=823636 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] actual_baud_a=9600 actual_baud_b=9600 ba=[achieved_baud_floor=9536 byte_exact=true bytes_received=720 bytes_sent=720 elapsed_us=815819 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] frame_delta=0 outcome=passed overrun_delta=0 parity_delta=0 phase=body refusal_errno=null requested_baud=9600] [ab=[achieved_baud_floor=19077 byte_exact=true bytes_received=1440 bytes_sent=1440 elapsed_us=823214 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] actual_baud_a=19200 actual_baud_b=19200 ba=[achieved_baud_floor=19082 byte_exact=true bytes_received=1440 bytes_sent=1440 elapsed_us=815718 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] frame_delta=0 outcome=passed overrun_delta=0 parity_delta=0 phase=body refusal_errno=null requested_baud=19200] [ab=[achieved_baud_floor=37365 byte_exact=true bytes_received=2880 bytes_sent=2880 elapsed_us=844463 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] actual_baud_a=38400 actual_baud_b=38400 ba=[achieved_baud_floor=38303 byte_exact=true bytes_received=2880 bytes_sent=2880 elapsed_us=832071 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] frame_delta=0 outcome=passed overrun_delta=0 parity_delta=0 phase=body refusal_errno=null requested_baud=38400] [ab=[achieved_baud_floor=56023 byte_exact=true bytes_received=4320 bytes_sent=4320 elapsed_us=844315 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] actual_baud_a=57600 actual_baud_b=57600 ba=[achieved_baud_floor=55195 byte_exact=true bytes_received=4320 bytes_sent=4320 elapsed_us=848360 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] frame_delta=0 outcome=passed overrun_delta=0 parity_delta=0 phase=body refusal_errno=null requested_baud=57600] [ab=[achieved_baud_floor=109994 byte_exact=true bytes_received=8640 bytes_sent=8640 elapsed_us=847162 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] actual_baud_a=115200 actual_baud_b=115200 ba=[achieved_baud_floor=109702 byte_exact=true bytes_received=8640 bytes_sent=8640 elapsed_us=848485 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] frame_delta=0 outcome=passed overrun_delta=0 parity_delta=0 phase=body refusal_errno=null requested_baud=115200] [ab=[achieved_baud_floor=219046 byte_exact=true bytes_received=17280 bytes_sent=17280 elapsed_us=850596 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] actual_baud_a=230400 actual_baud_b=230400 ba=[achieved_baud_floor=218655 byte_exact=true bytes_received=17280 bytes_sent=17280 elapsed_us=852546 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] frame_delta=0 outcome=passed overrun_delta=0 parity_delta=0 phase=body refusal_errno=null requested_baud=230400] [ab=[achieved_baud_floor=436016 byte_exact=true bytes_received=34560 bytes_sent=34560 elapsed_us=854363 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] actual_baud_a=460800 actual_baud_b=460800 ba=[achieved_baud_floor=436371 byte_exact=true bytes_received=34560 bytes_sent=34560 elapsed_us=853695 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] frame_delta=0 outcome=passed overrun_delta=0 parity_delta=0 phase=body refusal_errno=null requested_baud=460800] [ab=[achieved_baud_floor=870962 byte_exact=true bytes_received=69120 bytes_sent=69120 elapsed_us=855914 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] actual_baud_a=921600 actual_baud_b=921600 ba=[achieved_baud_floor=869985 byte_exact=true bytes_received=69120 bytes_sent=69120 elapsed_us=856730 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] frame_delta=0 outcome=passed overrun_delta=0 parity_delta=0 phase=body refusal_errno=null requested_baud=921600] [ab=[achieved_baud_floor=942489 byte_exact=true bytes_received=75000 bytes_sent=75000 elapsed_us=858156 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] actual_baud_a=1000000 actual_baud_b=1000000 ba=[achieved_baud_floor=942119 byte_exact=true bytes_received=75000 bytes_sent=75000 elapsed_us=858275 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] frame_delta=0 outcome=passed overrun_delta=0 parity_delta=0 phase=body refusal_errno=null requested_baud=1000000] [ab=[achieved_baud_floor=1414000 byte_exact=true bytes_received=112500 bytes_sent=112500 elapsed_us=858210 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] actual_baud_a=1500000 actual_baud_b=1500000 ba=[achieved_baud_floor=1414523 byte_exact=true bytes_received=112500 bytes_sent=112500 elapsed_us=858936 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] frame_delta=0 outcome=passed overrun_delta=0 parity_delta=0 phase=body refusal_errno=null requested_baud=1500000] [ab=[achieved_baud_floor=1880674 byte_exact=true bytes_received=150000 bytes_sent=150000 elapsed_us=862145 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] actual_baud_a=2000000 actual_baud_b=2000000 ba=[achieved_baud_floor=1882728 byte_exact=true bytes_received=150000 bytes_sent=150000 elapsed_us=859182 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] frame_delta=0 outcome=passed overrun_delta=0 parity_delta=0 phase=body refusal_errno=null requested_baud=2000000] [ab=[achieved_baud_floor=2802180 byte_exact=true bytes_received=196608 bytes_sent=196608 elapsed_us=764894 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] actual_baud_a=3000000 actual_baud_b=3000000 ba=[achieved_baud_floor=2801174 byte_exact=true bytes_received=196608 bytes_sent=196608 elapsed_us=765252 failure=null hung_up=false measured=true trials_passed=3 trials_run=3] frame_delta=0 outcome=passed overrun_delta=0 parity_delta=0 phase=body refusal_errno=null requested_baud=3000000] [ab=[achieved_baud_floor=null byte_exact=false bytes_received=0 bytes_sent=0 elapsed_us=0 failure=null hung_up=false measured=false trials_passed=0 trials_run=0] actual_baud_a=9600 actual_baud_b=9600 ba=[achieved_baud_floor=null byte_exact=false bytes_received=0 bytes_sent=0 elapsed_us=0 failure=null hung_up=false measured=false trials_passed=0 trials_run=0] frame_delta=null outcome=adapter-refused overrun_delta=null parity_delta=null phase=body refusal_errno=null requested_baud=4000000] [ab=[achieved_baud_floor=null byte_exact=false bytes_received=0 bytes_sent=0 elapsed_us=0 failure=null hung_up=false measured=false trials_passed=0 trials_run=0] actual_baud_a=9600 actual_baud_b=9600 ba=[achieved_baud_floor=null byte_exact=false bytes_received=0 bytes_sent=0 elapsed_us=0 failure=null hung_up=false measured=false trials_passed=0 trials_run=0] frame_delta=null outcome=adapter-refused overrun_delta=null parity_delta=null phase=refinement refusal_errno=null requested_baud=3500000] [ab=[achieved_baud_floor=null byte_exact=false bytes_received=0 bytes_sent=0 elapsed_us=0 failure=null hung_up=false measured=false trials_passed=0 trials_run=0] actual_baud_a=9600 actual_baud_b=9600 ba=[achieved_baud_floor=null byte_exact=false bytes_received=0 bytes_sent=0 elapsed_us=0 failure=null hung_up=false measured=false trials_passed=0 trials_run=0] frame_delta=null outcome=adapter-refused overrun_delta=null parity_delta=null phase=refinement refusal_errno=null requested_baud=3250000] [ab=[achieved_baud_floor=null byte_exact=false bytes_received=0 bytes_sent=0 elapsed_us=0 failure=null hung_up=false measured=false trials_passed=0 trials_run=0] actual_baud_a=9600 actual_baud_b=9600 ba=[achieved_baud_floor=null byte_exact=false bytes_received=0 bytes_sent=0 elapsed_us=0 failure=null hung_up=false measured=false trials_passed=0 trials_run=0] frame_delta=null outcome=adapter-refused overrun_delta=null parity_delta=null phase=refinement refusal_errno=null requested_baud=3125000] [ab=[achieved_baud_floor=null byte_exact=false bytes_received=0 bytes_sent=0 elapsed_us=0 failure=null hung_up=false measured=false trials_passed=0 trials_run=0] actual_baud_a=9600 actual_baud_b=9600 ba=[achieved_baud_floor=null byte_exact=false bytes_received=0 bytes_sent=0 elapsed_us=0 failure=null hung_up=false measured=false trials_passed=0 trials_run=0] frame_delta=null outcome=adapter-refused overrun_delta=null parity_delta=null phase=refinement refusal_errno=null requested_baud=3062500]]

**Consequence:** Maximum reliable rate 3000000 baud on this pair; the search stopped because the next rate was accepted by the ask and the driver landed somewhere else, which the requested-versus-actual cells above name; the adapter's divisor model is the limit (`ceiling_kind=adapter-refused`). Configure a serial node above that rate and you are past what was measured here. Read the number as a **floor over the probed set** under the stated trial policy — 3 byte-exact constant-airtime round-trips per direction — never as a promise about rates the ladder skipped, sustained throughput, longer cables, or other temperatures.

## Summary

24 supported · 0 degraded · 0 unsupported · 1 skipped
