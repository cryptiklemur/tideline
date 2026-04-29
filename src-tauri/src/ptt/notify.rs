use crate::Mode;
use notify_rust::{Notification, Urgency};
use std::sync::Mutex;

static LAST_ID: Mutex<Option<u32>> = Mutex::new(None);

pub fn notify_mode(mode: Mode) {
    let (summary, body) = match mode {
        Mode::Open => ("Open mic", "Microphone always on"),
        Mode::Ptt  => ("PTT mode", "Hold the bind to transmit"),
    };
    let mut n = Notification::new();
    n.summary(summary)
        .body(body)
        .appname("Tideline")
        .urgency(Urgency::Low)
        .timeout(notify_rust::Timeout::Milliseconds(1500));

    let mut last = LAST_ID.lock().unwrap();
    if let Some(id) = *last {
        n.id(id);
    }
    match n.show() {
        Ok(handle) => {
            *last = Some(handle.id());
        }
        Err(e) => {
            eprintln!("ptt notify failed: {}", e);
        }
    }
}
