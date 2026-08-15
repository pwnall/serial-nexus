# serial_nexus — implementation notes & handoff

**As of:** 2026-08-12 (**phases 0-8 + simplification + extension + web-console + v10 + v11
console-map + review-26 remediation + v12 graph-editing + v13 browser-UI automation +
review-32 remediation tracks done**, plus the **v14 rename track** — every binary, crate
and default path moved to the `serial-nexus-*` / `serial_nexus_*` family, design §15.40,
plan §17, with the consumer-context scrub of §15.41 beside it — the **review-37
remediation**, all 82 findings dispositioned, the 2026-08-05/07 doctor/rig/replug sessions
(§3.29–§3.74 below), the **v15 documentation generation**, the **v16 documentation
generation**, and the **v17 documentation revision** (2026-08-12; entries below).)
**Branch:** `implementation` (off `main`).
**One name was retired tree-wide on 2026-08-12** (§3.81): the privileged helper is
`serial-nexus-devprep` (`devprep/`), and entries written before that date were
**mechanically rewritten** to it. Commands quoted in older entries therefore use the current
name rather than the string that was typed; `docs/historical/` and the commit messages keep
the original spelling. The word *replug* still means the operation everywhere it appears.
**Normative docs are now v17:** `docs/43-design-claude-fable-v17.md` (design) and
`docs/44-implementation-plan-claude-fable-v17.md` (plan). v1–v16 docs, the reviews and their
remediation ledgers are in `docs/historical/`. Section references (§) point at the v17
design, whose §1–§17/§15.N/§16.N numbering is deliberately identical to v16's and v15's
(§15.56, plan §3 rule 22, and plan §18 items 47–55 are v17's appended additions).

---

## THE v17 GENERATION — the post-landing record folded, and the pattern wait specified (2026-08-12 session)

**What landed.** A new normative pair: `docs/43-design-claude-fable-v17.md` +
`docs/44-implementation-plan-claude-fable-v17.md`, with the v16 pair moved byte-identical to
`docs/historical/` and the entry points bumped in the same commit — AGENTS.md §2, README's
documentation index, and the four filename-keyed code sites: `itest/tests/meta_names.rs`'s
§15.41 ban-statement allowance and citation-gate scoping pair, and `itest/tests/meta_derive.rs`'s
`PLAN`/`DESIGN` consts (AGENTS §2's landing list undercounted everything past the first — it
predated both the citation gate's scoping check and item 40's gates — corrected with this
landing). Unlike v16 (a full rewrite), v17
is an **edit of v16's bytes**, so rewrite invariant 2's acceptance test applies directly: the
diff against the predecessor contains nothing outside the intended-changes list below. Every
§15/§16 entry number, plan §3 rule number, and plan §18 item number is unchanged; additions
append (§15.56; plan §3 rule 22; plan §18 items 47–55).

**The intended changes, enumerated.**

1. **The pattern wait specified** (the one new capability): §10 gains *The pattern wait* —
   `tap.wait`, a waiting verb on the observation surface under §15.20's doctrine unchanged:
   one to eight named byte patterns (literal or bytes-regex) on a linear-time engine with
   §16.12 maxima throughout, hub-exact matching with gap-resets-window honesty, splice-exact
   replay inclusion, a result-shaped timeout distinct from the typed teardown outcome, results
   on the parked request's reply only, and no web-bridge admission at introduction. §15.56 is
   the decision record (motivation, dependency reality, and five declines); plan §18 item 47
   the construction — design ahead of tree, AGENTS §5's amend-first order. The adjacent
   candidate shapes in the same input were dispositioned without change: one was already
   tracked as plan §18 item 16 exactly as filed, and the rest named no capability gap.
2. **The post-landing record folded at its sources** (notes §3.76–§3.81): §5's tripwire-table
   helper row, §15.45's status line, and §12's privileged-mechanism sentence now carry
   §15.55's amendment (two capabilities; the shown, run, and verified commands derive from
   `REQUIRED_CAPS`) — the first two had still read "one capability"; plan §2's devprep row
   likewise; §15.52 gains the 2026-08-12 **3-wire** re-measure annotation (two independent
   instruments; §3.63's measurement not refuted, its precondition gone); the plan's
   named-scopes block gains the legitimate-drop rule (notes §3.80); plan §3 gains rule 22 (a
   gate proves its own execution — the three measured instances of
   passing-output-identical-to-not-running, notes §3.74/§3.77/§3.78); and the doctor-gate
   spellings in plan §3 and extraction-box item 3 are corrected to CI's one-run `--json-out`
   form per rule 21 (AGENTS §3's mirror corrected in the same commit). Three catch-up folds
   from *earlier* session records ride with this item, each named in the design front matter:
   §13 clause 8 gains P5's fourth discovery arm (`hung up (peer closed) — not classifiable`,
   degrades — the P6–P11 session's record), §15.46 gains its two refutation chains
   (§3.40/§3.45), and plan §3's sim-verdict description gains the `overshoot`/`budget_met`
   fold (the review-32 audit session's record).
3. **Figures repaired where they stood:** §15.51's `platform-refused` exemplar replaced with
   the committed `42eac2a` reading — every rung to 3000000 accepted, the 3062500 ask refused —
   because the "caps the ask at 230400" parenthetical was the pre-implementation expectation
   and no committed capture supports it; §15.48's provider wall-clock ratio re-cited to notes
   §3.43 (where 6.30 s / 0.55 s actually lives); §15.44's withdrawn register extended with the
   un-artifacted poll-calibration readings (§3.41/§3.44); the Status table's 835 row annotated
   **attribution unreconciled** — notes §3.68's verbatim record reads 830 and 834/0 · 833/1,
   never 835 — with item 30's evidence corrected to match; and the closing register's
   `crossover_rig_map_node_both_directions` citation corrected §3.67 → §3.65 F.
4. **Clarifications the implementation record showed were needed:** §7.1's write-arbitration
   pointer (thirteen sites once misattributed §6's rule to it) and the disconnect-latency
   sentence (a destroyed tty fails the in-flight read at once — §3.54); §7.2's pty-seat
   orientation parenthetical (§3.32's inverted labels); §10's `id: null` and
   result-XOR-error protocol clauses (§3.5); §5's single-sink reconciliation caveat
   (`delivered + discarded == streamed` holds per sink, never across a fan-out); §13's sixth
   vacuity shape (a `timeout` whose stub never parks — LEG-3), the temporal self-testimony
   clause (Err at creation, Ok after hangup — §3.45), the probe-restore rule (§3.68), and the
   heuristic-scope corollary (§3.34); §15.44's abstain-arm rewording (§3.48); §15.45's
   Stale-verdict sentence (§3.62); §15.51's single-predicate caveat (§3.61); plan §3 rule
   amendments (3, 6, 8, 10, 11, 17, 19 — each citing its bought incident) and the
   citation-gate paragraph (§3.60); plan §2's binary-named-for-its-callers heuristic (§3.81);
   and the §15 topic index extended through §15.56 — it had stopped at §15.53 while
   §15.54/§15.55 already existed.
5. **New work filed, none promised as existing:** plan §18 items 48–55 — the macOS doctor
   gate's ENOEXEC observation and its first green execution owed (48); the python3 required
   mode and the skip-message-naming meta-gate (49); harness scaffolding consolidation beside
   item 24 (50); the client socket-fallback hoist into rpc (51); devprep parity, JSON grant
   reporting, the shared replug envelope, and orphaned-bless hygiene (52); the kit
   resync-accounting suite with its kit-honesty negative (53); the macOS lint gap (54); and
   the drifted-copy comment/dead-code sweep (55). Item 17 gains its bench re-inspection
   clause, item 19 its two named competing readings, item 20 §3.56's outcome table by
   reference. The closing register gains the rust-toolchain decline (previously notes-only),
   the editor-spec recorded lead, the `--show-output` open question, and the deliberately
   not-scheduled figure-to-artifact bijection gate.

**What deliberately did not change:** every recorded decline and refutation stands (overturned
ones stay marked at their entries); the withdrawn-figure bans are extended, never relaxed; the
citation notation is unchanged; the required-mode lattice's authority stays the code; no §14
register entry moved; and the era record and both digests are untouched — v17 is a documents
change with no capture and no probe edit.

**How the alignment obligation was executed.** The revision was authored from a disposition
map built by a thirteen-agent input digest — seven readers over the whole notes file
extracting unfolded deviations, misreading patterns, and contradictions (each verifying
absence against the v16 pair by grep before reporting); two code-grounding surveys for the
pattern wait (the daemon's tap/hub/waiting-verb mechanics and the client surface, with
file-line cites); and four record-checked improvement hunts that killed their own candidates
against the §15/§16 decline register (one candidate reported DEAD on notes §3.14/§3.20's
standing decline rather than re-proposed). The assembled pair was then adversarially verified
against the v16→v17 diff before landing, and the record closes this entry: **eight independent
checkers**, one per dimension — diff-versus-intent (rewrite invariant 2's acceptance test),
anchor stability, no-silent-reversal, request-acceptance mapping, tree-fact truth, citation
resolution, hygiene and figure-scope, and cross-file landing consistency — returned one
blocker, eleven majors, and twenty-five minors, every one repaired before the landing commit.
The majority clustered on two authoring drops the checkers caught exactly as designed — an
intended §15.46 fold that had not landed, and the quoted-ratio clause landing on rule 14
instead of rule 19; both were then executed as intended — plus this entry's own enumeration,
brought back into agreement with the diff. Three findings reached the *tree*: the harness's
blessed-helper remediation string still instructed the pre-§15.55 single-capability `setcap`
(an instruction whose product the helper's own `--verify` rejects — repaired to point at
`scripts/bless`, which derives the set), the bless header comment's hand-written capability
set (repaired to derivation phrasing), and the devprep `status` line's stale informational
spelling (filed under item 52, not patched blind). The checkers also confirmed what they were
pointed at: all 55 §15 entry titles byte-identical with §15.56 appended and none reused, the
historical move R100 byte-identical, the ban-statement counts exactly 3 per stating file,
§15.51's replacement figures artifact-true against the `42eac2a` triple, and §3.68's verbatim
record carrying no 835.

**Gates at landing.** `cargo build`/`fmt`/`clippy` (workspace and minimal-daemon)/`deny`, both
Apple compile cross-check triples, the doctor jq gate in CI's one-run form (doctor exit 0, jq
exit 0 on `expectations/linux.jq`), and all meta-gates green — the retired-names gate earned
its keep once during authoring, catching a draft plan item quoting the retired helper name,
paraphrased per §15.40. The suite (default CI scope, `--no-fail-fast`) reads **894 · 1 · 6**
over 121 test-result lines, the one failure being item 46's `p3_idle_cost` at its verbatim
recorded signature (3.80 % against the 3.50 % tripwire, "38 ticks over 10s"), reproduced in
two of three runs — once under deliberate parallel load and once on a quiet box at load 0.33 —
on product code this revision does not touch: item 46 resurfacing, exactly as filed, its
disposition unchanged. The figure and its scope are in the plan's Status table. One
process observation for the record: this session's first two suite invocations piped cargo's
output into `tail`/`grep`, which replaced the exit status — rule 22's own class, caught on the
third run by writing the log and reading the status separately; the recorded figures are the
third run's.

---

## THE v16 GENERATION — the full rewrite for new developers (2026-08-10 session)

**What landed.** A new normative pair: `docs/41-design-claude-fable-v16.md` +
`docs/42-implementation-plan-claude-fable-v16.md`, with the v15 pair moved to
`docs/historical/` and the entry points (AGENTS.md §2, README's documentation index, the
§15.41 ban-statement table and the §3.NN-scope list in `itest/tests/meta_names.rs`) bumped in
the same commit. Unlike v12–v15, which were edits of their predecessor's bytes, v16 is a
**full rewrite** optimized for a new developer: overview first, per-part sections diving
progressively into detail, justifications compressed into the status-headed decision record
(§15/§16, every entry keeping its v15 number — none dropped, none renumbered, none reused),
stale history deleted, and the current-era figures moved out of prose into the plan's Status
table (one row per figure, with scope/date/commit, quotable only with scope). New primary
content, each consolidating what the record had already built or decided: §8's
"Validating your codec" chapter (the kit, the exec battery, the corruption recipe, the
golden vectors, the consumer template — with the codec-side gaps filed as plan §18 items
32–39 rather than promised); §13's instrument-validity chapter (self-testimony, zero-witness,
the vacuity taxonomy, proxy-guard rules, gate blind spots) and comparability-ladder/era-record
section; §5's loss-taxonomy table and tripwire table; §14's deferral-state vocabulary and
register; plan §3's rules 17–21 (appended, never renumbered) and required-mode table; and
plan §18 rewritten under a fixed item schema — items 1–15 keep their v15 meanings, items
16–44 are filed this generation (carried residuals plus the codec/testing construction
items), all open at landing.

**How the alignment obligation was executed.** The v12/v13 hazard — a rewrite silently
dropping rules the code still enforces — was met structurally, since a full rewrite cannot
use the sentence-granular diff: (1) a 17-agent digest fan-out produced must-preserve
inventories (every rule, decline, refutation, and measured claim in the v15 pair, notes
§3.1–§3.74, review 37 + ledger, the codec surface, and the repo ground truth); (2) a
disposition map assigned every v15 section/entry a fate (BODY-AT/APPENDIX/STUB; two DROPs
total, both justified) and an anchor-stability contract built from tree greps; (3) nineteen
section writers wrote to that map; (4) ten independent adversarial verifiers checked the
assembled pair for dropped rules, reversed declines, wrong facts, broken anchors, hygiene,
code truth (33 mechanism claims spot-checked against source), cross-document consistency,
the fresh-reader experience, and the codec-author experience. The verifiers found and the
fix pass repaired: 7 blockers (a `STANDS` verdict wrongly minted for the shipped
`connect`/`disconnect` half of §15.25's deferral; the pty `pending` payload misfiled as open
work; a `regen-golden` cargo feature asserted that does not exist; a fixture-naming promise
the plan did not keep; a self-contradiction about the dual-scope 835 measurement; two
extraction-box pointers at a section that did not carry the rules; three unresolvable "box
item" citations), ~35 majors (including three duplicated rule statements collapsed to
single-home-plus-pointer, a fused "five later commits" caveat, an un-artifacted ~13.8 KiB
figure re-cited to committed captures, P14's trial policy restored to §15.51, and the
certification map in §8 corrected against what the kit can actually observe), and ~40
minors. The fresh-reader verifier answered all five comprehension questions correctly from
the pair alone.

**Repairs to standing files made at the landing, each scheduled by the record:** AGENTS §2
rewritten from the v15 wall-of-text to a summary pointing at the plan's Status table and
§18 (its claims-match-reality and same-commit rules kept verbatim); AGENTS.md's glued
`## §3` heading restored to its own line; AGENTS §3's rig-lane spelling gains
`SNX_WEB_UI=required` (notes §3.68 measured the 835 figure with it set; the line had omitted
it); `itest/tests/p8_web.rs:952`'s bare `(§14.3)` respelled `(plan §14.3)` and
`expectations/linux.jq:1`'s `(plan §4.3)` respelled `(plan §3)` — the two wrong-but-real
citations the anchor sweep found, recorded as executed in plan §18 item 23 (d).

**What deliberately did not change:** every §15/§16 entry number and every plan §3 rule and
§18 item number (the anchor contract: an entry cited by frozen artifacts, code, or gates is
never renumbered); every recorded decline and refutation (overturned ones marked OVERTURNED
at their entries); the withdrawn-figure ban (32/35 appears only inside §15.44's register);
and the citation notation, with one intended simplification stated in both front matters —
the plan's v15-era scoped exception is retired, so bare §N means the design everywhere,
including inside the plan.

---

## P14, THE MAXIMUM-RATE SEARCH — amended first, per AGENTS.md §5 (2026-08-05 session)

**What changed (docs only; no code).** Design §15.51 and plan §18 item 11 add a new
opt-in doctor probe, P14: on a P5-verified cross-paired rig, a ladder climb (standard
rates through 921600 plus the divisor-friendly 1M/1.5M/2M/3M family) with at most four
bounded-refinement midpoints, three byte-exact constant-airtime round-trips per direction
per rung, reporting `max_reliable_baud` plus `ceiling_kind` — never a grade (`supported`
on any completed measurement; a rig that tops out at 115200 is slow, not broken). Plain
bisection was rejected in the entry itself: achievable rates are divisor-quantized and
reliability is not guaranteed monotone in the requested rate, so midpoints are bounded,
read back requested-versus-actual, and the answer is stated as a floor over the probed
set. The macOS termios ask-ceiling (230400) is carried as `platform-refused` — a fact
about the ask, not the wire (§15.47's unmeasurable-as-data rule). Design §13's doctor
inventory now names P14 beside the P5 certificate, and its kernel-contact sentence was
corrected from "P6–P12" to "P6–P13" (a pre-existing stale enumeration, repaired in
passing). AGENTS §2's plan-§18 item count moved ten → eleven in the same commit. A new
probe id will move `probe_set` when the implementation lands — deliberately, opening a
new fingerprint era per §15.44's unequal-is-the-sound-direction rule.

**Revised the same day (upper bound).** The first draft's ladder was a fixed list capped
at 3 Mbaud, which builds the FT232R's limit into the instrument — the bench already
carries 6 Mbaud adapters, and the same vendor's H-series parts advertise 12. The ladder
now has a fixed body through the H-series divisor points (…3M, 4M, 6M, 8M, 12M) and an
**open end** above it: the rate doubles until refusal, unreliability, or the 32-bit rate
field's own cap, so the probe's list is never the ceiling and future hardware raises the
answer without a probe change. Termination is by construction (a stated structural
maximum per §15.34, reached in a handful of doublings, each one constant-airtime trial
set), and the plan's validations gained the matching case: an all-pass constructed
history must terminate at the cap with the decision function never proposing a rate
above it. `ladder-exhausted` is renamed `structural-cap` and names the instrument's
limit, never the wire's.

---

## THE v15 GENERATION — the record becomes the documents (2026-08-05 session)

**What landed.** A new normative pair: `docs/39-design-claude-fable-v15.md` +
`docs/40-implementation-plan-claude-fable-v15.md`, with the v14 pair moved to
`docs/historical/` and the two entry points (AGENTS.md §2, README's documentation index)
bumped in the same commit, plus the one gate table that names the pair as ban-statement
files (`itest/tests/meta_names.rs`, `BAN_STATEMENTS`) — which makes that table a *third*
place that names the pair by filename, corrected in AGENTS §2's wording this session
rather than left as a false "only two".

**The alignment pass ran first, by construction.** The v12 and v13 generations were both
rebased from stale bases and silently dropped rules the code still enforced; the standing
first step (this file, "The document alignment pass") is a sentence-granular diff proving
the new text is the old text plus only intended changes. v15 was produced by *starting
from the v14 bytes* (`cp`, then targeted edits), so the diff is the intended change set by
construction. The intended changes, exhaustively: (1) the review-37 `justify` annotations
restated as primary text at their sources — §11's load/add-node asymmetry (37-CFG-1),
§15.21's shipped-certificate scope (37-TOOL-3), §13's serial2 open-flags sentence
(37-SER-3), §17's eviction-bounded pre-auth pool (37-WEBS-6), plan §3's shipped sim flag
set and phase-6 item 5 (37-TOOL-6); (2) contract text the post-v14 record earned, folded
into §4 (rule-2 misreading), §6 (synchronous grant-purge, reclaim-does-not-purge,
per-chunk leg provenance), §7.2 (`prime_slave` named; the measured Darwin `TIOCPKT` byte),
§7.3 (scan-failure and failed-rotation semantics), §7.4 (listen-role bind retry), §10
(the `tap.closed` terminal lane; the pinned-write rule), §11 (`-32007`; the §15.42
barrier cross-reference), §12 (the enumerated one-source doors; the measured replug
premise), §15.44 (the committed passive/rig counterexample; the withdrawn 32/35 figures),
§16 (addendum 16.14), and §17 (the refusal-observability/lingering-close contract with
its measured numbers); (3) five new design entries, §15.46–§15.50 (instrument
self-testimony; the portable certificate; the provider seam and last-hop physics; the
zero-witness doctrine; the teardown ledger), each citing its §3 entries and artifacts;
(4) plan §3's harness doctrine extended from seven rules to sixteen; (5) plan §18, the
v15 work ledger — ten prioritized open items with validations, plus the
evaluated-and-not-scheduled list; and (6) a stated citation-notation rule in both
documents (bare §N = design, plan §N = plan; 37-WEBC-8's forty-site class). Every §3
entry's recorded declines and refutations were carried, none silently reversed; the
review-37 §3.23/§3.24 numbering follows the ledger's assignment (§3.23 = 37-TOOL-3,
§3.24 = 37-CFG-1), the review's own body text notwithstanding.

**What deliberately did not change.** Design §1–§9 architecture, every §15 entry's
recorded history (annotations stay; nothing renumbered — §16.13's immutable-artifact rule
is why §15.39 kept its number in v14 and the same rule binds here), the plan's executed
phase records, and the withdrawn figures (32/35 stay withdrawn; the unbacked raw/cooked
figure pair stays a plan-§18 repair item rather than being re-quoted).

**The pair was adversarially verified before landing, and the verifiers earned their
keep.** Four blind verifiers (sentence-granular diff of each document against its v14
base; a fact-check of §15.46–§15.50 against the notes, artifacts, and source; a
must-preserve/cross-reference sweep) confirmed no v14 rule was dropped — the v12/v13
failure mode did not recur — and every recorded decline and refutation survived. They
also found real defects in the drafts, all fixed before this entry: **two fused figures
in the sections whose subject is figure discipline** (the §15.44 extension attached the
124-leaf-path cross-commit movement to the passive/rig pair, which is 42 apart, all
rig-only — recomputed; and §15.48 called 6.30 s / 0.55 s "roughly 15×", a fusion with the
unrelated pty-depth ratio — it is ~11.5×, now "an order of magnitude"); **one superseded
claim restated as current** (the draft said six sibling probes still take the baseline
fallback and that P7 must never get the re-assert — §3.40/§3.41 had repaired P6/P7/P13
and left P8/P9/P12 measured-not-to-need-it, P12 being the one whose shapes a re-assert
would destroy); the `fionread_trust` set under-enumerated (six values, not four); a §7.2
sentence claiming doctor P1's artifact names the `0x20` byte it does not carry; the §10
`tap.closed` clause stating the unbounded lane as unconditional where the shipped fix
rides the data queue when there is room; and — the finding with the longest shadow —
**the new text violating the citation rule it had just promoted to primary text** (bare
§9/§7/§5 for AGENTS.md doctrine in the new design entries, bare §3.NN for notes entries
and bare §17/§18 for plan sections in the new plan text: 37-WEBC-8's class, reproduced by
the generation that legislated against it, now respelled throughout the new text; the
inherited v14 spellings in frozen §15 entries stay as they are). Plan §16 item 9's
review-32-era pre-auth "reserve" description gained a supersession bracket pointing at
37-WEBS-6 so the pair's two halves no longer disagree about the shipped mechanism. One
process note at the same standard: the first full-suite run of this session was piped
through `tail`, which both truncated the output and replaced cargo's exit status with
tail's — the §6 filter rule violated in one pipe — and was discarded and re-run
unfiltered: **786 passed, 0 failed, 4 ignored**, the recorded headline exactly. Gates at
landing: build, full suite, fmt, clippy (workspace and minimal-daemon), deny, doctor
`linux.jq`, and the meta-gates including `entry_point_design_and_plan_names_resolve`
against the bumped entry points.

---

## LINGERING CLOSE — the refusal notice now survives its own close (2026-07-30 session)

**What changed.** `web/src/bridge.rs` closes an oversize-refused WebSocket *gracefully*:
half-close, then read and discard what the peer still has in flight into one 8 KiB stack
buffer until its FIN, `LINGER_BUDGET` (1 MiB), or `LINGER_DEADLINE` (250 ms) — then close.
The point is a **FIN instead of an RST**: the caps leave the peer mid-send, `close(2)` with
bytes queued resets, and a reset destroys the peer's receive buffer including the `1009`
Close frame the bridge just wrote to explain itself. Measured before: a real Chromium got
the code on about two attempts in three, and the browser suite's assertion on it had to be
withdrawn as a coin toss (entry below).

**The guard, and the guard it replaced — the useful part of this entry.** The first
oracle asserted that *the client's write of the tail succeeded*, on the reasoning that the
server must have kept reading to absorb it. It was proved fail-first 5/5 on this Mac and it
is **worthless on Linux**: the proxy runs through kernel socket-buffer capacity, and Linux
loopback autotunes the send buffer into the megabytes, so the whole message fits and
`write_all` returns `Ok` whether or not anything drained. It would have passed vacuously on
the one platform CI runs — and the tell was already in this file, in the entry below: the
nightly failed at `1006` on Linux *while that client had no trouble finishing its send*.
One-box fail-first evidence is exactly what §8 says to distrust, and this is what it looks
like when the box in question is the friendly one.

The oracle now asserts the **shape of the close**: `Ws::drain_to_end` reports `Stop::Eof`
for a clean FIN and `Stop::Io(kind)` for a reset, and the test requires `Eof`. That is a
function of *"was anything unread at close"* — kernel-independent, not buffer-dependent.
Fail-first 6/6 with the drain removed (`Io(ConnectionReset)` against `Eof`), and the tail is
sized to sit above tungstenite's 128 KiB read-buffer prefetch and below the drain budget:
`WS_MAX_MESSAGE + 384 KiB` over 12 fragments trips the cap on fragment 9 and leaves ~469
KiB unread against a 1 MiB budget.

**What it costs, measured rather than asserted.** A refused connection holds its permit for
up to the deadline, so the pool turns over at `MAX_CONNECTIONS / LINGER_DEADLINE` = 512
refusals/s instead of immediately; hold time per refusal went 0.5 ms → 250 ms, and a
credentialed peer sustaining 512/s denied a legitimate console ~80% of its attempts where
before it denied none. The first version of the doc comment claimed the change "cannot make
things worse"; that was false in the availability dimension and now says so. Two things
keep it *low*: the path is reachable only past the token gate, and the same credential can
already pin all 128 permits **permanently and for free** with idle sockets, there being no
keepalive or idle timeout on an established bridge — so this is dominated by a primitive
§17 already accepts. A clean FIN also leaves an orphaned `FIN_WAIT_2` per refusal where an
RST left nothing, bounded by `tcp_max_orphans`, whose overflow behaviour is the reset this
replaced.

**Two narrowings that came out of reviewing the cost.** `Ending::Refused` now carries
whether the error was a size cap: only an oversize refusal lingers (a malformed two-byte
frame has nothing in flight and pays nothing), and only an oversize refusal closes `1009` —
the others close `1002 Protocol`, where before every protocol error was reported to the
browser as "too big", which it was not. And `shutdown()` moved *inside* the timeout: on the
TLS tier it loops until `close_notify` is written and pends forever against a peer whose
window is shut, so the docstring's "at most `LINGER_DEADLINE`" was false for the first
statement in the function. Not independently reachable today — `writer.await` hangs first
on that peer — but the bound should be true as written.

**Not a design amendment.** §17 states the principle ("a dead pane must never look live",
and must say why — review WEB-4); this is that principle's transport finally working, not a
new promise. `docs/security.md`'s cap paragraph said in as many words that "the over-cap
payload is never drained", which this makes false, and it is rewritten; the sentence
"bounds what one frame can make the server **buffer**" is kept verbatim, because reading
and discarding is not buffering and that precision is now load-bearing.

**`SO_LINGER` is not an alternative, categorically.** It waits on the *send* queue; the RST
here is caused by a non-empty *receive* queue. `SO_LINGER{1,0}` is the opposite knob — it
forces the reset. The only thing that empties a receive queue is reading it.

## macOS VALIDATION — the red lane was two harness defects, not one (2026-07-30 session)

**Scale and gates.** A whole-gate pass on the Mac (macOS 15.7.8 / Darwin 24.6.0, x86_64, load
avg 1.7 on 12 cores — §8 measured before anything was attributed to code), against `a102977`.
Final: **710 passing / 0 failing / 4 ignored**, 101 test binaries + 8 doc-test targets, with
`cargo build --workspace --locked` (including `serial-nexus-web`, which builds natively here
and needs no exclusion), `fmt`, **both** clippy gates, `cargo deny check licenses bans
sources`, `expectations/macos.jq`, all ten meta-gates, and `p8_web_ui` + `p8_web_history`
under `SNX_WEB_UI=required` all green — plus **all four `serial_hardware.rs` rig tests** on
the physical FT232R crossover, auto-detected by `crossover_ports()`'s macOS arm. The first
committed macOS doctor artifact, `docs/doctor/macos-24.6.0-2026-07-30-tier3.json`, carries
probe set `01b257ece8c48470`, the same fingerprint as the Linux reports.

**The CI failure: `p12_web_ws_bounds`, and the product was never wrong.** Both over-cap tests
failed on `ws.closed()` in CI; on the local Mac only one did, which is the tell that made the
rest of the diagnosis follow — a deterministic platform difference does not fail 2/2 on one
box and 1/2 on another. The mechanism is `docs/macos.md` delta 6: the refusal fires at frame-
header parse so the payload is never drained, `close(2)` with unread bytes queued is an RST,
and Darwin's `setsockopt(SO_RCVTIMEO)` — which the harness re-arms before every read —
returns `EINVAL` on a socket carrying a pending reset, so the pending `ECONNRESET` reached a
predicate spelled `matches!(read, Ok(0))` instead of being consumed a frame lower as it is on
Linux. Three separable test defects, all in one helper: the Close frame the server *does*
send was drained and dropped by `response_arrives` on the line above; `fill` collapsed
deadline / EOF / error into one `bool`, discarding a one-shot socket error that cannot be
asked for twice; and the probe covered only a graceful FIN. Fixed by giving `fill` a `Stop`
reason, latching the Close frame at the single commit point in `recv_frame`, and accepting
Close ∨ EOF ∨ a terminal `ErrorKind` while still refusing a mere deadline — so "silent" and
"closed" stay distinguishable, which is the helper's entire purpose.

Two assertions were also reordered ahead of the closure check, because the strongest and most
OS-independent evidence in the file — the daemon's own configuration afterwards — sat *behind*
the oracle that misjudged, and never ran on a failing run.

**Verification (§9).** Diagnosed from an instrumented run, then verified by three independent
skeptics with different lenses, none of which had read the diagnosis, on a tree that did not
move (the empirical one worked in a throwaway worktree). All three converged; the
`SO_RCVTIMEO`/`EINVAL` mechanism came from the verification, not the diagnosis, and a C
repro pinned it: `setsockopt` → `EINVAL(22)`, `read` → `ECONNRESET(54)`, `read` → `0`. The
product lens measured the caps boundary-exact (`cap-1` and `cap` served, `cap+1` refused),
246 over-cap trials with 0 daemon mutations, no fd leak over 200 refused sessions, and `401`
for an unauthenticated peer — **PRODUCT_DEFECT = no**. Fail-first: the fixed tests were run
15× on the Mac (15/15 green, against a race that used to flip), then re-proved sensitive by
deleting both `WebSocketConfig` calls — 5/5 runs red, 0 passed / 2 failed, and failing on the
**cap** assertion with the same messages the review-37 ledger recorded, not on the closure
one. The oracle was weakened nowhere.

**A second defect CI structurally cannot see.** `cargo clippy --workspace --all-targets --
-D warnings` does not pass on macOS: `p12_pty_setup.rs`'s `two_consoles_that_both_come_up` is
called only from a `#[cfg(target_os = "linux")]` test, so it is dead code off Linux. Its
sibling `fd_flags` carries the gate; this one was missed. The `check` job that runs clippy is
`ubuntu-latest`, and the `macos` job runs no lint at all — so this gate had never once
executed on the platform that fails it. Now gated, with the reason in the doc comment.

**A third defect, found by tripping over it: the meta-gate walkers descend into nested
checkouts.** The adversarial verification above ran one of its skeptics in a `git worktree`
under `.claude/worktrees/`, and the next whole-suite run came back with four meta-gates red
— `unsafe_is_confined_to_serial_nexus_sys` naming `.claude/worktrees/…/sys/src/lib.rs`,
`no_asyncfd_is_used_anywhere_in_the_workspace` naming the detector file itself, and both
`meta_names.rs` scans. Every finding was true of the *copy* and vacuous about the repository
under test. Three walkers were affected (`walk_text`, `walk_rs`, `crate_dirs`), each carrying
a skip list of bare directory *names*.

Fixed by skipping any directory that **is a checkout** — `dir.join(".git").exists()` — rather
than by adding `.claude` to the name lists. The distinction is load-bearing in both
directions and both were proved: a worktree leaves a `.git` **file** and a clone a `.git`
**directory**, so a name-keyed skip catches one and misses the other (fail-first: with the
predicate neutered the scratch plant surfaces `wt/thing.rs` *and* `clone/thing.rs`, and with
it only `nested/thing.rs`); and `.claude` must **not** be skipped wholesale, because
`.claude/settings.json` is tracked project configuration and §15.41's privacy rule is
tree-wide — proved by planting a retired name at `.claude/probe-scan.md` and watching the
gate convict it. End to end: with a real `git worktree add .claude/worktrees/…` present, all
ten meta-gates stay green. The plants live in the scratch trees the two gates already build,
which is §3's rule applied to a *skip* — a skip is a matcher too, and gets planted in every
spelling it claims to cover.

**Two process gaps, both self-inflicted, both closed.** (1) The macOS lane still ran plain
`cargo test --workspace`; the 2026-07-28 block below is the record of one failure hiding
three for six consecutive pushes, and the lesson never reached `.github/workflows/ci.yml`.
It has `--no-fail-fast` now — deliberately only on that lane, the one nobody runs before
pushing. (2) That block and the LEG-2 one cited rules as living in `AGENTS.md` §2 and §5 that
were not in `AGENTS.md` at all — the section numbers shifted under the rename track and the
prose kept the old ones. Both rules are now actually written there (§6 for `--no-fail-fast`,
§9 for "assert the promise, never a proxy for it — in space or in time"), and the two
citations here repointed. A rule that exists only in a citation is not a rule.

**Two observability gaps in the WS bridge — filed in the commit above, fixed in the one
after it.** Both surfaced from the product lens of the verification; both are
**cross-platform**, neither was an enforcement failure, and neither was macOS's doing —
which is why they were recorded as their own item and fixed on their own commit rather
than riding along with a platform fix:

- **A cap violation is logged nowhere.** `web/src/bridge.rs`'s browser-read arm collapses
  `Some(Err(_))` — a tungstenite `Capacity` error among them — into the same `break false`
  as a clean browser disconnect, and `bridge()` then returns `Ok(())`, so even
  `server.rs`'s `tracing::debug!("connection from {peer} ended")` says nothing that
  distinguishes them. An operator gets **zero signal** that a peer tripped a security cap
  §17 and `docs/security.md` both advertise. A bound nobody can observe being hit is a
  bound nobody can tell is working, which is the same argument 37-WEBS-4 made about it
  being untested.
- **The refusal's Close frame carries no status code.** The cap path exits through the
  writer's `ws_sink.close()`, i.e. tungstenite's `close(None)`, so the wire bytes are
  `0x88 0x00` — an empty payload, which a browser surfaces as `CloseEvent.code` **1005**
  (no status received). RFC 6455 defines **1009** (Message Too Big) for exactly this, and
  the bridge already has the machinery: the daemon-gone path sends a coded `CloseFrame`
  with a reason precisely so a page can say *why* (review WEB-4). The console cannot
  currently tell "the server refused my frame" from "the socket dropped".

**The fix.** `bridge()`'s ending was a `bool` — "was it the daemon?" — which is why a
refusal could not be named: it collapsed the two endings the server *causes* into the one
it merely observes. It is now an `Ending` enum, and the refusal arm both `warn!`s and
closes with `CloseCode::Size`. `app.js` stops discarding the `CloseEvent`: a coded close
now reaches the badge, so a refused frame, a dead daemon (the WEB-4 `1001`, which the page
had also been throwing away) and a pulled cable no longer print the same eight words.
`docs/security.md`'s cap paragraph states the code and the log, so both are promises now
rather than incidents of the implementation.

Guarded, and each guard proved fail-first against its own fix: deleting the `warn!` fails
both `p12_web_ws_bounds` tests on *"said nothing about it"*, and downgrading the close code
to `1008` fails them on *"ended with close code 1008, not 1009"*. The code assertion is
conditional on a Close frame arriving — the refusal's RST can destroy it — but that is not
slack in practice: with the expected code deliberately set wrong it bit **10 runs out of
10** on this Mac. A new device-free browser spec sends past the *message* cap rather than
the frame cap on purpose, because a browser may fragment one `send` into any number of
legal frames and a payload sized against the frame cap can arrive without tripping
anything (it did, first try, and hung the spec).

**That spec tried to assert the 1009 in Chromium, and could not — recorded because the
correction is the useful part.** It shipped as `expect(code).toBe(1009)`, passed per-push
and on the Mac, and failed the first nightly at **1006** ("abnormal closure — no Close
frame received"). The refusal lands partway through a message the browser is still
sending, so the server stops reading with bytes queued and its `close(2)` leaves as an RST
that can destroy the Close frame it just wrote — the same physics as the `closed()` oracle
two sections up, met from the other side. Measured on the *fixed* server: 1009, 1006, 1009
over three runs.

The obvious repair is also wrong, and was measured rather than reasoned about: tolerate
1006, reject 1005, on the theory that an uncoded close is the pre-fix shape. The
**reverted** server reports **1006** too — an RST destroys an empty Close frame exactly as
willingly as a coded one — so fixed and unfixed are indistinguishable from a browser on
any given run, and the assertion would have been a coin toss dressed as a guard. The spec
now asserts only what a browser can settle deterministically: the probe socket ends, and
the console's own bridge beside it does not, the cap being per-connection. The 1009 guard
stays where its timing is kind, in `p12_web_ws_bounds.rs`.

**Consequence stated plainly at the time: the coded close was best-effort on the wire, not
a guarantee.** An operator's console would usually be told why it was disconnected and
sometimes would not. **Superseded — the lingering close was made in the following session;
see the entry above.** The paragraph stays because its reasoning is why that entry exists.

What is deliberately *not* asserted: `disconnectMessage`'s text. The console's own socket
never sends anything near a cap and the page does not expose it, so the string is read
rather than run; the code it maps from is asserted at both ends. Said out loud here
because an unasserted behaviour that nobody writes down is the same shape as the defect
37-WEBS-4 filed in the first place.

**One caveat for anyone re-running this.** `cargo test` does not emit the plain
`target/debug/serial-nexus-*` artifacts the harness boots, so after touching product source
you must `cargo build --workspace` again or the suite silently exercises the *previous*
binary. This bit once during this very session — a suite run against a stale
`serial-nexus-web` left over from the fail-first deletion reported the cap tests red on a
tree whose source was already restored. The CI workflow comments say this; it is easy to do
anyway.

## REVIEW-37 REMEDIATION — all 82 findings dispositioned (2026-07-29 session)

**The finding-by-finding ledger is `docs/38-review-37-remediation-ledger.md`** — read it
before re-filing anything from review 37, and read the review's own §6 (cleared candidates,
including the two hypotheses its finders killed before filing) before filing anything new.
Eighty are fixed and, where behavioural, guarded; two are **justify** and were already
recorded here as §3.23 and §3.24, with the design annotations they called for made this
session. Nothing was silently declined.

**Scale and gates.** Suite **642 → 723 passing / 0 failing / 4 ignored** (+81), across
**102 → 111** test targets. `cargo build`, `cargo fmt`, `cargo clippy` (workspace **and**
minimal-daemon), `cargo deny check licenses bans sources`, the macOS cross-check,
`serial-nexus-doctor --json | jq -e -f expectations/linux.jq` and the headless-Chromium suite
(forced with `SNX_WEB_UI=required`, so the green is a run and not a skip) are all green. One
wire-surface change: a new application error **`-32007` (edge inbox full)** — additive, per
§10, and the registry's own gate refused it until `docs/rpc/README.md` carried the row.

**The shape of the fixes, which matters more than the list.** Two of the five clusters are
the same relocation twice, one level apart, and are the ones to re-read before touching
either area:

- **Provenance travels with the data; it cannot be re-derived later.** The leg's
  receiving-side purge identified outage-era chunks by reading the disconnect epoch *after*
  `rx.recv()` returned. A peer whose final data frames and FIN are readable together — one
  that sends its commands and disconnects in one breath — is enqueued, EOF'd and
  epoch-bumped inside a single poll of the supervise task, so every stale chunk snapshotted
  the **post**-bump epoch, both purge checks compared equal, and the dead connection's whole
  backlog fired into the device the §6 purge exists to protect. Deterministic on the
  current-thread runtime, not a race. The queue now carries `Inbound { epoch, bytes }`
  stamped at enqueue, on the pump's read half, which is the only moment the provenance is
  known. See §3.25 for what that cost: this instance of the purge no longer uses
  `boundary::drain_to_quiescence`.
- **The same collapse one level down.** `attach_edge` handed the consumer its hostward
  receiver with `inbox.try_send(hrx).is_ok()`, so a *full* inbox and a *closed* one were one
  fact. `Closed` is a property of the node (no pump — a faulted pty, a deferred
  `faces = "host"` channel) and is correctly reported, never refused; `Full` is a property of
  the moment, and reporting it as the former sent an operator to inspect a healthy node while
  a **configured** edge stayed hostward-dead for good. `Full` now attaches nothing at all and
  `connect` answers `-32007`.
- **A promise driven from three places and not the fourth.** §6/§15.23's "a `held` origin
  reclaims the moment it frees" had drivers in the codec, exec and map targetward pumps and
  none for a pty, while held-priority in `acquire` denied every *other* origin on the free
  lock — so any steal-release, unlock or lease expiry left the endpoint free-but-untakeable
  until a manual `lock`, another steal, or edge surgery. The reclaim gained a non-parking
  sibling for boundary origins that own their own lifecycle loop (§3.26).
- **Terminal events and deadlines moved off the congested path.** `tap.closed` was delivered
  with a discarded `try_send` on the very queue whose fullness is why the client needs it; a
  parked verb's deadline lived inside a `select!` arm body that a blocked `write_all` stopped
  polling, so one non-reading client stalled every other connection's arbitration on that
  endpoint with the lock reported free. The terminal event now has its own per-connection
  lane drained *ahead* of the data arm, and the connection's writes no longer block the loop
  that owns the deadline.
- **Gates that could not fail.** Three tree-scanning meta-gates proved their matchers and
  never their walkers, and the self-exclusion matched a bare file name; nothing in the suite
  ever sent the daemon a signal, so deleting its shutdown arms would have passed everything.
  All four now assert execution — the `meta_names.rs` treatment, which existed as the correct
  shape the whole time.

**Four things the remediation found that the review did not**, recorded because a review's
own errors are as load-bearing as its findings: `37-DOC-3`'s second location is wrong (the
doc comment is `itest/src/lib.rs:774`, not `serial_hardware.rs`) and its third site is
partially refuted (no such notes §3 entry exists; a *different* error at line 2231 was fixed
instead); a fourth `37-DOC-3` site — `serial_hardware.rs`'s four skip messages — told a Linux
operator to attach `cu.usbserial` adapters, which does nothing there; `docs/serial-nexus-doctor.md`
carried a fifth stale claim falsified by the very artifact `37-DOC-1` is about (`brk` "stays 0
at every tier on every kernel", against a committed `brk=2`); and `37-WEBC-8`'s dangling
`§11.8`/`§11.9` refs were tree-wide rather than confined to three web modules — 37 further
sites across `daemon/`, `itest/`, `ctl/` and `docs/rpc/` cited plan sections in the notation
README reserves for the design, and now read `plan §11.8`.

**One toolchain note worth more than it looks.** `cargo test -p serial-nexus-itest --test
<name>` does **not** rebuild the daemon binary the harness spawns — it runs whatever sits in
`target/debug/`. Every in-place revert taken for fail-first proof this session had to be
followed by `cargo build -p serial-nexus-daemon-bin` before the itest meant anything. This is
review 32's "a guard that drives the fix cannot fail against the code that lacked it", one
layer down in the toolchain, and it is invisible: the proof simply passes and reports nothing.

**Tier-3 hardware validation, and the certificate for this tree.** All four
`itest/tests/serial_hardware.rs` tests *ran* rather than self-skipped against the cross-wired
FTDI FT232R pair (`/dev/ttyUSB0` `BH00L4KU` ↔ `/dev/ttyUSB1` `BH00LL8O`): byte-exact both
directions at 115200 and at the custom rate 250000, the `send` verb reaching the far port,
`TIOCEXCL` exclusivity, the map node both directions over the physical wire, and the serial
signal verbs on a real UART with the break surfacing at the peer RX as a one-byte `0x00`.
The whole suite with the rig attached and `SNX_WEB_UI=required`: 111 targets, 0 failures.
The §15.21 certificate was taken *after* that run rather than before it, because the tooling
was briefly unable to invoke the doctor with `--port`; nothing failed, so nothing needed
attributing, but the documented order was inverted and is recorded as such rather than
tidied away. The certificate is committed as
`docs/doctor/linux-7.0-2026-07-29-tier3-2.json` — binary `2e5874bbe090` (the remediation
commit), probe set `01b257ece8c48470`, equal to every *pre-P13* artifact in that directory and
therefore diff-comparable with all of them — **21 · 0 · 0 · 1**, the one skip being P12,
inert on Linux by design. P5 reports the pair crossed in *both* directions with
`rate_ladder=true deliberate_mismatch_observed=true` on both ports, and it passes
`expectations/linux.jq`. P11's `brk=3`/`frame=5` on `/dev/ttyUSB1` are the certification's
own break and deliberate-mismatch phases surfacing in the driver counters — which is the
direct evidence behind this session's correction to `docs/serial-nexus-doctor.md`, whose
claim that `brk` stays 0 at every tier on every kernel was already false of the *committed*
artifact before it was false of this one.

---

## DEPENDENCY UPDATE — everything to latest stable, and the Tier-3 rig that checked it (2026-07-29 session)

`cargo update` plus five deliberate **major** bumps that sit outside the manifests' semver ranges:
`sha2` and `sha1` 0.10 → 0.11 (the RustCrypto `digest` 0.11 generation), `tokio-tungstenite`
0.26 → 0.30, `getrandom` 0.2 → 0.4, and `rcgen` 0.13 → 0.14. Two crates were deliberately **not**
taken: `libc` 1.0.0-alpha.4 and `rustls` 0.24.0-dev.1 are pre-releases, and a lab tool's TLS stack
is the last place to run one. `@playwright/test` was already at its latest (1.62.0).

**Three call sites needed porting, all in the web console, none behavioural.** `getrandom`'s free
function became `fill` (two sites — the session token and the asset credential); `rcgen`'s
`CertifiedKey` became generic over `SigningKey` and its `key_pair` field is now `signing_key`
(two sites in the self-signed generator). The RustCrypto and tungstenite majors needed no source
change at all, which is the whole point of using their trait surfaces rather than their internals.
The new graph pulls a visible tail of transitive crates — `asn1-rs`, `der-parser`, `nom`,
`num-bigint`, `rand` 0.10, `chacha20`, `hybrid-array` replacing `generic-array` — and
`cargo deny check licenses bans sources` stays green over all of it, which is the gate that
matters here: a major bump is exactly how a copyleft transitive dependency would arrive (§13).

**The generated-cert path was exercised, not assumed.** `rcgen` is the one bump that touches
cryptographic material, and its only in-tree consumer is the `--tls` first-run generator.
`tls.rs`'s `generation_happens_only_when_neither_path_exists_and_the_key_is_0600` covers it
through `build_config`, which means rustls itself accepts the pair — but the suite never completes
a *handshake* against a generated cert, so this session did one by hand: `--tls` with no cert paths,
then `curl --cacert` over `localhost`, giving `verify=0`, `302` with the token and `401` without.
Worth recording because the first attempt "failed": connecting by `127.0.0.1` is refused for want of
an IP SAN, which is **deliberate and pre-existing** — `generate_self_signed` skips IP hosts and says
so, and rcgen 0.13 and 0.14 behave identically there. A dependency bump makes every such refusal
look like a regression; the check is to read the code that predates it.

**Validated on the Tier-3 rig, which is on this box now.** `docs/doctor/README.md` recorded that the
7.0 side was passive-only "because the dev box has no adapter attached any more". It does again —
the same two FT232Rs (`BH00L4KU`, `BH00LL8O`) cross-wired — so this update was checked on real
silicon rather than on pty stand-ins: the doctor certifies **21 supported · 0 degraded · 0
unsupported · 1 skipped** (P12 inert on Linux by design), and `serial_hardware.rs` passes 4/4 —
byte-exact both directions at a nonstandard 250000 baud, the signal verbs, and the map node over
the physical crossover. That closes the asymmetry the doctor README named: there is now a Tier-3
artifact for 7.0, not only for 6.18.

## THE RENAME TRACK — one name for the family, and the two documents it had to correct (2026-07-29 session)

Plan §17 executed in full: design §15.40's family rename and §15.41's context scrub. What is worth
carrying forward is not the mechanics — a rename is a rename — but the four things that made it not
one.

**The tree arrived red, and the gates were right.** `HEAD` had two failing meta-gates before a line
was touched: the design revision moved the v13 pair into `docs/historical/` and left README's index
and AGENTS.md pointing at it. `entry_point_doc_links_resolve` and
`entry_point_design_and_plan_names_resolve` are exactly the gates §9's monotonic versioning demands,
and they fired on exactly the failure they were written for — the fourth generation running.
AGENTS.md §1 additionally said the normative pair was "named in §2" while §2 named nothing, so the
gate's non-vacuity floor had nothing to count. Both are fixed, and the second gate no longer *spells*
the current design: it discovers a generation doc under `docs/` instead, because a literal there was
a third hand-maintained copy of the name and went stale on the same bump the gate exists to catch.

**Two find-and-replace artifacts sat in the normative design.** §15.41's earlier scrub had rewritten
`downstream` → `out-of-tree` inside §3's terminology note — the very use §15.41 names as a legitimate
survivor of the data-flow sense — and had mangled "all reported `Active`" into "all out-of-tree
repositoryrted `Active`" by rewriting the bare substring `repo` wherever it appeared, including
inside an ordinary English word. The historical generations carry the earlier stage of the same
corruption, which is how the chain was identified: a first pass had already broken that word, and the
v14 scrub then rewrote the breakage. Both are corrected; the lesson is the one this session then had
to learn again for itself.

**A blanket textual rename conflates registers, and the compiler will not tell you.** The same token
is a *directory*, a *Cargo package*, a *lib crate*, a *binary*, a *Cargo feature*, or prose, and the
right replacement differs in each. Two concrete instances. First, an ordered list of replacements is
not a rename: the rule converting the old compound daemon name produced a string the *next* rule
matched again, doubling every name that was already correct in the v14 documents — fixed by a
single-pass matcher whose every rule carries a "not already converted" lookbehind. Second,
`unsafe_is_confined_to_serial_nexus_sys` went red because the prefix it tests is a *filesystem* path
and the directory is `sys/`, while the blanket pass had rewritten it as a crate name. Plan §17.1's
"with directories unchanged" clause was amended for the same reason — see §3.22.

**The gate found its own hole on its first run.** `retired_names_appear_only_where_history_lives`
and `consumer_context_terms_appear_only_where_the_ban_is_stated` are both plant-the-violation gates
per review 32 item 7. The context gate's first matcher required a word boundary *after* the token,
which silently exempted every inflected form — the plural, the `-ory`/`-ories` endings, the
past-participle spelling — and it knew only the spaced version of one term while a hyphenated one sat
in `--help` output at the time. Folding hyphens to spaces and dropping the trailing boundary surfaced
eleven further sites. A scanning gate's *matcher* needs the same adversarial treatment its walker
gets: both this session's gates plant a violation and prove the detector fires, but only the walker
half of that was thought about first.

**One residual lead, recorded rather than closed.** On one full-suite run (of five this session)
the browser gate's `adding a console through the editor makes bytes flow end to end` spec failed
after 20.4 s on a box at load 0.29 — and reported the wrong thing: its `finally` ran
`remove-node --cascade`, that threw, and JavaScript replaced the in-flight exception with the
teardown's, so the failure named the one step that was working. The elapsed time says the real
failure was the 15 s poll waiting for the editor-built log node to receive the token, but that is
inference, not evidence, because the evidence was destroyed. The masking is fixed — the cleanup now
reports and lets the body's error through — and the spec has since passed 3/3 in isolation and 1/1
in a full run. **Not diagnosed, not called fixed:** the next occurrence will name its own cause,
which is the only thing this session can honestly claim about it (AGENTS.md §9). The general lesson
is worth more than the instance: a `finally` that can throw is a diagnosis destroyed, and this suite
now has two shared-fixture ordering scars in the same file.

**The version went 0.2.0 → 0.3.0.** A minor bump, not a patch: §15.40 changes the crate names
an out-of-tree consumer builds against, and §15.26 calls that surface semver'd. Nothing here is
published, so no real pin breaks — but the number is what a consumer reads before the changelog,
and understating a rename of the thing they import would be the wrong first impression to give.
The twelve workspace manifests, `README`'s maturity line, `packaging/README.md`, the `info`
example in `docs/rpc/observation.md` and both normative documents' status lines moved together;
the captured `docs/doctor/` artifacts kept `0.2.0`, because that is the binary that produced them.

**The design shipped two sections numbered 15.39.** The session-boundary ADR landed first
(`85699d6`), and the v14 revision then drafted the rename and the context scrub as 15.39 and
15.40 — so every "design §15.39" citation in the tree was ambiguous, across shipped probe code,
both `expectations/*.jq` files, `docs/macos.md` and the doctor reference. The collision was
resolved *against* the newer pair — rename is now **§15.40**, context scrub **§15.41** — for one
reason that admits no argument: the committed `docs/doctor/` artifacts cite `§15.39` for the
session-boundary edge, and §16.13 forbids editing captured tool output, so renumbering that entry
would leave immutable evidence pointing at the wrong section forever. Everything written against
the drafts (AGENTS.md, plan §17, this file, the new gates) moved with it. Worth knowing for next
time: a new §15 entry's number is only free if nothing already shipped with it.

**And the gates then fired on the notes describing them**, twice — this entry and plan §17 both
started out spelling the banned tokens in order to explain them. That is not a false positive: a
document that reproduces the vocabulary is a document that reintroduces it, which is why the rule
statements carry exact-count allowances and everything else describes the tokens instead.

---

## THE 6.18 RE-RUN — a HEAD binary on a Tier-3 rig, and the first diff the repo can check (2026-07-29 report)

The 2026-07-28 entry below closed with a recommendation: re-run the doctor on 6.18 with a HEAD
binary, both adapters cross-wired, and `--json`, "because any report it produces will carry its own
commit, probe-set fingerprint and date, so committing it records provenance rather than asserting
it." **The owner ran it — with the HEAD binary and the cross-wired pair, though not `--json`**
(`85699d66c5a5`, generated 2026-07-29T00:15:16Z, Tier-3 rig,
**21 supported · 0 degraded · 0 unsupported · 1 skipped**). The numbers and the gap-by-gap
disposition live in `docs/serial-nexus-doctor.md`; this entry records what the exercise settled, what it
cost in corrections, and the three things about it a future session will get wrong.

**Verdict: no daemon change, again.** Not a shipped constant, comment, test premise or design claim
is falsified. P6, P7 and P8 came back byte-identical to a HEAD 7.0 baseline on every measured field
— and P8's wall clock, 10–25 ms apart in the previous diff, now agrees within 1 ms once the same
binary is on both sides, which confirms that spread was noise rather than kernel. (It is still not
evidence: `elapsed_ms` is 64 passes × a fixed pause, and two 7.0 runs of one binary differ by 1 ms.)
P1's and P2's booleans
matched, P9's timed floor came in *tighter* on 6.18 on every row (by 9–107 µs, ≤ 1.1 % of the
requested interval; the 2026-07-27 pair's "8–17 µs" does not describe this one), and P10's cross-kernel
difference **dissolved**: the two kernels swapped shapes, so what the 2026-07-28 entry called "6.18
the mid-fill, 7.0 the late flip" was a per-run artifact, exactly as the probe's own text warned.

**The artifacts are in the tree now, and that is the structural change.** `docs/doctor/` holds the
6.18 Markdown beside three passive HEAD 7.0 JSON runs. Until this session, "P6 and P7 read
field-for-field identical" was asserted in three documents with nothing in the repository to check
it against — DOCR-3's shape one level up, named in the 2026-07-28 entry and left open there for want
of a report worth committing. The 7.0 side is passive because **the crossover pair physically moved
to the 6.18 box**, which is how that box became Tier 3; that costs the kernel diff nothing (P1, P2,
P6–P10 need no hardware) and costs the *port* diff everything (P3/P4/P5/P11 skip on 7.0, so every
port-facing 6.18 number is a first measurement, not a comparison). It is three runs rather than one
because P9 and P10 vary run to run on one box, and one sample of a varying quantity is
indistinguishable from a cross-kernel difference — the precise mistake the previous P10 reading came
within one run of making.

**The fingerprint earned itself on its first outing, in both directions.** The recorded 2026-07-27
7.0 baseline is `a2d3b96`, which predates both the Build block and P12 — so it emits no `probe_set`
at all, and by the rule the field exists to enforce it is *not* comparable with the new 6.18 run.
The three passive runs were captured to supply a lawful counterpart. In the other direction, the
report-text corrections made this session (below) left the fingerprint at `01b257ece8c48470`,
verified by rebuild — which is the design working: `(id, question)` moves when the *question* moves,
and correcting a `Consequence` paragraph does not invalidate an archived comparison.

**Three claims this session had to correct, and each was a note that had drifted from its code.**
(1) **`brk = 0` is structural, not a Tier-1 artifact.** Three documents tied it to the dangling rig;
the Tier-3 report reads `brk = 0` on both ports of a certified pair. `p5_certify_port` computes
`break_ok` as `set_break(true).is_ok() && set_break(false).is_ok()` — local ioctl acceptance — and
`p5_certify_pair` transmits a rate ladder and a bulk mismatch pattern and no break at all — and the
two phases that do hold both ports open at once, discovery and the pair certificate, are precisely
the two that assert none, so nothing the doctor itself does can raise `brk` at any tier. The Tier-1 verdict string
carried the same false implicature ("and no break was received by anything" — a tier-scoped sentence
for a binary-scoped fact) and has lost that clause. Break *reception* on 6.18 is unobserved and
belongs to the suite gap, where `p12_serial_exclusivity::a_break_straddled_by_a_replace_leaves_the_line_transmitting`
lives — and note that **attaching the rig is not sufficient**: on Linux `crossover_ports()` reads
`SNX_CROSSOVER_A`/`_B` and has no auto-detect arm (that is `#[cfg(target_os = "macos")]`), so the
rig-gated tests still self-skip on the upgraded box until those are exported.
(2) **The probe-set fingerprint digests `(id, question)`, not `(id, title, question)`**, as
`AGENTS.md` and `docs/serial-nexus-doctor.md` both claimed. The exclusion is load-bearing and the docs had
inverted its reason: P3's *title* embeds the device path and P3 is emitted once per `--port`, so
folding it in would make a two-port 6.18 run and a zero-port 7.0 run of one binary report themselves
incomparable — over exactly the diff the field underwrites.
(3) **P10's operator-facing band was measured too narrow.** It printed "(7.0 measured 11776–13824
first-pass, 13824–15360 total)" and a doc comment blamed the 13824 case on "several doctors running
concurrently"; run 2 of three, alone on an idle box, produced 13824, and run 1 produced a hostward
first pass of 15360 — above the stated band. Both texts now report what has been measured.

**One hedge the run does *not* discharge, stated so it is not read as closed.** P4 came back
`by_id_tree: present, count: 2, sysfs_only: 0`. `enumerate_ports` merges its sysfs pass with
`or_insert`, so a `sysfs_usb_devices()` that returned nothing would have printed a byte-identical
block: the `<sys>/class/tty` listing **ran but is not witnessed**. What *is* witnessed is the sysfs
ancestor walk, since `discover_adapters` derives both printed identities through `sysfs_lookup`. The
no-udev arm RES-2 was written for remains unexercised on either kernel — and it needs no hardware to
close, `--dev-root` rerooting `/dev` *and* `/sys`, so a fixture tree fires it on the 6.18 box
directly.

**Housekeeping that the probe count had outgrown.** P12 landed in `85699d6` and eleven documents,
doc comments and a guard still enumerated the set as P1–P11 or "P6..P11" — including
`probes::tests::the_kernel_diff_probes_never_report_unsupported`, whose own doc names
`expectations/linux.jq` and `meta_gates` as the gates it stands in for while omitting the one probe
both of those files had just started gating. P12 is in the loop now (it is `skipped` on Linux and
carries measurements where it runs, so both assertions apply unchanged).

**What is still open on 6.18, so the next session inherits two items and not four.** (1)
**`cargo test --workspace` has never run there** — and attaching the rig does not by itself unblock
the `crossover_ports()`-gated tests, which need `SNX_CROSSOVER_A`/`_B` exported on Linux. That one
gap now also carries break reception and the far-side modem-line and parity items a Tier-3 *checklist*
covers but a Tier-3 *certificate* does not. (2) `--json` was not captured, so the `jq -e -f
expectations/linux.jq` re-gate is satisfied clause by clause on inspection — `.build.*` included this
time — and still never *executed* there. What Markdown cannot witness is not any clause's content but
the JSON encoding, which the three 7.0 runs of the identical commit discharge; the honest form is
"content proven on 6.18, encoding proven on 7.0", and one `--json` capture collapses the distinction.

---

## THE FIRST WHOLE-SUITE macOS RUN — four failures, three guards, one real defect (2026-07-28 session)

Run on macOS 15.7.8 / Darwin 24.6.0, x86_64, with two FTDI adapters cross-wired as a null
modem. **Result: 623 passed / 0 failed / 4 ignored**, `fmt`, both clippy gates, `cargo deny`,
`p0_license_gate`, `p8_web_ui`, `p8_web_history` and `expectations/macos.jq` all green, and
**all four `serial_hardware.rs` rig tests passing** — plus `p12_serial_exclusivity`'s
rig-gated `a_break_straddled_by_a_replace_leaves_the_line_transmitting`, the invariant-15
guard a pts structurally cannot run.

**Why this had never happened.** CI's macOS lane runs `cargo test --workspace`, and *cargo
test fail-fasts per crate*. One `serial-nexus-daemon` unit test had been failing there, so the lane
stopped before the integration harness every time: red on six consecutive pushes, always
reported as the same single failure, with **three more sitting behind it that nobody had
seen**. `--no-fail-fast` surfaced all four at once. That is now a rule in AGENTS §6 — when
you are validating a *platform* rather than a change, pass `--no-fail-fast`, because the
per-crate stop makes "one known failure" and "four unknown ones" look identical.

**Three of the four were guards asserting a Linux-specific proxy** for a property the daemon
satisfies portably. Each was diagnosed, then adversarially verified by two independent
skeptics with different lenses (§9's bar), and in each case the portable form turned out
**stricter on Linux** than what it replaced — which is the tell that the real property was
found rather than the guard weakened:

1. `pty.rs`'s §7.2 flush test asserted `!POLLIN` on the master. Darwin sets `POLLIN`
   *unconditionally* on a slave-less master and answers `read` with `Ok(0)`; Linux reports
   the hangup and answers `EIO`. The product was measured correct — `read_and_poll` latches
   `saw_session` on `Ok(n) if n >= 1`, so the `Ok(0)` takes the `closed` arm and sets neither
   `did` nor `saw_session` (no spin: 1.53% of a core against a 1.43% baseline; the lock
   survived ~6000 such passes) — and `read_and_poll`'s own close-block comment already
   disclaims "POLLIN goes quiet after a hangup" as a §13 kernel-dependent claim. The
   assertion now reads the master, which is the predicate the product actually uses.
2/3. The two LEG-2 guards required `accepted_targetward` to *settle nonzero*. That silently
   assumed the AF_UNIX buffer is at least one wire frame wide: macOS's is **8192 bytes**
   (`net.local.stream.sendspace`, measured — exactly 8192 absorbed with no reader) against
   Linux's ~208 KiB, and the counter is credited per **completed frame**, so a 60 006-byte
   frame never completes there and 0 is the correct reading. The ledger still closed to the
   byte on macOS (`0 + 1 140 019 + 60 001 = 1 200 020 = sent`). The predicate is now "frozen
   **while bytes are still owed**" — the portable form `p6_fragmentation` already used, and
   stricter besides: the old one also accepted a plateau at `== sent`, a fully drained peer,
   which is not the parked state at all. The constants stay at 20 × 60 000; shrinking them
   would have hidden the assumption rather than removed it.

**A fourth, pre-existing, unmasked by the fix.** Once those two stopped burning 30-second
timeouts the suite's timing changed and `WirePeer::dial` began failing `ECONNREFUSED` — in
the full suite, never in isolation, which is the shape that reads as flakiness and gets
chased as one (§8). It is not a flake: the callers wait on `sock.exists()`, and **`bind(2)`
creates the socket file one syscall before `listen(2)` stops `connect(2)` refusing**. The
dial now retries to a bounded deadline. Generalised into an AGENTS §9 rule beside the other
two, because all three are one mistake — a proxy standing in for the thing you need, in
space or in time.

**The fourth failure was a real macOS defect. It is now fixed — design §15.39.** After the
test-only work above was committed and CI went green, the owner signed off on the product
change, and it landed as `serial_nexus_sys::SessionLatch`: a `kqueue` knote registered
`EVFILT_READ | EV_CLEAR` on the pty master (Darwin), inert elsewhere, folded into
`read_and_poll`'s existing `saw_session` rather than added as a third disjunct. Suite
623 → **627** (the four new `serial-nexus-sys` tests), and `p9_pty_collapse`'s third test now runs
**unskipped on both platforms**. Proved fail-first on this box: **0/8** collapsed
termios-only sessions release with the latch's one assignment commented out, **8/8** with
it. Idle cost measured by A/B in the same binary: **1.62% → 1.75%** of a core, one
non-blocking `kevent` per 5 ms pass, against the 74%-of-a-core spin this area's other rules
exist to prevent. Four things a future editor must not undo, each of them measured rather
than argued: the latch must not set `did` (an edge is not data, and marking the pass
productive stops the idle backoff); `watch` must swallow the edge its own registration
posts (registering on an already-hung-up master posts one immediately, and every pty node
starts there because setup primes the slave); the last-close block must `discard` after
running, because `apply_baseline` and `flush_hostward_queue` open the slave themselves and
forge an identical edge — delete that and the handler re-fires on its own footsteps, which
`collapsed_client_sessions_still_release_the_write_lock` catches, in a *different* test
than the one the latch fixes; and invariant 1 is untouched, its ban being on `AsyncFd`/epoll
as a **readiness** source while readiness stays `poll(2)` alone. `serial-nexus-doctor` gained
**P12** so the two mechanisms are diffable across kernels — gated tightly in `macos.jq`,
where the edge is the only mechanism, and presence-only in `linux.jq`, where it is inert by
design. One asymmetry is recorded rather than levelled: the bare open→close that Linux
leaves deliberately uncovered *does* post an edge on Darwin, so macOS is here the stricter
platform.

The paragraph below is what the tree said before that landed, kept because the *diagnosis*
is the durable part and because it is the record of a refutation worth remembering.

A collapsed *termios-only*
pty session — open, `tcsetattr`, close, inside one 5 ms poll gap — leaks its write lock.
The first diagnosis called it unfixable ("nothing else reveals it", from an exhaustive sweep
of level-triggered observables: poll revents, `FIONREAD`, `TIOCOUTQ`, `TIOCGPGRP`,
`TIOCMGET`, `TIOCGWINSZ`, pts inode timestamps, all byte-identical to no session at all).
**Both skeptics refuted that, independently and by measurement**: level state cannot carry
an *edge*, and a kqueue `EVFILT_READ | EV_CLEAR` knote on the master does — a faithful model
of `read_and_poll`'s presence/last-close block went 0/8 → 8/8 with an edge latch, 3/3
reproducible, zero fires across idle controls and 200 daemon-shaped poll+read passes. The
conclusion had followed from the shape of the probe, and a measurement outranks it. Severity
against the shipped daemon: 20/20 real `stty -f` sessions leak, past 30 s, with another
origin's `send` failing `-32003`. **Deliberately left open**: the fix needs new `unsafe` in
`serial-nexus-sys` (invariant 4), an ADR separating an *edge latch* from invariant 1's *readiness*
ban, a doctor probe for the mechanism, and handling the daemon's own momentary slave opens,
which forge an identical edge — a §9 design decision, not a diagnosis-phase patch. Its guard
**skips rather than being retired**, gated on `serial-nexus-doctor` P7 and `cfg(not(linux))`: on the
kernel of record a `false` answer means the daemon is leaking locks, and `linux.jq` admits P7
`degraded`, so a P7-keyed skip there would retire the guard at exactly the wrong moment (§5's
"a gate that can skip silently is a gate CI passes over a hole"). Full detail in
AGENTS §7's macOS arm and `docs/macos.md` delta 3.

**Two things the rig run answered that the docs had marked "needs a Mac" for four
generations.** Doctor **P1 is `degraded`, and the mechanism is now measured, not inferred**:
a client `tcsetattr` *does* produce a packet, but Darwin's leading byte is `0x20`
(`TIOCPKT_DOSTOP`), not `0x40` (`TIOCPKT_IOCTL`), so `read_and_poll`'s IOCTL arm never
matches and termios reconciliation runs entirely off the `RECONCILE_INTERVAL` backstop —
the §7.2 fallback working exactly as designed, costing only latency. And
`discarded_at_last_close` is **structurally always 0** on macOS, because the kernel destroys
the pts's undelivered hostward queue at last close before the daemon can count it: §7.2's
guarantee holds there for free, and the counter that names the discard has nothing to name.

**One doctor over-claim fixed, the third in P5's prose.** With the rig named on `--port`, P5
discovered the pair correctly in both directions and then reported both genuine FTDI adapters
`skipped (not a UART)`, under a consequence line promising "the certificate populates on real
adapters". The UART predicate is `p5_is_uart` = `read_icounts(fd).is_ok()` = **`TIOCGICOUNT`,
which is Linux-only**, so off Linux it answers "no" for every port however real, and the
advice was to replace hardware that was already correct. Both strings are now `#[cfg]`-split
and say what was measured (§15.17). AGENTS §2's 6.18 entry records the other two over-claims;
the guard `a_clean_rig_is_supported_certified_or_skipped` gained a portable clause — an
uncertified rig must never borrow the certified arm's opening sentence — since that sentence
is what a tiered checklist run reads to decide it may start (§15.21).

---

## THE 6.18 KERNEL DIFF — taken at last, and it changed nothing (2026-07-28 session)

P6–P11 were added on 2026-07-26/27 "so the owner can run `serial-nexus-doctor --json` on 6.18 and diff it
against the 7.0 baseline". **The owner ran it on 2026-07-27 and the diff is now taken.** The scope
statement, the numbers and the residual gaps live in `docs/serial-nexus-doctor.md`'s 6.18 section — this
entry records what the exercise cost, what it settled, and the three things about it a future session
will get wrong.

**Verdict: no code change.** Not one shipped constant, comment, test premise or design claim is
falsified by the 6.18 numbers. P6 and P7 came back **byte-identical** to a same-day HEAD 7.0 run; P8
matched on every semantic field; P1/P2/P3's booleans matched; P9's 1/5/10 ms floor agreed within
8–17 µs; P10 differed by exactly the flip-scheduling case the probe's own text teaches you to
recognise. Every probe `supported` on both.

**The trap, stated so nobody walks into it.** P6's consequence string says the `saw_session` latch
"is not what holds the anti-spin argument up on this kernel", and now says it of *both* kernels. That
reads like a license to delete the latch and it is not one, for three independent reasons. (1)
`pty.rs` already disclaims dependence on POLLIN going quiet — the anti-spin argument was deliberately
built kernel-independent, which is why the 7.0-only evidence was never load-bearing in the first
place. (2) The latch's live justification is **invariant 16 rule (3)**: a collapsed-session write-lock
leak, measured five of five against a saturated endpoint, about a drain that ends early when the
endpoint refuses a payload. That is a correctness property no probe measures on any kernel, and it is
guarded by `p12_pty_setup.rs`, not by the doctor. (3) P6's `handler_reset_readable_bytes: 1` reads
**identically on 6.18**, which confirms the last-close drain load-bearing on the production kernel
rather than removable. The run's only new positive is that P7's widened-latch premise holds there —
`latch_covers_termios_only_session: true` — retiring the risk that probe was written to name.

**Three named hedges the run positively answers.** `serial-nexus-sys`'s TIOCOUTQ-on-a-pty comment called
itself "exactly the quiet 6.18-vs-7.0 difference this section exists to surface": 6.18 answers
`pending_output_bytes: 0` in both directions, same as 7.0. P7's own "if 6.18 leaves nothing there, the
widened latch silently fails" is retired on the probe's terms. And P2's `hup_after_close: true` is the
precondition `p12_sim_idle_cpu` reads out of `--json`, so that guard would *run* on the production
kernel rather than self-skipping there — if the suite were ever run there, which it never has been.

**What the diff cost, the instrumentation gap it exposed, and the fix.** Establishing that the 6.18
report came from a **`fe1c52c`-vintage binary rather than HEAD** took forensics on a *section title*:
its P4 block is the pre-`RES-2` "by-id resolution ground truth" shape. Nothing in the artifact said
which build produced it — the header was a bare `serial-nexus-doctor v0.2.0`, the JSON carried only
`tool`/`version`/`generated_unix_ms`, and `to_markdown` dropped even that timestamp, so a *committed*
6.18 Markdown would have dated itself only by its commit. `expectations/linux.jq` exists precisely to
prove the artifact is "diffable field by field" and had no clause that could see it. The vintage turned
out to be benign — `git diff fe1c52c a2d3b96 -- doctor/src/probes.rs` touches only
`p4_resolver`, `environment()` and tests, so P1–P3 and P6–P11 are validly diffable — but that was luck,
established after the fact.

Both renderers now open with a **Build** block: `commit`, `probe set`, `generated` (UTC). **Two
identifiers, because they answer different questions**, and the second is the load-bearing one. A
commit hash says *which tree* and then requires the reader to work out what changed between two of
them — which is exactly the manual step this session spent. The **probe-set fingerprint** answers what
a diff actually needs, *are these two artifacts comparable*, in one glance and with no repository
access. It digests each probe's `(id, title, question)` and deliberately **not** its observations or
verdict: those are the measurements the diff exists to compare, two healthy boxes differ in them by
design, and folding them in would report every real cross-kernel pair as incomparable — the field
being wrong in the direction nobody would notice. FNV-1a over a *length-delimited* encoding, because
concatenating raw fields lets a title's tail slide into the next question with no change in the digest.

**The pre-commit review caught two blocking defects in this very work, and both are worth recording
because both were self-defeating.** (1) The first fingerprint digested `(id, title, question)` — and
P3's *title* embeds the device path while P3 is emitted once per `--port`. The 6.18 box has one
adapter and the 7.0 box has two, so the very cross-kernel diff the Build block exists to underwrite
would have carried unequal fingerprints and printed "the numbers below are not comparable field by
field" over a valid comparison. It now digests the **deduplicated, sorted set of `(id, question)`**,
and `main`'s no-`--port` placeholders were rewritten to carry `probes::P3_QUESTION` /
`probes::P5_QUESTION` verbatim, because a paraphrased placeholder made a passive run and a rig run of
the *same binary* disagree. Measured after the fix: passive, either single port, and both ports all
report `68519e193e4c84d8`. (2) The new `.build.*` jq clauses **falsified two claims this same change
set was making** — that the 2026-07-27 6.18 report satisfies every clause of `linux.jq` and that
`linux.jq` is byte-identical across the two vintages. A `fe1c52c` binary emits no `build` object, so
that artifact would now fail the gate. Both statements are corrected in place rather than deleted:
the inference held against the file *as it stood*, and saying so is what keeps the re-run
recommendation honest.

The same review found two defects in the *Tier* work committed alongside. `RigFacts` counted
certificate **attempts**, but `p5_certify_pair` returns early before the mismatch block whenever a
port will not reopen — so P11 could still blame an item that never put a byte on the wire, which is
the entire defect the thread was added to remove. And a discovered pair that failed to reopen left no
observation, no failure and no fact, so `tier()` fell to 1 and P5 printed "**Tier 1** — a dangling
converter" directly above its own observation line reading `paired with …`. Both are fixed by
splitting the one count into two facts — `discovered_pairs` (the topology, which is what a *tier*
is) and `mismatch_pairs` (traffic that actually reached the wire, gated on a new
`Certificate::mismatch_transmitted`) — and by making the silent `continue` record an uncertified-pair
failure, which is §15.21's degrade case. P5 now has a fourth sentence for Tier-3 wiring whose
certificate did not complete, and P11's guidance is three-valued rather than two, since a Tier-2
operator was being told to reason about "a dangling port" that was not theirs.

Three implementation notes worth keeping. (1) **Dependency-free by requirement** — the doctor's
dependency list is part of the licensing gate, so `build.rs` shells out to `git` with `std` alone (no
`vergen`, no `git2`) and the UTC rendering is Hinnant's `civil_from_days` rather than a date crate;
both are ~25 lines with unit tests, and the timestamps were cross-checked against `date -u` on the
epoch, a 400-divisible leap day, an ordinary leap day and the day after a non-leap century. (2)
**Every failure path degrades to `unknown`, never to a build failure or a guess** — verified by
building against a `git` that exits non-zero, which still produced a usable artifact with the
fingerprint intact. A stamp frozen at first-compile HEAD would be *confidently wrong* provenance, so
`build.rs` emits `rerun-if-changed` on `.git/HEAD`, the resolved ref and `packed-refs`, and a
worktree git refuses to read is reported `-unknown-worktree` rather than assumed clean. (3) The jq
clause asserts **presence, not value**: `commit` may legitimately read `unknown` off a tarball build,
and reddening a healthy box over that is the false negative P4's clause already refuses to make.
Guards: `report::tests::{the_utc_stamp_agrees_with_date_on_the_cases_that_break_naive_conversions,
the_probe_set_fingerprint_moves_on_a_probe_rewrite_and_not_on_a_measurement,
both_renderers_carry_the_build_identity_and_the_timestamp}`, and the gate itself was proved able to
fail (`jq 'del(.build)'` and an emptied fingerprint are both rejected). Joined 2026-08-05 by the
cell-set digest and its guards (notes §3.51): `report::tests::{the_shared_encoder_is_byte_stable,
the_field_set_moves_on_a_new_observation_key_and_not_on_a_measurement,
the_two_field_set_paths_agree_on_a_real_report}`, the four in
`itest/tests/meta_doctor_artifacts.rs`, and
`expectation_gates::{the_field_set_clause_rejects_a_report_that_cannot_say_which_cells_it_carries,
both_expectation_files_carry_the_same_field_set_clause}` — because **equality of this fingerprint
never licensed a field-by-field diff**, and until then nothing said so.

**Two report-text defects the run surfaced, both Tier-1 over-claims, both now fixed.** They were in
the operator-facing report the README makes the first attachment on every bug report, and a Tier-1
dangling converter is §13's *baseline* rig, so neither was exotic — both were on the page the owner
pasted. (a) P11's consequence text said unconditionally that "a nonzero `frame` here is usually P5's
deliberate baud-mismatch item", but `p5_certify_pair` runs only over discovered *pairs*, so on a
one-port box that mechanism provably never transmitted — and the 6.18 report said it anyway over a
real `frame=4`. (b) `p5_verdict` emitted "Rig discovered and **certified**; every tiered checklist run
starts from this certificate (§15.21)" for any UART rig, including one where `pairs` was empty and
neither `integrity` failure site could fire, telling an operator a Tier-2/3 run may start from a
Tier-1 certificate and never naming the tier.

**The fix is one fact, threaded rather than re-derived.** `p5_rig` now returns `RigFacts` beside its
`Probe` — the pairs that actually *reached* `p5_certify_pair` (not the pairs discovery found: a pair
whose ports will not reopen `continue`s past the mismatch) plus the loopback count, with a `tier()`
of 3/2/1. P5's verdict names that tier and states what it did **not** run; P11 takes the same value
and only offers the mismatch as an explanation when a pair was certified, otherwise reporting that the
item did not transmit and how to tell history from crosstalk. The counter is taken where the
certification happens because the tempting inference is wrong: **`named >= 2` is necessary but not
sufficient** — two *dangling* ports are two named ports and no pair. Guards
`probes::tests::the_certificate_names_its_tier_and_what_that_tier_did_not_run` (all three tiers name
themselves, all three lines differ) and
`probes::tests::p11_blames_the_baud_mismatch_only_when_a_pair_was_certified`. **Fail-first proved**:
with both conditionals planted back, exactly those two tests fail and the other 21 pass — which is
also the measurement of why the defects shipped, the existing folds being blind to the distinction.

**Residual gaps, so "6.18 is confirmed" is not read wider than it is.** *(As of 2026-07-28. Two of
the four — the binary vintage and the rig tier — were closed by the 2026-07-29 re-run at the top of
this file, which also relocated `brk = 0` to a structural cause. Read that entry, not this
paragraph, for the current state.)* The binary vintage leaves
HEAD's P4/`environment()` rewrite unmeasured there. The box is Tier 1 — one dangling adapter — so
everything a *pair* certifies is unmeasured, no break was ever observed (`brk = 0`), and every
`crossover_ports()`-gated test self-skips. Only Markdown was captured, so the `jq -e -f
expectations/linux.jq` re-gate is satisfied clause by clause on inspection but has never been
*executed*. And the largest one: **`cargo test --workspace` has never run on 6.18** — CI is
`ubuntu-latest` + `macos-latest`, so the production kernel's evidence base is eleven probes and zero
executed tests. One visit with a HEAD binary, both adapters cross-wired, `--json`, and a suite run
closes all four.

**Neither artifact is in the tree.** *(Closed 2026-07-29 — `docs/doctor/` now holds the 6.18 report
and three same-fingerprint 7.0 baselines, exactly as this paragraph proposed.)* Both reports live in a session scratchpad. `docs/serial-nexus-doctor.md`
says "the report itself is the record" and no such record exists for either kernel, which makes
"P6/P7 read field-for-field identical" a claim in three documents with nothing in-repo to check it
against — DOCR-3's shape one level up. Committing both under `docs/doctor/` and pointing the prose at
them is the fix; it was not done here because the 6.18 Markdown is a chat paste rather than a captured
file, and a re-run that also closes the vintage and `--json` gaps would produce a better artifact to
commit than this one. **That re-run is now worth more than it was**: any report it produces will carry
its own commit, probe-set fingerprint and date, so committing it records provenance rather than
asserting it.

---

## REVIEW-32 REMEDIATION — all 80 unique findings dispositioned (2026-07-27 session)

**The finding-by-finding ledger is `docs/historical/33-review-32-remediation-ledger.md`** — read that before
re-filing anything from review 32, and read the review's own §6a/§6b (10 refuted, 2 already-known)
before filing anything new. One finding is deliberately narrowed rather than closed (`WEBS-1`, the
web token's outbound cookie exposure: reduced in code to `Path=/ws` plus a separate asset credential,
with the residual stated in `docs/security.md` and design §15.29/§17); the other 79 are fixed and, where
behavioural, guarded.

**Scale and gates.** Suite **485 → 630 passing / 0 failing / 4 ignored** (+145). `cargo fmt`,
`cargo clippy` (workspace **and** minimal-daemon), `cargo deny check licenses bans sources`, the macOS
cross-check, `serial-nexus-doctor --json | jq -e -f expectations/linux.jq` and the headless-Chromium suite
(19 specs per push at that landing, 2 `@slow` nightly — the per-push count has since grown; the plan's Status table is the home of the current figure (notes §3.75)) are all green. This review's guards are the **`p12_*`** family in
`itest/tests/`, plus `p6_fragmentation.rs` (§15.24 named a leg guard by number and the leg family
is `p6_*`) and new cases inside the modules that changed.

**The shape of the fixes, which matters more than the list.** Each cluster's remedy is a *relocation* —
the review's three-for-three thesis is that defects cluster where a rule lives in prose instead of in a
type, a cap, a parser or a helper:

- **One source for the resolver's two directions.** Capture walked sysfs while resolution, the
  duplicate-serial guard and `enumerate_ports` read only `/dev/serial/by-id`. Both directions now read
  `<sys_root>/class/tty` with by-id as a *fast path*, ambiguity is counted over **devices** (two clones
  sharing a serial collide on one udev-generated link name, so counting links could never fire for the
  hazard §12/§15.25 name), and a symlinked `/dev` path is canonicalized before capture. The unit fixture
  that made the old guard look tested — it invented two by-id names udev cannot produce for identical
  devices — was replaced with the single-link shape udev really emits.
- **Exclusivity belongs to the port.** `ExclusivePort` returns the `TIOCEXCL` claim on `Drop` and
  `release_port` is the one ordered discard, so the four exit paths stop being four things to remember.
  Reachable before this from §7.1's *documented* modem-line assertions on a pty-backed device: `set_dtr`
  ENOTTYs, the port is dropped still exclusive, and the flag clears only at the tty's last close — which
  a held master never reaches — so the device was un-openable by every unprivileged process on the
  machine, permanently, surviving teardown and daemon exit.
- **One place drains a connection's notifications.** `serve_connection` is now a single four-lane
  `select!` with dispatch as an optional arm, instead of an inner two-arm loop that left the `notes` and
  `tap_rx` arms unpolled for the whole of a parked wait (measured: 11.2 MB dropped in one 6 s
  `lock --wait`; a 2.001 s terminal blackout with 3.3 MB dropped per contended browser keystroke, since
  §17 mandates one daemon connection per browser). `send`'s deadline now bounds the delivery too — it
  used to hang forever *holding the exclusive lock* — and a pty reader parked on a full targetward
  channel no longer freezes its own presence, last-close, termios reset and detach-release.
- **Charge every non-delivery exit, through one verb.** The four §5 accounting holes are closed, and
  `SIMP-2` — their structural form — with them: every targetward charge site goes through
  `LossCounter`, with a `#[must_use]` residual so an uncharged exit is a compiler warning rather than an
  audit item. `runtime::route_channel_data` does the same for the hostward per-channel block that was a
  verbatim clone between `codec.rs` and `exec.rs`.
- **Gates that can fail.** `p0_license_gate` asserts cargo-deny's `error[banned]` diagnostic rather than
  its exit code (it passed vacuously on any `cargo metadata` failure — *proven* by deleting the ban
  entry); `p11_replace_atomicity`'s exclusivity guard asserts the EBUSY *reason* rather than bare
  `pass == false` (~40% detection on an idle box before, deterministic now); the `RefCell` meta-gate's
  exemption is a repo-relative path with a planted-impostor self-proof; and `entry_point_doc_links_resolve`
  replaces the third manual patch of README's stale design-pair links with a gate.

**Two process notes.** (1) **Fail-first proof was obtained where the shim was affordable** and the
guard's doc comment says so where it was reasoned instead — `ITEST-1` via an `LD_PRELOAD` shim
swallowing `TIOCEXCL` (6/6 fail, 4 at the exact verdict the old assertion called green), `TESTR-2`
end-to-end under the offline-runner shape the finding names, `TESTR-7` twice. Do not read an unproved
guard as an equal one. (2) **A shared-fixture Playwright spec cost a full investigation.** The new
`HIST-1` spec failed in the suite and passed in isolation, because `graph-editor.spec.mjs` attaches a
log node to the shared echo console and never removed it — so the later spec's `discarded_unattached`
oracle could never move and read exactly like "the device never spoke". The editor spec now restores the
graph in a `finally`, *and* the history spec asserts its own precondition, which is the durable half.

---

## THE AUDIT OF THE REMEDIATION — six skeptics on a frozen tree (2026-07-27 session)

The remediation above was put through §15.34's own treatment one level up: six independent agents got
review 32 and a **frozen** worktree, never the implementers' reports, and were told to refute the claim
that each finding was fixed. Verdicts, per-finding outcomes and the narrative are in
`docs/historical/33-review-32-remediation-ledger.md` ("The audit round"); **this entry is the engineering half** —
the mechanisms, the measurements, and why the code that answers each one looks the way it does. Read it
before "simplifying" anything named here: several of these are shapes a remediation already got wrong
once, in the obvious direction, with a green suite behind it.

### The lesson: a guard that drives the fix cannot fail against the code that lacked it

The audit's highest-yield finding was not a defect at all — it judged **nineteen** of the remediation's
new guards *unable to fail*, which for a project whose method is "every phase ends with an adversarial
audit" is worse than a missing test, because it is counted as coverage. One pattern produced most of
them, and it is a pattern a remediation is structurally prone to: the fix extracts a function
(`apply_spawn`, `attach_edge`, `with_scoped_port`, `route_channel_data`), and the guard drives *that
function*. Against the tree without the fix there is no such function — so the test does not fail, it
fails to *compile*, which is a state no fail-first run ever reaches and no reviewer ever sees.

Two more variants worth recognizing, because neither looks like the first: a guard whose bound is
generous enough to admit the unfixed behaviour (`HIST-2`'s trimmer spec streamed ~320 kB and asserted
`100_000 < chars < 1_000_000`, and an unfixed client renders ~320 008 — inside the window), and a guard
that removes the very condition the defect needs (SERX-2's unit test installed the successor on a
*second* pts, which is not the shared tty the whole mechanism turns on — and a pts has no `break_ctl`
anyway, so nothing it asserted could ever have been observable).

**The rule, applied throughout the fixes below: a regression guard names the entry point an operator
reaches, and its bound is a function of the constant the fix introduced, not of the volume the test
happens to produce.** The extracted helper still gets a unit test for its own arithmetic — as well,
never instead. Where a defect is only observable on hardware or under a resource limit, that is where
the guard goes, even at the cost of a self-skip (§5's skip-is-a-valid-verdict discipline): a rig-gated
test that really fails is worth more than a portable one that cannot.

### Accounting: two counters answering one question, and a hole answering none

**(a) `LEGD-2`'s over-correction, and why `FanOut` now has two fields.** The filed defect was
`delivered_hostward` crediting bytes no sink accepted, and the fix subtracted the drops:
`add_delivered(n.saturating_sub(fan.dropped_full))`. That is wrong in the other direction, and the
review's own verifier had said so in advance. `dropped_full` accumulates **per sink**, so a `[Ok, Full]`
fan-out reports `dropped_full == n` and the subtraction credits **zero** delivered for a chunk a live
consumer received in full — the exact inverse of the complaint. With *k* full sinks it saturates at zero
for several chunks' worth. `runtime::FanOut` therefore carries `live` (is anything still attached? — the
question the unattached charge turns on, and a consumer whose buffer is full is very much attached) and
`delivered` (bytes at least one sink actually took), decided inside `fan_out` from the sends themselves.
`route_channel_data` credits `fan.delivered` unmodified and never subtracts one counter from another.
The consequence an operator has to know, now stated in the field's doc: **with several consumers a chunk
is legitimately both delivered and discarded** — the two counters measure different consumers — so
`delivered + discarded == streamed` is a *single-sink* property, which is what `p12_leg_accounting` wires
and says it wires.

**(b) The demux emit-then-error over-count** (found by the audit; not in review 32). `WIRE-1`'s new
`note_demux_error(chunk.len(), …)` charged the whole input chunk to `multiplexed.discarded_hostward`
whenever `Codec::demux` returned `Err` — but `hostward_demux` drains its `events` vec unconditionally
*after* the match, so every event the same call emitted before failing is still routed and credited to
its channel's `delivered_hostward`. The realistic shape does exactly that: a non-resyncing framer decodes
the good frames out of a 64 KiB chunk and refuses on the corrupt tail, and nothing in `serial-nexus-codec-api` says a
transform must emit nothing before erroring — `demux` invokes `emit` once per decoded event *and*
separately returns a `Result`. So `state` reported ~64 KiB of hostward loss on a chunk that was ~100%
delivered, with the same payload counted as delivered *and* as lost — irreconcilable numbers, which §5
treats as worse than coarse ones (a counter that double-counts cannot be reconciled at all). The charge is
now `chunk_len.saturating_sub(salvaged)`, `salvaged` being the `data` payload of the events that call
emitted, summed by the caller. **The residual is stated rather than hidden:** framing overhead of the
salvaged frames is still charged as loss, because the trait cannot report consumption — erring toward
reporting loss, not toward hiding it — and a call that emits more than it was handed (legal: a transform
flushing frames buffered from earlier chunks) saturates at zero rather than wrapping. Guard
`codec::tests::a_partial_decode_charges_only_the_bytes_the_refusal_lost`, fail-first: the old charge
reported 12 lost against 8 delivered on a 12-byte chunk.

**(c) `Handoff` is `#[must_use]` too.** `SIMP-2` made the *blocking* helper's residual a compiler
concern (`TargetwardLoss`, with `charge` as its only consuming method), and left the non-blocking twin
without one: `try_forward_targetward(origin, chunk, &lost);` in statement position compiled clean and
silently dropped a targetward chunk with no counter — SIMP-2's own failure scenario, on the one path
whose helper could not warn. `runtime::Handoff` now carries the attribute, and `Handoff::Full(Chunk)`
hands the undelivered chunk back so the caller has something it must do with it. Both pty call sites
already matched, so nothing was broken at the time; the point is that a future third site cannot forget,
because with CI's `-D warnings` an ignored handoff is a hard error rather than an audit item.

**(d) `TapFeed::skipped`, and why the first TAP-2 guard could not see the gap.** The mirror-hop half of
TAP-2 shipped and the skip-site half did not, though the doc comment and a test comment both pointed at
"`serial.rs`'s `TapFeed::skipped` call" — a method that did not exist. The reachable shape is the one an
operator actually uses: a lone serial node watched only through `ctl tap`, `replay_ring = 0`, no graph
consumer at all. `reader_thread` takes the `hostward.is_empty() && !tap_wanted` read-and-discard arm,
never builds a `Chunk`, and so never reaches `TapFeed::mirror`; the audit measured `discarded_unattached`
growing by 196 608 across a 3 s dark window while the next `tap.open` returned the previous frontier, the
same `epoch`, `feed_dropped: 0` and a first chunk with `gap_before: 0`. The guard could not see it and
said so without noticing: it *deliberately* attaches a `log` sink, precisely to force the chunk to be
built. **A guard that steers around the one shape that still reproduces reads exactly like coverage.**
`TapFeed::skipped(n)` now charges the same `feed_dropped` atomic from the skip site, and the two guards
run one body with a `with_sink` switch so everything a client observes is asserted identical on both
paths (`p12_tap_replay::the_ringless_window_is_announced_with_no_graph_consumer_at_all`, fail-first
against a tree with the method present and the call site removed). The two charges stay independent and
both are right: the bytes are unattached (no graph consumer took them) **and** a hole in the feed (no hub
saw them) — the ring is a spy outside the graph, so its accounting never substitutes for the graph's
(invariant 9/10). `wanted()` and `skipped()` are a pair; a producer that takes the allocation-skipping
answer owes the charge.

### The serial line: break is tty state, and the reopen window is a second D2

**`SERX-2` — the fix converted a bounded outage into an unbounded one.** Making the deferred restore
generation-scoped is right and stays: a node that has been torn down, replaced or reconnected must not
drive a *successor's* line, so `RestoreGuard::drop` asks `with_scoped_port` and declines when the
generation moved. But declining removed the only `TIOCCBRK` in the tree. A `send-break` straddled by
`load --replace` therefore left the **tty** asserted with nobody left to clear it. Reproduced on the
bench crossover rig, control against treatment: control `send-break usb0 --ms 2000` with no replace →
the peer sees `\x00` at t=0.50 and `CTRL-OK\n` at t=2.51, the break clearing on schedule; treatment
`--ms 5000` with a `load --replace` at t≈1.0 → the verb returns `-32602 node was removed while
signalling`, the successor reports `status: active, open: true`, two `send`s report `sent 14 byte(s)` /
`sent 6 byte(s)`, `driver_counters.tx` climbs to 79 — and the peer receives nothing for the next 19 s.
An earlier run was still silent 30 s later. Recovery needed a full `teardown` (which destroys the tty)
or another `send-break`; `load --replace` does not clear it.

What the first fix missed is one sentence: **break is tty state exactly as `TIOCEXCL` is.** It outlives
the fd that asserted it whenever the tty outlives that fd, and under `--replace` the tty never reaches
last close. So the remedy is invariant 15 extended rather than a timer restored — `ExclusivePort` owns
*every* tty-level assertion this node made, and `release_claims` (plural, and renamed for it) gives them
all back in order: `TIOCCBRK` first so the line is transmitting normally, then `TIOCEXCL` so anyone else
may open. **Why there and not in the successor's `open_port`:** the departing node cleans up after
itself, which keeps the rule one place and keeps ownership honest — a successor-side clear would have to
run on every open, would fire on ports nobody asserted anything on, and would depend on the next arrival
noticing. DTR and RTS are deliberately *not* released: driving them on the way out is a reset pulse on
every auto-reset board (§7.1), the unrequested edge this whole area exists to prevent, and unlike break
they self-heal (the successor's own `open(2)` re-raises DTR on a real adapter, and `open_port` reapplies
the configured `[node.modem]` levels — measured). Guard
`p12_serial_exclusivity::a_break_straddled_by_a_replace_leaves_the_line_transmitting`, rig-gated on
purpose: a pts has no `break_ctl` and the sim null modem is a byte-copy loop that models no line
condition, so a `serial_pair()` version would be green against the stuck-break binary.

**The reconnect window is D2 surviving in a second place.** The audit read `supervise`'s reconnect arm
and found the same ordering defect §15.38's D2 fixed in `teardown`: `open_port` takes `TIOCEXCL`, then
`purge_on_reconnect` awaits (up to `DRAIN_ROUNDS` yields whenever there *is* an outage backlog), and only
then is the port stored — so for the width of the purge the daemon holds a device `release_port` cannot
see, because `sh.port` is still `None`. A `load --replace` landing there releases nothing, the aborted
supervisor future holds the only `Rc<ExclusivePort>` and is not dropped until the `LocalSet` regains the
thread, and the *successor's* `open(2)` EBUSYs against the daemon's own claim — a `faulted` flap during
which an accepted `send` is purged rather than written, which is what `p11_replace_atomicity` exists to
prevent. The window is sub-millisecond and the audit did not land it live; the mechanism is certain from
the code path, so it was fixed rather than left.

The arm is now the extracted `serial::reopen`, whose rule is **publish the port on the node before the
first `.await`** — the general form of D2, and the second site to need it. `adopt_port` stores it and
carries the reconnect's single generation bump (moved off `set_active` for this), then the purge awaits,
then a generation re-check declines to arm a reader on a port the node let go. Two state-reporting
consequences were checked rather than assumed and are accepted:

* The node reports `waiting` with `open: true` for the width of the purge. That is *more* truthful than
  the alternative — the daemon really does hold the device exclusively, and a third party's `open(2)`
  already says so.
* A signal verb is accepted in that window, because `signal_handle` keys off `sh.port`. The port is real
  and this node owns it, and because the bump happens at `adopt_port` rather than at the transition to
  Active, a signal issued in the window stays valid into Active instead of being orphaned there.

The generation re-check is insurance, not necessity: an aborted `spawn_local` future is never polled
again today, so a teardown inside the window means `reopen` simply never resumes. It is written so that
stays an asserted property rather than a fact about tokio's scheduler.

### The resolver's second door, and the diagnostic that pointed away from it

**`RES-1` ran on one of two paths.** The ambiguity guard was fixed to count *devices* rather than by-id
link names, and guarded end to end — on the **capture** path only. The other direction,
`Resolver::resolve_usb_identity`, is the path `load`, daemon startup from the state file, and `add-node
device = "usb:…"` all take, and it never called the guard at all. Reproduced against the fixed daemon on
a two-clone fixture tree: two nodes carrying
`usb:0403:6001:DUP:00` both accepted with `kind: usb` and no warning, both reporting
`resolved_path: …/dev/ttyUSB0`, the second adapter unreachable by any node. This is not hypothetical for
an upgrader — `dump` wrote the ambiguous `usb:` string, so restarting the fixed daemon on an existing
state file reproduces it with no operator action. Three doors reach that function carrying an ambiguous
identity and only one of them is history: a persisted config, a hand-typed identity, and the door no
amount of history fixes — an identity captured while only one clone was plugged in, whose twin appears
later.

**The disposition is decline, not guess**, and it costs a fast path. `find_usb` runs the device listing
*before* the by-id readlinks and returns `None` when two present devices answer, so a node whose identity
is ambiguous stays `waiting` rather than driving a coin-flip board; `resolve_usb_identity` returns the
identity with a `warning` naming every device that answers plus the by-path fix, which is what `add-node`
echoes. The reason the ordering had to invert is `RES-1` itself: a by-id *hit* says nothing about
uniqueness, because udev publishes exactly one link per colliding name. Measured cost on this dev box
(real `/sys`, ~110 tty entries): **0.93 ms per resolution against 0.041 ms** for the readlink alone, paid
once per open and once per 1 Hz faulted-and-wait recheck per waiting node — under a tenth of a percent of
a core, and `stat`s rather than DTR, since nothing here opens a device (§15.35). Declining is also the
only arm available at the lower level: `resolve_current_path` returns a bare `Option<PathBuf>`, so
"bind and warn" has nowhere to put the warning and would be indistinguishable, at every open, from
binding the right device.

**`RES-2`'s third surface was `serial-nexus-doctor`, and it is the one AGENTS §3 tells operators to attach to
every bug report.** The daemon learned to resolve identities in a tree with `/sys` and no by-id links;
the diagnostic still gated on `dev/serial/by-id.is_dir()` at both the environment check and P4, so in
exactly that environment it reported `/dev/serial/by-id: "absent (no USB-serial adapter)"` and
`P4 skipped: "no /dev/serial/by-id tree"` — the daemon works and the diagnostic points away from it. P4
is now "device identity resolution", asks about the `<sys>/class/tty` listing with by-id as a fast path
over it, gates on **devices**, and emits `by_id_tree` (present/absent), `sysfs_only` and
`other_candidates` beside `count`. It stays `supported` in a no-udev environment **by design** —
`linux.jq` admits only `supported|skipped` for P4, and a probe that reddens a box the daemon is fine on
is a false negative carried to 6.18. The environmental difference is carried by the by-id *environment
check* instead, which gained a third arm: `degraded`, naming how many devices are visible another way,
never `unsupported` (§13's diff-between-kernels rule).

### The web console's pre-auth pool: a reserve you cannot classify into is not a reserve

`WEB-5`'s first remediation split the 128-slot connection pool and took a 32-permit pre-auth semaphore
**in the accept loop**, before a byte is read. The cookie cannot be known there, so a full pre-auth pool
refused every new connection *including one carrying a valid session cookie* — the denial got four times
cheaper, not closed. Measured against the shipped binary with a bootstrap cookie in hand: 16 silent peers
→ `200 OK`, 31 → `200 OK`, **32 → connection reset**, 40 → reset, on `/app.js` and `/ws` alike; releasing
the peers restored service. Shortening `HEAD_TIMEOUT` from 15 s to 5 s made the other axis worse too —
the sustain rate an attacker needs fell from 128/15 s ≈ 8.5 conn/s to 32/5 s ≈ 6.4 conn/s. And because
the newest connection is the one dropped, the operator — always the newest, with no reconnect in
`app.js`, so a reload is the only recovery — is the structural victim.

The shipped shape inverts both halves. `MAX_CONNECTIONS = 128` bounds connections that have **passed**
the token gate; the permit is taken at the gate and the overflow is answered `503`, not queued.
`MAX_PRE_AUTH_CONNECTIONS = 32` bounds the population before it, **by eviction**: every accepted
connection joins unconditionally, and going over the cap cancels the *oldest* member's task and closes
its socket. A silent peer can no longer deny anyone; it can only be the thing that gets evicted. After
the change, 300 held peers and a sustained ~18 000 conn/s reconnect flood both leave authenticated
requests served.

**The generalizable half, worth more than the constants:** *a reserve you cannot classify into at
admission time is not a reserve.* Any bound taken before the credential is readable is taken against the
operator as well as the attacker, so the only question a pre-auth cap may answer is **how long** an
unauthenticated peer may sit, never **whether** it may connect — and eviction is the form that answers
it. Making the operator the newest arrival then makes them the last candidate rather than the first
victim. A per-peer-IP cap was considered and declined for the same reason: on the loopback default every
local user shares 127.0.0.1 with the operator. Guards
`p12_web_session::a_pre_authentication_flood_cannot_deny_an_authenticated_client_a_new_connection`
(a *new* authenticated connection while the pool is saturated — the property the first guard never
tested; it asserted the 33rd pre-auth connection was dropped, i.e. it asserted the lockout) and
`server::tests::the_pre_auth_population_is_a_fraction_of_the_connection_pool`.

### The browser saver's coalescing rule is asymmetric, and had to be said out loud

`HISTC-2`'s fix routed the clear's OPFS delete through `saver.mjs` so it could not be overtaken by a
snapshot already inside `createWritable()`. The queue it went into had a single `pending` slot assigned
unconditionally, so the *other* order silently reversed it: proved against the real module, `save(k)` in
flight → `remove(k)` → `save(k)` produces a write log of `[write, write]` and the delete **never runs**.
The reachable sequence is narrow but is `HISTC-2`'s own scenario through a smaller window — the debounced
snapshot of a large buffer is still writing when the operator clicks clear, the delete is queued, and a
`flushSave` landing before that write finishes (pagehide, visibilitychange, a console switch, the next
`scheduleSave`) replaces it; if the page then dies before the replacing snapshot drains — and the review
measured that pagehide's OPFS write does not land — the last committed record is the pre-clear
scrollback. Clear, walk away, next viewer sees the secret.

The queue now has **two slots, `del` and `pending`, because they are different intents**, and the rule is
written down where it can be read: a snapshot enqueued *before* a delete is superseded by it — the
operator asked for the record to be gone — while a snapshot enqueued *after* one does **not** cancel it;
the delete runs first and the snapshot follows as a new record. Only the first direction had a test; both
do now. This is the kind of rule that reads like an oversight when found and like an arbitrary asymmetry
when documented, which is exactly why it belongs in prose beside the code rather than only in a test
name.

### Two gates that could not report their own defect

**`CONC-3`.** The remediation factored the refused writer-thread spawn into `apply_spawn` and justified
the absence of an end-to-end guard with "provoking `EAGAIN` from `pthread_create` needs an
`RLIMIT_NPROC` the harness cannot impose on a daemon it shares a process tree with". That was wrong
twice over: `RLIMIT_NPROC` is per *process*, `bash -c 'ulimit -u 1; exec serial-nexus-daemon …'` applies it to
the daemon alone and the harness never notices — and the unit guard drove `apply_spawn`, a function the
fix introduced, so it could not fail against the old inline `.expect`. Both halves are now real:
`LogNode::create` takes an injected `WriterSpawn` (one production code path, a test-only argument) so the
unit guard drives the real constructor, and `itest/tests/p12_log_queue.rs` boots `serial-nexus-daemon`
under `ulimit -u 1` and asks it about the node — old tree: exit 101; new tree: daemon up, node
`faulted (spawn log writer thread: … os error 11)`. **The retracted justification is retracted in the
module doc**, not silently deleted, because "this cannot be tested" is a claim a future session will
otherwise inherit.

**`LEG-3`.** The backoff guard injected an accept returning `std::future::ready(Err(..))` and bounded the
arm with `tokio::time::timeout(300ms, …)`. Against the unfixed `loop { if let Ok(..) = accept().await {} }`
there is no yield point and a ready future consumes no tokio coop budget, so the timeout future was never
polled: the test **hung** instead of failing its ceiling. It stops a green CI and diagnoses nothing. The
reusable rule: **when bounding a never-returning loop with `timeout`, the injected stub must itself park
after a cap, or the bound is unreachable.** The stub now stops answering at `ACCEPT_CAP = 40` — one
constant serving as both the park point and the ceiling the paced loop must stay under — so the unpaced
loop reaches attempt 41 in microseconds, has to yield, and the assertion prints the count it reached.

### The pty's detach purge is a *purge*, not loss

A counter-semantics change a reader watching `state` will notice: a pty's backlog settled at detach —
the master's remaining kernel-buffer input **and** the payload the reader was holding in `pending` — is
charged to §6's per-origin `purged`, not to the node's `discarded_targetward`. The two mean different
things and only one is a defect: `discarded_targetward` is loss, bytes the endpoint could never take
because it went away, while these were discarded on purpose at the moment the floor question settled,
exactly as `handle_last_close`'s purge-on-detach branch already counts a non-holder's backlog. Reporting
them as `discarded_targetward` would announce a §5 violation that did not happen. **So a reader watching
a pty's `discarded_targetward` for detach-time backlog will now always see 0**; the number is in the
origin's `purged`. With no origin to attribute to (a read-only spy edge, or one `disconnect` cleared)
there is no per-origin counter and the node's own loss counter is the only honest home — §5 forbids the
silent version either way.

### A fresh console session inherited the previous one's bytes (§7.2 — and it predates all of this)

Found while chasing the `p6_outage` failure below, and the more valuable half of that investigation: a
**standing product defect**, reproduced on the pre-remediation commit as well, so it is not a regression.

§7.2 promises that on last close the daemon resets the pair "so every client session starts
deterministic". That sentence has to carry two meanings — how the next session's bytes are *framed*, and
*which bytes it sees at all* — and only the first shipped. `handle_last_close` re-applied the baseline
termios with `TCSANOW` and did nothing else; there was no `tcflush` and no drain anywhere in the tree.
The kernel accepts ~13.8 KiB of master writes for a client that never reads — that is the pts input
queue's depth here — and **keeps them across that client's last close**, so the next opener receives the
previous operator's output. The control measurement offers an 8192-byte burst to a session that never
reads and finds all 8192 waiting for the next opener, delivered in reads of `[4095, 4095, 2]` — the
`N_TTY_BUF_SIZE - 1` geometry again. A fresh `picocom` opens onto someone else's
scrollback. The §5 half is the worse half — nothing counted those bytes when they were eventually
destroyed, so `state` reported `discarded_no_client: 0` on a boundary that had silently shed kilobytes.

**Three `tcflush` variants were tried and rejected before the shipped one**, measured on 7.0 against an
8 KiB backlog, and the reasons are the durable part:

* **master `TCOFLUSH`** — clears the flip buffers and leaves the line discipline's own read buffer
  behind. The next opener still read 4095 bytes. Wrong queue.
* **master `TCIFLUSH`** — the other direction entirely; changed nothing.
* **slave `TCIFLUSH`** — does clear it, and fails on two counts. It reports *how much* it discarded to
  nobody, and §5 wants the number; and it leaves a `TIOCPKT_FLUSHREAD` packet readable on the master,
  which at the next poll is **indistinguishable from a client's own session evidence** — so it would
  re-arm the §15.36 presence latch it had just cleared, and the last-close handler would re-fire every
  pass (the 99%-CPU shape `b8d8ed8` records).

What shipped is **reading the slave dry** (`flush_hostward_queue`): it clears the same queue, yields the
exact count, and leaves the master with nothing readable — the last property asserted directly in the
unit test, on the master's own `poll_ready`. The open is the momentary-slave-open mechanism §15.30
already uses off Linux for the baseline termios, used a second time rather than a second mechanism; it
has to be the slave on both platforms because the master cannot name the whole queue on either. Only the
*readability* of that fd is a platform arm (`make_drainable`): on Linux the baseline applied through the
master survives the client's close so the fd is already raw, and a `tcsetattr` there would merely re-arm
the very `TIOCPKT_IOCTL` packet the flush is careful not to create; on BSD the pair has reset to cooked
by then and a canonical-mode read returns only complete lines, so a half-written line would survive the
flush into the next session. `MAX_FLUSH_ROUNDS = 16` bounds the one case that could keep feeding the
loop — the hostward writer thread finishing the chunk it was mid-write on when presence flipped — so a
session boundary can never become an unbounded loop on the single runtime thread; each round already
moves a kernel queue's worth (the whole 8 KiB backlog in round one, 24 µs), so reaching the cap costs
only the accuracy of the count.

**Why `discarded_at_last_close` is its own field** rather than folded into either neighbour: it is
neither of the losses they name. `discarded_no_client` is output the daemon had while *nobody held the
slave* — the presence gate — and these bytes were written for a client that was attached;
`dropped_slow_consumer` is output the daemon never handed to the kernel at all, shed at its own bounded
bridge while the session was live and the client could still catch up. These reached the kernel and were
destroyed by the session *ending*. Folding them into either number would send an operator watching it
move to the wrong mechanism, and §5 asks for loss that is attributable, not merely non-zero. Guards
`p12_pty_setup::a_fresh_console_session_does_not_inherit_the_previous_sessions_bytes` (fresh client reads
the post-attach marker and **no** stale byte; the conservation law
`discarded_no_client + dropped_slow_consumer + discarded_at_last_close == fanned-out total` doubles as
its quiescence signal) and the unit control/treatment pair
`pty::tests::a_sessions_undelivered_output_survives_last_close_and_the_flush_removes_it`, which measures
the control first so the test is a proof rather than a tautology — and prints a NOTE instead of failing
on a kernel that already discards the queue itself, since this is precisely the kind of claim that
differs between 6.18 and 7.0 (§13).

**The residual this work measured and did not fix.** The kernel queue is now emptied at last close; the
*daemon's* own in-flight hostward queue is not. Chunks already handed to the pty's blocking writer
(`sync_channel(hostward_buffer)`, default 32 chunks) are discarded-and-counted by `writer_thread` only
once it observes `present == false`, so a client attaching inside that window can still be handed a
departed session's bytes. It is bounded by `hostward_buffer` rather than unbounded, and — unlike the
kernel queue, which was the whole defect — it is **counted** (`discarded_no_client`) as soon as presence
is observed. Closing it entirely means gating the bridge on a presence epoch rather than on a boolean,
which is a larger change than the defect that remains justifies today.

### The oracle counted what it was handed, not what it asked for (4095, and `overshoot`)

One full-suite run failed `p6_outage::outage_faults_then_purges_then_recovers_byte_clean` with
`received: 8190, sent: 4096` — an apparent **doubling**, which is a far narrower clue than "flaky" and
was chased rather than re-run (§15.36). It was never a doubling. **A pts hands out at most 4095 bytes
per read** (`N_TTY_BUF_SIZE - 1` — a property of the line discipline, not of anything in this tree, and
you will meet it again), and `serial-nexus-sim`'s `read_until` looped `while out.len() < n` while appending the
*whole* of every read, unlike its sibling `recv_loop`, which caps. So `received` was not "how many of the
bytes I asked for arrived" but "how many bytes happened to be in the reads I did", and **any**
contaminated stream of this shape renders as 4095 + 4095 = 8190. The clue that looked like duplication
was an artefact of the oracle's counting rule — §15.36's harness-honesty class arriving a second time, in
the one component every loss-accounting test reads as ground truth.

`read_until` now caps the append at the budget and returns the surplus as `BudgetRead::overshoot`; the
`client` and `pty --sink` verdict lines carry `"overshoot"`, and `budget_met` — the one function both
call sites share, so the rule cannot be re-derived differently — is `bytes.len() == n && overshoot == 0`.
The surplus is still *read* rather than left in the kernel: capping the read would be cheaper and blind.
`overshoot` is a **lower bound** by construction — it counts only what arrived in the reads needed to
fill the budget, never what the peer sends afterwards — so zero means "no surplus was seen", not "no
surplus exists"; non-zero is proof. With the cap in place the same contamination now reports
`received: 4096`, a non-zero `overshoot` and a checksum mismatch: a *short* stream (`timed_out`, low
`received`) and a *long* one (contamination) are finally different verdicts instead of the same number.

### Reconnect releases two backlogs and purges one — by design

The contamination above was not a defect either, and the note exists so a future reader does not "fix"
it. A leg reconnect releases **two** outage-era backlogs and §6 sanctions only one drain: the
*targetward* backlog is purged-and-counted (`purged_on_reconnect`), and the *hostward* one — in
`p6_outage`, step 2's abandoned 64 KiB burst, echoed by the device while the link was down and parked in
daemon A's `uplink/c0` per-channel queue — crosses the restored link and lands in the console. That is
specified behaviour (§5, §7.4): purge-on-reconnect is "the one sanctioned *targetward* drain" and
`leg.rs` gates it on `faces == Facing::Host`. **Do not remove that gate.**

What was wrong was the test, which attached its round-trip client inside the ~20–30 ms window in which
that flood lands and read the flood as its own echo — ~1 failure in 10 unloaded runs. Step 6a now starts
step 7 from a known-quiet console, with two independent gates because each covers the other's hole:
drain the console to quiet (only a *reader* can clear bytes the flood already deposited in the pts
buffer — no RPC counter can see or clear those), then require the receiving daemon's hostward accounting
to stop moving (which catches a drain that finished before the flood began arriving). Two things about
this are worth carrying: it is not a bare sleep and both gates end on an observable (§5), and
**CPU load *suppressed* the failure** — load widens client-spawn latency past the flood — so a green
loaded re-run was evidence of nothing. When a flake is suppressed by load rather than caused by it, the
usual "reproduce it under hogs" instinct is exactly backwards; reproduce it deterministically instead.

---

## OPUS COMPREHENSIVE CODE REVIEW #3 — `docs/historical/32-claude-opus-code-review.md` (2026-07-27)

**Read the review for the findings, and the remediation entry above for what is true now.** The review
file is a frozen record of the review *as delivered* — it still reads "nothing is fixed yet", because it
was written before the remediation. It was taken at `cfb2187` against a tree whose full suite was green
(485 passed / 0 failed / 4 ignored, re-measured during the review), which is its main point — the
defects sat exactly where the suite did not look.

99 candidate findings from 16 lenses (25 finder runs), each handed to an independent verifier that had
**not** seen the report and was told to refute it, on a tree frozen for the whole pass (§15.34's second
clause). **87 confirmed** (7 high, 27 medium, 44 low, 9 nit; 80 unique), **10 refuted**, 2
already-known.

The four clusters, each with one structural remedy rather than N patches:
1. **The resolver's two §12 directions read disjoint sources** — capture walks sysfs, resolution and
   the ambiguity guard read only `/dev/serial/by-id`. The duplicate-serial guard therefore cannot fire
   for the hazard §12/§15.25 promise it closes (two clones collide on one udev link name), so a node
   binds the wrong physical board; and a `usb:` identity captured without a by-id tree can never be
   resolved back.
2. **Exclusivity is released by exit path, not by ownership** — §15.38's D2 fix covers `teardown` and
   *not* `open_port`'s error path, `set_waiting`/`fault`, or the reconnect window. One of those bricks
   a pty-backed device permanently from an ordinary §7.1 modem-line config.
3. **A parked waiting verb starves its own connection's streams** — measured at 11.2 MB of console
   output dropped in one 6 s `lock --wait`, and a 2 s terminal blackout per contended browser
   keystroke, because §17 mandates one daemon connection per browser.
4. **§5's "loss is always visible" has holes at four boundaries** — most sharply a `faces = target` leg
   that discards its own intake `DropCounters`.

Plus: three gates that cannot fail (`p0_license_gate` passes vacuously on a `cargo metadata` failure —
**proven** by deleting the ban entry; `p11_replace_atomicity`'s exclusivity guard; the `RefCell`
meta-gate's file-name exemption), and a documentation cluster in the files a newcomer opens first.

Its **§6 lists the 10 refutations and 2 already-knowns** — consult it before re-filing anything from
this review, exactly as review 26's §6 serves that role. The refutations that exist because the *code*
is right are additionally written up as **§3.20** below, in the deviations family, so they are findable
from the same place as §3.1–§3.19. Everything *confirmed* is answered id-by-id in
**`docs/historical/33-review-32-remediation-ledger.md`**.

---

## v13 BROWSER-UI TRACK (plan §15 / design §15.37, §15.38) — DONE (2026-07-27 session)

Two halves. First an **alignment pass on the v13 documents**, which had regenerated from a
stale base exactly as the v12 pair did. Then the **Playwright track** — which failed three
specs on its first run and turned out to be worth more for what it exposed than for what
it asserts.

**Gates:** 485 passed / 0 failed / 4 ignored (was 480); fmt; clippy (workspace +
minimal-daemon); `cargo deny check licenses bans sources`; macOS cross-check;
`linux.jq` green. The `load --replace` fix was additionally verified on this bench's real
FTDI adapter, three consecutive replaces, `active` throughout with `purged_on_reconnect`
at 0.

### The document alignment pass (do this first on any new design generation)

`docs/historical/30-design-claude-fable-v13.md` and its plan were rebased from a pre-v12 text and
silently dropped rules the code still enforces — the *same* failure the v12 track records
above, and the second time in a row, so it is now a standing first step rather than an
anecdote. **Nothing was a code deviation; the fix was to restore the text.** Restored into
the design: §3's name-legality clause (`BlankName`/`NameTooLong`/`MAX_NAME_LEN`); §7.1's
canonical kebab-case `flow_control` with the unhyphenated aliases; §8's and §15.26's
`unstable_fuzz_api` amendment; §10's `ports` description (it enumerates *every* candidate
and reports bound-ness as a field — §12 in the same document already said so, so v13
contradicted itself); §11's connect/disconnect-are-shipped text and its empty-parse
refusal and whitespace/length name rules; §15.21's P5 verdict folding; §15.34's four
clauses (the single `fan_out` helper, `effective_write_mode` consulted by both validator
and wiring, the empty-parse rule, and the frozen-tree clause for verifiers); §16.2's
`RefCell`-ban *scope*; §16.10's supersession note; and §17's graph page reading topology
from `dump` and status from `state` (which `graph.mjs` has always done). In the plan: §14's
wiring-change paragraph and all four `Validation (executed)` blocks, plus the
review-26 ledger's `docs/historical/` path and the paragraph boundary the new §3 doctrine
block had split.

Method that made this cheap and complete: a sentence-granular diff
(`diff <(sed 's/\. /.\n/g' old) <(sed 's/\. /.\n/g' new)`), every hunk classified, every
"the code still does this" claim checked against the code, and each claimed *code* change
independently verified before acting. After the pass the diff is v12 plus only the
intended new v13 content, which is the check to repeat.

### §15.1 — the Playwright scaffold

`web/ui-tests/`: pinned `@playwright/test` 1.62.0 (Apache-2.0) plus its
lockfile, `playwright.config.mjs` (Chromium only, `workers: 1`, `fullyParallel: false`,
**`retries: 0` on purpose** — §15.36's whole point is that a retry converts a mechanism
into a mystery), traces and screenshots retained on failure. `node_modules`,
`test-results` and `playwright-report` are gitignored; nothing here ships, and the
console's own assets stay vendored and unbundled.

`itest/tests/p8_web_ui.rs` is the gate. It builds the fixture in Rust — where every
other test's fixture lives — and hands the browser a bootstrap URL plus a description of
what it is looking at through the environment (`SNX_WEB_URL`, `SNX_ECHO_CONSOLE`,
`SNX_FAULT_*`, `SNX_REPLACE_CFG`, `SNX_CTL`/`SNX_SOCKET`, …). Three points worth keeping:

- **Skips are loud and defeatable.** `node`, `npx` and the installed package each
  self-skip with the command that fixes them, and `SNX_WEB_UI=required` (set by the CI
  job) turns every skip into a failure. A gate that can skip silently is a gate CI passes
  over a hole — plan §3 rule 7, applied to a new gate on the day it landed.
- **The gate asserts a spec-count floor**, not just "something passed". A filter typo or a
  deleted spec file cannot shrink the suite quietly.
- **`SNX_UI_GREP` narrows a run while debugging**, and `SNX_UI_SLOW=1` includes the
  `@slow` specs. Per push the gate passes `--grep-invert @slow`, which is Playwright's
  spelling of this repo's `#[ignore]` + nightly-sweep convention.

### §15.2 — the behaviour suites, and the three defects they found

13 specs per push plus one nightly (the figure at that landing; the current one lives in the plan's Status table). What matters is the first run: **three failed, and
only one was a browser defect.** Each was independently adversarially verified before any
fix shipped (§15.36 rule 5), and two of the five verifications materially corrected the
diagnosis they reviewed — including one that corrected a comment this session had already
written into the tree.

**D1 — the client duplicated the replay ring into stored scrollback on every reload.**
`offsetSpaceReset(h, from_offset)` returned "the offset space restarted" whenever
`from_offset < frontier`. That is *also* what an ordinary reload looks like, by
construction: `from_offset` is the ring *base* (`ingested − ring.len()`, `tap.rs`), and a
ring exists precisely to re-send bytes the client already has. So every reload with any
replay overlap threw the frontier back to the ring base, re-rendered the ring under a
false "the graph was reconfigured" marker, and persisted the duplicate. Measured in
Chromium: stored history 19 → 38 → 57 bytes over three reloads, one extra copy each time,
unbounded to the 16 MiB cap. The existing unit test passed because it asserted the
function against offsets that never occur (`--replay` "re-sends from at or after the
frontier" — it does not).

The obvious fix — decide on `from_offset + replay_bytes` instead — was proposed, verified,
and **rejected on the verification**: `replay_bytes` counts only the pieces that fit the
tap's 128-deep channel, so for a ring above 8 MiB the head under-reports and the bug
survives; and because the head test fires on a strict subset of today's cases, it silently
*loses* a genuine restart whenever the rebuilt hub has already ingested past the stored
frontier, swallowing real output with no seam marker. Offsets cannot answer the question
in either direction.

So the daemon answers it. `TapHub` carries an **`epoch`** — process-global monotonic,
unique per hub instance, never reused — reported by `tap.open`. The client persists it
beside the scrollback (`opfs.mjs` grew a `SNXHIST2` magic-tagged 24-byte header; a record
without the tag is treated as absent, which is the honest migration for best-effort
scrollback) and re-anchors exactly when it changes. `history.mjs`'s `offsetSpaceReset` is
replaced by `offsetSpaceChanged(storedEpoch, epoch)` + `reanchor(h, fromOffset)`. **This
closes the recorded open issue** that `info.instance` — correctly per-*boot* — does not
rotate when a hub is rebuilt.

**D2 — `load --replace` faulted the serial port it was keeping.** `Daemon::load` is one
synchronous critical section (§15.20): `teardown_with` then `Node::instantiate`, no yield.
`SerialNode::signal_stop` only `abort()`s the supervisor, whose future still holds an
`Rc<SerialPort>` clone across its `.await`, and an aborted `spawn_local` future is dropped
only when the `LocalSet` gets the thread back — after `load` returns. So the replacement's
`open(2)` lands while the outgoing fd is still open, and that fd carries `TIOCEXCL`. The
daemon EBUSYs against itself. On real hardware, measured by strace: `openat = -1 EBUSY`,
then `close` of its own fd 1.6 ms *later*, then a successful reopen one `RECONNECT_POLL`
(1 s) on. During that second the node reports `faulted` with a reason that sends an
operator hunting for a squatter that is the daemon — and **a `send` issued in the window
is acknowledged and then purged**: "sent 20 byte(s)" followed by `purged_on_reconnect: 20`.
§5 permits dropping hostward, never targetward. It reproduces through
`remove-node --cascade` + `add-node` too.

**Fix: one ioctl.** `SerialNode::teardown` calls `sys::set_exclusive(fd, false)` before
releasing the port. `TIOCNXCL` existed in `serial-nexus-sys` and had no caller. Exclusivity is a
claim the node made, so the node gives it back when it stops; the unclaimed window is
bounded by the same critical section and `open_port` re-takes it. Guard:
`p11_replace_atomicity.rs`, **proved fail-first** (3 of its 4 tests fail against the
unfixed tree with exactly the predicted reason; the fourth — that the port is still
exclusive while a live node holds it — passes before and after, and exists so a "fix" that
simply stopped taking `TIOCEXCL` cannot pass). Verified on the FTDI bench afterwards.

**D3 — and why a pts made D2 findable.** `TIOCEXCL` lives on the tty and is cleared only
at its *last* close. A pts whose master `serial-nexus-sim` holds open never reaches that close, so
the flag outlives the daemon's fd and **every** later `open(2)` is EBUSY — for the daemon
and for anyone else (verified with an unrelated `python3` open). A one-second hardware flap
is a permanent fault on a pts, which is the only reason it was visible at all. The same
was true for every pty-backed serial device an operator might legitimately use — socat
`PTY,link=`, QEMU `-serial pty` — and the D2 fix repairs all of them. §16.7's rule earns a
corollary: **a double that turns a transient fault permanent is an amplifier worth
keeping**, not only a coverage gap.

**Two harness bugs of my own, recorded because both were briefly mistaken for product
bugs.** The transport spec matched replies by arrival order, but the server auto-subscribes
a fresh WebSocket and answers that first, so the indices were off by one; and a frame the
server cannot parse as one request is refused with `-32600` and **no id**, so waiting for
the smuggled request's id waits forever. Both are now matched on predicates. The lesson is
the one §15.36 already recorded, in a new place: dump the frames before diagnosing.

**One measurement recorded rather than fixed.** A console rendering a firehose surfaces its
drop counter about **60 seconds** late — twice measured, with the daemon's numbers correct
from its first snapshot. `tap.data` and `state` handlers share one renderer thread, and
`appendText` sets `scrollTop = scrollHeight` per chunk, forcing a synchronous layout of a
`<pre>` growing by megabytes (~45 KB/s). Hiding the terminal first does not help: the click
that would hide it needs the same thread. Forcing the shed *before* the browser opens its
tap does not work either — with no tap the hub only appends to a 64 KiB ring, far faster
than the producer fills the feed, so nothing sheds; the loss genuinely requires a consumer
that cannot keep up. That is honest behaviour for a control-and-observation tool at serial
rates (§5), so the spec is tagged `@slow` and runs nightly. `p8_tap_drops.rs` keeps the
daemon-side half byte-exact per push.

### §15.3 — CI

`web-ui` (per push) and `web-ui-nightly` (`schedule` only, `SNX_UI_SLOW=1`): node 24
pinned, `npm ci` against the committed lockfile, `~/.cache/ms-playwright` cached on that
lockfile's hash, `npx playwright install --with-deps chromium`, `cargo build --workspace`
before the gate (the harness boots plain artifacts — see §5), `SNX_WEB_UI=required`, and
`test-results` uploaded only `if: failure()`. The existing `check` job picks the gate up
through `cargo test --workspace` and skips it, so the per-push Rust lane is unchanged.

### §15.4 — checklist reduction (§16.7)

**Moved from the manual checklist to CI-verified**, with the suite as the pointer: the
OPFS round-trip and its offset splice (`history.spec.mjs`), the `tap.closed` re-anchor
after `load --replace` (`lifecycle.spec.mjs`), the graph page's live indicators and the
editor's add/remove/connect/disconnect flows including the daemon's verbatim structural
refusal (`graph-editor.spec.mjs`), the storage badge, and export/clear. **Still manual:**
rendering fidelity (fonts, terminal visuals, and a firehose's on-screen behaviour — see
the measurement above), and real-rig interaction over the crossover hardware. `docs/security.md`
and the doctor's report language are untouched by the reduction.

---

## CI FLAKE REMEDIATION + 6.18 KERNEL PROBES — DONE (2026-07-26/27 session)

Started as "investigate the GitHub CI failures" and ended up covering three unrelated things: a
CI-infrastructure bug that was already fixed, a family of **load-sensitive test-harness races** that
had been red on four of the last six pushes, and one **real product defect** found beside them. Six
new `serial-nexus-doctor` probes were added on top, to settle on the production kernel the questions this
work could only answer on 7.0.

**Gates:** 480 passed / 0 failed / 4 ignored (was 459 at `548823e`) on **four consecutive**
full-workspace runs on an idle box; fmt; clippy (workspace + minimal-daemon);
`cargo deny check licenses bans sources`; macOS cross-check; `linux.jq` green with 17 supported /
0 degraded / 0 unsupported / 3 skipped.

**One honest gap in that record.** An earlier full-suite run — taken while the box was still
settling from a build, 15-minute load average 5.65 — reported 479 passed / 1 failed, and the output
was piped through `awk` before the failing test's name was captured. The four clean runs came after,
at load < 1. So the suite is green on a quiet machine and there is a residual, unidentified
load-sensitive failure that surfaced once. That is a lead, not a clean bill of health: **if CI shows
a lone failure in a test not named in F1–F5 above, treat it as this one resurfacing rather than as
something new**, and capture the name before re-running. The whole point of this track is that
"flaky" is a symptom with a mechanism behind it, and this one has not been found yet.

### The two stories in the CI history

**Story 1 — already fixed.** Every red run from 2026-07-24 through 07-25 08:03 failed with
`binary serial-nexus-daemon not found at target/debug/serial-nexus-daemon` (`itest/src/lib.rs:60`).
`cargo test` builds test-instrumented binaries under `deps/`, not the plain artifacts the harness
boots. `b81bb093` added `cargo build --workspace` to the four jobs that boot binaries and is an
ancestor of both `main` and HEAD. `main`'s scheduled nightly still hit it at 08:18 on 07-26 only
because `main` was still on `4311997`; it was fast-forwarded to `eb2446e` at 08:47 and passed.
Nothing to do — recorded because the failure text looks alarming in the history and is not.

**Story 2 — the live one.** Four of the last six pushes failed on *three different tests*. The proof
they were nondeterministic rather than commit-linked: `eb2446e` failed `check` on `implementation`
and passed completely on `main` fifteen seconds later — same SHA. Each was root-caused and then
adversarially verified by an independent agent that re-derived the evidence; three of the five
verifications materially corrected the diagnosis they reviewed, and those corrections are the
reason several of the fixes below do not look like the obvious one.

### F1 — `p8_web`'s RFC 6455 test client desynchronised (the macOS red on HEAD)

`web_ws_frame_cannot_smuggle_a_second_request` panicked at `p8_web.rs:418`
"a server frame must not be masked". The `Ws` client was **not frame-atomic with respect to its
deadline**, in two places: `read_bytes` discarded bytes it had already consumed when the deadline
expired, and — the dominant leak — `recv_message` read one frame with *three* `read_bytes` calls
sharing one deadline, so a `None` from a later call threw away the header it had already been
handed. The bytes were gone from the socket and from the caller.

The stream is never idle: the daemon publishes a full `state` snapshot at 5 Hz
(`daemon/src/lib.rs` `SNAPSHOT_INTERVAL`) and the bridge subscribes on construction, so
`collect_replies`' tail deadline always expires *into* a live frame stream. This test is the only
one in the file that reuses one `Ws` across two `collect_replies` calls, so call 1's desync
corrupted call 2 and a payload byte parsed as a header with the mask bit set.

**Not macOS-specific by mechanism** — it reproduced on Linux (2 failures in 120 runs under load, and
~2% of deadline expiries desync even idle). `548823e` grew the file 10 → 14 tests without touching
`Ws`; more concurrent daemons on a 3-core runner merely raised the probability. The product side was
ruled out from source and from a wire capture: tungstenite never masks, and exactly one task owns the
sink (`bridge.rs`).

**The fix is fill-then-commit, and the obvious fix was measured and rejected.** Adding a `pending`
buffer so `read_bytes` accumulates closes only the *rare* leak; a verifier implemented it and still
saw 2 desyncs per 100 idle runs, and noted the originally-proposed regression guard would have passed
against that patch. `Ws` now has a **non-consuming** `fill(n, deadline)` that only ever appends, and
`recv_message` inspects the header and length *in place* and issues exactly one
`pending.drain(..header_len + payload_len)` once the whole frame is buffered. Three new guards cover
each expiry point inside a frame (header, extended length, body) against a scripted TCP peer, so they
are deterministic and need no server. All three fail at `p8_web.rs:419` with the exact production
message without the fix. The file is now 17 tests.

Do **not** relax `assert!(!masked)` — it is the desync detector; removing it converts a loud flake
into a silently mis-parsed WEB-1/SEC-1 security test. Do not lengthen the tail deadlines or stop
`collect_replies` after the first reply either: that deletes the "exactly one reply" property that is
the whole point of the invariant-11 guard.

### F2 — `serial-nexus-sim`'s echo double died on a slave close (`p7_p5` loopback → dangling)

`pty_echo` treated a bare pty-master `POLLHUP` as terminal and exited its echo loop **forever**. On a
pty master `POLLHUP` is level-triggered and set whenever no slave is open, so it fired on a mere
*close*, not an unplug. `serial-nexus-doctor` runs P3 (which opens and closes every port) before P5, and the
close→reopen gap is ~22 µs — a wake-to-run race the doctor usually but not always wins. Measured
survival against the unfixed double: 0% fatal at ≤16 µs, 36% at 20 µs, 92% at 30 µs. Lose it, and P5
wrote into a dead peer for 4 s and reported `dangling (nothing wired to it)`.

A jumper is stateless — it does not stop reflecting bytes because something unplugged from it once.
`run_nullmodem_inner` was already built HUP-tolerant and its doc comment states that contract; the
same treatment was never applied to `--echo`, which is exactly why run 1 of this test never flaked
and run 2 did.

**The verification found a third cause the diagnosis missed, and it changed the fix.**
`run_nullmodem_inner` **busy-spun a whole core**: requesting `POLLIN` only does not suppress
`POLLHUP` in `revents`, so once both slaves closed, `poll` returned immediately, no branch consumed
the event, and there was no sleep. Measured 74.4% CPU standalone, and ~50% cumulative *inside the
failing test* — it pegged a core through the whole of run 2. On a 2-core CI runner that is the
scheduling pressure that pushed F2's window past its knee. So the null modem was not the precedent to
copy; it was a co-conspirator. Both loops now pause (`NO_SLAVE_PAUSE`, 5 ms) on a bare hangup instead
of breaking or spinning, tolerate `Ok(0)`/`EIO`, and make the reply write non-fatal;
`wait_readable` requests `POLLIN` only, with a comment recording that `POLLHUP`/`POLLERR` arrive
unrequested anyway. Verified: 0.3% CPU after both slaves close, flat.

**The originally proposed regression guard was refuted and redesigned.** "Open the pts, close it,
reopen and wait for a round trip" *passes* against the broken double, because in compiled Rust the
reopen lands sub-microsecond after the close and `wait_until` evaluates its condition before its
first sleep — the 0 µs column, where survival was 25/25. The guard now drives the real doctor over a
**multi-port** rig, which widens the window to a measured 86–260 µs. The two-run split is gone with
it: all four classes are exercised in one run, and `p7_p5.rs`'s module doc — which blamed CPU
starvation — now records that the split was a workaround for this defect. Without the sim fix, both
the merged classification test and the new dedicated guard fail **deterministically** on an idle box.

### F3 — tests asserting byte-exactness at a lossy-by-design boundary (`p3_log` and eight siblings)

`log_captures_hostward_stream_without_loss` failed with `received 238078, sent 262144`. This looked
like an invariant-3 data-loss bug and is not. A `pty` node's hostward path is a **bounded bridge whose
overflow is dropped and counted** (`daemon/src/nodes/pty.rs:301`, `:306-310`), default depth 32
chunks, and design §5/§15.19 sanction that loss outright: "a slow spy costs itself data, never its
neighbors." Under CPU contention the bridge fills and the pump sheds — legally.

The decisive evidence: `received + dropped_slow_consumer == 262144` **to the byte** in 3/3
reproductions, while the *log* — the sink the test exists to measure — was byte-exact with
`dropped_bytes == 0` in those same runs. Invariants 3 and 9 both intact. The verification also ran the
discriminator the diagnosis had skipped: raising the *serial* node's `hostward_buffer` does not help
(4/5 still failed), because the pty pump drops rather than awaits and never backpressures upstream —
the pty node's own depth is the only buffer in the path.

Fix: `hostward_buffer = 8192` on the console pty wherever a test asserts a large hostward stream
arrives **complete**, with a comment citing §5/§15.19 so it is not "simplified" away. Applied to
`p3_log` (3 nodes), `p3_counters`, `p5_resync` (4 per-channel consoles), `p7_signals`, `p8_web`,
`p5_exec_crash` (`con-c0` only — `con2`'s probes are 4–5 KiB), `p8_quickstart`, `data_path`, and
`p8_tap_drops`' console. Deliberately **not** applied: `p6_outage` (its byte-clean round-trips are
4 KiB and the 64 KiB burst is explicitly unasserted — a deeper bridge would retain more stale burst
bytes to leak into the post-restore read), `p5_exec_crash`'s `con2`, and every node whose *loss* is
the subject — `p8_tap_drops`' slow tap sheds through its own `TAP_QUEUE_CAP` and was untouched, so
neither of its properties is gutted. `p8_tap_drops`' module doc was corrected: its existing
`hostward_buffer = 16384` sits on the **serial** node and protects the log, not the console echo.

**How the ninth one was found, which is the reusable part.** The first eight came from reading the
files. `p5_resync` came out of a full-workspace run instead — `channel c2: received 458752 !=
manifest delivered 524288`, a 64 KiB shortfall from a per-channel console with no depth, under
nothing more exotic than cargo running test binaries in parallel. Static reading of a file will not
reliably find this class, because whether a given console sheds depends on how much else is running
beside it. **Run the whole suite on a quiet box and then again under parallelism** — the shortfall
always shows as `received + dropped_slow_consumer == sent`, which is the fingerprint to grep the
failure message for.

Secondary, and the change that would have made the original failure self-diagnosing: `serial-nexus-sim`'s
`client` verdict could not distinguish a read deadline from real byte loss — `read_until` broke on its
wall-clock deadline and returned the short buffer with no marker, so a timeout and a drop produced
identical verdicts. It now emits `"timed_out": true` (additive; existing field names unchanged).

### F4 — `p4_exclusivity` was already fixed, but a real product defect sat beside it

The detach-release failure was fixed by `b8d8ed8` (an ancestor of HEAD; 9/9 clean runs including
under the stress shape that reproduced it). The neighbouring defect was **not** fixed and had no
guard: a collapsed pty client session carrying **no data byte** never released the exclusive write
lock, leaving the endpoint dead to every other writer under the `exclusive` default (invariant 8).
Deterministic, 0/10 across two shapes, with two clean controls.

Mechanism: the last-close arm fired only on `(was && !present_now) || (closed && saw_data)`, and
`saw_data` was set **only** for `buf[0] == TIOCPKT_DATA`; the `TIOCPKT_IOCTL` branch deliberately did
not set it. A session that opens, calls `tcsetattr` and closes inside one 5 ms poll window fired
neither arm — even though the master still held the evidence (`read = 1` returning `0x40`, then
`EIO`). The gate was narrower than the evidence the code already had in `buf[0]`. Reachability is
narrow but real: a scripted sub-poll-window probe (`stty`, a health check) or a starved reader — not
a human quitting picocom, which latches presence normally.

**The one-character fix `|| closed` was deliberately not taken.** `b8d8ed8`'s message records that an
ungated `closed`-only attempt spun at 99% CPU; `p9_pty_collapse.rs`'s comment records that planting
that same arm did *not* raise CPU on Linux 7.0. Two credible sources, opposite answers, and AGENTS.md
§7 forbids a one-way decision on 7.0-only evidence. **(6.18 has since answered: doctor P6 reads
`pollin_passes: 0` there too, byte-identical to 7.0 — see the 2026-07-28 entry at the top of this
file. That discharges §7's evidence rule and changes nothing, because the shipped fix never depended
on it, and because what bars the arm today is invariant 16 rule (3) — a correctness property no probe
measures.)** The shipped fix is **kernel-independent**: the
latch (`saw_session`) arms on *any* successful read of `n >= 1`, and `handle_last_close`'s own
`apply_baseline` packet — indistinguishable by type from a client's, and therefore capable of
re-arming the widened latch every pass — is consumed by an unconditional drain afterwards, charged to
`discarded_targetward` if it ever finds a data byte (§5 forbids the silent version). The drain runs
only for a `may_write` endpoint: one not read from this pass is either a locked-out non-holder whose
backlog `handle_last_close` has already purged-and-counted, or an endpoint with no edge at all, whose
backlog is **parked** and must never be dropped (invariant 14).

**Known limitation, deliberate:** a bare open→close touching nothing leaves *nothing* readable, so
there is no evidence to latch on and it is not fixable this way. It is also the harmless case — such
a client sent no command to purge — and it self-heals on the next observed session. P7 (below)
measures exactly this and confirms it: shape (a) leaves 0 bytes.

Guard: `p9_pty_collapse::a_collapsed_termios_only_session_still_releases_the_write_lock`, which drives
a real `stty` (the actual `openat` → `TCGETS2` → `TCSETSW2` → `close` sequence — `serial-nexus-itest` has no
`nix` dependency and `libc`'s termios calls are `unsafe`, which invariant 4 confines to `serial-nexus-sys`).
Verified to fail without the latch fix and pass with it, with the two pre-existing properties still
green in both states.

### F5 — CI configuration

`fuzz-nightly` ran **4 of 9** fuzz targets: `ci.yml`'s loop was still the pre-SEC-7 list, so
`rpc_request_line`, `rpc_base64`, `config_load`, `control_request_lines` and `web_http_head` — every
parser reachable *without a leg*, the five added to close review-26 SEC-7 — were built by
`cargo fuzz build` and then never run. Aggravating: `meta_gates.rs`'s
`every_unstable_fuzz_api_export_has_a_fuzz_target` asserts only that a target *file* exists, so the
tree's own gate read green over the hole. The loop is now driven from `cargo +nightly fuzz list`, so a
new target is picked up automatically.

Also: `license-gate` gained the toolchain-install + cache pair every other job has (it was silently
depending on the runner image's Rust and recompiling cargo-deny on every push); `cargo install` of
cargo-deny and cargo-fuzz is version-pinned (`--locked` pins the tool's dependency graph, not the tool
— and pinning is safe here because CI runs `licenses bans sources`, not `advisories`); the MSRV is
pinned in every job that compiles repo code rather than only `check`; `actions/checkout` and
`actions/upload-artifact` moved to `@v5` (the Node 20 deprecation); `timeout-minutes` and a
`concurrency` group were added; a top-level `permissions: {contents: read}` was added; and the nested
`cargo build`/`test` for `examples/external-codec/` now pass `--locked` so a drifted lock fails loudly
instead of being silently regenerated. **No `rust-toolchain.toml`** — pinning contributors' local
toolchains is a repo-owner decision, not a CI fix.

### The six new `serial-nexus-doctor` probes (P6–P11)

Added so the owner can run `serial-nexus-doctor --json` on **6.18** and diff it against the 7.0 baseline
below. **That diff was taken on 2026-07-27** — see the 2026-07-28 entry at the top of this file and
`docs/serial-nexus-doctor.md`'s 6.18 section; the 7.0 readings recorded here are confirmed on the production
kernel, and none of them changed a line of code. Every probe emits its raw measurements as structured JSON, not just a status word — a human
diffing two runs must see the numbers. A probe reports what it *observed*: "this kernel differs" is
`degraded` with the observation named, **never** `unsupported`, because `linux.jq` gates
`.summary.unsupported == 0` and `meta_gates` asserts the doctor reports no unsupported capability.
Both expectation files were extended and both still pass.

- **P6 — pty-master readiness after the last slave closes.** The blocking question: does the master
  keep asserting `POLLIN` after last-close? **On 7.0 it does not** — 64 passes, 0 with `POLLIN`, all
  reads `EIO`. So an ungated `closed`-only arm would not spin *on the hangup* here, and the
  `saw_session` latch is **not** what holds the anti-spin argument up on this kernel. But the probe
  also caught the other route: the node's own last-close termios reset re-armed readability once
  (1 byte), which makes the drain added in F4 load-bearing regardless. Diff before simplifying.
- **P7 — what a collapsed session leaves readable.** Validates F4's premise. On 7.0: shape (a)
  open→close leaves **0** bytes; shape (b) open→`tcsetattr`→close leaves **1** byte, leading `0x40`,
  ioctl bit set; shape (c) open→write→close leaves 2 bytes with a data packet. So the widened latch
  covers the realistic collapsed session here, and the known limitation is confirmed as (a).
- **P8 — does epoll report a pty master readable while `read` returns EAGAIN?** The behaviour behind
  invariant 1. Probed with **raw epoll through `serial-nexus-sys`** — not `AsyncFd`, which invariant 1's
  meta-gate bans workspace-wide with an empty allowlist, and which would have added tokio to the
  doctor for nothing. Finding worth keeping: a bare level-triggered `EPOLLIN` registration on a pty
  master **agrees with `poll(2)` on 7.0** and does not reproduce the busy-loop. That is not a
  refutation of invariant 1 — the starvation §15.18 records is a property of tokio's readiness
  *guard* (registration lifecycle plus a synchronously-completing ready future), not of `epoll_ctl`
  in isolation. It is now written down where the next person to "just try AsyncFd" will read it.
- **P9 — poll(2) timeout granularity**, informing §15.19's timer floor and the adaptive idle backoff.
- **P10 — pty buffer depth**, informing the `hostward_buffer` defaults behind F3.
- **P11 — real-port line-state counters** (`TIOCGICOUNT`/`TIOCMGET`), **opt-in behind `--port`** like
  P3/P5, because opening a port toggles DTR on hardware that may be wired to live equipment.

`serial-nexus-sys` gained the epoll wrapper (`Epoll`, level-triggered only — an edge-triggered variant
cannot exhibit the persistent-ready loop at all and would quietly report "no problem"),
`pending_input_bytes` (FIONREAD) and `pending_output_bytes` (TIOCOUTQ), all documented as
*instruments, not data-plane primitives*, with 5 new unit tests. It remains the workspace's only
crate with `unsafe`.

P5's verdict fold gained the hung-up class: a port whose peer went away mid-probe is now reported as
`hung up (peer closed) — not classifiable` **and degrades the verdict**, instead of being reported as
`dangling` (which hands the operator the opposite instruction) beside a `supported` verdict — the
DOC-1b shape of an observation nobody acts on. It stays `degraded`, never `unsupported`, which is
reserved for a rig that demonstrably ate bytes.

### Process notes

- **A verifier must not read the report, and the tree must not move under it** (AGENTS.md §9) — but
  this session added a third clause worth keeping: *the verification is where the value was.* Three
  of five verifications materially corrected the diagnosis they reviewed (the `Ws` fix was
  insufficient; the `p7_p5` guard passed without the fix; the `p3_log` attribution rested on an
  experiment that could not discriminate). Every one of those corrections would have shipped as a
  believable-but-wrong fix.
- **The machine confusion, recorded in full because the wrong turn was mine and it cost real time.**
  Midway through, `p3_firehose` started failing its 60 s bound at ~94% of 256 MiB — roughly 4 MB/s
  where §15.32 records 256 MiB in ~2.5 s, a ~24× shortfall. That is exactly the shape of an
  invariant-9 regression (the replay-ring memcpy), so it had to be chased. Two things then happened,
  one right and one wrong.

  The right one: "is this ours?" was settled empirically, with a throwaway `git worktree` at the last
  commit. It failed there too, identically — so not a regression. **Use a worktree for this, never
  `git stash`**: the tree held a large uncommitted change set all session, and AGENTS.md §8's warning
  about `git checkout --` applies to `git stash` just as much.

  The wrong one: having established "not ours", I explained the slowness as *"a throttled sandbox"* —
  an environmental cause asserted with no measurement behind it. It was wrong. The dev box is a
  full-permission Linux laptop, and one `uptime` would have shown load **19.6 on 8 cores**. The cause
  was 16 `yes` processes my own diagnosis agents had spawned to reproduce load-sensitive flakes and
  then leaked; they had been pegging the machine for **ten hours**, and they were also why
  `p5_resync`, `p8_tap_drops` and `p5_exec_crash` were failing. Killed, `p3_firehose` ran in
  **2.53 s** — precisely the healthy figure — and the suite went green.

  Two rules out of it, both now in AGENTS.md §8. **Measure the box before attributing anything to
  it** (`uptime`, `pgrep -x yes`, `nproc`, cpufreq): a confident environmental guess is worse than no
  answer, because it closes an investigation that was still open. And **clean up every
  load-generating process a reproduction spawns** — prefer bounding them with `timeout`. This is a
  machine someone else is working on.

  Worth noting what the leak did *not* cost: because the worktree comparison was run under the same
  load on both sides, its conclusion ("not our regression") was sound, and the fixes it cleared were
  the right ones. The damage was a wrong explanation and a wasted round, not a wrong fix.
- The "is this ours?" question was settled with a throwaway `git worktree` at the last commit rather
  than by stashing — the tree held a large uncommitted change set all session, and AGENTS.md §8's
  warning about `git checkout --` applies just as much to `git stash`.

---

## v12 GRAPH-EDITING TRACK (plan §14 / design §15.35) — DONE (2026-07-26 session)

v12 is the first track since v10 that adds *capability* rather than closing findings. Its
three surfaces all answer the same complaint: the daemon could be operated, but only by
someone who already knew what was plugged in and was willing to take an outage to rewire
it. Design §15.35 is the rationale.

**First, an alignment pass on the design itself.** The v12 document was rebased from the
*pre-remediation* v11 text, so it silently dropped seven rules the review-26 remediation
had added and the code still enforces. None of these was a code deviation — the audit found
zero — so the fix was to restore the text, not to remove working code. Restored: §3's
name-legality clause (`BlankName`/`NameTooLong`/`MAX_NAME_LEN`, whose absence left AGENTS.md
invariant 7 with no design source); §8's and §15.26's `unstable_fuzz_api` amendment (both
modules ship and a meta-gate enforces its one-target-per-export rule, citing a section that
no longer said it); §16.2's `RefCell`-ban *scope* (v12 still said "in `serial-nexus-daemon`" — the
exact wording INV5-CLIPPY-SCOPE proved broken); §11's empty-parse refusal; §15.21's P5
verdict folding; and, in §15.34, the two clauses that make the shared-helper rules binding
(the hostward fan-out is *one* helper for all five producers, and `effective_write_mode` is
consulted by **both** the validator and the wiring). §7.1's `flow_control` spellings were
corrected to the canonical kebab-case with the unhyphenated forms named as aliases, and
§11/§16.10's "connect/disconnect are deferred" text was reconciled with §10/§14/§15.35.

**§14.1 — THE `ports` VERB.**
- **Enumeration** `core/src/resolver.rs::enumerate_ports` → `Vec<PortCandidate>`
  (identity, kind, path, description, `by_id`, warning). **Four** passive sources unioned and
  deduplicated by device node: `/dev/serial/by-id`, `/dev/serial/by-path` (which still
  covers an adapter whose serial number is absent), the `<sys-root>/class/tty` device
  listing itself — added by review 32's `RES-2` and the source that makes `ports` work in a
  tree with `/sys` and no udev serial rules, where the other two are empty — and a
  `<dev-root>/dev` scan for `cu.*` callout nodes, the BSD/macOS face. (This paragraph said
  "three" until review 37 `37-RES-7`; the fourth had been there since `RES-2` shipped.) The `cu.*` scan is deliberately **not** `cfg`-gated:
  the prefix matches nothing on Linux, and one code path keeps the macOS arm reachable from
  a Linux fixture instead of shipping untested.
- Each candidate's identity comes from the *same* private `capture_for_dev` chain
  `add-node` uses, so what `ports` advertises is byte-identical to what binding that path
  would store. `enumerate_ports_agrees_with_what_binding_the_path_would_store` pins it.
- **The verb** `Daemon::ports` adds `bound_to`, resolved by comparing *paths* (each serial
  node's stored identity through `resolve_current_path`) rather than identity spellings —
  so a device held by a `by-path:` identity reports bound even though `ports` advertises it
  as `usb:`.
- **Passivity has two proofs.** Structural: `meta_gates::port_enumeration_cannot_open_a_device`
  asserts `serial-nexus-core` declares no dependency that *could* open a device
  (`serial2`/`serial-nexus-sys`/`nix`/`libc`) and still forbids `unsafe`, with a planted-violation
  self-proof on the manifest scanner. Runtime: `p10_ports.rs`'s fixture device nodes are
  writer-less FIFOs, so a blocking `open(2)` would never return and the RPC would hit its
  timeout.
- `Daemon::start_with_args` was added to `serial-nexus-itest` so a test can pass `--dev-root`
  without a fourth hand-rolled `KillOnDrop` copy.

**§14.2 — `connect` / `disconnect`.** The verbs are easy; making them *the same operation
`load` performs, minus the outage* is the work, and it needed a wiring change.

- **The problem.** Edges were only ever created by `Wiring::build` and consumed by
  `Node::start`; a node's tasks owned their channels outright. Adding an edge to a running
  graph therefore meant restarting tasks — and aborting a task drops its *targetward*
  receiver out from under senders that stay live in `GraphState::endpoint_targetward` and in
  every writer origin. That is MAP-1's chain exactly.
- **The shape that fixed it** (`daemon/src/runtime.rs`), resting on one observation:
  §4 rule 2 gives a target-facing endpoint **at most one edge**, so everything an edge
  contributes is derivable from the two endpoints' *permanent* resources.
  - `FanOutList` / `SharedFanOut` — a host-facing endpoint's live sink list, `Arc<Mutex<…>>`
    because the serial reader owns it from a blocking thread (invariant 2). One uncontended
    lock per 64 KiB chunk; `p3_firehose` is the standing proof it costs nothing.
  - `EdgeInbox` — a target-facing endpoint's stream of hostward receivers. Its pump loops
    `while let Some(rx) = inbox.recv().await { while let Some(c) = rx.recv().await {…} }`,
    so it outlives every individual edge with **no extra per-chunk hop**. `disconnect`
    closes the edge channel by dropping the producer's sink, and the pump drains what was
    buffered before parking again — the detach costs no bytes.
  - `EdgeSlot`/`TargetEdge` — the targetward sender + lock + origin id, re-read per chunk,
    with a `Notify` so an unattached pump can park.
- **Three states, three behaviours** — and the middle one is the subtle one. Attached and
  writable: forward. **Attached but read-only** (`write_mode = "never"`): drain and count,
  because parking would wedge a writer forever on a configuration that will never become
  writable (MAP-1). **Not attached**: park in `runtime::await_origin`, because targetward is
  the direction §5 forbids dropping on — a detached edge must stall its writers exactly as a
  steal does. The `attached` flag exists precisely to tell the last two apart.
- **`disconnect` releases and purges.** §15.27's phantom-holder lesson applied before the
  bug could recur on the new path: unregister the origin (holder or queued waiter), wake the
  FIFO head, purge the departing origin's un-flushed backlog, and report both
  (`released_lock`, `purged_bytes`) rather than doing it silently.
- **Two smaller things that had to move with it.** `is_config_mutation` gained
  `connect`/`disconnect` — without that, a rewiring would have evaporated on restart while
  `dump` still showed it, a fail-*silent* shape. And the exec supervisor now consults the
  live edge slot when it reports `active`, instead of latching a status at child spawn and
  racing `connect` to describe the same node.
- **Deliberately not added:** a duplicate-edge check in the verb. Re-adding an edge gives
  its target endpoint two, which §4 rule 2 already refuses by name; a second implementation
  would be dead code that could only ever disagree with the validator.

**§14.3/§14.4 — THE GRAPH AND EDITOR PAGES, AND THE POSTURE.**
- `web/src/assets/graph.mjs` and `editor.mjs`: pure renderers, handed snapshots
  by `app.js`. One shell, three views, hash-routed (`#graph`, `#editor`) so each is
  bookmarkable without the server growing a router.
- The graph page reads `dump` for topology and `state` for status — the §15.8 split kept in
  the client too. It deliberately does **not** re-derive the write-mode promotions: that
  computation has exactly one implementation, in the daemon (invariant 12).
- The editor **enforces nothing**. Every §4 rule is the daemon's, and the page surfaces the
  refusal verbatim; `rpcFull` exists so the operator sees `data.errors[0]` rather than "it
  failed". What the page does own is the destructive confirmation, and it *names what
  cascades* rather than asking "are you sure?".
- `bridge::ALLOWED` grew `add-node`, `remove-node`, `connect`, `disconnect`, `ports`.
  `load`, `teardown`, `shutdown` and the nonexistent `set-attribute` stay refused, pinned
  both as a unit test and end to end. `docs/security.md` gained the capability statement:
  graph editing is daemon-user capability (a log node writes files, an exec codec runs
  commands), so the token is operator trust — stated in a block quote rather than implied.

**⚠️ Adversarial audit (6-lens find→verify workflow, ~50 agents) — 47 candidates; do NOT
regress these.** The lenses were the wiring refactor, the two verbs, `ports`, the web
surfaces, an invariant-by-invariant sweep, and the documentation. The wiring refactor is
where the damage was, which is the expected shape: it is the part that changed a
load-bearing structure rather than adding one.

1. `[correctness, CRITICAL]` **A targetward pump parked in `reacquire_held` when its edge
   was disconnected never woke.** `unregister` makes both `may_write` and `reclaim_held`
   false forever while the lock stays *open* (the endpoint is fine — this origin left), so
   the loop had no exit: the pump parked holding a chunk it had already taken, and a later
   `connect` could not revive it, because the new edge gets a new id and nothing wakes the
   old wait. `reacquire_held` now also exits on `write_mode(id).is_none()`.
2. `[correctness, HIGH]` **A `faces = "target"` leg channel with no local edge head-of-line
   blocked the whole wire connection.** The refactor gave every channel a task, and an
   unattached one parked — filling its bounded queue and stalling every other channel on
   the link (§9). The interior nodes park correctly, because their pump serves one endpoint;
   a **shared** pump must count instead. The leg and the exec codec (whose route runs inside
   the child's single stdout decode loop) now drain-and-count. The rule to keep: *park only
   where the stall is confined to the endpoint whose edge was removed.*
3. `[correctness, HIGH]` **`connect` of a `held` edge never woke the lock's waiters**, so a
   held pump already parked in `reacquire_held` sat there until an unrelated transition
   nudged it; and a `held` origin granted the lock inside `register` **skipped
   purge-on-acquire**, which §6 requires on every fresh exclusive grant. At load that was
   vacuous; on a running graph the new origin can have a backlog, and firing it is exactly
   the stale-command hazard §6 exists to prevent.
4. `[correctness, MEDIUM ×2]` **Two `never`-origin bugs, one root cause.** The lock
   registers *every* origin so `state` can list it, while `origin_locks` holds only those
   `lock`/`unlock` can address. Detach unregistered through the narrower map — so a log edge
   left a phantom origin per cycle — and `absorb_wiring` derived the live-`connect` id floor
   from it — so a graph whose only edge was `never` handed the next `connect` an id already
   registered on that very lock, where the two aliased. Fixed by recording the registration
   in the edge slot (`TargetEdge::registered`, every mode) and taking the floor from
   `Wiring`'s own counter. Guards:
   `disconnecting_a_read_only_edge_leaves_no_phantom_origin` and
   `a_live_connect_never_reuses_an_origin_id_the_load_already_registered`.
5. `[correctness, HIGH]` **The `ports` passivity meta-gate could not detect the violation it
   names.** `declares_dependency` matched `krate = …` and `[dependencies.krate]`, but **not**
   `krate.workspace = true` — the only spelling this workspace uses. It would have passed
   forever while catching nothing, which is INV5-CLIPPY-SCOPE's exact shape. It now matches
   every form (dotted, table, target-specific, renamed via `package`), plants each one in its
   self-proof, and its *claim* is narrowed: it proves serial-nexus-core carries neither the crates
   that drive a port nor an explicit file-opening API — not that nothing is opened, since
   `std::fs` needs no dependency and no `unsafe`. The behavioural proof is `p10_ports`'s
   writer-less FIFOs.
6. `[correctness, MEDIUM]` **`bound_to` compared paths textually**, so a node holding a
   device through a symlinked raw path (`raw:/dev/serial/by-id/…`) read as *free* and invited
   a second binding that just faults on TIOCEXCL. Both sides are canonicalized now;
   `bound_status_follows_a_symlinked_raw_path_to_the_device_it_names` was verified to fail
   without the fix.
7. `[correctness, HIGH]` **`hidden` did not hide.** The graph/editor views set `hidden` on
   `#pane-head` and `#sendform`, but the UA rule `[hidden] { display: none }` is
   origin-weaker than *any* author declaration, and both carry `display: flex`. The console
   chrome stayed on screen under the new views. One author-origin `[hidden]` rule fixes it.
8. `[correctness, MEDIUM ×3]` Web client: the graph page repainted on every `state` tick from
   a **stale `dump`**, so a topology change from another client never appeared while the page
   visibly refreshed around it (now a throttled refetch); `#term` lost its scroll position to
   `display:none` and never followed the tail again after a view round trip; and `ws.send`
   was called unguarded from controls that are live before `onopen`, silently discarding and
   leaking the promise (now guarded, and `onclose` settles what is in flight).
9. `[docs, HIGH]` **`docs/security.md` contradicted itself**: it said a compromised page
   "cannot stop the daemon or replace its configuration wholesale" two paragraphs after
   stating that a token holder can run an arbitrary command as the daemon's user. `add-node`
   accepts an exec codec, which subsumes both. The lifecycle verbs stay off the wire because
   they are not what the operator asked for — **not** because withholding them constrains an
   attacker who holds the token. The page says so now, and its "never writes to disk on the
   daemon's behalf" line is qualified: watching does not, *editing does*.
10. `[docs, MEDIUM ×2]` Design §10 described `ports` as enumerating devices "not bound into
   the graph" (it returns all, tagged); §17 said the graph page renders from `state` (`state`
   carries no edges and must not — they are configuration). Both corrected, plus the smaller
   stale-comment sweep (`connect` deferred, the flow-control spellings, the moved review-doc
   paths, the v11 doc pair in AGENTS.md §9, the test count below).

**A methodology note, because the verification half went wrong in a new way.** §15.34's rule
— a verifier gets the finding and the tree, never the report — was followed: no skeptic could
read the finder's output. But the *tree changed under them*. Fixes were landing while the
verifiers read, so 35 of 43 verdicts came back "not real", including for defects that were
unambiguously real when filed and are now fixed (the `reacquire_held` wedge, the leg's
head-of-line block, both `never`-origin bugs). A refutation of an already-fixed defect is not
evidence the finding was wrong; it is evidence the verifier read a different tree. Every
finding acted on above was therefore confirmed by reading the code directly, and the three
sharpest were confirmed the only way that settles it — a test that **fails without the fix**
(`bound_status_follows_a_symlinked_raw_path_to_the_device_it_names` was run against a reverted
`connect` and did fail; the two origin guards were written before their fixes and failed).
The rule to add for the next audit: **freeze the worktree for the verification pass**, or run
the verifiers against a `git worktree` pinned to the commit the finders read. Fixing while
verifying invalidates the verification just as surely as letting a verifier read the report.

Accepted rather than fixed, each now stated in the code and the RPC docs rather than left to
be discovered: `ports` is O(adapters²) because `capture_for_dev` re-scans to detect a
duplicated serial number — bounded by adapter count on a human-scale verb; `bound_to` names
the *first* node when two are configured on one device (a shape validation permits and
TIOCEXCL then punishes); and `disconnect`'s purge reaches a pty's kernel buffer only, which
is the case §6's rule is about — an interior origin's un-sent bytes stay in its own channel
and are delivered if it is wired again.

**Gates (all green on the Linux 7.0 dev box):** `cargo build --workspace --locked`;
`cargo test --workspace --locked` (**459 passing / 0 failing / 4 ignored**, up from 436);
`cargo fmt --all --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`
plus the minimal-daemon pass; `cargo deny check licenses bans sources`;
`cargo check --target x86_64-apple-darwin --workspace --exclude serial-nexus-web`;
`serial-nexus-doctor --json | jq -e -f expectations/linux.jq`. New test files: `p10_ports.rs` (4),
`p10_edge_surgery.rs` (6), `web/src/assets/graph.test.mjs` (10 under
`node --test`), plus four new `p8_web.rs` tests and one new meta-gate.

**REAL-BROWSER VALIDATED (2026-07-26, same session).** Driven through Chrome against the
FTDI crossover rig (`/dev/ttyUSB0` `BH00LL8O` as serial node `usb0`, an external
echo+banner responder on `ttyUSB1`), so §16.7 owes nothing here now. What was exercised, in
order: the console view streaming real device bytes with its replay-ring splice marker; the
**graph page** rendering three node cards with facing-labelled endpoints, live
`active`/`waiting` glyphs and the edge list; a **scripted fault from a second client**
(`add-node` a map with no upstream, then `disconnect`) appearing on the open page with **no
reload** — the new node in amber with its reason inline, the edge count dropping — and
flipping back to green when `connect` gave it an upstream, with `usb0` then wearing the
`🔒 mapped/raw` badge because the raw edge was promoted to `held`; the **editor page** with
its device dropdown populated from `ports` by real identity (`usb:0403:6001:BH00LL8O:00 —
FTDI FT232R USB UART, serial BH00LL8O, interface 00 (bound: usb0)`); an illegal `connect`
refused inline in the daemon's own words; a **log node created and wired entirely from the
browser** that then captured live rig bytes to disk; and the cascade confirmation naming all
three edges (captured by stubbing `window.confirm` so no modal blocked the harness, and
declined).

**Two defects only a browser could show, both found here and fixed.** They are worth naming
because they are the argument for doing this at all: an API test cannot see a stylesheet or
a scroll offset.

1. `[correctness]` The refusal line printed the broken rule **twice**. A structural error
   carries its first message in `error.message` *and* in `data.errors[0]`, and the editor
   appended the list wholesale. It now shows only the entries `message` does not already
   carry — so an operator who broke two rules sees both, and one who broke one reads it once.
2. `[confirmation]` The `[hidden]` fix from the audit is now confirmed *numerically*, not
   just by eye: in the graph view `#pane-head`, `#sendform` and `#term` all compute to
   `display: none` despite their author `display: flex` rules, which is only possible
   because of the author-origin `[hidden]` rule. Likewise the terminal follows the tail
   after a view round trip (`scrollTop + clientHeight >= scrollHeight`), which is the
   `display:none`-resets-scroll fix. Both would have read as green in any API test.

One rendering choice worth knowing rather than changing: the edge list shows the **declared**
write mode, so `usb0 ↔ mapped/raw` reads `on-demand` even though the runtime promoted it to
`held`. That is invariant 12 holding — the page must not re-derive a promotion the daemon
owns — and the effective truth is visible right beside it, as the lock badge on `usb0`.

---

## REVIEW-26 REMEDIATION — all 93 findings dispositioned (2026-07-25 session, uncommitted)

The findings in `docs/historical/26-claude-opus-code-review.md` are addressed. The finding-by-finding
ledger is **`docs/historical/27-review-26-remediation-ledger.md`** — read that before re-filing
anything from the review; several items are *deliberately declined* with reasons, and
re-fixing a cleared candidate is its own defect. Design §15.34 records the pattern-level
lessons; plan §13 is the track.

**Scale.** 69 files, ~12k insertions. Suite **265 → 435 passing / 0 failing**; nine new
`serial-nexus-itest` files (`p9_*`), three new fuzz targets, two new meta-gates. `cargo fmt`,
`cargo clippy` (full **and** minimal-daemon), `cargo deny`, the macOS cross-check and
`serial-nexus-doctor --json | jq -e -f expectations/linux.jq` are all green (the `-e` added 2026-08-07 — without it jq exits 0 whatever the clauses say; notes §3.74).

**The shape of the fixes, which matters more than the list.** Every headline defect was an
invariant upheld in a layer that could not enforce it, so the fixes moved each one down:

- **Structural validation instead of runtime surprise.** `replay_ring`, `hostward_buffer`,
  the leg timers and the log padding are range-checked in `GraphConfig::validate`; a
  codec/exec multiplexed edge must be `held` or `never`; at most one effectively-held edge
  per host endpoint; `faces = "target"` on a serial node is refused as §14-deferred;
  unknown keys *and* unknown tables are rejected; names are non-blank and length-bounded.
  All of it runs **before** `load --replace` tears anything down — which is the whole
  point, because two of these used to load cleanly and then kill the daemon.
- **One implementation per rule.** Write-mode promotion is `GraphConfig::effective_write_mode`,
  called by both the validator and `Wiring::build`. Hostward fan-out is `runtime::fan_out`,
  called by all five producers — which is what made the map's missing unattached-loss
  counter impossible rather than merely fixed. Purge-to-quiescence is
  `boundary::drain_to_quiescence`, shared by serial and leg.
- **Fail-safe instead of fail-open.** The web bridge screens a *parsed* value and forwards
  the re-serialised one behind an **allowlist**; a denylist admitted every verb §10 grows
  later and held only while someone remembered to extend it.
- **A gate that cannot silently disarm.** The clippy `RefCell` ban now exists in both
  crates *and* is backed by a `meta_gates` test that fails when a new crate starts holding
  daemon state — because the original broke by a crate move and nobody noticed for a
  release. Invariant 1 (no `AsyncFd`) got the same treatment.
- **A precondition that can fail.** Doctor P5 folds the rig certificate into its verdict.

**Two things a future session should know.**

1. **The same defect had two more homes than the review found.** MAP-1 — an interior node
   dropping a targetward receiver while its senders stay live, which closes the channel
   under a pty origin and takes presence latching and detach-release with it — was
   reported against the map. Fixing the map was not enough: the adversarial verification
   reproduced it live in `codec` and `exec`, reached through their *early-return* paths
   rather than their read-only mode. The guard is now written against the rule for all
   three kinds (`p9_unwired_interior.rs`), not against one node. When a finding names one
   node, check its siblings.
2. **`spchex` changes the bytes an existing configuration produces.** It now matches
   picocom's `M_SPCHEX` (DEL plus C0 except TAB/LF/CR) where it matched SPACE. Verified
   against upstream `picocom.c`'s `do_map`/`map2hex`, fetched and read — not against the
   review's transcription of it. `packaging/README.md` carries the operator-facing upgrade
   notes for this and everything else that used to load and now does not.

**Manual checklist addition (§16.7).** `app.js`'s console-switching logic — WEB-4's
re-entrancy generation guard, and the new `tap.closed` / offset-space-reset handling that
closes the browser half of the `load --replace` freeze — is exercisable only by a real
browser. Per §16.7 it goes on the real-browser checklist rather than being counted as
covered: select two consoles in rapid succession and confirm exactly one tap survives in
`state.taps`; then `load --replace` from a second client and confirm the pane says it was
detached and, on re-selection, resumes rendering across the offset restart. The pure
modules under it (`history.mjs`, `saver.mjs`) are `node --test`-covered per push.

---

## OPUS COMPREHENSIVE CODE REVIEW #2 — `docs/historical/26-claude-opus-code-review.md` (2026-07-25)

A second full-workspace review (multi-agent + adversarial verification + live reproductions) landed at
`docs/historical/26-claude-opus-code-review.md`. **The review file is a frozen record of the review as
delivered** — it still reads "nothing is fixed yet", because it was written before the remediation
that followed it. Read the remediation entry above it for what is true now, and read the review for
*why*: its §1 action table, its §6 list of the 20 refuted candidates (which need no action and should
not be re-filed), and its §7 reproduction log. Its **justified** deviations are recorded below as
§3.16–§3.18 (per-node `arbitration`, the map's `held` raw-edge promotion, and `serial_nexus_core::data` as
specification-not-path — the last also corrects §3.3). A fourth, §3.15 (flow-control spelling), was
**withdrawn**: blind re-verification showed it was a real defect, not a deviation.

Highest-impact confirmed items **as found** (each was reproduced on a live daemon at `b8d8ed8`, and
each now has a regression guard): the web bridge's verb denylist was bypassed by a newline inside one
WebSocket frame (`teardown`/`shutdown` executed from the browser); `replay_ring` and
`hostward_buffer` were unbounded, the first aborting the process on the next hostward byte (and
crash-looping via the persisted config), the second panicking *after* a `--replace` teardown; a
codec/exec multiplexed edge with an omitted `write_mode` silently parked every targetward byte while
`send` reported success; `spchex` implemented SPACE→hex where picocom's `M_SPCHEX` is the
control-character class, leaving no rule able to hex a control byte; and a pipelined request during a
waiting verb tore down the whole control connection, which killed web-console sessions. Also: the
clippy `RefCell` ban (invariant #5) had stopped covering `serial-nexus-daemon` at the v8 library split —
proven with a planted `RefCell` plus a lint canary — so AGENTS.md §6 and `cell.rs` overstated it.
That last one is the one to remember for its shape rather than its severity: a `clippy.toml` disarms
**silently** when the code it governs moves to a sibling crate, which is why the ban is now duplicated
into `daemon/clippy.toml` *and* backed by a `meta_gates` test that fails when a new crate starts
holding daemon state.

The review's §7 is a reproduction log (22 live reproductions). **Verification is complete and was
independently re-verified**: all 113 candidate findings faced an adversarial verifier, and 21 of them
were then re-checked **blind against a pristine `git worktree`** after an audit showed the first pass
was contaminated. Final: **93 survived** (2 critical [the same bridge defect found twice], 8 high,
29 medium, 45 low, 9 nit) and **20 were refuted**, with §6 tabulating every refutation.

**Why the re-verification happened, and why it mattered.** This review document and the new §3.1x
entries sat in the working tree throughout two quota-forced workflow resumes, and **64 of 113
verifiers actually fetched the review** while checking "is this already documented?" — so their
verdicts were not independent of the conclusions they were checking. The 21 whose disposition
genuinely rode on a contaminated verdict were re-judged from a checkout containing neither document.
**17 of 21 agreed; 4 did not**, in both directions:

- **CFG-1** REFUTED → **CONFIRMED**: the circularity caught. §3.15 below is withdrawn as a result.
- **LOG-2** CONFIRMED → **REFUTED**: reproduced live, but the behavior is what §5 specifies for the
  default overflow policy. A live reproduction is evidence about *behavior*, never about whether the
  behavior is a *defect* — that judgement is a design question, and the blind reader was right.
- **LOCK-3** and **SYS-1** REFUTED → CONFIRMED/PLAUSIBLE (both low).

`DM-6` and the map-config half of `MAP-1` were re-judged blind and came back REFUTED-as-actionable
again, so §3.16 and §3.17 stand on independent evidence.

---

## WEB CONSOLE REAL-BROWSER VALIDATION (design §17 / plan §11) — DONE (2026-07-25 session, uncommitted)

First **actual browser** validation of `serial-nexus-web`. Previously the web/OPFS round-trip rode the
manual checklist (§16.7) "because an agent can't drive a browser" (see the v11 track note below); this
session drove a real Chrome via the browser-automation harness. Run on the **real FTDI crossover rig**
(`/dev/ttyUSB0` `…BH00LL8O` ↔ `/dev/ttyUSB1` `…BH00L4KU`): the daemon owns `ttyUSB0` as serial node
`usb0` (exclusive, default 64 KiB ring); an **external raw echo+banner responder** on `ttyUSB1` returns
typed input and emits a periodic `[responder] tick N` line, so both the send round-trip **and**
unsolicited hostward rendering are exercised; `serial-nexus-web --bind 127.0.0.1:8088 --token … --socket …`.
Every claim below is asserted on structured RPC / OPFS state / byte content, never on a screenshot.

**All green:**
- **Bootstrap/token:** `?token=` → **302** → clean `/` (token stripped from the address bar);
  `Set-Cookie: nexus_session=…; Path=/; HttpOnly; SameSite=Strict` (HttpOnly ⇒ `document.cookie` empty,
  as intended). App assets + WS connect; console list from `state`; `info.instance` fetched on connect.
- **Terminal I/O over the physical UART:** select → `tap.open` (inputs enabled), `— replay (N bytes) —`
  marker, live incoming ticks render; a typed line → `send` → ttyUSB0 → crossover → responder echo →
  hostward tap → **rendered byte-exact, exactly once** (no dup).
- **OPFS scrollback:** file `hist_<host>__<endpoint>__<instanceNonce>.bin`; reload restores
  `— stored history —` then `— replay —` **spliced with no dup/gap** (the sent line appears once across
  the seam — the subtle §11.8 offset-splice claim). Export (`usb0.log`, correct bytes, no RPC); Clear
  (`— history cleared —` + OPFS file deleted, client-local).
- **Arbitration UI:** 🔒 holder badge + `locked by <holder>` (live via `lock`/subscribe); a send while
  locked → **explicit** steal confirm naming the holder (never automatic, §17); the steal took the lock
  at the daemon (holder → null).
- **Security:** the WS bridge refuses `load`/`teardown`/`add-node`/`remove-node`/`shutdown`/`connect`/
  `disconnect`/`set-attribute` at the bridge with **-32601** and the daemon **survives a `shutdown`
  attempt**; HTTP gates: no-cookie→**401**, bad-Host→**403**, wrong-token→**401**; the ES-module chain
  (`/history.mjs`, `/opfs.mjs`) serves 200.
- **Drop accounting (§5 / invariant #9):** a gated 256 MiB `serial-nexus-sim pty --source` firehose added via
  `add-node` (`usb0` untouched) into a slow browser tab → that tab's tap `dropped` climbed to
  **260,632,266** while `feed_dropped=0` (shared producer→hub hop) and `usb0` stayed unaffected — "a slow
  spy costs only itself".
- **Nonce reset:** a daemon restart rotated `info.instance` (`14446068600432912000` →
  `6963886532795428825`); the app keyed a **fresh** OPFS segment under the new nonce and left the old
  file **unread** (the reset-detection working as designed).

This **discharges the web/OPFS round-trip item on the manual checklist (§16.7)**. The **map** node's
browser visual (plan §12.2) still rides the checklist — this session validated a plain serial console,
not a map render.

**⚠️ UI ISSUE FOUND — NOT YET FIXED (proposed patch below): `load --replace` freezes a live web
console's OPFS-restored view.** After an operator runs `serial-nexus-ctl load --replace` (or
`remove-node`+`add-node`, or `teardown`+`load` — all reach the same teardown/rebuild) **beneath a live
browser session** tapping the endpoint, the console stops rendering new bytes. Root cause, verified
line-by-line:
- `TapHub::new` hard-codes `ingested: 0` (`tap.rs:234`); `load(replace=true)` does `teardown()` →
  `st.tap_hubs.clear()` → `spawn_tap_hubs(...)` (`daemon.rs:419-421`), so every hub's hostward **offset
  space restarts at 0**. But `instance` is an immutable per-boot `u64` (`daemon.rs:155/209`, reached only
  through `&self`), so `info.instance` **does not change** — violating the assumption the offset counter's
  own doc states at `tap.rs:196-200` ("a daemon restart resets it *and the info nonce changes* so a client
  detects the reset rather than splicing across it").
- Browser: `keyFor` (`app.js:159`) keys OPFS on the unchanged nonce → reopens the pre-reset file;
  `fromStored` sets `frontier = stored.endOffset` (high, `app.js:175`); the re-anchor
  `history.frontier = res.from_offset` is guarded by `if (!stored)` (`app.js:187`) so it **never fires**
  for a restored history; every live frame (offset ≈ 0) hits `splice()`'s `end <= frontier` overlap
  branch (`history.mjs:51-52`) → returns an **empty** array → nothing appended. **Permanent freeze**
  (the reset stream would have to emit ~`frontier` bytes before it caught up).
- **Severity: web-UI availability only.** §17 denies the web console all graph/lifecycle verbs, so a web
  user **cannot** trigger it — only an operator reconfiguring via CLI beneath a live session. **No data
  loss / corruption / persisted-state change:** the daemon stays healthy (rx climbs, a fresh raw
  `tap.open`/WS on the same endpoint streams live fine), and the stored OPFS bytes are intact (merely
  stranded in a dead offset space). Recovers today via the **Clear button or a hard reload** (both
  re-anchor). Empirically confirmed by deleting the OPFS file → live rendering resumes immediately.
- **Recommended fix — Option A (browser-only, ~6 lines, daemon untouched, OPFS keys stay stable so
  benign reloads keep their scrollback):** add a pure helper to `history.mjs` —
  `offsetSpaceReset(frontier, fromOffset, replayBytes) = frontier!==null && (fromOffset+(replayBytes||0)) < frontier`
  (the live edge rewound below the restored frontier, impossible within one monotonic offset space) — and
  in `app.js selectConsole`, when it fires on a restored history, re-anchor `history.frontier =
  res.from_offset` and emit a `— stream reset (endpoint reconfigured) —` marker (old bytes stay as
  scrollback, new bytes splice forward). Regression: a case in the `history.mjs` `node --test` suite run
  by `itest/tests/p8_web_history.rs`. **Alternatives considered and rejected as heavier:** (B) a
  per-endpoint offset epoch surfaced in `tap.open`/`state` folded into the OPFS key — honors invariant
  #10 for *all* offset clients but orphans scrollback on every benign reconfigure; (C) rebump the whole
  `instance` nonce — coarse, orphans every endpoint, needs interior mutability on an immutable field;
  (D) carry `ingested` forward per endpoint identity — semantically dishonest (concatenates two unrelated
  streams with no gap). A doc-only note at `tap.rs:196-200` recording that `load --replace`/hub rebuild
  also resets the offset space without a nonce change is worthwhile regardless.
- **Update after the review-26 remediation — still open, but the daemon half moved.** The review
  confirmed this and found the wider defect underneath it (TAP-1): the browser was only the loudest
  victim; `teardown`/`load --replace`/`remove-node` cleared `tap_hubs` beneath *every* open tap, and the
  client-side handles survived, so any tap client sat on an open connection receiving nothing, with no
  notification and no error. That half is **fixed**: a hub dropped beneath live taps now sends a terminal
  **`tap.closed`** notification naming the tap, the endpoint, and a stable reason token
  (`endpoint removed` / `graph replaced` / `teardown`), terminal for the tap and not for the connection.
  `history.mjs`'s `offsetSpaceReset` (Option A) is **still not implemented** and `app.js` does not yet
  act on `tap.closed`, so a browser session still stops rendering across a `--replace` — now because its
  tap is explicitly closed rather than because it is silently splicing into a dead offset space. Fixing
  the browser half is the remaining work, and `tap.closed` makes it easier: re-open the tap and re-anchor
  on the new `from_offset` rather than inferring a reset.

**Secondary UI observation (not a data bug):** under a sustained firehose the tab's main thread gets
pegged rendering the backlog (the extension connection briefly dropped), and `#pane-drops` lags reality
badly — it showed `6,925,512` while the daemon's real figure was `260,632,266`. Cause: `#pane-drops` only
refreshes on the daemon's 200 ms `emit_state_snapshot` notifications (`lib.rs:64` `SNAPSHOT_INTERVAL`),
which queue **behind** the tap-data backlog on the single shared WS (head-of-line blocking of the control
lane by the data lane). A future nicety: a separate control lane, or a bounded/coalesced data lane, so
control-plane freshness survives data-plane overload.

**Gates:** validation-only session; no source changed, so the workspace gates are unchanged from the v11
track below (265 pass / 0 fail / 4 ignored). The proposed Option-A fix + its regression test are **not
yet applied** — tracked as a follow-up.

---

## v11 CONSOLE-MAP TRACK (plan §12 / design §7.8, §15.33) — DONE (2026-07-24 session, uncommitted)

v11 = v10 + one new normative surface, the **map node**, plus three doc-catch-up tweaks that
describe already-committed behavior (the §5 bulk-copy replay-ring tripwire wording, the §15.30
doctor-P2 `termios_settable` discriminator sharpening, and the §2/§3 map mentions). **The two
doc-catch-up code items were verified already-aligned, no change:** `tap::ReplayRing` is already a
bulk `copy_from_slice` circular `Vec<u8>` (fix #2 from the v10 session), and doctor P2 already gates
on `termios_settable` not `never_opened` (`probes.rs`). The only executable work was **plan §12**.

**§12.1 — THE MAP NODE.** picocom's `--imap`/`--omap` byte mappings as a first-class **interior
transform** (the first *non-codec* one), slotting into the endpoint-keyed wiring (§15.23) with
**zero `Wiring::build` structural change** — purely via `shape()`, exactly as the design promised.
- **Pure engine** `core/src/map.rs` (`#[forbid(unsafe)]`, property-tested): `Mapping` (the 14
  picocom names), `MapDirection` (a compiled 256-entry first-match table + `k×` expansion bound),
  `MapDirection::apply(input, out, on_rule)` — a stateless byte→byte-sequence substitution, first
  match per byte wins, `on_rule` decouples the pure module from the daemon's `Cell` counters.
  Hex form is `[xx]` lowercase; `nrmhex` range is `0x20..=0x7e` (space included); `8bithex` is
  `0x80..=0xff`. Unit + proptests: 256-byte oracle per mapping, first-match ordering, k× output
  bound, chunk-boundary irrelevance.
  **CORRECTION (review 26, MAP-1).** This entry, and `map.rs`'s own module doc, claimed the
  vocabulary was "verified against picocom source". **It was not** — it was taken from the manual
  page, and one rule was wrong: `spchex` was implemented as `b == 0x20` (SPACE), where upstream's
  `M_SPCHEX` arm of `do_map` is `c == '\x7f' || (c < 0x20 && c != '\x09' && c != '\x0a' && c !=
  '\x0d')` — DEL plus every C0 control except TAB/LF/CR. The consequence was two-sided: an operator
  who reached for `spchex` to hunt stray `0x00`/`0x1b` bytes instead had **every space rewritten as
  `[20]`** in the console, its logs, its taps and the web view, while the bytes being hunted still
  passed through invisibly — and since `nrmhex` is `0x20..=0x7e` and `8bithex` is `0x80..=0xff`,
  `0x00..=0x1f` and `0x7f` were **unreachable by any rule in the vocabulary**, so §7.8's "cheap way
  to discover which quirk a mystery console actually has" did not exist. Now fixed against the
  upstream source (each rule transcribed in `map.rs`'s module doc and re-derived independently by
  the 256-byte oracle), which makes the hex family partition the whole byte space.
  *Upgrade note:* a graph whose `hostward`/`targetward` list contains `spchex` **emits different
  bytes after this change** — spaces are no longer hexed, control bytes now are. Anyone who added
  `spchex` to get `[20]` for spaces wants `nrmhex` (which hexes SPACE along with the rest of
  printable ASCII); no rule hexes SPACE alone, because picocom has none. No other mapping changed,
  configuration round-trips unaltered, and the map is stateless, so the new behavior simply takes
  effect at the next `load` with no migration. Same family as §3.14's RESOLV-1 note: a visible
  on-upgrade behavior change, safe and operator-recoverable, worth saying out loud.
- **Config** `core/src/config.rs`: `NodeConfig::Map { name, hostward, targetward, arbitration,
  replay_ring }`; `shape()` = a **host-facing default endpoint** (the mapped side, standard
  lock/fan-out/tap/ring machinery) + a **target-facing `raw` endpoint** (`MAP_RAW_ENDPOINT = "raw"`,
  addressed `node/raw`); `name()`/`replay_ring()`/`arbitration()` arms; `GraphConfig::validate()`
  rejects an unknown mapping name → new `ValidationError::UnknownMapping` (graph.rs), **structural,
  caught before any `--replace` teardown** so a bad map never destroys a good graph. Round-trip +
  proptest cover the new variant.
- **Node runtime** `daemon/src/nodes/map.rs` (`MapNode`, mirrors `codec.rs` but simpler — no
  framing, no fragmentation): a hostward pump (raw upstream → map → mapped fan-out + tap/ring mirror,
  lossy-at-boundary §5) and a targetward pump (consumer writes → map → upstream via `reacquire_held`,
  the §6 held origin). Per-direction `bytes_in`/`bytes_out` + per-rule substitution counters in
  `state_extra` (`Cell` on the runtime thread, `Rc`-shared — no borrow crosses `.await`, no `RefCell`).
  Wired into the `Node` enum (mod.rs).

**§12.2 — REFERENCE CONFIG + DOCS.** `packaging/serial-nexus-daemon.example.toml` gains an active mapped
console (`quirky`/`qcon`: `hostward=["lfcrlf"]` normalizes bare LF, `targetward=["lfcr"]` satisfies
CR). `docs/rpc/configuration.md` documents the map node, addressing, the full picocom vocabulary
table, first-match ordering, and steal-to-bypass; `docs/rpc/observation.md` documents the per-rule /
per-direction state counters; `README.md` architecture table gains a **map** row. Cross-links added.

**⚠️ Adversarial audit (5-lens find→verify workflow, 11 agents) → the 3 core lenses (design-fidelity,
data-plane-invariants, concurrency-lifecycle) found ZERO deviations; 5 findings CONFIRMED, all fixed
or covered; do NOT regress:**
1. **[correctness, the one real fix] a map raw edge that OMITS `write_mode` defaulted to on-demand,
   which the held-origin targetward pump can't drive → parked forever.** Design §7.8 says the raw edge
   "defaults to held". **Fixed** (`runtime.rs Wiring::build`): an omitted/`on-demand` edge whose target
   is a map's `raw` endpoint is **promoted to `held`** (mirroring the log→never override); explicit
   `held` passes through, explicit `never` is preserved for a read-only/display map. Docs updated
   (config.rs `EdgeConfig`/`Map` docs, configuration.md). Regression:
   `map_raw_edge_defaults_to_held_and_maps_targetward_at_volume` (omits `write_mode`, asserts
   holder=`console/raw`, then targetward byte-exact at volume vs oracle + counters — would hang without
   the fix).
2. **[test] fully-deleted-chunk path (the `is_empty()` guards) untested** → `map_deletion_emits_nothing_for_a_fully_deleted_chunk` (`ignlf` deletes a lone `\n`; the device receives only a following survivor, proving the empty chunk emitted nothing while `bytes_in`/`ignlf` advance and `bytes_out` stays 0).
3. **[test] targetward per-rule/per-direction counters never cross-checked** → the steal test now asserts targetward `bytes_in`/`bytes_out`/`rules.lfcrlf`.
4. **[test] targetward byte-exactness only at ~5 bytes** → the new volume test drives 40 varied/LF-dense sends byte-exact vs an independent oracle.
5. **[test] `raw.dropped_slow_consumer` never asserted** → surfaced-and-zero asserted in the volume test; a deterministic >0 firehose is infeasible (the map's hostward output is all-lossy so the pump never stalls on its own), and the counter uses the *identical* shared `DropCounters` machinery proven by `p3_counters`/`p3_exact_loss`, claimed at `map.rs` by one inspection-verified line. The "holdover-bound-under-Busy" finding was **REFUTED** (the map has no holdover slot — it is stateless; the k× proptest bounds interior memory and the design structurally precludes unbounded parking).

**Gates (all green on the Linux 7.0 dev box):** `cargo fmt --all --check`; `cargo clippy --workspace
--all-targets` (+ minimal `--no-default-features`); `cargo deny check licenses bans sources`;
`cargo check --target x86_64-apple-darwin --workspace --exclude serial-nexus-web`; and `cargo test
--workspace --locked` — **265 passed / 0 failed / 4 ignored**. New tests: `itest/tests/p8_map.rs`
(6 tests — 1 cross-platform config-validation + 1 unknown-mapping + 4 serial-device-gated data-plane,
self-skipping on macOS) plus the `serial-nexus-core` map unit/proptests.

**REAL TIER-3 HARDWARE VALIDATION (2026-07-24, two FTDI FT232R adapters cross-wired on the 7.0 box:
`/dev/ttyUSB0` `usb:0403:6001:BH00LL8O:00` ↔ `/dev/ttyUSB1` `usb:0403:6001:BH00L4KU:00`).** `serial-nexus-doctor
--port /dev/ttyUSB0 --port /dev/ttyUSB1` = **15 supported / 0 degraded / 0 unsupported** (P2 supported —
the §15.30 fix holds; P5 certifies the pair both directions, `rate_ladder=true
deliberate_mismatch_observed=true`); baseline passes `expectations/linux.jq`. The full suite run with
`SNX_CROSSOVER_A`/`_B` set = **265 pass / 0 fail** with all four `serial_hardware.rs` rig tests actually
*running* (not skipping): the three pre-existing (byte-exact bidi @115200 + `send` + TIOCEXCL; custom
baud @250000; the signal verbs) **plus a NEW `crossover_rig_map_node_both_directions`** — the v11 map
node driven over the physical wire: its raw edge omits `write_mode` (so it doubles as the **held-default
fix's real-silicon regression** — `port0` holder becomes `console/raw`), targetward `send console` →
`lfcrlf` → `port0` TX → wire → `port1` RX byte-exact against the oracle, and hostward CR-laden `send
port1` → wire → `port0` RX → `crlf` map → the mapped log byte-exact, per-rule counters confirmed. **The
one thing still owed: the map's real-browser visual check** (the web console rendering the mapped stream,
plan §12.2) — an agent can't drive a browser, so it rides the manual checklist (§16.7), like the OPFS
round-trip.

---

## v10 TRACK (plan §11.7–§11.9 + §16.11) — DONE (2026-07-24 session, uncommitted)

v10 = v9 + a normative revision (design §15.32 / §11.7–§11.9 / §16.11) plus doc-catch-up
ADRs §15.30 (macOS contact) and §15.31 (the validation suite became a crate) that describe
already-committed work. The four executable items were built + validated this session, and
validating them on the **real Linux 7.0 dev box with the FTDI crossover rig attached**
surfaced three additional real issues that were fixed at the root (see "Three fixes…" below):
a genuine doctor-P2 regression on `main`, a throughput regression the new default-on rings
introduced, and two over-specified/racy tests. The full suite is green after.

**§11.7 — DEFAULT-ON, PER-CHANNEL REPLAY RINGS.** `replay_ring` now defaults to **64 KiB**
(`serial_nexus_core::config::DEFAULT_REPLAY_RING`) on *every* host-facing endpoint, opt out with
`0`, superseding the serial-only opt-in default and the codec/leg scoped deferral. New
`replay_ring` field on `Codec` and `Leg` node configs (serial's default flipped); a
`NodeConfig::replay_ring()` accessor. `runtime::Wiring::build` sizes `host_ring_cap` for
**every host-facing endpoint** from its owning node's value (target-facing endpoints get
none — inert on a serial output leg, a sending leg's channels, a demux's multiplexed side).
The §5 accounting doctrine still holds: the tap/ring mirror is a spy *outside* the graph, so
`discarded_unattached` is independent of it (regression `active_tap_feed_does_not_hide_unattached_loss`
stays green). The ring is a **bulk-memcpy circular `Vec<u8>`** (`tap::ReplayRing`) — see fix
#2 below for why a byte-by-byte `VecDeque` is forbidden here now that the ring is on the hot
path of every endpoint. New guard:
`p8_replay_ring::default_ring_on_a_codec_channel_splices_exactly` (race-free: replay tap opened
after the stream drains, ring == last 64 KiB of the channel log, `feed_dropped == 0`).

**§11.8 — TAP OFFSETS + INSTANCE NONCE.** `TapHub` tracks a monotonic `ingested: u64`;
`TapMsg` and the `tap.data` notification carry the hostward byte `offset` of the chunk's
first byte; `tap.open` returns `from_offset` (the ring's oldest byte under `--replay`, else
the live edge); replay pieces walk `piece_off` (advancing even past a dropped piece, so a
gap is visible not silent). `register()` returns `Registered { replay_bytes, from_offset }`.
`from_offset = self.ingested − snap.len()` never underflows — every `ingest` pushes the same
chunk to the ring *and* advances `ingested`, so `snap.len() ≤ ingested` invariantly. `info`
gains a per-boot `instance` nonce (`daemon.rs::new_instance_nonce()`, a `RandomState`-seeded
u64 mixed with pid + nanos — no new dependency); it changes across restarts so a client
detects the offset reset. New tests `p8_tap_offsets.rs`
(`reconnecting_tap_reconstructs_stream_exactly_once_by_offset` — a raw-socket client folds two
replays by offset into a byte-exact stream, proving overlap-trim; and the instance-nonce
stable/changes test). `p5_info` asserts `instance`; `docs/rpc/observation.md` documents the
new fields + a Taps section (taps were previously undocumented in `docs/rpc/`).

**§11.9 — BROWSER-SIDE OPFS HISTORY.** New pure ES module
`web/src/assets/history.mjs` (offset-splice frontier + 16 MiB trim-oldest
retention, DOM/storage-free), unit-tested by `history.test.mjs` (`node --test`, 9 tests) and
gated by `itest/tests/p8_web_history.rs` (self-skips without `node`). Thin OPFS adapter
`opfs.mjs` (per-`(origin, endpoint, instance)` key, 8-byte end-offset header + capped bytes,
`navigator.storage.persist()` status surfaced, memory-only fallback). `app.js` became an ES
module wiring history + OPFS: restores stored scrollback before the ring replay, splices live
by offset (no reload duplication), debounced persist, export/clear controls, a storage badge.
`index.html` (module script + toolbar), `app.css`, `assets.rs` (serve the two `.mjs`).
`p8_web::web_http_security_gates` now also asserts the modules serve 200. **The OPFS adapter
itself is browser-only and rides the manual/hardware checklist (§16.7) — the *logic* that
must be correct is the node-tested pure module; a real-browser drive is still owed at the
checklist.**

**§16.11 — BASH RETIRED.** The last three shell scripts are folded into `serial-nexus-itest` and
`scripts/` is **deleted**: `p0_license_gate.rs` (plants `serialport`, asserts cargo-deny
rejects it; self-skips without cargo-deny), `p8_external_codec.rs` (now *builds* the excluded
template from the consumer position + runs its conformance kit, then drives `acme-daemon` over
RPC — lifting the batch-2b "may not invoke cargo" constraint), and `wait-for.sh` → `wait_until`.
CI (`ci.yml`): `license-gate`/`external-codec` jobs repointed to the folded tests. Sim doubles
stay *subprocesses* deliberately (recorded evaluated-and-kept).

**Three fixes surfaced by running the full suite on the real 7.0 dev box (`pwnblet`) with
the FTDI crossover rig attached — NOT environment artifacts, real issues:**

1. **Doctor P2 was a genuine regression from the macOS commit `d1d8520`, fixed here.** That
   commit added `&& o.never_opened` to P2's `Supported` gate, on the false premise (stated
   in its own comment) that "Linux HUPs a never-opened master." It does not — §3.2 and this
   box both show `hup_when_never_opened = false`, which is exactly why the PTY node primes
   the slave. So the change silently demoted **native Linux** from `Supported` to `Degraded`,
   failing `meta_gates::doctor_reports_no_unsupported_capability` and `expectations/linux.jq`
   (which *requires* P2 supported) on every real Linux — it was masked only because that
   session ran on macOS and *assumed* Linux (the §15.30 "predicted ≠ verified" trap, ironically
   reintroduced by the §15.30 fix). **Fix** (`probes.rs`): `never_opened` no longer gates the
   verdict (priming handles it on every platform); the real Linux-vs-BSD split is
   `termios_settable` — Linux master is a terminal → `Supported`; macOS master is not
   (ENOTTY) → `Degraded`. Both `linux.jq` (P2 supported) and `macos.jq` (P2 supported-or-degraded)
   now pass. This was a real pre-existing bug on committed HEAD, unrelated to v10 features but
   found while validating them.
2. **Default-on rings had a throughput regression; fixed at the root, not hidden.** The
   original `ReplayRing` used a `VecDeque<u8>` with byte-at-a-time `drain`+`extend` — ~128 KiB
   of per-byte churn per 64 KiB chunk, on the runtime thread, on *every* endpoint now that
   rings default on. It starved the runtime thread and collapsed the 256 MiB firehose from
   passing to ~1.9 MB/s (blew the 60 s bound). **Fix** (`tap.rs`): a fixed circular `Vec<u8>`
   ring — `push` is two bulk `copy_from_slice`s. Firehose now completes in **2.5 s**, honoring
   §15.32's "re-verified against the §15.19 throughput bar" instead of opting the benchmark out.
3. **Two over-specified/racy tests hardened** (the added ring load raised their flake rate under
   full-suite parallelism; each fix is design-faithful, not a mask): `Rpc::shutdown` is now
   RST-tolerant (a `shutdown` verb inherently may not flush a response before the process exits
   — a pre-existing race that flaked both ways); and `p3_counters::pty_no_client`'s
   `dropped_slow_consumer == 0` became "presence-gated discard dominates" (§5 requires loss be
   *counted*, not that a firehose never overflows a bounded buffer). The earlier `replay_ring = 0`
   opt-out on that test was reverted — it runs on the real default-on config now.

**Gates (all on the real Linux 7.0 box + FTDI FT232R crossover rig, `/dev/ttyUSB0 ↔ ttyUSB1`):**
`cargo fmt --all --check`, `cargo clippy --workspace --all-targets`(+minimal), `cargo deny check`,
`cargo check --target x86_64-apple-darwin --workspace --exclude serial-nexus-web` (the `ring`/rustls
dep can't cross-build from Linux — pre-existing; the real macOS gate is `cargo test` on a Mac),
and `cargo test --workspace --locked` — **all green, confirmed across repeated runs.** On the
physical rig: `serial-nexus-doctor --port /dev/ttyUSB0 --port /dev/ttyUSB1` reports **15 supported / 0
degraded / 0 unsupported** (P2 now `supported`; P5 certifies the pair both directions — custom
baud, break, TIOCGICOUNT, bidirectional rate ladder, deliberate-mismatch observed), and the
`serial_hardware::crossover_rig_data_plane_send_and_exclusivity` test drives the daemon through
the physical crossover byte-exact both directions with the `send` verb reaching hardware and
TIOCEXCL enforced. Committed on `implementation`; **no `main` merge without an explicit ask.**

**For the next validating session (esp. on macOS, without these notes' git context):** the four
v10 features plus the three fixes above are all in this change set. On macOS specifically —
doctor **P2 is expected to be `degraded`** (BSD master isn't a terminal; `macos.jq` accepts it),
serial-*device* itest tests self-skip (pts ≠ serial device; the real path is `serial_hardware.rs`
via `/dev/cu.usbserial-*` or `SNX_CROSSOVER_A`/`_B`), the `--target x86_64-apple-darwin`
cross-check is unnecessary (build natively), and `p8_web_history.rs` needs `node` (self-skips
without it). The browser OPFS round-trip (`app.js` + `opfs.mjs`) is the one thing not covered by
an automated test — drive it in a real browser as the manual-checklist step (§16.7).

---

## macOS runtime validation + PTY fix + Rust test-harness migration — 2026-07-24 session

**First hands-on macOS pass on real hardware** (the §13 roadmap "verified" milestone). Ran the
whole workspace on **macOS 15.7.8 / Darwin 24.6.0 (x86_64)** with two FTDI FT232R adapters
cross-wired as a null modem (`/dev/cu.usbserial-BH00L4KU` ↔ `…-BH00LL8O`).

**Now runtime-verified (was only "cross-checked"/"expected"):** `cargo build/test/fmt/clippy`
(incl. `--no-default-features` and `--target x86_64-apple-darwin`) all clean; **156 tests pass,
identical count to Linux**. Serial data plane on the real crossover cable: **32 KiB byte-exact
both directions**, `send` verb reaches hardware, **a second open of a held port is refused**
(TIOCEXCL is set unconditionally; on macOS `cu.*` is single-open at the driver layer regardless,
so the refusal is real but not attributable to the ioctl alone — see the follow-up note below),
driver counters gracefully absent (TIOCGICOUNT is Linux-only). `serial2` opening `/dev/cu.*` works.

**Two real macOS defects — both contradicted docs/macos.md's *predicted* verdicts — FOUND and FIXED:**

1. **PTY nodes faulted at creation: `tcgetattr: ENOTTY`.** The §7.2 baseline termios was applied
   through the pty **master**, which BSD rejects (the master is not a terminal there); the entire
   `pty` node type was non-functional on macOS. **Fix — cfg-gated, Linux path byte-for-byte
   unchanged** (`nodes/pty.rs::with_termios_fd`): on non-Linux the baseline/reconcile run through a
   momentarily-opened **slave**. Two further macOS pty facts, both handled:
   - **The slave's termios resets to cooked on last-close** (verified with a Python `openpty`
     probe: a daemon-side set does not survive to the client's open). So the baseline is
     re-asserted on the presence **rising edge** — the client then holds the slave, keeping the
     raw/echo-off/EXTPROC setting alive for the session. There is a poll-latency window before the
     re-assert, consistent with the "poll-only observation" macOS story (§7.2); a client that sets
     its own raw termios (all interactive clients, and the sim `client`) is unaffected.
   - **A never-opened master does not POLLHUP** — already handled by the existing `prime_slave`;
     the node simply never reached it before (it faulted on the master `tcgetattr` first). Post-fix,
     `client_present` true/false transitions are correct.
   Verified end-to-end over real hardware: sim client → pty → serial → crossover → serial → log,
   **byte-exact both directions**.

2. **Doctor P2 (POLLHUP presence) reported `unsupported`, failing `expectations/macos.jq`.** The
   probe treated macOS's `hup_when_never_opened=false` + `termios_settable_without_slave=false` as
   unsupported. But presence *works* via priming + slave-termios (proven). **Fix** (`probes.rs`):
   P2 now reports **`degraded`** when the core presence signals (`hup_after_close && !hup_while_open
   && !hup_after_reopen`) hold but the kernel needs the §7.2 platform arm. `macos.jq` now passes
   (summary: 0 unsupported); Linux stays `supported`.

**serial-nexus-sim — same master-termios ENOTTY.** `apply_raw_pair` is a **no-op on BSD/macOS**: the
consumer configures the slave (the daemon's serial node via serial2, or the sim `client`), and
opening the slave to set termios would prime POLLHUP → the echo/source/sink loops read that as
"client hung up" and exit early. `set_raw`/`termios_of_pair` cfg-gated to match.

**Discovered macOS test-infra limitation (NOT a product bug): a pty cannot stand in for a serial
device.** `serial2::SerialPort::open` on a macOS **pts** returns `ENOTTY` (it sets baud via a
macOS-specific ioctl a pty rejects). The Linux "no-target doctrine" (a pty as a fake serial device)
therefore does not run on macOS — serial-*device* tests there need **real hardware or must skip**.
Real UARTs are unaffected (proven byte-exact). See `docs/macos.md`.

**Rust test-harness migration (replacing the bash `scripts/validate/**`, per operator request).**
New **`serial-nexus-itest`** crate (`publish = false`, workspace member): a cross-platform harness that
boots `serial-nexus-daemon`, drives it with a small in-Rust JSON-RPC client (replacing `serial-nexus-ctl
--json | jq`), orchestrates `serial-nexus-sim` doubles as subprocesses, and asserts on structured results
+ byte-exact SHA-256 — none of the `stat -c` / `nc -q` / `sha256sum` / `timeout` /
`/dev/serial/by-id` bash portability hazards (all of which break the old scripts on macOS).
`serial_echo()`/`serial_pair()` yield a serial device on Linux (sim pty doubles) and `None`
elsewhere; `crossover_ports()` yields the real crossover rig — `SNX_CROSSOVER_A`/`_B` on any
platform, plus a `cu.usbserial-*` scan on **macOS only**. Otherwise `None` → the test
self-skips, the §5 skip discipline. (This sentence named a `serial_rig()` that no longer
exists and read as though `crossover_ports()` detected a Linux rig on its own; it does not —
review 37 `37-DOC-3`.) **Verified on macOS: 6/6** — `tests/control_plane.rs` and `tests/serial_hardware.rs`.

**Real-hardware validation (macOS, two FTDI FT232R adapters cross-wired).** `serial_hardware.rs`
(one `#[test]`, auto-detected via `crossover_ports()`, self-skips when absent) certifies end to
end through the daemon + the macOS-fixed PTY injector: **bidirectional 32 KiB byte-exact** across
the physical wire (each way, SHA-256), the `send` verb reaching hardware on the far port, and
**TIOCEXCL** exclusivity (a second open of a daemon-held port is refused). Confirmed green on the
rig `/dev/cu.usbserial-BH00L4KU ↔ …BH00LL8O` (~6.9 s of real serial transfer); driver counters are
gracefully `null` (TIOCGICOUNT is Linux-only). This is the macOS serial gate — a pty cannot be a
serial device there, so every other serial test self-skips and this exercises the real path via the
daemon's own fast, lossless reader.

**Follow-up: extended rig coverage + license gate + TIOCEXCL precision (same 2026-07-24 rig).** A
post-validation multi-agent adversarial audit confirmed the core green is non-vacuous and surfaced
runnable-but-unexercised coverage; commit `2c01170` closes the two agent-runnable gaps by adding two
real-rig tests to `serial_hardware.rs`:

- `crossover_rig_custom_baud_byte_exact` — byte-exact both directions at **250000** (32 KiB each way,
  SHA-256), proving the FTDI actually *clocks* a high custom rate on the wire (P3 only proves the
  driver stores/reads the divisor back). 9600 is deliberately omitted: the one-shot `Sim::client`
  closes the injector pty on exit and races the slow drain (saw 3072/4096 B) — a **test-harness
  artifact, not a targetward drop** (invariant #3 is guarded by
  `targetward_oversize_chunk_is_fragmented_never_dropped`; a held-open direct-write probe was itself
  flawed — cooked-mode pty `OPOST`/`ONLCR` corrupts binary data, so the injector must set raw mode).
- `crossover_rig_signal_verbs` — `send-break`/`set-modem`/`pulse-dtr` driven end-to-end through the
  daemon against a real UART (the Tier-3 property §13 defines the null modem to test; `p7_signals`
  reaches only a pts, which `ENOTTY`s set-modem/pulse-dtr). Break is observed far-end as a **NUL**
  (best-effort; deterministic frame-error detection needs Linux-only TIOCGICOUNT, so not asserted).

All three rig tests share a process-wide `RIG` mutex (poison-tolerant `into_inner`) so they never
contend the two physical ports under the default parallel harness. Shared `null_modem_cfg` /
`inject_verify` / `boot_rig` helpers. Full suite now **248 pass / 0 fail / 4 ignored**; fmt+clippy clean.

**Unplug→replug heal validated manually on the rig** (an agent can't pull a cable): a `serial-nexus-daemon`
holding both ports via `serial-nexus-ctl` — pull one adapter → its node → `waiting (device … lost)`
while the other stays `active` and the graph is unchanged (config-vs-state split, invariant #7);
replug → `active` in ~1 s at the **same stable `cu.usbserial-*` path** (termios + modem lines
reapplied); a `send` nonce then crossed the healed wire. This is the §7/§12 "survive replug" property
on real macOS hardware — the Linux `p7_replug`/`p7_unplug` self-skip there. `purged_on_reconnect`
stayed 0 (no in-flight bytes to purge), which is correct.

**License gate now PROVEN, not just configured** (it had been silently self-skipping): with
`cargo-deny` installed, `cargo deny check licenses bans sources` is clean and `p0_license_gate`
(plants a banned crate, asserts cargo-deny rejects it) passes — the permissive-only policy is
demonstrated, not assumed.

**TIOCEXCL precision (audit finding, docs corrected).** The daemon sets `TIOCEXCL` unconditionally
(`nodes/serial.rs`) and a second open of a held port is genuinely refused, but macOS `cu.*` call-out
devices are single-open at the driver layer regardless — so on macOS the refusal is real yet **not
attributable to the ioctl alone** (doctor P3 confirms the ioctl is *accepted*, not that it is what
refuses the open). The clean isolation — a second open that succeeds without TIOCEXCL and fails with
it — is a Linux-rig property. `docs/macos.md` and the macOS-session summary above were reworded from
the earlier "TIOCEXCL enforced" to the accurate "a second opener is refused" + this nuance.

**Migration COMPLETE.** All of phases 0–8 (58 bash scripts) are ported to 43 `itest/tests/*.rs`
files (**83 tests**, 1 `#[ignore]`d endurance soak), across three batches (0–4, 5–6, 7–8), each
compiling + clippy/fmt-clean and **green on macOS** (serial-*device* tests self-skip; codec/exec/leg/
tap/**web** tests run there). The 55 retired scripts are deleted; only three genuine tooling files
survive — `phase0/license-gate.sh` (cargo-deny gate), `phase8/external-codec.sh` (out-of-tree template
build), and `scripts/lib/wait-for.sh` (used by the latter). **CI switched:** `check`/`macos` now run
`cargo test --workspace` (the integration suite included); the `macos` lane runs the full suite and
gates on `expectations/macos.jq`; `harness-lint` (shellcheck/jq-lint) and the script-driven
`integration` job are gone; the nightly lanes run `cargo test … --ignored`/`--include-ignored`.
Key foundation decisions: serial providers are Linux-sim + lossless (a raw high-volume read over a
flow-control-less UART drops bytes — that byte-exactness lives in `serial_hardware.rs` via the
daemon's reader); the RPC verb→params shapes come from `serial-nexus-ctl::build_request` (`load` is
`{config, replace}`, not a path). `Cargo.lock` updated for the new member. AGENTS.md §5 rewritten.

---

**v9 REVISION + WEB CONSOLE TRACK (plan §11) — DONE (2026-07-23 session).**
v9 = v8 + a new normative surface: §5 gains **the replay ring** (`replay_ring = <bytes>`
on a host-facing endpoint, graduated from §14), §6/§10 gain **taps** (`tap.open`/
`tap.close`, the `never` write mode in dynamic form), and new §15.28/§15.29/§17 spec the
**web console client** (`serial-nexus-web`). Everything else in v9 (§5 fragmentation
invariant, §6 purge-to-quiescence + acquisition-time held-priority, §7.3 log write_mode,
§10 EOF-cancel, §12 identity spelling, §15.27) is **doc-catch-up describing the
already-committed Opus review remediation** (`b9d8a50`).

**Design-alignment check (v8→v9 diff, workflow-audited): 2 real deviations in committed
code, BOTH FIXED by aligning code to the design:**
1. **§5 "one shared helper" for targetward fragmentation** — the in-process codec
   (`codec.rs channel_targetward`) reimplemented the fragmentation-bounds loop inline
   (it frames via the pluggable `c.mux`, not the envelope `encode` that the old
   `data_frames` hardcoded), so "via the one shared helper" was literally false for the
   3rd framer. **Fixed:** extracted `runtime::frame_ranges`/`frame_payload_cap` (the
   error-prone cap+range math §15.27 cared about); `data_frames` (leg+exec) and the codec
   now BOTH fragment on that one shared helper. No-drop invariant already held; this makes
   the design true. Guard: `targetward_oversize_chunk_is_fragmented_never_dropped` +
   exec-crash 256KiB round-trip stay green.
2. **§12 whitespace identity field** — `resolve_usb_identity` rejected empty fields but
   not whitespace-only (`usb:0403:6001: :00` was accepted, would never match sysfs →
   permanent `waiting`). v9 §12 says "empty or whitespace-only fields are malformed at add
   time". **Fixed:** `f.trim().is_empty()`; test extended with two whitespace cases.

**§11.1 THE TAP + §11.2 THE REPLAY RING — BUILT + VALIDATED.** New `daemon/src/
tap.rs`: `TapHub` (per host-facing endpoint, `Rc<CriticalCell<>>`), `Tap`, `ReplayRing`,
`TapFeed`, `TapMsg`, `OpenTap`. **Architecture (the crux was the blocking-thread serial
reader owning its `Vec<HostwardSink>` — no shared mutable fan-out):** each host-facing
producer mirrors hostward chunks into a per-endpoint **tap-feed** channel (a runtime-thread
hub task drains it → `TapHub::ingest`), gated by an `Arc<AtomicBool> active` flag so an
untapped, ring-less endpoint pays only one relaxed atomic load per chunk ("costs nothing
when unset", §5). The tap-feed is **excluded from the serial reader's `any_live`/
`discarded_unattached` accounting** (a spy tap never masks a real consumer's absence).
Taps are connection-scoped: `serve_connection` owns one bounded per-connection channel
(`TAP_QUEUE_CAP=128`, the §5 boundary — a stalled tab fills it → hub drops-with-counter,
per-tap), intercepts `tap.open`/`tap.close`, streams `tap.data` (base64) notifications, and
`OpenTap::drop` detaches the tap from its hub on `tap.close` or connection drop (prompt even
on an idle endpoint). **Exact-splice** (`--replay`): `register` snapshots the ring and
queues it into the connection channel *before* adding the tap to the fan-out, and because
`ingest`+`register` are both synchronous `hub.with_mut` critical sections on the one thread,
no live chunk can interleave — ring-then-live is a contiguous suffix, no gap/dup (§15.20
doing double duty). `replay_ring` config landed on the **serial** node; codec/leg host
channels get a hub (tap works) but per-channel ring config is a scoped deferral. Hub tasks
self-terminate when the producer's feed sender drops (teardown/remove/replace); `state`
reports open taps `{tap,endpoint,dropped}`; `dump` is untouched (taps are state, §8).
`serial-nexus-rpc` gained tested `base64_encode`/`base64_decode`; `serial-nexus-ctl tap <endpoint>
[--replay] [--bytes N] [--stall-ms N]` (holds the write half open, §15.20); `serial-nexus-sim pty
--source` gained `--wait-file` gating (presence!=readiness). Producers touched: serial,
codec, exec, leg (all mirror; pty/log are target-facing → no tap).

**Gates:** `cargo test --workspace` = **154** (was 149), fmt/clippy(all-targets) clean.
New e2e (all pass, in `phase8/`): `tap.sh` (tap==co-attached-log==source byte-exact;
dump-unaffected; connection-drop detach), `tap-drops.sh` (unread tap dropped 6.5MB while
the log stayed byte-exact — a slow tab costs only itself), `replay-ring.sh` (exact splice:
replay+live == a contiguous suffix of the stream; empty-replay marker replay_bytes=0 on a
ring-off/empty endpoint; `replay_ring` round-trips dump/load). Regression-checked green:
phase3 counters/exact-loss/log, phase5 demux/exec-crash, phase6 reference/head-of-line.

**⚠️ Adversarial audit of the tap/ring code (6-dimension workflow) → 3 confirmed, ALL
FIXED; do NOT regress:**
- **[MED] a configured `replay_ring` silenced `discarded_unattached`** (serial reader).
  The tap-feed's `active` flag is set whenever a ring is configured *or* a tap is open,
  and the reader gated the discard counter on `!tap_wanted` — so a ring alone (no tap,
  no consumer) hid the loss of everything beyond the ring depth (§5 "loss always
  visible"; the ring "never substitutes for a log node"). **Fixed:** the tap/ring mirror
  is a spy *out of the graph* with its own accounting; `discarded_unattached` now counts
  graph-consumer-absence independent of the mirror, matching codec/exec/leg. Regression:
  `active_tap_feed_does_not_hide_unattached_loss` (SERIAL-4).
- **[LOW] `TapFeed::mirror` dropped on a full feed uncounted** — §5 wants all loss
  counted. **Fixed:** a per-endpoint `feed_dropped` atomic (shared TapFeed↔hub),
  incremented on the `try_send` Full, surfaced in `state.taps[].feed_dropped`. Still
  never backpressures (the ring must not stall the device).
- **[LOW] `serial-nexus-ctl tap --bytes 0` swallowed the ack** — the `while written<limit`
  loop never ran, so a failed open (unknown endpoint) exited 0. **Fixed:** read the
  tap.open ack *before* the byte loop; `--bytes 0` is a confirmed no-op, a failed open
  exits non-zero.

**§11.3–§11.6 THE WEB CLIENT — BUILT + VALIDATED.** New crate **`serial-nexus-web`** (a
pure RPC client of the daemon; the daemon gains no HTTP, §17). **Deps (all §13-permissive,
`cargo deny check licenses bans sources` green):** `tokio-tungstenite` (WS framing),
`sha1` (WS accept), `getrandom` (token), and for the TLS tier `rustls`+`rcgen` pinned to
the single **`ring`** backend (no aws-lc, no rustls-pemfile). HTTP is hand-rolled on tokio
(§15.13 ethos); tungstenite does only post-handshake framing (`from_raw_socket`); the
server is generic over the stream so TLS and plaintext share one path. **§15.29 security:**
a 256-bit bearer token gates every request as a `SameSite=Strict` session cookie (set by
the `?token=` bootstrap URL, then dropped from the address bar); the Host header is
validated on every request (DNS-rebinding defense, even on loopback); **three bind tiers**
— loopback+token (default), `--tls`+token (rustls, self-signed on first run for lab use,
key mode 0600), `--insecure-bind` (named footgun, warns). **The bridge is a filtering
JSON-RPC proxy** (`bridge.rs`): one daemon UDS connection per browser WS, auto-subscribes,
relays both ways, and **denies graph/lifecycle verbs** (`load`/`add-node`/`remove-node`/
`teardown`/`shutdown`/`connect`/`disconnect`/`set-attribute`) so the web console can never
mutate the graph (§17 non-goal). Frontend = embedded static `index.html`/`app.js`/`app.css`
(functional console: left rail from state+subscribe, tap terminal with replay marker + drop
counter, send box with holder-named LOCKED + explicit-steal). `serial-nexus-web wsclient`
(headless: `--endpoint`+`--bytes` taps and checksums; `--rpc`+`--params` one-shots) is the
validation client. `serial-nexus-rpc` gained shared `base64_encode`/`base64_decode`. `docs/
security.md` gained the web section (token/Host/three tiers, token-is-not-TLS).

**Gates (final):** `cargo test --workspace` = **156**, fmt/clippy(all-targets)/cargo-deny
(licenses+bans+sources)/macOS-cross-check clean. New e2e in `phase8/` (all pass; run in
`all.sh --through 8` / nightly): `tap.sh`, `tap-drops.sh`, `replay-ring.sh`, `web.sh`
(401/403/302/200 gates + non-loopback-refused + WS byte stream browser→device byte-exact +
console-list-matches-state + denylist-through-WS + TLS curl `--cacert` round-trip +
untrusted-cert-rejected + non-loopback-permitted-with-`--tls`). ⚠️ web.sh is slow (~90s:
4 server instances + cert gen + a 256KiB WS byte stream). ALL SIX plan §11 items done.
NOT committed; no `main` merge.

---

**v8 REVISION + EXTENSION TRACK (plan §10) — DONE (this session).** v8 = v7 with a
new normative extension surface — **ADR §15.26** ("out-of-tree codecs: embed the
daemon, don't load plugins"), §8 rewritten to registry-as-value, §10 gaining the
`info` verb — whose executable form is the NEW **plan §10** (five items). None of it
existed in the code; all five are now built + validated + adversarially audited.
(The v8 §16 dispositions reverted to "(adopt)" phrasing vs v7's "(done)"; that is
annotation only — plan §9 remains built, no code change there.)

1. **Library/binary split + registry-as-value (§10.1).** New crate **`serial-nexus-daemon`**
   (library) holds every former `serial-nexus-daemon` internal (`git mv` of boundary/cell/
   control/daemon/nodes/runtime + new `lib.rs`/`registry.rs`); `serial-nexus-daemon` is now a
   ~dozen-line binary that parses flags, installs tracing, and calls
   `serial_nexus_daemon::run(RunOptions, Registry)`. The codec registry is a **value**:
   `Registry::with_builtins().register(name, factory)` — a factory is
   `Rc<dyn Fn(&toml::Table)->Result<Box<dyn Codec>,String>>`; a **duplicate or
   reserved (`exec`) name is a startup error**. Public API is exactly `{run, RunOptions,
   Registry, CodecFactory, RegistryError, VERSION, WIRE_VERSION, ENVELOPE_VERSION}` —
   verified with `cargo doc` (every internal module is private). `Daemon` gained an
   `Rc<Registry>`, threaded into `Node::instantiate` in `load`/`add-node`.
2. **`info` verb (§10.2).** `{daemon_version, wire_version, envelope_version, codecs}`;
   `serial-nexus-ctl info`. An **unknown codec is a structural error** carrying
   `data.available`. (Fix #1 below extended this to codec *attribute* schemas.)
3. **External-consumer template (§10.3).** `examples/external-codec/` (workspace-
   excluded, its own workspace): `acme-codec` (against `serial-nexus-codec-api` only) + `acme-daemon`
   (a custom binary against `serial-nexus-daemon`). Built from the consumer position by
   `scripts/validate/phase8/external-codec.sh` + a per-push CI job.
4. **Conformance kit (§10.4).** `serial-nexus-codec-api` `test-support` feature →
   `serial_nexus_codec_api::test_support`: `round_trip_identity` / `fragmentation_tolerance` /
   `handles_garbage` / `bounded_parser_state` / `assert_buffer_bounded`. Reference
   codec + acme run it; four deliberately-broken codecs prove each suite bites.
5. **Exec-conformance harness (§10.5).** `serial-nexus-sim exec-conformance` (an `ExecChild`
   with a concurrent stdout-decoding thread): golden vectors, **full-duplex liveness**
   (the §15.22 deadlock class), fragmented reassembly, kill/restart. Fixtures
   `tests/ext-codec/{passthrough.py (pass), lag.py (bounded-lag, pass), half-duplex.py
   (fail)}`.

Docs: `docs/rpc/observation.md` (info verb), `docs/rpc/configuration.md` (unknown-codec
`data.available`), `docs/codec-authors.md` (registry-as-value + embedding + kit +
exec-conformance), `docs/rpc/README.md`. CI (`.github/workflows/ci.yml`): a per-push
`external-codec` job, extension gates in the integration lane, and a **minimal-build
clippy** (`--no-default-features`) step.

**⚠️ Adversarial audit (5 dimensions) found 6 confirmed (5 distinct), ALL FIXED; do
NOT regress:**
- **[MED] `load --replace` destroyed a good graph on a KNOWN codec's bad *attributes*.**
  `codec_unknown_error` only caught unknown *names* before teardown; a bad attribute
  table for a registered codec was caught inside `instantiate` (after teardown). **Fixed:**
  `Daemon::precheck_codecs` validates every codec node's name AND attribute schema
  **purely** (`registry.build` / `exec::parse_attributes`, discarded) **before** teardown,
  in both `load` and `add-node`. Bad codec attrs are now structural (`-32002`), graph
  preserved under `--replace` (verified: state stays `[console]`).
- **[MED] `bounded_parser_state` was a false-negative** — it only summed *emitted* bytes,
  so a non-resyncing `while let Ok(Some(..)){}` accumulator that hoards undecodable input
  (unbounded §5 buffer) PASSED all four trait-only suites. The trait can't see internal
  buffers, so **fixed** by (a) honest docs on `bounded_parser_state` and codec-authors.md,
  and (b) a new `assert_buffer_bounded(make, buffered_fn)` that feeds a `0xFF` oversize-
  prefix blob and asserts the reported buffer stays ≤ `MAX_FRAME_SIZE` — catches the
  `Hoarder` (negative test), passes resyncing codecs; wired into the reference codec's kit
  test.
- **[MED] exec-conformance liveness/restart falsely FAILED a valid bounded-lag codec.**
  The old lock-step (send frame N, block for echo N before N+1) deadlocks against a codec
  that emits one frame behind. **Fixed:** liveness now sends the whole pipeline, requires a
  majority of echoes to flow *before* EOF (catches half-duplex: 0 echoes), then closes
  stdin and requires an exact in-order match (bounded-lag flushes its tail). `restart`
  closes stdin before requiring the echo. Regression fixture `lag.py` (1-behind) now passes;
  `half-duplex.py` still fails liveness.
- **[LOW] `Registry::with_builtins()` `unused_mut` under `--no-default-features`** broke the
  §8 minimal build's `-D warnings`. **Fixed:** `#[cfg_attr(not(feature="codec-reference"),
  allow(unused_mut))]` + a CI minimal-build clippy step.
- **[LOW] `codec-authors.md` linked to moved source paths** (`daemon-bin/src/nodes/…`).
  **Fixed** to `daemon/src/{nodes/exec.rs,registry.rs}`.
0 findings refuted. Gates after fixes: `all.sh --through 8` = **48/48** (45 prior + info/
exec-conformance/external-codec), fmt/clippy(+minimal)/macOS-cross-check/shellcheck clean.
Not committed; no `main` merge.

---

**Post-1.0 simplification track — DONE (design §16 / plan §9).** All seven items
executed as seven commits on `implementation`, each behavior-preserving item
adversarially re-audited before commit. Final state: **102 unit/property tests**,
`all.sh --through 8` = **45/45** (the original 42 + the new unsafe-gate, jq-lint, and
harness self-test), fmt/clippy/`--target x86_64-apple-darwin`/shellcheck all clean.
- **§9.1 boundary-supervisor library** (`214e237`, §16.1). New `serial-nexus-daemon::boundary`:
  `park()` (park-don't-teardown), `race3` (concurrent halves — a *flat* 3-arm `select!`),
  `Backoff::{exponential,fixed}`, `BlockingReader` (loss-notify + join-then-transition).
  serial/exec/leg rebased onto it. The 3-lens audit caught a real medium bug — race3 was
  first drafted as nested `race2`, which biases the tie-break when two halves are ready in
  one poll (a spurious respawn on a teardown/crash race) — fixed to a flat select; plus a
  `fixed(0)` floor divergence. 8 boundary tests.
- **§9.2 critical-section cell** (`362a11e`, §16.2). `serial-nexus-daemon::cell::CriticalCell`
  (closure-only `with`/`with_mut`) replaces **every** `RefCell` in serial-nexus-daemon (daemon
  state, `LockCell`, all node shared cells); `daemon-bin/clippy.toml` bans
  `std::cell::RefCell` via `disallowed-types` (per-crate scoping via `CARGO_MANIFEST_DIR`,
  confirmed on clippy 0.1.97). The "borrow never crosses `.await`" tripwire is now a
  compile-shape fact. Audit clean. Gate proven (clippy fails on a planted RefCell). 3 tests.
- **§9.3 serial-nexus-sys crate** (`052fb8a`, §16.3). New `serial-nexus-sys` = all unsafe (ioctls,
  ptsname, poll); daemon/doctor `sys.rs` deleted, sim's local unsafe removed; every other
  crate now `#![forbid(unsafe_code)]`. `scripts/validate/phase0/unsafe-gate.sh` proves
  confinement (detector-proven). doctor `read_icounter`/`SerialIcounter` → canonical
  `read_icounts`/`SerialIcounts`. macOS cross-check clean.
- **§9.4+§9.5 harness + CI hardening** (`7f097e0`, §16.5). `scripts/lib/assert.sh` (tested
  helpers; the loss-counter check with correct `(add // 0) == 0`), `phase0/harness-selftest.sh`
  (feeds a nonzero counter, asserts the helper *fails* — the anti-tautology regression),
  `phase0/jq-lint.sh` (compiles .jq files + greps the `// N ==` antipattern), `.shellcheckrc`
  + **shellcheck green** across scripts/. soak.sh uses the tested helper. CI `harness-lint`
  (per-push) + `sweep-nightly` (full `--through 8`, archives the verdict JSON). `all.sh`
  gained `--json-summary`.
- **§9.6 state-file fsync** (`f129a2f`, §16.6). `atomic_write` fsyncs temp before rename +
  dir after (strace-confirmed `fsync→rename→fsync`); comment-pinned test; crash-recovery
  script stays green.
- **§9.7 error-code registry** (`0756022`, §16.8). `serial_nexus_rpc::AppError` enum = single
  registry; daemon `app_errors` re-exports its `.code()`; `error_code_registry()`; test
  `docs_rpc_table_matches_the_registry` asserts docs/rpc ↔ registry (catches undocumented
  or unregistered codes — the audit's `-32001` bug).

Design §16.9 (full readiness unification) stays **rejected** and §16.10 (standing §14
deferrals) stays **deferred** — deliberately NOT implemented. §16.7 is a checklist doctrine,
not a code task. NOT pushed; no `main` merge.

---
The remainder of this document (below) is the phase 0-8 build history, unchanged.

**Physical validation on a real Tier-3 rig (2026-07-22).** First end-to-end run on
real silicon — two FTDI FT232R adapters (`usb:0403:6001:BH00L4KU:00` /dev/ttyUSB0 ↔
`usb:0403:6001:BH00LL8O:00` /dev/ttyUSB1) cross-wired as a null modem. Device access
is resolved (the dev user is in `dialout`; the old "S3 access pending" caveat no longer
applies). `serial-nexus-doctor` baseline was clean (12/12), and the rig cert surfaced **the
first genuine real-hardware bug** — in the *doctor*, not the daemon: `p5_certify_pair`
(§15.21) had never run against real UARTs (the sim skips it as "not a UART"), and it
reopened both ports per rate and transmitted *before the FTDI applied the new baud
divisor*, so the rate ladder garbled at 115200+ and reported `rate_ladder=false` while
an independent pyserial test proved the physical link flawless 9600..921600. **Fixed
(`doctor/src/probes.rs`, commit `8cf61d0`):** a 150 ms post-open baud settle
before each single-shot exchange, a **both-direction** ladder (§15.21 "all must
round-trip", closing a pre-existing one-way gap), and a bulkier mismatch pattern so the
frame-error observation is deterministic — verified `rate_ladder=true
deliberate_mismatch_observed=true`, 6/6. Diagnostic-only; no daemon/data-plane change,
sim `phase7/p5.sh` CI path unaffected. The daemon was then driven through the rig and
**every behavior passed**: identity resolution both directions (§12), byte-exact
bidirectional data path (§4/§5/§7.1), the `send` verb, far-side break reception
(port1.brk++), TIOCEXCL exclusivity, exclusive arbitration (lock→LOCKED→steal, §6),
slow-consumer drop-with-counters isolation (§5, exact `received+dropped==sent`), the PTY
symmetric config over the §15.19 writer bridge, and observable framing/parity error
counters under a deliberate baud/parity mismatch. A 4-agent adversarial audit found **no
false passes** and confirmed the doctor fix correct and complete. Codified as
`scripts/validate/hardware/crossover-rig.sh` (commit `906c309`; see the hardware block
under Quality gates). A **guided physical unplug/replug** was then performed live and
passed on every point (§7.1 faulted-and-wait + reopen ritual, §15.25): on unplug the
node reached `waiting` while its attached PTY client stayed present (no HUP) and the
other node stayed `active` (isolation); a command written during the outage parked
(backpressure, never sent); on replug the node auto-healed to `active` by identity,
the reopen ritual reapplied (modem lines reasserted, driver counters fresh, `TIOCEXCL`
retaken), `purged_on_reconnect` equalled the parked command's length exactly (drained,
never fired into the reconnected device), and the healed port carried data both
directions again. Still needs a human hand (inherently interactive, not scripted):
squatter swap (a *different* adapter appearing on the old identity's path) and far-side
modem-line assertion (the 3-wire crossover carries no DTR/RTS to the peer).

**v6 revision + phase 0-4 alignment (2026-07-21).** The v6 docs are v5 with the
phase-5/6 ADRs (§15.22–15.24) *condensed* and their refinements folded forward into
§§3–11 as normative text plus forward-references; the plan gained two doc-only
sentences (endpoint-keyed wiring §15.23; the "presence is not readiness" §4 test note).
The normative additions touching phases 0-4: §6 now states *held-priority reclaim* as
first-class arbitration text (was §15.23-only); §11's structural-atomicity clause now
lists *name/identity legality* ("no `/`, no empties, no duplicate node names or channel
identities"); §3/§5 boundary taxonomy now names *child stdio pipes*. A multi-agent
adversarial audit of the **built** phase 0-4 code against v6 (one auditor per design
area, every finding independently verified) surfaced **5 confirmed deviations** (7
rejected as phase-7/8 scope, sanctioned poll-latency, or code-smell-not-design-text):
- **§11 empty node name accepted** (v6-introduced "no empties"): empty *channel
  identities* were rejected but empty *node names* were not. **Fixed** —
  `ValidationError::EmptyName`, checked in `GraphModel::validate` (covers `load` and
  incremental add-node), with `empty_node_name_is_rejected`.
- **`data.rs` comment said "four boundary types"** (v6 expanded to five, +child stdio
  pipes). **Fixed** — comment now enumerates the five, noting the exec pipe arrives in
  phase 5.
- **Four pre-existing config/CLI-surface gaps** (identical text in v5; the design lists
  a v1 attribute never built — **the user chose to build all now**): (a) serial
  `hostward_buffer` (§7.1 hostward-consumer drop policy → the fan-out channel depth,
  default 256), (b) serial `modem` initial DTR/RTS assertions (§7.1, applied at open,
  retained for phase-7 reopen), (c) PTY `hostward_buffer` (§7.2 → the writer-bridge
  depth, default 32), (d) daemon `--socket-group` (§10 "flags to widen to a group" →
  chgrp + mode 0660). All default to today's behavior, round-trip through dump/load
  (`serial_and_pty_hostward_and_modem_round_trip` + the config proptest), and were
  verified end-to-end (load→dump, `--socket-group` → `660 <group>`). See §3.13.
All gates green: 78 workspace tests, fmt/clippy clean, `all.sh --through 6` = 32/32.

**Phase 6 (2026-07-21).** The cross-daemon transport (§7.4/§9): a new **leg node**
(`nodes/leg.rs`) carrying N channels multiplexed over a tcp|unix socket by the
built-in **link codec** (the shared envelope, §8). `serial-nexus-codec-api` grew the **v1 wire
hello** (`WIRE_MAGIC` "SNXL", `WIRE_VERSION` distinct from `ENVELOPE_VERSION`, a `u32`
capability bitset with `CAP_LOCK_RELAY` reserved, `Hello`/`encode_hello`/
`try_decode_hello`, `WireError`) — a distinct wire construct, not a fifth event kind,
so the four golden vectors stay frozen. `serial-nexus-core` gained the `NodeConfig::Leg`
variant (+ `Transport`/`LegRole`), the leg `shape()` (N channel endpoints, no default
endpoint), and config-level validation (loopback-only unless `insecure_bind`, empty
channel/list rejection → new `ValidationError::{NonLoopbackBind,EmptyLeg}`). The leg
plugs into the §15.23 endpoint-keyed `Wiring` with **zero `Wiring::build` change** —
purely via `shape()`. `serial-nexus-sim` grew `wire` (hostile-or-conforming peer / §9
conformance driver) and `tcp-proxy` (outage injection) modes, plus `pty --stall`. One
new ADR landed — **§15.24** (the leg node, the hello frame, fragmentation-not-drop,
faulted-and-wait); §7.5/§15.23/§14 were touched for the re-multiplexer scoping. A
multi-agent adversarial audit of the built phase 6 found **17 confirmed issues, all
fixed** — most importantly a **critical §5/§9 targetward-no-drop violation** (the leg's
write half `continue`d on an oversize-frame encode error, silently dropping any chunk
whose framed size exceeded `MAX_FRAME_SIZE` — reachable because `READ_BUF ==
MAX_FRAME_SIZE` and the `send` verb line is uncapped; **fixed** by fragmenting oversize
chunks across consecutive `data` frames, verified with a 100 001-byte `send`
round-trip) and a **stale-status wedge** (a `faces=target` leg whose local producers
closed returned `SourceClosed` and left status `Active`/"connected" forever; **fixed**
by parking the write half so the independent read direction and the wire stay live).
See §6d below.

**Phase 5 (2026-07-21).** The codec runtime (§7.5/§7.6/§8): a new `codecs/reference`
crate (the v1 envelope framing as a `Codec`, with length-guided resync); the
interior **codec node** (`nodes/codec.rs`) and **exec codec node** (`nodes/exec.rs`)
on a **generalized endpoint-keyed data-plane wiring** (interior nodes have N+1
endpoints — the first non-two-layer topology); `serial-nexus-sim` grew `mux`/`envelope`
modes; two new ADRs landed — **§15.22** (exec child protocol: the multiplexed side
is a reserved empty channel; the exec codec is a child-pipe boundary, not a pure §5
interior node) and **§15.23** (endpoint-keyed wiring, length-guided resync,
held-priority reclaim); §3/§7.5/§7.6 were touched. A multi-agent adversarial audit of
the built phase 5 found **14 confirmed issues, all fixed** — most importantly a
**critical exec-pump deadlock** (the single `select!` coupled stdin-write and
stdout-read; under sustained flow the child filled its stdout pipe and blocked on
stdin while the daemon blocked writing stdin — fixed by running the two directions as
*concurrently-polled* futures) and **held-lock re-acquire** (was FIFO, letting a
non-held `--wait` waiter inherit the mux lock; now a `reclaim_held` primitive with
priority over on-demand waiters). See §6c below.

**v3 revision (2026-07-20).** The v3 docs folded the refinements below (§3.1–3.10)
into the design text and added two new normative requirements that phase 0-2 code
was realigned to satisfy: (a) design §3 now makes a node name or channel identity
containing `/` a **structural validation error** — enforced in
`serial_nexus_core::graph::GraphModel::validate` (`ValidationError::InvalidName`); and
(b) plan §2 now requires **`Cargo.lock` committed** (the cargo-deny gate is only as
strong as the committed graph) — `Cargo.lock` was un-gitignored and checked in. The
lingering `serial2-tokio` workspace-dependency declaration was also dropped (§13,
§15.1), matching the design narrative that it was removed during implementation.

**v4 revision + audit (2026-07-20).** The v4 docs are v3 plus one substantive
change: the phase-3 hybrid data plane (§3.11 below) was folded into design §5 and
recorded as a new ADR **§15.19** ("The benchmark cashed the escape hatch: a hybrid
data plane"), with **§15.18** now carrying a "(Superseded in part by §15.19)" note.
The split is now clean: §15.18 owns only the *poll(2)-not-epoll / `AsyncFd`-
prohibition* decision, while §15.19 owns the *dedicated blocking threads for the
hot hostward paths* (serial reader, PTY master writer) and the *adaptive
active-to-idle backoff* for the cold async paths. Phase 0-3 was then re-audited
against v4 (multi-agent + adversarial verify). Two genuine deviations were found
and fixed: (a) the PTY node re-asserted the baseline termios on last close only
when the close was observed via POLLHUP, skipping it when the read path saw
EOF/EIO first (§7.2) — `nodes/pty.rs` now does a swap-guarded reset on all three
paths, and the reconciliation backstop is gated on live presence; (b)
`scripts/validate/phase3/subscribe.sh` used a bare `sleep 0.3` to await
subscription registration, against plan §3 — now a bounded `wait-for` on the first
snapshot. Code comments that cited §15.18 for the thread/backoff decision were
repointed to §15.19. No other phase 0-3 deviations surfaced.

**v5 revision + phase 0-4 alignment audit (2026-07-20).** The v5 docs are v4 plus
the slice-C/P5 specification: design §6 gained a "Waiting and fairness" paragraph
(the FIFO waiter queue), lease generation-guarding, and the poll-sampled-presence
blind-spot note; §10 gained a "Waiting verbs" paragraph; §13/§14 gained P5 (doctor
rig certification) and the deferred per-open PTY epoch; and two new ADRs landed —
**§15.20** ("Waiting verbs: the two-lane control plane") and **§15.21** ("The rig
is a fixture, so the doctor certifies it"). A multi-agent adversarial audit of the
**built** phase 0-4 code against v5 found **one genuine deviation, fixed**: a
`waiting`/`faulted` serial node (device absent — a reachable startup state) drained
and silently discarded every targetward chunk (`while rx.recv().await.is_some(){}`),
violating §5's never-drop-targetward invariant. `nodes/serial.rs` now **parks the
targetward receiver unread** (field `parked_targetward`), so the bounded channel
fills and backpressures the origin (commands delayed, never dropped); only the
phase-7 reopen/heal is deferred, not the invariant. Everything else in phases 0-3 +
slice A/B verified faithful to v5.

This document records where the implementation stands and every place the code
deviates from — or refines — the design. The rule from plan §1 holds: where
implementation reality disagrees with the design, the design gets amended first;
the items below are refinements consistent with the design, none contradict it.

---

## 1. Status at a glance

> **Era banner (2026-08-12, the v17 landing):** this section is a frozen 2026-08-05-era
> snapshot kept as a record. Current figures live only in the plan's Status table, which is
> their single home; several sentences below are superseded there and are not quotable as
> current.

| Phase | Scope | Status |
|-------|-------|--------|
| 0 | Doctor + scaffolding | **done** — `serial-nexus-doctor`, CI, cargo-deny gate |
| 1 | Contracts in the small | **done** — serial-nexus-core, serial-nexus-codec-api, serial-nexus-sim |
| 2 | Walking skeleton | **done** — control plane + node lifecycle + data plane (serial↔PTY byte flow, presence gating, backpressure) |
| 3 | Boundaries & logging | **done** — drop counters, log node, `rotate`/`subscribe`, client-termios, high-throughput data plane + benchmark (§3.11) |
| 4 | Arbitration | **done** — slices A & B (exclusive write lock, `lock`/`unlock`, `may_write` gate, purge-on-acquire/-detach, detach-release, held, free-for-all) plus **slice C**: the FIFO waiter queue + two-lane async dispatch, `send`, `--steal`/`--wait`/`--lease-ms`, lease generation-guard, immediate lock notifications (§3.12, §6b, §15.20) |
| 5 | Codecs | **done** — codec runtime + registry (§8), the `codecs/reference` framing codec (resync), the interior codec node + exec codec (§7.5/§7.6), endpoint-keyed wiring, `serial-nexus-sim` `mux`/`envelope`; audited (§6c, §15.22, §15.23) |
| 6 | The wire | **done** — leg node (§7.4) + v1 wire hello (§9), fragmentation, binding, faulted-and-wait/purge-on-reconnect, `serial-nexus-sim` `wire`/`tcp-proxy`, §9 conformance scripts; audited (§6d, §15.24) |
| — | **v6 alignment** | **done** — phase 0-4 re-audited against the revised v6 design; 5 deviations fixed (empty-node-name §11, boundary comment §5, serial/PTY `hostward_buffer` + serial `modem` §7.1/§7.2, `--socket-group` §10) (§3.13) |
| 7 | Identity & resilience | **done** — resolver (§12) + faulted-and-wait/reopen (§7.1) + state file (§11) + `add-node`/`remove-node --cascade`/`load --replace` + serial-signal verbs (§7.1) + doctor P5 + `serial-nexus-sim nullmodem`; audited (§6e, §15.25) |
| 8 | Hardening & release | **done** — macOS build+cfg-gating (cross-checked via `--target x86_64-apple-darwin`) + macOS CI lane + `docs/macos.md`; docs (README, `docs/security.md`, `docs/codec-authors.md`, `docs/rpc/`); packaging (systemd unit, udev, example config); cargo-fuzz targets (`fuzz/`, nightly); `phase8/{quickstart,agent-task,soak}.sh` + CI wiring; audited (§6f) |

**Quality gates (all green):** `cargo fmt --all --check`, `cargo clippy --workspace
--all-targets --locked -- -D warnings` (plus the minimal-daemon clippy), `cargo build
--workspace --locked` **then** `cargo test --workspace --locked` — one suite now carrying
the unit/property tests *and* the whole `serial-nexus-itest` integration harness — `cargo check
--target x86_64-apple-darwin --workspace --exclude serial-nexus-web` (macOS portability), and
`cargo deny check licenses bans sources`. The per-phase counts this section used to quote
(87 workspace tests, 42 bash checks) are dead numbers from before §16.11 folded
`scripts/validate/**` into the harness; AGENTS.md §3 carries the exact current command block.
The current whole-suite figure is **767 passing, 0 failed, 4 ignored** across 114 test-result
targets on Linux (766 at `f8315cc`; §3.53's own passive/rig digest gate is the +1) — 114 is the count of `test result:` lines, not of cargo targets, of which
there are 112 (104 `Running` + 8 doc-test); two lines and two of the 766 come from the nested
`cargo test -p acme-codec` that `p8_external_codec.rs` spawns, so the workspace's own named
tests are 764 passed / 4 ignored / 0 failed (§3.53) — (2026-08-05: §3.51's cell-set digest added nine — three in `report.rs`, the four of the new `itest/tests/meta_doctor_artifacts.rs` target, and two in `expectation_gates.rs`; §3.49's three P5 guards plus its `serial_nexus_sys`
`ICOUNTS_SUPPORTED` guard, added to two existing targets; §3.38's listener-barrier guard, §3.39's orphan-leash fixture and guard, §3.40's two baseline guards, and earlier the same day the three doctor guards of §3.34 and the kernel-naming fix— `termios_mode_tells_the_daemons_baseline_from_a_cooked_pty`,
`p10_recoverability_separates_a_deep_buffer_from_a_black_hole` and
`the_os_name_survives_a_box_with_no_os_release_file` — on top of the 729 left by
2026-08-04's P13 probe unit test, `p8_map`'s residual-forward guard and
`p13_teardown_accounting`'s three, themselves on top of the 724 the review-37
remediation left on 2026-07-29); of those, one is the doc-tested
twelve-line embedder `main` in `daemon/src/lib.rs`, which is the §15.26 entry surface
proving it still compiles under the family names. On **macOS** the same tree is **715
passing, 0 failed, 4 ignored** across 102 binaries + 8 doc-test targets (2026-08-04 at
`fa4b12d`, Darwin 24.6.0 / 15.7.8, x86_64, with the FT232R crossover attached so
`serial_hardware.rs` runs rather than self-skipping — all four rig tests executed); the
shortfall against Linux is the Linux-only targets and the serial-device tests that self-skip
where a pts is not a serial device, not failures. The binary count moved 101 → 102 with
`p13_teardown_accounting`. **A second macOS figure landed on 2026-08-05 at `7ead470`, in a
different scope, and the two must not be conflated:** `cargo test --workspace --locked
--exclude serial-nexus-web --no-fail-fast` reads **684 passing, 0 failing, 4 ignored across 109
test binaries** on a clean run, against the 680/1/4-across-109 baseline `docs/macos.md` records
for `fa4b12d` in that same scope — 681 tests run there against 684 here, the +3 being exactly the
three doctor guards of §3.34. No whole-workspace macOS run was taken at `7ead470`, so the 715
figure above stands as the only one of its kind and is **not** superseded by 684.
**A third macOS figure landed on 2026-08-05 at `1a9a8fc`, in that same
`--exclude serial-nexus-web --no-fail-fast` scope: 690 passing, 1 failing, 4 ignored across 109
test binaries.** The +6 over 684 is new `#[test]`s from `45d50cb`, `88d0de5` and `4b78fff` with no
new test binary; 690 + 1 = 691 tests run. **The one failure is
`p4_free_for_all::free_for_all_endpoint_lets_concurrent_writers_both_reach_device`**, which is
**not** the `p6_hostility` flake below — that flake did not recur in this run — and which
reproduces 12 of 12 on an idle box. It is that test's first execution on Darwin (§3.46), so 690/1/4
is the honest current figure and 684/0/4 is not stale so much as taken before the rig fallback made
this test run at all. **That clean
number is one of three runs and must be quoted with the other two:** a second read 683/1/4, failing
`p6_hostility::wire_hostility_faults_cleanly_then_leg_heals` — a different test from the flake
described just below, in the same binary and with the same `Connection refused (os error 61)`
fingerprint, which widens that entry's scope without settling its cause — and a third lost all five
rig-touching tests to an orphaned daemon holding both FTDI ports (`docs/macos.md`, 2026-08-05). Two notes on reading that run. The `test result:` lines number 112
rather than 110, because `p8_external_codec` builds and runs the out-of-tree consumer template
and its nested cargo emits two more. And the figure is a *clean* run only on the second pass:
the first full run on this box hit one contention-dependent flake,
`p6_hostility::a_trickling_peer_trips_the_handshake_deadline_and_the_leg_heals`, which failed 0
of 20 times run serially and 1 of 40 under 8-way concurrency with the same `Connection refused
(os error 61)` fingerprint. ECONNREFUSED rather than ENOENT places it in the window between
`load`'s reply and the leg's `UnixListener::bind` in its spawned task — a §9 proxy-in-time
suspect, **located but not root-caused**: no independent adversarial verification, no fail-first
proof, and nothing fixed on its strength.

**Hardware integration test (Tier-3, opt-in):** `itest/tests/serial_hardware.rs` — the
end-to-end test on *real* silicon (design §13/§15.17/§15.21, plan §5), which replaced the
retired `hardware/crossover-rig.sh`. It requires two USB-serial adapters wired together with a
crossover UART cable, auto-detected by `crossover_ports()` (`/dev/cu.usbserial-*` on macOS, or
`SNX_CROSSOVER_A`/`_B`), and **self-skips** when none is present — a skip is a valid verdict.
Its four tests drive the daemon through the physical rig: byte-exact bidirectional data path
by SHA-256 (§4/§5/§7.1) at 115200 and at the custom rate 250000, the `send` verb reaching the
far port, `TIOCEXCL` exclusivity against a second opener, the serial signal verbs
(`send-break`/`set-modem`/`pulse-dtr`) — unreachable on the pts that `p7_signals` uses — and
the v11 map node in both directions. They share a process-wide mutex, so the two ports are
never contended. Certify the rig first with `serial-nexus-doctor --port … --port …` (the §15.21
precondition: a failure is attributable to a loose wire, not the daemon). Verified passing on
a cross-wired FTDI FT232R pair.

**Kernel matrix:** every probe that runs on Linux reports `supported` on **Linux
7.0.0** (dev box, Ubuntu 26.04 — **21 · 0 · 0 · 1 on 2026-07-29 with a `da290c616631`
binary and the Tier-3 cross-wired FT232R pair**, committed as
`docs/doctor/linux-7.0-2026-07-29-tier3.json`; the three 13 · 0 · 0 · 6 passive runs
committed beside it are the earlier state of that same day, taken while the pair was
on the other box, and are superseded as the 7.0 baseline — review 37 `37-DOC-2`) and
on **Linux 6.18.14** (Debian rodete — 19 · 0 · 0 · 0 on
2026-07-27 with a `fe1c52c` binary and one dangling adapter; **21 · 0 · 0 · 1 on
2026-07-29 with a HEAD binary and a Tier-3 cross-wired pair**, the one skip being
P12, which is inert on Linux by design). **P6, P7 and P8 are byte-identical across
the two** — P8 including `elapsed_ms` once both sides were the same binary —
P1/P2/P3's booleans match, P9's timer floor is *tighter* on 6.18 on every row
(≤ 1 %), and P10's apparent difference dissolved when the two kernels swapped
flip-scheduling shapes. The zero-timeout `poll(2)` costs that differ are box
properties, not kernel ones (the same 6.18 kernel measured 605, 1162 and 526 ns for
the same code on three dates). So the kernel-sensitive PTY/serial mechanics are
de-risked across the support matrix — but "zero deltas" would be the wrong
sentence, and what the 6.18 run does *not* cover (HEAD's P4, everything a paired
rig certifies, the `--json`/`jq` gate, and `cargo test` at all) is enumerated in
`docs/serial-nexus-doctor.md`. Read that section, not this line, before acting on it.

---

## 2. Where the code lives

| Crate | Role | State |
|-------|------|-------|
| `serial-nexus-codec-api` | codec trait (+ `resync_count`), event vocabulary, envelope frame codec + golden vectors, **v1 wire hello** (`WIRE_MAGIC`/`WIRE_VERSION`/`Hello`/`WireError`) (§8/§9) | done |
| `codecs/reference` (`serial-nexus-codec-reference`) | the v1 envelope framing as a `Codec`, with length-guided resync (§7.5/§9) | done (phase 5) |
| `serial-nexus-core` | graph model + validator (§4), data-plane deliver contracts + holdover (§5), lock state machine incl. `reclaim_held` (§6), config/state split (§15.8), **device-identity `resolver` (§12)** | done |
| `serial-nexus-rpc` | JSON-RPC 2.0 wire types — the stable §15.16 surface | done |
| `serial-nexus-sim` | test double: `pty`/`client`/`mux`/`envelope`/`exec-conformance`/`wire`/`tcp-proxy`/`nullmodem` modes (§3) | done |
| `serial-nexus-doctor` | shipping capability checker: probes P1–P13 + env checks (§15.17) | done |
| `serial-nexus-daemon` | the daemon | control plane + node lifecycle + data plane + codecs + leg/wire done |
| `serial-nexus-ctl` | the CLI (thin RPC client + `--json`) | `load [--replace]`/`add-node`/`remove-node [--cascade]`/`connect`/`disconnect`/`dump`/`state`/`info`/`ports`/`subscribe`/`tap`/`rotate`/`lock`/`unlock`/`send`/`send-break`/`set-modem`/`pulse-dtr`/`teardown`/`shutdown` |
| `serial-nexus-web` | the web console's server half (§17): static assets, the split credential and token gate, TLS, the bounded WebSocket bridge that forwards an allowlist of RPC methods, plus a `wsclient` headless client for driving the browser-facing protocol without a browser | done |

Daemon modules (the v8 library/binary split moved all of these out of the thin binary
crate into the `serial-nexus-daemon` library, §15.26; the binary is now flags + tracing +
a `run` call): `lib.rs` (entry surface, socket and state-file policy), `control.rs`
(JSON-RPC over UDS), `daemon.rs` (graph state + method impls), `runtime.rs`
(endpoint-keyed data-plane `Wiring` + the shared fragmentation and fan-out helpers),
`boundary.rs`, `cell.rs`, `registry.rs`, `tap.rs`, and
`nodes/{mod,serial,pty,log,codec,exec,leg,map}.rs`. The single unsafe-bearing module
is the separate `serial-nexus-sys` crate (ioctls, raw `read`/`write`/`fcntl`,
`poll_ready`/`poll_blocking`). AGENTS.md §1 carries the current crate-by-crate table
and is the one to keep in sync.

The integration harness is the canonical exit criterion (plan §3): the `serial-nexus-itest`
crate, run by `cargo test` like any other. It replaced the bash `scripts/validate/**`
maze in §16.11 — `scripts/` is **deleted**, `bash` appears nowhere in the gates, and each
former phase script is a `itest/tests/*.rs`. Where a section below still names a
`phaseN/*.sh`, read it as a dated record of what ran at the time, and see the migration
entry above for the script→test mapping.

---

## 3. Deviations & refinements from the design

These are implementation decisions the design does not spell out, or where a
kernel/library reality shaped the approach. None contradict the design.

### 3.1 Serial node uses `serial2` + poll-based readiness, not `serial2-tokio`
**Design:** §13 lists `serial2`/`serial2-tokio` for "concurrent async read/write."
**Reality (serial-nexus-doctor P3 research):** `serial2-tokio` 0.1.24 exposes **no
accessor for the inner fd**, and `serial2` **does not take `TIOCEXCL`** (only
`O_NOCTTY`). The daemon needs the raw fd for `TIOCEXCL` (§7.1) and later
`TIOCGICOUNT` (§5).
**Decision:** open a `serial2::SerialPort` (settings, modem lines,
break, and the raw ioctls via `as_raw_fd`), set it non-blocking, and drive async
I/O with poll-based readiness (see §3.10) — rather than `serial2-tokio`.
**Correction (review 37 `37-SER-3`):** this entry, `sys/src/lib.rs`, the
`nodes/serial.rs` module doc, the root `Cargo.toml` comment and design §13 all said
the fd was *opened blocking*. It is not: the pinned serial2 0.2.37
`SerialPort::open` passes `O_NONBLOCK | O_NOCTTY` as custom flags and never clears
them. The daemon's own `set_nonblocking` on that fd is therefore redundant — and is
kept deliberately, because the readiness loop's correctness must not rest on a
dependency's open flags, which are not part of its published API. Nothing about the
reopen-window reasoning changes; the prose was simply describing a state the fd was
never in.
Consistent with §13's "raw termios via nix/rustix as the fallback." `TIOCEXCL` is
issued by the daemon itself (`nodes/serial.rs`). `serial2-tokio` is now an unused
dependency and was dropped from `daemon-bin/Cargo.toml` — and, in the v3
realignment, from the root `Cargo.toml` `[workspace.dependencies]` as well, so the
design's "dropped during implementation" (§13, §15.1) is literally true of the
manifest.

### 3.2 PTY slave is *primed* at creation (POLLHUP never-opened refinement)
**Design:** §7.2 detects presence via the master's HUP condition.
**Reality (serial-nexus-doctor P2):** a master whose slave was **never opened** does
**not** report `POLLHUP`; HUP only appears after the first open→close. So HUP
alone cannot represent the initial no-client state.
**Decision:** at PTY node creation, open and immediately close the slave once
(`prime_slave` in `nodes/pty.rs`). This forces the "absent" HUP state, so
presence detection via POLLHUP is uniform from the start. This step is not in the
design text; it is a faithful refinement of §7.2's model, confirmed identical on
7.0 and 6.18.

### 3.3 Data-plane holdover needs an explicit `flush` on resume — *in the model*
**Design:** §5 — a transform that has emitted output when downstream refuses
"parks it in its holdover slot."
**Refinement:** a chunk parked on the *last* offer would be stranded if the
runtime only retries on new origin input. `serial-nexus-core::data::TargetwardSink` has
a `flush()` method that drains parked holdovers in order, independent of new
input. Caught by a property test (`prop_targetward_no_loss_bounded_interior`). v4 §5
now names this explicitly ("boundaries announce writability, and the runtime drains
parked holdover frames on that signal, independent of any new origin input").
**Correction (review 26, F3/LOCK-3):** this entry used to say "the runtime calls
`flush()`". It does not, and never did — nothing outside `data.rs` calls it, because
`serial_nexus_core::data` is the executable *specification* of §5, not the shipped data path
(§3.18). The anti-stranding property is real and is carried in the daemon by the
channel-plus-`send().await` shape: a paused origin is literally a task suspended inside
`tx.send(chunk).await`, and resumption *is* the bounded channel waking it, so there is
no separate writability signal to drain on. `data.rs`'s module doc now states the
model/path split outright; read the two together.

### 3.4 `EndpointAddr` serializes as its display string
**Design:** §3/§15.12 — display form is `node/channel`; neither part contains `/`.
**Decision:** in configuration, an endpoint address serializes as that **string**
(`"usb0"` or `"mux/console"`), not a nested `{node, endpoint}` table. This keeps
edges all-scalar and TOML-clean and makes configs read the way operators write
them. The design does not specify the on-disk encoding of an address; this is a
presentation choice. (`serial-nexus-core::graph::EndpointAddr`.)

### 3.5 JSON-RPC `id: null` and result-XOR-error validation
**Design:** §10 — hand-rolled JSON-RPC 2.0.
**Refinement (from an adversarial review):** `serial-nexus-rpc` now has an `Id::Null`
variant and `Response::error_without_id`, so a parse-error / invalid-request
reply carries the spec-mandated `id: null` (JSON-RPC 2.0 §5) and never desyncs a
client's read stream; and `Response`'s deserializer enforces exactly-one-of
`result`/`error` (distinguishing a present `result: null` from an absent one).
Completes §10's contract; not a deviation.

### 3.6 `load` RPC carries the config as JSON, not TOML text
**Design:** §10 — "Configuration files are TOML; the RPC carries JSON."
**Decision:** `serial-nexus-ctl` reads the `.toml` file, parses it to
`GraphConfig`, and sends `{"config": <GraphConfig as JSON>}` in the `load`
params; `dump` returns the config as JSON and the CLI renders TOML. The CLI owns
the TOML↔JSON conversion (presentation, §15.16); the daemon speaks only JSON.

### 3.7 Daemon-specific error codes
`load` on a non-empty graph → `-32001`; a structural validation failure →
`-32002` (with all offenders in `error.data.errors`). Both in the reserved
application range `[-32099, -32000]` (§10). `serial-nexus-rpc::error_codes` unchanged.

### 3.8 `advertised_baud` maps to standard rates only
PTY `advertised_baud` is cosmetic (§7.2). nix on Linux sets termios speed via a
`BaudRate` enum (standard rates only), so a non-standard advertised baud is
skipped rather than approximated. (`nodes/pty.rs::standard_baud`.)

### 3.9 Unimplemented node kinds were a structural load error (resolved in phase 3)
Before phase 3, a configuration containing a **log** node was rejected at load
(`node <name>: log nodes land in phase 3`), nothing created — a build-stage
limitation, not a design position. Phase 3 (slice B) implemented the log node and
removed the rejection; a log node now loads normally. Kept here only as a record.

### 3.10 Data-plane readiness is poll-based, not `tokio::AsyncFd` (the pty-master spin)
**Design:** §5 — a single-threaded async data plane; the design does not name a
readiness mechanism.
**Reality (found while wiring slice 2):** `tokio::io::unix::AsyncFd` (epoll)
**spuriously and persistently reports a pty master readable** once an external
client is attached — `readable()` returns ready every poll while `read(2)` gives
`EAGAIN` and a direct `poll(2)` reports *no* readiness (epoll disagrees with
`poll`). Because `readable()` completing synchronously never yields, this
busy-loops and **starves the entire current-thread runtime** (every other task,
including the control plane, freezes until an unrelated I/O event — e.g. the
client disconnecting — breaks the loop). Reproduced in isolation; independent of
packet mode, the sync presence poll, shared-vs-dup fds, and `select!`. It is a
genuine epoll/pty-master quirk, and `AsyncFd` is unsuitable for these fds.
**Decision:** drive readiness with a **non-blocking `poll(2)`** (`sys::poll_ready`,
zero timeout — returns immediately, never blocks the thread) plus a short async
`tokio::time::sleep` (`runtime::IDLE_POLL`, 5 ms) only when idle. During an active
transfer a task re-polls immediately after each full drain, so the interval
bounds idle latency (and idle CPU — measured ~1%), never throughput (1 MiB
echo round-trips in ~0.5 s). Reads: `poll(POLLIN|POLLHUP)` → drain to `WouldBlock`.
Writes: `write(2)` then, on `WouldBlock`, `poll(POLLOUT|POLLHUP)` + sleep. This
applies to **both** node types uniformly (`runtime.rs`, `nodes/{pty,serial}.rs`);
a real UART tolerates epoll but the daemon must also drive the PTY master and
(in tests) pts-backed "devices", so one poll-based path is simplest.
**Future:** idle CPU is a fixed ~1%/idle-fd today; a longer or adaptive idle
interval, or a `spawn_blocking` reader thread for high-baud serial, is a phase-3
optimization if the throughput benchmark demands it. `AsyncFd` is *not* the
answer for pty masters.

### 3.11 The phase-3 benchmark demanded §15.18's thread escape hatch (both axes)
**Design:** §15.18 frames the poll(2) readiness as bounding "idle latency, never
throughput" (re-poll immediately during active transfer), with `spawn_blocking`
reader threads as an escape hatch *if the benchmark demands it*, and idle CPU as
the named concern (~1%/idle-fd).
**Reality (phase-3 benchmark):** on the current-thread runtime the "re-poll
immediately" intuition does **not** hold for a peer in a *separate process* — a
`yield_now` spin returns instantly (no other runnable task), so no wall-clock
passes and the peer never refills; the wait therefore always pays the ~1 ms tokio
timer floor per buffer cycle, capping hostward throughput at **~1 MB/s** (measured
1.2 MiB/s serial→log). That is below even one 3 Mbaud port for a fast consumer —
so the escape hatch was **required**, not optional, exactly as §15.18 reserved.
**Decision:** the two high-throughput paths — the **serial hostward reader** and
the **PTY hostward writer** — run on **dedicated blocking threads** doing a
*blocking* `poll(2)` (`sys::poll_blocking`), which the kernel wakes the instant
the fd is ready. Result: ~185 MiB/s, lossless, and **zero** CPU while parked
(a blocked poll costs nothing — this also dissolves the idle-CPU concern for
these fds). Cross-thread counters became atomics (`Rc`→`Arc`, `Cell`→`Atomic*`);
the PTY writer is fed by an async pump through a **bounded** bridge so the buffer
stays bounded and full-buffer drops are counted. Low-rate paths (targetward
PTY→serial, PTY presence/termios) stay async poll-based, now with an
`ACTIVE_POLL`→`IDLE_POLL` adaptive backoff → **~0.06%/idle-fd** (2% total for 32
idle PTYs, well under budget; the §15.18 idle-CPU concern, resolved).
**Recorded:** `docs/benchmarks/phase3.json` (throughput + idle axes);
`scripts/validate/phase3/{firehose,exact-loss,benchmark}.sh`. **Folded into the
design in v4:** this decision is now ADR **§15.19** and §5's "hybrid" paragraph,
and §15.18's "never throughput" claim is corrected there (it held only until the
hot hostward path moved to a blocking thread). The design pass this section asked
for is done; the code comments were repointed from §15.18 to §15.19 to match.

### 3.12 Arbitration addressing: `lock`/`unlock` name the origin, not the endpoint
**Design:** §6 shows `serial-nexus-ctl lock <node/channel>` and `send <node/channel>`
without pinning down whether `<node/channel>` is the origin acquiring the lock or
the host-facing endpoint being locked.
**Decision (phase 4, slice A):** the lock lives on a **host-facing endpoint** (the
serial node), but the RPC `lock`/`unlock` name the **origin** — the target-facing
writer (a PTY) that acquires it. The daemon resolves the origin to the unique
endpoint it feeds (a target-facing endpoint has exactly one edge, §4). This is what
makes the reference workflow coherent: `lock ptya` grants *ptya* the write lock so
its operator can type, while other origins on the same serial are locked out. The
later `send` verb (slice C) instead names the **target** endpoint, since the CLI is
itself the transient origin. This is a presentation/RPC-shape choice the design
leaves open (§15.16); the state machine (`serial_nexus_core::lock`) is addressing-agnostic
(it keys on an opaque `OriginId`), so a future spelling change costs only the daemon
glue. **Architecture:** the lock is a pure state machine in `serial_nexus_core::lock`
(property-tested); the daemon shares one `Rc<RefCell<EndpointLock>>` per endpoint
(all tasks are on the one runtime thread) between the control-plane methods that
mutate it and each origin's PTY read task, which consults `may_write` before
draining targetward. A non-holder is *not read from* (its bytes stay in the kernel
buffer — backpressure, never dropped), so arbitration reuses the §5 pause machinery
and adds no data path, exactly as §6 requires. The serial node's host endpoint
carries a new `arbitration = exclusive | free-for-all` config attribute (§6).
**Purge-on-acquire runs synchronously in the daemon's `lock` at grant time**
(draining the origin's master fd via `Node::purge_origin` before the grant reply
returns), *not* lazily in the reader task — a lazy drain would race a correct
acquire-before-write client's first command and discard it (caught by an
adversarial review; guarded by `phase4/purge.sh` check 3).

**Known limitation — sub-poll close+reopen (poll-based presence).** Detach-release
and purge-on-detach hinge on observing the PTY's present→absent transition via
level `POLLHUP` (§7.2). If a client closes and a *different* client reopens the
same slave within one poll interval (≤ `IDLE_POLL`, 5 ms for a quiescent origin),
the transition is unobservable — the successor inherits the predecessor's lock
without an explicit re-acquire, and the baseline termios is not re-asserted. This
is inherent to poll-based presence (the §15.18/§15.19 tradeoff), not a logic bug;
it affects only the detach-release path (an explicit `unlock` is unaffected) and
never lets a *different endpoint's* origin write (exclusion still holds). A
per-open generation/epoch would close it if it ever matters; deferred.

### 3.13 Node config surface completed to match §7.1/§7.2/§10 (v6 alignment)
**Design:** §7.1 lists a serial node's Configuration as including *initial modem-line
assertions* and a *hostward-consumer drop policy*; §7.2 lists a PTY's as including a
*hostward drop policy*; §10 lists *flags to widen the control socket to a group*. These
attributes were specified in v1 but never built (the text is identical in v5/v6); a v6
alignment audit flagged the config-surface gap and the user directed building them.
**Decision (mapping each design attribute to the real boundary buffer):**
- **Serial `hostward_buffer`** (`usize`, default 256) — the depth of the per-consumer
  *fan-out channel* the serial reader `try_send`s into (§5 "bounded buffering where
  configured"). Plumbed in `runtime::Wiring::build` (a serial node's depth overrides
  `CHANNEL_CAP` for edges it produces; other producers keep the default). Hostward is
  always lossy-with-counters, never `fault` — a slow spy must cost only itself (§5) — so
  the only tunable is depth (a scalar), unlike the log node's `{drop-oldest|fault}`.
- **Serial `modem`** (`ModemLines { dtr: Option<bool>, rts: Option<bool> }`, default both
  `None` = untouched) — initial DTR/RTS assertions applied in `open_port` after
  `TIOCEXCL` (serial2 `set_dtr`/`set_rts`); a `None` line keeps the driver's power-on
  state, so the default is exactly today's behavior. Stored on `SerialNode` so phase 7's
  reopen ritual restores it against auto-reset adapters (§7.1). Serialized as a *trailing
  table* (after the scalar fields, like a codec's `attributes`) and skipped when unset.
- **PTY `hostward_buffer`** (`usize`, default 32) — the depth of the PTY's internal
  *writer-bridge* `sync_channel` (§5); replaces the former `WRITER_QUEUE` const.
- **`--socket-group <name>`** — resolves the group (hard error if absent), chgrps the
  control socket, and relaxes its mode to 0660; unset keeps the 0600 owner-only default
  (§10). Mirrors the PTY slave's group logic (§7.2).
The three drop-policy mentions (serial §7.1, PTY §7.2, log §7.3) thus map to three
*distinct* real buffers — producer fan-out, consumer writer-bridge, and the log file
queue — so listing a policy on both producer and consumer is not redundant. All default
to current behavior; validation is unchanged; round-trip is pinned by
`serial_and_pty_hostward_and_modem_round_trip` and the config proptest (generators now
vary `hostward_buffer` and `modem`).

### 3.14 Opus comprehensive review remediation — DONE (2026-07-23)
A full multi-agent, adversarially-verified code review landed at
`docs/historical/19-claude-opus-code-review.md` (63 verified findings, 56 distinct). **All of them
have now been addressed** — every should-fix item fixed with a deterministic regression
test, every justified deviation either hardened or documented, and the testing/doc gaps
closed. Gates after the work: `cargo test --workspace` green (all 16 binaries),
`cargo clippy --workspace --all-targets --locked -- -D warnings` clean,
`cargo fmt --all --check` clean, and `bash scripts/validate/all.sh --through 8` green.
Each fix was applied against a per-file spec produced by a 16-agent analysis workflow
and cross-checked against the design invariants.

**Criticals.** XC-NODROP-1: the in-process codec (`codec.rs channel_targetward`) now
*fragments* an oversize targetward chunk across consecutive data frames (via `codec.mux`
per piece, cap = `MAX_FRAME_SIZE − (3 + channel.len())`) instead of `continue`-dropping
it, and counts any residual against a new `discarded_targetward` channel counter (§5) —
regression `targetward_oversize_chunk_is_fragmented_never_dropped` (100 001-byte
round-trip). PTY-1: the blocking PTY writer now observes the `stop` flag inside
`blocking_write_all`'s poll loop, so a present-but-stalled client can no longer wedge the
teardown join and freeze the single-threaded daemon.

**High.** RESOLV-1: an empty/whitespace sysfs `serial` string now normalizes to the `-`
absent marker at the source, so it degrades to by-path instead of being captured as a
concrete `usb:vid:pid::iface` that would adopt the wrong second adapter (§15.10).
*Upgrade note:* a config persisted by a pre-fix daemon may hold the old `usb:vid:pid::iface`
form; sysfs now reports `usb:vid:pid:-:iface`, so that stored identity no longer matches
and the node comes up `waiting` until re-added (which re-captures it as by-path). This is
the intended retirement of the wrong-device-prone identity — the failure is safe and
operator-recoverable — but it is a visible on-upgrade behavior change worth noting.

**Deliberate deviations from a finding's literal recommendation (all sound):**
- **PTY-1 teardown join is NOT bounded-with-detach.** The finding suggested also copying
  the log node's `recv_timeout(FLUSH_WAIT)` + *detach* pattern. That is unsafe here: the
  PTY writer holds only a **raw fd** (`master.as_raw_fd()`), so detaching a still-running
  writer and then dropping the master (`self.master = None`) would close the fd out from
  under the thread — the exact fd-reuse race `BlockingReader::stop_join` avoids. Making the
  writer promptly stoppable (observe `stop` within one ≤500 ms poll) is the fd-safe
  equivalent and is what was applied; the unbounded `w.join()` now always returns promptly.
- **XC-PURGE-1 drains the in-flight backpressured chunk, not the origin kernel buffers.**
  `purge_on_reconnect` is now `async` and drains-then-`yield_now`s until the targetward
  pipeline is quiescent, so a producer suspended inside `tx.send().await` (holding one
  already-read outage-era chunk) resolves and is drained+counted *before* `set_active`,
  rather than firing into the just-reopened device on the first post-reconnect `recv`.
  Purging bytes still sitting in an *origin's own* kernel buffer during the outage remains
  out of scope (the same family as the documented sub-poll close/reopen blind spot §3.12);
  a continuously-producing remote leg is likewise not drained past quiescence.
- **RUNTIME-1 gives a defined error, it does not prune the endpoint.** A `send` to a
  codec/exec channel whose interior node cannot route targetward now fails fast with a
  defined `invalid_params` (the `sender.is_closed()` pre-check) instead of an opaque
  `-32603`; the residual mid-flight teardown case maps to `app_errors::LOCKED`. `lock`
  on such an endpoint still succeeds (a harmless, pointless lock) — out of the minimal
  correct scope.
- **CTRL-3 is resolved as documentation, not a code change.** The finding asked that a
  write-half EOF (the `echo | socat` idiom) not cancel a waiting verb. But §15.20 is
  normative — "a dropped control connection dequeues the waiter" — and `phase4/waiting.sh`
  enforces it by *killing* a `lock --wait` client and requiring prompt dequeue. A killed
  client and a half-close are indistinguishable at read-EOF, so a "keep awaiting on EOF"
  policy strands the killed waiter (verified: it regressed both `waiting.sh` and, via a
  hung control connection, `resync.sh`). The design-correct behavior — any second-lane
  resolution (EOF/pipeline/error) cancels the wait — is therefore *kept*. `serial-nexus-ctl`
  holds both socket halves open across the read, so its waiting verbs are unaffected; a raw
  `socat` waiting-verb user must likewise keep the write half open. CTRL-1 (the 1 MiB
  request-line bound via the new `RequestLines` reader) is the substantive control.rs fix
  and is applied.
- **daemon-arbitration-1 is fixed in `serial-nexus-core::lock::acquire`.** While the lock is free
  but a registered `held` origin (a demux) exists that is not the caller, `acquire` now
  returns `Denied { held_by }` *before* the FIFO-head check, so a woken on-demand `--wait`
  waiter re-parks and the held origin's `reclaim_held` wins deterministically rather than
  by tokio scheduling (§6/§15.23). Fuzzed by `prop_held_priority_invariants`.

**Justified deviations — now hardened or documented (were "accepted as-is"):**
- **GRAPH-1 (`write_mode` on a log-target edge):** kept cosmetic-but-correct; the
  `EdgeConfig::write_mode` doc now states plainly that the runtime forces `never` for a
  read-only (log) target regardless of the configured value, and that the value only
  round-trips through `dump`/`load`.
- **LEG-2 (connect-role `peer_address`):** now populated — a `connect` leg reports the
  dialed address (`a.address`) on a successful handshake, cleared to `None` on disconnect.
- **RESOLV-3 (`usb:` identity field count only):** `resolve_usb_identity` now rejects any
  empty field (`usb::::`, `usb:0403:6001::00`, …) as `Malformed` at add time; an absent
  serial/interface must be spelled `-`, never empty.

**Simplifications.** OPSIMP-1 (`reacquire_held`) and OPSIMP-2 (`data_frames`) are extracted
into `runtime.rs` and shared by codec/exec/leg; OPSIMP-4 (`GraphState::node`/`node_index`)
and OPSIMP-5 (`GraphState::absorb_wiring`) collapse the duplicated daemon idioms; OPSIMP-6
(state_extra Option guards) and OPSIMP-7 (CLI `read_config`) are applied. **OPSIMP-3**
(re-keying `GraphState`'s three maps from display `String` to `EndpointAddr`, dropping the
per-`state`-call reparse) is **deliberately deferred** — it is a cross-cutting NIT touching
`load`/`add_node`/`remove_node`/`state`/`send`/`resolve_origin` at once, and the review
itself recommends landing it as an isolated follow-up commit rather than bundling it with
the correctness fixes; the mechanical dedup it enables (OPSIMP-4/5) is already in place.

### 3.15 WITHDRAWN — `flow_control` spelling was a defect, not a justified deviation
This slot briefly held "the design text should follow the code" for `flow_control`'s kebab-case
values. It was wrong: `xonxoff`/`rtscts` — the exact spellings §7.1 listed at the time — failed to
deserialize, and since that is a TOML parse error the *entire* configuration file was rejected. It
is now fixed as CFG-1 (`#[serde(alias = …)]` on both, kebab-case still canonical so `dump`
round-trips unchanged), and v12 corrected §7.1 to name the canonical kebab-case form with the
unhyphenated ones as aliases — so there is no deviation left to record, in either direction.

The heading is kept as a marker rather than deleted, because *how* it was wrong is the useful part:
the verdict that made this look settled came from a verifier that had read this very entry. Blind
re-verification against a checkout containing neither the entry nor the review that proposed it
reversed it. **The slot number is retired; do not reuse §3.15.**

### 3.16 `arbitration` is configured per node, applied to every host-facing endpoint
**Design:** §6 calls `arbitration = exclusive | free-for-all` "a per-endpoint attribute", and
§15.7 "a per-endpoint opt-out".
**Reality:** it is a scalar on the node (`Serial`, `Codec`, `Leg`, `Map` configs), and
`Wiring::build` applies the node's value to each of that node's host-facing endpoints; `state`
reports it per endpoint, inside each `LockSnapshot`.
**Decision:** keep the code. No shipped node type has a case for divergent per-endpoint policy
— a codec's channels and a leg's channels are one operator decision in practice — and a
per-endpoint override is additive later (a channel-level attribute overriding the node's) with
no change to the lock machinery, which is already keyed per endpoint. The observable surface
already matches the design's wording. (Opus review `docs/26`, DM-6.)

### 3.17 The map's `held` raw-edge default is a runtime promotion, not a config default
**Design:** §7.8 — "The map's targetward edge into the upstream endpoint defaults to `held`".
**Reality:** `EdgeConfig::write_mode`'s serde default is `on-demand` like every other edge; an
omitted/`on-demand` edge whose target is a map's `raw` endpoint is promoted to `held` (explicit
`held` passes through, explicit `never` is preserved for a read-only map).
**Decision:** keep the code. Promotion at wiring time — mirroring the log→`never` override —
keeps `dump` faithful to what the operator actually wrote, which is the §11 round-trip
property; folding the default into the config layer would make `dump` emit a mode the operator
never typed, and would need the edge default to depend on the *other* endpoint's node type.
The cost is that the runtime mode and the dumped mode differ for an omitted value, which is
why `EdgeConfig`'s doc comment and `docs/rpc/configuration.md` both state the promotion
explicitly. (Opus review `docs/26`; the "dump round-trips wrongly" reading was refuted.)
**Refinement since (review 26, RV-4):** the promotion no longer *lives* in `Wiring::build`. Both
promotions are now `GraphConfig::effective_write_mode`, which `Wiring::build` calls and which
`GraphConfig::validate` also calls for the at-most-one-`held`-origin-per-endpoint rule. That
sharing is the point: the reachable failure was two maps attached to one upstream endpoint with
`write_mode` written nowhere, both promoted to `held`, one starved forever and invisible in
`state`. A validator re-deriving the rule would have missed the promoted shape; calling the same
function cannot.

### 3.18 `serial_nexus_core::data` is the executable specification of §5, not the shipped data path
**Design:** §5 — the deliver contracts, the one-chunk holdover, boundary-only policy.
**Reality:** nothing outside `core/src/data.rs` references `HostwardConsumer`,
`HostFanout`, `MockConsumer`, `TargetwardSink`, `Holdover`, `BusyBoundary` or `Delivery`; only
the `Chunk` type alias escapes the module. The daemon implements the same semantics directly
on bounded `tokio::sync::mpsc` channels in `runtime.rs` and each `nodes/*.rs`.
**Decision:** recorded rather than changed here, with two consequences a reader must know.
(a) **§3.3 above asserted something untrue and is corrected in place:** the runtime does *not*
call `TargetwardSink::flush()`; the anti-stranding property it describes is real but is carried
by the channel-plus-`send().await` shape, and `flush()` exists only in the model. (b) The `data`
property tests are evidence about the *contract*, not about the shipped boundaries. Treat a
change to §5 semantics as requiring edits in both places. `data.rs`'s module doc now says all
of this at the top of the file, so a reader cannot mistake a green property test there for
coverage of the data plane. (Opus review `docs/26`, F3/LOCK-3.)
**Since:** the divergence this entry warned about — the per-node hostward fan-out hand-rolled
five times, only the serial copy counting the all-sinks-closed case, which is how the map
shipped without an unattached-loss counter — is closed: all five producers now broadcast through
`runtime::fan_out`, which charges that case inside the helper (§16.1 applied to the data plane,
F1/DM-3). The targetward half is still per-node, so the two-places rule above still stands.

### 3.19 Two crates expose an `unstable_fuzz_api` module the design says should not exist
**Design:** §15.26 — "the supported extension surface is exactly two contracts, both semver'd:
`serial-nexus-codec-api` for in-process codecs, and the narrow `serial-nexus-daemon` entry API (run options,
registry, version constants); **everything else stays private**."
**Reality:** `serial-nexus-daemon` and `serial-nexus-web` each expose a `pub mod unstable_fuzz_api`
re-exporting internals — the control-socket line framer (`RequestLines`, `LineRead`,
`MAX_REQUEST_LINE`) and the web console's HTTP head parser (`read_request`, `Request`,
`MAX_HEAD`, `split_authority`, `origin_matches_host`). `serial-nexus-web` also gained a library
target beside its binary so that its module can exist at all.
**Decision: operator's call, taken deliberately, reversing the disposition this file carried
one commit earlier.** Review 26's SEC-7 observed that all four fuzz targets sat on the
`serial-nexus-codec-api` layer, so every parser reachable *without* a leg was unfuzzed. Three of those were
closed with no API change. The remaining two — the daemon's front door, and the most
network-exposed parser in the project — were declined on §15.26 grounds, with the note that
"the honest move is to lift the reader into a crate of its own, not to widen `serial-nexus-daemon`."
That rule is now suspended for these two modules: the stability promise is carried by a doc
comment on each module stating in its first line that nothing inside is supported, semver'd, or
guaranteed to exist next release, and that an embedder must not use it. The parent modules
(`control`, `server`) stay private, so the *only* way in is the named re-export.
**Why this is defensible rather than an erosion.** The §15.26 boundary exists so that internal
churn cannot break an embedder; it is not an end in itself. A module whose name is
`unstable_fuzz_api`, whose docs disclaim stability in their first sentence, and whose single
in-tree consumer is `fuzz/`, does not create the coupling §15.26 protects against — no embedder
can plausibly depend on it by accident. The alternative (extracting two parsers into crates)
buys the same coverage at the cost of two more workspace members, two more published surfaces,
and a seam through the middle of `control.rs` and `server.rs` that exists for a test harness.
**What it bought immediately.** `control_request_lines` and `web_http_head` (1.8M and 2.2M runs
clean). The first target *also* refuted its own author: it asserted a framed line never retains
a `\r`, and the fuzzer produced `\r\r` before EOF in seconds — `take_line` strips exactly one
trailing CR, which is harmless (CR is JSON whitespace and `parse_incoming_request` is the only
consumer) but was not what the target claimed. The assertion was wrong, not the framer; the
target now asserts the framer/parser *composition* instead, which no other target covers.
**Watch item:** if either module grows an item that is not a parser under fuzz, that is the
erosion this entry exists to catch. The rule is: a re-export here must have a target in `fuzz/`.

### 3.20 Justified deviations confirmed by review 32 (2026-07-27)

The third comprehensive review (`docs/historical/32-claude-opus-code-review.md`, 99 candidate findings each
adversarially verified on a frozen tree) produced ten refutations and two already-known dispositions.
Several of those exist because the **code is right and something else is wrong** — the design text, a
doc comment, or the finder's model of the system. They are recorded here, in the same slot family as
§3.1–§3.19, so the next review does not spend the effort again. Each names the finding id it retires.

**(a) `load` deliberately does not resolve device identity; only `add-node` does.** *(review 32
`CORE-1`/`DEV-1`, refuted.)* `Daemon::load` runs `config.validate()` + `precheck_codecs` and never
calls `resolve_input`, so a malformed identity string (`usb:0403:6001`) loads and the node sits
`waiting`, while the same string is refused `-32602` by `add-node`. That asymmetry looks like a
validation hole and is in fact §12's central rule: *"adding a node by raw path requires the device
present at that moment (identity must be captured); adding or loading by identity never does, which is
why dump emits identities and why configurations survive cold starts with hardware unplugged."*
Making `load` resolve would break cold-start recovery — the property the whole identity design exists
for. The destructive-typo path this was feared to open is closed elsewhere and structurally:
`deny_unknown_fields` refuses a mis-typed table with `-32602` **before** `--replace` reaches
`teardown_with` (verified live; the running graph survived). Do not "fix" the asymmetry.

**(b) The §11 empty-parse refusal belongs in the CLI and in `startup_load`, not in the `load` verb.**
*(review 32 `RV-2`, refuted — reviewer-originated and correctly killed.)* §11 states the rule as
"a non-empty source that parses to an *empty* graph is refused rather than obeyed", and its predicate
is literally `config.is_empty() && !text.trim().is_empty()` — it needs the **source text**. The RPC
verb receives a deserialized `GraphConfig`, never TOML, so the daemon cannot evaluate the rule even in
principle; `config.rs`, `graph.rs` and `ctl/src/main.rs` all say so in comments. What
remains reachable over raw RPC is a client *deliberately* sending `{"config":{},"replace":true}`,
which is an operator act, not a typo. `docs/rpc/configuration.md` already documents the split. If
anything changes here it should be design §11's wording, not the daemon.

**(c) `serial-nexus-sys::poll_ready`'s `unwrap_or_else(PollFlags::empty)` cannot silently swallow a real
readiness bit.** *(review 32 `DOC-1`, refuted.)* nix's `revents()` returns `None` — not a truncated
set — for any bit it does not model, which reads like a latent data-loss bug. It is unreachable:
`poll(2)` masks `revents` to `requested | POLLERR | POLLHUP` (plus `POLLNVAL`), and every call site in
this tree builds its interest from nix-modelled flags. Measured during the review: `events=POLLIN` on
a hung-up socketpair yields `revents=0x0011` with POLLRDHUP (0x2000) *absent*; asking for POLLRDHUP
makes it appear. So `probes.rs::revents_label`'s residual-hex branch is defensive, not dead-by-mistake.

**(d) `GraphConfig::validate`'s `if let` chain does route a new numeric knob to invariant 13.**
*(review 32 `SIMP-4`, refuted.)* The chain is non-exhaustive, so it looks like invariant 13 ("a new
numeric knob gets a range here on the day it is added") is prose the compiler cannot enforce. Settled
by experiment on a *copy* of the tree: adding a new `NodeConfig` variant produces four `E0004`
non-exhaustive-match errors **in `config.rs` itself**, and adding a numeric field to an existing
variant fails `cargo test --workspace` at seven initializer sites including the range-check test. An
author adding a knob is routed to the right file by the build, not by memory.

**(e) `EndpointLock::register` over a live registration is unreachable from the daemon.** *(review 32
`LOCK-1`, refuted.)* Re-registering an attached origin does corrupt the holder/`may_write` pair when
driven through the pure `serial-nexus-core` API, but two deliberate mechanisms prevent the daemon from ever
doing it — `SEND_ORIGIN_BASE` (so a transient CLI origin cannot collide with a real one) and
`next_edge_origin`'s floor taken from `Wiring`'s own counter — and no verb mutates a registered
origin's write mode. The remaining `register` call sites are `#[cfg(test)]`.

**(f) `tap.open`'s `epoch` is covered, indirectly but genuinely.** *(review 32 `ITEST-2`, refuted;
`TESTR-1` records the same observation.)* `grep -rni epoch itest/` really was zero hits **when this
was written**, which looks like a brand-new protocol field shipping unguarded. But
`p8_tap_offsets.rs::tap_offsets_reset_on_load_replace_while_the_instance_nonce_does_not` already
asserts `tap.closed` with reason `"graph replaced"` — emitted **only** when the hub is dropped — *and*
`from_offset == 0` on the reopen, which catches hub-reuse and offset-restart from opposite directions.
A direct epoch assertion is still worth adding (it is cheap and names the contract), but this is an
improvement, not a hole. **It was added:** the remediation's own `p12_tap_replay.rs` asserts the
epoch directly (`ack2["epoch"] == ack1["epoch"]` across a reopen), so the grep above now returns
six hits and the paragraph stands as the record of a refuted finding rather than a live gap.

**(g) The `unstable_fuzz_api` disclaimer is where an embedder would meet it.** *(review 32 `DOCR-7`,
refuted.)* `lib.rs`'s crate doc says the entry surface is "the only public API" while
`pub mod unstable_fuzz_api` exists, which reads as a contradiction an embedder could be misled by. The
scenario requires finding the module through rustdoc's module list — which is exactly where the
module's own first-line stability disclaimer renders. §3.19 above already carries the exception and
the rule that bounds it.

**(h) The web console *is* documented for operators.** *(review 32 `DOCR-8`, refuted.)*
`packaging/README.md` carries a dedicated web-console section under its operator-facing "what changed
for operators" heading, including the treat-the-token-as-shell-access warning. The finding reached the
opposite conclusion by grepping only the binary name; the document names it in prose. Recorded because
"grep found nothing" is a recurring way a documentation finding goes wrong.

**(i) The `web-ui` gate is not known to be flaky.** *(review 32 `ITEST-3`, refuted.)* Two intermittent
failures were reported against `lifecycle.spec.mjs:61` / `graph-editor.spec.mjs:145`. The verifier
could not reproduce in **100 consecutive gate runs**, 30 of them CPU-constrained at loads well above
the original observation — and a hang-shaped failure should get *more* likely under contention. The
two observations remain unexplained. **If CI ever shows this, treat it as the residual load-sensitive
lead §15.36 already records rather than as a new mechanism, and capture the failing spec name before
re-running.** Do not add a retry: `retries: 0` is deliberate (§15.36).

**(j) Two items were already dispositioned and were re-filed anyway.** The exec stdin feed losing an
in-flight chunk uncounted (`EXEC-1`) was verified and cleared in review 26; `Daemon::state()`'s
per-node endpoint rescan (`SIMP-5`) is review 19's `OPSIMP-3`, kept deliberately per §3.14. Both
verdicts stand unchanged.

**What this entry does *not* cover.** Review 32 confirmed 87 findings, and those are defects to fix,
not deviations to justify — including four the design text gets wrong in the *other* direction
(§17's browser-history key naming the epoch, §7.7's existing-terminal written as shipped and missing
from §14's deferred list, §17's console rail promising node status, and §17's blur-closes-taps
grace interval). Those are listed in the review's §2 with an (a)/(b)/(c) classification each; they
belong in the design, not here.

### 3.21 The pty's blocking writer thread still bypasses `boundary::BlockingReader` (review 32 SIMP-3, partial)

SIMP-3 had two halves and only one was taken. The half that landed is the uncontroversial one the
finding's own verifier preferred over the finder's suggestion: `PtyNode::drop` now calls
`self.teardown()` — idempotent, and covering the join and the symlink unlink — instead of re-deriving
`signal_stop`'s two statements, matching `LogNode::drop`. The half deliberately **not** taken is
rebasing `PtyNode`'s writer thread onto `boundary::BlockingReader`, which would have meant renaming
that type (its `lost` counter and its reader-shaped doc are not writer-shaped) and touching `serial.rs`
mid-remediation for a nit-severity cleanup. So the state of affairs is: `BlockingReader` is used by
`serial.rs` alone, and `PtyNode` (stop flag + join handle + spawn-with-EAGAIN-fault + join-before-fd-drop)
and `LogNode` are a second and third hand-rolled variant of the same pattern.

The cost is stated rather than hidden, because it is the cost §16.1 exists to remove: a hardening added
to `BlockingReader` — its re-arm `debug_assert`, say — reaches one of the three. **This is recorded as a
deviation rather than as a to-do**, which means a future session may close it, but should close it as
the §16.1 item it is (rename to something direction-neutral, make the loss counter optional, rebase all
three, keep `SerialNode::drop`'s join-after-abort ordering) rather than as a local tidy of `pty.rs`.

### 3.22 Directories dropped the retired vocabulary; plan §17.1's "directories unchanged" gave way

Plan §17.1 said the rename lands "with directories unchanged via explicit `name =` fields", and plan
§17.2 said the meta-gate must fail "on any hit outside `docs/historical/`". Those cannot both hold: a
directory still carrying a retired name is itself a hit, and so is every workspace `members` entry,
path dependency, `.gitignore` line and CI `working-directory` naming one. Design §15.40 and AGENTS.md
§11 both say only that directory names stay **short**, which a rename to short stems satisfies, so
the design was left alone and the plan's clause was amended in place (it now records why).

The layout: `core/`, `rpc/`, `sys/`, `daemon/` (the library), `daemon-bin/` (the seventy-line thin
binary), `ctl/`, `web/`, `sim/`, `doctor/`, `itest/`, with `codec-api/` and `codecs/reference/`
unchanged — neither carried a retired token. Package names are hyphenated throughout, so every lib
target auto-derives to the `serial_nexus_*` spelling §15.40 names as importable; the one wart is
`serial-nexus-daemon-bin`, the package producing the `serial-nexus-daemon` binary, because the
consumer-facing *library* is the one that deserves the clean name and Cargo will not give it to both.
Underscored library packages (`serial_nexus_daemon`) were rejected: two packages differing only by
separator collide on crates.io and read as a typo everywhere else.

**The corollary is the part worth remembering**, because it cost two red gates: a gate that scans the
filesystem spells the **directory**, a manifest or a `use` spells the **crate**, and a `Command::new`
spells the **binary**. A textual rename that does not distinguish them will convert all three to
whichever it saw first.

### 3.23 The P5 certificate's scope is narrower than §15.21's sentence, deliberately (review 37 `37-TOOL-3`)

**Design:** §15.21 (echoed by §13 and plan §3) promises the rig certificate covers "deliberate baud
and parity mismatches proving the error counters observable, break reception, and a modem-line map".
**Reality:** `p5_certify_pair` performs the rate ladder plus a deliberate **baud** mismatch only —
every open is `Parity::None` — and the per-port `break` item is local ioctl acceptance, never
reception into an open peer (`doctor/src/probes.rs`; `docs/serial-nexus-doctor.md` already states
both limits, and the §4 P5 entry above records that `brk = 0` everywhere is structural).
**Decision: keep the probe as shipped and record the narrowing here.** The missing pieces are not
uncovered — parity/framing-error observation and break assertion ride the Tier-3 checklist and the
`crossover_ports()`-gated `serial_hardware.rs` suite, which is where §16.7 wants
sim-unreachable behavior anyway — and widening P5 would put more TX-emitting machinery into the one
probe whose job is certifying the rig *before* anything else is trusted, not exhausting it. The
residue is honest: the certificate proves data integrity and clock accuracy; the checklist proves
the signal repertoire. The design sentence is the stale side; annotate §15.21 (and the §13/plan §3
echoes) at the next design revision — annotate, never rewrite (§15's own rule).

### 3.24 Design §11's "resolver-input well-formedness" clause names the wrong verb (review 37 `37-CFG-1`)

**Design:** §11's load-atomicity sentence lists "resolver-input well-formedness" among the checks run
before anything is created.
**Reality:** `Daemon::load` runs `GraphConfig::validate` plus `precheck_codecs` and never touches the
resolver; `resolve_input`'s sole caller is `add-node`. A structurally meaningless identity loads and
sits `waiting`, while `add-node` refuses the identical string with `-32602`.
**Decision: the code is right and stays; this is §3.20(a)'s asymmetry read from the design side.**
Load-by-identity must never require the device — or its identity syntax — to be resolvable, because
that is what lets configurations survive cold starts with hardware unplugged (§12); making `load`
resolve would break the property the identity design exists for, and the destructive-typo path is
closed structurally by `deny_unknown_fields` before `--replace` can reach teardown. What §11's
sentence actually describes is `add-node`'s *capture* rule (§12's "raw path requires the device
present at that moment"). **Landed 2026-07-29:** the design sentence is now qualified in place,
naming this entry.

### 3.25 The leg's receiving-side purge carries per-chunk provenance, not a quiescence drain (review 37 `37-LEG-1`)

**Design:** §6 — "purging is one rule with three instances", one of which is purge-on-reconnect,
which §7.1 defines as draining the parked targetward pipeline *to quiescence*.
**Reality:** a *local* backlog has no record of which connection produced it, so time is the only
available proxy and the drain's yield-redrain rounds are the right shape. A *wire-arriving* queue
does have that record, and approximating it by time is what let a peer that sent and disconnected in
one poll have its whole backlog attributed to the connection that replaced it.
**Decision:** the `faces = "target"` inbound queue carries `Inbound { epoch, bytes }`, stamped with
`LegShared::disconnect_epoch` at **enqueue** on the pump's read half; `channel_targetward` decides
staleness from that tag and discards the backlog one attributed chunk per loop turn instead of
calling `boundary::drain_to_quiescence`. This is exact where the drain was bounded (`DRAIN_ROUNDS`)
and strictly safer in the other direction too: a chunk a *live* connection queued behind stale ones
is now delivered rather than swept up. The sending side and the serial node are unchanged and still
share the helper. §6's "one invariant, three instances" is intact — one of the three now decides
provenance per chunk rather than per drain. **Do not** re-generalize the helper over the element
type and reintroduce a blanket drain here; the blanket drain is the part that was wrong.

### 3.26 Held-priority *reclaim* does not run purge-on-acquire, in two places (review 37 `37-LOCK-1`)

**Design:** §6 — "Every grant, immediate or queued, runs purge-on-acquire before the origin's bytes
flow."
**Reality:** held-priority reclaim is not one of those grants, and now there are two implementations
of it: the pre-existing `runtime::reacquire_held` (the codec, exec and map targetward pumps) and the
new `runtime::may_write_reclaiming`, the non-parking sibling added for boundary origins — a `held`
pty edge — which own their own lifecycle loop and cannot park inside a gate.
**Decision:** neither purges, deliberately, for three reasons that all point the same way. A held
origin's floor is *permanent* and a steal is a transient ouster of it, not a fresh acquisition;
§6's purge paragraph scopes the rule to "explicit lock acquisition" in its own words; and a boundary
observes the lock freeing at poll resolution rather than at the instant it happens, so purging on
reclaim would discard console input typed *after* the endpoint was already this origin's again —
loss §5 does not sanction. The rationale is stated at the new helper so the next reader finds it
there rather than here.

### 3.27 A log node whose directory scan cannot run faults at create (review 37 `37-LOG-2`)

**Design:** §7.3 recovers the rotation counter by scanning the directory at node start.
**Reality:** `scan_rotation` swallowed `read_dir` failure as `None`, indistinguishable from "no
rotations yet" — and the two are reachable independently, because they are separate permissions: a
mode-0300 directory grants create-and-traverse without list, so `read_dir` fails while `open_append`
succeeds. The node came up Active with the counter reset, and the next `rotate` renamed the live file
onto the newest rotation on disk, which `rename(2)` replaces without a word: the log node destroying
the one thing it exists to keep.
**Decision:** the scan returns `io::Result<Option<u64>>` and a scan that could not run faults the
node — §7's environmental-failure rule, exactly as an unopenable file already does — which also makes
`rotate` refuse for as long as the fault stands. The open is still attempted first, so a *missing*
directory (where both fail) keeps naming the open, the more useful diagnosis. New reason spelling:
`scan <directory> for rotations: <err>`.

### 3.28 The listen role's bind retries on the reconnect backoff (review 37 `37-LEG-2`)

**Design:** §7.4 says "the connect role retries with backoff; an outage is faulted-and-wait", and
§11/§15.8 generalize environmental faults to "visible in state, healing on their own".
**Reality:** the listen-role bind was one-shot — a transient EADDRINUSE, EMFILE or not-yet-up
interface address faulted the node and returned the supervisor permanently. It was the daemon's one
environmental fault that could be cleared only by remove-and-re-add.
**Decision:** the bind moved inside the supervisor's retry loop, sharing the existing `Backoff` with
the connect role's dial; `bind_listener` re-runs the SEC-8 stale-socket check on every attempt, which
is what makes the common heal — a predecessor's inode outliving the peer that was still answering on
it — work at all. A successful bind sets `waiting`, so the heal is observable rather than silent.
§7.4's sentence names only the connect role's retry because the bind used to be one-shot.

### 3.29 A harness never reads a byte counter after closing the client that produced it

**Design:** §5 says every byte is accounted where it is lost, and §7.2 gives the pty reader the
drain-before-close ordering that makes a closing client's residual reach the graph rather than the
purge. Neither says anything about what a *test* may assume of the kernel between those two events,
because from the daemon's side there is nothing to say.
**Reality:** `p8_map`'s `a_read_only_map_leaves_its_writers_pty_alive` typed 64 bytes at a pts,
closed the slave, and only then waited on the map's `discarded_no_raw_edge`. That asserts *the bytes
survived the slave's last close* — which no kernel promises — in place of *the bytes flowed during
the live session*, which is the property the map actually offers. Linux retains them (measured at
HEAD on 7.0.0-29: `close(2)` on the slave returns in ~1 µs and the master still reads all 64, then
EIO), so on Linux the guard could not fail for this reason and never had, and Darwin is under no
obligation to agree — which is enough to make the assertion wrong wherever it runs.

**What this entry deliberately does not claim.** It does not claim to explain the 08-03/08-04 macOS
red (29 observed CI passes and no observed failure 07-26 → 08-02, then two reds with every targetward
counter 0, on an unmoved tree and a byte-identical runner image). Read against XNU's source rather
than this repo's summary of it, `ptsclose` runs `ttylclose` → `ttywflush` → `ttywait` *first*, which
**waits** up to `t_timeout` (60 ticks, ≈0.6 s) for the master to drain and only flushes destructively
if that wait fails or the fd is `O_NONBLOCK` — so for the blocking fd this test opens, the expected
Darwin outcome of the *old* ordering is green, matching 29/29, and a red means the reader did not
drain inside ~0.6 s. That is a **reader stall**, a different defect class. `docs/macos.md`
(2026-08-04) carries the source excerpt, the correction to this repo's standing mechanism claim, and
the probe that separates the two hypotheses.
**Amended 2026-08-04: that source reading is now a measurement, and the *green* half is explained.**
P13 on Darwin 24.6.0 reports `policy: waits-then-discards`, `close_waits_for_reader: true`, and
600104 µs with 0 of 64 recovered against no reader — 60 ticks at the `hz = 100` this box reports —
against 23 µs with 64 of 64 when the reader drains first and 29 µs with 0 of 64 for an `O_NONBLOCK`
slave, which measures the `ttylclose` branch as an A/B instead of inferring it
(`docs/doctor/macos-24.6.0-2026-08-05-tier3.json`, binary `fa4b12d6f529`, probe set
`a131e1f4b46d6c83`). Against the daemon's reader cadence (`ACTIVE_POLL` 200 µs doubling to
`IDLE_POLL` 5 ms, `runtime.rs`) a 601 ms window is ~120 idle-poll periods of slack, so 29/29 green is
the **predicted** outcome rather than the anomaly this paragraph treated it as. What stays unclaimed
is the **red**: it is now pinned to a ≥601 ms reader stall, but no P13 ran on `macos-26-arm64`, that
lane is a different kernel and architecture, and none of P13's three shapes covers a reader arriving
*during* the close-wait — which is the shape the failing test actually inhabits. Also note the scope:
P13 measures the targetward direction only, so it does not speak to the hostward/`FREAD` claims this
entry and `docs/macos.md` delta 3 make about the control packet. The reorder below is justified as removing a latent
proxy-in-time defect, **not** as a root-cause fix; it may also mask the stall signal, which is stated
there in as many words. This is §9's "proxy in time" in its third dress, after the two
`docs/macos.md` already records — its delta 4 (a Linux-only closure predicate) and its delta 6 (an
RST scored as a live-but-silent peer).
**Decision:** the observation moves *above* `drop(client)` — the assertion stays below the lifecycle
ones so the more specific failure reports first, which is presentational and nothing more — and the
ordering is enforced
**by the compiler, not by a comment**. The wait lives in `settled_while_open(rpc, &client, ..)`,
which borrows the client; `drop(client)` moves it, so moving the call below the close does not merely
weaken the test, it fails to build with `error[E0382]: borrow of moved value: client`. Proved by
regressing it: the file does not compile. That is deterministic on every platform and costs nothing
at runtime. The failing assertion now also dumps the **pty** node beside the map, because "every
counter on the map is 0" cannot by itself separate *the bytes never reached the daemon* from *the pty
read them and accounted them elsewhere* — `discarded_targetward` and `discarded_at_last_close` live
on `p0`, and the 08-03 dump omitted them. **The rule this generalizes to, for anyone writing the next
one: a byte counter is read while the client that fed it is still open.**

**What the reorder costs, stated rather than glossed.** The *assertion* becomes logically stronger —
the map's counters are monotone `Cell<u64>` bumped only by `.add`, so "flowed during the live
session" implies "reads 64 afterwards" and not the reverse. The *test* does not become uniformly
stronger, and cannot: the two observations are not both available on one run. Waiting for the counter
before the close inserts an RPC round-trip there, which guarantees the daemon has drained the master
*before* the close happens — so this test no longer traverses the write-then-immediately-close
residual path, the one `nodes/pty.rs` describes as "a closing writer's residual must still be
forwarded (not purged) before the close is finalized". It never covered that promise deliberately: it
traversed it by accident, and only on kernels that retain, which is why the coverage evaporated the
moment a kernel disagreed.

**That gap is closed in the same change set, not merely recorded.** Adversarial verification found it
by planting the promise's natural regression — gating the drain on `now`, a plausible "consistency"
edit since the sibling `TIOCPKT_IOCTL` branch really is gated that way — and showing the reordered
test passes it. `p8_map`'s new `a_closing_writers_residual_is_forwarded_not_purged` fails it, with the
signature the promise predicts: `purged: 64` on the origin where `discarded_no_raw_edge: 64` belongs.
Fail-first proof, both directions, on the same tree:

| against a daemon with the drain gated on `now` | result |
|---|---|
| `a_read_only_map_leaves_its_writers_pty_alive` (reordered) | passes — the coverage loss, demonstrated |
| `a_closing_writers_residual_is_forwarded_not_purged` (new) | **FAILED**, `"purged": 64` |

That test is the one place in the suite that deliberately closes *before* reading a counter, so it
carries the exception in its doc comment: the guard is meaningful only where the kernel retains, and
that is no longer an assumption — **doctor P13 measures it** (`retains` on Linux 7.0.0-29), so the
`#[cfg(target_os = "linux")]` gate has a measurement behind it rather than a platform prejudice. Read
P13 before extending it anywhere. The other cost of the reorder is cosmetic: a failing `never` arm
now takes ~20 s rather than ~10 s, because the pre-close wait spends its budget before the lifecycle
wait spends its own.

**A dynamic Linux referee was tried first and withdrawn — record it so it is not retried.** The idea
was to precede the close with `tcflush(TCOFLUSH)`, "the same `ttyflush` XNU performs at last close",
so a Linux run would punish the bad ordering instead of tolerating it. It is not the same operation.
On Linux a slave-side `TCOFLUSH` reaches `pty_flush_buffer`, which empties the *peer's flip buffer*
and never the master's ldisc `read_buf` — so it is a race against the flip-to-ldisc push, not a queue
flush. Measured on this box, 300 trials per row: it destroys the bytes **100% at a 0 µs delay and 0%
at 20 µs and every delay beyond** (20 µs, 50 µs, 100 µs, 200 µs, 500 µs, 1 ms, 5 ms, 20 ms). In the
shipped position it would run an RPC round-trip after the write — `wait_until` alone sleeps 20 ms
between probes — and so destroy nothing whatever: **dead code that reads like a guard**, which is
worse than no guard. It also bought `itest` a `nix` dependency for the safe `tcflush` spelling
(invariant 3 / §16.3 confines `unsafe` to `serial-nexus-sys`), now reverted. The borrow is strictly
stronger, and the withdrawn approach is a good illustration of §9: an emulation is evidence only once
you have measured that it emulates.

The sweep behind that rule, because one fixed instance is not a fixed class. Seven further guards
share the shape and are **latent, not broken** — each is either Linux-gated or rig-gated, so none is
red today, and none is touched here:

<!-- ANNOTATION 2026-08-05 (§5): *Dispositioned in §3.56 — and "none is red today" went stale before
     it was.* Row 3, `p4_free_for_all`, stopped being latent when `serial_pair` gained the rig
     fallback: design §15.48 records it failing **12 of 12 on Darwin**, losing 5–31 bytes of 32768,
     against 20 of 20 on Linux over the same rig at the same commit. Read "latent" as "latent on the
     kernel this table was written on". Five of the seven rows are converted in §3.56 (1, 2, 3, 5,
     7); rows 4 and 6 are **not convertible** and become documented exceptions there, because the
     counters they assert come into existence *because* of the close and read 0 before it. The line
     numbers below are from 2026-08-04 and have all drifted; the tests are named in §3.56. -->

| where | what closes | asserted after the close |
|---|---|---|
| `itest/tests/serial_hardware.rs:104` `inject_verify` (both rig callers) | a one-shot `Sim::client --send`, no `--hold-ms` | the whole 32 KiB arrived, SHA-256-exact |
| `itest/tests/p4_exclusivity.rs:321` | the holder's one-shot `Sim::client` | sink `received == 65536`, SHA-256-exact |
| `itest/tests/p4_free_for_all.rs:130` | two background writers that exit on completion | sink `received == TOTAL` |
| `itest/tests/p4_purge.rs:241` | `drop(client)` on a locked-out holding `Sim` | `purged == 2048` **exactly** |
| `itest/tests/p4_purge.rs:347` | one-shot `Sim::client` | sink `received == 2048`, SHA-256-exact |
| `itest/tests/p12_pty_setup.rs:1017` | `drop(a)` on a never-reading client | `discarded_at_last_close > 0` |
| `itest/tests/p6_outage.rs:331` | one-shot `Sim::client` | `purged_on_reconnect > 0` |

`serial_hardware.rs:228` already half-knew — it warns that "only rates fast enough to drain before
the one-shot `Sim::client` injector closes its pty are reliable here" and attributes the race to
*baud*, when the variable is the kernel's retention policy. `p12_pty_setup.rs:1017` must **not** be
ported to macOS whatever else is done: `discarded_at_last_close` is structurally always 0 there
(`docs/macos.md` §3), so the counter it asserts has nothing to name.

<!-- ANNOTATION 2026-08-05 (§5): the `serial_hardware.rs` comment named here is **corrected in
     place** by §3.56, which states the variable (the kernel's last-close policy, P13) instead of
     the rate, and records that the test no longer depends on either. The `p12_pty_setup.rs`
     sentence stands unchanged and is restated in that test's own doc comment. -->


Two shapes that look like the class and are not, so a blanket rule is not misapplied: AF_UNIX
siblings (`p6_outage.rs:601`/`:779`, `p12_leg_accounting.rs:517`, `p6_fragmentation.rs:338`) are
safe because a `SOCK_STREAM` peer's queued data is delivered before EOF — no kernel flushes it; and
`p9_pty_collapse`'s collapsed-session loop and `p9_unwired_interior` write bytes before a close but
assert only lifecycle, which `SessionLatch` carries as an edge (§15.39) where bytes have no such
carrier. A byte assertion added to either lands straight in this class.

### 3.30 P13 measures the last-close disposition, because P7 structurally cannot

**Design:** §7 (and AGENTS.md §7) says a kernel disagreement is settled by a probe that *measures*
rather than by prose that assumes, and §16.13 says a kernel claim in prose cites a committed
`docs/doctor/` artifact.
**Reality:** the tree carried a kernel claim with no probe behind it — that XNU's last close destroys
bytes the master has not read — and the existing set could not have checked it. P7 asks what a
collapsed session leaves *readable* against a master **nobody drains**, and against such a master a
kernel that discards promptly and a kernel that waits for a reader and then discards produce byte-
identical observations: nothing readable, `terminal_read: "eof"`. The two are behaviourally very
different (§3.29): under "discard" a lost byte is a lost microsecond race, while under
"wait-then-discard" it means a reader stalled for the whole timeout, which is a daemon-side event
wearing a kernel costume.
**Decision:** `P13` measures the disposition directly and reports `policy` alongside
**`close_microseconds`**, which is the field that separates the two — a kernel that decided
immediately spends microseconds there, one that waited spends its timeout. Three shapes: no reader,
reader-drains-first, and a no-reader `O_NONBLOCK` slave, that last one because `ttylclose` branches
on exactly that flag, making the pair a controlled A/B on the branch rather than an inference about
it. It never judges: all three policies are legitimate and the daemon is correct under each, so the
verdict is `Supported` whenever the measurement completes and the answer lives in the numbers.
Measured on Linux 7.0.0-29: **`retains`, 7 µs, 64/64 recovered**, agreeing with the standalone C
measurement taken during the §3.29 triage. The classifier is split out as `p13_policy` and unit-tested
across all four quadrants including the pair that differs *only* in the close duration — a classifier
that read the byte count alone would collapse them, which is the conflation the probe exists to undo.
Both expectation files gained a presence-and-status clause (never a required word: pinning the answer
would make a kernel that changed its mind fail the lane instead of reporting the change), and
`macos.jq`'s structural count moved 12 → 13.
**The macOS artifact is no longer owed — it ran, and it answers the question.**
`docs/doctor/macos-24.6.0-2026-08-05-tier3.json` (binary `fa4b12d6f529`, probe set
`a131e1f4b46d6c83`, `2026-08-05T00:22:48Z`, Tier 3 on the FT232R crossover) measures Darwin
24.6.0 / macOS 15.7.8 x86_64 at **`waits-then-discards`, `close_waits_for_reader: true`**:
shape `a_no_reader_blocking_slave` **600104 µs / 0 of 64**, shape `b_reader_drains_before_close`
**23 µs / 64 of 64**, shape `c_no_reader_nonblocking_slave` **29 µs / 0 of 64**. The two kernels
differ by ~86000× in `close_microseconds`, which is exactly the separation this entry says the
field exists to provide, and shape `c` makes the `O_NONBLOCK` branch an A/B measurement rather
than an inference. The figure is stable: five captures on that box read 600115, 600249, 600363,
601087 and 601095 µs, against `sysctl kern.clockrate` reporting `hz = 100, tick = 10000` — so
XNU's `t_timeout` of 60 ticks predicts 600000 µs and the probe measures the predicted timeout
plus scheduling slop. **Both asymmetries this entry recorded are now discharged (2026-08-05).** They were: that the
Linux figures were not artifact-backed, and that the macOS artifact shared its fingerprint with
no committed report. Three sequential Linux Tier-3 captures at `71fc5a815852` — 
`docs/doctor/linux-7.0-2026-08-05-tier3{,-2,-3}.json`, probe set `a131e1f4b46d6c83`, taken on the
dev box with the FT232R crossover attached — close both: the Linux side is citable, and it carries
the *same* fingerprint as the macOS report, making that pair the first lawful field-by-field
cross-kernel diff of the P13 era. Linux reads `retains`, `close_waits_for_reader: false`, 64/64
recovered in **all three** shapes, with the no-reader close at 20/10/13 µs across the three
captures. The "7 µs" quoted above is within that shape's ordinary spread and was never a
reproducible digit — which is the entry's own point: the classifier keys on the
microseconds-versus-hundreds-of-milliseconds gap, and the two kernels differ by ~40000x on it.
**One asymmetry replaces them, and it is P10's, not P13's.** A shared fingerprint certifies two
runs asked the same questions; it does not certify they asked them of the same configuration. See
§3.34 — the macOS P10 block predates the baseline repair and must not be diffed until a macOS
capture reports `slave_termios_mode: "raw"`.
<!-- ANNOTATION 2026-08-05 (§5). Discharged: `docs/doctor/macos-24.6.0-2026-08-05-7ead470-tier3{,-2,-3}.json`
     report `raw` on both directions, and the P10 diff now reads ~15x with Linux the deeper kernel
     (15360 both directions against 1024 targetward / 1022 hostward). See the annotations on §3.34.
     One clause of this entry's neighbourhood was separately stale and is corrected there too: the
     §4 P13 bullet's "the Linux side … is **not** yet artifact-backed" stopped being true at
     `71fc5a815852`. -->
<!-- ANNOTATION 2026-08-05 (§5). This entry's Linux P13 close figure is quoted as "7 µs" in the §4
     bullet and in `expectations/macos.jq`; no committed capture contains it. The three artifacts
     read 20/3/15, 10/13/15 and 13/2/19 µs for the (a)/(b)/(c) shapes. The gate file is corrected
     (§3.36); the ~40000x ratio above survives it, since it is driven by Darwin's ~600 ms against
     Linux's tens of microseconds either way. -->

**Disposition note (§5):** this measurement gets **no new numbered §3 entry**. It changed no
implementation decision — `waits-then-discards` is one of the three policies this entry already
declares legitimate — so it is an amendment to the entry that made the prediction, not a
deviation. Read the absence of a new number as deliberate.

### 3.31 An interior node's queued targetward bytes died uncounted at teardown — fixed

**Design:** §5 forbids dropping targetward and requires every loss to be visible and
attributable. `runtime.rs`'s `TargetwardLoss` states the operational form verbatim: "Targetward is
the direction §5 forbids dropping on, so *every* non-delivery exit of *every* pump has to reach a
counter" — a type that exists because the obligation "was a review convention rather than a compiler
rule … and twice shipped with an exit that charged nothing".
**Reality:** this was the third instance, and it evaded that type completely. Every interior pump
took its `mpsc::Receiver<Chunk>` **by value**, which moved it into the spawned future. So
`TaskSet::abort_all` — the whole of `signal_stop` for every kind but `log` — dropped the receiver
*and every chunk queued in it*, and no exit of the pump ran to charge them. The pump bodies were
scrupulous; they simply never got another turn. Reproduced on the shipped daemon, one
`remove-node --cascade` on a saturated map: **808 448 bytes in flight, 23 042 accounted, every node
counter `0`** and the reply's `purged_bytes` structurally unable to cover it (`Node::purge_origin`
returns 0 for every non-pty kind). Found by adversarial verification of the §3.29 work and
reproduced before being believed.

**Decision — the shape, and why it is this shape.** Two constraints ruled out the obvious fixes:

* *The node cannot drain the queue*, because it does not hold it — the receiver is inside the
  future. So the receiver now lives in a shared slot, `TargetwardInbox`, that the node keeps a
  handle on: `TeardownLoss::watch(rx)` at the spawn, `drain()` at the top of `signal_stop`.
* *A `Drop` guard on the receiver would charge too late.* Aborting a task does not drop its future
  synchronously — the `LocalSet` gets to it after the enclosing critical section returns — and
  `remove-node` is a **synchronous** handler, so its reply would be composed before the charge
  landed. Worse, on that path the node is destroyed, so a counter living on the node has no reader
  left. The count has to exist at the instant the operator asks for it.

`TargetwardInbox::recv` reaches the receiver through `poll_fn` + tokio's `poll_recv`, so the
`CriticalCell` borrow is taken and released *inside* a synchronous closure on every poll and never
spans an `.await` — invariant 1 / §16.2 upheld structurally rather than by hand, with no new
`unsafe`. Draining is `try_recv` to exhaustion: synchronous and non-blocking, so it belongs in the
cheap half of teardown exactly as `abort_all`'s doc requires, and it is idempotent because the
removal path calls `signal_stop` twice. **Order matters and is stated where the swap happens**:
drain *before* `abort_all`, since abort is what drops the future the queue lives in. Draining first
also ends the pump *gracefully* — an emptied inbox answers `None`, so the `while let` finishes by
itself — which makes the abort a backstop rather than the mechanism.

The chunk a pump is holding *mid-flight* is counted too: `recv` records its length and clears it at
the top of the next `recv`, the one place a pump can be in only when the previous chunk's fate is
settled. The one inaccuracy is the sliver between a successful delivery and the next `recv`, where
this over-reports by one chunk — deliberate, and the same direction the codec's residual rule
takes: err toward reporting loss, never toward hiding it.

**Where it surfaces.** A new `discarded_at_teardown` on `map`, `codec` and `exec` in `state`, and
the same figure in the `remove-node` reply — because `state` cannot report the last loss of a node
that no longer appears in `state`. `docs/rpc/observation.md` and `docs/rpc/configuration.md` carry
both, including the sentence that `purged_bytes` and `discarded_at_teardown` are different losses
and must not be summed: the first is §6's deliberate purge at the edges, the second is what the
node's own pump had accepted and not yet delivered.

**Guard.** `itest/tests/p13_teardown_accounting.rs`, three tests, device-free and deterministic on
every platform. The determinism comes from two choices worth keeping: a map whose *raw* side is
unattached parks its pump inside `await_origin` (§5 forbids discarding for a detached edge, so it
must stall its writers), and the bytes go in through **`send`**, which is RPC-acked — so "in flight"
is a fact the harness observed rather than a timing assumption, and the assertions are equalities
rather than thresholds. One of the three is the conservation law itself: destroyed + purged + pty
== queued. Fail-first proof, by removing `TeardownLoss::drain()` from `MapNode::signal_stop` in
place and rebuilding: **3/3 fail**, with `destroyed 0 + purged 0 + pty 0 must equal the 2560
queued` — the shipped defect, verbatim.

**Exactly what is covered, because "map, codec and exec are fixed" would be too broad.** What is
drained and counted is each node's **host-facing targetward queue** — the arbitrated
`mpsc::Receiver` an endpoint's writers feed — plus the chunk its pump is holding. That is the whole
of the map's and the codec's targetward exposure. It is *not* the whole of exec's: exec's
per-channel forwarders pull from those queues and push into an internal merged
`mpsc::Receiver<(String, Chunk)>` (`CHANNEL_CAP` again) that `pump_child` reads, and a chunk that has
moved into that second stage is beyond this handle's reach. So exec's number is a floor, not a
total, and closing the rest means giving the merge stage the same treatment — the same shape of work
as `serial`/`leg` below. Stated here rather than discovered later by someone diffing the counter
against a conservation sum.

**What is deliberately NOT fixed here, and why not half-done.** `serial` and `leg` own targetward
queues of the same shape and lose them the same way. Their receivers are also fed to
`boundary::drain_to_quiescence` on the purge-on-reconnect path (§7.1, §7.4), so adopting the shared
inbox means moving that helper onto it — a §16.5 one-rule-one-place change rather than the four
lines the interior kinds needed. They therefore report nothing at all rather than a counter that
reads `0` while bytes are being destroyed, which would be worse than the silence it replaced. The
pty's sibling is different again and not fixable this way: its undelivered payload is a `pending`
slot inside the reader's own stack frame, not a queue anything else can reach.


### 3.32 P10's direction labels were inverted, in the keys and the prose alike

**Design:** a pty node holds the **master** and its client holds the slave; §5's directions are named
from the graph, so client→node→device is *targetward* and device→node→client is *hostward*.
`nodes/pty.rs` settles it operationally: it reads the master and hands what it finds to
`try_forward_targetward`.
**Reality:** `p10_pty_buffer_depth` filled master→slave and labelled it `targetward`, and slave→master
and labelled it `hostward` — backwards, in the observation keys, the consequence sentence and the
function's doc comment together, so nothing internal contradicted anything else. The cost is not
cosmetic, which is why this is a defect and not a typo: P10's stated purpose is to be read when
sizing `hostward_buffer`, and the number printed under that word was the *other* direction's depth.
Found while auditing the probe set during the §3.29 triage, and settled against `pty.rs`'s forwarding
call rather than against the prose that was wrong.
**Decision:** the fill mapping, both keys and both descriptions are corrected, and the reason the
mapping "looks backwards" is stated where the swap happens. The keys are
`slave_to_master_targetward` and `master_to_slave_hostward`. **`probe_set` does not move** — the
fingerprint covers ids and questions, and neither changed — so the committed `docs/doctor/` artifacts
stay diffable; what a diff against a pre-2026-08-04 report shows is the two keys renamed with their
numbers unchanged, which is exactly the correction. Those artifacts are frozen records (§16.13) and
are **not** rewritten: they printed what the tool printed on their date.

### 3.33 The editor spec's status oracle could not see a refusal, and one assertion was vacuous

**Design:** §15.35's whole point at the browser layer, stated in `graph-editor.spec.mjs`'s own header,
is "that the daemon's refusal reaches the operator's eyes verbatim rather than being paraphrased into
'failed'". `editor.mjs` honours it: `say(false, …)` prints `${label} refused: ${error.message}`
because "paraphrasing it here would cost the operator the only precise thing they got".
**Reality:** the spec then threw that away twice over, and the CI failure on `83239a5` is what
surfaced it. `editor.mjs` renders **one** status div and flips its class between `ok` and `err`, so a
locator spelled `.estatus.ok`:

1. **cannot see a refusal at all.** On the refusal path the element stops matching, so the wait burns
   its full 20 s and dies with `element(s) not found` — the daemon's named rule, rendered on screen,
   never reaches the report. The 2026-08-04 `web-ui` failure was undiagnosable for exactly this
   reason; a re-run passed, so it was a flake, but *which* rule the connect broke is still unknown
   and now unknowable. That is §9's proxy oracle: it cannot separate *still pending* from *refused*,
   and it discards the evidence on the one path where evidence exists.
2. **was outright vacuous in one place.** The labels are `connect a ↔ b` and `disconnect a ↔ b`, so
   after a disconnect the status reads `disconnect usb0 ↔ console/raw ok` — which **contains the
   substring `connect`**. The fault test's second assertion, `toContainText("connect")` issued right
   after clicking connect, therefore matched the *stale* line instantly and asserted nothing. Checked
   rather than eyeballed: `"disconnect usb0 ↔ console/raw ok".includes("connect")` is `true`, and
   `/^connect\b/.test(…)` is `false`.

**Decision:** two helpers, `statusOk` / `statusErr`, that wait on `.estatus` *whatever it says* and
then assert the class — so a refusal ends the wait at once and the message quotes it. Proved by
planting one: asserting the illegal connect as a success now fails with `the editor's status line
reports a refusal, not /^connect\b/` and `Received string: "estatus err"`, where the old form gave
`element(s) not found`. Verb matchers are anchored regexes (`/^connect\b/`), which is what closes the
vacuity; a node *name* is unique per run and stays a plain string. The class assertion carries a
short timeout because `say()` sets `className` and `textContent` in one call, so once the text
matches the class cannot still change — a long timeout there would only buy 20 s of waiting for a
settled value. Browser gate re-run 3× locally under `SNX_WEB_UI=required`, green each time.

### 3.34 P10 measured acceptance, not delivery — and off Linux it measured the wrong pty

**Design:** §7 (and AGENTS.md §7) says a cross-kernel disagreement is settled by a probe that
*measures*, and §13 says a differing kernel is `degraded` with the observation named. §7.2 fixes the
configuration the daemon actually runs a pty in: the raw, echo-off, EXTPROC baseline.

**Reality — two defects in one probe, found by diffing the first two reports that were lawfully
comparable.** With `docs/doctor/linux-7.0-2026-08-05-tier3.json` and
`macos-24.6.0-2026-08-05-tier3.json` both at probe set `a131e1f4b46d6c83`, P10 read 11776–15360
bytes symmetric on Linux against **1024 targetward and 4194304 hostward** on Darwin — the latter
`terminal_write: "ceiling"`, meaning it never answered EAGAIN at all and its "depth" is a lower
bound the probe never reached. Read naively that is an order-of-magnitude cross-kernel gap, which
P10's own consequence text calls signal rather than noise. It is neither.

1. **Acceptance is not delivery, and P10 could not tell them apart.** Every field it reported
   counted what `write(2)` took. A kernel that accepts 4 MiB and returns none of it scored
   identically to one holding 4 MiB ready for its reader. The one field that gestured at
   recoverability, `peer_pending_input_bytes`, is a `FIONREAD` best-effort that reads 0 for bytes
   sitting in a queue no reader will ever be given. **The probe's own `/dev/null` self-test is the
   proof it was blind**: a bottomless sink accepted the full ceiling and the test asserted only
   that the fill *stopped*.
2. **Off Linux it was not measuring the daemon's pty.** `apply_pty_baseline` applies the baseline
   through the master where the master is a terminal, and otherwise through a slave it opens **and
   immediately closes**. Darwin is the second case — P2 measures it as
   `termios_settable_without_slave: false` — and Darwin resets slave termios at last close, a fact
   `daemon/src/nodes/pty.rs` states in its own words at the non-Linux re-assert ("a momentary
   daemon-side set does not survive to the client's open"), which is *why* the node re-asserts on
   the client's rising presence edge. P10 then opened a fresh slave and never re-asserted. So its
   Darwin figures are a **cooked** pty's — a configuration the daemon never runs. The tree already
   knew the fallback does not survive; the probe relied on it anyway.

**That the mode is worth an order of magnitude is measured, not argued, and measured on Linux** —
the platform where it can be checked without a Mac. Filling hostward against a slave nobody reads:
**raw accepts ~13.8 KiB and every byte is recoverable; cooked accepts ~23.5 KiB and none of it is.**
Same kernel, same probe, opposite answers. So a depth reported without its line discipline is not a
cross-kernel measurement, and a mode mismatch explains a gap before any kernel difference does.

**Decision.** P10 re-asserts the baseline on **the slave it measures**, not the one
`apply_pty_baseline` opened and closed; reports `slave_termios_mode`; reports
`bytes_recovered_by_peer` and `bytes_unrecoverable` by draining the peer as its last step; names
each direction's actual terminal condition instead of asserting "before answering EAGAIN" when the
run ended at the ceiling; and **degrades** when the measured mode is not raw, because §13 says a run
that could not ask its intended question reports that rather than a confident number. The re-assert
is unconditional rather than `cfg(not(linux))`: a repair that only ever executes off the platform of
record is a §9 proxy in space, exercised nowhere it can be observed failing. On Linux it is
idempotent and the figures do not move — verified, 11776/15360 before and after.

**`probe_set` does not move**, for the same reason as §3.32: the fingerprint covers ids and
questions and neither changed. **That is a limitation here, not a reassurance**, and it is recorded
as one: the instrument genuinely moved and the fingerprint cannot say so. `docs/doctor/README.md`
carries the consequence — the macOS P10 block predates this repair, so that pair must not be diffed
on P10 until a macOS capture at the current binary reports `slave_termios_mode: "raw"` on both
directions. **A macOS capture is the new owed item.**

<!-- ANNOTATION 2026-08-05 (§5). **The owed capture landed and this entry's prediction held.**
     `docs/doctor/macos-24.6.0-2026-08-05-7ead470-tier3{,-2,-3}.json`, taken on the Mac at
     `7ead470f594c` with the FT232R crossover attached, report `slave_termios_mode: "raw"` on both
     directions in all three runs. P10 may now be diffed across the pair.

     **The answer, and it is not the one the pre-repair numbers implied.** At matching modes and
     provably identical probe code, Linux 7.0.0-29 accepts and fully recovers 15360 bytes in each
     direction; Darwin 24.6.0 accepts and fully recovers 1024 targetward and 1022 hostward. That is
     15.0x / 15.03x with **Linux the deeper kernel** — the opposite direction from the pre-repair
     reading, which put Darwin at >=273x deeper hostward against the same Linux artifact and was in
     any case a floor, since `ceiling_hit: true` means the blocking point was never reached.

     **Read the confidence labels, because they differ.** That Linux is ~15x deeper is *measured*
     on both sides. That the pre-repair Darwin run was in a cooked discipline is *inferred*: the
     pre-repair artifact carries no `slave_termios_mode`, so it cannot testify to its own
     configuration. The inference rests on a single-variable source delta — the only functional
     change on P10's fill path between `fa4b12d6f529` and `71fc5a815852` is `set_baseline(&slave)`
     on the slave the probe measures — plus that report's own P2 `termios_settable_without_slave:
     false`, which is the condition selecting `apply_pty_baseline`'s open-and-close path. This
     entry's own body above states the raw-vs-cooked figures as measured on Linux, which they are;
     what must not be written is a *Darwin* cooked figure, because no committed artifact holds one.

     **Something moved that this entry did not predict.** Darwin accepts 1024 targetward but
     **1022** hostward — a two-byte asymmetry, one 4096-byte write in each direction, identical
     across all three runs, where Linux is symmetric. Nothing in the tree explains it and no probe
     currently asks. Recorded as measured and unexplained; §7 says a probe measures rather than
     assumes, and the honest state here is that this one has not been asked.

     **P10 does not vary run to run on Darwin at all.** Its entire observations array is
     byte-identical across the three captures, where the Linux depths needed three runs to separate
     noise from signal. So the §4 P9/P10 bullet's "read a P10 delta as a scheduling artifact" is
     doubly inapplicable here: the modes agree, and there is no run-to-run spread to attribute.
     The annotation at that bullet is **satisfied, not outrun** — its ordering is mode ->
     `bytes_recovered_by_peer` -> scheduling, and this pair clears the first two steps and hands a
     surviving 15x to the third.

     **Still open, and not discharged by any of the above:** the six sibling probes named below.
     Only P10 learned to re-assert. -->
<!-- ANNOTATION 2026-08-05 (§5), on the sibling paragraph that follows. Re-examined against the
     source and the new captures, the six split three ways rather than standing as one block, and
     the ranking matters because it says which repair is worth the risk:
       * **P8 has no Darwin answer to be wrong** — it is `skipped` there with zero observations
         (epoll is Linux-only), so it cannot be contaminated.
       * **P9 carries its own contamination detector** — `ready_passes_total` is 0 in all three
         runs, which is the probe certifying that its never-ready-fd precondition held.
       * **P6's measured window is empty** in every capture on both kernels (`bytes_read: 0`), and
         a cooked discipline acts only on data, so the verdict-bearing half is discipline-
         independent. Inferred, not measured.
       * **P13 and P7's write-a-byte shape are the direction the discipline provably does not
         move**: across the same binary change that moved hostward acceptance 4194304 -> 1022 on
         this box, every targetward P10 field is unchanged at 1024. That is a measured control for
         exactly the direction those two probes write in.
       * **P7's tcsetattr-only shape is the one genuinely compromised stimulus** — it is built from
         ECHO and EXTPROC, the two flags a cooked reset destroys — but its Darwin answer is
         independently corroborated by P1, which configures its own slave raw+EXTPROC and agrees
         (`ioctl_packet_on_tcsetattr: false`).
     **And the obvious repair is wrong for two of them.** Copying P10's `set_baseline(&slave)` into
     P7 and P12 would overwrite the stimulus those probes exist to measure: a slave-side
     `tcsetattr` *is* their input, not their setup. That is why this stays filed rather than swept,
     and the reason is now sharper than "no Mac to measure on". -->
<!-- ANNOTATION 2026-08-05 (§5), refuted diagnosis, recorded per §9. During this session it was
     proposed that `apply_pty_baseline` never takes its fallback branch on Darwin at all — that
     `set_baseline(&master)` returns Ok there and short-circuits at
     `doctor/src/probes.rs:314`. **Refuted by measurement.** Had the master path succeeded, the
     pre-repair Darwin P10 would have measured the same raw pty the post-repair one does and the
     two would agree; they differ by a factor of 4104 hostward. The mechanism this entry states is
     the one the artifacts support. -->

<!-- ANNOTATION 2026-08-05 (§5). This entry's Linux raw/cooked pair — "raw accepts ~13.8 KiB and
     every byte is recoverable; cooked accepts ~23.5 KiB and none of it is" — is **not backed by
     any committed `docs/doctor/` artifact**, and neither is the same pair where it is repeated at
     `doctor/src/probes.rs` (the `termios_mode` doc comment, the shipped P10 consequence string,
     and the guard doc comment). No artifact in the directory carries a *cooked* P10 measurement at
     all, and the raw half does not match the committed Linux figure either: those captures read
     15360 bytes, which is 15.0 KiB, not ~13.8 KiB. The figures are a session-scratchpad
     measurement — the class §7 and §16.13 forbid citing — and the shipped string asserts them in
     every report, including Darwin reports that never took a Linux measurement. `expectations/
     linux.jq` is the correct form and should be the model: it asserts the order of magnitude,
     cites 7.0.0-29, and quotes no figures. The *relation* is properly guarded by
     `p10_recoverability_separates_a_deep_buffer_from_a_black_hole`, which asserts raw-conserves /
     cooked-does-not without either number, so the repair is to drop the two figures, not the
     sentence. Filed here rather than swept: it touches the shipped binary's output, and changing
     what every future report says is a decision to take deliberately, not inside a documentation
     pass. -->

<!-- ANNOTATION 2026-08-05 (§5). One further staleness this entry's neighbours carry: §3.30's
     "the Linux side is recorded in §3.30 and is **not** yet artifact-backed" (and §4's P13 bullet
     repeating it) was true when written and stopped being true at `71fc5a815852`, when
     `docs/doctor/linux-7.0-2026-08-05-tier3{,-2,-3}.json` landed. The 08-05 sweep corrected five
     documents and missed those two clauses. -->


**Not fixed here, and named so it is not mistaken for covered.** Six sibling probes take the same
fallback — P6, P7, P8, P9, P12 and P13 all call `apply_pty_baseline` and then open a slave — so on
Darwin each of them measures whatever the kernel reset the pair to. Their answers are not thereby
wrong (P13's targetward write and P7's readability question survive a cooked discipline far better
than a buffer *depth* does), but they are not known to be right either. Repairing six cross-kernel
instruments in one pass, with no Mac to re-measure on, is exactly the one-way decision on
single-kernel evidence §7 forbids: it would silently move every Darwin baseline in the directory.
The fix here is confined to the probe whose output is demonstrably wrong, and the sibling exposure
is filed rather than swept in.

**Fail-first, both guards, each against the mutation it exists to catch.**
`termios_mode_tells_the_daemons_baseline_from_a_cooked_pty` fails when `termios_mode` is collapsed
to the constant `"raw"` (`assertion left == right failed`), and
`p10_recoverability_separates_a_deep_buffer_from_a_black_hole` fails when the drain is reverted to
the acceptance-only measurement (`a cooked pty returned everything it accepted (21504 of 21504), so
this guard no longer discriminates`). Each mutation leaves the *other* guard green, so they
discriminate independently rather than one firing on everything.

### 3.35 A rig-gated test that never touched the hardware was indistinguishable from one that did

**Design:** plan §3 rule 7 — a capability-gated test may self-skip, but a skip must never be
readable as coverage. `SNX_WEB_UI=required` is the mechanism already in the tree for exactly this,
turning every browser-gate skip into a hard failure so a box with `node` cannot report green for a
gate that never ran.

**Reality, measured on this box with the FT232R crossover physically attached and working:**

```
cargo test --test serial_hardware                          -> 4 passed, 0.00s   (all four printed SKIP)
SNX_CROSSOVER_A=… SNX_CROSSOVER_B=… cargo test --test …    -> 4 passed, 10.39s  (all four drove the wire)
```

Same verdict line, same exit status, 1000x apart in wall clock. `crossover_ports` has an env-var
arm and a `cfg(target_os = "macos")` scan of `/dev/cu.usbserial*`, and **no Linux arm at all** — so
on a Linux rig every hardware-gated test self-skipped, and `docs/macos.md` had already recorded the
asymmetry ("that asymmetry bites on Linux") without it being fixed. Five tests in two binaries:
the four in `serial_hardware.rs` and `p12_serial_exclusivity`'s break-straddled-by-replace guard,
which is the *only* place the break clause is tested against real silicon.

**Decision — make the skip loud, not the detection clever.** The obvious fix, a symmetric Linux
by-id auto-detect arm, is the wrong one and is deliberately **not** taken: two adapters being
present is not two adapters being *cross-wired*, and a harness that opened whatever it found would
transmit at 250000 baud and pulse DTR on equipment it never verified. That is precisely why
`serial-nexus-doctor` is passive until a port is named with `--port` (§3), and a test harness has no
license the diagnostic tool refuses itself. So:

* **`SNX_CROSSOVER=required`** turns every rig self-skip into a hard failure, mirroring
  `SNX_WEB_UI=required` rather than inventing a second mechanism for one problem. A box with the rig
  can now *prove* its hardware coverage ran.
* **The skip message names the candidates it can see** — the by-id nodes on Linux, the `cu.usbserial`
  nodes on macOS — because the one box where the skip matters is the one with the hardware attached,
  and an operator staring at two adapters should not need to already know which variables to export.
  Reported, never auto-selected: naming the candidates is help, choosing them is the operator's.

**Verified on the rig, all three states:** unset → skips naming both FTDI by-id paths;
`SNX_CROSSOVER=required` with nothing named → all four fail with the fix in the message;
required *and* named → `4 passed` in 10.36s, genuinely on the wire.

**Left open deliberately.** The macOS auto-detect arm still opens and transmits on any two
`cu.usbserial` nodes it finds. That is the same hazard this entry declines to introduce on Linux,
and it is load-bearing for the documented macOS validation flow, so removing it is a doctrine
decision rather than a cleanup — filed, not silently changed.

### 3.36 The sweep that corrected five documents did not reach the gate file, or the index

**Design:** §16.13 and AGENTS.md §7 — a kernel claim in prose cites a committed `docs/doctor/`
report by commit, fingerprint and date, never a terminal scrollback. §2 records that on 2026-08-05
five documents were corrected for quoting Darwin P13 figures (`601087/13/28 µs`) that the artifact
they name does not contain.

**Reality — the sweep was scoped to prose, and two non-prose sites kept the defect.**

1. **`expectations/macos.jq` (lines 79-91).** It quoted `a_no_reader_blocking_slave` at
   `601087 us`, `b_reader_drains_before_close` at `13 us` and `c_no_reader_nonblocking_slave` at
   `28 us`, and cited `docs/doctor/macos-24.6.0-2026-08-05-tier3.json` **by name** for them. That
   artifact reads `600104` / `23` / `29`. It further asserted "Linux 7.0.0-29 reads `retains` at
   7 us" with no artifact named at all; the three committed Linux captures read `20/3/15`,
   `10/13/15` and `13/2/19`. `601087` appears in no committed report in the repository, and this
   was its last surviving instance. **Severity, stated precisely so it is neither inflated nor
   waved away:** the numbers sit in `#` comments, so no clause evaluated them and CI neither failed
   nor could have failed on them. The defect is the attribution, not the gate. What makes it worth
   an entry anyway is *who reads this file* — it is where a macOS CI maintainer goes to learn what
   the lane expects, and §16.13 exists precisely so that a number attributed to a named report is
   checkable. Here the check failed.
2. **`docs/doctor/README.md`'s index.** Both macOS rows were labelled "**Tier 3**", matching the
   Linux rows and the filenames. **The string `Tier 3` appears nowhere in any macOS artifact** — it
   appears once in each Linux Tier-3 report, inside P5's own consequence. P5 certifies a pair by
   characterizing each port and its UART predicate is `TIOCGICOUNT`, which is Linux-only, so on
   Darwin every port reports `cert: skipped (not characterizable here)` and the cross-pair
   rate-ladder line is absent from the report entirely. The rig genuinely *is* cross-wired — P5
   pairs both directions, and `serial_hardware.rs` moves 32768 bytes byte-exact each way at 250000
   baud over it — so what is true is the topology, not the certificate.

**Decision.** Both corrected against the artifacts themselves. The macOS rows now read "**Tier-3
wiring, uncertified**", which is not a new coinage: `doctor/src/probes.rs` already emits exactly
that phrase for a cross-wired pair whose certificate did not complete, so the index now borrows the
tool's own vocabulary instead of the Linux word. The **filenames keep `tier3`** — the wiring is
Tier 3, and renaming a committed artifact rewrites a record's identity, which §16.13 forbids for
the same reason it forbids editing one.

**Why this recurs, and the shape to watch for.** Both sites are places where a *number or a label*
was carried across from a session transcript into a file that is not prose and therefore did not
look like a claim. A gate file reads as configuration; an index column reads as metadata. Neither
is exempt, and a sweep that greps only `docs/*.md` will miss both again.

### 3.37 A second skip class, larger than §3.35's, with no required-mode and no operator remedy

**Design:** plan §3 rule 7, as restated by §3.35 — a capability-gated test may self-skip, but a
skip must never be readable as coverage.

**Reality, measured on the Mac with the crossover attached and `SNX_CROSSOVER=required` exported.**
§3.35 fixed the *rig* gate. Beside it sits a second, larger gate that the fix does not touch:
`serial_echo()` and `serial_pair()` (`itest/src/lib.rs`) are `#[cfg(target_os = "linux")]` and
return `None` everywhere else. **64 `#[test]` functions across 38 test binaries** gate directly on
them; the broader "no software serial device off Linux" family is ~78 tests across ~45 binaries,
roughly 27% of `serial-nexus-itest`. The tree has exactly four required-mode call sites
(`SNX_WEB_UI` ×2, `SNX_LICENSE_GATE`, `SNX_CROSSOVER`) and **none reaches this class.**

*How those counts were taken, because a bare number here is exactly what §16.13 says must be
checkable.* Every provider call site was attributed to its innermost enclosing `fn` with comments
and string literals stripped first, then helper-to-test edges resolved — two tests reach a provider
only through `p12_tap_replay`'s `ringless_window_gap`, and a per-`fn`-body scan alone would miss
them. A naive `grep -rl 'serial_echo\|serial_pair' itest/tests/*.rs` returns **39** files rather
than 38: the extra one is `p8_web_ui`, which calls a provider but self-skips behind the browser
gate, not this one. Expect that off-by-one when re-deriving the figure.

**The sharpest instance is not the count but two fix-it messages that cannot be acted on.**
`p4_exclusivity::exclusive_write_lock_is_byte_exact` and
`p4_send::send_is_atomic_locked_denies_then_steal_delivers_line_exactly_once` tell the operator to
attach a crossover rig. On this box the rig **is** attached, `SNX_CROSSOVER=required` **is**
exported, and both still self-skip and report green — because `serial_pair()` has no rig arm at
all. An operator who does exactly what the message asks sees no change, which is worse than a skip
that admits it is one.

**Also inaccurate: the message itself.** "no serial device on this platform" is false on a box with
two working FT232R ports — the fresh capture's P3 reports `custom_baud_ok`,
`tiocexcl_refuses_second_open`, `modem_calls_ok` and `break_ok` true on both, and P5 pairs them.
What is absent is a *software pty-backed double*, not a serial device.

**Filed, not fixed, and the reason is that §3.35's mechanism does not transplant.** A
`SNX_SERIAL_ECHO=required` would be permanently red on macOS, because the capability is genuinely
absent there and no operator action can supply it — where `SNX_CROSSOVER=required` is satisfiable
by plugging in hardware. On Linux the same flag would be unreachable, since `serial_echo()` cannot
return `None` there. So a required-mode is the wrong instrument for this gate in both directions,
and inventing one would produce a lane that fails for being macOS. The two defensible repairs are
(a) correct the two fix-it messages so they stop promising a remedy that does nothing, and (b) give
`serial_pair()` a rig arm so the tests that genuinely only need *two cross-wired ports* can use the
hardware that is already attached and already certified for it. Both are real changes to what the
harness covers on Darwin, and §7 forbids taking them on one box's evidence inside a session whose
purpose was to validate that box.

### 3.38 A config verb's reply preceded the listener it created — root-caused, and fixed in the product

**Design:** §15.42, added for this. §9 forbids a guard — or a promise — that asserts a proxy in
time for the property it stands for.

**Reality.** `load` returned `{"loaded": N}` immediately after `node.start(&mut wiring)`, and a
`role = "listen"` leg's `start` only `spawn_local`s its supervisor: `bind(2)` and `listen(2)` run
inside that task, on the same current-thread `LocalSet`, microseconds later. So the reply announced
a graph whose inbound address did not exist yet, and a caller that dialled the address it had just
configured raced the daemon. **Nothing else was observable in the meantime**, which is what made it
undiagnosable from the outside: `LegShared::new` initialises status to exactly the
`Waiting { reason: "no peer connected yet" }` that a *successful* bind sets, so `state` could not
distinguish a listening leg from an unbound one.

**The two hypotheses were separated by measurement, not argument.** `docs/macos.md` recorded a
located suspect and an explicitly un-eliminated competitor: ECONNREFUSED on a unix socket means
either "no listener yet" or "accept backlog full", and 8-way concurrency is the condition for the
second. They make opposite predictions, and both were tested on Linux 7.0.0-29:

* The failure rate falls **monotonically with the delay between the reply and the first connect** —
  40.5% at 0 µs, 0% at 5 ms, 200 trials per point, **one connection each**. That is the readiness
  shape exactly.
* A connection-count sweep against a *provably listening* leg reached **4097 simultaneous pending
  connections** before the kernel refused, and refused with **EAGAIN, never ECONNREFUSED**.

**The backlog hypothesis is dead**, and it is recorded as refuted rather than dropped (§9): a
refuted diagnosis is as load-bearing as a confirmed one, and this one would otherwise be
re-proposed every time the symptom returns under load.

**Decision — the fix is product-side and structural.** The leg carries a one-shot flag plus a
`Notify`; the supervisor sets it on its first turn *either way*; `load`/`add-node` collect the
handles **inside** the state critical section and await them **after** the borrow is released, so a
`RefCell` borrow structurally cannot cross the `.await` (invariant 1, §16.2). A 5 s bound caps the
wait so a wedged node task can never make `load` unanswerable.

**Attempt, not success.** A refused bind resolves the barrier too, having already faulted the node
with its reason in `state`. §15.8 puts an environmental failure in the node's status and never in
the verb's result; waiting for success would invert that rule and stall the caller for the whole
backoff schedule of an address that may never bind.

**`itest/tests/p6_hostility.rs` is deliberately unchanged**, and that is the point. A retry loop in
the harness would have made the same symptom disappear while leaving every other consumer of the
RPC racing — hiding a product defect behind a test-only workaround. Its three tests are now the
defect's own regression coverage.

**Fail-first, and deterministic rather than probabilistic.**
`load_does_not_reply_before_its_listen_legs_are_accepting` *is* the executor: it dials with no
`.await` between `dispatch("load")` resolving and the connect, which on a current-thread `LocalSet`
denies the leg's spawned task any chance to run. Reverting only the `await_listen_barriers` call
fails it every time — `dialling …/barrier.sock gave No such file or directory (os error 2)`, i.e.
`bind(2)` was not even reached. Timing margin plays no part. With the fix, 24 concurrent
`p6_hostility` runs (8-way × 3 rounds) are green on Linux.

**Pre-registered prediction for the next Mac run (§7):** all three `p6_hostility` tests pass under
8-way concurrency on Darwin, where the recorded rate was 1 in 40. If they still fail, this entry's
root cause is wrong and the ECONNREFUSED has a third source neither hypothesis names.

### 3.39 A daemon outlived the test process that spawned it, because `Drop` is the happy path

**Design:** §15.43, added for this.

**Reality.** Every daemon the harness starts was killed only by `Daemon::drop` — and `Drop` is
exactly what does not run when a process dies without unwinding. A whole-gate Mac run lost **all
five hardware-rig tests** to one leaked daemon still holding both FTDI ports; on the dev box, two
further leaked daemons from earlier sessions were found still running, on sockets nothing would
ever dial again. The symptom lands in the *next* run, as a `TIOCEXCL` refusal whose cause is
nowhere in that run's output.

**Two orphan paths, and only one of them needs a signal.** The first is the obvious one: SIGKILL,
`abort`, a runner killing the process group. The second is signal-free and was found by reading:
`Daemon::start_with_args` spawned the child and *then* asserted readiness, so a readiness timeout
unwound past a bare `std::process::Child` — whose `Drop` neither kills nor reaps. Measured: a
`sleep 600` child survived a `catch_unwind` panic and its parent's exit. The guard construction now
precedes the assertion, which closes that one by ordering alone.

**A refuted diagnosis, recorded because §9 makes it load-bearing.** The first explanation offered
for the Mac failure was cross-binary contention over a rig guarded only by a process-local mutex.
It is wrong: cargo runs test binaries strictly sequentially — sampled twelve times during a live
run — so there is no cross-binary race to lose. The leak is the mechanism; its *trigger* on that
run is still not established, and this entry does not claim it.

**Decision — an orphan leash, and deliberately not the platform primitives.** The daemon gains an
opt-in `--exit-on-stdin-eof`; the harness spawns it with `Stdio::piped()` and holds the write end
for the `Daemon`'s whole life. The kernel closes that fd however the parent dies, and the daemon
stops through its *normal* teardown — socket unlinked, claim released — rather than being killed.
`PR_SET_PDEATHSIG` (Linux-only, thread-scoped) and kqueue `NOTE_EXIT` (Darwin-only, `unsafe`
outside `serial_nexus_sys`) were both rejected: each is a repair that executes on only the platform
where the defect did *not* bite, which is §9's proxy in space. Pipe EOF is POSIX and needs no `cfg`,
so the Linux suite exercises the same code Darwin will.

**Fail-first, and it reproduces the reported defect exactly.** `a_sigkilled_test_process_leaves_no_daemon`
re-invokes this test binary as a fixture, waits for it to report its daemon's pid and socket,
`kill -9`s it, and requires both to be gone. Removing only the `--exit-on-stdin-eof` argument from
the harness fails it after the full 30 s wait: *"daemon 1644542 outlived the SIGKILLed test process
that spawned it; it still holds /tmp/snx-it-1644540-0/serial-nexus-daemon.sock"*. The socket check
is the half pid reuse cannot fool, and it is also evidence the daemon took the clean path rather
than merely dying.

**Not done, said rather than silently declined.** No leash on `Sim`, nor on the raw daemon spawn
sites outside `Daemon::start`. The five rig tests all go through `Daemon::start`, so the reported
defect is closed; extending the leash further is a separate follow-up, not a requirement of this
one.

### 3.40 P6 and P7 measured a pty the daemon does not run — and two diagnoses this tree recorded are refuted

**Design:** §7.2's baseline is the configuration the daemon runs a pty in; §9 forbids a measurement
that stands in for the property it names.

**Reality.** `apply_pty_baseline` sets the baseline before any slave exists. On a kernel that
re-initialises pts termios when the momentary slave closes, that set is gone by the time the
measured session runs, so P6 and P7 sampled whatever discipline the kernel left behind. The repair
is P10's, applied to the fd that matters: re-assert on the **client's own slave**, which is exactly
what `nodes/pty.rs` does on its rising presence edge.

**It needed one thing P10 did not: a drain.** P10 counts bytes it writes itself, so its re-assert is
invisible in its own numbers. P6 and P7 count *whatever is readable on the master*, and on a kernel
that emits `TIOCPKT_IOCTL` for an EXTPROC `tcsetattr` the re-assert is itself readable. Measured on
Linux 7.0.0-29: re-asserting **without** the drain moves P7's `a_open_close` from **0 → 1** and
`c_open_write_close` from **2 → 3** — the probe counting its own footprint as the session's evidence,
and inverting the one shape the §6 detach-release argument relies on leaving nothing behind. With the
drain, all three shapes read exactly what they read before the repair (0/1/2), and P6's
`after_last_close` block is byte-identical to the committed artifact. The footprint is *reported*
(`baseline_packet_bytes_drained`), so that invariance is auditable in the JSON rather than asserted
in a comment.

**Two recorded diagnoses are refuted here, and §9 makes that as load-bearing as a confirmation.**

1. **"Darwin's P7 zeros come from the lost baseline, so its `degraded` is false."** That was this
   session's own working hypothesis and it is **wrong**. Planting the loss on Linux silences the
   *termios* shape (`b`: 1 → 0) and leaves the *write* shape at 2 — and a fully cooked pair also
   leaves it at 2, so a written byte is neither EXTPROC- nor ICANON-gated. Darwin reports
   **`b == 0` and `c == 0`**. A lost discipline cannot produce that. What produces it is the
   last-close flush P13 measures directly (`waits-then-discards`, `terminal_read: "eof"` in all
   three P7 shapes). **P7's degrade on Darwin is genuine**, and no termios repair can move it. The
   probe now says which cause it is looking at, in `silence_cause`.

2. **"P2's `termios_settable_without_slave: false` means Darwin always takes the slave fallback."**
   Also wrong, and refuted by the committed artifact rather than by argument: P6's
   `handler_reset_applied: true` *is* `set_baseline(&master).is_ok()`, so `tcsetattr` through the
   Darwin master succeeds. P2's flag is a conjunction, and its failing conjunct is EXTPROC
   *retention*, not settability. The probe no longer infers this at all — `baseline_via_master`
   reports which arm actually ran, and `handler_reset_extproc_retained` separates "the syscall was
   accepted" from "the flag took", which is what stops `handler_reset_applied: true` sitting beside
   `handler_reset_readable_bytes: 0` as an unexplained contradiction on Darwin.

**What *is* false is the consequence P7's prose bolted onto its degrade.** It asserted that a
collapsed termios-only session keeps its write lock "until the node is removed or another writer
steals it". On the same Darwin box, **P12 reports `supported`** — all three shapes post a
session-boundary edge, and `pty.rs`'s `SessionLatch` (§15.39) carries detach-release there. The
sentence describes the pre-`SessionLatch` daemon and has been stale since. Every P7 degrade arm now
points the reader at P12 instead of asserting a leak P7 cannot see.

**Fail-first.** `arming_the_client_baseline_leaves_no_footprint_on_the_master` fails when the drain
is removed: *"arming the client baseline left 1 byte(s) readable on the master (0x40)"*.
`a_silent_write_shape_is_not_a_lost_line_discipline` pins the classifier across all four quadrants
and asserts the discrimination itself, so a classifier collapsed to one answer fails even though
every single-valued assertion would still hold.

**Pre-registered for the next Mac run (§7):** P7 stays `degraded` with
`silence_cause: "hangup-destroys-evidence"`, `extproc_retained_at_shape` reports what Darwin
actually does with the flag, and `baseline_via_master` reads **true** — not the `false` this
session assumed. If `baseline_via_master` comes back `false`, refutation 2 above is itself wrong.

<!-- ANNOTATION 2026-08-05 (§5, amend-first: this entry is annotated, not rewritten).
     THE FALSIFIER ABOVE FIRED, AND REFUTATION 2 OF THIS ENTRY IS ITSELF REFUTED.
     Measured on the Mac at binary `1a9a8fca1c36`, probe set `a131e1f4b46d6c83`:
     `baseline_via_master` reads **false** in 12 of 12 observations — P6's
     `client_session_baseline` plus P7's three shapes, across three sequential runs,
     byte-identical (`docs/doctor/macos-24.6.0-2026-08-05-1a9a8fc-tier3{,-2,-3}.json`).
     The other two clauses held: P7 is `degraded` with `silence_cause:
     "hangup-destroys-evidence"`, discriminated from `"extproc-unavailable"` by shape
     `c` reading 0 where Linux reads 2.

     Refutation 2 read: "P2's `termios_settable_without_slave: false` means Darwin
     always takes the slave fallback." — "Also wrong … `handler_reset_applied: true`
     *is* `set_baseline(&master).is_ok()`, so `tcsetattr` through the Darwin master
     succeeds." The premise is sound and the inference is not. `apply_pty_baseline`
     and `handler_reset_applied` call the IDENTICAL `set_baseline(&master)` at two
     different points in a pty's life, and Darwin answers **Err at creation** and **Ok
     after the hangup** — which is why `baseline_via_master: false` sits beside
     `handler_reset_applied: true` in every run. At creation, the moment `nodes/pty.rs`
     actually runs `apply_pty_baseline` and the moment P2's field is about, Darwin
     DOES always take the momentary-slave fallback. Recorded per §9: a refuted
     diagnosis is as load-bearing as a confirmed one, and this one was recorded by
     this tree against itself. Full reading in §3.45 C and C'. -->

### 3.41 P13 gets the baseline repair; P8, P9 and P12 are measured not to need it

**Design:** §7.2's baseline, and §7's rule that a kernel question is settled by measuring.

**Three of the four need nothing, and each for a different reason — recorded so the next reader
does not re-derive them.**

* **P8 — no change.** It is `skipped` with zero observations in all three committed Darwin
  captures, so no number escapes to be wrong. The precise reason matters and is not the obvious
  one: `apply_pty_baseline` *does* run inside `p8_inner` on Darwin; the probe then fails at
  `Epoll::new()` and discards everything. It is safe because nothing is reported, not because the
  fallback never fires.
* **P9 — no change.** Measured on Linux: a forced-cooked slave (ICANON+ECHO, with and without
  packet mode) changes neither the poll cost (143 ns vs 152 ns — noise) nor readiness
  (`ready = 0/4096` in every arm). The probe already self-certifies its own precondition with
  `ready_passes_total: 0`, which both kernels report.
* **P12 — no change**, upholding the 2026-08-05 annotation with a sharper reason than it had: the
  edge is `EV_EOF` on an `EVFILT_READ` knote — a hangup property, termios-independent — and adding
  a baseline re-assert to the shapes would **destroy shape `a`** by turning a bare open/close into
  a tcsetattr session. The a/b/c contrast *is* the instrument.

**P13 — repaired.** The `[b'x'; 64]` payload does survive a cooked discipline (measured: cooked and
raw both recover exactly 64, and a `\n` control proves the mechanism is live by expanding 64 → 128
under ONLCR), and close duration is termios-flat on Linux (raw 1–19 µs, cooked 2–12 µs). But the
probe could not *testify* to its own configuration — the same deficiency P10's repair closed — so it
now re-asserts on the slave it measures and reports `slave_termios_mode`, with a tripwire that
degrades if any shape was not raw.

**The repair carries a Darwin-only hazard Linux is structurally blind to, which is why it drains.**
With the master in packet mode a slave-side `tcsetattr` raises `TIOCPKT_IOCTL` — measured on Linux
as exactly one byte, `0x40`, which the caller's existing `bytes - reads` correction already absorbs.
A BSD `ptcread` may copy the whole `struct termios` after that control byte, which a per-read
correction cannot subtract, and ~72 uncounted bytes landing in `bytes_after` would flip Darwin's
headline from `waits-then-discards` to **`waits-then-retains`** — an inverted result caused by the
instrument. Rather than bet on which BSD does what, the bytes are consumed before the measurement
window opens and **reported** as `baseline_packet_bytes`, so the next Darwin capture answers the
question with a number instead of an assumption (§7).

**Pre-registered for the next Mac run:** P13 stays `waits-then-discards` with
`slave_termios_mode: "raw"` on all three shapes. `baseline_packet_bytes` reads **1** if XNU appends
nothing, or **~72** if it appends the termios struct — either is informative, and the second would
have silently inverted the headline before this change.

**Not taken here:** P9 and P2 both report a "zero-timeout poll" median and they differ 8–11× on
Darwin (22832/22980/16098 ns against 2091/2102/2086) while agreeing on Linux (195 vs 263 ns). The
two are not the same measurement — P2's fd is hung up and its mask is POLLHUP-only, so every pass
returns ready, where P9's fd has a slave open and asks about POLLIN, so none does. A Linux 2×2 ruled
out sample count and cold start (all four cells 145–152 ns). Decomposing it properly needs P9 to
reproduce P2's shape inside itself; that is a new measurement rather than a repair, and it is filed
rather than smuggled into this change.

<!-- ANNOTATION 2026-08-05 (§5). FILED, THEN BUILT, THEN MEASURED — this paragraph is
     discharged. §3.44 built the 2x2 and §3.45 B measured it on Darwin: the gap is the
     **fd state**, 7.46-10.12x across fd state at fixed mask against 0.968-1.314x across
     mask at fixed fd state, with Linux flat. Two corrections to this paragraph's own
     numbers, both from the committed artifacts rather than from argument:

       * "agreeing on Linux (195 vs 263 ns)" and "all four cells 145-152 ns" cite no
         artifact. The committed `-05b` triple at the same binary reads P2 173/169/173,
         P9 headline 267/302/263, and all four 2x2 cells at 258-260. §3.44 records the
         same cells as 166/172/166/166. Three Linux readings of the same cells spanning
         ~145-260 ns are unreconciled across this file; §16.13 makes the committed triple
         the figure of record and the others scrollback.
       * The Linux P2-vs-P9 *headline* ratio is not ~1.0 as "agreeing" implies — it is
         1.52-1.79x, and §3.45 B attributes essentially all of it to instrument offset
         (different poll wrapper, n=16 cold against n=4096). So the honest Linux statement
         is "the four 2x2 cells agree", not "P2 and P9 agree".

     What this paragraph got right and the measurement confirmed: P2 and P9 are not the
     same measurement, and the fd state is the variable that matters. §3.45 B. -->

### 3.42 P5's UART predicate was a Linux-only proxy, and the crossover rig could never certify off Linux

**Design:** §15.21's certificate, and §9's rule against a proxy in space.

**Reality.** `p5_is_uart` gated on `TIOCGICOUNT`. That is the property *on Linux* and nothing at all
anywhere else — `serial_nexus_sys`'s non-Linux arm is a hard `ENOTSUP` stub — so on Darwin it
answered "not a UART" for two genuine FT232R adapters and the **entire certificate, rate ladder
included, never ran**. All three 2026-08-05 macOS captures read `cert: skipped` on both ports, on a
rig that P3 certifies as fully functional and that moves 32768 bytes byte-exact each way at 250000
baud. §9's proxy in space exactly: a Linux-only observable standing in for a portable property,
passing forever on the box it was written on.

**`TIOCMGET` is the portable member, and it is the only item in P3's whole vector that
discriminates.** Measured on this box with the rig attached: a Linux **pts accepts a custom baud and
reports 250000 back**, accepts `TIOCEXCL`, and accepts `TIOCSBRK`/`TIOCCBRK` — so custom baud,
exclusivity and break cannot be the predicate however UART-ish they read. It refuses `TIOCMGET` with
`ENOTTY` (the pty master too), and both FT232R ports answer it. The other half is already committed:
P3 reports `modem_calls_ok: true` for both FTDI ports in every macOS capture.

**Decision:** `read_modem_bits(fd).is_ok() || read_icounts(fd).is_ok()`. A **disjunction**, not a
replacement — that is what makes it non-regressive by construction: every port that certifies today
answers `TIOCGICOUNT`, so it still certifies and no committed Linux artifact can move. A widening
cannot lose a port; a replacement could, and §7 forbids that one-way decision. Verified: the Linux
Tier-3 certificate is unchanged, rate ladder and deliberate mismatch still run.

Read-only, deliberately: `TIOCMGET` asserts nothing on the line where P3's `TIOCMSET` drives an
output, and this predicate runs over every port the operator named — which may be live equipment
(§3's passive-by-default rule). Only whether the driver answers is consulted, never the values.

**No decline was on record.** 37-TOOL-3's *justify* covers the certificate's **contents** (parity
mismatch, break reception, far-side modem), not the predicate's portability.

**Pre-registered for the next Mac run (§7):** both FTDI ports certify, and P5 reports **Tier 3** with
`rate_ladder=true`. If they still report `cert: skipped`, `TIOCMGET` does not discriminate on Darwin
the way the committed `modem_calls_ok: true` implies and this entry is wrong.

<!-- ANNOTATION 2026-08-05 (§5). HALF MET, and the half that failed is structural rather
     than a kernel surprise. Measured at `1a9a8fca1c36`
     (`docs/doctor/macos-24.6.0-2026-08-05-1a9a8fc-tier3{,-2,-3}.json`): the entry's own
     falsifier did NOT fire — neither port reports `cert: skipped` any more, both
     certify, and the pair reports `rate_ladder=true`, so `TIOCMGET` does discriminate
     on Darwin exactly as `modem_calls_ok: true` implied. What did not happen is the
     tier: `grep -c "Tier [0-9]"` over the report is **0**, because `p5_verdict`'s
     `!uncertified.is_empty()` arm returns before the tier-naming arm, and `icounter`
     (both ports) plus `deliberate_mismatch` (the pair) are still uncertified. So P5's
     status moved `supported` -> `degraded`, which is the honest direction: the old
     `supported` was UNFALSIFIABLE on macOS — with `p5_is_uart` gated solely on
     TIOCGICOUNT's ENOTSUP stub, `any_uart` was false and `failures` empty, and
     `p5_verdict` reaches a hardcoded `Status::Supported` by its final `else` for every
     possible macOS input. Zero items were evaluated before; five are now. Filed, not
     fixed: the tier-naming arm is unreachable whenever anything is uncertified, and the
     new consequence names the two uncertified items without saying they are
     structurally unmeasurable on this kernel (the old text said "TIOCGICOUNT, which is
     Linux-only"; the new text says neither). §7 wants the observation named. §3.45 E.

     AND ON THIS ENTRY'S OTHER OPEN ITEM: the paragraph below beginning "Filed, not fixed
     — the 1024/1022 asymmetry now has a mechanism but not a measurement" is superseded in
     its second half. The measurement exists as of §3.45 A — P10's `recheck`, 6 of 6 on
     Darwin — and it is CONSISTENT with the TTYHOG-2 / TTYCLSIZE two-queue source read
     recorded below, but it cannot DISCRIMINATE that reading from a reservation charged at
     the empty->nonempty transition, because the recheck's top-up never starts from empty.
     So the accurate status is "has a measurement, but not a decisive one", and one extra
     drain size in `p10_recheck` would settle it. -->


**Filed, not fixed — the 1024/1022 asymmetry now has a mechanism but not a measurement.** It is
**not** a probe artifact: P10 never enables packet mode, and enabling `TIOCPKT` on Linux was measured
to change the accepted counts in neither direction (it adds bytes to *recovery*, one type byte per
master read — it can only ever add on the read side, never subtract on the write side). A read of
XNU's `bsd/kern/tty_dev.c` explains both numbers exactly: `ptcwrite` guards every character insert
with `(t_rawq.c_cc + t_canq.c_cc) >= TTYHOG - 2` and `bsd/sys/tty.h` sets `TTYHOG 1024`, giving
hostward **1022**; `ttymalloc` allocates `t_outq` via `clalloc(&tp->t_outq, TTYCLSIZE /* 1024 */, 0)`,
giving targetward **1024**. The same guard's second clause also explains the pre-repair
`ceiling_hit: true` run, independently corroborating §3.34's *inferred* cooked-discipline claim. But
that is a third-party source read, not a measurement of the operator's kernel, and §7 forbids
promoting it to established prose. The discriminating experiment — refill-from-empty, partial-drain,
byte-granular top-up, separating "queue capacity" from "per-fill allowance" — is designed and not
built here.

**And a correction this turned up: "Linux is symmetric at 15360/15360" is not supported.** Six runs
of the shipped binary on this box gave 13824 and 15360 *independently per direction*, so Linux's own
within-run direction asymmetry (1536 bytes) is **768×** Darwin's (2 bytes). Darwin's figures, by
contrast, do not vary run to run at all. Any prose contrasting a "symmetric" Linux with an
"asymmetric" Darwin has the comparison backwards.

### 3.43 The second skip class, repaired — and this **overturns §3.37's recorded decline**

**§5 requires this to be said in the first line, not discovered in a diff:** §3.37 filed two repairs
and declined **both**, on the grounds that they were "real changes to what the harness covers on
Darwin, and §7 forbids taking them on one box's evidence inside a session whose purpose was to
validate that box." Both are taken here. The decline is overturned, not forgotten.

**The new evidence §3.37 lacked is a measurement rather than an argument.** Its crux — "does a real
crossover satisfy the `serial_pair()` contract?" — was unanswered. It is now answered per caller: on
this box, with the crossover forced under a patched provider, **6 of the 7 `serial_pair()` tests pass
byte-exact over the real FT232R wire** (3 reps each, plus 3 reps at default parallelism). The 7th,
`p7_p5::p5_classifies_paired_dangling_and_loopback_ports`, fails **structurally and repeatably**:
`serial-nexus-doctor` keys a real port by its `usb:0403:6001:BH00LL8O:00` identity and characterizes
it as a UART, while the test asserts path keys and `skipped (not a UART)` — and it needs
`serial_echo()`, which has no rig arm and gets none. So the answer is yes for six and *provably* no
for one, which is what makes a seam correct rather than a gamble.

**The seam is a second provider, not a swap.** `serial_pair_or_rig()` is opted into by six call
sites; `p7_p5` keeps `serial_pair()`. **Software wins whenever it exists**, so Linux and CI are
untouched and the rig is a fallback for the platform where the software double does not exist —
never a preference. That ordering matters concretely: any Linux box with `SNX_CROSSOVER_A`/`_B`
exported has both, and preferring the rig would silently move six tests onto hardware that
`serial_hardware.rs` and `p12_serial_exclusivity` need and that costs wall clock (measured: 6.30 s
against 0.55 s for one binary).

**Repair (a) falls out of it.** One `skip_no_pair()` helper replaces four hand-written messages,
drops the false *"no serial device on this platform"* — untrue on a box with two working FT232R
ports — and names a remedy that now actually works. `SNX_CROSSOVER=required` covers it, because the
skip is only reachable where the rig is the *only* provider.

**The decision table is a pure function, and that is what makes the guard portable.**
`choose_pair_source(software, rig, force_rig)` is checked by
`the_pair_provider_prefers_software_and_falls_back_to_the_rig` on any box, hardware or not — where
the provider itself can only ever return one arm. `SNX_SERIAL_PAIR=rig` forces the fallback so it is
exercisable where it is not the default, and forcing it with **no** rig visible is a hard failure
rather than a silent fallback to software — which would be §3.35's defect in a new place, an
operator instruction that quietly does nothing.

**Fail-first:** swapping the first two arms so a visible rig outranks the software double fails with
*"a visible rig must not displace the software double"*. Verified both ways on this box: default
(software) and `SNX_SERIAL_PAIR=rig` (hardware) both green across all four converted binaries.

**The caveat, and why this is not an unqualified fix.** The fallback arm executes only where the
software double is absent — Darwin — and it was measured here on Linux by forcing it. Its failure
mode is a red test that names its own cause, not a silent pass. **Pre-registered (§7):** on the next
Mac run the six converted tests execute against the rig instead of skipping; if any goes red it will
name the provider it used, and `p7_p5` continues to self-skip there as it always has.

<!-- ANNOTATION 2026-08-05 (§5). THE PRE-REGISTRATION CAME TRUE IN THE FAILING DIRECTION,
     and the safety property held exactly as written. On the Mac at `1a9a8fca1c36`,
     `p4_free_for_all::free_for_all_endpoint_lets_concurrent_writers_both_reach_device`
     executed against the rig and went RED, printing its provider first: "RIG: this test
     is running on the crossover rig (...), not the sim null modem". Not a regression —
     at `7ead470` it called `serial_pair()` and self-skipped with "no serial device on
     this platform", so this is its first hardware execution anywhere.

     Measured, not assumed (§8): on a quiet box, 12 of 12 reps failed at the committed
     30 s deadline losing 5-31 bytes of 32768; raising ONLY the sink deadline to 120 s
     gave one clean pass in 5.00 s and one failure that still lost 2 bytes after 122 s.
     So a healthy run takes ~5 s against a 30 s budget. Precisely (§6 forbids the short
     form, since every failing observation carries `timed_out: true` and the sim sets
     that flag so a deadline is never read as a drop): what is measured is "not
     recovered within 4x the committed deadline on a path where a healthy run finishes
     in 5 s" — a stall or a loss, not separated here.
     The same rig is byte-exact at 32768 bytes both ways in `serial_hardware.rs`, and the
     other five `serial_pair_or_rig()` call sites (`p4_send`, `p4_exclusivity`, and three
     in `p8_map`) all pass on it, so the wire and the single-writer path are sound and
     what is left is two concurrent writers merging onto one free-for-all serial
     endpoint. (`harness_contract` names the provider but is NOT a call site — its test
     is a pure unit test of `choose_pair_source` that never opens a port.) MECHANISM NOT ESTABLISHED
     and no root cause claimed (§9 would require an independent verifier; none ran).

     Consequence for THIS entry: its "6 of the 7 pass byte-exact over the real wire" is a
     LINUX figure, gathered by forcing an arm that is only the default on Darwin. On
     Darwin it is 5 of 6. The overturn of §3.37 is not reversed — the seam still works and
     still names itself — but the figure must not be quoted platform-free. §3.46. -->

### 3.44 The two experiments that were designed and not built — now built, so one Mac run answers both

**Design:** §7 — settle a kernel question by measuring it, not by reasoning about it from the other
platform.

Two questions were left open by §3.41 and §3.42 as *designed but unbuilt*. Both are Darwin
questions, both are cheap, and leaving them unbuilt meant a Mac run would have to happen three
times. They are built here, calibrated on Linux first, and their Darwin answers pre-registered with
named falsifiers.

**A. Is P10's depth a queue capacity or a per-fill allowance?** The two are indistinguishable in a
single fill, and the difference is exactly what Darwin's 1024-targetward / 1022-hostward left open.
A capacity republishes precisely the room a reader frees; an allowance charges its reservation
again. So P10 now, **after every existing field is final and the peer is empty** — so no committed
observation can move and old artifacts stay diffable (§16.13) — refills from empty, hands back 512
bytes, lets the tty's asynchronous work run, and writes **one byte at a time** until it blocks.

*Linux calibration, 20 samples, load 0.22, both directions:* `drained_again` is **512 in 20 of 20**;
`room_republished_minus_room_freed` is **bimodal — +2048 or +9216, and never 0**; `refilled`
reproduces `total()` once in 20. Linux answers "neither, exactly": its bound is a moving snapshot of
an asynchronous pipeline, and it hands back *more* room than was freed because the pipeline advanced
during the settle. **Pre-registered for Darwin:** `room_republished_minus_room_freed` reads **0** and
`refill_reproduced_total` reads **true**, because a fixed `TTYHOG`/`t_outq` bound republishes exactly
what is freed and reproduces exactly. A nonzero delta refutes the capacity reading; a negative one
means a reservation, which nothing in the XNU source read predicts.

**B. Why do P2 and P9 disagree 8–11× about "a zero-timeout poll" on Darwin and agree on Linux?**
Because they are not the same measurement, and nothing in the set said so: P2's fd is **hung up**
with a **POLLHUP-only** mask, so every pass returns ready; P9's has a slave open and asks **POLLIN**,
so none does. P9 now reproduces P2's shape inside itself — one pty, one clock, one variable at a
time — as a 2×2 over {unready, hung-up} × {POLLIN, POLLHUP} at P2's sample count.

*Linux measurement, 4096 samples per cell:* **166 / 172 / 166 / 166 ns** — all four cells
indistinguishable, which is what makes the Darwin gap a real finding rather than an artifact of the
comparison. **Pre-registered for Darwin:** if `ready_hungup_master_pollhup_ns` lands near 2000 ns and
`unready_master_pollin_ns` near 20000, the gap is the **fd state or the mask**, and the two probes
were always measuring different things; if all four cells land together, the gap is *neither*, and
the difference lives in something else P2 and P9 do not share — which would make it an instrument
error worth chasing.

**Three Darwin-only arms were forced on Linux before shipping, because §9 says a repair exercised
nowhere it can be observed failing is a proxy in space.** Forcing `apply_pty_baseline`'s
momentary-slave fallback: `baseline_via_master` reports `false` and **all four pty probes still read
correctly**, because the client-slave re-assert compensates — which is the whole claim of §3.40.
Forcing a baseline that does not retain EXTPROC: P7 reports `silence_cause: "extproc-unavailable"`
with shape `b` at 0 and shape `c` still at **2**, reproducing end to end the discriminator that
refuted the false-degrade hypothesis. Forcing a cooked pair: P10 and P13 both **degrade** and name
the mode, and `linux.jq` still passes — a degraded report reports rather than reddens, which is what
the gate change in §3.34 was for.

**Fail-first.** Moving the recheck ahead of the drain that empties the peer fails
`p10_recheck_measures_republished_room_rather_than_nothing` with *"the recheck refilled 0 bytes into
a peer that `recovered`'s drain had just emptied"*. The guard deliberately does **not** assert the
sign of `room_republished_minus_room_freed`: pinning Linux's answer would make it assert a prediction
about a kernel this box is not. The prediction lives here, where being wrong is a record.

**The fingerprint does not move, and that is correct here.** Both additions are new *observations*
under unchanged questions, and both run after every existing field is final, so the committed
artifacts stay field-by-field diffable. Per §3.34's standing rule, the instrument change is announced
in `docs/doctor/README.md` instead, because the digest will not announce it.

### 3.45 The Mac run that answered §3.44 — both predictions held, one inference did not, and §3.40's own falsifier fired

**Design:** §7 — settle a kernel question by measuring it.

Artifacts: `docs/doctor/macos-24.6.0-2026-08-05-1a9a8fc-tier3{,-2,-3}.json`, Darwin 24.6.0 /
macOS 15.7.8 x86_64, binary `1a9a8fca1c36`, probe set `a131e1f4b46d6c83`, three sequential runs on a
box at load 1.89–2.58 with the FT232R crossover attached and proven on the wire the same session.
**These are the same binary as the `-05b` Linux triple** (`4b78fffc4bf2`): the intervening commit was
docs-only, and `git diff 4b78fff 1a9a8fc -- '*.rs' '*.toml'` is empty. That is a stronger basis for
the diff than the fingerprint gives, and it is stated here because the fingerprint cannot state it.

**A. P10's depth is a capacity, not an allowance — the prediction held in 6 of 6, and the inference
it was attached to is still underdetermined.** `room_republished_minus_room_freed` reads **0** and
`refill_reproduced_total` reads **true** in all six Darwin observations (3 runs × 2 directions),
exactly as pre-registered; neither named falsifier fired. `drained_again_bytes` 512 → `topped_up_bytes`
512 in 512 one-byte writes, terminating in EAGAIN with `topup_ceiling_hit` false, and
`refilled_from_empty_bytes` reproduces `total_bytes_accepted` exactly (1024 targetward, 1022
hostward). Linux at the same binary reads delta **+2048 in 6 of 6**. **The whole P10 subtree is
byte-identical across the three Darwin runs** (md5 ×3) where all three Linux subtrees differ — the
determinism is itself the finding.

*But the confirmation does not carry the inference attached to it, and that distinction is the
point.* The discriminator was sampled at exactly one drain size, and `P10_RECHECK_DRAIN` is a
hardcoded 512 against a Darwin capacity of 1024 targetward / 1022 hostward — so D is C/2 exactly
targetward and D > C/2 (512 against 511) hostward, and either way **the experiment has one bit of
resolution on this platform**. Any watermark model "writable iff occupancy < T, then accept up to C"
predicts `topped_up == 512` for every T > 512; the data excludes only T ≤ 512. Worse for the
original framing, **a reservation charged only at the empty→nonempty transition is invisible by
construction** — the top-up always starts from occupancy C−512, never from empty — so the falsifier
"a negative delta means a reservation" *cannot fire* on that shape at all. **Cost to settle: one
extra drain size (128 and 900) in `p10_recheck`.**

*This does not reopen the 1024/1022 asymmetry, and §3.42 should be read first.* That entry already
carries an XNU source read explaining the two numbers as **two different queues** rather than one
queue with a deficit: `ptcwrite` guards on `(t_rawq.c_cc + t_canq.c_cc) >= TTYHOG - 2` with
`TTYHOG` 1024, giving 1022 hostward, while `ttymalloc` sizes `t_outq` at `TTYCLSIZE` 1024, giving
1024 targetward. Under that reading 1022 *is* a capacity and no reservation is needed. What this run
adds is that the recheck cannot **discriminate** the two-queue reading from a reservation reading,
not that the two-queue reading is in doubt — and what remains owed is the same thing §3.42 said was
owed: a measurement of the operator's kernel against that source read, which one extra drain size
would supply. Recorded rather than fixed, because changing the probe now would move a field in
the artifact this entry cites.

**B. The P2/P9 zero-timeout gap is the fd state. §3.41's "undecomposed" is closed.** P9's 2×2, 4096
samples per cell, medians in ns, three Darwin runs:

| cell | run 1 | run 2 | run 3 |
|---|---|---|---|
| hung-up + POLLHUP *(P2's shape verbatim)* | 1458 | 1650 | 1566 |
| hung-up + POLLIN | 1711 | 2057 | 2057 |
| unready + POLLHUP | 14540 | 15199 | 15852 |
| unready + POLLIN *(what the data plane parks on)* | 16436 | 16470 | 15349 |

Across fd state at fixed mask, all six pairs: 9.97, 9.61 / 9.21, 8.01 / 10.12, 7.46 — **7.46–10.12×**.
Across mask at fixed fd state: **0.968–1.314×** — and in run 3 it moves in the *opposite* direction,
so the mask is not even consistently signed. The largest hung-up cell (2057) and the smallest unready
cell (14540) do not overlap, a 7.07× floor between the two groups' extremes. Linux, same binary, lands
all four cells at **258–260 ns** with fd-state ratios of 0.996–1.004, which is what makes the Darwin
gap a finding rather than an artifact of the comparison. The instrument reproduces itself: P9's
hung-up/POLLHUP cell (1458/1650/1566) against P2's own headline (1733/2066/2084).

*One Linux figure in this tree does not reconcile, and it is named rather than quietly dropped.*
§3.44 records the Linux calibration of these same four cells as **166 / 172 / 166 / 166 ns**, also at
4096 samples per cell; the committed `-05b` triple — the *same binary* this entry diffs against —
reads **258–260**. The calibration cites no artifact, so §16.13 makes the committed triple the
figure of record, and the 166 ns number should be read as scrollback from a differently-loaded run.
Nothing in this entry's reasoning turns on which is right: both are flat across all four cells, and
flatness is the only property the Darwin comparison uses.

**Two corrections to how that result should be read, both from the adversarial verifier (§9).**
First, **the mask axis is degenerate by construction** — POSIX returns POLLHUP in `revents`
regardless of the requested mask, so on a hung-up master both mask cells return an event and on a
live master neither does. The honest shape of the finding is a **1×2, not a 2×2**: on Darwin a
zero-timeout poll that finds nothing ready costs ~15–16 µs and one that returns an event costs
~1.5–2 µs. *Read the headline against that.* §3.44 B pre-registered the answer as "the fd state **or**
the mask", and the mask arm was eliminated by POSIX semantics rather than by measurement — so the
variable the data actually isolates is **ready versus not-ready**, and "fd state" names how the probe
*achieves* that, not an independently confirmed cause. No shipped decision turns on the difference —
the data plane parks on the not-ready cell either way — but the two are not the same claim. Second, and unflattering to the instrument: on **Linux** the fd-state factor is 1.000 ± 0.004, so
**essentially 100% of the residual 1.52–1.79× P2/P9 headline disagreement there is instrument
offset** — P2 uses the local `hup()` helper on a *blocking* master, P9 uses `sys::poll_blocking` on
an `O_NONBLOCK` one, and P9's headline is n=16 taken cold against the 2×2's n=4096. *Two distinct
quantities must not be fused here, and an earlier draft of this entry fused them.* The 1.52–1.79× is
P9's **headline** (267/302/263, n=16) over P2's headline (173/169/173). A second ratio — P9's
**hung-up/POLLHUP cell** (259 each run, n=4096) over P2's headline — reads 1.50–1.53× on Linux and
0.75–0.84× on Darwin, and it is *that* one whose sign flips between platforms; the n=16-cold
explanation belongs to the first and not to it. For the headline quantity Darwin reads 13.30/11.15/
7.48×, the same direction as Linux, with no flip. The gap is real and the instrument is not clean;
both belong in the record, and so does the distinction between the two ratios.

*Order and warmup are excluded, and this is what rules out the "instrument artifact" arm.* P9's
cells run in a fixed, uncounterbalanced order, so "later is cheaper" is a live rival within P9
alone — but it dies on P2, which runs far earlier in the same process and reads the **cheap** value.
The wall-clock sequence is cheap (P2, early) → expensive (P9 unready, middle) → cheap again (P9
hung-up, late), and no monotone warmup produces a non-monotone sequence.

**C. §3.40's pre-registration named a falsifier and the falsifier fired. Refutation 2 is itself
refuted.** §3.40 reads verbatim: *"`baseline_via_master` reads **true** — not the `false` this
session assumed. If `baseline_via_master` comes back `false`, refutation 2 above is itself wrong."*
Measured: **`false` in 12 of 12 observations** (P6's `client_session_baseline` plus P7's three
shapes, × three runs, byte-identical). The other two clauses of that pre-registration held — P7 is
`degraded` with `silence_cause: "hangup-destroys-evidence"`, discriminated from
`"extproc-unavailable"` by shape **c** reading 0 on Darwin where Linux reads 2, which is the §3.44
discriminator firing correctly on the platform it was built for. §3.40 is annotated in place rather
than rewritten (§5); the mechanism is in **C′** below.

**C′. The mechanism, and it was in the capture all along.** `apply_pty_baseline` and
`handler_reset_applied` call the *identical* `set_baseline(&master)` at two different points in a
pty's life, and Darwin answers **Err at creation** and **Ok after the hangup** — `baseline_via_master:
false` sits beside `handler_reset_applied: true` in every run. So at creation, which is the moment
`nodes/pty.rs` actually runs `apply_pty_baseline` and the moment P2's
`termios_settable_without_slave: false` is about, Darwin **does** always take the momentary-slave
fallback — precisely what refutation 2 declared wrong. A virgin Darwin ptmx master is not settable;
the same master after a slave has been opened and hung up is.

**D. What was measured and held, recorded because §9 says a held expectation is as load-bearing as a
refuted one.** P13's `baseline_packet_bytes` reads **1** in all three shapes on Darwin, same as
Linux — the feared "~72" (XNU appending the termios struct) did **not** happen, so the P13 headline
is not inverted. P13 otherwise reproduces at the new binary: `waits-then-discards`,
`close_waits_for_reader` true, and the three shapes across the three runs at
a=601084/600116/600340 µs (0 of 64 each), b=19/22/21 µs (64 of 64 each), c=28/28/27 µs (0 of 64
each) — against the 600104/23/29 `expectations/macos.jq` records from the previous binary. P10's
`slave_termios_mode` is `raw` on both directions in all three runs, so §3.34's re-assert continues to
take on Darwin.

**E. P5 now certifies on Darwin, and `supported` → `degraded` is the honest direction.** §3.42's
predicate change lands. The pre-fix report's `supported` was vacuous in a precise, bounded sense:
with `p5_is_uart` gated solely on TIOCGICOUNT, whose non-Linux arm is a hard ENOTSUP stub, `any_uart`
was false and `failures` empty, so **no certificate item could ever fail on macOS** and `p5_verdict`
fell through to a hardcoded `Status::Supported` by its final `else`. *The narrower claim is the true
one, and an earlier draft of this entry overstated it:* discovery still runs on macOS regardless of
`p5_is_uart`, so a **half-crossed** or **hung-up** rig would still have degraded the pre-fix P5, and
a rig where no port opens still reported `skipped`. What was unreachable is a certificate failure —
so on a *cleanly wired* macOS rig, which is this one, P5 always read `supported` whatever the
hardware did. The post-fix report evaluates five items instead of zero and certifies
`custom_baud=true`, `break=true` and — the only item carrying `integrity=true`, i.e. the only one
that could have driven P5 to `unsupported` — **`rate_ladder=true`**, a 3-rate × 2-direction nonce
exchange over the physical crossover. The modem map is **reported, never judged** (`probes.rs`
says so in as many words) and reads `cts=false dsr=false dcd=false ri=false` on both ports, which is
a 3-wire crossover having no modem lines to assert, not a result. Nothing that passed now fails; the set of
items evaluated went from zero to five. **§3.42's pre-registration is half-met and the shortfall is
structural:** ports certify and `rate_ladder` is true, but the report never names **Tier 3** —
`grep -c "Tier [0-9]"` over it is **0**, because `p5_verdict`'s `!uncertified.is_empty()` arm returns
before the tier-naming arm. Filed, not fixed.
<!-- ANNOTATION 2026-08-05 (§5): closed by §3.49. The tier sentence was hoisted out of the
     status decision into `p5_tier_scope` and the uncertified arm prints it as
     `Topology: **Tier 3** …`. The arms were deliberately NOT reordered — that would have
     flipped Darwin back to `supported` and undone the honest direction this entry
     established. The observation above stands as the record of the 1a9a8fc run. -->

**Open, named, and not fixed here.** (i) The new P5 consequence lists `icounter` and
`deliberate_mismatch` as uncertified without saying they are *structurally* unmeasurable on this
kernel — the old text said "TIOCGICOUNT, which is Linux-only" and the new text says neither, so a
Darwin operator could re-seat cables chasing something no Darwin box can produce. §7 wants the
observation named, and the mechanism is now missing.
<!-- ANNOTATION 2026-08-05 (§5): (i) is closed by §3.49. The mechanism is carried as DATA on
     `CertFailure.unmeasurable_here`, set from `sys::ICOUNTS_SUPPORTED` at exactly the two
     counter-reading `fail_if` sites, never matched out of the item name — so it cannot widen
     to an item that genuinely failed. -->
(ii) **P4 is vacuous on Darwin and the Linux
gate would admit the same shape**: `count: 0` with `status: supported`, because the `skipped` early
return needs `adapters.is_empty() && candidates.is_empty()` and Darwin has 4 candidates, so
`for a in &adapters` runs zero times, `all_resolved` keeps its initialised `true`, and the
consequence asserts *"Resolver produces canonical identities; configs survive replug and cold
start"* — a verdict computed from a loop that never executed (§9). It also asserts a false
provenance (*"identities came from the `<sys>/class/tty` listing"* with count 0) in prose written for
a Linux container. `linux.jq` admits `supported` here too. (iii) P12 — the one clause where `macos.jq` is **stricter than `linux.jq`**, because
this is the platform its mechanism is load-bearing on (the status set `supported`-or-`degraded` it
shares with P2 and P13) — exports **no wall-clock witness**: its anti-spin claim is
`idle_edges_in_200_passes: 0` with no elapsed time and no live-master negative control, where P6
reports `elapsed_ms: 163` over its 64 passes.
<!-- ANNOTATION 2026-08-05 (§5): *Fixed in §3.50* — and the unit named here is the one thing that
     could not be copied from P6. The tight window costs 134–175 us on Linux 7.0.0-29, so
     `elapsed_ms` would have printed `0` and witnessed nothing; the witness is microseconds. §3.50
     also adds the positive control this item did not ask for and the zero needs. -->
(iv) `P10.slave_to_master_targetward.peer_pending_input_bytes` reads **0** beside
`bytes_recovered_by_peer: 1024` in 3 of 3 runs, so FIONREAD on a Darwin ptmx master is
direction-dependently wrong, and `0` there cannot be read as "empty".
<!-- ANNOTATION 2026-08-05 (§5): *Fixed in §3.50*, and the tally is corrected: **6 of 6**, not 3 of
     3. The same 0/1024 targetward and 1022/1022 hostward reading is byte-identical across
     `macos-24.6.0-2026-08-05-7ead470-tier3{,-2,-3}.json` AND
     `macos-24.6.0-2026-08-05-1a9a8fc-tier3{,-2,-3}.json` — two binaries, six captures, verified
     field by field against the committed artifacts rather than recalled. -->
(v) Shipped P10 prose says
*"`refill_reproduced_total` … on Linux it usually is not"*, which the committed `-05b` triple reads
as **3 of 6**. That is *not* filed as a defect: §3.44's 20-sample calibration records "reproduces
`total()` once in 20", which supports the shipped sentence, and six samples do not overturn twenty.
It is recorded only so a later reader diffing the triple alone does not mistake 3-of-6 for a
contradiction. Its `+2048 or +9216, bimodal` is likewise correctly scoped to "across 20 samples" and
is not a §16.13 substitution — the committed triple's uniform +2048 is the small-sample view of it.

**The comparability claim needs narrowing, and there is a committed counterexample.** `probe_set`
digests only `(id, question)` pairs — observations are deliberately excluded — so the safe direction
is the only sound one: an *unequal* fingerprint means two runs do not ask the same questions. The
converse is false and this tree proves it: `fa4b12d`, `71fc5a8`, `7ead470` and `4b78fff` all print
`a131e1f4b46d6c83` while observation fields were added across them (P9 gained
`zero_timeout_by_fd_state_and_mask`, P10 gained `recheck`, macOS P6 went 4 → 8 observation keys and
P7 5 → 8). Counted as newly-present scalar leaf paths under `.probes[].observations`, macOS alone
gains **32** between `7ead470` and `1a9a8fc` and **35** between `fa4b12d` and `1a9a8fc` — all under
one unchanged digest. Diffing this triple against the `-05b` triple is lawful because they are the **same
binary**, not because the fingerprints match.

### 3.46 §3.43's pre-registration came true in the failing direction: a converted test went red on the Darwin rig, and named its provider

**Design:** §7 — no one-way decision on single-kernel evidence.

§3.43 pre-registered: *"on the next Mac run the six converted tests execute against the rig instead
of skipping; **if any goes red it will name the provider it used**, and `p7_p5` continues to
self-skip there as it always has."* One did.

**`p4_free_for_all::free_for_all_endpoint_lets_concurrent_writers_both_reach_device`**, captured
verbatim: `assertion left == right failed: device received 32754 bytes, expected 32768 (a
free-for-all writer was blocked): {"behavior":"recv","mode":"client","pass":false,"received":32754,
"timed_out":true,"tool":"serial-nexus-sim"}` — preceded by its own first line of output, `RIG: this
test is running on the crossover rig (/dev/cu.usbserial-BH00L4KU <-> /dev/cu.usbserial-BH00LL8O),
not the sim null modem`. **The safety property §3.43 promised held exactly**: a red test that names
the provider it used, never a silent pass.

**This is not a regression from green.** At `7ead470` the test called `serial_pair()` and self-skipped
on Darwin with *"no serial device on this platform"*; at HEAD it calls `serial_pair_or_rig()` and
executes. This is the **first time it has ever run on this platform**, and it fails on first
hardware execution.

**It is not load, and it is not the deadline — both were measured rather than assumed (§8).** On a
quiet box (load 1.0–1.5, no competing builds, no analysis agents), **12 of 12 reps failed** at the
committed 30 s deadline, losing between **5 and 31 bytes** of 32768. Raising *only* the sink deadline
to 120 s produced one clean pass **in 5.00 s** and one failure that still lost 2 bytes after 122 s.
So a healthy run completes in ~5 s against a 30 s budget — the deadline was never tight.

**What that licenses, stated exactly, because §6 forbids the shorter sentence.** Every failing
observation carries `timed_out: true`, and §6's rule is that the sim marks `timed_out` *precisely so
a deadline is never read as a drop*. So the measured claim is **"not recovered within 4× the
committed deadline, on a path where a healthy run finishes in 5 s"** — not "lost". The distinction is
not pedantry here: it is the difference between a data-plane loss and a stall, and this session did
not separate them. **The rate is 1 clean run in 14, and the whole-gate failure is
deliberately not in that denominator**: that run was taken at load 3.2–3.3 with analysis agents
running, and §8 forbids reading a flake rate under uncontrolled load. It is a fifteenth observation
of the same failure, not a fifteenth sample.

**What the failure is narrowed to, and what it is not.** The same rig moves 32768 bytes byte-exact
in both directions at 250000 baud in `serial_hardware.rs` (4 passed, same session, same ports), and
the other five `serial_pair_or_rig()` call sites — `p4_send:52`, `p4_exclusivity:224` and
`p8_map:419/559/694` — all passed in the whole-gate run. (`harness_contract` is **not** a call site,
though it names the provider: its
`the_pair_provider_prefers_software_and_falls_back_to_the_rig` is a pure unit test of
`choose_pair_source` that never opens a port, so its green says nothing about the wire and must not
be counted as hardware evidence.) So the wire, the adapters and the single-writer path are byte-exact,
and what is left is **two concurrent writers merging onto one free-for-all serial endpoint**.
**Mechanism is NOT established** and no root cause is claimed here: per §9 that would need an
independent adversarial verifier, and none has run. The candidates that remain open are the
free-for-all merge dropping tail bytes at the serial node, the Darwin USB-serial driver's tail
behaviour, and the sim sink's read loop.

**What this does to §3.43's substantive claim.** Its evidence — *"6 of the 7 `serial_pair()` tests
pass byte-exact over the real FT232R wire"* — was gathered **on Linux by forcing the fallback**, and
it does **not** transfer to Darwin, which is the only platform where that arm is the default rather
than a forced one. §3.37 declined this change on exactly the grounds that §7 forbids taking it on one
box's evidence; §3.43 overturned the decline on Linux measurements. The overturn is not reversed
here — the seam is still correct and its failure mode still names itself — but the record must say
that the "6 of 7" figure is a **Linux** figure, and that on Darwin it is 5 of 6 with this one red.

### 3.47 §3.46 root-caused: a USB-serial port that is not open does not receive

**Design:** §9 — a guard must assert the property the product promises, never a proxy in time.

§3.46 recorded `p4_free_for_all::free_for_all_endpoint_lets_concurrent_writers_both_reach_device`
failing on the Darwin rig, 12 of 12, losing 5–31 bytes of 32768, with **mechanism not established
and no root cause claimed**. It is established here, and it is not a Darwin property.

**It reproduces on Linux.** Same test, same shape, on the FT232R crossover at 115200: **2 of 5
failed**, losing 1–2 bytes, on an idle box (load 0.58). So §3.46's framing as a Darwin observation
was too narrow — this is a property of the real-UART path on both kernels, and the Darwin run was
simply the first place a converted test met real silicon.

**The driver's own counters locate the loss below the daemon and below the sim.** On a failing rep,
`TIOCGICOUNT` read **`tx_delta = 32768` on the transmitting port and `rx_delta = 32754` on the
receiving one** — with `overrun`, `buf_overrun`, `frame` and `parity` all **0**. The daemon sent
every byte; 14 never reached the far driver at all; nothing reported an error. The sim received
exactly what the driver counted.

**A single-writer control refuted the obvious hypothesis.** The harness's own doc comment warns that
"a flow-control-less UART drops bytes under a raw high-volume read", which made the raw sink the
prime suspect. One writer into one raw reader, no daemon, 32768 bytes at 115200: **10 of 10 clean,
`tx_delta == rx_delta` every time.** The raw read is not lossy here. Recorded as a refutation
because §9 makes it as load-bearing as the confirmation — and because that doc comment will attract
the same guess again.

**The mechanism is the test's ordering, and it is linear in the delay.** The control differed from
the test in one respect: it opened the *sink* first. Re-running it the test's way — writer first,
sink after a delay *d* — gives:

| sink opens at | bytes lost | 11520 B/s predicts |
|---|---|---|
| +0.0 s (spawn latency only) | 21, 26, 27 | — |
| +0.2 s | 2323, 2321, 2357 | 2304 |
| +0.5 s | 5801, 5792, 5815 | 5760 |

**The loss is exactly the airtime.** 115200 baud is 11520 bytes/s, and the deficit tracks *d* ×
11520 across an order of magnitude. A USB-serial port that is not open does not receive: the driver
submits no URBs, the adapter's small FIFO overflows, and the bytes are gone — counted as sent,
never counted as received, with no error anywhere.

**Why it was invisible until now.** On the sim null modem the same ordering is harmless, because a
pts buffers whether or not a reader is attached. The test was written against that provider and its
module doc concluded, from the daemon's own backpressure invariants, that *"ordering of the writers
vs. the sink cannot lose bytes — only block until drained."* That conclusion is sound for every hop
**inside** the daemon and false for the last one: a UART has no flow control and no queue. The
comment is corrected rather than deleted, because the reasoning it records is exactly right up to
the point where it stops being right.

**Decision.** `serial-nexus-sim client` gains **`--open-file`**, a handshake one step earlier than
the existing `--ready-file`: created the instant the port is open and configured, before any read.
`--ready-file` cannot serve here — it fires on the first byte *read*, which cannot happen before a
byte is sent, and the hazard is precisely bytes sent before the far end opens. The test opens the
sink first, gates the writers on that file, and then collects the sink's verdict.

**Measured after:** 8 of 8 green on the rig where the old ordering failed 2 of 5, `LOST = 0` in 5 of
5 direct control reps, and the software null-modem path unchanged.

**This is a harness-fidelity defect, not a product defect**, and the distinction is load-bearing: the
daemon transmitted every byte it was given, on both kernels, in every failing run. What was wrong is
that the test asserted a lossless wire while arranging for part of the transmission to happen before
anyone was listening — §9's proxy in time, in a place no software double could expose.

### 3.48 P4 certified a resolver that had resolved nothing, and both gates admitted it

**Design:** §9 — a verdict computed from a loop that never executed is vacuous.

**Reality (notes §3.45 (ii), traced and reproduced).** `p4_resolver`'s `skipped` early return needs
`adapters.is_empty() && candidates.is_empty()`. On Darwin `adapters` is empty (no `/dev/serial/by-id`)
but `candidates` is 4 (the `cu.*` scan), so the early return does not fire, `for a in &adapters` runs
**zero times**, `all_resolved` keeps its initialised `true`, and the probe reports **`supported`**
with the consequence *"Resolver produces canonical identities; configs survive replug and cold
start"* — plus a provenance sentence claiming identities *"came from the `<sys>/class/tty` listing"*
on a box with no such listing and `sysfs_only: 0`. All three Darwin artifacts carry it byte-identical.

**Not a macOS-only defect.** The same zero-iteration loop mis-verdicts a **Linux** box: a USB adapter
with no serial number publishes no by-id link, so `adapters` is empty and P4 reports `supported` for
a box whose only identity is `by-path:` — §12's documented instability, reported as if configs
survived replug. The existing `Status::Degraded` arm was unreachable on the very box it was written
for.

**`degraded`, not `skipped`.** The resolver *ran*, over all four passive sources, and returned four
candidates whose identities are `raw:`. The question was asked and answered negatively — that is not
P8's "unmeasurable here". And the answer is not merely unproven but **false**: `docs/macos.md`
records that a node configured with a `usb:` identity resolves to nothing and stays `waiting`
forever, which is exactly what the `supported` consequence promises against. The report already says
`degraded` for this same fact one section away, in `environment()`'s by-id check; one report must not
answer one question two ways. `degraded` is also the durable signal — if the deferred IOKit backend
lands, P4 flips to `supported` and says so, where a `skipped` would stay silent forever.

**The RES-2 decline is preserved (§5).** Review 32 recorded that P4 *"stays `supported` in a no-udev
environment by design"*. That case is unchanged: one sysfs-only USB device still gives `canonical: 1`
and a byte-identical consequence. What is narrowed is only the case RES-2 never contemplated — a tree
where **nothing at all** resolved.

**Both expectation files gained the clause, and it is strict: a `supported` P4 must state a
population and that population must be non-zero.**
<!-- ANNOTATION 2026-08-05 (§5). **This paragraph originally justified an ABSTAIN arm, on a premise
     that is false, and the arm has been removed.** It read: "a report that omits `canonical`
     abstains … every artifact in `docs/doctor/` predates the field, so failing on absence would
     turn a defect detector into an instrument-version detector — the gate would go red on 19 of 19
     committed reports."
     The gate is never run over those reports. `.github/workflows/ci.yml` pipes a LIVE
     `serial-nexus-doctor --json` into it on both lanes, and no test, script or job runs it over
     `docs/doctor/*.json`. The 19-of-19 figure came from running it over the archive BY HAND while
     checking this change — a self-inflicted failure, then treated as evidence about the gate.
     Two further facts settle it. The one legitimate stored-file use is an operator validating a
     capture they have just taken (`docs/macos.md` records exactly that), and such a capture comes
     from the current binary, so it carries the key and strict costs it nothing. And the archive was
     never uniformly gate-clean to begin with: six of the nineteen reports predate P13 and fail that
     clause regardless, one of them a macOS capture — so "the gate passes the archive" was never a
     property there was any point preserving.
     Recorded rather than quietly rewritten because §9 makes a refuted premise as load-bearing as a
     confirmed one, and because the same mistaken reasoning was about to be applied to the P10 and
     P12 witness clauses of §3.50 before it was caught. Those are strict for the same reason. -->

**So the "a current binary always states its population" half lives where it can be honest**:
`p4_always_reports_its_population` in the probe's own tests, over both a resolving and a
non-resolving fixture, where no archived report can satisfy or violate it.

**Fail-first, three ways.** Deleting the `canonical` observation fails the probe guard
(*"P4 reported no population for the `degraded` verdict"*). Planting `status: supported` on a
zero-population report is rejected by both gates. Stripping `canonical` from a report and calling it
`supported` must still be **accepted**, and the guard asserts that direction too — so a later
tightening that fails the archive fails here first. A third guard pins the two gate files as carrying
the clause byte-for-byte, so an edit to one that forgets the other cannot silently reopen the hole on
the lane nobody is standing on.

**Verified unchanged where it must be:** the real dev box still reports `supported` with
`canonical: 2` and a character-identical consequence; the Darwin shape, reproduced on Linux through
the `--dev-root` seam, now reports `degraded` with `canonical: 0`, `unidentified: 4`, and each
device's `raw:` identity named.

### 3.50 P12's zero and P10's zero: two reported numbers that could not be read, and the witnesses that make them readable

**Design:** §15.17 — a probe emits its raw measurements and names a differing kernel's observation.
No amendment is owed; this brings P10 and P12 into compliance with it. Closes §3.45 (iii) and (iv).

**Both defects are the same shape: a `0` printed with nothing beside it that says what the `0`
means.** P12 printed `idle_edges_in_200_passes: 0` and concluded `supported` — with no elapsed time,
no control, and no way to tell a quiet kernel from an instrument that could not have posted an edge
at all. P10 printed `peer_pending_input_bytes: 0` beside `bytes_recovered_by_peer: 1024` and left the
contradiction to be noticed by eye. Both fixes are additive: **no existing field moved**, and
`idle_edges_in_200_passes` keeps its key, its 200-pass count and its unpaced shape precisely because
six committed artifacts carry it (§16.13).

**The witness had to be microseconds, and that is measured rather than styled.** P12's idle loop is
untimed *and* unpaced. On Linux 7.0.0-29 the 200 back-to-back `poll`+`read` passes over a hung-up
master cost **134 / 138 / 175 / 137 us** across four runs — a `elapsed_ms` field copied from P6 would
have printed `0` on every one of them and taught a reader nothing. The proof is not an argument: with
the paced window's pause forced to zero, 64 passes measured **33 us**. Darwin's zero-timeout poll on
a hung-up master is 1458–2057 ns (§3.45 B), so the same loop is far under a millisecond there too.

**A second window is paced at the daemon's own `IDLE_POLL` (5 ms), because that is the loop
`nodes/pty.rs` runs.** "200 back-to-back syscalls post no edge" and "an idle master posts no edge at
the cadence the daemon polls at" are different claims, and only the pair discriminates: an edge
appearing only in the paced window is time-driven, one appearing only in the tight window is
syscall-driven. 64 passes × 5 ms measures **326501 / 329049 / 325851 / 329361 us** here.

**The zero needs a positive control on the same latch instance, and this is the load-bearing half.**
The three shape trials each build their own pty and their own `SessionLatch`, so a latch that went
deaf on the *idle* pair is invisible to them. `EV_CLEAR` on a master already hung up at `watch()`
time — whose registration edge `watch()` deliberately swallows — is exactly the shape where "0 edges"
could be *structurally guaranteed* rather than measured. So after the idle windows the probe opens
and closes a slave on the **same master through the same latch** and asks again; if that boundary
posts nothing, every zero above is an inert instrument and `p12_verdict` refuses `supported`, saying
`unmeasured`, not `quiet`. A third window (slave open, idle) is the negative control: an edge *there*
fires §6 detach-release mid-session and hands away the write lock of a client that never left — the
mirror image of the spin, and nothing had measured it.

**The decision is a pure function, because the platform of record cannot produce a single row of its
input.** `SessionLatch` is inert on Linux, so no Linux run can exercise the Darwin verdict. `P12Facts`
→ `p12_verdict` factors the judgement out of the fds, and `p12_verdict_refuses_supported_from_an_unproven_latch`
regression-tests it here. §9's proxy-in-space rule cuts both ways: a decision may not be *asserted* on
a kernel it was not measured on, but a pure function of measured numbers may and must be.

**Linux runs the windows anyway, and reports them.** The `skipped` verdict and its wording do not
move, but `control_session_edge: false` beside a full set of executed passes is the inert arm
**proving itself inert** — the negative control the kernel that depends on this mechanism cannot
provide for itself. A Linux report where that field read `true` would mean the latch had grown a
second implementation nobody measured. Measured here: `idle_window_tight` 200/200 poll events, all
`EIO`; `idle_window_paced` 64/64, all `EIO`; `live_session_window` 0 poll events, all `EAGAIN` (the
attached-client shape, visibly different); `control_session_edge_us` 81505–82263.

**P10: the field is wrong on one kernel, and its being wrong is the finding — so it is not deleted.**
Verified against the artifacts rather than recalled: `peer_pending_input_bytes: 0` beside
`bytes_recovered_by_peer: 1024` targetward, and `1022` beside `1022` hostward, byte-identical across
**6 of 6** Darwin captures spanning two binaries (`…-7ead470-tier3{,-2,-3}` and
`…-1a9a8fc-tier3{,-2,-3}`). §3.45 (iv) said 3 of 3; corrected there in place.

**`pp < recovered` is NOT the fault signature.** Linux answers this ioctl correctly and still reads
*less* than the drain recovers, saturating at **4095** (the n_tty read buffer) against 13824–15360
recoverable, in 6 of 6 committed `-05b` observations and every run taken here. Undercounting is a
documented staging cap; claiming **empty** is the fault. `fionread_trust` separates them —
`agrees` / `undercounts` / `overcounts` / `contradicted-empty` / `nothing-to-check` / `unavailable` —
and `Some(0)` with nothing recovered classifies `nothing-to-check` rather than `agrees` on purpose:
an empty queue agreeing with an empty reading is not evidence the instrument works, and a gate
reading `agrees` off it would pass vacuously.

**The old sample could not prove a contradiction; the new one can.** `peer_pending_input` is taken
mid-measurement, *before* the second fill pass, so a disagreement there always had an innocent
reading — bytes arrived later. `peer_pending_input_bytes_at_drain` is taken as the statement
**immediately before** `p10_drain`, with both fills finished in `EAGAIN` and no writer anywhere, so
nothing runs between the ioctl and the first `read(2)` that can contradict it.

**Two pre-registrations, so the next Darwin run settles the mechanism rather than restating it.**
(1) `writer_pending_input_bytes` — `FIONREAD` on the *written* fd, which has nothing to read. Linux
answers `Some(0)` in both directions in every run taken here. If a Darwin master answers out of the
tty's **input** queue rather than its readable one, the hostward figure (the master's own reading)
comes back non-zero and equal to that direction's depth; a `0` there **refutes** that reading and
leaves the mechanism open. (2) The classification is interpretable only under
`slave_termios_mode: raw`, which the probe already reports — with ECHO on, a master legitimately has
echoed bytes to read.

<!-- ANNOTATION 2026-08-06 (§5). **Pre-registration (1) is answered, and it confirms rather
     than refutes.** `writer_pending_input_bytes` reads **1022 hostward and 0 targetward** on
     Darwin — 9 of 9 across three commits on the x86_64 rig, and again on an Apple M4 running
     Darwin 27.0.0 (notes §3.72) — against `Some(0)` in both directions in every Linux capture.
     The hostward figure is the master's own reading, it is non-zero, and it equals that
     direction's depth exactly, which is the condition written above. So on Darwin a pty
     master's `FIONREAD` answers out of the tty's **input** queue rather than its readable one,
     and the `contradicted-empty` classification on the targetward direction is that same
     mechanism seen from the other side rather than a second defect.
     Recorded here rather than only in §3.72 because this is the paragraph a later session
     greps for, and leaving a settled pre-registration reading as open is the defect §3.71's
     sibling entries keep finding. What the M4 adds over the earlier captures is not the
     answer — it was already 9 of 9 — but its scope: the same reading across a different
     architecture and a different kernel major rules out a per-box or per-build artifact.
     Still **not** established (§9): whether this is `n_tty`-versus-BSD-tty structure or an
     XNU implementation detail. `bytes_recovered_by_peer` is unaffected either way — it is a
     drain, not an ioctl, and remains the field to size anything against. -->



**P10's status is deliberately unchanged.** Its `degraded` arm means "the depth you are reading is
another configuration's" (§3.34). Folding an auxiliary ioctl fault into that word would make Darwin
permanently yellow, masking a real mode degradation later, and would lose the direction — a
probe-level word cannot say *which* of the two blocks is affected, and only one is. §7's "name the
observation" is met by a machine-readable field per direction plus an unmissable consequence
sentence; the depth question the probe exists to answer was answered correctly.

**Gates: presence, never answer.** Both expectation files require the three P10 keys in *both*
directions and P12's control plus a numeric `idle_window_paced.elapsed_us`/`passes`, with a named-error
hatch (`idle_windows`) for a box where the windows could not run. A kernel whose FIONREAD differs must
**report**, not fail (§7). Ten cases were run rather than predicted: the live post-fix report is
accepted; the committed pre-fix Linux artifact is rejected; deleting the trust key from **one**
direction, deleting either new P10 key, reducing P12 to the bare `idle_edges_in_200_passes`, deleting
`control_session_edge`, and deleting the paced `elapsed_us` are each rejected; the named-error hatch
and a `skipped` P10 with no observations are each accepted. On the macOS lane the same three cases
were run against a Darwin artifact — the pre-fix capture passes HEAD's `macos.jq` and is rejected by
the new one, and a synthesized post-fix report carrying the finding's own values (`0`/1024
targetward, `contradicted-empty`) is accepted.

**Fail-first, six mutations, each red with the message it names (§9).** Deleting `p12_verdict`'s
control arm makes the vacuous row report `supported` and reds
*"P12 reported `supported` from a latch that posted no edge for a boundary this probe produced on
purpose"*. Deleting its wall-clock arm greens the `no_window` row. Moving the at-drain sample to
*after* the drain reds the Linux calibration with *"FIONREAD on this Linux pty classified
`contradicted-empty` (Some(0) readable at the drain, 15360 recovered)"* — which is the proof that
guard tests the **ordering** the contradiction claim rests on, not merely the ioctl's existence.
Pointing the writer sample at the peer reds it with `Some(4095)` against `Some(0)`, naming the
invalidated discriminator. Classifying `Some(0)` as `Agrees` reds the Darwin row; classifying the
4095 cap as `ContradictedEmpty` reds the Linux row *and* the note guard with *"Linux's 4095-of-15360
is the n_tty read-buffer cap, not a fault"* — proving the warning cannot be trained away by firing
everywhere. Forcing the paced pause to zero reds the window guard at **33 us**, which is the
microsecond decision restated as a failure.

**The Linux output moved exactly where it was predicted to and nowhere else.** Comparing a pristine
`df48bfc` binary against this one on the same box: **41 newly-present observation leaf paths, zero
absent** (299 → 340) — six on P10 (three keys × two directions) and the rest a P12 subtree that was
empty here. `probe_set` reads `a131e1f4b46d6c83` on both, which is §3.45's counterexample to
"equal `probe_set` ⇒ field-by-field comparable", now created **deliberately** rather than found.
One field that *looks* like a move is not one: `recheck.room_republished_minus_room_freed` read 9216
in the first pre-change run and 2048 after, so it was measured on both binaries, four runs each,
under the same load — both produce the documented 2048/9216 bimodal (pre: 2048 ×4, 9216 ×4; post:
2048 ×7, 9216 ×1), and all six committed `-05b` observations read 2048. Scheduling, not the change
(§8).

**Cost, named because it is the whole price:** the doctor run goes **2.75 s → 3.74 s** on Linux
(3 runs each, load 2.25) — *superseded: re-measured at `f8315cc` on an idle box the passive run is
**3.94 s** (5 runs, spread 3.93–3.95) and a Tier-3 rig run is **11.6 s** (3 runs, 11.55–11.58), the
extra 0.2 s passive being §3.52's ladder and P9 rework (§3.53)* — — two paced windows and a live one at 64 × 5 ms, an ≤80 ms control, and the
three shape trials, which on Linux used to be skipped by the early return and now execute. On macOS
the shape trials already ran, so the delta there is the windows alone.

**Gates:** `cargo build --workspace --locked`; `cargo test --workspace --locked --no-fail-fast` —
**744 → 766 passing / 0 failed / 4 ignored** across 114 test-result targets (measured at the end of the 2026-08-05 run with §3.49's four and §3.51's nine landed alongside), the +5 attributable here being this
entry's guards on the existing `serial-nexus-doctor` target (all five run on Linux; three on macOS,
since one is Linux-gated and one not-macOS-gated, both deliberately); `cargo fmt --all --check`;
`cargo clippy --workspace --all-targets --locked -- -D warnings`;
`serial-nexus-doctor --json | jq -e -f expectations/linux.jq` exits 0. macOS is unrun here — this was
a Linux box, and the P12 probe body is unmeasurable on it by construction, which is exactly why the
verdict was factored into a pure function.

---

### 3.49 P5's tier was sequenced behind its certificate, and "uncertified" was standing in for "unmeasurable"

**Design:** §15.21 (the certificate is the precondition a tiered run starts from) and §7 (a kernel
that differs is `degraded` **with the observation named**).

**The control flow, quoted, because the defect is entirely in it.** `p5_verdict` is a worst-first
precedence chain of six early-returning arms: (1) integrity failures → `Unsupported`; (2) `!clean`
→ `Degraded`; (3) `!hung_up.is_empty()` → `Degraded`; (4) `!uncertified.is_empty()` → `Degraded`;
(5) `any_uart` → `Supported`, **and this was the only site that computed `facts.tier()` and emitted
"**Tier 3** …"**; (6) `else` → `Supported`, the "characterization does not run on this platform at
all" arm. So the tier printed if and only if the chain reached arm 5 — i.e. only when *every*
certificate item passed.

**What that cost, measured, not argued.** Of the five inputs to
`p5_verdict(clean, any_uart, failures, hung_up, facts)`, **four are identical** between the
2026-08-05 Linux triple (`linux-7.0-2026-08-05b-tier3{,-2,-3}.json`) and the Darwin triple of the
*same binary* (`macos-24.6.0-2026-08-05-1a9a8fc-tier3{,-2,-3}.json`): `clean=true` (both ports read
`paired with …` in both directions on both), `any_uart=true`, `hung_up=[]`, and
`facts = {discovered_pairs: 1, mismatch_pairs: 1, loopbacks: 0}`. The single differing input is
`failures` — empty on Linux, three non-integrity `CertFailure`s on Darwin (`BH00L4KU: icounter`,
`BH00LL8O: icounter`, `BH00L4KU ↔ BH00LL8O: deliberate_mismatch`). Arm 4 therefore fired on Darwin,
arm 5 never ran, and `grep -c "Tier [0-9]"` over all three Darwin captures is **0** against
`**Tier 3**` in all three Linux ones. A cross-wired FT232R pair had just certified `rate_ladder=true`
over physical silicon and its topology went unnamed. **The tier is a discovery fact and it was
sequenced behind a certificate fact** — precisely the distinction `RigFacts`' two counts exist to
preserve (§3.42, §3.45 E).

**And the second half: "uncertified" is not an observation.** The two items on that Darwin list are
**structurally unmeasurable off Linux**, read from the code rather than assumed. `sys::read_icounts`
has a `#[cfg(not(target_os = "linux"))]` arm that is an unconditional `Err(ENOTSUP)`. Exactly two
certificate items read it: `icounter` in `p5_certify_port` *is* `read_icounts(fd).is_ok()`, so it is
false for every fd off Linux, a genuine FTDI included; and `deliberate_mismatch` in
`p5_certify_pair` is `!contains_sub(&got, unit) && after > before`, where both counter reads collapse
to `unwrap_or(0)`/`unwrap_or(before)` off Linux, so the second conjunct is `0 > 0` and the item can
never be observed **whatever the wire did**. Its first conjunct — the corruption itself — *is*
measurable there and the bulk pattern really was transmitted, which is why the two get different
explanations rather than one shared sentence.

**Which items are NOT excused, and why that list is load-bearing.** `custom_baud` and `break` (both
read **true** on Darwin in 3 of 3 — they measured fine and merely could have failed), `reopen`,
`pair_reopen` and `mismatch_reopen` (rig states every kernel observes), and `rate_ladder` (the
integrity item; it reads *bytes*, not counters, and Darwin certifies it true over the physical
crossover). On **Linux** none of the five is structural either: `icounter=false` there is a real
measurement — a pts answers `ENOTTY` because that driver does not implement the ioctl — so the
mechanism clause must never appear in a Linux report.

**The fix, in the shape that cannot widen.**
1. The tier sentence moved out of the status decision into `p5_tier_scope(facts)`, printed by arm 4
   as `Topology: **Tier 3** …` (gated on `any_uart`, since a `pair_reopen` failure reaches arm 4 with
   nothing certified). **The arms were deliberately not reordered:** moving arm 5 above arm 4 would
   have flipped Darwin back to `supported` and undone §3.45 E's honest direction, re-breaking
   §15.21's "a precondition has to be able to fail". Arms 2 and 3 deliberately get **no** tier —
   there discovery itself is what is in doubt, and a topology word standing in for an unestablished
   topology is the §9 proxy in space.
2. The mechanism is carried as **data**: `CertFailure` gains
   `unmeasurable_here: Option<&'static str>`, set from the new `sys::ICOUNTS_SUPPORTED` at exactly
   the two counter-reading `fail_if` sites through one helper. The verdict reads it off the failure
   and never matches item names, so the excuse cannot widen to an item that genuinely failed on a
   path this kernel does measure. `fail_if` takes the answer as a required fourth argument and
   `Certificate::unavailable`/`pair_reopen` answer `None` explicitly, so the compiler enforces that
   every new item states which kind it is.
3. `p5_certify_port`/`p5_certify_pair` split their pure folds out as `p5_port_certificate` and
   `p5_pair_certificate` — the same move `p5_verdict` got, for the same reason: the part that must be
   tested is the *classification*, and it cannot be reached through the measuring function without a
   bench. A pts is not a substitute (`p5_is_uart` rejects one on both kernels), so a pts-driven guard
   would pass vacuously everywhere (§9).

**`sys::ICOUNTS_SUPPORTED` lives beside the two `read_icounts` arms** so it cannot drift from them,
with a guard asserting it equals `cfg!(target_os = "linux")` and — on the stub arm only — that an
invalid fd still answers `ENOTSUP`, which is the claim P5's consequence makes to the operator.

**Fail-first, four mutations, three of them reproducible on Linux and the fourth stated as not.**
Reproduced in-tree by reverting each edit in place (§8: no `git stash`), failing-test names captured
verbatim, then re-applied:
- **M1** — drop `{topology}` from arm 4's format string. Fails
  `probes::tests::an_uncertified_rig_still_names_the_tier_discovery_found`:
  *"a degraded certificate did not name its tier: The rig carries data, but is not fully characterized
  (usb-A: break) — a tier leaning on that item would be running uncertified (§15.21). Everything else
  above is certified."* This is the Darwin defect reproduced **on Linux** with a *measurable* failure,
  so the guard is not platform-shaped.
- **M3** — widen the excuse to every platform (`p5_icounts_unmeasurable` returns `Some(why)` always).
  Fails `probes::tests::the_counter_items_are_platform_excused_exactly_where_the_ioctl_is_absent`:
  *"assertion `left == right` failed: icounter excused on the wrong platform / left: true /
  right: false"*.
- **M4** — widen the *matcher* instead of the flag (`f.unmeasurable_here.or(Some(P5_WHY_NO_ICOUNTER))`
  in the fold). Fails
  `probes::tests::only_the_items_this_kernel_cannot_measure_are_excused_by_the_platform` on its
  negative assertion, printing *"… A: icounter, B: icounter, A: break cannot be measured on this
  kernel at all: …"*. This is the assertion that proves the guard's **matcher** and not merely its
  walker: an excuse that fired on everything would otherwise read as a pass.
- **M2** — revert the mechanism wiring (`p5_icounts_unmeasurable` returns `None` always). This
  reproduces today's shipped behaviour and is **invisible on Linux** — the platform-binding guard
  passes trivially there. Stated rather than hidden; M3 is the Linux-visible mutation for the same
  defect, pinning the wiring from the other side.

**Linux is byte-identical, checked rather than assumed.** `p5_verdict(true, true, &[], &[], paired())`
was asserted character-for-character against the committed
`docs/doctor/linux-7.0-2026-08-05b-tier3.json` P5 consequence, and both certificate observation lines
(`custom_baud=true break=true modem[…] icounter=true`, `rate_ladder=true
deliberate_mismatch_observed=true`) against the same artifact's observation values; a full
`serial-nexus-doctor --json` before and after differs only in the build commit and in timing numbers.
No `expectations/*.jq` change is owed: no status moves, and neither gate inspects P5's consequence
text.

**Pre-registered Darwin prediction, so the next capture can refute this.** The next macOS rig run
must print, in one `degraded` P5 line: `Topology: **Tier 3** — 1 cross-wired pair, …`; the two
`icounter` items grouped into one "cannot be measured on this kernel at all" sentence naming
TIOCGICOUNT; `deliberate_mismatch` in its own sentence naming that the traffic *was transmitted*;
and `break`/`custom_baud`/`rate_ladder` in **neither** clause.

**Named, not fixed (so it is not a silent re-fix later, §5).** Three sentences in shipped prose still
describe P5's UART predicate as *"TIOCGICOUNT, which is Linux-only"*, which §3.42 made false when the
predicate became the `TIOCMGET || TIOCGICOUNT` disjunction: `p5_verdict`'s final `else` arm and its
paired test assertion, the `supported` bullet in `docs/serial-nexus-doctor.md`, and `docs/macos.md`
around lines 707 and 889. They are adjacent to this change and out of its scope; the correction is
owed and is recorded here rather than folded in quietly.

<!-- ANNOTATION 2026-08-12 (§3.75, plan §18 item 23a). **All three are discharged.** The first two
     were corrected in an earlier sweep and now quote the old wording as superseded history — the
     uncharacterized-arm constant in `doctor/src/probes.rs` (with a guard forbidding the string
     "Linux-only" in that constant) and `docs/serial-nexus-doctor.md`'s `supported` bullet. The
     third was still live and is corrected in place, keeping the old sentence beside it: it was
     wrong *twice over*, because P3's verdict never read the counters either (`p3_serial` turns on
     `custom_baud_ok && exclusivity_ok`, and the committed
     `macos-24.6.0-2026-08-05-1a9a8fc-tier3.json` reads P3 `supported` with
     `tiocgicount_supported: false`), and P5 degrades on exactly two certificate *items*. -->

---

### 3.51 "Equal `probe_set` ⇒ comparable field by field" is false, and the tree holds the counterexample

**Design:** §15.44 (new, amend-first per §5), §16.13.

**The defect.** `docs/doctor/README.md` and `Build::probe_set`'s doc comment presented the `probe set`
fingerprint as the check a reader runs before a field-by-field cross-kernel diff. It cannot be that
check. The digest covers the deduplicated, sorted set of each probe's `(id, question)` **strings** —
not the code that asks them, and not the cells they emit. The sound direction (unequal ⇒ not
comparable) was stated correctly in the Markdown renderer; nothing anywhere said the converse is
false, and the README's framing ("Read the Build block first", "only meaningful between two reports
whose probe set fingerprints are equal") invited exactly the wrong reading.

**The counterexample is committed, and this session recomputed every number in it** rather than
re-quoting the earlier commit message. Method: scalar **leaf paths** under `.probes[].observations`,
formed as `<probe id>.<observation key>[.nested…]`, arrays collapsed to one `[]` step, values
excluded; extracted with an independent `jq` walker and cross-checked against the shipped
`--field-set` implementation, which reproduces all nineteen stored digests below.

| pair (both `probe_set = a131e1f4b46d6c83`) | leaf paths added | removed |
|---|---|---|
| macOS `7ead470` → `1a9a8fc` | **+65** | 0 |
| macOS `fa4b12d` → `1a9a8fc` | **+71** | 0 |
| macOS `fa4b12d` → `7ead470` | +6 | 0 |
| Linux `71fc5a8` → `4b78fff` | +65 | 1 |

**The 32/35 an earlier commit message quotes could not be reproduced under any collapsing tried, and
must not be re-quoted.** Collapsing sibling repetitions (P10's two directions, P7/P13's three shapes)
to `<probe id>.<leaf name>` gives 38 and 41, not 32 and 35. The *direction* of the claim is
unaffected — the counterexample is bigger than was stated, not smaller — and `AGENTS.md` §2 is
corrected in the same commit.

**The pair that makes it more than bookkeeping** is the small one. `fa4b12d` → `7ead470` adds only
six leaf paths (`bytes_recovered_by_peer`, `bytes_unrecoverable`, `slave_termios_mode` × 2
directions) — and that is the pair whose P10 hostward figure moves 4194304 → 1022, a factor of 4104,
under one unchanged digest. The tree's single most misleading measured pair sits inside the false
reading.

**A fifth instance landed while this was being written.** `df48bfc` (notes §3.48) added four P4
cells — `canonical`, `topology_only`, `unidentified`, `sysfs_tty_listing` — with P4's question
untouched, so the binary still prints `a131e1f4b46d6c83`. Measured on the dev box: a passive run at
`4b78fff` carries 6 P4 leaf paths, the same run at `df48bfc` carries 10. The defect reproduces on the
newest commit in the tree, which is why the fix is a field and not a prose correction.

**The decision: narrow the prose *and* add a second digest. Folding the keys into `probe set` is
declined, and the decline is measured** (§5 binds it against silent re-fixing).

*(a) narrowing alone is necessary but not sufficient.* A new field does not repair a false sentence,
so the prose is narrowed regardless. But narrowing alone retracts the field's headline value —
`Build::probe_set`'s doc sells "in one glance and with no repository access", and the replacement
rule ("compare commits") needs repository access and a source diff. A reader holding two JSON files,
the exact scenario the `Build` block was invented for, would be left with no machine-checkable
statement at all.

*(c) folding observation keys into `probe_set` is refuted by measurement, not by argument.* In this
tree some keys **are** the measurement: `P6.after_last_close.read_outcomes.EIO` (Linux) against
`…read_outcomes.eof` (Darwin); `revents_seen.POLLHUP` against `revents_seen.POLLIN|POLLHUP`;
`P8.slave_open_idle.epoll_flags_seen`, an empty histogram in the committed Linux runs whose key set is
whatever the kernel returned. Others are device identities: `P11./dev/ttyUSB0.…`,
`P5.usb:0403:6001:BH00L4KU:00 cert`, `P4.usb-FTDI_FT232R_USB_UART_BH00LL8O-if00-port0`. Consequences,
measured at `df48bfc` on this box: naming two ports adds **19** leaf paths to one binary's output
(two pty slaves; a rig adds more, since its device paths are themselves keys), and the same binary
emits **72 Linux-only and 22 macOS-only** paths across the two kernels, over 213 shared. Folding keys
in would therefore (i) make a passive and a rig run of one binary report themselves incomparable —
the P3-title failure `probe_set_fingerprint`'s choice 1 exists to prevent, (ii) make every
cross-kernel pair report itself incomparable — choice 3's failure, "the field being wrong in the
direction nobody would notice", (iii) move `a131e1f4b46d6c83`, orphaning every committed artifact and
the README index built on it, and (iv) destroy the one thing `probe_set` says truthfully.

*(b) the second digest, as a **cell-set** digest with an exactly provable contract.* Not a second
instrument-identity digest — that is unattainable from a report, and claiming it would re-commit this
very defect one level up. `field_set` digests the sorted, deduplicated set of scalar leaf **paths**
the report carries. Equal ⇒ the two reports carry exactly the same cells, so a field-by-field diff
has no missing ones (true by construction, unlike `probe_set`'s implied claim). Unequal ⇒ the cell
sets differ; restrict the diff to their intersection — and it cannot say *why* (binary moved,
platform differs, hardware differs, a histogram key was not observed). **Equal still does not certify
equal probe bodies**: a body change that alters a number without adding a key is invisible to it,
exactly as it is to `probe_set`. That sentence is printed in all three places — the field's doc, the
Markdown renderer, and the README — or the defect recurs one level up.

**No exclusion list and no heuristic.** Device-identity keys and outcome-keyed histograms are
*included*, and that is correct under the contract: if one run's P6 reports `read_outcomes.EIO` and
the other `read_outcomes.eof`, a field-by-field diff genuinely has missing cells, and reading
"EIO: 2 vs absent" as "Darwin returned zero EIOs" is a hazard the old rule silently permitted.

**Two design decisions taken from measurement rather than taste.**

1. **The scalar's JSON kind is excluded from the path.** Measured on the same-binary cross-kernel pair
   (`linux-7.0-2026-08-05b-tier3.json` at `4b78fff` against `macos-24.6.0-2026-08-05-1a9a8fc-tier3.json`
   at `1a9a8fc`, whose `*.rs`/`*.toml` diff is empty): **4 of 213** shared paths differ in kind, and all
   four are measurements — `P10.master_to_slave_hostward.pending_output_bytes` and its targetward twin
   read `number` where `TIOCOUTQ` answers and `null` where it does not, and
   `P7.{b_open_tcsetattr_close,c_open_write_close}.leading_bytes_hex[]` is a populated string array on
   Linux and empty on Darwin. A kind-sensitive digest would call two healthy runs of one binary
   incomparable.
2. **An empty array digests to the same path as a populated one** (`p[]` either way), for the same
   reason. An **empty object** digests as the path itself (`P8.slave_open_idle.epoll_flags_seen`), so
   an observation can never vanish from the shape entirely — the one case where a histogram's
   emptiness does move the digest, and that is honest: the cells really are absent.

**Measured, not assumed — the digest is not run-to-run noise.** Across all nineteen committed JSON
artifacts, every group of sequential runs of one binary shares one value. These digests were computed
**before the field existed**, which is what makes retroactive indexing lawful under §16.13: the
README table gains a column, the frozen files are not touched.

| artifact group | `probe_set` | `field_set` | leaf paths |
|---|---|---|---|
| `linux-7.0-2026-07-29-passive-{1,2,3}` | `01b257ece8c48470` | `9612da13d806026c` ×3 | 148 |
| `linux-7.0-2026-07-29-tier3`, `-2` | `01b257ece8c48470` | `76c9b8b293728e8e` ×2 | 192 |
| `macos-24.6.0-2026-07-30-tier3` | `01b257ece8c48470` | `94a11ac201de6613` | 141 |
| `macos-24.6.0-2026-08-05-tier3` (`fa4b12d`) | **`a131e1f4b46d6c83`** | **`36fd95f08831bb38`** | 164 |
| `macos-…-7ead470-tier3{,-2,-3}` | **`a131e1f4b46d6c83`** | **`e0047234b499d0c7`** ×3 | 170 |
| `linux-7.0-2026-08-05-tier3{,-2,-3}` (`71fc5a8`) | **`a131e1f4b46d6c83`** | **`64410fea8995f068`** ×3 | 221 |
| `macos-…-1a9a8fc-tier3{,-2,-3}` | **`a131e1f4b46d6c83`** | **`0c303d4cb11e3893`** ×3 | 235 |
| `linux-7.0-2026-08-05b-tier3{,-2,-3}` (`4b78fff`) | **`a131e1f4b46d6c83`** | **`88585243dafb4747`** ×3 | 285 |
| `linux-6.18-2026-07-29-tier3.md` | `01b257ece8c48470` | *not computable — Markdown, no `observations` array* | — |

One `probe_set` value, **five** `field_set` values, and the two known-bad pairs both fire. The
07-29 Tier-3 pair is two *different commits* with an equal `field_set`, so the field does not simply
redden everything. §9's tell applies: the portable form comes out **stricter** on the platform of
record, not weaker.

**What the first report carrying the field can and cannot be compared against** — because adding it
is itself an observation-shape change, of the `Build` block rather than of any probe. Against any
later report: by field equality, directly, with no repository access; that is the property narrowing
alone gives up. Against the nineteen frozen JSON artifacts: only by *recomputation*
(`serial-nexus-doctor --field-set <file>`), i.e. the comparison needs the tool — which is why the
README column records that recomputation once. Against `linux-6.18-2026-07-29-tier3.md`: not at all;
its digest is not computable and its cell set is **unknown**, which is never "equal" — the same rule
the README already applies to pre-2026-07-28 reports with no `probe set`.

**The Linux output did not move except where predicted.** Before and after, on the dev box at
`df48bfc`: the leaf-path set is **identical at 247 paths**, the environment block is byte-identical,
and `.build` gains exactly `field_set` (plus the `-dirty` suffix an uncommitted tree stamps). The
values that differ are P2's and P9's poll figures, P13's `close_microseconds`, P6's `elapsed_ms` and
P10's depths — and two sequential runs of the *unchanged* binary move a superset of exactly those,
so they are the run-to-run set `docs/doctor/README.md` already names, not an effect of this change.
The box was not quiet (load 9.87 at build time, 2.83 later, 8 cores, sibling agents building), which
is why nothing here rests on a timing figure (§8).

**Guards, and the fail-first mutation each one answers** (§9 — every one executed against the fixed
tree, message captured verbatim):

* `report::tests::the_field_set_moves_on_a_new_observation_key_and_not_on_a_measurement` — the
  discrimination, in six parts. **M1**, the mutation that matters most: make `field_paths` treat any
  object as a leaf (a top-level-only digest). → *"P10 gained a nested `recheck` block and the field
  set did not move — this is the 2026-08-05 defect verbatim"*, `left: "a528bc6947728b70"` equal to
  `right`. A top-level-only digest passes every other assertion in the test and still misses P10's
  `recheck`, P13's `slave_termios_mode` and P7's baseline block, because all three arrived *under*
  existing keys. **M2**, push `"{id}.{key}={value}"` beside each path. → *"a measurement moved the
  field set — every healthy pair of boxes would now report itself incomparable"*. **M3**, annotate
  each scalar with its JSON kind. → *"null-vs-number moved the field set — a kind-sensitive digest
  calls two healthy runs of one binary incomparable"*. M1 and M2 together are the discrimination the
  field exists for; a change that passes only one of them is not the fix.
* `report::tests::the_shared_encoder_is_byte_stable` — **M4**, delete the length prefix in
  `fnv1a_delimited`. → *"probe_set digest changed — every value in docs/doctor/ and its README index
  is now wrong"*, `left: "8ed97919f50e8cd0"` against `right: "ea5afd6873507ab9"`. The shared encoder
  was factored out of `probe_set_fingerprint` so both digests use it; the byte stream is unchanged by
  construction (flattening `(id, question)` pairs), and this pin is what proves it.
* `report::tests::the_two_field_set_paths_agree_on_a_real_report` — **M6**, emit a constant instead of
  the digest. → *"the emitted digest and the recomputed digest disagree — the README index of
  docs/doctor/ is computed by the recompute path"*. It also fires under **M5** (below).
* `meta_doctor_artifacts::equal_probe_sets_do_not_mean_equal_field_sets_and_the_tree_proves_it` —
  **M5**, walk questions instead of observations in the recompute path. → *"7ead470 and 1a9a8fc carry
  equal probe_set AND equal field_set — the committed counterexample stopped being visible"*. This is
  the guard that catches a future "simplification" of the digest back onto the questions.
* `meta_doctor_artifacts::a_fresh_report_carries_the_digest_its_own_recompute_produces` — fires under
  M5 and M6, against live probe output rather than a fixture.
* `meta_doctor_artifacts::the_field_set_does_not_move_between_sequential_runs_of_one_binary` — the
  other half: five triples of sequential runs, each triple one digest.
* `expectation_gates::the_field_set_clause_rejects_a_report_that_cannot_say_which_cells_it_carries`
  and `…::both_expectation_files_carry_the_same_field_set_clause` — deleting the clause from
  `expectations/linux.jq` fails both, the first with *"admitted a report with no field set"*. The
  clause pins shape, never value: the value is a property of the run, so pinning it would redden a
  healthy box.

**A refuted prediction, recorded because §9 says it is as load-bearing as a confirmed one.** The
design predicted that M4 (encoder drift) would also fail the artifact gate on
`assert_eq!(probe_set(&old), "a131e1f4b46d6c83")`. It does **not**, measured: that helper reads a
*stored* field out of a frozen JSON file, so no change to the encoder can move it. The gate was
therefore given the assertion that can see it — a pinned `field_set` of one frozen artifact
(`0c303d4cb11e3893`), which is recomputed on every run and is exactly the value the README publishes
in a column no reader can otherwise check. With that in place M4 fails the artifact gate too, with
*"the recomputed field set of a frozen artifact moved — the `field set` column of
docs/doctor/README.md now publishes wrong digests"*.

**Vacuity check** (§9 — a verdict from a loop that never executed is vacuous).
`the_field_set_does_not_move_between_sequential_runs_of_one_binary` iterates hard-named artifact
triples and would pass trivially if a file were renamed and the loop silently skipped it;
`the_artifact_directory_still_holds_what_this_gate_indexes` is the floor that keeps it honest, and
the floor was proved to fire by raising it to 99 — *"only 19 JSON artifacts found in docs/doctor
(floor 99)"*, which names the count it counted. `field_set()` panics rather than returning an empty
string when `--field-set` exits non-zero, so two failed recomputations cannot compare equal.

**Two deviations from the design as drafted, both recorded rather than silent.** (1) The design's
boundary case in assertion (6) compared `probe(…)`-built probes, which carry no observations at all
and so have an empty field set; it is written here as a genuine boundary pair —
`["P1.aP2.b"]` against `["P1.a", "P2.b"]`, which concatenate to the same bytes and are separated only
by the length prefix. (2) The `--field-set` reader in the design carried a malformed push expression;
it is written plainly.

### 3.52 Two instruments repaired: a discriminator with one bit of resolution, and an axis that could not vary

**Design:** §7 — an instrument that cannot separate two hypotheses must say so, not pick one.

Both defects were introduced by §3.44's own experiments, one day old, and both were found by the
Darwin capture they were built for. That is the experiments working: a pre-registered prediction
came back confirmed *and* the confirmation turned out not to carry the inference attached to it.

**A. P10's recheck was sampled at exactly one drain size.** `P10_RECHECK_DRAIN` was a hardcoded 512
against a Darwin capacity of 1024 targetward / 1022 hostward, so D = C/2 exactly. Any watermark model
— *writable iff occupancy < T, then accept up to C* — predicts `topped_up == 512` for **every** T >
512, so the data excluded only T ≤ 512. Worse for the framing, a reservation charged only at the
empty→nonempty transition is **invisible by construction**, because the top-up always starts from
occupancy C−512 and never from empty: the pre-registered falsifier "a negative delta means a
reservation" could not fire on that shape at all. The Darwin answer (`delta: 0`,
`refill_reproduced_total: true`, 6 of 6) was therefore consistent with a capacity *and* with a whole
family of watermarks, and §3.45 A said so rather than claiming the capacity.

The recheck now walks a **ladder** of drain sizes — `[512, 1, 128, 900]`, with 512 first so
`drained_again_bytes` and `topped_up_bytes` keep the meaning the committed artifacts recorded — plus
a from-empty rung. A rung that *refuses* to top up bounds T from above; a rung that tops up bounds it
from below; together they bracket a watermark instead of being consistent with all of them.

**The reading is reported with its own limits, which is the point.** On Linux, 5 runs × 2 directions:
`rungs_topping_up: 4`, `rungs_refusing: 0`, `watermark_threshold_le: null`. The emitted prose says
what that does and does not mean — no rung refused, so **no hysteresis was seen at any drain size
probed**, which is bounded by the smallest rung and *not* proof of a pure capacity; and where no rung
refuses, the `_gt` bound is merely the largest occupancy the ladder happened to reach, **not an
occupancy the kernel was observed to accept a write at**, because on a pipeline kernel it moved under
the top-up. Nothing is inferred from a rung that freed nothing.

**Linux is unmoved where it matters:** `room_republished_minus_room_freed` still reads bimodal
+2048 / +9216 and never 0 across 5 runs, and `refill_reproduced_total` is still false most runs.

**B. P9's second axis could not vary.** POSIX delivers `POLLHUP` in `revents` whether or not it was
requested, so at a fixed fd state every mask cell observes one kernel state: the cells are
**replicates, not levels**. The 2×2 was a 1×2 wearing a 2×2's name, and the "mask spread" it reported
(0.968–1.314x, sign-flipping between runs) was noise presented as a measurement.

The observation is renamed `zero_timeout_by_fd_state`, carries `shape: "1x2"` and
`isolated_variable: "ready-vs-not-ready"`, and **keeps the mask cells** — "the mask does not matter"
is a result worth publishing. Better, it now *measures* that claim instead of citing the standard:
an empty-mask cell records `hangup_delivered_to_a_mask_that_requested_nothing`, which reads **true**
on Linux, and it runs last in each group so it doubles as a within-group warmup control.

**And the residual P2/P9 headline gap is named as instrument, not kernel.**
`headline_offset_is` states it in the report: `median_ns_for_0ms_request` is n=16 taken cold, while
`unready_master_pollin_ns` is n=4096 on the same fd, same mask, same wrapper. A `p2_instrument_*`
cell reproduces P2's shape verbatim inside P9, so the two numbers can be compared without a reader
having to know they were never the same measurement.

**Both additions move `field_set`, and that is correct** — the report carries more cells than it did,
which is exactly what §15.44's second digest exists to announce. `probe_set` does not move: the
questions are unchanged. A diff against the `-05b` Linux or `1a9a8fc` Darwin triples is still lawful
on every cell those reports carry; the new cells simply have no counterpart there.

**Pre-registered for the next Mac run (§7):** if Darwin's bound really is `TTYHOG`/`t_outq` capacity
rather than a watermark, at least one ladder rung **refuses** and `watermark_threshold_le` comes back
non-null, bracketing T against the `_gt` bound. If every rung tops up there as it does on Linux, the
ladder has not separated the two on Darwin either, and the next step is a rung below 128.

---

## 4. Findings carried forward (from serial-nexus-doctor)

> **Era banner (2026-08-12, the v17 landing):** frozen at its writing (pre-P14/P15). At least
> three bullets below are stale against later committed evidence — the no-udev arm is
> exercised (plan §18 item 9, executed 2026-08-05), P11 has same-fingerprint 7.0 counterparts
> (the `f8315cc` triples), and the Linux P13 claim is artifact-backed
> (`docs/doctor/linux-7.0-2026-08-05-tier3.json`). Kept as written per the append-only rule;
> read through this banner.

Full report: `docs/serial-nexus-doctor.md`. Re-runnable per system with
`cargo run -p serial-nexus-doctor` (Markdown) / `--json | jq -e -f expectations/linux.jq`.

- **P1 EXTPROC/TIOCPKT — supported (7.0 & 6.18).** Packet-mode observation is the
  primary path; the §7.2 reconciliation poll remains an unconditional backstop
  (kept live regardless — do not delete it because a probe passed).
- **P2 PTY presence — supported.** Drives the slave-priming refinement (§3.2).
- **P3 serial fit — supported on real FTDI.** Custom baud (exact), `TIOCEXCL`,
  modem lines, break, `TIOCGICOUNT` all confirmed. Drives §3.1.
- **P4 device identity resolution — supported.** Canonical
  `usb:vid:pid:serial:iface` via a dependency-free sysfs *ancestor* walk (nearest
  `bInterfaceNumber` = interface; first `idVendor` = device — stop there or you
  bind the root hub). This is the reusable core of the phase-7 resolver. Since
  review 32 (`RES-2`) the probe asks about **devices**, reading the
  `<sys>/class/tty` listing with `/dev/serial/by-id` as a fast path over it, so it
  stays `supported` in a no-udev environment instead of skipping in the one place
  §12's fallback exists for. **The rewrite ran on 6.18 on 2026-07-29** (the
  2026-07-27 run predated it), reporting `by_id_tree: present, count: 2,
  sysfs_only: 0` for two adapters — which witnesses the sysfs *ancestor walk*
  there (both identities come out of `sysfs_lookup`) but **not** the `<sys>/class/tty`
  listing, whose contribution `enumerate_ports` merges with `or_insert` and which
  would print identically had it returned nothing. The no-udev arm is still
  unexercised on either kernel; `--dev-root` fires it without hardware.
- **P5 rig discovery/certification — supported on both kernels, and since
  2026-07-29 at the same tier.** Both boxes have now certified a cross-wired
  *pair* (rate ladder in both directions, deliberate baud mismatch); the 6.18
  Tier-1 run of 2026-07-27 certified per-port only, and "supported" meant strictly
  less there. What *no* tier certifies is break **reception** — the per-port
  `break` item is local ioctl acceptance and no probe transmits a break into an
  open peer — so `brk = 0` everywhere is structural.
- **P6 post-hangup pty readiness, P7 collapsed-session evidence — supported, and
  byte-identical on 7.0 and 6.18.** P6's `handler_reset_readable_bytes: 1` on both
  is what makes `pty.rs`'s last-close drain load-bearing on the production kernel;
  P7's two `latch_covers_*` booleans are true on both, retiring the probe's own
  named risk. **Neither licenses a simplification** — see the 2026-07-28 session
  entry at the top of this file.
- **P8 epoll vs `read(2)` — supported, identical on both.** `busy_loop_reproduced:
  false` on each; scoped to a layer below invariant 1's starvation (tokio's
  readiness guard, not `epoll_ctl`), so it refutes nothing.
- **P9 poll timeout granularity, P10 pty buffer depth — supported, numbers within
  their declared noise on both.** No backoff step and no `hostward_buffer` default
  moves. P10's apparent cross-kernel difference **dissolved on 2026-07-29**: the
  two kernels swapped flip-scheduling shapes, and three sequential 7.0 runs on an
  idle box produced all three first-pass values. Read a P10 delta as a scheduling
  artifact until several runs a side say otherwise.
  <!-- ANNOTATION 2026-08-05 (§5). SCOPED, and read the scope before applying the rule above.
       "Read a P10 delta as a scheduling artifact" was written about a Linux-vs-Linux pair and
       holds there. It is the WRONG first instinct across an OS boundary, where a second and
       larger cause outranks scheduling: the line discipline the probe measured. Until
       `71fc5a815852` P10 did not re-assert the baseline on the slave it measured, so off Linux
       — wherever the master is not a terminal, which P2 measures — it filled a COOKED pty. On
       7.0.0-29 that is worth an order of magnitude in what a reader can recover (raw ~13.8 KiB
       hostward, all of it recoverable; cooked ~23.5 KiB, none of it), which dwarfs the
       flip-scheduling spread this bullet is about. So the ordering is now: check
       `slave_termios_mode` agrees on both sides FIRST, then `bytes_recovered_by_peer`, and only
       then reach for scheduling. A delta between two runs whose modes differ is not a kernel
       delta at all. See §3.34. -->
- **P11 line-state counters — supported.** Both ioctls answer on every named port
  on both kernels; absolute counts differ by construction. Since 2026-07-29 that
  is two ports on 6.18 and **none on 7.0**, the crossover pair having moved to the
  production box — so P11, like P3/P4/P5, currently has no same-fingerprint 7.0
  counterpart to diff against.
- **P12 session-boundary edge — `skipped` on Linux, by design.** P7's sibling
  (§15.39): Darwin destroys the retained packet at last close so the *edge* is the
  only mechanism there, while Linux keeps the packet, which is P7's subject. A
  Linux `skipped` is the expected answer and is not a gap; `expectations/macos.jq`
  gates it tightly, `linux.jq` presence-only. **Scope note added 2026-08-04:** read
  "Darwin destroys the retained packet at last close" as the *hostward control
  packet* it is about — `ttywflush` flushes `FREAD` even on the drain-wait's success
  path. It is not a blanket statement about last close, and P13 refutes the blanket
  reading for the other direction: *targetward* bytes the slave wrote are preserved
  for up to ~601 ms while the kernel waits for the master to drain.
- **P13 last-close disposition of unread client bytes — measured on both platforms,
  and they disagree.** Linux 7.0.0-29 `retains`: 7 µs, 64/64 recovered. Darwin
  24.6.0 / macOS 15.7.8 `waits-then-discards`, `close_waits_for_reader: true`:
  600104 µs and 0 of 64 with no reader, 23 µs and 64 of 64 when the master drains
  first, 29 µs and 0 of 64 with `O_NONBLOCK` on the slave
  (`docs/doctor/macos-24.6.0-2026-08-05-tier3.json`; the Linux side is recorded in
  §3.30 and is **not** yet artifact-backed). Two consequences. It is what licenses
  the `#[cfg(target_os = "linux")]` gate on `p8_map`'s
  `a_closing_writers_residual_is_forwarded_not_purged` — the exclusion now has a
  measurement on *both* sides rather than a platform prejudice on one. And it prices
  §3.29's rule on Darwin: a post-close byte assertion against a blocking slave is not
  a coin flip but a deadline, with ~601 ms of slack against a 200 µs–5 ms reader
  cadence, so a red there means a reader stalled for the whole timeout — a
  daemon-side defect, not a lost race. The one shape with no slack at all is
  `O_NONBLOCK` on the slave (28 µs, unconditional), which is why §3.29's rule stays
  absolute rather than becoming "safe on Darwin".

---

## 5. How to build, test, run

```bash
# `cargo build` first is NOT optional: the serial-nexus-itest harness boots the plain
# target/debug/{serial-nexus-daemon,serial-nexus-sim,serial-nexus-web,serial-nexus-doctor} artifacts, which only
# `cargo build` emits — `cargo test` alone on a clean tree fails every itest.
cargo build --workspace --locked
# The one suite: unit + property tests AND the whole integration harness. There is no
# separate validation step any more — `scripts/` was deleted in §16.11 and every former
# phase script is now a `itest/tests/*.rs` (§5 of AGENTS.md).
cargo test --workspace --locked
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
cargo test -p serial-nexus-itest --test p4_steal_lease     # one former phase script
cargo test -p serial-nexus-itest --test p8_soak -- --ignored   # the endurance soak

# Capability report on this machine (attach to any bug report):
cargo run -p serial-nexus-doctor                      # Markdown
cargo run -p serial-nexus-doctor -- --port /dev/ttyUSB0   # include P3 on a real port

# Drive the daemon by hand (use a SHORT socket dir — see §7):
export XDG_RUNTIME_DIR=$(mktemp -d /tmp/snx.XXXXXX)
./target/debug/serial-nexus-daemon &                  # or --config demo.toml, --socket PATH
./target/debug/serial-nexus-ctl load demo.toml
./target/debug/serial-nexus-ctl --json state
./target/debug/serial-nexus-ctl dump
./target/debug/serial-nexus-ctl shutdown
```

A minimal `demo.toml` (serial→PTY fan of one):
```toml
[[node]]
type = "pty"
name = "console"
path = "/tmp/snx/console"
[[node]]
type = "serial"
name = "usb0"
device = "/dev/ttyUSB0"     # or a `serial-nexus-sim pty --echo --link` path
[[edge]]
a = "usb0"
b = "console"
```

---

## 6. Phase 2 slice 2 (the data plane) — DONE

Real bytes flow serial↔PTY through a configured daemon over RPC. As built:

- **Readiness (§3.10):** poll-based, *not* `AsyncFd`. Each boundary fd is set
  `O_NONBLOCK`; a task drains via `sys::poll_ready` + `sys::read_fd`/`write_fd`,
  sleeping `runtime::IDLE_POLL` (5 ms) only when idle.
- **Hostward (serial→PTY):** serial read → `try_send` into each attached PTY's
  bounded channel (drop-on-full = lossy at the boundary, §5); the PTY writer
  drains to the master, **presence-gated** (discard when no client).
- **Targetward (PTY→serial):** PTY read → `send().await` into the serial's bounded
  channel (lossless + backpressure: a full channel pauses the reader; the kernel
  buffers on the client side, §5).
- **Packet mode:** the leading `TIOCPKT` byte is stripped on every master read;
  only `TIOCPKT_DATA` (`sys::TIOCPKT_DATA`) payloads are forwarded. `TIOCPKT_IOCTL`
  (client-termios reconciliation into state) is still `#[allow(dead_code)]` — a
  later phase surfaces client termios; the data plane just drops control packets.
- **Presence:** per-PTY `client_present: Rc<Cell<bool>>` driven by the same 5 ms
  `POLLHUP` poll; reads are gated on presence, and on last close the baseline
  termios is re-asserted (§7.2). Feeds `PtyNode::state_extra`.
- **Wiring:** `runtime::Wiring::build` derives the channels from the validated
  edges; `daemon::load` starts each node via `Node::start` (`spawn_local` on the
  `LocalSet`). Teardown/Drop abort the tasks and close the fds.
- **`serial-nexus-sim`:** `client --report-termios` opens the daemon's PTY and reports
  its termios *without* disturbing it (verifies the §7.2 baseline end to end).
- **Validated by `scripts/validate/phase2/data-path.sh`:** 64 KiB seeded echo
  round-trip (checksums intact), both nodes `active`, baseline termios from the
  client's side (raw/echo-off/EXTPROC), and `client_present` true↔false
  transitions. Measured ad-hoc: ~1% idle CPU, 1 MiB echo in ~0.5 s.

## 6a. Phase 3 (boundaries and logging) — DONE

Built in four committed slices: **A** boundary drop counters + serial discard +
`TIOCGICOUNT` (`e064025`); **B** the log node — bounded queue, dedicated blocking
writer thread, on-demand `rotate`, counter recovery by directory scan, ENOSPC
fault; removed the §3.9 log-load rejection (`04b394d`); **C** `subscribe`
(broadcast + periodic snapshot) and client-termios surfacing via the
`TIOCPKT_IOCTL` path (`86ff94c`); **D** the high-throughput data plane (§3.11:
serial reader + PTY writer on dedicated blocking threads) with the firehose,
exact-loss, and throughput/idle benchmark (`c4d0e64`). All validated by
self-judging `scripts/validate/phase3/*.sh`; `docs/benchmarks/phase3.json` records
the throughput (~185 MiB/s) and idle (2% for 32 fds) axes.

## 6b. Phase 4 (arbitration) — COMPLETE

Per plan §Phase 4, built in slices; test topology needs no codec (PTYs on one
serial endpoint is a legal §4 fan-out).

- **Slice A DONE: the exclusive write lock.** `serial_nexus_core::lock::EndpointLock` —
  the pure, property-tested state machine (holder, per-origin write modes, purge
  accounting; `may_write` is the gate). Serial node gains an `arbitration` config
  attribute (§6). `Wiring::build` creates one `Rc<RefCell<EndpointLock>>` per
  host-facing endpoint and registers every edge as an origin (a log/`never` edge is
  a non-writer). Each writing PTY's read task gates its targetward drain on
  `may_write` — a non-holder is **not read from** (backpressure, no drop, §5/§6).
  `lock`/`unlock` RPC (address the **origin**, §3.12) with `-32003` LOCKED for a
  contended acquire; the host endpoint reports `.lock` (arbitration, holder,
  origins, purge) in `state`. CLI `lock`/`unlock`. Validated by
  `phase4/exclusivity.sh` (byte-exact: only the holder's stream reaches the sink;
  a locked-out present writer and a `write=never` spy leak nothing — verified with
  a negative control that a disabled gate makes the test fail).
  - **Consequence — exclusive is the default (§6), so a lone PTY needs a lock to
    write.** `only the holder's bytes are read targetward` holds even with one
    origin: an on-demand PTY that has not acquired the lock is not read from. This
    (correctly) broke five pre-arbitration phase-2/3 tests that wrote targetward
    (`--expect echo`) or changed termios without locking. They now set
    `arbitration = "free-for-all"` on their serial node — §6's documented opt-out
    — to keep testing the data plane / logging / termios (their actual subject)
    without arbitration ceremony; the exclusive-lock path is covered by
    `phase4/exclusivity.sh`. Real single-console operators have the same choice:
    `free-for-all`, or the "grab, write, release" flow.
- **Slice B DONE: purge + detach-release + free-for-all e2e.** The PTY reader
  (`nodes/pty.rs::read_and_poll`) was restructured: it now drains available data
  for any `may_write` writer **regardless of a simultaneous `POLLHUP`** (so a
  closing writer's residual is forwarded, not lost), and the present→absent
  transition is handled once, post-drain, by `handle_last_close` — the holder
  releases (detach-release), an **exclusive non-holder's** buffered backlog is
  drained+counted (purge-on-detach), and a free-for-all writer keeps its bytes.
  Purge-on-acquire runs **synchronously in `daemon::lock`** at grant time
  (exclusive only), draining+counting the pre-grant backlog via `Node::purge_origin`
  *before the grant reply returns*; a held holder keeps the lock across a client
  detach. Purge counters surface in `state` as `.lock.origins[].purged`. Bugs caught
  & fixed during build-out and an adversarial multi-agent review (details in §3.12):
  a closing free-for-all writer's residual was purged not forwarded; a lingering
  `TIOCPKT_IOCTL` re-populated `client_termios` after last-close; the purge-on-acquire
  drain was initially in the async reader and raced a correct acquire-then-write
  client (moved to the daemon); a held holder was wrongly detach-released. Validated
  by `phase4/{purge,free-for-all,held}.sh` (purge counts exact, post-grant survives,
  two free-for-all writers both reach the device, held keeps its lock);
  `exclusivity.sh` now also asserts detach-release.

- **Slice C DONE: waiting verbs + the two-lane control plane (§6, §10, §15.20).**
  - **Pure lock (`serial_nexus_core::lock`):** `EndpointLock` gained a FIFO `waiters`
    queue, a grant `generation`, `steal`, and `renew`. `acquire` is now queue-aware
    — it grants a free lock **only to the FIFO head** (barge prevention), naming an
    earlier waiter in `Denied { held_by }`. New pure API: `enqueue`/`dequeue`/`steal`
    /`renew`/`generation`/`waiters`; `snapshot()` now carries `waiters` and
    `last_steal`. 14 unit/property tests (the invariant proptest gained enqueue/
    dequeue/steal ops, generation-monotonicity, and holder-never-queued).
  - **`LockCell` (`runtime.rs`):** `SharedLock` is now `Rc<LockCell>` =
    `RefCell<EndpointLock>` + a `tokio::sync::Notify` (wakes queued waiters) + the
    `subscribe` broadcast sender + a `closed` flag. `wake_waiters`/`notified`/
    `emit_change`/`close`/`is_closed`. `Wiring::build` takes the notifier and creates
    **one targetward channel per host endpoint up front** (`endpoint_targetward`), so
    `send` works even with no PTY writer attached.
  - **Two-lane dispatch (`daemon.rs`, §15.20):** `Daemon::dispatch` is now `async`;
    `lock`/`send` are async, `unlock` stays sync. `wait_for_grant` is the waiting
    lane — it enables the `Notify` future **before** the acquire check (lost-wakeup-
    free), enqueues on `Denied`, and suspends on `notified`/deadline holding no
    borrow. The **`RefCell` borrow never crosses an `.await`** tripwire holds
    throughout (every borrow is a `{}` block dropped before the await; purge drains
    the fd synchronously). `WaiterGuard`/`TransientOrigin` are `Drop` guards that
    dequeue/unregister on cancellation. Immediate id-less `lock` notifications fire
    on every transition (acquire/release/steal/lease-expiry/detach-release) via
    `LockCell::emit_change`; the 200 ms snapshot is only an observability floor.
  - **`send <endpoint> --line`:** names the **endpoint**; the CLI is a transient
    origin (synthetic id from `SEND_ORIGIN_BASE = 1<<40`). register → acquire-with-
    timeout (default 2000 ms, `--timeout-ms`) or `--steal` → write `line + "\n"`
    targetward → release + unregister. Always cleaned up (guard) on timeout or a
    dropped connection.
  - **`control.rs` cancel-on-disconnect:** `serve_connection` races the (maybe-
    waiting) dispatch future against a second `lines.next_line()` in a
    `tokio::select! { biased; … }` — `biased` so a ready fast verb is never pre-
    empted by a spuriously-read next request, and a dropped connection cancels a
    `--wait` (dropping the dispatch future runs the guard).
  - **`nodes/pty.rs`:** `handle_last_close` now `wake_waiters()` + `emit_change()`
    after a detach-release / purge-on-detach, so a queued `--wait` waiter is granted
    on the detach-release path.
  - **Three bugs caught by the slice-C adversarial review & fixed** (regression-
    covered): (1) **lease re-arm** — a second `lock --lease-ms` hit `AlreadyHeld`
    without advancing the generation, so the *earlier* (shorter) timer still fired
    and released the grant; now `renew` bumps the generation on re-arm, invalidating
    the prior timer (`steal-lease.sh` check 4). (2) **teardown stranded parked
    waiters** — a deadline-less `lock --wait` hung forever when its endpoint was
    torn down; `teardown` now `close()`s every lock cell (which wakes waiters) and
    `wait_for_grant` returns a defined `Closed` error (`waiting.sh` sub-check D).
    (3) **steal didn't wake a same-origin `--wait`** — a `lock X --steal` from one
    connection left a `lock X --wait` on another parked; both steal paths now
    `wake_waiters()`.
  - **Validated:** `phase4/{send,steal-lease,waiting}.sh` (plan items 5, 4, 7):
    send LOCKED-then-steal byte-exact; steal record + immediate notification; lease
    auto-release, stale-timer-never-fires, and renewal-extends; FIFO across an unlock
    **and** a detach-release with byte-exact purge-on-acquire on the queued grant,
    kill-waiter-dequeues, deadline-send-queue-intact, teardown-wakes-waiter.

## 6c. Phase 5 (codecs) — COMPLETE

The interior codec node — the first node with more than one endpoint and the first
non-two-layer topology. Built in three slices, then an adversarial audit fixed 14
findings.

- **Slice A — pure contracts + reference codec + sim modes.** `serial-nexus-core` gained the
  `NodeConfig::Codec` variant (codec name, `faces`, channel list, opaque `attributes`
  table; multiplexed side = the default/empty endpoint, channels = identities) and the
  shape/validation; `Eq` was dropped from `GraphConfig`/`NodeConfig` (a `toml::Table`
  carries floats — only `PartialEq`; nothing needed `Eq`). New crate
  **`codecs/reference`** (`serial-nexus-codec-reference`): the v1 envelope framing as a `Codec`,
  with **length-guided resync** — on a body-decode error with an intact length prefix,
  skip exactly `4 + body_len` and count one framing error; only a mangled length prefix
  is unrecoverable, and the reliable-transport link codec (phase 6) never hits it, so
  §8's one shared frame format holds. `serial-nexus-sim` grew **`mux`** (round-robin
  seeded per-channel data → reference frames, `--corrupt-every`, a deterministic
  `--manifest` oracle, and a `--wait-file` feed gate so presence-gated channel PTYs
  don't miss the burst) and **`envelope`** (drives an external codec child through the
  golden-vector battery). Fixture: `tests/ext-codec/passthrough.py`. Validated by
  `phase5/envelope.sh` (item 3). The graph validator gained `DuplicateEndpoint`
  (empty/duplicate channel identity) — a slice-A adversarial review found the codec was
  the first node that could hit it.
- **Slice B — endpoint-keyed wiring + codec node (demux/resync/held).** `Wiring` was
  generalized from node-keyed (serial→consumer) to **endpoint-keyed** (`EndpointAddr`):
  every host-facing endpoint gets a lock + fan-out + one arbitrated targetward channel;
  every target-facing endpoint is a single-producer consumer that may write back. Only
  the `Node::start` dispatcher and `Wiring` changed — serial/pty/log `start` signatures
  are untouched. The daemon converts the endpoint-keyed maps to display-string keys for
  the RPC surface (`usb0`, `mux/console`) and reports each host endpoint's lock as
  `.lock` (serial) or `.channels[ch].lock` (codec). `nodes/codec.rs`: a hostward demux
  task (raw → per-channel `data` → fan-out) and one targetward mux task per channel
  (frame → serial, gated on the codec holding the serial lock). The demux edge holds
  the serial lock (`held`); a steal ousts it, and the channel task **reclaims with
  priority** once the stealer releases. Registry `build_codec` (match-on-name behind a
  `codec-reference` Cargo feature); a bad codec name / attribute schema is structural
  (aborts the load, nothing created). Validated by `phase5/{demux,resync,held,
  bad-attributes}.sh` (items 1, 2, 6, 5). **Remux (`faces = host`) is deferred to
  phase 6** — it needs a leg to drive; such a node loads and comes up faulted.
- **Slice C — exec codec.** `nodes/exec.rs`: a child process speaking the envelope on
  stdin/stdout, the multiplexed side on the **reserved empty channel** (ADR §15.22). A
  supervisor spawns the child, pumps both directions, and restarts with backoff on
  crash (restart count is observable); stderr → tracing. Validated by
  `phase5/exec-crash.sh` (item 4): a 256 KiB echo round-trip through the codec, `kill
  -9`, restart, clean resume, with an unrelated serial echo healthy throughout.
- **⚠️ Audit fixes (14 confirmed; do NOT regress).** (1) **CRITICAL exec-pump
  deadlock** — the single `select!` coupled stdin-write and stdout-read; under
  sustained flow (>64 KB) the child filled stdout and blocked on stdin while the daemon
  blocked writing stdin. **Fixed:** `pump_child` runs stdin-feeding and stdout-reading
  (and stderr) as **concurrently-polled** futures in one `select!`, so a blocked
  `write_all` never starves the stdout reader. The 256 KiB round-trip in `exec-crash.sh`
  is the regression guard — do NOT collapse the two directions back into one branch.
  (2) **Held re-acquire was FIFO** — a non-held `--wait` waiter could inherit the mux
  lock and corrupt framing. **Fixed:** `EndpointLock::reclaim_held` grants a held origin
  the free lock ahead of on-demand waiters (§6 "held indefinitely"); `ensure_holds`
  uses it. (3) **Duplicate node names** silently collapsed in the shape map →
  `ValidationError::DuplicateNodeName` + `GraphConfig::validate()` (checks the node
  *list* before the model's HashMap collapses it; `load` calls it). (4) Mux-side
  hostward drop counter now surfaced as `.multiplexed.dropped_slow_consumer` (§5 loss
  attribution). (5) A configured-but-unattached channel discards-with-count
  (`discarded_unattached`) instead of over-counting `delivered_hostward`. Plus the
  exec teardown-vs-crash discriminator is now an explicit `PumpEnd` outcome (not a
  `src_rx.is_closed()` heuristic), the stderr reader is a pump future (no leaked task),
  and doc corrections (§3 default endpoint, §15.22/§15.23, `daemon.rs`/`codec.rs`
  docstrings). Two audit findings were **rejected** on verification (an oversize-mux
  drop that can't be constructed since `MAX_FRAME_SIZE == READ_BUF`, and a
  resync-as-link-codec worry that doesn't apply — the link codec never resyncs).
  **Note:** the phase-6 audit re-examined the first rejection and found the oversize
  drop *is* reachable for a non-codec-bounded producer (the leg's `send` verb, and
  the exec node's raw device stream) — see §6d; both are now fixed by fragmentation.

## 6d. Phase 6 (the wire / leg node) — COMPLETE

The cross-daemon transport (§7.4/§9/§15.24). Built as one coherent slice (config +
wire contracts, then the leg node, then the six validation scripts), then an
adversarial audit fixed 17 findings.

- **Wire contracts (`serial-nexus-codec-api`).** The v1 **hello** frame: `WIRE_MAGIC` (`0x534E584C`
  "SNXL"), `WIRE_VERSION = 1` (versioned independently of `ENVELOPE_VERSION`), a `u32`
  capability bitset (`CAP_LOCK_RELAY = 1<<0` reserved, negotiated none in v1),
  `Hello{version,capabilities,channels}`, `encode_hello`/`try_decode_hello`,
  `WireError`. A distinct wire construct (not a fifth `EventKind`), so the four golden
  vectors stay byte-frozen; it reuses the envelope's `u32` length prefix, and its body
  begins with the magic so it never collides with a data frame. `try_decode_hello`
  validates the version-stable magic+version prefix *before* the v1 12-byte header, so
  a version mismatch is always refused as such (audit fix).
- **Config (`serial-nexus-core`).** `NodeConfig::Leg` (+ `Transport`/`LegRole`); `shape()`
  emits one endpoint per channel, all facing `faces`, **no default endpoint** (the
  socket is off-graph); host-facing channels carry the leg's arbitration.
  `GraphConfig::validate` gained the loopback-only check (tcp non-loopback needs
  `insecure_bind`; unix exempt), empty-channel-identity and empty-channel-*list*
  rejection → `ValidationError::{NonLoopbackBind, EmptyLeg}` (+ the existing
  `DuplicateEndpoint` for empty identities). `is_loopback_addr` handles `host:port`,
  bracketed/ bare IPv6, `localhost`, and wildcard binds. The leg plugs into the
  §15.23 endpoint-keyed `Wiring` with **zero `Wiring::build` change** — via `shape()`.
- **The leg node (`nodes/leg.rs`).** A supervisor task (mirroring the exec supervisor)
  does connect-with-backoff / listen-accept-one, the hello handshake (both send then
  read, under one overall deadline), binding, and per-connection pump. The pump runs
  the socket **read and write halves concurrently** (the §15.22 lesson). `faces=target`
  (sender): drains the local hostward stream onto the wire and writes wire-arriving
  targetward as an **on-demand origin** (implicit acquire; release on idle *or*
  disconnect via a shared `Notify`; never `held`, exempt from purge-on-acquire).
  `faces=host` (receiver): fans wire data hostward (lossy `try_send`+counters) and
  drains the arbitrated targetward stream onto the wire. **The link codec fragments,
  never drops** an oversize chunk. Binding: `bound`/`waiting`/`unbound` are
  leg-internal state; a `waiting` channel's targetward writers backpressure (not sent
  to be dropped at the peer). Outage = faulted-and-wait: reconnect backoff, listen
  reject-extras, park the receivers, purge-on-reconnect (faces=host targetward
  backlog), and park the SEND half — not tear down — when local producers close.
- **`serial-nexus-sim`.** `wire` (hostile-or-conforming peer: crafted `--hello-version`,
  `--bad-magic`, `--oversize-frame`, `--unknown-type`, `--echo`, `--send`, `--stall`)
  and `tcp-proxy` (`--drop-after`/`--restore-after` outage injection) modes; `pty
  --stall`.
- **Validated:** `phase6/{reference,binding,hostility,insecure-bind,outage,
  head-of-line}.sh` (plan items 1–6): two-daemon reference topology (per-channel
  bidirectional checksums), bound/waiting/unbound, the §9 clean-refusal battery +
  heal, the loopback gate + insecure marker, tcp-proxy outage + purge-on-reconnect,
  and the whole-connection head-of-line property (targetward freezes together,
  hostward advances).
- **⚠️ Audit fixes (17 confirmed; do NOT regress).** (1) **CRITICAL §5/§9
  targetward-no-drop violation** — the write half `continue`d on an oversize-frame
  encode error, silently dropping (uncounted) any chunk whose framed size exceeded
  `MAX_FRAME_SIZE`; reachable via the uncapped `send` verb and codec-emitted chunks
  (`READ_BUF == MAX_FRAME_SIZE`). **Fixed** by fragmenting oversize chunks across
  consecutive `data` frames in `leg.rs` (and the same idiom in `exec.rs`'s stdin feed
  for the raw device stream); verified with a 100 001-byte `send` round-trip
  (byte-exact, `discarded_hostward == 0`). Do NOT reinstate the `continue`-on-encode-
  error drop. (2) **Stale-status wedge** — a `faces=target` leg whose local producers
  all closed returned `SourceClosed` and left status `Active` forever, killing the
  independent targetward direction; **fixed** by parking the write half (removed
  `PumpEnd::SourceClosed`) so the wire/read half stay live. (3) On-demand lock
  **released on peer disconnect** now, not only after idle (a `Notify` the supervisor
  pulses). (4) Handshake bounded by **one overall deadline** (a trickling peer no
  longer wedges a listen leg). (5) `waiting`-channel targetward is **gated (not
  muxed-then-dropped-at-peer)** — `next_send` skips unbound channels so their writers
  backpressure. Plus: `insecure_bind` surfaced in `state`; configured-but-unattached
  channel drops counted (`discarded_hostward`); empty-channel-list rejected; the
  hello magic/version-first decode order; and test-fidelity fixes (head-of-line
  positive lower bound + honest comment; sim wire hello honors `--timeout-ms`). No
  findings were rejected.

## 6e. Phase 7 (identity & resilience) — COMPLETE

Built in seven slices (§12/§7.1/§11/§10 + doctor P5), then an adversarial audit
fixed 5 findings. New ADR **§15.25**; §11/§14 touched (state-file path policy,
deferred `connect`/`disconnect`/`set-attribute`).

- **The resolver (`core/src/resolver.rs`, §12).** A dependency-free (no
  libudev) module lifting the doctor's P4 sysfs walk into shared code — the doctor
  P4 probe now consumes it (`Resolver::with_roots(...).discover_adapters()`). Rooted
  by a `dev_root` whose `sys_root = dev_root/sys`, so a single `--dev-root` selects a
  self-contained fixture (`/` → `/sys` in production). Two directions:
  `resolve_input` (add-time: raw path / bare serial capture requires presence;
  `usb:`/`by-path:`/`raw:` identities never do) and `resolve_current_path`
  (open/recheck; a `usb:` identity resolves only to a device whose sysfs identity
  matches exactly → **squatter refusal by construction**). Fallback chain
  usb→by-path→raw with instability warnings; **absent OR duplicated non-empty
  serials degrade to by-path** (the §15.10 wrong-device guard, made concrete).
- **Serial faulted-and-wait + reopen ritual (`nodes/serial.rs`, §7.1).** Rewritten
  around `SerialShared{status,port}` (`Rc<RefCell>`, read by `&self`) + a `ReaderSlot`.
  **One async supervisor per node** drives the targetward writer AND the reconnect
  poll; the dedicated blocking-thread reader (§15.19) pulses a `Notify` on device
  loss (POLLHUP/EOF/error), the supervisor joins it, transitions to `waiting`, and
  polls `resolve_current_path` (~1 s) for the **same identity**. On reappearance the
  reopen ritual reapplies termios, retakes `TIOCEXCL`, restores modem lines, sets
  non-blocking, and re-arms; **purge-on-reconnect** drains the parked targetward
  channel with a counter (the one sanctioned targetward drop; origin buffers stay
  the lock-purge's job, §6). fd-reuse-safe (reader joined before the port drops);
  `WriterClosed` keeps hostward alive when targetward senders drop (§15.24 lesson).
  New serial config field `purge_on_reconnect` (default on). **Test-fidelity:** a
  finite `serial-nexus-sim pty --source` now CORRECTLY faults-and-waits when it closes —
  `pty --hold-ms` was wired to keep the device "plugged in"; `subscribe.sh` uses it;
  `log.sh` Check 3 now relies on **auto-recovery** (below) instead of a manual reload.
- **State file (`daemon.rs`/`main.rs`, §11/§15.9).** `Daemon::snapshot_config` writes
  config (TOML, atomic tmp+rename) after every config-mutating verb (dispatch-gated by
  `is_config_mutation`, NOT on read/arbitration traffic). Startup **prefers the state
  file** over `--config`. Default path is **socket-adjacent** (`<socket-stem>.state.toml` —
  the socket's *stem*, so `/run/serial-nexus-daemon.sock` yields `/run/serial-nexus-daemon.state.toml`,
  never `serial-nexus-daemon.sock.state.toml`; review 32 `RV-6`)
  — session-durable + restart-recovering, and per-daemon-unique so it never leaks
  across test daemons or into `$HOME`; `--state-file` gives reboot durability. Clean
  shutdown (`teardown_all`) does NOT persist an empty graph (preserves it for restart);
  the `teardown` VERB does. Write failure is logged, never corrupts the running graph.
- **Incremental verbs (`daemon.rs` + CLI).** `add-node` (resolver echo-back
  `{identity,description,kind,warning}`; path/serial absent → `DEVICE_ABSENT`; identity
  absent → waiting; wires an edgeless node via a partial `Wiring::build`),
  `remove-node [--cascade]` (refuses attached edges without cascade → `HAS_EDGES`;
  cascade flushes the log, closes+wakes the removed node's endpoint locks, prunes all
  maps, **unregisters a removed writer's origin from the surviving host lock** — audit
  fix), `load --replace` (validates BEFORE teardown so a bad config never destroys a
  good graph). New codes `HAS_EDGES=-32004`, `DEVICE_ABSENT=-32005`. **Deferred**
  (§14, §15.25): `connect`/`disconnect`/`set-attribute` (live-graph surgery; not in
  the Phase 7 Implements line, not validated).
- **Serial-signal verbs (`nodes/serial.rs`/`daemon.rs`/`sys.rs`/CLI, §7.1).**
  `send-break`, `set-modem`, `pulse-dtr` on the retained `Rc<SerialPort>`; `send_break`/
  `pulse_dtr` are **cancel-safe** (a `RestoreGuard` deasserts even if the dispatch
  future is dropped on client disconnect), and `serial_port()` clones the Rc and drops
  the borrow before the awaited sleep (RefCell-never-across-await). `set-modem` is
  ephemeral (does not rewrite config, §15.8). Modem-line readings surface in state via
  a new `sys::read_modem_bits` (TIOCMGET). **No-target doctrine:** a pts genuinely
  lacks modem lines, so `set-modem`/`pulse-dtr` return `ENOTTY` there (the exact
  Tier-3 boundary — the verb reached the live port); `send-break` latches on a pts;
  true master-side DTR/break observation is a Tier-3 hardware checklist item.
- **Doctor P5 + serial-nexus-sim nullmodem (§13/§15.21).** P5 (`probes.rs`) classifies each
  named port dangling/loopback/paired (both directions, so a half-crossed rig is named
  Degraded, never Unsupported) and certifies real UARTs, reporting `skipped (not a
  UART)` for the sim pts. Passive: `--port`-gated like P3. Discovery is a **poll-driven
  continuous scan** with periodic nonce re-sends + a 5 ms yield (a busy-spin on a
  perpetually-ready port would starve a software echo peer — a real bug found while
  hardening). `serial-nexus-sim nullmodem --link-a/--link-b` bridges two PTY pairs as a
  crossed pair. `expectations/linux.jq` gained a P5 `{supported,skipped}` clause.
  **Test note:** `phase7/p5.sh` runs the doctor twice (pair+dangling in one, loopback
  in its own) — a software `pty --echo` peer competing for CPU with other active peers
  in the SAME run is timing-sensitive on a loaded box (a sim/scheduling artifact, not a
  P5 logic issue: a real TX↔RX jumper reflects in hardware). Verified 8/8 under 4×CPU
  load after the split. **Real-hardware follow-up (2026-07-22, commit `8cf61d0`):** the
  paired independent-clock certificate (`p5_certify_pair` — the rate ladder + deliberate
  mismatch) had never run on real UARTs (the sim skips it); its first live run exposed a
  missing post-open baud settle. See the physical-validation block at the top.
- **Validation:** `scripts/validate/phase7/*.sh` (items 1–7) + a reusable
  `scripts/lib/fixture-tree.sh` that builds `/dev/serial/by-id` + `/dev/serial/by-path`
  + sysfs trees under `--dev-root` (the resolver seam, plan §3). `all.sh --through 7`
  = 39/39; 87 workspace unit/property tests.
- **⚠️ Adversarial audit found 5 confirmed (2 high, 1 medium, 2 low), ALL FIXED; do
  NOT regress:** (1) **[HIGH] duplicated non-empty serials** were captured as an
  ambiguous `usb:` identity (only the absent `-` half of §12 was implemented) →
  `usb_identity_ambiguous` degrades duplicates to by-path (test
  `duplicated_serial_degrades_to_by_path`). (2) **[HIGH] `remove-node --cascade` of a
  lock-HOLDING writer** left its origin registered/holding on the surviving host lock
  → a phantom holder wedged the endpoint forever; now `unregister` + wake/emit on
  release (regression in `signals.sh`). (3) **[MEDIUM] `--state-file` help** advertised
  a `/var/lib` default the code never uses → corrected to describe the socket-adjacent
  default + the reboot-durability caveat. (4) **[LOW] `find_usb`** aborted the whole
  by-id scan on one odd symlink (`file_name()?`) → skip the entry, continue. (5)
  **[LOW] empty `raw:`** input resolved to the dev-root dir → rejected as `Malformed`
  (test in `empty_input_is_malformed`). Two findings were REFUTED on verification (a
  `linux.jq` degraded-clause worry that misread intent; a reader POLLERR busy-spin
  unreachable for these fds).

## 6f. Phase 8 (hardening & release) — COMPLETE

The final phase (§13 macOS pass, packaging, docs, fuzzing, release validation).
Built as five slices, then an adversarial audit fixed 5 confirmed findings. No new
ADR (nothing contradicted the design); the additions are all §13/§Phase-8 plan work.

- **macOS portability (design §13, best-effort).** The workspace now COMPILES and
  degrades gracefully on `*-apple-darwin`, verified without a Mac by cross-checking
  `cargo check --target x86_64-apple-darwin --workspace` (which type-checks cfg
  resolution; it found the two blockers *and* one the up-front research missed). Two
  hard-compile blockers, both `#[cfg(target_os = ...)]`-gated: (1) **`libc::TIOCGICOUNT`**
  (Linux-only ioctl) in `daemon-bin/src/sys.rs` and `doctor/src/sys.rs` —
  gated with a `#[cfg(not(target_os="linux"))]` `read_icounts`/`read_icounter` stub
  returning `ENOTSUP`, which the callers already map to "omit driver counters, never
  fault" (the same path a pts takes on Linux); (2) **`nix::pty::ptsname_r`** (Linux/
  Android-only reentrant variant) in `pty.rs`, `probes.rs`, and `serial-nexus-sim` — a shared
  `sys::ptsname` wrapper (the daemon's + doctor's `sys` modules, a localized
  `#[allow(unsafe_code)]` fn in the deny-unsafe sim) uses `ptsname_r` on Linux and the
  static-buffer `unsafe ptsname` elsewhere. Plus the high-baud `BaudRate::{B460800,
  B921600}` match arms in `pty.rs` (absent on macOS termios) and `nix::unistd::getgroups`
  in the doctor's group-membership check (unavailable in nix on Apple) — both gated.
  Everything else (TIOCPKT/TIOCEXCL/TIOCMGET/TIOCM_*/EXTPROC/the poll(2) data plane/
  the resolver's `std::fs` backends) is portable; on macOS the by-id/sysfs resolver is
  inert at runtime (`usb:`/`by-path:` identities stay `waiting`; `cu.*` raw paths are
  the §12/§13 interim; the IOKit backend is the deferred §14 home). `expectations/
  macos.jq` is a lenient structural gate; the macOS CI lane BUILDS + runs the portable
  tests (gating) and the doctor report + phase-2 e2e informationally. `docs/macos.md`
  records the deltas as verified/expected/unverified.
- **Docs.** `README.md` (elevator pitch + five-minute quickstart, the author ran it);
  `docs/security.md` (the §9 "serial consoles are root shells" posture verbatim + the
  socket-permissions authz model + loopback/`insecure_bind`/SSH); `docs/codec-authors.md`
  (the byte-exact envelope contract + golden vectors + the exec-codec walkthrough);
  `docs/rpc/` (7 man-style pages over the full §10 verb surface, error codes,
  notifications — the docs auditor caught that the daemon defines a 5th app code
  `-32001` load-on-non-empty beyond the four in the research catalog).
- **Packaging.** `packaging/serial-nexus-daemon.service` (a hardened systemd unit —
  `DynamicUser`, `RuntimeDirectory`/`StateDirectory`/`LogsDirectory`, sandboxing with
  the deliberate device-access loosenings, validated by `systemd-analyze verify`),
  `serial-nexus-daemon.example.toml` (the §2 reference topology; load-verified), a udev rule,
  and `packaging/README.md`.
- **Fuzzing.** `fuzz/` — a cargo-fuzz crate (EXCLUDED from the workspace via root
  `[workspace] exclude`, needs nightly + libFuzzer) with four targets over the pure
  parsers: `envelope_decode` (`try_decode` + roundtrip), `frame_decoder`
  (`FrameDecoder` stream reassembly), `wire_hello` (`try_decode_hello` + stability),
  `reference_demux` (`ReferenceCodec::demux` resync termination + bounded buffer). The
  harness bodies were compile-verified on stable via a throwaway crate (only the
  libFuzzer glue needs nightly); a nightly CI job builds and runs each briefly.
- **Release validation.** `scripts/validate/phase8/{quickstart,agent-task,soak}.sh`.
  quickstart = the five-minute echo, wall-clocked under budget; agent-task = the full
  operator scenario via `serial-nexus-ctl --json` (inspect → lock + LOCKED negative
  control → send --steal → device-received via the echo→log oracle → rotate + byte-exact
  continuity → unlock), all deterministic with `printf|sha256sum` oracles; soak =
  parameterized (`SOAK_SECONDS`, default 8 smoke / nightly 1800+) asserting bounded
  VmRSS, an allowlist of loss counters staying zero, zero faults, and a final
  source↔log checksum reconciliation. CI: the deterministic phase-8 gates run per-push
  (the full `--through 8` sweep is not, to keep per-push CI lean — the heavy phase-3
  firehose/benchmark and multi-daemon topologies stay in the local suite), plus the
  macOS lane and nightly soak/fuzz jobs (`schedule` cron). *(The `phase5/demux.sh`
  flake that once justified capping the sweep is now fixed — see §7.)*
- **⚠️ Audit fixes (5 confirmed, ALL FIXED; do NOT regress).** (1) **[HIGH] packaged
  log node faulted out-of-the-box** — the unit granted `/var/log/serial-nexus-daemon` via
  `ReadWritePaths`, which flips the mount but does NOT chown, so the `DynamicUser`
  couldn't create files and the example config's `cap` log node faulted on `EACCES`
  every boot. **Fixed** with `LogsDirectory=serial-nexus-daemon` (systemd creates AND chowns
  it); removed the README's manual `install -d` step and documented the chown caveat
  for extra log dirs. (2) **[HIGH] `envelope_decode` fuzz target false-fired** — it
  asserted decode→encode byte-identity, but `try_decode` consumes `frame_end`
  (including trailing body bytes) for Open/Close while `encode` re-emits them empty, so
  a valid Open/Close frame with trailing bytes would report as a fuzz crash. **Fixed**
  by gating byte-identity to Data/Error and relying on decode→encode→decode STABILITY
  for Open/Close (the `wire_hello` pattern). (3) **[HIGH] `soak.sh` loss-counter check
  was a tautology** — `add // 0 == 0` parses as `add // (0==0)` = `add // true` (jq
  `//` binds looser than `==`), so a nonzero drop counter output a truthy number and
  the soak PASSED regardless. **Fixed** with `(add // 0) == 0`; verified it now fails
  on a 4096-byte drop and passes on zero/absent. (4) **[MEDIUM] `RuntimeDirectoryMode`
  shipped 0755** (world-traversable), undermining the design's 0700-parent
  post-bind-window guard (the daemon's own `main.rs` comment relies on it). **Fixed** to
  0700 (and `StateDirectoryMode` 0750→0700, added `PrivateTmp=yes`), aligning the unit
  UP to `security.md`'s tighter claims. (5) **[LOW] `security.md`↔unit drift** (device
  policy wording, a divergent inline unit copy missing the pty device rules). **Fixed**
  by rewording the device-policy prose and replacing the drift-prone inline unit with a
  pointer to the canonical `packaging/serial-nexus-daemon.service`. 0 findings refuted. All
  gates green after fixes: 42/42 `all.sh --through 8`, 87 tests, fmt/clippy/macOS-check
  clean.

---

## 7. Environment & operational notes

- **Unix socket path length:** paths are bounded by `SUN_LEN` (~108 bytes). The
  daemon errors clearly on overflow. Real deployments use `/run` or
  `$XDG_RUNTIME_DIR`; **test harnesses must use a short dir** (`mktemp -d
  /tmp/snx.XXXXXX`), not the long scratchpad path.
- **Serial device access:** the daemon runs as its own user and needs r/w on the
  device node. On the dev box `/dev/ttyUSB0` was `root:dialout 660`; a udev rule
  `SUBSYSTEM=="tty", SUBSYSTEMS=="usb", ATTRS{idVendor}=="0403", GROUP="plugdev",
  MODE="0660"` (or dialout membership) grants it. `serial-nexus-doctor`'s env checks
  report `group:*` membership and `access:<dev>`.
- **`Cargo.lock` is committed** (v3 plan §2): this is a binary workspace, and the
  cargo-deny gate is only as strong as the graph it inspects — an uncommitted lock
  would gate a freshly resolved, potentially different graph on every CI run. It was
  removed from `.gitignore` in the v3 realignment.
- **Licensing gate** (`deny.toml`) is proven in CI (rejects `serialport`); keep
  all new deps permissive (MIT/Apache/BSD/ISC/Zlib/Unicode), §13.
- **`serial-nexus-doctor` never gates the daemon:** runtime degradation paths (e.g.
  §7.2's poll) are unconditional, so a wrong probe misleads a developer but never
  the data plane. Keep it that way.
- **`phase5/demux.sh` presence-vs-readiness flake — FIXED (test-fidelity only; no
  daemon change).** The former ~1-in-5-under-load flake was a race in the *test*: the
  mux burst was released once every channel client reported `client_present==true`,
  but a slave can be open (present) a beat before its read loop is draining, so under
  load the burst outran the not-yet-reading consumer and the lossy presence-gated PTY
  shed the head, failing the byte-exact manifest check. The fix is entirely in the
  test double and the harness (plan §3's "presence is not readiness"):
  - **First-byte handshake (the prescribed fix).** `serial-nexus-sim mux` gained
    `--prime-file`/`--prime-bytes` and `client` gained `--skip`/`--ready-file`. Two
    phases: once the clients are present, the mux sends a small primer per channel
    (small enough that a present-but-not-yet-draining PTY buffers rather than drops
    it, so it reliably arrives); each client discards the primer and creates its
    ready-file *on the first byte it reads back* — proof it is draining, not merely
    present; only then does the harness release the payload burst, which can no longer
    outrun a parked reader.
  - **Isolate correctness from drop policy.** The channel PTYs set `hostward_buffer =
    512` so the whole burst is held (this test checks demux *correctness*, not the
    §5 drop policy — that is `exact-loss.sh`/`counters.sh`), and the client read
    buffer grew to 64 KiB so a fast, well-buffered stream drains in few syscalls.
  - **Right-sized for CPU starvation.** The burst dropped to 256 KiB/channel (256
    round-robin frames — full demux coverage) with a 90 s ceiling, so the
    single-threaded daemon completes it comfortably even when heavily CPU-starved,
    rather than the test being hostage to scheduling. Verified: **0 failures in 35
    runs under a fully CPU-saturated box (8 `yes` hogs on 8 cores) and under the
    fair ~4×CPU-load bar** — where the pre-fix test failed ~20-40%.

### 3.53 The Mac-developed doctor commits, measured on Linux with the rig

**Design:** §7 — no one-way decision on single-kernel evidence, and a kernel claim cites a
committed artifact. §9 — a guard asserts the property, never a proxy.

`df48bfc`, `6390940`, `50af61e`, `5c3e697`, `448f562`, `b21548d` and `f8315cc` were all
developed on the Mac. No Linux run existed for any of them: the newest committed Linux
artifact was `4b78fff`'s. This entry is that run, at `f8315cc`, on the dev box with the
FT232R crossover attached — artifacts `docs/doctor/linux-7.0-2026-08-05-f8315cc-tier3{,-2,-3}.json`
and `-passive-{1,2,3}.json`, probe set `a131e1f4b46d6c83`, field sets `3cb816e5b83dcf90`
(rig) and `60a346baeeb0b3d9` (passive).

**Every pre-registered Linux prediction held**, so the confirmatory half is stated once and
briefly: P10 `rungs_refusing: 0`, `watermark_threshold_le: null`, `writer_pending_input_bytes: 0`,
full recovery in both directions on a `raw` slave, `ceiling_hit: false`, the 512 rung first;
P9 `shape: "1x2"` with `hangup_delivered_to_a_mask_that_requested_nothing: true` measured rather
than cited; P12 `skipped` yet carrying ten observations, tight window **150 us** (recorded
134–175) and paced **328540 us** (recorded 325851–329361); P13 `retains` with 64/64 in all three
shapes and `baseline_packet_bytes: 1`; P5 byte-identical to `4b78fff`'s artifact and issuing the
Tier-3 certificate over the wire with no `unmeasurable_here` anywhere; P4 `supported` with
`canonical: 2`. The gate clause was proven to **reject**, not merely to pass: forcing
`canonical: 0` under `status: supported` makes both `linux.jq` and `macos.jq` exit 1 while the
unmodified report exits 0. `probe_set` did not move while **124 leaf paths did** (+116 / −8, P9's
2x2 collapsing to a 1x2) — the blindness `field_set` was added for, now demonstrated on Linux.

**The suite is not vacuous, and that was measured rather than assumed.** Run with
`SNX_CROSSOVER=required` and both ports named, it is 766 / 0 / 4 at `f8315cc` (767 with this
entry's own gate, below), and a second run under `--show-output` contains **zero** SKIP lines. The matcher was proved first: hiding `node` makes
`p8_web_history` report `ok` *and* print `SKIP … node not found`, which is review 32's TESTR-6
hazard reproduced on demand. So all 766 passes executed their property on this box.

**Four results are new rather than confirmatory.**

**(i) The rig is a five-wire crossover, and nothing in the tree knew it.** Driving RTS on either
port raises CTS on the other; DTR moves no DSR, DCD or RI in either direction (`TIOCMSET`/
`TIOCMGET`, both ends zeroed between trials). P5's `modem[...]` item lives in the single-adapter
block and the pair block covers only `rate_ladder` and `deliberate_mismatch`, so nothing certifies
the cross-pair handshake lines. Hardware flow control is therefore testable on this rig and
untested — a capability gap, not a defect.

**(ii) P10's top-up is drain-size independent on Linux, which refutes the ladder's own model.**
Across 8 runs x 2 directions x 4 rungs, `topped_up_bytes` is 2560 (11 of 16) or 9728 (5 of 16)
**regardless of whether 1 or 900 bytes were drained** — a 900x range in the input with no effect
on the output. A queue matching `writable iff occupancy < T, then accept up to capacity` would
re-admit what was freed; this one re-admits a fixed quantum. `room_republished_minus_room_freed`
is the same number minus the 512 rung, which is why it reads bimodal 2048/9216. The ladder's
summary fields do not surface this: `rungs_refusing`/`watermark_threshold_*` answer the watermark
question, and on Linux they cannot — all four rungs are ≤6.5% of the ~15360-byte capacity, so no
rung ever refuses and no bound is recoverable. **The discriminator that did fire is
`topped_up_minus_drained`.** §3.52 fixed the one-drain-size defect for Darwin; on Linux the ladder
is bounded by its largest rung instead, and that is now listed as open in AGENTS.md §2.

**(iii) `p4_free_for_all` passes 20 of 20 over this wire**, against the 12-of-12 failures §3.46
records on Darwin — same test, same rig hardware, same commit, box idle (load 0.26 → 0.09).
Per §9 **no mechanism is claimed**; what the repetition buys is that the failure is
kernel-specific rather than a test defect, which n=1 could not have said.

**(iv) `baseline_via_master` is `true` in 8 of 8 on Linux**, the exact inverse of the `false` in
12 of 12 that §3.40's falsifier produced on Darwin. §2 previously stated that `false` without a
kernel qualifier; it is Darwin's answer, not the general one.

**§3.43's "6 of 7" reproduces, and only because the rig was forced.** On Linux
`choose_pair_source` picks the software null modem whenever it exists, so the whole-suite run
never put those six callers on the wire — `p4_free_for_all` finished in 0.07 s. With
`SNX_SERIAL_PAIR=rig` all six pass (0.07 s → 2.91 s, and `p4_exclusivity` 5.75 s against the
5.76 s this file already records), and `p7_p5` is the deliberate seventh left on the software
provider. Wire use is *directly observed* for three of the six — the two large-volume binaries by
their 10.5x/41x wall-clock delta, and one `p8_map` caller by an `strace` showing `drain_stale`'s
read-only opens of both ports followed by the daemon's `O_RDWR` opens — and follows for the other
three from the single deterministic branch. The forced arm also has a working negative control:
`SNX_SERIAL_PAIR=rig` with the ports unset panics naming the candidates rather than silently
falling back, which is §3.35's rule holding in a second place.

**The passive/rig pair is a new gate, because it is the first one the directory could carry.**
§15.44 declined folding the observation keys into `probe_set` on the argument that it would make
a passive and a rig run of *one binary* report themselves incomparable. That was an argument:
`docs/doctor/` held no such pair, every capture in it being a rig run or from a different commit.
The `f8315cc` triples are that pair, and
`meta_doctor_artifacts::one_binary_run_passive_and_run_against_the_rig_shares_a_probe_set_but_not_a_field_set`
asserts both halves — equal `probe_set`, unequal `field_set` — so the declination now rests on a
committed measurement. It is the +1 in the suite count above.

**Cost.** Passive doctor **3.94 s** (5 runs, 3.93–3.95), Tier-3 rig **11.6 s** (3 runs,
11.55–11.58). §3.50's 3.74 s is superseded.

**One reading corrected in flight, recorded because §9 says refuted diagnoses are load-bearing.**
P9's `median_ns_for_0ms_request` read 1965 / 551 / 263 ns in the first rig triple and 166–285 ns
in eight later ones, which looked like an instrument regression and was then attributed to box
load. Both readings are wrong. The emitting code is byte-identical across `4b78fff..HEAD` (every
`f8315cc` P9 addition is sequenced after the sampling loop, and P10 after P9), the co-located
n=4096 control and the adjacent 1 ms row sit at baseline inside the same report, and the recorded
load *rose* while the number fell. It is an n=16 sample taken on the first sixteen `poll(2)` calls
of a fresh process — cold start, with in-tree precedent in the 6.18 artifact's 1323 ns. Nothing
was wrong; the field is simply not a figure to quote from a single early run.

### 3.54 The replug capability: a privileged helper the repository carries

**Design:** §15.45 (the whole entry). §9 — a guard asserts the property, never a
proxy for it, and the tell that you found the real property is that the portable
form comes out *stricter*.

**What was missing.** P4 prints "configs survive replug and cold start" on success
and §12 promises identity-to-path resolution "at every open and every
faulted-and-wait recheck". The only replug coverage in the tree, `p7_replug.rs`,
re-links a fixture sysfs tree under `--dev-root` and respawns a sim pty at a fixed
path — the daemon heals, but the kernel never enumerates, `ftdi_sio` never unbinds,
`/dev/serial/by-id` is never rebuilt and no `/dev` name ever changes. So the
shipped sentence had no measurement behind it on any kernel.

**What landed.** The replug helper's own crate — `devprep/` today, under a name this
entry predates (§3.81) — the one binary meant to carry a
Linux file capability, plus `sys/src/caps.rs` (capability reading from
`/proc/self/status`, `PR_SET_NO_NEW_PRIVS`, `PR_SET_PDEATHSIG`, the terminate
handler, and the `fstatfs` sysfs check) and `itest/tests/p7_replug_hardware.rs`.
The architecture argument — narrow helper, not a capability-conferring runner — is
§15.45's; the short form is that `CAP_DAC_OVERRIDE` is root-equivalent for files and
an ambient grant would reach every test target and every daemon, sim, node and
Chromium they spawn, which would make the suite prove the daemon works *as root*.

**Two departures from the pattern this was ported from, both deliberate.**

1. **No `.blessed` stamp.** The sibling keys a content-hash stamp on the built
   runner and re-blesses when it moves, with a documented defect (its M-BIN-2)
   where a stamp plus an existence check reported "already blessed" for a copy that
   had silently lost its caps. Here `inspect` compares the installed copy
   **byte-for-byte** against `target/<profile>/` and *then* re-reads mode and caps,
   so there is no third record that can disagree with the other two and that entire
   failure class does not exist. Verified in passing this session: after a rebuild
   the tool reported `Stale` without being told anything.
2. **The confinement is a kernel question, not a path question.** The sibling
   confines by `starts_with` under a `target/` derived from the runner's own
   location, and its design says three separate times that this is defense in depth
   rather than the boundary. Here the argument is not a path at all — it is a port
   *name* accepted against a digits/`-`/`.` alphabet — and `fstatfs` must answer
   `SYSFS_MAGIC` before any write. `sysfs_is_recognised_by_the_kernel_and_a_lookalike_path_is_not`
   asserts the half that matters: a directory shaped exactly like a USB device dir,
   on tmpfs, is refused.

**The `ep` substring trap, carried over as a unit test.** `getcap` prints
`<path> <caps>`, so a matcher that tests the whole line for `ep` is satisfied by a
path containing those letters — and `target/debug/deps/...` does. Such a matcher
reports a `+p`-only, un-raised binary as blessed, which is a skip that reads as a
pass. `getcap_field` returns the capability text only, and
`a_path_containing_the_flag_letters_does_not_read_as_blessed` plants the trap and
asserts the line *does* contain `ep` while the field does not grant it.

**The skip contract.** `SNX_REPLUG=required` turns the self-skip into a hard
failure, mirroring `SNX_CROSSOVER=required` rather than inventing a second
mechanism (§3.35). Both arms were run, not predicted: unblessed the three tests
self-skip naming the exact `sudo setcap` line, and under `=required` two of the
three fail with the same message. The capability is **proven, never assumed** —
`blessed_replug_helper()` asks the binary to report the effective bit from its own
`/proc/self/status`, because the kernel strips `security.capability` on every
rewrite and a copy left from before a rebuild is present and powerless.

**The rig variable is a by-id path, and that is not a style preference.**
Re-enumeration can hand the adapter a different `ttyUSBn` — the premise §1 is built
on — so `/dev/ttyUSB0` names something this very test can invalidate.
`SNX_REPLUG_DEV` accepts only `/dev/serial/by-id/...` and hard-fails otherwise. The
by-id → sysfs-port translation happens in the test, unprivileged, which is also what
keeps `/dev` names out of the capability-carrying binary entirely.

**The discriminator is `devnum`.** A driver unbind/bind leaves it alone; a real
disconnect and re-enumeration always changes it. It is compared for **inequality
only, never ordering** — it wraps at 127 per bus. `--dry-run` runs every check and
every wait and performs neither write, and
`the_replug_discriminator_goes_quiet_when_no_write_happens` executes that control in
the suite: if `devnum` moves under a dry run the helper wrote when it promised not
to. `the_test_process_itself_holds_no_capability_to_write_sysfs` is the other
control and it runs everywhere, asserting an absence — if the test process could
write a root-owned sysfs attribute, every result in the file would be about a
privileged daemon rather than the shipped one.

**Suite.** 767 → **786 passing, 0 failed, 4 ignored** across 116 test-result targets (785 before the renumbering test)
(114 cargo targets), the +19 being `sys`'s five capability guards, `replug`'s ten
unit guards, and the four in `p7_replug_hardware`. `.snx-bin` joins `SKIP_DIRS` in
`meta_names.rs`.

**The premise is confirmed, by measurement.** `CAP_DAC_OVERRIDE` alone is
sufficient for the `authorized` write: blessed with exactly that one capability and
nothing else, the helper deauthorizes and reauthorizes an FT232R with no `EACCES`
and no `EPERM`. The mode bits really are the whole gate. Recorded here because §9
says a confirmed premise is as load-bearing as a refuted one — and one *was*
refuted, below.

**`devnum` was the pre-registered discriminator and is refuted.** The reasoning was
that a driver rebind leaves it alone while a real re-enumeration always changes it,
so `devnum_before != devnum_after` would separate the two. Measured on 7.0.0-29 it
does **not** move: an `authorized` 0→1 cycle unbinds and rebinds the configuration
without destroying the `usb_device` object that owns the address, so the number
survives. It reads `6 → 6` in every run. A physical unplug would change it; this
operation does not.

What the same trace showed instead, sampled every 50 ms through a live cycle:

```
12:32:54.29  tty=1 devnum=6 auth=1 node=yes
12:32:54.83  tty=0 devnum=6 auth=0 node=no     <- the device is genuinely gone
12:32:56.36  tty=1 devnum=6 auth=1 node=yes
```

The tty is destroyed and `/dev/ttyUSB0` disappears and returns — which is exactly
the event the daemon experiences, and is therefore the honest discriminator. The
guard asserts that disappearance, sampled by the **test**, and records `devnum`
without asserting on it.

**The second vacuity, caught the same way §3.50 catches them.** With the
discriminator fixed the test passed in 1 ms of hold — the helper could take the
device down and put it back before the daemon noticed anything, so "the node is
`active` afterwards" proved nothing. The guard now also requires the daemon to
**leave `active`** during the outage, and the hold lasts exactly as long as that
takes. Measured, 3 runs: statuses seen while down are `["waiting"]`, the transition
is observed in **2–3 ms**, and `resolved_path` returns as `/dev/ttyUSB0` both times.
Two figures worth keeping: the daemon notices a real USB disconnect an order of
magnitude faster than its own 200 ms `READ_POLL_TIMEOUT_MS` budget (a destroyed tty
makes the read fail at once rather than waiting out a poll), and Linux reuses the
lowest free minor, so an unchanged `ttyUSBn` is the normal outcome of a
single-adapter replug — which is why the path is recorded and never asserted.

**Iterating on a privileged test must not cost a `sudo`, and at first it did.** The
helper was re-blessed twice during bring-up, both times because *measurement* code
living inside the blessed binary had changed. That is the exact problem the project
this pattern came from solves with a capability-conferring runner. The resolution
here keeps the narrow grant and moves the volatile half out instead: the `hold`
verb deauthorizes, waits for **stdin EOF**, and reauthorizes, so the caller owns the
hold length and does all sampling unprivileged. `while_deauthorized()` in the test
is the whole protocol — spawn, read the `down` line, measure, drop stdin, read the
`up` line. The blessed binary now contains the two writes and nothing else, and has
no reason to change; the experiment rebuilds freely. Crash safety is unchanged and
in fact stronger: EOF fires however the caller dies, including `SIGKILL`, which is
§15.43's leash used for the same reason in a second place.

**`scripts/bless`** is the `just bless` analogue: build, install, one `setcap`,
verify — `--debug` (default), `--release`, `--all`, `--verify`. It is idempotent and
**skips the privileged step entirely** when the installed copy already matches the
build and already carries the capability, so re-running it prompts for nothing. It
prefers `sudo` and falls back to `pkexec` when there is no terminal to prompt on,
which is what let this session bless at all.

**A whole-suite flake, unresolved, and stated at the strength the evidence
supports (§9).** Two rig tests fail together inside full rig-attached runs:
`p12_serial_exclusivity::a_break_straddled_by_a_replace_leaves_the_line_transmitting`
("the crossover rig is not carrying bytes; this test would prove nothing") and
`serial_hardware::crossover_rig_custom_baud_byte_exact` (once at 67.7 s against a
usual 10.4 s; once losing **64 bytes of 32768** on the wire). What is measured:

| observation | count |
|---|---|
| full rig-attached runs **with** `SNX_REPLUG*` set | 2 failing of 5 |
| full rig-attached runs **before** this work existed | 0 failing of 5 |
| the two binaries run **in isolation**, back to back | 0 failing of 5 each |
| the direct `p7_replug_hardware` → `p12_serial_exclusivity` sequence | 0 failing of 2 |

**It is the same two tests both times, which is not the shape of a random flake** —
they are the two most timing-sensitive consumers of the physical wire. But the
obvious story, "the replug tests disturb the rig", is **not supported**: in the
second failing run `p12_serial_exclusivity` executed at log line 676 and
`p7_replug_hardware` at 1073, so p12 failed *before* any replug test ran. The rig
itself is healthy — ten isolated binary runs immediately afterwards were clean.

So the honest statement is: a lane that was clean 5 of 5 now fails 2 of 5, the
failures cluster on two specific tests, the rig hardware is not the cause, and the
mechanism is **not established**. No root cause is claimed and none should be
quoted from this entry. The next rig session should reproduce with
`--test-threads=1` and with the replug tests excluded but their binary still built,
which separates "the suite got longer" from "the replug tests did something".

**The renumbering test landed, and it is the first measurement of §12's founding
premise.** Design §1 rests on *"the same adapter does not always return as the same
`/dev` path"*, and until now nothing exercised it: a single-adapter replug cannot,
because Linux reuses the lowest free minor and the adapter comes back as the same
`ttyUSB0` it left — a config keyed by *path* would have survived by luck.
`identity_survives_a_replug_that_renumbers_the_tty` cycles **both** adapters in one
`hold` and reauthorizes them in the opposite order, so the one back first takes the
lower minor. Measured: `BH00LL8O` moves `ttyUSB0 ↔ ttyUSB1`, the daemon's
`resolved_path` follows it, and `identity` is unchanged — with the config never
touched. Three consecutive runs alternate cleanly (0→1, 1→0, 0→1).

Two details that are the difference between a guard and a decoration:

* **The order is chosen from the current state, not assumed.** Naming the ports in
  a fixed order passes on a fresh box and then fails on its own second run, because
  the adapter is already on the far minor and the "swap" moves nothing. The test
  reads which adapter currently holds the lower tty and picks the order that moves
  the one under test. That is why it is repeatable.
* **Fail-first, run rather than argued.** Driving the helper with the *other* order
  — the one that returns the adapter to the minor it already had — leaves
  `ttyUSB1 → ttyUSB1`, which is exactly what the guard's `assert_ne` refuses. The
  control was executed on the rig, not predicted.

**A hazard the capability check cannot see, and where the warning belongs.** A copy
blessed before an edit is perfectly functional and runs the *old* helper, so a test
that only asks "are you blessed?" would silently measure code that no longer exists
in the tree. `blessed_replug_helper()` therefore also compares the blessed copy with
`target/<profile>/` and **warns** when they differ, naming `scripts/bless`. A
warning rather than a failure, because the comparison is over bytes: a relink
changes them with no source change (measured once during a `cargo test --workspace`
that touched nothing in `devprep/` — the build id at byte 25 moved), and reddening
the suite for a no-op would be worse than the hazard. Cargo turns out to be
reproducible for comment-only edits — `touch` plus a trailing-comment rebuild both
produced byte-identical binaries — so the false-positive rate is lower than feared.
Fail-first proof, run rather than argued: appending one byte to the built artifact
makes the warning fire verbatim while the test still passes, and restoring it
silences the warning again.

**Open.** Linux only, by construction. The `by-path` topology identity is still
untested against a real cycle: it is the one §12 fallback a sysfs
deauthorize/reauthorize could prove directly, since the sysfs name `3-1` denotes the
*port* and survives the cycle. Also worth knowing rather than discovering: after a
renumbering run the two adapters have exchanged `/dev` names. by-id paths and
identities are unaffected, so `SNX_REPLUG_DEV`/`_B` stay correct, but a
`SNX_CROSSOVER_A=/dev/ttyUSB0` now names the opposite adapter — harmless on a
symmetric crossover, and the test says so in its output rather than leaving it to be
found.

### 3.55 The teardown ledger's two named siblings closed: `serial` and `leg`

**Design:** §15.50 charges an interior node's queued targetward bytes at destruction and
names its own residue in the same breath: "`serial` and `leg` own queues of the same
shape and deliberately report nothing yet — a counter reading 0 while bytes are
destroyed would be worse than the silence it replaced — so their adoption is scheduled
work (plan §18), never a silent default". Plan §18 item 2 is that work, and it stated
the shape in advance: "a shared-layer change of design §16.1's class, never a local
patch (notes §3.31 files it under §16.5)".
**Reality:** both are closed. `remove-node` and `teardown` now report
`discarded_at_teardown` for `serial` and `leg` as they already did for `map`, `codec`
and `exec`, and the two silences were reproduced verbatim before the fix rather than
argued from the code.

**The serial case is the sharper of the two, and it is worth saying why.** The deepest
targetward backlog this daemon can legally hold is the one a `waiting` serial node
accumulates: §5 and §7.1's whole answer to an absent device is that its origins
*backpressure* — the channel fills, nothing is dropped — so those bytes are owed to the
operator until the device returns. That queue lived inside `SuperviseCtx`, i.e. inside
the supervisor's future, so `TaskSet::abort_all` destroyed it. The design's own
strongest promise about targetward data was made on the one queue nothing counted. The
leg's is the same shape one layer out: a `faces = host` leg whose peer has never dialled
holds an unbound channel that `next_send` skips by design ("waiting: open but
deliberately not drained"), and a `faces = target` leg's channel tasks hold a wire-fed
queue of the same kind.

**Why this could not be four lines per node, and what it forced.** The interior kinds
needed exactly four call sites each (`watch`, `drain`, `bytes` twice). `serial` and `leg`
could not, for two reasons that only appear together.

* *The queues do not all carry a `Chunk`.* A `faces = target` leg's targetward queue
  carries `Inbound { epoch, bytes }` — 37-LEG-1's per-chunk provenance tag. So
  `TargetwardInbox` became generic over its item behind a one-method
  `TeardownBytes` trait, with `TeardownLoss` holding `Box<dyn TeardownDrain>` so one
  node can watch queues of different item types. A default type parameter
  (`TargetwardInbox<T = Chunk>`) kept every existing spelling in `map`/`codec`/`exec`
  compiling unchanged, which is how a generalization this wide touched three adopters
  not at all.
* *The same receivers feed the purge*, which is what notes §3.31 predicted would make
  this a §16.5 change. `boundary::drain_to_quiescence(&mut Receiver<Chunk>)` is gone;
  the bounded drain-then-yield rounds now live on `TargetwardInbox::purge_to_quiescence`
  and are the daemon's only statement of that policy. Its three XC-PURGE-1 guards moved
  with it, unchanged, to `runtime.rs`'s tests — they have now been written against
  `nodes/serial.rs`, then `boundary.rs`, then `runtime.rs`, and the rule they pin has
  not moved once.

**The move was forced by a defect, not by tidiness, and this is the part the plan did
not predict.** The obvious adaptation is to lend the receiver: `slot.take()`, hand it to
the existing helper, put it back. That is wrong here, and wrong in exactly the way
§3.31's original defect was wrong. The helper holds the receiver on its own stack across
the `yield_now` between rounds — the yield exists precisely so a sender suspended in
`tx.send().await` can resolve and push one more chunk — so a `remove-node` landing on
that yield finds an empty slot, charges `0`, aborts the task, and destroys the chunks
that sender just pushed. The whole point of `TargetwardInbox` is that the node can reach
its queue *at the instant the operator asks*, and a lend is a window in which it cannot.
So `purge_to_quiescence` runs every round through `slot.with_mut` and the receiver never
leaves. `a_purge_leaves_the_queue_where_a_teardown_can_still_count_it` pins it, and its
own fail-first shape is stated in its doc comment: the lending version answers `0`.

**Where it surfaces.** `discarded_at_teardown` on `serial` in `state`, and on `leg` both
**per channel** and summed on the node. Per channel because §5 asks for loss that is
*attributable* and one number for eight channels tells an operator what was lost without
telling them where; the node-level sum is what the reply quotes. `docs/rpc/observation.md`
carries the per-kind breakdown, `docs/rpc/configuration.md` the `remove-node` row, and
`docs/rpc/lifecycle.md`'s `teardown` result — see the defect below.

**Exactly what is covered, restated because "serial and leg are fixed" would be too
broad.** For `serial`: the single arbitrated host-facing targetward receiver, plus the
chunk the writer holds. For `leg`: per channel, the `faces = host` arbitrated targetward
receiver *or* the `faces = target` wire-fed `Inbound` receiver, plus each pump's held
chunk. Deliberately **not** covered, and for a stated reason: a `faces = target` leg's
per-channel *relay* carries local hostward device data on its way to the wire. Those
bytes travel the other direction, whose §5 policy is drop-and-count at the consuming
boundary (`dropped_slow_consumer`), and charging them here would report a hostward loss
under a targetward name.

**The `leg`'s multiplexed write half needed one thing the interior kinds did not.**
`TargetwardInbox`'s mid-flight `held` slot is cleared "at the top of the next receive,
the one place a pump can be in only when the previous chunk's fate is settled". A
multiplexer breaks that: `next_send` round-robins one write half over N inboxes, so the
settling point is before polling *any* of them, not before polling the one that produced
the chunk. Clearing only the polled inbox would leave a producer's `held` set for as long
as its siblings kept the write half busy, turning §3.31's deliberate one-chunk
over-report into one unbounded in time. Hence `settle_held`, called across the whole set
at the top of `next_send`.

**Guards, and the fail-first proofs, executed.** Four new tests in
`itest/tests/p13_teardown_accounting.rs`, in the existing idiom: device-free, RPC-acked
`send`s so "in flight" is observed rather than assumed, and conservation asserted as an
equality. Two per node — the reply reports what it destroyed, and the conservation law
across the removal. Both reverts were done in place and `serial-nexus-daemon-bin` rebuilt
each time (plan §3 rules 9 and 10).

Removing `self.teardown_loss.drain()` from `SerialNode::signal_stop`, **2 of 7 fail and
the leg's two stay green**:

```
assertion `left == right` failed: the removal must report every targetward byte it
destroyed (§5): {"cascaded_edges":0,"discarded_at_teardown":0,"purged_bytes":0,
"released_locks":0,"removed":"usb0"}
  left: Some(0)
 right: Some(8016)

assertion `left == right` failed: conservation across the removal: destroyed 0 + purged 0
+ purged-on-reconnect 0 must equal the 2560 accepted, with delivery witnessed at zero by
`open: false`.
  left: 0
 right: 2560
```

Removing the `channel_teardown` drain from `LegNode::signal_stop`, **the mirror image —
the leg's two fail and the serial's two stay green**:

```
assertion `left == right` failed: the removal must report every targetward byte it
destroyed (§5): {"cascaded_edges":0,"discarded_at_teardown":0,"purged_bytes":0,
"released_locks":0,"removed":"uplink"}
  left: Some(0)
 right: Some(4040)
```

That the two reverts redden disjoint pairs is worth more than either proof alone: it is
what says the two adoptions are independent rather than one mechanism answering for both.

**Delivery is witnessed, not assumed, and the two nodes differ there.** The leg counts
what it puts on the wire (`accepted_targetward`), so its conservation law reads entirely
off reported counters and is the stricter of the two. A serial node has no
"bytes written to the device" counter, so the zero has to come from somewhere else: the
guard asserts `open: false` on the node immediately before the removal, and a node with
no port has written nothing. Stating that as a witness rather than leaving it implicit is
§9's rule — a guard that assumed delivery was zero would pass vacuously the day someone
gave the fixture a real device.

**`exec`'s floor is named rather than closed**, which is what plan §18 item 2 asked for,
and the naming now says something new. The mechanism to close it no longer has to be
built: the inbox is generic, so exec's internal merged `mpsc::Receiver<(String, Chunk)>`
needs only a `TeardownBytes` impl and the same `watch`/`drain` pair. What is missing is
the *guard*, and it is the hard half — "a chunk is sitting in the merge queue" is not a
state an RPC ack can establish, so it wants an exec child that has stopped reading its
stdin. Recorded at the counter in `exec.rs` and in `docs/rpc/observation.md`.

**One shipped documentation defect fixed on the way (plan §18 item 1 names it too).**
`docs/rpc/lifecycle.md` carried **two** result tables for `teardown`: a normative one
under `### Result` listing only `torn_down`, and a second, unheaded one after the example
carrying both fields with a *different* wording of `torn_down`. The example between them
emitted both fields, so the normative table was the wrong one. Collapsed to a single table
under `### Result`, following `configuration.md`'s `remove-node` section as the shape,
with both relative links preserved (`meta_gates::entry_point_doc_links_resolve` run).

**Two files outside plan §18 item 2's stated scope had to change, and both were forced
rather than chosen.** `daemon/src/nodes/mod.rs` holds the `Node::discarded_at_teardown`
dispatch — its `Serial | Pty | Log | Leg => 0` arm *is* the shipped statement of the
silence, so it cannot be removed from anywhere else. And `daemon/src/boundary.rs` had to
lose `drain_to_quiescence`: once both callers moved, it was dead code, and the gate is
`-D warnings`. That is a good outcome rather than a concession — leaving it would have
shipped two spellings of one purge rule, which is the §16.1 class this item was filed
under.

**Gates run for this change** (targeted, since the tree was under concurrent edit):
`cargo clippy -p serial-nexus-daemon --all-targets -- -D warnings` and the minimal-daemon
clippy (`-p serial-nexus-daemon-bin -p serial-nexus-daemon --no-default-features`), both
clean; `cargo test -p serial-nexus-daemon --lib` 174 passed; and the itest binaries that
touch a leg, a serial node or a purge — `p13_teardown_accounting` (7), `p12_leg_accounting`,
`p6_outage`, `p6_fragmentation`, `p6_hostility`, `p6_binding`, `p6_head_of_line`,
`p6_insecure_bind`, `p6_reference`, `p9_leg_arbitration`, `p7_matrix`, `p7_unplug`,
`p11_replace_atomicity`, `p4_purge`, `p4_waiting`, `p12_send_deadline`, `p10_edge_surgery`,
`p9_unwired_interior`, `control_plane`, `data_path`, `p3_counters`, `p3_exact_loss`,
`p5_info`, `p12_control_streams`, `p8_map`, `p12_resolver_identity`,
`p7_snapshot_lifecycle`, `p12_config_rules`, `meta_gates`, `meta_names` — all green. The
suite count moves by **+4** (the four new `p13_teardown_accounting` tests) plus **+1** in
`serial-nexus-daemon`'s lib target (`a_purge_leaves_the_queue_where_a_teardown_can_still_count_it`;
the three XC-PURGE-1 guards moved between targets rather than being added, so they are
not a delta). No new cargo target. A whole-workspace run was **not** taken here and the
headline figure in AGENTS.md §2 is therefore left for whoever runs one.

### 3.56 §3.29's seven latent siblings, dispositioned: five converted, two argued as exceptions

**Design:** plan §18 item 6, and the rule it enforces — design §15.46 / plan §3 rule 8 / AGENTS.md
§6: *a byte counter is read while the client that fed it is still open*, because reading it
afterwards asserts that the **kernel** retained the bytes across the slave's last close. Doctor P13
measures that rather than assuming it: `retains` on Linux 7.0.0-29 (`close(2)` in tens of µs, 64/64
recovered — `docs/doctor/linux-7.0-2026-08-05-tier3.json`) and `waits-then-discards` on Darwin 24.6.0
(600104 µs and 0 of 64 with no reader; 29 µs and 0 of 64 for an `O_NONBLOCK` slave —
`docs/doctor/macos-24.6.0-2026-08-05-tier3.json`).

**Reality:** the rule lived in one helper, in one test file, with one caller. §3.29's own sweep table
named seven further guards of the same shape and left every one of them alone, on the reasoning that
each was Linux- or rig-gated so "none is red today". That premise expired: `p4_free_for_all` fails
**12 of 12 on Darwin** over the crossover rig, losing 5–31 bytes of 32768, against 20 of 20 on Linux
over the same rig at the same commit (design §15.48, notes §3.51) — which is this class's predicted
signature, though not, on that evidence alone, its established mechanism.

#### The helper is promoted, and generalized without losing its teeth

`settled_while_open` moves from a private fn in `itest/tests/p8_map.rs` to
`itest/src/lib.rs`, carrying its whole 70-line argument: the borrow rationale, the "precisely what is
and is not stronger" paragraph, the pointer to the deliberate closes-first exception, and the
**withdrawn** `tcflush(TCOFLUSH)` referee (which on a Linux pts empties the peer's flip buffer, not
the master's ldisc `read_buf`, and destroys the bytes 100% at a 0 µs delay and 0% at 20 µs and
beyond — dead code that reads like a guard). Three shapes now exist:

* `trait OpenWitness` — `label()` plus `prove_open() -> Result<(), String>`, object-safe, so a call
  site can name several witnesses of different types at once.
* `settled_while_open(&mut [&mut dyn OpenWitness], what, timeout, cond) -> bool` — the polling form,
  unchanged in contract. The witnesses are proven open **twice**: before the wait, so an observation
  that began against an already-closed client is never scored, and at the instant the condition
  became true, so a witness that closed *during* the wait is named rather than tolerated.
* `observed_while_open(&mut [&mut dyn OpenWitness], what, FnOnce) -> T` — the same rule for an
  observation that **blocks** instead of polling. Four of the seven sites read a sim subprocess's
  verdict to EOF, which is a byte count as much as a `state` counter is, and wrapping a `join()` or a
  `wait_with_output()` in a `wait_until` predicate is contortion, not enforcement.

`attach_slave(path)` joins them: a blocking, `O_NOCTTY` open of a pty node's slave. Blocking
deliberately — P13 measures an `O_NONBLOCK` slave losing its queued bytes *unconditionally* in 29 µs
on Darwin against ~600 ms of drain-wait for a blocking one, so a witness opened non-blocking would
arm the hazard it is held to disarm.

**Two implementations of `prove_open`, and only one of them is decorative.** `std::fs::File` answers
`fstat`; Rust owns the descriptor, so the type system has already ruled out the interesting failure
and the syscall exists to keep the parameter from being ornamental. `Sim` answers `Child::try_wait`,
and that one is load-bearing — see the vacuity below.

**The enforcement is unchanged and is the whole point**: the witness is *borrowed*, so the caller's
later `drop(witness)` moves it and relocating the observation below the close is `E0382` at compile
time on every platform, not a comment a future editor can step over.

#### What each of the seven became

| # | site | disposition |
|---|---|---|
| 1 | `serial_hardware::inject_verify` (4 rig call sites) | converted — harness fd on the injector slave, held across the arrival wait |
| 2 | `p4_exclusivity::exclusive_write_lock_is_byte_exact` | converted — harness fd on `ttyA`, **plus** the two locked-out `Sim`s as witnesses |
| 3 | `p4_free_for_all` | converted — harness fds on both writer slaves, held across the sink's verdict |
| 4 | `p4_purge::non_holder_backlog_is_purged_on_detach…` | **exception**, argued in its doc comment |
| 5 | `p4_purge::synchronous_grant_lets_a_post_grant_command_through_intact` | converted — harness fd on `ttyB`, attached *after* the grant |
| 6 | `p12_pty_setup::a_fresh_console_session_does_not_inherit…` | **exception**, argued in its doc comment |
| 7 | `p6_outage::outage_faults_then_purges_then_recovers_byte_clean` | converted — harness fd on `p0`, held across the purge reading |

The converted shape is uniform: the harness opens the slave itself and lets the existing one-shot
`serial-nexus-sim client` do the writing. **That is exactly as strong as holding the writing client
open, and the reason is worth stating rather than assuming**: every kernel hangs its flush on the
*last* close — XNU runs `ptsclose` → `ttylclose` → `ttywflush` when the reference count reaches zero,
and Linux charges `discarded_at_last_close` at the same edge — so while any fd remains on the slave,
no kernel reaches its flush. It also buys the thing a `--hold-ms` cannot: there is no timer, so
nothing to expire under load (§9's proxy in time), and the close is a `drop` the compiler can see.

Two conversions carry a second change each. `p6_outage`'s single-shot read of `purged_on_reconnect`
became a bounded wait, which fixes a race the old code had: `leg.rs` sets the node status to
connected at the top of the connect path and runs the purge several statements later, so observing
`connected` never implied the purge had been counted. And `p4_purge`'s `spawn_holding_client` — a
`Sim` on a `--hold-ms 5000` timer — became `hold_a_locked_out_backlog`, which writes the 2048 seeded
bytes **from the harness** and hands back the still-open slave.

#### The two exceptions, and what they owe

Sites 4 and 6 assert counters that come into existence *because* of the close and read 0 before it:
purge-on-detach is the detach's own effect, and `discarded_at_last_close` moves exactly once, at the
close. No ordering observes them early; converting them would delete them rather than strengthen
them. They are therefore treated exactly as `p8_map`'s
`a_closing_writers_residual_is_forwarded_not_purged` already is — the post-close assertion stays,
each test's doc comment argues the exception and cites P13's *measured* policy, and neither moves to
a kernel P13 has not measured. Site 6 in particular is **not** ported to macOS, where
`discarded_at_last_close` is structurally 0 (`docs/macos.md` §3) and the guard would read green while
measuring nothing.

What they owe in exchange is a pre-close positive witness, so the post-close number is not the only
evidence.

* Site 4 gets a real one. The 2048 bytes are now written by the harness, so `write_all` returning
  `Ok(())` **is** the statement that the kernel accepted exactly that many into an unread buffer —
  the test's own premise, previously never checked. Its predecessor was `purged == 0` before the
  detach, a zero equally true of a client that wrote nothing at all. The payload is deliberately not
  filtered for control bytes: §7.2 promises the pair is already raw, so a byte-for-byte 2048 reaching
  the purge counter *checks* that promise, and a pair left cooked would expand `0x0a` under
  `OPOST`/`ONLCR` and redden the exact count. It does not: 2048 exactly, measured.
* Site 6's witness went through **three shapes, two of them refuted by their own controls**, and the
  record keeps all three because §9 says a refuted diagnosis is as load-bearing as a confirmed one.
  (i) `total - accounted() > 0` — "bytes are still inside the pair". Planting a session A that
  *drains* the console left it green: bytes delivered to a reader are absent from every loss counter
  too, so the metric could not see the premise being destroyed. (ii) `dropped_slow_consumer > 0` —
  "the pair is saturated". Refuted by measurement on this box: the console's hostward bridge
  backpressures rather than shedding in this graph, so all three loss counters read **0** with 65537
  bytes fanned out and the whole 65537 lands on `discarded_at_last_close` at the close — a guard
  built on that reading would have reddened the healthy run. (iii) shipped: with the session proven
  open, the console must have shed **none** of the `total` bytes that an independent consumer (the
  log file) has already received in full. Its limit is named at the assertion rather than glossed:
  no counter the pty node publishes separates *queued in the pair* from *delivered to a reader*, and
  closing that would need `FIONREAD` on the slave, which this crate may not issue (`unsafe` lives
  only in `serial_nexus_sys`, AGENTS.md invariant 3). What keeps the premise true meanwhile is a
  property of the file — `a` is never read from between its attach and its drop — stated rather than
  implied.

#### A vacuity the recon did not predict, and its executed proof

`p4_exclusivity` holds two locked-out writers open with `--hold-ms 20000` against a `--timeout-ms
25000`, so that "a non-holder's buffered bytes never reach the device" has bytes to be about. If
either hold expires before the holder finishes streaming, that writer's backlog is purged at its own
detach and the test's whole claim passes **because there is nothing left to leak** — a vacuous pass
no counter in the graph distinguishes from a real one, and one that gets likelier the slower the box.
That is what the `Sim` implementation of `OpenWitness` is for, and both halves were run:

```
# the holds shortened to 1000 ms (a loaded box), witnesses in place -> RED
thread 'exclusive_write_lock_is_byte_exact' panicked at itest/src/lib.rs:967:13:
the device's byte count under an exclusive lock: the held serial-nexus-sim child was no longer open
when its byte counter was read — the sim already exited (exit status: 0) — its --hold-ms elapsed
before the observation finished, so this reading was taken against a closed slave.
A byte counter read after its producer closed asserts that the *kernel* retained the bytes across the
slave's last close, which Linux does and Darwin does not (notes §3.29, plan §3 rule 8; doctor P13
measures both).

# the same shortened holds, the two Sim witnesses removed from the list -> GREEN
test exclusive_write_lock_is_byte_exact ... ok
```

The timer itself stays, and the reason is recorded rather than hidden: the sim's slave is held by a
subprocess this harness cannot leash without a sim-side stdin-EOF hold (the shape §3.54 built for
`serial-nexus-devprep hold`), and `sim/` was outside this change's scope. The check is what converts
the timer from a silent proxy into a named failure.

#### Fail-first, executed

The compile-enforced sites were each regressed by moving the observation below the close. Verbatim,
one of the six:

```
error[E0382]: borrow of moved value: `witness`
   --> itest/tests/serial_hardware.rs:144:15
    |
124 |     let mut witness = attach_slave(inj);
    |         ----------- move occurs because `witness` has type `File`, which does not implement the `Copy` trait
...
142 |     drop(witness);
    |          ------- value moved here
143 |     let arrived = settled_while_open(
144 |         &mut [&mut witness],
    |               ^^^^^^^^^^^^ value borrowed here after move
```

The same regression was run and captured at `p4_free_for_all.rs:219` (both sessions, two errors),
`p4_purge.rs:466`, `p6_outage.rs:385`, `p8_map.rs:1093`, and `p4_exclusivity.rs:365` — where it
produced **three** errors, one per witness, including the two of type `Sim`, which proves the borrow
carries through the `&mut [&mut dyn OpenWitness]` slice for a non-`File` witness as well.

Behavioural fail-firsts, each run against the tree and then restored:

| plant | result |
|---|---|
| `p4_exclusivity`: locked-out holds shortened to 1000 ms | **RED** with the witness message above |
| the same, `Sim` witnesses removed | **GREEN** — the vacuous pass, demonstrated |
| `p12_pty_setup`: session A attaches *after* the fan-out (an empty pair) | **RED**: `the console had already accounted for all 65537 fanned-out bytes while its client was still attached (65537 logged independently) …` |
| `p12_pty_setup`: session A drains the console (witness shape (i)) | **GREEN** — the refutation that produced shape (iii) |
| `p4_purge`: the harness writes `SB - 1` bytes | **RED**: `purge-on-detach did not count exactly 2048, got Some(2047)` |

#### Runs, on a measured box

Linux 7.0.0-29, 8 cores, load average 0.21–0.76 throughout, both FTDI adapters attached.

* All seven touched targets, `SNX_CROSSOVER=required` with both ports named: **31 passed, 0 failed,
  0 ignored** (p12_pty_setup 8, p8_map 9, p6_outage 4, p4_purge 3, p4_exclusivity 2,
  p4_free_for_all 1, serial_hardware 4). No self-skips; `serial_hardware` took 10.37 s, which is the
  figure a genuinely-executing rig lane reads (§3.35).
* `p4_free_for_all` over the **rig** (`SNX_SERIAL_PAIR=rig`, which is what design §15.48's "20 of 20"
  figure means — software wins by default even with the ports exported): **20 passed / 0 failed of
  20**, matching the pre-conversion Linux figure exactly. The conversion neither fixed nor broke
  anything on the kernel of record, which is the honest Linux outcome for a Darwin-only red.
* `p4_exclusivity` over the rig: **5 of 5**.
* `p6_outage` ten times: **10 of 10**. Run because the conversion holds a second fd on `p0` across
  the reconnect, and step 6a's flood barrier reasons in detail about where the post-reconnect
  hostward backlog lands. The witness is dropped before that barrier for exactly this reason.

Suite-count effect on the touched targets: **zero**. No test was added or removed; seven guards
changed shape and two gained assertions inside tests that already existed. A whole-workspace run was
not taken here (the tree is being edited concurrently), so AGENTS.md §2's headline figure is left for
whoever takes one.

#### The pre-registered falsifier for the next Darwin run (plan §3 rule 13)

This does **not** claim to have fixed `p4_free_for_all` on Darwin; nothing here was tested on Darwin,
and design §15.48/§9 forbid a root-cause claim on this evidence. What it does is remove one
hypothesis cheaply, and the next Darwin rig run decides between three outcomes, written down before
it happens:

* **Held** — the test passes on Darwin over the rig, repeatedly. Then the loss was the writers'
  last close destroying the tail of a payload the kernel had accepted but the daemon had not yet
  read, which is §3.29's class exactly, and the 5–31-byte magnitude fits a 1024-byte Darwin pty
  buffer draining at 11520 B/s against a ~90 ms residual.
* **Fired** — the test still fails 12 of 12, with the same 5–31 bytes short and `timed_out: true`.
  Then the read-after-close explanation is **refuted** for this site: the bytes are lost somewhere
  the pty session's lifetime does not reach — candidates in order, the UART's own FIFO under two
  concurrent writers merging onto one free-for-all endpoint, and the sink's `--recv` deadline. The
  conversion should stay anyway (the rule is not conditional on this test), and plan §18 keeps the
  item open with one hypothesis eliminated rather than zero.
* **Half-met** — it passes sometimes. Then the close is *a* contributor and not the only one, and the
  next instrument is a per-direction byte-count diff at the sink rather than another rerun.

A fourth outcome would be evidence against the conversion itself and is worth naming: if the test
begins failing on **Linux** over the rig, the extra fd is perturbing the graph (it keeps
`client_present` true and suppresses the detach purge), and the 20-of-20 above is the baseline to
compare against. Note also that Darwin's `p6_hostility` flake and the two-test rig-lane flake of
§3.54 are unrelated shapes; do not fuse them into this reading.

### 3.57 P14 lands, and the ceiling turns out to be a silent 400x fallback

**Design:** §15.51 (P14), §15.52 (the handshake block, new this session).
**Plan:** §18 items 11, 1, 9, 5, 10, 7 (partial). **Rules:** plan §3 rules 9, 10, 11,
13, 14; AGENTS §7, §9.

**What landed.** `P14`, the maximum-rate search: a ladder climb with a bounded
refinement over a P5-verified cross-paired rig, reporting `max_reliable_baud` plus
`ceiling_kind` — a number and its reason for stopping, never a grade. Opt-in behind
`--port` exactly as P3/P5/P11 are, additionally gated on a cross-paired rig whose
baseline integrity passed, and registered **last** in the binary, because it is by
far the largest wall-clock consumer and anywhere else it would perturb the timing
measurements P6..P13 take.

`p5_rig` now returns a third value, `Vec<VerifiedPair>`, carrying the discovered
pairs **by path**. `RigFacts` stays `Copy` and unchanged: it answers "what kind of
rig is this" for a verdict, and growing it a `Vec` would have rewritten twenty-five
by-value call sites to buy nothing those readers use.

**The measured answer is a finding, not a number.** On the bench rig (two FT232R
cross-wired, Linux 7.0.0-29) every body rung from 9600 to **3000000** passes three
byte-exact round-trips in both directions with zero frame and overrun deltas.
4000000 is `adapter-refused` — and the refusal is the interesting part: `ftdi_sio`
**accepts the ask at the syscall and reads back 9600**, four hundred times below the
request, with no errno anywhere. An operator who sets `baud = 4000000` on this
adapter today gets a 9600-baud port and no diagnostic at all. That is exactly the
experiment §15.51 says the doctor should run first. Refinement brackets the ceiling
to [3000000, 3062500). Two sequential runs agree to the byte and to five
milliseconds: `max_reliable_baud 3000000`, `first_unreliable_baud 3062500`,
`ceiling_kind adapter-refused`, search 21957 ms and 21962 ms.

**Cost, measured.** The search is ~22 s, so a Tier-3 doctor run goes from §3.53's
11.6 s to **~35 s**. A passive run is unchanged at ~3.9 s: P14 skips without
`--port`.

**The refusal discriminator is read off the error, not off its message.** serial2
collapses two very different failures into one `Err`: a `tcsetattr` the kernel
rejected, and its own post-set verification finding the driver landed more than 2.5%
from the ask. They are separable without matching on the crate's error *string* —
which is not a contract — because only the first carries an errno. So
`raw_os_error().is_some()` means the ask was refused before any byte moved (the
platform) and its absence means the ask was accepted and the clock landed elsewhere
(the adapter). Both arms report the read-back beside the request, so the
classification is corroborated by a number rather than resting on the rule alone. On
Linux the errno arm never fires for these rates, which is why the read-back is the
load-bearing evidence.

**A harness bug that produced a wrong number first**, recorded because it is the
kind that reads as a hardware fact. The first ground-truth measurement wrote the
whole payload and *then* read, and reported a ceiling of **250000 baud** on a rig
byte-exact to 3000000. At and above 460800 the payload outruns the receiver's buffer
while the sender is still writing, so the loss is the harness's and so is the
number. P14 polls both directions concurrently — which is what the daemon does (§5)
— and that is the only shape whose failures belong to the wire. The design does not
say this; it was found by measuring, before any probe code existed.

**Fail-first, executed, four mutations on the pure core.** (a) `p14_next_rate`
collapsed to always-climb → the non-monotone bracket test and the refinement-bound
test both red. (b) `p14_verdict` made to grade the number (degrade below 1 Mbaud) →
the never-grades test and the Darwin ask-ceiling test both red. (c) the open end's
clamp to `u32::MAX` deleted → the termination test red. (d) `p14_ceiling` made to
answer `structural-cap` whenever no failure bounds the floor → the bracket test red.
That last one guards a specific hazard: **the absence of a reason must not be
dressed up as a fifth reason**, or a truncated search prints the most impressive
answer in the taxonomy by default. `ceiling_kind` is therefore `Option`, `null`
renders in JSON, and the verdict degrades on it.

**Clippy caught a vacuous assertion of mine, and the repair is the interesting
half.** The termination test asserted `proposed.iter().all(|&r| r <= P14_MAX_BAUD)`
— always true, because `P14_MAX_BAUD` *is* `u32::MAX` and `r` is a `u32`. A guard
that cannot fail, in the test whose whole subject is a bound. What an unclamped
doubling actually produces is a **wrap**: `3_072_000_000 * 2` truncates to
`1_849_032_704`, *smaller* than the rung before it. So the property is
**monotonicity**, which is checkable in the type the field really has, and it is
what reddens under mutation (c).

**The sim hazard is measured, not asserted.** A pts pair carries bytes perfectly at
every rate because nothing clocks it, and `serial-nexus-sim nullmodem` reads
`discovered_pairs: 1` in P5 — so a P14 gated on the pair *count* would climb the
whole ladder against the software double. With **both** of P14's gates removed
(mutation E2) that is exactly what happens: 25 rungs pass, including `4294967295`,
and the probe reports `max_reliable_baud: 4294967295`, `ceiling_kind:
structural-cap`, `status: supported`. A confident wire number with no wire, on the
platform CI runs. Two independent gates prevent it — the UART predicate skips, and
P5's baseline-integrity flag degrades — and removing only the first (mutation E)
still degrades rather than answering. `p14_reports_skipped_not_a_uart_against_the_software_null_modem`
guards the outer layer positively, asserting the skip and the *absence* of a
`max_reliable_baud` key rather than inferring safety from silence.

**Digests.** `probe_set` moves `a131e1f4b46d6c83` → **`94d64d8bbacf1174`**,
deliberately: a new question *is* a new instrument, and §15.44's unequal direction is
the sound one. Every artifact committed before this point is refused by the digest
rather than silently diffed across, which is what that digest is for. `field_set`
moves too.

---

**§15.52 — the handshake block, and it reproduces a hand measurement.** P5's pair
block gains a handshake-continuity reading taken with **both ports open**, which no
other modem read in the probe is: every read in the certificate happens with the
peer closed and therefore cannot answer what the wire carries. It reproduces notes
§3.53 (i) exactly, and the report now says it rather than a session note:

```
5-wire crossover: RTS/CTS both ways, DTR moves nothing
[rts_a_to_cts_b=true rts_b_to_cts_a=true dtr_a_to_dsr_b=false
 dtr_a_to_dcd_b=false dtr_a_to_ri_b=false dtr_b_to_dsr_a=false]
```

**Reported, never judged** — no `fail_if`, no verdict movement — because a 3-wire
rig is §5's own stated assumption and an item that degraded one would report the
operator's cabling as a fault, and would move the verdict on committed artifacts
whose rigs nobody has re-inspected. The DTR arm is the **in-probe negative control**:
on a rig where RTS crosses and DTR does not, a `read_cts` returning a constant and a
rig with every line bridged both fail it. Both polarities are driven, because a line
stuck high passes a one-polarity test, and `stuck-high`/`inverted` are spellings the
classifier can produce rather than collapsing into `true`/`false`. The gate clause is
presence-only and **conditional on a pair certificate having run**, proven against a
planted violation in both directions (the reading deleted → rejected; the reading
present and saying `3-wire` → accepted, because whether *this* rig crosses RTS is
the operator's cabling).

This entry closes only the *doctor* half of plan §18 item 7. The end-to-end
`rts-cts` behaviour test — the daemon actually pausing a writer on CTS — is **not**
done and stays open: §15.52 draws the boundary (P5 measures line continuity; driving
the lines through the daemon is the suite's job), and the measured precondition that
test would gate on now exists.

---

**Plan §18 item 1, the prose-truth sweep.** Three claim families repaired, each with
a guard that did not exist before.

*(a) P5's UART predicate.* `P5_UNCHARACTERIZED` was `#[cfg]`-forked and its non-Linux
half said "not characterizable here (TIOCGICOUNT is Linux-only)"; `p5_verdict`'s
final arm said "no port certifies here however real it is … run the certificate on a
Linux box". Both were true of the predicate §15.47 replaced and false of the shipped
one. They are now **one sentence on both kernels**, whose subject is the *port*: it
answered neither `TIOCMGET` nor `TIOCGICOUNT`, which is what a pts looks like
everywhere. **The guard is the load-bearing part.** The old paired assertion was
`#[cfg(not(target_os = "linux"))]`, so it did not compile on the platform of record
— the defect was unreachable from the box every developer sits at, and repairing the
string reddened nothing. That is AGENTS §9's proxy in space sitting inside the guard
for a defect that was itself a proxy in space. The portable form is **stricter on
Linux**, which §9 names as the tell, and both new guards were proven red by reverting
the specific constant and the specific arm in place.

*(b) "the certificate has to come from a Linux box"* — deleted from
`docs/serial-nexus-doctor.md` and replaced with the artifact that refutes it
(`docs/doctor/macos-24.6.0-2026-08-05-1a9a8fc-tier3.json`, `1a9a8fca1c36`: both ports
`custom_baud=true break=true`, the pair `rate_ladder=true`, P5 `degraded` naming
`icounter` and `deliberate_mismatch` as the two platform-excused items). The
`docs/macos.md` feature-matrix row and `docs/doctor/README.md`'s Tier-3 paragraph
follow. The README's "`Tier 3` appears nowhere in any macOS artifact" clause is
**kept**, because it is still true — but its *reason* changed, and the paragraph now
says which: every committed macOS capture predates §3.49's hoist of the tier sentence
out of the certified arm.

*(c) The unbacked P10 figures.* "raw ~13.8 KiB / cooked ~23.5 KiB" is withdrawn from
the `termios_mode` doc comment, the **shipped P10 consequence string** and the guard
doc comment, and from `docs/macos.md`'s live prose; the relation is kept in the form
`expectations/linux.jq` already uses. `the_shipped_p10_consequence_quotes_no_uncommitted_figure`
is new and exists because **nothing asserted that string** — neither expectation file
inspects consequence text and neither digest can see it, so dropping the figures
would otherwise have reddened nothing. Proven red by putting the figures back.

*(d)* `docs/rpc/lifecycle.md`'s competing `teardown` tables — fixed under §3.55.

---

**Plan §18 item 9, P4's no-udev arm, and the "would print identically" claim
measured.** Two guards in `itest/tests/expectation_gates.rs` drive the *shipped
binary* over `--dev-root` fixture trees: one with no by-id tree at all (the
container / busybox-mdev shape, whose consequence sentence no committed artifact has
ever printed), one **mixed** — one device udev names, one reachable only through
`<sys>/class/tty`. The mixed tree is the discriminator, because `sysfs_only` reads 0
on every box anyone has captured from.

Three executed mutations. **M1**, the sysfs merge loop deleted: the mixed tree still
reports `status: supported` with the *identical* consequence "Resolver produces
canonical identities; configs survive replug and cold start (§12)." while
`sysfs_only` falls 1→0 and `canonical` 2→1 — the ledger's "would print identically
had it returned nothing" claim, now witnessed rather than argued. **M2**,
`or_insert` → `insert`: only the mixed test reds, proving the guard sees the merge's
*semantics* and not merely the loop's presence. A negative control runs in the same
body — the same assertions against a tree with the by-id device alone must report
`sysfs_only: 0` and no second key — so the numbers cannot be satisfied by a probe
that answers the same for any tree.

---

**Plan §18 item 5, the macOS `crossover_ports` doctrine — decided, not deferred.**
The arm auto-selected any two `cu.usbserial*` nodes it found, which is exactly what
`rig_candidates`'s own doc forbids ("reported, never auto-selected"), and it did so
on a plain `cargo test --workspace`. Deleting it outright is *not* free: the software
serial doubles are Linux-only (a pty is not a serial device on Darwin), so the scan
is macOS's only serial provider and removing it would make eleven tests self-skip on
a working box — notes §3.35's defect wearing the other hat. **The decision is: the
doctrine transplants, behind a named opt-in.** `SNX_CROSSOVER` set to anything runs
the scan; unset, the pair is reported in the skip message and never chosen. Three
consequences are stated rather than left to be discovered: the pre-change macOS
platform record needs `SNX_CROSSOVER` exported to reproduce; **`SNX_CROSSOVER=required`
becomes reachable on a Mac for the first time**, having been structurally dead there
(required-mode fires only on a skip, and the scan prevented skips); and the selection
is now announced, so a run that transmitted on scanned hardware says which two nodes
it picked.

**Plan §18 item 10, the last skip class without a `required` spelling.**
`web_tls_round_trip` carried **two** silent skips, not one — no `curl`, and an
environment that cannot bind `0.0.0.0:0` with `--tls`. `SNX_TLS=required` covers
both, as a third instance of one mechanism rather than a third mechanism. Both arms
run, not predicted: with `curl` hidden the test skips and stays green, and with
`SNX_TLS=required` it fails naming which skip fired.

### 3.58 Plan §18 item 3, run as far as this box allows — and the protocol's own weak arm

**Plan:** §18 item 3. **Rules:** plan §3 rules 13, 14; AGENTS §6 (`--no-fail-fast`
for platform validation), §8 (measure the box), §9 (record refuted diagnoses; no
root-cause claim without evidence).

**Pre-registration, written before any run.** Notes §3.54 records two rig tests
failing together in 2 of 5 full rig-attached runs —
`p12_serial_exclusivity::a_break_straddled_by_a_replace_leaves_the_line_transmitting`
and `serial_hardware::crossover_rig_custom_baud_byte_exact` — against 0 of 5 before
the replug work existed, with both 0 of 5 in isolation, the direct replug→rig
sequence 0 of 2, and the obvious ordering story refuted. Two hypotheses: **H1**, the
suite got longer (contention, not the replug tests); **H2**, the replug tests did
something. Predictions:

| arm | H1 predicts | H2 predicts |
|---|---|---|
| A — `--test-threads=1`, replug lane **green** | pass (concurrency removed) | may still fail (the tests still run, serially) |
| B — replug tests not executing, binary still built | pass | pass |

**Arm B does not discriminate, and that is a property of the protocol §3.54
pre-registered rather than a result** — both hypotheses predict the same outcome.
Recorded here because it was noticed *before* the runs, not after them.

**What ran.** Four full rig-attached workspace runs at `4621cff`, on a settled box
(load 0.06–0.87 throughout, 8 cores, nothing else building):

| run | shape | wall | result | the two §3.54 tests |
|---|---|---|---|---|
| B1 | default threads | 179.3 s | 810 passed, **1 failed**, 4 ignored | both **ok** |
| B2 | default threads | 176.7 s | 811 passed, 0 failed, 4 ignored | both **ok** |
| B3 | default threads | 177.2 s | 811 passed, 0 failed, 4 ignored | both **ok** |
| A | `--test-threads=1` | 280.2 s | 811 passed, 0 failed, 4 ignored | both **ok** |

**The §3.54 pair did not reproduce: 0 failing of 4.** That refutes nothing and
confirms nothing, and the three reasons are stated rather than buried.

1. **The replug lane did not run.** `scripts/bless --verify` reports the installed
   copy `Stale` — the kernel strips `security.capability` on every rewrite and this
   tree has been rebuilt many times — and re-blessing needs one interactive `sudo
   setcap` this session could not issue (`sudo -n true` → "interactive
   authentication is required"). So every run above is the **`SNX_REPLUG` absent**
   configuration, which §3.54 already measured at 0 failing of 5. This is not the
   configuration the flake was seen in.
2. **One of the two tests was modified this session.** `serial_hardware.rs`'s
   `inject_verify` was converted to `settled_while_open` (notes §3.56), so
   `crossover_rig_custom_baud_byte_exact` is not the test §3.54 measured. A green
   run here cannot be read as "the flake is gone".
3. **Arm A was run in its non-discriminating form.** With the replug lane absent,
   `--test-threads=1` collapses into arm B plus serialization, and the arm that
   separates H1 from H2 — replug green *and* serialized — was not run.

**One result that is new, and it is evidence about the family rather than the
case.** Run B1's single failure was **not** either §3.54 test: it was
`p8_web_ui::the_web_console_passes_its_headless_chromium_suite`, and inside it
`graph-editor.spec.mjs:171 › adding a console through the editor makes bytes flow
end to end`, failing with *"the editor's status line reports a refusal, not
/^connect\b/"*. Measured immediately afterwards: **1 of 3 in-suite, 0 of 5 in
isolation** (24.3–24.9 s each, all green). That is the same signature §3.54 reports
for the two rig tests — fails inside a long suite, never alone — on a test that
touches no serial hardware, no USB, and no replug binary at all. It is therefore a
datum for **H1's family** (something about a long, loaded suite) and against reading
the §3.54 pair as replug-specific. **Mechanism not established and no root cause
claimed**; a browser-driven editor test failing under contention has at least three
innocent readings and this session separated none of them.

**What the next rig session should run, and why it is not either of §3.54's arms.**
Neither arm isolates the replug *operation* from the presence of one more test
binary in the run. The design that does: keep `p7_replug_hardware` executing with
its hold reduced to a no-op — authorize immediately, no down time — and compare
against the same run with the normal hold. Same binary count, same suite length,
same test names; only the USB cycle differs. Run it blessed, five of each, and
capture failing names verbatim with no retries.

**Owed, and it is one command:** `scripts/bless` (build + install + one `sudo
setcap`), then the rig lane per AGENTS §3 with `SNX_REPLUG*` set.

### 3.59 The purge that emptied the queue and then dropped the count: §15.50's own defect, one layer up

**Found by an adversarial verification pass over notes §3.55, and reproduced through
production code before anything was changed.** §3.55 closed the teardown ledger's two
named siblings — `serial` and `leg` — by moving the purge-on-reconnect drain onto
`TargetwardInbox` so the receiver would stay in the node's slot across the purge's
yields. That reasoning was right and it was half the problem.

**The measurement.** A `SuperviseCtx` built by `serial.rs`'s own `test_ctx`, a queue
seeded with 8000 targetward bytes and handed in through `TeardownLoss::watch` exactly as
`SerialNode::start` does it, `nodes::serial::purge_on_reconnect` spawned as a task, one
`yield_now` to park it inside the drain, and then precisely what
`SerialNode::signal_stop` does, in its order — `teardown_loss.drain()`, then
`abort_all`:

```
PROBE SERIAL: accepted=8000 discarded_at_teardown=0 purged_on_reconnect=0 accounted=0
```

8000 accepted bytes destroyed with every counter reading zero. That is design §15.50's
Problem paragraph verbatim — *"a saturated interior node destroyed its queued targetward
bytes while answering `purged_bytes: 0` with every node counter at zero"* — reproduced
**after** the fix, on the node the fix was written for.

**Mechanism, and why keeping the queue reachable was not enough.**
`purge_to_quiescence` ran bounded drain-then-yield rounds, accumulated the bytes in a
`u64` local, and returned it; both callers charged their counter *after* the await
completed (`serial.rs`'s `ctx.purged.fetch_add(purged, …)`, `leg.rs`'s
`stat.purged_on_reconnect.add(purged)` per channel). So at a yield the bytes are in two
places at once and neither of them is a counter: gone from the queue, and alive only in
the purge future's stack frame. A teardown there asks both honest questions and gets two
honest zeros — `drain_and_count` finds a queue the earlier rounds emptied, and
`abort_all` drops the frame with the tally in it. The rule the type enforces is
therefore **two-sided**, and only the pair is the invariant: *the queue never leaves the
slot, and no byte that has left the queue crosses an `.await` uncharged.*

**Magnitude is not incidental.** The backlog a `waiting` serial node holds is §7.1's own
answer to an absent device — backpressure the origins, never drop their commands — so by
the time a reconnect runs the purge, the channel is *full* by construction
(`CHANNEL_CAP` chunks). The leg's exposure is N-fold: its loop purges `send_receivers`
one channel at a time, so an abort mid-loop strands every channel already visited. And
`purge_on_reconnect` defaults to `true`, so this is the default path, not a configured
one.

**Fix.** `purge_to_quiescence` takes its counter as an argument (`charge: &dyn Fn(u64)`)
and returns nothing; each round charges the moment its bytes leave the queue, before the
yield and before the loop's own `break`. Removing the return value is deliberate rather
than tidy — the accumulated local *was* the defect, and leaving the shape available
invites it back. The charge fires between `slot.with_mut` and the yield rather than
inside the closure, and the two are equally safe for one stated reason: a teardown is
synchronous (`daemon.rs` composes it inside one critical section) on a single-threaded
runtime, so it can only interleave at an `.await`, and there is none between the drain
and the charge. Outside is preferred because the sink is caller-supplied, and a future
adopter whose counter lived behind the same `CriticalCell` would deadlock a borrow it
never knew it was inside. **The yield was not touched**: it is what lets a sender
suspended mid-`send` land one more chunk, which is why quiescence needs rounds at all
(§6, DM-2/LEG-1).

**Guards, and their executed fail-first.** Two, at two levels, because the leg drains
through the same method and no fixture here builds a connected leg:
`nodes::serial::tests::a_teardown_inside_the_purge_accounts_for_the_backlog_it_drained`
(the production caller, asserting §5 conservation as an *equality* — deliberately not
legislating which of the two words the bytes land under) and
`runtime::tests::purge_to_quiescence_charges_each_round_before_it_yields` (the shared
contract). With the fix reverted in place — accumulate the local, charge once after the
loop — both go red and every pre-existing purge guard stays green, which is the point:
nothing in the tree could see this.

```
assertion `left == right` failed: a teardown inside the reconnect purge destroyed 8000 accepted targetward bytes and accounted for 0: purged_on_reconnect=0, discarded_at_teardown=0 (§5)
  left: 0
 right: 8000

assertion `left == right` failed: charged before the first yield: the bytes are out of the queue, so a teardown here finds nothing to charge and the tally must already be on a counter that outlives this task
  left: 0
 right: 7
```

**The interleave is constructed, not raced.** `purge_to_quiescence` drains the whole
seeded backlog in round 1 and parks on `yield_now`; round 2 would find nothing and break.
So a single `yield_now` in the test puts the purge at the one instant the defect exists
at, and the guard asserts it is *there* (`!task.is_finished()`) rather than hoping — a
purge that completed inside one poll would make the whole test vacuous, which is the
§3.50 class of failure and is refused explicitly.

**A third destroying verb, found in the same pass and fixed rather than declined.**
`load --replace` composes teardown-then-load and called `teardown_with` as a *statement*,
so the `{torn_down, discarded_at_teardown}` it computes was dropped and `load` answered
`{"loaded": 1}`. Measured: 12056 destroyed targetward bytes reported as nothing, against
8016 reported in full when the identical graph went through the `teardown` verb. Design
§15.50 names only the `remove-node` and `teardown` replies, so this needed a decision and
not a patch; it is **in scope**, annotated onto §15.50 rather than declined, for three
reasons. `load --replace` destroys the *whole graph*, so its loss is the largest of the
three verbs. It is the one an operator reaches for without the word "teardown" in front
of them. And a destroyed node's last loss can land nowhere else — `state` has nobody left
to ask — which is §15.50's own stated reason for putting the figure on a reply at all.
`torn_down` rides along rather than being dropped as "not loss", because §15.49's lesson
is that a `0` needs something beside it saying what the `0` means: `0` beside
`torn_down: 0` is "there was no graph", `0` beside `torn_down: 7` is "seven nodes went
and none owed a byte". Both are always present, `0` included, on a plain `load` too.
Fail-first, with the statement-and-dropped-value shape restored and the daemon binary
**rebuilt** (plan §3 rule 9 — `cargo test` does not rebuild a spawned binary):

```
assertion `left == right` failed: load --replace destroyed 8008 targetward bytes and reported none of them: {"loaded":1}
  left: None
 right: Some(8008)
```

`itest/tests/p13_teardown_accounting.rs::load_replace_reports_the_targetward_bytes_it_destroys`
also pins the successor's ledger at `0` — the figure is loss, and loss quietly carried
over would be *delivered* later instead — and asserts the always-present pair on the
non-replace arm. The assertion order is deliberate: the headline check runs first so an
unfixed daemon fails with the message that names the defect, not with the secondary one
it also happens to break. **Owed to the lead:** `docs/rpc/configuration.md` is not in
this session's allowed file set, so its `load` Result table and its `{"loaded": 1}`
example still describe the old two-field-less reply.

**A weakened guard, restored — and the weakening is the interesting part.** When §3.55
moved XC-PURGE-1's three guards onto `TargetwardInbox`, two of them had their terminal
`assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)))` **replaced** by
`assert_eq!(inbox.drain_and_count(), 0)`. That is strictly weaker for exactly the residue
those guards exist to forbid: a stranded `Chunk::new()` contributes `0` bytes, so it
satisfies the byte count and fails the emptiness check — and one of the two *is* the
empty-chunk guard, so the substitution was vacuous precisely where it mattered. Both
assertions are made now, the stronger first, through a `peek_empty` helper that reads
synchronously through the slot (assertion-only; no receiver is lent across an `.await`).
The distinction is not left as prose: `a_stranded_zero_length_chunk_is_invisible_to_the_byte_count`
proves the matcher, showing `drain_and_count() == 0` on a queue that holds a chunk while
the restored check sees it — AGENTS §3's "a scanning gate proves its matcher as well as
its walker", applied to an assertion.

**Exec's floor, named at the two sites that had it and the two that did not.** It was
stated at `docs/rpc/observation.md`, `configuration.md`, `lifecycle.md` and
`exec.rs`'s `state_extra`, and absent from `ExecCodecNode::discarded_at_teardown` and
from `Node::discarded_at_teardown` — the pointed omission, since that arm already spells
out `pty`/`log`'s structural `0` and `serial`/`leg`'s adoption and skipped the one kind
whose number is inexact. Both carry it now. Nothing about the figure changed; what
changed is that a reader arriving at either method no longer takes an exact number away.

**What is not claimed.** No production trace of this race firing in the field exists —
it was found by construction and measured by construction, and the window is one
scheduler slot wide. That is the wrong reason to leave it: §5's promise is not
probabilistic, the window opens on the *default* configuration of every reconnect, and
the loss it drops is the deepest backlog the daemon legally holds.

### 3.60 The witness that proved nothing, the citations that pointed one section off, and an exception whose reason was falsified twelve lines away

**Design:** §15.46 (the read-after-close rule), §15.41 (context hygiene, for the scan's
scope). **Plan:** §3 rules 8, 9, 10; §18 item 6's follow-through. **Rules:** AGENTS §3
(a scanning gate proves its matcher), §5 (anti-tautology), §7, §9.

Five defects an adversarial pass found in §3.56's own work, repaired here. Four of them
are the same shape: *a thing that reads as a proof and is not one*.

#### 1. Thirteen citations, written by the commit whose subject was prose truth

Every code comment §3.56's conversion produced cites **notes §3.55**. §3.55 is the
teardown ledger; the conversion is §3.56 and the prose sweep it also carried is inside
§3.57. Not one `.rs` file in the tree cited §3.56 at all. Corrected in
`itest/tests/{serial_hardware,p4_purge,p4_free_for_all,p4_exclusivity,p6_outage,p8_map,p12_pty_setup}.rs`
— twelve to §3.56, and `serial_hardware.rs`'s "Corrected 2026-08-05" note to §3.57,
which is where that sweep lives.

The mechanism is worth naming because it is not carelessness: the citing code and the
notes entry were written in one pass, the number was taken from the last section that
existed rather than from the one about to be appended, and every later citation copied
the first. A citation is the one kind of comment a reader *follows*. A wrong one costs
more than a vague one — it sends the next editor somewhere that argues something else,
and arriving at a real section reads as confirmation.

**The gate, and the exact bound on it.**
`itest/tests/meta_names.rs::notes_citations_in_code_resolve_to_a_real_section` resolves
every `§3.NN` in every `.rs` file in the tree against the `### 3.NN` headings of
`docs/implementation-notes.md`, and fails on any that does not resolve. It shares the
existing walker (so `target/`, `.snx-bin/` and nested checkouts are skipped once, for
all three gates), plants the violation in every spelling it claims to cover — module
doc, doc comment, line comment, block comment, **string literal** — and asserts the
three shapes that must stay silent: a `.md` file, a file under a skipped build
directory, and a file in a nested checkout.

**It would not have caught the thirteen.** They resolved to §3.55, which exists. A
scanner cannot tell a real section from the wrong real section without reading both for
meaning, and the doc comment says so rather than implying a coverage it does not have.
What it does catch is the adjacent class — a citation written *before* the section it
names — which is how the wrong number gets picked in the first place, and which is not
hypothetical: on its first run it fired on this session's own owed §3.60 citations, and
for a few minutes on another session's §3.59 ones. The uncovered half is a reviewer's.

Scope is `.rs` only. The documents cite each other constantly and historical entries
must keep naming sections a later generation renumbered; a gate that swept them would
need an allowlist per generation. **The scoping premise is checked, not asserted**: the
scan reads the current design and plan and fails if either grows a `### 3.N` heading,
because a bare `§3.N` would then be ambiguous and this gate would be resolving citations
against the wrong document. Design §3 is *Concepts and terminology* and plan §3 is
*Validation toolkit*; neither has numbered subsections today, and plan §3's rules are
cited as `plan §3 rule 8` — a space, not a dot, which the matcher does not touch.

#### 2. `File::prove_open` was a tautology, and the repair is stricter on the platform of record

The witness half of §3.56 shipped `impl OpenWitness for std::fs::File` with
`prove_open = self.metadata().map(|_| ())`. In safe Rust the descriptor is guaranteed
valid for the `File`'s lifetime, so that call has **no reachable failure**. Five of the
six converted sites used `File` witnesses only, so at those sites "proven live" reduced
entirely to the compile-time borrow — a real property, but not the one the name claims.

Measured on this box (Linux 7.0.0-29), on a pty whose **master had already been
closed**: `fstat` on the slave fd returns `Ok` with `mode=020600`, while `stat` of the
same slave's path returns `ENOENT`. The kernel unlinks `/dev/pts/N` at the master's
close. The fd is valid and useless, and the old check was green on it.

`attach_slave` now returns a `SlaveWitness` that **carries the path it was opened
through**, and `prove_open` compares the fd's `(st_dev, st_ino, st_rdev)` against a
fresh `stat` of that path. Three failures instead of none: the path stops resolving
(the pty case above), the path resolves to a different device (a replaced node, or
§12's replug renumbering `ttyUSB0`/`ttyUSB1` — the fd stays perfectly valid through
it), or the fd's own `fstat` fails (kept, so the parameter costs a syscall rather than
a name, and documented as the arm that cannot fire).

**This is §9's tell**: the portable form came out *stricter* on the platform of record,
not weaker. And the residual is stated rather than papered over — on a kernel whose
slave nodes are not per-allocation directory entries (Darwin's `/dev/ttysNNN` are
persistent devfs nodes) the path is expected to keep resolving after a master close, so
the check degrades there to the borrow plus the tautology. **Expected, not measured**:
this box is Linux (§7). The instrument that would close it portably is `poll(POLLHUP)`,
and this crate cannot issue it — `unsafe` lives only in `serial_nexus_sys` (AGENTS
invariant 3 / §16.3), the same wall `p12_pty_setup` already records for `FIONREAD`. A
doctor probe is the right home for it if it is ever wanted, not a `libc` call smuggled
into the harness.

`impl OpenWitness for std::fs::File` is **removed**, not merely bypassed: a `File`
cannot answer the question, and leaving the impl would let any plain client fd in this
suite be handed in as a witness that proves nothing. The two sites that open with their
own flags (`p12_pty_setup`'s `O_NONBLOCK` session A, `p8_map`'s client-and-witness-in-one
fd) use `adopt_slave(file, path)`, which **verifies** the path names the fd's device at
adoption and panics otherwise — the anti-tautology rule applied to the constructor,
since every later check is a comparison against what it records.

Three guards, fail-first proven by restoring the one-line tautology in place: both
`a_witness_is_refused_once_its_path_names_a_different_device` and
`a_witness_on_a_pty_whose_master_closed_is_refused_though_fstat_still_answers` go red
with `must be refused: ()`. The second **asserts the tautology as a positive control** —
`fstat` still answers, and the fd is still a character device, on a pair that no longer
exists — so the finding is recorded in the suite rather than only in this paragraph.

#### 3. `Sim::prove_open` is an approximation, and now says so

`Child::try_wait` answers "the process has not been reaped", and what is wanted is "the
sim's slave fd is open". The sim closes its port and *then* serializes and prints its
verdict (`sim/src/main.rs`: the mode function owns the port and returns a `Value`, which
`main` prints afterwards), so there is a window where `try_wait` reports the child alive
and the slave is already gone.

Measured against the shipped `serial-nexus-sim client` on this box (debug profile, load
average 0.4): the master saw the last slave close **0.658–0.843 ms** before
`waitpid(WNOHANG)` reported the child, **6 of 6**, with the instrument's own busy-poll
granularity inside that figure — an upper bound on the window, not the window. The doc
comment says that, with the number, instead of "proven live at the instant of the read".

It stays load-bearing for a reason the approximation does not touch: the failure it
exists to catch is a `--hold-ms` that expired *seconds* ago under load, turning a vacuous
pass into a named failure. A sub-millisecond blind spot at the tail does not blunt that,
and the thing it replaced was a bare 20 s timer with no check at all.

#### 4. The borrow forbids relocation, not rewriting

`settled_while_open`'s doc sold the witness borrow as the enforcement. It is, against the
cheap mistake — a moved line — and two rewrites defeat it and compile: re-calling
`attach_slave` below the close and handing in the fresh handle (a witness on a pair the
observation's bytes never went through), and moving the call down while dropping the dead
witness from the slice and keeping a live sibling. Both are what an editor does when the
borrow checker is in the way, which is exactly when the argument is worth reading. The
bound is now stated in the doc comment where the claim is made.

#### 5. An exception whose stated reason was falsified twelve lines away

`p4_purge`'s purge-on-detach check argued its exception with "the counter comes into
existence *because* of the close and reads 0 before it". The counter is `purged`, and the
**next test in the same file**, `pre_grant_backlog_is_purged_on_acquire_and_never_reaches_device`,
watches that same field reach `SB` **while its client is still open**, inside
`settled_while_open`, because purge-on-*acquire* bumps it too.

What is close-only is **this increment's trigger**, not the counter. The distinction is
not pedantry: the old wording licensed the wrong simplification — that `purged` cannot be
read early, when it demonstrably can — and a future editor removing the pre-close
readings on that authority would have deleted the guard's premise.

"Structurally unconvertible" is softened in both exceptions, `p4_purge`'s and
`p12_pty_setup`'s. Both guards already take pre-close observations under
`settled_while_open`; what cannot move above the close is a single post-close
*assertion*, because the edge that moves the number is the close. Saying "the guard is
unconvertible" overstated it in the direction that discourages exactly the tightening
§3.56 was for.

### 3.61 P14's own adversarial pass: five defects, and a read-back that lies

**Design:** §15.51. **Plan:** §18 item 11. **Rules:** AGENTS §9 (an independent
verifier that has not read the diagnosis; record refuted diagnoses), §7 (measure,
do not assume), plan §3 rules 9, 10, 14.

The P14 that landed in §3.57 was handed to a verifier that had not read its
reasoning, with instructions to **refute** rather than confirm. It refuted five
things. All five are fixed, each with an executed fail-first proof. Recorded in
full because a verification that finds nothing teaches nothing, and this one found
the probe's most dangerous sentence.

**(1) The read-back does not answer the question the doc said it answered — and
this is the finding.** `p14_refusal`'s doc claimed the classification was
"corroborated by a number rather than resting on this rule alone", and
`p14_apply_rate`'s said it reported "what the driver is actually running". Both
are false on the platform of record. Measured over the bench crossover, timing the
payloads rather than trusting `tcgetattr`:

| requested | reads back | achieved (timed) | ratio |
|---|---|---|---|
| 115200 | 115200 | 109740 | 0.95 |
| 921600 | 921600 | 869963 | 0.94 |
| 1500000 | 1500000 | 1414231 | 0.94 |
| 2000000 | 2000000 | 1879497 | 0.94 |
| **2500000** | **2500000** | **1903015** | **0.76** |
| **2750000** | **2750000** | **1911323** | **0.70** |
| 3000000 | 3000000 | 2795935 | 0.93 |

A steady ~0.94 is this instrument's own overhead. The two outliers are not
overhead: the FT232R is rounding both asks to its nearest supported divisor —
2 Mbaud — and `ftdi_sio` is echoing the **requested** number back. The bytes still
round-trip byte-exact, because both ends are mis-set *identically* and therefore
agree with each other.

So `max_reliable_baud` is **the highest rate that round-trips when both ends are
asked for it**, which is exactly what an operator configuring a port gets — and it
is not necessarily the rate on the wire. Each direction now carries
`achieved_baud_floor`, timed from its fastest clean trial (a floor: this probe's
poll loop can only push it down), and the report's own
`ceiling_is_a_floor_over` sentence says the number is a *requested* rate and tells
the reader to compare. The read-back keeps its job in the `adapter-refused` arm,
where the driver really does answer with a different number — 4000000 reads back
9600 — and is now documented as saying nothing about a rate it accepted.

**(2) A vanished peer printed as a kernel fact.** An adapter unplugged during a
rate change answers `EIO`, which carries an errno, so the discriminator called it
`platform-refused` — and the verdict then told the operator that *this kernel's ask
surface* stops here and a different kernel might ask for more over the same cable.
The cable was gone. `is_hangup` is the predicate P5 already uses for exactly this
distinction and `p14_refusal` never consulted it; it does now, and
`CeilingKind::HungUp` **degrades** rather than reporting a ceiling — which is what
`RungOutcome::HungUp`'s own doc comment ("Not a ceiling") had been asserting while
the fold contradicted it.

**(3) A stall on trial 2 or 3 was folded into a loss**, which §6 forbids. The
classification lived in the rung fold and compared *totals over up to three trials*
against a *single* payload length. Because a direction short-circuits on its first
failure, an intermittent rung failing on trial 2 or 3 — the shape a marginal rate
actually produces — had already banked one or two clean payloads, so a total stall
on trial 3 read `received == 2 x payload`: neither short nor starved, therefore
`Corrupt`. The identical failure on trial 1 classified correctly. The bug was a
sum compared against a unit. `TrialResult::failure` now classifies each trial
against its own payload, while that payload is still in scope.

**(4) The wall-clock budget was reported and never folded.** `P14_BUDGET`'s comment
promised "the fold refuses to dress up an incomplete search as a measured ceiling",
and `p14_verdict` could not see the flag: a budget expiring straight after a
*failing* rung, with up to four refinements still owed, left a bounded floor and a
named kind, so the verdict read `supported` over a search that had stopped early.
The `ceiling_kind.is_none()` arm only ever covered the case where *no* failure
bounds the floor. `search_budget_exhausted` is a fact now and degrades.

**(5) Two smaller ones.** `outcome` and `refusal_errno` were taken from two
independent `or`s, so a mixed-adapter rig could print `adapter-refused` beside a
non-null errno — the pairing the discriminator declares impossible, in the
artifact, for a reader to trip over; they come from the same port now. And the
counter deltas subtracted `i32` fields cast to `u64`, so a driver that reset a
counter between the two reads would underflow (a debug panic, ~1.8e19 in release);
they saturate.

**Two claims the verifier corrected without finding a bug, and both are now stated
in the code.** The non-monotone guard defends against a history **the probe cannot
produce** — the climb stops at its first failure and a refinement rate is always
below the failure that bracketed it, so the floor can never rise above a failure.
What that guard actually protects is the *fold's totality*, which is what would let
a future probe walk one rung past a failure without touching the pure core; the
test says so now, so nobody reads its green as evidence that the search handles a
non-monotone rig today. And the "two independent gates" keeping P14 off a software
null modem are **one predicate read twice**: `baseline_ok` is computed only inside
the `a_uart && b_uart` branch, so a pts fails the second gate *because* it failed
the first. The real protection is `p5_is_uart`, measured here on Linux (a pts
answers `ENOTTY` to both ioctls, on the slave and on the master) and unverified
from this box on Darwin — which is the single point of failure, and is named
rather than assumed.

**Fail-first, executed, three reverts.** The `is_hangup` consultation removed, the
per-trial classification collapsed back to "everything is a loss", and the budget
arm short-circuited: each reddened its own guard, and the tree was restored green
after each.

### 3.62 §18 item 3, discriminated: H1 refuted, the correlate named, and the signature is exactly 64 bytes

**Supersedes the reading of §3.58, does not replace its runs.** §3.58 recorded four
full rig-attached runs with 0 failures and said plainly why that refuted nothing:
the replug lane was not running, because the blessed helper read `Stale`. It was
re-blessed, and the picture changed completely.

**One correction to §3.58 first.** The helper was blessed the whole time — the
*install* never moved (still dated 12:40, byte-identical to the build). What moved
was `target/debug/serial-nexus-devprep`: a rebuild drifted it and a later rebuild
drifted it back, which is the reproducibility §3.54 measured and the reason the
staleness check warns rather than fails. §3.58's runs genuinely had no replug lane
— `SNX_REPLUG_DEV` was unset — so its evidence stands; only its explanation of
*why* it could not run does not.

**The measurements.** Same box, same commit, load 0.21–0.93 throughout.

| lane | shape | runs | failing |
|---|---|---|---|
| replug **absent** (`SNX_REPLUG*` unset, tests self-skip, binary still built) | default threads | 3 | **0** |
| replug **absent** | `--test-threads=1` | 1 | **0** |
| replug **green** | default threads | 3 | **2** |
| replug **green** | `--test-threads=1` | 2 | **1** |

**H1 — "the suite got longer" — is refuted as the mechanism.** The failure
reproduces under `--test-threads=1`, where there is no concurrency left to blame,
and it does *not* reproduce in a run of the same length whose only difference is
that the replug tests skipped instead of cycling the USB devices. Serialization was
the arm §3.54 pre-registered as discriminating, and it discriminated.

**What correlates is the replug lane executing**: 3 failing of 5 with it, 0 of 4
without, on top of §3.54's own 2 of 5 with and 0 of 5 before the replug work
existed. Combined: **5 of 10 with, 0 of 9 without.**

**Two things §3.54's framing got wrong, both now measured.**

1. **It is not "the same two tests".** The third failure is
   `serial_hardware::crossover_rig_data_plane_send_and_exclusivity` — a test §3.54
   never names — failing with the *identical* signature. So this is not two
   timing-sensitive tests being flaky; it is `serial_hardware`'s `inject_verify`
   path failing whenever the replug lane has run, and which of its four tests draws
   the short straw is incidental.
2. **The signature is exact and it repeats.** All three failures read
   `only 32704/32768 B crossed the wire` — **exactly 64 bytes short**, three times
   out of three, at 64.6–67.7 s against a usual ~10.4 s. A round 64 is not the
   shape of a marginal wire; it is one FTDI bulk packet. The long duration is the
   harness waiting out its deadline for bytes that never arrive, not a slow
   transfer.

**Mechanism not established, and no root cause is claimed.** A whole USB packet
going missing after a deauthorize/reauthorize cycle has several readings — a stale
URB, a device-side FIFO not drained across the cycle, a driver-side buffer boundary
— and this session separated none of them. What is established is which variable
moves the outcome, which is what the item asked for.

**One diagnosis this session's own work is exonerated of.** `serial_hardware.rs`'s
`inject_verify` was converted to `settled_while_open` earlier the same day
(§3.56), which made it a candidate cause. The replug-absent arms ran the *same
converted code* and failed 0 of 4, so the conversion is not the cause. Recorded
because §9 makes a refuted diagnosis as load-bearing as a confirmed one, and this
one was mine.

**What the next session should do, and it is now a narrow question.** Instrument
the 64 bytes rather than the suite: capture where in the 32768 the gap falls (head,
tail, or interior), and compare a run whose replug hold is a no-op — authorize
immediately, no down time — against one with the normal hold. Same binary count,
same suite length, same test names; only the USB cycle differs. That is the
experiment §3.58 proposed before any of this was measured, and it is now the only
one left worth running.

### 3.63 Hardware flow control, end to end — and a fail-first proof that never applied

**Design:** §5's transparency promise, §15.52 (the measured precondition).
**Plan:** §18 item 7, the half §15.52 explicitly left open. **Rules:** plan §3
rules 10, 11; AGENTS §9.

**What was missing.** `flow_control = "rts-cts"` is a shipped configuration
attribute. Its serde path has been proven since it shipped — spellings, kebab
canonicalization, the proptest strategy, the CFG-1 validation guard — and **no test
in this workspace had ever opened a port with it**. §5 states the promise as
transparency: "the kernel pausing transmission surfaces as Busy, so hardware flow
control transparently extends across the graph to remote writers." That sentence
had a config path and no wire path.

**Why the obvious test cannot be written.** The natural shape is "stall the
consumer and watch flow control engage" — and it is unwritable against this design.
§5 has the daemon reading its own port *continuously and never idling*, so a
receiver inside the graph can never fill its buffer and its kernel never has a
reason to drop RTS. The stall has to come from outside the graph.

**The shape that works.** The peer runs `flow_control = "none"`, which leaves its
RTS pin under `set-modem`'s control, so the *test* owns it; the port under test runs
`rts-cts`, so its kernel watches CTS. Dropping the peer's RTS takes the transmitter's
CTS low through the 5-wire crossover §15.52 measured, and the kernel holds the line.

**Measured on the rig, and the numbers are the reason the guard is not vacuous:**

| transmitter | CTS held low | payload appears |
|---|---|---|
| `flow_control = "none"` | yes | **25 ms** |
| `flow_control = "rts-cts"` | yes | **never** (6 s window) |
| either | released | immediately |

So the mechanism is real, and a 1.5 s stall window is a **60x margin** over the only
behaviour that could produce a false green. Two facts are asserted together, because
either alone is satisfiable by a bug: the bytes do not arrive while CTS is low (the
stall is real, not a slow path), and they arrive **byte-exact** when it rises (the
stall cost latency, not data — §5's "commands are delayed, never lost"). The control
arm runs every time and re-establishes the 25 ms figure rather than trusting the
table above.

**A second test, and it is not the doctor's.** `crossover_rig_rts_crosses_to_the_far_ports_cts`
asserts continuity through the daemon's own `state` — the `modem_lines` field §7.1
promises an operator — with both polarities and both directions. P5's handshake block
(§15.52) measures the same wire port-to-port with no daemon in the path, because §13's
boundary is that the doctor certifies the rig and never drives the daemon through it.
**Neither subsumes the other**: the doctor's arm would still pass if the daemon's state
reporting were broken, and this one would still pass if the doctor's were. The DTR arm
is the negative control, unchanged in purpose from §15.52's.

**A fourth `required` spelling, and the first with a measured precondition.**
`SNX_RIG_FLOW=required`. The other three ask whether a *thing* is present — a rig, a
blessed helper, `curl`. This one asks whether the rig that is present carries RTS↔CTS,
which is the operator's cabling; §5's stated assumption is the **3-wire** link, so a
bench that answers no is legitimate and `SNX_CROSSOVER=required` deliberately does not
redden it. The measurement is taken first and printed in the skip, so a reader never
guesses which bench they have. Both arms run: forced 3-wire, the tests skip green
without the variable and fail naming the measurement with it.

**The fail-first proof that never applied, recorded because it is the failure mode a
fail-first proof is supposed to have.** The first attempt mutated the transmitter to
`flow_control = "none"` with a `sed` whose pattern assumed the call site was on one
line. `rustfmt` had wrapped it. The `sed` matched nothing, the unmutated test ran, it
passed — and that pass reads *exactly* like "the mutation did not redden the guard",
which would have been a defect finding about the guard rather than about the
experiment. It was caught only because the result was implausible: the same
configuration passed in one arm and failed in another. Redone with the replacement
count asserted before the run, the mutation reddens with its own message verbatim.
**A mutation whose application is not verified is not a control**, and `sed` over
formatted Rust is where that bites.

### 3.64 Making the 6.18 visit worth taking: a gate hole I shipped, and the reference count nothing measured

**Plan:** §18 item 8 (the owed 6.18 visit), item 7's remainder. **Design:** §7,
§13, §15.44, §15.51. **Rules:** plan §3 rules 9, 10, 14; AGENTS §7, §9.

The production kernel is 6.18 and this box is 7.0. One visit is available and it
cannot be iterated on, so the doctor was audited against a single question: *what
would that visit come home without?* Four things landed as a result, and one of
them is a defect this session shipped three commits earlier.

**(1) The P14 gate clause rejected a legitimate `degraded` report — verified by
running the gate, not predicted.** `expectations/*.jq` exempts `skipped` and
requires `max_reliable_baud`, `ceiling_kind` and `ceiling_is_a_floor_over`
otherwise. Two of `p14_verdict`'s six `degraded` arms return **before** the
observation block — a pair whose P5 rate ladder did not round-trip, and a pair that
would not reopen — so they produced a `degraded` verdict carrying none of those
cells, and the gate refused it. **A rig whose cable seated a millimetre differently
after the drive would have turned the first ever executed 6.18 jq gate red for a
reason that is not a defect**, which is precisely what the `degraded` arm exists to
prevent.

The cells are now stamped up front on every path, `null` where the search did not
run — admissible by design, because the clause tests `has` and never a type or a
value. **Two guards, because the hole needed both halves.** The one in
`expectation_gates.rs` proves the *clause* admits the shape and that the repair did
not loosen it (the same `degraded` with the cells gone is still refused); the one in
`probes.rs` proves the *probe* produces it, and is the half that reddens when the
stamp is removed — reaching the arm with no hardware, via a `VerifiedPair` whose
`baseline_ok` is false. The original matcher proof missed this because it planted
only against `supported` and against the honest *passive* report. **`degraded` was a
third spelling, and it is the one a real rig produces** (plan §3 rule 10: plant the
violation in every spelling the clause claims to cover).

**(2) The last-close reference count is measured now, and it was measured by
nothing.** notes §3.56 converted seven guards to hold a harness-opened slave fd
across the observation, on the argument that *every kernel attaches its close-time
work to the **last** close*. P13 had exactly three shapes and **every one used a
single slave fd**, so none of them could see a reference count at all. The
architecture of an entire harness rule rested on two kernels' source read by eye.

`d_no_reader_second_fd_held` holds a second fd on the same pts across the writer's
close and releases it only after the master has drained. On Linux 7.0.0-29 it is
**not inert**, and the discriminator is not the one that was expected: the byte
counts agree (64 of 64 either way — Linux retains), and the **terminal read** moves,
`EIO` with one fd against `EAGAIN` with the witness held. The hangup itself is
deferred to the reference-count edge. On a kernel that discards at last close the
byte counts must move instead, which is what a Darwin run shows.

> **Annotated 2026-08-07 (§3.73): the "or 6.18" half of that last sentence was
> falsified by measurement and has been struck from it.** A Tier-3 6.18.14 capture
> at `3e23c52` — the same doctor source as the committed `7cf0338` 7.0 triple, both
> digests equal — reads `policy: retains`, with `a_no_reader_blocking_slave` and
> `d_no_reader_second_fd_held` both recovering 64 of 64. The byte counts do **not**
> move on 6.18; only `terminal_read` does, `EIO` against `EAGAIN`, exactly as on
> 7.0. The Darwin half held. This was a forward-looking expectation, **not** a
> pre-registered falsifier — §3.64 registers none, and the word is reserved here for
> formal registrations. Nothing downstream depended on it: the guard
> `the_last_close_reference_count_shape_is_not_inert_here` gates its strict branch
> on `bare_bytes < P13_PAYLOAD`, false on 6.18, and falls through to the portable
> `bytes_differ || terminal_differs`, which the `EIO`/`EAGAIN` move satisfies. What
> 6.18 does confirm is §3.56's premise — close-time work attaches to the last close.
> What stays single-kernel is the *strong* form: the witness fd recovering bytes
> that would otherwise be lost still rests on Darwin alone.

The guard asserts *something* differs rather than *which* cell — that is what keeps
it portable, and it is still a real assertion, because an implementation that opened
no second fd moves nothing. Fail-first: releasing the witness **before** the drain
instead of after collapses the shape into `a_no_reader_blocking_slave` and reddens it
with `same bytes (64 vs 64), same terminal (EIO vs EIO)`. That mutation is stronger
than "never open the witness", because it tests the semantic property — held
*across* the observation — rather than the existence of an fd.

**(3) A tripwire is documented as a finding rather than softened.**
`p10_fionread_is_trustworthy_on_the_platform_of_record` is Linux-gated, calibrated
on 7.0.0-29, and will run on 6.18 — where its own name refers to a kernel the design
calls the platform of record and every figure behind it was measured elsewhere. It
is **not** relaxed. §7's "a differing kernel is `degraded` with the observation
named" is discharged by the *probe*, which reports `peer_pending_input_trust` as data
and degrades properly; a test's job is to be loud. Its doc now says what a red there
invalidates — exactly one thing, notes §3.45 (iv)'s reading of Darwin's
`contradicted-empty` as a Darwin fault rather than a general one — and what it does
not: no shipped behaviour, since the daemon never consumes the field.

**(4) The protocol is written down**, in `docs/serial-nexus-doctor.md`. The last
6.18 capture was Markdown, so its field set is *not computable* and it is two
fingerprint eras behind; it is diffable against nothing this visit will produce. The
section covers what to do before leaving (commit, or every artifact stamps `-dirty`
and the same-binary claim becomes unprovable; take the counterpart triples at the
same commit; warm both cargo caches, because `p8_external_codec` shells out to a
second lock graph with no skip path), what to run there, and — the part that is easy
to get backwards — that `SNX_CROSSOVER_A`/`_B` take plain `/dev/tty*` paths while
`SNX_REPLUG_DEV` takes a by-id path and hard-fails on anything else.

**One thing the audit proposed and this session declined**, recorded so it is not
mistaken for an oversight: extending P10's drain ladder with a rung above capacity.
The named open item is that no watermark bound is recoverable, and the ladder's
smallest rung already drains **one byte**, leaving the highest occupancy the
instrument can produce — a *larger* drain leaves a *lower* occupancy and is therefore
even more likely to be accepted, so it cannot produce the refusal the bound needs.
Either the open item is misstated or the mechanism is the reverse of the obvious
reading, and changing a probe on a mechanism this session does not understand is how
a measurement becomes wrong rather than absent. Left open, with the reasoning
recorded (§5).

### 3.65 The Darwin run of the P14 era: three guards that asserted Linux's answer, and two platform gaps

**Design:** §7 — no one-way decision on single-kernel evidence; a kernel that differs is
`degraded` with the observation named. §9 — a guard asserts the property, never a proxy for it
in space.

`main` reached `60b9d0f` with nineteen commits since `f8315cc`, all developed and measured on
Linux: P14, the replug capability, hardware flow control, the v15 document generation, and P13's
fourth shape. This entry is the Darwin run of that tree, on macOS 15.7.8 / Darwin 24.6.0 x86_64
with the FT232R crossover attached and `SNX_CROSSOVER=required`. **Four tests failed and the
workspace did not build.** Three of the four were the same defect wearing different clothes — a
guard that pins the platform of record's *answer* instead of the property — and the fourth plus
the build break are genuine platform gaps.

**A. `cargo build --workspace --locked` does not build on macOS, and the macOS CI lane's first
step is exactly that command.** `serial-nexus-devprep` is an unconditional workspace member whose
`main.rs` opens with `use serial_nexus_sys::caps::{self, CAP_DAC_OVERRIDE, CapState}`, and that
module is `#[cfg(target_os = "linux")]`. The intent is already written down one file away — sys's
own comment says the module is "Linux-only … and the helper the module supports does not build
elsewhere" — so what is missing is not the decision but its enforcement. `cargo build --workspace
--exclude serial-nexus-devprep` is clean, which locates the break precisely. The reason it went unnoticed is worth more than the patch: the replug
commits landed after the last macOS run, and nothing between them and this session executed a
build off Linux. **Fixed** (see A′ below, which also answers whether a macOS equivalent exists).

**B. P9's mask premise, refuted on the kernel that has to live with it.** `f8315cc` collapsed
P9's reference table to a `1x2` on the premise that POSIX delivers `POLLHUP` whether or not it was
requested, so the mask cells are replicates. Darwin does not: an empty mask on a hung-up master
returns `revents: none` at ~14000 ns while a `POLLHUP`-requesting poll on the *same fd through the
same wrapper* returns `POLLHUP` at ~1270 ns, and `serial_nexus_sys::poll_blocking` returns
`revents` unmasked, which is the property the probe's own comment says would make the cell vacuous
if violated. `an_unrequested_hangup_is_still_delivered` failed **5 of 5**, deterministically,
printing the message its author wrote for exactly this case. The report meanwhile asserted the
refuted premise in three unconditional places — `shape: "1x2"`, the `mask_is_a_control_not_a_factor`
rationale, and a consequence sentence claiming the field "says so on this kernel" — printed
byte-identically beside that field reading `false`.

Repaired by deriving the framing from the measurement: `p9_mask_axis` is pure and returns the
shape, the `mask_role` prose, and **which** separation and mask-spread figures belong together;
the observation gains `worst_case_separation_requesting_masks_x100`,
`mask_spread_{not_,}ready_requesting_x100`, `read_the_separation_from` and
`read_the_mask_spread_from`. The guard now asserts the portable property and carries the control
that makes an empty reading interpretable at all: whatever the empty mask says, a mask that
*asked* must see the hangup, or the fd never hung up and nothing below measures a mask. A second
pure test exercises both arms, so the reading that does not run on the box in front of you is
still tested.

**And the repair reverses the first reading of the data, which is why it is worth the fields.**
Folding the empty-mask cell into the ready group makes Darwin read ~1.0x by fd state against a
mask spread of ~8-11x, failing P9's own survival criterion and looking like a cross-kernel
contradiction. Among the cells that *requested* something Darwin separates **7.02-9.92x by fd
state against 1.09-1.12x by mask** — the shape Linux reports (7.46-10.12x against 0.968-1.314x).
The empty mask is a third *kernel state*, not a third mask level. §3.41's "undecomposed" is closed
on both kernels, **agreeing**.

**C. P13's fourth shape asserted `retains` as a precondition, and Darwin discards.** §3.64 added
`d_no_reader_second_fd_held` and a guard whose *second* assertion was deliberately portable
("something differs" rather than "the terminal differs"). Its *first* was not: it asserted the
bare no-reader shape recovers the whole payload, which is what a **retaining** kernel does. Darwin
`waits-then-discards` and recovers 0 of 64, so `the_last_close_reference_count_shape_is_not_inert_here`
failed on the one platform whose §3.56 conversions the witness fd was bought for — with a message
that anticipated it exactly ("if this kernel does not, the reading below changes").

**The measurement is the good news, and it is stronger than the guard was asking for.** On Darwin
24.6.0: `a_no_reader_blocking_slave` recovers **0 of 64** with a 600060 us close and terminal
`eof`; `d_no_reader_second_fd_held` recovers **64 of 64** with a 12 us close and terminal
`EAGAIN`. So holding a second fd across the writer's close does not merely change a cell here — it
**recovers every byte that would otherwise be lost**, and collapses the close from 600 ms to 12
us. On Linux the witness only moves the terminal, because the bytes were never at risk. §3.56's
argument is therefore not just intact off Linux; it is load-bearing there and it works.

The guard now reads the disposition off the bare shape and asserts the consequence belonging to
it, which on a discarding kernel is **stricter** rather than weaker: the witness must recover the
whole payload, not merely differ. That is §9's tell that the portable form was found — it comes
out harder on the platform of record, not softer.

**D. §3.47's repair had a second unfixed sibling, and the Darwin rig is where it showed.**
`p4_exclusivity::exclusive_write_lock_is_byte_exact` spawns its sink in a thread and lets the
holder send immediately, under a comment stating the ordering intent it does not implement.
§3.47 established the mechanism in `p4_free_for_all` — a USB-serial port that is not open does not
receive, so every byte clocked onto the wire before the far end opens is gone, 21-31 bytes for
process start-up alone — and fixed it there with a `--open-file` handshake. This sibling never got
it. The same `--open-file` gate applied here: **3 of 3 fail unfixed at 32.2 s, 3 of 3 pass fixed
at 8.5 s**, and the 32.2 s is itself the evidence — the sink blocks to its own 30 s timeout waiting
for bytes that were never going to arrive, which outlives the held child's 20 s hold and trips
§3.56's `observed_while_open` witness. So on `main` the symptom had *moved* (the failure now names
the closed-slave hazard rather than a short byte count) while the cause had not.

**E. Hardware flow control is unavailable on this platform, and the driver says so only by
read-back.** `rts_cts_flow_control_stalls_the_writer_instead_of_losing_bytes` (plan §18 item 7,
`ebf9c52`) fails on Darwin: the node goes `faulted` with `reopen …: failed to apply some or all
settings`, which is `serial2`'s verification error — it sets the termios, reads it back, and finds
the request absent.

Measured below the daemon, because §7 wants the kernel measured rather than inferred from a
harness failure. Opening `/dev/cu.usbserial-BH00L4KU` directly and setting `CRTSCTS`: `tcsetattr`
returns **success** and `tcgetattr` reads the flag back **clear**, every time. The obvious
alternatives are refuted rather than argued — `CCTS_OFLOW` alone, `CRTS_IFLOW` alone, both
together, a blocking open, and the `/dev/tty.*` dial-in node instead of the `/dev/cu.*` callout
node all behave identically (the `cu`-bypasses-modem-control hypothesis is the one that looked
most likely and is dead). The **control** localises it: a *pty* slave on the same box accepts
`CRTSCTS` and reads it back set. So this is not macOS's tty layer — it is the serial driver, and
`ioreg` names it: Apple's own `IOSerialFamily` / `com.apple.driver.driverkit.serial`, with no FTDI
kext loaded. The adapter's RTS/CTS *wiring* is fine and independently proven on this same rig —
`crossover_rig_rts_crosses_to_the_far_ports_cts` passes, driving RTS on either port raises CTS on
the other.

The product question is stated rather than answered: §7 says a kernel that differs is
`degraded` with the observation named, never a hard failure, and today a `rts-cts` config **faults
the node** on Darwin. Whether the daemon should degrade with the observation named, or keep
faulting because silently running a flow-controlled link without flow control is worse, is a
design decision (§5) and not one to settle from the test that noticed it. **What this session did
build is the instrument that decision needs — see E′ below.**

**F. One failure was contention, not a defect, and it is recorded so it is not re-filed.**
`crossover_rig_map_node_both_directions` fails when `serial_hardware` runs its tests in parallel
and passes with `--test-threads=1` — the binary's six tests now share two physical ports, and the
`rts-cts` test above faults one of them mid-run. The rig tests were four when that binary was last
green here and are six now.

**The gate on this box, and what it is scoped to.** 759 passing, 2 failed, 4 ignored, zero SKIP
lines, `--no-fail-fast`, `--exclude serial-nexus-web --exclude serial-nexus-devprep`; the two
failures are E above and the citation gate that this entry closes. Before the three repairs it was
756 / 4 / 4. Keep the `--exclude serial-nexus-devprep` visible when quoting the figure: it is not
the documented macOS scope, it is A above, and the number is smaller than it looks by however many
tests that crate carries on Linux.

**Addendum (same session): the P14-era Darwin triple.** `docs/doctor/macos-24.6.0-2026-08-05-42eac2a-tier3{,-2,-3}.json`,
taken at `42eac2a` from a clean tree, `probe_set 94d64d8bbacf1174` — the same digest as the
`3d850cf` Linux triple, so that era now has a lawful cross-kernel diff and the counterpart the
6.18 visit was prepared against exists on a second kernel. Three sequential runs, load 1.30–2.12,
one `field_set` (`da21ac7678aeebaa`) across all three, all three passing `expectations/macos.jq`.
A Tier-3 run costs **50.1 s** here against Linux's 35.0 s, essentially all of it P14.

**P14's first Darwin reading agrees on the number and disagrees on the cause.**
`max_reliable_baud: 3000000` and `first_unreliable_baud: 3062500`, identical to Linux and
identical across all three runs — but `ceiling_kind` reads **`platform-refused`** here against
**`adapter-refused`** there. The same physical limit, attributed to different layers by the two
kernels. That is the distinction §15.51 built the field for, and it is the first time the two
readings have existed side by side.

**P13's fourth shape agrees across kernels while the policy disagrees, which is the point.**
`d_no_reader_second_fd_held` reads `bytes_recovered_total: 64` with terminal `EAGAIN` on *both*
kernels, while `policy` is `waits-then-discards` on Darwin and `retains` on Linux, and the bare
shape differs 0-of-64 against 64-of-64. So the witness fd **normalises two opposite last-close
dispositions to one reading**. That is notes §3.56's argument in its measured form: the seven
guards it converted do not depend on which disposition the kernel has, which is exactly what a
portable guard needs and what C above found the *guard* was not yet saying.

**A′ (same session) — the build break is fixed, and the macOS equivalent was investigated rather
than assumed away.** `devprep/src/{main,install,sysfs}.rs` become
`devprep/src/main.rs` (a platform dispatcher) plus `devprep/src/linux/{mod,install,sysfs}.rs`
reached through `#[cfg(target_os = "linux")] #[path = "linux/mod.rs"] mod platform;`. Off Linux
the binary refuses in one place with exit **2** — the Linux binary's "refused" code, so a harness
that shells out gets a verdict rather than an ambiguity — and `cargo build --workspace --locked`
now succeeds on macOS, so the documented gate scope no longer needs a second exclusion. The
restructure was **cross-checked, not eyeballed**: `cargo check --target x86_64-unknown-linux-gnu
--workspace --exclude serial-nexus-web --all-targets` is clean, which covers every Linux-only path
this session edited, and `--exclude serial-nexus-web` there is `ring`'s C build script wanting a
Linux cross-compiler, not a defect (`cargo tree -i ring` names `serial-nexus-web` as the only
consumer).

**A macOS equivalent exists, and the measurement is the design input.** IOUSBLib's
`USBDeviceReEnumerate` is reachable through the `IOCFPlugInTypes` handle the FT232R already
advertises in the IORegistry. Measured on Darwin 24.6.0 / macOS 15.7.8 against `BH00L4KU`:
`USBDeviceOpen` then `USBDeviceReEnumerate` both return `kIOReturnSuccess`, the target's
`sessionID` moves `26843088327954` → `26857977258534` **while an untouched second adapter's
(`BH00LL8O`) does not**, and `/dev/cu.usbserial-BH00L4KU` returns at the same path. That control is
what makes it a re-enumeration of *the named device* rather than a bus-wide event.

**It is a partial equivalent, and both differences are measured rather than argued.** First, it
needs **no privilege** — the open above succeeded at `euid=501`, where the whole Linux helper
exists to carry `CAP_DAC_OVERRIDE` for one sysfs write — so `capabilities`, `install` and
`preflight` are Linux concepts with nothing to port, and §15.45's security argument does not
transfer. Second, and the reason a backend is not a drop-in: Linux's mechanism is **two-step and
holdable** (deauthorize, wait `--hold-ms`, authorize) while `USBDeviceReEnumerate` is **atomic**.
The outage it produces measured **40, 41 and 42 ms** across three trials at 2 ms sampling, and
nothing in the API lengthens it. So `cycle --hold-ms` cannot be honoured off Linux and `hold`
cannot be implemented at all; what a macOS backend can offer is the replug *event*, not a
controllable outage.

Landing that backend is therefore a **design amendment** (§15.45 is written around a privileged
Linux helper), not a patch, and it is left for one: this session stops at the boundary where the
crate builds everywhere and the refusal carries the measurement. Recorded here so the next session
does not re-derive it — the expensive part was establishing that the primitive exists, is
unprivileged, and is 41 ms wide.

**E′ (same session) — P15, so the flow-control question is decided against an artifact.**
A new probe rather than an observation on P11, for a reason P11 states about itself: it opens with
the port's settings **unchanged** and "inspects, it does not configure", and this question cannot
be asked without configuring. A new question takes a new id under the append-only rule, so
`probe_set` moves to a new era (`94d64d8bbacf1174` → `82a8e2198e54626a`) — the same deliberate,
sound direction P14 took, and it means the `42eac2a`/`3d850cf` cross-kernel pair remains the pair
for *its* era rather than being silently compared across the boundary.

P15 asks each `--port` for `CRTSCTS`, reads it back, and puts the termios back as it found it.
Per port it reports `tcsetattr_ok`, `honoured_on_readback`, `silently_dropped`, the before/after
`cflag`, and `baseline_restored` — that last one required by both gate files, because a probe that
reconfigures a real adapter and cannot say it restored it is worse than one that never ran. On the
Darwin rig it reads `degraded` with `silently_dropped: true` on both ports, `tcsetattr_ok: true`,
`cflag` unchanged at `0x4b00` either side, and `baseline_restored: true`.

**The verdict distinguishes three cases, and only one of them is the defect.** A driver that
*refuses* the request is honest and reads `supported`; a driver that honours it reads `supported`;
only accept-then-drop degrades. That distinction is tested purely, both arms, because Linux
honours the flag and Darwin drops it — so on either box alone half the function is unreachable,
which is exactly the shape §9 says not to leave to whichever kernel is plugged in. A failed
restore outranks the finding itself and is reported first.

Both gate files require the per-port reading and the restore by **presence and type, never
answer**: flipping `honoured_on_readback` to `true` keeps the gate green, because "this driver
honours it" and "this driver drops it" are both legitimate kernel facts (§7). Proven by mutation
against a live report — removing P15, the reading, or the restore each reds the gate; flipping the
answer does not — and the passive path was checked too, where P15 skips and both gates stay green.

**What P15 does not do is answer the design question**, and that is deliberate. `rts_cts_flow_control_stalls_the_writer_instead_of_losing_bytes`
is still red on Darwin, and it should stay red until the fault-versus-degrade decision is made,
because a green suite would be the wrong summary of a platform that cannot do what the config asks.

### 3.66 The macOS replug backend: the equivalent that exists, built as far as it goes

**Design:** §15.45 (the replug helper), §16.3 (`unsafe` lives in `serial_nexus_sys` and
nowhere else), §7 (a platform that differs is reported, never silently approximated).

§3.65 A′ established that a macOS equivalent of Linux's sysfs `authorized` replug **exists** and
measured where the equivalence stops. This entry builds it. The result is a working
`serial-nexus-devprep` on Darwin with four of six verbs doing real work, one refusing on purpose,
and one reporting that it has nothing to do.

**The mechanism, and where the `unsafe` went.** `serial_nexus_sys::usb_macos` wraps IOUSBLib's
`USBDeviceReEnumerate` — the whole FFI surface, because §16.3 permits `unsafe` in exactly one
crate and a platform port is not a reason to widen that. **No new crates.io dependency**: IOKit
and CoreFoundation are linked as system frameworks with `#[link(kind = "framework")]`, so
`cargo deny check licenses bans sources` does not move, which matters because the replug helper's
dependency list is part of its security argument.

**Two registry nodes describe one device, and confusing them is silent.** `IOUSBHostDevice` is
the modern node and the one the serial driver's `IOSerialBSDClient` — and therefore
`IOCalloutDevice` — hangs beneath. `IOUSBDevice` is the legacy shim carrying the `IOCFPlugInTypes`
handle IOUSBLib needs, with no serial client under it. Enumeration reads the host node; the
re-enumeration matches the legacy node for the plugin; both key on the serial number. An earlier
draft read the callout from the legacy node and reported **no ttys at all**, cleanly and wrongly —
recorded because the failure had no error attached to it.

**The vtable is proven before anything destructive runs.** IOUSBLib is a COM-style plugin: the
interface is a pointer to a pointer to a table of function pointers, and calling the wrong slot is
calling an arbitrary function with a device open. The slot indices were **printed from the SDK
headers with `offsetof` on this machine**, not recalled — `USBDeviceReEnumerate` is slot 37 of 51,
`USBDeviceOpen` 8, `GetDeviceVendor` 13 — as were the three `CFUUIDBytes` constants. A number
correct today can still be wrong on a future macOS, so `reenumerate` calls the **harmless** slot
first: `GetDeviceVendor`, whose answer is already known from a CF property read by an entirely
different route. If the table were misaligned the two would disagree and the function refuses
without touching the bus. That is §9's discipline applied to an FFI boundary — prove the
instrument, then trust the measurement — and on this rig the check reads `0403` and matches.

**What the verbs do here.**

| verb | Darwin |
|---|---|
| `cycle` | Re-enumerates, then **measures** the outage and reports `session_id` either side |
| `status` | Identity, `session_id`, and the callout node |
| `preflight` | Ready iff a **serial adapter** is attached, not merely a device with a serial |
| `capabilities` | Reports that none is required, with the reason |
| `authorize` | Nothing to repair: there is no deauthorized state to be stuck in |
| `install` | Nothing to install: the mechanism is unprivileged |
| `hold` | **Refused**, exit 3 |

**`--hold-ms` is reported unhonoured rather than quietly dropped.** The Linux mechanism is
two-step and holdable; this one is atomic. `cycle` therefore emits `hold_ms_requested` beside
`hold_ms_honoured: false` and `hold_unsupported_because`, and prints a warning on the prose path.
A helper that silently turned a requested 1500 ms hold into 41 ms would hand a test a window it
never had, which is worse than refusing. `hold` — whose entire purpose is a caller-controlled
window — refuses outright rather than approximating one.

**`session_id` is Darwin's `devnum`, and it is the field that makes a replug provable.** Linux's
helper reports `devnum` because the kernel reissues it on every enumeration, separating a real
replug from a driver rebind. Measured here across a `USBDeviceReEnumerate`: the target's
`sessionID` moves and an untouched adapter's on the same bus does **not** — which is what makes it
a witness for *this* device rather than for bus activity. `cycle` reports
`session_id_changed` rather than leaving a reader to compare two large integers.

**`preflight` counts serial adapters, not devices with serial numbers.** Fourteen USB devices on
this machine name a serial — the Touch Bar, the camera — and counting those would report *ready*
on a box with no adapter attached, which is the one answer a caller uses to decide whether to
skip. Requiring a callout node makes the evidence "a serial driver attached", and on this box that
is 2 of 14.

**A stranded adapter is a failure, and that distinction cost an enum.** Watching the callout node
has three outcomes, not two: no node to watch (a device with no serial driver — replugging it
still succeeded), a node that left and returned (the number), and a node that left and **did not
come back** inside the window. Only the third is a failure, and it exits `FAILED` rather than 0,
because telling a caller its rig is intact when an adapter is off the bus is the worst thing this
helper can do. The first draft folded the first and third together into `Option<u64>`; the guard
that caught it is the one that now pins all three.

**Measured on the rig, both adapters, at `--json`:** `session_id_changed: true` on each, outages
of **42 ms** and **42 ms**, both nodes returned at their original paths, exit 0. `--dry-run`
resolves every name and changes nothing (`reenumerated: false`, `session_id` unmoved, no outage),
which is the positive control a caller's discriminator is checked against. An unresolvable name is
refused **even under `--dry-run`**, so a typo cannot pass as a successful rehearsal.

**What is still not portable, stated so it is not rediscovered.** `itest`'s `p7_replug_hardware`
takes `SNX_REPLUG_DEV` as a `/dev/serial/by-id/...` path and the Linux helper takes a bus port
name like `3-1`; the macOS arm is addressed by USB serial number, because that is the identity
IOKit offers and macOS has no bus path of that shape. Wiring the hardware replug test to this
backend therefore needs a per-platform address in the harness, which is a change to a test's
contract and is **not** made here.

### 3.67 The flow-control decision: refuse at load, where the operator can act on it

**Design:** §7 (a kernel that differs is reported, never silently approximated), §7.1 (an edge may
ask for `rts-cts`), §9 (a guard asserts the property, never a proxy), §15.26 (structural pre-checks
run before anything is created or torn down).

§3.65 E filed the question and §3.65 E′ built the instrument. The decision is: **a config asking
for hardware flow control on a port whose driver cannot honour it is refused at `load`**, rather
than accepted and left to fault at open, and rather than degraded into a link running without the
flow control it asked for.

**Why not degrade, given §7 says a differing kernel degrades rather than fails.** Because the thing
being degraded here is not an *observation* — it is the transport's contract. An `rts-cts` edge
exists because the far end needs the line held; running it without flow control loses data under
exactly the conditions it was configured to survive, and it does so silently. §7's rule is about
not turning a kernel difference into an unexplained failure, and refusing with the measurement
attached is not an unexplained failure. What §7 forbids is the version where the operator learns
nothing, and that was the *old* behaviour: `serial2` verifies line settings by reading them back,
so the driver's silent drop surfaced as `reopen …: failed to apply some or all settings` and a
`faulted` node, some time after `load` had already returned success.

**One predicate, because two callers must not be able to disagree.**
`serial_nexus_sys::honours_rtscts` opens the port, requests `CRTSCTS`, reads the termios back,
restores what it found, and answers. It is the *only* implementation: the daemon's pre-check
consults it, the harness's rts-cts test branches on it, and doctor P15 now calls it **and requires
its answer to match the read-back P15 took by hand**, reporting `shipped_predicate_agrees`. A
`false` there is its own `degraded` arm, ranked above the finding itself — a report that calls a
port fine while `load` refuses it, or the reverse, is worse than either verdict alone. On this rig
it reads `true`.

**Three states, and only one is a refusal.** `Ok(true)` honoured; `Ok(false)` accepted-and-dropped;
`Err` could not be interrogated. Only the middle one refuses. An unreadable port is **not** a
measured one (§9), and — separately — the pre-check does not run at all for a device path that does
not exist, so an adapter that is merely unplugged is a `waiting` node exactly as before. Collapsing
either of those into "does not honour it" would refuse configs on absent hardware.

**Placed with `precheck_codecs`, for its reason.** `precheck_flow_control` runs in the same
before-anything-is-created position, so a bad `--replace` config never destroys a good running
graph on its way to failing. It is wired into `load` and `add-node`. The refusal is a *structural*
error carrying `node`, `device`, `requested_flow_control` and `honoured_on_readback` as data, and
its message names the remedy (`flow = "none"`, or an adapter whose driver implements RTS/CTS) and
the doctor invocation that reports the same reading.

**The open it performs toggles DTR, and that is not an extra toggle.** The check only runs where
the config asked for `rts-cts` — which is exactly the node that was about to open the same port and
apply the same settings a moment later.

**The red test is now a green assertion of the new promise, which is stricter than the skip it
could have been.** `rts_cts_flow_control_stalls_the_writer_instead_of_losing_bytes` branches on the
shipped predicate: where the driver honours `CRTSCTS` (Linux) it runs the stall-and-recover
assertions unchanged; where it does not (Darwin) it asserts that `load` is **refused**, that the
refusal names both the flow control and the port, and that **nothing was created** on the way to
failing. That is §9's tell that the portable form was found — the platform that cannot do the thing
still has a promise to keep, and the test checks it rather than stepping around it. Measured: the
Darwin arm passes and prints which arm it took.

**Cost:** `serial-nexus-itest` gains a dependency on `serial-nexus-sys`, a workspace-internal crate
already in the graph, so `cargo deny check licenses bans sources` does not move. The harness asks
the shipped predicate rather than re-deriving it, for the same reason P15 does.

### 3.68 The Linux run of the P15 era: a refusal that was dead code, and a toggle the comment denied

**Design:** §7 (a kernel claim cites a committed report; no one-way decision on
single-kernel evidence), §7.1, §12 (identity, not path), §15.26 (structural
pre-checks before anything is created). **Rules:** AGENTS §9 (assert the promise,
never a proxy; fail-first proof; record refuted diagnoses), §5, §8.

`main` reached `17c6e87` with five commits developed and measured on macOS only:
the replug platform split, the macOS USB backend, doctor P15, and §3.67's
`load`-time flow-control refusal. This entry is the Linux run of that tree, on
7.0.0-29-generic with the FT232R crossover attached, load 0.39–0.9 throughout.

**Everything green first, so the defects below are read against a working tree.**
`cargo build --workspace --locked`, `fmt`, both clippy lanes, `cargo deny`, the
passive doctor against `expectations/linux.jq`, and the Playwright lane under
`SNX_WEB_UI=required` (`p8_web_ui` in 24.5 s of real browser time — a skip would
have failed) all pass. The suite reads **830 passing, 0 failed, 4 ignored across 116
test-result lines**, against §2's last recorded 826. **The delta is not re-derived
here, deliberately**: the obvious reconstruction does not close (`ebf9c52` added two
tests, not one, so a naive 826 + 2 + 3 overshoots by one), and 826 was measured
several commits before this tree on a box this session cannot re-run. What *is*
verified is the part that can be: the five Mac commits move the tree's
`#[test]`/`#[tokio::test]` count 834 → 842, of which 3 are P15's doctor guards and 5
are macOS-only and do not compile here — so +3 on Linux. The 830 is a measurement;
treat the 826 as the older measurement it is rather than an addend.

**P15's `supported` arm executes for the first time on any kernel.** Linux honours
`CRTSCTS`: `honoured_on_readback: true`, `silently_dropped: false`,
`shipped_predicate_agrees: true` on both ports, `cflag` moving `0x10021cb2` →
`0x90021cb2` — a delta of exactly `CRTSCTS`. Darwin reads `0x4b00` → `0x4b00` and
`honoured: false`. So the probe's three-way discrimination is now measured on both
arms rather than tested purely on one, and the `rts-cts` harness test takes the
**stall-and-recover** arm here against the **refusal** arm there. **This also
discharges a §7 debt the previous session left**: AGENTS §2 and §3.65 E′/§3.67
asserted "Linux honours the flag" as measured fact with no Linux artifact behind
it — `grep -l '"P15"' docs/doctor/*.json` returned only the three macOS files.

**One bound on the cross-kernel pair, and `field_set` is what says so.** The macOS
triple is at `acb5162`, which predates `shipped_predicate_agrees` (added by
`17c6e87`), so the two carry the same `probe_set` `82a8e2198e54626a` and *different*
`field_set`. Equal `probe_set` is not comparability — §15.44's second digest exists
for exactly this, and this is its first cross-kernel instance. The macOS lane owes a
capture at this tree before the P15 era has a same-binary pair.

#### 1. `precheck_flow_control` was dead code on every path the daemon itself writes

**The defect.** The check read `Path::new(device).exists()`. But `device` is "an
identity in resolver form *or* a raw `/dev` path", and the identity form is not
merely permitted — it is **produced**: `add_node` rewrites the operator's input to
the canonical identity (`daemon.rs`, `*device = resolved.identity.clone()`)
*immediately before* calling the pre-check. `capture_for_dev` can only return
`usb:…`, `by-path:…` or `raw:…`, none of which is ever an existing file. So the
refusal §3.67 announced was **100% dead on `add-node`**, dead on every `load` of a
`dump`ed config, and dead on every `startup_load` replay of the persisted state
file. It was live only for a hand-written literal path handed straight to `load` —
which is the one shape its own guard exercised, so the guard was a §9 proxy and
could not see it. Found by an adversarial review that had not read §3.67's
reasoning; three independent lenses raised it and the verifier could not kill it.

**The fix is the resolver, for §3.67's own stated reason.** `flow_precheck_target`
now calls `Resolver::resolve_current_path` — the *same* call `nodes/serial.rs` makes
to open the port. §3.67 argued there must be one flow-control predicate because two
callers must not be able to disagree about the *answer*; the same argument applies
to the *question*, and the pre-check and the open must not be able to disagree about
which device they are talking about. `None` still means "do not ask", and it still
covers an absent adapter, which stays a `waiting` node rather than a refusal.

**It also closes a second finding for free**: the old form bypassed `--dev-root`, so
a fixture-rooted daemon could have opened and reconfigured a real adapter outside
the tree it was pointed at. One verifier refuted that as unreachable today (no
`--dev-root` caller in the tree uses a literal path *and* `rts-cts`); the resolution
moots it either way, which is the better outcome than a reachability argument.

**Fail-first, executed.** `the_flow_pre_check_resolves_the_device_rather_than_treating_it_as_a_path`
reddens on "an rts-cts node whose identity resolves must be interrogated" against
the pre-fix expression restored in place. It carries its own control — it asserts
`Path::new("raw:/dev/null").exists()` is **false**, the pre-fix predicate's answer —
so a future reader can see why the naive form was dead rather than take it on trust.
It pins the daemon's half of the decision (which port gets asked) because that half
is kernel-independent; the platform's half stays with the harness test, which is the
only place both arms can be exercised.

#### 2. "It is not an *extra* toggle" was false, and the rig can measure it

The pre-check's doc argued its open was free because the node was about to open the
same port anyway. It is not: `honours_rtscts` performs a *complete*
open→configure→restore→close cycle that finishes before the node's open begins, and
on Linux the last close of a tty with `HUPCL` lowers DTR and RTS.

**Measured with `TIOCGICOUNT` on the far port** — the kernel's own CTS interrupt
counter, because the pulse is ~0.7 ms and a 0.5 ms poll loop misses it (it did, and
that inconclusive reading is recorded here so the instrument choice is not
re-litigated). The rig cross-wires RTS↔CTS, so the far port's CTS count is an exact
readout of the near port's RTS line.

| config | pre-check | CTS edges at the far port |
|---|---|---|
| `flow = "none"` | enabled | 0, 0, 1 |
| `flow = "rts-cts"` | enabled | **2, 2, 2** |
| `flow = "rts-cts"` | **disabled in place** | 0, 0, 0 |
| `honours_rtscts` alone, no daemon | — | 2, 2, 3 |
| no-op control | — | 0, 0 |

The third row is the one that matters: it **refutes the confound**. Enabling
`CRTSCTS` puts RTS under kernel control, which could have produced the edges by
itself — with the pre-check disabled and everything else identical, it produces
none. All 2 edges are the pre-check's.

**What is not measured:** DTR. The rig leaves it unwired (§3.53 (i)), so the DTR
half rests on the same kernel call path (`tty_port_lower_dtr_rts` moves both) and is
*inferred*, not observed. Said here rather than folded into the measured claim (§9).

**The cost is stated where the decision is, and the fix above widened it**:
identity-form configs now reach the check, so they now pay the pulse too. A board
that auto-resets on DTR takes one extra reset per `load`, per `add-node`, and per
`startup_load` replay. Bounded (only nodes that asked for `rts-cts`) and it buys a
refusal the operator can act on. Removing it means asking from inside the node's own
open, which is past the point where "nothing is created" holds — a design question
(§5), filed, not patched here.

#### 3. Two holes in P15's `baseline_restored`, the field its own verdict ranks first

Both are the same shape: the field claims the port was put back, and the probe's
last write to that port is outside what the field describes.

**(i) The restore was decided before the last write.** `restored` was computed, and
*then* `sys::honours_rtscts` was called — which opens the same tty again, sets
`CRTSCTS`, and restores with the result discarded. It structurally cannot verify
itself: it closes the fd it would need to re-read through. On Linux the flag is
genuinely honoured, so that write really does assert hardware flow control on the
adapter. P15's own fd is still open, so the fix is one `tcgetattr` after the call.

**(ii) The read-back `?` returned between the set and the restore.** A failed
`tcgetattr after` left a real adapter with `CRTSCTS` asserted and then reported
`skipped (no port could be opened)` — the most reassuring word in the vocabulary,
and one that **exempts both gate files' restore clause**, so the single error path
that could strand a port was the one path that emitted nothing about it. The restore
now runs before the read-back is inspected.

Verified externally rather than by reading: after a Tier-3 run both ports read
`-crtscts` under `stty`.

#### 4. Two guards the generation shipped without

**`shipped_predicate_agrees`'s degrade arm had zero coverage.** Nothing constructed
`Some(false)`, so the branch the notes rank *above the probe's own finding* was
unexecuted, and both of its rank relationships with it. `p15_ranks_a_daemon_disagreement_below_a_failed_restore_and_above_its_own_finding`
asserts them against rows that would otherwise select the neighbouring arm, plus
that `None` (the predicate could not run) is not a disagreement. Fail-first proven by
neutering the arm's filter.

**P15's expectation clauses had no in-tree guard**, unlike the `field_set`, P4, P14
and handshake clauses beside them — and they are the easiest kind to lose, because
CI runs the doctor **passively**, so P15 reports `skipped`, the antecedent is false,
and neither automated lane ever evaluates the clause. Deleting it left `cargo test`
and both gates green. Two guards now, on the handshake clause's pattern: verbatim
identity across both files, and a synthetic antecedent proving each required cell
reddens the gate when absent *or null*, that an `unreadable_ports`-only observation
does not satisfy it, and — deliberately — that flipping `honoured_on_readback` does
**not** redden it, because either kernel reading is a legitimate fact (§7). Both
fail-first proven by deleting the clause.

#### 5. Filed, not fixed — two items with the mechanism named

**(a) `load --replace` on a port the running graph holds defeats the pre-check.** It
runs before the teardown, deliberately, so a bad `--replace` cannot destroy a good
graph. But at that moment the outgoing node still holds the port under `TIOCEXCL`,
so the pre-check's own `open(2)` is refused, `Err` maps to "unmeasurable, not a
refusal", and the check cannot fire in the one scenario its placement was justified
by. The premise is proven in-tree, not assumed: `nodes/serial.rs`'s Linux-gated
`third_party_open` guard asserts exactly that a second open of a daemon-held port
fails. **Not patched**, because both repairs are worse than the gap until a decision
is taken: re-checking after teardown means a refusal *has* destroyed the good graph,
and inferring the answer from the outgoing node's own successful open is an
inference, not a measurement. Unobservable on Linux besides — this kernel honours
the flag, so the refusal never fires here at all (§7).

#### 6. §18 item 3 reproduced sharply: 3 of 4, one test, exactly 64 bytes, and a number that is a deadline

The replug lane ran last (the blessing needs a human at a polkit dialog and arrived
after the rest of this session's work), so it got its own measurements. **In
isolation it is clean**: `SNX_REPLUG=required` with both adapters named, 4 passed in
4.15 s, and it does real work — `["waiting"]` observed while the tty was destroyed,
released by the `stdin-eof` leash, `devnum 9 → 9` (§3.54's refuted discriminator,
refuted again), and §12's founding premise measured: adapter A moved
`ttyUSB1 → ttyUSB0`, the daemon's `resolved_path` followed, `identity` unchanged.

**In a full `cargo test --workspace` it reproduces §3.62's correlate, harder than the
record.** This session, on an idle box (load 0.12–0.76 throughout):

| lane | runs | failing |
|---|---|---|
| full suite, replug lane **green** | 4 | **3** |
| full suite, replug lane **not enabled** | 6 | **0** |

Running total across the three sessions that have measured it: **8 of 14 with, 0 of
15 without.**

**Three refinements, all new.**

1. **The signature is sharper than "two timing-sensitive tests".** All three failures
   were the *same* test, `crossover_rig_custom_baud_byte_exact`, *exactly* 64 bytes
   short (32704 of 32768), in 71.95 / 71.94 / 71.95 s. §3.62 recorded that which of
   `serial_hardware`'s tests draws it varies; here it did not vary at all.

2. **71.95 s is a deadline, not a slow run**, and §3.54 read it the other way ("once
   at 67.7 s against a usual 10.4 s"). `inject_verify`'s `settled_while_open` window
   is 60 s and the test runs two directions: one completes normally and the other
   waits the window out in full, which is where ~72 s comes from and why it
   reproduces to 10 ms. A number that stable is the harness's own constant, and
   reading it as a performance figure sends the next session after the wrong thing.

3. **Two hypotheses are refuted by controls run *immediately* after the third
   failure**, which is the part no previous session had. `serial_hardware` alone:
   **6 of 6 in 14.59 s** — the rig is not damaged and the adapters are not wedged.
   The whole suite with the replug lane simply not enabled: **834 passing, 0 failed**
   — the box is not left in a bad state either. So the effect is scoped to the
   `cargo test --workspace` invocation in which the replug lane ran, and does not
   outlive it.

**Say the failure precisely, because §6 forbids the short form.** What is measured is
64 bytes *not recovered within 60 s* on one direction of a link whose other direction
completed normally in the same run. That is a stall or a loss and **this session did
not separate them**. **Mechanism not established and no root cause claimed** (§9).
The `--test-threads=1` arm §3.62 used to refute H1 was not re-run here.

**Consequence for the headline figure, stated rather than buried:** the documented
rig lane of AGENTS §3 — which includes `SNX_REPLUG=required` — is **not** reliably
green on this box. The 834/0/4 quoted above is the rig lane *without* the replug
capability, reproduced across six runs; with it, this session saw 834/0/4 once and
833/1/4 three times.

**(b) P15's question cites §15.51, which is P14's section, and no design entry for
P15 exists** — §5 says amend the design first, and that step was skipped. The
citation is inside the `question` string, which is what `probe_set` digests, so
correcting it moves the era a second time and orphans the three macOS artifacts
committed under `82a8e2198e54626a` before any Linux counterpart exists. Recorded
rather than silently corrected, because which of those two costs to pay is a
decision about the comparability discipline, not a typo fix.

### 3.69 An out-of-tree report against `reader_thread`: two proposals refuted by measurement, one real gap

**Design:** §7 (measure the kernel, do not infer it from a harness failure; a kernel
claim cites a committed report). **Rules:** AGENTS §9, §10 (capabilities, never
consumers).

A change report arrived against `reader_thread`, written at `85699d6` — 69 commits
back, and before the rename track, so it names a directory that no longer exists. The
function is **unchanged** since, so the report is about current behaviour rather than
something already fixed. It proposed three things and said plainly that none of them
was the fix for the problem being chased. Two are refuted here and the third is real.

**(1) Adding `POLLERR` to the request mask: a no-op, measured twice.** POSIX delivers
`POLLERR`, `POLLHUP` and `POLLNVAL` in `revents` whether or not they were requested,
and this tree does not have to cite that — doctor P9 measures it: on Linux
`hangup_delivered_to_a_mask_that_requested_nothing` is `true` and a poll requesting
*nothing* reads back `POLLHUP`. Measured again directly for this report, on a real
FT232R unplugged through the replug helper while an fd was held: the existing
`POLLIN|POLLHUP` mask reads back **`POLLIN|POLLHUP|POLLERR`**, 3 of 3, both fd
flavours, 1.2–2.0 ms after the sysfs write, and `read` then answers EOF — so the
existing arm handles a real unplug, and `POLLERR` is already visible without being
asked for. Darwin is the one kernel that gates the hangup on the mask (P9: an empty
mask reads `none` there), and the mask in question is not empty; its own artifact
reads `POLLIN|POLLHUP` for the same fd state.

**(2) Draining on `POLLHUP`: the code already does, whenever there is anything to
drain.** The concern is exact — `reader_thread` drains only under `POLLIN` and gives
up under a bare `POLLHUP`, so it would lose trailing bytes if a kernel reported the
hangup *without* `POLLIN` while data was queued. It does not. Measured on Linux: a
hung-up pty master with 64 and with 4000 bytes still queued reads `POLLIN|POLLHUP`,
3 of 3 each, and yields every byte; the real unplug above reads all three flags and
then EOF. Darwin's committed P9 artifact reads `POLLIN|POLLHUP` for a hung-up master
under a `POLLIN`-requesting poll. So the bare-`POLLHUP` arm is reached only when
there is nothing left, and the one shape that would break it — a *canonical* tty
withholding `POLLIN` until a line completes — the daemon never runs, because it puts
every port in raw mode (P10 re-asserts that and reports `slave_termios_mode`).

**This is the "clarify it" case, and the clarification is a test, not only a
comment.** The arm reads as though it might drop data; a careful reader suspected it
did. `a_hangup_with_bytes_still_queued_reports_pollin_so_the_reader_drains_it` pins
the premise at three payload sizes, and carries the anti-vacuity control that makes
it worth having: it asserts `POLLHUP` **is** present, so a version that forgot to
close the slave — or a kernel that deferred the hangup past the poll — fails instead
of sailing through on `POLLIN` alone. Proven by removing the close: the control
fires, naming exactly that. Neither P9 nor P13 covers this: P13 measures whether the
bytes survive a last close and P9 measures poll's latency by fd state, but neither
puts *data* behind a hangup and asks what poll advertises.

**(3) The swallowed `errno` is a real gap, and it is now reported.** `Err(_) =>` threw
the error away, `lost` is a payload-free `Notify`, and the supervisor printed
`device <name> lost` — the same sentence for an unplugged adapter, a driver `EIO`,
and a peer that closed. That is precisely what §7 says a report must not do. A
`LossCause` word travels from whichever half noticed to the `Waiting { reason }` the
operator reads, distinguishing EOF, a read `errno`, and a failed targetward write;
it is cleared when a reader is armed, so a second outage cannot inherit the first
one's cause. Reported through the status rather than a log line **because this module
deliberately has no logging** — `tracing::` appears zero times in it — and the status
is what `ctl state` shows. The report's suggestion was `tracing::warn!`; the gap it
identified is real and the idiomatic surface here is a different one.

### 3.70 The replug-lane flake, root-caused: a re-enumerated FT232R eats its first packet

**Design:** §7. **Plan:** §18 item 3. **Rules:** AGENTS §6 (say a deadline precisely),
§8 (measure the box; do not trust a broken instrument), §9 (fail-first; record refuted
diagnoses).

§3.62 correlated this with the replug lane executing and named no mechanism. It has
one now, established below the daemon, and the fix is proven against a reproducer that
fails 9 of 11 unfixed.

**A reproducer, two minutes instead of eight.** `cargo test -p serial-nexus-itest
--test p7_replug_hardware --test serial_hardware` reproduces it **5 of 5** — which
**refutes §3.54's "the direct replug→rig sequence is 0 of 2"**, a figure taken at
n=2 against an event with a ~70% rate.

**The instrument was wrong first, and that is worth recording.** An early attempt read
the tty→USB-port mapping with a `sed` that took the wrong path component, so a series
of unplug measurements deauthorized the adapter that was *not* being watched and
concluded that a real unplug reports nothing at all. The tell was a control that had
been added for a different reason (`down during window=False`); without it the wrong
reading would have gone into this entry. `serial-nexus-devprep status` is the
authority for that mapping — `ttyUSB0` is on `3-3` and `ttyUSB1` on `3-1` on this box,
the reverse of what the `sed` said.

**The mechanism, measured at the stall through the daemon's own state RPC.** The
kernel's `TIOCGICOUNT` reads `tx 32768` on the sending port and `rx 32704` on the
receiving one, with `frame`, `overrun`, `parity`, `buf_overrun` and `brk` all **0**;
every daemon-side counter — `discarded_unattached`, `dropped_bytes`, `queued_bytes`,
`purged_on_reconnect`, `write_errors` — reads **0**, and every node is `active`. **The
daemon lost nothing**; the bytes never reached the receiving driver, and no error was
counted for them.

**And they are the *first* 64.** A failing capture is byte-identical to a good one at
the same seed from offset 64 — `bad == good[64..]` is **true** and `bad ==
good[..-64]` is **false**. Exactly one 64-byte USB bulk packet, at the head of the
first traffic to cross after a re-enumeration.

**Two hypotheses refuted along the way.** *Settling time*: gaps of 2 s and 5 s between
the replug and the measurement still failed 3 of 3, so it is not a device that needs
to quiesce. *The custom rate*: `crossover_rig_data_plane_send_and_exclusivity` at
115200 fails identically 2 of 4 when it runs first, so 250000 is not implicated —
what matters is being **first**. In a six-test run under `--test-threads=1` only the
first test ever failed; every later one is protected by the traffic its predecessor
already pushed. Idle time does not help and traffic does.

**The fix is a primer, and it is §3.47's shape.** `prime_the_wire_once` pushes 1 KiB
each way through a throwaway graph and waits for it to arrive before any measured test
boots — *prove the far end is really receiving before you start counting*, rather than
assume the link is live because both nodes report `active`. Once per process, because
the loss is once per enumeration and a test binary cannot replug its own adapters. Its
own daemon, logs and run dir go away with it, so no primed byte can land in a capture
under test. The 500 ms settle in `crossover_rig_custom_baud_byte_exact` is for a
different FTDI quirk (garbled bytes right after `open`+`set_baud`, P5_OPEN_SETTLE) and
does not cover this one.

**Fail-first, executed:** the reproducer fails **9 of 11** on the unfixed tree and
**0 of 8** with the primer, and the binary's runtime collapses from 72–129 s (a 60 s
`settled_while_open` deadline waited out in full) to a flat 14.74 s.

**What this does not claim.** Why the adapter drops that packet is not established —
whether it is the FT232R's receive path, `ftdi_sio`'s first URB after bind, or the
bus — and no root cause is claimed there (§9). What is established is where the bytes
are *not* lost: the daemon is faultless, by its own counters and the kernel's, and
this was never a product defect. The primer makes the rig tests measure the property
they promise rather than the state of an adapter that was re-enumerated a moment ago.

### 3.71 A cfg attribute stolen by an insertion: the macOS break `cargo build` cannot see

**Design:** §13 (macOS is best-effort, and must still *compile*). **Plan:** §3 rule 8.
**Rules:** AGENTS §3 (the gate set), §9 (fail-first; a guard must catch the class, not
the instance).

Found while answering a support report from an arm64 Darwin 27 box — the first evidence
this tree has from either arm64 or Darwin 27. The reporter could produce a doctor report
and not a single test result, and the reason was in this tree rather than on their
machine.

**The defect.** `3e23c52` inserted `a_hangup_with_bytes_still_queued_reports_pollin_so_
the_reader_drains_it` into `daemon/src/nodes/serial.rs`'s test module, between the
`#[cfg(target_os = "linux")]` that gated `fn pts_fixture` and `fn pts_fixture` itself.
Rust applied the attribute to the new item — which already carried its own — and
`pts_fixture` was left ungated while `struct PtsFixture` stayed gated. Off Linux that
is `E0425` and `E0422`, and `serial-nexus-daemon (lib test)` does not build.

**Why every existing gate missed it.** `cargo build --workspace` compiles no test
target, so it is green on every platform. `cargo test` and `cargo clippy --all-targets`
do compile it, but on Linux the cfg is satisfied and the code is correct — so the
entire default gate set passes, on the platform the gate set runs on. The failure is
reachable only by compiling a *test* target for a *non-Linux* target, which nothing did
until the macOS lane, which runs last and is the lane a maintainer is primed to read as
"macOS is flaky again".

**The guard is a cross-check, not a test.** The Linux `check` job now runs `cargo check
--target <triple> --workspace --exclude serial-nexus-web --all-targets` over both Apple
triples. `check` does not link and needs no Apple SDK, so it costs one `rustup target
add` and a few seconds. **Fail-first, with the guard as written**: reverting the one
attribute in place reddens that exact command with the two errors above, and restoring
it returns it to clean.

**Both triples, and the second one is justified by measurement rather than by caution.**
The first draft ran `aarch64` alone, reasoning that `macos-latest` is arm64 so one
triple compiles what the protected lane compiles. That reasoning is sound for *this*
defect and wrong for the guard's stated job, which is to catch the class. Neither target
is a superset of the other, proved by planting a defect in each direction inside a
`#[cfg(target_os = "macos")]` module in `sys` and running both:

| plant                     | x86_64 check     | aarch64 check    |
|---------------------------|------------------|------------------|
| x86-only intrinsic        | **passes**       | caught (`E0433`) |
| aarch64-only NEON type    | caught (`E0433`) | **passes**       |

So each triple catches exactly the arch-specific code written on the *other*
architecture — this session's mistake one axis over, reaching for `target_os` where the
code is really `target_arch`. The risk is live rather than hypothetical: `macos-latest`
is arm64, the bench rig is an Intel Mac, the newest field report is an M4, and
`sys/src/usb_macos.rs` is hand-written FFI in the one crate allowed `unsafe`. There are
zero `target_arch` hits in the tree today and this is what keeps that cheap to maintain.

**One instrument error inside the matcher proof, recorded because it nearly passed.**
The first mirror plant used `std::arch::aarch64::__rbit64`, which is not a real
intrinsic. It failed on *both* triples and would have read as "x86_64 catches it too"
— a pass for the wrong reason. The tell was the error text: `E0433 cannot find aarch64
in arch` on x86_64 against `E0425 cannot find function __rbit64` on aarch64, two
different failures. Re-run with a stable NEON *type* (`uint8x16_t`), which compiles on
aarch64 and is absent on x86_64, it separates cleanly. Reading only the exit status
would have recorded the wrong conclusion.

**What the guard does not cover, stated rather than implied (§9).** `serial-nexus-web`
is excluded: its TLS stack pulls `ring`, whose build script shells out to `cc` with
`-arch arm64`, which a Linux runner's `cc` rejects — the step would be red for a reason
that is not a defect. So a stolen cfg inside `web`'s own test code still reaches only
the macOS lane. Every other crate is covered, including `daemon`, where this instance
and the class's only other recorded member both landed.

**Second instance, not third.** §3.65 A was the first macOS-only compile break — the
`serial-nexus-devprep` crate as an unconditional workspace member using a Linux-only
symbol. A session note drafted here claimed a third; **it is withdrawn**, because
nothing in the history or the notes substantiates it and the two mechanisms are not the
same one twice. The distinction that matters is not the count: §3.65 A reddened
`cargo build --workspace`, the macOS lane's own first step, and this one does not. That
is why a new instrument was needed rather than a second reading of the old one.

### 3.72 The M4 field report: what it settled, and the defects it found in this tree

**Design:** §13, §15.44, §15.53. **Plan:** §18. **Rules:** AGENTS §7 (a differing
kernel is `degraded` with the observation named), §9 (fail-first; record refuted
approaches), §10.

A `serial-nexus-doctor` report arrived from a **macOS box with an Apple M4, Darwin
27.0.0, arm64** — the first evidence this tree has from either arm64 or Darwin 27, and
taken at `3e23c52`, which was HEAD, so the binary was current rather than an artifact of
some older vintage. The question asked of it was whether the platform needs more
fallbacks.

**It does not.** All 24 probe answers land on code paths that already ship: `14
supported / 10 degraded / 0 unsupported / 3 skipped` is ten §7 degrades with the
observation named, zero capabilities the daemon cannot work around, and three probes
structurally unaskable off Linux. What the report found was **defects in this
repository** — one of them blocking, and none of them on the reporter's machine.

#### 1. The platform reading, and why it carries so little new risk

Diffed cell by cell against `docs/doctor/macos-24.6.0-2026-08-05-acb5162-tier3.json`
(x86_64 Darwin 24.6.0, same `probe_set` `82a8e2198e54626a`), the M4 is **identical to
the already-validated Intel Mac except for durations**. Across the whole pty plane the
key sets are equal and exactly 11 cells moved, every one of them a timing; zero
booleans, strings, byte counts or pass counts differ. The serial plane is identical on
both ports including `cflag 0x4b00 → 0x4b00`, `ENOTSUP` for `TIOCGICOUNT`, and P14's
3 Mbaud `platform-refused`/errno-22 ceiling. P10's depths are byte-identical.

Two macOS-only mechanisms ran on arm64 for the first time and both work. `SessionLatch`
is `#[cfg(target_os = "macos")]` with no arch predicate, and P12 reads `supported` with
its positive control firing at 120 µs — that is §6 detach-release, the one macOS-only
mechanism never before exercised off x86_64. And `sys::honours_rtscts` demonstrably ran:
`shipped_predicate_agrees: true` beside `honoured_on_readback: false` is producible only
by `Ok(false)`, so §15.53's refusal fires on that box.

**What is genuinely new**, recorded so a later session does not rediscover it: the
Darwin↔Linux poll gap splits into two unrelated halves, and only one is CPU-bound — the
M4 is ~4x faster than the Intel Mac on every P9 syscall cell while the timer overshoot
does not move at all (5 ms → 641 µs here, 683–690 µs on x86_64 Darwin, 80–84 µs on
Linux). P13's pts `close(2)` is 5–6x outside every committed envelope on both kernels
(161/86/129 µs against maxima of 25/29/24 across 37 captures) with the *policy*
unchanged and `a_no_reader_blocking_slave` still at 601180 µs, so §6's harness rule keeps
its full margin. **Not separated (§9):** whether the slower close is a Darwin-27 effect
or an arm64 one — no capture isolates them, and n=1.

**Read P9's headline correctly.** `median_ns_for_0ms_request: 16333` is n=16 taken cold.
The cell the data plane parks on is `unready_master_pollin_ns: 3833` at n=4096, so the
honest ratio against Linux is **14.6x, not 62x**. The probe says so itself in
`headline_over_matched_cell_x100: 426`; nothing should be tuned against the 16 µs figure.

#### 2. The blocking defect was ours (§3.71)

`cargo test` did not compile on any macOS at `3e23c52`. Carried in its own entry.

#### 3. P15's consequence described a tree several commits old

The probe's `degraded` arm told every reader that an `rts-cts` node "goes `faulted`, not
`degraded`" and that whether to degrade or keep faulting "is a design question this
probe exists to inform, not to answer". **Both had been false since §3.67 shipped the
refusal at `load`.** The M4 report carried that text verbatim — so on the one platform
where this is the report's only *actionable* finding, the sentence the operator would act
on described behaviour that no longer existed, while every measurement beside it was
correct.

Nothing in the gate set could see it: `expectations/*.jq` assert over
`.probes[].observations`, `probe_set` digests `(id, question)`, `field_set` digests the
observation leaf paths, and a consequence is none of those. A consequence string is
invisible to every gate and visible to every operator. The arm now states the shipped
refusal, both remedies, and its own bound — the two paths that still reach a `faulted`
node — and a guard asserts those as *properties* rather than as a golden string, with
two negative clauses naming the retracted claims specifically, because those are what a
revert would restore. Fail-first: restoring the old string in place reddens it.

The decision it describes had also never been written into the design; §15.53 does that
now, which discharges the debt §3.68 filed when it noted P15's `question` citing
§15.51 — P14's section — for want of one of its own. The `question` string itself is
**not** touched: it feeds `probe_set`, and moving the era again would orphan the macOS
triple at `acb5162` and this report with it.

#### 4. `field_set` was not stable across runs of byte-identical code

Two Linux Tier-3 runs at one commit, `git diff -- doctor/` empty, produced different
`field_set` digests. Both carried 478 leaf paths and the *entire* delta was P5's pair key
spelled `A ↔ B` in one and `B ↔ A` in the other: the key was formatted straight from
`ports[i]`/`ports[j]`, which follow argv, and the identity standing at a given argv
position moves across a replug — §12's own test renumbers `ttyUSB0`/`ttyUSB1`
deliberately. So §15.44's second digest was telling a reviewer to "diff only the
intersection" of two runs whose cells were identical, which is the exact failure mode
`probe_set`'s choice 3 was written to avoid, reappearing in the digest built to fix it.

Sorting the two names is the whole fix, and it is deliberately a **key**-ordering change
only: the `a`/`b` roles keep argv order, because they carry measured direction
(`rts_a_to_cts_b` against `rts_b_to_cts_a`, P14's `ab` against `ba`), and reordering them
would relabel measurements to stabilize a digest that does not read them. The guard
asserts the property over several name pairs in both orders plus an anti-collapse
control, rather than against the one pair this bench has — a single-pair fixture cannot
see an ordering bug. **This moves `field_set` and not `probe_set`**, which is the pair
§15.44 exists to make visible.

#### 5. The doctor named a socket fallback the daemon does not use

With `XDG_RUNTIME_DIR` unset the environment block said *"daemon falls back to /run or a
--socket override"*. `/run` is the **root** arm. An unprivileged process gets
`/tmp/<name>-<uid>.sock`, and macOS sets no `XDG_RUNTIME_DIR` and has no `/run`, so
every unprivileged Mac session takes the arm the sentence did not mention — in the first
field an operator reads when the socket is not where they expected, in the report the
design calls the expected first attachment on any support request.

The policy had been implemented twice (daemon, `ctl`) and *described* a third time, and
it is the description that was wrong. The fix deletes the third copy rather than
correcting it: `serial_nexus_rpc::socket` holds one implementation, the daemon binds
through it, `ctl` connects through it, and the doctor **computes and prints** the path
with `SocketOrigin` naming which arm produced it. A printed path cannot drift from the
bound one.

**A refuted placement, recorded (§9).** `serial-nexus-core` was tried first — all three
callers already depend on it — and a meta-gate rejected it: `core` may not declare
`nix`, because the resolver's enumeration face is passive by construction and probing a
port toggles DTR and resets the board behind it. The gate exists for precisely the move
being made ("just add `nix` to core for one `getuid`") and it was right.
`serial-nexus-sys` was the next candidate and is declined: it links IOKit and
CoreFoundation on macOS, and `ctl` does not depend on it, so reading a uid would have
cost the CLI two framework links. It lands beside `DAEMON_NAME` instead.

The policy is split into a pure `socket_path_from(is_root, xdg, uid, name)` and a
two-line wrapper that reads the process state, so **all three arms are testable
including root** — which the suite can never run under. That is not a seam invented for
a test: the alternative was mutating process-global environment, which in Rust 2024
means `unsafe` in a `#![forbid(unsafe_code)]` crate, and is racy under the parallel
runner besides. An exported-but-empty `XDG_RUNTIME_DIR` is covered too: treating it as
set yields a *relative* path the daemon would bind in its working directory.

#### 6. A fault the pre-check could not prevent was also unreadable

§15.53 refuses an `rts-cts` node whose driver drops `CRTSCTS`, but it must **open the
port** to measure that, and two paths reach an open having never had a measurable one:
`load --replace` on a port the running graph holds (the outgoing node's `TIOCEXCL`
refuses the pre-check's open, which maps to *unmeasurable*, which is deliberately not a
refusal), and an adapter that arrives after the config loads. On both, `serial2` fails
by read-back and the operator got `failed to apply some or all settings` — no flow
control, no port, no remedy.

**Both repairs to the *refusal* stay declined** (§3.68 (5a)): re-checking after teardown
means a refusal has already destroyed the good graph, and inferring from the outgoing
node's successful open is an inference, not a measurement. Neither is re-litigated here.
What was never declined is that the resulting *fault* must be readable, and that is what
changed: the node's own failed open consults `sys::honours_rtscts` — the same one
predicate — and substitutes the reading and both remedies. It runs after the open has
already failed, decides nothing, and creates nothing, so it touches no declined ground.

Bounded: the extra open happens only when the node asked for `rts-cts` *and* is already
failing. And it consults the predicate rather than pattern-matching `serial2`'s error
text, which is not a contract. The message logic is split into a pure
`open_failure_text(honoured, flow, path, err)` for the reason `p15_verdict` is split
from P15 — **the arm that matters cannot be reached on Linux**, which honours the flag,
so a guard driving the real predicate here would exercise only the pass-through and read
as covered. `honoured: None` is *unmeasurable* and must not blame flow control; the
guard asserts all five non-blaming combinations explicitly, because collapsing `None`
into `Some(false)` is exactly the mistake §15.53 names for the `load`-time predicate.

#### 7. Three identity messages pointed away from the one form that works

The raw `/dev/cu.*` path works end to end on macOS, and P4's `degraded` is the designed
answer. What was wrong was everything the operator is *told*:

* A `usb:`/`by-path:` node sat `waiting` with reason **`device <d> not present`** — false
  on a box where the adapter is plugged in and `access(2)`-readable. The resolver has
  exactly two identity sources, the by-id tree and the `<sys>/class/tty` listing, and a
  system carrying neither resolves such a device to nothing *and always will*. So the
  message sent that operator to inspect a cable for a condition no cable can change.
  `Resolver::has_identity_source` distinguishes the two facts; the **status stays
  `waiting`**, because a future IOKit backend (§14) would make exactly this config
  resolve — only the sentence moves (§7: name the observation). A raw path is
  deliberately excluded: it is `stat`ed literally, never resolved, so "not present" is
  precisely true for it on every platform and widening the new text would be its own
  false statement. The predicate is a *directory* test, so an empty-but-present by-id
  tree is a source with nothing in it — real absence — while a missing tree is no source
  at all; the guard asserts that distinction, since a fixture that only ever populated
  the tree could not see it.
* `add-node BH00L4KU` — the most natural thing to type when the node is literally
  `cu.usbserial-BH00L4KU` — was refused with "add by a `usb:`/`by-path:` identity", i.e.
  the one remedy guaranteed to produce the forever-waiting node above. **A remedy has to
  be reachable on the system being advised.** Both arms now come from a free function so
  the text is assertable without standing up a daemon, and both point at
  `serial-nexus-ctl ports`, which lists every visible device with the identity it would
  bind. The "is not present" phrasing is kept in both arms: it is the true half, and it
  is what `DEVICE_ABSENT`'s consumers match on.

#### 8. Harness messages, and four things filed rather than fixed

Two harness defects were the same shape as the ones above — a message that is wrong only
off Linux, on a lane nobody runs before pushing:

* **`SNX_REPLUG=required` failed with a remedy that is a no-op on macOS.** The helper
  lookup reported "not installed — run `install`, then the sudo command it prints", but
  the macOS `install` deliberately installs nothing and prints no sudo command, because
  its re-enumeration path is unprivileged (§3.66). So a Mac operator got a hard failure
  whose fix they could run forever. The non-portability is unchanged and deliberate;
  only the explanation is new, and it names the one thing that would actually have to
  change (the hardware replug test's contract — it takes a `by-id` path where the macOS
  arm is addressed by USB serial number).
* **`p12_pty_setup.rs` hardcoded GNU's `stty -F`.** BSD spells it `-f`, so every call on
  Darwin returned `(false, "")` — and the caller reads that as "this `stty` does not
  report `extproc`" and **skips green, naming the wrong cause**. Right verdict, wrong
  reason, and a skip class with no `required` spelling to force it. The portable form
  was already in-tree in `p9_pty_collapse.rs`.

**Filed as plan §18 items 12–15, not patched**, because each needs a decision or a
measurement rather than an edit:

* **12** — the anti-spin guard is a §9 proxy in space, and it is the sharpest one in the
  tree: it self-skips off Linux, and *Linux is the kernel where the hazard it guards
  cannot occur* (P6 reads 0 `POLLIN` passes there against Darwin's 64 of 64, on x86_64
  and now arm64 alike). A regression widening `pty.rs`'s last-close predicate would burn
  a core and release operator-held write locks on macOS with the suite green. Not patched
  because the fix needs a portable CPU-time source and `p3_idle_cost.rs` states in-tree
  that none exists — a design question (§5). Its fail-first must be proven *on Darwin*,
  or it repeats the substitution it exists to remove.
* **13** — idle CPU on any Mac is arithmetic, not measurement: ~4–15% of a core projected
  on the M4, 17–19% on the Intel box, inside the 20% budget and above the 3.5% drift
  ceiling, with nothing in the gate set able to notice. Predates this report.
* **14** — `xon-xoff` has no pre-check and no probe, while `serial2` verifies `c_iflag`
  by read-back exactly as it verifies `c_cflag`. **Unmeasured, not known-good.**
* **15** — the arm64/Darwin-27 artifact is owed. Everything in this entry is quoted from
  a pasted report, not diffed against a committed capture. This **narrows** §18 item 8
  and does not close it: that owes `macos-26-arm64`, which is a different image, and no
  line of `sys/src/usb_macos.rs` has ever run on arm64.

### 3.73 A Linux 6.18 field report: five defects it found, four of them in the instrument

A `serial-nexus-doctor` Tier-3 report from a Linux 6.18.14-1rodete4-amd64 box, at
commit `3e23c52`, probe set `82a8e2198e54626a`, field set `179c9d15c6e450f5`. Assessed
against the committed `docs/doctor/linux-7.0-2026-08-05-7cf0338-tier3{,-2,-3}.json`
triple. **The comparison is lawful on a stronger basis than either digest can give:**
`git diff 7cf0338 3e23c52 -- doctor/ core/ sys/ rpc/ codec-api/ Cargo.lock` is empty,
so the two reports came from the *same doctor source*, which closes the residual
blindness §15.44 leaves open — a probe body that moves a number without moving a key.

**The kernel answer first, because it is the least interesting part: 6.18 and 7.0 are
the same kernel as far as this instrument can see.** `24 supported · 0 degraded · 0
unsupported · 1 skipped`, the skip being P12's deliberately inert `SessionLatch` arm.
Zero booleans, strings, errno histograms, byte counts, ceilings, ioctl-availability
flags or termios modes differ across the 16 probe sections. **All five deferred
"diff against the production kernel (6.18)" decisions are discharged and every one
licenses nothing new** — P6's `saw_session` latch still stands (Darwin at the same
probe set reads `pollin_passes: 64` against 6.18's 0), P8 leaves invariant 2 untouched,
P9's 1 ms floor is *tighter* on 6.18, P10 moves no buffer default. **P13, P14 and P15
executed on 6.18 for the first time** — the prior 6.18 artifact,
`docs/doctor/linux-6.18-2026-07-29-tier3.md`, is P1–P12 at a dead era — and P15 reads
`honoured_on_readback: true` with `cflag` `0x10021cb2` → `0x90021cb2`, a delta of
exactly `CRTSCTS`, so §15.53's pre-check is inert there.

**What blocks "fully supported" is execution, not behaviour, and it is plan §18 item 8
unchanged:** no test has ever run on 6.18, and `jq -e -f expectations/linux.jq` has
still never been *executed* there. This is the **third** Markdown-only 6.18 visit; the
one-shot protocol in `docs/serial-nexus-doctor.md` exists to prevent exactly that and
did not take. Every clause was evaluated by hand against a transcription and all pass,
with eight deliberate mutations rejecting, so the report is gate-clean *on inspection*
— which is inference, not an exit status, and five clauses rest on a JSON *kind* the
Markdown renderer cannot express. The next visit needs one `>` redirect.

**(1) `Probe::observe` appended where it had to replace, and P14 shipped every report
carrying three keys twice.** `p14_max_rate` stamps `max_reliable_baud`, `ceiling_kind`
and `ceiling_is_a_floor_over` as `null` placeholders on every path so an early return
still carries the cells the gate requires, then overwrites them when the search runs.
`observe` was a `push`, so it did not overwrite: the shipped Markdown printed "the
search did not run on this path" **nine lines above** `max_reliable_baud: 3000000`.
Which value a reader got depended on their idiom — last-wins reading into a map,
**first-match for the `[…] | first` spelling `expectations/*.jq` itself uses**. P14 is
the only probe in the tree that does this and **the only one with duplicate keys in any
committed artifact, on both kernels** (27 observations, 24 distinct). `observe` now
replaces in place, keeping the key's declared position so the placeholder is overwritten
where it stands. **Neither digest moves**: `field_set_core` sorts and dedups leaf paths,
which is why this shipped through five commits and two kernels invisibly — that blindness
is now itself a guard, `the_field_set_digest_cannot_see_a_key_emitted_twice`.

**(2) A `supported` P14 that measured nothing passed the gate.** Found by mutation, not
by reading: strip P14's measured trio back to its placeholders in the committed
`7cf0338` artifact, leave `status: supported`, and `jq -e -f expectations/linux.jq`
returns **0**. The presence clause deliberately admits `null` — an unfinished search
must be able to say so — and nothing complemented it. `p14_verdict` already refuses the
combination (it degrades whenever either cell is `None`), so the new clause pins a
property the probe **promises** rather than an answer it might give: no honest report can
trip it, it names no number and no `ceiling_kind`, and plan §3 rule 14 is untouched. It
reads through `last` so the frozen artifacts, which carry the pre-repair duplicates, are
still read at their answer (§16.13 leaves them untouched). The probe's own guard could
not have caught this: it exercises the `baseline_ok: false` and empty-ports arms, neither
of which reaches the search block.

**(3) The P4 population clause passed on type confusion.** It spelled its test
`(… // -1) > 0`, and **jq orders strings above numbers**, so a `canonical` of `"2"` — or
of any string — compared greater than `0`. That is the same defect class the clause was
written for (§3.45 (ii)): a population assertion blind to a population it does not
recognise. Now type-checked and read through `last`. Both new clauses are proven by
planted violation in every spelling they claim to cover, with a positive control on each,
and **no verdict changed across all 55 committed artifacts**.

**(4) Five printed consequences told a 6.18 box to diff against itself.** P6, P7, P8, P9
and P10 all ended "diff … against the production kernel (6.18)" — correct prose on the
7.0 dev box, circular on the kernel the sentence names, and invisible to every gate and
both digests because no `.jq` clause and neither fingerprint reads a consequence. They
now say "the other kernel of record (6.18 or 7.0, whichever this run is not)".

**(5) P5's handshake verdict was computed from four of the six crossings it prints.**
"DTR moves nothing" is a claim about both directions against all three inputs, but the
probe measured A→B against DSR/DCD/RI and B→A against **DSR alone**. A rig wiring B's
DTR to A's DCD would have read `false` on every published cell while the sentence
asserted a negative never asked in that direction. Both crossings are measured now, the
fold reads all six, and the line prints eight cells. **Measured on the bench rig, and
the pre-registered reading held exactly**: `dtr_b_to_dcd_a=false dtr_b_to_ri_a=false`,
verdict unchanged at `5-wire crossover`. **Neither digest moves** — the handshake is one
string cell, so the two new crossings live inside a value, which is precisely the
"changed a number without changing a key" blindness §15.44 names; announced by hand here
because no digest can announce it. The guard raises each of the six DTR cells alone and
requires each to lift the verdict, so a cell dropped from the fold reddens on CI with no
rig; fail-first against the four-cell fold names `dtr_b_to_dcd_a` exactly.

**Bounds, stated rather than implied (§9).** The two new B→A cells have exercised only
their `false` arm — nothing on this bench wires DTR — so `true`, `stuck-high` and
`inverted` are untested there, as they already were for the four existing cells on every
committed artifact of both kernels. And a **transposed** read (`read_ri` where `read_cd`
was meant) is invisible to CI *and* to this rig, since both answer `false`; the two lines
are character-for-character mirrors of their A→B siblings, and that is the whole
mitigation.

**(7) P15's `question` cited §15.51, which is P14's section — fixed, and the decline
that guarded it is explicitly overturned.** §3.68 filed this rather than fixing it, on
two grounds: no design entry for P15 existed, and correcting the string moves
`probe_set`. **The first ground is gone** — design §15.53 landed in `b8e4d8f` and its own
text says "§15.51's entry carried P15's citation because this entry did not exist yet
(notes §3.68 filed the debt); **it is discharged here**" — so the design had already
recorded the debt as paid while the code still carried it, which is the disagreement §5
exists to resolve in the design's favour. **The second ground is overturned by decision
rather than by evidence, and that is recorded as what it is** (§5): an era boundary was
being treated as more expensive than a wrong citation, and the judgement is that a
doctor worth collecting measurements from across every box is worth an era. The
correction is cheap *now* and gets steadily dearer, because every capture taken under a
wrong citation is another artifact stranded on the wrong side of the eventual fix.

`probe_set` moves **`82a8e2198e54626a` → `e79f5fcd86a2e5f0`**. What that costs, stated
plainly rather than minimised: the 6.18 Markdown report, the `7cf0338` Linux triple and
the `acb5162` macOS triple are now a **closed era**. They stay perfectly comparable *with
each other* — the 6.18↔7.0 pair this session established is untouched, and it remains the
first lawful cross-kernel Linux comparison in the tree — but no future capture joins
them, and the P1–P14 cells must not be diffed across the boundary without the mismatch
stated. §15.44's first digest is doing exactly its job here: the unequal direction is a
verdict, and it is now announcing a real instrument change rather than a cosmetic one.
The `field_set` half of this session's work (`a02e07f3f25e45b9`) moves for its own
reasons — P9's five new order-control cells — and the two movements are independent.

**(6) P9 documented a within-group order control and never published its outcome.**
`P9_REF_MASKS`'s empty-mask cell runs last at each fd state, so a monotone warmup would
leave it the cheapest cell in its group; `mask_role` has told readers that since the
probe was written, and nothing said whether it held. Same class as §3.50's "a `0`
printed with nothing beside it that says what the `0` means". It reports now, as
`order_control_says` plus a per-group cell and the tolerance it used.

**The obvious design was wrong and the corpus refuted it before it shipped.** A
rank-based "is the last-run cell the minimum?" reading fires on **20 of 27** committed
Linux 7.0 reports, on deltas of 0–3 ns out of ~260 — it would be a false-alarm generator
on the platform of record. So the reading is **magnitude-gated and per-group**: across
the 36 committed artifacts carrying the cell, the widest spread attributable to noise is
1.279x, while Linux 6.18's not-ready group at `883/418/418` is **2.112x**, larger than
all 72 committed group readings and monotone with the last-run cell tied cheapest — the
exact signature the control exists to catch. `P9_ORDER_TOLERANCE_X100` is 150, published
as a field so a later capture can re-derive the threshold rather than re-argue it, and
guarded to stay inside `128..=211` so a corpus that moved would redden rather than
silently re-fit. **This also corrects a reading in this session's own assessment**: the
6.18 rank *flip* between groups was called a disagreement, and it is corpus-normal noise
— what is singular there is the magnitude, not the order.

**The Darwin arm is the one that had to be got right, and it is guarded.** Its hung-up
group spreads ~11x, which is the mask being a real *level* (`revents: none` against
`POLLHUP` on the same fd), not instrument drift — so a group is `not-comparable` unless
`hangup_delivered_to_a_mask_that_requested_nothing`, and reporting a kernel difference
as a warmup artifact is structurally impossible. **Pre-registered and then run against
every committed artifact**: 27 Linux reports read `flat`/`flat`, 9 macOS reports read
`flat`/`not-comparable`, 19 predate the cell, and 6.18 reads `warmup-not-excluded`. All
four per-group values are exercised by a pure test — `warmup-refuted` appears in no
capture that exists, so nothing else covers it. `flat` is stated in the cell itself as
**not a strong pass**: on the platform of record the control has never discriminated,
and calling that "the control worked" would rebuild the defect one level up.
`field_set` moves and `probe_set` does not; the gate clause is presence-and-type in both
files, proven by mutation to bite on absence and to accept all five answers.

**One instrument error of this session's own, recorded rather than buried.** The first
attempt to prove the P4 string hole ran the *macOS* gate against a Linux artifact,
because the shell variable holding the "old" gate had been overwritten by an earlier
loop, and reported "old gate: rejected" — i.e. no defect. Re-run with the right file, the
old Linux gate **accepts** the string. The hole was real; the first measurement of it was
not. §8's rule about reverting the specific fix in place applies to gate files too.

### 3.74 Two renderings, one measurement: the doctor's Markdown and JSON twins

The question asked was whether the doctor needs both a Markdown and a JSON rendering,
and if so whether each is in good shape. **Both are needed, neither can be derived from
the other, and the defect was never in either renderer — it was that no single
invocation produced both.**

**Why both, measured rather than argued.** The Markdown is a pure function of the JSON
model minus exactly one field (`generated_unix_ms`) — established by transcribing
`to_markdown`/`render_value` and validating the transcription against real output at
228 of 228 passive lines and 290 of 291 Tier-3 lines, the one difference being a genuine
measurement drift between the two runs. The reverse does not hold: of 1064 Tier-3 scalar
leaves, **0 carry their JSON kind**, and `expectations/linux.jq` has **22 `type ==`
clauses** plus 5 `has()` clauses that turn on exactly that. Proven by mutating one cell's
*kind* in a real capture — `P4.canonical` `2` → `"2"` flips the gate to exit 1 while the
rendered Markdown stays **byte-identical** and `field_set` does not move. So the JSON is
the artifact of record and the Markdown is a view of it. Deleting the Markdown would
remove the thing that let a human spot P14's duplicate-key contradiction by eye when
neither digest could (§3.73), and would raise the floor on who can file a useful report —
§3.71 records a field report whose sender could produce a doctor report and not one test
result. Eight distinct JSON values were demonstrated to render to byte-identical
Markdown, including `2`/`"2"`, `null`/`"null"` and `true`/`"true"`.

**The defect. `--json` and `--markdown` were mutually exclusive by construction** — the
latter was `let _ = cli.markdown` — so the documented support-request path produced a
paste no gate could read, and obtaining the JSON meant running the probes **again**. Two
runs of one rig are not one measurement: on the bench P10's
`bytes_accepted_before_eagain` reads 11776 and 13824 across consecutive runs and P9's
medians move a nanosecond or two. So "the Markdown and the JSON of the same capture" was
not a thing that existed, in the protocol or in CI. `--json-out <path>` writes the JSON
twin from the same `Report` value; verified on the rig, the twin and the paste agree on
`field_set` (`5d99bdc231f8376d`) and on P10's volatile cell (13824 both), and the twin
passes the gate. `--json --markdown` now conflicts rather than silently resolving to
JSON.

**The header sentence, which is what actually cost three visits.** It said "paste this
whole report into a support request" and nothing more. Three consecutive Markdown-only
6.18 captures are three people doing exactly what the tool asked. It now asks for the
JSON too and names the flag. `docs/serial-nexus-doctor.md`'s one-shot protocol took the
Markdown as a *second* invocation and now uses `--json-out`.

**`--field-set` had a hole of the shape §3.50 keeps finding — a confident answer that
means nothing.** A report whose probes carried no observations produced a well-formed
16-hex-digit digest and exit **0**, so two such reports compared *equal* to each other;
`field_set_of_report_json` returns `None` on an empty triple set now, which routes to the
existing "not a doctor report" arm and exit 2. Unknown is never "equal". And
`--field-set` on the doctor's own Markdown twin answered with a bare serde error; it now
names the cause and both remedies, which is the message the operator most likely to hit
it will see.

**The Markdown renderer had one test and it was vacuous past the Build table.**
Everything after the first fifteen lines — every environment row, every probe section,
every observation, the summary — could have vanished with the whole gate set green, on
the rendering three field visits have produced and the JSON's 26 matcher-proven gates
never touch. `the_markdown_document_has_the_shape_a_reader_is_promised` asserts the
document's shape against a fixture built to carry the hazards: a `|` in an env-check
name and value, a `|` in a skip reason, and observations of every JSON kind including a
nested object and an array of objects. It checks the four section headers in order, that
**every table row has its header's unescaped-pipe count**, that every probe reaches the
document with its question and consequence, that nested leaves survive flattening, and
that the summary agrees with the verdicts — with an anti-vacuity control proving an empty
report does *not* satisfy the probe assertions. Fail-first: reverting the env-name
escaping reddens the column-count assertion, and rendering only the first probe reddens
on `P2 section missing`.

**Two things the guard settled rather than assumed.** A `|` in a *probe heading* is
deliberately **not** escaped — a heading is not a table row, and escaping there shows the
reader a stray backslash — so the guard asserts the escaped form in the env table and the
bare form in the heading. And the summary counts **environment checks and probes
together**, which is why a report's "24 supported" exceeds its probe count; that
arithmetic had been queried and is now pinned by a test.

**Also fixed: `md_escape` was applied to one of the Environment table's three columns.**
`name` reaches operator input through `access:<port>` and `--dev-root`, so a path
containing a pipe broke the table. A half-applied escape is worse than none, because it
reads as deliberate.

**And a documentation defect with no runtime effect, stated because the reverse would be
easy to assume.** `AGENTS.md` §3 spelled the doctor gate `jq -f expectations/linux.jq`,
without `-e` — and without `-e`, jq prints `false` and exits **0**, so the command reads
as a gate and asserts nothing. Measured against a deliberately-gutted report: exit 0
without `-e`, exit 1 with it. **CI always had `-e`** (`.github/workflows/ci.yml:195` and
`:251`), so no lane was ever blind and nothing shipped unchecked; only the line a human
copies was wrong.

**Filed, not fixed.** The Markdown value grammar is non-injective by construction —
`", "`, `"="`, `"["` and `"]"` are unescaped inside values, so `{"note":"a","b":"c"}` and
`{"note":"a, b=c"}` render identically. Quoting or escaping them would change the
rendering of every committed `.md` artifact, and §16.13 freezes those; it wants a design
note before a patch. A heuristic parser already recovers 483 of 483 Tier-3 leaf paths and
reproduces the printed digest, so the practical loss today is zero — but "recoverable by
a heuristic" is not a contract, and it should not be mistaken for one.

---

### 3.75 The v16 alignment pass, executed: nineteen deviations found, and the codec-author surface closed

**Design:** §15.54 (new, amend-first per §5), §5 (wiring invariant, loss taxonomy), §7.2, §8, §11.
**Plan:** §18 items 32, 33, 34, 35, 37, 39, 40, 43, 44 and 23; plan §3's env-var table; the Status
table's rig-lane scope.

The v16 pair landed 2026-08-10 as a full rewrite whose alignment obligation was met *structurally*
— a disposition map and ten blind verifiers over the documents. This session ran the other half of
that obligation: the pair against **the tree**. Six readers took a region each and checked 460
normative clauses against the code; each candidate went to an adversarial verifier that had read
the claim and not the reasoning. **Nineteen survived, two were refuted.** The refutations are as
load-bearing as the findings (AGENTS §9) and are recorded at the foot of this entry.

The shape of what was found is worth stating before the list, because it is not the shape a
rewrite is usually feared for. Not one finding was a rule the rewrite dropped. Fourteen were the
opposite — **prose that outran the tree**: an absolute where the code has a documented exception, a
figure re-cited rather than re-derived, a citation pointing one section off, a comment stating a
constant the gate does not assert. Two were real product defects, one of them shipping. The
rewrite's own invariant 2 held; what did not hold is that a document nobody executes drifts from
the tree in the direction of *sounding* more settled than it is.

#### A. The two product defects

**A1. The web console resolved a socket the daemon never binds.** §10 says the control-socket path
policy has "exactly one implementation". It had two: `web/src/rpc.rs` carried a copy with **two**
arms where the policy has three — it never asked `geteuid`, so it read `XDG_RUNTIME_DIR` first and
handed every process without a usable one the **root** arm `/run/serial-nexus-daemon.sock`, while
`serial_nexus_rpc::default_socket_path` returns `/tmp/serial-nexus-daemon-<uid>.sock`. That is every
unprivileged macOS session (no `XDG_RUNTIME_DIR`, no `/run`) and any stripped service environment
exporting the variable empty; the console reported the daemon unreachable at a path nothing on the
machine will ever create, under a `--help` line promising "defaults match the daemon, §10". All
eleven web spawns in the harness pass `--socket`, so no test could see it. Fixed by deletion —
`default_socket` is now one line delegating to the single implementation — and guarded by
`itest/tests/p12_web_socket_default.rs`, which reads **two real processes** (the doctor's computed
`daemon socket (default)` row and the line `serial-nexus-web` now logs before it binds) across three
environments, because the policy's discriminators are `geteuid()` and one environment variable and
mutating either in-process is `unsafe` (§16.3) and racy under the parallel runner. The guard also
requires the case set to have **discriminated** — unprivileged, the three cases must land on two
different arms — so it cannot pass by comparing one arm with itself. Fail-first: with the two-arm
copy restored, the `unset` and `empty` cases fail naming both paths; the `set` case passes either
way, which is exactly why it cannot be the only case.

**A2. A duplicated serial number was refused with a remedy that does not work.** §12 clause 4
requires the bare-serial ambiguity refusal to name every device *and the identity that pins each*.
It printed each device's `usb:` identity — in the two-clone shape, one string both devices answer
to, printed twice — and then advised "add by one of the identities listed here", which walks the
operator into §15.10's resolution-side ambiguity guard: the add succeeds, the identity binds
nothing, and the node waits forever with no further remedy named. It now offers what
`capture_for_dev` would store for that device — the same fallback chain `add-node <path>` and
`ports` read — so the identity offered is exactly what binding that device's path would bind, and
where the chain bottoms out at `raw:` the message says so rather than dressing a path up as an
identity.

#### B. The gate holes

**B1. Three doctor probes spelled an error `skipped`, and `skipped` is what the gate exempts.**
§13 is categorical: `skipped` is never an error path's output. P8, P9 and P10 routed their
catch-all `Err` arm there, and because the jq clauses exempt a skip, a probe that *errored* left
`jq -e -f expectations/linux.jq` at exit 0 — the P9 discriminator clause and the two P10 content
clauses going vacuously green exactly when the measurement they exist to guarantee was never taken.
The same failure spelled `degraded` exits 1. This is §9's vacuity taxonomy catching the instrument
that enforces §9.

**B2. `serial-nexus-itest` never carried `#![forbid(unsafe_code)]`, and two of its own files said
it did.** §16.3 and §5's tripwire table say `unsafe` lives only in `serial_nexus_sys` and
everything else forbids it. The harness crate — its `src/lib.rs` plus every file under `tests/`,
each its own crate root — carried the attribute nowhere; the invariant rested entirely on a grep
that cannot see edition-2024 `unsafe(...)` attributes or macro-generated blocks, while comments in
`p12_pty_setup.rs` and `p12_sim_idle_cpu.rs` told a reader checking the invariant that the
attribute was present. **92 of 95 crate roots** were missing it (three already had it), the sweep
compiles clean — nothing in the harness ever used `unsafe` — and the drift is now gated by
`every_crate_root_forbids_unsafe_except_the_one_that_may_not`, which enumerates crate roots from
the `crate_dirs` walk rather than from a list and checks **both** directions: the attribute must be
present, and the `#![allow(unsafe_code)]` exception must be exactly `sys/src/lib.rs` and nothing
else, because §16.3's whole value is that the answer to "where is the `unsafe`" is one directory.
The gate proved itself the minute it existed: it reddened on `itest/tests/meta_derive.rs`, a file
written by another agent in the same session and forgotten in exactly the way the gate exists to
catch.

**B3. The documented rig lane could not go green on the rig it describes.** Every normative
spelling of the lane — AGENTS §3, the plan's Status scope list, and the command `scripts/bless`
prints — set `SNX_REPLUG=required` without naming `SNX_REPLUG_DEV_B`. Without it,
`identity_survives_a_replug_that_renumbers_the_tty` takes its `skip_no_replug` path, and the
required-mode assert turns that legitimate skip into a **hard failure**. The variable was
documented (elided as `_DEV_B` in two places, which is why a literal grep missed it and one
auditor's stronger claim was refuted) — it was missing from the three places an operator copies.
Plan §3's "Three further variables are parameters" was also an undercount: there are four.

#### C. The design amendments (§15.54)

Two findings were the design being wrong about code that is right, which AGENTS §5 makes an
amendment rather than a patch.

§5's wiring invariant states three endpoint states with **"not attached parks"** as an absolute.
The leg's channel pump and the exec codec's mux route deliberately **drain and count** instead, and
say why in place: a leg's channels share one socket, so a parked channel head-of-line blocks a
cross-machine link (§9); the exec mux route runs inside the child's single stdout decode loop, so a
parked mux event stalls every hostward channel queued behind it. The codec, map and pty park as
written. The rule the tree actually follows is **park where the stall is confined to the endpoint
whose edge was removed, drain where one pump serves many** — and it was stated correctly in the v12
session log, with that exact qualifier, which was dropped when plan §14 promoted the invariant into
§5's body. Recorded now as §15.54, with the sentence corrected in place and the reason it went
uncaught named: nothing in the tree moved.

Its twin: §5's loss taxonomy asserted that "the only counters that can ever move for targetward
bytes are the purge family and the teardown ledger". Four shipped counters falsify that on a
*healthy* graph — `discarded_targetward` (codec, exec), `discarded_no_raw_edge` (map),
`discarded_unframable`, `discarded_peer_gone` — and the table had no row for the family. An
absolute that makes its own instances unreadable is worse than no rule: a reader checking the tree
against §5 would have found counters moving and no class to file them under. The table gains a
**pump-side discard** row and the paragraph is narrowed to targetward *delivery* loss.

#### C2. A deferral that refuses in a different shape than the vocabulary says

§14 entry 15 (`existing-terminal`) is tagged *refused-at-load*, a state whose definition includes
"the refusal is live, **tested** behavior — never a silent no-op". Nothing tested it: the refusal
fell out incidentally of serde's internally-tagged `NodeConfig` enum and was asserted only by a doc
comment, while its sibling entry 14 has a guard. It is now guarded — and writing the guard is what
found the second half. The refusal is real (nothing is created, and the shipped node kinds *are*
listed), but it is **not** entry 14's shape: `existing-terminal` is not a `NodeConfig` variant at
all, so serde refuses the tag at `INVALID_PARAMS` rather than `GraphConfig::validate` refusing it
structurally with the deferral named. §7.7's "the same treatment §7.1 gives the serial output leg"
was therefore half-true, and the vocabulary's "the schema admits the words" was false of this entry.

Dispositioned as a decision rather than a patch, both halves recorded so neither is silent: the
design now states **both** shapes under `refused-at-load` and says which entry has which, and the
upgrade — a variant refused in `validate` so the two deferrals read alike — is **plan §18 item 45**.
The guard asserts *today's* behaviour on purpose, so landing item 45 reddens it loudly instead of
passing unnoticed.

#### D. Figures and citations

**D1. §7.2's pts-queue depth was not the figure its artifact carries.** The design quoted
"13.8–15 KiB recovered across the committed P10 captures" and cited
`linux-7.0-2026-08-05-tier3.json`, which reads a flat `bytes_recovered_by_peer: 15360` in both
directions. Worse, "13.8–15" is not one unit: 13.8 is 13824 bytes counted in **KB**, 15 is 15360
counted in **KiB**, and no capture reads 13.8 KiB (14131 B). Re-derived across every committed
Linux 7.0 P10 capture: **13824–15872 bytes**, varying run to run and direction by direction, with
§15.46's cited triple spanning 13824–15360. §15.46 was already right; a v15-era un-artifacted
number had been re-cited rather than re-derived at the rewrite — the exact move §16.13 and AGENTS
§7 exist to stop, and `doctor/src/probes.rs` bans the same string from the shipped report.

**D2. §11 over-stated the destroying-verb ledger.** It said `teardown`, `remove-node` and
`load --replace` all reply with `torn_down` and `discarded_at_teardown`. `remove-node` carries no
`torn_down` and never has — it removes one node, so a node count would say nothing; its pair is
`cascaded_edges`, and `docs/rpc/configuration.md`, the schema authority, documents no such field.
§5, §15.50 and every shipped guard scope the pair correctly. Corrected in the design, not on the
wire.

**D3. Thirteen sites cited §7.1 — the serial port node — for a rule that lives in §6.** The leg's
implicit-acquire / idle-release / release-on-peer-disconnect rule is §6's "Write modes" clause 2,
and §7.4 correctly refers back to it; eight sites in `nodes/leg.rs` and five in
`p9_leg_arbitration.rs` sent a reader to a section containing no write-lock, idle, or
peer-disconnect rule at all — one of them putting quotation marks around a sentence §7.1 does not
contain, and one citing both sections for the same rule in one breath.

**D4. Two more comments that were claims.** `daemon/src/lib.rs` pointed at
`serial_nexus_core::socket` — a module that does not exist, and a placement a meta-gate has already
refuted — one line above the call that correctly uses `serial_nexus_rpc::default_socket_path`; that
is a *fourth* description of the policy §10 says has one implementation. And `ci.yml` told
operators the Playwright lane holds the suite to floors of 19/10 specs where the gate's constants
are 22/12 — a pair ci.yml has never stated correctly.

**D5. `set_no_new_privs` was attempted, not established.** §15.45 lists
"`PR_SET_NO_NEW_PRIVS`-guaranteed" among five bounds holding *by construction* on the
capability-carrying replug helper, and `devprep/src/linux/mod.rs` discarded the prctl's result. A
failed prctl left a blessed process running unhardened and unreported. "By construction" is a
promise about a `Result` nobody read.

#### E. The exec battery told a deadline as a loss

§8 promises the battery "reports a failed check with the deadline named on stderr", and plan §3's
standing rule is that the harness marks `timed_out` so a deadline is never read as a drop. Only the
*stdin-drain* deadline was ever named. The per-frame **echo** deadline that all four core checks
gate on returned a silent `Recv::Timeout` that every caller collapsed into a bare `false`, with no
discriminating field in the verdict — so "your codec did not answer in 2000 ms" and "your codec
lost frames" arrived as byte-identical verdicts with empty stderr. The checks now return an outcome
carrying `pass`, `timed_out` and a `detail`, `Recv` keeps `Timeout` and `Closed` apart, and the
verdict carries `timed_out` and `details`. A guard pins both directions: the half-duplex fixture's
failure must name the deadline, and a *passing* run must name none — without the second, the first
would pass against a verdict that named a deadline unconditionally.

Beside it, the `envelope` mode's stdin write was an unbounded `write_all` on a detached thread
whose only bound was a **total** 5000 ms deadline on the child completing — while §8 states the
idle deadline as a contract over *the battery*, in the paragraph that has just named both modes. A
correct-but-slow child failed there at the wall clock its sibling passed it at. Both modes now
drive the child through one boundary.

#### F. The codec-author validation surface, closed

§8's "Validating your codec" chapter named its own gaps and filed them rather than promising them.
Six of the eight shipped this session; **items 36 and 38 stay open** and are the whole of what §8
still defers.

* **Item 32 — `attributes_are_structural`.** Generic over the table type, so the kit stays
  dependency-free and never names a TOML crate. Every good table builds; every bad one returns
  `Err` **without panicking** (a panic is not a structural refusal — it unwinds through the daemon
  instead of aborting the load with a message an operator can act on) and names the key it refused.
  Four negatives prove each arm bites: lenient, unwinding, and anonymous-refusal schemas. Three
  consumers run it — the **exec** codec's schema (the richest in tree), the built-in `reference`
  factory, and `tinymux` from the consumer position — so `precheck_codecs`' promise is now proven
  from the position that depends on it rather than asserted about it.
* **Item 33 — `recovers_after_garbage`.** Clause 6's codec-side half: after one refused frame the
  next valid one decodes whole. `LatchesOnError` is its negative and its point — a decoder that
  drains correctly, passes every other suite in the kit, and fails only this one. Its stated limit
  is part of the suite: it injects an **envelope** frame with an unknown type byte so a
  length-guided resyncer skips exactly `4 + body_len`, and it deliberately does **not** assert
  re-alignment after unaligned noise, because where a correct resyncer re-aligns depends on the
  noise and a suite demanding one answer would fail correct codecs. That is `lag.py`'s lesson,
  applied inside the kit.
* **Item 34 — the exec battery's error paths.** `--error-paths`, opt-in, three arms: an unknown
  type byte at offset 4, an oversize length prefix at offset 0, a channel length overrunning its
  body at offset 5. Each requires the child to **terminate**, to not echo the fault back as a valid
  frame, and to **signal** the refusal — a non-zero exit or an `error` event, because a child that
  swallows a fault and exits 0 is indistinguishable from one that never received it. `strict.py` is
  the new positive control and the shape to copy where truncation must be reported; `passthrough.py`
  fails all three, which is both the fail-first proof and the honest statement that a permissive
  relay is legal but visible — this battery's `Hoarder`. The format's other two decode errors
  (non-UTF-8 channel identity, non-UTF-8 `error` reason) are deliberately not injected: the
  harness's own encoder cannot express them, so a hand-rolled frame would say more about our byte
  poking than about the child.
* **Item 35 — the demux shape.** `--mux-to <channel>` declares the swap a real demux performs, and
  the *whole* battery then runs against the codec an author ships rather than a passthrough build
  of it — the largest author-experience hole in the exec route. The golden battery gains one frame
  in each direction of the declared mapping (so both halves are exercised whatever channel is
  named), and liveness, fragmentation and restart drive the multiplexed side. Fail-first is the
  same fixture without the flag: `passthrough-codec.py console` fails `golden`, because every
  `console` frame comes back on the empty channel. Identity mode is byte-compatible — the map is
  the identity function in every method and an undeclared run carries no `mux_to`.
* **Item 37 — the author guide, executed.** Five guards in `meta_codec_authors_doc.rs`, every
  expectation **parsed from the document**: a table lookup would be a hand-kept copy of the doc's
  contents, and the first row someone added would be the row the gate never checked. The four
  frozen hex vectors and the worked `data("", "AB")` example must equal `encode()`; the §5 TOML
  block loads against a real daemon (four paths rewritten, each asserted present so a renamed one
  cannot silently leave the original in place); every counter the closing paragraph names must be
  one a graph carrying **both** codec kinds reports. Three mutations executed and reverted — one
  hex digit in the table, one in the worked example, one invented counter — each reddening exactly
  one guard. A fourth guard is the fail-first for the third: stripping the multiplexed
  `write_mode` the doc spends a paragraph on must be refused naming the edge.
* **Item 39 — the second template codec.** `tinymux-codec`: a two-channel tag framer
  (`tag|kind|len|payload`) with parser state, byte-wise resync, and one attribute. It exists
  because `acme` is a passthrough and exercises none of `control_event_round_trip`,
  `assert_buffer_bounded`, or `attributes_are_structural` — the three suites now have a consumer
  built from the consumer's position on every push. Deliberately **not** an envelope codec: a
  device's own framing is the commoner case, and this shows what it costs. `info.codecs` moves to
  `["acme", "exec", "reference", "tinymux"]` in the gate and the template README together, and the
  gate additionally asserts that a bad attribute table is refused **naming the key** with nothing
  created — §11's pre-create precheck, observed from outside the tree.

#### H. The derive-from-tools gates (item 40), and one more hand-kept list

AGENTS §3's closing rule — "CI enumerates loops from tools, never hand-kept lists; meta-gates
assert *execution*, not file existence, and a scanning gate proves its **matcher** as well as its
walker" — was doctrine with three of its four instances ungated. `itest/tests/meta_derive.rs` is
the four gates, each enumerating both sides from their real sources and each proved fail-first
against a planted stale entry.

Two of them found something on arrival, which is the whole argument for the item:

* **The probe roster in `doctor/src/main.rs`'s module doc was three probes stale** — it stopped at
  P12 while the registry holds P15 — and nobody had noticed. The item sanctioned either repair;
  **deleted** was the right one, because `docs/serial-nexus-doctor.md` is the registry of record
  (§13) and a second roster in a module doc buys nothing it can keep true. That documented roster
  was itself missing P15 until this same session (item 44), so both copies had drifted, in
  different directions, at once.
* **The fuzz gate that already existed was matching the wrong side.**
  `every_unstable_fuzz_api_export_has_a_fuzz_target` enumerates targets by listing
  `fuzz/fuzz_targets/*.rs`; CI's nightly loop iterates `cargo fuzz list`, which prints
  `fuzz/Cargo.toml`'s `[[bin]]` names. An unregistered `.rs` file therefore answered "yes, a target
  drives it" to the gate while never being built and never being fuzzed. The new gate is the
  bijection between manifest and corpus, and it composes with the old one rather than replacing it
  — stated in the file so nobody re-adds the duplicate. The manifest is *parsed*, not obtained by
  shelling out: `cargo fuzz` needs nightly and lives only in the scheduled lane, so a gate that
  called it would self-skip on every developer box and in every push lane, which is the vacuous
  green the `required` lattice exists to prevent. Narrow parsing is safe in the only direction that
  matters here, because the assertion is a bijection — a registration the parser misses leaves its
  file unregistered, which is loud.

A third enumeration was closed on the verifier's report rather than merely named: §13 rule 1's
prose says "**The probe roster is P1–P15**", which is a hand-kept list wearing two numbers and the
form most likely to survive a P16 unnoticed — a new row lands in the tables because the author is
looking at them, and that sentence is a page away. Gate (b) now reads it against the same registry
count, fail-first proven by planting `P1–P14`. Two enumerations remain ungated and are said rather
than implied in the gate's own header: `expectations/{linux,macos}.jq` each carry a per-probe
`.id == "PN"` clause, so a probe with no clause is ungated there — a gate-coverage question, not a
roster-drift one.

The same verifier caught two sentences this session's own repair falsified. The gate's header said
the roster "appears three times outside `probes.rs`" and named the parenthetical the same unit of
work had just deleted; and the replacement comment in `doctor/src/main.rs` said the roster "lives
once", which the design's §13 glance table — checked by the very gate cited two sentences later —
makes false. Both corrected. Worth recording rather than quietly fixing: a gate against hand-kept
lists shipped with a hand-kept count in its own header, which is the failure mode arriving through
the front door.

And the fifth gate, written beside them for B2:
`every_crate_root_forbids_unsafe_except_the_one_that_may_not` (above). It caught two things the
hour it existed — `meta_derive.rs`, written minutes earlier and missing the attribute, and then
**itself**: the detector's own `#![allow(unsafe_code)]` literal made `meta_gates.rs` read as a
second exemption. Both needles are now built by concatenation, the same dodge the planted-`unsafe`
sample beside it has always used, and the episode is left in the comment because a detector that
matches itself is the failure mode this file exists to keep teaching.

#### I. The gates, and one measurement that was the box rather than the code

**Green at default CI scope: 890 passing · 0 failed · 6 ignored**, 121 test-result lines over 117
cargo targets — with build, both clippy lanes, fmt, `cargo deny`, both Apple triples, and the
doctor jq gate. The **12 self-skips** in that run are measured with `--nocapture`, not asserted
from a default-capture log where the count is 0 whatever skipped (§3.78). The figure and its scope live in the plan's Status table; the
`ignored` moved 4 → 6 because the two new kit suites carry ```ignore` doc examples like their four
siblings, which is a documentation fact and not a coverage one.

**`p3_idle_cost` is load-sensitive at its ceiling, and the first readings were not evidence.** The
session's first suite run failed it at **3.60 %** of a core against the 3.50 % drift ceiling, and
two immediate re-runs read **5.50 %** and **4.30 %**. Measured the box before the code (AGENTS §8):
a second Claude session, two `rust-analyzer` instances holding ~4 GB between them, a
`vmcell build-kernels`, and this session's own 27-agent workflow fan-out. Solo on the quiet box
afterwards: **3.20, 2.90, 2.90, 3.00, 3.00 %** — 5 of 5 under the ceiling.

**But that does not settle it, and the honest record is that it failed again with the box to
itself.** Two full `cargo test --workspace --no-fail-fast` runs on the quiet box, back to back: the
first green, the second **3.80 % and red**. The suite runs test binaries in parallel, so the 10 s
measurement window overlaps ~20 other targets — the contention is the suite's *own*, not the
box's, and no amount of quiescing the machine removes it. So:

* it is **not** a regression this session introduced (it failed before the first commit, on the
  unmodified tree);
* it is **not** explained away by external load, which is what the paragraph above would have
  claimed if it had stopped at the solo readings;
* it is a guard sitting ~0.3–0.8 pp under a ceiling it is measured against, whose outcome depends
  on what else the runner is doing.

Deliberately **not patched**. Raising the ceiling is the shape this repository forbids — the
tripwire is 1.75× the committed artifact on purpose, and loosening it because it fired is silently
re-fixing a decision (AGENTS §5). Re-measuring `docs/benchmarks/phase3.json` is the other
disposition the test's own message names, and it is a decision needing a controlled box, not an
edit. Filed as **plan §18 item 46** with every reading above. CI's Linux `check` job passed on the
same commit, so no lane is red on it today; what is at risk is a developer's local
`cargo test --workspace` and the credibility of the next red it produces.

#### G. Refuted

Two candidates did not survive verification, recorded because a refuted diagnosis is as
load-bearing as a confirmed one (AGENTS §9).

* **"The pty node's `advertised_baud` has no structurally checked maximum, against §5's wire
  maxima."** The code facts were accurate as far as they went and the verifier still refuted the
  finding.
* **"`SNX_REPLUG_DEV_B` is documented nowhere."** False on this tree: two sites document it under
  the elided spelling `_DEV_B`, which a literal grep for `SNX_REPLUG_DEV_B` cannot match. The
  weaker, true claim — that it is missing from the three *lane spellings* an operator copies — is
  B3 above, and it is the one that was fixed.

---

### 3.76 The macOS lane's one red test asserted Linux's answer, and the doctor had already measured otherwise

**Design:** §7.2 (the EXTPROC re-assert), §7 (a kernel that differs is reported with the
observation named), AGENTS §9 (proxy in space). **Found by reading CI rather than the tree:** the
`macos` job had been the only red one for at least four consecutive runs, the v16 landing push
included, and its failure was a **single** test out of 116 result lines — which is exactly the
shape `--no-fail-fast` exists to make legible (§6).

**The failure.** `a_client_clearing_extproc_has_it_re_asserted_so_changes_keep_surfacing`
(`itest/tests/p12_pty_setup.rs`) tests §7.2's re-assert: a client rebuilds termios from scratch,
clearing EXTPROC, and the daemon's reconciliation sets it back so later changes keep surfacing
promptly instead of falling to the 3 s poll. Before clearing anything it *asserted* the baseline
had left the flag set — "the §7.2 baseline did not leave EXTPROC set on a fresh console, so
clearing it below would prove nothing". On Darwin `stty -a` answers `-extproc`, and it fired.

**The reading was right and the assertion was wrong, and this was already measured.** The doctor
probes exactly this property and the committed artifacts disagree across kernels by design:
`handler_reset_extproc_retained` is **`true`** in
`docs/doctor/linux-7.0-2026-08-07-2b44c17-tier3.json` and **`false`** in
`docs/doctor/macos-24.6.0-2026-08-05-1a9a8fc-tier3.json`. P6's own consequence prose says what
follows from it — "this kernel did not retain EXTPROC afterwards … so the EXTPROC-gated
`TIOCPKT_IOCTL` re-arm cannot fire at all" — and §7.2 already runs poll-only there. The guard was
asserting Linux's answer on both kernels: AGENTS §9's **proxy in space** at its plainest, passing
on the box it was written on.

**Why its own anti-tautology check missed it.** It had one, aimed at a different hazard: an `stty`
that does not *know* `extproc` would "clear" it silently and leave every later assertion trivially
true, so the test skips where `stty -a` names the flag in neither polarity. That check is right and
is kept. What it cannot see is the case where the tool knows the word and the **kernel** does not
keep the flag — a vocabulary check standing in for a retention measurement.

**The repair.** The second precondition becomes a measurement instead of an assertion: where the
baseline did not leave EXTPROC set there is nothing to clear and no re-assert to observe, so the
test **skips naming the reading and citing both artifacts**, which is §7's rule for a kernel that
differs. Keyed on the reading and deliberately **not** on `cfg!(target_os)` — a Darwin that starts
retaining EXTPROC must run this test, and a Linux that stops must redden rather than skip. Writing
the skip as a `cfg` would have reintroduced the same defect pointing the other way.

**Proof, and what it does not cover.** On Linux the test still *runs* — no SKIP line, 8 of 8 in the
file — so the skip does not fire spuriously and the guard is not vacuous where it bites. The Darwin
arm cannot be exercised on this box; its evidence is the CI log that produced the failure, which is
the same reading the skip now keys on (job 94135042408, 2026-08-12). An honest "not exercised here"
beats a proxy for it (AGENTS §9).

**What the same CI run also bought, unasked: plan §18 item 18's suite half.** That job is a
**whole-workspace macOS run at the current tree** — the thing item 18 records as owed, since
`cargo test` compiled on no macOS at `3e23c52`. It read **859 passed · 1 failed · 6 ignored** on the CI arm64 runner with the one failure being the
guard above, and **860 · 0 · 6** on the next run once it was repaired — the first green macOS lane
in this record. (Neither states a skip count: CI passes no `--nocapture`, so that log cannot
produce one — §3.78.) Every target this
session added ran there and passed: `meta_derive` 5/5, `meta_codec_authors_doc` 5/5,
`p12_web_socket_default` 4/4. The figure is quotable only with its scope — **whole workspace
(macOS)**, CI runner, not the x86_64 rig box — and it lands in the plan's Status table with that
scope beside it.

**And the capture half, made possible rather than done.** Item 18 also owes a macOS doctor
artifact, and both doctor jobs now take one run and gate on its twin (item 43) — but only the
Markdown was uploaded, so the JSON, which is the artifact of record (§3.74), was being discarded at
the end of every run. Both jobs now upload the pair. That does not discharge item 18's capture
half — a committed artifact is a deliberate act with an era stated, not a CI by-product — but it
means the artifact no longer has to be re-taken by hand to exist.

---


**Footnote, recorded so it is not re-derived as a fresh finding.** The run that turned the macOS
lane green failed `web-ui` instead, on
`graph-editor.spec.mjs:171 › adding a console through the editor makes bytes flow end to end` —
the **named** instance of §3.58's in-suite contention datum, which measured that same spec at 1 of
3 in-suite and 0 of 5 in isolation. The commit under it touched `ci.yml`, `AGENTS.md` and the notes
and nothing web-facing, and the same console code had passed `web-ui` in the two runs before it. A
re-run of the failed job alone, on the identical tree, went green. Its assertion differs from
§3.58's (`toHaveClass` rather than the status-line regex), so this is the same spec and the same
class, not the identical failure — stated that way rather than rounded to "the known flake".

### 3.77 The macOS jq gate had not been running, and both lanes were discarding the doctor's exit status

Two defects in one step, found by fixing §3.76 — which is the point worth keeping: repairing the
red test is what let the step *after* it execute for the first time in a long while, and it failed
immediately.

**1. A gate that was not running.** AGENTS §3 names `jq -e -f expectations/macos.jq` as the gate
"which the macOS lane actually gates on". It had not executed in **any** recent run. Every macOS
run since at least 2026-08-10 — the nightlies of 08-10, 08-11 and 08-12, and the v16 landing push —
failed at the test step, and a failed step skips the rest of the job, so the capability report and
its gate were reported `skipped` each time. Checked, not assumed: the step's conclusion is
`skipped` in all four runs. The macOS expectation file was being maintained against a lane that
never read it. That is the same class as the `jq -e` omission AGENTS §3 already records — a gate
that reads as a gate and asserts nothing — arriving through the *scheduler* rather than through the
command.

The general form is worth stating, because it will happen again: **a red test upstream of a gate
hides the gate, and the hiding is invisible in the failure**. The job reports one failure, the
failure is real, and every check behind it is silently untested for as long as the first one stays
red. `--no-fail-fast` (§6) buys this *within* `cargo test` and buys nothing across job steps.

**2. `| tee` discarded the doctor's exit status, on both lanes.** The doctor exits **1** when any
probe reports `unsupported` — §13's stop condition — and **2** when it could not produce a report
at all (`std::process::exit` at `doctor/src/main.rs`). Both were piped into `tee`, and a shell
pipeline's status is the *last* command's, so `tee` returning 0 discarded them; only the following
`jq` could fail the lane, and it can only speak to what reached the file. The `jq -e` defect in a
second costume. Both lanes now redirect instead of piping, so the status is the doctor's.

**What the first execution then reported, and what is not yet established.** With the test step
green, the macOS gate ran and could not exec the binary —
`target/debug/serial-nexus-doctor: cannot execute binary file` — 2.5 s after the step began, on a
runner where `cargo build --workspace --locked` had just succeeded and the harness had just booted
other binaries out of the same directory in the test step. That is `ENOEXEC`: not a signature
problem, not a permission problem, but the file not being a loadable image for this architecture.
**No cause is claimed.** The step now prints `uname -m`, `sw_vers`, `ls -la` and `file` on the
binary before running it, so the next run says what it is instead of inviting the guess — and it
prints on a *green* run too, so the evidence survives the fix. Recorded as an open observation, in
§7's sense: measured behaviour, mechanism unestablished.

---

### 3.78 "Zero SKIP lines" asserts nothing, and this record has quoted it as evidence for four generations

Found by disbelieving a green log. The macOS lane came back **860 passed · 0 failed · 6 ignored,
zero SKIP lines** — and the guard §3.76 had just converted from an assertion to a self-skip should
have skipped there, since the same runner had failed its assertion an hour earlier on the same
reading. A skip that leaves no trace in the log it is supposed to be visible in is either not a
skip or not visible.

**It is not visible.** `cargo test` captures a *passing* test's stdout and stderr and prints them
only for failures, and a self-skip is a passing test that printed to stderr. Measured on this box
against `serial_hardware`, which self-skips six times here with `SNX_CROSSOVER_A`/`_B` unexported:

* `cargo test -p serial-nexus-itest --test serial_hardware` → **0** SKIP lines;
* the same command with `-- --nocapture` → **6**.

CI passes `--nocapture` nowhere except the soak lane. So every "zero SKIP lines" quoted beside a
suite figure — the plan's Status table, this session's own §3.75 §I, and the runs the v15 record
took — was reading a constant. It is §13's vacuity taxonomy applied to the suite's own skip
discipline: a check that reads as a check and cannot fail.

**The real figure, measured.** One `cargo test --workspace --locked --no-fail-fast -- --nocapture`
on Linux at default CI scope: **12 self-skips**, from twelve distinct tests, every one accounted
for by a capability this box was not given — the five `crossover_rig_*` tests and
`rts_cts_flow_control_stalls_the_writer_instead_of_losing_bytes` (no `SNX_CROSSOVER_A`/`_B`), three
replug tests and `a_break_straddled_by_a_replace_leaves_the_line_transmitting` (unblessed helper),
and the two browser tests (no node). Nothing skipped that should have run — which is the answer
the vacuous check was always giving and never actually checking.

**Not repaired by making CI shout.** The temptation is `--nocapture` everywhere; it is the wrong
fix. The mechanism that makes a skip *fail* already exists and is the one plan §3 rule 11 built:
`SNX_*=required` turns a self-skip into a hard failure for the capability an operator says must be
exercised, and it does that whether or not anyone reads the log. A SKIP-line grep was never the
enforcement — it was a comfort reading beside it, and the honest repair is to stop quoting it as
evidence, which the Status table now does: the Linux row states 12 with the flag it was measured
under, the macOS row states **no** skip count because CI cannot produce one, and the v15-era row's
"zero SKIP lines" is withdrawn in place rather than deleted (§15.44's rule for a withdrawn figure).

**The general shape, because it is the third instance this session.** `jq` without `-e` (AGENTS §3),
a gate skipped because the step ahead of it was red (§3.77), and now a grep whose subject is
captured before it can be grepped. All three read as gates, all three asserted nothing, and none of
them was wrong about anything — they were silent about everything. The tell they share: **a check
whose passing output is identical to its not-running output.**

---

### 3.79 The blessed helper grants device access, at the repository owner's request

**Requested by the repository owner**, in these words: *"please design and implement an
extension of the blessed binary so that it also grant ownership to any USB /dev nodes needed
by tests… effectively enough capabilities to be able to do the equivalent of
`setfacl -m u:pwnall:rw` in Rust"*, together with *"have the bless script show and then run
the command, as opposed to just showing it"*. Recorded as a request rather than as a finding
because it is a deliberate widening of a privilege boundary, and the record should say who
asked for it. **Design:** §15.55 (new, amend-first per AGENTS §5 — AGENTS §4 makes this exact
change a design amendment rather than a patch).

**The problem it solves, measured the hard way.** The 2026-08-12 rig lane produced nine
failures. Eight had a single cause — `Permission denied (os error 13)` on reopen, six on
`/dev/ttyUSB0` and two on the `usb:` identity — and the cause was not the product. Device
access had been granted with `setfacl` on the device nodes, and the replug tests
**re-enumerate** the adapters: udev destroys the inode and builds a fresh one,
`root:dialout 0660`, so every grant made before a cycle is gone with the inode that carried
it. A lane that starts with access loses it half way through, and it surfaces seconds later
as a faulted node with nothing pointing at the cause. The ninth failure was `p3_idle_cost`
(plan §18 item 46), unrelated.

**Why the helper is the right home.** The re-enumeration is what destroys the access, the
helper is what performs the re-enumeration, and it is already the one blessed binary in the
tree. Anything else — a wrapper, a udev rule, a second privileged tool — either duplicates the
privilege or leaves a window in which the node exists and cannot be opened.

**What was built.** `grant --port <PORT>`, plus an automatic grant after every successful
reauthorization in `authorize`, `cycle` and `hold`, because restoring the access the
reauthorization destroyed is part of putting the device back rather than a step every caller
must remember. §15.55 states the bounds in full; the ones that were actually at risk:

* argv still carries a **port name**, validated by the same alphabet and the same four sysfs
  checks; the device nodes are derived by the helper from the kernel's view of that port, so
  **no caller-supplied path reaches `open`** — the bound AGENTS §4 warns a path-taking verb
  dissolves;
* the beneficiary is **`getuid()`**, never argv. There is no `--uid`, because a
  capability-carrying binary that takes one can hand privilege to any account;
* the node is **never opened** — `O_PATH` resolves the inode without calling the driver's
  open, so no DTR is asserted on the operator's equipment (§15.17), and the `setxattr` goes
  through `/proc/self/fd/<n>`;
* the inode **verified** is the inode **modified**: `O_NOFOLLOW` refuses a symlink and the
  character-device check runs on the fd, so there is no check-to-use window on a name.

**The cost, and the choice behind it.** A second capability: setting an ACL on a file you do
not own needs `CAP_FOWNER`, so the blessing is now `cap_dac_override,cap_fowner+ep`. An ACL
rather than `chown` deliberately — `CAP_CHOWN` is a strictly larger grant (it lets a process
give files away, not only take them), and chown would destroy the node's original ownership,
where the ACL leaves it reading `root:dialout 0660` with one extra `user:<uid>:rw-` line that
`getfacl` shows and `setfacl -b` removes.

**Proving the wire format without privilege.** The ACL blob is hand-encoded
(`system.posix_acl_access`, version 2, five entries in ascending tag order), and getting it
wrong is *silent* in the dangerous direction: a malformed blob is refused `EINVAL` and
noticed, but a well-formed blob carrying the wrong permission bits is applied exactly as
written. So it is checked twice. Four unit tests pin the encoding field by field — including
that `other` stays exactly what the mode said, which is where a mistake would open a device
node to the world, and that re-deriving from an already-ACL'd mode is idempotent rather than
widening a little on each pass. And one test asks the **kernel**: `grant_user_rw` on
`/dev/null` from an unprivileged process returns **`EPERM`, not `EINVAL`** — the blob was well
formed and only the capability was missing. That is the whole unprivileged proof available,
and it is worth having, because the alternative was discovering the format was wrong only on
a blessed box.

**`scripts/bless` shows the command, then runs it.** It already ran it; what it did not do
was *display* it. `run_privileged` printed the command only on the fallback path where there
was no terminal to prompt on — so on a box where sudo could prompt, the one privileged step
the script takes happened without ever being shown. An operator should see the exact
capability grant they are approving before the password prompt, not instead of it. The string
is now read from the helper itself (`install --print-setcap`), so the command shown, the
command run, and the set `--verify` checks against all derive from `REQUIRED_CAPS` and cannot
drift — the hand-kept-list shape §3.78 and plan §18 item 40 keep finding.

**Not renamed — and then renamed.** The owner allowed that the binary "might need to be
renamed"; I kept the old name and argued that the new verb was not a second purpose, since a
re-enumeration is what makes a node inaccessible and handing the access back is that operation
finishing. The owner overruled it, with the better reason: the helper is now *invoked for all
tests*, so a name ending in the one operation it happens to perform first reads as a much
narrower tool than it is. Renamed to `serial-nexus-devprep` in §3.81. The decision and its
reversal are both kept here, because the argument for the old name was wrong in a way worth
seeing — it weighed the verb's mechanism and not the tool's callers.

**Two residuals.** The grant does not survive the *next* re-enumeration either — nothing
placed on an inode does — so it is re-applied per cycle by construction; the durable answers
are still group membership or a udev rule, and this exists for the case where neither is
available without a re-login. And a copy blessed before `cap_fowner` joined the set replugs
exactly as before and *skips* the grant with a note rather than failing, because a capability
that arrived later must not turn every previously-working `cycle` red.

**Then the owner asked for the other half**, in these words: *"I'd you to use the new
capability to get permissions for USB nodes at the beginning of the test as well. I'd rather
not have to remember group changes + reboot when I move the project to a different system."*
`ensure_rig_access()` runs once per test binary, resolves the four variables that already
name the rig — `SNX_CROSSOVER_A`/`_B`, `SNX_REPLUG_DEV`/`_DEV_B` — to their USB ports,
deduplicates (two by-id links and two tty paths routinely name the same two adapters), and
calls `grant`. It is wired into the two funnels every hardware test passes through:
`crossover_ports()`, and `rig()` in `p7_replug_hardware.rs` — the replug tests need it
*before* their first cycle, since the node they open at the start is one no replug has
touched and nothing has re-granted. It grants only for ports the environment named, because
naming a port is the operator's statement that it is wired to a rig and not to live
equipment (§15.17), and failure reports rather than panics: without a blessing there is
nothing to do and each test then fails or skips with its own, more specific message.

**Measured, both arms.** Unblessed, the harness prints
`rig access: not granting device access — … is installed but not blessed` and the tests
proceed to their own diagnoses. Blessed, it prints
`rig access: granted on ports 3-2.4, 3-2.3 — {"granted":["/dev/ttyUSB0","/dev/ttyUSB1"],"uid":1000}`
— twice in a full run, once per test binary that asked for hardware, which is the
once-per-process guard behaving as designed rather than a repeat.

**And it broke the Apple cross-check on the way in, which is worth recording rather than
quietly fixing.** `O_PATH` does not exist on Darwin and Darwin's `setxattr` carries an extra
`position` argument, so `serial_nexus_sys` — which compiles for macOS even though the helper
that calls this does not — stopped building on both Apple triples. CI's `check` job caught
it, exactly the gate notes §3.71 added for exactly this class, and it caught it because I
pushed without running the Apple cross-check locally first: the full gate set was run before
the *previous* commit and not re-run after this change. The repair is a module-level
`#[cfg(target_os = "linux")]` over the whole ACL block rather than one on the `pub fn` alone,
because the constants and the encoder are only reachable through it — and a `cfg` covering
less than the code it protects is precisely §3.71's shape. The first attempt at that repair
demonstrated the same hazard in miniature: one anchor did not match, the encoder kept its
`cfg`-less definition, and both triples failed again on `ACL_XATTR_VERSION not found` — so
the second pass verified the attribute on *every* item individually instead of trusting the
edit. Both triples clean before the second push.

**The verb, measured on the bench.** From an unprivileged shell with the blessed copy:
`grant --port 3-2.3 --port 3-2.4 --json` → `{"granted":["/dev/ttyUSB1","/dev/ttyUSB0"],"uid":1000}`,
after which `getfacl /dev/ttyUSB0` reads `user::rw- / user:pwnall:rw- / group::rw- / mask::rw-
/ other::---` and `ls` reads `crw-rw----+ 1 root dialout`. Ownership unchanged, `other`
unchanged — the property the unit tests pin, and the one where a mistake would have opened a
device node to everyone.

---

### 3.80 The rig lane, green — and the bench is 3-wire where the record says 5

The first rig lane in this record that goes green, and the first that could: it needed
`SNX_REPLUG_DEV_B` in the documented spelling (§3.75 B3, fixed the same day) and device access
that survives a re-enumeration (§3.79). Before both, it could not have passed on any box.

**The figure, with its scope.** **894 passing · 1 failed · 6 ignored**, four self-skips, on the
crossover box 2026-08-12. Scope is the rig lane **minus two required modes**, and both
exclusions are measurements rather than conveniences:

* `SNX_WEB_UI=required` is not set because this box has no `node` — the browser gate would
  hard-fail on an absent interpreter, which says nothing about the product;
* `SNX_RIG_FLOW=required` is not set because **this bench measures 3-wire**, and §15.52 makes
  that a legitimate answer rather than a fault ("a rig that answers no is legitimate").

The one failure is `p3_idle_cost` — plan §18 item 46, unrelated to the rig and reproduced
here at the same load sensitivity that item records. Every hardware test passed, including
`identity_survives_a_replug_that_renumbers_the_tty`, which had never executed: it is the test
`SNX_REPLUG_DEV_B` gates, and its first run performed the swap and observed the renumbering
(ABSCDGL6 moved ttyUSB0 → ttyUSB1).

**The wiring changed under the record, and two independent instruments say so.** §15.52 and
notes §3.53(i) record this same adapter pair as:

```
5-wire crossover: RTS/CTS both ways, DTR moves nothing
[rts_a_to_cts_b=true rts_b_to_cts_a=true …]
```

Today, same two adapters, same probe, same `probe_set` era `e79f5fcd86a2e5f0`:

```
3-wire: no handshake lines carried
[rts_a_to_cts_b=false rts_b_to_cts_a=false …]
```

**Why this is not one instrument's opinion.** P5 drives the lines port-to-port with no daemon
in the path; the harness drives RTS through the *daemon* and reads CTS back through `state`'s
`modem_lines`, and the code says in as many words that neither subsumes the other — "the
doctor's arm would still pass if the daemon's state reporting were broken, and this one would
still pass if the doctor's were". Both now ran with working device access, and both report no
continuity. The harness's reading is the more pointed of the two because it shows the local
line being asserted while the far one does not move:

```
port1 RTS high -> port0 {"cts":false,…,"rts":true}
port1 RTS low  -> port0 {"cts":false,…,"rts":true}
```

`rts:true` in both polarities is the positive control the all-false reading needed: the
driving side worked.

**What is established, and what is not.** Established: on this bench, asserting RTS on either
port does not move the far port's CTS, measured twice by instruments that share no code path,
with the DTR arm reading false exactly as it always has. Not established: that the *cable*
changed rather than CTS sensing failing on both adapters simultaneously. An all-false
handshake cannot separate those two by itself, and no rung of this rig can — the discriminator
would be a line known to cross, which is precisely what is missing. The cabling reading is the
strongly favoured one, because the same adapters still certify (`custom_baud`, `break`,
`icounter`, `rate_ladder` all true), still reach the identical P14 ceiling
(`max_reliable_baud = 3000000`, `ceiling_kind = adapter-refused`, byte-for-byte with the
committed captures at the same era), and still honour `CRTSCTS` at the driver
(P15 `honoured_on_readback: true`, cflag delta exactly `CRTSCTS`, matching
`linux-7.0-2026-08-07-2b44c17-tier3.json`). A common-mode sense failure that spared every one
of those would be a strange thing. It is stated as favoured rather than proven because §7's
rule is that a kernel — or here a bench — that differs is reported with the observation named,
not argued into a conclusion.

**One consequence to carry.** Notes §3.63's end-to-end RTS/CTS measurement — *a `none`
transmitter delivers through a CTS stop in 25 ms; an `rts-cts` one never does* — was taken on
the 5-wire bench. It is not refuted; its **precondition has gone away**, and it cannot be
reproduced here until the pair is cross-wired again. §15.52's "reported, never judged" is what
kept this from reading as a regression: the probe measured the bench it was given and moved no
verdict, and the two end-to-end tests skipped with the measurement printed instead of failing.
Plan §18 item 28's DTR work is unaffected and still wants a rig with DTR wired; item 17's
break/parity checklist now wants a bench re-inspection too, since it was filed against a rig
proven 5-wire.

---

### 3.81 `serial-nexus-devprep`: the helper outgrew a name that described its first verb

**Requested by the repository owner**, who overruled the decision recorded at §3.79: *"Let's
rename the helper. It's a bit confusing that it ends in `replug` since it's invoked for all
tests."* Both halves of the choice were theirs — the name, from four candidates, and the
disposition of the old one.

**The argument I got wrong, kept because it is instructive.** §3.79 declined the rename on the
grounds that granting device access is not a second purpose: a re-enumeration is what makes a
node inaccessible, so handing access back is that operation finishing. That reasoning weighs
the *verb's mechanism*. What decides a name is the *tool's callers* — and after §3.79 the
helper is invoked by `ensure_rig_access()` at the start of every run that names a rig,
including runs that never replug anything. A name ending in the operation it happens to
perform first advertises a far narrower tool than the one that exists, and the owner's
sentence names exactly that.

**What moved.** `replug/` → `devprep/`; `serial-nexus-replug` → `serial-nexus-devprep`, crate,
binary and installed copy at `.snx-bin/<profile>/`; `blessed_replug_helper()` →
`blessed_devprep_helper()`. Seventeen files carried the binary name and seven more carried the
directory path.

**What deliberately did *not* move.** The word *replug* everywhere it means the operation
rather than the tool: `SNX_REPLUG`, `SNX_REPLUG_DEV`/`_DEV_B`, `skip_no_replug`,
`itest/tests/p7_replug{,_hardware}.rs`, "the replug lane", "a run that died mid-replug". The
helper still performs replugs; it is simply no longer only that. Renaming those would have
been the mirror of the mistake being fixed — losing a precise word because a tool changed
scope.

**The old name is banned tree-wide**, at the owner's choice between that and annotate-in-place.
`serial-nexus-replug` and `serial_nexus_replug` join §15.40's `RETIRED` list, so the gate that
keeps the v14 vocabulary from drifting back now keeps this one out too. There is precedent and it
is the same gate: §15.40's v14 rename rewrote the tree and left the retired vocabulary only in
`docs/historical/` and the captured `docs/doctor/` artifacts.

**And here is the cost of that choice, stated rather than absorbed.** This record is
append-only, and a tree-wide ban means pre-rename entries were **mechanically rewritten** to
the new name. Three consequences a reader needs:

1. **Commands quoted in older entries did not literally say this.** The macOS scope lines of
   §3.65 read `--exclude serial-nexus-devprep` today; the command as *run* on 2026-08-05 named
   the binary by its then-current name. The *meaning* is intact — the same crate was excluded,
   and the figure it qualifies (756 / 4 / 4) is untouched — but the string is not a
   transcript. Where a literal transcript matters, `docs/historical/` keeps the original
   spelling, and so do the commit messages.
2. **One entry stated a mapping that the rewrite falsified, and was repaired by hand.** The
   v14 rename entry recorded `replug/` → its crate name *as of that rename*; a blanket
   substitution made it claim the v14 track produced a name that did not exist until today. It
   now names the directory and points here instead of asserting a date-wrong mapping.
3. **§3.79's "Not renamed" paragraph is a reversed decision, not a deleted one.** It keeps the
   argument I made and records that it was overruled, with the reason. A decline that is
   quietly edited away is the thing AGENTS §5 forbids; a decline that is overturned in the
   open is the thing it asks for.

Found by the blanket substitution itself, which is the honest version of how this was noticed:
the first pass rewrote all three sites silently, and reading the diff — rather than trusting
it — is what turned them up. A rename script that touches an append-only record is a
prose-truth hazard, not a mechanical one.

**One operator consequence.** The blessed copy lives at a path that has changed, so the
capability does not follow it: `.snx-bin/<profile>/serial-nexus-replug` still holds
`cap_dac_override,cap_fowner` and is now an orphan no code will look for. `scripts/bless`
installs and re-blesses the new name, and the stale copy should be deleted — a blessed binary
nothing references is exactly the kind of thing §15.45's narrowness argument does not want
lying around.

### 3.82 The §15.55 amendment reached the design and stopped: four sites still promised one capability

**Found by the v17 alignment pass** (2026-08-12), which ran the v16→v17 diff against the *tree*
rather than against the documents. The v17 landing had folded §15.55's two-capability amendment
into the four sites its own change list named — design §5's tripwire row, §15.45's status line,
§12's privileged-mechanism sentence, plan §2's devprep row — and the list was not the set. Four
more sites still stated the retired bound, and one of them is the security document.

**The four, most serious first.**

1. **`docs/security.md`** enumerated the privileged helper's bounds and made "**One capability,
   and no other.** `setcap cap_dac_override+ep`, nothing else" the *first* of them. This is the
   page an auditor reads to learn what contains `CAP_DAC_OVERRIDE`, and it promised a bound the
   tree does not have — while printing a `setcap` command the helper's own `--verify` rejects
   (`field_grants_required_caps` requires every name in `REQUIRED_CAPS`, and there is a test
   pinning that half a set is not the set). AGENTS §2's rule is that a stale claim is a defect;
   a stale claim in the security narrative is the worst instance of it, because the reader
   cannot check it against code they were sent to trust.
2. **`sys/src/caps.rs`** contradicted itself one screen apart: `CAP_DAC_OVERRIDE`'s doc said
   "the one capability the replug helper is blessed with", and `CAP_FOWNER`'s, fourteen lines
   later, said "the second capability the replug helper carries". Both were written in good
   faith; the first simply predates the amendment.
3. **`devprep/src/linux/mod.rs`**'s module doc — the file `docs/security.md` names as the
   authority for the bounds — opened its narrowness argument with "blessed with **one** file
   capability", bolded. Its five bounds were all correct; only the count was wrong.
4. **`.gitignore`** carried `cap_dac_override+ep` in a comment.

**Repaired, and repaired so the class cannot recur.** Every site now derives or points rather
than spelling: the security doc names `REQUIRED_CAPS` as the one place the set is written and
states that adding a capability is an amendment, `caps.rs` names the bits and defers the set,
the module doc links `install::REQUIRED_CAPS`, and the `.gitignore` comment says "the one
`sudo setcap` the helper prints" instead of quoting it — with a note that it held a stale copy
once already.

**And one hole the audit found beside them, closed with a fail-first guard.** The
no-`exec`-while-blessed refusal in `main` read `capability_state(CAP_DAC_OVERRIDE)` alone — one
bit, from before §15.55 — while `field_grants_required_caps` required both names. So a copy
blessed with only `cap_fowner` carried privilege and was **not** refused when
`PR_SET_NO_NEW_PRIVS` could not be established. Unreachable through the sanctioned path
(`install --verify` calls such a copy `Unblessed`, and `scripts/bless` always applies the
derived full set), which is why this is recorded as a narrowness hole rather than a live
defect — but "unreachable today" is the argument that stops being true when the set next grows,
and §15.45's narrowness is a standing tripwire.

The fix makes the drift unrepresentable rather than merely fixing it: `REQUIRED_CAPS` is now
`&[(&str, u32)]` — the `setcap` name and the `linux/capability.h` bit in one entry — with
`required_cap_names()` as the projection every command string uses and `caps_held()` folding the
whole set for the bounds question. Two lists that must agree became one list with two
projections. The union is deliberate and is stated at the fold: the question is "does this
process hold privilege the bounds exist to contain", and one capability is enough to make it
yes; an intersection would let both half-blessed shapes through the refusal. Guard:
`the_bounds_question_folds_every_required_capability_not_just_the_first`, whose middle assertion
(`cap_fowner` held, `cap_dac_override` not) is exactly what the old single-bit read got wrong.

**Two prose defects in the normative pair, repaired where they stood.** Plan §3 rule 10's new
walker clause said skip lists are keyed on the is-a-checkout predicate "never by name lists" —
absolutely, while `meta_names.rs` legitimately keeps a static `SKIP_DIRS` for build outputs. The
substance was right and the wording was a licence to delete `SKIP_DIRS` and re-red every gate on
`target/`; it is narrowed to *nested checkouts*. And design §7.1's disconnect-latency sentence
said a real unplug lands "an order of magnitude inside the read-poll budget" when the committed
measurement — 1.2–2.0 ms, 3 of 3, against a 200 ms re-arm — is **two** orders; corrected, with
the warning not to fuse that budget with the 200 µs–5 ms *pty* cadence AGENTS §6 quotes, which
is a different node.

**One ledger defect.** Plan §18 item 52's evidence line sent an implementer to "the `status`
verb's `ready` line". `status` prints no capability line at all; the stale spelling is in
`install`/`install --verify`'s `Ready` arm. An implementer would have grepped `fn status` and
found nothing. Corrected in place — the item is still open, and only its evidence moved.

**What the audit did *not* find is worth recording too.** Every other clause v17 changed in
design §1–§14/§17 and plan §1–§3 checks out against the tree, including the four the landing
flagged as most at risk: P5's fourth discovery arm byte-for-byte, the corrected doctor-gate
spellings against both CI lanes, §15.51's `platform-refused` exemplar against the `42eac2a`
triple, and §15.44's withdrawn register (those figures appear nowhere but the register itself).
The four filename-keyed code sites all point at v17. So the drift was not general: it was one
amendment, one incomplete propagation list, and the sites nobody thought to grep.

### 3.83 The pattern wait, built: one matcher where four clients had three

**Plan §18 item 47 executed** (2026-08-12) — the one design-ahead-of-tree surface, closed. §10's
*The pattern wait* and §15.56 were specified at the v17 revision and the tree answered
`method not found`; it now answers.

**What it is.** `tap.wait <endpoint>` parks until one of up to eight named byte patterns appears
on the endpoint's hostward stream, its deadline expires, or the graph drops the endpoint. The
motivation §15.56 records was measured, not imagined: four independent tap consumers in this
tree, three hand-rolled reassembly implementations among them, each re-deriving frame
reassembly, replay-splice handling, gap discipline and timeout-versus-teardown discrimination.
This is §16.1's move — encode the subtle rules once — applied to the observation surface.

**Where each contract clause lives, because the mapping is the whole design.**

* **The matcher is a new module, `daemon/src/pattern.rs`**, holding everything decidable without
  the graph: the maxima, the compile, and the rolling window. `regex-automata`'s meta engine,
  whose every search strategy is linear in the haystack — its one backtracking strategy is the
  *bounded* backtracker, which memoizes precisely so it stays linear, never the exponential
  kind. Literals are escaped byte-wise (`\xNN` for every byte, no exceptions) into the **same**
  automaton rather than matched by a second engine, so there is one engine, one compile budget,
  and one leftmost rule. `nfa_size_limit` is §10 clause 3's bounded compile size, and it earns
  its keep: `(?:a{1000}){1000}` is refused at compile with `-32602` rather than paid for on the
  thread that runs every console.
* **The armed wait lives inside `TapHub`**, not beside it. §10 clause 4 requires the matcher to
  consume the hub stream with no lossy queue between hub and matcher, and a matcher fed through
  a bounded channel like a tap's would have made "no match" mean "no match, unless the channel
  was full" — which is not an answer. `TapHub::ingest` feeds every armed wait in the same
  synchronous critical section that fans the chunk out to the taps.
* **Arming is one critical section** (`TapHub::arm_wait`): ring snapshot scanned, live matcher
  armed, nothing on the thread between them — the `tap.open` register pattern, so §10 clause 5's
  splice-exactness is a single-thread fact rather than a lock. The ring is read **as the ring**,
  so the configured depth is scanned even when it is deeper than `lookback`; `lookback` bounds
  only how far a *later* match may reach back into what is retained.
* **A gap resets the window** (`ScanWindow::feed`). Bytes either side of a feed-hop hole were
  never adjacent, so leaving them adjacent would let the *loss* manufacture a match. Clearing
  costs some true negatives and cannot invent a positive, and the `gaps`/`gap_bytes` counters
  ride every result so a `matched: null` says how strong it is.
* **Two outcomes, two kinds** (§10 clause 6). Expiry is a **result** — `timed_out: true` beside
  the counters — because a deadline is an answer and a verdict never leaves one unnamed. A
  teardown is a **typed error**, `AppError::EndpointGone` (`-32008`), because the stream stopped
  existing and a timeout would claim it was watched throughout.
* **It is a waiting verb for free.** `tap.wait` lands in the ordinary `dispatch` lane rather than
  in `control.rs` beside `tap.open`/`tap.close`, because unlike those it needs nothing
  connection-scoped. That one placement decision buys §15.20's whole contract: the connection's
  one in-flight slot, the `-32006` refusal of a pipelined request, and cancel-on-EOF (dropping
  the dispatch future drops `WaitGuard`, which disarms).
* **Expiry never consumes, trivially, because the wait is a spy.** The only thing an armed wait
  changes is the hub's `active` flag — which is what makes it *observe* (§10 clause 7: an armed
  wait on a ring-off, untapped endpoint still observes) — and `WaitGuard` restores it on every
  exit path, including the one that runs no code in the verb at all.

**One filed clause deviated, and the reason is a collision the filing could not have seen.**
Item 47 asked for "0 matched, 2 timed out, 1 error — the doctor's exit-code precedent". Two
things are wrong with that, both visible only against the tree. `2` is already **clap's**
usage-error code on `serial-nexus-ctl`, so a script could not have distinguished "no match" from
"you misspelled a flag" — and clap's 2 arrives before any daemon contact. And the doctor
precedent the clause cites puts the *verdict* at 1 (`unsupported`) and the *operational failure*
at 2, so the filed numbering inverted the very scheme it named. What landed is `grep(1)`'s — **0
matched, 1 no match, 2 could not run** — which is the convention for the tool that asks `grep`'s
question, agrees with the doctor, and makes clap's 2 a harmless synonym rather than a
collision. Plan §18 item 47 carries the deviation with this reasoning.

**Verification, and the four reverts that were executed rather than asserted.** Twelve guards
in `itest/tests/p12_pattern_wait.rs`. Six need no serial device — every host-facing endpoint has
a tap hub at load even while its serial node sits `waiting` on an absent device — so the typed
expiry, the typed teardown, the maxima, `state` visibility, the `-32006` interplay and the CLI's
non-match exit all run on **every** platform, which matters for a macOS lane that has no
software null modem. The matching guards use a `sim pty --echo` driven by `send`, so the bytes
and their timing are the test's, not a seeded double's.

Rule 17 wants fail-first proof against the unfixed tree, and "the unfixed tree" for a new verb
is trivially `-32601` everywhere, which proves nothing about any individual guard. So three
reverts were performed in place (AGENTS §8: revert the specific fix, never stash) and the
results recorded in the file's own module doc:

1. Dropping `!self.waits.is_empty()` from `TapHub::refresh_active` → the ring-off/untapped guard
   reddens with `bytes_scanned: 0` on a console that was talking the whole time. Exactly the
   failure §10 clause 7 exists to prevent.
2. Feeding the matcher per-chunk instead of through `ScanWindow`'s rolling buffer → the
   cross-frame guard reddens. The pattern `A\nB` spans the newline `send` appends, and the guard
   proves the two frames were separate by watching a tap before emitting the second half —
   without that proof it could have passed on one coalesced frame and asserted nothing.
3. Deleting the armed-wait sweep from `TapHub::detach_all` → **the interesting one, and it
   corrected a claim I had already written.** I expected the wait to ride its deadline out and
   answer `timed_out`. It does not: dropping the hub drops the `oneshot` sender, so
   `tap_wait`'s `Err(_)` arm still answers `-32008`. What is lost is the *reason string* and the
   accounting. So the sweep is what makes the teardown outcome informative, not what makes it
   typed — and the module doc says so now, because a fail-first table that states an outcome
   nobody ran is the same class of claim as a gate that asserts nothing.

**One defect found by reasoning rather than by a test, and worth stating because no guard in
the battery would have caught it.** `tokio::select!` picks at *random* among arms ready in the
same poll unless told otherwise. The wait's two arms are "the hub delivered an outcome" and
"the deadline fired", so a match the hub had already delivered could be answered
`timed_out: true` — a wrong verdict rather than a late one, and the caller cannot tell. Worse,
the timeout arm then found `disarm()` returning `None` (the wait had already left the hub) and
fell back to zeroed accounting, so the reply claimed both that nothing matched *and* that
nothing was scanned. Fixed two ways: `biased;` with the settled arm first, which closes the
window entirely because both halves run on the one runtime thread and the hub cannot fire
between that poll and the timeout arm's body; and, on the `None` path, taking the outcome out of
the channel instead of inventing a timeout. The zero-filled fallback is gone. This is the class
a test finds only by losing a race, which is to say not reliably — the reason it is recorded
here rather than left to the guard that does not exist.

**A process finding worth more than the guards.** The first fail-first attempt ran
`cargo test -p serial-nexus-itest` after reverting `daemon/src/tap.rs` — and the test **passed**,
in 0.03 s, with nothing recompiled. The itest crate spawns the daemon *binary* by path; building
the test crate does not rebuild it. A revert had been made, a test had been run, and the run had
measured the unmodified binary. That is plan §3 rule 22's own class — a check whose passing
output is identical to its not-running output — arriving in the fail-first procedure itself,
which is the last place it would be noticed. Every revert here was re-run with an explicit
`cargo build --workspace` first, and the three results above are from those runs.

**Dependency reality, confirmed rather than assumed.** §15.56 predicted the matcher would add
zero lockfile packages, `regex-automata` and its family already being present transitively
(tracing-subscriber → matchers). Measured: the `Cargo.lock` diff for this change is **one line**
— the daemon's own edge. Both Apple compile triples, the minimal-daemon clippy, and
`cargo deny check licenses bans sources` are green with it.

**The adversarial review, and the four defects it found that I did not.** Six independent
lenses over the finished change — matcher arithmetic, cancel-safety, contract conformance, test
quality, docs-versus-code, and the alignment repairs — each told to find defects and to report a
clean lens as clean. (The verify half of the harness died on a session quota; the findings below
were therefore verified by hand, and the two that could be settled by execution were settled that
way rather than by a second opinion.) It earned its keep four times:

1. **The replay scan ran across an unobserved hole, and matched things the console never said.**
   The ring stores bytes and nothing else, so a feed-hop hole leaves no trace in it: `lo` and
   `gin: ` sit adjacent in the snapshot having never been adjacent on the wire. `feed` had the
   gap-reset discipline; `seed` had none, and scanned the raw snapshot. **Proven by execution**,
   not argued: with the fix reverted, the daemon reports `matched: {pattern:"p", offset:0}` for
   `login:` against a ring that only ever held `lo` and `gin: ` either side of a 4096-byte hole.
   On a *gating* verb that is the worst failure available — a caller sends a password to a device
   that never printed the prompt. The hub now remembers where the newest hole is (`ring_gap`),
   trims the snapshot to start after it, reports the trimmed extent as `replay_scanned`, and
   counts the hole in `gaps`/`gap_bytes` so a `matched: null` still states its own strength. I had
   reasoned about the live path's gap discipline and simply never asked the same question of the
   seed path.
2. **A wait was charged for every byte lost before it existed.** `ingest` hands the same
   `gap_before` to every registered wait, and on a ring-off, untapped endpoint — §10 clause 7's
   *headline* shape — the feed is inactive, so the endpoint's entire history sits on
   `feed_dropped` and arming is what turns the mirror on. The first chunk therefore handed a
   fresh wait a gap of megabytes, and the verdict claimed a scan holed by the endpoint's whole
   life when everything from `from_offset` on was scanned contiguously: §10 clause 4 inverted.
   `tap.open` solves the identical problem by handing the client a baseline to subtract (TAP-4);
   a wait has no such field and no client to do the subtracting, so it is credited at arming.
3. **`lookback: 0` silently voided the cross-frame guarantee.** §16.12's rule is about maxima and
   I applied it only upward. A window shorter than the pattern cannot hold the pattern, so the
   verb would answer `timed_out: true` — with `gaps: 0` asserting a whole scan — for a string
   that was on the wire. There is now a floor: the longest *literal*'s length, exact for literals
   and stated as unavailable for regexes (`a{1000}` is a six-byte source, so the daemon cannot
   size it and the docs say the caller must).
4. **The empty pattern was refused with a reason that was false** — "is 0 bytes, over the
   1024-byte maximum" — because it shared the over-length error. A refusal whose stated reason is
   the opposite of the truth is worse than one with no reason, and it is the same class as the
   messages plan §3 rule 11 governs.

The review also found four documentation defects that mattered more than they look, all of the
same shape — **the change updated the record and left the specification behind**:

- **The design still said the pattern wait was not in the tree**, in four places (the front
  matter's "exactly one design-ahead-of-tree surface", §10's "Specified this generation, not yet
  in the tree … the verb answers the standard method-not-found", the verb surface's parenthetical,
  and §15.56's own Status header), while AGENTS §2 and plan §18 item 47 — edited in the same
  change — said it had landed. Two normative documents contradicting each other in one commit,
  and AGENTS §5's amend-first order makes the *design* the document that has to stop being ahead.
  Repaired at all four.
- **Three sites enumerated the waiting verbs as exactly two.** `arbitration.md`'s "The two
  waiting verbs", README's "One in-flight *waiting* verb per connection" list, and — the one with
  teeth — README's "Do not use `-N` with a verb that waits", whose list of verbs that happens to
  did not include the one verb that *always* parks.
- **Every worked `state` example in the docs was stale**, including two in the file that added
  the `waits` array.
- **Two of my own alignment repairs over-claimed.** `docs/security.md` gained "nothing in the
  tree spells it by hand" — false, and plan §18 item 52, edited in the same change, says so. And
  the §7.1 latency repair imported the *wrong measurement*: it cited notes §3.54 while quoting
  1.2–2.0 ms, which is a different observable (the `poll` revents readback) from a different
  entry; §3.54's own figure for the node leaving `active` is 2–3 ms. Fixing a citation defect by
  introducing one is worth recording plainly.

Plus three more, each fixed: the no-`exec` refusal message hardcoded `cap_dac_override` in the
one branch the union check newly made reachable (a `cap_fowner`-only copy was told it held a
capability `getcap` would not show); `capabilities --json` answered `"blessed": true` for a
half-blessed copy while `install --verify` on the same file said `Unblessed`; and the harness's
`blessed_devprep_helper` proved half the blessing — it read `cap_dac_override_effective` and
never `can_grant_device_access`, which is the field that exists for exactly that question and
which nothing in the tree read. A rig box with a pre-§15.55 blessing would have passed that
check, cycled, silently skipped the grant, and failed every following test with
`Permission denied` — notes §3.79's signature, reintroduced through the back door.

**One assertion of mine was vacuous and is gone rather than weakened.** The expiry guard asserted
`feed_dropped == 0` "a spy charges no loss" on a fixture whose device is *absent*, so no hostward
byte can exist and the assertion passes identically whether the property holds or not — plan §3
rule 22's own tell, inside the battery written to honour it. It is deleted, with the reason and
the fixture it would need stated where it stood.

**CI, and two results the push produced that this session had not measured.** Run 31657666919
on `main` is green in all six jobs.

1. **macOS reads 896 · 0 · 6 at whole-workspace scope** (job 94315579211, macOS 26.5.2 / arm64;
   the lane runs `cargo test --workspace --locked --no-fail-fast`, no exclusions), superseding
   the 860 row. The delta closes exactly and is worth stating because it is a check on the
   Linux decomposition rather than a restatement of it: this session added 37 tests, and 36 of
   them run on macOS — the one that does not is the devprep capability-fold guard, which lives
   inside the Linux-only platform module. 860 + 36 = 896. That the six device-gated pattern-wait
   guards run and self-skip there is why the battery was split so half of it needs no serial
   device.
2. **The macOS doctor jq gate executed green — the first time in this record** (plan §18 item
   48). The diagnostic CI prints on every run, green ones included, shows `arm64`,
   `ProductVersion: 26.5.2`, and `Mach-O 64-bit executable arm64` for
   `target/debug/serial-nexus-doctor`; the doctor produced its JSON twin and
   `jq -e -f expectations/macos.jq` printed `true` and exited 0. Item 48 is therefore **half**
   discharged, and the half is the measurement. **No cause is claimed for the ENOEXEC** that
   notes §3.77 recorded: it was seen once and has not been seen since, which is not the same as
   explained, and a one-off that stops happening is exactly the evidence AGENTS §8 says not to
   reason from. The item's second half is untouched — the gate still sits after the test step,
   so a red test would hide it again, which is rule 22's own class.

**Still declined, and visible as such:** deliver-path matching, backtracking engines, patterns
over decoded text, unbounded lookback, and broadcast delivery. Web-bridge admission is the one
that needed more than an absence: a verb nobody added and a verb deliberately kept out look
identical in an allowlist, so `tap.wait` is now named in `bridge.rs`'s "what is deliberately
absent, and why" list *with the consequence that decides it* — the browser page holds one daemon
connection, and a parked wait would answer every other page verb with the in-flight refusal for
its whole duration — and it rides the refusal test's loop, so the exclusion is asserted rather
than assumed. The allowlist was already fail-safe, so nothing was reachable; what was missing
was the record of intent (§15.34's "the allowlist stays a deliberate act").

**The rig.** Run on the two-FT232R crossover box after the operator re-blessed the helper:
**931 · 1 · 6**, equal to the default-scope figure, with every hardware test passing — both
replug tests including `identity_survives_a_replug_that_renumbers_the_tty`, all five crossover
tests, and `web_tls_round_trip` under `SNX_TLS=required`. The lane dropped `SNX_RIG_FLOW` and
`SNX_WEB_UI`, both by measurement rather than convenience: the two `rts-cts` tests printed the
reading that justifies their skip (`port1 RTS high -> port0 cts:false`, and `false` again with
RTS low — a 3-wire bench, §15.52's legitimate answer, now confirmed by a third independent
instrument), and this box has no `node`.

**A rig lane hung once, and the honest handling of that is part of this record.** Between two
green lanes, one stalled 39 minutes on
`p6_outage::outage_faults_then_purges_then_recovers_byte_clean`: the receiving leg sat `waiting`
with "peer disconnected; awaiting reconnect" and `reconnect_count: 2`, the daemon answering
`state` normally, on a box at load 0.19. Attribution was **measured, not argued** — the same test
passed at default scope on the same binaries minutes earlier, passed in the preceding rig lane,
and passes 5 of 5 in isolation at full rig scope in ~4 s each; the change touches no line of
`leg.rs` or the outage path. The likely mechanism is whole-suite parallelism around loopback port
reuse, which this very file already has a test for
(`a_refused_listen_bind_retries_until_the_address_frees`). So: not this change's, not closed, and
not swept — one unreproduced hang is precisely the evidence AGENTS §8 forbids reasoning from, in
either direction. The lane was re-run whole and read 931 · 1 · 6.

One procedural note, because it is the kind of thing that quietly corrupts a figure: an earlier
rig lane was **discarded rather than recorded**. It had started against the previous blessed
helper, and `scripts/bless` replaced that binary underneath it mid-run, so the lane would have
exercised two different helper binaries. The rig was checked clean first (`authorized=1` on both
ports — AGENTS §8's dead-run residue) and the lane re-run whole.

---

### 3.84 The v17 alignment pass against the tree: fifteen deviations, eight refuted, and the defect a refutation was standing on

**What was run.** The v17 revision's alignment obligation executed against the *tree* rather than
the documents (AGENTS §5; the v16 pair had its own such pass at §3.75). Eleven readers, one per
design section range, each extracting the normative clauses of its range and checking every one
against the code by grep-and-read; then one adversarial verifier per claimed deviation, told to
**refute** it and given only the finding and the tree, never the finder's reasoning (AGENTS §9).
Twenty-three claims survived the finders; the verifiers confirmed **fifteen** and refuted
**eight**. Both halves are recorded, because a refutation is a decision.

**Design moved, nine times** — each corrected in place with the old reading annotated, never
silently rewritten. §6 sent readers to §17 for the tap contract, which is §10's (§17 is the web
console). §5 attributed a checksum assertion to `p6_head_of_line.rs`, which reads a monotonic
`delivered_hostward` byte counter and contains no checksum. §7.1 clause 1 promised "every other
node in the config still loads" — the one thing a *pre-create* precheck cannot do, since it
returns `Err` out of `load` and §11's atomicity creates nothing; the false half had also reached
an operator through P15's shipped consequence string, repaired in the same change. §7.3 clause 2
named `last_write_error` for the fault policy, where the tree deliberately puts the errno in the
fault reason and says so at the branch. §7.7 quoted §3 as saying "existing-PTY connectors", words
§3 has not carried since the v16 rewrite. §8's must-survive-every-rewrite fixture register said
"four Python fixtures" and omitted `strict.py` — the `--error-paths` positive control that shipped
with item 34 the same week the register was last rewritten, which is exactly the gap a
must-preserve list exists to close — and its sibling sentence said "the five this list carried"
over an enumeration of six. §17 claimed "permissive HTTP and WebSocket crates": the tree takes one
protocol crate for RFC 6455 framing and hand-rolls every byte of HTTP, including the token/Host
gate, which makes the sentence hide *where the security-relevant surface lives*. §17 keyed browser
history by the daemon's socket path — a value the daemon never puts on the §10 wire, so no
conforming client could key on it; the tree substitutes the web origin and documented the
substitution.

**Tree moved, six times.** Two drifted comments (`ReplayRing`'s rustdoc still called a ring-off
endpoint "the default" while its own next paragraph argued from the default-on ring, and repeated
a `cap == 0` construction the wiring never performs; `pty.rs` kept the withdrawn "~13.8 KiB"
figure the design had already repaired at its own site — 14131 bytes, which no committed P10
capture reads). Two schema-authority gaps in `docs/rpc/`, which §10 names as the authority and
deliberately duplicates none of: `tap.wait`'s `lookback` **floor** was enforced by the daemon and
documented nowhere, so a request inside the documented range answered `-32602`; and
`configuration.md`'s worked `remove-node` reply omitted `discarded_at_teardown`, which its own
table calls always present. And the two below, which are the pass's real yield.

**The major one: a three-way contract with a two-valued predicate underneath it.** §7.1 clause 7
says "a driver that *refuses* the flag outright is honest and is not refused here; only
accept-then-drop is the defect", and §11 calls it "the three-way discrimination — honour, honest
refusal, accept-then-drop". `serial_nexus_sys::honours_rtscts` could not make that call: it
discarded the `tcsetattr` result (`let _ = tcsetattr(...)`) and answered on the read-back alone,
so an honest refusal returned `Ok(false)` — byte-identical to accept-then-drop — and
`precheck_flow_control` refused the config. P15 *does* record `tcsetattr_ok`, so on the same port
the doctor answered `Supported` with the sentence "Every named port honoured `CRTSCTS` on
read-back" while `load` refused the node: precisely the split §7.1 clause 2 calls "worse than
either verdict alone", with `shipped_predicate_agrees` — the field that exists to catch exactly
this — blind by construction, both sides reading the same read-back.

**Why it survived a generation, which is the part worth keeping.** The design stated *both*
shapes and only one of them was code: clause 7 described the behaviour, clause 2 enumerated a
two-valued predicate ("`Ok(false)` refuses"). A reader checking the tree against clause 2 found
agreement. The repair moved the tree to clause 7 — `RtsCtsOutcome::{Honoured, Refused,
AcceptedThenDropped}` plus `Err` for unmeasured, with the read-back asked *first* because a port
already carrying CRTSCTS reads back set whatever the set returned — and then moved clause 2 to
describe the tree it now has. No two-valued shim was kept: the bool shape is what let two callers
collapse different facts into one answer. Five plants, each the exact pre-fix defect, reddened and
were restored. Neither `probe_set` nor `field_set` moved, proven three ways, so no era row is
owed.

**The defect a refutation was standing on top of.** One claim — "the design and the daemon's own
refusal message name a serial config key `flow` that the schema rejects" — came back REFUTED, and
correctly so *as framed*: the design never states the key normatively, and its `flow = "none"` is
prose shorthand. But the verifier's own confirmed sub-facts were a shipped defect in plain sight,
and refuting the framing had made it invisible. Measured by execution rather than by reading:

    $ serial-nexus-ctl load flowtest.toml      # the file contains  flow = "none"
    Error: parsing flowtest.toml: TOML parse error at line 1, column 1
    unknown field `flow`, expected one of `name`, `device`, `baud`, `data_bits`, `parity`,
    `stop_bits`, `flow_control`, `faces`, `arbitration`, `hostward_buffer`,
    `purge_on_reconnect`, `replay_ring`, `modem`

The key is `flow_control` under `deny_unknown_fields`, with no key alias anywhere in the tree.
Four operator-facing sites offered `flow = "none"` as **the remedy** — the serial node's open
failure text (both arms), the `precheck_flow_control` refusal, and P15's consequence — and two
tests asserted the wrong spelling, pinning it. An operator doing exactly what the refusal said got
a second error naming a different problem. That is plan §3 rule 11's bar failing on the box that
prints the message, and it is the failure §15.53 exists to prevent: the refusal was built so
nobody would meet `serial2`'s bare "failed to apply some or all settings", and then handed them a
key the parser denies. Twenty-two spellings repaired across five files; the two tests now assert
`flow_control` and are the tripwire.

**The procedural lesson, which is the one to carry:** *a verifier that refutes the framing of a
claim must still dispose of the claim's confirmed sub-facts, or the refutation launders them.*
The finder had the facts right and the label wrong; the verifier had the label right and let the
facts go. Neither half was wrong on its own account. The fix is not more verifiers — it is that a
REFUTED verdict has to say what happens to the observations it leaves standing.

**Also found, outside the sweep.** `fuzz/Cargo.lock` has been stale since item 47 landed
(2026-08-12, `ae36876`): the daemon's `regex-automata` edge never reached it, and this session's
item 51 added a second (`nix` on `serial_nexus_rpc`). CI's `fuzz-nightly` lane carries a
"Verify fuzz/Cargo.lock is current" step running `cargo +nightly fetch --locked`, precisely
because cargo-fuzz takes no `--locked` of its own — so that lane was red-in-waiting for the next
scheduled run and nobody had seen it, the nightly not having fired since. Refreshed; the diff is
two lines and `cargo fetch --locked` now exits 0. The gate was right and the lock was wrong,
which is the outcome a gate is for.

**One process finding, recorded because it is the hazard of running agents concurrently.** A
fail-first plant survived into the working tree — `open_failure_text`'s two `RtsCtsOutcome` arms
left merged after a reviewer reproduced the implementer's plant C and did not restore it. It was
caught by `rustc`'s own `unreachable_patterns` warning on the next workspace build, and would have
been caught again by `-D warnings`. Two things follow: the plant-and-restore discipline needs the
restore *verified* (`git diff --stat` on the planted file, which the item-46 agent did and this
one did not), and a tree several agents are mutating wants its build run between waves rather than
only at the end. The item-46 agent's answer to the same hazard is the better one and is worth
copying: it did its plant-and-restore in a `git archive HEAD` scratch copy with its own target
dir, so a neutered `back_off` could never poison another agent's build and the readings were not
taken against a moving tree (AGENTS §8/§9).

---

### 3.85 Ten ledger items closed, two of them by declining, and the gates that had been asserting nothing

The same session as §3.84. Every item below carries its disposition in plan §18; this entry
records what the ledger cannot: the evidence, the surprises, and the two items whose right answer
was **no**.

**Item 46 — `p3_idle_cost`, answered by measurement and then rebuilt.** The item asked for one
controlled measurement choosing between "the backoff regressed" and "the cost is genuinely higher,
re-measure the artifact". The first is **refuted**: on this 20-core box, three healthy samples per
rung at 1/2/8/16/32 idle fds fit a slope of **0.0728 %/fd** with a **1.194 %** intercept, against
**0.2004 %/fd** for the same sweep with `back_off`'s body replaced by `let _ = wait;` — a 2.75×
separation, and the healthy figure reproduces §15.19's recorded ~0.06 %/fd within 20 %. What had
moved is the **fixed** term, which the artifact never recorded separately: its 2.00 % is a single
total at 32 fds with no baseline rung, so an absolute ceiling derived from it is dominated by a
quantity the mechanism under test does not control. That is why 3.50 % fired against a healthy
3.3–3.6 % with the backoff measurably intact.

The guard now measures **two rungs in one daemon process** — the artifact's `baseline_tty_fds`,
then grown to `idle_tty_fds` with `add-node` — so the fixed term cancels by construction rather
than merely similarly, and asserts the marginal cost against a new `per_fd_cpu_percent` with a
full provenance block (box, date, commit, both sweeps, both fits, the control). `DRIFT_FACTOR`
stays 1.75; only the figure it multiplies changed. `total_cpu_percent: 2.00` is kept and annotated
HISTORICAL rather than repurposed. **The decline was not laundered:** the neutered control still
reddens the guard, at 0.2375 %/fd against the 0.1274 %/fd ceiling, observed.

Two things are worth more than the repair. First, the item's third disposition — "measure under
the suite's own parallelism and record that as the operating condition" — turned out **unnecessary
rather than declined**: the marginal form is load-insensitive where the absolute form was not, a
run at loadavg **12.68** reading 0.0792 %/fd inside the band of runs at loadavg 1.4. Second, the
coverage this *loses* is named in the guard's own module doc rather than buried: a regression
inflating the daemon's fixed cost without scaling per fd is now caught only by the 20 % budget.
Recovering that honestly needs a recorded fixed-cost figure with an era of its own, across more
than one box — a measurement nobody has taken, explicitly not a number to invent.

**Item 45 — closed by declining, with both alternatives measured.** The item offered a structural
`validate` refusal for `existing-terminal` or a recorded decision that the schema's silence is
better; it is the second. **Which shape a §14 deferral gets is decided by where it sits in the
type system, not by taste.** Entry 14 is a deferred *role* of a shipped kind whose `faces` field
is the shared two-valued `Facing`, so the schema cannot exclude it and `validate` must — a
structural refusal for want of an alternative. Entry 15 is a deferred *whole kind*, and a word the
schema never admits is unreachable by construction where a `validate` refusal is one forgotten
call away from admitted. Option (A) was built on a scratch tree and costs two things forever:
serde's internally-tagged error enumerates every variant, so `type = "seriall"` would be answered
with a list advertising a kind the daemon refuses one stage later; and §7.7 states two fields and
then "otherwise it behaves as a boundary", so the rest of the field set would be a guess frozen by
`deny_unknown_fields` while `shape()` would owe `to_model` an endpoint topology the design never
states — letting an operator's *edges* validate against a shape nobody designed. The handshake the
item designed still works, inverted: the existing guard pair is now the **tripwire on the
decline**, planting the variant reddening both at their error-code assertions.

An earlier draft of this decline cited §15.4's merge diamond as the precedent for "unrepresentable
beats refused". **The review of the change caught it and it is a counterexample:** the one-producer
invariant is `TargetEndpointOversubscribed`, a `GraphModel::validate` refusal over a config that
deserializes perfectly. The true precedent was one paragraph up in the same file — §15.8's
configuration/state split, "state fields simply do not exist on configuration types". Corrected in
three places, two of them normative. The decline is unaffected; its stated precedent was not.

**Item 27 — closed by declining, and the obligation the decline carries.** The doctor's Markdown
value grammar is non-injective and stays that way (§15.57). Three measurements decide it: the JSON
is the artifact of record and the Markdown a view of it — 0 of 1064 Tier-3 scalar leaves carry
their JSON kind, against 22 `type ==` clauses in the gate, so making the *view* injective buys a
property nothing reads; the cost lands on immutable evidence, since §16.13 freezes every committed
report and an escape either mass-edits them or splits the corpus's grammar in two forever; and the
heuristic recovering 483 of 483 leaf paths is why nothing is broken, **not** why the escape is
declined, because "recoverable by a heuristic" is not a contract. What a decline of this shape owes
is that nothing quietly parses the rendering, and §15.57 states that as a rule with the
`--field-set` message as its enforcement point.

**Item 51 — the hoist found a live defect, which is why two copies is the shape §16.5 bans.** The
web console's socket-path copy had *two* arms where the policy has three: it never asked whether
the process was root, so it read `XDG_RUNTIME_DIR` first and handed everyone without a usable one
the **root** arm `/run/<name>.sock` — a path only a root daemon binds — while the daemon beside it
sat at `/tmp/<name>-<uid>.sock`. Every unprivileged macOS session, and any stripped service
environment exporting the variable empty. One implementation now, with the policy split out as a
pure function of two candidate paths and an existence predicate so both arms are reachable from any
environment with nothing on disk.

The review of that change found the new unit pin on the wrapper's argument order **vacuous on any
box that has a socket at the current name** — both sides of the equality read the real filesystem,
so with the current name present the correct order and the swapped one answer the same path.
Measured, not argued: with the arguments swapped and `XDG_RUNTIME_DIR` pointing at a directory
holding `serial-nexus-daemon.sock` the test passed; with neither socket present it failed. Rule
22's tell, inside a test written to prevent drift. Repaired by pinning the *candidate pair*, which
is computed before any `exists` call and so has no machine state to depend on.

**Item 52 — five clauses, and a gate that found its own ordering defect.** The macOS arm gained the
`grant` verb its own contract banned it from lacking; `grant_skipped` rides the JSON on both
platforms and `hold` emits its promised `up` line before reporting a grant failure; the cycle/hold
envelope is factored; `scripts/bless` strips capability-carrying files it did not install, derived
from `REQUIRED_CAPS`; and the `Ready` arm derives its capability line instead of printing the
pre-§15.55 single-capability form. Six plants, each reddened and restored — two of them proving a
**matcher** rather than a walker. The gate's own ordering was a finding: the non-vacuity floors
originally preceded the parity assertion, so the first fail-first run reddened on "only parsed
seven" instead of on the sentence naming the defect. Floors after, now. One behaviour change is
named rather than smuggled: `Envelope::up()` re-authorizes every port even after one refuses, where
`cycle` used to return at the first failure leaving the rest deauthorized while `hold` carried on —
a divergence nothing in the tree argues for.

**Items 48 and 54 — two gates that had been asserting nothing, and the difference between them.**
Item 48's macOS doctor gate sat *after* the test step, and a failed step skips the rest of a job, so
every red test from at least 2026-08-10 hid the one gate that lane gates on. It now sits
immediately after the build — its only real dependency — and **both** steps carry
`steps.build.outcome == 'success'`, so the reorder does not move the hiding in the other direction.
Item 54's hole was wider and quieter: clippy ran on the Linux lane and nowhere else, so
cfg-gated-only-caller dead code was caught by **no lane at all**. Planting exactly that shape — an
ungated private fn whose one caller is `#[cfg(target_os = "linux")]` — the three lanes answered
Linux clippy **exit 0**, `cargo check --target aarch64-apple-darwin` **exit 0**, and
`cargo clippy --target aarch64-apple-darwin` **exit 101**, naming the unused fn. The Apple
cross-check now runs clippy on both triples plus the minimal-daemon spelling, which is strictly the
wider instrument at nearly the same cost. Both are rule 22's class, and they differ in the way
worth remembering: item 48's gate existed and did not run, item 54's ran and did not look.

**Item 49 — the python3 class, and why the variable is not named for python.** Thirteen tests over
four files drive a codec child through an external interpreter, and no lane asserted they had
executed — a runner image dropping python3 would have reported the whole out-of-process extension
surface green without running a byte of it. `SNX_EXEC_CODEC=required` is the seventh instance of
the one mechanism, set in CI's Linux lane. The name is the capability, not the tool, and that
follows the tree rather than taste: `SNX_TLS` gates a tier whose missing tool is `curl`,
`SNX_WEB_UI` a suite whose missing tool is `node`. A fixture ported to another language would
leave an `SNX_PYTHON` naming nothing while the battery it gated still existed; rule 11 puts the
tool's name in the message, on the box printing it. Fail-first both ways: a PATH without python3
under the variable reddens with the fix-it message; the same PATH without it skips and passes.

**The fuzz crate was outside the one invariant that has a tripwire.** Design §16.3 states the
containment absolutely — "everything else is `#![forbid(unsafe_code)]`" — and nine crate roots
carried neither the attribute nor a sanctioned allow: `fuzz/fuzz_targets/*.rs`, each its own root
via the `[[bin]]` table, in a workspace-excluded crate that therefore inherits no `[lints]`. Worse,
**both** enforcing gates skipped the directory by name, so an `unsafe` block landing there was
invisible to the structural gate and to the grep detector alike. The hole was measured, not
inferred: with an `unsafe` block planted in a fuzz target, re-adding the skip made the gate green
again. All nine now carry the attribute (verified to compile on stable, and to bite — the compiler
refuses the planted `unsafe`), both walkers reach the crate, and the crate-root enumeration derives
the third root shape from the manifest's `[[bin]] path` entries rather than a glob, which is item
40's doctrine applied to a shape that gate did not know existed.

**Item 29 — a record completed from the history rather than from memory.** The v15 plan asserted
three shell scripts survived as external-tool wrappers pending §16.11. All three —
`scripts/lib/wait-for.sh`, `scripts/validate/phase0/license-gate.sh`,
`scripts/validate/phase8/external-codec.sh` — were deleted in **one commit**, `563fb9c`
(2026-07-24), which is the §16.11 execution itself and which created their successors in the same
diff: `itest/tests/p0_license_gate.rs` (new), a rewritten `itest/tests/p8_external_codec.rs`, and
the `wait_until`/`wait_for` helpers in `itest/src/lib.rs`. The claim was true when written and was
discharged by the track it was pending on.

---

### 3.86 Four construction items, and two filings the code corrected

Same session as §3.84/§3.85. Items 21, 36, 38 and 53 executed. Each is recorded in plan §18; what
follows is what the ledger cannot hold — twice, the *filing* was wrong and the tree said so.

**Item 21 — the inventory came before the change, and it corrected the note in both directions.**
notes §3.55 recorded `exec`'s `discarded_at_teardown` as a floor "because its internal merge stage
is not reached". True, and understated in one way and overstated in another. The full inventory of
what an exec node holds targetward at teardown: the per-channel host-facing receivers plus each
forwarder's in-flight chunk, **already counted**; the internal merged queue `src_rx`, moved by
value into `pump_child`, which `TaskSet::abort_all` drops unread — **uncounted**, and this is
§3.31's original defect surviving one stage further in than the fix reached; the chunk
`pump_child` is mid-`write_all` on — uncounted, no `held` slot existing for a bare receiver; and
bytes already inside the child's stdin pipe, which are **structurally unreachable and argued not
to be loss** — the exec codec's obligation ends at the child's stdin exactly as a serial node's
ends at the device fd, which is the line the serial adoption already draws.

**And the thing §3.55 got wrong, which would have shipped a wrong number:** the merge queue is
*bidirectional*. `mux_inbox` is a `target_inbox` entry whose forwarder tags chunks with the
reserved MUX identity, so roughly half of that queue is the raw device stream on its way *into*
the child. §3.55's sketch — "the merge queue needs only a `TeardownLoss` watch" — would have
charged hostward bytes to a targetward ledger, which is a conservation error dressed as a
completeness fix. Recorded because the note is cited from the counter's own documentation.

A new fixture, `tests/ext-codec/deaf.py`, supplies the shape the item named: a child that has
stopped reading stdin, so the merge stage holds bytes at teardown. `daemon/src/nodes/mod.rs`'s
`discarded_at_teardown` doc asserted "`exec`'s answer is a floor; every other kind's is exact" and
became a shipped falsehood the instant the fix landed — corrected in the same change.

**Item 36 — the shape is two streams, and that is a finding rather than a simplification.** A
golden transcript records the exec child-pipe conversation as two ordered byte streams, `<` for
what the child writes and `>` for what the daemon writes, never one interleaved log. The exec
boundary is two pipes polled concurrently (§15.22), so there **is** no defined interleaving;
pinning one would pin a scheduling artifact and the golden would flake for the reason the item's
own design note warns about. Records are byte-exact hex with a run-encoded tail, each carrying a
generated annotation so the file reads as a conversation — which is the gap the item names, since
golden *vectors* already covered single frames.

The `serial-nexus-sim transcript` mode plays both roles off one file, and two of its properties
are worth stating because they are not obvious: it is the one sim mode that **cannot print a JSON
verdict line**, because stdout *is* the envelope pipe, so verdicts go to files and stderr; and it
**parks rather than exiting**, because an exec child that exits is a crash to the daemon, which
respawns it — over the evidence. The one-byte mutation the item asks for was caught twice
independently: by the comparator at the line, and by the replayer at the byte offset
(`mismatch_offset: 65684`, expected `0x0b` observed `0x0a`), so neither half is trusting the
other.

**Item 38 — and the four suites it deliberately did not write.** The node guard landed with the
disjoint-reddening the template asks for: removing `teardown_loss.drain()` from
`CodecNode::signal_stop` reddens two guards (`discarded_at_teardown: 0` against 8008 owed) and
leaves the third — the read-only demux, which charges the *running* discard and not the teardown
ledger — green. The item's "folds review 37's codec guards into the kit surface" clause is
executed as **one** suite plus four citations rather than five suites, with the reasoning in the
new file's header: only 37-CODEC-3 had a codec-side property no suite could see; unknown-key
naming is already `attributes_are_structural`; reserved-identity lifecycle and the mid-chunk
charge are node properties with existing guards; quoted exec paths is a harness property; and
37-EXTC-2's partial-frame tolerance is already asserted byte for byte by `fragmentation_tolerance`
— a negative for it would fail that suite too, so a second one would be one rule spelled twice,
which is the duplication §16.1 exists to prevent.

**Item 53 — the suite that refuses its own vacuous parameterization.** `resync_is_counted` asserts
four things in order, and two of them are there because of how this class of counter fails rather
than because of the happy path: a `demux` call carrying **no bytes** must still report 0, which
fails a counter that ticks per *call* rather than per unit skipped, before the malformed unit can
supply an excuse; and a second read must agree with the first, because the daemon re-reads
`resync_count` on every `state` poll and a reset-on-read counter would report the loss once and
report 0 forever after. It also panics on an empty malformed unit or a zero expectation — a suite
parameterized into asserting nothing is rule 22's tell inside the kit written to honour it. The
kit-honesty negative, `SilentResyncer`, is `GoodFraming` with the override deleted and nothing
else changed, proven to pass **all eight** other suites and fail only this one; `tinymux` runs it
from the consumer position, so deleting that counter reddens the consumer-position gate — which
is the exact deletion the item observed leaves the whole tree green today.

**The rig, half of it.** The crossover half of the lane ran green at the current tree —
**967 · 0 · 7**, equal to the default-scope figure, with all five crossover tests executing. The
distinguishing evidence is the self-skip count, which falls **12 → 7**: a rig row whose skip count
matches the default-scope run is a default-scope run wearing a rig label. Three of the drops are
measurements — the two `rts-cts` tests print `port1 RTS high -> port0 cts:false` and `false` again
with RTS low, a **3-wire** bench and §15.52's legitimate answer for the fourth independent time,
and the two browser tests find no `node`.

**The fourth drop is this session's own doing and is recorded as owed rather than quietly taken.**
Item 52 changed the privileged helper, so `.snx-bin/<profile>/serial-nexus-devprep` is now *Stale*
— `preflight` says exactly that and exits 2 — and `scripts/bless` needs a `setcap` this box cannot
give itself. `blessed_devprep_helper()` only **warns** on a byte mismatch, deliberately, since an
ordinary relink changes those bytes; so a replug lane run now would have exercised the
pre-change helper and reported green for code that never ran. That is the failure §3.83 discarded
a whole lane to avoid, and the right answer is the same one: do not run it. `SNX_REPLUG_DEV` was
left unset so the three replug tests self-skip visibly instead. **The replug half of the lane, and
with it item 52's whole rig surface, is owed at this tree once the helper is re-blessed.**

---

### 3.87 The guard audit: asking of every recorded measure whether its guard could actually fail

Prompted by the repo owner, whose ask was broader than any one item: *automated tests that catch
regressions to every measure that matters*. The audit asked one question of each recorded measure —
**is there a guard, and could that guard fail?** — with plan §3 rule 22's tell as the instrument:
would its passing output differ from its not-running output. Read-only, against a tree four other
agents were writing, so it ran without touching a file.

**The headline is that coverage is better than the question implied**, and that is worth recording
as loudly as the gaps. Roughly a dozen invariants are guarded to a standard the auditor could not
break by reading: unsafe containment (matcher plants, walker plants over scratch trees, a >100-root
non-vacuity floor, a two-sided exception set), the `RefCell` and `AsyncFd` bans, the teardown
ledger's conservation, the loss fingerprint with its own non-vacuity clause, head-of-line asserted
on both sides, the web bridge's deny-by-default, the derived rosters, the golden transcripts with
their one-byte mutation, and the pattern-wait maxima. The meta-gate layer is genuinely adversarial.

**Four gates that could not fail, closed the same session** (item 60). `SNX_TLS=required` was set
by **no CI lane** — the mechanism has existed since item 10 and nothing ever used it. The Linux
`check` job — the only job that runs the suite on Linux — **never installed `jq`**, so
`expectation_gates.rs`'s seventeen synthetic-antecedent guards, the machinery §13's whole gate
design rests on, were passing by luck of the runner image. The Linux lane lacked `--no-fail-fast`
while macOS carries it with a comment explaining the six pushes it cost. And the macOS lane did not
set `SNX_EXEC_CODEC=required`, leaving item 49's battery silently skippable on the platform that
lane exists to cover.

**The fifth is the one worth dwelling on**, because it is a tripwire that had been enforced by a
comment. AGENTS §4 and §15.45 say adding a capability to the privileged helper is a **design
amendment**, not a patch. The only assertion was `REQUIRED_CAPS.len() >= 2` — a *floor*, which
passes just as happily for a set carrying `cap_sys_admin`. It is now `assert_eq!` with a message
naming the amend-first order, proven by planting a third capability and watching it redden.
A floor cannot tell an amendment from a typo, and the narrowness bound is the helper's entire
safety argument.

**Two live findings that are not gate holes but product ones.** §16.12 promises that *every*
numeric attribute carries a structurally checked maximum; enforcement is seven hand-written call
sites behind `if let NodeConfig::X { …, .. }` patterns whose `..` silences the compiler, plus a
property test over a fixed list of four fields — and the gap has already bitten:
`NodeConfig::Pty { advertised_baud }` has **no range check at all**, rides the wire into `state`,
and feeds a `standard_baud()` that returns `Option` and silently ignores a nonstandard value.
Benign today; a literal violation of the promise, in exactly the way an exhaustiveness guard exists
to prevent (item 62). And `docs/rpc/observation.md`, which §5 calls the authoritative per-kind
enumeration that "stays so", is missing **17 keys the daemon emits** — including
`delivered_hostward`, which the design itself names as the counter `p6_head_of_line.rs` reads. That
is the drift class that produced §15.54 (item 63).

**The most consequential gap is a guard with 43× of slack.** `phase3.json` records 183.0 MiB/s and
an exit criterion of 30; `p3_firehose.rs` fails only below **4.27 MiB/s** — seven times *under* the
recorded criterion, so the guard admits a daemon that cannot meet the design's own headroom
requirement. It never prints elapsed time, so 183 → 20 MiB/s is invisible in both paths, and the
throughput block carries no provenance where `idle_cost` now does. This is precisely the state
`idle_cost` was in before item 46 — the same defect review 37 filed as 37-TEST-3, fixed for one
axis of one artifact and not the other. It also orphans a tripwire: §5's ring-storage row names
this benchmark as its detector, and a `VecDeque<u8>` rewrite would clear 4.27 MiB/s comfortably, so
that row is currently upheld by review (item 61).

**And an instrument-validity finding — which the next agent then proved I had got wrong.** The
obvious tool for the packaging gate is `systemd-analyze verify`. I measured it, exit status read
directly rather than through a pipe, and reported that it catches an unknown directive (1) and a
missing `ExecStart` (1) while missing a bad value (0), and that it returns 1 on the real unit
because the daemon binary the unit names is not installed here. The last of those is true. **The
first is an artifact of it.** Re-run with the environmental arm removed:

    unstaged (real ExecStart, nothing installed):  real=1  unknown=1  badval=1  noexec=1
    staged   (ExecStart -> an executable stub):    real=0  unknown=0  badval=0  noexec=1
    staged   + --recursive-errors=no:              real=0  unknown=1  badval=1  noexec=1

Every exit 1 in my row was the missing binary talking, not the planted defect. With it staged away,
systemd 259 exits **0** on an unknown directive, an unknown *section*, and a bad value alike — the
tool is far blinder than I reported, and I had built the "at least it catches structure" half of my
own recommendation on a confound. What survives every configuration is **stderr**: the diagnostic is
always printed. So the gate requires exit 0 **and empty stderr**, which also makes it work on
systemd < v250 where `--recursive-errors` does not exist. And a second finding, deterministic and
opposite to what the flag name suggests: `--recursive-errors=no` makes the exit status bite.

**Two lessons, and the second is the one that generalises.** A tool that prints a diagnostic and
exits 0 is the `jq -e` instance in a third costume — the family now has six members. And *the
control has to be part of the measurement*: I planted defects into a unit that was already failing
for an unrelated reason, so every reading was the confound's. Rule 17 says re-verify what you
inherit, and the agent that did — against a brief that handed it four confident numbers — is the
only reason the gate does not now rest on them.

---

### 3.88 The boundary worker unified, and the meta-gate that caught the orchestrator

**Item 42, executed.** `boundary::BlockingReader` → `BlockingWorker` — the name review 32's SIMP-3
proposed, not an invented one. The loss counter is optional *structurally* rather than by
convention: `arm` mints the `Notify` and returns it, `arm_quiet` mints none, and the `lost()`
accessor is gone, so "this worker has nothing to signal" became a fact about which method the
caller used. All three call sites rebased. Details and the fail-first table are at plan §18 item 42.

**The part worth recording is the third call site, because the armchair case against it was
strong and wrong.** The log writer looks like the one that should not unify: its stop is
`Queue::closed` under a mutex and a `Condvar`, which an `AtomicBool` cannot wake; its join is
bounded-with-detach (§7.3) rather than `join()`; its spawn is injected for CONC-3. Three real
divergences, each an argument for leaving it alone. Measured instead of argued: `log.rs` −50/+31,
`boundary.rs` +40 — and the three additions (`arm_with`, `is_armed`, `detach`) turned out to be
**general primitives rather than per-caller parameters**, `detach` being a bounded-wait primitive
the library had simply never grown. Net +21 lines for one type instead of three. The lesson is
§16.1's, arriving from the other direction: the divergences that look like reasons not to unify
are sometimes the missing primitives.

**Fail-first for a behaviour-preserving refactor is a different shape**, and this is the clearest
instance the record has. There is no new property to prove, so the proof owed is that the
*existing* guards still bite. Three defects planted in the unified worker, each caught by a
pre-existing guard: a `join` that silently becomes a detach; an `arm` returning a different
`Notify` than the thread received — caught at both altitudes, the boundary unit test *and*
`p7_unplug`, which is the pair that says the signal is wired end to end and not merely minted; and
a stop flag `signal_stop` never sets. **The third is caught only as a deadlock**, which is the
finding: `cargo test` has no per-test timeout, so in CI that class is a job timeout with no failing
test name — the one thing AGENTS §8 says to capture verbatim, missing exactly when it is needed.
Filed as item 64(h).

**And the meta-gate caught the orchestrator — twice, the second time inside this very
paragraph.** `retired_names_appear_only_where_history_lives` went red on the plan, at prose from
the guard-audit commit: I had line-wrapped an absolute path to the daemon binary across a break,
so the hyphen fell after the family prefix and the tail of the path began the next line as the
**pre-§15.40 spelling of the daemon's name** — the retired vocabulary reintroduced by typography
rather than by intent. Then, writing *this* entry, I quoted that spelling verbatim to explain it,
and the gate reddened again on the notes. AGENTS §10 already has the answer and I walked past it:
**when quoting material that violates the rule, paraphrase.** A record of a banned word cannot
contain the banned word; that is what makes the ban un-reintroducible rather than merely
discouraged. §15.40's whole point is that the retired vocabulary is un-reintroducible, and a scanner
that folds lines is why. Two things follow. The gate is right and the fix was to rewrap, not to
widen the allowance — that would have been silently re-fixing a decision. And the process failure
is mine: I ran the meta-gates before that edit and not after, so the commit went out red and the
next agent's report is what surfaced it. **Run the gates between the last edit and the commit, not
between the last-but-one and the commit** — the rule is obvious and I broke it inside the same
session that filed four gates for asserting nothing.

---

### 3.89 Two probe questions answered, one refused for the right reason, and a gate that agreed with everything

**Item 14, and the answer nobody had asked for.** §15.53 refuses an `rts-cts` config whose driver
drops `CRTSCTS`; software flow control had neither a pre-check nor a probe, and the item's honest
framing was *unmeasured, not known-good*. It is now measured: `ftdi_sio` on Linux 7.0.0-29
**honours** `IXON|IXOFF` — `c_iflag` `0x5` → `0x1405`, a delta of exactly those two bits, on both
ports. So no dropping driver has been found, and **§15.53 stays un-extended**, which is what the
item's *Declined* line demanded and what a weaker session would have quietly overturned by
"consistency".

The observation is shaped to what the daemon does rather than to what reads well:
`serial2::FlowControl::XonXoff` sets those bits and clears `CRTSCTS`, and its `matches_requested`
compares the **whole** `c_iflag` word — so the shipped cell is `serial2_readback_would_fault`, the
property the product actually promises, with the two human-readable flags beside it. The one place
the software pass reaches the *verdict* is `baseline_restored`, which now covers both flag words: a
restore check reading `c_cflag` alone would have certified a port left with `IXON` asserted.
Corroborated from outside the probe, notes §3.68's method: `stty` after a full Tier-3 run reads
`-crtscts -ixoff -ixon -ixany`.

**Item 22, where the Linux answer is a control proving itself inert.** P13 gained the shape it
lacked — a reader arriving *while* the kernel is inside its close-wait, which is the shape the
failing macOS test inhabits. The construction detail is the finding: the reader must be **already
spinning** when the close is entered. Spawning it, or sleeping and then reading, cannot land inside
Linux's µs-wide window at all, so either would have produced a probe that never measures its own
subject — §13's vacuity taxonomy 2, arriving in the instrument written to close a gap in the
instruments. Linux reads 3 of 3: the arriving reader wins, first `read(2)` at 0–1 µs against a
close returning in 3–7 µs, 64 of 64 recovered. On a `retains` kernel that is exactly what should
happen and it establishes nothing about the question; **Darwin is where this shape is the
measurement**, and until that capture the item is half discharged and says so in its registry row.

Two defects were caught inside the probe's own construction, which is worth recording because both
would have shipped a confident wrong number: a `bytes_lost: 64` printed beside a reader that
recovered all 64, and a cell named `bytes_recovered_during_close` that would have been *false* on a
lost race. And a panicked reader thread would have published `arrived: true` with 0 bytes — the
join fallback is now `u64::MAX`, so a broken instrument cannot read as a clean measurement.

**Item 26 was refused, and the refusal is the point.** A new probe id is derived from the design's
§13 roster by `meta_derive`'s gate. Landing P16 with the design still saying P1–P15 would have made
the **tree ahead of the design** — which AGENTS §5 forbids in the same words it forbids the reverse,
and which would have shipped a red meta-gate as the cost of a green feature. The right move was to
stop and say what unblocks it, which is what happened. The amendment is now made (§15.59 plus §13's
glance row, roster string and era row) and the construction proceeds under the amend-first order
rather than against it.

**The era decision, and why one boundary rather than two.** Items 14 and 22 add observation *keys*
under unchanged `question` strings, so `field_set` moves and `probe_set` does not — §13's era law
clause 4 in as many words, meaning the era stays open and the new artifacts are **era-mates** of the
rows below, diffable on the intersection. `field_set` went `fc01990f2e38876e` → `c4ed6e7ef3f8088f`
(Tier-3) and `141e256d40c1e83e` → `544dca850580d430` (passive). The genuinely hard sub-decision: P15's
`question` still names `CRTSCTS` while its probe now reports a software reading too. Widening that
string moves `probe_set` — and would close this era for a **wording** change while P16 is already
owed and will close it for a real instrument. Two boundaries where one will do, so the widening
rides with P16 in the same commit, and every software block carries its own `asks` string meanwhile
so the JSON is self-describing rather than merely narrower than its probe.

**And a gate that agreed with everything.** `expectations/macos.jq` — the file AGENTS §3 names as
the macOS gate — carried a bare `(.summary != null)` where `linux.jq` carries
`(.summary.unsupported == 0)`, while its own head comment, copied verbatim from its Linux sibling,
claimed unsupported stayed a gate failure through the summary clause. Six probes' clauses were bare
presence checks with no status constraint. Measured against a Darwin-shaped report before anything
was touched: `unsupported` on P1/P3/P4/P5/P11 exited **0**, and so did a `skipped` on P6/P7. After
the repair all four exit 1 and an honest report still exits 0.

One correction to the audit that found it: on a *passive* run P15 was already refused — but by the
**per-port presence clause**, a presence clause doing a status clause's job by accident. Name a port
and it passed. That is a nastier shape than a missing clause, because it makes the file look
covered from one direction.

Worst of all, `docs/macos.md` had grown a paragraph *explaining* the absence — "nothing may report
`unsupported` stopped being the right shape once the probe set grew past P5" — a rationale for a
clause that had simply never been written. A hole with a justification attached survives review in
a way a bare hole does not, and correcting the file meant correcting the story as well as the code.

---

### 3.90 P16, and two guards that started green

**The probe.** `pts slave-witness liveness`, built once §15.59 unblocked it. Two arms on one held
slave fd, **each the other's control**: `POLLHUP` must be *absent* while the master is open and
*present* after it closes. A probe with only the second arm would report a hangup that was never
absent — which is why a fired control is reported **first**, ahead of the finding it invalidates.
The quiet arm polls twice, back-to-back and again at the PTY node's own 5 ms `IDLE_POLL`, because
"no hangup right now" and "no hangup at the cadence the shipped node actually polls" are different
claims (§15.49 clause 3). It is placed after P8/P9/P10 deliberately: its paced arm parks 320 ms
inside `poll(2)`, which is the syscall P9 is timing.

**What it answers.** `SlaveWitness::prove_open` — the harness check notes §3.60 filed as
*expected, not measured* — is **sound on Linux, and now measured rather than examined**:
`path_still_resolves` goes `true` → `false` across the master's close while
`fstat_on_the_held_fd_answers` stays `true` on both sides, so `shipped_prove_open_would_refuse`
moves `false` → `true`. The tautology sits in the same row and is the point of printing all three
steps: `fstat` still answers on a pair that is gone, which is exactly why step 2 is the
load-bearing one and why a witness resting on step 1 alone would be a check that cannot fail.

Two bounds the probe prints rather than leaving to prose: it is sound *for this kernel* — the
residual was always the off-Linux half — and *for this edge*, meaning whether the comparison sees
**the master's close**, not liveness in general.

**The Darwin readings are pre-registered**, so the interpretation cannot be chosen after the
capture arrives. `path_still_resolves: true` after the close is the predicted persistent-devfs
shape and makes `prove_open` **unsound there**, leaving notes §3.56's seven converted guards held
by the compile-time borrow alone — and since `poll_can_tell` already reads `true` here, the
portable upgrade would then be a `serial_nexus_sys` `poll` helper rather than an argument.
`path_still_resolves: false` **refutes** the prediction, and that refutation is as load-bearing as
a confirmation. `poll_can_tell: false` means neither instrument is portable and the witness
argument needs re-deriving rather than re-coding.

**Two guards started green, and the code was not the thing that was wrong.** This is the failure
the fail-first discipline exists to catch, arriving inside the work written to honour it. The stat
guard constructed its reading **by hand**, so it could not see the *constructor* deciding between
`None` and `Some(false)` — it now drives the real `take` against a path that does not exist. And a
dropped negative-control conjunct in `poll_can_tell` was invisible because the verdict checks the
two windows separately; the guard now asserts the published cell agrees with the verdict, which is
§13's own shape — a verdict contradicted by its own observation — that it had been blind to. A
third, pre-existing, was repaired as collateral: a gate guard anchored its needle on the **tail**
of a list that grows, so it would have gone quiet the moment the list grew again.

**One design-invariant correction on the way**, and it is a small vindication of the wall it hit:
the first draft reached for `BorrowedFd::borrow_raw`, which is `unsafe` — §16.3, the very rule
that sent this question to the doctor rather than into the harness. `AsFd` does the job with a real
lifetime instead of a promised one.

**The era, and a naming problem that had to be solved rather than papered over.** `probe_set` moved
`e79f5fcd86a2e5f0` → **`4317ea5ac187f506`**: one boundary carrying two changes, P16's arrival and
P15's widened `question`, exactly as §15.59 scheduled so that a wording change would not spend a
boundary of its own. The six new captures share a UTC day *and* a `commit` with the era-closing
triple — the tree was uncommitted through both, so `build.rs` stamped the same `<sha>-dirty` — and
what separates them is the **instrument**, which a sha cannot say and which `-2`/`-3` would say the
opposite of. Hence a third filename segment naming the move. The lesson is recorded where the next
person meets it: the era-closing triple's owed *passive* half is now **never takeable**, because
the era closed before it was taken. Take both halves before an instrument change, not after.

---

### 3.91 260 orphans, a green suite, and a self-deadlock nobody guessed

**The finding.** Item 50's agent reported **260 orphaned `serial-nexus-sim transcript` processes**
on the development box, `ppid 1`, ages up to an hour. They came from `p8_daemon_transcript.rs`,
which had landed the same day with three tests passing. Reproduced from a clean state:
`transcript 0 → 3` per run, `daemon 0 → 0`. Three tests green, three processes leaked, and the
arithmetic says it had happened about eighty times.

**Zero daemons leaked, which is the first useful fact.** Item 24's stdin-EOF leash works. What
survived was the daemon's *exec children* — processes the harness never spawned and no
`KillOnDrop` guard can reach.

**The diagnosis, and my hypothesis was refuted in both halves.** The forensics ruled out the
obvious story before anyone read code: each orphan held its stdin pipe with **exactly one fd
reference across the whole box — its own**, so nobody held the write end and `read` would have
returned EOF at once. The process sat in `futex_do_wait`, single-threaded. I offered the guess that
`Stdin`'s lock is reentrant per thread and a *different* thread must be holding it. Both halves are
wrong. `strace` on a live specimen, reproduced with no daemon at all:

    write(2, " (stdin reached EOF before the e"..., 42) = 42
    futex(0x…, FUTEX_WAIT_BITSET_PRIVATE, 2, NULL, FUTEX_BITSET_MATCH_ANY)

and no `read(0, …)` ever follows. **`std::io::Stdin` is a plain, non-reentrant `Mutex`** — only
`Stdout` and `Stderr` use `ReentrantLock`. `park_transcript` called `std::io::stdin().lock()` while
its caller's `StdinLock` was still alive, so it self-deadlocked **on its own thread, before its
first read**. The leash's EOF arrived at a process that had already stopped listening. Exactly
three of the five transcript children park; the two that reach clean EOF return without parking and
never leaked, which is why the count was 3 and not 5.

The fix passes the caller's reader in rather than reaching for the global. It still **parks** — an
exec child that exits is a crash to the daemon and gets respawned over the evidence — and §15.36's
idle rule was re-measured on the fixed binary: `wchan=anon_pipe_read`, **0 CPU ticks over 5 s**.

**The guard is a harness property, because the defect was not a property of one test.** The child
is spawned by the *daemon*, so the relation the harness could use — parenthood — is precisely the
one already destroyed by the time anyone can ask. `Daemon::start` now spawns with
`process_group(0)` and `Daemon::drop` sweeps the **group**, waits bounded, kills what remains, and
panics naming pids and argv. Two details make it non-vacuous: a `ps` failure panics rather than
reading as "nothing found", and its own proof plants a child that ignores stdin — `/bin/sh -c
'exec sleep 300'`, nothing serial_nexus about it, since the sweep matches the group and not a name.

**What the sweep then found, measured across every fixture.** `tests/ext-codec/deaf.py` is a
**second live instance**, masked only because its one test calls `remove-node` first and takes the
graceful path; its docstring's claim that "nothing here outlives the node that started it" is true
only where the daemon gets to run code. And `serial-nexus-web` has **no leash arm at all** — no
piped stdin, no flag — with a live orphan on the box to prove it. Daemon ✓, sim ✓, web ✗.

**The supervisor meets its obligation everywhere it can run code**, which is worth stating because
the headline invites the opposite conclusion: on `teardown`, on `shutdown` and on SIGTERM the child
dies. Only after SIGKILL does it survive, and after SIGKILL nothing in the daemon can act — the
leash is the only backstop by construction, and a child that blocks before reading stdin cannot
honour it. But the harness makes SIGKILL the *only* path its tests take: `Daemon::drop` calls
`rpc.shutdown()` and then kills immediately, so the kill wins the race against the graceful
teardown that would have reaped the child. That is now a filed question rather than an inherited
default (item 65(f)).

**The latent sibling, recorded because it is one refusal away.** `stdin_eof_watch()` parks a
background thread holding `stdin.lock()` for the process's whole life. Any future sim mode that
both arms the leash and reads stdin on the main thread deadlocks identically. It is unreachable
today only because `transcript` is the sole stdin reader and *refuses* the leash — and **that
refusal has no test**.

**One unreproduced observation, recorded rather than swept.** A full-suite run stalled ~14 minutes
on `p6_outage::outage_faults_then_purges_then_recovers_byte_clean` on an idle box (load 0.12). This
is the same hang §3.83 recorded at 39 minutes and deliberately left unattributed. It did not recur:
4/4 in isolation in ~4 s, and green in the second full-suite run on the same tree. Process-group
membership has no bearing on `bind()`, so it is not attributed to this change — and it is not
closed either.

---

### 3.92 The documented rig lane, whole, and the dual-scope figure that had never been taken

The operator re-blessed the helper, so the replug half — deliberately dropped from the earlier
lane rather than measured against a stale binary (§3.91's predecessor row) — could run at this
tree. Checked before starting, because the point of dropping it was that a lane against the wrong
binary is worse than no lane: `preflight` reads `REPLUG-PREFLIGHT: READY` with an empty
`bless_problems`, `capabilities --json` answers `blessed: true` **and**
`can_grant_device_access: true` (the field §3.83 added because nothing in the tree read it), and
the blessed copy is **md5-identical** to `target/debug/serial-nexus-devprep`. Both adapters
authorized, no dead-run residue, box at load 0.27.

**999 · 0 · 7**, `--no-fail-fast`. Every hardware test ran and passed: all three replug tests
including `identity_survives_a_replug_that_renumbers_the_tty`, all five crossover tests, and
`web_tls_round_trip` under `SNX_TLS=required` — a mode that had existed since item 10 and that **no
lane had ever demanded** until item 60(a) set it this session. §15.55's `grant` verb is exercised
for real rather than by unit test, granting on both ports across each re-enumeration; the log
carries its answer flipping to `["/dev/ttyUSB1","/dev/ttyUSB0"]` mid-lane, which is the renumbering
replug happening in front of the instrument that exists to survive it. **Item 52's rig surface is
discharged.**

**Five self-skips, down from thirteen at default scope, and that ratio is the evidence.** A rig row
whose skip count matches the default-scope run is a default-scope run wearing a rig label — the
failure the v15 attribution made and could not be caught in. All five are named measurements
rather than conveniences: the packaging root arm (no root, with both routes measured closed rather
than assumed), two browser tests on a box with no `node`, and the two `rts-cts` tests printing the
reading that justifies them — a **3-wire** bench, §15.52's legitimate answer, now confirmed by a
fifth independent instrument.

**Item 30, closed by the pair rather than by either half.** The item has been open since v15 asked
for a dual-scope equivalence whose only support was a figure nobody could reconcile to its cited
session. It now has one: 999 · 0 · 7 at default CI scope and 999 · 0 · 7 on the documented rig lane,
same tree, same session, same box. Both rows carry their scopes; **no delta is derived across
them**, because the equivalence *is* the observation and subtracting across scopes is the very
thing plan §3's figure rule forbids. The unreconcilable 835 stays annotated where it stands —
superseded, not rehabilitated.

One residue worth recording rather than tidying away: the lane leaves the adapters with their tty
numbers **swapped** (`3-2.3` → `ttyUSB1`, `3-2.4` → `ttyUSB0`), both authorized. That is the
renumbering replug's lasting effect and it is harmless on a symmetric crossover — §12's whole
premise, and the reason `SNX_CROSSOVER_A`/`_B` name identities rather than device nodes. A reader
who checks the box after a lane and finds the numbers exchanged has found the test working, not a
dirty rig.

### 3.93 The macOS capture at the current era: two pre-registered readings held, and a decline's condition was met

The rig moved to the Mac. Same two adapters the `4317ea5ac187f506` Linux rows were taken on
(`ABSCDGL6` ↔ `BH00L4KU`), same 3-wire crossover cable, now on MacBookPro15,1 / Darwin 24.6.0 /
macOS 15.7.8 / x86_64, 12 cores. **That sameness is the point and it was not available before:**
same `probe_set`, same adapters, same cable, same wiring, one kernel apart. A cell that moves
across this pair is a kernel fact, and for the first time in this record nothing else is free to
have moved with it.

**The rig was proven on the wire before it was used**, per the protocol §3.45 set: `SNX_CROSSOVER=required`
with both ports named, `serial_hardware.rs` **6 passed** in 19.88 s, 32768 bytes byte-exact each way at
250000 baud, and P5's own handshake block reading `3-wire: no handshake lines carried` on all
eight crossings — a **sixth** independent confirmation of the bench §3.80 re-measured. Then six
captures on a quiet box: passive triple, Tier-3 triple, `--json-out` on each, and
`jq -e -f expectations/macos.jq` **executed** against all six rather than inspected. Exit 0 on
every one.

**Plan §18 item 18's capture half is discharged**, and with it the standing "the macOS half is
owed" line in `docs/doctor/README.md`. One thing that debt cannot now be paid in full is recorded
rather than tidied: `e79f5fcd86a2e5f0` closed with no Darwin capture ever taken in it, so *that*
era's macOS half is **unobtainable**, not pending — the same shape as the never-taken passive half
of the era-closing `8c00078-dirty` triple.

**Item 26's pre-registered Darwin reading fired, as written.** The item named three outcomes in
advance so the interpretation could not be chosen afterwards, and the first is what came back:
`path_still_resolves` reads `true` on **both** sides of the master's close, `fstat_on_the_held_fd_answers`
stays `true` throughout, so `shipped_prove_open_would_refuse` never moves off `false` and
`stat_comparison_can_tell` is **`false`**. **`itest`'s shipped `SlaveWitness::prove_open` is unsound
on this kernel — measured, not predicted**, and the seven guards §3.56 converted are held here by
the compile-time borrow alone. Beside it the other instrument answers: `poll_can_tell_a_live_pair_from_a_dead_one`
is `true`, the held slave fd quiet through all 200 tight passes and all 64 paced ones while the
master is open, then `POLLHUP` **6–16 µs** after it closes with a following `read(2)` answering
`eof`. So the item's own conditional resolves to its first branch: **the portable upgrade is a
`serial_nexus_sys` `poll` helper rather than an argument.** P16 was placed after P8/P9/P10 because
its paced arm parks inside `poll(2)`; on this kernel that arm costs 395–443 ms against Linux's
~328 ms, which is a cost note and not a finding.

**Item 22's shape becomes the measurement it was built to be.** On Linux, `waits-then-discards`
being absent, `e_reader_arrives_during_close_wait` was a control proving itself inert. Here the
kernel parks: shape `a` — no reader — pays **600368 µs** and loses all 64 bytes. A reader arriving
*inside* that window ends it: `close_microseconds` **14**, `arrived_before_close_returned: true`,
`bytes_recovered_by_arriving_reader` **64 of 64**, in all three runs. **So the ~600 ms is what a
reader that never arrives costs, not a floor the close pays regardless** — which is exactly the
discrimination the item filed the shape to make, and it bears directly on the reader-stall
hypothesis §3.29 could only predict: a stall long enough to matter has to be a reader that does not
arrive at all, not one that arrives late.

**Item 14's decline had a stated condition, and the condition is met.** The decline read: "the
refusal follows only if a dropping driver is found — extending §15.53 to a mode nobody has measured
would be policy without evidence." Both ports on this bench: `tcsetattr_ok: true` with
`tcsetattr_error: null`, `c_iflag` `0x0` → `0x0` — **a delta of nothing** — `ixon_on_readback: false`,
`ixoff_on_readback: false`, `silently_dropped: true`, **`serial2_readback_would_fault: true`**,
`baseline_restored: true`. Reproduced **6 of 6** across three runs and two ports, against
`ftdi_sio`'s `0x5` → `0x1405` on the *same two adapters* one kernel away. A dropping driver is
found. **This is not a decline being silently re-fixed** (AGENTS §5): it is a conditional decline
whose condition was named in advance and has now been measured true, so overturning it is the
recorded decision the item asked for, with the evidence it asked for.

**§3.49's other pre-registration held in the same line.** It required that the next macOS capture
print `Topology: **Tier 3**` "with the two counter items named as unmeasurable on this kernel
rather than listed bare". It does both: the tier line is printed in P5's consequence exactly where
the Linux artifacts carry it, and `icounter` on each port plus the pair's `deliberate_mismatch` are
attributed to `TIOCGICOUNT` being Linux-only, with "that is the platform, not the rig: re-seating a
cable cannot change it". P5 still reads `degraded`, which is the honest direction §3.42 established
— the tier is printed *beside* the declined items, not instead of them. `docs/doctor/README.md`'s
standing "`Tier 3` appears nowhere in any macOS artifact" paragraph named its own expiry and is
corrected in place; it stands unchanged for the rows below the new triple.

### 3.94 The witness gets its portable arm, and the mask that would have made it useless

Plan §18 item 66, executed the same day it was filed. `SlaveWitness::prove_open`'s own doc had
carried the residual for a generation — "Darwin's `/dev/ttysNNN` are persistent devfs nodes, so
step 2 is expected to stay `Ok` there … That is unmeasured here — this box is Linux" — and named
the instrument that would close it: `poll(POLLHUP)`, reached through `serial_nexus_sys` because
`unsafe` lives only there. P16 measured the residual (§3.93), the prediction held, and this is the
paragraph's own prescription filled.

**Added, not swapped, and the brief that said "replace" was wrong about that.** The stat comparison
sees a thing `poll` cannot: a node *replaced* underneath a still-valid descriptor, which is §12's
replug renumbering — `a_witness_is_refused_once_its_path_names_a_different_device` is that arm and
it is not about hangups at all. `poll` sees a thing the stat comparison cannot see on Darwin. So
there are two arms with two messages, and the hangup runs **last**: where the path arm can fire it
gives the more specific diagnosis, so Linux keeps "the node was unlinked" and the new arm is the
backstop for kernels where step 2 structurally cannot answer. Nothing about the existing Linux
guard changed, which is the check that the addition is an addition.

**The implementation turned on one measurement, and it is the kind that only appears on the
platform of record.** POSIX says `POLLHUP` is set in `revents` whatever the caller requested.
Linux does that. **Darwin gates it on the requested mask.** A helper written and self-tested on
Linux with an empty mask therefore passes there and is *silently dead* on the one platform whose
persistent devfs nodes make the helper necessary — the session's recurring shape, arriving one more
time in the fix for it.

It was measured three ways rather than read once. P9 already ships the cell —
`hangup_delivered_to_a_mask_that_requested_nothing`, `true` on Linux 7.0.0-29 and **`false`** on
Darwin 24.6.0 in this era's committed captures. An independent `posix_openpt` poll from outside the
tree reproduced it: after the master's close, an empty-mask poll of the slave answers `none` while
a `POLLHUP`-requesting poll of the same fd answers `POLLHUP`. And the planted empty mask reddens
the new self-test **here**, where it would not on Linux. *(The same measurement clears P16: its
`p16_window` requests `POLLHUP`, so its quiet arms are controls rather than vacuous on Darwin —
worth checking rather than assuming, since a window polling an empty mask would have read "quiet"
on this kernel no matter what the fd was doing.)* The rule is now spelled once, in the helper's
doc, instead of being available to be re-derived wrongly at each call site.

**Why adding this to a witness shared by pty and serial callers is safe — measured, not argued.**
A real FT232R port on the rig reports `none` both while the far end is open and after it closes,
with the mask requested either way: a serial port's peer is a wire and cannot hang up. So the new
arm cannot fire on the serial witnesses in `p4_purge`, `p4_exclusivity`, `p4_free_for_all` and
`p6_outage`; they are held by steps 1–3 exactly as before. That is P16's own `does_not_license`
sentence checked against the hardware it disclaims, which is the only way a disclaimer earns its
keep.

**Fail-first, both halves, each restored md5-identical.** The mask planted back to empty reddens
the `sys` self-test. Step 4 disabled reddens the new Darwin witness guard, whose message names the
consequence rather than the symptom: "the master closed and the witness still proved open — on this
kernel that leaves every guard built on it asserting nothing." The Darwin guard asserts the
**platform fact in the same run as the product property** — the path still resolves after the
master closes — so a future Darwin that started unlinking reports that instead of passing for
Linux's reason and leaving step 4 unexercised.

*Owed, and stated rather than implied:* the reddening of step 4 has been proven on Darwin only.
Both new guards run and pass on Linux, but this session had one box.

> **Correction, 2026-08-14 (§3.101).** The last sentence is wrong about one of the two guards, and
> the error is the interesting half. `peer_hungup`'s `sys` self-test does run and pass on Linux.
> `a_witness_on_a_pty_whose_master_closed_is_refused_where_the_node_outlives_the_pair` does **not**:
> it is `#[cfg(not(target_os = "linux"))]` and is not compiled on this kernel at all — so what was
> owed was never merely "the reddening", it was any Linux exercise of step 4 whatsoever. Measured
> on the Linux rig box: **step 4 is unreachable on Linux by construction, and that is a property of
> the kernel rather than of the test.** While any fd on a pts is still open, Linux will not
> reallocate that index — six sequential open/close cycles all returned `/dev/pts/9`, while the same
> pair with the slave *held* forced the next allocation to `/dev/pts/10`. A witness therefore cannot
> observe its path come back pointing at a different pair, so step 2 (the path is unlinked) always
> fires first and step 4's `true` arm has no Linux trigger. Deleting the step-4 block leaves the
> Linux suite green, which is the vacuity this correction records rather than papers over; the guard
> for the kernel property it rests on is §3.101's.

### 3.95 A conditional decline paid off, and a primer that left its own payload on the wire

Plan §18 items 67 and 70, and they are unrelated except that one session found both — the first
by reading a measurement, the second by running the rig binary that measurement licensed.

**Item 67: the refusal extended, design first.** §15.61 was written before the tree moved, which
is the order AGENTS §5 requires and the order item 26 was once blocked on. What made it writable
is that item 14's decline was **conditional and said so** — "the refusal follows only if a dropping
driver is found" — and §3.93 found one. Extending it is therefore the decision the decline asked
for, not a reversal of it, and the distinction matters enough that both this entry and the item say
it in the same words.

**The predicate generalized rather than being copied.** `honours_rtscts` →
`honours_flow_control(path, FlowMode)`, `RtsCtsOutcome` → `FlowOutcome`. The three-way
classification was already a pure function of two booleans; only *which flag in which termios word*
differs, so that is the parameter and everything else is written once. This is §16.5's rule applied
to the one function in the tree with a recorded history of the failure it prevents: two copies of
this question are how the daemon and P15 came to answer differently about one port (item 56).

**Both arms on one box, which is the part that makes it a refusal rather than a prohibition.** The
rig's FT232R drops `IXON|IXOFF` — `c_iflag` `0x0` → `0x0`, `tcsetattr` reporting success — and is
refused at `load` with nothing created. A Darwin **pts** honours the same request — `0x2b02` →
`0x2f02`, the delta being exactly `IXOFF` over an already-set `IXON` — and is not refused. Both
assertions ship: the rig guard takes whichever arm the driver in front of it produces, and the
`sys` self-test pins the honouring arm on a tty every box has. A rule proven only against ports it
refuses has not been shown to discriminate, and this session had a box that could show it.

**Three shipped strings were wrong the moment the second mode was measured**, and all three are
the same defect wearing different clothes — prose that outlived the tree it described. (a) The
remedy offered `flow_control = "none"` *(or `xon-xoff`)* for a dropped `rts-cts`; on the platform
of record the same driver drops both, so the refusal's own advice led to the late fault the
refusal exists to prevent. (b) `open_failure_text` described the software mode in `CRTSCTS`'s
words, which would have explained a failure with a measurement of a different flag. (c) P15's
`does_not_license` told every future reader that a `flow_control = "xon-xoff"` config "is refused
at neither `load` nor `add-node` whatever it says" — §7.1 clause 2's report-says-fine /
load-refuses split, in the direction that misleads hardest.

**The fail-first arrived unasked, which is the best kind.** Two pre-existing daemon guards reddened
on the behaviour change, one of them a table listing `(AcceptedThenDropped, XonXoff)` under "node
never asked for it" — the exact expectation §15.61 overturns. The entry moved to its own assertion
rather than being deleted, so the new behaviour is now pinned where the old one was.

---

**Item 70: the primer, and an attribution taken before a diagnosis was offered.** The rig binary
came back with `crossover_rig_map_node_both_directions` failing. Before theorising, the tree was
reverted: a clean `git worktree` at `e5a305f` with its own target dir failed **4 of 4**. So it was
pre-existing, and nothing this session did needed defending.

**The evidence identifies the bytes rather than characterising them.** The failure is 62 bytes
prepended to the test's own correct output — and 62 is one FTDI bulk packet's payload, 64 minus the
two status bytes, the same quantum §3.70 measured. Those 62 bytes are a **contiguous slice of
`prime_the_wire_once`'s seed-9001 stream**, found at offsets 833 and 789 of the 1 KiB in two
separate failures (62 of 62 and 62 of 63 bytes matching). The primer waited for
`file_len(&path) > 0` — satisfied by the *first* byte — then dropped its daemon with most of a KiB
still in flight, and the adapter handed the remainder to the next reader.

**The function's own closing comment claimed this was impossible:** "no primed byte can land in a
capture under test". It is the session's recurring shape one more time — a sentence stating an
intention in the grammar of a mechanism — and it is now true because the code establishes it rather
than because the comment says so.

**Quiescence, not a byte count.** Waiting for 1024 bytes would hang on precisely the kernel this
primer exists for: where the leading packet is *eaten*, the log never reaches 1024. Stability is
the portable question and reads the same on a kernel that drops bytes and one that retains them.
4 of 4 red before, 3 of 3 green after, 7 of 7 for the whole rig binary.

**No product defect, and this is §3.70 with the sign flipped.** The daemon neither lost nor invented
a byte: the payload arrived byte-exact, behind stale data from an earlier test. There, a
re-enumerated FT232R *ate* one bulk packet; here Darwin's *retains* one. Same 64-byte quantum,
opposite direction, different kernel. Recorded as an observation — **no mechanism is claimed** for
why the two kernels differ, and one measurement on one adapter pair is not the place to start.

### 3.96 The portable CPU source, and a prediction that did not survive being tested

Plan §18 items 12 and 13, and a new item 71 the work uncovered.

**The blocking design question dissolved on contact.** `p3_idle_cost` stated in-tree that
"there is no portable analogue" to `/proc/<pid>/stat`, and three CPU guards were gated to Linux on
that sentence. It was true of `/proc` and false of the *question*: Darwin answers it with
`proc_pid_rusage`, whose `ri_user_time` and `ri_system_time` are already nanoseconds. So the
deliverable is `serial_nexus_sys::process_cpu_nanos` — one function, two arms, the `unsafe` where
§16.3 puts it. Nanoseconds are the shared unit because the Linux tick converts up exactly, and
rounding the finer answer down to a 10 ms tick to make the two kernels look alike would throw away
resolution neither of them imposes. `getrusage(RUSAGE_SELF)` and `RUSAGE_CHILDREN` were both
evaluated and rejected before anything was written: every caller samples a *child that is still
running*, and `RUSAGE_CHILDREN` counts only children already reaped, so it reads zero exactly in
the case under test.

**The prediction the item was built on is refuted, and that is the entry's real content.** Item 12
argued that "a regression widening `pty.rs`'s last-close predicate or deleting the latch drain
would burn a core and release operator-held write locks on macOS with the suite green" — AGENTS
§9's proxy in space at its sharpest, and the reason the port was worth doing. The ungated
`|| closed` arm was planted on Darwin, the workspace rebuilt, the binary confirmed current, and the
guard read **1.81 / 1.88 / 1.81 %** of a core. The *unplanted* tree, three runs, read **1.87 /
1.88 / 1.81 %**. The bands are identical. The plant moves nothing here either.

That is worth being precise about, because the first single run looked like a signal: an unplanted
1.69 % against a planted 1.85 % is a 10 % difference and would have been reportable if the baseline
had been one sample. Three runs a side put 1.69 at the bottom of the ordinary spread. **One sample
of a varying quantity is indistinguishable from a difference** — P9/P10's own rule, applied to a
fail-first rather than to a probe.

So the mechanism is absent on both kernels, and the guard's Linux-scoped comment was describing a
general property. The reader's backoff is not defeated by the handler re-firing: the extra work per
pass is small and the cadence still relaxes to `IDLE_POLL`. What the port buys is not the hazard
the item predicted — it is the removal of a guard that asserted **nothing** off Linux, plus one
instrument for two other files. What still bars the ungated arm is AGENTS invariant 16 rule (3),
the collapsed-session write-lock leak, which `p9`'s other two tests assert directly.

**Item 13's number, and a projection corrected by it.** On the x86_64 Mac, at load 1.41: 3.72 % of
a core at 8 idle tty fds and **9.91 %** at 32, marginal **0.2578 %/fd** against the artifact's
recorded 0.0728 — 3.5×, which is the real `kevent` Darwin pays per pass. The item projected
"17–19 % on the Intel box" from P12's per-pass arithmetic; the measurement reads 9.91 %, so the
projection was pessimistic by about two, which is why the item asked for a measurement.

**And the number does not license lifting `p3_idle_cost`'s gate**, which is the part worth writing
down. Assertion (1), the 20 % budget, passes on Darwin with room. Assertion (2) — item 46's
marginal drift tripwire — multiplies `per_fd_cpu_percent`, a figure measured on another kernel.
Ungating on that basis would assert a **Linux ceiling on Darwin**: a fresh proxy in space pointing
the other way, which is the shape item 12 exists to remove rather than relocate. The file's header
and skip message now name that calibration blocker instead of the retired `/proc` one, so the gate
says what it is actually waiting on — a Darwin row in `docs/benchmarks/phase3.json`.

**Item 71, found by lifting a gate and believing the result.** `p12_sim_idle_cpu` was gated for the
`/proc` reason alone, so it was ungated wholesale — and both tests failed, *not* on CPU. Both pass
the CPU budget on Darwin (§15.36's invariant holds; the `nullmodem` double reads 0.01 s over a 3 s
idle window) and both fail the assertion straight after it: the double **stops relaying** after a
peer close. Checked outside the harness to separate the two causes the guard's own message names —
the process stays alive and paused, and a `ping` written into one link never reaches the other. So
it is stopped relaying, not exited.

That control is not decoration: "a double that exits instead of pausing would pass the CPU budget
above" is its own message, so shipping the CPU half alone would be a guard whose non-vacuity check
is red. The gate stays and now names the real blocker. **The temptation to ungate anyway was
concrete** — the CPU assertion *passes*, and taking that as the result would have shipped exactly
the vacuous green §13's taxonomy exists to catch.

### 3.97 The Darwin run §3.56 was waiting for, and its first pre-registered outcome

Plan §18 item 20, closed by an experiment written down a week before it ran rather than by the
instrument the item filed.

§3.56 converted seven guards to hold a witness fd open across the measurement, and said plainly
what it could not claim: *"This does not claim to have fixed `p4_free_for_all` on Darwin; nothing
here was tested on Darwin, and design §15.48/§9 forbid a root-cause claim on this evidence."* It
then wrote down three outcomes and what each would license. This session is that Darwin run.

**Held fired.** On the Mac rig at `535594c`, box at load 1.39–1.62: **12 passed, 0 failed of 12**,
the first at **5.11 s** — the record's own figure for a healthy run. The frozen finding was 12 of
12 *failing*, losing 5–31 bytes of 32768 with `timed_out: true` on every observation. Same sample
size, opposite result.

So the sentence §15.48 kept exact — *a stall or a loss, not separated* — separates, and it was a
**loss**: the writers' last close destroying the tail of a payload the kernel had accepted and the
daemon had not yet read. §3.29's class, and the reason the byte counter has to be read while the
client is still open. **The separation instrument item 20 filed is not owed**, because the
pre-registration asked the question the instrument was to have asked.

**The magnitude the prediction had to assume is now measured on the same hardware in the same
session.** Held's text reasons from "a 1024-byte Darwin pty buffer"; P10's from-empty rung on this
box reads a depth of **1024** targetward and **1022** hostward, and P13's shape `a` loses 64 of 64
to a `waits-then-discards` close taking 600368 µs. In 2026-08-05 that buffer size was an inference
from someone else's numbers; here it sits in the same triple as the run it explains.

**The confound, named so it cannot quietly strengthen the conclusion.** The failing measurement was
taken on `BH00L4KU` ↔ `BH00LL8O`, 5-wire. This run is `ABSCDGL6` ↔ `BH00L4KU`, 3-wire, and the tree
moved a long way between them. A different adapter is a live alternative explanation, and only a
re-test on the original pair could exclude it — that pair is no longer assembled. What licenses
"the conversion fixed it" is the **pre-registration**, not this run's own reasoning: the outcome
was tied to its interpretation before either was known, which is exactly the protection this
discipline buys and exactly what a post-hoc reading of the same 12 runs would not have.

The fourth outcome §3.56 named — the test failing on *Linux* over the rig, which would be evidence
against the conversion itself — has not been observed, on this session's Linux rows or any other.

### 3.98 A Linux-gated call to a function this session deleted, and the check that would have caught it

Item 12's rename retired `itest::cpu_ticks` for `cpu_nanos` and repointed every call site — every
*visible* one. Its own self-test, `cpu_ticks_reads_this_process_and_never_goes_backwards`, was
`#[cfg(target_os = "linux")]`, so the Mac that made the change could not compile it and neither
could `cargo build`, which compiles no test target at all (AGENTS §3's standing note, met here from
the other side). It would have failed to build on Linux.

**Found by grep, not by a gate**: scanning the tree for each name the session removed —
`cpu_ticks`, `honours_rtscts`, `RtsCtsOutcome`, `idle_ticks`, `MAX_TICKS` — and reading every hit
to separate live code from prose. Four of the five were prose-only; the fifth was this.

**This is plan §18 item 54's class arriving from the opposite direction.** That item added an Apple
clippy cross-check to the Linux lane because "a helper whose one caller is
`#[cfg(target_os = "linux")]` is dead off Linux and nobody looked". The mirror is a *call site*
inside a Linux gate that a Mac cannot compile, and the mirror instrument turns out to exist and to
be cheap: **`cargo check -p <crate> --all-targets --target x86_64-unknown-linux-gnu` runs from a
Mac for every crate in this workspace except `serial-nexus-web`**, whose `ring` needs a Linux C
toolchain (`failed to find tool "x86_64-linux-gnu-gcc"`). Measured on all eight crates this session
touched: clean. AGENTS §3 now says to run it before pushing from a Mac.

**It is a turnaround improvement rather than a coverage hole, and the difference is worth stating
so nobody files it as the latter.** CI's Linux lane compiles that code on every push, so the defect
was never going to reach anyone — it was going to redden the next run and cost a round trip. What
the local cross-check buys is finding it in the thirty seconds before the push instead of the ten
minutes after, which is exactly what the Apple cross-check buys the Linux lane.

### 3.99 Five defects deep, and none of them the subject: the packaging root arm turns green

Plan §18 item 68, closed, and with it item 31(c)'s `DynamicUser=` measurement — owed since the
packaging track landed and never once executed.

The arm reddened CI on **every run it ever had**. Each fix revealed the next defect, and every one
was the *probe's* rather than the packaged unit's:

1. **`/tmp` under `PrivateTmp=yes`** — the `ReadWritePaths=` directories named a path that does not
   exist inside the namespace, so systemd refused to build it: `status=226`, `EXIT_NAMESPACE`,
   before `ExecStart`. The assertion's message said "This is a finding about the packaged sandbox,
   not about the probe". It was a finding about the probe, and the message now routes by the
   systemd status word instead of by assumption.
2. **`/run/<tag>` colliding with `RuntimeDirectory=<tag>`** — the probe's repair put the scratch
   tree at the exact path systemd creates and, under `DynamicUser=`, **chowns** to the service
   user, handing the ownership control to the user whose writes it exists to refuse. It failed
   *silently*, reporting `listed_write=ok`, which is worse than the 226 it replaced. This item's
   own first draft had argued the move was safe *because* "`RuntimeDirectory=` already writes into"
   `/run` — the collision written down as the reassurance.
3. **`:` is a POSIX special built-in.** A redirection error on one "shall cause the shell to exit",
   `/bin/sh` is dash on the runner, and the refusal this probe exists to observe therefore killed
   the script mid-run — `set +e` cannot help. **The probe was dying on its own success.** Verified
   on dash rather than reasoned: `: >` into a read-only directory exits 2 with no further output,
   which is CI's signature exactly; `true >` prints the reading and exits 0.
4. **`2>/tmp/eN` applied after the redirection that failed.** Redirections go left to right, so
   `> file 2>/tmp/e2` never applies the `2>` and the diagnostic lands on the inherited stderr —
   `listed_write=fail:` with nothing after the colon, and the message visible in the unit's own
   stderr all along. Verified on dash again.
5. **The control on a tree `ProtectSystem=strict` does not refuse.** With the capture fixed, the
   ownership arm *passed* — the README's claim confirmed — and the control reported
   `unlisted_write=ok`, because `/run` is writable for services. Moved to
   `/var/lib/<tag>-scratch`: `/var` is read-only under strict, so the unlisted sibling gets `EROFS`
   while the listed one, remounted read-write and root-owned 0755, gets `EACCES`. That contrast
   *is* Claim 4.

**Green:** run 31695823765, 6 passed 0 failed, `uid=65180`, `state_stat=root:777`,
`state_real=/var/lib/private/…`, `private_list=ok`, `private_stat=root:755`. `continue-on-error` is
removed and the step gates again.

**Two things worth keeping.** The requirement on that directory turns out to be *exact* — read-only
under strict, present in the namespace, and not one of the unit's own `*Directory=` trees — and
each rejected candidate fails differently, so all four are written at the code rather than left in
commit messages. And the whole sequence is one lesson: **a test that cannot run has no verdict**,
and five unrelated mechanisms conspired to make "cannot run" look like "the claim is false". The
claim was true the entire time.

### 3.100 mach absolute time is nanoseconds only where the timebase is 1/1

`serial_nexus_sys::process_cpu_nanos` shipped in §3.96 with a Darwin arm reading
`proc_pid_rusage`'s `ri_user_time` and `ri_system_time` and returning them as nanoseconds. Those
fields are **mach absolute time**. On x86_64 `mach_timebase_info` answers `numer=1 denom=1`, so a
tick *is* a nanosecond and the arm is exactly right; on Apple Silicon the timebase is 125/3 — a
tick is ~41.67 ns — and the same code under-reports by about 24×.

**Measured, not read.** The helper's own self-test asserts that a deliberate 60M-iteration burn
moves the counter by more than one Linux clock tick (10 ms). On the Intel rig box that reads
hundreds of milliseconds and passes; on CI's arm64 macOS runner a 24× under-count puts the same
burn right at the 10 ms boundary, and it **failed there** one run after this session made that lane
green. The timebase on this box was then read directly — `numer=1 denom=1` — which is both the
explanation and the reason the defect was invisible locally.

The conversion is `ticks × numer / denom` through `u128`, with the timebase read once into a
`OnceLock`. **The Intel readings are unchanged, and that is the check that the fix is a fix rather
than a new number**: `p9_pty_collapse` reads 1.87–1.90 % of a core before and after, because the
conversion is the identity at 1/1. So item 13's 0.2578 %/fd and the 9.91 % at 32 fds stand as
measured — they were taken on the architecture where the bug does not bite.

*One dependency decision, recorded because it is a deprecation being deliberately used:* `libc`
marks `mach_timebase_info` deprecated in favour of the `mach2` crate. The symbol is stable and this
is the libc crate's policy rather than an Apple removal, so the call stays with a function-scoped
`#[allow(deprecated)]` (the deprecation lands on the struct's fields too, which is why the allow
cannot sit on the statements). Taking a new dependency into `serial_nexus_sys` to read two `u32`s
would move the workspace's dependency graph, `cargo deny`, and a second lockfile — item 58's
standing consequence — for no behaviour.

**The theme, one more time.** This session opened by finding two guards that were correct on the
box that wrote them and wrong on the platform of record; it closes by having written one of its own
— correct on x86_64 Darwin, wrong on arm64 Darwin, caught by the lane that exists for exactly that.
Two Macs are not one platform, and the record already said so in another register: plan §18 item 18
insists the CI arm64 runner, the M4, and the x86_64 rig box are "three machines, none substituting
for another".

### 3.101 The Linux side of a Mac session: §15.61 landed in the daemon and not in the report

Hardware validation of `b346188..4548881` — sixteen commits authored and measured on macOS — against
Linux 7.0.0-29 and the FT232R crossover rig (3-wire, TX/RX/GND). The headline is that the Mac
session's *code* survives Linux intact: no product behaviour on this kernel is wrong because of it.
What did not survive is a set of **claims** the same commits left standing, and the one that matters
is operator-facing.

**§15.61 was implemented in the daemon and in one constant, and nowhere else.** The decision extends
§15.53's load-time refusal to `xon-xoff` on a driver that accepts the request and reads the flags
back clear. `flow_precheck_target` learned the mode, `precheck_flow_control` learned to refuse on
it, and `P15_SOFT_DOES_NOT_LICENSE` was rewritten to say so. But `p15_soft_note` — the prose P15
prints *into the report an operator reads* — was left at the pre-§15.61 answer in all three of its
arms, as was the function's own doc ("**What did not change is the daemon.** … nothing in the
product consults this cell"). One report therefore carried both statements at once, which is §7.1
clause 2's split inside the artifact whose whole purpose is to prevent it.

The worst arm is the dropped one. It told an operator holding a structural refusal that
"a `flow_control = "xon-xoff"` node on such a port does **not** get §15.53's refusal at `load` —
that refusal covers `rts-cts` only — and instead faults at its own open". Every clause of that is
now false, and it prints exactly where the refusal fires. The honoured arm is the one this rig
prints, since `ftdi_sio` honours the mode (`c_iflag` `0x5` → `0x1405`,
`docs/doctor/linux-7.0-2026-08-13-8c00078-dirty-tier3.json`), and it claimed "no pre-check consults
it and no config is refused on it".

**Why a green suite certified it, which is the transferable half.** The guard
`p15s_software_finding_degrades_the_verdict_and_refuses_nothing` *asserted the stale clauses on
purpose*: `c.contains("not refused at \`load\`") || c.contains("refuses nothing")`, and on the
honoured arm `c.contains("no config is refused on it")` — the latter justified in as many words as
"the bound being that the daemon consults none of this (item 14's decline)". The reasoning was
sound when written and became false when the decline was paid off. **A guard that pins a *decision*
rather than a *mechanism* must be moved by the commit that changes the decision, or it stops being
a guard and becomes the thing holding the defect in place.** That is a new instance of AGENTS §3's
tell, one register over: not a gate that asserts nothing, but a gate that asserts the previous
answer. Nothing else could have caught it — `expectations/linux.jq` type-checks
`.software_flow_control.*` cells and never reads `.consequence`, a blind spot the probe's own doc
comment states.

Repaired: all three arms, the function doc, the pinning guard (renamed
`…_refuses_the_dropping_port`, now with negative clauses that redden on each stale sentence), and
the two verdict remedies that still offered `flow_control = "none"` (**or `xon-xoff`**) — advice
§15.61 clause 3 retracted, because the driver that founded the measurement drops both modes, so the
fallback led from one refusal into the other. The daemon's own messages had been corrected in the
landing commit; the doctor's had not. Fail-first proof taken by restoring the honoured arm's stale
sentence in place: the new guard reddens naming it, and passes when it is put back.

**A second claim, at the place the design requires the cost be stated.** `precheck_flow_control`'s
doc bounds the DTR/RTS drop it costs as "bounded (only nodes that asked for `rts-cts`)". §15.61 puts
every `xon-xoff` node on that same path and says so ("the DTR-toggle cost §7.1 clause 6 states now
applies to `xon-xoff` nodes too"); the cost statement did not move with it. Corrected, with the
asymmetry that is invisible from the mode name recorded beside it: the `xon-xoff` arm writes
`c_cflag` as well as `c_iflag` (it clears `CRTSCTS`, mirroring `serial2`), where the `rts-cts` arm
touches `c_cflag` only.

**A third, and it is a coverage claim rather than a prose one.** §3.94 closes "Both new guards run
and pass on Linux, but this session had one box." One of them —
`a_witness_on_a_pty_whose_master_closed_is_refused_where_the_node_outlives_the_pair` — is
`#[cfg(not(target_os = "linux"))]` and is not compiled here, so what was owed was never merely the
*reddening* of `SlaveWitness::prove_open`'s new step 4 but any Linux exercise of it at all. Deleting
that step leaves the Linux suite green.

Measured rather than argued: **step 4 has no Linux trigger by construction, and the reason is a
kernel property.** While any fd on a pts is open, Linux will not hand that index to a new pair — six
sequential open/close cycles all returned `/dev/pts/9`, while the same pair with the slave *held*
forced the next allocation to `/dev/pts/10`. A witness therefore cannot observe its path come back
pointing at a different pair, so the path check always answers first. That is why the vacuity is
acceptable, and it is now a guard (`a_held_pts_index_is_never_reallocated_to_a_new_pair`, in `sys`
beside `peer_hungup` because `itest` deliberately carries no `nix`): if a future Linux began reusing
a held index, the witness's path check would silently stop being sufficient and this reddens instead
of the witness quietly weakening. Its control arm proves this kernel *does* reuse a freed index, so
the assertion discriminates held from free rather than passing on a kernel that reuses nothing.

**What the Mac session got right, recorded because refuted diagnoses are load-bearing (§9).** Four
candidate defects were raised against it and refuted on the tree: the nanosecond conversions in
`p3_idle_cost`, `p9_pty_collapse` and `p12_sim_idle_cpu` preserve their thresholds exactly
(`_SC_CLK_TCK` is 100 here, so `1_000_000_000 / hz` is exact and every converted constant is the
same number in a finer unit — `p9_pty_collapse` 200 ms over a 2 s window and `p12_sim_idle_cpu`
300 ms over 3 s, both 10 % of a core; `p3_idle_cost` converts no ceiling of its own, since its two
bounds are the artifact's `budget_percent` of **20** and `DRIFT_FACTOR` 1.75 × the recorded
0.0728 %/fd slope — an earlier draft of this sentence said "10 % of a core each" and was wrong
about that third file); the `XonXoff` arm's
`CRTSCTS` clear is a no-op on the pts fixture and is restored either way; the load-refusal message
was already built inline before this diff, so its lack of a constructing test is not new; and the
absence of a `shipped_predicate_agrees` cross-check for the software mode is a real narrowness but
not the disagreement-by-construction it was filed as. The method was six readers over disjoint areas
of the diff with one adversarial verifier per candidate, each told to default to refuted.

**The rig lane, and what it settled.** The documented lane ran on the crossover box after a
re-bless (the `sys/` change relinked `devprep`, so `install` rewrote the binary and the kernel
stripped its capability — the expected consequence of §15.45's design, not a defect):
**1004 · 0 · 7**, zero failed binaries, self-skips **13 → 5**. All five are named measurements —
two browser tests with no `node`, the packaging root arm with no root, and the two `rts-cts`
tests printing the reading that justifies them (port1 RTS high and low both leave port0's CTS
`false`, the **sixth** independent confirmation of a 3-wire bench and §15.52's legitimate
answer). The `grant` verb worked on the rebuilt helper across every re-enumeration
(`granted on ports 3-2.3, 3-2.4`, uid 1000), and all four replug tests passed including the
renumbering one, which swapped the adapters' `/dev` names and said so.

**Item 67's owed remainder is discharged, and it is the interesting result.** That item closed
with: "the honouring **serial** arm is measured on a pts and on Linux's `ftdi_sio` through the
committed captures, *not by a rig guard executing on Linux* — this session had one box."
`xon_xoff_is_refused_at_load_exactly_where_the_driver_drops_it` has now taken its **`Honoured`**
arm on real hardware — "driver honours xon-xoff — asserting the config LOADS" — and that arm had
executed on **no machine anywhere** before this run: the Mac takes the accept-then-drop arm, and
the arm asserting that a honouring port is *not* refused shipped unexecuted. So §15.61's
discrimination is now proven end to end against hardware on both kernels, on the same two
adapters and the same cable: the FT232R under `IOSerialFamily` is refused at `load`, the same
FT232R under `ftdi_sio` loads and does not fault. **A refusal rule proven only against ports it
refuses is not proven** — that sentence now has both halves behind it.

Two more things the lane settled, both about this session's own subjects. `p4_exclusivity`,
`p4_purge`, `p4_free_for_all` and `p6_outage` all ran against the rig and passed, so
`SlaveWitness::prove_open`'s new step 4 causes no regression on Linux hardware — consistent with
its having no Linux trigger at all. And the primer's new quiescence wait behaves here exactly as
its comment claims: `primed … with 1024 B, quiet for 300 ms` on both logs, the full kibibyte
arriving because Linux retains it (P13 `retains`), where the kernel that motivated the wait eats
the leading packet. `crossover_rig_map_node_both_directions`, which item 70 records failing 4 of
4 on Darwin from that residue, is green.

**A third instance of §3.78's class, and the first on Linux.** The Status table's Linux rows have
carried "13 self-skips" since the figure was first taken, and AGENTS §3 warned that
`grep -c '^SKIP'` is unstable under `--nocapture` — but recorded that warning as a *macOS*
observation ("two adjacent trees on one box read 105 and 102"). It is not a macOS property. Two
default-scope runs of **this same tree**, this session, read **13** and **14** by the anchored
grep. Extracting names instead of counting anchors makes it worse rather than better: both runs
yield 13 names, but **different** ones — run one has no `rts_cts_flow_control_stalls_the_writer_
instead_of_losing_bytes` anywhere in its log, run two has no `crossover_rig_custom_baud_byte_exact`,
and at this scope (`SNX_CROSSOVER_A`/`_B` unexported) both tests must self-skip. Their union is
**14**, and the union is still a floor: a line mangled in both runs is invisible to both.

So the long-carried Linux 13 is an undercount, by at least one, for the same reason the macOS
count could not be pinned — a concurrent write splices a `SKIP` message mid-name, which defeats
the anchor *and* the name match at once. The 1004 row states 14 as a **lower bound** rather than
replacing one precise-looking number with another. What the figure is for is unaffected and needs
no precision: it separates a rig-lane row (5) from a default-scope one (~14), and that gap is an
order of magnitude clear of the instability.


**The repair reproduced the defect it was repairing, and that is the entry's real subject.** This
session's own commit was then put through the same treatment it had just applied to the macOS one —
four readers over disjoint claim classes in `7047b9f`, one adversarial verifier per allegation,
each told to default to refuted. Four survived, two of them the same class the commit exists to
close.

1. **`p15_verdict`'s ranking comment still carried the retired justification.** The repair rewrote
   `p15_soft_note`'s three arms, that function's doc, and the era paragraph — and left the sentence
   150 lines up the same file reading "that finding has a shipped consequence an operator acts on …
   and this one has none by decision (plan §18 item 14's decline)". Live, present tense, and false
   since §15.61. **The repair's own scope statement is what missed it**: notes and item 72 both
   enumerate "all three arms, the function doc, the pinning guard, and the two verdict remedies",
   and the enumeration was treated as the extent of the family rather than as the part of it that
   had been found. A sweep keyed on the *claim* rather than on a list of sites is what closes this;
   the claim is "the daemon consults nothing here", and it was grep-able the whole time.
2. **A new guard whose stated power exceeded its actual power** — the same defect as the pinning
   guard, in the commit that fixed the pinning guard. It read
   `c.contains("REFUSED at `load`/`add-node`") && c.contains("is REFUSED at `load`/`add-node`,
   before anything is created")`, whose first operand is a strict *substring* of its second, so the
   conjunction reduced to the second alone — which the **hardware** arm satisfies by itself. On the
   unfixed tree, where the software arm carried no refusal at all, it passed. A guard written to
   catch §15.61's half-landing that the half-landing walks straight through. Replaced with a
   *count* — two occurrences, one per mode — and proved fail-first in isolation by planting the
   phrase out of the hardware arm alone, which leaves the earlier assertion green and reddens this
   one at `left: 1, right: 2`. The old form passes that plant; the new one does not, so it is
   strictly stronger in both directions.
3. `peer_hungup`'s doc said the witness's **third** check answers first on Linux. It is the
   **second** — the `stat` on the path — and step 3 is the identity comparison, which that path
   never reaches. Two adjacent facts, conflated.
4. The refuted-candidate record said the three CPU guards' ceilings are "10 % of a core each".
   True of `p9_pty_collapse` and `p12_sim_idle_cpu`; `p3_idle_cost` converts no ceiling of its own,
   its bounds being the artifact's `budget_percent` of 20 and `DRIFT_FACTOR` 1.75 × 0.0728 %/fd.

Four allegations were refuted and are recorded as such: that the honoured arm's parenthetical about
honest refusals was new (it is not, and it is accurate), that the ranking assertion at the dropped
arm was misapplied (it is not), that witnesses are taken on real serial fds on Linux (they are not
— every `attach_slave` call site holds a pty link), and that "§15.61 was implemented in the daemon
and in one constant, and nowhere else" is a file-by-file inventory (it is a claim about where the
*decision* landed, and it holds).

**What generalizes.** AGENTS §3's tell has a third register now. The first is a gate that asserts
nothing; the second, found earlier in this session, is a gate that asserts the *previous* answer.
The third is a gate whose assertion is strictly weaker than the comment above it claims — and
because the comment is what a reviewer reads, it is the hardest of the three to see. All three
share one remedy: **plant the defect the guard names and watch it redden**, which is fail-first
proof applied to the guard's *stated* property rather than to its code.

### 3.102 The bench goes back to 5-wire, on a pair the record has never seen

The operator re-cabled the rig from a 3-wire crossover (TX/RX/GND) to a 5-wire one, adding
RTS↔CTS crossed both ways, and asked for whatever the 3-wire bench had made impossible. This
entry is that session. **Six readings were pre-registered before anything was measured** — the
discipline plan §18 item 28 states as "pre-registered readings before the wire is touched" — and
all six held; none was refuted. The value of writing them down first is entirely in the two that
could have gone the other way, and both are recorded below rather than folded away.

**The bench, measured.** P5's handshake block, port to port with no daemon in the path, reads
`5-wire crossover: RTS/CTS both ways, DTR moves nothing` — `rts_a_to_cts_b=true
rts_b_to_cts_a=true` with all six DTR crossings `false` — byte-identical across three sequential
runs, committed as `docs/doctor/linux-7.0-2026-08-14-b58a1c4-{passive-1..3,tier3,tier3-2,tier3-3}.json`
from one clean build at `b58a1c4b7fc8`, with `jq -e -f expectations/linux.jq` executed against all
six and exit 0 on every one.

**The adapter pair is one this record has never carried.** The 5-wire discovery ran on
`BH00L4KU` ↔ `BH00LL8O` (§3.53), the 3-wire re-measure on `ABSCDGL6` ↔ `BH00L4KU` (§3.80), and this
bench presents `BH00L4KU` ↔ `BH00LW9U` — three cablings, three pairs, one adapter common to all
three. So this reading corroborates the discovery's *shape* without reproducing its conditions, and
nothing may be diffed across those rows as though only the cable moved. Named as a bound on the
result rather than allowed to strengthen it, which is item 20's precedent one instrument over.

**What the wires actually bought, which is the answer to the question asked.** Two tests, and only
two, are gated on hardware handshake, and neither had executed on this bench since 2026-08-12:
`crossover_rig_rts_crosses_to_the_far_ports_cts` (both directions, both polarities, through the
daemon's own `modem_lines` rather than a test-issued ioctl) and
`rts_cts_flow_control_stalls_the_writer_instead_of_losing_bytes` (40 B held for 1.5 s under a
peer-held CTS, then delivered byte-exact, with the `flow_control = "none"` control arm delivering
through the same CTS-low state). Both pass. On top of them, **the documented rig lane ran fully
spelled for the first time in this record** — `SNX_CROSSOVER`, `SNX_REPLUG`, `SNX_TLS`,
`SNX_RIG_FLOW` and `SNX_EXEC_CODEC` all `required`, dropping only `SNX_WEB_UI` on a box with no
`node` — reading **1004 · 0 · 7** with the self-skip set falling **5 → 3**. Stated exactly: this is
the first row in the Status table whose scope names `SNX_RIG_FLOW=required`, **not** the
fully-spelled lane (`SNX_WEB_UI` is still dropped) and **not** establishably the first run ever to
set that word — `ebf9c52` introduced it on 2026-08-05 and the 835 row's `17c6e87` descends from it
on a bench that was 5-wire that day, which nothing in the record settles either way. The two names that left
it are exactly the two `skip_no_rig_flow` callers and no other name moved, which is what attributes
the delta to the cable rather than to anything else in the session. Skips counted by name and
unanchored, per §3.101.

**Neither flow guard was merely run; both were planted against.** Suppressing the RTS drive inside
`drive_rts_read_far` — electrically what an unwired handshake looks like to these tests — reddens
both, and `SNX_RIG_FLOW=required` converts the resulting self-skip into a hard failure, so the two
propositions "the tests read the wire" and "required mode bites" are proven by one plant. The plant
is worth one further sentence: under it CTS reads `true` at **both** polarities, the stuck-high
shape, so **a one-polarity test would have passed it**. The both-polarities design in
`handshake_measured` is not defensive decoration; it is the thing that catches this. The second
plant drops arm 1's transmitter from `rts-cts` to `none` while still holding CTS low, and the stall
reddens by its own message — 40 bytes crossed against 0 — which is what separates "the wire stopped"
from "the rig was slow". The tree was restored md5-identical after each.

**What the re-cable did *not* buy, recorded because the temptation runs the other way.** DTR is
still unwired: all six crossings read `false`, so plan §18 item 28 stays blocked, now on a *third*
independent cabling rather than one — its blocker is a property of every crossover cable this
project has measured, and wiring DTR↔DSR is a deliberate act, not a side effect of adding handshake
lines. The same measurement keeps the suite's DTR negative control a valid control rather than
turning it into a tripwire. And item 17's two clauses — break reception and the parity mismatch —
have their *entry* condition discharged by the re-inspection, but neither was ever electrically
gated on the handshake wires: both ride TX/RX, which the 3-wire bench also carried. What the cable
bought that item is its own stated precondition, not a new physical capability, and the work is
unchanged in size and still owed.

**One gate fact, measured from its other side.** `jq -e -f expectations/linux.jq` exits 0 on a
5-wire report exactly as it does on a 3-wire one, because the handshake clause is presence-only by
design (plan §3 rule 14; §15.52's "a clause that pinned the answer would redden every honest
bench"). That is the rule working, and it has a consequence worth stating rather than leaving
implicit: **no gate in this tree can tell a 5-wire bench from a 3-wire one.** The only instruments
that can are the committed P5 artifact and the two `rts-cts` tests under `SNX_RIG_FLOW=required` —
which is the whole reason that spelling exists, and not a redundancy in it. It is not an instance of
AGENTS §3's tell: a clause deliberately scoped to presence is not a clause asserting nothing, and
the distinction is that this one *was* proven to fail, on a gutted report, when it was written.

**One operational note.** `scripts/bless --verify` reported the installed helper *Stale* because
this session's `cargo build` relinked `devprep` from unchanged sources. The lane was run anyway and
the reading justifies it: `grant` fired seven times across the re-enumerations and all four replug
tests passed, so this was the ordinary-relink case the helper's own warning names and not §15.45's
capability-stripping one. A session that changes `sys/` still owes the re-bless.

**A third Linux instance of §3.101's skip-count class, and the first where the mechanism is
visible rather than inferred.** The default-scope run closing this session reads **1004 · 0 · 7**
at the same code as the rig lane (documentation-only delta), and its self-skip set extracts to
**13** names. The fourteenth is `crossover_rig_signal_verbs`, which has **zero** `SKIP <name>`
occurrences in the log — and it is nonetheless identifiable, because the splice that destroyed it
is legible where it happened:

    SKIP test crossover_rig_data_plane_send_and_exclusivity ... crossover_rig_signal_verbsok: no crossover rig (

That is one `SKIP crossover_rig_signal_verbs: no crossover rig (…)` line and one
`test crossover_rig_data_plane_send_and_exclusivity ... ok` line interleaved mid-write by two
parallel test binaries sharing a pipe. The previous two instances reasoned *backwards* — this test
must have self-skipped at this scope, and its line is absent in any form, therefore a line was
lost. Here the lost line's name is sitting inside the corruption, glued to a neighbour's `ok`. So
§3.101's **14** — recorded there as the union of two runs and explicitly "itself a floor" — is a
single run's actual figure on this box, which is a stronger statement than the union could make.

**It changes nothing about the rule and slightly strengthens the reason for it.** The count still
must not be quoted precisely: this splice destroyed one name because of where the two writes
happened to land, and one byte's difference would have destroyed both. What the figure is *for* is
separating a rig lane from a default-scope run — 3 against 14 here — and that gap is an order of
magnitude clear of the noise, which is the only property this session relied on.

### 3.103 Four guards repaired, and the one whose predicted failure would not reproduce

Plan §18 items 74–77, all filed the previous day by the 5-wire session against the guards that
carry its own flow-control claim. Three are ordinary repairs; the fourth is worth the entry on its
own, because its central prediction did not survive being tested.

**Item 74, and the decision it left open.** `handshake_measured` drove `port1 -> port0` only while
the test it gates asserted both directions. The decision — half-crossed **skips**, printing its
reading, and hard-fails under `SNX_RIG_FLOW=required` — follows one rule: *the precondition must
measure what the promise asserts*. §15.52's "not wired is a valid answer" is untouched for 3-wire;
a half-crossed bench is a miswiring, and `SNX_RIG_FLOW=required` is the operator asserting 5-wire,
so failing there names the fault rather than letting it surface as a mid-test assertion.

**The fix could not go where the item said it went, and that is the transferable part.** The stall
test took the precondition *after* loading its arm-1 graph, where port0 runs `rts-cts` — so **the
kernel's line discipline owns port0's RTS**, which is precisely what the sibling test keeps both
ports at `none` to avoid. Driving the added direction there would have read "does not carry" on a
**fully-crossed** bench: the test would skip, and under the documented rig lane's
`SNX_RIG_FLOW=required` it would redden. A defect fix that reddens the lane it was written to
protect is the failure mode this repo keeps finding one register at a time; here it was caught
before landing because the item's own "check whether the call sites need any change" clause was
executed instead of assumed. The measurement now runs on a `none`/`none` probe graph booted and
dropped ahead of arm 1.

**Item 75 turned a remembered number into a measured one.** The control arm now times delivery and
asserts `latency * 4 <= STALL_WINDOW`, with `STALL_WINDOW` a const both arms read. This bench reads
**20.35 ms** against the 25 ms the comment cited from another adapter pair. Two plants: `MIN_MARGIN
= 400` reddens naming the measured latency, and `STALL_WINDOW = 40 ms` leaves arm 1 green while it
prints `held 40 B for 40ms` — the proof the two arms cannot drift apart — and reddens the control.

**Item 77's plant is the whole of its value.** A `Certificate` failure keyed on the handshake was
planted at `p5_rig`'s call site — a real defect that degrades an honest 3-wire bench — and the loop
stayed green, with all 92 doctor unit tests. The loop is gone and the structural argument stands in
its place. Its *Validation* line asked for the loop to be "red after any honest rewrite", which is
unachievable: the fold lives in `p5_rig`, which needs a bench, so no unit test of that shape can
ever see the plant. **When a guard's stated property is carried by a signature, the honest repair
deletes the guard and names the signature** — adding a source-scanning substitute would have been a
fresh instance of the assertion-weaker-than-its-comment register, declined for that reason.

**Item 76's prediction did not reproduce, and the record is more useful for it.** The item held that
an unprimed stall test fails after a re-enumeration, its 40-byte payload sitting wholly inside the
64-byte packet a freshly re-enumerated FT232R swallows (§3.70). Measured: **5 of 5 unprimed trials
passed**, each after a real `devprep cycle` of both ports.

That is not a licence to drop the primer, because **the hazard itself did reproduce on this pair in
the same session**: with priming disabled globally, `crossover_rig_custom_baud_byte_exact`'s
32768-byte transfer failed its byte-exact assertion on **1 of 3** re-enumerations. So §3.70's
swallow is present here — *intermittently*, against the 3-of-3 it read on the original adapter pair
— while this particular test does not expose it in five attempts. Two candidate reasons are named
and **neither is measured**: the stall test boots three daemons before its bytes cross, and the
failing observation was at a **custom** baud, so the swallow may track the rate change rather than
the enumeration. The primer stays — free, consistent with every sibling, and defending a measured
hazard — but the item's stated failure mode is unproven on this bench and must not be cited as
though it were. One confound is recorded with the trial: removing the second priming path required
converting the precondition probe from `boot_rig` to `boot_rig_raw`, which changes the boot sequence
the trial measures.

**The generalizable half.** A first unprimed green was very nearly written down as a refutation of
item 76 after a single trial; what turned it into an honest "not reproduced in five" was asking a
different question — *does this bench exhibit the hazard at all?* — with an instrument big enough to
see it (32768 bytes rather than 40). **An underpowered negative is not a refutation, and the cheapest
way to tell them apart is to measure the mechanism rather than the symptom.** The misattributing
message the item also named is fixed: the post-release assertion now tells its reader that a payload
of 0 immediately after a replug is the primer's absence, not the peer's RTS.

### 3.104 A root step that asserted nothing, and a root cause that was wrong

Four repairs from a privilege inventory taken when the operator offered to remove the "no root"
constraint on the dev box. **The inventory's headline is that less was blocked than the record
implied, and none of it was blocked by the width of the privileged helper** — so the standing
decline at plan §18 item 31 is untouched, and `REQUIRED_CAPS` still reads two capabilities. What
the inventory found instead was a live gate hole and a wrong diagnosis, both in the machinery that
had been measuring the root question all along.

**The gate hole.** `SNX_PACKAGING_ROOT` was referenced in `.github/workflows/ci.yml` only inside
comments, and in `p8_packaging.rs`'s skip helper. **It was set by no lane.** So CI's root step
exited 0 whether it measured anything or self-skipped — passing output identical to not-running
output, AGENTS §3's tell — *in the one step that had just been un-escaped from `continue-on-error`
in order to gate*. The stated reason for leaving it unset was that whether a runner provides
systemd as PID 1 with passwordless root was unmeasured; that has been false since run 31695823765
read `PID 1: systemd`, `sudo: passwordless`, and six passing root-arm tests on 2026-08-13. It is
now `required`, which is the order §15.52 set for `SNX_RIG_FLOW`: measure the precondition, then
demand it.

**The trap inside the fix, which is the transferable part.** Setting the variable the obvious way —
a step-level `env:` block — would have reintroduced the very defect. `sudo` runs with `env_reset`,
so a workflow environment never reaches a process launched as `sudo "$BIN"`: the variable would be
unset inside the test, the root-gated test would find root and run anyway, the step would stay
green, and `required` would assert exactly nothing. **A fix for a gate that asserts nothing can
itself assert nothing, through a mechanism one layer below the one being fixed.** The variable is
therefore spelled at the call, `sudo -n env SNX_PACKAGING_ROOT=required "$BIN"`.

The probe line guarding it got its own correction, in the same register and within the same hour:
its comment claimed it would fail "if the variable ever stops arriving". It cannot — `env VAR=x`
always sets `VAR` for its child. What it actually detects is `sudo -n` not working at all. The
comment now says that, because a check described as stronger than it is has been this session's
most repeated finding.

**The wrong root cause.** `p8_packaging.rs` recorded that the rootless fallback is "refused by the
kernel's `uid_map` policy". Measured on Ubuntu 26.04 / 7.0.0-29: `unshare -U true` **succeeds**,
exit 0, and `kernel.unprivileged_userns_clone` reads `1` — the kernel grants the namespace. What the
namespace does not carry is capability: `CapEff` inside is `0000000000000000`, because
`kernel.apparmor_restrict_unprivileged_userns=1` transitions the process into AppArmor's
`unprivileged_userns` profile, and *that* is why the `/proc/self/uid_map` write `unshare -Ur`
performs returns `EPERM`. The observed stderr quoted at three other sites was always accurate; only
the cause attributed to it was wrong. Recorded as a refutation rather than quietly replaced
(AGENTS §9).

**A blocker that privilege cannot fix, removed from an item routed as privilege-blocked.** Item 31
carried the `/dev/ttyACM*` half of the dialout claim among its unverified rows while being routed
as "needs a root box" — so it covered a clause root cannot close, and could never have been closed.
Split out as **item 78**, whose blocker is a CDC-ACM *device*. Its cheaper half is answered in the
filing, unprivileged: `/usr/lib/udev/rules.d/50-udev-default.rules:47` matches `ttyACM0` under
`tty[A-Z]*[0-9]` and sets `GROUP="dialout"`, so the group half holds by the same shipped rule that
covers `ttyUSB0`. The rule sets no `MODE=`, so the mode half still needs a present device — which is
the whole of what item 78 now asks for.

**Why the helper stays two capabilities.** Four blocked items *are* capability-shaped
(`CAP_SETFCAP` to build an orphan-sweep fixture, `CAP_SYS_ADMIN` for a `NO_NEW_PRIVS` launcher,
`CAP_FOWNER` for a grant guard, and euid 0 proper for the control socket's root arm — that last one
reachable by no capability, since the policy branches on `geteuid()`). **But none of them wants the
capability in the *shipped* helper.** They want it in a throwaway test fixture, which is a different
binary and a different argument: `serial-nexus-devprep` is mode 0700 and invocable by the
unprivileged user at any time by design, so every capability it holds is a standing surface between
runs, and `CAP_SETFCAP` in particular is root-equivalent for files. §15.45's narrowness is the
safety argument, and nothing found here is worth spending it on.
