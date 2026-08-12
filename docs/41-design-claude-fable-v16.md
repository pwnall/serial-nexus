# serial_nexus — Design Document

**Status (2026-08-07):** Implemented and validated through the 0.3.0 release mark, including a real
Tier-3 hardware rig (the tier ladder is §13's) on both kernels, a Linux replug lane driven by the
repository-carried privileged helper (§15.45), and the cross-kernel doctor campaigns whose
committed artifacts under `docs/doctor/` back every kernel claim below. §1–§14 and §17 are
normative for the system as built; §15–§16 are the decision record. Measured figures — suite
counts, gate scopes, wall-clock costs — live in exactly one place, the plan's Status table, and are
quotable only with the scope recorded there; this document deliberately carries none. What remains
open is what plan §18 enumerates, and nothing else.

This v16 generation was produced under rewrite invariant 2 below: the v15 text plus intended
changes only, enumerated in the notes' generation entry — the post-v15 measured record restated
as primary text at its sources, contracts rendered as numbered clauses, the decision record
compressed under status headers with every number frozen, two new chapters (validating your
codec, §8; instrument validity, §13), the simplified citation rule below, and every current-era
figure moved to the plan's Status table. Nothing recorded as declined or refuted has been
silently reversed.

**Scope:** A daemon and CLI client for managing serial ports as a graph of data-routing nodes.

**Targets:** Rust edition 2024. Linux is required; macOS is best-effort; Windows is out of scope
(§13, §15.13).

**Names:** The project is `serial_nexus`; the daemon is `serial-nexus-daemon` and the CLI is
`serial-nexus-ctl`.

**Citations:** A bare `§N` anywhere in this repository cites this design document — everywhere,
including inside the implementation plan. The plan spells `plan §N` even for its own sections,
so a bare section number never means the plan; the implementation notes are cited as
`notes §3.NN` and the operating manual (`AGENTS.md`) as `AGENTS §N`. The v15 pair carried
a scoped exception — bare `§N` inside the plan named the plan's own sections, with design
references spelled `design §N` there; it is retired this generation, because a notation that
must be remembered is a notation that will drift — the rule's absence once produced a
forty-site defect class of plan-track numbers written as design citations (review 37,
37-WEBC-8). Both documents state this rule in their front matter; inside this document a bare
`§N` is an ordinary self-reference, matching the tree-wide meaning.

**Rewrite invariants.** Three invariants bind every regeneration of this document pair, including
the generation that produced this text. They are front matter because they are rules about the
document set itself, not about the system:

1. **Anchors are stable.** An anchor cited by immutable evidence — committed `docs/doctor/`
   artifacts, code comments, gate files, the frozen reviews — is never renumbered and never
   reused. Concretely: the design keeps top-level numbering §1–§17 and the §7.1–§7.8
   sub-numbering; §15 and §16 entry numbers are append-only (a retired entry becomes a stub,
   gaps are allowed, a number is never reassigned); the plan keeps §1–§18, and plan §3 rule
   numbers and plan §18 item numbers are append-only.
2. **Every generation is an alignment pass.** A new generation lands as the prior text plus
   intended changes only, with the intended changes enumerated in the implementation notes'
   generation entry; the acceptance test is that the diff against the predecessor contains
   nothing unintended. The rule exists because two consecutive generations once regenerated from
   stale bases and each silently dropped rules the code still enforced. This generation executed
   it as a per-section digest fan-out with a must-preserve sweep, recorded in the notes.
3. **Nothing recorded as declined or refuted is silently reversed.** A decline or refutation is
   re-opened only by a new decision that names the record it overturns (AGENTS §5). Silently
   re-fixing a declined item is a defect — and so is quoting a decline that a later entry
   overturned.

## How to read this document

This document records both the design and the reasoning, in that order.

- **§1–§14 and §17 describe the system as settled.** A reader who wants only "what are we
  building" can stop there. §17 (the web console client) reads as body and sits directly after
  §14; its high number is rewrite invariant 1's anchor stability, not an afterthought.
- **§15–§16 are the decision record** — one entry per decision, each opening with a status
  line (LIVE, SUPERSEDED-IN-PART, OVERTURNED, NARROWED-BY, DECLINED — STANDS, EXECUTED), each
  number frozen forever. §15 holds the design history; §16 the post-completion reliability review. Current
  truth is stated once, post-correction; history sits below it. Treat the record as normative
  context for future changes: it is where every decline and refutation lives, and rewrite
  invariant 3 makes those binding.
- **The implementation plan** — the companion document, cited `plan §N`, deliberately not named
  by filename here (the entry points that do name the pair are enumerated in the plan's front
  matter) — records how the system was validated and what is open: plan §3 is the harness
  doctrine, plan §18 the work ledger, and the plan's Status table the only home of current
  measured figures.
- **The implementation notes** (`docs/implementation-notes.md` §3) are the append-only deviation
  and measurement record the whole tree cites as `notes §3.NN`.

Citations of the form `review N, N-AREA-K` (and bare finding ids like `RES-2`, `CORE-2`) name
findings of the frozen adversarial reviews; the reviews and their remediation ledgers live under
`docs/` and `docs/historical/` (AGENTS §5). Kernel claims throughout cite doctor probes and
committed artifacts; §13 defines the instrument — skim it first if the P-numbers and shas bother
you.

Five satellite documents carry contracts this document states but does not duplicate:

- `docs/rpc/` — the schema authority for every control-plane verb and notification; §10 states
  the contracts, `docs/rpc/` states the shapes.
- `docs/codec-authors.md` — the codec author's guide; §8 is the contract it teaches.
- `docs/security.md` — the web console's exact pre-auth constants and measurements; §17 states
  the bounds, this file the numbers.
- `docs/macos.md` — the macOS platform record: measured deltas, per-test observations, and the
  history behind §13's platform matrix.
- `docs/serial-nexus-doctor.md` — the probe roster and the one-shot field-report protocol behind
  §13's measurement doctrine.

## System overview

The reference configuration (§2) exercised every design decision and is the fastest way to hold
the whole system in one's head. A device exposes one physical serial port carrying several logical
streams interleaved by a device-specific framing protocol; two daemons — one wired to the device,
one where the operators sit — turn that port into named, shareable, loggable, remotely forwarded
consoles:

```
computer A (wired to the device)
--------------------------------
device == UART == [serial] --- [codec, demux] --+-- "main"   --+--> [map] --> [pty] --> terminal
                                                |              +--> [log] --> append-only file
                                                |              +--> [leg channel] -----+
                                                +-- "coproc" --+--> [pty]              |
                                                |              +--> [log]              |
                                                |              +--> [leg channel] -----+
                                                +-- "trace"  ----> [log], [leg channel] ...
                                                                                       |
                                             one TCP or Unix connection, SSH-forwarded |
computer B (where the operators sit)                                                   |
------------------------------------                                                   |
[leg, listen] <------------------------------------------------------------------------+
      |
      +-- announces channel identities; each bound channel fans out again:
          "main" --> [pty] + [log]      "coproc" --> [pty] + [log]      "trace" --> [log]
```

Node kinds, briefly: a serial node (§7.1) owns the physical port; a codec node (§7.5) speaks the
device's framing and splits it into named channels; pty (§7.2), log (§7.3), and map (§7.8) nodes
consume channels; a leg node (§7.4) re-multiplexes every channel over one socket to the peer
daemon. Bytes moving toward the device travel *targetward*; bytes moving toward its consumers
travel *hostward* (§3 defines the vocabulary). Every edge carries data both ways — direction
encodes topological role, not flow (§3) — so the same graph that fans a console out to
terminal, log, and remote daemon carries the operator's keystrokes back, and §2 records why
demux-then-remux is the point, not overhead.

Four ideas carry the load:

1. **Everything is a typed graph.** Nodes process bytes and expose typed endpoints; an edge joins
   exactly one host-facing and one target-facing endpoint (§3, §4). Fan-out is implicit at
   host-facing endpoints — there is no tee node — while a target-facing endpoint accepts exactly
   one edge, so unrestricted reading is safe by construction and merging streams is always
   explicit, through a framing node, never a raw interleave (§4's one-producer invariant).
2. **Policy lives only at boundaries.** Interior nodes are queue-free and policy-free; every
   queue, counter, and drop decision sits at a boundary, where bytes enter or leave the graph
   (§5). Hostward delivery is lossy with counted drops — a slow consumer costs itself, never its
   neighbors, and never the device. Targetward delivery is backpressured and lossless: a full
   path answers Busy back to the origin, which stops reading; commands are delayed, never
   dropped.
3. **Operators own the graph; the environment owns only state.** Configuration operations fail
   only on structural invalidity; an absent, replugged, or misbehaving device never changes the
   graph — it changes node state, visibly, and heals alone (§3, §11). Device identity is
   captured once and re-resolved at every open, so operator intent survives replugging,
   renumbering, daemon restarts, and cold starts with the hardware unplugged (§12).
4. **Everything is measured.** The doctor probes every kernel behavior the design depends on,
   and kernel claims cite committed artifacts, never terminal scrollback (§13, §16.13). A kernel
   that differs is `degraded` with the observation named, never `unsupported` (AGENTS §7). The
   harness doctrine (plan §3), the instrument-validity rules (§13), and the decision record
   itself — refutations and declines included — are part of the system, not commentary on it.

Where the code lives, in six lines:

- Libraries: `core/` (graph model, config/state types), `rpc/` (JSON-RPC wire types and the one
  socket-path implementation), `sys/` (the only crate carrying `unsafe` — §16.3), `codec-api/`
  (the codec trait and envelope), `codecs/reference/` (the reference codec).
- `daemon/` is the daemon as an embeddable library (`serial_nexus_daemon`); `daemon-bin/` builds
  the thin `serial-nexus-daemon` binary (§15.26).
- `ctl/` and `web/` are pure RPC clients — the CLI and the web console; the daemon links neither.
- `sim/` is the subprocess test double, `doctor/` the kernel-capability prober, `replug/` the
  privileged USB re-enumeration helper (§15.45).
- `itest/` is the cross-platform integration harness that boots all of the above as subprocesses.
- Plan §2 carries the full directory → package → artifact table, the non-crate directories, and
  the naming rules.

## 1. Problem

Working with embedded targets over serial links looks simple — open `/dev/ttyUSB0`, run a terminal
program — until several realities collide:

**One physical serial port often carries several logical streams.** Devices with a hardware or
firmware multiplexer present multiple consoles, trace channels, and control channels interleaved
on a single UART, framed by a device-specific protocol. Nothing off the shelf splits that port
into separately usable streams.

**Every interesting stream has more than one consumer.** The same console needs an interactive
terminal for the operator, a permanent log for forensics, and a forwarded copy on another machine —
simultaneously, without the consumers interfering with each other or with the device.

**Streams must cross machines.** The lab machine physically wired to the devices is rarely the
machine where people work. Streams need to travel over TCP or Unix sockets to a second daemon that
re-exposes them as local pseudo-terminals and logs.

**Writers must not collide.** When two people (or a person and a script, on the same or different
machines) write to one console, their bytes interleave at arbitrary boundaries and corrupt whatever
line- or packet-oriented protocol the device speaks. Device maintenance requires disciplined,
exclusive write access with an escape hatch for stealing it.

**Ports come and go.** USB serial adapters disappear and reappear, and the same adapter does not
always return as the same `/dev` path. Operator intent must survive replugging, daemon restarts,
and device power cycles.

Existing tools each solve a slice: ser2net exposes ports over TCP, socat plumbs ad-hoc pipelines,
conserver manages shared console access and logging. None of them compose demultiplexing, PTY
fan-out, per-stream logging, re-multiplexing, and cross-machine forwarding under a single
operator-owned configuration — and all three are copyleft-licensed, which this project may run as
external tools but not link (§13). serial-nexus-daemon is a permissively-licensed daemon that
composes all of the above as one explicit, inspectable graph.

## 2. Illustrating use case

This scenario exercised every design decision and is the reference configuration for the rest of
the document.

A device exposes one physical serial port carrying a hardware multiplexer: several logical serial
streams (main console, coprocessor console, trace output) interleaved by a protocol the operator
knows. Computer A is wired to the device and runs serial-nexus-daemon with this configuration:

- a **serial port node** (§7.1) owns the physical port;
- a **codec node** (§7.5) speaking the device's multiplexing protocol — supplied as a compiled-in
  codec with configuration attributes, or via the exec codec escape hatch (§7.6) — demultiplexes
  the port into named **channels**;
- each channel fans out to a **PTY node** (§7.2; an interactive pseudo-terminal for local
  operators), a **log node** (§7.3; an append-only file with on-demand rotation), and one channel
  of a **leg node** (§7.4) — a socket transport that re-multiplexes all channels over a single TCP
  connection;
- consoles with character quirks — bare-LF output, CR-expecting input — route through an optional
  per-console **map node** (§7.8) between the channel and its consumers, so each UART's
  peculiarities are written into configuration once instead of being re-negotiated at every
  terminal, log reader, and remote session.

Computer B, where the operators actually sit, runs serial-nexus-daemon with the mirror
configuration: a leg node accepts the connection (bound to localhost on both ends and carried over
SSH forwarding), announces the channel identities across the wire, and each bound channel fans out
again into B-local PTY nodes and log nodes.

Maintenance works from either machine: an operator grabs the exclusive write lock on a channel,
types into its PTY (or lets a script drive it), and releases. The write lock, backpressure, and
purge rules (§5, §6) guarantee that concurrent operators cannot interleave bytes and that stale,
buffered commands never fire into a device that has rebooted in the meantime. When a second device
is wired to computer A, its channels join the same leg, and computer B sees them appear as
additional channel identities.

The apparent redundancy — demultiplex, then re-multiplex — is the point: once streams are
first-class channels inside the graph, forwarding, logging, spying, and write arbitration apply
uniformly to every stream regardless of how it entered the system.

## 3. Concepts and terminology

This vocabulary is used throughout both documents without re-introduction.

**Target system and host system.** Every stream in the graph runs between the *target system* —
the device under control — and the *host system* — the world of consumers: terminals, logs,
sockets, remote daemons. These are semantic anchors, not physical ones: on computer B the target
lies across a network hop, and a simulated device behind a pseudo-terminal is a target with no
hardware at all (§15.3 records the hardware-anchored naming candidates this refuted). *Targetward*
and *hostward* are the relative directions — what casual usage calls upstream and downstream — and
are the required direction vocabulary throughout the repository.

**Nodes, endpoints, edges.** The daemon manages a graph. *Nodes* process data and move it in and
out of the system. Nodes expose typed *endpoints*; an *edge* connects exactly two endpoints and
carries a bidirectional byte stream. Edges terminate on endpoints, never on nodes: a
demultiplexer's outgoing edges each carry a *different* stream, so "which channel" must be
structural rather than an ad-hoc edge annotation (§15.2 — the decision upstream of nearly
everything else here). *Channels* are endpoints with identities. Edge direction encodes topological
role, not data flow: data flows both ways along every edge (a console both prints and accepts
input).

**Facing.** Every endpoint declares an orientation: it *faces target* or *faces host* — "faces"
meaning looks-toward along the target–host axis. A valid edge always joins one host-facing
endpoint to one target-facing endpoint. Boundary nodes face inward: a serial port node's endpoint
faces host (it offers the device's stream to consumers); PTY nodes and log nodes face target (they
look back toward the device); a codec node faces target on its multiplexed side and host on its
channels. Dual-role node kinds — leg nodes, existing-terminal nodes (§7.7; in the model, refused
at load — §14), and serial ports used as outputs (§7.1; likewise refused at load — §14) — carry a
`faces` configuration attribute. In the reference configuration, computer A's leg faces target
(it consumes local channels for transport) and computer B's leg faces host (it offers the arriving
channels to local consumers).

**Schema vocabulary.** The orientation vocabulary carries collision hazards, resolved as
mechanical word bans rather than left to taste (§15.3) — a rewrite that "normalizes" any of these
reintroduces the collision it exists to prevent:

1. The schemas spell `address`, never `host`: the natural field name for a socket peer collides
   with the host-system anchor.
2. The words `source` and `target` never appear in the schema. Graph-theory edge terminology calls
   an edge's two ends its source and target; both words are banned because `target` is the anchor
   of the whole direction system, and an edge simply references its two endpoints.
3. Flow-connoting endpoint names — source/sink, producer/consumer, input/output — are refused
   throughout: data flows both ways on every oriented edge, so a flow-relative name is wrong for
   half the traffic (§15.3; the same rejection is restated for the map node's direction naming,
   §7.8).

**Configuration and state.** Everything the daemon knows is either *configuration* — desired,
operator-owned, round-trippable — or *state* — observed, environment-owned, reportable but never
persisted. The governing invariant: **operators own the graph; the environment owns only state.**
Configuration operations fail only on structural invalidity; environmental failure (a missing
device, an unwritable directory) never changes the graph, only a node's state (§7, §11).

**Names and identities.** Nodes have operator-chosen *names*, used for configuration addressing
and CLI verbs. Channels have codec-scoped *channel identities*, which are the names that cross the
wire between daemons. The display form `node/channel` combines them for human output; neither is
derived from device paths. Three legality rules bind both — a name that cannot do its job is a
structural validation error, never an operator's problem to discover at runtime:

1. Neither may contain `/`: the display form and the on-disk address encoding depend on it.
2. Neither may be empty or whitespace-only (§11; §12's spelling rule for identity fields, applied
   to names).
3. Both are bounded in length, under the stated, structurally checked maximum every wire-riding
   identifier carries (§11).

The length bound is not cosmetic, and its rationale is normative. A channel identity rides in the
header of *every* frame that carries its bytes (§9), so the per-frame payload is the frame size
minus the header minus the identity; an unbounded identity drives that to zero and leaves the
targetward fragmenter unable to place a single byte — the one way §5's fragment-never-drop
obligation degenerates into counting loss it could have prevented. A generous cap on a
human-chosen label removes the failure mode by construction.

Addressing follows from the model: a bare node name addresses the node's *default* endpoint — a
serial node's single endpoint, a codec node's multiplexed side, a map node's host-facing side —
while `node/channel` addresses a named channel. A leg node has no default endpoint; its socket
lives off-graph (§15.22, §15.24).

**Origins.** An *origin* is a hostward boundary through which bytes enter the graph traveling
targetward: a PTY with a client attached, an accepted socket connection, the CLI's `send` verb, a
remote daemon's leg. Write arbitration (§6) and backpressure (§5) both act on origins.

**Boundary nodes.** Nodes where the graph touches a kernel object with its own finite buffer and
independent consumer: serial ports (both directions), PTY masters, sockets, files, and the exec
codec's child stdio pipes (§15.22). All buffering, dropping, and flow-control policy lives at
boundaries; the graph interior is policy-free (§5).

## 4. Graph model

The graph obeys three structural rules, checked at load and on every incremental operation:

1. Every edge joins exactly one host-facing endpoint to exactly one target-facing endpoint.
2. A host-facing endpoint may have any number of attached edges; a target-facing endpoint has
   exactly one.
3. The graph is acyclic.

Rule 2 carries most of the semantics. Fan-out is implicit at host-facing endpoints: hostward data
arriving at the endpoint is broadcast to every attached edge, and targetward writes from the
attached edges are arbitrated by that endpoint's write lock (§6). There is no tee node; "attach
the PTY, the log, and the leg to channel 2" is expressed directly as three edges on one endpoint.

The single-edge rule on target-facing endpoints enforces the **one-producer invariant**: behind
every endpoint sits exactly one source of hostward data, which is what makes unrestricted reading
safe by construction. The dual failure mode — a diamond, one stream fanned through two paths that
reconverge, duplicating hostward delivery and doubling targetward writes into the device — is
unrepresentable rather than merely lintable, and accidental duplicate delivery would require a
node whose documented job is duplication (§15.4). **Deliberate:** implicit merging of two streams
into one consumer is unrepresentable too, a cost accepted with open eyes — merging is always
explicit: the multi-input node doing it (a codec, or the future labeled-combiner — §14) frames its
inputs rather than interleaving raw bytes (§15.4).

**One misreading of rule 2 recurred often enough to forbid here:** the rule bounds *configured
edges*, never runtime residue — a target-facing endpoint whose only edge was just disconnected may
still hold an undrained inbox, and the rule says nothing about that inbox's fate (§5's wiring
invariant does: not attached parks, because targetward is the direction §5 forbids dropping on).

Data flows both directions along every edge; some endpoints simply never exercise one direction (a
log node never writes targetward). Symmetric configurations — the daemon bridging two physical
devices as a virtual null modem or man-in-the-middle — are legal; the operator declares one side
the target, and the docs state plainly that the choice is a labeling judgment, not a property the
daemon can infer (§15.3).

## 5. Data plane

The data plane moves bytes in two directions under two different promises. Hostward — from the
device toward its consumers — flow is lossy at boundaries, with every loss counted where it
happens. Targetward — from consumers toward the device — flow is backpressured to the origin,
and nothing is dropped. Everything below is one of those two rules, an accounting obligation
that keeps the first rule honest, or a structural invariant that keeps the second one true
across wiring changes and teardown.

### The 3-wire assumption

The design assumption is the modern common case: 3-wire UART (TX/RX/GND), no flow-control
lines. Consequently no end-to-end flow control exists toward the host — the device transmits
whether or not anyone listens — while real, persistent bottlenecks exist toward the target (the
port drains at line rate). The data plane is built around that asymmetry.

The assumption is a default, not a constraint:

1. XON/XOFF and RTS/CTS remain ordinary port attributes. When a port does have flow control
   configured, the kernel pausing transmission surfaces as Busy (see the targetward group
   below), so hardware flow control transparently extends across the graph to remote writers —
   proven on the wire, not only in the model: over the crossover rig, a `flow = "none"`
   transmitter delivers through a CTS stop and an `rts-cts` one never does (notes §3.63).
2. Whether the driver *honours* a configured mode is measured, never assumed, by the one shared
   predicate `sys::honours_rtscts`; a driver that accepts `CRTSCTS` and silently drops it is
   refused at `load`/`add-node` (§15.53). The per-mode measurement status, the kernels of
   record, and the `xon-xoff` gap are §7.1's contract (clause 7) — the `xon-xoff` half is a
   measurement debt (plan §18 item 14), not a §14 deferral.
3. The 3-wire default is also why the rig-flow lane's precondition is measured rather than
   declared: a bench that answers "no flow-control wires" is a legitimate rig under this
   section's own assumption (§15.52; plan §3).

### The interior contract

1. Interior nodes are queue-free and policy-free.
2. An interior node may hold parser state (a partial frame, bounded by the codec's frame size)
   and a single-chunk holdover slot per direction — never queues. "Queue-free" means no policy
   buffering: the wiring layer's fixed-capacity per-endpoint channels (see the wiring invariant
   below) are backpressured plumbing, and their occupancy is backlog, never loss.
3. All policy — buffering, dropping, pausing — lives at the boundary types: serial ports, PTY
   masters, sockets, files, and child stdio pipes (§15.22).

### Hostward flow is lossy at boundaries

1. `deliver(chunk)` hostward is infallible and immediate: interior nodes transform and forward
   synchronously in the caller's context.
2. Host-facing endpoints broadcast to all attached edges.
3. Each consuming boundary applies its own policy when its kernel object cannot accept data —
   bounded buffering where configured, then counted drops.
4. A slow spy costs itself data, never its neighbors.

### Targetward flow is backpressured to the origin

1. `deliver(chunk)` targetward returns Accepted or Busy and never blocks.
2. Busy propagates synchronously back to the origin, which stops reading its own kernel object
   until the path drains: the TCP window closes, the PTY client's write blocks — the kernel
   buffers on the client's side of the fence, and nothing is dropped. Commands are delayed,
   never lost.
3. A transform that has already emitted output when downstream refuses parks it in its holdover
   slot, capping interior memory at one frame per node. Boundaries announce writability, and
   the runtime drains parked holdover frames on that signal, independent of any new origin
   input, so no frame can be stranded behind a quiescent origin.
4. On the wire (§9, protocol v1) channels share one socket with no per-channel flow control, so
   a stalled peer wedges every channel's targetward flow together. Head-of-line blocking is the
   designed behavior, pinned by test as the SUM of the targetward counters freezing while
   hostward checksums keep advancing (`itest/tests/p6_head_of_line.rs`) — the sum deliberately,
   because under a fully stalled peer whichever channel wins the race wedges the shared socket
   and the other can legitimately sit at 0, itself a head-of-line manifestation, so a
   per-channel assertion would have been the wrong pin. A future per-channel-flow-control
   substrate visibly changes this pin.

### The loss taxonomy

Loss by design has repeatedly been misread as loss bugs: one review round's harness rewrite
read bounded hostward loss as data-loss bugs in nine tests, and a reconnect's hostward flood
was once read as contamination when purge-on-reconnect is deliberately targetward-only
(§15.25). Read this table before writing or reviewing any test that counts bytes. It
classifies the counter *families*; `docs/rpc/observation.md` is the authoritative per-kind
enumeration and stays so.

| Counter family | Direction | Event class | What it means |
|---|---|---|---|
| `dropped_slow_consumer` | hostward | sanctioned boundary loss | A consuming boundary's bounded buffer overflowed; the slow consumer paid, its neighbors did not. |
| `discarded_unattached` (pty: `discarded_no_client`) | hostward | counted ingest discard | The device transmitted while no graph consumer was attached; ingest reads and discards rather than overflowing the kernel (see the serial-ingest group). |
| `discarded_at_last_close` (pty) | hostward | session-end loss | Delivered bytes destroyed by the session ending — a third mechanism, never folded into either neighbor above (§7.2). |
| `feed_dropped` | hostward mirror | spy drop | The tap/ring mirror is a spy *outside* the graph; its drops never touch graph accounting. |
| `dropped_bytes` (log) | hostward | file-boundary overflow, or a stopped writer | The regular-file exception's bounded queue overflowed, or the writer stopped for good (§7.3). |
| §6 per-origin `purged`, `purged_on_reconnect` | targetward | sanctioned purge | A deliberate discard when the write floor settled or a rebooted device returned (§6) — never delivery loss, and never summed with the loss counters. |
| `discarded_at_teardown` | targetward | teardown loss | Bytes destroyed with their node; `0` on any node still visible in `state` (see the teardown ledger below). |
| driver overrun (`TIOCGICOUNT`) | hostward, below the daemon | kernel loss | The driver overran before the daemon ever saw the bytes; surfaced beside the daemon's counters so the layer that lost them is named (§7.1). |
| `framing_errors`, `demux_errors`, `multiplexed.discarded_hostward` (codec/exec) | hostward | transform refusal | The transform's own resyncs and the daemon's demux-refusal accounting (§7.5, §8); `framing_errors == 0` is not health (§8). |
| `discarded_targetward` (codec/exec), `discarded_no_raw_edge` (map), `discarded_unframable`, `discarded_peer_gone` | targetward | pump-side discard | A pump looked at targetward bytes and could not place them: the edge is read-only or detached, the framer refused an oversize unit, or the peer left. Sanctioned where the wiring invariant's *drains-and-counts* arms apply (§15.54), and located rather than silent — which is the whole of §5's demand on it. |
| `timed_out` (sim verdict) | — | harness flag, not a counter | The sim marks deadline expiry so a deadline is never read as a drop (plan §3). |

The loss fingerprint: a hostward shortfall matching `received + dropped_slow_consumer == sent`
is the lossy-boundary signature, not a data-loss bug; a test that requires a large hostward
stream to arrive complete provisions `hostward_buffer` explicitly and cites this section
(plan §3). And targetward *delivery* loss has no counter in steady state, by construction:
queued bytes on a surviving node are backlog, never loss. What can move for targetward bytes on
a surviving node is the purge family (§6), the teardown ledger below, and the pump-side discard
row above — and nothing else. Anything that does not fit the fingerprint or one of the classes
above is a defect, and the taxonomy is what makes that claim checkable. *(This paragraph read
"targetward has no loss counter in steady state … the only counters that can ever move …are the
purge family and the teardown ledger" until 2026-08-12. Four shipped counters falsified it on a
healthy graph, and the table had no row for them — an absolute that made its own instances
unreadable rather than a rule anything enforced. §15.54, notes §3.75.)*

### The fragment-never-drop obligation

One obligation binds *every* component that frames targetward bytes — the wire, the envelope,
and in-process codecs alike: an oversize producer chunk is fragmented, via the one shared
helper, never skipped on encode error. This sentence exists because the skip-on-error variant
of that bug shipped three times in three framers before review caught the last one (§15.27);
the invariant is per-writer, not per-protocol.

### Serial ingest never blocks and never idles

A configured serial node holds its port open (§7.1) and reads continuously. When nothing is
attached, data is discarded with a counter rather than left to overflow the kernel: the
alternative — not reading — fills the driver buffer, drops on overrun anyway, and greets the
first consumer with a stale burst followed by a gap. Driver overrun counters (`TIOCGICOUNT`,
where supported) are surfaced in state alongside the daemon's own discard counters, so loss is
always visible and attributable — the promise the teardown ledger below extends to the moment
a node ceases to exist.

### The replay ring

1. Every host-facing endpoint carries a replay ring by default — `replay_ring = 65536` unless
   overridden, `0` to disable (§15.32) — a bounded ring of the most recent hostward bytes,
   retained solely so a late attacher sees what just happened: conserver's
   attach-and-see-the-panic feature, graduated from §14 with the web console (§17) as its
   driver, and the default because a console without scrollback is a console that punishes
   arriving late.
2. The cost is stated, not hidden: bounded memory per endpoint (64 KiB × a lab box's fan-out is
   a few MiB) and one mirrored copy of each hostward chunk into the tap feed — re-verified
   against the §15.19 throughput bar, never backpressuring the device.
3. Tripwire: the ring is bulk-copy circular storage, because a per-byte structure on this
   now-universal hot path is a throughput collapse — the benchmark caught exactly that when a
   byte-at-a-time deque first shipped, and the fix was the data structure, not an opt-out.
4. Tripwire: the ring is a feature buffer, explicitly not flow control, and it never
   substitutes for a log node — mechanically guaranteed by the accounting doctrine the audit
   forced: the tap/ring mirror is a spy *outside* the graph, with its own `feed_dropped`
   counter, and `discarded_unattached` counts graph-consumer absence independent of it, so a
   ring can never silently hide loss beyond its depth.
5. A tap opened with replay receives the ring snapshot and then the live stream with an exact
   splice — no gap, no duplication — cheap to guarantee because snapshot and attachment happen
   inside one critical section on the runtime thread (§15.20's two-lane model doing double
   duty). The offset, epoch, and instance machinery that makes the splice exact for a
   reconnecting client is §10's contract.
6. Client-side retention is bounded by *consoles*, not by daemon restarts, and a same-epoch
   `from_offset` past the stored frontier is a real hole the client marks rather than splices
   over — two clauses that are load-bearing and easy to get wrong in opposite directions; the
   full retention contract, including the boot-sweep and the marking rules, is §17's
   (Browser-side history; §15.32).

### Files are the exception that proves the boundary rule

Regular-file writes cannot be made non-blocking (`O_NONBLOCK` is a no-op on them), so each log
node owns a bounded queue feeding a dedicated writer task, with an overflow policy of
drop-oldest-with-counters or fault-the-node — triggered in practice by full disks and slow
network filesystems. The log node's contract, including its two defect-bought failure
semantics, is §7.3.

### The wiring invariant

Promoted from the graph-editing track (plan §14), where it was learned; it is the thing to
know before touching the data plane.

1. A target-facing endpoint's channels, counters, and origin slot are **per endpoint and
   permanent**, never per edge: a `FanOutList` the producer re-reads per chunk, an `EdgeInbox`
   its consumer's pump loops over, and an `EdgeSlot` re-read per chunk (§15.23).
2. Wiring changes are data mutations, never task restarts. Restarting a node's tasks would
   drop the targetward receiver out from under senders that stay live in
   `GraphState::endpoint_targetward` and in every writer origin — the failure chain review
   37's MAP-1 named, and the reason live edge surgery (§10's `connect`/`disconnect`) is
   possible at all.
