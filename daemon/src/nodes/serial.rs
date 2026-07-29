//! Serial port node (design §7.1). Faces host in the normal role.
//!
//! The device is named by a resolver identity (`usb:vid:pid:serial:iface`, a
//! `by-path:` topology port, or a `raw:`/`/dev` path) — [`serial_nexus_core::Resolver`]
//! turns that into the current `/dev` path at every open and every reconnect
//! recheck (§12). A missing device does not fail the load: the node comes up
//! `waiting` and heals when its device reappears (§7.1 faulted-and-wait).
//!
//! Byte flow: the `serial2` fd is driven directly — *not* through `serial2-tokio`
//! (which hides the fd we need for `TIOCEXCL`/`TIOCGICOUNT`) nor
//! `tokio::io::unix::AsyncFd` (whose epoll readiness busy-loops on pty masters,
//! §15.18). The drain loop needs the fd non-blocking, and the pinned `serial2` opens
//! it `O_NONBLOCK | O_NOCTTY` and never clears it — so [`arm_reader`]'s
//! `set_nonblocking` is a deliberate redundancy rather than the thing that makes the
//! fd non-blocking: the readiness loop's correctness must not rest on a dependency's
//! open flags, which are not part of its API (37-SER-3 corrected this prose, not the
//! call). The **hostward reader runs on a dedicated blocking thread** — a `poll(2)`
//! blocking wait wakes it the instant the device has data, so it drains at line rate
//! and costs zero CPU while parked (§15.19). It broadcasts to every attached consumer
//! (lossy `try_send`, §5). The low-rate **targetward writer** and the **reconnect
//! supervisor** share one async task on the runtime thread; read and write on the
//! shared fd are independent directions.
//!
//! **Faulted-and-wait (§7.1).** The reader thread signals device loss (an exit on
//! `POLLHUP`/EOF/error) by pulsing a `Notify`; the supervisor then joins it, drops
//! the port, transitions to `waiting`, and polls the resolver for the device's
//! reappearance *by the same identity* (a squatter on the old `/dev` path resolves
//! to a different identity and is refused, §12). On reappearance it runs the
//! **reopen ritual** — reapply termios, retake `TIOCEXCL`, restore the configured
//! modem lines against auto-reset adapters, set non-blocking — then purges the
//! outage-era targetward backlog (`purge_on_reconnect`, the one sanctioned drain
//! of the never-drop targetward path, counted) and re-arms the reader and writer.

use std::cell::Cell;
use std::os::fd::AsRawFd;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use nix::poll::PollFlags;
use serde_json::json;
use serial_nexus_core::Chunk;
use serial_nexus_core::config::{
    DataBits, FlowControl as CfgFlow, ModemLines, NodeConfig, Parity as CfgParity,
    StopBits as CfgStop,
};
use serial_nexus_core::resolver::{DeviceKind, Resolver};
use serial_nexus_core::state::{NodeState, NodeStatus};
use serial2::{CharSize, FlowControl, Parity, SerialPort, Settings, StopBits};
use tokio::sync::Notify;
use tokio::sync::mpsc;

use crate::boundary::{self, BlockingReader, TaskSet};
use crate::cell::CriticalCell;
use crate::runtime::{self, READ_BUF, SharedFanOut};
use crate::tap::TapFeed;
use serial_nexus_sys as sys;

/// Re-arm interval (ms) for the serial reader thread's blocking readiness poll.
/// A live device wakes it far sooner; on teardown the thread observes its stop
/// flag within this bound, so `join` returns promptly.
const READ_POLL_TIMEOUT_MS: u16 = 200;

/// How often the supervisor rechecks the resolver for a `waiting`/`faulted`
/// device's reappearance (§7.1 "a stat every second or two"). Cheap and portable
/// (by-id readlink + sysfs walk), free of libudev/LGPL linkage (§15.10).
const RECONNECT_POLL: Duration = Duration::from_millis(1000);

/// The termios + modem parameters an open (initial or reopen) applies (§7.1).
#[derive(Clone, Copy)]
struct OpenParams {
    baud: u32,
    data_bits: DataBits,
    parity: CfgParity,
    stop_bits: CfgStop,
    flow: CfgFlow,
    /// Initial/retained modem-line assertions, restored on every (re)open so line
    /// states stay deterministic against auto-reset adapters (§7.1).
    modem: ModemLines,
    /// Discard the outage-era targetward backlog on reconnect (default on, §7.1).
    purge_on_reconnect: bool,
}

/// An open [`SerialPort`] that owns every **tty-level assertion this node made on it**:
/// the `TIOCEXCL` claim taken at open, and a break condition a signal verb left standing.
///
/// **A port carrying an assertion cannot be discarded without giving it back.** That is
/// the whole type. Both assertions live on the *tty*, not the fd, and outlive this fd's
/// close whenever the tty does — for `TIOCEXCL`, until the tty's last close, which a
/// pty-backed device whose master another process holds (socat `PTY,link=`, QEMU
/// `-serial pty`, `serial-nexus-sim pty`) never reaches; for break, until somebody issues
/// `TIOCCBRK`, which on a UART shared with a `load --replace` successor is *nobody*. So a
/// port dropped while still asserted leaves the device un-openable by every unprivileged
/// process on the machine, permanently, surviving `teardown` and daemon exit; the node's
/// own reconnect then reports a self-inflicted "Device or resource busy" in place of the
/// true cause, sending the operator hunting a squatter that does not exist (§15.38 D2/D3;
/// review 32 RV-8/CONC-4/SERX-1) — or, for break, leaves a successor that reports
/// `active`/`open: true` and accepts every `send` while transmitting nothing at all.
///
/// v13 put the release in `SerialNode::teardown` alone, and there are at least four
/// other paths that let a port go: `open_port`'s own `?` after the flag is taken (an
/// ordinary `[node.modem] dtr = true` on a pty-backed device `ENOTTY`s there),
/// [`set_waiting`], [`fault`], and the reopen arm where a successful `open_port` is
/// followed by a failing `arm_reader`. Enumerating exit paths is how the rule was lost
/// once; making the assertions an *owned resource* is how it stops depending on memory.
///
/// They are released **at most once**, and that matters: [`Self::release_claims`] flips
/// `claimed` so an ordered, deliberate release (teardown, before a `--replace` successor
/// opens the same tty) is not undone by a late `Drop` — which would strip `TIOCEXCL` off
/// the *successor's* port, or clear a break the successor itself just asserted, since
/// both are tty state and the successor shares the tty. Every path where a successor can
/// appear releases explicitly first, so `Drop` is the backstop for the paths where none
/// can.
///
/// The port is held as `Rc<ExclusivePort>` (shared with the supervisor's in-flight
/// future), so `Drop` fires at the *last* clone. That is the wanted semantics: the fd
/// must outlive the blocking reader thread that holds its raw number (§7.1 fd-reuse),
/// and the assertions must outlive nothing at all.
struct ExclusivePort {
    port: SerialPort,
    /// Whether this port still carries the assertions it is responsible for.
    claimed: Cell<bool>,
}

impl ExclusivePort {
    /// Wrap a port whose `TIOCEXCL` claim has just been taken.
    fn new(port: SerialPort) -> ExclusivePort {
        ExclusivePort {
            port,
            claimed: Cell::new(true),
        }
    }

    /// Give this node's tty-level assertions back now, rather than at drop. Idempotent.
    /// Best effort: a port whose fd the driver already invalidated (unplug) has nothing
    /// to release, and failing over that would strand the node — the reopen ritual
    /// re-takes exclusivity anyway.
    ///
    /// **Break is cleared here and nowhere else** (review 32 SERX-2, and the regression
    /// its first remediation shipped). `send-break`'s [`RestoreGuard`] is correctly
    /// scoped to the port the break was asserted on, so a `load --replace` straddling
    /// the hold makes it decline — which left the *tty* asserted with nobody to clear
    /// it, converting an `ms`-bounded outage into an unbounded one (reproduced on the
    /// bench crossover rig: the successor reports `active`, `send` reports bytes
    /// accepted, `driver_counters.tx` climbs, and the peer receives nothing,
    /// indefinitely). Break is tty state exactly as `TIOCEXCL` is, so it takes the same
    /// remedy and the same layer: *an assertion this node made is given back before the
    /// port is let go*, not after a sleep by whoever remembers. Putting it here rather
    /// than in a successor's `open_port` keeps it one place and keeps the ownership
    /// honest — the departing node cleans up after itself instead of relying on the next
    /// arrival to notice.
    ///
    /// Order matters: the break goes first, so the tty is transmitting normally before
    /// `TIOCEXCL` drops and anyone else can open it.
    ///
    /// DTR and RTS are deliberately **not** touched. Driving them on the way out is a
    /// reset pulse on every auto-reset board (§7.1), which is the unrequested edge this
    /// whole area exists to prevent; and unlike break they self-heal — the successor's
    /// own `open(2)` re-raises DTR on a real adapter, and `open_port` reapplies the
    /// configured `[node.modem]` levels.
    fn release_claims(&self) {
        if self.claimed.replace(false) {
            let _ = self.port.set_break(false);
            let _ = sys::set_exclusive(self.port.as_raw_fd(), false);
        }
    }
}

impl std::ops::Deref for ExclusivePort {
    type Target = SerialPort;

    fn deref(&self) -> &SerialPort {
        &self.port
    }
}

impl Drop for ExclusivePort {
    fn drop(&mut self) {
        self.release_claims();
    }
}

