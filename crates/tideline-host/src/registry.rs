use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::{Mutex, RwLock, mpsc};
use tideline_sdk::Capability;
use tideline_sdk::types::Manifest;
use crate::capabilities::CapabilitySet;
use crate::contributions::{
    ChannelOverlayContribution, Contributions, IframeSurface, KeybindActionContribution,
    SettingsSectionContribution, StatusPillContribution, TrayItemContribution,
};
use crate::events::EventBus;
use crate::iframe::IframeMessage;
use crate::install::{self, InstallError, InstallPreview};
use crate::logging::PluginLog;
use crate::manifest;
use crate::paths::{plugin_log_path, plugin_permissions_path, data_home};
use crate::runtime::PluginRuntime;
use crate::supervisor::{CrashDecision, CrashTracker};
use crate::transport::TransportError;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("install: {0}")] Install(#[from] InstallError),
    #[error("manifest: {0}")] Manifest(#[from] crate::manifest::ManifestError),
    #[error("io: {0}")] Io(#[from] std::io::Error),
    #[error("plugin {0:?} not installed")] NotInstalled(String),
    #[error("plugin {0:?} already running")] AlreadyRunning(String),
    #[error("transport: {0}")] Transport(String),
    #[error("plugin error: {0}")] PluginError(String),
    #[error("invalid contribution payload: {0}")] InvalidContribution(String),
    #[error("unknown contribution kind {0:?}")] UnknownContributionKind(String),
}

/// Discriminator for the 6 contribution surface types. Used by the dispatcher
/// to route `host/contributions.{register,unregister}_*` RPC calls into the
/// shared `register_contribution` / `unregister_contribution` paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContribKind {
    SettingsSection,
    StatusPill,
    ChannelOverlay,
    IframeSurface,
    TrayItem,
    KeybindAction,
}

impl ContribKind {
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "settings_section" => Self::SettingsSection,
            "status_pill" => Self::StatusPill,
            "channel_overlay" => Self::ChannelOverlay,
            "iframe_surface" => Self::IframeSurface,
            "tray_item" => Self::TrayItem,
            "keybind_action" => Self::KeybindAction,
            _ => return None,
        })
    }
}

/// Per-plugin contribution storage. The registry maintains one of these per
/// running plugin; the global `Contributions` aggregate is recomputed (and
/// broadcast) on every register/unregister/evict.
#[derive(Debug, Clone, Default)]
pub struct PluginContribs {
    pub settings_sections: Vec<SettingsSectionContribution>,
    pub status_pills: Vec<StatusPillContribution>,
    pub channel_overlays: Vec<ChannelOverlayContribution>,
    pub iframe_surfaces: Vec<IframeSurface>,
    pub tray_items: Vec<TrayItemContribution>,
    pub keybind_actions: Vec<KeybindActionContribution>,
}

type TestHandler = Arc<dyn Fn(&str, &serde_json::Value) -> serde_json::Value + Send + Sync>;

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
    pub backend: RwLock<Arc<dyn crate::backend::HostBackend>>,
    contributions: RwLock<Contributions>,
    pub plugin_contribs: RwLock<HashMap<String, PluginContribs>>,
    contrib_tx: tokio::sync::broadcast::Sender<Contributions>,
    iframe_tx: tokio::sync::broadcast::Sender<IframeMessage>,
    test_handlers: RwLock<HashMap<String, TestHandler>>,
}

fn write_permissions(plugin_id: &str, granted: &[Capability]) -> std::io::Result<()> {
    let path = plugin_permissions_path(plugin_id);
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    let file = tideline_sdk::types::PermissionsFile {
        plugin_id: plugin_id.to_string(),
        granted: granted.to_vec(),
    };
    let raw = toml::to_string(&file).map_err(std::io::Error::other)?;
    std::fs::write(path, raw)
}

