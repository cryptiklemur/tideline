use std::sync::Arc;
use std::time::Duration;
use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;
use crate::transport::{SdkTransportError, StdioTransport};
use crate::types::AudioSource;

/// Abstraction over the JSON-RPC transport so `HostClient` can be unit-tested
/// without spawning the real stdin/stdout transport.
#[async_trait]
pub trait HostRpc: Send + Sync {
    async fn call(&self, method: &str, params: Option<Value>, timeout: Duration)
        -> Result<Value, SdkTransportError>;
}

#[async_trait]
impl HostRpc for StdioTransport {
    async fn call(&self, method: &str, params: Option<Value>, timeout: Duration)
        -> Result<Value, SdkTransportError>
    {
        StdioTransport::call(self, method, params, timeout).await
    }
}

pub struct HostClient {
    transport: Arc<dyn HostRpc>,
}

impl HostClient {
    pub fn new(transport: Arc<StdioTransport>) -> Self {
        Self { transport: transport as Arc<dyn HostRpc> }
    }

    /// Construct a client backed by an arbitrary `HostRpc` implementation.
    /// Useful for tests that want to intercept method/payload pairs.
    pub fn with_rpc(transport: Arc<dyn HostRpc>) -> Self {
        Self { transport }
    }

    pub async fn initialize(&self, plugin_id: &str, version: &str, sdk_version: &str)
        -> Result<Value, SdkTransportError>
    {
        self.transport.call(
            "host/initialize",
            Some(json!({"plugin_id": plugin_id, "version": version, "sdk_version": sdk_version})),
            Duration::from_secs(5),
        ).await
    }

    pub async fn log_write(&self, level: &str, message: &str) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/log.write",
            Some(json!({"level": level, "message": message})),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    pub async fn event_subscribe(&self, topic: &str) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/event.subscribe",
            Some(json!({"topic": topic})),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    pub async fn event_publish(&self, topic: &str, params: Value) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/event.publish",
            Some(json!({"topic": topic, "params": params})),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    pub async fn channel_list(&self) -> Result<Value, SdkTransportError> {
        self.transport.call("host/channel.list", Some(json!({})), Duration::from_secs(2)).await
    }

