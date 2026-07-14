use serde::{Deserialize, Serialize};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, VK_CONTROL, VK_DELETE,
    VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_F24, VK_HOME, VK_INSERT, VK_LEFT, VK_LWIN, VK_MENU,
    VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_RWIN, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};

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

    pub fn modifiers(&self) -> u32 {
        let mut modifiers = MOD_NOREPEAT;
        if self.ctrl {
            modifiers |= MOD_CONTROL;
        }
        if self.alt {
            modifiers |= MOD_ALT;
        }
        if self.shift {
            modifiers |= MOD_SHIFT;
        }
        if self.win {
            modifiers |= MOD_WIN;
        }
        modifiers
    }

    pub fn virtual_key(&self) -> Option<u32> {
        key_name_to_virtual_key(&self.key)
    }

    pub fn from_key_event(virtual_key: u32) -> Option<Self> {
        if is_modifier_key(virtual_key) {
            return None;
        }

        let key = virtual_key_to_key_name(virtual_key)?;
        Some(Self {
            ctrl: key_is_down(VK_CONTROL),
            alt: key_is_down(VK_MENU),
            shift: key_is_down(VK_SHIFT),
            win: key_is_down(VK_LWIN) || key_is_down(VK_RWIN),
            key,
        })
    }
}

fn default_key() -> String {
    "Home".to_owned()
}

fn key_is_down(key: u16) -> bool {
    unsafe { GetKeyState(key as i32) < 0 }
}

fn is_modifier_key(virtual_key: u32) -> bool {
    matches!(
        virtual_key,
        value if value == VK_CONTROL as u32
            || value == VK_MENU as u32
            || value == VK_SHIFT as u32
            || value == VK_LWIN as u32
            || value == VK_RWIN as u32
    )
}

fn key_name_to_virtual_key(name: &str) -> Option<u32> {
    let normalized = name.trim().to_ascii_uppercase();

    if normalized.len() == 1 {
        let byte = normalized.as_bytes()[0];
        if byte.is_ascii_alphanumeric() {
            return Some(byte as u32);
        }
    }

    if let Some(function_number) = normalized.strip_prefix('F') {
        let number = function_number.parse::<u32>().ok()?;
        if (1..=24).contains(&number) {
            return Some(VK_F1 as u32 + number - 1);
        }
    }

    match normalized.as_str() {
        "HOME" => Some(VK_HOME as u32),
        "END" => Some(VK_END as u32),
        "PAGEUP" | "PGUP" => Some(VK_PRIOR as u32),
        "PAGEDOWN" | "PGDN" => Some(VK_NEXT as u32),
        "INSERT" | "INS" => Some(VK_INSERT as u32),
        "DELETE" | "DEL" => Some(VK_DELETE as u32),
        "LEFT" => Some(VK_LEFT as u32),
        "RIGHT" => Some(VK_RIGHT as u32),
        "UP" => Some(VK_UP as u32),
        "DOWN" => Some(VK_DOWN as u32),
        "SPACE" => Some(VK_SPACE as u32),
        "TAB" => Some(VK_TAB as u32),
        "ENTER" | "RETURN" => Some(VK_RETURN as u32),
        "ESCAPE" | "ESC" => Some(VK_ESCAPE as u32),
        _ => None,
    }
}

fn virtual_key_to_key_name(virtual_key: u32) -> Option<String> {
    if (b'A' as u32..=b'Z' as u32).contains(&virtual_key)
        || (b'0' as u32..=b'9' as u32).contains(&virtual_key)
    {
        return char::from_u32(virtual_key).map(|character| character.to_string());
    }

    if (VK_F1 as u32..=VK_F24 as u32).contains(&virtual_key) {
        return Some(format!("F{}", virtual_key - VK_F1 as u32 + 1));
    }

    let name = match virtual_key {
        value if value == VK_HOME as u32 => "Home",
        value if value == VK_END as u32 => "End",
        value if value == VK_PRIOR as u32 => "PageUp",
        value if value == VK_NEXT as u32 => "PageDown",
        value if value == VK_INSERT as u32 => "Insert",
        value if value == VK_DELETE as u32 => "Delete",
        value if value == VK_LEFT as u32 => "Left",
        value if value == VK_RIGHT as u32 => "Right",
        value if value == VK_UP as u32 => "Up",
        value if value == VK_DOWN as u32 => "Down",
        value if value == VK_SPACE as u32 => "Space",
        value if value == VK_TAB as u32 => "Tab",
        value if value == VK_RETURN as u32 => "Enter",
        value if value == VK_ESCAPE as u32 => "Escape",
        _ => return None,
    };

    Some(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::HotkeyBinding;

    #[test]
    fn default_binding_is_readable() {
        assert_eq!(HotkeyBinding::default().display(), "Win+Alt+Home");
    }

    #[test]
    fn function_key_resolves() {
        let binding = HotkeyBinding {
            key: "F12".to_owned(),
            ..HotkeyBinding::default()
        };
        assert!(binding.virtual_key().is_some());
    }
}
