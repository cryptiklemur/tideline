//! Carla engine lifecycle. Filled in by later tasks.
use std::sync::Arc;

use crate::state::EffectsState;

pub async fn start(_state: Arc<EffectsState>) -> anyhow::Result<()> {
    Ok(())
}

pub async fn on_pipewire_restart_pre(_state: Arc<EffectsState>) {
    // Filled in by later tasks.
}

pub async fn on_pipewire_restart_post(_state: Arc<EffectsState>) {
    // Filled in by later tasks.
}
