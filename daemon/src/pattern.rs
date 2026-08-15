//! The pattern wait's matcher: compiled patterns, the bounded lookback window, and
//! the maxima that bound both (design §10 *The pattern wait*, §15.56; plan §18 item
//! 47).
//!
//! This module is the *pure* half of `tap.wait` — everything that can be decided
//! without the graph, the hub, or the runtime. It compiles one to
//! [`MAX_PATTERNS`] named byte patterns into one linear-time automaton, and it owns
//! the rolling window those patterns are matched against. The impure half — arming
//! inside the hub's critical section, parking on the runtime thread, the teardown
//! sweep — is [`crate::tap`] and [`crate::daemon`].
//!
//! **Why a daemon-side matcher exists at all** (§15.56): everything *below* one
//! already did — offset-stamped taps with exact-splice replay, the waiting-verb
//! machinery, the ring — so every client that watches a console re-derived the same
//! matcher with the same subtle bugs: reassembly across `tap.data` boundaries,
//! replay-splice handling, gap discipline, and timeout-versus-teardown
//! discrimination. Encoding those rules once is §16.1's move applied to the
//! observation surface.
//!
//! **Three properties this module exists to hold.**
//!
//! 1. **Linear time, always.** Patterns compile and match on the runtime thread —
//!    the thread that runs every console — so a pattern whose match cost is
//!    exponential in the haystack is an operator-reachable denial of service.
//!    [`Matcher::compile`] therefore builds a `regex-automata` *meta* automaton,
//!    whose every search strategy is linear in the haystack (its one backtracking
//!    strategy is the *bounded* backtracker, which memoizes a visited set precisely
//!    so it stays linear and refuses any input that would not — never the
//!    exponential kind), and caps the compiled program at [`MAX_COMPILED_BYTES`] so
//!    the *build* is bounded too. §15.56 records the decision and the declines.
//!
//! 2. **Bytes, never text.** Console output is not guaranteed UTF-8 (§10), so both
//!    pattern kinds are byte-oriented: a [`PatternKind::Literal`] is an arbitrary
//!    byte string, and a [`PatternKind::Regex`] is compiled with Unicode mode off
//!    and UTF-8 validity *not* assumed of the haystack. A literal is escaped into
//!    the same automaton rather than matched by a second engine, so there is one
//!    engine, one set of limits, and one leftmost rule to reason about.
//!
//! 3. **Every dimension has a stated maximum, checked before anything is armed**
//!    (§16.12, §10 clause 2). Pattern count, pattern length, name length, compiled
//!    size, the lookback window, the context width and the deadline are all capped,
//!    and [`WaitSpec::parse`]-time refusal is what keeps an operator input from
//!    choosing the daemon's allocation and scan cost. Two of the seven are *reused*
//!    rather than re-derived — a name is a wire identifier ([`MAX_NAME_LEN`]) and a
//!    deadline is a timer ([`MAX_TIMER_MS`]) — because §16.12's rule is that the
//!    maximum exists, not that each surface mints its own.

use regex_automata::meta;
use regex_automata::util::syntax;
use serial_nexus_core::config::MAX_TIMER_MS;
use serial_nexus_core::graph::MAX_NAME_LEN;

/// The most patterns one `tap.wait` may carry (§10 clause 2: "one to eight named
/// patterns"). Small on purpose: the alternation is compiled into one automaton, so
/// the cap bounds compile time and program size together, and a caller that wants
/// more opens a second connection — which costs nothing (§15.20 clause 5).
pub const MAX_PATTERNS: usize = 8;

/// The most bytes one pattern may carry — the literal's bytes, or the regex's
/// source. Generous for a console expectation ("login:", a prompt regex) and far
/// below anything that could make the compile itself the cost.
pub const MAX_PATTERN_BYTES: usize = 1024;

/// The compiled-program ceiling handed to the engine's builder (§10 clause 3:
/// "bounded compile size"). A pattern that would exceed it is *refused at compile*
/// rather than accepted and paid for at match time, so a counted-repetition bomb
/// (`(?:a{1000}){1000}`) is an error message instead of a memory spike on the
/// thread that runs every console.
pub const MAX_COMPILED_BYTES: usize = 1 << 20;

/// The most bytes of already-seen stream a match may span — the lookback window
/// (§10 clause 4). This is the one dimension that is *also* a per-chunk scan cost,
/// not merely an allocation: the window is re-scanned each time the hub ingests, so
/// the cap bounds both.
pub const MAX_LOOKBACK: usize = 65_536;

/// The lookback used when the caller names none. Comfortably wider than any console
/// line, narrow enough that re-scanning it per ingested chunk is noise beside the
/// read that produced the chunk.
pub const DEFAULT_LOOKBACK: usize = 4096;

