#!/usr/bin/env python3
"""A DELIBERATELY BROKEN exec codec: the half-duplex / deadlock antipattern (§15.22).

It reads *all* of stdin to EOF, then writes *all* of stdout — a byte-for-byte
passthrough, so it is correct when the input is finite and closed (the golden and
fragmentation checks pass). But it never emits a byte until end-of-input, so under
sustained full-duplex flow it never interleaves read and write. The exec
conformance harness's liveness check (send one frame, require its echo before the
next) therefore times out against this fixture — exactly the coupling the daemon's
concurrent-pump rule (§15.22) exists to forbid, here proven to be *caught* rather
than shipped. Python stdlib only.

Do not model a real codec on this file. See `passthrough.py` for the correct,
full-duplex shape (echo each frame as it is read).
"""
import sys


def main():
    # Read everything first — the bug. A full-duplex codec would echo as it reads.
    data = sys.stdin.buffer.read()
    sys.stdout.buffer.write(data)
    sys.stdout.buffer.flush()


if __name__ == "__main__":
    main()
