use crate::discovery_runner::discover_one;
use crate::effect::PluginFormat;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use crate::state::EffectsState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Eq,
    Dynamics,
    Reverb,
    Modulation,
    Utility,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginInfo {
    pub format: PluginFormat,
    pub uri: String,
    pub name: String,
    pub vendor: String,
    pub category: Category,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PluginScanCache {
    pub scanned_at: u64,
    pub source_mtime_max: u64,
    pub plugins: Vec<PluginInfo>,
}

/// Cache lives at `~/.cache/tideline/effects/plugins.json` — namespaced
/// under `effects/` so this plugin doesn't clobber other plugins' caches.
pub fn cache_path() -> PathBuf {
    let dir = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("tideline")
        .join("effects");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("plugins.json")
}

pub fn plugin_search_paths() -> Vec<(PluginFormat, PathBuf)> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let mut out = Vec::new();
    for p in ["/usr/lib/lv2", "/usr/local/lib/lv2"] {
        out.push((PluginFormat::Lv2, PathBuf::from(p)));
    }
    out.push((PluginFormat::Lv2, home.join(".lv2")));
    for p in ["/usr/lib/vst3", "/usr/local/lib/vst3"] {
        out.push((PluginFormat::Vst3, PathBuf::from(p)));
    }
    out.push((PluginFormat::Vst3, home.join(".vst3")));
    for p in ["/usr/lib/vst", "/usr/local/lib/vst"] {
        out.push((PluginFormat::Vst2, PathBuf::from(p)));
    }
    out.push((PluginFormat::Vst2, home.join(".vst")));
    for p in ["/usr/lib/clap", "/usr/local/lib/clap"] {
        out.push((PluginFormat::Clap, PathBuf::from(p)));
    }
    out.push((PluginFormat::Clap, home.join(".clap")));
    out
}

pub fn directory_mtime(path: &Path) -> Option<u64> {
    let md = std::fs::metadata(path).ok()?;
    let m = md.modified().ok()?;
    Some(m.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_secs())
}

pub fn max_source_mtime() -> u64 {
    plugin_search_paths()
        .iter()
        .filter_map(|(_, p)| directory_mtime(p))
        .max()
        .unwrap_or(0)
}

pub fn cache_is_fresh(cache: &PluginScanCache) -> bool {
    let current = max_source_mtime();
    cache.source_mtime_max == current && current > 0
}

pub fn load_cache() -> Option<PluginScanCache> {
    let path = cache_path();
    let bytes = std::fs::read(&path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn save_cache(cache: &PluginScanCache) -> std::io::Result<()> {
    let path = cache_path();
    let bytes = serde_json::to_vec_pretty(cache)?;
    std::fs::write(&path, bytes)
}

pub async fn scan_all() -> PluginScanCache {
    let mut all = Vec::new();
    for (format, dir) in plugin_search_paths() {
        if !dir.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&dir)
            .max_depth(2)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let matches = match format {
                PluginFormat::Lv2 => path.extension().map(|e| e == "lv2").unwrap_or(false)
                    || path.file_name().map(|n| n.to_string_lossy().ends_with(".lv2")).unwrap_or(false),
                PluginFormat::Vst3 => path.extension().map(|e| e == "vst3").unwrap_or(false),
                PluginFormat::Vst2 => path.extension().map(|e| e == "so").unwrap_or(false)
                    && path.parent().and_then(|p| p.file_name()).map(|n| n == "vst").unwrap_or(false),
                PluginFormat::Clap => path.extension().map(|e| e == "clap").unwrap_or(false),
            };
            if !matches {
                continue;
            }
            match discover_one(format, path).await {
                Ok(mut found) => all.append(&mut found),
                Err(e) => tracing::warn!(?e, ?path, "discovery failed for plugin"),
            }
        }
    }
    PluginScanCache {
        scanned_at: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        source_mtime_max: max_source_mtime(),
        plugins: all,
    }
}

pub async fn ensure_cached() -> PluginScanCache {
    if let Some(c) = load_cache() {
        if cache_is_fresh(&c) {
            return c;
        }
    }
    let fresh = scan_all().await;
    let _ = save_cache(&fresh);
    fresh
}

pub async fn run_first_boot(_state: Arc<EffectsState>) {
    // Filled in by Task 25.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_is_fresh_returns_false_when_mtime_changed() {
        let cache = PluginScanCache {
            scanned_at: 0,
            source_mtime_max: 1,
            plugins: vec![],
        };
        assert!(!cache_is_fresh(&cache));
    }

    #[test]
    fn plugin_info_round_trips() {
        let p = PluginInfo {
            format: PluginFormat::Vst3,
            uri: "/usr/lib/vst3/LSP Compressor.vst3#0".into(),
            name: "LSP Compressor".into(),
            vendor: "LSP Project".into(),
            category: Category::Dynamics,
        };
        let s = serde_json::to_string(&p).unwrap();
        let back: PluginInfo = serde_json::from_str(&s).unwrap();
        assert_eq!(p, back);
    }
}
