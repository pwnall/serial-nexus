//! The map node's character-mapping transform (design §7.8, §15.33): picocom's
//! `--imap`/`--omap` byte mappings, made a first-class interior transform instead
//! of a flag on every client.
//!
//! This is **pure logic** — no I/O, no kernel objects — so it lives in `serial-nexus-core`
//! and is property-tested here. The daemon's map node ([`crate::nodes`] in
//! `serial-nexus-daemon`) wraps a compiled [`MapDirection`] per direction and runs it on
//! the §5 interior contract: a stateless byte-to-byte-sequence substitution, so
//! chunk boundaries are irrelevant by construction and no parser state exists.
//!
//! **First match wins.** A direction is an *ordered* list of [`Mapping`]s. For each
//! input byte, the first rule in the list whose match-set contains the byte fires
//! and emits its substitution; if none matches, the byte passes through unchanged.
//! Order therefore resolves conflicts deterministically — `["igncr", "crlf"]`
//! deletes CR, `["crlf", "igncr"]` translates it (design §7.8). This differs from
//! picocom, which applies a fixed internal priority; here the operator's list order
//! is the priority, which is both simpler and more explicit.
//!
//! **Bounded expansion.** Every substitution is at most [`MAX_EXPANSION`] bytes (the
//! 4-byte hex form `[xx]`), so a direction's output is bounded at `k ×` its input,
//! where `k` is the largest expansion among its *active* rules — 1 for the plain
//! translations, 2 for the CRLF pair, 4 for the hex-display family. That bound is
//! what keeps the §5 one-frame holdover slot's memory bounded across the transform.
//!
//! **Fidelity to picocom.** The vocabulary and every rule's match set were read off
//! upstream `picocom.c` itself — the flag table (`M_CRLF` … `M_NRMHEX`), `do_map`,
//! and `map2hex` — and not off its manual page, which is where an earlier version of
//! this module got `spchex` wrong (next paragraph). The correspondence, rule by rule:
//! `crlf`/`crcrlf`/`igncr`/`crhex` all match CR and emit LF / CR+LF / nothing /
//! `[0d]`; `lfcr`/`lfcrlf`/`ignlf`/`lfhex` match LF symmetrically; `bsdel` is BS→DEL
//! and `delbs` DEL→BS; `tabhex` is TAB alone; `8bithex` is every high-bit byte
//! `0x80..=0xff` (upstream `c & 0x80`); `nrmhex` is printable ASCII `0x20..=0x7e`
//! (upstream `c >= 0x20 && c < 0x7f`), **space included** — so `nrmhex`, not
//! `spchex`, is what hexes a space. Every hex rule renders through `map2hex`'s `[` +
//! two *lowercase* hex digits + `]`, which is where [`MAX_EXPANSION`] = 4 comes from
//! (upstream's `M_MAXMAP`).
//!
//! **`spchex` is picocom's control-byte class, not SPACE.** Upstream's flag comment
//! reads "map special chars --> hex" and its `do_map` predicate is `c == '\x7f' ||
//! ((unsigned char)c < 0x20 && c != '\x09' && c != '\x0a' && c != '\x0d')` — DEL plus
//! every C0 control *except* TAB/LF/CR, which have rules of their own. This module
//! shipped `spchex` as `b == 0x20` through v11, which both rewrote every space as
//! `[20]` for anyone who enabled it and left `0x00..=0x1f` and `0x7f` unreachable by
//! *any* rule in the vocabulary — the "cheap way to discover which quirk a mystery
//! console actually has" that §7.8 advertises did not exist. **Correcting it changes
//! the bytes an existing `spchex` configuration produces**, so it is a behavior change
//! worth a release note, not a silent fix. With the upstream set the hex family now
//! partitions the whole byte space — `spchex` ∪ `tabhex` ∪ `crhex` ∪ `lfhex` ∪
//! `nrmhex` ∪ `8bithex` = `0x00..=0xff` — and `spchex` overlaps exactly two non-hex
//! rules, `bsdel` (BS, `0x08`) and `delbs` (DEL, `0x7f`), where first-match-wins means
//! the operator's list order decides rather than picocom's internal priority.

