# serial_nexus — Comprehensive Code Review

**Reviewer:** Claude Opus 4.8 (multi-agent, adversarially verified)
**Date:** 2026-07-23
**Scope:** The full `serial_nexus` workspace (~17.4k LOC Rust) as of `acf0528` — daemon, CLI, core, codec-api, reference codec, doctor, sim, and the validation harness — against the normative design `docs/17-design-claude-fable-v8.md`, the plan `docs/18-implementation-plan-claude-fable-v8.md`, and the deviations already recorded in `docs/implementation-notes.md`.
**Focus (as requested):** correctness, reliability, design deviations, and opportunities to improve testing, documentation, and clarity.

---

## Executive summary

serial_nexus is an unusually disciplined codebase. The load-bearing pure logic — the write-arbitration state machine (`lock.rs`), the data-plane deliver contracts and holdover (`data.rs`), the graph validator (`graph.rs`) — is correct, well-factored, and property-tested; the eight prior adversarial audits show in the code. The review found **no new defect in the pure `nexus-core` state machines**. Every surviving finding lives where the design predicts residual risk: the async daemon glue, the boundary node implementations, and the invariants that are still upheld "by convention per node type" rather than structurally — exactly the pattern design §16.1 flags.

This review ran **27 specialized reviewers** (one per subsystem, plus six cross-cutting invariant sweeps — no-drop, purge, await-across-borrow, panics, resource-cleanup, hostile-input parsers — and three opportunity sweeps). **Every** finding was then handed to an independent adversarial verifier that re-read the code, tried to refute it, and checked it against the already-documented deviations. Of 80 candidate findings, **63 survived verification** (60 CONFIRMED, 3 PLAUSIBLE), **16 were refuted**, and **1 was already documented**. After merging duplicates, **56 distinct findings** remain, summarized below.

The headline results:

- **Two criticals**, both concrete and reachable in normal operation:
  1. **`codec.rs` silently drops an oversize targetward command, uncounted** — the *exact* §5/§9 no-drop violation the phase-6 audit found and fixed in `leg.rs` and `exec.rs`, but never retrofitted into the in-process codec node. `send mux/console <~64 KB+>` returns `"delivered": true` while the bytes vanish. This is the most important finding in the review.
  2. **A wedged PTY client hangs teardown/shutdown and freezes the entire single-threaded daemon** — `blocking_write_all` never consults the `stop` flag and `teardown`/`Drop` join it unboundedly on the sole runtime thread. One non-cooperative client can make `remove-node`, `load --replace`, and `shutdown` hang forever.
- **One high**: an empty (not merely missing) sysfs serial string is captured as a concrete `usb:…` identity, defeating the by-path degradation and reopening the wrong-device-adoption hole §15.10 promises is closed by construction.
- **Twelve mediums**, mostly reliability: unbounded per-connection and per-child memory (control-socket line length, exec stderr), a stranded `lock --wait` waiter on cascade removal, a silent failed log rotation, a scheduling-race in held-lock reclaim, and the observed-state invariant break in `snapshot()` (two same-labelled `send` origins both report as holder).
- A rich seam of **testing gaps** (the anti-stranding flush, control-event round-trips, hostile-decode variants, the held-reclaim property model) and **doc drift** (README points at a deleted v6 design; version banner stale at 0.1.0).

None of the findings contradict the architecture; every one is a localized fix. The recurring theme is precisely design §16's thesis: three or four invariants (no-drop, cooperative teardown, purge exactness, held-priority) are re-derived by hand per node, and the misses cluster where a node skipped one. Making those structural (the §16.1 supervisor library already started this) would retire most of this list at the source.

### Prioritized action list

| # | Severity | Finding | Location |
|---|----------|---------|----------|
| 1 | 🔴 critical | Fragment (never drop) oversize targetward chunks in the in-process codec, mirroring `leg.rs`/`exec.rs`; count any residual | `codec.rs:324` (XC-NODROP-1) |
| 2 | 🔴 critical | Make the PTY blocking writer observe `stop`, and bound the teardown/Drop join | `pty.rs:663` (PTY-1) |
| 3 | 🟠 high | Normalize an empty/whitespace sysfs serial to the absent marker so it degrades to by-path | `resolver.rs:436` (RESOLV-1) |
| 4 | 🟡 medium | Bound the control-socket request line length | `control.rs:29` (CTRL-1) |
| 5 | 🟡 medium | Bound exec-child stderr buffering | `exec.rs:437` (CODEXEC-1) |
| 6 | 🟡 medium | Wake the surviving lock on cascade-removal of a *queued* waiter, not only the holder | `daemon.rs:553` (DLC-1) |
| 7 | 🟡 medium | Compute `holds_lock` by `OriginId`, not label string | `lock.rs:358` (LOCK-1) |
| 8 | 🟡 medium | Fault or surface a failed log rotation instead of silently appending to the unrotated file | `log.rs:329` (LOG-2) |
| 9 | 🟡 medium | Make held-lock reclaim win deterministically over a queued on-demand waiter | `daemon.rs:795` (daemon-arbitration-1) |
| 10 | 🟡 medium | Reject bare all-slash resolver input as malformed | `resolver.rs:222` (RESOLV-2) |
| 11 | 🟡 medium | Purge-on-reconnect must also drain the in-flight backpressured chunk; keep the counter exact | `serial.rs:395` (XC-PURGE-1) |

Full detail for every finding follows. Design deviations are classified in §4 into **should-fix** (reported here) and **justified** (added to `docs/implementation-notes.md §3`).

### Methodology & confidence

Findings were produced by static reasoning over the code and design, not by running the build (the CI gates are green and were not re-litigated). Each was adversarially verified against the actual source with an explicit triggering scenario; the verifier's verdict (`CONFIRMED` = reachable and correctly characterized; `PLAUSIBLE` = likely real, reachability not fully constructed) is shown per finding. The 16 refuted candidates are listed in §6 so the team need not re-investigate them — several were accurate code observations whose harm turned out to be unreachable (e.g. the `leg.rs` fragmentation `break`, which is safe precisely *because* the leg fragments first — the same reasoning that makes the non-fragmenting `codec.rs` `continue` a real bug).

---
## 1. Bugs — correctness & reliability

Ordered by severity. Each finding was raised by a subsystem or cross-cutting reviewer and independently confirmed by an adversarial verifier that re-read the code and attempted to refute it; the verifier's verdict is shown. `should-fix` design deviations that are effectively bugs are included here and cross-referenced in §4.

### Critical

#### 🔴 CRITICAL — PTY hostward-writer teardown join is unbounded and ignores the stop flag — a stalled-but-present client deadlocks the whole daemon on remove-node/shutdown

`nexus-daemon/src/nodes/pty.rs:663`  ·  _PTY-1_ · reliability  ·  verifier: **CONFIRMED**

The PTY hostward writer runs on a dedicated blocking thread. `writer_thread` (line 631) only re-reads the `stop` flag at its `recv_timeout` boundary; once it enters `blocking_write_all` (lines 663-678) the `Err(WouldBlock)` branch loops on `poll_blocking(fd, POLLOUT|POLLHUP, 500)` and returns only on POLLHUP-without-POLLOUT, a non-WouldBlock error, or a completed write — it never checks `stop`. The master fd is non-blocking (start() line 251), so when a PTY client holds the slave open but stops draining (paused terminal / Ctrl-S XOFF / a crashed reader whose child keeps the fd open) and the serial floods hostward data, the pts kernel buffer fills, `write_fd` returns EAGAIN forever, and there is no POLLHUP (slave still open) and no EIO. The writer thread spins in `blocking_write_all` indefinitely. `teardown` (lines 343-349) sets `writer_stop`, aborts the async tasks, then does an UNBOUNDED `w.join()`. Because teardown runs synchronously on the single current-thread runtime, the aborted async pump future is never dropped during the join, so its `btx` never disconnects, and the writer — wedged mid-chunk inside `blocking_write_all` — never returns. `w.join()` blocks the runtime thread forever, freezing the entire daemon (all connections, and `shutdown` via `teardown_all`). `Drop` (line 364) has the identical unbounded join. The log node bounds exactly this hazard with FLUSH_WAIT + detach (log.rs:220-232); the PTY writer does not.

**Recommendation.** Make `blocking_write_all` take the `stop: &AtomicBool` and return early (e.g. Err(Interrupted)) when it is set inside the WouldBlock poll loop, so the writer observes teardown within one poll interval; AND/OR bound the join in `teardown`/`Drop` like the log node (a done-signal + `recv_timeout(FLUSH_WAIT)` then detach) so a wedged writer can never block the runtime thread. Do both for defense in depth.

#### 🔴 CRITICAL — In-process codec node silently drops an oversize targetward chunk (uncounted) — the §15.24 continue-on-encode-error violation that was fixed in leg.rs/exec.rs but never retrofitted into codec.rs

`nexus-daemon/src/nodes/codec.rs:325`  ·  _XC-NODROP-1_ · correctness  ·  verifier: **CONFIRMED** _(also independently reported as CODEXEC-5)_

`channel_targetward` frames a whole targetward chunk with a single `codec.mux(&Event::data(channel, bytes), &mut framed)` and, on failure, executes `continue` (line 325) — dropping the entire chunk with no counter and no backpressure. The reference codec's `mux` is exactly `encode(event, out)` (codecs/reference/src/lib.rs:109-112), which returns `EnvelopeError::FrameTooLarge` whenever `1 + 2 + channel.len() + payload.len() > MAX_FRAME_SIZE` (codec-api/src/lib.rs:205-208; MAX_FRAME_SIZE = 64*1024). Unlike the leg (leg.rs:596-618) and the exec codec (exec.rs:390-405), which fragment an oversize chunk across consecutive data frames, the in-process codec passes the WHOLE unbounded chunk to `mux` and never fragments. The input to `channel_targetward` is an uncapped targetward write to a codec CHANNEL endpoint (host-facing), so it is not bounded to leave header room: (a) the `send` verb appends '\n' to an arbitrary-length RPC line and delivers it as one Chunk with no cap (daemon.rs:866-869) — its own §15.24 regression guard uses a 100_001-byte line; (b) a PTY attached to the codec channel forwards a packet-mode payload up to READ_BUF-1 = 65535 bytes as a single targetward chunk (pty.rs:518-520), and READ_BUF == MAX_FRAME_SIZE (runtime.rs:188). With any non-empty channel id (e.g. 'console', len 7), body_len = 3 + 7 + 65534 = 65544 > 65536 → encode fails → the command is silently discarded. This is exactly the CRITICAL targetward-no-drop violation the phase-6 audit re-examined and fixed 'in leg.rs (and the same idiom in exec.rs)'; the original phase-5 rejection reasoning ('bounded by READ_BUF == MAX_FRAME_SIZE') is wrong for this path because the channel-id + envelope header overhead pushes a READ_BUF-sized payload over the frame bound. The codec ChannelStat has delivered_hostward / discarded_unattached / accepted_targetward but NO targetward-drop counter, so the loss is neither backpressured, delivered-delayed, nor even counted — it just vanishes. Aggravating: the `send` verb returns `"delivered": true` (daemon.rs:869,877-878) because the handoff into the bounded channel succeeded, so the operator is told a ~64 KB command was delivered while the codec task silently drops it downstream.

**Recommendation.** Fragment the outbound chunk in `channel_targetward` exactly as leg.rs and exec.rs do: compute `cap = MAX_FRAME_SIZE.saturating_sub(3 + channel.len()).max(1)`, loop over `bytes.slice(off..min(off+cap, total))`, `mux`/`encode` each piece into its own data frame, gate on the lock, and `mux_tx.send(...).await` each — never dropping. Keep the `continue`/`break` on a mux error only as a truly-unreachable defensive fallback (each piece then provably fits), and if any residual drop path remains, count it against a new targetward-discarded counter so §5's all-loss-is-counted rule holds. Add a regression test mirroring the leg's 100_001-byte round-trip but through an in-process demux codec channel (byte-exact, drop count 0).

### High

