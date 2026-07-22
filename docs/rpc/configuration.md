# Configuration verbs

Methods that read or mutate the graph *configuration* — the operator-owned half
of the strict configuration/state split (§15.8). Every successful mutation here
is snapshotted to the daemon's state file so incremental surgery survives a
restart (§11/§15.9). Read-only `dump` and the observation verbs never touch it.

Methods on this page: [`load`](#load), [`add-node`](#add-node),
[`remove-node`](#remove-node), [`dump`](#dump).

`GraphConfig` and `NodeConfig` are the configuration types shared with `dump`;
they are exactly the load format. `dump` round-trips them, and `serialnexusctl`
renders them as TOML.

---

## `load`

Load a whole configuration onto the graph. **Structurally atomic** (§11): the
entire config is validated before anything is created, so a structural error
creates nothing. Accepted only on an *empty* graph unless `replace` is set,
which composes teardown-then-load so a full-file edit needs no manual teardown.
Environmental failures (a missing device) never fail the load — the node comes
up faulted/waiting and heals on its own (§15.8).

### Params

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `config` | `GraphConfig` | yes | the configuration to load (nodes + edges) |
| `replace` | bool | no (default `false`) | tear down any running graph first, then load |

### Result

| Field | Type | Description |
| --- | --- | --- |
| `loaded` | integer | number of nodes instantiated |

### CLI

```console
$ serialnexusctl load config.toml            # load onto an empty graph
$ serialnexusctl load config.toml --replace  # teardown-then-load
```

The CLI reads the TOML file, parses it into a `GraphConfig`, and sends it as the
`config` param; `--replace` maps to `replace: true`.

### Errors

* `-32001` — the graph is non-empty and `replace` was not set (`load requires an
  empty graph — teardown first (or use load --replace)`).
* `-32002` — structural validation failed; `data.errors` lists every message
  (duplicate node names, the three graph rules, illegal names/identities, codec
  tables). Caught *before* any teardown under `--replace`, so a bad config never
  destroys a good running graph. **An unknown codec name is structural too**
  (§8/§15.26): the error additionally carries `data.available`, the list of codecs
  this daemon *does* have — the same list the [`info`](observation.md#info) verb
  reports — so a misconfiguration names the codecs that would have worked.
* `-32602` — the params were missing, `config` was absent, or the config did not
  deserialize; also an unimplementable node kind.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"load","params":{"config":{"nodes":[],"edges":[]},"replace":false}}' \
    | nc -U "$SOCK" | jq .result
{
  "loaded": 0
}
```

---

## `add-node`

Add one node to a running graph (§11). The node arrives with **no edges** — its
own endpoints are wired self-contained. Validated against the same structural
rules as `load`, so a duplicate name or illegal identity creates nothing.

For a **serial** node the device is resolved to a canonical, structured identity
at add time and echoed back (§12): the captured identity replaces the operator
input in configuration, so `dump` round-trips it and the config survives a cold
start. Adding by raw path or serial number requires the device present *now*;
adding by an already-canonical `usb:`/`by-path:` identity never does.

### Params

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `node` | `NodeConfig` | yes | the single node to add |

### Result

| Field | Type | Description |
| --- | --- | --- |
| `added` | string | the node's name |
| `identity` | string | *(serial only)* the captured canonical identity |
| `description` | string | *(serial only)* human description, e.g. `FTDI FT232R, serial A6008isP, interface 0` |
| `kind` | string | *(serial only)* resolution kind label (`usb`, `by-path`, `raw`, …) |
| `resolved_path` | string \| null | *(serial only)* the current `/dev/tty*` path, or null if resolved while absent |
| `warning` | string | *(serial only, optional)* an instability warning (e.g. a raw-path add) |

The identity echo fields are present only when the added node is a serial node;
for other node kinds the result is just `{ "added": <name> }`.

### CLI

```console
$ serialnexusctl add-node one-node.toml
```

The file is a TOML configuration containing a single `[[node]]`; the CLI takes
its first node and sends it as the `node` param. On success it prints the name
and, for serial nodes, the bound `description`; a `warning` is printed to stderr.

### Errors

* `-32002` — structural validation of the candidate graph failed; `data.errors`
  lists the messages.
* `-32005` — a raw-path or serial-number add whose device is not present, so its
  identity cannot be captured. Add by a `usb:`/`by-path:` identity to configure
  it while absent (§12).
* `-32602` — missing `node`, a malformed node config, a malformed resolver
  input, or an unimplementable node kind.

---

## `remove-node`

Remove one node (§11). **Refused while any edge is attached** unless `cascade`
is set, which also removes those edges. Removal tears down the node's
environment (flushing a log queue within the bounded wait, §7.3), closes its
endpoint locks so parked `lock --wait`/`send` waiters leave with the defined
error (§6/§15.20), and prunes it from the wiring. Surviving neighbors self-heal.

### Params

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `node` | string | yes | the node name to remove |
| `cascade` | bool | no (default `false`) | also remove edges attached to the node |

### Result

| Field | Type | Description |
| --- | --- | --- |
| `removed` | string | the removed node's name |
| `cascaded_edges` | integer | number of attached edges removed (0 when none) |

### CLI

```console
$ serialnexusctl remove-node usb0
$ serialnexusctl remove-node usb0 --cascade
```

### Errors

* `-32004` — the node still has attached edge(s) and `cascade` was not set. The
  message names the count; retry with `--cascade`.
* `-32602` — missing or unknown `node`.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"remove-node","params":{"node":"usb0","cascade":true}}' \
    | nc -U "$SOCK" | jq .result
{
  "removed": "usb0",
  "cascaded_edges": 2
}
```

---

## `dump`

Emit the current configuration, in exactly the `load` format (§11). Configuration
only — everything observed lives behind [`state`](observation.md#state). This is
the migration story and the backup story; it round-trips through `load`.

### Params

None.

### Result

A `GraphConfig` object: `{ "nodes": [ … ], "edges": [ … ] }`.

### CLI

```console
$ serialnexusctl dump              # renders TOML
$ serialnexusctl --json dump       # raw GraphConfig JSON
```

The daemon returns structured JSON; `serialnexusctl dump` renders it as TOML
(the load format), while `--json` passes the JSON through unchanged.

### Errors

None beyond the transport-level codes.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"dump"}' | nc -U "$SOCK" | jq .result
{
  "nodes": [],
  "edges": []
}
```
