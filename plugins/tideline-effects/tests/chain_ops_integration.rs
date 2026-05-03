//! Verifies the chain-op cycle add → reorder → remove against a live
//! carla engine. Requires JACK/PipeWire-jack at test time, so #[ignore]-gated.
//!
//! Run with `cargo test -p tideline-effects --test chain_ops_integration -- --ignored --nocapture`.

use tideline_effects::chain_ops;
use tideline_effects::effect::Effect;
use tideline_effects::engine;
use tideline_effects::state::EffectsState;
use uuid::Uuid;

#[tokio::test]
#[ignore]
async fn add_reorder_remove_cycle() {
    let state = EffectsState::new("tideline-effects-test");
    if let Err(e) = engine::start(state.clone()).await {
        eprintln!("engine start failed (no JACK?): {e}; SKIP");
        return;
    }
    let channel = Uuid::new_v4();
    let e1 = Effect::new_lv2("http://lsp-plug.in/plugins/lv2/gate_mono");
    let e2 = Effect::new_lv2("http://lsp-plug.in/plugins/lv2/gate_mono");

    let p1 = chain_ops::add_effect(state.clone(), channel, e1.clone())
        .await
        .expect("add e1");
    let p2 = chain_ops::add_effect(state.clone(), channel, e2.clone())
        .await
        .expect("add e2");
    assert_ne!(p1, p2);

    chain_ops::reorder_chain(state.clone(), channel, vec![e2.id, e1.id])
        .await
        .expect("reorder");

    chain_ops::remove_effect(state.clone(), channel, e2.id)
        .await
        .expect("remove e2");
    chain_ops::remove_effect(state.clone(), channel, e1.id)
        .await
        .expect("remove e1");

    engine::stop(state).await;
}
