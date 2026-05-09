use tideline_core::model::{AppConfig, ChannelCfg, Mix};
use uuid::Uuid;

#[test]
fn channel_by_uuid_and_by_name() {
    let mut cfg = AppConfig::default();
    cfg.channels.push(ChannelCfg::new("Game"));
    cfg.channels.push(ChannelCfg::new("Music"));
    let game_uuid = cfg.channels[0].uuid;

    assert_eq!(cfg.channel_by_uuid(game_uuid).unwrap().name, "Game");
    assert_eq!(
        cfg.channel_by_name("Music").unwrap().uuid,
        cfg.channels[1].uuid
    );
    assert!(cfg.channel_by_uuid(Uuid::nil()).is_none());
    assert!(cfg.channel_by_name("Nope").is_none());
}

#[test]
fn mix_by_id_and_uuid_for_id() {
    let mut cfg = AppConfig::default();
    cfg.mixes.push(Mix::new("default", "Default"));
    cfg.mixes.push(Mix::new("vc", "Voice Chat"));

    assert_eq!(cfg.mix_by_id("vc").unwrap().name, "Voice Chat");
    assert_eq!(cfg.mix_uuid_for_id("default").unwrap(), cfg.mixes[0].uuid);
    assert!(cfg.mix_by_id("nope").is_none());
    assert!(cfg.mix_uuid_for_id("nope").is_none());
}

#[test]
fn remove_channel_by_uuid_keeps_others() {
    let mut cfg = AppConfig::default();
    cfg.channels.push(ChannelCfg::new("A"));
    cfg.channels.push(ChannelCfg::new("B"));
    let a_uuid = cfg.channels[0].uuid;
    cfg.remove_channel_by_uuid(a_uuid);
    assert_eq!(cfg.channels.len(), 1);
    assert_eq!(cfg.channels[0].name, "B");
}

#[test]
fn remove_mix_by_uuid_keeps_others() {
    let mut cfg = AppConfig::default();
    cfg.mixes.push(Mix::new("a", "A"));
    cfg.mixes.push(Mix::new("b", "B"));
    let a_uuid = cfg.mixes[0].uuid;
    cfg.remove_mix_by_uuid(a_uuid);
    assert_eq!(cfg.mixes.len(), 1);
    assert_eq!(cfg.mixes[0].id, "b");
}
