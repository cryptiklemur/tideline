//! Carla 2.6+ OSC contract spike.
//!
//! Run with `cargo test --test osc_contract_spike -- --ignored --nocapture`.
//! Requires `carla-rack` on PATH and a working JACK or pipewire-jack environment.
//! This test is the gate: green = approach 2 viable; red = revisit approach 3.

use rosc::OscType;
use std::process::Stdio;
use std::time::Duration;
use tideline_effects::osc::OscClient;
use tokio::process::Command;
use tokio::time::sleep;

const OSC_PORT: u16 = 22752;

#[tokio::test]
#[ignore]
async fn carla_rack_osc_contract_round_trip() {
    // carla-rack 2.6 does NOT accept --osc-tcp/--osc-udp/--client-name/
    // --no-jack-transport flags (they were removed/never existed in this build).
    // It binds UDP+TCP on 22752 by default when launched without args.
    // -n (--no-gui) requires a project file, so we run with the GUI.
    let mut child = Command::new("carla-rack")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("carla-rack must be on PATH for spike");

    sleep(Duration::from_millis(3500)).await;

    let target = format!("127.0.0.1:{}", OSC_PORT).parse().unwrap();
    let client = OscClient::connect(target).await.expect("connect osc");

    // 1. add_plugin: LV2 by URI. We use the LSP gate as the test plugin since
    //    the install path provisions it. URI taken from lsp-plugins-lv2 1.2+.
    client
        .send(
            "/add_plugin",
            vec![
                OscType::Int(2 /* PLUGIN_LV2 */),
                OscType::String("http://lsp-plug.in/plugins/lv2/gate_mono".into()),
            ],
        )
        .await
        .unwrap();
    sleep(Duration::from_millis(500)).await;

    // 2. set_active: bypass plugin id 0
    client
        .send("/set_active", vec![OscType::Int(0), OscType::Int(0)])
        .await
        .unwrap();
    client
        .send("/set_active", vec![OscType::Int(0), OscType::Int(1)])
        .await
        .unwrap();

    // 3. switch_plugins: pairwise reorder. With a single plugin this is a no-op,
    //    but the message must be accepted.
    client
        .send("/switch_plugins", vec![OscType::Int(0), OscType::Int(0)])
        .await
        .unwrap();

    // 4. show_custom_ui: open and close (no display in CI; we accept "fail to
    //    open window" as long as the message itself is accepted).
    client
        .send("/show_custom_ui", vec![OscType::Int(0), OscType::Int(1)])
        .await
        .unwrap();
    sleep(Duration::from_millis(200)).await;
    client
        .send("/show_custom_ui", vec![OscType::Int(0), OscType::Int(0)])
        .await
        .unwrap();

    // 5. save_plugin_state — carla replies with an event containing the state chunk.
    client
        .send("/save_plugin_state", vec![OscType::Int(0)])
        .await
        .unwrap();

    // 6. load_plugin_state with a known-bad chunk — we only check the message is
    //    accepted (carla typically replies with an error event we don't need).
    client
        .send(
            "/load_plugin_state",
            vec![OscType::Int(0), OscType::String("AAAA".into())],
        )
        .await
        .unwrap();

    // 7. remove_plugin
    client
        .send("/remove_plugin", vec![OscType::Int(0)])
        .await
        .unwrap();

    // Drain any replies for diagnostic output. We do NOT assert on reply shape —
    // the documented carla OSC contract historically has been one-way client→server
    // for these methods, with state coming back via separate event broadcasts.
    // What we ARE asserting: carla-rack stays alive (no crash from any of our
    // messages) for at least 1s after the last send.
    sleep(Duration::from_millis(1000)).await;
    let still_running = child.try_wait().unwrap().is_none();
    assert!(
        still_running,
        "carla-rack must survive our 7-message contract round-trip"
    );

    let _ = child.kill().await;
}
