use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::{Mutex, RwLock, mpsc};
use tideline_sdk::Capability;
use tideline_sdk::types::Manifest;
use crate::capabilities::CapabilitySet;
use crate::events::EventBus;
use crate::install::{self, InstallError, InstallPreview};
use crate::logging::PluginLog;
use crate::manifest;
use crate::paths::{plugin_log_path, plugin_permissions_path, data_home};
use crate::runtime::PluginRuntime;
use crate::supervisor::{CrashDecision, CrashTracker};

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("install: {0}")] Install(#[from] InstallError),
    #[error("manifest: {0}")] Manifest(#[from] crate::manifest::ManifestError),
    #[error("io: {0}")] Io(#[from] std::io::Error),
    #[error("plugin {0:?} not installed")] NotInstalled(String),
    #[error("plugin {0:?} already running")] AlreadyRunning(String),
}

pub struct InstalledPlugin {
    pub manifest: Manifest,
    pub install_dir: PathBuf,
    pub granted: Arc<RwLock<CapabilitySet>>,
    pub crash_tracker: Mutex<CrashTracker>,
    pub runtime: Mutex<Option<Arc<PluginRuntime>>>,
}

pub struct PluginRegistry {
    pub bus: EventBus,
    pub installed: RwLock<HashMap<String, Arc<InstalledPlugin>>>,
}

fn write_permissions(plugin_id: &str, granted: &[Capability]) -> std::io::Result<()> {
    let path = plugin_permissions_path(plugin_id);
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    let file = tideline_sdk::types::PermissionsFile {
        plugin_id: plugin_id.to_string(),
        granted: granted.to_vec(),
    };
    let raw = toml::to_string(&file)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(path, raw)
}

