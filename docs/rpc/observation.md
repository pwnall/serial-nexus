# Observation verbs

Methods that report *observed* state — the environment-owned half of the strict
split (§15.8). Observed state is never persisted and, by construction, absent
from every configuration type: the fields here simply do not exist in `dump`.

Methods on this page: [`state`](#state), [`subscribe`](#subscribe),
[`info`](#info), [`ports`](#ports), [`tap.open`](#tapopen),
[`tap.close`](#tapclose), [`tap.wait`](#tapwait). This page also
documents the [notification stream](#notifications) — the `state`/`lock` pair
`subscribe` opens plus the per-tap `tap.data`/`tap.closed` — and the
[`LockSnapshot`](#locksnapshot) shape shared by `state`, the `lock`
notification, and the arbitration verbs.

---

## `state`

Report the observed status of every node, every host-facing endpoint's tap/ring
accounting, every open tap and every armed pattern wait — a point-in-time
snapshot.

### Params

None.

### Result

Four arrays, all present unconditionally (empty ones are `[]`, never omitted):

| Field | Type | Description |
| --- | --- | --- |
| `nodes` | array | one object per node, in graph order |
| `taps` | array | one object per open [tap](#taps), across every connection |
| `endpoints` | array | one object per **host-facing** endpoint — each carries a tap hub, whether or not a ring is configured on it. Sorted by endpoint, so `state` reads the same twice running |
| `waits` | array | one object per armed [pattern wait](#tapwait), across every connection |

Each node object carries:

| Field | Type | Description |
| --- | --- | --- |
| `name` | string | the node name |
| `status` | string | `active`, `waiting`, or `faulted` |
| `reason` | string | present only for `waiting`/`faulted`: why |
| `since_unix_ms` | integer | when the node *entered* this status, in milliseconds since the Unix epoch — §7's "status … with reason and timestamp" |
| *(node-type extras)* | varies | observed counters/details for the node kind (e.g. serial driver counters, log/leg/exec/codec internals, [map substitution counters](#map-node-state)) — observed-only, disjoint from config |
| `lock` | `LockSnapshot` | present on a single-endpoint node (e.g. serial): its host-facing endpoint's write lock |
| `channels` | object | present on a multi-endpoint node (e.g. codec): `channels[<channel>].lock` is each channel's `LockSnapshot` |

`status` is a tagged value: `waiting` and `faulted` add a `reason` string;
`active` has none. `waiting`/`faulted` are the same state family — an
environmental failure faults a node without removing it, and it heals on its own
(§15.8). `since_unix_ms` is re-stamped only on a *real* transition — a recovery
poll that re-reports the same status and reason leaves it alone — so it answers
"since when has it been faulted?" rather than "when was it last polled?". It is
wall clock, so it moves with a clock step like every other absolute time on the
box, and a rebuilt node is re-stamped to now (a rebuilt node has no history).
The node-type extras vary by kind and are enumerated in full below — §5 makes
this page "the authoritative per-kind enumeration", so a key the daemon puts in
`state` and this page does not name is a defect in one of the two, and a
meta-gate says so (`itest/tests/meta_derive.rs`). Treat them as informational
except for the loss counters, which §5 *requires* to exist.

Each object in `endpoints`:

| Field | Type | Description |
| --- | --- | --- |
| `endpoint` | string | the host-facing endpoint display (e.g. `usb0`, `mux/console`) |
| `feed_dropped` | integer | hostward bytes this endpoint's producer→hub feed did not carry (§5): the hub fell behind the producer under a firehose, **or** nothing was listening at all — on a `replay_ring = 0` endpoint that is the whole window between one `tap.close` and the next `tap.open`, so such an endpoint accumulates this counter while untapped. See [the offset contract](#tapdata-notification) |
| `taps` | integer | how many taps are currently open on it |
| `waits` | integer | how many [pattern waits](#tapwait) are currently armed on it. An armed wait makes the endpoint mirror exactly as an open tap does, so a `taps: 0` endpoint with `waits: 1` is being observed |

Each object in `taps`:

| Field | Type | Description |
| --- | --- | --- |
| `tap` | integer | the tap id, daemon-unique across endpoints and connections |
| `endpoint` | string | the endpoint it observes |
| `dropped` | integer | bytes dropped toward *this* tap because its connection's bounded queue was full — a slow viewer costs only its own counter (§5, §17) |
| `feed_dropped` | integer | its endpoint's `feed_dropped`, mirrored here for convenience |

Each object in `waits` (§10 clause 7 — an armed wait is visible for its lifetime,
as a tap is, because an observer an operator cannot see is one they cannot account
for):

| Field | Type | Description |
| --- | --- | --- |
| `wait` | integer | the wait id, daemon-unique and never reused — it shares one allocator with `tap` ids, so an id on the observation surface names exactly one observer |
| `endpoint` | string | the endpoint it watches |
| `patterns` | array of string | the pattern names it is watching for, in the caller's order. Names only: the pattern bytes are the caller's, and `state` is readable by anyone who can reach the socket |
| `bytes_scanned` | integer | stream bytes it has observed so far |
| `gaps` | integer | how many feed gaps have reset its lookback window |

`feed_dropped` is reported per *endpoint* rather than only per tap because since
§15.32 every host-facing endpoint carries a ring whether or not anyone is
watching: a counter only reachable through an open tap would be a §5 loss nobody
can read.

### The node-type extras, per kind

Every key each node kind puts in its `state` object, in full. The loss counters
get their own section below, which says what each one *means*; this one exists so
that the enumeration is complete — §5 names this page the authoritative one, and
until 2026-08-15 twenty-nine of these keys appeared nowhere on it (plan §18 item
63). A `null` in the Type column is a real reported value and never an omission:
it is how a reading the kernel would not give us is distinguished from a zero.

**serial** (§7.1):

| Field | Type | Description |
| --- | --- | --- |
| `identity` | string | the configured device identity, verbatim — resolver form or a raw `/dev` path. Configuration, echoed here so it can be read beside the path it actually resolved to |
| `identity_kind` | string | which identity grammar that is: `usb`, `by-path`, `by-id`, `raw`, or `unknown` (§12) |
| `resolved_path` | string \| null | the `/dev` node the identity resolves to *now* — state, not configuration (§12). `null` when nothing answers it |
| `baud` | integer | the rate the port is opened at (§7.1) |
| `open` | boolean | whether the daemon currently holds the port open. `false` with `status: "waiting"` is an unplugged device, not a failure |
| `discarded_unattached` | integer | see [loss counters](#loss-counters-in-the-node-type-extras) |
| `purged_on_reconnect` | integer | see [loss counters](#loss-counters-in-the-node-type-extras) |
| `discarded_at_teardown` | integer | see [loss counters](#loss-counters-in-the-node-type-extras) |
| `modem_lines` | object \| null | the live modem-line reading, `null` where the device has no `TIOCMGET` (a pty-backed `raw:` device). Six booleans: `dtr`, `rts` (outputs the daemon asserts, §7.1), `cts`, `dsr`, `dcd`, `ri` (inputs the peer asserts). The *configured* initial assertions are `modem` in `dump`; these are what the line reads back, which is the pair §15.58 keeps apart |
| `driver_counters` | object \| null | `TIOCGICOUNT`, `null` where the device does not support it — `rx`, `tx` (characters the driver has seen in each direction), `frame`, `overrun`, `parity`, `brk`, `buf_overrun`. Kernel-side loss, reported beside the daemon's own counters so the layer that lost the bytes is named (§5, §7.1) |

**pty** (§7.2):

| Field | Type | Description |
| --- | --- | --- |
| `pts_path` | string | the pts node the daemon allocated (e.g. `/dev/pts/7`) — state: it changes on every re-creation |
| `symlink` | string | the configured stable path pointing at it (`path` in `dump`) |
| `advertised_baud` | integer | the cosmetic rate reported to a client's `tcgetattr` (§7.2). Applied only when it names a standard rate; a nonstandard value is skipped rather than approximated, so this field can report a rate the pair is not actually set to |
| `client_present` | boolean | whether a process currently holds the slave. §7.2's presence gate: hostward output is delivered while this is `true` and counted-and-discarded while it is `false` |
| `client_termios` | object \| null | the *client's* current termios, `null` until a client touches it — `baud` (the debug spelling of the speed, e.g. `B115200`), `char_bits`, `parity` (`none`/`odd`/`even`), `echo`, `icanon`, `extproc`. Observed, never configured: it is what the attached program asked for, which is how a console that has turned canonical mode back on becomes visible |
| `discarded_no_client`, `dropped_slow_consumer`, `discarded_targetward`, `discarded_at_last_close` | integer | see [loss counters](#loss-counters-in-the-node-type-extras) |

**log** (§7.3):

| Field | Type | Description |
| --- | --- | --- |
| `current_file` | string | the path being written now — `directory` and `filename` joined, so a rotation is visible without recomputing it |
| `rotation` | integer | how many rotations this node has performed; the counter the `rotation_padding` suffix renders |
| `queued_bytes`, `dropped_bytes`, `write_errors`, `last_write_error` | integer / string \| null | see [loss counters](#loss-counters-in-the-node-type-extras) |

**codec** (§7.5) and **exec** (§7.6) — one shape, two kinds:

| Field | Type | Description |
| --- | --- | --- |
| `codec` | string | the registry name selecting the transform; literally `"exec"` for an exec codec |
| `faces` | string | `host` or `target`: which way the multiplexed side points (§7.5) |
| `framing_errors`, `demux_errors`, `last_demux_error` | integer / string \| null | codec only — see [loss counters](#loss-counters-in-the-node-type-extras) |
| `restart_count` | integer | exec only: how many times the child has been respawned (§7.6) |
| `discarded_unframable` | integer | exec only — see [loss counters](#loss-counters-in-the-node-type-extras) |
| `multiplexed` | object | the multiplexed side's own counters: `dropped_slow_consumer`, `discarded_targetward`, and (codec only) `discarded_hostward` |
| `channels` | object | keyed by channel identity. Each carries `status` (`active` once any data has crossed it, else `waiting`), `delivered_hostward`, `discarded_unattached`, and — codec only — `accepted_targetward` and `discarded_targetward` |
| `discarded_at_teardown`, `discarded_unconfigured_channel`, `unconfigured_channels`, `unconfigured_overflow` | integer / array | see [loss counters](#loss-counters-in-the-node-type-extras) |

`delivered_hostward` and `accepted_targetward` are the two counters §5's
head-of-line clause is pinned against (`itest/tests/p6_head_of_line.rs`), and
they are asymmetric on purpose: `delivered_hostward` counts channel bytes handed
*to* the consumer boundary — a slow consumer's own drops are counted at that
boundary, not here — while `accepted_targetward` counts channel bytes handed
*into* the device-side channel, which is a handoff and not device consumption. It
freezes while the transform does not hold the write lock, which is §6's stall
made visible rather than a stuck counter.

**leg** (§7.4):

| Field | Type | Description |
| --- | --- | --- |
| `role` | string | `listen` or `connect` |
| `transport` | string | `tcp` or `unix` |
| `faces` | string | `host` or `target`: which way every channel endpoint points |
| `connection` | string | the socket's observed condition — `connected`, `connecting`, `listening`, or `disconnected`. Distinct from the node's `status`, which is about the node |
| `peer_address` | string \| null | the peer actually on the far end, `null` while there is none. The *configured* `address` is in `dump`; on a `listen` leg the two are different by nature |
| `protocol_version` | integer \| null | the wire protocol version the peer announced in its hello (§9), `null` before one arrives. §15.16's version-skew signal, read here rather than out of a log line |
| `capabilities` | integer \| null | the peer's announced capability bitmap (§9), `null` before a hello. Reported as the peer sent it: an unknown bit is a peer that is newer, which is the thing worth seeing |
| `reconnect_count` | integer | how many times this leg has re-established its connection. A leg that is `connected` with a rising count is a flapping link, which is invisible in `status` alone |
| `insecure_bind` | `true` | **present only when set** — §9's named footgun, surfaced as a greppable confession that this leg bound or dialled off-loopback (§15.12). Absent, never `false`: the whole point is that it appears exactly where it was opted into |
| `channels` | object | keyed by channel identity. Each carries `binding` (`bound`, `waiting`, or `unbound` for an identity the peer announced that this leg is not configured for), `active` (whether data has crossed it), and its counters — see below |
| `unbound_overflow`, `discarded_at_teardown` | integer | see [loss counters](#loss-counters-in-the-node-type-extras) |

**map** (§7.8): `hostward`, `targetward` and `raw` — [its own section](#map-node-state).

### Loss counters in the node-type extras

The extras vary by kind, but §5's accounting doctrine ("loss is always counted,
where it happens") makes one family of them meaningful everywhere. Per kind:

* **serial** — `discarded_unattached` (hostward bytes with no consumer bound),
  `purged_on_reconnect` (targetward backlog discarded when the device came back,
  §7.1), `discarded_at_teardown` (the targetward backlog destroyed when the node
  stopped — see below; `0` on any node you can still see in `state`), and
  `driver_counters` (`frame`/`overrun`/`parity`/`buf_overrun` from
  `TIOCGICOUNT`, `null` where the device does not support it). The two targetward
  figures are the two ways the same backlog can end and must not be confused: the
  first is the drop §6 sanctions on a reconnect, the second is the node ceasing to
  exist with bytes still owed.
* **pty** — `discarded_no_client` (no process held the slave),
  `dropped_slow_consumer` (the client did not drain its bounded buffer),
  `discarded_targetward` (client bytes read off the master that this node could
  not hand on because its host-facing endpoint went away between the read and
  the send, §15.35) and `discarded_at_last_close` (device bytes the kernel still
  held for a client that never read them, discarded when that client detached so
  the next session starts on an empty pair, §7.2). The last two are easy to
  confuse and mean opposite directions: `discarded_targetward` is console→device
  loss with no endpoint left to take it, and `discarded_at_last_close` is
  device→console output nobody was there to read. Neither is where a *detaching*
  client's un-delivered typing lands — that is §6's per-origin `purged`
  ([`LockSnapshot`](#locksnapshot)), because it was purged on purpose at the
  moment the floor question settled rather than lost, so a console with a
  writable edge that merely came and went leaves `discarded_targetward` at `0`.
  The exception is the case with no origin to attribute to — a read-only spy
  edge, or one a `disconnect` cleared — where the node's own counter is the only
  honest home and does take those bytes.
* **log** — `dropped_bytes` (queue overflow, ingest drops, and — once the node's
  writer has stopped, on a fatal write under `overflow = "fault"` or any failed
  rotation — every byte offered afterwards), `queued_bytes` (waiting in the queue
  *plus* the batch the writer is holding; it falls to `0` and stays there once the
  writer has stopped, because nothing will drain that queue again, so those bytes
  are loss rather than backlog), and `write_errors` / `last_write_error`, which
  separate "the filesystem is refusing every write" from "the consumer is slow" —
  under the default drop-oldest overflow policy both otherwise surface only as a
  rising `dropped_bytes`.
* **codec** — `framing_errors` (the *transform's own* resyncs past corrupt frames,
  §7.5 — that is `Codec::resync_count()`, whose trait default is `0` for exactly
  the codecs that never resync), `demux_errors` / `last_demux_error` /
  `multiplexed.discarded_hostward` (the *daemon's* count of `Codec::demux`
  refusals, the most recent message, and the multiplexed bytes those refusals
  did **not** turn into payload — the chunk as it arrived, less the `data` bytes
  the same `demux` call emitted before it failed, because a partial decode is the
  realistic shape (a non-resyncing framer takes the good frames out of a 64 KiB
  chunk and refuses on the corrupt tail) and those events are still delivered and
  credited to their channels; charging the whole chunk reported one payload as
  delivered *and* as lost. The framing overhead of the salvaged frames is
  included in the charge, since the trait reports emitted payload and not
  consumption — the residual errs toward reporting loss rather than hiding it. A
  refusal also **faults** the node, so §7.5's sanctioned never-resync policy is
  visible in `state` and not only in the daemon log),
  `multiplexed.dropped_slow_consumer`, `multiplexed.discarded_targetward`, and per
  channel `discarded_unattached` / `discarded_targetward`.
* **exec** — `discarded_unframable`, `multiplexed.dropped_slow_consumer`,
  `multiplexed.discarded_targetward`, `restart_count`, and per channel
  `discarded_unattached`.
* **map, codec, exec, serial and leg alike** — `discarded_at_teardown`: targetward bytes
  that were queued for the node's own pump and were destroyed because the node was torn
  down and will never deliver them. It reads `0` for the whole of a node's working life
  and moves exactly once, at `signal_stop`, so on a *surviving* node it is always `0`
  and the queued bytes are backlog rather than loss — the node may still deliver them
  if a `connect` gives it somewhere to go, if the device comes back, or if a peer
  arrives. It is therefore mostly read from the
  [`remove-node`](configuration.md#remove-node) reply, which carries the same figure
  for the node it just destroyed: `state` cannot report the last loss of a node that
  no longer appears in `state`. Distinct from every neighbouring counter, and the
  distinction is the point: `discarded_no_raw_edge` (map) and
  `multiplexed.discarded_targetward` (codec/exec) name bytes the pump *looked at* and
  decided to swallow, while this names bytes it never got to look at; §6's per-origin
  `purged` names bytes discarded *deliberately* when the write floor settled; and
  `purged_on_reconnect` (serial, leg) names the one *sanctioned* targetward drop, bytes
  a reconnect deliberately threw away rather than fire into a rebooted device. Until
  2026-08-04 this loss had no name at all and one cascade destroyed 808 448 bytes in
  silence (`docs/implementation-notes.md` §3.31). What each kind's figure covers:

  * **map** and **codec** — their whole host-facing targetward exposure, exactly.
  * **exec** — a **total**, since plan §18 item 21; it was a floor until then, and the one
    kind whose figure was. Both watched stages are counted now: the per-channel host-facing
    queues its forwarders read from, *and* the internal merged queue they push into, which
    `pump_child` reads. Two exclusions remain, and neither is a shortfall in this figure —
    they are bytes that are not this counter's to claim. The merge queue is *bidirectional*,
    so roughly half of it is the device stream travelling hostward on its way into the
    child; charging that would report a hostward loss under a targetward name. And bytes
    already written into the child's stdin pipe have left the daemon exactly as bytes
    written to a device fd have, which makes them delivery rather than loss — the same line
    the serial node already draws. *(This paragraph said "a floor, not a total, and
    deliberately so" until 2026-08-15: item 21 corrected the counter and the doc comment
    beside it and left this page, which is the one an operator reads. Plan §18 item 63.)*
  * **serial** — the backlog a `waiting` node accumulates, which is the deepest one the
    daemon legally holds: §5/§7.1's whole answer to an absent device is that its origins
    backpressure rather than lose their commands, so those bytes are owed until the node
    stops existing.
  * **leg** — reported **per channel** as well as summed on the node, because §5 asks for
    loss that is attributable and one number for eight channels says what was lost
    without saying where. A `faces = "host"` leg's counted queue is the arbitrated
    targetward stream local writers feed; a `faces = "target"` leg's is the wire-arriving
    stream its channel tasks hand into the local graph. A `faces = "target"` leg's
    per-channel *hostward* relay is deliberately excluded — those bytes travel the other
    way, and hostward loss is `dropped_slow_consumer`'s to report.
  * **pty** and **log** — structurally `0`: neither owns a queue of this shape. The pty's
    own undelivered payload is a `pending` slot inside its reader's stack frame, which
    is not reachable this way and is not fixed by this counter
    (`docs/implementation-notes.md` §3.31, still open).
* **codec and exec alike** — `discarded_unconfigured_channel` (bytes decoded onto
  a channel identity the node is not configured for: still dropped, §8 — an
  announcement never grows the graph — but counted where they are lost, §5), the
  bounded, deduplicated `unconfigured_channels` list naming those identities, and
  `unconfigured_overflow` for the occurrences the 256-entry cap refused to record.
  These are the leg's `unbound` / `unbound_overflow` terms applied to a codec's
  channels, and they are what distinguishes a mis-spelled channel from a stream
  the graph never enumerated. An identity longer than 64 bytes is stored truncated
  on a `char` boundary and marked `…(truncated)`.
* **leg** — per channel `discarded_hostward`, `discarded_targetward`,
  `discarded_unframable`, `discarded_peer_gone` (the untransmitted tail of a chunk
  the socket write half had already taken off its bounded receiver when the peer
  went away) and `purged_on_reconnect`, plus `dropped_slow_consumer` — what an
  upstream producer shed because a `faces = "target"` channel's intake was full,
  structurally zero for `faces = "host"`, whose channel endpoints are host-facing
  and have no such boundary; each drop is charged to the direction it was actually
  travelling. Per channel also `discarded_at_teardown` — that channel's targetward
  queue destroyed when the node stopped, `0` on any leg you can still see in `state`
  — with the node-level sum of the same figure beside `unbound_overflow`, which is
  what the [`remove-node`](configuration.md#remove-node) reply quotes. Node-level
  `unbound_overflow` counts
  wire frames whose unconfigured channel identity the bounded `unbound` list
  refused to record, so a peer inventing identities is visible rather than
  silent; the identities it did record appear in `channels` as
  `{"binding": "unbound"}`.
* **map** — `hostward.discarded_unattached` (mapped bytes that reached no live
  consumer) and `targetward.discarded_no_raw_edge` (bytes a read-only map
  swallowed, `write_mode = "never"` on the raw edge), plus
  `raw.dropped_slow_consumer`. Without the first two, a map's bytes would be
  visible in a tap while silently absent from the graph — the pairing §5 forbids.

### CLI

```console
$ serial-nexus-ctl state          # one line per node: "<name>  <status> (reason)"
$ serial-nexus-ctl --json state   # the raw {"endpoints":…,"nodes":…,"taps":…,"waits":…} object
```

The rendered form prints nodes only; `--json` is the way to reach the timestamp,
the counters, and the tap/endpoint arrays.

### Errors

None beyond the transport-level codes.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"state"}' | nc -N -U "$SOCK" | jq .result
{
  "endpoints": [
    { "endpoint": "usb0", "feed_dropped": 0, "taps": 1, "waits": 0 }
  ],
  "nodes": [
    {
      "name": "usb0",
      "status": "faulted",
      "reason": "ENOENT: /dev/ttyUSB0",
      "since_unix_ms": 1785016067321,
      "lock": {
        "arbitration": "exclusive",
        "holder": null,
        "origins": [],
        "waiters": []
      }
    }
  ],
  "taps": [
    { "tap": 1, "endpoint": "usb0", "dropped": 0, "feed_dropped": 0 }
  ],
  "waits": []
}
```

### Map node state

A [`map`](configuration.md#the-map-node--character-mapping-78) node reports its
observed transform activity as node-type extras — the cheap way to discover which
quirk a mystery console actually has (§7.8). Each direction (`hostward`,
`targetward`) carries:

| Field | Type | Description |
| --- | --- | --- |
| `bytes_in` | integer | input bytes seen in this direction |
| `bytes_out` | integer | output bytes produced (differs from `bytes_in` when rules expand or delete) |
| `rules` | object | per-rule substitution counts, keyed by mapping name — how many input bytes each configured rule actually substituted (a shadowed rule stays `0`) |

Each direction also carries its own §5 loss counter, because a map is a producer
like any other and §7.8 gives it a default ring — uncounted loss would be visible
in a tap while silently absent from the graph. Hostward it is
`discarded_unattached`: mapped bytes that reached no live consumer of the mapped
endpoint (none bound, or every one cascade-removed). Targetward it is
`discarded_no_raw_edge`: bytes discarded because the map has no writable raw edge
— the read-only/display map (`write_mode = "never"`) or an unattached raw side.
Those bytes are drained and counted rather than left to close the channel, so a
read-only map is inert rather than destructive to the writer feeding it.

A `raw.dropped_slow_consumer` count surfaces hostward bytes the upstream dropped
because the map's raw-side intake was full (§5 — the map falling behind, counted
where it happens, like a codec's multiplexed-side drop count). The map's mapped
endpoint's write lock appears in the top-level `lock` field, like any single
host-facing-endpoint node.

```console
$ serial-nexus-ctl --json state | jq '.nodes[] | select(.name=="console")'
{
  "name": "console",
  "status": "active",
  "since_unix_ms": 1785016067322,
  "hostward": {
    "bytes_in": 4096, "bytes_out": 4103,
    "rules": { "lfcrlf": 7 },
    "discarded_unattached": 0
  },
  "targetward": {
    "bytes_in": 12, "bytes_out": 12,
    "rules": { "lfcr": 1 },
    "discarded_no_raw_edge": 0
  },
  "raw": { "dropped_slow_consumer": 0 },
  "lock": { "arbitration": "exclusive", "holder": null, "origins": [], "waiters": [] }
}
```

---

## `subscribe`

Open a live stream of daemon → client notifications on this connection (§10).
The immediate reply is a one-field acknowledgement; thereafter the daemon pushes
id-less [notification](#notifications) lines on the same connection until the
client disconnects. Requests may still be issued on the connection afterward.

### Params

None.

### Result

| Field | Type | Description |
| --- | --- | --- |
| `subscribed` | bool | always `true` — the subscription acknowledgement |

### CLI

```console
$ serial-nexus-ctl subscribe             # one JSON notification per line, forever
$ serial-nexus-ctl subscribe --count 3   # exit after 3 notifications
```

`serial-nexus-ctl subscribe` swallows the acknowledgement and prints one JSON
notification object per line (a clean stream for `jq`), exiting after `--count`
of them or when the daemon closes the connection.

### Errors

None beyond the transport-level codes.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"subscribe"}' | nc -U "$SOCK"
{"jsonrpc":"2.0","id":1,"result":{"subscribed":true}}
{"jsonrpc":"2.0","method":"state","params":{"nodes":[ ... ], "taps": [], "endpoints": [ ... ], "waits": []}}
{"jsonrpc":"2.0","method":"lock","params":{"endpoint":"usb0","lock":{ ... }}}
```

The first line is the correlated response; every line after it is an id-less
notification. Note the **plain `nc -U`** here, against the `-N` every one-shot
example on these pages uses: the daemon closes a connection on read end-of-file,
so half-closing the write half ends the stream right after the acknowledgement.
A streaming client keeps both halves open.

---

## `info`

Report the daemon's **capability surface** (§10, §15.26): its version, the wire
and envelope protocol versions, and every codec name a configuration may use,
including the built-in `exec`.
Tools — and a version-skewed CLI — use it to *discover* what a daemon supports
rather than assume it, which matters because the daemon is embeddable: an
out-of-tree binary built on the `serial-nexus-daemon` library registers its own codecs
(§15.26), and `info` is how the unchanged `serial-nexus-ctl`, `serial-nexus-sim`, and
`serial-nexus-doctor` learn that daemon's codec set. The same list appears in an
unknown-codec load error's `data.available` (see
[configuration.md](configuration.md)), so a misconfiguration names the codecs
that *would* have worked.

Pure observation; touches no graph state.

### Params

None.

### Result

| Field | Type | Description |
| --- | --- | --- |
| `daemon_version` | string | the `serial-nexus-daemon` library (engine) version — what determines wire and behavior compatibility |
| `wire_version` | integer | the daemon-to-daemon wire protocol version (§9) |
| `envelope_version` | integer | the exec-codec envelope version (§8/§15.15) — a codec author pins against this |
| `codecs` | array of string | every codec name a configuration may legally name, sorted: the registered in-process codecs **plus** the reserved `exec` child-process codec (§7.6), which is always available. `exec` is deliberately not a registry entry — it is a child *process*, routed before the registry is consulted — but that is an implementation fact, and leaving it out of this list made the discovery surface disagree with what a `codec = …` field accepts |
| `instance` | integer | a per-boot nonce (plan §11.8). Tap byte offsets are only comparable within one daemon process; on restart the offsets reset to 0 and this value changes, so a client keyed on it (the web console's browser history, §17) detects the reset and starts fresh instead of splicing across it |

### CLI

```console
$ serial-nexus-ctl info          # rendered: version, wire/envelope, codec list
$ serial-nexus-ctl --json info   # the raw object
```

### Errors

None beyond the transport-level codes.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"info"}' | nc -N -U "$SOCK" | jq .result
{
  "daemon_version": "0.3.0",
  "wire_version": 1,
  "envelope_version": 1,
  "codecs": ["exec", "reference"],
  "instance": 12719384756019283746
}
```

---

## `ports`

List the serial devices the resolver can see on this machine, the identity that
would bind each one, and whether a node in the running graph already holds it
(§12, §15.35). This is the resolver's enumeration face: the answer to "what is
plugged in, and what is still free", so an operator no longer has to learn device
paths out-of-band before `add-node`.

**Scope, stated precisely.** Four sources (§12, §15.35), unioned and deduplicated
by device node: `/dev/serial/by-id`, `/dev/serial/by-path`, the
`<sys-root>/class/tty` listing — which covers a USB adapter udev named *nowhere*,
a container handed only `--device=/dev/ttyUSB0`, an image without
`60-serial.rules` — and, on BSD/macOS, `cu.*` nodes under `/dev`. A device none of
the four yields is **not listed**: an on-board UART (`/dev/ttyS0`, `/dev/ttyAMA0`)
is in `class/tty` like every other tty, but its sysfs ancestor walk finds no
`idVendor`, so it produces no `usb:` identity and is filtered out — it is missing
for want of a USB identity, not for want of a udev link, and the same is true of an
adapter whose driver exposes none. Those are still perfectly bindable — by
a `raw:` path, which is what §12's escape hatch is for — they simply do not appear
here. `ports` is a discovery aid, not an inventory.

**Strictly passive, and that is a contract rather than an implementation detail.**
The whole result is built from `/dev/serial/by-id` and `/dev/serial/by-path`
readlinks, a `<sys-root>/class/tty` listing and the sysfs reads its entries lead
to, and a `<dev-root>/dev` listing for BSD/macOS `cu.*` callout nodes. Listings,
readlinks and sysfs reads: no candidate device is opened, because opening a
USB-serial adapter asserts DTR and resets the board behind it — on exactly the
hardware people care about. `ports` is the verb you run to *look*.

The `identity` field comes from the same §12 fallback chain `add-node` captures
with (`usb:` → `by-path:` → `raw:`), so what `ports` shows is precisely what
binding that path would store — not a second opinion about it. An adapter whose
serial number is absent or duplicated therefore appears as `by-path:`, carrying
the documented instability `warning`, exactly as it would after an add.

Pure observation; touches no graph state and opens no device.

### Params

None.

### Result

| Field | Type | Description |
| --- | --- | --- |
| `ports` | array of object | one entry per device node found, sorted by `path` |

Each entry:

| Field | Type | Description |
| --- | --- | --- |
| `identity` | string | the canonical identity to put in a serial node's `device` field (§12) |
| `kind` | string | `"usb"`, `"by-path"` or `"raw"` — which form `identity` uses |
| `path` | string | the `/dev` path the device currently occupies |
| `description` | string | human echo, e.g. `FTDI FT232R USB UART, serial A6008isP, interface 00` |
| `by_id` | string \| null | the `/dev/serial/by-id` entry name, when the device has one |
| `warning` | string \| null | the documented instability warning carried by the `by-path`/`raw` fallbacks (§12); `null` for a `usb:` identity |
| `bound_to` | string \| null | the serial node that already binds this device, or `null` when it is free |

`bound_to` is decided by resolving each serial node's stored identity to its
*current path* and comparing canonicalized paths, so a device bound by `usb:`
identity, by `by-path:`, or by a raw path — including a raw path that is a symlink
into `/dev/serial/by-id` — all report bound. Comparing identity spellings would
only have caught the first. When two nodes are configured on one device (which
loads, and leaves the second faulted on `TIOCEXCL`), `bound_to` names the first.

### CLI

```console
$ serial-nexus-ctl ports          # rendered: path, free/bound, identity, description
$ serial-nexus-ctl --json ports   # the raw object
```

### Errors

None beyond the transport-level codes. A machine with no serial devices returns
an empty array, not an error.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"ports"}' | nc -N -U "$SOCK" | jq .result
{
  "ports": [
    {
      "identity": "usb:0403:6001:BH00LL8O:00",
      "kind": "usb",
      "path": "/dev/ttyUSB0",
      "description": "FTDI FT232R USB UART, serial BH00LL8O, interface 00",
      "by_id": "usb-FTDI_FT232R_USB_UART_BH00LL8O-if00-port0",
      "warning": null,
      "bound_to": "usb0"
    },
    {
      "identity": "usb:0403:6001:BH00L4KU:00",
      "kind": "usb",
      "path": "/dev/ttyUSB1",
      "description": "FTDI FT232R USB UART, serial BH00L4KU, interface 00",
      "by_id": "usb-FTDI_FT232R_USB_UART_BH00L4KU-if00-port0",
      "warning": null,
      "bound_to": null
    }
  ]
}
```

---

## Notifications

Notifications are id-less messages — `{"jsonrpc":"2.0","method":…,"params":…}`.
`state` and `lock` are delivered only on a *subscribed* connection; `tap.data`
and `tap.closed` are delivered on the connection that opened the tap, subscribed
or not (§10 delivers a tap's stream *on that connection*). Four methods are
emitted:

### `state` notification

A **full state snapshot**, identical in shape to the [`state`](#state) result
(`{ "nodes": […], "taps": […], "endpoints": […], "waits": […] }`). It is emitted on a periodic
tick (currently every 200 ms) and is the *floor* for observability — status
transitions and counter snapshots are always visible here even if a finer signal
is missed. State snapshots are cumulative, so a subscriber that falls behind and
drops one loses nothing. The tick is built only while at least one connection is
actually subscribed, so it costs nothing on a daemon nobody is watching.

```json
{"jsonrpc":"2.0","method":"state","params":{"nodes":[ ... ],"taps":[ ... ],"endpoints":[ ... ],"waits":[ ... ]}}
```

### `lock` notification

An **immediate, per-transition** signal for one endpoint's write lock — emitted
synchronously on every acquire, release, steal, lease expiry, and
detach-release (§10, §15.20), rather than waiting for the next periodic tick.

| Param | Type | Description |
| --- | --- | --- |
| `endpoint` | string | the host-facing endpoint display (e.g. `usb0`, `mux/console`) |
| `lock` | `LockSnapshot` | the endpoint's lock state after the transition |

```json
{"jsonrpc":"2.0","method":"lock","params":{"endpoint":"usb0","lock":{ "arbitration":"exclusive","holder":"demux","origins":[ ... ],"waiters":[] }}}
```

### `tap.data` notification

The live hostward byte stream of one open [tap](#taps), base64-chunked. Emitted
on the connection that opened the tap, one per hostward chunk (and, right after
`tap.open --replay`, one per replay-ring piece).

| Param | Type | Description |
| --- | --- | --- |
| `tap` | integer | the tap id from the `tap.open` result |
| `offset` | integer | the endpoint's monotonic hostward byte offset of this chunk's first byte (plan §11.8) — replay pieces carry their true stream offset, so a reconnecting client trims overlap and splices exactly. Offsets are comparable only within one daemon `instance` (see [`info`](#info)) |
| `gap_before` | integer | bytes this endpoint's producer→hub feed did not carry immediately before this chunk — the feed full, or nothing listening — and which are therefore **not** represented in the offset space. Normally `0` |
| `data` | string | base64 of the chunk's bytes |

```json
{"jsonrpc":"2.0","method":"tap.data","params":{"tap":3,"offset":131072,"gap_before":0,"data":"aGVsbG8="}}
```

**The offset contract.** `offset` counts *delivered* bytes: only bytes that
reached the endpoint's hub advance it. That is what makes the replay splice exact
— the ring is contiguous in offset space by construction, so `from_offset` names
the ring's true base and a replay piece can never straddle a hole — and it is why
the one lossy hop on the way in, the bounded producer→hub feed that must never
backpressure the device (§5), cannot be folded into the offsets. Folding it in
would leave `from_offset` naming an offset the live stream never uses, so a client
splicing by offset would silently write replay bytes at the wrong place: a
corruption strictly worse than a gap.

So the hole is reported **beside** the offsets instead. Each `tap.data` carries
the feed-hop loss accumulated since the previous one as `gap_before`, and
`tap.open` returns the feed loss the hub has **already charged** as the client's
baseline — loss recorded since the hub's last chunk arrives instead as the first
`gap_before`, so the two never overlap and `feed_dropped + Σgap_before` is the
true loss rather than twice it. The guarantee a client gets is therefore:
*offsets are contiguous, and a hole is always announced* — `gap_before > 0` means
bytes are missing between the previous chunk and this one. It is exact in size and
approximate in position (the drop happens on the producer's thread with up to a
feed's worth of chunks still queued ahead of it, so it is attributed at most that
early) — except for loss recorded while the feed was *inactive*, whose position is
exact, the feed being empty by definition while nothing is listening — which is
enough to detect the hole, which is what §5 requires. A client splicing by offset
must treat a non-zero `gap_before` as a discontinuity rather than concatenating.

Offsets reset to 0 when the daemon restarts, and the `instance` nonce
[`info`](#info) reports changes with them — a client keyed on it starts fresh
instead of splicing across the reset.

### `tap.closed` notification

The terminal event for one tap: the graph dropped its endpoint out from under it
(§17). `teardown`, `load --replace` and `remove-node` all reach it. It is
terminal for the *tap*, not for the connection — the connection's other taps and
its subscription keep running — and without it the client would sit on a live
connection receiving nothing, with no notification and no error.

| Param | Type | Description |
| --- | --- | --- |
| `tap` | integer | the tap id that just ended |
| `endpoint` | string | the endpoint it was observing, so a client with several taps learns *which* one died |
| `reason` | string | a stable token: `endpoint removed` (`remove-node`), `graph replaced` (`load --replace`), or `teardown` (`teardown`, and the clean-shutdown path) |

```json
{"jsonrpc":"2.0","method":"tap.closed","params":{"tap":0,"endpoint":"console","reason":"graph replaced"}}
```

**This event is not droppable.** `tap.data` is bounded and lossy by design — a
connection that stops draining its queue is counted as dropping (§5) — but the
terminal event is not part of that bargain: a full queue is the ordinary steady
state of a slow consumer on a firehose endpoint, which is exactly the client this
notification exists for. It rides the tap's own queue while there is room, and an
overflow lane when there is not, so a saturated client still gets it.

The one visible consequence of that lane: when it is used, `tap.closed` arrives
**ahead of** any `tap.data` for the same tap still queued behind it. Those bytes
belong to an endpoint that no longer exists, and a client that treats the terminal
event as terminal — dropping later frames carrying that tap id, which is what both
`serial-nexus-ctl tap` and the web console do — is correct. Tap ids are never
reused, so a late frame is never ambiguous.

A later `tap.close` for that id is refused rather than reporting a success that
closed nothing — see [`tap.close`](#tapclose).

---

## Taps

A **tap** is a connection-scoped, read-only observer on a host-facing endpoint
(§17): it streams that endpoint's hostward bytes as `tap.data` notifications and
is torn down when its `tap.close` runs, when the connection drops, or when the
graph drops its endpoint (`tap.closed`). Taps are *state* — they never appear in
configuration or `dump`.

### `tap.open`

| Param | Type | Description |
| --- | --- | --- |
| `endpoint` | string | the host-facing endpoint to observe (`usb0`, `mux/console`) |
| `replay` | bool | *optional* — prefix the endpoint's replay ring (§5) ahead of the live stream, with an exact splice |

Result:

| Field | Type | Description |
| --- | --- | --- |
| `tap` | integer | the new tap id (used by `tap.close` and in `tap.data`) |
| `endpoint` | string | echoed |
| `replay_bytes` | integer | bytes of ring replayed ahead of the live stream — `0` is the explicit empty-replay marker (ring off, or as-yet unfilled) |
| `replay_truncated` | integer | ring bytes `--replay` could **not** hand this tap: the snapshot's head, trimmed because this connection's bounded tap queue had room for fewer bytes than the ring holds. `0` in the ordinary case. Not a hole in the offset space — what *is* delivered is always the newest end of the ring and always contiguous with the live stream — but the only way to tell a short ring from a short channel, which `replay_bytes` alone cannot |
| `from_offset` | integer | the endpoint offset this tap's stream begins at (plan §11.8): with a non-empty replay, the ring's oldest byte — or, when the ring is deeper than the connection's tap channel can be handed at once, the oldest byte of the newest slice that fits, the snapshot being trimmed at its **head** so that `from_offset + replay_bytes` always lands exactly on the live edge; otherwise the live edge, i.e. the offset the next `tap.data` will carry. A reconnecting client trims replay against the last offset it stored |
| `epoch` | integer | which offset space `from_offset` counts in (§15.38). Unique per endpoint *hub* within a daemon process and never reused, so a client holding stored scrollback can tell an ordinary reconnect — where its own frontier is still meaningful and replay overlap must be trimmed — from a hub rebuild (`load --replace`, `add-node`, `remove-node`), after which offsets restart at 0 while the per-*boot* `info.instance` nonce, correctly, does not change. The two are indistinguishable from offsets alone: both present as `from_offset` below the client's frontier, because a replay ring exists to re-send bytes the client already has. Persist it beside the stored offset and re-anchor exactly when it changes |
| `feed_dropped` | integer | the endpoint's producer→hub feed loss that the hub has **already charged to its taps** at open time — the reported-so-far watermark, which may sit below the running counter [`state`](#state) shows. The difference is deliberately excluded: it is delivered to this tap as its first `tap.data`'s `gap_before`, so `feed_dropped + Σgap_before` is the true loss and not twice it (see [the offset contract](#tapdata-notification)) |

Errors: `-32602` when the endpoint is unknown or not host-facing (only a
host-facing endpoint has a hub — a tap observes a hostward stream).

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tap.open","params":{"endpoint":"console","replay":true}}' \
    | nc -U "$SOCK"
{"jsonrpc":"2.0","id":1,"result":{"endpoint":"console","epoch":1,"feed_dropped":0,"from_offset":0,"replay_bytes":0,"replay_truncated":0,"tap":0}}
```

(Plain `nc -U` again: half-closing would end the connection and with it the tap.)

### `tap.close`

| Param | Type | Description |
| --- | --- | --- |
| `tap` | integer | the tap id to close (must be open on this connection) |

Result: `{ "closed": <tap id> }`.

Errors: `-32602` for a missing `tap` param, for an id this connection did not
open (`no open tap <id> on this connection`), and for a tap the daemon has
**already** closed because its endpoint went away (`tap <id> was already closed
by the daemon: its endpoint "console" is gone`). The last is what pairs with the
[`tap.closed`](#tapclosed-notification) notification: a client that missed the
notification learns from the close rather than being told it closed something
that was not there.

### CLI

```console
$ serial-nexus-ctl tap console            # decoded bytes to stdout until the tap or the connection ends
$ serial-nexus-ctl tap console --replay   # ring first, then live (exact splice)
$ serial-nexus-ctl tap console --bytes 4096
```

`serial-nexus-ctl tap` opens the tap, prints the acknowledgement to stderr, and
writes the base64-decoded `tap.data` bytes to stdout, exiting after `--bytes` of
them, on the terminal [`tap.closed`](#tapclosed-notification), or when the
connection closes. A failed open exits non-zero. A `tap.closed` is reported on
stderr as `tap closed: <endpoint> (<reason>) — N byte(s) received` and exits
**0** — the bytes written are intact, and the connection the daemon deliberately
keeps alive after it will never carry another byte for this tap — unless a
`--bytes` budget was still outstanding, which is a short read and exits non-zero
with `tap closed: <endpoint> (<reason>) — N of M requested byte(s) received`.

Both discontinuity signals reach **stderr**, one line each, so stdout stays a
clean byte stream: `tap gap: N bytes lost before offset X (daemon feed)` for a
non-zero `gap_before`, and `tap gap: N bytes dropped before offset X (this tap's
queue)` when `offset` exceeds the previous chunk's end. Each is printed *before*
the bytes that follow it, so a stderr notice orders correctly against a stdout
capture. (An `offset` that goes *backwards* is not a hole any client can act on
and should not happen; it prints `tap warning: offset went backwards to X,
expected Y` rather than being swallowed.)

`--stall-ms` holds the tap open without reading, to exercise the bounded-queue
drop path.

---

## The pattern wait

### `tap.wait`

Park until one of up to eight named byte patterns appears on a host-facing
endpoint's hostward stream, the deadline expires, or the graph drops the endpoint
(§10 *The pattern wait*, §15.56). It is a **waiting verb** on §15.20's machinery:
it occupies this connection's one waiting slot, a request pipelined behind it is
refused with [`-32006`](README.md#error-codes) while the wait and this
connection's taps and subscription all keep running, and closing the connection
cancels it.

It is also a **spy**: arming, matching, timing out and cancelling a wait leave the
replay ring, the tap counters and the graph exactly as a never-issued wait would
have. The one thing an armed wait does change is that its endpoint starts
mirroring — an armed wait observes even on an endpoint with no ring and no open
tap — and it is [visible in `state`](#state) for its lifetime, as a tap is.

**Why the daemon matches, rather than you.** Everything below a matcher was
already here — offset-stamped taps, exact-splice replay, the ring — so a client
*can* do this over [`tap.open`](#tapopen), and the recipe for that is
[below](#doing-it-client-side). What it cannot do is scan deeper than its own
bounded tap channel can be handed, and what it must re-derive every time is
reassembly across frame boundaries, replay-splice handling, gap discipline, and
telling a timeout from a teardown. This verb is those rules written once.

#### Params

| Param | Type | Description |
| --- | --- | --- |
| `endpoint` | string | the host-facing endpoint to watch (`usb0`, `mux/console`) |
| `patterns` | array | 1 to 8 [pattern objects](#pattern-objects); the first to appear wins |
| `timeout_ms` | integer | **required** — give up after this long. Maximum 3600000. Required rather than optional because a wait holds this connection's one waiting slot until it settles, and because a deadline that was never stated cannot be reported as one |
| `replay` | bool | *optional*, default `false` — scan the endpoint's replay ring (§5) before arming on the live stream, so a pattern that appeared just **before** this call still matches |
| `lookback` | integer | *optional*, default 4096, maximum 65536 — how many bytes of already-seen stream a match may span. There is also a **floor**, and it is not a constant: the window must be at least as long as your longest `literal`, because a window shorter than the pattern could never hold it and the verb would answer `timed_out: true` with `gaps: 0` — a clean-scan claim — for a string that was on the wire. `lookback: 0` is refused for the same reason. The daemon can size literals exactly and **cannot** size a `regex` (`a{1000}` is a six-byte source), so for regex patterns the floor is whatever the literals in the same set demand and the adequacy of the window is the caller's to judge |
| `context` | integer | *optional*, default 128, maximum 4096 — how many bytes of surrounding context a match reports |

Every one of these is range-checked, and the patterns compiled, **before anything
is armed**: a refused request leaves the endpoint exactly as it found it, mirror
flag included.

##### Pattern objects

Each entry of `patterns` carries a `name` and **exactly one** of `literal` or
`regex` — the kind is structural rather than a `kind: "literal"` string, so a
misspelling is a refusal that names the field instead of a silent
reinterpretation of your bytes.

| Field | Type | Description |
| --- | --- | --- |
| `name` | string | what the result reports fired. 1 to 256 bytes, and distinct within one call — the result names one pattern, so two patterns sharing a name would make the answer unreadable |
| `literal` | string | base64 of an arbitrary byte string, matched verbatim. Any byte is legal, including regex metacharacters and bytes that are not valid UTF-8 |
| `regex` | string | base64 of a bytes-oriented regular expression's **source**. The source is text (regex syntax is), but what it matches is not: Unicode mode is off, `.` means "any byte but newline", and `\xNN` reaches any byte. A source that is not valid UTF-8 is refused rather than mangled |

Both forms ride the wire base64-encoded for the same reason
[`tap.data`](#tapdata-notification) does: console output is not guaranteed UTF-8,
and neither is a pattern over it. 1024 bytes is the maximum for either.

The engine is **linear-time and non-backtracking**, and that is a requirement
rather than an implementation note: patterns compile and match on the daemon's
runtime thread — the thread that runs every console — so an exponential-backtracking
engine would be an operator-reachable denial of service. `(a+)+b` against a
megabyte of `a` is answered promptly here. The compiled program is capped, so a
counted-repetition bomb (`(?:a{1000}){1000}`) is refused at compile with
`-32602`, not paid for at match time.

#### Result

| Field | Type | Description |
| --- | --- | --- |
| `endpoint` | string | echoed |
| `matched` | object \| null | the match, or `null` when the deadline expired first |
| `timed_out` | bool | `true` exactly when `matched` is `null` |
| `bytes_scanned` | integer | stream bytes this wait observed, replay seed included |
| `gaps` | integer | how many times a producer→hub feed gap (see [the offset contract](#tapdata-notification)) reset the lookback window |
| `gap_bytes` | integer | how many bytes those gaps totalled |
| `from_offset` | integer | the endpoint offset the scan began at: the oldest ring byte scanned with `replay`, otherwise the live edge at arming |
| `epoch` | integer | which offset space `from_offset` and any match offset count in (§15.38) — the same value [`tap.open`](#tapopen) reports |
| `replay_scanned` | integer | ring bytes the replay seed covered; `0` without `replay`, or with no ring |

`matched`, when present:

| Field | Type | Description |
| --- | --- | --- |
| `pattern` | string | the `name` of the pattern that fired |
| `offset` | integer | the endpoint offset of the match's first byte, in the same space `tap.data` reports |
| `end_offset` | integer | one past the match's last byte |
| `context` | string | base64 of up to `context` bytes around the match |
| `context_offset` | integer | the endpoint offset of `context`'s first byte, so you can place it without inferring it |

**A deadline is an answer, not an error.** Expiry returns a normal result with
`timed_out: true` and the counters above — "no pattern appeared within the
deadline" is exactly the thing a caller asked to find out, and reporting it as an
error would make every caller's success path a `catch`. Read `gaps` beside it: a
`matched: null` over a stream with holes in it is a weaker claim than one over a
whole stream, and this is where the difference is stated.

**Matching is exact where the hub is exact.** The matcher consumes the endpoint's
hub stream directly, with no bounded queue between hub and matcher, so a byte the
hub ingested is a byte matched and **a pattern split across `tap.data` frame
boundaries matches by construction**. The one lossy hop on the way in — the
producer→hub feed, which must never backpressure the device (§5) — is handled by
resetting the lookback window when a gap arrives: bytes on either side of a hole
were never adjacent in the stream, so a match is never allowed to span them. That
costs some true negatives and cannot manufacture a positive, and the `gaps` /
`gap_bytes` counters say when it happened.

**Replay inclusion is splice-exact.** With `replay: true` the ring snapshot is
scanned and the live matcher armed inside one critical section — the same
mechanism that makes [`tap.open --replay`](#tapopen) exact — so the ring→live seam
can neither hide a match nor report one twice. The matcher reads **the ring
itself**, not a tap channel's budgeted copy, so the whole configured ring depth is
scanned even when it is deeper than `lookback`; `lookback` bounds only how far a
*later* match may reach back into what is retained.

#### Errors

| Code | When |
| --- | --- |
| `-32602` | the endpoint is unknown or not host-facing; `timeout_ms` missing; any maximum exceeded; **`lookback` under the floor its own patterns need** (the message names the value and the floor); a malformed, un-decodable, duplicate-named, or uncompilable pattern |
| `-32006` | another waiting verb is already parked on this connection (§15.20) |
| `-32008` | the graph dropped the endpoint while this wait was parked — `teardown`, `load --replace`, `remove-node`. `data` carries the same scan and gap counters the result would have |

`-32008` is deliberately **not** a timeout. A timeout claims the stream was
watched for the whole deadline and stayed silent; this says the stream stopped
existing. Collapsing them would let a caller retry forever against an endpoint
that is gone — the same discrimination [`tap.closed`](#tapclosed-notification)
makes for a tap, applied to a wait.

There is no `tap.wait` notification, by design: a match delivered as a
notification would reach every [`subscribe`](#subscribe) consumer on the daemon,
so the result rides the parked request's own reply and nothing else (§15.56).

#### CLI

```console
$ serial-nexus-ctl tap-wait console --pattern login='login:' --timeout-ms 30000
console: matched "login" at offset 4211 (5183 byte(s) scanned)

$ serial-nexus-ctl tap-wait console --pattern prompt='[#$] $' --regex --replay --timeout-ms 5000
console: no match before the deadline (912 byte(s) scanned)

$ serial-nexus-ctl --json tap-wait console --pattern err=ERROR --timeout-ms 1000
{
  "bytes_scanned": 4096,
  "endpoint": "console",
  "epoch": 3,
  "from_offset": 131072,
  "gap_bytes": 0,
  "gaps": 0,
  "matched": null,
  "replay_scanned": 0,
  "timed_out": true
}
```

`--json` prints the whole result object — on both outcomes — *before* the process
exits with the verdict, so a caller gets the counters **and** the exit status.

`serial-nexus-ctl tap-wait` spells patterns as `<name>=<text>` and base64-encodes
them for you (so the CLI reaches only the bytes a shell argument can carry; the
RPC surface reaches all 256). **Its exit status is the verdict — `0` matched, `1`
the deadline expired, `2` the wait could not run** — which is `grep(1)`'s scheme,
for the tool that asks `grep`'s question:

```console
$ serial-nexus-ctl tap-wait console --pattern up='boot complete' --timeout-ms 60000 \
    && serial-nexus-ctl send console --line 'uname -a'
```

Match context goes to **stderr**, so stdout stays the one verdict line a script
reads — the same split `serial-nexus-ctl tap` makes for its gap notices.

#### Doing it client-side

The verb exists because this is easy to get subtly wrong, but the offset contract
below is public and a client that wants its own matcher is entitled to one. The
recipe, and the four places copies of it have gone wrong:

1. **`tap.open --replay`, then reassemble.** Concatenate `tap.data` payloads in
   arrival order; do **not** match per frame. A chunk boundary falls wherever the
   device happened to stop talking, so `login:` arrives as `lo` + `gin:` often
   enough to matter.
2. **Anchor on `from_offset`, and check `epoch`.** The first live frame carries
   `from_offset + replay_bytes`. If `epoch` differs from the one you stored, the
   offset space restarted beneath you (a hub rebuild) and your frontier means
   nothing — re-anchor rather than splice.
3. **Treat a non-zero `gap_before` as a discontinuity.** Bytes are missing before
   that chunk. Concatenating across it lets a pattern match bytes that were never
   adjacent, which is a false positive your log will never explain.
4. **Bound your buffer, and time out on your own clock.** Keep a fixed lookback
   window rather than the whole session, and remember that a `tap.open` that
   returns `replay_truncated > 0` means your channel was handed less ring than
   exists — which is the one gap no client-side matcher can close, because the
   daemon-side matcher reads the ring itself.

---

## `LockSnapshot`

The reportable view of one endpoint's write lock (§6) — observed state, disjoint
from configuration. It appears as the `.lock` (and `.channels[*].lock`) field of
a `state` node, and as the `lock` param of a `lock` notification.

| Field | Type | Description |
| --- | --- | --- |
| `arbitration` | string | `exclusive` or `free-for-all` — the endpoint's policy |
| `holder` | string \| null | the origin currently holding the lock, or null |
| `origins` | array of `OriginState` | every origin attached to this endpoint |
| `waiters` | array of string | origins parked in the FIFO queue, front = next to be granted |
| `last_steal` | object | *optional* — the most recent steal (omitted if none): `{ "from": <origin>, "by": <origin> }` |

Each `OriginState` in `origins`:

| Field | Type | Description |
| --- | --- | --- |
| `origin` | string | the origin display (a writer's node name) |
| `write_mode` | string | `never`, `on-demand`, or `held` |
| `holds_lock` | bool | whether this origin currently holds the lock |
| `purged` | integer | bytes discarded at each moment this origin's floor question settled (§6): its pre-grant backlog on acquire, and — for a pty origin — its un-delivered backlog at detach, which is both whatever remained in the console's kernel buffer and any payload the node's reader had already taken off it and could not hand on |
