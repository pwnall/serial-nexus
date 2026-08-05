# Lifecycle verbs

Whole-graph and whole-daemon lifecycle (§11). `teardown` empties the graph but
leaves the daemon running; `shutdown` stops the daemon itself. Both close every
endpoint lock outright, so a parked waiter always leaves by the torn-down arm
(`-32003`, `endpoint behind origin "p1" was torn down while waiting`, or
`endpoint "usb0" was torn down while sending`) — never by the detached arm that
[`disconnect`/`remove-node`](configuration.md#remove-node) can produce, which is
a statement about one writer and not about the endpoint.

Methods on this page: [`teardown`](#teardown), [`shutdown`](#shutdown).

---

## `teardown`

Tear down the entire graph (§11): release every node's environment (unlink PTY
symlinks, drop serial ports, flush and close log writers), close every endpoint
lock so parked `lock --wait`/`send` waiters leave with the defined error
(§6/§15.20), tell every open [tap](observation.md#taps) its endpoint is gone, and
clear the configuration. The daemon keeps running with an empty graph, ready for
a fresh `load`.

Every node is signalled to stop *before* any of them is joined, so a wedged
environment (a log node on a hung mount) costs the runtime thread one bounded
flush wait in total rather than one per node (§7.3).

### Params

None.

### Result

| Field | Type | Description |
| --- | --- | --- |
| `torn_down` | integer | how many nodes were removed |
| `discarded_at_teardown` | integer | targetward bytes the nodes were still holding for consumers that went away with them, summed across the graph — what tearing down cost, as against `torn_down`'s count of what it removed (§5: loss is always visible). Nonzero only where a node with a targetward queue had a non-empty one: `map`, `codec` and `exec` on their host-facing queues, `serial` on the backlog a `waiting` device accumulates, and `leg` on its per-channel queues. For `exec` the figure is a **floor** rather than a total — its per-channel forwarders feed a second internal merge stage this handle does not reach — and `pty` and `log` contribute nothing because they own no queue of this shape. See [`remove-node`](configuration.md#remove-node) for the single-node form and [observation.md](observation.md) for the per-node counter. Always present, `0` included |

Both fields are always present. `discarded_at_teardown` is a *loss* figure and must not
be added to the `purged_bytes` [`remove-node`](configuration.md#remove-node) reports
alongside it there: they name different losses, and the reason is spelled out on that
page.

### CLI

```console
$ serial-nexus-ctl teardown
```

### Errors

None beyond the transport-level codes.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"teardown"}' | nc -N -U "$SOCK" | jq .result
{
  "torn_down": 3,
  "discarded_at_teardown": 0
}
```

Every open tap receives a terminal
[`tap.closed`](observation.md#tapclosed-notification) notification on its own
connection, with `reason: "teardown"` here and `reason: "graph replaced"` when
the teardown is the one `load --replace` composes. The client's connection,
subscription and other taps are unaffected; only the taps whose endpoints went
away end. A subsequent `tap.close` for one of those ids is refused rather than
reporting a success that closed nothing.

> `teardown` is also composed into `load --replace` (teardown-then-load, §11), so
> a full-file edit needs no manual teardown. Note that a clean SIGTERM shutdown
> tears nodes down through a separate path that **preserves** the persisted
> configuration for the next start, rather than snapshotting an empty graph.

---

## `shutdown`

Ask the daemon to shut down. The reply is sent before the daemon stops; the
graph is then torn down cleanly on the way out (the socket is unlinked, PTY
symlinks removed, ports dropped), and the persisted configuration is preserved
for the next start.

### Params

None.

### Result

| Field | Type | Description |
| --- | --- | --- |
| `shutting_down` | bool | always `true` |

### CLI

```console
$ serial-nexus-ctl shutdown
```

### Errors

None beyond the transport-level codes.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"shutdown"}' | nc -N -U "$SOCK" | jq .result
{
  "shutting_down": true
}
```
