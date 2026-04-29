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
pub fn is_present() -> bool {
    let Ok(api) = HidApi::new() else { return false; };
    let found = api.device_list().any(|d| d.vendor_id() == VID && d.product_id() == PID);
    found
}

pub fn set_led(color: LedColor) -> Result<(), String> {
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
