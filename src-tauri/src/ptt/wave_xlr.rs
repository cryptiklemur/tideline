use crate::ptt::state::LedColor;
use hidapi::HidApi;

const VID: u16 = 0x0fd9;
const PID: u16 = 0x007d;

// PROTOCOL NOTE: The Wave XLR mute-LED protocol is not publicly documented.
// The byte sequences below are PLACEHOLDERS. To finish this feature, capture
// USB traffic on Windows with Wireshark + USBPcap while toggling mute in
// Wave Link, then derive the feature-report bytes for "set LED to red/blue".
//
// The HID interface to address is the non-audio-control interface (usage
// page typically 0xFF00 or similar — find it by enumerating interfaces and
// skipping ones with usage_page in {0x000B (Consumer), 0x0001 (Generic)}).
//
// Until captured, set_led() is a no-op that returns Ok(()) so the rest of
// the system keeps working.
const PLACEHOLDER_RED:  [u8; 2] = [0x00, 0xff]; // TODO: replace
const PLACEHOLDER_BLUE: [u8; 2] = [0x00, 0x00]; // TODO: replace
const _: &str = "Replace placeholders with real bytes after USB capture.";

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

pub fn set_led(color: LedColor) -> Result<(), String> {
    // HidApi::new() per call (not cached): caching trips E0597 because
    // device_list()'s iterator Drop borrows &HidApi past the MutexGuard's
    // release. ~5ms enumeration is fine for human-cadence LED toggles.
    let api = HidApi::new().map_err(|e| e.to_string())?;
    let info = api.device_list()
        .find(|d| d.vendor_id() == VID && d.product_id() == PID
                  && d.interface_number() != 0 // skip audio control interface
                  && d.usage_page() != 0x0001 && d.usage_page() != 0x000B)
        .ok_or("Wave XLR HID interface not found")?;
    let dev = info.open_device(&api).map_err(|e| e.to_string())?;
    let payload = match color { LedColor::Red => &PLACEHOLDER_RED, LedColor::Blue => &PLACEHOLDER_BLUE };
    // TODO(usb-capture): replace with actual feature-report layout once known.
    // Most Elgato HID devices use feature reports prefixed with a report id byte.
    // Example shape: [report_id, 0x01 (set-led cmd), r, g, b, 0x00, ...]
    dev.send_feature_report(payload).map_err(|e| e.to_string())?;
    Ok(())
}
