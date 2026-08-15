# vmcell requirements: running the root-blocked work in a cell

## What this document is

A considered feature request, written from the consumer position, for the platform capabilities
`vmcell` would need before this project's privilege-blocked work could run inside a micro-VM
("a cell") instead of on a root-capable box.

**It changes no decision.** The operator has routed the root-needing work to CI's root arm
(`.github/workflows/ci.yml:399`–`:410`), and that arm is already carrying its share: item 31's
sandbox measurement ran green there on 2026-08-13, and `SNX_PACKAGING_ROOT=required` has gated the
step since 2026-08-15. Nothing below is needed to keep that working. This is the list of what a cell
would have to become before it were the *better* instrument, and it is written so that vmcell can
evaluate each item as a platform capability on its own merits.

**Every request here is framed as a general capability with a stable interface**, justified by more
than the one use that prompted it, and specified so that a cell which does not ask for it pays
nothing. Where the obvious fix is a one-way global switch — swapping the erofs root for ext4 is the
standing example — the request says so explicitly and proposes the opt-in form instead. A request
only this project could ever want is a bad request; each one below names what else it unlocks.

## Provenance

| Thing | State when measured |
|---|---|
| `serial-nexus` | read at `198a654`, branch `main` — a commit **since reverted** (`94bfe63`) because it swept in-flight work into a docs-only change. Nothing in this document depends on the reverted code: every serial-nexus claim below is about `devprep`, `sys`, `rpc` and the packaging tests, none of which that commit touched |
| `vmcell` | `fbcd018`; design read: `docs/82-claude-opus-design-v32.md` (v32), plus `README.md` and `AGENTS.md` |
| Guest kernel configs read | `target/vmcell-artifacts/vmlinux.config`, `vmlinux-6-12-94.config`, `vmlinux-6-6-143.config` (all host-`make` builds, 6.12.94 / 6.6.143) |
| Guest userland read | the digest-pinned base layer `sha256-e95a6c7ea7d49b37920899b023ecd0e32796c976c1748491f76cae53ba86d13a` in `target/vmcell-artifacts/oci-cache/`, 3262 entries, listed with `tar -t` |
| Date | 2026-08-15 |

Line numbers are anchors, not contracts; they drift. Nothing here was run inside a cell — no cell was
booted for this document — so every claim is a claim about *code and artifacts as read*, and the
verification sections below say what would have to be executed to convert each into a measurement.

One document in the workspace covers adjacent ground and is cited once for its caveat, not its
conclusions: `usb-teleporter/docs/4-claude-fable-repo-research.md` is a research report on structuring
a workspace that consumes several sibling workspaces. Its §0 states plainly that the repositories
"could not be inspected" and that its recommendations "are grounded in Cargo best practice and are
written so they hold regardless of those details". It is therefore **generic advice, not grounded
findings**, and nothing in this document rests on it. The sibling
`usb-teleporter/docs/feature-requests-vmcell.md` is a prior request set written under the same
generality directive; the structure here deliberately follows it.

## The finding that reorders the list

The blocked work is filed under "needs root". Inside a cell, **root is not the missing thing.** It is
already there, unconditionally:

- PID 1 is `vmcell-guest-agent` (`crates/vmcell/src/config.rs:446`, `DEFAULT_INIT`).
- The exec path builds its child with `Command::new` + args + cwd + env and a `PATH` prefix, and
  nothing else — no `uid()`, no `gid()`, no credential change of any kind
  (`crates/vmcell-guest-agent/src/main.rs:1206`–`:1225`, one-shot spawn at `:1228`–`:1260`, session
  spawn at `:1739` whose only `pre_exec` is the `login_tty` sequence at `:1803`–`:1830`).
- The wire type carries no place to ask for anything else: `ExecRequest { argv, env, cwd, timeout }`
  (`crates/vmcell-protocol/src/lib.rs:403`–`:412`).

So an exec'd guest command is uid 0 with the full bounding set. vmcell records this as a premise
rather than an accident — `crates/vmcell/src/artifact/tar2erofs.rs:121`–`:127` says "the guest agent
and every in-guest `exec` run as root (§4.2)" and uses it to justify dropping capability xattrs at
pack time.

The guest kernel already carries what the blocked items need, in all three configs read:

| Symbol | Value | Line (in `vmlinux.config`) | Which item needs it |
|---|---|---|---|
| `CONFIG_TMPFS_XATTR` | `y` | `:4510` | A — a `security.capability` xattr has somewhere to live |
| `CONFIG_TMPFS_POSIX_ACL` | `y` | `:4509` | C — `system.posix_acl_access` is honoured |
| `CONFIG_SECCOMP_FILTER` | `y` | `:796` | B — a launcher can filter `prctl` |
| `CONFIG_UNIX98_PTYS` | `y` | `:2646` | the pty probes generally |
| `CONFIG_EXT4_FS`, `CONFIG_EXT4_FS_POSIX_ACL`, `CONFIG_EXT4_FS_SECURITY` | `y` | `:4417`, and adjacent | an alternative home for a fixture |
| `CONFIG_BLK_DEV_LOOP` | `y` | — | mounting a caller-supplied image in-guest |
| `CONFIG_PID_NS`, `CONFIG_NAMESPACES` | `y` | — | the namespace route discussed under E |
| `CONFIG_USER_NS` | **not set** | — | closes the unprivileged-userns route entirely |
| `CONFIG_IKCONFIG` | **not set** | `:166` | no `/proc/config.gz`; the sidecar is the only route |

And the writable layer imposes no restriction: the agent mounts `tmpfs` on `/mnt` and then overlayfs
over it with `MountFlags::empty()` and no data string in both cases
(`crates/vmcell-guest-agent/src/main.rs:186`–`:192` and `:203`–`:209`). No `nosuid`, no `nodev`, no
`noexec`, no `size=`. So `mknod` works, setuid works, and a `security.capability` xattr has a
filesystem that supports it.

