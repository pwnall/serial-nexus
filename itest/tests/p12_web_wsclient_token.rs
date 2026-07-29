//! Phase 12 — **how the headless client receives its token** (review
//! `docs/37-claude-fable-code-review.md`, 37-WEBS-1).
//!
//! One file per defect area, following the `p9_*`/`p10_*`/`p11_*` convention (§5).
//!
//! `serial-nexus-web wsclient` is the headless access path §17 sanctions, and it took
//! the per-session bearer token as `--token` on argv and nowhere else. On Linux with
//! the default `/proc` mount, a process's command line is world-readable for as long as
//! it runs, so the one client the project ships for scripted access published a
//! shell-equivalent credential — whoever reaches `/ws` can `add-node` an exec codec
//! (`docs/security.md`) — to precisely the adversary the token exists to gate: §17's
//! "a loopback TCP port is reachable by every local user". The server's own flow never
//! puts the token on argv; only this client did, and it had no alternative to offer.
//!
//! What is pinned here is the capability, end to end through the shipped binary: the
//! token can be supplied **without appearing in the command line at all** — from a file
//! (the preferred spelling, mode 0600) or from the environment — and a client with no
//! token at all says how to give it one. `--token` keeps working, because scripts and
//! this suite's own tests use it and it is the right tool on a single-user box; the
//! finding is that it was the *only* tool.
//!
//! No serial device is involved, so this runs on **every** platform (§5).

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use serial_nexus_itest::{Daemon, bin, wait_until};

/// The fixed per-session token, so "this exact string is absent from argv" is a
/// byte-level fact.
const TOKEN: &str = "webs1envtoken0123456789abcdef";

/// The variable the client reads the token from — mirrors `web/src/wsclient.rs`'s
/// `TOKEN_ENV`, spelled out here because what is under test is the shipped binary
/// honouring this name.
const TOKEN_ENV: &str = "SERIAL_NEXUS_WEB_TOKEN";

// ---------------------------------------------------------------- child process ----

/// A `serial-nexus-web` child whose printed bootstrap URL is scanned for the OS-chosen
/// port. Killed on drop.
struct WebServer {
    child: Child,
    port: u16,
}

