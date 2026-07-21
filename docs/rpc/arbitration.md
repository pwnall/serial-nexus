# Arbitration verbs

The write-lock surface (§6). Reading targetward is never arbitrated; these verbs
govern *who may write* through a host-facing endpoint. They run on the two-lane
control plane (§15.20): every lock transition is a synchronous critical section
on the runtime thread, and a verb that must wait suspends holding nothing and
re-attempts when woken, so concurrent connections keep flowing past a parked
waiter. Every transition also emits an immediate [`lock`
notification](observation.md#lock-notification) to subscribers.

Methods on this page: [`lock`](#lock), [`unlock`](#unlock), [`send`](#send). The
[`LockSnapshot`](observation.md#locksnapshot) that `state` and notifications
report is documented on the observation page.

> **`lock`/`unlock` name the ORIGIN; `send` names the ENDPOINT.** A lock belongs
> to an endpoint, but an origin (a target-facing writer) feeds exactly one
> endpoint, so `lock`/`unlock` address it by the origin that wants to write.
> `send` writes as a *transient* origin of its own, so it addresses the
> host-facing endpoint directly.

---

## `lock`

A named origin acquires its endpoint's exclusive write lock — thereafter only
its bytes are read targetward through that endpoint (§6). A plain, un-waited
contended acquire **fails fast**. `wait` joins the FIFO queue and suspends until
granted; `steal` takes the lock from the current holder; `lease_ms`
auto-releases after a duration, guarded by grant generation so a stale timer can
never release a later grant.

### Params

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `origin` | string | yes | the writable origin acquiring the lock |
| `steal` | bool | no (default `false`) | take the lock from whoever holds it |
| `wait` | bool | no (default `false`) | join the FIFO queue and block until granted, instead of failing fast |
| `lease_ms` | integer | no | auto-release this many ms after the grant |

`steal` and `wait` are mutually distinct paths: `steal` bypasses the queue
immediately; `wait` joins it. A re-lock by the current holder with a `lease_ms`
re-arms (renews) the lease.

### Result

| Field | Type | Description |
| --- | --- | --- |
| `origin` | string | the origin named |
| `held` | bool | whether the origin now holds the lock |
| `acquired` | bool | whether *this call* freshly acquired it (false if it already held it) |
| `stole_from` | string \| null | *(steal only)* the ousted holder's name, or null if none held it |

The `held`/`acquired` combinations: a fresh grant is `held: true, acquired:
true`; an idempotent re-lock by the current holder is `held: true, acquired:
false`; a steal is `held: true, acquired: true` plus `stole_from`.

### CLI

```console
$ serialnexusctl lock demux                       # fail fast if contended
$ serialnexusctl lock demux --wait                # block until granted
$ serialnexusctl lock demux --steal               # take it from the holder
$ serialnexusctl lock demux --lease-ms 5000       # auto-release after 5s
```

Note the CLI spelling `--lease-ms` maps to the `lease_ms` param.

### Errors

* `-32003` (locked) — a contended fast acquire (no `wait`, no `steal`). The
  `data.held_by` field names the current holder when known. Also returned if the
  endpoint is torn down while a `--wait` acquire is parked.
* `-32602` — missing `origin`, an origin that is not a writable origin on any
  endpoint, or an origin whose write mode is `never` (it cannot hold the lock).

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"lock","params":{"origin":"demux","steal":true}}' \
    | nc -U "$SOCK" | jq .result
{
  "origin": "demux",
  "held": true,
  "acquired": true,
  "stole_from": "console"
}
```

A contended fast acquire instead returns:

```json
{"jsonrpc":"2.0","id":1,"error":{"code":-32003,"message":"endpoint is locked by demux","data":{"held_by":"demux"}}}
```

---

## `unlock`

Release the endpoint's write lock if the named origin holds it, then wake the
FIFO head so the next waiter is granted. Releasing when you do not hold the lock
is reported, not an error.

### Params

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `origin` | string | yes | the origin whose lock to release |

### Result

| Field | Type | Description |
| --- | --- | --- |
| `origin` | string | the origin named |
| `released` | bool | true if it held the lock and released it; false if it was not holding it |

### CLI

```console
$ serialnexusctl unlock demux
```

### Errors

* `-32602` — missing `origin`, or an origin that is not a writable origin on any
  endpoint.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"unlock","params":{"origin":"demux"}}' \
    | nc -U "$SOCK" | jq .result
{
  "origin": "demux",
  "released": true
}
```

---

## `send`

Deliver one line targetward through a named host-facing endpoint, with the
daemon acting as a **transient origin** on the caller's behalf (§6). It registers
a synthetic origin, acquires the write lock (with a timeout, or `steal`s it),
writes the line with a trailing newline appended, releases, and unregisters —
**one atomic acquire-write-release**. The transient origin is always cleaned up,
even if the call times out or the connection drops.

### Params

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `endpoint` | string | yes | the host-facing endpoint to write to (e.g. `usb0`, `mux/ch2`) |
| `line` | string | yes | the line to send; a trailing `\n` is appended by the daemon |
| `timeout_ms` | integer | no (default `2000`) | give up with the locked error after this long if the lock is held |
| `steal` | bool | no (default `false`) | take the lock from the current holder instead of waiting |

### Result

| Field | Type | Description |
| --- | --- | --- |
| `endpoint` | string | the endpoint named |
| `sent` | integer | bytes written targetward, including the appended newline |
| `delivered` | bool | whether the bytes reached the targetward channel |

### CLI

```console
$ serialnexusctl send usb0 --line "reboot"
$ serialnexusctl send usb0 --line "reboot" --timeout-ms 500
$ serialnexusctl send usb0 --line "reboot" --steal
```

The CLI spelling `--timeout-ms` maps to `timeout_ms`.

### Errors

* `-32003` (locked) — the endpoint stayed locked until `timeout_ms` elapsed
  (`endpoint … is locked; send timed out`), or was torn down while sending. This
  path carries no `data.held_by`.
* `-32602` — missing `endpoint`/`line`, or an `endpoint` that is not a
  host-facing endpoint with a write lock.
* `-32603` — the endpoint's targetward channel was closed (nothing to deliver
  to).

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"send","params":{"endpoint":"usb0","line":"reboot"}}' \
    | nc -U "$SOCK" | jq .result
{
  "endpoint": "usb0",
  "sent": 7,
  "delivered": true
}
```

`sent` is `7` — the six bytes of `reboot` plus the appended newline.
