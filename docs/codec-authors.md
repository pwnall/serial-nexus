# Writing a codec for serial_nexus

This guide is for someone about to teach serial_nexus a new device protocol —
most often a **demultiplexer** that splits one framed device stream into named
channels. It documents the two ways to add a codec, and then documents, byte for
byte, the **envelope protocol** an external (exec) codec speaks on stdin and
stdout. That protocol is the whole reason this guide exists: get it right and any
program in any language becomes a codec.

Everything below is grounded in the code. The envelope format is defined in
[`codec-api/src/lib.rs`](../codec-api/src/lib.rs); the exec host is
[`serialnexusd/src/nodes/exec.rs`](../serialnexusd/src/nodes/exec.rs); the
compiled-in registry is
[`serialnexusd/src/nodes/codec.rs`](../serialnexusd/src/nodes/codec.rs); the
reference codec is [`codecs/reference/src/lib.rs`](../codecs/reference/src/lib.rs).
The canonical working exec codecs live under
[`tests/ext-codec/`](../tests/ext-codec/).

## 1. What a codec is

A **codec is a multi-channel framing transform** (design §8). It converts between
**one multiplexed byte stream** and **N channels**, in both directions, by
emitting and consuming per-channel events drawn from a small vocabulary:

| Event   | Meaning                                            |
| ------- | -------------------------------------------------- |
| `data`  | opaque channel bytes                               |
| `open`  | the channel became active / was announced          |
| `close` | the channel closed                                 |
| `error` | a channel-scoped error, with a human-readable reason |

That is the entire vocabulary — four events (`EventKind` in `codec-api`). v1 is
exactly these four; the protocol is evolvable to more per-channel control events
later, but a codec today produces and consumes only these.

Two properties matter and shape everything else:

- **Edges carry raw bytes; framing knowledge is internal to the codec.** The
  graph never sees your frames. On one side the codec faces a single multiplexed
  stream (the device); on the other it faces N channel endpoints (consumers).
  The design's interior contract (§5) holds: a codec may retain *at most one
  partial frame*, bounded by the frame size, and nothing else — no queues.
- **One implementation serves both orientations.** A `faces = target`
  demultiplexer splits the device stream into channels hostward and re-frames
  channel writes back into the device stream targetward. `faces = host` is the
  mirror, driven by a leg; a standalone re-multiplexer loads and waits for a
  driver today (§7.5, §14).

If you are writing a demux for your device, you are writing the thing that turns
a device read into `data("console", …)`, `data("trace", …)`, … hostward, and
turns a write on `"console"` back into device bytes targetward.

## 2. Two ways to add a codec

### (a) A compiled-in Rust codec

Write a small crate against the `codec-api` crate and implement the `Codec`
trait:

```rust
pub trait Codec {
    fn name(&self) -> &str;
    fn demux(&mut self, input: &[u8], emit: &mut dyn FnMut(Event)) -> Result<(), CodecError>;
    fn mux(&mut self, event: &Event, out: &mut Vec<u8>) -> Result<(), CodecError>;
    fn resync_count(&self) -> u64 { 0 }   // framing errors / resyncs, surfaced as state (§7.5)
}
```

`demux` consumes multiplexed bytes and calls `emit` once per decoded per-channel
event (it may hold a partial frame across calls). `mux` encodes one event into
multiplexed bytes. Your resync policy is your own: the reference codec resyncs by
length-guidance and counts one framing error; a codec on a reliable transport can
treat any violation as fatal and never resync (§15.23).

Register it in **one explicit `match`-on-name** — there is no linker-magic
auto-registration — in `serialnexusd/src/nodes/codec.rs`, behind a Cargo feature
so minimal builds can drop it. The `reference` arm is the example to copy:

```rust
pub fn build_codec(name: &str, attributes: &toml::Table) -> Result<Box<dyn Codec>, String> {
    match name {
        #[cfg(feature = "codec-reference")]
        "reference" => { /* validate attributes, construct */ }
        other => Err(format!("unknown codec {other:?}")),
    }
}
```

Attributes arrive as an opaque `toml::Table` that your codec deserializes via
serde into its own type and validates itself; a schema failure is **structural**
and aborts the load with nothing created (§8, §11). The reference codec takes no
attributes. Net cost of a new built-in codec: a crate, one registry line, and a
feature flag — accepted as a recompile per codec (§15.11).

### (b) The exec codec — the escape hatch

