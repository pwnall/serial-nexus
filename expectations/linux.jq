# Per-platform expectation for a supported Linux system (plan §4.3):
#   serial-nexus-doctor --json | jq -e -f expectations/linux.jq
#
# Encodes what a supported Linux MUST report. Deliberately lenient where the
# design has a fallback, strict where it does not:
#   - Nothing may be `unsupported` (a probe contradicting the design).
#   - P2 (PTY presence) MUST be `supported` — presence-gated output has no
#     fallback (§7.2).
#   - P1 (EXTPROC/TIOCPKT) may be `supported` OR `degraded` — the §7.2
#     reconciliation poll is an unconditional backstop.
#   - P3 (serial-port fit) may be `supported`, `degraded` OR `skipped` — it is opt-in
#     behind --port (it opens a real device), so the passive run this gate normally
#     makes skips, a port whose driver refuses a control degrades by design (§13), and
#     an unreadable port skips with the reason attached. Every word being admissible is
#     the point: the clause is a PRESENCE clause, and it exists because presence was
#     the one thing nothing checked — a report that lost the P3 block entirely used to
#     pass this file while its header claimed completeness against the 7.0 baseline
#     (37-TOOL-5). `unsupported` is still refused, by the summary clause above.
#   - P4 (device identity resolution) may be `supported`, `degraded` or `skipped`,
#     and it carries a second clause below that the other probes do not need.
#     `skipped` is an adapter-less box. `degraded` is reachable on Linux and was
#     unreachable only by accident: an adapter with no serial number publishes no
#     `/dev/serial/by-id` link at all, so the loop P4 judged from ran zero times and
#     the probe reported `supported` for a box whose only identity is `by-path:` —
#     §12's documented instability warning, reported as if configs survived replug.
#     The `degraded` arm is what §12 always intended for that box; admitting it here
#     is what lets the probe say so. It does not redden a lane — the exit code hinges
#     on `unsupported`, and the summary clause above still refuses that.
#   - P5 (rig discovery/certification) may be `supported`, `skipped` OR
#     `degraded` — it is opt-in (transmits), so a run without --port skips; a run
#     against a rig that is miswired, or whose certificate has an uncertified
#     characterization item, is `degraded` with the item named: a rig fault, not a
#     doctor failure (§15.21). `unsupported` stays a gate failure, and since the
#     P5 verdict now folds the certificate in (review 26, DOC-1b) that verdict is
#     reachable: it means the rig did not round-trip data, which §15.21 makes a
#     stop condition before any tiered checklist item runs.
#
# The kernel-diff probes (P6..P12) are gated on PRESENCE, not on a particular
# answer. This is the 6.18 re-gate command (§7: `serial-nexus-doctor --json | jq -e -f
# expectations/linux.jq`), and its job there is to prove the *artifact* is
# complete — every measurement block the 7.0 baseline has, so the two runs are
# diffable field by field. The finding lives in the numbers inside each block, and
# a clause that demanded a verdict word would be asserting the very thing the
# owner went to 6.18 to find out:
#   - P6 (post-hangup pty readiness) and P7 (collapsed-session evidence) may be
#     `supported` OR `degraded`. Both answers are legitimate kernel behaviour and
#     the shipped daemon is correct under either; `degraded` means "a pending
#     simplification is unsafe here" / "one session shape is uncovered", which is
#     a warning to a future editor, not a broken box. They never skip (a probe
#     error degrades, leaving the question open), so those two are the whole set.
#   - P8 (epoll vs read(2)) and P9 (poll timeout granularity) may be `supported`
#     OR `skipped` — never `degraded`, which is the real content of this clause.
#     They are informational: the design is *justified* by P8's answer rather than
#     dependent on it, and P9 reports numbers a tuning decision is made against.
#     `skipped` covers a mechanism that does not exist (epoll off Linux) or a probe
#     that could not run, with the reason attached.
#   - P10 (pty buffer depth) additionally admits `degraded`, and that arm is not a
#     loosening — it is the clause that stops the probe lying. A depth measured in
#     the wrong line discipline is not this kernel's depth (raw and cooked differ
#     by an order of magnitude in what a peer can recover, measured on 7.0.0-29),
#     so P10 degrades when the slave it measured was not the daemon's raw baseline.
#     Forbidding `degraded` here would force the one probe that knows its own
#     measurement is unsound to report a confident number instead. On this platform
#     the baseline always takes, so `supported` is still the expected answer and a
#     `degraded` P10 on Linux is a real signal worth reading (notes §3.34).
#   - P11 (real-port line-state ioctls) may be any of the three: it is opt-in
#     behind --port like P3/P5, so the passive run that this gate normally makes
#     `skipped`, and a port whose driver lacks TIOCGICOUNT is `degraded` by design
#     (the serial node omits the counters rather than faulting, §5).
# `unsupported` remains a gate failure for all of them via the summary clause
# above — no probe here may contradict a design premise, and none can.
#
# Evaluates to `true` (exit 0) only when every clause holds.

