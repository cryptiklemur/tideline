use crate::config::PluginConfig;
use tideline_sdk::ui::UiNode;

pub const SECTION_ID: &str = "main";
pub const EVT_LED_ENABLED: &str = "led_enabled";

/// Single section, single toggle. Toggle id is `led_enabled`; `runtime::
/// on_settings_event` matches that id exactly.
pub fn render(cfg: &PluginConfig, present: bool) -> UiNode {
    let mut children = vec![];

    if !present {
        children.push(serde_json::json!({
            "kind": "banner",
            "tone": "warning",
            "title": "Wave XLR not detected",
            "body": "Plug in your Elgato Wave XLR. The toggle below stays inert until the device is present."
        }));
    }

    children.push(serde_json::json!({
        "kind": "toggle",
        "id": EVT_LED_ENABLED,
        "label": "Drive Wave XLR mute LED",
        "value": cfg.led_enabled,
        "help": "When on, the LED turns red while you are muted and blue while transmitting."
    }));

    let v = serde_json::json!({
        "kind": "section",
        "title": "Wave XLR",
        "description": "Hardware LED follows your push-to-talk transmit state.",
        "children": children
    });
    serde_json::from_value(v).expect("render produces valid UiNode")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_present_has_no_banner() {
        let tree = render(&PluginConfig { led_enabled: true }, true);
        let s = serde_json::to_value(&tree).unwrap().to_string();
        assert!(!s.contains("Wave XLR not detected"));
        assert!(s.contains("led_enabled"));
    }

    #[test]
    fn render_absent_has_banner() {
        let tree = render(&PluginConfig { led_enabled: true }, false);
        let s = serde_json::to_value(&tree).unwrap().to_string();
        assert!(s.contains("Wave XLR not detected"));
    }

    #[test]
    fn toggle_value_reflects_config() {
        let on = serde_json::to_value(&render(&PluginConfig { led_enabled: true }, true)).unwrap().to_string();
        let off = serde_json::to_value(&render(&PluginConfig { led_enabled: false }, true)).unwrap().to_string();
        assert!(on.contains("\"value\":true"));
        assert!(off.contains("\"value\":false"));
    }
}