/// The maximum bytes any single mapping emits for one input byte: the hex-display
/// form `[xx]` (4 bytes). The output of a direction is bounded at
/// `MAX_EXPANSION × input`, so the map's interior holdover stays bounded (§5, §7.8).
pub const MAX_EXPANSION: usize = 4;

/// Lowercase hex digits, matching picocom's `map2hex`.
const HEXD: &[u8; 16] = b"0123456789abcdef";

/// One picocom byte mapping. Each variant matches a fixed input byte (or, for the
/// two range families, a byte range) and emits a fixed byte sequence. Names are
/// exactly picocom's (`crlf`, `8bithex`, `nrmhex`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mapping {
    /// CR → LF.
    Crlf,
    /// CR → CR LF.
    Crcrlf,
    /// CR → (deleted).
    Igncr,
    /// LF → CR.
    Lfcr,
    /// LF → CR LF.
    Lfcrlf,
    /// LF → (deleted).
    Ignlf,
    /// BS → DEL.
    Bsdel,
    /// DEL → BS.
    Delbs,
    /// Any "special" (C0 control) byte → `[xx]`: DEL (`0x7f`) plus `0x00..=0x1f`
    /// except TAB/LF/CR, which have their own rules. Picocom's `M_SPCHEX` — *not*
    /// SPACE, despite the name (see the module doc's fidelity note).
    Spchex,
    /// TAB → `[09]`.
    Tabhex,
    /// CR → `[0d]`.
    Crhex,
    /// LF → `[0a]`.
    Lfhex,
    /// Any 8-bit byte `0x80..=0xff` → `[xx]`.
    EightBitHex,
    /// Any printable ASCII byte `0x20..=0x7e` (space included) → `[xx]`.
    NrmHex,
}

/// The full vocabulary, in a stable order, paired with its configuration name — the
/// single source of truth for parsing, display, and the "unknown mapping" error's
/// available-list (design §7.8).
const VOCABULARY: &[(&str, Mapping)] = &[
    ("crlf", Mapping::Crlf),
    ("crcrlf", Mapping::Crcrlf),
    ("igncr", Mapping::Igncr),
    ("lfcr", Mapping::Lfcr),
    ("lfcrlf", Mapping::Lfcrlf),
    ("ignlf", Mapping::Ignlf),
    ("bsdel", Mapping::Bsdel),
    ("delbs", Mapping::Delbs),
    ("spchex", Mapping::Spchex),
    ("tabhex", Mapping::Tabhex),
    ("crhex", Mapping::Crhex),
    ("lfhex", Mapping::Lfhex),
    ("8bithex", Mapping::EightBitHex),
    ("nrmhex", Mapping::NrmHex),
];

impl Mapping {
    /// Parse a configuration name (`"crlf"`, `"8bithex"`, …). `None` for an unknown
    /// name — the map node turns that into a structural error naming the offender
    /// (§7.8, §11).
    pub fn from_name(name: &str) -> Option<Mapping> {
        VOCABULARY.iter().find(|(n, _)| *n == name).map(|(_, m)| *m)
    }

