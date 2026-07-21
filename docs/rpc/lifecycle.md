# Lifecycle verbs

Whole-graph and whole-daemon lifecycle (§11). `teardown` empties the graph but
leaves the daemon running; `shutdown` stops the daemon itself.

Methods on this page: [`teardown`](#teardown), [`shutdown`](#shutdown).

---

## `teardown`

Tear down the entire graph (§11): release every node's environment (unlink PTY
symlinks, drop serial ports, flush and close log writers), close every endpoint
lock so parked `lock --wait`/`send` waiters leave with the defined error
(§6/§15.20), and clear the configuration. The daemon keeps running with an empty
graph, ready for a fresh `load`.

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
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"teardown"}' | nc -U "$SOCK" | jq .result
{
  "torn_down": 3
}
```

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
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"shutdown"}' | nc -U "$SOCK" | jq .result
{
  "shutting_down": true
}
```
