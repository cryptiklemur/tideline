//! One JACK client per channel. The chain is shared between the engine
//! (mutates) and the JACK callback thread (reads). `parking_lot::Mutex::try_lock`
//! on the audio thread keeps mutation lock-free in steady state; if the lock
//! is held by the engine for a chain edit we passthrough that frame.

// Channel struct has `uuid`/`slug` fields and helper methods that aren't all
// called yet but document the channel's identity; keep them.
#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use anyhow::Context;
use parking_lot::Mutex;
use uuid::Uuid;

use crate::host::Plugin;

pub struct Slot {
    pub id: Uuid,
    pub bypass: bool,
    pub plugin: Box<dyn Plugin>,
}

pub struct Channel {
    pub uuid: Uuid,
    pub slug: String,
    chain: Arc<Mutex<Vec<Slot>>>,
    chain_bypass: Arc<AtomicBool>,
    lowcut: Arc<AtomicBool>,
    clipguard: Arc<AtomicBool>,
    /// Software input gain stored as linear amp factor. f32 bits packed
    /// into AtomicU32 so the audio thread reads it lock-free. 1.0 = unity.
    input_gain: Arc<AtomicU32>,
    _client: jack::AsyncClient<NotificationHandler, ProcessHandler>,
}

impl Channel {
    pub fn open(uuid: Uuid, slug: &str, buffer_size: u32) -> anyhow::Result<Self> {
        let name = format!("tideline-fx-{slug}");
        tracing::info!(target: "tideline-effects::engine", %name, %uuid, "Channel::open: creating jack client");
        let (client, status) = jack::Client::new(&name, jack::ClientOptions::default())
            .with_context(|| format!("failed to open jack client {name}"))?;
        tracing::info!(target: "tideline-effects::engine", %name, ?status, sample_rate = client.sample_rate(), buffer_size = client.buffer_size(), "jack client opened");
        let in_l = client.register_port("in_FL", jack::AudioIn::default())?;
        let in_r = client.register_port("in_FR", jack::AudioIn::default())?;
        let out_l = client.register_port("out_FL", jack::AudioOut::default())?;
        let out_r = client.register_port("out_FR", jack::AudioOut::default())?;

        let chain: Arc<Mutex<Vec<Slot>>> = Arc::new(Mutex::new(Vec::new()));
        let chain_bypass = Arc::new(AtomicBool::new(false));
        let lowcut = Arc::new(AtomicBool::new(false));
        let clipguard = Arc::new(AtomicBool::new(false));
        // Default to unity gain (1.0). f32::to_bits packs into u32 so the
        // audio thread reads via Relaxed atomic without fence cost.
        let input_gain = Arc::new(AtomicU32::new(1.0_f32.to_bits()));

        // RBJ HPF biquad at 80Hz, Q=0.707 (Butterworth slope, 12dB/oct).
        // Coefficients are computed once at jack-client open against the
        // current sample rate and reused across every process callback.
        let hpf_coefs = Biquad::hpf(80.0, 0.707, client.sample_rate() as f32);
        // Soft-clip ceiling at -1 dBFS ≈ 0.891251 linear amplitude.
        let clip_ceiling: f32 = 0.8912509;

        let scratch_size = buffer_size as usize;
        let process = ProcessHandler {
            chain: chain.clone(),
            chain_bypass: chain_bypass.clone(),
            lowcut: lowcut.clone(),
            clipguard: clipguard.clone(),
            input_gain: input_gain.clone(),
            in_l,
            in_r,
            out_l,
            out_r,
            scratch_a_l: vec![0.0; scratch_size],
            scratch_a_r: vec![0.0; scratch_size],
            scratch_b_l: vec![0.0; scratch_size],
            scratch_b_r: vec![0.0; scratch_size],
            hpf_coefs,
            hpf_l: BiquadState::default(),
            hpf_r: BiquadState::default(),
            clip_ceiling,
        };
        let active = client
            .activate_async(NotificationHandler, process)
            .with_context(|| format!("failed to activate jack client {name}"))?;
        tracing::info!(target: "tideline-effects::engine", %name, "jack client activated");

        Ok(Self {
            uuid,
            slug: slug.to_string(),
            chain,
            chain_bypass,
            lowcut,
            clipguard,
            input_gain,
            _client: active,
        })
    }

    pub fn push_slot(&self, slot: Slot) {
        self.chain.lock().push(slot);
    }

    pub fn remove_slot(&self, slot_id: Uuid) -> Option<Slot> {
        let mut chain = self.chain.lock();
        let idx = chain.iter().position(|s| s.id == slot_id)?;
        Some(chain.remove(idx))
    }

    pub fn reorder(&self, order: &[Uuid]) {
        let mut chain = self.chain.lock();
        let mut taken: Vec<Slot> = std::mem::take(&mut *chain);
        for id in order {
            if let Some(pos) = taken.iter().position(|s| s.id == *id) {
                chain.push(taken.remove(pos));
            }
        }
        chain.append(&mut taken);
    }

