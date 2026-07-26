# Packaging & deployment

Files for running `serialnexusd` as a system service on Linux. serial_nexus is
lab-usable on Linux at `0.2.0`, pre-1.0; see [`../docs/macos.md`](../docs/macos.md)
for the best-effort macOS status and [`../docs/security.md`](../docs/security.md)
for the threat model you accept by exposing the control socket.

| File | Purpose |
|------|---------|
| `serialnexusd.service` | systemd unit (dedicated identity, state/runtime dirs, sandboxing) |
| `serialnexusd.example.toml` | first-boot configuration seed |
| `99-serial-nexus.rules` | optional udev rules for narrower device access |

## Install

```sh
# 1. Build and install the binaries (release build recommended).
cargo build --release
sudo install -m0755 target/release/serialnexusd  /usr/local/bin/
sudo install -m0755 target/release/serialnexusctl /usr/local/bin/

# 2. Seed configuration (edit for your device — capture its identity first, below).
sudo install -d -m0755 /etc/serialnexusd
sudo install -m0644 packaging/serialnexusd.example.toml /etc/serialnexusd/config.toml

# 3. (The default log directory /var/log/serialnexusd is created and chowned to the
#    service automatically by the unit's LogsDirectory= — no manual step needed.)

# 4. (Optional) narrower device access than the whole `dialout` group.
sudo install -m0644 packaging/99-serial-nexus.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger

# 5. Install and start the service.
sudo install -m0644 packaging/serialnexusd.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now serialnexusd
```

`RuntimeDirectory=`/`StateDirectory=` in the unit create `/run/serialnexusd` and
`/var/lib/serialnexusd` automatically under the service's transient identity — no
manual `useradd` needed.

## Capture your device's identity

The example config names the device by a resolver identity so it survives replug and
reboot (§12). Start with `ports`, which lists what is actually plugged in and the
identity that would bind each one — it opens nothing, so listing a port never toggles
DTR and never resets the board behind it:

```sh
serialnexusctl ports
# /dev/ttyUSB0   free   usb:0403:6001:A6008isP:00
#                       FTDI FT232R USB UART, serial A6008isP, interface 00
```

Paste that identity into `config.toml`, or bind it straight away with `add-node`,
which echoes back what it resolved so a wrong device answering is noticed:

```sh
printf '[[node]]\ntype="serial"\nname="usb0"\ndevice="usb:0403:6001:A6008isP:00"\n' > /tmp/n.toml
serialnexusctl add-node /tmp/n.toml
# -> added usb0 — bound: FTDI FT232R, serial A6008isP, interface 0
serialnexusctl ports                   # the port now reads: bound usb0
```

## Operating it

```sh
serialnexusctl state                 # observed status of every node
serialnexusctl --json state | jq .   # machine-readable (or speak JSON-RPC directly)
serialnexusctl ports                 # what is plugged in, and what already binds it
serialnexusctl send usb0 --line "…"  # atomic acquire-write-release to the device
serialnexusctl rotate cap            # rotate a log node on demand
serialnexusctl connect usb0 console  # wire an edge onto the running graph
serialnexusctl disconnect usb0 cap   # …and take one out, with no outage
sudo systemctl reload-or-restart serialnexusd   # note: no live reload; restart re-reads state
```

The control socket is `/run/serialnexusd/serialnexusd.sock`, mode `0600` — **whoever
can open it owns every console** (§10). To let a group of operators drive the daemon,
create a group, add it to the unit's `SupplementaryGroups=`, and pass
`--socket-group <group>` (widens the socket to `0660`). Read
[`../docs/security.md`](../docs/security.md) before doing that: serial consoles are
frequently root shells and bootloader prompts.

## Adjusting the sandbox

The unit is hardened as far as a daemon that needs raw character devices can go. Two
things you will likely edit:

- **`DeviceAllow=`** — the unit allows `char-ttyUSB`, `char-ttyACM`, and the pty
  subsystem. If your adapters enumerate elsewhere (a platform UART `/dev/ttyS*`,
  `/dev/ttyAMA*`, or a different major), add the matching `DeviceAllow=` line, or the
  daemon's serial node will come up `faulted` with a permission error in `state`.
- **Log directories** — the default `/var/log/serialnexusd` is provisioned by
  `LogsDirectory=` (created and chowned to the service each start). For a log node
  pointed *outside* that tree, add its `directory` to `ReadWritePaths=` **and**, under
  `DynamicUser`, pre-`chown` it to the service — `ReadWritePaths` only flips the mount
  to read-write, it does not chown, so a root-owned directory stays unwritable.
  Simplest is to keep extra logs under a subdirectory of `/var/log/serialnexusd`.

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

**New in this build (design §15.35).** Three additions, none of which breaks an
existing configuration:

- **`serialnexusctl ports`** lists the serial devices on the machine, the identity
  that would bind each one, and which node already holds it. It is strictly passive —
  by-id/by-path readlinks and sysfs, never `open(2)` — so listing a port cannot reset
  the board behind it.
- **`serialnexusctl connect` / `disconnect`** reshape a running graph one edge at a
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
`serialnexusctl load <file>` against a scratch daemon before rolling one out.

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
- **`serialnexusctl add-node` errors** on a file carrying more than one `[[node]]` or
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
