#![forbid(unsafe_code)]

//! **`docs/codec-authors.md` is executed, not proofread** (plan §18 item 37).
//!
//! The author guide makes byte-level and configuration claims — four frozen golden
//! vectors as hex, one worked frame derived by hand, a TOML block annotated
//! "verified against the daemon", and a closing sentence enumerating the counters a
//! codec node reports. Every one of those was verified exactly once, by the hand that
//! wrote it. Plan §18 item 1's rule — *a number in a non-prose file is a claim* —
//! extended here to author-facing prose: these guards **parse the document** and check
//! it against `encode()`, against a running daemon, and against the state schema, so
//! the doc cannot drift away from the tree it teaches.
//!
//! The tell that this is a gate and not a decoration: mutate one hex digit in the
//! table, rename one counter in the daemon, or drop `write_mode` from the doc's
//! multiplexed edge, and one of these tests reds. Nothing here hard-codes the doc's
//! content — the expectations are *derived from the document*, which is the only way
//! a gate can notice an edit nobody told it about (AGENTS §3's derive-from-tools
//! doctrine, applied to prose).
//!
//! **The one thing rewritten before loading**: the doc's TOML names three absolute
//! paths — a device, two pty paths under `/run` — and one workspace-relative `argv`.
//! A test cannot open `/dev/ttyUSB0` (it may be a real adapter — this box has two) or
//! create `/run/serial_nexus/…` (root), so those four strings are rewritten into the
//! test's temp directory and named here. Everything else is the document's bytes:
//! every node type, key, `faces`, `channels`, `write_mode`, and the whole attributes
//! table load exactly as printed.

use std::path::PathBuf;

use serde_json::Value;
use serial_nexus_codec_api::{Event, encode};
use serial_nexus_itest::Daemon;

/// The author guide, from this crate's compile-time manifest dir (`itest/`), so the
/// path is checkout- and platform-independent.
fn doc_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("serial-nexus-itest has a parent (the workspace root)")
        .join("docs")
        .join("codec-authors.md")
}

