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
  to store a `Secure` cookie from a non-trustworthy origin at all. The cookie's
  *name* is the listener's own — `nexus_session_<bound port>` — because a cookie's
  jar key is (name, domain, path) and never the port (RFC 6265 §8.5): under one
  fixed name the two-console arrangement §17 prescribes collided on a single jar
  entry, and opening the second bootstrap URL silently logged the operator out of
  the first, whose only recovery is a reload that `401`s (review 32, WEB-3). Every
  value the browser sends under that name is checked, not just the first, so a page
  on a sibling port cannot shadow the session by planting a longer-path cookie of
  the same name — RFC 6265 §5.4 orders the longer path first, and a
  first-value-wins reader saw only the plant: the assets (path `/`) still returned
  `200` while `/ws` got `401`, a console that renders normally and never connects.
  A headless client (`serialnexusweb wsclient`, `curl`) may instead present the
  unscoped name `nexus_session`: it is handed a token rather than a cookie jar and
  cannot know the bound port, which under the SSH forwarding §17 sanctions is not
  even the port in its own URL.
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

**Residual: the session cookie is readable by any same-host service the operator's
browser contacts.** Cookies are not port-scoped in *either* direction. The `Origin`
check above closes the inbound half — a sibling-port page cannot make this server
act — but nothing stops the browser from *sending* the cookie to a different service
on another port of the same host, because to the browser that is the same site. If
the operator navigates to a hostile local port while a console session is live, that
service's access log contains the token, and the token is shell-equivalent: its
holder can `add-node` an exec codec and run an arbitrary command line as the
daemon's user. `HttpOnly` does not help (it stops a script reading the cookie, not
the browser sending it), and neither does `SameSite=Strict` (same site is exactly
the problem); naming the cookie after the listener, above, separates two consoles'
jar entries but does not change which ports the browser replays it to.

What *does* narrow it is the cookie's **path**, and that is what ships: the session
cookie carrying the token is `Path=/ws` — the WebSocket upgrade route and nothing
else — so a browser applying RFC 6265 §5.1.4 path-matching sends it only to requests
whose path is `/ws` or below. A navigation to `http://127.0.0.1:9999/`, the shape the
finding reproduced, no longer carries it. The static assets are authorized by a
second, separate `nexus_assets_<port>` credential minted from the OS CSPRNG and
unrelated to the token by construction rather than derived from it, so the value that
*is* replayed across the whole path space is not the one that commands the daemon.

The residual is therefore narrowed, not closed: a hostile sibling-port service that
answers on the path `/ws` still receives the token if the operator's browser is made
to request it. Closing it entirely means keeping the long-lived token out of a
standing, auto-replayed cookie at all — holding it in `sessionStorage` and presenting
it on the upgrade — which no browser permits for the WebSocket handshake without
either a custom subprotocol value or a query parameter, both of which trade this
exposure for a different one (a token in a URL is a token in logs and history). That
trade was declined for now and is recorded rather than hidden. On a multi-user
machine, treat the token as only as safe as every other local port the operator
visits (review 32, WEBS-1).

None of those three checks answers a fourth question — ***who can read and replay***,
which is the *channel's* to answer and the bind policy's to enforce. A bearer
token over plaintext HTTP is a secret broadcast to every on-path observer, who
reads it once and holds console access — root shells, per above — indefinitely.
That is exactly what TLS fixes, and why the token alone is not enough off
loopback.

