# serial_nexus

**serial_nexus** is a permissively-licensed (`MIT OR Apache-2.0`) daemon
(`serialnexusd`) and control CLI (`serialnexusctl`) that manages serial ports as
an explicit, inspectable **graph** of data-routing nodes under a single,
operator-owned configuration. Embedded serial work looks trivial — open
`/dev/ttyUSB0`, run a terminal — until the realities collide: one UART carries
several logical streams (console, coprocessor, trace) multiplexed by a device
protocol; every stream has several simultaneous consumers (a terminal, a
forensic log, a forwarded copy) that must not interfere; streams have to cross
machines; concurrent writers interleave bytes and corrupt line- or
packet-oriented protocols, so writing needs an exclusive lock with a steal
escape hatch; and USB adapters come and go under changing `/dev` paths, so
operator intent must survive replug, restart, and power-cycle. serial_nexus
composes all of that as one directed acyclic graph: a **serial** node owns the
port, a **codec** node demultiplexes it into named channels, and each channel
fans out to a **pty** node (interactive), a **log** node (append-only, on-demand
rotation), and a **leg** node (re-multiplexes every channel over one socket to a
peer daemon that fans out again).

`ser2net`, `socat`, and `conserver` each solve a slice of this — TCP exposure,
ad-hoc pipelines, shared console access and logging — and all three are
copyleft. None of them compose demultiplexing, PTY fan-out, per-stream logging,
re-multiplexing, and cross-machine forwarding under one configuration. The
stable contract is a JSON-RPC method set over a Unix socket (design §10):
operators and AI agents drive `serialnexusctl` or speak JSON-RPC directly, and
the whole surface is debuggable with `socat` and `jq`.

> **Maturity:** 0.2.0, pre-1.0. Lab-usable on Linux. macOS is best-effort;
> Windows is out of scope. Interfaces may still shift before 1.0.

## Architecture

The daemon holds one graph. Nodes expose typed *endpoints*; an *edge* joins one
host-facing endpoint to one target-facing endpoint and carries a bidirectional
byte stream. Fan-out is implicit: a host-facing endpoint broadcasts to every
attached edge, and one exclusive write lock arbitrates who may write back.

| Node | Role |
|------|------|
| **serial** | Owns a physical port; holds it open with `TIOCEXCL`, reads continuously, and reconnects to the *same* device identity after an unplug or power-cycle (§7.1). |
| **pty** | Allocates a pseudo-terminal and a stable symlink for an interactive operator or a driving script; a `never` write mode makes it a read-only spy terminal (§7.2). |
| **log** | Appends raw bytes to a file with on-demand rotation and a bounded writer queue; always read-only toward the device (§7.3). |
| **codec** | Interior protocol transform: splits one multiplexed stream into N named channels (or re-multiplexes them), framing knowledge staying inside the node (§7.5). |
| **exec-codec** | The escape hatch: a `codec` node with `codec = "exec"` that runs an external child process speaking a documented envelope protocol on stdin/stdout, so protocol tools under any license can be wrapped without linking (§7.6). |
| **leg** | Cross-daemon transport: carries every channel multiplexed over one TCP or Unix socket to a peer daemon, loopback-only unless explicitly opted out (§7.4). |
| **existing-terminal** | *(design-specified, §7.7; not yet implemented in 0.2.0)* connects to a pre-existing PTY or tty by path — a QEMU console, a simulator, a mock device. |

## Five-minute quickstart

Everything below runs against a fake, echoing serial device — no hardware
required. The daemon's control socket is a Unix socket, whose path is bounded to
roughly 108 bytes (`SUN_LEN`), so keep `XDG_RUNTIME_DIR` short.

