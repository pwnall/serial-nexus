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

---

## The `map` node — character mapping (§7.8)

A `map` node is a per-console **character-mapping transform**: picocom's
`--imap`/`--omap` byte mappings made a place in the graph instead of a flag on
every terminal, log, and remote session (design §7.8, §15.33). It is deliberately
*not* a codec — no channels, no frames — just a stateless byte-to-byte-sequence
substitution.

**Shape and addressing.** A map has two endpoints. Its **mapped** side is the
host-facing default endpoint, addressed by the bare node name and carrying the
standard write-lock, fan-out, tap, and replay-ring machinery — so consumers (PTY,
log, leg, tap, web console) attach here and see the corrected stream. Its **raw**
side is the target-facing endpoint, addressed as `node/raw`; the upstream endpoint
whose bytes it maps attaches there. Because both the upstream endpoint and the map
carry a default replay ring, a **raw view and a mapped view coexist** by default.

```toml
[[node]]
type = "map"
name = "console"
hostward = ["lfcrlf"]   # device -> consumers (picocom --imap)
targetward = ["lfcr"]   # consumers -> device (picocom --omap)
# arbitration (default "exclusive") and replay_ring (default 65536) apply to the
# mapped host-facing endpoint, exactly as for any other host endpoint.

[[edge]]                 # the serial feeds the map's RAW side, held (see below)
a = "usb0"
b = "console/raw"
write_mode = "held"

[[edge]]                 # the map's MAPPED side fans out to consumers
a = "console"
b = "some-pty"
```

**Direction names, not flow names.** The two lists are named by the *direction of
the bytes they transform*, never the flow-relative input/output vocabulary rejected
everywhere else in the schema (§15.3): `hostward` is picocom's `--imap` (device
toward consumers), `targetward` is `--omap` (consumers toward device). An empty (or
omitted) list is the identity.

**First match wins.** Within a direction the rules are an *ordered* list; for each
input byte, the **first** rule whose match-set contains it fires, and the rest are
shadowed. Order therefore resolves conflicts deterministically —
`["igncr", "crlf"]` deletes CR, `["crlf", "igncr"]` translates it. (This differs
from picocom, which applies a fixed internal priority; here the operator's list
order *is* the priority.) An **unknown mapping name is a structural error** naming
the offender — caught before any teardown under `--replace`, so a bad name never
destroys a good graph.

**The held edge and steal-to-bypass.** The map's edge into the upstream endpoint
**defaults to `held`** — the demux's pattern with softer stakes: bypassing a map is
not corruption, merely unmapped. Because the generic edge default is `on-demand`
(which a held-origin transform pump cannot drive), an **omitted or `on-demand`**
`write_mode` on the raw edge is treated as `held` at runtime; write `held` explicitly
if you prefer (the shipped example does). An explicit `never` instead makes a
**read-only/display map** with no targetward path. `send` at the map's endpoint
(`send console`) speaks the mapped stream; a `send --steal` at the *upstream*
endpoint (`send usb0 --steal`) ousts the map transiently and injects raw bytes
verbatim, the map reclaiming its held edge afterward (§6 held priority).

### The mapping vocabulary (picocom's)

| Name | Matches | → Output | Expansion |
| --- | --- | --- | --- |
| `crlf` | CR (`0x0d`) | LF (`0x0a`) | 1 |
| `crcrlf` | CR | CR LF | 2 |
| `igncr` | CR | *(deleted)* | 0 |
| `lfcr` | LF (`0x0a`) | CR | 1 |
| `lfcrlf` | LF | CR LF | 2 |
| `ignlf` | LF | *(deleted)* | 0 |
| `bsdel` | BS (`0x08`) | DEL (`0x7f`) | 1 |
| `delbs` | DEL (`0x7f`) | BS (`0x08`) | 1 |
| `spchex` | SPACE (`0x20`) | `[20]` | 4 |
| `tabhex` | TAB (`0x09`) | `[09]` | 4 |
| `crhex` | CR | `[0d]` | 4 |
| `lfhex` | LF | `[0a]` | 4 |
| `8bithex` | any `0x80..=0xff` | `[xx]` | 4 |
| `nrmhex` | any printable `0x20..=0x7e` | `[xx]` | 4 |

The hex-display form is `[` + two lowercase hex digits + `]`, matching picocom's
`map2hex`. `nrmhex` covers every printable ASCII byte **including space** (picocom's
`0x20..=0x7e`); `spchex`/`tabhex` exist to hex *only* space or tab. Output is bounded
at `k ×` input, where `k` is the largest expansion among the active rules (the
right-hand column), which keeps the §5 interior holdover bounded across the map.

Per-rule and per-direction substitution counters are observed state, reported by
[`state`](observation.md#map-node-state).
