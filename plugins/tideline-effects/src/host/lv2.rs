//! LV2 host implementation backed by `livi`. Phase 1: audio + parameter state.
//! UI hosting (suil) is wired in Step 13.

use std::collections::HashMap;
use std::ffi::CStr;
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::Arc;

use livi::event::LV2AtomSequence;
use livi::{FeaturesBuilder, World};

use super::{
    Format, ParamInfo, ParentWindow, Plugin, PluginFormat, PluginInfo, PluginUi, UiController,
};

// LSP plugins push fat mesh atoms (spectrum, response curves) per block;
// 8KB overflowed and the UI never received the meshes. 64KB matches what
// LSP recommends in its own host.
const ATOM_BUFFER_BYTES: usize = 65_536;
pub(crate) const UI_ATOM_QUEUE_DEPTH: usize = 256;

/// One plugin → UI feedback event captured after `instance.run`. Forwarded
/// by `Lv2PluginUi::idle` to suil via `suil_instance_port_event`.
///
/// `Atom`: data contains the LV2_Atom header (size + type, 8 bytes) followed
/// by the body — exactly what suil's eventTransfer protocol expects.
/// `Control`: a single f32 value for a ControlOutput port; suil expects
/// format=0 + buffer_size=4 + buffer pointing at the float.
pub(crate) enum UiPluginEvent {
    Atom { port_index: u32, data: Vec<u8> },
    Control { port_index: u32, value: f32 },
}

/// One UI → plugin atom written by suil's write callback (in response to UI
/// interactions like subscribing to mesh outputs). The audio thread drains
/// these into the matching `atom_in` sequence on each block.
pub(crate) struct UiToPluginAtom {
    pub port_index: u32,
    /// Full LV2_Atom payload as suil hands it to us: 8-byte header (size +
    /// type, host endianness) followed by the body.
    pub data: Vec<u8>,
}

pub struct Lv2Format {
    world: Arc<World>,
    features: Arc<livi::Features>,
}

impl Lv2Format {
    pub fn new(max_block_length: usize) -> Self {
        let world = Arc::new(World::new());
        let features = world.build_features(FeaturesBuilder {
            min_block_length: 1,
            max_block_length,
        });
        Self { world, features }
    }
}

impl Default for Lv2Format {
    fn default() -> Self {
        Self::new(4096)
    }
}

impl PluginFormat for Lv2Format {
    fn name(&self) -> Format {
        Format::Lv2
    }

    fn scan(&self) -> Vec<PluginInfo> {
        self.world
            .iter_plugins()
            .filter_map(|p| {
                let counts = p.port_counts();
                if counts.audio_inputs == 0 || counts.audio_outputs == 0 {
                    return None;
                }
                let category = p.classes().next().unwrap_or("").to_string();
                Some(PluginInfo {
                    format: Format::Lv2,
                    uri: p.uri(),
                    name: p.name(),
                    vendor: String::new(),
                    category,
                    audio_inputs: counts.audio_inputs as u32,
                    audio_outputs: counts.audio_outputs as u32,
                    has_custom_ui: false,
                })
            })
            .collect()
    }

