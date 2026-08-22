# serial_nexus — Design Document

**Status (2026-08-12):** Implemented and validated through the 0.3.0 release mark, including a real
Tier-3 hardware rig (the tier ladder is §13's) on both kernels, a Linux replug lane driven by the
repository-carried privileged helper (§15.45, amended by §15.55), and the cross-kernel doctor
campaigns whose committed artifacts under `docs/doctor/` back every kernel claim below. §1–§14 and
§17 are normative for the system as built and carry **no design-ahead-of-tree surface at all**.
The last one was the `actual_baud` read-back in a serial node's state (§7.1 clause 7, decided at
§15.58), written here before the tree moved — the amend-first order AGENTS §5 requires — and
**built 2026-08-15 as plan §18 item 41**, whose entry records in as many words that "the two now
agree completely". §7.1 clause 7 says the same at its own site. Its predecessor went the same way:
the pattern wait (§10, §15.56) was specified one generation back and plan §18 item 47 built it the
same day, so §10's pattern-wait subsection is system like the rest. **This paragraph claimed the
`actual_baud` surface was still ahead of the tree for six days after item 41 closed it**, while the
clause it named said the opposite — the class §15.72 collects, filed as plan §18 item 108.
§15–§16 are the decision record.
Measured figures — suite
counts, gate scopes, wall-clock costs — live in exactly one place, the plan's Status table, and are
quotable only with the scope recorded there; this document deliberately carries none. What remains
open is what plan §18 enumerates, and nothing else.

