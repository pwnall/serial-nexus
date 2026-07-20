#![forbid(unsafe_code)]

//! S5 — RPC skeleton (design §10, plan phase 0).
//!
//! Question: does newline-delimited JSON-RPC 2.0 over a Unix domain socket,
//! using the `nexus-rpc` serde types, round-trip a request/response and a
//! notification, and does it reject batches? This spike fixes the `nexus-rpc`
//! type shapes by exercising them end to end over a real socket.
//!
//! Self-judging: prints one JSON verdict line and exits nonzero on any mismatch.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::thread;

use nexus_rpc::{
    Id, Incoming, Notification, Request, Response, error_codes, parse_incoming_request,
};
use serde_json::json;

fn main() {
    let verdict = run();
    println!("{verdict}");
    let pass = verdict
        .get("pass")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    std::process::exit(if pass { 0 } else { 1 });
}

fn run() -> serde_json::Value {
    match exercise() {
        Ok(()) => json!({
            "tool": "s5_rpc",
            "spike": "S5",
            "question": "newline-delimited JSON-RPC 2.0 over UDS round-trips and rejects batches",
            "req_resp_ok": true,
            "notification_ok": true,
            "batch_rejected": true,
            "pass": true
        }),
        Err(e) => json!({
            "tool": "s5_rpc",
            "spike": "S5",
            "error": e.to_string(),
            "pass": false
        }),
    }
}

fn exercise() -> anyhow::Result<()> {
    // Batch rejection is a pure check on the parser (§10).
    let batch = r#"[{"jsonrpc":"2.0","id":1,"method":"state"}]"#;
    match parse_incoming_request(batch) {
        Err(e) if e.code == error_codes::INVALID_REQUEST => {}
        other => anyhow::bail!("batch array was not rejected as invalid request: {other:?}"),
    }

    // A connected socket pair standing in for the control socket.
    let (client, server) = UnixStream::pair()?;

    // Server thread: read one request line, answer it, then push a
    // notification (the `subscribe` shape).
    let server_thread = thread::spawn(move || -> anyhow::Result<()> {
        let mut reader = BufReader::new(server.try_clone()?);
        let mut writer = server;

        let mut line = String::new();
        reader.read_line(&mut line)?;
        let req = parse_incoming_request(&line)
            .map_err(|e| anyhow::anyhow!("server failed to parse request: {}", e.message))?;
        anyhow::ensure!(req.method == "state", "unexpected method {}", req.method);

        let resp = Response::success(req.id.clone(), json!({"nodes": [], "echo": req.params}));
        writer.write_all(nexus_rpc::to_line(&resp).as_bytes())?;

        let note = Notification::new(
            "node.status",
            Some(json!({"node": "usb0", "status": "active"})),
        );
        writer.write_all(nexus_rpc::to_line(&note).as_bytes())?;
        writer.flush()?;
        Ok(())
    });

    // Client side: send a request, read the correlated response, then the
    // notification — distinguishing the two structurally via `Incoming`.
    let mut reader = BufReader::new(client.try_clone()?);
    let mut writer = client;

    let req = Request::new(1, "state", Some(json!({"probe": true})));
    writer.write_all(nexus_rpc::to_line(&req).as_bytes())?;
    writer.flush()?;

    let mut resp_line = String::new();
    reader.read_line(&mut resp_line)?;
    match serde_json::from_str::<Incoming>(resp_line.trim())? {
        Incoming::Response(r) => {
            anyhow::ensure!(r.id == Id::Number(1), "response id mismatch: {:?}", r.id);
            anyhow::ensure!(r.is_success(), "expected a success response");
            let echoed = r
                .result
                .as_ref()
                .and_then(|v| v.get("echo"))
                .cloned()
                .unwrap_or(json!(null));
            anyhow::ensure!(
                echoed == json!({"probe": true}),
                "params not echoed: {echoed}"
            );
        }
        Incoming::Notification(_) => anyhow::bail!("got a notification where a response was due"),
    }

    let mut note_line = String::new();
    reader.read_line(&mut note_line)?;
    match serde_json::from_str::<Incoming>(note_line.trim())? {
        Incoming::Notification(n) => {
            anyhow::ensure!(
                n.method == "node.status",
                "unexpected notification {}",
                n.method
            );
        }
        Incoming::Response(_) => {
            anyhow::bail!("got a second response where a notification was due")
        }
    }

    server_thread
        .join()
        .map_err(|_| anyhow::anyhow!("server thread panicked"))??;
    Ok(())
}