    fn instantiate(
        &self,
        info: &PluginInfo,
        sample_rate: f64,
        _max_block_size: u32,
    ) -> anyhow::Result<Box<dyn Plugin>> {
        let plugin = self
            .world
            .plugin_by_uri(&info.uri)
            .ok_or_else(|| anyhow::anyhow!("LV2 plugin {} not found", info.uri))?;

        // SAFETY: livi marks instantiate as unsafe because LV2 plugin code is
        // arbitrary native code. We trust LV2 plugins installed on the host
        // system at the same level we trust any other native dynamic library.
        let instance = unsafe { plugin.instantiate(self.features.clone(), sample_rate)? };

        let mut params = Vec::new();
        let mut symbol_to_index = HashMap::new();
        for port in plugin.ports() {
            if port.port_type != livi::PortType::ControlInput {
                continue;
            }
            let idx = port.index.0 as u32;
            symbol_to_index.insert(port.symbol.clone(), idx);
            params.push(ParamInfo {
                index: idx,
                symbol: port.symbol.clone(),
                name: port.name.clone(),
                default: port.default_value,
                min: port.min_value.unwrap_or(f32::NEG_INFINITY),
                max: port.max_value.unwrap_or(f32::INFINITY),
            });
        }

        let counts = *plugin.port_counts();
        let mut atom_in = Vec::with_capacity(counts.atom_sequence_inputs);
        for _ in 0..counts.atom_sequence_inputs {
            atom_in.push(LV2AtomSequence::new(&self.features, ATOM_BUFFER_BYTES));
        }
        let mut atom_out = Vec::with_capacity(counts.atom_sequence_outputs);
        for _ in 0..counts.atom_sequence_outputs {
            atom_out.push(LV2AtomSequence::new(&self.features, ATOM_BUFFER_BYTES));
        }

        // Parallel to atom_out: the LV2 port indices for each AtomSequenceOutput
        // port, in the same order as atom_out. Used to forward UI port events
        // and to control which port_index suil sees.
        let mut atom_notify_ports: Vec<u32> = Vec::with_capacity(atom_out.len());
        let mut atom_input_ports: Vec<u32> = Vec::with_capacity(atom_in.len());
        let mut control_output_ports: Vec<u32> = Vec::new();
        for port in plugin.ports() {
            match port.port_type {
                livi::PortType::AtomSequenceInput => {
                    atom_input_ports.push(port.index.0 as u32);
                }
                livi::PortType::AtomSequenceOutput => {
                    atom_notify_ports.push(port.index.0 as u32);
                }
                livi::PortType::ControlOutput => {
                    control_output_ports.push(port.index.0 as u32);
                }
                _ => {}
            }
        }

        let event_transfer_urid = self
            .features
            .urid(CStr::from_bytes_with_nul(b"http://lv2plug.in/ns/ext/atom#eventTransfer\0").unwrap());

        Ok(Box::new(Lv2Plugin {
            info: info.clone(),
            instance,
            params,
            symbol_to_index,
            atom_in,
            atom_out,
            atom_input_ports,
            atom_notify_ports,
            control_output_ports,
            event_transfer_urid,
            ui_event_tx: None,
            ui_atom_rx: None,
            plugin_handle: plugin,
            world: self.world.clone(),
            features: self.features.clone(),
        }))
    }
}

pub(crate) struct Lv2Plugin {
    pub(crate) info: PluginInfo,
    instance: livi::Instance,
    params: Vec<ParamInfo>,
    symbol_to_index: HashMap<String, u32>,
    atom_in: Vec<LV2AtomSequence>,
    atom_out: Vec<LV2AtomSequence>,
    /// LV2 port indices for each entry in `atom_in` (parallel arrays).
    atom_input_ports: Vec<u32>,
    /// LV2 port indices for each entry in `atom_out` (parallel arrays).
    atom_notify_ports: Vec<u32>,
    /// LV2 port indices of ControlOutput ports — values are snapshot every
    /// block and forwarded to the UI so meters track the audio thread.
    control_output_ports: Vec<u32>,
    /// URID for `http://lv2plug.in/ns/ext/atom#eventTransfer` — the protocol
    /// argument suil expects when forwarding atom events to the UI.
    pub(crate) event_transfer_urid: u32,
    /// Set by `lv2_ui::open` so the audio thread can hand events to the UI
    /// thread. Cleared on UI close (rx dropped → try_send returns Err).
    pub(crate) ui_event_tx: Option<SyncSender<UiPluginEvent>>,
    /// Receiver for atoms suil writes from the UI thread (mesh subscribe
    /// requests, etc.). Drained into `atom_in` at the top of every block.
    pub(crate) ui_atom_rx: Option<Receiver<UiToPluginAtom>>,
    pub(crate) plugin_handle: livi::Plugin,
    pub(crate) world: Arc<livi::World>,
    pub(crate) features: Arc<livi::Features>,
}

impl Plugin for Lv2Plugin {
    fn info(&self) -> &PluginInfo {
        &self.info
    }

    fn parameters(&self) -> &[ParamInfo] {
        &self.params
    }

    fn get_param(&self, index: u32) -> Option<f32> {
        self.instance.control_input(livi::PortIndex(index as usize))
    }

    fn set_param(&mut self, index: u32, value: f32) {
        self.instance
            .set_control_input(livi::PortIndex(index as usize), value);
    }

    fn save_state(&self) -> anyhow::Result<Vec<u8>> {
        let mut map: HashMap<String, f32> = HashMap::new();
        for param in &self.params {
            if let Some(v) = self
                .instance
                .control_input(livi::PortIndex(param.index as usize))
            {
                map.insert(param.symbol.clone(), v);
            }
        }
        Ok(serde_json::to_vec(&map)?)
    }

    fn load_state(&mut self, blob: &[u8]) -> anyhow::Result<()> {
        let map: HashMap<String, f32> = serde_json::from_slice(blob)?;
        for (sym, value) in map {
            if let Some(&idx) = self.symbol_to_index.get(&sym) {
                self.instance
                    .set_control_input(livi::PortIndex(idx as usize), value);
            }
        }
        Ok(())
    }

