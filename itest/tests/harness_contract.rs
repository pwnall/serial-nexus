//! Harness self-checks that need a live daemon (design §5's anti-tautology rule: a
//! broken harness must fail loudly, never pass vacuously).
//!
//! The three cases the harness itself can get wrong live in `serial_nexus_itest`'s own
//! unit tests, over a socket pair, because they need a peer that misbehaves on purpose.
//! This one needs the opposite — a *correct* daemon, answering a stream request it is
//! right to refuse — so it lives here.

use serde_json::json;
use serial_nexus_itest::Daemon;

/// **Review 37, 37-TEST-4.** [`serial_nexus_itest::Rpc::stream`] used to swallow the
/// stream verb's ack with `let _ =`, so a daemon that *refused* the subscribe or the
/// tap handed back a live-looking [`serial_nexus_itest::Subscription`] that then yielded
/// nothing. The test downstream reported "timed out", which sends diagnosis to the tap
/// pipeline, the poll loop, the runtime — everywhere except the refusal that was
/// already sitting in the discarded line.
///
/// A refusal is easy to arrange honestly: no graph is loaded, so there is no
/// host-facing endpoint to tap and `tap.open` answers with an error, exactly as §10
/// says it should.
#[test]
#[should_panic(expected = "refused by the daemon")]
fn a_stream_the_daemon_refuses_fails_loudly_rather_than_timing_out_later() {
    let d = Daemon::start();
    let _sub = d
        .rpc()
        .stream("tap.open", json!({ "endpoint": "no-such-endpoint" }));
}

/// The positive control for the guard above: an accepted stream still works, so the
/// ack parsing rejects refusals rather than everything.
#[test]
fn an_accepted_stream_still_opens() {
    let d = Daemon::start();
    let mut sub = d.rpc().subscribe();
    // The daemon publishes a state snapshot on a tick, and `subscribe` is what turns
    // that tick from a no-op into traffic (§10), so a notification arriving at all is
    // proof the stream is live rather than a `Subscription` over a dead ack.
    let note = sub
        .wait_for(std::time::Duration::from_secs(10), |v| {
            v.get("method").and_then(|m| m.as_str()) == Some("state")
        })
        .expect("subscribe yielded no state notification within 10s");
    assert!(
        note.get("params").is_some(),
        "a state notification must carry params: {note}"
    );
}
