# serial_nexus — Comprehensive Code Review (review 37)

**Date:** 2026-07-29. **Tree:** branch `implementation` at commit `d4743f9` (clean working tree; the tree did not move between finding and verification). **Normative pair:** `docs/35-design-claude-fable-v14.md` + `docs/36-implementation-plan-claude-fable-v14.md`.
**Scope:** correctness, reliability, design conformance (deviations split into fix-vs-justify), test coverage, documentation, and clarity/simplification — across every workspace crate, the web assets, the harness, the packaging, and the documents.
**Outcome:** 82 findings (13 major, 55 minor, 14 nit; two raw findings describing one defect are merged as 37-DATA-1). **No critical findings.** The pure state machines produced no confirmed defect for the fourth review running.

## 1. Method

Sixteen area reviewers read the tree in parallel — serial node + sys, PTY node, log/map, codec/exec/reference/codec-api, leg/wire, data-plane runtime, lock machinery, graph/config, resolver, control plane/taps, daemon lifecycle, web server, web client JS, ctl/sim/doctor, harness quality, and documentation — each primed with the design sections governing its area and the standing do-not-refile set (implementation-notes §3.1–§3.22, remediation ledgers 27 and 33, review 32 §6, and the cleared-candidate tables of reviews 19 and 26). Every candidate that survived the finder's own screen went to an independent adversarial verifier that received **only the claim and the tree** — never the finder's reasoning — per AGENTS.md §9, with the tree frozen for the whole find-and-verify span. A completeness critic then reviewed area coverage and dispatched three targeted gap reviewers (the daemon process startup/shutdown seam, packaging semantics vs code, and the external-codec Python fixtures), whose findings went through the same verification.

Raw candidates: 77 from the area pass plus 9 from the gap pass. Every verified candidate came back CONFIRMED; none was refuted at the verification stage. That zero is a property of the filing bar, not of the verifiers: finders were instructed to file only what they would defend, and their summaries record candidates killed before filing (see §6). Five of the highest-impact verdicts (37-LEG-1, 37-LOCK-1, 37-RES-1, 37-CTRL-1, 37-CTRL-2) were additionally re-derived by hand against the code before this document was written, as were the three verdicts produced while a reviewing check was degraded (37-CODEC-2, 37-CODEC-4, 37-LEG-4).

## 2. Gate verification (all green, claims exact)

Run on this tree before the review started, on a quiet box (load 0.19, 8 cores):

- `cargo build --workspace --locked` — clean.
- `cargo test --workspace --locked` — **102 targets, 642 passed, 0 failed, 4 ignored**: exactly the AGENTS.md §2 / implementation-notes §1 claim.
- `cargo fmt --all --check` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean; the minimal-daemon clippy (`-p serial-nexus-daemon-bin -p serial-nexus-daemon --no-default-features`) — clean.
- `cargo deny check licenses bans sources` — ok (two unmatched-allowance warnings: `Unicode-DFS-2016` and `Zlib` are allowed but no longer encountered; cosmetic).
- `cargo check --target x86_64-apple-darwin --workspace --exclude serial-nexus-web --locked` — clean.
- `serial-nexus-doctor --json | jq -f expectations/linux.jq` — `true`.
- The Playwright gate ran inside `cargo test` (node present).

## 3. The shape of the findings

The review-32 observation holds again: defects cluster in **accounting edges, client halves, error paths, and the documents** — not in the model. The graph validator, the lock state machine, the map oracle, the fragmentation helper, and the resolver's identity core survived adversarial reading intact. What did not:

1. **Purge and reclaim integration seams.** The two worst correctness findings are both places where a design §6 promise is carried by one integration path and silently absent from another: the leg's receiving-side purge cannot see a backlog whose peer disconnected in the same poll (37-LEG-1), and a `held` PTY origin has no reclaim driver at all (37-LOCK-1).
2. **The one-source rule, one door short.** The RES-1/RES-2 remediation moved `find_usb` and `enumerate_ports` onto the device listing, but bare-serial input still reads only the by-id tree (37-RES-1) — the same failure class, in the arm nobody tested.
3. **Terminal-event delivery under pressure.** `tap.closed` — the review-26 hardening that mutation never silently orphans an observer — is dropped exactly when the observer is busiest (37-CTRL-1), and a write-stalled control connection can park a FIFO-head waiter forever (37-CTRL-2).
4. **Documentation staleness from the last two sessions.** The 2026-07-29 dependency/rig session updated some claim sites and not others (37-DOC-1, 37-DOC-2); three sites still describe a Linux rig auto-detect that does not exist (37-DOC-3).
5. **Deployment surfaces no test can reach.** Both packaging majors (37-PKG-1, 37-PKG-2) are semantic mismatches between the systemd unit's environment and the daemon's assumptions — exactly the class the gap pass was dispatched to find.

## 4. Findings

Index, most severe first within each area. Category abbreviations: corr = correctness, rel = reliability, dev = deviation, test = testing, docs = docs, simp = simplification. Disposition: **fix** = code (or the named document) should change; **justify** = code is right, recorded in implementation-notes §3.

