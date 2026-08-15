#![forbid(unsafe_code)]

//! `serial-nexus-rpc` — the JSON-RPC 2.0 wire types shared by `serial-nexus-daemon` and
//! `serial-nexus-ctl` (design §10).
//!
//! This is the *stable surface* of §15.16: the daemon constrains the CLI only
//! through this RPC surface, and everything here is hand-rolled JSON-RPC 2.0
//! over newline-delimited JSON — a page of serde types, no framework crate.
//!
//! Design commitments encoded here:
//!
//! * Request/response correlation by `id` (supports concurrent CLI clients).
//! * Id-less [`Notification`]s are the shape of `subscribe` streams (§10).
//! * Batch arrays are **rejected outright** — [`parse_incoming_request`]
//!   returns [`error_codes::INVALID_REQUEST`] for a top-level `[`, "deleting
//!   the specification's awkward corner".
//! * Method params and results are carried as opaque [`serde_json::Value`]; the
//!   daemon owns the concrete per-method schemas. This keeps `serial-nexus-rpc` the
//!   thin, stable framing layer and lets version skew degrade gracefully via
//!   the standard `method not found` error.

use std::fmt;

use serde::{Deserialize, Serialize, de};
use serde_json::Value;

/// The only JSON-RPC version this daemon speaks.
pub const JSONRPC_VERSION: &str = "2.0";

/// The daemon's name, and with it every default path §10 and §11 derive from it:
/// `<name>.sock` for the control socket, `<name>.state.toml` for the snapshot beside
/// it, and `/tmp/<name>-<uid>.sock` where there is no `$XDG_RUNTIME_DIR`.
///
/// It lives *here*, in the crate that already is the control plane's stable surface
/// (§15.16), because three separate processes must agree on it — the daemon that
/// creates the socket, the CLI that connects to it, and the web console that does the
/// same — and each of them used to spell it out for itself.
pub const DAEMON_NAME: &str = "serial-nexus-daemon";

pub mod socket;
pub use socket::{SocketOrigin, default_socket_path, resolve_client_socket};

/// The pre-§15.40 spelling of [`DAEMON_NAME`], **accepted on read for one release**
/// (plan §17.3) and then deleted.
///
/// The rename moved the default control socket and state file, and a running system's
/// state file is not something an operator can be asked to notice: a daemon restarted
/// after the upgrade would have found no snapshot at the new path and come up with an
/// empty graph, silently discarding every node built by incremental surgery. So the
/// old default is still *read* — the daemon adopts a legacy snapshot and then writes
/// under the new name, and the two clients fall back to a legacy socket only when no
/// current one exists. Nothing ever *writes* this spelling again.
///
/// This constant is the single place the retired name survives in live code; the
/// `retired_names_appear_only_where_history_lives` meta-gate allows exactly it, so
/// derive from it rather than repeating it. The window has two halves that read it: the
/// client fallback is [`resolve_client_socket`], one implementation for both clients
/// since plan §18 item 51, so closing that half edits this crate alone; snapshot
/// adoption is the daemon's and closes there.
pub const LEGACY_DAEMON_NAME: &str = "serialnexusd";

/// A version marker that serializes as the string `"2.0"` and rejects anything
/// else on the wire. Zero-sized, so it costs nothing to carry on every message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct V2;

impl Serialize for V2 {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(JSONRPC_VERSION)
    }
}

impl<'de> Deserialize<'de> for V2 {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        if s == JSONRPC_VERSION {
            Ok(V2)
        } else {
            Err(de::Error::custom(format!(
                "jsonrpc version must be \"{JSONRPC_VERSION}\", got {s:?}"
            )))
        }
    }
}

/// A JSON-RPC request id: a string or a number for a correlated request, or
/// `Null`. We never *mint* a null id for an outbound request, but the protocol
/// requires it in one place: a response to a request whose id could not be
/// determined — a parse error or an invalid request (JSON-RPC 2.0 §5) — must use
/// `id: null`. So the type can both produce and consume it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    Number(i64),
    String(String),
    Null,
}

impl From<i64> for Id {
    fn from(n: i64) -> Self {
        Id::Number(n)
    }
}

impl From<String> for Id {
    fn from(s: String) -> Self {
        Id::String(s)
    }
}

impl From<&str> for Id {
    fn from(s: &str) -> Self {
        Id::String(s.to_owned())
    }
}

/// A client-to-daemon request. Always carries an `id`; the daemon rejects
/// id-less requests (client-side notifications are not part of this protocol).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: V2,
    pub id: Id,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl Request {
    pub fn new(id: impl Into<Id>, method: impl Into<String>, params: Option<Value>) -> Self {
        Request {
            jsonrpc: V2,
            id: id.into(),
            method: method.into(),
            params,
        }
    }
}

