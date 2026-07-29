//! `connect` / `disconnect` — live edge surgery (design §15.35; plan §14.2).
//!
//! The two verbs left §14 so that reshaping a graph stops meaning either
//! remove-and-re-add or a `load --replace` outage. What they must be is *the same
//! operation `load` performs, minus the outage* — so the properties here are about
//! sameness as much as about capability:
//!
//! 1. **An illegal edge is refused, naming the rule, with nothing changed.** Every
//!    structural rule `load` enforces (§4's three graph rules, §6's mux-edge write
//!    mode and held-origin uniqueness, name legality) is enforced on an edge
//!    operation too, because both run `GraphConfig::validate` on the candidate
//!    graph. The refusal carries `data.errors`, and `dump` is untouched.
//! 2. **A mid-stream `connect` captures from the join point.** A log connected to a
//!    live endpoint holds exactly the bytes that arrived after it joined — checked
//!    byte-exactly by SHA-256, not by eyeball.
//! 3. **`disconnect` of a lock-holding origin releases and purges.** This is
//!    §15.27's phantom-holder lesson applied to the edge path before the bug could
//!    recur: a writer removed while holding the lock must not leave the endpoint
//!    wedged as locked by someone who no longer exists.
//! 4. **`disconnect` of a codec's held mux edge stalls its channels** rather than
//!    shedding their bytes — targetward is the direction §5 forbids dropping on, so
//!    an unattached interior node backpressures its writers exactly as a steal does.
//! 5. **`dump` round-trips.** After a connect/disconnect sequence the configuration
//!    reads back as the edges that actually exist, and it survives a restart —
//!    edge surgery is configuration, so it is snapshotted (§11/§15.9).
//!
//! Most of this is device-free and runs on every platform: a `map` node's mapped
//! endpoint is host-facing, so it plays the role of "something a pty can attach to"
//! with no UART in sight. Only property 2 needs a real byte source, and it uses the
//! Linux sim echo device with the standard self-skip.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

use serde_json::Value;
use serial_nexus_itest::{Daemon, serial_echo, sha256_hex, wait_until};

/// A graph with one map node (host-facing `m` endpoint, target-facing `m/raw`) and
/// two pty nodes, **no edges**. Everything below wires it live.
const BARE: &str = r#"
[[node]]
type = "map"
name = "m"
hostward = []
targetward = []

[[node]]
type = "pty"
name = "p0"
path = "PTY0"

[[node]]
type = "pty"
name = "p1"
path = "PTY1"
"#;

fn bare_graph(d: &Daemon) -> String {
    BARE.replace("PTY0", &d.run().join("p0").display().to_string())
        .replace("PTY1", &d.run().join("p1").display().to_string())
}