    /// This mapping's configuration name.
    pub fn name(self) -> &'static str {
        VOCABULARY
            .iter()
            .find(|(_, m)| *m == self)
            .map(|(n, _)| *n)
            .expect("every Mapping is in the VOCABULARY table")
    }

    /// Every valid mapping name, for an error's available-list and the docs.
    pub fn all_names() -> impl Iterator<Item = &'static str> {
        VOCABULARY.iter().map(|(n, _)| *n)
    }

    /// Whether this mapping transforms input byte `b`.
    fn matches(self, b: u8) -> bool {
        match self {
            Mapping::Crlf | Mapping::Crcrlf | Mapping::Igncr | Mapping::Crhex => b == 0x0d,
            Mapping::Lfcr | Mapping::Lfcrlf | Mapping::Ignlf | Mapping::Lfhex => b == 0x0a,
            Mapping::Bsdel => b == 0x08,
            Mapping::Delbs => b == 0x7f,
            // picocom's `M_SPCHEX` arm of `do_map`, transcribed: `c == '\x7f' ||
            // ((unsigned char)c < 0x20 && c != '\x09' && c != '\x0a' && c != '\x0d')`.
            // The name says SPACE, the flag comment says "special chars", and the code
            // says C0-minus-TAB/LF/CR plus DEL — the code wins (module doc).
            Mapping::Spchex => b == 0x7f || (b < 0x20 && b != 0x09 && b != 0x0a && b != 0x0d),
            Mapping::Tabhex => b == 0x09,
            // picocom's `c & 0x80` on a signed char — every byte with the high bit set.
            Mapping::EightBitHex => b >= 0x80,
            // Printable ASCII, space included — picocom's `0x20 <= c < 0x7f`.
            Mapping::NrmHex => (0x20..=0x7e).contains(&b),
        }
    }

    /// The substitution for input byte `b` (only meaningful when [`Self::matches`]).
    /// Returns the output in a fixed 4-byte buffer plus a length `0..=4`, so no
    /// allocation is needed — the output is always at most [`MAX_EXPANSION`] bytes.
    fn output(self, b: u8) -> ([u8; MAX_EXPANSION], usize) {
        let mut out = [0u8; MAX_EXPANSION];
        let n = match self {
            Mapping::Crlf => {
                out[0] = 0x0a;
                1
            }
            Mapping::Lfcr => {
                out[0] = 0x0d;
                1
            }
            Mapping::Crcrlf | Mapping::Lfcrlf => {
                out[0] = 0x0d;
                out[1] = 0x0a;
                2
            }
            Mapping::Igncr | Mapping::Ignlf => 0,
            Mapping::Bsdel => {
                out[0] = 0x7f;
                1
            }
            Mapping::Delbs => {
                out[0] = 0x08;
                1
            }
            // Every hex-display mapping renders the matched byte as `[xx]` (picocom's
            // `map2hex`). For the single-byte rules (tabhex/crhex/lfhex) `b` is that one
            // byte; for the class rules (spchex/8bithex/nrmhex) it is whichever matched.
            Mapping::Spchex
            | Mapping::Tabhex
            | Mapping::Crhex
            | Mapping::Lfhex
            | Mapping::EightBitHex
            | Mapping::NrmHex => {
                out[0] = b'[';
                out[1] = HEXD[(b >> 4) as usize];
                out[2] = HEXD[(b & 0x0f) as usize];
                out[3] = b']';
                4
            }
        };
        (out, n)
    }

    /// The largest number of output bytes this mapping can emit for one input byte
    /// — its expansion factor (§7.8): 1 for the plain translations, 2 for the CRLF
    /// pair, 4 for the hex family, 0 for the deletions.
    fn expansion(self) -> usize {
        match self {
            Mapping::Igncr | Mapping::Ignlf => 0,
            Mapping::Crlf | Mapping::Lfcr | Mapping::Bsdel | Mapping::Delbs => 1,
            Mapping::Crcrlf | Mapping::Lfcrlf => 2,
            Mapping::Spchex
            | Mapping::Tabhex
            | Mapping::Crhex
            | Mapping::Lfhex
            | Mapping::EightBitHex
            | Mapping::NrmHex => 4,
        }
    }
}

/// One byte's compiled fate under a direction's ordered rule list.
#[derive(Clone, Copy)]
enum Slot {
    /// No rule matched: emit the byte unchanged, count nothing.
    Pass,
    /// The first matching rule (by list index) fired: emit `out[..len]`, count `rule`.
    Sub {
        rule: usize,
        out: [u8; MAX_EXPANSION],
        len: u8,
    },
}

