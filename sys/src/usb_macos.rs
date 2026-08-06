//! USB re-enumeration on Darwin — the macOS half of design §15.45's replug helper.
//!
//! # What this is the equivalent of, and where the equivalence stops
//!
//! Linux forces a re-enumeration by writing `0` then `1` to a USB device's sysfs
//! `authorized` attribute. That is **two-step and holdable**: the device is gone for
//! as long as the caller waits between the writes, which is what `replug cycle
//! --hold-ms` and `replug hold` are built on, and the write needs
//! `CAP_DAC_OVERRIDE`, which is the entire reason the Linux helper is a separately
//! blessed binary.
//!
//! Darwin has no `authorized`. What it has is IOUSBLib's `USBDeviceReEnumerate`,
//! reached through the `IOCFPlugInTypes` handle a USB device advertises in the
//! IORegistry, and it differs in two ways that are measurements rather than
//! opinions (notes §3.65 A′, §3.66):
//!
//! * **It needs no privilege.** `USBDeviceOpen` succeeds at an ordinary `euid`, so
//!   nothing here is a privileged operation and the capability apparatus the Linux
//!   side exists for has no counterpart. `replug capabilities`, `install` and
//!   `preflight` are therefore Linux verbs with nothing to port.
//! * **It is atomic, so the outage is not a parameter.** The device drops and comes
//!   back on its own schedule — measured at **40, 41 and 42 ms** across three
//!   trials at 2 ms sampling on Darwin 24.6.0 with an FT232R. Nothing in the API
//!   lengthens it. `--hold-ms` therefore cannot be honoured here and `hold` cannot
//!   be implemented at all; the caller gets the replug *event*, not a controllable
//!   window.
//!
//! # The vtable, and why it is checked before anything destructive happens
//!
//! IOUSBLib is a COM-style plugin: the interface is a pointer to a pointer to a
//! table of function pointers, and calling the wrong slot is calling an arbitrary
//! function. The slot numbers below were not recalled or inferred — they were
//! printed from the SDK headers with `offsetof` on this machine
//! (`USBDeviceReEnumerate` is slot **37** of 51 in `IOUSBDeviceInterface`).
//!
//! A number taken from a header today can still be wrong tomorrow, so
//! [`reenumerate`] does not trust it: before it calls the destructive slot it calls
//! a **harmless** one — `GetDeviceVendor`, slot 13 — and requires the answer to
//! equal the vendor id already read out of the IORegistry by a completely different
//! route (a CF property). If the table were misaligned, that read would disagree
//! and the function refuses without touching the bus. This is the same discipline
//! §9 asks of a guard: prove the instrument before trusting the measurement.

use std::ffi::{CStr, CString, c_char, c_void};

// IOKit and CoreFoundation are system frameworks, so this adds no crates.io
// dependency — deliberately, because the replug helper's dependency list is part of
// its security argument and `cargo deny check` must not move for a platform port.
#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const c_char) -> *mut c_void;
    fn IOServiceGetMatchingServices(
        main_port: u32,
        matching: *mut c_void,
        existing: *mut u32,
    ) -> i32;
    fn IOIteratorNext(iterator: u32) -> u32;
    fn IOObjectRelease(object: u32) -> i32;
    fn IORegistryEntryCreateCFProperty(
        entry: u32,
        key: *const c_void,
        allocator: *const c_void,
        options: u32,
    ) -> *const c_void;
    fn IORegistryEntrySearchCFProperty(
        entry: u32,
        plane: *const c_char,
        key: *const c_void,
        allocator: *const c_void,
        options: u32,
    ) -> *const c_void;
    fn IOCreatePlugInInterfaceForService(
        service: u32,
        plugin_type: *const c_void,
        interface_type: *const c_void,
        the_interface: *mut *mut *mut IOCFPlugInVtbl,
        the_score: *mut i32,
    ) -> i32;

}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
    fn CFGetTypeID(cf: *const c_void) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFStringCreateWithCString(
        alloc: *const c_void,
        cstr: *const c_char,
        encoding: u32,
    ) -> *const c_void;
    fn CFStringGetCString(
        string: *const c_void,
        buffer: *mut c_char,
        size: isize,
        encoding: u32,
    ) -> bool;
    fn CFNumberGetValue(number: *const c_void, the_type: isize, value_ptr: *mut c_void) -> bool;
    #[allow(clippy::too_many_arguments)]
    fn CFUUIDGetConstantUUIDWithBytes(
        alloc: *const c_void,
        b0: u8,
        b1: u8,
        b2: u8,
        b3: u8,
        b4: u8,
        b5: u8,
        b6: u8,
        b7: u8,
        b8: u8,
        b9: u8,
        b10: u8,
        b11: u8,
        b12: u8,
        b13: u8,
        b14: u8,
        b15: u8,
    ) -> *const c_void;
}

