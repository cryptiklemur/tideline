use crate::config::NotifConfig;
use tideline_sdk::ui::UiNode;

pub const SECTION_ID: &str = "main";
pub const EVT_ENABLED: &str = "enabled";

pub fn render(cfg: &NotifConfig) -> UiNode {
    let v = serde_json::json!({
        "kind": "section",
        "title": "PTT Notifications",
        "description": "Show a desktop notification when push-to-talk mode is toggled.",
        "children": [
            {
                "kind": "toggle",
                "id": EVT_ENABLED,
                "label": "Enable notifications",
                "value": cfg.enabled,
                "help": "Posts a low-urgency toast titled 'Open mic' or 'PTT mode' when the mode changes."
            }
        ]
    });
    serde_json::from_value(v).expect("render produces valid UiNode")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test] fn renders_one_toggle_with_current_value() {
        let cfg = NotifConfig { enabled: false };
        let v = serde_json::to_value(render(&cfg)).unwrap();
        assert_eq!(v["kind"], "section");
        assert_eq!(v["children"].as_array().unwrap().len(), 1);
        assert_eq!(v["children"][0]["kind"], "toggle");
        assert_eq!(v["children"][0]["id"], EVT_ENABLED);
        assert_eq!(v["children"][0]["value"], json!(false));
    }

    #[test] fn renders_default_enabled_true() {
        let v = serde_json::to_value(render(&NotifConfig::default())).unwrap();
        assert_eq!(v["children"][0]["value"], json!(true));
    }
}
