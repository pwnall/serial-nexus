# Packaging & deployment

Files for running `serial-nexus-daemon` as a system service on Linux. serial_nexus is
lab-usable on Linux at `0.3.0`, pre-1.0; see [`../docs/macos.md`](../docs/macos.md)
for the best-effort macOS status and [`../docs/security.md`](../docs/security.md)
for the threat model you accept by exposing the control socket.

| File | Purpose |
|------|---------|
| `serial-nexus-daemon.service` | systemd unit (dedicated identity, state/runtime dirs, sandboxing) |
| `serial-nexus-daemon.example.toml` | first-boot configuration seed |
| `99-serial-nexus.rules` | optional udev rules for narrower device access |

## Install

```sh
# 1. Build and install the binaries (release build recommended).
cargo build --release
sudo install -m0755 target/release/serial-nexus-daemon  /usr/local/bin/
sudo install -m0755 target/release/serial-nexus-ctl /usr/local/bin/

# 2. Seed configuration (edit for your device — capture its identity first, below).
sudo install -d -m0755 /etc/serial-nexus-daemon
sudo install -m0644 packaging/serial-nexus-daemon.example.toml /etc/serial-nexus-daemon/config.toml

# 3. (The default log directory /var/log/serial-nexus-daemon is created and chowned to the
#    service automatically by the unit's LogsDirectory= — no manual step needed.)

# 4. (Optional) narrower device access than the whole `dialout` group. The rules hand
#    matching adapters to a DEDICATED group, so the file alone does nothing: create the
#    group first, and name it in the unit instead of `dialout` (keeping `dialout` too
#    grants everything back and defeats the point). Read the rules file's own comments
#    for which adapters it matches — it ships an FTDI vendor id you will likely edit.
sudo groupadd --system serialnexus
sudo install -m0644 packaging/99-serial-nexus.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger
#    …then in serial-nexus-daemon.service: SupplementaryGroups=serialnexus
#    Half of this step is a silent failure either way: the rules without the group in
#    the unit leave the serial node `faulted` on a permission error, and the group in
#    the unit without the rules leaves nothing granting it anything.

# 5. Install and start the service.
sudo install -m0644 packaging/serial-nexus-daemon.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now serial-nexus-daemon
```

**Upgrading from a pre-rename release?** Step 5 is not the last thing you do: the state
snapshot has to be carried across by hand, and until it is, the daemon comes up on the
step-2 seed with every node you added by `add-node`/`connect` since its last `load`
missing. The procedure is in "Upgrading to this build" below.

`RuntimeDirectory=`/`StateDirectory=` in the unit create `/run/serial-nexus-daemon` and
`/var/lib/serial-nexus-daemon` automatically under the service's transient identity — no
manual `useradd` needed.

## Capture your device's identity

The example config names the device by a resolver identity so it survives replug and
reboot (§12). Start with `ports`, which lists what is actually plugged in and the
identity that would bind each one — it opens nothing, so listing a port never toggles
DTR and never resets the board behind it:

```sh
serial-nexus-ctl ports
# /dev/ttyUSB0   free   usb:0403:6001:A6008isP:00
#                       FTDI FT232R USB UART, serial A6008isP, interface 00
```

Paste that identity into `config.toml`, or bind it straight away with `add-node`,
which echoes back what it resolved so a wrong device answering is noticed:

```sh
printf '[[node]]\ntype="serial"\nname="usb0"\ndevice="usb:0403:6001:A6008isP:00"\n' > /tmp/n.toml
serial-nexus-ctl add-node /tmp/n.toml
# -> added usb0 — bound: FTDI FT232R, serial A6008isP, interface 0
serial-nexus-ctl ports                   # the port now reads: bound usb0
```

## Operating it

```sh
serial-nexus-ctl state                 # observed status of every node
serial-nexus-ctl --json state | jq .   # machine-readable (or speak JSON-RPC directly)
serial-nexus-ctl ports                 # what is plugged in, and what already binds it
serial-nexus-ctl send usb0 --line "…"  # atomic acquire-write-release to the device
serial-nexus-ctl rotate cap            # rotate a log node on demand
serial-nexus-ctl connect usb0 console  # wire an edge onto the running graph
serial-nexus-ctl disconnect usb0 cap   # …and take one out, with no outage
sudo systemctl reload-or-restart serial-nexus-daemon   # note: no live reload; restart re-reads state
```

The control socket is `/run/serial-nexus-daemon/serial-nexus-daemon.sock`, mode `0600` — **whoever
can open it owns every console** (§10). Letting a group of operators drive the daemon
means giving that group the runtime **directory** as well as the socket, and the
directory is not a mode change: systemd chowns every `*Directory=` to `User=`/`Group=`
and to nothing else (`systemd.exec(5)`), so `SupplementaryGroups=` cannot reach it.
Under `DynamicUser=` the directory belongs to the transient identity, the operators
land in `other`, and they get `EACCES` at `connect(2)` before the socket's own mode is
ever consulted. The working recipe is a **static** service identity whose primary group
is the operators' group — it is spelled out in the unit's socket-group comment block,
with the `stat` that confirms it. Read
[`../docs/security.md`](../docs/security.md) before doing any of it: serial consoles are
frequently root shells and bootloader prompts.