#### 🟠 HIGH — Empty (whitespace-only) sysfs serial string is not treated as absent, defeating the by-path degradation and reopening the wrong-device-adoption hole

`nexus-core/src/resolver.rs:436`  ·  _RESOLV-1_ · correctness  ·  verifier: **CONFIRMED**

sysfs_lookup only marks a serial *absent* when the sysfs `serial` file is MISSING: `let serial = read_trimmed(&cur.join("serial")).unwrap_or_else(|| "-".into());` (line 436). `read_trimmed` returns `Some("")` when the file exists but is empty or whitespace-only (`read_to_string(...).map(|s| s.trim().to_owned())`, line 460) — a real case for cheap USB adapters that expose an empty iSerialNumber string descriptor. capture_for_dev's absent test is `let absent = usb_serial_field(&info.identity) == Some("-")` (line 265), and `usb_serial_field` on `usb:0403:6001::00` returns `Some("")`, which is NOT `Some("-")`. So a single present device with an empty serial is captured as the concrete identity `usb:<vid>:<pid>::<iface>` instead of degrading to by-path, and `usb_identity_ambiguous` (line 296) does not fire because only one device is present at capture time. Later, when a second identical adapter (same vid/pid, same empty serial, same iface) is plugged into a different port, the stored identity `usb:<vid>:<pid>::<iface>` matches BOTH devices; `resolve_current_path`→`find_usb` (line 367) returns the FIRST match in readdir order (line 383) with no uniqueness guard, so a reopen/faulted-and-wait recheck can adopt the wrong physical device — precisely the wrong-device adoption §15.10 states is impossible by construction. Note the sibling absent case (missing serial → `-`) and the duplicated-non-empty-serial case (audit fix #1) are both handled; the empty-string serial slips between them.

**Recommendation.** Normalize an empty/whitespace serial to the absent marker at the source, e.g. in sysfs_lookup: `read_trimmed(&cur.join("serial")).filter(|s| !s.is_empty()).unwrap_or_else(|| "-".into())`. This makes the identity canonical (`usb:vid:pid:-:iface`), routes it through the existing `absent` branch so it degrades to by-path, and also groups true clones for the ambiguity check. Add a fixture test with an empty `serial` file asserting degradation to by-path.

### Medium

#### 🟡 MEDIUM — snapshot() computes holds_lock by label-string comparison, so two same-labeled origins (concurrent `send`) both report as holder

`nexus-core/src/lock.rs:358`  ·  _LOCK-1_ · correctness  ·  verifier: **CONFIRMED**

In `snapshot()` the per-origin holder flag is derived from the *label string*, not the OriginId that is available as the BTreeMap key:

```rust
origins: self.origins.values().map(|o| OriginState {
    ...
    holds_lock: self.holder
        .and_then(|h| self.label(h))
        .is_some_and(|l| l == o.label),   // compares by label, not id
    ...
})
```

`origins` is iterated with `.values()`, discarding the key, so the only identity available is the label. When two distinct OriginIds share a label, the holder's label matches BOTH, and both get `holds_lock: true`. This is reachable in normal operation: the `send` verb registers a synthetic transient origin with a fixed label `"send"` and a distinct id (`SEND_ORIGIN_BASE=1<<40`, then `wrapping_add`) — see nexus-daemon/src/daemon.rs:823-827. Two clients running `send` against the SAME endpoint concurrently both register origins labeled "send" on the same lock cell; the first holds while the second waits in the FIFO queue (daemon.rs:807-862, connections served concurrently in control.rs). A `state` verb during that window calls `snapshot()` and reports an exclusive endpoint with TWO holders (`origins: [{origin:"send", holds_lock:true}, {origin:"send", holds_lock:true}]`), contradicting §6's at-most-one-holder guarantee in observed state. The `may_write` gate itself is unaffected (it is id-based, lock.rs:169), so this is a state-reporting/observability defect, not a data-path safety hole — but it misreports the core invariant to operators and AI agents driving the RPC surface. The stale-label window on `last_steal`/`holder` reuse is a milder instance of the same label-vs-id confusion.

**Recommendation.** Iterate `self.origins.iter()` and compute `holds_lock: self.holder == Some(*id)` (compare by OriginId, the authoritative identity), rather than comparing label strings. The same id-based comparison removes any dependence on labels being unique per endpoint.

#### 🟡 MEDIUM — Bare all-slash path input ("/", "//") is accepted and captured as raw:/ bound to the dev-root directory instead of being rejected as Malformed

`nexus-core/src/resolver.rs:222`  ·  _RESOLV-2_ · design-deviation · deviation: **should-fix**  ·  verifier: **CONFIRMED**

resolve_input routes any input starting with '/' to capture_from_path (line 153-154). capture_from_path computes `rooted = self.rooted(input)` and only fails when `!rooted.exists()` (line 224). For input "/", `rooted("/")` = `dev_root.join("/".trim_start_matches('/'))` = `dev_root.join("")` = the dev-root directory itself (line 127), which always exists. So capture proceeds: `file_name()` of "/" is None → dev_name = "" (line 229-232), sysfs_lookup("") and bypath_of("") both miss, and capture_for_dev returns `raw:/` with `path = Some(dev_root)` (lines 285-291). In production (`Resolver::new("/")`, dev_root="/") `add-node --device /` therefore succeeds, stores `raw:/` in config (daemon.rs:394 overwrites the device with `resolved.identity`), and the serial node then tries to open the root directory as a serial port. This is exactly the ill-formed-input class the audit explicitly rejected for `resolve_raw_identity` ("empty raw path", lines 205-210) and for empty input (line 137), but the bare-path branch has no equivalent guard. `"//"`, `"///"`, and `" / "` (trimmed) hit the same path.

**Recommendation.** In capture_from_path, reject an all-slash / empty-after-trim input the same way resolve_raw_identity does: if `input.trim_start_matches('/').is_empty()` return `ResolveError::Malformed { reason: "empty path" }`. Extend `empty_input_is_malformed` to cover `/` and `//`.

#### 🟡 MEDIUM — remove-node --cascade strands a `lock --wait` waiter parked for the removed writer origin on a surviving endpoint's lock

`nexus-daemon/src/daemon.rs:553`  ·  _DLC-1_ · reliability  ·  verifier: **CONFIRMED**

When removing a node, the cascade loop unregisters each of the removed node's writer origins from the SURVIVING host lock they fed, but only wakes that lock's waiters when the removed origin was the *holder* (`if released { host_lock.wake_waiters(); host_lock.emit_change(); }`, lines 551-559). `EndpointLock::unregister` (lock.rs:145-154) also drops the origin from the FIFO waiter queue and returns `false` when it was a queued waiter rather than the holder. So the not-held case wakes nothing.

Concrete triggering state: serial `usb0` (exclusive lock) with two PTY writers `pty0`, `pty1`; `pty1` holds usb0's lock; a CLI `lock --wait {origin:"pty0"}` is parked in usb0's FIFO queue, its task suspended on `usb0_lock.notified()` (wait_for_grant, lines 1031-1057), WaiterGuard armed. Operator runs `remove-node {node:"pty0", cascade:true}`. The loop hits `st.origin_locks.remove("pty0")` → `Some((usb0_lock, id0))`, calls `unregister(id0)` which retains-out pty0 from usb0's waiter queue and returns `false` (pty0 was a waiter, not holder). `released == false`, so `usb0_lock.wake_waiters()` is NOT called. Only the removed node's OWN host locks are `close()`d (lines 541-544); the surviving usb0 lock is neither closed nor woken. The parked `lock --wait pty0` task therefore stays suspended and does not observe that its origin no longer exists. It only resumes on the next unrelated wake of usb0's lock (a future release/steal by pty1); at that point it re-checks `is_closed()` (false — usb0 is alive), re-attempts `acquire(id0)` → `Acquire::ReadOnly` (origin gone) → returns the write=never error. If the current holder pty1 never releases (e.g. a lease-less indefinite hold), the CLI call hangs indefinitely — the exact 'park forever after removal' failure §6/§15.20 declares defined-away. The already-recorded audit fix (implementation-notes §6e) covers only the lock-HOLDING removed writer; the parked-WAITER case is uncovered.

**Recommendation.** After `unregister(origin_id)` in the cascade loop, wake the surviving lock unconditionally so a parked waiter for that origin re-evaluates and returns promptly: call `host_lock.wake_waiters()` (and `emit_change()`) whenever the origin was present, not only when `released` is true. Equivalently, treat 'removed origin that may have been queued' the same way teardown treats every waiter. Add a regression test mirroring the holder-case one: park a `lock --wait` on a writer, remove that writer's node while another origin holds the lock, and assert the waiting call returns a defined error rather than hanging.

#### 🟡 MEDIUM — Held-lock reclaim loses a scheduling race to a queued on-demand waiter on release-path wakes

`nexus-daemon/src/daemon.rs:795`  ·  _daemon-arbitration-1_ · design-deviation · deviation: **unsure**  ·  verifier: **CONFIRMED**

§6 states an absolute invariant: a `held` origin (a codec demultiplexer) reclaims its lock "ahead of every on-demand waiter the moment it frees", because "granting a queued waiter a demultiplexer's lock would corrupt the very framing the hold protects." Held priority is realized by `reclaim_held` (nexus-core/src/lock.rs:264), which bypasses the FIFO head, invoked from the codec's `ensure_holds` task (nexus-daemon/src/nodes/codec.rs:345). But every release path in daemon.rs — `unlock` (line 795), lease expiry (`spawn_lease`, line 998), a stealer's own unlock, and detach-release (`remove_node` line 557) — signals recovery via `cell.wake_waiters()`, i.e. `Notify::notify_waiters()`, which wakes ALL registered parkers: BOTH the demux's `ensure_holds` reclaim task AND any on-demand `lock --wait` waiter parked in `wait_for_grant` (line 1050). Whichever task's synchronous critical section runs first wins the now-free lock. The on-demand waiter's `acquire` (lock.rs:180) grants a free lock to the FIFO head unconditionally; it does NOT defer to a pending held-reclaim. So if the on-demand waiter's task is scheduled before the demux's task, `acquire` grants it the demultiplexer's lock — exactly the outcome §6 says must never happen. There is no enforced ordering between the two woken tasks on the shared `Notify`; the winner is data-dependent tokio scheduling.

**Recommendation.** Make held-priority deterministic rather than scheduling-dependent. Preferred fix in nexus-core/src/lock.rs `acquire`: when the lock is free but a registered `Held` origin exists that is not the caller, return `Denied`/defer instead of granting to an on-demand FIFO head, so a held origin's `reclaim_held` always wins regardless of which task is polled first. Alternatively, have the daemon's release paths attempt the held reclaim synchronously in the same critical section that frees the lock (before `wake_waiters`), so no on-demand waiter can interpose. Note this overlaps the deliberately-accepted `--steal` "§6 stall" behavior and requires an unusual topology plus a steal, hence medium/unsure; a maintainer should decide whether the stated invariant must hold against scheduling.

#### 🟡 MEDIUM — Codec/exec channel advertises a targetward path its node silently drops, so `send <codec>/<ch>` acquires the lock then fails with an internal error

`nexus-daemon/src/runtime.rs:294`  ·  _RUNTIME-1_ · reliability  ·  verifier: **CONFIRMED**

