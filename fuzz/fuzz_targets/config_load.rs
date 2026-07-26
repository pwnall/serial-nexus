#![no_main]
//! Fuzz the `load` verb's payload — `GraphConfig` deserialization plus
//! `GraphConfig::validate` (§11: "the entire file is validated … before anything is
//! created").
//!
//! Added for review 26 **SEC-7** (the parsers reachable without a leg) and its testing
//! item 10 (*"numeric config ranges are unfuzzed — a proptest over the config generator
//! with extreme values would have caught findings #2 and #3 before they reached a live
//! daemon"*). Two of that review's three worst defects were **configuration values**: a
//! `replay_ring` large enough to abort the process on the first hostward byte, and a
//! `hostward_buffer` large enough to panic tokio's semaphore *after* `load --replace`
//! had already torn the running graph down. Both walked through this parser.
//!
//! JSON, not TOML, deliberately: JSON is the shape the `load` RPC actually carries
//! (`serialnexusctl` converts TOML to it before sending), so this fuzzes the bytes a
//! hostile client can put on the control socket rather than a CLI convenience format.
//!
//! Invariants:
//!
//! * `validate` is **total** — a config that deserialized must be judgeable without
//!   panicking, however extreme its numbers;
//! * a config validating **clean** must build its `GraphModel` without panicking, since
//!   that is exactly what the daemon does next;
//! * `dump`/`load` fidelity — re-serializing a parsed config and parsing it back yields
//!   an equal config with the same verdict. §11 makes configuration round-trippable, and
//!   `dump` is the operator's only way to read the graph back.
//!
//! Worth seeding a corpus from `packaging/serialnexusd.example.toml` (converted to
//! JSON) and from `nexus-itest`'s inline configs: `deny_unknown_fields` (the CP-2 fix)
//! means an unseeded fuzzer will spend most of its budget being refused at the door.

use libfuzzer_sys::fuzz_target;
use nexus_core::GraphConfig;

fuzz_target!(|data: &[u8]| {
    let Ok(config) = serde_json::from_slice::<GraphConfig>(data) else {
        return; // not a config at all — the door did its job
    };

    // Total: extreme numerics must produce a verdict, not a crash.
    let errors = config.validate();

    // What the daemon does with a clean config, before anything is created.
    if errors.is_empty() {
        let _model = config.to_model();
    }

    // Round-trip fidelity (§11): dump -> load is the operator's read-back path.
    let bytes = serde_json::to_vec(&config).expect("a parsed GraphConfig must re-serialize");
    let back = serde_json::from_slice::<GraphConfig>(&bytes)
        .expect("a serialized GraphConfig must parse back");
    assert_eq!(
        back, config,
        "GraphConfig did not survive a dump/load round trip"
    );
    assert_eq!(
        back.validate().len(),
        errors.len(),
        "the round trip changed the validation verdict"
    );
});