## Adjusting the sandbox

The unit is hardened as far as a daemon that needs raw character devices can go. Two
things you will likely edit:

- **`DeviceAllow=`** — the unit allows `char-ttyUSB`, `char-ttyACM`, and the pty
  subsystem. If your adapters enumerate elsewhere (a platform UART `/dev/ttyS*`,
  `/dev/ttyAMA*`, or a different major), add the matching `DeviceAllow=` line, or the
  daemon's serial node will come up `faulted` with a permission error in `state`.
- **Log directories** — the default `/var/log/serial-nexus-daemon` is provisioned by
  `LogsDirectory=` (created and chowned to the service each start). For a log node
  pointed *outside* that tree, add its `directory` to `ReadWritePaths=` **and**, under
  `DynamicUser`, pre-`chown` it to the service — `ReadWritePaths` only flips the mount
  to read-write, it does not chown, so a root-owned directory stays unwritable.
  Simplest is to keep extra logs under a subdirectory of `/var/log/serial-nexus-daemon`.

- **`RestrictAddressFamilies=`** — drop `AF_INET AF_INET6` if you configure no leg
  nodes (legs are loopback-only, carried over SSH; §7.4/§9).

## Cross-machine legs over SSH

Legs bind loopback-only by default (§9). To join two daemons, forward the listening
leg's loopback port over SSH rather than binding a public address:

```sh
# On the operator's machine, forward computer A's leg to a local loopback port:
ssh -L 127.0.0.1:7420:127.0.0.1:7420 labmachine
# then point computer B's `connect` leg at 127.0.0.1:7420.
```

`insecure_bind = true` exists for a non-loopback bind, but it is a deliberately ugly,
greppable footgun — prefer SSH forwarding.

## Upgrading to this build — what changed for operators

There is no separate changelog, so the list lives here, where someone installing the
unit will read it.

**This is `0.3.0`, and the minor bump is the rename.** A patch would have understated it:
design §15.40 changes the crate names an out-of-tree consumer builds against, which is a
breaking change to the §15.26 extension surface — taken deliberately at 0.x, where the cost
is one `Cargo.toml` edit plus an import rename, rather than after that surface has pins on it.

**Everything user-visible was renamed (design §15.40).** Every binary now carries the
family prefix: the daemon `serialnexusd` is `serial-nexus-daemon`, the CLI
`serialnexusctl` is `serial-nexus-ctl`, and the web console, capability checker and
test double follow the same scheme. Reinstall the unit file and re-create the
configuration directory under the new names shown in the install steps above — the old
`/etc` and `/run` directories are not read. The old `/var/lib` one is, and it is the one
that matters — the rest of this section is about getting its contents across.

**Two defaults are still accepted on read, for this release only.** Both are
daemon-owned paths nobody looks at, which is exactly why a rename is dangerous there:

- a state snapshot at a pre-rename path is **adopted** at startup, and the next
  configuration mutation rewrites it under the current name. The old file is left
  exactly as it was — remove it once the new one appears. Two shapes are recognised:
  the socket-adjacent default (`serialnexusd.state.toml`), and an explicit
  `--state-file` whose path is the current one with the rename undone — which is what
  the unit passes, `/var/lib/<old daemon name>/state.toml` becoming
  `/var/lib/serial-nexus-daemon/state.toml`. Without this, an upgraded daemon would have
  found no snapshot at the new path and come up with an **empty graph**, silently
  dropping every node added by `add-node`/`connect` since the last `load`.
- `serial-nexus-ctl` and `serial-nexus-web`, given no `--socket`, fall back to a socket
  left under the pre-rename name by a still-running pre-upgrade daemon — but only when
  no current-name socket exists, so a live daemon is never passed over for a stale
  inode.

**The packaged unit needs one manual step anyway, and it is not optional.** Adoption
knows where the old snapshot is; under `DynamicUser=` it cannot read it. systemd keeps
the real state directory under `/var/lib/private/`, which is root-only, so the file has
to be carried across by root. Do this once, between installing the new unit and putting
it into service:

```sh
# 1. Let systemd create the new state directory (this start comes up on the seed).
sudo systemctl start serial-nexus-daemon && sudo systemctl stop serial-nexus-daemon

# 2. Copy the pre-rename snapshot over the seeded one. `cp` onto an existing file keeps
#    that file's owner and its 0600 mode, and the next start re-chowns the directory
#    anyway.
sudo cp /var/lib/serialnexusd/state.toml /var/lib/serial-nexus-daemon/state.toml

# 3. Start it for real, and confirm the graph came across before deleting anything.
sudo systemctl start serial-nexus-daemon
sudo serial-nexus-ctl state
```