Wiring::build unconditionally creates a targetward tx/rx pair for EVERY host-facing endpoint (runtime.rs:294-296), including each interior codec/exec channel, and the daemon then copies host_targetward_tx into st.endpoint_targetward for every host endpoint (daemon.rs:352-356 in load, 462-464 in add_node) and endpoint_locks likewise. But the interior node only claims the channel's host_targetward_rx receiver when it can actually route targetward: codec.start early-returns 'Waiting' when the multiplexed side has no upstream (codec.rs:129-134, reachable directly via an edgeless `add-node` of a codec), and it skips the whole per-channel targetward block when the mux edge is read-only, i.e. mux_targetward_tx is None (codec.rs:163 — a demultiplexer whose device edge is write=never, a common config). In both paths the per-channel rx that build removed into the local `channel_rxs` vec is simply dropped, while endpoint_targetward still advertises the matching tx. exec.start mirrors this exactly (exec.rs:151-158 early return, exec.rs:163 the read-only skip). Consequence: `send <codec>/<channel>` (and the lock dance behind it) resolves both the lock and the sender, registers the transient origin, successfully acquires the (now-pointless) lock, and only fails at delivery — `sender.send(chunk).await.is_ok()` is false because the rx was dropped — returning a JSON-RPC internal error (-32603) 'endpoint ... targetward closed' (daemon.rs:869,879-884) instead of a defined 'endpoint not ready / read-only channel' error. No data loss and no hang (the closed channel errors immediately rather than blocking), but the daemon advertises a working targetward endpoint and then fails opaquely on use. The endpoint-keyed wiring's 'every host endpoint owns one arbitrated targetward channel' becomes an orphaned dead-end whenever the interior node cannot service it.

**Recommendation.** Make the advertised targetward path and its actual serviceability consistent: either (a) do not advertise endpoint_targetward/lock for a codec/exec channel whose interior node cannot route targetward (read-only or unattached mux), or (b) have codec.start/exec.start keep the per-channel receiver drained-and-counted (or reply with a defined 'not ready' app error) so `send`/`lock` fail with a meaningful, documented error class rather than a -32603 internal error. At minimum document the behavior as a known deviation in implementation-notes §3.

#### 🟡 MEDIUM — rotate silently no-ops on rename failure: no fault raised, counter not advanced, log keeps appending to the same file

`nexus-daemon/src/nodes/log.rs:329`  ·  _LOG-2_ · reliability  ·  verifier: **CONFIRMED** _(also independently reported as LOG-3)_

In the rotation step the rename result is only recorded as a boolean: `let renamed = std::fs::rename(&current, &rotated).is_ok();` (line 329). On rename failure the code still reopens `current` in append mode (which succeeds because appending to an already-existing, writable file needs write permission on the FILE, not the directory), then at lines 343-345 only advances `q.rotation` `if renamed`, decrements rotate_pending, and does NOT fault. Net effect on a rename failure: no .NNN file is created, the rotation counter does not advance, the node stays Active, and the writer keeps appending to the original file indefinitely — a silent failed rotation. Realistic triggers: a log directory with read-only/permission-restricted entry perms but a writable current file (rename gets EACCES/EROFS while append-open succeeds), or the current file having been renamed/removed out from under the daemon by an external process (rename gets ENOENT, then create(true) reopens a fresh empty current). The operator's `rotate` RPC returned a rotation number implying success while nothing rotated.

**Recommendation.** Distinguish rename failure from success: on a hard rename error either fault the node (as ENOSPC does) or surface the failure to the rotate caller, rather than swallowing it and continuing to append to the unrotated file.

#### 🟡 MEDIUM — Child stderr is read with unbounded line buffering (memory exhaustion)

`nexus-daemon/src/nodes/exec.rs:437`  ·  _CODEXEC-1_ · reliability  ·  verifier: **CONFIRMED**

The stderr pump arm does `let mut lines = BufReader::new(stderr).lines();` then `while let Ok(Some(line)) = lines.next_line().await`. `tokio::io::Lines::next_line` accumulates bytes into an internal String via `read_line` until it sees a `\n` or EOF, with no size cap. The exec codec is explicitly the escape hatch that runs arbitrary third-party protocol tools unmodified (§7.6/§13). A child that streams to stderr without newlines — a progress spinner using `\r`, binary/hex diagnostics, a core dump, or simply `cat /dev/urandom 1>&2` from a wrapper — makes this String grow without bound while the daemon keeps its user's serial ports running for days/weeks, driving the daemon toward OOM. Because stderr is continuously drained (the errs arm) there is no pipe-fill deadlock, so the failure is silent memory growth rather than a stall.

**Recommendation.** Bound the stderr read: use a fixed-size `stdout.read(&mut buf)` loop and log/truncate per read chunk instead of `.lines()`, or cap the accumulated line length (e.g. split at a max byte budget and emit a 'stderr line truncated' marker). Match the bounded READ_BUF discipline already used for stdout.

#### 🟡 MEDIUM — No request line-length bound: one connection can OOM the shared daemon

`nexus-daemon/src/control.rs:29`  ·  _CTRL-1_ · reliability  ·  verifier: **CONFIRMED**

`serve_connection` reads requests with `BufReader::new(read_half).lines()` and `lines.next_line()` (lines 29, 35). tokio's `next_line` grows its internal String without any upper bound until it sees a `\n`. A single connection that streams bytes with no newline (an accidental binary blob piped into the socket, a `cat file > /dev/…` mistake, a buggy client, or a large paste) forces the daemon to buffer the entire stream in memory before parsing even begins, exhausting RAM and taking down ALL consoles the daemon serves — not just the offending session. No `.take(N)` / length cap exists anywhere in the read path (confirmed: `nexus-rpc::parse_incoming_request` also imposes none), and the assignment/design (§10 'a page of serde types … everything debuggable with socat and jq') implies bounded, well-behaved framing. The socket-auth trust model (0600) means the peer is authorized, so this is not a privilege escalation, but an unbounded per-connection allocation on shared infrastructure is a real availability gap and is undocumented (implementation-notes §7 bounds the socket PATH via SUN_LEN but says nothing about request line length).

**Recommendation.** Wrap `read_half` in a length-limited reader (e.g. `read_half.take(MAX_LINE)`), or after `next_line` returns, reject any line whose length exceeds a documented cap (a few KiB is ample for JSON-RPC control verbs) with a JSON-RPC error and close. Document the bound alongside the SUN_LEN note.

#### 🟡 MEDIUM — purge-on-reconnect leaves an in-flight backpressured chunk (and re-read kernel backlog) able to fire into the just-reconnected device

`nexus-daemon/src/nodes/serial.rs:395`  ·  _XC-PURGE-1_ · design-deviation · deviation: **should-fix**  ·  verifier: **CONFIRMED**

purge_on_reconnect (serial.rs:395, called at serial.rs:339) drains ONLY the parked targetward mpsc receiver via a synchronous `while rx.try_recv()` loop, then arm_reader + set_active run and the supervisor's next await is active_step's `rx.recv()`. But the design (serial.rs:180-181, confirmed) makes a waiting port backpressure its origins by letting the bounded channel (CHANNEL_CAP = 256) fill; the lock-holding origin's producing task is then suspended INSIDE `tx.send(chunk).await` (e.g. pty.rs:520) holding one already-read, outage-era chunk. try_recv frees channel permits but, being synchronous with no await, never lets that blocked sender run during the drain. Immediately after purge, on the first `rx.recv()` await, the blocked send completes and pushes its stale chunk, which active_step then writes into the freshly reopened (likely power-cycled) port — exactly the boot-prompt hazard §7.1 exists to prevent. Worse, that same holder task then loops and re-reads more outage-era bytes still sitting in its source kernel buffer and, now Active, forwards those too. None of this is added to purged_on_reconnect, so the counter also undercounts. The identical pattern exists in the leg at leg.rs:503-514 (faces=host send_receivers drain). impl-notes:951 punts 'origin buffers' to the lock-purge, but the lock-purge (purge-on-acquire) never fires here because the holder holds through the outage, and the in-flight chunk is in the daemon's own pipeline, not an origin buffer — so it is covered by neither purge. The existing test (impl-notes:165) only exercised a single parked command that fit in the channel, never the channel-full + blocked-sender case.

**Recommendation.** Recognize that draining the parked receiver is not sufficient while a producer is backpressured mid-send. Options: after emptying the channel, yield once (or drain in a loop that awaits) so any in-flight blocked send resolves and is drained+counted before set_active; or gate the holder's targetward drain on device liveness (not just the lock) so it stops producing while the node is waiting; or drain the origin source once at reopen. At minimum, count any bytes that slip through so purged_on_reconnect stays exact, and add a regression test that fills the targetward channel during the outage and asserts nothing outage-era reaches the reopened device.

### Low

#### 🔵 LOW — try_decode_hello pre-allocates from an untrusted announcement count, amplifying a 16-byte hello into a ~1.5 MB allocation

`codec-api/src/lib.rs:446`  ·  _CODECAPI-1_ · reliability  ·  verifier: **CONFIRMED** _(also independently reported as XC-FUZZ-1)_

On the hostile-facing wire-hello decode path, `count` is read straight from the untrusted body (`let count = u16::from_be_bytes([body[10], body[11]]) as usize;`, line 445) and then used directly to size the result Vec: `let mut channels = Vec::with_capacity(count);` (line 446). No cross-check against how many announcement bytes the body can actually contain is done first. A peer connecting to a listen leg can send a fully-formed but minimal hello — 4-byte length prefix + a 12-byte body (magic + WIRE_VERSION + caps=0 + count=0xFFFF) with zero announcement bytes = 16 bytes total — which passes the magic and version checks, then drives `Vec::with_capacity(65535)`. Since `ChannelId` wraps a `String` (24 bytes on 64-bit, line 41), that reserves ~1.5 MB of backing storage; the very first loop iteration then hits `WireError::Truncated("announcement length")` (line 450) and the buffer is freed. The allocation is bounded (u16 caps count at 65535) and transient (freed on the immediate error, and the handshake runs under one overall deadline), so this is not a §9-clause-4 bounded-memory violation — but it is a ~100,000x input-to-allocation amplification triggerable per connection attempt and repeatable via reconnect, in a codebase whose audits specifically police disproportionate allocation on hostile decode paths. body_len is already bounded by MAX_FRAME_SIZE and each real announcement needs >=2 bytes, so the true announcement ceiling is ~32K regardless of what count claims; trusting count over the body length is the defect.

**Recommendation.** Do not trust `count` for the allocation size. Either drop the pre-sizing entirely (`let mut channels = Vec::new();` — the per-iteration pushes amortize fine for the ≤~32K real ceiling) or clamp it to what the remaining body can hold, e.g. `Vec::with_capacity(count.min((body.len() - 12) / 2))`. Growth then stays proportional to bytes actually received.

#### 🔵 LOW — BlockingReader::arm silently detaches a still-running reader on precondition violation (fd-reuse hazard left unenforced)

`nexus-daemon/src/boundary.rs:154`  ·  _BCELL-1_ · reliability  ·  verifier: **PLAUSIBLE**

The whole thesis of this module (docstring lines 8-13, design §16.1) is to make the boundary invariants — including 'join before the fd drops (fd-reuse-safe)' — structural rather than a by-hand rule. But `arm` enforces its own stated precondition ('Any previously-armed reader must already be joined via stop_join') only in prose. If a future caller calls `arm` twice without an intervening `stop_join`, line 168 `self.handle = Some(handle)` overwrites the old `Some(handle)`, dropping it. Dropping a `JoinHandle` detaches the thread: the previous reader keeps running on the previous fd. Since callers commonly drop/reuse that fd right after re-arming, this is exactly the fd-reuse race the module claims to have made unrepresentable — reintroduced as an unchecked precondition. The current sole caller (serial.rs supervise loop, which always stop_join_reader()s before re-arm) is correct, so this is latent, not live; hence low. Nothing detects the violation, not even in tests.

**Recommendation.** Add `debug_assert!(self.handle.is_none(), "arm called on an un-joined reader; call stop_join first");` at the top of `arm` so a contract violation trips in tests/debug builds instead of silently detaching a live reader on a reused fd. Optionally `stop_join()` defensively before re-arming.

#### 🔵 LOW — Reader-thread spawn failure panics the supervisor task via .expect instead of surfacing a fault

`nexus-daemon/src/boundary.rs:165`  ·  _BCELL-2_ · reliability  ·  verifier: **CONFIRMED**

`arm` unwraps the thread-spawn result with `.expect("spawn blocking reader thread")`. `std::thread::Builder::spawn` returns Err on OS resource exhaustion (EAGAIN — thread/PID limit, e.g. RLIMIT_NPROC) — reachable on a busy host managing many reconnecting serial ports. `arm` has no fallible return, so the failure panics. With the workspace's default unwind panic strategy (no `panic = "abort"` in any Cargo.toml), tokio isolates the panic to the supervise task, which then silently dies: the serial node is left stuck in whatever status it last set, never re-armed and never faulted, with no supervisor to recover it. This diverges from the module's own model where device/environment loss faults the node and retries rather than killing its supervisor. Kept low because it requires OS resource exhaustion.

