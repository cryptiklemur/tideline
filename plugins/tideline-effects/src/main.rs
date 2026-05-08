mod audition;
mod chain_ops;
#[allow(dead_code)]
mod effect;
mod state;
mod discovery;
mod engine;
mod host;
mod pipewire_contributor;
mod iframe_bridge;
mod overlay_render;
mod rack;
#[allow(dead_code)]
mod persist;
mod namespace_config;
#[allow(dead_code)]
mod util;

#[allow(dead_code)]
mod suil_sys;
#[allow(dead_code)]
mod ui_bridge;

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

        // Phase 1: init engine BEFORE announcing to host. host.initialize()
        // triggers a contribute_request whose first-call sync (in
        // pipewire_contributor::respond) calls apply_persisted_chains against
        // the engine. If the engine isn't ready yet (~450ms livi+jack init),
        // every add_effect fails with "engine not initialized" and the
        // user sees no JACK clients spawn for any effect chain.
        let persisted = persist::load_chains_from_disk();

        let mut registry = host::FormatRegistry::new();
        registry.register(Arc::new(host::lv2::Lv2Format::default()));
        let engine_ready = match engine::AudioEngine::new(Arc::new(registry)) {
            Ok(engine) => {
                let engine = Arc::new(engine);
                self.state.set_engine(engine.clone());
                engine::spawn_ui_idle_pump(Arc::downgrade(&engine));
                // Catalog MUST be populated before apply_persisted_chains —
                // otherwise plugin_info_for returns None for every persisted
                // effect, add_effect fails, and the user observes "effects
                // lost on restart" even though chains.json had the data.
                discovery::run_first_boot(self.state.clone()).await;
                state::apply_persisted_chains(self.state.clone(), persisted).await;
                true
            }
            Err(e) => {
                error!(error = %e, "audio engine init failed");
                false
            }
        };

        // Phase 2: announce plugin + register surfaces. Contribute_request
        // RPCs that fire in response now find a ready engine.
        if let Err(e) = host.initialize(PLUGIN_ID, VERSION, SDK_VERSION).await {
            error!(?e, "initialize failed");
            return;
        }

        if !engine_ready {
            let _ = host
                .event_publish(
                    "tideline-effects:engine_unhealthy",
                    serde_json::json!({"reason": "audio engine init failed"}),
                )
                .await;
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

        let webview_path = std::env::current_dir()
            .ok()
            .map(|p| p.join("webviews/detail/index.html"))
            .filter(|p| p.exists())
            .unwrap_or_else(|| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("webviews/detail/index.html")
            })
            .to_string_lossy()
            .into_owned();
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

        // Phase 3: push startup state now that surfaces are registered.
        if engine_ready {
            let mut persisted_any = false;
            for channel in self.state.channels_with_effects().await {
                match crate::iframe_bridge::persist_channel(
                    &self.state,
                    &host,
                    channel,
                ).await {
                    Ok(_) => { persisted_any = true; }
                    Err(e) => warn!(?e, %channel, "startup persist_channel failed"),
                }
            }
            if persisted_any {
                if let Err(e) = host
                    .event_publish(
                        "tideline-effects:rack_changed",
                        serde_json::json!({ "reason": "startup_persist" }),
                    )
                    .await
                {
                    warn!(?e, "startup rack_changed publish failed");
                }
            }
        }

        for topic in &[
            "host:channel_removed",
            "host:pipewire_restarted",
        ] {
            if let Err(e) = host.event_subscribe(topic).await {
                warn!(?e, topic, "event_subscribe failed");
            }
        }

        // Periodic auto-save: pull current LV2 state every 10s and persist it.
        let s = self.state.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(10));
            tick.tick().await; // first tick fires immediately, skip it
            loop {
                tick.tick().await;
                if s.engine().is_some() {
                    persist::refresh_state_and_save(s.clone()).await;
                }
            }
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
            "effects.list_catalog" => rack::list_catalog(&self.state, params).await,
            "effects.persist_now" => {
                persist::refresh_state_and_save(self.state.clone()).await;
                Ok(serde_json::json!({ "ok": true }))
            }
            "effects.render_rack" => rack::render_rack(&self.state, params).await,
            "effects.rack_event" => rack::handle_event(&self.state, host, params).await,
            "pipewire.contribute_request" => {
                pipewire_contributor::respond(&self.state, params).await
            }
            "effects.audition_record_start" => {
                audition::handle_record_start(&self.state, host, params).await
            }
            "effects.audition_record_stop" => {
                audition::handle_record_stop(&self.state, host, params).await
            }
            "effects.audition_loop_start" => {
                audition::handle_loop_start(&self.state, host, params).await
            }
            "effects.audition_loop_stop" => {
                audition::handle_loop_stop(&self.state, host, params).await
            }
            "effects.audition_loop_pause" => {
                audition::handle_loop_pause(&self.state, host, params).await
            }
            "effects.audition_loop_resume" => {
                audition::handle_loop_resume(&self.state, host, params).await
            }
            "effects.audition_state" => {
                audition::handle_status(&self.state, host, params).await
            }
            "effects.audition_discard" => {
                audition::handle_discard(&self.state, host, params).await
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
            "host:channel_removed" => state::on_channel_removed(self.state.clone(), Some(params)).await,
            "host:pipewire_restarted" => {
                tracing::info!("pipewire restarted — recreating engine channels");
                state::recreate_engine_from_state(self.state.clone()).await;
            }
            other => warn!(topic = other, "unexpected event topic"),
        }
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,tideline_effects=debug")),
        )
        .init();

    let state = state::EffectsState::new(PLUGIN_ID);
    let plugin = EffectsPlugin { state };
    run(plugin).await;
}