This v17 generation was produced under rewrite invariant 2 below: the v16 text plus intended
changes only, enumerated in the notes' v17 generation entry — the pattern-wait capability
specified at §10 with its decision record at §15.56 and its construction filed as plan §18
item 47; the notes §3.76–§3.81 record folded at its sources (the two-capability helper in §5's
tripwire table, §15.45's amendment mark and §12's, §15.52's re-measured bench), with three
catch-up folds from earlier session records (P5's fourth discovery arm, §15.46's two refutation
chains, the sim verdict's `overshoot` fold); falsified or unreconcilable figures repaired where they stood (§15.51's `platform-refused`
exemplar, §15.48's wall-clock citation, §15.44's register additions); and the clarifications the
implementation record showed were needed (§7.1's arbitration pointer and disconnect-latency
sentence, §13's temporal self-testimony and probe-restore rules, §5's single-sink caveat, the
sixth vacuity shape, §10's protocol-conformance clauses). Nothing recorded as declined or
refuted has been silently reversed.

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
- `sim/` is the subprocess test double, `doctor/` the kernel-capability prober, `devprep/` the
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
   proven on the wire, not only in the model: over the crossover rig, a `flow_control = "none"`
   transmitter delivers through a CTS stop and an `rts-cts` one never does (notes §3.63).
2. Whether the driver *honours* a configured mode is measured, never assumed, by the one shared
   predicate `sys::honours_flow_control`, which takes the mode as a parameter and answers for
   **both**; a driver that accepts the request and silently drops it is refused at
   `load`/`add-node` (§15.53, extended to the software mode at §15.61). The per-mode measurement
   status and the kernels of record are §7.1's contract (clause 7). *(This clause named
   `honours_rtscts` and called the `xon-xoff` half "a measurement debt (plan §18 item 14)" until
   2026-08-14. Both halves of that were overtaken on 2026-08-13: the predicate was renamed when it
   was generalized, and item 14's debt was paid — a dropping driver was found, so §15.61 extended
   the refusal and clause 7 now records both modes as measured on both kernels. Notes §3.101,
   plan §18 item 72.)*
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
   designed behavior, pinned by test as the SUM of the targetward counters freezing while the
   hostward `delivered_hostward` counters keep advancing (`itest/tests/p6_head_of_line.rs`)
   — *(this clause read "hostward checksums" until 2026-08-12; that test reads a monotonic byte
   counter out of `state` and contains no checksum, so the sentence named an instrument the
   named file does not use)* — the sum deliberately,
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
(plan §3). One reconciliation caveat rides with the fingerprint: `delivered + discarded ==
streamed` is a **single-sink** property — with several consumers attached, one chunk is
legitimately both delivered (to a live consumer) and discarded (for a full one), because the two
counters measure different consumers — so fan-out accounting is never reconciled by that
subtraction. And targetward *delivery* loss has no counter in steady state, by construction:
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
   **Every kind that has a targetward queue of this shape reports a total, `exec`'s included**
   (plan §18 item 21, executed 2026-08-12; notes §3.86). `exec` was the one floor, and the shape is worth keeping because
   it is what the item closed: the ledger watched only the host-facing per-channel queues, so a
   chunk that had moved into the node's *internal* merged queue was beyond the handle's reach
   and a torn-down `exec` could destroy more than it reported, never less. That merge stage is
   watched now, through the same `TargetwardInbox` the `serial`/`leg` adoption built — the
   invariant holding one stage further in than the fix that named it reached (notes §3.31,
   §3.55). Only its targetward half is charged: the merged queue carries both directions at
   once, and an item riding the reserved multiplexed channel identity is the raw hostward
   device stream on its way into the child, so charging it here would report a hostward loss
   under a targetward name — the same exclusion the leg's per-channel relay is held to, one
   node kind over (§7.6 clause 8, §8 clause 14).
   **What remains is a boundary, not a floor:** what has left the daemon is delivery — bytes
   already inside a child's stdin pipe, exactly like bytes written to a device fd, are outside
   every kind's figure — and the pty's held `pending` payload is not fixable the same way and
   is recorded, not re-filed, in the ledger's closing register. Both are stated here rather
   than left to be discovered by someone diffing the counter against the conservation sum.
   **The headline's qualifier is load-bearing, because the code carries it too:** `pty` and `log`
   answer a structural `0` — the log is target-facing and the pty's undelivered payload is a held
   `pending` slot inside its reader's own stack frame, reachable from nowhere this counter can
   go — so those two zeros are *the absence of a queue*, never a measured total of one. Reading
   the headline over all seven kinds is the over-read this sentence exists to stop; it said
   "Every kind's figure is a total" flat until 2026-08-21, which is stronger than
   `Node::discarded_at_teardown`, whose `pty`/`log` arm returns `0` with the reason written
   beside it (plan §18 item 108, §15.72).

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
| Privileged helper | The privileged helper is narrow by construction (§15.45, amended by §15.55): **two** capabilities — `cap_dac_override` for the sysfs write, `cap_fowner` for the ACL grant — argv-only, no environment read, no `exec` while blessed, and its device argument is a kernel-verified sysfs USB port name, never a path. A verb that accepts a filesystem path dissolves every one of those bounds at once, and adding a capability likewise widens the blast radius — each is a design amendment, not a patch; the commands shown, run, and verified all derive from `REQUIRED_CAPS` (§15.55). | §15.45, §15.55 (§12) |
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
   for the lock at all. Taps (§10) are its dynamic form: connection-scoped, read-only attachments
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
modem-line readings — which are `TIOCMGET`'s answer and therefore the *driver's*, not the wire's:
on `cdc_acm` the CTS bit is synthesised and reads asserted unconditionally, even with the peer
closed, because the CDC `SERIAL_STATE` notification has no CTS field to carry it (§15.62). Reported,
never judged; no verdict in this tree moves on a modem line. Write arbitration is deliberately not
restated here: acquisition, idle
release, and a peer disconnect releasing an implicitly-held floor are §6's Write modes (clause 2),
and thirteen code sites once cited this section for that rule — one of them quoting a sentence
this section has never contained (notes §3.75).

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
   reappears. A different adapter squatting on the old path is not adopted (§12). The poll
   paces *reappearance* only — disconnect is never polled for: a destroyed tty fails the
   in-flight read at once, so a real unplug moves the node off `active` in **2–3 ms**,
   measured 3 runs on the replug lane against a 200 ms `READ_POLL_TIMEOUT_MS` re-arm — the
   figure, rather than a ratio, because the two readings a ratio invites fusing are
   different observables: notes §3.54 measures *this* one (the node leaving `active`),
   while the 1.2–2.0 ms elsewhere in the record is the `poll` revents readback after the
   sysfs write. Nor is either the 200 µs–5 ms cadence AGENTS §6 quotes: that is the *pty*
   node's active-to-idle backoff, a third node and a third budget.
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
7. **The rate the port actually got is reported beside the rate that was asked for.** A serial
   node's state carries the termios read-back's baud alongside the configured one. Reporting
   only: no verdict, no fault, no refusal — a driver quantizing to what its clock divisor can
   express is ordinary, and only the operator knows which margin their device tolerates
   (§15.58, which records why this is *not* §15.53's refusal). Where a platform cannot report
   the rate back, the field says so rather than echoing the request: an unknown rendered as an
   answer is the shape §12's `has_identity_source` exists to prevent, and echoing the ask would
   make the field agree with itself everywhere and assert nothing. It is a read-back and not a
   wire measurement — P14's `achieved_baud_floor` is that, and it needs a cross-wired peer.
   *Constructed 2026-08-15 (plan §18 item 41; notes §3.107) — this clause is settled system, and
   with it the design carries **no** surface specified ahead of the tree.* **One correction the
   construction forced, recorded here because the clause's motivating example was wrong:** the
   4 Mbaud ask does **not** reach an `active` node running at 9600. `serial2` verifies its own
   `set_configuration` by read-back within ±2.5 %, so an FT232R clamping to 9600 fails that check
   and the **open fails** — the node is `faulted`, and `actual_baud` is `null` because there is no
   port to read. Measured on the rig: `status="faulted" baud=4000000 actual_baud=null`. So on this
   platform the field earns its keep as *the `null` that refuses to claim a rate*, plus the truthful
   answer wherever a driver reports a quantized one; the wire's realized rate stays P14's
   `achieved_baud_floor`.
   **Scoped 2026-08-16 (§15.62): the example above is `ftdi_sio`'s, and the field's usefulness is
   the driver's to grant.** `cdc_acm` accepted every rate P14 asked it for and reported each ask
   straight back — measured `status="active" baud=4000000 actual_baud=4000000`, over 15 of the
   ladder's 16 body rungs plus four refinements on that pair, without one `adapter-refused`
   outcome. (Every rate *tried*, not every rate the field can spell: the field spells `1 ..= u32::MAX`
   and the highest rate ever asked here was 8 Mbaud.) There
   `actual_baud` is exactly the echo this clause distinguishes itself from, so it carries no
   information and the suite's guard **skips on the measured reading** rather than failing: the
   property is unmeasurable on that transport, not violated by it.

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
   (§11's pre-create precheck contract), and **nothing in the config is created** — §11's
   structural atomicity, which this precheck sits inside. *(This clause read "and every other node
   in the config still loads" until 2026-08-12. That is the one thing a pre-create precheck cannot
   do: it returns `Err` out of `load`, so a five-node config with one accept-then-drop port creates
   zero nodes, not four — §11's "the entire file is validated before anything is created; a
   structural error creates nothing", and §15.53's own harness assertion that a refused `load`
   created nothing. The true property the sentence was reaching for is its neighbour: because the
   check runs before the teardown, a refused `load --replace` leaves the **already-running** graph
   intact. The false half had also reached an operator-facing string in the doctor's P15 verdict,
   repaired in the same change.)* The
   refusal is structural and carries `node`, `device`, `resolved_path`, `requested_flow_control`,
   and `honoured_on_readback` as data, plus two remedies: `flow_control = "none"` for this
   port, or an adapter whose driver implements the mode. **The clause said "`flow_control =
   "none"` (or `xon-xoff`)" and "implements RTS/CTS" until 2026-08-13, and the parenthetical was
   wrong in the worst place:** the driver that gives this refusal its founding measurement drops
   `IXON`/`IXOFF` too (§15.61), so the advice sent an operator from a structural refusal into
   exactly the late fault the refusal exists to prevent. **Since that measurement the clause
   applies to `xon-xoff` as well** — same position, same structural atomicity, same three-way
   predicate — and the refusal names the mode it is about.
2. **One predicate, because two callers must not be able to disagree**:
   `serial_nexus_sys::honours_flow_control` is the only implementation, **for both modes** — it
   takes the mode as a parameter rather than being copied per mode, since the three-way
   classification is already a pure function of two booleans and only *which flag in which
   termios word* differs (§15.61; it was `honours_rtscts` and answered for one mode until
   2026-08-13). The daemon's pre-check
   consults it, the harness branches on it, and doctor P15 calls it and requires its answer to
   match the read-back P15 takes by hand, reporting `shipped_predicate_agrees` with its own
   `degraded` arm ranked above the finding itself — a report that calls a port fine while `load`
   refuses it is worse than either verdict alone (§13). **Four states and only one refuses:**
   the predicate answers `Ok(FlowOutcome::{Honoured, Refused, AcceptedThenDropped})` or `Err`,
   only `AcceptedThenDropped` refuses the config, `Err` is *unmeasured* and never a refusal, and
   an absent device never reaches the check at all — unplugged hardware still just waits (§12).
   *(This clause enumerated a two-valued `Ok(false)`-refuses predicate until 2026-08-12, and that
   is exactly how clause 7's contract went unimplemented for a generation: **the design stated
   both shapes and only one of them was code.** The shipped predicate discarded the `tcsetattr`
   status and answered on the read-back alone, so an honest refusal was indistinguishable from
   accept-then-drop and was refused at load, while P15 — which does record the set status — called
   the same port `supported`. The tree moved to clause 7, which is the contract; this clause moved
   to match the tree it now describes.)*
   *(Annotated 2026-08-21 (plan §18 item 73, notes §3.146): **the cross-check this clause
   requires covered `rts-cts` alone until this date.** P15's software cell was a second
   hand-rolled read-back with nothing between it and the shipped predicate — the
   two-callers-that-can-disagree shape this clause's own first sentence forbids — tolerable
   only while plan §18 item 14's decline stood and
   no `load` consulted the software answer, and a defect from §15.61 onward, which made an
   accept-then-drop software reading refuse an operator's config. The
   `software_flow_control.shipped_predicate_agrees` cell now carries the comparison for the
   second mode, on both kernels' expectation files.
   **Which arm it compares is the whole value of the field:** the by-hand side is classified from
   the probe's *whole*-`c_iflag` comparison — `serial2`'s own, and the one that decides whether the
   node's open faults — never from the `ixon`/`ixoff` reading that mirrors `honours_flow_control`'s
   `contains(IXON | IXOFF)` subset test. A field built from the mirroring reading would agree **by
   construction**, and would report `true` on precisely the port where the two implementations
   part: item 56 one mode over, and worse than no field at all, because it reads as evidence.
   Measured on the FT232R pair, both ports `shipped_predicate_agrees: true` against `c_iflag`
   `0x5` → `0x1405` (`docs/doctor/linux-7.0-2026-08-21-800915b-dirty-tier3.json`, `probe_set`
   `4317ea5ac187f506`, `field_set` `f18630922c4eecc7`); and separated on the same bench by a plant
   that simulates the driver rather than the conclusion — a third `c_iflag` bit inserted into the
   word fed to `iflag_matches_request`, leaving the flag cells and the predicate answering honestly
   — under which both ports read `software_flow_control.shipped_predicate_agrees: false` while the
   top-level hardware cross-check read **`true`** and saw nothing. **A bound the field cannot
   exceed:** it sees a drift in the *answer*, never in the *request*, since the probe compares its
   read-back against its own `want` (notes §3.146). A probe change and not a contract change: no
   verdict, no refusal and no predicate moved, P15's `question` is unchanged, and §13's era law
   clause 4 covers the `field_set` move it costs — **no era closes**.)*
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
   notes §3.68). A board that auto-resets on DTR takes one extra reset per `load` of a node that
   asked for **either** handshaking mode; removing it would mean asking from inside the node's own
   open, past where "nothing is created" holds — a filed design question, not a patch. *(This
   clause read "an `rts-cts` node" until 2026-08-14. §15.61 states in as many words that "the
   DTR-toggle cost §7.1 clause 6 states now applies to `xon-xoff` nodes too" — the clause it
   names simply did not move with it, as neither did the same bound in `precheck_flow_control`'s
   own doc; notes §3.101, plan §18 item 72. The asymmetry worth knowing is that the `xon-xoff`
   arm writes `c_cflag` as well as `c_iflag`, clearing `CRTSCTS` to mirror `serial2`, where the
   `rts-cts` arm writes `c_cflag` alone — both restored before the close that drops the lines.)*
7. Per-mode measurement status. **Both modes are now measured on both kernels, and both are
   refused where the driver drops them (§15.61, 2026-08-13).** `xon-xoff`'s Darwin arm is the
   FT232R on `IOSerialFamily` accepting `IXON|IXOFF` and reading `c_iflag` back `0x0` → `0x0` —
   a delta of nothing, `tcsetattr_ok: true`, 6 of 6 (the `b346188` macOS triple) — against
   `ftdi_sio` honouring it `0x5` → `0x1405` on the same two adapters. So the sentence this
   clause carried until then, *"that mode is unmeasured rather than known-good"*, is discharged
   by measurement rather than by argument, and the refusal follows §15.53's reasoning unchanged:
   `serial2` verifies `c_iflag` by read-back exactly as it verifies `c_cflag`, so the node would
   fail its own open with the bare `failed to apply some or all settings`. The discrimination is
   proven rather than assumed — a Darwin **pts** honours the same request (`0x2b02` → `0x2f02`)
   and is not refused. **The superseded per-mode status follows, quoted rather than corrected —
   and exactly one of its two halves is superseded.** The block's **`xon-xoff` sentence** is the
   stale half; the annotation printed under the block shows it was false in *both* of its claims.
   The block's **`rts-cts` sentence is still accurate**, and both of its citations still read as
   quoted: the `7cf0338` Linux triple reads `cflag` `0x10021cb2` → `0x90021cb2`, a delta of
   exactly `0x80000000` (`CRTSCTS`), on both ports of all three captures, and the `acb5162`
   macOS triple reads `0x4b00` → `0x4b00` with `silently_dropped: true` on both. That split is
   why the whole block kept being read as live — an accurate half carrying citations the live
   text above does not restate. **This preamble said "nothing inside the quoted block below is
   current" and then contradicted itself in the next sentence**, from 2026-08-21 until it was
   repaired the same day: a blanket disclaimer over a block whose halves differ makes them
   indistinguishable again from the other direction, which is the failure it was written to fix
   (plan §18 item 108, §15.72).

   > `rts-cts` is measured on both kernels — Linux honours the flag
   > (`cflag` delta exactly `CRTSCTS`; the `7cf0338` Linux triple — a committed doctor artifact,
   > the citation convention is §13's — 2026-08-05, and the Linux 6.18
   > field report at `3e23c52` agree) and Darwin's `IOSerialFamily` accepts-then-drops it on the
   > FT232R rig (`silently_dropped: true` on both ports; the `acb5162` macOS triple, 2026-08-05)
   > — so the refusal arm and the honour arm have each executed on real hardware. A driver that
   > *refuses* the flag outright is honest and is not refused here; only accept-then-drop is the
   > defect (§15.53). `xon-xoff` has no pre-check and no probe, and `serial2` verifies `c_iflag`
   > by read-back too, so a driver silently dropping `IXON`/`IXOFF` would fault the same late way:
   > that mode is **unmeasured rather than known-good**, named here and carried as open work
   > (plan §18 item 14).

   **Annotated 2026-08-21 (plan §18 item 113): the quoted `xon-xoff` sentence is false in
   both of its claims, and had been for eight days.** *There is a pre-check*:
   `flow_precheck_target` maps `FlowControl::XonXoff` to `serial_nexus_sys::FlowMode::XonXoff`,
   and the daemon's load path runs the result through
   `flow_precheck_refuses(honours_flow_control(&path, mode).ok())` — the same call the
   `rts-cts` mode takes, with `.ok()` as clause 5's sanctioned *unmeasured* collapse.
   `honours_flow_control`'s `XonXoff` arm sets `IXON|IXOFF` and clears `CRTSCTS`, and its
   read-back requires **both** input flags, because `serial2` compares the whole `c_iflag`
   word. *There is a probe*: P15 carries a `software_flow_control` observation block, landed at
   plan §18 item 14 — **executed** 2026-08-13 (notes §3.89), not open — reading `c_iflag`
   `0x5` → `0x1405` on `ftdi_sio`, both ports
   (`docs/doctor/linux-7.0-2026-08-21-25d8ecd-tier3.json`, `25d8ecd39aed`, `probe_set`
   `4317ea5ac187f506`, 2026-08-21), against `0x0` → `0x0` with `tcsetattr_ok: true` on Darwin's
   `IOSerialFamily` (`docs/doctor/macos-24.6.0-2026-08-13-b346188-tier3.json`, `b3461886e27a`,
   the same `probe_set`, 2026-08-13). And the mode *is* refused where the driver drops it, per
   §15.61. **The clause therefore disagreed with itself, twice over** — this clause's own live
   text above and clause 6's parenthetical some twenty lines up each said all three, and §15.61
   says it a third time — and the half a reader reaches last is the half that was wrong. **How it was
   found is the transferable part:** not by reading §7.1, which two passes had read closely,
   but by a scan costing an unrelated gate, which had no idea which sentence was meant to be
   live and so read them all.
8. **The pre-check decides whether the driver *accepts* the setting; it never decides whether the
   wire *honours* it.** A port that reads `Honoured` may still be inert, and that is measured
   rather than feared: on §15.62's CDC-ACM bench `CRTSCTS` was accepted **and persisted in the
   `termios` read-back** — `c_cflag` gaining exactly `0x80000000` on both ports of all four
   captures, which is the invariant; the words themselves differ per port and per run
   (`0x10021cb2` → `0x90021cb2` on `ttyACM0`, `0x100218b2` → `0x900218b2` on `ttyACM1` in
   `docs/doctor/linux-7.0-2026-08-17-a7e6070-tier3.json`) — while a 2×2 control (peer RTS low/high
   × `CRTSCTS` on/off, peer never reading) wrote 44672 bytes in every one of the four cells,
   spread 0. The predicate of clause 2 answers about
   acceptance, so `Honoured` licenses the `load` and nothing further; the operator can hold a port
   that reports flow control and does not perform it. **`load` and `add-node` are unchanged and no
   new refusal is added** (decided 2026-08-21 — §15.73, plan §18 item 85), and the reason is
   structural rather than a tolerance: separating *honoured* from *inert* needs a peer, a transfer
   and a stall, while a pre-check has one port and one `tcsetattr` at a position §11 puts *before
   anything is created*. It is the wrong instrument for the question, not a lax one. **This clause
   adds no bound to clause 2 and states the one clause 2 enumerated**: four read-back answers, of
   which one refuses. It is here because the shipped code and the ledger both cite §7.1 for the
   sentence, and a citation whose target has to be inferred is the shape §15.72 catalogues. **The
   instrument that can ask it is the doctor**, because it holds both ends of a cable P5 has
   certified: P15 carries a `wire_flow_control` reading per direction, emitted only where P5
   measured an RTS/CTS crossing **both ways** on that pair, and it is **reported, never judged** —
   no pre-check consults it, no verdict moves on it, and `flow_control = "rts-cts"` behaves exactly
   as clauses 1–5 say whatever the cell reports. What such a reading licenses is one port, one
   driver, one peer, at its stated rate and payload: an `inert` reading is that port's answer, and
   a `gated` one clears that port rather than the driver family it belongs to.

### 7.2 PTY node

One endpoint, faces target. (Orientation at this seat, stated once because an instrument
inverted it consistently enough to ship: the node holds the *master* and its client the *slave*,
so bytes a client writes — slave→master — are the node's targetward ingest, and slave→master
fill measures targetward depth; notes §3.32.) Configuration: symlink `path` (required); `owner`/`mode` (default:
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
12. The last-close block also drains the master. **Its headline reason read "so the poll loop
    cannot spin on a hung-up pty" until 2026-08-21, and that reason did not survive the
    measurement thirty lines below** (plan §18 item 108, §15.72): the spin is a property
    *neither* kernel exhibits, so the drain's two live legs are the collapsed-session write-lock
    leak, which both kernels share, and P6's re-arm, which is Linux's alone — and those are the
    reasons this clause now leads with. The spin was never observable on the kernel of record in
    the first place: P6 measures `pollin_passes: 0` with `read_outcomes {EIO: 64}` on a hung-up
    master there, against Darwin's 64 of 64 `POLLIN|POLLHUP` passes. That is exactly what made
    the old regression guard the tree's sharpest known **proxy-in-space** — it ran on Linux and
    self-skipped everywhere else, so it asserted the property only on the kernel where P6 says
    the property cannot fail, and stayed silent on the platform family the hazard was believed to
    live on. *(This sentence read "it self-skipped off Linux, the one platform family where the
    hazard exists" until 2026-08-21. Read left to right the appositive attaches to **Linux**,
    which inverts both halves at once and contradicts the refutation three lines down; §15.72.)*
    **That gate is gone** (plan §18 item 12, executed 2026-08-13; notes §3.96):
    `serial_nexus_sys::process_cpu_nanos` is the portable CPU source the tree had said did not
    exist, Darwin answering with `proc_pid_rusage` where Linux reads `/proc/<pid>/stat`, so
    `a_bare_hangup_leaves_the_daemon_cpu_bounded` is a bare `#[test]` that runs on both kernels
    against the same 10 % ceiling, converted rather than re-chosen. **The two kernels do not
    read alike under it, and the ceiling is neither one's figure.** Darwin spends
    **1.7–1.9 %** of a core over the 2 s window — the band plan §18 item 12 scopes to Darwin
    in as many words, and notes §3.96 with it — while the Linux cost recorded at the guard's
    own `MAX_CPU_NANOS` is **~10 ms in the same window, ≈0.5 %**, three to four times lower.
    The shared 10 % ceiling is therefore ~20× the observed cost on the platform of record and
    ~5× it on Darwin: a wall against a handler re-running `tcsetattr` on every poll, never a
    tight bound on either kernel's idle cost, and quoting one kernel's band as both is the
    scope error §13's citation rule exists to stop.
    **The causal claim this clause made was then measured on the platform it named, and it did
    not hold.** The claim was that a widened last-close predicate or a deleted latch drain
    "would burn a core — and release operator-held write locks — on macOS with the suite green".
    Planted on Darwin against a rebuilt binary, the ungated `|| closed` arm reads
    **1.81/1.88/1.81 %** against an unplanted **1.87/1.88/1.81 %** — identical bands, the plant
    moving nothing, and the same null Linux had already given. So the spin is a property neither
    kernel exhibits rather than a Darwin hazard the guard was blind to: the reader's backoff is
    not defeated by the handler re-firing, the extra work per pass being small and the cadence
    still relaxing to `IDLE_POLL`. **Recorded as refuted rather than quietly dropped**
    (AGENTS §9), and the refutation moves the drain's justification to where measurement puts
    it — what bars the ungated arm is the collapsed-session **write-lock leak**, a correctness
    property no probe measures and the two collapse guards beside it assert directly. With the
    spin refuted the drain stands on exactly two legs, and that is the one both kernels share;
    the second — P6's re-arm — has to be stated per kernel, because P6 does not read alike
    across them.
    **P6's re-arm reading is a Linux fact, and Darwin's value is the mechanism rather than a
    gap.** `handler_reset_readable_bytes` is `1` in every committed Linux observation — 72 of
    72 — and `0` in every committed macOS one — 32 of 32; the current-era witnesses are
    `docs/doctor/linux-7.0-2026-08-21-25d8ecd-tier3.json` (`25d8ecd39aed`, `probe_set`
    `4317ea5ac187f506`, 2026-08-21) and
    `docs/doctor/macos-24.6.0-2026-08-13-b346188-tier3.json` (`b3461886e27a`, the same
    `probe_set`, 2026-08-13), with the Linux 6.18 field report at `3e23c52` reading `1` too.
    **Darwin's `0` is not missing evidence; it is the thing P6 exists to separate.**
    `handler_reset_extproc_retained` reads `true` in 64 of 64 Linux observations and `false` in
    27 of 27 macOS ones: Darwin *accepts* the last-close baseline re-assert —
    `handler_reset_applied: true` on both kernels, which says only that the syscall returned —
    and then does not retain `EXTPROC`; and `EXTPROC` gates `TIOCPKT_IOCTL` entirely, so a
    kernel that drops the flag emits no control packet at all, nothing becomes readable, and
    the drain has nothing to consume. Reading the `0` as "the drain is idle here" inverts it.
    **So the drain is justified on both kernels and the justifications are different.** On
    **Linux** it is load-bearing by measurement: the node's own re-assert re-arms readability
    and leaves exactly one byte on the master, which without the drain the handler would re-arm
    for itself — the runaway returning by that route rather than through a stuck `POLLIN`. On
    **Darwin** that packet never exists, so the drain rests there on the write-lock leak alone,
    and **clause 4 of the baseline contract** above — this section carries two independently
    numbered clause lists, and clause 4 of *this* one is the read-the-slave-dry clause — already
    puts Darwin on the poll-only observation story for the same reason: P1 reports the fast-path
    signals **absent** and degrades with the reconciliation poll named as the carrying mechanism.
    So the two probes agree about one kernel, from two directions.
    **P1 reads no byte, and this clause said it did between 2026-08-15 and 2026-08-21** (plan §18
    item 108, §15.72). P1 carries exactly three booleans — `ioctl_packet_on_tcsetattr`,
    `clear_extproc_produces_packet`, `reassert_extproc_via_master` — `false` in **32 of 32**
    committed macOS artifacts, the current-era witness being
    `docs/doctor/macos-24.6.0-2026-08-13-b346188-tier3.json` (`b3461886e27a`, `probe_set`
    `4317ea5ac187f506`, 2026-08-13). `doctor/src/probes.rs`'s `p1_inner` tests
    `b & TIOCPKT_IOCTL` and **discards** the bytes, so no leading byte can reach a report, and
    `0x20` appears in no committed artifact on either kernel. The `0x20` (`TIOCPKT_DOSTOP`)
    reading is real but is a **rig-session** measurement from 2026-07-28 recorded in
    `docs/macos.md`, cited as such at the baseline contract's clause 4 — where its provenance is
    stated instead of an artifact's — and it must not be re-cited as a probe observation, which
    is AGENTS §7's rule and the reason the mis-attribution is worth a sentence rather than a
    quiet deletion. **The identical false attribution was found and repaired once before**, by
    the blind verifiers at the v15 landing, whose record names *"a §7.2 sentence claiming doctor
    P1's artifact names the `0x20` byte it does not carry"*; it was reintroduced here in prose
    that had every other citation right, which is the tell §15.72 is about — a claim verified
    against a neighbouring sentence rather than against the artifact.
    Neither reading licenses deleting the drain: AGENTS §7 forbids a one-way decision on
    single-kernel evidence, and Darwin's `0` is the second leg being *explained*, not the
    property being absent. What the
    port bought is therefore not the hazard the clause predicted but the removal of a guard
    that asserted nothing off Linux. P6 agreeing across **Linux** kernels — 7.0 and 6.18 —
    remains measurement of the friendly platform, never permission to widen the predicate or
    delete the drain.

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
   fault policy faults **this node alone**, with the errno in the node's **fault reason** — the
   port's other consumers keep flowing (§5's isolation). Under `drop-oldest`, where the node
   deliberately stays `active`, the same errno lands in the `write_errors` / `last_write_error`
   pair instead, because that is the only thing separating a disk that rejects every write from a
   merely slow one. *(This clause named `last_write_error` for the fault policy until 2026-08-12;
   the tree puts the errno in the fault reason there and leaves the pair null, deliberately and
   with the reason stated where the code branches — the fault arm's reason string already says it.
   One errno, two policies, two homes.)*
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
8. **Its teardown-ledger figure is a total** (plan §18 item 21, executed 2026-08-12; notes
   §3.86). It was a floor until then — the node's internal merge stage was not reached by the
   §5 ledger, notes §3.31's original defect surviving one stage further in than the fix reached
   — and that queue is watched now, through the same `TargetwardInbox` `serial` and `leg` made
   possible. **Only its targetward half is charged**, and that discrimination lives on the
   queue item's own type rather than in a blanket impl: the merged queue is the one queue here
   carrying both directions at once, so an item tagged with the reserved multiplexed channel
   identity is the raw hostward stream on its way into the child and charging it would report a
   hostward loss under a targetward name (§5's exclusion, notes §3.55). The `deaf.py` fixture
   of §8's register — a child that has stopped reading stdin, so the merge stage is holding
   bytes at teardown — is what makes the guard measure a real quantity instead of a zero. What
   stays outside the figure is what has left the daemon: the child's stdin pipe is delivery,
   exactly as a serial node's device fd is.

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
§3's dual-role list ("existing-terminal nodes (§7.7; in the model, refused at load — §14)") and
§12's "Existing-terminal nodes … pass through as path identities" describe this same deferred node
type. *(This sentence quoted §3 as saying "existing-PTY connectors" until 2026-08-12; §3 has not
carried those words since the v16 rewrite, so the pointer sent a reader looking for a phrase that
is not there.)*

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
    bytes it destroyed as `discarded_at_teardown` under §5's teardown ledger. **`exec`'s figure
    is a total** (plan §18 item 21, executed 2026-08-12): it was a floor until the node's
    internal merge stage came under the same watch, and the reserved multiplexed channel
    identity charges `0` there, that half of the merged queue being hostward. The `deaf.py`
    entry in the fixture register below is the fixture that makes the guard for it measure a
    real quantity instead of a zero. **What remains is a boundary, not a floor:** bytes already
    inside the child's stdin pipe are delivery, exactly as bytes written to a device fd are,
    and no kind's figure claims them — stated here where the counter is documented.

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

**Fixtures that must survive every rewrite.** **Six** Python fixtures under `tests/ext-codec/` and
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
- `strict.py` is the `--error-paths` battery's positive control (item 34): it terminates rather
  than relaying on every injected decode fault, and `passthrough.py` fails all three arms, which
  is what makes the pair a fail-first proof rather than an assertion. Delete `strict.py` and
  `a_strict_codec_refuses_every_injected_decode_fault` dies while the permissive fixture stays
  green — a battery with nothing left to pass it. *(This register said "four" and omitted
  `strict.py` until 2026-08-12; the fixture shipped with item 34 the same week the register was
  last rewritten, which is exactly the gap a must-preserve list exists to close.)*
- `deaf.py` is the only fixture here that is deliberately **inert** rather than deliberately
  broken: a child that has stopped reading stdin, so the exec node's internal merge stage is
  holding bytes at teardown. It is a fixture for the daemon's *accounting* (§15.50) rather than
  for the envelope contract, and it is what makes plan §18 item 21's guard measure a real
  quantity instead of a zero.
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

**Every kit and battery capability this paragraph has ever listed now exists in the tree.** Nine
were filed; all nine shipped in one batch on 2026-08-12 (notes §3.86). Six were already recorded
here as shipped and are described above and in `docs/codec-authors.md`: the attribute-schema suite
(item 32), the `Err`-then-`Ok` recovery suite (item 33), the exec battery's error paths (item 34),
demux-shape exec conformance (item 35, retiring the identity-passthrough limitation), executable
doc examples (item 37), and the second template codec (item 39). The other three shipped in the
**same batch** and are:

- **Golden transcripts of the daemon boundary** (item 36, executed 2026-08-12) —
  `itest/tests/p8_daemon_transcript.rs`, generated against the live daemon so they cannot drift,
  with a `serial-nexus-sim transcript` mode playing both roles off one file (`--record`,
  `--verdict`). A transcript is two *ordered* streams, `<` and `>`, never one interleaved log: the
  exec boundary is two pipes polled concurrently (§15.22), so pinning an interleaving would pin a
  scheduling artifact.
- **A teardown-conservation suite on a codec node** (item 38, executed 2026-08-12) —
  `itest/tests/p5_codec_teardown.rs`, asserting `discarded_at_teardown` and §5's conservation
  equality on a codec node under teardown.
- **A resync-accounting suite** (item 53, executed 2026-08-12) — `resync_is_counted` in the kit
  (`codec-api/src/test_support.rs`), opt-in and parameterized by the author's own malformed unit,
  with `SilentResyncer` as the kit-honesty negative that passes all eight other suites and fails
  only this one.

**This paragraph promised those three as deliberately-absent for nine days after the tree built
them**, and its own fixture register some 160 lines above already described one of them — the class
§15.72 collects, filed as plan §18 item 108. A future capability that really is filed-and-unbuilt
belongs here with its ledger number *and* the answer's date beside it, never in a standing sentence
that outlives the answer.

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
deleting the specification's awkward corner. Two conformance clauses keep a client's read
stream honest (notes §3.5): a parse-error or invalid-request reply carries the spec-mandated
`id: null` rather than being dropped, so a pipelining client never desyncs waiting for a reply
that will not come; and a reply is exactly one of `result` or `error`, enforced in the
deserializer — a present-but-`null` `result` and an absent one are different frames. Everything
is debuggable with socat and jq.

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
   client doctrine: a client commits on success precisely because expiry consumes nothing. One
   stated exception to the *error* half (never to the consumes-nothing half): the pattern wait's
   deadline answers a typed result rather than an error, because for a query a deadline is an
   answer — the divergence is deliberate and recorded (below; §15.56).
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

### The pattern wait

`tap.wait <endpoint>` is a waiting verb on the observation surface: it parks until one of its
named patterns matches the endpoint's hostward stream, its deadline expires, or the endpoint is
torn down. **Specified this generation ahead of its construction, and built the same day**: the
amend-first order (AGENTS §5) ran its full course — §15.56 is the decision record, plan §18 item
47 the construction, and the verb now answers on the wire. The contract:

1. **A wait is §15.20's machinery, unchanged.** It suspends between transitions holding
   nothing, counts against the one-waiting-verb-per-connection rule, is cancelled by
   connection EOF, and a request pipelined behind it is refused with the in-flight error while
   the wait survives. Expiry never consumes — trivially, because the wait is a spy: arming,
   matching, timing out, and cancelling a wait leave the ring, the tap counters, and the graph
   exactly as a never-issued wait would have.
2. **Patterns are bytes, bounded, and named.** A call carries one to eight named patterns,
   each a byte literal or a bytes-oriented regular expression — console output is not
   guaranteed UTF-8, so pattern literals and result context ride the wire base64-encoded and
   the engine never assumes text. Every dimension carries a stated, structurally checked
   maximum under §16.12's rule — pattern count, pattern and name length, compiled-pattern
   size, the lookback window, the context width, and the deadline — checked before anything
   is armed (§7.1's range-check-before-assert shape). **One further maximum is the
   endpoint's rather than the request's**: an endpoint holds at most a stated number of
   waits armed at the same time, because the hub rescans every armed wait's whole lookback
   window on every chunk that endpoint ingests. The request past that maximum is refused
   before anything is armed *or scanned* — the replay ring included — exactly as an
   out-of-range dimension is (§15.70).
3. **The engine is linear-time by requirement.** Patterns compile and match on the runtime
   thread, so a backtracking engine is an operator-reachable denial of service and is refused
   by design: the engine is non-backtracking with a bounded compile size (§15.56 records the
   dependency reality — the linear-time engine family is already in the dependency graph).
4. **Matching is exact where the hub is exact.** The matcher consumes the endpoint's hub
   stream — the same delivered-bytes offset space `tap.data` reports — with no lossy queue
   between hub and matcher, so a byte the hub ingested is a byte matched, and a pattern split
   across data-frame boundaries matches by construction. The producer→hub feed hop stays
   lossy by design (§5): a gap resets the bounded lookback window — a match never spans bytes
   that were not observed — and the gap accounting rides the result, so a "no match" is
   exactly as strong as the observed stream, and says so.
5. **Replay inclusion is splice-exact.** With replay requested, the ring snapshot is scanned
   and the live matcher armed inside one critical section — the `tap.open` register pattern —
   so a pattern wholly emitted before the wait began matches, and the ring→live seam can
   neither hide nor duplicate a match. The matcher reads the ring itself, never a tap
   channel's budgeted copy, so ring depth is never silently trimmed out of the scan.
6. **The result names its evidence.** A match reports the pattern's name, the match's byte
   offset and epoch in the endpoint's offset space, and a bounded window of surrounding
   context (base64). Deadline expiry is a **typed result, never an error** — `matched: null`
   beside `timed_out: true` and the bytes-scanned and gap counters — because "no pattern
   appeared within the deadline" is an answer, and a verdict never leaves a deadline unnamed
   (plan §3). Teardown, removal, or graph replacement while parked is the **typed error**
   outcome, distinct from expiry — the `tap.closed` discrimination, applied to a wait. The
   result rides the parked request's reply, never a broadcast notification: matches in the
   notification stream would leak into every `subscribe` consumer (§15.56).
7. **An armed wait is an observer with a tap's obligations.** It counts toward the endpoint's
   mirror activity exactly as an open tap does — an armed wait on a ring-off, untapped
   endpoint still observes — it never affects `discarded_unattached` (§15.32's tripwire), and
   it runs concurrently with every other consumer on the same endpoint — pty spies, log nodes,
   open taps — as any fan-out observer does (§4). It is visible in `state` for its lifetime,
   as taps are.
8. **The web bridge does not carry it at introduction.** A parked wait occupies its
   connection's one waiting slot, and the browser page holds exactly one daemon connection —
   an admitted wait would answer every other page verb with the in-flight refusal for its
   whole duration. Admission to the allowlist is a later, deliberate act that states that
   consequence (§15.34's screening rule; the exclusion is recorded at §15.56).

### The verb surface

The verb surface, grouped by semantics: configuration (`load`, `load --replace`, `dump`,
`add-node`, `remove-node [--cascade]`, `connect`, `disconnect`; `set-attribute` remains deferred,
§14); observation (`state`, `subscribe`, `info`, `ports`, `tap.open`/`tap.close`, `tap.wait`);
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
construction bounds, two platform arms, and the rule that a path-accepting verb (or a new
capability) is a design amendment, never a patch — is §15.45's contract, amended by §15.55: the
access grant reauthorization destroys, restored by the helper that destroyed it, at the cost of a
second capability. Its narrowness is a standing tripwire (§5).

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

1. **The probe roster is P1–P16**, and `docs/serial-nexus-doctor.md` is the probe registry of
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
   half-crossed wire is named rather than mysterious, and a port whose peer vanishes mid-probe
   is a fourth arm, `hung up (peer closed) — not classifiable`, which **degrades** the verdict
   rather than reading as `dangling` beside a clean certificate — and characterizes what the
   rig supports, producing the certificate every tiered checklist run starts from. The UART
   predicate is portable (§15.47), so a rig certifies on any kernel.
   **Misreading callout, kept adjacent:** the shipped certificate contains a deliberate
   **baud** mismatch and local break **assertion** — not a parity mismatch, break reception,
   or far-side modem signalling (review 37, 37-TOOL-3; §15.21). Those belong to the checklist
   run the certificate is the precondition *for* — and both are since **executed** as suite
   guards rather than grown into P5, the doctor gaining nothing (plan §18 item 17, executed
   2026-08-15; notes §3.108; §15.21's own annotation): break reception raises the far node's
   `driver_counters.brk` by 1 with `frame +0` — one per break *event*, a 250 ms and a 25 ms
   break moving it alike — and the parity mismatch answers **counted, not lost**: an 8E1
   transmitter into an 8O1 receiver reads `parity +2` while the 43-byte payload arrives
   byte-exact, so the pre-registered prediction of payload damage is refuted and recorded.
   **Both readings are scoped to `ftdi_sio` on Linux 7.0.0-29, on a cross-wired FT232R pair**
   (plan §18 item 17 scopes them, and the
   `IGNBRK | IGNPAR` reading that explains why an errored character still arrives is P15's, at
   `docs/doctor/linux-7.0-2026-08-14-b58a1c4-tier3.json`, `b58a1c4b7fc8`, `probe_set`
   `4317ea5ac187f506`, 2026-08-14).
   **The break half has a second reading, and it disagrees.** This clause added *"and neither has
   been taken anywhere else"* on 2026-08-15 and this document falsified it on its own page the next
   day (plan §18 item 108, §15.72): §15.62 clause 3 records a 250 ms break on `cdc_acm` moving
   **`frame` +1 with `brk` +0**, filed and executed as plan §18 item 81. Where `ftdi_sio` moves
   `brk` +1 with `frame` +0, that driver moves the other counter for the same physical event.
   **Two drivers disagreeing about which counter a break lands in is worth more than one driver
   measured once, and it is what the clause is written around**: which counter rises is the
   *driver's* choice and no part of anything serial_nexus promises, so §15.21's checklist clause
   asks only whether a break was **received** — it was, on both — and the guard names the counter
   it found rather than pinning one, re-running its idle-window control against *that* counter so
   a driver whose framing count free-runs cannot have noise chosen for it. `ftdi_sio`'s
   break-over-parity-over-framing precedence is therefore one driver's ordering and never a kernel
   fact; a pre-registration that assumed it was already refuted once (plan §18 item 81's own
   pre-written message called the `cdc_acm` reading a *refutation, not a product defect*).
   The **parity** half is the one with no second reading anywhere, and saying so narrowly rather
   than over both halves is the whole use of a scope sentence.
   Both guards self-skip off Linux on
   `serial_nexus_sys::ICOUNTS_SUPPORTED`, and that is a **real gap rather than a formality** —
   Darwin has no equivalent input-error counter, so this clause is unanswerable there until an
   observable exists, and no rig creates one. The far-side handshake wiring is since
   measured by P5's pair block (clause 9, §15.52), reported and never judged.
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
| P16 | Does a held pts slave fd report `POLLHUP` once the master closes, and stay quiet while it is open? |

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
   covers stimulus, never setup. And *when* in the measured object's life a configuration
   call runs is part of the question it answers: the identical call on the identical fd can
   answer `Err` at creation and `Ok` after a hangup — measured (notes §3.45) — so a success
   at one lifecycle point licenses nothing about another.
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
kernel delta at all. A diagnostic heuristic recorded from a same-kernel comparison carries its
comparison scope: applied across an OS boundary, read-it-as-scheduling is the wrong first
instinct, because the line discipline a probe itself leaves behind outranks scheduling by an
order of magnitude (notes §3.34).

**A probe that reconfigures hardware restores before it explains (notes §3.68).** The restore
runs before any early return is inspected — no `?` between the set and the restore; the restore
claim is verified by the probe's *last* read of the port, because a restore decided before the
probe's final write structurally cannot verify itself; and no error path answers `skipped` while
hardware is left reconfigured — `skipped` is the most reassuring word available, and it is also
the word that exempts every conditional gate clause (gate-design rule 3). P15's first-ranked
field shipped both holes; both were repaired and verified externally, `stty` on both ports after
a Tier-3 run.

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

**The vacuity taxonomy — six shapes of one defect**, each found separately in this tree before
being named as a class; a new instrument is checked against all six:

1. *Zero-iteration loops*: an all-pass fold over an empty population.
2. *Discriminators that cannot fire*: a single sample at a point every admissible threshold
   predicts identically.
3. *Axes that cannot vary*: replicates wearing a factor's name — a 2×2 that was a 1×2.
4. *Certificates that cannot fail*: a predicate false on an entire platform routes every input
   to the accepting arm, so a cleanly wired rig reads `supported` whatever the hardware does.
5. *Guards pinning the platform-of-record's answer* instead of the promised property: green
   where written, red or vacuous everywhere else.
6. *Bounds that cannot fire*: a `timeout` wrapped around a loop whose injected stub never
   parks — a ready-`Err` future consumes no coop budget, so the timeout future is never polled
   and the test hangs instead of failing (review 32's LEG-3: the stub must itself park after a
   cap, or the bound is unreachable).

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
| `e79f5fcd86a2e5f0` | P1–P15, corrected citation | the six `2b44c17` rows of 2026-08-07 — Tier-3 and passive triples from one clean Linux build, `jq -e` executed against both halves — and the era-closing `8c00078-dirty` Tier-3 triple, an era-mate rather than an opener: same `probe_set`, moved `field_set` (plan §18 items 14 and 22), which closes nothing under clause 4. **Two halves of this era were never taken and are now unobtainable rather than owed**: no Darwin capture exists in it at all, and the closing triple has no passive counterpart — the cost of taking a rig capture and an instrument change in one session | opened by notes §3.73; closed 2026-08-13 by P16's arrival with P15's widened `question` folded into the same boundary — one boundary, two changes, deliberately (§15.59; notes §3.89/§3.90) |
| `4317ea5ac187f506` | P1–P16 | the current era, opened 2026-08-13 by P16's arrival together with P15's widened `question` — **one boundary, two changes**, deliberately (§15.59). The `field_set` moves earlier the same day (P15's software reading, P13's fifth shape) are **not** boundaries — clause 4 — so the artifacts either side of *those* stay era-mates, while nothing diffs across *this* row | opened by notes §3.89/§3.90; open |

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
  shape is weaker in what it *says*, not in what it *does*. **Which shape an entry gets is
  decided by where the deferral sits in the type system, not by taste** — plan §18 item 45,
  closed as a decline 2026-08-12. A deferred *role* of a shipped kind (entry 14: `faces =
  "target"` on a serial node) has no other option, because `faces` is the two-valued [`Facing`]
  every dual-role kind carries and target-facing is legitimate elsewhere — the schema cannot
  exclude it, so `validate` must. A deferred *whole kind* (entry 15) has the stronger option and
  takes it: a word the schema never admits is unreachable by construction, where a `validate`
  refusal is one forgotten call away from admitted. The precedent for preferring the schema is
  §15.8's configuration/state split, which `core/src/config.rs` states as its own first rule —
  state fields *do not exist* on configuration types, so the question cannot be asked rather than
  being asked and refused. (**Not** §15.4's merge diamond, which an earlier draft of this
  paragraph cited: the one-producer invariant is a `GraphModel::validate` refusal
  — `TargetEndpointOversubscribed` — so it is an instance of the weaker shape, not the stronger
  one. Corrected 2026-08-12.) Upgrading entry 15 to entry 14's shape was measured on a scratch
  tree and costs
  two things permanently: serde's internally-tagged error enumerates every variant, so a plain
  typo would be answered with a list advertising a kind the daemon refuses one stage later; and
  §7.7 states two fields and then "otherwise it behaves as a boundary", so the rest of the field
  set would be a guess frozen by `deny_unknown_fields`, while `shape()` would owe `to_model` an
  endpoint topology the design never states — letting an operator's *edges* validate against a
  shape nobody designed.
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
    no deferral. Making it structural was plan §18 item 45, and the item is **closed as a
    decline** (2026-08-12): the schema's silence is the stronger refusal for a whole kind, for
    the reason the vocabulary above now states, and the two costs of the alternative were
    measured rather than argued. The guard that asserts today's behaviour stays, and it is now
    the tripwire on the decline rather than a waiting room — planting the variant was measured
    to redden both guards at their error-code assertions (`-32002` where they demand `-32602`),
    because the word stops being unknown one stage before the listed-kinds clause is reached.

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
3. The single-line input drives `send`; a LOCKED refusal shows the holder by name, and whether the
   line is then sent over that holder follows a **standing, visible operator preference** —
   never a steal the operator did not set. *(Amended 2026-08-17 by §15.66. This clause read
   "…with an explicit steal affordance — never an automatic steal", and the affordance was a
   `confirm()` per refused line; the affordance is now a checkbox beside the send box, checked by
   default. The steal stays announced and stays confined to `-32003`; what is given up is the
   per-line confirmation. Read §15.66 for why the modal was the wrong shape for the question.)*
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
transcript. The server side takes exactly **one** protocol crate under the §13 gate —
`tokio-tungstenite`, for RFC 6455 framing after the handshake — and hand-rolls every byte of HTTP
on tokio: head parsing, static-asset routing, the token/Host/Origin gate, and the 101 upgrade,
matching the daemon's hand-rolled JSON-RPC (§15.13). *(This sentence said "permissive HTTP and
WebSocket crates" until 2026-08-12, asserting a dependency the tree does not have and hiding the
fact that the security-relevant surface — the gate and the head parser — is code in this
repository rather than a vendor's.)* The browser link carries base64 chunks relayed from
`tap.data`.

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
keyed by the **web origin** (a stable `host:port`, standing in for the daemon's socket path — a
value the daemon never puts on the §10 wire, so no browser client could key on it), the endpoint
address and the daemon `instance` nonce, with
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
  becomes a stub. This record keeps the prior generation's numbering through §15.55;
  §15.56, §15.57, §15.58 and §15.59 are this generation's additions.
- **Refutations are decisions.** A refuted diagnosis or a declined proposal is recorded like an
  adopted design — falsifier (or reason), outcome, status — because refutations are what stop a
  rejected shape from being re-proposed on no new evidence. Silently re-fixing a declined item is
  a defect (AGENTS §5); overturning one takes new measurement or an explicit recorded decision,
  never drift (§15.48 carries the exemplar overturn, notes §3.37 → §3.43).

**Topic index.** "Was this decided, declined, or refuted?" should be one lookup. Every entry
§15.1–§15.59 appears below, titled at its primary topic (numbers alone on repeats); an entry can
appear under more than one topic.

- **Graph model and vocabulary** — §15.2 typed endpoints · §15.3 orientation vocabulary ·
  §15.4 one-producer invariant
- **Data plane and boundaries** — §15.5 boundary policy · §15.18 poll(2) readiness / AsyncFd
  ban · §15.19 hybrid data plane · §15.23 endpoint-keyed wiring · §15.50 teardown ledger ·
  §15.54 pump park-or-drain
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
  §15.35 ports, edge surgery, web editor · §15.43 stdin-EOF leash · §15.56 pattern wait
- **Web console and map** — §15.28 web console architecture · §15.29 web bind policy · §15.32
  default scrollback · §15.33 map node · §15.35 · §15.37 Playwright
- **Doctor and measurement doctrine** — §15.17 doctor consolidation · §15.21 rig discovery /
  certificate · §15.44 two digests · §15.46 instrument self-testimony · §15.47 portable
  certificate · §15.49 a zero is a claim · §15.51 P14 maximum-rate search · §15.52 handshake
  continuity · §15.57 the Markdown rendering is a view, not a format · §15.58 actual
  baud in node state · §15.59 P16, the slave-witness liveness instrument
- **Harness and validation doctrine** — §15.31 harness as crate · §15.34 review-26 classes
  become rules · §15.36 flake doctrine · §15.37 · §15.48 provider seam, last-hop physics
- **Platform and privilege** — §15.13 · §15.30 · §15.45 privileged replug capability ·
  §15.55 devprep access grant
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
**Executed 2026-08-15 (plan §18 item 17; notes §3.108), and two sentences above want reading with
that answer beside them.** Both clauses are now suite guards asserting `driver_counters` — the
doctor grew nothing — so naming
`p12_serial_exclusivity::a_break_straddled_by_a_replace_leaves_the_line_transmitting` as "the
in-tree guard for the break clause" is **imprecise**: that test guards the SERX-2 *restore* half,
asserting the line resumes transmitting, and reads no counter. And framing both clauses as items
whose evidence would be a corrupted pattern does not survive measurement: on `ftdi_sio` /
Linux 7.0.0-29 the parity clause answers **counted, not lost** — the receiver's `parity` counter
rises and the payload arrives byte-exact, because that driver's `TIOCGICOUNT` counter and its
per-character `TTY_PARITY` flag are independent, so `IGNPAR` (verified in force,
`docs/doctor/linux-7.0-2026-08-14-b58a1c4-tier3.json`, P15, `iflag_before_hex: "0x5"`) never
receives a flagged character to drop. **Payload damage is therefore not the evidence for either
clause**, and a guard demanding it would pin another driver's decision rather than this system's
mechanism.

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
unknown, never "equal" — both expectation files require it by presence, unconditionally:
provenance must be present. The P4 contrast is precise and is not a live abstain arm: the
*gates* do not demand a population of a non-supported P4 — the probe itself always states one
(notes §3.48 ships `p4_always_reports_its_population`) — while a `supported` P4 must state it
non-zero (§15.49); the abstain arm the archive once justified is removed (notes §3.48). `field_set` is a run property, not a binary property; three sequential runs is
the floor for run-varying quantities; the strongest basis is outside both digests — same source
on both sides, a git diff over the doctor's crates empty (notes §3.73). Amendments: `field_set`
sorted (notes §3.72); `--field-set` exits 2 on a zero-observation report (notes §3.74).

**Withdrawn-figures register.** 32/35 — the cross-binary leaf-path pair an earlier commit
message quoted — is WITHDRAWN (notes §3.51): no collapsing of the paths reproduces it; never
re-quote it. The reproducible figures are 65 (`7ead470` → `1a9a8fc`) and 71 (`fa4b12d` →
`1a9a8fc`). Also held here, per §16.13: the zero-timeout-poll calibration readings that never got
artifacts — the `166/172/166/166 ns` cells, the `195 vs 263` headline, and the `145–152 ns` range
(notes §3.41, §3.44) — are scrollback beside the committed `-05b` triple's figure of record, and
are never re-quoted.

**The era-close law.** An era is the set of captures sharing one `probe_set`; a new probe id —
or a corrected question string — is a new instrument (notes §3.57) and moves the digest
deliberately, closing the era. A closed era's artifacts stay comparable with each other, no
later capture joins them, and P1–P14 are never diffed across an era boundary without the
mismatch stated (notes §3.73). The eras — `a131e1f4b46d6c83` → `94d64d8bbacf1174` (P14) →
`82a8e2198e54626a` (P15) → `e79f5fcd86a2e5f0` (notes §3.73) — are tabulated at §13.

### 15.45 A privileged capability the repository carries, not the machine

**Status:** LIVE — AMENDED by §15.55, which adds the `grant` verb and a second capability
(`cap_fowner`) and renames the helper; this entry remains the base contract's home: one contract,
two platform arms, one refusal; the founding-premise measurement at §12, the narrowness tripwire
in §5's table.

The §12 identity promises had never met a real hotplug (`p7_replug.rs` re-links a fixture tree
— AGENTS §9's proxy in space), and the real operation, writing `0` then `1` to the USB
`authorized` attribute, needs privilege. DECLINED, STANDS: a udev rule granting `dialout`
write — machine state; a moved checkout silently loses
the capability (do not fuse with `packaging/99-serial-nexus.rules`, which is deployment
configuration, not test capability). DECLINED, STANDS: a capability-conferring test runner —
`CAP_DAC_OVERRIDE` is root-equivalent, ambient across every `fork`/`exec` the tests make, and a
daemon holding it would prove the daemon works as root (AGENTS §9's proxy at the largest scale).

One binary, `serial-nexus-devprep`, carries `CAP_DAC_OVERRIDE` on a copy its own `install` verb
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

*[**Annotated 2026-08-21 (plan §18 items 103 and 104, §15.71): the second bound was violated in the
tree, and the guarantee cited for it answers the wrong half of the hazard.** `install` answered *what
capabilities does this file carry* by spawning `getcap`, and the refusal that was supposed to contain
that guarded the **verb** while the spawn lived in a **module** two verbs used — `preflight` reached
it with no refusal in front of it, so a blessed copy execed a `PATH`-selected binary while holding
both capabilities (measured: `CapPrm` = `CapEff` = `000000000000000a`). `PR_SET_NO_NEW_PRIVS` stops
an `exec` from *gaining* privilege and says nothing about one that inherits the environment and the
`PATH` of a process that already holds it. **The bound's wording above is unchanged**; what changed
is that it is now enforced by a gate rather than by this paragraph, and the capability reader is one
`getxattr(2)` in `serial_nexus_sys::caps`. §15.71 is the record.]*

Crash safety is the helper's: no `deauthorize` verb — `cycle` does both writes in one process,
re-authorizes on signals, caps any hold at 30 s; an idempotent `authorize` repair verb exists.
Self-invalidation is the primary bound: any write clears `security.capability`; mode `0700`
before `setcap` is the real, same-user boundary; CI never blesses. Re-blessed twice during
bring-up, the shape shrinks what the blessed binary decides: `hold` deauthorizes, waits for
stdin EOF (§15.43's leash), reauthorizes — the caller samples unprivileged, the blessed binary
containing the two writes and nothing else. `scripts/bless` is the one command (byte-for-byte
staleness, no stamp file); the capability is proven, never assumed — and a `--verify` answer of
`Stale` names build-drift of `target/debug/` from the blessed install, never a lapsed blessing:
the installed copy stays blessed until rewritten, so a Stale report alone never explains a
skipped lane — the `SNX_REPLUG*` variables answer that first (notes §3.62). `SNX_REPLUG=required`
reddens the self-skip via the same mechanism as `SNX_CROSSOVER=required`; preflight answers
ready / blocked-on-bless / genuinely-not-ready.

Two arms, one refusal: a platform dispatcher over `devprep/src/linux/` (notes §3.65); the macOS
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
*(Annotated 2026-08-13, §15.60: that last sentence was true when written and the ladder that
makes it false had already shipped — the decisive rung is `D = 1`, and it is in the constant
this entry itself spells. Read §15.60 for what the Darwin ladder settles and what it leaves
open; nothing here is rewritten, and the rung list is unchanged.)*
Two further refutation chains are register entries here, because AGENTS §9 makes them
load-bearing: *"Darwin's P7 degrade is an artifact of the lost baseline"* — REFUTED: the degrade
is genuine, produced by the last-close flush P13 measures, with `silence_cause` discriminating
`hangup-destroys-evidence` from `extproc-unavailable`; and the second §3.40 refutation was itself
refuted by its own pre-registered falsifier *firing* — `baseline_via_master: false` in 12 of 12,
the identical `set_baseline(&master)` answering `Err` at creation and `Ok` after the hangup, so
at creation Darwin always takes the momentary-slave fallback (notes §3.40, §3.45).
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
clock — notes §3.43), `SNX_SERIAL_PAIR=rig` hard-failing with no rig visible, the provider printed
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
**Closed 2026-08-12 (plan §18 item 21; notes §3.86): the `exec` floor above is a record of what
that item closed, not a live limit.** Every kind's `discarded_at_teardown` is exact now. The
internal merge stage is watched through the same `TargetwardInbox` this entry's `serial`/`leg`
adoption built, so the two-sided invariant holds one stage further in than the fix it describes
reached — notes §3.31's original defect, surviving that far and no further. **The item's own
filing got one thing wrong, and the correction is the load-bearing half:** the merge queue is
*bidirectional*, roughly half of it being the raw device stream on its way into the child under
the reserved multiplexed channel identity, so notes §3.55's sketched "it needs only a watch"
would have charged hostward bytes to a targetward ledger — the precise error the leg's
per-channel relay is excluded to avoid, one node kind over. The charge is discriminated on the
queue item's own type instead, which is why that item is a named struct (§7.6 clause 8, §8
clause 14). *Guard:*
`p13_teardown_accounting::exec_teardown_counts_a_merge_stage_a_deaf_child_is_holding`, against a
`tests/ext-codec/deaf.py` child that has stopped reading stdin so the merge stage is holding
bytes at the instant of destruction; *fail-first:* with the merge queue's watch removed the
removal reports **708000** destroyed — the host-facing queue alone — against a fixed **1736000**,
the remaining 64000 being the child's stdin pipe, which is delivery and not this counter's to
claim. The pty's held `pending` payload is untouched by any of this and stays in the closing
register. **Annotated 2026-08-21 (plan §18 item 108)**, when the three settled-system
restatements of the floor — §5's teardown-ledger clause 9, §7.6 clause 8 and §8 clause 14 — were
found still calling it open nine days after the tree closed it, while §8's own fixture register
described the guard that closed it some 160 lines further down §8 itself.

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
and a loss are different facts); `platform-refused` names the ask surface, never
the wire — measured on the committed Darwin triple (`42eac2a`, 2026-08-05): every rung to
3000000 accepted and byte-exact, the 3062500 ask refused by the set call itself,
`max_reliable_baud: 3000000` under `ceiling_kind: platform-refused` a fact about what that
platform lets you *ask* (§15.47's unmeasurable-is-data; the "caps the ask at 230400" figure an
earlier revision carried here was the pre-implementation expectation, and no committed capture
supports it); `structural-cap` names the instrument's own limit; a
vanished peer is `HungUp` and **degrades**, as does an exhausted search budget — never a ceiling.
`supported` whenever the measurement completes, whatever the number — slow is not broken (P13's
rule that the probe never judges); against the sim's null modem the pure, CI-testable search
reports `skipped(not a UART)`, so the claim never executes where a pts would vacuously satisfy it
— one predicate read twice, not two independent gates: `p5_is_uart`, measured on Linux (a pts
answers `ENOTTY` to both ioctls) and unverified from that box on Darwin, the named single point
of failure (notes §3.61);
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
**Re-measured (notes §3.80, 2026-08-12):** a different adapter pair (`ABSCDGL6` ↔ `BH00L4KU` —
this clause read "the same adapter pair" until 2026-08-16 and **contradicted the paragraph twelve
lines below it**, which has always named both pairs correctly; the error originates in notes §3.80,
which says "this same adapter pair" twenty lines after recording `ABSCDGL6` on that bench), same
probe, same `probe_set` era answered **3-wire** — no handshake lines carried — by two independent instruments: P5's
port-to-port continuity, and the daemon-path RTS/CTS read whose positive control (`rts: true`,
both polarities) proves the reader lives. The cabling explanation is favoured, not proven; no
diagnosis is claimed. The 5-wire discovery above stays what it was — a dated record of that bench
on that day — and notes §3.63's 25 ms end-to-end measurement is not refuted: its *precondition*
went away, which is exactly why `SNX_RIG_FLOW`'s precondition is measured per run rather than
remembered (plan §3), and why the documented rig lane drops it on a bench that measures 3-wire.
Plan §18 item 17's bench re-inspection clause carries the follow-up.
**Re-cabled and re-measured (notes §3.102, 2026-08-14):** the operator re-cabled the bench and it
answers **5-wire** again — `rts_a_to_cts_b=true rts_b_to_cts_a=true` with all six DTR crossings
`false`, reproduced 3 of 3 and committed as
`docs/doctor/linux-7.0-2026-08-14-b58a1c4-tier3{,-2,-3}.json`. **The adapter pair is one the record
has never seen**: `BH00L4KU` ↔ `BH00LW9U`, where the 5-wire discovery above ran on
`BH00L4KU` ↔ `BH00LL8O` and the 3-wire re-measure on `ABSCDGL6` ↔ `BH00L4KU`. Three cablings, three
pairs, one adapter common to all of them — so this reading corroborates the discovery's *shape*
without reproducing its conditions, and no cell may be diffed across the rows as though only the
cable moved (item 20's confound, one instrument over). What the re-cable did **not** bring is DTR:
a third independent measurement now says those six crossings carry nothing, which is why plan §18
item 28 stays blocked rather than merely unscheduled, and why the suite's DTR negative control
(`crossover_rig_rts_crosses_to_the_far_ports_cts`) remains a valid control rather than a tripwire.
**The clause's presence-never-answer form is now measured from both sides** rather than argued from
one: `jq -e -f expectations/linux.jq` exits 0 on a 5-wire report exactly as it does on a 3-wire one.
That is rule 14 working, and it has a consequence worth stating plainly — **no gate in this tree can
tell a 5-wire bench from a 3-wire one, by design.** The only instruments that can are the committed
P5 artifact and the two `rts-cts` end-to-end tests under `SNX_RIG_FLOW=required`, which is the whole
reason that spelling exists and not a redundancy in it.
**Scoped 2026-08-16 (§15.62):** that last sentence is `ftdi_sio`'s. Both named instruments read CTS
through `TIOCMGET`, and on `cdc_acm` the driver synthesises that bit — so on a CDC-ACM bench
**neither can tell**, and a physically 5-wire rig read `stuck-high` in both directions. The
instruments are not wrong; their subject has to be able to report CTS, and one whole device class
cannot. §15.62 carries the reading, the vocabulary repair, and the scope.

### 15.53 A transport's contract is refused, not degraded: hardware flow control at load

**Status:** LIVE — restated at §7.1 (pre-check) and §11 (pre-create precheck contract); history,
declines, and the falsified claim here.

`serial2` verifies by read-back, so a driver that accepts `rts-cts` and reads the flag back clear
produced a `faulted` node *after `load` had returned success* — measured, not hypothetical: Apple's
`IOSerialFamily` does exactly this on an FT232R (P15, notes §3.65 E) while Linux honours the flag
(`cflag` delta exactly `CRTSCTS`, notes §3.68). The decision — refuse at `load`/`add-node`, before
anything is created, through the one predicate `serial_nexus_sys::honours_rtscts` [**stale symbol, noted 2026-08-21**: §15.61 parameterised it to `honours_flow_control(path, FlowMode::RtsCts)`; the name in this sentence no longer exists in any `.rs` file — plan §18 item 96; the
*four states* half of this sentence is correct and survives — three `FlowOutcome` variants plus the
`Err` arm] and its four
states (`Honoured`, `Refused`, `AcceptedThenDropped`, and `Err` for unmeasured — §7.1 clause 2,
restated 2026-08-12 when the tree gained the third measured arm the contract had always named)
— is stated at §7.1/§11, with §7 as the reason rather than an exception: what §7 forbids is
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
`flow_control = "none"` and 0, 0, 0 with the pre-check disabled; DTR is inferred, not observed — the rig
leaves it unwired (notes §3.68). And the citation debt: §15.51 carried P15's `question` citation
because this entry did not exist; notes §3.68 filed and declined the string fix, and notes §3.73
**overturned that decline**, deliberately moving `probe_set` `82a8e2198e54626a` →
`e79f5fcd86a2e5f0` and closing that era — a correction only gets dearer as captures accumulate
under a wrong citation.

*[**Annotated 2026-08-21 (plan §18 item 85, §15.73), not rewritten:** every refusal above is
decided on what the driver *accepted*, which is the only thing this entry's instrument — one port,
one `tcsetattr`, one read-back — can see. A driver that keeps the flag on read-back and is inert on
the wire satisfies this predicate and is not refused. That is a stated bound rather than a gap:
§15.73 records the decision (no new refusal; the functional question goes to the doctor, where a
peer exists), its one hardware arm, and why the bench that motivated it is one the new instrument
cannot be run on — notes §3.147. Nothing here moves.]*

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

**The helper is `serial-nexus-devprep`** (`devprep/`), renamed from a name that described
only its first verb. The trigger is this entry: once `ensure_rig_access()` calls it at the
start of every run that names a rig, a tool invoked by every rig test carried a name naming
one operation, which advertises something far narrower than what exists. The retired spelling
joins §15.40's banned vocabulary so it cannot drift back, and the word *replug* keeps its
meaning everywhere it denotes the operation — `SNX_REPLUG*`, `skip_no_replug`, the replug lane
(notes §3.81, which also records what a tree-wide ban costs an append-only record).

**Two residuals, recorded rather than hidden.** The grant does not survive the *next*
re-enumeration either — nothing put on an inode does — so it is re-applied per cycle by
construction rather than being durable; the durable answer remains group membership or a
udev rule, and this exists for the case where neither is available without a re-login. And a
copy blessed before `cap_fowner` joined the set replugs exactly as before and *skips* the
grant with a note, rather than failing: a capability that arrived later must not turn every
previously-working `cycle` red.


### 15.56 The observation surface gains a pattern wait: match daemon-side, once

**Status:** LIVE and EXECUTED — contract at §10 (The pattern wait); built at plan §18 item 47
(2026-08-12, notes §3.83). This entry was written one revision ahead of the tree, as AGENTS §5
requires; it no longer is, and every decision below is implemented rather than promised.

The capability: park until a byte pattern appears on an endpoint's hostward stream, with a
deadline, optional replay-ring inclusion, and a result that names which pattern fired and at
what offset. Everything *below* a matcher existed — offset-stamped taps with exact-splice
replay, the waiting-verb machinery, the ring — and no expectation matching existed anywhere in
the daemon, the CLI, or the harness's RPC surface. The consequence is measured in this tree,
not imagined: every consumer that watches a console re-derives the same client-side matcher
with the same subtle bugs — frame reassembly across `tap.data` boundaries, replay-splice
handling, gap discipline, timeout-versus-teardown discrimination — and the tree itself already
carries four independent tap consumers (the CLI, the web page, the headless ws client, the
harness) with three hand-rolled reassembly implementations among them. One daemon-side matcher,
on the machinery that already makes the splice exact, is §16.1's move — encode the subtle rules
once — applied to the observation surface.

Decisions, and their reasons:

- **The verb, not only the recipe.** A documented client recipe over `tap.open` closes most of
  the gap and none of it well: it re-pays the correctness burden per client; it cannot scan
  ring depth beyond a tap channel's replay budget, while the daemon-side matcher reads the
  ring itself; and its timeout typing is only as good as each copy. The recipe is still
  written — it documents the offset contract every tap client needs — as an item-47
  deliverable beside the verb, not instead of it.
- **Result-shaped timeout.** Deadline expiry answers `timed_out: true` with the scan and gap
  counters, never an error: a deadline is an answer (plan §3's the-sim-marks-`timed_out`
  doctrine, applied to a verb). Error outcomes are reserved for a wait that did not run or
  cannot continue — structural refusal, a second waiting verb on the connection, endpoint
  teardown.
- **Dependency reality, stated under §13's policy.** The linear-time engine family
  (`regex-automata`, `aho-corasick`, `memchr`, `regex-syntax`) is already in the lockfile
  transitively (tracing-subscriber → matchers), all under permissive licenses, so the matcher
  adds zero new lockfile packages; what it adds is a new *direct* dependency edge of the
  daemon, which is the reviewed decision this entry records. A literals-only first cut
  (aho-corasick) is the sanctioned narrowing if the regex half stalls: it meets every
  acceptance clause except the regex grammar itself.

DECLINED, recorded so they are not re-proposed (AGENTS §5):

- **Matching inside the hostward deliver path** — `deliver` is §15.19's hot path with a
  measured throughput bar; the matcher consumes the hub stream on the runtime thread's
  scheduled side instead, as tap fan-out does.
- **A backtracking regex engine, or patterns over decoded text** — the first is an
  operator-reachable CPU denial on the thread that runs every console; the second assumes
  UTF-8 of a stream the design explicitly does not (§10's contract).
- **Unbounded lookback** — a match window without a stated maximum is an allocation and a scan
  cost an operator input controls; §16.12's rule applies before the first byte.
- **Broadcast delivery of matches** — a match notification would reach every `subscribe`
  pipeline (the CLI prints unknown notifications to stdout by contract); the result rides the
  parked request's reply, the `lock --wait` shape.
- **Web-bridge admission at introduction** — excluded deliberately; §10's contract states the
  one-connection consequence any later admission must weigh, and the allowlist stays a
  deliberate act (§15.34).

*[**Annotated 2026-08-21 (plan §18 item 64(a), §15.70):** the *Unbounded lookback* decline above
names the right rule and reaches one factor of three. The hub's per-chunk work is
`waits × lookback × the pattern's cost per byte`; the **list length** carried no maximum until
§15.70 gave it one — `MAX_ARMED_WAITS = 64`, refused ahead of the replay scan — and the **pattern's
cost per byte** still carries none, measured there at an 18× spread between a literal and a
prefilter-less regex and recorded rather than fixed. Nothing decided here moves.]*

### 15.57 The doctor's Markdown is a view, not a format: the value grammar stays non-injective

**Status:** DECIDED — the escape is DECLINED and the Markdown's non-contract is stated instead.
Plan §18 item 27, which asked for exactly this decision before any renderer change (notes §3.74
filed it "not fixed" for the same reason).

The defect, stated precisely so the decline is not mistaken for not noticing it. The Markdown
renderer leaves `", "`, `=`, `[` and `]` unescaped inside *values*, so `{"note":"a","b":"c"}` and
`{"note":"a, b=c"}` render to the same line. The grammar is therefore non-injective by
construction: a reader cannot in general recover the observation tree from the rendering, and
eight distinct JSON values were demonstrated to render byte-identically — `2`/`"2"`,
`null`/`"null"`, `true`/`"true"` among them (notes §3.74).

**The decision: do not escape.** Three measurements decide it, none of them "it would be work".

1. **The JSON is the artifact of record and the Markdown is a view of it**, which is measured
   rather than asserted: the Markdown is a pure function of the JSON model minus one field
   (`generated_unix_ms`), validated at 228 of 228 passive lines and 290 of 291 Tier-3 lines; the
   reverse fails at 0 of 1064 Tier-3 scalar leaves, none of which carries its JSON kind, against
   an `expectations/*.jq` gate with 22 `type ==` clauses **at `bc75857`, and 51 in each of `expectations/{linux,macos}.jq` at `eba5548`** — taken there by `9237cfc`, re-measured 2026-08-15 (notes §3.109). **The figure moved and its direction strengthens the decline rather than weakening it:** more of the gate's assertions turn on the JSON's kinds, so the case that the JSON is the artifact of record and the Markdown a view is stronger now than when the entry was written. Making the *view* injective would not
   make it the artifact — the kinds are still gone — so the escape buys a property nothing needs
   and does not buy the one thing that would matter.
2. **The cost lands on immutable evidence.** §16.13 freezes every committed `docs/doctor/`
   report, and the era record and the comparability ladder (§13) read across them. An escape
   changes the rendering of all of them at once, which is either a mass edit of frozen artifacts
   (forbidden) or a permanent split between reports rendered before and after — a diff hazard in
   exactly the corpus whose value is that it diffs.
3. **The practical loss today is zero and is not the argument.** A heuristic parser recovers 483
   of 483 Tier-3 leaf paths and reproduces the printed digest. That is why nothing is broken; it
   is *not* why the escape is declined, because "recoverable by a heuristic" is not a contract
   and must never be quoted as one.

**What is owed instead, and is already in the tree.** The obligation a decline of this shape
carries is that nothing may quietly depend on parsing the rendering. The enforcement point
exists and is the one an operator actually meets: `--field-set` handed the Markdown twin used to
answer with a bare serde error and now names the cause and both remedies, and `--json-out` makes
the twin available from the *same* run rather than a second measurement of the same box (notes
§3.74; plan §18 item 43). The rule this entry adds to that: **anything in this repository that
needs to read a doctor report reads the JSON.** A future consumer that wants to parse the
Markdown is not a parser bug to fix — it is this decision being overturned, and AGENTS §5 makes
that a recorded decision naming new evidence.

**Overturned by**, if ever: a consumer that genuinely cannot be given the JSON. The sanctioned
shape is then a *new* rendering (a third twin) rather than a redefinition of the existing one,
so the frozen corpus keeps one grammar for its whole life.


### 15.58 The rate a port actually got is node state, not only a probe observation

**Status:** DECIDED — contract at §7.1; construction is plan §18 item 41. New design content, so
it is written here before the tree moves (AGENTS §5).
*[**Annotated 2026-08-21 (plan §18 item 108, §15.72) — the status line above is a dated record of
this entry's filing, not a live state.** The tree moved on **2026-08-15**: plan §18 item 41 is
**EXECUTED** (notes §3.107) and its entry states that this was "the design's only surface specified
ahead of the tree" and that "the two now agree completely". §7.1 clause 7 is settled system and
says so at its own site; §15.69 clause 2 later scoped what its ±2.5 % read-back can and cannot
verify. Nothing in the decision recorded below moves — only the "before the tree" tense does.]*

**The gap, measured rather than supposed.** P14's `adapter-refused` class is a driver accepting a
rate and landing somewhere else: on the committed 6.18 triple a 4000000 ask returns success with
`refusal_errno: null` and the port sits at `actual_baud_a: 9600`, `actual_baud_b: 9600`. The
doctor can see this because it asks — `requested_baud` beside `actual_baud_*` is P14's whole
instrument. **The daemon cannot**, and neither can an operator: a serial node configured
`baud = 4000000` on such an adapter reports `active` with the requested figure and nothing else,
and the first symptom is a link that does not carry bytes. §7.1's own doctrine is that whether a
driver honours a setting is *measured, never assumed* — stated for flow control (§15.53) and
enforced there by a pre-check — and the rate is the one termios parameter where the same class of
silent divergence is already proven to exist in the field.

**What §7.1 gains: reporting, and only reporting.** A serial node's state carries the rate the
port actually reports beside the rate that was asked for. No verdict, no fault, no refusal.

**Why not a refusal, given §15.53 refuses flow control.** Because the two are not the same fact,
and treating them alike would be policy without evidence:

- A dropped `CRTSCTS` silently removes a *safety property* — the link runs and loses data under
  exactly the conditions the mode was configured for. A rate that lands elsewhere breaks the link
  *loudly and immediately*: nothing round-trips, which is a symptom an operator meets in seconds.
- Rate divergence is **legitimate and common**. Every driver quantizes to what its clock divisor
  can express, so "asked 250000, got 250000" and "asked 3062500, got 3000000" are both ordinary,
  and only the operator knows which margin their device tolerates. There is no threshold this
  design could pick that would not refuse working configurations somewhere.
- §15.51 already names the discipline for exactly this reading: `platform-refused` is a fact about
  the ask surface and never about the wire. A node-state field is that same fact, reported at the
  node instead of at a probe.

So the rule is §13's reported-never-judged, moved from the doctor to node state: **name the
divergence, decide nothing.** An operator with the two numbers in `state` can act; a daemon
guessing a tolerance cannot.

**Where the reading comes from, and what it is not.** The termios read-back after the node's own
open — the same source `serial2` already uses to verify settings, so it costs no extra syscall
and no extra open. It is **not** a measurement of the wire: P14's `achieved_baud_floor` is that,
and it needs a cross-wired peer and a transmission. A driver that lies in its read-back is
invisible here exactly as it is to `serial2`, and saying so is part of the field's contract
(§16.13's discipline applied to a state field: a number carries what produced it).

**Absence is a value.** Where a platform cannot report the rate back, the field says so rather
than echoing the request — the §12 rule that an unknown is never rendered as an answer
(`has_identity_source`'s shape). Echoing the ask would make the field agree with itself on every
platform and assert nothing, which is plan §3 rule 22's tell in a state field.

**Validation** (item 41): fail-first against a refusing rate on the rig — the FT232R that P14
already measured accepting 3000000 and refusing above it is the fixture, so the guard has a real
divergence to see rather than a synthetic one.

### 15.59 The slave-witness liveness question becomes an instrument: P16

**Status:** DECIDED — roster and era rows at §13; construction is plan §18 item 26. Written before
the tree moves (AGENTS §5), which is the order item 26 was blocked on: a new probe id is derived
from *this* document by `meta_derive`'s roster gate, so landing P16 first would have made the tree
ahead of the design — the same defect as the reverse, and the reason the item stopped rather than
shipping a red gate.
*[**Annotated 2026-08-21 (plan §18 item 108, §15.72):** the tree moved on **2026-08-13** — plan §18
item 26 is **EXECUTED** (notes §3.90) and P16 ships with both arms serving as each other's control,
so the "written before the tree moves" tense above is a dated record of the order this entry
imposed rather than a live status. The ordering rule it states is the durable half and still binds:
a probe id is derived from *this* document by `meta_derive`'s roster gate, so the design entry has
to land first or the gate goes red.]*

**The gap, and why it is a probe rather than a comment.** The harness's `SlaveWitness::prove_open`
establishes that a pts slave is still open by comparing `(st_dev, st_ino, st_rdev)` against a fresh
`stat` of the path. That works on Linux because the kernel unlinks `/dev/pts/N` when the master
closes. On Darwin, whose devfs nodes persist, the same comparison is **expected to degrade and has
never been measured** — notes §3.56 leans on the behaviour, §3.60 names the doctor as its home, and
between them sits a load-bearing assumption with no artifact. §7's rule is the whole answer: when
two credible readings of kernel behaviour are possible, measure instead of assuming.

**What P16 asks.** `poll(POLLHUP)` on a held slave fd, twice: with the master **open**, where it
must stay quiet — that is the negative arm, and a probe without it would report a hangup that was
never absent — and after the master **closes**, where it must fire. Beside it, whether `stat(path)`
still resolves, so the two instruments are read side by side on the same kernel in the same run.
Two arms, both able to fire, no third-way verdict.

**Consequences, stated because they are the cost.** A new probe id moves `probe_set` and therefore
**closes the `e79f5fcd86a2e5f0` era** — the deliberate, recorded kind of era move (§15.44's law),
not a drifted one. That is also why P15's `question` string was *not* widened **at its own
landing**, when the probe gained a software-flow reading beside `CRTSCTS`: widening it would have
moved `probe_set` for a **wording** change, spending a second era boundary where one will do. The
two are folded together at P16's landing, in one commit — the widened `question`, the software
reading folded into P15's verdict, and P16's arrival — so the archive gains one boundary rather
than two.

### 15.61 The flow-control refusal covers both modes, because one driver drops both

**Status:** DECIDED — extends §15.53 (which is annotated, not rewritten); restated at §7.1's
flow-control clauses 1, 2 and 7. Construction is plan §18 item 67.

**This is a conditional decline being paid off, not a decline being reversed.** Plan §18 item 14
declined extending the refusal to `xon-xoff` in exactly these words: *"the refusal follows only if
a dropping driver is found — extending §15.53 to a mode nobody has measured would be policy
without evidence."* The condition named the evidence, and the evidence arrived: on Darwin 24.6.0,
Apple's `IOSerialFamily` on an FT232R accepts `IXON|IXOFF` (`tcsetattr` returns success with a null
error) and reads `c_iflag` back **`0x0` → `0x0`** — a delta of nothing — on both ports of the rig,
6 of 6 across three captures, with `serial2_readback_would_fault: true` (notes §3.93; the
`b346188` macOS triple). The same two adapters one kernel away read `0x5` → `0x1405` under
`ftdi_sio`, a delta of exactly the two flags. **So the driver that gave §15.53 its founding
measurement drops the software mode the same way it drops the hardware one**, and the mode §7.1
clause 7 called "unmeasured rather than known-good" is now measured and not good.

**The decision is the same decision, for the same reason.** `serial2` verifies `c_iflag` by
read-back exactly as it verifies `c_cflag`, so a node configured `flow_control = "xon-xoff"` on
such a port fails its own open with the bare `failed to apply some or all settings` — the late,
uninformative fault §15.53 exists to convert into an early, structural refusal. Nothing about the
argument is new; only the second mode's evidence was missing, and the entry that declined without
it was right to.

**What changes.**

1. **The predicate generalizes rather than being copied.** One implementation still answers for
   both modes — the two-copies-that-must-agree shape §16.5 bans is the failure mode here, and the
   three-way classification (`Honoured` / `Refused` / `AcceptedThenDropped`, plus `Err` for
   unmeasured) is already a pure function of two booleans and mode-independent. Only *which flag in
   which termios word* is set and read back differs, so that is the parameter. §7.1 clause 2's "one
   predicate, because two callers must not be able to disagree" is unchanged in force and widened
   in subject.
2. **Only `AcceptedThenDropped` refuses, for either mode.** An honest refusal stays honest (the
   node's own open fails loudly), `Err` stays *unmeasured* and never refuses, and an absent device
   still never reaches the check — §12's `waiting` node. The refusal names the mode it is about.
3. **A remedy that was wrong on the platform of record is corrected.** §7.1 clause 1 and the
   shipped refusal offered `flow_control = "none"` **(or `xon-xoff`)** as the remedy for a dropped
   `rts-cts`. On this rig the same driver drops both, so that parenthetical sent an operator from a
   refusal straight into a late fault — advice that is worst exactly where the refusal fires. The
   remedy is `flow_control = "none"`, or an adapter whose driver implements the mode.

**What does not change, stated because a widening is where scope creeps.** No new capability, no
new verb, no change to *when* the check runs (still before anything is created, §11), and no change
to the two paths that still reach `faulted` — a `--replace` on a port the running graph holds, and
an adapter arriving after load — whose repairs stay declined with the reasons §15.53 records. The
DTR-toggle cost §7.1 clause 6 states now applies to `xon-xoff` nodes too, which is a widening of a
recorded cost rather than a new one: the pre-check's open is the same open.

**The bound on the evidence, kept sharp.** One driver on two kernels is what is measured. This
refuses a mode a port *demonstrably* drops; it does not assert that any other driver drops it, and
a port that honours the mode is not refused — which is not a hope, because both arms are
reachable on one box: a Darwin **pts** honours `IXON|IXOFF` (`c_iflag` `0x2b02` → `0x2f02`) while
the FT232R beside it drops them. That pair is the discrimination proof, and it is what keeps this
from being a rule that refuses everything and calls it safety.

*[**Annotated 2026-08-21 (plan §18 item 85, §15.73), not rewritten:** "one predicate, parameterised"
bounds *what* this entry widened as well as *how*. Both modes are still judged on acceptance and
read-back, and neither is judged on the wire; §15.73 records why the wire question belongs to the
doctor rather than to the pre-check, and how much of that reading has been taken off hardware.
Clause 1's own widening obligation was then executed a second time, one mode over: the software
cell gained the `shipped_predicate_agrees` cross-check that the hardware cell had carried since
notes §3.67 (plan §18 item 73, notes §3.146, annotated at §7.1 clause 2). No clause above moves.]*

### 15.60 The Darwin pty buffer is a capacity, not a watermark — settled by a rung that already shipped

**Status:** LIVE — annotates §15.46's closing sentence; §15.49 is untouched and P10's status does
not move. The measurement is plan §18 item 19's Darwin half.

**The finding about the record comes first, because it is the transferable one.** Item 19's
pre-registered next step was "a rung below 128", carried in the ledger and in notes §3.52 as owed
work. It was **never owed**: the ladder is `[512, 1, 128, 900]` and the `1` rung — the strongest
member of that family, not merely one below 128 — shipped in `f8315cc`, *the same commit whose
notes pre-registered the step*. The pre-registration was stale on arrival and stayed in the ledger
for a generation, so a session scheduling item 19 would have written code to add an experiment the
binary was already running. The rule that follows is §13's instrument-validity rule pointed at the
ledger instead of at a probe: **before building a pre-registered instrument, check whether it is
already in the constant** — a pre-registration is a statement about a tree that has since moved.

**What the ladder settles.** On Darwin 24.6.0, both directions, all three runs of the `b346188`
triple, `topped_up_bytes` **equals** `drained_bytes` at every rung: 512→512, 1→1, 128→128,
900→900, and the from-empty rung republishes the whole depth (1024 targetward, 1022 hostward).
`rungs_refusing` is 0 and `watermark_threshold_le` is null. The `D = 1` rung is what makes this
decisive rather than suggestive, and the argument is one line: a watermark model republishing only
once occupancy falls below a threshold `T` predicts that draining a **single byte** from a full
queue republishes **nothing**, for every `T ≤ C − 1`. One byte was drained and one byte was
republished. So every watermark strictly below capacity is refuted, and the only survivor of that
family is `T = C`, which *is* the capacity reading. The reservation variant charged at the
empty→nonempty transition is refuted by the same rows from the other end: the from-empty rung
republishes the full depth with no shortfall. **This is what §15.46 called "a measurement, not a
decisive one", and it is now decisive** — for this kernel, on this hardware.

**What it does not settle, stated so nobody reads more off it.** (a) It bounds the *behaviour*, not
the *mechanism*: the two-queue XNU source read (a `TTYHOG`-guarded input bound beside a
`TTYCLSIZE`-sized output queue) remains a hypothesis under test, exactly as §7 and notes §3.42
have it — a capacity reading is consistent with it and does not establish it. (b) It says nothing
about Linux, where the ladder is bounded by its largest rung (900 against ~15360 recoverable) and
no watermark bound is recoverable at all — the two kernels are answered by different instruments
and the pair must not be diffed into a single sentence. (c) **One Linux observation is recorded
here rather than folded in, because it sits against a claim this record already carries:** the
current-era Linux triple's top-ups are uniform within runs 2 and 3 (9728 at every rung) but read
1536 at the first rung and 2560 at the rest in run 1, whose first-rung refill is also anomalous
(15872 against 13824). Plan §18 item 19's *Evidence* line and notes §3.53 (ii) both say the Linux
top-up is "measured drain-size independent". That is what runs 2 and 3 say; run 1 does not, and
the deviation tracks the **first rung** rather than the drain size, which is a warm-up shape and
not drain dependence. Recorded as an open observation for the next Linux session, **not** as a
falsification — one run of a quantity known to vary is not a refutation (P9/P10's own three-sample
rule), and calling it one here would be the mistake this document keeps cataloguing.

**No instrument moves.** The rung list is unchanged, `probe_set` and `field_set` are untouched, and
no era row is owed — which is precisely why this entry exists: the answer came from reading
committed artifacts rather than from taking a new measurement, and an answer nobody wrote down is
one the next session pays for again.

### 15.62 A transport that cannot report CTS, and the vocabulary that mistook it for a bare cable

**Status:** DECIDED — scopes §15.52's closing claim and §7.1's `modem_lines`/`actual_baud` clauses;
annotates neither away. Construction is plan §18 items 80–83.

**Scoped 2026-08-21 (§15.68):** the same two adapters were read on a second CDC-ACM stack
(Apple's, on Darwin 24.6.0). Three sentences below are narrower than they read. (a) The
`stuck-high` reading is this **driver's** constant, not the device class's — Apple's stack reads
the same wire **low**, and the low case is indistinguishable from a bare cable, so `UNREADABLE` is
not "the CDC-ACM answer". (b) Consequence 4's *"on **this transport**"* attributes to CDC-ACM the
termios behaviour of Linux `cdc_acm` specifically: **these same two devices are caught** on
Apple's CDC-ACM driver, which drops the flag and lets §15.53's refusal fire. The gap in
`FlowOutcome::classify` is not platform-scoped — one bench's *exposure* to it is. (c) Consequence
2's echo is not merely uninformative on Apple's stack: it conceals rates at which the link is
dead. §15.68 carries all three with their artifacts.

**The hardware.** Two WCH `1a86:55d3` adapters — `bDeviceClass=02`, two interfaces, bound to
`cdc_acm`, enumerating `/dev/ttyACM0` and `/dev/ttyACM1` — cabled by the operator as a **5-wire**
crossover and attached 2026-08-16. The adapter pair is new to the record and shares nothing with
`BH00L4KU`/`BH00LW9U`: chip, driver, device class, node name, cable and pair all moved at once, so
**no cell here may be diffed against a `ttyUSB` row** as though one variable had.
**The artifacts are committed** (§16.13): `docs/doctor/linux-7.0-2026-08-17-a7e6070-{tier3,tier3-2,
tier3-3,passive-1,passive-2,passive-3}.json` for the repaired build, and
`docs/doctor/linux-7.0-2026-08-16-8759516-{tier3,passive-1,passive-2,passive-3}.json` for the build
that carried the defect — kept deliberately, because the sentence this entry repairs is only legible
beside the reading that produced it. `jq -e -f expectations/linux.jq` exits 0 on all ten. Every
driver and kernel claim below is quotable from those, and from no scrollback.

**What was measured, before any of it was diagnosed.** Data crosses byte-exact in both directions,
so TX/RX are genuinely wired. But CTS, read through `TIOCMGET`, is **asserted in every state**: at
both drive levels of the peer's RTS, in every one of the eight drive states an independent probe put
it through (two owners x two lines x two levels), and *with the peer closed entirely*. It never once
read low. (Eight *drive states*, not eight P5 cells — P5 prints eight cells of which only two are
CTS, and both read `stuck-high`.) `CRTSCTS` is accepted and **persists in the `termios` read-back**, and does
nothing: a 2×2 control — peer RTS low/high × `CRTSCTS` on/off, peer never reading — wrote **44672
bytes in every one of the four cells, spread 0**. The first reading of that experiment was a
*stall*, and the stall was buffer backpressure from a peer that was not reading; the control is
what separated them. This is §6's rule in a new register: the symptom was reproducible and the
mechanism was elsewhere.

The mechanism, not the symptom (the discipline item 76 established) — and this half is a
**specification** fact, cited rather than measured, flagged so it is not read as another reading:
the CDC PSTN `SERIAL_STATE` notification carries `bRxCarrier`, `bTxCarrier`, `bBreak`,
`bRingSignal` and the error bits — and **no CTS field at all**. There is no wire-format in which a conforming CDC-ACM device *can* report
CTS, so the bit `TIOCMGET` returns is manufactured rather than read.

**The defect this exposed is ours, and it is a vocabulary defect.** `p5_handshake`'s `crosses()`
already returned five distinct strings — `true`, `false`, `stuck-high`, `inverted`, `?` — and the
shape fold tested only `== "true"`, putting the last three in the same bucket as a *measured*
`false`. So a bench whose cells read `rts_a_to_cts_b=stuck-high rts_b_to_cts_a=stuck-high` was
printed as **`3-wire: no handshake lines carried`** — a claim about the cable, from a reading that
says only *the instrument could not see it*, with the contradicting cells sitting in the same
string. The suite's `handshake_measured` had the same hole and a worse one: its comment promised an
operator would not have to translate between it and a doctor report (§15.52), while it carried two
cell words against P5's four, so it could not express `stuck-high` at all.

**The rule, stated once so it generalizes past this transport:** *a shape sentence may assert a
negative only about cells that answered.* Where a cell is `stuck-high`, `inverted` or `?`, the
sentence says the reading is not determinable and never that the line is bare. Both instruments now
carry `UNREADABLE handshake: RTS/CTS gave no usable reading at either drive level — this is not a
3-wire answer`, and `3-wire: no handshake lines carried` is kept byte-for-byte for the all-`false`
case, which is the design's stated common case and rides in committed artifacts.
*[**Superseded 2026-08-21 by §15.69 clause 1** (plan §18 item 92): the all-`false` case no longer prints that sentence — a bare FT232R input and a manufactured-low one were measured bit-identical, so the wording it preserved was preserving a cabling claim the cells cannot carry.]*

**§15.52's closing claim is scoped, not withdrawn.** It reads: *no gate in this tree can tell a
5-wire bench from a 3-wire one, by design — the only instruments that can are the committed P5
artifact and the two `rts-cts` end-to-end tests under `SNX_RIG_FLOW=required`.* True on `ftdi_sio`.
**False on `cdc_acm`, where neither can**, because both read CTS through `TIOCMGET` and that is the
path the driver synthesises. The correct reading of this bench is *not determinable by any
instrument here*, and `SNX_RIG_FLOW=required` must be dropped on it — which is the same action a
3-wire bench calls for, arrived at for a different reason and now said in different words. An
operator who reads "3-wire" and re-crimps a correct cable is the harm this entry exists to prevent.

**Four product-facing consequences follow. Three are narrowings of promises that were stated flat;
the fourth is a gap this entry files rather than closes.**

1. **§7.1's `modem_lines.cts` is transport-dependent.** The daemon reads it with the same
   `TIOCMGET` (`sys/src/lib.rs:166`), so on any `/dev/ttyACM*` node `state` reports `cts: true`
   unconditionally. The field is not wrong on FTDI and is not a reading anywhere on CDC-ACM. It is
   **reported, never judged** — no verdict moves on it — but a document that promises "current
   modem-line readings" without this scope is promising more than the kernel can supply.
2. **§7.1 clause 7's `actual_baud` example is `ftdi_sio`'s.** The corrected example there — a
   4 Mbaud ask *failing* on a 9600-capable adapter because `serial2` verifies `set_configuration`
   by ±2.5 % read-back — depends on a driver that reports back what it will really run. `cdc_acm`
   echoes the ask: measured `status="active" baud=4000000 actual_baud=4000000`, and P14 accepted
   **every rate it tried** — 15 of the ladder's 16 body rungs (the search stops at the first failing
   rung, so 12 Mbaud was never asked for) plus four refinements — **without one `adapter-refused`
   outcome anywhere**, ceiling `unreliable-timed-out` at 6 Mbaud. That ceiling is quoted from the
   capture whose search **completed**; the earlier one on the same bench hit its budget and its own
   verdict disclaims its ceiling as *interrupted*, so it is evidence for the absent refusals and not
   for the number. On such a transport `actual_baud` *is* the echo the suite's guard is named
   against, so that property is **unmeasurable there rather than violated** — a skip with its
   reading printed, keyed on the reading and never on a driver name.
3. **Break reception is confirmed on both drivers; the counter is not the same one.** `ftdi_sio`'s
   break-over-parity-over-framing precedence does not hold on `cdc_acm`, which reports a 250 ms
   break as **`frame` +1, `brk` +0**. §15.21's clause asks whether a break is *received*, and it is;
   the guard now names the counter it found and re-runs its idle-window control against that
   counter, so a driver with a free-running framing count cannot have noise chosen for it.
4. **§15.53's refusal does not protect an operator on this transport, and that is filed, not fixed**
   (plan §18 item 85). The refusal is built on a read-back: a driver that *accepts* `CRTSCTS` and
   then drops it from `c_cflag` is `AcceptedThenDropped` and is refused at `load`. `cdc_acm` does
   neither — it accepts the flag and **keeps** it, measured on both ports of this bench as
   `honoured_on_readback: true` with a `c_cflag` delta of exactly `0x80000000`, so
   `serial_nexus_sys::honours_rtscts` answers **`Honoured`** [**stale symbol, noted 2026-08-21**: §15.61 parameterised it to `honours_flow_control(path, FlowMode::RtsCts)`; the name in this sentence no longer exists in any `.rs` file — plan §18 item 96] and the daemon loads an `rts-cts` node
   without complaint. The 2×2 above says that flag does nothing. So this is a **third** state the
   predicate's two-valued world has no name for — *honoured on paper, inert on the wire* — and it is
   the §15.61 shape one transport over, with the polarity that matters reversed: §15.61's driver
   lied by dropping the flag, this one lies by keeping it. **No read-back can catch it**, because
   the read-back is exactly what the driver satisfies; separating them needs a peer and a transfer,
   which a load-time pre-check does not have. Recording it as a known limit is therefore the honest
   disposition, and closing it is a design decision about whether `flow_control` gains a
   *functionally verified* tier — not a patch.

**A cost, measured and partly repaired.** One P14 search on this pair ran **2089 s** against a
`P14_BUDGET` documented as "a hard wall-clock stop on the whole search" — of which only **26.9 s**
was trials. The remainder was inside the rate-apply: `serial2` configures with `TCSETSW`, the
*drain* spelling, so the call waits for queued output to leave at a rate the adapter accepted and
cannot achieve. Discarding the output buffer before the ask removes the wait, and **that is the
whole repair** — the same capture on the same bench, 4148 s to 86.6 s. Two honest caveats on that
pair: n=1 per side, and P14's *search* is only half of it (2089 s of the 4148 s), the rest being the
same drain in the baseline restore, which runs after `search_elapsed_ms` is stamped. The post-fix
run also did strictly **more** work — 19 rungs against 17 — so the ratio understates the repair
rather than flattering it. The constant's doc now states
the bound its placement can actually deliver, "the budget plus at most one rung", instead of the
stronger thing it claimed. A second budget check at the top of the search loop was written and then
**removed by this entry's own adversarial pass**: the pre-existing check at the loop's end already
guarantees no rung *begins* over budget, so the new one could never fire — passing behaviour
identical to not-running behaviour, which is the tell this very entry documents two fresh instances
of. Keeping it would have been the third.

**No era is owed.** `probe_set` reads `4317ea5ac187f506` on every capture from this bench: the
roster did not move, and §13's era law turns on the roster and not on the hardware. `field_set`
does move — adapter identities ride as observation *keys* — which is the ladder working as
described, not an era event.

### 15.63 The browser link is write-clocked: Nagle off, on the socket both tiers share

**Status:** DECIDED — one line in `web/src/server.rs`, its mirror in `web/src/wsclient.rs`, and the
guard that keeps them. Construction is plan §18 item 86.

**The reading.** An operator reported the console as sluggish: a second or more between clicking a
console in the rail and seeing it, and again between pressing Enter and the device answering. The
first thing the investigation established is that **the daemon is not slow.** Driven directly over
its control socket, on the same box and the same graph: `tap.open` 0.25 ms, `send` reply 0.19 ms,
and a `send`'s echo returning through a `pty --echo` device as `tap.data` in **0.45–0.75 ms**, six
of six. Everything below is therefore the web tier's, and none of it is the data plane's.

**What the browser saw instead.** The same round trip, instrumented inside a real Chromium by
timestamping every frame the page's own `WebSocket` sent and received: the `send` was answered
`{"delivered":true}` at **1.1 ms**, and the device's echo — which the daemon had already produced
and handed to the bridge — reached the page **43.4 ms** later. The two `lock` notifications the
verb emits arrived in the same lump, 41.4 ms after the answer that preceded them. That is not a
distribution; it is a constant, and the constant is `TCP_DELACK_MIN`.

**The mechanism.** The bridge's writer task drains one funnel into the WebSocket sink with one
`ws_sink.send(msg).await` per message, and tungstenite's `Sink::poll_flush` turns each of those into
one `write(2)`. So every browser-bound message is its own segment, and at console sizes every one of
them is sub-MSS. Nagle then holds message N+1 until message N is acknowledged — and the peer that
owes the acknowledgement is a browser sitting inside `await rpc(…)`, which by construction sends
nothing for the acknowledgement to ride on. The wait is therefore the browser's *delayed-ACK* timer,
and it is paid by every console interaction whose answer is more than one frame: a `tap.open` and
the replay behind it, a `send` and the device's echo, a graph edit and the `state` that follows it.

**Measured, on this kernel, both ways.** With Nagle on, after a single prior round trip has taken
the socket out of quickack and into pingpong mode: **40.1–42.0 ms**, 18 of 18, always exactly two
segments. With `TCP_NODELAY` set: **0.13 ms**. A firehose shape — twelve frames written 2 ms apart,
which is the daemon's own reader cadence — arrived as **one lump at 42.08 ms** with Nagle on and
spread 0.03–23.18 ms with it off, so a streaming console was ACK-clocked rather than write-clocked.
Through the browser, end to end, the echo gap named above went from **43.4 ms to 0.4 ms**.

**Why the option is set at accept and not in either tier.** The TLS acceptor wraps the *same*
`TcpStream`, so the accept loop is the one place `ws` and `wss` share; setting it inside the
plaintext arm would leave the sanctioned off-loopback mode still stalling. It is `let _ =`, matching
the precedent in `daemon/src/nodes/leg.rs`: a socket that refuses a latency hint is still a
perfectly serviceable socket, and dropping a healthy connection over one would be the worse trade.
`serial-nexus-web wsclient` makes its own outbound socket and had the mirror-image problem; it gets
the same line, unwrapped through `MaybeTlsStream::get_ref()` so one spelling covers both tiers.

**Loopback is not an exemption, and that is the part worth keeping.** The intuition that says
"Nagle cannot bite at microsecond RTT" is the reason this survived: it is *true on a fresh
connection*, where the socket is still in quickack mode — a twelve-frame burst there coalesces into
a single segment delivered in 0.18 ms. It stops being true after one round trip, which every real
console performs before it does anything interesting. **The guard therefore performs a round trip
before it measures, and that step is load-bearing rather than decorative: removed, the guard passes
on the unfixed tree** — measured, not reasoned — which is §3's *passing output identical to
not-running output* tell in its purest form. The guard states this at its own definition, because a
future simplification that deletes the warm-up as redundant would silently convert a real gate into
a vacuous one.

### 15.64 The console stops re-shipping itself: assets gain a validator

**Status:** DECIDED — `no-store` becomes `no-cache`, every asset gains a content `ETag`, and the
server learns to answer `304`. Construction is plan §18 item 87.

**What it was.** `write_asset` hard-coded `Cache-Control: no-store`, and `grep` over the whole
repository found that string in exactly one place and found `ETag`, `If-None-Match`,
`Last-Modified` and `304` in none. So there was no conditional-request path for a browser to take
even if it had tried, and `no-store` forbade it from keeping the bytes to revalidate. Every
navigation therefore re-shipped all nine served files — **116 876 bytes**, measured — in three
serial waves, since `index.html` names only `/app.css` and `/app.js` and the six `.mjs` specifiers
are not discoverable until `app.js`'s 52 981 bytes have landed and parsed. Each of the nine rode a
fresh TCP connection (`Connection: close`), hence a fresh congestion window.

**Why it matters more here than the arithmetic suggests.** The console has no auto-reconnect: every
`onclose` message ends in "reload to reconnect" and `connect()` is called exactly once. So a
daemon restart, a dropped socket, or a `scripts/bless` reinstall — routine on a workbench — routes
the operator into a full page load. Repeat loads are designed in, not incidental.

**What it is now.** Each asset carries a strong `ETag` over its own body, and a browser presenting a
matching `If-None-Match` gets a bodiless `304`. Measured through a real Chromium: a reload's
transfer went from **116 876 B to 2 627 B**, the whole remainder being `index.html`, which a reload
revalidates unconditionally.

**Three choices in the tag, each of which the obvious alternative gets wrong.** A **content** hash,
not a per-run nonce — a nonce invalidates every entry on every restart, and restart is exactly the
event this is meant to absorb. Not `CARGO_PKG_VERSION` either, which fails in the worse direction:
it does not move across the dev rebuilds that change these files most often, so a browser would
hold a stale console and the tag would swear it was current. And it is derived from a **table** row
rather than written per arm, so an asset added tomorrow gets a validator by construction — the
two-copies-that-must-agree shape §16.5 bans.

**`no-cache`, deliberately, and not a `max-age`.** `no-cache` means "store it, but ask before using
it": the bytes are saved and revalidated, which is where the whole 114 KB comes from. A freshness
lifetime would additionally save the *round trip*, and is refused: these URLs carry no content
hash, so any `max-age` is a window in which a reloaded page runs the previous build's JavaScript
against the current daemon — and §17 grew its provenance line precisely because a version mismatch
is otherwise invisible. Revalidation costs a round trip that `Connection: close` was already
spending; staleness would cost correctness. The 304 carries the CSP and the other security headers,
because a 304 refreshes the stored response's headers and a bare one would let a cached page run
without the policy the 200 that filled the cache carried.

**The guard's anti-tautology arm is the load-bearing half.** Asserting "a matching validator yields
304" is satisfied just as well by a server that answers 304 to *everything* — a far worse defect,
serving an empty console, and one that would share a passing test with the fix. So the guard also
presents a **wrong** validator and demands the bytes, and demands that two assets never share a
tag. All three arms were proved fail-first by planting the corresponding defect in place.

**Two things the adversarial pass changed, both of the same shape.** First, the comparison was
*strong*, while RFC 9110 §13.1.2 — the section the code's own comment cited as licence — requires
the **weak** comparison function for `If-None-Match`: a peer presenting `W/"x"` for a tag issued as
`"x"` must be answered `304`. It failed safe (a correct 200, and the pre-fix 116 876 B) and
**silently**, which is the half that mattered, since nothing could observe that revalidation had
stopped working for a class of peer. Our own tags are always strong, so stripping the request-side
prefix *is* the weak comparison and can never grant a 304 that strong comparison would have refused.
The guard gains a weak-form arm, proved fail-first.

Second, `the_validator_follows_the_bytes` exercised the hash function and not the **wiring**: a
validator derived from the request *path* would have passed it, and passed the injectivity test too,
since paths are distinct. The bodies are compile-time constants and cannot be mutated at runtime,
which is precisely why the wiring has to be pinned rather than inferred — the test now compares the
*served* tag against the hash of the *served body*. Measured: a planted path-derived validator
passes the injectivity test and reddens this one.

### 15.65 The rail is reconciled, not rebuilt: a click held across a snapshot was lost

**Status:** DECIDED — the left rail's rows are keyed and patched in place, and its click handler
moves to the container. Construction is plan §18 item 88.

**This is the defect the operator was actually reporting**, and neither of its two obvious
descriptions is right: nothing was slow, and nothing was waiting. Roughly half the operator's clicks
on a console name **never reached a handler at all.**

**The mechanism.** `renderConsoles` opened with `consolesEl.innerHTML = ""` and rebuilt every row,
attaching a fresh `onclick` to each new `<li>`. It is driven by `state`, and §10's snapshot is
published **every 200 ms unconditionally** — `emit_state_snapshot` returns early only when there is
no subscriber, and the bridge subscribes at connect — so on a graph where nothing whatsoever was
happening the rail was destroyed and rebuilt five times a second for the life of the page. A pointer
press spans `mousedown` and `mouseup`; the `click` event goes to the nearest common ancestor of the
two targets, and when the element under the press has been detached in between there is no row left
to receive it. The operator pressed the name, nothing happened, and they pressed again.

**Measured against the shipped page**, with a synthetic press held for a human's duration (20 trials
per row, alternating targets, counting presses after which the pane title never changed):

| press dwell | clicks lost, before | after |
|---|---|---|
| 0 ms | 0/20 | 0/20 |
| 30 ms | 1/20 | 0/20 |
| 60 ms | 9/20 | 0/20 |
| 100 ms | 10/20 | 0/20 |
| 150 ms | 16/20 | 0/20 |

and 0/40 at each of 60/150/300/500 ms after the fix. The median latency of a click that *did* land
was 9–22 ms throughout, before and after: **the code on the selection path was never the subject.**

**Why no gate caught it, which is the part worth generalising.** `ui-tests/fixture.mjs` selects with
Playwright's `.click()`, which presses and releases within one tick — 0 ms of dwell, the one row of
the table where the defect does not appear — *and* auto-retries when an element goes detached. The
harness was forgiving in exactly the two ways a mouse is not. This is a new register of §3's
recurring tell: not a gate whose subject never ran, nor one whose assertion was weaker than its
comment, but a gate whose **stimulus was gentler than the product's real one**. The guard therefore
presses with `mouse.down` / hold / `mouse.up` and holds for 400 ms — two snapshots at 5 Hz, so it
cannot pass by winning a race.

**The construction.** Rows are keyed by display address and patched: an `<li>` for a console that
still exists is the same `<li>` it was a tick ago, fields are written only when they differ, and
badges are created and removed rather than re-created. A rail whose contents did not change performs
**zero** DOM mutations. The click handler moves to the container as one delegated listener — belt
and braces rather than the fix itself, since a reconciled row is never detached, but a listener on
the container cannot be lost to any future rebuild of its children, and it is one listener instead
of one per console.

**One defect keying the rows introduced, found by the adversarial pass and fixed.** The badges are
created lazily, and both were appended — so a row that gained waiters while the lock was free and
*then* gained a holder carried them in the opposite order from a row built holder-first, which the
rebuild-per-tick version could not produce. The free-but-queued state is a real wire state
(`release` nulls the holder and deliberately keeps the queue), so a 5 Hz snapshot can observe it.
The lock badge is now anchored ahead of the waiter count; `insertBefore(x, null)` appends, so the
common case needs no branch. Transient and cosmetic — the transposition un-does itself when waiters
next reach zero — but it is exactly the class of drift that reconciliation trades for, and it is
cheaper to remove than to remember.

**A second guard pins the mechanism rather than the outcome** (AGENTS §3). "The click landed" is an
outcome a later implementation could reproduce by rebuilding the rail and papering over it with a
retry, at which point the guard would hold the defect in place. So the row's DOM element is stamped
and the stamp is read a second later: a rebuilt row cannot carry a property set on its predecessor.
Both guards were proved fail-first by restoring the destroy-and-rebuild in place.

### 15.66 The steal affordance is a preference, not a modal

**Status:** DECIDED — **amends §17 clause 3**, which is annotated at its own site rather than
rewritten. Construction is plan §18 item 89. Asked for by the operator.

**What it was.** A refused line raised `confirm("<endpoint> is locked by <holder>. Steal the lock
and send?")`. The clause it implemented — "an explicit steal affordance — never an automatic steal"
— was right about the *policy* and wrong about the *shape*, in three ways.

1. **It asks at the worst moment.** A lock conflict is discovered only after the operator has typed
   a line and pressed Enter. The dialog therefore lands on top of the console, over the output the
   operator is presumably reading, and must be dismissed before anything else can happen.
2. **It asks the same question every time**, of an operator whose answer is a property of how they
   work — one bench, one person, one device — and not of this particular line. A question with a
   stable answer belongs in a setting.
3. **`confirm()` blocks the renderer**, and this client has had to be designed around that twice
   already: 37-WEBC-1 and HISTC-2 are both races that exist *because* a dialog can park a
   continuation for as long as a human takes to read it. The clear button's synchronous-before-the-
   delete ordering is a scar from exactly this. Adding no new instances of a hazard the code
   already carries two documented workarounds for is worth something on its own.

**What it is.** A checkbox beside the send box, labelled *automatically steal during line write*,
**checked by default**, with the full sentence in its `title`. Its default is the `checked`
attribute in the markup rather than an assignment in script, so it holds from the first paint and a
reader of the HTML can see what the console does.

**What is deliberately preserved, because it is the half that mattered.**

- **The first attempt never carries `steal`.** It costs one round trip on a contended endpoint and
  nothing on an uncontended one, and it is what obtains the holder's name: `data.held_by` arrives
  *on the refusal*. An operator who leaves the box ticked is still owed the sentence naming whose
  lock was taken, and that sentence is only purchasable by being refused once.
- **The steal is announced**, in the terminal, before the retry — `— stealing the write lock from
  <holder> —`. A steal displaces another origin's floor (§6); §5's honesty is that the operator
  watches that happen rather than inferring it from a line that went through. Unticked, the same
  refusal is reported with the holder named and the remedy stated, and the line goes back in the
  box.
- **It stays confined to `-32003`.** Every other refusal is a different failure and still gets the
  daemon's own words (WEBUI-1). Widening this to any other code would re-create the defect that
  finding closed.

**What is given up, stated plainly:** per-line confirmation. An operator who leaves the default in
place will steal a lock without being asked at the moment it happens. That is the operator's
setting, visible on screen while they type, and §17 has always held that web access *is* operator
access — the same session may already `add-node` an exec codec. The other two confirmations on this
page — clearing stored scrollback, and a cascading `remove-node` — are **not** touched: both destroy
something that cannot be recovered by re-typing it, which is the distinction that makes a modal the
right shape there and the wrong one here.

**No holder, no steal — a rule about the verb, added by the adversarial pass.** `-32003` is the
daemon's answer to *three* situations: the floor is held by someone else, the target would not
accept the write inside the deadline, and the endpoint was torn down mid-send. Only the first has
anything for a steal to take. With the box ticked, the other two would have been retried identically
and — worse — had the daemon's own words replaced by a lock narration that was never true, which is
WEBUI-1 exactly: the console telling the operator a false story about why the line did not land. So
the preference is consulted only where a holder is actually *named*; otherwise the refusal is
reported in the daemon's words, like every other refusal. The `title` also moved from the wrapping
`<label>` to the control, since it is the input's accessible description that a screen reader reads.

**The guard's fourth arm is the one that keeps this honest.** Asserting the two positions of the
checkbox would be satisfied by a console that also opened a dialog, so the spec registers a `dialog`
handler for the whole test and demands it never fired.

### 15.67 The terminal paints on a frame boundary, not on a notification's arrival

**Status:** DECIDED — arrivals are queued and drained once per animation frame. Construction is
plan §18 item 90.

**The cost was per notification, not per byte, and that is the whole finding.** `writeTerminal` ran
to completion inside the `tap.data` handler, and it brackets its DOM work with a **read** of the
pane's scroll metrics — which forces the browser to lay out a `<pre>` holding up to `TERM_CHAR_CAP`
characters, synchronously, before the read can answer. Whether the notification carried 40 bytes or
5 000, it paid that layout once. Measured against a serial node delivering **49 KiB/s** — an
ordinary 460800-baud device, not a stress case — the shipped client spent **78 % of the main
thread** on ten notifications a second at ~120 ms each: **23 µs per byte**, which no amount of text
processing accounts for.

Work arriving faster than it drains does not degrade gracefully; the event-loop backlog grows
without bound. Main-thread latency reached **1.1–2.6 s** and one rail selection took **15.4 s**. The
console had stopped being merely behind its device and started failing to answer its operator —
which is the same symptom §15.65 produces by a completely different mechanism, and one reason this
investigation had to measure rather than reason.

**Before and after, same box, same device, same 15-second window:**

| | before | after |
|---|---|---|
| absorbed | 49.0 KiB/s | **110.9 KiB/s** |
| main thread busy | 78 % | **36 %** |
| main-thread latency while streaming | 1145–2591 ms | **2–112 ms** |
| rail selection while streaming | 15 383 ms | **48 ms** |

and on the unpaced 64 MiB firehose, which is the worst case this fixture can produce: selection
**17 952 ms → 79 ms**, long tasks **300 totalling 32.6 s → 41 totalling 4.8 s**. The client is no
longer the bottleneck in either case; throughput more than doubled as a side effect of not paying a
layout per notification.

**Why concatenating is not merely cheaper but *more* correct.** `parseAnsi` is a resumable state
machine over a character stream, so parsing `a + b` is identical to parsing `a` then `b` — and an
escape sequence split across two notifications, which §17 already calls the ordinary case at serial
rates, no longer crosses a call boundary at all. Markers keep their position in the queue, so a gap
annotation still lands exactly where the gap was. The batch pays **one** layout read and one scroll
write whatever it contains.

**Three things the queue must not do, each handled rather than hoped at.** It must not stall in a
hidden tab — `requestAnimationFrame` does not fire there, so a timer is armed alongside it and
whichever runs first performs the flush. It must not grow without bound while visible — a flush
drains everything queued rather than a fixed slice, so the queue holds one frame's arrivals. And it
must not paint the *previous* console's bytes into the pane of the one just selected —
`resetTerminal` discards the queue, which is safe because the bytes are in `history` and the OPFS
record, and the screen is what the caller is deliberately clearing.

**Batching widened one bound, and the adversarial pass caught it.** `commitNode` enforces the
rendered-scrollback cap by dropping whole committed nodes and never drops the last one — a cap
smaller than one node has to shrink the screen, not blank it. Before batching a node was one
notification's output, single-digit KB, so that exception was invisible; afterwards a node is one
*flush*'s output, which under a backlogged main thread is however much arrived while the previous
flush ran. The bound would quietly have become "the cap, or one flush, whichever is larger". A
flush's committed output is therefore split at `TERM_BLOCK_CAP` (32 KiB), which costs a handful of
extra nodes and restores the bound to `TERM_CHAR_CAP + TERM_BLOCK_CAP`.

**The guard's own first draft was vacuous, and finding that out is the recorded lesson.** The
property is *where* the painting happens, so the spec brackets the client's `onmessage` handler with
two listeners — one registered at construction (first) and one from a microtask (last) — and asserts
the terminal did not grow between them. The obvious spelling instead recorded `before` in one
listener and `after` in a `queueMicrotask`, which **passes on the unfixed tree**: a microtask
checkpoint runs whenever the JS stack empties, which is *between* event listeners, so the reading
was taken before the client's handler had run at all. Measured: the synchronous build reported
`before === after`, six of six. That is a third register of §3's tell — an observation taken at the
wrong *point in the task* rather than of the wrong thing — and both spellings were run against both
trees before one was kept.

### 15.68 The same two adapters on a second CDC stack: what that scoped, and two transport defects the first stack hid

**Status:** DECIDED — scopes §15.62's generalization and its consequence 4; annotates neither away.
Files plan §18 items 92–96. **No product code changes on this finding**: both defects are in the
platform, and the tree's own byte-exactness guard already reddens on one of them.

**The experiment §15.62 could not run.** §15.62 was measured on Linux `cdc_acm` with two WCH
`1a86:55d3` adapters, serials `5A7C298854` and `5A7C297954`. **Those same two adapters, on the same
cable, were attached to the x86_64 Mac rig box on 2026-08-20** and enumerate as
`/dev/cu.usbmodem5A7C2988541` / `/dev/cu.usbmodem5A7C2979541` on
`AppleUSBCDCCompositeDevice` → `AppleUSBACMData` → `IOSerialBSDClient`. Hardware and cable are held
fixed; the kernel and the driver move. That is the one variable §15.62 had to hold, and it is
exactly the variable that separates its **specification** claims from its **driver** claims.
**The artifacts are committed** (§16.13): `docs/doctor/macos-24.6.0-2026-08-21-3a39896-{tier3,
tier3-2,tier3-3,passive-1,passive-2,passive-3}.json`. `jq -e -f expectations/macos.jq` exits 0 on
all six, and the three Tier 3 captures agree byte-for-byte on every cell quoted here.

**1. The CTS bit is manufactured, and *which constant* is stack-dependent.** On Linux these
adapters read `stuck-high`; on Darwin they read low at both drive levels of the peer's RTS and
with the peer closed entirely. Two independent CDC-ACM stacks, one wire, opposite constants —
which is a stronger argument for §15.62's specification claim than §15.62 could make, because it
does not depend on reading the CDC PSTN table correctly. If CTS were carried on the wire, both
stacks would read it and agree.

**And the low case is the dangerous one, because it is indistinguishable from a bare cable.**
`crosses()` returns `false` for `(false, false)` and the shape fold calls that *measured absent*,
so P5 prints **`3-wire: no handshake lines carried`** — a claim about the cable — and
`handshake_measured` prints the same. **This is not the defect §15.62 repaired, and saying so
precisely matters:** that repair covered `stuck-high`, `inverted` and `?`, all three of which are
*self-refuting* readings (a line that stays high with the peer closed is doing something no wire
can do, so the cell announces its own failure). A constant low announces nothing — it is exactly
what an absent conductor looks like. **The gap is observability, not coverage**, and nothing
available to this instrument closes it: three explanations are byte-identical in every artifact —
the cable lacks RTS/CTS, the WCH module does not bond those pins to the header, or the stack never
surfaces CTS. **The first of the three was eliminated later the same day by moving the cable; read
on before quoting this sentence.**

**No gate reddens on this, and that is part of the finding.** Neither `expectations/linux.jq` nor
`expectations/macos.jq` pins the shape word, and `itest/tests/expectation_gates.rs:1288-1300`
*plants* the all-`false` sentence and asserts the gate must accept it. The harm is to a human
reader and to a skip reason — `crossover_rig_rts_crosses_to_the_far_ports_cts` skipped on this
bench citing `3-wire: no RTS/CTS handshake in either direction` — never to CI.

**The cable was then measured, and the sentence is wrong about its own subject.** This clause was
first written to claim only undecidability — no instrument in this tree had ever measured RTS/CTS
continuity on this cable, so "5-wire" was operator testimony and convicting a report of an
unfounded cable claim by means of a cable claim resting on testimony would have been §15.62's own
error run backwards. **The operator then moved that cable to the FTDI fixture (`BH00L4KU` ↔
`BH00LW9U`) and the question became decidable.** Both instruments answer the same way, 3 of 3
captures: P5 prints **`5-wire crossover: RTS/CTS both ways, DTR moves nothing`** with
`rts_a_to_cts_b=true rts_b_to_cts_a=true`, and an independent `TIOCMGET` probe reads the far CTS
following the near RTS at **both** drive levels in **both** directions, dropping low when the peer
closes. Artifacts: `docs/doctor/macos-24.6.0-2026-08-21-3a39896-ftdi5w-tier3{,-2,-3}.json`,
`jq -e -f expectations/macos.jq` exit 0 on all three.

**So the cable is a measured 5-wire crossover, and the CDC-ACM bench printed `3-wire: no handshake
lines carried` about it.** §15.62's stated harm — *"An operator who reads '3-wire' and re-crimps a
correct cable"* — is therefore **demonstrated rather than hypothesized**, and plan §18 item 92
carries a measured motivating case.

**What the FTDI reading still does not decide, stated because it is the obvious over-read.** It
eliminates only the first of clause 1's three explanations. Whether the CDC-ACM bench failed
because the WCH module does not bond RTS/CTS to its header, or because Apple's stack never
surfaces CTS, remains undecided and no instrument here separates them. **It does not need to be
decided for the defect to stand:** P5's sentence is a claim about cabling, the cabling is correct,
and the operator harm follows from that alone. Cable identity across the move is operator
testimony — the adapters are verified changed by serial number, the cable is not verifiable by
instrument — and re-seating a connector can change continuity in either direction.

**And the same move is the control that scopes clause 3** — see there.

**The positive control that makes the transport scoping safe:** on this same Darwin 24.6.0 box,
FTDI `/dev/cu.*` nodes read `rts_a_to_cts_b=true rts_b_to_cts_a=true` and print `5-wire crossover`
(`docs/doctor/macos-24.6.0-2026-08-05-42eac2a-tier3.json`), and a true-negative control exists too
(`macos-24.6.0-2026-08-13-b346188-tier3.json` reads all-`false` for a pair that `ftdi_sio` also
reads all-`false`, so that cable really is 3-wire). **Darwin's CTS path works.** The low reading is
a property of this transport, not of this kernel.

**2. `UNREADABLE` is not "the CDC-ACM answer", and the generalization that said so is retired.**
AGENTS §3 carried *"`UNREADABLE` is the CDC-ACM answer and it is not a cabling statement … Do not
re-crimp a bench on that reading."* On Darwin the same device class and the same two adapters
answer **`3-wire`**, which *is* a cabling statement, so the operator guidance was inverted on the
platform it most needed to hold for. The corrected form, stated once so it survives the next
stack: **the CDC-ACM CTS bit is manufactured; which constant it is manufactured as is
stack-dependent — Linux `cdc_acm` high, Apple's CDC-ACM low — and only the high case is legible as
an instrument failure.** A bench that reads all-`false` on a CDC-ACM transport is *undetermined*,
and no reading in this tree resolves it.

**3. A byte-exact transfer is not a whole transfer: this stack loses and duplicates in equal
measure.** Writing 1024 bytes port-to-port and reading them back delivers **1024 bytes**, of which
**8 of 128 position-tagged records are missing and 8 are delivered twice** — 5 of 5 runs at 115200,
byte count exact every time, alignment intact every time. The effect appears in **54 of 54** trials
across both directions and 9600 / 115200 / 921600, and **pacing the writes ~20 ms apart eliminates
it** (0 faults over 3 reps, against 12 for one burst write and 20 for unpaced 64-byte chunks), which
localizes it to concurrent in-flight transfers rather than the wire, the cable, the device or the
rate. Whether the displacement happens on the transmit or the receive side is **not established**.

**The topology, the harness and the platform are all excluded by a same-box control.** When the
operator moved this cable to the FTDI fixture, the identical program at the identical payload and
rate read **128 of 128 distinct records, 0 lost, 0 duplicated, 5 of 5 runs** — same machine, same
USB hubs, same cable, same 1024-byte payload, same 115200. So this is not USB topology, not the
reader being descheduled, not the measuring program, and not Darwin generally. **It is this
transport**, and on the two device classes now measured here Apple's CDC-ACM path has it and
Apple's FTDI path does not.

**The loss is invisible to the tree's own loss fingerprint.** Plan §3's fingerprint is
`received + dropped_slow_consumer == sent`, and this defect preserves the byte count exactly — so
the fingerprint balances while an eighth of a 1024-byte payload is wrong. Only a content
comparison sees it, which is why the three guards that redden are the SHA-256 ones.

**Why this entry states it as loss-and-duplication and not as reordering:** the session's first
instrument counted *parsed* records rather than *distinct* ones and therefore reported "every byte
delivered, nothing lost". It was a metric asserting something weaker than its name claimed — §3's
tell, in the measuring apparatus rather than in the product — and it inverted the severity of the
finding. The corrected instrument counts distinct coverage, losses and duplicates separately.

**The tree already catches this, which is the one reassuring sentence here.** `inject_verify`
compares a SHA-256 of the sent stream against the capture and fails with *"bytes lost/reordered
across the wire"*; on the rig lane three guards redden on this bench —
`crossover_rig_data_plane_send_and_exclusivity`, `crossover_rig_custom_baud_byte_exact` and
`exclusive_write_lock_is_byte_exact` — end to end through the daemon. A length check would have
passed all three.

**4. A rate this stack cannot realize is accepted, echoed back, and then transmits nothing.**
With the payload held constant at 240 bytes so that rate is the only variable: 9600, 14400, 19200,
38400, 57600 and 115200 are byte-exact; **15000, 15600, 16800 and 20000 deliver 0 or 1 byte of
240**. `IOSSIOSPEED` returns success at every one of them and the `tcgetattr` read-back **echoes
the requested rate**, so nothing anywhere in the stack reports a problem — the operator gets
`status="active"`, an `actual_baud` equal to the ask, and a link that is completely dead.

**Scoped to the four rates measured, because a fifth contradicts the tempting generalization:**
this is *not* "non-standard rates are dead here". P5's own ladder round-trips the non-standard
`CUSTOM_BAUD = 250_000` on this same pair in the same captures (`rate_ladder=true`). Four rates
were asked and four carried nothing; what selects them is unknown.

**The product-facing half, which is what makes this more than a probe curiosity:** `serial2` sets
baud on macOS through this same `IOSSIOSPEED` path, so a serial node configured at 15000 on this
box **sets successfully, satisfies §7.1 clause 7's ±2.5 % read-back verification against an exact
echo, comes up `status="active" actual_baud=15000`, and carries nothing.** That is the
accept-echo-do-not-perform class of plan §18 item 85, one layer above the flow-control flag, and
it defeats the read-back that §15.58 built the clause on. **The suite asked for exactly this
measurement**: `crossover_rig_actual_baud_is_a_read_back_not_an_echo` skips on this bench saying
*"which rungs it refuses there is not measured — take it with `serial-nexus-doctor --port A
--port B` (P14) and file the Darwin arm rather than assuming this one carries over"*. This is that
arm.

**This sharpens §15.62 consequence 2 rather than contradicting it.** That clause records the echo
and calls the read-back property *unmeasurable* on this transport rather than violated. True; and
the echo is not merely uninformative here, it is **actively concealing**. **What is not
established is the axis:** the Linux ladder never asked 15000, 15600 or 16800 — it steps
9600 → 19200 → 38400 — and its 6 000 000 rung passed byte-exact, so whether these rates are dead
on the *device* or only on this *stack* is untested. That is a pre-registration for the next Linux
session, not a finding.

**5. It also explains P14's number, and the explanation is not the one first proposed.** P14
reported `max_reliable_baud = 14400` on hardware that reached 6 000 000 on Linux. The tempting
account — that P14's constant-airtime payload (`baud/40`) crosses the displacement threshold — is
**refuted by P14's own table**: 14400's payload is 360 bytes, above the 352-byte offset where
displacement first appears, and that rung passed **6 of 6 byte-exact in both directions**. Payload
and rate are perfectly collinear inside P14, so its table cannot separate them in either
direction. What actually happened is both defects at once: clause 3 knocked out the 19200 rung
(480 bytes received, `failure: "corrupt"`), refinement then descended into 15000/15600/16800, and
clause 4 made those silent — so `ceiling_kind = "unreliable-timed-out"` is read off a rung that
received **zero** bytes — which makes it the *correct* word for a genuine stall, and evidence
**against** a payload account rather than for one, since the displacement defect classifies as
`Corrupt` and not `TimedOut`. **`max_reliable_baud` on this bench is not this adapter's maximum rate**,
and no wording in P14 could say so.

**6. The instrument now says which of those two happened, and that is the repair this entry
earned.** P14 *detected* both defects with the payload it already sends and could name neither: one
word, `corrupt`, for a count-preserving displacement, and one word, `timed-out`, for both a silent
link and a transmit-side stall. The evidence was in every committed report and the fold spent it.
Each failing direction now carries a `failure_detail` object naming the shape —
`short-write` / `silent` / `starved` / `displaced` / `interleaved` / `garbled` — with
`lost_bytes`, `duplicated_bytes`, `first_defect_offset`, `matched_bytes`, `unaligned_bytes`,
`read_window_saturated` and `expected_windows_unique`.

**It needed no change to what P14 sends**, which is the finding underneath the repair: `p14_payload`'s
LCG tail is **8-gram-unique at every ladder length** — 0 duplicate 8-byte windows at 240, 480, 2880
and 65536 bytes — so the expected byte at any offset is already computable and every displacement is
already localizable. **A battery of constant patterns would have been a step backwards** on exactly
this axis: all-zero and all-ones are blind to the whole insert/delete/reorder class under
`contains_sub`, and `0x55`/`0xAA` is blind to every even-length member, which is every displacement
size measured on this bench. The stimulus half of that idea is real and is filed as plan §18 item 98,
Linux-gated because its readout is `TIOCGICOUNT`. *[**Superseded 2026-08-21 by §15.69 clause 3**
(plan §18 item 108, §15.72): item 98 ran on the FT232R bench and is **closed as a measured
decline**. The inlay separates from the LCG on nothing that bench can read, and the null counts as
evidence only because `frame`, `brk`, `overrun` and `parity` were each moved by a positive control
in the same session. P14's payload does not move. Read "is filed" above as the filing it was.]*

**Three properties are preserved deliberately, and each is asserted.** `RungOutcome`'s six words are
untouched — the stall arms still fold to `TimedOut` and the displacement arms to `Corrupt` — so
`ceiling_kind`, the ladder fold and every committed verdict are unmoved. The cost is on the failure
path only, so a healthy rig pays nothing. And the vocabulary is closed **by the type** rather than by
a sentence: plan §18 item 84 is the case against the other spelling, where a class table's closure
was a comment enforced as *each word appears at least once* and a fourth word passed.

**`probe_set` does not move** — `P14_QUESTION` already asks *"and what stopped the search"*, so a
better stop-reason is an answer rather than a new question — and **`field_set` moves once**, which
closes no era (§13's era law clause 4). The FT232R bench reads the difference immediately: its
ceiling rung printed `timed-out` and now prints `starved`, `written: 37500`, `received: 37482` — a
receive-side stall eighteen bytes short of a completed write, which is a different sentence from the
one the CDC-ACM bench's `silent` rungs earn.
### 15.69 The FT232R bench answers three of the Darwin session's open questions, and refuses one

**Status:** DECIDED — overturns one decline at §15.62/item 92 by measurement, adds one wire-rate
finding that scopes §7.1 clause 7 on **both** kernels, and records one experiment whose accepted
half was measured to buy nothing. Notes §3.121 (clause 1), §3.122 (clause 2), §3.123 (clause 3). Closes plan §18 items 92, 93, 96 and 98; items 94 and 95 are
record repairs and carry no design consequence. **One product surface changes** — the handshake
shape sentence, clause 1 — and it changes a word, never a classification.

**The bench.** The FT232R pair `BH00L4KU` ↔ `BH00LW9U`, the fixture the Darwin session moved its
cable onto (notes §3.118), read on Linux 7.0.0-30 at `432aa0c`. P5 measures
`5-wire crossover: RTS/CTS both ways, DTR moves nothing`
(`rts_a_to_cts_b=true rts_b_to_cts_a=true`, all six DTR crossings `false`), P14's ceiling is
`3000000` `adapter-refused`, and `icounts_measurable=true`. So the same two adapters and the same
cable have now been read on Darwin and on Linux within one day, and **the wiring is not a variable
in anything below**. Artifacts are committed (§16.13).

**1. An all-`false` handshake stops asserting a cabling fact, because a bare input and a
manufactured-low input are bit-identical — and that is measured, not argued.** §15.62 keyed its
repair on the *reading*: `stuck-high`, `inverted` and `?` fold to `Inconclusive` because a line
that stays high with its peer closed is doing something no wire can do, so the cell announces its
own failure. **A constant low announces nothing**, and plan §18 item 92 filed the consequence —
Apple's CDC-ACM stack manufactures CTS low, which is bit-identical to an absent conductor, so the
fold fell through to `3-wire: no handshake lines carried` about a cable three instruments had
measured as a true crossover.

Item 92 left three candidates and **declined deciding between them on one session's evidence**:
(a) key on transport capability, which §15.62 explicitly refused; (b) require a positive control —
a state in which this port's CTS has ever read high — before licensing a cabling negative; (c)
weaken the all-`false` sentence for every bench. **The decline is overturned here, and what
overturns it is that (b) is (c) wearing another name.** Two readings show it:

* `docs/doctor/linux-7.0-2026-08-13-8c00078-dirty-tier3{,-2,-3}.json` is a genuine **3-wire FT232R**
  bench on this kernel, and it reads **all eight cells `false`, CTS included**. A bare FT232R CTS
  input reads constant low.
* This session's own capture is a **5-wire** bench, so its DSR/DCD/RI pins are bare — six
  unconnected modem inputs — and every one of them reads `false` at both peer drive levels.

So a positive control asking *has this port's CTS ever read high* fails on every legitimate 3-wire
bench exactly as it fails on a synthesising transport. It licenses the legacy sentence **nowhere**,
which makes it (c) with extra machinery. The repair is therefore (c), and the cost §15.62 named —
the byte-for-byte wording of the design's stated common case — is paid on measured grounds rather
than on preference.

**What changes is one word and nothing else.** The all-`false` arms of `p5_handshake_line` and of
the suite's `handshake_measured` now read *no handshake crossing read: 3-wire, or a transport that
manufactures these lines — this reading cannot separate them*, with the eight-cell suffix
unchanged. `CellReading`, `crosses()`, `cell_word` and the other five shape arms are untouched; a
3-wire bench lands in the same arm, skips the same tests, and hard-fails under
`SNX_RIG_FLOW=required` exactly as before (§15.52 stands). **`CellReading::Absent`'s doc comment was
the defect in one line** — it read *measured absent, and sayable as such* — and it is corrected at
the variant.

***The sentence was the easy half, and this is the clause's real content.*** The first repair
changed both diagnostics and left `skip_no_rig_flow`'s hard-fail message ending *"Cross-wire RTS↔CTS
both ways — a half-crossed bench … is a miswiring, not a 3-wire rig"*. So under
`SNX_RIG_FLOW=required` the tool pasted the newly hedged reading into a message that then **told the
operator to re-cable a bench it had just said it could not judge**, in the two-state world this entry
refutes. **The harm survived the repair, in the one sentence an operator acts on.** The rule this
entry binds is therefore stated on the imperative and not on the diagnostic: *a re-cabling
instruction may be issued only on a reading that identifies a miswiring.* A HALF-CROSSED reading
does; an all-`false` or `UNREADABLE` one does not, and no longer carries one. (Notes §3.121 carries
the three further defects the same adversarial pass found, including a swap of the two half-crossed
direction strings that no guard caught.)

**What this does not decide, stated because it is the obvious over-read:** nothing here says the
CDC-ACM benches are correctly cabled or incorrectly cabled. It says the instrument cannot tell, and
now says so. **Do not re-crimp a bench on this sentence** — that was the harm.

**2. A rate can be accepted, echoed back exactly, and put the wire a megabaud away — on Linux, on
FTDI, and by 41 %.** Plan §18 item 93 recorded four rates that Apple's CDC-ACM stack accepted,
echoed and could not carry, and asked for the Linux arm. It was run at 240 bytes held constant
across every rate, the payload byte-identical throughout so rate is the only variable.

*The four Darwin-dead rates are alive here.* 15000, 15600, 16800 and 20000 are byte-exact 3 of 3 in
both directions, as are 9600, 14400, 19200, 38400, 57600, 115200, 230400, 250000, 460800, 921600.
**No Darwin-versus-Linux contrast may be drawn from that** and the pre-registration forbade it in
advance: the Darwin cell moved device *and* stack, this one moves neither back, and the fourth cell
of that 2×2 — WCH on Linux — needs hardware this bench does not have. Item 93's remainder (b) is
answered for this cell and stays open as a comparison.

*The finding is at the top of the range instead.* `set_baud_rate(2_823_529)` succeeds, reads back
`2823529`, and puts the wire at the rate a peer asked for `2000000` runs at — proved by byte-exact
round-trips 3 of 3 in both directions against that peer, and by garbage in both directions against a
peer asked for `3000000`. One baud higher, `2_823_530`, and the wire is at 3 Mbaud. The negative
controls garble as they must: `(2000000, 1500000)`, `(921600, 460800)`, `(2823530, 2000000)`.

*What selects it is `ftdi_sio`'s rounding, and the boundary was predicted before it was asked.*
`divisor3 = DIV_ROUND_CLOSEST(24_000_000, baud)`; integer part 1 with a nonzero eighths fraction is
a divisor an FT232R cannot run, so every such ask lands at 2 Mbaud. `divisor3 == 8` requires
`baud > 24_000_000 / 8.5 = 2_823_529.4`. **Both predicted boundaries were confirmed to the baud, in
both arms** — 2823529 → 2 Mbaud / 2823530 → 3 Mbaud, and 1548387 → 1.5 Mbaud / 1548388 → 2 Mbaud.
Item 93's remainder (a), *what selects a dead rate*, is answered for this adapter: divisor
reachability, with the grid spelled.

**The consequence for §7.1 clause 7, and it is the reason this is a design entry.** The clause
verifies a configured rate by read-back within ±2.5 %. On this platform that check is satisfied by
an **echo**: `ftdi_sio` reports the rate it was *asked* for, which `doctor/src/probes.rs` already
recorded for the accepted rungs and which is now quantified — a **41 %** error reported as
verified. Darwin's `IOSSIOSPEED` path does the same (§15.68 clause 4). **So the read-back is not a
wire-rate verification on either kernel**, and the clause's guarantee is narrower than its wording
invites: it catches a rate the driver *refuses* or *answers differently* (4000000 reads back 9600
and the open fails — §15.58's corrected example), and it says nothing about a rate the driver
accepted.

***Declined: making `load` refuse a rate it cannot verify.*** Recorded so it is not re-proposed as
hardening. The only honest predicate is *this ask lands on this chip's divisor grid*, which needs a
per-chip divisor model in the daemon — FT232R's `3 MHz / (n + f)` with sub-integer divisors barred
below 2 is one chip in one family, and the design's §13 position is that the daemon does not carry
silicon tables. The alternative, refusing every non-standard rate, would refuse `250000`, which
this very bench round-trips byte-exact and which P5's ladder has exercised since the beginning.
**What is done instead is to stop the surface from over-promising**: `actual_baud` is a read-back
and the clause now says what a read-back does and does not establish, with this measurement cited.
An operator who needs the wire rate reads `achieved_baud_floor`, which is timed from the trials —
and note that cell is only meaningful under P14's *constant-airtime* payload; see clause 3.

**3. The pattern-stimulus experiment is a measured decline, and P14's payload does not move.** Plan
§18 item 98 accepted the stimulus half of the operator's battery proposal and specified its shape:
a 12-byte `0x00` run, a 12-byte `0xFF` run, a 16-byte `0x55`/`0xAA` alternation, LCG elsewhere, with
position tags shipping alongside or not at all. Its own pre-registration predicted the inlay would
change nothing on a short TTL-level crossover, and said a measured decline would be worth more than
an untested entry. **It was measured on a throwaway instrument first, deliberately**, because
landing it re-bases `max_reliable_baud` across every committed artifact under an unchanged digest
pair (§15.44's residual) and owes a hand-announcement — a cost that must not be paid for a stimulus
measured to buy nothing.

Three hypotheses, one per block, each predicting something the others do not: the `0x00` run is the
sustained-low-duty stimulus that bites an AC-coupled path; the `0xFF` run is the only one whose
distinctive counter is `brk`; the alternation is the transition-density stimulus that bites clock
recovery at the top of the ladder. **Result: the inlay separates from the LCG on nothing this bench
can read.** Both arms byte-exact 3 of 3 in both directions at 9600, 19200, 115200, 460800, 921600,
1500000, 2000000 and 3000000, with `frame`, `overrun`, `parity` and `brk` deltas all zero for both,
at P14's own constant-airtime lengths.

**The null is evidence because every counter it rests on was moved on the same bench in the same
session.** A rate mismatch moved `frame` by 18; `tcsendbreak` moved `brk` by 1; a present-but-slow
reader moved `overrun` by 5 at 460800 and again at 3000000; a receiver demanding even parity against
a sender using none moved `parity` by 41. Without those, an all-zero table would be AGENTS §3's
tell — a passing output identical to a not-running output. `buf_overrun` moved in none of them and
is **not** claimed as exercised.

*Scope, stated rather than implied:* this is a null on a DC-coupled ~30 cm TTL crossover. The
`0x00` run's mechanism needs an AC-coupled, opto-isolated, RS-232-transceiver or long-cable path to
bite, and none was available. The decline is to the *stimulus on this class of bench*, not to the
hypothesis.

*One instrument defect, recorded because it is the more useful half.* The first version of the
comparison wrote each payload whole and only then read, which overran the receiver above 115200 and
made **both** arms fail identically — `overrun=30` and a byte count identical at four different
rates and four different lengths. A comparison whose two arms are broken by the apparatus says
nothing, and it says it in the shape of agreement. The repair was to interleave read and write
under `poll`, structurally as `p14_trial` already does. **The same trap has a second instance in
this session**: item 93's constant-**payload** requirement destroys the timed `achieved_baud_floor`
readout, because 240 bytes at 2 Mbaud is 1.2 ms of wire time against a comparable USB turnaround —
the column reads 33459 for an ask of 38400 and saturates near 145000 for every ask above it. P14
avoids this with a constant-**airtime** payload. The two requirements are in direct conflict and a
figure from the wrong one is not a slow reading but a meaningless one.

**4. A symbol-keyed citation gate is declined, with the count rather than an argument.** Plan §18
item 96 asks whether the filename-keyed citation gate has a symbol-keyed sibling worth building,
after §15.61's rename left `serial_nexus_sys::honours_rtscts` standing in normative prose. Measured
over the two normative documents: **937 identifier-shaped backtick tokens, 63 flagged by a naive
"must exist in some `.rs` file" check, 24 surviving five mechanical filters, and 1 true positive.**
The residue is not noise to be tuned away — it includes a **verbatim quote of corrupted test-runner
output** (`crossover_rig_actual_baud_is_a_read_back_not_an_echotest`, plan §18 item 95's own
evidence for the interleaving hazard, symbol-shaped and wrong on purpose) and several names quoted
*in the sentence that records their rename*, where the stale spelling is the subject. A gate would
cost a 23-entry counted allowance on day one and a doc edit in every commit renaming any of 937
tracked tokens, to catch one defect that an alignment pass catches for free. **Declined**, and the
declining rule is the one already on the books: nothing reads prose, and §16.13's discipline plus
each generation's alignment pass is the practical guard. Re-opened on a second recurrence, not
before. The three live stale sites are annotated where they stand (AGENTS §5) rather than rewritten.

### 15.70 The armed-wait list is the pattern wait's sixth dimension, and it gets the maximum the other five had

**Status:** DECIDED and EXECUTED — contract at §10 (*The pattern wait*, clause 2, which now bounds
the endpoint's **occupancy** beside the request's dimensions); construction is plan §18 item 64(a),
**executed 2026-08-21**, which was the last clause of that item still open and whose closing closed
the item. Its own filed remedy had been recorded REFUTED first (notes §3.109: six runs of the filed
throughput test read a wall ratio of 0.86–1.13 with `bytes_scanned` complete and `gaps: 0` in all
six), which is why the guards below assert mechanism. Nothing else in §10 moves, and the verb's
semantics are unchanged: this is a stated maximum where there was none.
*[**Corrected 2026-08-21:** this line read "the one clause of that item still open" in the present
tense, after item 64's own entry already recorded (a) EXECUTED 2026-08-21 and the item closed with
it. A standing promise that outlives its answer is what rule 2 of §15.72 below forbids, and this is
its instance (j)'s shape — a dated filing read as live status — in an entry that landed the same day
as the sweep collecting them.]*

**The gap, measured rather than supposed.** §10 clause 2 gives every dimension of a `tap.wait`
*request* a stated, structurally checked maximum under §16.12's rule — pattern count, pattern and
name length, compiled size, the lookback window, the context width, the deadline — and §15.56
declined an unbounded lookback in exactly those terms: a match window without a stated maximum is
an allocation and a scan cost an operator input controls. `TapHub::ingest` walks the endpoint's
armed-wait list on **every** ingested chunk and each entry rescans its **whole** retained window on
the single runtime thread, so the hub's per-chunk work is

    waits × lookback × the pattern's cost per byte

Two of those three factors carried a maximum. The third — how many waits — carried none, so the
product carried none. §16.12's rule is that the maximum exists, not that every surface mints its
own; this surface had none at all.

**What was measured, before any number was chosen.** Release build, 4 KiB chunks fed straight into
`TapHub::ingest`, windows warmed to `MAX_LOOKBACK`, one 20-core box under a load average of
3.6–5.7 — so these are ceilings on a busy machine rather than best cases, and the ladder was read
three times. Scope is plan §18 item 64(a); `daemon/src/tap.rs` carries the same table at the
constant, which is where a reader of the code meets it.

| reading | figure |
|---|---|
| per armed wait per chunk, literal `login:` | 6.1–6.2 µs — **0.089 ns per scanned byte** |
| per armed wait per chunk, `(?-u)[a-zA-Z0-9]{200}` | 111.4–112.3 µs — **1.61 ns/B** |
| per armed wait per chunk, `(?-u)[^\n]{4000}` | 111.2–111.7 µs — **1.60 ns/B** |
| per **open tap** per chunk, same list, same run | **20–22 ns**, flat |
| per-wait cost against list length | flat 2.7–4.0 µs from 8 waits through 128; 3.5–4.1 at 256; 4.8–5.1 at 512; 5.4–6.0 at 1024 |

and end to end — `info` timed over the control socket while a `sim pty --source` firehose feeds one
endpoint with no consumer and the ring off, three runs at load 9.9–19.0:

| armed waits | `info` p50 | p90 | max |
|---|---|---|---|
| 0 | 0.11–0.22 ms | 0.20–0.27 ms | 0.31–0.97 ms |
| 1 | 7.8–11.7 ms | 11.5–15.7 ms | 14.1–18.7 ms |
| 8 | 60.0–71.6 ms | 74.0–87.4 ms | 83.6–101.5 ms |
| 64 | 0.08–0.26 ms | 0.16–689 ms | 625 ms–1.06 s |

The bytes actually ingested inside each window differ per rung — the producer→hub feed hop drops
when the hub is slow — so these are not per-byte normalised and **no ratio is derived across
rungs**; the n=64 median is bimodal for the same reason, and its tail is the reading rather than its
middle. This is the worst case the tree can build — a software firehose, the maximum lookback, a
prefilter-defeating pattern — and is **not** a console figure.

Three things follow, and only the last is this entry.

1. **A wait is not a tap, by 130× to 5300× per element.** That refutes the symmetry the surface had
   been surviving on: `taps` rides the same per-chunk list uncapped because a tap is a pointer, a
   clone and a `try_send`, and the reason does not extend to a full window rescan.
2. **The pattern is a cost the caller chooses, on top of the window the caller chooses.** 0.089 ns/B
   against 1.60 is an 18× spread between a literal the engine can prefilter and a regex with no
   literal in it. `MAX_COMPILED_BYTES` bounds the *build*; nothing bounds the match throughput.
3. **The list's length is the factor with no maximum at all**, and it is the one this entry closes.

**Reachability, stated because a decline would have turned on it.** Every armed wait is one
control-socket connection: §15.20 runs one waiting verb per connection, and §10 clause 8 keeps
`tap.wait` off the web bridge's allowlist — **asserted rather than merely absent** (`web/src/bridge.rs`
requires the refusal by name), since an absence proves nothing about intent. So the caller is
already through the 0600/0660 socket and could call `shutdown` instead, which is strictly the
stronger lever. **That argument is not available here, because §15.56 already rejected it for this
exact surface**: a backtracking engine and an unbounded lookback are refused *by design* on the same
socket, from the same caller. The settled posture is that the pattern wait's cost dimensions carry
stated maxima regardless of who can reach them, and this is that posture applied to the dimension
that was missed. The accidental case decides it as firmly as the adversarial one — a client that
leaks connections reaches an unbounded list by accident, and today that degrades every console the
daemon runs.

**The decision: `MAX_ARMED_WAITS = 64` per host-facing endpoint, refused before anything is armed
or scanned.**

- **The number comes from the ladder, not from taste.** Per-wait cost is flat from 8 waits through
  128 and climbs past it as the retained windows outgrow cache, so beyond that knee an operator's
  input stops buying linear cost. 64 sits one binary step inside the measured flat region. It bounds
  the retained windows to **4 MiB** per endpoint at the maximum lookback, and the hub's per-chunk
  work to a *stated* 0.40 ms (literal) or 7.2 ms (no-prefilter regex), where it had no bound at all.
  It is eight times `MAX_PATTERNS`, whose own doc names the escape hatch this cap must not close —
  a caller that wants more opens a second connection — so 512 patterns can still be watched on one
  endpoint at once.
- **Checked as `arm_wait`'s first statement, ahead of the replay scan.** A hub at its cap that still
  scanned the ring would hand a *refused* caller a bigger lever than an accepted one: 14.984 ms
  (literal) and 91.997 ms (no-prefilter regex) per `tap.wait --replay` over a ring at its 16 MiB
  maximum, repeatable per call with no wait ever registered to be capped.
- **The refusal is `-32602` and names both numbers.** Same code, same breath, as every other §10
  clause 2 maximum, and §16.12's rule about refusals applies: the message names the occupancy, the
  maximum, and what makes the count a cost.
- **It is occupancy, not a quota.** A wait that matches, expires, is cancelled, or whose connection
  closes frees its slot at once, so the refusal's *retry* advice is true and the guards assert it.

**What this does NOT do, stated so it is not quoted as more than it is.** *One* max-lookback
no-prefilter wait already moves the control socket's `info` p50 by 40–110× on a firehose, and the
cap cannot touch that: the lookback and the pattern's cost per byte are the dominant term and are
already-stated maxima this design accepts. **This bounds a product that had no bound; it does not
make the worst case comfortable.** Lowering 64 is now a one-constant change with a plant-proven
guard behind it.

**Two factors the measurement found and this entry does not fix**, recorded with their numbers so
that whoever files them starts from a measurement rather than an argument:

- **Match throughput is uncapped, and it is the dominant factor.** `MAX_COMPILED_BYTES` bounds how
  large a compiled pattern may be; nothing bounds how many nanoseconds a byte of the window costs
  at match time, and the spread between a literal and `(?-u)[a-zA-Z0-9]{200}` — both ordinary asks —
  is 18×. `MAX_LOOKBACK`'s doc says the window "is a scan cost as well as an allocation", which is
  half the sentence: the window is the byte count and the pattern is the price per byte. **Do not
  close this by lowering `MAX_ARMED_WAITS`** — that is the other factor, and this entry already
  bounded it.
- **`tap.wait --replay` is a repeatable ring scan with no cap on the repetition.** The cap check
  sits ahead of the scan precisely so a capped endpoint cannot be milked for it, but a caller
  *under* the cap can arm, match-or-disarm and re-arm without limit, and each cycle is one full
  scan at the figures above. It is one connection's serial cost (§15.20), so the amplification is
  connections rather than requests — the same reachability recorded above.

**DECLINED, recorded so they are not re-proposed (AGENTS §5):**

- **A dedicated application error code for the refusal.** It is the more precise typing — a cap is a
  transient state refusal, closer to `EdgeInboxFull`'s shape than to *your params are wrong*, and a
  caller cannot today separate "shrink your regex" (never retry) from "wait for a slot" (retry) by
  code alone. Declined **for now**, and the reason is not typing: a ninth `AppError` is a `docs/rpc`
  table row plus a two-way registry gate, and a code with no documented row is exactly what that
  gate exists to stop. Carried as this item's residual rather than pretended away; overturned by
  anyone landing the row and the registry entry together.
- **Capping per daemon rather than per endpoint.** The scan cost is per chunk *per endpoint*, so the
  endpoint is the axis the cost actually has. Endpoints are operator configuration rather than
  request input, which is the line §15.34's screening rule draws.
- **Capping control-socket connections instead.** It would bound this and everything else, and it is
  a far larger decision — every stream verb, every subscription, the web bridge's own leg — taken on
  evidence about one verb. It is also the wrong shape: `tap.wait` is the cheapest lever on that
  socket with a 5300× multiplier, and a connection cap generous enough for legitimate clients is far
  too generous for this one.
- **Choosing the number from a latency budget.** Measured, and it disqualifies itself: a budget
  tight enough to hold the firehose figures above is met only at zero waits, because one accepted
  wait already blows it. A cap chosen that way would be a refusal of the feature wearing a number.
- **A throughput assertion as the guard.** Item 64(a)'s own filed remedy was exactly that, and it
  was built, measured and removed: six runs read a wall ratio of 0.86–1.13 with `bytes_scanned`
  whole and `gaps: 0` in all six. The guards here assert *mechanism* — the bound is reached, the
  next is refused, the refusal arms nothing, a freed slot is reusable — which is deterministic and
  needs no clock.

**Three things the work found about its own guards, the first being the useful one.**

*A plant that reddened nothing.* Moving the cap check below the ring scan but **above** the
`AlreadyMatched` return still answers `Refused` — the scan runs, the cost is paid, and no assertion
anywhere can see it, **because a cost leaves no outcome behind**. The guard's doc now says which
mis-placements it catches and which it does not, and states that the ordering is held by
construction — the check is the function's first statement — rather than by assertion. That is
AGENTS §3's weaker-than-its-comment register met head-on: the remedy was not a cleverer assertion,
it was writing down what the assertion actually holds.

*An off-by-one only a "the bound is reached" assertion can see.* Spelling the arm `>` instead of
`>=` lets 65 waits arm against a stated 64; a `<=` check would pass, and so would any assertion that
merely counted refusals. Both guards assert that the count accepted **equals** the maximum the
daemon's own refusal names, and the itest reads that maximum out of the refusal rather than
hard-coding it — a copy of `MAX_ARMED_WAITS` in the test file would be a second implementation of
the thing under test, which is the shape §7.1 clause 2 forbids one surface over.

*A guard whose first version named the wrong defect.* Two properties — the 65th is refused, and a
capped endpoint does not scan the ring anyway — were probed by one `replay: true` request, so
deleting the cap reddened on the ring-ordering message. Split into two probes; each plant now
reddens on its own defect's sentence. A guard that fails for a true reason that is not *the* reason
costs the next reader the diagnosis.

*[**Citation note, 2026-08-21, recorded because grep is how a §15 number is read.** Two code sites
spell *this* number for decisions that are not this one, and neither is repaired here:
`web/src/assets/app.js` and `web/src/assets/graph.test.mjs` cite §15.70 for the graph page's repaint
skip (plan §18 item 91), and `itest/tests/meta_gates.rs` cites it twice for the privileged helper's
no-`exec` gate, which is **§15.71**. The graph-page decision is a live instance of the class §15.72
collects, in the other direction: as of this entry's landing the tree carries the construction and
this document carries no entry for it, so its record is plan §18 item 91 and the session notes.]*

### 15.71 The blessed helper's no-`exec` bound was true by argument and false in the tree

**Status:** DECIDED and EXECUTED — §15.45's second bound, **unchanged in wording** and now enforced
by construction and by a gate instead of by a paragraph. Construction is plan §18 items 103 (the
`exec` itself) and 104 (the gate). §15.45 is annotated at the bound; no contract moves, and nothing
here widens what the blessed binary may do.

**The transferable fact belongs first.** An invariant this repository states absolutely — AGENTS §4,
§15.45's bound list, and `devprep`'s own module documentation twice over — was **false in the tree**
for as long as `install` shelled out to `getcap`, and **nothing could see it**. The documents that
state it are otherwise scrupulous, which is the point rather than the excuse. Nor could the gate
written to see it, in its first draft: an adversarial plant opened **three matcher holes**, each
demonstrated rather than argued, and all three were one root cause — *a scanner that did not know what a Rust
string literal is*. The sharpest of the three is that the gate reported **green** on the real defect
restored into `read_caps`, because a URL earlier on the same line ended the scan at its `//`; the
binary built from that tree execed `getcap` from `preflight`, confirmed with the same `PATH` shim
that found the original. The other two are the mirror pair: an unbalanced `]` inside a literal drops
the attribute pass's bracket depth so a four-line `#[arg(…, env = "…")]` scans clean, and a brace
inside a literal moves where a `#[cfg(test)] mod` ends and therefore what counts as product code at
all. Every scanner in that file now runs over **masked** source.

**The breach.** `install::read_caps` answered *what capabilities does this file carry* with
`Command::new("getcap")`. The safety argument, written twice in the tree, was that `install` is
refused while any capability in `REQUIRED_CAPS` is held, so the spawn and the capability could never
coexist. **True of the verb, false of the module**: `preflight` → `install::inspect` → `read_caps`,
with no refusal in front of it. **Nothing had to be edited into a violation.** No line of the argued
code changed; a second call site was added somewhere else, and the argument became false where it
stood. That is the failure mode of an invariant held by prose rather than by a gate, and it is why
this entry exists rather than a one-line patch.

**Measured, on the rig box, with a `PATH` shim that reads its *parent's* `/proc/<pid>/status`** so
the capability reading is the spawning process and not the shim: `CapPrm` = `CapEff` =
`000000000000000a` — bits 1 and 3, `CAP_DAC_OVERRIDE|CAP_FOWNER`, exactly the pair the installed
file carries. **Reproduced three times, by three agents, on this box — two of them before this
session** (plan §18 item 103, notes §3.127).
*[**Corrected 2026-08-21:** this read "Reproduced twice, independently". Plan §18 item 103 and
notes §3.127 both record three, on three occasions by three agents, two of them predating the
session that filed the item — so the undercount made a finding that had survived three independent
sightings read as one session's pair.]*

**The harm is not "a child process ran", and this is the measurement worth keeping.** A shim that
answers *this file carries nothing* turns a **correctly blessed** copy's own report inside out, on
the same inode, old reader against new: `ready (mode 0700, cap_dac_override,cap_fowner+ep)` and
exit 0 becomes `Unblessed("(none)")` with a `sudo setcap …` line attached and exit 2. So the
**environment chose the verdict** of a binary whose first stated bound is that it reads none — and
the verdict it chose is plan §18 item 101's harm exactly: a tool naming a privileged repair for a
file that needs none. Item 101 arrived through a hardlink that `cargo` re-pointed; this one arrives
through `PATH`. Two mechanisms, one wrong sentence, one wrong conclusion available to the next
session.

**`PR_SET_NO_NEW_PRIVS` is what §15.45 cites as making this bound a kernel guarantee, and it does
not cover this.** It stops an `exec` from *gaining* privilege. It says nothing about an `exec` that
inherits the `PATH` and the whole environment of a process that already holds it. The bit was
established and read back correctly — against the wrong half of the hazard, which is worth stating
because a correct mechanism cited for the wrong property reads exactly like coverage.

**The repair, and the hint that found it.** `install --verify` answers the same question on the same
binary in the same session and execs nothing — two readers of one question, one spawning and one
not. A file's capability set *is* its `security.capability` extended attribute, so the honest reader
is one `getxattr(2)`. It lives in `serial_nexus_sys::caps` because §16.3 puts every syscall there
and this is the only `unsafe` involved; `devprep` keeps the vocabulary, `REQUIRED_CAPS` remaining
the one place a capability name is written. No new dependency, so `cargo deny` does not move.
`docs/vmcell-requirements.md` had already recommended the same change for an unrelated reason — the
pinned base image ships no `getcap` at all, so the old reader failed on `ErrorKind::NotFound` there.
Two arguments, one line of code.

**A guard was deleted on purpose, and the deletion is the point.** The old pipeline parsed `getcap`'s
stdout and carried a careful regression test for a real defect: `getcap` prints `<path> <caps>`, so a
whole-line test for `ep` is satisfied by the **path** — a `deps/` component supplies those letters,
and so does any home directory whose name happens to contain them — and a `+p`-only binary read as
blessed. There is no line and no path in a
twenty-byte kernel record, so that class is now **unrepresentable** rather than guarded. Every
*decision* the deleted tests protected is re-asserted on bits, and one case got stronger: `carries`
sees an inheritable-only capability, which the text era could see only if libcap happened to print
the name. The stock tool's rendering survives pinned to real output — `cap_dac_override,cap_fowner=ep`
against that same file's actual twenty bytes — as the decoder's fixture one crate over.

**The gate, and what it cost to make non-vacuous.**
`the_privileged_helper_neither_spawns_a_process_nor_reads_the_environment` scans `devprep/src/**`
structurally for both classes. Three things it needed that a first draft would not have had:
`Command` matched as a **substring** rather than a whole word, because
`use std::os::unix::process::CommandExt;` is the counter-example and is planted for that reason; a
**separate attribute pass with bracket-depth tracking over masked source**, because clap's
`#[arg(env = "…")]` reads an environment variable with none of the obvious tokens present and
rustfmt may put the token on a continuation line — the four-line plant reddened naming the
attribute's **third** line, where a first-line scanner would have passed it while claiming coverage;
and the **token list proven load-bearing**, removing any single spelling from either list reddening
the gate's own matcher proof, without which a gate can list five spellings and match two. Validation:
**26 plants → 26 red** (the original `getcap` spawn restored in place, an aliased import, `CommandExt`,
all four method spellings, eleven environment spellings, three clap-attribute spellings, a spawn in
the macOS arm, a spawn in `main.rs`, the manifest feature, and the walker floor), **3 negative
controls green** (a spawn inside `#[cfg(test)]`, `Command::new` in a comment, a local variable named
`env`), and **6 token-removal probes red**. Every restore verified byte-identical.

**What the gate does not cover, named at the gate rather than left to be discovered.** It scans this
crate's sources, so a spawn or environment read performed *for* the helper by a dependency is
invisible to it — which is why `devprep`'s manifest states that its dependency list is part of its
security argument and why that list is three crates long. And `#[command(version)]` expands to an
`env!("CARGO_PKG_VERSION")` **inside clap's macro**, so the binary does contain a build-time
environment read that no scan of these sources can see: it is the accepted instance, named at the
gate so the claim reads *no such spelling in these sources* rather than the larger claim it might be
mistaken for. Attribute-borne reads are additionally closed at the manifest, clap's `env` feature
having to stay off for `#[arg(env)]` to compile at all — which covers spellings rustfmt has not
invented yet.

**What could not be measured here, said plainly.** The repaired binary has never run **blessed**.
There is no passwordless `sudo` on this box and no user-namespace route — `unshare -Ur` answers
`Operation not permitted` writing `uid_map` — so no unprivileged process here can create or refresh
a file capability. *Does not exec while blessed* therefore rests on two things that **were**
measured: the code path execs nothing at all, proven on the identical route with the identical shim
against a binary built from the unfixed tree in the same scratch harness, and the construct is now
forbidden by a gate shown to redden on every spelling it claims. `sys/` changed, so §15.45's standing
re-bless applies before the next rig lane — that is §15.45's design and not a defect.

*[**Annotated 2026-08-21 — the paragraph above is now answered rather than softened.** The repaired
binary has since run **blessed**. The maintainer refreshed the installed copy's capabilities out of
band — nothing above changes about this session's own reach, no unprivileged route to `setcap` was
found or used — and the repaired `preflight --json` was then driven on the rig box under a `PATH`
poisoned with shims for `getcap`, `setcap`, `capsh`, `sh`, `bash`, `env` and `cat`, each shim
logging its own invocation to one file. It answered `REPLUG-PREFLIGHT: READY` and **the shim log
was empty**: the first measurement of the repaired path on a genuinely blessed helper, and the
bound §15.45 states read directly rather than inferred from an unblessed route. **Re-run later the
same day against the same blessed inode** — `getcap .snx-bin/debug/serial-nexus-devprep` reading
`cap_dac_override,cap_fowner=ep` — with the same seven shims: **the log is still zero bytes**,
while the verdict now reads `REPLUG-PREFLIGHT: BLOCKED-ON-BLESS` with `bless_problems: ["Stale"]`
at exit 2, because a concurrent lane rebuilt `devprep` two minutes after the bless and
`install::inspect` compares the two artifacts byte for byte. That is §15.45's standing re-bless
again, not a defect — and the same run is a live sighting of the sentinel defect recorded unfixed
just below, since `Stale` is what asks the maintainer for a `sudo setcap` it does not need.]*

**Recorded and not fixed.** `preflight`'s verdict sentinel names `sudo setcap` for **every** bless
problem, `Stale` included, which needs no privilege — measured on this tree: a `Stale` bless answers
`REPLUG-PREFLIGHT: BLOCKED-ON-BLESS`, and the sentence beside it asks the maintainer for one
`sudo setcap`. That is item 101's defect surviving one function over, in the sentence an operator
actually acts on. `install --verify` was repaired and `preflight` was not, because the two print
different strings and only one was under audit; left owed rather than fixed in the same commit,
because the string is a harness-visible sentinel and its repair deserves its own fail-first proof.

**The register entry.** AGENTS §3 collects gates whose passing output is identical to their
not-running output. This is a **seventh register** in that family and its sharpest member: **a gate
that does not exist, standing in for one because a comment argues the case.** Its passing output is
identical to its not-running output for the simple reason that it is not running. The remedy is the
one already on the books, aimed one level up — for each invariant stated as absolute, ask not *is
this true* but **what would go red**, and if the answer is a paragraph, the answer is nothing.

### 15.72 Design behind the tree: settled prose a ledger execution falsified

**Status:** DECIDED and EXECUTED — the class named, every instance found repaired **where it stands**
(AGENTS §5, with the §15/§16 entries annotated rather than rewritten), and the gate that would catch
it **scoped, costed and DECLINED** on its sibling's grounds, re-openable on a second recurrence
naming new evidence. Construction is plan §18 items 108 and 113. No contract moves: every repair
below brought prose to the tree, never the tree to prose.
*[**Corrected 2026-08-21:** this line read "left as open work rather than claimed", and the
comparison paragraph below read "left **open** rather than declined". Plan §18 item 108 and notes
§3.139 record the candidate **DECLINED** on item 96's grounds, and plan §18's item entries are the
authority on an item's disposition (decided 2026-08-21 at item 95). A design entry and the ledger
disagreeing about a disposition is one register off this entry's own class — the disagreement is
with the ledger rather than with the tree — and a reader had no way to tell which of the two was
wrong, which is the whole harm.]*

**The class.** AGENTS §2 tracks **design-ahead-of-tree** surfaces by name, one at a time, and got
the last of them to zero on 2026-08-15 when plan §18 item 41 built the `actual_baud` read-back. This
is the mirror, and it had no name, no counter and no sweep: **settled-system prose describing a tree
that a ledger item's execution moved past.** The asymmetry is the whole reason it survives.
Ahead-of-tree is *deliberate* — the amend-first order creates it on purpose, dates it, and files an
item whose closing is the thing everyone is watching for. Behind-tree is nobody's decision. It is
created by a *success* somewhere else, it carries no date, nothing announces it, and the sentence
that becomes false is usually in a section a reader treats as the system rather than as a record.

**The instances, and each is annotated at its own site.** *Ten here against the ledger's five, and
the gap is two things at once — a finer unit of counting, and three sites the ledger does not
itemise.* Plan §18 item 108's entry itemises **four** — (a), (b), (e) and (f) below — and names a
fifth, (i), filed separately as **item 113** because the mechanism that found it differs; those five
are what its head sentence counts as *Five sites*. The finer unit accounts for two of the extra
five: **(c)** and **(d)** are further false sentences inside (b)'s own clause, which the ledger
counts once, as a site. The other three are additional locations, repaired in the same session and
annotated where they stand but not itemised at item 108: **(g)** the front matter, **(h)** §8's
deferred-capability list, and **(j)**, one bullet over five decision-record entries and the only
instance whose sites all sit in §15/§16 rather than in settled-system prose. So a count of this
class is quotable only with its unit and its scope — sites item 108 itemises, or false sentences
this list corrects — and for *how much work item 108 was*, the ledger's is the one to quote.

- **(a) An `exec` teardown floor, restated in three settled clauses nine days after it stopped being
  one** — §5's teardown-ledger clause 9, §7.6 clause 8 and §8's codec-contract clause 14, against
  plan §18 item 21 executed 2026-08-12. §15.50's *"Open, named"* sentence stands verbatim under a
  dated closing annotation, because it is the shape that item closed. What survives is renamed a
  **boundary**, not a floor: bytes already inside a child's stdin pipe are delivery, exactly as bytes
  written to a device fd are. **The document was already contradicting itself and nobody read across
  the gap** — §8's own fixture register, some 160 lines below clause 14 in the same section,
  described `deaf.py` as what makes item 21's guard measure a real quantity instead of a zero.
- **(b) §7.2 clause 12 asserted a gate the tree had removed and a causal claim measurement had
  refuted.** It called the anti-spin guard the tree's sharpest known proxy-in-space *because it
  self-skips off Linux*, and predicted that a widened last-close predicate would burn a core on
  macOS with the suite green. Item 12 executed 2026-08-13 with that prediction **refuted** on the
  platform it named, and the guard is a bare `#[test]`. A refuted causal claim standing as normative
  prose is sharper than a stale citation: a reader reasoning from §7.2 would have reasoned from a
  mechanism neither kernel exhibits.
- **(c) The same clause's appositive inverted both halves at once** — *"it self-skipped off Linux,
  the one platform family where the hazard exists"* reads, left to right, as an apposition to
  **Linux**, and contradicted the refutation three lines below it.
- **(d) A `0x20` reading attributed to a probe that discards the byte.** §7.2 cited doctor P1 for
  `TIOCPKT_DOSTOP`; P1 carries three booleans and `p1_inner` tests the bit and **discards** the
  bytes, so `0x20` appears in no committed artifact on either kernel. The reading is real and is a
  2026-07-28 rig-session measurement in `docs/macos.md`, cited as such at the baseline contract's
  clause 4. **The identical false attribution had been found and repaired once before**, by the blind
  verifiers at the v15 landing — and was reintroduced in prose that had every other citation right,
  which is the tell this entry is about: *a claim verified against a neighbouring sentence rather
  than against the artifact.*
- **(e) A remainder discharged in one copy of a sentence and not the other.** §13 clause 8 named item
  17's parity-mismatch and break-reception work as remainder after it executed 2026-08-15; the
  near-identical sentence at §15.21 was already annotated with the answer. The tell for the whole
  class is in that pair: **a document with two copies of a sentence gets one of them fixed**, and the
  unfixed one is the one in the section a reader treats as the system.
- **(f) An era row reading "the current era" two eras on.** `e79f5fcd86a2e5f0` sat open beside
  `4317ea5ac187f506` and named no bounding artifacts where every sibling row does — contradicted by
  §13's own era law two paragraphs above it, by `docs/doctor/README.md`, and by AGENTS §2. Closed
  from the committed artifacts rather than from memory, with two halves now stated as **unobtainable
  rather than owed**: no Darwin capture exists in that era at all, and its closing triple has no
  passive counterpart.
- **(g) The front matter claimed one design-ahead-of-tree surface for six days after item 41 closed
  it**, while §7.1 clause 7 — the clause it named — said the opposite at its own site. The register
  that exists to count this class was itself an instance of the mirror class.
- **(h) §8 promised three capabilities as deliberately absent for nine days after the tree built
  them** (items 36, 38 and 53, all executed 2026-08-12), with its own fixture register 160 lines
  above already describing one of them.
- **(i) §7.1 clause 7's superseded per-mode block was being read as live, and its `xon-xoff` sentence
  was false in both of its claims** — plan §18 item 113. It said the mode *has no pre-check and no
  probe* and is *unmeasured rather than known-good, carried as open work*. There is a pre-check
  (`flow_precheck_target` maps the mode and the load path runs it through the same
  `honours_flow_control` call `rts-cts` takes), there is a probe (P15's `software_flow_control`
  block, landed at item 14, executed 2026-08-13), and §15.61 refuses the mode where the driver drops
  it. The clause disagreed with itself twice over — its own live text immediately above the block,
  and clause 6's parenthetical twenty lines up. **The half a reader reaches last is the half that was
  wrong**, and a supersession marker sitting mid-paragraph is not a marker.
- **(j) Dated filings read as live status.** §15.58's and §15.59's *written before the tree moves*
  status lines, §15.68's *is filed* for item 98, §16.1's pty/log rebase recipe and §16.11's owed
  destination record all pointed at items since executed. Each now carries a dated annotation; the
  ordering rules those entries state are the durable half and still bind.

**Two rules the instances leave, both about repairs rather than about the original defects.**

*A repair that widens a claim's scope is a new claim and owes the evidence of one* (AGENTS §7).
Repairing (b) rested the pty last-close drain's justification on two legs and wrote the second —
P6's `handler_reset_readable_bytes: 1` — as *identical on both kernels*. Checked against the
committed corpus: **`1` in 72 of 72 Linux observations and `0` in 32 of 32 macOS ones**, and
`handler_reset_extproc_retained` **`true` in 64 of 64 Linux and `false` in 27 of 27 macOS**, with
`EXTPROC` gating `TIOCPKT_IOCTL` entirely, so a kernel that drops the flag emits no control packet
and the drain has nothing to consume there. The notes and the in-tree comment the repair was checked
against say *identical on 6.18* and *byte-identical on 7.0 and 6.18* — **claims about two Linux
kernels**, carried across the Linux/Darwin boundary without a figure changing, so there was nothing
numerically wrong to notice. Two things caught it: the clause contradicted itself six lines later,
and **an in-tree comment is not a citation** — it is a claim of the same kind, and this tree has now
found that particular one stale. The drain is justified **per kernel** now, and neither reading
licenses deleting it.

*A figure moved between clauses loses its scope unless the move carries it.* The same pass
attributed a 1.7–1.9 %-of-a-core band to both kernels when item 12 scopes it to **Darwin** (the Linux
cost recorded at the guard's own ceiling is some three to four times lower), and quoted item 17's
`brk +1` / `frame +0` and `parity +2` with no scope at all, both being `ftdi_sio` on Linux 7.0.0-29
on a cross-wired FT232R pair, both guards self-skipping off Linux on `ICOUNTS_SUPPORTED`.

**No gate in this tree reads design prose against the ledger, and none was built this session.** One
was scoped and costed, and the cost is recorded here so a future filing starts from numbers rather
than from an argument. It has two halves and they are not alike:

- **The resolves half — does a cited item number exist in plan §18?** Cheap and *precise*, because
  item numbers are structured tokens with an authoritative table behind them and `meta_ledger`
  already parses that table. It has a demonstrated true positive from this very session: the first
  repair's own annotation at §15.50 cited a plan item that does not exist, caught by re-reading
  rather than by any instrument. It is still a matcher problem before it is a lookup problem, which
  is item 100's lesson one document over — measured on this document's normative half after the
  repairs above, **33 citation sites over 13 distinct item numbers, of which 3 wrap across a line
  break and one uses the plural `items 14 and 22` form**, so a line-anchored, singular-only scanner
  reads 29 of them and never sees the numbers in the other four. **The ledger states this
  population differently, and the two are not the same measurement.** Plan §18 item 108 and notes
  §3.139 give **18 citations over 11 distinct items**, counted while the gate was being costed — in
  the same breath as the unfixed-tree yield above, and necessarily before these repairs, since the
  post-repair count of the scope below is 33. The repairs added **17** citation sites to that scope
  themselves, repaired sentences and dated annotations alike, since rule 2 below requires the item
  number *and* the date at every one of them. Under this entry's matcher the pre-repair document at
  `800915b` reads **16 over 9** in the normative half and **19 over 11** when §16's clauses are
  counted with it, so the ledger's distinct-item count reconciles against that wider scope while
  its site count reproduces under neither — no scope is stated beside it, so the two figures can be
  compared only by re-measuring, which is what this entry's own figure-scope rule asks. **Quote the
  post-repair figure for what a gate would face** — a gate built later scans the document as it
  then stands — and the ledger's for what the costing saw. Scope for both figures here: `plan §18
  item(s) NN` citations, wrap-tolerant and plural-aware, over everything ahead of §15.
- **The open-versus-executed half — does the prose describe as owed what the ledger records as
  executed?** That is a natural-language property, and a word list is a proxy for it. Measured with a
  ten-word list: on the unfixed tree **3 flagged, 3 true positives, 0 false alarms**; after the
  repairs **2 flagged, 1 true positive and 1 false alarm**, the false alarm landing on this item's
  own new era-row prose (*unobtainable rather than owed*). The precision is good. **What kills it is
  coverage, not noise: it reaches 2 of the 6 spots corrected by the pass that costed it.** Three of
  those cite the ledger with no item number at all, and the era row states its false claim in prose
  naming no item — a citation-keyed gate is structurally blind to exactly the sites that cite
  nothing, which is the population that grows. Making it see them means first **banning the bare
  `plan §18` citation in normative prose**, which is a document-wide edit plus a standing constraint
  on every future clause.

**Compared honestly with the sibling decline.** §15.69 clause 4 declined a symbol-keyed citation gate
with its count — 937 identifier-shaped tokens, 63 naive flags, 24 surviving five filters, **1 true
positive** — on the grounds that a 23-entry allowance and a doc edit in every renaming commit is too
much machinery for a defect an alignment pass catches for free. This candidate's population is
smaller by a factor of **28** — 937 tokens over the two normative documents against 33 citation
sites in this one's normative half — and its measured precision is far better, and its resolves half
is not a proxy for anything. Against the ledger's pre-repair 18 the factor is **52**.
*[**Corrected 2026-08-21:** this read "two orders of magnitude smaller", which the sentence's own
numbers falsify — 937 ÷ 33 = 28.4 and 937 ÷ 18 = 52.1, both well short of the hundredfold that
phrase claims. A ratio written in words beside the figures it is computed from is checkable in one
division, which is how this one was caught.]*
**And it is still DECLINED**, because none of that is what decides it. The coverage measurement is:
it reaches 2 of the 6 spots corrected by the pass that costed it, and that applies to the resolves
half as squarely as to the other, both halves being keyed to a citation the blind sites do not
carry. The two halves *can* land separately and the cheap one is genuinely cheap; that argument was
weighed and did not carry against a structural blind spot on the population that grows. Declined on
item 96's grounds and re-openable on a second recurrence naming new evidence (AGENTS §5, plan §18
item 108). **It is not claimed as built, and no fail-first proof is claimed for it**; the three true
positives above are its measured yield on the unfixed tree and are recorded so a future filing
starts from a number.

**What holds the class today**, stated so the absence is not mistaken for coverage: §16.13's citation
discipline, and each generation's alignment obligation — a periodic sweep, not a per-commit gate. It
is what found (a) through (h). **And it is a sweep with a known blind spot**, which (i) demonstrates:
that instance survived two close readings of §7.1 and was turned up by the gate-costing scan, *an
instrument with no idea which sentence was meant to be live*, which is precisely why it read the
stale half that careful reading had learned to skip.

**The rules this entry binds.**

1. **Superseded prose left inline is live prose.** Set it off as a quotation with its falsity
   annotated beneath, or delete it. A *partly* superseded block is the worst case of all: §7.1
   clause 7's `rts-cts` half is still accurate and carries citations the live text does not restate,
   which is exactly what kept earning the false half a reader's trust.
2. **A settled-system sentence that names ledgered open work carries the item number *and* the date
   its answer landed** — never a standing promise that outlives its answer, and never a bare
   `plan §18`.
3. **Executing a ledger item includes grepping this document for the sentences that execution
   falsified.** The notes entry for the item is not that grep, and neither is the plan's own status
   line: both record what moved, while the defect is always somewhere that did not move.

**One live instance is left standing and is named rather than quietly carried**, because the class is
not closed by the sweep that found it: the graph page's repaint skip is built and cited from
`web/src/assets/app.js` as a §15 decision, and this document carries no entry for it. Its record is
plan §18 item 91 and the session notes (2026-08-21).

### 15.73 A pre-check answers acceptance; the wire needs a peer, so the wire question is the doctor's

**Status:** DECIDED and EXECUTED — §7.1 clause 8 is the contract statement, §15.53 and §15.61 are
annotated rather than rewritten (AGENTS §5), and **no refusal, verb, predicate, configuration value
or verdict moves**. Construction is plan §18 item 85 and notes §3.147; the item was filed as
*needing a design decision, not a patch*, and this is the decision together with the instrument that
makes it worth stating.

**The question.** §15.53 separates two states by read-back: a driver that accepts `CRTSCTS` and
reads the flag back clear is `AcceptedThenDropped` and refused at `load`, and one that keeps it is
`Honoured` and allowed through. §15.62's CDC-ACM bench **keeps** it — P15 read
`honoured_on_readback: true` and `shipped_predicate_agrees: true` on both `/dev/ttyACM*` ports,
with `c_cflag` gaining exactly `0x80000000` on each of them (the words themselves differ per port
and per run — `0x10021cb2` → `0x90021cb2` on `ttyACM0` against `0x100218b2` → `0x900218b2` on
`ttyACM1` in `docs/doctor/linux-7.0-2026-08-17-a7e6070-tier3.json`, so **the delta is the quotable
figure and neither word is**) — and the flag did nothing: a 2×2 control (peer RTS low/high ×
`CRTSCTS` on/off, peer never reading) wrote 44672 bytes
in every one of the four cells, spread 0, reproduced on an independent re-run. So the predicate says
`Honoured`, the daemon loads the node, and the operator has a port that *reports* flow control and
does not perform it. **It is §15.61's shape with the polarity reversed**, which is why it was filed
separately rather than as that entry re-opened: §15.61's driver lied by *dropping* the flag and a
read-back caught it; this one lies by *keeping* it, and no read-back can, because the read-back is
what it satisfies.

**The decision is to state the bound, not to close it.** §7.1 clause 8 now says in the contract what
was true and unwritten: the load-time pre-check decides whether the driver **accepts** the setting,
never whether the wire **honours** it, and a port reporting `Honoured` may still be inert. `load`
and `add-node` are unchanged, the predicate is unchanged, no configuration value is added, no new
refusal fires. **The reason is structural rather than a tolerance.** Separating *honoured* from
*inert* needs a peer, a transfer and a stall; a pre-check has one port and one `tcsetattr`, at the
position §11 puts it — *before anything is created* — so the transfer it would need would have to be
driven into the operator's device before the graph exists, past every bound §7.1's open ritual is
built on. A pre-check is the wrong instrument for this question, not a lax one, and widening it is
not a smaller change than moving the question.

**The three candidates plan §18 item 85 named, and why the third is the one taken.**

1. *A functionally verified tier for `flow_control`.* **Taken as a report and refused as a refusal
   grade**, which is the distinction that carries the whole decision. As a grade it would be a
   configuration value the daemon cannot verify at load — a claim wearing a setting's clothes, the
   shape §12's `has_identity_source` exists to prevent one field over — because the verification
   the grade names is exactly the one a pre-check structurally cannot perform. As a *reading* it is
   the `wire_flow_control` cell below, which measures the same property where a peer exists and
   refuses nothing on it.
2. *A documented per-driver allowlist.* Refused on §15.61's own construction — "one predicate,
   parameterised" — and on §16.5's ban on two copies that must agree. A driver name is not a
   measurement, and §15.62's whole lesson is that keying on the transport's identity is how a
   reading about an instrument became a claim about a cable.
3. *An accepted limitation stated in §7.1.* Taken — and paid for with an instrument rather than
   with a sentence alone, because a limitation is only worth stating if something can measure what
   it excludes. **Where in §7.1 is worth one sentence, because the tree cites it.** Clause 2's four
   states already *implied* the bound — only `AcceptedThenDropped` refuses, and all four are read-back
   answers — and the shipped code and plan §18 item 85 both cite clause 2 for it. Clause **8** is
   that citation made literal rather than a second home for it: it says in as many words what clause
   2 said by enumeration, so a reader who follows the citation finds the sentence instead of
   inferring it. Nothing in clause 2 moves.

**What was built: P15 gains a `wire_flow_control` reading, and it counts what the peer receives.**
One reading per direction, emitted **only** where P5 measured an RTS/CTS crossing in *both*
directions on that pair. The certification is the precondition and not a convenience: a reading that
answered *inert* where CTS never followed the peer's RTS would be blaming a driver for a cable,
which is §15.69 clause 1's lesson carried one instrument over. Three cells on the same wire, at
115200 with a 1024-byte payload and a 300 ms receive window — the subject (`CRTSCTS` set, peer
holding RTS low) and two controls (the flag cleared with the peer still not ready; the flag set with
the peer ready) — then the peer raises RTS and the release is read. **Bytes delivered to the peer,
never bytes the kernel accepted:** the accepted count rides beside each cell precisely because it is
the number that cannot answer the question. Six words come out — `gated`, `partly-gated`, `inert`,
`gated-then-lost`, `no-cts-path`, `unmeasurable` — with `honoured_on_the_wire` reserved as `null`
for the last two, which are statements about the bench rather than about the driver. One byte of the
payload crossing under backpressure is enough to leave `gated`.

**The reading's own stimulus control runs first, before any comparison** — does *this* transmitter's
CTS follow the peer's RTS at both levels? — so a bench that cannot be asked answers `no-cts-path`
with `honoured_on_the_wire: null` instead of `inert`. It is proven on the bench rather than argued:
with the stimulus withheld, the gated cell delivers **1024 bytes, byte for byte the count that reads
`inert` when the stimulus arrived** (notes §3.147). A 3-wire rig is §5's own stated assumption and a
CDC stack manufactures the bit (§15.62, §15.68, §15.69 clause 1); on both of those a transmitter that
does not stall is what an **honest** driver does, and calling it `inert` would be §15.69 clause 1's
harm reappearing one instrument over — the same mistake with the nouns swapped, which is why the
ordering is a check and not a comment.

**And it is reported, never judged.** No pre-check consults it, no `load` is refused on it, P15's
verdict does not move on it, and P15's `question` — the string an era is keyed on — is unchanged. A
stricter word in that vocabulary can only ever change a sentence in a report, never redden a lane.

**Why delivery rather than acceptance, and how the payload is sized — the scope the instrument's own
constants cite this entry for.** Plan §18 item 85 was filed with an instrument that counts what the
*writer* got rid of, and that number has a per-driver threshold under it: at 1024 bytes all three
cells accept every byte on the bench of record, so acceptance separates nothing there, and the same
bench shows the separation appearing higher up — with a **65536**-byte payload and a 2 s budget it
accepted **4608** bytes under backpressure against **27648** with the flag off, 2 of 2. Delivery has
no such threshold, which is why it is what the reading folds. The payload is then sized between a
floor and a ceiling. *The floor* is whatever the transmitter can absorb without sending: a payload
small enough to sit inside the adapter would read `delivered: 0` on a bench where nothing gated
anything, which is the one way this instrument could manufacture a false `gated`. It is demonstrated
rather than looked up — at 1024 bytes the two control cells deliver the whole payload on the bench
of record while the gated cell delivers none of it — and **the threshold itself is not measured
anywhere, so no number for it is quotable from this entry or from the constant that cites it.** *The
ceiling* is airtime: 1024 bytes at 115200 8N1 is 89 ms, better than three times inside the 300 ms
window, and the whole block costs one pair of opens and about two seconds.

**The evidence, and it is narrower than an entry like this usually gets to claim. Exactly one arm
has been read off hardware.** The FT232R pair `BH00L4KU` ↔ `BH00LW9U` on Linux 7.0.0-30 under
`ftdi_sio` reads `gated` in both directions in all three captures: **0 of 1024 bytes** delivered
while the peer held RTS low, against **1024 of 1024 in each of the two controls on the same wire**,
1024 arriving once the peer raised RTS, and `cts_after_release: true`
(`docs/doctor/linux-7.0-2026-08-21-800915b-dirty-wireflow-tier3{,-2,-3}.json`, `probe_set`
`4317ea5ac187f506`). P5 in those same captures reads
`5-wire crossover: RTS/CTS both ways, DTR moves nothing`, so the wiring is not a variable in any of
it. **Every other arm — `inert` included — has executed under a fixture and on no bench**, and that
is a construction rather than a shortfall: no bench in this record produces both halves of item 85's
discrimination pair — the FT232R gates, and the transport that is inert is the one P5 will not
certify — so each arm is folded against the others (AGENTS §9) instead of against whichever bench
happens to be plugged in.

*Recorded rather than quietly dropped, because it is the more useful half:* an earlier draft of this
instrument's own documentation cited a second hardware arm, "the CDC-ACM capture's `inert`".
**No such capture exists.** The CDC-ACM bench was read 2026-08-16/17, before this cell existed, and
no artifact in `docs/doctor/` carries a `wire_flow_control` block other than the three named above.
§15.62's inert *finding* is real and was taken with that session's bespoke 2×2, not with this
instrument — and that is exactly the distinction the sentence erased: a session's measurement and an
instrument's arm are not interchangeable evidence, however identical the word they produce. An entry
whose whole subject is *reported versus performed* had a fabricated citation in it, which is the
tell worth keeping.

**The bench that motivated the item is one this instrument cannot be run on**, and that bounds every
reading it will ever accumulate. §15.62's CDC-ACM ports read CTS `stuck-high` in both directions;
`stuck-high` is not a crossing, so P5 does not certify the pair and the cell reports the bench
instead of the driver. **The transport whose defect prompted the measurement is the transport that
defeats it.** Answering there needs a driver that manufactures nothing and is inert anyway —
hardware this record does not hold.

**The captures this entry cites are one `field_set` behind the instrument, and that bounds what they
prove.** They were taken at `9a75a9f83a83a617`; `released_intact` — the comparison that separates a
transmitter which held *the payload* from one that held *some bytes* — landed after them, moving
`field_set` to `b73dba8f32301a84` (notes §3.147). So the hardware arm above proves the stall and the
release **count**, while the **content** half of the `gated`/`gated-then-lost` separation has
executed under fixture only, and the corpus holds no capture of this instrument at the current
digest. `jq -e -f expectations/linux.jq` accordingly exits **1** on all three, isolated to that one
conjunct: adding the single key to a copy of each takes it to exit **0**, measured on all three.
**That is the scope of a decided property and not a finding** — notes §3.146 (i) enumerated where the
expectation files execute (the `doctor` job's step, the `macos` job's step, and
`itest/tests/expectation_gates.rs`, which always runs the doctor fresh) and recorded that **no
committed artifact is ever gated by them**. The scope is wider than these three, and is worth one
number so nobody mistakes the platform gate for a corpus validator: in the current era, **18 of 18
passive captures satisfy their platform's expectation file and 1 of 26 Tier-3 captures does** — the
one taken on the build that added the field the gate now requires. §13's era law keeps a `field_set`
move from closing anything, while `expectations/*.jq` requires each field from the moment it lands,
so a Tier-3 capture satisfies that gate on the day it is taken and is cited afterwards under §16.13
for the readings it carries, never for its exit status.

**Era, and it is the reason `question` was left alone.** Widening P15's `question` string is a
`probe_set` move, which would close an era for a wording change — §15.59's first step, repeated on
purpose — so the string is unchanged, `probe_set` stays `4317ea5ac187f506`, and **no era closes**
(§13's era law clause 4; the record already carries `field_set` moves that closed nothing, notes
§3.89/§3.90). `field_set` moves four times across this session's Linux Tier-3 line, all at one
`-dirty` commit string: `64eb252e565113b2`, then `f18630922c4eecc7` with plan §18 item 73's software
cross-check (571 → 573 leaf paths, the two added being that cell for each named port), then
`9a75a9f83a83a617` as this reading lands, then `b73dba8f32301a84` as `released_intact` joins it
(notes §3.146, §3.147). **A `-dirty` artifact named by its commit alone therefore names four
different trees**, and the digest is the only field that separates them. `expectations/linux.jq` and
`expectations/macos.jq` gained their clauses in the same change as the fields they type-check, and
both admit a P15 row carrying no `wire_flow_control` block at all — which is what keeps a bench with
no certified pair, and every capture that predates the key, legal rather than red.

**What this does not decide, stated because they are the obvious over-reads.** It says nothing about
`cdc_acm` as a class or `ftdi_sio` as a class: one port, one driver, one peer, one rate, one
payload. It does not reopen §15.53 or §15.61 — an accept-then-drop port is still refused, on the
same predicate, in the same position, for the same reason. And it does not promise a refusal later:
if a wire reading is ever to gate anything, that is a fresh decision needing its own evidence, and
the ground for declining one here is not that the evidence is thin but that the pre-check is not the
instrument.


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
*[**Annotated 2026-08-21 (plan §18 item 108, §15.72):** item 42 is **EXECUTED 2026-08-12**
(notes §3.88) — `BlockingReader` became `BlockingWorker`, the loss counter became optional
*structurally* rather than by convention, and all three call sites rebased — so the sentence above
points at a closed item, never at owed work.]*

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
*[**Annotated 2026-08-21 (plan §18 item 108, §15.72): the destination record is no longer owed.**
Plan §18 item 29 is **EXECUTED 2026-08-12**, answered from the git history rather than from memory,
which is what the item asked for: `scripts/lib/wait-for.sh`,
`scripts/validate/phase0/license-gate.sh` and `scripts/validate/phase8/external-codec.sh` were all
three deleted in one commit — `563fb9c`, 2026-07-24 — which is this entry's own execution, and the
same commit created their successors (`itest/tests/p0_license_gate.rs`, a rewritten
`itest/tests/p8_external_codec.rs`, and the `wait_until`/`wait_for` helpers in the harness). The
"is owed" wording above is a dated record of the filing.]*

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
