use std::process::Command;

/// Mute or unmute a PipeWire/PulseAudio source by node name.
/// Returns Err(stderr text) on pactl failure.
pub fn set_source_mute(node: &str, muted: bool) -> Result<(), String> {
    if node.is_empty() {
        return Err("PTT input device not configured".into());
    }
    let out = Command::new("pactl")
        .args(["set-source-mute", node, if muted { "1" } else { "0" }])
        .output()
        .map_err(|e| format!("pactl exec failed: {}", e))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(())
}
