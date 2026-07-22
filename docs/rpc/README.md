# serial_nexus control-plane RPC reference

The daemon `serialnexusd` is driven entirely over a Unix domain socket speaking
hand-rolled **JSON-RPC 2.0 over newline-delimited JSON** (design §10). This
directory documents that surface method by method.

> **This is the stable contract (§15.16).** The RPC method set and its JSON
> schemas are what `serialnexusd` guarantees. `serialnexusctl` is a thin
> presentation layer over these methods — its subcommand names, argument
> spellings, and rendered output may be renamed, regrouped, or composed from
> several RPCs without any daemon change. Each page below documents the RPC
> method first and notes the current `serialnexusctl` spelling second.

## Pages

| Page | Methods |
| --- | --- |
| [configuration.md](configuration.md) | `load`, `add-node`, `remove-node`, `dump` |
| [observation.md](observation.md) | `state`, `subscribe`, `info` (+ the `state` / `lock` notifications and `LockSnapshot`) |
| [arbitration.md](arbitration.md) | `lock`, `unlock`, `send` |
| [logging.md](logging.md) | `rotate` |
| [serial-signals.md](serial-signals.md) | `send-break`, `set-modem`, `pulse-dtr` |
| [lifecycle.md](lifecycle.md) | `teardown`, `shutdown` |

## Transport

The protocol is JSON-RPC 2.0, framed as **one JSON value per line** (a trailing
`\n` terminates each message). The daemon serves one task per connection;
mutations are serialized daemon-side, so many clients may connect at once.

* **Requests** are client → daemon and always carry an `id` (a string or a
  number) plus a `method` and optional `params`. An id-less client request is
  *not* part of this protocol — the daemon rejects it as an invalid request.
* **Responses** are daemon → client and carry exactly one of `result` or
  `error`, correlated to the request `id`. A response with neither or both is a
  protocol violation and never emitted.
* **Notifications** are id-less messages the daemon pushes to a connection
  *after* it has issued `subscribe` — see [observation.md](observation.md).
  Clients never send notifications.
* **`jsonrpc` must be `"2.0"`** on every message; any other version is rejected.
* **Batch arrays are rejected outright.** A line whose first non-space byte is
  `[` returns `-32600` (`"batch requests are not supported"`) — "deleting the
  specification's awkward corner" (§10). Send one request per line instead.
* A line that is not valid JSON returns `-32700`; a well-formed JSON value that
  is not a valid request object returns `-32600`. Both reply with `id: null`
  (JSON-RPC 2.0 §5) so the client's read stream never desyncs.

### Request / response shape

```json
{"jsonrpc":"2.0","id":1,"method":"state"}
```

```json
{"jsonrpc":"2.0","id":1,"result":{"nodes":[]}}
```

```json
{"jsonrpc":"2.0","id":1,"error":{"code":-32003,"message":"endpoint is locked by demux","data":{"held_by":"demux"}}}
```

## Socket path policy

The socket path is chosen by privilege, and is overridable on both binaries with
`--socket <PATH>` (§10):

| Condition | Default socket path |
| --- | --- |
| running as root (euid 0) | `/run/serialnexusd.sock` |
| `$XDG_RUNTIME_DIR` set and non-empty | `$XDG_RUNTIME_DIR/serialnexusd.sock` |
| otherwise | `/tmp/serialnexusd-<uid>.sock` |

`serialnexusctl` mirrors this policy exactly, so the CLI and a raw client find
the same socket without configuration.

**Socket permissions are the authorization model** — whoever can open the socket
owns every console. The daemon creates it mode `0600` (owner only) by default;
`serialnexusd --socket-group <GROUP>` chgrps it to that group and relaxes the
mode to `0660`. The stale-socket unlink dance runs at startup, and the socket is
removed on clean shutdown.

## Poking it by hand (nc + jq)

Any newline-delimited client works. With `nc -U` (Unix-socket mode) and `jq`:

```console
$ SOCK="${XDG_RUNTIME_DIR:-/tmp}/serialnexusd.sock"
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"state"}' | nc -U "$SOCK" | jq
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": { "nodes": [] }
}
```

`printf` closes `nc`'s send side after the single line; the daemon replies and
closes the connection, so this is a clean one-shot. `socat - UNIX-CONNECT:$SOCK`
works identically and is handy for interactive sessions. A `subscribe` request
instead holds the connection open and streams notification lines (feed them to
`jq -c` line by line).

## `serialnexusctl --json` is a pass-through

`serialnexusctl` renders results for humans by default (a table for `state`,
TOML for `dump`, one-line acknowledgements elsewhere). The global `--json` flag
prints the daemon's raw `result` value instead, unmodified:

```console
$ serialnexusctl --json state
{
  "nodes": []
}
```

This makes the CLI a drop-in JSON-RPC front end for scripts and agents that
prefer not to open the socket themselves. `--json` and `--socket` are global
flags and precede the subcommand.

## A note on version skew

Version skew between a client and the daemon degrades gracefully by
construction: a method this daemon does not implement returns the standard
`-32601` (method not found), telling a mismatched CLI exactly which operations
are missing (§15.16). The design's §10 verb list additionally names `connect`,
`disconnect`, and `set-attribute` (edge and attribute surgery); those are **not
implemented in this daemon build** and currently return `-32601`. Only the
methods documented on the pages above are live.

## Error codes

The standard JSON-RPC 2.0 codes, plus the daemon's application codes in the
reserved `[-32099, -32000]` range (§10). Application errors may carry a `data`
object with structured detail.

| Code | Name | Meaning |
| --- | --- | --- |
| `-32700` | parse error | the line was not valid JSON (`id: null`) |
| `-32600` | invalid request | not a valid request object, wrong `jsonrpc` version, or a rejected batch array (`id: null`) |
| `-32601` | method not found | unknown method — the graceful version-skew signal (§15.16) |
| `-32602` | invalid params | missing or malformed params for a known method |
| `-32603` | internal error | an unexpected daemon-side failure |
| `-32001` | load on non-empty graph | `load` without `replace` while a graph is already loaded |
| `-32002` | structural error | configuration failed validation; `data.errors` is the list of messages |
| `-32003` | locked | a contended `lock`/`send` was refused; `data.held_by` names the holder when known |
| `-32004` | has edges | `remove-node` refused because edges are still attached and `--cascade` was not given |
| `-32005` | device absent | `add-node` by raw path or serial number, but the device is not present so its identity cannot be captured (§12) |