    pub fn set_slot_bypass(&self, slot_id: Uuid, bypass: bool) {
        let mut chain = self.chain.lock();
        if let Some(slot) = chain.iter_mut().find(|s| s.id == slot_id) {
            slot.bypass = bypass;
        }
    }

    pub fn set_chain_bypass(&self, bypass: bool) {
        self.chain_bypass.store(bypass, Ordering::Relaxed);
    }

    pub fn set_lowcut(&self, enabled: bool) {
        self.lowcut.store(enabled, Ordering::Relaxed);
    }

    pub fn set_clipguard(&self, enabled: bool) {
        self.clipguard.store(enabled, Ordering::Relaxed);
    }

    /// Set software input gain as a linear amplitude factor.
    /// 0.0 = silence, 1.0 = unity. Values clamped to [0.0, 8.0] (~+18dB).
    pub fn set_input_gain(&self, amp: f32) {
        let clamped = amp.clamp(0.0, 8.0);
        self.input_gain.store(clamped.to_bits(), Ordering::Relaxed);
    }

    pub fn with_slot<R>(&self, slot_id: Uuid, f: impl FnOnce(&Slot) -> R) -> Option<R> {
        let chain = self.chain.lock();
        let slot = chain.iter().find(|s| s.id == slot_id)?;
        Some(f(slot))
    }

    pub fn with_slot_mut<R>(&self, slot_id: Uuid, f: impl FnOnce(&mut Slot) -> R) -> Option<R> {
        let mut chain = self.chain.lock();
        let slot = chain.iter_mut().find(|s| s.id == slot_id)?;
        Some(f(slot))
    }

    pub fn has_slots(&self) -> bool {
        !self.chain.lock().is_empty()
    }

    pub fn snapshot_states(&self) -> Vec<(Uuid, Vec<u8>)> {
        let chain = self.chain.lock();
        chain
            .iter()
            .filter_map(|slot| match slot.plugin.save_state() {
                Ok(blob) => Some((slot.id, blob)),
                Err(_) => None,
            })
            .collect()
    }
}

pub struct NotificationHandler;
impl jack::NotificationHandler for NotificationHandler {}

pub struct ProcessHandler {
    chain: Arc<Mutex<Vec<Slot>>>,
    chain_bypass: Arc<AtomicBool>,
    lowcut: Arc<AtomicBool>,
    clipguard: Arc<AtomicBool>,
    input_gain: Arc<AtomicU32>,
    in_l: jack::Port<jack::AudioIn>,
    in_r: jack::Port<jack::AudioIn>,
    out_l: jack::Port<jack::AudioOut>,
    out_r: jack::Port<jack::AudioOut>,
    scratch_a_l: Vec<f32>,
    scratch_a_r: Vec<f32>,
    scratch_b_l: Vec<f32>,
    scratch_b_r: Vec<f32>,
    hpf_coefs: Biquad,
    hpf_l: BiquadState,
    hpf_r: BiquadState,
    clip_ceiling: f32,
}