**Everything before the token check is reachable by an unauthenticated peer** in
the sanctioned non-loopback tiers, so the pre-auth path is bounded in three
dimensions rather than trusted: a **5-second deadline** covers the TLS handshake
and the delivery of a complete request head — a complete head is one round trip, and
a peer that connects and says nothing is dropped, not held; the request head is
capped at **16 KiB**; and the connection pool is split at the token gate (review
32, WEB-5). **128** bounds the connections that have *passed* it — the permit is
taken there, after the credential is known, and a request over the cap is answered
`503` rather than queued, since a queue only moves the exhaustion. What bounds the
population *before* the gate is a separate cap of **32**, and that one is enforced
by **evicting its oldest member, never by refusing an accept**. The distinction is
the whole finding rather than an implementation detail: the cookie cannot be read
until the head is read, so anything that refuses at accept refuses the operator
too. The first attempt at this split did exactly that — a second semaphore taken
in the accept loop — and made the denial four times cheaper instead of closing it,
32 silent sockets resetting every subsequent connection on `/app.js` and `/ws`
alike where the single pool had cost 128. Under eviction a silent peer can no
longer deny anyone; it can only be the thing that gets evicted, and the operator's
browser — always the newest arrival, and out of the population entirely one round
trip later when its cookie is read — is the last candidate rather than the
structural victim. To make the console so much as retry, a flood would have to turn
the whole pre-auth pool over inside that round trip. The residual is that an
evicted connection's socket closes when its task is next polled, so a burst can
leave evicted-but-unreaped tasks briefly alive; they are on their way out rather
than holding a slot for the head deadline. A per-peer-IP cap is deliberately
absent — on the loopback default every local user shares 127.0.0.1 with the
operator, so it would ration the browser without separating it from the attacker. After the upgrade, incoming
WebSocket messages are capped at **1 MiB** (frames at 256 KiB) — the browser→server
direction carries JSON-RPC requests only, so the cap costs nothing and bounds what
one frame can make the server buffer. The hostward `tap.data` firehose flows the
other way and is untouched.

**The served assets carry a `default-src 'none'` policy, and its `connect-src` is
`'self'` alone.** Nothing the page needs is off-origin or inline — scripts and styles
come from this server, the WebSocket is same-origin, the history export is a `blob:`
URL — so the policy costs the console nothing while bounding what a future
DOM-injection slip could do; and since a token holder can now edit the graph, such a
slip would be code execution *as the operator's session* rather than a defaced page.
The bare `ws:`/`wss:` scheme sources the policy used to carry are gone (review 32,
WEBS-2): they let a script open a WebSocket to *any* host, which is exactly the
silent, bidirectional, page-preserving exfiltration channel the rest of this section
assumes is shut, and CSP3's `'self'` matches a same-origin `ws:`/`wss:` connection on
both tiers — measured against this project's own pinned Chromium. Read it as defense
in depth and not as a boundary: CSP cannot stop a top-level navigation to an
attacker's URL, so what it removes is the quiet channel, not every channel.

**The bind policy is three-tiered, and the tiers are not interchangeable:**

1. **Loopback + token (the default).** On loopback the kernel is the channel; there
   is nothing on the wire to sniff, so the token needs no crypto. Remote access is
   SSH port forwarding of the loopback port — the same posture as the legs, above.
2. **`--tls` + token (the sanctioned non-loopback mode).** rustls plus the token is
   the configuration in which "the bearer token is like an API key" is *actually
   true*, because every widely deployed API rides an encrypted channel. This is the
   only non-loopback mode that is not a footgun. **The cert and the key are one
   atomic pair:** `--tls-cert`/`--tls-key` are *loaded* when both paths exist and a
   self-signed lab pair is *generated* when **neither** does, while a half-present
   pair is refused at startup, naming the file that is there and the one that is not.
   The refusal is the substance rather than the tidiness: generating writes both
   paths, so the ordinary CA workflow — make the key first, install the signed cert
   later — used to truncate the operator's private key, unrecoverably, while the
   server came up green and the one log line named only the cert (review 32,
   WEB-1/WEB-2). Presence is decided with `symlink_metadata`, not `exists()`, so a
   dangling symlink planted at either path counts as *present* rather than as an
   invitation to write through it; both files are created with `create_new`; and the
   generated key is narrowed to `0600` explicitly, because an open-time mode applies
   only at creation and is masked by the umask, which is how a regenerate into a
   pre-existing `0644` path once served a world-readable key.
3. **`--insecure-bind` (the named footgun).** A non-loopback bind without TLS is
   refused outright unless this flag is set — the same "a named footgun beats a
   patched binary" reasoning as the legs' `insecure_bind`. The token stays mandatory,
   and the flag's own help text states what is forfeited: **every console byte, and
   the token itself, is readable and replayable by anyone on the network path.** Use
   it only on a network you genuinely trust; prefer `--tls` or SSH forwarding.