| id | sev | cat | where | title |
|---|---|---|---|---|
| 37-LEG-1 | major | corr | `daemon/src/nodes/leg.rs:1262` | Receiving-side purge misses the backlog of a peer that sends and disconnects in one poll |
| 37-LOCK-1 | major | corr | `daemon/src/nodes/pty.rs:901` | A held pty origin never reclaims a freed lock; the endpoint wedges |
| 37-RES-1 | major | corr | `core/src/resolver.rs:371` | Bare-serial input reads only by-id, refusing a present adapter in a no-udev environment |
| 37-CTRL-1 | major | dev | `daemon/src/tap.rs:608` | Terminal `tap.closed` is dropped when the tap queue is full |
| 37-CTRL-2 | major | rel | `daemon/src/control.rs:469` | A write-stalled connection at the FIFO head stalls the endpoint's arbitration |
| 37-TOOL-1 | major | rel | `ctl/src/main.rs:838` | `ctl tap --bytes N` exits 0 on a truncated capture at clean EOF |
| 37-TEST-1 | major | test | `itest/tests/meta_gates.rs:258` | Three tree-scanning meta-gates prove their matcher but never their walker |
| 37-SEAM-1 | major | test | `itest/src/lib.rs:537` | No test exercises the SIGTERM/SIGINT/shutdown clean-exit path |
| 37-PKG-1 | major | corr | `packaging/README.md:141` | Packaged-unit upgrade drops incremental surgery: legacy state adoption never applies to the unit |
| 37-PKG-2 | major | corr | `packaging/serial-nexus-daemon.service:42` | The `--socket-group` recipe fails under `DynamicUser`: operators get no directory traversal |
| 37-DOC-1 | major | docs | `docs/serial-nexus-doctor.md:256` | Doctor reference still calls the 7.0 evidence passive-only, contradicting the committed Tier-3 artifact |
| 37-DOC-2 | major | docs | `docs/implementation-notes.md:2739` | Notes §1 kernel matrix contradicts the same file's newest session entry |
| 37-DOC-3 | major | docs | `docs/macos.md:87` | Three sites claim a Linux by-id rig auto-detect that `crossover_ports()` does not have |
| 37-SER-1 | minor | corr | `daemon/src/nodes/serial.rs:987` | Concurrent line-holding signal verbs silently clobber each other |
| 37-SER-2 | minor | corr | `daemon/src/daemon.rs:1367` | A mistyped `ms`/`assert` RPC param silently becomes the verb default |
| 37-SER-3 | nit | docs | `sys/src/lib.rs:186` | Docs claim serial2 opens the fd blocking; serial2 0.2.37 opens `O_NONBLOCK` |
| 37-PTY-1 | minor | rel | `daemon/src/nodes/pty.rs:197` | PTY master fd lacks CLOEXEC and is inherited by exec-codec children |
| 37-PTY-2 | minor | corr | `daemon/src/nodes/pty.rs:254` | `apply_perms` widens the slave's mode before chown |
| 37-PTY-3 | minor | test | `daemon/src/nodes/pty.rs:1276` | §7.2's EXTPROC re-assert after a client clears the flag is untested |
| 37-PTY-4 | nit | docs | `daemon/src/nodes/pty.rs:397` | `discarded_targetward` comments omit the spy-console arm |
| 37-LOG-1 | minor | corr | `daemon/src/nodes/log.rs:542` | `dropped_bytes` over-counts on a partial write |
| 37-LOG-2 | minor | rel | `daemon/src/nodes/log.rs:693` | Rotation recovery silently degrades to a clobbering rename on scan failure |
| 37-LOG-3 | minor | test | `daemon/src/nodes/log.rs:482` | The `overflow = "fault"` queue-overflow arm has no test |
| 37-CODEC-1 | minor | rel | `daemon/src/nodes/exec.rs:360` | Mux-edge surgery during crash backoff rewrites Faulted to Active |
| 37-CODEC-2 | minor | dev | `daemon/src/nodes/exec.rs:59` | Exec attribute table silently ignores unknown keys |
| 37-CODEC-3 | minor | test | `daemon/src/nodes/codec.rs:809` | The mux-refusal targetward charge has no regression guard |
| 37-CODEC-4 | nit | rel | `daemon/src/nodes/exec.rs:727` | `open("")` on the reserved empty identity is recorded as an unconfigured channel named `""` |
| 37-CODEC-5 | nit | docs | `daemon/src/nodes/codec.rs:25` | codec.rs comments say the stolen held lock is re-acquired "FIFO" |
| 37-LEG-2 | minor | dev | `daemon/src/nodes/leg.rs:686` | A listen-role bind failure is terminal — the one environmental fault that never heals |
| 37-LEG-3 | minor | test | `daemon/src/nodes/leg.rs:1032` | §7.4's second-connection refusal has no end-to-end test |
| 37-LEG-4 | minor | test | `daemon/src/nodes/leg.rs:723` | The overall handshake deadline is untested |
| 37-LEG-5 | nit | simp | `daemon/src/nodes/leg.rs:398` | Infallible channel-stat lookups silently mint orphan default stats |
| 37-DATA-1 | minor | corr | `daemon/src/runtime.rs:1179` | `attach_edge` conflates a full `EdgeInbox` with a closed one; pipelined surgery can overflow it |
| 37-DATA-2 | nit | docs | `daemon/src/boundary.rs:258` | `TaskSet::is_empty` doc states the inverse of the method |
| 37-LOCK-2 | minor | dev | `daemon/src/daemon.rs:2076` | `lease_ms` is the one timer input with no checked maximum |
| 37-LOCK-3 | minor | dev | `core/src/lock.rs:451` | A `send --steal` theft record vanishes from state when the transient origin unregisters |
| 37-CFG-1 | minor | dev | `docs/35-design-claude-fable-v14.md:213` | Design §11 still lists "resolver-input well-formedness" among load's checks (**justify**) |
| 37-CFG-2 | nit | test | `core/src/config.rs:2582` | The §3 length bound has no config-level test through `GraphConfig::validate` |
| 37-RES-2 | minor | corr | `core/src/resolver.rs:714` | `find_usb_by_id` omits the dev-node existence check every other arm has |
| 37-RES-3 | minor | corr | `core/src/resolver.rs:319` | Path and raw input accept directories, capturing e.g. `raw:/dev` |
| 37-RES-4 | minor | corr | `core/src/resolver.rs:296` | `..` components in raw/path input escape `dev_root` |
| 37-RES-5 | minor | corr | `core/src/resolver.rs:782` | A sysfs serial containing `:` mints an identity the resolver's own parser rejects |
| 37-RES-6 | minor | test | `core/src/resolver.rs:199` | Bare-serial input has zero test coverage tree-wide |
| 37-RES-7 | nit | docs | `docs/implementation-notes.md:1514` | Notes say `enumerate_ports` unions three sources; the code unions four |
| 37-CTRL-3 | minor | docs | `daemon/src/tap.rs:575` | `replay_truncated` never reaches the client; comments claim it is reported |
| 37-LIFE-1 | minor | dev | `daemon/src/daemon.rs:835` | `remove-node --cascade` discards the lock-release and purged-bytes facts `disconnect` reports |
| 37-LIFE-2 | minor | test | `daemon/src/daemon.rs:2203` | No test pins that `teardown` persists an empty graph or that `remove-node` is snapshotted |
| 37-LIFE-3 | nit | docs | `itest/tests/p8_external_codec.rs:130` | Comment spells the RV-6-retired `<socket>.state.toml` form |
| 37-WEBS-1 | minor | corr | `web/src/wsclient.rs:22` | The bearer token rides argv, world-readable via /proc |
| 37-WEBS-2 | minor | rel | `web/src/main.rs:63` | A ported `--host` value can never match: silent all-requests 403 |
| 37-WEBS-3 | minor | rel | `web/src/server.rs:639` | Daemon-down at upgrade: 101 then abrupt drop with no Close frame |
| 37-WEBS-4 | minor | test | `web/src/server.rs:225` | Promised WS message/frame caps are untested |
| 37-WEBS-5 | minor | docs | `web/src/server.rs:303` | Docs promise one 5 s pre-auth deadline; TLS handshake and head each get 5 s |
| 37-WEBS-6 | minor | docs | `docs/35-design-claude-fable-v14.md:523` | Design §17 still describes the superseded pre-auth "reserve" model |
| 37-WEBS-7 | nit | rel | `web/src/tls.rs:135` | A failed cert write during generation leaves a partial cert that blocks later starts |
| 37-WEBC-1 | minor | corr | `web/src/assets/app.js:932` | Clear confirmed during an in-flight selection deletes nothing and the record re-renders |
| 37-WEBC-2 | minor | corr | `web/src/assets/app.js:598` | `resumeTap` can race an in-flight `tap.open` and leak a daemon-side tap |
| 37-WEBC-3 | minor | corr | `web/src/assets/ansi.mjs:216` | `MAX_STRING` is defeated by ESC-dense input |
| 37-WEBC-4 | minor | dev | `web/src/assets/app.js:397` | The drop badge wears hub-lifetime feed loss as the tab's own |
| 37-WEBC-5 | minor | dev | `web/src/assets/app.js:694` | The ANSI unknown-sequence counter is computed and never surfaced |
| 37-WEBC-6 | minor | rel | `web/src/assets/app.js:473` | `selectConsole` restores from OPFS without ordering behind its own queued flush |
| 37-WEBC-7 | minor | test | `web/ui-tests/tests/console.spec.mjs:248` | The LOCKED-steal affordance has no positive-path browser test |
| 37-WEBC-8 | nit | docs | `web/src/assets/history.mjs:1` | History/OPFS/saver comments cite plan sections as design sections |
| 37-TOOL-2 | minor | rel | `ctl/src/main.rs:660` | `ctl subscribe` swallows an error ack and blocks forever |
| 37-TOOL-3 | minor | dev | `doctor/src/probes.rs:2038` | P5 certificate omits the parity mismatch and break reception §15.21 promises (**justify**) |
| 37-TOOL-4 | minor | rel | `sim/src/main.rs:2044` | `sim wire --stall` can wedge in an unbounded blocking `write_all` with no verdict |
| 37-TOOL-5 | minor | test | `expectations/linux.jq:52` | `linux.jq` asserts nothing about P3, not even presence |
| 37-TOOL-6 | nit | docs | `docs/36-implementation-plan-claude-fable-v14.md:47` | Plan §3 documents sim pty behaviors that were never built |
| 37-TEST-2 | minor | test | `itest/tests/meta_gates.rs:124` | Gate self-exclusion by bare file name exempts any future `meta_gates.rs` |
| 37-TEST-3 | minor | test | `docs/benchmarks/phase3.json:9` | The phase-3 idle-cost axis has no executable check; its producer was deleted unported |
| 37-TEST-4 | minor | rel | `itest/src/lib.rs:381` | `Rpc::stream` discards the ack unexamined; malformed notifications read as timeouts |
| 37-TEST-5 | minor | simp | `itest/tests/p8_tap.rs:68` | Ground-truth helpers hand-copied across 6–8 test files each |
| 37-TEST-6 | minor | test | `itest/tests/meta_gates.rs:612` | The fuzz-api gate credits a re-export whose name appears only in a comment |
| 37-DOC-4 | minor | docs | `examples/external-codec/README.md:23` | Cites a deleted validation script and a wrong `info` codec list |
| 37-DOC-5 | minor | docs | `docs/implementation-notes.md:2768` | Notes §2 crate table understates the shipped CLI, sim, and web surface |
| 37-DOC-6 | nit | docs | `docs/macos.md:378` | Roadmap still lists as future the CI lane and hands-on pass the page reports done |
| 37-SEAM-2 | minor | rel | `daemon/src/lib.rs:383` | `prepare_socket` deletes an existing non-socket file at the `--socket` path |
| 37-SEAM-3 | minor | rel | `daemon/src/lib.rs:378` | Two daemons racing the stale-socket dance can unlink each other's live socket |
| 37-PKG-3 | minor | docs | `packaging/README.md:29` | README udev step references a group the flow never creates and the unit never joins |
| 37-PKG-4 | minor | docs | `packaging/serial-nexus-daemon.example.toml:42` | Example config's log comment prescribes `ReadWritePaths`, contradicting `LogsDirectory` |
| 37-EXTC-1 | minor | test | `itest/tests/p5_exec_conformance.rs:63` | Fixture path passed to `sh -c` unquoted; the sibling quotes for this exact hazard |
| 37-EXTC-2 | nit | docs | `tests/ext-codec/passthrough.py:22` | `read_exact` docstring misdescribes truncated-read behavior |