/// A daemon-to-client response. Exactly one of `result`/`error` is present on
/// the wire; the constructors enforce that on the send side, and the custom
/// [`Deserialize`] enforces it on the receive side — a response with neither or
/// both is rejected, not silently accepted. The layout matches JSON-RPC 2.0
/// byte-for-byte.
#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub jsonrpc: V2,
    pub id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    /// A success response carrying a structured result.
    pub fn success(id: impl Into<Id>, result: Value) -> Self {
        Response {
            jsonrpc: V2,
            id: id.into(),
            result: Some(result),
            error: None,
        }
    }

    /// An error response correlated to a request id.
    pub fn error(id: impl Into<Id>, error: RpcError) -> Self {
        Response {
            jsonrpc: V2,
            id: id.into(),
            result: None,
            error: Some(error),
        }
    }

    /// An error response for a request whose id could not be determined — a
    /// parse error or an invalid request. Uses `id: null` as JSON-RPC 2.0 §5
    /// requires, so the daemon can always reply and the client's read stream
    /// never desyncs.
    pub fn error_without_id(error: RpcError) -> Self {
        Response::error(Id::Null, error)
    }

    /// True when this response carries a successful result (a result is present
    /// and no error) — not merely the absence of an error.
    pub fn is_success(&self) -> bool {
        self.result.is_some() && self.error.is_none()
    }
}

impl<'de> Deserialize<'de> for Response {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // Deserialize to a Value first so we can distinguish a *present* `result`
        // whose value is JSON `null` (a legitimate success result) from an
        // *absent* one — `Option<Value>` would collapse both to `None`. Then
        // enforce the result-XOR-error invariant the wire format guarantees.
        let v = Value::deserialize(d)?;
        let obj = v
            .as_object()
            .ok_or_else(|| de::Error::custom("jsonrpc response must be a JSON object"))?;

        if obj.get("jsonrpc").and_then(Value::as_str) != Some(JSONRPC_VERSION) {
            return Err(de::Error::custom(format!(
                "jsonrpc version must be \"{JSONRPC_VERSION}\""
            )));
        }
        let id: Id = obj
            .get("id")
            .cloned()
            .ok_or_else(|| de::Error::custom("jsonrpc response missing id"))
            .and_then(|iv| serde_json::from_value(iv).map_err(de::Error::custom))?;

        let has_result = obj.contains_key("result");
        let has_error = obj.contains_key("error");
        match (has_result, has_error) {
            (true, false) => Ok(Response {
                jsonrpc: V2,
                id,
                result: obj.get("result").cloned(),
                error: None,
            }),
            (false, true) => {
                let error =
                    serde_json::from_value(obj["error"].clone()).map_err(de::Error::custom)?;
                Ok(Response {
                    jsonrpc: V2,
                    id,
                    result: None,
                    error: Some(error),
                })
            }
            (false, false) => Err(de::Error::custom(
                "jsonrpc response has neither result nor error",
            )),
            (true, true) => Err(de::Error::custom(
                "jsonrpc response has both result and error",
            )),
        }
    }
}

