use crate::Mode;
use notify_rust::{Notification, Urgency};
use std::sync::Mutex;

static LAST_ID: Mutex<Option<u32>> = Mutex::new(None);

/// Show a "mode changed" toast.
///
/// Runs on a fresh `std::thread` because `notify_rust::Notification::show()`
/// internally creates a `tokio` runtime to drive the D-Bus call, and tokio
/// panics if you try to do that from inside an existing tokio runtime —
/// which is exactly where we land when a portal Activated signal triggers
/// a mode change. Spawning a plain OS thread sidesteps the conflict.
///
/// We also tolerate a previously-poisoned `LAST_ID` mutex: if an earlier
/// invocation of this fn panicked mid-show (e.g. before this fix landed),
/// the lock is poisoned forever. Recover the inner value so subsequent
/// notifications still work.
pub fn notify_mode(mode: Mode) {
    let (summary, body) = match mode {
        Mode::Open => ("Open mic", "Microphone always on"),
        Mode::Ptt  => ("PTT mode", "Hold the bind to transmit"),
    };
    std::thread::spawn(move || {
        let mut n = Notification::new();
        n.summary(summary)
            .body(body)
            .appname("Tideline")
            .urgency(Urgency::Low)
            .timeout(notify_rust::Timeout::Milliseconds(1500));

        let mut last = LAST_ID.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(id) = *last {
            n.id(id);
        }
        match n.show() {
            Ok(handle) => { *last = Some(handle.id()); }
            Err(e)     => { eprintln!("ptt notify failed: {}", e); }
        }
    });
}
