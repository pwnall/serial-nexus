# Security posture

This page states, plainly, what `serial_nexus` defends and what it does not — so a
deployment decision is made with eyes open rather than by assumption. It is the
operator-facing companion to the normative design (§9 wire protocol, §10 control
plane, §7.4 leg node, §7.6 exec codec). Where the two disagree, the design wins.

One sentence carries the whole threat model, and the documentation says it in
exactly those words:

> Serial consoles are frequently root shells and bootloader prompts.

Everything below follows from taking that literally. A stream you can write to is,
often enough, a root prompt on the other end; a stream you can read is its output.
So access to a `serial_nexus` console is not "access to a log" — it is device
control, at whatever privilege the far side runs.

## Security posture, version one

The design fixes the v1 posture in §9. Reproduced faithfully:

> Security posture, version one: legs bind and dial loopback only; SSH port
> forwarding (or streamlocal forwarding for Unix-socket legs) provides
> confidentiality and authentication between machines. Non-loopback addresses
> require `insecure_bind = true` — a named footgun beats the patched binary
> someone would otherwise ship. Serial consoles are frequently root shells and
> bootloader prompts; the documentation says so in exactly those words. In-daemon
> TLS is deferred work (§14).

The rest of this page unpacks each clause into operational guidance.

## The authorization model is the control socket's file permissions

The daemon listens on one Unix domain socket. There is no network control plane,
no password, no token: **socket permissions *are* the authorization model —
whoever can open the socket owns every console** (§10). Filesystem permissions
are the entire access-control surface — but they are not the permissions of one
path. The control socket is the door into the *control plane*; a `pty` node's
slave device and a `listen` leg's socket are doors into the same consoles that
the control socket never sees, and each is governed by its own file mode. All
three are enumerated below, because a deployment is only as tight as its widest
one.

**Socket path.** Chosen by privilege, overridable with `--socket`:

- running as root: `/run/serialnexusd.sock`
- otherwise: `$XDG_RUNTIME_DIR/serialnexusd.sock`
  (and, with no `XDG_RUNTIME_DIR`, `/tmp/serialnexusd-<uid>.sock`)

**Socket mode.** Owner-only by default; group-widened on request:

- default: mode `0600` — only the daemon's user can connect.
- `--socket-group <grp>`: the daemon `chgrp`s the socket to `<grp>` and relaxes
  the mode to `0660`, so members of that group can connect. Nothing wider than a
  group is offered.

```sh
# Owner-only (default): only the user running serialnexusd can drive it.
serialnexusd

# Let a trusted operator group in, and nobody else.
serialnexusd --socket-group consoleops
```

Because a serial console is so often a root shell or a bootloader prompt, opening
the control socket is equivalent to full device control over every node in the
graph. **Treat the socket like a root credential.** Anyone you add to
`--socket-group` you are, in effect, handing the equivalent of console-level root
on every attached device. Grant it as narrowly as you would grant `sudo`.

The same logic extends to the console endpoints themselves. A PTY node's slave
device is created `0600` by default (owner only), widened to `0660` only when a
group is configured (§7.2); those permissions gate `open(2)` on the pts node and
are the second door into the same console. Keep them as tight as the socket.

**Every file the daemon creates carries an explicit mode, never the umask.**
Console bytes are frequently root shells, so nothing the daemon writes inherits
whatever `umask` it happened to be started with:

| Artifact | Mode | Note |
|---|---|---|
| control socket | `0600` | `0660` + `chgrp` with `--socket-group` (above) |
| PTY slave device | `0600` | `0660` when the node configures a group (§7.2) |
| state snapshot (`--state-file`) | `0600` | owner-only, and **not** widened by `--socket-group`: it records exec-codec argv and environment and names every console (§11) |
| log files | `0640` | at creation only, so an operator's later `chmod` survives; the group bit is what a deployment widens by placing the directory under a group |
| `listen` + `unix` leg socket | `0600` | no group widen — see the leg section below |

The sockets, the pts node and the state file get their mode from an explicit
`chmod` after creation, so the value in the table is exact; the log file's is a
creation ceiling the umask may narrow further. The daemon never widens. The
directories still matter regardless — put the socket and the state file under a
`0700` parent (`RuntimeDirectoryMode=0700`, `StateDirectoryMode=0700`; see the
checklist), because the `chmod` necessarily lands one syscall *after* the
`bind(2)`/`create(2)`, and a `0700` parent closes that window.

## The replay ring: the daemon holds 64 KiB of every console, by default