/// A daemon-to-client notification: an id-less message that powers `subscribe`
/// streams (node status transitions, lock changes, client-termios updates,
/// counter snapshots — §10).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub jsonrpc: V2,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl Notification {
    pub fn new(method: impl Into<String>, params: Option<Value>) -> Self {
        Notification {
            jsonrpc: V2,
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        RpcError {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn method_not_found(method: &str) -> Self {
        RpcError::new(
            error_codes::METHOD_NOT_FOUND,
            format!("method not found: {method}"),
        )
    }

    pub fn invalid_params(msg: impl Into<String>) -> Self {
        RpcError::new(error_codes::INVALID_PARAMS, msg)
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        RpcError::new(error_codes::INTERNAL_ERROR, msg)
    }
}

/// **The one client-facing rendering of a refused request: `<message> (<code>)`.**
///
/// It lives here, beside the type, for the reason [`DAEMON_NAME`] does: more than one
/// process turns this object into a line a human reads, and each of them used to spell
/// the rendering out for itself. `serial-nexus-ctl` printed `<message> (<code>)` from
/// its streaming acks and `error <code>: <message>` from its one-shot verbs — two
/// spellings in one binary — while the headless WebSocket client printed the whole
/// error *object*, JSON braces and all, because it holds the frame untyped and `{err}`
/// was the shortest thing to write (plan §18 item 59(c)). The precedent is review
/// **SIMP-6**, one layer down: `-32002` used to reach `error.message` prefixed from one
/// producer and bare from another, so one refusal read as two sentences. This is that
/// finding at the *consumer* end.
///
/// Two details are deliberate:
///
/// * **`data` is not on the line.** It is free-form and can be arbitrarily large — a
///   structural refusal carries `data.errors` and, for an unknown codec, every
///   registered name in `data.available` (§8/§15.26) — so a client that wants it prints
///   it *after* the line, where a terminal can survive it.
/// * **The message leads.** The code is the machine's half of the answer and rides in
///   parentheses at the end; the operator reads the sentence first. Nothing parses this
///   line — `serial-nexus-ctl --json` exists so that nothing has to (review 26, CLI-4),
///   and that is what makes this rendering free to be one thing rather than three.
impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

/// [`RpcError`]'s rendering for a client that holds the error frame **untyped** — the
/// web console's headless WebSocket client, which parses a bridged frame as a bare
/// [`Value`] and never builds the typed response.
///
/// A frame that is not a well-formed error object falls back to its own JSON rather
/// than to silence: this is the client of a *bridge*, so the answer on the wire is not
/// guaranteed to be the daemon's — and a line that says `{"oops":1}` still tells the
/// operator what arrived, where a line that dropped it would say nothing at all. The
/// fallback deliberately does **not** invent a code to render: a code on this line is a
/// code the daemon sent.
pub fn describe_error(err: &Value) -> String {
    match RpcError::deserialize(err) {
        Ok(e) => e.to_string(),
        Err(_) => err.to_string(),
    }
}

/// The standard JSON-RPC 2.0 error codes, plus room for application codes.
pub mod error_codes {
    /// Invalid JSON was received.
    pub const PARSE_ERROR: i64 = -32700;
    /// The JSON is not a valid Request object (includes rejected batch arrays).
    pub const INVALID_REQUEST: i64 = -32600;
    /// The method does not exist — the graceful version-skew signal (§15.16).
    pub const METHOD_NOT_FOUND: i64 = -32601;
    /// Invalid method parameters.
    pub const INVALID_PARAMS: i64 = -32602;
    /// Internal daemon error.
    pub const INTERNAL_ERROR: i64 = -32603;

    /// Application errors live in the reserved implementation-defined range
    /// [-32099, -32000]; the daemon assigns specific meanings (e.g. a locked
    /// endpoint) within it — see [`super::AppError`], the single registry (§16.8).
    pub const APP_ERROR_BASE: i64 = -32000;
}

/// Every **application** error code the daemon can emit, in the reserved
/// implementation-defined range [-32099, -32000] (§10). This enum is the single
/// registry (§16.8): a new code is a new variant, so it cannot be emitted without a
/// stable name and a one-line meaning, and the `docs/rpc` error table plus the
/// no-duplicate-codes invariant are asserted from it. Application codes had grown
/// ad hoc to five and a docs audit caught an undocumented one; defining them once
/// here makes drift a test-time fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppError {
    /// `load` attempted on a non-empty graph (§11 load-on-empty).
    LoadNonEmpty,
    /// A structural validation failure (§4); `data.errors` lists the messages.
    Structural,
    /// A contended `lock`/`send` was refused (§6); `data.held_by` names the holder.
    Locked,
    /// `remove-node` refused because edges are attached and `--cascade` was absent.
    HasEdges,
    /// `add-node` by raw path/serial with the device absent (§12).
    DeviceAbsent,
    /// A request arrived on a connection whose waiting verb (`lock --wait`,
    /// `send`, `tap.wait`) is still parked. §15.20 runs one waiting verb per connection; the
    /// pipelined request is refused so the wait survives, because the alternative
    /// the review found was silence — the connection died with no reply to either
    /// request, taking a web console's subscription and taps with it (CTRL-1).
    WaitInFlight,
    /// `connect` refused because the target-facing endpoint's pump has not drained
    /// the hostward receivers of its earlier edges yet (§4 rule 2 bounds *configured*
    /// edges, not queued receivers). Purely transient — nothing changed and an
    /// immediate retry is the remedy — and distinct from every code above, all of
    /// which are properties of the request or the graph rather than of the moment
    /// (37-DATA-1).
    EdgeInboxFull,
    /// A parked `tap.wait` ended because the graph dropped its endpoint —
    /// `teardown`, `load --replace`, `remove-node` (§10 *The pattern wait*, clause
    /// 6). Deliberately **not** the deadline: expiry is a typed *result*
    /// (`timed_out: true`), because "no pattern appeared within the deadline" is an
    /// answer about a stream that was watched throughout, while this says the stream
    /// stopped existing. Collapsing the two would let a caller retry a wait on an
    /// endpoint that is gone, forever. `data` carries the same scan and gap counters
    /// the timeout result does, so the caller learns how much of the stream the
    /// abandoned wait did cover.
    EndpointGone,
}

impl AppError {
    /// Every application error, in code order — the registry's application half.
    pub const ALL: &'static [AppError] = &[
        AppError::LoadNonEmpty,
        AppError::Structural,
        AppError::Locked,
        AppError::HasEdges,
        AppError::DeviceAbsent,
        AppError::WaitInFlight,
        AppError::EdgeInboxFull,
        AppError::EndpointGone,
    ];

    /// The numeric code, offset from [`error_codes::APP_ERROR_BASE`].
    pub const fn code(self) -> i64 {
        error_codes::APP_ERROR_BASE
            - match self {
                AppError::LoadNonEmpty => 1,
                AppError::Structural => 2,
                AppError::Locked => 3,
                AppError::HasEdges => 4,
                AppError::DeviceAbsent => 5,
                AppError::WaitInFlight => 6,
                AppError::EdgeInboxFull => 7,
                AppError::EndpointGone => 8,
            }
    }

    /// The stable short name shown in the docs table.
    pub const fn name(self) -> &'static str {
        match self {
            AppError::LoadNonEmpty => "load on non-empty graph",
            AppError::Structural => "structural error",
            AppError::Locked => "locked",
            AppError::HasEdges => "has edges",
            AppError::DeviceAbsent => "device absent",
            AppError::WaitInFlight => "waiting verb in flight",
            AppError::EdgeInboxFull => "edge inbox full",
            AppError::EndpointGone => "endpoint gone",
        }
    }

    /// A one-line meaning for the docs table.
    pub const fn summary(self) -> &'static str {
        match self {
            AppError::LoadNonEmpty => "`load` without `replace` while a graph is already loaded",
            AppError::Structural => {
                "configuration failed validation; `data.errors` is the list of messages, and `message` is always `structural error: <first>`"
            }
            AppError::Locked => {
                "a contended `lock`/`send` was refused; `data.held_by` names the holder when known"
            }
            AppError::HasEdges => {
                "`remove-node` refused because edges are still attached and `--cascade` was not given"
            }
            AppError::DeviceAbsent => {
                "`add-node` by raw path or serial number, but the device is not present so its identity cannot be captured (§12)"
            }
            AppError::WaitInFlight => {
                "a request was pipelined behind an in-flight waiting verb on the same connection; §15.20 runs one at a time, and the wait, its taps and its subscription are all left intact"
            }
            AppError::EdgeInboxFull => {
                "`connect` refused: the target-facing endpoint has not drained the hostward receivers of its earlier edges yet. Transient — nothing changed, retry"
            }
            AppError::EndpointGone => {
                "a parked `tap.wait` ended because the graph dropped its endpoint (`teardown`, `load --replace`, `remove-node`) — distinct from the deadline, which is a typed *result* (`timed_out: true`) rather than an error; `data` carries the scan and gap counters the wait did cover"
            }
        }
    }
}

