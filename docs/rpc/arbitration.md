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

> **The two waiting verbs, and the rules that come with them.** `lock` with
> `wait` and `send` are the only verbs that can suspend, and a connection runs
> **one of them at a time**. A request pipelined onto a connection whose waiting
> verb is still parked is answered `-32006` (`a waiting verb is already in flight
> on this connection; only one is supported at a time — open a second connection,
> or retry once it resolves`), carrying that request's own `id`; the parked verb
> is untouched and still answers when it resolves. Concurrent work wants a second
> connection.
>
> A parked verb does **not** freeze the rest of its connection: the wait is one
> arm of that connection's single `select!`, so its `subscribe` stream and every
> tap it holds keep being delivered for the whole wait. The distinction matters
> unevenly — `subscribe` traffic would merely arrive late, but `tap.data` past
> the 128-chunk per-connection tap queue is really *lost* — and the §17 console
> drives `subscribe`, every tap and every `send` down one daemon connection, so
> a contended keystroke would otherwise black its terminal out for the whole
> `timeout_ms`.
>
> Reading **end-of-file** on the connection is the other case, and it is not the
> same: it *cancels* the parked verb and closes the connection, because a killed
> `lock --wait` client must leave the FIFO queue promptly and a half-close is
> indistinguishable from a killed client at read time (§15.20). A raw client must
> therefore keep its write half open across a waiting verb — `nc -N -U` gets no
> reply at all — which is why the one-shot `nc -N -U` idiom used elsewhere on
> these pages does not apply to a contended `lock --wait` or `send`.
> `serialnexusctl` keeps both halves open.

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
false`; a steal is `held: true, acquired: true` plus `stole_from`. A `steal` by
the origin that *already* holds the lock is the same no-op as the re-lock —
`held: true, acquired: false, stole_from: null` — and deliberately so: it must
not purge the holder's own in-flight bytes or void its lease by bumping the grant
generation. A `lease_ms` given on either no-op path re-arms the lease.

Every *fresh* grant does purge: whatever the origin buffered before it held the
floor is drained and discarded (§6's stale-command rule — safe by construction
for a client that acquires before it writes), and the count lands in that
origin's `purged` field in
[`LockSnapshot`](observation.md#locksnapshot). The same counter carries §6's
**detach** instance, and for a pty origin that instance covers more than the
console's kernel buffer: a client that typed into an endpoint which could not
take its bytes leaves both the input still queued in the pair *and* whatever
chunk the node's reader had already taken off the master and was holding for a
full endpoint. Both are drained and charged to `purged` when the client goes.
Deliberately to `purged` and not to the pty's `discarded_targetward`: that
counter means loss — bytes an endpoint that went away could never take — while
these were discarded on purpose at the moment the floor question settled, and
reading them as loss would report a §5 violation that did not happen. Carrying
them instead of purging them is the alternative that is actually unsafe, since
the next holder's line would be interleaved with a departed console's typing.

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
  endpoint, or an origin whose write mode is `never` (it cannot hold the lock);
  or an origin that was **detached from its endpoint while waiting** (`origin
  "p1" was detached from its endpoint while waiting`) — a parked `--wait` whose
  edge was `disconnect`ed, or whose node was removed while a *surviving*
  endpoint's lock still had it queued. That case is stated separately because it
  is not a claim about configuration: the origin's declared write mode is
  untouched, and reporting it as `write=never` sent operators hunting a
  `write_mode` value the file has never contained.
* `-32006` (waiting verb in flight) — answered to a request *pipelined behind* a
  parked `--wait` acquire on the same connection, not to the acquire itself; the
  acquire keeps waiting. See the note at the top of this page.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"lock","params":{"origin":"demux","steal":true}}' \
    | nc -N -U "$SOCK" | jq .result
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
    | nc -N -U "$SOCK" | jq .result
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

`timeout_ms` bounds that whole sequence, not the acquire alone. The endpoint's
targetward channel is a backpressure point too — it is 256 chunks deep and fills
whenever the target stops draining, which a *present*, `active` node whose peer
merely stopped reading does just as thoroughly as an absent device — and a `send`
parked there is a `send` still holding the endpoint's exclusive lock, which is
precisely what makes §6's "transient" origin non-transient. `steal`, which skips
the acquire entirely, runs under the same deadline for the same reason.

### Params

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `endpoint` | string | yes | the host-facing endpoint to write to (e.g. `usb0`, `mux/ch2`) |
| `line` | string | yes | the line to send; a trailing `\n` is appended by the daemon |
| `timeout_ms` | integer | no (default `2000`) | give up with the locked error after this long — the deadline bounds the **whole** operation, the acquire *and* the targetward write, not the acquire alone (§6) |
| `steal` | bool | no (default `false`) | take the lock from the current holder instead of waiting |

### Result

| Field | Type | Description |
| --- | --- | --- |
| `endpoint` | string | the endpoint named |
| `sent` | integer | bytes written targetward, including the appended newline |
| `delivered` | bool | always `true` on success — the bytes reached the endpoint's targetward channel. Failing to reach it is an error, not a `false` result (see below) |

### CLI

```console
$ serialnexusctl send usb0 --line "reboot"
$ serialnexusctl send usb0 --line "reboot" --timeout-ms 500
$ serialnexusctl send usb0 --line "reboot" --steal
```

The CLI spelling `--timeout-ms` maps to `timeout_ms`.

### Errors

* `-32003` (locked) — three refusals share the code and the message separates
  them. The endpoint stayed locked until `timeout_ms` elapsed (`endpoint "usb0"
  is locked; send timed out`); it was torn down while sending (`endpoint "usb0"
  was torn down while sending` — also the answer if the targetward channel closes
  between the pre-flight check and delivery); or the lock was won and the *write*
  could not land inside the same deadline because the endpoint's targetward
  channel is full (`endpoint "usb0" did not accept the write within 2000ms
  (targetward backpressure); nothing was sent`). None of the three carries
  `data.held_by`. The backpressure path delivered **nothing** — the underlying
  send is cancel-safe, so a timed-out delivery enqueues zero bytes and this verb
  can never write a partial line — and it releases the transient origin on its
  way out, so the endpoint is not left held past the deadline the caller asked
  for.
* `-32602` — missing `endpoint`/`line`; an `endpoint` that is not a host-facing
  endpoint with a write lock; or an endpoint that advertises a targetward path it
  cannot use (`endpoint "mux/c0" cannot accept targetward writes (its interior
  node has no writable path to the device)`) — a read-only or unattached interior
  side, refused up front rather than after the lock dance.
* `-32006` (waiting verb in flight) — answered to a request *pipelined behind* a
  `send` that is still waiting for the lock, not to the `send` itself. See the
  note at the top of this page.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"send","params":{"endpoint":"usb0","line":"reboot"}}' \
    | nc -N -U "$SOCK" | jq .result
{
  "endpoint": "usb0",
  "sent": 7,
  "delivered": true
}
```

`sent` is `7` — the six bytes of `reboot` plus the appended newline.
