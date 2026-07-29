//! Phase 12 — the web console's **operator-declared Host names** (review
//! `docs/37-claude-fable-code-review.md`, 37-WEBS-2).
//!
//! One file per defect area, following the `p9_*`/`p10_*`/`p11_*` convention (§5).
//!
//! `--host` exists so the §15.29 Host gate — the DNS-rebinding defense, applied to
//! every request, loopback or not — can accept the name an operator actually reaches
//! the console by behind `--tls`. The gate compares against the request's Host with the
//! *port stripped*, while the flag's values were compared verbatim, so a value written
//! with a port (`example.com:8443`, which is both what the help invites and what every
//! browser puts in the header) could never match: the console answered `403 unrecognized
//! Host` to every request, its own bootstrap URL included, and said nothing at startup
//! to connect the two.
//!
//! The unit test in `web/src/server.rs` pins the normalizer and the comparison against
//! each other. This file carries the one shape it cannot: that the **flag is wired to
//! it** in the shipped binary. That seam is where the defect lived — there was nothing
//! wrong with either half on its own — so a guard that never runs `serial-nexus-web`
//! would pin the wrong thing.
//!
//! No daemon and no serial device: the Host gate runs before the token gate and long
//! before anything reaches the control socket, and the assets answer without one (§5).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serial_nexus_itest::{TempRun, bin, wait_until};

/// The fixed per-session token; nothing here turns on its value.
const TOKEN: &str = "webs2token0123456789abcdef";

/// The name an operator declares, with the port they reach it on — the spelling the
/// finding is about.
const DECLARED: &str = "console.lab:8443";
/// …and the same name as a browser's Host header carries it.
const AS_SENT: &str = "console.lab:8443";

// ---------------------------------------------------------------- child process ----

/// A `serial-nexus-web` child whose printed bootstrap URL is scanned for the OS-chosen
/// port. Killed on drop.
struct WebServer {
    child: Child,
    port: u16,
}

impl WebServer {
    /// Spawn the server with the given extra arguments appended.
    fn spawn(socket: &Path, xdg: &Path, extra: &[&str]) -> WebServer {
        let socket_str = socket.to_string_lossy().into_owned();
        let mut args: Vec<String> = ["--bind", "127.0.0.1:0", "--token", TOKEN, "--socket"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        args.push(socket_str);
        args.extend(extra.iter().map(|s| s.to_string()));
        let mut child = Command::new(bin("serial-nexus-web"))
            .args(&args)
            .env("XDG_RUNTIME_DIR", xdg)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn serial-nexus-web");
        // Drained for the child's whole life: the tracing subscriber writes to stdout
        // too, and a pipe whose read end we dropped would take the server down on its
        // next log line.
        let stdout = child.stdout.take().expect("piped serial-nexus-web stdout");
        let lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = lines.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(l) => sink.lock().unwrap().push(l),
                    Err(_) => break,
                }
            }
        });
        let mut port = None;
        let found = wait_until(Duration::from_secs(10), || {
            for l in lines.lock().unwrap().iter() {
                if let Some(rest) = l.split("http://").nth(1)
                    && let Some(authority) = rest.split('/').next()
                    && let Some((_, p)) = authority.rsplit_once(':')
                    && let Ok(n) = p.trim().parse::<u16>()
                {
                    port = Some(n);
                    return true;
                }
            }
            false
        });
        assert!(
            found,
            "serial-nexus-web never printed its bound http URL; saw {:?}",
            lines.lock().unwrap()
        );
        WebServer {
            child,
            port: port.expect("a port once the URL line was found"),
        }
    }
}

impl Drop for WebServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ------------------------------------------------------------------ HTTP helpers ----

/// `GET <target>` with an explicit `Host` and optional `Cookie`, returning
/// `(status, the response head)`.
fn get(port: u16, target: &str, host: &str, cookie: Option<&str>) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect web server");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set read timeout");
    let mut req = format!("GET {target} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    if let Some(c) = cookie {
        req.push_str(&format!("Cookie: {c}\r\n"));
    }
    req.push_str("\r\n");
    stream.write_all(req.as_bytes()).expect("write request");
    stream.flush().expect("flush request");
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(0) | Err(_) => break,
            Ok(_) => buf.push(byte[0]),
        }
        if buf.len() >= 4 && &buf[buf.len() - 4..] == b"\r\n\r\n" {
            break;
        }
    }
    let text = String::from_utf8_lossy(&buf).into_owned();
    let status = text
        .split("\r\n")
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse().ok())
        .unwrap_or_else(|| panic!("no HTTP status for GET {target} (Host {host}):\n{text}"));
    (status, text)
}

