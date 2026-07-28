# Serial-signal verbs

Out-of-band control-line and break signaling on a serial node's live port
(§7.1). These act on the **live port only** — they are ephemeral actions, never
configuration (§15.8), and each requires the node's device currently open. A
node that is faulted or whose device is absent has no open port and rejects
these verbs.

A signal is additionally **node-scoped for its whole duration**. `send-break` and
`pulse-dtr` hold a line asserted for `ms` and then restore it, so they outlive the
instant they were issued: if the node is torn down, removed, replaced by
`load --replace`, or simply reconnects while one is in flight, the verb returns
with `node was removed while signalling` and neither the assertion nor its restore
is applied to whatever port the node holds afterwards (§7.1). The alternative was
reproduced on the bench — a `pulse-dtr --ms 12000` outliving a `load --replace`
flipped DTR on the *successor's* line twelve seconds later, resetting a board with
nothing in `state` to attribute it to, and a break, being tty state, left the
successor transmitting under an asserted break for the remainder of `ms`.

**Scoping decides who may drive the line; it does not decide what the line is
left in, and the two are separate promises.** A declined restore on its own would
leave a break standing on the *tty* with nobody left to clear it — worse than the
bug it fixes, since the assertion outlives `ms` indefinitely instead of expiring
with it. So a departing node **clears** what it asserted rather than leaving it
for its successor: the port returns every tty-level assertion the node made —
break first, then `TIOCEXCL` — before the descriptor goes, on every exit path
(teardown, `remove-node`, the `--replace` handover, a fault, a reconnect). The
successor therefore inherits a transmitting line and its own exclusivity, and no
`ms` has to elapse for that to be true (§7.1).

Methods on this page: [`send-break`](#send-break), [`set-modem`](#set-modem),
[`pulse-dtr`](#pulse-dtr).

---

## `send-break`

Assert a serial break condition on the named node for a duration (§7.1).

### Params

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `node` | string | yes | the serial node |
| `ms` | integer | no (default `250`, max `60000`) | break duration in milliseconds; **maximum 60000** (§7.1, range-checked before any line is asserted) |

### Result

| Field | Type | Description |
| --- | --- | --- |
| `node` | string | the node named |
| `break_ms` | integer | the break duration asserted |

### CLI

```console
$ serialnexusctl send-break usb0            # 250 ms default
$ serialnexusctl send-break usb0 --ms 500
```

### Errors

* `-32602` — missing `node`, an unknown node, a node that is not a serial node,
  a serial node with no open port (device absent/faulted), or a break failure on
  the port. Also an `ms` above 60000, refused by name before the port is even
  resolved (`send-break: ms = 90000, above the maximum 60000 (a numeric field is
  range-checked before anything is asserted, §11)`) — the same sentence a numeric
  configuration field out of range gets, because it is the same mistake; and the
  node losing or replacing its port while the break was in flight (`send-break on
  "usb0": node was removed while signalling`) — in which case the deferred
  restore is declined too, and **the line is nonetheless left cleared**, because
  the departing node returns the break with the port (§7.1). The verb reporting
  failure is not a report that the break is still asserted.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"send-break","params":{"node":"usb0","ms":500}}' \
    | nc -N -U "$SOCK" | jq .result
{
  "node": "usb0",
  "break_ms": 500
}
```

---

## `set-modem`

Drive the DTR and/or RTS modem-control lines on the live port (§7.1). A line
whose param is omitted or `null` is **left untouched**; the result echoes the
values applied (null where untouched). Ephemeral, not configuration (§15.8).

### Params

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `node` | string | yes | the serial node |
| `dtr` | bool \| null | no | set DTR to this level; null/omitted leaves it untouched |
| `rts` | bool \| null | no | set RTS to this level; null/omitted leaves it untouched |

### Result

| Field | Type | Description |
| --- | --- | --- |
| `node` | string | the node named |
| `dtr` | bool \| null | the DTR level applied (null if untouched) |
| `rts` | bool \| null | the RTS level applied (null if untouched) |

### CLI

```console
$ serialnexusctl set-modem usb0 --dtr true
$ serialnexusctl set-modem usb0 --dtr false --rts true
```

Omitting `--dtr`/`--rts` on the CLI sends `null` for that line, leaving it
untouched.

### Errors

* `-32602` — missing `node`, an unknown node, a non-serial node, a serial node
  with no open port, or a modem-line failure on the port.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"set-modem","params":{"node":"usb0","dtr":true,"rts":null}}' \
    | nc -N -U "$SOCK" | jq .result
{
  "node": "usb0",
  "dtr": true,
  "rts": null
}
```

---

## `pulse-dtr`

Pulse DTR — the classic auto-reset toggle (§7.1): drive DTR to `assert` for a
duration, then to the opposite level.

### Params

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `node` | string | yes | the serial node |
| `ms` | integer | no (default `100`, max `60000`) | pulse duration in milliseconds; **maximum 60000** (§7.1, range-checked before any line is asserted) |
| `assert` | bool | no (default `true`) | the level to hold during the pulse (then reset to its opposite) |

### Result

| Field | Type | Description |
| --- | --- | --- |
| `node` | string | the node named |
| `pulse_ms` | integer | the pulse duration |
| `assert` | bool | the asserted level used |

### CLI

```console
$ serialnexusctl pulse-dtr usb0                         # 100 ms, assert=true
$ serialnexusctl pulse-dtr usb0 --ms 200 --assert false
$ serialnexusctl pulse-dtr usb0 --ms 200 --assert=false  # equivalent
```

`--assert` **takes a value** (`true` or `false`); it is not a bare flag, and both
the space and `=` spellings work. That is deliberate: the RPC's low-then-high
pulse is `assert = false`, and a bare boolean flag defaulting to `true` could
only ever send `true`, leaving half the verb unreachable from the CLI. Omitting
`--assert` entirely still gives the documented default of `true`; writing
`--assert` with no value is an error naming the two accepted values.

### Errors

* `-32602` — missing `node`, an unknown node, a non-serial node, a serial node
  with no open port, or a pulse failure on the port. Also an `ms` above 60000,
  refused by name before the port is resolved (`pulse-dtr: ms = 90000, above the
  maximum 60000 (a numeric field is range-checked before anything is asserted,
  §11)`), and the node losing or replacing its port mid-pulse (`pulse-dtr on
  "usb0": node was removed while signalling`) — in which case the deferred
  restore is declined too, rather than driving DTR on a port the node no longer
  owns.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"pulse-dtr","params":{"node":"usb0","ms":200,"assert":true}}' \
    | nc -N -U "$SOCK" | jq .result
{
  "node": "usb0",
  "pulse_ms": 200,
  "assert": true
}
```
