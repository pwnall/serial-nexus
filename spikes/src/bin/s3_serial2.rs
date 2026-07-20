#![deny(unsafe_code)]

//! S3 — serial2 fit (design §7.1, §13; plan phase 0).
//!
//! Questions against a real adapter (nothing need be wired to the far end):
//!
//! * Custom/arbitrary baud (serial2 uses termios2/BOTHER on Linux).
//! * Modem-line get/set for the §7.1 control verbs (DTR/RTS set, CTS/DSR/CD/RI
//!   read).
//! * `set_break` toggling.
//! * `TIOCEXCL` exclusivity: serial2 does **not** take it (only `O_NOCTTY`),
//!   so the daemon must issue it on the raw fd — this spike proves a second
//!   open is then refused with `EBUSY`.
//! * Optional `--loopback` (TX↔RX jumper): seeded byte round-trip + checksum.
//! * Optional `--watch-unplug`: report the exact `io::Error` when the adapter
//!   is physically removed (feeds §7.1 faulted-and-wait detection).
//!
//! Skips cleanly (verdict `skipped`, exit 0) when no adapter is present or when
//! access is denied — the latter with a reason pointing at the udev/group fix.

use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::time::Duration;

use serde_json::json;
use serial2::{CharSize, FlowControl, Parity, SerialPort, Settings, StopBits};

/// Raw ioctls nix/serial2 don't wrap, localized exactly like the daemon's `sys`
/// module (§2).
mod sys {
    #![allow(unsafe_code)]
    use nix::libc;
    use std::os::fd::RawFd;

    nix::ioctl_none_bad!(tiocexcl, libc::TIOCEXCL);
    nix::ioctl_none_bad!(tiocnxcl, libc::TIOCNXCL);

    pub fn set_exclusive(fd: RawFd, on: bool) -> nix::Result<()> {
        // Safety: no-argument legacy ioctls on a valid fd.
        unsafe {
            if on {
                tiocexcl(fd)?;
            } else {
                tiocnxcl(fd)?;
            }
        }
        Ok(())
    }
}

const CUSTOM_BAUD: u32 = 250_000;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let device = arg_value(&args, "--device").or_else(first_by_id_device);
    let loopback = args.iter().any(|a| a == "--loopback");
    let watch_unplug = args.iter().any(|a| a == "--watch-unplug");

    let verdict = run(device, loopback, watch_unplug);
    println!("{verdict}");
    let pass = verdict
        .get("pass")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    std::process::exit(if pass { 0 } else { 1 });
}

fn arg_value(args: &[String], key: &str) -> Option<String> {
    args.iter()
        .position(|a| a == key)
        .and_then(|i| args.get(i + 1).cloned())
}

fn first_by_id_device() -> Option<String> {
    let rd = std::fs::read_dir("/dev/serial/by-id").ok()?;
    for e in rd.flatten() {
        if let Ok(t) = std::fs::read_link(e.path()) {
            if let Some(name) = t.file_name() {
                return Some(format!("/dev/{}", name.to_string_lossy()));
            }
        }
    }
    None
}

fn skipped(reason: &str, device: Option<&str>) -> serde_json::Value {
    json!({
        "tool": "s3_serial2", "spike": "S3",
        "skipped": true, "reason": reason,
        "device": device,
        "pass": true
    })
}

