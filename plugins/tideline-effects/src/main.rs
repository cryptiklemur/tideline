#[allow(dead_code)]
mod carla;
#[allow(dead_code)]
mod effect;
mod state;
#[allow(dead_code)]
mod discovery;
mod discovery_runner;
mod engine;
mod pipewire_contributor;
mod install;
mod install_runner;
mod install_script;
mod iframe_bridge;
mod overlay_render;
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

        if let Err(e) = engine::start(self.state.clone()).await {
            error!(error = %e, "engine start failed");
            let _ = host
                .event_publish(
                    "tideline-effects:engine_unhealthy",
                    serde_json::json!({"reason": e.to_string()}),
                )
                .await;
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
            "host:channel_removed" => state::on_channel_removed(self.state.clone(), params).await,
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
