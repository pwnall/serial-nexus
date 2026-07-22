#!/usr/bin/env python3
"""A CORRECT full-duplex exec codec with a bounded one-frame output lag (§15.26).

It echoes each frame, but one frame behind: it holds the frame it just read until
the next frame arrives, then emits the held one — and flushes the last held frame
at EOF. This is legitimate buffering: it never hoards more than one frame and
always makes progress as input flows (it is NOT the half-duplex antipattern in
`half-duplex.py`, which emits nothing until EOF). The exec-conformance liveness
check must ACCEPT this shape; it exists as the regression fixture proving the check
is not a lock-step ping-pong that would reject any buffering codec. Python stdlib
only.
"""
import struct
import sys


def read_exact(f, n):
    buf = bytearray()
    while len(buf) < n:
        chunk = f.read(n - len(buf))
        if not chunk:
            return None
        buf.extend(chunk)
    return bytes(buf)


def read_frame(f):
    header = read_exact(f, 4)
    if header is None:
        return None
    (body_len,) = struct.unpack(">I", header)
    body = read_exact(f, body_len)
    if body is None:
        return None
    return header + body


def main():
    stdin, stdout = sys.stdin.buffer, sys.stdout.buffer
    held = None
    while True:
        frame = read_frame(stdin)
        if frame is None:
            break  # EOF
        if held is not None:
            stdout.write(held)  # emit the previous frame — one behind
            stdout.flush()
        held = frame
    if held is not None:
        stdout.write(held)  # flush the final held frame at EOF
        stdout.flush()


if __name__ == "__main__":
    main()
