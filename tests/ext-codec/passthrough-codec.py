#!/usr/bin/env python3
"""A 1-channel passthrough exec codec for the crash-containment test (§7.6).

Speaks the shared envelope on stdin/stdout (see codec-api). The multiplexed side
is the reserved empty channel identity; this codec swaps it with one real channel
(argv[1], default "c0") in both directions, so a device byte stream appears on the
channel and vice versa — a trivial demux/remux whose correctness is a checksum.
Python stdlib only, no dependencies.
"""
import struct
import sys

MUX = ""
CHAN = sys.argv[1] if len(sys.argv) > 1 else "c0"


def read_exact(f, n):
    buf = bytearray()
    while len(buf) < n:
        chunk = f.read(n - len(buf))
        if not chunk:
            return None
        buf += chunk
    return bytes(buf)


def encode(channel, type_byte, payload):
    chan = channel.encode("utf-8")
    body = bytes([type_byte]) + struct.pack(">H", len(chan)) + chan + payload
    return struct.pack(">I", len(body)) + body


def main():
    inp, out = sys.stdin.buffer, sys.stdout.buffer
    while True:
        header = read_exact(inp, 4)
        if header is None:
            break
        (body_len,) = struct.unpack(">I", header)
        body = read_exact(inp, body_len)
        if body is None:
            break
        type_byte = body[0]
        (chan_len,) = struct.unpack(">H", body[1:3])
        channel = body[3:3 + chan_len].decode("utf-8", "replace")
        payload = body[3 + chan_len:]

        if channel == MUX:
            out_channel = CHAN
        elif channel == CHAN:
            out_channel = MUX
        else:
            out_channel = channel
        out.write(encode(out_channel, type_byte, payload))
        out.flush()


if __name__ == "__main__":
    main()
