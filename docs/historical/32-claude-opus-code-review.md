# serial_nexus — Comprehensive Code Review

**Reviewer:** Claude Opus 5 (16 lenses across 25 independent finder runs, 99 adversarial verifiers,
plus reviewer-run live reproductions on the dev box)
**Date:** 2026-07-27
**Scope:** the full workspace at `cfb2187` on `implementation` — daemon, CLI, core, `codec-api`, the
reference codec, the web console and its browser assets, doctor, sim, the `nexus-itest` harness, the
`fuzz/` targets, the Playwright suite and CI — against the normative design
`docs/30-design-claude-fable-v13.md`, the plan `docs/31-implementation-plan-claude-fable-v13.md`, and
the deviations already recorded in `docs/implementation-notes.md`.
**Focus (as requested):** correctness, reliability, design deviations, and opportunities in testing,
documentation, and clarity.

> **Verification status: complete.** 99 candidate findings were each handed to an independent skeptic
> that had **not** seen this report — only the finding and the tree — and was told to refute it.
> **87 survived** (7 high, 27 medium, 44 low, 9 nit; 80 unique after merging the pairs that two
> finders reported independently), **10 were refuted**, and **2 were already-known** dispositions.
> Every verdict is recorded; §6 tabulates what was cleared so it is not re-investigated.
>
> **The tree was frozen for the whole verification pass.** Design §15.34's second clause — added after
> the v12 audit returned 35 of 43 verdicts as "not real" for defects that were real when filed — says
> the tree must not move under the verifier. No file in the repository was modified between the first
> finder starting and the last verdict landing; `git status` showed only the pre-existing untracked
> `.claude/` throughout.

---

## 0. Baseline

Established on the dev box (Linux 7.0.0, 8 cores, load < 0.6, no CPU hogs) at `cfb2187`, before any
finding below was acted on — nothing in the repository was modified during this review:

```
cargo build --workspace --locked      → ok
cargo test  --workspace --locked      → 485 passed / 0 failed / 4 ignored   (exit 0)
git status                            → only the pre-existing untracked .claude/
```

That is exactly the figure `docs/implementation-notes.md` and AGENTS.md §2 claim, so the suite's own
bookkeeping is accurate. **Every defect below is present in a tree whose entire test suite is green**,
which is the point worth carrying: the findings cluster precisely where the suite does not look.

---

## Methodology, and why the numbers are what they are

Sixteen finders each took one subsystem or one lens (`nexus-core` graph/config, the pure state
machines, the daemon's verbs, the control transport and runtime, taps and the ring, the boundary
nodes, the interior/transport nodes, the web server and bridge, the browser assets, `codec-api` and
the wire, the client crates, a design-deviation sweep, test coverage, documentation, simplification,
and a cross-cutting concurrency/lifecycle lens). Each was told that quality beats quantity, that a
finding needs a concrete failure scenario rather than a worry, and that re-filing anything from
review 26's refutation list, the remediation ledger's declines, or notes §3.1–§3.19 is itself a
defect.

Then every candidate went to a verifier whose instructions were to **kill** it, on three axes: the
facts are wrong; the facts are right and the consequence is wrong; or it is already known. Verifiers
were also told that a failure to reproduce is not a refutation — other agents shared the box, and
this project has already lost a round of investigation to load-sensitive results (AGENTS.md §8).

That verification did real work, in both directions:

- **It killed ten findings**, including two of mine. `ITEST-3` (an intermittent failure in the new
  `web-ui` CI gate) died after the verifier ran the gate **100 consecutive times**, 30 of them
  CPU-constrained at loads worse than the finder's. `CORE-1` and `SIMP-4` died because the proposed
  fixes would have regressed behaviour the design requires — `SIMP-4`'s verifier settled it by adding
  a hypothetical config variant to a *copy* of the tree and watching the compiler produce four `E0004`s
  in `config.rs`, which is exactly the enforcement the finder said was missing.
- **It corrected many that survived.** `RV-8` was filed critical and corrected to high, because the
  permanent brick is specific to pty-backed devices while a real UART self-heals at last close — and
  the same verifier found a *second* leak site the finder had missed. `RES-1`'s verifier confirmed the
  claim and then made it worse: adding both clones produces two nodes carrying the same identity, so
  the second adapter becomes unreachable and reports a bare `Device or resource busy`. `HIST-1`'s
  verifier confirmed the hole and rejected half the finder's framing.

**Several findings were reported twice by finders working independently** — the leg's discarded drop
counters (`LEG-1`/`RT-1`), the >8 MiB replay truncation, the pty symlink alias, the pty reader park,
the OPFS orphan leak, the leg write-half tail loss, and the `app.js` error collapse. Independent
convergence on the same defect from different starting points is the strongest signal in the set, and
those are marked in place.

### What this review did *not* find

Worth stating plainly, because it is the more important half of the result. The **pure state machines
survived intact for the third review running**: no reachable sequence breaks the lock's single-holder,
FIFO-beneath-held, generation-guard or purge-on-acquire properties; every picocom mapping still matches
the upstream `do_map`/`map2hex` oracle; the graph validator's `deny_unknown_fields`, name-legality and
cycle detection all hold. **Invariant 11 (the web bridge) survived a deliberate attack** — fragmented
two-request frames, duplicate `"method"` keys, batches, scalars, binary frames — with every lifecycle
verb refused and the graph intact. **`nexus-sys`'s unsafe is sound.** **The `epoch` machinery this
release added is correct**, and so is the `TIOCEXCL` release for the path §15.38 wrote it for. The
defects below cluster almost entirely in *accounting*, in *client halves*, and in *error paths* — not
in the model.

---

## Executive summary

The system's core is in good shape and the recurring §16 thesis holds again: **defects cluster where a
rule lives in prose instead of in a type, a cap, a parser, or a helper.** Four clusters carry most of
the weight, and each has a single structural remedy rather than N local patches.

**1. The resolver's two §12 directions read disjoint sources — and that is a wrong-device bug.**
Identity *capture* walks sysfs; identity *resolution* and the ambiguity guard scan only
`/dev/serial/by-id`. Two consequences, both reproduced. The duplicate-serial guard counts by-id
*links*, but two adapters sharing an identity necessarily collide on **one** udev-generated link name
(`usb-$ID_SERIAL-if$ID_USB_INTERFACE_NUM…`, every component a function of the ambiguous fields), so the
guard can never fire for the exact hazard §12 and §15.25 promise it closes — the node binds the wrong
physical board, silently, with no warning (`RES-1`). And a `usb:` identity captured in any environment
with `/sys` but no by-id tree — a `--device=`-mapped container, an mdev image — can never be resolved
back: `add-node` returns success *with a populated `resolved_path`*, then the node waits forever
(`RES-2`). The in-tree test for the first passes only because its fixture invents two by-id names udev
cannot produce for identical devices. One fix serves both: count ambiguity over sysfs tty devices and
give `find_usb` a sysfs fallback, so both directions read the same source.

**2. Exclusivity is released by exit path, not by ownership — and one ordinary config bricks a device
permanently.** §15.38's D2 fix put `set_exclusive(fd, false)` in `SerialNode::teardown`. There are at
least three other places the node lets go of an exclusive port and none of them release it:
`open_port`'s own error path after it has taken the flag (`RV-8`), `set_waiting`/`fault` (`CONC-4`),
and the reconnect window where the reopened port is a supervisor local across an `.await` (`SERX-1`).
`RV-8` is reachable from §7.1's *documented* initial modem-line assertions pointed at a pty-backed
device — socat `PTY,link=`, QEMU `-serial pty`, the project's own `nexus-sim`, all legal `raw:` devices:
`set_dtr` fails `ENOTTY`, the port is dropped still exclusive, and because `TIOCEXCL` clears only at the
tty's last close — which a held master never reaches — the device becomes un-openable **by every
unprivileged process on the machine**, permanently, surviving `teardown` and daemon exit. The node's
reported reason flips after one reconnect poll from the true cause (`set DTR: Inappropriate ioctl`) to
a self-inflicted `Device or resource busy`, sending the operator hunting for a squatter that is the
daemon. The structural fix is to make the release belong to the *port* — a small owner whose `Drop`
returns the claim — rather than to whichever exit path someone remembered.

**3. A parked waiting verb silently starves its own connection's data stream.** While a waiting verb is
in flight, `serve_connection` drops into an inner two-arm loop that polls only the dispatch future and
the request reader; the `notes.recv()` and `tap_rx.recv()` arms are not polled at all. So the
connection stops delivering every notification it is subscribed to, and beyond the 128-slot tap queue
the bytes are *really lost*, not deferred. Measured by the verifier: **11.2 MB of console output
dropped during one 6-second `lock --wait`**, counted in `state` as that tap's `dropped`. §17 mandates
one daemon connection per browser, so this lands on the shipped console: the verifier reproduced a
**2.001-second full terminal blackout with 3.3 MB dropped** from a single ordinary contended `send` —
and on any graph with a `held` origin (a demux codec, or a map's raw edge, which §7.8 *promotes* to
`held`) every un-stolen keystroke costs that blackout (`CTRLW-1`). Two siblings share the root:
`send`'s `timeout_ms` bounds only the acquire, so a backpressured `send` hangs forever **holding the
exclusive lock** (`CONC-2`), and a pty reader parked on a full targetward channel freezes presence,
last-close, detach-release and the termios baseline reset — leaving an endpoint locked by a client
that closed minutes ago (`CONC-1`, independently reported as `PTY-2`).

**4. §5's "loss is always visible and attributable" has holes at four boundaries.** A `faces = target`
leg *discards* its channels' `DropCounters` at `start`, so everything it sheds at its own intake is
invisible: ~50–62 MB gone with every counter in `state` reading zero, while an identically-fed `log`
node accounted for its loss to the byte (`LEG-1`, found twice). A demux drops every byte on an
unconfigured channel identity with no counter, no state and no log line — a typo'd channel name makes
100% of a device's output vanish from a graph that looks healthy (`CODEC-1`). The leg's write half and
the exec child's stdin feed both lose the untransmitted tail of an in-flight chunk uncounted
(`LEG-2`). And `delivered_hostward` counts bytes that only reached a full sink, inflating it by exactly
`discarded_hostward` (`LEGD-2`).

**5. Three gates do not gate.** For a project whose method is "every phase ends with an adversarial
audit", a check that cannot fail is worse than a missing one, because it is counted as coverage.
`p0_license_gate` — the executable proof behind §13's permissive-only policy — **passes vacuously
whenever `cargo metadata` fails to resolve**, which the finder proved by deleting the ban entry from
`deny.toml` and watching the gate stay green (`TESTR-2`). `p11_replace_atomicity`'s exclusivity guard,
written *this release* so that "a fix that simply stopped taking `TIOCEXCL` cannot pass", asserts only
`pass == false` on a sim verdict that an ordinary echo race produces just as readily — so the guard it
was written to be cannot fail (`ITEST-1`, reported twice). And the `RefCell` meta-gate's exemption
matches on bare file name, so any future `cell.rs` anywhere in a ban crate is silently exempt from
invariant 5 (`TESTR-7`) — the same shape as the `clippy.toml` scope break that review 26 caught.

**6. The documentation drifted in the places a newcomer starts.** `README.md`'s documentation index
links to two files that do not exist and names the superseded v12 design as normative (`DOCR-1`);
`docs/nexus-doctor.md`, which README calls "the probe reference", documents 5 of the binary's 11
probes — P6–P11, this project's kernel-contact instrument, are absent (`DOCR-2`); AGENTS.md's crate
table still says the web console "refuses graph/lifecycle verbs", contradicting invariant 11 in the
same file (`DOCR-4`), and §9 still names v12 as the current pair (`DOCR-5`). None of these is a code
defect; all of them mislead the next session on its first read, which is the audience AGENTS.md exists
for. `docs/rpc/*` by contrast checked out field-by-field against the serde sites that produce it,
including the new `epoch`, `consumer_live`, `purged_bytes` and `gap_before`.

Beyond the clusters, three findings deserve individual attention:

- **`WEB-1` (high).** `--tls` treats "first run" as *both* files existing, so any half-present pair
  takes the generate branch and **overwrites whichever half exists** — including truncating an
  operator's private key. The realistic trigger is not a typo but the ordinary CA workflow: generate
  the key, point the flags at the intended pair before the signed cert is installed, and the key is
  destroyed at startup. The server then comes up green, and the one log line names only the cert.
- **`RV-1` (high).** A pty node that faults *after* `install_symlink` but before `apply_perms` /
  `prime_slave` leaves its symlink on disk pointing at a pts index the kernel immediately recycles. The
  faulted console's published path then resolves to a **different, live console's terminal** — opening
  it flips that console's `client_present` and its keystrokes reach that console's device. Reachable
  from a plain `chown` EPERM: a non-root daemon with the `group =` setting `docs/security.md` itself
  endorses.
- **`HIST-1` (high).** `history.mjs` computes a real offset-space hole into `h.dropped` — and nothing
  in the client ever reads it. A reload of any console that emitted more than its ring while the tab
  was closed silently concatenates across the gap, both on screen and in the exported log. This is the
  *common* path with the 64 KiB default ring, and it is the same class §15.38 just fixed one instance
  of.

### Prioritized action list

| # | Severity | Finding | Location |
|---|----------|---------|----------|
| 1 | 🟠 high | `CONC-1` A pty reader parked on targetward backpressure permanently freezes presence, last-close and detach-release | `nexus-daemon/src/nodes/pty.rs:655` |
| 2 | 🟠 high | `HIST-1` A reload whose ring rolled past the stored frontier splices across a real hole in silence; `history.dropped` is computed and never read | `serialnexusweb/src/assets/app.js:391` |
| 3 | 🟠 high | `RES-1` Duplicate-serial guard counts by-id entries, so it can never fire for two identical adapters — the node binds the wrong physical device | `nexus-core/src/resolver.rs:349` |
| 4 | 🟠 high | `RES-2` A `usb:` identity captured without a `/dev/serial/by-id` tree can never be resolved back — add succeeds, node waits forever | `nexus-core/src/resolver.rs:504` |
| 5 | 🟠 high | `RV-1` A pty node that faults during setup leaves its symlink installed, aliasing another console's live pts | `nexus-daemon/src/nodes/pty.rs:160` |
| 6 | 🟠 high | `RV-8` open_port leaks TIOCEXCL on its own error paths: an ordinary modem-line config permanently bricks a pty-backed device | `nexus-daemon/src/nodes/serial.rs:703` |
| 7 | 🟠 high | `WEB-1` A mistyped or missing --tls-cert silently truncates and overwrites the operator's --tls-key | `serialnexusweb/src/tls.rs:24` |
| 8 | 🟡 medium | `CONC-2` `send`'s timeout bounds only the acquire, so a backpressured send hangs forever holding the endpoint's write lock | `nexus-daemon/src/daemon.rs:1577` |
| 9 | 🟡 medium | `CONC-3` `LogNode::create` panics instead of faulting when the writer thread cannot be spawned, killing the daemon at startup | `nexus-daemon/src/nodes/log.rs:215` |
| 10 | 🟡 medium | `CONC-4` The v13 TIOCEXCL release is applied in one of the three places the serial supervisor drops the port | `nexus-daemon/src/nodes/serial.rs:499` |
| 11 | 🟡 medium | `CTL-1` `serialnexusctl tap` ignores `tap.closed` and hangs forever after teardown/`load --replace` | `serialnexusctl/src/main.rs:734` |
| 12 | 🟡 medium | `CTRL-1` `send-break`/`pulse-dtr` keep the serial port fd and a deferred line-state write alive past `remove-node`/`load --replace`, for an unbounded caller-supplied duration | `nexus-daemon/src/daemon.rs:1382` |
| 13 | 🟡 medium | `CTRLW-1` A parked waiting verb freezes that connection's `tap.data` and `subscribe` notification lanes until it resolves | `nexus-daemon/src/control.rs:300` |
| 14 | 🟡 medium | `DOCR-3` docs/nexus-doctor.md (and AGENTS.md §7) assert 6.18 is "all probes supported, zero deltas" — a confirmation that predates six of the eleven probes and cannot cover them | `docs/nexus-doctor.md:108` |
| 15 | 🟡 medium | `HIST-2` The rendered terminal has no cap: the `<pre>` grows without bound, and shows more than `export` or storage retains | `serialnexusweb/src/assets/app.js:443` |
| 16 | 🟡 medium | `HIST-3` OPFS history records are keyed by the per-boot `instance` nonce and never reclaimed — every daemon restart orphans up to 16 MiB per console, permanently | `serialnexusweb/src/assets/app.js:322` |
| 17 | 🟡 medium | `HISTC-2` `clear` does not cancel the debounced save, so the cleared scrollback is written back to OPFS | `serialnexusweb/src/assets/app.js:466` |
| 18 | 🟡 medium | `ITEST-1` `the_port_is_still_exclusive_while_the_node_holds_it` passes identically without TIOCEXCL — the anti-regression guard cannot fail | `nexus-itest/tests/p11_replace_atomicity.rs:220` |
| 19 | 🟡 medium | `LEG-1` A faces=target leg throws away its channels' DropCounters, so everything it sheds at its intake is invisible in `state` | `nexus-daemon/src/nodes/leg.rs:394` |
| 20 | 🟡 medium | `LEG-2` The leg's write half loses the untransmitted tail of an in-flight chunk, uncounted, when the socket write fails | `nexus-daemon/src/nodes/leg.rs:819` |
| 21 | 🟡 medium | `RES-3` Adding by the canonical `/dev/serial/by-id/...` path degrades to `raw:` with a warning that is the opposite of true | `nexus-core/src/resolver.rs:282` |
| 22 | 🟡 medium | `SIM-2` `nexus-sim client --recv/--drain` verdict cannot distinguish a deadline from byte loss | `nexus-sim/src/main.rs:910` |
| 23 | 🟡 medium | `TAP-1` `tap.open --replay` on a ring larger than 8 MiB delivers the OLDEST 8 MiB and silently discards the newest, breaking the exact-splice guarantee | `nexus-daemon/src/tap.rs:419` |
| 24 | 🟡 medium | `TAP-2` With `replay_ring = 0` the offset space silently skips every byte produced while no tap is open — a client splices across an arbitrary hole with no `gap_before`, no counter and no epoch change | `nexus-daemon/src/tap.rs:161` |
| 25 | 🟡 medium | `TESTR-2` `p0_license_gate` passes vacuously on any `cargo metadata` failure — proven with the ban entry deleted | `nexus-itest/tests/p0_license_gate.rs:73` |
| 26 | 🟡 medium | `WEB-2` write_private's 0600 mode is silently not applied when the key file already exists | `serialnexusweb/src/tls.rs:93` |
| 27 | 🟡 medium | `WEB-3` The session cookie is not port-scoped, so a second web console evicts the first's session | `serialnexusweb/src/server.rs:461` |
| 28 | 🟡 medium | `WEB-4` The bridge never closes the WebSocket when the daemon connection ends, so the console lies about being connected and silently swallows the next send | `serialnexusweb/src/bridge.rs:123` |
| 29 | 🟡 medium | `WEBUI-1` Every `send` refusal is reported to the operator as a lock conflict with a steal offer, and the steal retry's failure is discarded silently | `serialnexusweb/src/assets/app.js:488` |
| 30 | 🟡 medium | `WIRE-1` A `Codec::demux` error is logged and thrown away: no counter, no state, no status change — the never-resync policy §7.5 sanctions has no signal at all | `nexus-daemon/src/nodes/codec.rs:396` |
| 31 | 🟡 medium | `WIRER-3` The leg's oversize-chunk fragmentation has no end-to-end regression guard, though §15.24 names one | `nexus-itest/tests/p6_reference.rs:85` |
| 32 | 🔵 low | `CODEC-1` Demuxed data on an unconfigured channel identity is dropped with no counter, no state entry and no log line | `nexus-daemon/src/nodes/codec.rs:420` |
| 33 | 🔵 low | `CORE-2` `MultiplexedEdgeNotHeld` ignores arbitration while its sibling rule ten lines below exempts free-for-all, so a working codec graph is refused with a message that is false for that case | `nexus-core/src/config.rs:393` |
| 34 | 🔵 low | `CORE-3` The exec codec's `restart_backoff_ms` is the one timer with no structural range check, so a crashed child can be configured never to restart | `nexus-daemon/src/nodes/exec.rs:68` |
| 35 | 🔵 low | `CTL-2` `serialnexusctl tap` discards `gap_before`, so a capture is silently holed | `serialnexusctl/src/main.rs:737` |
| 36 | 🔵 low | `CTRL-2` A parked `lock --wait` whose edge is disconnected (or whose node is removed) is refused with "origin is write=never" — a claim about configuration that is false | `nexus-daemon/src/daemon.rs:1463` |
| 37 | 🔵 low | `DATA-1` `data.rs`'s model→shipped map — the whole justification for keeping the module — names daemon types that no longer exist | `nexus-core/src/data.rs:22` |
| 38 | 🔵 low | `DEVR-2` §17's console rail omits node status: the left rail shows address, lock holder and waiter count, but never the node's active/waiting/faulted state | `serialnexusweb/src/assets/app.js:257` |
| 39 | 🔵 low | `DEVR-3` §7.7's existing-terminal node is written as a shipped node type and is absent from §14's deferred list, but no such node kind exists | `nexus-core/src/config.rs:522` |
| 40 | 🔵 low | `DEVR-5` §17's "taps … close on blur after a grace interval" is unimplemented: a backgrounded console tab keeps its tap open and the daemon keeps feeding it | `serialnexusweb/src/assets/app.js:479` |
| 41 | 🔵 low | `DOCE-1` Design §17 says browser history is *keyed by* the offset-space epoch, contradicting §15.38 and the implementation | `docs/30-design-claude-fable-v13.md:509` |
| 42 | 🔵 low | `DOCR-1` README's documentation index links to two files that do not exist and names the superseded v12 design as normative | `README.md:180` |
| 43 | 🔵 low | `DOCR-2` docs/nexus-doctor.md — the file README calls "the probe reference" — documents 5 of the binary's 11 probes; P6–P11 appear nowhere | `docs/nexus-doctor.md:25` |
| 44 | 🔵 low | `DOCR-4` AGENTS.md's crate table still says the web console "refuses graph/lifecycle verbs", contradicting invariant 11 and the shipped allowlist | `AGENTS.md:157` |
| 45 | 🔵 low | `DOCR-5` AGENTS.md §9 still names v12 as the current design/plan pair | `AGENTS.md:564` |
| 46 | 🔵 low | `DOCR-6` AGENTS.md invariant 10 still calls the `instance`-does-not-rotate problem "the known open issue" and never mentions the `epoch` field that replaced it | `AGENTS.md:424` |
| 47 | 🔵 low | `HIST-4` `historyEpoch` is not adopted atomically with `historyKey`/`history`, so a flush during (or after a failed) `tap.open` stamps the previous console's epoch onto the new console's record | `serialnexusweb/src/assets/app.js:376` |
| 48 | 🔵 low | `HIST-5` The OPFS filename sanitiser collapses `/` to `_`, so two legally-named consoles can share one storage file and overwrite each other's scrollback | `serialnexusweb/src/assets/opfs.mjs:39` |
| 49 | 🔵 low | `ITEST-4` `MIN_SPECS = 8` sits at exactly the device-free spec count, so up to 6 of the 14 browser specs can vanish and the gate stays green | `nexus-itest/tests/p8_web_ui.rs:66` |
| 50 | 🔵 low | `ITEST-5` `NO_SLAVE_PAUSE` — the fix for a measured 74.4%-of-a-core busy spin in the sim doubles — shipped with no guard, though the measuring primitive is already in the tree | `nexus-sim/src/main.rs:536` |
| 51 | 🔵 low | `LEG-3` The listen role's reject-second-peer loop busy-spins the single-threaded runtime on a persistent accept error | `nexus-daemon/src/nodes/leg.rs:877` |
| 52 | 🔵 low | `LEGD-2` `delivered_hostward` counts bytes that were only dropped at a full sink, so it is inflated by exactly `discarded_hostward` | `nexus-daemon/src/nodes/leg.rs:934` |
| 53 | 🔵 low | `LOGQ-1` After the log writer thread returns, the pump keeps accepting bytes into a queue nobody drains; up to 16 MiB of loss is reported as `queued_bytes`, never as `dropped_bytes` | `nexus-daemon/src/nodes/log.rs:449` |
| 54 | 🔵 low | `RV-10` exec is a usable codec name that info does not report and the unknown-codec error does not list | `nexus-daemon/src/registry.rs:40` |
| 55 | 🔵 low | `RV-4` pty mode is not range-checked, and a plausible octal/decimal typo faults the node with an unrelated message | `nexus-core/src/config.rs:584` |
| 56 | 🔵 low | `RV-6` The state file is <stem>.state.toml, not <socket>.state.toml as the design and rustdoc say | `nexus-daemon/src/lib.rs:305` |
| 57 | 🔵 low | `SERX-2` `send-break`/`pulse-dtr` hold the port fd across an unbounded sleep, and drive its lines after teardown has handed the device to a replacement node | `nexus-daemon/src/nodes/serial.rs:636` |
| 58 | 🔵 low | `SIM-1` `nexus-sim exec-conformance` deadlocks with no verdict against a codec that does not drain its stdin | `nexus-sim/src/main.rs:1651` |
| 59 | 🔵 low | `SIMP-1` The per-channel hostward routing block is a verbatim clone between `codec.rs` and `exec.rs` — the `fan_out` extraction stopped one level short | `nexus-daemon/src/nodes/codec.rs:400` |
| 60 | 🔵 low | `SIMP-2` Targetward "charge every non-delivery exit" is hand-written at sixteen sites across five pumps, and has already shipped a missed exit twice | `nexus-daemon/src/nodes/codec.rs:511` |
| 61 | 🔵 low | `SIMP-3` The PTY's blocking writer thread bypasses `boundary::BlockingReader`, so two of the daemon's three blocking threads are still hand-rolled — and `PtyNode::drop` re-derives `signal_stop` instead of calling it | `nexus-daemon/src/nodes/pty.rs:405` |
| 62 | 🔵 low | `SIMPB-1` Edge attachment is implemented twice, while edge detachment was deliberately consolidated into one helper | `nexus-daemon/src/daemon.rs:922` |
| 63 | 🔵 low | `SIMPB-2` The §15.20 lost-wakeup lock-wait discipline is hand-written three times | `nexus-daemon/src/runtime.rs:166` |
| 64 | 🔵 low | `SIMPB-5` `codec::channel_targetward_drain` and half of `drain_unwired_channels` are structurally unreachable, and their doc describes a case a different function handles | `nexus-daemon/src/nodes/codec.rs:465` |
| 65 | 🔵 low | `TAP-4` `tap.open`'s `feed_dropped` baseline double-counts feed loss that the hub has not yet charged as `gap_before` | `nexus-daemon/src/tap.rs:445` |
| 66 | 🔵 low | `TESTR-6` `p8_web_history` — the only CI runner of the browser modules' unit tests, including the epoch predicate — self-skips silently with no `required` escape hatch | `nexus-itest/tests/p8_web_history.rs:27` |
| 67 | 🔵 low | `TESTR-7` The `RefCell` meta-gate's exemption matches on bare file name, so any future `cell.rs` anywhere in a ban crate is silently exempt | `nexus-itest/tests/meta_gates.rs:291` |
| 68 | 🔵 low | `UIR-1` No terminal renderer and no ANSI handling: escape sequences reach the DOM as literal text | `serialnexusweb/src/assets/app.js:441` |
| 69 | 🔵 low | `WEB-5` 140 unauthenticated bytes lock every local user out of the console | `serialnexusweb/src/server.rs:107` |
| 70 | 🔵 low | `WEBS-1` Bearer token cookie is host-scoped, so any sibling-port local service harvests a shell-equivalent token (undocumented residual) | `serialnexusweb/src/server.rs:460` |
| 71 | 🔵 low | `WIRE-3` The reference codec's "only unrecoverable corruption is an over-max length prefix" claim is false; an under-max mangled prefix silently merges following frames into one bogus `data` event, and no test covers the class | `codecs/reference/src/lib.rs:18` |
| 72 | ⚪ nit | `CTL-3` `serialnexusctl load`'s file-read error does not name the file it could not read | `serialnexusctl/src/main.rs:361` |
| 73 | ⚪ nit | `ITEST-6` Two spec docs still name `offsetSpaceReset`, a function `cfb2187` deleted | `serialnexusweb/ui-tests/tests/lifecycle.spec.mjs:7` |
| 74 | ⚪ nit | `RV-7` nexus-core::config's module doc lists a node kind that does not exist | `nexus-core/src/config.rs:9` |
| 75 | ⚪ nit | `SIMP-6` daemon.rs extracts param and error helpers but only half the call sites adopt them, so the validate-error block appears three times verbatim and the same error code ships two different message shapes | `nexus-daemon/src/daemon.rs:573` |
| 76 | ⚪ nit | `SIMPB-10` `for t in self.tasks.drain(..) { t.abort(); }` is written eleven times, and three nodes re-derive `signal_stop` inside `Drop` where a fourth already calls it | `nexus-daemon/src/nodes/codec.rs:358` |
| 77 | ⚪ nit | `SIMPB-7` `server.rs::path_is` documents a trailing-slash tolerance its body does not implement | `serialnexusweb/src/server.rs:535` |
| 78 | ⚪ nit | `SIMPB-8` The web bridge hard-codes JSON-RPC error numbers instead of the §16.8 registry it already depends on | `serialnexusweb/src/bridge.rs:198` |
| 79 | ⚪ nit | `WEBS-2` CSP connect-src permits WebSocket to any host (`ws:`/`wss:`), broader than the same-origin claim in the code | `serialnexusweb/src/server.rs:422` |
| 80 | ⚪ nit | `WIRE-4` `data_frames`' justification comment claims nothing bounds channel-identity length structurally; validation has bounded it at 256 bytes since invariant 7 | `nexus-daemon/src/runtime.rs:274` |

---

## 1. Bugs — correctness, reliability and security

Ordered by severity. Each finding carries the finder's claim and failure scenario, the verifier's
correction where it made one, and the verifier's own reproduction where one exists.

### `CONC-1` — A pty reader parked on targetward backpressure permanently freezes presence, last-close and detach-release

**🟠 high** · correctness · `nexus-daemon/src/nodes/pty.rs:655` · design §6 detach-release ("an on-demand holder's client left, so the lock frees"); §7.2 presence; the MAP-1 chain named in pty.rs:657-661, codec.rs:205-209, map.rs:236-242 · verdict **CONFIRMED** (high confidence)

`read_and_poll` forwards a client's bytes with `tx.send(payload).await` (pty.rs:655) inside its drain loop. When the host endpoint's targetward channel is full — the ordinary state of a serial node that is `waiting` or flow-control stalled, because its supervisor holds the receiver unread — that await parks for as long as the device is gone. Everything after it in the loop stops running: the presence swap (pty.rs:736), `handle_last_close` (pty.rs:738), termios reconciliation, and detach-release. The node therefore reports `client_present: true` forever after its client exits, and an on-demand write lock stays held by an origin whose client is gone. The MAP-1 fix immediately below (pty.rs:655-669) covers only the *closed*-channel case; the *full*-channel case reaches the identical chain and is uncovered.

**Failure scenario.** Graph: serial `usb0` (device absent or RTS/CTS stalled) — pty `con`, edge `write_mode = "on-demand"`. Operator runs `lock con`, opens the console, types ~256 characters (one master read = one chunk; CHANNEL_CAP = 256, runtime.rs:673), then closes the terminal. `state` reports `client_present: true` and `holder: "con"` indefinitely; a second operator's `send usb0` fails `-32003 endpoint "usb0" is locked; send timed out` and `lock con` fails `endpoint is locked by send/con`. Neither `disconnect` nor a fresh `connect` of the edge unwedges it — the task is still parked on the sender clone it took before the await — so recovery needs `unlock`/`--steal` (which fixes the lock but not the frozen `client_present`), `remove-node`, or the device coming back.

**Verification correction.** The mechanism is exactly as filed, with four corrections/extensions. (1) The threshold is CHANNEL_CAP + 1 = 257 chunks, not ~256: 200 chunks leave presence and detach-release working, 260 freeze them (measured). (2) It needs no device outage. A second, equally reachable path is a serial node that is `active` while the far end is not draining (stalled target / asserted hardware flow control — the §9 head-of-line shape `nexus-sim pty --stall` models): `write_all` backpressures, the channel fills, the console pty's reader parks identically. Measured: 10 client chunks against a stalled device → `client_present:false`, `holder:null`, `send` succeeds; ~137 chunks (>256 daemon chunks) → `client_present:true`, `holder:con`, `send` → -32003, still frozen at T+20s. (3) The frozen block costs more than presence + detach-release: `handle_last_close` also never runs, so the §7.2 baseline-termios reset ("every client session starts deterministic") and purge-on-detach are skipped too, and the RECONCILE_INTERVAL backstop below it stops. (4) The reader task is alive, not dead — it is parked on `tx.send().await`, and the freeze self-heals the moment anything drains the channel: with the device back, `purge_on_reconnect` drained it (`purged_on_reconnect: 400`) and `client_present` went false / `holder` null inside one reconnect poll. Minor: after the freeze `lock con` returns `{"acquired": false}` rather than an error (because `con` *is* the recorded holder); a *different* origin gets the LOCKED error, and `send usb0` does return -32003 as filed. I did not personally re-run the finder's `disconnect`/`connect` arm; that it cannot unwedge follows from the code (the parked task holds a sender clone taken before the await, and the receiver is owned by the serial supervisor for the node's whol…

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
All under XDG_RUNTIME_DIR=$(mktemp -d /tmp/snxver.XXXXXX); graph = pty `con` + serial `usb0` (absent device) + edge usb0-con (default on-demand); `lock con`; a client opens the pts, does N 1-byte writes 2 ms apart, closes.

Scripts: /tmp/claude-1000/-home-pwnall-workspace-serial-nexus/6784367f-5731-471a-b43c-de9e6ca9c5ce/scratchpad/repro_conc1.py (freeze + threshold), repro_conc1b.py (mechanism proof), repro_conc1c.py + child_writer.py (stalled-active variant).

A) Threshold, device absent (`usb0 = waiting`):
  N=50   T+1s..T+20s  client_present=False holder=None      <- control, detach-release works
  N=200  T+1s..T+20s  client_present=False holder=None      <- control
  N=260  T+1s..T+20s  client_present=True  holder=con       <- frozen
  N=400  T+1s..T+20s  client_present=True  holder=con
         send usb0 -> {"code": -32003, "message": "endpoint \"usb0\" is locked; send timed out"}
         lock con  -> {"acquired": false, "held": true, "origin": "con"}
         unlock con -> {"released": true}; after unlock: client_present=True holder=None
         (discarded_targetward stays 0 throughout — the freeze is invisible in the counters)

B) Mechanism proof (the task is parked, not dead) — repro_conc1b.py 400:
  after client exit      client_present=True  holder=con  usb0=waiting  purged=0
  device back (t=0.5s)   client_present=False holder=None usb0=active   purged=400
  2s later               client_present=False holder=None usb0=active   purged=400
  i.e. purge-on-reconnect draining the 256-deep channel released the parked `tx.send()`, the loop
  resumed, and presence + 
… (truncated)
```

</details>

**Fix.** Do not let the data half park the lifecycle half. Either race the forward against a presence tick — e.g. `tokio::select!` over `tx.reserve()` (so the payload is not committed) and a `sleep(IDLE_POLL)` that re-polls `POLLIN|POLLHUP` and runs the present→absent transition — or hoist the presence/last-close block onto its own short-lived poll that cannot be starved by the send. The invariant to state and guard: `read_and_poll`'s presence/last-close/detach-release block must run within a bounded time regardless of targetward backpressure. A `p9_`-family guard should assert `client_present` returns to false after the client exits with the serial endpoint's targetward channel deliberately saturated.

### `HIST-1` — A reload whose ring rolled past the stored frontier splices across a real hole in silence; `history.dropped` is computed and never read

**🟠 high** · correctness · `serialnexusweb/src/assets/app.js:391` · design §5 (all loss is counted and visible), §15.32 browser-side history, §11.8 offsets; history.mjs's own doc at lines 12-13 and 96-97 · verdict **CONFIRMED** (high confidence)

`history.mjs:56` counts a genuine offset-space hole into `h.dropped` when a chunk starts past the frontier — the exact case where the daemon's replay ring rotated past what the tab had stored — but no code anywhere in the client ever reads `h.dropped`. `app.js` marks the *other* kind of hole (`params.gap_before`, line 407) with an explicit terminal marker and comments that "a silent splice here would conceal a real hole — show it instead", yet the identical hole arriving via `tap.open`'s `from_offset` gets only a `— replay (N bytes) —` marker (line 391) that positively suggests continuity. The information needed is in hand at line 391: `res.from_offset` and `stored.endOffset` are both live in that scope.

**Failure scenario.** Reproduced live at HEAD `cfb2187`. Graph: one serial node `usb0` over a `nexus-sim pty --echo` device with `replay_ring = 1024`. Session 1 stores 244 bytes, frontier 244, epoch 1, and the tab closes. The device then emits 2880 bytes with nobody watching, so the 1 KiB ring wraps. On reload, `tap.open` returns `{from_offset: 2880, replay_bytes: 1024, epoch: 1}` — same epoch, so the (correct) re-anchor branch is not taken. `#term` then shows exactly: `— stored history (244 bytes) —`, `session1-line-0…session1-line-3……`, `— replay (1024 bytes) —`, `e-out-43===…while-you-were-out-59===`. 2636 bytes are gone and nothing says so. `history.dropped` equals 2636. Worse, `bytesOf(history)` — what `export` downloads and what `flushSave` writes back to OPFS — is the two ranges concatenated with no marker at all: the exported log reads `…session1-line-3.............\ne-out-43=======…`, i.e. a fabricated adjacency an engineer reading the log as evidence will act on. This is the *common* path: the ring is 64 KiB by default, so any console that emits more than 64 KiB while its tab is closed hits it on every reload.

**Verification correction.** The mechanism is exactly as filed and I reproduced it, but two clauses need correcting.

(1) The finder's "worse" clause — that `bytesOf(history)` concatenates the two ranges with no marker, so `export` and the OPFS write-back carry a fabricated adjacency — is true but is NOT specific to this hole. `appendMarker` (app.js:447) only appends a DOM `<span>`; it never touches `history.chunks`. So the persisted/exported byte log is markerless across *every* hole, including the `params.gap_before` case the project considers handled and the epoch-reanchor case. Storing raw device bytes is a deliberate property (the export is a device log, not a transcript), and injecting markers into `bytesOf` would be the wrong fix. The novel, real defect is narrower and still serious: **the terminal shows no indication of the hole, and `h.dropped` — computed at history.mjs:56 precisely for this — is read nowhere in the client.**

(2) The finder's suggested fix (compare `res.from_offset` against `history.frontier` in `selectConsole`) is correct for the default path but must not be advertised as covering the `replay_ring = 0` case. With no ring the hub is `active` only while a tap is open (`TapHub::new`, `refresh_active`), so `ingested` does not advance while nobody watches; `from_offset` on the next `tap.open` equals the stored frontier and no gap is observable at all. That is invariant 10's delivered-bytes offset space working as specified, not a second instance of this bug.

Everything else stands: default `replay_ring = 65536` means any console emitting >64 KiB while its tab is closed (or while another console is selected — `selectConsole` flushes to OPFS and reloads from it, so a plain console switch is enough) hits this on every return.

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Live, against `target/debug/serialnexusd` at HEAD cfb2187. Daemon on a short runtime dir over a `nexus-sim pty --echo` double, graph = one `serial` node `usb0` with `replay_ring = 1024`, `arbitration = "free-for-all"`. Driver: /tmp/claude-1000/-home-pwnall-workspace-serial-nexus/6784367f-5731-471a-b43c-de9e6ca9c5ce/scratchpad/repro.mjs — it speaks the daemon's NDJSON control socket directly, imports the **real** `history.mjs` from the tree, and transcribes app.js:361-392 + :397-413 line for line. (Rule 4: the reader logs every raw line; 220 lines seen, first/last dumped, so the "no notification" results are not a buffered-readline artifact.)

    subscribe -> {"subscribed":true}
    session 1 tap.open -> {"endpoint":"usb0","epoch":1,"feed_dropped":0,"from_offset":0,"replay_bytes":0,"tap":0}
    [tab closes] OPFS record: 264 bytes, endOffset=264, epoch=1
                 history.dropped so far = 0
    notifications received with no tap open: 0
    session 2 tap.open -> {"endpoint":"usb0","epoch":1,"feed_dropped":0,"from_offset":2050,"replay_bytes":1024,"tap":1}

    ================ what #term shows after the reload ================
    MARKER: "— stored history (264 bytes) —\n"
    TEXT  : "session1-line-0.....…...........\n"
    MARKER: "— replay (1024 bytes) —\n"
    TEXT  : "were-out-38=========…ou-were-out-59=====\n"
    ===================================================================
    stored frontier was      : 264
    tap.open from_offset is  : 2050
    epoch stored / reported  : 1 / 1
    bytes silently missing   : 1786
    h2.dropped counted       : 1786
    s
… (truncated)
```

</details>

**Fix.** Surface the gap where the client already surfaces the other one. Minimal: in `selectConsole`, in the `stored` branch beside the `offsetSpaceChanged` check, compare `res.from_offset` against `history.frontier` and, when it is larger, `appendMarker("\n— ${res.from_offset - history.frontier} bytes lost (ring rotated past stored history) —\n")` before the replay marker. More robust: have `onTapData` snapshot `history.dropped` around each `splice` and emit a marker on any increase, which covers the live case too. Add a `history.test.mjs` case asserting the gap is reported, and a `history.spec.mjs` Playwright case that stores, wraps a small ring, reloads and expects the marker.

### `RES-1` — Duplicate-serial guard counts by-id entries, so it can never fire for two identical adapters — the node binds the wrong physical device

**🟠 high** · correctness · `nexus-core/src/resolver.rs:349` · design §12 ("adapters with absent or duplicated serial numbers degrade to topology identity"), §15.10 · verdict **CONFIRMED** (high confidence)

`Resolver::usb_identity_ambiguous` decides whether a `usb:` identity pins one device by counting `discover_adapters()` entries with that identity — and `discover_adapters` (resolver.rs:393) enumerates only `/dev/serial/by-id`. Two devices that share a `usb:vid:pid:serial:iface` identity necessarily share a single by-id *name* (udev derives it from `ID_SERIAL` + `ID_USB_INTERFACE_NUM` + port — exactly the fields that make the identity ambiguous), so at most one symlink can exist and the count can never exceed 1 for the precise hazard the guard was written for. `capture_for_dev` (resolver.rs:319) therefore stores the ambiguous identity with `warning: None`, and `resolve_current_path` → `find_usb` then resolves it to whichever clone owns the surviving by-id link.

**Failure scenario.** Two FTDI/CH340 clones with the same hard-coded serial (`0403:6001:DUP`, iface 00) on ports 1-1 and 2-1; udev creates one by-id link `usb-FTDI_FT232R_USB_UART_DUP-if00-port0 -> ttyUSB0`. Operator runs `add-node {type:serial, name:portB, device:"/dev/ttyUSB1"}`. The daemon replies `identity: usb:0403:6001:DUP:00`, `resolved_path: .../dev/ttyUSB1`, no warning. `state` immediately reports `resolved_path: .../dev/ttyUSB0` — the *other* physical adapter. Everything typed at console `portB`, every `send`, every `pulse-dtr`/`send-break` goes to the wrong board, and the log/PTY fan-out records the wrong board's output under the wrong name.

**Verification correction.** The finder's claim is accurate as written; two refinements. (1) The guard is not dead code — it *does* fire when one identity is reachable through two differently-named by-id entries (e.g. a multi-port usb-serial whose ttys share one USB interface, named `…-port0`/`…-port1`), which is why the in-tree test passes; it just cannot fire for the shape §12/§15.25 name, two distinct adapters with the same non-empty serial, because those necessarily collide on one by-id name and udev publishes exactly one symlink for it. Confirmed against this box's real `/usr/lib/udev/rules.d/60-serial.rules`: the by-id name is `usb-$ID_SERIAL-if$ID_USB_INTERFACE_NUM[-port$attr{port_number}]`, every component of which is a function of vendor/model/serial/interface — the same fields that make the identity ambiguous. (2) The impact is one step worse than the finder described: because the ambiguous identity is captured with no warning, adding *both* clones produces two serial nodes carrying the *same* `usb:` identity, both resolving to the same `/dev` node — so the second adapter is unreachable by any node, and on real hardware the loser is permanently `faulted` with a bare `Device or resource busy` (TIOCEXCL), with no by-path guidance anywhere in the reply. Reproduced: `add-node portA /dev/ttyUSB0` and `add-node portB /dev/ttyUSB1` both stored `usb:0403:6001:DUP:00` and both reported `resolved_path .../ttyUSB0`.

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Fixture (models exactly what udev produces for two identical clones: one by-id link, by-path for both):
  dev/ttyUSB0, dev/ttyUSB1
  dev/serial/by-id/usb-FTDI_FT232R_USB_UART_DUP-if00-port0 -> ../../ttyUSB0        (ONE link only)
  dev/serial/by-path/pci-0000:00:14.0-usb-0:1:1.0-port0 -> ../../ttyUSB0
  dev/serial/by-path/pci-0000:00:14.0-usb-0:2:1.0-port0 -> ../../ttyUSB1
  sys/bus/usb/devices/{1-1,2-1}/{idVendor=0403,idProduct=6001,serial=DUP}, <u>:1.0/bInterfaceNumber=00
  sys/class/tty/{ttyUSB0->1-1:1.0, ttyUSB1->2-1:1.0}

RT=$(mktemp -d /tmp/snxver.XXXXXX); XDG_RUNTIME_DIR=$RT target/debug/serialnexusd --dev-root $FX &
serialnexusctl --socket $RT/serialnexusd.sock --json ports
  → BOTH devices listed with "identity":"usb:0403:6001:DUP:00", "kind":"usb", "warning":null
serialnexusctl … add-node portB.toml   (device = "/dev/ttyUSB1")
  → {"added":"portB","identity":"usb:0403:6001:DUP:00","kind":"usb",
     "resolved_path":".../dev/ttyUSB1"}          <-- no warning key
serialnexusctl … --json state
  → "identity":"usb:0403:6001:DUP:00", "resolved_path":".../dev/ttyUSB0"   <-- the OTHER adapter
     "reason":"open usb:0403:6001:DUP:00: Inappropriate ioctl for device"  (fixture file, expected)
serialnexusctl … --json ports   (after adding only portB)
  → ttyUSB0 "bound_to":"portB"  AND  ttyUSB1 "bound_to":"portB"  — one node binding two devices
serialnexusctl … add-node portA.toml   (device = "/dev/ttyUSB0")
  → also stores usb:0403:6001:DUP:00; state shows BOTH portA and portB at resolved_path .../ttyUSB0

Mechanism isolated: planting a second, differently-named by-id link
… (truncated)
```

</details>

**Fix.** Count ambiguity over *devices*, not over by-id links: enumerate `<sys_root>/class/tty/*`, run `sysfs_lookup` on each, and count how many yield `identity`. (That listing is also the right source for `enumerate_ports` — see RES-2.) Add a fixture with two same-identity sysfs devices and a single by-id symlink, asserting the capture degrades to `by-path:` with the §12 warning.

### `RES-2` — A `usb:` identity captured without a `/dev/serial/by-id` tree can never be resolved back — add succeeds, node waits forever

**🟠 high** · reliability · `nexus-core/src/resolver.rs:504` · design §12 ("The resolver runs in two directions: input-to-identity once, at add time; identity-to-current-path at every open"); §13 (the doctor's environment check for "by-id tree presence") · verdict **CONFIRMED** (high confidence)

The two §12 directions read disjoint sources. Capture (`capture_for_dev` → `sysfs_lookup`, resolver.rs:317/557) mints a `usb:` identity from the sysfs ancestor walk, which needs only `/sys`. Resolution (`resolve_current_path` → `find_usb`, resolver.rs:364/504) can only match a `usb:` identity by scanning `/dev/serial/by-id`, and bails with `read_dir(&by_id).ok()?` when that directory does not exist. In any environment with `/sys` but no by-id tree, capture succeeds and resolution is permanently impossible — the daemon mints an identity it cannot ever honour.

**Failure scenario.** A container started with `--device=/dev/ttyUSB0` (Docker/podman give the container a fresh `/dev` holding only that node; `/sys` is mounted, `/dev/serial/by-id` is not propagated), or any mdev/busybox-mdev image without udev's `60-serial.rules`. Operator runs `add-node {type:serial, name:console, device:"/dev/ttyUSB0"}`. The daemon replies success with `identity: usb:0403:6001:DUP:00`, a full human description, and `resolved_path: /dev/ttyUSB0` — i.e. it says the device is present and bound. The node then reports `status: waiting`, `resolved_path: null`, `reason: "device usb:0403:6001:DUP:00 not present"` — forever, for a device that is right there and that the same call just described. Re-adding recaptures the identical unresolvable identity, so the operator cannot recover except by discovering the undocumented `raw:/dev/ttyUSB0` spelling. `ports` reports `[]` in the same tree, so the enumeration face gives no hint either.

**Verification correction.** The mechanism is exactly as filed, but two details of the finder's scenario are wrong and one supporting fact is missing.

(1) Recovery is NOT limited to "the undocumented `raw:` spelling". In the same no-by-id tree I proved two working paths: a `load` whose config carries the plain path (`device = "/dev/ttyUSB0"`) comes up `status: active, open: true` — because `resolve_current_path` handles a bare `/`-prefixed string by literal existence check (resolver.rs:388) and `load` never captures; and `add-node` with `device: "raw:/dev/ttyUSB0"` resolves correctly too. `raw:` is also documented — design §12 and the text of `RAW_WARNING` (resolver.rs:544). The defect is therefore not "unrecoverable" but "silently mis-signalled": all three operator-facing surfaces point away from the fix. `add-node` returns success with `resolved_path` populated and no warning; `ports` returns `[]`; and — the fact the finder missed — `nexus-doctor`'s environment check reports `/dev/serial/by-id: "absent (no USB-serial adapter)", status: skipped, reason: "no adapter"` and skips P4 (probes.rs:2465-2477, 1429), on a tree where the adapter IS present in sysfs and at `/dev/ttyUSB0`. The diagnostic AGENTS.md §3 tells operators to attach to every bug report actively misdiagnoses the one environment where this bites.

(2) The finder's fixture used serial `DUP`, which conflates a second effect with the core one. With the tree missing, `usb_identity_ambiguous` (resolver.rs:349) also calls `discover_adapters`, sees zero adapters, and returns false — so the §15.10 duplicate-serial wrong-device-adoption guard silently does not fire either, and a duplicated serial is captured as `usb:` where a healthy tree would have degraded it to `by-path`. That is a real second consequence, but it is not required: I reprodu…

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Fixture built entirely under a short `mktemp -d /tmp/snxver.XXXXXX`, mirroring `nexus-itest/tests/p7_squatter.rs::make_usb_iface` (sysfs idVendor/idProduct/serial/bInterfaceNumber + `sys/class/tty/ttyUSB0/device` symlink) but with **no** `dev/serial/by-id` and no `by-path`; the device itself is a `nexus-sim pty --echo --link <root>/dev/ttyUSB0`. Daemon: `serialnexusd --socket $R/d.sock --state-file $R/state.json --dev-root $R/root`. Raw NDJSON over the Unix socket (raw bytes dumped, not a parsed summary — per rule 4).

--- no by-id tree, UNIQUE serial UNIQ01 ---
-> {"method":"add-node","params":{"node":{"type":"serial","name":"console","device":"/dev/ttyUSB0","arbitration":"free-for-all"}}}
<- {"result":{"added":"console","description":"FTDI-ish Fixture Serial, serial UNIQ01, interface 00","identity":"usb:0403:6001:UNIQ01:00","kind":"usb","resolved_path":"/tmp/snxver.4fAHSq/root/dev/ttyUSB0"}}
   (success; no warning; resolved_path populated — the reply says the device is present and bound)
-> {"method":"state"}   [after 4 s = several 1 s reconnect-poll cycles]
<- {"identity":"usb:0403:6001:UNIQ01:00","identity_kind":"usb","name":"console","open":false,
    "reason":"device usb:0403:6001:UNIQ01:00 not present","resolved_path":null,"status":"waiting"}
-> {"method":"dump"}
<- {"node":[{... "device":"usb:0403:6001:UNIQ01:00" ...}]}   (the dead identity is persisted)
-> {"method":"ports"}
<- {"ports":[]}   (enumeration gives the operator no hint)

--- CONTROL: identical fixture + one symlink `dev/serial/by-id/usb-FTDI_UNIQ01-if00 -> ../../ttyUSB0` ---
<- add-node: identical rep
… (truncated)
```

</details>

**Fix.** Give `find_usb` (and `enumerate_ports`) a sysfs fallback: when `/dev/serial/by-id` is unreadable or yields no match, list `<sys_root>/class/tty/*`, run `sysfs_lookup` on each name, and return `<dev_root>/dev/<name>` for the one whose identity matches exactly (squatter refusal is preserved — it is still an exact identity match). That makes both §12 directions read the same source and simultaneously gives RES-1 its device-level ambiguity count and `ports` a working enumeration without udev.

### `RV-1` — A pty node that faults during setup leaves its symlink installed, aliasing another console's live pts

**🟠 high** · correctness · `nexus-daemon/src/nodes/pty.rs:160` · design 7.2, 15.8 · verdict **CONFIRMED** (high confidence)

PtyNode::setup() calls install_symlink (setting symlink_installed = true) BEFORE apply_perms and prime_slave. If either later step fails, setup returns Err with self.master still None, so the master fd is dropped and the kernel reclaims the pts number - but the symlink stays on disk and now resolves to whatever pty node next receives that number. install_symlink's dangling_into_devpts recovery does not help because the leftover no longer dangles.

**Failure scenario.** Two pty nodes in one config; the first has an unresolvable group (an ordinary operator typo). conA faults with 'group nosuchgroup12345 not found'; conB comes up active on /dev/pts/4. readlink(conA) and readlink(conB) BOTH yield /dev/pts/4, and holding conA open flips conB's client_present to true. Any operator or script opening the faulted console's path attaches to a different device's console - reading its output and writing into it.

**Verification correction.** The mechanism and file:line are right. Two refinements worth carrying into the report. (1) Reachability is broader than "an operator typo": the same fault fires on a plain `chown` EPERM — a non-root daemon configured with `group = "<a group it is not in>"` (the documented, security.md-endorsed way to widen a console to 0660) faults with `chown /dev/pts/N: EPERM: Operation not permitted`, leaving the symlink behind. `prime_slave` is the other post-symlink step. (2) The aliasing is not confined to one `load`: the leaked symlink points at the freed pts index, and *any* later pty allocation that receives that index is aliased. Proved separately with `add-node` — conA faulted holding /dev/pts/12, a later `add-node conD` got /dev/pts/12, and holding the faulted conA path open flipped conD's `client_present` to true. Scope limit worth stating so the fix is not over-sold: `PtyNode::teardown`/`Drop` do unlink (symlink_installed stays true), so the leak does not survive `load --replace`, node removal or clean shutdown — it persists exactly as long as the faulted node sits in the graph, which is indefinite (nothing retries pty setup).

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Load average was 4.66 but the result is not timing-sensitive — it reproduced deterministically 4/4 in four different configurations.

R=$(mktemp -d /tmp/snxver.XXXXXX)
target/debug/serialnexusd --socket $R/d.sock --state-file $R/state.json &

# Config A (finder's shape): conA with an unresolvable group, then conB.
#   [[node]] type="pty" name="conA" path="$R/conA" group="nosuchgroup12345"
#   [[node]] type="pty" name="conB" path="$R/conB"
serialnexusctl --socket $R/d.sock load $R/cfg.toml     -> "loaded 2 node(s)"
state: conA faulted "group nosuchgroup12345 not found", pts_path: null, symlink "$R/conA"
       conB active, pts_path "/dev/pts/7"
$ readlink $R/conA -> /dev/pts/7 ; readlink $R/conB -> /dev/pts/7
$ stat -L -c '%n dev=%d ino=%i' $R/conA $R/conB
    /tmp/.../conA dev=28 ino=10
    /tmp/.../conB dev=28 ino=10        # same inode: one device, two names

Presence proof (python holds $R/conA open):
    conB client_present BEFORE:  False
    conB client_present WHILE:   True
    conB client_present AFTER:   False

Data-plane proof (config C: sim echo device -> serial usb0 free-for-all -> conB; conA
faulted alongside). Opening $R/conA — the *faulted* node's path — and writing:
    opening /tmp/.../conA -> /dev/pts/10
    read back from the aliased console: b'hello-from-the-faulted-console\n'
i.e. bytes typed at the faulted console's path traversed conB -> usb0 -> the device and
echoed back. An operator or script that opens the faulted console reads and writes a
different node's device.

Mundane trigger (config D), non-root daemon, no typo at all:
    group = "root"  -> 
… (truncated)
```

</details>

**Fix.** Install the symlink last, after apply_perms and prime_slave succeed; or unlink on every setup error path.

### `RV-8` — open_port leaks TIOCEXCL on its own error paths: an ordinary modem-line config permanently bricks a pty-backed device

**🟠 high** · correctness · `nexus-daemon/src/nodes/serial.rs:703` · design 7.1, 15.38, 16.7 · verdict **CONFIRMED** (high confidence)

open_port takes TIOCEXCL at line 703 and then applies configured modem lines (set_dtr/set_rts, 708-715). Any failure after 703 propagates with ?, dropping the SerialPort and closing the fd WITHOUT releasing exclusivity. This is the same claim-not-given-back shape design 15.38 (D2/D3) fixed, but the fix landed only in SerialNode::teardown, which cannot help because the failed open never produced a live port (sh.port is None).

**Failure scenario.** A serial node over a pty-backed device (socat PTY,link=, QEMU -serial pty, nexus-sim - all legal raw: devices per 7.1/12) configured with 7.1's documented initial modem-line assertions ([node.modem] dtr = true). A pts does not support TIOCMSET so set_dtr fails ENOTTY; the port is dropped still exclusive; TIOCEXCL lives on the tty and clears only at its last close, which a held master never reaches. Every reconnect poll then answers EBUSY forever, and the device is un-openable by EVERY process on the machine, not just the daemon.

**Verification correction.** The mechanism is exactly as filed, with three refinements. (1) The finder's suggested fix is incomplete: the same leak also exists in the reopen path (nexus-daemon/src/nodes/serial.rs ~393), where a successful `open_port` is followed by a failing `arm_reader` — `fault()` sets `sh.port = None` and the only `Rc<SerialPort>` drops still-exclusive. A release confined to `open_port`'s internal error path would not cover that arm, because `open_port` returned `Ok`. The correct shape is "release TIOCEXCL on every path that discards a port without storing it in `sh.port`". (2) "un-openable by EVERY process on the machine" overstates by one case: TIOCEXCL is overridden by CAP_SYS_ADMIN, so root can still open; every unprivileged process, including the daemon itself, is refused. This nuance is already recorded in docs/implementation-notes.md's "TIOCEXCL precision" note. (3) Severity is high rather than critical: the *permanent* brick is specific to pty-backed devices, because on a real UART the daemon's close is the tty's last close and the flag clears, so the leak self-heals on the primary production target. It also needs a non-default `[node.modem]` attribute. What keeps it at high rather than medium is that recovery requires restarting the process that owns the pty master (QEMU / socat / nexus-sim) — i.e. rebooting the emulated target — that it survives `teardown` and daemon exit with the graph empty, and that the node's reported reason flips from the true cause to a self-inflicted "Device or resource busy", sending the operator hunting for a squatter that does not exist. That is precisely the operator-hostile failure design §15.38 called out for D2. Worth noting for the fix's guard: no test in the tree configures `[node.modem]` at all, so this open path has zero coverage — wh…

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Load average was 5.36, but this is a deterministic ioctl-ordering defect, not timing-sensitive; it reproduced on every attempt.

Setup (short runtime dir; tree never modified):
  RD=$(mktemp -d /tmp/snxver.XXXXXX); export XDG_RUNTIME_DIR=$RD
  B=target/debug
  $B/nexus-sim pty --echo --link $RD/serialdev --timeout-ms 600000 &   # holds the master
  $B/serialnexusd &

ARM A (control, no modem table) — a.toml:
  [[node]]
  type = "serial"
  name = "usb0"
  device = "raw:$RD/serialdev"
  baud = 115200
  $ serialnexusctl load a.toml   -> "loaded 1 node(s)"; state: "status": "active"
  $ serialnexusctl teardown      -> "tore down 1 node(s)"
  third-party python open of the pts -> "ARM A third-party open OK -> /dev/pts/8"

ARM B (identical + one attribute) — b.toml adds:
  [node.modem]
  dtr = true
  $ serialnexusctl load b.toml   -> "loaded 1 node(s)"
Polling state every 250 ms on a fresh pts shows the true cause, then the mask:
  faulted | open raw:.../dev2: set DTR: Inappropriate ioctl for device (os error 25)   x4
  faulted | reopen raw:.../dev2: Device or resource busy (os error 16)                 forever
  $ serialnexusctl teardown      -> "tore down 1 node(s)";  state: nodes after teardown: 0
  third-party python open -> "ARM B third-party open FAILED -> [Errno 16] Device or resource busy: '/dev/pts/8'"

Decisive disambiguation from the already-fixed D2 (retained fd) — with the graph empty and the daemon alive:
  daemon fds pointing at pts: "(none: daemon holds NO fd to any pts)"
  every process holding /dev/pts/8: (empty)
  sim's fds: 3 -> /dev/ptmx        # only the mas
… (truncated)
```

</details>

**Fix.** Capture the raw fd after set_exclusive(fd, true) and, on any subsequent error in open_port, let _ = sys::set_exclusive(fd, false) before returning. Same one-ioctl shape and reasoning as the teardown fix.

### `WEB-1` — A mistyped or missing --tls-cert silently truncates and overwrites the operator's --tls-key

**🟠 high** · reliability · `serialnexusweb/src/tls.rs:24` · design §15.29 tier 2 / §17 "--tls (rustls, permissive) plus token is the sanctioned non-loopback mode" · verdict **CONFIRMED** (high confidence)

`build_config` decides load-vs-generate with `cert_path.exists() && key_path.exists()`, so if only *one* of the two exists it takes the generate branch — and `generate_self_signed` unconditionally writes **both** paths, truncating the operator-supplied private key that does exist. A private key is not recoverable once truncated, and the only log line names the cert path.

**Failure scenario.** Operator keeps a real cert/key pair at `/etc/nexus/fullchain.pem` + `/etc/nexus/tls.key` and starts the TLS tier with `--tls-cert /etc/nexus/fullchain.pm --tls-key /etc/nexus/tls.key` (one dropped `e`). serialnexusweb writes a self-signed cert to the typo'd path and truncates `/etc/nexus/tls.key`, replacing the real private key with a throwaway one. The server comes up and serves happily; the loss is discovered later, when the key is needed and cannot be restored.

**Verification correction.** The defect is symmetric and its most plausible trigger is not a typo. `build_config` (`/home/pwnall/workspace/serial-nexus/serialnexusweb/src/tls.rs:24`) treats "first run" as `cert_path.exists() && key_path.exists()`, so *any* half-present pair takes the generate branch, and `generate_self_signed` then overwrites **whichever half exists**: `std::fs::write(cert_path, …)` (tls.rs:81) clobbers an operator cert, and `write_private` (tls.rs:93-102, `.create(true).truncate(true)`) truncates an operator private key. The realistic trigger is the ordinary CA workflow rather than a mistyped path: an operator generates the key first (`openssl genrsa -out tls.key`), points `--tls-cert/--tls-key` at the intended pair while the signed cert has not yet been installed, and the private key is destroyed on startup. The server then comes up green — it prints the normal `https://…?token=…` banner and serves — and the single log line names only the cert path (`generating a self-signed TLS cert cert=/…/fullchain.pem`), never the key it is about to truncate. One partial mitigation exists and does not generalize: a mode-0400 key survives (the key write fails EACCES and the process exits 1), but the cert has already been overwritten by then, because the cert write happens first.

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Shape A — CA workflow, key exists, cert not yet installed (the destructive one):

  D=$(mktemp -d /tmp/snxver.XXXXXX); cd "$D"
  openssl genrsa -out tls.key 2048            # 1704 bytes, sha256 0e403542fce0…
  XDG_RUNTIME_DIR="$D" timeout 6 /home/pwnall/workspace/serial-nexus/target/debug/serialnexusweb \
      --bind 127.0.0.1:0 --tls --tls-cert "$D/fullchain.pem" --tls-key "$D/tls.key"

Output (verbatim, the only mention of any file is the cert path):
  INFO serialnexusweb::tls: generating a self-signed TLS cert cert=/tmp/snxver.oxYkZy/fullchain.pem
  serial_nexus web console — open:
    https://127.0.0.1:42665/?token=bced1033…
  INFO serialnexusweb::server: web console listening on https://127.0.0.1:42665

  key before: 1704 bytes 0e403542fce03435831aca8db366fb13fcd267453eb64968c0353169dabde6a2
  key after :  241 bytes e71b6719775c31567fb9ebfddc5d3c3b37b267370a558f7be6ed6ceaf382c0d4   (fresh rcgen PKCS#8)

Shape B — the finder's typo shape, same result: 1704-byte RSA key c0a2d6ab78… replaced by 241-byte a70e73d436… while the server served normally.

Shape C — reverse half (real cert present, key path wrong): cert eee9234b5a1a… overwritten by the 534-byte self-signed e371527… , no warning.

Shape D — mode-0400 key: the cert is still overwritten, then the process exits 1 with
  Error: writing TLS key /tmp/snxver.6btCty/ro.key
  Caused by: Permission denied (os error 13)
and the key survives at 1704 bytes — so file permissions are the only thing standing between an operator's key and this code path.
```

</details>

**Fix.** Treat the pair atomically: if either path exists, refuse to generate — `anyhow::bail!` naming the file that exists and the one that does not — and generate only when *neither* exists. Failing that, open the key with `create_new(true)` so an existing file is an error rather than a truncation, and log the key path (not just the cert path) before writing it. Add an itest for the cert-missing-key-present and key-missing-cert-present shapes.

### `CONC-3` — `LogNode::create` panics instead of faulting when the writer thread cannot be spawned, killing the daemon at startup

**🟡 medium** · reliability · `nexus-daemon/src/nodes/log.rs:215` · design §15.8 (environmental failure changes a node's state, never the graph, and never fails the operation); AGENTS.md §2 "environmental failures fault nodes without failing the load" · verdict **CONFIRMED** (high confidence)

`LogNode::create` ends its writer-thread spawn with `.expect("spawn log writer thread")` (log.rs:215). A thread-spawn failure is an environmental failure (EAGAIN under RLIMIT_NPROC / thread-cgroup limits / memory pressure), which §15.8 says must fault the node without failing the operation that created it — and the two sibling call sites in this same crate do exactly that: `PtyNode::start` matches the spawn error and sets `NodeStatus::Faulted` with the comment "fault the node rather than panicking the runtime thread" (pty.rs:326-334), and `BlockingReader::arm` returns `io::Result` explicitly "for the caller to fault the node rather than panic its supervisor" (boundary.rs:153-155).

**Failure scenario.** A daemon started with `--config` (or a persisted state file) containing a log node, on a host at its thread/PID limit: `LogNode::create` panics, the panic unwinds out of `startup_load` → `serve` → `local.block_on(&rt, serve(...))` in `run()` (lib.rs:165) → the embedder's `main`, so the whole daemon dies at boot instead of coming up with one faulted log node and every console working. Over RPC the same panic unwinds out of `Daemon::load`'s `state.with_mut` closure inside a `spawn_local` connection task: the client gets no response and its connection dies, while the operator has no diagnostic other than a panic line.

**Verification correction.** The finder's facts and mechanism are exactly right (verbatim reproduction below, panic at `nexus-daemon/src/nodes/log.rs:215:18`, daemon exit 101). Two things it understated, both reproduced:

(1) **The startup-fatal path covers the state file, not just `--config`, so it is a crash loop.** `serve` deliberately wraps the state-file `startup_load` in `if let Err(e) = …` with a comment (lib.rs, RV-10) saying a bad state file "must not cost the daemon its life — come up with an empty graph the operator can `load` into". A *panic* is not an `Err` and walks straight through that arm. Because the daemon persists its graph to `<socket>.state.toml` after every successful mutation, a log node that once loaded fine is now in the state file: under thread/PID pressure the daemon exits 101 on **every** boot with no graph and no socket — the same shape invariant 13 was written for (the `replay_ring` crash loop), reached by an environmental failure instead of a numeric one. I hit this by accident: my second run picked up the state file rather than the `--config` I passed and died the same way.

(2) …

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
All runs used a short runtime dir `RD=$(mktemp -d /tmp/snxver.XXXXXX)` and `target/debug/serialnexusd`. Thread-spawn EAGAIN was induced with `ulimit -u 1` before `exec` (startup) and `prlimit --pid <dpid> --nproc=1` (live daemon).

1. BASELINE — lone log node, no limit:
   $ XDG_RUNTIME_DIR=$RD serialnexusd --config $RD/cfg.toml &
   state → {"nodes":[{"name":"rot",...,"status":"active"}]}   # fine

2. STARTUP, DAEMON DIES (config path):
   $ XDG_RUNTIME_DIR=$RD bash -c 'ulimit -u 1; exec .../serialnexusd --config $RD/cfg.toml'
   exit=101
   INFO nexus_daemon: control socket listening socket=/tmp/snxver.gvB7BX/serialnexusd.sock
   thread 'main' (4122452) panicked at nexus-daemon/src/nodes/log.rs:215:18:
   spawn log writer thread: Os { code: 11, kind: WouldBlock, message: "Resource tempor
… (truncated)
```

</details>

**Fix.** Match the spawn result: on `Err(e)`, leave `node.writer`/`node.writer_done` as `None` and set the queue's status to `NodeStatus::Faulted { reason: format!("spawn log writer thread: {e}") }` — the same shape the file-open failure already takes at log.rs:158-166, and the same shape pty.rs uses. The pump then drops-and-counts against a faulted node, which is the documented behaviour for a log whose file would not open.

### `CONC-4` — The v13 TIOCEXCL release is applied in one of the three places the serial supervisor drops the port

**🟡 medium** · reliability · `nexus-daemon/src/nodes/serial.rs:499` · design design §15.38 D2/D3; docs/implementation-notes.md:131-146 ("Exclusivity is a claim the node made, so the node gives it back when it stops") · verdict **CONFIRMED** (high confidence)

The v13 D2 fix releases exclusivity before letting go of the port, on the stated principle that "exclusivity is a claim this node made, so this node gives it up when it stops" — but only in `SerialNode::teardown` (serial.rs:305-311). The supervisor drops the port in two other places, `set_waiting` (serial.rs:499-504) and `fault` (serial.rs:509-514), and neither calls `sys::set_exclusive(fd, false)`. On any tty that survives the daemon's close, the flag outlives the fd (AGENTS.md §7 / notes D3), so the node's own reconnect ritual then reopens the same path and gets EBUSY from exclusivity it left behind — and because the reconnect poll retries the identical open every second, the node never recovers.

**Failure scenario.** Serial node bound to a pty-backed device whose master another process holds open (`nexus-sim pty`, `socat PTY,link=`, QEMU `-serial pty`), or to a real tty a stray process opened before the daemon did. `arm_reader` fails (thread/PID limit) or the reader signals loss: `fault`/`set_waiting` null `sh.port`, the last `Rc<SerialPort>` in the supervisor's match arm drops and closes the fd without `TIOCNXCL`, the tty stays alive on the other holder's fd so the flag persists, and every subsequent `open_port` in the 1 s reconnect loop returns EBUSY. The node reports `faulted`/`waiting` with "Device or resource busy" forever — exactly the symptom AGENTS.md §8 tells the next session to blame the daemon for, on the one path the fix does not cover.

**Verification correction.** The v13 D2 release (`sys::set_exclusive(fd, false)`) is the daemon's ONLY release call site (`nexus-daemon/src/nodes/serial.rs:309`, in `teardown`) against a single take site (`open_port`, serial.rs:703), so the "exclusivity is a claim this node made, so this node gives it back" principle holds at exactly one of the places a `SerialNode` drops a port carrying TIOCEXCL. The finder's two named sites (`set_waiting` serial.rs:499-504, `fault` serial.rs:509-514) do omit the release, but their consequence is largely unreachable as described: on a pty-backed device the ONLY event that produces `Step::Lost` is the master closing, and a master close deletes /dev/pts/N outright (measured: a slave fd held open does not preserve the node), so the stale flag can never EBUSY anything; on a real tty the abandoned tty is destroyed at the daemon's own last close unless a co-holder opened it before the daemon did. Those two sites bite only with a pre-existing co-holder, or via an `arm_reader` thread-spawn failure. The finder's thesis and its predicted symptom are nonetheless correct, at a site it did …

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
RT=$(mktemp -d /tmp/snxver.XXXXXX)
setsid nohup target/debug/nexus-sim pty --echo --link $RT/dev0 --timeout-ms 180000 >$RT/sim.log 2>&1 </dev/null &
setsid nohup target/debug/serialnexusd --socket $RT/d.sock --state-file $RT/state.json >$RT/daemon.log 2>&1 </dev/null &
# config: serial node on raw:$RT/dev0 with `modem = { dtr = true }`, plus a log node + edge
target/debug/serialnexusctl --socket $RT/d.sock load $RT/dtr.toml     # -> "loaded 2 node(s)"

t0 status: faulted
t0 reason: open raw:/tmp/snxver.22u1HK/dev0: set DTR: Inappropriate ioctl for device (os error 25)
t3 status: faulted
t3 reason: reopen raw:/tmp/snxver.22u1HK/dev0: Device or resource busy (os error 16)
=== holders of /dev/pts/8:   (scan of /proc/[0-9]*/fd)
  (none)

# unrecoverable, three ways:
target/debug/serialnexusctl
… (truncated)
```

</details>

**Fix.** Funnel all three through one helper, e.g. `fn release_port(shared: &CriticalCell<SerialShared>)` that does the `set_exclusive(fd, false)` + `sh.port = None` pair, and call it from `teardown`, `set_waiting` and `fault`. That makes the D2 principle structural rather than one call site, which is the same "one rule, one place" shape §16 applies to `effective_write_mode` and `fan_out`.

*Independently reported a second time as `SERX-1`.*

### `CTL-1` — `serialnexusctl tap` ignores `tap.closed` and hangs forever after teardown/`load --replace`

**🟡 medium** · correctness · `serialnexusctl/src/main.rs:734` · design docs/rpc/observation.md `tap.closed` notification; review 26 TAP-1 (ledger line 52) · verdict **CONFIRMED** (high confidence)

`tap_stream`'s notification loop `continue`s on every method that is not `tap.data`, so the terminal `tap.closed` notification is discarded. The CLI then blocks in `read_line` on a connection that will never carry another byte — the exact failure mode `tap.closed` was introduced to eliminate.

**Failure scenario.** Operator runs `serialnexusctl tap console` (or `... --bytes 65536 > capture.bin`) in one terminal and `serialnexusctl load --replace new.toml` (a routine §11 operation) or `teardown` in another. The daemon detaches the tap and sends `{"jsonrpc":"2.0","method":"tap.closed","params":{"endpoint":"usb0","reason":"teardown","tap":1}}`. The CLI drops it on the floor and never returns; a script or CI step that waits on it deadlocks, and the capture file is silently truncated with exit status never produced.

**Verification correction.** `serialnexusctl tap` discards the terminal `tap.closed` notification and then blocks forever in `read_line` on a connection the daemon deliberately keeps alive, so the process never exits after `teardown` / `load --replace` / `remove-node --cascade` on the tapped endpoint. One correction to the finder's wording: the capture file is not "truncated" — the bytes already written are intact and flushed; what is lost is the *termination* (the file simply stops growing and the process never yields an exit status, so any script or CI step waiting on it deadlocks). Note also that the stall is permanent, not a wait for the endpoint to return: the hub is gone from `state.taps` and a re-`load` does not revive that tap id.

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Short XDG_RUNTIME_DIR, absent-device serial node (same shape as `p8_tap::absent_cfg`), so no hardware is needed:

  RD=$(mktemp -d /tmp/snxver.XXXXXX)
  cat > $RD/g.toml <<EOF
  [[node]]
  type = "serial"
  name = "usb0"
  device = "$RD/absent-usb0"
  [[node]]
  type = "pty"
  name = "console"
  path = "$RD/console"
  [[edge]]
  a = "usb0"
  b = "console"
  EOF
  XDG_RUNTIME_DIR=$RD nohup ./target/debug/serialnexusd &
  XDG_RUNTIME_DIR=$RD ./target/debug/serialnexusctl load $RD/g.toml      # loaded 2 node(s)

  # raw client on the same daemon, dumping bytes verbatim (harness audit)
  python3 rawtap.py $RD/serialnexusd.sock &        # tap.open usb0, then recv() loop, 10 s
  XDG_RUNTIME_DIR=$RD nohup ./target/debug/serialnexusctl tap usb0 &   # TPID
  XDG_RUNTIME_DIR=$RD ./target/debug/seria
… (truncated)
```

</details>

**Fix.** In `tap_stream`'s loop, match `note.method == "tap.closed"`: report the endpoint and `reason` on stderr (where the `tap opened:` line already goes) and return, so the process exits with the bytes it received. Add a `p8_tap`-family guard that opens a CLI tap, tears the graph down, and asserts the child exits within a bound.

### `CTRL-1` — `send-break`/`pulse-dtr` keep the serial port fd and a deferred line-state write alive past `remove-node`/`load --replace`, for an unbounded caller-supplied duration

**🟡 medium** · correctness · `nexus-daemon/src/daemon.rs:1382` · design §7.1 (serial signals are ephemeral, act on the live port only); §12/§15.35 (a stray DTR toggle resets the board, which is why `ports` never `open(2)`s); §15.38 D2 (the fd an aborted future holds outlives the node) · verdict **CONFIRMED** (high confidence)

`send_break` (daemon.rs:1382-1390) and `pulse_dtr` (daemon.rs:1406-1415) take an `Rc<SerialPort>` clone out of the node via `serial_port()` (daemon.rs:1367) and then `.await` a sleep of caller-supplied `ms` — with no upper bound (`u64_param(&params, "ms")`, lines 1384/1408) and no re-check that the node still exists. `SerialNode::teardown` (nodes/serial.rs:276-313) clears `sh.port` and releases `TIOCEXCL`, but the verb's clone keeps the fd open, keeps the physical line in the asserted state (BREAK, or DTR at `assert`), and fires `RestoreGuard::drop` (nodes/serial.rs:625-632) — `set_break(false)` / `set_dtr(!assert)` — on that stale fd long after `remove-node` or `load --replace` reported success and a *replacement* node has bound the same device.

**Failure scenario.** Operator runs `pulse-dtr usb0 --ms 5000` (a normal auto-reset), then within the pulse window runs `load --replace` on a file that still contains `usb0`. `load` reports `{"loaded": n}` and the new node opens the same port and reports `active`. Five seconds later the *old*, torn-down node's verb future runs `set_dtr(!assert)` on its retained fd — an unrequested DTR edge on the port the new node now owns, i.e. a board reset nobody asked for, seconds after a reconfiguration the operator believes finished. The same shape with `remove-node usb0`: the daemon reports `removed`, `state` and `ports` show the device free, and the operator hands it to picocom — while the daemon still holds an fd with BREAK asserted and will drive a line-state change on it later.

**Verification correction.** The finder's claim is accurate as written; two refinements from live evidence. (1) The DTR half is no longer an inference — I reproduced it on the bench's real FTDI adapter (`/dev/ttyUSB0`): `pulse-dtr hw --ms 12000 --assert false` in flight, `load --replace` onto a new node `hw2` on the same device at t=1.06s (reported "loaded 1 node(s)", `hw2` `active`), operator sets `hw2 --dtr false` at t=2.07s (state confirms `dtr: false`), and at **t=12.10s DTR flips to `true` on `hw2` with nobody asking** — the torn-down node `hw`'s `RestoreGuard::drop` running `set_dtr(!false)` on its retained fd, ten seconds after the reconfiguration reported success. That is an unrequested DTR edge (a board reset on any auto-reset target) on a console a *different* node now owns, observed through the *new* node's own TIOCMGET. (2) The window is `min(ms, control-connection lifetime)`, not `ms` alone: the §15.20 cancel-safety path does fire the guard and release the clone when the client disconnects (verified: fd count 1 -> 0 on socket close with 600000 ms still to run). That does not weaken the finding, beca…

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Tree untouched (`git status --porcelain` = only pre-existing `.claude/`). Load average 0.89-2.27 throughout. All daemons/sims I started were killed; scratch dir removed.

SETUP
  RD=/tmp/ctrl1v
  nexus-sim pty --echo --link $RD/dev0 --timeout-ms 900000 --hold-ms 900000 &
  # $RD/graph.toml: [[node]] type="serial" name="s" arbitration="free-for-all" device="$RD/dev0"
  serialnexusd --socket $RD/d.sock -c $RD/graph.toml --state-file $RD/state.toml &
  cnt() { ls -l /proc/$DPID/fd | grep -c " -> /dev/pts/10$"; }

RUN A -- remove-node during send-break
  fds before: 1
  serialnexusctl send-break s --ms 15000 &      # fds during break: 1
  serialnexusctl remove-node s   ->  removed s
  serialnexusctl state           ->  (empty graph)
  fds AFTER remove-node: 1
  lrwx------ ... 10 -> /dev/pts/10
… (truncated)
```

</details>

**Fix.** Two independent halves. (1) Bound the duration: range-check `ms` on both verbs against a stated maximum (reuse `nexus_core`'s `MAX_TIMER_MS`, or a tighter signal-specific cap), returning the same named range error §11 uses — `ms` is the one input that makes the window arbitrarily wide. (2) Make the restore node-scoped: after the sleep, re-resolve the node's current port and skip the restore unless it is still the same handle (`self.serial_port(&node)` + `Rc::ptr_eq`), or have `SerialNode` carry a generation/`Notify` that `teardown` trips so an in-flight signal verb aborts, drops its clone and returns a defined "node was removed while signalling" error. A guard in `nexus-itest/tests/p11_*` can pin it exactly as reproduced above: assert the daemon's open-fd count on the device returns to zero immediately after `remove-node`, fail-first proved.

### `HIST-2` — The rendered terminal has no cap: the `<pre>` grows without bound, and shows more than `export` or storage retains

**🟡 medium** · reliability · `serialnexusweb/src/assets/app.js:443` · design §15.32: "Retention is a per-console cap (default 16 MiB, trim-oldest) with export-to-file and clear controls" · verdict **CONFIRMED** (high confidence)

`appendText` appends one text node per fresh chunk to `#term` and nothing ever removes any of them (`termEl.textContent = ""` happens only on console switch and on `clear`). The 16 MiB `DEFAULT_CAP` in `history.mjs` bounds the *retained buffer*, not the DOM. So the page's memory grows linearly with everything the console has ever emitted, and the two views disagree: after 20 MiB have streamed past, the terminal shows all 20 MiB while `export` (`bytesOf(history)`, line 457) hands back only the last 16 MiB.

**Failure scenario.** An operator opens the console on a chatty device and leaves the tab open — the ordinary use of a console. At a modest 100 KB/s the `<pre>` accumulates ~2.9 GB of text over an 8-hour shift plus one text node per `tap.data` chunk (hundreds per second), and the tab is OOM-killed or unusable long before that. The project's own browser suite already observed the mechanism without treating it as a defect: `serialnexusweb/ui-tests/tests/console.spec.mjs:80-84` records that after a 64 MiB burst "the screen catches up at t+60 s" and "Hiding the terminal first does not help — the click that would hide it needs the same thread", and gives that spec a 240 s timeout. §5's contract for a tap is that "a slow spy costs itself data, never its neighbors" — bounded queue, counted drops — but the client converts "costs itself data" into "costs itself the tab", because it renders everything it manages to drain.

**Verification correction.** The rendered terminal is unbounded and nothing trims it — confirmed. Three corrections to how the finder described it.

(1) The dominant harm is not memory, it is that the console's *render throughput decays monotonically with the size of the `<pre>`*, so the console falls further behind its device the longer it stays attached. `appendText` reads `termEl.scrollHeight` on every chunk (line 441) and writes `scrollTop = scrollHeight` when at the bottom (line 443), forcing a synchronous layout of a `<pre>` that only grows. Measured in the project's own pinned Chromium: 34.9 → 33.7 → 28.1 → 28.4 → 24.0 → 17.7 → 14.4 KiB/s over the first 2.9 MB of rendered text. An otherwise identical control run with a 256 KiB DOM trimmer injected (the finder's suggested fix; the history buffer and the debounced full-buffer OPFS save left untouched, so they are not the variable) held a flat 121–144 KiB/s and rendered 10.3 MB in 75 s versus 3.1 MB in 145 s untrimmed.

(2) "one text node per `tap.data` chunk (hundreds per second)" is wrong. The daemon coalesces: I measured ~3–5 nodes/s, 530 nodes for 2.9 MB…

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Fixture (scratchpad only; `$BIN` = /home/pwnall/workspace/serial-nexus/target/debug):

    WORK=$(mktemp -d /tmp/hv2.XXXXXX); export XDG_RUNTIME_DIR=$WORK
    setsid nohup "$BIN/nexus-sim" pty --source --bytes 64MiB --rate 200000 \
      --link "$WORK/hosedev" --wait-file "$WORK/go" --timeout-ms 1800000 --hold-ms 1800000 &
    setsid nohup "$BIN/serialnexusd" --socket "$WORK/d.sock" &
    # g.toml: [[node]] type="serial" name="hose" arbitration="free-for-all"
    #         device="$WORK/hosedev"  hostward_buffer=16384
    "$BIN/serialnexusctl" --socket "$WORK/d.sock" load "$WORK/g.toml"
    setsid nohup "$BIN/serialnexusweb" --bind 127.0.0.1:0 --token <tok> --socket "$WORK/d.sock" &

Driver (Playwright, headless Chromium): goto the bootstrap URL, wait for `#conn.connected`,
click the `hose
… (truncated)
```

</details>

**Fix.** Bound the rendered scrollback to the same retention cap the buffer uses: after appending, drop leading child nodes of `#term` until its accumulated character count is under a limit derived from `history.cap` (tracking the running total in a variable rather than reading `textContent.length`, which is O(n)). That also makes the screen and `export` agree about what the console's scrollback is.

### `HIST-3` — OPFS history records are keyed by the per-boot `instance` nonce and never reclaimed — every daemon restart orphans up to 16 MiB per console, permanently

**🟡 medium** · reliability · `serialnexusweb/src/assets/app.js:322` · design §15.32 ("the party that decided to record is the party whose disk fills"; "the server binds a stable default port, because … an ephemeral port would orphan history on every restart") · verdict **CONFIRMED** (high confidence)

`keyFor(display)` embeds the daemon's per-boot `instance` nonce, so every daemon restart mints a brand-new OPFS filename for every console. `opfs.mjs` has no enumeration and no garbage collection — its only deletion is `clear(key)` for the single currently-selected console (lines 98-104). Nothing ever removes the records belonging to previous boots, so origin storage accumulates one capped record (up to 16 MiB) per console per daemon boot, forever. The client also asks for `navigator.storage.persist()` (`app.js:115`), so when the browser grants it the orphans are exempt from eviction too.

**Failure scenario.** A developer restarts `serialnexusd` twenty times in a day across three consoles — the normal rhythm of iterating on a graph, and exactly what `load --replace`/restart cycles look like. That is 60 new OPFS records a day, none of which any code path will ever delete; with active consoles they reach megabytes each. Once the origin's quota is reached, `save()` rejects with `QuotaExceededError`, `saver.mjs`'s `onError` fires `storageFailed` (`app.js:310-316`), and the client sets `opfsOk = false` permanently and prints "— history persistence failed; scrollback is memory-only from here —". A reload does not recover it: the quota is still full, so the very next debounced save fails again. The console's persistent-history feature is then permanently dead until the operator manually clears site data, and every reload silently loses that console's scrollback. There is no UI or code path that can reclaim the space — `clearbtn` only touches `keyFor(selected)` with the *current* nonce.

**Verification correction.** Substantially as filed, with two corrections and one addition.

(1) Correction — "a reload does not recover it … every reload silently loses that console's scrollback". The last *successfully* stored snapshot still loads and renders after a reload (I observed `— stored history —` restored post-quota-wall); what is lost is everything that arrives after the wall, because no further save ever commits. And the failure is not silent: `storageFailed` paints the badge `history: memory only (write failed)` and writes the terminal marker on every page load, which is §15.32's stated honesty working. The correct statement is "persistence is dead for that origin until site data is cleared out of band, and every reload re-announces it while dropping everything since the last committed snapshot."

(2) Correction of framing — this is a deviation from the design's stated *retention model*, which is the sharpest way to put it. §15.32 says "Retention is a per-console cap (default 16 MiB, trim-oldest)", i.e. total stored bytes bounded by 16 MiB × consoles. Because `keyFor` folds in the per-boot `instan…

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Scratch driver at /tmp/claude-1000/-home-pwnall-workspace-serial-nexus/6784367f-5731-471a-b43c-de9e6ca9c5ce/scratchpad/hist3/{run.sh,drive.mjs,run2.sh,drive2.mjs}. Nothing in the project tree was modified.

PART 1 — the leak. One `nexus-sim pty --echo` device, one `serial` node `usb0`, `serialnexusweb` pinned to 127.0.0.1:18099 (so `location.host` is constant), and one *persistent* Chromium profile reused across three daemon boots. Each phase: start daemon, load the graph, start the web server, drive the browser (select `usb0`, send ~20 KB, wait out the 1 s debounce, fire `visibilitychange`), then enumerate the OPFS root from the page.

  phase A  daemon instance 18101665719524445211
    hist_127.0.0.1_18099__usb0__18101665719524446000.bin  20069
    estimate.usage = 20319
  phase B  daemo
… (truncated)
```

</details>

**Fix.** Add a sweep to `opfs.mjs` (e.g. `pruneForeign(prefix, keepInstance)`) that iterates the OPFS root via `for await (const [name] of root().entries())` and `removeEntry`s any `hist_*` file whose key component does not match the current `location.host` + `instanceNonce`, and call it once from `ws.onopen` after `instanceNonce` is known. Optionally cap total retained records as a second bound.

*Independently reported a second time as `TAP-3`.*

### `HISTC-2` — `clear` does not cancel the debounced save, so the cleared scrollback is written back to OPFS

**🟡 medium** · correctness · `serialnexusweb/src/assets/app.js:466` · design §17 ("Retention is a per-console cap … with export-to-file and clear controls in the UI"; "stored console output lives unencrypted in the browser profile — on shared machines … clearing site data is part of walking away") · verdict **CONFIRMED** (high confidence)

`clearBtn.onclick` deletes the OPFS record but never clears `saveTimer`. A snapshot scheduled by `scheduleSave()` before the click becomes overdue while `window.confirm` blocks the renderer thread, then fires inside `await clear(historyKey)` — while `history` still holds the pre-clear buffer — and re-creates the file the operator just deleted.

**Failure scenario.** A shared lab machine. An operator finishes a session on a device whose console carried credentials, clicks `clear`, reads the confirmation for a second or two, clicks OK, sees `— history cleared —`, and walks away. The next person to open the console URL sees `— stored history (N bytes) —` with the full pre-clear scrollback restored from OPFS.

**Verification correction.** Accurate as filed. One clarification worth carrying into the fix: the leak does not require the record to already exist — in the traced run `removeEntry` threw `NotFoundError` (nothing had been persisted yet) and the overdue debounce then *created* the record from the un-reset `history`. So "cancel the debounce and detach the buffer before awaiting the delete" is the load-bearing half of the fix; serializing the delete through `saver` is a second, independent narrowing (an already-running `saver.save` can also complete after `removeEntry`). Also note the leak is durable: it survives a page reload (the `pagehide` flush does not land before teardown), and the next viewer's pane reads `— stored history (72 bytes) —` with the pre-clear scrollback.

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Fixture (all under /tmp; killed and removed afterwards):
  RUN=$(mktemp -d /tmp/snxver.XXXXXX)
  target/debug/nexus-sim pty --echo --link $RUN/serialdev --timeout-ms 600000 &
  XDG_RUNTIME_DIR=$RUN target/debug/serialnexusd &
  target/debug/serialnexusctl --socket $RUN/serialnexusd.sock load $RUN/graph.toml   # one serial node `usb0`, arbitration = "free-for-all", device = $RUN/serialdev
  target/debug/serialnexusweb --bind 127.0.0.1:0 --token uitesttoken0123456789abcdef --socket $RUN/serialnexusd.sock &

Driver (Playwright from the repo's pinned node_modules, run from scratchpad): open the bootstrap URL, select `usb0`, `#sendline`=SECRET + `#sendbtn` (the echo device returns it hostward, arming scheduleSave), then `page.once("dialog", d => setTimeout(accept, DIALOG_MS))` and click `#clear
… (truncated)
```

</details>

**Fix.** Cancel the debounce and detach the buffer *before* awaiting the delete: at the top of the handler (after the confirm) do `if (saveTimer) { clearTimeout(saveTimer); saveTimer = null; }` and reset `history` to the empty buffer synchronously, then `await clear(historyKey)`. Better still, route the delete through `saver` so it is serialized against writes on the same key rather than racing them. Add a browser spec that asserts the OPFS record is gone after a slow-confirmed clear (the existing `clear drops the stored scrollback…` spec only checks `#term`).

### `LEG-1` — A faces=target leg throws away its channels' DropCounters, so everything it sheds at its intake is invisible in `state`

**🟡 medium** · correctness · `nexus-daemon/src/nodes/leg.rs:394` · design §5 (all loss is counted where it happens); §7.4 leg state; AGENTS.md invariant 9 (F1/DM-3, "the map shipped as the one hostward producer that never counted consumer absence") · verdict **CONFIRMED** (high confidence)

`LegNode::start`'s `Facing::Target` arm does `let _ = wiring.target_counters.remove(&addr);` — it removes each channel endpoint's `Arc<DropCounters>` from the wiring plan and drops it on the floor. That is the *same* `Arc` the producer's `AttachedSink` holds and charges via `counters.add_full(n)` in `runtime::fan_out`, and it is the only place a hostward full-buffer drop at this consuming boundary is recorded. `LegNode::state_extra` (leg.rs:445-493) never reports it, so every byte a `faces = target` leg sheds at its own intake is uncounted and unreportable. Every other consuming node keeps the handle and reports it: `map.rs:203`/`map.rs:307`, `codec.rs:213`/`codec.rs:344`, `exec.rs:223`/`exec.rs:343`, `pty.rs:356`, and `log.rs` via `mod.rs:162`.

**Failure scenario.** Graph: `serial usb0` (hostward_buffer = 4) → edge → `leg uplink` (faces=target, unix, role=listen, channels=["c0"]), plus a second edge `usb0` → `log lg` as a witness. No wire peer ever connects, so the leg's pump never runs, the per-channel relay (leg.rs:382, CHANNEL_CAP = 256) fills, the edge's hostward mpsc (depth 4) fills, and `fan_out` takes the `TrySendError::Full` arm for the remainder. Push 64 MiB through the device. Result: the log accounts for the whole stream (67,002,240 bytes written + `dropped_bytes: 106624` = 67,108,864 exactly), while the leg reports `delivered_hostward: 0, discarded_hostward: 0, discarded_targetward: 0, discarded_unframable: 0, purged_on_reconnect: 0` and `usb0.discarded_unattached: 0` — roughly 50 MB shed with a total of zero across every counter in the daemon. The same thing happens in ordinary operation whenever the far peer or the link is slower than the device (a bound channel whose wire backs up), which is precisely the condition an operator would be debugging.

**Verification correction.** The mechanism and the observable consequence are exactly as filed; two points of precision. (1) `leg.rs:394`'s `let _ = wiring.target_counters.remove(&addr);` does not destroy the `Arc<DropCounters>` — `GraphState::absorb_wiring` (`nexus-daemon/src/daemon.rs:289-290`) already cloned every entry into `st.target_counters` before any node's `start` drains the wiring, so the counter keeps accumulating and `Daemon::connect` (`daemon.rs:970`) even re-hands it to a new `AttachedSink`. The defect is therefore precisely "the leg is the one consuming node kind that never *reports* its target-facing endpoint's `dropped_full`", not "the counter is lost". A fix could equally read it back out of `st.target_counters`, though keeping it on the node (as `map.rs:203`, `codec.rs:213`, `exec.rs:223`, `mod.rs:156/162` do) matches every sibling. (2) The finder's "roughly 50 MB shed" is an understatement of the magnitude: measured, the peerless leg buffers far less than the 256-chunk relay bound suggests, because serial-reader chunks are far smaller than READ_BUF. In my 64 MiB run the leg had retained only…

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Two runs, both on a short XDG_RUNTIME_DIR, scripts under the scratchpad, all children killed on exit (verified no /tmp/snxver.* process survived).

Graph: serial `usb0` (sim pty source, `hostward_buffer = 4`) → log `lg` (witness) and → leg `uplink` (`faces = "target"`, unix, listen, channels ["c0"]); no peer ever dials.

Run A (`--bytes 64MiB`):
  nexus-sim verdict: {"behavior":"source","pass":true,"sent":67108864,...}
  lg:      {"dropped_bytes": 92672, ...}   cap.log = 67016192 bytes
           67016192 + 92672 = 67108864 exactly — the log accounts for the whole stream.
  uplink:  {"c0": {"accepted_targetward":0,"active":false,"binding":"waiting",
                   "delivered_hostward":0,"discarded_hostward":0,"discarded_targetward":0,
                   "discarded_unframable":0,"purged
… (truncated)
```

</details>

**Fix.** Store the `Arc<DropCounters>` per channel (`self.channel_counters: HashMap<String, Arc<DropCounters>>`) exactly as `map.rs:203` and `codec.rs:213` do, and surface it in `state_extra`'s per-channel object — either as a new `dropped_slow_consumer` field (matching every other node kind) or folded into `discarded_hostward`, which is otherwise unreachable for this facing. Add a `p6_*` guard that pushes more than `CHANNEL_CAP + hostward_buffer` chunks at a peerless `faces = target` leg and asserts `delivered_hostward + <the new counter> == sent` the way `p3_log`/`p3_exact_loss` already do for the log.

### `LEG-2` — The leg's write half loses the untransmitted tail of an in-flight chunk, uncounted, when the socket write fails

**🟡 medium** · correctness · `nexus-daemon/src/nodes/leg.rs:819` · design §5 (no-drop / all-loss-counted); §7.4 purge-on-reconnect with counters; AGENTS.md invariant 3 clause 3 ("count any residual") · verdict **CONFIRMED** (high confidence)

In `pump`'s `write` half, `next_send` has already removed a chunk from its bounded receiver when `write_half.write_all(&frame).await` fails. The code returns `PumpEnd::PeerGone` immediately, dropping the rest of that chunk — every source byte from the failing piece onward — with no counter touched. This is the sending-side mirror of the hole LEG-3 closed on the receiving side, where `purge_inbound(purging, &mut rx, n, &stat)` (leg.rs:1161) deliberately counts the in-flight chunk's `n` bytes alongside the drained backlog. Worse, the pieces written *before* the failure were already added to `delivered_hostward`/`accepted_targetward` (leg.rs:822-831), so one disconnect simultaneously overstates delivery and understates loss.

**Failure scenario.** A `faces = host` leg (computer B) whose send source is the arbitrated targetward stream of local writers. An operator issues `serialnexusctl send uplink/c0 "reboot"`; the pump picks the chunk up via `next_send` and the peer's TCP connection drops mid-`write_all`. The `reboot` line is gone: it is not on the wire, it is no longer in the receiver, and it is not in `purged_on_reconnect` (the purge at leg.rs:711-719 runs on the *next* connect and can only drain what is still queued). With `purge_on_reconnect = false` — where the operator explicitly asked the backlog to survive the outage — the loss is both silent and a direct contradiction of the configured intent. Repeats on every peer drop that lands mid-chunk; each occurrence can lose up to one full 64 KiB chunk.

**Verification correction.** The defect is real and reproduced, but the finder described only one of the two ways the pump loses the chunk, and one of its sub-claims is weak.

(a) TWO exit paths, not one. `pump` runs its write/read halves under `boundary::race3`, which is a plain `tokio::select!` (nexus-daemon/src/boundary.rs:59-70) with random branch polling. When the peer dies, the in-flight chunk is lost *either* via the cited `write_all(...).is_err() -> return PumpEnd::PeerGone` arm (leg.rs:819-821) *or* because the read half returns `PumpEnd::PeerGone` first and `select!` drops the write future while it is suspended inside `write_all`, taking the popped chunk's stack frame with it. I proved the second path independently: with the peer doing a clean `shutdown(SHUT_WR)` (read half sees `Ok(0)` deterministically) while its receive window is closed so the write half is definitely parked in `write_all`, exactly one chunk still vanished uncounted. So the finder's suggested fix — charging the residual only on the `write_all` error arm — is INCOMPLETE. The chunk has to be owned somewhere the pump's owner can charge…

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Deterministic, three runs, no timing sensitivity (box load average 1.9-2.6 throughout; the result is an exact byte identity, not a race). Script: /tmp/claude-1000/-home-pwnall-workspace-serial-nexus/6784367f-5731-471a-b43c-de9e6ca9c5ce/scratchpad/leg2/repro.py (nothing in the repo was touched; my daemons were killed on exit — the `serialnexusd` processes still on the box belong to other verifiers).

Setup: one daemon, one node — a `faces = "host"`, `role = "connect"` TCP leg with channel `c0` — dialing a fake peer written in Python that speaks the §9 hello (`SNXL`/v1) and then STOPS READING with `SO_RCVBUF = 2048`, so the daemon's `write_all` parks with a chunk popped. 150 x `send {"endpoint":"downlink/c0","line": "x"*60000}`, each answered `{"sent":60001,"delivered":true}` = 9,000,150 byt
… (truncated)
```

</details>

**Fix.** On the `write_all` failure path, charge the remaining source bytes of the chunk — track a running offset over the `Piece(payload_len, _)` values and add `bytes.len() - written_so_far` to `discarded_unframable` (or a new `discarded_peer_gone`) before returning `PumpEnd::PeerGone`. A `p6_outage` guard that fills a `faces = host` leg's send queue, kills the peer mid-write, and asserts `accepted_targetward + purged_on_reconnect + <the new counter> == sent` would pin it.

*Independently reported a second time as `WIRE-2`.*

### `RES-3` — Adding by the canonical `/dev/serial/by-id/...` path degrades to `raw:` with a warning that is the opposite of true

**🟡 medium** · correctness · `nexus-core/src/resolver.rs:282` · design §12 ("Operator input naming a serial port — a raw /dev path ... — is converted by the resolver into a canonical, structured identity"; "the CLI echoes the resolved identity in human terms ... so the operator notices if the wrong physical device answered") · verdict **CONFIRMED** (high confidence)

`capture_from_path` takes `rooted.file_name()` of the operator's literal input without canonicalizing symlinks, so for `/dev/serial/by-id/usb-FTDI_FT232R_USB_UART_DUP-if00-port0` the `dev_name` handed to `capture_for_dev` is the *link name*, not `ttyUSB0`. `sysfs_lookup("usb-FTDI_…")` and `bypath_of("usb-FTDI_…")` both miss, so the §12 fallback chain runs all the way to `raw:` — for the single most idiomatic input there is, and the one `ports` itself advertises in its `by_id` field.

**Failure scenario.** Operator copies the by-id name out of `ports` (or out of any serial-tooling doc) and runs `add-node {type:serial, name:console, device:"/dev/serial/by-id/usb-FTDI_FT232R_USB_UART_DUP-if00-port0"}`. Instead of the canonical `usb:0403:6001:DUP:00`, `dump` persists `raw:/dev/serial/by-id/usb-FTDI_…`; the operator echo becomes the useless `"raw path /dev/serial/by-id/usb-FTDI_…"` instead of `"FTDI FT232R USB UART, serial DUP, interface 00"` (so §12's wrong-physical-device check is defeated); and the warning emitted reads "a replugged or different adapter on this path is adopted blindly, and the path is not stable across reboots" — which is exactly backwards for a by-id path, whose whole purpose is reboot stability and identity encoding. Squatter refusal (`find_usb`'s exact sysfs match) is silently switched off for that node. The identical degradation applies to `/dev/serial/by-path/...` input.

**Verification correction.** The mechanism and the location are exactly as filed: `capture_from_path` (`/home/pwnall/workspace/serial-nexus/nexus-core/src/resolver.rs:276-286`) derives `dev_name` from `rooted.file_name()` — the *link* name — so `sysfs_lookup` and `bypath_of` both miss for any symlinked `/dev` path and the §12 chain falls through to `raw:`. Two of the finder's sub-claims need sharpening, and one deserves more weight than it was given.

(1) "the one `ports` itself advertises in its `by_id` field" overstates it: `ports.by_id` is the by-id *entry name*, and pasting it verbatim fails loudly (`-32005 device … is not present`, verified). The reachable operator action is constructing `/dev/serial/by-id/<name>` — idiomatic, but the finder's framing implies `ports` hands out the failing string, which it does not. `ports` correctly advertises `identity: usb:0403:6001:A6008isP:00` as the thing to put in `device`.

(2) For a **by-id** path the emitted warning is backwards, as claimed, and the description is degraded — but not "useless": `raw path /dev/serial/by-id/usb-FTDI_FT232R_USB_UART_A6008isP-if00-port0…

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Fixture (mirrors `resolver.rs`'s `add_usb_device`): `<root>/dev/ttyUSB0`; `<root>/dev/serial/by-id/usb-FTDI_FT232R_USB_UART_A6008isP-if00-port0 -> ../../ttyUSB0`; `<root>/dev/serial/by-path/pci-0000:00:14.0-usb-0:2:1.0-port0 -> ../../ttyUSB0`; `<root>/sys/bus/usb/devices/1-1/{idVendor=0403,idProduct=6001,serial=A6008isP,manufacturer=FTDI,product=FT232R USB UART}`, `1-1/1-1:1.0/bInterfaceNumber=00`; `<root>/sys/class/tty/ttyUSB0/device -> ../../../bus/usb/devices/1-1/1-1:1.0`.

  RD=$(mktemp -d /tmp/snxver.XXXXXX)
  XDG_RUNTIME_DIR=$RD target/debug/serialnexusd --dev-root /tmp/snxres3-devroot &

Three `add-node` calls on the same physical device, raw responses:

  device="/dev/serial/by-id/usb-FTDI_FT232R_USB_UART_A6008isP-if00-port0"
  -> {"added":"byidpath","kind":"raw",
      "identity":
… (truncated)
```

</details>

**Fix.** In `capture_from_path`, resolve the input through `std::fs::canonicalize` (or `read_link` while the target is a symlink) before deriving `dev_name`, and pass the canonical `/dev/<name>` as the `raw` fallback spelling. Add tests asserting that a by-id path and a by-path path both capture the same `usb:` identity, description and (absent) warning as the underlying `/dev/ttyUSB0`.

### `TAP-1` — `tap.open --replay` on a ring larger than 8 MiB delivers the OLDEST 8 MiB and silently discards the newest, breaking the exact-splice guarantee

**🟡 medium** · correctness · `nexus-daemon/src/tap.rs:419` · design §5 replay ring / design line 197: "with `--replay` it is preceded by the endpoint's ring (§5) with the exact-splice guarantee"; tap.rs:19-20 "receives the ring snapshot and then the live stream with an exact splice — no gap, no duplication" · verdict **CONFIRMED** (high confidence)

`TapHub::register` queues the whole ring snapshot into the connection's tap channel with `try_send`, inside the same synchronous critical section that the connection task — the only drainer of that channel — is blocked in. The channel is bounded at `TAP_QUEUE_CAP = 128` and each piece is `REPLAY_PIECE = 64 KiB`, so replay is hard-capped at 8 MiB. Because `snap.chunks()` walks oldest-first and the loop *continues* past `Err(Full)` (tap.rs:431-433) instead of stopping, the pieces that fit are the **oldest** 8 MiB and everything discarded is the **newest**. `MAX_REPLAY_RING` is 16 MiB (`nexus-core/src/config.rs:57`) and `GraphConfig::validate` accepts every value up to it, so the entire upper half of a documented, range-validated knob is unusable.

**Failure scenario.** An operator sets `replay_ring = 16777216` on a busy console (the natural "give me the most scrollback the daemon allows" setting; the validator accepts it). The endpoint has produced 24 MiB. The web console opens the console: `tap.open --replay` returns `from_offset: 8388608, replay_bytes: 8388608`. The browser renders and stores ring bytes [8388608, 16777216), then the next live `tap.data` arrives at offset 25165824. `history.mjs::splice` (line 56) sees `offset > frontier`, adds 8388608 to `h.dropped` and appends — so the console shows 8 MiB of *stale* scrollback, then jumps straight to live, with the 8 MiB immediately preceding the live edge — the part the operator actually wants after an incident — permanently gone and rendered without any seam marker.

**Verification correction.** The mechanism is exactly as described and I reproduced it byte-for-byte; two wording corrections. (1) "silently" overstates it: the discarded bytes ARE counted per §5 — `state.taps[].dropped` showed 8388608 in my run — and the delivered pieces carry truthful offsets, so the hole is detectable by a client comparing `from_offset + replay_bytes` against the first live offset (the browser's `history.mjs::splice` line 56 does count it into `h.dropped`, and `app.js:291` surfaces the tap drop count in the status line). What is genuinely absent is any seam in the rendered stream — `app.js:407` marks only `params.gap_before`, which is 0 here because this is registration-time loss, not the `TapFeed::mirror` hop — and any signal in the `tap.open` result itself. (2) The defect is better stated as a *retention-policy* bug than as a size cap: the budget is the connection's *free* channel space, not a constant, so the same truncation hits a sub-8 MiB ring on a connection whose 128-deep tap channel is already loaded (one daemon connection per browser WS session — `bridge.rs:96` — shared by every con…

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Frozen tree, git HEAD cfb2187, prebuilt target/debug binaries. Load average 1.7-2.2.

  RUN=$(mktemp -d /tmp/snxver.XXXXXX)
  ./target/debug/nexus-sim pty --link $RUN/dev0 --source --bytes 24MiB --rate 6291456 \
      --timeout-ms 120000 --hold-ms 120000 --seed 7 &
  XDG_RUNTIME_DIR=$RUN ./target/debug/serialnexusd &
  # load: one serial node, no consumer at all
  #   {"config":{"node":[{"type":"serial","name":"usb0","device":"$RUN/dev0",
  #     "arbitration":"free-for-all","replay_ring":16777216}]},"replace":true}
  # -> {"result":{"loaded":1}}

`state` after the source finished (24 MiB = 25165824 B ingested, feed hop lossless):

  "endpoints":[{"endpoint":"usb0","feed_dropped":0,"taps":0}]
  "nodes":[{"name":"usb0","status":"active","discarded_unattached":25165824,...}]

Then a raw NDJS
… (truncated)
```

</details>

**Fix.** Trim the snapshot's **head**, not its tail: compute the deliverable budget (`out.capacity()` at minimum, or a `REPLAY_PIECE * TAP_QUEUE_CAP` constant) and take only the newest `min(snap.len(), budget)` bytes, setting `from_offset = self.ingested - delivered.len()`. That keeps the splice exact and contiguous with the live stream — a shorter replay is exactly what a bounded ring means — instead of preserving the least useful half. Optionally also cap `MAX_REPLAY_RING` at what a tap can actually deliver, or report the shortfall explicitly in the `tap.open` result. Add a `p8_replay_ring` case with a ring above the budget asserting `from_offset + replay_bytes == ingested`.

### `TAP-2` — With `replay_ring = 0` the offset space silently skips every byte produced while no tap is open — a client splices across an arbitrary hole with no `gap_before`, no counter and no epoch change

**🟡 medium** · correctness · `nexus-daemon/src/tap.rs:161` · design AGENTS.md §6 invariant 10; design §5 "loss is always visible and attributable"; design §11.9/§15.32 (browser-side scrollback beyond the ring) · verdict **CONFIRMED** (high confidence)

`TapFeed::mirror` only forwards (and only counts) when `active` is set, and `refresh_active` (tap.rs:498-501) leaves `active` false when the endpoint has no ring and no open tap. Bytes produced in that window never reach the hub, so `ingested` does not advance and `feed_dropped` is not incremented. On the next `tap.open` the daemon reports `from_offset = ingested` — exactly the previous tap's frontier — with the same `epoch` and `feed_dropped: 0`. Invariant 10's contract is that the offset space is the delivered-bytes space *and* every hole beside it is signalled via `gap_before` from the shared `feed_dropped` atomic; these bytes are a third category that is neither delivered nor signalled, so the hole is invisible to every offset consumer.

**Failure scenario.** An operator sets `replay_ring = 0` on a chatty console to keep daemon memory down, relying on the browser-side OPFS scrollback §11.9 offers instead. `app.js` persists that console's history regardless of the ring setting. The operator closes the tab (or just switches to another console in the left rail, which issues `tap.close`), an hour of device output goes by, and they come back. `selectConsole` restores the stored bytes (frontier 63 in the transcript above), gets `from_offset: 63` and the unchanged epoch, so `offsetSpaceChanged` is false and no re-anchor happens; the first live chunk at offset 63 splices flush against the stored bytes. The console shows one continuous log with an hour of output silently missing and no marker, no `⚠ dropped` badge (both `dropped` and `feed_dropped` are 0), and nothing in `state` pointing at it.

**Verification correction.** The mechanism and the numbers are as filed; two refinements. (1) Scope: because `active` is per-*endpoint* and any open tap sets it, the hole can only open when **no** tap at all is open on that endpoint and `replay_ring = 0`. It is therefore strictly a *cross-session* (tap.close → tap.open) discontinuity, never a hole inside one tap's stream — which is precisely the case §15.32/§11.9 built the offset space for (a reconnecting client splicing stored scrollback). (2) The contract it breaks is stated verbatim and unconditionally in `docs/rpc/observation.md` ("The offset contract … The guarantee a client gets is therefore: *offsets are contiguous, and a hole is always announced*"), which the review-26 ledger cites as the TAP-1b fix's normative text — so this is the same defect class TAP-1b was accepted and fixed for, reachable through a second door. Caveat on the suggested fix: charging un-mirrored bytes to the existing `feed_dropped` atomic conflates "nobody was listening" with "the bounded feed overflowed under a firehose" — that counter is surfaced as loss in `state.endpoints[].feed_…

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Live, git HEAD cfb2187, `target/debug/{serialnexusd,nexus-sim}`; graph = `serial usb0` on a `nexus-sim pty --echo` double (arbitration free-for-all) wired to a `log sink`, driven over the raw Unix control socket from a python client that dumps every raw byte it reads (checker audited per rule 4 — raw frames printed, no readline buffering).

replay_ring = 0:
  1st tap.open  -> {"endpoint":"usb0","epoch":1,"feed_dropped":0,"from_offset":0,"replay_bytes":0,"tap":0}
     tap.data   -> (offset 0,len 19,gap_before 0) (19,19,0) (38,19,0)
  tap.close     -> {"closed":0}
     ... 20 lines echoed hostward while NO tap was open, no tap.data emitted ...
  2nd tap.open  -> {"endpoint":"usb0","epoch":1,"feed_dropped":0,"from_offset":57,"replay_bytes":0,"tap":1}
     tap.data   -> (offset 57,len 13,gap_b
… (truncated)
```

</details>

**Fix.** Charge the un-mirrored bytes to the same shared `feed_dropped` atomic when `active` is false — `if !active { feed_dropped.fetch_add(n) } else if try_send fails { ... }` — so the existing `gap_before` / `tap.open` baseline machinery reports the hole with no new field and no new client work. The cost is one relaxed add per chunk on an untapped ring-less endpoint (§5's "costs nothing when unset" becomes a load plus an add), which is negligible beside the read that produced the chunk. Fix together with TAP-4, whose baseline arithmetic this makes reachable on the first chunk. Guard: a `p8_replay_ring` case that taps, closes, streams, re-taps and asserts `from_offset` moved by exactly the bytes produced, or that the shortfall arrived as `gap_before`.

### `WEB-2` — write_private's 0600 mode is silently not applied when the key file already exists

**🟡 medium** · security · `serialnexusweb/src/tls.rs:93` · design §15.29 tier 2: TLS is what makes the bearer token safe off loopback; §15.28's threat model is explicitly "a loopback TCP port is reachable by every local user" · verdict **CONFIRMED** (high confidence)

`write_private` is documented as "Write a private key with owner-only (0600) permissions", but `OpenOptions::mode()` only takes effect when the file is *created*. Regenerating into a pre-existing path leaves whatever mode the file already had, so the TLS private key can end up group- or world-readable while the code and the CI gate both claim 0600.

**Failure scenario.** Operator restores `~/.config/nexus/tls.key` from a backup or `touch`es it under the default umask 022 (mode 0644) but does not restore the cert. serialnexusweb regenerates, truncates the key into the existing 0644 file, and serves TLS with a private key every local user can read — the exact adversary §15.28 introduced the token to exclude — while the daemon's own CI gate still reports "the generated TLS key is mode 600".

**Verification correction.** The mechanism is exactly as claimed; two details in the write-up need correcting/sharpening. (1) There is no `~/.config/nexus/tls.key` path — the defaults are `$XDG_RUNTIME_DIR/serialnexusweb.crt` / `.key`, and when `XDG_RUNTIME_DIR` is unset they fall back to `./serialnexusweb.{crt,key}` in the *current directory* (`serialnexusweb/src/main.rs:148-156`). That fallback makes the hazard reachable without any operator mode mistake: run `serialnexusweb --tls` from a world-writable cwd and another local user can pre-create `serialnexusweb.key` mode 0666 (or a symlink) and receive the generated private key. (2) The strongest corroboration the finder missed is in-tree: `nexus-daemon/src/daemon.rs:2138-2146` does the belt-and-braces thing for the state file with the hazard spelled out in its own comment — "`mode` applies only at creation; a leftover temp sibling from a crash would keep its old (possibly umask-wide) mode, so narrow it explicitly" — plus a planted-0666 regression test (`state_file_is_written_owner_only`, daemon.rs:2353). The project already learned this lesson for a less sensi…

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
All under /tmp scratch; the repo tree was not touched, and I started no process that outlived its `timeout`.

$ mkdir -p $S/web2 && cd $S/web2 && umask 022 && touch tls.key && chmod 644 tls.key
$ XDG_RUNTIME_DIR=$S/web2 timeout 8 target/debug/serialnexusweb \
    --bind 127.0.0.1:0 --tls --tls-cert $S/web2/tls.crt --tls-key $S/web2/tls.key \
    --socket $S/web2/nope.sock
INFO serialnexusweb::tls: generating a self-signed TLS cert cert=.../tls.crt
serial_nexus web console — open: https://127.0.0.1:39161/?token=4bd56d73...
INFO serialnexusweb::server: web console listening on https://127.0.0.1:39161
$ stat -c '%a %s %n' tls.key
644 241 .../tls.key
$ head -c 30 tls.key
-----BEGIN PRIVATE KEY-----

A freshly generated 241-byte PKCS#8 private key, mode 0644, serving live TLS.

Variant A (0640 
… (truncated)
```

</details>

**Fix.** Use `.create_new(true)` (which both fixes this and enforces WEB-1's refusal), or explicitly `std::fs::set_permissions(path, Permissions::from_mode(0o600))` after the write and verify it; and re-point the itest at a pre-existing, pre-chmodded key path so the assertion actually covers the reachable case.

### `WEB-4` — The bridge never closes the WebSocket when the daemon connection ends, so the console lies about being connected and silently swallows the next send

**🟡 medium** · reliability · `serialnexusweb/src/bridge.rs:123` · design §17: taps must not "silently die" — "the operator watches a dead pane believing it is live" (the principle `tap.closed` exists to serve) · verdict **CONFIRMED** (high confidence)

`bridge` handles only the browser-gone direction. When the *daemon* side ends, the `daemon_to_browser` task exits and drops its `to_browser` clone, but the main loop still holds the original sender, so the writer task never sees the channel close and never calls `ws_sink.close()`. The WebSocket stays open with no Close frame; the page keeps showing "connected", and because a first write to a just-closed socket succeeds, the operator's next `send` is accepted and discarded with no error.

**Failure scenario.** Operator restarts serialnexusd (a routine config change or upgrade) with a console tab open. The tab keeps rendering `connEl.textContent = "connected"` (app.js:110) forever, the terminal simply stops, and no `tap.closed` arrives. The operator types a command into a device console and hits send: `rpc("send", …)` is written into the dead socket, returns no response, and its promise never settles — the input box clears and nothing is reported. Only on the *second* action does the socket error out; `ws.onclose` (app.js:126-129) then settles every pending promise with `null`, which `sendForm.onsubmit` (app.js:488) interprets as LOCKED and pops "…is locked by someone. Steal the lock and send?" for a line that was never delivered to anything.

**Verification correction.** The bridge handles only the browser-gone direction: when the *daemon* connection ends, `daemon_to_browser` exits and drops only its clone of `to_browser`, while the main loop parks in `ws_stream.next().await` still holding the original sender — so the writer task never sees the channel close, `ws_sink.close()` is never called, and the browser's WebSocket stays open with no Close frame for as long as the tab is idle (no keepalive, no timer, no watchdog anywhere: `serialnexusweb/src/server.rs`'s only deadline is `HEAD_TIMEOUT` on the request head, and `app.js` has no polling — its state stream is push-only). The console therefore keeps rendering `connected` (`app.js:110`) over a dead daemon, the terminal simply stops, and `ws.onclose`'s "disconnected — reload to reconnect" (`app.js:119-121`) — the client's own designed signal — never fires. **Correction to the finder's mechanism:** the first browser action after the daemon dies does *not* silently succeed. The daemon socket is AF_UNIX, so the first `d_write.write_all` after peer close fails with EPIPE, the loop breaks, and the Close fr…

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Frozen tree, nothing modified; scratch only. Ran twice, identical result.

  RD=$(mktemp -d /tmp/snxver.XXXXXX)
  XDG_RUNTIME_DIR=$RD target/debug/serialnexusd --socket $RD/d.sock --state-file $RD/state.json &   # dpid
  XDG_RUNTIME_DIR=$RD target/debug/serialnexusweb --bind 127.0.0.1:0 --socket $RD/d.sock --token testtoken &
  python3 wsprobe.py <port> testtoken <dpid>     # raw RFC6455 client, dumps raw bytes (rule 4)

Transcript (run 2; run 1 identical):
  HANDSHAKE: HTTP/1.1 101 Switching Protocols
  --> sent request id=1 method=info
    [after info] b'\x815{"jsonrpc":"2.0","id":0,"result":{"subscribed":true}}\x81~\x00\x98{"…"id":1,"result":{"codecs":…}}\x81Q{"…"method":"state",…}\x81Q{…}'   # 5 Hz state stream live
  ### killing daemon pid 3792446   (SIGKILL)
  daemon alive? False
   
… (truncated)
```

</details>

**Fix.** Make the daemon-gone direction symmetric: select the browser→daemon loop against the `daemon_to_browser` join handle (or a oneshot it fires on EOF) and break out, so `drop(to_browser)` runs and the writer closes the WebSocket. Ideally send a Close frame with a reason first, so app.js can distinguish "daemon went away" from a network drop. Guard it with an itest that opens a bridged WS, kills the daemon, and asserts the WS closes within a bounded time.

### `WIRE-1` — A `Codec::demux` error is logged and thrown away: no counter, no state, no status change — the never-resync policy §7.5 sanctions has no signal at all

**🟡 medium** · reliability · `nexus-daemon/src/nodes/codec.rs:396` · design §7.5 (codec node state: "codec-specific counters (framing errors, resyncs)"; "a codec … treats any framing violation as a protocol error and never resyncs"), §5 all-loss-is-counted, §15.26 out-of-tree codecs · verdict **CONFIRMED** (high confidence)

`hostward_demux` handles a `demux` failure with `if let Err(e) = c.demux(...) { tracing::warn!("codec demux error: {e}"); }` and nothing else. The chunk is dropped, no byte counter moves, `state_extra`'s `framing_errors` stays 0 (it is `resync_count()`, whose trait default is 0 for exactly the codecs that never resync), and `CodecNode::status` is a pure function of upstream attachment (`set_upstream_attached`, codec.rs:292-303) so it stays `active`. Design §7.5 explicitly contemplates the "treats any framing violation as a protocol error and never resyncs" policy, and `docs/codec-authors.md:70-72` tells codec authors it is supported — but returning `Err` from `demux` is the only way the trait can express it, and the daemon's response is a log line.

**Failure scenario.** An operator runs a custom daemon registering an out-of-tree codec (§15.26, the CI-exercised extension surface) whose `demux` returns `CodecError::Framing("bad CRC")` on a corrupt chunk and, being non-resyncing, on every chunk after it. A `[[node]] type="codec" codec="myproto"` node behind a serial port then delivers nothing for the rest of the session. `serialnexusctl state --json` reports the node `status: "active"`, `framing_errors: 0`, every channel `delivered_hostward` frozen at its last value, and `discarded_unattached`/`discarded_targetward`/`multiplexed.dropped_slow_consumer` all 0 — i.e. a completely healthy-looking graph with 100% hostward data loss and zero accounting. The only evidence is a WARN line in the daemon log, which §5 does not accept as loss accounting.

**Verification correction.** Two scope corrections, neither of which kills it. (1) **Unreachable with the shipped daemon.** `Registry::with_builtins` registers exactly one in-process codec, `reference`, whose `demux` returns `Ok(())` on every path — it resyncs by length guidance and counts internally (`codecs/reference/src/lib.rs:88-115`). So no stock `serialnexusd` configuration can hit the `Err` arm; it is reachable only through the §15.26 out-of-tree/custom-daemon surface (which is first-class: `examples/external-codec/`, `docs/codec-authors.md`, a CI gate, and a conformance kit whose `handles_garbage` says in so many words "Err is acceptable" and "a codec over a reliable transport legitimately never resyncs — the kit serves both"). (2) **"No signal at all" is slightly overstated.** A codec author who wants visibility can count its own violations and return them from `resync_count()`, which the node surfaces as `framing_errors` — the hook exists and works. What is genuinely missing is the *daemon's own* response to an `Err`: no counter of its own, no status change, no `state` field, only a WARN. That is what …

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Built a custom daemon outside the frozen tree (scratchpad workspace, path deps pointing back at the repo read-only; nothing written inside /home/pwnall/workspace/serial-nexus — `git status` after: only the pre-existing `?? .claude/`).

acme-codec: passthrough on channel "console" until it sees b'!', then permanently
    `return Err(CodecError::Framing("bad CRC".to_owned()))` — the documented
    "reliable transport, never resync" policy.
acme-daemon: the in-tree examples/external-codec/daemon/src/main.rs verbatim
    (`Registry::with_builtins().register("acme", ...)`).

Graph (nexus-sim pty --echo as the device, so a `send` loops back hostward):
  serial usb0 -> codec mux (codec="acme", faces="target", channels=["console"]) [held]
  mux/console -> pty con [never]

  $ serialnexusctl send m
… (truncated)
```

</details>

**Fix.** Give `ChannelStat`/the node a `framing_errors` (or `demux_errors`) counter incremented on the `Err` arm, add the discarded chunk's bytes to a `multiplexed.discarded_hostward`-style counter, and — matching the leg — set the node `Faulted { reason }` (or at minimum surface the last error string in `state_extra`) so a non-resyncing codec's protocol violation is visible in `state` rather than only in the log. A guard in `nexus-itest/tests/p5_*` driving a stub codec that returns `Err` would pin it.

### `CODEC-1` — Demuxed data on an unconfigured channel identity is dropped with no counter, no state entry and no log line

**🔵 low** · correctness · `nexus-daemon/src/nodes/codec.rs:420` · design §5 (loss is always visible and attributable); §7.5 codec state; §8 (the leg's `unbound` mechanism exists for exactly this situation) · verdict **CONFIRMED** (high confidence)

In `hostward_demux`, an `EventKind::Data` event whose channel is not in the node's configured list has `stat == None` and `channel_sinks.get(...) == None`, so both arms of the `if let Some(sinks) … else if let Some(s) = stat` are skipped and the bytes are discarded in complete silence — no counter, no `state` field, not even a `tracing` line (the only diagnostic in this loop is the `debug!` on `EventKind::Error`). `exec.rs:651-665` has the identical shape for a child that emits on an unknown channel. The inline justification ("data on an unconfigured channel (no stat) is noise from the mux and simply dropped — announced-but-unbound is a leg concern (§7.4)", codec.rs:417-419) conflates two different rules: §8's *unbound* list is about announcements not growing the graph, whereas §5's "all loss is counted where it happens" is universal. This is not a recorded deviation — `docs/implementation-notes.md` §3.1–§3.19 do not mention it, and review 26 §6 does not refute it.

**Failure scenario.** An operator configures a demux for a hardware mux whose channel identity is `console`, but typos it as `consle` (or the device multiplexes a channel the operator did not enumerate, e.g. `gps`). Every byte on the real identity is decoded, matched against no stat, and thrown away. `state` shows the codec `active`, `framing_errors: 0`, `multiplexed.dropped_slow_consumer: 0`, `multiplexed.discarded_targetward: 0`, and the configured channel sitting at `delivered_hostward: 0` — a graph that looks entirely healthy while 100% of the device's output disappears, with nothing anywhere in the daemon pointing at the identity mismatch. The leg, facing the identical situation, surfaces the offending identity in its bounded `unbound` list plus `unbound_overflow`.

**Verification correction.** A `codec`/`exec` node demuxing a channel identity that is not in its configured `channels` list discards those bytes with no counter and no log line — reproduced, 65536 bytes vanished with every counter in `state` reading 0. The finder's *facts* are exact, but two parts of its framing need correction. (1) The claimed headline scenario — an operator typo — is *not* invisible: the mis-spelled channel sits at `status: "waiting"` / `delivered_hostward: 0` forever (I ran that variant), which is precisely §8's "configured-but-unannounced endpoints sit in `waiting`" signal. What is missing is the *diagnosis*, not the symptom: nothing anywhere names the identity actually on the wire, which is exactly the ambiguity the leg's `unbound` list exists to remove. (2) The behavior is not undocumented: `docs/codec-authors.md:249-251` states it outright — "data on an unconfigured identity is dropped as mux noise" — and the normative design does not require a counter (§7.5's codec state is "per-channel status and codec-specific counters (framing errors, resyncs)"; §8's `unbound` is scoped to the *link*…

**Fix.** Give the codec and exec nodes a node-level `discarded_unconfigured_channel` byte counter in `state_extra` (a one-line `Rc<Cell<u64>>` beside the existing `mux_discarded_targetward`), and optionally a bounded, deduplicated identity list mirroring `leg::UnboundSet` so the offending name is visible — the leg has already paid for that design (LEG-2's cap and truncation) and it can be reused. At minimum emit a rate-limited `tracing::warn!` naming the identity, so the misconfiguration is discoverable at all.

### `CORE-2` — `MultiplexedEdgeNotHeld` ignores arbitration while its sibling rule ten lines below exempts free-for-all, so a working codec graph is refused with a message that is false for that case

**🔵 low** · correctness · `nexus-core/src/config.rs:393` · design §6 (free-for-all has no lock), §7.5/§7.6, §16 "one rule, one place" · verdict **CONFIRMED** (high confidence)

The codec-mux write-mode rule (`config.rs:393-402`) is unconditional on the upstream host endpoint's arbitration policy, while the held-origin-uniqueness rule immediately below it (`config.rs:410-415`) explicitly exempts `free-for-all` endpoints "because a free-for-all endpoint has no lock at all (§6)". The same reasoning applies to the mux rule and is not applied: under `Arbitration::FreeForAll`, `EndpointLock::may_write` (`nexus-core/src/lock.rs:177-180`) returns `true` for any origin whose mode is not `never`, so `runtime::reacquire_held`'s fast path (`runtime.rs:167-169`) returns immediately and an `on-demand` mux origin never parks.

**Failure scenario.** An operator wires a machine-to-machine link whose write coordination lives elsewhere: `[[node]] type="serial" name="s" arbitration="free-for-all"` feeding a demux codec, with the mux edge left at the default `on-demand` (or written explicitly). `load` refuses the whole file with `-32002`, naming a lock-parking failure that cannot occur on a lock-less endpoint. The graph is legal and would run; the operator's only recourse is to write `write_mode = "held"` on an endpoint that has no lock to hold. (Fails closed, so nothing is lost — but the two kind-dependent edge rules disagree with each other inside one function, which is the drift shape §16 exists to prevent.)

**Verification correction.** The mechanism is as described, but the impact is narrower than "a working graph is refused" suggests, and the fix is not code-only. On a `free-for-all` host endpoint `held` and `on-demand` are *behaviourally indistinguishable* — I measured `may_write == true` for both, and `Wiring::build` only ever discriminates `Never` vs not (`nexus-daemon/src/runtime.rs:904,912`), so the refused configuration is equivalent to one the validator accepts and differs from it by one word that the error message itself names. So the real defect is (a) an over-rejection of a configuration that would run, and (b) an error message whose stated reason — "any other mode parks forever while `send` reports success" — is factually false for a lock-less endpoint. Also note the fix touches the design, not just the code: `docs/30-design-claude-fable-v13.md:104` states *both* corollaries unconditionally ("an edge feeding a codec's multiplexed endpoint must be `held` or `never` … and two `held` origins on one endpoint are refused"), and the code already deviates from the second one on purpose, guarded by `p9_config_v…

**Fix.** Give the mux rule the same guard the held-uniqueness rule already has: skip it when `self.node_named(&host.node).map(NodeConfig::arbitration) == Some(Arbitration::FreeForAll)`. Factor the "does this host endpoint arbitrate at all?" test into one helper both rules call, so the exemption cannot be added to one and forgotten on the other again. Add a unit test beside `codec_multiplexed_edge_must_be_held_or_never` (config.rs:1950) pinning the free-for-all case as accepted.

### `CTRL-2` — A parked `lock --wait` whose edge is disconnected (or whose node is removed) is refused with "origin is write=never" — a claim about configuration that is false

**🔵 low** · correctness · `nexus-daemon/src/daemon.rs:1463` · design §6/§15.20 (a waiter leaves the queue with a *defined* error); docs/rpc/arbitration.md:96-97 (the two distinct meanings of this -32602); docs/rpc/configuration.md:230-232 (removal wakes parked waiters "with the defined error") · verdict **CONFIRMED** (high confidence)

`wait_for_grant` (daemon.rs:1741-1791) re-attempts `EndpointLock::acquire(id)` after each wake, and `acquire` returns `Acquire::ReadOnly` for **two** different states — an id that is no longer registered *and* an id whose `write_mode == Never` (nexus-core/src/lock.rs:193-198). `disconnect` and `remove-node` unregister the origin and wake the endpoint's waiters (`GraphState::detach_edge_runtime`, daemon.rs:187-198; `remove_node`, daemon.rs:877-888), so a parked waiter that has just *lost its edge* takes the `WaitOutcome::ReadOnly` arm at daemon.rs:1463 and is told `origin "p1" is write=never and cannot hold the lock`. That origin's declared write mode was `on-demand`; nothing about the graph is `never`. The wait does terminate, so this is a diagnosis defect, not a hang.

**Failure scenario.** Endpoint `m` is held by origin `p0`. Origin `p1` (write_mode `on-demand`) issues `lock --wait` and parks in the FIFO queue. A second operator (or the web editor, which has `disconnect` on its allowlist) runs `disconnect m p1`. The parked client's request fails with `-32602 origin "p1" is write=never and cannot hold the lock`, sending whoever debugs it to look for a `write_mode = "never"` in the configuration that has never existed, instead of at the edge that was just removed. The identical message appears when the origin's *node* is removed with `remove-node --cascade`, where docs/rpc/configuration.md promises the removal-defined error.

**Verification correction.** The mechanism and both trigger paths are exactly as filed; two refinements. (1) The root is a doc/code mismatch inside `nexus-core`, not merely a rendering choice in the daemon: `nexus-core/src/lock.rs:74` documents `Acquire::ReadOnly` as "The origin is `write = never` and cannot hold the lock at all", and `nexus-daemon/src/daemon.rs:352` repeats that for `WaitOutcome::ReadOnly` — yet `acquire`'s first match arm (`lock.rs:194`) returns `ReadOnly` for `origins.get(&id) == None`. Both the state machine's own contract and the daemon's render the same. The daemon already owns the accurate wording for exactly this state — `resolve_origin` (daemon.rs:1607-1610) says `"p1" is not a writable origin on any endpoint`, which is the second meaning `docs/rpc/arbitration.md:96-97` documents for this -32602 — so the fix has a ready phrase. (2) The `docs/rpc/configuration.md:230-232` citation is slightly loose: that sentence covers the *removed node's own* endpoint locks, which really do close and yield the correct `WaitOutcome::Closed` message ("endpoint behind origin ... was torn down while waitin…

**Fix.** Split the two states at the source: give `Acquire` a distinct `Unregistered` (or `Detached`) variant in `nexus-core/src/lock.rs:193-198` for the `origins.get(&id) == None` case, add the matching `WaitOutcome`, and render it in `lock` as something like `origin "p1" was detached from its endpoint while waiting` (keeping `-32602`, which docs/rpc/arbitration.md already covers with "an origin that is not a writable origin on any endpoint"). Both `disconnect` and `remove-node --cascade` should be covered by one guard in `p10_edge_surgery.rs`.

### `HIST-4` — `historyEpoch` is not adopted atomically with `historyKey`/`history`, so a flush during (or after a failed) `tap.open` stamps the previous console's epoch onto the new console's record

**🔵 low** · correctness · `serialnexusweb/src/assets/app.js:376` · design §15.38 (the epoch re-anchor); the WEB-4 remediation stance recorded in app.js:329-331 ("`historyKey` and `history` are therefore adopted in one synchronous step and never exist as a mismatched pair") · verdict **CONFIRMED** (high confidence)

`selectConsole` deliberately adopts `historyKey` and `history` in one synchronous step (lines 360-361) precisely so they can never be a mismatched pair — but `historyEpoch`, which is persisted alongside them by `flushSave` (line 438), is assigned much later, at line 376, only inside `if (res)` and only after an `await`. Between line 361 and line 376 the module-global `historyEpoch` still holds the *previous* console's epoch while `historyKey` and `history` already belong to the new one; and if `tap.open` returns an error (`res === null`), it holds the stale value indefinitely.

**Failure scenario.** The operator selects console B while console A (epoch 5) was selected. `historyKey`/`history` become B's at lines 360-361; `historyEpoch` is still 5. Before `tap.open` resolves — or at any point afterwards if `tap.open` failed, e.g. B was removed by another client between the `state` tick and the click — the tab is hidden or navigated away, firing the `visibilitychange`/`pagehide` listeners at lines 479-480. `flushSave` writes B's key with `epoch = 5`. On the next reload of B, `offsetSpaceChanged(5, B's real epoch 6)` returns true, so `reanchor(history, res.from_offset)` throws the frontier back to the ring base and appends the false marker "— the daemon's graph was reconfigured; offsets restarted —", after which the ring is re-rendered and the duplicate is written back to storage. That is precisely the defect §15.38 was written to remove (implementation-notes D1: "stored history 19 → 38 → 57 bytes over three reloads"), reintroduced through the epoch's own pairing gap.

**Verification correction.** `historyEpoch` (app.js:59) is adopted at app.js:376, one `await` after `historyKey`/`history` are adopted at app.js:360-361, and only inside `if (res)`. Any `flushSave` reached while it is stale writes the *new* console's key, bytes and end-offset stamped with the *previous* console's hub epoch — and because `TapHub::epoch` comes from a process-global monotonic counter (`nexus-daemon/src/tap.rs:301-305,335`), two distinct endpoints never share an epoch, so the stamp is wrong *by construction*, not by chance. On the next selection of that console `offsetSpaceChanged` fires, `reanchor` throws the frontier back to the ring base, the client prints the false marker "— the daemon's graph was reconfigured; offsets restarted —", re-renders the replay ring and persists the duplicate. Two corrections to the finder's account, neither of which saves it: (a) the trigger the finder leans on hardest, `pagehide` on a real unload (app.js:480), does *not* land — I measured it, the OPFS write is fire-and-forget and dies with the page; the triggers that do land are `selectConsole`'s own `flushSave()` at…

**Fix.** Reset `historyEpoch` in the same synchronous adopt block as `historyKey`/`history` — set it to `stored ? stored.epoch : null` at line 361 — and treat a null epoch as "do not persist yet" in `flushSave` (or persist the stored epoch unchanged), so a record can never carry another console's epoch. Better still, move `epoch` into the history object itself so the triple cannot be split.

*Independently reported a second time as `TAP-5`.*

### `HIST-5` — The OPFS filename sanitiser collapses `/` to `_`, so two legally-named consoles can share one storage file and overwrite each other's scrollback

**🔵 low** · correctness · `serialnexusweb/src/assets/opfs.mjs:39` · design §3 (only `/` is forbidden in a node name / channel identity); the WEB-4 hazard recorded in app.js:328-330 ("`save()` truncates and one console's scrollback overwrites another's") · verdict **CONFIRMED** (high confidence)

`fileName(key)` replaces every character outside `[A-Za-z0-9._-]` with `_`, which maps the endpoint separator `/` onto a character that is itself legal inside a node name. Two distinct consoles therefore collapse to one filename. `save()` uses `createWritable()`, which truncates, so the collision is a silent mutual overwrite rather than an error.

**Failure scenario.** A graph contains a demux codec named `mux` with a channel `b` (console address `mux/b`) and, separately, a node named `mux_b` — both entirely legal under §3, which bans only `/` in names. Their keys `host::mux/b::N` and `host::mux_b::N` both sanitise to `hist_host__mux_b__N.bin`. Whichever console's debounced save fires last truncates and replaces the other's record. On the next reload, selecting `mux/b` restores `mux_b`'s bytes and `mux_b`'s frontier: the wrong device output is rendered under the wrong console name, and the borrowed frontier then either freezes the console (frontier ahead of the live stream, every chunk trimmed as already-seen) or manufactures a large `history.dropped` gap — which, per HIST-1, is not surfaced either. This is the same consequence the WEB-4 generation-counter fix was written to prevent, reached by a different route.

**Verification correction.** `fileName` (`serialnexusweb/src/assets/opfs.mjs:39`) is non-injective — it collapses `/` onto `_`, a character that is itself legal in a node name — so the consoles `mux/console` and `mux_console` share one OPFS record, and `save()`/`load()`/`clear()` all operate on that one file. Reproduced against the real module (below) and against a live daemon: a graph with codec `mux` channel `console` plus a `map` node named `mux_console` loads cleanly and `endpointsFromState` yields both displays, so both are simultaneously selectable consoles.

The finder's *downstream* elaboration is wrong and should be dropped: the console does **not** freeze and does **not** manufacture a `history.dropped` gap. `TapHub::epoch` (`nexus-daemon/src/tap.rs:335`) comes from a process-global monotonic counter, one per hub, so the record written by the other console *always* carries a different epoch; `offsetSpaceChanged` (`history.mjs:87`) therefore always fires and `app.js:387-390` re-anchors `frontier` to the live `from_offset`. The §15.38 epoch machinery contains exactly the failure mode the finder predicted…

**Fix.** Make the mapping injective: percent- or hex-encode the key (`key.replace(/[^A-Za-z0-9.-]/g, c => "%" + c.charCodeAt(0).toString(16))`), or hash the key and use the digest as the filename. Add a `history.test.mjs` case asserting `fileName` distinguishes `a/b` from `a_b`.

*Independently reported a second time as `WEB-6`.*

### `LEG-3` — The listen role's reject-second-peer loop busy-spins the single-threaded runtime on a persistent accept error

**🔵 low** · reliability · `nexus-daemon/src/nodes/leg.rs:877` · design §7.4 (one active peer per leg; concurrent second connections are refused); §15.36 (the flake session's "a busy-spun core" class) · verdict **CONFIRMED** (high confidence)

`pump`'s third arm is `Some(l) => loop { if let Ok((extra, _)) = l.accept().await { drop(extra); } }` — an `Err` from `accept` falls through the `if let` and the loop re-calls `accept` immediately, with no backoff, no error handling and no exit. Because `accept` on a non-transient error (EMFILE/ENFILE) returns `Err` without waiting for readiness, this becomes an unbounded tight loop on the daemon's single-threaded `LocalSet` runtime, which starves every other node's tasks and the whole JSON-RPC control plane. The sibling accept in `supervise` (leg.rs:634-651) handles exactly this by faulting the node and calling `backoff.sleep().await`; the asymmetry looks like an oversight rather than a decision.

**Failure scenario.** A long-lived daemon with several legs, ptys, taps and control connections exhausts its file-descriptor limit (a leaked tap connection, a low `RLIMIT_NOFILE`, or simply enough concurrent web-console sessions). A `listen`-role leg with a live peer is inside `pump`. `accept(2)` on its listener now returns EMFILE on every call and never blocks. The `reject_extra` future spins at 100% of the single runtime thread: the leg's own read/write halves, every other node's pumps, the 5 Hz state snapshot and the control socket all stop being polled, so the daemon becomes wholly unresponsive rather than degrading — and `nexus-doctor`/`state` cannot be reached to diagnose it. The fd pressure never clears on its own because nothing is running to release anything.

**Verification correction.** The mechanism is real and I reproduced it: with a `listen`-role leg in `pump` and the daemon at its RLIMIT_NOFILE, one extra client connecting to the leg socket makes `reject_extra` (nexus-daemon/src/nodes/leg.rs:877-886) spin at ~54,000 `accept4`/s — 216,420 calls in 4 s on the leg's listening fd, every one `-1 EMFILE` — pinning the runtime thread at 100.4% CPU. Three corrections. (1) The consequence is overstated: the daemon is NOT "wholly unresponsive". tokio's cooperative budget (128 ops per task poll, then a deferred wake — task/coop/mod.rs) keeps every other task scheduled; a `state` round-trip on an already-open control connection measured 0.7-1.1 ms throughout the spin, identical to baseline, and the daemon self-heals the instant fd pressure clears (freeing one fd via `remove-node` dropped CPU from 100.4% back to 2.6%). The real damage is one silently pinned core — silent because this arm, unlike its siblings, logs nothing at all. (2) Precondition: the arm only spins once the leg listener has actually been knocked on while the process is at its fd limit; a quiescent listener …

**Fix.** Match on the result and back off on error, e.g. `match l.accept().await { Ok((extra, _)) => drop(extra), Err(e) => { tracing::warn!(target: "leg", "rejecting a second peer failed: {e}"); tokio::time::sleep(Duration::from_millis(50)).await; } }`. A `Backoff::exponential` capped at a second would be closer to the supervisor's shape. The same guard is worth a unit test that hands the arm a listener whose accept always errors and asserts the loop yields.

### `LEGD-2` — `delivered_hostward` counts bytes that were only dropped at a full sink, so it is inflated by exactly `discarded_hostward`

**🔵 low** · correctness · `nexus-daemon/src/nodes/leg.rs:934` · design §5 "loss is always visible and attributable" — the loss is visible, but the delivery figure beside it is wrong by exactly that amount · verdict **CONFIRMED** (high confidence)

`route_recv`'s host arm adds the whole chunk to `delivered_hostward` whenever `out.live` is true, and separately adds `out.dropped_full` to `discarded_hostward`. `fan_out` sets `live = true` on a `TrySendError::Full` sink (runtime.rs:607-611, deliberately — "it is still live"), so a chunk that no consumer actually received is counted as *both* delivered and discarded. With a single consumer the two fields overlap completely.

**Failure scenario.** A `faces = host` leg with one local consumer that falls behind: the sink's bounded buffer fills and `fan_out` returns `live = true, dropped_full = n` for every subsequent chunk. `state` then reports `delivered_hostward` ≈ everything the peer sent, even though only the buffered prefix ever reached a consumer. An operator reconciling `delivered_hostward` against what the downstream log actually holds is told the leg delivered ~4× what it did.

**Verification correction.** The mechanism and the numbers are exactly right; the headline arithmetic is over-general and the suggested fix is unsafe.

Precise statement: `runtime::fan_out` sets `live = true` on `TrySendError::Full` (runtime.rs:607-611), so `FanOut.live` means "at least one sink is not Closed", not "at least one sink took the chunk". `leg.rs:932-940` (and identically `codec.rs:428-431`, `exec.rs:658-662`) credit the whole chunk to `delivered_hostward` on `live`. The inflation is therefore *not* "exactly `discarded_hostward`" in general:
* `discarded_hostward` also absorbs the all-sinks-Closed/empty case via `unattached.add(n)` (runtime.rs:616-621), where `delivered_hostward` is correctly not incremented — so discards can exceed the inflation;
* with several sinks, `dropped_full` accumulates per sink, so `discarded_hostward` can grow by up to `k*n` per chunk while delivery inflates by at most `n`;
* a mixed chunk (one sink Ok, one Full) is *legitimately* both delivered and discarded — those two counters measure different consumers there.
The unambiguous defect is the chunk where **no sink accepte…

**Fix.** Credit only what was actually taken: `if out.live { s.delivered_hostward.set(get() + n - out.dropped_full) }` (or have `FanOut` report `delivered` bytes distinctly from `live`). Note the codec (`codec.rs:427-431`) and exec (`exec.rs:658-662`) have the same `.live`-gated `delivered_hostward` shape and would need the same treatment; they simply do not also report `dropped_full`, so their inflation is silent rather than self-contradictory.

### `LOGQ-1` — After the log writer thread returns, the pump keeps accepting bytes into a queue nobody drains; up to 16 MiB of loss is reported as `queued_bytes`, never as `dropped_bytes`

**🔵 low** · reliability · `nexus-daemon/src/nodes/log.rs:449` · design §5 ("loss is always visible and attributable"); §7.3 state ("queued bytes, dropped bytes") · verdict **CONFIRMED** (high confidence)

`writer_loop` returns on two paths — a `write(2)` failure under `overflow = "fault"` (log.rs:449) and *any* rotation failure under *either* policy (log.rs:482). Its own comment says "stop draining; the pump drops-and-counts", but the pump does not: `enqueue` (log.rs:370-408) only drops-and-counts once `queued_bytes + len > QUEUE_CAP_BYTES` (16 MiB, log.rs:49). Until then every chunk is pushed into a queue with no consumer, and `state_extra` (log.rs:273) reports those bytes as `queued_bytes` while `dropped_bytes` (log.rs:276) stays flat — so up to 16 MiB of console bytes that will provably never be written are reported as pending rather than lost, and are retained in RAM until the node is removed.

**Failure scenario.** A log node whose directory becomes read-only (or hits ENOSPC on the rename) while running. Operator issues `rotate cap`. `perform_rotation`'s `std::fs::rename` fails, the node faults, `count_abandoned` counts the drained batch, and `writer_loop` returns (log.rs:479-483). The producer keeps streaming; `state` for the node shows `dropped_bytes` frozen at the abandoned-batch count while `queued_bytes` climbs to 16 MiB over the next N seconds. An operator watching `dropped_bytes` to size the outage sees 0 new loss for the whole first 16 MiB, and only then does the counter start moving.

**Verification correction.** The mechanism and the failure scenario are both exactly right; three refinements. (1) Line numbers drift by one/­two: the Fault-arm `return; // stop draining; the pump drops-and-counts` is `nexus-daemon/src/nodes/log.rs:450` (449 is the `notify_all`), and the rotation-Err arm is 475-483 (`count_abandoned(&mut q, &batch[i + 1..])` at 480, `return` at 482). Everything else the finder cites (`QUEUE_CAP_BYTES` at :49, `enqueue` 370-408, `state_extra` 266-282, `signal_stop` 292-304) is accurate. (2) "never as `dropped_bytes`" is precisely: *exactly* `QUEUE_CAP_BYTES` (16777216 bytes) of provably-unwritable console data is permanently reported as `queued_bytes`; the pump's drop-and-count is not absent, it is delayed by 16 MiB. Measured on a 64 MiB stream: `dropped_bytes` 50331648 + `queued_bytes` 16777216 = 67108864 = the bytes produced, to the byte. (3) The finder understates the "provably never written" half of its own claim, which is worth stating because it is what makes the queued bytes loss rather than backlog: `writer_loop` is spawned only from `LogNode::create` (`nexus-daemon/src/n…

**Fix.** Have the writer mark the queue dead on its two return paths (a `writer_gone` flag, or reuse `closed`) and make `enqueue` short-circuit to `dropped_bytes += len` when it is set — matching the comment the code already carries. Optionally drain `items` and count them at the same point so the 16 MiB is released rather than retained. Extend `p3_log_enospc.rs` / the `write_error_under_fault_counts_the_abandoned_batch` unit test with a post-return `enqueue` asserting `dropped_bytes` moves and `queued_bytes` does not.

### `RV-4` — pty mode is not range-checked, and a plausible octal/decimal typo faults the node with an unrelated message

**🔵 low** · correctness · `nexus-core/src/config.rs:584` · design 11, 7.2 · verdict **CONFIRMED** (high confidence)

Invariant 13 and 11 require every numeric attribute to carry a stated maximum and be range-checked structurally. GraphConfig::validate has no case for pty mode (Option<u32>) or advertised_baud, and docs/rpc/configuration.md's range table omits both.

**Failure scenario.** TOML rejects leading-zero integers, so an operator must write 0o600 or 384. Writing mode = 666 (the octal spelling read as decimal) yields 0o1232 - owner -w-. The node faults with 'prime open /dev/pts/N: EACCES: Permission denied', a message that never mentions the mode and sends the operator hunting for a devpts permissions problem.

**Verification correction.** The `mode` half is real and reproduced; the `advertised_baud` half should be dropped, and the suggested fix is incomplete as stated.

(a) `advertised_baud` is not a co-equal gap. Design §7.2 and `docs/implementation-notes.md` §3.8 both state that a nonstandard advertised baud is *skipped rather than approximated* (`nodes/pty.rs::standard_baud` returns `None` and the termios speed is simply not set), so every `u32` value — including 0 — is safely handled and no consequence is reachable. Listing it beside `mode` as an equal omission overstates the finding.

(b) A `mode <= 0o777` check catches the 3-digit-decimal-typo family only by arithmetic accident, not by rule. `mode = 154` (0o232 — no owner read) passes such a cap and produces the byte-identical unrelated message. And the *principled* cap — 0o7777, since Linux's `chmod_common` masks everything above `S_IALLUGO`, which I confirmed with `mode = 100666` chmod'ing successfully to 0o4472 — would let `666` (0o1232) straight through. So the durable fix is either to reject a mode that does not grant the daemon owner rw (`prime_slave` open…

**Fix.** Range-check mode (<= 0o777) in validate, which rejects every 3-digit decimal typo of an octal mode since they all exceed 511, and names the field before anything is created.

### `SERX-2` — `send-break`/`pulse-dtr` hold the port fd across an unbounded sleep, and drive its lines after teardown has handed the device to a replacement node

**🔵 low** · reliability · `nexus-daemon/src/nodes/serial.rs:636` · design §7.1 ("holds the port open for its lifetime … line states must be deterministic"); §15.38 D2 · verdict **CONFIRMED** (high confidence)

`serial_port()` (daemon.rs:1367-1379) hands the control connection an `Rc<SerialPort>` clone which `send_break`/`pulse_dtr` hold across `tokio::time::sleep(Duration::from_millis(ms))` (serial.rs:642, 654), with `ms` taken straight from RPC params and never range-checked (daemon.rs:1384, 1408). Teardown neither cancels nor waits for that future: it releases `TIOCEXCL` and clears `shared.port`, but the signal future's clone keeps the fd open with a break or DTR level asserted. Because exclusivity was just released, a `load --replace` replacement node opens the *same physical port* successfully, and the old future's `RestoreGuard` (serial.rs:625-632) then drives `set_break(false)` / `set_dtr(level)` on a line the new node owns.

**Failure scenario.** Operator (or a provisioning script) issues `pulse-dtr usb0 --ms 100` and, inside that 100 ms, a config reload lands: `load --replace`. The outgoing node's teardown releases TIOCEXCL; the replacement `SerialNode::create` opens `/dev/ttyUSB0`, applies its configured termios and modem lines, and comes up `active`. 100 ms later the orphaned guard sets DTR to `!assert` on the old fd — the classic auto-reset toggle — rebooting the board the new node has just come up against, with nothing in `state` attributing it. With `send-break --ms 60000` (accepted: no bound) the break stays asserted on the line for a minute across the replacement node's whole startup, so its transmissions are garbage and it cannot be diagnosed from `state`.

**Verification correction.** The mechanism is real and I reproduced it at the ioctl level, but three clauses need correcting.

(1) The strongest half of the defect is not the restore guard — it is that the *asserted line state itself* straddles the replacement, with no race at all. `send-break` asserts `TIOCSBRK` on fd A; `load --replace` then releases `TIOCEXCL` on fd A and opens fd B on the *same tty*; break is tty state, so the replacement node comes up and transmits under an asserted break for the remainder of `ms`, with nothing in `state` showing it. Only afterwards does the orphaned `RestoreGuard` issue `TIOCCBRK` on fd A — a line the successor owns.

(2) What is reproducible without hardware: `send-break` only. On a pts `set_dtr` ENOTTYs at `pulse_dtr`'s *first* call (serial.rs:649), so it never reaches the sleep — `p7_signals.rs` pins exactly that. The finder's headline DTR-auto-reset scenario is therefore real by code shape (identical guard, identical orphaning) but hardware-only in practice; the finder should not have presented it as the demonstrated case.

(3) The "unbounded `ms`" clause is factually …

**Fix.** Bound the assertion duration at the verb (reuse `MAX_TIMER_MS`, one hour, or something far smaller for a break) and make the guard generation-aware: capture the node's port generation/`Rc::as_ptr` before the sleep and skip the restore if `shared.port` no longer holds that same port, so a torn-down node's signal verb cannot drive a successor's line. Alternatively have `teardown` bump a per-node epoch the signal futures check after their sleep.

### `SIM-1` — `nexus-sim exec-conformance` deadlocks with no verdict against a codec that does not drain its stdin

**🔵 low** · reliability · `nexus-sim/src/main.rs:1651` · design nexus-sim module doc ("prints a single JSON verdict line on exit"); §15.26 / plan §10.5 — "the CI entry point for an out-of-tree exec codec"; §15.36 doubles doctrine · verdict **CONFIRMED** (high confidence)

`check_fragmentation` pushes a near-`MAX_FRAME_SIZE` frame (~65 419 bytes) into the child's stdin with unbounded blocking `write_all` (`ExecChild::write_raw`, line 1508-1516). A 64 KiB pipe accepts only ~65 184 bytes when written in 97-byte pieces, so a child that does not read stdin blocks the harness in `anon_pipe_write` permanently. No timeout guards the write, so `nexus-sim` never prints a verdict line and never exits.

**Failure scenario.** A third-party codec author runs the documented conformance entry point against a codec whose main loop is broken so it never reads stdin (e.g. it blocks on a socket, or crashes into a sleep before the read loop): `nexus-sim exec-conformance --exec "python3 my-codec.py"`. Instead of `{"checks":{...},"pass":false}` and exit 1, the process hangs indefinitely. In CI this is a wedged job with no diagnostic; the tool's own module doc promises "prints a single JSON verdict line on exit, and exits 0 only on pass" (main.rs:7-8).

**Verification correction.** The mechanism is exactly as described and reproduces deterministically; two framing details deserve correction. (1) This is not a §15.22 violation — `ExecChild` *does* drain stdout on a thread, so a blocked stdin write never starves the read, and the module comment at nexus-sim/src/main.rs:1420-1425 is accurate. The defect is narrower: `ExecChild::write_raw` (main.rs:1508-1516) is the one terminal path in the exec-conformance battery with no deadline, while every `recv` is bounded and the sibling `envelope` mode in the same file already solves this (main.rs:1368-1395 feeds stdin from an abandonable thread and bails with `child did not complete within N ms`). (2) `check_fragmentation` is where it hangs *at default arguments*, but the unbounded write is a property of `write_raw`, so `check_liveness`'s send loop reaches it too once `--liveness-frames` exceeds ~124 (523 wire bytes per liveness frame vs. 65 184 of usable pipe) — the fix belongs in `write_raw`, not in one caller. Severity: this is a `publish = false` dev/CI tool, the shipped daemon's exec node is unaffected (`nexus-daemon/…

**Fix.** Move the stdin feed off the main thread (a writer thread the harness can abandon), or bound each `write_raw` — set the pipe non-blocking and loop with the same deadline `recv` already uses, returning `Ok(false)` with a named reason (`"child did not drain stdin within N ms"`) instead of blocking. Every other terminal condition in this file is already bounded; this is the one that is not.

### `TAP-4` — `tap.open`'s `feed_dropped` baseline double-counts feed loss that the hub has not yet charged as `gap_before`

**🔵 low** · correctness · `nexus-daemon/src/tap.rs:445` · design AGENTS.md §6 invariant 10 ("`tap.open`'s `feed_dropped` as the client's baseline"); §5 all-loss-counted · verdict **CONFIRMED** (high confidence)

`register` reports `feed_dropped: self.feed_dropped.load(...)` — the raw atomic — while `ingest` charges `gap_before = feed_dropped - feed_dropped_seen` to *every* registered tap, including one that just joined (tap.rs:369-371, 375-389). When a drop occurs and the hub has not ingested a chunk since, `feed_dropped > feed_dropped_seen` at registration time, so the new tap's documented baseline already contains loss that the very next chunk will *also* deliver to it as `gap_before`. The contract the daemon states — `tap.open`'s `feed_dropped` is "the baseline for the `gap_before` deltas that follow" (daemon.rs:1250-1254, tap.rs:108-111) — is then violated: baseline + Σ`gap_before` exceeds the true loss.

**Failure scenario.** A burst overruns the 256-deep feed and 50 000 bytes are dropped on the producer's last mirror attempt before the device goes quiet; `feed_dropped = 50000`, `feed_dropped_seen` stays at its pre-burst value. Minutes later an operator opens the console: `tap.open` returns `feed_dropped: 50000`. The device emits one more line; `ingest` computes `gap_before = 50000 - old_seen = 50000` and sends it to the new tap. A client obeying the documented contract records 100 000 bytes lost where 50 000 were lost; the web console renders "— 50000 bytes lost (daemon feed) —" (app.js:407) for a hole that predates the tap and is already inside the replayed ring window.

**Verification correction.** The claim body is correct and I reproduced it; the *failure scenario* narrative is wrong in its tail and should not ship as written.

What is true: `TapHub::register` (`nexus-daemon/src/tap.rs:445`) reports `feed_dropped: self.feed_dropped.load(Relaxed)` — the raw shared atomic — while `TapHub::ingest` (`tap.rs:369-371`) charges `gap_before = feed_dropped − feed_dropped_seen` to *every* tap in `self.taps`, with no per-tap watermark and with the just-registered tap already pushed (`tap.rs:439`). When a feed drop is recorded but the hub has not ingested a chunk since, a tap registering in that window gets the same bytes twice: once inside its `tap.open` baseline and again as its first `gap_before`. `baseline + Σgap_before` then exceeds the endpoint's true `feed_dropped` (which `state` reports from the same atomic, `tap.rs:510`), contradicting `docs/rpc/observation.md:481-483` and `:544` ("the baseline against which the `gap_before` deltas that follow are read") and AGENTS.md invariant 10.

What is NOT true, and should be struck from the finding: "the web console renders '— N bytes lost…

**Fix.** Report the *reported-so-far* watermark as the baseline — `feed_dropped: self.feed_dropped_seen` — so the not-yet-charged delta reaches the new tap as its first `gap_before` and `baseline + Σgap` is exact. (Reporting the raw atomic and also advancing `feed_dropped_seen` in `register` would be wrong the other way: it would swallow the gap for the taps that were already open.) Extend `tap::tests::feed_loss_surfaces_as_gap_before_on_the_next_chunk` with a tap registered between the drop and the next ingest.

### `WEB-5` — 140 unauthenticated bytes lock every local user out of the console

**🔵 low** · security · `serialnexusweb/src/server.rs:107` · design §17 "Pre-authentication surfaces are capped and dead-lined (connection cap, header timeout, bounded WS messages)" · verdict **CONFIRMED** (high confidence)

`MAX_CONNECTIONS` is a single 128-slot pool shared by pre-authentication and authenticated connections, with no per-peer bound and a 15 s pre-auth deadline. Any unprivileged local user — the adversary §15.28 names as the reason the token exists — can hold all 128 slots with one byte per connection, and the newest connection (the operator's) is dropped. I am not proposing to remove the cap; the gap is that the recorded trade (drop-vs-queue, server.rs:59-60) weighed only queueing, not a per-peer bound or a shorter pre-auth window.

**Failure scenario.** A second (unprivileged, untrusted) local user — or any local process — runs a 10-line loop opening 140 TCP connections to 127.0.0.1:8080 and sending one byte each, reopening every 15 s. The operator's browser can no longer reach the console at all (connection reset), permanently, at a cost of ~140 bytes/15 s and no credentials. The same loop also blocks a `--tls`/`--insecure-bind` deployment from the network.

**Verification correction.** A single global 128-slot Semaphore shared by pre-authentication and authenticated connections, with no per-peer bound, lets any unauthenticated peer (a local user on the loopback default; any on-network peer under --tls/--insecure-bind) pin all slots at trivial cost — one byte per connection holds a permit for the full 15 s HEAD_TIMEOUT — and every new connection, including the operator's browser, is reset while the pool is full. The effect is severe, effectively-denying degradation of the web console rather than a literal 100% permanent lockout: a client retrying fast still wins some slots, but a browser needs a burst of successful connections to load and hold the console, and a staggered attacker holding 128 permits at roughly 8.5 conn/s sustains near-continuous denial. Availability of the web UI only; the daemon and its CLI/RPC over the 0600 Unix socket are unaffected.

**Fix.** Split the pool: reserve slots for connections that have already passed the token gate (acquire a cheap pre-auth permit, release it and take an authenticated permit after the cookie check), and/or add a small per-peer-IP cap on pre-auth connections and shorten `HEAD_TIMEOUT` to a couple of seconds (a complete head is one round trip). Update the itest to assert the operator can still be served while the pre-auth pool is full.

### `WEBS-1` — Bearer token cookie is host-scoped, so any sibling-port local service harvests a shell-equivalent token (undocumented residual)

**🔵 low** · security · `serialnexusweb/src/server.rs:460` · verdict **CONFIRMED** (high confidence)

The per-session bearer token is delivered as the host-scoped cookie `nexus_session` (`Path=/`, cookies cannot be port-scoped). The browser therefore replays it on every request to any service on any TCP port of the same host. The design's stated mitigation for 'cookies are not port-scoped' (the Origin check, server.rs:273-283) guards only the inbound direction — requests arriving *at* serialnexusweb — and does nothing to stop the cookie value being *sent to* a different-port service. This outbound token-exfiltration residual is neither closed nor documented; security.md discusses port-non-scoping only in the inbound/SameSite context, so a reader would reasonably believe Origin+SameSite cover cross-port.

**Failure scenario.** On a shared machine (the exact threat model §15.28/§15.29 cite: 'a loopback TCP port is reachable by every local user'), a hostile local user runs any web server on another port, e.g. `python3 -m http.server 9999 --bind 127.0.0.1`. While the operator holds the console cookie, they visit http://127.0.0.1:9999 in the same browser (their own dev service, a link, any navigation to that host:port). The attacker's access log now contains `Cookie: nexus_session=<token>` (I demonstrated this live: a rogue Node server on 127.0.0.1:18081 received `nexus_session=deadbeefcafe` on a plain navigation while the console cookie was set). The token equals shell-level access — the attacker uses it to `add-node` an exec codec, executing an arbitrary command line as the daemon's user (docs/security.md:290-300).

**Verification correction.** The mechanism requires the operator's browser to actually navigate to / contact the hostile sibling port (the finding's own scenario states this); a passive sibling service the operator never contacts harvests nothing. The title's phrasing ("any sibling-port local service harvests") slightly overstates passivity, but the body is accurate. Severity low stands.

**Fix.** At minimum, document this residual explicitly in docs/security.md and the §15.29 narrative: the token cookie is readable by any same-host service the operator's browser contacts, so on multi-user machines the token is only as safe as every other local port. Preferably reduce the exposure: deliver the long-lived token via a mechanism the browser will not auto-replay to sibling ports (e.g. keep it in sessionStorage and pass it as a WebSocket subprotocol / one-time query on the /ws upgrade, and scope assets behind the same), so a passive sibling-port service never receives the standing credential.

### `WEBS-2` — CSP connect-src permits WebSocket to any host (`ws:`/`wss:`), broader than the same-origin claim in the code

**⚪ nit** · security · `serialnexusweb/src/server.rs:422` · verdict **CONFIRMED** (high confidence)

The asset CSP uses `connect-src 'self' ws: wss:`. The bare `ws:` and `wss:` scheme sources allow a script on the page to open a WebSocket to *any* host, which contradicts the adjacent comment stating 'the WebSocket is same-origin' (server.rs:418). Modern browsers already treat a same-origin WebSocket as matching `'self'`, so the broad schemes are unnecessary and weaken the connect-src backstop that the file's own rationale (server.rs:415-420 — token holder can now edit the graph, so a DOM-injection slip would be code execution) leans on.

**Failure scenario.** No injection vector exists today (all rendering is textContent), so this is defense-in-depth only. If a future DOM-injection regression were introduced in any renderer (app.js/graph.mjs/editor.mjs render daemon-supplied node names, refusal messages, and port descriptions), the current CSP would let injected script exfiltrate console bytes or the graph over a WebSocket to an attacker-controlled host, whereas `connect-src 'self'` would confine connections to the console's own origin — the containment the comment claims is already in force.

**Verification correction.** The claim holds as written; two clarifications. (1) The `'self'`-covers-same-origin-WS half is not a general assumption but is verified for *this* project's pinned browser (Chromium 151.0.7922.34 from `serialnexusweb/ui-tests`) on **both** deployment tiers: `ws://` from an `http://` page and `wss://` from an `https://` page are both allowed by `connect-src 'self'`, so dropping `ws: wss:` does not break either the plaintext or the TLS tier. The one residual risk the finder's fix carries is non-Chromium engines that predate the CSP3 `'self'`-matches-ws clause (older Safari) — the project states no support matrix beyond Chromium, and the finder's own hedge (listing the concrete `ws(s)://host` origin) covers it. (2) The containment `connect-src 'self'` buys is real but partial: CSP has no way to block a top-level `location = 'http://evil/?d=…'` (there is no shipped `navigate-to`), so the fix removes the silent, bidirectional, page-preserving exfil channel rather than all exfiltration. That keeps this squarely at nit/defense-in-depth, which is what the finder claimed.

**Fix.** Drop the standalone `ws:`/`wss:` sources: `connect-src 'self'`. If an older-browser compatibility hedge is desired, scope it to the origin (e.g. list the concrete ws(s)://host of `location.host`) rather than the whole scheme; but 'self' matches same-origin WS in the browsers the pinned Chromium suite targets.

---

## 2. Design deviations

Classified per the request: **(a) the code should change** — the design is right; **(b) the design text
should change** — the code is right for a good reason, and the reason is now recorded in
`docs/implementation-notes.md`; **(c) document defect** — v13 dropped or garbled text the code still
enforces, and the fix is to restore it.

The design-deviation sweep read v13 §§1–14 and §17 sentence by sentence against the code and reports
that **the classic dropped-text class is essentially empty this round** — the v12→v13 diff over those
sections is purely additive, so the alignment pass AGENTS.md §1 made a standing first step did its
job. Almost every specific claim checked out: `replay_ring = 65536`, `advertised_baud = 115200`, pty
0600/0660, rotation padding 3, arbitration `exclusive`, write mode `on-demand`, the leg's
idle-release and backoff, §7's status timestamp, the per-node `state` field lists, `-32601` for
`set-attribute`, `faces = target` serial refused, the picocom vocabulary including the corrected
`spchex`, batch rejection, `remove-node`'s cascade refusal, and `ports` passivity.

**Every design-relevant finding, with its class:**

| Finding | Class | What changes |
| --- | --- | --- |
| `CONC-2` `send`'s deadline bounds only the acquire | **(a)** | Code. §6 calls `send` "one atomic daemon-side operation … failing with the locked error at its deadline"; today it can hold the lock forever. |
| `CTRLW-1` a parked verb freezes its connection's streams | **(a)** | Code. §10 says a pipelined request is refused while "the connection — and the parked wait — survive intact"; the streaming half does not survive. |
| `WEB-3` session cookie not port-scoped | **(a)** | Code. §17 prescribes "run two for two daemons"; two consoles on one host log each other out. |
| `WEBUI-1` every refusal reported as a lock conflict | **(a)** | Code. §17 requires a LOCKED refusal to "show the holder by name"; it invents one. |
| `CORE-3` `restart_backoff_ms` unbounded | **(a)** | Code. Invariant 13 / §11 — the one timer with no structural range check, because it lives in a codec's opaque table. §8 already puts codec schema validation in the factory. |
| `CTL-2` CLI tap discards `gap_before` | **(a)** | Code. §10's offset contract has two discontinuity signals; the browser handles both, the CLI neither. |
| `RV-10` `exec` missing from `info.codecs` | **(a)** | Code. §7.6 packages exec "as an ordinary compiled-in codec"; §15.26 makes `info` the discovery surface. |
| `DEVR-2` console rail omits node status | **(a)** | Code. §17 enumerates four rail items; the code renders three, and the data is already in hand on every `subscribe` tick. |
| `UIR-1` no ANSI/terminal renderer | **(b)** *or* (a) | §17 promises "a vendored, permissively licensed terminal renderer (or a minimal ANSI subset)". Either implement the subset or record the omission — today they disagree silently. |
| `DEVR-5` taps do not close on blur | **(b)** *or* (a) | §17 says "taps open lazily per viewed console and close on blur after a grace interval". The shipped policy (one tap, closed on switch) is defensible; pick one and write it down. |
| `DEVR-3` §7.7 existing-terminal | **(c)** | Design text. Written in the present tense as a shipped node type and absent from §14's deferred list, while `NodeConfig` has no such variant — unlike §7.1's serial output leg and §7.5's standalone re-muxer, which the design marks deferred consistently. |
| `DOCE-1` §17 keys browser history by `epoch` | **(c)** | Design text. Contradicts §15.38, §15.32 and the code — and keying by epoch would *destroy* scrollback on every hub rebuild, the exact failure §15.38 fixed. |
| `RV-6` `<socket>.state.toml` | **(c)** | Design text + two rustdoc comments. The path is `<socket-stem>.state.toml`. |

The **(b)** and **(c)** items are the justified deviations in the sense the request asked about — the
code is right, or defensibly right, and the document is what should move. The *refutations* that exist
for the same reason (a finding that died because the code's behaviour is deliberate) are written into
`docs/implementation-notes.md` **§3.20** as part of this review, so a future session finds them beside
§3.1–§3.19 rather than re-deriving them.

### `CONC-2` — `send`'s timeout bounds only the acquire, so a backpressured send hangs forever holding the endpoint's write lock

**🟡 medium** · design-deviation · `nexus-daemon/src/daemon.rs:1577` · design design §6 ("it performs acquire-with-timeout, write, and release as one atomic daemon-side operation … failing with the locked error at its deadline"); docs/rpc/arbitration.md:181 ("give up with the locked error after this long") · verdict **CONFIRMED** (high confidence)

`Daemon::send` applies `timeout_ms` to the acquisition only (`wait_for_grant` with the deadline built at daemon.rs:1551) and then does an unbounded `sender.send(Chunk::from(bytes)).await` at daemon.rs:1577. If the endpoint's targetward channel is full, that await never returns, and the `TransientOrigin` registered at daemon.rs:1535 keeps the exclusive write lock for the whole time — so a verb the design calls transient and atomic becomes an unbounded lock holder. `serialnexusctl` applies no client-side timeout, so the CLI hangs with it.

**Failure scenario.** Serial node `usb0` whose device is absent or flow-control stalled, with ≥256 chunks already queued targetward (256 prior `send`s, or a console typing while the device is out). `serialnexusctl send usb0 --line hello --timeout-ms 3000` never returns. While it hangs, `state` shows `holder: "send"`, and every other origin is locked out: `lock con` → `-32003 {"held_by":"send"} endpoint is locked by send`. A script that runs `send` without an external `timeout` wedges the endpoint until it is killed; nothing on the daemon side ever releases it.

**Verification correction.** `Daemon::send` bounds only the *acquire* with `timeout_ms`; the delivery `sender.send(Chunk::from(bytes)).await` (nexus-daemon/src/daemon.rs:1577) is unbounded, so when the endpoint's single 256-deep targetward channel (`runtime::CHANNEL_CAP = 256`) is full the verb blocks indefinitely while its `TransientOrigin` (registered daemon.rs:1535) holds the endpoint's exclusive write lock — `state` reports `holder: "send"` and every other origin gets `-32003 held_by:"send"`. `serialnexusctl` adds no client-side read deadline, so the CLI hangs (measured: `--timeout-ms 500` still running at 10 s, exit 124). Two corrections to the finder's framing. (1) The premise is broader and worse than "device absent": a **present, `status: "active"`, `open: true`** serial node whose target has merely stopped reading — a flow-control stall, the exact condition `nexus-sim pty --stall` and `p6_head_of_line.rs` model — wedges it identically, and nothing in `state` flags it except the phantom holder. (2) "nothing on the daemon side ever releases it" is too strong: teardown/`load --replace` closes the sender an…

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Repro A (device absent, the finder's scenario). Daemon on a short socket, graph = pty `con` + serial `usb0` with an absent device path. A python client on one Unix-socket connection issues sends with `timeout_ms: 3000`:

  FILL: request 256 BLOCKED (no response in 2s); 256 succeeded before it
  state right after the block: {"arbitration":"exclusive","holder":"send","origins":[{"holds_lock":false,"origin":"con",...},{"holds_lock":true,"origin":"send","write_mode":"on-demand"}],"waiters":[]}
  t+1s holder='send'  t+3s holder='send'  t+6s holder='send'  t+10s holder='send'
  lock con -> {"code": -32003, "message": "endpoint is locked by send", "data": {"held_by": "send"}}
  after dropping the blocked connection: holder=None

Exactly `CHANNEL_CAP` sends land; the 257th blocks past its 3 s dead
… (truncated)
```

</details>

**Fix.** Carry the same deadline into the delivery: `tokio::time::timeout_at(deadline, sender.send(chunk))`, returning the locked/backpressured error on expiry. `mpsc::Sender::send` is cancel-safe, so a timed-out send delivers nothing and the byte count stays exact. Either way the `TransientOrigin` guard then unregisters on the normal path, so the endpoint is never held past the deadline the operator asked for. If an unbounded write is deliberate, the deadline's scope must be stated in docs/rpc/arbitration.md and design §6 — today both read as bounding the whole operation.

### `CTRLW-1` — A parked waiting verb freezes that connection's `tap.data` and `subscribe` notification lanes until it resolves

**🟡 medium** · design-deviation · `nexus-daemon/src/control.rs:300` · design §10 ("a request pipelined behind a parked wait is refused with its own error while the connection — and the parked wait — survive intact"; "`tap.open` … streams its hostward bytes as id-less `tap.data` notifications on that connection"; "Lock transitions … are additionally emitted as immediate id-less notifications to subscribers"); §17 (one daemon connection per browser carries `subscribe`, every tap and `send`) · verdict **CONFIRMED** (high confidence)

While a waiting verb is in flight, `serve_connection` leaves the outer three-arm `select!` (control.rs:169-432) and enters an inner two-arm loop (control.rs:300-355) that polls only `dispatch` and `lines.next_line()`. The `notes.recv()` arm (control.rs:377) and the `tap_rx.recv()` arm (control.rs:396) are not polled at all for the whole duration of the wait, so the connection stops writing every notification it is subscribed to. The connection therefore does not "survive intact" in any functional sense: it survives as a request/response channel and dies as a stream. `lock --wait` passes no deadline (daemon.rs:1432 `wait_for_grant(cell, id, None)`), so the freeze is unbounded.

**Failure scenario.** Connection B has `tap.open` on `usb0` (a live console) and issues `lock cb --wait` while origin `ca` holds the endpoint. From that instant B receives nothing — no `tap.data`, no `lock` transition notifications, no state snapshots — until someone unlocks `ca`. Beyond `TAP_QUEUE_CAP` = 128 queued chunks (tap.rs:49) the hub starts dropping them at tap.rs:383-386, so a firehose endpoint loses real bytes to a stall that the client did not cause and cannot see coming. For the shipped web console (one daemon connection per WebSocket, bridge.rs:96) the same thing happens on every contended `send`: `DEFAULT_SEND_TIMEOUT_MS` = 2000 (daemon.rs:46), so pressing Enter on a locked console blacks the tab's terminal out for two seconds; `lock` is on the bridge allowlist (bridge.rs:80), so a `wait: true` from a page blacks it out indefinitely.

**Verification correction.** Accurate as filed, with two refinements. (1) The design grounding is §10's "a tap streams its hostward bytes as id-less `tap.data` notifications on that connection" plus §17/`bridge.rs`'s mandated one-daemon-connection-per-browser, rather than the "the connection ... survives intact" clause, which in context is about the pipelined request specifically. (2) The mechanism is broader than lock waits: the inner loop freezes the notification lanes for *any* dispatch future that is not ready on the first poll, which includes `send`'s targetward `sender.send(chunk).await` under backpressure, not only `wait_for_grant`. Also worth stating precisely: `subscribe` notifications are only *delayed* (the broadcast receiver buffers, dropping oldest with `Lagged`, and snapshots are cumulative), whereas `tap.data` bytes are *really lost* once the 128-slot per-connection tap queue fills — measured at 11,188,225 bytes in a 6 s wait, counted in `state` as the tap's `dropped`.

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Setup (Linux, HEAD cfb2187, prebuilt target/debug binaries):

  RUN=$(mktemp -d /tmp/snxver.XXXXXX)
  ./target/debug/nexus-sim pty --link $RUN/dev --source --bytes 256MiB --rate 2000000 \
      --timeout-ms 600000 --hold-ms 60000 &
  XDG_RUNTIME_DIR=$RUN ./target/debug/serialnexusd &
  # graph.toml: serial usb0 (device=$RUN/dev, replay_ring=65536) fanned out to
  # pty ca and pty cb (hostward_buffer = 8192 each, per AGENTS §5)
  XDG_RUNTIME_DIR=$RUN ./target/debug/serialnexusctl load $RUN/graph.toml   # all three active

Driver: /tmp/claude-1000/-home-pwnall-workspace-serial-nexus/6784367f-5731-471a-b43c-de9e6ca9c5ce/scratchpad/ctrlw/repro2.py
(conn A: lock ca, subscribe, tap.open usb0 | conn B: subscribe, tap.open usb0, then lock cb wait:true)

  PHASE 1 (2s, nothing parked)
    A: tap.da
… (truncated)
```

</details>

**Fix.** Give the inner wait loop the same notification lanes as the outer one — add `note = notes.recv(), if subscribed.counted` and `tap = tap_rx.recv()` arms that write out and then `continue` the inner loop — or restructure `serve_connection` around a single four-lane `select!` in which the dispatch future is an optional arm, so there is one place that drains notifications and it cannot be bypassed. A guard in `nexus-itest` (a tap plus a parked `lock --wait` on one connection, asserting `tap.data` keeps arriving) would pin it.

### `WEB-3` — The session cookie is not port-scoped, so a second web console evicts the first's session

**🟡 medium** · design-deviation · `serialnexusweb/src/server.rs:461` · design §17 "It is single-daemon per instance in v1 (run two for two daemons)"; §15.32 "the server binds a stable default port" · verdict **CONFIRMED** (high confidence)

`session_cookie` always emits the name `nexus_session` with `Path=/` and a host-only domain. Cookies are not isolated by port (RFC 6265 §8.5), so two `serialnexusweb` instances on the same host — the arrangement §17 explicitly prescribes ("single-daemon per instance in v1 (run two for two daemons)") — collide on the cookie-store key (name, domain, path) and the second bootstrap silently logs the operator out of the first. `Request::cookie` compounds it by returning only the *first* `nexus_session` value in the header, so a shadowing cookie cannot be recovered from either.

**Failure scenario.** Operator runs two daemons and two consoles (:8080 and :8081) as §17 instructs, and opens both bootstrap URLs in one browser. Opening the second console silently invalidates the first: every request from the first tab now returns 401 "missing or invalid session token — open the bootstrap URL", and re-opening its bootstrap URL breaks the second — an unbreakable ping-pong with no diagnostic naming the cause. Secondary variant: any page the operator visits on another localhost port (same site, so `SameSite=Strict` never fires) can run `document.cookie = "nexus_session=x; path=/ws"`; the longer path sorts first, `cookie()` takes it, and the console's WebSocket upgrade 401s until the operator clears cookies.

**Verification correction.** The finder had the mechanism and both variants right. Two refinements from live measurement, both of which sharpen rather than soften it:

(1) The eviction is not instantaneous from the operator's seat. A tab that is already loaded keeps streaming over its *established* WebSocket, because the upgrade was authorized before the cookie was replaced. The break surfaces on the next reload or on any WS drop — and the client's only recovery path is a reload (`app.js:120` sets the status text to "disconnected — reload to reconnect"; there is no reconnect logic), which then returns 401. So the state is "works until it doesn't, then is unrecoverable without breaking the other console".

(2) In the path-shadowing variant the damage is *narrower and therefore better hidden* than "the console 401s". Only `/ws` carries the planted longer-path cookie, so `GET /app.js` (path `/`) still returns 200 and the console renders normally — it simply never connects, and reloading does not clear it. Measured: after a sibling-port page runs `document.cookie = "nexus_session=bogus; path=/ws"`, `A /ws -> 401` wh…

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Two servers, distinct fixed tokens, no daemon needed (the HTTP token gate runs before any RPC):

  ./serialnexusweb --bind 127.0.0.1:18080 --token 0123...cdef &
  ./serialnexusweb --bind 127.0.0.1:18081 --token fedc...3210 &

A. Real Chromium, the repo's own pinned build (`serialnexusweb/ui-tests/node_modules/playwright-core`), ONE browser context = one cookie jar:

  cookies after bootstrap A: [{"n":"nexus_session","v":"01234567","d":"127.0.0.1","p":"/"}]
  A /app.js after A bootstrap -> 200
  cookies after bootstrap B: [{"n":"nexus_session","v":"fedcba98","d":"127.0.0.1","p":"/"}]   <-- replaced, not added
  A /app.js after B bootstrap -> 401 missing or invalid session token — open the bootstrap URL (§
  B /app.js after B bootstrap -> 200
  tab A reload -> 401

B. The ping-pong, same con
… (truncated)
```

</details>

**Fix.** Scope the cookie name to the listener, e.g. `nexus_session_<bound port>` (the port is already known at `run`), and/or make `Request::cookie` return every value for the name so the token is accepted if any matches. Add an itest that bootstraps two servers with different tokens against one cookie store and asserts both stay authorized.

### `WEBUI-1` — Every `send` refusal is reported to the operator as a lock conflict with a steal offer, and the steal retry's failure is discarded silently

**🟡 medium** · design-deviation · `serialnexusweb/src/assets/app.js:488` · design §17 interface contract: "The bottom-right single-line input drives the `send` verb; a LOCKED refusal shows the holder by name with an explicit steal affordance — never an automatic steal." · verdict **CONFIRMED** (high confidence)

`rpc()` collapses every error envelope to `null` (`app.js:218`, `cb(full ? msg : (msg.error ? null : msg.result))`), so `sendForm.onsubmit` cannot distinguish `app_errors::LOCKED` from any other refusal. It treats *all* of them as a lock conflict, names a holder it does not have (`holder = ep && ep.lock ? ep.lock.holder : "someone"`, line 491), and offers to steal a lock that may not exist. If the operator accepts, the retry's result is not inspected at all (line 493) — no message, no marker, and the typed line was already cleared at line 486. The `rpcFull` helper that exists precisely to hand back the daemon's own words (lines 92-104) is used only by the editor page.

**Failure scenario.** Reproduced live at HEAD `cfb2187`, entirely inside the web console: the operator selects console `usb0`, switches to the editor page in the same tab, removes `usb0` (cascading its log edge), then switches back to the console view. `selected` is still `"usb0"` and `#sendline` is still enabled — nothing clears them, and `onTapClosed` (line 420) only appends a marker. The operator types `reboot` and presses send. The daemon answers `{"code":-32602,"message":"\"usb0\" is not a host-facing endpoint with a write lock"}`. The browser instead pops `confirm("usb0 is locked by someone. Steal the lock and send?")`. The operator, believing a colleague holds the port, clicks OK; the steal retry gets the same `-32602` and the client shows nothing whatsoever. The line is gone from the input, was never delivered, and the operator has been told a false story about why. The same applies to any non-LOCKED `send` failure (endpoint torn down mid-send, `load --replace` racing the click); and where a holder genuinely exists but the failure was not a lock conflict, the accepted prompt fires a real steal — revoking the holder's grant and purging its pre-grant backlog — for a reason that was never contention.

**Verification correction.** The finding is correct except for its final clause. Drop this sentence: "where a holder genuinely exists but the failure was not a lock conflict, the accepted prompt fires a real steal — revoking the holder's grant and purging its pre-grant backlog — for a reason that was never contention." I traced it and it does not hold. Both reachable non-LOCKED `send` refusals are returned from the pre-flight `self.state.with(...)` block at `nexus-daemon/src/daemon.rs:1500-1530` — the endpoint-unknown `-32602` at 1501-1506 and the dead-path `-32602` at 1523-1528 — and that block runs *before* the `if p.steal { let _ = cell.with_mut(|g| g.steal(id)); }` at line 1543. So the steal retry fails at the identical check and no steal is ever performed; my live transcript shows the `steal:true` retry returning the same `-32602`. The only arm that would reach the real `steal()` after a non-LOCKED outcome is `WaitOutcome::ReadOnly` (daemon.rs:1560), and that is an internal-invariant violation the code itself treats as unreachable — the transient origin is registered `WriteMode::OnDemand` at line 1535. Ever…

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Real headless Chromium (the tree's pinned Playwright, imported as a library from
scratchpad; the repo was not modified) against a live daemon + serialnexusweb at HEAD
cfb2187. Box load average 1.21 throughout.

Setup:
  export XDG_RUNTIME_DIR=$(mktemp -d /tmp/snxv2.XXXXXX)
  ./target/debug/nexus-sim pty --echo --link $RTD/dev0 --timeout-ms 600000 --hold-ms 600000 &
  ./target/debug/serialnexusd &
  # graph.toml: one serial node "usb0", arbitration = "free-for-all", device = $RTD/dev0
  ./target/debug/serialnexusctl --socket $RTD/serialnexusd.sock load $RTD/graph.toml
  ./target/debug/serialnexusweb --bind 127.0.0.1:0 --socket $RTD/serialnexusd.sock \
      --token uitesttoken0123456789abcdef &

Browser script: goto the bootstrap URL, click the usb0 console, send one healthy line,
then `ser
… (truncated)
```

</details>

**Fix.** Route the console's `send` through `rpcFull` and branch on the error code: offer the steal affordance only for `app_errors::LOCKED` (naming the holder from the error/state), and for every other refusal `appendMarker` the daemon's `error.message` verbatim — the same "report the daemon's own words" stance `editor.mjs:138-162` already takes. Report the steal retry's outcome too, and restore the typed line to `#sendline` when a send is refused rather than discarding it. Also clear `selected`/disable `#sendline` when the selected endpoint leaves `state` or its tap is closed.

*Independently reported a second time as `WEB-1b`.*

### `CORE-3` — The exec codec's `restart_backoff_ms` is the one timer with no structural range check, so a crashed child can be configured never to restart

**🔵 low** · design-deviation · `nexus-daemon/src/nodes/exec.rs:68` · design §11 ("every numeric attribute carries a stated maximum and is range-checked structurally"); AGENTS invariant 13 ("A new numeric knob gets a range here on the day it is added"); §7.6 · verdict **CONFIRMED** (high confidence)

`ExecAttributes::restart_backoff_ms` (`exec.rs:68`) is a `u64` millisecond timer that `parse_attributes` (`exec.rs:76-83`) validates only for deserializability and a non-empty `argv`. Every other millisecond timer in the schema is capped at `MAX_TIMER_MS` (one hour) by `GraphConfig::validate` (`config.rs:320-326`), with the rationale written into `config.rs:67-72`: "a leg that waits longer than that to retry … is indistinguishable from a dead one, and the operator who typed an extra three digits learns it at load rather than by watching a console that never comes back." That reasoning transfers verbatim to the exec restart backoff and is not applied.

**Failure scenario.** An operator writes `restart_backoff_ms = 6000000` intending six seconds and slipping three digits (or `86400000` for "a day", the exact shape the leg cap was written against). The config loads clean. The first time the exec child crashes — which is the one event the backoff exists for — the node reports `faulted … retrying` and then never respawns for the rest of the daemon's life; every channel behind that codec goes permanently silent with a status that says it is retrying. Nothing at load time named the mistake, which is precisely what the sibling timers' cap prevents.

**Verification correction.** The finding is accurate as written and every line reference in it checks out; two refinements. (1) The finder's `u64::MAX`/tokio-saturation discussion is moot because that value is unreachable: both intake paths refuse it. TOML integers are i64-bounded, so `load` rejects `18446744073709551615` with "u64 value was too large", and the JSON-RPC `add-node` path round-trips through toml and rejects it identically (`-32602 invalid node config: u64 value was too large`). The reachable range is `0..=i64::MAX` ms (~292 million years), which is far past absurd and needs no saturation argument — the defect is fully demonstrated at `86400000`, the exact value the leg's `reconnect_initial_ms` refuses. No panic and no daemon crash occurs at any reachable value. (2) The unbounded wait governs the *real* crash path, not just the spawn-failure path the finder's evidence traces: `PumpEnd::ChildDied` (exec.rs:455-465) bumps `restart_count`, sets `Faulted { "child exited; restarting (count N)" }` and calls the same `backoff.sleep()`. Verified live with `argv = ["/bin/true"]`. The suggested fix's atomici…

**Fix.** Range-check it in `exec::parse_attributes` against `nexus_core::config::MAX_TIMER_MS` (the same constant, so the two timer families cannot drift), returning the same structural error string shape — `parse_attributes` is already called from both `load` (`daemon.rs:544`) and `add_node` (`daemon.rs:725`) before anything is created, so the atomicity guarantee comes for free. Re-check the other codec-owned numerics for the same gap when new ones are added.

*Independently reported a second time as `EXEC-2`.*

### `CTL-2` — `serialnexusctl tap` discards `gap_before`, so a capture is silently holed

**🔵 low** · design-deviation · `serialnexusctl/src/main.rs:737` · design docs/rpc/observation.md:489 — "A client splicing by offset must treat a non-zero `gap_before` as a discontinuity rather than concatenating"; §5 all-loss-is-visible · verdict **CONFIRMED** (high confidence)

`tap_stream` reads only `params.data` from each `tap.data` notification and concatenates the decoded bytes to stdout. It never inspects `gap_before` (the producer→hub feed loss the offsets deliberately cannot express) nor compares consecutive `offset` values, so both of the protocol's discontinuity signals are dropped and the operator's capture is a holed stream with no indication anywhere in the output.

**Failure scenario.** Operator captures a busy console: `serialnexusctl tap console --replay > incident.bin`. The endpoint's bounded producer→hub feed sheds a chunk under load; the daemon marks the next notification `gap_before: 4096`. `incident.bin` gets the surrounding bytes concatenated across the hole with no marker on stdout or stderr, and the operator analyses a stream that never existed. The sibling client renders `— 4096 bytes lost (daemon feed) —` for the same notification.

**Verification correction.** The mechanism is exactly as filed; only the design-ref framing needs sharpening. `serialnexusctl tap` is *not* a client "splicing by offset", so it does not violate the clause the finder quotes (`docs/rpc/observation.md:489`) on its own terms — it never splices, it concatenates a live stream. The accurate statement is: the CLI is the one first-party consumer that discards *both* discontinuity signals the protocol carries — `gap_before` (added by the TAP-1b remediation precisely so "the hole is visible, not merely tallied", `nexus-daemon/src/tap.rs:159`) and the `offset` jump a per-tap queue drop leaves (`tap.rs:375-390`, which the daemon's own comment at `tap.rs:415-417` calls "a gap it can see, never a silent shift") — so a `serialnexusctl tap … > incident.bin` capture is holed with nothing on stdout or stderr for that invocation. Mitigating, and worth stating with the finding: the loss is still *counted* and reachable out-of-band — `state` renders endpoint `feed_dropped` even with no tap open (`daemon.rs:1184/1192`) and per-tap `dropped` — and `docs/rpc/observation.md:581-584` docu…

**Fix.** On a non-zero `gap_before`, write a one-line notice to stderr (`tap gap: N bytes lost before offset X`) — stdout must stay a clean byte stream, and stderr already carries the `tap opened:` line. Optionally also warn when `offset` does not equal the previous `offset + len`, which is how per-tap queue drops surface.

### `DEVR-2` — §17's console rail omits node status: the left rail shows address, lock holder and waiter count, but never the node's active/waiting/faulted state

**🔵 low** · design-deviation · `serialnexusweb/src/assets/app.js:257` · design §17 ("The interface contract") · verdict **CONFIRMED** (high confidence)

§17 specifies the left rail as "display address, node status, lock holder and waiter count, live via `subscribe`", and says in the same paragraph that "The layout is the contract". `renderConsoles()` renders only the display address (`.cname`), an optional lock badge and an optional waiter count. `endpointsFromState` (app.js:246-256) already carries the whole node object — which holds `status`/`reason`/`since_unix_ms` — as `ep.node`, and no code path ever reads it; grep for `.node` in app.js returns nothing.

**Failure scenario.** An operator has the console view open with `usb0` selected. The FTDI adapter is unplugged: the daemon flips the serial node to `faulted` and the hostward stream stops. The rail entry for `usb0` is byte-identical to before — name, no lock badge, no waiters — and `#pane-head` shows only title/lock/drops/storage. The operator sees a console that has gone quiet and cannot distinguish "the device is silent" from "the device is gone" without leaving the console view for the graph page. The graph page (`graph.mjs`, `.gstatus`) is the only place any status is rendered, and `graph-editor.spec.mjs:76-77` is the only spec that asserts a status indicator anywhere.

**Verification correction.** The deviation is real and the mechanism is exactly as filed, with two refinements. (1) The scenario's status word is wrong for the headline case: an *unplug* (device path gone) puts a serial node in `waiting` with `reason = "device … lost"`, not `faulted` — §7.1 faulted-and-wait maps absence to `waiting` and any other open error (e.g. EACCES) to `faulted` (`nexus-daemon/src/nodes/serial.rs:141-160`). Both are equally invisible in the rail, so the consequence stands for either. (2) The stronger statement of the defect, which I measured rather than inferred: the three fields `renderConsoles()` actually reads — `ep.display`, `ep.lock.holder`, `ep.lock.waiters.length` — are *byte-identical* across the transition. The node keeps its `lock` object while `waiting`/`faulted`, so the console entry does not even disappear, and no `tap.closed` fires (the endpoint's hub survives), so the pane gets no marker either. The rail's only live signal on a lost device is nothing at all.

**Fix.** (a) code should change — the design is right and the data is already in hand. Render `ep.node.status` as a per-console indicator in `renderConsoles()` (the graph page's `.gstatus` glyph/class vocabulary already exists and would keep the two views consistent), with the `reason` as the element's `title`. Add a Playwright spec beside `graph-editor.spec.mjs`'s existing scripted-fault case asserting the rail entry gains the `waiting`/`faulted` class when the same fault fires.

### `DEVR-3` — §7.7's existing-terminal node is written as a shipped node type and is absent from §14's deferred list, but no such node kind exists

**🔵 low** · design-deviation · `nexus-core/src/config.rs:522` · design §7.7, §14, §3 ("Facing"), §12 · verdict **CONFIRMED** (high confidence)

§7.7 describes the existing-terminal node in the present tense as one of the specified node types ("Connects to a pre-existing PTY or tty device by path … Otherwise behaves as a boundary with the standard policies"), §3 lists "existing-PTY connectors" among the dual-role node types that carry a `faces` attribute, and §12 says "Existing-terminal nodes (§7.7) have no hardware identity and pass through as path identities" — yet §14's deferred list does not mention it, and `NodeConfig` has no such variant. AGENTS.md §2 records it under "Deferred / not implemented on purpose", so the code is right and the design has never recorded the deferral. This contrasts with the treatment §7.1 gives the serial output leg, which the design explicitly says "is refused as not implemented at load, a structural error naming the deferral (§14)".

**Failure scenario.** An operator reads §7.7, adds `[[node]] type = "existing-terminal" name = "qemu" path = "/dev/pts/9" faces = "host"` to an otherwise-valid config and runs `serialnexusctl load prod.toml`. Live-reproduced at HEAD: the CLI fails with a *TOML parse* error — `unknown variant \`existing-terminal\`, expected one of \`serial\`, \`pty\`, \`log\`, \`codec\`, \`leg\`, \`map\`` — which, being a deserialization failure, rejects the entire file (the CFG-1 failure shape §15.34/notes §3.15 record) rather than naming a deferral, and there is no §14 entry to send the reader to. Over the raw socket the same config yields `-32602 invalid config: unknown variant …`, not the `-32002` structural error with the deferral named that §7.1's analogous deferral produces.

**Verification correction.** The substance holds, but one clause of the failure scenario should not survive: "rejects the entire file" is not part of the defect. The §7.1 deferral path also refuses the whole load and creates nothing, and I verified that `load --replace` with an `existing-terminal` node leaves the running graph intact (params deserialization fails at the RPC boundary, before any teardown) — so there is no atomicity or data-plane consequence. The defect is exactly and only documentary plus error-message quality: design §7.7 specifies the existing-terminal node in the present tense (as do §3's "existing-PTY connectors … carry a `faces` configuration attribute" and §12's "Existing-terminal nodes (§7.7) have no hardware identity and pass through as path identities"), §14 — whose stated purpose is "Recorded so deferral stays deliberate" — does not list it, and `NodeConfig` has no such variant, so the operator's only signal is a serde `unknown variant` error (`-32602`) naming no deferral, versus the `-32002` structural error the two other unimplemented-but-specified capabilities produce. The design app…

**Fix.** (c) document defect — restore/insert the missing text; the code is right. Give §7.7 the same one-clause treatment §7.1 gives the serial output leg ("remains in the model but is not implemented; a configuration naming it is refused, §14") and add it to §14's deferred list beside the serial output leg. §3's and §12's present-tense references should gain the same qualifier. Optionally (b): if the design wants the §7.1-style *structural* refusal naming the deferral rather than a serde parse error, that is a small code change — but the deferral must be recorded either way.

### `DEVR-5` — §17's "taps … close on blur after a grace interval" is unimplemented: a backgrounded console tab keeps its tap open and the daemon keeps feeding it

**🔵 low** · design-deviation · `serialnexusweb/src/assets/app.js:479` · design §17 ("Implementation stance") · verdict **CONFIRMED** (high confidence)

§17 states "Taps open lazily per viewed console and close on blur after a grace interval". The client opens lazily (`selectConsole` → `tap.open`) but the *only* close paths are selecting a different console (app.js:349), a losing re-entrant selection (app.js:371), a daemon-side `tap.closed` (app.js:418), and the WebSocket dying. The one visibility hook that exists — `visibilitychange` at app.js:479 — flushes the OPFS save and nothing else; no blur handler and no grace timer exist anywhere in the assets.

**Failure scenario.** An operator selects a high-rate console, then switches browser tabs (or minimises) and leaves the machine for the day. The tap stays registered on the daemon's hub indefinitely: the endpoint keeps mirroring every hostward chunk into that tap's bounded queue, the connection task keeps base64-encoding and writing `tap.data` frames over the WebSocket, and the hidden page keeps decoding, rendering and (per `scheduleSave`) writing history to OPFS once a second. Nothing releases until the tab is closed or another console is picked — precisely the cost the blur-close clause exists to avoid.

**Verification correction.** The clause is unimplemented in a broader sense than "blur": the client never releases a tap for *any* reason short of selecting another console, a daemon-side `tap.closed`, or the link dropping. Two reachable cases, not one. (a) Backgrounding the tab — no `blur` listener, no grace timer, and the single `visibilitychange` listener (`serialnexusweb/src/assets/app.js:479`) only calls `flushSave()`. (b) Switching to the graph or editor view (`renderView`, app.js:153-158) hides the console pane as one unit and leaves `currentTap` untouched, so the operator is not viewing *any* console while its tap keeps streaming — that case fails even the charitable reading of "close on blur" as "the console pane lost focus". One correction to the finder's mechanism: "the endpoint keeps mirroring every hostward chunk" is not attributable to the stale tap. `TapHub::refresh_active` (nexus-daemon/src/tap.rs:498-502) holds `active` true whenever a ring is configured, and the ring is default-on at 64 KiB on every host-facing endpoint (invariant 9), so the producer→hub mirror runs with zero taps too. The stal…

**Fix.** (b) design text should change, or (a) implement the clause — pick one and record it. The shipped policy (at most one tap, closed on switch) is defensible and simpler; if it is the intended policy, §17's sentence should read "Taps open lazily per viewed console and close when another console is selected or the link drops". If the blur-close is genuinely wanted, add a `visibilitychange`-driven grace timer that issues `tap.close` after N seconds hidden and re-opens with `--replay` on return (the `from_offset`/`epoch` splice already makes the re-open lossless), plus a `@slow` Playwright spec asserting the daemon's `state.endpoints[].taps` drops to 0 while the page is hidden.

### `RV-10` — exec is a usable codec name that info does not report and the unknown-codec error does not list

**🔵 low** · design-deviation · `nexus-daemon/src/registry.rs:40` · design 7.6, 8, 15.26 · verdict **CONFIRMED** (high confidence)

Design 7.6 packages the exec codec 'as an ordinary compiled-in codec (8)' and 15.26 makes info the discovery surface. exec is not in the Registry - it is a RESERVED_NAME special-cased at daemon.rs:543 and nodes/mod.rs:57 - so neither info.codecs nor the unknown-codec error's data.available knows about it.

**Failure scenario.** A tool reading info.codecs concludes the escape hatch is unavailable. An operator who typos codec = "exec" as "exe" gets 'available: ["reference"]' - a list that omits the very name they wanted, while docs/rpc/configuration.md promises that list 'names the codecs that would have worked'.

**Verification correction.** `exec` is a usable codec name that appears in neither `info.codecs` nor an unknown-codec error's `data.available`, and two normative doc sentences state the opposite. The `info` half alone is NOT a design deviation: design §15.26 says the daemon "reports its **registered** codec names", `exec` is deliberately not registered (registry.rs:19-22 + `RESERVED_NAMES`), and `docs/rpc/observation.md:285` explicitly documents the exclusion ("the `exec` child-process codec is always available and is not listed here"). What is actually defective is narrower and sits in the surrounding prose and in the CLI/error surface: (1) `docs/rpc/configuration.md:60-62` promises `data.available` is "the list of codecs this daemon *does* have … so a misconfiguration names the codecs that would have worked" — false for `exec`, and unlike the `info` table it carries no caveat; (2) `docs/rpc/observation.md:262` says `info` reports "the names of every codec it can instantiate" — contradicting its own field-table footnote 23 lines later. (3) The sharpest reachable form, documented nowhere: in the supported minima…

**Fix.** Union RESERVED_NAMES into Registry::codec_names() (or the info assembler) and into the unknown-codec error's available list.

### `UIR-1` — No terminal renderer and no ANSI handling: escape sequences reach the DOM as literal text

**🔵 low** · design-deviation · `serialnexusweb/src/assets/app.js:441` · design §17 Implementation stance: "a vendored, permissively licensed terminal renderer (or a minimal ANSI subset) keeps console escape sequences legible"; plan §11.4: "the renderer vendored and permissive" · verdict **CONFIRMED** (high confidence)

`appendText` appends decoded bytes straight into `<pre id="term">` as a Text node. Neither the "vendored, permissively licensed terminal renderer" nor the "minimal ANSI subset" §17 offers as the alternative exists, so CSI sequences from any ordinary device console are rendered as visible litter rather than being interpreted or stripped. This deviation is not recorded in docs/implementation-notes.md.

**Failure scenario.** An operator opens the web console on a Linux target or a U-Boot prompt. Every colourised line arrives as `[0;32m…[0m`, a `\r`-based progress bar renders as one unreadable run-on line, and a screen-clear sequence does nothing. The console is materially harder to read than picocom, which is the tool §7.8 already borrows its byte mappings from.

**Verification correction.** Accurate as filed, with three refinements. (1) The exact line is `serialnexusweb/src/assets/app.js:443` (`termEl.appendChild(document.createTextNode(s))`) inside `appendText` at :441. (2) This is best framed as an *undocumented design deviation*, not a functional bug: design §17's own "The layout is the contract; the rendering is presentation and iterates freely under §15.16's rules" gives the frontend latitude in how it renders, and plan §11.4's validation clause explicitly says "rendering itself stays presentation per §15.16" — so the defect is that the §17 Implementation-stance clause ("a vendored, permissively licensed terminal renderer (or a minimal ANSI subset) keeps console escape sequences legible") is the one clause of that paragraph not implemented in any form and not recorded as a deviation in `docs/implementation-notes.md` §3.x, while every sibling clause (embedded assets, no bundler, permissive HTTP/WS crates, lazy taps, base64 tap.data, Playwright CI) is. (3) The CR detail is right but worth stating precisely: with `white-space: pre-wrap` Chromium neither overwrites nor…

**Fix.** Either implement the minimal SGR subset §17 sanctions (parse CSI m into spans, honour CR/BS/screen-clear) or record the omission as a deviation in docs/implementation-notes.md §3.x with its rationale, so the design and the code stop disagreeing silently.

---

## 3. Testing coverage

The harness is high quality — see §6c. What follows are holes, and two guards that provably cannot
fail, which for a project whose method is "every phase ends with an adversarial audit" matters more
than its severity label suggests.

### `ITEST-1` — `the_port_is_still_exclusive_while_the_node_holds_it` passes identically without TIOCEXCL — the anti-regression guard cannot fail

**🟡 medium** · testing · `nexus-itest/tests/p11_replace_atomicity.rs:220` · design §7.1 (TIOCEXCL), §15.38, AGENTS.md §2 "the fourth pins that the port is still exclusive while a live node holds it, so a 'fix' that simply stopped taking TIOCEXCL cannot pass" · verdict **CONFIRMED** (high confidence)

The test's only assertion is `assert_eq!(verdict["pass"], false, …)` on a `nexus-sim client --expect echo` verdict. `pass:false` is also what the sim reports when the open *succeeds* but a concurrent non-exclusive reader consumes the echo — which is exactly what the daemon's serial reader does (invariant 2: it parks in blocking `poll(2)` and reads everything hostward). So a regression that stopped calling `set_exclusive(fd, true)` would leave this test green.

**Failure scenario.** Someone "simplifies" `SerialNode` by dropping the `TIOCEXCL` ioctl entirely (a plausible follow-on to §15.38's "exclusivity is a claim the node made" reasoning). `a_replace_keeps_its_serial_port`, `a_write_accepted_right_after_a_replace_reaches_the_device` and `remove_then_add_keeps_the_same_port_openable` all still pass (they get *better*). `the_port_is_still_exclusive_while_the_node_holds_it` also still passes: the stray `nexus-sim client` now opens the pts successfully, writes 16 bytes, the daemon's reader eats the echo, the client reads 0 and reports `{"pass":false,"received":0,"timed_out":true}`. §7.1's exclusivity is silently retired with a full green suite.

**Verification correction.** The mechanism is real but the finder's absolute framing ("the guard cannot fail", "would leave this test green") is too strong. `the_port_is_still_exclusive_while_the_node_holds_it` (/home/pwnall/workspace/serial-nexus/nexus-itest/tests/p11_replace_atomicity.rs:220-223) asserts only `verdict["pass"] == false`, and `nexus-sim client` emits `pass:false` in two structurally different shapes — the EBUSY refusal (`{"error":"Device or resource busy (os error 16)",…,"pass":false}`, `nexus-sim/src/main.rs` `err_verdict`) and a *successful* open whose echo a concurrent reader consumed (`{"received":0,"timed_out":true,"pass":false,…}`). The test distinguishes neither. But against the actual regression (a daemon that never takes TIOCEXCL) the outcome is a scheduling race between the daemon's parked blocking reader and the stray client's reader, not a certainty: on an idle box the guard caught the regression roughly 40% of the time (5/12 client verdicts `pass:true`, and 5 of 12 completed runs of the test binary FAILED), and on a loaded box (load ~4.8, the CI-like case) it caught it only 3/12 ≈ 2…

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Frozen tree, nothing written inside the repo. Regression emulated faithfully with an LD_PRELOAD shim that swallows TIOCEXCL (0x540C) and returns success — exactly as if `sys::set_exclusive(port.as_raw_fd(), true)` were deleted from `open_port` (nexus-daemon/src/nodes/serial.rs:703).

  # shim (scratchpad): int ioctl(int fd, unsigned long req, ...) { if (req==TIOCEXCL) return 0; return real(...); }
  gcc -shared -fPIC -o /tmp/.../noexcl.so noexcl.c

  # baseline, unmodified daemon: guard green, and the reason is EBUSY
  cd /home/pwnall/workspace/serial-nexus
  ./target/debug/deps/p11_replace_atomicity-8b34d6bb8ee01ac6 --test-threads 1
  -> 4 passed
  # 12 hand-run client verdicts against a daemon-held sim pts, all identical:
  {"error":"Device or resource busy (os error 16)","mode":"client"
… (truncated)
```

</details>

**Fix.** Assert the *reason*, not just the verdict: `assert!(verdict["error"].as_str().is_some_and(|e| e.contains("busy")), …)` (or, equivalently, `assert!(verdict.get("received").is_none())`). Both discriminate the EBUSY refusal from mere read contention, and both fail loudly the day `TIOCEXCL` stops being taken.

### `SIM-2` — `nexus-sim client --recv/--drain` verdict cannot distinguish a deadline from byte loss

**🟡 medium** · testing · `nexus-sim/src/main.rs:910` · design design §15.36 — "the sim's verdict now distinguishes timeout from loss" · verdict **CONFIRMED** (high confidence)

`recv_loop` exits on its `deadline` (line 1045-1047) exactly as it exits on quiet/EOF/EIO, and returns only `(received, sha)`. The verdict built at line 910-914 therefore reports a timed-out receive as a short count with `pass:false` and no flag — the identical shape a real byte loss produces. The `timed_out` flag §15.36 added was applied to `read_until` (line 979-1011, whose own doc says the ambiguity "cost a full investigation") and not to `recv_loop`, which is the mode the loss-accounting tests actually use.

**Failure scenario.** `nexus-itest/tests/p5_demux.rs:233` spawns four `Sim::client(--recv 256KiB --timeout-ms 90000)` readers. On a loaded runner one client hits its 90 s deadline mid-stream. Its verdict is `{"received":<short>,"sha256":<partial>,"pass":false}` — read by a maintainer as "the demux dropped bytes on channel N", triggering a hunt for a product defect in `codecs/reference`, when the rig merely ran out of wall clock. The same applies to `p4_free_for_all.rs:142`, `p4_send.rs:121`, `p4_exclusivity.rs:309` and the `--drain` users.

**Verification correction.** The finder is right and understates one half. `recv_loop` (`nexus-sim/src/main.rs:1019`) returns `(u64, String)` and breaks on `Instant::now() >= deadline` (1045-1047) exactly as it breaks on quiet/EOF/EIO/POLLHUP, and the recv/drain verdict (910-914) emits only `received`/`sha256`/`pass` — no `timed_out`. Two corrections: (1) for `--recv`, a deadline expiry and a genuinely short peer produce *byte-identical* verdict JSON (I reproduced both, same string, same exit 1) — not merely "the same shape"; (2) for `--drain` the outcome is worse than "a short count with pass:false": a drain that expires mid-stream emits `"pass": true` with a truncated `received` and a checksum of a truncated stream, and exits 0. In `p5_resync.rs:201` that lands on `assert_eq!(got_sha, want_sha, "recovered checksum != manifest (lossy/misaligned recovery)")`, i.e. a wall-clock timeout is rendered as a resync-correctness accusation against `codecs/reference`; in `p3_exact_loss.rs:141` the `pass` gate (`verdict["pass"] == true`) cannot even notice. This is a design-vs-code gap, not a nicety: design §15.36 states a…

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
$ export RT=$(mktemp -d /tmp/snxver.XXXXXX)
# (1) deadline expiry: only 100 bytes exist, master held open past the client's deadline
$ nexus-sim pty --link $RT/dev1 --source --bytes 100 --hold-ms 6000 --timeout-ms 9000 &
$ /usr/bin/time -f "elapsed=%e" nexus-sim client --path $RT/dev1 --recv 200 --timeout-ms 2000
{"behavior":"recv","mode":"client","pass":false,"received":100,"sha256":"b859c4bd1232b653344946be27cbff9a7d2d6c92bf16a15049bae496eb0adea2","tool":"nexus-sim"}
elapsed=2.00   (exit 1)

# (2) genuine short peer: 100 bytes then the master closes (EOF at 1.49 s, no deadline)
$ nexus-sim pty --link $RT/dev4 --source --bytes 100 --hold-ms 1500 --timeout-ms 6000 &
$ /usr/bin/time -f "elapsed=%e" nexus-sim client --path $RT/dev4 --recv 200 --timeout-ms 9000
{"behavior":"recv","mode":"clie
… (truncated)
```

</details>

**Fix.** Have `recv_loop` return `(received, sha, timed_out)` — set `true` only on the `Instant::now() >= deadline` break at line 1045, not on the quiet/EOF breaks — and emit `"timed_out"` in the recv/drain verdict object at line 910, mirroring `pty_sink` (line 814-818) and the echo path.

### `TESTR-2` — `p0_license_gate` passes vacuously on any `cargo metadata` failure — proven with the ban entry deleted

**🟡 medium** · testing · `nexus-itest/tests/p0_license_gate.rs:73` · design §13 licensing policy; plan §2 "the gate is proven, not assumed" · verdict **CONFIRMED** (high confidence)

The gate that exists so the licensing policy is "proven, not assumed" asserts only `!banned.status.success()`. `cargo deny check bans` exits non-zero for *any* failure of the underlying `cargo metadata` — a network failure, an unresolvable crate, an offline runner — not only for a ban hit. Step 1 (clean tree passes) resolves entirely from the committed `Cargo.lock` and a warm cargo cache, so it survives offline; step 2 must fetch `serialport`, which does not, and its fetch failure satisfies the assertion.

**Failure scenario.** CI (or a developer) runs on an air-gapped or index-throttled runner with a warm `~/.cargo` from `Swatinem/rust-cache`. `license-gate`'s `cargo deny check licenses bans sources` step passes from the lock; `cargo test -p nexus-itest --test p0_license_gate` then passes because its second step failed to fetch. The job reports green having proven nothing about the ban list, and the `serialport`/`libudev` ban could have been removed entirely without any signal.

**Verification correction.** The mechanism and the consequence are exactly as claimed; two details in the finder's write-up need correcting, neither of which weakens it. (1) The finder's own repro used `CARGO_NET_OFFLINE=1`, which this cargo rejects as a malformed value ("error in environment variable `CARGO_NET_OFFLINE`: provided string was not `true` or `false`") — so their step-2 failure was a config-parse error rather than offline mode. That is if anything a *second* trigger for the same vacuity (any cargo misconfiguration also yields a non-zero exit with no ban diagnostic); I re-established the claim properly with `CARGO_NET_OFFLINE=true` plus a CARGO_HOME whose index genuinely lacks `serialport`. (2) The suggested-fix note that "the first hit is actually its transitive libudev" is wrong: cargo-deny emits two `error[banned]` diagnostics and names both `libudev-sys = 0.1.4` and `serialport = 4.9.0`, so asserting on `error[banned]` and on `serialport` is safe. Precise statement: `cargo deny check bans` exits **2** for a real ban rejection and **1** for a `cargo metadata` failure; `p0_license_gate.rs:73` asser…

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
All commands run from /tmp/claude-1000/.../scratchpad/tr2 with cargo-deny 0.19.9.

A. Controls with a scratch crate depending on `serialport = "*"` and the repo's deny.toml copied in (exactly what the test builds):
  # online, ban list intact  -> EXIT=2, stderr has TWO `error[banned]` diagnostics
  #   "crate 'libudev-sys = 0.1.4' is explicitly banned" and "crate 'serialport = 4.9.0' is explicitly banned"
  cargo deny --manifest-path .../banned/Cargo.toml check bans ; echo $?      # 2
  # online, ban list gutted to `deny = []` (grep -c serialport -> 0) -> EXIT=0, the test's assertion correctly fires
  cargo deny --manifest-path .../banned/Cargo.toml check bans ; echo $?      # 0

B. Simulated rust-cache runner that cannot fetch (fake CARGO_HOME: registry/{cache,src} symlinked to the real w
… (truncated)
```

</details>

**Fix.** Assert on the *diagnostic*, not the exit code: require cargo-deny's output to contain `error[banned]` (and ideally the offending crate name) before accepting the rejection, e.g. `let text = String::from_utf8_lossy(&banned.stderr); assert!(text.contains("error[banned]"), "cargo-deny failed for a reason that is not the ban list — the gate proved nothing: {text}")`. Note the current failure message names `serialport` while the first hit is actually its transitive `libudev`, so quote whichever the output names.

### `WIRER-3` — The leg's oversize-chunk fragmentation has no end-to-end regression guard, though §15.24 names one

**🟡 medium** · testing · `nexus-itest/tests/p6_reference.rs:85` · design §15.24 ("Fragmentation is contract text now, with a 100 001-byte `send` round-trip as its regression guard"), §9 clause 4 · verdict **CONFIRMED** (high confidence)

Design §15.24 states the leg's fragment-never-drop rule has "a 100 001-byte `send` round-trip as its regression guard". No such test exists, and never has. The `nexus-itest` leg family (`p6_reference`, `p6_binding`, `p6_head_of_line`, `p6_hostility`, `p6_outage`, `p6_insecure_bind`) only ever moves 32 KiB per channel — below `frame_payload_cap`, so no leg test has ever crossed a frame boundary. The only fragmentation coverage is the `data_frames` unit test in `runtime.rs` and the exec-codec case in `p5_exec_conformance`; the leg's own path (`next_send` → `data_frames` → `write_all` → peer `FrameDecoder` reassembly → `route_recv`) is untested end to end. That is not an academic gap: both WIRE-1 and WIRE-2 above live on exactly this untested path.

**Failure scenario.** A regression in `next_send`, in the leg's `data_frames` loop, or in the accounting around it (e.g. charging `piece_len` twice, or dropping the tail — WIRE-1) ships green. Concretely: today an operator's `send downlink/c0 <100 000 chars>` across a leg is correct (I verified: B reports `accepted_targetward: 100001` and A reports the byte-exact 100,001 arriving), but nothing in CI would notice if it stopped being.

**Verification correction.** The core claim holds: design §15.24 (line 381 of `docs/30-design-claude-fable-v13.md`, verbatim in every generation back to v6) says fragmentation has "a 100 001-byte `send` round-trip as its regression guard", and no such guard exists for the leg — not in `nexus-itest`, not in `leg.rs`'s unit tests, and not in the retired `scripts/validate/phase6/*.sh` either. Two of the finder's supporting facts are wrong and should not be repeated. (1) "the number only ever appeared in docs/ prose" is false: `100_001` appears at `/home/pwnall/workspace/serial-nexus/nexus-daemon/src/nodes/codec.rs:649`, in `targetward_oversize_chunk_is_fragmented_never_dropped`, added by commit b9d8a50 for review-19's XC-NODROP-1. Invariant 3 is therefore not test-free — the boundary helper (`runtime::data_frames_fragments_byte_exactly_with_no_residual`, `..._reports_the_residual_...`) and the in-process codec's use of it are unit-tested. What is missing is specifically the *leg's* end-to-end use of that helper: `next_send` → `data_frames` → `write_all` → peer `FrameDecoder` → `route_recv`, plus the per-piece `acce…

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
Two daemons over a loopback unix leg, short runtime dirs, binaries from target/debug:

cfgB.toml: leg "downlink" faces=host role=listen transport=unix channels=["c0"], plus a log "sink", edge downlink/c0 -> sink.
cfgA.toml: leg "uplink" faces=target role=connect transport=unix channels=["c0"] (channel deliberately unattached).

  serialnexusctl --json --socket $RB/serialnexusd.sock load cfgB.toml   -> {"loaded": 2}
  serialnexusctl --json --socket $RA/serialnexusd.sock load cfgA.toml   -> {"loaded": 1}
  LINE=$(python3 -c "print('x'*100000, end='')")
  serialnexusctl --json --socket $RB/serialnexusd.sock send downlink/c0 --line "$LINE" --timeout-ms 5000
    -> {"delivered": true, "endpoint": "downlink/c0", "sent": 100001}

  B state: downlink.c0.accepted_targetward = 100001, discarded_unfr
… (truncated)
```

</details>

**Fix.** Add `nexus-itest/tests/p6_fragmentation.rs`: two daemons over a loopback unix leg, one channel, sender side attached to a lossless sink (a `log`, or an unattached channel whose `discarded_targetward` is the arrival oracle). Issue a `send` of 100,000 characters, then assert (a) the far side's byte count is exactly 100,001, (b) the near side's `accepted_targetward` is exactly 100,001, and (c) with the peer stalled and then killed mid-chunk, the sender's counters still sum to 100,001 — which is WIRE-1's fail-first guard.

### `ITEST-4` — `MIN_SPECS = 8` sits at exactly the device-free spec count, so up to 6 of the 14 browser specs can vanish and the gate stays green

**🔵 low** · testing · `nexus-itest/tests/p8_web_ui.rs:66` · design §15.37, plan §3 rule 7 ("assert execution, not existence") · verdict **CONFIRMED** (high confidence)

The floor is 8. On Linux — the only platform the `web-ui` job runs on — 14 specs execute per push, not the 13 the constant's own comment claims. Eight of the 14 are device-free; the other six are exactly the specs that guard §15.38's two defects (the reload splice, the `load --replace` re-anchor) and the end-to-end editor path. The assertion message promises "a *removed* spec, a filter typo, or a suite that silently self-skipped its way to nothing all trip it", which is true only for removals of 7 or more.

**Failure scenario.** `history.spec.mjs` (4 specs, including `a reload splices stored history against the replay ring exactly once` — the guard for the ring-duplication defect) and `lifecycle.spec.mjs` (1 spec — the `tap.closed`/epoch re-anchor guard) are deleted, renamed to `.js`, or made undiscoverable by a `testDir` typo. Playwright reports `9 passed`; `9 >= 8`, so the gate is green and CI never mentions that both v13 regression guards are gone.

**Verification correction.** Accurate as filed; two refinements. (1) The stale count is in two places, not one: `nexus-itest/tests/p8_web_ui.rs:64-66` ("It runs 13 specs per push plus the `@slow` one nightly") and `docs/implementation-notes.md` §15.2 ("13 specs per push plus one nightly"). The true figures are 15 `test(...)` declarations, 1 tagged `@slow`, so **14 run per push** and 8 of those are device-free. (2) The floor is not merely slack — it is doing two incompatible jobs with one constant. On a device-free run (macOS, or a Linux dev box where the fixture had no echo device) 8 is exactly tight, which is presumably why it is 8; on `ubuntu-latest`, the only platform the `web-ui` job runs on, `serial_echo()` always returns `Some` (nexus-itest/src/lib.rs:610, unconditional on Linux), so 14 always pass and the floor carries 6 specs of slack. Any 6 specs may vanish — deleted file, rename off `*.spec.mjs`, `testDir` typo, `--grep` mistake, or a `test.skip` firing when it should not — and `passed >= MIN_SPECS` still holds; only the 7th trips it. The false promise is in the gate's own assertion comment ("a *remove…

**Fix.** Make the floor a function of what the Rust gate already knows: it decides `echo.is_some()` and sets `SNX_ECHO_CONSOLE`/`SNX_HOSE_CONSOLE` accordingly, so require `MIN_SPECS_WITH_DEVICE = 14` when `echo.is_some()` and 8 otherwise. Also assert Playwright's *skipped* count is 0 in the device-bearing case — that is the direction (`test.skip` firing when it should not) the current floor cannot see at all. Fix the stale "13 specs" comment while there.

### `ITEST-5` — `NO_SLAVE_PAUSE` — the fix for a measured 74.4%-of-a-core busy spin in the sim doubles — shipped with no guard, though the measuring primitive is already in the tree

**🔵 low** · testing · `nexus-sim/src/main.rs:536` · design §16.5 (harness hardening), §15.36, AGENTS.md §9 ("find, verify, fix, add a regression guard") · verdict **CONFIRMED** (high confidence)

`fe1c52c` fixed two apparatus loops that free-ran on a full core after their pty slaves closed (`run_nullmodem_inner` at main.rs:638/702 and `pty_echo` at main.rs:728/742-757). The echo double's *functional* half got a real guard (`p7_p5::a_probe_that_opens_and_closes_the_port_leaves_the_loopback_alive`), but the CPU half got none. A reintroduction is completely silent: no assertion anywhere measures a `nexus-sim` process's CPU, so the only symptom is renewed flakiness in unrelated tests — which is precisely the 10-hour misdiagnosis loop the flake session paid for.

**Failure scenario.** Someone deletes the `thread::sleep(NO_SLAVE_PAUSE)` in `pty_echo`'s `Ok(0)` / `Err(EIO)` arms while "tightening the echo latency" (a level-triggered `POLLHUP` on a pty master with no slave open makes `poll` return immediately and forever, so the loop spins). The whole suite still passes on an 8-core dev box. On a 2-core CI runner the double burns a core for the rest of the run, `p7_p5` and `p5_*` start losing close→reopen races at 22 µs, and CI goes red on three different tests with no test naming the cause.

**Verification correction.** The finder understates its own case by resting it on AGENTS.md §9's general "add a regression guard" rhythm. This is not a missing nice-to-have: the *normative design* mandates exactly this assertion and the tree does not have it. Design §15.36 ("The flake session: mechanisms, not mysteries"), Decision paragraph, docs/30-design-claude-fable-v13.md:456: "sim doubles modeling stateless hardware tolerate hangup forever, pause rather than spin, **and get their idle CPU asserted**". Plan §3, docs/31-implementation-plan-claude-fable-v13.md:63, restates it as one of "seven rules with regression teeth": "(2) Sim doubles modeling stateless hardware tolerate bare hangups forever — pause (never spin, never exit) while peerless — **and their idle CPU is asserted**." No test in the tree asserts any nexus-sim process's CPU, so §15.36's Implications sentence — "every mechanism found became a deterministic guard" — is inaccurate for this one mechanism. The correct framing is a specified-but-unimplemented doctrine item shipped in the same commit that wrote the doctrine, not a discretionary test gap. …

**Fix.** Add a sibling to `p9_pty_collapse::a_bare_hangup_leaves_the_daemon_cpu_bounded` that points the same `/proc/<pid>/stat` sampler at the sim: spawn `nexus-sim pty --echo`, open and close its pts once, sleep a fixed window, and assert the double's `utime+stime` delta stays under a small budget (say 15 ticks / 5 % of a core over 3 s). Repeat for `nexus-sim nullmodem`. Both self-skip off Linux exactly as the existing sampler does.

### `TESTR-6` — `p8_web_history` — the only CI runner of the browser modules' unit tests, including the epoch predicate — self-skips silently with no `required` escape hatch

**🔵 low** · testing · `nexus-itest/tests/p8_web_history.rs:27` · design plan §3 rule 7; AGENTS §5 ("prefer that shape for any gate whose prerequisites are provisioned in CI but optional locally") · verdict **CONFIRMED** (high confidence)

This gate runs every `*.test.mjs` under `serialnexusweb/src/assets/` and is, by its own doc comment, "the only place those tests are run in CI". It returns a silent SKIP when `node` is absent. Unlike `p8_web_ui`, which grew `SNX_WEB_UI=required` precisely because "a gate that can skip silently is a gate CI passes over a hole", no CI job asserts this one actually ran — and no job provisions node for it either (`web-ui` sets up node but runs only `--test p8_web_ui`). It relies entirely on the runner image happening to ship node.

**Failure scenario.** A runner image bump drops the preinstalled node (or a self-hosted runner never had it). `browser_console_modules_pass_their_node_tests` prints SKIP inside `cargo test --workspace` output, the `check` job stays green, and `history.test.mjs` — the sole test of `offsetSpaceChanged`, the splice arithmetic and `saver.mjs`'s write serialisation — stops running indefinitely, unnoticed.

**Verification correction.** Two corrections, one strengthening and one softening.

(1) STRENGTHENING — the skip is not merely silent, it is *invisible*. The finder writes that the gate "prints SKIP inside `cargo test --workspace` output". It does not: `eprintln!` from a **passing** test is captured by libtest, so CI's transcript shows only `test browser_console_modules_pass_their_node_tests ... ok`. Reproduced both ways against the HEAD-built binary (see repro). The one string a human could have grepped for is absent from the very output where the hole would live.

(2) SOFTENING — the "epoch mechanism rests on two gates that can both go quiet" framing overstates the client-side blast radius. `serialnexusweb/ui-tests/tests/lifecycle.spec.mjs` ("load --replace under an open console detaches, re-anchors, and does not duplicate", asserting the `offsets restarted` marker and `countInTerminal(before) === 1`) and `history.spec.mjs` ("a reload splices stored history against the replay ring exactly once") exercise the epoch re-anchor end to end in a real browser — and `p8_web_ui` **cannot** go quiet in CI: the `web-ui` …

**Fix.** Give it the same shape as `p8_web_ui`: honour an env var (e.g. `SNX_WEB_UI=required`, or a new `SNX_NODE=required`) that turns the skip into a failure, and set it in a job that provisions node — the `web-ui` job already does, so adding `cargo test -p nexus-itest --test p8_web_history` there with the variable set costs nothing.

### `TESTR-7` — The `RefCell` meta-gate's exemption matches on bare file name, so any future `cell.rs` anywhere in a ban crate is silently exempt

**🔵 low** · testing · `nexus-itest/tests/meta_gates.rs:291` · design §16.2 / AGENTS invariant 5 · verdict **CONFIRMED** (high confidence)

`refcell_ban_covers_every_crate_that_holds_daemon_state` walks all of `<crate>/src` recursively and skips a file when `path.file_name()` is in `REFCELL_EXEMPT = ["cell.rs"]`. The exemption is intended for exactly one file, `nexus-daemon/src/cell.rs` (whose `RefCell` carries a localized `#[allow]`), but the check is name-only and depth-independent, so a new `nexus-daemon/src/nodes/cell.rs` — or any other file named `cell.rs` in either ban crate — would be exempt from the scan without anyone stating an exemption.

**Failure scenario.** A future refactor adds `nexus-daemon/src/nodes/cell.rs` (a plausible name for a per-node state cell) holding a raw `std::cell::RefCell` for node state. Clippy would still catch it via `disallowed-types` — but the whole reason this meta-gate exists is that the clippy configuration disarmed silently once already (INV5-CLIPPY-SCOPE); the belt-and-braces scan that was supposed to survive a configuration break is itself blind to the file, so the two defences fail together rather than independently.

**Verification correction.** The mechanism is exactly as filed, but the finder's framing that "the two defences fail together rather than independently" is broader than what holds. A plain raw `std::cell::RefCell` in a future `nexus-daemon/src/nodes/cell.rs` IS still caught by clippy: both `nexus-daemon/clippy.toml` and `serialnexusd/clippy.toml` carry the `disallowed-types` entry, the gate's own assertion 2 proves each file exists and carries the ban, and assertion 3 catches the historical INV5-CLIPPY-SCOPE shape (state moving to a crate off the ban list). So this is not a case where both layers go dark for the same edit.

What is genuinely lost is the meta-gate's *independent* value, which is precisely its coverage of the one case clippy cannot see: a raw `RefCell` carrying a local `#[allow(clippy::disallowed_types)]`. Today exactly two such allows exist, both in `nexus-daemon/src/cell.rs:37,44`, and nothing bounds how many more may appear or where. A second `#[allow]`ed `RefCell` in any file named `cell.rs` under either ban crate's `src` — at any depth — is invisible to clippy by construction and invisible to…

**Fix.** Make the exemption a repo-relative path rather than a bare name: store `"nexus-daemon/src/cell.rs"` and compare `path.strip_prefix(&root)`. Add a planted-violation clause proving the exemption does *not* cover a same-named file at another path, in keeping with this file's own self-proof discipline.

---

## 4. Documentation

Statements that are false about the code as it stands, or gaps a reader falls into. Verified against
the code, not against other documents.

### `DOCR-3` — docs/nexus-doctor.md (and AGENTS.md §7) assert 6.18 is "all probes supported, zero deltas" — a confirmation that predates six of the eleven probes and cannot cover them

**🟡 medium** · documentation · `docs/nexus-doctor.md:108` · design AGENTS.md §7 ("**Pause and check with the user before any one-way (hard-to-reverse) decision that depends on a kernel ability confirmed only on 7.0**") · verdict **CONFIRMED** (high confidence)

docs/nexus-doctor.md:102-125 ("## Confirmed on Linux 6.18 (Debian rodete)") records a 2026-07-19 run and concludes "**All probes `supported` — 12 supported · 0 degraded · 0 unsupported · 0 skipped; zero deltas from 7.0.**" P6–P11 were added on 2026-07-26/27, a week later, specifically so that diff could be taken — and by the engineering log's own words it has not been taken. The section therefore asserts a completed kernel confirmation over a probe set that has grown by six, and its "12 supported" total no longer corresponds to any run of the current binary.

**Failure scenario.** A future session wants to simplify `pty.rs`'s `saw_session` latch or its last-close termios-reset drain. It reads P6's 7.0 consequence ("an ungated `closed`-only last-close arm would NOT spin on the hangup alone here"), then checks whether 6.18 agrees — and both AGENTS.md §7 and docs/nexus-doctor.md tell it the 6.18 confirmation is complete with zero deltas. It removes the latch or the drain on 7.0-only evidence, which is precisely the one-way decision AGENTS.md §7 exists to gate, and precisely what the 2026-07-26/27 fix avoided by refusing to add the `|| closed` arm.

**Verification correction.** The finder is right and understates the scope: the staleness is not confined to the 6.18 section. `docs/nexus-doctor.md` predates P6–P11 *entirely* — its last commit is `d7d840f` (2026-07-25), a day before `fe1c52c` (2026-07-26) added the six probes. So (a) the "## Probes" table at lines 26–33 enumerates only P1–P5; (b) the "## Kernel-of-record report (Linux 7.0.0-28-generic)" section at lines 79–100 covers only P1–P4 and is the *7.0* baseline the 6.18 diff is supposed to be taken against, yet it too lacks every P6–P11 measurement block; and (c) the 6.18 section at 102–125 asserts the absolute "**All probes `supported` — 12 supported · 0 degraded · 0 unsupported · 0 skipped; zero deltas from 7.0.**" over a probe set that has since grown from 4 to 11. A fix scoped only to the 6.18 section would leave `docs/nexus-doctor.md` still describing a five-probe tool. One extra sharpening: AGENTS.md:498 was already slightly overstated the day it was written (`8a0ffd3f`, 2026-07-23) — P5 landed 2026-07-21 in `aef797f`, after the 2026-07-19 6.18 run, so "all-probes-supported on 6.18" never covere…

<details><summary>Reproduction (verifier's own, independent of the finder)</summary>

```
$ ./target/debug/nexus-doctor --json | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d['environment']),'env'); print([(p['id'],p['status']) for p in d['probes']]); print(d['summary'])"
9 env
[('P1','supported'),('P2','supported'),('P4','supported'),('P3','skipped'),('P5','skipped'),('P6','supported'),('P7','supported'),('P8','supported'),('P9','supported'),('P10','supported'),('P11','skipped')]
{'supported': 17, 'degraded': 0, 'unsupported': 0, 'skipped': 3}

$ ./target/debug/nexus-doctor | sed -n '145,147p'
## Summary
17 supported · 0 degraded · 0 unsupported · 3 skipped
   # vs docs/nexus-doctor.md:108 — "12 supported · 0 degraded · 0 unsupported · 0 skipped"

$ grep -c 'P6\|P7\|P8\|P9\|P10\|P11' docs/nexus-doctor.md
0
$ git log --format='%h %ad %s' --date=short -1 -- do
… (truncated)
```

</details>

**Fix.** Scope the 6.18 section to what it actually covers — retitle it "Confirmed on Linux 6.18 (P1–P5, 2026-07-19)" and drop the absolute "All probes" / "12 supported" totals — and add an explicit "P6–P11: not yet run on 6.18" line with the command that closes it. Make the same correction at AGENTS.md:498.

### `DATA-1` — `data.rs`'s model→shipped map — the whole justification for keeping the module — names daemon types that no longer exist

**🔵 low** · documentation · `nexus-core/src/data.rs:22` · design §5; implementation-notes §3.18 (data.rs stays the executable specification rather than being rebased onto the shipped boundaries) · verdict **CONFIRMED** (high confidence)

The module doc states hostward is "Shipped as `nexus-daemon`'s endpoint-keyed `runtime::Wiring::host_sinks` (a `Vec<HostwardSink>` per host-facing endpoint)". Neither name exists: `Wiring`'s field is `host_fanout: HashMap<EndpointAddr, SharedFanOut>` (runtime.rs:724) and the sink type is `AttachedSink` inside a `FanOutList` (`Arc<Mutex<Vec<AttachedSink>>>`), broadcast through the single `runtime::fan_out` helper. The doc predates the v10 fan-out consolidation (invariant 9) and the v12 shared-wiring change (invariant 14).

**Failure scenario.** §3.18's deliberate decision to keep `data.rs` as a model rests on this doc being the honest bridge to the real code. A maintainer auditing whether the shipped hostward path still obeys §5 greps `host_sinks`/`HostwardSink`, finds nothing, and either concludes the model is dead or audits the wrong thing. Concretely, the doc's `Vec<HostwardSink>` phrasing hides precisely the property the model does not express and invariant 14 says must not be regressed — that the fan-out is shared mutable state (`Arc<Mutex<…>>`) reachable from the serial reader's blocking thread and mutated live by `connect`/`disconnect`.

**Verification correction.** `nexus-core/src/data.rs:22-23` maps §5's hostward rule onto two daemon identifiers that no longer exist anywhere in the workspace: `runtime::Wiring::host_sinks` and `Vec<HostwardSink>`. The live shape is `Wiring::host_fanout: HashMap<EndpointAddr, SharedFanOut>` (runtime.rs:724) over `FanOutList` (`Arc<Mutex<Vec<AttachedSink>>>`, runtime.rs:332/350/355), broadcast through the one `runtime::fan_out` helper the doc never names. Correct the finder on three points. (1) Provenance: the doc was accurate when written — `d7d840f` (the review-26 remediation that authored it) really declared `pub host_sinks: HashMap<EndpointAddr, Vec<HostwardSink>>`; it went stale at v12's `548823e`, which renamed the field and did not touch data.rs. It does not "predate the v10 fan-out consolidation"; the `fan_out` helper landed in the same commit that wrote the doc, so omitting it is an original gap, not v12 drift. (2) Scope: only the *first* of the three bullets is stale. Bullet 3's `nodes/codec.rs` `framed` slot still exists (codec.rs:522) and bullet 2's "a paused origin is a task suspended inside `tx.send…

**Fix.** Update the three shipped-path bullets to name `runtime::Wiring::host_fanout` / `FanOutList` / `AttachedSink` and the single `runtime::fan_out` helper, note that the list is `Arc<Mutex<…>>` shared with the serial reader thread and mutated live by `connect`/`disconnect` (invariant 14), and drop `log.rs` from the producer list. Consider a cheap meta-gate asserting the identifiers this doc names still resolve, since its accuracy is the whole basis of §3.18.

### `DOCE-1` — Design §17 says browser history is *keyed by* the offset-space epoch, contradicting §15.38 and the implementation

**🔵 low** · documentation · `docs/30-design-claude-fable-v13.md:509` · design design §17 line 509 vs §15.38 line 467 · verdict **CONFIRMED** (high confidence)

§17 states the OPFS history is "keyed by the daemon's socket path, the endpoint address, the daemon `instance` nonce and the endpoint's offset-space `epoch` (§15.38)". §15.38 says the opposite about the same value: "A client persists it beside its scrollback and re-anchors exactly when it changes." The implementation follows §15.38 — `keyFor` omits the epoch and `opfs.mjs` stores it inside the record's 24-byte header — and it is right to: keying by epoch would give every hub rebuild a fresh, empty file and orphan the previous one, i.e. `load --replace` would *destroy* scrollback rather than re-anchor onto it, which is the failure §15.38 was written to fix.

**Failure scenario.** A future session reads §17 as normative (AGENTS.md: "When this file and the design disagree, the design wins"), moves the epoch into `keyFor`, and every `load --replace`, `add-node` or `remove-node` silently abandons each console's stored scrollback and starts a new 16 MiB file — losing history the operator can still see in the pane, and compounding WEB-1's orphan growth by one file per graph edit instead of one per daemon boot.

**Verification correction.** The core claim holds, and is stronger than filed in two ways and weaker in one.

Stronger: (a) §17's sentence contradicts *three* other places, not one — §15.38 (line 467, "A client persists it beside its scrollback and re-anchors exactly when it changes"), §15.32's own decision paragraph in the same document (line 430, "keyed by socket path, endpoint, and instance"), and the v13 plan (docs/31-implementation-plan-claude-fable-v13.md:293, same three-part key). (b) The epoch-in-the-key variant is not merely unimplemented, it is the alternative that was explicitly *considered and rejected* on the record: docs/implementation-notes.md:919-921 lists it as option (B) — "a per-endpoint offset epoch surfaced in `tap.open`/`state` folded into the OPFS key — honors invariant #10 for *all* offset clients but orphans scrollback on every benign reconfigure" — and the shipped fix is option (A) plus a daemon-reported epoch. So §17 now documents the rejected design. (c) The clause is HEAD's own drift, not inherited: `git show cfb2187 -- docs/30-design-claude-fable-v13.md` shows the v12 text "…the end…

**Fix.** Amend §17's sentence to "…keyed by the daemon's socket path, the endpoint address and the daemon `instance` nonce, with the endpoint's offset-space `epoch` stored beside the bytes so a rebuilt hub re-anchors rather than starting over (§15.38)", and note the reason so it is not "corrected" back.

### `DOCR-1` — README's documentation index links to two files that do not exist and names the superseded v12 design as normative

**🔵 low** · documentation · `README.md:180` · design AGENTS.md §9 ("Design/plan pairs are version-suffixed and monotonic … `§N` always means the *current* normative design") · verdict **CONFIRMED** (high confidence)

README.md:180-181 links to `docs/28-design-claude-fable-v12.md` and `docs/29-implementation-plan-claude-fable-v12.md`. Both paths were moved to `docs/historical/` when v13 landed, so both links are dead, and line 180 additionally declares that superseded document to be what every `§N` reference in the repository means. The current normative pair is `docs/30-design-claude-fable-v13.md` + `docs/31-implementation-plan-claude-fable-v13.md`.

**Failure scenario.** A contributor or agent opens README.md (the repository's only entry point), clicks the design link and gets a 404 on GitHub / a missing file locally. If they instead find the file in `docs/historical/` and read it as instructed, they resolve `§15.37`/`§15.38` (the v13 browser-UI track), `§15.38`'s `epoch` contract and the restored §3/§11 name-legality and empty-parse rules against a document that predates all of them — the exact "acting on a stale design generation" failure AGENTS.md:6-10 warns has already cost this project twice.

**Verification correction.** The README claim is exactly as filed, but the finding understates the *scope* and overstates the severity. Scope: `README.md:180-181` are not the only stale pointers — `AGENTS.md:564` ("The newest pair lives in `docs/` (currently v12: `28-design-…-v12.md` + `29-implementation-plan-…-v12.md`)") names the same superseded pair, and AGENTS.md *was* edited in the very commit that shipped v13 (cfb2187) without that line being updated. Those three lines are the only references to the v12 filenames outside `docs/historical/` in the whole tree. Severity: this is a recurrence of review-26 **DOC-5** ("`README.md:167-168`: the documentation index still points at the **deleted** v9 design/plan pair and calls them normative"), which that review rated **low** and the remediation ledger closed as "DOC-5 ✅ (points at v11)". Same defect, third generation, so the useful fix is not a third manual patch: `docs/30-…-v13.md` was moved into place by `2fae6ef` with a pure `git mv` of the v12 pair and no README/AGENTS edit, so a docs-link check (or version-agnostic filenames / a `docs/README.md` index) is wha…

**Fix.** Point both entries at `docs/30-design-claude-fable-v13.md` and `docs/31-implementation-plan-claude-fable-v13.md`, and consider making the filenames version-agnostic (or adding a `docs/README.md` index) so the next generation bump does not silently break the front page again.

### `DOCR-2` — docs/nexus-doctor.md — the file README calls "the probe reference" — documents 5 of the binary's 11 probes; P6–P11 appear nowhere

**🔵 low** · documentation · `docs/nexus-doctor.md:25` · design README.md:162 ("See [`docs/nexus-doctor.md`](docs/nexus-doctor.md) for the probe reference."); AGENTS.md §3 crate table row for `nexus-doctor` · verdict **CONFIRMED** (high confidence)

The probe table at docs/nexus-doctor.md:25-31 lists P1, P2, P3, P4, P5 and nothing else, and the string `P6`/`P7`/`P8`/`P9`/`P10`/`P11` does not occur anywhere in the file. The shipped `nexus-doctor` emits eleven probes. Six probes — including the two (P6, P7) whose output explicitly tells the reader "diff this block before simplifying anything" and the one (P8) that probes invariant 1's premise — have no reference documentation at all.

**Failure scenario.** An operator hits a PTY problem, is told by README.md:154-155 and docs/security.md to "attach nexus-doctor output to any bug report", runs it, and gets six probe blocks with raw kernel measurements (`spin_ratio`, `pollin_with_no_data_passes`, `bytes_accepted_before_eagain`, `overshoot_median_us`) that the reference documentation never mentions. There is no documented answer to "what verdict is expected here", "is `degraded` on P8 a problem", or "do I need `--port` for P11" — the P3/P5 `--port` opt-in is documented at line 12 and 21, but P11's identical opt-in is not, so a bug reporter running the documented command silently omits the real-port counter probe.

**Verification correction.** The gap is real and the finder understated the *sharper* half of it while overstating the operator harm.

Real and verified: `docs/nexus-doctor.md` — the file `README.md:162` designates "the probe reference", and the only user-facing probe reference in the tree (no man page, nothing in `packaging/`) — has a probe table at lines 25–31 covering P1–P5 only, and the strings `P6`…`P11` occur nowhere in it (`grep` exits 1 across `docs/nexus-doctor.md`, `docs/macos.md`, `README.md`, `docs/security.md`). The shipped binary emits eleven probes and renders `### P6` … `### P11` sections in the default Markdown report. Only `AGENTS.md:159` and `docs/implementation-notes.md:400-431` describe them. The omission is accidental, not a decline: `git log -- docs/nexus-doctor.md` ends at `d7d840f` (review-26 remediation), and `fe1c52c` — the commit that added P6–P11 — touched `docs/implementation-notes.md` and nothing else under `docs/`.

Two statements in the file are now actively wrong, not merely absent, and that is the part worth fixing first:
- **line 12**: "`nexus-doctor --port /dev/ttyUSB0   # op…

**Fix.** Add six rows to the probe table (ID, what it checks, verdict → consequence) and extend line 12's `--port` note to say P11 is opt-in for the same DTR reason as P3/P5. State the P6–P11 doctrine the notes record: these report observations, so a differing kernel is `degraded` with the observation named, never `unsupported`.

### `DOCR-4` — AGENTS.md's crate table still says the web console "refuses graph/lifecycle verbs", contradicting invariant 11 and the shipped allowlist

**🔵 low** · documentation · `AGENTS.md:157` · design design §15.35 / §17 (docs/30-design-claude-fable-v13.md:448, :507); AGENTS.md invariant 11 ("**v12 widened the list** (§15.35): `add-node`, `remove-node`, `connect`, `disconnect` and `ports` are now forwarded") · verdict **CONFIRMED** (high confidence)

The `serialnexusweb` row of the AGENTS.md §3 crate table reads "Filtering JSON-RPC proxy; enforces per-session token + Host validation; **refuses graph/lifecycle verbs** (§17)." Since §15.35 the bridge forwards `add-node`, `remove-node`, `connect`, `disconnect` and `ports`; only the three *lifecycle* verbs are refused. The sentence is wrong in the permissive direction (it understates browser capability) and miscites §17, which says the opposite.

**Failure scenario.** A reviewer or agent orienting from the AGENTS.md crate table — which is the table the file exists to provide — concludes the web console cannot mutate the graph, and therefore treats the web token as strictly less privileged than the control socket. Concretely: they approve widening the web bind (or hand the token to a junior operator) on the belief that a token holder can only watch, when in fact a token holder can `add-node` a `log` node that writes anywhere the daemon's user can write and an `exec` codec that runs an arbitrary command line as that user — the capability docs/security.md:290-296 states in a block quote precisely because it must not be implied.

**Verification correction.** The factual claim is correct and precise: `AGENTS.md:157` (the `serialnexusweb` row of the §3 crate table) still reads "**refuses graph/lifecycle verbs** (§17)", while since v12/§15.35 the bridge forwards `ports`, `add-node`, `remove-node`, `connect` and `disconnect`; only `load`/`teardown`/`shutdown` are refused. The cited §17 now says the opposite ("With §15.35 the allowlist includes the graph-editing verbs"), so the citation points at text that contradicts the sentence. What should be corrected is the *consequence*, which the finder overstates. This is not a plausible route to a security misconfiguration: the same file states the truth twice and loudly — §2's v12 track entry ("the web console's graph and editor pages with the bridge allowlist widened to the graph-editing verbs") and invariant 11 in the "DO NOT REGRESS" section AGENTS.md itself directs the reader to read first ("**v12 widened the list** (§15.35): `add-node`, `remove-node`, `connect`, `disconnect` and `ports` are now forwarded") — and AGENTS.md §1 states its own precedence rule ("When this file and the design disagr…

**Fix.** Replace "**refuses graph/lifecycle verbs** (§17)" with "allowlisted verb bridge: observation + arbitration + graph editing; `load`/`teardown`/`shutdown` stay off the browser wire (§17/§15.35)" so the table agrees with invariant 11 and with `bridge::ALLOWED`.

### `DOCR-5` — AGENTS.md §9 still names v12 as the current design/plan pair

**🔵 low** · documentation · `AGENTS.md:564` · design AGENTS.md §9 "Design/plan pairs are version-suffixed and monotonic" · verdict **CONFIRMED** (high confidence)

AGENTS.md:563-565 reads "The newest pair lives in `docs/` (currently v12: `28-design-…-v12.md` + `29-implementation-plan-…-v12.md`); superseded generations move to `docs/historical/`." Both named files are in `docs/historical/`; the pair in `docs/` is v13. This directly contradicts AGENTS.md:6, which correctly names `docs/30-design-claude-fable-v13.md`.

**Failure scenario.** An agent following §9's own rhythm instructions ("`§N` always means the *current* normative design") reads §9's parenthetical as authoritative, opens the v12 pair, and files or resolves a finding against text v13 changed — e.g. §10's `ports` description or §11's connect/disconnect-are-shipped paragraph, both of which docs/implementation-notes.md:32-45 records as having been restored *into v13*.

**Verification correction.** The stale-pointer defect is real, but two details in the finding need correcting. (1) The sharper half is README.md:180-181, which the finder relegates to the suggested fix: those are markdown *links* to `docs/28-design-claude-fable-v12.md` / `docs/29-implementation-plan-claude-fable-v12.md`, paths that no longer exist (the files are in `docs/historical/`), so they 404 on GitHub and dead-end locally — and they are the lines that say "A §N reference anywhere in this repository means this document." AGENTS.md:564 is a parenthetical naming filenames without linking them. (2) The failure scenario's examples are inverted: §10's `ports` description and §11's connect/disconnect-are-shipped text are exactly the paragraphs `docs/implementation-notes.md:36-45` records as *restored into* v13 from the v12 text, so v12 and v13 agree there and reading v12 would not mislead. The real divergence is v13-only content: `grep -c "15.37\|15.38" docs/historical/28-design-claude-fable-v12.md` returns 0, while AGENTS.md §2/§3/§5 and `docs/implementation-notes.md` cite §15.37/§15.38 repeatedly (plus §16.7's …

**Fix.** Update the parenthetical to v13 (`30-design-…-v13.md` + `31-implementation-plan-…-v13.md`), and note that §9's parenthetical must be bumped with every generation — it and README.md:180-181 are the two places that name the pair by filename.

### `DOCR-6` — AGENTS.md invariant 10 still calls the `instance`-does-not-rotate problem "the known open issue" and never mentions the `epoch` field that replaced it

**🔵 low** · documentation · `AGENTS.md:424` · design design §15.38; AGENTS.md §2 "What the browser suite found (2026-07-27)" item (1) · verdict **CONFIRMED** (high confidence)

Invariant 10 ends: "`info.instance` is a per-boot nonce so a client detects the offset reset across a restart — but note it does **not** rotate on a hub rebuild (`load --replace`), which is the known open issue in `docs/implementation-notes.md`." That issue was closed by the v13 per-hub `epoch` reported on `tap.open`, which invariant 10 — the invariant a tap-consuming client author is told to read — does not mention at all.

**Failure scenario.** Someone writing a second tap client (a TUI, a log-shipper, a replacement for the browser history layer) reads invariant 10 — advertised as load-bearing and settled by real bugs — sees no epoch, sees "the known open issue", and infers an offset-space restart from `from_offset < frontier`. That is exactly the `offsetSpaceReset` heuristic the browser suite found on 2026-07-27, which duplicated stored scrollback once per reload; AGENTS.md:104 records that "the daemon now reports a per-hub `epoch` … and the client re-anchors on that", but invariant 10 does not, so the heuristic gets rebuilt.

**Verification correction.** Accurate as filed, with the failure scenario tightened. The stale text is real and verified: `/home/pwnall/workspace/serial-nexus/AGENTS.md:422-425` (invariant 10) ends "`info.instance` is a per-boot nonce … but note it does **not** rotate on a hub rebuild (`load --replace`), which is the known open issue in `docs/implementation-notes.md`", and the invariant never mentions `epoch`. Two things make it a defect rather than a stylistic gap: (a) AGENTS.md §2 line 105 forward-references the epoch fix *to* invariant 10 — "the client re-anchors on that **(invariant 10)**" — so the file points a reader at text that says the opposite; (b) the pointer is dangling: the only remaining "open issue" string in `docs/implementation-notes.md` is line 114, which says the epoch "**closes the recorded open issue**". Note the invariant's factual clause is still true in isolation (`info.instance` genuinely does not rotate on a hub rebuild — verified live, the nonce is per-boot); what is stale is the "which is the known open issue" characterization plus the omission of the replacement mechanism. The finder…

**Fix.** Extend invariant 10 with the epoch clause: `tap.open` reports a per-endpoint-hub `epoch` that is unique within a process and never reused; a client persists it beside the stored offset and re-anchors when it changes, and must not infer a reset from `from_offset < frontier`. Drop "the known open issue" or mark it closed.

### `RV-6` — The state file is <stem>.state.toml, not <socket>.state.toml as the design and rustdoc say

**🔵 low** · documentation · `nexus-daemon/src/lib.rs:305` · design 11 · verdict **CONFIRMED** (high confidence)

state_file_for uses socket.file_stem() + '.state.toml', so /run/serialnexusd.sock yields /run/serialnexusd.state.toml. Design 11 and two rustdoc comments (lib.rs:113, lib.rs:332) write it as <socket>.state.toml, which reads as appending to the whole socket path.

**Failure scenario.** An operator or script deriving the state-file path from the documented formula looks for serialnexusd.sock.state.toml and finds nothing. Separately, two daemons whose socket paths differ only in extension (a.sock, a.socket) derive the SAME state file, which undercuts 11's 'per-daemon-unique so parallel test daemons never share state'.

**Verification correction.** The mechanism is real but the finder names the wrong function and misses the most important doc site. The function is `resolve_state_file` (`nexus-daemon/src/lib.rs:300`), not `state_file_for`; line 305 is `.file_stem()` and line 308 is `socket_path.with_file_name(format!("{stem}.state.toml"))`. The `<socket>.state.toml` spelling appears at SEVEN sites, not three: design §11 lines 213 and 386, `nexus-daemon/src/lib.rs:113` and `:332`, **`serialnexusd/src/main.rs:40`** (the operator-facing `--state-file` `--help` text — the one an operator actually reads, and the finder omitted it), `docs/implementation-notes.md:2563`, and a comment at `nexus-itest/tests/p8_external_codec.rs:130`. The collision half is not merely theoretical: I reproduced silent cross-daemon config loss (see repro). One caveat on the suggested fix: the tempting one-line code fix — `file_name()` instead of `file_stem()` — would make the code match the documented `<socket>.state.toml` AND make the derivation injective, but it silently orphans every existing deployment's `serialnexusd.state.toml`, so the daemon would com…

**Fix.** Correct the design 11 parenthetical and the two rustdoc comments to <socket-stem>.state.toml.

### `WIRE-3` — The reference codec's "only unrecoverable corruption is an over-max length prefix" claim is false; an under-max mangled prefix silently merges following frames into one bogus `data` event, and no test covers the class

**🔵 low** · documentation · `codecs/reference/src/lib.rs:18` · design §7.5 ("the reference codec resynchronizes by length-guidance — skip exactly the framed length, count one framing error"), §9 length-guided resync, §5 all-loss-is-counted · verdict **CONFIRMED** (high confidence)

The module doc asserts that recovery is exact and that "The only unrecoverable corruption is a mangled length prefix (`body_len` over the maximum)". A prefix mangled to a value *under* the maximum is equally unrecoverable and strictly worse: `try_decode` accepts the phantom frame, so the decoder emits a single `data` event on a legitimate channel whose payload is the raw framing bytes of the frames that followed, and every real frame inside that span disappears — with no framing error counted for the merge itself. `p5_resync`'s oracle only corrupts the type byte (`nexus-sim`'s mux writes `wire[frame_start + 4] = 0xFF`, leaving the length prefix intact by construction), so the class that actually loses and corrupts data has no executable case anywhere in the tree.

**Failure scenario.** A hardware-mux serial line (the reference codec's stated deployment) flips one bit in a frame's 4-byte length prefix, turning `body_len = 205` into `1485`. Reproduced live: nine frames (eight 200-byte `data` on `c0`, one 5-byte `data` on `c1`, 1605 real payload bytes) are muxed, `wire[2]` is set to `0x05`, and `ReferenceCodec::demux` emits exactly **one** event — `data` on `c0` carrying **1480 bytes**, of which only the first 200 are real `c0` payload and the remaining 1280 are the envelope framing bytes of frames 1-7. The `c1` frame never appears. A `log` node on `c0` writes the 1280 bytes of foreign framing to disk as console output, and `state` reports the loss only as `framing_errors: 46` (from the subsequent re-scan) with no byte magnitude and nothing at all attributable to the merge.

**Verification correction.** The module doc at `codecs/reference/src/lib.rs:15-17` narrows "mangled length prefix" with the parenthetical "(`body_len` over the maximum)", which makes its "only unrecoverable corruption" sentence false — and it contradicts both design §15.23 and `docs/implementation-notes.md:2400`, which state the rule correctly without the over-max qualifier. A prefix mangled to any value `<= MAX_FRAME_SIZE` is equally unrecoverable and strictly worse: `try_decode` succeeds on the phantom boundary, so `demux` emits one `data` event on the frame's real (configured) channel whose payload is the raw framing bytes of the frames that followed, and every real frame inside that span disappears with no framing error attributable to the merge. Beyond the finder's account, the daemon does not merely pass the bytes on: `nodes/codec.rs:400-434` mirrors them to the channel's tap/replay ring, broadcasts them to that channel's sinks, and adds them to `delivered_hostward` — the corruption is affirmatively reported as delivered payload. Two corrections to the finder's coverage claim: the tree *does* test an under…

**Fix.** Correct the module doc to state that recovery is exact only when the length prefix survives, and that a prefix corrupted to an under-max value merges the following frames into one `data` event on the corrupt frame's channel (undetectable without an integrity field — a deliberate v1 trade, §9 mandates no checksum). Add a `codecs/reference` unit test pinning the observed behaviour so the trade is a recorded fact rather than a surprise, and consider a `nexus-sim mux --corrupt-prefix` mode plus a byte-magnitude counter alongside `framing_errors` so the size of a resync gap is visible in `state`.

### `CTL-3` — `serialnexusctl load`'s file-read error does not name the file it could not read

**⚪ nit** · documentation · `serialnexusctl/src/main.rs:361` · verdict **CONFIRMED** (high confidence)

`read_config` propagates `std::fs::read_to_string(file)?` with no context, while the very next line adds `parsing {}` context to the TOML error. A mistyped path therefore produces a bare errno with no path, in both the human and the `--json` arm.

**Failure scenario.** `serialnexusctl load ~/rigs/lab-rig.tml` (typo: `.tml`) prints `Error: No such file or directory (os error 2)` — with several config files in play and no path in the message, the operator cannot tell which file the CLI looked for. Under `--json` it is `{"error":{"code":-32603,"message":"No such file or directory (os error 2)","data":{"origin":"client"}}}`, equally pathless.

**Verification correction.** The mechanism is exactly as filed, but the finder's supporting detail about the doc comment is wrong and the sharper framing is different. The doc comment (`serialnexusctl/src/main.rs:358-359`) reads "mapping a **parse error** to a message that names the file" — it is scoped to the parse arm and does not overclaim, so "the doc comment claims it maps errors ... which is true only of the parse arm" should be dropped. The stronger statement is internal inconsistency: `read_config`'s `std::fs::read_to_string(file)?` (main.rs:361) is the *only* client-side I/O failure in the whole CLI that does not name its subject. The parse arm names the file (`parsing {}: {e}`, :363), the empty-graph bail names the file (:372-379), and all three `UnixStream::connect` sites map to `connecting to {}: {e}` (:630-631, :682, :755). So the fix is restoring a convention the file already keeps everywhere else, not adding a new one. Category is better read as diagnostics/error-messages than documentation.

**Fix.** `let text = std::fs::read_to_string(file).map_err(|e| anyhow::anyhow!("reading {}: {e}", file.display()))?;` — matching the parse arm immediately below.

### `ITEST-6` — Two spec docs still name `offsetSpaceReset`, a function `cfb2187` deleted

**⚪ nit** · documentation · `serialnexusweb/ui-tests/tests/lifecycle.spec.mjs:7` · design §15.38 · verdict **CONFIRMED** (high confidence)

`lifecycle.spec.mjs`'s header says "The client's answer is `offsetSpaceReset`, and until now nothing exercised it against a real stored history", and `p8_tap_offsets.rs`'s module doc for the same behaviour says the client "must key on … the terminal `tap.closed`". Both describe the pre-v13 design: `history.mjs` now exports `offsetSpaceChanged`/`reanchor` and the client keys on the daemon-reported `epoch`.

**Failure scenario.** A reader debugging a console-freeze report greps for `offsetSpaceReset`, finds nothing in `history.mjs`, and concludes the spec is testing dead code — or worse, re-derives the offset-inference heuristic §15.38 rejected in both directions, because the surviving prose still frames the problem as one offsets can answer.

**Verification correction.** Substantively correct; three refinements. (1) `cfb2187` did not merely delete `offsetSpaceReset` — it RENAMED it to `offsetSpaceChanged` and split the re-anchor out as `reanchor`, in the very same commit that ADDED `lifecycle.spec.mjs` (63 new lines). The spec therefore shipped self-inconsistent with its own commit, which is a sharper statement than "docs drifted since". (2) The finder under-cites `p8_tap_offsets.rs`: besides lines 550-554 the same stale framing appears twice more in that file — module-doc bullet 4 (lines 32-34, "the reason `tap.closed` (TAP-1) is the signal a client must key on rather than the nonce") and the inline comment at line 620 ("`tap.closed` is what tells it not to"). That file was NOT touched by `cfb2187` (last touched `d7d840f`, the review-26 remediation), so it is pre-epoch by provenance, not by oversight. (3) A third stale site the finder missed corroborates it: `AGENTS.md:424` (invariant 10) still calls the non-rotating `instance` "the known open issue", contradicting AGENTS.md §2 which says the epoch closed it — and `lifecycle.spec.mjs:6` cites that s…

**Fix.** Update both docs to name `offsetSpaceChanged`/`reanchor` and the daemon-reported `epoch`, keeping `tap.closed` as what tells the client the *stream* ended rather than as the re-anchor trigger.

### `RV-7` — nexus-core::config's module doc lists a node kind that does not exist

**⚪ nit** · documentation · `nexus-core/src/config.rs:9` · design 7.7, 14 · verdict **CONFIRMED** (high confidence)

The module doc says the format covers 'exec, and existing-terminal kinds'. NodeConfig has no existing-terminal variant; 7.7 is unimplemented and the daemon answers 'unknown variant `existing-terminal`, expected one of `serial`, `pty`, `log`, `codec`, `leg`, `map`'.

**Failure scenario.** A reader of the config module believes 7.7 shipped. README.md:48 correctly marks it as not implemented; this module doc contradicts it.

**Verification correction.** `nexus-core/src/config.rs:7-11` carries phase-1-era roadmap prose whose node-kind list is wrong three ways, not one: "Phase 1 models the graph container and the first three boundary node kinds (serial, pty, log). Later phases extend [`NodeConfig`] with codec, leg, exec, and existing-terminal kinds". (1) `existing-terminal` never landed — §7.7 is deliberately deferred (AGENTS.md §2, README.md:48) and the daemon refuses it. (2) `exec` is not a `NodeConfig` variant either and never was intended to be: the exec codec is an ordinary `Codec` node selected by `codec = "exec"`, as the very same file says at lines 156-158 and 611-613. (3) The kind a later phase *did* add — `map` (§7.8, v11) — is missing from the list. The shipped variant set is exactly `Serial | Pty | Log | Codec | Leg | Map`. The finder's paraphrase "the module doc says the format covers …" is a slight misquote — the sentence is forward-looking ("later phases extend"), so the harm is a stale roadmap at the head of a *publishable* crate's config module (nexus-core has no `publish = false`) rather than a positive claim that §7…

**Fix.** Drop existing-terminal from the module doc, or mark it as design-specified and unimplemented.

### `SIMPB-7` — `server.rs::path_is` documents a trailing-slash tolerance its body does not implement

**⚪ nit** · documentation · `serialnexusweb/src/server.rs:535` · design §17 · verdict **CONFIRMED** (high confidence)

`/// Path comparison ignoring a trailing slash difference on `/`.` sits directly above `fn path_is(path: &str, want: &str) -> bool { path == want }`. The body is plain equality; nothing about trailing slashes happens. The function is also a zero-value wrapper — it has two call sites (server.rs:289 `path_is(&req.path, "/")`, server.rs:319 `path_is(&req.path, "/ws")`), both of which read identically as `req.path == "…"`.

**Failure scenario.** A maintainer adding a route, or debugging why `GET /ws/` returns 404 instead of upgrading, reads the doc comment and concludes the router already normalises trailing slashes — so they look for the bug somewhere else (the Origin gate, the token gate, the upgrade handshake) rather than in the comparison. The same comment invites the opposite error: someone "restoring" the documented behaviour by making `path_is` strip trailing slashes would make `GET /?token=…` and `GET /` diverge from what the bootstrap-URL branch (server.rs:288-300) and the Playwright suite assume.

**Verification correction.** `path_is` (serialnexusweb/src/server.rs:535-538) carries the doc line "Path comparison ignoring a trailing slash difference on `/`" over a body that is plain `path == want`, and has done since it was introduced in 18f5216 — the tolerance was never implemented, and `req.path` is not normalised upstream either (read_request only splits the target at the first `?`). Two call sites, both trivially inlineable. The finder's second failure branch is wrong and should be dropped: implementing the documented tolerance would NOT make `GET /?token=…` diverge from `GET /`, because the query is split off before the comparison, so `path_is("/", "/")` holds under either implementation; nothing in the bootstrap branch or the Playwright suite depends on `/ws/` returning 404. The live consequence is one-directional only — a maintainer trusting the comment looks for a `/ws/` 404 in the Origin/token/upgrade gates instead of in the router.

**Fix.** Delete `path_is` and inline `req.path == "/"` / `req.path == "/ws"` at the two call sites — the honest spelling, and one fewer name to keep truthful. If trailing-slash tolerance is actually wanted, implement it and add a case to `a_request_head_parses_and_is_bounded`; do not leave the doc asserting it. Risk: none.

### `WIRE-4` — `data_frames`' justification comment claims nothing bounds channel-identity length structurally; validation has bounded it at 256 bytes since invariant 7

**⚪ nit** · documentation · `nexus-daemon/src/runtime.rs:274` · design AGENTS invariant 7 / §3 (`graph::MAX_NAME_LEN`); docs/codec-authors.md:157-160 · verdict **CONFIRMED** (high confidence)

The doc comment on `data_frames` says the `DataFrame::Residual` path is "pathological rather than impossible — nothing bounds identity length structurally today". Every endpoint name — which *is* the channel identity for `codec` and `leg` nodes — is bounded at `graph::MAX_NAME_LEN = 256` by `GraphConfig::validate`'s topology pass, so `frame_payload_cap`'s floor-at-1 can never actually be hit through configuration. The defensive code is correct and should stay (invariant 3's third clause); only its stated premise is wrong, and a future reader could reasonably conclude that identity length is an open hole worth 'fixing' elsewhere.

**Failure scenario.** No runtime failure. A maintainer reading runtime.rs:270-275 concludes that channel identities are unbounded and either adds a redundant length check at the framing layer or, worse, relaxes `graph.rs`'s `NameTooLong` check believing the framer already defends against it — at which point the residual path becomes genuinely reachable and every byte of an affected chunk is charged to `discarded_unframable` instead of being sent.

**Verification correction.** The comment at /home/pwnall/workspace/serial-nexus/nexus-daemon/src/runtime.rs:270-275 does not merely misstate a fact — it contradicts the *current normative design*. v13 design §3 ("Names and identities") states the length bound and gives verbatim the rationale the comment denies ("an unbounded identity drives that to zero and leaves the targetward fragmenter unable to place a single byte... A generous cap on a human-chosen label removes the failure mode by construction"), and §11 lists "the §3 length bound" among the structural checks `load` performs before anything is created. The bound is enforced on *every* graph-creating path, not just `load`: `GraphConfig::validate` ends with `errors.extend(self.to_model().validate())` (nexus-core/src/config.rs:430), and the model pass pushes `NameTooLong` for any endpoint name > `MAX_NAME_LEN` (nexus-core/src/graph.rs:700-707); `add-node` and the startup load from `--config`/the state file all funnel through the same `load` dispatch. Every `channel` string that reaches `data_frames` is config-derived — leg.rs:358/374 iterate `self.channels` …

**Fix.** Reword to "unreachable through configuration (`graph::MAX_NAME_LEN` = 256 bounds every channel identity), but kept because invariant 3's third clause forbids a silent truncation if that ever changes" — keeping the code exactly as it is.

---

## 5. Simplification and clarity

Design §16's standing thesis, applied to itself: these are the places where a rule is *nearly* owned by
a helper and the surrounding half is still hand-written per site. Severity here is about
maintainability — except `SIMP-2`, which is the structural form of four confirmed accounting defects in
§1 and should be read as a bug-prevention item.

### `SIMP-1` — The per-channel hostward routing block is a verbatim clone between `codec.rs` and `exec.rs` — the `fan_out` extraction stopped one level short

**🔵 low** · simplification · `nexus-daemon/src/nodes/codec.rs:400` · design §16 ("the same rules being re-derived, per node, by hand"); AGENTS.md invariant 9 ("the hostward fan-out is one helper") · verdict **CONFIRMED** (high confidence)

`codec::hostward_demux` (codec.rs:400-450) and `exec::route_event` (exec.rs:635-681) contain logically identical code: stat lookup, `active` latch, `feed.mirror`, the `scratch = Cell::new(0u64)` defensive-counter dance, `sinks.broadcast(&bytes, unattached).live` → `delivered_hostward`, the `else if let Some(s) = stat` → `discarded_unattached` arm, and all three of the `Open`/`Close`/`Error` match arms. Both crates even declare their own `struct ChannelStat`, and `exec`'s three fields (exec.rs:86-90) are exactly the three the shared block touches. `leg::route_recv` (leg.rs:903-976) is a third instance of the same rule with the same `scratch` trick, and it has already diverged: it mirrors `out.dropped_full` into a per-channel counter (leg.rs:937-939) while the other two do not.

**Failure scenario.** The hostward accounting rule is amended once — say `delivered_hostward` is made to exclude the bytes `FanOut::dropped_full` reports, the way `leg.rs` already treats them. Whoever changes it edits `codec.rs` (the canonical codec) and ships; `exec.rs` keeps the old arithmetic, so `state` reports different `delivered_hostward` for an in-process and an exec codec carrying byte-identical traffic, and nothing fails. That is the F1/DM-3 failure mode exactly: the loop existed five times and only one copy was right.

**Verification correction.** The duplication is real and is exactly a two-way verbatim clone: `nexus-daemon/src/nodes/codec.rs:404-434` (inside `hostward_demux`) and `nexus-daemon/src/nodes/exec.rs:636-665` (the non-mux `else` arm of `route_event`) are token-identical after comment/indent normalization except for one line — exec binds `let stat = stats.get(ev.channel.as_str());` inside the Data arm, codec binds it one level up at `codec.rs:401`. The `Open`/`Close`/`Error` arms are likewise the same modulo the tracing `target:` label and the inline vs. hoisted `stat` lookup. Three of the finder's supporting statements need correcting: (1) `leg.rs:931-939` is NOT evidence of accidental divergence — leg's counter is `discarded_hostward`, documented at `leg.rs:154-157` as covering *both* the full-buffer drop and the no-consumer-bound case, so folding `out.dropped_full` into it is that counter's definition, whereas codec/exec charge a narrower, differently-named `discarded_unattached`; leg is a third, differently-specified instance, not a drifted copy. (2) The fix must not be read as merging the `ChannelStat` structs…

**Fix.** Add `runtime::route_channel_data(bytes, feed: Option<&TapFeed>, sinks: Option<&SharedFanOut>, stat: Option<&dyn HostwardChannelStat>)` beside `fan_out`, with a three-method trait (`set_active`, `add_delivered`, `unattached() -> &dyn LossCounter`) implemented by both `ChannelStat`s. That deletes ~35 duplicated lines, removes one of the two `ChannelStat` declarations' reason to exist, and — the real point — makes "which counter gets which bytes" a thing with one implementation and one test. `leg.rs` can adopt it too by having its impl fold `dropped_full` in, which would also make its current divergence a visible property of one trait impl rather than an accident. Risk: near zero, because the two implementations are already identical; the leg's adoption is the only judgement call and can be deferred.

### `SIMP-2` — Targetward "charge every non-delivery exit" is hand-written at sixteen sites across five pumps, and has already shipped a missed exit twice

**🔵 low** · simplification · `nexus-daemon/src/nodes/codec.rs:511` · design §5 (all loss is counted where it happens); §16; AGENTS.md invariant 3's third clause ("count any residual") · verdict **CONFIRMED** (high confidence)

Every interior targetward pump implements the same contract by hand — obtain a writable origin (park / drain-and-count / forward), gate on the lock, send, and charge *every* failure exit to a per-node loss counter — and each exit's `counter.set(counter.get() + n)` is written out longhand. There are 16 such charge sites: codec.rs:511-515, 530-538, 542-550, 551-555; map.rs:392-398, 405-412, 417-419; leg.rs:1070-1075, 1111-1119, 1148-1159; exec.rs:620-623, 624-628, 629-634; pty.rs:645-648, 655-670, 759-764. Nothing in the type system requires an exit to charge anything, so a forgotten one is silent and invisible in `state`. `docs/implementation-notes.md` §3.18 records the targetward half as deliberately still per-node, which is what makes this the next §16 candidate rather than a recorded deviation to re-file.

**Failure scenario.** A new interior node kind (or a new exit added to an existing pump — e.g. a `write_mode` change detected mid-chunk) forwards targetward and returns on failure without charging. Bytes an operator's `send` was told were accepted vanish; `state` shows every counter at zero and every node `active`. Nothing in the suite fails, because each pump's loss counter is asserted only by that pump's own test. This is CODEXEC-2 recurring in a place review 26 did not happen to look.

**Verification correction.** The duplication is real and the finding understates it, but two clauses need correcting and one strengthening.

(1) **The site count is low and the location list is incomplete.** Beyond the 16 listed, `codec.rs:465-472` and `codec.rs:477-481` are two more hand-written charge loops (`channel_targetward_drain`, `mux_targetward_drain`), and the leg has at least five, not three: `leg.rs:955` (targetward handoff to a torn-down channel task), plus `note_undeliverable`/`Undeliverable` at `leg.rs:978-1010`, on top of 1073, 1115, 1154. Call it ~19-20 sites across five pumps.

(2) **The crate already contains the abstraction the finder proposes to build, and the targetward half simply does not use it.** `runtime.rs:302-324` defines `pub(crate) trait LossCounter { fn add(&self, n: u64); }` with `impl for AtomicU64` and `impl for Cell<u64>`, added for the hostward `fan_out` consolidation (F1/§16) and reachable from every node. Every one of the ~19 targetward sites charges a `Cell<u64>` or `Rc<Cell<u64>>` — types that already implement it — yet writes `c.set(c.get() + n)` longhand. `LossCounter` …

**Fix.** Two pieces, in order of payoff. (1) A `#[must_use]` `TargetwardLoss(u64)` returned by the send helper, matching `DataFrame::Residual`'s shape, so an uncharged exit is a compiler warning rather than an audit item. (2) `runtime::forward_targetward(edge: &SharedTargetEdge, chunk: Chunk, lost: &dyn LossCounter) -> bool` covering the three *unframed* pumps (map, exec's mux arm, pty), which are structurally identical today. Do **not** try to unify the framed pumps (codec, leg) into the same call: their park-vs-drain policies genuinely differ (leg.rs:1060-1069 documents why a leg drains where an interior node parks), and collapsing that would regress invariant 14. Keep the helper to the send-and-charge step only.

### `SIMP-3` — The PTY's blocking writer thread bypasses `boundary::BlockingReader`, so two of the daemon's three blocking threads are still hand-rolled — and `PtyNode::drop` re-derives `signal_stop` instead of calling it

**🔵 low** · simplification · `nexus-daemon/src/nodes/pty.rs:405` · design §16.1 ("Extract one supervisor abstraction … and rebase the three nodes onto it. The invariants stop being conventions the next node type must rediscover") · verdict **CONFIRMED** (high confidence)

`boundary::BlockingReader` (boundary.rs:136-213) exists to own exactly one pattern — an `Arc<AtomicBool>` stop flag, an `Option<JoinHandle<()>>`, `signal_stop`/`join`/`stop_join`, plus a `debug_assert` guarding re-arm and an `io::Result` on spawn so the caller can fault on EAGAIN. `PtyNode` re-implements all of it inline (`writer`/`writer_stop` fields at pty.rs:92-96; spawn + EAGAIN fault at pty.rs:321-335; `signal_stop` at 380-385; `teardown`'s join at 394-396; `Drop` at 405-419), and `LogNode` implements a third variant (log.rs:203-218, 292-340). Only `serial.rs` uses the library. Worse, `PtyNode::drop` copies `signal_stop`'s two statements rather than calling it — every sibling node's `Drop` either calls `signal_stop` (leg.rs:530-532) or is a two-line abort loop.

**Failure scenario.** Someone adds a step to PTY stop — say, a `writer_stop` counterpart that must also be set, or a second flag for the reader — and edits `signal_stop` and `teardown`. `Drop` keeps the old two-statement body, so a `PtyNode` dropped without teardown (the `load --replace` rollback path in daemon.rs:617-622 drops nodes after `teardown`, but a panic-unwind or a future `remove-node` refactor need not) skips the new step. The join-before-drop-fd ordering the module comment at pty.rs:388-392 relies on has no test that would notice.

**Verification correction.** `PtyNode` hand-rolls the stop-flag + join-handle lifecycle that `boundary::BlockingReader` already encodes and property-tests, and `PtyNode::drop` (pty.rs:406-417) re-derives `PtyNode::teardown`'s body — set `writer_stop`, abort `tasks`, join `writer`, unlink the symlink — instead of calling it, the way the sibling `LogNode::drop` (log.rs:343-349) calls `self.teardown()`. The minimal, uncontroversial fix is `impl Drop for PtyNode { fn drop(&mut self) { self.teardown(); } }` (teardown is idempotent and the two extra field writes it does — `master = None`, `symlink_installed = false` — are harmless during drop), which is strictly better than the finder's suggested `self.signal_stop()` because it also covers the join and the unlink.

Three of the finder's supporting claims are overstated and should not be carried into the report:
(1) "Only `serial.rs` uses the library" is wrong. `exec.rs` and `leg.rs` both use `boundary` (`race3`, `park`, `Backoff`, `drain_to_quiescence`); it is `BlockingReader` specifically that is serial-only, and that is because it is the only *hostward reader on a b…

**Fix.** Rename `BlockingReader` to something direction-neutral (`BlockingWorker` — its doc already describes only "a stop flag plus join handle so the supervisor joins the thread *before* dropping the fd", which is not reader-specific), make `lost` optional or leave it unused for the pty, and rebase `PtyNode` onto it. At minimum, make `PtyNode::drop` call `self.signal_stop()`. Risk: mechanical; the pty never re-arms its writer, so the `debug_assert` is a free extra guard rather than a behaviour change.

### `SIMPB-1` — Edge attachment is implemented twice, while edge detachment was deliberately consolidated into one helper

**🔵 low** · simplification · `nexus-daemon/src/daemon.rs:922` · design §15.35 (edge surgery), §16 one-rule-one-place, AGENTS invariant 12/14 · verdict **CONFIRMED** (high confidence)

`Daemon::connect` (daemon.rs:922-1065) and `Wiring::build`'s edge loop (runtime.rs:869-944) are two independent implementations of "attach one edge": derive the effective write mode, allocate an origin id, register it on the host endpoint's lock, derive `writer` from `mode != Never`, populate `origin_locks`, fill the target's `EdgeSlot` (`attached`/`registered`/`writer`), create the hostward channel at `hostward_depth`, attach an `AttachedSink` with the target's counters, and hand the receiver to the target's inbox. The *mirror* operation was consolidated: `GraphState::detach_edge_runtime` (daemon.rs:153) is shared by `disconnect` and `remove-node --cascade`, and its doc says why in so many words — "two implementations of 'leave the lock cleanly' is how the phantom holder came back the first time (§15.27, §16 one-rule-one-place)". The attach half never got the same treatment.

**Failure scenario.** The two copies already disagree in shape without (today) disagreeing in behaviour: `Wiring::build` inserts into `origin_locks` when `mode != WriteMode::Never` (runtime.rs:912-916), while `connect` inserts when `writer.is_some()` (daemon.rs:984-987), i.e. additionally conditioned on `endpoint_targetward` having an entry for the host. They coincide only because every host-facing endpoint currently gets a targetward sender in `Wiring::build`. Concretely: add a host-facing endpoint kind that does not get a targetward channel (or make `send`'s injection sender conditional) and a loaded edge becomes addressable by `lock`/`unlock` while the identical edge added by `connect` does not — a `lock <origin>` that works after a restart and fails after a live `connect`. More generally, any future change to how an edge is registered (a per-edge epoch, an `EdgeSlot` field, a second counter) has to be written twice or the two paths silently diverge, which is exactly the failure `detach_edge_runtime` exists to prevent on the other side.

**Verification correction.** The duplication is real and every cited line is accurate, but the "concrete" divergence in the failure scenario must be read as latent, not reachable. `Wiring::build` gates `origin_locks` on `mode != WriteMode::Never` (runtime.rs:912-916) while `connect` gates it on `writer.is_some()` (daemon.rs:984-987), i.e. additionally on `endpoint_targetward` holding the host — and those two maps are populated and pruned in strict lockstep at all four sites (`Wiring::build`'s facing loop inserts `endpoint_locks` and `host_targetward_tx` together, runtime.rs:820-833; `absorb_wiring` copies both, daemon.rs:271-276; `remove_node` removes both for the same endpoint list, daemon.rs:864-868; `teardown`/`load` clear both, daemon.rs:642-643 and 1842-1843). So no reachable configuration today makes a loaded edge `lock`-addressable and a connected one not; I confirmed the equivalence live. The finding's real content is the §16 one-rule-one-place / maintenance argument, and it holds: the assembly of an edge (mode derivation ordering, which conditions gate `origin_locks` vs `EdgeSlot.writer`, slot field pop…

**Fix.** Extract `runtime::attach_edge(parts: EdgeParts, mode: WriteMode, depth: usize) -> AttachOutcome`, where `EdgeParts` bundles the six handles both callers already hold (`SharedLock`, host targetward `Sender`, `SharedFanOut`, `EdgeInboxTx`, `SharedTargetEdge`, `Arc<DropCounters>`) plus the target `EndpointAddr` and an `OriginId`. `Wiring::build` calls it from its edge loop; `connect` calls it and then does its two extra, genuinely-connect-only steps (purge-on-acquire on a fresh held grant, `wake_waiters`/`emit_change`) on the returned outcome. Risk: low and mechanical — the handles are the same `Rc`/`Arc` values on both sides (`absorb_wiring` just re-keys them from `EndpointAddr` to display `String`); the only care needed is keeping `connect`'s `consumer_live` warning, which is a return value rather than a behaviour change.

### `SIMPB-2` — The §15.20 lost-wakeup lock-wait discipline is hand-written three times

**🔵 low** · simplification · `nexus-daemon/src/runtime.rs:166` · design §15.20 two-lane control plane, §15.23 held-priority reclaim, §16.1 · verdict **CONFIRMED** (high confidence)

Three functions implement the same five-clause parking protocol: (1) check `is_closed()` at the top of every iteration; (2) create the `Notified` and `.enable()` it *before* the acquisition attempt, or a wake landing in between is lost; (3) attempt the acquisition inside one synchronous `with_mut` critical section holding no borrow across the await; (4) `emit_change()` only on a *fresh* grant; (5) park on `notified.await` and loop. They are `runtime::reacquire_held` (runtime.rs:166-210, held-priority reclaim), `leg::ensure_acquired` (leg.rs:1209-1240, on-demand acquire), and `Daemon::wait_for_grant` (daemon.rs:1741-1791, on-demand acquire with a deadline and a `WaiterGuard`). The only thing that genuinely differs between them is the one-line attempt inside the critical section (`reclaim_held` vs `acquire`) and whether a deadline/dequeue guard is present.

**Failure scenario.** Clause (2) is the whole reason this shape is subtle, and it is upheld by three separate comments rather than by one function. A fourth writer kind (or a rework of one of these three) that writes `if lock.with(|g| …) { … } else { lock.notified().await }` — checking before enabling — parks forever on a lock that is free, because `Notify::notify_waiters` stores no permit. That is precisely the stranded-waiter/wedged-writer class §15.20 and §15.23 were written to close. The divergence has already begun: `reacquire_held` carries an "origin was unregistered by `disconnect`" exit (runtime.rs:181-183) with a five-line comment explaining that without it the loop is unreachable-forever; `ensure_acquired` has no such exit and is saved only incidentally, because `EndpointLock::acquire` happens to return `Acquire::ReadOnly` for an unknown id (lock.rs:194-198). If `acquire` ever returned `Denied` for an unregistered origin, `ensure_acquired` would enqueue an id that can never be granted and park a leg channel forever, while `reacquire_held` would exit cleanly.

**Verification correction.** The lost-wakeup park-and-retry shell (`is_closed` guard → `notified()`/`pin!`/`enable()` *before* the attempt → one synchronous `with_mut` attempt → `notified.await` and loop) is hand-written three times over the endpoint lock's `Notify`: `runtime::reacquire_held` (runtime.rs:166-210), `leg::ensure_acquired` (leg.rs:1209-1240) and `Daemon::wait_for_grant` (daemon.rs:1741-1791) — plus a fourth instance of the same enable-before-read discipline over a different `Notify` in `runtime::await_origin` (runtime.rs:517). The substantive duplication is the pair `ensure_acquired` / `wait_for_grant`: the same on-demand attempt (`acquire`, `enqueue` on `Denied`) with the same four arms, differing only in return type (`bool` vs `WaitOutcome`), the optional deadline, and the `WaiterGuard`. `reacquire_held` shares only the shell — it uses `reclaim_held`, never enqueues (held priority is by rule inside `acquire`, §15.23), and carries an extra `write_mode(id).is_none()` exit because `may_write`/`reclaim_held` return bare bools that cannot distinguish "denied now" from "never registered". Corrections t…

**Fix.** Add one helper beside `reacquire_held` in `runtime.rs`: `async fn await_grant<T>(lock: &SharedLock, id: OriginId, deadline: Option<Instant>, attempt: impl FnMut(&mut EndpointLock) -> Option<T>) -> GrantOutcome<T>`, owning clauses (1), (2), (3-shell) and (5). The three call sites keep their own `attempt` closure and their own post-grant handling (`emit_change`, `WaiterGuard`, the `write_mode(id).is_none()` exit becomes a shared clause rather than a per-copy one). Risk: moderate — `wait_for_grant` composes with `WaiterGuard` and a `select!` deadline, so the helper must return a value the guard can disarm on; keep `wait_for_grant`'s guard outside the helper rather than folding it in.

### `SIMPB-5` — `codec::channel_targetward_drain` and half of `drain_unwired_channels` are structurally unreachable, and their doc describes a case a different function handles

**🔵 low** · simplification · `nexus-daemon/src/nodes/codec.rs:465` · design §7.5, §15.8, review 26 MAP-1 (runtime) · verdict **CONFIRMED** (high confidence)

`CodecNode::drain_unwired_channels` (codec.rs:144-173) sweeps the multiplexed endpoint *and* every channel endpoint out of `wiring.host_targetward_rx`, routing channel receivers to `channel_targetward_drain` and the mux receiver to `mux_targetward_drain`. The channel arm cannot fire. `drain_unwired_channels` is called from exactly one place — the `self.faces != Facing::Target` branch of `start` (codec.rs:195) — and for a `faces = "host"` codec `NodeConfig::shape` (config.rs:816-829) makes the mux endpoint host-facing and every channel *target*-facing, while `Wiring::build` populates `host_targetward_rx` only for host-facing endpoints (runtime.rs:820-831). So the channel addresses are never present, `channel_targetward_drain` has no reachable caller, and it has no test. Its 8-line doc claims it serves "a codec whose multiplexed edge is `write_mode = \"never\"` (validation's documented read-only demux) or unattached" — that is the *demux* (`faces = target`) case, and it is handled by `channel_targetward`'s `await_origin` → `None` arm (codec.rs:511-515), not here. `ExecCodecNode::drain_unwired_channels` (exec.rs:174-196) is the same rule written a second time, and it is the honest shape: one drain, everything charged to the node's mux counter.

**Failure scenario.** A maintainer tracing the MAP-1 fix reads `channel_targetward_drain`'s doc, believes the read-only-demux case is handled there, and "simplifies" `channel_targetward`'s `await_origin`-returns-`None` arm — reintroducing the exact defect (a channel receiver dropped under live senders → a pty origin's write fails → `read_and_poll` returns → presence latching, `handle_last_close`, termios reconciliation and detach-release all stop) that the ledger records as the most serious item found by review 26's completeness audit. The dead branch is not itself harmful; the misattributed doc in the one module whose comments are the project's main defence against MAP-1 recurring is.

**Verification correction.** The finding is right, and its provenance makes it stronger and cheaper to fix than stated: `channel_targetward_drain` is not dead-by-design, it is **dead-by-regression from v12**. At `d7d840f` (the review-26 remediation that introduced `drain_unwired_channels`) `CodecNode::start` had **two** early returns — the `faces=host` one, and `let Some(mux_hostward_rx) = wiring.target_hostward_rx.remove(&mux) else { … self.drain_unwired_channels(wiring); return; }` for a demux with no attached upstream. That second exit is a `faces = "target"` codec, whose channels *are* host-facing, so the channel arm and `channel_targetward_drain` were genuinely live, and the doc was accurate. Commit `548823e` (v12 §15.35 edge surgery) removed that exit — the mux-unattached case now only sets `Waiting` and falls through, because every task must start parked so a later `connect` needs no restart (invariant 14) — leaving `faces=host` as the only caller and the channel arm unreachable. So three comments are stale, not one: (1) `channel_targetward_drain`'s 8-line doc (codec.rs:455-464), which describes the read-…

**Fix.** Keep the defensive sweep (it is cheap insurance against a future early return in `start`), but make the reachability honest and stop the two copies from disagreeing: collapse codec's version onto exec's shape — one drain task charging the node's `mux_discarded_targetward` — and delete `channel_targetward_drain` and its doc, or, if per-channel attribution is wanted for a future orientation, keep it and rewrite the doc to say plainly that it covers the *re-multiplexer* orientation and that the read-only demux is handled by `channel_targetward`. Risk: near-zero (dead branch); the only care is not weakening `channel_targetward`'s live drain arm, which `p9_unwired_interior.rs` pins.

### `SIMP-6` — daemon.rs extracts param and error helpers but only half the call sites adopt them, so the validate-error block appears three times verbatim and the same error code ships two different message shapes

**⚪ nit** · simplification · `nexus-daemon/src/daemon.rs:573` · design §16 ("one rule, one place"); §16.8 (the error-code registry) · verdict **CONFIRMED** (high confidence)

Three verbatim copies of the same eight-line block turn `GraphConfig::validate`'s errors into an `app_errors::STRUCTURAL` `RpcError` — daemon.rs:573-581 (`load`), 733-741 (`add_node`), 928-936 (`connect`) — while a `structural_error()` helper sits at daemon.rs:2050-2057 building the *same code* with a differently-shaped `message` (no `"structural error: "` prefix). Separately, the params helpers `node_param`/`bool_param`/`u64_param`/`origin_param` (daemon.rs:2160-2191) are used by the three serial-signal verbs and bypassed by `remove_node` (daemon.rs:777-788), `rotate` (1350-1354) and `tap_open` (1214-1223), which hand-roll the identical `and_then(|p| p.get(k)).and_then(Value::as_*)` chains.

**Failure scenario.** A client (or the editor page, which renders `res.error.message` verbatim per app.js:88-91) matches on the `-32002` message prefix to distinguish a graph-rule violation from a codec-schema violation. It works for `load`/`add-node`/`connect` and silently fails for the codec precheck path, because the same code carries a different message shape. Separately, when the structural-error `data` contract grows a field (§11 already carries `errors` and, for codecs, `available`), whoever adds it edits one or two of the three copies.

**Verification correction.** The duplication and the divergent message shape are real and reproduced; the *client-parses-the-prefix* half of the failure scenario is invented and should be dropped from the write-up.

What is actually true:
- Three verbatim copies of the same eight-line block turn `GraphConfig::validate()`'s errors into an `app_errors::STRUCTURAL` `RpcError` with `data.errors = [all messages]` and `message = format!("structural error: {}", messages[0])`: `nexus-daemon/src/daemon.rs:574-581` (`load`), `734-741` (`add_node`), `929-936` (`connect`). They differ only in indentation and in `config.validate()` vs `candidate.validate()`. No borrow or lifetime obstacle blocks a shared `fn structural_errors(errors: &[ValidationError]) -> Option<RpcError>` — two sites are inside `state.with_mut` closures, one is not, and all three return `Result<_, RpcError>`.
- A fourth site, `structural_error` (`daemon.rs:2050-2057`, used only by `precheck_codecs` at 545/553/558), emits the *same* code `-32002` with the raw message and no prefix. Verified live against a daemon at HEAD `cfb2187`: the same `-32002` carries …

**Fix.** Give `structural_error` a second constructor — `structural_errors(msgs: &[String])` — that owns the `format!("structural error: {}", msgs[0])` + `data.errors` shape, and call it from all three sites; decide once whether the codec precheck should share the prefix. Adopt `node_param`/`bool_param` at the three bypassing call sites, and add a `str_param(params, key)` for `endpoint`/`tap`. Risk: `p9_config_validation.rs` and `p5_bad_attributes.rs` assert on these messages, so the prefix decision is a deliberate one-line contract change, not a silent cleanup.

### `SIMPB-10` — `for t in self.tasks.drain(..) { t.abort(); }` is written eleven times, and three nodes re-derive `signal_stop` inside `Drop` where a fourth already calls it

**⚪ nit** · simplification · `nexus-daemon/src/nodes/codec.rs:358` · design §16.1, review 26 BND-1 · verdict **CONFIRMED** (high confidence)

Six node modules carry the same three-line abort loop eleven times (codec.rs:359 and 371, exec.rs:354 and 366, map.rs:317 and 329, serial.rs:267 and 596, pty.rs:382 and 408, leg.rs:518). For codec, exec and map the picture is starkest: `teardown` is literally `self.signal_stop()`, and `Drop` re-derives `signal_stop`'s body instead of calling it — while `LegNode`'s `Drop` (leg.rs:529-533) does call `self.signal_stop()`, showing the shape is available and simply not used. §16.1 extracted the *supervisor* lifecycle (park, race3, `BlockingReader`, `Backoff`) into `boundary.rs` but left the task-set half per node.

**Failure scenario.** A new node kind (or a new task added to an existing one) that forgets its `impl Drop` leaves spawned `LocalSet` tasks running against `Rc` state after the node value is gone — they keep the endpoint's `Rc<EdgeSlot>`/`SharedLock` alive and keep draining channels a torn-down node should have released. Nothing in the type system, clippy or the test suite says a node must have a `Drop`; it is a convention held by six copies. The same class already bit this project once as `signal_stop`/`teardown`/`Drop` drift (BND-1).

**Verification correction.** The duplication is real and slightly larger than filed, but the failure scenario is prospective, not live. Corrections: (a) there are **seven** node kinds with an `impl Drop`, not six — `log.rs:343` has one too, and it is a *fourth* shape (`if self.pump.is_some() || self.writer.is_some() { self.teardown(); }`, i.e. it delegates to the blocking teardown). So the real content is not just "eleven copies of a loop" but a four-way divergence in how `Drop` relates to `signal_stop`/`teardown`: codec/exec/map re-derive `signal_stop`'s body verbatim; leg delegates to `signal_stop`; log delegates to `teardown`; serial/pty re-derive a teardown-shaped sequence by hand. (b) No node currently leaks: every production drop path calls `signal_stop` then `teardown` before the value dies — `load` rollback (daemon.rs:617-622), `remove-node` (daemon.rs:848-850), `teardown_with` (daemon.rs:1833-1838) — and `add_node` has no post-instantiate error return. The `Drop` impls are backstops (they matter for unit tests that construct a node without tearing it down, and for unwind paths), so this must be filed as…

**Fix.** Add `struct TaskSet(Vec<JoinHandle<()>>)` to `boundary.rs` with `push`, `abort_all(&mut self)` and `impl Drop for TaskSet { fn drop(&mut self) { self.abort_all() } }`, and give each node a `tasks: TaskSet` field. Every node's `impl Drop` for the task half then disappears (serial and pty keep theirs for the reader-thread join, which is genuinely more than an abort), `signal_stop` becomes `self.tasks.abort_all()`, and "a node's tasks die with the node" becomes a type property instead of six conventions. Risk: low — verify `SerialNode::drop` still joins the reader (`stop_join_reader`) *after* the task aborts, since field drop order would otherwise change the sequence.

### `SIMPB-8` — The web bridge hard-codes JSON-RPC error numbers instead of the §16.8 registry it already depends on

**⚪ nit** · simplification · `serialnexusweb/src/bridge.rs:198` · design §16.8 · verdict **CONFIRMED** (high confidence)

`bridge::screen` writes the literals `-32600` and `-32601` at four production sites (bridge.rs:198, 206, 212, 218) and five more in tests, while `nexus_rpc::error_codes::{INVALID_REQUEST, METHOD_NOT_FOUND}` (nexus-rpc/src/lib.rs:272, 274) are the single registry §16.8 established for exactly these values — and `serialnexusweb` already depends on `nexus-rpc` (it calls `nexus_rpc::base64_encode` at server.rs:392). The daemon side uses the registry consistently (`app_errors` in daemon.rs:63-70 are const projections of `AppError`); the bridge is the one JSON-RPC-emitting surface that does not.

**Failure scenario.** The bridge's rejections are the only JSON-RPC errors in the system that are not derived from the registry, so `grep -rn METHOD_NOT_FOUND` — the natural way to answer "where can a -32601 come from, and is it documented in docs/rpc?" — misses the browser boundary entirely. That matters here more than usual: this is the surface a compromised page hits, so its refusal codes are the ones a security reader most wants to enumerate.

**Verification correction.** `bridge::screen` writes the raw literals -32600 (bridge.rs:198, 206, 212) and -32601 (bridge.rs:218) while `nexus_rpc::error_codes::{INVALID_REQUEST, METHOD_NOT_FOUND}` (nexus-rpc/src/lib.rs:272, 274) name exactly those values, and `serialnexusweb` already depends on `nexus-rpc` as a normal dependency (Cargo.toml:27, used at server.rs:392 / wsclient.rs:160). `bridge.rs` is the only JSON-RPC-error-emitting site in the workspace that does not name the constant — control.rs:181/318, daemon.rs:507/2199/2258 and serialnexusctl/src/main.rs:214 all do. Correction to the finder's design ref: §16.8's "single registry" is `AppError` (the application range), not the standard-code module; the `error_codes` constants predate it. So the argument is tree-wide convention and greppability, not a §16.8 requirement — which leaves this a genuine but purely cosmetic nit. A fuller cleanup would also route the hand-built `rpc_error` object through `nexus_rpc::Response::error`, but the minimal constant substitution is the safe change and preserves the wire bytes exactly.

**Fix.** `use nexus_rpc::error_codes::{INVALID_REQUEST, METHOD_NOT_FOUND};` and substitute at the four production sites. Leave the raw numbers in the tests — asserting on the literal is the point of a wire-contract test. Risk: none.

---

## 6. Verified and cleared

### 6a. Refuted (10) — do not re-file

Each of these was filed by a finder, attacked by a verifier, and died. Recorded with *why* so the next
review does not spend the same effort.

| Finding | Claimed | Why it fell |
| --- | --- | --- |
| `CORE-1` | `load` never validates resolver-input well-formedness, so a typo'd identity replaces a running graph | Facts right, consequence wrong — **and the proposed fix would regress the design.** §12's asymmetry is deliberate: identity-form input never requires the device present, which is why `dump` emits identities and why configs survive cold starts with hardware unplugged. `load` resolving would break that. The destructive-typo path it worried about is already closed by `deny_unknown_fields` (`-32602` before any teardown). |
| `LOCK-1` | `EndpointLock::register` over a live registration breaks holder-may-write | Facts right and the misuse reproduces *against the pure API*, but unreachable from the daemon: `SEND_ORIGIN_BASE` and `next_edge_origin`'s floor (taken from `Wiring`'s counter) both exist to prevent id collision, with comments saying so, and no verb mutates a registered origin's mode. The other `register` call sites are `#[cfg(test)]`. |
| `DOC-1` | `revents_label`'s unknown-bit branch is unreachable because `poll_ready` collapses an unmodelled flag to `empty()` | Facts right about nix; consequence wrong. `poll(2)` masks `revents` to `requested \| POLLERR \| POLLHUP \| POLLNVAL`. The verifier measured it: `events=POLLIN` on a hung-up socketpair gives `revents=0x0011` with POLLRDHUP absent; ask for POLLRDHUP and it appears. Every call site builds its interest from nix-modelled flags, so the `None` arm cannot be reached. |
| `SIMP-4` | `validate`'s `if let` chain means invariant 13 is prose the compiler cannot enforce | Consequence wrong, settled by experiment on a *copy* of the tree: adding a new `NodeConfig` variant produces four `E0004` non-exhaustive-match errors **in `config.rs` itself**, and adding a numeric field to an existing variant fails `cargo test --workspace` at seven initializer sites including the range-check test. The compiler does route the author to the right file. |
| `ITEST-2` | `tap.open`'s `epoch` has zero coverage in `nexus-itest` | The *fact* is right (0 grep hits) and the *consequence* is wrong: `p8_tap_offsets.rs:557` already asserts `tap.closed` with reason `"graph replaced"` — emitted only when the hub is dropped — **and** `from_offset == 0` on the reopen, which catches hub reuse and offset restart from opposite directions. (A direct epoch assertion is still cheap and worth adding; see `ITEST-4`'s neighbourhood. It is an improvement, not a hole.) |
| `ITEST-3` | The `web-ui` CI gate flakes ~1/18 runs with `retries: 0` | Could not be reproduced in **100 consecutive gate runs**, 30 of them CPU-constrained at loads (1.13–5.82) consistently worse than the finder's reported 0.15–0.69 — and a hang-shaped failure should get *more* likely under contention. The finder's two observations remain unexplained; if CI ever shows this, treat it as the residual load-sensitive lead §15.36 already records rather than as a new mechanism, and capture the failing spec name before re-running. |
| `DOCR-7` | `nexus-daemon`'s crate rustdoc says the entry surface is "the only public API" while `unstable_fuzz_api` is `pub` | Facts right, consequence wrong. The scenario needs an embedder to find the module *through rustdoc's module list* and still believe the crate doc — but rustdoc renders the module's own first-line disclaimer right there in that list. Notes §3.19 already records the exception and its rule. |
| `DOCR-8` | `serialnexusweb` has no user-facing documentation; packaging never mentions it | **Facts wrong.** `packaging/README.md:135-140` carries a dedicated web-console bullet under the operator-facing "what changed for operators" heading, including the treat-the-token-as-shell-access warning the finding said was missing. The finder had grepped only the binary token `serialnexusweb`; the document names it in prose. Grep-token blindness. |
| `SIMPB-11` | The Linux-only serial-skip rule is re-derived with cfg+allow fixtures in six test files | **Facts wrong.** That shape exists in exactly one file (`p3_counters.rs`). The other five use an allow-free `#[cfg] mod linux_impl { … }` idiom. |
| `RV-2` (reviewer's own) | The §11 empty-parse refusal exists only in the CLI, so a direct-RPC `load --replace` empties a running graph | Three kills. The rule's predicate is `config.is_empty() && !text.trim().is_empty()` — it needs the **source text**, which the RPC verb never receives; `config.rs`, `graph.rs` and `main.rs` all say so in as many words. The destructive-typo path (`[[nodez]]`) is refused over RPC by `deny_unknown_fields` with `-32602` *before* any teardown (verified live; the graph survived). What remains is a client deliberately sending an empty config, which is a legitimate operator act. |

### 6b. Already known (2)

| Finding | Disposition |
| --- | --- |
| `EXEC-1` | The exec stdin feed losing an in-flight chunk uncounted was verified and cleared in review 26; the code facts are unchanged. |
| `SIMP-5` | `Daemon::state()`'s per-node endpoint rescan is review 19's `OPSIMP-3`, recorded in notes §3.14 as a deliberate keep. |

### 6c. Checked and found sound

Recorded so the next review starts from here rather than re-deriving it. Sources: the sixteen finder
area summaries and the reviewer's own live probes.

**The v13 work itself.** The `epoch` machinery is sound: a new epoch is minted exactly once per
`TapHub::new`, `ingested` starts at 0 in the same constructor so epoch↔offset-space is 1:1 *by
construction*, and `NEXT_HUB_EPOCH` is process-global and never reused. Confirmed live from two
directions: `load --replace` emits `tap.closed` and mints a fresh epoch with `from_offset: 0` (2→3→4→5
in one process), while `add-node`, `connect`, `disconnect` and `remove-node` of an unrelated node leave
a tapped endpoint's epoch **and** offsets untouched. There is no path that reuses a hub while resetting
offsets, or mints an epoch while continuing an offset space. The client half — `offsetSpaceChanged` /
`reanchor`, the `SNXHIST2` header and its untagged-record-is-absent migration, `saver.mjs`'s per-key
serialization, and the microtask ordering that makes `currentTap` live before the first `tap.data` —
is also sound. The `TIOCEXCL` release works correctly for the teardown path §15.38 wrote it for.

**`tap.closed` fires on every mutation path** with the documented reasons — `load --replace` →
`"graph replaced"`, `remove-node --cascade` → `"endpoint removed"`, `teardown` → `"teardown"` — and
`tap.close` of a daemon-closed tap answers `-32602 "tap N was already closed by the daemon…"`. *(An
early reviewer run suggested otherwise; that was a buffered-`readline` bug in the checker, not the
product. Recorded because it is §15.36's lesson in miniature: audit the harness before believing a
negative.)*

**Invariant 11 (the web bridge) held under attack.** Parse-one-forward-re-serialized survived a
fragmented two-request frame, duplicate `"method"` keys, batches, scalars and binary frames;
`load`/`teardown`/`shutdown`/`set-attribute` were each refused `-32601` with the graph intact. Host,
Origin (sibling port, `null`, evil host, absent) and the bootstrap/cookie token gates all behaved as
documented; 60 aborted WS sessions leaked no fds. Reviewer's own smoke test: no token → 401, wrong
token → 401, wrong Host → 403, bootstrap → 302, non-loopback plaintext bind refused with the §15.29
message. Token comparison goes through `ct_eq`.

**XSS is clean.** Every insertion point uses `textContent`/`createTextNode`; the only `innerHTML` is
`consolesEl.innerHTML = ""`; `opt.value` / `a.download` are property assignments with a sanitiser.

**`nexus-sys`'s unsafe is sound.** Correct ioctl request codes and argument types,
`serial_icounter_struct` layout matches, packed-`epoll_event` reads are field copies not references,
`Epoll` cannot leak its fd, `ptsname`'s non-reentrant arm is serialized.

**`codec-api`'s decode surface holds.** Every length/bounds check on `try_decode`, `try_decode_hello`
and `FrameDecoder` is correct; no truncating cast is reachable (`body_len ≤ 65536` bounds every
downstream arithmetic); the `count`-driven allocation is clamped to received bytes; magic and version
are validated before any v1-specific field; encoders append nothing when they refuse; encode/decode are
symmetric at the exact `MAX_FRAME_SIZE` boundary. The envelope golden vectors are asserted for real,
and the hello layout is pinned field-by-field by `hello_header` /
`hello_version_mismatch_is_refused_with_value` despite having no *named* golden vector. `nexus-rpc`'s
hand-rolled base64 rejects every malformed shape constructed against it, and the `AppError` registry
really is the sole source of every code the daemon emits.

**The pure state machines.** `lock.rs`: no reachable sequence breaks single-holder, FIFO fairness,
held priority, generation guarding or purge-on-acquire; the wake path is `notify_waiters` (all
waiters), so there is no lost-wakeup or head-starvation hole. `map.rs`: every picocom rule matches the
upstream oracle, including the corrected `spchex` control class, `nrmhex`'s inclusion of SPACE and
`8bithex`'s `c & 0x80`. The validator: `deny_unknown_fields` covers every config struct; name legality
(`/`, empty, blank, 256-byte cap) is enforced on node names *and* channel identities; invariant 12's
single promotion helper genuinely has exactly three callers that all agree; cycle detection handles
self-loops and codec/map nodes.

Four hypotheses were **tried and refuted by the finders themselves** before reaching a verifier: TOML
value-after-table mis-serialization of a codec `attributes` table (toml 1.1.3 emits values before
sub-tables — verified with a compiled probe); dump/load round-trip loss (a config exercising all six
node kinds, every attribute, a nested `env` table, a float and a TOML datetime round-tripped
byte-identically through the JSON hop); `advertised_baud` lacking a range check (§7.2 specifies
skip-rather-than-approximate, and `standard_baud` does exactly that); and node-level cycle-arc
coarseness producing false positives.

**The review-26 remediation held.** All five structural refusals re-verified live, each naming its
offender: a codec mux edge with the default `write_mode`; two `held` origins on one endpoint (naming
*both*); `serial faces = "target"`; `replay_ring = 1152921504606846976`; and a whitespace-only node
name. Also re-verified: `unix` leg listener socket **0600** (SEC-2), state file 0600 (SEC-4), control
socket 0600, log file 0640; `set-attribute` → `-32601`; `existing-terminal` → structural refusal
listing the valid kinds; unknown key `advertized_baud` → `-32602` naming the field.

**Verb-layer and lifecycle.** `load`/`add-node`/`connect` all validate the candidate graph before
mutating anything, and `load --replace` cannot reach a teardown on a structural error —
`Node::instantiate`'s only `Err` arms are each covered by `precheck_codecs` or `GraphConfig::validate`,
so the post-teardown abort path is genuinely unreachable. The §15.27 phantom-holder class is clean
under connect/disconnect cycling, `remove-node --cascade` in either orientation, and
`add-node`+`connect`+`remove-node` round trips. `is_config_mutation` covers `connect`/`disconnect`. A
24-case hostile-params sweep produced no panic and no half-mutation. The `CriticalCell` re-entrancy
audit came back clean, and invariant 5's scope is correct — `CriticalCell` appears only in
`nexus-daemon`, and both `clippy.toml` files exist. The leg's `unix` socket is unlinked on teardown and
the same address re-loads cleanly; a log node with an unwritable directory faults with a precise reason
and creates nothing; a standalone `faces = host` codec comes up `waiting` with the §14 reason; pty
symlinks are unlinked on clean shutdown for healthy *and* faulted nodes.

**Harness quality.** The meta-gates all plant a violation and prove their own detector fires; the fuzz
targets assert real invariants rather than only absence-of-panic; oracles are independently
reimplemented (`p8_map`); the `Ws` fill-then-commit guards are exemplary. `scripts/` is gone and no
`.sh` remains anywhere in the tree, as AGENTS.md claims. The conformance kit's negative self-tests do
run under `cargo test --workspace`. `EdgeSlot`/`EdgeInbox`/`FanOutList` implement invariant 14's three
states faithfully with no lost-wakeup windows, and `RequestLines` is genuinely cancel-safe and bounded.

---

## 7. Suggested order of work

Grouped so each item is a structural fix rather than a patch, and ordered by "what would I be unhappy
to ship another release without".

1. **Give exclusivity an owner** (`RV-8`, `CONC-4`, `SERX-1`, and `SERX-2`'s successor-line half). Wrap
   the `SerialPort` so `Drop` returns the `TIOCEXCL` claim, and the four exit paths stop being four
   things to remember. This is the one confirmed defect that leaves a device permanently unusable.
2. **Make the resolver's two directions read one source** (`RES-1`, `RES-2`, and `RES-3`'s
   canonicalization). Count ambiguity over sysfs tty devices; give `find_usb` and `enumerate_ports` a
   sysfs fallback. Fix the fixture in `duplicated_serial_degrades_to_by_path` at the same time — it is
   currently the reason the guard's own hazard has never been tested.
3. **Stop the waiting lane from starving the streaming lane** (`CTRLW-1`, `CONC-2`, `CONC-1`).
   Restructure `serve_connection` around one `select!` in which dispatch is an optional arm, carry
   `send`'s deadline into the delivery (`mpsc::Sender::send` is cancel-safe, so the byte count stays
   exact), and stop the pty reader parking its own lifecycle block.
4. **Close the §5 accounting holes** (`LEG-1`, `CODEC-1`, `LEG-2`, `LEGD-2`, `WIRE-1`, `LOGQ-1`).
   `SIMP-2` is the structural version of the same item: sixteen hand-written "charge every
   non-delivery exit" sites across five pumps is how two of these shipped.
5. **The TLS key destruction** (`WEB-1`, `WEB-2`). `create_new(true)` fixes both, and refusing a
   half-present pair is a two-line guard.
6. **The pty symlink publish order** (`RV-1`) — install last, after the pair is fully usable.
7. **The browser history cluster** (`HIST-1`, `HIST-3`, `HIST-2`, `HIST-4`, `HIST-5`). `HIST-1` first:
   the gap is already computed and thrown away.
8. **The gates that do not gate** (`TESTR-2`, `ITEST-1`, `TESTR-7`, `ITEST-4`). Do `TESTR-2` first: it
   is the executable proof behind §13's licensing policy and it currently passes on a `cargo metadata`
   failure, which was demonstrated by deleting the ban entry. `ITEST-1` is next because it is a
   regression guard for *this release's own* fix and cannot fail. Each of these is a few lines; each
   restores a claim the project already counts as coverage.
9. **Documentation**, weighted toward what a newcomer reads first — `DOCR-1` (README's index links two
   files that do not exist and names v12 as normative), `DOCR-4`/`DOCR-5` (AGENTS.md contradicting its
   own invariant 11 and naming the wrong design generation), then `DOCR-2`/`DOCR-3` (the doctor page).
   `DOCE-1` should ride along with the next design touch, because AGENTS.md tells the next session the
   design wins on disagreement — so a wrong sentence there propagates into code.
10. Clarity items as convenient. `SIMP-2` is the exception and belongs with item 4, not here.

---

## 8. Reproduction index

Every high and medium finding above carries the verifier's own reproduction inline (in a `<details>`
block), produced independently of the finder's. All reproductions ran on the Linux 7.0 dev box against
`cargo build --workspace --locked` artifacts at `cfb2187`, in short `mktemp -d /tmp/…` runtime
directories, and every daemon, sim and web server started was killed. The tree was verified unmodified
(`git status` clean apart from the pre-existing untracked `.claude/`) at the start of the review, after
the finder pass, and after the last verdict.

**A note on reading the reproductions.** Several are transcripts of a *controlled A/B* — the same graph
with one attribute changed — rather than a single run, because for this system that is usually the only
way to separate the defect from the environment. Where a verifier's own reproduction disagrees with the
finder's, the verifier's is the one shown, and the disagreement is stated in the **Verification
correction** line rather than smoothed over.

One reproduction in this review was itself wrong and is recorded in §6c: an early "the daemon never
sends `tap.closed`" result turned out to be a buffered-`readline` bug in the checker. It is called out
because it is §15.36's own lesson arriving on schedule — audit the harness before believing a negative
— and because the review's verification rules were tightened for it (rule 4: dump the raw bytes before
concluding "it does not happen").