### What the token holder can do, stated plainly

**Graph editing from the browser is daemon-user capability, and the token is
operator trust.** Since §15.35 the web console edits the graph: the bridge's
allowlist admits `add-node`, `remove-node`, `connect`, `disconnect` and the passive
`ports` alongside the observation, tap, arbitration, rotation and serial-signal
verbs. That is a real widening and it deserves a plain statement rather than an
implication:

> A log node writes files, and an exec codec runs a command — both as the user
> `serialnexusd` runs as. Whoever holds the web token can therefore create a node
> that writes a file anywhere that user can write, and a node that executes an
> arbitrary command line as that user. Treat the token as equivalent to shell
> access for that account, and run the daemon under a dedicated, unprivileged user
> (see the checklist below) so "the daemon's user" is as small a blast radius as
> possible.

The earlier posture — the console never mutates the graph — was the right default
until the operator decided otherwise, and the argument that retired it is recorded
rather than erased: a token holder already commands every configured console on the
machine, so withholding graph edits protected little while costing real workflow.

What the console still **cannot** do is as load-bearing as what it can, and the
bridge enforces it with an **allowlist**, not a denylist: the browser may invoke
exactly `state`, `subscribe`, `info`, `ports`, `dump`, `tap.open`, `tap.close`,
`send`, `lock`, `unlock`, `rotate`, `send-break`, `set-modem`, `pulse-dtr`,
`add-node`, `remove-node`, `connect`, `disconnect` — and **everything else is
refused with `-32601`**, including a verb §10 grows tomorrow. That direction is the
load-bearing half: a denylist admits every future verb and keeps the boundary true
only for as long as someone remembers to extend it, which is exactly how a stated
boundary erodes. Widening the list, as §15.35 did, is a deliberate act with a
reason attached; an inverted list would have widened itself.

So **`load`, `teardown` and `shutdown` never reach the daemon** from a browser:
whole-graph replacement and daemon lifecycle are not graph editing, and a page that
can turn the daemon off serves no one.

Do not read that as a containment boundary. It is not one, and saying so would be
worse than saying nothing: `add-node` accepts a full node configuration, and an exec
codec's node configuration *is* a command line. A token holder who can add a node can
therefore run a command as the daemon's user, which subsumes stopping the daemon and
rewriting its configuration on disk. The lifecycle verbs stay off the wire because
they are not what the operator asked for, not because withholding them constrains an
attacker who already holds the token. **The token is the boundary.** Treat it as
shell access for the account `serialnexusd` runs as; the checklist below is how you
make that account small.

Two properties make that screen binding rather than decorative, both settled by a
reproduced bypass (review 26, WEB-1/SEC-1). **One frame is exactly one request:**
a frame that does not parse to a single JSON *object* — a batch array, a scalar, or
two newline-separated requests — is refused outright with `-32600`. And **the
bridge forwards the parsed value re-serialized, never the browser's raw text**, so
the request that reaches the daemon's newline-delimited socket is byte-for-byte the
one the screen approved, and no second request can ride behind an embedded newline.
A screen that decides on a different object than the one it transmits is not a
screen.

**Watching** still never writes to disk on the daemon's behalf: a tap is not a log
node, so viewing never becomes an unasked-for recording, and the browser's own OPFS
scrollback is the viewer's disk rather than the daemon's (see the replay-ring
section above). **Editing does**, and since §15.35 the console can edit: the editor
page's palette offers a `log` node with a directory and a filename, so a token
holder can start a recording — anywhere the daemon's user can write. That is the
same capability the block quote above states; it is repeated here because "the web
console does not record" is the kind of half-true an operator remembers and the
qualifier is not.

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
allowlist, and bounds its pre-auth path separately from its authenticated one — but
the browser replays its session cookie to every service on every port of the same
host, so that token is only as safe as the other local ports the operator visits.
The exec codec runs as the daemon's user,
so run that user small. There is no in-daemon crypto in v1 — that, and finer
per-caller authorization via `SO_PEERCRED`, are named as future work, not present
guarantees (§14, §10).
