use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use tideline_sdk::rpc::{error_codes, RpcError};
use tideline_sdk::{run, HostClient, Plugin};
use tideline_tones::config::TonesConfig;
use tideline_tones::settings::{render, EVT_ENABLED, EVT_VOLUME, SECTION_ID};

const TOPIC_PTT_STATE: &str = "tideline-ptt:state_changed";
const PLUGIN_ID: &str = "tideline-tones";
const SDK_VERSION: &str = "1.0";
const VERSION: &str = "1.0.0";
const DEBOUNCE_MS: u64 = 40;

const MIC_UNMUTE_WAV: &[u8] = include_bytes!("../assets/mic-unmute.wav");
const MIC_MUTE_WAV: &[u8] = include_bytes!("../assets/mic-mute.wav");

struct TonesPlugin {
    cfg: Arc<Mutex<TonesConfig>>,
    last_played_ms: Arc<AtomicU64>,
}

#[async_trait]
impl Plugin for TonesPlugin {
    async fn on_ready(&self, host: Arc<HostClient>) {
        if let Err(e) = host.initialize(PLUGIN_ID, VERSION, SDK_VERSION).await {
            error!(?e, "initialize failed");
            return;
        }
        if let Err(e) = host
            .register_settings_section(json!({
                "surface_id": SECTION_ID,
                "title": "PTT Tones",
                "icon": { "name": "volume-up" },
                "priority": 110,
                "parent_surface_id": "tideline-ptt",
                "tree": {},
            }))
            .await
        {
            warn!(?e, "register_settings_section failed");
        }
        match host.config_namespace_get(PLUGIN_ID).await {
            Ok(v) => *self.cfg.lock().await = TonesConfig::from_json(&v),
            Err(e) => warn!(?e, "config_namespace_get failed; using defaults"),
        }
        if let Err(e) = host.event_subscribe(TOPIC_PTT_STATE).await {
            error!(?e, "event_subscribe failed");
        }
        let cfg_now = self.cfg.lock().await.clone();
        if let Err(e) = host.settings_section_render(SECTION_ID, serde_json::to_value(render(&cfg_now)).unwrap_or(serde_json::Value::Null)).await {
            warn!(?e, "initial settings_section_render failed");
        }
        info!(plugin = PLUGIN_ID, "ready");
    }

