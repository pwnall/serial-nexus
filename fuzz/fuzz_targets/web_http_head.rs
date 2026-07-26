#![no_main]
//! The web console's **HTTP request-head parser** and the header checks that gate on
//! it (review 26, SEC-7).
//!
//! This is the most exposed parser in the project. §15.29 sanctions binding the
//! console beyond loopback under `--tls` or `--insecure-bind`, so this code can face
//! an unauthenticated network peer — and it runs *before* the bearer-token gate, which
//! is precisely where a fuzzer belongs. `read_request` is a hand-rolled byte loop
//! (§15.13's ethos, matching the daemon's hand-rolled JSON-RPC) rather than a
//! battle-tested HTTP crate, so it earns its own target.
//!
//! Reached through `serialnexusweb::unstable_fuzz_api`, which exists for exactly this
//! and promises nothing (see that module's docs and implementation-notes §3.19).
//!
//! Also fuzzed here: `split_authority` and `origin_matches_host`, the pure functions
//! behind the Origin/Host gate the review added (SEC-3/WEB-7). They are unit-tested
//! against the cases we thought of; this covers the ones we did not, and they are
//! reachable with attacker-chosen header values on every request.

use libfuzzer_sys::fuzz_target;
use serialnexusweb::unstable_fuzz_api::{
    MAX_HEAD, Request, origin_matches_host, read_request, split_authority,
};

fuzz_target!(|data: &[u8]| {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("current-thread runtime");

    rt.block_on(async {
        // `read_request` reads a byte at a time from any `AsyncRead`; a slice is a
        // faithful stand-in for the socket, including the "stream ends mid-head" case
        // that a network peer can always produce.
        let mut src = data;
        // The `Request` annotation is deliberate rather than inferred: the meta-gate in
        // `nexus-itest/tests/meta_gates.rs` holds every `unstable_fuzz_api` re-export to
        // having a target that names it, so the API widening stays exactly as large as
        // the fuzzing it was granted for.
        let parsed: Result<Option<Request>, _> = read_request(&mut src).await;
        match parsed {
            Ok(Some(req)) => {
                // The head is bounded: everything the parser retains comes out of a
                // buffer it refuses to grow past MAX_HEAD, so no single field — and no
                // sum of them — can exceed it.
                let total: usize = req.method.len()
                    + req.path.len()
                    + req.query.len()
                    + req
                        .headers
                        .iter()
                        .map(|(k, v)| k.len() + v.len())
                        .sum::<usize>();
                assert!(
                    total <= MAX_HEAD,
                    "parsed head retains {total} bytes, above the {MAX_HEAD} cap"
                );
                // A header name that kept its colon would make `header()` — and every
                // gate built on it (Host, Origin, Cookie, Sec-WebSocket-Key) — match
                // something the client never sent.
                for (k, _) in &req.headers {
                    assert!(!k.contains(':'), "header name kept its separator: {k:?}");
                    assert_eq!(k.trim(), k, "header name kept surrounding whitespace");
                }
                // The path/query split is exact: neither side may carry the `?`.
                assert!(!req.path.contains('?'), "path kept the query separator");

                // The Origin/Host gate, driven with whatever the client actually sent.
                // The property that matters is total-ness: these run on every request,
                // so a panic here is a remote crash before authentication.
                let host = req.header("host").unwrap_or_default().to_owned();
                let origin = req.header("origin").unwrap_or_default().to_owned();
                let _ = origin_matches_host(&origin, &host, false);
                let _ = origin_matches_host(&origin, &host, true);
                let _ = split_authority(&host);
            }
            // A truncated head (clean EOF) and an over-cap or malformed one are both
            // ordinary outcomes — the target is asserting the absence of panics and
            // the invariants above, not that arbitrary bytes parse.
            Ok(None) | Err(_) => {}
        }
    });
});
