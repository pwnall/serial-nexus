# Review 26 — remediation ledger

**What this is:** a finding-by-finding disposition for every item in
`docs/26-claude-opus-code-review.md`, with the file, test, or reasoned refusal that
answers it.

**Why it exists.** The review carried 93 surviving findings across five severity tiers.
The remediation closed them across nine parallel work packages, and the summaries in
`AGENTS.md §2` and `docs/implementation-notes.md` cover the headline items well — but
nothing mapped the 93 *ids* to outcomes. The completeness audit below had to reconstruct
that mapping from scratch, and observed that the next auditor would pay the same cost
again. So it is written down. If you are re-filing a finding from review 26, look for its
id here first: it may be answered, or **deliberately declined with a reason**, and a
declined item that gets silently re-fixed is its own kind of defect (the review's §6
cleared 22 candidates for exactly that reason).

**Status of the two documents.** `docs/26-…-code-review.md` is a frozen record of the
review *as delivered* — it still reads "nothing is fixed yet", because it was written
before this work. This file is the answer to it.

---

## Part 1 — the completeness audit

Produced by an independent agent instructed to refute the claim that the findings were
addressed, working from the review and the tree, not from any implementer's report. Its
text is reproduced as written; the items it found still open are answered in Part 2.

**Method.** Read the whole review, then verified against the tree at its current (uncommitted) state. Live reproductions ran against `target/debug/*` on a daemon booted at `XDG_RUNTIME_DIR=/tmp/snx-verify.K35TyH` (now removed; `git status` shows only remediation files, no probe residue). Baselines I re-ran myself: `cargo build --workspace --locked` OK; `cargo fmt --all --check` OK; `cargo clippy --workspace --all-targets --locked -- -D warnings` exit 0; minimal-daemon clippy exit 0; **`cargo test --workspace --locked` = 434 passed / 0 failed / 4 ignored** (was 265 at `b8d8ed8`). `cargo check` in `fuzz/` compiles the three new targets.

## Ledger

### §1 Critical / High

