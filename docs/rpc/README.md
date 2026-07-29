# serial_nexus control-plane RPC reference

The daemon `serial-nexus-daemon` is driven entirely over a Unix domain socket speaking
hand-rolled **JSON-RPC 2.0 over newline-delimited JSON** (design §10). This
directory documents that surface method by method.

> **This is the stable contract (§15.16).** The RPC method set and its JSON
> schemas are what `serial-nexus-daemon` guarantees. `serial-nexus-ctl` is a thin
> presentation layer over these methods — its subcommand names, argument
> spellings, and rendered output may be renamed, regrouped, or composed from
> several RPCs without any daemon change. Each page below documents the RPC
> method first and notes the current `serial-nexus-ctl` spelling second.

## Pages

| Page | Methods |
| --- | --- |
| [configuration.md](configuration.md) | `load`, `add-node`, `remove-node`, `connect`, `disconnect`, `dump` |
| [observation.md](observation.md) | `state`, `subscribe`, `info`, `ports`, `tap.open`, `tap.close` (+ the `state` / `lock` / `tap.data` / `tap.closed` notifications and `LockSnapshot`) |
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
* **Notifications** are id-less messages the daemon pushes to a connection — see
  [observation.md](observation.md). `state` and `lock` go only to a connection
  that has issued `subscribe`; `tap.data` and `tap.closed` go to the connection
  that opened the tap, subscribed or not. Clients never send notifications.
* **`jsonrpc` must be `"2.0"`** on every message; any other version is rejected.
* **Batch arrays are rejected outright.** A line whose first non-space byte is
  `[` returns `-32600` (`"batch requests are not supported"`) — "deleting the
  specification's awkward corner" (§10). Send one request per line instead.
* A line that is not valid JSON returns `-32700`; a well-formed JSON value that
  is not a valid request object returns `-32600`. Both reply with `id: null`
  (JSON-RPC 2.0 §5) so the client's read stream never desyncs.
* **A request line is capped at 1 MiB.** A longer line — or an unterminated one
  that grows past the cap — is refused with `-32600` (`"request line exceeds the
  1048576-byte limit"`, `id: null`) and the connection is closed, so one client
  cannot grow the shared daemon's read buffer without bound. The cap sits far
  above any real verb, including a `load`'s inline graph JSON.
* **One in-flight *waiting* verb per connection.** `lock --wait` and `send` may
  suspend; while one is parked, a second request pipelined onto the same
  connection is answered `-32006` — with its own `id` — and the parked verb is
  left intact and still answers later. The wait is one *arm* of the connection's
  select rather than a pause on it, so `subscribe` and `tap.data` notifications
  keep flowing on that connection for the whole wait. Use a second connection for
  concurrent work. Reaching end-of-file on the read half is different: that *cancels* the
  waiting verb and closes the connection (§15.20), which is why a half-close is
  not a way to say "I am done sending" (see below).

### Request / response shape

```json
{"jsonrpc":"2.0","id":1,"method":"state"}
```

```json
{"jsonrpc":"2.0","id":1,"result":{"endpoints":[],"nodes":[],"taps":[]}}
```

```json
{"jsonrpc":"2.0","id":1,"error":{"code":-32003,"message":"endpoint is locked by demux","data":{"held_by":"demux"}}}
```

## Socket path policy

The socket path is chosen by privilege, and is overridable on both binaries with
`--socket <PATH>` (§10):

| Condition | Default socket path |
| --- | --- |
| running as root (euid 0) | `/run/serial-nexus-daemon.sock` |
| `$XDG_RUNTIME_DIR` set and non-empty | `$XDG_RUNTIME_DIR/serial-nexus-daemon.sock` |
| otherwise | `/tmp/serial-nexus-daemon-<uid>.sock` |

`serial-nexus-ctl` mirrors this policy exactly, so the CLI and a raw client find
the same socket without configuration.

**Socket permissions are the authorization model** — whoever can open the socket
owns every console. The daemon creates it mode `0600` (owner only) by default;
`serial-nexus-daemon --socket-group <GROUP>` chgrps it to that group and relaxes the
mode to `0660`. The stale-socket unlink dance runs at startup, and the socket is
removed on clean shutdown.

## Poking it by hand (nc + jq)

Any newline-delimited client works. With `nc -N -U` (Unix-socket mode, half-close
on stdin EOF) and `jq`:

