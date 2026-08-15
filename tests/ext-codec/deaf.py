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

**Why it never exits on its own.** A child that exits is a *crash* to the supervisor
(§7.6): it faults the node, bumps `restart_count`, and respawns with a fresh pair of
pipes, which would drain another pipe-full out of the queue under the measurement.
Staying alive and silent is what makes the backed-up state stable for as long as the
test needs it.

**Why it nevertheless watches for its parent's death** (plan §18 item 65(c)). This
file used to claim that "the daemon spawns children with `kill_on_drop`, so nothing
here outlives the node that started it". That is true only where the daemon gets to
*run code* — on `remove-node`, on `teardown`, on `shutdown`, on SIGTERM. After a
SIGKILL the daemon runs nothing, and §15.43's leash cannot help either: that leash is
the EOF on this child's stdin, and the one thing this fixture must never do is read
its stdin. So a SIGKILLed daemon left this process running forever, `ppid 1`,
invisible to every assertion the suite makes — measured, and reproduced by
`p5_exec_orphans.rs`. The watch below closes it without touching a single pipe.

`os.getppid()` and not a `poll()` for the hangup: with the stdin pipe full, `poll`
can only distinguish "closed" from "has data" through a mask that requests nothing,
and this project has *measured* that Darwin does not deliver a hangup to an empty
requested mask where Linux does (doctor P9's
`hangup_delivered_to_a_mask_that_requested_nothing`; plan §18 item 66). A watch built
on that would pass on the box it was written on and be silently dead on the other
kernel — AGENTS §9's proxy in space — while reparenting is POSIX and identical on
both. It is the same argument §15.43 used to decline `PR_SET_PDEATHSIG` and kqueue
`NOTE_EXIT`.

Do NOT model a real codec on this file — it is the *sustained targetward stall* the
exec codec's module doc describes as a documented property of the single child pipe,
reproduced on demand. `passthrough.py` is the correct full-duplex shape, and a codec
that reads its stdin gets §15.43's leash for free and needs none of this.
Python stdlib only.
"""
import os
import sys
import threading
import time

# How often the parent watch looks. Four wakeups a second of one `getppid(2)` is not
# a busy-wait (§15.31's idle rule, which this fixture is measured against): 0 CPU
# ticks over 5 s, the same reading the parked transcript child was re-measured at
# (notes §3.91). It bounds how long an orphan can exist, and the harness's own
# process-group sweep allows ten seconds before it complains — forty times this
# interval, so a loaded box has room and a leak still has none.
WATCH_INTERVAL_S = 0.25


def orphaned(started_under):
    """Has the process that spawned us gone away?

    Two arms, and the second one is the whole reason this is not a one-liner.

    1. **Reparented.** A child of a dead parent is adopted by pid 1 (or by the nearest
       subreaper), so `getppid()` stops answering the pid we started under. Compared
       against that pid rather than against `1`, so this arm is right under a
       subreaper too.
    2. **Already an orphan before we could look.** CPython takes tens of milliseconds
       to reach its first bytecode, and a `kill -9` of the daemon can land inside that
       window — *measured*, on the second run of `p5_exec_orphans.rs`, where the whole
       stimulus is over in about that long. Then `started_under` is already the
       adopter's pid, arm 1 compares it with itself forever, and the fixture leaks
       exactly as it did before the watch existed. The tell is that no codec child is
       ever legitimately spawned by init.

    The stated limit: a daemon running *as* pid 1 (a container with no init) makes arm
    2 true at startup and this fixture exits at once. It is a test fixture, the suite
    never runs that way, and the alternative — trusting arm 1 alone — is a guard with a
    live race in it.
    """
    ppid = os.getppid()
    return ppid != started_under or ppid == 1


def watch_parent(started_under):
    """Exit once [`orphaned`] says the spawner is gone.

    `os._exit` because this runs on a non-main thread, where `sys.exit` unwinds only
    the thread and would leave the process exactly as orphaned as before.
    """
    while not orphaned(started_under):
        time.sleep(WATCH_INTERVAL_S)
    os._exit(0)


READY_FILE_ENV = "SNX_DEAF_READY_FILE"


def announce_ready():
    """Record that this file actually ran, for the guard that asserts our *absence*.

    `p5_exec_orphans.rs`'s two-arm watch test kills the spawner and then requires the
    fixture to be gone. An absence is evidence only once presence has been
    established (AGENTS §3) — and presence is exactly what that test could not
    establish from the outside: in its *before-it-starts* arm the pid the supervisor
    prints belongs to a subshell that has not `exec`ed CPython yet, so a liveness poll
    on it would answer yes against a fixture that never reached a single bytecode.
    Measured: a `deaf.py` whose first statement is `raise SystemExit` passed both arms
    green, in 0.1 s and 0.2 s.

    So the fixture says so itself, and says *which pid it is*, which lets the test
    check that the pid it goes on to watch is this process and not an unrelated one.
    Written with a rename so the file never exists half-written; unset in every other
    run — including every daemon-spawned one — where this is a no-op.

    Before the watch is armed, deliberately: in the already-an-orphan arm that watch
    can `os._exit` on its first tick, which would race this write and make readiness
    look like a fixture that never started.
    """
    path = os.environ.get(READY_FILE_ENV)
    if not path:
        return
    tmp = path + ".partial"
    with open(tmp, "w") as f:
        f.write(str(os.getpid()))
    os.replace(tmp, path)


def main():
    announce_ready()
    threading.Thread(
        target=watch_parent, args=(os.getppid(),), name="parent-watch", daemon=True
    ).start()
    # Never touch sys.stdin. Not one read, not one poll: the point of the fixture is
    # that the writer at the other end of this pipe cannot make progress once the
    # pipe is full, so every byte after that stays inside the daemon where §5 says
    # it must remain countable.
    while True:
        time.sleep(3600)


if __name__ == "__main__":
    sys.exit(main())