**That is the reordering.** Items A, B, C and D are not blocked on privilege a cell would have to
grant. They are blocked on four things a cell does not currently offer:

1. **Getting the test binary in and its exit code out.** There is no supported route for a consumer's
   own executable. This blocks everything.
2. **A writable filesystem whose semantics are *declared*.** The behaviour above is real but
   undocumented — an implementation detail of the agent's mount sequence that a plausible hardening
   increment (vmcell's own §17 lists a "jailer chroot/`pivot_root`/uid-drop increment") could add
   `nodev,nosuid` to without anyone noticing a consumer's fixture had gone silently wrong.
3. **Guest userland this project's code already assumes.** `getcap` is absent from the base image, and
   one existing unit test shells out to it — it reddens on contact, before any new work.
4. **A kernel whose configuration is readable.** Which is also, separately, the whole of item 8's
   value.

Item E is different in kind and is treated separately below.

## Request index

| # | Capability | Unblocks | Priority |
|---|---|---|---|
| **R1** | Push-and-run: a cell usable as a cargo target runner | A, B, C, D — all of them | **blocking** |
| **R2** | A declared writable-scratch contract | A, B, C | high |
| **R3** | A supported seam for consumer-owned in-guest content | A (its `getcap` half), and an existing red test | high |
| **R4** | Every kernel self-describing and citable | F, and the preconditions of A/B/C | medium |
| — | E (systemd as PID 1) | — | **not requested** — see below |

---

## R1 — Push-and-run: a cell usable as a cargo target runner

**The capability, in one sentence.** A supported path by which a caller hands a cell an executable it
built on the host, runs it with arguments, streams its output, and receives its exit code — enough
that `vmcell run` can be wired directly as `CARGO_TARGET_<triple>_RUNNER`.

**The consumer need.** All four of A (plan §18 item 52's residual, `docs/44-implementation-plan-claude-fable-v17.md:2192`–`:2194`),
B (`devprep/src/linux/mod.rs:222`–`:233`), C (`sys/src/lib.rs:1278`) and D
(`rpc/src/socket.rs:128`–`:150`) are exercised by a compiled Rust test binary. Not one of them can be
attempted in a cell until such a binary can run there.

**What exists today, and exactly where it stops.**

- `vmcell run` already does three of the four jobs: it takes a trailing argv with
  `allow_hyphen_values` (`crates/vmcell-cli/src/main.rs:168`–`:174`), and it exits with the guest
  command's code (`main.rs:667`–`:700`; README's subcommand table says so too).
- It cannot get the binary in. The full `Run` arg set is `--kernel`, `--rootfs`, `--vcpus`,
  `--mem-mib`, `--disk`, `--disk-rw`, `--append`, `--tty`, `--stdin`, plus argv
  (`crates/vmcell-cli/src/main.rs:133`–`:175`). No `--share`, no `--push`, no `--inject`;
  `ephemeral_vm` calls only `.vcpus()`, `.mem_mib()`, `.with_extra_disk()` and `.with_kernel_arg()`
  (`main.rs:945`–`:972`).
- The library route exists but has two defects for this purpose. `AgentClient::put_file(dst, bytes,
  timeout)` (`crates/vmcell/src/agent/mod.rs:1009`–`:1057`) sends one `Message::PutFile { dst, bytes
  }` (`crates/vmcell-protocol/src/lib.rs:273`–`:280`), and:
  - **there is no mode field.** The guest handler is `create_dir_all(parent)` then
    `std::fs::write(dst, bytes)` (`crates/vmcell-guest-agent/src/main.rs:1055`–`:1071`) — no
    `set_permissions`, and the agent never calls `umask`. **A pushed binary lands
    non-executable**, and the only fix in-protocol is a second round trip exec'ing `chmod`.
  - **it is one frame.** `MAX_FRAME_BYTES` is 16 MiB (`crates/vmcell-protocol/src/lib.rs:55`) and the
    cap is checked on both sides (host `agent/mod.rs`, guest `main.rs:1044`–`:1049`). Measured
    against this tree's own artifacts: `target/debug/deps/serial_nexus_daemon-*` is **112–120 MB**,
    seven times over. `p8_packaging-*` is 9.97 MB and would fit — so the cap does not block
    *everything*, which is worse, because it makes the limit look like it works.
- The virtio-fs route is the wrong instrument for three independent reasons, each verified: shares
  are library-only (no CLI flag anywhere in `vmcell-cli`); they need host `CAP_SYS_ADMIN` for
  virtiofsd's sandbox (`crates/vmcell/src/hostcaps.rs:30`, `:45`, `:114`), so they fail in the
  unprivileged mode the design itself notes at §4.4; and they are snapshot-ineligible by law S1
  (`crates/vmcell/src/config.rs:1569`–`:1573`). A route that only works privileged and disqualifies
  the fast path is not the general answer.

**Why this is a platform capability, not a bolt-on.** The target-runner idiom is *already vmcell's
own*: `vmcell-test-runner` exists precisely so an unprivileged developer can run a privileged suite,
wired through `CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER` (`README.md:112`, `justfile:151`). This
request is the same idiom one isolation level up — where `vmcell-test-runner` confers *host*
capabilities on a test process, a cell runner confers *guest* root plus a chosen kernel. The
mechanism is identical in shape and the security argument is strictly better, because the boundary is
the VM rather than a capability set.

The second consumer is inside vmcell already. The pins registry ships `KASAN`, `KCOV`, `LOCKDEP` and
`SLUB_DEBUG` fragments (`pins.json:33`–`:36`) and `vmcell build-kernels` builds them side by side as
`vmlinux-<label>`. **There is no supported way to run anybody's test suite under them.** A platform
that can build a LOCKDEP kernel but cannot run a suite on it is one flag short of a capability it has
already paid for. Any project wanting a suite under KASAN, under a specific LTS line, or simply at
uid 0 without a root-capable machine is the same consumer.

**Interface sketch** (naming vmcell's to choose):

```
# The minimal increment: expose the control plane's existing file push on the CLI.
vmcell run --kernel K --rootfs R \
    --push ./target/debug/deps/p8_packaging:/tmp/t:0755 \   # repeatable: SRC:DEST[:MODE]
    /tmp/t --nocapture --test-threads=1
```

Three sub-clauses, each independently useful:

1. **`--push SRC:DEST[:MODE]`**, repeatable — the CLI face of `put_file`, with `MODE` defaulting to
   the file's own host mode. This needs a `mode` on `Message::PutFile`, which is a wire change and
   therefore a deliberate contract bump; the presence-attribute codec rule (design Appendix A
   reversal 10) applies if the field is optional.
2. **Chunked transfer** for a payload over `MAX_FRAME_BYTES` — an offset/append shape, or a
   `PutFileChunk` frame, so the 16 MiB ceiling stops being the real limit on what a cell can run.
   The alternative that needs no wire change is to say plainly that a caller over the cap must use a
   `--disk-rw` image and mount it in-guest; that is the workaround in force below, and documenting it
   as *the* answer is a legitimate outcome of this request.
3. **Streaming on the one-shot path.** `exec` captures stdout/stderr and relays them at the end
   (`crates/vmcell-cli/src/main.rs:667`–`:700`); `--stdin`/`--tty` route through a session and
   stream. A test suite that prints for minutes and then dumps is usable but poor, and vmcell's own
   §17 already records that the exec reply has no size ceiling of its own. Either stream the one-shot
   path or document `--stdin` as the runner spelling.

**What must not regress.**

- **Nothing on the fast path.** A cell that passes no `--push` sends no extra frame and mounts
  nothing; boot cost is unchanged.
- **Snapshot eligibility.** `put_file` rides the existing vsock control plane and attaches no
  vhost-user device, so law S1 (`config_has_vhost_user_device`) is untouched. This is the specific
  reason not to route the request through shares.
- **Determinism.** A pushed file is per-VM runtime state, not an artifact input; no cache key moves,
  unlike the pack-time `ExtraFile` path whose identity fold already exists
  (`fold_rootfs_injection_identity`).
- **The unprivileged mode.** Everything above works with `NetConfig::None` (the default,
  `crates/vmcell/src/config.rs:1054`–`:1056`) and needs no host capability.

**How the requester would verify it.** Build `p8_packaging` on the host; `vmcell run --push` it into
a cell; assert the process exits with the binary's own code for both a passing and a deliberately
failing test (`--exact` on a known-red name), because an exit code that is always 0 is the same
assertion-free shape this project calls a tell. Then repeat with a binary over 16 MiB and assert it
either transfers or **fails loud naming the cap** — a silent truncation would be the worst possible
outcome of this request.

**Workaround in force.** `vmcell run --disk-rw <image>` (`crates/vmcell-cli/src/main.rs:151`–`:153`)
attaches a caller-supplied raw image read-write; the guest kernel has `CONFIG_EXT4_FS=y` and the base
image ships `usr/bin/mount`, so a host-built ext4 image carrying the test binary can be mounted
in-guest by the exec'd command itself. It works and needs no vmcell change. It also means building a
filesystem image on the host per run, and it puts a `sh -c 'mount … && exec …'` wrapper between the
harness and the binary under test, which is exactly the layer a target runner exists to remove.

---

## R2 — A declared writable-scratch contract

**The capability, in one sentence.** An opt-in, per-VM scratch mount whose filesystem semantics —
`security.*` xattrs, POSIX ACLs, device nodes, setuid — vmcell *states* and proves in its own
conformance battery, instead of a consumer inferring them from the agent's mount sequence.

**The consumer need.** Two items rest on exactly this:

- **A** — plan §18 item 52's residual, verbatim at `docs/44-implementation-plan-claude-fable-v17.md:2192`–`:2194`:
  *"clause (d)'s strip could not be exercised against a real capability-carrying file … so the
  matcher is proven and the `setcap` removal is not."* `sweep_orphans`
  (`devprep/src/linux/install.rs:239`) unlinks files carrying a `security.capability` xattr; the
  fixture must be a file that really carries one. The walker is already testable unprivileged through
  an injected reader (`install.rs:181`, rationale at `:172`–`:176`), and the existing test
  (`install.rs:492`) uses a `fake_getcap` closure and asserts the *real* `getcap` path only in its
  negative direction (`install.rs:528`, `:532`).
- **C** — `grant_user_rw` (`sys/src/lib.rs:1278`). Worth stating precisely, because the shorthand is
  wrong: it does **not** call `setfacl` and links no ACL library. It hand-encodes the POSIX ACL blob
  (`sys/src/lib.rs:1229`) and writes it with
  `setxattr("/proc/self/fd/<n>", "system.posix_acl_access", …)` (`:1314`–`:1328`) against an
  `O_PATH` fd it has `fstat`ed and confirmed is `S_IFCHR` (`:1286`–`:1312`). So the requirement is
  not "a filesystem with `setfacl`" but "a filesystem on which the kernel honours a
  `system.posix_acl_access` setxattr against a caller-owned **character device**". devpts does not
  qualify — measured this session, `setfacl` on a caller-owned pts slave returns `Operation not
  supported` — so the free route is closed and the fixture needs `mknod`.

**What exists today.** Both work — by accident. The tmpfs behind the overlay is mounted with
`MountFlags::empty()` and no data string (`crates/vmcell-guest-agent/src/main.rs:186`–`:192`), so
there is no `nodev` to block `mknod` and no `nosuid`; and the kernel carries `CONFIG_TMPFS_XATTR=y`
and `CONFIG_TMPFS_POSIX_ACL=y`. Nothing declares any of that, nothing gates it, and vmcell has a
recorded premise pointing the other way: the packer strips every xattr from every rootfs node
unconditionally — `xattrs: vec![]` on all six node kinds
(`crates/vmcell/src/artifact/tar2erofs.rs:132`, `:139`, `:152`, `:170`, `:188`, `:194`), pinned by
`test_pax_xattrs_are_not_preserved` (`:797`–`:824`), with the rationale at `:121`–`:127`: *"the guest
agent and every in-guest `exec` run as root (§4.2), so file capabilities are moot; the erofs
Node/XattrSpec plumbing exists but is unused."*

That premise is correct for vmcell's own content and wrong for a consumer's test fixture, where the
file capability is the thing under test. The request is not to reverse it. It is to **scope** it: the
platform keeps stripping its own baked content, and gains one place where a consumer's runtime
fixture is guaranteed to behave.

**Why this is a platform capability, not a bolt-on.** "A writable mount whose semantics are stated"
is the filesystem analogue of `VmmCapabilities` — vmcell's own doctrine is that a facility is
*reported, never assumed*, and that a narrow claim keeps a narrow name
(`crates/vmcell/src/vmm/mod.rs`, the `usb_host_passthrough` comment). Today the guest filesystem is
the one subsystem with no such descriptor. Other consumers of exactly this: anything testing package
installation, where file capabilities on binaries like `ping` are the observable; container-runtime
tests, where device nodes in a private tree are the observable; any code implementing ACL-based
permissions; anything testing setuid semantics.

**Interface sketch** (naming vmcell's to choose). Ride the existing `vmcell_share=` idiom, which is
already the design's answer to "the agent must be told to mount something"
(`crates/vmcell/src/config.rs:973`–`:988`, parsed at `crates/vmcell-guest-agent/src/main.rs:118`–`:145`):

```rust
pub struct ScratchMount {
    pub guest_path: PathBuf,        // absolute; rejected if it collides with a reserved path
    pub size_mib: Option<u32>,      // None → kernel default
    pub semantics: FsSemantics,     // what the caller needs; build() rejects what it cannot serve
}
pub struct FsSemantics { pub xattr: bool, pub posix_acl: bool, pub dev_nodes: bool, pub setuid: bool }
// cmdline token: vmcell_scratch=<guest_path>:<size_mib>:<xattr,acl,dev,suid>
// CLI:           vmcell run --scratch /scratch:64:xattr,acl,dev
```

The honest home for the *claim* is not a new `VmmCapabilities` field — this is a guest-kernel
property, not a backend one, and putting it there would make four backends declare a stance on
something none of them controls. It belongs in `vmcell-artifact-validator`'s battery as a check that
proves each declared property **on the data plane**: set a `security.capability` xattr and read it
back, set an ACL and read it back, `mknod` and open the node. The static half is already available
through `KconfigValues` (`crates/vmcell-artifact-validator/src/kconfig.rs`) once R4 makes the
resolved config universally readable.

**The naive fix is a one-way switch, and this request explicitly is not it.** "Make the rootfs ext4"
would deliver a writable, xattr- and ACL-capable filesystem in one line — and charge every cell that
never needs one. The erofs root is read-only and shared with **no per-VM copy**, so the host page
cache holds a single image for all concurrent guests (design §4.1, §8.3); it has no journal, which
removes journal-recovery panics on read-only mounts and concurrent-mount corruption; and
`RootfsSource::Block` already exists as the ext4 fallback carrying `rootflags=noload` for exactly
those reasons (`crates/vmcell/src/config.rs:369`–`:376`, `:601`–`:607`). A scratch mount costs cells
that do not ask for it precisely nothing.

**What must not regress.**

- **The erofs root, unchanged.** No new rootfs variant, no writable root, no per-VM image copy.
- **The overlay upper, unchanged.** The scratch mount is a *separate* tmpfs at a caller-named path,
  not a change to the mount flags of the writable layer every cell already gets.
- **Snapshot eligibility.** tmpfs is not a vhost-user device, so law S1 is unaffected — unlike a
  share, which is rejected outright with `snapshotting`.
- **PID-1 discipline.** The mount is best-effort and outside the fatal core set
  `{overlay, /proc, /dev}` (`crates/vmcell-guest-agent/src/main.rs:193`–`:197`, `:210`–`:212`,
  `:270`–`:273`): a scratch mount that fails logs and continues, exactly as `/sys`, devpts and the
  share mounts do. A new fatal mount in PID 1 would be a regression in kind.
- **Determinism.** No artifact input moves; `size_mib` and the flag set are per-VM config.

**How the requester would verify it.** Boot with `--scratch /scratch:64:xattr,acl,dev`; in-guest,
write a file there, set `security.capability` on it, read it back and assert the value; `mknod` a
character device, `chown` it to the caller, apply the `system.posix_acl_access` blob
`sys/src/lib.rs:1229` produces, and read it back. Then run `sweep_orphans` against the directory and
assert the capability-carrying stray is gone and the keep is not — which is item 52(d)'s owed
measurement, executed. The fail-first proof is the inverse: with `dev_nodes: false` requested, the
`mknod` must fail, or the flag asserts nothing.

---

## R3 — A supported seam for consumer-owned in-guest content

**The capability, in one sentence.** A caller can add its own files and its own helper binary to the
image vmcell builds — the canonical rootfs, not only a hand-rolled OCI derivation — through a
documented, cache-correct seam.

**The consumer need, and it bites before any new work starts.** `read_caps`
(`devprep/src/linux/install.rs:291`–`:303`) shells out to **bare `getcap`**:

```rust
let out = Command::new("getcap").arg(path).output()?;
```

There is no `which` probe, no skip, no `required` gate, no graceful arm — a missing `getcap` becomes
`ErrorKind::NotFound`, propagated by `?`. The base image does not have it: the layer listing shows
`usr/lib/x86_64-linux-gnu/libcap.so.2` and `libacl.so.1` present but **no** `getcap`, `setcap`,
`setfacl`, `getfacl` or `capsh` — the libraries, not the `libcap2-bin` and `acl` command-line
packages. So `devprep/src/linux/install.rs:492`
(`the_sweep_removes_a_blessed_stray_and_never_the_copy_that_belongs`), which calls the real
`sweep_orphans` at `:528`, **panics inside a cell** on a plain `cargo test --workspace` — a red test
that has nothing to do with the work being attempted. Four sibling tests in the same module are safe;
they return before `read_caps` runs.

The same seam covers item A's other half: creating the fixture wants `setcap`, and B wants a launcher
binary of the consumer's own that installs a seccomp filter and execs a blessed copy.

**What exists today, and where it stops.** `ExtraFile { dest, src, mode }`
(`crates/vmcell/src/artifact/rootfs/mod.rs:36`–`:46`) is a genuine, well-specified mechanism: mode is
honoured explicitly rather than heuristically (`:41`–`:45`), reserved dests are rejected by
`is_reserved_injection_path` (`:81`–`:101`), duplicates are rejected, and the cache identity folds
`(dest, mode, content-hash)` in sorted order. Two limits:

- **It is reachable only from `vmcell oci2-erofs`** (`crates/vmcell-cli/src/main.rs:106`–`:113`).
  `vmcell build` — the command that produces the canonical rootfs — takes no `--inject`, stated in a
  comment at `main.rs:468`. So adding one file means re-deriving a ~98 MB image from a digest-pinned
  base rather than extending the one vmcell already built.
- **It is a build-time artifact knob, not a per-VM one** (`rootfs/mod.rs:107`–`:109`): it never passes
  through `VmConfig`. That is the right design — the image is the artifact — but it means the only
  per-VM route is `put_file`, which lands files non-executable (R1).

**Why this is a platform capability, not a bolt-on.** vmcell hit this exact wall itself and answered
it by writing a whole crate. Design §4.4: *"The minimal Debian base omits `iproute2`, `curl`, and
`cpu-checker` — tools a handful of integration tests need … Rather than bloat the rootfs with distro
packages or weaken the tests, the harness ships a small Rust multicall binary,
`vmcell-guest-tools`"* — four applets, `["ip", "curl", "kvm-ok", "echo-server"]`
(`crates/vmcell-protocol/src/lib.rs:162`), injected with one symlink each and put first on the guest
`PATH`. That is precisely the shape a consumer needs, built once for vmcell's own content. **The
request is to generalize the seam, never to add content**: the mechanism belongs in vmcell, the
applets belong to whoever needs them. Any consumer whose guest workload needs a tool the minimal base
omits is the same case, and the pattern is the one vmcell already promotes for kernel fragments —
`examples/downstream-kernel/` proves an out-of-tree consumer can extend the pins registry without
forking it, and the same proof shape applies here.

**Interface sketch** (naming vmcell's to choose), in increasing order of ambition:

1. **`vmcell build --inject dest=…,src=…,mode=…`** — the same repeatable flag `oci2-erofs` already
   parses (`crates/vmcell-cli/src/main.rs:800`–`:836`), threaded to the `RootfsStage` field the
   packer already reads. This is the small, complete increment.
2. **A pins-overlay spelling**, so the injection set is data rather than argv and rides
   `VMCELL_PINS` like everything else: `rootfs_extra_files: [{dest, src, mode}, …]`. The overlay
   merge already inserts keys absent from the baseline
   (`crates/vmcell/src/artifact/mod.rs:996`–`:1010`) and the strict top-level parse already names an
   unknown namespace (`:945`–`:978`), so a new namespace is a bounded change.
3. **A named "consumer guest binary" stage** — the documented generalization of `GuestToolsStage`:
   a caller points at a crate, vmcell builds it for the guest triple, injects it, and folds its
   content into the rootfs cache key. This is what "make it easier to develop an additional in-VM
   binary" means concretely, and it is the difference between a consumer copying
   `GuestToolsStage` and a consumer *calling* it.

**What must not regress.**

- **The canonical rootfs stays byte-identical** for a caller that injects nothing. Injections are
  additive and already fold their own identity, so an empty set must fold to the current key.
- **The reserved-path guard stays authoritative.** `is_reserved_injection_path` covers the agent, the
  two CA trust-store paths and everything under `vmcell-tools/`; extending the injection surface must
  extend nothing about what a consumer may shadow.
- **Cache correctness.** A content change must re-pack — the recorded v20 precedent (an identity-fold
  change without a `STAGE_VERSION` bump serving stale images) applies to any new fold.
- **Law G1.** vmcell ships the mechanism; `getcap` and `setfacl` are consumer content and stay
  consumer content.

**How the requester would verify it.** Build a rootfs with `libcap2-bin`'s `getcap` and `acl`'s
`setfacl` injected; boot; `stat` and execute both in-guest before the first `exec` of the payload;
then run `cargo test -p serial-nexus-devprep` in the cell and assert
`the_sweep_removes_a_blessed_stray_and_never_the_copy_that_belongs` passes rather than panicking.
The fail-first proof is the same run against an image without the injections, which must fail with
`getcap` named.

**Consumer-side work this does not excuse.** `read_caps` shelling to a bare PATH binary is a
consumer-side fragility regardless of vmcell: the `security.capability` xattr can be read directly
with `getxattr`, exactly as `grant_user_rw` already writes `system.posix_acl_access` directly
(`sys/src/lib.rs:1314`–`:1328`). Doing that would remove one of the two reasons this request exists
and is worth filing here whether or not R3 lands.

---

## R4 — Every kernel self-describing and citable

**The capability, in one sentence.** Every `vmlinux` vmcell hands a caller — including the prebuilt
bootstrap seed — arrives with a machine-readable statement of what it is: its resolved
post-`olddefconfig` configuration and an identity record a consumer can quote in its own artifact.

**The consumer need.** Plan §18 item 8, "Kernel-of-record closure"
(`docs/44-implementation-plan-claude-fable-v17.md:1144`–`:1158`). The prize is not a version number,
it is the **controlled variable**. This project's comparison discipline is stated at
`docs/doctor/README.md:174`: two captures pair *"on a basis stronger than either digest"* because
`git diff` over the doctor source is empty — same instrument, same adapters, same cable, same wiring,
only the kernel differs. The report's own identity fields are `build.probe_set` and
`build.field_set`, and the era is `4317ea5ac187f506`. A kernel-of-record row is only quotable if
everything except the kernel is provably held fixed.

Separately, R2's whole argument rests on symbols (`CONFIG_TMPFS_XATTR`, `CONFIG_TMPFS_POSIX_ACL`,
`CONFIG_SECCOMP_FILTER`) that a consumer currently cannot assert against the kernel it was actually
handed.

**What exists today — most of it, which is why this request is small.**

- Every labelled kernel shares the default namespace's `microvm_config`; the flattener emits no
  per-label config, stated in code at `crates/vmcell/src/artifact/kernel.rs:519`–`:527`. **So the
  input is already the controlled variable.** The resolved outputs necessarily differ —
  `vmlinux-6-6-143.config` is 141369 bytes against `vmlinux-6-12-94.config`'s 144979 — and that
  difference is exactly what a consumer needs to see and state.
- Both *compiling* producers publish the resolved config beside the kernel as
  `vmlinux-<label>.config` through one shared law (`kernel.rs:268`–`:273` for the path,
  `:303`–`:319` for the publish; host-`make` at `:751`–`:756`, in-VM at
  `crates/vmcell-kernel-builder/src/lib.rs:204`–`:210`), and it is named contract surface
  (`README.md:44`–`:46`).
- `vmcell bundle` already writes a digest-pinned manifest covering every labelled kernel and its
  sidecar (`crates/vmcell-cli/src/main.rs:554`–`:575`), and `verify-bundle` re-hashes it.
- `KconfigValues::parse` gives the assertion primitive
  (`crates/vmcell-artifact-validator/src/kconfig.rs`), and `examples/downstream-kernel/` proves the
  whole loop from a consumer's position.

**Where it stops.** The prebuilt arm — the default for `vmcell build`
(`crates/vmcell-cli/src/main.rs:70`) — **deletes** the sidecar (`clear_resolved_config` at
`kernel.rs:958`, the removal law at `:336`–`:353`, gated by
`test_prebuilt_kernel_clears_a_stale_resolved_config` at `:1393`–`:1439`) and refuses both labels and
fragments (`reject_labelled_prebuilt`, `:367`–`:379`). The reason is sound: leaving a stale config
would digest-pin a description of a *different* kernel as this one's. The consequence is that the
kernel most callers actually boot is the one kernel whose configuration cannot be read — and there is
no runtime fallback either, because `CONFIG_IKCONFIG` is `not set` in every kernel this tree builds
(`vmlinux.config:166`), so `/proc/config.gz` does not exist.

**Why this is a platform capability, not a bolt-on.** "An artifact states what it is" is the same law
as the pins discipline and `verify-bundle`; the sidecar is already contract surface and the gap is
one producer that has nothing to publish. Consumers of exactly this: anyone doing a cross-kernel A/B
of any kind — perf-regression bisection across LTS lines, syscall-behaviour differences, driver
behaviour — and anyone who must cite a kernel in a durable artifact rather than in scrollback. The
platform already treats kernel-as-a-dimension as a first-class idea (design §5.5, and the recorded
payoff of *disproving* a wrong belief with it).

**Interface sketch.** Three clauses, first two independent:

1. **The prebuilt pin gains an optional config.** Beside `kernel_prebuilt`'s existing `url` /
   `archive_sha256` / `archive_member` / `sha256` (`pins.json:22`–`:27`), an optional
   `config_archive_member` + `config_sha256`, published through the same
   `publish_resolved_config` law. A seed whose publisher ships no config keeps clearing the sidecar
   and says so — the honest arm stays honest.
2. **A per-kernel identity record**, or the `bundle` manifest promoted to named contract surface: the
   kernel's version banner, the resolved-config sha256, the pins-overlay digest, and which producer
   built it. A consumer embeds that record beside its own measurements the way this project embeds
   `build.probe_set` in every doctor artifact.
3. *Named as the alternative, with its cost, so the platform can choose.* Adding
   `CONFIG_IKCONFIG=y` + `CONFIG_IKCONFIG_PROC=y` to `pins.json`'s `microvm_config` would make every
   kernel vmcell **builds** self-describe at runtime, at a few tens of KB in a 54 MB image. It does
   nothing for the prebuilt seed, and it has a real cost the platform should weigh: it would make
   `examples/downstream-kernel/`'s data-plane proof **vacuous**, since that example's whole point is
   a fragment whose survival is observable only as `/proc/config.gz` appearing. Clause 1 is the
   request; this is recorded so the cheaper-looking option is not chosen without its consequence.

**What must not regress.**

- **The prebuilt fast path.** No kernel compile, no toolchain requirement, and the one-time download
  gains at most a small sidecar.
- **The no-stale-config invariant.** A seed with no published config must keep clearing the sidecar
  rather than inventing one; the existing gate at `kernel.rs:1393`–`:1439` should still pass
  unchanged for that arm.
- **Determinism and content addressing.** Any new sidecar is content-addressed with its kernel, as
  the current one is (`crates/vmcell/tests/kernel_toolkit.rs:351`–`:389` already asserts a vanished
  sidecar forces a rebuild).
- **The example's non-vacuity**, per clause 3.

**How the requester would verify it.** Build two labelled kernels from one overlay; assert both
sidecars exist and that `KconfigValues` reports `CONFIG_TMPFS_XATTR` and `CONFIG_TMPFS_POSIX_ACL`
built-in in each; `diff` the two sidecars and record the residue in the same artifact as the
measurement, so the reader can see exactly what else moved. Then run the doctor's hardware-free tier
under each kernel with `probe_set`, `field_set` and the doctor source tree all held fixed, and
compare cell-for-cell.

**The honest limit, stated because it bounds what item 8 a cell can close.** A cell cannot run the
crossover tier. There are no USB adapters, no `CONFIG_USB_SERIAL` and no `CONFIG_USB_ACM` in any
vmcell kernel (`vmlinux.config:3809`, `:3760`, and identically in the other three), no udev and so no
`/dev/serial/by-id`, and the USB-passthrough path is QEMU-only
(`crates/vmcell-qemu/src/lib.rs:1437`), addressed by `vid:pid` rather than by port
(`crates/vmcell/src/config.rs:697`–`:702`), rejected outright with `snapshotting`
(`config.rs:1600`–`:1606`), and `#[ignore]`d behind `VMCELL_TEST_USB_DEVICE`
(`crates/vmcell/tests/usb_passthrough.rs:39`, `:411`–`:413`). What a cell *can* close is the
hardware-free half — and that half is where this project's most kernel-sensitive recorded facts
actually live: P13's `retains` versus `waits-then-discards`, the pts close-wait figures, the
`POLLHUP` timings. Those are pty facts, not USB facts, and they are precisely the ones that would
benefit from being read on several kernels with everything else held fixed.

---

## E — systemd as PID 1: not requested, and why

**The item.** Plan §18 item 31's remaining rows
(`docs/44-implementation-plan-claude-fable-v17.md:1686`–`:1758`). Four `unverified` entries in
`packaging/README.md` — the socket-group static-identity recipe (`:325`), the operators-group
paragraph (`:339`), the upgrade `cp` procedure (`:344`), and the clause that the daemon runs under
the resulting sandbox (`:327`) — and the summary says it plainly at `:373`: they *"want a live
systemd to start a unit on"*.

**Replacing PID 1 is structurally at odds with vmcell's architecture, and this document says so
rather than asking.** `VmConfig::init` exists (`crates/vmcell/src/config.rs:98`, builder at `:1328`),
and its own field documentation states the cost: a custom PID 1 *"replaces the agent — so the VM has
no control plane (`MicroVm::agent` fails loud) and cannot snapshot … A custom init also loses the
agent's tmpfs overlay over the RO erofs root"* (`config.rs:88`–`:97`). It is enforced in four places
(`orchestrator.rs:1750`–`:1758`, `orchestrator.rs:1866`–`:1873`, `config.rs:1555`–`:1562`,
`orchestrator.rs:2238`). Asking vmcell to keep the control plane while surrendering PID 1 is asking
it to give up law C1, the Ready handshake, `exec`, sessions, `Resync` and the entire snapshot tier —
which is not a feature request, it is a different product.

**And the rootfs would not carry it anyway.** The digest-pinned Debian base has no systemd binary and
no `systemctl` — the layer listing shows `etc/systemd/system/…` unit files and
`usr/lib/systemd/system/*.service` shipped by other packages, and **no** `usr/lib/systemd/systemd`,
**no** `usr/bin/systemctl`. So E needs R3 *and* a PID-1 change, and even then it would be testing
systemd-in-a-container rather than systemd-as-init.

**The general capability that would be adjacent, named for completeness, not requested.** vmcell
*could* offer namespace-scoped exec — run the payload as PID 1 of a fresh PID + mount namespace, the
agent staying PID 1 of the VM. The kernel supports it (`CONFIG_PID_NS=y`, `CONFIG_NAMESPACES=y`;
`CONFIG_USER_NS` is not set, which is fine because guest exec is already root), it would serve
init-system testing, container-runtime testing and anything needing a private `/proc`, and it costs
cells that do not use it nothing. **This project is not asking for it**, because it would not close
item 31: what item 31 needs is systemd's *own* behaviour under `DynamicUser=`, `StateDirectory=` and
`ReadWritePaths=`, and a claim about systemd is not made true by any capability this project or this
platform holds.

**The recommendation is to keep E on a systemd CI runner**, which is where it already is and where it
is already working. CI's `packaging` job reads `PID 1: systemd` and passwordless sudo on
`ubuntu-latest`, ran item 31's sandbox measurement green on 2026-08-13, and has demanded it with
`SNX_PACKAGING_ROOT=required` since 2026-08-15 (`.github/workflows/ci.yml:399`–`:410`). That is the
right instrument for the question, and a cell would be a worse one even if every request above landed.

The same reasoning has a precedent in this tree, filed 2026-08-15: plan §18 item 78 was split out of
item 31 precisely because *"root cannot conjure a device node"* — a clause routed as "needs a root
box" that no privilege could ever satisfy. E's residue is the same shape one axis over.

---

## Considered and not requested

Each of these was worked through and rejected as too single-use, as a one-way switch, or as the wrong
instrument. They are recorded so the same ground is not re-covered.

- **USB-serial support in a vmcell kernel, or a USB-serial passthrough path.** This is consumer
  content in the platform (law G1), and it would not work: passthrough is QEMU-only, addressed by
  `vid:pid` rather than by port so a cross-wired *pair* of identical FTDI adapters is not addressable
  at all, snapshot-incompatible, and gated on a designated device. The mechanism a consumer would
  need already exists as the pins-overlay fragment route — a consumer's own `USBSERIAL` fragment in
  its own overlay, exactly as `examples/downstream-kernel/` demonstrates with `IKCONFIG`. And the
  hardware work belongs on the rig regardless. Worth correcting one point for the record: vmcell does
  **not** merely detach the host driver — QEMU's libusb detaches implicitly, and vmcell records each
  interface's prior driver and re-binds it on teardown with a 5 s budget
  (`crates/vmcell/src/vmm/usb.rs:11`–`:35`, `:49`, `:154`).

- **Switching the rootfs to ext4 to get a writable, xattr-capable filesystem.** The one-way global
  switch this document exists to avoid. It would charge every cell the loss of the shared-page-cache
  density lever and the journal-free property, to serve a fixture directory. R2 is the opt-in form.

- **`--share` on `vmcell run`.** Rejected on three verified grounds rather than on taste: shares need
  host `CAP_SYS_ADMIN` (`crates/vmcell/src/hostcaps.rs:30`, `:45`, `:114`); they are snapshot-
  ineligible by law S1 (`config.rs:1569`–`:1573`); and virtiofsd is spawned with exactly
  `--socket-path … --shared-dir … --cache=… --sandbox=namespace [--readonly]`
  (`crates/vmcell/src/fs.rs:123`–`:138`) — **no `--xattr`, no `--posix-acl`, no `--xattrmap`
  anywhere in the tree**. So a share could not carry either the capability xattr or the ACL even if
  it were the right transport. This is stronger than "unproven": it is proven absent.

- **Asking vmcell to install `libcap2-bin` and `acl` in its rootfs.** Consumer content again. R3 asks
  for the seam instead, which is the same answer vmcell gave itself when it wrote
  `vmcell-guest-tools` rather than adding distro packages.

- **A guest-side privilege API — dropping to a non-root uid, or capability control inside the cell.**
  Not needed: guest exec is already root with the full bounding set, so this would be asking for
  *less*. It would also be load-bearing in the wrong direction — the packer's xattr strip is
  justified *by* the everything-runs-as-root premise (`tar2erofs.rs:121`–`:127`), so introducing a
  non-root uid without R2's declared semantics would silently break any capability-dependent binary.

- **Anything that gives a cell's caller root on the *host*.** Out of scope by construction; the point
  of a cell is that the boundary is the VM.

---

## What this project would do on its side

Three items, recorded here because a feature request that hides the consumer's own work is dishonest
about the cost.

1. **Two existing tests break in a cell, in opposite directions.**
   `devprep/src/linux/install.rs:492` panics *without* `getcap` (R3). And
   `sys/src/lib.rs:2154` — `the_blob_is_well_formed_enough_for_the_kernel_to_reach_the_permission_check`
   — breaks *with* uid 0: it asserts `expect_err` and `EPERM` on `/dev/null`
   (`:2157`–`:2163`) on the premise that *"an unprivileged process cannot set an ACL on a root-owned
   node"*, which as root simply succeeds. It is gated by nothing but
   `#[cfg(all(test, target_os = "linux"))]`. Both need a uid-aware arm before any suite is run at uid
   0 anywhere — cell or CI root arm.

2. **`read_caps` should read the xattr, not shell out.** See R3's closing note.

3. **A cell lane would be a tenth `required` gate**, and the discipline for that is already written:
   measure the precondition on a real runner first, then demand it
   (`itest/tests/p8_packaging.rs:176`–`:188`, plan §18 item 31 at `:1745`–`:1748`). Shipping a
   `required` mode on an assumption reddens a lane for someone else's environment, and a lane whose
   passing output is identical to its self-skipping output asserts nothing.

## What "landed" would look like

**R1 alone** makes **D** runnable: the §10 control-socket Root arm needs only a process at euid 0 and
a writable `/run`, and `/run` is present in the base image and writable through the overlay upper.
The gap there is narrower than "untested" — the policy function's Root arm already has a unit test
that deliberately passes `is_root = true` alongside a set `XDG_RUNTIME_DIR` so an
order-of-checks bug fails rather than passes by accident (`rpc/src/socket.rs:256`), and
`itest/tests/p12_web_socket_default.rs:229`–`:243` carries a root arm that collapses all three cases
to `/run` rather than skipping. What is missing is a real euid-0 process actually binding
`/run/<name>.sock`.

**R1 plus R2** makes **A**, **B** and **C** measurable. B is easy to under-scope: beyond a launcher
holding `CAP_SYS_ADMIN` that installs a seccomp filter making `prctl(PR_SET_NO_NEW_PRIVS)` fail, it
needs a *blessed copy* — a binary really carrying `cap_dac_override` or `cap_fowner`, since
`unhardened_disposition` takes the `Refuse` arm only when `caps_held()` reports permitted or
effective (`devprep/src/linux/mod.rs:253`–`:259`, `:268`–`:274`). That copy has to live on a
filesystem where a `security.capability` xattr is honoured, which is R2 exactly. And setting
`no_new_privs` yourself to dodge `CAP_SYS_ADMIN` is self-defeating: the kernel then strips file caps
at exec, `caps_held()` reports nothing, and the run silently exercises the `Report` arm instead.

**R1 plus R3** turns the red test green and makes A's `getcap` half work. **R4** converts item 8's
hardware-free half from "one visit per box" into a repeatable, citable comparison across kernels that
differ in the one variable.

Not one of those requires vmcell to grant a privilege it does not already grant, to weaken an
isolation boundary, to change the erofs root, or to slow down a cell that asks for none of it.