| ID | Disposition | Evidence |
|---|---|---|
| **WEB-1/SEC-1** bridge bypass | **ADDRESSED** | `bridge.rs:159` `screen() -> Result<Value,String>`, `forward_line()` re-serialises. Live WS frame `{…"info"}\n{…"teardown"}` → `{"error":{"code":-32600,"message":"invalid request: a frame must be exactly one JSON-RPC request object"}}`, graph intact. Denylist→**allowlist** (`ALLOWED` const). Guards: `bridge::tests::a_second_request_behind_a_newline_is_refused_never_forwarded`, `an_unknown_future_verb_is_refused_by_the_allowlist`, itests `web_ws_frame_cannot_smuggle_a_second_request` / `…_a_shutdown` |
| **RV-11a** `replay_ring` unbounded | **ADDRESSED** | `config.rs:57 MAX_REPLAY_RING = 16 MiB`. Live: `replay_ring = 1152921504606846976` → `-32002 … above the maximum 16777216 (…range-checked before anything is created, §11)`; `16777216` loads. Guards: `p9_config_validation::absurd_replay_ring_*` (2), `config.rs` proptest `prop_…` over `(MAX+1)..=usize::MAX` |
| **LOAD-1/RV-11b** `hostward_buffer` | **ADDRESSED** | `MAX_HOSTWARD_BUFFER = 65_536`. Live: `18446744073709551615` + `--replace` → structural error, **running graph survived** (`usb0` still present). Guard: `absurd_hostward_buffer_under_replace_leaves_the_running_graph_intact` |
| **CODEC-1/WIRE-1** mux default write_mode | **ADDRESSED** (structural arm chosen) | Live: omitted mode → `-32002 … edge 0 (usb0 -> mux) feeds a codec's multiplexed endpoint with write_mode = "on-demand"; only "held" or "never" work there…`; with `held`, `accepted_targetward` advanced 0→12. Example config updated (`packaging/serialnexusd.example.toml:131-140`). Guards: `codec_mux_edge_without_write_mode_is_a_structural_load_error`, `held_mux_edge_actually_advances_accepted_targetward` |
| **MAP-1** `spchex` | **ADDRESSED** | `nexus-core/src/map.rs:151` `b == 0x7f \|\| (b < 0x20 && b != 0x09 && b != 0x0a && b != 0x0d)`. Oracle re-transcribed from upstream (`oracle_one`, `special` enumerated independently). Guards: `spchex_maps_the_control_class_not_space`, `the_hex_family_can_render_every_byte`, `single_mapping_matches_the_oracle_over_all_256_bytes`, itest `spchex_hexes_the_control_class_never_space_…`. Behaviour change documented in design §7.8, notes §v11 track, `docs/rpc/configuration.md:425` |
| **SEC-2** unix leg 0775 | **ADDRESSED** | `leg.rs:1342 apply_socket_perms`. Live: `srw------- 600 …/leg.sock`. `docs/security.md:86` table row. Guard: `p9_permissions::a_listen_unix_leg_binds_its_socket_owner_only` |
| **SEC-3/WEB-7 (6b/#28)** Origin + port scoping | **ADDRESSED** | Live: `Origin: http://127.0.0.1:9999` → **403**; `Origin: http://evil.example` → 403; own authority → 101; absent Origin → 101 (documented judgement call, `server.rs:261-268`). Guards: `a_sibling_port_on_the_same_host_is_refused`, `an_origin_on_this_very_authority_is_accepted`, `ssh_forwarding_keeps_working`, itest `web_origin_is_validated_against_the_requests_own_host` |
| **CTRL-1/CP-1** pipelined request | **ADDRESSED** | Live raw socket: `lock --wait` then `state` → `-32006 "a waiting verb is already in flight on this connection…"`, **connection alive**, `waiters:["console2"]` intact. EOF semantics preserved (half-close → waiter removed, conn closed). Connection reusable after the wait resolves. Guards: `p9_control_pipelining` ×4 incl. `eof_on_the_request_half_cancels_the_parked_wait` |

### §1 Medium

| ID | Disposition | Evidence |
|---|---|---|
| **INV5-CLIPPY-SCOPE** | **ADDRESSED** | New `nexus-daemon/clippy.toml`. **I re-ran the reviewer's probe**: planted `zzprobe.rs` with a `RefCell` + `len_zero` canary → `cargo clippy -p nexus-daemon` now reports **3× "use of a disallowed type `std::cell::RefCell`"** plus the canary (previously canary only). `meta_gates::refcell_ban_covers_every_crate_that_holds_daemon_state` **failed** with the probe planted; probe removed by targeted Edit + `rm`, tree verified clean |
| **CP-2/CFG-3** unknown keys | **ADDRESSED** | `config.rs:43,476,508,933` `deny_unknown_fields`. Live: `[[nodez]]` → `unknown field \`nodez\`, expected \`node\` or \`edge\``, running graph survived. Non-empty file parsing to nothing refused **client-side** (`serialnexusctl`), zero-byte file still legal. Guards: `misspelled_config_key_is_rejected_naming_the_key`, `a_file_that_parses_to_nothing_cannot_destroy_a_running_graph` |
| **TAP-1** orphaned taps | **ADDRESSED (daemon half only — browser half open, recorded)** | Live: `tap.open` then `load --replace` → `{"jsonrpc":"2.0","method":"tap.closed","params":{"endpoint":"usb0","reason":"graph replaced","tap":0}}`. Later `tap.close` for a dead id → `-32602`. Design §10 amended. Guards: `p8_tap` ×5 (`load_replace_…`, `teardown_…`, `remove_node_cascade_…`, `add_node_and_unrelated_remove_leave_a_live_tap_attached`). **`app.js` still does not act on `tap.closed` and `offsetSpaceReset` is not implemented** — explicitly recorded, notes lines 144-154 |
| **LEG-2** unbound growth | **ADDRESSED** | `UnboundSet` = capped `Vec` + `HashSet` dedup + `truncate_identity` + `overflow` counter, cleared per connection. Guards: `unbound_identities_are_capped_and_the_overflow_counted`, `a_long_unbound_identity_is_truncated_with_a_marker`, itest `a_peer_streaming_fresh_channel_identities_cannot_grow_the_unbound_list` |
| **LOCK-1** mutual deny | **ADDRESSED** | `lock.rs:211-219` — caller's own `Held` mode settled before the FIFO check, with the §15.23 rationale in the comment |
| **MAP-1 (runtime)** read-only map | **ADDRESSED** | `map.rs:240-245` `targetward_drain` keeps the receiver alive + counts. **Live A/B repro of §7 item 21**: raw edge `never`, client attached → `client_present:true` at t=2 s; after client exit → `client_present:false` (previously stuck `true`). Guard: `p8_map::a_read_only_map_leaves_its_writers_pty_alive` |
| **RV-4** two held origins | **ADDRESSED** | Live: two maps on one endpoint → `-32002 … host-facing endpoint usb0 has two held origins (m1/raw and m2/raw)…`. Promotion moved into shared `GraphConfig::effective_write_mode` (notes §3.17 refinement). Guards: `two_promoted_held_edges_on_one_endpoint_are_refused_naming_both_offenders`, `free_for_all_endpoint_still_accepts_two_held_edges` |
| **DM-3 / MAP-UNATTACHED-LOSS** | **ADDRESSED** | Live: serial→map, no consumer → `hostward.discarded_unattached: 512` (was: no counter at all). Counted **inside** `runtime::fan_out` |
| **DM-2/LEG-1** purge to quiescence | **ADDRESSED** | `boundary::drain_to_quiescence` shared by `serial.rs:430` and `leg.rs:698`. Guards: `drain_to_quiescence_drains_a_backpressured_in_flight_chunk` +2 |
| **DM-1** `faces = target` serial | **ADDRESSED** (option 2: refuse) | Live: `-32002 … serial node "outport" declares faces = "target", which is not implemented … (deferred work, §7.1/§14)`. Design §7.1 and §14 both amended. Guards: `serial_faces_target_is_refused_as_not_implemented`, `codec_and_leg_may_still_face_target` |
| **SEC-3/CP-5/WEB-3** web pre-auth | **ADDRESSED** | `MAX_CONNECTIONS = 128` semaphore, `HEAD_TIMEOUT` (live: silent peer → `HTTP/1.1 408 Request Timeout` after 15.0 s), `WebSocketConfig::max_message_size(WS_MAX_MESSAGE)`, `Secure` under TLS. Guards: `a_silent_peer_hits_the_head_deadline`, `web_pre_auth_connections_are_capped_and_time_out` |
| **SEC-4/RV-6** file modes | **ADDRESSED** | Live: `serialnexusd.state.toml` = **0600**, `logs/console.log` = **0640**. Guard: `p9_permissions::a_running_daemon_writes_its_socket_state_and_logs_owner_only` |
| **LOG-2** (withdrawn) | **DELIBERATELY DECLINED, correctly** | Default `DropOldest` behaviour unchanged. Only the *surviving suggestion* implemented: `write_errors` / `last_write_error` in `state_extra` (`log.rs:278`). Guard: `p3_log_enospc::the_default_policy_stays_active_and_separates_write_refusals` — which also closes the review's "the default arm has no test either way" |
| **LOG-1** teardown blocks runtime | **ADDRESSED** | Two-phase `signal_stop` / `teardown`; `FLUSH_WAIT` measured from `signal_stop`, so N wedged nodes cost one bound. Guards: `signal_stop_closes_the_queue_and_teardown_still_flushes`, `teardown_measures_the_flush_bound_from_signal_stop` |
| **LEG-3** faces=target backlog | **ADDRESSED** | `purge_inbound` (`leg.rs:1125`) drains to quiescence + counts the in-flight chunk; the "there is none" comment replaced. Guard: `p6_outage::a_receiving_leg_purges_its_own_targetward_backlog_on_peer_disconnect` |
| **DOC-1b** doctor P5 verdict | **ADDRESSED** | `probes.rs:407-472` `Certificate`/`CertFailure{integrity}`; `probes.rs:783` "Fold discovery and every certificate into P5's one verdict (§15.21, DOC-1b)", pure and unit-tested. `expectations/linux.jq` updated |
| **CLI-2** add-node discards | **ADDRESSED** | Live: 2-node file → `Error: … add-node takes a single [[node]] and no [[edge]], but this file has 2 node(s) and 0 edge(s) — nothing was added…`, exit 1; 1 node + 1 edge likewise refused |
| **F1** fan-out ×5 | **ADDRESSED** | `runtime::fan_out` called from `serial.rs:548`, `codec.rs:340`, `exec.rs:576`, `leg.rs:916`, `map.rs:340`. 6 unit tests incl. `fan_out_with_all_sinks_closed_counts_unattached_not_full`. Notes §3.18 records that the **targetward** half is still per-node |
| **WEB-4** selectConsole re-entrancy | **ADDRESSED — but unguarded** | `app.js:186` generation counter re-checked after every `await`; `historyKey`/`history` adopted in one synchronous step; a superseded `tap.open` is closed rather than leaked. **No test exercises `selectConsole`** (the JS suite covers `history.mjs` + `saver.mjs` only) |
| **TAP-1b (#27)** offset contiguity vs feed drops | **ADDRESSED, differently than proposed** | Not "document the asymmetry" — a real mechanism: `tap.data.gap_before` (`tap.rs:339-341`) + `feed_dropped` baseline in `tap.open`. Live: `NOTIF tap.data {'gap_before': 0, 'offset': 0, 'tap': 1}`. `docs/rpc/observation.md:377-387` states the contract ("offsets are contiguous, and a hole is always announced"). Guard: `tap_data_carries_a_gap_signal_and_a_contiguous_offset_space` |

### §1 Low / nit (the tier I was told to be exhaustive about)

| ID | Disposition | Evidence |
|---|---|---|
| **CODEXEC-2** | **ADDRESSED** | `exec.rs:527-551` — length captured before the move; both the `reacquire_held`-false and post-grant-send-failure arms charge `mux_discarded_targetward`. Guard: `mux_targetward_drop_is_counted_when_the_endpoint_was_torn_down` |
| **PTY-3** | **ADDRESSED** | `WriteShortfall{unwritten,error}` replaces `io::Result<()>`; `writer_thread` charges `counters.add_absent(short.unwritten)`. Guard: `blocking_write_all_reports_the_full_remainder_when_the_peer_is_gone` |
| **DATAFRAMES-SILENT-RESIDUAL / RV-9** | **ADDRESSED** | `map_while` → `enum DataFrame::{Piece,Residual}` a caller must match. Counted by leg (`discarded_unframable`, `leg.rs:823`), exec (`exec.rs:452`), codec. Guards: `data_frames_reports_the_residual_instead_of_truncating_in_silence`, `data_frames_residual_is_terminal`. Also the parenthetical ("nothing bounds identity length") → `graph::MAX_NAME_LEN = 256` |
| **TAP-1 (offsets)** | **ADDRESSED** | Same fix as TAP-1b above; `tap.rs:317-330` explains why the lossy hop must *not* be folded into `ingested` |
| **LOCK-4** | **ADDRESSED** | `Steal::AlreadyHeld` (`lock.rs:73,289`); proptest asserts it (`lock.rs:1045`), unit test at `:784` |
| **CFG-1** | **ADDRESSED** | Live: `flow_control = "xonxoff"` and `"rtscts"` both load; `dump` canonicalises to `rts-cts`. Guard: `p9_config_validation::every_flow_control_spelling_loads_and_dumps_kebab_case`. Notes §3.15 rewritten as WITHDRAWN-and-fixed |
| **DM-4** | **ADDRESSED** | Live: `faces = "host"` codec → `"status":"waiting","reason":"standalone re-multiplexer orientation (faces=host) has no driver; deferred work (§14) …"` |
| **DM-5** | **ADDRESSED** | Live: `state` now carries a top-level `endpoints` array — `{"endpoint":"remux","feed_dropped":0,"taps":0}` — reachable with no tap open. Guard: `endpoint_feed_dropped_is_readable_with_no_tap_open`. Documented `docs/rpc/observation.md:58-64` |
| **CP-6** | **ADDRESSED** | `resolver.rs:459` blank `bInterfaceNumber` → `-`; blank `idVendor`/`idProduct` → no usb identity (degrade to by-path). Guards: `blank_interface_number_normalizes_to_the_absent_marker`, `blank_vendor_id_yields_no_usb_identity_and_degrades_to_by_path` |
| **BND-1** | **ADDRESSED** (the note's fix, not the review's rejected one) | `daemon.rs:1364-1371` pass 1 `signal_stop` all, pass 2 join all; `Node::signal_stop` on all 7 kinds. The "shorten the poll interval" trade was explicitly *not* taken |
| **STATE-1** | **ADDRESSED** | `NodeState` is now the live holder with a private `since_unix_ms: u64` (no longer `Option`, no longer dead); `set()` re-stamps only on a real transition. Live: `"since_unix_ms":1785021639086` in `state`. Guards: `state_serializes_status_reason_and_timestamp_flat`, `set_restamps_only_on_a_real_transition` |
| **CLI-4** | **PARTIALLY ADDRESSED** | `main.rs:189-198` — daemon errors under `--json` now emit `{"error":{"code":…,"message":…}}` (live-confirmed). **Local/client-side errors still print human text**: `serialnexusctl --json load /nonexistent.toml` → `Error: No such file or directory (os error 2)`, no JSON. Not recorded as a deliberate boundary anywhere |
| **LEG-4 / LEG-5** | **ADDRESSED** | `ChannelStat::discarded_targetward` added (LEG-4, `leg.rs:160-162,989`); `peer_version`/`peer_capabilities` reset on disconnect (`leg.rs:762-763`) |
| **LOG-3 / LOG-4** | **ADDRESSED** | `rotate()` pushes `QueueItem::Rotate` into the queue (ordered); `queued_bytes` reports `queued + draining`. Guards: `queued_bytes_counts_the_batch_the_writer_still_holds`, itest `a_batch_accepted_after_rotate_never_lands_in_the_old_file` |
| **CODECAPI-1/3** | **ADDRESSED** | `FrameDecoder` gained a `start` cursor; compaction once per `push`. Guard: `batched_small_frames_decode_identically_and_report_live_bytes` (asserts observable behaviour unchanged both batched and byte-at-a-time) |
| **WEB-2** | **ADDRESSED** | `server.rs:444-446` adds `; Secure` iff this listener terminates TLS, with the "browsers refuse a Secure cookie from a non-trustworthy origin" rationale. Guard: `the_session_cookie_is_secure_only_under_tls` |
| **WEB-5** | **ADDRESSED** | New `serialnexusweb/src/assets/saver.mjs` — one write in flight per key, newest snapshot wins, errors surfaced via `storageFailed` (badge + terminal marker). 5 `node --test` cases in `history.test.mjs`; served asset covered by `every_module_app_js_imports_is_served` |
| **SEC-5** | **ADDRESSED** | `docs/security.md` gained a leg section (line 136+) and a permissions table row (line 86) naming the second unauthenticated door |
| **SEC-7** | **PARTIALLY ADDRESSED — declined half is reasoned and recorded** | 3 new targets: `rpc_request_line`, `rpc_base64`, `config_load` (all compile). The two not fuzzed — `RequestLines` and `serialnexusweb::read_request` — are declined with a written rationale in `fuzz/Cargo.toml:12-42` ("a lib target exists to be depended on"). Also in AGENTS.md §5 |
| **SEC-8** | **ADDRESSED** | `clear_stale_socket` (`leg.rs:1365`): `symlink_metadata` is-a-socket check + live-peer dial probe; teardown unlinks only `bound_unix_path`. Live: leg pointed at `notes.txt` → `faulted`, reason `exists and is not a unix socket; refusing to unlink it`, **file intact**. Guard: `a_leg_pointed_at_an_existing_file_refuses_and_leaves_it_intact` |
| **F3 / LOCK-3** | **ADDRESSED (documentation resolution, deliberately)** | `data.rs` module doc rewritten: "**Read this before trusting a test in this module**… a green property test here proves the rule is coherent … it does not prove the daemon obeys it", with a rule→shipped-location map. Notes §3.3 corrected in place, §3.18 records the disposition |
| **F7** | **ADDRESSED** | Notes §5 build block rewritten to `cargo build`+`cargo test --workspace --locked`; §2 explicitly says "Where a section below still names a `phaseN/*.sh`, read it as a dated record" |
| **F9** | **ADDRESSED** | `_endpoint` no longer exists in `tap.rs` |
| **OBS-1** | **ADDRESSED** | `daemon.rs:283-311` `add_subscriber`/`remove_subscriber`/`subscriber_count`; `emit_state_snapshot` gates on subscribers. Guard: `state_snapshots_are_gated_on_subscribers_not_connections` |
| **SYS-1** | **ADDRESSED** | `nexus-sys/src/lib.rs` — `PTSNAME_LOCK` around the non-Linux `ptsname(3)` arm, `# Safety` section stating the residual precondition, plus a concurrency test |

### §2 Testing opportunities

| Item | Disposition | Evidence |
|---|---|---|
| 1. **T2** pty-collapse guard | ADDRESSED | `p9_pty_collapse.rs`: `collapsed_client_sessions_still_release_the_write_lock`, `a_bare_hangup_leaves_the_daemon_cpu_bounded` (the spin guard the review asked for) |
| 2. **F3** | ADDRESSED (as documentation) | above |
| 3. **T5** `runtime.rs` tests | ADDRESSED | `frame_payload_cap_reserves_the_envelope_header`, `…floored_at_one_for_a_pathological_channel_id`, `frame_ranges_covers_every_byte_exactly_once`, `…of_an_empty_chunk_yields_nothing`, `data_frames_*` ×3, `fan_out_*` ×6 |
| 4. **T1** leg lock lifecycle | ADDRESSED | `p9_leg_arbitration.rs`: `the_leg_acquires_on_first_data_and_releases_when_idle`, `the_leg_releases_the_local_lock_when_the_peer_disconnects` |
| 5. **T3** `tap.close` | ADDRESSED | `tap_close_detaches_its_own_tap_and_a_stranger_connection_cannot`, `tap_close_stops_the_stream_while_bytes_keep_flowing` |
| 6. **T6** reconfigure under a live tap | ADDRESSED | `p8_tap` ×4 + `p8_tap_offsets::tap_offsets_reset_on_load_replace_while_the_instance_nonce_does_not` |
| 7. **CP-3** `--socket-group` | ADDRESSED | `socket_group_chgrps_the_control_socket_and_widens_it_to_0660`, `an_unknown_socket_group_is_a_hard_startup_error` (self-skips without a secondary group) |
| 8. **T8** TLS tier | ADDRESSED | `web_tls_tier_binds_and_secures_key`, `web_tls_round_trip` (curl-gated). The stale `// TODO(port)` at `p8_web.rs:36` is gone |
| 9. **SEC-7** fuzz | PARTIAL (above) |
| 10. numeric-range proptest | ADDRESSED | `config.rs:2664-2705` proptest over `(MAX+1)..=usize::MAX` for both fields |
| 11. **T7** harness readiness | ADDRESSED | `nexus-itest/src/lib.rs` `daemon_answers()` — connect + RPC, not `socket.exists()` |
| 12. **INV1-NO-GUARD** | ADDRESSED | `meta_gates::no_asyncfd_is_used_anywhere_in_the_workspace` (ran green; has a planted-violation self-proof and a comment-vs-code discriminator) |

### §3 Documentation

DOC-1 ✅ (`--timeout-ms 60000`; I verified `pty_echo` really is an *idle* timeout, `nexus-sim/src/main.rs:692`) · RV-5 ✅ (README uses `--send seeded:64 --expect echo`; **and the sim was fixed** — a zero-byte `--drain` now reports `pass:false`) · DOC-2 ✅ (`nc -N -U` in all 7 rpc pages) · DOC-3 ✅ (`observation.md:32-75` documents `taps` + `endpoints`) · DOC-4/CLI-1 ✅ (live: `--assert <ASSERT> … [possible values: true, false]`) · DOC-5 ✅ (points at v11) · DOC-6 ✅ (`nexus-doctor.md:91` "AsyncFd is ruled out") · DOC-7 ✅ (`--exclude serialnexusweb`) · DOC-8 ✅ (AGENTS.md inv. 5 + `cell.rs:13-22` rewritten) · DOC-9 ✅ (`security.md` §"The replay ring: the daemon holds 64 KiB of every console, by default") · DOC-10 ✅ (0.2.0; rotated-filename comment corrected) · F7 ✅ · DM-7 ✅ (`tap.rs:12-18` now "**default 65536** … since §15.32") · PTY-4 ✅ (`pty.rs:643-648` attributes the anti-spin to the `saw_data` latch reset) · AGENTS-INV7 ✅ (live: `node name "   " is whitespace-only…`; guards `whitespace_only_node_name_is_refused`, `whitespace_only_channel_identity_is_refused`, `the_reserved_empty_default_endpoint_name_still_works`).

### §4 / §5 / Appendix

- **§3.16, §3.17, §3.18** upheld and still recorded; **§3.17 refined** (promotion moved to `GraphConfig::effective_write_mode` so validator and wiring share one rule). §3.15 rewritten as WITHDRAWN-and-fixed, slot retired.
- **§4 known open issue (OPFS freeze):** daemon half fixed, **browser half explicitly still open** (notes 144-154).
- **§5:** F1 ✅, allowlist ✅, CODECAPI ✅, F9 ✅. F8/OPSIMP-3 closure noted at notes:1288.
- **RV-8** folded into TAP-1b ✅. **RV-10** ✅ — live: garbage state file → daemon starts, logs `ERROR … starting with an EMPTY graph. The file is preserved untouched`, `state` = 0 nodes, file byte-identical. `lib.rs:176-199` documents why `--config` still fails fast.

### §6 "Verified and cleared" — did the remediation break any?

**No.** Checked each of the 22: `CLI-3` — `instance` still absent from `serialnexusctl info` (zero hits in `main.rs`). `PTY-2/F6` — no `yield_now` added to the pty reader. `SEC-6` — no symlink/permission reordering in `pty.rs`. `SER-1` — the targetward mid-write accounting is untouched; only the hostward loop moved to `fan_out`. `F2` — three distinct `ChannelStat` types still exist. `F4` — 23 `lock().unwrap()` still in `log.rs`. `F5` — `serve_connection` not split. `FRAG-1` — floor-at-1 retained (with a test). `CODEXEC-4` — codec `attributes` stayed open at the config layer (live: rejection comes from the *reference codec*, not serde). `MAP-1 (config)` — live `dump` still emits the operator's `write_mode = "on-demand"`, promotion runtime-only. `LOG-2` — behaviour unchanged. `T4` — `p6_head_of_line.rs` untouched.

---

## What genuinely remains undone (prioritised)

1. **The browser half of the `load --replace` console freeze.** `history.mjs` has no `offsetSpaceReset`, and `app.js` ignores the new `tap.closed`. A live web console still stops rendering across an operator `--replace` — now with the daemon telling it, and nobody listening. This is the single largest carried-forward item and it *is* recorded (notes 144-154), so it is a declared deferral rather than an omission — but it is the top of the queue.
2. **WEB-4's fix is unguarded.** The generation counter in `selectConsole` is correct code with no test; `history.test.mjs` covers `history.mjs` and `saver.mjs` only. Reverting the guard breaks nothing in CI — exactly the T2 failure mode the review flagged.
3. **CLI-4 is half done.** `--json` is machine-readable for daemon errors and human-only for client-side errors (file not found, TOML parse, connect failure). Either extend the envelope or write the boundary down.
4. **SEC-7's two declined parsers.** `RequestLines` and `read_request` remain unfuzzed. The reasoning is sound and written, but it lives in `fuzz/Cargo.toml` comments — a place a future reviewer refiling SEC-7 is unlikely to open. Consider a line in `docs/security.md` or AGENTS.md §5 (AGENTS.md mentions the three added targets, not the two declined).
5. **`graph::MAX_NAME_LEN = 256` is a new structural rejection with no design entry.** It is justified (RV-9's parenthetical) and documented in AGENTS.md inv. 7, `docs/rpc/configuration.md:122` and `docs/codec-authors.md:158` — but the normative design's name rules (`/`-free, non-empty) do not mention it, and it is not filed as a notes §3.x deviation. By this repo's own "the design wins" rule that is the one new code-side rule the design does not state.
6. **`docs/implementation-notes.md:19` has a dangling pointer.** It tells the reader to "read the remediation entry **above** it" — the review section is the file's first section; no remediation entry exists above it. The remediation content is inside that same section.
7. **There is no finding-by-finding remediation ledger.** AGENTS.md and the notes summarise headline items well, but nothing maps the 93 IDs to dispositions. I had to reconstruct it. For a repo whose method is "every phase ends with an adversarial audit", the next auditor pays this cost again.

**Collateral:** none found. Suite grew 265 → 434 passing, 0 failing; fmt/clippy (both profiles) clean; `dump→load` round-trip still green under `deny_unknown_fields`; a genuinely empty config file still empties the graph. One thing that *looks* like a regression and is not: after `load --replace` over a `nexus-sim` pty, the serial node faults `EBUSY` forever. I proved this is kernel behaviour, not the remediation — `TIOCEXCL` is cleared only on last close of the *tty*, and the sim keeps the master open (standalone Python repro: `reopen failed -> [Errno 16] … (EXCL persists while master open)`).

---

## Part 2 — closed in response to this audit

The audit and its sibling regression hunt found eight things still open or newly broken.
All are now closed; each was proved by reverting the fix where a revert was cheap.

| Item | What was wrong | Answer |
| --- | --- | --- |
| **MAP-1's shape in `codec` and `exec`** (the audit's only *partial* verdict, and the most serious) | Fixing the map was not enough. `CodecNode::start` and `ExecCodecNode::start` return early when the multiplexed side has no attached upstream — the ordinary state during incremental graph buildout — and that `return` came *before* they claimed their channels' targetward receivers, which then died with the wiring plan. The full MAP-1 chain followed: a pty writer's task ended, `client_present` froze at `true`, the endpoint's lock wedged on a holder that was gone, and the bytes vanished uncounted. Reproduced live by the verifier. | `drain_unwired_channels` on both nodes, sweeping the multiplexed endpoint *and* the channels so neither orientation nor either exit can leak a receiver. Guard: `nexus-itest/tests/p9_unwired_interior.rs`, written against the **rule** ("a `waiting` interior node is inert, not destructive") over all three interior kinds — proved by reverting both fixes and watching it reproduce the reviewer's exact symptom. |
| **The browser half of the `load --replace` console freeze** — the largest carried-forward item | The daemon now emits `tap.closed` and `gap_before`, and `app.js` listened for neither; and `history.mjs` had no way to notice that an endpoint's offset space had restarted while the per-*boot* `instance` nonce had not, so a restored scrollback rejected every fresh chunk as already-seen. | `onTapClosed` (flush, drop the tap, mark the pane) and a `gap_before` marker in `app.js`; `offsetSpaceReset` in `history.mjs`, which re-anchors the frontier and lets the caller mark the seam rather than splicing two offset spaces together silently. Guards: two `node --test` cases, one of which asserts the freeze itself before re-anchoring. |
| **`pulse-dtr --assert` as a bare flag** (regression introduced by the CLI-1 fix) | `ArgAction::Set` made the previously-harmless bare `--assert` a hard error, and clap rendered the fix's rationale verbatim into `--help`. | `num_args = 0..=1` + `default_missing_value`; rationale moved to a `//` comment. Guard extended in `pulse_dtr_accepts_an_explicit_assert_value_in_both_spellings`. |
| **CLI-4 was half done** | `--json` was machine-readable for *daemon* errors and human-only for client-side ones (unreadable file, TOML parse, connect failure). | `main` now wraps every failure in the same envelope under `--json`, tagged `data.origin = "client"`, exit unchanged. |
| **`MAX_NAME_LEN` was a code-side rule the design did not state** | By this repo's own rule the design wins, so a new structural rejection with no design entry is a divergence. | Design §3's name rules now state both the whitespace rule and the length bound, with the reason the bound is load-bearing (an identity rides in every frame header, so an unbounded one starves the payload and defeats §5's fragment-never-drop obligation by construction). |
| **SEC-7's two declined parsers** | The reasoning for *not* fuzzing `RequestLines` and the web head parser lived only in `fuzz/Cargo.toml` comments — where a future reviewer re-filing SEC-7 would not look. | Stated in `AGENTS.md §5`, with the rule that matters: lift the parser into its own crate, never widen a published surface for a harness. |
| **The dangling pointer in the notes** | `implementation-notes.md` told the reader to see "the remediation entry above", which did not exist. | It does now. |
| **No finding-by-finding ledger** | The audit had to reconstruct one. | This file. |

Two further items the audit raised are answered as **documentation, not code**, because
the behaviour is correct:

- **The log flush budget** is measured from `signal_stop`, so a log node joined late can
  see `remaining == 0`. That is not a shortened flush: the writer is its own thread, so
  the time spent joining the nodes ahead of it is time it spent draining, and
  `recv_timeout(ZERO)` still reports a writer that finished. `remaining == 0` means "this
  writer has already had the full bounded wait and is still going" — precisely the wedged
  case §7.3 says to detach. The reasoning is now in the code.
- **The two "is anyone listening" gates disagree** (`subscriber_count` for the 5 Hz
  snapshot, `receiver_count` for lock notifications). Deliberate: the snapshot is periodic
  and serialises the whole graph, so building it for a merely-connected client was real
  waste; a lock transition is rare and human-scale. Stated at the call site.

And one is a **checklist item, not a test**, per §16.7: **WEB-4's re-entrancy guard**
(`selectConsole`) is correct code that only a real browser can exercise. The project's own
doctrine says such behaviour must appear on the tiered/manual checklist or be marked
unverified — it is now on the checklist rather than pretended-covered.

## What remains open, deliberately

- **`nexus-itest` cannot drive a browser.** `app.js`'s console-switching logic — including
  WEB-4's generation guard and the new `tap.closed` / offset-reset handling — rides the
  manual real-browser checklist (§16.7). The pure modules underneath it (`history.mjs`,
  `saver.mjs`) are `node --test`-covered on every push.
- **`RequestLines` and `serialnexusweb`'s HTTP head parser stay unfuzzed** (see above).
- The review's **§6 "verified and cleared"** list — 22 refuted candidates — was checked
  and none was "fixed" anyway.
