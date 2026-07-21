# Observation verbs

Methods that report *observed* state — the environment-owned half of the strict
split (§15.8). Observed state is never persisted and, by construction, absent
from every configuration type: the fields here simply do not exist in `dump`.

Methods on this page: [`state`](#state), [`subscribe`](#subscribe). This page
also documents the [notification stream](#notifications) `subscribe` opens and
the [`LockSnapshot`](#locksnapshot) shape shared by `state`, the `lock`
notification, and the arbitration verbs.

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
| *(node-type extras)* | varies | observed counters/details for the node kind (e.g. serial driver counters, log/leg/exec/codec internals) — observed-only, disjoint from config |
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