**Recommendation.** Make `arm` return `io::Result<()>` (propagate the spawn error) and have the caller route it through the node's existing fault path (arm_reader already returns `io::Result` and calls `fault(...)` on error), rather than `.expect`-panicking the supervisor task.

#### 🔵 LOW — Serial bytes silently dropped uncounted when a consumer is cascade-removed while the serial node lives

`nexus-daemon/src/nodes/serial.rs:508`  ·  _SERIAL-3_ · reliability  ·  verifier: **CONFIRMED**

reader_thread decides discard-vs-broadcast on `hostward.is_empty()` (line 496) and treats a per-sink `TrySendError::Closed` as 'not a boundary drop' (line 508, silently ignored). The hostward Vec is snapshotted at arm time and never rebuilt for a surviving node. `remove-node <consumer> --cascade` (daemon.rs remove_node) tears down the consumer and drops the S↔consumer edge but does NOT re-arm the surviving serial producer's reader. The serial node's reader therefore keeps a now-permanently-Closed sink: every subsequent chunk hits try_send→Closed and is dropped WITHOUT counting — not against discarded_unattached (hostward is non-empty) and not against any consumer's DropCounters. If that was the serial node's only consumer, all hostward output is lost invisibly, contradicting §5's guarantee that loss is 'always visible and attributable'. The Closed=ignore rule is sound for teardown of the whole node but wrong for a persistently-closed sink on a surviving producer.

**Recommendation.** Either re-arm the serial reader with an updated hostward set when an attached consumer is removed, or count a Closed sink's bytes as discarded (e.g. fold them into discarded_unattached once every sink for a chunk is Closed) so the loss stays attributable.

#### 🔵 LOW — prime_slave open failure is silently swallowed, inverting initial presence to phantom-present

`nexus-daemon/src/nodes/pty.rs:682`  ·  _PTY-2_ · reliability  ·  verifier: **CONFIRMED**

prime_slave() discards the result of open() (`if let Ok(fd) = open(...) { drop(fd) }`, lines 682-686) and setup() ignores it entirely (line 165). The whole presence model depends on priming: per §3.2 a never-opened master does NOT report POLLHUP. If the prime open ever fails, the master never enters the primed 'absent' HUP state, so read_and_poll's first poll computes `now = !POLLHUP = true` (line 484) and `present_now = true` (line 557), reporting `client_present=true` from creation with no client attached — the writer thread then blocks-writing into a slave nobody holds instead of discarding-with-count, and detach-release/purge never fire correctly. While hard to trigger for the daemon that just allocated the pts (root can always open it), the failure is swallowed rather than faulting the node, contrary to §3.2 treating priming as load-bearing.

**Recommendation.** Have prime_slave return Result and let setup() fault the node (or at least log) when the prime open fails, so presence detection is never left inverted.

#### 🔵 LOW — No lower bound on hostward_buffer; a value of 0 builds a rendezvous channel that drops nearly all hostward output

`nexus-daemon/src/nodes/pty.rs:280`  ·  _PTY-3_ · reliability  ·  verifier: **CONFIRMED**

The bridge is `std_mpsc::sync_channel::<Chunk>(self.hostward_buffer)` (line 280) and hostward_buffer comes straight from config with no validation (nexus-core/src/config.rs:219-220; GraphConfig::validate has no bound check). A config with `hostward_buffer = 0` yields a zero-capacity rendezvous channel: btx.try_send (line 285) returns Full unless the writer thread is blocked in recv at that exact instant, so almost every hostward chunk is dropped-and-counted as add_full even for a fast, fully-present consumer. The output is technically bounded and counted, but silently near-dead — a surprising foot-gun with no guard rail.

**Recommendation.** Reject hostward_buffer == 0 in GraphConfig::validate (or clamp to >=1 at node build), mirroring the other drop-policy fields.

#### 🔵 LOW — Writer-thread spawn failure panics instead of faulting the node

`nexus-daemon/src/nodes/pty.rs:300`  ·  _PTY-6_ · reliability  ·  verifier: **PLAUSIBLE**

start() spawns the blocking writer with `.expect("spawn pty writer thread")` (line 300). std::thread::Builder::spawn returns Err on resource exhaustion (EAGAIN / thread limit); .expect turns that environmental condition into a panic on the runtime thread rather than faulting the node per §15.8. Low probability, but every other environmental failure in this file faults the node (setup(), apply_perms) instead of panicking.

**Recommendation.** Handle the spawn Err by setting self.status = Faulted with the error rather than panicking.

#### 🔵 LOW — Under overflow=fault, the in-flight batch lost when a write(2) error faults the writer is not counted in dropped_bytes

`nexus-daemon/src/nodes/log.rs:300`  ·  _LOG-6_ · correctness  ·  verifier: **CONFIRMED**

On a write error under OverflowPolicy::Fault the writer sets Faulted and `return`s immediately (lines 301-307), abandoning the failing chunk and every remaining chunk in the already-drained `batch` (queued_bytes was reset to 0 at line 290). None of those abandoned bytes are added to dropped_bytes — the Fault arm, unlike the DropOldest arm (line 309 `q.dropped_bytes += chunk.len()`), does not count them. So the module's stated invariant 'loss is always counted' is violated for the batch present at fault time: those bytes vanish uncounted. The Fault status itself is visible, so it is not fully silent, but the dropped_bytes total under-reports by up to one batch (bounded, since subsequent overflow losses are counted by the pump).

**Recommendation.** Before returning from the Fault write-error arm, add the unwritten chunk(s) of the current batch to dropped_bytes so the reported loss stays exact, matching the counting done everywhere else.

#### 🔵 LOW — Child-emitted targetward (mux-channel) data is silently discarded uncounted when the serial has no targetward path

`nexus-daemon/src/nodes/exec.rs:463`  ·  _CODEXEC-3_ · correctness  ·  verifier: **CONFIRMED**

In `route_event`, a `data("", …)` frame the child emits (device-bound remux output) is written only inside `if let (Some(tx), Some((lock, id))) = (mux_targetward_tx, serial_lock)`. When either is `None` — the mux endpoint has no targetward-capable serial edge (e.g. the codec↔serial edge registered `write = never`, or a hostward-only attachment) — the branch is skipped and the child's device-bound bytes are dropped with no counter, unlike the hostward side which attributes discards via `discarded_unattached`/`dropped_slow_consumer`. This is an uncounted §5 loss. Reachability is narrow (normal held codec↔serial edges provide both), so severity is low, but there is no attribution if it occurs.

**Recommendation.** When the mux targetward path is absent, count the dropped bytes against a mux-side targetward-discard counter (mirroring `multiplexed.dropped_slow_consumer`) so the loss stays located, or come up faulted/waiting if a target-facing exec codec has no writable serial edge.

#### 🔵 LOW — Per-channel `active` flag never resets on disconnect; stays `true` while leg is waiting

`nexus-daemon/src/nodes/leg.rs:538`  ·  _LEG-3_ · correctness  ·  verifier: **CONFIRMED**

`ChannelStat.active` is documented as "Whether any data has crossed the channel since connect." It is set `true` on data/Open and set `false` only on an explicit `Close` wire event (route_recv L718-721). The disconnect cleanup block clears `bound`, `unbound`, and `peer_address` but does NOT reset `active`. So a channel that ever saw data reports `active: true` in `state_extra` even after the peer disconnects and the connection shows `waiting`, and it stays sticky-true across reconnects until a Close frame arrives. This mildly misrepresents per-channel liveness during an outage.

**Recommendation.** Reset `stat.active.set(false)` in the disconnect cleanup loop alongside `bound`, so `active` reflects the current connection rather than any past one.

#### 🔵 LOW — Disconnect-release can be missed (Notify stores no permit), holding the on-demand lock until idle

`nexus-daemon/src/nodes/leg.rs:542`  ·  _LEG-4_ · reliability  ·  verifier: **CONFIRMED** _(also independently reported as XCAWAIT-1)_

The supervisor pulses `disconnect.notify_waiters()` on connection loss so each `channel_targetward` task promptly releases its on-demand write lock (design §7.1, and the named §16 audit fix). `Notify::notify_waiters()` wakes only tasks currently registered as waiters and stores no permit. A `channel_targetward` task only listens on `disconnect.notified()` inside its `holding` select arm; while it is outside that select — notably blocked in `tx.send(bytes).await` (L788) feeding a backpressured local consumer, or in `ensure_acquired().await` — the pulse is lost. When the send later completes, the task re-enters the select but the pulse is gone, so the lock is released via the `idle` timer instead of promptly on disconnect. With a slow local device and a large `idle_release_ms`, a local operator can stay blocked behind the vanished remote for up to that interval, partially defeating the disconnect-release guarantee (it still self-heals via idle).

**Recommendation.** Have each task observe disconnect around its blocking write as well (e.g. `select!` the `tx.send` against `disconnect.notified()`), or gate the local write on a per-connection epoch/flag that the supervisor bumps on loss so a task re-checks disconnect after every send rather than relying solely on the transient pulse.

#### 🔵 LOW — RunOptions::default() yields an empty dev_root that breaks device resolution for embedders

`nexus-daemon/src/lib.rs:81`  ·  _CTRL-2_ · design-deviation · deviation: **should-fix**  ·  verifier: **CONFIRMED**

`RunOptions` derives `Default` (lib.rs:68) and `dev_root: PathBuf` is a plain field (not `Option`), so `RunOptions::default()` leaves `dev_root = ""` (empty). In `serve`, this is fed to `nexus_core::Resolver::new(options.dev_root.clone())` (lib.rs:137), and `Resolver::new` computes `sys_root = dev_root.join("sys")` and resolves device paths via `dev_root.join(abs.trim_start_matches('/'))` (resolver.rs:112,127). With an empty `dev_root` these become RELATIVE paths (`sys`, `dev/serial/by-id`, …) resolved against the daemon's CWD instead of `/`, so device-identity resolution silently reads the wrong tree. The §15.26 entry API is meant to be embedder-friendly and the field doc explicitly states '`/` in production', yet the derived Default contradicts that. The in-tree binary is unaffected (clap sets `default_value = "/"`), and the doc example sets `dev_root` explicitly, but an embedder writing `RunOptions::default()` (a natural thing given every other field is a sensible Option/None) gets a silently-broken resolver.

**Recommendation.** Either make `dev_root: Option<PathBuf>` (None → `/`, matching the socket/state_file pattern), or hand-implement `Default for RunOptions` so `dev_root` defaults to `PathBuf::from("/")`. Failing that, treat an empty/relative `dev_root` as `/` in `serve`.

#### 🔵 LOW — A write-half EOF (half-close) cancels an in-flight waiting verb, defeating the advertised `echo | socat` idiom

`nexus-daemon/src/control.rs:80`  ·  _CTRL-3_ · reliability  ·  verifier: **PLAUSIBLE**

The biased inner select (control.rs:74-87) races the dispatch future against `lines.next_line()`; ANY resolution of that second `next_line` — including `Ok(None)`, i.e. a clean EOF on the client's WRITE half — hits the `break` arm (line 85) which abandons the in-flight verb. For fast verbs this is harmless (the response was already produced on the biased first poll). But for the WAITING verbs (`lock --wait`, `send`'s acquire-with-timeout — the very reason dispatch is async, §15.20), a client that finishes sending its request and then closes only its write end while keeping its read end open to receive the eventual grant will have the wait cancelled the instant EOF is observed, its FIFO waiter dequeued, and the connection closed with NO response. This is exactly the shape of the `echo '{…}' | socat - UNIX-CONNECT:sock` idiom the design advertises (§10 'debuggable with socat and jq'): the pipe EOFs stdin, socat half-closes, and `lock --wait` returns nothing. The design's cancel-on-disconnect (§15.20) targets a *dropped connection*; a half-close is a mild over-application. The in-tree `serialnexusctl` is unaffected because `call()` keeps the socket fully open across the read (serialnexusctl/src/main.rs:434-440), so this only bites raw socket/socat users of waiting verbs.