/// State the reconnect supervisor mutates and the node's `&self` methods read,
/// shared on the single runtime thread (Rc/[`CriticalCell`] — the reader thread
/// touches only atomics, never this).
struct SerialShared {
    /// The node's observed status *and the moment it entered it* (§7). Reported
    /// through [`NodeState`] so a flapping adapter's `waiting` stamp answers "since
    /// when?" rather than "when was the last reconnect poll?".
    status: NodeState,
    /// The currently-open port, or `None` while `waiting`/`faulted`. Retained for
    /// `state_extra` (driver counters) and the serial-signal verbs.
    port: Option<Rc<ExclusivePort>>,
    /// Monotonic port identity: bumped by [`SerialShared::set_port`] on every port
    /// transition, so an in-flight signal verb can tell "the port I asserted on" from
    /// "whatever port this node happens to hold now" without comparing raw pointers
    /// (§7.1; review 32 CTRL-1/SERX-2).
    generation: u64,
    /// Pulsed by the same transition, so a signal verb parked on its hold interval
    /// wakes at once instead of sleeping out a duration whose node is already gone.
    signal_gone: Rc<Notify>,
    /// The port generation on which a **line-holding** signal verb (`send-break`,
    /// `pulse-dtr`) currently holds a line, if one does (§7.1). One slot per port,
    /// because a physical line has one state: two overlapping holds — reachable
    /// today, since the one-waiting-verb rule is per *connection* — would have the
    /// shorter one's restore clear the line while the longer one still reported
    /// success for its full duration (37-SER-1). Keyed by generation rather than a
    /// bare flag so a slot left by a verb whose port has since gone reads as free
    /// for the successor port, exactly as [`RestoreGuard`] declines on it.
    signal_in_flight: Option<u64>,
}

impl SerialShared {
    /// Install a new port (or `None`) — **the one place a node's port changes**.
    /// Bumps the generation and wakes every in-flight signal verb, then hands the
    /// outgoing port back for the caller to release in a defined order. Deriving the
    /// bump anywhere else is how the validator-vs-runtime class of drift starts.
    fn set_port(&mut self, port: Option<Rc<ExclusivePort>>) -> Option<Rc<ExclusivePort>> {
        self.generation = self.generation.wrapping_add(1);
        self.signal_gone.notify_waiters();
        std::mem::replace(&mut self.port, port)
    }
}

/// Discard the node's current port, returning the tty-level assertions it carries —
/// `TIOCEXCL` and any standing break — *first* and in a defined order, before the
/// caller's critical section ends and a `--replace` successor opens the same device
/// (§15.38 D2, review 32 SERX-2). [`ExclusivePort`]'s `Drop` is the backstop that covers
/// the paths this helper cannot reach (an error inside `open_port`, a port never
/// stored); this is the ordered half, and the two agree by construction because the
/// assertions are released at most once.
///
/// **Every path that lets a port go reaches this helper** — `teardown`, [`set_waiting`],
/// [`fault`] — which is the property that makes "the port is handed on clean" a fact
/// about one function rather than a fact about four remembered call sites.
fn release_port(shared: &CriticalCell<SerialShared>) {
    if let Some(port) = shared.with_mut(|sh| sh.set_port(None)) {
        port.release_claims();
    }
}

pub struct SerialNode {
    pub name: String,
    /// The resolver identity this node is configured for (config, round-trips).
    device: String,
    resolver: Resolver,
    params: OpenParams,
    shared: Rc<CriticalCell<SerialShared>>,
    reader_slot: Rc<CriticalCell<BlockingReader>>,
    /// Bytes read from the port and discarded because nothing was attached to
    /// consume them (§5). Shared with the reader thread, hence atomic.
    discarded_unattached: Arc<AtomicU64>,
    /// Bytes drained from the targetward backlog and discarded on reconnect
    /// (§7.1 purge-on-reconnect). The one sanctioned targetward drop, counted.
    purged_reconnect: Arc<AtomicU64>,
    /// The supervisor task (drives the targetward writer and the reconnect poll).
    tasks: TaskSet,
}

impl SerialNode {
    pub fn create(config: &NodeConfig, resolver: &Resolver) -> SerialNode {
        let NodeConfig::Serial {
            name,
            device,
            baud,
            data_bits,
            parity,
            stop_bits,
            flow_control,
            purge_on_reconnect,
            modem,
            ..
        } = config
        else {
            unreachable!("SerialNode::create called with non-Serial config");
        };

        let params = OpenParams {
            baud: *baud,
            data_bits: *data_bits,
            parity: *parity,
            stop_bits: *stop_bits,
            flow: *flow_control,
            modem: *modem,
            purge_on_reconnect: *purge_on_reconnect,
        };

        // Initial open, synchronous so `state` is accurate the instant `load`
        // returns. Identity → current path (squatter-safe for identity forms,
        // §12); absence is `waiting`, any other open error is `faulted` — neither
        // fails the load (§15.8).
        let (status, port) = match resolver.resolve_current_path(device) {
            Some(path) => match open_port(&path, &params) {
                // `open_port` hands back an `ExclusivePort`, so its own error arms
                // below have already returned the claim (RV-8).
                Ok(p) => (NodeStatus::Active, Some(Rc::new(p))),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (
                    NodeStatus::Waiting {
                        reason: format!("device {device} vanished during open"),
                    },
                    None,
                ),
                Err(e) => (
                    NodeStatus::Faulted {
                        reason: format!("open {device}: {e}"),
                    },
                    None,
                ),
            },
            None => (
                NodeStatus::Waiting {
                    reason: format!("device {device} not present"),
                },
                None,
            ),
        };

        SerialNode {
            name: name.clone(),
            device: device.clone(),
            resolver: resolver.clone(),
            params,
            shared: Rc::new(CriticalCell::new(SerialShared {
                status: NodeState::new(status),
                port,
                generation: 0,
                signal_gone: Rc::new(Notify::new()),
                signal_in_flight: None,
            })),
            reader_slot: Rc::new(CriticalCell::new(BlockingReader::default())),
            discarded_unattached: Arc::new(AtomicU64::new(0)),
            purged_reconnect: Arc::new(AtomicU64::new(0)),
            tasks: TaskSet::default(),
        }
    }

    /// Start the data plane: broadcast hostward to the attached consumers, drain
    /// targetward from them, and supervise faulted-and-wait reconnect (§7.1). A
    /// single async supervisor owns the targetward receiver across outages, so a
    /// `waiting` port backpressures its origins (channel fills, §5) rather than
    /// dropping their commands; only the sanctioned purge-on-reconnect drains it.
    pub fn start(
        &mut self,
        hostward: SharedFanOut,
        targetward: Option<mpsc::Receiver<Chunk>>,
        tap_feed: Option<TapFeed>,
    ) {
        let ctx = SuperviseCtx {
            name: self.name.clone(),
            device: self.device.clone(),
            resolver: self.resolver.clone(),
            params: self.params,
            hostward,
            targetward,
            tap_feed,
            shared: self.shared.clone(),
            reader_slot: self.reader_slot.clone(),
            discarded: self.discarded_unattached.clone(),
            purged: self.purged_reconnect.clone(),
        };
        self.tasks.push(tokio::task::spawn_local(supervise(ctx)));
    }

    pub fn status(&self) -> NodeState {
        self.shared.with(|sh| sh.status.clone())
    }

    /// A handle on this node's live port for the serial-signal verbs (§7.1). `None`
    /// while the node is `waiting`/`faulted`.
    ///
    /// It deliberately carries **no `Rc<ExclusivePort>`**: a signal verb holds it
    /// across a sleep, and a strong clone there is what kept the device's fd open —
    /// with a break or a DTR level asserted on a physical line — past `remove-node`
    /// and `load --replace`, so a *successor* node's console was driven by a departed
    /// node's restore (reproduced on real hardware, review 32 CTRL-1/SERX-2). The
    /// handle instead records the port *generation*, and every access re-resolves
    /// through the shared cell and declines if the node has moved on.
    pub(crate) fn signal_handle(&self) -> Option<SignalHandle> {
        self.shared.with(|sh| {
            sh.port.as_ref()?;
            Some(SignalHandle {
                shared: self.shared.clone(),
                generation: sh.generation,
                gone: sh.signal_gone.clone(),
            })
        })
    }

    pub fn state_extra(&self) -> serde_json::Value {
        self.shared.with(|sh| {
            // Driver input counters (TIOCGICOUNT) surface framing/parity/overrun loss
            // "where supported" (§5, §7.1); a pts (test device) returns an error, so
            // report null rather than faulting or fabricating zeros.
            let driver_counters = sh
                .port
                .as_ref()
                .and_then(|p| sys::read_icounts(p.as_raw_fd()).ok())
                .map(|c| {
                    json!({
                        "rx": c.rx,
                        "tx": c.tx,
                        "frame": c.frame,
                        "overrun": c.overrun,
                        "parity": c.parity,
                        "brk": c.brk,
                        "buf_overrun": c.buf_overrun,
                    })
                });
            // The current resolved /dev path is state (§12); the configured identity is
            // config. Report both so an operator sees which device answered.
            let resolved_path = self
                .resolver
                .resolve_current_path(&self.device)
                .map(|p| p.display().to_string());
            let modem_lines = sh.port.as_ref().map(|p| read_modem_lines(p.as_raw_fd()));
            json!({
                "identity": self.device,
                "identity_kind": DeviceKind::of(&self.device).label(),
                "resolved_path": resolved_path,
                "baud": self.params.baud,
                "open": sh.port.is_some(),
                "discarded_unattached": self.discarded_unattached.load(Ordering::Relaxed),
                "purged_on_reconnect": self.purged_reconnect.load(Ordering::Relaxed),
                "modem_lines": modem_lines,
                "driver_counters": driver_counters,
            })
        })
    }

    /// Ask this node's workers to stop, without waiting (§16.1, BND-1). Aborts the
    /// supervisor — so it cannot re-arm a reader behind teardown's back — and sets
    /// the reader thread's stop flag; the thread is *not* joined here and the port
    /// is deliberately kept, since its fd must outlive the reader (§7.1 fd-reuse).
    /// Teardown calls this on every node first, then pays the join cost once.
    pub fn signal_stop(&mut self) {
        self.tasks.abort_all();
        self.reader_slot.with_mut(|slot| slot.signal_stop());
    }