    pub async fn notify(&self, title: &str, body: &str) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/notify",
            Some(json!({"title": title, "body": body})),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }


    pub async fn list_input_sources(&self) -> Result<Vec<AudioSource>, SdkTransportError> {
        let v = self.transport.call(
            "host/sources.list",
            Some(json!({})),
            Duration::from_secs(5),
        ).await?;
        let sources = v.get("sources").cloned().unwrap_or(json!([]));
        serde_json::from_value(sources).map_err(|e| SdkTransportError::Decode(e.to_string()))
    }

    pub async fn audio_play_b64(&self, audio_b64: &str) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/audio.play",
            Some(json!({"audio_b64": audio_b64})),
            Duration::from_secs(5),
        ).await.map(|_| ())
    }

    pub async fn config_namespace_get(&self, namespace: &str) -> Result<Value, SdkTransportError> {
        self.transport.call(
            "host/config.namespace.get",
            Some(json!({"namespace": namespace})),
            Duration::from_secs(2),
        ).await
    }

    pub async fn config_namespace_set(&self, namespace: &str, value: Value) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/config.namespace.set",
            Some(json!({"namespace": namespace, "value": value})),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    /// Ask the host to focus a channel overlay surface (e.g. open the rack iframe
    /// when the sidebar badge is clicked). Payload typically includes `surface_id`
    /// and `channel_uuid`.
    pub async fn ui_channel_overlay_focus(&self, payload: Value) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/ui.channel_overlay.focus",
            Some(payload),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    /// Send a typed message to a plugin-owned iframe surface. Payload must include
    /// `surface_id` and the serialized OutboundMsg under `message`.
    pub async fn ui_iframe_send(&self, surface_id: &str, message: Value) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/ui.iframe.send",
            Some(json!({"surface_id": surface_id, "message": message})),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    pub async fn channel_attach_data(&self, channel_uuid: Uuid, data: Value) -> Result<(), SdkTransportError> {
        // 10s timeout: the host serializes plugin_data writes through the
        // AppConfig mutex and a sync save_config_to_disk call. Under
        // contention from other plugins or the pipewire rebuild path, 2s was
        // not enough and persist_channel was timing out every 10s tick.
        self.transport.call(
            "host/channel.attach_data",
            Some(json!({"channel_uuid": channel_uuid, "data": data})),
            Duration::from_secs(10),
        ).await.map(|_| ())
    }

    pub async fn settings_section_render(&self, section_id: &str, tree: Value) -> Result<(), SdkTransportError> {
        eprintln!("SDK: settings_section_render called with section_id={}", section_id);
        self.transport.call(
            "plugin/settings.section.render",
            Some(json!({"surface_id": section_id, "tree": tree})),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    /// Register a settings section UI surface with the host.
    ///
    /// `payload` is a JSON object matching `SettingsSectionContribution` with
    /// fields: `surface_id` (string, required), `title` (string, required),
    /// `icon` (optional), `priority` (i32, optional, default 0),
    /// `tree` (UI tree object). The `plugin_id` field is stamped by the host.
    pub async fn register_settings_section(&self, payload: Value) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/contributions.register_settings_section",
            Some(payload),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    /// Unregister a previously-registered settings section by its `surface_id`.
    pub async fn unregister_settings_section(&self, surface_id: &str) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/contributions.unregister_settings_section",
            Some(json!({"surface_id": surface_id})),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    /// Register a status pill UI surface.
    ///
    /// `payload` is a JSON object matching `StatusPillContribution`:
    /// `surface_id` (required), `label` (required), `tone` (optional),
    /// `icon` (optional), `priority` (optional), `tooltip` (optional).
    /// The `plugin_id` field is stamped by the host.
    pub async fn register_status_pill(&self, payload: Value) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/contributions.register_status_pill",
            Some(payload),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    /// Unregister a previously-registered status pill by its `surface_id`.
    pub async fn unregister_status_pill(&self, surface_id: &str) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/contributions.unregister_status_pill",
            Some(json!({"surface_id": surface_id})),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    /// Register a channel overlay UI surface.
    ///
    /// `payload` is a JSON object matching `ChannelOverlayContribution`:
    /// `surface_id` (required), `placement` (one of "detail" / "sidebar_badge" / "header_chip"),
    /// `channel_filter` (`{"kind": "all"}` or `{"kind": "channel_ids", "ids": [...]}`),
    /// `tree` (UI tree object). The `plugin_id` field is stamped by the host.
    pub async fn register_channel_overlay(&self, payload: Value) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/contributions.register_channel_overlay",
            Some(payload),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    /// Unregister a previously-registered channel overlay by its `surface_id`.
    pub async fn unregister_channel_overlay(&self, surface_id: &str) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/contributions.unregister_channel_overlay",
            Some(json!({"surface_id": surface_id})),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    /// Register an iframe UI surface.
    ///
    /// `payload` is a JSON object matching `IframeSurface`:
    /// `surface_id` (required), `entry_path` (required), `initial_data` (optional).
    /// The `plugin_id` field is stamped by the host.
    pub async fn register_iframe_surface(&self, payload: Value) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/contributions.register_iframe_surface",
            Some(payload),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    /// Unregister a previously-registered iframe surface by its `surface_id`.
    pub async fn unregister_iframe_surface(&self, surface_id: &str) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/contributions.unregister_iframe_surface",
            Some(json!({"surface_id": surface_id})),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    /// Register a tray menu item.
    ///
    /// `payload` is a JSON object matching `TrayItemContribution`:
    /// `item_id` (required), `label` (required), `accelerator` (optional),
    /// `icon` (optional), `priority` (optional). The `plugin_id` field is stamped by the host.
    pub async fn register_tray_item(&self, payload: Value) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/contributions.register_tray_item",
            Some(payload),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    /// Unregister a previously-registered tray item by its `item_id`.
    pub async fn unregister_tray_item(&self, item_id: &str) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/contributions.unregister_tray_item",
            Some(json!({"item_id": item_id})),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    /// Register a keybind action.
    ///
    /// `payload` is a JSON object matching `KeybindActionContribution`:
    /// `action_id` (required), `label` (required). The `plugin_id` field is stamped by the host.
    pub async fn register_keybind_action(&self, payload: Value) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/contributions.register_keybind_action",
            Some(payload),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    /// Unregister a previously-registered keybind action by its `action_id`.
    pub async fn unregister_keybind_action(&self, action_id: &str) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/contributions.unregister_keybind_action",
            Some(json!({"action_id": action_id})),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    /// Register an input overlay UI surface.
    ///
    /// `payload` matches `InputOverlayContribution`: `surface_id`,
    /// `input_filter` (`{"kind": "all"}` / `{"kind": "physical_only"}` /
    /// `{"kind": "source_names", "names": [...]}`), `tree`. `plugin_id` is
    /// stamped by the host.
    pub async fn register_input_overlay(&self, payload: Value) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/contributions.register_input_overlay",
            Some(payload),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    pub async fn unregister_input_overlay(&self, surface_id: &str) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/contributions.unregister_input_overlay",
            Some(json!({"surface_id": surface_id})),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    pub async fn call_raw(&self, method: &str, params: Option<Value>, timeout: Duration)
        -> Result<Value, SdkTransportError>
    {
        self.transport.call(method, params, timeout).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Spy {
        calls: Mutex<Vec<(String, Option<Value>)>>,
    }

    #[async_trait]
    impl HostRpc for Spy {
        async fn call(&self, method: &str, params: Option<Value>, _timeout: Duration)
            -> Result<Value, SdkTransportError>
        {
            self.calls.lock().unwrap().push((method.to_string(), params));
            Ok(Value::Null)
        }
    }

    fn client_with_spy() -> (HostClient, Arc<Spy>) {
        let spy: Arc<Spy> = Arc::new(Spy::default());
        let client = HostClient::with_rpc(spy.clone() as Arc<dyn HostRpc>);
        (client, spy)
    }

    #[tokio::test]
    async fn register_settings_section_sends_correct_method_and_payload() {
        let (c, spy) = client_with_spy();
        let payload = json!({"surface_id": "main", "title": "Hi", "tree": {}});
        c.register_settings_section(payload.clone()).await.unwrap();
        let calls = spy.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "host/contributions.register_settings_section");
        assert_eq!(calls[0].1, Some(payload));
    }

    #[tokio::test]
    async fn unregister_settings_section_sends_surface_id() {
        let (c, spy) = client_with_spy();
        c.unregister_settings_section("main").await.unwrap();
        let calls = spy.calls.lock().unwrap().clone();
        assert_eq!(calls[0].0, "host/contributions.unregister_settings_section");
        assert_eq!(calls[0].1, Some(json!({"surface_id": "main"})));
    }

    #[tokio::test]
    async fn register_status_pill_sends_correct_method_and_payload() {
        let (c, spy) = client_with_spy();
        let payload = json!({"surface_id": "p", "label": "L"});
        c.register_status_pill(payload.clone()).await.unwrap();
        let calls = spy.calls.lock().unwrap().clone();
        assert_eq!(calls[0].0, "host/contributions.register_status_pill");
        assert_eq!(calls[0].1, Some(payload));
    }

    #[tokio::test]
    async fn unregister_status_pill_sends_surface_id() {
        let (c, spy) = client_with_spy();
        c.unregister_status_pill("p").await.unwrap();
        let calls = spy.calls.lock().unwrap().clone();
        assert_eq!(calls[0].0, "host/contributions.unregister_status_pill");
        assert_eq!(calls[0].1, Some(json!({"surface_id": "p"})));
    }

    #[tokio::test]
    async fn register_channel_overlay_sends_correct_method_and_payload() {
        let (c, spy) = client_with_spy();
        let payload = json!({
            "surface_id": "o",
            "placement": "detail",
            "channel_filter": {"kind": "all"},
            "tree": {}
        });
        c.register_channel_overlay(payload.clone()).await.unwrap();
        let calls = spy.calls.lock().unwrap().clone();
        assert_eq!(calls[0].0, "host/contributions.register_channel_overlay");
        assert_eq!(calls[0].1, Some(payload));
    }

    #[tokio::test]
    async fn unregister_channel_overlay_sends_surface_id() {
        let (c, spy) = client_with_spy();
        c.unregister_channel_overlay("o").await.unwrap();
        let calls = spy.calls.lock().unwrap().clone();
        assert_eq!(calls[0].0, "host/contributions.unregister_channel_overlay");
        assert_eq!(calls[0].1, Some(json!({"surface_id": "o"})));
    }

    #[tokio::test]
    async fn register_iframe_surface_sends_correct_method_and_payload() {
        let (c, spy) = client_with_spy();
        let payload = json!({"surface_id": "f", "entry_path": "ui/index.html"});
        c.register_iframe_surface(payload.clone()).await.unwrap();
        let calls = spy.calls.lock().unwrap().clone();
        assert_eq!(calls[0].0, "host/contributions.register_iframe_surface");
        assert_eq!(calls[0].1, Some(payload));
    }

    #[tokio::test]
    async fn unregister_iframe_surface_sends_surface_id() {
        let (c, spy) = client_with_spy();
        c.unregister_iframe_surface("f").await.unwrap();
        let calls = spy.calls.lock().unwrap().clone();
        assert_eq!(calls[0].0, "host/contributions.unregister_iframe_surface");
        assert_eq!(calls[0].1, Some(json!({"surface_id": "f"})));
    }

    #[tokio::test]
    async fn register_tray_item_sends_correct_method_and_payload() {
        let (c, spy) = client_with_spy();
        let payload = json!({"item_id": "i", "label": "Q"});
        c.register_tray_item(payload.clone()).await.unwrap();
        let calls = spy.calls.lock().unwrap().clone();
        assert_eq!(calls[0].0, "host/contributions.register_tray_item");
        assert_eq!(calls[0].1, Some(payload));
    }

    #[tokio::test]
    async fn unregister_tray_item_sends_item_id() {
        let (c, spy) = client_with_spy();
        c.unregister_tray_item("i").await.unwrap();
        let calls = spy.calls.lock().unwrap().clone();
        assert_eq!(calls[0].0, "host/contributions.unregister_tray_item");
        assert_eq!(calls[0].1, Some(json!({"item_id": "i"})));
    }

    #[tokio::test]
    async fn register_keybind_action_sends_correct_method_and_payload() {
        let (c, spy) = client_with_spy();
        let payload = json!({"action_id": "a", "label": "X"});
        c.register_keybind_action(payload.clone()).await.unwrap();
        let calls = spy.calls.lock().unwrap().clone();
        assert_eq!(calls[0].0, "host/contributions.register_keybind_action");
        assert_eq!(calls[0].1, Some(payload));
    }

    #[tokio::test]
    async fn unregister_keybind_action_sends_action_id() {
        let (c, spy) = client_with_spy();
        c.unregister_keybind_action("a").await.unwrap();
        let calls = spy.calls.lock().unwrap().clone();
        assert_eq!(calls[0].0, "host/contributions.unregister_keybind_action");
        assert_eq!(calls[0].1, Some(json!({"action_id": "a"})));
    }
}
