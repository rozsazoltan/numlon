use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HotkeyBinding {
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub win: bool,
    #[serde(default = "default_key")]
    pub key: String,
}

impl Default for HotkeyBinding {
    fn default() -> Self {
        Self {
            ctrl: false,
            alt: true,
            shift: false,
            win: true,
            key: default_key(),
        }
    }
}

impl HotkeyBinding {
    pub fn display(&self) -> String {
        let mut parts = Vec::with_capacity(5);
        if self.win {
            parts.push("Win".to_owned());
        }
        if self.ctrl {
            parts.push("Ctrl".to_owned());
        }
        if self.alt {
            parts.push("Alt".to_owned());
        }
        if self.shift {
            parts.push("Shift".to_owned());
        }
        parts.push(self.key.clone());
        parts.join("+")
    }

    #[cfg(windows)]
    pub fn to_global_hotkey(&self) -> Result<global_hotkey::hotkey::HotKey, String> {
        let mut parts = Vec::with_capacity(5);
        if self.ctrl {
            parts.push("control".to_owned());
        }
        if self.alt {
            parts.push("alt".to_owned());
        }
        if self.shift {
            parts.push("shift".to_owned());
        }
        if self.win {
            parts.push("super".to_owned());
        }

        let normalized = self.key.trim().to_ascii_uppercase();
        let code = if normalized.len() == 1 {
            let byte = normalized.as_bytes()[0];
            if byte.is_ascii_alphabetic() {
                format!("Key{}", byte as char)
            } else if byte.is_ascii_digit() {
                format!("Digit{}", byte as char)
            } else {
                return Err(format!("Unsupported shortcut key: {}", self.key));
            }
        } else if normalized.starts_with('F')
            && normalized[1..]
                .parse::<u8>()
                .is_ok_and(|number| (1..=24).contains(&number))
        {
            normalized
        } else {
            match normalized.as_str() {
                "HOME" => "Home".to_owned(),
                "END" => "End".to_owned(),
                "PAGEUP" | "PGUP" => "PageUp".to_owned(),
                "PAGEDOWN" | "PGDN" => "PageDown".to_owned(),
                "INSERT" | "INS" => "Insert".to_owned(),
                "DELETE" | "DEL" => "Delete".to_owned(),
                "LEFT" => "ArrowLeft".to_owned(),
                "RIGHT" => "ArrowRight".to_owned(),
                "UP" => "ArrowUp".to_owned(),
                "DOWN" => "ArrowDown".to_owned(),
                "SPACE" => "Space".to_owned(),
                "TAB" => "Tab".to_owned(),
                "ENTER" | "RETURN" => "Enter".to_owned(),
                "ESCAPE" | "ESC" => "Escape".to_owned(),
                _ => return Err(format!("Unsupported shortcut key: {}", self.key)),
            }
        };

        parts.push(code);
        parts
            .join("+")
            .parse()
            .map_err(|error| format!("Invalid shortcut {}: {error}", self.display()))
    }
}

fn default_key() -> String {
    "Home".to_owned()
}

#[cfg(test)]
mod tests {
    use super::HotkeyBinding;

    #[test]
    fn default_binding_is_readable() {
        assert_eq!(HotkeyBinding::default().display(), "Win+Alt+Home");
    }

    #[cfg(windows)]
    #[test]
    fn default_binding_converts_to_global_hotkey() {
        assert!(HotkeyBinding::default().to_global_hotkey().is_ok());
    }
}
