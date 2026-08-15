# vmcell requirements: one predicate, and the artifacts it hides

## What this document is

A considered feature request, written from the consumer position, for the platform capabilities
`vmcell` would need before a micro-VM ("a cell") could run anything this project wants to test —
including a live systemd and a full `debian-latest` userland — instead of only the minimal image
vmcell bakes for itself.

The goal it is written to, in the operator's words:

> The platform should allow us to test anything we want, including systemd and even a
> `debian-latest` image (modulo straightforward repacking logic). Of course, everything needs to be
> pay-for-what-you-use.

**It changes no decision today.** The root-needing work is routed to CI's root arm
(`.github/workflows/ci.yml:399`–`:410`), and that arm is carrying its share: plan §18 item 31's (c)
ran green there on 2026-08-13 — 6 passed, 0 failed, the probe under `DynamicUser=yes` at a transient
uid (CI run 31695823765, reproduced at 31877969760 on 2026-08-15;
`itest/tests/p8_packaging.rs:176`–`:188`) — and `SNX_PACKAGING_ROOT=required` has gated the step
since. Nothing below is needed to keep that working. This is the list of what a cell would have to
become before it were the *better* instrument, and it is written so that vmcell can evaluate each
item as a platform capability on its own merits.

**Every request here is framed as a general capability with a stable interface**, justified by more
than the one use that prompted it, and specified so that a cell which does not ask for it pays
nothing. Where the obvious fix is a one-way global switch — swapping the erofs root for ext4 is the
standing example — the request says so explicitly and proposes the opt-in form instead. A request
only this project could ever want is a bad request; each one below names what else it unlocks.

Every capability discussed is marked **SHIPPED**, **DESIGNED** (specified in vmcell's own documents,
not in the tree) or **ABSENT**.

## Provenance

| Thing | State when measured |
|---|---|
| `serial-nexus` | read at `0574dea`, branch `main` |
| `vmcell` | `079c17f`; design read: `docs/82-claude-opus-design-v32.md` (v32), plus `README.md`, `AGENTS.md`, `docs/requirements.md` and `docs/todo.md` |
| Guest kernel configs read | `target/vmcell-artifacts/vmlinux.config`, `vmlinux-6-12-94.config`, `vmlinux-6-6-143.config`, `vmlinux-usbhost.config` (all host-`make` builds) |
| Guest userland read | the digest-pinned base. `pins.json:28`–`:31` pins the **image** as `docker.io/library/debian@sha256:a617c1cdde36a7e0194b2f07dff669e1753c03c3205356b94f9f350b0f9a57d1`; the OCI cache is keyed by **layer** digest, and the one layer present is `sha256-e95a6c7ea7d49b37920899b023ecd0e32796c976c1748491f76cae53ba86d13a` (29,785,419 bytes, 3262 entries, listed with `tar -t`). Naming only one of those two digests, as the previous version of this document did, describes the wrong object |
| Date | 2026-08-15 |

Line numbers are anchors, not contracts; they drift, and they drifted between the previous version of
this document and this one — every citation below was re-verified against the trees named above, and
the corrections are called out where they land. Nothing here was run inside a cell — **no cell was
booted for this document** — so every claim is a claim about *code and artifacts as read*, and each
requirement's verification section says what would have to be executed to convert it into a
measurement.

One document in the workspace covers adjacent ground and is cited once for its caveat, not its
conclusions: `usb-teleporter/docs/4-claude-fable-repo-research.md` is a research report on structuring
a workspace that consumes several sibling workspaces. Its §0 states plainly that the repositories
"could not be inspected" and that its recommendations "are grounded in Cargo best practice and are
written so they hold regardless of those details". It is therefore **generic advice, not grounded
findings**, and nothing in this document rests on it. The sibling
`usb-teleporter/docs/feature-requests-vmcell.md` is a prior request set written under the same
generality directive; the structure here deliberately follows it.

---

## The reframe: one predicate conflates two independent facts

The previous version of this document asked for four things and declined a fifth, systemd, on the
ground that keeping the control plane while surrendering PID 1 "is not a feature request, it is a
different product". That reasoning was sound. Its premise was not.

The premise was that **control-plane availability and init identity are the same fact**. In the tree
they are the same *predicate*, and one line encodes it twice:

```rust
control_plane_disabled: cfg.init.is_some(),
```

— `crates/vmcell/src/orchestrator.rs:1527` (the `start` path) and `:1721` (the `restore` path),
stored in the field declared at `:662` and documented at `:659`.

Untie those and systemd, `debian-latest`, ext4 and a consumer's own guest handler stop being special
cases that each need their own mechanism, and become ordinary **artifacts** the platform already
knows how to register, key, cache and validate. That is this document's thesis, and the six requests
after R1 are what it takes to make an artifact a first-class thing rather than a hardcoded one.

**The inventory, verified site by site.** Eight places in the tree key on `cfg.init`. Exactly one of
them is about init identity:

| Site | What it does | Which fact is it really about |
|---|---|---|
| `config.rs:392`–`:405` | builds the `init=` cmdline token (`DEFAULT_INIT = "/usr/sbin/vmcell-guest-agent"`, `config.rs:446`) | **init identity** — legitimately |
| `config.rs:1555`–`:1562` | `build()` rejects `init` + `snapshotting` | control-plane availability (the post-restore resync runs through the agent) |
| `orchestrator.rs:1480` | skips the control-plane health gate and its re-spawn loop | control-plane availability |
| `orchestrator.rs:1527`, `:1721` | sets `control_plane_disabled` | control-plane availability |
| `orchestrator.rs:1750`–`:1758` | `agent()` returns a typed `Error::Agent` | control-plane availability |
| `orchestrator.rs:1866`–`:1873` | `connect_sessions()` returns the same | control-plane availability |
| `orchestrator.rs:2036`–`:2046` | `snapshot()` refuses | control-plane availability |
| `orchestrator.rs:2237`–`:2239` | `clone_ineligible_feature`'s custom-init arm | control-plane availability |

Seven of the eight ask "can I reach an agent?" and answer it by asking "did the caller set `init=`?".

**The tree already contains the counter-example, deliberately.** `dial_vsock`
(`orchestrator.rs:1935`) does *not* copy the guard, and its own doc comment (`:1890`–`:1900`) says
why: the vsock **device** is attached unconditionally on every backend — "CH's `vsock` create payload
field, Firecracker's `PUT /vsock`, QEMU's device/daemon block, and crosvm's `--vsock cid=` are all
straight-line, none reads `cfg.init`". It is pinned by `dial_vsock_bypasses_the_custom_init_guard`
(`orchestrator.rs:5610`–`:5686`), which drives the real transport through a `MicroVm` whose
`control_plane_disabled` is `true` and asserts the dial reached the wire. So the platform has already
decided, once, that "the caller set `init=`" is the wrong question for one subsystem. R1 is that
decision applied to the other seven.

**Timeouts are not what breaks, and that is worth saying because it is the obvious wrong guess.** The
`Ready` handshake is per-connection and cheap: the guest binds `VMADDR_CID_ANY` on port 5000
(`vmcell-guest-agent/src/main.rs:550`, const at `:523`) and sends `Message::Ready` as the first frame
of each accepted connection (`main.rs:906`, inside `serve_connection` at `:898`), which the host reads
as the handshake (`crates/vmcell/src/agent/mod.rs:808`). The connect budget is caller-supplied and
defaults to 10 s (`orchestrator.rs:1742`–`:1745`, `:1784`; the session path the same at `:1862`,
`:1877`), and the backoff is public, tunable configuration (`config.rs:241`–`:247`, shipped defaults
20 ms floor / 100 ms cap at `:267`–`:268`, clamped at `:288`–`:289`). A guest that takes longer to
come up is a budget away from working. What is not a budget away is a refusal that fires before the
transport is touched.

**And `VmConfig::init` is library-only**, which bounds the blast radius of changing it: there is no
CLI flag anywhere in `vmcell-cli` (zero hits for `init` in the whole crate), and the daemon's
launcher never calls `.init(…)` at all — `crates/vmcell-daemon/src/launcher.rs:208`–`:212` hardcodes
`RootfsSource::Erofs` and builds from `spec` fields that contain no init. So today a REST caller
cannot even *express* the custom-init mode. The predicate this document asks to split is reachable
from exactly one surface, and that surface is Rust.

---

## Request index

| # | Capability | State | Unblocks | Priority |
|---|---|---|---|---|
| **R1** | Agent placement declared, not inferred from `init=` | ABSENT | systemd, and every non-agent PID 1 | **the unlock** |
| **R2** | An artifact registry for kernel, rootfs **and** guest handler | partly SHIPPED (kernels only) | `debian-latest`, custom handlers, R6/R7 | high |
| **R3** | Declared features; a cell's set is an intersection, with provenance | ABSENT | every claim the other six make | **first** |
| **R4** | A two-directional conformance kit (present *and* absent) | partly SHIPPED (present only) | trusting R1/R2/R5 | **first** |
| **R5** | The guest agent as a library; the binary a thin wrapper | partly SHIPPED (reaper only) | R1's `Service` placement | high (largest) |
| **R6** | Repacking usable externally; xattr policy per artifact | partly SHIPPED / ext4 ABSENT | `debian-latest` as shipped | medium |
| **R7** | Reproducibility survives consumer-supplied artifacts | SHIPPED as discipline, ABSENT as a registry rule | citable artifacts | medium |

**Build order, and the reasoning.** **R3 and R4 first, against today's artifact set.** They need no
new artifact, they establish the vocabulary everything else declares in, and they verify the feature
claims vmcell already makes implicitly. That ordering is not tidiness: R1, R2 and R5 are each a claim
that a new configuration works, and a claim is only as good as the kit that checks it. Building the
kit *after* the features it validates means the kit's first job is to certify code written without
it — and the first thing this project's own record says about that shape is that a guard written
alongside the fix it guards tends to pin the fix rather than the property (AGENTS §9's fail-first
rule exists because of it). R3+R4 against the current artifacts also has the property that they can
go red today, on facts already in the tree, which is the only proof that they can go red at all.

Then **R1**, which is the unlock and the smallest of the remaining four. Then **R5**, the largest
piece, which is what makes R1's `Service` placement more than a predicate change. **R6 and R7 ride
on R2** — both are properties of a registered artifact, and neither has anywhere to live until the
registry does.

---

## R1 — Split control-plane availability from init identity

