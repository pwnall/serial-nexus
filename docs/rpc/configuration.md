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
* `-32002` — structural validation failed; `data.errors` lists every message, and
  the `message` repeats the first. Caught *before* any teardown under
  `--replace`, so a bad config never destroys a good running graph. See
  [what counts as structural](#what-counts-as-structural) below. **An unknown
  codec name is structural too** (§8/§15.26): the error additionally carries
  `data.available`, the list of codecs this daemon *does* have — the same list the
  [`info`](observation.md#info) verb reports — so a misconfiguration names the
  codecs that would have worked.
* `-32602` — the params were missing, `config` was absent, or the config did not
  deserialize (**including an unknown key or an unknown table**, see below); also
  an unimplementable node kind.

### What counts as structural

§11 wants the *entire* file judged before anything is created — and, under
`--replace`, before the running graph is torn down. Everything below is therefore
decided at validation time, not discovered at runtime.

**Deserialization is strict.** Every configuration type is
`deny_unknown_fields`, so an unknown key (`advertized_baud` for
`advertised_baud`) and an unknown table (`[[nodez]]` for `[[node]]`) are both
rejected outright rather than silently ignored. A silently-empty parse was the
one input that turned `--replace` into an unannounced `teardown` reporting
success. The one deliberately open table is a codec node's `attributes`, which §8
requires to stay opaque. Over the CLI these surface as a TOML parse error naming
the offending field before anything is sent; over the wire they are `-32602`,
with the message *invalid config: unknown field `nodez`, expected `node` or
`edge`*. `serialnexusctl` additionally refuses a file whose text is non-empty but
which parses to *no* nodes and no edges — an empty graph is legal over RPC
(`teardown` persists one), but a file the operator wrote and that yielded nothing
is a mistake worth naming, not an instruction to empty the graph.

**Numeric fields are range-checked, inclusively.** Two of these were reachable
process-killers rather than merely silly values, which is why the whole class is
validated rather than trusted:

| Field | Range | Why bounded |
| --- | --- | --- |
| `replay_ring` | `0 ..= 16777216` (16 MiB) | the ring allocates lazily on the first hostward byte, so an unbounded value loads cleanly and then aborts the process — on a configuration `load` has already persisted, so the daemon crash-loops |
| `hostward_buffer` | `1 ..= 65536` chunks | `0` is a rendezvous channel that drops nearly all hostward output; the depth is handed straight to a bounded tokio channel, which panics above its permit ceiling |
| `baud` | `1 ..= 4294967295` | no upper bound by design (§7.1/§13 buy nonstandard rates deliberately), but `B0` means *hang up the line* |
| `rotation_padding` | `0 ..= 20` | the rotation counter is a `u64`, so beyond twenty digits the padding is pure filename noise |
| `reconnect_initial_ms`, `reconnect_max_ms`, `idle_release_ms` | `0 ..= 3600000` (1 h) | a leg that waits longer than an hour to retry, or to release an idle implicit lock, is indistinguishable from a dead one |

The message names the node, the field, the value and the bound: `node "p"
declares hostward_buffer = 70000, above the maximum 65536 (a numeric field is
range-checked before anything is created, §11)`.

**Two edge rules that depend on node kinds rather than facings.**

* An edge into a codec's or exec's **multiplexed** endpoint (its default,
  empty-named endpoint) must be `write_mode = "held"` or `"never"`. That side's
  targetward pump is gated by the lock's held reclaim, which only ever grants a
  *held* origin, so any other mode — including the generic `on-demand` default an
  omitted `write_mode` produces — parks on its first chunk forever while `send`
  reports success. Bytes accepted, acknowledged and lost is what §5 forbids
  outright, so the operator is told instead of left to hunt a stall.
* At most **one effectively-`held` edge per host-facing endpoint**. `held` means
  acquire-on-attach and held indefinitely (§6), which two origins cannot both
  have: one wins arbitrarily, the loser can never write, never joins the waiter
  queue, and appears nowhere in `state`. "Effectively" counts the runtime
  promotions — notably a [map's raw edge](#the-map-node--character-mapping-78) —
  so two maps attached to one upstream with `write_mode` written nowhere are
  caught too. An endpoint whose node is `arbitration = "free-for-all"` is exempt:
  there is no lock there, so two held writers are a deliberate choice.

**Names.** A node name or channel identity may not contain `/`, may not be empty,
may not be **whitespace-only** (a node called `" "` renders indistinguishably
from every other blank name in `state`, `dump`, and every message that quotes
it), and may not exceed 256 bytes — an identity rides in every envelope frame
header, so an oversize one leaves no room for payload (§3, §9).

**Per-kind attribute checks** the topology model cannot make: a leg's
transport/address (loopback-only without `insecure_bind`), a leg with no channels
or an empty channel identity, and a map's mapping names (an unknown one is
structural, naming the offender).

**`faces = "target"` on a *serial* node is refused.** §7.1 describes the role —
the port used as an output leg toward another machine's tools — but no wiring for
it exists, so such a node would open the device, take `TIOCEXCL`, and be attached
to nothing: a port held hostage with no data path and no diagnostic. It is
deferred work (§14) and says so: `serial node "s" declares faces = "target",
which is not implemented …`.

### Example

`GraphConfig` is the TOML load format expressed as JSON, so its arrays are named
`node` and `edge` — the TOML table names — not `nodes`/`edges`, and an empty one
is omitted rather than sent as `[]`:

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"load","params":{"config":{"node":[{"type":"pty","name":"p","path":"/tmp/p"}]},"replace":false}}' \
    | nc -N -U "$SOCK" | jq .result
{
  "loaded": 1
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

The file is a TOML configuration containing **exactly one `[[node]]` and no
`[[edge]]`**; the CLI sends that node as the `node` param. On success it prints
the name and, for serial nodes, the bound `description`; a `warning` is printed
to stderr.

A file carrying more is an **error** — nothing is sent — naming what it found:

```console
$ serialnexusctl add-node two-nodes.toml
Error: two-nodes.toml: add-node takes a single [[node]] and no [[edge]], but this
file has 2 node(s) and 1 edge(s) — nothing was added. Use `serialnexusctl load`
(or `load --replace` over a running graph) for a multi-node configuration: edges
cannot be added afterwards, because `connect` is deferred (§14).
```

Taking the first node and discarding the rest would be bad enough for a dropped
`[[node]]`; a dropped `[[edge]]` is *unrecoverable*, because `connect` is
deferred (§14) and no verb could add it afterwards.

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
    | nc -N -U "$SOCK" | jq .result
{
  "removed": "usb0",
  "cascaded_edges": 2
}
```

Any tap open on a removed node's endpoints is told its endpoint is gone with a
[`tap.closed`](observation.md#tapclosed-notification) notification carrying
`reason: "endpoint removed"`, rather than going silently dead.

---

## `dump`

Emit the current configuration, in exactly the `load` format (§11). Configuration
only — everything observed lives behind [`state`](observation.md#state). This is
the migration story and the backup story; it round-trips through `load`.

### Params

None.

### Result

A `GraphConfig` object: `{ "node": [ … ], "edge": [ … ] }` — the arrays carry the
TOML table names, and an empty one is **omitted**, so an empty graph dumps as
`{}`. This is exactly what `load` accepts back.

### CLI

```console
$ serialnexusctl dump              # renders TOML
$ serialnexusctl --json dump       # raw GraphConfig JSON
```

The daemon returns structured JSON; `serialnexusctl dump` renders it as TOML
(the load format), while `--json` passes the JSON through unchanged. Defaults are
materialized on the way out — a node dumps with every attribute the daemon
actually applied, not just the ones the operator typed — so the dump is a
complete, re-loadable statement of intent.

### Errors

None beyond the transport-level codes.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"dump"}' | nc -N -U "$SOCK" | jq .result
{
  "node": [
    {
      "type": "pty",
      "name": "p",
      "path": "/tmp/p",
      "advertised_baud": 115200,
      "hostward_buffer": 32
    }
  ]
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
if you prefer (the shipped example does). Because that promotion is what
[the one-held-edge rule](#what-counts-as-structural) counts, two maps cannot share
one upstream endpoint even with `write_mode` written nowhere — that is a structural
error, not a silently starved second map. An explicit `never` instead makes a
**read-only/display map** with no targetward path: writes toward it are drained
and counted as `targetward.discarded_no_raw_edge` rather than left to close the
channel, so `send console` still reports success while the bytes go nowhere, and
the pty or other writer upstream of the map keeps working. `send` at the map's endpoint
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
| `spchex` | DEL (`0x7f`) and every control byte `0x00..=0x1f` **except** TAB/LF/CR | `[xx]` | 4 |
| `tabhex` | TAB (`0x09`) | `[09]` | 4 |
| `crhex` | CR | `[0d]` | 4 |
| `lfhex` | LF | `[0a]` | 4 |
| `8bithex` | any `0x80..=0xff` | `[xx]` | 4 |
| `nrmhex` | any printable `0x20..=0x7e` | `[xx]` | 4 |

The hex-display form is `[` + two lowercase hex digits + `]`, matching picocom's
`map2hex`. `spchex` is picocom's *special-character* class, read off upstream's
`do_map` (`c == '\x7f' || (c < 0x20 && c != '\x09' && c != '\x0a' && c != '\x0d')`)
— TAB, LF and CR are excluded because they have rules of their own. It is the rule
to reach for when hunting a stray `0x00` or `0x1b`. **A space is hexed by `nrmhex`**,
which covers every printable ASCII byte including `0x20` (picocom's `0x20..=0x7e`).
With this set the hex family partitions the whole byte space — `spchex` ∪ `tabhex`
∪ `crhex` ∪ `lfhex` ∪ `nrmhex` ∪ `8bithex` = `0x00..=0xff` — and `spchex` overlaps
exactly two non-hex rules, `bsdel` (BS) and `delbs` (DEL), where first-match-wins
means the list order decides. Output is bounded at `k ×` input, where `k` is the
largest expansion among the active rules (the right-hand column), which keeps the
§5 interior holdover bounded across the map.

> **Behavior change: `spchex` used to mean SPACE.** The map node shipped with
> `spchex` implemented as `b == 0x20`, i.e. SPACE → `[20]`, which is what the
> table above used to say. That was wrong against picocom, and doubly wrong in
> effect: an operator who wrote `hostward = ["spchex"]` to reveal stray control
> bytes got every space rewritten as `[20]` — corrupting the console, its logs,
> its taps and the web view — while the bytes being hunted still passed through
> invisibly, since **no rule in the vocabulary could render a control byte at
> all**. Correcting it changes the bytes an existing `spchex` configuration
> produces: a graph that used `spchex` for its old SPACE behavior must now say
> `nrmhex` (which also hexes the rest of printable ASCII — the vocabulary has no
> space-only rule), and a graph that used it for its documented purpose starts
> working.

Per-rule and per-direction substitution counters are observed state, reported by
[`state`](observation.md#map-node-state).