**Recommendation.** Distinguish EOF (`Ok(None)`) from a pipelined line in the inner select: on `Ok(None)` while a waiting verb is in flight, consider continuing to await the dispatch (the write half may still be live) rather than breaking immediately, or document that waiting verbs require the connection's write half to stay open. At minimum note this limitation next to the §10 socat claim.

#### 🔵 LOW — Log-rotation counter overflow on a hostile-named file in the log directory

`nexus-daemon/src/nodes/log.rs:185`  ·  _xc-panics-1_ · reliability  ·  verifier: **CONFIRMED**

The log node recovers its rotation suffix by scanning the log directory: `scan_rotation` (log.rs:363) reads every entry, strips the `<filename>.` prefix, and `parse::<u64>()`s the suffix, keeping the maximum in `q.rotation: Option<u64>`. The directory contents are environment-controlled, not produced only by the daemon. Two sites then compute `q.rotation.map_or(0, |n| n + 1)`: the rotate-request/status path (log.rs:185, `q.rotation.map_or(0, |n| n + 1) + u64::from(q.rotate_pending)`) and the writer loop (log.rs:326). If a file named exactly `<filename>.18446744073709551615` (u64::MAX) exists in the configured log directory, `scan_rotation` returns `Some(u64::MAX)` and the subsequent `n + 1` overflows: in a debug build this is an arithmetic-overflow panic reachable from directory contents; in a release build it wraps to 0, silently defeating the monotonic-rotation invariant (§7.3 'higher is newer') so a later rotation overwrites `<filename>.0`. A well-behaved daemon can never reach u64::MAX by counting (that is 2^64 rotations), so this is only reachable by an attacker/operator who can plant a specifically-named file in the daemon's log directory, which is why the severity is low; but the fix is free and removes both the debug panic and the release monotonicity break.

**Recommendation.** Use `n.saturating_add(1)` at both log.rs:185 and log.rs:326 (or clamp/ignore an out-of-range parsed suffix in `scan_rotation`). Saturating is the natural choice: it keeps the counter pinned at the maximum rather than wrapping, so no crash and no filename collision.

#### 🔵 LOW — listen+unix leg never unlinks its socket file on teardown/removal, unlike the control socket and PTY symlink

`nexus-daemon/src/nodes/leg.rs:335`  ·  _LEG-1_ · reliability  ·  verifier: **CONFIRMED** _(also independently reported as LEG-5)_

For `role = listen, transport = unix`, `bind_listener` (lines 959-968) creates a filesystem socket inode (after a best-effort stale unlink of any prior one), but nothing ever removes that file. `teardown` (lines 335-339) and `Drop` (lines 342-348) only abort tasks; the `UnixListener` (owned by the aborted supervise future) closes the fd on drop but does not unlink the path. So `remove-node <leg>`, `load --replace`, the `teardown` verb, or clean shutdown all leave an orphan socket file behind. This is inconsistent with the control socket (removed on clean shutdown, lib.rs:192) and the PTY symlink (meticulously unlinked on teardown and Drop, pty.rs:352/369). Functionality is not broken — a later leg binding the same address runs the stale-socket unlink dance first — but a removed listen+unix leg with no subsequent rebind leaves a permanent stray inode.

**Recommendation.** Track the bound unix address and `std::fs::remove_file(address)` in the leg's `teardown` (and `Drop`) when role=listen and transport=unix, mirroring the PTY symlink cleanup and the control-socket removal.

#### 🔵 LOW — ReferenceCodec::demux resync is O(n^2) on a large oversize-prefix garbage buffer

`codecs/reference/src/lib.rs:78`  ·  _XC-FUZZ-2_ · reliability  ·  verifier: **CONFIRMED**

`demux` first does `self.buf.extend_from_slice(input)` (line 90), then loops. On a buffer whose leading 4 bytes decode to `body_len > MAX_FRAME_SIZE`, `try_decode` returns Err, `resync()` drops just the 4-byte prefix (`self.buf.drain(..4)`, line 78 with skip=4) and re-scans. `Vec::drain(..4)` shifts the entire remaining buffer left, i.e. O(remaining). For an input chunk that is all oversize-prefix garbage (e.g. a run of 0xFF bytes: each 4-byte window reads as body_len ~= 0xFFFFFFFF), this performs ~len/4 drains of O(len) each — O(len^2) work within a single demux call. The loop always terminates (each resync removes >=4 bytes) and the buffer stays bounded at call end, so this is not the flagged non-termination; it is a CPU cost. Realistic exposure is limited because per-read chunks are bounded by the runtime read size and the link codec over reliable TCP never resyncs — but a malicious/buggy wire peer streaming garbage could pin CPU at O(chunk^2) per chunk. The `reference_demux` fuzz target uses libFuzzer's small default max_len, so the quadratic never manifests as a timeout there.

**Recommendation.** Track a consumed-offset cursor and drain once (or use a VecDeque / `bytes::BytesMut::advance`) instead of draining from the front of a Vec on every resync/decode, so resync over a garbage run is O(n) rather than O(n^2). The same front-drain pattern in `FrameDecoder::next_event` and the demux success path (`self.buf.drain(..consumed)`) benefits identically.

## 2. Testing coverage opportunities

Gaps where a design invariant or a real bug class is not exercised by any unit/property test or validation script.

#### 🟡 MEDIUM — Conformance kit round-trips only `data` events; open/close/error control events are never proven to survive a codec

`codec-api/src/test_support.rs:74`  ·  _OPT-TEST-1_  ·  verifier: **CONFIRMED**

`round_trip_identity` (line 74) only muxes `Event::data`, and its collector `demux_data` (line 56) discards every non-`Data` event. §8's per-channel vocabulary is data/open/close/error, and the kit is the executable contract a *closed-source* external codec runs to prove conformance without forking serial_nexus (its whole stated purpose). Nothing in the generic kit ever mux→demux round-trips an `open`, `close`, or `error` event, so a codec that drops, misroutes, or corrupts control events passes every suite. A concrete failing consumer: a codec whose `mux` mishandles `EventKind::Open` (e.g. emits the wrong channel id or nothing) ships green through `round_trip_identity`, `fragmentation_tolerance` (data-only), `handles_garbage`, and `bounded_parser_state`. The reference codec's own `mux_then_demux_round_trips` does cover the four kinds, but that assurance is bespoke and does not transfer to kit consumers.

**Recommendation.** Add a control-event round-trip suite (or extend `round_trip_identity`) that muxes an open/data/close/error sequence per channel and asserts kind-and-channel identity after demux, gated to codecs whose demux surfaces control events. Add a deliberately-broken 'drops-open' fixture to the negative self-tests so the new suite is proven to fail.

#### 🟡 MEDIUM — Loopback gate's bare-IP branch (the flagged bare-IPv6 hazard) is never exercised by a test

`nexus-core/src/config.rs:126`  ·  _OPT-TEST-2_  ·  verifier: **CONFIRMED**

`is_loopback_addr` is the security-relevant classifier that decides whether a tcp leg needs the `insecure_bind` confession. Its bare-IP branch (line 126: `else if let Ok(ip) = address.parse::<IpAddr>() { return ip.is_loopback(); }`) is the exact bare-IPv6 case the surrounding comment (lines 118-119) calls out as hazardous ('rsplit_once(':') on a bare `::1` would wrongly split the address'). But every address in `non_loopback_leg_without_insecure_bind_is_rejected` (config.rs:908-918) is either a `host:port` form (else branch, line 131) or bracketed `[::1]` (bracket branch) — none is a bare IP with no port, so line 126-128 has zero coverage. A bare loopback `::1` (must be accepted) or a bare non-loopback `2001:db8::1` (must require insecure_bind) is entirely unverified; a regression that flipped `is_loopback()` or mis-split the bare form would not be caught.

**Recommendation.** Extend the test with bare-IP cases: `::1` and `127.0.0.1` (loopback, accepted) and `2001:db8::1` / `10.0.0.5` bare (non-loopback, rejected without insecure_bind), pinning the branch the comment flags.

#### 🔵 LOW — §5 anti-stranding guarantee (holdover drained on writability with a quiescent origin) has no deterministic test; only a probabilistic prop test guards it

`/home/pwnall/workspace/serial-nexus/nexus-core/src/data.rs:349`  ·  _DATAPLANE-1_  ·  verifier: **CONFIRMED**

The load-bearing line for the anti-stranding refinement is `self.sink.flush()` at the top of `Origin::pump` (line 349). It is only necessary in exactly one state: a chunk is parked in the interior holdover, `pending` is empty (quiescent origin), and the boundary has become writable — there is no new `deliver_targetward` call to trigger `InteriorTargetward::deliver_targetward`'s own step-1 `flush_holdover` (line 273). None of the three targetward unit tests exercise that state. I traced all three: `targetward_busy_pauses_the_offering_origin_only` and `interior_holds_at_most_one_chunk` never call resume/flush; `paused_origin_drains_in_order_on_resume` resumes with `pending = [BBB,CCC]` NON-empty and holdover=AAA, so the subsequent `deliver_targetward(BBB)` step-1 flush drains AAA even if pump's pre-flush were removed. Concretely: delete line 349 (`self.sink.flush();`) and all three unit tests still pass; only `prop_targetward_no_loss_bounded_interior` fails, and only on schedules whose final offer parks into an empty holdover leaving `pending` empty — a non-deterministic trigger. So the guarantee the §3.3 refinement was created to protect is guarded only by a flaky-by-construction property test.

**Recommendation.** Add a focused, deterministic regression test: with `Origin::new(InteriorTargetward::new(BusyBoundary::new()))`, set boundary busy, `offer` one chunk (it parks in the holdover, origin is NOT paused, `pending_bytes()==0`), then set boundary not-busy and call `resume()` with no further `offer`; assert the byte reaches `downstream().received()` and `held_bytes()==0`. This pins line 349's necessity so a future refactor cannot silently reintroduce stranding.

#### 🔵 LOW — Tests never exercise two origins sharing a label, nor Held/reclaim_held/renew in the property model, leaving LOCK-1 and held-priority interleavings uncovered

`nexus-core/src/lock.rs:734`  ·  _LOCK-2_  ·  verifier: **CONFIRMED**

Every unit test and the `prop_exclusive_invariants` property test assigns each OriginId a unique label (`"a"`,`"b"`,`format!("o{i}")`), so the label-vs-id snapshot defect (LOCK-1) is structurally invisible to the suite — no test ever registers two origins with the same label, which is exactly the reachable `send`/`send` case. Additionally the property model (lock.rs:711-784) only drives four `OnDemand` origins and the op set Acquire/Release/Detach/Reattach/Enqueue/Dequeue/Steal — it never registers a `Held` origin, and never calls `reclaim_held`, `renew`, or checks `snapshot()` invariants. The held-priority reclaim path (§15.23, the one that outranks the FIFO queue and is the subtlest transition in the module) is covered only by a single scripted unit test (`held_origin_reclaims_...`), not by randomized interleaving, so a held/steal/waiter race that lost or double-granted would not be caught.

**Recommendation.** Add a snapshot invariant to the property test asserting exactly one `OriginState` has `holds_lock == true` and it equals `holder()` (this would have caught LOCK-1 once a same-label origin is introduced); extend the op model with a `Held` origin plus `ReclaimHeld` and `Renew` ops and assert holder-never-queued / single-holder still hold; add a case with two identically-labeled origins.

#### 🔵 LOW — Reference codec's own tests exercise only the intact-prefix resync branch, not the mangled-prefix or short-body branches

`codecs/reference/src/lib.rs:177`  ·  _REFCODEC-3_  ·  verifier: **CONFIRMED**