### 4.1 Serial node and sys (37-SER)

**37-SER-1 (minor, correctness).** `send_break` and `pulse_dtr` (`daemon/src/nodes/serial.rs:987-1004`) have no interlock against a second in-flight line-holding verb on the same port: both verbs' restore guards gate only on the port generation, which both pass when no port transition occurred. A `send-break --ms 5000` overlapped by a `send-break --ms 500` from a second control connection (reachable — the one-waiting-verb rule is per-connection) has the shorter verb's guard clear the break while the longer verb still reports success for its full duration. *Fix:* make the line-holding verbs mutually exclusive per port (a signal-in-flight flag on the shared port state), refusing the second with a named error; add a two-connection overlap test.

**37-SER-2 (minor, correctness).** `Daemon::signal_ms` (`daemon/src/daemon.rs:1367`) reads `ms` via `Value::as_u64`, so a present-but-mistyped value in a raw RPC request (`"ms": "70000"`, `-5`, `1.5`) yields `None` and silently falls back to the verb default rather than returning `-32602`; the range check then runs against the substituted default. `bool_param` does the same for `pulse-dtr`'s `assert`, so `"assert": "false"` (string) drives the DTR pulse in the opposite direction from what the caller wrote. *Fix:* distinguish absent from mistyped — a present param that fails type conversion is `invalid_params` naming the field.

**37-SER-3 (nit, docs).** Three sites claim serial2 opens the serial fd blocking (`sys/src/lib.rs:186` `set_nonblocking` doc, `serial.rs` module doc, design §13 / notes §3.1), but the pinned serial2 0.2.37 opens with `O_NONBLOCK | O_NOCTTY` and never clears it. The daemon's re-set is harmlessly redundant; the prose misleads a reader auditing the reopen-window reasoning. *Fix:* correct the prose (design §13's clause via the amend-first rule).

### 4.2 PTY node (37-PTY)

**37-PTY-1 (minor, reliability).** The pty master is opened with `posix_openpt(O_RDWR|O_NOCTTY)` (`pty.rs:197`) and `FD_CLOEXEC` is never set, while exec-codec children spawn with no fd cleanup — so every codec child inherits an open copy of every console's master fd. A child then keeps the master/slave pair alive after `remove-node` or `load --replace` (the slave client never sees HUP, the pts index is not reclaimed), and holds a descriptor from which the console's targetward bytes can be read or into which bytes can be injected. *Fix:* set `FD_CLOEXEC` immediately after `posix_openpt` (small `serial_nexus_sys` helper).

**37-PTY-2 (minor, correctness).** `apply_perms` chmods the slave to the configured mode (0660 when a group is set) *before* chowning it to the configured group (`pty.rs:254` vs `:278`), and a fresh devpts node is group `tty` — so between chmod and chown (a window widened by NSS name resolution in between) any group-`tty` process can open the slave read-write, and an fd opened in the window survives the chown. Distinct from review-26 SEC-6 (symlink publication ordering — refuted and not re-filed): this is the device node's own mode/owner ordering. *Fix:* chown first, then chmod — never widen a mode until ownership matches the configuration.

