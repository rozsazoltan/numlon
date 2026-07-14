use anyhow::{Context, Result};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_NUMLOCK,
};

pub fn is_numlock_on() -> bool {
    unsafe { (GetKeyState(VK_NUMLOCK as i32) & 1) != 0 }
}

pub fn ensure_numlock_on() -> Result<bool> {
    if is_numlock_on() {
        return Ok(false);
    }

    tap_numlock().context("failed to turn NumLock on")?;
    Ok(true)
}

fn tap_numlock() -> Result<()> {
    let key = VK_NUMLOCK as VIRTUAL_KEY;
    let mut inputs = [
        keyboard_input(key, KEYEVENTF_EXTENDEDKEY),
        keyboard_input(key, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP),
    ];

    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_mut_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };

    if sent != inputs.len() as u32 {
        anyhow::bail!("SendInput sent {sent} of {} NumLock events", inputs.len());
    }

    Ok(())
}

fn keyboard_input(key: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