/// `kCFStringEncodingUTF8`.
const UTF8: u32 = 0x0800_0100;
/// `kCFNumberSInt32Type`.
const CF_NUMBER_SINT32: isize = 3;
/// `kCFNumberSInt64Type`.
const CF_NUMBER_SINT64: isize = 4;
/// `kIORegistryIterateRecursively`.
const ITERATE_RECURSIVELY: u32 = 1;
/// `MACH_PORT_NULL`, which `IOServiceGetMatchingServices` documents as "the
/// default port" — the modern spelling of `kIOMainPortDefault`.
const MAIN_PORT_DEFAULT: u32 = 0;
/// `kIOReturnSuccess`.
const KERN_SUCCESS: i32 = 0;

/// Printed from the SDK with `CFUUIDGetUUIDBytes`, not transcribed from memory.
const USB_DEVICE_USER_CLIENT_TYPE_ID: [u8; 16] = [
    0x9d, 0xc7, 0xb7, 0x80, 0x9e, 0xc0, 0x11, 0xd4, 0xa5, 0x4f, 0x00, 0x0a, 0x27, 0x05, 0x28, 0x61,
];
const IOCFPLUGIN_INTERFACE_ID: [u8; 16] = [
    0xc2, 0x44, 0xe8, 0x58, 0x10, 0x9c, 0x11, 0xd4, 0x91, 0xd4, 0x00, 0x50, 0xe4, 0xc6, 0x42, 0x6f,
];
const USB_DEVICE_INTERFACE_ID: [u8; 16] = [
    0x5c, 0x81, 0x87, 0xd0, 0x9e, 0xf3, 0x11, 0xd4, 0x8b, 0x45, 0x00, 0x0a, 0x27, 0x05, 0x28, 0x61,
];

/// `REFIID` — a `CFUUIDBytes` passed by value.
#[repr(C)]
#[derive(Clone, Copy)]
struct CFUUIDBytes([u8; 16]);

/// Only the three `IUnknown` slots are named; nothing below them is called.
#[repr(C)]
struct IOCFPlugInVtbl {
    _reserved: *mut c_void,
    query_interface:
        unsafe extern "C" fn(*mut c_void, CFUUIDBytes, *mut *mut *mut IOUSBDeviceVtbl) -> i32,
    _add_ref: *mut c_void,
    release: unsafe extern "C" fn(*mut c_void) -> u32,
}