Since §15.32 **every host-facing endpoint carries a replay ring, on by default at
65536 bytes** (`replay_ring = <bytes>` per node; `0` opts out). It is scrollback,
retained so a console does not punish whoever arrives late — and it means the
daemon is holding, in memory, the last 64 KiB each console emitted, whether or
not anyone was watching.

The access-control consequence is one sentence: **anyone who can open the control
socket can replay it.** `tap.open` with `replay: true` (`serialnexusctl tap
<endpoint> --replay`) hands back the endpoint's ring snapshot and then the live
stream, and a tap needs nothing but a connection — so the bytes a root shell
printed *before* an operator connected are as reachable as the bytes it prints
after. The web console requests exactly this on every console it opens (§17),
which puts a console's recent past one click behind whoever holds the web token.
Nothing here weakens the socket's `0600`; it widens what holding the socket
*buys*, from "watch this console from now on" to "and read what it just said."

Three things bound it, and they are worth knowing before deciding it is fine:

- **It is bounded and in memory.** A ring is a fixed circular buffer per
  host-facing endpoint (`replay_ring` bytes, structurally capped at 16 MiB), never
  written to disk, and gone when the daemon exits. Watching a console is a tap, not
  a log node: viewing never becomes an unasked-for recording (§15.28).
- **It cannot hide loss.** The ring mirror is a spy *outside* the graph, with its
  own `feed_dropped` counter, and `discarded_unattached` counts graph-consumer
  absence independently of it — so a ring can never quietly stand in for a log.
- **You can turn it off.** `replay_ring = 0` on a node opts that node's endpoints
  out entirely. Do that for a console whose recent output is more sensitive than
  its liveness is useful — a key-material prompt, a recovery console — rather than
  relying on nobody thinking to ask for a replay.

The browser's own history is a separate, larger exposure with a separate answer:
`serialnexusweb` persists per-console scrollback in the browser's Origin Private
File System (default cap 16 MiB per console), **unencrypted in the browser
profile**. On a shared machine — and doubly under the insecure bind tier —
clearing site data is part of walking away (§15.32).

## Cross-machine legs: loopback by default, SSH for the wire

A **leg** is the cross-daemon transport (§7.4). In v1 the wire itself carries no
cryptography — confidentiality and authentication between machines are the
operator's to layer on, and the design's answer is SSH:

- **loopback only, by default.** A leg's bind (`listen`) or dial (`connect`)
  address must resolve to loopback — `127.0.0.1`, `::1`, `localhost`, or a `unix`
  socket path (Unix sockets are inherently local). A non-loopback TCP address is
  rejected at load time.
- **SSH provides auth + confidentiality.** For a TCP leg, forward the loopback
  port over SSH (`ssh -L`/`-R`). For a `unix` leg, OpenSSH *streamlocal*
  forwarding tunnels the Unix socket directly, skipping TCP entirely (§7.4). SSH
  authenticates both ends and encrypts the link; `serial_nexus` inherits that
  guarantee without carrying its own crypto.

```sh
# Computer B (leg listens on loopback); Computer A dials it through SSH.
#   B:  leg role=listen  address=127.0.0.1:7000  (loopback-only, default)
#   A:  leg role=connect address=127.0.0.1:7000  reached via the tunnel below
ssh -N -L 127.0.0.1:7000:127.0.0.1:7000 operator@computer-b
```

**A `listen` leg is a further, unauthenticated door into the consoles bound to
it — state it plainly.** The v1 wire has no authentication of its own (§9), so
anyone who can *reach* a listening leg's address can dial it, complete the hello
handshake, read every channel that leg carries, and write into any of them that
has a writable local edge. There is no token, no peer credential check, and
nothing about a leg peer that the control socket's `0600` mode governs. What
bounds reachability is therefore the whole of the leg's access control, and it
differs by transport:

- **`transport = "unix"`:** the listener socket is created **`0600`** — the same
  policy point the control socket uses — so only the daemon's own user can dial
  it. There is deliberately no group widen: `--socket-group` has no plumbing to a
  leg node, and a leg is `0600`, full stop. The `chmod` lands one syscall after
  the `bind(2)`, so put the address under a directory only the daemon's user can
  traverse, exactly as for the control socket.
- **`transport = "tcp"`:** loopback-only by default, which means "every local
  user", not "only me" — a loopback port has no file mode. Treat a TCP leg as
  reachable by anyone with an account on the box, and use SSH forwarding for
  anything beyond that.

