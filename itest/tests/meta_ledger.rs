#![forbid(unsafe_code)]

//! **The ledger-parity meta-gate** (plan §18 items 95 and 100; AGENTS §3's
//! derive-from-tools rule).
//!
//! AGENTS §2 carries a sentence beginning `Still open:` — a hand-typed copy of
//! which plan §18 items are open. Plan §18 is the authority ("§18's item entries
//! are the only currently-accurate surface", item 95), and the copy had never been
//! re-derived from it: when this gate was written the copy called **sixteen**
//! executed items open (§2 enumerates them), omitted eight open ones filed since it
//! was last touched, and contradicted itself about item 41 inside a single sentence.
//! Item 95's validation line asks for "whatever gate would have caught the drift".
//! This is it.
//!
//! It is the same doctrine as [`meta_derive`](../meta_derive.rs): a roster that
//! exists twice is correct once. The difference is that both copies here are prose,
//! so the derivation is a *parse* rather than a symbol lookup — which puts the whole
//! weight of the gate on the parser being right about two separable things: **where**
//! an entry's status is written, and **what** it says there.
//!
//! # Where the status is: the declaration
//!
//! Plan §18's **Schema** paragraph is explicit about the shape: "An open item states,
//! *in order*: **State** (open / executed / declined; size S/M/L; …) … **Evidence** …
//! **Remainder** …". The status is the entry's *opening statement*, and everything
//! after it is the item's record. So the parser cuts, in this order:
//!
//! 1. the head's `NN. `;
//! 2. the entry's own bolded **title**, which is prose about the *defect* and is
//!    routinely written with the words this gate matches on — item 98's title is
//!    literally "The pattern-stimulus experiment — open, and it is the first thing to
//!    run when this project is next on Linux" while its status is `CLOSED AS A
//!    MEASURED DECLINE`. Most titles are themselves *sentences* (`**Prose-truth sweep
//!    (S).**`), so keeping the title would end the declaration before the status;
//! 3. the schema's **size marker** where it sits outside the title (`(S).`, `(M).`,
//!    `(S/M).`) — items 32–44 write it there;
//! 4. everything after the **first sentence end that does not have another bolded
//!    clause statement behind it**. That bound is what keeps the classifier off the
//!    *superseded filing* most executed entries still carry underneath — `The
//!    superseded filing follows.` and six sibling phrases, all enumerated in
//!    [`SUPERSEDED_MARKERS`] — which still reads `**State:** open` because it is a
//!    verbatim record and AGENTS §5 forbids rewriting one. A classifier that reads the
//!    whole entry sees "open" in a majority of the executed items, and a gate built on
//!    it is worse than no gate: it reports drift that is not there and trains its
//!    reader to ignore it. The clause exception is not a nicety: a plain
//!    first-sentence bound is a **proxy** for "the opening statement", and item 64's
//!    mixture rewritten `**… EXECUTED.** **(a) PARTLY, …**` — the open clause left
//!    byte-for-byte in its recognised spelling, one separator changed — was cut off
//!    before [`classify`] could see it and the item went EXECUTED.
//!
//! Rules 2 and 4 are each proven load-bearing against the real ledger rather than
//! assumed — [`the_title_strip_is_load_bearing_on_this_tree`] and
//! [`the_declaration_bound_is_load_bearing_on_this_tree`] re-parse this tree with the
//! mechanism switched off and require the parse to *change*, and
//! [`a_punctuation_only_rewrite_of_a_mixtures_joint_never_moves_the_derived_set_in_silence`]
//! rewrites every mixture the tree spells — and one of each joint shape constructed over a
//! real entry besides — in every punctuation the ledger's own joiner vocabulary can
//! express, and requires the derived set *not* to move in silence.
//! Without that trio, a mechanism that never fires passes for the wrong reason on a
//! reconciled tree — AGENTS §3's tell, "its passing output is identical to its
//! not-running output".
//!
//! The marker list is no longer a truncation, because the declaration bound already
//! ends the entry long before any of those phrases: measured on this tree, **no**
//! entry's declaration reaches one. It is a **tripwire on the bound instead** — a
//! declaration that does reach a marker is refused with the item named, so the day an
//! entry's opening statement runs into its own superseded filing this gate says so
//! rather than reading it. That is why the phrases are matched against
//! whitespace-normalised text: **eight** instances in plan §18 wrap across the
//! document's 100-column lines (items 18, 19, 22, 38, 46, 52, 53 and 64 all spell
//! `The superseded⏎    filing follows`), and a line-oriented matcher finds none of
//! those eight.
//!
//! # What it says: no default status, and no *inferred* mixture
//!
//! [`classify`] returns an **error** naming the item when a declaration matches none
//! of the spellings in [`SPELLINGS`]. It does not fall back to "executed" or to
//! "open". Defaulting is how this drift happened in the first place: a status nobody
//! wrote down became a status somebody assumed.
//!
//! Loudness has to survive the *mixed* entry, which is where the first two versions
//! of this gate failed. Items 31, 64 and 78 were each held open by a single
//! hand-listed literal, and rewording it left the entry classified EXECUTED with **no
//! error at all**, because a sibling clause still supplied a status. The repair for
//! that — split at a clause marker, demand a status in every lettered segment — was
//! itself keyed on the wrong thing: on the **marker** and on the separator before it.
//! Four edits to item 64's and item 78's own declarations each went silently EXECUTED
//! against the real plan: reword the open half and drop its letter; drop the `;` so
//! the letter lands in a segment its sibling already answered; drop the letter from a
//! `PARTLY` half; and — needing no rewording at all — write `**…EXECUTED.** **(a)
//! PARTLY…**` in place of `**…EXECUTED**; **(a) PARTLY…**`.
//!
//! So the declaration is read as the **statements** it is made of, and each of them
//! has to account for itself:
//!
//! * every clause group opens a statement, unconditionally, and so does every `;` that
//!   is not inside brackets (in a declaration that speaks in clauses at all). A
//!   statement with words in it and no recognised status is an error naming the item;
//! * the one shape that legitimately has no status of its own is the **governed
//!   enumeration** — item 49's `**EXECUTED**: (a) and (b) 2026-08-12 …`, where the
//!   status is stated *ahead of* any clause letter and the letters follow it. A
//!   letters-only statement inherits only from such a statement, which is what tells
//!   `(a) EXECUTED … and (b) still owed` (two statements, the second unanswered) from
//!   item 49 (one);
//! * an open statement in a declaration that speaks in clauses must **name a clause
//!   letter**, because that letter is what AGENTS §2 carries as `64(a)`;
//! * a mixture is never *inferred*. `Partly` is returned only when the entry declares
//!   it — by the word `PARTLY`, or by statements that disagree with each other. Open
//!   and executed spellings in the *same* statement are an error, because nothing in
//!   that text says which one is the item's state.
//!
//! The first thing this family of rules found was a status the table did not have:
//! item 55's `**(f) and one paragraph of (a) are carried**`. `carried` is not an
//! invention of this gate — plan §18's **Discipline** paragraph names it as one of the
//! three dispositions a clause may take ("executed, carried, or re-filed under a new
//! number"), and a carried clause leaves the item for a successor (55's went to item
//! 59), so it closes the item rather than opening it. That is the mechanism working as
//! designed: a spelling nobody had written down had to be written down.
//!
//! # The ledger's own schema, read as a backstop
//!
//! A declaration can be wrong about its entry, and one was. Item 13 read `**MEASURED
//! 2026-08-13** … **the gating half is carried**` while its live record carried
//! `*Remainder:* a Darwin per_fd_cpu_percent … row` — and §18's **Schema** paragraph
//! defines a `Remainder` as *exactly what is owed*. This gate derived EXECUTED, went
//! red, and told its reader that AGENTS §2 was the copy to repair; following that
//! instruction deletes the one genuinely-open item from the list the gate exists to
//! keep honest.
//!
//! So an entry carrying a `Remainder` in its live record is open whatever its opening
//! statement says (see [`owed_remainder`]). The rule only ever *keeps* an item on the
//! list — it can neither close one nor contradict an open declaration — and it is
//! narrowed to the record above the entry's filing, because a `Remainder` inside a
//! preserved filing states what was owed when the entry was filed. Both halves are
//! measured on the tree rather than argued, in
//! [`an_owed_remainder_keeps_an_item_open_whatever_its_declaration_says`].
//!
//! # What is *not* asserted
//!
//! This gate compares two lists of item numbers **and their clause letters**: AGENTS
//! §2 spells its mixed entries `13(b)`, `64(a)` and `78(b)`, and those letters are checked
//! against the clauses the ledger declares open, so `78(a)` where (a) is executed is
//! drift rather than agreement. It does not judge whether an item *should* be open,
//! does not read `docs/implementation-notes.md`, and does not check the plan's Status
//! table — the other hand-kept surface item 95 names. `PARTLY` and `OPEN` are both
//! "open" for the comparison: a letter clause is still an entry on the list.
//!
//! Two limits are worth stating because they are design choices and not oversights.
//! A status written *outside* the declaration is invisible: §18's Schema says the
//! State comes first, and this parser holds the document to that, so an entry that
//! buries its state in paragraph four fails as "declares no status" — the right
//! complaint to make about it. And a mixture whose halves are *both* reworded, with
//! the status moved ahead of the letters (`**EXECUTED 2026-08-16** for (a) and (b)
//! still owed`), reads as one governed enumeration and passes.
//!
//! **What that paragraph used to claim next was false, and the correction is the more
//! useful sentence.** It said the statement rules "catch every edit that leaves one
//! half in place, which is the class the record actually contains" — an absolute,
//! written while the declaration bound reached exactly one of the two shapes the
//! ledger writes a mixture in. Item 78 keeps both halves inside **one** bold run, and
//! rewriting its `;` as a `.` — one byte, the open clause left verbatim in its
//! recognised spelling — silently reclassified it `PARTLY{b}` → `EXECUTED`, whereupon
//! this gate instructed its reader to delete a genuinely-open item from AGENTS §2:
//! the same "worse than no gate" outcome described for item 13 above, arrived at from
//! the other direction. Item 64's separator had the identical hole one emphasis mark
//! over. The rule is keyed on the statement structure now (see
//! [`continues_with_a_clause_statement`]), and both shapes are exercised, on the tree
//! and in fixtures. The claim is not restated in a stronger form here, because the
//! reason it was wrong was that nobody had enumerated the shapes it quantified over.
//!
//! **That last sentence earned itself twice** (plan §18 item 100). The repair reached
//! a stop *inside* an open `**` run and a stop outside one with `**` behind it, and
//! that pair is not a partition: a stop just **past** a run's close, with `*` after it
//! or with nothing, satisfied neither, and neither did a joint the ledger writes with
//! the word `and` in it. What is worth more than the fix is **why the guard could not
//! see them**: it planted by rewriting the separator `;` where it stood, and a `;`
//! rewritten in place always leaves the stop at the `;`'s own offset, so the whole
//! family of edits that move the stop across a `**` boundary was outside what its plant
//! generator could express. Nothing about its passing output said so. A guard's coverage
//! is bounded by the edits its plant can spell, and that bound is the part no reviewer
//! reads — one more register of AGENTS §3's tell, and the closest sibling of the
//! "stimulus gentler than the product's" one.
//!
//! # Anti-vacuity that closing work does not break
//!
//! Two of this file's assertions were written as *liveness* claims against the live
//! document: every row of the status vocabulary must decide or co-occur in some entry's
//! declaration, and the mixture battery must reach both of the joint shapes the ledger
//! writes. Both are the strongest kind of guard this file has — a mechanism reached by
//! nothing cannot be shown to work, which is AGENTS §3's tell applied to the gate's own
//! vocabulary — and both went red on 2026-08-21 when items **64(a)** and **78(b)** were
//! executed. Between them those two clauses were the ledger's only `PARTLY` declaration
//! and its only mixture whose halves share one bold run. Neither assertion was broken.
//! Their **subject** had been finished.
//!
//! That is a defect in the guards and not in the ledger, and the shape of it is worth
//! more than either repair: each assertion had fused a claim about **the parser** with a
//! claim about **which items happen to be open**, and only the first is the gate's to
//! make. Fused, the guard reports a defect when work is *completed* — and worse, the
//! instruction it prints is wrong in a way a reader will follow. "Delete the row, since a
//! row that matches nowhere cannot be shown to work" deletes a disposition the ledger has
//! used three times and [`classify`] still implements; "must reach both shapes" names
//! nothing a reader can do short of reopening an item. **A guard whose green depends on
//! work being unfinished pays for its teeth with a false instruction, and the instruction
//! is the half people act on.**
//!
//! Both are now split the same way, and the split is the doctrine rather than the patch:
//!
//! * the **parser** claim is asserted for every row and every shape, against the real
//!   document, by construction rather than by luck — each vocabulary row is planted into
//!   a real entry and required to move the whole gate ((a), (a′), (a″) and the coverage
//!   assertion (d) in
//!   [`planted_drift_reddens_in_every_spelling_and_a_superseded_filing_does_not`]), and a
//!   subject of each joint shape is *built* over a real open entry on every run;
//! * the **document** claim is reported, not failed — which item still spells a quiet
//!   status word and in which region of its entry; how many mixtures the ledger writes
//!   itself and which shapes they are.
//!
//! A report is a no-op unless something holds it up, so each keeps one assertion that
//! cannot go empty when work closes. A quiet spelling must be **founded** — written
//! somewhere in §18 under its own [`Match`] kind — which kills a speculative row while
//! surviving every execution, because executing an item does not unwrite its record. And
//! a constructed subject must answer **arm for arm** as the live mixture it shares a
//! joint token with, which is the only honest licence for a fixture to stand in for a
//! document: a stimulus gentler than the product's is precisely what a hand-written
//! stand-in becomes, and the comparison is the measurement that says it has not.
//!
//! What was **rejected**, recorded because each is the obvious move: deleting the quiet
//! row (it drops a parser branch and a disposition the ledger may write again, and the
//! failure message itself offered it); scoping the live requirement to "spellings the
//! ledger currently uses" (computed, not hand-listed — and circular, since that set is
//! defined by the thing being checked); and building the missing mixture shape from a
//! [`synthetic_ledger`] entry (a hand-written entry drops the title that is a sentence,
//! the size marker outside it, the 100-column wraps and the superseded filing underneath
//! — everything that makes a real declaration hard to parse).
//!
//! The battery that replaces it reads each mixture's joint from the document and
//! rewrites the whole of it, and it is the reason the family's size is a measurement
//! here rather than the count the item was filed with. **The item named five silent
//! rewrites; the battery scored sixteen and ten of them went silent** against the rule
//! as it stood — the four it named, and *all six* of item 13's, because the `and`
//! joiner defeats the continuation test at both stop placements and in all three
//! emphases rather than in the one spelling the item happened to write down. A guard
//! built to the filed count would have covered half of it and passed. **Re-measured
//! 2026-08-21, once the subjects stopped being whichever mixtures happened to be open:
//! 26 rewrites, of which 20 go silent** under [`Continuation::BeforeItem100`] — the same
//! plant against a larger family, because the constructed subjects put the shared-run
//! shape back after 64(a) and 78(b) closed and carry both joiner spellings with it.
//!
//! Both figures are taken by planting that variant into [`parse_ledger`] for one run, and
//! the sentence that used to close this paragraph said something stronger and wrong: that
//! keeping the variant makes "that proof run on every invocation". What runs on every
//! invocation is a *different* claim —
//! [`the_costs_that_keep_the_continuation_rule_narrow_are_re_measured_here`] asserts that
//! the repair moved **no verdict** on the unplanted tree, which is the absence of a side
//! effect and not the presence of the defect. The silent-rewrite count is still a session
//! measurement, and saying so is cheaper than a reader believing the suite re-takes it.
//!
//! **The third instance of the class was in this file's own parser** (plan §18 item 115),
//! and it is the clearest of the three because the constant was, literally, a count of how
//! much work happens to be open: [`parse_agents_open_list`] refused any §2 list carrying
//! fewer than **ten** numbers. Executing items 73 and 85 took the derived set from eleven
//! to nine, and the suite read `24 passed · 3 failed` — the gate itself, plus **both**
//! plant batteries, which build a reconciled AGENTS copy from the derived list and assert
//! it green as their control. They panicked before planting anything. Not one of the three
//! failures had found a defect; between them they had found the completion of two items.
//! The session before this one restored the count to ten by filing a real ledger entry and
//! wrote the honest sentence in it: *that is an arithmetic coincidence, not a repair, and
//! the next two items this ledger closes re-trip it.*
//!
//! The split is the same one, one level down, and the hazard the count was aimed at is
//! kept rather than deleted: if the lead literal matches a different sentence, or the
//! terminator search stops inside the list, the parser reads a number set that is not §2's
//! list, and a derived set that happened to match it would pass for the wrong reason. What
//! replaced the count is three structural claims, none of which counts anything — the lead
//! is spelled once, the segment reads as a list under a five-word connective vocabulary,
//! and the list does not carry on past the terminator ([`list_shape_defects`] and
//! [`list_continues_past_terminator`] carry the reasoning and the rejected alternatives).
//! The parser half is then proved by construction at one, three and nine open items, end to
//! end against the real AGENTS.md
//! ([`the_agents_list_is_read_at_any_size_the_ledger_can_reach`]), and the document half —
//! how many numbers §2 carries today — is printed.
//!
//! **The rule worth extracting is the one all three share, and this instance states it
//! plainly: a gate that hard-codes how much work is open is asserting a property of the
//! project's schedule, not of its code — and completing work is what exposes it.** The
//! tell is that its failure arrives on a green change, and the instruction it prints
//! cannot be followed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// The ledger's shape
// ---------------------------------------------------------------------------

/// The heading that opens plan §18, and the one that closes its item list.
///
/// The closing register after `### Evaluated and deliberately not scheduled` is
/// prose about *declined* work and carries no item entries; parsing past it would
/// invent items.
const LEDGER_START: &str = "## 18. The work ledger";
const LEDGER_END: &str = "### Evaluated and deliberately not scheduled";

/// A floor on the ledger's size that is **never bumped**.
///
/// The real floor is derived per parse, from the highest head number the document
/// spells with a matcher looser than the walker's (see [`loose_head_number`]): a walker
/// that stops in the tail then fails even though what it did read is contiguous from 1.
/// The derivation is only ever as good as that matcher, which is why the matcher — not
/// the idea — is what carries the test cases. The constant that used to sit
/// here was hand-set to the count of the day, which structurally could not see a lost
/// tail — once the ledger passed it, a short parse satisfied both the floor and a
/// contiguity check derived from its own length. This number exists only so that a
/// gutted section fails with a sentence about the section rather than about item 1.
const SANITY_FLOOR: u32 = 50;

/// Phrases that introduce an item's superseded or original filing.
///
/// Derived by reading every entry rather than from memory. All seven are in use on
/// the tree that landed this gate; `The original filing follows` is the newest and
/// was absent from the list this gate was briefed with, which is the reason the
/// derivation is recorded here instead of trusted.
///
/// Matching is on whitespace-normalised text because these phrases wrap: **eight**
/// instances of `The superseded filing follows` are split across two lines in plan
/// §18 (items 18, 19, 22, 38, 46, 52, 53 and 64), and a line-by-line matcher misses
/// every one of them. That count is measured by
/// [`every_superseded_marker_that_the_ledger_uses_is_matched_where_it_stands`], not
/// remembered — the sentence here used to say "six".
const SUPERSEDED_MARKERS: &[&str] = &[
    "The superseded filing follows",
    "The superseded line follows",
    "The original filing follows",
    "its record follows unchanged",
    "**Original state:**",
    "*Original state:*",
    "*Original:*",
];

/// The tag that marks a refusal as [`refuse_superseded`]'s own.
///
/// It exists because the tripwire's test could not tell the tripwire from a bystander:
/// the plant it used made the *classifier* fail too, and the classifier's error quotes
/// the planted declaration back — so `err.contains("superseded filing")` was satisfied
/// by the plant's own bytes with the tripwire switched off. Replacing the marker lookup
/// with `return Ok(())` left all fourteen tests green. This tag appears in one function
/// and in no document, so an assertion on it can be satisfied by nothing else.
const SUPERSEDED_TRIPWIRE_TAG: &str = "[superseded-declaration tripwire]";

/// The schema labels that open an entry's **filing**.
///
/// §18's Schema states the parts of a filing *in order*: `State … Evidence … Remainder
/// … Validation`. Everything from the first of these onward is that filing — the
/// entry's own if it is still open, or the one an executed entry preserves underneath
/// its record. What sits *above* them is the executing session's live prose.
///
/// This matters for [`owed_remainder`] and nowhere else. Items 49 and 50 are executed
/// and preserve their original filings with **no marker phrase at all** — the filing
/// simply begins at `*Evidence:*` — so their `Remainder` lines are records of what was
/// owed when they were filed, not claims about today. Items 4, 8, 15 and 78 have the
/// same shape and are open, which is the proof that the *shape* decides nothing: what
/// separates them is the declaration, and the Remainder rule is a backstop under it
/// rather than a second opinion beside it.
const FILING_LABELS: &[&str] = &["**State:**", "*State:*", "**Evidence", "*Evidence"];

/// How §18 spells the schema's `Remainder`.
///
/// Deliberately the two label spellings and not the bare word: item 13's declaration
/// says "per this entry's own live *Remainder* below", and a matcher that fired on
/// that would read a pointer as the thing pointed at.
const REMAINDER_LABELS: &[&str] = &["**Remainder:**", "*Remainder:*"];

// ---------------------------------------------------------------------------
// The status vocabulary
// ---------------------------------------------------------------------------

/// How a spelling is matched against a declaration.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Match {
    /// Plain substring. Used where the spelling carries its own delimiters
    /// (`**open**`, `**executed`), so a word boundary would add nothing.
    Sub,
    /// Substring with non-alphanumeric neighbours on both sides. Used for the bare
    /// words, so `EXECUTED` does not fire inside `UNEXECUTED` and `Executed` does not
    /// fire inside a longer identifier. The distinction is proven to change a verdict
    /// by [`word_matching_is_load_bearing_for_every_spelling_that_claims_it`], which
    /// re-classifies the same fixtures with the whole table downgraded to `Sub`.
    Word,
}

/// Spellings that say an item, or one clause of it, is open.
///
/// `) open**` is item 78's `**(a) EXECUTED 2026-08-16; (b) open**`, where the bold
/// run covers both clauses and neither is delimited on its own.
///
/// `**Open**` is **not** in this table, and its absence is measured rather than
/// assumed: the two instances in plan §18 both sit inside superseded filings, past
/// the declaration, so the spelling decided nothing and matched nothing. A table row
/// that matches nowhere is a claim nobody checks — see the liveness block in
/// [`planted_drift_reddens_in_every_spelling_and_a_superseded_filing_does_not`], which
/// **reports** such a row rather than refusing it and says which item still spells it and
/// where, that judgement having turned out to be one a gate cannot make: a row can be
/// quiet because the ledger closed the work that used it, and this one was retired for a
/// reason no gate reached — *where* its two instances sat.
const OPEN_SPELLINGS: &[(&str, Match)] = &[
    ("**State:** open", Match::Sub),
    ("**open**", Match::Sub),
    (") open**", Match::Sub),
];

/// Spellings that say an item, or one clause of it, is executed.
///
/// `**MEASURED` is bold-anchored deliberately: item 13's status is
/// `— **MEASURED 2026-08-13**`, but the bare word also appears inside item 98's
/// `**CLOSED AS A MEASURED DECLINE 2026-08-21**`, where reading it as "executed"
/// would swallow a decline. `**executed` is bold-anchored for the same class of
/// reason — unbolded lowercase "executed" is ordinary prose in these entries.
const EXECUTED_SPELLINGS: &[(&str, Match)] = &[
    ("EXECUTED", Match::Word),
    ("Executed", Match::Word),
    ("**executed", Match::Sub),
    ("**MEASURED", Match::Sub),
];

/// The spelling that says an item is part executed and part open.
///
/// **It decides nothing on this tree and is kept deliberately.** Items 31, 64 and 78 each
/// wrote it; executing 64(a) and 78(b) on 2026-08-21 closed the last of them, and the
/// word now survives only in item 64's preserved `*Original head declaration:*`. Deleting
/// the row is what the liveness assertion of the day instructed and it is the wrong move
/// twice over: [`classify`] implements the branch, so the row's deletion leaves a parser
/// path with no test, and §18's own **Discipline** rule — every clause of every item
/// dispositioned explicitly at every rewrite — is the rule that produces a part-executed
/// item, so the ledger will write it again the next time a clause lands half-done. The
/// row is held to the two things a quiet row can still be held to: it is planted into a
/// real entry and shown to move the gate, and it is shown to be *founded* — written
/// somewhere in §18 under its own `Match` kind.
const PARTLY_SPELLINGS: &[(&str, Match)] = &[("PARTLY", Match::Word)];

/// Spellings that close an item, or one clause of it, by declining it.
///
/// Both are needed: `CLOSED AS A DECLINE` is not a substring of
/// `CLOSED AS A MEASURED DECLINE`.
const DECLINED_SPELLINGS: &[(&str, Match)] = &[
    ("CLOSED AS A MEASURED DECLINE", Match::Sub),
    ("CLOSED AS A DECLINE", Match::Sub),
];

/// The spelling that says a clause left this item for a successor.
///
/// Plan §18's **Discipline** paragraph names the three dispositions a clause may take
/// — "executed, carried, or re-filed under a new number" — and item 55's declaration
/// uses the middle one: `**(f) and one paragraph of (a) are carried**`, with the
/// residue filed as item 59. It closes the item, because the open work is on the
/// list under the successor's number and counting it twice would put a number on
/// AGENTS §2's list that plan §18 records as executed.
///
/// This row exists because the clause-completeness rule demanded it: item 55's second
/// clause segment declared a status this table did not have, and the parse refused
/// rather than silently reading the item as executed on its first clause alone.
///
/// It is deliberately narrow, and the reason is that the ledger uses the word both
/// ways. Item 13's declaration also says `**the gating half is carried**`, and there
/// it means a half still *owed* on this item rather than moved off it — the opposite
/// disposition, one clause word apart. So `are carried` is matched and `is carried`
/// is not, and an entry that writes the second one in a clause fails as an
/// unrecognised status. That is the right complaint to make about it: which of the
/// two a `carried` clause means is a decision for the ledger to state, not for this
/// parser to guess, and guessing is what the gate exists to stop.
const CARRIED_SPELLINGS: &[(&str, Match)] = &[("are carried", Match::Sub)];

/// The four-table vocabulary, passed as a parameter so a test can prove a row of it
/// changes an answer.
///
/// The `markers`-as-a-parameter pattern below has the same justification: a mechanism
/// that cannot be switched off cannot be shown to do anything.
#[derive(Clone, Copy)]
struct Spellings<'a> {
    open: &'a [(&'static str, Match)],
    executed: &'a [(&'static str, Match)],
    partly: &'a [(&'static str, Match)],
    declined: &'a [(&'static str, Match)],
    carried: &'a [(&'static str, Match)],
}

impl<'a> Spellings<'a> {
    fn tables(self) -> [(Status, &'a [(&'static str, Match)]); 5] {
        [
            (Status::Open, self.open),
            (Status::Executed, self.executed),
            (Status::Partly, self.partly),
            (Status::Declined, self.declined),
            (Status::Carried, self.carried),
        ]
    }

    fn every_spelling(self) -> Vec<&'static str> {
        self.tables()
            .iter()
            .flat_map(|(_, table)| table.iter().map(|&(s, _)| s))
            .collect()
    }
}

/// The vocabulary the gate ships with.
const SPELLINGS: Spellings<'static> = Spellings {
    open: OPEN_SPELLINGS,
    executed: EXECUTED_SPELLINGS,
    partly: PARTLY_SPELLINGS,
    declined: DECLINED_SPELLINGS,
    carried: CARRIED_SPELLINGS,
};

/// The state of one ledger item, as the entry itself declares it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Status {
    Executed,
    Open,
    /// Some clauses executed, some open — the shape AGENTS §2 spells `64(a)`.
    Partly,
    Declined,
    /// A clause disposition, not an item state on its own: the work moved to a
    /// successor item and is on the list under that number.
    Carried,
}

impl Status {
    /// Whether an entry in this state belongs on AGENTS §2's `Still open:` list.
    fn is_open(self) -> bool {
        matches!(self, Status::Open | Status::Partly)
    }

    fn label(self) -> &'static str {
        match self {
            Status::Executed => "EXECUTED",
            Status::Open => "OPEN",
            Status::Partly => "PARTLY",
            Status::Declined => "DECLINED",
            Status::Carried => "CARRIED",
        }
    }
}

/// One parsed ledger entry.
#[derive(Clone, Debug)]
struct Item {
    number: u32,
    /// 1-based line of the entry's head, so a failure message points at the file.
    line: usize,
    status: Status,
    /// The recognised spelling that decided the status.
    spelling: &'static str,
    /// Verbatim text from that spelling onward, so the reader can see the claim
    /// rather than a paraphrase of it.
    quote: String,
    /// The clause letters this entry declares **open**, if it declares clauses at
    /// all — item 78's `(b)`, item 64's `(a)`. Empty for an entry that states one
    /// status for the whole item.
    open_letters: BTreeSet<char>,
    /// Every spelling that matched anywhere in the declaration, deciding or not.
    /// The liveness assertion reads this; a table row that appears in no item's list
    /// is a row the ledger no longer uses.
    matched: BTreeSet<&'static str>,
    /// The schema `Remainder` label this entry carries **above its filing**, if any —
    /// see [`owed_remainder`]. Present whether or not it changed the verdict, so the
    /// gate can print which entries the backstop is standing under.
    remainder: Option<&'static str>,
}

// ---------------------------------------------------------------------------
// The three regions of an entry
// ---------------------------------------------------------------------------