/// The smallest lookback that can honour §10 clause 4 for a given pattern set: the
/// longest **literal**'s length.
///
/// §16.12's rule is about maxima, and a maximum is what an operator input needs to
/// stop being a cost. This is the other end, and it exists for a different reason: a
/// window shorter than a pattern cannot hold that pattern, so the clause-4 promise
/// that "a pattern split across data-frame boundaries matches **by construction**"
/// would be silently false — the verb would answer `timed_out: true`, with
/// `gaps: 0` asserting the scan was whole, for a string that was on the wire.
/// Refusing is the only answer that does not lie. `lookback: 0` is refused outright
/// for the same reason.
///
/// Exact for literals; for a regex the daemon cannot know the longest match its
/// grammar admits (`a{1000}` is a six-byte source), so a regex contributes only the
/// floor of 1 and the docs tell the caller to size the window for the match they
/// expect. A heuristic that guessed would be worse than a stated limit.
pub fn min_lookback(specs: &[PatternSpec]) -> usize {
    specs
        .iter()
        .filter(|s| s.kind == PatternKind::Literal)
        .map(|s| s.bytes.len())
        .max()
        .unwrap_or(1)
        .max(1)
}

/// The most bytes of surrounding context a match may report (§10 clause 6).
pub const MAX_CONTEXT: usize = 4096;

/// The context width used when the caller names none.
pub const DEFAULT_CONTEXT: usize = 128;

/// Which of the two pattern grammars §10 clause 2 admits a pattern is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternKind {
    /// An arbitrary byte string, matched verbatim. Escaped into the shared automaton
    /// byte-by-byte, so a literal may contain any byte — including ones that are
    /// regex metacharacters, and ones that are not valid UTF-8.
    Literal,
    /// A bytes-oriented regular expression. Its *source* is text (regex syntax is),
    /// but it matches over bytes: Unicode mode is off and the haystack is never
    /// assumed to be UTF-8, so `\xNN` escapes reach any byte and `.` means "any
    /// byte but newline" rather than "any scalar value".
    Regex,
}

impl PatternKind {
    /// The wire spelling, which is also what the error messages quote.
    pub fn as_str(self) -> &'static str {
        match self {
            PatternKind::Literal => "literal",
            PatternKind::Regex => "regex",
        }
    }
}

/// One named pattern as it arrived on the wire, before compilation.
#[derive(Debug, Clone)]
pub struct PatternSpec {
    /// The caller's name for this pattern — what the result reports fired, so a
    /// caller matching eight patterns learns *which* without re-deriving it.
    pub name: String,
    pub kind: PatternKind,
    /// The literal's bytes, or the regex's source bytes. Carried as bytes in both
    /// cases because both ride the wire base64-encoded (§10 clause 2): the daemon
    /// never assumes the caller's pattern is text, and only [`PatternKind::Regex`]
    /// requires it to be.
    pub bytes: Vec<u8>,
}

/// Why a `tap.wait` was refused before anything was armed. Every variant is a
/// caller error (`-32602`), and every message names the offending dimension *and*
/// its maximum, because §16.12's rule is worth nothing if the refusal does not tell
/// the caller what to shrink.
#[derive(Debug)]
pub enum PatternError {
    /// Zero patterns, or more than [`MAX_PATTERNS`].
    Count(usize),
    /// A pattern name longer than [`MAX_NAME_LEN`], or empty.
    NameLen { name_len: usize },
    /// Two patterns share a name — the result would not say which fired.
    DuplicateName(String),
    /// A pattern longer than [`MAX_PATTERN_BYTES`].
    PatternLen { name: String, len: usize },
    /// A pattern with no bytes at all. Split from [`PatternError::PatternLen`]
    /// because sharing it produced the message `pattern "p" is 0 bytes, over the
    /// 1024-byte maximum` — a refusal whose stated reason is the opposite of the
    /// truth, which is worse than no reason.
    PatternEmpty { name: String },
    /// A [`PatternKind::Regex`] whose source bytes are not valid UTF-8. The regex
    /// *grammar* is text even though what it matches is not; a caller wanting an
    /// arbitrary byte writes `\xNN`, which is why this is a refusal rather than a
    /// lossy conversion.
    RegexNotUtf8 { name: String },
    /// The regex did not parse, or would not compile within [`MAX_COMPILED_BYTES`].
    Compile { name: Option<String>, why: String },
    /// A numeric dimension outside its stated maximum.
    Range {
        field: &'static str,
        value: u64,
        max: u64,
    },
    /// A lookback window too small to hold the patterns it is meant to match across
    /// a frame boundary — see [`min_lookback`].
    LookbackTooSmall { value: usize, min: usize },
}

impl std::fmt::Display for PatternError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatternError::Count(n) => write!(
                f,
                "a pattern wait carries 1 to {MAX_PATTERNS} patterns, not {n}"
            ),
            PatternError::NameLen { name_len } => write!(
                f,
                "a pattern name is 1 to {MAX_NAME_LEN} bytes, not {name_len}"
            ),
            PatternError::DuplicateName(name) => write!(
                f,
                "two patterns are both named {name:?}; the result names the pattern that fired, so names must be distinct"
            ),
            PatternError::PatternLen { name, len } => write!(
                f,
                "pattern {name:?} is {len} bytes, over the {MAX_PATTERN_BYTES}-byte maximum"
            ),
            PatternError::PatternEmpty { name } => write!(
                f,
                "pattern {name:?} is empty; a pattern that matches everywhere would fire on the first byte and answer nothing"
            ),
            PatternError::RegexNotUtf8 { name } => write!(
                f,
                "pattern {name:?} is a regex whose source is not valid UTF-8; regex *syntax* is text (write \\xNN for an arbitrary byte), while what it matches is not"
            ),
            PatternError::Compile { name, why } => match name {
                Some(name) => write!(f, "pattern {name:?} did not compile: {why}"),
                None => write!(f, "the patterns did not compile together: {why}"),
            },
            PatternError::Range { field, value, max } => {
                write!(f, "{field} is {value}, over the maximum {max}")
            }
            PatternError::LookbackTooSmall { value, min } => write!(
                f,
                "lookback is {value}, under the {min} bytes these patterns need: a window shorter than a pattern cannot hold it, so the cross-frame guarantee (§10) would be silently false rather than merely narrow"
            ),
        }
    }
}