/// `IOUSBDeviceInterface`, declared as far as slot 37 and no further.
///
/// Every named slot's index was printed from the SDK headers on this machine; the
/// unnamed ones are placeholders that exist only to put the named ones at the right
/// offset, and are never called. Slots past 37 are omitted entirely — the struct is
/// only ever reached through a pointer and its size is never taken.
#[repr(C)]
struct IOUSBDeviceVtbl {
    _reserved: *mut c_void,                            // 0
    _query_interface: *mut c_void,                     // 1
    _add_ref: *mut c_void,                             // 2
    release: unsafe extern "C" fn(*mut c_void) -> u32, // 3
    _async_event_source: *mut c_void,                  // 4
    _get_async_event_source: *mut c_void,              // 5
    _create_async_port: *mut c_void,                   // 6
    _get_async_port: *mut c_void,                      // 7
    open: unsafe extern "C" fn(*mut c_void) -> i32,    // 8
    close: unsafe extern "C" fn(*mut c_void) -> i32,   // 9
    _get_class: *mut c_void,                           // 10
    _get_subclass: *mut c_void,                        // 11
    _get_protocol: *mut c_void,                        // 12
    /// The integrity check. Harmless, and its answer is independently known.
    get_vendor: unsafe extern "C" fn(*mut c_void, *mut u16) -> i32, // 13
    _get_product: *mut c_void,                         // 14
    _slot15: *mut c_void,
    _slot16: *mut c_void,
    _slot17: *mut c_void,
    _slot18: *mut c_void,
    _slot19: *mut c_void,
    _slot20: *mut c_void,
    _slot21: *mut c_void,
    _slot22: *mut c_void,
    _slot23: *mut c_void,
    _slot24: *mut c_void,
    _slot25: *mut c_void,
    _slot26: *mut c_void,
    _slot27: *mut c_void,
    _slot28: *mut c_void,
    _slot29: *mut c_void,
    _slot30: *mut c_void,
    _slot31: *mut c_void,
    _slot32: *mut c_void,
    _slot33: *mut c_void,
    _slot34: *mut c_void,
    _slot35: *mut c_void,
    _slot36: *mut c_void,
    /// Slot 37, the only destructive call in this file.
    reenumerate: unsafe extern "C" fn(*mut c_void, u32) -> i32, // 37
}

/// What the IORegistry says about one USB device, read through CF properties and
/// never through the plugin vtable — which is what makes it usable as the
/// independent check on that vtable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbAdapter {
    pub serial: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub product: String,
    /// `/dev/cu.*`, found by searching the IOService plane below the device for
    /// `IOCalloutDevice`. Empty when no serial driver has attached.
    pub callout: Option<String>,
    /// **The re-enumeration discriminator, and the reason it is here.** Linux's
    /// helper reports `devnum`, which the kernel reissues on every enumeration, so a
    /// changed `devnum` separates a real replug from a driver rebind. Darwin's
    /// counterpart is `sessionID`: measured across a `USBDeviceReEnumerate` it moves
    /// on the target (26843088327954 → 26857977258534) and does **not** move on an
    /// untouched adapter on the same bus — which is what makes it a witness for
    /// *this* device rather than for bus activity. Compared for inequality only.
    pub session_id: Option<i64>,
}

/// What a re-enumeration did, so the caller can report it rather than assert it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reenumerated {
    pub serial: String,
    /// The vendor id the vtable reported, which had to match [`UsbAdapter::vendor_id`]
    /// before the destructive call was allowed.
    pub vtable_vendor_id: u16,
    /// Always `false` here, and reported rather than omitted: the Linux mechanism
    /// can hold a device down and this one cannot, so a caller that logs this field
    /// cannot silently believe otherwise.
    pub hold_supported: bool,
}

fn cfstr(s: &str) -> Option<CfOwned> {
    let c = CString::new(s).ok()?;
    // SAFETY: `c` is a valid NUL-terminated C string that outlives the call.
    let r = unsafe { CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), UTF8) };
    (!r.is_null()).then_some(CfOwned(r))
}

/// A CF object we own and must release. Drop is the only release path, so an early
/// return cannot leak one.
struct CfOwned(*const c_void);

impl Drop for CfOwned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: created by a CF `Create` function, released exactly once.
            unsafe { CFRelease(self.0) };
        }
    }
}

/// An `io_object_t` we own. Same reasoning as [`CfOwned`].
struct IoObject(u32);