/// The entry with any verbatim record of an earlier filing cut off.
///
/// AGENTS §5 forbids rewriting a superseded filing, so most executed entries still
/// carry `**State:** open` somewhere underneath. Everything this gate reads as a claim
/// about *today* is read from here.
fn live_region_of<'a>(entry: &'a str, markers: &[&str]) -> &'a str {
    let cut = markers
        .iter()
        .filter_map(|m| entry.find(*m))
        .min()
        .unwrap_or(entry.len());
    &entry[..cut]
}

/// The live prose an executing session wrote *above* the entry's filing.
///
/// Cut at the first of [`FILING_LABELS`], because §18's Schema puts `State` and
/// `Evidence` at the head of a filing and the `Remainder` inside it.
fn record_region_of(live: &str) -> &str {
    let cut = FILING_LABELS
        .iter()
        .filter_map(|l| live.find(*l))
        .min()
        .unwrap_or(live.len());
    &live[..cut]
}

/// A `Remainder` this entry owes **today**, as `(offset, label)`.
///
/// §18's **Schema** paragraph defines a `Remainder` as *exactly what is owed*. An entry
/// that carries one in its live record therefore has open work, whatever its opening
/// statement says — and that is not a hypothetical: item 13's declaration read
/// `**MEASURED 2026-08-13** … **the gating half is carried**` while its live record
/// carried `*Remainder:* a Darwin per_fd_cpu_percent … row`, so this gate derived
/// EXECUTED and reported the one genuinely-open item as drift *in AGENTS §2*. A reader
/// following the report's own instruction would have deleted it from the list.
///
/// The rule is narrowed to the **record** rather than the whole live region, and the
/// narrowing is measured rather than assumed. Eight entries carry a `Remainder` in
/// their live region — 4, 8, 13, 15, 28, 49, 50 and 78 — and two of those (49, 50) are
/// executed with the work done: their `Remainder` sits inside a preserved *filing* that
/// no marker phrase introduces, so it says what was owed at filing time. Cutting at the
/// filing labels leaves exactly 13 and 28, both of which the ledger's own declarations
/// already call open, so on this tree the backstop **agrees with every declaration and
/// moves no classification**. That it fires at all is proven by planting a closed
/// declaration over each of them — see
/// [`an_owed_remainder_keeps_an_item_open_whatever_its_declaration_says`].
fn owed_remainder(record: &str, labels: &[&'static str]) -> Option<(usize, &'static str)> {
    labels
        .iter()
        .filter_map(|l| record.find(*l).map(|at| (at, *l)))
        .min()
}

// ---------------------------------------------------------------------------
// Reading the two documents
// ---------------------------------------------------------------------------

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is <root>/itest — the *directory*, which §15.40 kept short.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the itest crate has a parent directory")
        .to_path_buf()
}

fn read_tree_file(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| {
        panic!(
            "read {}: {e} — a gate cannot derive a roster it cannot read",
            path.display()
        )
    })
}

/// The normative plan, found by shape rather than by name.
///
/// [`meta_derive`](../meta_derive.rs) names its documents as literal paths, with the
/// stated reasoning that a generation bump must update them in the same commit —
/// AGENTS §2 counts those literals as one of the code sites a landing has to touch.
/// This gate deliberately does not add another one: a generation bump moves the
/// superseded pair into `docs/historical/`, so `docs/` holds exactly one file whose
/// name contains `implementation-plan`, and finding it costs nothing. Two matches or
/// none is a failure with the candidates named, never a silent pick.
fn normative_plan_path(root: &Path) -> PathBuf {
    let docs = root.join("docs");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&docs)
        .unwrap_or_else(|e| panic!("read {}: {e}", docs.display()))
        .map(|e| e.expect("a readable directory entry").path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            name.contains("implementation-plan") && name.ends_with(".md")
        })
        .collect();
    found.sort();
    assert_eq!(
        found.len(),
        1,
        "expected exactly one `*implementation-plan*.md` directly under {} — the \
         superseded pair moves to docs/historical/ at a generation bump (AGENTS §2). \
         Found: {found:?}",
        docs.display()
    );
    found.remove(0)
}

/// Collapse every run of whitespace to one space.
///
/// Every phrase this file matches can wrap across the plan's 100-column lines, and
/// several of them do — the title of nearly every entry, and eight instances of `The
/// superseded filing follows`. Matching on the joined, normalised form is the only
/// way a literal finds all of its instances.
fn normalise(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// Matching
// ---------------------------------------------------------------------------

/// Does `haystack` contain `needle` under `kind`, and where?
fn find_spelling(haystack: &str, needle: &str, kind: Match) -> Option<usize> {
    let bytes = haystack.as_bytes();
    let mut from = 0usize;
    while let Some(offset) = haystack[from..].find(needle) {
        let at = from + offset;
        let ok = match kind {
            Match::Sub => true,
            Match::Word => {
                let before_ok = at == 0 || !is_word_byte(bytes[at - 1]);
                let end = at + needle.len();
                let after_ok = end >= bytes.len() || !is_word_byte(bytes[end]);
                before_ok && after_ok
            }
        };
        if ok {
            return Some(at);
        }
        from = at + 1;
    }
    None
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Every spelling in `table` that matches `region`, as `(offset, spelling)`, earliest
/// first.
fn spelling_hits(region: &str, table: &[(&'static str, Match)]) -> Vec<(usize, &'static str)> {
    let mut hits: Vec<(usize, &'static str)> = table
        .iter()
        .filter_map(|&(s, k)| find_spelling(region, s, k).map(|at| (at, s)))
        .collect();
    hits.sort_unstable();
    hits
}

/// A short verbatim excerpt starting at `at`, cut on a character boundary.
fn quote_from(region: &str, at: usize) -> String {
    region[at..].chars().take(72).collect()
}

// ---------------------------------------------------------------------------
// Finding the declaration
// ---------------------------------------------------------------------------

/// Whether the entry's own bolded title is cut before the status is read.
///
/// A parameter for one reason: [`the_title_strip_is_load_bearing_on_this_tree`]
/// re-parses the real ledger with it off and requires the parse to fail. Without that,
/// the strip is a mechanism nobody has ever seen do anything.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Title {
    Stripped,
    Kept,
}

/// Whether the declaration is cut at the entry's first sentence end.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Bound {
    FirstSentence,
    WholeEntry,
}

/// Which rule decides that a sentence end *inside* a declaration is a statement
/// separator rather than the declaration's end.
///
/// A parameter for the same reason [`Title`] and [`Bound`] are, and for one more. The
/// two widenings this rule deliberately does not take are refused on a **measured**
/// cost, and a cost quoted in a comment is a cost nobody re-checks as the document
/// grows: [`the_costs_that_keep_the_continuation_rule_narrow_are_re_measured_here`]
/// re-derives both of them from the tree on every run. And the rule as it stood before
/// plan §18 item 100 is kept beside the shipped one so that the repair can be shown, on
/// every invocation, to have moved **no verdict** on the unplanted document — a second
/// change riding along inside the first is what that assertion is for. It is deliberately
/// *not* the same thing as showing item 100's family redden against the old rule: that
/// takes planting this variant into [`parse_ledger`], the figure lives in the module
/// documentation with the date it was taken, and this comment used to run the two
/// together as though the suite re-took it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Continuation {
    /// What this gate ships.
    Shipped,
    /// The rule as it stood before plan §18 item 100: the stop had to end **inside** an
    /// open `**` run, or the statement after it had to open one. Blind to the mirror
    /// shape — a stop just *past* a run's close, with `*` or nothing after it — and
    /// blind to a statement the ledger joins with the word `and`.
    BeforeItem100,
    /// Measured and refused: a leading single `*` on the continuation licenses it.
    ASingleAsteriskLicensesIt,
    /// Measured and refused: emphasis decides nothing, so any clause marker continues.
    AnyClauseMarker,
}

/// Strip the entry's leading `NN. `, its own bolded title, and the schema's size
/// marker where that sits outside the title.
fn status_region_of(entry: &str, title: Title) -> &str {
    let after_number = match entry.find(". ") {
        Some(at) => &entry[at + 2..],
        None => entry,
    };
    let after_title = match title {
        Title::Kept => after_number,
        Title::Stripped => match after_number.strip_prefix("**") {
            Some(rest) => match rest.find("**") {
                Some(at) => &rest[at + 2..],
                None => after_number,
            },
            None => after_number,
        },
    };
    strip_size_marker(after_title)
}

/// Drop a leading `(S).` / `(M)` / `(S/M).` — §18's Schema puts the size on the State
/// line, and items 32–44 write it between the title and the status.
fn strip_size_marker(region: &str) -> &str {
    let trimmed = region.trim_start();
    let Some(rest) = trimmed.strip_prefix('(') else {
        return region;
    };
    let Some(close) = rest.find(')') else {
        return region;
    };
    let inside = &rest[..close];
    if inside.is_empty() || !inside.chars().all(|c| matches!(c, 'S' | 'M' | 'L' | '/')) {
        return region;
    }
    let after = &rest[close + 1..];
    after.strip_prefix('.').unwrap_or(after)
}

/// The entry's opening statement: up to the first sentence end that no further clause
/// statement of the same declaration continues past.
///
/// The plain first-sentence bound was a **proxy** for "the opening statement", and the
/// difference is one keystroke wide. Item 64 spells its mixture
/// `**… EXECUTED**; **(a) PARTLY, …**`; written `**… EXECUTED.** **(a) PARTLY, …**` —
/// the open clause left byte-for-byte in its recognised spelling, only the separator
/// changed — the bound cut before `classify` ever saw the clause and the item went
/// EXECUTED. Punctuation alone must not move a verdict in silence, in **any** of the
/// shapes the ledger writes a mixture in, which is what
/// [`a_punctuation_only_rewrite_of_a_mixtures_joint_never_moves_the_derived_set_in_silence`]
/// asserts over every mixed entry rather than over one spelling of one of them.
///
/// The continuation test is keyed on the declaration's **statement structure** — see
/// [`continues_with_a_clause_statement`] — and not on the emphasis of what follows,
/// which is what confined the first repair to item 64.
fn declaration_of(region: &str, bound: Bound, rule: Continuation) -> &str {
    if bound == Bound::WholeEntry {
        return region;
    }
    let mut from = 0usize;
    loop {
        let Some((at, after)) = first_sentence_end(region, from) else {
            return region;
        };
        if continues_with_a_clause_statement(region, at, after, rule) {
            from = after;
            continue;
        }
        return &region[..after];
    }
}

/// Whether the clause statement after a sentence end belongs to the same declaration.
///
/// The first repair keyed this on the **emphasis** of what follows: bold, then a clause
/// marker. That reads exactly one of the ledger's two mixture shapes. Item 78 writes
/// both halves inside **one** bold run — `**(a) EXECUTED 2026-08-16; (b) open**` — so
/// rewriting its `;` as a `.` leaves a sentence end with no `**` behind it anywhere,
/// and the open clause was cut off: one byte, `PARTLY{b}` → `EXECUTED`, no error, and a
/// gate that then tells its reader to delete a genuinely-open item from AGENTS §2. Item
/// 64's own separator has the same hole one emphasis mark over (`.** *(a)`, `.** (a)`),
/// and the italic spelling is not contrived — its next paragraph is literally
/// `*(a) The remedy is refuted, not deferred.*`.
///
/// So the rule reads the structure the statement is written in: a clause statement
/// continues the declaration when the sentence it follows ended **inside a bold run**,
/// i.e. the two halves are one bolded statement the ledger split with a `;` or a full
/// stop — whatever emphasis (none, `*`, `**`) the continuation carries. The bolded arm
/// is kept beside it as an independent route rather than replaced, because it is the
/// one that carries a mixture whose *first* half is unbolded (`— EXECUTED. **(a)
/// PARTLY**`), where no bold run is open at the sentence end at all.
///
/// **Those two arms had an unguarded mirror, and it is the third arm here** (plan §18
/// item 100). They read a stop *inside* an open run and a stop outside one with `**`
/// behind it — and nothing else. A stop **just past a run's close**, with `*` or with
/// no emphasis at all after it, satisfied neither, so four punctuation-only rewrites of
/// the ledger's own two mixtures — item 64's `EXECUTED**; **(a)` written
/// `EXECUTED**. *(a)` or `EXECUTED**. (a)`, item 78's `2026-08-16; (b) open**` written
/// `2026-08-16**. (b) open` or `2026-08-16**. *(b) open*`, every one of them identical
/// under `letters_only` — cut the declaration before the open clause and moved the item
/// to EXECUTED with no error. Measured against the whole family rather than against
/// those four, the count is **ten of sixteen**: item 13's joint carries the word `and`,
/// which defeats the test at both stop placements and in all three emphases. The guard that was meant to cover this **structurally
/// could not reach it**: it rewrote the separator `;` in place, which always leaves the
/// stop exactly where the `;` was, so no plant it could build ever moved the stop
/// across a `**` boundary. Coverage keyed on a plant's *mechanism* is coverage of that
/// mechanism.
///
/// The mirror arm is the local statement of the same fact the parity arm states
/// globally: a `**` immediately behind the stop is the ledger closing one bolded
/// statement at the sentence boundary, exactly as an odd delimiter count is the ledger
/// splitting one open run. And the joiner is the fifth shape, one separator over: item
/// 13 writes `**MEASURED …**, and **(b) open**`, whose comma rewritten as a full stop
/// leaves `and` between the stop and the clause. So a single `and` may sit in the
/// joint, which is [`clause_groups`]' own joiner vocabulary (`between.is_empty() ||
/// between == "and"`) rather than a new one invented here. On the tree that stop is
/// covered twice over — by the mirror arm and by the `**` the continuation opens with —
/// and the item is held open by its live `Remainder` besides; without that Remainder
/// the same shape was silent, which is why the joiner is here rather than filed.
///
/// Emphasis on its own still decides nothing, and narrowness is still the reason: three
/// real entries (55, 64 and 65) open their *next paragraph* with a clause marker —
/// `(a) \`daemon.rs\`'s verb enumeration…`, `*(a) The remedy is refuted…*`, `*(c) the
/// \`deaf.py\` orphan is fixed…*` — and every one of those sentence ends sits outside
/// every bold run **and has a word or a `)` immediately behind it**, which is what keeps
/// whole paragraphs of record out of the declaration. Widening by emphasis instead is
/// refused on a cost that is re-measured from this document on every run rather than
/// quoted here — see
/// [`the_costs_that_keep_the_continuation_rule_narrow_are_re_measured_here`], which also
/// prints it. That is why the widening goes through the bold delimiters the sentence
/// ends *at*, rather than through the emphasis of what follows it.
fn continues_with_a_clause_statement(
    region: &str,
    at: usize,
    after: usize,
    rule: Continuation,
) -> bool {
    let rest = region[after..].trim_start();
    match rule {
        Continuation::AnyClauseMarker => opens_a_clause(rest),
        Continuation::ASingleAsteriskLicensesIt => {
            opens_a_clause(rest) && (ends_inside_a_bold_run(region, at) || rest.starts_with('*'))
        }
        Continuation::BeforeItem100 => {
            opens_a_clause(rest) && (ends_inside_a_bold_run(region, at) || rest.starts_with("**"))
        }
        Continuation::Shipped => match clause_statement_after(rest) {
            None => false,
            Some(statement) => {
                ends_inside_a_bold_run(region, at)
                    || ends_just_past_a_bold_delimiter(region, at)
                    || statement.starts_with("**")
            }
        },
    }
}

/// Whether `text` opens with an `(a)`-shaped clause marker under any emphasis.
fn opens_a_clause(text: &str) -> bool {
    let b = text.trim_start_matches('*').as_bytes();
    b.len() >= 3 && b[0] == b'(' && b[1].is_ascii_lowercase() && b[2] == b')'
}

/// The clause statement that follows a sentence end, sliced from its own emphasis
/// onward, or `None` when what follows is not one.
///
/// The one word allowed to sit in the joint is `and`, and it is not a general
/// tolerance: it is the vocabulary [`clause_groups`] already uses to decide that two
/// clause markers belong to one enumeration, applied to the joint between two clause
/// *statements*. Item 13 is the entry that writes it (`**MEASURED …**, and **(b)
/// open**`). Returning the slice from the statement's own emphasis — rather than from
/// the sentence end — is what lets the `**` arm above see the continuation's emphasis
/// through the joiner.
fn clause_statement_after(rest: &str) -> Option<&str> {
    let mut statement = rest;
    if let Some(tail) = statement.trim_start_matches('*').strip_prefix("and ") {
        statement = tail.trim_start();
    }
    opens_a_clause(statement).then_some(statement)
}

/// Whether the sentence stop at `at` sits immediately past a `**` delimiter.
///
/// The mirror of [`ends_inside_a_bold_run`], and deliberately **local** where that one
/// is global: it reads the two bytes behind the stop rather than a parity over
/// everything before it. That matters for the plants item 100 named — several of them
/// leave the entry's emphasis unbalanced, and a rule that answered by parity alone would
/// answer differently for a reason the edit never touched.
fn ends_just_past_a_bold_delimiter(region: &str, at: usize) -> bool {
    region.as_bytes()[..at].ends_with(b"**")
}

/// Whether byte `at` lies inside a `**` run that has not been closed yet.
///
/// §18 writes a declaration's statements in bold, so an odd number of `**` delimiters
/// behind a sentence end means that sentence ended *inside* one statement's emphasis —
/// the ledger splitting one bolded statement rather than closing it.
fn ends_inside_a_bold_run(region: &str, at: usize) -> bool {
    let b = region.as_bytes();
    let mut delimiters = 0usize;
    let mut i = 0usize;
    while i + 1 < at {
        if b[i] == b'*' && b[i + 1] == b'*' {
            delimiters += 1;
            i += 2;
        } else {
            i += 1;
        }
    }
    delimiters % 2 == 1
}

/// The first sentence end at or after `from`, as `(offset of the full stop, offset just
/// past the emphasis or brackets it closes inside)`.
///
/// `from` is what lets [`declaration_of`] step over a sentence end that a clause
/// statement continues past.
///
/// A sentence ends at a `.` that is followed by a space or by the end of the text,
/// allowing the closing emphasis or bracket a sentence may end inside
/// (`**EXECUTED 2026-08-12.**`). A `.` between two digits is a section or version
/// number (`§3.75`), never a sentence end.
fn first_sentence_end(text: &str, from: usize) -> Option<(usize, usize)> {
    let b = text.as_bytes();
    for i in from..b.len() {
        if b[i] != b'.' {
            continue;
        }
        let digit_before = i > 0 && b[i - 1].is_ascii_digit();
        let digit_after = i + 1 < b.len() && b[i + 1].is_ascii_digit();
        if digit_before && digit_after {
            continue;
        }
        let mut j = i + 1;
        while j < b.len() && matches!(b[j], b'*' | b'"' | b')' | b']') {
            j += 1;
        }
        if j >= b.len() || b[j] == b' ' {
            return Some((i, j));
        }
    }
    None
}

/// A declaration that runs into the entry's own superseded filing is refused.
///
/// The declaration bound already ends the entry well before any of these phrases on
/// this tree; this is the tripwire that says so when it stops being true, rather than
/// letting the classifier read a verbatim record of what an item *used* to say as a
/// claim about today.
fn refuse_superseded(number: u32, declaration: &str, markers: &[&str]) -> Result<(), String> {
    let Some(marker) = markers.iter().find(|m| declaration.contains(**m)) else {
        return Ok(());
    };
    Err(format!(
        "{SUPERSEDED_TRIPWIRE_TAG} plan §18 item {number}'s status declaration runs into its \
         superseded filing: it contains {marker:?}, which introduces a verbatim record of what \
         the item used to say. §18's Schema puts the State first; a declaration that reaches \
         this phrase cannot be told from the record underneath it, so this gate refuses to \
         classify the entry rather than read one as the other.\nDeclaration: {}",
        declaration.chars().take(200).collect::<String>()
    ))
}

// ---------------------------------------------------------------------------
// Clause structure
// ---------------------------------------------------------------------------

/// A maximal run of `(a)`-shaped clause markers separated by nothing but commas,
/// emphasis and the word `and` — item 64's `(b), (c), (d), (e), (f), (g) and (h)`.
#[derive(Clone, Debug)]
struct ClauseGroup {
    start: usize,
    letters: Vec<char>,
}

fn clause_groups(declaration: &str) -> Vec<ClauseGroup> {
    let b = declaration.as_bytes();
    let mut markers: Vec<(usize, usize, char)> = Vec::new();
    let mut i = 0usize;
    while i + 2 < b.len() {
        if b[i] == b'(' && b[i + 1].is_ascii_lowercase() && b[i + 2] == b')' {
            markers.push((i, i + 3, char::from(b[i + 1])));
            i += 3;
        } else {
            i += 1;
        }
    }

    let mut groups: Vec<ClauseGroup> = Vec::new();
    let mut last_end = 0usize;
    for (start, end, letter) in markers {
        let joins = match groups.last() {
            Some(_) => {
                let between: String = declaration[last_end..start]
                    .chars()
                    .filter(|c| !c.is_whitespace() && *c != ',' && *c != '*')
                    .collect();
                between.is_empty() || between == "and"
            }
            None => false,
        };
        if joins {
            if let Some(g) = groups.last_mut() {
                g.letters.push(letter);
            }
        } else {
            groups.push(ClauseGroup {
                start,
                letters: vec![letter],
            });
        }
        last_end = end;
    }
    groups
}

/// One statement of a declaration: a byte range and the clause letters it names.
#[derive(Clone, Debug)]
struct Statement {
    start: usize,
    end: usize,
    letters: BTreeSet<char>,
}

/// Byte offsets just past every `;` that is **not** inside brackets.
///
/// The bracket depth is the whole point: item 78's declaration ends
/// `(S; needed a **CDC-ACM device**, not privilege)`, and a depth-blind split would cut
/// a parenthetical aside into a statement that declares nothing.
fn top_level_semicolons(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    for (at, c) in text.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            ';' if depth == 0 => out.push(at + c.len_utf8()),
            _ => {}
        }
    }
    out
}

/// Split a declaration into the statements it is made of.
///
/// The rule this replaces cut a segment only where the text before a clause marker
/// ended in `;`, `,` or `.`, which keyed the whole completeness check on **punctuation
/// the ledger is free to change**. Three of the four ways a mixed entry could lose its
/// open half in silence went through that one condition: drop the separator
/// (`; (b) open**` → ` and (b) still owed**`) and the letters landed inside a segment a
/// sibling clause had already given a status to.
///
/// So a clause group now opens a statement **unconditionally**, and the enumeration
/// case that the separator rule was really there for — item 49's `**EXECUTED**: (a) and
/// (b) 2026-08-12 …`, where one status governs the letters that follow it — is handled
/// where it belongs, by [`classify`] letting a letters-only statement inherit a status
/// that was stated *ahead of* any clause letter. A top-level `;` opens a statement too,
/// but only in a declaration that speaks in clauses at all: that is what catches the
/// fourth shape, where the letter is dropped along with the status
/// (`; (b) open**` → `; the device half is still owed**`) and nothing is left to
/// hang a clause check on. Requiring a status in every `;`-statement of *every*
/// declaration was measured against the tree and refused four entries (3, 34, 44, 48)
/// whose execution records legitimately continue past a semicolon in prose.
fn statements(declaration: &str, groups: &[ClauseGroup]) -> Vec<Statement> {
    let mut cuts: BTreeSet<usize> = BTreeSet::from([0usize]);
    for group in groups {
        cuts.insert(group.start);
    }
    if !groups.is_empty() {
        for at in top_level_semicolons(declaration) {
            if at < declaration.len() {
                cuts.insert(at);
            }
        }
    }
    let starts: Vec<usize> = cuts.into_iter().collect();
    starts
        .iter()
        .enumerate()
        .map(|(k, &start)| {
            let end = starts.get(k + 1).copied().unwrap_or(declaration.len());
            Statement {
                start,
                end,
                letters: letters_in(groups, start, end),
            }
        })
        .collect()
}

/// Whether a statement says nothing at all — emphasis, dashes and punctuation only.
///
/// The joiner between two clause statements (`— **`, ` **`) is not a claim and must not
/// be asked for a status; a statement with words in it and no status is the defect.
fn is_contentless(text: &str) -> bool {
    !text.chars().any(|c| c.is_alphanumeric())
}