**37-PTY-3 (minor, testing).** The §7.2 promise that a client clearing EXTPROC (rebuilding termios from scratch) triggers a re-assert is implemented (`pty.rs:1276`) but untested anywhere: no test or sim mode ever clears EXTPROC (the sim's raw setup preserves it), so deleting the branch would pass the suite, silently degrading observation to the poll backstop. *Fix:* a sim client mode applying a from-scratch termios, plus a test asserting a subsequent baud change surfaces promptly.

**37-PTY-4 (nit, docs).** `Losses::targetward`'s doc and the `state_extra` comment (`pty.rs:83-86`, `:396-397`) describe `discarded_targetward` only as the endpoint-went-away arm, omitting the attached read-only spy edge that dominates the counter in steady state (and which `docs/rpc/observation.md` documents). *Fix:* widen the two comments.

### 4.3 Log node and map node (37-LOG)

**37-LOG-1 (minor, correctness).** On a partial write failure (the ordinary disk-full shape: `write(2)` stores what fits, the retry gets ENOSPC), `writer_drain` counts the *entire* chunk into `dropped_bytes` under both overflow policies (`log.rs:558`, `:550`) even though a prefix is already durably in the file — so `file_len + dropped_bytes` exceeds the produced total, breaking the exactness identity the code claims and `p12_log_queue.rs` pins (the existing guards fail at offset 0 and never see partial progress). *Fix:* hand-roll the write loop and charge only the unwritten remainder — the review-26 PTY-3 remedy applied here.

**37-LOG-2 (minor, reliability).** `scan_rotation` (`log.rs:693`) swallows `read_dir` failure as `None`, indistinguishable from "no rotations yet", and the create-time comment claims an unreadable directory faults the open — but a mode-0300 directory fails the scan while `open_append` succeeds, so the node comes up Active with the counter reset; the next `rotate` renames the live file *onto* the newest existing rotation (`std::fs::rename` silently replaces), destroying a rotated log. *Fix:* distinguish scan failure from an empty scan; fault the node or refuse `rotate` until a rescan succeeds.

**37-LOG-3 (minor, testing).** The `overflow = "fault"` queue-overflow arm (`log.rs:481-489`: drop-and-count the arriving chunk, fault with reason "log queue overflow", keep consuming per the recorded §15.27 decline) has no test at any level — the reason string appears nowhere outside the source. *Fix:* a unit test planting a near-full Fault-policy queue and asserting all three behaviors.

### 4.4 Codec runtime, exec codec, codec API (37-CODEC)

**37-CODEC-1 (minor, reliability).** `ExecCodecNode::set_upstream_attached` (`exec.rs:356-369`) unconditionally rewrites node status to Active/Waiting on mux-edge surgery, overwriting the supervisor's `Faulted{child exited; restarting}` stamp while the crashed child sits in restart backoff (legal up to an hour; nothing corrects the overwrite until the respawn attempt). An operator sees a dead exec codec reported "active". *Fix:* consult a shared child-liveness flag in `set_upstream_attached` so surgery recomputes Faulted while a crashed child is between kill and respawn.

**37-CODEC-2 (minor, deviation — fix).** `ExecAttributes` (`exec.rs:59`) derives `Deserialize` without `deny_unknown_fields`, so a misspelled attribute (`restart_backoffms`, `enviroment`) loads clean and the default silently applies — the one place in a configuration where a typo changes behavior without being named, contradicting §11's review-hardened rule, which extends to codec tables. Every other table in the schema refuses unknown keys. *Fix:* add `deny_unknown_fields` and an unknown-key case to the existing structural test.

**37-CODEC-3 (minor, testing).** `channel_targetward`'s defensive branch for a mux refusing a piece — charge `total - off` to `discarded_targetward` and stop framing (`codec.rs:809-816`) — is unreachable with the reference codec and no stub codec returns `Err` from `mux`, so a regression breaking out of the loop without charging would ship unseen on exactly the skip-on-encode-error path AGENTS.md §4 forbids. *Fix:* a stub codec whose `mux` errs after N pieces; assert `accepted + discarded == total`.

**37-CODEC-4 (nit, reliability).** `route_event` special-cases the reserved empty identity only in the Data arm (`exec.rs:661`), so a child emitting `open("")` records the reserved identity in `unconfigured_channels` as `""` and fires the mis-spelled-channel WARN with an empty name; `close("")` is a silent no-op. Neither is pinned by a test. *Fix:* special-case the reserved identity in the Open/Close/Error arms; add a unit test.

**37-CODEC-5 (nit, docs).** `codec.rs:24-26` and `:817-820` describe post-steal re-acquisition as "FIFO", contradicting §15.23's held-priority reclaim, which is what the code does. *Fix:* reword both comments.

### 4.5 Leg node and wire (37-LEG)

**37-LEG-1 (major, correctness).** The receiving-side purge (review-26 LEG-3's mechanism) identifies outage-era chunks by comparing a per-chunk epoch snapshot taken **after** `rx.recv()` returns (`leg.rs:1262`) against the live disconnect epoch. When a peer's final data frames and its FIN are readable together — a peer that sends its commands and disconnects immediately — one poll of the supervise task enqueues the whole backlog, reads EOF, and synchronously bumps the epoch before the channel task can poll; every stale queued chunk then snapshots the *post*-bump epoch, both purge checks compare equal, and the permitless `Notify` pulse has already fired. Deterministic on the current-thread runtime, not a race: the dead connection's entire backlog is later delivered into the device when the local floor frees, credited to `accepted_targetward` with `purged_on_reconnect` = 0 — the §6 stale-command hazard verbatim. The existing guard pins only the peer-holds-until-parked ordering. *Fix:* attribute each queued chunk to the connection that delivered it (snapshot the epoch at *enqueue*, carrying it with the chunk), so a chunk from a dead connection is identifiable regardless of when the channel task dequeues it; regression test = the existing p6_outage receiving-side test with the hold removed.

**37-LEG-2 (minor, deviation — fix).** A listen-role bind failure (transient EADDRINUSE, EMFILE, a not-yet-up interface address) sets Faulted and returns the supervisor permanently (`leg.rs:686`) — the one environmental fault in the daemon that never heals, against §11's "faulted-and-wait … healing on their own" and §15.8's generalization; recovery is remove-and-re-add. *Fix:* wrap the bind in the supervisor's retry loop with backoff (re-running the stale-socket check each attempt), or amend the design declaring the listen bind terminal and why.

**37-LEG-3 (minor, testing).** §7.4's concurrent-second-connection refusal has no end-to-end test: `reject_extra_peers` is unit-tested only via an injected always-failing accept, never a real second peer against a live listener with the first session's data flow asserted to survive. *Fix:* a p6_binding sibling driving a second dial mid-session.

**37-LEG-4 (minor, testing).** The overall handshake deadline (§15.24: a trickling peer cannot wedge a listener) is unexercised: p6_hostility's four cases all complete or actively terminate their hello, the sim has no mute/trickle mode, and the timeout arm (`leg.rs:723`, hardcoded 5 s) never fires in the suite. *Fix:* a sim `wire --mute-ms`/`--trickle-hello` mode plus a hostility case asserting the fault and subsequent heal.

**37-LEG-5 (nit, simplification).** `stat_for` (`leg.rs:398`) and the lookup at `:462` use `unwrap_or_default()` on lookups that are infallible by construction; the fallback's only reachable effect, should the invariant break, is silently minting an orphan stat invisible in `state` — an accounting gap where a panic would be honest. *Fix:* `.expect("stats is keyed by self.channels")`.

### 4.6 Data-plane runtime (37-DATA)

**37-DATA-1 (minor, correctness; two findings merged).** `attach_edge` hands the consumer its hostward receiver with `inbox.try_send(hrx).is_ok()` (`runtime.rs:1179`), treating `Full` identically to `Closed`: the receiver is dropped and the validated edge is dead from birth under a misattributed diagnosis. The guarding comment argues fullness is impossible via §4 rule 2, but that rule bounds *configured* edges, not undrained receivers: `EDGE_INBOX_CAP` is 4 and `detach_edge_runtime` cannot retract a queued receiver, so five pipelined connects with four interleaved disconnects on one target endpoint — processed before the consumer's pump gets the thread, deterministically on the current-thread runtime — overflow the inbox and leave a configured edge hostward-dead. *Fix:* match on the error; on `Full` either refuse the connect with a named transient error or make stale receivers droppable via an epoch the pump checks; correct the comment either way.

**37-DATA-2 (nit, docs).** `TaskSet::is_empty`'s doc (`boundary.rs:255-257`) states the inverse of the return value. The sole production caller uses it correctly. *Fix:* invert the comment.

### 4.7 Write arbitration (37-LOCK)

**37-LOCK-1 (major, correctness).** A pty edge with `write_mode = "held"` — a legal, validator-accepted shape, and the exact graph `p4_held.rs` loads — never reclaims a freed lock: the pty's read gate is only `may_write` (`pty.rs:901-904`), and nothing ever drives `reacquire_held` for a pty origin (`runtime.rs`'s helper is called only from the codec, exec, and map targetward pumps), while held-priority in `EndpointLock::acquire` (`core/src/lock.rs:248-249`) denies every *other* origin's acquire on the free lock. After any sequence that frees the endpoint while the pty origin is not the holder — a completed `send --steal`, an `unlock`, a lease expiry — the endpoint sits free-but-untakeable: the console's bytes backpressure forever and every contender gets the LOCKED error, until a manual `lock`, another steal, or edge surgery. This breaks §6/§15.23's "reclaims the moment it frees", proven by test for the codec and map and unimplemented for the pty. *Fix:* drive reclaim for boundary-origin held edges too — e.g. the pty's drain path attempts `reacquire_held` when `may_write` is false and its mode is held, or the lock's release path notifies registered held origins through the same waker the pumps use; regression test = p4_held with a steal-release cycle.

**37-LOCK-2 (minor, deviation — fix).** `lease_ms` is parsed as a raw u64 (`daemon.rs:2076`) and fed to a sleep task with no checked maximum — the one daemon-side timer input outside the §15.34/§16.12 rule; its siblings refuse (`signal_ms`) or deliberately clamp with a stated rationale (`send`'s deadline). *Fix:* range-check at parse against `MAX_TIMER_MS` with the standard named-error sentence; document the ceiling.

**37-LOCK-3 (minor, deviation — fix).** `snapshot()` resolves a steal record's parties lazily and drops the whole record when either id no longer resolves (`lock.rs:451-456`); a `send --steal` unregisters its transient origin milliseconds after the steal, so the §6 promise that a steal "is recorded in state so the previous holder can see what happened" holds for `lock --steal` and silently fails for `send --steal`. *Fix:* store the label strings in the record at steal time; alternatively amend §6 to scope the promise.

### 4.8 Graph model and configuration (37-CFG)

**37-CFG-1 (minor, deviation — justify; recorded as implementation-notes §3.23).** Design §11 (line 213) still lists "resolver-input well-formedness" among load's structural checks, but `Daemon::load` deliberately performs none — `resolve_input`'s sole caller is `add-node` — per the settled §12 asymmetry notes §3.20(a) records (load-by-identity must never require the device or its syntax to be resolvable, or cold-start recovery breaks). The code is right; the design sentence is the stale side. *Disposition:* recorded in the notes; the design sentence should be qualified at the next design revision (annotate, never rewrite).

**37-CFG-2 (nit, testing).** The §3/§16.12 length bound (`MAX_NAME_LEN` → `NameTooLong`) is proven only against directly built `GraphModel` shapes; no test drives an oversize name or channel identity through `GraphConfig::validate`, unlike the whitespace sibling that has exactly that config-level proof. *Fix:* add the config-level counterpart beside `whitespace_only_channel_identity_is_rejected`.

### 4.9 Resolver (37-RES)

**37-RES-1 (major, correctness).** Bare-serial operator input — a first-class capture form per §12/§15.25 — reads only `/dev/serial/by-id`: `capture_from_serial` (`resolver.rs:371`) and the bare-token arm of `resolve_current_path` (`:478-488`) iterate `discover_adapters()` (`:495`), which returns empty when the by-id tree is absent, while §12's one-source rule requires both directions to read the `class/tty` listing with by-id as a fast path *over* it. In the exact environment §12 names (a container handed a bare device, an image without udev serial rules — the fixture the RES-2 unit test itself builds), `add-node` with a bare serial fails DEVICE_ABSENT while the adapter is present — a diagnostic pointing away from the cause, the RES-2 pattern in the arm RES-2 missed. Additionally `discover_adapters` returns unsorted `read_dir` order and `capture_from_serial` takes the first serial-field match with no ambiguity refusal, so two adapters sharing a serial string capture nondeterministically. No test exercises bare-serial input in either direction (see 37-RES-6). *Fix:* route bare-serial capture and resolution through the same device listing `find_usb`/`enumerate_ports` now use, with the ambiguity guard counting over devices; sort or refuse on multiple matches.

**37-RES-2 (minor, correctness).** `find_usb_by_id` (`resolver.rs:697-718`) returns a `/dev` path with no dev-node existence check, unlike every sibling source — so the by-id arm can answer with an absent device the primary listing already excluded (a stale by-id tree in a static-/dev container, or the devtmpfs-before-sysfs removal window), making by-id an alternative source rather than the fast path §12 requires. *Fix:* one `exists()` call matching the siblings' presence predicate; unit test with a by-id link whose target node is gone.

**37-RES-3 (minor, correctness).** `capture_from_path` (`resolver.rs:319`) gates presence on `exists()` alone, true for directories, so `add-node` with device `/dev` or `/dev/serial` succeeds and persists `raw:/dev` bound to a directory — the outcome the function's own all-slash guard comment says §11 rejects up front; the node then faults EISDIR with the nonsense identity persisted by dump. The raw and bare-path resolution arms share the hole. *Fix:* reject directories (`Malformed: "path is a directory, not a device node"`) in all four arms.

**37-RES-4 (minor, correctness).** `..` components in raw/path input escape `dev_root`: the shared `rooted` join does only leading-slash trimming, and no resolver caller rejects `Component::ParentDir`, so under a non-`/` dev root a device string like `raw:/../x` resolves, is minted as an identity, and binds a path outside the root — contradicting the module's documented containment (and the first-class test-seam status plan §3 gives `--dev-root`). *Fix:* reject `..` before joining, in every literal arm.

**37-RES-5 (minor, correctness).** `sysfs_lookup` formats `usb:{vid}:{pid}:{serial}:{iface}` with no reserved-character check (`resolver.rs:782`), so an adapter whose serial contains `:` (MAC-style serials exist) mints an identity `resolve_usb_identity` rejects as Malformed — the dumped configuration cannot be re-added by identity form. *Fix:* treat a serial containing `:` as unusable for the usb form and degrade to by-path (the CP-6 pattern), or amend §12 with an escaping rule first.

**37-RES-6 (minor, testing).** The bare-serial input form has zero test coverage in the whole tree — not the successful capture, not the NotPresent refusal, not duplicate-serial degradation — which is how 37-RES-1 shipped through the RES-2 remediation. *Fix:* unit tests for all three plus the no-by-id fixture (fails today), and a p12_resolver_identity extension.

**37-RES-7 (nit, docs).** `docs/implementation-notes.md:1514` still says `enumerate_ports` unions "Three passive sources"; since RES-2 the code unions four, the fourth being exactly the source that makes `ports` work without udev. *Fix:* update the sentence.

### 4.10 Control plane and taps (37-CTRL)

**37-CTRL-1 (major, deviation — fix).** `TapHub::detach_all` (`tap.rs:608`) delivers the terminal `TapMsg::Closed` via `let _ = try_send(...)` on the connection's bounded 128-slot tap channel and discards `Full`; `taps.clear()` then drops the senders, so the loss is permanent, uncounted, and undetectable from the channel. Reachable from `teardown`, `load --replace`, and `remove-node`; a full queue is the ordinary steady state for a slow consumer on a firehose endpoint. Design §10 promises "the tap does not silently die" with no best-effort qualifier, while the code comment and `docs/rpc/observation.md` declare delivery best-effort — a divergence with no recorded amendment. When the drop occurs, passive clients sit on a live connection receiving nothing: `ctl tap` re-enters the hang shape CTL-1 was fixed for, and the console pane freezes without re-anchor. The only guard tests a non-full queue. *Fix:* make the terminal event undroppable — a per-tap terminal slot (or pending-closed list on the connection) drained alongside the data queue — or amend §10 to state and bound best-effort terminal delivery with a recovery story; add the full-queue regression either way.

**37-CTRL-2 (major, reliability).** `serve_connection` awaits `write_all` inside each select-arm body (`control.rs:469` is the volume carrier), so while a write to a client that stopped reading is blocked, that connection's parked in-flight verb is never polled — and its deadline and dequeue guard live inside the unpolled future, so §15.20's cancel-safety enumeration cannot fire. If the parked verb's origin is the FIFO head, release never grants and acquire denies every other on-demand origin while free-but-queued: one non-reading client stalls every other connection's arbitration on that endpoint indefinitely, with the lock reported free and only `--steal` as an escape. Reachable through the shipped web console (a frozen tab backpressures the bridge's bounded funnel into exactly this stall). p12_control_streams guards the inverse direction only. *Fix:* decouple the waiter's deadline from the connection's write path — e.g. run the parked verb's future on the two-lane dispatch rather than inside the select body, or bound the notification writes with a shed-and-count policy so the select loop keeps polling; regression test = a stalled-reader connection whose parked `send` still expires at its deadline.

**37-CTRL-3 (minor, docs).** `Registered::replay_truncated` — created to name the short-ring-vs-short-channel ambiguity — never reaches a client: it surfaces only as a `tracing::debug!` line (`tap.rs:570-582`) and is absent from the `tap.open` result and `docs/rpc/observation.md`, while the comments claim it "is reported … rather than left for a client to infer". The TAP-1 ledger entry recorded the residual; the comments overstate what shipped. *Fix:* put the field on the wire (one json field plus a doc row, extending the oversize-ring guard), or align the comments with the deliberate residual.

### 4.11 Daemon lifecycle (37-LIFE)

**37-LIFE-1 (minor, deviation — fix).** `remove_node`'s cascade loop (`daemon.rs:835`) drops `detach_edge_runtime`'s `DetachedEdge` return — whose own doc says these facts are "reported rather than done silently" — so the purged byte count and lock-release fact land in no response field, counter, or log, while `disconnect` of the identical edge reports both. *Fix:* aggregate the results into the `remove-node` response (`released_locks`, `purged_bytes`) and document; or record the asymmetry deliberately.

**37-LIFE-2 (minor, testing).** No test pins the persisted snapshot after `teardown` (empty graph written) or after `remove-node` (node absent from the state file); a regression dropping either verb from `is_config_mutation` (`daemon.rs:2196`) — the exact defect class the rename track's p13 guards fail-first-proved for adoption — would pass the suite and resurrect removed nodes on restart. *Fix:* three cases in the p7/p10 family (remove-node + restart; teardown + restart; clean-shutdown preservation — see also 37-SEAM-1).

**37-LIFE-3 (nit, docs).** `p8_external_codec.rs:130`'s comment spells the state-file derivation as `<socket>.state.toml` — the exact wrong form review 32 RV-6 retired; the shared helper derives from the socket's stem. *Fix:* one-word comment fix.

### 4.12 Web console server (37-WEBS)

**37-WEBS-1 (minor, correctness).** The sanctioned headless client (`serial-nexus-web wsclient`) requires the shell-equivalent bearer token as `--token` on argv (`wsclient.rs:21-22`) with no environment or file alternative — world-readable via `/proc/<pid>/cmdline` (default Linux, no hidepid) for the process's lifetime, to exactly the local-user adversary class the token exists to gate (§17: a loopback port is reachable by every local user). The server's own default flow never exposes the token on argv; the wsclient always does. *Fix:* accept the token via environment variable and/or `--token-file` (0600), prefer those in docs, and note the argv exposure in `--token`'s help.

**37-WEBS-2 (minor, reliability).** Operator-supplied `--host` values are compared verbatim against the port-stripped request Host, so a value containing a port (`example.com:8443` — a spelling the help text invites and browsers' Host headers carry) can never match, and every request 403s "unrecognized Host" with no startup diagnostic. *Fix:* normalize `--host` through the existing `split_authority` at startup, or refuse ported values naming the flag.

**37-WEBS-3 (minor, reliability).** `upgrade_ws` writes the 101 before `bridge()` makes first daemon contact (`server.rs:639-640`), so a browser connecting while the daemon is down completes the handshake, briefly renders "connected", and is dropped with no Close frame — the exact ambiguity WEB-4 removed for mid-session death, absent at session birth. *Fix:* connect to the daemon before the 101 (answer 503 naming the daemon), or send `Close(1001)` through the fresh socket on connect failure.

**37-WEBS-4 (minor, testing).** The promised WS bounds (messages 1 MiB, frames 256 KiB — design §17, `docs/security.md`) are wired but untested; deleting the two config calls reverts to the 64 MiB default with the suite green. *Fix:* an itest sending an over-cap frame post-auth (closed, nothing reaches the daemon) plus an under-cap control.

**37-WEBS-5 (minor, docs).** `docs/security.md` and the `HEAD_TIMEOUT` comment promise one 5 s pre-auth deadline covering TLS handshake plus head; the code grants `HEAD_TIMEOUT` twice in sequence (`server.rs:303`, `:422`), so a TLS-tier pre-auth peer holds a slot up to ~10 s. *Fix:* one shared `timeout_at` deadline across both phases, or amend both doc sites.

**37-WEBS-6 (minor, docs).** Design §17 (line 523) still describes the pre-auth bound as a "reserve" of the connection cap — the first WEB-5 shape the audit demonstrated to be an operator lockout and reworked. The shipped bound is a disjoint 32-slot pool bounded by evicting its oldest member, as security.md and the notes correctly describe. *Fix:* amend the design parenthetical (amend-first rule) to the eviction-bounded disjoint pool.

**37-WEBS-7 (nit, reliability).** In `generate_self_signed`, a cert write failing after `create_new` succeeded leaves the partial cert on disk, and every later start takes the half-present-pair refusal whose "supply the missing file" remedy misleads for a garbage file the tool itself created; the sibling key-failure path removes the just-created cert with a comment stating exactly this rationale. *Fix:* symmetric cleanup — remove any file this call created when its write fails.

### 4.13 Web console client (37-WEBC)

**37-WEBC-1 (minor, correctness).** `clearBtn.onclick` guards the OPFS delete with `historyKey` (`app.js:932`), but `selectConsole` nulls it synchronously and re-adopts only after awaiting `tap.close` and the OPFS load — and `confirm()`'s modal blocking parks the selection continuation, so the window cannot close itself. A clear confirmed in that gap deletes nothing, and the resumed selection re-renders the record the operator just confirmed destroying — the §15.32 clear-vs-snapshot race's sibling, arriving through selection rather than the debounce. *Fix:* compute the key from `selected` in the clear handler (or disable clear until `historyKey` is adopted) and make the resumed selection observe the clear.

**37-WEBC-2 (minor, correctness).** While `selectConsole`'s `openTapAndAnchor` awaits its `tap.open` reply, a watch-state flip (view switch, tab visibility) runs `resumeTap`, whose guards all pass, starting a second concurrent open in the same selection generation; both succeed, `currentTap` keeps the later id, and the earlier tap leaks daemon-side for the page's life (base64 relay + queue per §17's cost model, invisible to the grace-interval release). *Fix:* an `opening` flag set synchronously before any open, or have `openTapAndAnchor` close its own tap when it finds `currentTap` already set on completion.

**37-WEBC-3 (minor, correctness).** In the ANSI parser, STRING→STRING_ESC→STRING transitions never increment `strLen` (`ansi.mjs:202-216`), so after an unterminated OSC/DCS, ESC-dense input (a stuck line spewing 0x1B) freezes the `MAX_STRING` give-up forever: the console renders nothing and the honesty tally does not advance; once ordinary bytes resume, up to 4096 further real bytes are still swallowed. *Fix:* count in both states; test with `ESC ]` + an ESC run.

**37-WEBC-4 (minor, deviation — fix).** `updateHead` (`app.js:397`) sums the tap's own `dropped` with `state.taps[].feed_dropped` — the endpoint hub's *lifetime* producer→hub loss — while discarding the `feed_dropped` baseline `tap.open` returns, whose recorded contract exists precisely so a client can separate pre-open loss from its own. Once the endpoint has accrued any feed loss ever, every later tab wears it as "the tab's own drops". *Fix:* store the baseline at open; show `dropped + max(0, feed_dropped − baseline)`.

**37-WEBC-5 (minor, deviation — fix).** The ANSI unknown-sequence counter — §17 frames it as §5's honesty on the one surface where the operator cannot otherwise see what was thrown away — is computed at 18 sites and never read: no UI element, export, or log surfaces it. The HIST-1 computed-but-never-shown shape recurring. *Fix:* surface the tally beside the drop badge when nonzero, or amend §17 to call the counter diagnostic-only (the current sentence's purpose clause argues for surfacing).

**37-WEBC-6 (minor, reliability).** `selectConsole` enqueues the outgoing console's snapshot through the saver, then `await load(key)` reads OPFS directly, bypassing the saver's per-key queue — so when outgoing and incoming keys coincide (re-clicking the selected console is unguarded), load returns the previous committed record while the fresh snapshot is mid-flight, and the restored frontier can lag enough for a talkative console to read as a false ring-gap. *Fix:* give the saver a per-key settled() the restore awaits, or read through pending snapshots.

**37-WEBC-7 (minor, testing).** §17's central send contract — LOCKED shows the holder by name with an explicit steal affordance, never automatic — has only a negative-path browser test; no automated test at any layer drives the −32003 branch (holder-naming dialog, decline-restores-line, accept-retries-with-steal). The one positive-path validation on record is a manual checklist session. *Fix:* a console.spec case locking the echo endpoint over the control socket and driving both dialog outcomes.

**37-WEBC-8 (nit, docs).** `history.mjs`, `saver.mjs`, `opfs.mjs` cite "design §11.9" (and siblings cite bare §11.8/§11.9) — plan-§11 track items, not design sections; the design's §11 is configuration lifecycle. Under AGENTS.md §1 these refs are dangling. *Fix:* cite the design homes (§10, §15.32, §17) or label the refs "plan §11.8/§11.9".

### 4.14 CLI, sim, doctor (37-TOOL)

**37-TOOL-1 (major, reliability).** `ctl tap <ep> --bytes N` reaching clean connection EOF short of its budget — daemon crash or abrupt close — breaks the read loop and falls through to `Ok(())` (`ctl/src/main.rs:838-879`): exit 0 over a silently truncated capture. The identical stream-ended-short condition arriving as `tap.closed` exits 1, per the documented rule ("an outstanding `--bytes` budget at stream end is a short read and errors"); the EOF arm has no test — both existing guards exit through `tap.closed`. *Fix:* treat EOF with outstanding budget exactly as `tap_closed` does; add the EOF-path test.

**37-TOOL-2 (minor, reliability).** `subscribe_stream` discards every Response frame (`ctl/src/main.rs:660`) including an error reply to the subscribe request itself, then blocks in `read_line` with no timeout on a connection the daemon deliberately keeps open — so against a daemon without `subscribe` (version skew, the §15.16 graceful-degradation path; any out-of-tree daemon), `ctl subscribe` hangs forever with no output. *Fix:* mirror `tap_stream`'s ack handling — bail on an error ack with the code named, exit 1.

**37-TOOL-3 (minor, deviation — justify; recorded as implementation-notes §3.24).** P5's rig certificate performs only a baud mismatch and local break-ioctl acceptance (`probes.rs:2038`, `:1991`), while design §15.21/§13 and plan §3 still promise "deliberate baud *and parity* mismatches … break reception" as certificate contents. The narrowing is real, acknowledged in `docs/serial-nexus-doctor.md`, and the missing pieces are carried by the Tier-3 checklist suite — but the design text was never annotated. *Disposition:* justified narrowing recorded in the notes; annotate §15.21 (and the §13/plan §3 echoes) at the next design revision.

**37-TOOL-4 (minor, reliability).** `sim wire --stall` issues blocking `write_all` with no write timeout, checking its hold deadline only between rounds (`sim/src/main.rs:2036-2052`) — so a peer that stops reading (exactly the head-of-line shape this mode exists to expose) parks the sim in `write_all` indefinitely: no verdict line, no exit, breaking the plan's verdict-on-exit contract in the §9 conformance driver. SIM-1's shape recurring. *Fix:* bounded writes (non-blocking + `wait_writable` with the remaining hold budget), verdict on expiry.

**37-TOOL-5 (minor, testing).** `expectations/linux.jq` asserts nothing about P3 — not even presence — despite its header claiming completeness against the 7.0 baseline; a report that lost the P3 block entirely still passes the Linux gate (macos.jq does assert presence). *Fix:* add a P3 presence clause beside P4's.

**37-TOOL-6 (nit, docs).** Plan §3's pty sim-mode list promises `--script`, `--stall-read-after`, and `--hup-after`/`--reopen-after`, none of which exist (`--stall` covers one intent under another name), and plan phase 6 item 5 names `--stall-read-after` where the shipped test uses `wire --stall`. *Fix:* annotate the plan to the shipped flag set.

### 4.15 Harness quality (37-TEST)

**37-TEST-1 (major, testing).** The three tree-scanning meta-gates — unsafe confinement, the RefCell ban, the AsyncFd ban — prove their matchers against planted strings but never their walkers: no planted file is driven through `walk_rs`/`sources_under`/`crate_dirs`, no visited-count floor is asserted, and each non-vacuity check reads its anchor file directly rather than through the walker (the RefCell gate's `assert!(dir.is_dir())` is the file-existence shape AGENTS.md §3 bans). Both walkers swallow `read_dir` failure silently, so partial walker degradation leaves all three gates green over a shrunken file set — for the gates enforcing invariants 1 and 5. `meta_names.rs` already implements the correct shape (scratch-tree planted-file walk plus a visited floor). *Fix:* give each scanning gate the meta_names treatment.

**37-TEST-2 (minor, testing).** Gate self-exclusion matches `file_name() == "meta_gates.rs"` (`meta_gates.rs:124-127`, `:268`), so any future file of that name anywhere in the workspace is silently exempt from the unsafe, AsyncFd, and CriticalCell scans — and for AsyncFd this gate is the sole automated enforcement of invariant 1. The TESTR-7 class (path-vs-name), one instance over. *Fix:* match by repo-relative path (as meta_names.rs already does) plus an impostor-path self-proof.

**37-TEST-3 (minor, testing).** Phase 3's idle-cost exit criterion (32 idle tty fds under a stated CPU budget) lost its producer in the §16.11 migration — the benchmark script was deleted and, unlike its two siblings, never ported — so a regression in the adaptive backoff (§15.19's recorded ~0.06 %/idle-fd) passes silently, and `docs/benchmarks/phase3.json` has no executable check behind its idle axis. *Fix:* an itest guard sampling daemon CPU over 32 idle ptys with the existing tick sampler, self-skipping off Linux.

**37-TEST-4 (minor, reliability).** `Rpc::stream` consumes the subscribe/tap.open ack with `let _ =` (`itest/src/lib.rs:381`) — an RPC *error* ack is silently discarded and a late ack can be yielded as a notification — and `Subscription::next` maps malformed JSON to `None`, indistinguishable from a timeout, which `wait_for` treats as terminal. A daemon answering errors or emitting garbage reads as "timed out", pointing diagnosis away from the cause. *Fix:* parse the ack (panic on error/timeout with the method named); panic on non-JSON notification lines.

**37-TEST-5 (minor, simplification).** Ground-truth helpers are hand-copied across test files: the sim-matching `seeded_bytes` generator in 8 files, `base64_decode` in 6 (while `serial_nexus_rpc` ships a tested one), `file_len` in 8, the `/proc` tick sampler in 2 — the §16.5 shared-helper consolidation, unapplied to the post-migration harness. *Fix:* move them into `serial_nexus_itest` with unit tests, including a committed-vector test pinning `seeded_bytes` to the sim's output.

**37-TEST-6 (minor, testing).** `every_unstable_fuzz_api_export_has_a_fuzz_target` matches re-export names against fuzz sources without skipping comments (unlike the adjacent helper), so a re-export mentioned only in a comment would pass the one rule bounding the §15.26 exception; secondarily the module-body slice stops at the first line-start `}`. Both latent today. *Fix:* strip comments before matching; delimit by brace counting; add the negative self-proof.

### 4.16 Documents (37-DOC)

**37-DOC-1 (major, docs).** `docs/serial-nexus-doctor.md` still asserts in present tense that the dev box has no adapter and the in-tree 7.0 baseline is three passive runs (13 supported / 6 skipped) — but commit `d4743f9` committed a 7.0 **Tier-3** artifact (21 · 0 · 0 · 1) and `docs/doctor/README.md` records the pair moved back and the asymmetry closed. The notes were updated in that session; this page was not — an AGENTS.md §2 same-commit-discipline breach. *Fix:* update the four stale sites to cite the Tier-3 artifact.

**37-DOC-2 (major, docs).** The notes §1 kernel-matrix paragraph (`implementation-notes.md:2737-2740`) still says the latest 7.0 evidence is "13 · 0 · 0 · 6 passive … the box having no adapter", contradicted by the same file's newest session entry (21 · 0 · 0 · 1 Tier-3, the committed artifact). Per AGENTS.md §2 a stale claim in this file is a defect. *Fix:* update the paragraph to cite the Tier-3 run.

**37-DOC-3 (major, docs).** Three sites claim `crossover_ports()` auto-detects a Linux rig via two by-id entries — `docs/macos.md:87-88`, the function's own doc comment, and an older notes §3 entry — but the function has only the env-var arm plus a macOS-only `cu.*` scan; the notes' newest session states the correct behavior. A Linux operator who wires a pair without exporting the env vars gets a silent self-skip of every rig-gated test and can read a green run as hardware coverage that never executed — on the kernel of record. *Fix:* fix all three sites, or add the Linux by-id arm and keep the docs.

**37-DOC-4 (minor, docs).** `examples/external-codec/README.md` cites the deleted validation-script path (the gate is now `itest/tests/p8_external_codec.rs`) and its expected `info` output omits "exec", which the registry always includes. *Fix:* both corrections.

**37-DOC-5 (minor, docs).** The notes §2 crate table understates the shipped surface: the ctl row omits `connect`/`disconnect`/`info`/`ports`/`tap`, the sim row omits the exec-conformance mode, and there is no web-console row at all — in the file AGENTS.md §2 designates as what reviews check claims against. *Fix:* bring the table current or date-stamp it as a phase-era record pointing at AGENTS.md §1.

**37-DOC-6 (nit, docs).** `docs/macos.md`'s closing roadmap still lists as future the macOS CI lane and the hands-on hardware pass that the page's own update blocks report done; only the IOKit resolver remains genuinely future. *Fix:* rewrite the roadmap paragraph.

### 4.17 Daemon process seam (37-SEAM, from the gap pass)

**37-SEAM-1 (major, testing).** No automated test exercises the daemon's clean-exit path: nothing sends SIGTERM/SIGINT or asserts that `shutdown` terminates the process, removes the control socket, unlinks PTY symlinks, or preserves the state file — the post-loop teardown in `serve` is never observed, because the harness's `Daemon::drop` issues `shutdown` and immediately SIGKILLs. Deleting the signal-handling select arms would pass the entire suite. *Fix:* a test that SIGTERMs a live daemon and asserts exit, socket removal, symlink cleanup, and graph preservation (pairs with 37-LIFE-2's clean-shutdown case).

**37-SEAM-2 (minor, reliability).** `prepare_socket` (`daemon/src/lib.rs:378-389`) probes an existing path with `connect` and unlinks on *any* error with no is-it-a-socket check — a daemon started with `--socket` naming an existing regular file silently deletes it. The daemon's own unix leg was hardened against exactly this shape (review 26 SEC-8: symlink_metadata check plus ECONNREFUSED-only unlink); the control socket kept the naive dance. *Fix:* mirror the leg's `clear_stale_socket` discipline.

**37-SEAM-3 (minor, reliability).** Two daemons racing the stale-socket dance from a common stale file can both pass the probe before either unlinks; the loser's `remove_file` lands after the winner's bind, unlinking the winner's live socket — both then load the same state file and contend for the devices under TIOCEXCL, with the unreachable zombie typically winning the opens. *Fix:* an flock on a sibling lock file held across probe-unlink-bind and for the daemon's lifetime; gate the shutdown-time unlink on still owning it.

### 4.18 Packaging (37-PKG, from the gap pass)

**37-PKG-1 (major, correctness).** The packaged unit never benefits from the rename track's legacy state-snapshot adoption, for two independent reasons: adoption is gated on `options.state_file.is_none()` (`daemon/src/lib.rs:229-231`) and the unit passes an explicit `--state-file`; and `legacy_state_file` looks only beside the control socket, while the pre-rename unit persisted its state under its old `/var/lib` directory (named in `packaging/README.md`'s upgrade note). A systemd deployment upgraded per the README therefore starts from the first-boot `--config` seed and silently discards every node added by incremental surgery since the last load — the exact defect the p13 adoption work exists to prevent, in the deployment shape the packaging ships. The README says the old directory is not read yet gives no copy step. *Fix:* either extend adoption to cover an explicit `--state-file` whose file is absent while the pre-rename path exists, or add the explicit copy step to the upgrade instructions (and state the limitation in the unit).

**37-PKG-2 (major, correctness).** The unit's group-widening recipe (`packaging/serial-nexus-daemon.service:42`, `:56-60`; `packaging/README.md:79-84`) fails under `DynamicUser=yes`: systemd chowns the runtime directory to the *transient* user and primary group — `SupplementaryGroups=` never affects `*Directory=` ownership — so the recommended 0750 grants traversal to the transient group, and console-operators members classify as "other", getting EACCES at `connect(2)` before the (correctly chgrp'd) socket is ever reached. No test can catch this (the permission guard runs outside systemd). *Fix:* a working unit-level recipe — e.g. a named static `Group=` (which `DynamicUser` chowns directories to), or an `ExecStartPost` chgrp of the runtime directory — in both the unit comment and the README.

**37-PKG-3 (minor, docs).** README install step 4 presents the udev rules as a self-contained optional step, but the rules assign a group the README flow never creates and the unit's `SupplementaryGroups=dialout` does not include; the rules file's own comments carry the full recipe. Either half-followed variant fails (faulted serial nodes, or a rule that silently does nothing). *Fix:* put both prerequisites in the numbered step.

**37-PKG-4 (minor, docs).** The example config's log-node comment says the directory "must be in the unit's `ReadWritePaths`", but the configured default is provisioned by `LogsDirectory=` (no entry needed), and for a directory outside it the sentence is insufficient under `DynamicUser` — the exact packaged-EACCES shape a prior audit fixed. The unit and README already carry the correct guidance. *Fix:* align the comment.

### 4.19 External-codec fixtures (37-EXTC, from the gap pass)

**37-EXTC-1 (minor, testing).** `run_conformance` builds its exec string unquoted (`p5_exec_conformance.rs:63`) and the sim passes it to `sh -c` verbatim, so a workspace path with a space fails all three exec-conformance tests — while the sibling `p5_envelope.rs` single-quotes the same path with a comment naming this exact hazard. *Fix:* quote identically, or share one quoted-exec helper.

**37-EXTC-2 (nit, docs).** `passthrough.py`'s `read_exact` docstring says it returns None "at a clean EOF on a frame boundary", but it returns None for any short read, silently discarding a partial frame — behavior that is (correctly) teardown-tolerant, in the file codec authors are told to model. *Fix:* fix the docstring, not the behavior.

## 5. Deviations: fix versus justify

Eleven findings carry the deviation category. Nine are **fix** (the code or the named document should change): 37-CTRL-1 (tap.closed delivery vs §10 — or an explicit design amendment), 37-CODEC-2 (exec unknown keys vs §11), 37-LEG-2 (terminal bind fault vs §11/§15.8), 37-LOCK-2 (`lease_ms` vs §15.34/§16.12), 37-LOCK-3 (steal record vs §6), 37-LIFE-1 (cascade reporting vs §5/§6's reported-not-silent), 37-WEBC-4 (drop badge vs the tap accounting contract), 37-WEBC-5 (unknown-sequence counter vs §17's stated purpose), 37-WEBS-6 (design §17's stale pre-auth wording — a design-text fix).

Two are **justify** — the code is right and the record needed updating, done in this review's companion commit as implementation-notes **§3.23** (37-TOOL-3: the P5 certificate's recorded narrowing — parity mismatch and break reception ride the Tier-3 checklist suite, not the probe) and **§3.24** (37-CFG-1: design §11's "resolver-input well-formedness" clause describes `add-node`'s capture rule, not `load`, per the settled §3.20(a) asymmetry). Both entries name the design sentences to annotate at the next design revision; per §5's amend-first doctrine those annotations belong to the session that next touches the design.

## 6. Checked and found sound (for the next review's cleared-candidate table)

Beyond the standing dispositions (none of which this review re-files), the area passes explicitly examined and cleared, with the finder's negative results recorded so they are not re-derived:

- **Serial/sys:** the ordered at-most-once release of tty assertions (break before TIOCEXCL) is structurally owned by the port object and reached on every exit path constructed — mid-open failure, set-waiting, fault, the reopen await window (publish-before-purge with a generation re-check), teardown, and the Drop backstop — each with a fail-first guard; signal verbs are port-scoped by generation with the 60 s cap checked before resolution; the sys wrappers are sound on fd ownership and error paths.
- **PTY:** all six review-hardened mechanisms verified as shipped — the §15.39 session latch and both self-posted-edge forge sites, the read-the-slave-dry last-close accounting (exact, with the writer's mid-flight chunk being the §3-dispositioned residual), EXTPROC re-assert termination, both §15.30 platform arms, and the writer-thread lifecycle (stop observed inside the blocked write, join before fd drop).
- **Log/map:** the map is clean against the transcribed picocom oracle (including `spchex`/`nrmhex`/`8bithex` edge semantics), first-match-wins compiled into a 256-entry table, expansion bound property-tested, loss counted on both directions; the log node's queue/writer shape, both policies, rotation ordering, and two-phase bounded flush are as designed.
- **Codec/exec:** every targetward framer fragments through the one shared helper with counted residuals; the child pipes are genuinely concurrently pumped; teardown-vs-crash is discriminated; unconfigured-channel bytes are counted; golden vectors frozen; resync counts exactly one framing error per skip, pinned end-to-end.
- **Leg:** the review-32 hardenings (accounting, bounded remote-fed state, custody-slot loss attribution, unix-socket discipline) hold as shipped with fail-first guards.
- **Data plane:** the shared helpers each hold their stated contract; fragmentation boundary math is exact against the codec-api encode bound; the tap/ring mirror is outside the graph's accounting at all five producer sites; the lost-wakeup and holdover-purge disciplines are structurally sound.
- **Locks:** the pure state machine is sound (single holder, FIFO-beneath-held, generation guard, purge accounting — property-tested); every grant-then-purge sequence is synchronous within one critical span; cancel-safety on all four §15.20 paths is implemented and guarded; `send`'s single deadline bounds acquire plus write.
- **Graph/config/resolver cores:** the three graph rules, numeric maxima, name legality, `deny_unknown_fields`, the write-mode promotion's single shared helper, and the usb-identity ambiguity guard (both directions, counted over devices) all verified as shipped; the resolver findings above are all in the input arms around that core.
- **Web server:** the split credential, every-cookie-value checking, Host/Origin validation, the bridge's parse-one-request allowlist, TLS pair atomicity, and the pre-auth pool eviction shape all verified against their p12 guards.
- **Two hypotheses the finders killed before filing** (recorded so they are not re-found): the exec stdin feed's in-flight chunk (EXEC-1 — already cleared twice; unchanged) and a suspected `Daemon::state` reparse cost (review 19 OPSIMP-3 — the deliberate keep stands).

## 7. Verification record

- 83 verified verdicts (74 area + 9 gap), all CONFIRMED; zero refuted at the verification stage. Verifiers received the claim and the tree only, never the finder's reasoning; the tree did not move during the pass (AGENTS.md §9).
- The five highest-impact verdicts were re-derived by hand against the code before this document was written; the mechanisms reproduce as claimed (37-LEG-1's post-dequeue epoch snapshot, 37-LOCK-1's absent reclaim driver, 37-RES-1's by-id-only iteration, 37-CTRL-1's discarded `try_send`, 37-CTRL-2's in-arm `write_all`).
- Per AGENTS.md §9, any remediation of 37-LEG-1, 37-LOCK-1, or 37-CTRL-2 must carry fail-first proof (each has a stated regression shape above), and the flaky-class items (none found this round) would need independent re-verification.

## 8. Recommended order of attack

1. **The three correctness majors in the daemon:** 37-LOCK-1 (held pty wedge), 37-LEG-1 (missed receiving-side purge), 37-CTRL-2 (write-stall starving arbitration) — each is a §6/§5 promise broken on a reachable path.
2. **37-CTRL-1** (tap.closed delivery) plus its decision: guarantee delivery or amend §10 — either way with the full-queue regression.
3. **37-RES-1..6 as one resolver session** — the input arms share the fixes and the missing tests.
4. **The two packaging majors** (37-PKG-1, 37-PKG-2) — deployment-breaking, invisible to the suite, cheap to fix in the unit and README.
5. **The documentation majors** (37-DOC-1..3) — AGENTS.md §2 makes these defects, and all five sites are single-paragraph fixes.
6. **The harness self-trust items** (37-TEST-1, 37-TEST-2, 37-SEAM-1, 37-TOOL-5) — gates that cannot fail, before the next change relies on them.
7. Everything else in severity order; the web client races (37-WEBC-1/2/6) profitably land together with a shared in-flight-selection guard.

Every finding above requires a remediation-ledger disposition (including deliberate declines) per AGENTS.md §5 before this review is considered closed.