/// A validated, compiled pattern set: one automaton over every pattern, plus the
/// names to report matches by.
///
/// Leftmost wins, and ties break by the caller's order — the automaton is built
/// with [`meta::Regex::build_many`], whose leftmost-first semantics make "which
/// pattern fired" a deterministic function of the stream and the request, never of
/// which engine strategy the meta searcher happened to pick.
pub struct Matcher {
    re: meta::Regex,
    names: Vec<String>,
}

/// Where a pattern fired, in *window* coordinates. [`ScanWindow`] converts these to
/// the endpoint's stream offsets, which is the only space a client ever sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    /// Index into the request's pattern list — [`Matcher::name`] resolves it.
    pub index: usize,
    pub start: usize,
    pub end: usize,
}

impl Matcher {
    /// Validate and compile `specs`, or refuse (§10 clause 2: every maximum checked
    /// before anything is armed).
    ///
    /// The whole set becomes **one** automaton rather than one per pattern: an
    /// alternation is what makes a single left-to-right pass answer "which of these
    /// eight appeared first", and it is also what lets [`MAX_COMPILED_BYTES`] bound
    /// the request rather than each pattern separately.
    pub fn compile(specs: &[PatternSpec]) -> Result<Matcher, PatternError> {
        if specs.is_empty() || specs.len() > MAX_PATTERNS {
            return Err(PatternError::Count(specs.len()));
        }
        let mut names: Vec<String> = Vec::with_capacity(specs.len());
        let mut sources: Vec<String> = Vec::with_capacity(specs.len());
        for spec in specs {
            if spec.name.is_empty() || spec.name.len() > MAX_NAME_LEN {
                return Err(PatternError::NameLen {
                    name_len: spec.name.len(),
                });
            }
            if names.iter().any(|n| n == &spec.name) {
                return Err(PatternError::DuplicateName(spec.name.clone()));
            }
            if spec.bytes.is_empty() {
                return Err(PatternError::PatternEmpty {
                    name: spec.name.clone(),
                });
            }
            if spec.bytes.len() > MAX_PATTERN_BYTES {
                return Err(PatternError::PatternLen {
                    name: spec.name.clone(),
                    len: spec.bytes.len(),
                });
            }
            let source = match spec.kind {
                PatternKind::Literal => escape_bytes(&spec.bytes),
                PatternKind::Regex => String::from_utf8(spec.bytes.clone()).map_err(|_| {
                    PatternError::RegexNotUtf8 {
                        name: spec.name.clone(),
                    }
                })?,
            };
            names.push(spec.name.clone());
            sources.push(source);
        }

        // `utf8(false)` on the syntax half and `utf8_empty(false)` on the meta half
        // are the two switches that make this a *bytes* engine: without them the
        // builder refuses patterns that can match invalid UTF-8, and the searcher
        // would refuse to report an empty match that splits a codepoint — both of
        // which are the wrong answer for a stream §10 explicitly does not assume is
        // text. `unicode(false)` keeps `.` and the classes byte-oriented for the
        // same reason.
        let re = meta::Regex::builder()
            .syntax(syntax::Config::new().utf8(false).unicode(false))
            .configure(
                meta::Config::new()
                    .utf8_empty(false)
                    // The compile-size ceiling of §10 clause 3, applied to every
                    // engine the meta searcher may build.
                    .nfa_size_limit(Some(MAX_COMPILED_BYTES))
                    .hybrid_cache_capacity(MAX_COMPILED_BYTES),
            )
            .build_many(&sources)
            .map_err(|e| {
                // A build error names a pattern id when it can, which is what turns
                // "the patterns did not compile" into "pattern "prompt" did not
                // compile" for the one caller who can act on it.
                let name = e.pattern().and_then(|p| names.get(p.as_usize()).cloned());
                PatternError::Compile {
                    name,
                    why: e.to_string(),
                }
            })?;
        Ok(Matcher { re, names })
    }

    /// The earliest match in `hay`, if any.
    pub fn earliest(&self, hay: &[u8]) -> Option<Hit> {
        self.re.find(hay).map(|m| Hit {
            index: m.pattern().as_usize(),
            start: m.start(),
            end: m.end(),
        })
    }

    /// The caller's name for pattern `index`.
    pub fn name(&self, index: usize) -> &str {
        &self.names[index]
    }