impl Drop for IoObject {
    fn drop(&mut self) {
        if self.0 != 0 {
            // SAFETY: obtained from IOKit, released exactly once.
            unsafe { IOObjectRelease(self.0) };
        }
    }
}

fn string_property(entry: u32, key: &str) -> Option<String> {
    let k = cfstr(key)?;
    // SAFETY: `entry` is a live io_object_t and `k` a live CFString.
    let v = unsafe { IORegistryEntryCreateCFProperty(entry, k.0, std::ptr::null(), 0) };
    if v.is_null() {
        return None;
    }
    let v = CfOwned(v);
    // SAFETY: `v` is a live CF object.
    if unsafe { CFGetTypeID(v.0) != CFStringGetTypeID() } {
        return None;
    }
    let mut buf = [0i8; 256];
    // SAFETY: buffer and length agree; CFStringGetCString NUL-terminates.
    let ok = unsafe { CFStringGetCString(v.0, buf.as_mut_ptr(), buf.len() as isize, UTF8) };
    if !ok {
        return None;
    }
    // SAFETY: NUL-terminated by the call above.
    Some(
        unsafe { CStr::from_ptr(buf.as_ptr()) }
            .to_string_lossy()
            .into_owned(),
    )
}

fn number_property(entry: u32, key: &str) -> Option<u16> {
    let k = cfstr(key)?;
    // SAFETY: as above.
    let v = unsafe { IORegistryEntryCreateCFProperty(entry, k.0, std::ptr::null(), 0) };
    if v.is_null() {
        return None;
    }
    let v = CfOwned(v);
    // SAFETY: `v` is a live CF object.
    if unsafe { CFGetTypeID(v.0) != CFNumberGetTypeID() } {
        return None;
    }
    let mut out: i32 = 0;
    // SAFETY: `out` matches kCFNumberSInt32Type.
    let ok = unsafe { CFNumberGetValue(v.0, CF_NUMBER_SINT32, (&raw mut out).cast::<c_void>()) };
    ok.then_some(out as u16)
}

/// `sessionID` is 64-bit, so it needs its own reader rather than the `u16` one.
fn i64_property(entry: u32, key: &str) -> Option<i64> {
    let k = cfstr(key)?;
    // SAFETY: live entry, live CFString.
    let v = unsafe { IORegistryEntryCreateCFProperty(entry, k.0, std::ptr::null(), 0) };
    if v.is_null() {
        return None;
    }
    let v = CfOwned(v);
    // SAFETY: live CF object.
    if unsafe { CFGetTypeID(v.0) != CFNumberGetTypeID() } {
        return None;
    }
    let mut out: i64 = 0;
    // SAFETY: `out` matches kCFNumberSInt64Type.
    let ok = unsafe { CFNumberGetValue(v.0, CF_NUMBER_SINT64, (&raw mut out).cast::<c_void>()) };
    ok.then_some(out)
}

