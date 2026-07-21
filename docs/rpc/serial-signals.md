# Serial-signal verbs

Out-of-band control-line and break signaling on a serial node's live port
(§7.1). These act on the **live port only** — they are ephemeral actions, never
configuration (§15.8), and each requires the node's device currently open. A
node that is faulted or whose device is absent has no open port and rejects
these verbs.

Methods on this page: [`send-break`](#send-break), [`set-modem`](#set-modem),
[`pulse-dtr`](#pulse-dtr).

---

## `send-break`

Assert a serial break condition on the named node for a duration (§7.1).

### Params

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `node` | string | yes | the serial node |
| `ms` | integer | no (default `250`) | break duration in milliseconds |

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
  the port.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"send-break","params":{"node":"usb0","ms":500}}' \
    | nc -U "$SOCK" | jq .result
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
    | nc -U "$SOCK" | jq .result
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
| `ms` | integer | no (default `100`) | pulse duration in milliseconds |
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
```

### Errors

* `-32602` — missing `node`, an unknown node, a non-serial node, a serial node
  with no open port, or a pulse failure on the port.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"pulse-dtr","params":{"node":"usb0","ms":200,"assert":true}}' \
    | nc -U "$SOCK" | jq .result
{
  "node": "usb0",
  "pulse_ms": 200,
  "assert": true
}
```