(.summary.unsupported == 0)
and (any(.probes[]; .id == "P2" and .status == "supported"))
and (any(.probes[]; .id == "P1" and (.status == "supported" or .status == "degraded")))
and (any(.probes[]; .id == "P3" and (.status == "supported" or .status == "degraded" or .status == "skipped")))
and (any(.probes[]; .id == "P4" and (.status == "supported" or .status == "degraded" or .status == "skipped")))
# **A `supported` P4 must have resolved at least one device.** The clause above is a
# status clause and a status clause cannot see this: P4's `supported` consequence
# asserts "Resolver produces canonical identities; configs survive replug and cold
# start", and until 2026-08-05 that sentence came off a `for a in &adapters` loop
# that ran zero times whenever udev published no by-id links — §9's vacuous pass,
# caught on Darwin (`count: 0`, `status: supported`, notes §3.45 (ii)) and admitted
# by this file, which is why the clause lives here and not only in the probe. The
# Darwin shape is constructible on Linux through `--dev-root`, so this is a live
# Linux clause, not a courtesy to another platform. `canonical` is the count of
# devices the resolver produced a `usb:vid:pid:serial:iface` for; a report that omits
# the key fails too, because a report that cannot state the population cannot prove
# the verdict was computed over one.
#
# **This gate is only ever run against a LIVE report** — CI pipes
# `serial-nexus-doctor --json` straight into it (`.github/workflows/ci.yml`), and nothing
# runs it over `docs/doctor/*.json`. So "strict" costs nothing an operator will meet: a
# capture they have just taken comes from the current binary and carries the key. It
# rejects only genuinely old artifacts, which nobody validates and which honestly cannot
# answer the question. (The archive was never uniformly gate-clean anyway — six of the
# nineteen committed reports predate P13 and fail on that clause regardless.)
and (all(.probes[]; . as $p
      | ($p.id != "P4")
      or ($p.status != "supported")
      or (((([$p.observations[] | select(.key == "canonical") | .value] | first) // -1)) > 0)))
and (any(.probes[]; .id == "P5" and (.status == "supported" or .status == "skipped" or .status == "degraded")))
# **A certified pair must carry its handshake reading** (§15.52). The certificate's
# own modem map is read with the peer port *closed*, so it cannot answer what the
# wire carries; the handshake block is the only read taken with both ports open,
# and it is what turns "this rig might have RTS/CTS" into a measurement. Presence
# only, and conditioned on a pair certificate having run — whether *this* rig
# crosses RTS is the operator's cabling, a 3-wire rig is §5's own stated
# assumption, and a clause that pinned the answer would redden every honest bench
# (plan §3 rule 14). A passive run has no pair, so the antecedent is false and the
# clause is silent.
and (all(.probes[]; . as $p | ($p.id != "P5")
      or (($p.observations | any((.key | endswith(" cert"))
             and (.value | type == "string") and (.value | startswith("rate_ladder")))) | not)
      or ($p.observations | any((.key | endswith(" handshake")) and (.value | type == "string")))))
and (any(.probes[]; .id == "P6" and (.status == "supported" or .status == "degraded")))
and (any(.probes[]; .id == "P7" and (.status == "supported" or .status == "degraded")))
and (any(.probes[]; .id == "P8" and (.status == "supported" or .status == "skipped")))
and (any(.probes[]; .id == "P9" and (.status == "supported" or .status == "skipped")))
# P9's mask column must MEASURE its own framing. Identical to `expectations/macos.jq`'s
# clause: §3.52 hardcoded `shape` and the "replicates, not levels" rationale from POSIX,
# and Darwin then printed both beside a field reading `false` (notes §3.65). This kernel
# answers `true` and so reads as a 1x2 — which is exactly why the clause belongs here too,
# since a gate that only runs where the premise holds cannot catch the premise failing.
# Presence of the measurement and of the pair naming which figures go together; the answer
# stays free (§7).
and (all(.probes[]; . as $p | ($p.id != "P9") or ($p.status == "skipped") or
      ($p.observations | any(.key == "zero_timeout_by_fd_state"
         and (.value.hangup_delivered_to_a_mask_that_requested_nothing | type == "boolean")
         and (.value.shape | type == "string")
         and (.value.mask_role | type == "string")
         and (.value.read_the_separation_from | type == "string")
         and (.value.read_the_mask_spread_from | type == "string")))))
and (any(.probes[]; .id == "P10" and (.status == "supported" or .status == "skipped" or .status == "degraded")))
# P10 carries an instrument check on its own FIONREAD reading. `peer_pending_input_bytes`
# answers 0 on a Darwin pty master with 1024 bytes recoverable (6 of 6 captures, notes
# §3.45 iv / §3.50), so a bare `0` there reads as "empty" and is not. What is gated is the
# PRESENCE of the cross-check, not its answer — the answer is the kernel's to give.
and (all(.probes[]; . as $p | ($p.id != "P10") or ($p.status == "skipped") or
      (["slave_to_master_targetward","master_to_slave_hostward"] | all(. as $d |
         $p.observations | any(.key == $d
            and (.value.peer_pending_input_trust | type == "string")
            and (.value | has("peer_pending_input_bytes_at_drain"))
            and (.value | has("writer_pending_input_bytes")))))))
# P10's recheck must be a LADDER. Identical to `expectations/macos.jq`'s clause,
# deliberately: `f8315cc` replaced a one-rung recheck — which could not separate a capacity
# from any watermark above its single drain size — with a multi-rung one, and neither
# expectation file was touched, so deleting `recheck`, emptying `ladder`, or handing back
# exactly the one rung the commit exists to replace all left both gates green (notes
# §3.65). Cardinality, not answer: this kernel refuses no rung and that is a legitimate
# reading; what may not happen is a bracket reported from a single point.
and (all(.probes[]; . as $p | ($p.id != "P10") or ($p.status == "skipped") or
      (["slave_to_master_targetward","master_to_slave_hostward"] | all(. as $d |
         $p.observations | any(.key == $d
            and ((.value.recheck.ladder | type == "array") and (.value.recheck.ladder | length >= 2))
            and (.value.recheck.ladder_reading | has("rungs_carrying_a_bound"))
            and (.value.recheck.ladder_reading | has("watermark_threshold_gt"))
            and (.value.recheck.ladder_reading | has("watermark_threshold_le")))))))
and (any(.probes[]; .id == "P11" and (.status == "supported" or .status == "degraded" or .status == "skipped")))
# P12 (session-boundary edge, §15.39) is inert on Linux by design — the retained
# `TIOCPKT_IOCTL` packet carries §6's detach-release here, which is P7's subject —
# so `skipped` is the *expected* answer and the clause is presence-only. It is
# gated tightly on macOS instead (`expectations/macos.jq`), which is the platform
# where it is the only mechanism. `supported` would mean a Linux kernel grew the
# edge too, which is interesting and not a failure.
and (any(.probes[]; .id == "P12" and .status != "unsupported"))
# P12's anti-spin claim must arrive with its witness: six committed captures print
# `idle_edges_in_200_passes: 0` with no elapsed time and no control, and a zero from a
# latch that cannot post an edge is not a measurement (§9). The `idle_windows` arm is the
# escape hatch for a box with no pty — an error that NAMES itself, which is what §7 asks.
# Gated here as well as on macOS even though the latch is inert on Linux: the windows run
# on both platforms, and a Linux report carrying `control_session_edge: false` beside a
# full set of executed passes is the inert arm proving itself inert (notes §3.50).
and (all(.probes[]; . as $p | ($p.id != "P12") or
      ((($p.observations | any(.key == "control_session_edge"))
        or ($p.observations | any(.key == "idle_windows")))
       and (($p.observations | any(.key == "idle_window_paced"
              and (.value.elapsed_us | type == "number")
              and (.value.passes | type == "number")))
        or ($p.observations | any(.key == "idle_windows"))))))
# P13 (last-close disposition of unread client bytes) is presence-gated for the same
# reason as P6/P7: all three of `retains` / `discards` / `waits-then-discards` are
# legitimate kernel policies and the daemon is correct under each, so demanding a
# verdict word would assert the very thing a cross-kernel run goes looking for. It
# never skips — a probe error degrades, leaving the question open — so those two words
# are the whole set. Its `policy` and `close_microseconds` observations are the
# content: the first is the answer, the second is what separates `discards` from
# `waits-then-discards`, which no other probe in this set can tell apart.
and (any(.probes[]; .id == "P13" and (.status == "supported" or .status == "degraded")))
# P14 (maximum-rate search, §15.51) is opt-in behind `--port` *and* needs a
# cross-paired rig, so `skipped` is the expected answer on every passive run and
# on every box with one adapter. `degraded` is admissible and is not a
# loosening: §15.51 gives that word to exactly three states in which the probe
# could not *ask* its question — P5's rate ladder did not round-trip under it,
# the search did not complete, or the closing restore did not put the rig back —
# and forbidding it here would force the one probe that knows its own
# measurement is unsound to report a confident number instead, which is the
# defect `expectations/linux.jq`'s P10 clause already exists to prevent.
# `unsupported` stays a gate failure through the summary clause: a rig that tops
# out at 115200 is slow, not a contradicted design premise.
and (any(.probes[]; .id == "P14" and (.status == "supported" or .status == "skipped" or .status == "degraded")))
# **And the answer is never pinned.** The ceiling is a property of the operator's
# silicon — an FT232R stops at 3 Mbaud, an H-series part advertises 12, and a
# Darwin ask-surface may refuse below either — so a clause naming a number, or
# even a `ceiling_kind`, would be this file asserting the very thing the probe
# was written to find out (plan §3 rule 14). What is gated is that the two cells
# a reader needs are *present*: the number, and the reason the search stopped.
# Both may read `null` — an incomplete search has no ceiling and says so, and the
# absence of a reason must not be dressed up as a fifth reason — so the clause
# tests `has`, never a type and never a value.
and (all(.probes[]; . as $p | ($p.id != "P14") or ($p.status == "skipped")
      or (($p.observations | any(.key == "max_reliable_baud"))
          and ($p.observations | any(.key == "ceiling_kind"))
          and ($p.observations | any(.key == "structural_max_baud" and (.value | type == "number")))
          and ($p.observations | any(.key == "ceiling_is_a_floor_over" and (.value | type == "string"))))))
# And the clause that makes the ones above worth having: a kernel-diff probe that
# RAN must carry measurements. A verdict word cannot be diffed, so a probe whose
# observations went empty would pass every clause above while making the 6.18 run
# useless. (`skipped` is exempt — it measured nothing by definition, and its
# `reason` says why.)
and (all(.probes[]; . as $p
      | ((["P6","P7","P8","P9","P10","P12","P13","P14"] | index($p.id)) == null)
      or ($p.status == "skipped")
      or (($p.observations | length) > 0)))
# And the clause that closes the hole the 2026-07-27 6.18 run walked through: an
# artifact must say what produced it. That run came from a `fe1c52c`-vintage
# binary rather than HEAD, and the only reason anyone noticed was that its P4
# section still carried the pre-`RES-2` *title* — read by eye, after the fact.
# Every clause above passed it, because none of them could see the probe set move.
#
# `probe_set` is the load-bearing half: two reports with the same fingerprint
# asked the same questions of their kernels, which is precisely the precondition
# "diffable field by field" names, and it is computable anywhere. `commit` must be
# *present* but may read `unknown` — a build from a source tarball, or inside a
# container without git, cannot know it, and failing a healthy box over that would
# be the false negative this file's P4 clause already refuses to make.
and (.build.probe_set | type == "string" and length > 0)
and (.build.commit | type == "string" and length > 0)
# The cell-set digest, and the half of the provenance `probe_set` cannot supply:
# `probe_set` says the two runs asked the same questions, never that they carry the
# same cells (four commits in `docs/doctor/` print one `a131e1f4b46d6c83` across
# five observation sets). Structure only — its *value* is a property of the run
# (which ports were named, which kernel, which histogram keys were observed), so
# pinning it here would redden a healthy box. What the clause buys is that the
# field cannot go missing: a report with no field set cannot be compared cell by
# cell against one that has it, and an absent digest must read "unknown", never
# "equal". Reports captured before 2026-08-05 fail it, which costs nothing — this
# file gates fresh `--json` output, never `docs/doctor/*`.
and (.build.field_set | type == "string" and length == 16)