fn read_permissions(plugin_id: &str) -> Option<Vec<Capability>> {
    let path = plugin_permissions_path(plugin_id);
    let raw = std::fs::read_to_string(&path).ok()?;
    let file: tideline_sdk::types::PermissionsFile = toml::from_str(&raw).ok()?;
    Some(file.granted)
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginRegistry {
    pub fn new() -> Self {
        let (contrib_tx, _) = tokio::sync::broadcast::channel(16);
        let (iframe_tx, _) = tokio::sync::broadcast::channel(64);
        Self {
            bus: EventBus::new(),
            installed: RwLock::new(HashMap::new()),
            backend: RwLock::new(crate::backend::null_backend()),
            contributions: RwLock::new(Contributions::default()),
            plugin_contribs: RwLock::new(HashMap::new()),
            contrib_tx,
            iframe_tx,
            test_handlers: RwLock::new(HashMap::new()),
        }
    }

    /// Inject a real host-side backend (mute via pactl, OS notifications, etc.).
    /// The default `NullBackend` makes every call a no-op so headless harnesses
    /// keep working without OS plumbing.
    pub async fn set_backend(&self, backend: Arc<dyn crate::backend::HostBackend>) {
        *self.backend.write().await = backend;
    }

    pub fn inspect(&self, source: &Path) -> Result<InstallPreview, RegistryError> {
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
        let root = match std::env::var_os("TIDELINE_DEV_PLUGINS_DIR") {
            Some(p) => std::path::PathBuf::from(p),
            None => data_home().join("plugins"),
        };
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
            let backend = registry.backend.read().await.clone();
            let ctx = crate::dispatcher::HostContext {
                plugin_id: plugin_id.clone(),
                granted: plugin.granted.clone(),
                bus: registry.bus.clone(),
                backend,
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
                        reg2.evict_plugin_contributions(&pid2).await;
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
        self.evict_plugin_contributions(plugin_id).await;
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

    pub async fn installed_ids(&self) -> Vec<String> {
        self.installed.read().await.keys().cloned().collect()
    }

    pub async fn runtime(&self, plugin_id: &str) -> Option<Arc<PluginRuntime>> {
        let p = self.installed.read().await.get(plugin_id).cloned()?;
        let g = p.runtime.lock().await;
        g.clone()
    }

    pub async fn contributions(&self) -> Contributions {
        self.contributions.read().await.clone()
    }

    pub fn subscribe_contributions(&self) -> tokio::sync::broadcast::Receiver<Contributions> {
        self.contrib_tx.subscribe()
    }

    pub fn subscribe_iframe_messages(&self) -> tokio::sync::broadcast::Receiver<IframeMessage> {
        self.iframe_tx.subscribe()
    }

    /// Publish an iframe message to all subscribers (typically the Tauri layer
    /// which forwards it to the matching webview via window.postMessage).
    pub fn publish_iframe_message(&self, msg: IframeMessage) {
        let _ = self.iframe_tx.send(msg);
    }

    /// Replace the aggregated contributions and notify subscribers.
    /// Wave 3 will populate this from per-plugin contribution streams.
    pub async fn set_contributions(&self, c: Contributions) {
        *self.contributions.write().await = c.clone();
        let _ = self.contrib_tx.send(c);
    }

    /// Register a UI surface contribution for `plugin_id`.
    ///
    /// Parses `payload` into the appropriate typed contribution, stamps
    /// `plugin_id` from the host context (overriding any value the plugin
    /// supplied), and stores it in the per-plugin slot for `kind`.
    ///
    /// Replace-on-same-id semantics: if a contribution with the same
    /// surface_id (or item_id / action_id for tray and keybind) already
    /// exists for this plugin, it is replaced in place.
    ///
    /// After mutation, the global aggregate is recomputed and broadcast.
    pub async fn register_contribution(
        &self,
        plugin_id: &str,
        kind: ContribKind,
        mut payload: serde_json::Value,
    ) -> Result<(), RegistryError> {
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("plugin_id".into(), serde_json::Value::String(plugin_id.to_string()));
        } else {
            return Err(RegistryError::InvalidContribution(
                "payload must be a JSON object".into(),
            ));
        }
        {
            let mut map = self.plugin_contribs.write().await;
            let entry = map.entry(plugin_id.to_string()).or_default();
            match kind {
                ContribKind::SettingsSection => {
                    let c: SettingsSectionContribution = serde_json::from_value(payload)
                        .map_err(|e| RegistryError::InvalidContribution(e.to_string()))?;
                    if let Some(slot) = entry.settings_sections.iter_mut()
                        .find(|x| x.surface_id == c.surface_id)
                    { *slot = c; } else { entry.settings_sections.push(c); }
                }
                ContribKind::StatusPill => {
                    let c: StatusPillContribution = serde_json::from_value(payload)
                        .map_err(|e| RegistryError::InvalidContribution(e.to_string()))?;
                    if let Some(slot) = entry.status_pills.iter_mut()
                        .find(|x| x.surface_id == c.surface_id)
                    { *slot = c; } else { entry.status_pills.push(c); }
                }
                ContribKind::ChannelOverlay => {
                    let c: ChannelOverlayContribution = serde_json::from_value(payload)
                        .map_err(|e| RegistryError::InvalidContribution(e.to_string()))?;
                    if let Some(slot) = entry.channel_overlays.iter_mut()
                        .find(|x| x.surface_id == c.surface_id)
                    { *slot = c; } else { entry.channel_overlays.push(c); }
                }
                ContribKind::IframeSurface => {
                    let c: IframeSurface = serde_json::from_value(payload)
                        .map_err(|e| RegistryError::InvalidContribution(e.to_string()))?;
                    if let Some(slot) = entry.iframe_surfaces.iter_mut()
                        .find(|x| x.surface_id == c.surface_id)
                    { *slot = c; } else { entry.iframe_surfaces.push(c); }
                }
                ContribKind::TrayItem => {
                    let c: TrayItemContribution = serde_json::from_value(payload)
                        .map_err(|e| RegistryError::InvalidContribution(e.to_string()))?;
                    if let Some(slot) = entry.tray_items.iter_mut()
                        .find(|x| x.item_id == c.item_id)
                    { *slot = c; } else { entry.tray_items.push(c); }
                }
                ContribKind::KeybindAction => {
                    let c: KeybindActionContribution = serde_json::from_value(payload)
                        .map_err(|e| RegistryError::InvalidContribution(e.to_string()))?;
                    if let Some(slot) = entry.keybind_actions.iter_mut()
                        .find(|x| x.action_id == c.action_id)
                    { *slot = c; } else { entry.keybind_actions.push(c); }
                }
            }
        }
        self.recompute_and_broadcast().await;
        Ok(())
    }

    /// Remove a contribution by id from `plugin_id`'s slot for `kind`.
    /// `id` is the surface_id (or item_id / action_id for tray and keybind).
    pub async fn unregister_contribution(
        &self,
        plugin_id: &str,
        kind: ContribKind,
        id: &str,
    ) -> Result<(), RegistryError> {
        {
            let mut map = self.plugin_contribs.write().await;
            let Some(entry) = map.get_mut(plugin_id) else { return Ok(()) };
            match kind {
                ContribKind::SettingsSection => {
                    entry.settings_sections.retain(|x| x.surface_id != id);
                }
                ContribKind::StatusPill => {
                    entry.status_pills.retain(|x| x.surface_id != id);
                }
                ContribKind::ChannelOverlay => {
                    entry.channel_overlays.retain(|x| x.surface_id != id);
                }
                ContribKind::IframeSurface => {
                    entry.iframe_surfaces.retain(|x| x.surface_id != id);
                }
                ContribKind::TrayItem => {
                    entry.tray_items.retain(|x| x.item_id != id);
                }
                ContribKind::KeybindAction => {
                    entry.keybind_actions.retain(|x| x.action_id != id);
                }
            }
        }
        self.recompute_and_broadcast().await;
        Ok(())
    }

    /// Drop all contributions registered by `plugin_id` and rebroadcast the
    /// aggregate. Called when a plugin stops cleanly or its process exits.
    pub async fn evict_plugin_contributions(&self, plugin_id: &str) {
        let removed = {
            let mut map = self.plugin_contribs.write().await;
            map.remove(plugin_id).is_some()
        };
        if removed {
            self.recompute_and_broadcast().await;
        }
    }

    async fn recompute_and_broadcast(&self) {
        let merged = {
            let map = self.plugin_contribs.read().await;
            let mut merged = Contributions::default();
            for entry in map.values() {
                merged.settings_sections.extend(entry.settings_sections.iter().cloned());
                merged.status_pills.extend(entry.status_pills.iter().cloned());
                merged.channel_overlays.extend(entry.channel_overlays.iter().cloned());
                merged.iframe_surfaces.extend(entry.iframe_surfaces.iter().cloned());
                merged.tray_items.extend(entry.tray_items.iter().cloned());
                merged.keybind_actions.extend(entry.keybind_actions.iter().cloned());
            }
            merged
        };
        self.set_contributions(merged).await;
    }

    pub async fn send_request(
        &self,
        plugin_id: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, RegistryError> {
        if let Some(handler) = self.test_handlers.read().await.get(plugin_id).cloned() {
            return Ok(handler(method, &params));
        }
        let runtime = self.runtime(plugin_id).await
            .ok_or_else(|| RegistryError::NotInstalled(plugin_id.into()))?;
        let transport = runtime.transport().await
            .ok_or_else(|| RegistryError::NotInstalled(plugin_id.into()))?;
        match transport
            .call(method, Some(params), std::time::Duration::from_secs(30))
            .await
        {
            Ok(value) => Ok(value),
            Err(TransportError::Rpc(err)) => {
                Err(RegistryError::PluginError(format!("{}: {}", err.code, err.message)))
            }
            Err(e) => Err(RegistryError::Transport(e.to_string())),
        }
    }

    pub async fn dispatch_event(
        &self,
        plugin_id: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<(), RegistryError> {
        if let Some(handler) = self.test_handlers.read().await.get(plugin_id).cloned() {
            let _ = handler(method, &params);
            return Ok(());
        }
        let runtime = self.runtime(plugin_id).await
            .ok_or_else(|| RegistryError::NotInstalled(plugin_id.into()))?;
        let transport = runtime.transport().await
            .ok_or_else(|| RegistryError::NotInstalled(plugin_id.into()))?;
        transport
            .notify(method, Some(params))
            .await
            .map_err(|e| RegistryError::Transport(e.to_string()))?;
        Ok(())
    }

    pub fn new_for_test() -> Arc<Self> {
        Arc::new(Self::new())
    }

    pub async fn set_dispatch_for_test<F>(&self, plugin_id: &str, handler: F)
    where
        F: Fn(&str, &serde_json::Value) -> serde_json::Value + Send + Sync + 'static,
    {
        self.test_handlers
            .write()
            .await
            .insert(plugin_id.to_string(), Arc::new(handler));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[serial_test::serial]
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

    #[tokio::test]
    #[serial_test::serial]
    async fn discover_uses_dev_plugins_dir_env_override() {
        let dir = tempfile::tempdir().unwrap();
        let plugin_dir = dir.path().join("io.test");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(plugin_dir.join("tideline-plugin.toml"), r#"
[plugin]
schema = 1
id = "io.test"
name = "Test"
version = "0.1.0"
publisher = "Tideline"

[host]
api = "1.x"

[entry]
exec = "bin/test"

[capabilities]
required = []
"#).unwrap();
        std::env::set_var("TIDELINE_DEV_PLUGINS_DIR", dir.path());
        let reg = PluginRegistry::new();
        let count = reg.discover().await.unwrap();
        std::env::remove_var("TIDELINE_DEV_PLUGINS_DIR");
        assert_eq!(count, 1);
    }
}