**The capability, in one sentence.** vmcell *declares* where the guest agent runs — `Pid1`,
`Service { port }`, or `None` — instead of inferring "there is no control plane" from "the caller set
`init=`", so the control plane is available whenever an agent is reachable, whatever pid it holds.

**State: ABSENT.** The predicate is `cfg.init.is_some()` at both construction sites.

**The consumer need.** Plan §18 item 31's residue. Four claims in `packaging/README.md` are marked
**unverified** and the file says why in one sentence at `:373`–`:377`: the socket-group and
operators-group recipes and the upgrade `cp` procedure *"want a live systemd to start a unit on"*.
The four rows are the socket-group static-identity recipe (`:325`), the operators-group paragraph
(`:339`), the upgrade procedure (`:344`), and the clause that the daemon actually runs under the
resulting sandbox (`:327`, whose evidence column says plainly "no lane starts it as a service").
Those are claims about systemd's behaviour under `DynamicUser=`, `StateDirectory=` and
`ReadWritePaths=`, and nothing but systemd makes them true.

Under today's predicate, booting systemd as PID 1 costs the entire control plane. The field
documentation states the price itself (`config.rs:88`–`:97`): a custom init *"replaces the agent — so
the VM has no control plane (`MicroVm::agent` fails loud) and cannot snapshot … A custom init also
loses the agent's tmpfs overlay over the RO erofs root"*. And the replacement is not free: the
fail-loud message names the alternatives — *"Observe it via the serial log, a shared directory, an
extra block device, or networking"* (`orchestrator.rs:1752`–`:1757`) — each of which is a whole
harness a consumer would have to write and maintain, for a payload the platform is already able to
talk to.

**Why this is a platform capability, not a bolt-on.** It is vmcell's own stated goal one rank up.
`docs/requirements.md`'s micro-VM feature checklist, item 5, ranks the guest environment:
*"Great: environment perfectly matches an installed Debian flavor, such as server"*; *"Good:
stripped down Debian installation built using supported methods"*. The shipped answer is the Good
tier, and the reason it cannot be the Great tier is not the image — it is that a Debian flavor as
shipped boots systemd, and booting systemd surrenders the control plane. R1 is the piece that makes
item 5's top tier reachable at all.

Other consumers, none of them this project: anyone testing an **init system** (OpenRC, s6, runit, a
hand-rolled PID 1); anyone testing a **container runtime** or a supervisor, where the payload must
own pid 1 of the VM to be the thing under test; anyone testing a **distro image as shipped** rather
than as repacked, which is the only way to catch a defect that lives in the distro's own boot
sequence. And one inside vmcell: the daemon cannot express the custom-init mode over REST today
(`vmcell-daemon/src/launcher.rs:208`–`:212`), so a declared placement is also what makes the mode
expressible on the platform's own third entry surface.

