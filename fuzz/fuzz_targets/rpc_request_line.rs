#![no_main]
//! Fuzz the **daemon's front door** — `nexus_rpc::parse_incoming_request`, the parser
//! every byte written to the 0600 control socket passes through (§10, §15.16).
//!
//! Added for review 26 **SEC-7**: the four original targets all sit on the `codec-api`
//! layer, so everything reachable *without a leg* was unfuzzed. This is the largest of
//! those surfaces — it is what a `serialnexusctl`, the web console's bridge, and any
//! `socat`-wielding operator all speak, and unlike the wire it needs no peer daemon and
//! no configuration to reach.
//!
//! Scope, matching how the daemon actually calls it: `RequestLines` (in `nexus-daemon`,
//! see the note in `Cargo.toml`) does the framing — it splits on `\n`, caps the line
//! length, strips one trailing `\r`, and refuses invalid UTF-8 with an `InvalidData`
//! error before this parser ever sees the bytes. So a fuzz case is one UTF-8 line.
//!
//! Invariants beyond "no panic":
//!
//! * every refusal carries one of the **two** codes §10 defines for the door
//!   (`PARSE_ERROR` for "not JSON", `INVALID_REQUEST` for "JSON, but not a request");
//! * a leading `[` is **always** refused — §10's "batch arrays are rejected outright",
//!   which must hold before any parsing effort, not as a consequence of one;
//! * a request that parses re-serializes to a line that parses back to the same
//!   request, so the daemon and its clients cannot disagree about what was asked.

use libfuzzer_sys::fuzz_target;
use nexus_rpc::{error_codes, parse_incoming_request, to_line};

fuzz_target!(|data: &[u8]| {
    let Ok(line) = std::str::from_utf8(data) else {
        return; // the framing layer refuses non-UTF-8 before this parser sees it
    };
    // One line per request: the framing layer has already split on '\n'.
    if line.contains('\n') {
        return;
    }

    let is_batch = line.trim_start().starts_with('[');

    match parse_incoming_request(line) {
        Err(e) => {
            assert!(
                e.code == error_codes::PARSE_ERROR || e.code == error_codes::INVALID_REQUEST,
                "refusal used an undefined code {} for {line:?}",
                e.code
            );
            if is_batch {
                assert_eq!(
                    e.code,
                    error_codes::INVALID_REQUEST,
                    "a batch array must be refused as INVALID_REQUEST, not parsed then \
                     rejected: {line:?}"
                );
            }
        }
        Ok(request) => {
            assert!(
                !is_batch,
                "a batch array parsed as a single request: {line:?}"
            );

            // Re-serialize and re-parse: the round trip must be stable, or a client
            // and the daemon can disagree about the request that was made.
            let round = to_line(&request);
            let round = round.strip_suffix('\n').unwrap_or(&round);
            let again = parse_incoming_request(round)
                .unwrap_or_else(|e| panic!("a re-serialized request did not re-parse: {e:?}"));
            assert_eq!(
                to_line(&again),
                to_line(&request),
                "re-parsing a re-serialized request produced a different request"
            );
        }
    }
});