    /// Every pattern name, in the caller's order — what `state` reports an armed
    /// wait is watching for (§10 clause 7).
    pub fn names(&self) -> &[String] {
        &self.names
    }
}

/// Escape arbitrary bytes into a regex source that matches exactly them.
///
/// Every byte becomes `\xNN`, with no exceptions and no attempt to keep the
/// printable ones readable: a literal may contain any byte, including regex
/// metacharacters and bytes that are not valid UTF-8, and the uniform form is the
/// one that cannot be wrong for a subset of inputs. The source is pure ASCII by
/// construction, so it is always a valid `&str` for the builder.
fn escape_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 4);
    for &b in bytes {
        out.push_str(&format!("\\x{b:02x}"));
    }
    out
}

/// What a scan found, in the endpoint's *stream* offset space (§10 clause 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternMatch {
    /// Which pattern fired, by the caller's name.
    pub pattern: String,
    /// The stream offset of the match's first byte, in the same delivered-bytes
    /// space `tap.data` reports (plan §11.8).
    pub offset: u64,
    /// The stream offset one past the match's last byte.
    pub end_offset: u64,
    /// Bytes around the match, bounded by the request's context width. Rides the
    /// wire base64-encoded, because a console's bytes are not text (§10 clause 2).
    pub context: Vec<u8>,
    /// The stream offset of `context[0]`, so the caller can place the context
    /// without inferring it from the match offset and the width.
    pub context_offset: u64,
}

/// The bounded lookback window a pattern is matched in, plus the honesty counters
/// the result carries (§10 clauses 4 and 6).
///
/// **A gap resets the window** — the clause that keeps a "no match" exactly as
/// strong as the observed stream. The producer→hub feed hop is lossy by design
/// (§5), and the hub announces each hole as the next chunk's `gap_before`. Bytes on
/// either side of a hole were never adjacent in the stream, so leaving them
/// adjacent in the window would let a pattern match across bytes nobody observed —
/// a false positive manufactured by the loss. Clearing instead costs at most one
/// window's worth of true negatives and cannot invent a positive, and the gap
/// counters ride the result so a caller reading `matched: null` knows whether the
/// scan was whole.
pub struct ScanWindow {
    matcher: Matcher,
    buf: Vec<u8>,
    lookback: usize,
    context: usize,
    /// Stream offset of `buf[0]`.
    base: u64,
    /// Stream bytes fed to the matcher — *observed* bytes, not byte-comparisons.
    /// The window is re-scanned per chunk, so the comparison count is higher by
    /// construction; what a caller acts on is how much stream the verdict covers.
    scanned: u64,
    /// How many times a gap reset the window.
    gaps: u64,
    /// How many bytes those gaps totalled.
    gap_bytes: u64,
}

impl ScanWindow {
    /// A window over `matcher`, empty, anchored at stream offset `base`.
    pub fn new(matcher: Matcher, lookback: usize, context: usize, base: u64) -> ScanWindow {
        ScanWindow {
            matcher,
            buf: Vec::new(),
            lookback,
            context,
            base,
            scanned: 0,
            gaps: 0,
            gap_bytes: 0,
        }
    }

    /// Seed the window with the replay ring's bytes and scan them (§10 clause 5).
    ///
    /// `ring` is the ring's own snapshot, oldest first, whose first byte sits at
    /// stream offset `base` — **the ring itself, never a tap channel's budgeted
    /// copy**, which is what keeps ring depth from being silently trimmed out of the
    /// scan by a bound that has nothing to do with the request. The whole snapshot
    /// is scanned even when it is deeper than `lookback`; only what is *retained*
    /// afterwards, to be re-scanned against later live bytes, is trimmed to the
    /// window.
    ///
    /// **`excluded_gap` is the hole the caller had to cut out of the snapshot**, and
    /// it is the other half of this type's gap discipline. The ring is contiguous in
    /// *offset* space by construction — that is what makes `tap.open --replay`'s
    /// splice exact — but it is **not** contiguous in the *stream*: `TapHub::ingest`
    /// pushes every chunk into the ring whatever feed loss preceded it, so two ring
    /// bytes either side of a hole sit adjacent in the snapshot while never having
    /// been adjacent on the wire. Scanning across that seam is exactly the false
    /// positive [`Self::feed`]'s reset exists to prevent, and it is worse here,
    /// because a wait is a gating verb: a caller would send a password to a device
    /// that never printed the prompt. So the hub trims the snapshot at the newest
    /// hole and reports its size here, where it is counted like any other gap.
    pub fn seed(&mut self, ring: &[u8], base: u64, excluded_gap: u64) -> Option<PatternMatch> {
        if excluded_gap > 0 {
            self.gaps += 1;
            self.gap_bytes = self.gap_bytes.saturating_add(excluded_gap);
        }
        self.base = base;
        self.buf.clear();
        self.buf.extend_from_slice(ring);
        self.scanned += ring.len() as u64;
        let hit = self.scan();
        if hit.is_none() {
            self.trim();
        }
        hit
    }

