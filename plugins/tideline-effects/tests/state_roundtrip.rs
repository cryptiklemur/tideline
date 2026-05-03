use tideline_effects::persist::{decode_state, encode_state};

#[test]
fn empty_state_round_trips() {
    let chunk = b"";
    let s = encode_state(chunk);
    assert_eq!(decode_state(&s).unwrap(), Vec::<u8>::new());
}

#[test]
fn arbitrary_state_round_trips() {
    let chunk = b"<carla-state-chunk>raw bytes \x00\x01\x02</>";
    let s = encode_state(chunk);
    assert_eq!(decode_state(&s).unwrap(), chunk.to_vec());
}

#[test]
fn invalid_b64_returns_none() {
    assert!(decode_state("not valid base64!@#$").is_none());
}

#[tokio::test]
#[ignore]
async fn round_trip_lv2_state() {
    use tideline_effects::effect::Effect;
    use tideline_effects::state::EffectsState;
    use tideline_effects::{chain_ops, engine, persist};
    use uuid::Uuid;

    let state = EffectsState::new("tideline-effects-test");
    if engine::start(state.clone()).await.is_err() {
        eprintln!("no JACK; SKIP");
        return;
    }
    let channel = Uuid::new_v4();
    let effect = Effect::new_lv2("http://lsp-plug.in/plugins/lv2/gate_mono");
    let plugin_id = chain_ops::add_effect(state.clone(), channel, effect.clone())
        .await
        .expect("add");

    let b64 = persist::save_effect_state(state.clone(), channel, effect.id)
        .await
        .expect("save");
    assert!(!b64.is_empty(), "state must encode");

    persist::load_effect_state(state.clone(), plugin_id, &b64)
        .await
        .expect("load");

    chain_ops::remove_effect(state.clone(), channel, effect.id)
        .await
        .expect("remove");
    engine::stop(state).await;
}
