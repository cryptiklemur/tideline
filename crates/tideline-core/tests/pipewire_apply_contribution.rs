use tideline_core::pipewire::contribute::apply_contribution;
use tideline_core::pipewire::directive::{ArgValue, LoadModuleHeader, PipewireDirective, RewireableTag};

fn tag() -> RewireableTag {
    RewireableTag {
        channel_uuid: "11111111-1111-1111-1111-111111111111".into(),
        role: "output_loopback".into(),
    }
}

#[test]
fn load_module_appends() {
    let base = vec![];
    let extra = vec![PipewireDirective::LoadModule {
        header: LoadModuleHeader::Named("module-x".into()),
        args: vec![],
        rewireable_tag: None,
    }];
    let out = apply_contribution(base, &extra);
    assert_eq!(out.len(), 1);
}

#[test]
fn rewire_loopback_replaces_capture_node() {
    let base = vec![PipewireDirective::LoadModule {
        header: LoadModuleHeader::Named("libpipewire-module-loopback".into()),
        args: vec![(
            "capture.props".into(),
            ArgValue::Literal("media.class=Audio/Sink".into()),
        )],
        rewireable_tag: Some(tag()),
    }];
    let extra = vec![PipewireDirective::RewireLoopback {
        target_tag: tag(),
        new_capture_node: Some("custom_capture".into()),
        new_playback_node: None,
    }];
    let out = apply_contribution(base, &extra);
    let PipewireDirective::LoadModule { args, .. } = &out[0] else {
        panic!("expected LoadModule")
    };
    let cap = args
        .iter()
        .find(|(k, _)| k == "capture.props")
        .map(|(_, v)| v)
        .expect("capture.props present");
    match cap {
        ArgValue::Literal(s) | ArgValue::Quoted(s) => assert!(s.contains("custom_capture")),
        ArgValue::Group(_) => panic!("expected leaf, got nested group"),
        ArgValue::Array(_) => panic!("expected leaf, got array"),
    }
}

#[test]
fn destroy_module_removes_tagged_directive() {
    let base = vec![
        PipewireDirective::LoadModule {
            header: LoadModuleHeader::Named("a".into()),
            args: vec![],
            rewireable_tag: Some(tag()),
        },
        PipewireDirective::LoadModule {
            header: LoadModuleHeader::Named("b".into()),
            args: vec![],
            rewireable_tag: None,
        },
    ];
    let extra = vec![PipewireDirective::DestroyModule { target_tag: tag() }];
    let out = apply_contribution(base, &extra);
    assert_eq!(out.len(), 1);
    let PipewireDirective::LoadModule { header, .. } = &out[0] else {
        panic!("expected LoadModule")
    };
    match header {
        LoadModuleHeader::Named(name) => assert_eq!(name, "b"),
        LoadModuleHeader::Factory(_) => panic!("expected Named header"),
    }
}

#[test]
fn insert_node_before_inserts_in_order() {
    let base = vec![PipewireDirective::LoadModule {
        header: LoadModuleHeader::Named("loop".into()),
        args: vec![],
        rewireable_tag: Some(tag()),
    }];
    let extra = vec![PipewireDirective::InsertNodeBefore {
        target_tag: tag(),
        node_factory: "support.null-audio-sink".into(),
        args: vec![("node.name".into(), ArgValue::Quoted("fx".into()))],
    }];
    let out = apply_contribution(base, &extra);
    assert_eq!(out.len(), 2);
    let PipewireDirective::LoadModule { header, .. } = &out[0] else {
        panic!("expected LoadModule")
    };
    match header {
        LoadModuleHeader::Factory(name) => assert_eq!(name, "support.null-audio-sink"),
        LoadModuleHeader::Named(_) => panic!("expected Factory header for inserted node"),
    }
}