3. The three states an endpoint can be in behave differently on purpose: attached-and-writable
   forwards; **attached but read-only** drains-and-counts, because parking would wedge a
   writer on a configuration that will never become writable; and **not attached** parks —
   because this section forbids dropping targetward, and a detached edge stalls its writers
   exactly as a steal does (§6) — **where the pump serves one endpoint**. Where one pump
   serves many, it drains and counts instead: parking would stall traffic that has nothing
   to do with the detached edge. Which pumps those are, and why the choice is per node and
   not shared, is **§15.54**; the counters it charges are the taxonomy's pump-side discard
   row below.

### The teardown ledger

Loss is charged at destruction, or the silence is named (§15.50). Teardown once destroyed
bytes no counter had ever seen — the one place §5's visible-and-attributable promise failed
was exactly where a node ceases to exist and `state` can no longer report it.

1. An interior node's queued targetward bytes are drained and counted at teardown:
   `discarded_at_teardown`, carried per kind in `state` and in the `remove-node` and
   `teardown` replies, because the reply is the only place a destroyed node's last loss can
   land.
2. The counter reads `0` for a node's entire working life and moves exactly once, at stop. On
   a surviving node, queued bytes are backlog, never loss.
3. The drain runs *before* task abort — abort drops the future the queue lives in — is
   synchronous and idempotent, and a mid-flight chunk is counted with a deliberate over-report
   toward loss: the ledger errs toward reporting loss, never toward hiding it.
4. The invariant is **two-sided, and only the pair is the invariant**: *the queue never leaves
   the node's slot, and no byte that has left the queue crosses an `.await` uncharged*. Each
   half was bought by its own reproduction of the same silence — first receivers moved into
   spawned futures (notes §3.31), then a drain that charged an accumulated local after its own
   yields, inside the very future abort drops (notes §3.59; the reproductions are in §15.50).
   The shared drain, `TargetwardInbox::purge_to_quiescence` — the daemon's single statement of
   the purge policy, through which §6's purge instances also drain — therefore takes its
   charge as an argument and charges per round, before it yields, and returns nothing: the
   accumulated return value *was* the defect, and removing the return type forbids its
   reinvention.
5. Never sum: `purged_bytes` and `discarded_at_teardown` are different losses and must never
   be summed.
6. `remove-node --cascade` also reports `released_locks` and `purged_bytes`, always present
   including `0`/`0` — the same facts `disconnect` reports for the identical edge (review 37,
   37-LIFE-1: the same removal was loud through one verb and mute through the other).
7. `load --replace` is the third destroying verb, and the one whose loss is largest, since it
   destroys the whole graph; its reply carries the ledger's fields under §11's
   replies-account-for-what-they-destroy contract.
8. A conservation law guards the ledger: destroyed + purged + pty-buffered equals queued,
   asserted as an equality rather than a threshold — the harness makes "in flight"
   deterministic by acknowledging every `send` before the teardown (plan §3).
9. Scope, and the residuals named at the counter (`docs/rpc/observation.md` enumerates
   coverage per kind): `map` and `codec` charge their whole host-facing targetward exposure;
   `serial` charges the backlog a `waiting` node accumulates — the deepest one the daemon
   legally holds; `leg` charges per channel *and* summed on the node, because loss must be
   attributable and one number for eight channels says what was lost without saying where.
   **Deliberate:** a `faces = "target"` leg's per-channel hostward relay is excluded —
   charging it here would report a hostward loss under a targetward name (notes §3.55).
   **Limit, named and open:** `exec`'s figure is a floor, not a total — its internal merge
   stage is beyond the handle's reach, so a torn-down `exec` can destroy more than it reports,
   never less — ledgered open work (plan §18); the pty's held `pending` payload is not fixable
   the same way and is recorded, not re-filed, in the ledger's closing register. Both are
   stated here rather than left to be discovered by someone diffing the counter against the
   conservation sum.

### The hybrid architecture

The data plane is a hybrid (§15.18, §15.19). Control, coordination, and low-rate paths —
targetward flow, presence and termios polling — run on one async thread, with readiness for
tty-family descriptors driven by non-blocking `poll(2)` under an adaptive idle backoff rather
than epoll, because epoll misreports pty-master readiness (§15.18; the later measurement by
doctor probe P8 — probes are P1–P15, rostered in §13 and `docs/serial-nexus-doctor.md` —
localizing the starvation to the runtime's readiness-guard lifecycle is annotated there and
does **not** reopen the question; read it before re-litigating). Each high-rate hostward
path — a serial port's reader, a PTY master's writer — runs on a dedicated blocking thread
parked in blocking `poll(2)`, which costs nothing while idle and wakes the instant the fd is
ready (§15.19). The synchronous deliver contract is context-agnostic and executes on whichever
thread originates the data; cross-thread state is limited to atomic counters; socket
boundaries use the async runtime's native socket types, which do not share the quirk. Unifying
the two readiness paths was evaluated and rejected (§16.9) — it would move lock consultation
across threads — and that decline is load-bearing: the split is the architecture, not an
accident awaiting cleanup.

### The model and the shipped path

`serial_nexus_core::data` is the executable **specification** of this section, not the shipped
data path — `TargetwardSink::flush()` exists only in the model, and the daemon carries
anti-stranding via its channel-plus-`send().await` shape (notes §3.3, corrected in place;
notes §3.18). The two-places rule follows: a change to §5 semantics requires edits in both
`data.rs` (the model) and the daemon's node paths, and `data.rs`'s module doc states the
split. The hostward half is unified in `runtime::fan_out`, which charges all-sinks-closed
inside the helper; the targetward half is still per-node, so the rule still stands. A green
model-property run is never data-plane coverage by itself.

### The tripwire table

These invariants have tripwire status: violations die in review (AGENTS §4). Each row states
the rule and names its decision record; where the rule's full contract lives elsewhere, the
body home is cited. The rows are deliberately unnumbered — legacy "invariant N" citations
predate this generation and resolve through the mapping table at the end of §16.

| Tripwire | The rule | Record (body home) |
|---|---|---|
| Critical-section cell | A `RefCell` borrow never crosses an `.await`; `std::cell::RefCell` is clippy-banned in the daemon — use the critical-section cell. | §16.2 |
| AsyncFd ban | `AsyncFd` is banned workspace-wide for pty masters. | §15.18 (this section) |
| Unsafe containment | `unsafe` lives only in `serial_nexus_sys`; everything else is `#![forbid(unsafe_code)]`. | §16.3 |
| Fragmenter | Every targetward framer fragments oversize chunks via the one shared helper; never skip-on-encode-error. | §15.27 (this section) |
| Concurrent pump | A child-stdio boundary's two directions are concurrently-polled futures; a blocked write never starves the read. | §15.22 (§7.6) |
| Purge | Purge is one invariant, three instances; the tap/ring mirror never affects `discarded_unattached`. | §15.25, §15.32 (§6) |
| Wire maxima | Every numeric attribute and every wire-riding identifier carries a stated, structurally checked maximum. | §15.34, §16.12 (§11) |
| Bridge boundary | The web bridge parses exactly one request per frame and forwards only the allowlist; lifecycle verbs stay off it. | §15.34, §15.35 (§10, §17) |
| Sim doubles | Sim doubles are subprocesses, HUP-tolerant, never busy-waiting, idle-CPU asserted. | §15.31, §15.36 (plan §3) |
| Privileged helper | The privileged helper is narrow by construction: one capability, argv-only, no environment read, no `exec` while blessed, and its device argument is a kernel-verified sysfs USB port name, never a path. Giving it a verb that accepts a filesystem path dissolves every one of those bounds at once and is a design amendment, not a patch. | §15.45 (§12) |
| Ring storage | The replay ring is bulk-copy circular storage; a per-byte structure on this hot path is a measured throughput collapse. | §15.32 (this section) |

## 6. Write arbitration

Reading is never arbitrated: the one-producer invariant (§4) makes hostward flow unambiguous, and
every attachment may watch. Writing is arbitrated per host-facing endpoint: among all edges
attached to an endpoint, at most one holds the exclusive write lock, and only the holder's bytes
are read targetward. The lock is implemented as a gate on the §5 pause machinery — non-holders are
simply not read from — so arbitration adds no new data path. Throughout this section the
contender is an **origin** (§3): the writer being granted the floor.

### Write modes

Each edge declares a **write mode**:

1. `never` — the read-only capability. Log edges and spy PTYs; these attachments cannot contend
   for the lock at all. Taps (§17) are its dynamic form: connection-scoped, read-only attachments
   created over the control plane, each with a bounded queue and drop counters per §5, gone when
   their connection closes.
2. `on-demand` — the default for interactive and programmatic origins (PTY clients, socket
   connections, the CLI). Acquisition is explicit via the control plane for named origins, and
   implicit for leg channels: a leg acquires when it has pending targetward bytes and the lock is
   free, and releases after a configurable idle interval or on peer disconnect.
3. `held` — acquire-on-attach, held indefinitely. The demux codec's edge to the serial port holds
   that endpoint's lock permanently, because any other writer would corrupt the multiplexing
   protocol's framing on the wire. Stealing that lock is possible and stalls every channel —
   which is not a limitation but an accurate description of what raw injection does to a
   multiplexed link.

Whether an endpoint arbitrates at all is the `arbitration` attribute: `exclusive` (the default)
or `free-for-all` ("no lock"), which remains for machine-to-machine links coordinated elsewhere.
The attribute is a per-**node** scalar, applied to every host-facing endpoint the node owns and
reported per endpoint in `LockSnapshot` (notes §3.16 — an earlier generation said "per-endpoint
attribute"; no shipped node type needs divergent per-endpoint policy, the lock machinery is
already keyed per endpoint, and a per-endpoint override is additive later if one is ever needed).

### Etiquette, verbs, and addressing

1. The intended workflow for admin operations is grab, write, release — and the two verb
   families name different things (§15.20).
2. `lock`/`unlock` name the **origin**, the writer being granted the floor
   (`serial-nexus-ctl lock console-a`), which the daemon resolves to the one host-facing endpoint
   that origin feeds — unambiguous by §4's single-edge rule.
3. `send` names the **endpoint** (`serial-nexus-ctl send usb0 --line "..."`, or `send mux/ch2`),
   because the CLI is itself the transient origin there: it performs acquire-with-timeout, write,
   and release as one atomic daemon-side operation, joining the waiter queue below like any other
   contender and failing with the locked error at its deadline.
4. Because exclusive is the default, even a sole on-demand origin must acquire before its bytes
   flow — single-console operators either adopt the grab-write-release habit or set
   `arbitration = "free-for-all"` and skip the ceremony.
5. Acquisition happens out-of-band on the control socket because the data streams have no side
   channel, and in-band escape sequences would corrupt binary protocols.

### The purge invariant

The hazard the invariant exists for: because non-holders are paused rather than dropped, a
locked-out client can type into its kernel buffer, get no response, and walk away; when the lock
later frees, minutes-old commands would fire into the device. The cure is safe by construction
for correct clients, which acquire before writing.

