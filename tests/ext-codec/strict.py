#!/usr/bin/env python3
"""A passthrough exec codec that REFUSES malformed input (§8 clause 5).

`passthrough.py` is the identity fixture: it re-encodes whatever it parses and
validates nothing, which is fine for the universal battery and is exactly why it
fails the opt-in `--error-paths` checks. This one is the positive control for those
checks, and the shape to copy where a codec must refuse rather than relay.

The three-outcome decode contract, in full:

  * whole frame  -> handle it and continue;
  * need more    -> read more bytes; a strict prefix is never an error, and EOF
                    mid-frame is teardown, not corruption (see `read_exact`);
  * malformed    -> refuse cleanly, and *say so*. Here that is an `error` event on
                    the reserved empty channel followed by a non-zero exit. Either
                    signal alone satisfies the harness; both together is what a real
                    codec wants, because the daemon reads the frame and the operator
                    reads the exit status.

The faults it refuses are the format's own decode errors: a body length above
MAX_FRAME_SIZE (refused from the length prefix alone, *before* the body is
buffered), a type byte outside 0..=3, and a channel identity length that overruns
the body it was declared in. Python stdlib only.
"""
import struct
import sys

MAX_FRAME_SIZE = 64 * 1024
KINDS = (0, 1, 2, 3)  # data, open, close, error


class Malformed(Exception):
    """A frame that cannot be decoded — the third outcome, raised at its field."""


def read_exact(f, n):
    """Exactly n bytes, or None if the stream ends first.

    None is the *teardown* answer, not the malformed one: a writer can vanish
    between any two reads, and a codec that reported truncation as corruption would
    turn every clean stop into an error. Faults are raised as `Malformed`; running
    out of input is not a fault.
    """
    buf = bytearray()
    while len(buf) < n:
        chunk = f.read(n - len(buf))
        if not chunk:
            return None
        buf.extend(chunk)
    return bytes(buf)


def encode(channel, type_byte, payload):
    chan = channel.encode("utf-8")
    body = bytes([type_byte]) + struct.pack(">H", len(chan)) + chan + payload
    return struct.pack(">I", len(body)) + body


def refuse(out, reason):
    """Signal the refusal both ways, then stop. Nothing is relayed onward."""
    out.write(encode("", 3, reason.encode("utf-8")))
    out.flush()
    print("strict.py: refused: %s" % reason, file=sys.stderr)
    sys.exit(2)


def main():
    inp, out = sys.stdin.buffer, sys.stdout.buffer
    while True:
        header = read_exact(inp, 4)
        if header is None:
            break  # clean EOF at a frame boundary
        (body_len,) = struct.unpack(">I", header)

        # Oversize is refused from the prefix alone: buffering a body we have already
        # decided to reject is the memory-exhaustion bug the bound exists to prevent.
        if body_len > MAX_FRAME_SIZE:
            refuse(out, "frame body length %d exceeds the maximum %d"
                        % (body_len, MAX_FRAME_SIZE))

        body = read_exact(inp, body_len)
        if body is None:
            break  # truncated trailing frame: teardown, not corruption
        if len(body) < 3:
            refuse(out, "truncated frame: body shorter than its own header")

        type_byte = body[0]
        if type_byte not in KINDS:
            refuse(out, "unknown frame type %d" % type_byte)

        (chan_len,) = struct.unpack(">H", body[1:3])
        if 3 + chan_len > len(body):
            refuse(out, "truncated frame: channel identity length %d overruns a "
                        "%d-byte body" % (chan_len, len(body)))
        try:
            channel = body[3:3 + chan_len].decode("utf-8")
        except UnicodeDecodeError:
            refuse(out, "channel identity is not valid UTF-8")
        payload = body[3 + chan_len:]
        if type_byte == 3:
            try:
                payload.decode("utf-8")
            except UnicodeDecodeError:
                refuse(out, "error message is not valid UTF-8")

        out.write(encode(channel, type_byte, payload))
        out.flush()
    out.flush()


if __name__ == "__main__":
    main()