    /// Feed one ingested chunk and scan (§10 clause 4).
    ///
    /// `gap_before` is the hub's per-chunk feed-loss delta — the same number
    /// `tap.data` carries — and a non-zero value resets the window before the chunk
    /// lands, per this type's doc.
    pub fn feed(&mut self, chunk: &[u8], gap_before: u64) -> Option<PatternMatch> {
        if gap_before > 0 {
            self.gaps += 1;
            self.gap_bytes = self.gap_bytes.saturating_add(gap_before);
            // The window's bytes and the incoming chunk are not adjacent in the
            // stream: drop the former rather than let a match span the hole.
            self.base = self.base.wrapping_add(self.buf.len() as u64);
            self.buf.clear();
        }
        self.buf.extend_from_slice(chunk);
        self.scanned = self.scanned.wrapping_add(chunk.len() as u64);
        let hit = self.scan();
        if hit.is_none() {
            self.trim();
        }
        hit
    }

    /// Scan the whole window and lift a hit into stream coordinates.
    ///
    /// The *whole* window, every time, rather than only the newly-appended tail: a
    /// match that ends in the new bytes may begin anywhere behind them, and a regex
    /// gives no bound on how far behind other than the window itself. That is
    /// exactly what makes the window a *lookback* and why [`MAX_LOOKBACK`] is a scan
    /// cost as well as an allocation (see this module's doc, property 3). A match
    /// lying wholly inside the retained bytes cannot be a surprise — a previous scan
    /// covered it and this wait would already have settled — so re-finding one is
    /// impossible rather than merely unlikely.
    fn scan(&self) -> Option<PatternMatch> {
        let hit = self.matcher.earliest(&self.buf)?;
        let half = self.context / 2;
        let ctx_start = hit.start.saturating_sub(half);
        let ctx_end = (hit.end + half).min(self.buf.len());
        let ctx = &self.buf[ctx_start..ctx_end];
        // A match wider than the whole context budget is truncated rather than
        // allowed to choose the daemon's reply size: the offsets say where the match
        // is, and `context` is bounded by the caller's own stated maximum.
        let ctx = &ctx[..ctx.len().min(self.context)];
        Some(PatternMatch {
            pattern: self.matcher.name(hit.index).to_owned(),
            offset: self.base.wrapping_add(hit.start as u64),
            end_offset: self.base.wrapping_add(hit.end as u64),
            context: ctx.to_vec(),
            context_offset: self.base.wrapping_add(ctx_start as u64),
        })
    }

    /// Drop the window's head down to `lookback`, advancing the base with it.
    fn trim(&mut self) {
        if self.buf.len() > self.lookback {
            let drop = self.buf.len() - self.lookback;
            self.buf.drain(..drop);
            self.base = self.base.wrapping_add(drop as u64);
        }
    }

    /// Stream bytes this wait has observed.
    pub fn scanned(&self) -> u64 {
        self.scanned
    }

    /// How many gaps interrupted the scan, and how many bytes they totalled — the
    /// two numbers that make a `matched: null` say how strong it is (§10 clause 4).
    pub fn gaps(&self) -> (u64, u64) {
        (self.gaps, self.gap_bytes)
    }

    /// Every pattern name this window watches for, for `state` (§10 clause 7).
    pub fn names(&self) -> &[String] {
        self.matcher.names()
    }
}

/// A parsed, range-checked `tap.wait` request: everything the verb needs, with
/// every §16.12 maximum already applied (§10 clause 2).
pub struct WaitSpec {
    pub endpoint: String,
    pub patterns: Vec<PatternSpec>,
    pub replay: bool,
    pub lookback: usize,
    pub context: usize,
    pub timeout_ms: u64,
}

/// Range-check one numeric dimension, naming the field and its maximum on refusal.
pub fn checked(field: &'static str, value: u64, max: u64) -> Result<u64, PatternError> {
    if value > max {
        return Err(PatternError::Range { field, value, max });
    }
    Ok(value)
}

/// The deadline maximum, reusing the config timer ceiling (§16.12: the rule is that
/// a maximum exists, not that each surface mints its own).
pub const MAX_WAIT_MS: u64 = MAX_TIMER_MS;

#[cfg(test)]
mod tests {
    use super::*;

    fn lit(name: &str, bytes: &[u8]) -> PatternSpec {
        PatternSpec {
            name: name.into(),
            kind: PatternKind::Literal,
            bytes: bytes.to_vec(),
        }
    }

    fn rx(name: &str, source: &str) -> PatternSpec {
        PatternSpec {
            name: name.into(),
            kind: PatternKind::Regex,
            bytes: source.as_bytes().to_vec(),
        }
    }

    fn window(specs: Vec<PatternSpec>) -> ScanWindow {
        ScanWindow::new(
            Matcher::compile(&specs).expect("compiles"),
            DEFAULT_LOOKBACK,
            DEFAULT_CONTEXT,
            0,
        )
    }

    #[test]
    fn a_literal_matches_and_reports_its_stream_offset() {
        let mut w = window(vec![lit("login", b"login:")]);
        assert!(w.feed(b"booting\n", 0).is_none());
        let m = w.feed(b"welcome, login: ", 0).expect("matches");
        assert_eq!(m.pattern, "login");
        // 8 bytes of "booting\n", then "welcome, " is 9 more → offset 17.
        assert_eq!(m.offset, 17);
        assert_eq!(m.end_offset, 23);
    }