/// A compiled mapping direction: an ordered rule list plus a 256-entry first-match
/// table, so applying the transform is one table lookup per input byte with no
/// per-byte rule scan (§5 hot path). Built once at node start from the configured
/// name list; immutable thereafter (the map is stateless).
pub struct MapDirection {
    rules: Vec<Mapping>,
    table: Box<[Slot; 256]>,
    max_expansion: usize,
}

impl MapDirection {
    /// Compile an ordered list of parsed mappings into the first-match table.
    pub fn compile(rules: Vec<Mapping>) -> MapDirection {
        let mut table = Box::new([Slot::Pass; 256]);
        for (byte, slot) in table.iter_mut().enumerate() {
            let b = byte as u8;
            // First match wins: the earliest rule in the list whose match-set holds
            // `b` decides the byte's fate; later rules on the same byte are shadowed.
            if let Some((rule, m)) = rules
                .iter()
                .copied()
                .enumerate()
                .find(|(_, m)| m.matches(b))
            {
                let (out, len) = m.output(b);
                *slot = Slot::Sub {
                    rule,
                    out,
                    len: len as u8,
                };
            }
        }
        // `k` for the `k × input` output bound: the largest expansion among the
        // active rules, floored at 1 (an identity or delete-only direction never
        // grows, so 1 is a safe, tight bound there, §7.8).
        let max_expansion = rules
            .iter()
            .map(|m| m.expansion())
            .max()
            .unwrap_or(1)
            .max(1);
        MapDirection {
            rules,
            table,
            max_expansion,
        }
    }

    /// Parse and compile an ordered list of configuration names. `Err(name)` names
    /// the first unknown mapping — the map node turns that into a structural error
    /// (§7.8, §11), so a bad name never grows a graph or, under `--replace`, destroys
    /// one.
    pub fn parse(names: &[String]) -> Result<MapDirection, String> {
        let mut rules = Vec::with_capacity(names.len());
        for name in names {
            match Mapping::from_name(name) {
                Some(m) => rules.push(m),
                None => return Err(name.clone()),
            }
        }
        Ok(MapDirection::compile(rules))
    }

