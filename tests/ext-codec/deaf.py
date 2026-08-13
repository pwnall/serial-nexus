#!/usr/bin/env python3
"""An exec codec that has STOPPED READING STDIN, on purpose (§15.22, §5/§15.50).

This is a fixture for the daemon's *accounting*, not for the envelope contract, and
it is the only one here that is deliberately inert rather than deliberately broken.
It spawns, holds its three pipes open, and never reads a byte. The daemon's stdin
feed therefore fills the child's stdin pipe, blocks in `write_all`, and everything
behind it backs up: the internal merge queue fills, then the per-channel targetward
queues behind that. That is the state `discarded_at_teardown` has to be able to
count — the merge stage holding bytes at the moment the node is destroyed (plan §18
item 21).

**Why it never exits.** A child that exits is a *crash* to the supervisor (§7.6): it
faults the node, bumps `restart_count`, and respawns with a fresh pair of pipes,
which would drain another pipe-full out of the queue under the measurement. Staying
alive and silent is what makes the backed-up state stable for as long as the test
needs it. The daemon spawns children with `kill_on_drop`, so nothing here outlives
the node that started it, and `sleep` in a loop costs no CPU (§15.31's idle rule).

Do NOT model a real codec on this file — it is the *sustained targetward stall* the
exec codec's module doc describes as a documented property of the single child pipe,
reproduced on demand. `passthrough.py` is the correct full-duplex shape.
Python stdlib only.
"""
import sys
import time


def main():
    # Never touch sys.stdin. Not one read, not one poll: the point of the fixture is
    # that the writer at the other end of this pipe cannot make progress once the
    # pipe is full, so every byte after that stays inside the daemon where §5 says
    # it must remain countable.
    while True:
        time.sleep(3600)


if __name__ == "__main__":
    sys.exit(main())
