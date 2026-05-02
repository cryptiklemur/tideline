use std::fs;
use std::path::Path;

pub const VID: u16 = 0x0fd9;
pub const PID: u16 = 0x007d;

pub fn is_present() -> bool {
    is_present_at(Path::new("/sys/bus/usb/devices"))
}

pub fn is_present_at(root: &Path) -> bool {
    let entries = match fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return false,
    };
    let want_vid = format!("{:04x}", VID);
    let want_pid = format!("{:04x}", PID);
    for entry in entries.flatten() {
        let path = entry.path();
        let vid = fs::read_to_string(path.join("idVendor")).ok();
        let pid = fs::read_to_string(path.join("idProduct")).ok();
        if let (Some(v), Some(p)) = (vid, pid) {
            if v.trim().eq_ignore_ascii_case(&want_vid)
                && p.trim().eq_ignore_ascii_case(&want_pid)
            {
                return true;
            }
        }
    }
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LedColor {
    Blue,
    Red,
}

/// Writes the requested color to the device. Errors are non-fatal at the
/// runtime layer (LED state is best-effort).
pub trait LedWriter: Send + Sync {
    fn set_led(&self, color: LedColor) -> Result<(), String>;
}

/// Real hardware writer. Re-opens the HID handle on each call so hot-plug
/// after the plugin started still works.
pub struct HidLedWriter;

impl LedWriter for HidLedWriter {
    fn set_led(&self, color: LedColor) -> Result<(), String> {
        if !is_present() {
            return Err("Wave XLR not present".to_string());
        }
        let api = hidapi::HidApi::new().map_err(|e| format!("hidapi init: {e}"))?;
        let dev = api
            .open(VID, PID)
            .map_err(|e| format!("hidapi open: {e}"))?;
        let report = build_color_report(color);
        dev.send_feature_report(&report)
            .map_err(|e| format!("hidapi feature report: {e}"))
    }
}

/// 64-byte feature report. Report id = 0x03; byte 1 selects the color
/// bank (0x10 = mute / red, 0x11 = unmute / blue); remaining bytes 0.
/// Layout reverse-engineered from the previous native stub (see git
/// history of `src-tauri/src/ptt/wave_xlr.rs`); not documented by Elgato.
pub fn build_color_report(color: LedColor) -> [u8; 64] {
    let mut report = [0u8; 64];
    report[0] = 0x03;
    report[1] = match color {
        LedColor::Red => 0x10,
        LedColor::Blue => 0x11,
    };
    report
}

#[cfg(test)]
mod color_tests {
    use super::*;

    #[test]
    fn report_red_byte() {
        let r = build_color_report(LedColor::Red);
        assert_eq!(r[0], 0x03);
        assert_eq!(r[1], 0x10);
        assert!(r[2..].iter().all(|&b| b == 0));
    }

    #[test]
    fn report_blue_byte() {
        let b = build_color_report(LedColor::Blue);
        assert_eq!(b[0], 0x03);
        assert_eq!(b[1], 0x11);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_device(root: &Path, sub: &str, vid: &str, pid: &str) {
        let dev = root.join(sub);
        fs::create_dir_all(&dev).unwrap();
        fs::write(dev.join("idVendor"), vid).unwrap();
        fs::write(dev.join("idProduct"), pid).unwrap();
    }

    #[test]
    fn empty_sysfs_returns_false() {
        let t = tempfile::tempdir().unwrap();
        assert!(!is_present_at(t.path()));
    }

    #[test]
    fn missing_root_returns_false() {
        assert!(!is_present_at(Path::new("/nonexistent/path/xyz")));
    }

    #[test]
    fn matching_vid_pid_returns_true() {
        let t = tempfile::tempdir().unwrap();
        write_device(t.path(), "1-1", "0fd9", "007d");
        assert!(is_present_at(t.path()));
    }

    #[test]
    fn case_insensitive_match() {
        let t = tempfile::tempdir().unwrap();
        write_device(t.path(), "1-1", "0FD9", "007D");
        assert!(is_present_at(t.path()));
    }

    #[test]
    fn trims_trailing_newline() {
        let t = tempfile::tempdir().unwrap();
        write_device(t.path(), "1-1", "0fd9\n", "007d\n");
        assert!(is_present_at(t.path()));
    }

    #[test]
    fn other_vendor_returns_false() {
        let t = tempfile::tempdir().unwrap();
        write_device(t.path(), "1-1", "1234", "5678");
        assert!(!is_present_at(t.path()));
    }
}
