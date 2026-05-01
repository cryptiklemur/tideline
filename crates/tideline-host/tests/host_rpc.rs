mod common;

use std::time::Duration;
use serde_json::json;
use tideline_sdk::Capability::*;

const HOST_METHODS: &[&str] = &[
    "host/log.write",
    "host/channel.list",
    "host/channel.get",
    "host/channel.subscribe_meters",
    "host/channel.create",
    "host/channel.update",
    "host/channel.attach_data",
    "host/mix.attach_data",
    "host/levels.read",
    "host/audio.play",
    "host/source.set_mute",
    "host/sources.list",
    "host/audio.position",
    "host/notify",
    "host/ui.iframe.show",
    "host/ui.iframe.hide",
    "host/ui.channel_overlay.focus",
    "host/keybind.register",
    "host/keybind.unregister",
    "host/pipewire.contribute",
    "host/config.namespace.get",
    "host/config.namespace.set",
    "host/config.read",
    "host/config.write",
    "host/fs.read",
    "host/fs.write",
    "host/net.http",
    "host/process.spawn",
    "host/secrets.read",
    "host/secrets.write",
];

#[tokio::test]
async fn host_rpc_smoke_all_methods() {
    common::ensure_test_plugin_built().await;
    let reg = common::fresh_registry().await;

    for c in [
        ChannelRead, ChannelWrite, ChannelSubscribe, ChannelCreate, ChannelAttachData,
        MixAttachData, LevelsRead, AudioPlay, AudioMute, AudioBackendStatus, AudioPosition,
        TrayContribute, UiIframe, UiChannelOverlay, KeybindRegister, PipewireContribute,
        ConfigNamespaceRead, ConfigNamespaceWrite, ConfigRead, ConfigWrite,
        FsRead, FsWrite, NetHttp, ProcessSpawn, SecretsRead, SecretsWrite, LogWrite,
    ] {
        reg.grant_capability("io.tideline.test", c).await.unwrap();
    }

    reg.start("io.tideline.test").await.unwrap();

    let runtime = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(rt) = reg.runtime("io.tideline.test").await { return rt; }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }).await.expect("plugin start");
    let transport = runtime.transport().await.expect("transport");

    tokio::time::sleep(Duration::from_millis(300)).await;

    let resp = transport
        .call("plugin/smoke_run", Some(json!({})), Duration::from_secs(20))
        .await
        .expect("smoke_run");

    let results = resp.get("results").and_then(|v| v.as_array())
        .expect("results array");

    let mut failed: Vec<String> = Vec::new();
    for entry in results {
        let method = entry.get("method").and_then(|v| v.as_str()).unwrap_or("?");
        let ok = entry.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
        let error = entry.get("error").and_then(|v| v.as_str()).unwrap_or("");
        if !ok {
            failed.push(format!("{method}: {error}"));
        }
    }
    assert!(failed.is_empty(), "failed methods: {:#?}", failed);
    assert_eq!(results.len(), HOST_METHODS.len(), "result count mismatch");

    reg.stop("io.tideline.test").await;
}
