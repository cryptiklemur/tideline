//! Engine lifecycle and driver selection.
//!
//! Carla's standalone host supports one process-wide engine instance. We
//! pick the driver in priority order: JACK (PipeWire's libjack shim handles
//! it), then PulseAudio, then the first-listed driver. "Dummy" is preferred
//! when present (test environments) but isn't shipped on Arch's carla-git.

use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::carla::{CarlaError, Host};
use crate::state::EffectsState;

const PREFERRED_DRIVERS: &[&str] = &["Dummy", "JACK", "PulseAudio"];

pub fn select_driver(available: &[String]) -> Option<String> {
    for &p in PREFERRED_DRIVERS {
        if let Some(d) = available.iter().find(|d| d.eq_ignore_ascii_case(p)) {
            return Some(d.clone());
        }
    }
    available.first().cloned()
}

pub async fn start(state: Arc<EffectsState>) -> Result<(), CarlaError> {
    let host = Host::init()?;
    let drivers = host.drivers();
    let driver = select_driver(&drivers)
        .ok_or_else(|| CarlaError::Ffi("no carla drivers reported".into()))?;
    info!(?drivers, chose = %driver, "carla engine driver");
    host.engine_init(&driver, "tideline-effects")?;
    state.set_engine(Arc::new(Mutex::new(host))).await;
    Ok(())
}

pub async fn stop(state: Arc<EffectsState>) {
    if let Some(h) = state.take_engine().await {
        let host = h.lock().await;
        if !host.engine_close() {
            warn!("carla engine_close returned false");
        }
    }
}

pub async fn on_pipewire_restart_pre(state: Arc<EffectsState>) {
    // Pipewire is restarting — close the engine so it doesn't hold stale jack ports.
    stop(state).await;
}

pub async fn on_pipewire_restart_post(state: Arc<EffectsState>) {
    // Pipewire is back — reinit and re-add every channel's effect chain.
    if let Err(e) = start(state.clone()).await {
        warn!(error = %e, "engine restart after pipewire restart failed");
        return;
    }
    crate::state::reapply_all_chains(state).await;
}

pub async fn mark_unhealthy(state: Arc<EffectsState>, reason: String) {
    warn!(reason = %reason, "marking carla engine unhealthy");
    if let Some(h) = state.take_engine().await {
        // Best-effort close; ignore failure.
        let _ = h.lock().await.engine_close();
    }
    if let Some(host) = state.host.get() {
        let _ = host
            .event_publish(
                "tideline-effects:engine_unhealthy",
                serde_json::json!({ "reason": reason }),
            )
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dummy_wins_when_present() {
        let avail = vec!["JACK".into(), "Dummy".into(), "PulseAudio".into()];
        assert_eq!(select_driver(&avail).as_deref(), Some("Dummy"));
    }

    #[test]
    fn jack_beats_pulse() {
        let avail = vec!["PulseAudio".into(), "JACK".into()];
        assert_eq!(select_driver(&avail).as_deref(), Some("JACK"));
    }

    #[test]
    fn first_when_no_preferred() {
        let avail = vec!["SDL".into(), "ALSA".into()];
        assert_eq!(select_driver(&avail).as_deref(), Some("SDL"));
    }

    #[test]
    fn empty_returns_none() {
        assert!(select_driver(&[]).is_none());
    }

    #[test]
    fn case_insensitive_match() {
        let avail = vec!["jack".into()];
        assert_eq!(select_driver(&avail).as_deref(), Some("jack"));
    }
}

#[cfg(test)]
mod resilience_tests {
    use super::*;

    #[tokio::test]
    async fn unhealthy_clears_engine_slot() {
        let state = crate::state::EffectsState::new("test");
        // No engine started — mark_unhealthy should still be safe.
        mark_unhealthy(state.clone(), "test".into()).await;
        assert!(state.engine().await.is_none());
    }
}