A `listen` + `unix` leg also refuses to unlink the path it was given unless it is
provably a stale socket: `address` must be a socket (checked with
`symlink_metadata`, so a symlink is refused as itself rather than followed), and
a dial to it must be refused promptly (`ECONNREFUSED`). A live peer, a regular
file, or an answer that is neither prompt nor a refusal all fault the node with
the reason in `state` instead of deleting anything — so `address =
"/home/me/notes.txt"` costs you a faulted node, not the file. Teardown unlinks
only the inode that leg actually bound.

**The opt-out is deliberately ugly.** Binding or dialing a non-loopback address
requires a per-leg configuration attribute, spelled exactly:

```toml
[[node]]
type = "leg"
name = "uplink"
transport = "tcp"
role = "listen"
address = "0.0.0.0:7000"
insecure_bind = true          # required for any non-loopback bind/dial
channels = ["console"]
```

Without `insecure_bind = true`, a non-loopback TCP address fails validation with a
`NonLoopbackBind` error (`nexus-core/src/config.rs`), and the load creates nothing
(§11). The flag is named to be greppable and impossible to set by accident: **a
named footgun beats the patched binary someone would otherwise ship.** The point
is not to make remote exposure impossible — it is to make it a recorded, auditable
choice that shows up in `dump` output and in a `grep insecure_bind` across your
configs, rather than a silent default someone quietly forked the code to obtain.

## The web console: three request gates, a bounded pre-auth path, three bind tiers

`serialnexusweb` (§17) is a **separate process** — a pure client of the daemon's
control socket on one side, and an HTTP + WebSocket server for a browser on the
other. The daemon does not link it, serve it, or know it exists; the web server is
"simply a client that holds the socket, and whoever holds the token holds exactly
what the web server holds."

The delta that shapes its security is one sentence: **the control socket is mode
0600, but a loopback TCP port is reachable by every local user.** So the web
server cannot lean on file permissions the way the daemon does. Three checks
replace them, applied to every request including the WebSocket upgrade, and they
answer three *different* questions — none substitutes for another (§15.29):

- **The token answers *who may act*.** Every request and every WebSocket upgrade
  requires a per-session bearer token — 256 bits from the OS CSPRNG, generated at
  startup and printed as a ready-to-open URL (`http://127.0.0.1:PORT/?token=…`,
  Jupyter-style). Opening that URL sets the token as a `HttpOnly; SameSite=Strict`
  session cookie and drops it from the address bar; every later request (assets and
  the WS upgrade alike) carries the cookie, whose `SameSite=Strict` policy doubles
  as CSRF protection. No cookie, no access — a request without it gets `401`.
  **The cookie is marked `Secure` exactly when this server
  terminates TLS**, so in the `--tls` tier the token stops riding on any same-host
  plaintext request; it is omitted on the plaintext tiers because a browser refuses
  to store a `Secure` cookie from a non-trustworthy origin at all.
- **Host answers *was this addressed to a name we serve*.** Validated on every
  request against the localhost family plus any `--host` names, so a page that
  rebinds DNS to `127.0.0.1` still fails — its Host is its own, and it gets `403`.