fn run(device: Option<String>, loopback: bool, watch_unplug: bool) -> serde_json::Value {
    let Some(device) = device else {
        return skipped(
            "no serial adapter present (no --device and empty /dev/serial/by-id)",
            None,
        );
    };

    // Configure raw + custom baud + explicit framing in the open closure (§7.1).
    let port = SerialPort::open(&device, |mut s: Settings| {
        s.set_raw();
        s.set_baud_rate(CUSTOM_BAUD)?;
        s.set_char_size(CharSize::Bits8);
        s.set_stop_bits(StopBits::One);
        s.set_parity(Parity::None);
        s.set_flow_control(FlowControl::None);
        Ok(s)
    });

    let mut port = match port {
        Ok(p) => p,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            return skipped(
                "permission denied opening the port — add access via udev (GROUP=plugdev) or the dialout group",
                Some(&device),
            );
        }
        Err(e) => {
            return json!({
                "tool": "s3_serial2", "spike": "S3", "device": device,
                "error": format!("open failed: {e} (kind={:?}, errno={:?})", e.kind(), e.raw_os_error()),
                "pass": false
            });
        }
    };

    // Custom baud read-back within serial2's tolerance.
    let baud_readback = port
        .get_configuration()
        .and_then(|c| c.get_baud_rate())
        .ok();
    let custom_baud_ok = baud_readback
        .map(|b| {
            (b as i64 - CUSTOM_BAUD as i64).unsigned_abs() as f64 / CUSTOM_BAUD as f64 <= 0.025
        })
        .unwrap_or(false);

    // Modem-line get/set — the §7.1 control verbs. Values depend on wiring; we
    // require only that the calls succeed.
    let modem_calls_ok = port.set_dtr(true).is_ok()
        && port.set_dtr(false).is_ok()
        && port.set_rts(true).is_ok()
        && port.set_rts(false).is_ok()
        && port.read_cts().is_ok()
        && port.read_dsr().is_ok()
        && port.read_cd().is_ok()
        && port.read_ri().is_ok();
    let modem_snapshot = json!({
        "cts": port.read_cts().ok(),
        "dsr": port.read_dsr().ok(),
        "cd": port.read_cd().ok(),
        "ri": port.read_ri().ok(),
    });

    // Break toggling (§7.1 send-break verb).
    let break_ok = port.set_break(true).is_ok() && port.set_break(false).is_ok();

    // TIOCEXCL: serial2 does not take it; issue it on the raw fd, then prove a
    // second open is refused (§7.1 "takes TIOCEXCL so stray processes cannot
    // share the port").
    let excl_set = sys::set_exclusive(port.as_raw_fd(), true).is_ok();
    let second_open = SerialPort::open(&device, 9600);
    let second_open_errno = second_open.as_ref().err().and_then(|e| e.raw_os_error());
    let exclusivity_ok =
        excl_set && second_open.is_err() && second_open_errno == Some(nix::libc::EBUSY);
    drop(second_open);
    let _ = sys::set_exclusive(port.as_raw_fd(), false);

    // Optional TX↔RX loopback data round-trip.
    let loopback_result = if loopback {
        Some(loopback_check(&mut port))
    } else {
        None
    };

    // Optional interactive unplug observation.
    let unplug_result = if watch_unplug {
        Some(watch_unplug_error(&mut port))
    } else {
        None
    };

    let pass = custom_baud_ok
        && modem_calls_ok
        && break_ok
        && exclusivity_ok
        && loopback_result
            .as_ref()
            .map(|v| v.get("ok").and_then(|b| b.as_bool()).unwrap_or(false))
            .unwrap_or(true);

    json!({
        "tool": "s3_serial2", "spike": "S3",
        "device": device,
        "requested_baud": CUSTOM_BAUD,
        "baud_readback": baud_readback,
        "custom_baud_ok": custom_baud_ok,
        "modem_calls_ok": modem_calls_ok,
        "modem_snapshot": modem_snapshot,
        "break_ok": break_ok,
        "tiocexcl_set": excl_set,
        "second_open_errno": second_open_errno,
        "exclusivity_ok": exclusivity_ok,
        "loopback": loopback_result,
        "unplug": unplug_result,
        "designed": {
            "custom_baud_ok": true, "exclusivity_ok": true,
            "note": "serial2 sets O_NOCTTY but not TIOCEXCL — daemon issues it on the raw fd"
        },
        "pass": pass
    })
}

fn loopback_check(port: &mut SerialPort) -> serde_json::Value {
    // Deterministic seeded stream; requires a TX↔RX jumper on the adapter.
    let payload: Vec<u8> = (0..1024u32)
        .map(|i| (i.wrapping_mul(2654435761) >> 24) as u8)
        .collect();
    let _ = port.set_read_timeout(Duration::from_millis(500));
    let _ = port.discard_buffers();
    if let Err(e) = port.write_all(&payload) {
        return json!({"ok": false, "error": format!("write: {e}")});
    }
    let mut got = Vec::new();
    let mut buf = [0u8; 256];
    while got.len() < payload.len() {
        match port.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => got.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => break,
            Err(e) => return json!({"ok": false, "error": format!("read: {e}")}),
        }
    }
    json!({
        "ok": got == payload,
        "sent": payload.len(),
        "received": got.len(),
    })
}

fn watch_unplug_error(port: &mut SerialPort) -> serde_json::Value {
    // Blocks reading until the adapter is removed; reports the exact error the
    // daemon will see and map to faulted-and-wait (§7.1).
    let _ = port.set_read_timeout(Duration::from_secs(3600));
    let mut buf = [0u8; 256];
    loop {
        match port.read(&mut buf) {
            Ok(0) => return json!({"observed": "EOF (0-byte read)"}),
            Ok(_) => continue,
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(e) => {
                return json!({
                    "observed": "read error",
                    "kind": format!("{:?}", e.kind()),
                    "errno": e.raw_os_error(),
                    "message": e.to_string(),
                });
            }
        }
    }
}
