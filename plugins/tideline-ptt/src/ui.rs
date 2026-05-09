use crate::config::PluginConfig;
use crate::runtime::CaptureMethod;
use crate::state::{Mode, PerSourceState};
use serde_json::{json, Value};
use tideline_sdk::types::AudioSource;

pub fn settings_section(
    cfg: &PluginConfig,
    capture: CaptureMethod,
    error: Option<&str>,
    _sources: &[AudioSource],
) -> Value {
    let heading_text = match capture {
        CaptureMethod::Portal => "Push-to-talk (XDG portal)",
        CaptureMethod::Evdev => "Push-to-talk (evdev)",
        CaptureMethod::None => "Push-to-talk",
    };
    let mut children = vec![json!({
        "kind": "heading",
        "id": "ptt-heading",
        "text": heading_text,
    })];

    if let Some(err) = error {
        let action = match capture {
            CaptureMethod::Evdev => json!({
                "label": "Grant input access",
                "action": "tideline-ptt:install_udev_rule",
            }),
            CaptureMethod::Portal => json!({
                "label": "Configure shortcuts in System Settings",
                "action": "tideline-ptt:configure_shortcuts",
            }),
            CaptureMethod::None => Value::Null,
        };
        children.push(json!({
            "kind": "banner",
            "id": "ptt-banner-error",
            "tone": "warning",
            "text": err,
            "action": action,
        }));
    }

    match capture {
        CaptureMethod::Evdev => {
            children.push(json!({
                "kind": "heading",
                "id": "ptt-default-keybinds",
                "text": "Default keybinds",
                "level": 4,
            }));
            children.push(json!({
                "kind": "label",
                "id": "ptt-default-keybinds-help",
                "text": "These apply to every push-to-talk input.",
                "muted": true,
            }));
            children.push(json!({
                "kind": "binding_capture",
                "id": "tideline-ptt:set_mode_toggle_binding",
                "label": "Mode toggle (tap)",
                "sublabel": "Switches between Open mic and PTT",
                "binding": cfg.mode_toggle_binding,
            }));
            children.push(json!({
                "kind": "binding_capture",
                "id": "tideline-ptt:set_hold_binding",
                "label": "PTT hold",
                "sublabel": "Hold to transmit while in PTT mode",
                "binding": cfg.hold_binding,
            }));
        }
        CaptureMethod::Portal => {
            children.push(json!({
                "kind": "label",
                "id": "ptt-portal-info",
                "text": "Shortcuts are managed by your desktop's GlobalShortcuts portal. Look for `mode_toggle` and `hold` under Tideline shortcuts.",
                "muted": true,
            }));
            children.push(json!({
                "kind": "button",
                "id": "tideline-ptt:configure_shortcuts",
                "text": "Configure shortcuts in System Settings",
                "icon": { "name": "settings" },
                "variant": "soft",
            }));
        }
        CaptureMethod::None => {}
    }

    let enabled_summary = if cfg.enabled_sources.is_empty() {
        "Push-to-talk is not enabled on any inputs. Toggle it on individual inputs from the channel input picker.".to_string()
    } else {
        format!(
            "Push-to-talk is active on {} input{}.",
            cfg.enabled_sources.len(),
            if cfg.enabled_sources.len() == 1 {
                ""
            } else {
                "s"
            }
        )
    };
    children.push(json!({
        "kind": "label",
        "id": "ptt-enabled-summary",
        "text": enabled_summary,
        "muted": true,
    }));

    json!({
        "kind": "section",
        "id": "tideline-ptt",
        "children": children,
    })
}

#[allow(dead_code)]
pub fn status_pill(state: PerSourceState, error: Option<&str>) -> Value {
    let (text, tone) = if error.is_some() {
        ("PTT error", "warning")
    } else {
        match (state.mode, state.transmitting()) {
            (Mode::Open, _) => ("Open mic", "primary"),
            (Mode::Ptt, true) => ("PTT — live", "primary"),
            (Mode::Ptt, false) => ("PTT — muted", "error"),
        }
    };
    json!({
        "kind": "badge",
        "text": text,
        "tone": tone,
        "pulse": state.transmitting() && error.is_none(),
        "action": "tideline-ptt:toggle_mode",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::Binding;
    use crate::state::PerSourceState;

    fn cfg_with_bindings() -> PluginConfig {
        PluginConfig {
            mode_toggle_binding: Some(Binding::parse("Ctrl+Shift+F12").unwrap()),
            hold_binding: Some(Binding::parse("Mouse5").unwrap()),
            enabled_sources: vec!["alsa_input.usb".to_string()],
            ..PluginConfig::default()
        }
    }

    #[test]
    fn settings_section_no_error_no_evdev_omits_bindings_and_banner() {
        let v = settings_section(&PluginConfig::default(), CaptureMethod::Portal, None, &[]);
        let kinds: Vec<&str> = v["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, ["heading", "label", "button", "label"]);
    }

    #[test]
    fn settings_section_evdev_includes_default_keybinds_block() {
        let v = settings_section(&cfg_with_bindings(), CaptureMethod::Evdev, None, &[]);
        let kinds: Vec<&str> = v["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["kind"].as_str().unwrap())
            .collect();
        assert_eq!(
            kinds,
            [
                "heading",
                "heading",
                "label",
                "binding_capture",
                "binding_capture",
                "label"
            ]
        );
    }

    #[test]
    fn settings_section_with_error_adds_banner() {
        let v = settings_section(
            &PluginConfig::default(),
            CaptureMethod::Evdev,
            Some("denied"),
            &[],
        );
        let banner = v["children"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["kind"] == "banner");
        assert!(banner.is_some());
        let b = banner.unwrap();
        assert_eq!(b["text"], "denied");
        assert_eq!(b["tone"], "warning");
        assert_eq!(b["action"]["action"], "tideline-ptt:install_udev_rule");
    }

    #[test]
    fn settings_section_portal_error_uses_configure_action() {
        let v = settings_section(
            &PluginConfig::default(),
            CaptureMethod::Portal,
            Some("oops"),
            &[],
        );
        let banner = v["children"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["kind"] == "banner")
            .unwrap();
        assert_eq!(
            banner["action"]["action"],
            "tideline-ptt:configure_shortcuts"
        );
    }

    #[test]
    fn status_pill_error_overrides_mode() {
        let s = PerSourceState {
            mode: Mode::Open,
            hold_active: false,
        };
        let v = status_pill(s, Some("nope"));
        assert_eq!(v["text"], "PTT error");
        assert_eq!(v["tone"], "warning");
        assert_eq!(v["pulse"], false);
    }

    #[test]
    fn status_pill_open_no_error_pulses_true() {
        let s = PerSourceState {
            mode: Mode::Open,
            hold_active: false,
        };
        let v = status_pill(s, None);
        assert_eq!(v["text"], "Open mic");
        assert_eq!(v["tone"], "primary");
        assert_eq!(v["pulse"], true);
    }

    #[test]
    fn status_pill_ptt_held_lives() {
        let s = PerSourceState {
            mode: Mode::Ptt,
            hold_active: true,
        };
        let v = status_pill(s, None);
        assert_eq!(v["text"], "PTT — live");
        assert_eq!(v["pulse"], true);
    }

    #[test]
    fn status_pill_ptt_idle_muted() {
        let s = PerSourceState {
            mode: Mode::Ptt,
            hold_active: false,
        };
        let v = status_pill(s, None);
        assert_eq!(v["text"], "PTT — muted");
        assert_eq!(v["tone"], "error");
        assert_eq!(v["pulse"], false);
    }
}