impl WebServer {
    fn spawn(socket: &Path, xdg: &Path) -> WebServer {
        let socket_str = socket.to_string_lossy().into_owned();
        let mut child = Command::new(bin("serial-nexus-web"))
            .args([
                "--bind",
                "127.0.0.1:0",
                "--token",
                TOKEN,
                "--socket",
                &socket_str,
            ])
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

// ------------------------------------------------------------------ the client ----

/// One `serial-nexus-web wsclient --rpc info` run: its argv (minus the program), its
/// exit success, stdout and stderr.
struct Run {
    args: Vec<String>,
    ok: bool,
    stdout: String,
    stderr: String,
}

/// Run the headless client against `port`, with `extra` appended to the fixed
/// `--rpc info` shape and `env` set (or, for `None`, explicitly removed).
fn wsclient(port: u16, extra: &[&str], env: Option<&str>) -> Run {
    let url = format!("ws://127.0.0.1:{port}/ws");
    let mut args: Vec<String> = vec![
        "wsclient".into(),
        "--url".into(),
        url,
        "--rpc".into(),
        "info".into(),
    ];
    args.extend(extra.iter().map(|s| s.to_string()));
    let mut cmd = Command::new(bin("serial-nexus-web"));
    cmd.args(&args);
    match env {
        // Removed rather than left inherited: whether this suite's own environment
        // happens to carry the variable must not decide what the test proves.
        None => cmd.env_remove(TOKEN_ENV),
        Some(v) => cmd.env(TOKEN_ENV, v),
    };
    let out = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run serial-nexus-web wsclient");
    Run {
        args,
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

impl Run {
    /// The JSON response the one-shot mode prints, or a panic naming what came instead.
    fn response(&self) -> Value {
        assert!(
            self.ok,
            "the client failed: {:?}\nstdout: {}\nstderr: {}",
            self.args, self.stdout, self.stderr
        );
        serde_json::from_str(self.stdout.trim())
            .unwrap_or_else(|e| panic!("stdout is not the JSON response ({e}): {}", self.stdout))
    }

    /// Whether the token appears anywhere in the command line — the exposure the
    /// finding is about, since `/proc/<pid>/cmdline` is world-readable by default.
    fn token_on_argv(&self) -> bool {
        self.args.iter().any(|a| a.contains(TOKEN))
    }
}

// ---------------------------------------------------------------------- the tests ----

/// Review **37-WEBS-1**: the headless client can authenticate with nothing secret on
/// its command line.
#[test]
fn the_headless_client_takes_its_token_from_a_file_or_the_environment() {
    let d = Daemon::start();
    let web = WebServer::spawn(&d.socket(), d.run().path());

    // The preferred spelling: a file, mode 0600, named on argv while its *contents*
    // stay off it.
    let token_file = d.run().join("wsclient.token");
    let mut f = std::fs::File::create(&token_file).expect("create the token file");
    // Written with the trailing newline `printf '%s\n' "$TOKEN" > tok` produces, which
    // is how one is written in practice.
    writeln!(f, "{TOKEN}").expect("write the token file");
    drop(f);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&token_file, std::fs::Permissions::from_mode(0o600))
            .expect("chmod 600");
    }
    let path = token_file.to_string_lossy().into_owned();
    let run = wsclient(web.port, &["--token-file", &path], None);
    assert!(
        !run.token_on_argv(),
        "the token must not be on the command line: {:?}",
        run.args
    );
    assert!(
        run.response().get("result").is_some(),
        "a token file must authenticate the bridge: {}",
        run.stdout
    );

    // The environment: `/proc/<pid>/environ` is readable by the process's own user,
    // where `cmdline` is readable by every local user — the distinction the finding
    // turns on.
    let run = wsclient(web.port, &[], Some(TOKEN));
    assert!(!run.token_on_argv(), "{:?}", run.args);
    assert!(
        run.response().get("result").is_some(),
        "{TOKEN_ENV} must authenticate the bridge: {}",
        run.stdout
    );

    // `--token` still works — it is the right tool on a single-user box, and the rest
    // of this suite passes it — and it still beats an inherited variable, because an
    // explicit flag is the more specific statement of intent.
    let run = wsclient(web.port, &["--token", TOKEN], Some("not-the-token"));
    assert!(run.token_on_argv(), "precondition: this spelling *is* argv");
    assert!(run.response().get("result").is_some(), "{}", run.stdout);

    // A file beats the environment, being the narrower source.
    let run = wsclient(web.port, &["--token-file", &path], Some("not-the-token"));
    assert!(run.response().get("result").is_some(), "{}", run.stdout);
}

/// A client with no token at all names every spelling it accepts, rather than failing
/// on the one flag it used to require.
#[test]
fn a_client_with_no_token_names_all_three_spellings() {
    let d = Daemon::start();
    let web = WebServer::spawn(&d.socket(), d.run().path());

    let run = wsclient(web.port, &[], None);
    assert!(!run.ok, "a client with no credential must not succeed");
    let said = format!("{}{}", run.stdout, run.stderr);
    for spelling in ["--token-file", TOKEN_ENV, "--token"] {
        assert!(
            said.contains(spelling),
            "the refusal must name {spelling}: {said}"
        );
    }

    // A wrong token is a refusal from the *server*, not a usage error — the two are
    // different problems and must not read alike.
    let run = wsclient(web.port, &[], Some("0000000000000000"));
    assert!(!run.ok, "a wrong token must not authenticate");
    let said = format!("{}{}", run.stdout, run.stderr);
    assert!(
        !said.contains("--token-file"),
        "a rejected token is not a missing one: {said}"
    );
}