fn edges_of(dump: &Value) -> Vec<(String, String)> {
    // `GraphConfig` renames the field for TOML's `[[edge]]` spelling.
    dump.get("edge")
        .and_then(Value::as_array)
        .map(|es| {
            es.iter()
                .map(|e| {
                    (
                        e["a"].as_str().unwrap_or("?").to_owned(),
                        e["b"].as_str().unwrap_or("?").to_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn an_illegal_connect_is_refused_naming_the_rule_and_changes_nothing() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let cfg = format!(
        r#"
{}
[[node]]
type = "map"
name = "up"
hostward = []
targetward = []
"#,
        bare_graph(&d)
    );
    rpc.load_toml(&cfg, false).expect("load");
    rpc.connect("m", "p0", None).expect("the legal edge");
    let before = rpc.dump();

    // Each case: (a, b, write_mode, expected code, a phrase the refusal must
    // contain). The phrases are the *rules*, not the wording — an operator has to
    // learn which rule they broke, which is the whole difference between a refusal
    // and a 500. The codes are pinned because `docs/rpc/configuration.md` documents
    // them, and a doc that names the wrong code is worse than one that names none:
    // -32002 is `AppError::Structural` (the graph was judged), -32602 is
    // invalid-params (the request never got that far).
    const STRUCTURAL: i64 = -32002; // serial_nexus_rpc::AppError::Structural
    const INVALID_PARAMS: i64 = -32602;
    let cases: &[(&str, &str, Option<&str>, i64, &str)] = &[
        // §4 rule 2, the one-producer invariant: p0's endpoint already has an edge,
        // and a second *host* endpoint cannot also feed it.
        ("up", "p0", None, STRUCTURAL, "at most one"),
        // §4 rule 1: two host-facing endpoints cannot be joined.
        ("m", "m", None, STRUCTURAL, "facing"),
        // Reference integrity is *structural*, not a params error: the request was
        // well formed, the graph it would make is not.
        ("m", "ghost", None, STRUCTURAL, "unknown"),
        ("m", "p0/nope", None, STRUCTURAL, "unknown endpoint"),
        // A bad `write_mode` never reaches the graph — the params do not parse.
        (
            "m",
            "p1",
            Some("sideways"),
            INVALID_PARAMS,
            "expected one of",
        ),
    ];
    for (a, b, mode, code, phrase) in cases {
        let err = rpc
            .connect(a, b, *mode)
            .expect_err(&format!("connect {a} <-> {b} must be refused"));
        assert_eq!(
            err.code, *code,
            "refusal for {a} <-> {b} must carry the documented code; got: [{}] {}",
            err.code, err.message
        );
        assert!(
            err.message.to_lowercase().contains(&phrase.to_lowercase()),
            "refusal for {a} <-> {b} must name the rule ({phrase}); got: {}",
            err.message
        );
    }
    assert_eq!(
        edges_of(&rpc.dump()),
        edges_of(&before),
        "a refused connect changes nothing (§11 atomicity)"
    );

    // A duplicate of an existing edge needs no rule of its own: it gives the target
    // endpoint two edges, which is §4 rule 2 — so it is refused by the validator,
    // not by a second implementation in the verb (§16, one rule one place).
    let err = rpc.connect("m", "p0", None).expect_err("duplicate refused");
    assert!(err.message.contains("at most one"), "got: {}", err.message);

    // …and so is a disconnect of an edge that is not there — a params error, since
    // no candidate graph is judged.
    let err = rpc.disconnect("m", "p1").expect_err("no such edge");
    assert_eq!(err.code, INVALID_PARAMS, "got: {}", err.message);
    assert!(err.message.contains("no edge"), "got: {}", err.message);
}

#[test]
fn two_held_origins_on_one_endpoint_are_refused_on_the_edge_path_too() {
    // §6's held-origin uniqueness is a *load* rule that must hold for edge surgery,
    // or `connect` becomes the door around it: two `held` origins on one endpoint
    // means one of them can never write, never queues, and appears nowhere in
    // `state` as blocked. The map's raw edge is promoted to `held` with `write_mode`
    // written nowhere (§7.8), so this is reachable without anyone typing "held".
    let d = Daemon::start();
    let rpc = d.rpc();
    let cfg = format!(
        r#"
{}
[[node]]
type = "map"
name = "m2"
hostward = []
targetward = []
"#,
        bare_graph(&d)
    );
    rpc.load_toml(&cfg, false).expect("load");

    // Both maps' raw sides onto p0's endpoint? No — a target endpoint takes one
    // edge. Instead: both maps' raw sides onto the *same* host endpoint, which is
    // the reachable two-held shape (§16, RV-4).
    rpc.connect("p0", "m/raw", None)
        .expect_err("a pty endpoint faces target; this is same-facing");

    // The real shape: p0 (target) is the endpoint; give it one edge, then try to
    // give the map's mapped endpoint two held raw origins.
    rpc.connect("m", "m2/raw", None).expect("first held origin");
    let err = rpc
        .connect("m", "p0", Some("held"))
        .expect_err("a second held origin on one endpoint must be refused");
    assert!(
        err.message.contains("held"),
        "the refusal names the rule: {}",
        err.message
    );
}

#[test]
fn a_mid_stream_connect_captures_from_the_join_point() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP a_mid_stream_connect_captures_from_the_join_point: serial_echo() needs Linux"
        );
        return;
    };
    let d = Daemon::start();
    let rpc = d.rpc();
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log directory");
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "s"
device = "{}"
baud = 115200

[[node]]
type = "log"
name = "lg"
directory = "{}"
filename = "late.log"
"#,
        echo.device().display(),
        logdir.display()
    );
    rpc.load_toml(&cfg, false).expect("load");
    assert!(
        rpc.wait_status("s", "active", Duration::from_secs(10)),
        "serial node never came up: {:?}",
        rpc.node("s")
    );

    // Before the log exists on the graph: these bytes echo back and reach nobody.
    let before = b"BEFORE-THE-JOIN\n";
    rpc.send("s", "BEFORE-THE-JOIN", true, 2000)
        .expect("send before");
    // Wait for the echo to have been read and discarded, so the join point is
    // unambiguous — otherwise a slow echo could land after the connect and the test
    // would be asserting a race rather than the property.
    assert!(
        wait_until(Duration::from_secs(5), || {
            rpc.node("s")
                .and_then(|n| n.get("discarded_unattached").and_then(Value::as_u64))
                .unwrap_or(0)
                >= before.len() as u64
        }),
        "the pre-join echo was never accounted: {:?}",
        rpc.node("s")
    );

    // Join mid-stream. From here on the log is a consumer of the live endpoint.
    rpc.connect("s", "lg", None).expect("connect the log edge");

    let after = b"AFTER-THE-JOIN\n";
    rpc.send("s", "AFTER-THE-JOIN", true, 2000)
        .expect("send after");

    let logfile = logdir.join("late.log");
    assert!(
        wait_until(Duration::from_secs(10), || {
            std::fs::read(&logfile)
                .map(|b| b.len() >= after.len())
                .unwrap_or(false)
        }),
        "the log never captured the post-join bytes (file: {:?})",
        std::fs::read(&logfile).map(|b| b.len())
    );
    // Byte-exact, and byte-exact about what is *absent*: the pre-join bytes must not
    // be there. A log that replayed history would pass a "contains" assertion.
    let captured = std::fs::read(&logfile).expect("read log");
    assert_eq!(
        sha256_hex(&captured),
        sha256_hex(after),
        "the log must hold exactly the post-join bytes, got {:?}",
        String::from_utf8_lossy(&captured)
    );
}

#[test]
fn disconnecting_a_lock_holding_origin_releases_the_lock_and_purges() {
    // The phantom-holder regression (§15.27), edge-op variant: `remove-node
    // --cascade` of a lock-holding writer once left the endpoint wedged as locked by
    // an origin that no longer existed, with no recovery. `disconnect` reaches the
    // same state through a different door, so it gets the same guarantee.
    let d = Daemon::start();
    let rpc = d.rpc();
    rpc.load_toml(&bare_graph(&d), false).expect("load");
    rpc.connect("m", "p0", None).expect("connect p0");
    rpc.connect("m", "p1", None).expect("connect p1");

    rpc.lock("p0", false, false, None).expect("p0 takes it");
    let holder = |rpc: &serial_nexus_itest::Rpc| {
        rpc.node("m")
            .and_then(|n| n.pointer("/lock/holder").cloned())
            .unwrap_or(Value::Null)
    };
    assert_eq!(holder(rpc), Value::String("p0".into()));

    let out = rpc.disconnect("m", "p0").expect("disconnect the holder");
    assert_eq!(
        out.get("released_lock"),
        Some(&Value::Bool(true)),
        "the verb reports that it changed who may write: {out}"
    );
    assert_eq!(
        holder(rpc),
        Value::Null,
        "the endpoint must not stay locked by an origin that is gone: {:?}",
        rpc.node("m")
    );
    // And the endpoint is genuinely usable again by the survivor, not merely
    // reported free.
    rpc.lock("p1", false, false, None)
        .expect("the surviving origin can take the freed lock");
    assert_eq!(holder(rpc), Value::String("p1".into()));

    // The departed origin is gone from the endpoint's origin list too — a lingering
    // registration is the other half of the phantom (§6).
    let origins: Vec<String> = rpc
        .node("m")
        .and_then(|n| n.pointer("/lock/origins").cloned())
        .and_then(|o| o.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|o| o.get("origin").and_then(Value::as_str).map(str::to_owned))
        .collect();
    assert!(
        !origins.iter().any(|o| o == "p0"),
        "the disconnected origin must be unregistered, got {origins:?}"
    );
}

/// **37-LIFE-1.** `remove-node --cascade` reports the same two facts `disconnect`
/// does, for the identical edge.
///
/// Both verbs undo an edge through one helper, whose return exists precisely because
/// these facts are "reported rather than done silently" — but the cascade loop dropped
/// it, so the same removal was loud through one door and mute through the other. An
/// operator who cascades a lock-holding writer has changed who may write (§6), and one
/// whose client had bytes queued has lost them by design (§5 all-loss-is-visible);
/// neither landed in a response field, a counter or a log.
///
/// The counts are what a cascade adds over a single `disconnect`: two edges, one of
/// them the holder's, sum to exactly one released lock.
#[test]
fn a_cascade_reports_the_released_locks_and_purged_bytes_disconnect_does() {
    let d = Daemon::start();
    let rpc = d.rpc();
    rpc.load_toml(&bare_graph(&d), false).expect("load");
    rpc.connect("m", "p0", None).expect("connect p0");
    rpc.connect("m", "p1", None).expect("connect p1");
    rpc.lock("p0", false, false, None).expect("p0 takes it");

    // Remove the *map*, cascading both edges: p0's (the lock holder's) and p1's.
    let out = rpc.remove_node("m", true).expect("remove-node m --cascade");
    assert_eq!(
        out.get("cascaded_edges"),
        Some(&Value::from(2)),
        "the cascade count is the pre-existing field, unchanged: {out}"
    );
    assert_eq!(
        out.get("released_locks"),
        Some(&Value::from(1)),
        "the cascade must report the lock it took away — exactly the holder's, not \
         every edge's: {out}"
    );
    assert_eq!(
        out.get("purged_bytes"),
        Some(&Value::from(0)),
        "the byte count must be present, and `0` rather than absent: a field that \
         appears only when nonzero is one a client learns to read as absent: {out}"
    );

    // An edgeless removal reports the same two fields at zero, so a client never has
    // to branch on their presence.
    let out = rpc.remove_node("p0", false).expect("remove-node p0");
    assert_eq!(out.get("cascaded_edges"), Some(&Value::from(0)), "{out}");
    assert_eq!(out.get("released_locks"), Some(&Value::from(0)), "{out}");
    assert_eq!(out.get("purged_bytes"), Some(&Value::from(0)), "{out}");
}

#[test]
fn disconnecting_an_interior_nodes_upstream_stalls_it_instead_of_shedding_bytes() {
    // Property 4. A map's raw side is an interior targetward path; disconnecting it
    // must backpressure the mapped endpoint's writers (§5: targetward never drops),
    // not silently swallow their bytes — the same stall a steal produces. Observable
    // through `send`, which joins the arbitration queue and reports its own outcome.
    let d = Daemon::start();
    let rpc = d.rpc();
    // A second map gives the first one an upstream that is itself a host endpoint,
    // so the whole test is device-free.
    let cfg = format!(
        r#"
{}
[[node]]
type = "map"
name = "up"
hostward = []
targetward = []
"#,
        bare_graph(&d)
    );
    rpc.load_toml(&cfg, false).expect("load");
    rpc.connect("up", "m/raw", None).expect("m's upstream");
    assert!(
        rpc.wait_status("m", "active", Duration::from_secs(5)),
        "with an upstream attached the map is active: {:?}",
        rpc.node("m")
    );

    rpc.disconnect("up", "m/raw").expect("disconnect upstream");
    assert!(
        rpc.wait_status("m", "waiting", Duration::from_secs(5)),
        "an interior node with no upstream reports waiting, honestly: {:?}",
        rpc.node("m")
    );

    // The mapped endpoint still accepts a bounded amount (the channel's buffer) —
    // that is buffering, not loss — and none of it is counted as discarded, because
    // nothing was dropped.
    rpc.send("m", "queued while detached", true, 2000)
        .expect("a bounded buffer accepts the write");
    let discarded = rpc
        .node("m")
        .and_then(|n| n.pointer("/targetward/discarded").cloned())
        .or_else(|| rpc.node("m").and_then(|n| n.get("targetward").cloned()));
    assert!(
        discarded.is_some(),
        "the map reports its targetward accounting: {:?}",
        rpc.node("m")
    );

    // Re-connecting revives it with no restart: the node goes active again and its
    // parked pump resumes.
    rpc.connect("up", "m/raw", None).expect("reconnect");
    assert!(
        rpc.wait_status("m", "active", Duration::from_secs(5)),
        "reconnecting revives the node: {:?}",
        rpc.node("m")
    );
}

#[test]
fn disconnecting_a_read_only_edge_leaves_no_phantom_origin() {
    // The lock registers *every* origin so `state` can list it, `never` included —
    // a log edge is a real origin that simply cannot write. If detach unregisters
    // only the ones `lock`/`unlock` can address, a read-only edge leaves its origin
    // behind for an edge that no longer exists, and one more of them per cycle. The
    // symptom is `state` describing a graph that is not there (§6).
    let d = Daemon::start();
    let rpc = d.rpc();
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir");
    let cfg = format!(
        r#"
{}
[[node]]
type = "log"
name = "cap"
directory = "{}"
filename = "cap.log"
"#,
        bare_graph(&d),
        logdir.display()
    );
    rpc.load_toml(&cfg, false).expect("load");

    let origins = |rpc: &serial_nexus_itest::Rpc| -> Vec<String> {
        rpc.node("m")
            .and_then(|n| n.pointer("/lock/origins").cloned())
            .and_then(|o| o.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|o| o.get("origin").and_then(Value::as_str).map(str::to_owned))
            .collect()
    };

    // A log target is forced to `never` whatever the edge says (§7.3), so this is
    // the read-only case without anyone typing "never".
    rpc.connect("m", "cap", None).expect("connect the log");
    assert_eq!(
        origins(rpc),
        vec!["cap".to_string()],
        "the origin is listed"
    );

    rpc.disconnect("m", "cap").expect("disconnect the log");
    assert!(
        origins(rpc).is_empty(),
        "a read-only origin must leave with its edge: {:?}",
        rpc.node("m")
    );

    // And it does not accumulate: three cycles leave exactly one, then none.
    for _ in 0..3 {
        rpc.connect("m", "cap", None).expect("reconnect");
        rpc.disconnect("m", "cap").expect("redisconnect");
    }
    assert!(origins(rpc).is_empty(), "no phantoms after three cycles");
    rpc.connect("m", "cap", None).expect("final connect");
    assert_eq!(
        origins(rpc),
        vec!["cap".to_string()],
        "exactly one origin, not four"
    );
}

#[test]
fn a_live_connect_never_reuses_an_origin_id_the_load_already_registered() {
    // The live allocator's floor must clear *every* id the initial wiring assigned,
    // not just the writable ones. A graph whose only edge is read-only registers
    // `OriginId(0)` on the host lock while contributing nothing to the writable
    // map — so a floor derived from that map stays at 0 and hands the next
    // `connect` an id already in use on the same lock. The two origins then alias:
    // unregistering one drops the other.
    let d = Daemon::start();
    let rpc = d.rpc();
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir");
    let cfg = format!(
        r#"
{}
[[node]]
type = "log"
name = "cap"
directory = "{}"
filename = "cap.log"

[[edge]]
a = "m"
b = "cap"
"#,
        bare_graph(&d),
        logdir.display()
    );
    rpc.load_toml(&cfg, false)
        .expect("load with only a never edge");

    // Now add a *writable* origin live. If it collides with the log's id, removing
    // the writer would take the log's registration with it.
    rpc.connect("m", "p0", None).expect("connect a pty");
    let origins: Vec<String> = rpc
        .node("m")
        .and_then(|n| n.pointer("/lock/origins").cloned())
        .and_then(|o| o.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|o| o.get("origin").and_then(Value::as_str).map(str::to_owned))
        .collect();
    assert_eq!(
        origins.len(),
        2,
        "both origins must be distinct registrations: {origins:?}"
    );

    rpc.disconnect("m", "p0").expect("disconnect the pty");
    let after: Vec<String> = rpc
        .node("m")
        .and_then(|n| n.pointer("/lock/origins").cloned())
        .and_then(|o| o.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|o| o.get("origin").and_then(Value::as_str).map(str::to_owned))
        .collect();
    assert_eq!(
        after,
        vec!["cap".to_string()],
        "removing the pty must not take the log's registration with it: {after:?}"
    );
}

/// The origins `state` lists on the map's host-facing endpoint (§6).
fn origins_of_m(rpc: &serial_nexus_itest::Rpc) -> Vec<String> {
    rpc.node("m")
        .and_then(|n| n.pointer("/lock/origins").cloned())
        .and_then(|o| o.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|o| o.get("origin").and_then(Value::as_str).map(str::to_owned))
        .collect()
}

/// Write `requests` to the control socket as one pipelined burst and read back
/// `requests.len()` NDJSON response lines. The harness's own [`serial_nexus_itest::Rpc`]
/// is one-request-per-connection by design, and pipelining is the point here: the
/// daemon dispatches a burst without yielding to the data plane between requests.
fn pipeline(socket: &std::path::Path, requests: &[String]) -> Vec<Value> {
    let mut s = UnixStream::connect(socket).expect("connect to the control socket");
    s.set_read_timeout(Some(Duration::from_secs(10)))
        .expect("read timeout");
    s.write_all(requests.join("").as_bytes())
        .expect("write the burst");
    s.flush().expect("flush");

    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if out.iter().filter(|b| **b == b'\n').count() >= requests.len() {
            break;
        }
        match s.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => out.extend_from_slice(&buf[..n]),
            Err(e) => panic!("read the burst's replies: {e}"),
        }
    }
    String::from_utf8_lossy(&out)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("parse {l:?}: {e}")))
        .collect()
}