    /// Stop the supervisor and the reader thread, then drop the port. The reader is
    /// joined *before* the port drops so its fd stays valid throughout (fd-reuse
    /// race). Called on teardown/shutdown; idempotent after [`Self::signal_stop`].
    pub fn teardown(&mut self) {
        // Abort the supervisor first so it cannot re-arm a reader after we join.
        // Sync teardown holds the runtime thread, so the aborted task will not run
        // again before we finish; its dropped future releases its port clone after
        // the reader is already joined.
        self.signal_stop();
        stop_join_reader(&self.reader_slot);
        // Give the exclusivity back before letting go of the port (§7.1), in that
        // order — [`release_port`], which is also what `set_waiting`/`fault` call, so
        // the D2 principle is one helper rather than one remembered call site.
        //
        // `sh.port = None` does NOT close the fd here: the aborted supervisor's
        // future still holds an `Rc<ExclusivePort>` clone across its `.await`, and an
        // aborted `spawn_local` task is only dropped once the `LocalSet` gets the
        // runtime thread back — which cannot happen until the synchronous critical
        // section that called us returns (§15.20). `load --replace` composes
        // teardown-then-load inside one such section, so the *replacement* node's
        // `open(2)` lands while this fd is still open, and `TIOCEXCL` made the
        // daemon EBUSY against itself: on real hardware a one-second `faulted`
        // flap during which an accepted `send` was purged rather than written; on a
        // pty-backed device (socat `PTY,link=`, QEMU `-serial pty`, `serial-nexus-sim`) a
        // *permanent* one, because the flag lives on the tty and clears only at its
        // last close, which a held master never reaches.
        //
        // Releasing it is one ioctl and it is the right one: exclusivity is a claim
        // this node made, so this node gives it up when it stops. The unclaimed
        // window is bounded by the same critical section, and whoever opens next
        // re-takes it (`open_port`). A guaranteed outage is the worse trade.
        // Guarded by `p11_replace_atomicity.rs`, including the clause that the port
        // is still exclusive while a live node holds it — and by
        // `p12_serial_exclusivity.rs` for the paths this used to be the only one of.
        //
        // The explicit release also flips the port's `claimed` flag, so the late
        // `Drop` of that lingering clone is a no-op and cannot strip `TIOCEXCL` off
        // the successor that has by then opened the same tty.
        release_port(&self.shared);
    }
}

/// Everything the reconnect supervisor owns for one node's lifetime.
struct SuperviseCtx {
    name: String,
    device: String,
    resolver: Resolver,
    params: OpenParams,
    hostward: SharedFanOut,
    targetward: Option<mpsc::Receiver<Chunk>>,
    /// The tap-feed mirror for this endpoint (§17): the reader mirrors each
    /// hostward chunk here while a tap or ring wants it. `None` only in unit tests.
    tap_feed: Option<TapFeed>,
    shared: Rc<CriticalCell<SerialShared>>,
    reader_slot: Rc<CriticalCell<BlockingReader>>,
    discarded: Arc<AtomicU64>,
    purged: Arc<AtomicU64>,
}

/// One step of the Active loop: drive the targetward writer and watch for loss.
enum Step {
    /// Keep running Active (a chunk was written, or a quiescent wait elapsed).
    Continue,
    /// The device was lost (reader exited, or a write failed) — reconnect.
    Lost,
    /// The targetward channel closed (origins gone / teardown); keep hostward
    /// alive but stop driving the writer.
    WriterClosed,
}

/// The reconnect supervisor (§7.1): adopts the create-opened port, drives the
/// targetward writer, and on device loss polls the resolver until the *same*
/// identity reappears, then runs the reopen ritual.
async fn supervise(mut ctx: SuperviseCtx) {
    // Adopt the port `create` opened, if any: arm a reader on it.
    let mut lost: Option<Arc<Notify>> = None;
    if let Some(port) = ctx.shared.with(|sh| sh.port.clone()) {
        match arm_reader(&port, &ctx) {
            Ok(notify) => lost = Some(notify),
            Err(e) => fault(&ctx, format!("arm reader: {e}")),
        }
    }

    loop {
        match &lost {
            // Active: a live port with an armed reader.
            Some(notify) => {
                let port = match ctx.shared.with(|sh| sh.port.clone()) {
                    Some(p) => p,
                    None => {
                        lost = None;
                        continue;
                    }
                };
                match active_step(&port, notify, &mut ctx.targetward).await {
                    Step::Continue => {}
                    Step::WriterClosed => {
                        // No more targetward; keep the reader running (hostward
                        // survives, §15.24) and wait for loss only.
                        ctx.targetward = None;
                    }
                    Step::Lost => {
                        stop_join_reader(&ctx.reader_slot);
                        set_waiting(&ctx, format!("device {} lost", ctx.device));
                        lost = None;
                    }
                }
            }
            // Waiting/faulted: poll for the device's reappearance.
            None => {
                tokio::time::sleep(RECONNECT_POLL).await;
                if let Some(path) = ctx.resolver.resolve_current_path(&ctx.device) {
                    lost = reopen(&mut ctx, &path).await;
                }
            }
        }
    }
}

/// One reopen attempt on a `waiting`/`faulted` node whose device has reappeared (§7.1
/// reopen ritual). Returns the reader's loss `Notify` once the node is Active again, or
/// `None` to keep polling.
///
/// **The port is published on the node *before* the first `.await`** (§15.38 D2, audit of
/// review 32). The obvious order — open, purge, arm, publish — leaves the node holding
/// `TIOCEXCL` on a device [`release_port`] cannot see, because `sh.port` is still `None`
/// while [`purge_on_reconnect`] yields (up to `DRAIN_ROUNDS` times, whenever there *is*
/// an outage backlog to drain). A `load --replace` landing in that window tears the node
/// down releasing nothing; the aborted supervisor's future still holds the only
/// `Rc<ExclusivePort>` and is not dropped until the `LocalSet` regains the thread — i.e.
/// after the critical section — so the *successor's* `open(2)` on the same device
/// EBUSYs against the daemon's own claim. That is exactly the D2 shape teardown's
/// ordered release exists to prevent, surviving in a second place. Publishing first
/// makes the port reachable by the one ordered discard, and [`ExclusivePort`]'s
/// at-most-once flag keeps the late drop of this function's own clone from undoing it.
///
/// What publishing first costs, and why each cost is acceptable — checked rather than
/// assumed:
///
/// * The node reports `waiting` with `open: true` for the width of the purge. That is
///   *more* truthful than the alternative: the daemon really does hold the device
///   exclusively, and a third party's `open(2)` already says so.
/// * A signal verb is accepted in that window ([`SerialNode::signal_handle`] keys off
///   `sh.port`). The port is real and this node owns it, and because the generation is
///   bumped by [`adopt_port`] rather than by the later transition to Active, a signal
///   issued in the window stays valid into Active instead of being orphaned there.
/// * No half-initialized reader can observe the fd: the reader is armed only below
///   (which is also what sets it non-blocking) and the targetward writer runs only in
///   the supervisor's Active arm. The only fd uses reachable in the window are ioctls
///   (`state_extra`'s counters, a signal verb), which do not care about blocking mode.
/// * If the node let this port go while the purge was awaiting, the generation moved and
///   we do not arm a reader on a port we no longer own. Today an aborted `spawn_local`
///   future is never polled again, so a teardown inside the window means this function
///   simply never resumes — the re-check is the cheap way to keep that an *asserted*
///   property rather than a fact about tokio's scheduler.
async fn reopen(ctx: &mut SuperviseCtx, path: &std::path::Path) -> Option<Arc<Notify>> {
    let port = match open_port(path, &ctx.params) {
        Ok(port) => Rc::new(port),
        // Vanished again between the resolve and the open: stay waiting and retry.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        // Any other error faults but keeps polling.
        Err(e) => {
            fault(ctx, format!("reopen {}: {e}", ctx.device));
            return None;
        }
    };
    adopt_port(ctx, port.clone());
    let generation = ctx.shared.with(|sh| sh.generation);
    purge_on_reconnect(ctx).await;
    if ctx.shared.with(|sh| sh.generation) != generation {
        // A teardown/`--replace` ran inside the purge and already released this port in
        // order. Dropping our clone is a no-op on the assertions (at-most-once).
        return None;
    }
    match arm_reader(&port, ctx) {
        Ok(notify) => {
            set_active(ctx);
            Some(notify)
        }
        Err(e) => {
            fault(ctx, format!("arm reader: {e}"));
            None
        }
    }
}

/// Drive one targetward chunk to the port, or wait for loss (§7.1/§5). Returns as
/// soon as either fires so the supervisor can react promptly.
async fn active_step(
    port: &Rc<ExclusivePort>,
    lost: &Notify,
    targetward: &mut Option<mpsc::Receiver<Chunk>>,
) -> Step {
    match targetward {
        Some(rx) => {
            tokio::select! {
                biased;
                _ = lost.notified() => Step::Lost,
                got = rx.recv() => match got {
                    Some(chunk) => {
                        if runtime::write_all(port.as_raw_fd(), &chunk).await.is_err() {
                            Step::Lost
                        } else {
                            Step::Continue
                        }
                    }
                    None => Step::WriterClosed,
                }
            }
        }
        // No writer: wait only for device loss, at zero CPU.
        None => {
            lost.notified().await;
            Step::Lost
        }
    }
}