fn doc() -> String {
    let p = doc_path();
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// Parse one of the doc's event expressions — `data("console", "hi")`,
/// `open("trace")`, `error("c0", "boom")` — into the [`Event`] it names.
///
/// Deliberately a parser rather than a lookup table: a table would be a hand-kept
/// list of the doc's own contents, and the first row someone added to the doc would
/// be the row the gate never checked.
fn parse_event(expr: &str) -> Event {
    let expr = expr.trim();
    let open = expr
        .find('(')
        .unwrap_or_else(|| panic!("no `(` in {expr:?}"));
    let kind = &expr[..open];
    let inner = expr[open + 1..]
        .strip_suffix(')')
        .unwrap_or_else(|| panic!("no closing `)` in {expr:?}"));

    // Split the argument list on the comma *between* the quoted strings, then strip
    // the quotes. Every argument in this document is a plain quoted literal.
    let args: Vec<String> = inner
        .split("\", \"")
        .map(|a| a.trim().trim_matches('"').to_owned())
        .collect();

    match (kind, args.as_slice()) {
        ("data", [channel, payload]) => Event::data(channel.as_str(), payload.clone().into_bytes()),
        ("open", [channel]) => Event::open(channel.as_str()),
        ("close", [channel]) => Event::close(channel.as_str()),
        ("error", [channel, reason]) => Event::error(channel.as_str(), reason.clone()),
        _ => panic!("unrecognised event expression {expr:?} in the author guide"),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// (1) The four frozen golden vectors in `docs/codec-authors.md` §3 are the bytes
/// `encode()` produces — every row of the table, parsed from the table.
///
/// The vectors are frozen in `serial-nexus-codec-api`'s own `golden_vectors` test; what
/// nothing checked until now is that the *document* reproduces them. A drifted doc is
/// worse than a missing one: an author implements against it, and the mismatch shows
/// up as their codec failing a battery that is right.
#[test]
fn the_documented_golden_vectors_are_the_bytes_encode_produces() {
    let doc = doc();
    let mut rows = 0usize;
    for line in doc.lines() {
        // `| `data("console", "hi")`   | `0000000c…`   |`
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').collect();
        if cells.len() != 2 {
            continue;
        }
        let expr = cells[0].trim().trim_matches('`');
        let want = cells[1].trim().trim_matches('`');
        // Only the golden-vector table's rows look like this: an event expression on
        // the left and a lowercase hex string on the right.
        if !expr.contains('(')
            || want.is_empty()
            || !want
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            continue;
        }

        let event = parse_event(expr);
        let mut got = Vec::new();
        encode(&event, &mut got).expect("the documented event must encode");
        assert_eq!(
            hex(&got),
            want,
            "docs/codec-authors.md's golden vector for {expr} disagrees with encode(): \
             the document says {want}, the envelope produces {}. These bytes are frozen \
             (§15.15) — if the change is deliberate it is a breaking envelope change, \
             and the vectors move in `serial-nexus-codec-api`'s golden_vectors test first",
            hex(&got)
        );
        rows += 1;
    }
    assert_eq!(
        rows, 4,
        "expected the author guide's four frozen golden vectors; found {rows}. A gate \
         that checks zero rows passes vacuously, so the count is asserted too"
    );
}

/// (2) The worked `data("", "AB")` example — the one frame the doc derives by hand
/// rather than quoting from the frozen table — is also the bytes `encode()` produces.
///
/// It is the empty-channel frame, so it is the example an exec-codec author reads
/// most closely: it is how the multiplexed side looks on the wire (§8 clause 13).
#[test]
fn the_worked_empty_channel_example_is_the_bytes_encode_produces() {
    let doc = doc();
    // The doc introduces it as: Worked example — `data("", "AB")`, …
    // and concludes with: → `000000050000004142`.
    let intro = doc
        .find("Worked example — `")
        .expect("the author guide's worked example is gone — §4 derives it by hand");
    let rest = &doc[intro..];
    let expr_start = rest.find('`').expect("worked example has no expression") + 1;
    let expr_end = expr_start
        + rest[expr_start..]
            .find('`')
            .expect("worked example expression is unterminated");
    let expr = &rest[expr_start..expr_end];

    let arrow = rest
        .find("→ `")
        .expect("the worked example states no resulting byte string");
    let hex_start = arrow + "→ `".len();
    let hex_end = hex_start
        + rest[hex_start..]
            .find('`')
            .expect("the worked example's byte string is unterminated");
    let want = &rest[hex_start..hex_end];

    let event = parse_event(expr);
    let mut got = Vec::new();
    encode(&event, &mut got).expect("the worked example must encode");
    assert_eq!(
        hex(&got),
        want,
        "docs/codec-authors.md's worked {expr} example disagrees with encode(): the \
         document derives {want}, the envelope produces {}",
        hex(&got)
    );
    // …and the field-by-field breakdown above it must agree with the same frame, so a
    // corrected hex string cannot leave a stale body_len in the annotation.
    let body_len = u32::from_be_bytes([got[0], got[1], got[2], got[3]]);
    assert!(
        rest[..arrow].contains(&format!("body_len = {body_len}")),
        "the worked example's field breakdown does not state body_len = {body_len}"
    );
}

/// The doc's TOML block from §5, with the four paths a test cannot use rewritten into
/// `dir`. Returns the config plus the rewritten pty paths, so the caller can assert
/// against them.
fn wiring_toml(dir: &std::path::Path) -> String {
    let doc = doc();
    let heading = doc
        .find("### The TOML to wire it")
        .expect("the author guide's wiring section is gone");
    let rest = &doc[heading..];
    let start = rest
        .find("```toml\n")
        .expect("the wiring section has no TOML block")
        + "```toml\n".len();
    let end = start
        + rest[start..]
            .find("\n```")
            .expect("the wiring section's TOML block is unterminated");
    let block = &rest[start..end];

    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("tests")
        .join("ext-codec")
        .join("passthrough-codec.py");

    // The four rewrites, and only these four (each asserted to have applied, so a
    // doc edit that renames one cannot silently leave the original path in place).
    let rewrites = [
        (
            "\"/dev/ttyUSB0\"",
            format!("{:?}", dir.join("absent-device").display().to_string()),
        ),
        (
            "\"/run/serial_nexus/console\"",
            format!("{:?}", dir.join("console").display().to_string()),
        ),
        (
            "\"/run/serial_nexus/trace\"",
            format!("{:?}", dir.join("trace").display().to_string()),
        ),
        (
            "\"tests/ext-codec/passthrough-codec.py\"",
            format!("{:?}", script.display().to_string()),
        ),
    ];
    let mut out = block.to_owned();
    for (from, to) in &rewrites {
        assert!(
            out.contains(from),
            "the author guide's TOML no longer contains {from} — this gate rewrites \
             exactly four strings and every one of them must still be there, or it is \
             loading something other than what the document prints"
        );
        out = out.replace(from, to);
    }
    out
}

/// (3) The doc's TOML block loads against a real daemon — the claim the block itself
/// makes ("The following config loads and runs (verified against the daemon)").
///
/// This is the whole §5 example: an exec codec node with `faces`, `channels`, an
/// `attributes` table with `argv`/`restart_backoff_ms`/`env`, two pty consumers, and
/// three edges — including the multiplexed `write_mode = "held"` the surrounding prose
/// calls "not optional". If any of that stops being loadable, an author following the
/// document meets the failure before the document does.
#[test]
fn the_documented_wiring_toml_loads_against_the_daemon() {
    let daemon = Daemon::start();
    let dir = daemon.run().path().to_owned();
    let toml_cfg = wiring_toml(&dir);

    daemon
        .rpc()
        .load_toml(&toml_cfg, false)
        .unwrap_or_else(|e| panic!("the author guide's TOML did not load: {e:?}\n{toml_cfg}"));

    let state = daemon.rpc().state();
    for node in ["modem", "mux", "console-pty", "trace-pty"] {
        assert!(
            daemon.rpc().node(node).is_some(),
            "the documented config created no {node:?} node: {state}"
        );
    }
}

/// (4) …and the multiplexed edge's `write_mode` is load-bearing exactly as the doc
/// says: strip it, and the same config is refused **naming the edge**. This is the
/// fail-first proof for (3) — without it, (3) would pass against a daemon that had
/// quietly stopped enforcing the rule the document spends a paragraph on.
#[test]
fn dropping_the_documented_write_mode_is_refused_naming_the_edge() {
    let daemon = Daemon::start();
    let dir = daemon.run().path().to_owned();
    // Only the multiplexed edge carries `held`; the channel edges are on-demand/never.
    let stripped = wiring_toml(&dir).replace("write_mode = \"held\"", "");

    let err = daemon
        .rpc()
        .load_toml(&stripped, false)
        .expect_err("an on-demand-by-omission origin into a codec's multiplexed side is refused");
    let text = format!("{err:?}");
    assert!(
        text.contains("mux"),
        "the refusal must name the offending edge, as the author guide promises: {text}"
    );
}

/// (5) Every counter the doc's closing sentence names is a counter a codec node
/// actually reports.
///
/// The names are scraped from the document, not listed here: a hand-kept list would
/// be the thing that drifts. Both codec kinds are loaded, because the sentence spans
/// both — an exec node for the node/per-channel counters, an in-process `reference`
/// node for the `demux_errors` / `last_demux_error` pair it says an in-process codec
/// "additionally carries".
#[test]
fn every_counter_the_doc_names_is_one_a_codec_node_reports() {
    let doc = doc();
    // The paragraph is located by the command it tells the author to run, not by a
    // prose fragment: prose re-wraps, `serial-nexus-ctl state` does not.
    let sentence = doc
        .split("\n\n")
        .find(|para| para.contains("`serial-nexus-ctl state`"))
        .expect("the author guide's counter paragraph is gone");

    // Backticked snake_case identifiers: the counter names, and nothing else in this
    // sentence looks like one (`serial-nexus-ctl` and `--json` are hyphenated).
    let mut named: Vec<String> = Vec::new();
    for piece in sentence.split('`').skip(1).step_by(2) {
        let piece = piece.trim();
        let looks_like_a_counter = piece.contains('_')
            && piece
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if looks_like_a_counter && !named.iter().any(|n| n == piece) {
            named.push(piece.to_owned());
        }
    }
    assert!(
        named.len() >= 8,
        "expected the author guide to name at least eight codec counters; scraped \
         {named:?} — a gate that checks nothing passes vacuously"
    );

    let daemon = Daemon::start();
    let dir = daemon.run().path().to_owned();
    daemon
        .rpc()
        .load_toml(&wiring_toml(&dir), false)
        .expect("the author guide's TOML loads");
    // The in-process half of the sentence.
    daemon
        .rpc()
        .add_node_toml(
            "[[node]]\ntype = \"codec\"\nname = \"inproc\"\ncodec = \"reference\"\nchannels = [\"c0\"]\n",
        )
        .expect("an in-process reference codec node loads");

    let state = daemon.rpc().state();
    let rendered = state.to_string();
    for counter in &named {
        assert!(
            rendered.contains(&format!("\"{counter}\"")),
            "docs/codec-authors.md names the counter `{counter}`, which no codec node \
             in a graph carrying both codec kinds reports. Either the counter was \
             renamed and the document was not, or the document invented it.\n{}",
            serde_json::to_string_pretty(&state).unwrap_or_default()
        );
    }
    let _ = Value::Null;
}