- **`Origin` answers *which page sent it*.** This is the one cookies cannot
  answer: cookies are **not port-scoped**, so a page served from another port on
  the same host is same-*site* and `SameSite=Strict` — a cookie policy, not a
  check — still attaches the session cookie to its `fetch` and its WebSocket. A
  *present* `Origin` must designate this very server (compared against the
  request's own already-validated Host, so SSH-forwarded ports still agree) or the
  request gets `403`; an *absent* one is accepted, because Host is mandatory in
  HTTP/1.1 and `Origin` is not — browsers always send it on a WS handshake and on
  cross-origin fetches, while non-browser clients (`serialnexusweb wsclient`,
  `curl`) never do, and refusing them would break the §17 headless client.

None of those three answers a fourth question — ***who can read and replay***,
which is the *channel's* to answer and the bind policy's to enforce. A bearer
token over plaintext HTTP is a secret broadcast to every on-path observer, who
reads it once and holds console access — root shells, per above — indefinitely.
That is exactly what TLS fixes, and why the token alone is not enough off
loopback.

**Everything before the token check is reachable by an unauthenticated peer** in
the sanctioned non-loopback tiers, so the pre-auth path is bounded in three
dimensions rather than trusted: a **15-second deadline** covers the TLS handshake
and the delivery of a complete request head (a peer that connects and says nothing
is dropped, not held); the request head is capped at **16 KiB**; and in-flight
connections are capped at **128**, the newest refused rather than queued. After
the upgrade, incoming WebSocket messages are capped at **1 MiB** (frames at
256 KiB) — the browser→server direction carries JSON-RPC requests only, so the cap
costs nothing and bounds what one frame can make the server buffer. The hostward
`tap.data` firehose flows the other way and is untouched.

**The bind policy is three-tiered, and the tiers are not interchangeable:**

1. **Loopback + token (the default).** On loopback the kernel is the channel; there
   is nothing on the wire to sniff, so the token needs no crypto. Remote access is
   SSH port forwarding of the loopback port — the same posture as the legs, above.
2. **`--tls` + token (the sanctioned non-loopback mode).** rustls plus the token is
   the configuration in which "the bearer token is like an API key" is *actually
   true*, because every widely deployed API rides an encrypted channel. This is the
   only non-loopback mode that is not a footgun.
3. **`--insecure-bind` (the named footgun).** A non-loopback bind without TLS is
   refused outright unless this flag is set — the same "a named footgun beats a
   patched binary" reasoning as the legs' `insecure_bind`. The token stays mandatory,
   and the flag's own help text states what is forfeited: **every console byte, and
   the token itself, is readable and replayable by anyone on the network path.** Use
   it only on a network you genuinely trust; prefer `--tls` or SSH forwarding.

What the web console **cannot** do is as load-bearing as what it can. It never
mutates the graph, and the bridge enforces that with an **allowlist**, not a
denylist: the browser may invoke exactly `state`, `subscribe`, `info`, `dump`,
`tap.open`, `tap.close`, `send`, `lock`, `unlock`, `rotate`, `send-break`,
`set-modem`, `pulse-dtr` — and **everything else is refused with `-32601`**,
including a verb §10 grows tomorrow. That direction is the load-bearing half: a
denylist admits every future verb and keeps the non-goal true only for as long as
someone remembers to extend it, which is exactly how a stated boundary erodes. So
`load`, `add-node`, `remove-node`, `teardown`, `shutdown` and the §14-deferred
live-surgery verbs never reach the daemon, and a compromised page cannot
reconfigure it — it can only watch consoles, send lines, and arbitrate the write
lock (explicit steal only, never automatic).

Two properties make that screen binding rather than decorative, both settled by a
reproduced bypass (review 26, WEB-1/SEC-1). **One frame is exactly one request:**
a frame that does not parse to a single JSON *object* — a batch array, a scalar, or
two newline-separated requests — is refused outright with `-32600`. And **the
bridge forwards the parsed value re-serialized, never the browser's raw text**, so
the request that reaches the daemon's newline-delimited socket is byte-for-byte the
one the screen approved, and no second request can ride behind an embedded newline.
A screen that decides on a different object than the one it transmits is not a
screen.

And it never writes to disk on the daemon's behalf: watching a console is a tap,
not a log node, so viewing never becomes an unasked-for recording. The browser's
own OPFS scrollback is the exception, and it is the viewer's disk, not the
daemon's — see the replay-ring section above.

## The exec codec child runs as the daemon's user

The exec codec (§7.6) is the escape hatch for proprietary framing: it spawns a
child process, from operator-supplied argv and environment, that speaks the fixed
envelope protocol on stdin/stdout. Stated plainly, as the design states it:

> The child runs as the daemon's user (documented plainly).

There is no sandbox around the exec child in v1. Its argv executes with the
daemon's full privileges and file access — so a codec command line is code you are
choosing to run as the daemon. Vet exec-codec argv and environment with the same
care you would vet anything launched by the account `serialnexusd` runs as, and
prefer running the daemon under a dedicated, unprivileged user (see the checklist
below) so "the daemon's user" is as small a blast radius as possible.

## What v1 deliberately does not do

- **No in-daemon cryptography.** There is no TLS, no wire encryption, and no
  in-protocol authentication inside `serialnexusd`. In-daemon TLS and non-loopback
  legs are recorded as deferred work (§14). Confidentiality and authentication
  between machines are SSH's job, per above; on a single host, the socket's file
  permissions are the boundary.
- **No per-caller authorization — yet, but the hook exists.** Every connection
  that opens the socket is equally trusted. `SO_PEERCRED` remains available for
  finer authorization later *without a protocol change* (§10) — a future release
  can distinguish callers by uid/gid over the very same socket. Do not assume that
  distinction exists today: today it is all-or-nothing at the socket boundary. The
  same gap is what makes a `listen` leg's file mode its whole access control: with
  no peer credential check at accept, reachability *is* authorization.

## Hardening checklist

The controls above are the design's guarantees; these are the deployment steps
that make them tight in practice. The reference systemd unit lives at
`packaging/serialnexusd.service` — install and adapt it rather than running the
daemon by hand.

1. **Run as a dedicated, unprivileged user.** Do not run the daemon as root unless
   a device genuinely requires it. A dedicated identity shrinks what "the daemon's
   user" — and therefore an exec-codec child (§7.6) — can touch. `DynamicUser=yes`
   gives a transient one for free.
2. **Confine the socket's directory.** Point `--socket` at a per-service runtime
   directory the daemon owns exclusively (`RuntimeDirectory=serialnexusd`,
   `RuntimeDirectoryMode=0700`). The `0700` parent bounds the brief post-`bind`
   window before the daemon narrows the socket itself to `0600` (§10). A system
   service has no `XDG_RUNTIME_DIR`, so set `--socket` explicitly.
3. **Give state a reboot-durable home.** The default state snapshot sits beside the
   socket under `/run`, which is cleared on reboot (§11). For persistence across
   reboots, pass `--state-file /var/lib/serialnexusd/state.toml` and provision it
   with `StateDirectory=serialnexusd` (`StateDirectoryMode=0700`).
4. **Widen socket access by group, never wider.** Keep the `0600` default unless a
   second operator truly needs in; then use `--socket-group <grp>` and add only the
   people who should hold console-root-equivalent to that group.
5. **Grant device access by group, not by root.** USB serial adapters are
   typically `root:dialout 0660`. Add the daemon's user to the owning group rather
   than running as root: `SupplementaryGroups=dialout` (or `plugdev`, matching your
   udev rules). A dependency-free rule that puts adapters in a group:

   ```udev
   # /etc/udev/rules.d/70-serial-nexus.rules
   SUBSYSTEM=="tty", SUBSYSTEMS=="usb", ATTRS{idVendor}=="0403", \
     GROUP="plugdev", MODE="0660"
   ```

   `nexus-doctor`'s environment checks (probe P3) verify device permissions and
   group membership and tell you exactly what is missing — run it first when a
   node comes up faulted on a permission error.
6. **Sandbox the service.** The unit applies the standard systemd confinement:
   `NoNewPrivileges=yes`, `ProtectSystem=strict`, `ProtectHome=yes`,
   `PrivateTmp=yes`, kernel/cgroup protections, `RestrictAddressFamilies=AF_UNIX
   AF_INET AF_INET6`, and a `DevicePolicy=closed` scoped to the serial tty nodes
   (`char-ttyUSB`, `char-ttyACM`) plus the pty master/slave devices (`/dev/ptmx`,
   `char-pts`) the daemon needs to allocate PTY nodes — `PrivateDevices` stays off
   precisely so those remain reachable.

The complete, maintained unit is [`packaging/serialnexusd.service`](../packaging/serialnexusd.service)
— install and adapt it rather than copying a snippet here. It applies exactly the
controls above, and two details worth knowing: the default log directory is
provisioned with `LogsDirectory=serialnexusd` (systemd creates *and* chowns it to the
transient user — a bare `ReadWritePaths` would flip the mount without chowning and the
log node would fault on `EACCES`), and the `/dev/ptmx` + `char-pts` device rules are
required or PTY nodes cannot allocate their pairs. See
[`packaging/README.md`](../packaging/README.md) for the full install walk-through.

## In one breath

File permissions are the whole authorization model, and a console is usually a
root shell — so guard the control socket like a root credential and widen it only
by a group you trust that far, remembering that a PTY slave (`0600`/`0660`) and a
`unix` leg listener (`0600`, no group widen) are further doors into the same
consoles, that the v1 wire authenticates nobody, and that a loopback TCP leg has no
file mode at all. Every console also keeps 64 KiB of scrollback in the daemon by
default, replayable by anyone who can open the socket — set `replay_ring = 0` on
the consoles where that matters. Cross-machine legs stay on loopback and ride SSH;
reaching past loopback means writing `insecure_bind = true` on purpose. The web
console gates on token, Host and `Origin`, refuses every verb outside its
allowlist, and bounds its pre-auth path. The exec codec runs as the daemon's user,
so run that user small. There is no in-daemon crypto in v1 — that, and finer
per-caller authorization via `SO_PEERCRED`, are named as future work, not present
guarantees (§14, §10).
