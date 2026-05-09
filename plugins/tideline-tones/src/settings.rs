use crate::config::TonesConfig;
use tideline_sdk::ui::UiNode;

pub const SECTION_ID: &str = "main";
pub const EVT_ENABLED: &str = "enabled";
pub const EVT_VOLUME: &str = "volume";

pub fn render(cfg: &TonesConfig) -> UiNode {
    let v = serde_json::json!({
        "kind": "section",
        "title": "PTT Tones",
        "description": "Plays a short cue when push-to-talk transmits or releases.",
        "children": [
            {
                "kind": "toggle",
                "id": EVT_ENABLED,
                "label": "Enable tones",
                "value": cfg.enabled,
                "help": "Plays mic-unmute on press and mic-mute on release while in PTT mode."
            },
            {
                "kind": "slider",
                "id": EVT_VOLUME,
                "label": "Volume",
                "value": cfg.volume,
                "min": 0,
                "max": 100,
                "step": 1,
                "unit": "%"
            }
        ]
    });
    serde_json::from_value(v).expect("render produces valid UiNode")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_two_controls_with_current_values() {
        let cfg = TonesConfig {
            enabled: false,
            volume: 42,
        };
        let v = serde_json::to_value(render(&cfg)).unwrap();
        assert_eq!(v["kind"], "section");
        assert_eq!(v["children"].as_array().unwrap().len(), 2);
        assert_eq!(v["children"][0]["kind"], "toggle");
        assert_eq!(v["children"][0]["id"], EVT_ENABLED);
        assert_eq!(v["children"][0]["value"], json!(false));
        assert_eq!(v["children"][1]["kind"], "slider");
        assert_eq!(v["children"][1]["id"], EVT_VOLUME);
        assert_eq!(v["children"][1]["value"], json!(42));
        assert_eq!(v["children"][1]["min"], json!(0));
        assert_eq!(v["children"][1]["max"], json!(100));
    }

    #[test]
    fn renders_default_values() {
        let v = serde_json::to_value(render(&TonesConfig::default())).unwrap();
        assert_eq!(v["children"][0]["value"], json!(true));
        assert_eq!(v["children"][1]["value"], json!(100));
    }
}