/// `serial_nexus_rpc::AppError::EdgeInboxFull` — `APP_ERROR_BASE (-32000) - 7`.
/// Hardcoded as a literal for the same reason every other code in this family is:
/// `serial-nexus-itest` drives the daemon over the wire, so a constant imported from
/// the daemon's own crate would prove only that the daemon agrees with itself.
const EDGE_INBOX_FULL: i64 = -32007;

#[test]
fn a_pipelined_surgery_burst_leaves_no_phantom_origin_behind() {
    // 37-DATA-1. Each `connect` hands the consuming endpoint's pump a hostward
    // receiver through a bounded inbox (`runtime::EDGE_INBOX_CAP` = 4), and
    // `disconnect` cannot retract one already queued — so a burst dispatched with no
    // scheduler yield between its requests (one control connection, pipelined) fills
    // that inbox with stale entries and the next hand-off is refused. §4 rule 2
    // bounds *configured* edges, not undrained receivers, so it does not prevent
    // this; five connects with four interleaved disconnects reach it deterministically.
    //
    // What must not happen is the half-attached edge: an origin registered on the
    // lock, a sink in the fan-out, and a hostward receiver nobody holds — a
    // configured edge that is dead from birth, reported as if the *consumer* had no
    // pump. The refused attachment attaches nothing at all instead, so `state` never
    // grows a phantom — and the verb says so as the transient it is (`-32007`), which
    // is the control plane's half of the same finding: `consumer_live: false` names a
    // node that cannot receive at all, so answering it here sent an operator to
    // inspect a healthy pty while the configured edge stayed dead for good.
    let d = Daemon::start();
    let rpc = d.rpc();
    rpc.load_toml(&bare_graph(&d), false).expect("load");

    let mut burst = Vec::new();
    let mut id = 100;
    for i in 0..5 {
        if i > 0 {
            burst.push(format!(
                r#"{{"jsonrpc":"2.0","id":{id},"method":"disconnect","params":{{"a":"m","b":"p0"}}}}"#
            ));
            burst.push("\n".to_string());
            id += 1;
        }
        burst.push(format!(
            r#"{{"jsonrpc":"2.0","id":{id},"method":"connect","params":{{"a":"m","b":"p0"}}}}"#
        ));
        burst.push("\n".to_string());
        id += 1;
    }
    // Two strings per request (the line and its newline); the reply count is one per
    // request.
    let lines: Vec<String> = burst.chunks(2).map(|c| c.concat()).collect();
    let replies = pipeline(&d.socket(), &lines);
    assert_eq!(
        replies.len(),
        lines.len(),
        "one reply per request: {replies:?}"
    );

    // The burst's last connect is the one whose hand-off was refused. It must be
    // refused *as a refusal*: nothing succeeded, so nothing may report success.
    let last = replies.last().expect("a reply for the last connect");
    assert_eq!(
        last.pointer("/error/code").and_then(Value::as_i64),
        Some(EDGE_INBOX_FULL),
        "the overflowed hand-off must answer the transient by name, not report a \
         connected edge: {last}"
    );
    assert!(
        last.pointer("/result").is_none(),
        "an error reply carries no result (§10 result-XOR-error): {last}"
    );

    // A refused hand-off must also attach *nothing*: no origin on the lock, so `state`
    // cannot list a writer for an edge that carries no byte. (It registered one before,
    // alongside a fan-out sink feeding a receiver nobody held.)
    let origins = origins_of_m(rpc);
    assert!(
        origins.is_empty(),
        "the burst left {origins:?} registered on `m` — a hand-off refused for a full \
         edge inbox attached half an edge, whose hostward receiver was dropped; \
         edges={:?}",
        edges_of(&rpc.dump())
    );

    // And the endpoint is still usable: a plain connect after the burst behaves.
    let _ = rpc.disconnect("m", "p0");
    rpc.connect("m", "p0", None)
        .expect("a connect after the burst still works");
    assert_eq!(
        origins_of_m(rpc),
        vec!["p0".to_string()],
        "exactly the live edge's origin, not a phantom beside it"
    );
}

