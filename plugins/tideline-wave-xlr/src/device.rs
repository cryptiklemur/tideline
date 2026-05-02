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