    fn process(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
    ) {
        // Drain any UI → plugin atoms (e.g. LSP mesh subscribe requests)
        // into the matching atom_in port BEFORE we run the plugin. Without
        // this the UI's subscription handshake never completes and analysis
        // outputs (filter response curves, FFT spectrum) stay empty.
        if let Some(rx) = self.ui_atom_rx.as_ref() {
            while let Ok(atom) = rx.try_recv() {
                let Some(in_idx) = self
                    .atom_input_ports
                    .iter()
                    .position(|&p| p == atom.port_index)
                else {
                    continue;
                };
                if atom.data.len() < 8 {
                    continue;
                }
                let size = u32::from_ne_bytes(atom.data[0..4].try_into().unwrap()) as usize;
                let type_urid = u32::from_ne_bytes(atom.data[4..8].try_into().unwrap());
                if atom.data.len() < 8 + size {
                    continue;
                }
                let body = &atom.data[8..8 + size];
                // 4 KiB per UI → plugin atom is plenty for LSP-style Object
                // requests; subscription messages are typically <200 bytes.
                match livi::event::LV2AtomEventBuilder::<4096>::new(0, type_urid, body) {
                    Ok(event) => {
                        if let Err(e) = self.atom_in[in_idx].push_event(&event) {
                            tracing::trace!(error = ?e, "atom_in push failed");
                        }
                    }
                    Err(e) => {
                        tracing::trace!(error = ?e, "ui→plugin atom too large");
                    }
                }
            }
        }

        let frames = out_l.len();
        let n_in = self.info.audio_inputs as usize;
        let n_out = self.info.audio_outputs as usize;
        let audio_in: [&[f32]; 2] = [in_l, in_r];
        let mut audio_out_arr: [&mut [f32]; 2] = [out_l, out_r];

        let ports = livi::PortConnections {
            audio_inputs: audio_in.iter().copied().take(n_in),
            audio_outputs: audio_out_arr
                .iter_mut()
                .map(|b| &mut **b)
                .take(n_out),
            atom_sequence_inputs: self.atom_in.iter(),
            atom_sequence_outputs: self.atom_out.iter_mut(),
            cv_inputs: std::iter::empty::<&[f32]>(),
            cv_outputs: std::iter::empty::<&mut [f32]>(),
        };

        // SAFETY: same justification as instantiate; the plugin code we run
        // is whatever the user has installed on the host.
        if let Err(e) = unsafe { self.instance.run(frames, ports) } {
            tracing::warn!(error = ?e, plugin = %self.info.uri, "lv2 run failed");
        }
        // Mono plugin in a stereo chain: only out_l was written above.
        // Duplicate it to out_r so downstream effects (and the channel
        // process callback's mix loopbacks) see signal on both channels
        // instead of stale scratch data.
        if n_out == 1 {
            out_r[..frames].copy_from_slice(&out_l[..frames]);
        }
        // Forward plugin → UI feedback (atoms + control outputs) so suil can
        // drive meters and graphs. try_send keeps the audio thread non-
        // blocking — slow UIs drop events rather than stall the callback.
        if let Some(tx) = self.ui_event_tx.as_ref() {
            for (out_idx, seq) in self.atom_out.iter().enumerate() {
                let port_index = self.atom_notify_ports[out_idx];
                for ev in seq.iter() {
                    let body_size = ev.event.body.size as usize;
                    let mut buf: Vec<u8> = Vec::with_capacity(8 + body_size);
                    // LV2_Atom header: u32 size + u32 type (host endianness).
                    buf.extend_from_slice(&ev.event.body.size.to_ne_bytes());
                    buf.extend_from_slice(&ev.event.body.mytype.to_ne_bytes());
                    buf.extend_from_slice(ev.data);
                    let _ = tx.try_send(UiPluginEvent::Atom {
                        port_index,
                        data: buf,
                    });
                }
            }
            for &port_index in &self.control_output_ports {
                if let Some(value) = self
                    .instance
                    .control_output(livi::PortIndex(port_index as usize))
                {
                    let _ = tx.try_send(UiPluginEvent::Control { port_index, value });
                }
            }
        }
        // atom_in must be cleared too — events we pushed for this block
        // have been consumed by the plugin and shouldn't replay next block.
        for seq in &mut self.atom_in {
            seq.clear();
        }
        for seq in &mut self.atom_out {
            seq.clear();
        }
    }

    fn show_ui(
        &mut self,
        parent: ParentWindow,
        controller: Arc<dyn UiController>,
    ) -> anyhow::Result<Box<dyn PluginUi>> {
        crate::host::lv2_ui::open(self, parent, controller)
    }
}