    /// §10 clause 4: "a pattern split across data-frame boundaries matches by
    /// construction" — the reason the matcher consumes the hub stream rather than a
    /// framed copy of it.
    #[test]
    fn a_pattern_split_across_two_chunks_matches() {
        let mut w = window(vec![lit("login", b"login:")]);
        assert!(w.feed(b"lo", 0).is_none());
        let m = w.feed(b"gin: ", 0).expect("matches across the seam");
        assert_eq!(m.offset, 0);
        assert_eq!(m.end_offset, 6);
    }

    /// A literal is escaped into the automaton byte-by-byte, so metacharacters and
    /// non-UTF-8 bytes are matched verbatim rather than interpreted.
    #[test]
    fn a_literal_is_verbatim_even_when_it_looks_like_a_regex() {
        let mut w = window(vec![lit("dot", b"a.c")]);
        assert!(
            w.feed(b"abc", 0).is_none(),
            "the literal `a.c` must not match `abc`"
        );
        assert!(w.feed(b"xa.c", 0).is_some(), "but it matches `a.c`");
    }

    #[test]
    fn a_literal_may_be_arbitrary_bytes() {
        let mut w = window(vec![lit("raw", &[0xff, 0x00, 0xfe])]);
        assert!(w.feed(&[0x01, 0xff, 0x00, 0xfe], 0).is_some());
    }

    /// The engine is byte-oriented: `.` is any byte, and the haystack is never
    /// assumed to be UTF-8 (§10 clause 2).
    #[test]
    fn a_regex_matches_over_bytes_not_scalars() {
        let mut w = window(vec![rx("any", r"a.c")]);
        assert!(w.feed(&[b'a', 0xff, b'c'], 0).is_some());
    }

    #[test]
    fn the_first_pattern_to_appear_wins_and_the_result_names_it() {
        let mut w = window(vec![lit("second", b"world"), lit("first", b"hello")]);
        let m = w.feed(b"hello world", 0).expect("matches");
        assert_eq!(
            m.pattern, "first",
            "leftmost in the stream, not in the list"
        );
    }

    /// §10 clause 4: a gap resets the window, so a match never spans bytes that
    /// were not observed.
    #[test]
    fn a_gap_resets_the_window_so_no_match_spans_unobserved_bytes() {
        let mut w = window(vec![lit("login", b"login:")]);
        assert!(w.feed(b"lo", 0).is_none());
        // 4096 bytes vanished at the feed hop between the two chunks: `lo` and
        // `gin:` were never adjacent, so this must not match.
        assert!(
            w.feed(b"gin: ", 4096).is_none(),
            "a gap must not be spliced over"
        );
        assert_eq!(w.gaps(), (1, 4096));
    }

    #[test]
    fn the_window_is_bounded_and_a_match_older_than_it_is_forgotten() {
        let mut w = ScanWindow::new(
            Matcher::compile(&[lit("login", b"login:")]).unwrap(),
            8,
            DEFAULT_CONTEXT,
            0,
        );
        // The pattern's head falls out of an 8-byte window before its tail arrives.
        assert!(w.feed(b"login", 0).is_none());
        assert!(w.feed(b"XXXXXXXX", 0).is_none());
        assert!(
            w.feed(b":", 0).is_none(),
            "the window is the stated bound on how far a match may reach back"
        );
    }

    /// **The per-chunk scan COST is bounded by the lookback, not by the stream**
    /// (plan §18 item 64(a); this module's doc, property 3).
    ///
    /// The guard above bounds the window's *semantics* — how far back a match may
    /// reach — and that is a different property from this one. A window that trimmed
    /// lazily (say, every hundredth chunk) would keep those semantics exactly and
    /// multiply the work [`ScanWindow::scan`] does per chunk by a hundred, because
    /// `scan` re-scans the **whole** window every time. At [`MAX_LOOKBACK`] on the
    /// runtime thread that is the difference between a wait an operator may arm on a
    /// busy console and one they may not, and nothing here was watching it: the
    /// module doc has always said "`MAX_LOOKBACK` is a scan cost as well as an
    /// allocation" and no test read the buffer's length.
    ///
    /// So this asserts the state that *determines* the next scan's cost, which is
    /// deterministic and needs no clock: after every non-matching feed the retained
    /// window is at most `lookback`, so the work per chunk is at most
    /// `lookback + chunk` however long the stream runs. Timing it instead would make
    /// the guard a property of the box (plan §18 item 61 measured this axis moving
    /// 30x with load on one tree), and a timed ceiling wide enough not to flake is
    /// wide enough to admit the regression.
    ///
    /// **The bound is asserted to be REACHED, not merely respected.** A `<=` over a
    /// window that never fills holds for any bound at all — the vacuous shape this
    /// item is about — so the worst observed length must equal the lookback exactly.
    #[test]
    fn the_retained_window_never_exceeds_the_lookback_so_per_chunk_work_is_bounded() {
        // A max-lookback wait, the operator-reachable end of the one dimension that
        // is a per-chunk cost, and a pattern that cannot appear in the fed bytes so
        // every feed takes the trimming path.
        let mut w = ScanWindow::new(
            Matcher::compile(&[lit("never", b"NEVER-ON-THE-WIRE")]).unwrap(),
            MAX_LOOKBACK,
            DEFAULT_CONTEXT,
            0,
        );
        // Chunks smaller than the window, which is the interesting shape: the window
        // is then rebuilt from many feeds rather than replaced by one, and a lazy or
        // deleted trim accumulates instead of staying flat.
        let chunk = vec![b'.'; 4096];
        let chunks = 512;
        let mut worst = 0usize;
        for i in 0..chunks {
            assert!(
                w.feed(&chunk, 0).is_none(),
                "nothing may match at chunk {i}"
            );
            worst = worst.max(w.buf.len());
            assert!(
                w.buf.len() <= MAX_LOOKBACK,
                "after {} fed bytes the retained window is {} B, past the {MAX_LOOKBACK} B \
                 lookback. `ScanWindow::scan` re-scans the whole window per chunk, so this \
                 is the per-chunk cost of every armed wait on the runtime thread growing \
                 with the STREAM rather than with the operator's stated window (§10 \
                 clause 4)",
                (i + 1) * chunk.len(),
                w.buf.len()
            );
        }
        assert_eq!(
            worst,
            MAX_LOOKBACK,
            "the window never reached its lookback across {} fed bytes, so the bound \
             above held for a window that was never under pressure and would hold for \
             any bound at all",
            chunks * chunk.len()
        );
        // And the scan really did cover the stream, or the loop above trimmed a
        // window nothing was ever fed into.
        assert_eq!(w.scanned(), (chunks * chunk.len()) as u64);
    }