/// One documented error code: its number, a stable short name, and a one-line
/// meaning. [`error_code_registry`] assembles these for the `docs/rpc` table and
/// the docs↔behavior test (§16.8).
pub struct ErrorCodeDoc {
    pub code: i64,
    pub name: &'static str,
    pub summary: &'static str,
}

/// Every code the daemon can emit — the standard JSON-RPC codes followed by the
/// application codes — as the single source for the `docs/rpc` error table. A test
/// asserts the table matches this registry, so an unregistered or undocumented code
/// is caught at test time (§16.8).
pub fn error_code_registry() -> Vec<ErrorCodeDoc> {
    let mut v = vec![
        ErrorCodeDoc {
            code: error_codes::PARSE_ERROR,
            name: "parse error",
            summary: "the line was not valid JSON (`id: null`)",
        },
        ErrorCodeDoc {
            code: error_codes::INVALID_REQUEST,
            name: "invalid request",
            summary: "not a valid request object, wrong `jsonrpc` version, or a rejected batch array (`id: null`)",
        },
        ErrorCodeDoc {
            code: error_codes::METHOD_NOT_FOUND,
            name: "method not found",
            summary: "unknown method — the graceful version-skew signal (§15.16)",
        },
        ErrorCodeDoc {
            code: error_codes::INVALID_PARAMS,
            name: "invalid params",
            summary: "missing or malformed params for a known method",
        },
        ErrorCodeDoc {
            code: error_codes::INTERNAL_ERROR,
            name: "internal error",
            summary: "an unexpected daemon-side failure",
        },
    ];
    v.extend(AppError::ALL.iter().map(|&e| ErrorCodeDoc {
        code: e.code(),
        name: e.name(),
        summary: e.summary(),
    }));
    v
}

/// A message read from the daemon by a client: either a correlated [`Response`]
/// or an id-less [`Notification`]. Distinguished structurally by the presence
/// of `id`/`method`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Incoming {
    Response(Response),
    Notification(Notification),
}

/// Serialize a message to a single newline-terminated line (the framing).
///
/// # Panics
/// Never in practice: our own request/response/notification types always
/// serialize. Kept infallible for call-site ergonomics.
pub fn to_line<T: Serialize>(msg: &T) -> String {
    let mut s = serde_json::to_string(msg).expect("serial-nexus-rpc types always serialize");
    s.push('\n');
    s
}

/// Parse one newline-delimited line as a daemon-side [`Request`], applying the
/// two protocol rules the daemon enforces at the door: valid JSON (else
/// [`error_codes::PARSE_ERROR`]) and no batch arrays (a leading `[` yields
/// [`error_codes::INVALID_REQUEST`], per §10 "Batch arrays are rejected
/// outright"). A structurally invalid request (wrong version, missing method)
/// also yields `INVALID_REQUEST`.
pub fn parse_incoming_request(line: &str) -> Result<Request, RpcError> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('[') {
        return Err(RpcError::new(
            error_codes::INVALID_REQUEST,
            "batch requests are not supported",
        ));
    }
    // First check it's valid JSON at all, to distinguish PARSE_ERROR from a
    // well-formed-but-invalid Request object.
    let value: Value = serde_json::from_str(trimmed)
        .map_err(|e| RpcError::new(error_codes::PARSE_ERROR, format!("invalid JSON: {e}")))?;
    serde_json::from_value(value).map_err(|e| {
        RpcError::new(
            error_codes::INVALID_REQUEST,
            format!("invalid request: {e}"),
        )
    })
}

