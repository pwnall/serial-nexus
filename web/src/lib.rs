#![forbid(unsafe_code)]

//! `serial-nexus-web` — the serial_nexus web console (design §17).
//!
//! A pure RPC client of `serial-nexus-daemon` on one side, a loopback HTTP + WebSocket
//! server on the other. The daemon does not link, serve, or know about this binary;
//! everything it does rides the §10 RPC surface — `state`/`subscribe` for the
//! console list and lock badges, `tap.open`/`tap.close` for bytes, `send` for
//! input, `info` for provenance — which is §15.16's separation paying out a third
//! time (§15.26): the web client works unchanged against any embedding daemon.
//!
//! Security (§15.29): a loopback TCP port is reachable by every local user, unlike
//! the 0600 control socket, so every request and WebSocket upgrade requires a
//! per-session bearer token (a cookie after the bootstrap URL), and the Host header
//! is validated against localhost. The bind policy is three-tiered: loopback+token
//! by default; `--tls`+token off loopback (the sanctioned mode); `--insecure-bind`
//! as the named footgun, token still mandatory.

mod assets;
mod bridge;
mod rpc;
mod server;
mod tls;
mod wsclient;

// The binary's entry points. `serial-nexus-web` is a binary that happens to have a
// library, not a library with a binary: this surface exists so `main.rs` stays thin
// and so the fuzz harness below has something to link against. It is not an
// embedding API and carries no stability promise.
pub use rpc::resolve_socket;
pub use server::{ServerConfig, run as serve};
pub use tls::build_config as build_tls_config;
pub use wsclient::{WsclientArgs, run as wsclient};

/// **Not part of any supported API. Exposed only so the fuzz harness can reach it,
/// and free to change or disappear in any release.**
///
/// Review 26's SEC-7 observed that every fuzz target sat on the `serial-nexus-codec-api` layer,
/// leaving the parsers reachable *without* a leg uncovered. This crate's HTTP head
/// parser is one of them, and it is the most exposed parser in the project: §15.29
/// sanctions binding it beyond loopback under `--tls` or `--insecure-bind`, so it can
/// face an unauthenticated network peer — and it runs *before* the token gate, which
/// is exactly the position a fuzzer should be aimed at. Unit tests cover its contract
/// (the `MAX_HEAD` cap, the deadline, header splitting); they cannot cover its input
/// space.
///
/// See `docs/implementation-notes.md` §3.19 for the terms this module is offered on.
pub mod unstable_fuzz_api {
    pub use crate::server::{
        MAX_HEAD, Request, origin_matches_host, read_request, split_authority,
    };
}
