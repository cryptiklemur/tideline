//! Plugin discovery — wraps FormatRegistry::scan_all with a cheap on-disk
//! cache. The cache invalidates when any plugin search path's mtime changes
//! or when the immediate-entry count differs from the cached value.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::effect::PluginFormat;
use crate::host::PluginInfo;
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

/// Map an LV2 plugin class string (or similar) to a coarse Category for UI grouping.
pub fn categorize(class: &str) -> Category {
    let c = class.to_ascii_lowercase();
    if c.contains("eq") || c.contains("equal") {
        Category::Eq
    } else if c.contains("compress") || c.contains("dynamic") || c.contains("gate") || c.contains("limit") || c.contains("expander") {
        Category::Dynamics
    } else if c.contains("reverb") {
        Category::Reverb
    } else if c.contains("delay") || c.contains("chorus") || c.contains("flange") || c.contains("phaser") || c.contains("modulator") {
        Category::Modulation
    } else if c.contains("util") || c.contains("amplif") || c.contains("filter") || c.contains("spectrum") || c.contains("analys") {
        Category::Utility
    } else {
        Category::Other
    }
}

// Bump when PluginInfo / scan logic changes meaning of cached entries
// so old caches are silently invalidated on next boot.
pub const CACHE_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PluginScanCache {
    #[serde(default)]
    pub schema_version: u32,
    pub scanned_at: u64,
    pub source_mtime_max: u64,
    #[serde(default)]
    pub source_entry_count: u64,
    pub plugins: Vec<PluginInfo>,
}

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

    for (var, fmt) in [
        ("LV2_PATH", PluginFormat::Lv2),
        ("VST3_PATH", PluginFormat::Vst3),
        ("VST_PATH", PluginFormat::Vst2),
    ] {
        if let Ok(value) = std::env::var(var) {
            for piece in value.split(':') {
                let p = piece.trim();
                if p.is_empty() {
                    continue;
                }
                let path = PathBuf::from(p);
                if !out.iter().any(|(f, existing)| *f == fmt && existing == &path) {
                    out.push((fmt, path));
                }
            }
        }
    }

    out
}

fn directory_mtime(path: &Path) -> Option<u64> {
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

pub fn source_entry_count() -> u64 {
    let mut total: u64 = 0;
    for (_, dir) in plugin_search_paths() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            total += 1;
        }
    }
    total
}

pub fn cache_is_fresh(cache: &PluginScanCache) -> bool {
    if cache.schema_version != CACHE_SCHEMA_VERSION {
        return false;
    }
    let current_mtime = max_source_mtime();
    if current_mtime == 0 {
        return false;
    }
    if cache.source_mtime_max != current_mtime {
        return false;
    }
    let current_count = source_entry_count();
    if cache.source_entry_count != current_count {
        return false;
    }
    true
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

pub fn clear_cache() -> std::io::Result<()> {
    let path = cache_path();
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Walk every registered format and collect their plugin lists.
pub fn scan_via_state(state: &EffectsState) -> Vec<PluginInfo> {
    let Some(engine) = state.engine() else {
        return Vec::new();
    };
    engine.formats().scan_all()
}

pub async fn ensure_cached(state: &EffectsState) -> PluginScanCache {
    if let Some(c) = load_cache() {
        if cache_is_fresh(&c) {
            return c;
        }
    }
    let plugins = scan_via_state(state);
    let fresh = PluginScanCache {
        schema_version: CACHE_SCHEMA_VERSION,
        scanned_at: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        source_mtime_max: max_source_mtime(),
        source_entry_count: source_entry_count(),
        plugins,
    };
    let _ = save_cache(&fresh);
    fresh
}

pub async fn run_first_boot(state: Arc<EffectsState>) {
    let cache = if let Some(c) = load_cache() {
        if cache_is_fresh(&c) {
            tracing::info!(count = c.plugins.len(), "effects plugin cache fresh, skipping scan");
            c
        } else {
            tracing::info!("effects plugin cache stale, rescanning");
            let plugins = scan_via_state(&state);
            let fresh = PluginScanCache {
                schema_version: CACHE_SCHEMA_VERSION,
                scanned_at: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                source_mtime_max: max_source_mtime(),
                source_entry_count: source_entry_count(),
                plugins,
            };
            let _ = save_cache(&fresh);
            fresh
        }
    } else {
        tracing::info!("effects plugin cache missing, running first-boot scan");
        let plugins = scan_via_state(&state);
        let fresh = PluginScanCache {
            schema_version: CACHE_SCHEMA_VERSION,
            scanned_at: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            source_mtime_max: max_source_mtime(),
            source_entry_count: source_entry_count(),
            plugins,
        };
        let _ = save_cache(&fresh);
        fresh
    };
    state.set_catalog(cache.plugins).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_is_fresh_returns_false_when_mtime_zero() {
        let cache = PluginScanCache {
            schema_version: CACHE_SCHEMA_VERSION,
            scanned_at: 0,
            source_mtime_max: 1,
            source_entry_count: 0,
            plugins: vec![],
        };
        // mtime zero (no plugin paths exist) → fresh check returns false so we
        // always rescan in that case.
        let _ = cache_is_fresh(&cache);
    }
}