/// The `name=value` pairs of every `Set-Cookie` in a response head, joined as a browser
/// would replay them.
fn jar(head: &str) -> String {
    head.split("\r\n")
        .filter_map(|l| l.split_once(':'))
        .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
        .filter_map(|(_, v)| v.trim().split(';').next())
        .collect::<Vec<_>>()
        .join("; ")
}

// ---------------------------------------------------------------------- the test ----

/// Review **37-WEBS-2**: a `--host` value written with a port is a name the console
/// actually serves, rather than an entry that can never match.
#[test]
fn an_operator_host_written_with_a_port_is_accepted_end_to_end() {
    let run = TempRun::new();
    let web = WebServer::spawn(&run.socket(), run.path(), &["--host", DECLARED]);

    // The bootstrap URL, opened under the declared name — the operator's very first
    // request, and the one that used to 403 with the flag set exactly as documented.
    let (code, head) = get(web.port, &format!("/?token={TOKEN}"), AS_SENT, None);
    assert_eq!(
        code, 302,
        "the bootstrap URL 403s under the name it was told to serve (--host {DECLARED}, \
         Host {AS_SENT}), so every request from that browser does:\n{head}"
    );
    let cookies = jar(&head);
    assert!(!cookies.is_empty(), "the bootstrap set no cookie:\n{head}");

    // …and so does everything after it.
    let (code, head) = get(web.port, "/app.js", AS_SENT, Some(&cookies));
    assert_eq!(code, 200, "the console's assets under {AS_SENT}:\n{head}");

    // The port genuinely is not this gate's business: it answers "is this a name we
    // serve", and Origin is what pins the port, against the request's own authority
    // (§15.29). So the same name on another port is still served…
    let (code, _) = get(web.port, "/app.js", "console.lab:9999", Some(&cookies));
    assert_eq!(code, 200, "the Host gate matches names, not ports");
    // …and the loopback defaults keep working beside the declared name.
    let (code, _) = get(
        web.port,
        "/app.js",
        &format!("127.0.0.1:{}", web.port),
        Some(&cookies),
    );
    assert_eq!(code, 200, "the localhost family is always served");

    // What must still be refused is a name nobody declared: the gate is what stops a
    // page that rebinds DNS to 127.0.0.1 from being answered under its own name.
    for host in ["evil.example", "evil.example:8443", "notconsole.lab"] {
        let (code, _) = get(web.port, "/app.js", host, Some(&cookies));
        assert_eq!(
            code, 403,
            "Host {host:?} was never declared and must be refused (§15.29)"
        );
    }
}

/// A `--host` value whose authority cannot be parsed is a startup failure naming the
/// flag, rather than an entry that silently matches nothing — the same reasoning as the
/// half-present TLS pair (`p12_web_tls.rs`): a console that comes up and refuses
/// everything is worse than one that refuses to come up.
#[test]
fn an_unparsable_host_value_is_a_startup_failure_naming_the_flag() {
    let run = TempRun::new();
    let socket = run.socket().to_string_lossy().into_owned();
    let mut child = Command::new(bin("serial-nexus-web"))
        .args([
            "--bind",
            "127.0.0.1:0",
            "--token",
            TOKEN,
            "--socket",
            &socket,
            "--host",
            "console.lab:not-a-port",
        ])
        .env("XDG_RUNTIME_DIR", run.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn serial-nexus-web");
    // Waited for with a deadline rather than `output()`, which is `p12_web_tls.rs`'s
    // shape and for its reason: a binary that *does* come up would park a blocking read
    // forever, and a guard whose failure mode is a hung suite is worse than no guard.
    let exited = wait_until(Duration::from_secs(20), || {
        matches!(child.try_wait(), Ok(Some(_)))
    });
    if !exited {
        let _ = child.kill();
        let _ = child.wait();
        panic!(
            "serial-nexus-web came up with an unparsable --host value: it would then \
             answer 403 to every request, which is the failure the refusal exists to \
             turn into a message"
        );
    }
    let status = child.wait().expect("wait for serial-nexus-web");
    assert!(
        !status.success(),
        "an unparsable --host must be a startup failure, got {status}"
    );
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("piped serial-nexus-web stderr")
        .read_to_string(&mut stderr)
        .expect("read serial-nexus-web stderr");
    assert!(
        stderr.contains("--host") && stderr.contains("console.lab:not-a-port"),
        "the refusal must name the flag and the value: {stderr}"
    );
}