The bespoke resync tests (corrupt_type_byte_resyncs_exactly_and_counts, corrupt_channel_id_utf8_resyncs) cover only the recoverable branch of resync() — an intact body_len <= MAX_FRAME_SIZE with a body-decode error, exact skip = 4 + body_len. Two distinctive branches have no direct test: (a) the mangled-length-prefix branch (body_len > MAX_FRAME_SIZE → the `else { 4 }` skip at line 76), whose counting and best-effort re-alignment behavior is only touched incidentally by assert_buffer_bounded (which ignores the framing_errors count and does not assert re-alignment to a following valid frame); and (b) the truncated-header branch, where a body_len of 0, 1, or 2 (<= MAX but structurally impossible) makes try_decode return Err(Truncated) and resync skips 4 + body_len. Given REFCODEC-1 lives in branch (a), the absence of a test asserting the framing-error count on that branch is the reason that behavior went unnoticed.

**Recommendation.** Add a test that prepends an oversize (>MAX_FRAME_SIZE) length prefix ahead of a valid frame and asserts demux still recovers the valid frame and that framing_errors matches the intended semantics; add a test with a body_len of 2 to cover the truncated-header skip.

#### 🔵 LOW — docs↔registry test pins only the numeric code column, not the name/summary text it carries

`nexus-rpc/src/lib.rs:611`  ·  _RPC-1_  ·  verifier: **CONFIRMED**

The §16.8 test `docs_rpc_table_matches_the_registry` only compares the *set of numeric codes* between `error_code_registry()` and `docs/rpc/README.md` (it reads `d.code` at line 619 and matches backtick-wrapped integers). The `name` and `summary` columns that `error_code_registry()` deliberately carries (`ErrorCodeDoc.name`/`.summary`, populated from `AppError::name()`/`AppError::summary()` at lines 401-405 and from the five standard-code string literals at lines 375-399) are consumed nowhere except this test, and the test never reads them. I confirmed by grep that `ErrorCodeDoc.name`/`.summary` and `AppError::name()`/`summary()` have no other consumer in the workspace. Consequence: the README table's name/summary columns (README lines 137-146) can drift arbitrarily from the registry with zero test failure. Concrete scenario: change `AppError::Locked.summary()` (lib.rs:347) or edit README line 144's description text — the docs and the single-source registry silently diverge, yet the gate stays green because `-32003` still appears in both. This also makes the doc claim at lib.rs:291 ("the `docs/rpc` error table ... are asserted from it") and daemon.rs:53-56 overstated: only the code column is asserted, not the table.

**Recommendation.** Either (a) strengthen the test to also assert each registry row's name/summary appears in the corresponding README table row, or (b) soften the comments at lib.rs:291 and daemon.rs:53-56 to say only the code set is asserted. Option (a) makes the name/summary fields load-bearing as intended.

#### 🔵 LOW — Envelope hostile-decode error variants (BadChannelId, BadErrorMessage, Truncated) have no regression test

`codec-api/src/lib.rs:229`  ·  _OPT-TEST-3_  ·  verifier: **CONFIRMED**

`try_decode` maps four hostile-input conditions — invalid-UTF8 channel id → `EnvelopeError::BadChannelId` (line 255), invalid-UTF8 error message → `BadErrorMessage` (line 264), and internally-inconsistent frames → `Truncated("header")` (line 243) / `Truncated("channel id")` (line 249). No unit test constructs any of them (grep for the variants finds only the definitions and construction sites). `partial_frame_needs_more` only feeds prefixes, which return `Ok(None)`, never these `Err`s. These fire on a frame whose `body_len` is satisfied but whose inner `channel_len` overruns the body, or whose channel/error bytes are non-UTF8 — the §9-clause-6 clean-refusal path a hostile peer drives. Fuzzing (`envelope_decode`, nightly only) may reach them but asserts no specific mapping, so the clean-refusal contract for these branches is on the honor system.

**Recommendation.** Add unit tests crafting (a) a frame with `channel_len` exceeding the body → Truncated('channel id'), (b) a data frame with non-UTF8 channel bytes → BadChannelId, (c) an error frame with non-UTF8 payload → BadErrorMessage, asserting the exact variant.

#### 🔵 LOW — Wire-hello announcement-parse error paths (truncated length/identity, bad UTF-8) are untested

`codec-api/src/lib.rs:448`  ·  _OPT-TEST-4_  ·  verifier: **CONFIRMED**

`try_decode_hello`'s announcement loop (lines 448-461) has three hostile-input branches with no coverage: a `count` larger than the announcements present → `WireError::Truncated("announcement length")` / `Truncated("announcement identity")`, and a non-UTF8 announced channel name → `WireError::BadChannelId`. The existing hello tests cover bad magic, version mismatch, oversize, and partial prefixes, but never a well-framed hello whose declared announcement count or channel bytes are malformed — the precise case a hostile peer in `nexus-sim wire` would send to a `faces=host` leg. The clean-refusal-with-reason contract (leg surfaces the WireError) is unverified for these variants.

**Recommendation.** Add tests crafting a hello with `count=2` but only one announcement (→ Truncated) and one with a non-UTF8 announcement (→ BadChannelId).

#### 🔵 LOW — Encoder-side oversize refusal (`encode`/`encode_hello` -> FrameTooLarge) is never tested

`codec-api/src/lib.rs:198`  ·  _OPT-TEST-5_  ·  verifier: **CONFIRMED**

`encode` (line 198) and `encode_hello` (line 378) both promise, in their doc comments, that they return `FrameTooLarge` rather than emit a frame the decoder would reject — a bound the leg/exec fragmentation logic relies on to know an over-large chunk must be split, not handed to `encode`. Only the *decode* side of oversize is tested (`oversize_frame_is_refused` line 563, `hello_oversize_is_refused_before_buffering` line 712). No test calls `encode` with a >MAX_FRAME_SIZE data payload / oversize channel id, nor `encode_hello` with announcements exceeding the bound, so the encoder's own guard is unverified; a regression that emitted the oversize frame instead of erroring would pass all current tests.

**Recommendation.** Add a test asserting `encode` of a data event whose body exceeds MAX_FRAME_SIZE returns `Err(FrameTooLarge)` and appends nothing, and likewise `encode_hello` with an over-budget announcement set.

#### 🔵 LOW — Lock invariant property test excludes Held / free-for-all origins and purge, so it cannot exercise the reclaim class that already produced an audit bug

`nexus-core/src/lock.rs:740`  ·  _OPT-TEST-6_  ·  verifier: **CONFIRMED**

`prop_exclusive_invariants` (line 740) registers all four origins as `WriteMode::OnDemand` (line 744) and its `Op` set (line 712) has no `ReclaimHeld` and no free-for-all model. The single worst arbitration bug in the project's history — the phase-5 'held re-acquire was FIFO, a non-held waiter could inherit the mux lock and corrupt framing' (implementation-notes §6c fix 2) — lives exactly in the Held/`reclaim_held` interaction with the FIFO queue and steals, and is only covered by one hand-written scenario (`held_origin_reclaims_a_free_lock_ahead_of_on_demand_waiters`). Because the randomized interleaving never mixes a Held origin with steal/enqueue/reclaim, the property that a non-held waiter can never end up holding the mux lock is not fuzzed across schedules.

**Recommendation.** Add a Held origin (id 0) plus a `ReclaimHeld` op to the strategy and assert the extra invariant 'if a Held origin is registered and the lock is free, no on-demand origin may hold it'; optionally a second property over `Arbitration::FreeForAll`.

## 3. Documentation

#### 🟡 MEDIUM — README's headline pointer to the normative design doc is a broken link to a superseded version

`README.md:161`  ·  _DOC-1_  ·  verifier: **CONFIRMED**

The README's 'Documentation' section leads with:

    - [`docs/13-design-claude-fable-v6.md`](docs/13-design-claude-fable-v6.md) — the normative design document (concepts, node types, RPC contract, and the reasoning behind each decision).

This link is doubly wrong. (1) The path 404s: `docs/13-design-claude-fable-v6.md` does not exist under `docs/`; the file was moved to `docs/historical/13-design-claude-fable-v6.md`. (2) Even the moved file is a SUPERSEDED design (v6). The actual normative design is `docs/17-design-claude-fable-v8.md` (98 KB, dated 2026-07-22, the doc every source comment cites by §number and the one the rpc/README calls 'the stable contract'). So the README points a reader who wants 'the normative design document' at a dead path that, if followed to historical/, would hand them the wrong, obsolete spec. This is the single broken doc link in the README — every other link in the section (nexus-doctor.md, security.md, macos.md, codec-authors.md, rpc/, packaging/) resolves.

**Recommendation.** Change the link target and text to `docs/17-design-claude-fable-v8.md`. Optionally reference §-numbers rather than a filename to avoid recurring drift on the next design revision.

#### 🔵 LOW — restart_count conflates spawn failures with crash restarts and is misreported on the spawn-failure path

`nexus-daemon/src/nodes/exec.rs:302`  ·  _CODEXEC-2_  ·  verifier: **CONFIRMED**

`restart_count` is documented as "Times the child has been (re)started after a crash — observable state (§7.6)". On the spawn-failure path (lines 293-304) the child never started, yet the code still does `a.restart_count.set(a.restart_count.get() + 1)` after backing off, so a child that can never spawn (bad argv / ENOENT) inflates a counter labelled as successful restarts. The spawn-failure Faulted reason (`format!("spawn {:?}: {e}", a.argv[0])`) also omits the count, unlike the ChildDied path which embeds `count {}` (line 331-334), so the observable-state presentation is inconsistent between the two fault kinds.

**Recommendation.** Either keep a single 'restart attempts' counter and update the docstring to say so, or split spawn-failures from post-run crashes; make the spawn-failure Faulted reason and the ChildDied reason consistent about whether the count is shown.

#### 🔵 LOW — README advertises version 0.1.0 but the workspace is at 0.2.0 (phase 8 shipped)

`README.md:29`  ·  _DOC-2_  ·  verifier: **CONFIRMED**

README.md:29 states '> **Maturity:** 0.1.0, pre-1.0. Lab-usable on Linux.' and README.md:47 marks existing-terminal as '(design-specified, §7.7; not yet implemented in 0.1.0)'. But every crate is versioned 0.2.0 (nexus-daemon/Cargo.toml:3, codec-api/Cargo.toml:3, nexus-core/Cargo.toml:3, serialnexusd/Cargo.toml:3, serialnexusctl/Cargo.toml:3 all `version = "0.2.0"`), and phase 8 has landed (git HEAD 'v8 alignment: the out-of-tree codec extension track'). The plan (18-...:205) ties 0.2.0 to end-of-phase-8. The daemon's own `info` verb reports `crate::VERSION` = 0.2.0, and docs/rpc/observation.md's info example correctly shows `"daemon_version": "0.2.0"`. So the README's version banner is stale relative to both the code and the sibling RPC doc.

**Recommendation.** Update the maturity banner to 0.2.0 (and the '(not yet implemented in 0.1.0)' aside to 0.2.0), or phrase the maturity note without a hard-coded version so it does not drift each release.

#### ⚪ NIT — Documented buffer bound (MAX_FRAME_SIZE) understates the true retention (MAX_FRAME_SIZE + 3)

`codecs/reference/src/lib.rs:27`  ·  _REFCODEC-2_  ·  verifier: **CONFIRMED**

The struct doc (line 27) and the crate §5 claim say the accumulation buffer is 'bounded by MAX_FRAME_SIZE'. Actually the codec retains a partial frame whenever try_decode returns Ok(None), i.e. when body_len <= MAX_FRAME_SIZE and buf.len() < 4 + body_len; the maximum retained is 4 + MAX_FRAME_SIZE - 1 = MAX_FRAME_SIZE + 3 bytes (the 4-byte length prefix is not part of body_len). Memory is still bounded, so this is not a §5 violation — but the stated constant is off by the header. Note the shared kit's test_support::assert_buffer_bounded asserts `held <= MAX_FRAME_SIZE` and passes for the reference codec only because it feeds oversize 0xFF prefixes that drain the buffer below 4 bytes; a genuine near-max valid partial frame (body_len = MAX_FRAME_SIZE, 4+MAX-1 bytes buffered) would report MAX_FRAME_SIZE+3 and trip that assertion.

