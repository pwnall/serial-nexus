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
whoever can open the socket owns every console** (§10). Filesystem permissions on
that one path are the entire access-control surface.

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

## The web console: a bearer token, Host validation, and three bind tiers

`serialnexusweb` (§17) is a **separate process** — a pure client of the daemon's
control socket on one side, and an HTTP + WebSocket server for a browser on the
other. The daemon does not link it, serve it, or know it exists; the web server is
"simply a client that holds the socket, and whoever holds the token holds exactly
what the web server holds."

The delta that shapes its security is one sentence: **the control socket is mode
0600, but a loopback TCP port is reachable by every local user.** So the web
server cannot lean on file permissions the way the daemon does. Two mechanisms
replace them, and they solve *different layers* (§15.29):

- **The token answers *who may act*.** Every request and every WebSocket upgrade
  requires a per-session bearer token — 256 bits from the OS CSPRNG, generated at
  startup and printed as a ready-to-open URL (`http://127.0.0.1:PORT/?token=…`,
  Jupyter-style). Opening that URL sets the token as a `SameSite=Strict` session
  cookie and drops it from the address bar; every later request (assets and the WS
  upgrade alike) carries the cookie, which doubles as CSRF protection. No cookie,
  no access — a request without it gets `401`.
- **The channel answers *who can read and replay*.** A bearer token over plaintext
  HTTP is a secret broadcast to every on-path observer, who reads it once and holds
  console access — root shells, per above — indefinitely. That is exactly what TLS
  fixes, and why the token alone is not enough off loopback.

The Host header is validated on every request against the localhost family (plus any
`--host` names), so a page that rebinds DNS to `127.0.0.1` still fails — its Host is
its own, and it gets `403`.

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
mutates the graph: the server refuses `load`, `add-node`, `remove-node`, `teardown`,
`shutdown`, and the live-surgery verbs at the bridge, so even a compromised page
cannot reconfigure the daemon — it can only watch consoles (`tap`/`subscribe`/
`state`), send lines (`send`), and arbitrate the write lock (`lock`/`unlock`,
explicit steal only, never automatic). And it never writes to disk: watching a
console is a tap, not a log node, so viewing never becomes an unasked-for recording.

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
  distinction exists today: today it is all-or-nothing at the socket boundary.

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

The control socket's file permissions are the whole authorization model, and a
console is usually a root shell — so guard the socket like a root credential and
widen it only by a group you trust that far. Cross-machine legs stay on loopback
and ride SSH; reaching past loopback means writing `insecure_bind = true` on
purpose. The exec codec runs as the daemon's user, so run that user small. There is
no in-daemon crypto in v1 — that, and finer per-caller authorization via
`SO_PEERCRED`, are named as future work, not present guarantees (§14, §10).