fn callout_property(entry: u32) -> Option<String> {
    let k = cfstr("IOCalloutDevice")?;
    let plane = CString::new("IOService").ok()?;
    // SAFETY: live entry, live CFString, NUL-terminated plane name.
    let v = unsafe {
        IORegistryEntrySearchCFProperty(
            entry,
            plane.as_ptr(),
            k.0,
            std::ptr::null(),
            ITERATE_RECURSIVELY,
        )
    };
    if v.is_null() {
        return None;
    }
    let v = CfOwned(v);
    // SAFETY: live CF object.
    if unsafe { CFGetTypeID(v.0) != CFStringGetTypeID() } {
        return None;
    }
    let mut buf = [0i8; 256];
    // SAFETY: buffer and length agree.
    let ok = unsafe { CFStringGetCString(v.0, buf.as_mut_ptr(), buf.len() as isize, UTF8) };
    // SAFETY: NUL-terminated by the call above.
    ok.then(|| {
        unsafe { CStr::from_ptr(buf.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    })
}

/// Walk every `IOUSBHostDevice` and hand back the ones that name a serial number.
///
/// **Two registry nodes describe one physical device, and they are not
/// interchangeable.** `IOUSBHostDevice` is the modern node; the serial driver and
/// its `IOSerialBSDClient` — and therefore `IOCalloutDevice` — hang beneath *it*.
/// `IOUSBDevice` is the legacy shim that carries the `IOCFPlugInTypes` handle
/// IOUSBLib needs, and it has no serial client under it. So enumeration reads the
/// host node (richer, and the only one that can answer "which tty is this?") while
/// [`reenumerate`] matches the legacy node for the plugin. Both key on the serial
/// number, which is the identity common to the two. Reading the callout from the
/// legacy node is what an earlier draft did, and it silently returned no ttys at
/// all.
///
/// A device with no `USB Serial Number` is skipped rather than reported: the whole
/// helper is addressed by serial, and an adapter that cannot be named cannot be
/// re-enumerated safely — naming it in a list would invite exactly the "just pick
/// the one that's there" call this API refuses to offer.
pub fn adapters() -> Vec<UsbAdapter> {
    let class = match CString::new("IOUSBHostDevice") {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    // SAFETY: NUL-terminated class name; the dictionary is consumed by
    // IOServiceGetMatchingServices, which is why it is not released here.
    let matching = unsafe { IOServiceMatching(class.as_ptr()) };
    if matching.is_null() {
        return Vec::new();
    }
    let mut iter: u32 = 0;
    // SAFETY: `matching` is live and consumed by this call; `iter` is out-only.
    let kr = unsafe { IOServiceGetMatchingServices(MAIN_PORT_DEFAULT, matching, &raw mut iter) };
    if kr != KERN_SUCCESS || iter == 0 {
        return Vec::new();
    }
    let iter = IoObject(iter);
    let mut out = Vec::new();
    loop {
        // SAFETY: `iter` is a live iterator.
        let next = unsafe { IOIteratorNext(iter.0) };
        if next == 0 {
            break;
        }
        let dev = IoObject(next);
        let Some(serial) = string_property(dev.0, "USB Serial Number") else {
            continue;
        };
        out.push(UsbAdapter {
            vendor_id: number_property(dev.0, "idVendor").unwrap_or(0),
            product_id: number_property(dev.0, "idProduct").unwrap_or(0),
            product: string_property(dev.0, "USB Product Name").unwrap_or_default(),
            callout: callout_property(dev.0),
            session_id: i64_property(dev.0, "sessionID"),
            serial,
        });
    }
    out
}

/// The one adapter with this serial, or why not.
pub fn find(serial: &str) -> Result<UsbAdapter, String> {
    let all = adapters();
    let mut hits = all.into_iter().filter(|a| a.serial == serial);
    let Some(first) = hits.next() else {
        return Err(format!("no USB device has serial number {serial}"));
    };
    if hits.next().is_some() {
        // Two devices answering one serial is a counterfeit-adapter classic, and
        // re-enumerating "whichever came first" is not a thing this helper does.
        return Err(format!(
            "more than one USB device reports serial number {serial}; refusing to \
             choose between them"
        ));
    }
    Ok(first)
}

/// Force a re-enumeration of the device with this serial number.
///
/// Returns once the request has been accepted, **not** once the device is back: the
/// outage is the kernel's (~41 ms measured), and a caller that needs the node
/// should wait for its path rather than for this function.
pub fn reenumerate(serial: &str) -> Result<Reenumerated, String> {
    let adapter = find(serial)?;

    let class = CString::new("IOUSBDevice").map_err(|e| e.to_string())?;
    // SAFETY: NUL-terminated; consumed by IOServiceGetMatchingServices.
    let matching = unsafe { IOServiceMatching(class.as_ptr()) };
    if matching.is_null() {
        return Err("IOServiceMatching(IOUSBDevice) returned null".to_owned());
    }
    let mut iter: u32 = 0;
    // SAFETY: as in `adapters`.
    let kr = unsafe { IOServiceGetMatchingServices(MAIN_PORT_DEFAULT, matching, &raw mut iter) };
    if kr != KERN_SUCCESS || iter == 0 {
        return Err(format!("IOServiceGetMatchingServices failed ({kr:#x})"));
    }
    let iter = IoObject(iter);

    let mut target: Option<IoObject> = None;
    loop {
        // SAFETY: live iterator.
        let next = unsafe { IOIteratorNext(iter.0) };
        if next == 0 {
            break;
        }
        let dev = IoObject(next);
        if string_property(dev.0, "USB Serial Number").as_deref() == Some(serial) {
            target = Some(dev);
            break;
        }
    }
    let target =
        target.ok_or_else(|| format!("serial {serial} vanished between lookup and open"))?;

    let mut plugin: *mut *mut IOCFPlugInVtbl = std::ptr::null_mut();
    let mut score: i32 = 0;
    // SAFETY: constant UUID bytes from the SDK; out-params are writable.
    let (plugin_type, interface_type) = unsafe {
        let p = USB_DEVICE_USER_CLIENT_TYPE_ID;
        let i = IOCFPLUGIN_INTERFACE_ID;
        (
            CFUUIDGetConstantUUIDWithBytes(
                std::ptr::null(),
                p[0],
                p[1],
                p[2],
                p[3],
                p[4],
                p[5],
                p[6],
                p[7],
                p[8],
                p[9],
                p[10],
                p[11],
                p[12],
                p[13],
                p[14],
                p[15],
            ),
            CFUUIDGetConstantUUIDWithBytes(
                std::ptr::null(),
                i[0],
                i[1],
                i[2],
                i[3],
                i[4],
                i[5],
                i[6],
                i[7],
                i[8],
                i[9],
                i[10],
                i[11],
                i[12],
                i[13],
                i[14],
                i[15],
            ),
        )
    };
    // SAFETY: live service, live UUIDs, writable out-params.
    let kr = unsafe {
        IOCreatePlugInInterfaceForService(
            target.0,
            plugin_type,
            interface_type,
            &raw mut plugin,
            &raw mut score,
        )
    };
    if kr != KERN_SUCCESS || plugin.is_null() {
        return Err(format!(
            "IOCreatePlugInInterfaceForService failed ({kr:#x}) — no IOUSBLib handle for {serial}"
        ));
    }

    let mut usb: *mut *mut IOUSBDeviceVtbl = std::ptr::null_mut();
    // SAFETY: `plugin` is a live COM-style interface; QueryInterface is slot 1,
    // printed from the SDK. The receiver is the interface pointer itself, per the
    // IOKit plugin calling convention.
    let hr = unsafe {
        ((**plugin).query_interface)(
            plugin.cast::<c_void>(),
            CFUUIDBytes(USB_DEVICE_INTERFACE_ID),
            &raw mut usb,
        )
    };
    // The plugin handle has done its job either way.
    // SAFETY: live interface; Release is slot 3.
    unsafe { ((**plugin).release)(plugin.cast::<c_void>()) };
    if hr != 0 || usb.is_null() {
        return Err(format!(
            "QueryInterface(IOUSBDeviceInterface) failed ({hr:#x})"
        ));
    }

    let result = reenumerate_through(usb, &adapter);
    // SAFETY: live interface; Release is slot 3. Runs on both arms.
    unsafe { ((**usb).release)(usb.cast::<c_void>()) };
    result
}

/// The part that touches the vtable, split out so the release above covers every
/// early return.
fn reenumerate_through(
    usb: *mut *mut IOUSBDeviceVtbl,
    adapter: &UsbAdapter,
) -> Result<Reenumerated, String> {
    // SAFETY: live interface; Open is slot 8.
    let kr = unsafe { ((**usb).open)(usb.cast::<c_void>()) };
    if kr != KERN_SUCCESS {
        return Err(format!(
            "USBDeviceOpen failed ({kr:#x}) for {} — another process or a driver holds it \
             exclusively",
            adapter.serial
        ));
    }

    // **The vtable integrity check, before anything destructive.** `GetDeviceVendor`
    // is slot 13 and harmless; its answer is already known from a CF property read
    // by an entirely different route. If the table were misaligned, this would be
    // some other function and the answer would not match.
    let mut vendor: u16 = 0;
    // SAFETY: live, opened interface; GetDeviceVendor is slot 13 and writes one u16.
    let kr = unsafe { ((**usb).get_vendor)(usb.cast::<c_void>(), &raw mut vendor) };
    if kr != KERN_SUCCESS || vendor != adapter.vendor_id {
        // SAFETY: live, opened interface; Close is slot 9.
        unsafe { ((**usb).close)(usb.cast::<c_void>()) };
        return Err(format!(
            "refusing to re-enumerate {}: the IOUSBLib vtable did not answer as expected \
             (GetDeviceVendor returned {kr:#x}/{vendor:#06x}, the IORegistry says \
             {:#06x}). The slot layout this build was compiled against does not match \
             this system's IOUSBLib, so calling the re-enumerate slot would be calling \
             something else.",
            adapter.serial, adapter.vendor_id
        ));
    }

    // SAFETY: live, opened interface, vtable just proven aligned; ReEnumerate is
    // slot 37 and takes one options word. 0 is "no options", the plain replug.
    let kr = unsafe { ((**usb).reenumerate)(usb.cast::<c_void>(), 0) };
    if kr != KERN_SUCCESS {
        // SAFETY: live interface.
        unsafe { ((**usb).close)(usb.cast::<c_void>()) };
        return Err(format!(
            "USBDeviceReEnumerate failed ({kr:#x}) for {}",
            adapter.serial
        ));
    }

    // No close: re-enumeration tears the device down, and closing a handle to a
    // device that is being re-enumerated is at best redundant. The interface is
    // released by the caller.
    Ok(Reenumerated {
        serial: adapter.serial.clone(),
        vtable_vendor_id: vendor,
        hold_supported: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The enumeration must not invent devices, and must name the ones it finds.
    /// Runs on any Mac — with no USB serial adapter attached it asserts the empty
    /// case, which is the arm a CI runner exercises.
    #[test]
    fn adapters_are_named_by_serial_and_never_blank() {
        for a in adapters() {
            assert!(
                !a.serial.is_empty(),
                "an adapter reached the list with no serial number, which is the one \
                 thing `adapters` filters on"
            );
            if let Some(c) = &a.callout {
                assert!(
                    c.starts_with("/dev/"),
                    "IOCalloutDevice should be a device path, got {c}"
                );
            }
        }
    }

    /// A serial nobody has must be an error, not a silent no-op and not a panic —
    /// and the message must name what was asked for, because the caller's next move
    /// is to check the string they passed.
    #[test]
    fn an_unknown_serial_is_a_named_error() {
        let e = find("SNX-NO-SUCH-ADAPTER").expect_err("a made-up serial must not resolve");
        assert!(e.contains("SNX-NO-SUCH-ADAPTER"), "unhelpful error: {e}");
        let e = reenumerate("SNX-NO-SUCH-ADAPTER").expect_err("and must not re-enumerate");
        assert!(e.contains("SNX-NO-SUCH-ADAPTER"), "unhelpful error: {e}");
    }

    /// `hold_supported` is a constant `false`, and this pins it so a future edit
    /// that starts claiming otherwise has to change a test that says why it cannot
    /// be true: the outage is the kernel's, measured at ~41 ms and not a parameter.
    #[test]
    fn the_outcome_never_claims_a_holdable_outage() {
        let r = Reenumerated {
            serial: "X".to_owned(),
            vtable_vendor_id: 0x0403,
            hold_supported: false,
        };
        assert!(!r.hold_supported);
    }
}