The exec codec is an ordinary compiled-in codec (`codec = "exec"`) whose
transform is a **separate child process** speaking the envelope on stdin/stdout.
Because the child is a separate process talking a documented protocol — **no
linking** — it can be written in **any language** and licensed under **any
license, including copyleft**. This is how you wrap an existing protocol tool:
run ser2net, a vendor CLI, a GPL framing utility, or a fifty-line Python script
beside the daemon, unmodified.

The rest of this guide is about the envelope, because the envelope *is* the exec
codec's contract.

## 3. The envelope frame format (the stdin/stdout contract)

Everything the child reads and writes is a stream of **length-prefixed frames**.
All integers are **big-endian**.

```
 u32  body_len          length of everything after this field
 ---- body ----
 u8   type              0 = data, 1 = open, 2 = close, 3 = error
 u16  channel_id_len    length of the UTF-8 channel identity
 ...  channel_id        channel_id_len bytes, UTF-8, never contains "/"
 ...  payload           data: the bytes | error: UTF-8 reason | open/close: empty
```

Constants, from `codec-api`:

- `MAX_FRAME_SIZE = 65536` (`64 * 1024`). A frame whose `body_len` exceeds this is
  **refused** — the decoder rejects it before buffering the body, and a
  conforming encoder never emits one.
- `ENVELOPE_VERSION = 1`. Bumping it is a deliberate, breaking change to this
  contract.

Payload rules by type:

- `data` — the payload is the opaque channel bytes.
- `error` — the payload is the UTF-8 reason string.
- `open` / `close` — the payload is **empty**.

Channel identities are UTF-8 and never contain `/` (§15.12), which keeps the
`node/channel` display form unambiguous.

### The decode contract

A correct reader distinguishes three outcomes on the bytes it has so far. This is
`try_decode` in `codec-api`, and your child must implement the same discipline:

- **Whole frame** → `Ok(Some((event, consumed)))`. Consume `consumed` bytes and
  continue.
- **Need more** → `Ok(None)`. A partial frame; read more bytes and retry. Never
  an error — any strict prefix of a valid frame yields "need more".
- **Malformed / oversize** → `Err(_)`. Refuse cleanly (§9 clause 6). The daemon
  treats a malformed frame arriving from the child's stdout as equivalent to a
  crash: it faults the node and restarts it.

Error cases the format defines: `body_len > MAX_FRAME_SIZE`, an unknown type
byte, a body truncated below its own declared header/channel length, a
channel identity that is not valid UTF-8, and (for `error`) a reason that is not
valid UTF-8.

### The frozen golden vectors

These four byte strings are **frozen** (§15.15). They are asserted in
`codec-api`'s `golden_vectors` test; a drift is a breaking envelope change and
must be deliberate. Use them as conformance fixtures for your implementation — if
your encoder reproduces these exactly and your decoder round-trips them, your
framing is correct.

| Event                     | Bytes (hex)                          |
| ------------------------- | ------------------------------------ |
| `data("console", "hi")`   | `0000000c000007636f6e736f6c656869`   |
| `open("trace")`           | `000000080100057472616365`           |
| `close("trace")`          | `000000080200057472616365`           |
| `error("c0", "boom")`     | `000000090300026330626f6f6d`         |

Reading the first one field by field:

```
0000000c  body_len = 12
      00  type     = 0 (data)
    0007  channel_id_len = 7
636f6e736f6c65  "console"
    6869  "hi"                       body = 1 + 2 + 7 + 2 = 12  ✓
```

## 4. The multiplexed side and the child-stdio boundary

### The reserved empty channel

