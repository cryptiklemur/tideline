use std::path::PathBuf;
use std::sync::Arc;
use tideline_host::PluginRegistry;
use tideline_host::paths::plugin_permissions_path;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/tideline-test-plugin")
}

#[tokio::test]
async fn install_writes_permissions_file_and_revoke_updates_it() {
    let data = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_DATA_HOME", data.path());
    std::env::set_var("XDG_CONFIG_HOME", cfg.path());

    let reg = Arc::new(PluginRegistry::new());
    let preview = reg.inspect(&fixture()).unwrap();
    let granted = preview.declared_required.clone();
    reg.install(&preview, &granted).await.unwrap();

    let perms_path = plugin_permissions_path("io.tideline.test");
    assert!(perms_path.exists(), "expected {perms_path:?}");
    let raw = std::fs::read_to_string(&perms_path).unwrap();
    assert!(raw.contains("io.tideline.test"));
    assert!(raw.contains("channel.read"));

    reg.revoke_capability("io.tideline.test", tideline_sdk::Capability::ChannelRead)
        .await.unwrap();
    let raw2 = std::fs::read_to_string(&perms_path).unwrap();
    assert!(!raw2.contains("channel.read"), "channel.read should be removed");
}
