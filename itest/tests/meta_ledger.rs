#![forbid(unsafe_code)]

//! **The ledger-parity meta-gate** (plan §18 item 95; AGENTS §3's derive-from-tools rule).
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
//! [`the_verdict_survives_a_punctuation_only_rewrite_of_a_mixture_on_this_tree`]
//! rewrites the tree's own mixture and requires the answer *not* to. Without that
//! trio, a mechanism that never fires passes for the wrong reason on a reconciled tree
//! — AGENTS §3's tell, "its passing output is identical to its not-running output".
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
/// that matches nowhere is a claim nobody checks — see the liveness assertion in
/// [`planted_drift_reddens_in_every_spelling_and_a_superseded_filing_does_not`].
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
/// EXECUTED. Punctuation alone must not move a verdict, in **either** of the shapes the
/// ledger writes a mixture in, which is what
/// [`the_verdict_survives_a_punctuation_only_rewrite_of_a_mixture_on_this_tree`] asserts
/// over every mixed entry rather than over one spelling of one of them.
///
/// The continuation test is keyed on the declaration's **statement structure** — see
/// [`continues_with_a_clause_statement`] — and not on the emphasis of what follows,
/// which is what confined the first repair to item 64.
fn declaration_of(region: &str, bound: Bound) -> &str {
    if bound == Bound::WholeEntry {
        return region;
    }
    let mut from = 0usize;
    loop {
        let Some((at, after)) = first_sentence_end(region, from) else {
            return region;
        };
        if continues_with_a_clause_statement(region, at, after) {
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
/// Emphasis on its own still decides nothing, and narrowness is still the reason: three
/// real entries (55, 64 and 65) open their *next paragraph* with a clause marker —
/// `(a) \`daemon.rs\`'s verb enumeration…`, `*(a) The remedy is refuted…*`, `*(c) the
/// \`deaf.py\` orphan is fixed…*` — and every one of those sentence ends sits **outside**
/// every bold run, which is what keeps whole paragraphs of record out of the
/// declaration. Widening by emphasis instead was measured on this tree, one step at a
/// time: accept a leading single `*` and items **64 and 65** stop parsing; drop the
/// emphasis condition altogether and **55** joins them, its paragraph carrying no
/// emphasis at all. The rule shipped here refuses none of the 98 and moves no verdict.
/// That is why the widening goes through the bold run the sentence *ends in* rather
/// than through the emphasis of what follows it.
fn continues_with_a_clause_statement(region: &str, at: usize, after: usize) -> bool {
    let rest = region[after..].trim_start();
    let b = rest.trim_start_matches('*').as_bytes();
    let opens_a_clause = b.len() >= 3 && b[0] == b'(' && b[1].is_ascii_lowercase() && b[2] == b')';
    opens_a_clause && (ends_inside_a_bold_run(region, at) || rest.starts_with("**"))
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
/// `markers`, `remainders`, `spellings`, `title` and `bound` are parameters only so the
/// tests can switch one mechanism off at a time and require the answer to change;
/// production callers use [`parse_ledger`].
fn parse_ledger_with(
    plan: &str,
    markers: &[&str],
    remainders: &[&'static str],
    spellings: Spellings<'_>,
    title: Title,
    bound: Bound,
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
        let declaration = declaration_of(region, bound);
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
/// item 98's head by two spaces, or writing it `**98.** The pattern-…`, each left
/// `parse_ledger` returning `Ok` with 97 contiguous items, no error, and the last entry
/// silently gone. Leading whitespace and `**` on either side of the dot are tolerated,
/// and [`a_tail_entry_that_stops_looking_like_a_head_is_still_counted_by_the_floor`]
/// plants all three shapes.
///
/// **What this matcher reaches is not "every way an entry can stop looking like an
/// entry", and that absolute — which stood here — is retired rather than restated.**
/// The reach is: an optional indent, an optional `**`, the digits, a `.`, an optional
/// `**`, and then a space or end of line. Outside it, each of `98) **Title**` (valid
/// CommonMark for an ordered list), `- 98. **Title**`, `*98.* **Title**` and
/// `98.**Title**` still returns `None`, so a tail entry written any of those four ways
/// is lost by the walker *and* uncounted by the floor — `Ok`, contiguous from 1, one
/// entry gone. Named here because a hole a reader can see is worth more than an
/// absolute that reads as a guarantee.
///
/// The plausible one, `98)`, was **not** taken, and the reason is measured on this very
/// document rather than argued: accepting `)` beside `.` makes this matcher read plan
/// §18 item 66's wrapped CI run id — a line beginning `31689537882) and it moved the
/// failure one stage forward…` — as a head, which puts the floor at 31689537882 and
/// refuses every parse with `not 1..=31689537882`. A looser eye buys nothing once it is
/// loose enough to read prose as structure; if `98)` is ever wanted, it needs a
/// condition that separates a head from a wrapped number, not another separator byte.
///
/// Measured against the whole file from §18's heading to EOF with the matcher as it
/// stands: the highest number it finds is still 98 and it finds nothing at all past
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
/// fixture and reads both answers. Measured on this tree with the matcher
/// [`loose_head_number`] actually uses: the highest number it finds anywhere from §18's
/// heading to EOF is 98, and it finds nothing at all after the closing heading, so the
/// wider scan costs the floor no precision.
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

    // (number, byte offset of its first digit, byte offset just past its last digit).
    // Digits inside brackets are commentary, never list entries: §2's own aside
    // `(the gating half, owed — its declaration was made explicit 2026-08-21 …)` would
    // otherwise put items 2026, 8 and 21 on the derived list.
    let bytes = segment.as_bytes();
    let mut spans: Vec<(u32, usize, usize)> = Vec::new();
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
                    let n: u32 = segment[s..i].parse().map_err(|e| {
                        format!(
                            "AGENTS §2 list: unparseable number {:?}: {e}",
                            &segment[s..i]
                        )
                    })?;
                    spans.push((n, s, i));
                }
            }
            _ => i += 1,
        }
    }
    if spans.len() < 10 {
        return Err(format!(
            "AGENTS §2's `{AGENTS_LIST_LEAD}` list parsed only {} numbers, which is below the \
             floor a real list clears — the sentence's shape has changed under the parser. \
             Segment: {segment}",
            spans.len()
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
    let letters_only = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
    };
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
              punctuation only (in {} spellings)",
        punctuation_only.len()
    );
}

#[test]
fn the_verdict_survives_a_punctuation_only_rewrite_of_a_mixture_on_this_tree() {
    // Shape (4) again, against the tree's own bytes rather than a transcription — and
    // over EVERY mixture the ledger spells that way rather than the one a literal
    // picked out. The victim used to be selected by searching the entry's lines for
    // `**; **(`, which **only item 64 spells**: item 78 writes its two halves inside
    // one bold run (`**(a) EXECUTED 2026-08-16; (b) open**`), so the guard could never
    // reach the entry a one-byte edit closes in silence. Coverage keyed on one
    // entry's spelling is coverage of one entry.
    //
    // So the separator is derived from each mixed entry's own declaration — the `;`
    // that runs into the next clause marker with nothing but emphasis and spaces
    // between them — and rewritten as a full stop, which is the whole edit. The
    // reader's eye cannot see it; the parser must not either.
    let plan = tree_plan();
    let items = parse_ledger(&plan, SUPERSEDED_MARKERS).unwrap_or_else(|e| panic!("{e}"));

    let victims: Vec<(&Item, String)> = items
        .iter()
        .filter(|i| i.status == Status::Partly)
        .filter_map(|i| {
            let entry = entry_text(&plan, i.number);
            let declaration = declaration_of(
                status_region_of(&entry, Title::Stripped),
                Bound::FirstSentence,
            );
            clause_separator(declaration).map(|separator| (i, separator))
        })
        .collect();

    // Two-sided by assertion, not by hope. The two spellings differ in exactly the
    // thing the old continuation rule keyed on: whether the halves are two bold runs
    // (`; **(a)`) or one shared run (`; (b)`). A guard that reached only the first is
    // the one that let item 78 stay open to a one-byte edit.
    let shares_one_run = victims.iter().any(|(_, sep)| !sep.contains('*'));
    let spans_two_runs = victims.iter().any(|(_, sep)| sep.contains("**"));
    assert!(
        victims.len() >= 2 && shares_one_run && spans_two_runs,
        "this guard reached {} of plan §18's mixtures ({:?}) and must reach both shapes — a \
         mixture whose halves share one bold run and one whose halves are two runs. Selecting \
         victims by a literal that only one entry spells is how item 78 went uncovered.",
        victims.len(),
        victims
            .iter()
            .map(|(i, sep)| (i.number, sep.as_str()))
            .collect::<Vec<_>>()
    );

    for (victim, separator) in &victims {
        // Same reason as the fixture's: two empty letter sets agree with each other.
        assert!(
            !victim.open_letters.is_empty(),
            "item {} is a mixture that names no open clause, so the comparison below would \
             hold whatever the plant did",
            victim.number
        );
        let sentence_end = format!(".{}", &separator[1..]);
        let planted = plant_in_entry(&plan, victim.number, separator, &sentence_end);
        let after = parse_ledger(&planted, SUPERSEDED_MARKERS)
            .unwrap_or_else(|e| panic!("item {}: {e}", victim.number))
            .into_iter()
            .find(|i| i.number == victim.number)
            .expect("the victim is still there");
        assert_eq!(
            after.status, victim.status,
            "item {}'s verdict moved on a punctuation-only rewrite of {separator:?}",
            victim.number
        );
        assert_eq!(
            after.open_letters, victim.open_letters,
            "item {}'s open clause moved on a punctuation-only rewrite of {separator:?}",
            victim.number
        );
        println!(
            "PUNCT  item {:>3} {separator:?} -> {sentence_end:?} : {} {} unchanged",
            victim.number,
            after.status.label(),
            render_letters(&after.open_letters)
        );
    }
}

/// The `;` that separates one clause statement from the next, with everything up to
/// that next clause marker — `; **(a)` where the halves are two bold runs, `; (b)`
/// where they share one.
///
/// Returned verbatim so a plant can rewrite the `;` as a `.` and change nothing else.
/// Only a run of emphasis and spaces may sit between the two: a `;` with words behind
/// it is prose continuing a sentence, not a statement separator, and item 13's
/// `**MEASURED …**, and **(b) open**` is deliberately not reached — its separator is a
/// comma plus a word, which is not a punctuation-only edit away from a sentence end.
fn clause_separator(declaration: &str) -> Option<String> {
    let b = declaration.as_bytes();
    for (at, c) in declaration.char_indices() {
        if c != ';' {
            continue;
        }
        let mut j = at + 1;
        while j < b.len() && matches!(b[j], b' ' | b'*') {
            j += 1;
        }
        if j + 2 < b.len() && b[j] == b'(' && b[j + 1].is_ascii_lowercase() && b[j + 2] == b')' {
            return Some(declaration[at..j + 3].to_string());
        }
    }
    None
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
    assert!(
        items.iter().any(|i| i.status == Status::Partly),
        "plan §18 declares no mixed entry at all, so the clause-completeness rule has nothing \
         to bite on this tree — if the ledger really has closed every one of them, say so here \
         and keep the fixtures above; do not leave this assertion passing on an empty set"
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
fn the_agents_list_has_a_floor_and_reads_through_a_bracketed_aside() {
    // (a) The floor. A sentence whose shape has changed under the parser reads a
    //     handful of numbers rather than none, so "empty" is not the failure to guard
    //     against — the assertion in the gate covers that, and this covers the rest.
    let short = "prose. **Still open: 4, 8, 15 \u{2014} the rest.** more";
    let err = parse_agents_open_list(short)
        .unwrap_err_or_panic("a three-number list must fail the floor");
    assert!(err.contains("below the floor"), "{err}");

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

#[test]
fn planted_drift_reddens_in_every_spelling_and_a_superseded_filing_does_not() {
    let root = repo_root();
    let plan = read_tree_file(&normative_plan_path(&root));
    let agents = read_tree_file(&root.join("AGENTS.md"));
    let items = parse_ledger(&plan, SUPERSEDED_MARKERS).unwrap_or_else(|e| panic!("{e}"));
    let reconciled = agents_listing(&agents, &items, &BTreeSet::new());

    // (0) LIVENESS, for every table rather than for one of them. A spelling the ledger
    //     no longer uses is a claim nobody is checking: it can go dead by a one-word
    //     edit and every plant below would keep passing on the entries that still use
    //     its siblings. `**Open**` was deleted from OPEN_SPELLINGS on exactly this
    //     evidence — its only two instances in §18 sit inside superseded filings, past
    //     every declaration.
    let live: BTreeSet<&'static str> = items
        .iter()
        .flat_map(|i| i.matched.iter().copied())
        .collect();
    let dead: Vec<&'static str> = SPELLINGS
        .every_spelling()
        .into_iter()
        .filter(|s| !live.contains(s))
        .collect();
    assert!(
        dead.is_empty(),
        "these spelling(s) match no entry's status declaration in plan §18: {dead:?}. Either \
         the ledger stopped using them — in which case delete the row, since a row that matches \
         nowhere cannot be shown to work — or the parser stopped reaching the text that does."
    );
    println!("LIVE   {} spellings, all matched in plan §18", live.len());

    // The control. Everything below is measured against this being green, so it is
    // asserted rather than assumed.
    assert!(
        matches!(gate(&plan, &reconciled), Outcome::Green),
        "the reconciled control is not green, so no plant below proves anything: {}",
        gate(&plan, &reconciled).message()
    );
    let open_count = items.iter().filter(|i| i.status.is_open()).count();
    println!("CONTROL   reconciled copies -> GREEN ({open_count} open items)");

    // (a) One plant per executed spelling: written over a real *open* entry's status,
    //     each must take that entry off the derived list, which AGENTS' reconciled copy
    //     still carries.
    //
    //     This used to look for an executed entry *decided by* each spelling and flip it
    //     to open. That shape stopped being satisfiable — and the reason is worth the
    //     comment, because it is the mechanism under test working: `**MEASURED` decides
    //     nothing any more, since item 13, its only user in the whole ledger, now
    //     declares `**MEASURED …**, and **(b) open**` and is therefore decided by its
    //     *open* half. The spelling is still live (it is in `matched`, asserted above)
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
        let planted = plant_in_entry(&plan, open_host.number, "**open**", &closed);
        let outcome = gate(&planted, &reconciled);
        assert!(
            !matches!(outcome, Outcome::Green),
            "closing item {} with {spelling:?} did not redden",
            open_host.number
        );
        assert!(
            outcome
                .message()
                .contains(&format!("item {}", open_host.number)),
            "the report must name the closed item {}: {}",
            open_host.number,
            outcome.message()
        );
        println!(
            "PLANT (a) item {:>3} executed spelling {:<12} -> RED   (left the derived list)",
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
        let planted = plant_in_entry(&plan, host.number, "EXECUTED", spelling);
        let outcome = gate(&planted, &reconciled);
        assert!(
            !matches!(outcome, Outcome::Green),
            "planting the open spelling {spelling:?} into item {} did not redden",
            host.number
        );
        assert!(
            outcome.message().contains(&format!("item {}", host.number)),
            "{}",
            outcome.message()
        );
        println!(
            "PLANT (a') item {:>3} open spelling {:<16} -> RED",
            host.number, spelling
        );
    }

    // (a″) The spellings that *close* an item — a decline, or a clause carried to a
    //      successor: replace a real open entry's status with each of them and the
    //      entry must leave the derived set, which AGENTS' reconciled copy still lists.
    for &(spelling, _) in DECLINED_SPELLINGS.iter().chain(CARRIED_SPELLINGS) {
        let planted = plant_in_entry(
            &plan,
            open_host.number,
            "**open**",
            &format!("**{spelling}**"),
        );
        let outcome = gate(&planted, &reconciled);
        assert!(
            !matches!(outcome, Outcome::Green),
            "closing item {} with {spelling:?} did not redden",
            open_host.number
        );
        assert!(
            outcome
                .message()
                .contains(&format!("item {}", open_host.number)),
            "{}",
            outcome.message()
        );
        println!(
            "PLANT (a\") item {:>3} closing spelling {:<30} -> RED",
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
}