A deployment that does *not* use `DynamicUser=` — a static `User=`, a container, a
hand-run daemon — needs none of this: the daemon reads the pre-rename path itself.

Nothing writes the old spelling again, and both fallbacks are deleted in the next
release: finish the migration now rather than relying on them.

**New in this build (design §15.35).** Three additions, none of which breaks an
existing configuration:

- **`serial-nexus-ctl ports`** lists the serial devices on the machine, the identity
  that would bind each one, and which node already holds it. It is strictly passive —
  by-id/by-path readlinks and sysfs, never `open(2)` — so listing a port cannot reset
  the board behind it.
- **`serial-nexus-ctl connect` / `disconnect`** reshape a running graph one edge at a
  time, under the same structural validation `load` performs. Rewiring no longer needs
  a `load --replace` outage; disconnecting a writer that holds the write lock releases
  it and purges its un-flushed bytes rather than leaving the endpoint wedged.
- **The web console can now edit the graph** — a graph page and an editor page join
  the console view. *This is a security-posture change and it is deliberate:* whoever
  holds the web token can now create a log node (which writes files) and an exec codec
  (which runs a command), both as the daemon's user. Treat the token as equivalent to
  shell access for that account. Daemon lifecycle stays off the browser wire: `load`,
  `teardown` and `shutdown` are still refused there. See `docs/security.md`.

**From the `docs/historical/26-claude-opus-code-review.md` remediation**, which tightened
configuration validation and changed a few operator-visible behaviours.

**Configurations that used to load and now do not.** Each is refused *structurally*,
before anything is created — so under `load --replace` a bad file can no longer
destroy the running graph, which is the point of the change (§11). Run
`serial-nexus-ctl load <file>` against a scratch daemon before rolling one out.

- **Unknown keys and unknown tables are rejected.** `advertized_baud = 9600` used to
  be accepted and silently ignored; a misspelled `[[nodez]]` table used to parse to an
  empty graph, which `--replace` then applied as an unannounced teardown reported as
  success. Both now name the offender.
- **Numeric fields are range-checked**: `replay_ring` ≤ 16 MiB, `hostward_buffer`
  1..=65536, leg timers ≤ 1 h, log `rotation_padding` ≤ 20, `baud` ≥ 1. A 64 MiB
  scrollback or a six-hour reconnect backoff is no longer configurable. (Unbounded
  values were not merely unwise: a large `replay_ring` aborted the daemon on the next
  byte from the device and then crash-looped from the persisted configuration.)
- **An edge into a codec's or exec codec's multiplexed endpoint must declare
  `write_mode = "held"`** (or `"never"` for a read-only demux). Omitting it used to
  load and then swallow every targetward byte while `send` reported success.
- **At most one effectively-`held` edge per host-facing endpoint**, unless that
  endpoint's node sets `arbitration = "free-for-all"`.
- **A `serial` node with `faces = "target"` is refused** as unimplemented (§14). It
  used to load, open the device, take `TIOCEXCL`, and be wired to nothing.
- **Node names and channel identities** may not be whitespace-only and are capped at
  256 bytes.

**Behaviour changes worth knowing about.**

- **`spchex` maps different bytes.** It now matches picocom's `M_SPCHEX` — DEL plus
  every control byte below 0x20 except TAB/LF/CR — where it previously matched SPACE.
  A configuration using `spchex` produces different output; the old behaviour is
  approximately `nrmhex` restricted to SPACE, and was never what the design specified.
- **`serial-nexus-ctl add-node` errors** on a file carrying more than one `[[node]]` or
  any `[[edge]]`, instead of silently adding the first node and discarding the rest.
- **`--json` prints errors as JSON** (`{"error": {...}}` on stdout, non-zero exit)
  rather than only as human text on stderr.
- **`lock --steal` by the origin that already holds the lock** is now a no-op that
  reports `acquired: false`, instead of a fresh grant that purged the holder's own
  in-flight bytes and voided its lease.
- **New file modes**: the state file is created `0600` and log files `0640`. `mode`
  applies at *creation*, so an existing world-readable log keeps its permissions until
  it next rotates — if a log shipper runs as another user, fix its group access before
  the next `rotate` rather than after.
- **`rotate` is ordered against the write queue**, so everything accepted before the
  request lands in the old file and everything after in the new one. The cost is
  latency: rotating a node with a large backlog now waits for that backlog to drain.
- **A structurally invalid *state file* no longer prevents startup.** The daemon logs
  an error naming the file, preserves it untouched, and comes up with an empty graph.
  An invalid `--config` file given explicitly on the command line still fails fast.