```sh
# 1. Build the workspace (daemon, CLI, the nexus-sim test double, nexus-doctor).
cargo build

# 2. A short runtime dir: the control socket and the state file live here.
export XDG_RUNTIME_DIR=$(mktemp -d /tmp/snx.XXXX)
BIN=$PWD/target/debug

# 3. Stand a fake device where /dev/ttyUSB0 would be — an echoing PTY behind a
#    stable symlink.
"$BIN/nexus-sim" pty --echo --link "$XDG_RUNTIME_DIR/device" &

# 4. Start the daemon. It creates $XDG_RUNTIME_DIR/serialnexusd.sock (mode 0600 —
#    whoever can open the socket owns every console).
"$BIN/serialnexusd" &

# 5. Describe the graph: a serial node on the fake device, a pty node for an
#    operator console, and one edge wiring them together.
cat > "$XDG_RUNTIME_DIR/demo.toml" <<EOF
[[node]]
type = "serial"
name = "usb0"
device = "$XDG_RUNTIME_DIR/device"
arbitration = "free-for-all"

[[node]]
type = "pty"
name = "console"
path = "$XDG_RUNTIME_DIR/console"

[[edge]]
a = "usb0"
b = "console"
EOF

# 6. Load it.
"$BIN/serialnexusctl" load "$XDG_RUNTIME_DIR/demo.toml"
#   -> loaded 2 node(s)

"$BIN/serialnexusctl" state
#   -> usb0     active
#   -> console  active

# 7. Send a line targetward through the endpoint; the fake device echoes it back,
#    the serial node reads it hostward, and it fans out to the console PTY.
"$BIN/serialnexusctl" send usb0 --line "hello"
#   -> usb0: sent 6 byte(s)

# 8. Tear it all down.
"$BIN/serialnexusctl" shutdown
```

To watch the echo arrive on the console instead of trusting the send count,
attach any terminal to the pty before step 7 — for example `screen
"$XDG_RUNTIME_DIR/console"`, or, without a terminal, drive it with the test
double:

```sh
"$BIN/nexus-sim" client --path "$XDG_RUNTIME_DIR/console" --drain &
"$BIN/serialnexusctl" send usb0 --line "hello"   # the client reports 6 bytes received
```

**Why `arbitration = "free-for-all"`?** Writing is arbitrated per endpoint, and
the default is `exclusive`: a terminal attached to `console` would have to grab
the write lock (`serialnexusctl lock console`) before its keystrokes reach the
device, and release it (`serialnexusctl unlock console`) when done. `free-for-all`
opts that endpoint out of the lock entirely, which is what you want for a single
console with no contention. (The `send` verb self-acquires the lock either way,
which is why step 7 works under both policies.)

## Building and testing

```sh
cargo build              # all workspace crates
cargo test               # unit and property tests
scripts/validate/all.sh  # end-to-end phase validation (add --through N to stop early)
```

`nexus-doctor` is the shipping capability report: it probes every kernel
behavior the design depends on (PTY packet mode, serial ioctls, `by-id`
resolution) and emits a Markdown report — **attach its output to any bug
report.** It never touches a real serial port unless you name one with `--port`.

```sh
./target/debug/nexus-doctor            # Markdown report on stdout
./target/debug/nexus-doctor --json     # JSON twin for CI
```

See [`docs/nexus-doctor.md`](docs/nexus-doctor.md) for the probe reference.

## Platform support

Linux is **required** and is the kernel of record. macOS is **best-effort** —
see [`docs/macos.md`](docs/macos.md) for what works and what degrades. Windows
is **out of scope**.

## Security

Serial consoles are frequently root shells and bootloader prompts, so treat the
control socket as a grant of full device access. The default socket mode is
`0600` (owner only); widening it to a group hands every attached console to that
group. See [`docs/security.md`](docs/security.md) for the threat model and
hardening guidance.

## Documentation

- [`docs/20-design-claude-fable-v9.md`](docs/20-design-claude-fable-v9.md) — the normative design document (concepts, node types, RPC contract, and the reasoning behind each decision).
- [`docs/21-implementation-plan-claude-fable-v9.md`](docs/21-implementation-plan-claude-fable-v9.md) — the normative implementation plan (phases and post-1.0 tracks).
- [`docs/rpc/`](docs/rpc/) — the JSON-RPC method reference (the stable contract of §10).
- [`docs/codec-authors.md`](docs/codec-authors.md) — writing a codec: the trait, the event vocabulary, and the envelope protocol for external (any-language) codecs.
- [`docs/security.md`](docs/security.md) — threat model and hardening.
- [`docs/macos.md`](docs/macos.md) — macOS best-effort notes.
- [`docs/nexus-doctor.md`](docs/nexus-doctor.md) — the capability checker.
- [`packaging/`](packaging/) — distribution packaging.

## License

Licensed under either of **MIT** or **Apache-2.0** at your option
(`SPDX: MIT OR Apache-2.0`).
