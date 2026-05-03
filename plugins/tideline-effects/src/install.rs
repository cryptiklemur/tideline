use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distro {
    Arch,
    DebianUbuntu,
    Fedora,
}

#[derive(Debug, Clone)]
pub struct InstallProbe {
    /// Path to the carla launcher (or the discovery binary on Arch where carla
    /// itself isn't on PATH but the discovery binary lives at /usr/lib/carla/).
    pub carla_path: Option<PathBuf>,
    pub lsp_plugins_present: bool,
}

impl InstallProbe {
    pub fn needs_install(&self) -> bool {
        self.carla_path.is_none() || !self.lsp_plugins_present
    }

    pub async fn run() -> Self {
        let carla = locate_carla().await;
        let lsp = lsp_plugins_present().await;
        Self {
            carla_path: carla,
            lsp_plugins_present: lsp,
        }
    }
}

pub fn detect_distro(os_release_body: &str) -> Option<Distro> {
    let mut id = "";
    let mut id_like = "";
    for line in os_release_body.lines() {
        if let Some(v) = line.strip_prefix("ID=") {
            id = v.trim_matches('"');
        } else if let Some(v) = line.strip_prefix("ID_LIKE=") {
            id_like = v.trim_matches('"');
        }
    }
    match id {
        "arch" | "manjaro" | "endeavouros" => Some(Distro::Arch),
        "debian" | "ubuntu" | "pop" | "linuxmint" | "elementary" => Some(Distro::DebianUbuntu),
        "fedora" | "rhel" | "centos" | "rocky" | "almalinux" => Some(Distro::Fedora),
        _ if id_like.contains("debian") => Some(Distro::DebianUbuntu),
        _ if id_like.contains("arch") => Some(Distro::Arch),
        _ if id_like.contains("fedora") || id_like.contains("rhel") => Some(Distro::Fedora),
        _ => None,
    }
}

pub async fn detect_distro_from_etc() -> Option<Distro> {
    let body = tokio::fs::read_to_string("/etc/os-release").await.ok()?;
    detect_distro(&body)
}

pub fn install_command(distro: Distro) -> String {
    match distro {
        Distro::Arch => "pacman -S --noconfirm carla lsp-plugins-vst3".into(),
        Distro::DebianUbuntu => "apt-get install -y carla lsp-plugins".into(),
        Distro::Fedora => "dnf install -y carla lsp-plugins".into(),
    }
}

/// Locate carla. Prefer PATH lookup of `carla`, fall back to checking the
/// Arch carla-git location at `/usr/lib/carla/carla-discovery-native`.
async fn locate_carla() -> Option<PathBuf> {
    if let Ok(p) = which::which("carla") {
        return Some(p);
    }
    let arch_path = Path::new("/usr/lib/carla/carla-discovery-native");
    if arch_path.exists() {
        return Some(arch_path.to_path_buf());
    }
    None
}

async fn lsp_plugins_present() -> bool {
    if let Ok(mut entries) = tokio::fs::read_dir("/usr/lib/vst3").await {
        while let Ok(Some(e)) = entries.next_entry().await {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with("LSP") || name.starts_with("lsp") {
                return true;
            }
        }
    }
    if let Ok(mut entries) = tokio::fs::read_dir("/usr/lib/lv2").await {
        while let Ok(Some(e)) = entries.next_entry().await {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with("lsp") || name.contains("lsp-plugins") {
                return true;
            }
        }
    }
    false
}
