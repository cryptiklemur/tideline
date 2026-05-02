use crate::ptt::state::LedColor;

const VID: u16 = 0x0fd9;
const PID: u16 = 0x007d;

// LED control: the Wave XLR drives its own mute LED from the OS source-mute
// state — when PipeWire reports the capture source as muted, the device
// flips the LED to red on its own; unmute → blue. Since our PTT path mutes
// the source via `pactl`, the LED already follows correctly without any
// HID writes from us.
//
// We keep `set_led()` as an explicit no-op so the rest of the effect
// pipeline can stay symmetric (state.rs unconditionally emits a LedColor
// in its Effects). If we ever want to override the device's native color
// behavior, that would require capturing the proprietary HID protocol
// (Elgato doesn't document it) — see git history for the previous stub.

/// Returns true if a Wave XLR is currently enumerable on this machine.
///
/// Walks `/sys/bus/usb/devices/*/idVendor + idProduct` rather than using
/// hidapi: the Wave XLR exposes only Audio + Vendor-specific interfaces
/// (no HID), so hidapi's enumeration would always return false even with
/// the device plugged in. Sysfs is unprivileged and reliable on Linux.
pub fn is_present() -> bool {
    let entries = match std::fs::read_dir("/sys/bus/usb/devices") {
        Ok(e) => e,
        Err(_) => return false,
    };
    let want_vid = format!("{:04x}", VID);
    let want_pid = format!("{:04x}", PID);
    for entry in entries.flatten() {
        let path = entry.path();
        let vid = std::fs::read_to_string(path.join("idVendor")).ok();
        let pid = std::fs::read_to_string(path.join("idProduct")).ok();
        if let (Some(v), Some(p)) = (vid, pid) {
            if v.trim().eq_ignore_ascii_case(&want_vid) && p.trim().eq_ignore_ascii_case(&want_pid) {
                return true;
            }
        }
    }
    false
}

#[allow(dead_code)] // Removed in W5.T11; kept for one task to avoid module-decl churn.
pub fn set_led(_color: LedColor) -> Result<(), String> {
    // Intentional no-op — see file-level comment.
    Ok(())
}
