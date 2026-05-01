use tideline_host::PluginRegistry;
use tideline_host::contributions::{Contributions, TrayItemContribution};

#[tokio::test]
async fn contributions_carry_tray_items_sorted_by_priority() {
    let registry = PluginRegistry::new_for_test();
    registry
        .set_contributions(Contributions {
            tray_items: vec![
                TrayItemContribution {
                    plugin_id: "b".into(),
                    item_id: "1".into(),
                    label: "Beta".into(),
                    accelerator: None,
                    icon: None,
                    priority: 5,
                },
                TrayItemContribution {
                    plugin_id: "a".into(),
                    item_id: "1".into(),
                    label: "Alpha".into(),
                    accelerator: None,
                    icon: None,
                    priority: 10,
                },
            ],
            ..Default::default()
        })
        .await;
    let snap = registry.contributions().await;
    let mut items = snap.tray_items.clone();
    items.sort_by(|x, y| {
        y.priority
            .cmp(&x.priority)
            .then_with(|| x.plugin_id.cmp(&y.plugin_id))
    });
    assert_eq!(items[0].plugin_id, "a");
    assert_eq!(items[1].plugin_id, "b");
}
