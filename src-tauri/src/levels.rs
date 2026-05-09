use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use libpulse_binding::def::BufferAttr;
use libpulse_binding::sample::{Format, Spec};
use libpulse_binding::stream::Direction;
use libpulse_simple_binding::Simple;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Clone)]
struct LevelEvent {
    source: String,
    peak: f32,
    rms: f32,
}

pub struct LevelMonitor {
    workers: Mutex<HashMap<String, Arc<AtomicBool>>>,
    app: AppHandle,
}

impl LevelMonitor {
    pub fn new(app: AppHandle) -> Self {
        Self {
            workers: Mutex::new(HashMap::new()),
            app,
        }
    }

    pub fn watch(&self, sources: Vec<String>) {
        let mut workers = self.workers.lock().unwrap();
        let new_set: HashSet<String> = sources.iter().cloned().collect();

        let stale: Vec<String> = workers
            .keys()
            .filter(|k| !new_set.contains(*k))
            .cloned()
            .collect();
        for name in stale {
            if let Some(flag) = workers.remove(&name) {
                flag.store(true, Ordering::SeqCst);
            }
        }

        for name in sources {
            if workers.contains_key(&name) {
                continue;
            }
            let cancel = Arc::new(AtomicBool::new(false));
            workers.insert(name.clone(), cancel.clone());
            let app = self.app.clone();
            thread::spawn(move || worker(app, name, cancel));
        }
    }
}

fn worker(app: AppHandle, source: String, cancel: Arc<AtomicBool>) {
    const RATE: u32 = 48000;
    const FRAMES_PER_READ: usize = 240;
    const BYTES_PER_READ: usize = FRAMES_PER_READ * 4;

    let spec = Spec {
        format: Format::FLOAT32NE,
        channels: 1,
        rate: RATE,
    };
    if !spec.is_valid() {
        return;
    }

    let attr = BufferAttr {
        maxlength: (BYTES_PER_READ * 4) as u32,
        tlength: u32::MAX,
        prebuf: u32::MAX,
        minreq: u32::MAX,
        fragsize: BYTES_PER_READ as u32,
    };

    let event_name = format!("level:{}", source.replace('.', "_"));

    while !cancel.load(Ordering::SeqCst) {
        let stream = match Simple::new(
            None,
            "tideline-meter",
            Direction::Record,
            Some(&source),
            "level",
            &spec,
            None,
            Some(&attr),
        ) {
            Ok(s) => s,
            Err(_) => {
                thread::sleep(Duration::from_millis(500));
                continue;
            }
        };

        let mut buf = vec![0u8; BYTES_PER_READ];
        loop {
            if cancel.load(Ordering::SeqCst) {
                return;
            }
            if stream.read(&mut buf).is_err() {
                break;
            }
            let mut peak: f32 = 0.0;
            let mut sum_sq: f64 = 0.0;
            let mut count: u32 = 0;
            for chunk in buf.chunks_exact(4) {
                let v = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                let a = v.abs();
                if a > peak {
                    peak = a;
                }
                sum_sq += (v as f64) * (v as f64);
                count += 1;
            }
            let rms = if count > 0 {
                (sum_sq / count as f64).sqrt() as f32
            } else {
                0.0
            };
            let _ = app.emit(
                &event_name,
                LevelEvent {
                    source: source.clone(),
                    peak,
                    rms,
                },
            );
        }
        thread::sleep(Duration::from_millis(250));
    }
}
