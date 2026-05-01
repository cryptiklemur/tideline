use std::path::PathBuf;
use std::sync::Arc;
use tideline_host::PluginRegistry;

pub fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/tideline-test-plugin")
}

pub async fn fresh_registry() -> Arc<PluginRegistry> {
    let data = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let data_path = data.into_path();
    let cfg_path = cfg.into_path();
    let state_path = state.into_path();
    std::env::set_var("XDG_DATA_HOME", &data_path);
    std::env::set_var("XDG_CONFIG_HOME", &cfg_path);
    std::env::set_var("XDG_STATE_HOME", &state_path);

    let plugin_target = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/debug/tideline-test-plugin");
    let dest_dir = data_path.join("tideline/plugins/io.tideline.test");
    std::fs::create_dir_all(&dest_dir).unwrap();
    std::fs::copy(&plugin_target, dest_dir.join("tideline-test-plugin")).unwrap();
    std::fs::copy(
        fixture_dir().join("tideline-plugin.toml"),
        dest_dir.join("tideline-plugin.toml"),
    ).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(dest_dir.join("tideline-test-plugin"))
            .unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(dest_dir.join("tideline-test-plugin"), perms).unwrap();
    }

    let registry = Arc::new(PluginRegistry::new());
    registry.discover().await.unwrap();
    registry
}

pub async fn ensure_test_plugin_built() {
    let status = std::process::Command::new("cargo")
        .args(["build", "-p", "tideline-test-plugin"])
        .status()
        .expect("cargo build");
    assert!(status.success(), "test plugin failed to build");
}