```console
$ SOCK="${XDG_RUNTIME_DIR:-/tmp}/serial-nexus-daemon.sock"
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"state"}' | nc -N -U "$SOCK" | jq
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": { "endpoints": [], "nodes": [], "taps": [] }
}
```

**`-N` is not optional for a one-shot.** The daemon closes a connection when it
reads end-of-file, never after a reply — nothing in the protocol says a request
is the last one — so the client must be the side that half-closes. `printf`
closes `nc`'s *stdin*, and netcat-openbsd does not propagate that to the socket
without `-N`: the reply prints and then the command hangs forever.
`socat - UNIX-CONNECT:$SOCK` needs no such flag — it propagates the half-close by
default — and is handy for interactive sessions.

**Do not use `-N` with a verb that waits or streams.** Because end-of-file
cancels an in-flight waiting verb (§15.20), a `lock --wait` or a `send` that must
queue behind a holder gets *no reply at all* under `-N` — the half-close reads as
a disconnected client. Likewise `subscribe` holds the connection open to stream
notification lines (feed them to `jq -c` line by line), and `-N` ends it right
after the acknowledgement. For those, keep the write half open: plain `nc -U`, or
`socat -,ignoreeof UNIX-CONNECT:$SOCK`.

## `serial-nexus-ctl --json` is a pass-through

`serial-nexus-ctl` renders results for humans by default (a table for `state`,
TOML for `dump`, one-line acknowledgements elsewhere). The global `--json` flag
prints the daemon's raw `result` value instead, unmodified:

```console
$ serial-nexus-ctl --json state
{
  "endpoints": [],
  "nodes": [],
  "taps": []
}
```

The **error** path is machine-readable too: `--json` prints the daemon's JSON-RPC
error object to stdout under an `error` key — `{"error":{"code":…,"message":…,
"data":…}}` — and exits non-zero, so an agent never has to parse human text to
learn what failed. The key cannot collide with the success path, which is a raw
pass-through of the daemon `result` and is never printed alongside it. Without
`--json` the same failure goes to stderr as `error <code>: <message>` with any
`data` pretty-printed beneath it.

```console
$ serial-nexus-ctl --json load bad.toml; echo "exit=$?"
{
  "error": {
    "code": -32002,
    "data": { "errors": [ "…" ] },
    "message": "structural error: …"
  }
}
exit=1
```

This makes the CLI a drop-in JSON-RPC front end for scripts and agents that
prefer not to open the socket themselves. `--json` and `--socket` are global
flags and precede the subcommand.

## A note on version skew

Version skew between a client and the daemon degrades gracefully by
construction: a method this daemon does not implement returns the standard
`-32601` (method not found), telling a mismatched CLI exactly which operations
are missing (§15.16). The design's §10 verb list additionally names
`set-attribute` (attribute surgery on a live node); it is **not implemented in
this daemon build** and returns `-32601` — remove-and-re-add covers it (§14).
Only the methods documented on the pages above are live.

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
| `-32002` | structural error | configuration failed validation; `data.errors` is the list of messages, and `message` is always `structural error: <first>` |
| `-32003` | locked | a contended `lock`/`send` was refused; `data.held_by` names the holder when known |
| `-32004` | has edges | `remove-node` refused because edges are still attached and `--cascade` was not given |
| `-32005` | device absent | `add-node` by raw path or serial number, but the device is not present so its identity cannot be captured (§12) |
| `-32006` | waiting verb in flight | a request was pipelined behind an in-flight waiting verb on the same connection; §15.20 runs one at a time, and the wait, its taps and its subscription are all left intact |
| `-32007` | edge inbox full | `connect` refused: the target-facing endpoint has not drained the hostward receivers of its earlier edges yet. Transient — nothing changed, retry |

The `-32002` row's "always" is worth reading twice, because it was not always true:
every path that refuses a configuration — `load`, `add-node`, `connect`, and the
codec-attribute precheck — now builds that message through one constructor, so the code
carries exactly one message shape. Anything rendering `error.message` verbatim (the web
console's editor page does) therefore shows one sentence for every refusal that carries
this code, the unknown-codec precheck included. The prefix lives on `message` only,
never inside `data.errors`, so a client that wants the raw validator text reads the
array rather than stripping a string.

Every row above is generated from the same registry the daemon emits from
(`serial_nexus_rpc::error_code_registry`), and `serial-nexus-rpc`'s own
`docs_rpc_table_matches_the_registry` test asserts each row appears here **verbatim**
(§16.8). Editing a description in either place without matching the other is a test
failure, not a documentation drift — which is the point.