fn letters_in(groups: &[ClauseGroup], start: usize, end: usize) -> BTreeSet<char> {
    groups
        .iter()
        .filter(|g| g.start >= start && g.start < end)
        .flat_map(|g| g.letters.iter().copied())
        .collect()
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

/// What one declaration says, or why it cannot be read.
struct Verdict {
    status: Status,
    spelling: &'static str,
    at: usize,
    open_letters: BTreeSet<char>,
    matched: BTreeSet<&'static str>,
}

/// One recognised status hit: what it says, how it is spelled, and where.
struct Hit {
    status: Status,
    spelling: &'static str,
    at: usize,
}

/// Decide one entry's status, or say why it cannot be decided.
///
/// Every statement the declaration is made of must account for itself, and a mixture
/// must be declared rather than inferred. Every refusal names the item, because the
/// repair is always in the document.
fn classify(number: u32, declaration: &str, spellings: Spellings<'_>) -> Result<Verdict, String> {
    let groups = clause_groups(declaration);
    let statements = statements(declaration, &groups);
    let names_clauses = groups.iter().any(|g| !g.letters.is_empty());

    let mut hits: Vec<Hit> = Vec::new();
    let mut matched: BTreeSet<&'static str> = BTreeSet::new();
    let mut resolved: Vec<Option<Status>> = Vec::with_capacity(statements.len());
    // The status a letters-only enumeration may inherit. It is set only by a statement
    // that declares its status *ahead of* its own clause letters — `**EXECUTED**: (a)
    // and (b) …` governs what follows it, while `(a) EXECUTED …` describes (a) and
    // governs nothing. That distinction is the whole difference between item 49, which
    // is one statement, and `(a) EXECUTED … and (b) still owed`, which is two and must
    // not be read as one.
    let mut governing: Option<Status> = None;

    for statement in &statements {
        let text = &declaration[statement.start..statement.end];
        let mut own: Vec<Hit> = Vec::new();
        for (status, table) in spellings.tables() {
            for (at, spelling) in spelling_hits(text, table) {
                own.push(Hit {
                    status,
                    spelling,
                    at: statement.start + at,
                });
                matched.insert(spelling);
            }
        }
        own.sort_by_key(|h| h.at);

        if own.is_empty() {
            if statement.letters.is_empty() {
                if is_contentless(text) {
                    resolved.push(None);
                    continue;
                }
                return Err(format!(
                    "plan §18 item {number}'s status declaration contains a statement that \
                     declares no status this gate recognises.\n\
                     The statement reads:\n    {}\n{}\n\
                     A declaration that speaks in clauses is read one statement at a time, and \
                     every one of them has to say where it stands. Rewording an open half into \
                     prose — `; (b) open**` becoming `; the device half is still owed**` — used \
                     to leave the item classified by whichever sibling clause still matched.",
                    text.chars().take(200).collect::<String>(),
                    known_spellings(spellings),
                ));
            }
            let Some(inherited) = governing else {
                return Err(format!(
                    "plan §18 item {number} names clause(s) {} in its status declaration and \
                     declares no status this gate recognises for them.\n\
                     The clause reads:\n    {}\n{}\n\
                     A clause whose status is reworded must fail here rather than leave the item \
                     classified by whichever sibling clause still matches — that is exactly how \
                     items 31, 64 and 78 could be closed silently before this rule existed. A \
                     clause may borrow a status only from one stated ahead of it, the way item \
                     49 writes `**EXECUTED**: (a) and (b) …`.",
                    render_letters(&statement.letters),
                    text.chars().take(200).collect::<String>(),
                    known_spellings(spellings),
                ));
            };
            resolved.push(Some(inherited));
            continue;
        }

        let opens = own.iter().any(|h| h.status.is_open());
        let closes = own.iter().any(|h| !h.status.is_open());
        if opens && closes {
            return Err(format!(
                "plan §18 item {number} declares an open status and a closed one in the *same* \
                 clause, so its mixture would have to be inferred rather than read.\n\
                 The statement reads:\n    {}\n\
                 Spell the clauses the way items 13, 64 and 78 do — `(a) EXECUTED …; (b) open` \
                 — or say `PARTLY`. Inferring the mixture from whichever spelling matched first \
                 is what let a reworded clause close an item in silence.",
                text.chars().take(200).collect::<String>(),
            ));
        }
        let status = if own.iter().any(|h| h.status == Status::Partly) {
            Status::Partly
        } else if opens {
            Status::Open
        } else if own.iter().any(|h| h.status == Status::Executed) {
            Status::Executed
        } else if own.iter().any(|h| h.status == Status::Declined) {
            Status::Declined
        } else {
            Status::Carried
        };
        let first_letter = groups
            .iter()
            .filter(|g| g.start >= statement.start && g.start < statement.end)
            .map(|g| g.start)
            .min();
        governing = match first_letter {
            Some(at) if own[0].at > at => None,
            _ => Some(status),
        };
        resolved.push(Some(status));
        hits.extend(own);
    }

    if hits.is_empty() {
        return Err(format!(
            "plan §18 item {number} declares no status this gate recognises.\n\
             Its status declaration reads:\n    {}\n{}\n\
             A gate that guessed here is how AGENTS §2's list went stale: add the new spelling \
             to `meta_ledger.rs` in the same commit that introduces it, or spell the status the \
             way the other entries do.",
            declaration.chars().take(200).collect::<String>(),
            known_spellings(spellings),
        ));
    }

    // When the ledger writes a mixture, the open half has to say *which* clause is
    // open: that letter is what AGENTS §2 carries beside the number, and an open
    // statement that names none leaves the copy nothing to be checked against. Closed
    // statements are exempt — item 13's executed half is "the measuring half", which
    // the entry deliberately never lettered.
    if names_clauses {
        for (index, statement) in statements.iter().enumerate() {
            let Some(status) = resolved[index] else {
                continue;
            };
            if status.is_open() && statement.letters.is_empty() {
                return Err(format!(
                    "plan §18 item {number} declares a mixture but its open statement names no \
                     clause letter, so there is nothing for AGENTS §2's `{number}(x)` to be \
                     checked against.\n\
                     The statement reads:\n    {}\n\
                     Name the clause — items 13, 64 and 78 write `(b) open`, `(a) PARTLY`, \
                     `(b) open` — or state one status for the whole item.",
                    declaration[statement.start..statement.end]
                        .chars()
                        .take(200)
                        .collect::<String>(),
                ));
            }
        }
    }

    let live: Vec<(usize, Status)> = resolved
        .iter()
        .enumerate()
        .filter_map(|(index, status)| status.map(|s| (index, s)))
        .collect();
    let declares_partly = live.iter().any(|&(_, s)| s == Status::Partly);
    let open_statements: BTreeSet<usize> = live
        .iter()
        .filter(|(_, s)| s.is_open())
        .map(|&(index, _)| index)
        .collect();
    let closed_statements: BTreeSet<usize> = live
        .iter()
        .filter(|(_, s)| !s.is_open())
        .map(|&(index, _)| index)
        .collect();

    let status =
        if declares_partly || (!open_statements.is_empty() && !closed_statements.is_empty()) {
            Status::Partly
        } else if closed_statements.is_empty() {
            Status::Open
        } else if live.iter().any(|&(_, s)| s == Status::Executed) {
            Status::Executed
        } else if live.iter().any(|&(_, s)| s == Status::Declined) {
            Status::Declined
        } else {
            Status::Carried
        };

    // The deciding hit is the earliest one that carries the verdict, so the quote in a
    // drift report is the text the reader has to go and change.
    let wanted = |h: &Hit| match status {
        Status::Partly => {
            if declares_partly {
                h.status == Status::Partly
            } else {
                h.status.is_open()
            }
        }
        other => h.status == other,
    };
    let decider = hits
        .iter()
        .filter(|h| wanted(h))
        .min_by_key(|h| h.at)
        .expect("the verdict came from a hit");

    let open_letters = statements
        .iter()
        .enumerate()
        .filter(|(index, _)| open_statements.contains(index))
        .flat_map(|(_, statement)| statement.letters.iter().copied())
        .collect();

    Ok(Verdict {
        status,
        spelling: decider.spelling,
        at: decider.at,
        open_letters,
        matched,
    })
}

fn known_spellings(spellings: Spellings<'_>) -> String {
    let one = |table: &[(&'static str, Match)]| {
        table.iter().map(|&(s, _)| s).collect::<Vec<_>>().join(", ")
    };
    format!(
        "Known spellings — open: [{}]; executed: [{}]; partly: [{}]; declined: [{}]; \
         carried: [{}].",
        one(spellings.open),
        one(spellings.executed),
        one(spellings.partly),
        one(spellings.declined),
        one(spellings.carried),
    )
}

fn render_letters(letters: &BTreeSet<char>) -> String {
    if letters.is_empty() {
        return "(none)".to_string();
    }
    letters.iter().map(|c| format!("({c})")).collect()
}

// ---------------------------------------------------------------------------
// Walking the ledger
// ---------------------------------------------------------------------------

/// Parse plan §18 into its items.
///
/// `markers`, `remainders`, `spellings`, `title`, `bound` and `continuation` are
/// parameters only so the tests can switch one mechanism off at a time and require the
/// answer to change; production callers use [`parse_ledger`].
fn parse_ledger_with(
    plan: &str,
    markers: &[&str],
    remainders: &[&'static str],
    spellings: Spellings<'_>,
    title: Title,
    bound: Bound,
    continuation: Continuation,
) -> Result<Vec<Item>, String> {
    let lines: Vec<&str> = plan.lines().collect();
    let (start, end) = ledger_line_range(&lines)?;

    // Entry heads look like `12. **`, at column zero. Everything else in the section
    // is continuation (indented 3–4 spaces), a `###` sub-heading, or a blank line.
    let heads: Vec<(u32, usize)> = (start..end)
        .filter_map(|i| head_number(lines[i]).map(|n| (n, i)))
        .collect();
    if (heads.len() as u32) < SANITY_FLOOR {
        return Err(format!(
            "plan §18 parsed {} item heads, below the sanity floor of {SANITY_FLOOR} — this is \
             not a ledger, so the walker is reading the wrong region of the document",
            heads.len()
        ));
    }

    // The floor is the document's own highest head number, read with a looser matcher
    // than the walker's — a head that stops being bold, or that lands past a heading
    // the walker treats as the end, still counts here. Deriving `expected` from
    // `heads.len()` instead (as this gate first did) structurally cannot see a lost
    // tail: a parse that stops early is contiguous from 1 over everything it did read.
    let highest = highest_head_number(&lines, start);
    let expected: Vec<u32> = (1..=highest).collect();
    let got: Vec<u32> = heads.iter().map(|&(n, _)| n).collect();
    if got != expected {
        let missing: Vec<u32> = expected
            .iter()
            .copied()
            .filter(|n| !got.contains(n))
            .collect();
        let shown: Vec<u32> = missing.iter().copied().take(8).collect();
        return Err(format!(
            "plan §18's item heads are not 1..={highest} contiguous. The highest head number in \
             the section is {highest}; the walker reached {} head(s), missing {shown:?}{}. \
             The ledger is append-only (§18's **Numbering**), so a gap means the parser stopped \
             or lost a head, never that an item was removed.",
            heads.len(),
            if missing.len() > shown.len() {
                format!(" and {} more", missing.len() - shown.len())
            } else {
                String::new()
            },
        ));
    }

    let mut items = Vec::with_capacity(heads.len());
    for (k, &(number, head_line)) in heads.iter().enumerate() {
        // An entry ends at the next entry, at the next `###` sub-heading (whose
        // following paragraph belongs to the *group*, not to the entry above it), or
        // at the end of the section.
        let hard_stop = heads.get(k + 1).map(|&(_, i)| i).unwrap_or(end);
        let stop = (head_line + 1..hard_stop)
            .find(|&i| lines[i].starts_with("### "))
            .unwrap_or(hard_stop);
        let entry = normalise(&lines[head_line..stop].join("\n"));
        let region = status_region_of(&entry, title);
        let declaration = declaration_of(region, bound, continuation);
        refuse_superseded(number, declaration, markers)?;
        let verdict = classify(number, declaration, spellings)?;
        // The ledger's own schema, read as a backstop under the declaration: an entry
        // that still owes a `Remainder` in its live record has open work whatever its
        // opening statement says. It never *closes* an item and never contradicts an
        // open declaration — the only move it can make is to keep one on the list.
        let owed = owed_remainder(
            record_region_of(live_region_of(&entry, markers)),
            remainders,
        );
        let (status, spelling, quote) = match owed {
            Some((at, label)) if !verdict.status.is_open() => (
                Status::Partly,
                label,
                quote_from(record_region_of(live_region_of(&entry, markers)), at),
            ),
            _ => (
                verdict.status,
                verdict.spelling,
                quote_from(declaration, verdict.at),
            ),
        };
        items.push(Item {
            number,
            line: head_line + 1,
            status,
            spelling,
            quote,
            open_letters: verdict.open_letters,
            matched: verdict.matched,
            remainder: owed.map(|(_, label)| label),
        });
    }
    Ok(items)
}

fn parse_ledger(plan: &str, markers: &[&str]) -> Result<Vec<Item>, String> {
    parse_ledger_with(
        plan,
        markers,
        REMAINDER_LABELS,
        SPELLINGS,
        Title::Stripped,
        Bound::FirstSentence,
        Continuation::Shipped,
    )
}

/// The line range of plan §18's item list, as `[start, end)`.
///
/// Every walk over the ledger goes through here. The plan is full of numbered lists
/// outside §18 — §3's harness rules, §12's checklists — and an entry head is spelled
/// exactly like one of their items, so a search that is not bounded by these two
/// headings finds the wrong `1. **`.
fn ledger_line_range(lines: &[&str]) -> Result<(usize, usize), String> {
    let start = lines
        .iter()
        .position(|l| l.starts_with(LEDGER_START))
        .ok_or_else(|| format!("plan has no `{LEDGER_START}` heading"))?;
    let end = lines
        .iter()
        .skip(start + 1)
        .position(|l| l.starts_with(LEDGER_END))
        .map(|p| p + start + 1)
        .ok_or_else(|| format!("plan §18 has no `{LEDGER_END}` heading closing it"))?;
    Ok((start, end))
}

/// The line range of one entry inside plan §18, as `[head, stop)`.
fn entry_line_range(lines: &[&str], number: u32) -> (usize, usize) {
    let (start, end) = ledger_line_range(lines).expect("plan §18 is present");
    let head = (start..end)
        .find(|&i| head_number(lines[i]) == Some(number))
        .unwrap_or_else(|| panic!("item {number} has no head line inside plan §18"));
    let stop = (head + 1..end)
        .find(|&i| head_number(lines[i]).is_some() || lines[i].starts_with("### "))
        .unwrap_or(end);
    (head, stop)
}

/// One entry's whitespace-normalised text, exactly as the walker assembles it.
fn entry_text(plan: &str, number: u32) -> String {
    let lines: Vec<&str> = plan.lines().collect();
    let (head, stop) = entry_line_range(&lines, number);
    normalise(&lines[head..stop].join("\n"))
}

/// `12. **` → `Some(12)`: the walker's own matcher, at column zero and bolded.
fn head_number(line: &str) -> Option<u32> {
    let digits: String = line.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || !line[digits.len()..].starts_with(". **") {
        return None;
    }
    digits.parse().ok()
}

/// A head number seen with a looser eye than the walker's.
///
/// The floor exists to see a head the walker **cannot**. It used to demand column-zero
/// digits followed by `". "`, which is two of the walker's own three requirements — so
/// the two shapes that had actually lost a tail entry were invisible to it: indenting
/// the last entry's head by two spaces, or writing it `**NN.** The pattern-…`, each left
/// `parse_ledger` returning `Ok` one entry short, contiguous from 1, no error, and the
/// tail silently gone. Leading whitespace and `**` on either side of the dot are
/// tolerated, and
/// [`a_tail_entry_that_stops_looking_like_a_head_is_still_counted_by_the_floor`] plants
/// all three shapes.
///
/// **What this matcher reaches is not "every way an entry can stop looking like an
/// entry", and that absolute — which stood here — is retired rather than restated.**
/// The reach is: an optional indent, an optional `**`, the digits, a `.`, an optional
/// `**`, and then a space or end of line. Outside it, each of `NN) **Title**` (valid
/// CommonMark for an ordered list), `- NN. **Title**`, `*NN.* **Title**` and
/// `NN.**Title**` still returns `None`, so a tail entry written any of those four ways
/// is lost by the walker *and* uncounted by the floor — `Ok`, contiguous from 1, one
/// entry gone. Named here because a hole a reader can see is worth more than an
/// absolute that reads as a guarantee.
///
/// The plausible one, `NN)`, is still **not** taken — **but the reason recorded here was
/// wrong, and both halves of it were wrong** (plan §18 item 100). It said accepting `)`
/// beside `.` makes this matcher read a wrapped CI run id — a line beginning
/// `31689537882) and it moved the failure one stage forward…` — as a head, putting the
/// floor at 31689537882 and refusing every parse. It cannot: that number is larger than
/// the `u32` head numbers parse into, so the widened matcher answers `None` on it for a
/// reason the bracket has nothing to do with. It also named the wrong entry — the line
/// is item 68's, not item 66's. Measured instead of remembered, by
/// [`the_costs_that_keep_the_continuation_rule_narrow_are_re_measured_here`]: the
/// document's `NN)` lines are wrapped numbers in prose, and **not one of them would
/// become the floor today** — the other, item 41's wrapped baud rate `9600); two pty
/// itests…`, escapes on the byte after the bracket rather than on anything the widening
/// changes. So the narrowness is kept on what it *is* rather than on a cost it does not
/// currently have: a looser eye buys nothing once it is loose enough to read prose as
/// structure, every instance of the family here is prose, and the two escapes are
/// accidents of their own rather than a rule. If `NN)` is ever wanted, it needs a
/// condition that separates a head from a wrapped number, not another separator byte.
///
/// What this matcher reads on the tree is derived rather than frozen — the number that
/// stood here went stale by three items while still reading as a present-tense
/// measurement. [`the_floors_matcher_reads_this_trees_own_highest_head_and_nothing_beyond_it`]
/// re-measures it on every run, and prints it: its highest reading from §18's heading to
/// EOF equals the walker's own highest head, and nothing above that is read past
/// [`LEDGER_END`], so the looser eye costs the floor no precision.
fn loose_head_number(line: &str) -> Option<u32> {
    let trimmed = line.trim_start();
    let body = trimmed.strip_prefix("**").unwrap_or(trimmed);
    let digits: String = body.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let rest = body[digits.len()..].strip_prefix('.')?;
    let rest = rest.strip_prefix("**").unwrap_or(rest);
    if rest.is_empty() || rest.starts_with(' ') {
        digits.parse().ok()
    } else {
        None
    }
}

/// The highest item head number the document spells from `start` onward.
///
/// The scan deliberately runs to the end of the file rather than stopping at
/// [`LEDGER_END`]: a heading that appears early would otherwise truncate the floor by
/// exactly as much as it truncates the walk, and the two errors would cancel into a
/// short parse that is contiguous from 1 over everything it did read. That is not a
/// claim in a comment — [`the_floor_scan_runs_past_the_closing_heading`] builds the
/// fixture and reads both answers. What it costs on *this* document is derived rather
/// than frozen, for the reason the number that stood here demonstrated: it said 98 and
/// the ledger had reached 101, still reading as a present-tense measurement.
/// [`the_floors_matcher_reads_this_trees_own_highest_head_and_nothing_beyond_it`] takes
/// it on every run — the scan's highest reading equals the walker's own highest head,
/// and nothing above that is read after the closing heading, so the wider scan costs the
/// floor no precision.
fn highest_head_number(lines: &[&str], start: usize) -> u32 {
    highest_head_number_in(&lines[start..])
}

/// The same scan over an explicit slice, so
/// [`the_floor_scan_runs_past_the_closing_heading`] can show what a scan bounded by
/// [`LEDGER_END`] would have answered instead.
fn highest_head_number_in(lines: &[&str]) -> u32 {
    lines
        .iter()
        .filter_map(|l| loose_head_number(l))
        .max()
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// AGENTS §2's copy
// ---------------------------------------------------------------------------

/// The literal that opens AGENTS §2's hand-kept copy of the open set.
const AGENTS_LIST_LEAD: &str = "Still open:";

/// The words §2's list may spell *between* its entries.
///
/// Read off the record rather than imagined, the way [`SUPERSEDED_MARKERS`] was: `and`
/// and `plus` join entries; `residual` is the tail of item 4's `4-residual`; and
/// `the new` is how §2 introduced a freshly-filed item in the em-dash shape
/// (`… **79** and the new **91** — 64(a) and 79 were open before`). The table is
/// deliberately closed and small — five words — because its whole job is to tell a list
/// from prose, and prose has verbs.
///
/// A legitimate rewrite of §2 that reaches for a sixth connective lands here as a named
/// refusal quoting the word. That is the intended cost: adding the word is a one-line
/// edit with a reason attached, and the alternative — a matcher loose enough that no
/// rewrite ever trips it — is a matcher that cannot tell the list from the paragraph it
/// sits in.
const LIST_CONNECTIVES: &[&str] = &["and", "plus", "residual", "the", "new"];

/// Characters that carry emphasis or quotation around a list entry and say nothing about
/// its grammar. Trimmed from both ends of a segment before its shape is read, because §2
/// writes `**64(a)**` and has quoted an older list in backticks.
const LIST_EMPHASIS: &[char] = &['*', '_', '`', '"', '\''];

/// Where §2's list stops and its commentary starts.
///
/// The terminator is not part of the segment, so a test that rewrites the list in
/// memory leaves the document's own punctuation in place.
///
/// **Bracket depth is load-bearing**, and it was not here until §2 wrote an aside into
/// the list itself: `plus **13(b)** (the gating half, owed — its declaration was made
/// explicit 2026-08-21 …)`. A depth-blind scan takes that em dash for the list's
/// terminator, truncates one entry early and drops `64(a)` from the copy — reporting
/// drift against a §2 sentence that is right. A terminator inside a bracketed aside is
/// not the list's terminator.
fn list_terminator(rest: &str) -> Option<usize> {
    let b = rest.as_bytes();
    let mut depth = 0i32;
    for (at, c) in rest.char_indices() {
        match c {
            '(' | '[' => {
                depth += 1;
                continue;
            }
            ')' | ']' => {
                depth = (depth - 1).max(0);
                continue;
            }
            _ => {}
        }
        if depth > 0 {
            continue;
        }
        if c == '\u{2014}' {
            return Some(at);
        }
        if c == '.' {
            let digit_before = at > 0 && b[at - 1].is_ascii_digit();
            let digit_after = at + 1 < b.len() && b[at + 1].is_ascii_digit();
            if digit_before && digit_after {
                continue;
            }
            let mut j = at + 1;
            while j < b.len() && matches!(b[j], b'*' | b'"') {
                j += 1;
            }
            if j >= b.len() || b[j] == b' ' {
                return Some(at);
            }
        }
    }
    None
}

/// Every run of digits `text` spells at bracket **depth 0**, as
/// `(value, first byte, one past the last byte)`.
///
/// Digits inside brackets are commentary, never list entries: §2's own aside
/// `(the gating half, owed — its declaration was made explicit 2026-08-21 …)` would
/// otherwise put items 2026, 8 and 21 on the derived list.
///
/// Lifted out of [`parse_agents_open_list`] so that
/// [`list_continues_past_terminator`] can run the same scan over the text *after* the
/// terminator. Two scans that disagreed about what a number is would make that check a
/// different question from the one it is asked.
fn depth0_numbers(text: &str) -> Result<Vec<(u32, usize, usize)>, String> {
    let bytes = text.as_bytes();
    let mut out: Vec<(u32, usize, usize)> = Vec::new();
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'(' | b'[' => {
                depth += 1;
                i += 1;
            }
            b')' | b']' => {
                depth = (depth - 1).max(0);
                i += 1;
            }
            c if c.is_ascii_digit() => {
                let s = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if depth == 0 {
                    let n: u32 = text[s..i].parse().map_err(|e| {
                        format!("AGENTS §2 list: unparseable number {:?}: {e}", &text[s..i])
                    })?;
                    out.push((n, s, i));
                }
            }
            _ => i += 1,
        }
    }
    Ok(out)
}

/// Every alphabetic word `text` spells at bracket depth 0, lowercased.
///
/// Bracketed text is skipped for the same reason the number scan skips it: §2 writes its
/// commentary inside brackets — `(PARTLY)`, `(the gating half, owed …)`, `(partial)` —
/// and that commentary is prose by design. What is left outside the brackets is the list
/// itself, and a list has a five-word vocabulary.
fn depth0_words(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut depth = 0i32;
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut Vec<String>| {
        if !word.is_empty() {
            out.push(std::mem::take(word).to_lowercase());
        }
    };
    for c in text.chars() {
        match c {
            '(' | '[' => {
                flush(&mut word, &mut out);
                depth += 1;
            }
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth > 0 => {}
            _ if c.is_alphabetic() => word.push(c),
            _ => flush(&mut word, &mut out),
        }
    }
    flush(&mut word, &mut out);
    out
}

/// Everything about `segment` that says it is **not** a list of item numbers.
///
/// This is what replaced the count floor (plan §18 item 115), and the reasoning behind
/// the replacement is the point rather than the code.
///
/// The floor said `spans.len() < 10` is a defect. That is a claim about the *schedule* —
/// how much work happens to be open — wearing the clothes of a claim about the sentence,
/// and it fails the moment enough items close: executing items 73 and 85 took the derived
/// set to nine and reddened three tests, none of which had found anything. A gate that
/// hard-codes how much work is open asserts a property of the project's plan, not of its
/// code, and **completing work is what exposes it**.
///
/// The hazard the floor was reaching for is real and is kept: if the terminator search
/// stops inside the list, or the lead literal matches some other sentence, this parser
/// reads a number set that is not §2's list — and if the plan's derived set happened to
/// be the same set, the comparison would pass for the wrong reason. So the question is
/// re-asked structurally: *did the parser consume the sentence it claims to have
/// consumed?* Three claims answer it, and not one of them counts anything:
///
/// 1. the segment's depth-0 words all come from [`LIST_CONNECTIVES`] — prose that has
///    grown into the list is named, word for word, and with it every date that would
///    otherwise be read as three item numbers;
/// 2. it opens on an entry and does not end on a dangling separator or connective — the
///    shape an edit leaves when an entry is deleted off either end;
/// 3. no number is hyphen-joined to the text before it — `2026-08-21` is a date and
///    `12-20` is a range spelled with the wrong dash, and §2's ranges use an en dash.
///
/// A segment with **no** numbers in it is a list of nothing, which is a legitimate answer
/// at the end of the ledger's life and is returned as the empty set; the same segment with
/// *words* in it is prose and is refused. That asymmetry is the whole repair in one line:
/// small is not a defect, unread is.
///
/// **What was rejected, each recorded because it is the obvious move.**
/// *Deleting the check* — the hazard is real, and the two plant batteries derive their
/// controls through this parser, so a silently-truncated read would disarm them rather
/// than redden them. *Asserting every depth-0 digit run is accounted for* — measured to
/// be vacuous before it was rejected: [`parse_agents_open_list`]'s own loop consumes every
/// span, as a range's two ends or as a single entry, so the assertion is true by
/// construction and would pass with the parser gutted. *Deriving the floor from the plan's
/// own parse* (`spans.len() >= derived.len()`) — it re-introduces exactly the coupling
/// being removed, and it adds nothing the comparison does not already do: a segment
/// shorter than the derived set is drift, and the drift report already names every
/// missing item and quotes the segment verbatim. *A synthetic sentence of known shape in
/// addition to the live one* — that one was **not** rejected; it is
/// [`the_agents_list_is_read_at_any_size_the_ledger_can_reach`], and it is the parser half
/// of the split this file made twice before.
///
/// The limit, stated because it is a design choice: a truncation that leaves a
/// **well-formed prefix** — an unbracketed em-dash aside mid-list — is not caught here,
/// because a prefix of a list is a list and nothing local to the segment can tell them
/// apart. It is caught by the comparison instead, which reports the missing items and
/// prints the segment it read; and its most likely spelling, a stray full stop, *is*
/// caught, by [`list_continues_past_terminator`].
fn list_shape_defects(segment: &str, numbers: &[(u32, usize, usize)]) -> Vec<String> {
    let mut defects: Vec<String> = Vec::new();

    let unaccounted: Vec<String> = depth0_words(segment)
        .into_iter()
        .filter(|w| !LIST_CONNECTIVES.contains(&w.as_str()))
        .collect();
    if !unaccounted.is_empty() {
        defects.push(format!(
            "it spells word(s) no list connective accounts for: {unaccounted:?} (the list's whole \
             vocabulary is {LIST_CONNECTIVES:?}, and everything else §2 wants to say about an \
             item goes in brackets, where this parser reads none of it). Either prose has grown \
             into the list — which is this refusal doing its job, and note that any date it \
             swallowed was about to be read as item numbers — or §2 has reached for a connective \
             the record has not used, in which case add it to `LIST_CONNECTIVES` with the reason"
        ));
    }

    let bytes = segment.as_bytes();
    for &(n, start, _) in numbers {
        if start > 0 && bytes[start - 1] == b'-' {
            defects.push(format!(
                "the number {n} is hyphen-joined to the text in front of it. §2 spells its ranges \
                 with an en dash (U+2013) and its one hyphenated entry is `4-residual`, where the \
                 hyphen *follows* the number; an ASCII hyphen in front of a number is a date \
                 (`2026-08-21`, three phantom items) or a range this parser would read as two \
                 unrelated entries"
            ));
        }
    }

    let trimmed = segment.trim_matches(|c: char| c.is_whitespace() || LIST_EMPHASIS.contains(&c));
    if trimmed.is_empty() {
        // A list of nothing. Legitimate, and deliberately not a refusal — see the doc
        // comment: the count floor's error was treating small as broken.
        return defects;
    }
    if numbers.is_empty() {
        defects.push(format!(
            "it names no item number at all, and yet is not empty: {trimmed:?}. Whatever the \
             parser stopped on, it was not §2's list"
        ));
        return defects;
    }
    let first = trimmed.chars().next().expect("non-empty");
    if !first.is_ascii_digit() {
        defects.push(format!(
            "it opens on {first:?} rather than on an item number — the shape an edit leaves when \
             the list's first entry is deleted and its connective is not"
        ));
    }
    let last = trimmed.chars().last().expect("non-empty");
    let dangling = if matches!(last, ',' | ';' | ':' | '-' | '\u{2013}' | '\u{2014}') {
        Some(last.to_string())
    } else if last.is_alphabetic() {
        let tail: String = trimmed
            .chars()
            .rev()
            .take_while(|c| c.is_alphabetic())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<String>()
            .to_lowercase();
        // `residual` closes item 4's entry and is not dangling; every other connective is.
        (tail != "residual" && LIST_CONNECTIVES.contains(&tail.as_str())).then_some(tail)
    } else {
        None
    };
    if let Some(dangling) = dangling {
        defects.push(format!(
            "it ends on {dangling:?}, which is a separator and not an entry — the shape an edit \
             leaves when the list's last entry is deleted and its connective is not"
        ));
    }
    defects
}

/// The text just past the terminator, if the list appears to **carry on** into it.
///
/// The other half of "did the parser consume the sentence it claims to have consumed",
/// and the half that catches a real truncation. [`list_terminator`] stops at the *first*
/// depth-0 sentence end or em dash, so a stray full stop inside the list — a comma typed
/// as a stop, the likeliest single-character edit there is — silently ends the segment
/// early and hands back a prefix. A prefix of a list is itself a well-formed list, so
/// nothing about the segment can say so; what says so is the text on the *other* side of
/// the stop, which in that case is more list.
///
/// So the run from the terminator to the next one is read with the same two scans the
/// segment gets: if it names at least one item number and [`list_shape_defects`] finds
/// nothing wrong with it, it is a list, and the parser stopped in the middle of one.
///
/// Both shapes the record actually writes clear this comfortably, which is what makes it
/// usable: `Everything else in 1–98 is executed or closed as a recorded decline` and
/// `64(a) and 79 were open before; **80–83 were executed**` are both stuffed with verbs.
/// Commentary that is *nothing but* numbers and connectives would be refused, and the
/// message says to bracket it or reword it — the same intended cost as
/// [`LIST_CONNECTIVES`]'s, for the same reason.
fn list_continues_past_terminator(rest: &str, end: usize) -> Option<String> {
    let terminator = rest[end..].chars().next()?;
    let body = &rest[end + terminator.len_utf8()..];
    let run = match list_terminator(body) {
        Some(stop) => &body[..stop],
        None => body,
    };
    let numbers = depth0_numbers(run).ok()?;
    if numbers.is_empty() {
        return None;
    }
    list_shape_defects(run, &numbers)
        .is_empty()
        .then(|| run.to_string())
}

/// AGENTS §2's `Still open:` sentence, parsed.
struct AgentsList {
    numbers: BTreeSet<u32>,
    /// The clause letters §2 names beside a number — `**64(a)**` → `64 → {a}`.
    letters: BTreeMap<u32, BTreeSet<char>>,
    segment: String,
}

/// Parse AGENTS §2's `Still open:` sentence.
///
/// Its grammar is prose, and every part of it is load-bearing:
///
/// * ranges use an **en dash** (U+2013) — `12–20` — which is not the em dash
///   (U+2014) §2 comments after, and not the ASCII hyphen in `4-residual`;
/// * entries carry `**` emphasis (`**64(a)**`) and clause letters (`78(b)`); the
///   number names the ledger item and the letter names which of its clauses is the
///   open one, and both are compared;
/// * `4-residual` is item 4, and a parenthetical that is not a single lowercase
///   letter — `31(partial)`, `**13** (measured, gating half carried)` — is
///   commentary rather than a clause.
///
/// The list ends at the first **sentence end or em dash, whichever comes first**:
/// both spellings are in the record, because §2 has been written each way — a list
/// closed by an em dash before its commentary (`… and the new **91** — 64(a) and 79
/// were open before`) and one closed by a full stop before it (`… and **64(a)**
/// (PARTLY). Everything else in 1–98 is executed`). Reading past either terminator
/// reads the opposite of what this gate wants, because the commentary names *more*
/// item numbers and says they were executed — and the second shape even carries the
/// en-dash range `1–98`, which a runaway parse would expand into ninety-eight open
/// items. The `executed` tripwire below is the backstop on both.
fn parse_agents_open_list(agents: &str) -> Result<AgentsList, String> {
    let lead = agents
        .find(AGENTS_LIST_LEAD)
        .ok_or_else(|| format!("AGENTS.md §2 has no `{AGENTS_LIST_LEAD}` sentence"))?;
    let rest = &agents[lead + AGENTS_LIST_LEAD.len()..];
    // (R1) THE LEAD IS SPELLED ONCE. This parser takes the *first* occurrence, so a
    // second one anywhere in the file means it may be reading a sentence that is not
    // §2's list — an older list quoted in prose, or a second list §2 has grown — and no
    // count could ever tell it so. That is half of the hazard the deleted floor was
    // aimed at ("the lead matches a different segment"), and it is the half a count is
    // structurally blind to.
    if let Some(again) = rest.find(AGENTS_LIST_LEAD) {
        return Err(format!(
            "AGENTS.md spells `{AGENTS_LIST_LEAD}` twice — at byte {lead} and again at byte \
             {} — and this gate reads the list by that literal, taking the first, so it \
             cannot know which of the two it is reading. Either the second is a quotation \
             of an older list, which AGENTS §10 would have paraphrased anyway, or §2 has \
             grown a second list and the two must be merged.\nSecond occurrence: {}",
            lead + AGENTS_LIST_LEAD.len() + again,
            rest[again..].chars().take(160).collect::<String>()
        ));
    }
    let end = list_terminator(rest).ok_or_else(|| {
        format!(
            "AGENTS.md §2's `{AGENTS_LIST_LEAD}` sentence is closed by neither a full stop nor \
             an em dash (U+2014); this gate reads the list only up to one of those, because §2 \
             names further item numbers in the commentary after it. Text seen: {}",
            rest.chars().take(160).collect::<String>()
        )
    })?;
    let segment = rest[..end].to_string();
    // A tripwire on the one way the terminator can fail open. §2's commentary after
    // the em dash names further item numbers and says they *were executed*; if that
    // word is inside the segment, the dash this parser found is not the one that ends
    // the list, and the numbers it read are the opposite of what it wanted.
    if segment.to_ascii_lowercase().contains("executed") {
        return Err(format!(
            "AGENTS §2's `{AGENTS_LIST_LEAD}` segment runs past its terminator — it contains \
             the word `executed`, which belongs to the commentary after the em dash, not to \
             the list.\nSegment: {segment}"
        ));
    }

    // (number, byte offset of its first digit, byte offset just past its last digit),
    // read at bracket depth 0 only.
    let spans = depth0_numbers(&segment)?;

    // (R2) THE SEGMENT READS AS A LIST. What stood here was `spans.len() < 10` — a
    // count of how much work happens to be open, which went false the moment two more
    // items closed. The shape is asserted instead of the size; [`list_shape_defects`]
    // carries the whole reasoning, including what was rejected and what this cannot see.
    let defects = list_shape_defects(&segment, &spans);
    if !defects.is_empty() {
        return Err(format!(
            "AGENTS §2's `{AGENTS_LIST_LEAD}` segment does not read as a list of item numbers, \
             so the {} number(s) this parser took out of it are not §2's list:\n  - {}\n\
             Segment: {segment}",
            spans.len(),
            defects.join("\n  - ")
        ));
    }

    // (R3) ...AND IT ENDS WHERE THE PARSER STOPPED. A stray full stop inside the list
    // hands back a well-formed *prefix*, which nothing local to the segment can tell
    // from a short list — the text past the terminator is what tells.
    if let Some(run) = list_continues_past_terminator(rest, end) {
        return Err(format!(
            "AGENTS §2's `{AGENTS_LIST_LEAD}` list continues past the terminator this parser \
             stopped at: the text after it is itself a list of item numbers, so the segment read \
             is a prefix of §2's list rather than the whole of it. A stray full stop where a \
             comma belongs does exactly this, and the resulting short read compares clean against \
             everything it did not see.\nSegment read: {segment}\nList past the terminator: {run}"
        ));
    }

    let mut numbers = BTreeSet::new();
    let mut letters: BTreeMap<u32, BTreeSet<char>> = BTreeMap::new();
    let mut k = 0usize;
    while k < spans.len() {
        let (from, _, from_end) = spans[k];
        // A range is `A–B` with nothing but the en dash and spaces between them.
        let ranged = spans.get(k + 1).filter(|&&(_, next_start, _)| {
            let between = segment[from_end..next_start].trim();
            between == "\u{2013}"
        });
        match ranged {
            Some(&(to, _, _)) if to >= from => {
                numbers.extend(from..=to);
                k += 2;
            }
            _ => {
                numbers.insert(from);
                let named = clause_letters_after(&segment, from_end);
                if !named.is_empty() {
                    letters.entry(from).or_default().extend(named);
                }
                k += 1;
            }
        }
    }
    Ok(AgentsList {
        numbers,
        letters,
        segment,
    })
}

/// The `(a)`-shaped clause letters written immediately after a number in §2's list.
fn clause_letters_after(segment: &str, at: usize) -> BTreeSet<char> {
    let mut out = BTreeSet::new();
    let mut rest = segment[at..].trim_start_matches('*');
    while rest.len() >= 3 {
        let b = rest.as_bytes();
        if b[0] == b'(' && b[1].is_ascii_lowercase() && b[2] == b')' {
            out.insert(char::from(b[1]));
            rest = rest[3..].trim_start_matches('*');
        } else {
            break;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The comparison
// ---------------------------------------------------------------------------

/// Render the drift between the ledger's open set and AGENTS' copy, or `None` if
/// they agree.
///
/// The message names each disagreeing item with its plan line and the verbatim text
/// the classification came from. "The lists disagree" is useless to whoever has to
/// fix it; a line number and a quote is the whole repair.
fn drift_report(items: &[Item], listed: &AgentsList) -> Option<String> {
    let by_number: BTreeMap<u32, &Item> = items.iter().map(|i| (i.number, i)).collect();
    let derived: BTreeSet<u32> = items
        .iter()
        .filter(|i| i.status.is_open())
        .map(|i| i.number)
        .collect();

    let wrongly_listed: Vec<u32> = listed.numbers.difference(&derived).copied().collect();
    let wrongly_omitted: Vec<u32> = derived.difference(&listed.numbers).copied().collect();
    // Clause letters, for the items both sides agree are open. `78(a)` where (a) is
    // the executed clause is drift, and comparing numbers alone reads it as agreement.
    let letter_drift: Vec<u32> = derived
        .intersection(&listed.numbers)
        .copied()
        .filter(|n| {
            let ours = by_number.get(n).map(|i| i.open_letters.clone());
            let theirs = listed.letters.get(n).cloned().unwrap_or_default();
            ours.unwrap_or_default() != theirs
        })
        .collect();
    if wrongly_listed.is_empty() && wrongly_omitted.is_empty() && letter_drift.is_empty() {
        return None;
    }

    let describe = |n: u32| match by_number.get(&n) {
        Some(item) => format!(
            "  item {:<3} plan:{:<5} {:<8} matched {:?} in: {}{}",
            n,
            item.line,
            item.status.label(),
            item.spelling,
            item.quote,
            if item.remainder.is_some() && !REMAINDER_LABELS.contains(&item.spelling) {
                "\n      (this entry also owes a live Remainder — see §18's Schema)"
            } else {
                ""
            }
        ),
        None => format!(
            "  item {n:<3} — no such entry in plan §18; AGENTS §2 names an item the ledger \
             does not have"
        ),
    };

    let mut out = String::new();
    out.push_str(
        "AGENTS §2's `Still open:` list does not match plan §18's item states.\n\
         Plan §18 is the authority (item 95: \"§18's item entries are the only currently-accurate \
         surface\"); AGENTS §2 is the copy, and the copy is what gets repaired.\n\n",
    );
    if !wrongly_listed.is_empty() {
        out.push_str(&format!(
            "AGENTS §2 calls these {} item(s) open; plan §18 does not:\n",
            wrongly_listed.len()
        ));
        for n in &wrongly_listed {
            out.push_str(&describe(*n));
            out.push('\n');
        }
        out.push('\n');
    }
    if !wrongly_omitted.is_empty() {
        out.push_str(&format!(
            "Plan §18 has these {} item(s) open; AGENTS §2 omits them:\n",
            wrongly_omitted.len()
        ));
        for n in &wrongly_omitted {
            out.push_str(&describe(*n));
            out.push('\n');
        }
        out.push('\n');
    }
    if !letter_drift.is_empty() {
        out.push_str(&format!(
            "Both lists have these {} item(s) open, but disagree about *which clause*:\n",
            letter_drift.len()
        ));
        for n in &letter_drift {
            let ours = by_number
                .get(n)
                .map(|i| i.open_letters.clone())
                .unwrap_or_default();
            let theirs = listed.letters.get(n).cloned().unwrap_or_default();
            out.push_str(&describe(*n));
            out.push('\n');
            out.push_str(&format!(
                "      plan §18 declares {} open; AGENTS §2 names {}\n",
                render_letters(&ours),
                render_letters(&theirs)
            ));
        }
        out.push('\n');
    }
    out.push_str(&format!(
        "AGENTS §2 as parsed ({} numbers): {:?}\n\
         Plan §18 as derived ({} numbers): {:?}\n\
         The list AGENTS §2 should carry, in ledger order: {}\n\
         Verbatim segment parsed from AGENTS §2:{}\n",
        listed.numbers.len(),
        listed.numbers.iter().copied().collect::<Vec<_>>(),
        derived.len(),
        derived.iter().copied().collect::<Vec<_>>(),
        derived
            .iter()
            .map(|n| match by_number.get(n) {
                Some(item) if !item.open_letters.is_empty() =>
                    format!("{n}{}", render_letters(&item.open_letters)),
                _ => n.to_string(),
            })
            .collect::<Vec<_>>()
            .join(", "),
        listed.segment,
    ));
    Some(out)
}

fn tree_plan() -> String {
    read_tree_file(&normative_plan_path(&repo_root()))
}

fn tree_ledger() -> Vec<Item> {
    parse_ledger(&tree_plan(), SUPERSEDED_MARKERS).unwrap_or_else(|e| panic!("{e}"))
}

fn tree_agents() -> String {
    read_tree_file(&repo_root().join("AGENTS.md"))
}

// ---------------------------------------------------------------------------
// The gate
// ---------------------------------------------------------------------------

#[test]
fn agents_2s_still_open_list_matches_plan_18s_item_states() {
    let items = tree_ledger();
    let agents = tree_agents();
    let listed = parse_agents_open_list(&agents).unwrap_or_else(|e| panic!("{e}"));

    // Both sides assert a floor. A gate that compares two sets it enumerated to zero
    // passes forever, which is the silent-disarm shape AGENTS §3 names.
    assert!(
        items.iter().any(|i| i.status.is_open()),
        "plan §18 derived zero open items — the classifier has stopped recognising the open \
         spellings, not the ledger stopped having open work"
    );
    assert!(
        items.iter().any(|i| i.status == Status::Executed),
        "plan §18 derived zero executed items — the classifier has stopped recognising the \
         executed spellings"
    );
    assert!(
        !listed.numbers.is_empty(),
        "AGENTS §2's `Still open:` list parsed empty from: {}",
        listed.segment
    );

    let held: Vec<u32> = items
        .iter()
        .filter(|i| i.remainder.is_some())
        .map(|i| i.number)
        .collect();
    println!(
        "LEDGER {} items, {} open; live Remainder above the filing in {held:?}",
        items.len(),
        items.iter().filter(|i| i.status.is_open()).count(),
    );

    if let Some(report) = drift_report(&items, &listed) {
        panic!("{report}");
    }
}

// ---------------------------------------------------------------------------
// Proving the matcher, the walker and the comparison (AGENTS §3)
// ---------------------------------------------------------------------------

/// How many entries a synthetic ledger carries. A fixture size, not a census: the
/// real ledger's length is derived from the real ledger.
const FIXTURE_ITEMS: u32 = SANITY_FLOOR + 4;

/// A synthetic ledger holding one entry per line of `entries`, wrapped in the real
/// section headings and padded to the fixture size with plainly-executed filler.
fn synthetic_ledger(entries: &[(u32, &str)]) -> String {
    let mut out = String::from("# plan\n\n");
    out.push_str(LEDGER_START);
    out.push_str("\n\nPreamble prose.\n\n");
    let overrides: BTreeMap<u32, &str> = entries.iter().copied().collect();
    for n in 1..=FIXTURE_ITEMS {
        match overrides.get(&n) {
            Some(body) => out.push_str(&format!("{n}. {body}\n")),
            None => out.push_str(&format!(
                "{n}. **Filler item {n}** — Executed 2026-01-01 (notes §9.9).\n"
            )),
        }
    }
    out.push('\n');
    out.push_str(LEDGER_END);
    out.push_str(" — the closing register\n\nMore prose.\n");
    out
}

/// The parsed item 7 of a one-entry fixture, under a chosen vocabulary.
fn fixture_item(body: &str, spellings: Spellings<'_>) -> Result<Item, String> {
    let plan = synthetic_ledger(&[(7, body)]);
    parse_ledger_with(
        &plan,
        SUPERSEDED_MARKERS,
        REMAINDER_LABELS,
        spellings,
        Title::Stripped,
        Bound::FirstSentence,
        Continuation::Shipped,
    )
    .map(|items| {
        items
            .into_iter()
            .find(|i| i.number == 7)
            .expect("item 7 is in every fixture")
    })
}

fn status_of(body: &str) -> Status {
    fixture_item(body, SPELLINGS)
        .unwrap_or_else(|e| panic!("{e}"))
        .status
}

#[test]
fn every_status_spelling_the_ledger_uses_is_recognised_in_its_own_spelling() {
    // AGENTS §3: "plant the violation in every spelling it claims to cover". These are
    // transcribed from the real entries named beside them.
    let cases: &[(&str, Status, &str)] = &[
        (
            "**A title** — **EXECUTED 2026-08-13** (notes §3.96).",
            Status::Executed,
            "item 12",
        ),
        (
            "**A title (S).** Executed 2026-08-05 (notes §3.57).",
            Status::Executed,
            "item 1",
        ),
        (
            "**A title** (S). **Executed 2026-08-12** (notes §3.75).",
            Status::Executed,
            "item 32 — the size marker sits outside the title",
        ),
        (
            "**A title** — **executed 2026-08-17** (§15.63).",
            Status::Executed,
            "item 86",
        ),
        (
            "**A title** — **MEASURED 2026-08-13** (notes §3.96).",
            Status::Executed,
            "item 13",
        ),
        (
            "**A title** — **EXECUTED**: (a) and (b) landed.",
            Status::Executed,
            "item 49 — an enumeration under one status, not two clauses",
        ),
        (
            "**A title** — **(a) and (b) EXECUTED 2026-08-13**; **(f) CLOSED AS A DECLINE**.",
            Status::Executed,
            "item 65",
        ),
        (
            "**A title** — **EXECUTED 2026-08-12** for clauses (b), (c) and (d); \
             **(f) and one paragraph of (a) are carried**, blocked elsewhere.",
            Status::Executed,
            "item 55 — a carried clause closes the item, its work being filed elsewhere",
        ),
        (
            "**A title.** **State:** open (S/M).",
            Status::Open,
            "item 15",
        ),
        (
            "**A title** — **open** (S). *Evidence:* nothing.",
            Status::Open,
            "item 84",
        ),
        (
            "**A title** — **(b) EXECUTED**; **(a) PARTLY, with its remedy REFUTED**.",
            Status::Partly,
            "item 64",
        ),
        (
            "**A title** — **(a) EXECUTED 2026-08-16; (b) open** (S; needs a device).",
            Status::Partly,
            "item 78",
        ),
        (
            "**A title** — **CLOSED AS A DECLINE 2026-08-12.** The item offered two answers.",
            Status::Declined,
            "item 45",
        ),
        (
            "**A title** — **CLOSED AS A MEASURED DECLINE 2026-08-21** (notes §3.123).",
            Status::Declined,
            "item 98",
        ),
    ];
    for (body, want, whose) in cases {
        assert_eq!(
            status_of(body),
            *want,
            "the spelling taken from {whose} classified wrong: {body}"
        );
    }

    // The near-misses. Each of these appears in the real ledger's prose and none of
    // them is a status; a matcher that fires on them reports drift that is not there.
    // Every one is written so the near-miss sits *inside* the declaration, because a
    // near-miss the parser never reads proves nothing about the parser.
    let bare_open: Vec<(&'static str, Match)> = vec![("open", Match::Sub)];

    let prose_executed = "**A title** — **open** (S), and the guard was executed on the Mac \
                          while `jq -e` was executed against all six.";
    assert_eq!(
        status_of(prose_executed),
        Status::Open,
        "unbolded lowercase `executed` is prose, not a status (item 18's shape): {prose_executed}"
    );

    let title_says_open = "**The pattern-stimulus experiment — open, and the first thing to \
                           run.** **CLOSED AS A MEASURED DECLINE 2026-08-21**.";
    assert_eq!(
        status_of(title_says_open),
        Status::Declined,
        "a title that contains the word open is not an open status (item 98): {title_says_open}"
    );
    // ...and that near-miss is reached rather than decorative: with the title kept,
    // the declaration ends inside the title and no status is left to read.
    let plan = synthetic_ledger(&[(7, title_says_open)]);
    let err = parse_ledger_with(
        &plan,
        SUPERSEDED_MARKERS,
        REMAINDER_LABELS,
        SPELLINGS,
        Title::Kept,
        Bound::FirstSentence,
        Continuation::Shipped,
    )
    .expect_err("with the title kept, a title that is a sentence hides the status");
    assert!(
        err.contains("item 7") && err.contains("declares no status"),
        "{err}"
    );

    let opened_by_the_daemon = "**A title** — **EXECUTED 2026-08-15**, and it reopened nothing \
                                while the port stays opened by the daemon.";
    assert_eq!(
        status_of(opened_by_the_daemon),
        Status::Executed,
        "`reopened` / `opened` must not match the open spellings: {opened_by_the_daemon}"
    );
    // Proven reached, not assumed: the same words *do* fire an open spelling that has
    // dropped its delimiters, so the anchoring in OPEN_SPELLINGS is what saves this.
    let bare = Spellings {
        open: &bare_open,
        ..SPELLINGS
    };
    assert_ne!(
        fixture_item(opened_by_the_daemon, bare).map(|i| i.status),
        Ok(Status::Executed),
        "with a bare `open` spelling this entry must be misread — otherwise the delimiters in \
         OPEN_SPELLINGS are decoration and this near-miss proves nothing"
    );
}

#[test]
fn word_matching_is_load_bearing_for_every_spelling_that_claims_it() {
    // `Match::Word` was inert when this gate landed: downgrading every Word row to Sub
    // left the derived set byte-identical, and the one near-miss that named the
    // distinction never reached it — the legitimate spelling sat at a lower offset and
    // the earliest hit won. So each Word row now gets a case where the *only* thing in
    // the declaration that could match it is a longer word around it, with the entry's
    // real status supplied by a different table.
    let all_sub: Vec<Vec<(&'static str, Match)>> = SPELLINGS
        .tables()
        .iter()
        .map(|(_, table)| {
            table
                .iter()
                .map(|&(s, _)| (s, Match::Sub))
                .collect::<Vec<_>>()
        })
        .collect();
    let sub_only = Spellings {
        open: &all_sub[0],
        executed: &all_sub[1],
        partly: &all_sub[2],
        declined: &all_sub[3],
        carried: &all_sub[4],
    };

    let mut proven = 0usize;
    for (status, table) in SPELLINGS.tables() {
        for &(spelling, kind) in table {
            if kind != Match::Word {
                continue;
            }
            // The host declares a status from a *different* table, so the only route to
            // the spelling under test is the longer word.
            let host = if status == Status::Open || status == Status::Partly {
                "**EXECUTED 2026-08-13**"
            } else {
                "**open** (S)"
            };
            let host_status = if status == Status::Open || status == Status::Partly {
                Status::Executed
            } else {
                Status::Open
            };
            // One host per *edge*, and each is asymmetric: `un{spelling}` is rejected
            // only by the left neighbour check and `{spelling}ness` only by the right,
            // so neither can be satisfied by the other conjunct. The single symmetric
            // fixture that used to stand here — `no_{spelling}_here` — was rejected by
            // either half alone and therefore showed neither to be needed.
            for (edge, longer) in [
                ("left ", format!("un{spelling}")),
                ("right", format!("{spelling}ness")),
            ] {
                let body = format!("**A title** — {host}, and `{longer}` names nothing else.");

                assert_eq!(
                    status_of(&body),
                    host_status,
                    "word matching must not fire inside a longer word: {body}"
                );
                let downgraded = fixture_item(&body, sub_only).map(|i| i.status);
                assert_ne!(
                    downgraded,
                    Ok(host_status),
                    "downgrading {spelling:?} to Match::Sub changed nothing on {body:?}, so \
                     the Word/Sub distinction is inert for that row — make it bite or delete \
                     the row"
                );
                println!(
                    "WORD  {spelling:<12} {edge} edge: Word -> {host_status:?}, \
                     Sub -> {downgraded:?}"
                );
                proven += 1;
            }
        }
    }
    assert!(
        proven >= 3,
        "only {proven} Word spelling(s) were exercised; every row that claims the distinction \
         must be shown to use it"
    );
}

#[test]
fn an_entry_with_no_recognised_status_is_an_error_rather_than_a_default() {
    let plan = synthetic_ledger(&[(42, "**A title** — filed, and someday it will be done.")]);
    let err = parse_ledger(&plan, SUPERSEDED_MARKERS)
        .expect_err("an entry with no status token must not be given one by default");
    assert!(
        err.contains("item 42") && err.contains("declares no status"),
        "the error must name the item that could not be classified: {err}"
    );
}

#[test]
fn a_reworded_clause_status_is_an_error_rather_than_a_silent_reclassification() {
    // THE MATERIAL ONE. Before the clause-completeness rule, each of these left the
    // entry classified EXECUTED with no error at all, because a sibling clause still
    // supplied a status — so the item left the derived open set in silence while the
    // ledger's own words still said open. The three shapes are items 31, 64 and 78.
    let cases: &[(&str, &str, &str)] = &[
        (
            "**A title** — **not done**, and **re-scoped 2026-08-12** from one thing to another.",
            "declares no status",
            "item 31's shape: the single status reworded",
        ),
        (
            "**A title** — **(b), (c) and (d) EXECUTED**; **(a) part-done, with its remedy \
             REFUTED** (notes §3.106).",
            "names clause(s) (a)",
            "item 64's shape: PARTLY reworded, EXECUTED sibling intact",
        ),
        (
            "**A title** — **(a) EXECUTED 2026-08-16; (b) not finished** (S; needs a device).",
            "names clause(s) (b)",
            "item 78's shape: the open clause reworded inside a shared bold run",
        ),
    ];
    for (body, wanted, whose) in cases {
        let err = fixture_item(body, SPELLINGS)
            .expect_err(&format!("{whose} must not classify silently: {body}"));
        assert!(
            err.contains("item 7") && err.contains(wanted),
            "the error must name the item and the clause it could not read ({whose}): {err}"
        );
        println!("REWORD {whose} -> refused");
    }
}

#[test]
fn every_way_a_mixed_entry_can_lose_its_open_half_is_covered() {
    // THE COMMENT THIS REPLACES CLAIMED TOO MUCH. It said the loop over the real
    // ledger below "is where items 31, 64 and 78 are actually proven", but `reworded`
    // preserves every non-alphanumeric byte — so it preserves the clause marker, the
    // statement separator and the sentence boundary, and exercises exactly one of the
    // four shapes a mixed entry can lose its open half in. The other three were each
    // proven to go silently EXECUTED against the real plan, and each is here as a
    // fixture. All four are punctuation- or letter-level edits to item 78's and item
    // 64's own declarations; none of them touches what the entry *means*.
    let mixed_78 = "**A title** — **(a) EXECUTED 2026-08-16; (b) open** \
                    (S; needed a **CDC-ACM device**, not privilege).";
    let mixed_64 = "**A title** — **(b), (c) and (d) EXECUTED**; **(a) PARTLY, with its filed \
                    remedy REFUTED** (notes §3.106).";
    assert_eq!(
        status_of(mixed_78),
        Status::Partly,
        "the control must be mixed"
    );
    assert_eq!(
        status_of(mixed_64),
        Status::Partly,
        "the control must be mixed"
    );

    // (1) Reword the open clause *and* drop its letter. Nothing names a clause any
    //     more, so a rule that only inspects lettered segments has nothing to inspect.
    //     What catches it is that a declaration speaking in clauses is read one
    //     statement at a time, and the statement after the `;` says nothing.
    let dropped_letter = mixed_78.replace("; (b) open**", "; the device half is still owed**");
    let err = fixture_item(&dropped_letter, SPELLINGS)
        .expect_err("a reworded clause that also drops its letter must not classify silently");
    assert!(
        err.contains("item 7") && err.contains("declares no status this gate recognises"),
        "{err}"
    );

    // (2) Drop the *separator*. The letter and the rewording are both there, but the
    //     old segmenter opened a segment only where the preceding text ended in
    //     `;`/`,`/`.`, so `(b)` landed inside the segment `(a)` had already answered.
    let dropped_separator = mixed_78.replace("; (b) open**", " and (b) still owed**");
    let err = fixture_item(&dropped_separator, SPELLINGS)
        .expect_err("a clause that loses its separator must not inherit its sibling's status");
    assert!(
        err.contains("item 7") && err.contains("names clause(s) (b)"),
        "{err}"
    );

    // (3) Drop the letter on the open half of a PARTLY entry. The mixture survives,
    //     but nothing says *which* clause is open — and that letter is precisely what
    //     AGENTS §2 carries as `64(a)`.
    let letterless_open = mixed_64.replace("**(a) PARTLY,", "**PARTLY,");
    let err = fixture_item(&letterless_open, SPELLINGS)
        .expect_err("an open half that names no clause leaves §2's `64(a)` uncheckable");
    assert!(
        err.contains("item 7") && err.contains("names no clause letter"),
        "{err}"
    );

    // (4) THE SHARPEST: punctuation alone. The open clause is left VERBATIM in its
    //     recognised spelling and only the separator between the two halves changes.
    //     The first-sentence bound cut the declaration before `classify` ever saw the
    //     clause.
    //
    //     **Both of the ledger's mixture shapes are here, and covering only one is how
    //     this stayed open.** The repair keyed the continuation on the *emphasis* of
    //     what follows the sentence end — bold, then a clause marker — so it saw item
    //     64's separator between two bold runs and nothing else. Item 64's own next
    //     paragraph is written `*(a) The remedy is refuted, not deferred.*`, so the
    //     italic and plain spellings of that separator are not contrived; and item 78
    //     writes **both halves inside one bold run**, where rewriting its `;` as a `.`
    //     leaves no `**` behind the sentence end at all — one byte, `PARTLY{b}` ->
    //     `EXECUTED`, no error, and the gate then tells its reader to delete a
    //     genuinely-open item from AGENTS §2.
    let punctuation_only: &[(&str, &str, &str, &str)] = &[
        (
            "item 64, two bold runs",
            mixed_64,
            "EXECUTED**; **(a)",
            "EXECUTED.** **(a)",
        ),
        (
            "item 64, italic continuation",
            mixed_64,
            "EXECUTED**; **(a)",
            "EXECUTED.** *(a)",
        ),
        (
            "item 64, unemphasised continuation",
            mixed_64,
            "EXECUTED**; **(a)",
            "EXECUTED.** (a)",
        ),
        (
            "item 78, one shared bold run",
            mixed_78,
            "2026-08-16; (b)",
            "2026-08-16. (b)",
        ),
        // (5) THE MIRROR, plan §18 item 100. The four above put the stop *inside* a
        //     bold run or leave `**` behind it; these put it just **past** a run's
        //     close, with `*` or with nothing after it, which satisfied neither arm the
        //     rule had. All four went silently EXECUTED against the real plan, and the
        //     on-tree guard that was meant to cover this could not express them: it
        //     rewrote the separator `;` in place, which pins the stop to the `;`'s own
        //     offset.
        (
            "item 64, the mirror: stop past the close, italic",
            mixed_64,
            "EXECUTED**; **(a)",
            "EXECUTED**. *(a)",
        ),
        (
            "item 64, the mirror: stop past the close, unemphasised",
            mixed_64,
            "EXECUTED**; **(a)",
            "EXECUTED**. (a)",
        ),
        (
            "item 78, the mirror: the run closed at the stop",
            mixed_78,
            "2026-08-16; (b) open**",
            "2026-08-16**. **(b) open**",
        ),
        // (6) The fifth shape, one separator over: the ledger joins the two statements
        //     with a word. Item 13 writes `**MEASURED …**, and **(b) open**`, and that
        //     comma rewritten as a full stop leaves `and` sitting between the stop and
        //     the clause. On the tree that entry is held open by its live `Remainder`
        //     besides, which made the shape fail loudly there for a reason that has
        //     nothing to do with the rule; here it has no Remainder under it, which is
        //     the silent case.
        (
            "item 13, the joiner: `and` between the stop and the clause",
            "**A title** — **MEASURED 2026-08-13**, and **(b) open**: the measuring half is \
             discharged.",
            "2026-08-13**, and",
            "2026-08-13**. and",
        ),
    ];
    for (whose, mixture, from, to) in punctuation_only {
        let control = fixture_item(mixture, SPELLINGS).unwrap_or_else(|e| panic!("{e}"));
        // Or the letter comparison below is two empty sets agreeing with each other.
        assert!(
            control.status == Status::Partly && !control.open_letters.is_empty(),
            "{whose}: the control must be a mixture that names its open clause, not {} {}",
            control.status.label(),
            render_letters(&control.open_letters)
        );
        let rewritten = mixture.replace(from, to);
        assert_ne!(&rewritten, mixture, "the rewrite must change the bytes");
        // "Punctuation only" is asserted structurally rather than claimed in a comment:
        // the rewrite leaves every alphanumeric byte of the declaration where it was.
        assert_eq!(
            letters_only(&rewritten),
            letters_only(mixture),
            "{whose}: the rewrite {from:?} -> {to:?} changed more than punctuation"
        );
        let after = fixture_item(&rewritten, SPELLINGS)
            .unwrap_or_else(|e| panic!("{whose}: {from:?} -> {to:?}\n{e}"));
        assert_eq!(
            after.status, control.status,
            "{whose}: a punctuation-only rewrite moved the verdict: {rewritten}"
        );
        assert_eq!(
            after.open_letters, control.open_letters,
            "{whose}: a punctuation-only rewrite moved the open clause: {rewritten}"
        );
        println!(
            "SHAPE4 {whose:<34} {from:?} -> {to:?} : {} {} unchanged",
            after.status.label(),
            render_letters(&after.open_letters)
        );
    }
    println!(
        "SHAPES 4 of 4 covered: dropped letter, dropped separator, letterless open, \
              punctuation only (in {} spellings, item 100's mirror family among them)",
        punctuation_only.len()
    );
}

#[test]
fn a_punctuation_only_rewrite_of_a_mixtures_joint_never_moves_the_derived_set_in_silence() {
    // THE BATTERY plan §18 item 100 asks for: every punctuation-only rewrite of the
    // joint between a mixed entry's two clause statements, scored end to end through
    // [`gate`].
    //
    // The guard this replaces rewrote each mixture's separator `;` **in place** — and
    // that, not the entries it picked, is why it missed what item 100 found. A `;`
    // rewritten where it stands always leaves the sentence stop exactly where the `;`
    // was, so no plant it could build ever moved the stop across a `**` boundary, and
    // the whole mirror family — a stop just *past* a run's close, with `*` or with
    // nothing after it — was outside its reach by construction. A guard's coverage is
    // bounded by the edits its plant generator can express, and that bound is invisible
    // in its passing output.
    //
    // So the joint is read as the document spells it — the head's closing `**` if it
    // has one, the punctuation, an optional `and`, and the continuation's emphasis —
    // and the battery rewrites the whole joint, moving the stop to either side of the
    // bold delimiter and varying the continuation's emphasis over all three spellings
    // the ledger uses. `letters_only` over the whole rewritten entry is asserted equal
    // to the original's, so "punctuation only" is structural rather than claimed.
    //
    // **Where the subjects come from changed on 2026-08-21, and the change is the same
    // class of finding as the one the battery exists for.** This test used to require
    // plan §18 itself to spell **both** joint shapes, and took them from items 64 and
    // 78; executing 64(a) and 78(b) left item 13 as the only mixture in the document and
    // the assertion went red. It was right on its own terms — a shape reached by nothing
    // cannot be shown to work, which is exactly what item 100 was filed about — but the
    // property it meant to assert is *the battery reaches both shapes*, and it was
    // reading that off **which items happen to be open**. So closing work broke the
    // gate, which is backwards, and the instruction the failure carried ("reach both
    // shapes") named nothing a reader could do short of reopening an item.
    //
    // The two are separated now. A subject of each shape is **constructed** on every run
    // by planting a mixture over a real open entry: the tree's own document, that
    // entry's own title, size marker, line wraps, siblings and neighbouring prose, with
    // AGENTS.md reconciled against the planted ledger — so both shapes are scored
    // whatever the ledger currently spells, and both joiner spellings with them. Every
    // mixture the document *does* spell is scored too, unchanged, and is what keeps the
    // constructed ones honest: where a live mixture and a constructed one write the same
    // joint token, the two must produce the **same arm for every rewrite**. That is this
    // file's answer to "a fixture easier than the document proves less than the
    // assertion it replaces" (AGENTS §3's sixth register, the stimulus gentler than the
    // product's) — a constructed subject that behaved differently from the live one
    // would not be standing in for it, and the arms are printed either way.
    //
    // No synthetic ledger is built here, on purpose. [`synthetic_ledger`] is the right
    // tool for the walker's structural cases — a gappy list, a lost tail — where the
    // real document cannot be made to hold the defect. A declaration is the opposite
    // case: everything that makes one hard to parse (a title that is itself a sentence,
    // a size marker outside it, 100-column wraps, a superseded filing underneath) is
    // exactly what a hand-written entry drops.
    //
    // The property scored is not "the verdict survives", because it must not be: two of
    // these rewrites move the `**` that an open clause may be *spelled* with
    // (`) open**`), and an entry that stops spelling its status in a recognised way
    // should be refused, loudly, naming itself. What must never happen is the third
    // outcome — the derived open set moving while the parse stays green, which makes
    // this gate report drift against AGENTS §2 and instruct its reader to delete a
    // genuinely-open item. So every rewrite must land in exactly one of two arms:
    //
    //   * `Outcome::Green` — the derived set is unmoved, or
    //   * `Outcome::Refused` naming the entry — the parse says it cannot read it.
    //
    // `Outcome::Drift` is the defect, in either direction, and it is what item 100's
    // four named rewrites produced.
    let plan = tree_plan();
    let agents = tree_agents();
    let items = parse_ledger(&plan, SUPERSEDED_MARKERS).unwrap_or_else(|e| panic!("{e}"));

    // (1) The mixtures the document spells today. A mixture whose joint the plant
    //     generator cannot read is **named** rather than dropped: the `filter_map` that
    //     stood here swallowed it, so a mixture spelled in a shape `clause_joint` does
    //     not reach would have left the battery scoring one subject fewer and printing
    //     the same line as before. It is a report and not an assertion because a
    //     mixture declared by the bare word `PARTLY` in a single statement legitimately
    //     has no joint at all — there being nothing between two statements when there is
    //     only one.
    let mut unreadable: Vec<u32> = Vec::new();
    let mut subjects: Vec<MixtureSubject> = Vec::new();
    for item in items.iter().filter(|i| i.status == Status::Partly) {
        match joint_of(&plan, item.number).and_then(|j| j.shape().map(|s| (j, s))) {
            Some((joint, shape)) => subjects.push(MixtureSubject {
                number: item.number,
                shape,
                joint,
                plan: plan.clone(),
                constructed: false,
            }),
            None => unreadable.push(item.number),
        }
    }
    let live_subjects = subjects.len();

    // (2) One constructed subject per (shape, joiner). The host is a real entry the
    //     ledger declares open with `**open**` and no live `Remainder` under it: no
    //     Remainder because the schema backstop would hold it open through every rewrite
    //     and make each one look Green for a reason the rule under test never touched,
    //     and `**open**` because that is the token the plant replaces with a whole
    //     mixture. Both joiner spellings are built rather than only the one the document
    //     happens to use, because `clause_statement_after`'s `and` tolerance is a second
    //     axis and item 13 is currently the only entry exercising it.
    let host = items
        .iter()
        .find(|i| i.status == Status::Open && i.spelling == "**open**" && i.remainder.is_none())
        .expect(
            "the ledger has no entry declared open by `**open**` with no live Remainder under \
             it, so this battery has nothing to build a mixture over. Any open entry whose \
             status token can be replaced wholesale would do; widen the search rather than \
             dropping the constructed subjects, which are what keep both joint shapes scored.",
        );
    for shape in [JointShape::TwoRuns, JointShape::OneSharedRun] {
        for joiner in ["", "and "] {
            let planted = constructed_mixture(&plan, host.number, shape, joiner);
            let built = parse_ledger(&planted, SUPERSEDED_MARKERS).unwrap_or_else(|e| {
                panic!(
                    "constructing a {} mixture over item {} left plan §18 unparseable, so the \
                     subject is not a subject:\n{e}",
                    shape.label(),
                    host.number
                )
            });
            let built_item = built
                .iter()
                .find(|i| i.number == host.number)
                .expect("the host entry is still there");
            assert!(
                built_item.status == Status::Partly && !built_item.open_letters.is_empty(),
                "the constructed {} mixture over item {} reads {} {} rather than a mixture that \
                 names its open clause — a subject that is not a mixture scores nothing",
                shape.label(),
                host.number,
                built_item.status.label(),
                render_letters(&built_item.open_letters),
            );
            let joint = joint_of(&planted, host.number).unwrap_or_else(|| {
                panic!(
                    "the constructed {} mixture over item {} spells a joint `clause_joint` cannot \
                     read",
                    shape.label(),
                    host.number
                )
            });
            // The constructor claims a shape; the joint reader is what decides it. If
            // these disagree, the subject exercises the shape nobody asked for and the
            // coverage line below is a label rather than a measurement.
            assert_eq!(
                joint.shape(),
                Some(shape),
                "the constructed subject was built as {} and reads back as {:?} (joint {:?})",
                shape.label(),
                joint.shape(),
                joint.token,
            );
            subjects.push(MixtureSubject {
                number: host.number,
                shape,
                joint,
                plan: planted,
                constructed: true,
            });
        }
    }

    // (3) Both shapes must be reached. This can no longer fail on a reconciled document
    //     — the constructed subjects supply both — and it is kept because it is the
    //     property the test is *for*: a refactor that dropped a shape from the
    //     constructor, or a `clause_joint` that stopped distinguishing them, lands here.
    for shape in [JointShape::TwoRuns, JointShape::OneSharedRun] {
        assert!(
            subjects.iter().any(|s| s.shape == shape),
            "no subject exercises the {} shape. The document need not spell it — the \
             constructed subjects exist so that it is scored either way — so this means the \
             constructor or `ClauseJoint::shape` has stopped producing it. Subjects: {:?}",
            shape.label(),
            subjects
                .iter()
                .map(|s| (s.number, s.shape, s.joint.token.as_str()))
                .collect::<Vec<_>>(),
        );
    }

    // (4) Score every subject, and collect rather than assert one at a time: the family
    //     this battery exists for has five members, and a run that stops at the first of
    //     them tells its reader about one shape and hides the other four. Item 100 was
    //     filed from a reading of the rule rather than from a failure, precisely because
    //     no run had ever printed the family together.
    let scores: Vec<Scored> = subjects
        .iter()
        .map(|subject| score_subject(subject, &agents))
        .collect();
    let silent: Vec<&SilentRewrite> = scores.iter().flat_map(|s| s.silent.iter()).collect();
    let total: usize = scores.iter().map(|s| s.total).sum();
    let green: usize = scores.iter().map(|s| s.green).sum();
    let refused: usize = scores.iter().map(|s| s.refused).sum();
    if !silent.is_empty() {
        let mut report = format!(
            "{} of {total} punctuation-only rewrite(s) of a plan §18 mixture moved the answer \
             without saying so.\nEvery one of them leaves the ledger's own words untouched — \
             `letters_only` is asserted equal per rewrite — so what moved is a verdict, never a \
             statement.\n\n",
            silent.len(),
        );
        for entry in &silent {
            // Three lines of each report, not the whole of it: sixteen full drift reports
            // bury the one line that says which shapes went silent, and that line is the
            // finding.
            let head: Vec<&str> = entry.why.lines().take(3).collect();
            report.push_str(&format!(
                "  item {} ({}, {}) — {}\n    joint rewritten as {:?}\n    {}\n\n",
                entry.number,
                if entry.constructed {
                    "constructed"
                } else {
                    "as the ledger spells it"
                },
                entry.shape.label(),
                entry.rewrite.what,
                entry.rewrite.joint,
                head.join("\n    ")
            ));
        }
        panic!("{report}");
    }

    // (5) THE FAITHFULNESS CHECK, and it is what licenses the constructed subjects to
    //     stand in for a shape the document has stopped spelling. Where a live mixture
    //     and a constructed one write the **same joint token**, every rewrite of that
    //     token must land in the same arm for both. A constructed subject that answered
    //     differently from a real entry would be a stimulus gentler (or harsher) than
    //     the product's, which is the register AGENTS §3 names — and it would be
    //     invisible, because both would be printing arms and neither would be printing a
    //     comparison.
    //
    //     Pairing is on the token rather than on the shape, because two subjects of one
    //     shape can still differ in the joiner and in the continuation's emphasis, and
    //     those are the two things the arms actually turn on.
    let mut compared = 0usize;
    let mut unpaired: Vec<(u32, String)> = Vec::new();
    for (subject, scored) in subjects.iter().zip(scores.iter()) {
        if subject.constructed {
            continue;
        }
        let twin = subjects
            .iter()
            .zip(scores.iter())
            .find(|(other, _)| other.constructed && other.joint.token == subject.joint.token);
        let Some((twin, twin_scored)) = twin else {
            unpaired.push((subject.number, subject.joint.token.clone()));
            continue;
        };
        assert_eq!(
            scored.arms,
            twin_scored.arms,
            "the constructed {} mixture over item {} does not answer as item {}'s real one does, \
             joint {:?} in both. A constructed subject that scores differently from the entry it \
             stands in for is not standing in for it: either the construction has drifted from \
             the shape the ledger writes, or the difference is real and worth a ledger entry \
             rather than a silent divergence.",
            subject.shape.label(),
            twin.number,
            subject.number,
            subject.joint.token,
        );
        compared += 1;
        println!(
            "TWIN   item {:>3} (live) and the constructed subject agree on all {} arm(s) of \
             joint {:?}",
            subject.number,
            scored.arms.len(),
            subject.joint.token,
        );
    }
    assert!(
        live_subjects == 0 || compared > 0,
        "plan §18 spells {live_subjects} mixture(s) and not one of them shares a joint token with \
         a constructed subject ({unpaired:?}), so nothing here shows the constructed subjects \
         answering as the document does. Mirror the ledger's joiner and continuation emphasis in \
         `constructed_mixture` rather than leaving the constructed subjects unchecked."
    );

    println!(
        "JOINT  {} subject(s) — {live_subjects} as the ledger spells them, {} constructed; \
         {total} rewrite(s): {green} green, {refused} refused, 0 silent; {compared} live/\
         constructed pair(s) compared arm for arm",
        subjects.len(),
        subjects.len() - live_subjects,
    );
    if !unreadable.is_empty() {
        println!(
            "JOINT  plan §18 declares {unreadable:?} a mixture with no joint this generator can \
             read — a single-statement `PARTLY` has none, and anything else here is a shape \
             `clause_joint` does not reach"
        );
    }
    // Both arms must be reached, or the property is being satisfied by a parser that
    // answers one way to everything — the tell AGENTS §3 names, one level up.
    assert!(
        green > 0 && refused > 0,
        "the battery scored {green} green and {refused} refused; a run that never reaches one of \
         the two arms is not exercising the distinction it asserts"
    );
}

/// Which of the two shapes the ledger writes a mixture's joint in.
///
/// [`continues_with_a_clause_statement`] keys on exactly this distinction, and covering
/// one of the two is how plan §18 item 100's family stayed open: the guard that preceded
/// this battery read item 64's two-run joint and never item 78's shared one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum JointShape {
    /// Both halves inside one bold run — item 78's `**(a) EXECUTED …; (b) open**`.
    OneSharedRun,
    /// Two bold runs with the joint between them — items 64 and 13.
    TwoRuns,
}

impl JointShape {
    fn label(self) -> &'static str {
        match self {
            JointShape::OneSharedRun => "one shared bold run",
            JointShape::TwoRuns => "two bold runs",
        }
    }
}

/// One mixture the battery scores: where it lives, and in which document.
///
/// The document is carried per subject because a constructed subject is a *different*
/// plan — the tree's own with one entry's status rewritten into a mixture — and every
/// figure taken from it (its reconciled AGENTS copy, its control, its `letters_only`
/// baseline) has to come from that plan rather than from the tree's.
struct MixtureSubject {
    number: u32,
    shape: JointShape,
    joint: ClauseJoint,
    plan: String,
    /// False where plan §18 spells this mixture itself.
    constructed: bool,
}

/// One rewrite that moved an answer without saying so.
struct SilentRewrite {
    number: u32,
    constructed: bool,
    shape: JointShape,
    rewrite: JointRewrite,
    why: String,
}

/// What one subject's rewrites came to.
#[derive(Default)]
struct Scored {
    total: usize,
    green: usize,
    refused: usize,
    silent: Vec<SilentRewrite>,
    /// The arm each rewrite landed in, keyed by the rewrite's own description. Read by
    /// the faithfulness check, which compares a live subject's map with its constructed
    /// twin's — the descriptions are a function of the shape and the emphasis, so two
    /// subjects writing the same joint token generate the same keys.
    arms: BTreeMap<String, &'static str>,
}

/// The joint of item `number`'s declaration in `plan`, read the way the parser reads it.
fn joint_of(plan: &str, number: u32) -> Option<ClauseJoint> {
    let entry = entry_text(plan, number);
    let declaration = declaration_of(
        status_region_of(&entry, Title::Stripped),
        Bound::FirstSentence,
        Continuation::Shipped,
    );
    clause_joint(plan, number, declaration)
}

/// `plan` with one real open entry's `**open**` status rewritten into a mixture.
///
/// The two spellings are the ledger's own, taken from the entries that wrote them: item
/// 64's two bold runs with the joint between them, and item 78's single run holding both
/// clauses. The executed half is lettered `(a)` and the open half `(b)`, because a
/// mixture that names no clause letter is refused by [`classify`] and would make a
/// subject that scores nothing.
///
/// Everything else about the entry — its title, its size marker, the prose after its
/// declaration, its line wraps, the entries either side of it — is the document's.
fn constructed_mixture(plan: &str, host: u32, shape: JointShape, joiner: &str) -> String {
    let declaration = match shape {
        JointShape::TwoRuns => format!("**(a) EXECUTED 2026-08-21**, {joiner}**(b) open**"),
        JointShape::OneSharedRun => format!("**(a) EXECUTED 2026-08-21, {joiner}(b) open**"),
    };
    plant_in_entry(plan, host, "**open**", &declaration)
}

/// Every punctuation-only rewrite of one subject's joint, scored through [`gate`].
fn score_subject(subject: &MixtureSubject, agents: &str) -> Scored {
    let items = parse_ledger(&subject.plan, SUPERSEDED_MARKERS).unwrap_or_else(|e| panic!("{e}"));
    let victim = items
        .iter()
        .find(|i| i.number == subject.number)
        .expect("the subject's own entry is in the subject's own plan");
    // Or the letter comparison inside `gate` is two empty sets agreeing with each other.
    assert!(
        !victim.open_letters.is_empty(),
        "item {} is a mixture that names no open clause, so a rewrite that lost the clause would \
         compare equal to one that kept it",
        victim.number
    );
    let reconciled = agents_listing(agents, &items, &BTreeSet::new());
    assert!(
        matches!(gate(&subject.plan, &reconciled), Outcome::Green),
        "the reconciled control for item {} is not green, so no rewrite of it proves anything: {}",
        subject.number,
        gate(&subject.plan, &reconciled).message()
    );
    let control_entry = entry_text(&subject.plan, subject.number);

    let rewrites = subject.joint.rewrites();
    assert!(
        rewrites.len() >= 4,
        "item {}'s joint {:?} generated only {} rewrite(s)",
        subject.number,
        subject.joint.token,
        rewrites.len()
    );

    let origin = if subject.constructed {
        "constructed"
    } else {
        "as spelled  "
    };
    let mut scored = Scored::default();
    for rewrite in &rewrites {
        let mut planted = subject.plan.clone();
        for (needle, with) in &rewrite.edits {
            planted = plant_normalised_in_entry(&planted, subject.number, needle, with);
        }
        assert_ne!(
            planted, subject.plan,
            "the rewrite must change the bytes: {rewrite:?}"
        );
        // "Punctuation only" is asserted structurally rather than claimed in a comment,
        // and over the whole entry rather than over the joint, so a plant that dropped a
        // word somewhere else could not pass as one.
        assert_eq!(
            letters_only(&entry_text(&planted, subject.number)),
            letters_only(&control_entry),
            "item {}: {rewrite:?} changed more than punctuation",
            subject.number
        );

        scored.total += 1;
        let outcome = gate(&planted, &reconciled);
        let arm = match &outcome {
            Outcome::Green => {
                scored.green += 1;
                "GREEN, derived set unmoved"
            }
            Outcome::Refused(message) => {
                scored.refused += 1;
                if message.contains(&format!("item {}", subject.number)) {
                    "REFUSED, naming the entry"
                } else {
                    scored.silent.push(SilentRewrite {
                        number: subject.number,
                        constructed: subject.constructed,
                        shape: subject.shape,
                        rewrite: rewrite.clone(),
                        why: format!(
                            "was refused without naming the entry, so the report points \
                             nowhere:\n{message}"
                        ),
                    });
                    "REFUSED, naming nothing"
                }
            }
            Outcome::Drift(message) => {
                scored.silent.push(SilentRewrite {
                    number: subject.number,
                    constructed: subject.constructed,
                    shape: subject.shape,
                    rewrite: rewrite.clone(),
                    why: format!(
                        "moved the derived open set while the parse stayed green, so this gate \
                         now reports drift against AGENTS §2 and tells its reader to repair the \
                         copy rather than the document:\n{message}"
                    ),
                });
                "SILENT — the derived set moved"
            }
        };
        // Only where the outcome was a refusal: a `Drift` is already recorded above, and
        // one rewrite reported twice reads as two shapes.
        if rewrite.expect_unmoved && matches!(outcome, Outcome::Refused(_)) {
            scored.silent.push(SilentRewrite {
                number: subject.number,
                constructed: subject.constructed,
                shape: subject.shape,
                rewrite: rewrite.clone(),
                why: format!(
                    "keeps every delimiter the entry put there and moves only the stop, so its \
                     verdict must be untouched — it is not:\n{}",
                    outcome.message()
                ),
            });
        }
        scored.arms.insert(rewrite.what.clone(), arm);
        println!(
            "JOINT  {origin} item {:>3} {:<44} {:?} -> {:?}  {arm}",
            subject.number, rewrite.what, subject.joint.token, rewrite.joint
        );
    }
    scored
}

/// The punctuation joint between a mixed entry's last two clause statements, read from
/// the ledger's own bytes.
///
/// This is the plant generator item 100 was filed against, and its shape is the finding.
/// The one it replaces returned the `;` alone and rewrote it in place, which fixes the
/// sentence stop at the separator's own offset — so the family of edits that move the
/// stop across a `**` boundary could not be expressed, let alone scored. Reading the
/// delimiters on both sides of the punctuation is what makes those edits expressible.
#[derive(Clone, Debug)]
struct ClauseJoint {
    /// The joint verbatim, from the head's closing `**` (or from the punctuation, when
    /// the joint sits inside a bold run) up to the clause marker: `**; **`, `; `,
    /// `**, and **`.
    token: String,
    /// The joint with enough of the text before it to name it uniquely inside the
    /// entry, whitespace-normalised — what a plant hands [`plant_normalised_in_entry`].
    needle: String,
    /// `"and "` where the ledger joins the two statements with that word (item 13), else
    /// `""`. Preserved verbatim by every rewrite, since it is the joint's only letters.
    joiner: &'static str,
    /// The emphasis the second statement opens with: `**`, `*` or nothing.
    emphasis: String,
    /// Whether the head closes a bold run immediately before the punctuation — the two
    /// halves are two runs (items 64 and 13).
    head_closes_a_run: bool,
    /// Whether the punctuation sits inside an unclosed bold run — the two halves share
    /// one run (item 78).
    inside_a_run: bool,
    /// The text from the clause marker through the `**` that closes the run the joint
    /// sits inside, named uniquely the same way, where there is one. A rewrite that
    /// closes the run early has to move that delimiter too, or it is not the edit item
    /// 100 names — it is an unbalanced entry.
    run_close: Option<String>,
}

/// One punctuation-only rewrite of a joint: the edits to apply, and whether the ledger's
/// own answer must be untouched by it.
#[derive(Clone, Debug)]
struct JointRewrite {
    what: String,
    joint: String,
    edits: Vec<(String, String)>,
    /// True where the rewrite moves only the stop and leaves every emphasis delimiter
    /// where the entry put it. Those cannot change what the entry *says*, so `Green` is
    /// the only acceptable answer; the rest are held to the weaker two-arm property.
    expect_unmoved: bool,
}

impl ClauseJoint {
    /// Which shape this joint is written in, or `None` for a joint that is neither.
    ///
    /// The two flags are read in this order because they are not quite exclusive by
    /// construction: `head_closes_a_run` reads the two bytes behind the punctuation while
    /// `inside_a_run` reads a parity over everything before it, and a document with an
    /// unbalanced run could satisfy both. `None` is a sentence stop with no bold
    /// delimiter on either side of it — a shape the ledger does not write and
    /// [`ClauseJoint::rewrites`] has nothing to say about, since both of its arms are
    /// keyed on one of these flags being set.
    fn shape(&self) -> Option<JointShape> {
        if self.inside_a_run {
            Some(JointShape::OneSharedRun)
        } else if self.head_closes_a_run {
            Some(JointShape::TwoRuns)
        } else {
            None
        }
    }

    fn rewrites(&self) -> Vec<JointRewrite> {
        let mut out = Vec::new();
        let tails: [&str; 3] = ["**", "*", ""];
        if self.head_closes_a_run {
            // Two bold runs. The stop can sit on either side of the head's closing
            // `**` — item 64's `EXECUTED.**` and `EXECUTED**.` — and the continuation
            // carries any of the three emphases. Six rewrites, of which the ledger's own
            // spelling is one and item 100's two named mirrors are two more.
            for (head, where_it_sits) in [
                ("**.", "the stop just past the run's close"),
                (".**", "the stop inside the run"),
            ] {
                for tail in tails {
                    let joint = format!("{head} {}{tail}", self.joiner);
                    out.push(JointRewrite {
                        what: format!("{where_it_sits}, continuation {}", emphasis_name(tail)),
                        joint: joint.clone(),
                        edits: vec![(self.needle.clone(), self.replacement(&joint))],
                        expect_unmoved: tail == self.emphasis,
                    });
                }
            }
        }
        if self.inside_a_run {
            // One shared run. Leaving the delimiters alone puts the stop inside it,
            // which is the ledger's own shape with a full stop for its `;`. Closing the
            // run *at* the stop is item 100's other pair, and it has to move the run's
            // closing `**` as well — otherwise the entry is unbalanced and the edit is
            // not the one the item names.
            let joint = format!(". {}{}", self.joiner, self.emphasis);
            out.push(JointRewrite {
                what: "the stop where the punctuation was".to_string(),
                joint: joint.clone(),
                edits: vec![(self.needle.clone(), self.replacement(&joint))],
                expect_unmoved: true,
            });
            let Some(run_close) = &self.run_close else {
                return out;
            };
            for tail in tails {
                let joint = format!("**. {}{tail}", self.joiner);
                let mut edits = vec![(self.needle.clone(), self.replacement(&joint))];
                // The run's own closing `**` follows the continuation's emphasis: bold
                // keeps it, italic halves it, bare drops it. All three keep every letter.
                let moved = run_close
                    .strip_suffix("**")
                    .expect("the run close ends in its own delimiter");
                edits.push((run_close.clone(), format!("{moved}{tail}")));
                out.push(JointRewrite {
                    what: format!(
                        "the run closed at the stop, continuation {}",
                        emphasis_name(tail)
                    ),
                    joint,
                    edits,
                    expect_unmoved: false,
                });
            }
        }
        out
    }

    /// The needle with its joint swapped for `joint`, its leading context untouched.
    fn replacement(&self, joint: &str) -> String {
        let head = self
            .needle
            .strip_suffix(&self.token)
            .expect("the needle ends in the joint it names");
        format!("{head}{joint}")
    }
}

fn emphasis_name(emphasis: &str) -> &'static str {
    match emphasis {
        "**" => "bold",
        "*" => "italic",
        _ => "unemphasised",
    }
}

/// Read the joint between two clause statements of `declaration` — the first one it
/// spells, which on every mixture the ledger currently writes is the one that separates
/// the executed half from the open half.
///
/// Only a punctuation mark whose neighbours are the ledger's own joiner vocabulary
/// counts: emphasis, spaces and a single `and` — [`clause_groups`]' rule, applied one
/// level up. An **enumeration** comma is excluded by the byte behind it: `(b), (c)` has
/// a clause marker's `)` there, and a joint never does.
fn clause_joint(plan: &str, number: u32, declaration: &str) -> Option<ClauseJoint> {
    let raw = raw_entry(plan, number);
    let b = declaration.as_bytes();
    let mut depth = 0i32;
    for (at, c) in declaration.char_indices() {
        match c {
            '(' | '[' => {
                depth += 1;
                continue;
            }
            ')' | ']' => {
                depth = (depth - 1).max(0);
                continue;
            }
            _ => {}
        }
        if depth != 0 || !matches!(c, ';' | ',') || (at > 0 && b[at - 1] == b')') {
            continue;
        }
        let rest = declaration[at + 1..].trim_start();
        let (joiner, statement) = match rest.trim_start_matches('*').strip_prefix("and ") {
            Some(tail) => ("and ", tail.trim_start()),
            None => ("", rest),
        };
        if !opens_a_clause(statement) {
            continue;
        }
        let emphasis: String = statement.chars().take_while(|c| *c == '*').collect();
        let marker_at = declaration.len() - statement.len() + emphasis.len();
        let token_at = if declaration[..at].ends_with("**") {
            at - 2
        } else {
            at
        };
        let run_close = ends_inside_a_bold_run(declaration, at)
            .then(|| {
                declaration[marker_at..]
                    .find("**")
                    .map(|k| marker_at + k + 2)
            })
            .flatten()
            .map(|end| unique_needle(&raw, declaration, marker_at, end));
        return Some(ClauseJoint {
            token: declaration[token_at..marker_at].to_string(),
            needle: unique_needle(&raw, declaration, token_at, marker_at),
            joiner,
            emphasis,
            head_closes_a_run: token_at < at,
            inside_a_run: ends_inside_a_bold_run(declaration, at),
            run_close,
        });
    }
    None
}

#[test]
fn the_plants_needle_matcher_reads_through_the_documents_line_wraps() {
    // The battery aims its plants with needles read from the whitespace-normalised
    // declaration and applied to the document, which wraps at 100 columns. Deleting the
    // tolerance that bridges the two left all 26 tests green, so it is a mechanism this
    // suite has never seen do anything — AGENTS §3's tell, in the plant machinery rather
    // than in a gate. Two answers were possible: delete it, or show it working and say
    // what it is for. It is shown working here and the count it costs on this tree is
    // printed rather than claimed, because the count is what makes the choice honest.
    let wrapped = "EXECUTED**;\n    **(a) PARTLY";
    assert!(
        !wrapped.contains("EXECUTED**; **(a)"),
        "the fixture must actually wrap, or this proves nothing about wrapping"
    );
    assert_eq!(
        find_normalised_all(wrapped, "EXECUTED**; **(a)").len(),
        1,
        "a joint split across two of the document's lines must still be findable"
    );
    assert_eq!(
        find_normalised_all("EXECUTED**; **(a)", "EXECUTED**; **(b)").len(),
        0,
        "and the tolerance must not turn the matcher into one that matches anything"
    );

    // How many of the tree's own joints need it today. Zero is the expected answer and
    // is printed rather than asserted: it is a property of how the ledger happens to
    // wrap this week, not of anything this file controls.
    let plan = tree_plan();
    let items = parse_ledger(&plan, SUPERSEDED_MARKERS).unwrap_or_else(|e| panic!("{e}"));
    let mut needles = 0usize;
    let mut spanning = 0usize;
    for item in items.iter().filter(|i| i.status == Status::Partly) {
        let entry = entry_text(&plan, item.number);
        let declaration = declaration_of(
            status_region_of(&entry, Title::Stripped),
            Bound::FirstSentence,
            Continuation::Shipped,
        );
        let Some(joint) = clause_joint(&plan, item.number, declaration) else {
            continue;
        };
        let raw = raw_entry(&plan, item.number);
        for needle in [Some(joint.needle), joint.run_close].into_iter().flatten() {
            needles += 1;
            if !raw.contains(&needle) {
                spanning += 1;
            }
        }
    }
    println!(
        "WRAP   {spanning} of {needles} plant needle(s) on this tree span a line break; the \
         tolerance is proven as a unit above rather than by the document"
    );
}

/// One entry's text as the document spells it — line wraps and indentation included.
fn raw_entry(plan: &str, number: u32) -> String {
    let lines: Vec<&str> = plan.lines().collect();
    let (head, stop) = entry_line_range(&lines, number);
    lines[head..stop].join("\n")
}

/// `declaration[from..to]`, widened backwards until it names one place in `raw`.
///
/// A plant needs a needle the document spells exactly once: item 78's joint is `; `,
/// which occurs in its entry a dozen times over, and a plant that rewrote the first of
/// them would be testing a parenthetical aside. Widening backwards over the entry's own
/// words is the cheapest way to get there, and the refusal when it cannot is loud
/// because a joint nobody can name is a joint nobody can plant against.
fn unique_needle(raw: &str, declaration: &str, from: usize, to: usize) -> String {
    let bounds: Vec<usize> = declaration[..from]
        .char_indices()
        .map(|(at, _)| at)
        .chain([from])
        .collect();
    for &start in bounds.iter().rev() {
        if from - start > 120 {
            break;
        }
        let candidate = &declaration[start..to];
        if find_normalised_all(raw, candidate).len() == 1 {
            return candidate.to_string();
        }
    }
    panic!(
        "no slice of {:?} ending at the clause marker names one place in the entry — a plant \
         cannot be aimed at it",
        &declaration[from..to]
    )
}

/// Every place `needle` occurs in `haystack`, where one space in the needle matches any
/// run of whitespace.
///
/// The needle is read from the whitespace-normalised declaration and the haystack is the
/// document, which wraps at 100 columns: a joint that straddles a line break exists in
/// the first and not in the second.
///
/// **The tolerance is inert on this tree, and that is measured rather than glossed** —
/// [`the_plants_needle_matcher_reads_through_the_documents_line_wraps`] counts the
/// joints that need it and prints the count, which is zero today, because
/// [`unique_needle`] widens each needle backwards only until it is unique and every one
/// of them lands inside a single line. It is kept, and proven as a unit instead, because
/// of what the byte-exact alternative does when a joint *does* wrap: not a miss but a
/// **panic** out of `unique_needle`, which widens until nothing matches at all. The
/// battery then stops being able to reach that entry — which is item 100's own failure
/// mode one level down, a plant generator that cannot express the edit it needs. An
/// inert mechanism kept for a stated reason is not the same thing as one nobody has
/// looked at; this comment is the difference.
fn find_normalised_all(haystack: &str, needle: &str) -> Vec<(usize, usize)> {
    let (hb, nb) = (haystack.as_bytes(), needle.as_bytes());
    let mut out = Vec::new();
    'outer: for start in 0..hb.len() {
        let (mut i, mut j) = (start, 0usize);
        while j < nb.len() {
            if nb[j] == b' ' {
                if i >= hb.len() || !hb[i].is_ascii_whitespace() {
                    continue 'outer;
                }
                while i < hb.len() && hb[i].is_ascii_whitespace() {
                    i += 1;
                }
            } else {
                if i >= hb.len() || hb[i] != nb[j] {
                    continue 'outer;
                }
                i += 1;
            }
            j += 1;
        }
        out.push((start, i));
    }
    out
}

/// Replace the one occurrence of `needle` inside item `number`'s entry, matching across
/// the document's line wraps.
///
/// [`plant_in_entry`] is line-oriented and stays that way — its callers plant single
/// tokens that never wrap. This one exists because a *joint* does wrap, and a plant that
/// silently found nothing to rewrite would leave the battery scoring an unplanted tree.
fn plant_normalised_in_entry(plan: &str, number: u32, needle: &str, with: &str) -> String {
    let lines: Vec<&str> = plan.lines().collect();
    let (head, stop) = entry_line_range(&lines, number);
    let raw = lines[head..stop].join("\n");
    let found = find_normalised_all(&raw, needle);
    assert_eq!(
        found.len(),
        1,
        "item {number} spells {needle:?} {} time(s); a plant must name exactly one place",
        found.len()
    );
    let (from, to) = found[0];
    let planted = format!("{}{with}{}", &raw[..from], &raw[to..]);
    let mut out: Vec<&str> = lines[..head].to_vec();
    out.extend(planted.lines());
    out.extend(lines[stop..].iter().copied());
    out.join("\n")
}

#[test]
fn the_costs_that_keep_the_continuation_rule_narrow_are_re_measured_here() {
    // Two widenings are refused by [`continues_with_a_clause_statement`], and both are
    // refused on a **cost**, not on taste. A cost quoted in a comment is a cost nobody
    // re-checks: the ledger is append-only, so the entries that pay it change under the
    // sentence that names them. This re-derives both from the document on every run and
    // prints them, and the shipped rule's own cost — which must be zero — beside them.
    let plan = tree_plan();

    // (0) The shipped widening moves nothing that was already readable. Item 100's
    //     repair adds two routes into a declaration, and a repair that also *changed*
    //     an answer somewhere else would be a second edit hiding inside the first.
    let shipped = parse_ledger(&plan, SUPERSEDED_MARKERS).unwrap_or_else(|e| panic!("{e}"));
    let before = parse_ledger_with(
        &plan,
        SUPERSEDED_MARKERS,
        REMAINDER_LABELS,
        SPELLINGS,
        Title::Stripped,
        Bound::FirstSentence,
        Continuation::BeforeItem100,
    )
    .unwrap_or_else(|e| panic!("the rule as it stood before item 100 must still parse: {e}"));
    let moved: Vec<u32> = shipped
        .iter()
        .zip(before.iter())
        .filter(|(a, b)| a.status != b.status || a.open_letters != b.open_letters)
        .map(|(a, _)| a.number)
        .collect();
    assert!(
        moved.is_empty(),
        "item 100's repair moved {} verdict(s) on the unplanted tree ({moved:?}). It widens which \
         sentence ends continue a declaration; on a document that spells none of the mirror \
         shapes it must read exactly what the narrow rule read, and an entry that moves is a \
         second change riding along with the first.",
        moved.len()
    );
    println!(
        "COST   shipped rule vs the rule before item 100: {} entries, 0 verdicts moved",
        shipped.len()
    );

    // (a) Accept a leading single `*` on the continuation. Three real entries open their
    //     *next paragraph* with an italic clause marker, and swallowing one puts a
    //     paragraph of record inside the declaration — where it declares no status of
    //     its own, or contradicts the one above it.
    let single = refusals_under(&plan, Continuation::ASingleAsteriskLicensesIt);
    assert!(
        !single.is_empty(),
        "accepting a leading single `*` refuses no entry on this tree, so the narrowness it \
         buys is untested — either the ledger has stopped writing `*(a) …` paragraphs, in which \
         case say so where the rule is written, or the measurement has stopped reaching them"
    );

    // (b) Drop the emphasis condition altogether: any clause marker continues. A
    //     superset of (a) by construction, and the assertion is that it is a *strict*
    //     one — at least one entry opens such a paragraph with no emphasis at all, which
    //     is what makes (a) and (b) two measurements rather than one.
    let any = refusals_under(&plan, Continuation::AnyClauseMarker);
    let single_numbers: BTreeSet<u32> = single.iter().map(|&(n, _)| n).collect();
    let any_numbers: BTreeSet<u32> = any.iter().map(|&(n, _)| n).collect();
    assert!(
        single_numbers.is_subset(&any_numbers) && any_numbers.len() > single_numbers.len(),
        "dropping the emphasis condition must refuse a strict superset of what accepting a \
         single `*` refuses — measured: single `*` {single_numbers:?}, any marker {any_numbers:?}"
    );
    println!(
        "COST   a leading single `*` licenses it -> {} entr(ies) stop parsing: {single_numbers:?}",
        single_numbers.len()
    );
    println!(
        "COST   any clause marker continues      -> {} entr(ies) stop parsing: {any_numbers:?}",
        any_numbers.len()
    );
    for (number, reason) in single.iter().chain(any.iter()) {
        println!(
            "COST   item {number:>3}: {}",
            reason.lines().next().unwrap_or("").trim()
        );
    }

    // (c) The third narrowness, in the walker rather than in the classifier:
    //     [`loose_head_number`] will not read `98)` as a head. **The reason recorded
    //     beside it was wrong, and this measurement is what found that out — twice
    //     over.** It named item 68's wrapped CI run id, a line beginning
    //     `31689537882) and it moved the failure…`, and said accepting `)` would put the
    //     floor there and refuse every parse. That number is larger than `u32::MAX`, so
    //     the widened matcher answers `None` on it for a reason the bracket has nothing
    //     to do with; and the document's other `NN)` line — item 41's wrapped baud rate
    //     `9600); two pty itests…` — escapes on the byte *after* the bracket, a `;`
    //     where the matcher wants a space. So the widening costs this document nothing
    //     today, and the decision to stay narrow cannot rest on the sentence that stood
    //     there. What it can rest on is measured here instead: the family exists, every
    //     member of it is a *wrapped number in prose*, and each escapes by an accident
    //     of its own rather than by anything the matcher checks. A near miss is not a
    //     licence.
    let lines: Vec<&str> = plan.lines().collect();
    let (start, end) = ledger_line_range(&lines).expect("plan §18 is present");
    let walker_high = highest_head_number(&lines, start);

    // First that the two matchers differ at all, or every assertion below is about one
    // matcher wearing two names.
    assert_eq!(loose_head_number("102) **A title**"), None);
    assert_eq!(
        loose_head_number_accepting_a_bracket("102) **A title**"),
        Some(102),
        "the widened matcher must read what the shipped one refuses, or measuring its cost \
         measures nothing"
    );

    let bracketed: Vec<&str> = lines[start..end]
        .iter()
        .copied()
        .filter(|l| {
            let t = l.trim_start();
            let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
            digits > 0 && t[digits..].starts_with(')')
        })
        .collect();
    assert!(
        !bracketed.is_empty(),
        "plan §18 spells no `NN)` line at all any more, so this narrowness has nothing to be \
         narrow against on this document — re-derive its reason before leaving the matcher as \
         it is, since the one recorded beside it was already wrong"
    );
    let mut would_become_the_floor: Vec<u32> = Vec::new();
    for line in &bracketed {
        assert_eq!(
            loose_head_number(line),
            None,
            "the shipped matcher read a wrapped number in prose as an item head: {line}"
        );
        let widened = loose_head_number_accepting_a_bracket(line);
        if let Some(n) = widened.filter(|n| *n > walker_high) {
            would_become_the_floor.push(n);
        }
        let why = match widened {
            Some(n) if n > walker_high => format!("READ AS A HEAD, floor would become {n}"),
            Some(n) => format!("read as {n}, at or below the walker's own highest head"),
            None => "still refused — by the u32 the head numbers parse into, or by the byte \
                     after the bracket, not by the bracket"
                .to_string(),
        };
        println!(
            "COST   `NN)` widened: {why}\n           {}",
            line.trim().chars().take(76).collect::<String>()
        );
    }
    println!(
        "COST   `NN)` accepted -> {} of {} bracketed line(s) would become the floor: \
         {would_become_the_floor:?} (walker's highest head is {walker_high})",
        would_become_the_floor.len(),
        bracketed.len()
    );
}

/// Every ledger entry whose status declaration `rule` cannot classify, with the reason.
///
/// [`parse_ledger_with`] stops at the first refusal, which is right for a gate and wrong
/// for a measurement: the *cost* of a widening is every entry it breaks, not the first
/// one in document order.
fn refusals_under(plan: &str, rule: Continuation) -> Vec<(u32, String)> {
    let lines: Vec<&str> = plan.lines().collect();
    let (start, end) = ledger_line_range(&lines).expect("plan §18 is present");
    (start..end)
        .filter_map(|i| head_number(lines[i]))
        .filter_map(|number| {
            let entry = entry_text(plan, number);
            let declaration = declaration_of(
                status_region_of(&entry, Title::Stripped),
                Bound::FirstSentence,
                rule,
            );
            classify(number, declaration, SPELLINGS)
                .err()
                .map(|reason| (number, reason))
        })
        .collect()
}

/// [`loose_head_number`] with `)` accepted beside `.`.
///
/// Exists only so the cost of that widening is read off the document by
/// [`the_costs_that_keep_the_continuation_rule_narrow_are_re_measured_here`] rather than
/// remembered in the comment beside the matcher — where it had gone wrong.
fn loose_head_number_accepting_a_bracket(line: &str) -> Option<u32> {
    let trimmed = line.trim_start();
    let body = trimmed.strip_prefix("**").unwrap_or(trimmed);
    let digits: String = body.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let after = &body[digits.len()..];
    let rest = after
        .strip_prefix('.')
        .or_else(|| after.strip_prefix(')'))?;
    let rest = rest.strip_prefix("**").unwrap_or(rest);
    if rest.is_empty() || rest.starts_with(' ') {
        digits.parse().ok()
    } else {
        None
    }
}

/// Every alphanumeric character of `text`, in order.
///
/// What "punctuation-only" means in this file: a rewrite that leaves this string
/// untouched has changed no word of the ledger, only how it is punctuated and
/// emphasised.
fn letters_only(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).collect()
}

#[test]
fn rewording_an_open_entrys_own_status_never_closes_it_in_silence() {
    // Every entry that is open today, reworded in place, in the tree's own bytes. The
    // property is not "this must be an error" — it is that **the item must not leave
    // the open set without saying so**, which is a weaker demand and the true one.
    // Two arms satisfy it, and which arm an entry takes is printed rather than assumed:
    // the declaration becomes unreadable and the parse refuses, naming the item; or the
    // entry's live *Remainder* holds it open under a declaration that no longer does.
    let plan = tree_plan();
    let items = parse_ledger(&plan, SUPERSEDED_MARKERS).unwrap_or_else(|e| panic!("{e}"));
    let open: Vec<&Item> = items.iter().filter(|i| i.status.is_open()).collect();

    // The clause-completeness rule has to be shown to bite on this document, and what
    // stood here read that off the document's *content*: `items.iter().any(|i| i.status
    // == Status::Partly)`, with a message telling the next reader to "say so here" if the
    // ledger had closed every mixture. That is the third instance of the shape items
    // 64(a) and 78(b) exposed on 2026-08-21, and the only one that was not yet red — the
    // guard would have gone off the day item 13 closes, reporting *completed work* as a
    // defect and offering an instruction (edit the assertion) rather than a repair.
    //
    // Planted instead, which is strictly more than the assertion it replaces: a mixture
    // is **constructed** over a real entry of this tree — the same constructor the joint
    // battery uses — its open clause is reworded into prose with its letter left where it
    // stands, and the parse must refuse naming the item and the clause. That is the rule
    // biting, on the real document, whatever plan §18 currently declares. Which entries
    // the ledger itself spells as mixtures is printed below rather than asserted, because
    // it is a fact about how much work is finished.
    let clause_host = items
        .iter()
        .find(|i| i.status == Status::Open && i.spelling == "**open**" && i.remainder.is_none())
        .expect("the ledger has an entry declared open by `**open**` with no live Remainder");
    let constructed = constructed_mixture(&plan, clause_host.number, JointShape::OneSharedRun, "");
    let reworded_clause = plant_in_entry(&constructed, clause_host.number, ") open**", ") owed**");
    let err = parse_ledger(&reworded_clause, SUPERSEDED_MARKERS).unwrap_err_or_panic(
        "a constructed mixture whose open clause was reworded into prose classified silently, so \
         the clause-completeness rule bites nothing on this document",
    );
    assert!(
        err.contains(&format!("item {}", clause_host.number))
            && err.contains("names clause(s) (b)"),
        "the refusal must name the item and the clause it could not read: {err}"
    );
    let mixtures: Vec<u32> = items
        .iter()
        .filter(|i| i.status == Status::Partly)
        .map(|i| i.number)
        .collect();
    println!(
        "MIXED  clause completeness proven on a mixture constructed over item {}; plan §18 \
         spells {mixtures:?} as mixtures of its own",
        clause_host.number
    );
    let mut refused = 0usize;
    let mut held_by_remainder = 0usize;
    for item in open {
        let planted = plant_in_entry(&plan, item.number, item.spelling, &reworded(item.spelling));
        match parse_ledger(&planted, SUPERSEDED_MARKERS) {
            Err(err) => {
                assert!(
                    err.contains(&format!("item {}", item.number)),
                    "the error must name the item it could not read: {err}"
                );
                refused += 1;
                println!(
                    "REWORD real item {:>3} {:<16} -> refused",
                    item.number, item.spelling
                );
            }
            Ok(after) => {
                let after = after
                    .into_iter()
                    .find(|i| i.number == item.number)
                    .expect("the entry is still there");
                assert!(
                    after.status.is_open(),
                    "rewording item {}'s {:?} closed it in silence",
                    item.number,
                    item.spelling
                );
                assert!(
                    after.remainder.is_some(),
                    "item {} stayed open with its status reworded and no live Remainder under \
                     it, so something other than the two named mechanisms decided it",
                    item.number
                );
                held_by_remainder += 1;
                println!(
                    "REWORD real item {:>3} {:<16} -> still open, held by its live {:?}",
                    item.number, item.spelling, after.spelling
                );
            }
        }
    }
    assert!(
        refused > 0,
        "no open entry's rewording was refused, so the clause-completeness rule bit nothing \
         on this tree"
    );
    println!("REWORD {refused} refused, {held_by_remainder} held open by a live Remainder");
}

#[test]
fn an_undeclared_mixture_is_refused_rather_than_inferred() {
    // Two statuses in one clause say nothing about which one is the item's state. The
    // old classifier called this PARTLY, which is a guess wearing a verdict's clothes.
    let body = "**A title** — **EXECUTED 2026-08-12** and also **open** (S), take your pick.";
    let err = fixture_item(body, SPELLINGS).expect_err("an inferred mixture must be refused");
    assert!(
        err.contains("item 7") && err.contains("same* clause"),
        "the error must say why the mixture could not be read: {err}"
    );

    // The declared shapes still parse, and both count as open.
    assert_eq!(
        status_of("**A title** — **(a) EXECUTED 2026-08-16; (b) open** (S)."),
        Status::Partly
    );
    assert_eq!(
        status_of("**A title** — **(b) EXECUTED**; **(a) PARTLY**."),
        Status::Partly
    );
}

#[test]
fn the_walker_refuses_a_short_gappy_or_tail_truncated_ledger() {
    // A parser that stops early produces a short list, and a short list compares
    // clean against everything it did not read.
    let short = {
        let full = synthetic_ledger(&[]);
        let cut = full.find("\n50. ").expect("filler item 50 exists");
        let tail_at = full.find(LEDGER_END).expect("the closing heading exists");
        format!("{}\n\n{}", &full[..cut], &full[tail_at..])
    };
    let err = parse_ledger(&short, SUPERSEDED_MARKERS).expect_err("a short ledger must fail");
    assert!(err.contains("sanity floor"), "{err}");

    let gappy = synthetic_ledger(&[]).replace("\n3. **Filler item 3**", "\n300. **Filler 300**");
    let err = parse_ledger(&gappy, SUPERSEDED_MARKERS).expect_err("a gappy ledger must fail");
    assert!(err.contains("not 1..="), "{err}");

    // THE TAIL. This is the case a hand-set floor plus a contiguity check derived from
    // `heads.len()` structurally cannot see: everything parsed is contiguous from 1,
    // and the count clears any floor the ledger has already passed. The last entry
    // stops being bold — one edit — and the walker loses it.
    let full = synthetic_ledger(&[]);
    let tail = format!("\n{FIXTURE_ITEMS}. **Filler item {FIXTURE_ITEMS}**");
    let lost = full.replace(&tail, &format!("\n{FIXTURE_ITEMS}. Filler item, unbolded."));
    assert_ne!(full, lost, "the fixture's tail entry must be there to lose");
    let err = parse_ledger(&lost, SUPERSEDED_MARKERS)
        .expect_err("a ledger whose tail entry the walker cannot see must fail");
    assert!(
        err.contains(&format!(
            "highest head number in the section is {FIXTURE_ITEMS}"
        )) && err.contains(&format!("missing [{FIXTURE_ITEMS}]")),
        "the error must name the head the walker did not reach: {err}"
    );
}

#[test]
fn a_planted_open_state_inside_a_superseded_block_does_not_reopen_the_item() {
    // THE NEGATIVE PLANT, and it is what makes the positive ones mean anything: a
    // gate that reads the whole entry passes for the wrong reason on a reconciled
    // tree. Both halves run against one entry so the only variable is where the plant
    // lands.
    let executed = "**A title** — **EXECUTED 2026-08-12** (notes §3.75), both halves. \
                    PLANT The superseded filing follows. *Evidence:* the original's evidence.";

    let inside = executed.replace("PLANT", "");
    let inside = inside.replace(
        "The superseded filing follows.",
        "The superseded filing follows. **State:** open (S).",
    );
    assert_eq!(
        status_of(&inside),
        Status::Executed,
        "an `**State:** open` inside a superseded block must not reopen an executed item — \
         that text is a verbatim record of a filing, not a claim about today"
    );

    let ahead = executed.replace(
        "**EXECUTED 2026-08-12** (notes §3.75), both halves.",
        "**State:** open (S).",
    );
    assert_eq!(
        status_of(&ahead),
        Status::Open,
        "the same text in the declaration MUST change the verdict, or the negative plant above \
         proves nothing: {ahead}"
    );
}

#[test]
fn the_title_strip_is_load_bearing_on_this_tree() {
    // Not a fixture: the real ledger, parsed twice. Most entry titles are themselves
    // sentences — `**Prose-truth sweep (S).**` — so keeping the title ends the
    // declaration before the status is reached. If dropping the mechanism changed
    // nothing, it would be decoration and this gate would be asserting one mechanism
    // fewer than it claims.
    let plan = tree_plan();
    parse_ledger(&plan, SUPERSEDED_MARKERS).unwrap_or_else(|e| panic!("{e}"));
    let err = parse_ledger_with(
        &plan,
        SUPERSEDED_MARKERS,
        REMAINDER_LABELS,
        SPELLINGS,
        Title::Kept,
        Bound::FirstSentence,
        Continuation::Shipped,
    )
    .expect_err(
        "keeping the entry titles changed no verdict on this tree, so the title strip is a \
         mechanism nobody has seen do anything — give it a case or delete it",
    );
    assert!(
        err.contains("declares no status"),
        "the title strip's absence must show up as an unreadable declaration: {err}"
    );
    println!("TITLE  strip off -> {}", err.lines().next().unwrap_or(""));
}

#[test]
fn the_declaration_bound_is_load_bearing_on_this_tree() {
    // The other half of the same proof. With the bound off, the classifier reads the
    // whole entry — including the superseded filing most executed entries carry — and
    // the tripwire on that text is what fires. Either way the answer must *change*:
    // a gate whose passing output is identical to its not-running output is asserting
    // nothing (AGENTS §3).
    let plan = tree_plan();
    let bounded = parse_ledger(&plan, SUPERSEDED_MARKERS).unwrap_or_else(|e| panic!("{e}"));
    match parse_ledger_with(
        &plan,
        SUPERSEDED_MARKERS,
        REMAINDER_LABELS,
        SPELLINGS,
        Title::Stripped,
        Bound::WholeEntry,
        Continuation::Shipped,
    ) {
        Err(e) => {
            assert!(
                e.contains("plan §18 item"),
                "the refusal must name the entry it could not read: {e}"
            );
            println!("BOUND  off -> refused: {}", e.lines().next().unwrap_or(""));
        }
        Ok(whole) => {
            let moved: Vec<u32> = bounded
                .iter()
                .zip(whole.iter())
                .filter(|(a, b)| a.status != b.status)
                .map(|(a, _)| a.number)
                .collect();
            assert!(
                !moved.is_empty(),
                "reading the whole entry moved no verdict and hit no superseded marker, so \
                 neither the declaration bound nor the marker table does anything on this tree"
            );
            println!("BOUND  off -> {} verdict(s) moved: {moved:?}", moved.len());
        }
    }
}

#[test]
fn every_superseded_marker_that_the_ledger_uses_is_matched_where_it_stands() {
    // The walker half of the marker table: each phrase must be findable in the real,
    // whitespace-normalised section, and the normalisation must be shown to be what
    // finds them. The assertion this replaces was `section.contains("The superseded\n")
    // || section.contains("The superseded filing follows")` on already-normalised text
    // — the first term could not be satisfied at all and the second is satisfied by
    // the 26 instances that never wrapped, so it demonstrated nothing about wrapping.
    let plan = tree_plan();
    let start = plan.find(LEDGER_START).expect("plan §18 exists");
    let end = plan[start..].find(LEDGER_END).expect("plan §18 is closed") + start;
    let raw = &plan[start..end];
    let section = normalise(raw);

    let used: Vec<&str> = SUPERSEDED_MARKERS
        .iter()
        .copied()
        .filter(|m| section.contains(m))
        .collect();
    assert!(
        used.len() >= 5,
        "only {} of the {} known superseded markers appear in plan §18 ({used:?}) — the table \
         has drifted from the document it describes",
        used.len(),
        SUPERSEDED_MARKERS.len()
    );

    // The wrapped instances, counted rather than asserted by a literal that cannot
    // occur: an instance that is split across two of the document's 100-column lines
    // exists in the normalised text and not in the raw text.
    let wrapped: Vec<(&str, usize)> = SUPERSEDED_MARKERS
        .iter()
        .map(|m| (*m, section.matches(m).count() - raw.matches(m).count()))
        .filter(|&(_, n)| n > 0)
        .collect();
    let total: usize = wrapped.iter().map(|&(_, n)| n).sum();
    assert!(
        total >= 5,
        "only {total} superseded-marker instance(s) in plan §18 are found by normalisation and \
         not by a line-oriented match ({wrapped:?}). Eight were wrapped when this gate landed \
         (items 18, 19, 22, 38, 46, 52, 53 and 64); a fall to zero means `normalise` has \
         stopped joining lines, and the tripwire on a declaration that reaches one of these \
         phrases would miss every wrapped instance"
    );
    println!("WRAP   {total} marker instance(s) exist only after normalisation: {wrapped:?}");

    // And the tripwire reaches a wrapped instance: the phrase is planted into a real
    // entry's declaration *split across two lines*, exactly as the document writes it.
    //
    // The plant joins the phrase to the status with `and` rather than a full stop, and
    // the assertion is on the tripwire's own tag. Both are deliberate. Written with a
    // full stop, the plant ended the declaration before any status, so the *classifier*
    // failed too — and its error quotes the planted declaration back, which put the
    // words "superseded filing" in the message with the tripwire switched off. The
    // assertion passed on the plant's own bytes.
    let carrier = tree_ledger()
        .into_iter()
        .find(|i| i.status == Status::Executed)
        .expect("the ledger has executed items");
    let planted = plant_in_entry(
        &plan,
        carrier.number,
        carrier.spelling,
        &format!(
            "The superseded\n    filing follows and {}",
            carrier.spelling
        ),
    );
    let err = parse_ledger(&planted, SUPERSEDED_MARKERS).unwrap_err_or_panic(
        "a wrapped superseded marker inside a declaration was not seen — the matcher is \
         line-oriented again",
    );
    assert!(
        err.contains(SUPERSEDED_TRIPWIRE_TAG) && err.contains(&format!("item {}", carrier.number)),
        "{err}"
    );
    println!(
        "WRAP   a two-line marker planted in item {}'s declaration -> refused",
        carrier.number
    );
}

#[test]
fn the_agents_list_grammar_reads_ranges_letters_and_emphasis() {
    let sentence = "prose. **Still open: 4-residual, 8, 12\u{2013}20, 22, 31(partial), \
                    **64(a)** (PARTLY), **78(b)**, **79** and the new **91** \u{2014} 64(a) and \
                    79 were open before; **80\u{2013}83 were executed**.** more prose";
    let got = parse_agents_open_list(sentence).expect("the sample parses");
    let want: BTreeSet<u32> = [
        4, 8, 12, 13, 14, 15, 16, 17, 18, 19, 20, 22, 31, 64, 78, 79, 91,
    ]
    .into_iter()
    .collect();
    assert_eq!(
        got.numbers, want,
        "the en-dash range must expand, `4-residual` must be item 4, `(a)`/`(b)` clauses must \
         name their item, and the numbers after the em dash must NOT be read: {}",
        got.segment
    );
    assert!(
        !got.numbers.contains(&80) && !got.numbers.contains(&83),
        "the em dash ends the list; 80–83 are named after it as *executed*: {}",
        got.segment
    );

    // The clause letters, which the comparison now checks rather than parsing away.
    assert_eq!(got.letters.get(&64), Some(&BTreeSet::from(['a'])));
    assert_eq!(got.letters.get(&78), Some(&BTreeSet::from(['b'])));
    assert_eq!(
        got.letters.get(&31),
        None,
        "`31(partial)` is commentary, not a clause letter: {}",
        got.segment
    );
    assert_eq!(got.letters.get(&79), None);

    // The other shape in the record: the list closed by a full stop, with commentary
    // after it that carries the en-dash range `1–98`. A parser that ran past this
    // terminator would read ninety-eight open items out of a sentence that says the
    // opposite.
    let stopped = "prose. **Still open: 4-residual, 8, 15, 28, 31, 71, 73, **78(b)**, 79, 84, \
                   85, 91, plus **13** (measured, gating half carried) and **64(a)** (PARTLY). \
                   Everything else in 1\u{2013}98 is executed or closed as a recorded \
                   decline.** more prose";
    let got = parse_agents_open_list(stopped).expect("the full-stop shape parses");
    assert_eq!(
        got.numbers,
        [4, 8, 13, 15, 28, 31, 64, 71, 73, 78, 79, 84, 85, 91]
            .into_iter()
            .collect::<BTreeSet<u32>>(),
        "the full stop ends the list: {}",
        got.segment
    );
    assert!(
        !got.numbers.contains(&50),
        "`1–98` belongs to the commentary after the terminator and must not expand: {}",
        got.segment
    );
    assert_eq!(got.letters.get(&78), Some(&BTreeSet::from(['b'])));
    assert_eq!(
        got.letters.get(&13),
        None,
        "`**13** (measured, …)` names no clause: {}",
        got.segment
    );

    // The whole sentence being reworded is a loud failure, never a quiet empty read.
    assert!(parse_agents_open_list("no such sentence here").is_err());
    assert!(
        parse_agents_open_list("Still open: 1, 2, 3 and nothing else").is_err(),
        "a list with no em dash terminator must fail rather than run on into §2's prose"
    );
    let ran_on = "Still open: 4, 8, 12, 15, 22, 24, 25, 28, 30, 31, 84, 85 and 91, and \
                  80\u{2013}83 were executed 2026-08-16 \u{2014} the rest.";
    let err = parse_agents_open_list(ran_on)
        .unwrap_err_or_panic("a segment that swallowed the commentary must fail, not read 80..83");
    assert!(err.contains("runs past its terminator"), "{err}");
}

#[test]
fn the_comparison_names_every_disagreeing_item_with_its_line_and_its_words() {
    // Derived from the real ledger rather than a fixture, so the message this gate
    // would print is exercised on this tree's own bytes.
    let items = tree_ledger();
    let reconciled = |extra: &[u32], without: &[u32]| -> AgentsList {
        let mut numbers: BTreeSet<u32> = items
            .iter()
            .filter(|i| i.status.is_open())
            .map(|i| i.number)
            .collect();
        numbers.extend(extra.iter().copied());
        for n in without {
            numbers.remove(n);
        }
        let letters: BTreeMap<u32, BTreeSet<char>> = items
            .iter()
            .filter(|i| i.status.is_open() && !i.open_letters.is_empty())
            .map(|i| (i.number, i.open_letters.clone()))
            .collect();
        AgentsList {
            numbers,
            letters,
            segment: "(reconciled)".to_string(),
        }
    };
    assert!(
        drift_report(&items, &reconciled(&[], &[])).is_none(),
        "a list reconciled from the ledger itself must produce no drift"
    );

    // (a) an executed item wrongly listed as open.
    let executed = items
        .iter()
        .find(|i| i.status == Status::Executed)
        .expect("the ledger has executed items");
    let report = drift_report(&items, &reconciled(&[executed.number], &[]))
        .expect("adding an executed item must redden");
    assert!(
        report.contains(&format!(
            "item {:<3} plan:{:<5} EXECUTED",
            executed.number, executed.line
        )),
        "the report must name the item, its plan line and its derived status: {report}"
    );
    assert!(
        report.contains(&executed.quote),
        "the report must quote the words the classification came from: {report}"
    );

    // (b) an open item wrongly omitted.
    let open = items
        .iter()
        .find(|i| i.status.is_open())
        .expect("the ledger has open items");
    let report = drift_report(&items, &reconciled(&[], &[open.number]))
        .expect("dropping an open item must redden");
    assert!(
        report.contains("AGENTS §2 omits them"),
        "the other direction must be reported as its own class: {report}"
    );
    assert!(
        report.contains(&format!("item {:<3} plan:{:<5}", open.number, open.line)),
        "{report}"
    );

    // (c) a phantom number AGENTS names and the ledger does not have.
    let report =
        drift_report(&items, &reconciled(&[9999], &[])).expect("a phantom number must redden");
    assert!(
        report.contains("item 9999") && report.contains("no such entry"),
        "a number with no ledger entry must be called out as such: {report}"
    );

    // (d) the right item, the wrong clause. Both lists call it open, so a comparison
    //     on numbers alone reads this as agreement — which is what `78(a)` would have
    //     been before the letters were compared.
    if let Some(mixed) = items
        .iter()
        .find(|i| i.status.is_open() && !i.open_letters.is_empty())
    {
        let mut listed = reconciled(&[], &[]);
        let wrong: BTreeSet<char> = mixed
            .open_letters
            .iter()
            .map(|c| if *c == 'a' { 'b' } else { 'a' })
            .collect();
        listed.letters.insert(mixed.number, wrong.clone());
        let report =
            drift_report(&items, &listed).expect("naming the wrong open clause must redden");
        assert!(
            report.contains("disagree about *which clause*")
                && report.contains(&render_letters(&wrong)),
            "the report must name both readings of the clause: {report}"
        );
        println!(
            "LETTER item {:>3} plan says {}, AGENTS says {} -> RED",
            mixed.number,
            render_letters(&mixed.open_letters),
            render_letters(&wrong)
        );
    }
}

#[test]
fn an_owed_remainder_keeps_an_item_open_whatever_its_declaration_says() {
    // THE ONE THAT WAS WRONG IN THE TREE. Item 13's declaration read `**MEASURED
    // 2026-08-13** … **the gating half is carried**` while its live record carried
    // `*Remainder:* a Darwin per_fd_cpu_percent … row`. §18's Schema defines a
    // `Remainder` as *exactly what is owed*, so the item had owed work — and this gate
    // derived EXECUTED, went red, and told its reader that AGENTS §2 was the copy that
    // needed repairing. Following that instruction deletes the one genuinely-open item
    // from the list the gate exists to keep honest, which is worse than the drift it
    // was built to catch.
    let plan = tree_plan();
    let items = parse_ledger(&plan, SUPERSEDED_MARKERS).unwrap_or_else(|e| panic!("{e}"));

    // The two sets, derived rather than remembered: every entry carrying a `Remainder`
    // in its live region, and the subset carrying one *above its filing*.
    let mut live_carriers: Vec<u32> = Vec::new();
    let mut record_carriers: Vec<u32> = Vec::new();
    for item in &items {
        let entry = entry_text(&plan, item.number);
        let live = live_region_of(&entry, SUPERSEDED_MARKERS);
        if owed_remainder(live, REMAINDER_LABELS).is_some() {
            live_carriers.push(item.number);
        }
        if owed_remainder(record_region_of(live), REMAINDER_LABELS).is_some() {
            record_carriers.push(item.number);
        }
    }
    println!("REMAIN live-region carriers          : {live_carriers:?}");
    println!("REMAIN record carriers (rule's set)  : {record_carriers:?}");

    // The narrowing is the answer to "does this rule move anything it should not?", and
    // it is measured here rather than argued. Entries whose only `Remainder` sits
    // inside a preserved filing — items 49 and 50 preserve theirs with no marker phrase
    // at all, the filing simply beginning at `*Evidence:*` — record what was owed *when
    // they were filed*. They are executed with the work done, and the rule must leave
    // them alone.
    let filed_only: Vec<u32> = live_carriers
        .iter()
        .copied()
        .filter(|n| !record_carriers.contains(n))
        .collect();
    assert!(
        !filed_only.is_empty(),
        "no entry carries a `Remainder` only inside its filing, so cutting the live region at \
         the schema's filing labels is a narrowing nobody has seen do anything — measured on \
         the tree that landed this rule the set was {{49, 50}}"
    );
    let mut would_have_moved: Vec<u32> = Vec::new();
    for number in &filed_only {
        let item = items.iter().find(|i| i.number == *number).expect("parsed");
        assert!(
            item.remainder.is_none(),
            "item {number}'s `Remainder` is inside its filing and the rule read it anyway"
        );
        if !item.status.is_open() {
            would_have_moved.push(*number);
        }
    }
    // These are the entries the *unnarrowed* rule would have put on AGENTS §2's list:
    // executed, with the work done, and carrying the `Remainder` their original filing
    // was written with. If this set is empty the narrowing costs nothing and proves
    // nothing, and the rule should be widened back to the whole live region.
    assert!(
        !would_have_moved.is_empty(),
        "every filing-only carrier is open for its own reasons, so reading the whole live \
         region would have moved no classification and the narrowing is untested — on the tree \
         that landed this rule the set was {{49, 50}}"
    );
    println!(
        "REMAIN filing-only carriers {filed_only:?}; a rule reading the whole live region \
         would wrongly open {would_have_moved:?}"
    );
    assert!(
        !record_carriers.is_empty(),
        "no entry carries a live `Remainder` above its filing, so this rule has nothing to \
         stand under on this tree"
    );

    // And the rule fires. Each carrier's own open status is rewritten as a closed one —
    // item 13's `) open**` becomes `) EXECUTED**`, which is the shape its declaration
    // had before it was made explicit — and the item must stay open, decided by the
    // Remainder rather than by the declaration. With the rule switched off, the same
    // bytes close it: a mechanism that cannot be switched off cannot be shown to do
    // anything (AGENTS §3).
    for number in &record_carriers {
        let item = items.iter().find(|i| i.number == *number).expect("parsed");
        assert!(
            item.status.is_open(),
            "item {number} owes a `Remainder` and is classified {}",
            item.status.label()
        );
        let planted = plant_in_entry(
            &plan,
            *number,
            item.spelling,
            &closed_restatement(item.spelling),
        );

        let armed = parse_ledger(&planted, SUPERSEDED_MARKERS)
            .unwrap_or_else(|e| panic!("{e}"))
            .into_iter()
            .find(|i| i.number == *number)
            .expect("the entry is still there");
        assert!(
            armed.status.is_open(),
            "item {number}'s declaration was closed and its live `Remainder` did not hold it \
             open: {} {:?}",
            armed.status.label(),
            armed.quote
        );
        assert!(
            REMAINDER_LABELS.contains(&armed.spelling),
            "item {number} stayed open for some other reason than its Remainder: {:?}",
            armed.spelling
        );

        let disarmed = parse_ledger_with(
            &planted,
            SUPERSEDED_MARKERS,
            &[],
            SPELLINGS,
            Title::Stripped,
            Bound::FirstSentence,
            Continuation::Shipped,
        )
        .unwrap_or_else(|e| panic!("{e}"))
        .into_iter()
        .find(|i| i.number == *number)
        .expect("the entry is still there");
        assert!(
            !disarmed.status.is_open(),
            "with the Remainder rule switched off item {number} stayed open anyway, so the \
             rule is not what is holding it and this plant proves nothing"
        );
        println!(
            "REMAIN item {:>3} declaration closed by hand -> {} armed, {} disarmed",
            number,
            armed.status.label(),
            disarmed.status.label()
        );
    }
}

#[test]
fn the_superseded_tripwire_reports_a_refusal_only_it_can_produce() {
    // The tripwire and its whole marker table were INERT with respect to this suite:
    // replacing the marker lookup with an unconditional `return Ok(())` left every test
    // green. The test that claimed to prove it planted the marker *with a full stop*,
    // which ended the declaration before any status — so the classifier failed too, and
    // its error quotes the planted declaration back, putting the words "superseded
    // filing" in the message whether or not the tripwire ran. Rewording the refusal was
    // equally invisible.
    //
    // Two repairs, and both are needed. The plant now leaves a valid status behind it,
    // so with the tripwire disarmed the parse *succeeds* — nothing else in the parser
    // can produce a failure here. And the assertion is on [`SUPERSEDED_TRIPWIRE_TAG`],
    // which is written in one function and in no document, so no planted text can
    // satisfy it by being quoted back.
    let plan = tree_plan();
    let items = parse_ledger(&plan, SUPERSEDED_MARKERS).unwrap_or_else(|e| panic!("{e}"));
    let carrier = items
        .iter()
        .find(|i| i.status == Status::Executed && i.spelling == "EXECUTED")
        .expect("the ledger has an entry decided by `EXECUTED`");

    // AGENTS §3: plant the violation in every spelling the table claims to cover.
    for marker in SUPERSEDED_MARKERS {
        let planted = plant_in_entry(
            &plan,
            carrier.number,
            carrier.spelling,
            &format!("{marker} and {}", carrier.spelling),
        );
        let err = parse_ledger(&planted, SUPERSEDED_MARKERS).unwrap_err_or_panic(&format!(
            "{marker:?} planted in item {}'s declaration was not seen",
            carrier.number
        ));
        assert!(
            err.contains(SUPERSEDED_TRIPWIRE_TAG),
            "the refusal must be the tripwire's own, not a bystander quoting the plant back: \
             {err}"
        );
        assert!(
            err.contains(&format!("item {}", carrier.number)) && err.contains(marker),
            "the refusal must name the entry and the phrase it found: {err}"
        );

        let disarmed = parse_ledger(&planted, &[]).unwrap_or_else(|e| {
            panic!(
                "with the marker table empty the planted declaration must still classify — if \
                 it does not, the plant is failing for a second reason and the assertion above \
                 proves nothing about the tripwire.\n{e}"
            )
        });
        let after = disarmed
            .into_iter()
            .find(|i| i.number == carrier.number)
            .expect("the carrier is still there");
        assert_eq!(
            after.status,
            Status::Executed,
            "with the tripwire disarmed the planted entry must read exactly as it did before"
        );
        println!("TRIP   {marker:<32} -> refused, and green with the table empty");
    }
}

#[test]
fn a_tail_entry_that_stops_looking_like_a_head_is_still_counted_by_the_floor() {
    // The floor exists to see a head the *walker* cannot, so every way a head can stop
    // looking like one has to be inside its reach. It demanded column-zero digits
    // followed by `". "` — two of the walker's own three requirements — so two of the
    // three shapes were invisible to it. Each of these left `parse_ledger` returning Ok
    // with 97 items on the real ledger, contiguous from 1, and the last entry gone.
    let full = synthetic_ledger(&[]);
    let head = format!("\n{FIXTURE_ITEMS}. **Filler item {FIXTURE_ITEMS}**");
    let shapes = [
        (
            "indented two spaces",
            format!("\n  {FIXTURE_ITEMS}. **Filler item {FIXTURE_ITEMS}**"),
        ),
        (
            "written `**NN.**`",
            format!("\n**{FIXTURE_ITEMS}.** Filler item {FIXTURE_ITEMS}"),
        ),
        (
            "no longer bold",
            format!("\n{FIXTURE_ITEMS}. Filler item, unbolded."),
        ),
    ];
    for (what, rewritten) in shapes {
        let lost = full.replace(&head, &rewritten);
        assert_ne!(
            full, lost,
            "the fixture's tail entry must be there to lose ({what})"
        );
        let err = parse_ledger(&lost, SUPERSEDED_MARKERS)
            .expect_err(&format!("a tail head {what} must not vanish silently"));
        assert!(
            err.contains(&format!("missing [{FIXTURE_ITEMS}]")),
            "the error must name the head the walker did not reach ({what}): {err}"
        );
        println!("TAIL   {what:<20} -> refused");
    }
}

#[test]
fn the_floor_scan_runs_past_the_closing_heading() {
    // The scan for the highest head number runs to the end of the file rather than
    // stopping at LEDGER_END, and this is the case that says why rather than the
    // comment that used to. A closing heading appearing early truncates the walk by
    // exactly as much as it would truncate a bounded floor, and the two errors cancel
    // into a short parse that is contiguous from 1 over everything it did read.
    let full = synthetic_ledger(&[]);
    let cut = full.find("\n52. ").expect("filler item 52 exists");
    let early = format!(
        "{}\n\n{LEDGER_END} — an early close\n{}",
        &full[..cut],
        &full[cut..]
    );
    let lines: Vec<&str> = early.lines().collect();
    let (start, end) = ledger_line_range(&lines).expect("the section is found");
    assert_eq!(
        highest_head_number_in(&lines[start..end]),
        51,
        "a floor bounded by the closing heading agrees with the truncated walk, which is the \
         cancellation this scan exists to avoid"
    );
    assert_eq!(
        highest_head_number(&lines, start),
        FIXTURE_ITEMS,
        "the unbounded scan must see the heads that live past the early heading"
    );
    let err = parse_ledger(&early, SUPERSEDED_MARKERS)
        .expect_err("a ledger closed early must not parse as a short contiguous one");
    assert!(err.contains(&format!("not 1..={FIXTURE_ITEMS}")), "{err}");
    println!("FLOOR  early `{LEDGER_END}` -> walk 51, floor {FIXTURE_ITEMS}, refused");
}

#[test]
fn the_floors_matcher_reads_this_trees_own_highest_head_and_nothing_beyond_it() {
    // The two comments that used to carry this were **frozen numbers**: each said the
    // loose matcher's highest reading on this document "is still 98", measured on the
    // tree that landed the gate. The ledger is append-only, so a number measured once is
    // a number that goes stale on the next filing — and both had, by three items, while
    // still reading as a present-tense measurement. They say what is measured and this
    // says what it comes to, which is the only arrangement that cannot drift.
    //
    // Two properties, and the second is why the scan deliberately runs past
    // [`LEDGER_END`] (see [`the_floor_scan_runs_past_the_closing_heading`] for the
    // cancellation it avoids): the looser eye must cost the floor no precision, in
    // either direction. Both are read off a tree they hold on, so both get a planted
    // copy that they must *not* hold on — otherwise this test's passing output is
    // identical to its not-running output, which is the tell AGENTS §3 names.
    let plan = tree_plan();
    let (walked, floor, beyond) = floor_reading(&plan);
    assert_eq!(
        floor, walked,
        "the floor's matcher and the walker disagree about this document's highest head. That \
         is not necessarily a defect — the floor exists to see a head the walker cannot — but \
         it means one of them is reading something the other is not, and the parse will refuse \
         with `not 1..=N` until it is understood."
    );
    assert!(
        beyond.is_empty(),
        "the floor's scan runs to the end of the file on purpose, and something past `{}` now \
         reads as a head number above the ledger's own highest ({walked}): {beyond:?} (line, \
         number). Every parse will refuse with `not 1..=N`. Either that prose needs rewriting \
         or the scan needs a bound that is not the closing heading — which is the one bound it \
         cannot have.",
        LEDGER_END,
    );

    // (a) The agreement, planted apart. Indenting the tail entry's head by two spaces is
    //     one of the three shapes that lost an entry in silence: the walker stops seeing
    //     it and the floor does not, which is the whole point of the floor having a
    //     looser eye than the walker.
    let indented = plant_in_entry(
        &plan,
        walked,
        &format!("{walked}. **"),
        &format!("  {walked}. **"),
    );
    let (walked_after, floor_after, _) = floor_reading(&indented);
    assert!(
        walked_after < walked && floor_after == floor,
        "with item {walked}'s head indented, the walker must lose it ({walked_after}) while the \
         floor keeps it ({floor_after}) — if both move together, the two matchers are one \
         matcher and the agreement above is an identity"
    );

    // (b) The tail, planted past the closing heading. A head number above the ledger's
    //     own highest, written where the walk does not reach, refuses every parse — the
    //     hazard the scan's deliberate unboundedness carries, stated as an assertion
    //     that can fire rather than as a sentence promising it will not.
    let phantom = plan.replacen(
        LEDGER_END,
        &format!(
            "{LEDGER_END}\n\n{}. **A head past the closing heading**\n",
            walked + 900
        ),
        1,
    );
    let (_, _, beyond_after) = floor_reading(&phantom);
    assert_eq!(
        beyond_after.iter().map(|&(_, n)| n).collect::<Vec<u32>>(),
        vec![walked + 900],
        "a head written past `{LEDGER_END}` must be seen by the scan, or the reason it runs \
         past that heading is a reason nobody has watched work"
    );

    println!(
        "FLOOR  walker reads {walked} as its highest head and the loose matcher agrees; \
         nothing above it past `{LEDGER_END}`. Planted: head indented -> walker {walked_after}, \
         floor {floor_after}; head past the heading -> {beyond_after:?}"
    );
}

/// What the walker and the floor make of one plan: `(the walker's highest head, the
/// floor's highest reading, every head the floor reads past [`LEDGER_END`] that is above
/// the walker's highest)`.
fn floor_reading(plan: &str) -> (u32, u32, Vec<(usize, u32)>) {
    let lines: Vec<&str> = plan.lines().collect();
    let (start, end) = ledger_line_range(&lines).expect("plan §18 is present");
    let walked = (start..end)
        .filter_map(|i| head_number(lines[i]))
        .max()
        .expect("plan §18 has item heads");
    let beyond = lines[end..]
        .iter()
        .enumerate()
        .filter_map(|(k, l)| loose_head_number(l).map(|n| (end + k + 1, n)))
        .filter(|&(_, n)| n > walked)
        .collect();
    (walked, highest_head_number(&lines, start), beyond)
}

#[test]
fn word_matching_needs_both_of_its_edges_and_each_has_a_case_only_it_rejects() {
    // `before_ok && after_ok` survived the deletion of either conjunct, because the one
    // fixture that reached it was symmetric — `no_{spelling}_here`, an underscore on
    // both sides, which either half alone rejects. These are asymmetric: each is
    // rejected by exactly one edge and would match with that edge deleted.
    assert_eq!(
        find_spelling("`EXECUTED`", "EXECUTED", Match::Word),
        Some(1),
        "non-word neighbours on both sides must match"
    );
    assert_eq!(
        find_spelling("UNEXECUTED`", "EXECUTED", Match::Word),
        None,
        "only the *left* edge forbids this; with `before_ok` deleted it matches"
    );
    assert_eq!(
        find_spelling("`EXECUTEDLY", "EXECUTED", Match::Word),
        None,
        "only the *right* edge forbids this; with `after_ok` deleted it matches"
    );
    // ...and a rejected occurrence does not stop the walk, or the anchoring would turn
    // a near-miss into a blind spot for the legitimate spelling behind it.
    assert_eq!(
        find_spelling("UNEXECUTED and `EXECUTED`", "EXECUTED", Match::Word),
        Some(16),
        "the search must continue past an occurrence its edges rejected"
    );
}

#[test]
fn an_enumeration_joined_by_and_is_one_clause_group_and_a_statement_is_two() {
    // `clause_groups`' `|| between == "and"` conjunct had no case of its own. This is
    // it, and it reaches the real ledger: item 64 spells its executed half
    // `(b), (c), (d), (e), (f), (g) and (h)`. Without the conjunct, `(h)` becomes a
    // statement of its own carrying no status, with none stated ahead of it, and the
    // entry stops parsing — so the conjunct is what keeps a real enumeration readable
    // rather than a nicety.
    let joined = clause_groups("(b), (c) and (d) EXECUTED**; **(a) PARTLY");
    assert_eq!(
        joined.len(),
        2,
        "`(b), (c) and (d)` is one enumeration and `(a)` is a second statement"
    );
    assert_eq!(joined[0].letters, vec!['b', 'c', 'd']);
    assert_eq!(joined[1].letters, vec!['a']);

    let split = clause_groups("(a) EXECUTED 2026-08-16 and (b) still owed");
    assert_eq!(
        split.len(),
        2,
        "a clause with words of its own between it and the next is not an enumeration"
    );

    // The two shapes are read differently end to end, which is what makes the
    // distinction worth having.
    assert_eq!(
        status_of("**A title** — **(b), (c) and (d) EXECUTED**; **(a) open** (S)."),
        Status::Partly
    );
    assert_eq!(
        status_of("**A title** — **EXECUTED**: (a) and (b) landed 2026-08-12."),
        Status::Executed
    );
}

#[test]
fn the_agents_list_is_read_by_its_shape_and_reads_through_a_bracketed_aside() {
    // (a) THE SAME THREE NUMBERS, READ TWO WAYS — and this pair is what the count floor
    //     that stood here could not express (plan §18 item 115). The floor refused any
    //     list of fewer than ten numbers, so it refused the first of these for the same
    //     reason it refused the second, and the first is a **correct sentence**: a
    //     ledger with three items open writes three numbers. It went from a hypothetical
    //     to a red suite the day items 73 and 85 closed.
    //
    //     The property is the shape, not the size. A three-entry list parses...
    let short = "prose. **Still open: 4, 8, 15 \u{2014} the rest of it.** more";
    let got = parse_agents_open_list(short)
        .unwrap_or_else(|e| panic!("a three-entry list is a list and must parse: {e}"));
    assert_eq!(
        got.numbers,
        [4, 8, 15].into_iter().collect::<BTreeSet<u32>>(),
        "a short list is a short list: {}",
        got.segment
    );

    //     ...and the same three numbers, left behind by a stray full stop with the rest
    //     of the list on the other side of it, are refused — with the text past the
    //     terminator quoted, which is the thing that says the read was a prefix. Neither
    //     of these two sentences can be told from the other by counting, and the pair is
    //     the whole argument for the replacement.
    let truncated = "prose. **Still open: 4, 8, 15. 28, 31 and 71. The rest of it.** more";
    let err = parse_agents_open_list(truncated)
        .unwrap_err_or_panic("a list split by a stray full stop must be refused");
    assert!(
        err.contains("continues past") && err.contains("28, 31 and 71"),
        "the refusal must quote the list it found past the terminator: {err}"
    );

    // (b) An aside *inside* the list, which is how §2 writes it: `plus **13(b)** (the
    //     gating half, owed — its declaration was made explicit …)`. A depth-blind
    //     terminator takes that em dash for the list's end, truncates one entry early
    //     and drops `64(a)` — reporting drift against a §2 sentence that is right. And
    //     a depth-blind number scan reads the date inside the aside as three more open
    //     items.
    let aside = "prose. **Still open: 4-residual, 8, 15, 28, 31, 71, 73, **78(b)**, 79, 84, 85, \
                 91, plus **13(b)** (the gating half, owed \u{2014} its declaration was made \
                 explicit 2026-08-21 because the gate read it as closed) and **64(a)** \
                 (PARTLY). Everything else in 1\u{2013}98 is executed.** more prose";
    let got = parse_agents_open_list(aside).expect("an aside must not terminate the list");
    assert_eq!(
        got.numbers,
        [4, 8, 13, 15, 28, 31, 64, 71, 73, 78, 79, 84, 85, 91]
            .into_iter()
            .collect::<BTreeSet<u32>>(),
        "the list must be read past its own aside and stop at the full stop: {}",
        got.segment
    );
    assert!(
        !got.numbers.contains(&2026) && !got.numbers.contains(&21) && !got.numbers.contains(&98),
        "digits inside brackets are commentary, and `1–98` is past the terminator: {}",
        got.segment
    );
    assert_eq!(got.letters.get(&13), Some(&BTreeSet::from(['b'])));
    assert_eq!(got.letters.get(&64), Some(&BTreeSet::from(['a'])));
}

#[test]
fn the_agents_list_is_read_at_any_size_the_ledger_can_reach() {
    // THE PROPERTY THE FLOOR GOT WRONG (plan §18 item 115).
    //
    // `parse_agents_open_list` used to refuse any §2 list carrying fewer than **ten**
    // numbers — "below the floor a real list clears". That constant is a count of how
    // much work happens to be open, which makes it the third instance in this file of
    // one class: a guard whose green depends on work being unfinished. The first two
    // were the `PARTLY` liveness row and the mixture-shape battery, both split into a
    // parser claim (asserted by construction, every run) and a document claim (reported,
    // may legitimately go empty). This is the same split, one level down.
    //
    // The way it announced itself is worth the sentence. Executing items 73 and 85 took
    // the derived open set from eleven numbers to nine, and this file went to
    // `24 passed · 3 failed` — not because the ledger drifted, but because both plant
    // batteries build a **reconciled** AGENTS copy from the derived list and assert it
    // green as their control, and a nine-number control did not parse. So the batteries
    // panicked before planting anything: three tests reporting a defect that was the
    // completion of two items. The session before this one put the count back to ten by
    // filing a real ledger entry and said so in the entry — an arithmetic coincidence,
    // not a repair, and the next two items this ledger closes re-trip it.
    //
    // What is asserted here is the parser claim, at the sizes the ledger will actually
    // reach as the remaining items close: **one**, three, and the nine that tripped it.
    let agents = tree_agents();

    // (a) The sentence, read on its own.
    for open in [1u32, 3, 9] {
        let numbers: BTreeSet<u32> = (7..7 + open).collect();
        let sentence = format!(
            "prose. **Still open: {}. Everything else is closed or declined.** more prose",
            numbers
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        let got = parse_agents_open_list(&sentence)
            .unwrap_or_else(|e| panic!("a {open}-entry list is a list and must parse: {e}"));
        assert_eq!(
            got.numbers, numbers,
            "a {open}-entry list must read as its own {open} entries: {}",
            got.segment
        );
    }

    // (b) The whole gate, end to end, against the **real** AGENTS.md — its own
    //     terminator, its own commentary after it, its own surrounding paragraph — with
    //     a ledger that has almost nothing open. This is the case the floor refused: the
    //     reconciled copy is exactly what [`agents_listing`] builds for the two plant
    //     batteries, so a size the parser will not read is a size at which those
    //     batteries cannot run their controls, whatever they are planting.
    for open in [1u32, 3, 9] {
        let entries: Vec<(u32, &str)> = (7..7 + open)
            .map(|n| (n, "**A title** \u{2014} **open** (S)."))
            .collect();
        let plan = synthetic_ledger(&entries);
        let items = parse_ledger(&plan, SUPERSEDED_MARKERS).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            items.iter().filter(|i| i.status.is_open()).count(),
            open as usize,
            "the fixture must have exactly {open} open item(s) or it is not the case under test"
        );
        let reconciled = agents_listing(&agents, &items, &BTreeSet::new());
        let outcome = gate(&plan, &reconciled);
        assert!(
            matches!(outcome, Outcome::Green),
            "a ledger with {open} open item(s), reconciled into the real AGENTS.md, must be \
             green — the gate compares two lists and neither of them has a minimum size: {}",
            outcome.message()
        );
        println!("SIZE   {open:>2} open item(s) -> reconciled real AGENTS.md -> GREEN");
    }

    // (c) The endpoint, stated because it is where the old constant's reasoning ran out.
    //     A §2 list with nothing on it parses to an **empty set** rather than being
    //     refused for being small. Whether an empty list is *right* is a judgement about
    //     the two documents, and it belongs to the comparison and to
    //     [`agents_2s_still_open_list_matches_plan_18s_item_states`]'s own anti-vacuity
    //     assertions — which still refuse to compare two sets they enumerated to zero.
    //     The parser's job is to say what the sentence says.
    let empty = "prose. **Still open: . Everything else is closed.** more prose";
    let got = parse_agents_open_list(empty).expect("an empty list is a list");
    assert!(
        got.numbers.is_empty(),
        "an empty list must read as no items, not as a refusal: {}",
        got.segment
    );

    // (d) THE DOCUMENT CLAIM, reported rather than asserted, which is the other half of
    //     the split. How many numbers §2 carries today is a fact about the schedule; the
    //     gate above is what holds it to plan §18, and nothing here may fail because that
    //     number got smaller.
    match parse_agents_open_list(&agents) {
        Ok(live) => println!(
            "LIVE   AGENTS §2's list carries {} number(s) today: {:?}",
            live.numbers.len(),
            live.numbers
        ),
        Err(e) => println!("LIVE   AGENTS §2's list does not parse today: {e}"),
    }
}

#[test]
fn a_truncated_or_reshaped_still_open_sentence_is_refused_whatever_its_length() {
    // The hazard the floor was reaching for, kept and re-aimed. If `Still open:` ever
    // matches a **different** sentence, or the terminator search stops **inside** the
    // list, this parser reads a set of numbers that is not §2's list — and if the plan's
    // derived set happened to be the same set, the comparison passes for the wrong
    // reason. That hazard is real and does not go away because the constant guarding it
    // was wrong.
    //
    // What replaces it are three structural claims, none of which counts anything:
    // the literal is spelled once, the segment reads as a list, and the list does not
    // carry on past the terminator. Each fixture below carries **ten or more** depth-0
    // numbers deliberately — the floor would have waved every one of them through, so a
    // fixture that happened to be short would be proving the old constant instead of the
    // new rule.
    let cases: &[(&str, String, &str)] = &[
        (
            "a stray full stop splits the list in two, and the parser stops at it",
            "prose. **Still open: 4-residual, 8, 13(b), 15, 28, 31, 71, 111, 112 and 115. \
             92, 93, 94 and 98. Everything else is closed.** more prose"
                .to_string(),
            "continues past",
        ),
        (
            "prose has grown into the list, and its dates read as item numbers",
            "prose. **Still open: after the rig session of 2026-08-21 the remaining work is 4, \
             8, 13, 15, 28, 31, 71, 111, 112 and 115. Everything else is closed.** more prose"
                .to_string(),
            "no list connective accounts for",
        ),
        (
            "the literal is spelled twice, and this parser takes the first",
            "§2 used to read `Still open: 4-residual, 8, 12, 13, 15, 28, 31, 71, 84, 85 and 91`, \
             and it now reads: **Still open: 4, 8, 13, 15, 28, 31, 71, 111, 112 and 115. \
             Everything else is closed.** more prose"
                .to_string(),
            "twice",
        ),
        (
            "a range spelled with an ASCII hyphen, which reads as two unrelated items",
            "prose. **Still open: 4-residual, 8, 12-20, 28, 31, 71, 111, 112 and 115. \
             Everything else is closed.** more prose"
                .to_string(),
            "hyphen-joined",
        ),
        (
            "the list ends on its own connective, one entry having been deleted",
            "prose. **Still open: 4-residual, 8, 13, 15, 28, 31, 71, 111, 112, 115 and. \
             Everything else is closed.** more prose"
                .to_string(),
            "ends on",
        ),
        (
            "the list opens on a connective, one entry having been deleted from its head",
            "prose. **Still open: plus 8, 13, 15, 28, 31, 71, 84, 85, 111, 112 and 115. \
             Everything else is closed.** more prose"
                .to_string(),
            "opens on",
        ),
    ];
    for (what, sentence, wanted) in cases {
        let numbers = parse_agents_open_list(sentence)
            .map(|l| l.numbers.len())
            .unwrap_or(0);
        let err = parse_agents_open_list(sentence).unwrap_err_or_panic(&format!(
            "{what}: this sentence parsed to {numbers} number(s) rather than being refused. \
             That is the false green the floor was aiming at, and the floor would have missed \
             it too — every fixture here clears ten numbers.\nSentence: {sentence}"
        ));
        assert!(
            err.contains(wanted),
            "{what}: refused, but not by the rule that should have refused it (wanted \
             {wanted:?}):\n{err}"
        );
        println!("SHAPE  {what:<66} -> REFUSED");
    }
}

// ---------------------------------------------------------------------------
// The plants, run against reconciled copies of the two real documents
// ---------------------------------------------------------------------------

/// AGENTS.md with its `Still open:` segment replaced by the ledger's own open set,
/// clause letters included.
///
/// The copies are in memory rather than on disk deliberately: the tree is the thing
/// under audit and a gate that rewrites it to test itself is a worse hazard than the
/// drift it is looking for.
fn agents_listing(agents: &str, items: &[Item], extra: &BTreeSet<u32>) -> String {
    let listed = parse_agents_open_list(agents).expect("the real sentence parses");
    let by_number: BTreeMap<u32, &Item> = items.iter().map(|i| (i.number, i)).collect();
    let mut numbers: BTreeSet<u32> = items
        .iter()
        .filter(|i| i.status.is_open())
        .map(|i| i.number)
        .collect();
    numbers.extend(extra.iter().copied());
    let replacement = format!(
        " {} ",
        numbers
            .iter()
            .map(|n| match by_number.get(n) {
                Some(item) if !item.open_letters.is_empty() =>
                    format!("{n}{}", render_letters(&item.open_letters)),
                _ => n.to_string(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    );
    agents.replacen(&listed.segment, &replacement, 1)
}

/// A spelling with its words replaced and its punctuation kept.
///
/// The plant has to change what a clause *says* without changing the shape it says it
/// in: item 78's open clause is spelled `) open**`, and simply deleting that token
/// takes the `)` of `(b)` with it — which removes the clause marker as well as the
/// status and tests a different thing entirely.
fn reworded(spelling: &str) -> String {
    let mut out = String::new();
    let mut in_word = false;
    for c in spelling.chars() {
        if c.is_alphanumeric() {
            if !in_word {
                out.push_str("unstated");
                in_word = true;
            }
        } else {
            out.push(c);
            in_word = false;
        }
    }
    out
}

/// A spelling with its words replaced by a *closed* one and its punctuation kept.
///
/// The mirror of [`reworded`], and it exists for the same reason: item 13's open clause
/// is spelled `) open**`, so a plant that does not keep the `)` takes the `(b)` marker
/// with it and tests something else.
fn closed_restatement(spelling: &str) -> String {
    let mut out = String::new();
    let mut in_word = false;
    for c in spelling.chars() {
        if c.is_alphanumeric() {
            if !in_word {
                out.push_str("EXECUTED");
                in_word = true;
            }
        } else {
            out.push(c);
            in_word = false;
        }
    }
    out
}

/// Replace the first occurrence of `token` inside item `number`'s entry.
fn plant_in_entry(plan: &str, number: u32, token: &str, with: &str) -> String {
    let lines: Vec<&str> = plan.lines().collect();
    let (head, stop) = entry_line_range(&lines, number);
    let at = (head..stop)
        .find(|&i| lines[i].contains(token))
        .unwrap_or_else(|| panic!("item {number} does not spell {token:?} on any of its lines"));
    let mut out: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    out[at] = lines[at].replacen(token, with, 1);
    out.join("\n")
}

/// What the whole gate says about one pair of documents.
enum Outcome {
    Green,
    Drift(String),
    Refused(String),
}

impl Outcome {
    fn message(&self) -> &str {
        match self {
            Outcome::Green => "",
            Outcome::Drift(m) | Outcome::Refused(m) => m,
        }
    }
}

fn gate(plan: &str, agents: &str) -> Outcome {
    let items = match parse_ledger(plan, SUPERSEDED_MARKERS) {
        Ok(items) => items,
        Err(e) => return Outcome::Refused(e),
    };
    let listed = match parse_agents_open_list(agents) {
        Ok(listed) => listed,
        Err(e) => return Outcome::Refused(e),
    };
    match drift_report(&items, &listed) {
        Some(report) => Outcome::Drift(report),
        None => Outcome::Green,
    }
}

/// `Result::unwrap_err`, but the panic says which plant went unnoticed.
trait UnwrapErrOrPanic<T> {
    fn unwrap_err_or_panic(self, what: &str) -> String;
}

impl<T> UnwrapErrOrPanic<T> for Result<T, String> {
    fn unwrap_err_or_panic(self, what: &str) -> String {
        match self {
            Ok(_) => panic!("{what}"),
            Err(e) => e,
        }
    }
}

/// Which region of an entry a status spelling was found in.
///
/// The order is the order the regions nest, and the report reads it as a verdict: a
/// [`Region::Declaration`] sighting of a spelling the classifier did not record is a
/// parser defect, a [`Region::Preserved`] one is a verbatim record AGENTS §5 forbids
/// rewriting, and the two in between are the ledger's own live prose about the item.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Region {
    /// Inside the entry's opening statement, as [`declaration_of`] cuts it.
    Declaration,
    /// Inside the entry's own bolded title, which [`status_region_of`] strips.
    Title,
    /// In the live prose above the entry's filing, as [`record_region_of`] cuts it.
    Record,
    /// Inside the filing itself — from `**State:**` or `*Evidence` onward.
    Filing,
    /// Past one of [`SUPERSEDED_MARKERS`], in a preserved record of an earlier filing.
    Preserved,
}

impl Region {
    fn label(self) -> &'static str {
        match self {
            Region::Declaration => "IN ITS STATUS DECLARATION",
            Region::Title => "in its title",
            Region::Record => "CHECK: in its live record, above its filing",
            Region::Filing => "in its filing",
            Region::Preserved => "in a preserved record, past a superseded marker",
        }
    }
}

/// One place plan §18 still spells a status word: which entry, and where inside it.
#[derive(Clone, Copy, Debug)]
struct Sighting {
    number: u32,
    region: Region,
}

/// The `Match` kind the shipped vocabulary matches `spelling` under.
///
/// Read from the table rather than passed in, because a sighting search that used a
/// looser rule than the classifier's would find occurrences the classifier could never
/// have matched — and then report a row as founded on text that could not found it.
fn match_kind(spelling: &str) -> Match {
    SPELLINGS
        .tables()
        .iter()
        .flat_map(|(_, table)| table.iter())
        .find(|&&(s, _)| s == spelling)
        .map(|&(_, kind)| kind)
        .unwrap_or_else(|| panic!("{spelling:?} is not a row of the shipped vocabulary"))
}

/// Every entry of plan §18 that spells `spelling`, with the region it lands in.
///
/// The first occurrence in each entry decides that entry's region, which is enough for
/// a report and is deliberately not enough for anything else: this function is read by
/// the liveness block above to say *why* a row is quiet, and by no gate to decide a
/// status. The regions are tested by containment rather than by offset arithmetic
/// because [`strip_size_marker`] does not return a prefix slice of what it was handed,
/// so an offset comparison would be right for most entries and quietly wrong for items
/// 32–44.
fn sightings_of(plan: &str, items: &[Item], spelling: &'static str) -> Vec<Sighting> {
    let kind = match_kind(spelling);
    let mut out = Vec::new();
    for item in items {
        let entry = entry_text(plan, item.number);
        if find_spelling(&entry, spelling, kind).is_none() {
            continue;
        }
        let after_title = status_region_of(&entry, Title::Stripped);
        let declaration = declaration_of(after_title, Bound::FirstSentence, Continuation::Shipped);
        let live = live_region_of(&entry, SUPERSEDED_MARKERS);
        let record = record_region_of(live);
        let region = if find_spelling(declaration, spelling, kind).is_some() {
            Region::Declaration
        } else if find_spelling(after_title, spelling, kind).is_none() {
            Region::Title
        } else if find_spelling(record, spelling, kind).is_some() {
            Region::Record
        } else if find_spelling(live, spelling, kind).is_some() {
            Region::Filing
        } else {
            Region::Preserved
        };
        out.push(Sighting {
            number: item.number,
            region,
        });
    }
    out
}

fn render_sightings(sightings: &[Sighting]) -> String {
    sightings
        .iter()
        .map(|s| format!("item {} ({})", s.number, s.region.label()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Which side of the derived open set a planted status must put its entry on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Side {
    Open,
    Closed,
}

/// Plant one status spelling over item `number`'s `token`, and require the classifier to
/// have **read** it and the entry to have moved to the side the spelling says.
///
/// **The assertion this replaces was weaker than the comment above it claimed, and its
/// own fail-first sweep is what found that out** (AGENTS §3's fifth register, and the one
/// this file already carries two instances of). It read
/// `assert!(!matches!(outcome, Outcome::Green))` — the gate reddened — under a comment
/// promising that the entry "must take that entry off the derived list". Those are not
/// the same claim: [`Outcome::Refused`] satisfies the first and denies the second, so a
/// plant that left the declaration *unreadable* passed exactly as a plant the classifier
/// read would. For a spelling the ledger still uses that gap is covered by accident,
/// because making the classifier blind to it also breaks the unplanted parse. For a
/// **quiet** spelling nothing covers it at all — and quiet spellings are the case this
/// block exists to carry, since the liveness assertion that used to stand above it was
/// split precisely so a row could go quiet without the gate going red.
///
/// Measured on this tree, with the classifier made blind to one row at a time inside
/// [`spelling_hits`]: ten of the eleven rows reddened the gate anyway — seven through
/// "declares no status this gate recognises" on the unplanted document, two through the
/// straddle tripwire in the liveness block — and **`PARTLY`, the one row plan §18 no
/// longer spells, stayed green**. Every plant below passed with the matcher for it
/// switched off.
///
/// So three things are asserted where one was. The parse must **succeed**, because a
/// refusal means the spelling was never reached. The spelling must appear in the entry's
/// own `matched` set, which is the classifier saying in its own words that it read this
/// text. And the entry must land on `side`, which is the spelling's meaning rather than
/// merely its presence. The gate is then required to report drift naming the item, so the
/// whole path — parse, classify, compare, report — is what moved.
fn planted_status_is_read_and_moves_the_item(
    plan: &str,
    reconciled: &str,
    number: u32,
    token: &str,
    replacement: &str,
    spelling: &'static str,
    side: Side,
) {
    let planted = plant_in_entry(plan, number, token, replacement);
    let items = parse_ledger(&planted, SUPERSEDED_MARKERS).unwrap_or_else(|e| {
        panic!(
            "planting {spelling:?} over item {number}'s {token:?} left plan §18 unparseable, so \
             the spelling was never read. A refusal reddens this gate and proves nothing about \
             the matcher — that is the hole this helper exists to close, so it is a failure here \
             rather than a pass.\n{e}"
        )
    });
    let after = items
        .iter()
        .find(|i| i.number == number)
        .expect("the planted entry is still there");
    assert!(
        after.matched.contains(spelling),
        "item {number}'s declaration was rewritten to spell {spelling:?} and the classifier did \
         not record it as matching. It recorded {:?}. The row is not being read where it was \
         planted, whatever the gate goes on to say about the entry.",
        after.matched
    );
    assert_eq!(
        after.status.is_open(),
        side == Side::Open,
        "item {number} was rewritten to spell {spelling:?} and reads {} — the spelling was \
         matched but did not decide the entry's side. Deciding text: {:?}",
        after.status.label(),
        after.quote
    );
    let outcome = gate(&planted, reconciled);
    assert!(
        matches!(outcome, Outcome::Drift(_)),
        "planting {spelling:?} over item {number} moved the entry to {:?} and the gate did not \
         report drift against the reconciled copy: {}",
        side,
        outcome.message()
    );
    assert!(
        outcome.message().contains(&format!("item {number}")),
        "the drift report must name item {number}: {}",
        outcome.message()
    );
}

#[test]
fn planted_drift_reddens_in_every_spelling_and_a_superseded_filing_does_not() {
    let root = repo_root();
    let plan = read_tree_file(&normative_plan_path(&root));
    let agents = read_tree_file(&root.join("AGENTS.md"));
    let items = parse_ledger(&plan, SUPERSEDED_MARKERS).unwrap_or_else(|e| panic!("{e}"));
    let reconciled = agents_listing(&agents, &items, &BTreeSet::new());

    // (0) LIVENESS — one assertion until plan §18 item 101's session, and it was two
    //     claims wearing one `assert!`. What stood here demanded that every row of the
    //     status vocabulary match some entry's **live status declaration**, on the
    //     reasoning that "a row that matches nowhere cannot be shown to work". The
    //     reasoning is right and the conclusion overreached, because the row's ability to
    //     work is a property of the *parser* while its presence in a declaration is a
    //     property of *today's document* — and the second one moves every time an item is
    //     executed. Closing items 64(a) and 78(b) took the ledger's only `PARTLY`
    //     declaration with them and this assertion went red. It was correct on its own
    //     terms and useless as an instruction: the repair it named was "delete the row",
    //     which drops a disposition §18 has used three times (items 31, 64, 78), which
    //     [`classify`] still implements, and which the ledger may write again the next
    //     time a clause lands half-done. **Closing work must not break a gate**, and a
    //     parser branch with no test is what this file exists to refuse. Both at once is
    //     the shape the fused assertion could not express.
    //
    //     So the two claims are separated, and each is asserted where it can actually be
    //     held:
    //
    //     * **the matcher works** — asserted for *every* row, quiet or not, by the plants
    //       in (a), (a′) and (a″) below. Each writes the spelling into a real entry of
    //       the real ledger — its own title, size marker, line wraps, siblings and
    //       superseded filing all in place — and requires the whole gate, through
    //       [`gate`], to move. That is **stronger** than what the fused assertion proved
    //       and it does not depend on which items happen to be open, which is why no
    //       synthetic ledger fixture is built here: [`synthetic_ledger`] exists for the
    //       walker's structural cases, and a hand-written entry would be a stimulus
    //       gentler than the document (AGENTS §3's sixth register) in exactly the places
    //       a status declaration is hard to parse. The three loops' coverage of the table
    //       is itself asserted at the end of this test rather than left to the reader's
    //       eye, so a sixth table cannot be added without a plant;
    //     * **the ledger still writes it** — a statement about the document, which may
    //       legitimately go empty as work is executed. It is **reported**, naming the
    //       item and the region of the entry the word survives in, rather than failed.
    //
    //     A report is a no-op unless something holds it up, and two assertions do. First,
    //     a quiet row must be **founded**: the ledger has to spell it somewhere in §18,
    //     under the row's own [`Match`] kind. That is the failure the original assertion
    //     was really guarding — a speculative row somebody adds because they believe the
    //     document uses a word it has never used — and it survives every execution,
    //     because executing an item does not unwrite its record. `PARTLY` is founded on
    //     item 64's preserved `*Original head declaration:*`; an invented row is founded
    //     nowhere and fails here naming itself. Second, a quiet row must not be found in
    //     any entry's **declaration**: that is the other branch of the sentence this
    //     replaces — "the parser stopped reaching the text that does" — in the one form a
    //     gate can actually decide. [`classify`] records every spelling that matched any
    //     statement, and the statements partition the declaration, so a spelling present
    //     in a declaration and absent from `matched` means an occurrence straddled a
    //     statement cut. Nothing on this tree does that; the assertion exists so that the
    //     day one does, it is a named refusal rather than a row that has quietly gone
    //     quiet.
    //
    //     **Two things it deliberately does not claim, both because the sentence it
    //     replaces over-claimed in the same place.** Founding is *not* the test that
    //     retired `**Open**`: that row's two instances sat inside superseded filings, so
    //     it was founded by this rule and still correctly deleted — by a person who read
    //     *where* the two instances were. The report is what puts that reading in front of
    //     the next person, with the region named, instead of leaving them to grep. And a
    //     quiet row sighted in an entry's **live record** is not asserted against, though
    //     it is the sighting worth a second look: the ledger is free to use these words in
    //     prose about an item, so a rule refusing that would fail on legitimate writing.
    //     It is printed with a `CHECK` marker for the reader instead of being decided
    //     here, and that is a limit, not a subtlety.
    let live: BTreeSet<&'static str> = items
        .iter()
        .flat_map(|i| i.matched.iter().copied())
        .collect();
    let every: Vec<&'static str> = SPELLINGS.every_spelling();
    let quiet: Vec<&'static str> = every
        .iter()
        .copied()
        .filter(|s| !live.contains(s))
        .collect();
    let sightings: BTreeMap<&'static str, Vec<Sighting>> = quiet
        .iter()
        .map(|s| (*s, sightings_of(&plan, &items, s)))
        .collect();

    let unfounded: Vec<&'static str> = quiet
        .iter()
        .copied()
        .filter(|s| sightings[s].is_empty())
        .collect();
    assert!(
        unfounded.is_empty(),
        "these spelling(s) decide no entry's status in plan §18 **and are written nowhere in \
         it**: {unfounded:?}.\n\
         A row the ledger has never spelled is a claim about the document that the document \
         does not support: delete it, or cite the entry that uses it. (A row the ledger *has* \
         spelled and no longer needs is a different case and is reported rather than failed — \
         see the QUIET lines this test prints. Executing an item does not unwrite its record, \
         so a genuinely-used spelling stays founded after its last live use closes.)"
    );

    let in_a_declaration: Vec<(&'static str, u32)> = quiet
        .iter()
        .flat_map(|s| {
            sightings[s]
                .iter()
                .filter(|sight| sight.region == Region::Declaration)
                .map(move |sight| (*s, sight.number))
        })
        .collect();
    assert!(
        in_a_declaration.is_empty(),
        "these spelling(s) are written inside an entry's own status declaration and yet matched \
         nothing the classifier recorded: {in_a_declaration:?} (spelling, item).\n\
         `classify` records every spelling that matches any statement, and the statements \
         partition the declaration, so this can only mean the occurrence straddles a statement \
         cut — a clause-group start or a top-level `;`. That is the parser failing to reach text \
         it is standing on, which is the half of `a row that matches nowhere` that is never the \
         ledger's fault."
    );

    for spelling in &quiet {
        println!(
            "QUIET  {spelling:<30} decides nothing in plan §18 today; still written at {}",
            render_sightings(&sightings[spelling])
        );
    }
    println!(
        "LIVE   {} of {} spellings match a live declaration; {} quiet and founded: {quiet:?}",
        live.len(),
        every.len(),
        quiet.len()
    );

    // The control. Everything below is measured against this being green, so it is
    // asserted rather than assumed.
    assert!(
        matches!(gate(&plan, &reconciled), Outcome::Green),
        "the reconciled control is not green, so no plant below proves anything: {}",
        gate(&plan, &reconciled).message()
    );
    let open_count = items.iter().filter(|i| i.status.is_open()).count();
    println!("CONTROL   reconciled copies -> GREEN ({open_count} open items)");

    // Which rows of the vocabulary the three plant loops below actually reach. It is
    // accumulated rather than reasoned about, and asserted equal to the whole table at
    // the end of this test: the loops are keyed on the five *tables*, so a sixth table
    // — or a row moved between two of them — would leave a spelling with no plant and
    // nothing here would say so. That claim used to be carried by the liveness
    // assertion, which reached every row only because the ledger happened to spell every
    // row; this reaches every row because the table has it.
    let mut planted_spellings: BTreeSet<&'static str> = BTreeSet::new();

    // (a) One plant per executed spelling: written over a real *open* entry's status,
    //     each must take that entry off the derived list, which AGENTS' reconciled copy
    //     still carries.
    //
    //     This used to look for an executed entry *decided by* each spelling and flip it
    //     to open. That shape stopped being satisfiable — and the reason is worth the
    //     comment, because it is the mechanism under test working: `**MEASURED` decides
    //     nothing any more, since item 13, its only user in the whole ledger, now
    //     declares `**MEASURED …**, and **(b) open**` and is therefore decided by its
    //     *open* half. The spelling is still live (it is in `matched`, reported above)
    //     and still has to be shown to close an entry, which is what this plants.
    // The host carries no live `Remainder`, and that filter is the backstop's own
    // shadow: an entry the schema rule holds open cannot be closed by editing its
    // declaration, so planting a closed status over item 28's `**open**` correctly
    // leaves the gate green and would look like a dead plant.
    let open_host = items
        .iter()
        .find(|i| i.status == Status::Open && i.spelling == "**open**" && i.remainder.is_none())
        .expect("the ledger has an entry decided by `**open**` with no live Remainder under it");
    for &(spelling, _) in EXECUTED_SPELLINGS {
        let closed = if spelling.starts_with("**") {
            format!("{spelling} 2026-08-21**")
        } else {
            format!("**{spelling} 2026-08-21**")
        };
        planted_status_is_read_and_moves_the_item(
            &plan,
            &reconciled,
            open_host.number,
            "**open**",
            &closed,
            spelling,
            Side::Closed,
        );
        planted_spellings.insert(spelling);
        println!(
            "PLANT (a) item {:>3} executed spelling {:<12} -> RED   (read, and left the derived list)",
            open_host.number, spelling
        );
    }

    // (a′) The other side of the matcher: every *open* and *partly* spelling, planted
    //      into a real executed entry in place of its status.
    let host = items
        .iter()
        .find(|i| i.status == Status::Executed && i.spelling == "EXECUTED")
        .expect("the ledger has an EXECUTED-spelled entry");
    for &(spelling, _) in OPEN_SPELLINGS.iter().chain(PARTLY_SPELLINGS) {
        planted_status_is_read_and_moves_the_item(
            &plan,
            &reconciled,
            host.number,
            "EXECUTED",
            spelling,
            spelling,
            Side::Open,
        );
        planted_spellings.insert(spelling);
        println!(
            "PLANT (a') item {:>3} open spelling {:<16} -> RED   (read, and joined the derived list)",
            host.number, spelling
        );
    }

    // (a″) The spellings that *close* an item — a decline, or a clause carried to a
    //      successor: replace a real open entry's status with each of them and the
    //      entry must leave the derived set, which AGENTS' reconciled copy still lists.
    for &(spelling, _) in DECLINED_SPELLINGS.iter().chain(CARRIED_SPELLINGS) {
        planted_status_is_read_and_moves_the_item(
            &plan,
            &reconciled,
            open_host.number,
            "**open**",
            &format!("**{spelling}**"),
            spelling,
            Side::Closed,
        );
        planted_spellings.insert(spelling);
        println!(
            "PLANT (a\") item {:>3} closing spelling {:<30} -> RED   (read, and left the derived list)",
            open_host.number, spelling
        );
    }

    // (b) The other direction: a number AGENTS names that the ledger does not have
    //     open.
    let phantom = agents_listing(&agents, &items, &BTreeSet::from([9999]));
    let outcome = gate(&plan, &phantom);
    assert!(
        outcome.message().contains("item 9999") && outcome.message().contains("no such entry"),
        "{}",
        outcome.message()
    );
    println!("PLANT (b) phantom item 9999 in AGENTS' list -> RED");

    // (c) THE NEGATIVE PLANT. `**State:** open (S).` inserted into a real executed
    //     entry's *superseded* block must leave the gate green. Without this, a gate
    //     that reads the whole entry passes for the wrong reason.
    let marker = "The superseded filing follows.";
    let carrier = items
        .iter()
        .find(|i| {
            if i.status != Status::Executed {
                return false;
            }
            let lines: Vec<&str> = plan.lines().collect();
            let (head, stop) = entry_line_range(&lines, i.number);
            lines[head..stop].iter().any(|l| l.contains(marker))
        })
        .expect("some executed entry carries a superseded filing on one line");
    let planted = plant_in_entry(
        &plan,
        carrier.number,
        marker,
        &format!("{marker} **State:** open (S)."),
    );
    let after = parse_ledger(&planted, SUPERSEDED_MARKERS)
        .unwrap_or_else(|e| panic!("{e}"))
        .into_iter()
        .find(|i| i.number == carrier.number)
        .expect("the carrier is still there");
    assert_eq!(
        after.status,
        Status::Executed,
        "item {} was reopened by text inside its superseded filing",
        carrier.number
    );
    assert!(
        matches!(gate(&planted, &reconciled), Outcome::Green),
        "the negative plant reddened the gate: superseded text is being read as a live status"
    );
    println!(
        "PLANT (c) item {:>3} `**State:** open (S).` INSIDE its superseded block -> GREEN",
        carrier.number
    );

    // ...and the same bytes in the entry's declaration must redden, or (c) proves
    // nothing about where the status is read from.
    let ahead = plant_in_entry(
        &plan,
        carrier.number,
        carrier.spelling,
        "**State:** open (S).",
    );
    assert!(
        !matches!(gate(&ahead, &reconciled), Outcome::Green),
        "the same text in the declaration must redden, or the negative plant is vacuous"
    );
    println!(
        "PLANT (c') item {:>3} the same bytes in its DECLARATION       -> RED",
        carrier.number
    );

    // (d) THE COVERAGE CLAIM, asserted rather than read off the three loops above by eye.
    //     Every row of the vocabulary — the quiet ones included, which is the whole point
    //     of splitting the liveness block at the top of this test — has been written into
    //     a real entry of the real ledger and shown to move the gate. Nothing else in this
    //     file proves a spelling *works*; the liveness report proves only that the ledger
    //     still spells it, and the fixtures in
    //     [`every_status_spelling_the_ledger_uses_is_recognised_in_its_own_spelling`] prove
    //     it against a synthesised entry rather than end to end. So this is the assertion
    //     that has to hold when a row goes quiet, and it holds for a reason that has
    //     nothing to do with which items are open.
    let covered: BTreeSet<&'static str> = SPELLINGS.every_spelling().into_iter().collect();
    assert_eq!(
        planted_spellings,
        covered,
        "the plants above reached {} of the vocabulary's {} row(s). Every row must be planted \
         into a real entry and shown to move this gate, whether or not plan §18 currently spells \
         it — the three loops are keyed on the five tables, so a sixth table, or a row moved \
         between two of them, leaves a spelling with no plant at all.\n\
         Planted: {planted_spellings:?}\nIn the table: {covered:?}",
        planted_spellings.len(),
        covered.len(),
    );
    println!(
        "PLANT (d) every one of the {} vocabulary row(s) planted into a real entry -> RED",
        covered.len()
    );
}