fn read_permissions(plugin_id: &str) -> Option<Vec<Capability>> {
    let path = plugin_permissions_path(plugin_id);
    let raw = std::fs::read_to_string(&path).ok()?;
    let file: tideline_sdk::types::PermissionsFile = toml::from_str(&raw).ok()?;
    Some(file.granted)
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self { bus: EventBus::new(), installed: RwLock::new(HashMap::new()) }
    }

    pub fn inspect(&self, source: &PathBuf) -> Result<InstallPreview, RegistryError> {
        Ok(install::inspect(source)?)
    }

    pub async fn install(&self, preview: &InstallPreview, granted: &[Capability])
        -> Result<Arc<InstalledPlugin>, RegistryError>
    {
        let dir = install::commit_install(preview, granted)?;
        write_permissions(&preview.manifest.plugin.id, granted)?;
        let plugin = Arc::new(InstalledPlugin {
            manifest: preview.manifest.clone(),
            install_dir: dir,
            granted: Arc::new(RwLock::new(CapabilitySet::new(granted.iter().copied()))),
            crash_tracker: Mutex::new(CrashTracker::default()),
            runtime: Mutex::new(None),
        });
        self.installed.write().await
            .insert(preview.manifest.plugin.id.clone(), plugin.clone());
        Ok(plugin)
    }

    pub async fn discover(&self) -> Result<usize, RegistryError> {
        let root = data_home().join("plugins");
        if !root.exists() { return Ok(0); }
        let mut count = 0;
        for entry in std::fs::read_dir(&root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() { continue; }
            let manifest_path = entry.path().join("tideline-plugin.toml");
            if !manifest_path.exists() { continue; }
            let m = manifest::load(&manifest_path)?;
            let granted_caps = read_permissions(&m.plugin.id)
                .unwrap_or_else(|| m.capabilities.required.clone());
            let plugin = Arc::new(InstalledPlugin {
                manifest: m.clone(),
                install_dir: entry.path(),
                granted: Arc::new(RwLock::new(CapabilitySet::new(granted_caps))),
                crash_tracker: Mutex::new(CrashTracker::default()),
                runtime: Mutex::new(None),
            });
            self.installed.write().await.insert(m.plugin.id.clone(), plugin);
            count += 1;
        }
        Ok(count)
    }

    pub fn start(self: &Arc<Self>, plugin_id: &str)
        -> Pin<Box<dyn Future<Output = Result<Arc<PluginRuntime>, RegistryError>> + Send + '_>>
    {
        let registry = self.clone();
        let plugin_id = plugin_id.to_string();
        Box::pin(async move {
            let plugin = registry.installed.read().await
                .get(&plugin_id).cloned()
                .ok_or_else(|| RegistryError::NotInstalled(plugin_id.clone()))?;
            {
                let guard = plugin.runtime.lock().await;
                if guard.is_some() {
                    return Err(RegistryError::AlreadyRunning(plugin_id.clone()));
                }
            }
            let log = Arc::new(PluginLog::open(&plugin_log_path(&plugin_id)).await?);
            let (exit_tx, mut exit_rx) = mpsc::channel::<i32>(1);
            let runtime = PluginRuntime::spawn(
                plugin_id.clone(),
                plugin.install_dir.clone(),
                plugin.manifest.entry.exec.clone(),
                plugin.granted.clone(),
                log,
                exit_tx,
            ).await?;
            *plugin.runtime.lock().await = Some(runtime.clone());

            let mut event_rx = registry.bus.register_plugin(
                &plugin_id,
                plugin.manifest.contributes.publishes_topics.clone(),
            ).await;

            let transport = runtime.transport().await.expect("transport present after spawn");
            let mut requests = transport.take_requests().await;
            let ctx = crate::dispatcher::HostContext {
                plugin_id: plugin_id.clone(),
                granted: plugin.granted.clone(),
                bus: registry.bus.clone(),
            };
            tokio::spawn(async move {
                while let Some((req, ack)) = requests.recv().await {
                    let ctx = ctx.clone();
                    tokio::spawn(async move {
                        let resp = match crate::dispatcher::dispatch(&ctx, &req.method, req.params).await {
                            Ok(v) => tideline_sdk::rpc::Response::ok(req.id, v),
                            Err(e) => tideline_sdk::rpc::Response::err(req.id, e),
                        };
                        let _ = ack.send(resp);
                    });
                }
            });

            let transport_for_events = transport.clone();
            tokio::spawn(async move {
                while let Some(evt) = event_rx.recv().await {
                    let _ = transport_for_events.notify(
                        "host/event.fire",
                        Some(serde_json::json!({"topic": evt.topic, "params": evt.params})),
                    ).await;
                }
            });

            let reg2 = registry.clone();
            let pid2 = plugin_id.clone();
            tokio::spawn(async move {
                if let Some(code) = exit_rx.recv().await {
                    if let Some(p) = reg2.installed.read().await.get(&pid2).cloned() {
                        *p.runtime.lock().await = None;
                        reg2.bus.unregister_plugin(&pid2).await;
                        let decision = p.crash_tracker.lock().await.record(Instant::now(), code);
                        if let CrashDecision::RestartAfter(delay) = decision {
                            let reg3 = reg2.clone();
                            let pid3 = pid2.clone();
                            tokio::spawn(async move {
                                tokio::time::sleep(delay).await;
                                let _ = reg3.start(&pid3).await;
                            });
                        }
                    }
                }
            });

            Ok(runtime)
        })
    }

    pub async fn stop(&self, plugin_id: &str) {
        if let Some(p) = self.installed.read().await.get(plugin_id).cloned() {
            if let Some(rt) = p.runtime.lock().await.take() { rt.shutdown().await; }
            self.bus.unregister_plugin(plugin_id).await;
        }
    }

    pub async fn revoke_capability(&self, plugin_id: &str, cap: Capability)
        -> Result<bool, RegistryError>
    {
        let plugin = self.installed.read().await.get(plugin_id).cloned()
            .ok_or_else(|| RegistryError::NotInstalled(plugin_id.into()))?;
        let removed = plugin.granted.write().await.revoke(cap);
        if removed {
            let granted = plugin.granted.read().await.as_vec();
            write_permissions(plugin_id, &granted)?;
            if let Some(rt) = plugin.runtime.lock().await.as_ref() {
                if let Some(t) = rt.transport().await {
                    let _ = t.notify(
                        "plugin/permissions.changed",
                        Some(serde_json::json!({"granted": granted})),
                    ).await;
                }
            }
        }
        Ok(removed)
    }

    pub async fn grant_capability(&self, plugin_id: &str, cap: Capability)
        -> Result<(), RegistryError>
    {
        let plugin = self.installed.read().await.get(plugin_id).cloned()
            .ok_or_else(|| RegistryError::NotInstalled(plugin_id.into()))?;
        plugin.granted.write().await.grant(cap);
        let granted = plugin.granted.read().await.as_vec();
        write_permissions(plugin_id, &granted)?;
        if let Some(rt) = plugin.runtime.lock().await.as_ref() {
            if let Some(t) = rt.transport().await {
                let _ = t.notify(
                    "plugin/permissions.changed",
                    Some(serde_json::json!({"granted": granted})),
                ).await;
            }
        }
        Ok(())
    }

    pub async fn runtime(&self, plugin_id: &str) -> Option<Arc<PluginRuntime>> {
        let p = self.installed.read().await.get(plugin_id).cloned()?;
        let g = p.runtime.lock().await;
        g.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn install_then_discover() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_DATA_HOME", dir.path());
        std::env::set_var("XDG_CONFIG_HOME", cfg.path());
        let reg = Arc::new(PluginRegistry::new());
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let fixture = PathBuf::from(manifest_dir)
            .join("tests/fixtures/tideline-test-plugin");
        let preview = reg.inspect(&fixture).unwrap();
        let granted = preview.declared_required.clone();
        reg.install(&preview, &granted).await.unwrap();
        let count = reg.discover().await.unwrap();
        assert!(count >= 1);
    }
}

