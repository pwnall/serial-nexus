//! The bounded identity set (LEG-2 / CODEC-1): the one place a wire-supplied
//! channel identity is capped, deduplicated and truncated.
//!
//! Two node kinds hold a list of channel identities that **did not come from the
//! operator's configuration**: the leg remembers what a peer announced but this
//! side does not declare (§8), and the codec node — with the exec codec sharing
//! its one instance — remembers what a transform decoded that the node is not
//! configured for (§5, "loss is always visible and attributable"). The
//! provenances differ; the exposure does not. In both, an identity arrives from
//! off-box at wire rate, in any length, in unlimited variety, on the single
//! runtime thread. So both need the same four properties, and this module is
//! where they are written:
//!
//! 1. **Capped in count** ([`MAX_IDENTITIES`]) — an uncapped `Vec` is an
//!    unbounded allocation driven by whatever is on the wire, and its linear
//!    dedup scan is O(n²) work on the thread that also moves bytes. Hostile
//!    peers are in scope (`p6_hostility`), and a `listen`+`unix` leg is dialable
//!    by anyone who can reach its path.
//! 2. **Capped per identity** ([`MAX_IDENTITY_LEN`]) — identities an operator
//!    writes are short; the wire admits far longer ones, and `state` needs only
//!    enough to recognize what arrived.
//! 3. **Truncation is marked and lands on a `char` boundary** — `state` must
//!    never show a shortened name as though it were the real one, and the
//!    identity is externally-supplied UTF-8, so a split code point would not
//!    survive JSON.
//! 4. **Insertion-ordered, with refusals counted** — the order is what `state`
//!    reports, so two snapshots diff meaningfully; the earliest identities (the
//!    ones an operator is most likely acting on) are the ones kept, and what the
//!    cap refused is a counter rather than a silence.
//!
//! Both kinds spelled all four out, separately and identically, which is two
//! chances for a fix to land once (plan §18 items 55(f), 59(d)). They now spell
//! them here and wrap what differs: the leg's `insert`/`clear`, the codec's
//! `record`/`report_into` with its byte count and its first-sighting WARN. The
//! **reported counter names stay per-kind** — a leg reports `unbound_overflow`
//! beside `binding: "unbound"` channels, a codec reports
//! `discarded_unconfigured_channel` / `unconfigured_channels` /
//! `unconfigured_overflow` — because those name what the node observed, not how
//! it remembered it, and they are `state`'s reported surface, where a rename is a
//! break rather than a tidy-up.

use std::borrow::Cow;
use std::collections::HashSet;

/// The largest number of *distinct* wire-supplied identities one set remembers.
/// A few hundred is far past the point where a human reads the list, and the
/// list exists to prompt a human.
pub(crate) const MAX_IDENTITIES: usize = 256;

/// The largest stored length of one such identity, in bytes, before the marker.
pub(crate) const MAX_IDENTITY_LEN: usize = 64;

/// Appended to a truncated identity, so `state` never implies the shortened name
/// is the one that arrived.
pub(crate) const TRUNCATION_MARKER: &str = "…(truncated)";

/// A capped, insertion-ordered set of wire-supplied identities: the `Vec` is what
/// `state` reports, the parallel `HashSet` makes the per-frame membership test
/// O(1) rather than a scan of the cap.
#[derive(Default)]
pub(crate) struct IdentitySet {
    order: Vec<String>,
    seen: HashSet<String>,
    /// Occurrences the cap refused to record — *not* distinct identities, which
    /// cannot be counted without remembering them, which is the thing being
    /// bounded. A repeat of an already-recorded identity is not an overflow.
    overflow: u64,
}

impl IdentitySet {
    /// Record `id`, truncated and capped.
    ///
    /// Returns the stored identity on a **first sighting** and `None` for a
    /// repeat or for an occurrence the cap refused. That return is the only hook
    /// a caller needs to log once per identity: the dedup *is* the rate limit, so
    /// an unconfigured channel screaming at 1 MB/s costs exactly one line, and no
    /// second mechanism exists to fall out of step with the list.
    pub(crate) fn insert(&mut self, id: &str) -> Option<&str> {
        let id = truncate_identity(id);
        if self.seen.contains(id.as_ref()) {
            return None;
        }
        if self.order.len() >= MAX_IDENTITIES {
            self.overflow += 1;
            return None;
        }
        let id = id.into_owned();
        self.seen.insert(id.clone());
        self.order.push(id);
        self.order.last().map(String::as_str)
    }

    /// Forget everything, the overflow count included — it describes the same
    /// connection the identities do.
    pub(crate) fn clear(&mut self) {
        self.order.clear();
        self.seen.clear();
        self.overflow = 0;
    }

    /// The recorded identities in arrival order — what `state` reports.
    pub(crate) fn identities(&self) -> &[String] {
        &self.order
    }

    /// Occurrences the cap refused to record.
    pub(crate) fn overflow(&self) -> u64 {
        self.overflow
    }

    /// The dedup index's size, which a test asserts against
    /// [`Self::identities`]' to prove the two halves cannot drift apart.
    #[cfg(test)]
    pub(crate) fn seen_len(&self) -> usize {
        self.seen.len()
    }

    /// Plant an overflow count, so a test can prove the figure reaches `state`
    /// without driving a cap's worth of identities through the wire path.
    #[cfg(test)]
    pub(crate) fn set_overflow(&mut self, n: u64) {
        self.overflow = n;
    }
}

/// Bound the stored length of a wire-supplied identity, marking a truncation
/// explicitly. Truncation lands on a `char` boundary; a short identity is
/// borrowed untouched, so the common case allocates nothing.
pub(crate) fn truncate_identity(id: &str) -> Cow<'_, str> {
    if id.len() <= MAX_IDENTITY_LEN {
        return Cow::Borrowed(id);
    }
    let mut end = MAX_IDENTITY_LEN;
    while end > 0 && !id.is_char_boundary(end) {
        end -= 1;
    }
    Cow::Owned(format!("{}{TRUNCATION_MARKER}", &id[..end]))
}