    /// §10 clause 5: the ring is scanned as the ring, and a pattern wholly emitted
    /// before the wait began matches.
    #[test]
    fn a_seeded_ring_matches_a_pattern_that_predates_the_wait() {
        let mut w = window(vec![lit("login", b"login:")]);
        let m = w.seed(b"...login: ", 90, 0).expect("the ring matches");
        assert_eq!(m.offset, 93, "ring offsets are stream offsets");
        assert_eq!(w.scanned(), 10);
    }

    /// The whole snapshot is scanned even when it is deeper than the lookback: the
    /// window bounds what is *retained* for later live bytes, never what the ring
    /// scan covers.
    #[test]
    fn a_ring_deeper_than_the_lookback_is_scanned_whole() {
        let mut w = ScanWindow::new(
            Matcher::compile(&[lit("login", b"login:")]).unwrap(),
            8,
            DEFAULT_CONTEXT,
            0,
        );
        let mut ring = b"login:".to_vec();
        ring.extend_from_slice(&[b'.'; 4096]);
        let m = w.seed(&ring, 0, 0).expect("the ring's head is scanned too");
        assert_eq!(m.offset, 0);
    }

    #[test]
    fn context_is_bounded_and_carries_its_own_offset() {
        let mut w = ScanWindow::new(Matcher::compile(&[lit("m", b"MARK")]).unwrap(), 4096, 8, 0);
        let m = w.feed(b"aaaaaaaaaaMARKbbbbbbbbbb", 0).expect("matches");
        assert!(m.context.len() <= 8, "context is bounded: {:?}", m.context);
        assert_eq!(
            &w.buf[(m.context_offset as usize)..(m.context_offset as usize + m.context.len())],
            &m.context[..],
            "context_offset places the context in the stream"
        );
    }

    // --- The maxima, each refused before anything is armed (§16.12) -------------

    #[test]
    fn zero_patterns_and_nine_patterns_are_both_refused() {
        assert!(matches!(Matcher::compile(&[]), Err(PatternError::Count(0))));
        let many: Vec<_> = (0..=MAX_PATTERNS)
            .map(|i| lit(&format!("p{i}"), b"x"))
            .collect();
        assert!(matches!(
            Matcher::compile(&many),
            Err(PatternError::Count(9))
        ));
    }

    #[test]
    fn an_over_long_name_and_an_over_long_pattern_are_refused() {
        let long = "n".repeat(MAX_NAME_LEN + 1);
        assert!(matches!(
            Matcher::compile(&[lit(&long, b"x")]),
            Err(PatternError::NameLen { .. })
        ));
        assert!(matches!(
            Matcher::compile(&[lit("p", &vec![b'x'; MAX_PATTERN_BYTES + 1])]),
            Err(PatternError::PatternLen { .. })
        ));
        // And each is accepted exactly at its cap, so the bound is `>` not `>=`.
        assert!(Matcher::compile(&[lit(&"n".repeat(MAX_NAME_LEN), b"x")]).is_ok());
        assert!(Matcher::compile(&[lit("p", &vec![b'x'; MAX_PATTERN_BYTES])]).is_ok());
    }

    #[test]
    fn duplicate_names_are_refused_because_the_result_names_one() {
        assert!(matches!(
            Matcher::compile(&[lit("same", b"a"), lit("same", b"b")]),
            Err(PatternError::DuplicateName(_))
        ));
    }

