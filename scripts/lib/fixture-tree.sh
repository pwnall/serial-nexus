#!/usr/bin/env bash
# Build fixture `/dev/serial/by-id`, `/dev/serial/by-path`, and sysfs trees under
# a `--dev-root` prefix, so the resolver's identity↔path translation (§12) is
# exercised unprivileged against symlink trees that reproduce the real ones — the
# documented resolver seam (plan §3). No hardware, no root.
#
# The device node itself (`<root>/dev/<devname>`) is created by `nexus-sim pty
# --link <root>/dev/<devname>` (a symlink to a pts); these helpers add the by-id /
# by-path entries and the sysfs walk that yields `usb:vid:pid:serial:iface`.

# make_usb_iface ROOT USBDIR VID PID SERIAL DEVNAME IFACE BYID
# Adds one USB interface: the sysfs device (shared USBDIR) + its interface dir,
# the class/tty link, and a by-id entry. SERIAL may be "" for a no-serial clone.
make_usb_iface() {
  local root="$1" usbdir="$2" vid="$3" pid="$4" serial="$5" devname="$6" iface="$7" byid="$8"
  local dev="$root/sys/bus/usb/devices/$usbdir"
  mkdir -p "$dev"
  printf '%s' "$vid" > "$dev/idVendor"
  printf '%s' "$pid" > "$dev/idProduct"
  [ -n "$serial" ] && printf '%s' "$serial" > "$dev/serial"
  printf '%s' "FTDI-ish" > "$dev/manufacturer"
  printf '%s' "Fixture Serial" > "$dev/product"
  local ifdir="$dev/$usbdir:1.$iface"
  mkdir -p "$ifdir"
  printf '%s' "$iface" > "$ifdir/bInterfaceNumber"
  # class/tty/<devname>/device -> the interface dir (relative, stays in-tree).
  mkdir -p "$root/sys/class/tty/$devname"
  ln -sfn "../../../bus/usb/devices/$usbdir/$usbdir:1.$iface" \
    "$root/sys/class/tty/$devname/device"
  # by-id/<byid> -> ../../<devname>
  mkdir -p "$root/dev/serial/by-id"
  ln -sfn "../../$devname" "$root/dev/serial/by-id/$byid"
}

# make_bypath ROOT PORT DEVNAME — add a /dev/serial/by-path entry covering a
# device node (for the no-serial-clone by-path fallback, §12).
make_bypath() {
  local root="$1" port="$2" devname="$3"
  mkdir -p "$root/dev/serial/by-path"
  ln -sfn "../../$devname" "$root/dev/serial/by-path/$port"
}

# unplug_usb ROOT DEVNAME BYID — remove the by-id entry and sysfs device so the
# resolver sees the device as absent (an unplug), leaving the dev node to dangle
# when its sim is also killed.
unplug_usb() {
  local root="$1" devname="$2" byid="$3"
  rm -f "$root/dev/serial/by-id/$byid"
  rm -rf "$root/sys/class/tty/$devname"
}
