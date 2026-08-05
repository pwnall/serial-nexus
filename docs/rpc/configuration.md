# Configuration verbs

Methods that read or mutate the graph *configuration* — the operator-owned half
of the strict configuration/state split (§15.8). Every successful mutation here
is snapshotted to the daemon's state file so incremental surgery survives a
restart (§11/§15.9). Read-only `dump` and the observation verbs never touch it.

Methods on this page: [`load`](#load), [`add-node`](#add-node),
[`remove-node`](#remove-node), [`connect`](#connect), [`disconnect`](#disconnect),
[`dump`](#dump).

`GraphConfig` and `NodeConfig` are the configuration types shared with `dump`;
they are exactly the load format. `connect`/`disconnect` take an `EdgeConfig` —
the same `[[edge]]` table `load` accepts — for the same reason. `dump` round-trips
them, and `serial-nexus-ctl` renders them as TOML.

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
$ serial-nexus-ctl load config.toml            # load onto an empty graph
$ serial-nexus-ctl load config.toml --replace  # teardown-then-load
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
  codecs that would have worked. That list includes the built-in
  `exec` (§7.6), which has no registry entry but is a legal `codec = …` value: it
  is a child *process* rather than an in-process codec and is routed before the
  registry is consulted, an implementation fact that has no business leaking into
  the answer a `codec = "exe"` typo gets.
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
`edge`*. `serial-nexus-ctl` additionally refuses a file whose text is non-empty but
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
| `mode` (pty) | `0o600 ..= 0o777` (384 ..= 511) | a permission mode is nine bits, so the ceiling names the whole family of three-digit *decimal* typos of an octal one: `mode = 666` is 0o1232 — owner `-w-` — which used to load and then fault the node with an `EACCES` that never mentioned the mode. The floor is the daemon's own access: a pty's setup chmods the slave and then *opens* it to prime the session, so a mode denying the owner read+write faults by construction, and it catches what the ceiling misses by arithmetic accident (`mode = 154` is 0o232, no owner read, and sits inside 0o777) |
| `rotation_padding` | `0 ..= 20` | the rotation counter is a `u64`, so beyond twenty digits the padding is pure filename noise |
| `reconnect_initial_ms`, `reconnect_max_ms`, `idle_release_ms` | `0 ..= 3600000` (1 h) | a leg that waits longer than an hour to retry, or to release an idle implicit lock, is indistinguishable from a dead one |
| `restart_backoff_ms` (exec codec) | `0 ..= 3600000` (1 h) | the same bound as the leg timers, and the same reasoning, applied to the one timer that lives inside a codec's opaque `attributes` table rather than in the node schema — `restart_backoff_ms = 86400000` used to load clean and then never respawn a crashed child for the rest of the daemon's life, with the node reporting that it was retrying. Checked in the codec's own `parse_attributes`, which both `load` and `add-node` call before anything is created, so the §11 atomicity guarantee is unchanged |

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
  outright, so the operator is told instead of left to hunt a stall. The whole
  hazard presumes a lock upstream, so — exactly as in the sibling rule below — an
  endpoint whose node is `arbitration = "free-for-all"` is exempt: with no lock
  there, `held` and `on-demand` are behaviourally identical, no writer can park,
  and the refusal would reject a graph that runs while citing a stall that cannot
  happen on it.
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

A `/dev` path that is itself a **symlink** — `/dev/serial/by-id/usb-FTDI_…-if00-port0`,
`/dev/serial/by-path/…` — is followed to its device node before the identity is
derived, so the most canonical spelling an operator has captures the same `usb:`
identity the device node would. A link name is not a sysfs device name, and
deriving from the literal input degraded that input all the way to `raw:`,
carrying the "not stable across reboots" warning — which is precisely backwards
for a by-id path. A device with genuinely no identity, reached through a link,
stores the canonical device-node path in its `raw:` form rather than the link
name: the link may be gone next boot, the node will not. Two cases deliberately
keep the operator's own spelling instead — a link that cannot be resolved at all
(a race with an unplug) and one whose target escapes the daemon's `--dev-root`,
since the resolver never binds a device outside the tree it was pointed at.

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
| `resolved_path` | string \| null | *(serial only)* the current `/dev/tty*` path, or null if resolved while absent — the canonical device node, so a symlinked input is echoed back as what it points at |
| `warning` | string | *(serial only, optional)* an instability warning (e.g. a raw-path add) |

The identity echo fields are present only when the added node is a serial node;
for other node kinds the result is just `{ "added": <name> }`.

### CLI

```console
$ serial-nexus-ctl add-node one-node.toml
```

The file is a TOML configuration containing **exactly one `[[node]]` and no
`[[edge]]`**; the CLI sends that node as the `node` param. On success it prints
the name and, for serial nodes, the bound `description`; a `warning` is printed
to stderr.

A file carrying more is an **error** — nothing is sent — naming what it found:

```console
$ serial-nexus-ctl add-node two-nodes.toml
Error: two-nodes.toml: add-node takes a single [[node]] and no [[edge]], but this
file has 2 node(s) and 1 edge(s) — nothing was added. Use `serial-nexus-ctl load`
(or `load --replace` over a running graph) for a multi-node configuration, or add
the nodes one at a time and wire them with `connect`.
```

Taking the first node and discarding the rest would silently drop configuration
the operator wrote, which is bad enough for a `[[node]]`; refusing names what it
found instead. (A dropped `[[edge]]` used to be *unrecoverable* — `connect` was
deferred. Since §15.35 it is recoverable, but a silent drop is still a silent
drop.)

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
environment (flushing a log queue within the bounded wait, §7.3), leaves every
lock it touched cleanly so no parked `lock --wait`/`send` waiter is stranded
(§6/§15.20), and prunes it from the wiring. Surviving neighbors self-heal.

Which *defined* error a waiter gets depends on which side of the removal it was
on, and the two are worth telling apart. A waiter parked on a lock belonging to
the **removed node's own** endpoint gets `-32003` (`endpoint behind origin "p1"
was torn down while waiting`, or `endpoint "usb0" was torn down while sending`) —
the endpoint it wanted no longer exists. A waiter whose *origin* was on the
removed node but whose endpoint **survives** is unregistered from that surviving
lock and gets `-32602` (`origin "p1" was detached from its endpoint while
waiting`): the endpoint is still there and still writable, this writer simply is
not attached to it any more. That second one used to borrow the `write=never`
sentence, which is a claim about configuration and was never true here — the
origin's declared mode is untouched by a removal.

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
| `released_locks` | integer | how many of those cascaded edges held their endpoint's write lock and released it on the way out. `0` unless `cascade` removed a lock-holding writer |
| `purged_bytes` | integer | un-flushed targetward bytes discarded with the cascaded origins, summed — the same fact [`disconnect`](#disconnect) reports for one edge, and honest for the same reason (§5: loss is always visible). Nonzero only where a **pty** origin was cascaded |
| `discarded_at_teardown` | integer | targetward bytes the **removed node itself** was still holding for a consumer that is going away with it: what was queued for its own pump and will now never be delivered. Nonzero only for a kind that owns a targetward queue — `map`, `codec` and `exec` on their host-facing queues, `serial` on the backlog a `waiting` device accumulates, and `leg` summed over its per-channel queues; for `exec` it is a **floor** rather than a total, and `pty`/`log` own no queue of this shape, so their `0` is structural. See [observation.md](observation.md) for the per-node and per-channel counters and for exactly what the `exec` floor excludes. Always present, `0` included |

The last three rows exist because the identical edge removal used to be loud through
`disconnect` and mute through this verb: an operator cascading a lock-holding writer
changed who may write, and one cascading a writer with bytes queued lost them by design.
Both are reported rather than done silently (review 37 `37-LIFE-1`).

`purged_bytes` and `discarded_at_teardown` are **different losses and must not be
added together as one number**. The first is §6's deliberate purge *at the edges* —
bytes an origin had offered while the floor question was unsettled. The second is what
the node's own pump had already accepted and had not yet delivered when the node
stopped existing. Until 2026-08-04 only the first was reported and the second was
silent: a `remove-node --cascade` on a saturated map destroyed 808 448 bytes and
answered `purged_bytes: 0` with every node counter reading `0`
(`docs/implementation-notes.md` §3.31). The same figure appears on the node's own
`state` while it still exists — see
[observation.md](observation.md) — which is where a `disconnect` that leaves the node
alive reports it, since there the bytes are backlog the node may still deliver rather
than loss.

### CLI

```console
$ serial-nexus-ctl remove-node usb0
$ serial-nexus-ctl remove-node usb0 --cascade
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
  "cascaded_edges": 2,
  "released_locks": 1,
  "purged_bytes": 41
}
```

Any tap open on a removed node's endpoints is told its endpoint is gone with a
[`tap.closed`](observation.md#tapclosed-notification) notification carrying
`reason: "endpoint removed"`, rather than going silently dead.

---

## `connect`

Attach **one edge** to a running graph (§15.35). Reshaping a graph no longer means
either remove-and-re-add or a `load --replace` outage.

The operation is deliberately not a special case. The *candidate* graph —
everything currently configured plus this one edge — runs the same
`GraphConfig::validate` a `load` runs, so §4's three graph rules, §6's mux-edge
write mode and held-origin uniqueness, name legality and the numeric ranges all
apply here, and a violation creates nothing. On success the producer's live
fan-out gains a sink and the consumer's edge slot is filled *under running tasks*,
so nothing restarts: a mid-stream connect joins the stream at the join point, and
a serial node keeps its `TIOCEXCL` and its DTR line untouched.

Orientation comes from the endpoints' facings (§15.3), not from the argument
order: `connect a b` and `connect b a` are the same edge. The result reports the
resolved orientation, host end first.

### Params

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `a` | `EndpointAddr` | yes | one endpoint's display address (`usb0`, `mux/console`, `cons/raw`) |
| `b` | `EndpointAddr` | yes | the other endpoint |
| `write_mode` | string | no (default `on-demand`) | `never`, `on-demand` or `held` (§6) |

The params are the same `[[edge]]` table `load` accepts, so an operator writes one
thing; a nested `{"edge": {...}}` is accepted too. `write_mode: null` means
"unspecified" (the default), for clients that always send the field.

### Result

| Field | Type | Description |
| --- | --- | --- |
| `connected.a` | string | the **host-facing** end, as resolved |
| `connected.b` | string | the **target-facing** end |
| `write_mode` | string | the *effective* mode (§6) — a log target is forced to `never`, a map's `raw` endpoint is promoted to `held` (§7.3/§7.8) |
| `consumer_live` | bool | whether the target endpoint has a running pump to receive on. `false` is reachable and is **not** an error: a pty whose setup faulted, or a `faces = "host"` codec/exec whose driver is deferred (§14), takes the edge and carries nothing — the same dead edge `load` would have produced for the same graph (§15.8). Reported so "connected" and "no bytes will ever flow" do not look alike |

Note the result reports the **effective** mode while `dump` round-trips the
**declared** one. That is the §16 rule: one implementation computes the
promotions, and both the validator and the data plane consult it.

### CLI

```console
$ serial-nexus-ctl connect usb0 console
$ serial-nexus-ctl connect usb0 mux --write-mode held
```

### Errors

* `-32002` — structural validation of the candidate graph failed; `data.errors`
  lists every message, and the first is in the error's own `message`. This covers
  every rule `load` enforces, **including reference integrity**: an unknown node,
  an unknown endpoint on a known node, a same-facing pair, a target endpoint that
  would have two edges (which is also how re-adding an existing edge is refused),
  a cycle, a codec's multiplexed edge that is neither `held` nor `never`, and a
  second `held` origin on one endpoint.
* `-32602` — the *params* are malformed, before any graph is considered: a missing
  `a`/`b`, an unknown `write_mode` value, or an unknown key (the edge schema is
  `deny_unknown_fields`, §11).
* `-32007` — **transient**, and the only error here that is a property of the
  moment rather than of the request: the target-facing endpoint has not yet drained
  the hostward receivers of the edges attached before this one, so the attachment
  did nothing and the graph is exactly as it was. Reachable only from a pipelined
  burst of edge surgery on one endpoint faster than its pump is scheduled; retry.
  It is deliberately *not* reported as `consumer_live: false`, which names a node
  that cannot receive at all — a permanent property that would send an operator to
  inspect a healthy node while the configured edge stayed dead (37-DATA-1).

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"connect","params":{"a":"usb0","b":"console"}}' \
    | nc -N -U "$SOCK" | jq .result
{
  "connected": { "a": "usb0", "b": "console" },
  "write_mode": "on-demand"
}
```

---

## `disconnect`

Remove **one edge** from a running graph (§15.35). The reverse of `connect`, with
one clause that is not symmetric and is the reason the verb needs care: **a
lock-holding origin releases and purges on its way out.**

That is §15.27's phantom-holder lesson applied before the bug could recur here. A
writer removed while holding an endpoint's write lock would otherwise leave it
wedged as locked by an origin that no longer exists, with no recovery. So
`disconnect` unregisters the origin (whether it was the holder or a queued
waiter), wakes the FIFO head so a parked `lock --wait` is granted, and purges the
departing origin's un-flushed targetward backlog so bytes typed under the old lock
cannot surface later under whoever takes the endpoint next (§6). If the departing
origin was itself parked in that queue, its `lock --wait` leaves with `-32602
origin "p1" was detached from its endpoint while waiting` — the endpoint it was
queued for is alive and writable; this writer just stopped being attached to it.

Nothing buffered hostward is lost to the detach itself: removing the producer's
sink closes that edge's channel, and the consumer drains what it already holds
before parking.

### Params

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `a` | `EndpointAddr` | yes | one endpoint of the edge (either order) |
| `b` | `EndpointAddr` | yes | the other endpoint |

### Result

| Field | Type | Description |
| --- | --- | --- |
| `disconnected.a` | string | the host-facing end |
| `disconnected.b` | string | the target-facing end |
| `released_lock` | bool | whether the removed origin was holding the write lock — reported, because it changed who may write |
| `purged_bytes` | integer | un-flushed targetward bytes discarded with the departing origin (§5: loss is always visible). Nonzero only for a **pty** target: that is the buffer §6's purge rule is about (a human typing ahead of a grant). An interior origin's un-sent bytes stay in its own bounded channel and are delivered if it is wired again, so nothing is purged and `0` is the honest answer |

### CLI

```console
$ serial-nexus-ctl disconnect usb0 console
```

### Errors

* `-32602` — no edge joins those two endpoints, or the params are malformed. The
  message names the pair.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"disconnect","params":{"a":"usb0","b":"console"}}' \
    | nc -N -U "$SOCK" | jq .result
{
  "disconnected": { "a": "usb0", "b": "console" },
  "released_lock": true,
  "purged_bytes": 41
}
```

An interior node left with no upstream reports `waiting` — the same honest state
it would have loaded in — and its writers *backpressure* rather than losing bytes:
targetward is the direction §5 forbids dropping on, so an unattached node stalls
its writers exactly as a steal does.

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
$ serial-nexus-ctl dump              # renders TOML
$ serial-nexus-ctl --json dump       # raw GraphConfig JSON
```

The daemon returns structured JSON; `serial-nexus-ctl dump` renders it as TOML
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