    #[test]
    fn a_regex_source_that_is_not_utf8_is_refused_rather_than_mangled() {
        let spec = PatternSpec {
            name: "bad".into(),
            kind: PatternKind::Regex,
            bytes: vec![0xff, 0xfe],
        };
        assert!(matches!(
            Matcher::compile(&[spec]),
            Err(PatternError::RegexNotUtf8 { .. })
        ));
    }

    #[test]
    fn an_unparseable_regex_is_refused_naming_the_pattern() {
        match Matcher::compile(&[rx("bad", "a(")]).err() {
            Some(PatternError::Compile { name, .. }) => {
                assert_eq!(name.as_deref(), Some("bad"))
            }
            other => panic!("expected a compile error naming the pattern, got {other:?}"),
        }
    }

    /// §10 clause 3 / §15.56's decline: a pattern whose *compile* would blow the
    /// budget is refused at compile rather than paid for on the thread that runs
    /// every console.
    #[test]
    fn a_counted_repetition_bomb_is_refused_at_compile() {
        let err = Matcher::compile(&[rx("bomb", r"(?:a{1000}){1000}")]);
        assert!(
            matches!(err, Err(PatternError::Compile { .. })),
            "a program over the {MAX_COMPILED_BYTES}-byte ceiling must be refused"
        );
    }

    /// The property §15.56 declines a backtracking engine to get: match cost stays
    /// linear on the classic exponential-backtracking input, so an operator input
    /// cannot deny the runtime thread.
    #[test]
    fn the_pathological_backtracking_input_is_linear_here() {
        let mut w = window(vec![rx("evil", r"(a+)+b")]);
        let hay = vec![b'a'; 4096];
        let start = std::time::Instant::now();
        // No `b`, so this is the input that makes a backtracking engine hang.
        assert!(w.feed(&hay, 0).is_none());
        let elapsed = start.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(1),
            "a linear-time engine answers this at once; took {elapsed:?}"
        );
    }

    /// The seam a replay scan must not cross: the ring holds bytes either side of a
    /// feed-hop hole with nothing between them, so the hub trims at the hole and
    /// tells `seed` how big it was. The counters must then say the scan was not
    /// whole — a `matched: null` over a holed ring is a weaker answer than one over
    /// a whole ring, and §10 clause 4 requires it to say so.
    #[test]
    fn an_excluded_ring_gap_is_counted_so_the_verdict_states_its_own_strength() {
        let mut w = window(vec![lit("login", b"login:")]);
        // The hub handed us only the post-hole tail; 4096 bytes were cut out.
        assert!(w.seed(b"gin: ready", 500, 4096).is_none());
        assert_eq!(
            w.gaps(),
            (1, 4096),
            "an excluded ring gap is a gap in this wait's scan"
        );
        // And the trimmed snapshot cannot manufacture the match the hole hid: the
        // `lo` that preceded the hole is not in the window at all.
        assert!(w.feed(b" now", 0).is_none());
    }

    /// §10 clause 4 must not be silently voidable by an input. A window shorter than
    /// the pattern cannot hold it, so it is refused rather than accepted into a
    /// `timed_out: true` that claims a whole scan.
    #[test]
    fn the_lookback_floor_is_the_longest_literal_and_never_zero() {
        assert_eq!(min_lookback(&[lit("a", b"login:")]), 6);
        assert_eq!(
            min_lookback(&[lit("a", b"ab"), lit("b", b"abcdef")]),
            6,
            "the longest literal decides"
        );
        // A regex cannot be sized by the daemon — `a{1000}` is a six-byte source —
        // so it contributes only the floor of 1 and the docs say who must size it.
        assert_eq!(min_lookback(&[rx("r", "a{1000}")]), 1);
        assert_eq!(
            min_lookback(&[rx("r", "x"), lit("l", b"abc")]),
            3,
            "a literal beside a regex still sets the floor"
        );
    }

    /// An empty pattern is refused with a reason that is true. Sharing
    /// [`PatternError::PatternLen`] produced `is 0 bytes, over the 1024-byte
    /// maximum` — the opposite of the truth, which is worse than no reason at all.
    #[test]
    fn an_empty_pattern_is_refused_and_the_message_is_not_about_the_maximum() {
        match Matcher::compile(&[lit("p", b"")]).err() {
            Some(e @ PatternError::PatternEmpty { .. }) => {
                let msg = e.to_string();
                assert!(msg.contains("empty"), "{msg}");
                assert!(
                    !msg.contains("maximum"),
                    "the refusal must not blame a maximum: {msg}"
                );
            }
            other => panic!("expected an empty-pattern refusal, got {other:?}"),
        }
    }

    #[test]
    fn checked_names_the_field_and_its_maximum() {
        assert!(checked("timeout_ms", MAX_WAIT_MS, MAX_WAIT_MS).is_ok());
        match checked("timeout_ms", MAX_WAIT_MS + 1, MAX_WAIT_MS) {
            Err(PatternError::Range { field, value, max }) => {
                assert_eq!(field, "timeout_ms");
                assert_eq!(value, MAX_WAIT_MS + 1);
                assert_eq!(max, MAX_WAIT_MS);
            }
            other => panic!("expected a range refusal, got {other:?}"),
        }
    }
}
