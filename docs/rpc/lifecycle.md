# Lifecycle verbs

Whole-graph and whole-daemon lifecycle (§11). `teardown` empties the graph but
leaves the daemon running; `shutdown` stops the daemon itself.

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
| `torn_down` | integer | number of nodes torn down |

### CLI

```console
$ serialnexusctl teardown
```

### Errors

None beyond the transport-level codes.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"teardown"}' | nc -N -U "$SOCK" | jq .result
{
  "torn_down": 3
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
$ serialnexusctl shutdown
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
