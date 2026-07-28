# Logging verbs

On-demand log rotation (§7.3). A log node writes hostward traffic to a file; this
verb asks it to close the current file and start a new one.

Method on this page: [`rotate`](#rotate).

---

## `rotate`

Rotate a log node's file on demand (§7.3). The request is **ordered against the
node's queue**: a marker is pushed behind every byte already accepted and ahead
of everything accepted from now on, so the writer renames the current file to
`<name>.NNN` and reopens fresh at exactly that boundary — which is the operator's
mental model of `rotate`. The verb returns as soon as the marker is queued, so
it never blocks the control plane on the disk; the returned index identifies the
new generation — **higher is newer**, and the first rotation is `0`
(`<name>.000` with the default 3-digit `rotation_padding`). Rapid successive
calls each queue their own marker and each get their own index. The counter is
observed state, recovered at start by scanning the directory and never
persisted.

The ordering has a cost worth stating: because the marker sits *behind* the
accepted backlog, a node with a deep queue rotates only once that backlog has
been written. An operator rotating **because** the filesystem is struggling
should expect the rename to lag the acknowledgement by however long the queue
takes to drain — `state`'s `queued_bytes` for the node is the number to watch,
and it now includes the batch the writer is holding, not just the pending one.

If the rotation itself fails — an unwritable directory, `ENOSPC` on the rename,
or a reopen that cannot recreate the file — the node faults and its writer stops
**for good**, under either overflow policy: a failed rename means nothing rotated,
and a writer that kept appending would silently merge the two generations the
operator asked to separate. What that costs is worth reading off `state` rather
than inferring: `queued_bytes` falls to `0` and stays there, because nothing will
drain that queue again, and every byte the node is handed from then on is counted
in `dropped_bytes`. Those bytes are loss, not backlog, and saying so is the whole
point — reporting them as depth once hid 16 MiB of unwritable console data behind
a flat loss counter.

### Params

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `node` | string | yes | the log node to rotate |

### Result

| Field | Type | Description |
| --- | --- | --- |
| `node` | string | the node named |
| `rotated_to` | integer | the new rotation index (monotonic; higher is newer; the first is `0`) |

### CLI

```console
$ serialnexusctl rotate applog
```

### Errors

* `-32602` — missing `node`, an unknown node, a node that is not a log node, or
  a log node that is currently faulted.

### Example

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"rotate","params":{"node":"applog"}}' \
    | nc -N -U "$SOCK" | jq .result
{
  "node": "applog",
  "rotated_to": 0
}
```

The rotated-out file is the node's `filename` with `.000` appended; the node keeps
writing to `filename`. A log node's `state` extras report the current file, the
last completed rotation
index, and the queue's `queued_bytes`/`dropped_bytes` plus
`write_errors`/`last_write_error` — the pair that separates a slow consumer from
a filesystem refusing every write (see
[observation.md](observation.md#loss-counters-in-the-node-type-extras)).

A log node has one more way to arrive without a writer, and it is environmental
rather than structural, so it never fails the `load` (§15.8): if the writer thread
cannot be spawned — a thread or PID limit reached — the node comes up `faulted`
with `spawn log writer thread: <os error>` and behaves exactly as a node whose
writer has stopped, counting everything offered to it as `dropped_bytes`. The
graph loads; the node says why it is not writing.
