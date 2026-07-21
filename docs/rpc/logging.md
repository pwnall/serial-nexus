# Logging verbs

On-demand log rotation (§7.3). A log node writes hostward traffic to a file; this
verb asks it to close the current file and start a new one.

Method on this page: [`rotate`](#rotate).

---

## `rotate`

Rotate a log node's file on demand (§7.3). The rotation is requested on the
node's writer thread and flushed within the bounded wait; the returned index
identifies the new generation — **higher is newer**.

### Params

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `node` | string | yes | the log node to rotate |

### Result

| Field | Type | Description |
| --- | --- | --- |
| `node` | string | the node named |
| `rotated_to` | integer | the new rotation index (monotonic; higher is newer) |

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
    | nc -U "$SOCK" | jq .result
{
  "node": "applog",
  "rotated_to": 1
}
```
