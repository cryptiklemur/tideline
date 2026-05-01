use tideline_core::pipewire::directive::{LoadModuleHeader, PipewireDirective, RewireableTag};
use tideline_host::contribute::{PluginContribution, PluginPriority, resolve_collisions};

fn tag(uuid: &str) -> RewireableTag {
    RewireableTag {
        channel_uuid: uuid.into(),
        role: "output_loopback".into(),
    }
}

#[test]
fn higher_priority_wins_on_destroy_collision() {
    let target = tag("11111111-1111-1111-1111-111111111111");
    let low = PluginContribution {
        plugin_id: "io.tideline.a".into(),
        priority: PluginPriority(10),
        directives: vec![PipewireDirective::DestroyModule {
            target_tag: target.clone(),
        }],
    };
    let high = PluginContribution {
        plugin_id: "io.tideline.b".into(),
        priority: PluginPriority(50),
        directives: vec![PipewireDirective::RewireLoopback {
            target_tag: target.clone(),
            new_capture_node: Some("custom".into()),
            new_playback_node: None,
        }],
    };
    let resolved = resolve_collisions(vec![low, high]);
    assert_eq!(
        resolved.len(),
        1,
        "destroy must lose to rewire when rewire has higher priority"
    );
    assert_eq!(resolved[0].plugin_id, "io.tideline.b");
}

#[test]
fn unrelated_directives_all_survive() {
    let a = PluginContribution {
        plugin_id: "a".into(),
        priority: PluginPriority(10),
        directives: vec![PipewireDirective::LoadModule {
            header: LoadModuleHeader::Named("x".into()),
            args: vec![],
            rewireable_tag: None,
        }],
    };
    let b = PluginContribution {
        plugin_id: "b".into(),
        priority: PluginPriority(20),
        directives: vec![PipewireDirective::LoadModule {
            header: LoadModuleHeader::Named("y".into()),
            args: vec![],
            rewireable_tag: None,
        }],
    };
    let resolved = resolve_collisions(vec![a, b]);
    assert_eq!(resolved.len(), 2);
}
