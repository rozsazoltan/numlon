use anyhow::Result;
use std::{
    ptr,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};
use windows_sys::Win32::{
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
            KEYEVENTF_UNICODE, VK_CLEAR, VK_DELETE, VK_DOWN, VK_END, VK_HOME, VK_INSERT, VK_LEFT,
            VK_NEXT, VK_NUMLOCK, VK_PRIOR, VK_RIGHT, VK_UP,
        },
        WindowsAndMessaging::{
            CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT,
            LLKHF_EXTENDED, LLKHF_INJECTED, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN,
            WM_SYSKEYUP,
        },
    },
};

static REMAP_ACTIVE: AtomicBool = AtomicBool::new(false);
static SUPPRESSED_KEYS: AtomicU32 = AtomicU32::new(0);

pub struct KeyboardHook {
    handle: HHOOK,
}

impl KeyboardHook {
    pub fn install() -> Result<Self> {
        let module = unsafe { GetModuleHandleW(ptr::null()) };
        let handle =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), module, 0) };

        if handle.is_null() {
            anyhow::bail!("failed to install low-level keyboard hook");
        }

        Ok(Self { handle })
    }

    pub fn set_remap_active(active: bool) {
        REMAP_ACTIVE.store(active, Ordering::Release);
        if !active {
            SUPPRESSED_KEYS.store(0, Ordering::Release);
        }
    }
}

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        Self::set_remap_active(false);
        if !self.handle.is_null() {
            unsafe {
                UnhookWindowsHookEx(self.handle);
            }
        }
    }
}

unsafe extern "system" fn low_level_keyboard_proc(
    code: i32,
    wparam: usize,
    lparam: isize,
) -> isize {
    if code < 0 || !REMAP_ACTIVE.load(Ordering::Acquire) {
        return CallNextHookEx(ptr::null_mut(), code, wparam, lparam);
    }

    let event = &*(lparam as *const KBDLLHOOKSTRUCT);
    if event.flags & LLKHF_INJECTED != 0 {
        return CallNextHookEx(ptr::null_mut(), code, wparam, lparam);
    }

    let message = wparam as u32;
    let is_key_down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
    let is_key_up = message == WM_KEYUP || message == WM_SYSKEYUP;

    if !is_key_down && !is_key_up {
        return CallNextHookEx(ptr::null_mut(), code, wparam, lparam);
    }

    if event.vkCode == VK_NUMLOCK as u32 {
        return 1;
    }

    if event.flags & LLKHF_EXTENDED != 0 {
        return CallNextHookEx(ptr::null_mut(), code, wparam, lparam);
    }

    let Some((character, bit)) = remapped_character(event.vkCode) else {
        return CallNextHookEx(ptr::null_mut(), code, wparam, lparam);
    };

    if is_key_down {
        if send_unicode_character(character) {
            SUPPRESSED_KEYS.fetch_or(bit, Ordering::AcqRel);
            return 1;
        }

        return CallNextHookEx(ptr::null_mut(), code, wparam, lparam);
    }

    let suppressed = SUPPRESSED_KEYS.fetch_and(!bit, Ordering::AcqRel) & bit != 0;
    if suppressed {
        1
    } else {
        CallNextHookEx(ptr::null_mut(), code, wparam, lparam)
    }
}

fn remapped_character(virtual_key: u32) -> Option<(char, u32)> {
    let mapping = match virtual_key {
        value if value == VK_INSERT as u32 => ('0', 1 << 0),
        value if value == VK_END as u32 => ('1', 1 << 1),
        value if value == VK_DOWN as u32 => ('2', 1 << 2),
        value if value == VK_NEXT as u32 => ('3', 1 << 3),
        value if value == VK_LEFT as u32 => ('4', 1 << 4),
        value if value == VK_CLEAR as u32 => ('5', 1 << 5),
        value if value == VK_RIGHT as u32 => ('6', 1 << 6),
        value if value == VK_HOME as u32 => ('7', 1 << 7),
        value if value == VK_UP as u32 => ('8', 1 << 8),
        value if value == VK_PRIOR as u32 => ('9', 1 << 9),
        value if value == VK_DELETE as u32 => ('.', 1 << 10),
        _ => return None,
    };

    Some(mapping)
}

unsafe fn send_unicode_character(character: char) -> bool {
    let code_unit = character as u32;
    if code_unit > u16::MAX as u32 {
        return false;
    }

    let mut inputs = [
        unicode_input(code_unit as u16, 0),
        unicode_input(code_unit as u16, KEYEVENTF_KEYUP),
    ];
    SendInput(
        inputs.len() as u32,
        inputs.as_mut_ptr(),
        std::mem::size_of::<INPUT>() as i32,
    ) == inputs.len() as u32
}

fn unicode_input(code_unit: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: 0,
                wScan: code_unit,
                dwFlags: KEYEVENTF_UNICODE | flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