**Interface sketch** (naming vmcell's to choose):

```rust
/// Where the guest agent runs. Independent of *what* the kernel starts as pid 1.
pub enum AgentPlacement {
    /// Today's default: the kernel starts the agent, `init=/usr/sbin/vmcell-guest-agent`.
    Pid1,
    /// The guest's own init starts the agent; the host dials `port`. `VmConfig::init`
    /// names the init, and the two facts are stated separately.
    Service { port: u32 },
    /// No agent anywhere. Today's `init=` + no control plane, said out loud.
    None,
}
```

Three re-keyings, each mechanical:

1. `control_plane_disabled` becomes `matches!(placement, AgentPlacement::None)`. The guards at
   `orchestrator.rs:1750`, `:1866` keep their bodies and change their trigger.
2. **The health gate re-keys the same way, and this is the site most likely to be missed.**
   `orchestrator.rs:1480` skips `verify_control_plane` and its bounded re-spawn loop whenever
   `cfg.init.is_some()`, with a correct rationale for `None` (a custom-init QEMU VM would "re-spawn
   to exhaustion against a listener that never comes up"). Under `Service` the gate *should* run —
   an agent is coming up, and a wedged `vhost-device-vsock` bring-up is precisely what the probe
   exists to catch.
3. **The snapshot/clone predicate re-keys on the question it is actually asking**: not "is init
   custom" but *"is the post-restore resync reachable"*. `snapshot()` (`:2036`),
   `clone_ineligible_feature` (`:2237`) and `build()`'s exclusion (`config.rs:1555`) all cite the
   same reason — the mandatory clock / CSPRNG / MAC-IP resync runs through the agent — and that
   reason is false for `Pid1`, false for `Service`, true for `None`.

The `Service` variant needs the port agreed on both sides. Today `VSOCK_PORT` is a **private** const
in the guest (`vmcell-guest-agent/src/main.rs:523`) mirrored by `pub const AGENT_VSOCK_PORT: u32 =
5000` on the host (`crates/vmcell/src/vmm/mod.rs:1193`), whose doc calls the guest const "its mirror
on the other side of the boundary" — so 5000 is duplicated, not shared. The cmdline is the existing
channel and the idiom is already reserved: `is_reserved_cmdline_arg` (`config.rs:509`) treats every
`vmcell_`-prefixed token as guest-agent-trusted (`config.rs:498`–`:500`), and the agent already
parses `vmcell_share=`, `vmcell_accept_poll_ms=` and `vmcell_rebind_idle_ms=` out of `/proc/cmdline`
(`main.rs:312`, `:313`–`:353`, `:416`–`:429`). A `vmcell_agent_port=` token rides that idiom exactly.

**This amends a test-pinned fail-loud law, and is presented as an amendment.**
`agent_fails_loud_when_control_plane_disabled` (`orchestrator.rs:5561`–`:5598`) pins that `agent()`
returns a typed `Error::Agent` whose message contains `"custom init"`, and its comment states the
law: fail loud immediately "instead of blocking for the full connect timeout on a listener that will
never answer".

**The law is right and the amendment keeps it.** What changes is the predicate it is keyed on.
Under the amendment:

- `AgentPlacement::None` keeps today's behaviour exactly, and this test survives with its message
  assertion re-worded from the init spelling to the declared placement.
- `AgentPlacement::Service { .. }` must **not** take the arm — and that is the test the current one
  cannot have, because today no configuration exists in which a custom `init=` and a reachable agent
  coexist. The guard has therefore never been shown to *discriminate*; it has only ever been shown
  to fire. Adding `Service` is what gives it a negative case, which makes the amendment a
  strengthening of the law rather than a relaxation of it.
- The shape to copy is already in the file: `dial_vsock_bypasses_the_custom_init_guard`
  (`:5610`–`:5686`) is a guard-shaped test for a path deliberately *not* keyed on the predicate, and
  it states its own red-on-inverse ("adding the `control_plane_disabled` early-return to `dial_vsock`
  makes this fail with the custom-init Agent error instead").

Per this project's own amend-first order (AGENTS §5), the reasoning belongs in vmcell's decision
register *before* the code moves, with the superseded position annotated rather than rewritten. This
is a design amendment, not a patch.

**What must not regress.**

- **The fail-loud law itself.** A placement with no reachable agent must still return a typed error
  immediately, never block for the connect budget.
- **`dial_vsock` stays unkeyed** (`orchestrator.rs:1935`), and its gate stays green verbatim.
- **`config.rs:392` stays keyed on init identity**, because that one genuinely *is* init identity.
- **Snapshot eligibility stays conservative where it is uncertain.** `Service` + `snapshotting`
  raises a real new question — after a restore re-creates the vhost-vsock device, does the guest's
  init restart the agent, or does the agent's own idle re-bind (`vmcell_rebind_idle_ms`, default
  250 ms, `main.rs:543`) cover it? The honest answer is to keep `Service` + `snapshotting`
  **rejected** until it is measured. That is strictly narrower than today's rejection and no worse
  for anyone.
- **Byte-identical default.** `AgentPlacement::Pid1` with `init: None` must emit the same cmdline
  (`config.rs:399`–`:405`) and take the same code path everywhere.

**Pay-for-what-you-use.** The default placement is `Pid1`, which is what every cell gets today. No
new cmdline token is emitted unless a non-default placement asks for one; no new frame rides the
wire; no boot cost changes. A cell that never names a placement cannot tell R1 landed.

**How the requester would verify it.** The full proof needs R5, because `Service` needs an agent that
can run as a service. The **predicate half is verifiable alone, and is worth doing first**: build a
cell with `AgentPlacement::Service { port: 5000 }` while still starting the agent as pid 1, and
assert that `agent()` returns a client rather than the typed refusal, that `connect_sessions()` does
the same, and that `orchestrator.rs:1480`'s health gate *ran* — three assertions that today's tree
cannot satisfy under any configuration. Then boot with `AgentPlacement::None` and assert the refusal
arrives without touching the transport. The fail-first proof is the inverse in both directions:
re-key the guard back onto `cfg.init.is_some()` and the `Service` case must go red, not merely slow.

---

## R2 — An artifact registry for all three kinds

**The capability, in one sentence.** Kernels, rootfs images **and** guest handler binaries are all
registered by digest into one registry and selected per cell, extending the shape the kernel registry
already has rather than inventing a second one.

**State: partly SHIPPED — for one of the three kinds.**

**What exists, and it is most of the design.** The kernel registry is real and its keying discipline
is already content-addressed:

- `KernelRegistryEntry` (`crates/vmcell/src/artifact/mod.rs:1053`; `label` `:1056`, `fragments`
  `:1060`), whose doc at `:1045`–`:1050` states the invariant that matters — **the label alone fully
  determines the build**.
- `resolve_kernel_registry` (`mod.rs:1081`) reads `doc["kernels"]` out of the baseline+overlay merge;
  `pins_overlay_or_env` (`:1201`) gives the override order (explicit `--pins` beats `$VMCELL_PINS`
  beats the committed baseline, which is embedded with `include_str!` at `:627`).
- The naming and keying laws are one apiece: `fragment_pin_key` (`kernel.rs:67`), `kernel_pin_key`
  (`:89`), `kernel_artifact_key` (`:111`), `kernel_filename` (`:234`, `vmlinux-<label>`),
  `kernel_label_from_filename` (`:253`, the inverse), `resolved_config_path` (`:269`),
  `config_artifact_key` (`:283`), plus `reject_sanitized_label_collision` (`mod.rs:1117`) and a
  deterministic build order (`sort_kernel_registry`, `:1143`).

`pins.json` today carries three kernel labels (`6.6.143`, `6.12.94`, `usbhost`) and five fragments
(`KASAN`, `KCOV`, `LOCKDEP`, `SLUB_DEBUG` at `:33`–`:36`, and **`USBHOST` at `:37`** — a fifth the
previous version of this document did not know about, which matters for the USB discussion under
"Considered and not requested").

**Where it stops — three things, each verified.**

1. **Rootfs is a singleton, not a registry.** `pins.json:28`–`:31` is one `rootfs` object with
   `image` + `digest`. There is no `rootfs-<label>` naming law, no `rootfs_artifact_key`, no
   `resolve_rootfs_registry`. A second userland is not a second entry; it is a fork of the pins file.
2. **The guest handler is not an artifact at all.** `GuestToolsStage` builds `vmcell-guest-tools`
   from vmcell's own workspace unconditionally — `guest_tools_closure_hash(workspace_root())` in both
   `cache_key` and `run` (`crates/vmcell/src/artifact/guest_tools.rs:39`, `:59`), then
   `cargo build --release --target x86_64-unknown-linux-gnu -p vmcell-guest-tools` with
   `current_dir(ws_root)` (`:66`–`:75`). `GuestAgentStage` does the same (`guest_agent.rs:52`–`:63`).
   There is exactly one prebuilt escape hatch, `--agent-musl`, and it covers the agent only.
3. **Registration is declarative but selection is eager.** `vmcell build-kernels` builds *every*
   label in the merged registry with no per-label filter (`vmcell-cli/src/main.rs:435`–`:458`,
   dispatch `:617`–`:660`). The single-label entry point exists only in the library
   (`build_labelled_kernel`, `artifact/mod.rs:1239`–`:1271`). Content-addressed stages turn a warm
   rebuild into a cache hit, but the roster is still all labels — so registering a `debian-latest`
   userland today would mean building it on every registry-shaped invocation, which is precisely the
   cost this request must not impose.

**Why this is a platform capability, not a bolt-on.** It is `docs/requirements.md`'s item 6 shape
applied to its item 5. Item 6 ranks the guest *kernel* and vmcell answered it with a registry; item 5
ranks the guest *userland* identically and it has a singleton. The asymmetry is not a decision
recorded anywhere — it is the difference between the axis someone needed to vary and the axis nobody
had yet.

Other consumers: anyone running a **distro matrix** (trixie against sid against a derivative);
anyone A/B-ing a **kernel and a userland together**, which today is impossible because only one axis
is registrable; anyone whose guest-side helper is not vmcell's multicall binary — which vmcell's own
§17 already concedes is coming ("USB-passthrough guest-side coverage beyond enumeration + one class
smoke … is consumer territory") and which `vmcell-guest-tools`' four applets are themselves an
instance of. The pattern vmcell already promotes for kernel fragments —
`examples/downstream-kernel/` proving an out-of-tree consumer can extend the pins registry without
forking it — is the same proof shape, and it would extend to two more kinds for free.

**Interface sketch** (naming vmcell's to choose). The pins schema gains two namespaces shaped like
the one it has:

```jsonc
// pins.json / the VMCELL_PINS overlay
"rootfs": {                                  // today: a bare {image, digest} singleton
  "default":        { "image": "docker.io/library/debian",
                      "digest": "sha256:a617c1…" },
  "debian-systemd": { "image": "docker.io/library/debian",
                      "digest": "sha256:…",
                      "xattrs": "preserve" }          // R6
},
"handlers": {
  "default": { "build": "workspace:vmcell-guest-tools" },
  "acme":    { "digest": "sha256:…" }                 // R7: a digest, never a path
}
```

```rust
VmConfig::builder(kernel, rootfs)
    .rootfs_label("debian-systemd")
    .handler_label("acme")
```

Naming laws mirror the kernel's exactly — `rootfs_artifact_key` (`rootfs-<label>`), a
`rootfs-<label>.erofs` filename law and its inverse — so a stale rootfs artifact is as detectable as
a stale `vmlinux-<label>`, by the same code shape.

**What must not regress.**

- **The canonical artifacts stay byte-identical** for a cell that names no label. `default` must
  resolve to today's inputs and fold to today's cache key.
- **The five cache-key rules (§10.2) and the identity folds.** A new fold gets a `STAGE_VERSION`
  bump; the recorded v20 precedent — an identity-fold change without one, serving stale images —
  applies to every kind added.
- **`is_reserved_injection_path` stays authoritative** (`artifact/rootfs/mod.rs:83`, gated by
  `is_reserved_injection_path_covers_every_vmcell_dest` at `:806`). Registering more artifacts must
  extend nothing about what a consumer may shadow.
- **Determinism.** A label resolves to a digest, never a mutable tag. `oci2erofs` already refuses a
  tag and demands `IMAGE@sha256:<64 hex>` (`vmcell-cli/src/main.rs:1012`–`:1020`); that rule becomes
  the registry's rule (R7).

**Pay-for-what-you-use.** **Registration is lazy: a registered artifact is not built until a cell
selects it.** A cell naming no label takes today's path byte-for-byte. This is the one place R2 asks
the *kernel* registry to change as well — `build-kernels`' eager all-labels roster becomes "build
what is selected", with `--all` preserving today's behaviour — because otherwise adding a
`debian-latest` entry to a shared pins file taxes every build in every workspace that reads it.

**How the requester would verify it.** Register a second rootfs label pointing at the *same* digest
as `default`, build both, and assert the two artifacts are byte-identical and that the `default`
build's cache key did not move — the empty-change-folds-to-the-current-key property, which is the
only assertion that catches a registry change quietly re-keying every existing artifact. Then
register a `debian-latest` label and assert that a build which selects nothing **does not build it**;
the fail-first proof is that removing the laziness makes that assertion red rather than slow.

---

## R3 — Declared features, and a cell's features as an intersection

**The capability, in one sentence.** A cell's feature set is computed as the **intersection** of what
its artifacts declare and what its VMM backend supports, with **provenance on every removal**, so a
consumer reads `snapshot: unavailable (rootfs "debian-systemd" declares no-snapshot)` rather than a
bare `false`.

**State: ABSENT as an intersection. The backend half is SHIPPED and is a good model.**

**What exists.** `VmmCapabilities` (`crates/vmcell/src/vmm/mod.rs:1066`) is nine bools —
`snapshot_restore`, `lazy_restore`, `virtio_fs_shares`, `unprivileged_vhost_user_net`, `nested_virt`,
`virtio_console`, `restore_rotates_host_paths`, `disk_io_throttle`, `usb_host_passthrough` — produced
as one exhaustive literal per backend (`cloud_hypervisor.rs:838`–`:860`,
`vmcell-firecracker/src/lib.rs:75`–`:116`, `vmcell-qemu/src/lib.rs:1379`–`:1431`,
`vmcell-crosvm/src/lib.rs:288`–`:327`). It is deliberately **not** `#[non_exhaustive]`, and the doc
says why (`:1058`–`:1064`): "a new capability must force every backend to declare its stance on it (a
compile error until it does), not default silently to `false`." The trait doc (`:1123`–`:1129`)
states the reported-never-assumed law, and three shared predicates enforce it —
`reject_unsupported_console` (`:802`), `reject_unadvertised_capabilities` (`:852`),
`reject_usb_host_devices` (`:888`) — each taking `caps: &VmmCapabilities` rather than hardcoding a
refusal.

The `usb_host_passthrough` comment (`:1108`–`:1120`) is the closest thing in the tree to this
request already written down: *"Deliberately **narrow** (USB, not a generic `host_device` flag): the
flag claims exactly what is live-validated; the flag + config + typed-refusal *pattern* is the part
that generalizes to other device classes."*

**Where it stops — four things.**

1. **There is no intersection anywhere.** Three descriptors exist and none of them meet:
   `VmmCapabilities` (backend), `HostCapabilities` (`crates/vmcell/src/hostcaps.rs:42`–`:59` — host
   privilege and cgroup delegation), and **nothing at all for an artifact**. The nearest artifact-side
   analogues are a static clause→symbol map (`vmcell-artifact-validator/src/classify.rs:195`–`:210`)
   and two runtime self-checks the guest agent writes to the console as *text*
   (`vmcell-guest-agent/src/main.rs:384`–`:401`), which the classifier then parses back out of the
   serial log (`classify.rs:103`). Never structured, never returned as data.
2. **A missing feature yields a free-form string, not provenance.** `Error::Unsupported { vmm:
   String, feature: String }` (`crates/vmcell/src/error.rs:80`–`:87`) carries the pair and nothing
   else — no source-of-removal, no remediation. Meanwhile `Error::CapabilityUnavailable { op, needed
   }` (`error.rs:98`–`:107`) *does* carry remediation, and is used only on the host axis. **vmcell
   already has the richer shape; it is applied to one axis of three.**
3. **The vocabulary is convention, not type.** A naming norm called N-VMM-1 — "the feature string IS
   the `VmmCapabilities` field name" — is asserted in comments at `vmm/mod.rs:837`, `:860`, `:867`,
   `:882`, `:896` and eight further sites, and honoured by roughly half of them. The rest are prose:
   `"snapshot with a vhost-user device"` (`cloud_hypervisor.rs:83`), `"boot after restore (a restored
   VM is resumed, not booted)"` (`:875`), `"concurrent zygote fan-out (backend re-binds baked host
   paths verbatim; §9.4, use one clone at a time, or the CH tier)"` (`zygote.rs:233`–`:235`), plus
   two runtime-composed forms (`orchestrator.rs:1638`, `zygote.rs:355`–`:357`).
   **The consequence is visible in the tree and it is a shape this project has a name for**: matchers
   degrade to substring tests — `feature.contains("vhost-user")` (`cloud_hypervisor.rs:1775`),
   `feature.contains("segment")` / `"custom init"` / `"USB"` (`zygote.rs:716`, `:754`, `:770`),
   `feature.contains("read-only")` (`fs.rs:834`). A substring assertion over a free-form string is
   AGENTS §3's "assertion strictly weaker than the comment above it" register, structurally: the
   comment names a feature, the code accepts any message containing a fragment of its spelling.
4. **A consumer cannot demand a feature at construction.** `VmConfigBuilder::build()`
   (`config.rs:1488`) never sees a `VmmCapabilities` — there is no backend parameter — and validates
   only config-internal shape, returning `Error::Config(String)`. The refusal arrives later, at
   `Vmm::create()` or `restore()`, per backend. Every `require_*` in the tree is test or bench
   harness: `require_cap!` (`crates/vmcell/tests/common/mod.rs:242`–`:257`),
   `require_snapshot_restore_capable` (private, `vmcell-qemu/src/lib.rs:619`), `require_artifacts`,
   `require_privileged_net`, `require_preconditions`.

**Why this is a platform capability, not a bolt-on.** The intersection is the only honest answer once
R2 exists: the moment a rootfs and a handler are selectable, "what can this cell do" stops being a
property of the backend and becomes a property of the *combination*, and there is no correct place
for that computation except the platform. And the platform has already been bitten by the shape once:
`config_has_vhost_user_device` (`vmm/mod.rs:927`) exists as the ONE shared predicate because the
former per-backend copies had diverged — the Firecracker copy never grew the virtio-fs-rootfs term
the CH copy carried (`:909`–`:914`). An un-centralized intersection over three sources would
reproduce that divergence three ways.

Other consumers: any harness that must **skip honestly** rather than fail — `require_cap!`'s own
comment names the hazard, *"A `require_cap!` skip is an invisible nextest PASS"*
(`crates/vmcell/tests/nested_virt.rs:12`–`:13`), and a declared set with provenance is what turns an
invisible skip into a named one; anyone doing a **backend matrix**, where the interesting cell is the
one a specific pairing removes; anyone generating **documentation or a capability report** for a cell
they did not configure.

**Interface sketch** (naming vmcell's to choose):

```rust
/// A versioned enum vmcell owns. Not a string, and not `#[non_exhaustive]`, for the same
/// reason `VmmCapabilities` is not: adding a variant must force every declarer to state a
/// stance rather than defaulting to absent.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Feature { Snapshot, Restore, VirtioFsShares, NestedVirt, XattrPreserved, PosixAcl, /* … */ }

pub enum Source { Backend(&'static str), Rootfs(String), Kernel(String), Handler(String), Host }

/// Why a feature is not in the set. Never a bare `false`.
pub struct Removal { pub feature: Feature, pub by: Source, pub reason: &'static str }

pub struct FeatureSet { /* … */ }
impl FeatureSet {
    pub fn has(&self, f: Feature) -> bool;
    /// `None` when present. `Some(Removal)` names who removed it and why.
    pub fn why_absent(&self, f: Feature) -> Option<&Removal>;
}

// A consumer DEMANDS at construction and gets a named refusal, not a use-site surprise:
VmConfig::builder(kernel, rootfs).require(Feature::Snapshot)?;   // Error::Unsupported { by, reason }
```

Four clauses, each independently arguable:

1. **Unknown feature names are errors, not absences.** This is the load-bearing one. Absence is the
   silent direction: a typo in a declaration that reads as "unsupported" produces a cell that
   quietly does less, and every downstream check passes because nothing claimed the feature. An
   enum makes the typo a compile error at the declaration site; a strict parse makes it a hard error
   at the pins-overlay site — the shape `KconfigValues::parse` already uses, rejecting any non-blank,
   non-comment, non-`CONFIG_x=v` line precisely because "a silently-empty parse would blame every
   assertion on 'symbol absent'" (`vmcell-artifact-validator/src/kconfig.rs:55`–`:59`).
2. **Provenance on every removal**, carried as data. The seed already exists:
   `clone_ineligible_feature` (`orchestrator.rs:2214`) is the one predicate in the tree that returns
   a *reason* rather than a bool, and its doc states the principle — "a typed refusal names the
   feature it is about, not a paraphrase" (`:2211`–`:2213`). R3 asks for that shape everywhere and
   with the source attached.
3. **A consumer can demand at construction.** `build()` gains an optional capability argument or a
   `require` list resolved at `MicroVm::start`, so "this cell cannot snapshot" is answered before the
   VM boots rather than at the first `snapshot()` call.
4. **Granularity is decided up front, and the decision is recorded.** A feature that later splits —
   `Snapshot` into `Snapshot` + `Restore` — breaks every declaration in every consumer's overlay, and
   the tree already contains the evidence that these two are not the same thing: Firecracker has
   `snapshot_restore: true` but `restore_rotates_host_paths: false`, which is single-lineage-only
   (§8.4), and crosvm the same. So the split is live today and should be made before the vocabulary
   ships, not after.

**What must not regress.**

- **The exhaustive-literal law.** Adding a `Feature` variant must remain a compile error for every
  declarer, which is why the enum is not `#[non_exhaustive]` — the same reason `VmmCapabilities` is
  not (`vmm/mod.rs:1058`–`:1064`).
- **The narrow-name doctrine.** `usb_host_passthrough`'s comment is the rule: a `Feature` variant
  claims exactly what is validated, never a generalization of it.
- **The one-shared-predicate law.** The intersection is computed in one place. A second copy is the
  H-VMM-3 divergence again, with more sources to diverge across.
- **Existing typed refusals keep working.** `Error::Unsupported { vmm, feature }` stays constructible
  from a `Removal`, so no call site loses its message.

**Pay-for-what-you-use.** Computing an intersection is arithmetic over small sets at construction;
nothing boots, nothing dials, no artifact is read that was not already resolved. A cell that never
calls `why_absent` or `require` pays the same as today.

**How the requester would verify it.** Declare a rootfs artifact as no-snapshot; build a cell on a
backend whose `snapshot_restore` is `true`; assert `has(Feature::Snapshot)` is false and
`why_absent(Feature::Snapshot)` names the **rootfs** rather than the backend. Then run the same
artifact on a backend whose `snapshot_restore` is `false` and assert the removal names the
**backend** — the two together are what prove the intersection is an intersection and not a rename
of the backend flags. Then misspell a feature in the overlay and assert a hard error naming the
token; the fail-first proof is that removing the strict parse turns that case green, which is the
whole hazard.

---

## R4 — A two-directional conformance kit

**The capability, in one sentence.** The kit checks both directions: a **present** feature that does
not work is an **error**, and an **absent** feature that *does* work is a **warning** — with a third
state, `unverified`, for absences that cannot be decided, so an undecidable absence is visible rather
than counted as a pass.

**State: partly SHIPPED. The present direction is real and good; the absent direction does not exist
at the data plane.**

**What exists, and it is a strong base.** `vmcell-artifact-validator` is named the conformance kit in
vmcell's own `AGENTS.md:21`–`:22` and `README.md:14`, and is downstream contract surface under §10.4.
Its entry point is `validate(&ArtifactSet, &ValidationOptions) -> Result<ValidationReport>`
(`src/lib.rs:265`–`:311`); it boots real micro-VMs (`:290`) rather than reading configs; it runs 16
checks across three ordered levels (`Core ⊂ Extended ⊂ Full`, `lib.rs:97`–`:121`); and it refuses to
emit a green all-skipped report — a missing kernel or rootfs is `Error::Artifact` and a missing
`/dev/kvm` is `Error::CapabilityUnavailable { op: "artifact validation", needed: "/dev/kvm (no VM can
boot to verify the contract)" }` (`lib.rs:269`–`:288`).

Crucially, **two of R4's three states already ship**: `CheckStatus::{ Pass, Fail(String),
Skip(String) }` (`lib.rs:156`–`:164`) — a skip is never a pass and always carries a reason, with
`failures()` (`:217`), `skipped()` (`:224`) and `into_result()` (`:241`) keeping them separable. And
failure messages are never bare timeouts: every reporting arm routes through
`classify::explain_boot_failure_of` (`classify.rs:296`) or `explain_without_serial` (`:337`).

**Where it stops — three things, all verified.**

1. **Nothing verifies an absence at the data plane.** No check boots a backend with `nested_virt:
   false` and asserts `/dev/kvm` is missing in-guest; none boots with `virtio_fs_shares: false` and
   asserts no virtiofs mount; none boots a kernel built *without* the IKCONFIG fragment and asserts
   `/proc/config.gz` is absent. What exists instead are **capability-honesty pins** — one per flag,
   asserting each backend's advertised value including the falses (`crates/vmcell/tests/
   nested_virt.rs:16`–`:42`, `usb_passthrough.rs:56`) — and **typed-refusal tests** that assert
   `create()` refuses before a VM boots (`usb_passthrough.rs:97`ff, refusal asserted at `:124`–`:130`
   with QEMU as positive control, and the comment at `:131` noting no cgroup is touched because "the
   refusal returns first"). Both assert what the *descriptor says* and what the *guard does*; neither
   asserts what the guest *is*.
2. **The opposite move is what the harness makes today.** `require_cap!`
   (`crates/vmcell/tests/common/mod.rs:242`–`:257`) skips the leg entirely on an incapable backend,
   and the comment at `nested_virt.rs:12`–`:13` names the hazard exactly: *"A `require_cap!` skip is
   an invisible nextest PASS, so if a backend's `nested_virt` flipped false the nested_virt leg would
   go dark silently."* The honesty pins are the mitigation, and they pin only the declaration.
3. **vmcell has already recorded one instance of this document's central failure mode, in its own
   justfile.** `justfile:177`–`:193` states that before the `just test-validator` recipe existed, no
   invocation in the tree selected the battery's tests — "the only proof the battery can go red was
   compiled and skipped." That is the tell this project states as *a gate whose passing output is
   identical to its not-running output* (AGENTS §3), found and fixed independently on the other side
   of the workspace. And one narrower instance is still open by design:
   `level_full_rustdoc_names_exactly_the_shipped_checks` (`lib.rs:392`–`:412`) pins that `Level::Full`'s
   rustdoc names exactly the ids `run_full` records — filed as `level-full-rustdoc-claims-absent-checks`
   (`:384`–`:391`) after the doc promised an egress-proxy check and restore state-rotation assertions
   that were never run — while **Core and Extended are deliberately not gated that way**.

**A live example of why the absent direction matters, found while writing this document.** The
`usbhost` label's published sidecar `vmlinux-usbhost.config` is **byte-identical** to the unlabelled
`vmlinux.config` and to `vmlinux-6-12-94.config` (md5 `ba11c458f87c4594f99ac4b059b663a3` for all
three; `vmlinux-6-6-143.config` differs at `3567d95a3c62ceacbbd40e26b28fcc90`). **This is not a
dropped fragment.** Every symbol `USBHOST` names (`pins.json:37`) is already enabled in the build's
starting configuration — `pins.json`'s `microvm_config` (`:5`) names no USB or HID symbol at all, so
`CONFIG_USB_SUPPORT=y` (`vmlinux.config:3709`) and `CONFIG_USB_XHCI_HCD=y` (`:3736`) come from the
base defconfig. The fragment is a **no-op against this baseline**, and the sidecar therefore cannot
distinguish "the fragment was applied and changed nothing" from "the fragment was never applied". A
positive survival predicate of `examples/downstream-kernel/`'s shape — is the symbol `=y` in the
sidecar? — passes here while the fragment contributed nothing. That is R4's problem in the *present*
direction, live in the tree today, and the same reasoning is why an absence test without a positive
control certifies everything.

**Why this is a platform capability, not a bolt-on.** Once R3 lets an artifact *declare*, the
declaration is a claim, and an unchecked claim is worse than no claim — it is a fact a consumer will
build a fixture on. The kit is the only thing that can convert a declaration into a measurement, and
it must be the platform's because the artifacts are the platform's. Other consumers: anyone shipping
an artifact to somebody else (the kit is what makes "this image supports X" transferable); anyone
running a backend matrix, where the absent direction is what catches a backend that quietly grew a
capability it still advertises as `false` — which is a real class, since `restore_rotates_host_paths`
flipped for QEMU exactly that way (§17, "concurrent QEMU zygote fan-out … is now shipped too").

**Interface sketch** (naming vmcell's to choose):

```rust
pub enum CheckStatus {
    Pass,
    Fail(String),                 // a PRESENT feature that does not work
    Warn(String),                 // an ABSENT feature that does work — under-claiming, not misbehaving
    Unverified(String),           // an absence that cannot be decided; carries why
    Skip(String),                 // shipped today, unchanged
}

pub struct ConformanceOptions {
    /// Absences whose "it works anyway" is already known and dispositioned.
    /// A warning NOT in this set is an error; a warning in it stays a warning.
    pub expected_warnings: BTreeSet<(Feature, ArtifactId)>,
}
```

Four clauses:

1. **Both directions.** Present ⇒ must work ⇒ `Fail` if not. Absent ⇒ must not work ⇒ `Warn` if it
   does, because an artifact that under-claims is a documentation defect, not a runtime one, and
   reddening a suite for it would push declarers toward over-claiming — the exact wrong incentive.
2. **`Unverified` is a real state, not a soft pass.** Proving a negative is sometimes impractical.
   Snapshot absence is testable by attempting a snapshot; page-sharing absence is not, because no
   in-guest observation distinguishes "the host is not sharing pages" from "the host is sharing them
   and nothing has diverged yet". An honest kit says so per check rather than counting the
   undecidable ones as passes — the same distinction `KconfigValues::get` already draws between
   `None` (olddefconfig dropped the symbol) and `Some(No)` (the author disabled it), which
   `kconfig.rs:131`–`:135` calls out as the crate's central distinction.
3. **Every absence test carries a positive control.** Run the same probe against an artifact that
   *declares* the feature and require it to report "works". Without it an absence test is a constant
   that silently certifies everything — a probe that always answers "absent" passes every absence
   check ever written. The tree already has this discipline in one place and names it: the
   `usb_passthrough` refusal test uses QEMU as the positive control (`usb_passthrough.rs:124`–`:130`),
   and the vendored-vhost script's positive control is the example workspace itself (§10.4).
4. **Warnings need a lifecycle.** Declare the expected-warning set, so a **new** working-absent
   feature is an error while a known one stays a warning until dispositioned. Without the set,
   warnings accumulate until nobody reads them, which is the same terminal state as not emitting
   them.

**What must not regress.**

- **Skip is never pass.** The shipped property (`lib.rs:156`–`:164`, `:224`) survives verbatim.
- **The refuse-to-report-green precondition** (`lib.rs:269`–`:288`). A kit that cannot boot must
  error, not emit an all-skipped green.
- **Failure messages keep their classifier** (`classify.rs:296`, `:337`). A `Warn` gets the same
  treatment — an under-claim with no explanation is a bare bool again.
- **The rustdoc-names-the-checks gate** (`lib.rs:392`–`:412`) extends to Core and Extended rather
  than staying Full-only, since the defect it was filed for is level-independent.

**Pay-for-what-you-use.** The kit **runs on demand, never on cell boot.** It is already shaped that
way — a separate crate, its own entry point, invoked by `just test-validator` or by a consumer's own
call — and R4 must not change that: an absence probe is by construction the most expensive kind
(it boots to prove a negative). Note that vmcell's §17 already records the related gap — `validate()`
has no overall wall-clock budget, only per-check deadlines — and a battery that doubles its check
count makes that gap twice as visible, so R4 should land with the budget rather than before it.

**How the requester would verify it.** Take one declared feature with a decidable absence — snapshot
is the clean case — and run four legs: a declaring artifact on a capable backend (must `Pass`), a
declaring artifact on an incapable backend (must `Fail`, and R3's provenance must name the backend),
a non-declaring artifact that genuinely cannot (must `Pass` as a verified absence), and a
non-declaring artifact that in fact can (must `Warn`). The fourth leg is the positive control for
legs three and four together, and without it leg three is a constant. Then delete the positive
control and assert the suite notices — fail-first proof applied to the kit's own stated property, not
merely to its code.

---

## R5 — The guest agent as a library, the binary a thin wrapper

**The capability, in one sentence.** `vmcell-guest-agent` becomes a library with named
parameterizations plus a thin binary that drives it, so the same agent can be pid 1 of the VM or a
service started by somebody else's init.

**State: partly SHIPPED, and the split exists in name.** `crates/vmcell-guest-agent/Cargo.toml`'s own
comment says "A library (the PID-1 reaper coordination — pure, unit-tested) plus the thin binary that
drives it". In practice `lib.rs` is 643 lines of reaper coordination plus `netif`, and `main.rs` is
2867 lines carrying the mounts, the vsock server, exec, sessions and the power-off policy.

**The consumer need.** R1's `Service { port }` placement is a predicate change; R5 is what makes it a
capability. An agent that can only be pid 1 leaves `Service` with nothing to place.

**This is vmcell's own rule, stated in vmcell's own requirements.** `docs/requirements.md`, "Source
code requirements", items 2 and 3: *"All functionality in library crates, which make up the system
interface to it users"* and *"Binary crates wrapping the library crate to allow quickly trying out
the functionality. The binary crate implements CLI argument parsing and output."* The guest agent is
the one crate where that rule is met for a component rather than for the crate.

**The shipped precedent this project can cite.** `serial_nexus_daemon` is the library;
`daemon-bin/src/main.rs` is 82 lines that parse flags, install a tracing subscriber, and call
`serial_nexus_daemon::run(RunOptions, Registry)` (`daemon/src/lib.rs:100`, `:165`). The extension
surface is two semver'd contracts, and — the detail that keeps the split honest — it is **proven from
the consumer's position on every push**: `examples/external-codec/` is a self-contained workspace
built *from its own manifest*, with path deps standing in for version pins, by
`itest/tests/p8_external_codec.rs`, wired as a dedicated CI job at the MSRV
(`.github/workflows/ci.yml:419`–`:428`). Without that, a library/binary split drifts back into a
binary with a `lib.rs` one convenience at a time, and nothing notices because the in-tree binary keeps
compiling. vmcell already runs exactly this discipline for its *toolkit* contract — design §10.4's
out-of-tree example workspace "is the living consumer that reddens CI when any listed surface drifts"
— so R5 is asking for the guest agent to join a list that already exists.

**The named parameterizations, each verified against the tree.**

1. **Logging.** `main.rs:169` is `tracing_subscriber::fmt::init();`, the first statement of `main()`
   (`:168`) — a global process-wide subscriber, with no injectable seam anywhere in the crate. A
   service-mode agent under a journal, or inside a host process, must be able to decline to install
   one.
2. **vsock port.** `main.rs:523` is `const VSOCK_PORT: u32 = 5000;` — private, and the only
   `std::env::var` in the whole file is `main.rs:1195` (`PATH`). Its host mirror is
   `pub const AGENT_VSOCK_PORT: u32 = 5000` (`crates/vmcell/src/vmm/mod.rs:1193`). R1's `Service {
   port }` needs this configurable on both sides and shared rather than duplicated.
3. **Tools path.** `fn child_path` (`main.rs:1192`) hardcodes `/vmcell-tools` at `:1193`, in both the
   empty-base fallback (`:1198`) and the normal form (`:1200`), applied unconditionally at `:1225`
   for the one-shot and session paths alike. A request-supplied `PATH` is honoured but always
   suffixed behind it. R2's handler label needs this to name where the registered artifact landed.
4. **Tuning, injectable rather than `/proc/cmdline`-only.** `main.rs:312` is the single
   `/proc/cmdline` read; `:416`–`:422` parses `vmcell_accept_poll_ms=` (default 20 ms at `:534`,
   clamped `[1, 10_000]`) and `:423`–`:429` parses `vmcell_rebind_idle_ms=` (default 250 ms at
   `:543`, clamped `[20, 60_000]`). There is no env or API alternative. A service-mode agent does not
   own the cmdline.
5. **Boot-time filesystem assembly, separable.** `main.rs:182`–`:232` is the PID-1 assembly: tmpfs on
   `/mnt` (`:185`–`:191`), overlay on `/mnt/rootfs` (`:203`–`:209`), **`pivot_root(".", "oldroot")`
   at `:221`**, `mount_change("/", PRIVATE|REC)` at `:228`, detach-unmount at `:231`,
   `remove_dir_all` at `:232`. In service mode none of it may run — somebody else already assembled
   the filesystem.
   *A correction the previous version of this document owed:* it said both mounts pass
   `MountFlags::empty()` **and no data string**. The flags half holds; the data-string half does not
   — the overlay passes `Some(c"lowerdir=/,upperdir=/mnt/upper,workdir=/mnt/work")` (`:203`–`:209`)
   and devpts passes `c"gid=5,mode=620,ptmxmode=666"` (`:295`). The load-bearing half is unchanged:
   there is no `nosuid`, `nodev` or `noexec` on the writable layer.
6. **SIGTERM policy a mode, not a constant.** Signals registered at `:438`–`:441` are exactly
   `[SIGCHLD, SIGTERM]`. The reaper loop is `for signal in signals.forever()` (`:454`), drains
   zombies (`:455`), and breaks on SIGTERM at **`:456`–`:458`** — unconditional, no pid check. The
   degraded fallback loop (`:481`–`:484`) exits on the same flag. Both fall through to
   `power_off_never_returns()` at **`:493`**, defined `:502`–`:520`: log, `libc::sync()`,
   `libc::reboot(RB_POWER_OFF)`, then park forever if reboot returns. **So any SIGTERM delivered to
   the agent powers off the guest.** As pid 1 that is exactly right. As a service under systemd it
   means `systemctl stop vmcell-agent` powers off the machine and `systemctl restart` never returns.
   *Stated honestly:* no such unit exists in the tree — the scenario is what R1+R5 create, and this
   is the code that would meet it.

**The regression that would otherwise be silent: in service mode the agent must set
`PR_SET_CHILD_SUBREAPER`.** Verified absent — there is no `prctl` call of any kind in
`crates/vmcell-guest-agent/`, and the only `SUBREAPER` hits repo-wide are vendored kernel headers
under `examples/downstream-kernel/target/`. As pid 1 the agent does not need it: every orphan
reparents to pid 1 by definition. In service mode it does, and the failure mode is the worst kind —
a double-forking payload reparents to systemd, systemd reaps it, and the agent's `wait_for(pid)`
blocks on a status that will never be recorded. The host sees a hung `exec`, not an error.

**And the machinery around that is not orphan machinery — which is the distinction a service form is
most likely to get wrong.**

- **The exec paths deliberately never call `child.wait()`**, and say so: `main.rs:1337`–`:1339`,
  *"Claim this pid's exit code from the single shared reaper; no `child.wait()` here, so the reaper
  cannot have its status stolen (the false-127 race)"*, repeated for the session path at
  `:1677`–`:1678`. The only `child.wait()` in the file is at `:2845`, in a test.
- So the exit code comes from the **reservations map** (`lib.rs:102`, `reservations: HashMap<u32,
  u64>`) keyed against a **pre-spawn epoch** (`ReaperCoordinator::pre_spawn_epoch`, `lib.rs:160`),
  captured at `main.rs:1259` immediately before `cmd.spawn()` at `:1260`, reserved at `main.rs:1316`,
  and claimed by `wait_for` (`lib.rs:311`), which accepts a status only if it was recorded strictly
  after the reservation epoch (`lib.rs:320`, `:330`–`:333`). Sessions do the same
  (`main.rs:1738`/`:1874`, claimed at `:1690`). **That is pid-reuse correctness for children the
  agent spawned itself, and it is needed under any init.** It must be carried into a service form
  intact.
- **What *is* orphan machinery** is `DEFAULT_MAX_REAPED_STATUSES: usize = 1024` (`lib.rs:55`), whose
  own doc (`:50`–`:54`) says it exists because "As PID 1, the guest agent reaps re-parented
  grandchildren that no exec waiter will ever claim", pruned by generation in `ReaperInner::record`
  (`lib.rs:110`–`:127`, retain predicate `:122`–`:124`). Under systemd *without* the subreaper bit
  those grandchildren never arrive and the bound is dead weight; *with* it they arrive again and it
  is load-bearing again. Its necessity is a function of the placement, which is the same reason
  SIGTERM policy should be a parameter.

Two further facts confirm this is a genuine parameterization rather than a flag: `main()` (`:168`)
takes no arguments and never inspects `std::env::args()`, and there is no `getpid`, `std::process::id`
or `pid == 1` check anywhere in the crate — every path assumes pid 1 unconditionally, including the
fatal-mount `return Err` that panics init and the "PID 1 must never exit" comments at `:488`–`:492`
and `:547`–`:548`.

**Why this is a platform capability, not a bolt-on.** Beyond vmcell's own source-code rule: the
agent is the platform's *contract with the guest*, and today that contract can only be honoured by a
process the kernel starts. Every consumer whose guest has an opinion about pid 1 — an init-system
test, a container-runtime test, a distro image as shipped, a nested harness that wants its own
supervisor — needs the agent to be placeable, and none of them needs a different agent.

**Interface sketch** (naming vmcell's to choose):

```rust
// crates/vmcell-guest-agent/src/lib.rs — grows from the reaper into the whole agent.
pub struct AgentOptions {
    pub placement: Placement,        // Pid1 { assemble_filesystem: bool } | Service
    pub vsock_port: u32,             // default AGENT_VSOCK_PORT — one const, shared with the host
    pub tools_dir: PathBuf,          // default /vmcell-tools
    pub tuning: Tuning,              // accept_poll / rebind_idle; the BINARY parses /proc/cmdline
    pub on_sigterm: Sigterm,         // PowerOff (Pid1) | Shutdown (Service)
    pub max_reaped_statuses: usize,  // DEFAULT_MAX_REAPED_STATUSES
    pub tracing: TracingSetup,       // Install | AlreadyInstalled
}
/// Sets PR_SET_CHILD_SUBREAPER under `Service`, and does not under `Pid1`.
pub fn run(opts: AgentOptions) -> Result<()>;
```

`main.rs` shrinks to what `daemon-bin` is: read `/proc/cmdline`, install the subscriber, call `run`.

**What must not regress.**

- **PID-1 discipline.** The fatal core set stays fatal under `Pid1` — overlay (`main.rs:207`),
  `/proc` (`:262`), devtmpfs (`:273`); note the tmpfs mount at `:190` also returns `Err` in code even
  though the design names the set as three, and a refactor must not quietly change which of the four
  is fatal. Best-effort mounts (`/sys` `:234`–`:247`, devpts `:285`–`:301`, shares `:330`–`:353`,
  loopback `:360`–`:365`) stay best-effort.
- **The lean-agent graph.** `Cargo.toml`'s CI assertion — `cargo tree -e no-dev` sees no tokio/hyper
  on the agent's production graph — must survive the split. A library that pulls a host async stack
  in defeats the crate's whole shape.
- **The false-127 race guard**, the no-`child.wait()` rule, and the reservation/epoch pairing.
- **The wire protocol.** `Ready` stays the first frame of each accepted connection (`main.rs:906`,
  read at `agent/mod.rs:808`), and the credential posture stays unchanged — there is no `.uid(`,
  `.gid(`, `setuid` or `setgid` anywhere in the crate, and the `login_tty` `pre_exec`
  (`main.rs:1809`, registered `:1871`) does only `setsid`, `ioctl_tiocsctty` and three `dup2`s.

**Pay-for-what-you-use.** `Placement::Pid1` with the shipped defaults is today's binary. No new
syscall on the default path — `PR_SET_CHILD_SUBREAPER` is set only under `Service` — no new
allocation, no new dependency, and the same cmdline.

**How the requester would verify it.** Under `Placement::Service`, spawn a double-forking payload
and assert `exec` returns its exit code rather than hanging — with **fail-first proof** by removing
the `PR_SET_CHILD_SUBREAPER` call and watching the same test hang. Then send SIGTERM to the agent and
assert the guest is still running and a subsequent `exec` still works, with the inverse being today's
behaviour (the guest powers off). Then run both legs under `Placement::Pid1` and assert the SIGTERM
leg *does* power off — because a SIGTERM policy that never powers off is the same
assertion-free shape one direction over.

---

## R6 — Repacking usable externally, with xattr preservation a declared artifact property

**The capability, in one sentence.** Producing a rootfs artifact — `oci2-erofs` plus an ext4 path —
works from outside a vmcell checkout, and whether the packer strips or preserves xattrs is a property
of the artifact rather than of the packer.

**State: mixed, and narrower than it looks. The library half is SHIPPED; the CLI half has one hard
external blocker; the ext4 path is ABSENT.**

**What exists, and it is more than the request assumes.** `pack_erofs_with_injection` is public
(`crates/vmcell/src/artifact/rootfs/mod.rs:471` under `#[cfg(feature = "am-fs-erofs")]`; the
non-feature arm at `:630` is a typed error, matching §17's note that the `mkfs.erofs` shell fallback
is designed but unimplemented). It is **named contract surface** in design §10.4. `vmcell-rootfs-builder`
wraps it (`src/lib.rs:53`, `:281`). `ExtraFile` (`rootfs/mod.rs:37`) and `RootfsStage.extra`
(`:223`) are both public — so the previous version of this document was wrong to say injection is
"reachable only from `vmcell oci2-erofs`": that is true of the *CLI* surface (`--inject` exists only
on `Oci2Erofs`, `vmcell-cli/src/main.rs:112`–`:113`, with `vmcell build` taking none and saying so at
`:468`–`:470`) and false of the library.

**Where it stops — three things.**

1. **The CLI cannot run outside a vmcell checkout, and there is exactly one hard reason.**
   `GuestToolsStage` is unconditional in `oci2erofs` (`vmcell-cli/src/main.rs:1044`), and both its
   `cache_key` and its `run` call `guest_tools_closure_hash(workspace_root())`
   (`artifact/guest_tools.rs:39`, `:59` — a hard `?`) and then
   `cargo build --release --target x86_64-unknown-linux-gnu -p vmcell-guest-tools` with
   `current_dir(ws_root)` (`:66`–`:75`). `GuestAgentStage` does the same (`guest_agent.rs:52`–`:63`)
   but **is** skippable with `--agent-musl` (`main.rs:1040`); tools is not skippable at all. A
   secondary friction: staging and the OCI blob cache go under `artifacts_dir()` —
   `$VMCELL_ARTIFACTS_DIR` else `<workspace_root>/target/vmcell-artifacts` (`artifact/mod.rs:50`–`:59`,
   `:92`–`:93`, `main.rs:1029`–`:1030`) — so an external caller silently writes a large staging
   directory under its own `target/` unless it knows to set the variable.
   **R2's handler registry is the fix, and it already has a shape**: a registered handler artifact
   selected by digest is `--agent-musl`'s treatment generalized to tools, and it severs the one edge
   that requires the checkout.
2. **The consumer-position gate covers argument parsing, not packing.**
   `examples/downstream-kernel/ci-check.sh` does invoke the documented CLI, and its own header states
   the scope: the legs are "exercised on their fail-fast contract boundaries so the leg needs no
   network and no 6-minute kernel compile". Both `oci2-erofs` legs `expect … nonzero` (`:110`–`:115`
   — one asserting the digest-pinning refusal, one asserting an unknown `--inject` key is named), and
   the README tells the consumer to bring "an externally built rootfs" (`README.md:71`–`:75`). So
   **no rootfs has ever been packed from the consumer's position in CI.** The comment is honest about
   its scope, which is what separates this from AGENTS §3's tell — but the packing half of the
   contract has no consumer-shaped gate, and R6's own verification leg is the one that would add it.
3. **No ext4 path exists at all — ABSENT, not partial.** `RootfsSource::Block` is *consumable* by
   every backend (`config.rs:602`–`:607`, `rootfstype` selection `:372`–`:375`, `rootflags=noload`
   `:376`–`:379`), but nothing in-tree ever *produces* an ext4 image: no `mkfs.ext4`, no `mke2fs`, no
   `e2fsprogs`, no `debugfs` anywhere in `crates/`, `scripts/`, `fuzz/`, `examples/` or the
   `justfile`. The only packer is tar→EROFS (`fs_erofs::mkfs`, `tar2erofs.rs:6`, `:635`).

**The xattr premise, and why it expired.** `tar2erofs.rs:121`–`:127` records the rationale verbatim:
*"the guest agent and every in-guest `exec` run as root (§4.2), so file capabilities are moot; the
erofs Node/XattrSpec plumbing exists but is unused."* It is implemented as `xattrs: vec![]` on all six
tar-derived node kinds — file `:132`, dir `:139`, symlink `:152`, char `:170`, block `:188`, fifo
`:194` (the file has ten such sites in all; the other four are on injected nodes) — and pinned by
`test_pax_xattrs_are_not_preserved` at **`:817`**, body `:817`–`:844`, recorded-limitation doc
`:810`–`:815`. *That corrects the previous version's `:797`–`:824`, which straddles the end of a
different test (`test_injected_ca_is_not_executable`, ending `:808`).*

**The premise is true of the pinned minimal base and false of `debian-latest`.** The pinned base's
single cached layer has 3262 entries and ships **no** `getcap`, `setcap`, `setfacl`, `getfacl` or
`capsh` — only `libcap.so.2`, `libcap-ng.so.0` and `libacl.so.1`. So nothing in that image could even
observe a file capability if one survived, and the strip costs it nothing. A full Debian ships file
capabilities on real binaries. An image that packs them away is not a `debian-latest` image; it is a
`debian-latest` image with one class of behaviour deleted, and nothing anywhere would say so. The
request is not to reverse the strip — it is to **scope** it, which the comment itself makes cheap by
noting the plumbing already exists.

**Why this is a platform capability, not a bolt-on.** The general form is "an artifact's packer
policy is a property of the artifact, not of the packer", which is the same law R2 applies to
selection and R3 applies to declaration. Other consumers: anyone testing **package installation**,
where a file capability on a binary like `ping` is the observable; anyone testing a **container image
as shipped**; anyone testing **SELinux or AppArmor labelling**, which rides the same `security.*`
xattr mechanism and would be silently erased by the same line.

**Interface sketch** (naming vmcell's to choose):

```jsonc
"rootfs": {
  "default":        { "image": "…", "digest": "sha256:…" },            // xattrs default to "strip"
  "debian-systemd": { "image": "…", "digest": "sha256:…",
                      "xattrs": "preserve" }                            // or an allowlist of prefixes
}
```

```
# The CLI half, severing the checkout edge and making the staging dir explicit.
vmcell oci2-erofs debian@sha256:… -o out.erofs \
    --agent-musl ./prebuilt/vmcell-guest-agent \
    --tools      ./prebuilt/vmcell-guest-tools \    # the missing mirror of --agent-musl
    --work-dir   ./build/scratch
```

Plus the ext4 producer as a second packer behind the same stage interface, so `RootfsSource::Block`
finally has something in-tree that fills it.

**What must not regress.**

- **The default artifact stays byte-identical.** Default `xattrs: "strip"`, same density, same cache
  key.
- **`test_pax_xattrs_are_not_preserved` stays green for the default artifact** and gains a `preserve`
  twin. The two together are R4's two-directional shape applied to one property, and are the cheapest
  possible instance of it.
- **The erofs root's properties.** R6 adds artifacts; it does not replace the root. Read-only, no
  journal, no per-VM copy, one host page cache across concurrent guests (design §4.1, §8.3) all
  stand, and so does the reasoning at `config.rs:485`–`:487` about `rw` inverting `ro` over
  `rootflags=noload`.
- **`is_reserved_injection_path`** and the reserved-dest guard (`rootfs/mod.rs:83`, gate at `:806`).
- **Cache correctness.** An xattr-policy change is an artifact-identity change and must re-pack.

**Pay-for-what-you-use.** The default artifact keeps stripping, at the same density and the same
bytes. A custom artifact opts in and pays the size of the xattrs it asked to keep. `--tools` and
`--work-dir` are additive flags whose absence reproduces today's behaviour exactly.

**How the requester would verify it.** Pack the default artifact twice and diff the bytes and the
cache key. Pack a `preserve` artifact from a base carrying a `security.capability` xattr, boot it, and
read the xattr back in-guest — with the fail-first proof being the same artifact declared `strip`,
where the read must find nothing. Then run the same pack **from a directory that is not a vmcell
checkout**, with `--tools` and `--agent-musl` pointing at prebuilt binaries, and assert it succeeds;
the fail-first proof there is the same run without `--tools`, which must fail naming
`vmcell-guest-tools` rather than silently producing an image with no applets.

---

## R7 — Reproducibility survives consumer-supplied artifacts

**The capability, in one sentence.** Every registration in R2's registry takes a **digest**, never a
path; a path is accepted only as a hint verified against the digest before use.

**State: SHIPPED as a discipline, ABSENT as a registry rule — because the registry it would govern
does not exist yet.** This request exists to be settled *before* R2 lands, not after.

**What exists.** The discipline is already the platform's, in four independent places: `oci2erofs`
rejects a tag and demands `IMAGE@sha256:<64 hex>` (`vmcell-cli/src/main.rs:1012`–`:1020`);
`pins.json` pins the base image by digest (`:28`–`:31`) and the prebuilt kernel by `archive_sha256`
plus `sha256` (`:22`–`:27`); `vmcell bundle` writes a digest-pinned manifest that `verify-bundle`
re-hashes; and §10.2's five cache-key rules and §10.3's determinism scope are the same rule one layer
down.

**Where it stops, and the distinction that matters.** The two escape hatches are `VMCELL_KERNEL` and
`VMCELL_ROOTFS`, whose documented contract (§10.4) is *path redirect only* — "the harness uses this
kernel verbatim and still requires it to exist (fail-loud)", and `VMCELL_ROOTFS`'s presence makes
`ensure_test_artifacts` a **full no-op**. That is correct for an **override**: a deliberate, per-run
act by an operator who knows what they are pointing at. It is exactly the shape a **registration**
must not copy, because a registration is a durable claim that outlives the session that made it. A
registry that accepts a path is a registry whose entries mean "whatever is at that location today".

**The consumer need.** This project's kernel-claims discipline (AGENTS §7, design §16.13) requires a
kernel claim to cite a committed artifact with its commit, fingerprint and date, never terminal
scrollback. Its comparison discipline is stricter still: `docs/doctor/README.md:174` records a
cross-kernel pair that holds "on a basis stronger than either digest", because `git diff` over the
doctor source between the two commits is empty — same instrument, same adapters, same cable, only the
kernel differs. A cell whose rootfs is "whatever was at that path" cannot participate in that
discipline at all. The artifact would be un-citable, and a row quoting it would be precisely what
§16.13 exists to stop.

**Why this is a platform capability, not a bolt-on.** Reproducibility is the guarantee vmcell already
sells — `docs/requirements.md`'s artifact-production section states it as five numbered rules
("All following stages are deterministic / idempotent / repeatable", "When a deterministic stage
succeeds, its output is completely determined by its inputs"). A registry that accepts a path trades
that guarantee for convenience the first time anyone uses it, and the trade is **invisible**: the
cell boots, the tests pass, and only the artifact's identity is a lie. Other consumers: any CI that
must reproduce a red run from six months ago; any bisection across artifact versions; anyone shipping
a bundle, since `verify-bundle` has nothing to verify if the registry accepted a path.

**Interface sketch** (naming vmcell's to choose):

```jsonc
"rootfs": {
  "debian-systemd": {
    "digest": "sha256:…",                       // authoritative
    "source": { "oci": "docker.io/library/debian" }   // or { "url": …, "archive_sha256": … }
  }
}
```

The digest is authoritative and the source is a fetch instruction verified against it before use,
failing loud on mismatch — the shape `pins.json:22`–`:27` already has for the prebuilt kernel. A local
path with no digest is accepted only under an explicitly named development override, which (a) marks
the resulting cell's artifact identity `unpinned` wherever the cell reports its provenance, and (b)
is refused by `bundle`.

**What must not regress.**

- **`VMCELL_KERNEL` / `VMCELL_ROOTFS` keep their documented path-redirect semantics.** R7 governs
  registrations, not overrides. Conflating the two would break the documented downstream
  configuration that `examples/downstream-kernel/ci-check.sh:82`–`:89` pins from the consumer
  position — the leg that asserts the getters return the named paths *with* the full override set,
  and fail loud naming the two-step route without it.
- **The fetch stays cacheable and offline-friendly.** vmcell's own requirement — "Minimize access to
  external servers while testing and iterating. Use on-demand caching to avoid downloading a resource
  multiple times" — means a digest-keyed cache hit must skip the fetch entirely, which is how the OCI
  layer cache already works.
- **Determinism of the resulting cache key.** A registration's digest is an input to the artifact's
  identity fold, so two registrations naming the same digest must produce the same key.

**Pay-for-what-you-use.** A cell that names no registered artifact resolves `default` and never
evaluates a digest it did not already evaluate. The verification is one hash over bytes already being
read.

**How the requester would verify it.** Register an artifact by digest, build it, and assert the fetch
verified. Then corrupt one byte of the cached blob and assert the build **fails naming the digest
mismatch** — that is the fail-first proof, and it is the whole assertion, because a digest that is
stored and never checked has passing output identical to its not-running output. Then attempt a
development registration with a path and no digest and assert `bundle` refuses it.

---

## Reframed, not overturned: what the previous version declined

The superseded text declined two things, and this document reverses neither. Both declines are
restated here with where they stand, because silently re-fixing a declined item is a defect
(AGENTS §5) and the reasoning is worth more than the conclusion.

**1. systemd as PID 1** — superseded § "E — systemd as PID 1: not requested, and why", around
`:531`–`:574` of the previous file.

What E actually declined was **asking vmcell to keep the control plane while surrendering PID 1**, on
the ground that this "is asking it to give up law C1, the Ready handshake, `exec`, sessions, `Resync`
and the entire snapshot tier — which is not a feature request, it is a different product." That
reasoning is correct and this document does not contradict it. What it identifies is the **premise**
it rested on: that control-plane availability and PID-1 identity are the same fact. In the tree they
are the same predicate, so E's reading of the tree was accurate. Untie the predicate — R1 — and
nothing is surrendered: under `AgentPlacement::Service` the agent keeps its vsock listener, its
`Ready` frame, `exec`, sessions and `Resync`, and it is systemd rather than the kernel that starts
it. **The decline was against a trade; R1 removes the trade.** That is the same principle applied to
a better mechanism, not an overturn.

E's second, independent ground stands unchanged and re-verified: *"the rootfs would not carry it
anyway."* The pinned base's cached layer has 3262 entries and contains no `usr/lib/systemd/systemd`,
no `usr/bin/systemctl` and no `usr/sbin/init` — only unit files shipped by other packages,
`libsystemd.so.0`, and dpkg's `deb-systemd-helper` / `deb-systemd-invoke` shims. R2 does not change
that image. It lets a second image exist beside it, which is the difference between a switch and an
artifact.

**2. Switching the rootfs to ext4** — superseded "Considered and not requested", around `:594`–`:596`:
*"The one-way global switch this document exists to avoid. It would charge every cell the loss of the
shared-page-cache density lever and the journal-free property, to serve a fixture directory."*

That is exactly right and R6 does not ask for it. **R6 asks for an ext4 producer, not an ext4 root.**
The erofs root stays the default and stays byte-identical; the density lever (one host page cache
across concurrent guests, no per-VM copy — design §4.1, §8.3) and the journal-free property are
untouched, because an artifact a cell may select is not a switch every cell pays for. The earlier
text's cost analysis is the reason the request has this shape rather than the obvious one, and it is
preserved rather than replaced.

**3. One thing is closer to an overturn than a reframe, and is marked as one.**

E closed with a recommendation: *"The recommendation is to keep E on a systemd CI runner, which is
where it already is and where it is already working."* Its accompanying reasoning — that item 31
needs systemd's *own* behaviour, and "a claim about systemd is not made true by any capability this
project or this platform holds" — is still true, and R1 does not contradict it; R1 supplies real
systemd, which is the only thing that could.

But the *recommendation* was written when a cell could not host systemd at any price, so "keep it on
CI" was a choice between one instrument and none. It now has a competitor, and the honest current
statement is narrower: CI's root arm remains the right instrument for item 31 today, and item 31(c)
has already gone green there; a cell would become a **second** instrument once R1, R5 and R2 land,
useful for the reason CI cannot be — a cell can hold the kernel and the userland fixed across runs
and vary one of them deliberately, which is the same property that makes plan §18 item 8's
kernel-of-record comparison worth having. That changes what the previous text asserted about the
choice, so it is flagged as an overturn of the recommendation's scope rather than quietly restated.
Its conclusion — that CI is where item 31 lives today — is unchanged.

---

## Considered and not requested

Each of these was worked through and rejected as too single-use, as a one-way switch, or as the wrong
instrument. They are recorded so the same ground is not re-covered.

- **A dedicated `--push` / target-runner flag on `vmcell run`.** The previous version's R1. It is
  **subsumed rather than dropped**: with R2's handler registry, a consumer's own binary is a
  registered artifact selected per cell, which is a stronger answer than a per-run file push — it is
  cached, content-addressed, and citable. The two limits that made the push route awkward are
  unchanged and worth keeping on the record: `Message::PutFile { dst, bytes }` carries **no mode
  field** and the guest handler is `create_dir_all` + `std::fs::write` with no `set_permissions`, so
  a pushed binary lands non-executable; and it is one frame under a 16 MiB `MAX_FRAME_BYTES`, which
  several of this project's own test binaries exceed. An artifact has neither problem.

- **USB-serial support in a vmcell kernel, or a USB-serial passthrough path.** Consumer content in
  the platform (law G1), and it would not work: passthrough is QEMU-only, addressed by `vid:pid`
  rather than by port so a cross-wired *pair* of identical adapters is not addressable at all, and
  snapshot-incompatible. **Two corrections for the record.** First, `pins.json` *does* carry a
  `USBHOST` fragment (`:37`) that the previous version of this document missed, and a `usbhost`
  kernel label alongside `6.6.143` and `6.12.94` — so the host-controller half of the route is
  already registered. Second, that changes nothing for serial: `CONFIG_USB_SERIAL` and
  `CONFIG_USB_ACM` remain **not set** in every config in the tree, including
  `vmlinux-usbhost.config` (`vmlinux.config:3809`, `:3760`, identically in all four). The mechanism a
  consumer would need is the pins-overlay fragment route — its own fragment in its own overlay,
  exactly as `examples/downstream-kernel/` demonstrates with `IKCONFIG` — and the hardware work
  belongs on a wired rig regardless.

- **A writable-scratch mount with declared filesystem semantics.** The previous version's R2, and
  **subsumed by R3 rather than dropped**: "the semantics of this filesystem" is a feature
  declaration, and once features are declared with provenance and checked in both directions, a
  scratch mount is a cell that declares `XattrPreserved` and `PosixAcl` and has that declaration
  verified. A separate descriptor for one mount would be a second capability vocabulary, which is the
  divergence R3 exists to prevent. The underlying facts are unchanged and still favourable: the
  writable layer carries no `nosuid`, `nodev` or `noexec`, and `CONFIG_TMPFS_XATTR` and
  `CONFIG_TMPFS_POSIX_ACL` are `y` in all three kernel configs (`vmlinux.config:4510`, `:4509`, and
  identically at `:4384`/`:4383` in the 6.6.143 config).

- **`--share` on `vmcell run`.** Rejected on three verified grounds rather than on taste: shares need
  host `CAP_SYS_ADMIN` for virtiofsd's sandbox; they are snapshot-ineligible — `build()` refuses
  shares + snapshotting at `config.rs:1569`–`:1573`, and the shared predicate
  `config_has_vhost_user_device` (`crates/vmcell/src/vmm/mod.rs:927`, **not** in `config.rs` as the
  previous version had it) is what every backend self-guards with; and virtiofsd is spawned with no
  `--xattr`, no `--posix-acl` and no `--xattrmap` anywhere in the tree, so a share could not carry a
  capability xattr or an ACL even if it were the right transport. That is stronger than "unproven":
  it is proven absent.

- **Asking vmcell to install `libcap2-bin` and `acl` in its rootfs.** Consumer content. R2 asks for
  the registry instead, which is the same answer vmcell gave itself when it wrote
  `vmcell-guest-tools` rather than adding distro packages — design §4.4: *"Rather than bloat the
  rootfs with distro packages or weaken the tests, the harness ships a small Rust multicall
  binary."*

- **A guest-side privilege API — dropping to a non-root uid, or capability control inside the cell.**
  Not needed: guest exec is already root with the full bounding set (no credential syscall anywhere
  in the agent crate), so this would be asking for *less*. It would also be load-bearing in the wrong
  direction — the packer's xattr strip is justified *by* the everything-runs-as-root premise
  (`tar2erofs.rs:121`–`:127`) — so introducing a non-root uid without R3's declared semantics would
  silently break any capability-dependent binary. `CONFIG_USER_NS` is `not set` in every config
  (`vmlinux.config:217`), which closes the unprivileged-userns route independently.

- **Anything that gives a cell's caller root on the *host*.** Out of scope by construction; the point
  of a cell is that the boundary is the VM.

---

## What this project would do on its side

Recorded here because a feature request that hides the consumer's own work is dishonest about the
cost.

1. **Two existing tests break in a cell, in opposite directions.**
   `devprep/src/linux/install.rs:492`
   (`the_sweep_removes_a_blessed_stray_and_never_the_copy_that_belongs`) calls the real
   `sweep_orphans` (`:239`), which reaches `read_caps` (`:292`) — and `read_caps` shells out to bare
   `getcap` with no `which` probe, no skip and no graceful arm, so a missing binary becomes
   `ErrorKind::NotFound` propagated by `?`. The pinned base has no `getcap`, so that test panics in a
   cell before any new work starts. In the other direction, `sys/src/lib.rs:2155`
   (`the_blob_is_well_formed_enough_for_the_kernel_to_reach_the_permission_check`) breaks *with*
   uid 0: it asserts `expect_err` and `EPERM` on a root-owned node on the premise that an
   unprivileged process cannot set an ACL there, which as root simply succeeds. Both need a
   uid-aware arm before any suite runs at uid 0 anywhere — cell or CI root arm.

2. **`read_caps` should read the xattr, not shell out.** The `security.capability` xattr can be read
   directly with `getxattr`, exactly as `grant_user_rw` (`sys/src/lib.rs:1278`) already *writes*
   `system.posix_acl_access` directly against an `O_PATH` fd (`:1317`). Doing that removes one of the
   two reasons a cell would need extra guest content at all, and is worth filing whether or not any
   request here lands.

3. **A cell lane would be another `required` gate, and the discipline for that is already written.**
   Measure the precondition on a real runner first, then demand it — the order
   `itest/tests/p8_packaging.rs:176`–`:188` records for `SNX_PACKAGING_ROOT`, whose own comment notes
   that until the variable was set by a lane, "the root step's passing output [was] identical to its
   self-skipping output — AGENTS §3's tell, in the very step that had just been un-escaped from
   `continue-on-error` in order to gate."

## What "landed" would look like

**R3 + R4** land first and change no capability. What they change is that every claim the platform
already makes implicitly becomes a claim that can go red — including the two this document found
while reading: an intersection that does not exist, and a fragment whose survival predicate passes on
a build it did not affect.

**R1** makes systemd expressible. On its own it makes the *predicate* right, which is verifiable and
worth landing alone; it does not yet make a service-mode agent exist.

**R1 + R5** make a live systemd reachable with the control plane intact, which is what plan §18 item
31's four `unverified` rows have been waiting for — and what `packaging/README.md:373`–`:377` names
in one sentence.

**R2 + R6 + R7** make `debian-latest` an artifact rather than a fork: registered by digest, packed
with its file capabilities intact, selectable per cell, and citable in a durable record. That is also
the only configuration in which a cell becomes a better instrument than CI for anything — because a
cell can hold the kernel and the userland fixed and vary one of them deliberately, which is the
property plan §18 item 8's kernel-of-record comparison needs and no CI runner offers.

Not one of these requires vmcell to grant a privilege it does not already grant, to weaken an
isolation boundary, to change the erofs root, or to slow down a cell that asks for none of it.