/// The standard base64 alphabet (RFC 4648).
const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 encode (RFC 4648), dependency-free. Used to carry arbitrary
/// console bytes inside a JSON string — the `tap.data` notification payload (§10,
/// §17); the browser decodes with native `atob`, and [`base64_decode`] is the
/// round-trip inverse for Rust clients (`serial-nexus-ctl tap`).
pub fn base64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(BASE64_ALPHABET[(n >> 18 & 0x3f) as usize] as char);
        out.push(BASE64_ALPHABET[(n >> 12 & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            BASE64_ALPHABET[(n >> 6 & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            BASE64_ALPHABET[(n & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Standard base64 decode (RFC 4648), dependency-free — the inverse of
/// [`base64_encode`]. Ignores ASCII whitespace; returns `None` on any other invalid
/// character or a malformed length. Used by `serial-nexus-ctl tap` to reconstruct the
/// hostward byte stream from `tap.data` notifications (§17).
pub fn base64_decode(input: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut symbols = Vec::with_capacity(input.len());
    let mut pad = 0usize;
    for &c in input.as_bytes() {
        if c.is_ascii_whitespace() {
            continue;
        }
        if c == b'=' {
            pad += 1;
            continue;
        }
        if pad > 0 {
            return None; // data after padding
        }
        symbols.push(val(c)?);
    }
    if !(symbols.len() + pad).is_multiple_of(4) || pad > 2 {
        return None;
    }
    let mut out = Vec::with_capacity(symbols.len() / 4 * 3);
    for group in symbols.chunks(4) {
        let n = group.iter().fold(0u32, |acc, &s| (acc << 6) | s) << ((4 - group.len()) * 6);
        out.push((n >> 16 & 0xff) as u8);
        if group.len() >= 3 {
            out.push((n >> 8 & 0xff) as u8);
        }
        if group.len() >= 4 {
            out.push((n & 0xff) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_round_trips_on_the_wire() {
        let req = Request::new(7, "state", Some(json!({"node": "usb0"})));
        let line = to_line(&req);
        assert!(line.ends_with('\n'));
        let parsed = parse_incoming_request(&line).expect("valid request");
        assert_eq!(parsed.method, "state");
        assert_eq!(parsed.id, Id::Number(7));
        assert_eq!(parsed.params, Some(json!({"node": "usb0"})));
    }

    #[test]
    fn base64_known_vectors_and_round_trip() {
        // RFC 4648 §10 test vectors.
        for (raw, enc) in [
            (&b""[..], ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg=="),
            (b"fooba", "Zm9vYmE="),
            (b"foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64_encode(raw), enc);
            assert_eq!(base64_decode(enc).as_deref(), Some(raw));
        }
        // Round-trip over all byte values, including the tap.data payload shape.
        let all: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
        assert_eq!(
            base64_decode(&base64_encode(&all)).as_deref(),
            Some(&all[..])
        );
        // Whitespace is ignored; junk, short length, and data-after-pad are rejected.
        assert_eq!(base64_decode("Zm9v\nYmFy").as_deref(), Some(&b"foobar"[..]));
        assert_eq!(base64_decode("Zm9v!"), None); // invalid character
        assert_eq!(base64_decode("abc"), None); // length not a multiple of 4
        assert_eq!(base64_decode("ab=c"), None); // data after padding
    }

    #[test]
    fn wrong_version_is_rejected() {
        let line = r#"{"jsonrpc":"1.0","id":1,"method":"state"}"#;
        let err = parse_incoming_request(line).unwrap_err();
        assert_eq!(err.code, error_codes::INVALID_REQUEST);
    }

    #[test]
    fn batch_arrays_are_rejected() {
        let line = r#"[{"jsonrpc":"2.0","id":1,"method":"state"}]"#;
        let err = parse_incoming_request(line).unwrap_err();
        assert_eq!(err.code, error_codes::INVALID_REQUEST);
        assert!(err.message.contains("batch"));
    }

    #[test]
    fn malformed_json_is_a_parse_error() {
        let err = parse_incoming_request("{not json").unwrap_err();
        assert_eq!(err.code, error_codes::PARSE_ERROR);
    }

    #[test]
    fn null_id_error_response_round_trips() {
        // The reply the daemon must send for a parse error (§5): id is null.
        let resp =
            Response::error_without_id(RpcError::new(error_codes::PARSE_ERROR, "Parse error"));
        let line = to_line(&resp);
        assert!(
            line.contains(r#""id":null"#),
            "expected null id, got {line}"
        );

        // And a compliant null-id error response is consumable via Incoming.
        match serde_json::from_str::<Incoming>(line.trim()).unwrap() {
            Incoming::Response(r) => {
                assert_eq!(r.id, Id::Null);
                assert!(!r.is_success());
                assert_eq!(r.error.as_ref().unwrap().code, error_codes::PARSE_ERROR);
            }
            Incoming::Notification(_) => panic!("null-id error must parse as a response"),
        }
    }

    /// **The refusal rendering is pinned to its exact bytes, not to "the code appears
    /// somewhere"** (plan §18 item 59(c)).
    ///
    /// The distinction is the whole point of this test. The two guards that already
    /// watch a refused streaming verb assert `stderr.contains("-32601")`, which every
    /// candidate rendering satisfies — including printing the raw JSON object, which is
    /// what the WebSocket client did while the CLI printed `<message> (<code>)`. So the
    /// divergence item 59(c) found was invisible to the suite, and would have been
    /// invisible again the next time. An `assert_eq!` on the whole line is the only
    /// assertion that can tell the renderings apart, and it is why this lives beside the
    /// type rather than beside either client.
    #[test]
    fn a_refusal_renders_as_message_then_code_and_never_carries_data() {
        let err = RpcError::new(error_codes::METHOD_NOT_FOUND, "method not found: tap.open");
        assert_eq!(err.to_string(), "method not found: tap.open (-32601)");

        // `data` is the free-form half and can be arbitrarily large (an unknown-codec
        // refusal rides every registered name in `data.available`, §8/§15.26); it must
        // not be spliced into the one line an operator reads.
        //
        // Asserted by the equality alone. A following `assert!(!…contains("usb0"))`
        // stood here and could not fail while the equality passed — the same
        // subsumed-assertion shape as the two the reviewers found one function down
        // and in `serial-nexus-ctl`; its intent is in the message instead.
        let with_data = RpcError::invalid_params("unknown endpoint: nope")
            .with_data(json!({"available": ["usb0", "console"]}));
        assert_eq!(
            with_data.to_string(),
            "unknown endpoint: nope (-32602)",
            "`data` rode the operator's line: the available-codec list is unbounded in \
             length and belongs after it, not in it"
        );
    }

    /// The untyped twin renders **the same bytes** as the typed one for the same object,
    /// which is what lets a client holding a bare frame share the rendering rather than
    /// re-spell it; and a frame that is not an error object keeps its JSON instead of
    /// vanishing or growing an invented code.
    #[test]
    fn describe_error_agrees_with_the_typed_rendering_and_falls_back_to_json() {
        let typed = RpcError::new(error_codes::METHOD_NOT_FOUND, "method not found: tap.open");
        let wire = json!({"code": -32601, "message": "method not found: tap.open"});
        assert_eq!(describe_error(&wire), typed.to_string());

        // A `data` key on the wire is carried by the type and still stays off the line.
        let with_data = json!({"code": -32602, "message": "bad", "data": {"available": []}});
        assert_eq!(describe_error(&with_data), "bad (-32602)");

        // Not an error object: no message, no code, wrong type for the code, or not an
        // object at all. Each keeps its own JSON.
        //
        // **There is no second assertion here that the fallback did not invent a code**,
        // and its absence is deliberate. One stood here reading
        // `assert!(!line.contains("(-327"))` under the comment "the fallback invented a
        // code", and it asserted nothing twice over: no code in this loop begins `-327`
        // (they are `-326xx`), so the needle could not match any rendering at all, and
        // the `assert_eq!` above pins the whole line anyway, which subsumes every later
        // assertion about the same string. A reviewer removed the `assert_eq!`, planted
        // an invented code, and watched the remaining twenty-eight assertions stay
        // green. **The `assert_eq!` is what carries the property**: a line that says
        // `x (-32603)` is not `{"message":"x"}`, so inventing a code fails it by
        // construction — and `{"message": "x"}` is in the list precisely because it is
        // the frame where inventing one is tempting (`RpcError::code` has no serde
        // default, so a message-only frame reaches the fallback with a sentence and no
        // number).
        for junk in [
            json!({"code": -32601}),
            json!({"message": "x"}),
            json!({"code": "minus a lot", "message": "x"}),
            json!("boom"),
        ] {
            let line = describe_error(&junk);
            assert_eq!(
                line,
                junk.to_string(),
                "the fallback must be the frame's own JSON — anything else is a code or \
                 a message this crate made up for a frame that carried neither"
            );
        }
    }

    #[test]
    fn response_with_neither_result_nor_error_is_rejected() {
        let err = serde_json::from_str::<Response>(r#"{"jsonrpc":"2.0","id":1}"#).unwrap_err();
        assert!(err.to_string().contains("neither"), "got: {err}");
    }

    #[test]
    fn response_with_both_result_and_error_is_rejected() {
        let line = r#"{"jsonrpc":"2.0","id":1,"result":1,"error":{"code":-1,"message":"x"}}"#;
        let err = serde_json::from_str::<Response>(line).unwrap_err();
        assert!(err.to_string().contains("both"), "got: {err}");
    }

    #[test]
    fn is_success_requires_a_result_present() {
        assert!(Response::success(1, json!({"ok": true})).is_success());
        assert!(!Response::error(1, RpcError::internal("boom")).is_success());
    }

    #[test]
    fn success_response_has_result_and_no_error_key() {
        let resp = Response::success(3, json!({"ok": true}));
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains("\"result\""));
        assert!(!s.contains("\"error\""));
        assert!(resp.is_success());
    }

    #[test]
    fn error_response_has_error_and_no_result_key() {
        let resp = Response::error(3, RpcError::method_not_found("bogus"));
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains("\"error\""));
        assert!(!s.contains("\"result\""));
        assert!(!resp.is_success());
    }

    #[test]
    fn incoming_distinguishes_response_from_notification() {
        let resp_line = to_line(&Response::success(1, json!(null)));
        match serde_json::from_str::<Incoming>(resp_line.trim()).unwrap() {
            Incoming::Response(r) => assert_eq!(r.id, Id::Number(1)),
            Incoming::Notification(_) => panic!("expected a response"),
        }

        let note_line = to_line(&Notification::new(
            "node.status",
            Some(json!({"n": "usb0"})),
        ));
        match serde_json::from_str::<Incoming>(note_line.trim()).unwrap() {
            Incoming::Notification(n) => assert_eq!(n.method, "node.status"),
            Incoming::Response(_) => panic!("expected a notification"),
        }
    }

    #[test]
    fn string_and_number_ids_both_round_trip() {
        for id in [Id::Number(42), Id::String("abc".into())] {
            let req = Request::new(id.clone(), "ping", None);
            let parsed = parse_incoming_request(&to_line(&req)).unwrap();
            assert_eq!(parsed.id, id);
        }
    }

    // --- error-code registry (§16.8) ---------------------------------------------

    #[test]
    fn registry_has_no_duplicate_codes() {
        let mut seen = std::collections::BTreeSet::new();
        for d in error_code_registry() {
            assert!(
                seen.insert(d.code),
                "duplicate error code {} in the registry",
                d.code
            );
        }
    }

    #[test]
    fn app_codes_are_in_the_reserved_range() {
        // JSON-RPC 2.0 reserves [-32099, -32000] for implementation-defined errors.
        for &e in AppError::ALL {
            let c = e.code();
            assert!(
                (-32099..=-32000).contains(&c),
                "app code {c} ({}) is outside the reserved range",
                e.name()
            );
        }
    }

    /// The `docs/rpc` error table is asserted from the registry: the set of codes
    /// documented there must equal the set the daemon can emit. This is the test the
    /// §16.8 docs audit motivated — it fails if a code exists but is undocumented
    /// (the original `-32001` bug) or if the docs list a code the daemon cannot emit.
    #[test]
    fn docs_rpc_table_matches_the_registry() {
        let readme = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../docs/rpc/README.md"
        ))
        .expect("docs/rpc/README.md is readable from the workspace");

        let registry: std::collections::BTreeSet<i64> =
            error_code_registry().iter().map(|d| d.code).collect();

        // Every registry code must be documented as a backtick-wrapped literal in
        // the table (the JSON example uses bare `"code":-32003`, so backticks
        // distinguish table rows).
        for &code in &registry {
            assert!(
                readme.contains(&format!("`{code}`")),
                "error code {code} is in the registry but not documented in docs/rpc/README.md"
            );
        }

        // Every backtick-wrapped error code in the docs must be a real registry code
        // — the reserved [-32768, -32000] range picks out error codes and excludes
        // the mode literals (`0600`, `0660`) elsewhere in the page.
        let documented: std::collections::BTreeSet<i64> = readme
            .split('`')
            .filter_map(|t| t.trim().parse::<i64>().ok())
            .filter(|c| (-32768..=-32000).contains(c))
            .collect();
        assert_eq!(
            documented, registry,
            "docs/rpc/README.md documents a code set that differs from the registry"
        );

        // Beyond the code column, each registry row's name and summary must appear
        // verbatim in the README's markdown table row for that code (§16.8). This
        // makes `ErrorCodeDoc.name`/`.summary` load-bearing: editing a description in
        // either the registry or the docs without matching the other fails the gate.
        for d in error_code_registry() {
            let row = format!("| `{}` | {} | {} |", d.code, d.name, d.summary);
            assert!(
                readme.contains(&row),
                "docs/rpc/README.md is missing the exact table row for error code {}:\n{row}",
                d.code
            );
        }
    }

    // --- the registry's own roster (plan §18 item 64(e)) --------------------------

    /// Drop `//`-comments so a variant named in prose cannot enter either roster.
    /// Naive about `//` inside a string literal, which neither block below contains
    /// — and if one ever does, the scanner truncates a line rather than inventing a
    /// variant, so the failure is a missing entry rather than a phantom one.
    fn strip_line_comments(src: &str) -> String {
        src.lines()
            .map(|l| l.split_once("//").map_or(l, |(head, _)| head))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The body of the first `needle { … }` block, balanced on braces.
    fn braced_block<'a>(src: &'a str, needle: &str) -> &'a str {
        let at = src
            .find(needle)
            .unwrap_or_else(|| panic!("{needle} is in this file"));
        let open = src[at..].find('{').expect("the block opens") + at;
        let mut depth = 0usize;
        for (i, c) in src[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return &src[open + 1..open + i];
                    }
                }
                _ => {}
            }
        }
        panic!("{needle}'s block never closes");
    }

    /// Every variant `enum AppError` declares, read out of this file's own source.
    ///
    /// A variant is an identifier at the head of a line inside the enum body,
    /// followed by `,`, `(`, `{` or `=` — the shapes a Rust variant can take, unit
    /// and tuple and struct and explicitly-discriminated. Attribute and doc lines are
    /// skipped rather than parsed. The list is deliberately over-wide: a shape this
    /// scanner does not know is a variant it silently omits, which is a licence to
    /// leave that variant out of `ALL` — the exact hole this gate closes.
    fn declared_app_error_variants(src: &str) -> std::collections::BTreeSet<String> {
        let body = strip_line_comments(braced_block(src, "pub enum AppError"));
        let mut out = std::collections::BTreeSet::new();
        for line in body.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let end = line
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .unwrap_or(line.len());
            let (ident, rest) = line.split_at(end);
            if ident.is_empty() || !ident.starts_with(|c: char| c.is_ascii_uppercase()) {
                continue;
            }
            if [",", "(", "{", "="]
                .iter()
                .any(|p| rest.trim_start().starts_with(p))
            {
                out.insert(ident.to_owned());
            }
        }
        out
    }

    /// Every variant `AppError::ALL` lists, in the order it lists them.
    fn app_error_all_entries(src: &str) -> Vec<String> {
        let at = src
            .find("pub const ALL: &'static [AppError] = &[")
            .expect("the ALL constant is in this file");
        let end = src[at..].find("];").expect("the ALL constant closes") + at;
        strip_line_comments(&src[at..end])
            .split("AppError::")
            .skip(1)
            .map(|t| {
                t.chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect::<String>()
            })
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// **`AppError::ALL` must list every variant the enum declares** (plan §18 item
    /// 64(e)).
    ///
    /// `ALL` is the registry's application half and everything downstream of it is
    /// derived: [`error_code_registry`] builds the docs table from it, and
    /// `docs_rpc_table_matches_the_registry` above asserts the table two ways. But
    /// both directions run **over `ALL`**, never over the enum — so a ninth variant
    /// added to `AppError` and forgotten here compiles (`code`, `name` and `summary`
    /// are exhaustive matches and force *their* arms to be written), is emittable by
    /// the daemon the moment something constructs it, and is documented by nothing.
    /// Every gate in this file would stay green, because a code that is in neither
    /// the registry nor the docs is in neither set the comparison sees.
    ///
    /// So this reads the enum itself. Both rosters come out of this file's source
    /// rather than out of a list someone typed a third time, which is `meta_derive`'s
    /// doctrine one crate over: a list typed once is correct once.
    #[test]
    fn app_error_all_lists_every_variant_the_enum_declares() {
        // 0. The matcher, in every spelling it claims to cover and against the
        //    near-misses that must not trip it.
        let synthetic = "pub enum AppError {\n\
             /// A doc comment naming AppError::Ghost, which is prose.\n\
             #[allow(dead_code)]\n\
             Plain,\n\
             Tuple(u8),\n\
             Braced { n: u8 },\n\
             Numbered = 9,\n\
             // Commented, \n\
             }";
        assert_eq!(
            declared_app_error_variants(synthetic),
            ["Braced", "Numbered", "Plain", "Tuple"]
                .into_iter()
                .map(str::to_owned)
                .collect::<std::collections::BTreeSet<_>>(),
            "the variant scanner misses a variant shape, reads a commented-out one, \
             or manufactures one out of a doc comment — each of which turns this gate \
             into a demand that `ALL` list something that does not exist, or a licence \
             to omit something that does"
        );
        assert_eq!(
            app_error_all_entries(
                "pub const ALL: &'static [AppError] = &[\n\
                 AppError::Plain,\n\
                 // AppError::Ghost,\n\
                 AppError::Tuple,\n];"
            ),
            vec!["Plain".to_owned(), "Tuple".to_owned()],
            "the ALL scanner reads a commented-out entry as a listing — which would \
             let a variant be dropped from the shipped slice while this gate called it \
             present"
        );

        // 1. Both rosters, from this file's own source, each with a floor: two sets
        //    enumerated to zero are equal forever.
        let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"))
            .expect("this crate's source is readable from its own manifest dir");
        let declared = declared_app_error_variants(&src);
        let listed = app_error_all_entries(&src);
        assert!(
            declared.len() >= 8,
            "the enum scanner found {} variants — `AppError` was reshaped and this \
             comparison is now against almost nothing: {declared:?}",
            declared.len()
        );
        assert!(
            listed.len() >= 8,
            "the ALL scanner found {} entries — the constant was reshaped and this \
             comparison is now against almost nothing: {listed:?}",
            listed.len()
        );

        // 2. The comparison, both ways.
        let listed_set: std::collections::BTreeSet<String> = listed.iter().cloned().collect();
        assert_eq!(
            listed_set.len(),
            listed.len(),
            "`AppError::ALL` lists a variant twice, so `error_code_registry` emits a \
             duplicate row: {listed:?}"
        );
        assert_eq!(
            declared, listed_set,
            "`AppError::ALL` and the `AppError` enum disagree. A variant the enum \
             declares and `ALL` omits is emittable, undocumented, and invisible to \
             `docs_rpc_table_matches_the_registry`, whose comparison runs over `ALL` \
             on both sides"
        );

        // 3. And the property `ALL`'s own doc comment states — "in code order" —
        //    which nothing read. It is what fixes the order of the docs table rows
        //    `error_code_registry` builds, so a reordering here silently reorders a
        //    document that is asserted line by line elsewhere.
        let codes: Vec<i64> = listed
            .iter()
            .map(|n| {
                AppError::ALL
                    .iter()
                    .find(|e| format!("{e:?}") == *n)
                    .unwrap_or_else(|| panic!("{n} is an AppError"))
                    .code()
            })
            .collect();
        let mut ordered = codes.clone();
        ordered.sort_by(|a, b| b.cmp(a));
        assert_eq!(
            codes, ordered,
            "`AppError::ALL` is not in code order, which its own doc comment claims \
             and the docs table's row order depends on"
        );
    }
}
