#!/usr/bin/env python3
"""A passthrough external codec for the envelope golden-vector battery (§8).

Reads envelope frames on stdin, parses each one, and re-emits it identically on
stdout. Parsing-then-re-encoding (rather than a raw byte copy) is deliberate: it
proves the child actually implements the shared frame format in another language,
which is the point of the any-language envelope contract. Python stdlib only —
no dependencies, no copyleft.

Envelope layout (all integers big-endian), per serial-nexus-codec-api:
    u32 body_len            length of everything after this field
    u8  type                0=data, 1=open, 2=close, 3=error
    u16 channel_id_len
    ..  channel_id          UTF-8 channel identity
    ..  payload             data bytes / error message / empty
"""
import struct
import sys


def read_exact(f, n):
    """Read exactly n bytes, or return None if the stream ends before n arrive.

    None does NOT mean "a frame boundary": a short read mid-frame returns None too,
    and the caller silently drops the partial frame. That is deliberate and worth
    copying — the writer's end of this pipe can go away between any two reads, so a
    truncated trailing frame is an ordinary teardown shape rather than corruption,
    and a codec that raised on it would turn every clean stop into an error. What it
    costs is that this function cannot tell the two apart, so it is not the thing to
    reach for where a truncated frame has to be *reported*.
    """
    buf = bytearray()
    while len(buf) < n:
        chunk = f.read(n - len(buf))
        if not chunk:
            return None
        buf.extend(chunk)
    return bytes(buf)


def main():
    stdin = sys.stdin.buffer
    stdout = sys.stdout.buffer
    while True:
        header = read_exact(stdin, 4)
        if header is None:
            break  # clean EOF
        (body_len,) = struct.unpack(">I", header)
        body = read_exact(stdin, body_len)
        if body is None:
            break
        type_byte = body[0]
        (chan_len,) = struct.unpack(">H", body[1:3])
        channel = body[3:3 + chan_len]
        payload = body[3 + chan_len:]

        # Re-encode the parsed event, byte for byte.
        out_body = bytes([type_byte]) + struct.pack(">H", chan_len) + channel + payload
        stdout.write(struct.pack(">I", len(out_body)) + out_body)
        stdout.flush()
    stdout.flush()


if __name__ == "__main__":
    main()
