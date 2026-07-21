# Packaging & deployment

Files for running `serialnexusd` as a system service on Linux. serial_nexus is
lab-usable on Linux at `0.1.0`; see [`../docs/macos.md`](../docs/macos.md) for the
best-effort macOS status and [`../docs/security.md`](../docs/security.md) for the
threat model you accept by exposing the control socket.

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
reboot (§12). Capture yours with `add-node`, which echoes the resolved identity:

```sh
# With the adapter plugged in, add a serial node by raw path and read back its
# canonical identity, then paste that identity into config.toml.
printf '[[node]]\ntype="serial"\nname="usb0"\ndevice="/dev/ttyUSB0"\n' > /tmp/n.toml
serialnexusctl add-node /tmp/n.toml
# -> added usb0 — bound: FTDI FT232R, serial A6008isP, interface 0
serialnexusctl dump | grep device      # the captured usb:… identity
```

## Operating it

```sh
serialnexusctl state                 # observed status of every node
serialnexusctl --json state | jq .   # machine-readable (or speak JSON-RPC directly)
serialnexusctl send usb0 --line "…"  # atomic acquire-write-release to the device
serialnexusctl rotate cap            # rotate a log node on demand
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