    /// The number of configured rules (the length of the per-rule counter vector).
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// The configuration name of rule `i` (for per-rule counter reporting, §7.8).
    pub fn rule_name(&self, i: usize) -> &'static str {
        self.rules[i].name()
    }

    /// The output expansion bound `k`: `apply` never emits more than `k ×` its input
    /// (§7.8). 1 for an identity or delete-only direction.
    pub fn max_expansion(&self) -> usize {
        self.max_expansion
    }

    /// Apply the transform to `input`, appending the mapped bytes to `out` and
    /// invoking `on_rule(i)` once for every input byte that rule `i` substituted
    /// (first match). Stateless and chunk-boundary-agnostic (§5): calling it twice
    /// on the two halves of a buffer yields the same output as one call on the whole.
    ///
    /// The `on_rule` callback decouples this pure module from the daemon's counter
    /// representation (`Cell` on the runtime thread); a test passes a no-op or a
    /// tally closure. Output is bounded at `max_expansion × input.len()` bytes.
    pub fn apply(&self, input: &[u8], out: &mut Vec<u8>, mut on_rule: impl FnMut(usize)) {
        for &b in input {
            match &self.table[b as usize] {
                Slot::Pass => out.push(b),
                Slot::Sub {
                    rule,
                    out: buf,
                    len,
                } => {
                    out.extend_from_slice(&buf[..*len as usize]);
                    on_rule(*rule);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// A direct oracle for one mapping applied to one byte — the specification the
    /// compiled table is checked against, transcribed from upstream `picocom.c`'s
    /// `do_map`/`map2hex` rather than derived from this module. Where the oracle can
    /// state a rule in a *different* form than [`Mapping::matches`] does, it
    /// deliberately does (the class rules enumerate their byte sets; `8bithex` uses
    /// upstream's `c & 0x80`), so a predicate edit fails here instead of agreeing with
    /// itself. The upstream predicates, for the record:
    ///
    /// ```text
    /// M_CRLF/M_CRCRLF/M_IGNCR/M_CRHEX   case '\x0d'
    /// M_LFCR/M_LFCRLF/M_IGNLF/M_LFHEX   case '\x0a'
    /// M_DELBS                           case '\x7f'   -> '\x08'
    /// M_BSDEL                           case '\x08'   -> '\x7f'
    /// M_TABHEX                          case '\x09'
    /// M_SPCHEX   c == '\x7f' || ((unsigned char)c < 0x20
    ///                            && c != '\x09' && c != '\x0a' && c != '\x0d')
    /// M_8BITHEX  c & 0x80
    /// M_NRMHEX   (unsigned char)c >= 0x20 && (unsigned char)c < 0x7f
    /// ```
    fn oracle_one(m: Mapping, b: u8) -> Option<Vec<u8>> {
        // picocom's `map2hex`: '[' + hexd[c >> 4] + hexd[c & 0x0f] + ']', lowercase.
        let hex = |c: u8| format!("[{c:02x}]").into_bytes();
        // M_SPCHEX's class, enumerated: C0 minus TAB/LF/CR, plus DEL.
        let special = matches!(b, 0x00..=0x08 | 0x0b | 0x0c | 0x0e..=0x1f | 0x7f);
        // M_NRMHEX's class, stated as "printable ASCII" instead of as a range.
        let normal = b == b' ' || b.is_ascii_graphic();
        match m {
            Mapping::Crlf if b == 0x0d => Some(vec![0x0a]),
            Mapping::Crcrlf if b == 0x0d => Some(vec![0x0d, 0x0a]),
            Mapping::Igncr if b == 0x0d => Some(vec![]),
            Mapping::Lfcr if b == 0x0a => Some(vec![0x0d]),
            Mapping::Lfcrlf if b == 0x0a => Some(vec![0x0d, 0x0a]),
            Mapping::Ignlf if b == 0x0a => Some(vec![]),
            Mapping::Bsdel if b == 0x08 => Some(vec![0x7f]),
            Mapping::Delbs if b == 0x7f => Some(vec![0x08]),
            Mapping::Spchex if special => Some(hex(b)),
            Mapping::Tabhex if b == 0x09 => Some(hex(b)),
            Mapping::Crhex if b == 0x0d => Some(hex(b)),
            Mapping::Lfhex if b == 0x0a => Some(hex(b)),
            Mapping::EightBitHex if b & 0x80 != 0 => Some(hex(b)),
            Mapping::NrmHex if normal => Some(hex(b)),
            _ => None,
        }
    }

    #[test]
    fn every_name_round_trips() {
        for name in Mapping::all_names() {
            let m = Mapping::from_name(name).expect("known name parses");
            assert_eq!(m.name(), name, "name round-trip");
        }
        assert_eq!(Mapping::from_name("bogus"), None);
        assert_eq!(Mapping::all_names().count(), 14, "picocom's 14 mappings");
    }

    #[test]
    fn single_mapping_matches_the_oracle_over_all_256_bytes() {
        // Every mapping against every byte of `0x00..=0xff` — the control range and DEL
        // included, which is what the MAP-1 defect slipped through: the sweep existed,
        // but the oracle it checked against restated this module's wrong `spchex`
        // predicate instead of picocom's. `oracle_one` is now transcribed from upstream.
        for name in Mapping::all_names() {
            let m = Mapping::from_name(name).unwrap();
            let dir = MapDirection::compile(vec![m]);
            for b in 0u8..=255 {
                let mut out = Vec::new();
                let mut fired = 0usize;
                dir.apply(&[b], &mut out, |_| fired += 1);
                match oracle_one(m, b) {
                    Some(expected) => {
                        assert_eq!(out, expected, "{name} on byte {b:#04x}");
                        assert_eq!(fired, 1, "{name} on {b:#04x} must count one substitution");
                    }
                    None => {
                        assert_eq!(out, vec![b], "{name} passes byte {b:#04x} through");
                        assert_eq!(fired, 0, "{name} on {b:#04x} must count nothing");
                    }
                }
            }
        }
    }

    #[test]
    fn first_match_wins_resolves_conflicting_rules() {
        // The design's own example: igncr-before-crlf deletes CR; the reverse
        // translates it. Both rules match CR; list order is the tiebreak (§7.8).
        let del = MapDirection::parse(&["igncr".into(), "crlf".into()]).unwrap();
        let mut out = Vec::new();
        del.apply(b"a\rb", &mut out, |_| {});
        assert_eq!(out, b"ab", "igncr before crlf deletes CR");

        let xlate = MapDirection::parse(&["crlf".into(), "igncr".into()]).unwrap();
        let mut out = Vec::new();
        xlate.apply(b"a\rb", &mut out, |_| {});
        assert_eq!(out, b"a\nb", "crlf before igncr translates CR");
    }

    #[test]
    fn per_rule_counts_attribute_to_the_firing_rule_only() {
        // crlf fires on CR (index 0); igncr never fires (shadowed on CR). A
        // shadowed rule's count stays zero.
        let dir = MapDirection::parse(&["crlf".into(), "igncr".into()]).unwrap();
        let mut counts = vec![0u64; dir.rule_count()];
        let mut out = Vec::new();
        dir.apply(b"\r\r\rx", &mut out, |i| counts[i] += 1);
        assert_eq!(counts, vec![3, 0], "crlf fired 3×, igncr shadowed");
        assert_eq!(out, b"\n\n\nx");
    }

    #[test]
    fn nrmhex_includes_space_matching_picocom() {
        // picocom's nrmhex range is 0x20..=0x7e — space included, so `nrmhex` is the
        // rule that hexes a space (`spchex` does not; see the test below). Pin the
        // space inclusion so a future refactor can't drift.
        let dir = MapDirection::compile(vec![Mapping::NrmHex]);
        let mut out = Vec::new();
        dir.apply(b" A~\t\n", &mut out, |_| {});
        // space->[20], 'A'->[41], '~'->[7e]; tab (0x09) and LF (0x0a) are outside
        // the printable range and pass through.
        assert_eq!(out, b"[20][41][7e]\t\n");
    }

    #[test]
    fn spchex_maps_the_control_class_not_space() {
        // Regression guard for the v11 defect: `spchex` was `b == 0x20`, so the rule an
        // operator reaches for to reveal stray control bytes instead rewrote every space
        // as `[20]` while the hunted bytes still passed through invisibly. Upstream
        // `M_SPCHEX` is DEL plus C0 except TAB/LF/CR (module doc's fidelity note).
        let dir = MapDirection::compile(vec![Mapping::Spchex]);

        let mut out = Vec::new();
        let mut fired = 0usize;
        dir.apply(b"a \x00\x1b\x7f\t\n\rz", &mut out, |_| fired += 1);
        assert_eq!(out, b"a [00][1b][7f]\t\n\rz");
        assert_eq!(fired, 3, "only 0x00, 0x1b and 0x7f are substituted");

        // The defect's signature, stated on its own: SPACE is untouched.
        let mut sp = Vec::new();
        dir.apply(b" ", &mut sp, |_| panic!("spchex must not fire on SPACE"));
        assert_eq!(sp, b" ", "hexing a space is nrmhex's job, not spchex's");
    }

    #[test]
    fn the_hex_family_can_render_every_byte() {
        // §7.8 sells the hex family as "the cheap way to discover which quirk a mystery
        // console actually has", which only holds if the six rules together reach every
        // byte. Before the spchex fix, 0x00..=0x1f and 0x7f were unreachable by *any*
        // rule in the vocabulary, so the advertised capability did not exist.
        let dir = MapDirection::parse(&[
            "spchex".into(),
            "tabhex".into(),
            "crhex".into(),
            "lfhex".into(),
            "nrmhex".into(),
            "8bithex".into(),
        ])
        .unwrap();
        let all: Vec<u8> = (0u8..=255).collect();
        let mut out = Vec::new();
        let mut fired = 0usize;
        dir.apply(&all, &mut out, |_| fired += 1);
        assert_eq!(fired, 256, "every byte is claimed by some hex rule");
        let expected: Vec<u8> = all
            .iter()
            .flat_map(|b| format!("[{b:02x}]").into_bytes())
            .collect();
        assert_eq!(out, expected);
    }

    #[test]
    fn spchex_overlaps_bsdel_and_delbs_with_list_order_deciding() {
        // spchex's class contains BS (0x08) and DEL (0x7f), which bsdel/delbs also
        // match — the only overlaps it has outside the hex family. picocom resolves
        // that with a fixed internal priority; here the operator's list order does
        // (§7.8). Pin both directions so the overlap stays deliberate.
        let hex_first = MapDirection::parse(&["spchex".into(), "delbs".into()]).unwrap();
        let mut out = Vec::new();
        hex_first.apply(b"\x7f", &mut out, |_| {});
        assert_eq!(out, b"[7f]", "spchex before delbs hexes DEL");

        let xlate_first = MapDirection::parse(&["delbs".into(), "spchex".into()]).unwrap();
        let mut out = Vec::new();
        xlate_first.apply(b"\x7f", &mut out, |_| {});
        assert_eq!(out, b"\x08", "delbs before spchex translates DEL");
    }

    #[test]
    fn identity_direction_is_a_verbatim_passthrough() {
        let dir = MapDirection::parse(&[]).unwrap();
        assert_eq!(dir.max_expansion(), 1, "identity never grows");
        let mut out = Vec::new();
        let all: Vec<u8> = (0u8..=255).collect();
        dir.apply(&all, &mut out, |_| panic!("identity fires no rule"));
        assert_eq!(out, all);
    }

    proptest! {
        /// Output is bounded at k × input for any rule set and any input (§7.8),
        /// which is what keeps the interior holdover bounded across the transform.
        #[test]
        fn output_is_bounded_at_k_times_input(
            names in prop::collection::vec(
                prop_oneof![
                    Just("crlf"), Just("crcrlf"), Just("igncr"), Just("lfcr"),
                    Just("lfcrlf"), Just("ignlf"), Just("bsdel"), Just("delbs"),
                    Just("spchex"), Just("tabhex"), Just("crhex"), Just("lfhex"),
                    Just("8bithex"), Just("nrmhex"),
                ].prop_map(String::from),
                0..6,
            ),
            input in prop::collection::vec(any::<u8>(), 0..512),
        ) {
            let dir = MapDirection::parse(&names).unwrap();
            let mut out = Vec::new();
            dir.apply(&input, &mut out, |_| {});
            prop_assert!(
                out.len() <= dir.max_expansion() * input.len(),
                "output {} exceeds k({}) × input {}",
                out.len(), dir.max_expansion(), input.len(),
            );
        }

        /// Statelessness: mapping the two halves of a buffer separately and
        /// concatenating equals mapping the whole — so chunk boundaries never matter
        /// (§5), pinned even though it is true by construction.
        #[test]
        fn chunk_boundaries_are_irrelevant(
            names in prop::collection::vec(
                prop_oneof![
                    Just("crlf"), Just("crcrlf"), Just("igncr"), Just("lfcrlf"),
                    Just("bsdel"), Just("nrmhex"), Just("8bithex"), Just("spchex"),
                ].prop_map(String::from),
                0..5,
            ),
            input in prop::collection::vec(any::<u8>(), 0..256),
            split in 0usize..=256,
        ) {
            let dir = MapDirection::parse(&names).unwrap();
            let at = split.min(input.len());

            let mut whole = Vec::new();
            dir.apply(&input, &mut whole, |_| {});

            let mut split_out = Vec::new();
            dir.apply(&input[..at], &mut split_out, |_| {});
            dir.apply(&input[at..], &mut split_out, |_| {});

            prop_assert_eq!(whole, split_out);
        }
    }
}