**Recommendation.** State the bound as MAX_FRAME_SIZE + 4 (length prefix) in the doc comment, and relax the kit assertion to `<= MAX_FRAME_SIZE + 4` (test_support.rs) so it stays valid for codecs fed a legitimately near-max partial frame.

#### ⚪ NIT — Stale module header: claims 'Phase 2 has no interior nodes' and 'Counters land in phase 3' in a runtime that now hosts codec/exec/leg interior nodes and live counters

`nexus-daemon/src/runtime.rs:16`  ·  _RUNTIME-2_  ·  verifier: **CONFIRMED**

The module doc comment still narrates a phase-2 snapshot: line 9 'Counters land in phase 3', lines 16-18 'the interior holdover ... is exercised when codec (interior) nodes arrive in phase 5. Phase 2 has no interior nodes, so the two boundaries connect directly through these channels.' The crate now ships codec, exec, and leg interior nodes and the DropCounters defined right below, so the header materially misdescribes the current runtime and could mislead a maintainer about which topologies Wiring supports. Cosmetic only — behavior is correct.

**Recommendation.** Refresh the header to describe the current endpoint-keyed wiring (host/target endpoints, interior codec/exec/leg) rather than the phase-2 serial↔PTY-only snapshot.

#### ⚪ NIT — Module docs link to a nonexistent `race2` item (broken intra-doc link)

`nexus-daemon/src/boundary.rs:18`  ·  _BCELL-3_  ·  verifier: **CONFIRMED**

The module doc comment references `[`race2`]` as a primitive, but no `race2` is defined anywhere in the crate (grep across the tree finds only this single line). Only `race3` exists — the two-direction serial node hand-rolls its own biased select in `active_step` rather than using a `race2`, so `race2` was never extracted. As written this is a broken rustdoc intra-doc link (`[`race2`]` resolves to nothing). It only warns rather than failing the build because no `#![deny(rustdoc::broken_intra_doc_links)]` is set in the crate, but it is a stale doc reference that misleads readers into thinking a `race2` primitive exists.

**Recommendation.** Drop the `[`race2`]` reference (make it `[`race3`]` alone), or if a two-arm variant is genuinely wanted, add `race2`. At minimum unlink it so rustdoc stops warning.

#### ⚪ NIT — codec-api's public `Event` fields and constructors carry no rustdoc

`codec-api/src/lib.rs:94`  ·  _DOC-3_  ·  verifier: **CONFIRMED**

`codec-api` is one of the two semver'd public extension contracts (design §15.26), consumed by out-of-tree codec authors. Its `pub struct Event { pub channel: ChannelId, pub kind: EventKind }` (lib.rs:94-97) documents the struct but not its two public fields, and the four public constructors `Event::data/open/close/error` (lib.rs:100-123) have no doc comments at all. The crate has no `#![warn(missing_docs)]` (only `#![forbid(unsafe_code)]` at lib.rs:1), so these gaps are not caught by the build. This is purely cosmetic — the fields are self-descriptive and codec-authors.md explains the vocabulary — but it is an incomplete-rustdoc gap on the public codec API the review asks about.

**Recommendation.** Add one-line doc comments to `Event.channel`, `Event.kind`, and the four constructor methods; consider `#![warn(missing_docs)]` on codec-api to keep the author-facing surface documented going forward.

## 4. Design deviations — classification

Per the review request, undocumented deviations from the normative design are split into **should-fix** (breaks a design guarantee — reported below and carried in §1) and **justified** (a sound refinement that merely lacked a written record — added to `docs/implementation-notes.md §3`).

### Should-fix deviations (reported)

