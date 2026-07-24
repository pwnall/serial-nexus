# Observation verbs

Methods that report *observed* state — the environment-owned half of the strict
split (§15.8). Observed state is never persisted and, by construction, absent
from every configuration type: the fields here simply do not exist in `dump`.

Methods on this page: [`state`](#state), [`subscribe`](#subscribe),
[`info`](#info). This page also documents the [notification
stream](#notifications) `subscribe` opens and the [`LockSnapshot`](#locksnapshot)
shape shared by `state`, the `lock` notification, and the arbitration verbs.

---

## `state`

Report the observed status of every node — a point-in-time snapshot.

### Params

None.

### Result

| Field | Type | Description |
| --- | --- | --- |
| `nodes` | array | one object per node, in graph order |

Each node object carries:

| Field | Type | Description |
| --- | --- | --- |
| `name` | string | the node name |
| `status` | string | `active`, `waiting`, or `faulted` |
| `reason` | string | present only for `waiting`/`faulted`: why |
| *(node-type extras)* | varies | observed counters/details for the node kind (e.g. serial driver counters, log/leg/exec/codec internals, [map substitution counters](#map-node-state)) — observed-only, disjoint from config |
| `lock` | `LockSnapshot` | present on a single-endpoint node (e.g. serial): its host-facing endpoint's write lock |
| `channels` | object | present on a multi-endpoint node (e.g. codec): `channels[<channel>].lock` is each channel's `LockSnapshot` |

`status` is a tagged value: `waiting` and `faulted` add a `reason` string;
`active` has none. `waiting`/`faulted` are the same state family — an
environmental failure faults a node without removing it, and it heals on its own
(§15.8). The node-type extras are opaque observed detail and vary by kind; treat
them as informational.

### CLI

```console
$ serialnexusctl state          # one line per node: "<name>  <status> (reason)"
$ serialnexusctl --json state   # the raw {"nodes":[...]} object
```

### Errors

None beyond the transport-level codes.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"state"}' | nc -U "$SOCK" | jq .result
{
  "nodes": [
    {
      "name": "usb0",
      "status": "faulted",
      "reason": "ENOENT: /dev/ttyUSB0",
      "lock": {
        "arbitration": "exclusive",
        "holder": null,
        "origins": [],
        "waiters": []
      }
    }
  ]
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

A `raw.dropped_slow_consumer` count surfaces hostward bytes the upstream dropped
because the map's raw-side intake was full (§5 — the map falling behind, counted
where it happens, like a codec's multiplexed-side drop count). The map's mapped
endpoint's write lock appears in the top-level `lock` field, like any single
host-facing-endpoint node.

```console
$ serialnexusctl --json state | jq '.nodes[] | select(.name=="console")'
{
  "name": "console",
  "status": "active",
  "hostward": { "bytes_in": 4096, "bytes_out": 4103, "rules": { "lfcrlf": 7 } },
  "targetward": { "bytes_in": 12, "bytes_out": 12, "rules": { "lfcr": 1 } },
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
$ serialnexusctl subscribe             # one JSON notification per line, forever
$ serialnexusctl subscribe --count 3   # exit after 3 notifications
```

`serialnexusctl subscribe` swallows the acknowledgement and prints one JSON
notification object per line (a clean stream for `jq`), exiting after `--count`
of them or when the daemon closes the connection.

### Errors

None beyond the transport-level codes.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"subscribe"}' | nc -U "$SOCK"
{"jsonrpc":"2.0","id":1,"result":{"subscribed":true}}
{"jsonrpc":"2.0","method":"state","params":{"nodes":[ ... ]}}
{"jsonrpc":"2.0","method":"lock","params":{"endpoint":"usb0","lock":{ ... }}}
```

The first line is the correlated response; every line after it is an id-less
notification.

---

## `info`

Report the daemon's **capability surface** (§10, §15.26): its version, the wire
and envelope protocol versions, and the names of every codec it can instantiate.
Tools — and a version-skewed CLI — use it to *discover* what a daemon supports
rather than assume it, which matters because the daemon is embeddable: a
closed-source binary built on the `nexus-daemon` library registers its own codecs
(§15.26), and `info` is how the unchanged `serialnexusctl`, `nexus-sim`, and
`nexus-doctor` learn that daemon's codec set. The same list appears in an
unknown-codec load error's `data.available` (see
[configuration.md](configuration.md)), so a misconfiguration names the codecs
that *would* have worked.

Pure observation; touches no graph state.

### Params

None.

### Result

| Field | Type | Description |
| --- | --- | --- |
| `daemon_version` | string | the `nexus-daemon` library (engine) version — what determines wire and behavior compatibility |
| `wire_version` | integer | the daemon-to-daemon wire protocol version (§9) |
| `envelope_version` | integer | the exec-codec envelope version (§8/§15.15) — a codec author pins against this |
| `codecs` | array of string | the registered in-process codec names, sorted (the `exec` child-process codec is always available and is not listed here) |
| `instance` | integer | a per-boot nonce (§11.8). Tap byte offsets are only comparable within one daemon process; on restart the offsets reset to 0 and this value changes, so a client keyed on it (the web console's browser history, §17) detects the reset and starts fresh instead of splicing across it |

### CLI

```console
$ serialnexusctl info          # rendered: version, wire/envelope, codec list
$ serialnexusctl --json info   # the raw object
```

### Errors

None beyond the transport-level codes.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"info"}' | nc -U "$SOCK" | jq .result
{
  "daemon_version": "0.2.0",
  "wire_version": 1,
  "envelope_version": 1,
  "codecs": ["reference"],
  "instance": 12719384756019283746
}
```

---

## Notifications

Notifications are id-less messages — `{"jsonrpc":"2.0","method":…,"params":…}` —
delivered only on a subscribed connection. Two methods are emitted:

### `state` notification

A **full state snapshot**, identical in shape to the [`state`](#state) result
(`{ "nodes": [ … ] }`). It is emitted on a periodic tick (currently every
200 ms) and is the *floor* for observability — status transitions and counter
snapshots are always visible here even if a finer signal is missed. State
snapshots are cumulative, so a subscriber that falls behind and drops one loses
nothing.

```json
{"jsonrpc":"2.0","method":"state","params":{"nodes":[ ... ]}}
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
| `offset` | integer | the endpoint's monotonic hostward byte offset of this chunk's first byte (§11.8) — replay pieces carry their true stream offset, so a reconnecting client trims overlap and splices exactly. Offsets are comparable only within one daemon `instance` (see [`info`](#info)) |
| `data` | string | base64 of the chunk's bytes |

```json
{"jsonrpc":"2.0","method":"tap.data","params":{"tap":3,"offset":131072,"data":"aGVsbG8="}}
```

---

## Taps

A **tap** is a connection-scoped, read-only observer on a host-facing endpoint
(§17): it streams that endpoint's hostward bytes as `tap.data` notifications and
is torn down when its `tap.close` runs or the connection drops. Taps are *state*
— they never appear in configuration or `dump`.

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
| `from_offset` | integer | the endpoint offset this tap's stream begins at (§11.8): with a non-empty replay, the ring's oldest byte; otherwise the live edge, i.e. the offset the next `tap.data` will carry. A reconnecting client trims replay against the last offset it stored |

### `tap.close`

| Param | Type | Description |
| --- | --- | --- |
| `tap` | integer | the tap id to close (must be open on this connection) |

Result: `{ "closed": <tap id> }`.

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
| `purged` | integer | bytes discarded from this origin's pre-grant backlog on acquire (§6) |