    async fn on_event(&self, _host: Arc<HostClient>, topic: String, params: Value) {
        eprintln!("TONES: on_event topic={} params={}", topic, params);
        if topic != TOPIC_PTT_STATE {
            return;
        }
        let cfg_now = self.cfg.lock().await.clone();
        if !cfg_now.enabled {
            eprintln!("TONES: bail - disabled");
            return;
        }
        let Some(tone) = params.get("play_tone").and_then(|t| t.as_str()) else {
            eprintln!("TONES: bail - no play_tone field");
            return;
        };
        let wav: &'static [u8] = match tone {
            "up" => MIC_UNMUTE_WAV,
            "down" => MIC_MUTE_WAV,
            other => {
                warn!(?other, "unknown play_tone value");
                return;
            }
        };
        let now_ms = now_millis();
        let prev = self.last_played_ms.swap(now_ms, Ordering::SeqCst);
        if now_ms.saturating_sub(prev) < DEBOUNCE_MS {
            eprintln!("TONES: bail - debounce");
            return;
        }
        let volume = cfg_now.volume_scalar();
        eprintln!("TONES: playing tone={} volume={}", tone, volume);
        let tone_label = tone.to_string();
        tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            use tokio::process::Command;
            // sox applies a 5ms fade-in / 50ms fade-out so the wav doesn't end
            // on a non-zero sample (which paplay's stream tear-down turns into
            // a sharp click). paplay routes through the pulse/pipewire default
            // sink with media.role=event so notification sounds follow system
            // routing rules instead of getting locked to whatever device a
            // direct ALSA path would grab.
            let pa_volume = (volume.clamp(0.0, 1.0) * 65536.0).round() as u32;
            let cmd = format!(
                "sox - -t wav - fade t 0.005 0 0.05 | paplay --volume={} --property=media.role=event --client-name=tideline-tones",
                pa_volume
            );
            let mut child = match Command::new("sh")
                .arg("-c")
                .arg(&cmd)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("TONES: tone pipeline spawn failed tone={} err={:?}", tone_label, e);
                    return;
                }
            };
            if let Some(mut stdin) = child.stdin.take() {
                if let Err(e) = stdin.write_all(wav).await {
                    eprintln!("TONES: pipeline write failed tone={} err={:?}", tone_label, e);
                    return;
                }
                drop(stdin);
            }
            match child.wait_with_output().await {
                Ok(out) if !out.status.success() => {
                    eprintln!(
                        "TONES: tone pipeline exited non-zero tone={} status={:?} stderr={}",
                        tone_label,
                        out.status,
                        String::from_utf8_lossy(&out.stderr)
                    );
                }
                Err(e) => {
                    eprintln!("TONES: tone pipeline wait failed tone={} err={:?}", tone_label, e);
                }
                _ => {}
            }
        });
    }

    async fn on_request(
        &self,
        host: Arc<HostClient>,
        method: String,
        params: Option<Value>,
    ) -> Result<Value, RpcError> {
        match method.as_str() {
            "settings.section.render" => {
                let section_id = params
                    .as_ref()
                    .and_then(|v| v.get("section_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if section_id != SECTION_ID {
                    return Err(RpcError {
                        code: error_codes::INVALID_PARAMS,
                        message: format!("unknown section_id {section_id}"),
                        data: None,
                    });
                }
                let cfg_now = self.cfg.lock().await.clone();
                Ok(serde_json::to_value(render(&cfg_now)).unwrap_or(Value::Null))
            }
            "settings.section.event" => {
                let p = params.unwrap_or(Value::Null);
                let section_id = p.get("section_id").and_then(|v| v.as_str()).unwrap_or("");
                if section_id != SECTION_ID {
                    return Err(RpcError {
                        code: error_codes::INVALID_PARAMS,
                        message: format!("unknown section_id {section_id}"),
                        data: None,
                    });
                }
                let event_id = p
                    .get("event_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let value = p.get("value").cloned().unwrap_or(Value::Null);
                let mut cfg_lock = self.cfg.lock().await;
                match event_id.as_str() {
                    EVT_ENABLED => {
                        if let Some(b) = value.as_bool() {
                            cfg_lock.enabled = b;
                        }
                    }
                    EVT_VOLUME => {
                        if let Some(n) = value.as_u64() {
                            cfg_lock.volume = (n as u32).min(100);
                        }
                    }
                    _ => warn!(?event_id, "unknown settings event"),
                }
                let cfg_clone = cfg_lock.clone();
                drop(cfg_lock);
                if let Ok(v) = serde_json::to_value(&cfg_clone) {
                    if let Err(e) = host.config_namespace_set(PLUGIN_ID, v).await {
                        error!(?e, "config_namespace_set failed");
                    }
                }
                if let Err(e) = host
                    .settings_section_render(SECTION_ID, serde_json::to_value(render(&cfg_clone)).unwrap_or(Value::Null))
                    .await
                {
                    error!(?e, "settings_section_render failed");
                }
                Ok(json!({}))
            }
            _ => Err(RpcError {
                code: error_codes::METHOD_NOT_FOUND,
                message: format!("unknown method {method}"),
                data: None,
            }),
        }
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let _log_guard = tideline_sdk::logging::init("tideline-tones");
    let plugin = TonesPlugin {
        cfg: Arc::new(Mutex::new(TonesConfig::default())),
        last_played_ms: Arc::new(AtomicU64::new(0)),
    };
    run(plugin).await;
}

fn now_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
