//! In-process audio engine: one JACK client per channel, ordered chain of
//! plugins per channel. All plugin work runs on the JACK callback thread.

// Engine accessor methods (sample_rate, parameters, channels_with_plugins,
// close_ui_for_slot, has_active_uis) are part of the engine's public surface
// and called from various places as the system evolves; keep them even when
// not currently referenced.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::Duration;

use anyhow::Context;
use parking_lot::Mutex;
use tracing::{info, warn};
use uuid::Uuid;

use crate::host::{FormatRegistry, ParamInfo, PluginInfo, PluginUi, UiController};
use crate::ui_bridge::PluginWindow;

mod channel;

pub use channel::{Channel, Slot};

/// One active LV2 (or other format) UI. Field order matters: the UI must be
/// freed before the X11 window it lives inside.
pub struct ActiveUi {
    pub ui: Box<dyn PluginUi>,
    #[allow(dead_code)]
    pub window: PluginWindow,
}

pub struct AudioEngine {
    formats: Arc<FormatRegistry>,
    channels: Mutex<HashMap<Uuid, Channel>>,
    active_uis: Mutex<HashMap<(Uuid, Uuid), ActiveUi>>,
    sample_rate: f64,
    buffer_size: u32,
    /// Debounced save trigger. Pinged from the UI write path
    /// (`set_param`) and from `tick_idle_uis` when a plugin window
    /// closes. EffectsState wires a tokio task on this Notify that
    /// runs `refresh_state_and_save` after a short debounce, so
    /// settings persist within a fraction of a second of the user
    /// moving a knob — no waiting for the 10s background tick.
    save_trigger: Arc<tokio::sync::Notify>,
}

/// Bridges UI-side parameter writes (suil → write_func) back into the engine's
/// authoritative parameter store.
struct EngineUiController {
    engine: Weak<AudioEngine>,
    channel_id: Uuid,
    slot_id: Uuid,
}

impl UiController for EngineUiController {
    fn write_param(&self, port_index: u32, value: f32) {
        if let Some(engine) = self.engine.upgrade() {
            if let Err(e) = engine.set_param(self.channel_id, self.slot_id, port_index, value) {
                tracing::debug!(?e, "ui write_param dropped");
            }
        }
    }
}

impl AudioEngine {
    pub fn new(formats: Arc<FormatRegistry>) -> anyhow::Result<Self> {
        let (probe, _status) =
            jack::Client::new("tideline-fx-probe", jack::ClientOptions::default())
                .context("failed to open jack probe client")?;
        let sample_rate = probe.sample_rate() as f64;
        let buffer_size = probe.buffer_size();
        drop(probe);
        info!(sample_rate, buffer_size, "audio engine ready");
        Ok(Self {
            formats,
            channels: Mutex::new(HashMap::new()),
            active_uis: Mutex::new(HashMap::new()),
            sample_rate,
            buffer_size,
            save_trigger: Arc::new(tokio::sync::Notify::new()),
        })
    }

    /// Handle that callers nudge to request a debounced save. State spawns
    /// the consuming task in `set_engine` so the engine itself stays
    /// agnostic about persistence.
    pub fn save_trigger(&self) -> Arc<tokio::sync::Notify> {
        self.save_trigger.clone()
    }

