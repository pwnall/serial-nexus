//! `serial-nexus-devprep` on Darwin — the macOS half of design §15.45.
//!
//! # What is the same, and what cannot be
//!
//! The Linux helper writes `0` then `1` to a USB device's sysfs `authorized`
//! attribute. Here the mechanism is IOUSBLib's `USBDeviceReEnumerate`, wrapped in
//! `serial_nexus_sys::usb_macos` because §16.3 keeps every `unsafe` in that one
//! crate. Three consequences shape this file, and each of them is a measurement
//! rather than a preference (notes §3.66):
//!
//! * **A device is named by its USB serial number, not by a port name.** Linux's
//!   `--port 3-1` is a sysfs bus path; macOS has no such thing, and the stable
//!   identity IOKit offers is `USB Serial Number`. So `--port BH00L4KU` here. The
//!   flag keeps its name so a caller's argument list does not fork per platform,
//!   and `status` prints the identity it actually matched.
//! * **Nothing is privileged.** `USBDeviceOpen` succeeds at an ordinary `euid`, so
//!   `capabilities`, `install` and `preflight` have nothing to install or bless.
//!   They stay, because a caller that shells out to them must get a clean answer
//!   rather than an unknown verb, and they report *ready* with the reason.
//! * **The outage is atomic and is not a parameter.** `--hold-ms` cannot be
//!   honoured and [`Verb::Hold`] cannot be implemented. Rather than silently
//!   ignoring a duration the caller asked for, `cycle` **measures what it actually
//!   produced** and reports `hold_ms_honoured: false` beside it, and `hold`
//!   refuses outright. A helper that quietly turned a 1500 ms hold into 41 ms
//!   would hand a test a window it never had.
//!
//! # What `cycle` gives a caller that the raw call does not
//!
//! `USBDeviceReEnumerate` returns as soon as the request is accepted, which is
//! before the device is gone and long before it is back. This verb waits for the
//! callout node to disappear and return, and reports the **measured** outage plus
//! the `sessionID` either side — Darwin's counterpart to Linux's `devnum`, and the
//! field that separates a real re-enumeration from a driver rebind.

use std::path::Path;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use serial_nexus_sys::usb_macos;

/// Longest this verb waits for the node to come back before calling it a failure.
/// The measured outage is ~41 ms; a second is two orders of magnitude of slack and
/// still fails fast enough to be a test's problem rather than a hang.
const RETURN_TIMEOUT: Duration = Duration::from_secs(1);
/// How often the wait samples the device node.
const SAMPLE: Duration = Duration::from_millis(2);

#[derive(Parser)]
#[command(
    name = "serial-nexus-devprep",
    version,
    about = "serial_nexus USB re-enumeration helper (macOS: IOUSBLib, §15.45)"
)]
struct Cli {
    #[command(subcommand)]
    verb: Verb,
}