The multiplexed (raw device-side) stream travels on the **reserved empty channel
identity** — the empty string `""` (§15.22). Configuration validation
independently forbids the empty string as a real channel identity (it would
collide with a node's default endpoint), so the reservation can never clash with
one of your channels.

Concretely, for a `faces = target` demux:

- **Hostward** (device → consumers): the daemon reads raw device bytes and frames
  them as `data("", …)` into the child's **stdin**. The child parses the device's
  proprietary framing and emits `data("<channel>", …)` on **stdout**; the daemon
  fans each channel out to its consumers.
- **Targetward** (consumer → device): the daemon frames a channel write as
  `data("<channel>", …)` into the child's **stdin**. The child re-frames it and
  emits `data("", …)` on **stdout**; the daemon writes those bytes to the device.

A `data` frame on the empty channel (`channel_id_len = 0`, no channel bytes)
therefore carries the raw device stream. Worked example — `data("", "AB")`,
derived from the format above (not one of the four frozen vectors):

```
00000005  body_len = 5
      00  type = 0 (data)
    0000  channel_id_len = 0   (the reserved multiplexed side)
    4142  "AB"                 body = 1 + 2 + 0 + 2 = 5
```

→ `000000050000004142`.

**Treat every channel as a byte stream, including the empty one.** A single
device read can exceed one frame (the daemon reads up to `MAX_FRAME_SIZE` at
once), so the daemon **fragments** an oversize chunk across consecutive `data`
frames rather than dropping it (§9 clause 4, §15.24) — the same discipline the
leg uses. Your child must reassemble per channel and must never assume one frame
equals one logical protocol unit. The same applies to large targetward channel
writes.

`open` / `close` / `error` on a real channel drive that channel's observable
state: `open` (or any `data`) marks it active, `close` marks it inactive, `error`
is logged. Emit events only on channel identities the node is configured with;
data on an unconfigured identity is dropped as mux noise, and data on a configured
channel with no consumer bound is discarded with a counter (`discarded_unattached`,
a located §5 loss) — neither is an error, but neither reaches a consumer.

### stdin and stdout are boundaries, not interior plumbing

A child's stdin and stdout are kernel pipes with finite buffers and an
independent consumer — §3's boundary test, verbatim (§15.22). The daemon
therefore **pumps both directions concurrently**: the stdin-feeding and
stdout-reading loops run as concurrently-polled futures, so a `write` blocked on a
full stdin pipe can never starve the stdout reader (which keeps draining,
unblocking the child, which drains stdin), and a targetward emit parked on
backpressure or a stolen write lock never starves the hostward feed. Coupling the
two directions in one loop was the phase-5 audit's mutual-deadlock bug; the
concurrent pump is now a review tripwire with a 256 KiB round-trip regression
guard.

Your child should honor the same rule: **a blocked write must never starve the
read.** A strict one-frame-in / one-frame-out codec that flushes each frame — like
the reference codecs below — is safe against this by construction, because the
daemon always drains stdout concurrently. But a codec that *buffers* input or
*amplifies* it (emits more than it consumes, or waits for several input frames
before producing output) must pump its two directions concurrently itself
(threads, async, or a `select`), especially when it wraps a tool that does its own
buffering. Otherwise it can fill its own stdout pipe while refusing to read stdin.

Two more boundary facts:

- **stderr passes through to daemon diagnostics.** Anything the child writes to
  stderr is logged line-by-line under the `exec-codec` target — the place to put
  debug output.
- **A crashed child faults the node and restarts with backoff.** The daemon
  distinguishes teardown from a crash: on a crash (stdin broke, stdout hit
  EOF/error, or the child emitted a malformed frame) it kills the child, marks the
  node `faulted`, waits `restart_backoff_ms`, and respawns. The restart count is
  observable in node state. The child runs as the daemon's user.

## 5. A minimal exec codec, and how to wire it

### The reference exec codecs

Two working, dependency-free (Python stdlib only) exec codecs ship as references:

- [`tests/ext-codec/passthrough.py`](../tests/ext-codec/passthrough.py) — the
  canonical envelope example. It reads each frame, parses it, and re-emits it
  **identically**. Parsing-then-re-encoding (rather than a raw byte copy) is
  deliberate: it proves the child actually implements the shared frame format in
  another language. This is the program the conformance battery drives (§6).
- [`tests/ext-codec/passthrough-codec.py`](../tests/ext-codec/passthrough-codec.py)
  — the same framing, in the **shape of a real demux**. It swaps the reserved
  empty (multiplexed) channel with one real channel in both directions, so a
  device byte stream appears on the channel and channel writes appear on the
  device. This is the skeleton to start from for your own demux.

The heart of `passthrough-codec.py` is the read/decode/route/encode loop — read a
frame with a length-prefixed `read_exact`, decode the header, decide the output
channel, re-encode:

```python
MUX  = ""                                   # the reserved multiplexed side
CHAN = sys.argv[1] if len(sys.argv) > 1 else "c0"

# ... read u32 body_len, then body; then:
type_byte = body[0]
(chan_len,) = struct.unpack(">H", body[1:3])
channel = body[3:3 + chan_len].decode("utf-8", "replace")
payload = body[3 + chan_len:]

if   channel == MUX:  out_channel = CHAN    # device bytes -> the channel (hostward)
elif channel == CHAN: out_channel = MUX     # channel write -> the device (targetward)
else:                 out_channel = channel

out.write(encode(out_channel, type_byte, payload))
out.flush()
```

For your device, replace that trivial channel swap with real work: on `data("",
raw_device_bytes)` run your framer, emit one `data("<channel>", payload)` per
decoded unit; on `data("<channel>", bytes)` re-frame into device bytes and emit
`data("", framed)`. Reassemble the incoming byte stream per channel (§4), flush
after writing, and if you buffer or amplify, pump both directions concurrently
(§4).

### The TOML to wire it

An exec codec is a codec node with `codec = "exec"`; its `argv`, `env`, and
restart backoff live in the opaque `attributes` table, which the exec codec
validates (`argv` is required and non-empty). The node's `faces` orients the
multiplexed side (`target` for a demux; the default is `target`), and `channels`
lists the channel identities — each becomes a host-facing channel endpoint. The
following config loads and runs (verified against the daemon):

```toml
# The device on the multiplexed side.
[[node]]
type = "serial"
name = "modem"
device = "/dev/ttyUSB0"       # or a resolved usb:/by-path: identity (§12)
baud = 115200

# The exec demux: raw multiplexed stream in, two channels out.
[[node]]
type = "codec"
name = "mux"
codec = "exec"
faces = "target"              # demultiplexer; channels face host
channels = ["console", "trace"]

  [node.attributes]
  argv = ["python3", "tests/ext-codec/passthrough-codec.py", "console"]
  restart_backoff_ms = 250    # optional; default 200

  [node.attributes.env]       # optional extra environment for the child
  MYTOOL_MODE = "framed"

# Consumers for each channel.
[[node]]
type = "pty"
name = "console-pty"
path = "/run/serial_nexus/console"

[[node]]
type = "pty"
name = "trace-pty"
path = "/run/serial_nexus/trace"

# The multiplexed side (the codec's default endpoint, addressed by node name)
# joins the serial's host-facing endpoint, and holds its write lock (§6): any
# other writer would corrupt the mux framing.
[[edge]]
a = "modem"
b = "mux"
write_mode = "held"

# Each channel endpoint (node/channel) joins a consumer.
[[edge]]
a = "mux/console"
b = "console-pty"
write_mode = "on-demand"

[[edge]]
a = "mux/trace"
b = "trace-pty"
write_mode = "never"          # a read-only spy channel
```

Load it with the daemon's `--config` at startup, or `serialnexusctl load`. Watch
it with `serialnexusctl state` (or `--json` for the codec's per-channel
`delivered_hostward` / `discarded_unattached` counters and the exec node's
`restart_count`).

## 6. Testing your codec through a pipe

Because the child is a plain program on stdin/stdout, you can test it without the
daemon. `nexus-sim` ships a conformance battery that drives an exec child through
the golden vectors and checks it re-emits them intact:

```console
$ ./target/debug/nexus-sim envelope --exec "python3 tests/ext-codec/passthrough.py"
{"mode":"envelope","pass":true,"received_frames":10,"sent_frames":10,"tool":"nexus-sim","trailing_bytes":0}
```

Point `--exec` at *your* passthrough during development to confirm your framing
before wiring it into a graph. (The battery expects an identity passthrough, so
run it against a passthrough build of your codec, not a channel-swapping demux.)
You can also feed your codec crafted frames by hand — the format is pipe-testable
with any tool that speaks bytes.

## 7. The envelope contract versus the wire protocol

You will see two versions in `codec-api`. Know which one is yours.

- **`ENVELOPE_VERSION` (currently 1)** — the **public** contract, versioned *for
  codec authors*. It governs the exec-codec child-process interface: the frames in
  this guide, the four event kinds, and the frozen golden vectors. This is the
  only thing an exec codec author cares about.
- **`WIRE_VERSION` (currently 1)** — the **internal** daemon-to-daemon link
  protocol. It carries the same envelope frames but opens each connection with a
  distinct `hello` frame (a magic number, a version, capabilities, channel
  announcements) that an exec child never sends or sees.

In v1 they deliberately **share one frame format and one implementation**, but
they are **two separately versioned contracts** (§8, §15.15). The rule that
protects you: **wire evolution must never break envelope users.** The daemon may
change how two daemons talk to each other, or bump `WIRE_VERSION`, without
touching the envelope — the golden vectors stay byte-frozen across wire evolution
by construction. So as a codec author you can ignore the wire entirely and program
strictly against the envelope described here.