    fn nudge_save(&self) {
        self.save_trigger.notify_one();
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    pub fn buffer_size(&self) -> u32 {
        self.buffer_size
    }

    pub fn formats(&self) -> &FormatRegistry {
        &self.formats
    }

    pub fn ensure_channel(&self, uuid: Uuid, slug: &str) -> anyhow::Result<()> {
        let mut channels = self.channels.lock();
        if channels.contains_key(&uuid) {
            return Ok(());
        }
        let channel = Channel::open(uuid, slug, self.buffer_size)?;
        channels.insert(uuid, channel);
        Ok(())
    }

    pub fn remove_channel(&self, uuid: Uuid) {
        let mut channels = self.channels.lock();
        if let Some(ch) = channels.remove(&uuid) {
            drop(ch);
        }
    }

    pub fn add_plugin(
        &self,
        channel_id: Uuid,
        slot_id: Uuid,
        info: &PluginInfo,
        state: Option<&[u8]>,
    ) -> anyhow::Result<()> {
        let channels = self.channels.lock();
        let channel = channels
            .get(&channel_id)
            .ok_or_else(|| anyhow::anyhow!("channel {channel_id} not opened"))?;
        let mut plugin = self
            .formats
            .instantiate(info, self.sample_rate, self.buffer_size)?;
        if let Some(blob) = state {
            if let Err(e) = plugin.load_state(blob) {
                warn!(?e, plugin = %info.uri, "load_state failed; using defaults");
            }
        }
        // Drop any existing slot with this id before pushing the new one.
        // apply_persisted_chains can run twice in one boot (chains.json
        // load + pipewire_contributor first-call AppConfig sync). Without
        // this, both calls push duplicate Lv2Plugin instances; the audio
        // path then drives them in sequence and snapshot_all_states folds
        // them with HashMap::insert, last-write-wins. The stale instance
        // (loaded second from out-of-sync AppConfig) clobbers the live
        // one, and every subsequent UI param tweak gets overwritten on
        // the next save tick. Replacing in place keeps the invariant that
        // a slot id maps to exactly one plugin instance.
        if let Some(_old) = channel.remove_slot(slot_id) {
            tracing::debug!(?channel_id, ?slot_id, plugin = %info.uri, "add_plugin: replacing existing slot with same id");
        }
        channel.push_slot(Slot {
            id: slot_id,
            bypass: false,
            plugin,
        });
        Ok(())
    }

    pub fn remove_plugin(&self, channel_id: Uuid, slot_id: Uuid) -> anyhow::Result<()> {
        let channels = self.channels.lock();
        let channel = channels
            .get(&channel_id)
            .ok_or_else(|| anyhow::anyhow!("channel {channel_id} not opened"))?;
        channel.remove_slot(slot_id);
        Ok(())
    }

    pub fn reorder_chain(&self, channel_id: Uuid, order: &[Uuid]) -> anyhow::Result<()> {
        let channels = self.channels.lock();
        let channel = channels
            .get(&channel_id)
            .ok_or_else(|| anyhow::anyhow!("channel {channel_id} not opened"))?;
        channel.reorder(order);
        Ok(())
    }

    pub fn set_bypass(&self, channel_id: Uuid, slot_id: Uuid, bypass: bool) -> anyhow::Result<()> {
        let channels = self.channels.lock();
        let channel = channels
            .get(&channel_id)
            .ok_or_else(|| anyhow::anyhow!("channel {channel_id} not opened"))?;
        channel.set_slot_bypass(slot_id, bypass);
        Ok(())
    }

    pub fn set_chain_bypass(&self, channel_id: Uuid, bypass: bool) -> anyhow::Result<()> {
        let channels = self.channels.lock();
        let channel = channels
            .get(&channel_id)
            .ok_or_else(|| anyhow::anyhow!("channel {channel_id} not opened"))?;
        channel.set_chain_bypass(bypass);
        Ok(())
    }

    pub fn set_lowcut(&self, channel_id: Uuid, enabled: bool) -> anyhow::Result<()> {
        let channels = self.channels.lock();
        let channel = channels
            .get(&channel_id)
            .ok_or_else(|| anyhow::anyhow!("channel {channel_id} not opened"))?;
        channel.set_lowcut(enabled);
        Ok(())
    }

    pub fn set_clipguard(&self, channel_id: Uuid, enabled: bool) -> anyhow::Result<()> {
        let channels = self.channels.lock();
        let channel = channels
            .get(&channel_id)
            .ok_or_else(|| anyhow::anyhow!("channel {channel_id} not opened"))?;
        channel.set_clipguard(enabled);
        Ok(())
    }

    pub fn set_input_gain(&self, channel_id: Uuid, amp: f32) -> anyhow::Result<()> {
        let channels = self.channels.lock();
        let channel = channels
            .get(&channel_id)
            .ok_or_else(|| anyhow::anyhow!("channel {channel_id} not opened"))?;
        channel.set_input_gain(amp);
        Ok(())
    }

    pub fn set_param(
        &self,
        channel_id: Uuid,
        slot_id: Uuid,
        index: u32,
        value: f32,
    ) -> anyhow::Result<()> {
        let channels = self.channels.lock();
        let channel = channels
            .get(&channel_id)
            .ok_or_else(|| anyhow::anyhow!("channel {channel_id} not opened"))?;
        channel
            .with_slot_mut(slot_id, |slot| slot.plugin.set_param(index, value))
            .ok_or_else(|| anyhow::anyhow!("slot {slot_id} not in channel {channel_id}"))?;
        drop(channels);
        self.nudge_save();
        Ok(())
    }

    pub fn get_param(&self, channel_id: Uuid, slot_id: Uuid, index: u32) -> Option<f32> {
        let channels = self.channels.lock();
        let channel = channels.get(&channel_id)?;
        channel.with_slot(slot_id, |slot| slot.plugin.get_param(index))?
    }

    pub fn parameters(&self, channel_id: Uuid, slot_id: Uuid) -> Option<Vec<ParamInfo>> {
        let channels = self.channels.lock();
        let channel = channels.get(&channel_id)?;
        channel.with_slot(slot_id, |slot| slot.plugin.parameters().to_vec())
    }

    pub fn save_slot_state(&self, channel_id: Uuid, slot_id: Uuid) -> anyhow::Result<Vec<u8>> {
        let channels = self.channels.lock();
        let channel = channels
            .get(&channel_id)
            .ok_or_else(|| anyhow::anyhow!("channel {channel_id} not opened"))?;
        channel
            .with_slot(slot_id, |slot| slot.plugin.save_state())
            .ok_or_else(|| anyhow::anyhow!("slot {slot_id} not in channel {channel_id}"))?
    }

    pub fn snapshot_all_states(&self) -> HashMap<(Uuid, Uuid), Vec<u8>> {
        let channels = self.channels.lock();
        let mut out = HashMap::new();
        for (channel_id, channel) in channels.iter() {
            for (slot_id, blob) in channel.snapshot_states() {
                out.insert((*channel_id, slot_id), blob);
            }
        }
        out
    }

    pub fn channels_with_plugins(&self) -> Vec<Uuid> {
        let channels = self.channels.lock();
        channels
            .iter()
            .filter(|(_, c)| c.has_slots())
            .map(|(id, _)| *id)
            .collect()
    }

    pub fn show_ui_for_slot(
        self: &Arc<Self>,
        channel_id: Uuid,
        slot_id: Uuid,
        title: &str,
    ) -> anyhow::Result<()> {
        {
            let uis = self.active_uis.lock();
            if uis.contains_key(&(channel_id, slot_id)) {
                return Ok(());
            }
        }

        // Initial size is a placeholder; we resize to the widget's natural
        // size right after the LV2 UI is instantiated (most LV2 UIs are
        // 800–1100px wide). 1280x800 keeps any UI we've seen on-screen even
        // before the resize fires.
        let window = PluginWindow::open(title, 1280, 800).context("create plugin window")?;
        let parent = window.parent();

        let controller: Arc<dyn UiController> = Arc::new(EngineUiController {
            engine: Arc::downgrade(self),
            channel_id,
            slot_id,
        });

        let channels = self.channels.lock();
        let channel = channels
            .get(&channel_id)
            .ok_or_else(|| anyhow::anyhow!("channel {channel_id} not opened"))?;
        let ui_result = channel
            .with_slot_mut(slot_id, |slot| {
                slot.plugin.show_ui(parent, controller.clone())
            })
            .ok_or_else(|| anyhow::anyhow!("slot {slot_id} not in channel {channel_id}"))?;
        drop(channels);
        let ui = ui_result.context("plugin show_ui failed")?;

        // Resize parent window to match the embedded widget's natural size,
        // if the plugin reports one. Without this the UI gets clipped by the
        // 1280x800 default.
        if let Some(child_id) = ui.widget_window_id() {
            if let Some((w, h)) = window.query_child_size(child_id) {
                if w > 0 && h > 0 {
                    window.resize(w, h);
                }
            }
        }

        self.active_uis
            .lock()
            .insert((channel_id, slot_id), ActiveUi { ui, window });
        Ok(())
    }

    /// Close an open plugin UI, if one exists for the slot.
    pub fn close_ui_for_slot(&self, channel_id: Uuid, slot_id: Uuid) {
        let mut uis = self.active_uis.lock();
        uis.remove(&(channel_id, slot_id));
    }

    pub fn tick_idle_uis(&self) {
        let mut to_remove = Vec::new();
        {
            let mut uis = self.active_uis.lock();
            for (key, active) in uis.iter_mut() {
                // Drain xcb events. If the WM asked us to close (user clicked
                // the title-bar X), schedule removal — we can't drop while
                // holding the mutex iterator.
                if active.window.pump_events() {
                    to_remove.push(*key);
                    continue;
                }
                active.ui.idle();
            }
        }
        if !to_remove.is_empty() {
            // Move entries out of the map under the lock, then drop OUTSIDE
            // the lock and on a blocking thread. Drop is slow for DPF/Cairo
            // LV2 UIs because suil_instance_free tears down GL contexts and
            // PluginWindow::drop does a synchronous DestroyWindow round-trip
            // to the X server. Holding active_uis across that wedges every
            // other consumer (open_ui_for_slot, clear_state, the iframe
            // bridge's persist path) and stalls the rack dialog close.
            let mut removed = Vec::with_capacity(to_remove.len());
            {
                let mut uis = self.active_uis.lock();
                for key in to_remove {
                    if let Some(active) = uis.remove(&key) {
                        removed.push(active);
                    }
                }
            }
            if !removed.is_empty() {
                // User closed the plugin window — flush any pending param
                // tweaks that haven't been picked up by the periodic tick yet.
                self.nudge_save();
                tokio::task::spawn_blocking(move || drop(removed));
            }
        }
    }

    pub fn has_active_uis(&self) -> bool {
        !self.active_uis.lock().is_empty()
    }

    /// Drop every JACK-backed channel and every active plugin UI. Used when
    /// pipewire restarts under us — the old JACK clients are dead, so we must
    /// build fresh ones (callers reapply persisted state afterwards).
    pub fn clear_all_channels(&self) {
        // Drop UIs first; some hold references back into channels through the
        // controller, and freeing the X window before tearing down the JACK
        // client avoids surprising the audio thread mid-process.
        {
            let mut uis = self.active_uis.lock();
            uis.clear();
        }
        let mut channels = self.channels.lock();
        channels.clear();
        tracing::info!("AudioEngine::clear_all_channels: all channels and UIs dropped");
    }
}

/// Spawn a tokio task that ticks every open plugin UI at ~30Hz. The task
/// terminates when the AudioEngine is dropped.
pub fn spawn_ui_idle_pump(engine: Weak<AudioEngine>) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(33));
        loop {
            tick.tick().await;
            let Some(engine) = engine.upgrade() else {
                return;
            };
            engine.tick_idle_uis();
        }
    });
}
