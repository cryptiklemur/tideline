use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modifier {
    Ctrl,
    Shift,
    Alt,
    Super,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Binding {
    Keyboard { mods: Vec<Modifier>, key: String },
    Mouse { mods: Vec<Modifier>, button: String },
}

const MOD_ORDER: &[(Modifier, &str)] = &[
    (Modifier::Ctrl, "Ctrl"),
    (Modifier::Shift, "Shift"),
    (Modifier::Alt, "Alt"),
    (Modifier::Super, "Super"),
];

fn parse_mod(token: &str) -> Option<Modifier> {
    match token {
        "Ctrl" => Some(Modifier::Ctrl),
        "Alt" => Some(Modifier::Alt),
        "Shift" => Some(Modifier::Shift),
        "Super" | "Meta" | "Win" => Some(Modifier::Super),
        _ => None,
    }
}

fn is_valid_key(key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    if key.len() == 1 && key.chars().next().unwrap().is_ascii_alphanumeric() {
        return true;
    }
    matches!(
        key,
        "F1" | "F2"
            | "F3"
            | "F4"
            | "F5"
            | "F6"
            | "F7"
            | "F8"
            | "F9"
            | "F10"
            | "F11"
            | "F12"
            | "F13"
            | "F14"
            | "F15"
            | "F16"
            | "F17"
            | "F18"
            | "F19"
            | "F20"
            | "F21"
            | "F22"
            | "F23"
            | "F24"
            | "Space"
            | "Enter"
            | "Tab"
            | "Backspace"
            | "Delete"
            | "Insert"
            | "Home"
            | "End"
            | "PageUp"
            | "PageDown"
            | "Up"
            | "Down"
            | "Left"
            | "Right"
            | "Escape"
            | "CapsLock"
            | "NumLock"
            | "ScrollLock"
            | "PrintScreen"
            | "Pause"
            | "Minus"
            | "Equal"
            | "BracketLeft"
            | "BracketRight"
            | "Semicolon"
            | "Quote"
            | "Comma"
            | "Period"
            | "Slash"
            | "Backslash"
            | "Backquote"
    )
}

fn parse_mouse_button(token: &str) -> Option<String> {
    match token {
        "Mouse1" | "MouseLeft" => Some("Mouse1".into()),
        "Mouse2" | "MouseRight" => Some("Mouse2".into()),
        "Mouse3" | "MouseMiddle" => Some("Mouse3".into()),
        "Mouse4" | "MouseSide" => Some("Mouse4".into()),
        "Mouse5" | "MouseExtra" => Some("Mouse5".into()),
        "Mouse6" | "MouseForward" => Some("Mouse6".into()),
        "Mouse7" | "MouseBack" => Some("Mouse7".into()),
        "Mouse8" | "MouseTask" => Some("Mouse8".into()),
        _ => None,
    }
}

fn canonical_mods(mods: &[Modifier]) -> Vec<Modifier> {
    let mut out: Vec<Modifier> = Vec::new();
    for (m, _) in MOD_ORDER {
        if mods.contains(m) && !out.contains(m) {
            out.push(*m);
        }
    }
    out
}

#[allow(dead_code)]
fn fmt_mods(mods: &[Modifier]) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for (m, name) in MOD_ORDER {
        if mods.contains(m) {
            parts.push(name);
        }
    }
    parts.join("+")
}

impl Binding {
    pub fn parse(s: &str) -> Result<Self, String> {
        if s.is_empty() {
            return Err("empty binding".into());
        }
        let parts: Vec<&str> = s.split('+').collect();
        if parts.iter().any(|p| p.is_empty()) {
            return Err(format!("invalid binding '{}': empty token", s));
        }
        let last = *parts.last().unwrap();
        let mod_tokens = &parts[..parts.len() - 1];
        let mut mods: Vec<Modifier> = Vec::new();
        for tok in mod_tokens {
            match parse_mod(tok) {
                Some(m) => mods.push(m),
                None => return Err(format!("'{}' is not a modifier", tok)),
            }
        }
        let mods = canonical_mods(&mods);
        if let Some(button) = parse_mouse_button(last) {
            return Ok(Binding::Mouse { mods, button });
        }
        if is_valid_key(last) {
            return Ok(Binding::Keyboard {
                mods,
                key: last.to_string(),
            });
        }
        Err(format!(
            "'{}' is not a recognized key or mouse button",
            last
        ))
    }

    #[allow(dead_code)]
    pub fn format(&self) -> String {
        match self {
            Binding::Keyboard { mods, key } => {
                let mods_s = fmt_mods(mods);
                if mods_s.is_empty() {
                    key.clone()
                } else {
                    format!("{}+{}", mods_s, key)
                }
            }
            Binding::Mouse { mods, button } => {
                let mods_s = fmt_mods(mods);
                if mods_s.is_empty() {
                    button.clone()
                } else {
                    format!("{}+{}", mods_s, button)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt(s: &str) -> String {
        Binding::parse(s).unwrap().format()
    }

    #[test]
    fn keyboard_plain() {
        assert_eq!(rt("F12"), "F12");
    }
    #[test]
    fn keyboard_one_mod() {
        assert_eq!(rt("Ctrl+F12"), "Ctrl+F12");
    }
    #[test]
    fn keyboard_three_mods() {
        assert_eq!(rt("Ctrl+Shift+Alt+F12"), "Ctrl+Shift+Alt+F12");
    }
    #[test]
    fn keyboard_named() {
        assert_eq!(rt("ScrollLock"), "ScrollLock");
    }
    #[test]
    fn mouse_plain() {
        assert_eq!(rt("Mouse5"), "Mouse5");
    }
    #[test]
    fn mouse_with_mod() {
        assert_eq!(rt("Ctrl+Mouse5"), "Ctrl+Mouse5");
    }
    #[test]
    fn mod_order_normalized() {
        assert_eq!(rt("Shift+Ctrl+F1"), "Ctrl+Shift+F1");
    }
    #[test]
    fn invalid_returns_err() {
        assert!(Binding::parse("").is_err());
        assert!(Binding::parse("Bogus+F12").is_err());
        assert!(Binding::parse("Ctrl+").is_err());
    }
}