#[derive(Subcommand)]
enum Verb {
    /// Report whether this platform needs a capability to replug. It does not.
    Capabilities {
        #[arg(long)]
        json: bool,
    },
    /// Report an adapter's identity and its re-enumeration witness. Reads only.
    Status {
        /// USB **serial number**, e.g. `BH00L4KU` — not a path, and not a Linux
        /// bus port name.
        #[arg(long)]
        port: String,
        #[arg(long)]
        json: bool,
    },
    /// Re-enumerate one or more adapters — the replug.
    Cycle {
        /// USB serial number, repeatable.
        #[arg(long = "port", required = true)]
        ports: Vec<String>,
        /// Accepted for argument-compatibility with the Linux helper and **not
        /// honourable here**: the outage is the kernel's and measured at ~41 ms.
        /// Reported back as `hold_ms_requested` beside `hold_ms_honoured: false`
        /// and the outage actually observed.
        #[arg(long, default_value_t = 1500)]
        hold_ms: u64,
        /// Run every check and every wait, but do not re-enumerate. The positive
        /// control: a test body that passes under `--dry-run` is not measuring the
        /// replug.
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    /// Not implementable on this platform. Refused rather than approximated.
    Hold {
        #[arg(long = "port", required = true)]
        ports: Vec<String>,
        #[arg(long, default_value_t = 30_000)]
        max_ms: u64,
        #[arg(long)]
        dry_run: bool,
    },
    /// There is no deauthorized state here, so there is nothing to repair.
    Authorize {
        #[arg(long)]
        port: String,
        #[arg(long)]
        json: bool,
    },
    /// Nothing to install: the mechanism is unprivileged.
    Install {
        #[arg(long, default_value = "debug")]
        profile: String,
        #[arg(long)]
        verify: bool,
    },
    /// Is the replug capability usable here? On macOS: yes, with no blessing.
    Preflight {
        #[arg(long, default_value = "debug")]
        profile: String,
        #[arg(long)]
        json: bool,
    },
}

/// The same exit vocabulary the Linux arm uses, so a caller reads one table.
mod exit {
    pub const READY: i32 = 0;
    pub const NOT_READY: i32 = 1;
    pub const REFUSED: i32 = 3;
    pub const FAILED: i32 = 70;
}

fn refuse(message: &str, json: bool) -> i32 {
    if json {
        println!("{}", serde_json::json!({ "error": message }));
    } else {
        eprintln!("serial-nexus-devprep: {message}");
    }
    exit::REFUSED
}

fn facts(a: &usb_macos::UsbAdapter) -> serde_json::Value {
    serde_json::json!({
        "port": a.serial,
        "identified_by": "usb-serial-number",
        "serial": a.serial,
        "id_vendor": format!("{:04x}", a.vendor_id),
        "id_product": format!("{:04x}", a.product_id),
        "product": a.product,
        // Darwin's `devnum`: reissued on every enumeration, so a change separates a
        // real replug from a driver rebind. Compared for inequality, never order.
        "session_id": a.session_id,
        "ttys": a.callout.as_ref().map(|c| vec![c.clone()]).unwrap_or_default(),
    })
}

/// What watching the callout node saw.
///
/// Three outcomes, not two, because "no number" has two very different causes and
/// only one of them is a failure: a device with no serial driver attached has no
/// node to watch and a replug of it still succeeded, while a node that went away
/// and never came back is an adapter left off the bus — which is the worst thing
/// this helper can do and must not exit 0.
///
/// **The caller passes a path it already observed on the adapter**, so an absent
/// path here means the node is gone *now*, not that it never existed.
#[derive(Debug, PartialEq, Eq)]
enum Outage {
    /// No callout node to watch. Not a failure.
    NotWatchable,
    /// The node disappeared and did not return inside [`RETURN_TIMEOUT`].
    DidNotReturn,
    Returned(u64),
}

fn observed_outage(path: Option<&str>) -> Outage {
    let Some(path) = path else {
        return Outage::NotWatchable;
    };
    let p = Path::new(path);
    let start = Instant::now();
    let mut gone: Option<Instant> = None;
    while start.elapsed() < RETURN_TIMEOUT {
        let exists = p.exists();
        match (&gone, exists) {
            (None, false) => gone = Some(Instant::now()),
            (Some(g), true) => return Outage::Returned(g.elapsed().as_millis() as u64),
            _ => {}
        }
        std::thread::sleep(SAMPLE);
    }
    // Never seen to vanish at all within the window is also "not watchable" in
    // practice: the sampling missed a ~41 ms edge, or nothing happened to the node.
    // Only a node observed to LEAVE and not come back is a failure.
    if gone.is_some() {
        Outage::DidNotReturn
    } else {
        Outage::NotWatchable
    }
}

fn status(name: &str, json: bool) -> i32 {
    match usb_macos::find(name) {
        Err(e) => refuse(&e, json),
        Ok(a) => {
            if json {
                println!("{}", facts(&a));
            } else {
                println!(
                    "port {} vendor={:04x} product={:04x} session_id={:?} ttys={:?}",
                    a.serial, a.vendor_id, a.product_id, a.session_id, a.callout
                );
            }
            exit::READY
        }
    }
}

fn cycle(names: &[String], hold_ms: u64, dry_run: bool, json: bool) -> i32 {
    // Resolve every adapter BEFORE touching any of them, so a typo in the second
    // name cannot leave the first one already re-enumerated.
    let mut before = Vec::new();
    for n in names {
        match usb_macos::find(n) {
            Ok(a) => before.push(a),
            Err(e) => return refuse(&e, json),
        }
    }

    let mut results = Vec::new();
    // Adapters whose node went away and never came back. Collected rather than
    // returned on first sight, so a two-port cycle still reports both.
    let mut stranded: Vec<String> = Vec::new();
    for a in &before {
        let outcome = if dry_run {
            None
        } else {
            match usb_macos::reenumerate(&a.serial) {
                Ok(o) => Some(o),
                Err(e) => {
                    return refuse(&format!("re-enumerate {}: {e}", a.serial), json);
                }
            }
        };
        let outage = if dry_run {
            Outage::NotWatchable
        } else {
            observed_outage(a.callout.as_deref())
        };
        if outage == Outage::DidNotReturn {
            stranded.push(a.serial.clone());
        }
        // Re-read the identity so the witness is reported after, not predicted.
        let after = usb_macos::find(&a.serial).ok();
        results.push(serde_json::json!({
            "port": a.serial,
            "dry_run": dry_run,
            "reenumerated": outcome.is_some(),
            "vtable_vendor_id": outcome.as_ref().map(|o| format!("{:04x}", o.vtable_vendor_id)),
            "session_id_before": a.session_id,
            "session_id_after": after.as_ref().and_then(|x| x.session_id),
            // The discriminator, stated rather than left to the reader.
            "session_id_changed": after
                .as_ref()
                .and_then(|x| x.session_id)
                .zip(a.session_id)
                .map(|(x, y)| x != y),
            "observed_outage_ms": match outage {
                Outage::Returned(ms) => Some(ms),
                _ => None,
            },
            "node_left_and_did_not_return": outage == Outage::DidNotReturn,
            "node_returned": after.as_ref().and_then(|x| x.callout.clone()),
        }));
    }

    let payload = serde_json::json!({
        "verb": "cycle",
        "mechanism": "IOUSBLib USBDeviceReEnumerate",
        "hold_ms_requested": hold_ms,
        // Never silently dropped. The Linux arm can hold a device down; this one
        // cannot, and a caller reading this field cannot come to believe otherwise.
        "hold_ms_honoured": false,
        "hold_unsupported_because":
            "USBDeviceReEnumerate is atomic: the device drops and returns on the kernel's \
             own schedule (~41 ms measured on Darwin 24.6.0) and nothing in the API \
             lengthens it. `observed_outage_ms` is what actually happened.",
        "ports": results,
    });

    if json {
        println!("{payload}");
    } else {
        for r in &results {
            println!(
                "replug {} session_id {:?} -> {:?} (changed={:?}) outage={:?}ms",
                r["port"],
                r["session_id_before"],
                r["session_id_after"],
                r["session_id_changed"],
                r["observed_outage_ms"]
            );
        }
        if hold_ms > 0 && !dry_run {
            eprintln!(
                "serial-nexus-devprep: --hold-ms {hold_ms} was NOT honoured — this platform's \
                 re-enumeration is atomic; see observed_outage_ms in --json"
            );
        }
    }
    if stranded.is_empty() {
        exit::READY
    } else {
        // The device left the bus and did not come back inside the window. Exiting
        // 0 here would tell a caller its rig is intact when it is not.
        eprintln!(
            "serial-nexus-devprep: re-enumerated but the callout node never returned for {} \
             within {RETURN_TIMEOUT:?} — the adapter may be off the bus",
            stranded.join(", ")
        );
        exit::FAILED
    }
}

pub fn main() {
    let cli = Cli::parse();
    let code = match cli.verb {
        Verb::Capabilities { json } => {
            let v = serde_json::json!({
                "platform": "macos",
                "capability_required": false,
                "mechanism": "IOUSBLib USBDeviceReEnumerate",
                "why": "Re-enumeration here is an unprivileged IOKit call: USBDeviceOpen \
                        succeeds at an ordinary euid. The Linux helper exists to carry \
                        CAP_DAC_OVERRIDE for one sysfs write; there is no counterpart.",
                "ready": true,
            });
            if json {
                println!("{v}");
            } else {
                println!(
                    "no capability required on macOS: re-enumeration is an unprivileged \
                     IOUSBLib call"
                );
            }
            exit::READY
        }
        Verb::Status { port, json } => status(&port, json),
        Verb::Cycle {
            ports,
            hold_ms,
            dry_run,
            json,
        } => cycle(&ports, hold_ms, dry_run, json),
        Verb::Hold {
            ports,
            max_ms,
            dry_run,
        } => {
            let _ = (ports, max_ms, dry_run);
            refuse(
                "`hold` is not implementable on macOS. It exists to keep a device \
                 deauthorized for a caller-controlled window, and USBDeviceReEnumerate is \
                 atomic — the outage is the kernel's (~41 ms measured) and nothing in the \
                 API lengthens it. Use `cycle`, which reports the outage it actually \
                 produced, and sample around that.",
                false,
            )
        }
        Verb::Authorize { port, json } => {
            // Not an error to ask: the Linux verb repairs a run that died between the
            // two writes. There is no such window here, so the honest answer is that
            // the state it repairs cannot exist.
            match usb_macos::find(&port) {
                Err(e) => refuse(&e, json),
                Ok(a) => {
                    let v = serde_json::json!({
                        "port": a.serial,
                        "authorized": true,
                        "no_op_because":
                            "macOS has no deauthorized state to repair: re-enumeration is \
                             atomic, so a run cannot die between two writes and leave a \
                             device down.",
                    });
                    if json {
                        println!("{v}");
                    } else {
                        println!(
                            "{}: nothing to authorize — no deauthorized state on macOS",
                            a.serial
                        );
                    }
                    exit::READY
                }
            }
        }
        Verb::Install { profile, verify } => {
            let _ = (profile, verify);
            println!(
                "nothing to install on macOS: the mechanism is unprivileged, so there is \
                 no capability to bless and no blessed copy to install."
            );
            exit::READY
        }
        Verb::Preflight { profile, json } => {
            let _ = profile;
            let all = usb_macos::adapters();
            // **`ready` means a SERIAL adapter, not merely a device with a serial
            // number.** Every internal peripheral on this machine names one — a
            // Touch Bar, a camera — so counting those would report ready on a box
            // with no adapter attached at all, which is the one answer a caller uses
            // to decide whether to skip. A callout node is the evidence that a
            // serial driver attached, which is what makes the device replugable
            // *for this suite's purposes*.
            let serial_adapters: Vec<&usb_macos::UsbAdapter> =
                all.iter().filter(|a| a.callout.is_some()).collect();
            let ready = !serial_adapters.is_empty();
            let v = serde_json::json!({
                "platform": "macos",
                "capability_required": false,
                "ready": ready,
                "usb_devices_naming_a_serial": all.len(),
                "serial_adapters": serial_adapters.len(),
                "adapters": serial_adapters.iter().map(|a| facts(a)).collect::<Vec<_>>(),
                "why": if ready {
                    "An unprivileged IOUSBLib re-enumeration is available and at least one \
                     USB device has a serial driver attached (it owns a callout node)."
                } else {
                    "No USB device owns a callout node, so no serial adapter is attached \
                     for this helper to address. That is an absent facility, not a missing \
                     blessing — and it is the honest answer even though other USB devices \
                     on this machine do name serial numbers."
                },
            });
            if json {
                println!("{v}");
            } else if ready {
                println!(
                    "ready: {} serial adapter(s) of {} USB devices naming a serial, no \
                     blessing needed",
                    serial_adapters.len(),
                    all.len()
                );
            } else {
                println!(
                    "not ready: none of {} USB devices naming a serial owns a callout node",
                    all.len()
                );
            }
            if ready { exit::READY } else { exit::NOT_READY }
        }
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The outage watcher must not report a number it did not observe, and it must
    /// keep "nothing to watch" apart from "left and never came back" — only the
    /// second is a failure, and conflating them would either exit 0 on a stranded
    /// adapter or fail every replug of a device with no serial driver attached.
    ///
    /// The caller only ever passes a path it *already saw* on the adapter, so an
    /// absent path means the node is gone right now — and a node that is gone and
    /// stays gone for the whole window is the stranding this exit code exists for,
    /// not a "nothing to see here". `None` is the genuinely unwatchable case: no
    /// serial driver ever attached, so there was never a node.
    ///
    /// A zero would be worse than either answer — it reads as "no outage", the
    /// opposite of what happened.
    #[test]
    fn a_known_node_that_never_returns_is_a_stranding_and_no_node_is_not() {
        assert_eq!(
            observed_outage(None),
            Outage::NotWatchable,
            "an adapter with no callout node has nothing to watch and replugging it \
             is still a success"
        );
        assert_eq!(
            observed_outage(Some("/dev/snx-no-such-node")),
            Outage::DidNotReturn,
            "a node the caller knew about that is absent for the whole window is an \
             adapter left off the bus, and must not exit 0"
        );
        assert_ne!(Outage::Returned(0), Outage::NotWatchable);
    }

    /// A dry run must resolve names and change nothing, and it must still be a
    /// success — it is the positive control a caller's discriminator is checked
    /// against.
    #[test]
    fn a_dry_run_over_an_unknown_port_still_refuses() {
        assert_eq!(
            cycle(&["SNX-NO-SUCH".to_owned()], 0, true, true),
            exit::REFUSED,
            "an unresolvable name must be refused even under --dry-run"
        );
    }
}
