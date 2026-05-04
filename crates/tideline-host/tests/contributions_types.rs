use tideline_host::contributions::{Contributions, SettingsSectionContribution};

#[test]
fn contributions_default_is_empty() {
    let c = Contributions::default();
    assert!(c.settings_sections.is_empty());
    assert!(c.status_pills.is_empty());
    assert!(c.channel_overlays.is_empty());
    assert!(c.tray_items.is_empty());
    assert!(c.keybind_actions.is_empty());
    assert!(c.iframe_surfaces.is_empty());
}

#[test]
fn settings_section_round_trips() {
    let s = SettingsSectionContribution {
        plugin_id: "io.tideline.test".into(),
        surface_id: "main".into(),
        title: "Test".into(),
        icon: None,
        priority: 10,
        parent_surface_id: None,
        tree: serde_json::json!({"kind": "section", "children": []}),
    };
    let json = serde_json::to_string(&s).unwrap();
    let back: SettingsSectionContribution = serde_json::from_str(&json).unwrap();
    assert_eq!(back.title, "Test");
    assert_eq!(back.priority, 10);
}
