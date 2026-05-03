//! Per-channel chain operations against the in-process carla engine.
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use uuid::Uuid;

use crate::carla::CarlaError;
use crate::effect::Effect;
use crate::state::EffectsState;

/// Convert a panic payload into a printable string.
fn panic_msg(panic: Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "carla FFI panicked".into())
}

pub async fn add_effect(
    state: Arc<EffectsState>,
    channel_id: Uuid,
    effect: Effect,
) -> Result<u32, CarlaError> {
    let engine = state
        .engine()
        .await
        .ok_or_else(|| CarlaError::Ffi("engine not initialized".into()))?;
    let host = engine.lock().await;
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        host.add_lv2(&effect.uri, &format!("{channel_id}:{}", effect.id))
    }));
    drop(host);
    let plugin_id = match result {
        Ok(r) => r?,
        Err(panic) => {
            let msg = panic_msg(panic);
            crate::engine::mark_unhealthy(state.clone(), msg.clone()).await;
            return Err(CarlaError::Ffi(msg));
        }
    };
    state.attach_effect(channel_id, effect, plugin_id).await;
    Ok(plugin_id)
}

pub async fn remove_effect(
    state: Arc<EffectsState>,
    channel_id: Uuid,
    effect_id: Uuid,
) -> Result<(), CarlaError> {
    let engine = state
        .engine()
        .await
        .ok_or_else(|| CarlaError::Ffi("engine not initialized".into()))?;
    let plugin_id = state
        .lookup_plugin_id(channel_id, effect_id)
        .await
        .ok_or_else(|| CarlaError::Ffi(format!("effect {effect_id} not in chain {channel_id}")))?;
    let host = engine.lock().await;
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| host.remove(plugin_id)));
    drop(host);
    match result {
        Ok(true) => {}
        Ok(false) => return Err(CarlaError::Ffi("remove_plugin returned false".into())),
        Err(panic) => {
            let msg = panic_msg(panic);
            crate::engine::mark_unhealthy(state.clone(), msg.clone()).await;
            return Err(CarlaError::Ffi(msg));
        }
    }
    state.detach_effect(channel_id, effect_id).await;
    Ok(())
}

pub async fn reorder_chain(
    state: Arc<EffectsState>,
    channel_id: Uuid,
    new_order: Vec<Uuid>,
) -> Result<(), CarlaError> {
    let engine = state
        .engine()
        .await
        .ok_or_else(|| CarlaError::Ffi("engine not initialized".into()))?;
    let current_order = state.chain_order(channel_id).await;
    if current_order.len() != new_order.len() {
        return Err(CarlaError::Ffi("reorder: length mismatch".into()));
    }
    let host = engine.lock().await;
    let mut working = current_order.clone();
    for (target_idx, target_eid) in new_order.iter().enumerate() {
        let cur_idx = working
            .iter()
            .position(|e| e == target_eid)
            .ok_or_else(|| CarlaError::Ffi(format!("reorder: {target_eid} missing")))?;
        if cur_idx == target_idx {
            continue;
        }
        let a_pid = state
            .lookup_plugin_id(channel_id, working[cur_idx])
            .await
            .ok_or_else(|| CarlaError::Ffi("reorder: plugin id lookup".into()))?;
        let b_pid = state
            .lookup_plugin_id(channel_id, working[target_idx])
            .await
            .ok_or_else(|| CarlaError::Ffi("reorder: plugin id lookup".into()))?;
        let result =
            std::panic::catch_unwind(AssertUnwindSafe(|| host.switch_plugins(a_pid, b_pid)));
        match result {
            Ok(true) => {}
            Ok(false) => {
                drop(host);
                return Err(CarlaError::Ffi(format!(
                    "switch_plugins({a_pid},{b_pid}) failed"
                )));
            }
            Err(panic) => {
                drop(host);
                let msg = panic_msg(panic);
                crate::engine::mark_unhealthy(state.clone(), msg.clone()).await;
                return Err(CarlaError::Ffi(msg));
            }
        }
        working.swap(cur_idx, target_idx);
    }
    drop(host);
    state.set_chain_order(channel_id, new_order).await;
    Ok(())
}