All purging is one rule with three instances (a tripwire — see §5's invariant table): bytes
offered while an origin lacked the floor are never delivered — they are drained and counted at
the moment the floor question settles.

1. **Purge-on-acquire (the grant instance).** On explicit lock acquisition, the daemon drains
   and discards, with a counter, anything the origin buffered before the grant. The grant
   instance runs **synchronously at grant time**, inside the same critical section that answers
   the acquire — a lazy drain deferred to the reader races a correct client's acquire-then-write
   first command (notes §3.12).
2. **Purge-on-detach.** An origin that detaches without the lock has its backlog purged and
   counted the same way. This instance covers one thing more than the origin's kernel buffer,
   and it has to: a boundary reader may already have *taken* a payload off the client and be
   holding it because the endpoint cannot accept it yet (§7.2). That payload settles with the
   floor question like everything else — offered once more, then purged and counted — because
   carrying it across the release means delivering a departed console's typing under whatever
   origin holds the lock next, which is the interleave the exclusive floor exists to prevent,
   and on a merely stalled endpoint it is the stale-command hazard verbatim. Seeing the detach at
   all is its own problem on a collapsed pty session (the session-boundary subsection below;
   §15.39).
3. **Purge-on-reconnect** — the one sanctioned targetward drain (§7.1, §7.4). When a device
   reappears, the parked targetward pipeline is drained *to quiescence* — including a chunk held
   by a producer suspended mid-send — before the device goes active again. Bytes still sitting
   in an origin's own kernel buffer remain the grant and detach instances' job, and a
   continuously producing origin is drained only to quiescence, not chased (§15.27). The
   instance's counter (`purged_on_reconnect`) is charged per round, as the bytes leave the
   queue, never as an accumulated local after the drain's own yields — the deferred form let a
   teardown racing the drain read zero on every counter while accepted bytes died (notes §3.59;
   the teardown-accounting ledger this feeds is §5's).

**Principled exemption.** Implicitly-acquiring wire origins are exempt from the grant instance
only, because data arrival *is* their floor request — there is no pre-grant interval to purge.
The analogous hazard for legs and for reappearing serial devices is exactly what
purge-on-reconnect covers.

Two further sharpenings, each once a bug or a near-bug, now contract text beside the
synchronous-grant rule above:

4. **Held-priority reclaim is not a grant and does not purge.** **Deliberate:** the absence of a
   purge here is the rule, not a hole, in both implementations of it (`runtime::reacquire_held`
   and `runtime::may_write_reclaiming`), for three reasons that must survive together: the held
   floor is permanent and a steal only a transient ouster; this section scopes purging to
   explicit acquisition; and a boundary observes the lock freeing at poll resolution, so purging
   on reclaim would discard input typed after the endpoint was already the held origin's again —
   loss §5 does not sanction (notes §3.26). A rewrite that "fixes" reclaim by adding the missing
   purge reintroduces exactly that loss.
5. **The reconnect instance's receiving-side sibling on the leg decides staleness per chunk,
   not per drain.** Each queued chunk carries the connection epoch it arrived under
   (`Inbound { epoch, bytes }`, stamped at enqueue — the only moment provenance is known), so a
   dead connection's backlog is purged exactly and a live connection's chunk queued behind stale
   ones is delivered, not swept (notes §3.25; review 37, 37-LEG-1 — the epoch-after-recv version
   delivered a disconnected peer's entire backlog into the device, deterministically). Do not
   re-generalize the drain helper over the element type and reintroduce a blanket drain: the
   blanket time-proxy drain was the wrong part, not the bound it carried.

### Lifecycle

1. The lock releases on explicit unlock and automatically on origin detach (slave closed,
   connection dropped).
2. `serial-nexus-ctl lock --steal` exists for the hung-script case and is recorded in state so
   the previous holder can see what happened.
3. An optional lease duration bounds a wedged session. Its expiry is guarded by the grant's
   generation, so a stale timer can never release a later grant, and its firing follows the
   normal release path — notification, then grant to the queue head.
4. State reports holder, waiters, and per-origin purge counters per endpoint.

### Waiting and fairness

1. Contention is managed by an explicit FIFO waiter queue per lock, not by racing wakeups. A
   plain `lock` against a held endpoint fails fast with the locked error; `lock --wait` joins
   the queue; `send` joins with its deadline.
2. Grants pass to the queue head on every release path — explicit unlock, detach-release, lease
   expiry, and a stealer's own release — and steal bypasses the queue without destroying it.
3. One class outranks the queue: a `held` origin that lost its lock to a steal reclaims it ahead
   of every on-demand waiter the moment it frees — "held indefinitely" is the promise, and
   granting a queued waiter a demultiplexer's lock would corrupt the very framing the hold
   protects. Fairness in one sentence, in its corrected form: **FIFO among on-demand contenders,
   beneath held reclaim** (§15.23).
4. The reclaim rule is an *acquisition-time* property, not a wake-ordering one: while a `held`
   origin is registered, `acquire` by any other origin is denied even on a free lock, so the
   reclaim wins deterministically rather than by scheduler luck — review found the wake-race
   version, and the fix lives in the pure state machine, where it is property-tested (§15.23,
   §15.27).
5. Two structural corollaries were review-hardened into load-time errors (§15.34): an edge
   feeding a codec's multiplexed endpoint must be `held` or `never` — an on-demand writer there
   can never legitimately win, so accepting the configuration was accepting a dead edge — and
   two `held` origins on one endpoint are refused with both offenders named, since two
   unconditional priorities cannot coexist.
6. **Both corollaries are conditioned on the endpoint arbitrating at all**: a `free-for-all`
   host endpoint has no lock, so `held` and `on-demand` are behaviourally indistinguishable
   there, no writer can park, and neither rule has a hazard to prevent — refusing such a graph
   would reject a configuration that runs, with a stated reason that is false for it.
   **Misreading callout:** the exemption is one predicate consulted by both rules, so it cannot
   be applied to one and forgotten on the other — review 32's `CORE-2` found exactly that split
   in the shipped tree (the second rule already carried it, the first did not).
7. Every grant, immediate or queued, runs purge-on-acquire before the origin's bytes flow.
8. Waiting is cancel-safe: a deadline, a dropped control connection, teardown, or removal of the
   endpoint dequeues the waiter with a defined error, and a cancelled waiter costs nothing but
   its queue slot.
9. Mechanically this is the two-lane control plane of §10 and §15.20: every lock transition is a
   synchronous critical section on the runtime thread, and a waiting verb suspends holding
   nothing — no borrows, no locks, only its place in line.

### Detach-release and the session boundary

Detach-release — the automatic release when an origin detaches (Lifecycle above) — is only as
good as the daemon's ability to *see* the detach, and on a pty endpoint that is not a given:
a session boundary is a transition, not a level, and a
client that opens, writes, and closes inside one poll gap can leave every level observable —
poll revents, `FIONREAD`, queue and modem-line state — exactly as if no session ever happened.
On Darwin that shape held a write lock forever until §15.39 added `serial_nexus_sys::SessionLatch`
(a kqueue knote on the master there, inert elsewhere), whose answer folds into the node's one
`saw_session` predicate rather than becoming a second detach mechanism. The full contract — the
one-predicate rule, the two forge sites that swallow the daemon's own edges, the platform arms,
and why none of this weakens the poll(2)-only readiness rule — lives at §7.2; doctor P12 keeps
the two kernels' mechanisms diffable (§13).

### Limits, documented

The holder is an origin endpoint, not a process: two processes sharing one PTY slave are
indistinguishable to the daemon. Cross-machine exclusion composes by nesting — the remote
daemon's writers contend locally on their side, and the whole remote leg contends as one origin
on this side, with backpressure crossing the TCP link — at the cost of blocking rather than
fail-fast UX for remote contenders; explicit lock request/grant frames are a reserved wire
capability (§9) for later. And poll-sampled presence has a documented blind spot: a client that
closes and a different client that reopens the same slave within one poll interval are
indistinguishable, so the successor inherits the lock on the detach-release path — explicit
unlock is unaffected, exclusion across endpoints is unaffected, and the per-open generation
epoch that would close the window is deferred (§14, recorded beside this consequence).

## 7. Node types

Each node type is specified as: endpoints and facing, configuration (operator-owned, round-trips
through dump/load), state (observed, reported by the `state` verb), and behavior. Common to all
node types:

1. A `name` is configuration; a status of `active | waiting | faulted`, with reason and
   timestamp, is state.
2. Environmental failure faults the node without failing the operation that created it (§3, §11).
   The environment owns node state and only node state; it never edits the graph.
3. A `waiting` status names its cause: the reason carries a `LossCause` distinguishing a clean
   EOF, a read `errno`, and a failed targetward write — an unplug, an `EIO`, and a peer's orderly
   close are different events and the operator sees which. The cause is cleared when a reader is
   re-armed, so a second outage never inherits the first's explanation; it travels through
   status, not a log line, because the reader module deliberately has no logging (notes §3.69).
4. Node kinds that exist in the model but not in the implementation are *refused at load* with a
   structural error naming the deferral — the deferral-state vocabulary and the register of
   everything deferred live in §14.

### 7.1 Serial port node

One endpoint; faces host in the normal role. The target role — a physical port as an output leg
toward another machine's tools — remains in the model but is **refused at load** (§14's
vocabulary), a structural error naming the deferral: review found the schema accepted an
orientation nothing could drive. Codec and leg nodes keep both orientations.

Configuration: device identity in resolver form (§12), termios parameters (baud, data bits, parity,
stop bits), flow control (`none` default, `xon-xoff`, `rts-cts` — kebab-case is canonical and is
what `dump` emits, with the unhyphenated `xonxoff`/`rtscts` accepted as aliases, §15.34), initial
modem-line assertions, and hostward-consumer drop policy — including `hostward_buffer`, the bounded
per-node hostward buffer (a depth in chunks) that plan §3's completeness doctrine provisions.
State: resolved `/dev` path, status, daemon discard counters, driver overrun counters where the
kernel supports them (`TIOCGICOUNT`, surfaced beside the daemon's own counters per §5), and current
modem-line readings.

**Open, hold, and reopen.**

1. The node opens with `O_NOCTTY`, takes `TIOCEXCL` so stray processes cannot share the port,
   applies configured termios and modem lines, and holds the port open for its lifetime —
   open/close toggles DTR on many USB adapters (the classic auto-reset), so line states must be
   deterministic.
2. It reads continuously, discarding with counters when nothing is attached (§5's
   never-block/never-idle ingest rule). On a hangup with bytes still queued, the kernel reports
   `POLLIN` beside `POLLHUP` and the reader drains every byte before the loss is reported — a
   measured premise, pinned at multiple payload sizes with a control proving the hangup really
   fired (notes §3.69). **Deliberate:** the reader's request mask does not add `POLLERR` — the
   kernel delivers the error and hangup bits whether or not they were requested, doctor P9
   measures it (`hangup_delivered_to_a_mask_that_requested_nothing`), and a real unplug answers
   the existing mask with `POLLIN|POLLHUP|POLLERR`, so the addition is a recorded no-op
   (notes §3.69).
3. On read failure the node goes **faulted-and-wait**: hostward flow goes silent, targetward
   reports Busy so all origins pause, and the node polls the resolver (a stat every second or
   two — cheap, portable, and free of libudev's LGPL linkage) until *the same identity*
   reappears. A different adapter squatting on the old path is not adopted (§12).
4. Reopening reapplies configured termios, retakes `TIOCEXCL`, restores modem lines, and by
   default purges origin backlogs accumulated during the outage — the device likely
   power-cycled, and twenty minutes of buffered commands must not fire into its boot prompt —
   with counters and a per-node override. This is the reconnect instance of §6's purge invariant.
5. The order inside the reopen ritual is normative, not incidental: the reopened port is
   **published on the node before the purge**. The purge awaits, and a port the node's own
   ordered release cannot see is a port the `--replace` handover walks straight past — the
   daemon then EBUSYs against its own claim — §15.38's self-EBUSY shape surviving in a second
   place. The reported cost is accepted as the more truthful reading: for the width of the purge the
   node says `waiting` with its device open — exactly what a third party's `open(2)` already
   says — and a signal verb issued in that window is accepted and stays valid into `active`.
6. Serial nodes leave the graph only by explicit configuration operations.

**Ordered release.** Giving back what the node asserted *on the tty* belongs to the port rather
than to whichever exit path someone remembered:

1. Every route by which the node lets go of a device — clean teardown, the `--replace` handover
   (§15.38), a fault, the reconnect ritual, and a failure partway through the open ritual
   itself — returns **every tty-level assertion this node made** before the descriptor goes.
   Today that is two, and stating the rule over the class rather than over `TIOCEXCL` is the
   point: the exclusive claim taken at open, and a break condition a signal verb left standing.
2. Both are tty state, not fd state, and outlive the fd the same way — exclusivity until the
   tty's *last* close, break until *somebody* issues the clearing ioctl, which on a UART a
   `--replace` successor has just inherited is nobody. They are returned break first, so the
   line is transmitting normally before anyone else can open the port.
3. **Deliberate:** DTR and RTS are excluded from the release — driving them on the way out is an
   auto-reset pulse on the boards this whole area exists to protect, and unlike the other two
   they self-heal: a successor's own `open(2)` re-raises DTR and the open ritual reapplies the
   configured levels (review 32 SERX-2).
4. The stakes of clause 1 in one concrete: `TIOCEXCL` clears only at the tty's last close, and
   on a pty-backed device (socat `PTY,link=`, QEMU `-serial pty`, the project's own doubles —
   all legal `raw:` inputs) a held master means that close never arrives. One leaked claim makes
   the device un-openable by every unprivileged process on the machine, permanently, surviving
   daemon exit — with the node's own reason flipping after a reconnect poll to a `Device or
   resource busy` that sends the operator hunting a squatter that is the daemon.
5. The release is **at-most-once**, for the mirror-image reason: an ordered release before a
   successor opens the same tty must not be undone by a late drop stripping the *successor's*
   claim off the tty they share (review 32 RV-8/CONC-4/SERX-1).

**Signal verbs.** `send-break`, `set-modem`/`pulse-dtr` (and RTS), and reading modem lines — the
signals a 3-wire-focused PTY cannot convey (§7.2).

1. The two verbs that hold a line asserted for a duration carry a stated maximum on `ms` — 60
   seconds, deliberately far tighter than §11's general numeric maxima, which bound retry
   intervals rather than hardware: this number holds a *physical* line down (a break garbling every byte
   the device transmits, or DTR parked at a reset level), and it is the one input that makes the
   window in which an in-flight verb outlives its node arbitrarily wide. It is range-checked
   before the port is resolved and before anything is asserted (§11), so three extra digits are
   refused by name rather than learned from a console that never comes back.
2. A signal is **scoped to the port it was issued against**, not to the node's name: if the node
   is torn down, removed, replaced, or simply reconnects while the assertion is in flight, the
   verb wakes at once and declines, and neither the assertion nor its deferred restore reaches
   whatever port the node holds afterwards — an fd and a line state outliving their node into a
   successor's ownership is the same class as the leaked claim above (review 32 CTRL-1/SERX-2).
3. Declining is only half an answer, and it is the half that reads like the whole: a break the
   deferred restore no longer clears is still asserted *on the tty*, so scoping by itself turns
   an `ms`-bounded outage into an unbounded one — the successor reporting `active`, accepting
   every `send`, its driver `tx` counter climbing, the peer receiving nothing until somebody
   destroys the tty. What actually leaves the line transmitting is the ordered release above: a
   bounded assertion is cleared when the port changes hands, by the port, not by whether its own
   verb survived long enough to restore it.

**Flow control is refused at load where the driver drops it (§15.53).** A driver that accepts
`CRTSCTS` and reads the flag back clear has not produced a degraded link; it loses data silently
under exactly the conditions the configuration was written to survive, surfacing — under the old
behavior — as a `faulted` node some time after `load` had already returned success. What §13's
kernel doctrine forbids is the operator learning nothing, and that was the old behavior; what
would be degraded here is not an observation but the transport's *contract*.

1. A config asking `rts-cts` on a port whose driver accepts-then-drops the flag is **refused at
   `load` and `add-node`**, in the same before-anything-is-created position as `precheck_codecs`
   (§11's pre-create precheck contract), and every other node in the config still loads. The
   refusal is structural and carries `node`, `device`, `resolved_path`, `requested_flow_control`,
   and `honoured_on_readback` as data, plus two remedies: `flow = "none"` (or `xon-xoff`) for this
   port, or an adapter whose driver implements RTS/CTS.
2. **One predicate, because two callers must not be able to disagree**:
   `serial_nexus_sys::honours_rtscts` is the only implementation. The daemon's pre-check
   consults it, the harness branches on it, and doctor P15 calls it and requires its answer to
   match the read-back P15 takes by hand, reporting `shipped_predicate_agrees` with its own
   `degraded` arm ranked above the finding itself — a report that calls a port fine while `load`
   refuses it is worse than either verdict alone (§13). Three states and only one refuses:
   `Ok(false)` refuses, `Err` is *unmeasured* and never a refusal, and an absent device never
   reaches the check at all — unplugged hardware still just waits (§12).
3. The pre-check asks its question through `Resolver::resolve_current_path` — the same call the
   node's open makes — so check and open cannot disagree about *which device* is asked either;
   the naive `Path::exists` form was 100% dead on `add-node`, on every `load` of a `dump`ed
   config, and on every startup replay, because the daemon rewrites `device` to canonical
   identity before the check runs (notes §3.68).
4. The bound is stated because the refusal is not total: the check must open the port, so two
   paths still reach `faulted` — a `load --replace` on a port the running graph already holds
   (the outgoing node's `TIOCEXCL` makes the open `Err`, i.e. *unmeasured*), and an adapter
   arriving after the config loads. Both repairs to the *refusal* are declined with reasons
   recorded (§15.53, notes §3.68): re-checking after teardown means a refusal has already
   destroyed the good graph, and inferring from the outgoing node's own successful open is an
   inference, not a measurement.
5. What is not declined, and is shipped: a fault the pre-check cannot prevent must not be
   opaque. On those paths the failed open consults the same one predicate — never error-text
   matching, which is not a contract — and reports the same reading and both remedies. The
   message is a pure function of `(honoured, flow, path, err)`, and `honoured: None` never
   blames flow control: collapsing "unmeasured" into "refused" is the named mistake
   (notes §3.72). The gap is a missing *refusal*, never a missing *explanation*.
6. The cost is stated where the decision is: the pre-check's own open→configure→restore→close
   is an extra DTR toggle (last close of a tty with `HUPCL` lowers DTR and RTS — measured on
   the rig by far-port CTS edge counting, which also falsified the shipped claim that the open
   was "not an *extra* toggle", with the confound that `CRTSCTS` itself moved the line refuted;
   notes §3.68). A board that auto-resets on DTR takes one extra reset per `load` of an
   `rts-cts` node; removing it would mean asking from inside the node's own open, past where
   "nothing is created" holds — a filed design question, not a patch.
7. Per-mode measurement status: `rts-cts` is measured on both kernels — Linux honours the flag
   (`cflag` delta exactly `CRTSCTS`; the `7cf0338` Linux triple — a committed doctor artifact,
   the citation convention is §13's — 2026-08-05, and the Linux 6.18
   field report at `3e23c52` agree) and Darwin's `IOSerialFamily` accepts-then-drops it on the
   FT232R rig (`silently_dropped: true` on both ports; the `acb5162` macOS triple, 2026-08-05)
   — so the refusal arm and the honour arm have each executed on real hardware. A driver that
   *refuses* the flag outright is honest and is not refused here; only accept-then-drop is the
   defect (§15.53). `xon-xoff` has no pre-check and no probe, and `serial2` verifies `c_iflag`
   by read-back too, so a driver silently dropping `IXON`/`IXOFF` would fault the same late way:
   that mode is **unmeasured rather than known-good**, named here and carried as open work
   (plan §18 item 14).

### 7.2 PTY node

One endpoint, faces target. Configuration: symlink `path` (required); `owner`/`mode` (default:
daemon user, 0600; 0660 when a group is configured — applied to the slave device node, which is
what gates `open(2)`; accepted range `0o600 ..= 0o777`, one window carrying two rules, since a
permission mode is nine bits and TOML rejects leading-zero integers: the ceiling names the whole
family of three-digit *decimal* typos of an octal mode, of which `mode = 666` — 0o1232, owner `-w-`
— is the one that used to load and then fault the node with an EACCES that never mentioned the
mode, and the floor is the daemon's own access, because the node's setup chmods the slave and then
opens it to prime the session); write mode (default `on-demand`; `never` makes a spy terminal);
hostward drop policy, including `hostward_buffer` as on §7.1's serial node; and an optional
`advertised_baud` (default 115200) — cosmetic, since PTYs ignore baud, but it makes `tcgetattr`
tell attached clients something sensible, and it may mirror the paired serial node's configured
rate (standard rates only — a nonstandard advertised value is skipped rather than approximated).
State: allocated pts path, client-present flag, the client's current termios parameters (baud,
character size, parity, flags), and drop counters.

**The baseline contract.**

1. At creation the daemon allocates the pair, sets the baseline termios — raw, echo off, EXTPROC
   on — and creates the configured symlink to the pts node. Raw-and-no-echo is load-bearing: a
   fresh PTY echoes slave input back to the master and translates newlines, which builds
   feedback loops the moment data reflects.
2. `prime_slave` — opening and closing the slave once at creation — is mandatory, not
   droppable: a never-opened master does not report HUP on either measured kernel
   (`hup_when_never_opened: false` on Linux 6.18 and 7.0 alike, doctor P2), so priming forces
   the canonical absent state and makes presence detection uniform from the node's first
   instant.
3. Client-termios observation rides packet mode (TIOCPKT) plus EXTPROC on the master: any client
   `tcsetattr` surfaces as a control packet, after which one `tcgetattr` updates the reported
   client parameters in state — TIOCPKT alone reports flushes and flow-control toggles but not
   baud changes, so EXTPROC is the necessary companion. Clients that rebuild termios from
   scratch clear EXTPROC; that clearing generates a final notification and the daemon re-asserts
   the flag through the master.
4. A slow reconciliation poll (a few seconds; one ioctl, effectively free) backstops the
   mechanism's obscure corners, and — by measurement, not by hedge — is what *carries*
   client-termios observation on macOS: Darwin does emit a packet on a client's `tcsetattr`, but
   its leading byte is `0x20` (`TIOCPKT_DOSTOP`), never the `0x40` (`TIOCPKT_IOCTL`) the fast
   path matches, measured on the rig and recorded in `docs/macos.md`; doctor P1 reports the
   fast-path signals absent and degrades with the poll named as the carrying mechanism.
5. The poll stays live unconditionally on every platform, and the rule is general, not an
   instance: **a probe passing is never a license to delete an unconditional degradation
   path.** The daemon never consumes doctor output (§13), so a wrong probe can mislead a
   developer but never the data plane.
6. The daemon only *observes* client termios; propagation to hardware is deliberately deferred
   (§14), with an experiment path already available: subscribe to state changes and drive the
   serial node's configuration via RPC from a userspace tool.

**Presence, sessions, and last close.**

1. The PTY outputs hostward data while at least one client holds the slave open, and discards
   (with counters) while none does — which conveniently also means the daemon never parks bytes
   in a kernel buffer nobody will read. Presence means "at least one opener"; the daemon cannot
   tell sharers apart (§6).
2. Disconnect detection is the master's HUP condition; attach detection needs two mechanisms
   because no un-HUP event exists: terminal programs announce themselves instantly by calling
   `tcsetattr` (a packet-mode event), and a sub-second zero-timeout HUP-status check catches
   silent openers like `cat`.
3. On last close, the daemon resets the pair's termios to the baseline **and discards whatever the
   pair still holds hostward** — the device output this session was sent and never read — so every
   client session starts deterministic in both senses that phrase must carry: how the next
   session's bytes are *framed*, and *which bytes it sees at all*. Only the first half shipped
   originally, and the sentence read as though it covered both: the kernel keeps a departed
   session's undelivered output across the close, up to the pts input queue's depth — on Linux
   7.0.0-29, **13824–15872 bytes** recovered across the committed P10 captures, varying run to run
   and direction by direction, with §15.46's cited triple
   (`docs/doctor/linux-7.0-2026-08-05b-tier3{,-2,-3}.json`) spanning 13824–15360 — and hands it to
   the next opener, so a fresh `picocom` opens onto the previous operator's scrollback. *(The
   figure this sentence carried until 2026-08-12 was "13.8–15 KiB", which is no capture's reading
   and not even one unit: 13.8 is 13824 bytes counted in KB, 15 is 15360 counted in KiB, and the
   artifact it cited reads a flat 15360 in both directions. A v15-era un-artifacted number
   re-cited rather than re-derived — the exact move §16.13 and AGENTS §7 exist to stop; notes
   §3.75.)*
   The discard is **counted** (`discarded_at_last_close`), because §5 permits a slow or absent
   consumer to cost itself data and never permits losing it invisibly.
4. Mechanically the daemon **reads the slave dry** rather than issuing `tcflush`: the queue
   belongs to the slave, and of the flush variants only a read reports how much it removed —
   which is the number §5 wants. A master `TCOFLUSH` also leaves the line discipline's own
   buffer behind, and a slave `TCIFLUSH` leaves a `TIOCPKT_FLUSHREAD` packet on the master that
   the next poll cannot tell from a client's session evidence.
5. What a kernel does with those bytes when nobody drains them is measured, not assumed: Linux
   *retains* across the close (close completes in tens of microseconds, full recovery in every
   shape; `docs/doctor/linux-7.0-2026-08-05-tier3.json`), and Darwin *waits-then-discards*
   (~600 ms close-wait and 0 of 64 recovered with no reader; full recovery when the master
   drains first; unconditional loss in ~29 µs for an `O_NONBLOCK` slave;
   `docs/doctor/macos-24.6.0-2026-08-05-tier3.json`). Doctor P13 measures the policy and never
   judges it — all three dispositions are legitimate and the daemon is correct under each. The
   harness consequence (a byte counter is read while the client that fed it is still open) is
   plan §3 rule 8, enforced by the `OpenWitness` idiom (notes §3.56).
6. The baseline is a *contract* — raw, echo off, EXTPROC at session start — and the fd it is
   applied through is a **platform arm** (§15.30). The discriminator is whether the master
   accepts termios: Linux does, and applies and resets through the master; BSD-family kernels
   reject termios on the master (ENOTTY), so macOS applies through a momentarily opened slave —
   and because macOS also resets a pty's termios to cooked on last close, the baseline is
   re-asserted on the presence *rising edge*, the arriving client then holding the slave and
   keeping the setting alive. A poll-latency window precedes that re-assert, consistent with the
   macOS poll-only observation story; interactive clients, which set their own raw termios,
   never notice.
7. The last-close flush of clause 4 opens the slave on **both** platforms — the master cannot
   name the whole queue on either — so it is the momentary-slave-open mechanism used a second
   time, not a second mechanism. What stays a platform arm is only the *readability* of that fd:
   on Linux the master-applied baseline survives the client's close, so the fd is already raw;
   on BSD the pair has reset to cooked by then, and a canonical-mode read returns only complete
   lines, so the flush sets the pair raw first or a half-written line survives it into the next
   session (§15.30).
8. Last-close detection latches on **session evidence** (§15.36): any successfully read master
   packet — data or ioctl alike — arms it, because a client that opens, sets termios, and closes
   inside one poll window is a real session whose write lock must release (§6 detach-release).
   The node's own baseline re-assert packet is then drained unconditionally (counted if it ever
   carries data, per §5) so it cannot re-arm the latch it triggered.
9. On Darwin that evidence can be destroyed before the daemon sees it: `ptsclose` flushes both
   tty queues at the slave's last close, so a session collapsed inside one poll gap leaves the
   master byte-identical to no session at all — no *level* observable can recover it, because a
   session boundary is an edge and level state cannot carry an edge (§15.39). The
   `serial_nexus_sys::SessionLatch` — on Darwin a `kqueue` knote registered
   `EVFILT_READ | EV_CLEAR` on the master, elsewhere inert — folds its answer into the same
   `saw_session` latch rather than adding a third disjunct: it is the same fact arriving by
   another road, and one predicate keeps the close block reviewable. This does not weaken the
   AsyncFd ban (§5, §15.18): readiness remains `poll(2)` and only `poll(2)`; the latch answers
   "did a session boundary happen", a question `poll(2)` cannot answer on that kernel, and it is
   polled once per pass, non-blocking, and **never marks the pass productive** — an edge is not
   data, so the idle backoff is untouched.
10. Two forge sites, both measured, both load-bearing (§15.39): the daemon opens the slave
    itself — the baseline re-assert, the last-close flush, the reconciliation backstop — and
    each posts an edge the kernel cannot distinguish from a client's. So the latch's registration
    swallows the edge its own registration posts (registering on an already-hung-up master posts
    one immediately, and every pty node starts there because setup primes the slave), and the
    last-close block discards after running. Removing the first invents a session on the node's
    first pass; removing the second makes the handler re-fire on its own footsteps and release a
    lock no client ever took.
11. **Deliberate:** one residue stays open on Linux — a bare open-and-close that touches nothing
    leaves no evidence to latch on; it also sent nothing to purge, and it self-heals on the next
    observed session. On Darwin that same shape *does* post an edge, so macOS is the stricter
    platform for it. The asymmetry is recorded rather than levelled: levelling upward means
    inventing a Linux mechanism for a shape that costs nothing, downward means discarding an
    edge the kernel is already giving away (§15.39). Doctor P12 sits beside P7 so the two
    detach-release mechanisms are diffable across kernels.
12. The last-close block also drains the master so the poll loop cannot spin on a hung-up pty —
    a hazard that cannot occur on the kernel of record (P6 measures `pollin_passes: 0` with
    `read_outcomes {EIO: 64}` on a hung-up master there, against Darwin's 64 of 64
    `POLLIN|POLLHUP` passes), which makes its regression guard the tree's sharpest known
    proxy-in-space: it self-skips off Linux, the one platform family where the hazard exists, so
    a widened last-close predicate or a deleted latch drain would burn a core — and release
    operator-held write locks — on macOS with the suite green. That gap is named open work, not
    accepted silently (plan §18 item 12); P6 agreeing across Linux kernels is measurement of the
    friendly platform, never permission to widen the predicate or delete the drain.

**Symlink rules.** The configured path is configuration; the pts target is state. A pre-existing
path faults the node (environmental) — except a symlink dangling into devpts, presumed to be the
daemon's own stale artifact from a crash, which is silently replaced. The symlink is unlinked on node removal
and clean shutdown.

### 7.3 Log node

One endpoint, faces target: the log consumes a hostward stream and appends it to a file. Its
write mode is inherently `never`. A configured edge value is not rejected — it is promoted to
`never` at runtime through `GraphConfig::effective_write_mode`, the one implementation of every
write-mode promotion, consulted by both the validator and the wiring so the two cannot disagree
about what a graph does; `dump` therefore round-trips exactly what the operator wrote (§11), and
the override is cosmetic on disk while absolute at runtime (notes §3.17, §7.8).

Configuration: directory, filename, overflow policy (drop-oldest with counters, or fault the
node), and rotation-suffix padding (default three digits). State: current file, most recent
rotation number, `queued_bytes`, `dropped_bytes`, and `write_errors`/`last_write_error`.

Contract:

1. **Regular-file shape.** Raw bytes are appended through the bounded-queue-plus-writer-task
   shape that §5's files exception requires (`O_NONBLOCK` is a no-op on regular files); overflow
   follows the configured policy, always with counters.
2. **A filesystem error faults this node alone.** A filesystem error (ENOSPC included) under the
   fault policy faults **this node alone**, with the errno in `last_write_error` — the port's
   other consumers keep flowing (§5's isolation).
3. **Rotation is on demand only.** `serial-nexus-ctl rotate <node>` renames the current file to
   `<name>.NNN` with an incrementing counter — higher is newer, no logrotate-style shifting
   cascade — and reopens fresh at a byte boundary.
4. **The rotation counter is state.** It is recovered at node start by scanning the directory
   and never persisted.
5. **A directory scan that cannot run faults the node at create.** Bought by a defect: a
   mode-0300 directory makes create-and-traverse succeed while listing fails, and conflating
   scan-failure with no-rotations once let `rotate` clobber the newest rotation with a silent
   `rename(2)`. The open is still attempted first, so a missing directory names the open; while
   the fault stands, `rotate` refuses; the fault reason spells `scan <directory> for
   rotations: <err>` (notes §3.27).
6. **A failed rotation stops the writer for good**, under either overflow policy — everything
   after it counts in `dropped_bytes`. Also bought by a defect: reporting unwritable data as
   queue depth once hid megabytes of console output behind a flat counter. `queued_bytes`
   falling to zero and staying there is the signal, and `write_errors`/`last_write_error`
   separate a filesystem refusing every write from a slow consumer.
7. **Removal and clean shutdown flush the queue** within a bounded wait before closing (§11).

### 7.4 Leg node

The cross-daemon transport: a socket carrying all of its channels multiplexed by the built-in
link codec (§8, §9). Endpoints: one per configured channel, each with a channel identity; all
channel endpoints face target on the sending side (`faces = target`, computer A: the leg
consumes local channels) or host on the receiving side (`faces = host`, computer B: the leg
offers arriving channels). A leg node has no default endpoint (§3).

Configuration: `faces`, transport (`tcp | unix`), `role = listen | connect`, address
(loopback-only by default; any non-loopback bind or dial requires the deliberately ugly
`insecure_bind = true`), reconnect backoff for the connect role, idle-release interval for
implicit lock acquisition (§6), purge-on-reconnect override, and the channel list with
identities. State: peer address, connection status, and per-channel binding — `bound`,
`waiting` (configured here, not announced by the peer), and the peer's announced-but-
unconfigured identities listed as `unbound`.

Contract:

1. **One active peer per leg.** Concurrent second connections are refused.
2. **The connect role retries with backoff.** An outage is faulted-and-wait, never a graph
   change.
3. **The listen role's `bind` also retries**, on the same backoff, re-running the stale-socket
   check per attempt; a successful late bind sets `waiting` so the heal is observable. Bought
   by a defect: the one-shot bind an earlier revision described made a refused bind the
   daemon's only environmental fault clearable solely by remove-and-re-add, which §15.8
   forbids (notes §3.28).
4. **Outage handling rides existing machinery.** Targetward writers pause on §5's backpressure;
   purge-on-reconnect (default on) discards outage-era backlogs with counters — §6's third
   purge instance. The receiving side decides staleness per chunk, by the connection epoch
   stamped at enqueue, never per drain: chunk-level provenance is §6's sharpening, stated
   there once.
5. **Wire announcements never grow the graph.** They bind to configured channel identities, and
   everything else is visible state awaiting an operator (§8).
6. **The leg's two directions are independent.** Exhaustion of one half parks that half only —
   it never tears down the wire or the other direction (§15.24).
7. **SSH is the confidentiality and authentication layer** for now — including OpenSSH
   streamlocal forwarding of Unix-socket legs, which skips TCP entirely (§9).

### 7.5 Codec node

A protocol transform: one multiplexed-side endpoint and N channel endpoints, instantiable in
either orientation via `faces` on the multiplexed side (demultiplexer: multiplexed side faces
target, channels face host; the mirror for re-multiplexing). Configuration: codec name
(selecting from the compiled-in registry, §8), an opaque attribute table the codec deserializes
and validates itself — schema failure is structural and fails the load, caught by the
pre-create check before anything is torn down (§11) — and the channel list with identities.
State: per-channel status and byte counters, plus the framing accounting below.

Contract:

1. **A codec's resync policy is its own** (§15.23): the reference codec resynchronizes by
   length-guidance — skip exactly the framed length, count one framing error — while the link
   codec, running on a reliable transport, treats any framing violation as a protocol error
   and never resyncs.
2. **State carries two framing counters, and they are not duplicates.** `framing_errors` is the
   transform's own accounting, read from the trait's `resync_count()` — 0 by trait default for
   exactly the codecs that never resync — and `demux_errors` beside it is the daemon's own
   count of demux refusals. A refusal faults the node: `active` is a claim about delivery, and
   a transform refusing the stream is not delivering. The salvage accounting, and the trait
   limitation it works around, are stated in §8.
3. **A standalone re-multiplexing instance** (`faces = host`) is accepted-and-waiting in §14's
   deferral vocabulary: validation accepts it, and it waits for a driver; until one exists it
   is deferred work (§14).

### 7.6 Exec codec node

The escape hatch, packaged as an ordinary compiled-in codec (§8): configuration adds argv,
environment, and restart backoff. This section is written to be read by codec authors — the
child's side of every clause here is what §8's validation chapter exercises.

Contract:

1. **The child speaks the fixed envelope protocol on stdin/stdout** — the same frame format as
   the link codec's wire protocol, versioned separately from it (§8) — translating between the
   envelope and the device's proprietary framing.
2. **stderr is a third pumped stream** into daemon diagnostics.
3. **A crashed child faults the node and restarts with backoff**; teardown-versus-crash is a
   discriminated outcome, never a heuristic (§15.22).
4. **The child's pipes are boundaries, not interior plumbing** (§15.22) — kernel pipes with
   finite buffers and an independent consumer are §3's boundary test, verbatim. stdin and
   stdout are pumped as concurrently-polled directions, so a blocked write can never starve
   the read. Tripwire, bought by a defect: one `select!` coupling stdin-write with stdout-read
   deadlocked cleanly under sustained flow (the phase 5 audit's critical finding — plan §4); the
   exec-conformance battery tests the class from the author's side (§8).
5. **The multiplexed raw stream travels on the reserved *empty* channel identity**, which
   configuration validation independently forbids for configured channels, so the reservation
   cannot collide (§15.22).
6. **Deliberate:** the child is not sandboxed — it runs as the daemon's user, and the
   documentation says so plainly; `docs/security.md` carries the capability statement.
7. **Copyleft tools run unmodified.** Because the child is a separate process speaking a
   documented protocol, protocol tools under any license, including copyleft, can be used
   without linking (§13).
8. **Its teardown-ledger figure is a floor**: the node's internal merge stage is not reached by
   the §5 ledger, and §5 names that limit at the counter (notes §3.31); the remainder is held
   in the work ledger (plan §18).

### 7.7 Existing-terminal node

Connects to a pre-existing PTY or tty device by path (no hardware identity; the resolver passes
paths through, §12). `faces` is configuration: `host` when the far side acts as the target — a
QEMU serial console, a protocol simulator, a mock device for testing — or `target` when the
daemon feeds a stream into some other program's terminal. Otherwise it behaves as a boundary
with the standard policies.

**Refused-at-load** (§14's deferral vocabulary): the node remains in the model but is not
implemented, and a configuration naming it is refused, listing the node kinds that do exist.
Not the *same* treatment §7.1 gives the serial output leg, which this sentence claimed until
2026-08-12: that one is a structural error from validation naming the deferral, and this one is
serde's unknown-variant error at `INVALID_PARAMS`, because `existing-terminal` is not a schema
variant at all. Both refuse at load with nothing created; only the first says why (§14 entry 15;
plan §18 item 45). Stated here, not only in §14,
because this section once read in the present tense while absent from §14 (review 32 DEVR-3).
§3's "existing-PTY connectors" and §12's "Existing-terminal nodes … pass through as path
identities" describe this same deferred node type.

### 7.8 Map node

The character-mapping transform — picocom's `--imap`/`--omap`, made a place in the graph
instead of a flag on every client (§15.33). One target-facing endpoint, one host-facing
endpoint; the host-facing side is the node's default endpoint and carries the full standard
machinery — write lock, fan-out, taps, and the default replay ring — which means both a raw
view (the upstream endpoint's ring) and a mapped view (the map's ring) exist by default.

Contract:

1. **Configuration is two ordered lists of named mappings**, `hostward` and `targetward`. The
   direction names the bytes being transformed: `hostward` corresponds to picocom's `--imap`
   (device toward consumers) and `targetward` to `--omap` (consumers toward device). The
   flow-relative input/output vocabulary is rejected here for the same reason §15.3 rejected
   it everywhere else (§3).
2. **The mapping vocabulary is picocom's**: `crlf`, `crcrlf`, `igncr`, `lfcr`, `lfcrlf`,
   `ignlf`, `bsdel`, `delbs`, and the hex-display family (`spchex`, `tabhex`, `crhex`, `lfhex`,
   `8bithex`, `nrmhex`) — where `spchex`, per the upstream oracle (picocom's own implementation,
   used as the reference answer), hexes the *control class* (DEL and controls other than tab,
   LF, CR), never the space character (§15.34).
3. **An unknown name is a structural error; an empty list is the identity**; within a
   direction, the first matching rule per input byte wins, so order resolves conflicts
   deterministically.
4. **Every rule is a stateless byte-to-byte-sequence substitution** (possibly empty —
   deletion), so chunk boundaries are irrelevant by construction and no parser state exists.
   **Deliberate:** this is not a codec — no channels, no frames, no resync — just §5's
   interior contract at its simplest (§15.33).
5. **Output is bounded at k× input**, where k is the largest expansion among the active rules
   (2 for the CRLF pair, 4 for the hex family), so the holdover slot's memory bound survives
   expansion (§5).
6. **State reports per-direction byte counters and per-rule substitution counts** — the cheap
   way to discover which quirk a mystery console actually has.
7. **The map's targetward edge into the upstream endpoint defaults to `held`** — the demux's
   pattern with softer stakes: bypassing a map is not corruption, merely unmapped, so
   steal-to-bypass is a legitimate, visible act, and the addressing states intent — `send` at
   the map's endpoint speaks mapped, `send` at the upstream endpoint (post-steal) speaks raw
   (§6).
8. **The `held` default is a runtime promotion, not a serde default.** It lives in
   `GraphConfig::effective_write_mode`, so `dump` stays faithful to what the operator wrote
   (§11's round-trip), and the one implementation is consulted by both the validator and the
   wiring, so a re-derived validator cannot miss the two-maps-both-promoted starvation shape
   (notes §3.17; the "dump round-trips wrongly" reading was refuted there).

## 8. Codecs and extensibility

A **codec** is a multi-channel framing transform: it converts between one multiplexed byte stream
and N channels, in both directions, emitting and consuming per-channel events drawn from a small
vocabulary — `data`, `open`, `close`, `error`. One implementation serves both orientations; the
node's `faces` attribute selects which. Edges always carry raw bytes; all framing knowledge is
internal to codec nodes, which keeps the interior contract (§5) intact — a codec may hold a
partial frame, bounded by its frame size, and nothing else.

This section binds in three layers. *The codec model* states the architecture and the numbered
contract every codec must honour. *Validating your codec* is the practitioner's path: the shipped
kits, what passing each one certifies, and the fixtures that must survive every rewrite.
*The extension surface* states exactly what an out-of-tree author may depend on and what was
declined. The standalone author guide, `docs/codec-authors.md`, carries the tutorial half — the
frame-layout table, the worked examples, the minimal exec codec and the TOML that wires it (its
§1–§7) — and this section cites it rather than duplicating it: the guide teaches, the design
binds.

### The codec model

**Static and announced channels.** Hardware mux codecs declare their channel set from
configuration, so their endpoints exist at load time. The link codec inside leg nodes instead
*announces* channels over the wire — and an announcement must not grow the graph. The binding rule
reconciles this with the operators-own-the-graph invariant (§4): announcements bind to channel
identities the receiving configuration already declares; announced-but-unconfigured channels
appear in state as `unbound` with no endpoints and no attachments; configured-but-unannounced
endpoints sit in `waiting` — the same state family as an unplugged serial port, reusing
faulted-and-wait (§7) wholesale.

**Registry and workspace.** The project is a multi-crate Cargo workspace. The
`serial_nexus_codec_api` crate defines the codec trait, the event vocabulary, and the envelope
frame types (codec attributes arrive as an opaque TOML table at the `serial_nexus_daemon`
registry boundary); codec crates depend on it and never on the daemon.
Compiled-in codecs live in an explicit registry-as-value — a name-to-factory table,
`Registry::with_builtins().register(name, factory)` — populated with the built-ins (each behind a
Cargo feature, so minimal builds drop what they don't need) and extensible by an embedding binary
(§15.26). There is no linker-magic auto-registration and no dynamic loading; a registration that
collides with an existing or reserved name (`exec`) is a startup error, before any configuration
is read.

**Embed, don't load (§15.26).** The daemon is a library with a thin binary: `serial_nexus_daemon`
exposes a deliberately narrow entry surface — run options mirroring the CLI flags, the codec
registry, and the version constants — with everything else private. An out-of-tree repository
ships two small crates: a codec crate depending only on `serial_nexus_codec_api`, and a custom
daemon binary whose `main` is a dozen lines wiring `Registry::with_builtins().register(...)` into
`serial_nexus_daemon::run`. Everything else in the ecosystem works against that binary unchanged —
`serial-nexus-ctl`, `serial-nexus-sim`, `serial-nexus-doctor`, and the harness speak RPC and the
envelope, never the codec list — which is §15.16 paying out a second time. The daemon reports its
registered codec names and the wire/envelope versions through the `info` verb; `info.codecs`
answers "which names may a config use" by unioning reserved names over registered ones (`exec`
appears without being a registry entry, via `Registry::usable_codec_names`), and an unknown codec
name in configuration fails structurally with the available list in `data.available` — tools
discover capabilities rather than assume them. The exec path (§7.6) remains the zero-Rust
alternative for other languages and copyleft tooling, with the same conformance treatment. The
full surface enumeration and its standing declines are stated under *The extension surface* below.

**The envelope.** The exec codec's child-process interface and the daemon-to-daemon wire framing
are two *contracts* with distinct stability promises: the envelope is public and versioned for
external codec authors (`ENVELOPE_VERSION = 1`); the wire protocol is internal between daemons and
free to evolve under the §9 contract (`WIRE_VERSION = 1`). In v1 they deliberately share one frame
format and one implementation, defined in `serial_nexus_codec_api` — one specification to
document, pipe-testable codecs, and any-language authorship (including wrapping copyleft protocol
tools) behind a stable, non-linking interface — but they version independently, and **evolving the
wire must never break envelope users** (§15.15). The frozen golden vectors stay byte-identical
across wire evolution by construction. The shared v1 frame is big-endian
`u32 body_len | u8 type (0–3) | u16 channel_id_len | UTF-8 channel identity | payload`, bounded by
`MAX_FRAME_SIZE = 65536`; `open`/`close` payloads are empty and the `error` payload is the UTF-8
reason. The layout table, the five named decode-error cases, and the worked examples live in
`docs/codec-authors.md` §3; the hello that opens a daemon-to-daemon connection is a distinct wire
construct, never a fifth event kind (§9).

**The `unstable_fuzz_api` exception.** "Everything else stays private" has one bounded, *named*
exception (§15.26 amendment, notes §3.19): a crate whose parser accepts bytes from an untrusted
peer may expose that parser through a module named `unstable_fuzz_api`, whose documentation
disclaims stability in its first sentence and whose only sanctioned consumer is `fuzz/`. Two
parsers qualify today: the daemon's control-socket line framer, which frames every byte before any
verb is dispatched, and the web console's HTTP head parser, which §15.29 permits to face a network
and which runs before the token gate. The rule that keeps the exception from eroding is
mechanical: **an item re-exported there must have a fuzz target driving it** — any non-parser item
appearing in such a module is the erosion the rule exists to catch.

**The codec-author contract.** The following clauses bind every codec, in-process or exec; each is
enforced or measured where the clause says, and a dropped clause is a visible diff:

1. **Four events, exactly.** The event vocabulary is `data`, `open`, `close`, `error` — nothing
   else in v1. The vocabulary is evolvable to richer per-channel control events later under §9's
   version discipline; a codec must not invent event kinds ahead of it.
2. **At most one partial frame.** A codec may retain one partial frame, bounded by its frame size,
   and nothing else — no queues, no policy, per the interior contract (§5).
3. **Channel identities are names, not indices.** Identities are UTF-8 and never contain `/`;
   configuration additionally refuses whitespace-only identities and identities over 256 bytes.
   The identity rides in every frame header (§3's name-legality rationale), so an oversize
   identity leaves no payload room by construction.
4. **Fragment, never drop.** A chunk exceeding the frame bound is fragmented across consecutive
   `data` frames — never rejected, never dropped — and every targetward framer fragments via the
   one shared helper, whose per-fragment payload cap is `MAX_FRAME_SIZE − (3 + channel.len())`,
   floored at 1. Never skip-on-encode-error. **Misreading callout:** this rule exists because the same
   skip-on-error bug shipped three times — fixed in the leg, fixed in the exec node, missed in the
   in-process codec — before §15.27 moved it into the shared helper. The obligation's normative
   home is §5; it is a tripwire (AGENTS §4).
5. **Three-outcome decode.** A demultiplexer decodes to exactly one of: a whole frame; need-more —
   any strict prefix yields `Ok(None)`, never an error; or malformed-or-oversize — a clean `Err`,
   with an oversize length prefix rejected *before* the body is buffered. The daemon treats a
   malformed frame on an exec child's stdout as a crash: fault and restart (§7.6).
6. **Demux `Err` is non-latching.** A `demux` error is counted (`demux_errors`), surfaced
   (`last_demux_error`), and faults the node — and is cleared automatically the moment a later
   chunk decodes. An `Err`-then-`Ok` codec is legal; recovery is the normal case for a resyncing
   codec on a lossy line.
7. **Emit-before-error is supported, and the accounting errs against the codec.** A codec may
   emit salvaged frames and then return `Err` for the remainder; the loss charge is
   `chunk_len.saturating_sub(salvaged data payload)`, so the framing overhead of salvaged frames
   stays charged. The trait cannot report consumption — **the daemon knows what a codec
   emitted, never what it consumed** — so the residual deliberately errs toward over-reporting
   loss. A call that emits more payload than it was handed charges zero, never wraps. The first
   repair of this accounting (review 32's WIRE-1) charged the whole chunk and double-counted
   salvage — a refuted repair, re-fixed by the audit; the guard
   `codec::tests::a_partial_decode_charges_only_the_bytes_the_refusal_lost` pins the corrected
   charge.
8. **The silent-merge trade.** A length prefix corrupted to a value still at or under
   `MAX_FRAME_SIZE` silently *merges* the swallowed frames into one `data` payload on a legitimate
   channel — mirrored to the ring, counted `delivered_hostward`, with no framing error
   attributable to the merge. **Deliberate:** §9 specifies no per-frame integrity field, so the
   merge is undetectable by construction; adding one is an envelope version bump, not a patch
   (review 32 WIRE-3; pinned by `merged_frames_when_the_length_prefix_is_mangled_under_max` so it
   stays a fact, not a surprise). The consequence for anyone reading counters:
   **`framing_errors == 0` is not stream health.**
9. **Resync policy is the codec's own** (§7.5). The reference codec resyncs by length guidance —
   skip exactly `4 + body_len`, count one framing error, stay aligned; the link codec on a
   reliable transport never resyncs. `resync_count()` is surfaced as node state, defaulting to 0.
10. **Unconfigured is counted, not fatal.** Announcements or data on a channel identity the
    configuration does not declare never grow the graph; they are counted
    (`discarded_unconfigured_channel`) and named in the bounded, deduplicated
    `unconfigured_channels` list (with `unconfigured_overflow` when the bound is hit). Data on a
    configured channel with no consumer is `discarded_unattached` — a located §5 loss. Neither is
    an error.
11. **The mux edge's `write_mode` must be `held` or `never`.** Anything else — including the
    `on-demand` a config gets by omission — is refused at load, naming the edge. **Misreading
    callout:** this is the first configuration mistake nearly every exec-codec author makes,
    because the refused value is the default. The refusal exists because an on-demand origin parks
    on its first chunk forever while `send` answers `{"delivered": true}` — bytes accepted,
    acknowledged, and lost, the exact shape §5 forbids. One exemption: a `free-for-all` origin
    (§6), where the rule does not fire rather than refusing a graph that runs.
12. **Attribute schemas fail structurally.** Codec attributes arrive as an opaque TOML table the
    codec deserializes and validates itself, via serde into its own types; a schema failure is
    structural and aborts with nothing created, consistent with §11 — and
    `Daemon::precheck_codecs` validates every codec node's name *and* attribute schema before a
    `--replace` teardown, so a bad table on a known codec can never destroy a good graph on its
    way to failing (the pre-create precheck contract, §11; the pattern §15.53's flow-control check
    reuses). For the exec codec: `argv` is required non-empty, an unknown key is refused naming
    the key, and `restart_backoff_ms` (default 200) is capped at 3600000 with out-of-range
    structural (`docs/codec-authors.md` §5).
13. **The empty channel is reserved.** The multiplexed raw stream travels on the reserved *empty*
    channel identity `""` (§7.6); configuration validation independently forbids the empty string
    as a real channel identity, so the reservation can never collide.
14. **Teardown is accounted.** A codec or exec node destroyed by teardown reports the targetward
    bytes it destroyed as `discarded_at_teardown` under §5's teardown ledger. **Deliberate
    limit:** `exec`'s figure is a floor, because its internal merge stage is not reached — an
    open, recorded residual carried in the plan §18 ledger, named here where the counter is
    documented.

The exec codec's child-stdio boundary — stdin and stdout pumped as concurrently-polled futures so
a blocked write never starves the read, stderr as a third pumped diagnostic stream, and
teardown-vs-crash as a discriminated outcome — is §7.6's contract; the child runs as the daemon's
user, unsandboxed, documented plainly in `docs/security.md`. A standalone `faces = host`
re-multiplexer is accepted by validation and loads accepted-and-waiting for a driver — a §14
register entry, not a supported configuration today.

### Validating your codec

This subsection is written to you, the codec author. The path is: write the codec, run the kit,
run the exec battery if you ship an exec codec, wire it into a sim graph, and watch the counters.
Each step below names what passing it certifies — and, just as deliberately, what it cannot see.

**Surface and trust model.** First know which contract is yours. An in-process Rust codec
implements the `Codec` trait from `serial_nexus_codec_api` and is compiled into a daemon binary —
your code runs in the daemon's process, with the daemon's privileges. An exec codec is a child
process speaking the envelope over stdin/stdout — any language, any license, including copyleft,
with no linking — and the child runs unsandboxed as the daemon's user (§7.6;
`docs/security.md` states the posture; supervising what you spawn is your job). Know which
version constant is yours, too: the envelope you program against is `ENVELOPE_VERSION`; the
daemons' `WIRE_VERSION` may move without you, and wire evolution never breaks envelope users —
`docs/codec-authors.md` §7 keeps the disambiguation in full — know which one is yours.

**What each check certifies.** The numbered contract above is the thing under test. The golden
vectors certify clause-level frame encoding; the kit's suites certify clauses 1, 2 (via the
opt-in accessor), 4–5's accept/need-more discipline, 6's codec-side half (the opt-in recovery
suite), 9's termination half, and 12's schema discipline — all from the consumer's position; the
exec battery's opt-in error paths certify clause 5's third outcome for a child; clause 7 is
daemon-side accounting pinned by the daemon's own guards; the sim corruption run certifies clause
9's accounting against a computed manifest; and the daemon enforces clauses 3, 11, 12, and 13
structurally at load, before anything is created.

**The golden vectors.** Four envelope frames are frozen as exact byte strings in
`serial_nexus_codec_api`'s `golden_vectors` test, reproduced with a worked layout in
`docs/codec-authors.md` §3. A drift is a breaking envelope change; the vectors are frozen
constants in the test itself, and regenerating them is a deliberate edit requiring a written
rationale in the commit (the test's doc comment states this). Your encoder either
reproduces these bytes or it does not speak the envelope — start here, because every later check
assumes it.

**The in-process conformance kit.** `serial_nexus_codec_api`'s `test-support` feature (a
dev-dependency, compiled only on opt-in) ships suites that any `Codec` implementation instantiates
in its own tests; each takes a factory (`Fn() -> C`) and fails by panic, so a broken codec fails
its own `#[test]`. Four are universal — `round_trip_identity`, `fragmentation_tolerance`,
`handles_garbage`, `bounded_parser_state` — and four are opt-in, each for a codec that has the
property to prove:

- `control_event_round_trip`, only for codecs that transport the control vocabulary (a passthrough
  legitimately must not run it);
- `assert_buffer_bounded`, for codecs exposing a buffered-byte accessor;
- `recovers_after_garbage`, clause 6's codec-side half, for codecs that **resync**: after one
  refused frame the next valid one must decode whole. A codec on a reliable transport exempts
  itself by not calling it, and the suite feeds an *envelope* frame with an unknown type byte, so
  a codec with its own framing replicates the pattern rather than calling this. It deliberately
  does not assert re-alignment after unaligned noise: where a correct length-guided resyncer
  re-aligns depends on the noise, and a suite demanding one answer would fail correct codecs — the
  trap `lag.py` was written to pin, in the kit;
- `attributes_are_structural`, clause 12's suite: every good table builds, every bad one returns
  `Err` **without panicking**, and a refusal names the key it refused. It is generic over the table
  type, so the kit still names no TOML crate, and it is what lets `precheck_codecs`' promise be
  proven from the consumer position rather than asserted about it.

The kit is deliberately dependency-free — a seeded LCG, no `rand` — so instantiating it adds no
crates to your tree.

**The kit-honesty rule.** A conformance suite that cannot observe a property must document that in
its own doc comment, ship the negative codec proving the gap, and offer the opt-in accessor-based
check that closes it. The shipped instance: `bounded_parser_state` cannot see a codec's internal
buffer through the trait and does not catch a hoarding decoder — the classic non-resyncing
accumulator, a memory leak on a lossy line — while `assert_buffer_bounded` does, and the `Hoarder`
negative codec proves both sides (`a_hoarding_codec_passes_the_trait_only_suites` beside
`a_hoarding_codec_fails_the_buffer_bound`). The limitation is documented, not hidden; if you
buffer, expose an accessor and opt in. Every kit suite likewise has a deliberately-broken codec
proving it bites — `DropsLastByte`, `DropsOpen`, `PanicsOnGarbage`, `Amplifier`,
`WholeFrameOnly`, `Hoarder` — fail-first discipline (plan §3) applied to the kit itself, and the
rule any future suite inherits. `LatchesOnError` and the lenient/unwinding/anonymous attribute
schemas are the same discipline for the four opt-in suites: the latching decoder drains correctly,
passes every other suite in the kit, and fails only `recovers_after_garbage` — Hoarder's shape,
one contract over.

**The exec battery.** `serial-nexus-sim` ships two exec modes, both reporting a JSON verdict —
never parsed text. `envelope` runs a 10-frame golden-vector battery through your child;
`exec-conformance` is the recommended CI entry point and adds sustained full-duplex liveness (the
§15.22 deadlock class as a test), fragmented-frame reassembly, and kill-and-restart cleanliness.
Two properties of the battery are contracts you can rely on: the stdin write carries a 5000 ms
*idle* deadline reset by every byte your child accepts, so a slow codec is never failed for being
slow, only for stopping — a child that never reads yields a failing verdict rather than a wedged CI
job; and `--exec` reaches your child through `sh -c` verbatim, so quote your paths — the in-tree
harness single-quotes and proves a spaced checkout runs
(`a_fixture_path_containing_a_space_still_runs`). Both properties hold in **both** modes: the
`envelope` feeder was an unbounded write behind a *total* deadline until 2026-08-12, which failed a
correct-but-slow child at the wall clock its sibling passed it at, and both now drive the child
through one boundary (notes §3.75).

Two flags widen what the battery can judge:

- **`--mux-to <channel>`** declares a demux shape instead of an identity passthrough: your child
  swaps the reserved empty channel identity with that channel in both directions, and the *whole*
  battery runs against the codec you actually ship rather than a passthrough build of it. The
  golden battery gains a frame in each direction of the declared mapping, and liveness,
  fragmentation and restart drive the multiplexed side. Omit it and every expectation is the
  identity it has always been, byte for byte.
- **`--error-paths`** adds clause 5's third outcome: an unknown type byte, an oversize length
  prefix, and a body truncated below its own declared channel length are each handed to a fresh
  child, which must **terminate**, must not echo the fault back as a valid frame, and must
  *signal* the refusal (a non-zero exit, or an `error` event). The verdict names the arm and the
  byte offset of the injected fault. Opt-in, because a permissive relay is a legal thing to write —
  and `passthrough.py` is exactly that, so it passes every universal check and fails all three
  arms, which is this battery's `Hoarder` and its fail-first proof. `strict.py` is the positive
  control and the shape to copy where a codec must report rather than relay.

A verdict never leaves a deadline unnamed: a check that expired reports so in `timed_out` and says
what it saw in `details`, because "not delivered within the deadline" and "dropped" are different
findings (plan §3).

**The corruption recipe.** Resynchronization is accounted, not approximate. If your codec speaks
the envelope framing, wire it under `serial-nexus-sim mux --corrupt-every N`: the sim emits a loss
manifest, and the validation is an equality, not a tolerance — the framing-error counter equals the
manifest's corruption count, and per-channel received bytes equal the computed expected-loss set.
For your own framing, replicate the pattern — a deterministic feeder that emits its own loss
manifest, with the same two equalities (`itest/tests/p5_resync.rs` is the worked instance).
Remember clause 8 while you read the counters: a corrupted length prefix that stays under the frame
bound merges silently, so `framing_errors == 0` is not health and only the manifest equality is.

**The consumer template.** `examples/external-codec/` — two codec crates plus a dozen-line
`acme-daemon`, workspace-excluded with its own `Cargo.lock` — is built from the consumer's position
on every push by `itest/tests/p8_external_codec.rs`, with `--locked` on both the build and the
conformance runs so a drifted lock fails loudly. `acme-codec` is a passthrough that demonstrates
the embedding pattern; `tinymux-codec` is a two-channel tag framer with parser state and one
attribute, and it exists because a passthrough exercises none of `control_event_round_trip`,
`assert_buffer_bounded`, or `attributes_are_structural` — the three suites now have a consumer.
The gate boots the custom daemon, asserts a `codec = "acme"` node loads with structured state
(never CLI text) and a `codec = "tinymux"` node loads with its attributes while a bad table is
refused naming the key with nothing created, and it pins the *whole* `info.codecs` list —
`["acme", "exec", "reference", "tinymux"]` — against the template README, because a containment
check cannot see the list drift. If your build breaks against a new tag, this template broke
first, on the push that would have broken you.

**Fixtures that must survive every rewrite.** Four Python fixtures under `tests/ext-codec/` and
one negative pair in the kit are load-bearing — named here, and in the plan's workspace map, so a
tidier future session cannot simplify them away:

- `lag.py` and `half-duplex.py` exist because of a refuted check design: the original liveness
  check was lock-step — send frame N, block for echo N — and **falsely failed a valid
  bounded-lag codec**. `lag.py` (echoes one frame behind, flushes at EOF) is the fixture the check
  must *accept*; `half-duplex.py` (read-all-then-write) is the antipattern it must *catch*.
  Without the pair, the battery would drift back into the bug it was fixed out of.
- `passthrough.py` is the conformance-battery target and `passthrough-codec.py` the demux skeleton
  to copy. `passthrough.py`'s `read_exact` returns `None` on a short read and silently drops the
  partial trailing frame — **Deliberate:** a writer can vanish between reads, and teardown is not
  corruption — and its doc says plainly that this is *not* the shape to copy where truncation must
  be reported. Copy the skeleton; copy the teardown shape too unless your codec must report
  truncation.
- The `Hoarder` pair and the other deliberately-broken kit codecs are the kit's own fail-first
  proof (above); deleting one deletes the evidence that a suite bites.

**Patterns inherited from the wider record.** Two harness rules apply to you unchanged. First, the
sim marks `timed_out`, so a deadline expiry is never read as a codec drop (plan §3): the
graph-level sim verdicts carry `timed_out`, and the exec battery reports a failed check with the
deadline named on stderr — when your CI run fails, read the verdict's fields before concluding your
codec lost bytes; "not delivered within the deadline" and "dropped" are different findings. Second,
prove the instrument before trusting the measurement (§13's instrument-validity doctrine): if you
build your own harness around the envelope, give it a positive control — a known-good passthrough
it must pass and a known-broken codec it must fail — before you let it judge your codec, because
the tree's own battery once failed correct codecs until `lag.py` pinned the check.

Further kit and battery capabilities stay filed as plan §18 ledger items, deliberately not promised
here as existing: **golden transcripts of the daemon boundary** (item 36) and a
**teardown-conservation suite on a codec node** (item 38). The five this list carried alongside
them shipped 2026-08-12 and are described above and in `docs/codec-authors.md`: the
attribute-schema suite (item 32), the `Err`-then-`Ok` recovery suite (item 33), the exec battery's
error paths (item 34), demux-shape exec conformance (item 35, retiring the identity-passthrough
limitation), executable doc examples (item 37), and the second template codec (item 39).

### The extension surface

The supported extension surface is exactly two contracts, both semver'd:

1. **`serial_nexus_codec_api`** — the codec trait, the event vocabulary, and the envelope frame
   types (codec attributes arrive as an opaque TOML table at the `serial_nexus_daemon` registry
   boundary) — for in-process codecs; and
2. **the `serial_nexus_daemon` entry API** — run options, the registry, and the version
   constants — for embedding.

Everything else is private, with the single bounded `unstable_fuzz_api` exception stated above.
An out-of-tree repository pins one version tag and inherits everything by linking: the CLI, the
sim, the doctor, and the harness all work against a custom daemon unchanged, because §15.16 made
the RPC surface the contract. The surface is guarded by proof rather than promise: the in-tree
external-consumer template is built from the consumer's position on every push, so a change that
would break an embedder breaks the tree first. The exec path is the same promise for authors who
never touch Rust — the envelope is public, versioned, and byte-frozen at its golden vectors
(§16.14's two-way rule pins constants tables of this kind against code).

Three declines stand, recorded so they are not re-proposed as fresh ideas (AGENTS §5):

- **No dynamic loading.** Runtime plugin loading (`dlopen`) is rejected on §15.11's grounds: Rust
  has no stable ABI, and a `dlopen` surface would turn every internal type into a compatibility
  promise (§15.26).
- **No parser-crate extraction.** Extracting each fuzzable parser into its own crate was weighed
  against the named-module exception and rejected: the privacy boundary exists to stop internal
  churn from breaking an *embedder*, not as an end in itself, and a module named
  `unstable_fuzz_api` with a first-sentence disclaimer cannot be depended on by accident
  (§15.26 amendment, notes §3.19).
- **The kit stays dependency-free.** The conformance kit deliberately carries no third-party
  dependencies — a seeded LCG stands in for `rand` — so instantiating it can never pull crates
  into a consumer's tree; a richer kit that costs an embedder a dependency graph is the wrong
  trade.

## 9. Wire protocol

The framing protocol is a module, not a design element. The design imposes a contract on any leg
framing and otherwise stays out of it: frame layouts, handshake encodings, and substrate choices
are properties of a particular protocol version and must not cascade into the rest of the system
(§15.15). *Wire* means the leg-to-leg framing between daemons (§7.4); the exec-codec envelope is
a separately versioned contract sharing the v1 implementation (below).

### The contract

Any framing must:

1. Multiplex any number of independent bidirectional byte channels over one reliable, ordered
   transport, addressing them by channel identity carried losslessly.
2. Transport the §8 event vocabulary per channel — `data`, `open`, `close`, `error` — and be
   evolvable to additional per-channel control events (the reserved lock request/grant relay of
   §6 depends on this).
3. Convey each peer's channel announcements, so the receiving daemon computes
   `bound`/`waiting`/`unbound` state without operator involvement.
4. Declare a bounded maximum frame size, so the interior one-frame holdover (§5) and
   receive-side reassembly remain bounded-memory (v1: a fixed constant; negotiable later). A
   producer chunk exceeding the bound is fragmented across consecutive `data` frames — never
   rejected, never dropped (§15.24).
5. Preserve the targetward no-drop guarantee end to end — at minimum through whole-connection
   backpressure, optionally through per-channel flow control. The protocol itself never drops;
   hostward loss remains a counted boundary policy of the sending daemon's leg.
6. Identify its version and negotiate optional capabilities at connection start, refusing
   mismatches cleanly with the reason surfaced in leg state.

**Misreading, recorded — clause 4's silent-drop variant shipped three times.** An oversize chunk
— the uncapped `send` line, or an exec node's raw device stream — hit the frame bound's encode
error and was silently dropped, uncounted, reachable precisely because the read buffer equals the
frame bound (§15.24, §15.27). Fragmentation is therefore contract text, implemented as one shared
helper applied per writer, never per protocol; the tripwire is in §5's table, and the standing
regression guard is a 100 001-byte `send` round-trip.

Conversely, the design keeps — and names as such — the aspects that exist to admit some form of
protocol: identity-keyed channels, the event vocabulary as lingua franca, Accepted/Busy as a
flow-control hook, binding states in leg state, and capability-conditional features.

### The v1 protocol

A custom framing satisfying the contract minimally: length-prefixed frames carrying a channel
identity and a type, opened by a `hello` frame (magic number, protocol version, channel
announcements, capability bitset). The hello is a distinct wire construct rather than a fifth
event kind — it reuses the length prefix and opens with the magic, validated together with the
version before any version-specific field, so a mismatch always refuses *as* a mismatch and the
envelope's golden vectors stay byte-frozen; the wire and envelope versions are independent,
cashing §15.15's two-contract split (§15.24). The handshake runs under one overall deadline, so a
trickling peer cannot wedge a listener; once up, the leg's two directions are independent (§7.4).

yamux was evaluated as a substrate — permissively licensed, with per-stream flow control — and
declined for v1 because identities, announcements, and the envelope sharing below are exactly
what it would not provide; under this contract, swapping the substrate later is a contained
change (§14). The record is §15.12 and §15.15.

Two v1 properties are documented as protocol properties, not design properties:

- Flow control is whole-connection only, so targetward traffic is subject to head-of-line
  blocking — one Busy targetward path stalls all channels' targetward flow across that leg.
  Acceptable because targetward traffic is human-scale command entry, and hostward flow is
  unaffected; per-channel flow control is admissible later without design change (§15.15). The
  harness pins the consequence, not the mechanism (§5).
- The v1 frame format doubles as the exec-codec envelope's v1 implementation, though the two are
  separately versioned contracts (§8) and wire evolution must never break envelope users.

### Security posture

Security posture, version one: legs bind and dial loopback only; SSH port forwarding (or
streamlocal forwarding for Unix-socket legs) provides confidentiality and authentication between
machines (§15.12). Non-loopback addresses require `insecure_bind = true` — a named footgun beats
the patched binary someone would otherwise ship. Serial consoles are frequently root shells and
bootloader prompts; the documentation says so in exactly those words. In-daemon TLS for legs is
deferred work (§14) — distinct from the web console's shipped `--tls`, which protects only the
browser hop (§17, §15.29); the two TLS stories must not be conflated.

## 10. Control plane

The daemon's entire operator surface is one Unix domain socket speaking JSON-RPC. This section
states the contracts and semantics of that surface; the JSON schema of every verb, reply, and
notification lives in `docs/rpc/`, which is the schema authority — this section deliberately
duplicates none of it (§16.14).

### The socket

The socket path is computed by one policy with three arms: `/run/serial-nexus-daemon.sock` when
running as root, `$XDG_RUNTIME_DIR/serial-nexus-daemon.sock` otherwise, and
`/tmp/<name>-<uid>.sock` when `$XDG_RUNTIME_DIR` is unset — an empty-but-exported value counts as
unset, since honouring it would yield a relative bind path. Any arm is overridden by a
command-line argument.

That policy has exactly one implementation, `serial_nexus_rpc::socket`, and every consumer goes
through it: the daemon binds through it, `serial-nexus-ctl` connects through it, the web console
resolves its default through it and names the result at startup, and the doctor
computes and prints it, with `SocketOrigin` naming which arm applied — so anything *printed*
about the path is computed from the same code that binds it, and a printed path cannot drift from
a bound one. **Misreading, recorded:** the doctor once named a socket fallback the daemon does
not use (notes §3.72) — the defect a second implementation of a policy invites. The policy core
is a pure function of `(is_root, xdg, uid, name)` behind a thin wrapper, so all
three arms are testable — including the root arm, which the test suite can never run under.

Socket permissions **are** the authorization model — whoever can open the socket owns every
console — with mode 0600 by default and flags to widen to a group; the standard stale-socket
unlink dance runs at startup, and the socket is removed on clean shutdown. SO_PEERCRED remains
available for finer authorization later without protocol changes.

### The protocol

JSON-RPC 2.0, hand-rolled over newline-delimited JSON — a page of serde types, no framework
crate. Request/response correlation supports concurrent CLI clients (mutations are serialized
daemon-side, in the sense sharpened under *Waiting verbs* below); id-less notifications are the
natural shape for subscriptions: a `subscribe` verb streams node status transitions, lock
changes, client-termios updates, and counter snapshots. Batch arrays are rejected outright,
deleting the specification's awkward corner. Everything is debuggable with socat and jq.

Error codes are a registry, not a convention: every code the daemon emits is registered in
`serial_nexus_rpc`, the documentation's error table is rendered from that registry, and a test
asserts the relation both ways — every emitted code registered, every registered code documented
— so editing either side alone is a test failure, never doc drift (§16.8, §16.14). This section
therefore enumerates no codes; the registry does.

### Waiting verbs

Some arbitration verbs cannot complete immediately — `lock --wait`, `send`'s
acquire-with-timeout — and the control plane runs them on two lanes (§15.20):

1. **Two lanes.** Every state transition remains a synchronous critical section on the runtime
   thread, exactly as before; a verb that must wait suspends *between* transitions holding
   nothing — no borrows, no locks, only its queue position — and re-attempts inside a fresh
   critical section when woken. "Mutations are serialized" therefore survives unchanged in
   meaning while concurrent connections keep flowing past a parked waiter. **Misreading,
   recorded:** serialization was never "dispatch is synchronous" but "transitions are critical
   sections" — a restatement, not a weakening, and a rewrite reverting to the old phrasing would
   re-break the two-lane model's justification (§15.20).
2. **Deadline expiry never consumes.** A waiting verb that fails — deadline, cancellation,
   teardown, removal of the endpoint — is dequeued with a defined error having done nothing:
   `send` at its deadline fails with the locked error having delivered no byte, and a cancelled
   waiter costs only its queue slot. This is the daemon-side half of plan §3's fill-then-commit
   client doctrine: a client commits on success precisely because expiry consumes nothing.
3. **Notifications are the delivery mechanism.** Lock transitions — acquire, release, steal,
   lease expiry — are emitted as immediate id-less notifications to subscribers; the periodic
   state snapshot is a floor for observability, never the delivery mechanism.
4. **Cancellation is deliberately indistinguishable from client death.** EOF on a connection's
   request half cancels that connection's waiting verbs, because a half-close and a killed client
   look identical at read time, and a keep-awaiting policy would strand the killed waiter. A
   raw-socket client must therefore hold its write half open across a wait (`serial-nexus-ctl`
   does; the `echo | socat` idiom does not, and the docs say so, §15.27).
5. **One waiting verb per connection.** A request pipelined behind a parked wait is refused with
   its own error while the connection — and the parked wait — survive intact; a client wanting
   concurrency opens a second connection, which costs nothing (§15.34).
6. **A connection's outbound writes never stop its inbound progress.** Every notification and
   response write is raced against the connection's parked-verb machinery rather than awaited
   inline in one select arm, because a blocked `write_all` there silently stops polling every
   other arm — the measured consequence was a parked verb whose deadline never fired and an
   endpoint frozen under a lock that `state` reported free (review 37, 37-CTRL-2).
7. **Verbs land on ports, not names, across reopen windows.** A signal verb arriving while a
   serial node is mid-reopen is accepted and stays valid into `active`, because the reopened port
   is published on the node *before* the purge (§7.1, §15.38); the full signal-scoping and
   ordered-release contract is §7.1's.

### Taps

`tap.open <endpoint> [--replay]` attaches a connection-scoped, read-only observer to a
host-facing endpoint; `tap.close` or the connection dropping detaches it.

1. **A tap is a §5 boundary consumer in miniature** — bounded queue, counted drops, a slow tab
   costs only itself. It streams the endpoint's hostward bytes as id-less `tap.data`
   notifications on that connection, base64-chunked.
2. **Replay splices exactly.** With `--replay` the live stream is preceded by the endpoint's
   ring (§5) with the exact-splice guarantee — no gap, no duplication — obtained by taking the
   snapshot and attaching the tap inside one critical section on the runtime thread (§15.20).
3. **Offsets, epoch, instance.** Every `tap.data` frame carries the endpoint's monotonic
   hostward byte *offset*; replay reports its `from_offset` and the `epoch` of the offset space
   that offset counts in (§15.38); and `info` exposes a per-boot daemon `instance` nonce.
   Together these let a reconnecting client (the browser history of §17) trim replay overlap
   exactly and detect counter resets, instead of appending ring-depth duplicates on every reload
   (§15.32).
4. **A tap never dies silently.** When graph mutation removes a tapped endpoint
   (`load --replace`, teardown, cascade removal), the daemon emits a `tap.closed` notification
   carrying the reason, and a `tap.close` for an already-dead tap is a plain error — clients
   re-anchor instead of guessing (§15.34).
5. **The obituary is mechanical, never best-effort.** Every connection carries an **unbounded
   terminal lane** beside the bounded data queue, attached synchronously at `tap.open`. In the
   ordinary case `tap.closed` rides the data queue and ordering is unchanged; when that queue is
   full — often the very condition the client needs to hear about — the event takes the terminal
   lane instead, which is drained ahead of the data arm. A full tap queue can therefore never
   swallow its own obituary, and because tap ids are never reused, a terminal event arriving
   ahead of queued data is never ambiguous (review 37, 37-CTRL-1).
6. **Taps never appear in configuration or dump**: they are state, scoped to a connection.

### The verb surface

The verb surface, grouped by semantics: configuration (`load`, `load --replace`, `dump`,
`add-node`, `remove-node [--cascade]`, `connect`, `disconnect`; `set-attribute` remains deferred,
§14); observation (`state`, `subscribe`, `info`, `ports`, and `tap.open`/`tap.close`);
arbitration (`lock [--steal] [--wait] [--lease]`, `unlock`, `send`); logging (`rotate`); serial
signals (`send-break`, `set-modem`, `pulse-dtr`); lifecycle (`teardown`, `shutdown`).
`serial-nexus-ctl` is a thin presentation layer over that surface — deliberately nothing more.
The reply contract for configuration verbs — a reply is a readiness promise for everything the
verb created — is §11's (§15.42).

`ports` is the resolver's passive enumeration of the serial devices it can see, each carrying the
identity that would bind it and the node that already does, if any. It never calls `open(2)`,
because probing a port toggles DTR on exactly the adapters people care about; the enumeration
mechanics, including the platform arms, are §12's (§15.35).

The web bridge forwards only an explicit allowlist of this
surface; the graph-editing verbs are on it, and the lifecycle verbs (`shutdown`, `teardown`,
`load`) stay off the browser wire — `shutdown` from a web page serves no one (§15.35). The
bridge's screening mechanics and its tripwire are §17's.

### The lifetime leash

The daemon is stopped by whatever started it — and a supervisor that dies *without unwinding*
(SIGKILL, `abort`, a runner killing the process group on a timeout) runs no `Drop`, no `atexit`,
no signal arm, leaving the daemon behind holding its control socket and every device its graph
had opened under `TIOCEXCL`. The opt-in leash closes this (§15.43):

1. `--exit-on-stdin-eof` (`RunOptions::exit_on_stdin_eof`): the supervisor hands the daemon the
   read end of a pipe as stdin and holds the write end; the kernel closes that end however the
   supervisor dies; the daemon reads EOF and stops **through its normal shutdown path** —
   teardown, socket unlink, claim release — so a leashed daemon leaves no more residue than a
   `SIGTERM`ed one.
2. The default is off, deliberately: under a service manager or `< /dev/null`, stdin is at EOF
   from the first instant, so an always-on leash would kill the daemon at startup. The flag means
   "someone is holding the other end on purpose."
3. The watch is a detached `std` thread blocked in `read(2)`, **not** `tokio::io::stdin` — the
   latter parks an uncancellable blocking-pool task that runtime shutdown waits on, hanging every
   other exit path.
4. `RunOptions` gaining a public field is a semver-visible addition to the embedding API (§8,
   §15.26); the out-of-tree consumer template's `..Default::default()` construction is what makes
   it non-breaking, a contract on every future `RunOptions` change.

The platform primitives were declined with the reason recorded: `PR_SET_PDEATHSIG` is Linux-only
and thread-scoped, kqueue `NOTE_EXIT` is Darwin-only and needs `unsafe` outside
`serial_nexus_sys` (§16.3) — either is a repair that executes on only one platform, AGENTS §9's
proxy in space, exercised nowhere it can be observed failing — while pipe EOF is POSIX, identical
on both kernels, and needs no `cfg` at all (AGENTS §7; §15.43).

### CLI shape is presentation, not contract

The CLI's shape — subcommand names and hierarchy, argument names, output formatting — will
iterate on feedback from its users, human and AI agent alike, and the daemon must stay flexible
to that iteration: nothing in `serial-nexus-daemon` may depend on how the CLI spells things
(§15.16). The stable surface is the RPC method set and its JSON schemas; the verb list above
names semantic operations, not command-line spellings, and a CLI subcommand may be renamed,
regrouped, or composed from several RPCs without any daemon change. All human-oriented rendering
— tables, prose, color — lives in the CLI; the daemon returns structured results and opaque
identifiers only, which makes a raw JSON pass-through mode essentially free and gives AI agents
the choice of driving the CLI or speaking JSON-RPC to the socket directly. Version skew between
CLI and daemon degrades gracefully by construction: JSON-RPC's standard method-not-found error
tells a mismatched CLI exactly which operations this daemon lacks. The two surfaces evolve on
different budgets — the RPC surface deliberately, additively where possible; the CLI shape
freely.

### Windows

Windows is out of scope, declared loudly rather than left ambiguous: PTY nodes have no Windows
equivalent (ConPTY hosts console applications; it does not emulate serial devices), and the
control socket assumes Unix domain sockets (§15.13). The declaration deletes an
interprocess-abstraction layer, keeps the PTY story purely POSIX, and is accepted with eyes open:
supporting Windows later would be a redesign, not a port.

## 11. Configuration lifecycle

### Load, and the incremental verbs

**Load is accepted only on an empty graph** — at daemon startup or after explicit teardown;
`load --replace` composes teardown-then-load so nobody scripts it by hand. Diffing a new file
against a running graph is deliberately deferred (§14), `set-attribute` (attribute surgery on a
live node) with it (§14, §15.25); running graphs change through the incremental verbs —
`add-node`, `remove-node --cascade`, `connect`, `disconnect`, `load --replace` — which obey the
load rules, not a looser copy of them:

1. Edge surgery runs the *same* critical-section structural validation as load (§15.35): an
   illegal `connect` is refused naming the rule, with nothing changed.
2. A transient inability is answered as transient: a `connect` whose hand-off momentarily cannot
   land (the endpoint's inbox full) answers the retryable inbox-full code `-32007` (§10's
   registry), never the permanent `consumer_live: false` — and the fallible hand-off runs *before*
   any mutation, so nothing is ever half-attached to unwind (review 37, 37-DATA-1).
3. `remove-node` refuses while edges are attached unless `--cascade`; removal flushes log queues
   within a bounded wait, and the reply carries the node's teardown accounting (below).

### Structural atomicity, and the pre-create prechecks

**Load is structurally atomic.** The entire file is validated before anything is created; a
structural error creates nothing. The validation set:

1. The three graph rules (§4).
2. Name and identity legality: no `/`, no empties, no whitespace-only names, the §3 length
   bound, no duplicate node names or channel identities.
3. Attribute schemas, including codec tables — a table a codec's schema refuses is a structural
   error (`precheck_codecs`, §8).
4. Every numeric attribute carries a stated maximum, range-checked structurally — an absurd
   `replay_ring` or `hostward_buffer` is a named error, never an allocation — and the rule
   generalizes to strings (§16.12): every identifier riding the wire or a frame header carries a
   stated maximum length (`MAX_NAME_LEN` first), checked at every door — `load`, the incremental
   verbs, the web bridge's re-checks (§10, §15.34).
5. Unknown configuration keys are refused naming the key, so a typo cannot silently become a
   default.
6. A non-empty source that parses to an *empty* graph is refused rather than obeyed — under
   `--replace` that composition is an unannounced `teardown` reporting success (§15.34).
7. Flow control the driver accepts and then silently drops is refused here, before anything is
   created (`precheck_flow_control`; the three-way discrimination — honour, honest refusal,
   accept-then-drop — is §7.1's, §15.53).

**Deliberate:** one check is absent from that list, and the asymmetry is the rule. Resolver-input
well-formedness is `add-node`'s **capture** rule — input-to-identity runs once, at add time
(§12), and `resolve_input` has no other caller — so `load` never re-parses or re-resolves an
identity string. It cannot: an identity-loaded configuration must come up with the hardware
absent, and an identity that no longer binds is a `waiting` node — the environmental arm, not a
structural refusal. Two reviews independently filed this absence as a hole (review 37, 37-CFG-1;
notes §3.24); "fixing" it would break cold starts by identity.

**The prechecks ask the node's own question, before anything is destroyed.** The two pre-create
prechecks, `precheck_codecs` and `precheck_flow_control`, share one contract:

1. Both run before anything is created — under `--replace`, before the teardown half: a bad
   `--replace` never destroys a good graph on its way to failing.
2. A refusal is structural and carries its facts as data (for flow control: `node`, `device`,
   `requested_flow_control`, `honoured_on_readback`) plus the remedy, never prose alone.
3. The question is asked as the node's open will ask it: the pre-check resolves the device
   through `Resolver::resolve_current_path`, the same call the open makes. The first version
   asked `Path::new(device).exists()` after `add_node` had rewritten `device` to canonical
   identity — dead on `add-node`, on `load` of any `dump`ed config, and on startup replay, live
   only for the hand-written literal path its own guard exercised (notes §3.68). Pre-check and
   open must not disagree about the *question*, as §7.1's one-predicate rule forbids two callers
   disagreeing about the *answer*.
4. An absent device never reaches a precheck — unplugged hardware still just waits (§12). The
   two paths that still reach a `faulted` node despite the pre-check, their standing declined
   repairs, and the readable-fault promise are §7.1's contract (§15.53).

**Environmental failures never fail a load**: nodes whose environment is missing come up
faulted-and-wait or `waiting`, visible in state, healing on their own — the
operators-own-the-graph invariant in operational form (§15.8).

### The reply barrier (§15.42)

A config verb's reply is a readiness promise for everything the verb created; the barrier exists
because the reply was once a proxy in time for listener readiness (AGENTS §9's class; plan §3's
presence-is-not-readiness convention) and root-caused a real flake — the measured race and the
refuted accept-backlog diagnosis that competed with it are §15.42's record (notes §3.38). The
contract:

1. A config verb holds its reply until every `listen` leg it created has finished its **first
   bind attempt**. The barrier handles are collected inside the state critical section and
   awaited after the borrow is released — a `RefCell` borrow structurally cannot cross the
   `.await` (§16.2).
2. **Attempt, not success — the distinction is load-bearing.** A refused bind resolves the
   barrier too, the node already faulted with its reason in `state`; §15.8 puts environmental
   failure in node status, never the verb's result — waiting for success would invert that and
   stall the caller for the backoff schedule of an address that may never bind.
3. A 5-second bound caps the wait, so a wedged node task can never make `load` unanswerable —
   an RPC that never replies is worse than one that replies early; the early reply at least
   leaves `state` readable.
4. **Deliberate:** the `connect` role gets no barrier — its readiness is the *peer's*, which no
   reply can promise and no caller should be made to wait for (§15.42).
5. The harness was deliberately not patched around the defect: `itest/tests/p6_hostility.rs` is
   unchanged, so those tests are the barrier's own regression coverage; a retry loop would have
   hidden the defect with every other RPC consumer still racing.

### Replies account for what they destroy

The destroying verbs report their destruction in §5's two-sided ledger form (§15.50): a count of
what was removed beside the bytes removing it cost, because a bare `0` with nothing saying what it
counts is unreadable (§15.49; notes §3.59). There are three of them, and the *count* half is
whatever that verb removes:

- `teardown` and `load --replace` — the largest loss, since it displaces the whole graph — reply
  with **`torn_down`** (nodes removed) and `discarded_at_teardown`. Both fields are always
  present, `0` included, on plain `load` too.
- `remove-node` removes exactly one node, so a node count would say nothing; its pair is
  **`cascaded_edges`** beside `discarded_at_teardown`, with `released_locks` and `purged_bytes`
  for what the cascade cost at the edges (review 37 `37-LIFE-1`). It carries no `torn_down` and
  never has — `docs/rpc/configuration.md` is the schema authority and documents none. *(This
  sentence read "`teardown`, `remove-node`, and `load --replace` … reply with `torn_down`" until
  2026-08-12, over-stating a settled contract that §5, §15.50 and the shipped guards all scope
  correctly; corrected in the design, not on the wire — notes §3.75.)*

### Dump round-trips

`dump` emits configuration only, in exactly the load format; it is the migration story and the
backup story. `state` is a separate verb for everything observed. The split is enforced
mechanically: state fields simply do not exist in the configuration types (§15.8).

### The daemon persists its own configuration

1. After each config-mutating verb — never on read or arbitration traffic — the daemon snapshots
   configuration (same format) to a state file; startup prefers the state file over `--config`.
2. The write is atomic and durable: tmp-plus-rename, with the temp file and its directory
   fsynced around the rename, so a power outage cannot leave a truncated state file (§16.6).
3. The default path is socket-adjacent: `<socket-stem>.state.toml` — the socket's *stem*, so
   `/run/serial-nexus-daemon.sock` yields `/run/serial-nexus-daemon.state.toml` — per-daemon-unique,
   so parallel test daemons never share state; `--state-file` opts into
   reboot durability (§15.25).
4. One stem consequence is stated rather than discovered: socket paths differing only in
   extension (`a.sock`, `a.socket`) derive the *same* state file, so parallel daemons that must
   not share state need distinct stems or an explicit `--state-file` (§15.34, review 32 RV-6).
5. Clean shutdown preserves the graph; only the explicit `teardown` verb persists an empty one.
   A snapshot write failure is logged and never corrupts the running graph. A restart with
   devices missing comes up faulted and heals — restart, replug, and first boot are the same
   code path.

## 12. Device identity

### The resolver and its two directions

Operator input naming a serial port — a raw `/dev` path or a device serial number — is converted
by the **resolver** into a canonical, structured identity stored in configuration:
`usb:<vid>:<pid>:<serial>:<iface>` in the common case. The resolved `/dev/tty*` path is state,
never configuration. The resolver runs in two directions: input-to-identity, once, at add time
(`resolve_input` — §11's capture rule); and identity-to-current-path, at every open and every
faulted-and-wait recheck. One consequence is a rule: a raw-path add requires the device present
at that moment (identity must be captured); adding or loading by identity never does — why
`dump` emits identities and configurations survive cold starts unplugged.

### One source, enumerated over its doors

On Linux both directions read the *same* source — the `<sys-root>/class/tty` device listing plus
the sysfs ancestor walk that derives an identity from it — with `/dev/serial/by-id` as a fast
path *over* that listing, never an alternative to it; still no dependencies, no libudev
(§15.10). Reading one source is a rule, not an implementation note: the two ways to break it are
the resolver's two worst failures, and both were shipping:

1. Capture that walks sysfs while resolution reads only by-id mints an identity the daemon can
   never honour: a box with `/sys` and no udev serial links (a container handed a bare
   `--device=/dev/ttyUSB0`) accepts the add, populates the
   resolved path, then waits forever for a device that is right there (review 32 RES-2).
2. A duplicate-serial guard that counts by-id link *names* cannot fire where the hazard is
   duplicate *devices*, since udev publishes exactly one link per colliding name (review 32
   RES-1, §15.10).

And the rule is enumerated over its doors — a rule implemented at N−1 of N doors is where its
violations live (review 37's thesis, held four reviews running):

3. **Both** bare-serial directions read the `class/tty` listing via one helper — the RES-2
   remediation fixed capture and resolution and missed the bare-serial arm, "the same failure
   class, in the arm nobody tested" (review 37, 37-RES-1).
4. A serial number two present devices answer is **refused naming every device and the by-path
   identity that pins each**, never sorted — sorting picks a physical port the operator never
   named: the wrong-device adoption this section exists to make impossible.
5. `-`, the absent-field marker, matches nothing.
6. Device-node presence is one module-level predicate (`is_dev_node`: present and not a
   directory) applied at every site that asks.
7. Literal path arms reject directories and any parent-directory component **before** the
   fixture-root join — lexical normalization is unsound past a symlink, so `/dev/../dev/ttyX` is
   refused too.
8. A `:` in any captured usb-identity field degrades to by-path rather than minting a string the
   resolver's own parser cannot read back (the tree had minted one).
9. Enumeration output is sorted, so doctor P4 is order-stable.

### Ambiguity: three doors in, and decline is the only arm

An identity two present devices answer to resolves to **neither**: the node carrying it stays
`waiting` rather than driving a coin-flip adapter. The rule binds **resolution as well as
capture** — capture degrading a duplicated serial to by-path (below) does not make the
resolution-side check redundant, because three doors carry an ambiguous identity into resolution
anyway: a configuration an older daemon persisted (`dump` wrote the string), a hand-typed
identity, and the door no history fixes — an identity captured while one clone was plugged in,
whose twin appears afterwards. Declining is the only arm available there, since identity-to-path
resolution answers with a path and nothing else: "bind and warn" has nowhere to put the warning
and would be indistinguishable from binding the right device, at every open and recheck
(§15.10). The two recorded wrong shapes — counting by-id links (refuted at clause 2 above;
review 32 RES-1) and sorting the tie (refused at clause 4) — must not be re-derived.

At add time the CLI echoes the resolved identity in human terms ("bound: FTDI FT232R, serial
A6008isP, interface 0") so the operator notices if the wrong physical device answered; an
ambiguous identity binds nothing, and `add-node` returns it with a warning naming every
answering device and the by-path identity `ports` lists for each. Through `load` or a startup
from the state file there is no echo, so the refusal surfaces as every other unbindable device
does: the node sits `waiting`, indistinguishably from an absent one — the honest residue — and
`ports` shows the two clones, each under the by-path identity that does pin it (§15.10).

### Fallback chain and field hygiene

Adapters with absent or duplicated serial numbers degrade to topology identity (by-path:
"whatever occupies this physical port"), then to a raw-path escape hatch carrying a documented
instability warning. Multi-port adapters — one serial number, several UARTs — are why the
interface index is part of the identity. Field spelling is one rule, sharpened by review:

1. An absent serial or interface is written `-`, never left empty; empty or whitespace-only
   fields are malformed at add time.
2. A whitespace-only sysfs serial normalizes to absent at the source, so it degrades to by-path
   instead of minting a wrong-device-prone `usb:vid:pid::iface`.
3. Configurations persisted before that fix hold the retired empty-field form and come up
   `waiting` until re-added — a safe, operator-recoverable, intended retirement (§15.27).

Existing-terminal nodes (§7.7, deferred — §14) would have no hardware identity and pass through
as path identities.

### The founding premise, measured

The premise this section rests on — §1's "the same adapter does not always return as the same
`/dev` path" — is measured, not assumed, on this repository's own replug lane (§15.45, notes
§3.54). Cycling both adapters in one privileged hold and reauthorizing them in the opposite
order makes serial `BH00LL8O` move `ttyUSB0 ↔ ttyUSB1`; the daemon's `resolved_path` follows,
`identity` is unchanged, and the configuration is never touched. A single-adapter replug cannot
show this — Linux reuses the lowest free minor, so a path-keyed config would survive by luck —
so the test cycles two adapters and chooses the reauthorization order from current state: the
fail-first control (the order returning each adapter to the minor it already held) was run on
the rig, leaves `ttyUSB1 → ttyUSB1`, and the guard refuses it. The privileged mechanism — five
construction bounds, two platform arms, and the rule that a path-accepting verb is a design
amendment, never a patch — is §15.45's contract; its narrowness is a standing tripwire (§5).

### Absence is not "no source": `has_identity_source`

A `waiting` reason must be true on the box that prints it. `Resolver::has_identity_source`
separates "device not present" from "no identity source exists here" (notes §3.72):

1. On a box with no identity source (no by-id tree, no sysfs — macOS today), an identity-form
   node does not claim `device <d> not present` — false where the adapter is plugged in and
   readable — but that the identity cannot be resolved *here*; the status deliberately stays
   `waiting`, because a future IOKit backend (§14) would make the same configuration resolve —
   only the sentence moves, per §7.
2. The predicate is a *directory* test: an empty-but-present by-id tree is real absence, while a
   missing tree is no source — the guard asserts that distinction.
3. **Deliberate:** raw paths are excluded — a raw path is stat'ed literally, so "not present" is
   precisely true for it (notes §3.72).
4. A remedy must be reachable on the system being advised: refusing a bare-serial `add-node` on
   a box with no identity source no longer points only at `usb:`/`by-path:` identities
   (unreachable forever there); both arms of the message come from one free function and both
   point at `serial-nexus-ctl ports`.

### `ports`, the enumeration face

The `ports` verb (§15.35, §10) is the resolver's enumeration
face: it lists candidate serial devices with identity, current path, and whether a graph node
already binds them — and it is strictly *passive*, built from by-id readlinks and sysfs (a
`cu.*` listing on macOS), never `open(2)`, because opening a port to look at it toggles DTR on
exactly the adapters people care about. macOS has no by-id tree; an IOKit-backed resolver is
deferred (§14), with raw `cu.*` paths as the interim. The *transmitting* crossover discovery the
harness and doctor use to find a cross-wired pair on macOS no longer fires unasked — it runs
only under `SNX_CROSSOVER` (plan §3's required-mode table; plan §18 item 5, executed; notes
§3.57).

## 13. Platform support and licensing

The platform matrix, the dependency-licensing policy, and the measurement doctrine governing
every kernel claim in this repository: the doctor, instrument validity, and the comparability
ladder. The incidents that bought each rule are compressed in §15.44, §15.46–§15.49, and
§16.13; the rules live here.

### Platform matrix and licensing

**Linux** is the required platform, the platform of record, and the one all mechanisms are
specified against. Kernel behavior is never assumed even there: kernel claims cite committed
doctor artifacts (provenance rule below), and the two Linux kernels with committed captures —
7.0 and 6.18 — are diffed against each other, not presumed interchangeable.

**macOS** is best-effort: supported where plain POSIX carries the design, degrading — never
crashing, never silently misbehaving — wherever the design leans on a Linux-only facility. It
is runtime-verified on real hardware (§15.30); `docs/macos.md` is the platform matrix of record
and the dated Darwin measurement record. Current suite figures live only in the plan's Status
table, with their scopes; this section quotes none. The named deltas:

1. `cu.*` call-out nodes, never `/dev/tty.*` (those block on carrier detect); no by-id tree and
   no `/sys`, so `usb:`/`by-path:` identities are inert until the deferred IOKit resolver lands
   (§12, §14) — such a node stays `waiting`, its status naming unresolvable-here, not absence
   (notes §3.72).
2. Driver counters gracefully absent: `TIOCGICOUNT` is Linux-only, carried as a mechanism on
   the affected items, never a widened excuse (§15.47).
3. The §7.2 termios-and-flush platform arm (§15.30) — measured, cited at §7.2.
4. A macOS pts cannot stand in for a serial device (the baud ioctl rejects it), so
   serial-device tests self-skip there in favor of the real-rig gate — the §16.7 doctrine,
   served by the provider seam (plan §3, §15.48).
5. The rig's adapters sit on a driver that accepts `CRTSCTS` and reads it back clear, which is
   why `rts-cts` configurations are refused at `load` there rather than left to fault (§7.1,
   §15.53).
6. The replug capability's macOS arm is atomic, so `cycle` works and `hold` refuses (§15.45).

The validation suite itself is a cross-platform Rust crate (plan §2, §15.31). **Windows** is out
of scope, declared loudly at §10: supporting it later would be a redesign, not a port.

**Licensing policy**, as clauses:

1. Permissive licenses only (MIT, Apache-2.0, BSD) for anything linked; no copyleft crates,
   including weak copyleft.
2. Copyleft *tools and daemons* may be used unmodified as external processes — run, never link.
   The §7.6 exec codec exists in part so copyleft tools can join a graph without linking.
3. The rule governs what serial_nexus links, not who links serial_nexus: out-of-tree codecs and
   embedding daemons (§8, §15.26) are an intended use the project's own permissive licensing
   exists to allow.
4. CI-enforced (`cargo deny`; plan §2). The dependency list is part of the security argument for
   the privileged helper (§15.45) and the web console (§17): a new dependency is a reviewed
   decision, never a convenience.

**Selections under the policy.** **serial2** (BSD-2-Clause OR Apache-2.0) provides port I/O —
settings, custom baud rates, modem-line control for the §7.1 verbs, and break — driven by the
daemon's own poll-based readiness (§15.18) over a non-blocking fd, with `TIOCEXCL` and
`TIOCGICOUNT` issued directly on the raw fd through `serial_nexus_sys`. The fd arrives
non-blocking from the dependency itself — serial2 0.2.37's `SerialPort::open` passes
`O_NONBLOCK | O_NOCTTY` as custom open flags and never clears them — and the daemon's own
`set_nonblocking` on it is therefore redundancy, kept deliberately: the readiness loop's
correctness must not rest on a dependency's open flags, which are not part of its published API
(review 37, 37-SER-3 — five sites once asserted the opposite). PTY syscalls come from **nix**
(MIT) or **rustix** (Apache-2.0 OR MIT); raw termios via the same crates is the fallback if
serial2 ever falls short (a watch item, plan §6). The rest is the permissive standard set:
tokio, serde, clap, bytes, tracing.

The declines are recorded at §15.1 and stand: `serial2-tokio` (dropped — no inner-fd accessor,
exactly the contingency §15.1 planned for), the ecosystem-default `serialport` stack (MPL-2.0
weak copyleft, present even under MIT-badged wrappers), and libudev bindings (LGPL linkage;
hotplug uses resolver polling, with raw kernel uevent netlink the dependency-free upgrade,
§14). ser2net/gensio, socat, and conserver: valuable prior art, all copyleft, all fine to run
beside the daemon.

### The doctor

Supporting several systems is a stated requirement, and capability differences are discovered by
tooling, not by bug reports. The project ships **`serial-nexus-doctor`** (§15.17), the one
diagnostic binary consolidating every kernel-behavior probe the design depends on. Its contract:

1. **The probe roster is P1–P15**, and `docs/serial-nexus-doctor.md` is the probe registry of
   record: per-probe questions, verdict grammars, and measured baselines live there, not here.
   Identity probes ask about *devices*, never the by-id directory — the diagnostic cannot skip
   in the one environment §12 grew a fallback for, and a by-id tree absent with devices
   visible another way is `degraded`, never "no adapter".
2. **Passive by default.** Any probe that opens a real serial port requires that port named
   with `--port` — a listed port could be wired to live equipment.
3. **The daemon never consumes doctor output.** Its degradation paths (for example §7.2's
   reconciliation poll) are unconditional, so a wrong probe can mislead a developer but never
   the data plane. The general form: a probe passing is never a license to remove an
   unconditional degradation path.
4. **The doctor is the design's kernel-contact instrument** (§15.36). Probes emit raw
   measurements as structured JSON, never just verdicts. A kernel that *differs* is `degraded`
   with the observation named, never `unsupported` (AGENTS §7); `unsupported` fails the
   process — exit 1, a stop condition: surface the report for a design amendment rather than
   coding around it. `skipped` and `degraded` exit 0. New kernels get diffed, not assumed.
5. **JSON is the artifact of record; Markdown is the view a human reads.** The Markdown is a
   pure function of the JSON minus the generation timestamp; the reverse fails — rendered
   leaves carry no JSON kind while the gates test types — so the jq gates read JSON only.
   `--json-out <path>` writes the JSON twin of the *same* `Report` as the Markdown paste, in
   one invocation, because two runs of one rig are two measurements; `--json` with
   `--markdown` conflicts rather than silently choosing. The report's own header sentence asks
   for the JSON and names the flag — load-bearing protocol: reporters do exactly what it asks,
   and the old wording cost three Markdown-only field visits (notes §3.74). The one-shot
   capture protocol lives in `docs/serial-nexus-doctor.md`.
6. `--field-set <report.json>` recomputes a captured report's field digest; zero observations
   answer exit 2, never a digest — unknown is never "equal" (notes §3.74).
7. **The no-target doctrine and the tier ladder.** Nothing in the project's own test matrix
   requires a real device. Tier 1 is a dangling USB-serial converter — the *baseline*, not a
   corner case: wired to nothing, it exercises identity, enumeration, exclusivity, unplug,
   replug. Tier 2 adds a TX–RX jumper: a real driver-level data path on one clock. Tier 3 is
   two converters cross-wired as a null modem: independently clocked baud verification,
   framing/parity/break observation, modem-line signaling, a physical instance of §4's
   symmetric configuration.
8. **The doctor certifies the rigs itself** (§15.21): an opt-in probe discovers which named
   ports are dangling, looped, or cross-paired — verifying pairs in both directions, so a
   half-crossed wire is named rather than mysterious — and characterizes what the rig
   supports, producing the certificate every tiered checklist run starts from. The UART
   predicate is portable (§15.47), so a rig certifies on any kernel.
   **Misreading callout, kept adjacent:** the shipped certificate contains a deliberate
   **baud** mismatch and local break **assertion** — not a parity mismatch, break reception,
   or far-side modem signalling (review 37, 37-TOOL-3; §15.21). Those belong to the checklist
   run the certificate is the precondition *for*. The parity-mismatch and break-reception
   remainder is plan §18 item 17; the far-side handshake wiring is since measured by P5's pair
   block (clause 9, §15.52), reported and never judged.
9. On a pair discovery has verified, **P14** measures the rig's ceiling — the maximum baud
   rate at which transmission is still reliable — a number plus its reason for stopping, never
   a grade (§15.51). **P5's pair block** reports handshake continuity in the six-crossing
   form, reported and never judged: not-wired is a valid answer, a 3-wire bench being
   legitimate under §5's stated assumption (§15.52); the `rts-cts` end-to-end tests gate on
   that *measured* precondition (`SNX_RIG_FLOW`; plan §3).
10. **The boundary stays crisp: the doctor certifies the rig; it never drives the daemon
    through it.** Driving the daemon over the certified rig is the suite's job; neither
    promise moves (§15.52).
11. **Era discipline.** A new probe id is a new question, and a new question is a new
    instrument: adding one deliberately moves `probe_set` and opens a new fingerprint era
    (§15.51 established the rule at P14's landing). A correction to an existing `question`
    string moves it too, taken when it is worth an era (notes §3.73). What a boundary means
    for diffing is defined below.
12. **Committed artifacts and provenance** (§16.13, stated here as body law). Every run opens
    with a Build block — `commit` (`SNX_BUILD_COMMIT` for reproducible builds), `probe_set`,
    `field_set`, date — and captures land in `docs/doctor/`, indexed by
    `docs/doctor/README.md`. Kernel claims in prose cite a committed report by commit,
    fingerprint, and date, never terminal scrollback. Frozen output is never edited —
    recomputing a digest *over* a frozen file is lawful, rewriting it is not, and an old
    artifact lacking a newer field stays as captured. Non-prose surfaces are claims too: gate
    comments, index columns, shipped strings (notes §3.36).
13. **`Probe::observe` replaces, never appends**, keeping the key's declared position — a
    duplicate key with disagreeing values shipped once, invisible to both digests because the
    field digest deduplicates; gates read observation keys through `last`, so frozen artifacts
    carrying the pre-repair duplicate are read at their answer (notes §3.73).

**The roster at a glance** — one clause per probe; the full registry (verdict grammars, measured
baselines) stays delegated to `docs/serial-nexus-doctor.md`:

| Probe | Question |
|---|---|
| P1 | Does a client `tcsetattr` surface as a packet-mode event, and can the master re-assert EXTPROC? |
| P2 | PTY presence: POLLHUP only while no client holds the slave, clearing on reopen? |
| P3 | Serial fit on a named port: custom baud, `TIOCEXCL`, modem lines, break, `TIOCGICOUNT`? |
| P4 | Does the resolver's one source yield `usb:vid:pid:serial:iface` identities? |
| P5 | Rig discovery and certification: is each named port dangling, looped, or cross-paired — and certified? |
| P6 | Does a hung-up pty master keep asserting POLLIN with nothing to read? |
| P7 | What evidence does a collapsed client session leave on the master? |
| P8 | Does epoll report a pty master readable while `read(2)` returns EAGAIN? |
| P9 | What does a zero-or-short `poll(2)` timeout actually cost on a never-ready tty fd? |
| P10 | How many bytes does a pty accept per direction, and how many can a reader recover? |
| P11 | Do `TIOCGICOUNT` and `TIOCMGET` answer on a real port, and what do they read? |
| P12 | Does an edge latch report a session that left nothing readable, staying silent while idle? |
| P13 | At a pts last close, are unread client bytes retained, discarded, or does the close wait? |
| P14 | The highest baud at which a payload round-trips byte-exact both ways, and what stopped the search? |
| P15 | Does a named port honour a requested flow-control mode, or accept it and silently drop it? |

### Instrument validity

Probes, guards, and gates are instruments, and the record shows instruments failing in
characteristic, repeated ways. This is the checklist a new probe or gate author follows; the
incidents behind each rule are compressed in §15.46–§15.49.

**Self-testimony (§15.46) — three rules with tripwire status for every measuring instrument.**

1. A probe re-asserts the configuration its question assumes *on the very object it measures*,
   reports it as data (`slave_termios_mode` is the exemplar), and degrades when the measured
   mode is not the assumed one — a run that could not ask its intended question reports that,
   never a confident number. The re-assert is unconditional, never cfg-gated off the platform
   of record: a repair that executes only where the defect was observed is the same proxy again
   (AGENTS §9). *Exception, stated with the rule:* where the shape contrast **is** the
   instrument (P12's a/b/c shapes), a re-assert would destroy the stimulus — the exception
   covers stimulus, never setup.
2. **Acceptance is not delivery.** A filling probe drains the far side and reports
   `bytes_recovered_by_peer` and `bytes_unrecoverable`; a probe that counts what is readable
   drains and reports its own footprint — an instrument that does not account for itself
   counts itself (§15.46).
3. **A discriminator must be able to fire, and an axis must be able to vary.** A sampling
   design whose every admissible hypothesis predicts the same reading is not a measurement;
   replicates wearing a factor's name are not levels. The instrument states in its own output
   what the reading does *not* license.

Corollary: cross-kernel diffs read configuration observables first, payload figures second,
scheduling stories last — a delta between runs whose `slave_termios_mode` differs is not a
kernel delta at all.

**A zero is a claim (§15.49).** A reported zero carries its witnesses or it is not a
measurement:

1. A wall-clock witness in the unit of the loop's *true* cost, measured, never styled — a
   millisecond field over a microsecond loop prints `0` and witnesses nothing.
2. A positive control on the same instrument instance: the quiet instrument is shown able to
   see a one through the same mechanism it watches; a missing control makes every idle count
   `unmeasured`, never `quiet`.
3. A pacing arm at the mechanism's own cadence: "no edge in back-to-back syscalls" and "no edge
   at the mechanism's cadence" are different claims.
4. An inert arm proves itself inert. A probe may carry observations while `skipped` —
   **Deliberate:** that is the negative control the kernel that depends on the mechanism cannot
   provide for itself (P12 on Linux; §15.49).
5. A wrong field is kept when its being wrong is the finding. **Deliberate:** P10's
   `peer_pending_input_bytes` answers empty beside a kilobyte the same probe then recovers on
   one kernel, classified by `fionread_trust`, and the probe's status deliberately does not
   move — folding an auxiliary ioctl's fault in would make one platform permanently yellow and
   lose the direction information (AGENTS §7; §15.49). A cleanup that "fixes" either deliberate
   shape silently drops a finding.
6. A `supported` verdict states a non-zero population; a tree where nothing resolved is
   `degraded`, not `skipped` — the question was asked and answered negatively (§15.49; review
   32's RES-2 decline stands, narrowed).

**The vacuity taxonomy — five shapes of one defect**, each found separately in this tree before
being named as a class; a new instrument is checked against all five:

1. *Zero-iteration loops*: an all-pass fold over an empty population.
2. *Discriminators that cannot fire*: a single sample at a point every admissible threshold
   predicts identically.
3. *Axes that cannot vary*: replicates wearing a factor's name — a 2×2 that was a 1×2.
4. *Certificates that cannot fail*: a predicate false on an entire platform routes every input
   to the accepting arm, so a cleanly wired rig reads `supported` whatever the hardware does.
5. *Guards pinning the platform-of-record's answer* instead of the promised property: green
   where written, red or vacuous everywhere else.

The catches, stated once: a verdict must be reachable from a failing input *on the platform
the instrument runs on*; every control asserts its own application; gate cases are run rather
than predicted; plan §3's fail-first checklist applies to probes as to regression guards.

**The portable certificate, and unmeasurable as data (§15.47).**

1. A capability predicate goes portable by *widening*, never replacement — a widening cannot
   lose a port, a replacement could: AGENTS §7's one-way decision applied to predicates.
2. Unmeasurability is data, not absence: `CertFailure.unmeasurable_here` carries the mechanism,
   set at exactly the sites that read the unavailable facility, the constructor taking the
   answer as a required argument so the compiler forces every new item to state its kind. The
   excuse can never widen to items every kernel measures — and where the facility exists, its
   failure is a *real* measurement and the mechanism clause must never appear.
3. **Verdicts are pure functions of measured facts.** AGENTS §9's proxy rule cuts both ways: a
   decision may not be *asserted* on a kernel it was not measured on, and a pure function of
   measured numbers may and must be — which is how a verdict is unit-tested on a box that
   cannot produce one row of its input (`p5_verdict`, `p12_verdict`, `choose_pair_source`,
   P14's folds).
4. Verdict-arm ordering is load-bearing. A fact every reader needs is hoisted out of the
   status decision and printed by every arm — *without reordering the arms*, because nothing in
   the type system stops a reorder from silently flipping a platform's verdict; the constraint
   is stated beside the code it governs (§15.47).

**Proxy rules.** A proxy in space is a platform-specific observable standing in for the
portable property; a proxy in time is a precursor standing in for the condition it precedes.
The tell that the real property was found: the portable form comes out *stricter* on the
platform of record, not weaker (AGENTS §9). Kernel-behavior premises inside probes and guards
are derived from the measurement itself or cited to a committed per-kernel artifact — never to
the standard, which one guard cited for a premise Darwin then measured false (notes §3.65).

**Gate design.**

1. **Presence, never answer — in both directions.** An expectation clause requires a
   measurement's presence and type, never its value, so a kernel that differs reports instead
   of failing (AGENTS §7); pinning one rig's answer reddens every honest bench (§15.52). The
   opposite failure is real too: a clause must not *admit* a vacuous shape (§15.49's
   population clause).
2. **Plant the violation in every spelling the clause claims to cover** — including `degraded`
   (the shape a real rig produces) and the passive path, where probe-conditional clauses are
   vacuously green. The two-guard pattern closes gate holes: one guard proves the *clause*
   admits every legitimate shape without loosening, one proves the *probe* produces the shape
   (notes §3.64).
3. **`skipped` is never an error path's output** — it is the one word that exempts every
   conditional gate clause, so an error path routing to `skipped` exempts itself from exactly
   the clauses that would catch it (notes §3.68). Every skip class carries a `SNX_*=required`
   spelling or a synthetic-antecedent guard (plan §3).
4. **Gate strictness is decided against the real input population.** CI pipes a live report
   into the expectation files; frozen artifacts are checked only by dedicated meta-gates and
   pinned recomputations — a strictness argument about "the archive" is about inputs the gate
   never consumes (notes §3.48).
5. **Named jq hazards.** Type ordering: strings compare above numbers, so an untyped
   comparison can pass on type confusion — clause values are type-checked. Duplicate keys:
   read through `last` (doctor clause 13). Gate spellings, including the load-bearing `-e`,
   are plan §3's.

**Gate blind spots, enumerated as a rule.** What each instrument reads — and therefore what
none reads: the jq gates read *observations*; `probe_set` reads `(id, question)` strings;
`field_set` reads observation leaf paths; **nothing reads prose**. Every operator-facing
consequence or remedy string is therefore an ungated instrument: it is either *computed from
the mechanism it describes* (the socket-path pattern, §10) or *guarded as properties* with
negative clauses naming the retracted claims a revert would restore (four incidents bought
this rule — notes §3.72–§3.74). The instruction that matters is the one the tool prints at the
moment of use.

### The comparability ladder and the era record

"May I diff these two reports field by field?" is answered here and nowhere else. The ladder,
strongest basis first, with what each rung licenses:

1. **Same source.** `git diff <a> <b> -- doctor/ core/ sys/ rpc/ codec-api/ Cargo.lock` empty
   between the two stamped commits: the same doctor source on both sides. Closes, for this
   pair only, the residual blindness stated below, and licenses attributing every cell
   difference to the kernel or the run (notes §3.73).
2. **Same binary.** `git diff <a> <b> -- '*.rs' '*.toml'` empty: the same code, checked over
   the broader path set — stated in prose beside any diff that uses it, because neither
   fingerprint can state it (§15.44).
3. **Equal `field_set`.** The two reports carry exactly the same cells, so a field-by-field
   diff has no missing ones — provable, because the digest is computed from the very thing it
   certifies. Unequal means diff only the intersection. Equal `field_set` does **not** certify
   equal probe bodies.
4. **Equal `probe_set`.** The same questions were asked — two instruments of one lineage. This
   licenses *nothing* about fields; only the negative direction is sound: unequal means two
   different instruments, and the diff is refused.
5. **Unknown.** A report whose `field_set` cannot be computed (Markdown-only, or zero
   observations) has an unknown cell set, and unknown is never "equal".

For run-varying quantities one capture is never enough at any rung: three sequential runs is
the floor — a single sample of a varying quantity is indistinguishable from a cross-kernel
difference (plan §3).

**The two digests, each with its negative sentence.**

- **`probe_set`** digests the deduplicated, sorted set of every probe's `(id, question)`
  strings; titles are deliberately excluded (a per-port title would make a one-port and a
  two-port run of one binary read "not comparable"), observations and verdicts excluded
  because they are the measurements the diff exists to compare. *Negative sentence:* equal
  `probe_set` does not license a field-by-field diff — measured, not argued: five commits
  printed `a131e1f4b46d6c83` across six distinct observation sets, up to 71 newly-present
  leaf paths apart, one pair's P10 hostward figure moving 4104× under six added paths
  (§15.44, which also registers the withdrawn figures that once described this movement).
- **`field_set`** digests the sorted, deduplicated set of scalar leaf paths under
  `.probes[].observations` — values excluded, arrays collapsed to one `[]` step, JSON kind
  excluded (measured: kinds differ across healthy same-binary runs), keys sorted so the
  digest is deterministic across runs of byte-identical code (notes §3.72). A property of the
  **run**, not the binary, recomputable for any captured JSON report. *Negative sentence:*
  equal `field_set` does not certify equal probe bodies.

Printed beside both digests, everywhere both are described: **neither digest sees a body
change that moves a number without moving a key.** That residual is a stated limitation beside
a recorded decline, not a schedulable fix: folding observation keys into `probe_set` is
declined *with measurement* (§15.44) — some keys are themselves measurements, others device
identities, and folding would make a passive and a rig run of one binary report themselves
incomparable. What neither digest can announce is announced by hand in
`docs/doctor/README.md`; the same-source rung is the only mechanical closure, pair by pair.

**The era law.** A `probe_set` move closes an era:

1. Artifacts inside a closed era stay comparable *with each other* under the ladder above; no
   later capture joins them.
2. Captures are never diffed across an era boundary without the mismatch stated — even for
   probes whose questions did not move (the current boundary's spelling: P1–P14 must not be
   diffed across it without the mismatch stated, the move having been P15's alone).
3. A new probe id opens an era deliberately (§15.51). A correction to an existing `question`
   string also moves the digest, taken on the recorded criterion: a doctor worth collecting
   measurements from across every box is worth an era, and a wrong citation only gets dearer
   as captures accumulate under it (notes §3.73, overturning notes §3.68's decline per §5).
4. `field_set` moves independently — cells added under unchanged questions — and closes
   nothing; that movement is what the second digest exists to announce (§15.44, §15.52).

**The era record**, one line per era; `docs/doctor/README.md` is the operational index of
record and carries the per-artifact rows, including eras predating both digests.

| `probe_set` era | Instrument | Held captures (bounding artifacts) | Opened / closed by |
|---|---|---|---|
| `01b257ece8c48470` | the P1–P12 roster of its day | the 2026-07-29 Linux pair, the 07-29 6.18 Tier-3 Markdown, the 07-30 macOS capture | closed by the 2026-08-05 P13/P10-rename move |
| `a131e1f4b46d6c83` | P1–P13 | the 2026-08-05 cross-kernel campaign: the `71fc5a8`, `4b78fff`/`1a9a8fc` (same-binary pair), `7ead470`, and `f8315cc` triples | closed by P14's landing — the digest's first move, and deliberate (notes §3.57) |
| `94d64d8bbacf1174` | + P14 | the `42eac2a` macOS triple against the `3d850cf` Linux triple — P14's first cross-kernel reading (§15.51) | opened by P14 (notes §3.57); closed by P15 (notes §3.65) |
| `82a8e2198e54626a` | + P15 | the `7cf0338` Linux triple, the `acb5162` macOS triple, the 6.18 field report at `3e23c52` — holding the first lawful cross-kernel Linux comparison (6.18 ↔ 7.0, same-source basis) | opened by P15 (notes §3.65); closed by the P15 `question` correction (notes §3.73) |
| `e79f5fcd86a2e5f0` | P1–P15, corrected citation | the current era; first artifacts committed at its opening | opened by notes §3.73; open |

Two reading rules close the section: a cross-kernel claim quotes the rung it stands on — "same
binary", "equal `field_set`", or the era and fingerprint pair — and a diff without its basis
is treated as uncited (§16.13); withdrawn figures are never re-quoted (register at §15.44).

## 14. Deferred work

Recorded so deferral stays deliberate. This register holds design-scope deferrals — capabilities
the model names, or could name, that the system does not yet provide — each with what covers the
need meanwhile and the decision-record entry that declined or scoped it. It is the one
enumeration: the entries that defer from their own Implications paragraphs (§15.4, §15.7, §15.9,
§15.10, §15.14, §15.20, §15.23, §15.25) point here rather than keeping private copies. Defects,
owed captures, and open measurements are not deferrals; they live in plan §18, the work ledger.

Entry numbers are positional anchors: a citation of the form §14.N resolves to entry N below, and
the numbering is append-only — an entry that graduates or exits keeps its number and its tag
rather than vacating the slot.

### The deferral vocabulary

Every entry carries exactly one of five states; the record once used four different lifecycle
verbs in four places, and these are now the only ones.

- **deferred** — named future work no configuration can reach; this entry is its only surface.
- **refused-at-load** — the model specifies it but the implementation does not exist, so a
  configuration naming it is refused at load, listing what does exist, with nothing created.
  The refusal is live, **tested** behavior — never a silent no-op. Two shapes qualify, and the
  entry says which: where the **schema admits the words**, the refusal is a structural error
  from `GraphConfig::validate` naming the deferral and its section (entry 14); where the schema
  does not admit them at all, the refusal is serde's own unknown-variant error at
  deserialization, which lists the shipped kinds and cites no section (entry 15). The second
  shape is weaker in what it *says*, not in what it *does*, and §18 item 45 carries the
  decision about upgrading it (notes §3.75).
- **accepted-and-waiting** — validation accepts the configuration and the graph loads; the
  instance waits for a missing driver.
- **graduated** — a driver arrived and the capability shipped; the body sections named in the
  entry now own its contract, and the entry remains as the record of the move.
- **exited** — the entry left this register under a recorded decision entry, cited, rather than
  by a §14 driver.

### The register

1. **Configuration diffing and reconciliation** against a running graph — *deferred*. Load on an
   empty graph is the v1 constraint (§11); `load --replace` and an accepted outage cover the
   need. Declined at §15.9.
2. **Native client-termios propagation to hardware** — *deferred*. Observe-only now: the pty
   observes client termios as state (EXTPROC observe-then-decide, §7.2), and the
   subscribe-plus-RPC experiment path exists precisely to inform this decision (§15.14).
3. **Lock request/grant wire frames** for fail-fast cross-machine arbitration — *deferred*; the
   capability bit is reserved (§9). Until they ship, remote contenders block instead of failing
   fast — §6's documented limit, recorded at §15.7.
4. **The conserver-style attach-time replay ring** — *graduated*. The web console was the driver;
   §5 and §17 own the contract (§15.28, §15.32).
5. **An explicit labeled-combiner node** for merged logs — *deferred*. Implicit merging is
   deliberately unrepresentable (§4); merging must be an explicit framing node (§15.4).
6. **In-daemon TLS and non-loopback legs** — *deferred*, and distinct from §17's shipped web
   `--tls` (§15.29), which terminates at `serial-nexus-web`, a client; the daemon's leg listener
   still binds loopback, SSH the remote transport (§9, §15.12).
7. **uevent-based hotplug detection** replacing the resolver poll — *deferred* (§15.10).
   §15.45's replug lane is not this — it exercises a real hotplug in tests, below the daemon;
   detection remains the §12 resolver poll.
8. **An IOKit-backed macOS resolver** — *deferred*; raw `cu.*` paths are the interim form (§12).
   The IOUSBLib replug backend (notes §3.66) is not this either — it re-enumerates a named
   device for the test lane and resolves nothing.
9. **Swapping the framing substrate (yamux)** if per-channel flow control is ever needed —
   *deferred*; a contained change under the §9 contract, where the evaluation and decline are
   recorded (§15.15).
10. **systemd socket activation** — *deferred*; covered meanwhile by `packaging/`'s unit
    starting the daemon directly — never scheduled, no declining entry.
11. **A per-open generation epoch on PTY slaves** — *deferred* (§15.20). It would close §6's
    sub-interval close-and-reopen window on the detach-release path: poll-sampled presence
    cannot see a close and reopen inside one poll interval, so a successor client can inherit
    the lock — stated beside the deferral so the blind spot stays documented next to the lock
    lifecycle (§6).
12. **A driver for standalone re-multiplexing codec instances** (`faces = host`) —
    *accepted-and-waiting*: validation accepts them and today they load and wait (§7.5; §15.23).
13. **`set-attribute`** (attribute surgery on live nodes) — *deferred*; remove-and-re-add covers
    it (§15.25; §16.10's surviving deferral is this one). `connect` and `disconnect` *exited*
    this register with §15.35 and are live verbs (§10).
14. **The serial output leg** (`faces = target`, §7.1) — *refused-at-load*: accepted in the
    model but refused until a driving use case arrives, with a structural error naming the
    deferral.
15. **The existing-terminal node** (§7.7) — *refused-at-load*, in the vocabulary's **second**
    shape: specified in the model, not implemented, and `existing-terminal` is not a
    `NodeConfig` variant — so a configuration naming it is refused by serde's unknown-variant
    error at `INVALID_PARAMS`, listing the node kinds that do exist, with nothing created. It is
    a real refusal and it is now tested (`existing_terminal_is_refused_at_load_listing_the_
    shipped_kinds`), but it is **not** entry 14's structural form: it names no section and cites
    no deferral. Making it structural is plan §18 item 45, filed rather than done quietly, and
    the guard asserts today's behaviour so that upgrade reddens it loudly (notes §3.75).

*§17 follows; §15–§16 are the decision record and read as an appendix — the numbering is stable
across generations by rule (front matter).*

## 17. The web console client

`serial-nexus-web` is a separate binary and crate: an RPC client of the daemon on one side, an
HTTP and WebSocket server — loopback by default — on the other. The daemon does not link it,
serve it, or know it exists; everything it does rides the §10 surface — `state` and `subscribe`
for the console list and lock badges, `tap.open`/`tap.close` for bytes, `send` for input, `info`
for provenance — §15.16's separation paying out a third time: the web client works unchanged
against any embedding daemon (§15.26). The tap contract itself (offsets, `tap.closed`, bounded
queue, counted drops) is §10's; this section is its consumer.

### The interface contract

1. A left rail lists every host-facing endpoint as a console — display address, node status,
   lock holder and waiter count, live via `subscribe`.
2. Selecting a console opens (or foregrounds) its tap: a terminal view of the hostward stream,
   preceded by the replay ring — present by default (§5) — with a visible marker at the splice;
   the quiet affordance survives only where an operator set `replay_ring = 0`.
3. The single-line input drives `send`; a LOCKED refusal shows the holder by name with an
   explicit steal affordance — never an automatic steal.
4. The tab's own tap drop counter is shown when nonzero — §5's honesty applies to browsers too.
5. The *graph page* renders the whole graph — types and facing, edges with write modes,
   `active`/`waiting`/`faulted` as live indicators via `subscribe`, lock holders badged — and
   reads *both* halves of the split: topology from `dump` (configuration), status from `state`
   (observation, §15.8). Taking the whole thing from either verb would invent a third view of
   the graph that neither reports.
6. The *editor page* drives the graph-editing verbs: add and remove nodes from a palette,
   connect and disconnect edges with the §4 rules enforced by the daemon and surfaced inline,
   destructive operations behind an explicit confirmation naming what cascades. Both pages
   joined the console view with §15.35.
7. The layout is the contract; the rendering is presentation and iterates freely under §15.16's
   rules.

### Implementation stance

The frontend is static assets embedded in the binary — no Node toolchain, no bundler. Escape
sequences are kept legible by the minimal ANSI *subset* arm, the arm "no bundler" makes affordable
(the alternative was a vendored terminal renderer): a dependency-free module rendering SGR
reset/bold/dim and the 8/16 colours, honouring `\r`, `\b` and erase-to-end-of-line within the
current line, treating screen clears as a clear, and **consuming and counting** every other CSI,
OSC, DCS and ESC sequence instead of letting it reach the DOM as visible litter — the counter is
§5's honesty on the one surface where the operator cannot otherwise see what was thrown away. It is
a resumable state machine (at serial rates a sequence split across two `tap.data` notifications is
the ordinary case), and deliberately not a terminal emulator: no grid, no cursor addressing, no
alternate screen — a scrollback pane cannot honour state the daemon never sends. It reads decoded
text and emits render operations, so the byte log behind the export and the OPFS (Origin Private
File System) record stays the raw device stream (§15.32): the export is a device log, not a
transcript. The server side uses permissive HTTP and WebSocket crates under the §13 gate; the
browser link carries base64 chunks relayed from `tap.data`.

Taps open lazily per viewed console and are released a grace interval (a minute) after the
console stops being watched — watched meaning the console pane showing *and* the tab not hidden:
a switch to the graph or editor page hides the console as a unit and leaves a tap streaming with
no console on screen, which fails even a charitable reading of "on blur". The release is
announced in the terminal so a later gap marker is attributable, and the re-open runs the
fresh-selection path, so §15.38's epoch re-anchor and §15.32's ring-gap marker cover whatever
the interval cost; holding a tap costs a bounded queue and some base64, releasing one costs a
re-open that may announce a hole — why the grace is long, not tight.

The UI's behavior is CI-tested in real headless Chromium via a pinned Playwright suite, launched
and gated from `serial-nexus-itest` under the standing self-skip discipline (§15.37, plan §15) —
the OPFS splice, the `tap.closed` re-anchor, and the editor flows are asserted in the browser,
not assumed from the API.

### Security, stated precisely

Same posture as the wire (§9), plus one delta the design must not blur: the control socket is
mode 0600, but a loopback TCP port is reachable by every local user.

1. **Token.** Every request and WebSocket upgrade requires a per-session bearer token —
   generated at startup, printed as a ready-to-open bootstrap URL, then carried as a
   `SameSite=Strict` cookie and dropped from the address bar. The cookie is named for the
   listener that set it (`nexus_session_<bound port>`, §15.29) so two consoles keep two sessions
   instead of evicting one, and *every* value the browser sends under that name is checked, not
   merely the first — a sibling-port page can plant a longer-path cookie of the same name, and
   RFC 6265 orders it ahead of the real one.
2. **Host and Origin.** The Host header is validated on every request, loopback included (DNS
   rebinding); SameSite doubles as CSRF protection for `send`; Origin is validated against the
   request's own authority, so a sibling port on the same host is not this origin. Off loopback,
   Host validation extends to the configured names and the token moves from the bootstrap URL
   into a header after first exchange, shrinking the URL-leak vectors (history, referrers,
   shoulders).
3. **Split credential.** The token is shell-equivalent (§15.35: graph editing is daemon-user
   capability), so it is offered on the
   WebSocket upgrade path alone — the one path a browser needs it on, and the path behind which
   every capability lives — while a second, freshly minted asset credential rides `Path=/`,
   where a navigation or a `<script src>` can present nothing else. That credential is worth
   exactly the static assets this project publishes in its own source tree and is refused on the
   upgrade: a sibling-port harvest authorizes reading this console's JavaScript, not commanding
   its devices (§15.29).
4. **Screening proxy** (tripwire, AGENTS §4). The bridge parses each frame as exactly one
   JSON-RPC request — never scans, so a second request smuggled behind a newline is refused, not
   forwarded — and forwards only verbs on an explicit allowlist (§15.34). With §15.35 the
   allowlist includes the graph-editing verbs; lifecycle verbs (`shutdown`, `teardown`, `load`)
   remain outside it.
5. **Pre-auth bounds**, design numbers reached by measurement and stated so they cannot drift:
   **one** deadline taken at accept, spanning the TLS handshake plus the delivery of a complete
   request head — two sequential deadlines let a peer that drips a `ClientHello` and falls
   silent hold a slot for twice the ceiling (37-WEBS-5); a capped head; 32 pre-gate slots
   against 128 post-gate connections (`503` over cap); bounded WS messages and frames. Both
   cookies are `Secure` under TLS; `docs/security.md` carries the exact constants and the
   measurements behind each.
6. **Eviction, not refusal.** The pre-auth pool is **separate and disjoint** from the post-gate
   connections and bounded by **evicting its oldest member, never by refusing an accept**. The
   first attempt is recorded because it will be re-proposed: a *reserve* enforced by refusal at
   accept — but the cookie cannot be read until the head is read, so a reserve refuses the
   operator's own browser exactly as readily as a flood, and the audit measured it making the
   denial four times cheaper rather than closing it (37-WEBS-6). Under eviction a silent peer
   can only ever be the thing evicted, while the operator's browser — the newest arrival, out of
   the pool a round trip later when its cookie is read — is the last candidate, not the
   structural victim.
7. **Refusal observability, and the lingering close.** A tripped bound closes with code `1009`
   *and* a log line — a bound nobody can observe being enforced is a bound nobody can tell is
   still there — and the `1009` actually arrives: the refusal half-closes and drains the peer's
   remaining bytes into one fixed small buffer, bounded in bytes (`LINGER_BUDGET`, 1 MiB) and
   time (`LINGER_DEADLINE`, 250 ms), until the peer's FIN, because `close(2)` with bytes queued
   is an RST, which destroys the peer's receive queue *including the Close frame itself* — an
   unlingered refusal delivered its code to a real browser about two times in three
   (`docs/security.md` holds the measurement). Only an oversize refusal lingers; other protocol
   errors close `1002` and pay nothing; `shutdown()` sits inside the timeout (a TLS
   `close_notify` can pend forever against a shut window); the enforcement never depended on the
   notification arriving. Refuted categorically — do not re-propose `SO_LINGER`: it waits on the
   *send* queue while the RST is caused by a non-empty *receive* queue (`SO_LINGER{1,0}` forces
   the reset), and only reading empties a receive queue. The drain's cost is priced, not hidden:
   the hold per refusal rises from ~0.5 ms to the 250 ms deadline, so the pool turns over
   boundedly under sustained refusal — a denial strictly weaker than what the same credential
   already buys by pinning the established pool, which carries no keepalive by accepted design
   (the claim that the drain "cannot make things worse" was false in the availability dimension,
   and the record says so). The browser surfaces `CloseEvent` codes, so a refused frame, a dead
   daemon (`1001`) and a pulled cable print different words.
8. **Deliberate:** no per-peer-IP cap — on the loopback default every local user shares
   127.0.0.1 with the operator, so it would ration the browser without separating it from the
   attacker (§15.29).
9. **Three-tier bind policy** (§15.29): the token answers *who may act*, the channel *who can
   read and replay*. (1) Loopback plus token by default — the kernel is the channel; nothing on
   the wire to sniff. (2) `--tls` (rustls, permissive) plus token, the sanctioned non-loopback
   mode — the configuration in which "bearer tokens are like API keys" is actually true, since
   every widely deployed API rides an encrypted channel. (3) `--insecure-bind`, the
   leg-precedent named footgun for networks the operator genuinely trusts — token still
   mandatory, and the flag's help text states plainly what is forfeited: every console byte, and
   the token itself, readable and replayable by anyone on the path.
10. **The residual, stated rather than implied.** Path scoping narrows but cannot close: a page
    *served from* a sibling port is same-*site*, so it can fetch its own `/ws` with credentials
    and log what the browser attaches; ending that auto-replay means taking the token out of
    cookies altogether, which no browser can do for the requests that must also be gated
    (§15.29, review 32 WEBS-1, `docs/security.md`).
11. **The authorization model is unchanged throughout.** The web server is simply a client that
    holds the socket, and whoever holds the token holds exactly what the web server holds.

### Browser-side history

Scrollback beyond the ring lives where the viewer lives (§15.32): the client persists each
console's stream in the browser's OPFS — append-only per-console history
keyed by the daemon's socket path, the endpoint address and the daemon `instance` nonce, with
the endpoint's offset-space `epoch` (§15.38) stored *beside* the bytes, never folded into the
key, spliced exactly by the §10 tap offsets so a reload never duplicates ring bytes into the
stored log. The epoch's placement is load-bearing: keyed by epoch, every graph rebuild
(`load --replace`) would open a fresh, empty file and orphan the last, destroying scrollback
instead of re-anchoring onto it — the failure §15.38 fixed, arriving from the other direction. And the
heuristic the epoch replaced is refuted — do not reintroduce it: offsets alone cannot tell a
daemon offset-space reset from an ordinary reload against a non-empty ring, and each wrong guess
fails differently — duplicated storage (measured in Chromium: one ring copy per reload) or a
frozen console (§15.38). §15.28's principle survives in its strongest form: the daemon and the
graph are untouched, and the party that decided to record is the party whose disk fills — the
viewer records for the viewer.

Two clauses of the retention story are load-bearing and easy to get wrong in opposite
directions:

1. **Retention is bounded by consoles, not by daemon restarts.** Because a record's key folds in
   the per-boot `instance` nonce, a cap enforced only within a boot is not a cap at all — every
   restart orphans one full record per console, forever, until the origin's quota is reached and
   persistence dies for good. So the client sweeps every stored record that does not belong to
   the current origin and the current boot, once per connection, and the per-console cap becomes
   a bound on what accumulates rather than on what one boot retains.
2. **A same-epoch `from_offset` past the stored frontier is a real hole.** The daemon's ring
   rotated over the bytes between while nobody was watching — with the default 64 KiB ring the
   ordinary outcome for a talkative console — so the client charges the lost byte count once and
   marks it ahead of the replay marker, which would otherwise positively assert a continuity
   that is false. The marking is a terminal annotation only: the stored and exported bytes stay
   a raw device log, because an export that interleaves the client's commentary with the
   device's output is no longer the thing an operator attaches to a bug report.

Retention is a per-console cap (default 16 MiB, trim-oldest) with export and clear controls.
`clear` cancels the pending debounced snapshot and detaches the buffer *synchronously* before
asking for the delete, and the delete is serialized against same-key writes — the confirmation
dialog blocks the renderer, and a snapshot that came due while it was up otherwise fired at the
first `await` and re-created the record the operator had just confirmed destroying. The client
requests `navigator.storage.persist()` and *shows* the answer: origin storage is evictable, and
honesty about best-effort persistence beats pretending. Two operational consequences are stated
plainly: the server binds a stable default port (the storage origin includes the port; an
ephemeral port would orphan history on every restart), and stored console output lives
unencrypted in the browser profile — on shared machines, and doubly under the insecure tier,
clearing site data is part of walking away. Without OPFS the client degrades to memory-only
history with a visible indicator; **deliberate:** there is no second storage backend.

### Posture and non-goals, recorded

The web client edits the graph — §15.35 superseded the original never-mutates non-goal, noted
here inline: a token holder already commands every configured console, and the settled posture
is that web access is operator access. The allowlist scopes that power to the graph-editing and
observation verbs (lifecycle verbs stay off the wire from a browser), and the capability
statement is made plainly in the security docs: graph editing is daemon-user capability — a log
node writes files, an exec codec runs commands as the daemon's user — and whoever holds the
token is trusted with exactly that. The web client still does not tail log files: the ring plus
browser-side history is the scrollback mechanism, and server-side logs remain an operator
recording decision (§15.28's decline stands). It is single-daemon per instance in v1 (run two
for two daemons). And its authentication story is the token, full stop — no users, no passwords,
no sessions to administer; remote access is SSH forwarding or the TLS mode, and the token rides
whichever channel is chosen.

## 15. Historical decisions

This section and §16 are the decision record: one entry per decision. §1–§14 and §17 state the
system as settled; the entries say why, and hold everything that must never be silently reversed.
Entries are compressed; where a rule an entry minted is live, the normative statement is body
text and the status line names that home. A few entries still carry normative text of their own
(§15.21's certificate paragraph is the exemplar) and say so.

**Conventions.**

- **Status headers.** Every entry opens with a bold **Status:** line, from a fixed vocabulary:
  LIVE (usually "— restated at §X", the body home); SUPERSEDED-IN-PART by §Y — current truth at
  §X; OVERTURNED by notes §3.NN; NARROWED-BY; DECLINED — STANDS; EXECUTED. Current truth is
  stated once, post-correction; history sits below it, never interleaved. §16 follows the same
  conventions.
- **The no-renumber rule.** Entry numbers are append-only and permanent (a front-matter rewrite
  invariant): frozen `docs/doctor/` artifacts, gates, and code cite §15.N by number, so an entry
  cited by immutable evidence is never renumbered, its number never reused; an emptied entry
  becomes a stub. This record keeps the prior generation's numbering through §15.53.
- **Refutations are decisions.** A refuted diagnosis or a declined proposal is recorded like an
  adopted design — falsifier (or reason), outcome, status — because refutations are what stop a
  rejected shape from being re-proposed on no new evidence. Silently re-fixing a declined item is
  a defect (AGENTS §5); overturning one takes new measurement or an explicit recorded decision,
  never drift (§15.48 carries the exemplar overturn, notes §3.37 → §3.43).

**Topic index.** "Was this decided, declined, or refuted?" should be one lookup. Every entry
§15.1–§15.53 appears below, titled at its primary topic (numbers alone on repeats); an entry can
appear under more than one topic.

- **Graph model and vocabulary** — §15.2 typed endpoints · §15.3 orientation vocabulary ·
  §15.4 one-producer invariant
- **Data plane and boundaries** — §15.5 boundary policy · §15.18 poll(2) readiness / AsyncFd
  ban · §15.19 hybrid data plane · §15.23 endpoint-keyed wiring · §15.50 teardown ledger
- **Serial ports and device identity** — §15.1 serialport declined / serial2 · §15.6 hold-open
  serial · §15.10 identity + ambiguity · §15.25 resolver, reopen, state file · §15.38 epoch,
  TIOCEXCL release · §15.53 flow control refused at load
- **Write arbitration and sessions** — §15.7 write lock · §15.20 two-lane control plane ·
  §15.23 · §15.38 · §15.39 session boundary is an edge
- **PTY** — §15.14 PTY doctrine · §15.30 macOS contact / §7.2 arms · §15.39
- **Configuration and state** — §15.8 config/state split · §15.9 load/snapshot · §15.25 ·
  §15.42 config-verb reply barrier
- **Wire protocol and legs** — §15.12 loopback + SSH · §15.15 six-clause protocol contract ·
  §15.24 hello, fragmentation, independence
- **Codecs and the extension surface** — §15.11 codec pluggability · §15.22 exec child-pipe
  boundary · §15.26 embed, don't load
- **Control plane and CLI** — §15.13 control socket / Windows out · §15.16 CLI is presentation ·
  §15.35 ports, edge surgery, web editor · §15.43 stdin-EOF leash
- **Web console and map** — §15.28 web console architecture · §15.29 web bind policy · §15.32
  default scrollback · §15.33 map node · §15.35 · §15.37 Playwright
- **Doctor and measurement doctrine** — §15.17 doctor consolidation · §15.21 rig discovery /
  certificate · §15.44 two digests · §15.46 instrument self-testimony · §15.47 portable
  certificate · §15.49 a zero is a claim · §15.51 P14 maximum-rate search · §15.52 handshake
  continuity
- **Harness and validation doctrine** — §15.31 harness as crate · §15.34 review-26 classes
  become rules · §15.36 flake doctrine · §15.37 · §15.48 provider seam, last-hop physics
- **Platform and privilege** — §15.13 · §15.30 · §15.45 privileged replug capability
- **Process, naming, and hygiene** — §15.27 review round: invariants relocate · §15.40 family
  naming · §15.41 capabilities, never consumers
- **Flow control** — §15.52 · §15.53

### 15.1 The serial crate licensing landmine

**Status:** LIVE — dependency rule restated at §13.

The ecosystem-default `serialport` crate is MPL-2.0 and its MIT-badged wrappers keep it in the
tree: DECLINED — STANDS, the whole family. Shipped: `serial2` (BSD-2-Clause OR Apache-2.0), raw
termios via nix/rustix as fallback; `serial2-tokio` was later dropped for lack of an fd accessor
(§15.18's round). Port enumeration is never depended on — the resolver reads `/dev/serial/by-id`
directly (§12).

### 15.2 Edges terminate on endpoints, not nodes

**Status:** LIVE — model restated at §3/§4.

Nodes expose typed endpoints; edges join endpoints, never nodes; channels are endpoints with
identities — because a demultiplexer's outgoing edges each carry a *different* stream. Facing,
locks, binding, and §15.23's wiring all attach to endpoints; this entry is upstream of nearly
everything else in the design.

### 15.3 Orientation: from global rules to local typing, and the target/host anchor

**Status:** LIVE — vocabulary and the schema word bans restated at §3 as a mechanical list.

Orientation is a local per-endpoint property — faces target or faces host, "faces" meaning
looks-toward — anchored on the *system under control* ("hardware-facing"/"user-facing" was
factually wrong for three required configurations); every edge joins one of each. DECLINED —
STANDS: flow-connoting names (source/sink, producer/consumer, input/output) — bidirectional flow
on an oriented edge is the whole point. Bans: the schema uses `address`, never `host`; the words
source/target never appear in the schema.

### 15.4 Fan-out without tee nodes; the one-producer invariant

**Status:** LIVE — rules restated at §4.

Host-facing endpoints accept any number of edges (broadcast hostward, arbitrate targetward);
target-facing endpoints accept exactly one — the diamond's duplicated delivery becomes
unrepresentable, and the write lock got its universal scope in the same stroke (§15.7).
DECLINED — STANDS (deliberate cost): implicit merging of two streams into one consumer; merging
must be an explicit framing node (§14).

### 15.5 Boundary-only policy and the directional asymmetry

**Status:** LIVE — restated at §5; SUPERSEDED-IN-PART by §15.19 (the single-thread *claim* only).

Interior nodes are queue-free and policy-free; all policy lives at the four boundary types
(review-expanded to PTY masters and serial ports, both directions). Hostward is lossy at
boundaries with counters — 3-wire UART makes end-to-end hostward backpressure physically
impossible — while targetward returns Accepted/Busy and backpressures to the origin. The pause
machinery is reused wholesale by arbitration (§15.7) and faulted-and-wait (§15.9) — the design's
main economy.

### 15.6 Idle serial ports: gate reads or read-and-discard

**Status:** LIVE — behavior restated at §7.1.

A configured serial node holds its port open (exclusivity; deterministic DTR) and reads
continuously, discarding with counters when nothing is attached — the kernel drops on overrun
regardless, so not reading merely relocates the loss. Corrected on the record: the expensive
thing about polling is latency, never CPU (re-measured at §15.19). The replay ring became an
opt-in future feature (§14 item 4; since graduated — §15.32 made it default-on).

### 15.7 Write arbitration: exclusive locks, write modes, and the stale-command hazard

**Status:** LIVE — restated at §6 (with §15.20/§15.23; fairness stated once, corrected).

An exclusive write lock per host-facing endpoint gates the pause machinery; reading is never
arbitrated; write modes, atomic `send`, purge defaults, steal/lease/detach-release,
`free-for-all`, and cross-machine lock nesting are §6 contract text now. Documented limits
stand: origins are endpoints, not processes, and remote contenders block instead of failing fast
until lock-forwarding frames ship (§9, §14).

### 15.8 Configuration versus state; operators own the graph

**Status:** LIVE — restated at §3/§11.

A strict configuration/state split enforced by schema: operators own the graph, the environment
owns only state. Configuration operations fail only on structural invalidity; environmental
failure faults nodes without removing them — faulted-and-wait, generalized to every boundary
type. `dump` round-trips by construction.

### 15.9 Load semantics and the persistence gap

**Status:** LIVE — restated at §11.

`load` is accepted only on an empty graph and is structurally atomic; running graphs change only
via incremental verbs; the daemon snapshots configuration after every successful mutation and
startup prefers the state file. DECLINED — STANDS (explicit deferral): configuration diffing
(§14) — full-file edits to a live system mean `--replace` and an accepted outage.

### 15.10 Device identity and hotplug without libudev

**Status:** LIVE — restated at §12, where the three-doors passage is self-contained.

A resolver converts operator input to a canonical identity (`usb:vid:pid:serial:iface`) stored in
configuration, resolved at every open and recheck; libudev is LGPL, excluded from linking; uevent
netlink is a future optimization (§14). REFUTED (review 32 `RES-1`): counting ambiguity over
by-id *entries* — two clones collide on exactly one published link; the rule counts over
*devices*, and binds resolution as well as capture (the three doors, §12). DECLINED — STANDS:
"bind and warn" — resolution answers with a path and nothing else, so the warning has nowhere to
live; an ambiguous identity resolves to neither device and the node stays `waiting`.

### 15.11 Codec pluggability and the envelope unification

**Status:** LIVE — restated at §8; the envelope claim is stated as amended by §15.15.

Compiled-in codecs behind an explicit registry and Cargo features, in workspace crates against
`serial_nexus_codec_api`; attributes are codec-validated opaque tables; the exec codec's child
envelope shares the link codec's frame format — since §15.15, two separately versioned contracts
sharing one v1 implementation; announced channels bind to configured identities, with
`unbound`/`waiting` as state. The cost of no dynamic loading is accepted.

### 15.12 Wire security posture and channel identity

**Status:** LIVE — posture restated at §9; hello mechanics as refined by §15.24.

Loopback-only by default with SSH (including streamlocal forwarding) as the security layer and
`insecure_bind = true` as the named, greppable exception — serial consoles are frequently root
shells, and v1 ships no crypto to get wrong. Operator-chosen node names split from codec-scoped
channel identities (`node/channel` display form); a small custom frame format over yamux, for
the identity/hello/envelope semantics yamux would not carry.

### 15.13 Control plane and the Windows exclusion

**Status:** LIVE — restated at §10, with the socket-path chain computed from the one
implementation in `serial_nexus_rpc::socket` (notes §3.72).

A Unix domain socket whose file permissions are the entire authorization model; hand-rolled
JSON-RPC 2.0 over newline-delimited JSON, batches rejected, notifications powering `subscribe`.
DECLINED — STANDS: Windows, a large redesign accepted as the price of ever changing that; the
exclusion kept the PTY story purely POSIX.

### 15.14 The PTY is not a serial port

**Status:** LIVE — restated at §7.2, with the Darwin divergences measured and cited to committed
artifacts.

Baseline raw + echo-off + EXTPROC at creation, reasserted on last close; client termios
*observed* as state, never propagated to hardware — propagation is deferred behind a
subscribe-plus-RPC experiment path (§14); break and modem signals are control-plane verbs on the
serial node; presence-gated output with counters. The PTY/port semantic gap is *reported*
honestly rather than papered over.

### 15.15 Protocol agnosticism: the framing is a module

**Status:** LIVE — the contract is assembled once at §9, with §15.24's completions.

The design constrains the protocol only through §9's six requirements, with one sanctioned
exception class: aspects that exist to admit some form of protocol. Envelope and wire are two
separately versioned contracts sharing one v1 implementation, so wire evolution never breaks
envelope users. Head-of-line blocking on targetward flow is a documented v1 property, bounded to
human-scale command traffic; per-channel flow control is admissible without design change.

### 15.16 CLI shape is presentation, not contract

**Status:** LIVE — one paragraph at §10.

The daemon constrains the CLI only through the RPC surface: structured JSON out, all rendering
in `serial-nexus-ctl`, CLI shape free to rename, regroup, and compose. Two stability tiers — the
RPC surface deliberate and additive where possible, the CLI shape free. §10 documents semantics,
not spellings; usage examples elsewhere are illustrative spellings of the day.

### 15.17 Capability probing and the no-target hardware doctrine

**Status:** LIVE — restated at §13, in notes §3.74's ordering: JSON is the artifact of record,
Markdown the human view (this entry's original "Markdown (JSON for CI)" emphasis is superseded).

Every probe consolidates into `serial-nexus-doctor` under the no-target doctrine: Tier 1 a
dangling converter; Tier 2 one converter, TX jumpered to RX; Tier 3 two converters cross-wired —
independent clocks, so only Tier 3 can detect baud inaccuracy. Probes that transmit require the
port explicitly named. The daemon's runtime behavior stays independent of probing — the doctor
can be wrong without the data plane being wrong — and release gates reference tiers, never
ownership of a target device.

### 15.18 Readiness for tty descriptors: poll(2), not epoll

**Status:** LIVE — the AsyncFd ban is in §5's tripwire table (AGENTS §4: read this entry before
re-litigating); SUPERSEDED-IN-PART by §15.19 (the "never throughput" claim). P8 later measured
that a bare level-triggered epoll registration on a pty master *agrees* with `poll(2)` — the
starvation recorded here is the async runtime's readiness-guard lifecycle, not `epoll_ctl` in
isolation; that does not reopen the question.

`AsyncFd` spuriously reported a pty master readable while `read(2)` returned EAGAIN and a direct
`poll(2)` reported nothing; the ready future completed synchronously, so the loop never yielded —
starving the current-thread runtime and freezing the control plane. Decision: tty-family
descriptors are driven by non-blocking `poll(2)` plus a short idle sleep; `AsyncFd` is prohibited
for pty masters; socket boundaries keep the runtime's native types, which do not share the quirk
(the round also folded the serial2-tokio drop, slave priming, and holdover-flush amendments into
§13/§7.2/§5). A refactor toward the idiomatic `AsyncFd` pattern will reintroduce a control-plane
freeze; this entry exists so that refactor dies in review.

### 15.19 The benchmark cashed the escape hatch: a hybrid data plane

**Status:** LIVE — the architecture is current truth at §5; the figures below are hedged dev-box
measurements of their era, not re-taken since.

The phase 3 benchmark (plan §4) falsified §15.18's "never throughput" for a cross-process peer on a
current-thread runtime — every cycle pays tokio's roughly one-millisecond timer floor per
kernel-buffer refill, capping hostward throughput near 1 MB/s (measured 1.2 MiB/s serial→log).
Exactly the hatch §15.18 reserved: the two high-rate hostward paths moved to dedicated blocking
threads parked in blocking `poll(2)`; measured then, about 185 MiB/s lossless hostward and idle
about 0.06% per polled fd. What §5 retires is the single-thread *claim*; what it keeps is
single-writer-per-state, per thread, with atomics at the seams.

### 15.20 Waiting verbs: the two-lane control plane

**Status:** LIVE — restated at §6/§10. The serialization invariant is "transitions are critical
sections", never "dispatch is synchronous" — a restatement, not a weakening; quoting the old
phrasing would re-break this model's justification.

Every state transition remains a synchronous critical section on the runtime thread; a verb that
cannot complete registers in an explicit FIFO waiter queue and suspends holding nothing — grant
rules, cancel-safety, the lease generation guard, and the origin/endpoint addressing are §6/§10
contract text. Tripwire minted here: a `RefCell` borrow never crosses an `.await` (§5's table).
Limitation, documented: poll-sampled presence cannot see a close-and-reopen inside one poll
interval, so a successor client can inherit a lock on the detach-release path; the per-open
generation epoch that would close it is deferred (§14).

### 15.21 The rig is a fixture, so the doctor certifies it

**Status:** LIVE — the shipped-certificate paragraph below is exact and normative; dropping it
revives the wider promise (a recorded v14→v15 rewrite hazard). SUPERSEDED-IN-PART by §15.47: how
the certificate certifies off Linux — the portable UART predicate, the tier hoisted out of the
verdict, unmeasurability carried as data — is current truth there.

Physical rigs can themselves be miswired, making a Tier 2/3 failure unattributable, so rig
discovery and characterization are doctor probe P5, opt-in like every TX-emitting probe:
discovery classifies each named port as dangling, loopback, or paired (both directions verified —
half-crossed wiring is reported by name); characterization was specified as data integrity across
a rate ladder, deliberate baud and parity mismatches, break reception, and a modem-line map; the
doctor certifies the rig and stops — end-to-end rig exercises are checklist items whose
precondition is a clean certificate. Amended after review 26: a precondition has to be able to
fail — data-integrity failure is `unsupported`, an uncharacterized item `degraded`, named.

**The shipped certificate, stated exactly** (review 37, 37-TOOL-3, disposition justify; notes
§3.23 — carried as an annotation in v14, primary text since v15, without reviving the wider
promise): per port, custom-baud acceptance, break **assertion** (`set_break(true)`/
`set_break(false)` accepted on a port the doctor holds open alone), error-counter availability,
and a raw modem-line read printed but folded into no verdict; per verified pair, a rate ladder at
9600, 115200 and a nonstandard rate in *both* directions, and one deliberate **baud** mismatch
(115200 transmitting into a 9600 receiver) that must corrupt the pattern *and* raise the
receiver's frame counter. Three items the decision above names are **not** in it: the **parity**
mismatch (`p5_certify_pair` runs `Parity::None` throughout — only baud is mismatched), break
**reception**, and far-side **modem-line signalling** (every modem read happens with the peer port
closed, so it cannot answer what the wire carries). Those three are not lost: they are checklist
items the certificate is the precondition *for*, and the in-tree guard for the break clause is
`p12_serial_exclusivity::a_break_straddled_by_a_replace_leaves_the_line_transmitting`, which needs
a rig the harness can see (`crossover_ports()` — `SNX_CROSSOVER_A`/`_B` on Linux). The
parity-mismatch and break-reception remainder is a plan §18 ledger item.

### 15.22 The exec codec is a boundary, and the empty channel is the multiplexed side

**Status:** LIVE — restated at §7.6/§8.

A child's stdin and stdout are kernel pipes with finite buffers and an independent consumer —
§3's boundary test — and one `select!` coupling the two directions produced a clean mutual
deadlock (phase 5 audit, critical). The exec codec is a child-pipe *boundary pair*: its two
directions are concurrently-polled futures — a blocked write must never starve the read
(tripwire, §5's table). The multiplexed stream travels on the reserved *empty* channel identity,
which configuration validation independently forbids for real channels. Regression guard: the
256 KiB round-trip in the exec crash test; any future subprocess integration inherits both rules.

### 15.23 Endpoint-keyed wiring, held-priority reclaim, and resync scoping

**Status:** LIVE — restated at §5 (wiring), §6 (fairness, once, corrected), §7.5 (resync
scoping).

The runtime wiring is keyed by endpoint address — every host-facing endpoint owns its lock,
fan-out, and one arbitrated targetward channel; new node shapes plug in through declared shape
alone. §6 gains held-priority reclaim, because a `--wait` waiter inheriting a stolen
demultiplexer's `held` lock corrupts framing *by design*: fairness is "FIFO among on-demand
contenders, beneath held reclaim" — a correction to §6 as previously written. Resynchronization
is scoped per user of the one shared frame format: the reference codec resyncs by
length-guidance; the link codec never resyncs. Standalone re-multiplexing instances load and
wait for a driver — deferred (§14).

### 15.24 The leg: hello as a distinct construct, and fragmentation, never drop

**Status:** LIVE — restated at §9.

The hello is a distinct wire construct, not a fifth event kind — it validates magic and version
before any version-specific field, so a mismatch always refuses *as* a mismatch; wire and
envelope versions are independent (cashing §15.15). The phase 6 audit found an oversize chunk
silently dropped, uncounted, at the frame bound's encode error; §9's frame-size clause is
completed — fragmented across consecutive data frames, never rejected, never dropped
(100 001-byte `send` round-trip as regression guard; the shared fragmentation helper is §5's
tripwire). A leg's two directions are independent — exhaustion of one parks that half, never
tears down the wire — and the handshake runs under one overall deadline.

### 15.25 Phase 7: identity by construction, and the state file finds its home

**Status:** LIVE — restated at §11/§12; purge-on-reconnect is the third instance of §6's purge
invariant, charged per round since notes §3.59.

The resolver became a shared, dependency-free `serial_nexus_core` module; a `usb:` identity
resolves only on an exact sysfs match — squatter refusal by construction. Purge-on-reconnect
drains the parked targetward channel with a counter — the one sanctioned targetward drain.
DECLINED — `set-attribute` alone STANDS (§14 item 13); the `connect`/`disconnect` half exited
the register with §15.35 and they are live verbs (§10). Dated record, not a
present-tense claim: through phase 7 the one genuine hardware bug was in the *doctor's* pair
certification, on a path the sim structurally skips — the lesson §16 generalizes; later rig work
found adapter-level behavior of its own (notes §3.70).

### 15.26 Out-of-tree codecs: embed the daemon, don't load plugins

**Status:** LIVE — restated at §8, where the extension surface and both conformance kits are
contract text.

DECLINED — STANDS: runtime plugin loading — no stable Rust ABI, and a `dlopen` surface would
turn every internal type into a compatibility promise. Decision: source-level composition, made
cheap — a `serial_nexus_daemon` library plus a thin binary, the registry a value, the extension
surface exactly two semver'd contracts guarded by the external-consumer template built from the
consumer's position on every push (§8). Amended after review 26 (notes §3.19): the
`unstable_fuzz_api` exception — a parser that accepts untrusted bytes may be exposed through a
module of that name, stability disclaimed, sole sanctioned consumer `fuzz/`, and an item
re-exported there must have a fuzz target driving it. DECLINED — STANDS: extracting each
fuzzable parser into its own crate, rejected in favor of the named-module exception.

### 15.27 The review round: invariants move into the places that can enforce them

**Status:** EXECUTED — rules restated at §5, §6, §11, §12; three declines STAND.

A 27-reviewer review (56 verified findings) hit one defect a *third* time — a targetward framer
skipping an oversize chunk on encode error: fixed in the leg, fixed in the exec node, missed in
the in-process codec — founding the relocation thesis: move every invariant into the layer that
can enforce it, whose instances now live at §5, §6, §11, §12. Declines standing:
bounded-join-and-detach for the stalled PTY writer (it would close a raw fd under a live thread;
the writer observes `stop` inside its poll loop instead, §7.2); waiting-verb cancellation on
request-half EOF (§15.20 behaving correctly); an overflow-faulted log node keeps consuming
(§7.3).

### 15.28 The web console: a tap, a ring, and a token — not a logger

**Status:** LIVE — surfaces restated at §5, §10, §17; bind policy SUPERSEDED-IN-PART by
§15.29.

The web client is a pure RPC client (`serial-nexus-web`; the daemon gains no HTTP); the daemon
gained two general capabilities instead — the tap (§10) and the replay ring (§5). Declined,
recorded so they are not re-proposed: the log-node-based design (a viewer must never mutate the
operator-owned graph nor couple watching to disk recording — recording is an operator decision,
never a viewer's side effect) and the spy-PTY variant.

### 15.29 Bearer tokens are not TLS: the web bind policy, revisited

**Status:** LIVE — restated at §17 (three tiers, cookie naming, credential split, `WEBS-1`).

The API-key analogy is structurally right and operationally incomplete; the gap is exactly TLS —
a bearer token over plaintext HTTP is a secret broadcast to every on-path observer (root shells,
per §9). The token is authentication, loopback or TLS is transport, and on loopback the kernel
is the channel: hence §17's three tiers, the analogy true only at `--tls`, with no plaintext
sanctioned mode short of the loud flag. Deliberately not adopted: rotation, revocation, rate
limiting — one per-session token, network exposure priced accordingly.

### 15.30 macOS contact: the baseline is a contract, the fd is a platform arm

**Status:** LIVE — contract restated at §7.2; refutation history here.

The first hands-on macOS run found the PTY node non-functional (the baseline applied through
the master, which BSD rejects), and both defects contradicted the platform notes' *predicted*
verdicts — predicted platform verdicts are not verified ones. The platform-arm contract and P2's
discriminator (master accepts termios: Linux → `supported`, BSD → `degraded`) live at §7.2.
REFUTED: the first fix assumed Linux HUPs on a never-opened master — the §15.30 trap
reintroduced by the §15.30 fix, corrected by hardware (never-opened-HUP absence is universal;
priming handles it). The no-target fixture leg is Linux-only (§16.7 applied, not violated); the
later last-close flush reuses the momentary-slave-open on *both* platforms, only that fd's
readability staying a platform arm (§7.2).

### 15.31 The validation suite becomes a crate

**Status:** EXECUTED — stub. Fifty-eight bash validation scripts were ported wholesale to
`serial-nexus-itest`, with skip discipline as a library primitive. The four sim-double
properties (subprocess, HUP-tolerant, never busy-waiting, idle-CPU asserted) and the deliberate
decline to move the doubles in-process are stated at plan §3.

### 15.32 Default scrollback, and history that lives with the viewer

**Status:** LIVE — restated at §5, §10, §17 (both retention clauses).

`replay_ring` defaults on (64 KiB) per host-facing endpoint, opt-out `0`; the mirror is a spy
outside the graph and `discarded_unattached` never depends on it (§5); the offset/instance
protocol is at §10 and the OPFS story, with both retention clauses — easy to get wrong in
opposite directions, both errors silent — near verbatim at §17. Trade accepted with eyes open:
the ring's "costs nothing when unset" clause is retired for a stated, bounded,
benchmark-guarded cost.

### 15.33 The map node: console quirks belong in configuration

**Status:** LIVE — restated at §7.8.

Console byte quirks get one `dump`-able home; the vocabulary is picocom's byte mappings plus its
hex-display family, the two ordered lists named by the direction of the bytes they transform,
with flow-relative input/output names rejected on §15.3's standing grounds. Not a codec; the
targetward edge defaults to `held`, steal-to-bypass the sanctioned raw path; zero new wiring
machinery (§15.23).

### 15.34 Review 26: the classes behind the findings become rules

**Status:** EXECUTED — class rules at §11/§17; verification rule and ledger discipline at
plan §3 (AGENTS §5/§9); the v12-audit record here.

Four classes behind 93 findings became rules at their birth sites: structurally checked maxima
(§11); parse one request per frame, forward only an allowlist (§17); remote-fed collections
capped with counted overflow; relocation for the re-derivation class (`drain_to_quiescence`,
`effective_write_mode` computed in the one place both validator and wiring consult, the one
hostward fan-out helper charging no-live-sink inside itself — §5/§6). A refused file that parses
to nothing is structural — a mis-typed `[[nodez]]` under `--replace` was an unannounced teardown
reported as success (§11). Alongside, now at their body homes: `tap.closed`,
one-waiting-verb-per-connection, the codec-mux edge-mode corollary, the two-held refusal,
`spchex` corrected against the upstream oracle, the serial output leg refused, 0600 leg sockets.
The verification rule's two clauses: a verifier gets the finding and the tree, never the report
— and the tree must not move under the verifier (the v12 generation's audit broke the second,
35 of 43 verdicts wrong: a refutation of an already-fixed defect is a different tree). Every disposition,
declines included, lives in a remediation ledger, so a declined item cannot be silently
re-fixed.

### 15.35 The graph opens up: enumeration, edge surgery, and the web editor

**Status:** LIVE — restated at §10, §12, §17.

`ports` never opens (probing toggles DTR); `connect`/`disconnect` left §14 with `load`'s own
critical-section validation, a disconnect of a lock-holding origin releasing and purging (the
§15.27 phantom-holder lesson pre-applied); `set-attribute` alone stays deferred (§14). The
never-mutates web non-goal is superseded — graph editing is daemon-user capability, the token
operator trust, lifecycle verbs off the browser wire (§17). The earlier read-only defense is
recorded, not erased: posture decisions belong to the operator.

### 15.36 The flake session: mechanisms, not mysteries

**Status:** LIVE — doctrine restated at plan §3; the mechanisms and fail-first's origin here.

Five mechanisms, each now doctrine: a protocol client abandoning partially-read frames on
deadline expiry (hence fill-then-commit); sim doubles treating pty `POLLHUP` as terminal or
busy-spinning on it; byte-exactness asserted across boundaries §5 sanctions as lossy; a
presence gate narrower than the evidence in the master (the latch arms on any session evidence,
§15.39); a fuzz loop running a stale hand-kept list under a file-existence gate (enumerate from
the tool; assert execution, not existence). Twice the first proposed guard passed against the
unfixed defect — the recorded origin of fail-first — and three of five adversarial
verifications materially corrected the diagnosis.

### 15.37 The browser half joins CI: Playwright over hand-rolled CDP

**Status:** EXECUTED — scaffold rules at plan §3; the CDP decline STANDS.

A pinned Playwright suite, Chromium only, a subprocess from an itest gate test that self-skips
without node — never a build dependency. DECLINED: a Rust CDP client — §15.36's F1 was a
hand-rolled protocol client desynchronizing under a deadline, and a hand-rolled CDP client is
that risk wearing a bigger protocol.

### 15.38 The browser suite pays for itself: an epoch, and an ioctl on the way out

**Status:** LIVE — epoch at §10/§17; generalized release rule at §7.1; the offset-heuristic
refutation here, marked do-not-reintroduce.

Three defects from the first green-to-red run, one in the browser. REFUTED, do not reintroduce:
inferring an offset-space reset from `from_offset < frontier` — an ordinary reload against a
non-empty ring looks identical, offsets alone cannot answer, and each wrong guess fails
differently (duplicated storage, 19 → 38 → 57 bytes over three reloads, versus a frozen
console); `tap.open`'s `epoch` answers it instead (§10, §17). Below the browser, `load
--replace` opened the replacement device before releasing the outgoing fd — self-EBUSY,
permanent on a pty since `TIOCEXCL` clears only at last close; the node now releases what it
asserted, on every exit path (§7.1). Corollaries: a double
that makes a transient fault permanent is an amplifier worth keeping; the firehose drop-counter
render lag is honest at serial rates — its spec is tagged slow and runs nightly.

### 15.39 A session boundary is an edge, and invariant 1 never said otherwise

**Status:** LIVE (mechanism restated at §6/§7.2; the invariant distinction and forge sites here)
— number frozen by committed `docs/doctor/` artifacts.

Darwin flushes both tty queues at the slave's last close: a session collapsed inside one poll gap
leaves the master byte-identical to no session, and §6's detach-release leaked the write lock
forever (measured 20 of 20). Level state cannot carry an edge; that is the whole content of this
section. `serial_nexus_sys::SessionLatch` (on Darwin a kqueue knote `EVFILT_READ | EV_CLEAR` on the
master, elsewhere inert) folds into the existing `saw_session` latch, not a third disjunct — one
predicate keeps the close block reviewable — polled once per pass, non-blocking, never marking the
pass productive. The readiness invariant is not weakened: the `AsyncFd`/epoll ban (legacy invariant
1 — §15.18; mapping table at the end of §16) answers "is this fd readable", and readiness stays
`poll(2)` only, the `meta_gates` grep unchanged; this asks "did a session boundary happen", which
`poll(2)` cannot answer on this kernel. Two forge sites, both measured: the daemon's own slave
opens post edges indistinguishable from a client's, so `watch` swallows the edge its own
registration posts and the last-close block discards after running — removing either breaks a
*different* test than the one this section fixed. The asymmetry (a bare open→close posts an edge
only on Darwin) is recorded, not levelled; doctor P12 keeps the two mechanisms diffable across
kernels (notes §3.50). REFUTED: the "unfixable" diagnosis — exhaustive over the wrong category
(levels), overturned by two skeptics measuring (§15.34's rule).

### 15.40 One name for the family: the serial-nexus prefix

**Status:** EXECUTED — stub. Family naming and the three-spellings corollary live at plan §2 and
AGENTS §11; retired spellings are described, never quoted (the plan §17 item-2 gate bans them
tree-wide). The renumbering precedent recorded here — this entry's own number moved off a draft
collision because committed artifacts cite §15.39 and §16.13 forbids editing captured output —
is now the front-matter rewrite invariant: an entry cited by immutable evidence is never
renumbered.

### 15.41 Capabilities, never consumers: business context stays out of the documents

**Status:** LIVE — this entry is the ban's one normative home in the design; AGENTS §10 and the
plan each restate it once.

The documents describe capabilities and never assert the existence, count, or nature of external
consumers (a rename-era ADR had promoted private business context to documented fact). The ban,
exactly as gated: consumer-flavored phrasing is genericized to "out-of-tree";
`closed-source`, `closed repo`, and `known repository` have no
legitimate use in this document set and are gate-banned; `downstream` survives only in its
data-flow sense (§3's terminology note, the holdover contract) and `proprietary` only where it
names a general capability, both under review judgment rather than a gate. The meta-gate
(`itest/tests/meta_names.rs`) keys on this statement and the pair's filenames, so both move in
one commit (AGENTS §2). One norm is explicitly ranked: the privacy rule outranks the
frozen-history rule — `docs/historical/` gets scrubbed too, because leaked context is not
decision history — and older violating material is paraphrased, never quoted.

### 15.42 A config verb's reply does not precede the listener it creates

**Status:** LIVE — restated at §11 (the reply barrier).

`load`/`add-node` replied before a `listen` leg's `bind(2)`/`listen(2)` had run, so a caller
dialling the address it had just configured raced the daemon and lost — the reply was a proxy
in time for listener readiness (AGENTS §9). §11 states the barrier: the reply waits for every
created `listen` leg's first bind *attempt* — attempt, not success, a refused bind resolving
the barrier with the fault in node status per §15.8, never in the verb's result — and the
`connect` role gets no barrier, its readiness being the peer's.
DECLINED: a harness retry — `itest/tests/p6_hostility.rs` is unchanged, so those tests are the
defect's own regression coverage. REFUTED (notes §3.38): the backlog hypothesis — 4097 pending
connections answered EAGAIN, never ECONNREFUSED, and the failure rate fell monotonically with
reply-to-connect delay (40.5% at 0 µs to 0% at 5 ms).

### 15.43 A daemon's lifetime can be leashed to its supervisor's, by a pipe

**Status:** LIVE — restated at §10 (the stdin-EOF leash).

A supervisor that dies without unwinding leaves the daemon holding its control socket and every
`TIOCEXCL` device; one leaked daemon cost a whole-gate run its five rig tests. The opt-in
`--exit-on-stdin-eof` (`RunOptions::exit_on_stdin_eof`) leashes the daemon to a pipe: EOF routes
into the daemon's normal shutdown path, no more residue than a `SIGTERM`; the default is off
because under a service manager or `< /dev/null` stdin is at EOF from the first instant.
DECLINED: the platform primitives (`PR_SET_PDEATHSIG`, Linux-only, thread-scoped; kqueue
`NOTE_EXIT`, Darwin-only, `unsafe` outside `serial_nexus_sys`, §16.3) — each executes on one
platform only, AGENTS §9's proxy in space; pipe EOF is POSIX on both kernels. The watch is a
detached `std` thread in `read(2)`, not `tokio::io::stdin`, whose uncancellable task runtime
shutdown waits on. `RunOptions` gains a public field — semver-visible on the entry API (§15.26);
the template's `..Default::default()` keeps it non-breaking.

### 15.44 A fingerprint over questions cannot certify a diff over fields

**Status:** LIVE — doctrine restated at §13 (comparability ladder, era table); the
withdrawn-figures register and the era-close law live here.

`probe_set` digests each probe's `(id, question)` strings; the probe *body* interrogates the
kernel, and the two move independently — measured: one digest across five commits and six
observation sets. The second digest, `build.field_set`, covers the sorted, deduplicated scalar
leaf paths under `.probes[].observations` — equal means exactly the same cells (computed from
what it certifies), unequal means diff the intersection, and neither digest sees a body change
that moves a number without moving a key (a stated limitation, recorded at plan §18's
not-scheduled register); §13 states the full ladder. Declines, both measured, both STAND:
folding observation keys into `probe_set` (some keys *are* measurements, others device
identities — naming two ports adds 19 leaf paths, and one binary emits 72 Linux-only against 22
macOS-only paths; a passive and a rig run of one binary would self-report incomparable —
counterexample frozen by `itest/tests/meta_doctor_artifacts.rs`); a kind-sensitive digest (the
divergent kinds are all measurements). An absent `field_set` is
unknown, never "equal" — both expectation files require it by presence, contrasting P4's
abstaining absence arm: a measurement may abstain, provenance
must be present. `field_set` is a run property, not a binary property; three sequential runs is
the floor for run-varying quantities; the strongest basis is outside both digests — same source
on both sides, a git diff over the doctor's crates empty (notes §3.73). Amendments: `field_set`
sorted (notes §3.72); `--field-set` exits 2 on a zero-observation report (notes §3.74).

**Withdrawn-figures register.** 32/35 — the cross-binary leaf-path pair an earlier commit
message quoted — is WITHDRAWN (notes §3.51): no collapsing of the paths reproduces it; never
re-quote it. The reproducible figures are 65 (`7ead470` → `1a9a8fc`) and 71 (`fa4b12d` →
`1a9a8fc`).

**The era-close law.** An era is the set of captures sharing one `probe_set`; a new probe id —
or a corrected question string — is a new instrument (notes §3.57) and moves the digest
deliberately, closing the era. A closed era's artifacts stay comparable with each other, no
later capture joins them, and P1–P14 are never diffed across an era boundary without the
mismatch stated (notes §3.73). The eras — `a131e1f4b46d6c83` → `94d64d8bbacf1174` (P14) →
`82a8e2198e54626a` (P15) → `e79f5fcd86a2e5f0` (notes §3.73) — are tabulated at §13.

### 15.45 A privileged capability the repository carries, not the machine

**Status:** LIVE — this entry is the contract's home: one contract, two platform arms, one
refusal; the founding-premise measurement at §12, the narrowness tripwire in §5's table.

The §12 identity promises had never met a real hotplug (`p7_replug.rs` re-links a fixture tree
— AGENTS §9's proxy in space), and the real operation, writing `0` then `1` to the USB
`authorized` attribute, needs privilege. DECLINED, STANDS: a udev rule granting `dialout`
write — machine state; a moved checkout silently loses
the capability (do not fuse with `packaging/99-serial-nexus.rules`, which is deployment
configuration, not test capability). DECLINED, STANDS: a capability-conferring test runner —
`CAP_DAC_OVERRIDE` is root-equivalent, ambient across every `fork`/`exec` the tests make, and a
daemon holding it would prove the daemon works as root (AGENTS §9's proxy at the largest scale).

One binary, `serial-nexus-replug`, carries `CAP_DAC_OVERRIDE` on a copy its own `install` verb
places at `.snx-bin/<profile>/` — project-local, gitignored, mode `0700`, blessed by a single
`sudo setcap` the tool prints and never runs. The bounds, by construction:

- **argv only** — no environment variable is read.
- **no `exec` while blessed** — `PR_SET_NO_NEW_PRIVS`-guaranteed; `install` is refused while
  the capability is held.
- **no path parameter** — the device argument is a USB port name (`3-1`) joined to a
  compile-time root; a path-accepting verb would dissolve every bound at once — adding one is an
  amendment to this section, never a patch.
- **the kernel confirms the filesystem** — `fstatfs` must report `SYSFS_MAGIC` before any write.
- **non-hub serial adapters only** — `bDeviceClass 09` refused.

Crash safety is the helper's: no `deauthorize` verb — `cycle` does both writes in one process,
re-authorizes on signals, caps any hold at 30 s; an idempotent `authorize` repair verb exists.
Self-invalidation is the primary bound: any write clears `security.capability`; mode `0700`
before `setcap` is the real, same-user boundary; CI never blesses. Re-blessed twice during
bring-up, the shape shrinks what the blessed binary decides: `hold` deauthorizes, waits for
stdin EOF (§15.43's leash), reauthorizes — the caller samples unprivileged, the blessed binary
containing the two writes and nothing else. `scripts/bless` is the one command (byte-for-byte
staleness, no stamp file); the capability is proven, never assumed. `SNX_REPLUG=required`
reddens the self-skip via the same mechanism as `SNX_CROSSOVER=required`; preflight answers
ready / blocked-on-bless / genuinely-not-ready.

Two arms, one refusal: a platform dispatcher over `replug/src/linux/` (notes §3.65); the macOS
arm (`serial_nexus_sys::usb_macos`, IOUSBLib — notes §3.66) is unprivileged and atomic (measured
40–42 ms outages), `hold` refusing with exit 3 — its whole purpose is a caller-controlled window
— and `--hold-ms` reported as `hold_ms_honoured: false` rather than quietly dropped.
REFUTED (notes §3.54): the pre-registered `devnum` discriminator — `devnum` does not move across
an `authorized` cycle (the owning `usb_device` survives); the guard asserts the tty destroyed
and returned and the node leaving `active` (a first green run was vacuous at a 1 ms hold).

### 15.46 An instrument testifies to its own configuration

**Status:** LIVE — rules restated at §13 (instrument validity); record and refutations here.

The pty probes applied §7.2's baseline through a momentarily opened slave that BSD-family kernels
do not carry to the next open, so off Linux several probes measured a configuration the daemon
never runs — AGENTS §9's proxy in space, inside the instrument. §13 states the three tripwire
rules this bought: self-testimony on the measured object, unconditional and never cfg-gated;
acceptance-is-not-delivery with the probe's own footprint reported (the un-drained re-assert
moved P7's shapes 0→1 and 2→3 on the kernel of record — the probe counting itself); and
discriminator-can-fire / axis-can-vary (P10 walks a drain ladder — `[512, 1, 128, 900]`, 512
first so the committed artifacts' fields keep their meaning; P9's declared `1x2` with its
premise *measured*, not cited to POSIX, since Darwin answers an empty mask differently — notes
§3.65). **Refutation, load-bearing:** P10's pre-repair Darwin hostward 4194304 was never a depth
(`ceiling_hit: true`) and the conclusion it licensed was inverted — Linux 7.0.0-29 recovers
13824–15360 bytes per direction in the cited triple (a later capture reads 15872; the quantity
varies run to run), directions varying independently, so **"Linux is symmetric" is withdrawn**;
Darwin 24.6.0 reads 1024/1022 byte-stable — Linux deeper by ~15×. P6/P7/P13 were repaired;
P8/P9/P12 stay on the fallback, **measured not to need it, each for a different reason** (notes
§3.41; P12 must never get a re-assert — its a/b/c shape contrast *is* the instrument). Repairing
all six at once on single-kernel evidence was **declined** as AGENTS §7's forbidden one-way
decision. The Darwin 1024/1022 asymmetry has a measurement, not a decisive one (plan §18).
Artifacts: `docs/doctor/linux-7.0-2026-08-05b-tier3{,-2,-3}.json` and
`docs/doctor/macos-24.6.0-2026-08-05-1a9a8fc-tier3{,-2,-3}.json`, one binary on both sides.
(Notes §3.34, §3.40, §3.41, §3.44, §3.45, §3.52.)

### 15.47 The certificate goes portable, and unmeasurable becomes data

**Status:** LIVE — restated at §13; vacuity record and measurement basis here.

P5's is-this-a-UART predicate was `TIOCGICOUNT`, Linux-only, so on the old predicate **no
certificate item could ever fail on macOS** — a cleanly wired rig always read `supported`. The
vacuity was real *and bounded*: discovery still ran, so a half-crossed rig would still have
degraded. The repair is a disjunction (`read_modem_bits(fd).is_ok() || read_icounts(fd).is_ok()`)
— a **widening, never a replacement**, because a widening cannot lose a port and a replacement
could (AGENTS §7); measured basis: a Linux pts accepts custom baud, `TIOCEXCL`, and break, and
only `TIOCMGET` discriminates (`ENOTTY` on a pts, answered by both FTDI ports). The tier is
computed by the pure `p5_tier_scope` and printed by the uncertified arm too — **without
reordering the verdict arms**, which would have flipped Darwin back to `supported`; the ordering
is load-bearing and nothing in the type system defends it. `CertFailure.unmeasurable_here`
carries the mechanism, set at exactly the two counter-reading sites, and the excuse can never
widen to `custom_baud`, `break`, `rate_ladder`, or the reopen items, which every kernel measures;
on Linux, `icounter=false` for a pts is a *real* measurement and the mechanism clause must never
appear in a Linux report. `supported` → `degraded` was the honest direction on Darwin: five
certificate items evaluated against zero before, `rate_ladder=true` over the physical wire.
Verdicts-as-pure-functions ("may and must") lives at §13. (Notes §3.42, §3.45 E, §3.49.)

### 15.48 The provider seam: software first, the rig as measured fallback — and the last hop's physics

**Status:** LIVE — seam contract and airtime law restated at plan §3; overturn record and residual
here.

Notes §3.37 declined a hardware arm for `serial_pair()` on one box's evidence; new measurement
**overturned the decline, recorded as such rather than as a silent re-fix** (AGENTS §5): six of
seven call sites pass byte-exact over the real wire on Linux (five of six on Darwin); the seventh
fails structurally and stays on the software provider. The contract — one seam, a pure decision
table, software wins whenever it exists (the two providers sit an order of magnitude apart in wall
clock — notes §3.45), `SNX_SERIAL_PAIR=rig` hard-failing with no rig visible, the provider printed
before a test transmits — lives at plan §3; the printing property held exactly when
`p4_free_for_all` went red on its first Darwin hardware execution, and `harness_contract` names the
provider without being a call site — never counted as hardware evidence. The red taught the last
hop's physics (notes §3.46 → §3.47): **a USB-serial port that is not open does not receive** — the
airtime law and its measurements are plan §3's — a harness-fidelity defect, not a product defect:
the daemon transmitted every byte on both kernels. The same red produced the sim's
`--open-file`/`--ready-file` split — a readiness file must mark the state the hazard is about —
restated at plan §3 rule 16. One residual stays open at exactly the strength the evidence supports:
`p4_free_for_all` fails 12 of 12 on Darwin at the committed deadline, every failing observation
carrying `timed_out: true`, so the licensed sentence is "not recovered within 4× the committed
deadline on a path where a healthy run finishes in 5 s" — a stall or a loss, not separated —
against 20 of 20 passes on Linux, same test, rig, and commit. Mechanism not established; no root
cause claimed; plan §18 carries it. (Notes §3.43, §3.46, §3.47, §3.53.)

### 15.49 A zero is a claim: witnesses, controls, and the wrong field kept

**Status:** LIVE — doctrine restated at §13 (zero-witness); case studies and the standing decline
here.

Three probes reported zeros nothing licensed: P12's idle-edge count, P10's peer-side `FIONREAD`
reading 0 beside a kilobyte the same probe then recovered, and P4's `supported` from a loop that
iterated zero times — not macOS-only: a Linux adapter with no serial number reproduces it, and
both expectation files admitted the shape. The doctrine (witness in the unit of the loop's true
cost, pacing arm at the mechanism's cadence, positive control on the same instrument instance) is
restated at §13. The record: P12's witness is **microseconds** (200 back-to-back passes cost
134–175 µs on Linux 7.0.0-29; an `elapsed_ms` field would print `0` and witness nothing; forcing
the paced pause to zero reds the guard at 33 µs), and without its positive control — a slave
genuinely closed through the same latch — every idle count is `unmeasured`, never `quiet`, the
pure `p12_verdict` refusing `supported`. **Deliberate:** P12 carries observations while `skipped`
on Linux — the inert arm proving itself inert is the negative control the kernel that depends on
the mechanism cannot provide for itself. **Deliberate:** P10 keeps its wrong field, because being
wrong on one kernel *is* the finding (AGENTS §7): `peer_pending_input_bytes` reads 0 beside 1024
recoverable on Darwin in six of six captures across two binaries (notes §3.45's "3 of 3"
corrected in place), classified by `fionread_trust` from a sample taken immediately before the
drain; an empty answer beside nothing recovered is `nothing-to-check`, never `agrees`; P10's
status deliberately does not move. **Refutation:** `pp < recovered` is *not* the fault signature
— Linux answers correctly and still saturates at 4095 (the n_tty read buffer) against ~15 KiB
recoverable. P4's `supported` now states a **non-zero population**; a tree where nothing resolved
is `degraded`, not `skipped`, flipping to `supported` if the deferred IOKit backend ever lands —
and **review 32's RES-2 decline stands untouched**: P4 stays `supported` in a no-udev environment
by design. Both gate files take the new keys by presence, never answer; the gates were run over
ten constructed cases rather than predicted. (Notes §3.48, §3.50.)

### 15.50 The teardown ledger: loss is charged at destruction, or the silence is named

**Status:** LIVE — restated in its two-sided form at §5, reply fields at §11; reproductions here.

`remove-node --cascade` of a saturated interior node answered `purged_bytes: 0` with every
counter at zero — measured 808448 bytes in flight, 23042 accounted — failing §5's visible-loss
promise exactly where a node ceases to exist and `state` can no longer report it. The ledger
(`discarded_at_teardown` charged at destruction, riding the destroying verb's reply; never summed
with `purged_bytes`; conservation asserted as an equality) is stated at §5/§11. The invariant is
**two-sided, and only the pair is the invariant**: *the queue never leaves the node's slot, and
no byte that has left the queue crosses an `.await` uncharged*. `serial` and `leg` adopted the
ledger (notes §3.55), and the adoption's first form satisfied the first side while violating the
second — the drain charged an accumulated local after its own yields, inside the very future
`abort_all` drops, so 8000 accepted targetward bytes died with every counter at zero, this
entry's own Problem signature one layer up, reproduced through the production caller (notes
§3.59); it charges per round now, before it yields. `load --replace` is the **third destroying
verb**: it composed `teardown_with` and dropped the `{torn_down, discarded_at_teardown}` it
computed — measured at 12056 destroyed targetward bytes reported as `{"loaded": 1}`; both fields
ride its reply now, always present, `0` included, on a plain `load` as well (`0` beside
`torn_down: 7` reads "seven nodes went and none owed a byte" — the §15.49 lesson). **Open,
named:** `exec`'s figure is a floor (its internal merge stage is beyond the handle's reach) —
ledgered open work (plan §18); the pty's held `pending` payload is not fixable the same way and
is recorded, not re-filed, in the ledger's closing register. (Notes §3.31, §3.55, §3.59.)

### 15.51 The ceiling is measured, not assumed: P14, the maximum-rate search

**Status:** LIVE — spec of shipped code, restated at §13; the v15 "no code exists until it
executes" clause is retired (notes §3.57, §3.61).

The certificate proves integrity at named rates and deliberately never "how fast can it go", and a
datasheet cannot substitute: achievable rates are a divisor family, and the rig itself was other
than assumed until measured (notes §3.53). P14's contract — opt-in exactly as P5 is, running only
against a discovery-verified pair after baseline integrity passes, restoring and re-proving the
baseline rate on exit — is restated at §13. Its shape is a **ladder climb with bounded refinement**
and an open end, so the probe's own list is never the ceiling (bounded by the rate's 32-bit field
and its §15.34 maximum); plain bisection was **ruled out** because rates are quantized per adapter
and reliability is not guaranteed monotone in the requested rate. The trial policy, stated: a rung
is reliable only if a seeded payload round-trips byte-exact in both directions, three trials each,
inside a bounded deadline, payloads sized for constant airtime (~0.25 s per direction, floored and
capped); phase 1 climbs the fixed body 9600–921600 plus
1000000/1500000/2000000/3000000/4000000/6000000/8000000/12000000 then doubles open-endedly; phase 2
refines with at most four requested midpoints; every rate change is followed by the §15.25 post-set
settle. The answer is a number plus its reason for stopping, never a grade — in the **corrected
semantics** (notes §3.61, whose adversarial pass refuted the probe's own claim that the read-back
reports "what the driver is actually running"): `max_reliable_baud` is **the highest rate that
round-trips when both ends are asked for it**, not necessarily the rate on the wire — an FT232R
rounds a 2.5 Mbaud ask to its nearest divisor while `ftdi_sio` echoes the *requested* number back,
both ends mis-set identically and agreeing; each direction carries `achieved_baud_floor`, timed
from its fastest clean trial; the read-back keeps its job only in the `adapter-refused` arm
(4000000 reads back 9600). `ceiling_kind` separates `corrupt` from `timed_out` per trial (a stall
and a loss are different facts); `platform-refused` names the ask surface (macOS caps the *ask* at
230400 — §15.47's unmeasurable-is-data); `structural-cap` names the instrument's own limit; a
vanished peer is `HungUp` and **degrades**, as does an exhausted search budget — never a ceiling.
`supported` whenever the measurement completes, whatever the number — slow is not broken (P13's
rule that the probe never judges); against the sim's null modem the pure, CI-testable search
reports `skipped(not a UART)`, so the claim never executes where a pts would vacuously satisfy it;
and the number is a floor over the probed set under the stated trial policy, never a promise about
unprobed rates. The new probe id deliberately moved `probe_set` — a new question is a new
instrument — opening a new fingerprint era in `docs/doctor/README.md`. (Notes §3.57, §3.61.)

### 15.52 The rig's handshake wiring is measured, not assumed — and kept out of the daemon

**Status:** LIVE — restated at §13; `SNX_RIG_FLOW` at plan §3; discovery record and the
six-crossing correction here.

Every modem read in §15.21's certificate happens with the peer port closed, so it cannot answer
what the wire carries. A hardware session measured the rig directly with `TIOCMSET`/`TIOCMGET`
and found a **5-wire crossover** — RTS↔CTS cross-wired in both directions, DTR moving no
DSR/DCD/RI (notes §3.53 i) — so hardware flow control was testable here and untested: §15.17's
failure in its quieter form, an absent claim rather than a false one. The continuity measurement
(both directions, both polarities — a line stuck high satisfies a one-polarity test — resting
level restored between trials; the DTR arm as the **in-probe negative control**, which a reader
returning constants and a rig with every line bridged both fail) is restated at §13, **reported,
never judged**: it adds no certificate item and folds into no verdict, because *not wired is a
valid answer* — a 3-wire rig is the design's own stated assumption, and an item degrading honest
rigs would move the verdict on committed artifacts whose rigs nobody re-inspected. **The
boundary:** P5 measures line continuity, port to port, no daemon in the path; §15.21's excluded
checklist item means driving the lines through the daemon's own `set-modem` verb and reading them
back through node state — that lands in the suite. The doctor certifies the rig; it never drives
the daemon through it. Neither promise moves. The new key moves `field_set` and leaves
`probe_set` untouched; gates take it presence-never-answer (plan §3 rule 14) — pinning this rig's
crossing would redden every 3-wire bench — and the `rts-cts` end-to-end tests gate on the
*measured* precondition, `SNX_RIG_FLOW`, the first `required` spelling whose precondition is
measured rather than declared (notes §3.63). **Correction (notes §3.73):** the shipped verdict
was computed from four of the six crossings it prints (B→A was measured against DSR alone); both
missing crossings are now measured, the pre-registered reading holding exactly
(`dtr_b_to_dcd_a=false`, `dtr_b_to_ri_a=false`, verdict unchanged) — and neither digest could see
the change, because the handshake is one string cell: the residual blindness §13 names.

### 15.53 A transport's contract is refused, not degraded: hardware flow control at load

**Status:** LIVE — restated at §7.1 (pre-check) and §11 (pre-create precheck contract); history,
declines, and the falsified claim here.

`serial2` verifies by read-back, so a driver that accepts `rts-cts` and reads the flag back clear
produced a `faulted` node *after `load` had returned success* — measured, not hypothetical: Apple's
`IOSerialFamily` does exactly this on an FT232R (P15, notes §3.65 E) while Linux honours the flag
(`cflag` delta exactly `CRTSCTS`, notes §3.68). The decision — refuse at `load`/`add-node`, before
anything is created, through the one predicate `serial_nexus_sys::honours_rtscts` with its three
states — is stated at §7.1/§11, with §7 as the reason rather than an exception: what §7 forbids is
a kernel difference the operator learns nothing about, which the old behaviour was, and what would
be degraded is the transport's **contract** — an `rts-cts` edge exists because the far end needs
the line held, and running without it loses data silently under exactly the conditions it was
configured to survive. P15 calls the same predicate and requires its answer to match its own hand
read-back (`shipped_predicate_agrees`, its `degraded` arm ranked above the finding itself). Two
paths still reach `faulted` (a `--replace` on a port the running graph holds; an adapter arriving
after load); **both repairs to the refusal are declined with reasons recorded** (notes §3.68 (5a)):
re-checking after teardown means the refusal already destroyed the good graph, and inferring from
the outgoing node's own open is an inference, not a measurement — what is shipped instead is that
the fault is never opaque, those paths consulting the same predicate and reporting the same reading
and both remedies (notes §3.72). `xon-xoff` has no pre-check and no probe — **unmeasured, not
known-good**, named here and filed (plan §18); the harness assertion that a refused `load` created
**nothing** (AGENTS §9's tell) rides with the §7.1 contract. History, kept because each piece
shaped a rule: the first refusal was **dead code on every shipped path** — it read
`Path::new(device).exists()` while `add_node` rewrites `device` to the canonical identity first,
live only for a hand-written literal path, the one shape its own guard exercised (AGENTS §9's
proxy); it resolves through `Resolver::resolve_current_path` now, the same call the node's open
makes (notes §3.68 #1). One doc claim is **falsified by measurement**: "the open it performs is not
an *extra* toggle" is false — counted with `TIOCGICOUNT` (exact; a 0.5 ms poll loop misses the ~0.7
ms pulse and did), a `load` of an `rts-cts` node moves the far CTS 2, 2, 2 times against 0, 0, 1 at
`flow = "none"` and 0, 0, 0 with the pre-check disabled; DTR is inferred, not observed — the rig
leaves it unwired (notes §3.68). And the citation debt: §15.51 carried P15's `question` citation
because this entry did not exist; notes §3.68 filed and declined the string fix, and notes §3.73
**overturned that decline**, deliberately moving `probe_set` `82a8e2198e54626a` →
`e79f5fcd86a2e5f0` and closing that era — a correction only gets dearer as captures accumulate
under a wrong citation.

### 15.54 Park or drain is a property of the pump, not of the edge

**Status:** LIVE — amends §5's wiring invariant (clause 3) and its loss taxonomy, which stated the
park rule as an absolute and carried no row for the counters the exception charges. The behaviour
is unchanged and was always deliberate; what changes is that the design now says what the tree
does.

**The rule.** On a targetward edge that is **not attached**, a pump whose stall is confined to the
endpoint whose edge was removed **parks** — the writers behind it backpressure, exactly as a steal
makes them (§6), and nothing is dropped. A pump whose stall would reach traffic belonging to
*other* endpoints **drains and counts** instead, charging a located pump-side discard (§5's
taxonomy). Which arm a node takes is that node's own policy, and the three shipped instances name
their reasons in place:

- the **interior** nodes — codec, map, pty — park (`nodes/codec.rs`, `nodes/map.rs`): one pump per
  host-facing endpoint, so the stall lands exactly on the writers of the edge that went away;
- the **leg** drains, charging per-channel `discarded_targetward` (`nodes/leg.rs`): its channels
  share one socket, so a parked channel fills its bounded queue and then head-of-line blocks every
  other channel on a cross-machine link (§9). §7.4 already counts data for a channel with no local
  endpoint;
- the **exec** codec drains, charging `mux_discarded_targetward` (`nodes/exec.rs`): the mux route
  runs inside the child's single stdout decode loop, so a parked mux event stalls every *hostward*
  channel event queued behind it — one detached device edge would stop delivery to local consumers
  that have nothing to do with it.

**Why it is not unified.** Only the send-and-charge *tail* is shared (`runtime.rs`'s
`forward_targetward`, SIMP-2); the pump around it is not, precisely because this choice differs.
Sharing the whole pump would force one arm on all three and silently break whichever two it was
not written for — which is why the tail is where exits get forgotten (CODEXEC-2 returned through
one without charging) and the arm is where the policy stays.

**Why it is recorded now.** The rule was stated correctly in the v12 session log, with the
qualifier *"park only where the stall is confined to the endpoint whose edge was removed"*. When
plan §14 promoted the invariant into §5's body the qualifier was dropped, and the design then
stated an absolute that three shipped nodes did not follow — with the counters charging on the
exception having no row in §5's own loss taxonomy either. A reader checking the tree against §5
would have found four counters moving on a healthy graph and no class to file them under. Nothing
in the tree moved; the design is what was wrong (notes §3.75).

### 15.55 The replug helper grants device access, and keeps the bound that made it safe

**Status:** LIVE — amends §15.45, which AGENTS §4 makes a tripwire: *"its device argument
is a kernel-verified sysfs USB port name, never a path. Giving it a verb that accepts a
filesystem path dissolves every one of those bounds at once and is a design amendment, not
a patch."* This is that amendment, and it is written to keep the bound rather than spend it.
**Requested by the repository owner** after the 2026-08-12 rig session (notes §3.79).

**The problem, measured.** A USB re-enumeration destroys the device node. udev builds a
fresh one, `root:dialout 0660`, and every grant made against the old inode — an ACL, a
chown, anything — went with it. So a rig lane that begins with device access loses it the
moment the replug tests run: the 2026-08-12 lane produced eight failures with one cause,
`reopen /dev/ttyUSB0: Permission denied (os error 13)`, six on the tty path and two on the
`usb:` identity. The helper that *caused* the re-enumeration is the only thing in the tree
positioned to undo its side effect.

**The verb.** `grant --port <PORT>` adds `u:<uid>:rw-` to the POSIX access ACL of every tty
the named port owns, and `authorize`, `cycle` and `hold` do it automatically after a
successful reauthorization — because restoring the access the reauthorization destroyed is
part of putting the device back, not a separate favour a caller must remember.

**What is kept, and it is the whole point:**

- **argv still carries a port name, never a path.** The same `validate_port_name` alphabet
  (digits, `-`, `.`, bounded length and depth) and the same four sysfs checks run first. The
  device nodes are then derived *by the helper* from the kernel's own view of that port, so
  no caller-supplied path reaches `open`.
- **The beneficiary is `getuid()`, never argv.** A `--uid` flag on a capability-carrying
  binary would let any caller hand privilege to any account. There is no such flag; the
  helper can only ever grant to whoever ran it.
- **The node is never opened.** Opening a serial port asserts DTR and can reset the board
  behind it (§15.17), so the reference is taken with `O_PATH` — which resolves the inode
  without calling the driver's open — and the `setxattr` is aimed through
  `/proc/self/fd/<n>`.
- **The inode verified is the inode modified.** `O_NOFOLLOW` refuses a symlink and the
  character-device check runs on the *fd*, so there is no name-based window between check
  and use. On a binary holding a capability, that window would be the whole attack.
- **No `exec` while blessed**, and `PR_SET_NO_NEW_PRIVS` still established and read back.

**What it costs, stated plainly.** A second capability. Setting an ACL on a file you do not
own requires `CAP_FOWNER`, so the blessing becomes `cap_dac_override,cap_fowner+ep` and the
blast radius grows from "write `0`/`1` to one sysfs attribute" to "…and add one ACL entry on
a kernel-derived tty node". `REQUIRED_CAPS` is the single source for that set: the command
`install --print-setcap` shows, the command `scripts/bless` runs, and the set `--verify`
checks are all derived from it, so they cannot drift.

**Why an ACL and not `chown`.** `CAP_CHOWN` would also solve it and is a strictly larger
grant — it lets a process give files away as well as take them — and chown would destroy the
node's original ownership. The ACL leaves the node reading `root:dialout 0660` with one
extra `user:<uid>:rw-` line that `getfacl` shows and `setfacl -b` removes. It is also
exactly what the operator would otherwise type by hand, which is the shape this replaces.

**Two residuals, recorded rather than hidden.** The grant does not survive the *next*
re-enumeration either — nothing put on an inode does — so it is re-applied per cycle by
construction rather than being durable; the durable answer remains group membership or a
udev rule, and this exists for the case where neither is available without a re-login. And a
copy blessed before `cap_fowner` joined the set replugs exactly as before and *skips* the
grant with a note, rather than failing: a capability that arrived later must not turn every
previously-working `cycle` red.


## 16. Post-completion review: reliability through simplification

This is the decision record of the post-completion review round: the completed system's
adversarial audits, read for structure. The pattern across the serious findings — the exec pump
deadlock, the leg stale-status wedge, the waiting-node targetward drain, the phantom lock holder,
the stranded waiters — was not carelessness; it was the same four pattern-level rules — concurrent
halves, park-don't-teardown, loss notification, and join-then-transition — being re-derived, per
node, by hand. The items below made those rules structural.

**Status ledger.** §16.1–§16.8 are EXECUTED and audited (plan §9); §16.9 is DECLINED — STANDS — a
load-bearing decline §5 cites where the hybrid is described; §16.10's deferrals stand, superseded
in part by §15.35; §16.11 is EXECUTED (bash retired; plan §5 states the canonical form);
§16.12–§16.14 are later addenda promoted to body — §11, §13, and §10/§13 respectively. The
section closes with the legacy-invariant mapping table, giving the retired numbered invariant
list a resolvable home.

### 16.1 One boundary-supervisor library

**Status:** EXECUTED — the architecture is a fact of the code, cited from §5/§7.

Three of the five worst audit findings were instance-level violations of the same hand-rolled
lifecycle rules; one supervisor abstraction encodes concurrent halves, park-don't-teardown, loss
notification, and join-then-transition once, property-tested once, with serial, exec, and leg
rebased onto it. The pty/log rebase recipe (notes §3.21) is plan §18 item 42.

### 16.2 Make the borrow tripwire unrepresentable

**Status:** EXECUTED — the ban is a live tripwire restated in §5's tripwire table.

Daemon state lives in a closure-only critical-section cell, synchronous by type, and
`std::cell::RefCell` is clippy-banned in **every crate that holds daemon state** — the scope
clause settled by a defect: when the library split moved daemon state into a sibling crate, the
ban silently stopped covering it, because clippy resolves `clippy.toml` upward through
*ancestors* and a sibling is not one. A lint configuration disarms silently when the code it
governs moves, so **the durable half of the rule is a test** that fails when a crate starts
holding daemon state without joining the ban list.

### 16.3 One `serial_nexus_sys` crate

**Status:** EXECUTED — the confinement is a live tripwire restated in §5's tripwire table.

The unsafe-bearing wrappers existed three times (daemon, doctor, sim) and the macOS port had to
gate `TIOCGICOUNT` and `ptsname` per copy — a missed copy is a silent platform break. Every raw
wrapper lives in one crate with the cfg-gating written once; everything else is
`#![forbid(unsafe_code)]`.

### 16.4 The purge invariant, stated once

**Status:** EXECUTED — doc-only; §6's body is the content.

Four scattered purge rules became one invariant with three instances and one principled
exemption.

### 16.5 Harness and CI hardening

**Status:** EXECUTED — the doctrine survives at plan §3; the bash-specific gates retired with
§16.11.

Three audits found bugs in the *tests* — a jq precedence tautology that made the soak
unfalsifiable, bare sleeps, presence-gated feeds. The durable rules: assertion helpers are shared
and self-tested (the tautology became a regression test against the helpers, not a memory), and
the deterministic full sweep runs in CI — a validation suite that only runs on the author's
machine is a reliability claim on the honor system.

### 16.6 State-file durability

**Status:** EXECUTED — the rule is one §11 clause: fsync the temp file and its directory around
the rename.

The failure it removes is a truncated state file after power loss; config mutations are rare, so
the cost is unmeasurable.

### 16.7 Sim-skip implies hardware coverage

**Status:** EXECUTED — doctrine live, restated at §13 and plan §5.

Any behavior the sim cannot exercise either appears on the tiered hardware checklist or is marked
*unverified* in the doctor's own report — untested-because-unsimulatable must be visible, never
silent (the one genuine real-hardware bug lived on the path the sim structurally skips). Amended
by §15.37: browser-reachable behavior left the category (real headless Chromium in CI); the
manual checklist keeps rendering fidelity and real-rig interaction.

### 16.8 RPC error-code registry

**Status:** EXECUTED — restated at §10.

The registry lives once in `serial_nexus_rpc`, the docs table is rendered from it, and a test
asserts every code the daemon can emit is registered — two-way per §16.14.

### 16.9 Full readiness unification

**Status:** DECLINED — STANDS. Recorded so the unification is not re-attempted as a cleanup; §5
cites it.

Moving the remaining cold async poll paths onto blocking threads makes the system less reliable,
not more, for two reasons that must survive together: presence detection cannot use blocking
`poll(2)` (level-triggered `POLLHUP` with no client busy-loops), and targetward PTY reads are
gated on `may_write`, which lives with the lock on the runtime thread — moving them trades a
small tuning mechanism for cross-thread arbitration. The hybrid's boundary rule *is* the simple
thing: hot hostward paths on threads, anything lock-gated or inherently periodic on the runtime
thread.

### 16.10 Standing deferrals, reaffirmed

**Status:** SUPERSEDED-IN-PART by §15.35 — the surviving deferrals live in §14's register.

`connect`/`disconnect` left §14 with §15.35; the head-of-line property remains bounded to
human-scale command traffic, exactly as §9 documents.

### 16.11 Fold the shell scripts into the harness; sim doubles stay subprocesses

**Status:** EXECUTED — the bash validation suite is retired (`scripts/` holds only `bless`,
§15.45); the three tool wrappers' destination record is owed (plan §18 item 29).

The three surviving shell scripts (license gate, external-consumer build, wait helper) are retired
with the suite; where each went is plan §18 item 29's deliverable. The evaluated-and-kept decline
rides with it, restated at plan §3: **sim doubles stay subprocesses, never in-process libraries** —
cross-process scheduling is load-bearing realism, exactly what exposed §15.19's timer-floor bug.

### 16.12 Wire-identifier maxima

**Status:** LIVE — restated as a body invariant at §11.

§15.34's numeric rule generalizes to strings: every identifier that rides the wire or a header
carries a stated maximum length, range-checked structurally — an unbounded name is an allocation
and a payload-starver exactly as an unbounded integer is. `MAX_NAME_LEN` is the first instance;
any future wire-visible identifier inherits the rule.

### 16.13 Doctor reports are committed artifacts

**Status:** LIVE — restated at §13 (provenance rule).

A probe run carries its commit, probe-set fingerprint, and date, lands in `docs/doctor/`, and
kernel claims in prose cite a committed report, never terminal scrollback (AGENTS §7 restates).
Frozen output is never edited, and the rule reaches non-prose surfaces — gate comments, index
columns, and shipped strings are claims too (notes §3.36).

### 16.14 Documents asserted against code

**Status:** LIVE — restated at §10/§13; the harness half at plan §3.

A document stating machine-checkable facts is rendered from or asserted against the code that
owns them — the §16.8 error table two-way, the envelope's golden-vector table backed by the
frozen-vector test, any future constants table following — so editing either side alone is a test
failure, never documentation drift.

### The legacy numbered invariants, mapped

A retired AGENTS.md vintage carried a numbered "Load-bearing invariants — DO NOT REGRESS" list
(1–16), and code, tests, and frozen documentation still cite those numbers ("invariant 1",
"invariant 16 rule (3)"). The list is no longer printed anywhere current, and this generation
deliberately mints **no** new numbered invariants list that could collide with it — §5's tripwire
table is unnumbered for exactly this reason. The legacy numbers resolve here:

| Legacy | Substance | Lives in v16 at |
|---|---|---|
| 1 | No `AsyncFd`/epoll on pty/tty fds | §5 (hybrid, tripwire table); record §15.18 |
| 2 | High-rate hostward paths on dedicated blocking threads | §5 (hybrid); record §15.19 |
| 3 | Fragment targetward, never skip on encode error, count any loss | §5; record §15.27 |
| 4 | All `unsafe` in the one sys crate | §5 (tripwire table); record §16.3 |
| 5 | `RefCell` ban / critical-section cell, per holding crate | §5 (tripwire table); record §16.2 |
| 6 | MSRV is a two-way constraint | plan §2 |
| 7 | Configuration/state split; structural-only config failure | §3, §11; record §15.8 |
| 8 | Arbitration defaults `exclusive`; `send` self-acquires | §6 |
| 9 | Ring is bulk-copy, a spy outside the graph; `discarded_unattached` independent | §5; record §15.32 |
| 10 | `tap.data` offsets are the delivered-bytes space; splice exactness; `instance` nonce | §10; record §15.32/§15.38; plan §11.8 |
| 11 | Web bridge screens a parsed value against an allowlist; admission bounded by eviction | §17; record §15.34/§15.35 |
| 12 | Write-mode promotion has exactly one implementation (`effective_write_mode`) | §6/§7.8; notes §3.17 |
| 13 | Numeric fields range-validated structurally, before anything is created | §11; record §15.34/§16.12 |
| 14 | Edge surgery mutates shared wiring, never restarts a node; three endpoint states | §5 (wiring invariant); record §15.35 |
| 15 | Every tty assertion is owned by the port and given back at most once, in order | §7.1 (ordered release); record §15.38 |
| 16 | A held pty payload is registered on the endpoint, never crossing a session boundary; rule (3): lifecycle observation never depends on the data slot | §7.2 (latch and hold clauses), §6 (detach-release); record §15.36/§15.39 |