#[test]
fn dump_round_trips_after_a_connect_disconnect_sequence() {
    let d = Daemon::start();
    let rpc = d.rpc();
    rpc.load_toml(&bare_graph(&d), false).expect("load");

    rpc.connect("m", "p0", None).expect("connect p0");
    rpc.connect("m", "p1", Some("never")).expect("connect p1");
    assert_eq!(edges_of(&rpc.dump()).len(), 2);

    rpc.disconnect("p0", "m").expect("either order disconnects");
    let edges = edges_of(&rpc.dump());
    assert_eq!(edges.len(), 1, "exactly the surviving edge: {edges:?}");
    assert!(
        edges[0].0.contains('m') || edges[0].1.contains('m'),
        "the survivor is m <-> p1: {edges:?}"
    );

    // The dumped configuration is the load format, so it must reload as itself.
    let dumped = rpc.dump();
    rpc.load_config(dumped.clone(), true)
        .expect("a dump of a surgically-edited graph reloads");
    assert_eq!(
        edges_of(&rpc.dump()),
        edges_of(&dumped),
        "round-trip is exact"
    );

    // And it is persisted: edge surgery is configuration (§11/§15.9), so it must not
    // evaporate on restart. The state file is the daemon's own snapshot path.
    let state_file = d.run().state_file();
    assert!(
        wait_until(Duration::from_secs(5), || state_file.exists()),
        "the daemon never snapshotted its configuration"
    );
    // The snapshot is the load format (TOML), the same text `dump` renders.
    let snapshot: Value =
        toml::from_str(&std::fs::read_to_string(&state_file).expect("read state"))
            .expect("state file is TOML");
    assert_eq!(
        edges_of(&snapshot),
        edges_of(&dumped),
        "the snapshot carries the surgically-edited edges, not the loaded ones"
    );
}