/// Purge the outage-era targetward backlog on reconnect (§7.1). The parked
/// receiver's buffered chunks — plus any chunk an origin is suspended mid-`send`
/// behind that full channel (§5 backpressure) — are the daemon-side backlog that
/// accumulated while the node was `waiting`; draining them with a counter is the
/// one sanctioned targetward drop. Post-reconnect commands are kept.
///
/// The bounded drain-then-yield rounds live in
/// [`boundary::drain_to_quiescence`] — the leg's purge-on-reconnect (§7.4) needs
/// the identical semantics, and re-deriving them per node is what §16.1 exists to
/// stop. This runs while the node is still `waiting` (no reader/writer armed), so
/// nothing reaches the device during the drain.
async fn purge_on_reconnect(ctx: &mut SuperviseCtx) {
    if !ctx.params.purge_on_reconnect {
        return;
    }
    let Some(rx) = ctx.targetward.as_mut() else {
        return;
    };
    let purged = boundary::drain_to_quiescence(rx).await;
    if purged > 0 {
        ctx.purged.fetch_add(purged, Ordering::Relaxed);
    }
}

/// Set the port non-blocking and spawn a fresh reader thread, recording its
/// stop flag and handle in the shared slot (any previous reader must already be
/// joined). Returns the `Notify` the reader pulses on device loss.
fn arm_reader(port: &Rc<ExclusivePort>, ctx: &SuperviseCtx) -> std::io::Result<Arc<Notify>> {
    sys::set_nonblocking(port.as_raw_fd())?;
    let fd = port.as_raw_fd();
    let hostward = ctx.hostward.clone();
    let discarded = ctx.discarded.clone();
    let tap_feed = ctx.tap_feed.clone();
    // The boundary-supervisor library owns the stop flag, the loss `Notify`, and
    // the join handle (§16.1 loss-notify + join-then-transition); we supply the
    // node-specific reader body.
    ctx.reader_slot.with_mut(|slot| {
        slot.arm(format!("serial-rx-{}", ctx.name), move |stop, lost| {
            reader_thread(fd, hostward, tap_feed, discarded, stop, lost)
        })?;
        Ok(slot.lost())
    })
}

/// Stop the current reader thread and join it within the poll-timeout bound. On
/// the loss path the thread has already exited, so this returns at once; on
/// teardown of a live device it costs at most one poll interval.
fn stop_join_reader(reader_slot: &Rc<CriticalCell<BlockingReader>>) {
    reader_slot.with_mut(|slot| slot.stop_join());
}

/// Publish a freshly reopened port on the node, before [`reopen`]'s first `.await` —
/// see that function for why the ordering is the fix and not an accident.
///
/// This is where the reconnect's single generation bump happens, so an in-flight signal
/// verb is scoped to the port rather than to the node's status.
fn adopt_port(ctx: &SuperviseCtx, port: Rc<ExclusivePort>) {
    let outgoing = ctx.shared.with_mut(|sh| sh.set_port(Some(port)));
    // A reopen only ever runs out of `waiting`/`faulted`, where the port is already
    // `None`. It must stay that way: an outgoing port released *here* would be released
    // after the incoming one claimed the same tty, and `TIOCEXCL` is tty state — the
    // release would strip the live port's own claim.
    debug_assert!(
        outgoing.is_none(),
        "a reopen replaced a live port; releasing it would strip the new claim"
    );
}

/// Mark the node Active once its reader is armed. The port itself was published earlier,
/// by [`adopt_port`]; this is the status half alone, which is why it takes no port.
fn set_active(ctx: &SuperviseCtx) {
    ctx.shared.with_mut(|sh| sh.status.set(NodeStatus::Active));
}

fn set_waiting(ctx: &SuperviseCtx, reason: String) {
    // Return the exclusivity claim with the port (RV-8/CONC-4): the reconnect poll
    // reopens this same path a second later, and a claim left behind makes the node's
    // own retry EBUSY forever on any tty that outlives the daemon's close.
    release_port(&ctx.shared);
    ctx.shared
        .with_mut(|sh| sh.status.set(NodeStatus::Waiting { reason }));
}

/// Fault the node with `reason`. The reconnect poll re-derives the same reason on
/// every failed retry, and [`NodeState::set`] re-stamps only on a real transition,
/// so the reported age stays the age of the *fault*, not of the last poll (§7).
fn fault(ctx: &SuperviseCtx, reason: String) {
    release_port(&ctx.shared);
    ctx.shared
        .with_mut(|sh| sh.status.set(NodeStatus::Faulted { reason }));
}

/// Hostward reader thread (§15.19): a blocking `poll(2)` waits for readability —
/// waking the instant the device has data (line rate) and parking at zero CPU
/// otherwise — then the loop drains fully and broadcasts each chunk to every
/// attached consumer. On device loss (`POLLHUP`, EOF, or a read error) it pulses
/// `lost` so the supervisor enters faulted-and-wait; a clean stop (teardown)
/// exits without pulsing.
///
/// Loss is always counted at the boundary that drops it: with nothing attached,
/// bytes are discarded against `discarded_unattached` (§5); with a consumer
/// attached but its bounded buffer full, the drop is counted against that
/// consumer's [`DropCounters`] (a slow spy costs only itself).
fn reader_thread(
    fd: std::os::fd::RawFd,
    hostward: SharedFanOut,
    tap_feed: Option<TapFeed>,
    discarded_unattached: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    lost: Arc<Notify>,
) {
    let mut buf = vec![0u8; READ_BUF];
    while !stop.load(Ordering::Relaxed) {
        let re = sys::poll_blocking(
            fd,
            PollFlags::POLLIN | PollFlags::POLLHUP,
            READ_POLL_TIMEOUT_MS,
        );
        if re.contains(PollFlags::POLLIN) {
            loop {
                match sys::read_fd(fd, &mut buf) {
                    Ok(0) => {
                        lost.notify_one(); // device closed
                        return;
                    }
                    Ok(n) => {
                        // The tap/ring mirror is a spy *out of the graph* with its own
                        // accounting; it never counts as a graph consumer for the §5
                        // discard counter, which must report bytes that reached no live
                        // graph consumer regardless of any tap — exactly as the codec,
                        // exec, and leg producers do (audit finding, §5 "loss is always
                        // visible and attributable"; the ring "never substitutes for a
                        // log node"). `tap_wanted` only decides whether to spend a chunk
                        // allocation on the mirror when nothing else needs one.
                        let tap_wanted = tap_feed.as_ref().is_some_and(TapFeed::wanted);
                        if hostward.is_empty() && !tap_wanted {
                            // Nothing attached at all: read-and-discard with a counter.
                            discarded_unattached.fetch_add(n as u64, Ordering::Relaxed);
                            // These bytes are *two* kinds of loss and both must be
                            // counted (TAP-2). They reached no graph consumer — the
                            // §5 counter above — and they are also a hole in this
                            // endpoint's tap offset space, indistinguishable to a
                            // client splicing by offset from the ones an inactive feed
                            // refuses in `TapFeed::mirror`. Skipping the chunk
                            // allocation is the *only* difference between this arm and
                            // that one, so it must not also skip the accounting: with
                            // `replay_ring = 0` this arm is the entire window between
                            // one `tap.close` and the next `tap.open`, and charging
                            // nothing let a returning client concatenate its stored
                            // scrollback against a stream missing an hour, with no
                            // `gap_before`, no counter and no epoch change to say so
                            // (invariant 10).
                            if let Some(feed) = &tap_feed {
                                feed.skipped(n as u64);
                            }
                            continue;
                        }
                        let chunk = Chunk::copy_from_slice(&buf[..n]);
                        // Mirror to the tap hub (lossy, never backpressures the
                        // device — §5); done *before* the fan-out and excluded from
                        // it, so a spy tap never masks a real consumer's absence.
                        if let Some(feed) = &tap_feed {
                            feed.mirror(&chunk);
                        }
                        // The one shared hostward fan-out (§5, F1): it delivers,
                        // charges a slow consumer's full-buffer drop to that
                        // consumer, and charges an all-`Closed`/empty sink set to
                        // `discarded_unattached` — the case a consumer cascade-removed
                        // while this node survives produces, which would otherwise
                        // vanish uncounted.
                        hostward.broadcast(&chunk, &*discarded_unattached);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(_) => {
                        lost.notify_one();
                        return;
                    }
                }
            }
        } else if re.contains(PollFlags::POLLHUP) {
            lost.notify_one(); // device gone
            return;
        }
        // Bare timeout (empty revents): re-check the stop flag and re-arm.
    }
}

impl Drop for SerialNode {
    fn drop(&mut self) {
        // Explicit and in this order: the supervisor must be aborted *before* the
        // reader is joined (so it cannot re-arm one behind us) and the reader joined
        // *before* the port's fd drops (§7.1 fd-reuse). [`TaskSet`]'s own `Drop` would
        // abort at an order decided by field layout, which is not a guarantee this
        // sequence can rest on — so it aborts here and finds nothing left to do.
        self.tasks.abort_all();
        stop_join_reader(&self.reader_slot);
    }
}

// ---------------------------------------------------------------------------
// Serial-signal operations (§7.1) — driven on the runtime thread from the
// control plane, through a [`SignalHandle`] on the node rather than a clone of
// its port. `send-break` and `pulse-dtr` hold a line for a bounded interval; a
// restore guard deasserts even if the dispatch future is cancelled (a dropped
// control connection, §15.20), so a line is never left stuck. Each returns the
// driver error unchanged, so a fd that does not support the operation (a pts)
// surfaces cleanly rather than pretending success.
//
// **Every access is node-scoped** (review 32 CTRL-1/SERX-2). These verbs used to
// hold an `Rc<SerialPort>` across their sleep, which kept the device's fd open —
// with a break or a DTR level asserted on the physical line — past `remove-node`
// and `load --replace`. Reproduced on the bench's FTDI adapter: `pulse-dtr --ms
// 12000` in flight, `load --replace` onto a successor node on the same device,
// and at t=12.1 s DTR flipped on the *successor's* line with nobody asking — a
// board reset attributable to nothing in `state`. The break half is worse and has
// no race in it at all: break is tty state, so the successor came up transmitting
// under an asserted break for the remainder of `ms`.
// ---------------------------------------------------------------------------

/// Why a serial-signal verb could not complete (§7.1).
#[derive(Debug)]
pub(crate) enum SignalError {
    /// The driver refused the operation — e.g. a pts has no `TIOCMSET`. Reported
    /// unchanged, so a hardware boundary reads as a hardware boundary.
    Io(std::io::Error),
    /// The node lost or replaced its port while the signal was in flight: a
    /// `teardown`, `remove-node`, `load --replace`, or an ordinary reconnect. The
    /// verb declines rather than driving a line whose owner has changed.
    NodeGone,
    /// Another line-holding verb already holds this port's one line slot
    /// (37-SER-1). Refused rather than queued: the caller asked for a *bounded*
    /// assertion, and a queued one would be a different duration than the one it
    /// was told succeeded.
    LineBusy,
}

impl std::fmt::Display for SignalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignalError::Io(e) => write!(f, "{e}"),
            SignalError::NodeGone => write!(f, "node was removed while signalling"),
            SignalError::LineBusy => write!(
                f,
                "another line-holding signal verb is already in flight on this port"
            ),
        }
    }
}

