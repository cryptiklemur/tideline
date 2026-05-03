use tideline_effects::discovery::{cache_is_fresh, load_cache, save_cache, PluginScanCache};

#[test]
fn save_and_load_cache_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_CACHE_HOME", dir.path());
    let cache = PluginScanCache {
        scanned_at: 12345,
        source_mtime_max: 67890,
        plugins: vec![],
    };
    save_cache(&cache).unwrap();
    let back = load_cache().unwrap();
    assert_eq!(cache, back);
}

#[test]
fn fresh_cache_check_handles_missing_dirs() {
    let cache = PluginScanCache::default();
    // With source_mtime_max = 0, cache is never fresh (forces a rescan on first launch).
    assert!(!cache_is_fresh(&cache));
}
