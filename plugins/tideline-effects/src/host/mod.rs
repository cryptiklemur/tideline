//! Plugin host abstraction. Format-specific code (LV2, VST3, VST2) lives behind
//! the [`Plugin`] / [`PluginFormat`] traits so the engine, chain ops, and
//! discovery layer never see format details.

// Trait + DTO surface; some methods/fields aren't called yet but are part of
// the documented host API and intentionally kept.
#![allow(dead_code)]

use std::fmt;
use std::sync::Arc;

pub mod lv2;
pub(crate) mod lv2_ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Lv2,
    Vst2,
    Vst3,
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Format::Lv2 => f.write_str("lv2"),
            Format::Vst2 => f.write_str("vst2"),
            Format::Vst3 => f.write_str("vst3"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PluginInfo {
    pub format: Format,
    pub uri: String,
    pub name: String,
    pub vendor: String,
    pub category: String,
    pub audio_inputs: u32,
    pub audio_outputs: u32,
    pub has_custom_ui: bool,
}

#[derive(Debug, Clone)]
pub struct ParamInfo {
    pub index: u32,
    pub symbol: String,
    pub name: String,
    pub default: f32,
    pub min: f32,
    pub max: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct ParentWindow {
    pub x11_window: u32,
}

pub trait Plugin: Send {
    fn info(&self) -> &PluginInfo;
    fn parameters(&self) -> &[ParamInfo];
    fn get_param(&self, index: u32) -> Option<f32>;
    fn set_param(&mut self, index: u32, value: f32);
    fn save_state(&self) -> anyhow::Result<Vec<u8>>;
    fn load_state(&mut self, blob: &[u8]) -> anyhow::Result<()>;
    fn process(&mut self, in_l: &[f32], in_r: &[f32], out_l: &mut [f32], out_r: &mut [f32]);
    fn show_ui(
        &mut self,
        parent: ParentWindow,
        controller: Arc<dyn UiController>,
    ) -> anyhow::Result<Box<dyn PluginUi>>;
}

pub trait PluginUi: Send {
    fn idle(&mut self);
    fn resize(&mut self, width: u32, height: u32);
    /// X11 window id of the embedded plugin widget (the child of the parent
    /// we handed to the plugin). Used by the host to query the UI's natural
    /// size and grow the parent window to fit.
    fn widget_window_id(&self) -> Option<u32> {
        None
    }
}

/// Back-channel from the plugin UI into the host. The plugin's UI thread
/// invokes [`UiController::write_param`] when the user manipulates a control
/// — implementations route the write back into the audio engine so the
/// matching slot's parameter actually updates.
pub trait UiController: Send + Sync {
    fn write_param(&self, port_index: u32, value: f32);
}

pub trait PluginFormat: Send + Sync {
    fn name(&self) -> Format;
    fn scan(&self) -> Vec<PluginInfo>;
    fn instantiate(
        &self,
        info: &PluginInfo,
        sample_rate: f64,
        max_block_size: u32,
    ) -> anyhow::Result<Box<dyn Plugin>>;
    /// Rebuild whatever internal index this format uses to enumerate
    /// plugins. Called by the rack rescan path so newly installed
    /// plugins (after app start) become visible without restarting.
    /// Default is a no-op for formats whose `scan()` re-walks disk
    /// every call.
    fn refresh(&self) {}
}

pub struct FormatRegistry {
    formats: Vec<Arc<dyn PluginFormat>>,
}

impl FormatRegistry {
    pub fn new() -> Self {
        Self {
            formats: Vec::new(),
        }
    }

    pub fn register(&mut self, format: Arc<dyn PluginFormat>) {
        self.formats.push(format);
    }

    pub fn formats(&self) -> &[Arc<dyn PluginFormat>] {
        &self.formats
    }

    pub fn find(&self, format: Format) -> Option<&Arc<dyn PluginFormat>> {
        self.formats.iter().find(|f| f.name() == format)
    }

    pub fn scan_all(&self) -> Vec<PluginInfo> {
        let mut out = Vec::new();
        for f in &self.formats {
            out.extend(f.scan());
        }
        out
    }

    /// Tell every format to rebuild its plugin index. Cheap for formats
    /// that already re-walk on `scan()`; heavier for LV2 where lilv's
    /// `World` is built once at construction and otherwise stays stale.
    pub fn refresh_all(&self) {
        for f in &self.formats {
            f.refresh();
        }
    }

    pub fn instantiate(
        &self,
        info: &PluginInfo,
        sample_rate: f64,
        max_block_size: u32,
    ) -> anyhow::Result<Box<dyn Plugin>> {
        let format = self
            .find(info.format)
            .ok_or_else(|| anyhow::anyhow!("format {:?} not registered", info.format))?;
        format.instantiate(info, sample_rate, max_block_size)
    }
}

impl Default for FormatRegistry {
    fn default() -> Self {
        Self::new()
    }
}