/// A control-plane handle on one serial node's live port, valid only while the node
/// still holds *that* port (§7.1). Built by [`SerialNode::signal_handle`].
///
/// It holds no `Rc<ExclusivePort>`, so it cannot keep a torn-down node's fd — or its
/// asserted line state — alive; it re-resolves through the node's shared cell on every
/// access and refuses once the generation has moved.
pub(crate) struct SignalHandle {
    shared: Rc<CriticalCell<SerialShared>>,
    /// The port generation this handle was taken at.
    generation: u64,
    /// Pulsed by every port transition, so a parked hold wakes at once.
    gone: Rc<Notify>,
}

/// **The one predicate a deferred serial signal asks**: is this node still holding
/// the very port the signal was issued against? Both the in-flight verb
/// ([`SignalHandle::with_port`]) and its deferred restore ([`RestoreGuard`]) go
/// through it, because the two answering that question differently is exactly the
/// shape CTRL-1 took — the verb noticed the teardown and the guard drove the line
/// anyway.
fn with_scoped_port<T>(
    shared: &CriticalCell<SerialShared>,
    generation: u64,
    f: impl FnOnce(&ExclusivePort) -> T,
) -> Result<T, SignalError> {
    shared.with(|sh| match sh.port.as_ref() {
        // A generation bump accompanies every transition, including to `None`, so the
        // two conditions are one; both are checked because neither costs anything.
        Some(port) if sh.generation == generation => Ok(f(port)),
        _ => Err(SignalError::NodeGone),
    })
}

impl SignalHandle {
    /// Run `f` against the node's current port, iff it is still the port this handle
    /// was taken on.
    fn with_port<T>(&self, f: impl FnOnce(&ExclusivePort) -> T) -> Result<T, SignalError> {
        with_scoped_port(&self.shared, self.generation, f)
    }

    /// [`Self::with_port`] for an ioctl, flattening the two failure kinds.
    fn try_port(
        &self,
        f: impl FnOnce(&ExclusivePort) -> std::io::Result<()>,
    ) -> Result<(), SignalError> {
        self.with_port(f)?.map_err(SignalError::Io)
    }

    /// Hold an asserted line for `ms`, returning early the moment the node's port
    /// changes underneath us. `ms` is range-checked at the verb (`daemon.rs`'s
    /// `Daemon::signal_ms`, invariant 13) before any line is asserted, so this
    /// interval is bounded by configuration rather than by the caller's imagination.
    ///
    /// The wakeup is race-free by placement: nothing awaits between taking the handle
    /// and this `select!`, and the daemon's data plane is one thread, so no transition
    /// can land in the window where `notified()` is not yet registered. The re-check
    /// on the timeout arm covers a transition that happened *before* the handle's
    /// generation was read — impossible today, cheap insurance if that ever changes.
    async fn hold(&self, ms: u64) -> Result<(), SignalError> {
        tokio::select! {
            biased;
            _ = self.gone.notified() => Err(SignalError::NodeGone),
            _ = tokio::time::sleep(Duration::from_millis(ms)) => self.with_port(|_| ()),
        }
    }

    fn restore_guard(&self, op: RestoreOp) -> RestoreGuard {
        RestoreGuard {
            shared: self.shared.clone(),
            generation: self.generation,
            op,
        }
    }

    /// Claim this port's one line-holding slot for the duration of a `send-break` /
    /// `pulse-dtr` (§7.1, 37-SER-1), or refuse.
    ///
    /// The two verbs' restore guards gate on the port generation alone, which both
    /// halves of an overlap pass — so the shorter verb's guard cleared the line under
    /// the longer one, which went on reporting success for its full duration. A
    /// second control connection is all it took, the one-waiting-verb rule being per
    /// connection (§10). The slot makes them mutually exclusive per port, which is the
    /// scope a physical line has; different nodes hold different slots.
    ///
    /// Taken **before** the line is asserted, so a refusal costs the wire nothing.
    fn claim_line(&self) -> Result<LineLease, SignalError> {
        self.shared.with_mut(|sh| {
            if sh.port.is_none() || sh.generation != self.generation {
                return Err(SignalError::NodeGone);
            }
            // A slot recorded against an older generation belongs to a verb whose port
            // is gone; its guard will decline for the same reason, so it is free here.
            if sh.signal_in_flight == Some(sh.generation) {
                return Err(SignalError::LineBusy);
            }
            sh.signal_in_flight = Some(sh.generation);
            Ok(())
        })?;
        Ok(LineLease {
            shared: self.shared.clone(),
            generation: self.generation,
        })
    }
}

/// Holds a port's line-holding slot (see [`SignalHandle::claim_line`]) and releases
/// it on drop, including cancellation. Declared *before* the verb's [`RestoreGuard`]
/// so it drops *after* it: the slot must not free while the line is still asserted,
/// or the next verb would assert over a restore that has not run.
struct LineLease {
    shared: Rc<CriticalCell<SerialShared>>,
    generation: u64,
}

impl Drop for LineLease {
    fn drop(&mut self) {
        self.shared.with_mut(|sh| {
            // Only our own claim: a port transition inside the hold lets a successor
            // verb take the slot on the new generation, and this drop must not free it.
            if sh.signal_in_flight == Some(self.generation) {
                sh.signal_in_flight = None;
            }
        });
    }
}

/// Restores a break/modem line on drop (including cancellation), so a bounded
/// assertion never leaks past the verb — and **only on the port it was asserted on**.
/// A guard whose node has since torn down, replaced or reconnected its port declines
/// silently: driving that line would be an unrequested edge on a device a successor
/// node owns, which on any auto-reset target is a board reset nobody asked for.
struct RestoreGuard {
    shared: Rc<CriticalCell<SerialShared>>,
    generation: u64,
    op: RestoreOp,
}

enum RestoreOp {
    ClearBreak,
    SetDtr(bool),
}

impl Drop for RestoreGuard {
    fn drop(&mut self) {
        let op = &self.op;
        let _ = with_scoped_port(&self.shared, self.generation, |port| match op {
            RestoreOp::ClearBreak => port.set_break(false),
            RestoreOp::SetDtr(level) => port.set_dtr(*level),
        });
    }
}

/// Assert a serial break for `ms` milliseconds, then clear it (§7.1). The clear
/// runs even on cancellation, and never on a port this node no longer holds. Refused
/// while another line-holding verb has this port's line (37-SER-1).
pub(crate) async fn send_break(handle: &SignalHandle, ms: u64) -> Result<(), SignalError> {
    let _lease = handle.claim_line()?;
    handle.try_port(|p| p.set_break(true))?;
    let _guard = handle.restore_guard(RestoreOp::ClearBreak);
    handle.hold(ms).await
}

/// Pulse DTR: drive it to `assert` for `ms` milliseconds, then to `!assert` — the
/// classic auto-reset toggle (§7.1). The final level is set even on cancellation, and
/// never on a port this node no longer holds. Refused while another line-holding verb
/// has this port's line (37-SER-1) — the two share one slot, because a `pulse-dtr`
/// restore landing under a `send-break` is the same clobber either way round.
pub(crate) async fn pulse_dtr(
    handle: &SignalHandle,
    ms: u64,
    assert: bool,
) -> Result<(), SignalError> {
    let _lease = handle.claim_line()?;
    handle.try_port(|p| p.set_dtr(assert))?;
    let _guard = handle.restore_guard(RestoreOp::SetDtr(!assert));
    handle.hold(ms).await
}

/// Set DTR and/or RTS on the live port (§7.1). A `None` line is left untouched.
/// This acts on the live port only; the configuration's initial modem lines are
/// what a reopen restores (§15.8), so a live change does not survive a replug.
/// Synchronous, so it cannot straddle a teardown — but it goes through the same
/// handle, so there is one way to reach a serial node's lines and not two.
pub(crate) fn set_modem(
    handle: &SignalHandle,
    dtr: Option<bool>,
    rts: Option<bool>,
) -> Result<(), SignalError> {
    handle.try_port(|port| {
        if let Some(dtr) = dtr {
            port.set_dtr(dtr)?;
        }
        if let Some(rts) = rts {
            port.set_rts(rts)?;
        }
        Ok(())
    })
}

/// Read the current modem-line levels for state (§7.1), or `null` where the fd
/// does not support `TIOCMGET` (a pts). DTR/RTS are outputs, the rest inputs.
fn read_modem_lines(fd: std::os::fd::RawFd) -> serde_json::Value {
    match sys::read_modem_bits(fd) {
        Ok(bits) => json!({
            "dtr": bits & libc::TIOCM_DTR != 0,
            "rts": bits & libc::TIOCM_RTS != 0,
            "cts": bits & libc::TIOCM_CTS != 0,
            "dsr": bits & libc::TIOCM_DSR != 0,
            "dcd": bits & libc::TIOCM_CAR != 0,
            "ri": bits & libc::TIOCM_RI != 0,
        }),
        Err(_) => serde_json::Value::Null,
    }
}