impl jack::ProcessHandler for ProcessHandler {
    fn process(&mut self, _client: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        let in_l = self.in_l.as_slice(ps);
        let in_r = self.in_r.as_slice(ps);
        let frames = in_l.len();

        let gain = f32::from_bits(self.input_gain.load(Ordering::Relaxed));
        // Avoid mul-by-1 when slider is at unity. Cheap branch, common case.
        let apply_gain = (gain - 1.0).abs() > 1e-6;

        if self.chain_bypass.load(Ordering::Relaxed) {
            let out_l = self.out_l.as_mut_slice(ps);
            let out_r = self.out_r.as_mut_slice(ps);
            out_l[..frames].copy_from_slice(in_l);
            out_r[..frames].copy_from_slice(in_r);
            if apply_gain {
                for s in &mut out_l[..frames] {
                    *s *= gain;
                }
                for s in &mut out_r[..frames] {
                    *s *= gain;
                }
            }
            return jack::Control::Continue;
        }

        let lowcut_on = self.lowcut.load(Ordering::Relaxed);
        let clipguard_on = self.clipguard.load(Ordering::Relaxed);

        let mut chain = match self.chain.try_lock() {
            Some(c) => c,
            None => {
                // Engine is mutating the chain — passthrough this frame
                // but still respect lowcut/gain/clipguard so toggle-driven
                // cleanup never disappears mid-edit.
                let out_l = self.out_l.as_mut_slice(ps);
                let out_r = self.out_r.as_mut_slice(ps);
                out_l[..frames].copy_from_slice(in_l);
                out_r[..frames].copy_from_slice(in_r);
                if lowcut_on {
                    self.hpf_coefs
                        .process(&mut self.hpf_l, &mut out_l[..frames]);
                    self.hpf_coefs
                        .process(&mut self.hpf_r, &mut out_r[..frames]);
                }
                if apply_gain {
                    for s in &mut out_l[..frames] {
                        *s *= gain;
                    }
                    for s in &mut out_r[..frames] {
                        *s *= gain;
                    }
                }
                if clipguard_on {
                    soft_clip_inplace(&mut out_l[..frames], self.clip_ceiling);
                    soft_clip_inplace(&mut out_r[..frames], self.clip_ceiling);
                }
                return jack::Control::Continue;
            }
        };

        let a_l = &mut self.scratch_a_l[..frames];
        let a_r = &mut self.scratch_a_r[..frames];
        let b_l = &mut self.scratch_b_l[..frames];
        let b_r = &mut self.scratch_b_r[..frames];
        a_l.copy_from_slice(in_l);
        a_r.copy_from_slice(in_r);

        // Lowcut runs pre-chain so rumble/handling noise never reaches
        // compressors or eqs — they shouldn't waste headroom on signal
        // we'd cut anyway.
        if lowcut_on {
            self.hpf_coefs.process(&mut self.hpf_l, a_l);
            self.hpf_coefs.process(&mut self.hpf_r, a_r);
        }

        // Input gain runs post-lowcut, pre-chain. Acts as a software
        // input trim — feeds compressors / EQs at the level the user
        // wants regardless of hardware preamp behavior (e.g. Wave XLR
        // firmware clipguard plateau).
        if apply_gain {
            for s in a_l.iter_mut() {
                *s *= gain;
            }
            for s in a_r.iter_mut() {
                *s *= gain;
            }
        }

        if chain.is_empty() {
            let out_l = self.out_l.as_mut_slice(ps);
            let out_r = self.out_r.as_mut_slice(ps);
            out_l[..frames].copy_from_slice(a_l);
            out_r[..frames].copy_from_slice(a_r);
            if clipguard_on {
                soft_clip_inplace(&mut out_l[..frames], self.clip_ceiling);
                soft_clip_inplace(&mut out_r[..frames], self.clip_ceiling);
            }
            return jack::Control::Continue;
        }

        // Ping-pong scratch buffers: src → dst per active slot.
        let mut src_l: &mut [f32] = a_l;
        let mut src_r: &mut [f32] = a_r;
        let mut dst_l: &mut [f32] = b_l;
        let mut dst_r: &mut [f32] = b_r;
        let mut wrote_any = false;

        for slot in chain.iter_mut() {
            if slot.bypass {
                continue;
            }
            slot.plugin.process(src_l, src_r, dst_l, dst_r);
            std::mem::swap(&mut src_l, &mut dst_l);
            std::mem::swap(&mut src_r, &mut dst_r);
            wrote_any = true;
        }

        let out_l = self.out_l.as_mut_slice(ps);
        let out_r = self.out_r.as_mut_slice(ps);
        // Both the chain-output and chain-bypass branches resolve to copying
        // src_l/src_r into the output ports — wrote_any is reserved here for
        // future divergence (e.g. routing post-FX vs pre-FX) so leave the
        // flag in scope but do the copy unconditionally for now.
        let _ = wrote_any;
        out_l[..frames].copy_from_slice(src_l);
        out_r[..frames].copy_from_slice(src_r);

        // Clipguard runs post-chain — catches anything the chain pushed
        // hot. Soft tanh keeps signal under -1 dBFS without hard clipping.
        if clipguard_on {
            soft_clip_inplace(&mut out_l[..frames], self.clip_ceiling);
            soft_clip_inplace(&mut out_r[..frames], self.clip_ceiling);
        }
        jack::Control::Continue
    }
}

/// RBJ cookbook biquad. Holds the normalized coefficients; per-channel
/// state lives in `BiquadState` so a single coefficient set drives both
/// L and R filters without locking.
#[derive(Clone, Copy)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

#[derive(Default, Clone, Copy)]
pub struct BiquadState {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    pub fn hpf(f0: f32, q: f32, sample_rate: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * f0 / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);
        let a0 = 1.0 + alpha;
        let b0 = (1.0 + cos_w0) * 0.5 / a0;
        let b1 = -(1.0 + cos_w0) / a0;
        let b2 = (1.0 + cos_w0) * 0.5 / a0;
        let a1 = -2.0 * cos_w0 / a0;
        let a2 = (1.0 - alpha) / a0;
        Self { b0, b1, b2, a1, a2 }
    }

    #[inline]
    pub fn process(&self, st: &mut BiquadState, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            let x = *s;
            let y =
                self.b0 * x + self.b1 * st.x1 + self.b2 * st.x2 - self.a1 * st.y1 - self.a2 * st.y2;
            st.x2 = st.x1;
            st.x1 = x;
            st.y2 = st.y1;
            st.y1 = y;
            *s = y;
        }
    }
}

#[inline]
fn soft_clip_inplace(buf: &mut [f32], ceiling: f32) {
    // tanh-based soft knee. (x/c).tanh() * c stays under ±c smoothly,
    // adding harmonic distortion only when signal approaches the ceiling.
    let inv_c = 1.0 / ceiling;
    for s in buf.iter_mut() {
        *s = (*s * inv_c).tanh() * ceiling;
    }
}
