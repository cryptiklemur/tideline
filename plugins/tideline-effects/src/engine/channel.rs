//! One JACK client per channel. The chain is shared between the engine
//! (mutates) and the JACK callback thread (reads). `parking_lot::Mutex::try_lock`
//! on the audio thread keeps mutation lock-free in steady state; if the lock
//! is held by the engine for a chain edit we passthrough that frame.

// Channel struct has `uuid`/`slug` fields and helper methods that aren't all
// called yet but document the channel's identity; keep them.
#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};
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

        let scratch_size = buffer_size as usize;
        let process = ProcessHandler {
            chain: chain.clone(),
            chain_bypass: chain_bypass.clone(),
            in_l,
            in_r,
            out_l,
            out_r,
            scratch_a_l: vec![0.0; scratch_size],
            scratch_a_r: vec![0.0; scratch_size],
            scratch_b_l: vec![0.0; scratch_size],
            scratch_b_r: vec![0.0; scratch_size],
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
    in_l: jack::Port<jack::AudioIn>,
    in_r: jack::Port<jack::AudioIn>,
    out_l: jack::Port<jack::AudioOut>,
    out_r: jack::Port<jack::AudioOut>,
    scratch_a_l: Vec<f32>,
    scratch_a_r: Vec<f32>,
    scratch_b_l: Vec<f32>,
    scratch_b_r: Vec<f32>,
}

impl jack::ProcessHandler for ProcessHandler {
    fn process(&mut self, _client: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        let in_l = self.in_l.as_slice(ps);
        let in_r = self.in_r.as_slice(ps);
        let frames = in_l.len();

        if self.chain_bypass.load(Ordering::Relaxed) {
            self.out_l.as_mut_slice(ps)[..frames].copy_from_slice(in_l);
            self.out_r.as_mut_slice(ps)[..frames].copy_from_slice(in_r);
            return jack::Control::Continue;
        }

        let mut chain = match self.chain.try_lock() {
            Some(c) => c,
            None => {
                self.out_l.as_mut_slice(ps)[..frames].copy_from_slice(in_l);
                self.out_r.as_mut_slice(ps)[..frames].copy_from_slice(in_r);
                return jack::Control::Continue;
            }
        };

        if chain.is_empty() {
            self.out_l.as_mut_slice(ps)[..frames].copy_from_slice(in_l);
            self.out_r.as_mut_slice(ps)[..frames].copy_from_slice(in_r);
            return jack::Control::Continue;
        }

        let a_l = &mut self.scratch_a_l[..frames];
        let a_r = &mut self.scratch_a_r[..frames];
        let b_l = &mut self.scratch_b_l[..frames];
        let b_r = &mut self.scratch_b_r[..frames];
        a_l.copy_from_slice(in_l);
        a_r.copy_from_slice(in_r);

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
        if wrote_any {
            out_l[..frames].copy_from_slice(src_l);
            out_r[..frames].copy_from_slice(src_r);
        } else {
            out_l[..frames].copy_from_slice(in_l);
            out_r[..frames].copy_from_slice(in_r);
        }
        jack::Control::Continue
    }
}