/// Open `device` and bring it to the configured line settings (§7.1).
///
/// Everything after the `TIOCEXCL` take runs against an [`ExclusivePort`], so each
/// `?` below returns the claim on its way out. That is not decoration: `set_dtr` on a
/// pty-backed device (a *documented* §7.1/§12 `raw:` configuration — socat, QEMU,
/// `serial-nexus-sim`) `ENOTTY`s, and the port used to be dropped still exclusive, which on
/// that device class is permanent (review 32 RV-8). Guard:
/// `p12_serial_exclusivity.rs`.
fn open_port(device: &std::path::Path, params: &OpenParams) -> std::io::Result<ExclusivePort> {
    let port = SerialPort::open(device, |mut s: Settings| {
        s.set_raw();
        s.set_baud_rate(params.baud)?;
        s.set_char_size(char_size(params.data_bits));
        s.set_parity(map_parity(params.parity));
        s.set_stop_bits(map_stop(params.stop_bits));
        s.set_flow_control(map_flow(params.flow));
        Ok(s)
    })?;
    // serial2 does not take TIOCEXCL; the daemon does, so stray processes cannot
    // share the port (§7.1, P3 finding). Re-taken on every reopen.
    sys::set_exclusive(port.as_raw_fd(), true)
        .map_err(|e| std::io::Error::other(format!("TIOCEXCL: {e}")))?;
    // The claim now has an owner; from here the `?`s below cannot leak it.
    let port = ExclusivePort::new(port);
    // Modem-line assertions (§7.1): a line left unset (None) keeps the driver's
    // power-on state; an asserted/deasserted line is made deterministic here and
    // reapplied by the reopen ritual against auto-reset adapters.
    if let Some(dtr) = params.modem.dtr {
        port.set_dtr(dtr)
            .map_err(|e| std::io::Error::other(format!("set DTR: {e}")))?;
    }
    if let Some(rts) = params.modem.rts {
        port.set_rts(rts)
            .map_err(|e| std::io::Error::other(format!("set RTS: {e}")))?;
    }
    Ok(port)
}

fn char_size(d: DataBits) -> CharSize {
    match d {
        DataBits::Five => CharSize::Bits5,
        DataBits::Six => CharSize::Bits6,
        DataBits::Seven => CharSize::Bits7,
        DataBits::Eight => CharSize::Bits8,
    }
}

fn map_parity(p: CfgParity) -> Parity {
    match p {
        CfgParity::None => Parity::None,
        CfgParity::Odd => Parity::Odd,
        CfgParity::Even => Parity::Even,
    }
}

fn map_stop(s: CfgStop) -> StopBits {
    match s {
        CfgStop::One => StopBits::One,
        CfgStop::Two => StopBits::Two,
    }
}

