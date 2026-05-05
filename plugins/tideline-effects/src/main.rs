#[allow(dead_code)]
mod carla;
mod chain_ops;
#[allow(dead_code)]
mod effect;
mod state;
mod discovery;
#[allow(dead_code)]
mod discovery_runner;
mod engine;
mod pipewire_contributor;
#[allow(dead_code)]
mod install;
mod install_runner;
#[allow(dead_code)]
mod install_script;
mod iframe_bridge;
mod overlay_render;
#[allow(dead_code)]
mod persist;
mod namespace_config;
#[allow(dead_code)]
mod util;

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tracing::{error, info, warn};

use tideline_sdk::rpc::{error_codes, RpcError};
use tideline_sdk::{run, HostClient, Plugin};

const PLUGIN_ID: &str = "tideline-effects";
const VERSION: &str = "0.1.0";
const SDK_VERSION: &str = "1.0";

struct EffectsPlugin {
    state: Arc<state::EffectsState>,
}

#[async_trait]
impl Plugin for EffectsPlugin {
    async fn on_ready(&self, host: Arc<HostClient>) {
        self.state.set_host(host.clone());

        if let Err(e) = host.initialize(PLUGIN_ID, VERSION, SDK_VERSION).await {
            error!(?e, "initialize failed");
            return;
        }

        if let Err(e) = host
            .register_settings_section(serde_json::json!({
                "surface_id": "effects",
                "title": "Effects",
                "icon": { "name": "fx" },
                "priority": 50,
                "tree": overlay_render::settings_tree(),
            }))
            .await
        {
            warn!(?e, "register_settings_section failed");
        }

        let webview_path = format!(
            "{}/webviews/detail/index.html",
            env!("CARGO_MANIFEST_DIR")
        );
        if let Err(e) = host
            .register_iframe_surface(serde_json::json!({
                "surface_id": overlay_render::RACK_IFRAME_SLOT,
                "entry_path": webview_path,
            }))
            .await
        {
            warn!(?e, "register_iframe_surface rack failed");
        }

        if let Err(e) = host
            .register_channel_overlay(serde_json::json!({
                "surface_id": "channel_card",
                "placement": "channel_card",
                "channel_filter": { "kind": "all" },
                "tree": overlay_render::channel_card_tree(false),
            }))
            .await
        {
            warn!(?e, "register_channel_overlay channel_card failed");
        }

        if let Err(e) = engine::start(self.state.clone()).await {
            error!(error = %e, "engine start failed");
            let _ = host
                .event_publish(
                    "tideline-effects:engine_unhealthy",
                    serde_json::json!({"reason": e.to_string()}),
                )
                .await;
        }

        for topic in &[
            "host:pipewire_restarting",
            "host:pipewire_restarted",
            "host:channel_removed",
        ] {
            if let Err(e) = host.event_subscribe(topic).await {
                warn!(?e, topic, "event_subscribe failed");
            }
        }

        let s = self.state.clone();
        tokio::spawn(async move {
            discovery::run_first_boot(s).await;
        });

        info!(plugin = PLUGIN_ID, "ready");
    }

    async fn on_request(
        &self,
        host: Arc<HostClient>,
        method: String,
        params: Option<Value>,
    ) -> Result<Value, RpcError> {
        match method.as_str() {
            "settings.section.render" => overlay_render::render_settings(&self.state, params).await,
            "settings.section.event" => {
                overlay_render::handle_settings_event(&self.state, host, params).await
            }
            "channel_overlay.render" => overlay_render::render_overlay(&self.state, params).await,
            "channel_overlay.event" => {
                overlay_render::handle_overlay_event(&self.state, host, params).await
            }
            "ui.iframe.message" => iframe_bridge::dispatch(&self.state, host, params).await,
            "pipewire.contribute_request" => {
                pipewire_contributor::respond(&self.state, params).await
            }
            other => Err(RpcError {
                code: error_codes::METHOD_NOT_FOUND,
                message: format!("unknown method {other}"),
                data: None,
            }),
        }
    }

    async fn on_event(&self, _host: Arc<HostClient>, topic: String, params: Value) {
        match topic.as_str() {
            "host:pipewire_restarting" => engine::on_pipewire_restart_pre(self.state.clone()).await,
            "host:pipewire_restarted" => engine::on_pipewire_restart_post(self.state.clone()).await,
            "host:channel_removed" => state::on_channel_removed(self.state.clone(), Some(params)).await,
            other => warn!(topic = other, "unexpected event topic"),
        }
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let state = state::EffectsState::new(PLUGIN_ID);
    let plugin = EffectsPlugin { state };
    run(plugin).await;
}
