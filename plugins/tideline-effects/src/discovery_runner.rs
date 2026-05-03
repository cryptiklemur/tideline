use crate::discovery::{Category, PluginInfo};
use crate::effect::PluginFormat;
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

/// Carla-git on Arch installs discovery binaries here, off PATH.
const CARLA_BIN_DIR: &str = "/usr/lib/carla";

fn discovery_binary(format: PluginFormat) -> Result<PathBuf> {
    let name = match format {
        PluginFormat::Lv2 => "carla-discovery-native",
        PluginFormat::Vst3 => "carla-discovery-vst3",
        PluginFormat::Vst2 => "carla-discovery-vst",
        PluginFormat::Clap => "carla-discovery-clap",
    };
    let abs = PathBuf::from(CARLA_BIN_DIR).join(name);
    if abs.exists() {
        return Ok(abs);
    }
    // Fallback: PATH lookup (e.g. distros that install to /usr/bin)
    if let Ok(found) = which::which(name) {
        return Ok(found);
    }
    Err(anyhow!("{name} not found at {} or on PATH", abs.display()))
}

/// Runs `carla-discovery-<format> <FORMAT> <path>` and parses its output. Carla
/// emits key=value lines per plugin separated by `carla-discovery::end::------------`.
pub async fn discover_one(format: PluginFormat, path: &Path) -> Result<Vec<PluginInfo>> {
    let bin = discovery_binary(format)?;
    let format_arg = match format {
        PluginFormat::Lv2 => "LV2",
        PluginFormat::Vst3 => "VST3",
        PluginFormat::Vst2 => "VST2",
        PluginFormat::Clap => "CLAP",
    };
    let mut cmd = Command::new(&bin);
    cmd.arg(format_arg).arg(path);
    let fut = cmd.output();
    let out = timeout(Duration::from_secs(10), fut).await??;
    if !out.status.success() {
        return Ok(vec![]);
    }
    Ok(parse_carla_discovery(format, &String::from_utf8_lossy(&out.stdout)))
}

pub fn parse_carla_discovery(format: PluginFormat, stdout: &str) -> Vec<PluginInfo> {
    let mut out = Vec::new();
    let mut current_uri = String::new();
    let mut current_name = String::new();
    let mut current_vendor = String::new();
    let mut current_category = Category::Other;
    for line in stdout.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("carla-discovery::uri::") {
            current_uri = rest.into();
        } else if let Some(rest) = line.strip_prefix("carla-discovery::name::") {
            current_name = rest.into();
        } else if let Some(rest) = line.strip_prefix("carla-discovery::maker::") {
            current_vendor = rest.into();
        } else if let Some(rest) = line.strip_prefix("carla-discovery::category::") {
            current_category = parse_category(rest);
        } else if line == "carla-discovery::end::------------" {
            if !current_uri.is_empty() && !current_name.is_empty() {
                out.push(PluginInfo {
                    format,
                    uri: std::mem::take(&mut current_uri),
                    name: std::mem::take(&mut current_name),
                    vendor: std::mem::take(&mut current_vendor),
                    category: current_category,
                });
            }
            current_uri.clear();
            current_name.clear();
            current_vendor.clear();
            current_category = Category::Other;
        }
    }
    out
}

fn parse_category(raw: &str) -> Category {
    match raw {
        "eq" | "filter" => Category::Eq,
        "dynamics" | "compressor" | "expander" | "gate" | "limiter" => Category::Dynamics,
        "reverb" | "delay" => Category::Reverb,
        "modulator" | "chorus" | "flanger" | "phaser" => Category::Modulation,
        "utility" => Category::Utility,
        _ => Category::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_one_lv2_plugin_block() {
        let stdout = "\
carla-discovery::uri::http://lsp-plug.in/plugins/lv2/gate_mono
carla-discovery::name::LSP Gate Mono
carla-discovery::maker::Linux Studio Plugins Project
carla-discovery::category::dynamics
carla-discovery::end::------------
";
        let v = parse_carla_discovery(PluginFormat::Lv2, stdout);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].name, "LSP Gate Mono");
        assert_eq!(v[0].vendor, "Linux Studio Plugins Project");
        assert_eq!(v[0].category, Category::Dynamics);
    }

    #[test]
    fn parses_two_blocks_with_categories() {
        let stdout = "\
carla-discovery::uri::a
carla-discovery::name::A
carla-discovery::maker::vendor
carla-discovery::category::eq
carla-discovery::end::------------
carla-discovery::uri::b
carla-discovery::name::B
carla-discovery::maker::vendor
carla-discovery::category::reverb
carla-discovery::end::------------
";
        let v = parse_carla_discovery(PluginFormat::Vst3, stdout);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].category, Category::Eq);
        assert_eq!(v[1].category, Category::Reverb);
    }

    #[test]
    fn drops_blocks_missing_uri_or_name() {
        let stdout = "\
carla-discovery::name::orphan
carla-discovery::end::------------
";
        let v = parse_carla_discovery(PluginFormat::Lv2, stdout);
        assert!(v.is_empty());
    }
}