fn map_flow(f: CfgFlow) -> FlowControl {
    match f {
        CfgFlow::None => FlowControl::None,
        CfgFlow::XonXoff => FlowControl::XonXoff,
        CfgFlow::RtsCts => FlowControl::RtsCts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A supervisor context whose only meaningful fields for `purge_on_reconnect`
    /// are `params`, `targetward`, and `purged`; the rest are inert placeholders.
    fn test_ctx(targetward: Option<mpsc::Receiver<Chunk>>) -> SuperviseCtx {
        SuperviseCtx {
            name: "test".into(),
            device: "raw:/dev/null".into(),
            resolver: Resolver::new("/"),
            params: OpenParams {
                baud: 115_200,
                data_bits: DataBits::Eight,
                parity: CfgParity::None,
                stop_bits: CfgStop::One,
                flow: CfgFlow::None,
                modem: ModemLines {
                    dtr: None,
                    rts: None,
                },
                purge_on_reconnect: true,
            },
            hostward: crate::runtime::FanOutList::new(),
            targetward,
            tap_feed: None,
            shared: Rc::new(CriticalCell::new(SerialShared {
                status: NodeState::new(NodeStatus::Waiting {
                    reason: "test".into(),
                }),
                port: None,
                generation: 0,
                signal_gone: Rc::new(Notify::new()),
                signal_in_flight: None,
            })),
            reader_slot: Rc::new(CriticalCell::new(BlockingReader::default())),
            discarded: Arc::new(AtomicU64::new(0)),
            purged: Arc::new(AtomicU64::new(0)),
        }
    }

    /// XC-PURGE-1 (wiring guard): the drain-to-quiescence *semantics* are tested
    /// once, on the shared helper (`boundary::drain_to_quiescence`); what stays here
    /// is that this node routes them correctly — the backlog is drained and the
    /// count lands on `purged_on_reconnect` (§7.1).
    #[test]
    fn purge_on_reconnect_drains_the_backlog_and_counts_it() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async {
            let (tx, rx) = mpsc::channel::<Chunk>(4);
            tx.send(Chunk::copy_from_slice(b"AAA")).await.unwrap(); // 3 bytes
            tx.send(Chunk::copy_from_slice(b"BBBB")).await.unwrap(); // 4 bytes

            let mut ctx = test_ctx(Some(rx));
            purge_on_reconnect(&mut ctx).await;

            assert_eq!(ctx.purged.load(Ordering::Relaxed), 3 + 4);
            let mut rx = ctx.targetward.take().unwrap();
            assert!(matches!(
                rx.try_recv(),
                Err(tokio::sync::mpsc::error::TryRecvError::Empty)
            ));
            drop(tx);
        });
    }

    /// `purge_on_reconnect = false` (§7.1) keeps the outage-era backlog: an operator
    /// who wants their queued commands delivered after a replug must get them, and
    /// nothing may be counted as purged.
    #[test]
    fn purge_on_reconnect_disabled_keeps_the_backlog() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async {
            let (tx, rx) = mpsc::channel::<Chunk>(4);
            tx.send(Chunk::copy_from_slice(b"AAA")).await.unwrap();

            let mut ctx = test_ctx(Some(rx));
            ctx.params.purge_on_reconnect = false;
            purge_on_reconnect(&mut ctx).await;

            assert_eq!(ctx.purged.load(Ordering::Relaxed), 0);
            let mut rx = ctx.targetward.take().unwrap();
            assert!(matches!(rx.try_recv(), Ok(c) if c.len() == 3));
            drop(tx);
        });
    }

    /// SERIAL-3: when every hostward sink is permanently Closed (a consumer
    /// cascade-removed while this node lives), the bytes must be counted as
    /// unattached loss (§5), not dropped silently.
    #[test]
    fn all_sinks_closed_counts_discarded_on_surviving_producer() {
        use std::os::fd::AsRawFd;
        let (read_fd, write_fd) = nix::unistd::pipe().unwrap();
        let (tx, rx) = mpsc::channel::<Chunk>(4);
        drop(rx); // consumer gone: the sink is now permanently Closed.
        let counters = Arc::new(crate::runtime::DropCounters::default());
        let hostward = crate::runtime::FanOutList::new();
        hostward.attach(crate::runtime::AttachedSink {
            target: "consumer".parse().expect("address parses"),
            tx,
            counters: counters.clone(),
        });
        let discarded = Arc::new(AtomicU64::new(0));

        // Feed 5 bytes, then close the write end: the reader drains them, hits EOF,
        // and returns — deterministic, no polling race.
        nix::unistd::write(&write_fd, b"hello").unwrap();
        drop(write_fd);

        let rfd = read_fd.as_raw_fd();
        let discarded_thread = discarded.clone();
        std::thread::spawn(move || {
            reader_thread(
                rfd,
                hostward,
                None,
                discarded_thread,
                Arc::new(AtomicBool::new(false)),
                Arc::new(Notify::new()),
            );
        })
        .join()
        .unwrap();
        drop(read_fd); // the fd stays valid until the reader thread has joined.

        // The 5 bytes reached no live boundary → counted as unattached loss, and
        // the dead consumer is not mis-charged a full-buffer drop.
        assert_eq!(discarded.load(Ordering::Relaxed), 5);
        assert_eq!(counters.dropped_full(), 0);
    }

    /// SERIAL-4 (audit regression): a configured replay ring / open tap makes the
    /// tap feed `active`, but it is a spy *out of the graph* — it must NOT suppress
    /// `discarded_unattached` when no graph consumer is present (§5 "loss is always
    /// visible", the ring "never substitutes for a log node"). With an active feed
    /// and no hostward sink, the bytes are mirrored to the tap AND counted as loss.
    #[test]
    fn active_tap_feed_does_not_hide_unattached_loss() {
        use std::os::fd::AsRawFd;
        let (read_fd, write_fd) = nix::unistd::pipe().unwrap();
        let discarded = Arc::new(AtomicU64::new(0));
        // An active tap feed (as a ring or a live tap would make it), with a receiver
        // kept alive so the mirror's try_send succeeds.
        let (feed_tx, _feed_rx) = mpsc::channel::<Chunk>(8);
        let feed = TapFeed {
            tx: feed_tx,
            active: Arc::new(AtomicBool::new(true)),
            feed_dropped: Arc::new(AtomicU64::new(0)),
        };

        nix::unistd::write(&write_fd, b"hello").unwrap();
        drop(write_fd);

        let rfd = read_fd.as_raw_fd();
        let discarded_thread = discarded.clone();
        std::thread::spawn(move || {
            reader_thread(
                rfd,
                crate::runtime::FanOutList::new(), // no graph consumer
                Some(feed),                        // but a ring/tap is watching
                discarded_thread,
                Arc::new(AtomicBool::new(false)),
                Arc::new(Notify::new()),
            );
        })
        .join()
        .unwrap();
        drop(read_fd);

        // No graph consumer took the bytes → counted, even though a tap mirrored them.
        assert_eq!(discarded.load(Ordering::Relaxed), 5);
    }

    // -----------------------------------------------------------------------
    // Exclusivity ownership + node-scoped signals (review 32 RV-8, CONC-4/SERX-1,
    // CTRL-1/SERX-2). These drive a real pts, so they are Linux-only: a pts cannot
    // be a serial device on macOS (`serial2` → `ENOTTY`, AGENTS §7), which would
    // make every arm below vacuous rather than green. The end-to-end guards live in
    // `itest/tests/p12_serial_exclusivity.rs`.
    // -----------------------------------------------------------------------

    /// A master + the path of its slave. The master is held for the fixture's life:
    /// that is precisely the device class the leak is *permanent* on, because
    /// `TIOCEXCL` clears at the tty's last close and a held master never reaches it.
    #[cfg(target_os = "linux")]
    struct PtsFixture {
        _master: nix::pty::PtyMaster,
        path: std::path::PathBuf,
    }

    #[cfg(target_os = "linux")]
    fn pts_fixture() -> PtsFixture {
        use nix::fcntl::OFlag;
        use nix::pty::{grantpt, posix_openpt, unlockpt};
        let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY).expect("posix_openpt");
        grantpt(&master).expect("grantpt");
        unlockpt(&master).expect("unlockpt");
        let path = sys::ptsname(&master).expect("ptsname");
        PtsFixture {
            _master: master,
            path: std::path::PathBuf::from(path),
        }
    }

    /// A third party's plain `open(2)` of the pts: `Ok` when nobody holds `TIOCEXCL`
    /// on the tty, `Err(EBUSY)` when somebody does. This is what every unprivileged
    /// process on the machine — the daemon's own reconnect included — sees.
    #[cfg(target_os = "linux")]
    fn third_party_open(path: &std::path::Path) -> nix::Result<std::os::fd::OwnedFd> {
        use nix::fcntl::{OFlag, open};
        use nix::sys::stat::Mode;
        open(path, OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty())
    }

    #[cfg(target_os = "linux")]
    fn open_params(modem: ModemLines) -> OpenParams {
        OpenParams {
            baud: 115_200,
            data_bits: DataBits::Eight,
            parity: CfgParity::None,
            stop_bits: CfgStop::One,
            flow: CfgFlow::None,
            modem,
            purge_on_reconnect: true,
        }
    }

    /// RV-8: `open_port` takes `TIOCEXCL` and *then* applies the configured modem
    /// lines. A pts has no `TIOCMSET`, so an ordinary documented configuration —
    /// `[node.modem] dtr = true` on a `raw:` pty-backed device (socat, QEMU,
    /// `serial-nexus-sim`) — fails after the claim is taken. The port must not be discarded
    /// still exclusive: that bricks the device for every unprivileged process on the
    /// machine, permanently, and masks the true cause behind a self-inflicted EBUSY.
    #[cfg(target_os = "linux")]
    #[test]
    fn open_port_returns_the_exclusivity_claim_when_a_modem_line_fails() {
        let pts = pts_fixture();
        let err = open_port(
            &pts.path,
            &open_params(ModemLines {
                dtr: Some(true),
                rts: None,
            }),
        )
        .err()
        .expect("set_dtr must ENOTTY on a pts");
        assert!(
            err.to_string().contains("set DTR"),
            "the reported cause is not the modem-line failure: {err}"
        );
        // The claim went back with the port: the device is still usable by anyone.
        assert!(
            third_party_open(&pts.path).is_ok(),
            "a failed open left TIOCEXCL on the tty — the device is bricked"
        );
    }

    /// The detector the test above depends on, proven rather than assumed: while a
    /// port really does hold the claim, a third party's open is refused. Without this
    /// arm, an `open_port` that silently stopped taking `TIOCEXCL` would make the
    /// RV-8 guard pass for the wrong reason.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_live_port_holds_the_claim_and_drops_it_with_its_last_clone() {
        let pts = pts_fixture();
        let port = Rc::new(
            open_port(&pts.path, &open_params(ModemLines::default())).expect("open the pts"),
        );
        assert!(
            third_party_open(&pts.path).is_err(),
            "a live port did not take TIOCEXCL (the guards below would be vacuous)"
        );
        let clone = port.clone();
        drop(port);
        assert!(
            third_party_open(&pts.path).is_err(),
            "the claim was returned while a clone was still live"
        );
        drop(clone);
        assert!(
            third_party_open(&pts.path).is_ok(),
            "dropping the last clone left TIOCEXCL on the tty"
        );
    }

    /// CONC-4/SERX-1: `set_waiting` and `fault` both discard the port through
    /// [`release_port`], which returns the claim *before* letting go. Without it the
    /// node's own 1 s reconnect reopens the path it just poisoned and EBUSYs forever,
    /// reporting a squatter that does not exist.
    #[cfg(target_os = "linux")]
    #[test]
    fn release_port_returns_the_claim_before_the_reconnect_reopens() {
        let pts = pts_fixture();
        let port = open_port(&pts.path, &open_params(ModemLines::default())).expect("open the pts");
        let shared = CriticalCell::new(SerialShared {
            status: NodeState::new(NodeStatus::Active),
            port: Some(Rc::new(port)),
            generation: 0,
            signal_gone: Rc::new(Notify::new()),
            signal_in_flight: None,
        });

        release_port(&shared);

        assert!(shared.with(|sh| sh.port.is_none()), "port not cleared");
        assert!(
            third_party_open(&pts.path).is_ok(),
            "set_waiting/fault left TIOCEXCL behind: the reconnect will EBUSY forever"
        );
        // The very open the supervisor's reconnect poll would make, one second later.
        assert!(
            open_port(&pts.path, &open_params(ModemLines::default())).is_ok(),
            "the node cannot reopen its own device"
        );
    }

    /// The `Drop` backstop must not undo a *deliberate* release. `TIOCEXCL` is tty
    /// state, not fd state, so a late drop of a torn-down node's lingering clone
    /// would otherwise strip the claim off the `--replace` successor that has since
    /// opened the same tty — turning one fix into a different exclusivity hole.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_deliberate_release_is_not_undone_by_a_later_drop() {
        let pts = pts_fixture();
        let outgoing =
            open_port(&pts.path, &open_params(ModemLines::default())).expect("open the pts");
        // Teardown's ordered release, then the successor node opens the same tty.
        outgoing.release_claims();
        let successor =
            open_port(&pts.path, &open_params(ModemLines::default())).expect("successor open");
        // The outgoing node's aborted future finally drops its clone.
        drop(outgoing);
        assert!(
            third_party_open(&pts.path).is_err(),
            "a late drop stripped TIOCEXCL off the successor's port"
        );
        drop(successor);
    }

    /// CTRL-1/SERX-2: a signal issued against one port must not act on another. Both
    /// the in-flight verb and its deferred restore ask [`with_scoped_port`], so the
    /// generation captured at assert time is the whole answer — proven here with a
    /// *successor port installed on the very same tty*, which is the reproduced shape
    /// (`send-break`/`pulse-dtr` in flight, `load --replace` onto the same device).
    ///
    /// The same tty is the point, and this test did not have it: an earlier version
    /// bound the successor to a *second* pts, which removes the shared-tty condition
    /// that is the entire hazard — the outgoing node's restore would have been harmless
    /// there whatever it did. Reusing the fixture also exercises the ordering that makes
    /// it possible at all: `release_port` must have returned `TIOCEXCL` before
    /// `open_port` can bind the successor, so a regression there fails here too.
    ///
    /// What this **cannot** show, stated rather than implied (§15.36): a pts has no
    /// `break_ctl`, so `TIOCSBRK`/`TIOCCBRK` succeed and change nothing observable. The
    /// line-state half of SERX-2 — that a straddled break is cleared before the port is
    /// handed on — is asserted on real UARTs by
    /// `p12_serial_exclusivity.rs::a_break_straddled_by_a_replace_leaves_the_line_transmitting`,
    /// which self-skips without a crossover rig.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_port_transition_scopes_out_an_in_flight_signal_and_its_restore() {
        let pts = pts_fixture();
        let shared = Rc::new(CriticalCell::new(SerialShared {
            status: NodeState::new(NodeStatus::Active),
            port: Some(Rc::new(
                open_port(&pts.path, &open_params(ModemLines::default())).expect("open first"),
            )),
            generation: 0,
            signal_gone: Rc::new(Notify::new()),
            signal_in_flight: None,
        }));
        let generation = shared.with(|sh| sh.generation);

        assert!(
            with_scoped_port(&shared, generation, |_| ()).is_ok(),
            "the signal's own port must be reachable while the node holds it"
        );
        // A signal verb asserts its line here. On a pts this is a kernel no-op; on the
        // UART this models it latches on the *tty*, which is why the discard below has
        // to clear it rather than the departed guard.
        with_scoped_port(&shared, generation, |p| p.set_break(true))
            .expect("assert the break on the node's own port")
            .expect("TIOCSBRK");

        // teardown-then-load, as `load --replace` composes it: the outgoing port goes
        // (returning its assertions in order), and the successor binds *the same tty*.
        release_port(&shared);
        let successor = Rc::new(
            open_port(&pts.path, &open_params(ModemLines::default()))
                .expect("the successor cannot open the tty the outgoing node just left"),
        );
        shared.with_mut(|sh| sh.set_port(Some(successor)));

        assert!(
            shared.with(|sh| sh.port.is_some()),
            "the successor is bound (otherwise this proves nothing about scoping)"
        );
        assert!(
            matches!(
                with_scoped_port(&shared, generation, |_| ()),
                Err(SignalError::NodeGone)
            ),
            "an in-flight signal reached a port its node no longer holds"
        );
        // The deferred restore asks the same predicate, so it declines too — the clause
        // that made clearing the break the *discard's* job rather than the guard's.
        drop(RestoreGuard {
            shared: shared.clone(),
            generation,
            op: RestoreOp::ClearBreak,
        });
        // And that is what the operator is told, rather than a driver error.
        assert_eq!(
            SignalError::NodeGone.to_string(),
            "node was removed while signalling"
        );
        release_port(&shared);
    }

    // --- the ordered discard, driven through the paths that reach it ---------------

    /// A `SuperviseCtx` holding a live port on `pts`, plus a second strong clone of that
    /// port. The clone is the whole apparatus: it stands in for the one the supervisor's
    /// in-flight future holds, so [`ExclusivePort`]'s `Drop` backstop *cannot* fire and
    /// only a **deliberate** release can pass the assertions below.
    #[cfg(target_os = "linux")]
    fn ctx_with_live_port(pts: &PtsFixture) -> (SuperviseCtx, Rc<ExclusivePort>) {
        let port = Rc::new(
            open_port(&pts.path, &open_params(ModemLines::default())).expect("open the pts"),
        );
        let ctx = test_ctx(None);
        ctx.shared.with_mut(|sh| {
            sh.status.set(NodeStatus::Active);
            sh.set_port(Some(port.clone()))
        });
        (ctx, port)
    }

    /// CONC-4/SERX-1, at the call site rather than one level below it: [`set_waiting`]
    /// is reached on every device loss, and the node's own reconnect poll reopens the
    /// same path one second later. If it lets the port go without returning the claim,
    /// that retry EBUSYs against the daemon itself — forever on any tty whose master
    /// somebody holds.
    ///
    /// `release_port`'s own unit test covers the helper; this covers the *wiring*, which
    /// is what actually rotted. Deleting `release_port(&ctx.shared)` from `set_waiting`
    /// used to break no test in the tree, because `Drop` released the claim anyway — at
    /// an order relative to a successor's open that nothing constrains. The lingering
    /// clone removes `Drop` from the picture, so this fails on that deletion.
    #[cfg(target_os = "linux")]
    #[test]
    fn set_waiting_returns_the_claim_while_a_port_clone_is_still_live() {
        let pts = pts_fixture();
        let (ctx, lingering) = ctx_with_live_port(&pts);
        assert!(
            third_party_open(&pts.path).is_err(),
            "detector proof: the fixture port does not hold TIOCEXCL, so this test \
             would pass vacuously"
        );

        set_waiting(&ctx, "device lost".into());

        assert!(
            ctx.shared.with(|sh| sh.port.is_none()),
            "set_waiting kept the port"
        );
        assert!(
            matches!(
                ctx.shared.with(|sh| sh.status.status().clone()),
                NodeStatus::Waiting { .. }
            ),
            "set_waiting did not report waiting"
        );
        assert!(
            third_party_open(&pts.path).is_ok(),
            "set_waiting let the port go without returning the claim: the node's own \
             reconnect poll will EBUSY against it a second from now"
        );
        // The fd is still open — only the claim went back, which is the whole point.
        drop(lingering);
    }

    /// The same for [`fault`], the other supervisor path that discards a live port (a
    /// failed `arm_reader` on a port `open_port` had just claimed).
    #[cfg(target_os = "linux")]
    #[test]
    fn fault_returns_the_claim_while_a_port_clone_is_still_live() {
        let pts = pts_fixture();
        let (ctx, lingering) = ctx_with_live_port(&pts);
        assert!(
            third_party_open(&pts.path).is_err(),
            "detector proof: the fixture port does not hold TIOCEXCL"
        );

        fault(&ctx, "arm reader: boom".into());

        assert!(
            ctx.shared.with(|sh| sh.port.is_none()),
            "fault kept the port"
        );
        assert!(
            matches!(
                ctx.shared.with(|sh| sh.status.status().clone()),
                NodeStatus::Faulted { .. }
            ),
            "fault did not report faulted"
        );
        assert!(
            third_party_open(&pts.path).is_ok(),
            "fault let the port go without returning the claim"
        );
        drop(lingering);
    }

    /// The reconnect window (audit of review 32): [`reopen`] must publish its port on the
    /// node **before** the purge's first `.await`, so a `load --replace` landing inside
    /// that window finds a port to release in order instead of EBUSYing the daemon
    /// against its own claim.
    ///
    /// The interleave is deterministic, not raced. `purge_on_reconnect` yields exactly
    /// once for the single seeded backlog chunk (`boundary::drain_to_quiescence`: drain,
    /// yield, drain nothing, stop), and that yield is `reopen`'s first await — so the
    /// probe task's first poll lands inside the window by construction. It then does
    /// precisely what `SerialNode::teardown` does, `release_port`, and asks the question
    /// every third party asks.
    ///
    /// Fail-first: with the port published after `arm_reader` instead, `release_port`
    /// finds `sh.port == None`, releases nothing, and this function's own live clone
    /// keeps `TIOCEXCL` on the tty — the probe's open is refused and the reopen goes on
    /// to arm a reader for a node that no longer exists.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_reopen_publishes_its_port_before_the_purge_so_a_teardown_can_release_it() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async {
            let pts = pts_fixture();
            let (tx, rx) = mpsc::channel::<Chunk>(4);
            // An outage-era backlog, so the purge really drains and really yields.
            tx.send(Chunk::copy_from_slice(b"outage-era"))
                .await
                .unwrap();
            let mut ctx = test_ctx(Some(rx));
            let shared = ctx.shared.clone();
            let path = pts.path.clone();
            let (probe_tx, probe_rx) = std::sync::mpsc::channel::<bool>();

            tokio::task::spawn_local(async move {
                // First poll = inside `purge_on_reconnect`'s drain-and-yield.
                release_port(&shared);
                let _ = probe_tx.send(third_party_open(&path).is_ok());
            });

            let armed = reopen(&mut ctx, &pts.path).await;

            assert_eq!(
                probe_rx.try_recv(),
                Ok(true),
                "a teardown inside the reconnect purge could not reach the port: the \
                 daemon holds TIOCEXCL on a device `release_port` cannot see, so a \
                 `--replace` successor EBUSYs against the daemon's own claim"
            );
            assert!(
                armed.is_none(),
                "the reopen armed a reader on a port its node had already released"
            );
            assert!(
                ctx.shared.with(|sh| sh.port.is_none()),
                "the released port came back"
            );
            assert_eq!(
                ctx.purged.load(Ordering::Relaxed),
                10,
                "the backlog was not purged, so the window this test needs never opened"
            );
            drop(tx);
        });
    }

    /// CTRL-1: the hold must not sleep out a duration whose node is already gone —
    /// the fd and the asserted line have to go with the node, not with the caller's
    /// `ms`. An hour-long hold returns the moment the port transitions.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_hold_wakes_and_declines_the_moment_the_node_drops_its_port() {
        let pts = pts_fixture();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async {
            let shared = Rc::new(CriticalCell::new(SerialShared {
                status: NodeState::new(NodeStatus::Active),
                port: Some(Rc::new(
                    open_port(&pts.path, &open_params(ModemLines::default()))
                        .expect("open the pts"),
                )),
                generation: 0,
                signal_gone: Rc::new(Notify::new()),
                signal_in_flight: None,
            }));
            let handle = SignalHandle {
                shared: shared.clone(),
                generation: shared.with(|sh| sh.generation),
                gone: shared.with(|sh| sh.signal_gone.clone()),
            };

            let tearing_down = shared.clone();
            tokio::task::spawn_local(async move {
                tokio::task::yield_now().await;
                release_port(&tearing_down);
            });

            // An hour, deliberately: the point is that it does not elapse. The outer
            // timeout is the test's own safety net, not the mechanism under test.
            let held = tokio::time::timeout(
                Duration::from_secs(5),
                handle.hold(serial_nexus_core::config::MAX_TIMER_MS),
            )
            .await
            .expect("the hold did not wake on the port transition");
            assert!(
                matches!(held, Err(SignalError::NodeGone)),
                "a hold survived its node: {held:?}"
            );
        });
    }

    /// 37-SER-1: a port's line-holding slot admits one verb, and it is scoped to the
    /// *port* — not to the node and not to the process.
    ///
    /// The overlap itself (two control connections, the shorter break clearing the
    /// longer one's line) is proved end to end in `itest/tests/p7_signals.rs`. What
    /// needs a unit is the half no RPC sequence can stage deterministically: a claim
    /// left behind by a verb whose port has since gone must read as *free* for the
    /// successor port — otherwise a reconnect during a long `send-break` would refuse
    /// every later signal for the life of the node — and the departing verb's own
    /// release must not then free the successor's claim.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_line_holding_slot_admits_one_verb_and_is_scoped_to_the_port() {
        let pts = pts_fixture();
        let shared = Rc::new(CriticalCell::new(SerialShared {
            status: NodeState::new(NodeStatus::Active),
            port: Some(Rc::new(
                open_port(&pts.path, &open_params(ModemLines::default())).expect("open the pts"),
            )),
            generation: 0,
            signal_gone: Rc::new(Notify::new()),
            signal_in_flight: None,
        }));
        let handle_for = |shared: &Rc<CriticalCell<SerialShared>>| SignalHandle {
            shared: shared.clone(),
            generation: shared.with(|sh| sh.generation),
            gone: shared.with(|sh| sh.signal_gone.clone()),
        };

        let first = handle_for(&shared);
        let lease = first.claim_line().expect("the first claim on a free port");
        assert!(
            matches!(first.claim_line(), Err(SignalError::LineBusy)),
            "a second line-holding verb took a port whose line is already held"
        );
        drop(lease);
        drop(
            first
                .claim_line()
                .expect("the slot is free again once the holder's lease drops"),
        );

        // A reconnect under a still-running verb: the same device, a new port
        // identity. `set_port` is the one place a generation moves (§7.1).
        let stale = first.claim_line().expect("claim on the pre-reconnect port");
        let outgoing = shared.with_mut(|sh| sh.set_port(sh.port.clone()));
        drop(outgoing);
        let successor = handle_for(&shared);
        let fresh = successor
            .claim_line()
            .expect("the successor port's slot is free: the stale claim is not its own");
        drop(stale);
        assert!(
            matches!(successor.claim_line(), Err(SignalError::LineBusy)),
            "the departing verb's release freed a claim it never made"
        );
        drop(fresh);
    }
}
