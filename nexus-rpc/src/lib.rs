#![forbid(unsafe_code)]

//! `nexus-rpc` — the JSON-RPC 2.0 wire types shared by `serialnexusd` and
//! `serialnexusctl` (design §10).
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
//!   daemon owns the concrete per-method schemas. This keeps `nexus-rpc` the
//!   thin, stable framing layer and lets version skew degrade gracefully via
//!   the standard `method not found` error.

use serde::{Deserialize, Serialize, de};
use serde_json::Value;

/// The only JSON-RPC version this daemon speaks.
pub const JSONRPC_VERSION: &str = "2.0";

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
}

impl AppError {
    /// Every application error, in code order — the registry's application half.
    pub const ALL: &'static [AppError] = &[
        AppError::LoadNonEmpty,
        AppError::Structural,
        AppError::Locked,
        AppError::HasEdges,
        AppError::DeviceAbsent,
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
        }
    }

    /// A one-line meaning for the docs table.
    pub const fn summary(self) -> &'static str {
        match self {
            AppError::LoadNonEmpty => "`load` without `replace` while a graph is already loaded",
            AppError::Structural => {
                "configuration failed validation; `data.errors` is the list of messages"
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
    let mut s = serde_json::to_string(msg).expect("nexus-rpc types always serialize");
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
    }
}
