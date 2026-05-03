use crate::install_script::INSTALL_SCRIPT;
use anyhow::{anyhow, Result};
use std::os::unix::fs::PermissionsExt;
use tokio::process::Command;

pub struct InstallOutcome {
    pub success: bool,
    pub stderr_tail: String,
}

pub async fn run_install_via_pkexec() -> Result<InstallOutcome> {
    let dir = tempfile::tempdir()?;
    let script_path = dir.path().join("tideline-effects-install.sh");
    tokio::fs::write(&script_path, INSTALL_SCRIPT).await?;
    let mut perms = tokio::fs::metadata(&script_path).await?.permissions();
    perms.set_mode(0o755);
    tokio::fs::set_permissions(&script_path, perms).await?;

    let out = Command::new("pkexec")
        .arg(&script_path)
        .output()
        .await
        .map_err(|e| anyhow!("spawn pkexec: {}", e))?;

    let stderr_tail: String = String::from_utf8_lossy(&out.stderr)
        .lines()
        .rev()
        .take(20)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");

    Ok(InstallOutcome {
        success: out.status.success(),
        stderr_tail,
    })
}