- **Bare all-slash path input ("/", "//") is accepted and captured as raw:/ bound to the dev-root directory instead of being rejected as Malformed** — `nexus-core/src/resolver.rs:222` (§11 (structural atomicity: "resolver-input well-formedness" validated up front) — the audit closed the identical hole for the `raw:` form (§6e fix #5) but not for the bare-path form). See §1.
- **Held-lock reclaim loses a scheduling race to a queued on-demand waiter on release-path wakes** — `nexus-daemon/src/daemon.rs:795` (§6 (line 102), §15.23). See §1.
- **RunOptions::default() yields an empty dev_root that breaks device resolution for embedders** — `nexus-daemon/src/lib.rs:81` (§12/§15.26). See §1.
- **purge-on-reconnect leaves an in-flight backpressured chunk (and re-read kernel backlog) able to fire into the just-reconnected device** — `nexus-daemon/src/nodes/serial.rs:395` (§7.1 / §6 purge-on-reconnect (the one sanctioned targetward drain)). See §1.

### Justified deviations (recorded in implementation notes)

#### 🔵 LOW — resolve_usb_identity validates only field COUNT, so usb identities with empty vid/pid/serial/iface (e.g. "usb::::") are accepted and stored

`nexus-core/src/resolver.rs:163`  ·  _RESOLV-3_ · deviation: **justified**  ·  verifier: **CONFIRMED**

resolve_usb_identity only checks `rest.split(':').count() != 4` (line 163). Input `usb::::` gives rest `:::` with count 4 and is accepted; likewise `usb::0000:S:00` (empty vid) or `usb:0403:6001::00` (empty serial). The empty-field identity is echoed and persisted as a canonical `device` string (daemon.rs:394). It is harmless at runtime because no real sysfs device reports empty idVendor/idProduct so `find_usb` never matches (a stored `usb:...::...` never resolves), but it is an under-enforcement of the §11 well-formedness guarantee: a structurally meaningless identity should be rejected at add time rather than silently stored and dumped. (The empty-serial variant is the reachable-via-capture hazard tracked in RESOLV-1; this finding is about accepting such strings as *identity-form input*.)

**Recommendation.** After the count check, reject if any of the four fields is empty (and optionally validate vid/pid as 4-hex-digit), returning `Malformed`. Low priority; primarily hardens §11 and prevents storing junk identities.

#### 🔵 LOW — Connect-role legs never populate `peer_address` state

`nexus-daemon/src/nodes/leg.rs:471`  ·  _LEG-2_ · deviation: **justified**  ·  verifier: **CONFIRMED**

§7.4 lists `peer address` as leg state. Only the listen role produces a `peer_addr` (from `accept()`); `connect_stream` returns the stream with no address, so `established` maps to `(s, None)` and the `if let Some(addr) = peer_addr` guard never fires for a connect-role leg. As a result a connect-role leg reports `peer_address: null` in `state` even while fully connected, whereas the dialed address is known (`a.address`).

**Recommendation.** For the connect role, set `peer_address` to `a.address` (the dialed endpoint) on a successful handshake so the state field reflects the live peer.

#### ⚪ NIT — write_mode on a log-target edge validates and round-trips but is silently overridden to `never` by the runtime

`nexus-core/src/config.rs:145`  ·  _GRAPH-1_ · deviation: **justified**  ·  verifier: **CONFIRMED**

`EdgeConfig::write_mode` defaults to `OnDemand` and is neither normalized nor validated for edges whose target is an inherently read-only node. A config edge such as `a="usb0" b="mylog" write_mode="held"` (serial host -> log target) passes `GraphConfig::validate()` unchanged and `dump` re-emits it verbatim, yet `Wiring::build` unconditionally forces the mode to `WriteMode::Never` for a log target (nexus-daemon/src/runtime.rs:316-320). The effective behavior is always correct (the log origin registers as `Never`, gets no targetward path and no lock handle, so there is no wedge), but the persisted/dumped config misrepresents runtime behavior: an operator reading a dumped config sees `write_mode="held"` on a read-only log edge. Purely cosmetic — no invariant is broken and the round-trip is preserved.

**Recommendation.** Optionally normalize (or reject) a non-`never` `write_mode` on edges whose target node is a log at config-validation time, so the persisted config matches the runtime's forced `never`. Alternatively document that write_mode is meaningless on log edges.

## 5. Simplification & clarity

#### 🔵 LOW — `ensure_holds` is duplicated byte-for-byte in codec.rs and exec.rs

`nexus-daemon/src/nodes/exec.rs:514`  ·  _OPSIMP-1_  ·  verifier: **CONFIRMED**

The async held-lock re-acquire loop `ensure_holds(lock: &SharedLock, id: OriginId) -> bool` exists twice, identically. codec.rs:345-379 and exec.rs:514-545 differ only in a trailing comment and one blank line (verified by diffing with comments stripped). Both do the same fast-path `may_write` check, then the same lost-wakeup-free `notified()/enable()` loop calling `reclaim_held`, emitting `emit_change` on a fresh reclaim, and returning false when the lock is closed. The doc-comments even cross-reference each other (exec's says "Mirrors the in-process codec's gate"). Any future fix to the held-reclaim protocol (as in the §6b audit that added `reclaim_held`) has to be applied in two places or they silently diverge. Note leg.rs:803 `ensure_acquired` is deliberately different (on-demand `acquire`+`enqueue`, not held `reclaim_held`), so it should stay separate.

**Recommendation.** Extract one shared async helper — e.g. a free `pub(crate) async fn reacquire_held(lock: &SharedLock, id: OriginId) -> bool` in runtime.rs beside `LockCell`, or a method on `LockCell` — and call it from both codec::channel_targetward and exec::route_event. No behavior change; removes ~34 duplicated lines and a divergence hazard.

#### 🔵 LOW — The oversize-chunk frame-fragmentation loop is duplicated in leg.rs and exec.rs

`nexus-daemon/src/nodes/leg.rs:591`  ·  _OPSIMP-2_  ·  verifier: **CONFIRMED**

The §15.24 "fragment a chunk into consecutive envelope Data frames rather than drop" idiom is written out twice with the same subtle constants. leg.rs:593-618 and exec.rs:387-405 both compute `let cap = MAX_FRAME_SIZE.saturating_sub(3 + <channel>.len()).max(1);` (the same magic `3` = 1 type byte + 2 channel-length bytes), then loop `off` in `cap`-sized steps, `encode(&Event::data(channel, bytes.slice(off..end)), &mut frame)`, break defensively on encode error, write the frame, advance `off = end`. The header-size constant `3` and the `.max(1)` guard are exactly the sort of detail that rots when duplicated (and this idiom already had a real bug fixed once — impl-notes §219). Only per-piece bookkeeping differs (leg increments a byte counter per piece; exec does not).

**Recommendation.** Extract a shared frame-splitter — e.g. `fn each_data_frame(channel: &str, bytes: &Chunk, mut f: impl FnMut(&[u8]))` (or an iterator yielding `(piece_len, frame)`) in a small daemon util or in codec_api. leg's stat increment and exec's plain write become the per-piece closure body. Centralizes the `MAX_FRAME_SIZE - (3 + channel.len())` payload-cap math in one place.

#### 🔵 LOW — State keeps display-string keys that are converted to/from EndpointAddr repeatedly

`nexus-daemon/src/daemon.rs:347`  ·  _OPSIMP-3_  ·  verifier: **CONFIRMED**

`Wiring` keys its maps by `EndpointAddr` (runtime.rs:224-245), but `State` re-keys the same three maps by the display `String` (daemon.rs:73-80). This forces a stream of `.to_string()` conversions when absorbing the wiring (daemon.rs:350,355,360,460,463), a `EndpointAddr::new(&name, ep.name.clone()).to_string()` rebuild in remove-node (daemon.rs:527), and a `display.parse().expect("address is infallible")` round-trip on every key in the `state` verb (daemon.rs:604) purely to recover `addr.node` and `addr.is_default()`. Since `EndpointAddr: FromStr` is infallible, keying `State`'s `endpoint_locks`/`endpoint_targetward`/`origin_locks` by `EndpointAddr` and parsing the user-supplied RPC string once at the lookup sites (e.g. daemon.rs:810 `st.endpoint_locks.get(&p.endpoint.parse().unwrap())`) would remove all the per-entry conversions and the whole `state`-loop reparse.

**Recommendation.** Change the three State maps to `HashMap<EndpointAddr, ...>`, drop the `.to_string()` collect/insert conversions in load/add-node, build the remove-node endpoint list as `EndpointAddr`s directly from the node shape, and iterate `EndpointAddr` keys in `state` without parsing. Convert the (few) RPC-string entry points with a single infallible `.parse()`.

#### ⚪ NIT — Dead "load-replace" arm in is_config_mutation

`nexus-daemon/src/daemon.rs:1253`  ·  _daemon-arbitration-2_  ·  verifier: **CONFIRMED** _(also independently reported as DLC-2)_

`is_config_mutation` matches the method string `"load-replace"`, but `dispatch` (line 189) never routes a `"load-replace"` method — a `--replace` load is dispatched as method `"load"` with a `replace` boolean param (lines 191-199). So the `"load-replace"` arm is unreachable dead code. It is harmless (the real `"load"` arm already triggers the snapshot for both plain and replace loads), but it can mislead a reader into thinking a distinct verb exists.

**Recommendation.** Drop the `"load-replace"` literal from the match in `is_config_mutation` (line 1253); the `"load"` arm already covers replace loads since dispatch normalizes both to method `"load"`.

#### ⚪ NIT — Repeated "find node by name or fail" lookup idiom in daemon RPC handlers

`nexus-daemon/src/daemon.rs:659`  ·  _OPSIMP-4_  ·  verifier: **CONFIRMED**

The pattern `st.nodes.iter().find(|n| n.name() == X).ok_or_else(|| RpcError::invalid_params(format!("unknown node {X:?}")))?` is written out in `rotate` (659-661), `serial_port` (676-678), and again as `.position(...).ok_or_else(...)` with the identical message in `remove_node` (496-500). The origin lookup at 958-959 is a near-twin. The message text and error code are copy-pasted, so a wording/code change touches several sites.

**Recommendation.** Add a private helper on the daemon state, e.g. `fn node<'a>(st: &'a State, name: &str) -> Result<&'a Node, RpcError>` (and a `position` variant for remove), returning the shared `unknown node {name:?}` error. Collapses three+ call sites.

#### ⚪ NIT — Absorbing a freshly-built Wiring into State is duplicated between load and add-node

`nexus-daemon/src/daemon.rs:459`  ·  _OPSIMP-5_  ·  verifier: **CONFIRMED**

`load` (347-361) and `add_node` (459-464) both take a just-built `Wiring` and fold its `endpoint_locks` and `host_targetward_tx` (and, in load, `origin_locks`) into the display-keyed State maps, using the same `.to_string()` key conversion. load replaces via `= …collect()`; add-node merges via `insert`, and omits origin_locks only because a single node has no edges. The two spellings of the same absorb make it easy to update one and miss the other (e.g. if origin_locks ever became relevant to add-node).

**Recommendation.** Add a `State::absorb_wiring(&mut self, wiring: &Wiring)` (or a free helper) that inserts all three map families uniformly; call it from both load (after clearing) and add-node. If OPSIMP-3 is taken, this helper becomes a trivial `extend`.

#### ⚪ NIT — state_extra channel loops treat a guaranteed-present stat as optional

`nexus-daemon/src/nodes/codec.rs:195`  ·  _OPSIMP-6_  ·  verifier: **CONFIRMED**

In codec.rs:195-208, exec.rs:225-237, and leg.rs:291-308 the state_extra builders iterate `self.channels`, do `let stat = self.stats.get(ch)` and then guard every field with `stat.map_or(0, |s| s.field.get())` / `stat.is_some_and(|s| s.field.get())`. But `self.stats` is constructed in each node's `create` from exactly `self.channels` (e.g. codec.rs:95-98), so the lookup is always `Some`; the Option handling is dead defensiveness that obscures the intent and repeats `map_or(0, …)` a dozen times. Iterating `self.stats` (or `&**self.stats`) directly yields `&Rc<ChannelStat>` with no Option, letting each field read plainly as `s.delivered_hostward.get()`.

**Recommendation.** Iterate the stats map directly (or index `self.stats[ch]`) and drop the `map_or`/`is_some_and` guards. leg.rs still layers unbound identities in afterward, which is unaffected. Also note the identical `stats: Rc<HashMap<String, Rc<ChannelStat>>>` field type and its `channels.iter().map(|c| (c.clone(), Rc::new(ChannelStat::default()))).collect()` builder appear in all three nodes and could share a small type alias / constructor helper.

#### ⚪ NIT — CLI read-config-from-TOML-file is duplicated for Load and AddNode

`serialnexusctl/src/main.rs:171`  ·  _OPSIMP-7_  ·  verifier: **CONFIRMED**

`build_request` reads and parses a GraphConfig TOML file twice with the same code and error mapping: `Cmd::Load` (172-174) and `Cmd::AddNode` (182-184) each `std::fs::read_to_string(file)?` then `toml::from_str(&text).map_err(|e| anyhow::anyhow!("parsing {}: {e}", file.display()))?`.

**Recommendation.** Extract `fn read_config(file: &Path) -> anyhow::Result<GraphConfig>` and call it from both arms.

## 6. Verified and cleared

For transparency: 16 candidate findings were **refuted** on verification (the code observation was often accurate, but the harmful scenario was unreachable or the behavior was intended), and 1 was **already documented**. These are recorded so the team does not re-investigate them.

**Refuted** (candidate → why it is not a defect):

- `nexus-core/src/graph.rs:325` — DuplicateEndpoint Display message names a "default endpoint" that a leg does not have  
  ↳ The quoted behavior exists: graph.rs:324-336 emits, for an empty `DuplicateEndpoint`, the string "node ... declares an empty channel identity, which is reserved for a default endpoint and forbidden as a real channel (§3)", and config.rs:92-97 raises this same….
- `/home/pwnall/workspace/serial-nexus/nexus-core/src/data.rs:177` — held_bytes() reports 0 for a parked empty chunk while is_parked() is true, so the prop test's bounded-interior proxy cannot see an empty holdover  
  ↳ The literal code observation is correct: Holdover::held_bytes() (nexus-core/src/data.rs:177-179) returns 0 for a parked zero-length chunk while is_parked() (173-175) returns true, and prop_targetward_no_loss_bounded_interior uses held_bytes() (line 526) as….
- `nexus-core/src/resolver.rs:383` — find_usb returns the /dev path without an existence check, unlike the by-path and raw branches, so resolve_current_path can return Some(non-existent-path)  
  ↳ The literal code observation is correct: find_usb (nexus-core/src/resolver.rs:383) returns Some((dev_root/dev/dev_name, info)) with no .exists() gate, unlike bypath_lookup (L396), the raw: branch (L319), and the bare /-path branch (L322), which all gate on….
- `codecs/reference/src/lib.rs:76` — framing_errors over-counts on a mangled-length-prefix run (one per 4-byte drop, not one per frame)  
  ↳ The raw mechanics are real and reachable exactly as quoted: try_decode (codec-api/src/lib.rs:234) returns Err(FrameTooLarge) for body_len > MAX_FRAME_SIZE; resync() (codecs/reference/src/lib.rs:75-79) then drains 4 bytes and does framing_errors += 1; the….
- `nexus-rpc/src/lib.rs:85` — Wire types accept unknown/extra fields (no deny_unknown_fields)  
  ↳ The cited code is accurate: nexus-rpc/src/lib.rs:85-92 shows `struct Request` deriving `Deserialize` with no `#[serde(deny_unknown_fields)]`, and `params` uses `#[serde(default, skip_serializing_if = "Option::is_none")]` so present-null and absent both….
- `nexus-daemon/src/nodes/serial.rs:373` — In-flight targetward chunk dropped uncounted on write failure  
  ↳ The code behavior is exactly as described — serial.rs:371-380 dequeues a targetward chunk via `rx.recv()`, passes it to `runtime::write_all`, and on `Err` returns `Step::Lost`, dropping that in-flight chunk without a counter; `write_all` (runtime.rs:388-412)….
- `nexus-daemon/src/nodes/pty.rs:520` — Reader task exits on targetward-send error, abandoning presence detection and lock lifecycle  
  ↳ The quoted code is real: pty.rs:520-522 does `if tx.send(payload).await.is_err() { return; }`, and that early return abandons the loop that owns `present.swap` and `handle_last_close` (lines 557-568).
- `nexus-daemon/src/nodes/pty.rs:313` — state_extra reports a non-standard advertised_baud that was never applied  
  ↳ Code facts verified: pty.rs:382-384 skips cfsetspeed when standard_baud() is None; pty.rs:313 reports self.advertised_baud unconditionally; config.rs:213-214 is an unvalidated u32, so a non-standard advertised_baud (e.g.
- `nexus-daemon/src/nodes/log.rs:262` — Queue-overflow fault does not stop the writer; a Faulted log node keeps consuming and appending, and the fault status is sticky/misleading  
  ↳ The code is exactly as quoted (log.rs:262-269 vs 300-307), but the finding mischaracterizes a coherent, intended policy distinction as a defect, and its recommended fix would introduce a real regression.
- `nexus-daemon/src/nodes/log.rs:220` — teardown blocks the single-threaded runtime up to FLUSH_WAIT (2s), freezing the whole daemon when a wedged log node is removed  
  ↳ The low-level mechanics are accurate: teardown() (log.rs:208-233) calls done.recv_timeout(FLUSH_WAIT) with FLUSH_WAIT=2s (log.rs:43,221), teardown is invoked synchronously from RPC dispatch on the single current-thread runtime (daemon.rs:534 remove-node →….
- `nexus-daemon/src/nodes/exec.rs:445` — One in-flight merged-source chunk is lost on child crash despite the 'source persists across restarts' claim  
  ↳ The code mechanism is real: `feed` (exec.rs:385-408) does `src_rx.recv().await`, which removes a chunk from the merged queue; when `race3` (exec.rs:445) completes on the `read` arm returning `PumpEnd::ChildDied` (exec.rs:415/424), `feed` is dropped and the….
- `nexus-daemon/src/nodes/leg.rs:603` — Fragmentation loop `break`s on encode error, silently dropping the remainder uncounted  
  ↳ The quoted code is real (leg.rs:600-604: `break` on encode error abandons bytes[off..total] uncounted), but the path is provably unreachable, so no no-drop invariant is violated.
- `nexus-sys/src/lib.rs:163` — read_fd/write_fd never retry EINTR; every caller maps a generic Err to fatal device-loss/hangup  
  ↳ The quoted code is accurate: read_fd (nexus-sys/src/lib.rs:163) and write_fd (:176) do a single libc::read/write, translate any negative return via last_os_error(), and callers treat a non-WouldBlock Err as terminal loss (serial.rs:513, pty.rs:545,….
- `nexus-sys/src/lib.rs:198` — poll_ready/poll_blocking discard the poll(2) return value, collapsing hard errors into 'no events'  
  ↳ The quoted code is real and accurately described: poll_ready (lib.rs:198) and poll_blocking (lib.rs:219) both do `let _ = poll(...)` and then return `fds[0].revents().unwrap_or_else(PollFlags::empty)`, so an Err from poll(2) is collapsed into an empty….
- `nexus-sys/src/lib.rs:87` — Non-Linux ptsname unsafe path relies on an unenforced 'single-threaded callers' invariant  
  ↳ The code exists as quoted (nexus-sys/src/lib.rs:83-88), but no reachable corruption scenario exists, and the "single-threaded" precondition is in fact upheld by every caller — not merely assumed.
- `fuzz/Cargo.toml:26` — No fuzz target over nexus-rpc::parse_incoming_request, the other untrusted line parser  
  ↳ The factual observation is accurate — fuzz/Cargo.toml (lines 27-53) declares only envelope_decode, frame_decoder, wire_hello, reference_demux; the crate has no nexus-rpc dependency, and parse_incoming_request has only unit tests (nexus-rpc/src/lib.rs:461-491).

**Already documented:**

- `nexus-daemon/src/nodes/serial.rs:519` — Reader thread ignores POLLERR/POLLNVAL: possible busy-loop and masked device loss (see implementation-notes / phase-audit logs).
