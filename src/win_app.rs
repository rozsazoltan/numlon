use anyhow::Result;
use std::{
    env, mem,
    ptr,
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{GetLastError, COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::{
        Dwm::{
            DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
            DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
        },
        Gdi::{
            BeginPaint, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW,
            Ellipse, EndPaint, FillRect, RoundRect, SelectObject, SetBkMode, SetTextColor,
            HBRUSH, HDC, HGDIOBJ, PAINTSTRUCT, PS_SOLID, TRANSPARENT, DT_CENTER,
            DT_END_ELLIPSIS, DT_LEFT, DT_SINGLELINE, DT_TOP, DT_VCENTER, DT_WORDBREAK,
        },
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Input::KeyboardAndMouse::{
            RegisterHotKey, UnregisterHotKey, VK_ESCAPE,
        },
        Shell::{
            Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
            NOTIFYICONDATAW,
        },
        WindowsAndMessaging::{
            AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
            DestroyWindow, DispatchMessageW, DrawIconEx, FindWindowW, GetClientRect, GetCursorPos,
            GetMessageW, GetWindowLongPtrW,
            GetWindowRect, InvalidateRect, IsIconic, LoadCursorW, LoadIconW, MessageBoxW,
            PostMessageW, PostQuitMessage, RegisterClassW, SetForegroundWindow, SetMenuDefaultItem,
            SetTimer, SetWindowLongPtrW, ShowWindow, TrackPopupMenu, TranslateMessage,
            CW_USEDEFAULT, DI_NORMAL, GWLP_USERDATA, HMENU, IDC_ARROW, IDI_APPLICATION,
            MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, MSG,
            SW_HIDE, SW_RESTORE, SW_SHOW, TPM_RIGHTBUTTON, WM_APP, WM_CLOSE, WM_COMMAND,
            WM_DESTROY, WM_ERASEBKGND, WM_HOTKEY, WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONUP,
            WM_NCDESTROY,
            WM_PAINT, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_TIMER, WNDCLASSW, WS_CAPTION,
            WS_EX_APPWINDOW, WS_OVERLAPPED, WS_SYSMENU,
        },
    },
};

use crate::{
    config::{self, NumlockMode, SavedState},
    hotkey::HotkeyBinding,
    keyboard_hook::KeyboardHook,
    numlock, startup, updater,
    wide::{copy_wide_truncated, str_wide_null},
};

pub const CLASS_NAME: &str = "NumlonWindowClass";

const WM_TRAY_ICON: u32 = WM_APP + 1;
const WM_SHOW_EXISTING: u32 = WM_APP + 2;
const TRAY_ID: u32 = 1;
const HOTKEY_TOGGLE_ALWAYS: i32 = 1;
const APP_ICON_RESOURCE_ID: u16 = 1;
const PAUSED_ICON_RESOURCE_ID: u16 = 2;
const TIMER_ENFORCE: usize = 1;
const TIMER_POLL_UPDATES: usize = 2;
const ENFORCE_INTERVAL_MS: u32 = 300;
const UPDATE_POLL_INTERVAL_MS: u32 = 250;
const AUTO_UPDATE_INTERVAL_SECONDS: u64 = 60 * 60;

const WINDOW_WIDTH: i32 = 660;
const WINDOW_HEIGHT: i32 = 800;

const MENU_OPEN: usize = 2001;
const MENU_TOGGLE_ENABLED: usize = 2002;
const MENU_MODE_FORCE_ON: usize = 2003;
const MENU_MODE_LED_OFF: usize = 2004;
const MENU_CHANGE_SHORTCUT: usize = 2005;
const MENU_TOGGLE_STARTUP: usize = 2006;
const MENU_TOGGLE_PRERELEASES: usize = 2007;
const MENU_CHECK_UPDATES: usize = 2008;
const MENU_INSTALL_UPDATE: usize = 2009;
const MENU_OPEN_RELEASES: usize = 2010;
const MENU_EXIT: usize = 2011;

const ENABLED_SWITCH: UiRect = UiRect::new(540, 128, 596, 158);
const MODE_FORCE_ROW: UiRect = UiRect::new(36, 246, 604, 298);
const MODE_LED_ROW: UiRect = UiRect::new(36, 306, 604, 358);
const HOTKEY_BUTTON: UiRect = UiRect::new(480, 418, 592, 456);
const STARTUP_SWITCH: UiRect = UiRect::new(540, 524, 596, 554);
const UPDATE_CHANNEL_SWITCH: UiRect = UiRect::new(540, 622, 596, 652);
const UPDATE_ACTION_BUTTON: UiRect = UiRect::new(426, 616, 522, 658);
const HIDE_BUTTON: UiRect = UiRect::new(510, 700, 604, 738);

pub fn started_from_startup() -> bool {
    env::args_os().any(|argument| argument == "--startup")
}

pub fn activate_existing_instance() {
    let class_name = str_wide_null(CLASS_NAME);

    for _ in 0..20 {
        let hwnd = unsafe { FindWindowW(class_name.as_ptr(), ptr::null()) };
        if !hwnd.is_null() {
            unsafe {
                PostMessageW(hwnd, WM_SHOW_EXISTING, 0, 0);
            }
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

pub fn run() -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(ptr::null());
        let class_name = str_wide_null(CLASS_NAME);
        let title = str_wide_null(&config::window_title());
        let initial_state = config::load_state();

        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: load_app_icon(),
            hCursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
            hbrBackground: ptr::null_mut(),
            lpszMenuName: ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };

        RegisterClassW(&class);

        let x = if initial_state.window_x == i32::MIN {
            CW_USEDEFAULT
        } else {
            initial_state.window_x
        };
        let y = if initial_state.window_y == i32::MIN {
            CW_USEDEFAULT
        } else {
            initial_state.window_y
        };

        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
            x,
            y,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            ptr::null_mut(),
            ptr::null_mut(),
            instance,
            ptr::null_mut(),
        );

        if hwnd.is_null() {
            anyhow::bail!("failed to create Numlon window: {}", GetLastError());
        }

        style_window(hwnd);

        let mut app = Box::new(App::new(hwnd, initial_state));
        app.sync_startup_state();
        app.install_keyboard_hook();

        let app_ptr = Box::into_raw(app);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, app_ptr as isize);

        if let Some(app) = app_from_hwnd(hwnd) {
            app.add_tray_icon();
            app.register_saved_hotkey();
            app.apply_runtime_mode();
            app.prompt_startup_on_first_run();
            app.maybe_start_auto_update_check();
            app.repaint();

            if config::is_dev_build() || !started_from_startup() {
                app.show_window();
            }
        }

        SetTimer(hwnd, TIMER_ENFORCE, ENFORCE_INTERVAL_MS, None);
        if !config::is_dev_build() {
            SetTimer(hwnd, TIMER_POLL_UPDATES, UPDATE_POLL_INTERVAL_MS, None);
        }

        let mut message = MSG::default();
        while GetMessageW(&mut message, ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }

    }

    Ok(())
}

struct App {
    hwnd: HWND,
    state: SavedState,
    keyboard_hook: Option<KeyboardHook>,
    hotkey_registered: bool,
    capturing_hotkey: bool,
    last_update_check: Option<updater::UpdateCheck>,
    update_rx: Option<Receiver<anyhow::Result<updater::UpdateCheck>>>,
    status: String,
}

impl App {
    fn new(hwnd: HWND, state: SavedState) -> Self {
        let status = if state.last_status.is_empty() {
            "Ready.".to_owned()
        } else {
            state.last_status.clone()
        };

        Self {
            hwnd,
            state,
            keyboard_hook: None,
            hotkey_registered: false,
            capturing_hotkey: false,
            last_update_check: None,
            update_rx: None,
            status,
        }
    }

    fn install_keyboard_hook(&mut self) {
        match KeyboardHook::install() {
            Ok(hook) => self.keyboard_hook = Some(hook),
            Err(error) => {
                self.status =
                    format!("LED-off digit mode unavailable: keyboard hook failed: {error}");
            }
        }
    }

    fn sync_startup_state(&mut self) {
        match startup::is_enabled() {
            Ok(enabled) => self.state.startup_enabled = enabled,
            Err(error) => self.status = format!("Startup check failed: {error}"),
        }
    }

    fn save(&mut self) {
        self.remember_window_position();
        self.state.last_status = self.status.clone();
        if let Err(error) = config::save_state(&self.state) {
            self.status = format!("State save failed: {error}");
        }
    }

    fn remember_window_position(&mut self) {
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(self.hwnd, &mut rect) } != 0 {
            self.state.window_x = rect.left;
            self.state.window_y = rect.top;
        }
    }

    unsafe fn add_tray_icon(&self) {
        let mut data = self.tray_data();
        Shell_NotifyIconW(NIM_ADD, &mut data);
    }

    unsafe fn update_tray_icon(&self) {
        let mut data = self.tray_data();
        Shell_NotifyIconW(NIM_MODIFY, &mut data);
    }

    unsafe fn remove_tray_icon(&self) {
        let mut data = self.tray_data();
        Shell_NotifyIconW(NIM_DELETE, &mut data);
    }

    unsafe fn tray_data(&self) -> NOTIFYICONDATAW {
        let mut data: NOTIFYICONDATAW = mem::zeroed();
        data.cbSize = mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = self.hwnd;
        data.uID = TRAY_ID;
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.uCallbackMessage = WM_TRAY_ICON;
        data.hIcon = load_tray_icon(self.state.always_enabled);

        let state = if self.state.always_enabled {
            self.state.numlock_mode.label()
        } else {
            "Paused"
        };
        let tooltip = format!(
            "{} {} — {} — {}",
            config::app_name(),
            config::app_version_label(),
            state,
            self.state.hotkey.display()
        );
        copy_wide_truncated(&mut data.szTip, &tooltip);
        data
    }

    unsafe fn register_saved_hotkey(&mut self) {
        match self.register_hotkey_binding(&self.state.hotkey.clone()) {
            Ok(()) => self.hotkey_registered = true,
            Err(error) => {
                self.hotkey_registered = false;
                self.status = error;
            }
        }
    }

    unsafe fn register_hotkey_binding(&self, binding: &HotkeyBinding) -> Result<(), String> {
        let Some(virtual_key) = binding.virtual_key() else {
            return Err(format!("Unsupported shortcut key: {}", binding.key));
        };

        let ok = RegisterHotKey(
            self.hwnd,
            HOTKEY_TOGGLE_ALWAYS,
            binding.modifiers(),
            virtual_key,
        );
        if ok == 0 {
            return Err(format!(
                "Shortcut {} is unavailable. Windows error: {}",
                binding.display(),
                GetLastError()
            ));
        }

        Ok(())
    }

    unsafe fn unregister_hotkey(&mut self) {
        if self.hotkey_registered {
            UnregisterHotKey(self.hwnd, HOTKEY_TOGGLE_ALWAYS);
            self.hotkey_registered = false;
        }
    }

    unsafe fn begin_hotkey_capture(&mut self) {
        if self.capturing_hotkey {
            return;
        }

        self.unregister_hotkey();
        self.capturing_hotkey = true;
        self.status = "Press new shortcut. Esc cancels.".to_owned();
        self.repaint();
    }

    unsafe fn capture_hotkey(&mut self, virtual_key: u32) {
        if !self.capturing_hotkey {
            return;
        }

        if virtual_key == VK_ESCAPE as u32 {
            self.capturing_hotkey = false;
            self.register_saved_hotkey();
            self.status = "Shortcut change cancelled.".to_owned();
            self.repaint();
            return;
        }

        let Some(candidate) = HotkeyBinding::from_key_event(virtual_key) else {
            return;
        };

        match self.register_hotkey_binding(&candidate) {
            Ok(()) => {
                self.hotkey_registered = true;
                self.capturing_hotkey = false;
                self.state.hotkey = candidate;
                self.status = format!("Shortcut saved: {}.", self.state.hotkey.display());
                self.save();
                self.repaint();
                self.update_tray_icon();
            }
            Err(error) => {
                self.capturing_hotkey = false;
                self.register_saved_hotkey();
                self.status = error;
                self.repaint();
            }
        }
    }

    unsafe fn prompt_startup_on_first_run(&mut self) {
        if config::is_dev_build() || self.state.startup_prompted {
            return;
        }

        let result = message_box(
            self.hwnd,
            "Enable Numlon at Windows startup?\n\nMove numlon.exe to its final folder first. Do not move it afterward because Windows stores its exact path.",
            "Numlon startup",
            windows_sys::Win32::UI::WindowsAndMessaging::MB_YESNO
                | windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONWARNING,
        );
        self.state.startup_prompted = true;

        if result == windows_sys::Win32::UI::WindowsAndMessaging::IDYES {
            self.set_startup_enabled(true);
        } else {
            self.save();
        }
    }

    unsafe fn toggle_enabled(&mut self) {
        self.state.always_enabled = !self.state.always_enabled;
        self.apply_runtime_mode();

        self.status = if self.state.always_enabled {
            format!("Enabled: {}.", self.state.numlock_mode.label())
        } else {
            "Numlon paused. NumLock left untouched.".to_owned()
        };

        self.save();
        self.repaint();
        self.update_tray_icon();
    }

    unsafe fn set_numlock_mode(&mut self, mode: NumlockMode) {
        if mode == NumlockMode::LedOffDigits && self.keyboard_hook.is_none() {
            self.status = "LED-off digit mode unavailable: keyboard hook could not start.".to_owned();
            self.repaint();
            return;
        }

        self.state.numlock_mode = mode;
        self.apply_runtime_mode();
        self.status = format!("Mode changed: {}.", mode.label());
        self.save();
        self.repaint();
        self.update_tray_icon();
    }

    unsafe fn apply_runtime_mode(&mut self) {
        if !self.state.always_enabled {
            KeyboardHook::set_remap_active(false);
            return;
        }

        match self.state.numlock_mode {
            NumlockMode::ForceOn => {
                KeyboardHook::set_remap_active(false);
                if let Err(error) = numlock::ensure_numlock_on() {
                    self.status = format!("NumLock enable failed: {error}");
                }
            }
            NumlockMode::LedOffDigits => {
                if self.keyboard_hook.is_none() {
                    KeyboardHook::set_remap_active(false);
                    self.status =
                        "LED-off digit mode unavailable: keyboard hook could not start.".to_owned();
                    return;
                }

                if let Err(error) = numlock::ensure_numlock_off() {
                    self.status = format!("NumLock disable failed: {error}");
                    return;
                }
                KeyboardHook::set_remap_active(true);
            }
        }
    }

    unsafe fn enforce_numlock(&mut self) {
        if !self.state.always_enabled {
            return;
        }

        let result = match self.state.numlock_mode {
            NumlockMode::ForceOn => numlock::ensure_numlock_on(),
            NumlockMode::LedOffDigits => numlock::ensure_numlock_off(),
        };

        match result {
            Ok(true) => {
                self.status = match self.state.numlock_mode {
                    NumlockMode::ForceOn => "NumLock restored.".to_owned(),
                    NumlockMode::LedOffDigits => "NumLock turned off; keypad digit remap active.".to_owned(),
                };
                self.repaint();
            }
            Ok(false) => {}
            Err(error) => {
                self.status = format!("NumLock state update failed: {error}");
                self.repaint();
            }
        }
    }

    unsafe fn toggle_startup(&mut self) {
        if config::is_dev_build() {
            self.status = "Startup changes disabled in dev builds.".to_owned();
            self.repaint();
            return;
        }

        let target = !self.state.startup_enabled;
        if target {
            let result = message_box(
                self.hwnd,
                "Move numlon.exe to its final folder first. Do not move it afterward because Windows stores its exact path.\n\nEnable startup now?",
                "Numlon startup",
                windows_sys::Win32::UI::WindowsAndMessaging::MB_YESNO
                    | windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONWARNING,
            );
            if result != windows_sys::Win32::UI::WindowsAndMessaging::IDYES {
                return;
            }
        }

        self.set_startup_enabled(target);
        self.repaint();
        self.update_tray_icon();
    }

    fn set_startup_enabled(&mut self, enabled: bool) {
        match startup::set_enabled(enabled) {
            Ok(()) => {
                self.state.startup_enabled = enabled;
                self.status = if enabled {
                    "Startup enabled.".to_owned()
                } else {
                    "Startup disabled.".to_owned()
                };
            }
            Err(error) => self.status = format!("Startup update failed: {error}"),
        }
        self.save();
    }

    unsafe fn toggle_prerelease_updates(&mut self) {
        if config::is_dev_build() {
            self.status = "Update checks disabled in dev builds.".to_owned();
            self.repaint();
            return;
        }

        self.state.include_prereleases = !self.state.include_prereleases;
        self.last_update_check = None;
        self.status = if self.state.include_prereleases {
            "Prerelease update channel selected.".to_owned()
        } else {
            "Stable update channel selected.".to_owned()
        };
        self.save();
        self.repaint();
    }

    unsafe fn maybe_start_auto_update_check(&mut self) {
        if config::is_dev_build() || self.update_rx.is_some() {
            return;
        }

        let now = config::seconds_since_unix_epoch();
        if now.saturating_sub(self.state.last_auto_update_check_unix_seconds)
            < AUTO_UPDATE_INTERVAL_SECONDS
        {
            return;
        }

        self.state.last_auto_update_check_unix_seconds = now;
        self.save();
        self.start_update_check();
    }

    unsafe fn start_update_check(&mut self) {
        if config::is_dev_build() {
            self.status = "Update checks disabled in dev builds.".to_owned();
            self.repaint();
            return;
        }

        if self.update_rx.is_some() {
            return;
        }

        let include_prereleases = self.state.include_prereleases;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let result = updater::check_for_update(include_prereleases);
            let _ = tx.send(result);
        });

        self.update_rx = Some(rx);
        self.status = if include_prereleases {
            "Checking prerelease updates...".to_owned()
        } else {
            "Checking stable updates...".to_owned()
        };
        self.repaint();
    }

    unsafe fn poll_update_check(&mut self) {
        let Some(rx) = self.update_rx.as_ref() else {
            return;
        };

        match rx.try_recv() {
            Ok(Ok(check)) => {
                self.status = update_status(&check);
                self.last_update_check = Some(check);
                self.update_rx = None;
                self.save();
                self.repaint();
            }
            Ok(Err(error)) => {
                self.status = format!("Update check failed: {error}");
                self.update_rx = None;
                self.save();
                self.repaint();
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.status = "Update check failed: worker disconnected.".to_owned();
                self.update_rx = None;
                self.save();
                self.repaint();
            }
        }
    }

    unsafe fn install_update(&mut self) {
        if config::is_dev_build() {
            self.status = "Updates disabled in dev builds.".to_owned();
            self.repaint();
            return;
        }

        let Some(check) = self.last_update_check.clone() else {
            self.status = "Check for updates first.".to_owned();
            self.repaint();
            return;
        };

        if !check.is_update_available || check.asset_download_url.is_none() {
            self.status = "No installable update available.".to_owned();
            self.repaint();
            return;
        }

        self.status = "Installing update...".to_owned();
        self.save();
        self.repaint();

        if let Err(error) = updater::install_update(&check) {
            self.status = format!("Update install failed: {error}");
            self.save();
            self.repaint();
        }
    }

    fn update_is_installable(&self) -> bool {
        self.last_update_check
            .as_ref()
            .map(|check| check.is_update_available && check.asset_download_url.is_some())
            .unwrap_or(false)
    }

    unsafe fn show_window(&self) {
        ShowWindow(
            self.hwnd,
            if IsIconic(self.hwnd) != 0 {
                SW_RESTORE
            } else {
                SW_SHOW
            },
        );
        SetForegroundWindow(self.hwnd);
        self.repaint();
    }

    unsafe fn hide_window(&mut self) {
        self.save();
        ShowWindow(self.hwnd, SW_HIDE);
    }

    unsafe fn repaint(&self) {
        InvalidateRect(self.hwnd, ptr::null(), 0);
    }

    unsafe fn handle_click(&mut self, x: i32, y: i32) {
        if ENABLED_SWITCH.contains(x, y) {
            self.toggle_enabled();
        } else if MODE_FORCE_ROW.contains(x, y) {
            self.set_numlock_mode(NumlockMode::ForceOn);
        } else if MODE_LED_ROW.contains(x, y) {
            self.set_numlock_mode(NumlockMode::LedOffDigits);
        } else if HOTKEY_BUTTON.contains(x, y) {
            self.begin_hotkey_capture();
        } else if STARTUP_SWITCH.contains(x, y) {
            self.toggle_startup();
        } else if !config::is_dev_build() && UPDATE_CHANNEL_SWITCH.contains(x, y) {
            self.toggle_prerelease_updates();
        } else if !config::is_dev_build() && UPDATE_ACTION_BUTTON.contains(x, y) {
            if self.update_is_installable() {
                self.install_update();
            } else {
                self.start_update_check();
            }
        } else if HIDE_BUTTON.contains(x, y) {
            self.hide_window();
        }
    }

    unsafe fn show_tray_menu(&mut self) {
        let menu = CreatePopupMenu();
        let mode_menu = CreatePopupMenu();

        append_menu(menu, MENU_OPEN, "Open Numlon", MF_STRING);
        append_separator(menu);
        append_menu(
            menu,
            MENU_TOGGLE_ENABLED,
            "Enabled",
            MF_STRING
                | if self.state.always_enabled {
                    MF_CHECKED
                } else {
                    MF_UNCHECKED
                },
        );

        append_menu(
            mode_menu,
            MENU_MODE_FORCE_ON,
            "Keep NumLock on",
            MF_STRING
                | if self.state.numlock_mode == NumlockMode::ForceOn {
                    MF_CHECKED
                } else {
                    MF_UNCHECKED
                },
        );
        append_menu(
            mode_menu,
            MENU_MODE_LED_OFF,
            "Keep LED off, type digits",
            MF_STRING
                | if self.state.numlock_mode == NumlockMode::LedOffDigits {
                    MF_CHECKED
                } else {
                    MF_UNCHECKED
                }
                | if self.keyboard_hook.is_none() {
                    MF_GRAYED
                } else {
                    0
                },
        );
        append_submenu(menu, mode_menu, "Behavior");

        let shortcut_label = format!("Change shortcut...  {}", self.state.hotkey.display());
        append_menu(menu, MENU_CHANGE_SHORTCUT, &shortcut_label, MF_STRING);
        append_menu(
            menu,
            MENU_TOGGLE_STARTUP,
            "Start with Windows",
            MF_STRING
                | if self.state.startup_enabled {
                    MF_CHECKED
                } else {
                    MF_UNCHECKED
                }
                | if config::is_dev_build() { MF_GRAYED } else { 0 },
        );

        if !config::is_dev_build() {
            append_separator(menu);
            append_menu(
                menu,
                MENU_TOGGLE_PRERELEASES,
                "Include prereleases",
                MF_STRING
                    | if self.state.include_prereleases {
                        MF_CHECKED
                    } else {
                        MF_UNCHECKED
                    },
            );
            append_menu(menu, MENU_CHECK_UPDATES, "Check for updates", MF_STRING);
            append_menu(
                menu,
                MENU_INSTALL_UPDATE,
                "Install available update",
                MF_STRING
                    | if self.update_is_installable() {
                        0
                    } else {
                        MF_GRAYED
                    },
            );
            append_menu(menu, MENU_OPEN_RELEASES, "Open releases", MF_STRING);
        }

        append_separator(menu);
        append_menu(menu, MENU_EXIT, "Quit Numlon", MF_STRING);
        SetMenuDefaultItem(menu, MENU_OPEN as u32, 0);

        let mut point = POINT::default();
        GetCursorPos(&mut point);
        SetForegroundWindow(self.hwnd);
        TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            self.hwnd,
            ptr::null(),
        );
        DestroyMenu(menu);
    }

    unsafe fn handle_command(&mut self, command_id: usize) {
        match command_id {
            MENU_OPEN => self.show_window(),
            MENU_TOGGLE_ENABLED => self.toggle_enabled(),
            MENU_MODE_FORCE_ON => self.set_numlock_mode(NumlockMode::ForceOn),
            MENU_MODE_LED_OFF => self.set_numlock_mode(NumlockMode::LedOffDigits),
            MENU_CHANGE_SHORTCUT => {
                self.show_window();
                self.begin_hotkey_capture();
            }
            MENU_TOGGLE_STARTUP => self.toggle_startup(),
            MENU_TOGGLE_PRERELEASES => self.toggle_prerelease_updates(),
            MENU_CHECK_UPDATES => self.start_update_check(),
            MENU_INSTALL_UPDATE => self.install_update(),
            MENU_OPEN_RELEASES => {
                if let Err(error) = updater::open_releases_page() {
                    self.status = format!("Open releases failed: {error}");
                    self.repaint();
                }
            }
            MENU_EXIT => {
                DestroyWindow(self.hwnd);
            }
            _ => {}
        }
    }

    unsafe fn paint(&self) {
        let mut paint = PAINTSTRUCT::default();
        let hdc = BeginPaint(self.hwnd, &mut paint);
        if hdc.is_null() {
            return;
        }

        let mut client = RECT::default();
        GetClientRect(self.hwnd, &mut client);
        fill_rect(hdc, client, rgb(247, 247, 244));

        draw_header(hdc);
        self.draw_enabled_card(hdc);
        self.draw_mode_card(hdc);
        self.draw_hotkey_card(hdc);
        self.draw_startup_card(hdc);
        self.draw_updates_card(hdc);
        self.draw_footer(hdc);

        EndPaint(self.hwnd, &paint);
    }

    unsafe fn draw_enabled_card(&self, hdc: HDC) {
        draw_card(hdc, UiRect::new(24, 102, 616, 190), rgb(255, 255, 255));
        draw_text(
            hdc,
            "NUMLOCK CONTROL",
            UiRect::new(40, 118, 410, 138),
            11,
            700,
            rgb(128, 128, 125),
            DT_LEFT | DT_SINGLELINE,
        );
        draw_text(
            hdc,
            if self.state.always_enabled {
                "Numlon is on"
            } else {
                "Numlon is paused"
            },
            UiRect::new(40, 140, 500, 168),
            20,
            700,
            rgb(28, 28, 30),
            DT_LEFT | DT_SINGLELINE,
        );
        draw_text(
            hdc,
            if self.state.always_enabled {
                self.state.numlock_mode.label()
            } else {
                "Keyboard state remains untouched"
            },
            UiRect::new(40, 168, 500, 186),
            12,
            400,
            rgb(104, 104, 102),
            DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        draw_switch(hdc, ENABLED_SWITCH, self.state.always_enabled);
    }

    unsafe fn draw_mode_card(&self, hdc: HDC) {
        draw_card(hdc, UiRect::new(24, 206, 616, 376), rgb(255, 255, 255));
        draw_text(
            hdc,
            "Behavior",
            UiRect::new(40, 220, 300, 242),
            15,
            700,
            rgb(28, 28, 30),
            DT_LEFT | DT_SINGLELINE,
        );

        draw_choice_row(
            hdc,
            MODE_FORCE_ROW,
            self.state.numlock_mode == NumlockMode::ForceOn,
            "Keep NumLock on",
            "Restores NumLock and keeps keypad in numeric mode.",
            true,
        );
        draw_choice_row(
            hdc,
            MODE_LED_ROW,
            self.state.numlock_mode == NumlockMode::LedOffDigits,
            "Keep LED off, type digits",
            "Remaps numpad navigation keys to 0–9 while NumLock stays off.",
            self.keyboard_hook.is_some(),
        );
    }

    unsafe fn draw_hotkey_card(&self, hdc: HDC) {
        draw_card(hdc, UiRect::new(24, 392, 616, 484), rgb(255, 255, 255));
        draw_text(
            hdc,
            "Toggle shortcut",
            UiRect::new(40, 408, 260, 430),
            15,
            700,
            rgb(28, 28, 30),
            DT_LEFT | DT_SINGLELINE,
        );
        draw_text(
            hdc,
            if self.capturing_hotkey {
                "Press shortcut now. Esc cancels."
            } else {
                &self.state.hotkey.display()
            },
            UiRect::new(40, 438, 458, 464),
            14,
            if self.capturing_hotkey { 700 } else { 500 },
            if self.capturing_hotkey {
                rgb(118, 88, 0)
            } else {
                rgb(72, 72, 70)
            },
            DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        draw_button(
            hdc,
            HOTKEY_BUTTON,
            if self.capturing_hotkey {
                "Listening…"
            } else {
                "Change"
            },
            true,
        );
    }

    unsafe fn draw_startup_card(&self, hdc: HDC) {
        draw_card(hdc, UiRect::new(24, 500, 616, 580), rgb(255, 255, 255));
        draw_text(
            hdc,
            "Start with Windows",
            UiRect::new(40, 516, 420, 540),
            15,
            700,
            rgb(28, 28, 30),
            DT_LEFT | DT_SINGLELINE,
        );
        draw_text(
            hdc,
            if config::is_dev_build() {
                "Disabled in dev builds"
            } else if self.state.startup_enabled {
                "Uses current executable path"
            } else {
                "Keep executable in final folder before enabling"
            },
            UiRect::new(40, 544, 500, 566),
            12,
            400,
            rgb(104, 104, 102),
            DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        draw_switch(
            hdc,
            STARTUP_SWITCH,
            self.state.startup_enabled && !config::is_dev_build(),
        );
    }

    unsafe fn draw_updates_card(&self, hdc: HDC) {
        draw_card(hdc, UiRect::new(24, 596, 616, 676), rgb(255, 255, 255));
        draw_text(
            hdc,
            "Updates",
            UiRect::new(40, 612, 240, 636),
            15,
            700,
            rgb(28, 28, 30),
            DT_LEFT | DT_SINGLELINE,
        );

        if config::is_dev_build() {
            draw_text(
                hdc,
                "Disabled in dev builds — no GitHub API requests.",
                UiRect::new(40, 640, 580, 662),
                12,
                400,
                rgb(104, 104, 102),
                DT_LEFT | DT_SINGLELINE,
            );
            return;
        }

        draw_text(
            hdc,
            if self.state.include_prereleases {
                "Prerelease channel"
            } else {
                "Stable channel"
            },
            UiRect::new(40, 640, 390, 662),
            12,
            400,
            rgb(104, 104, 102),
            DT_LEFT | DT_SINGLELINE,
        );
        draw_button(
            hdc,
            UPDATE_ACTION_BUTTON,
            if self.update_is_installable() {
                "Install"
            } else {
                "Check"
            },
            false,
        );
        draw_switch(hdc, UPDATE_CHANNEL_SWITCH, self.state.include_prereleases);
    }

    unsafe fn draw_footer(&self, hdc: HDC) {
        draw_text(
            hdc,
            &self.status,
            UiRect::new(28, 696, 492, 742),
            11,
            400,
            rgb(96, 96, 93),
            DT_LEFT | DT_TOP | DT_WORDBREAK | DT_END_ELLIPSIS,
        );
        draw_button(hdc, HIDE_BUTTON, "Hide", false);
    }
}

fn update_status(check: &updater::UpdateCheck) -> String {
    let kind = if check.prerelease {
        "prerelease"
    } else {
        "stable"
    };

    if check.is_update_available {
        if check.asset_download_url.is_some() {
            format!("Update available: {kind} v{}.", check.latest_version)
        } else {
            format!(
                "Update available: {kind} v{}, but no Windows executable asset was found.",
                check.latest_version
            )
        }
    } else {
        format!(
            "No newer {kind} release. Current version: v{}.",
            check.current_version
        )
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_PAINT => {
            if let Some(app) = app_from_hwnd(hwnd) {
                app.paint();
                0
            } else {
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
        }
        WM_ERASEBKGND => 1,
        WM_LBUTTONUP => {
            if let Some(app) = app_from_hwnd(hwnd) {
                let (x, y) = point_from_lparam(lparam);
                app.handle_click(x, y);
            }
            0
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if let Some(app) = app_from_hwnd(hwnd) {
                app.capture_hotkey(wparam as u32);
            }
            0
        }
        WM_COMMAND => {
            if let Some(app) = app_from_hwnd(hwnd) {
                app.handle_command((wparam & 0xffff) as usize);
            }
            0
        }
        WM_HOTKEY => {
            if wparam as i32 == HOTKEY_TOGGLE_ALWAYS {
                if let Some(app) = app_from_hwnd(hwnd) {
                    app.toggle_enabled();
                }
            }
            0
        }
        WM_TIMER => {
            if let Some(app) = app_from_hwnd(hwnd) {
                match wparam {
                    TIMER_ENFORCE => app.enforce_numlock(),
                    TIMER_POLL_UPDATES => app.poll_update_check(),
                    _ => {}
                }
            }
            0
        }
        WM_TRAY_ICON => {
            if let Some(app) = app_from_hwnd(hwnd) {
                match lparam as u32 {
                    WM_RBUTTONUP => app.show_tray_menu(),
                    WM_LBUTTONUP | WM_LBUTTONDBLCLK => app.show_window(),
                    _ => {}
                }
            }
            0
        }
        WM_SHOW_EXISTING => {
            if let Some(app) = app_from_hwnd(hwnd) {
                app.show_window();
            }
            0
        }
        WM_CLOSE => {
            if let Some(app) = app_from_hwnd(hwnd) {
                app.hide_window();
            }
            0
        }
        WM_DESTROY => {
            if let Some(app) = app_from_hwnd(hwnd) {
                app.unregister_hotkey();
                KeyboardHook::set_remap_active(false);
                app.remove_tray_icon();
                app.save();
            }
            PostQuitMessage(0);
            0
        }
        WM_NCDESTROY => {
            let app_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if !app_ptr.is_null() {
                drop(Box::from_raw(app_ptr));
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn style_window(hwnd: HWND) {
    let corner = DWMWCP_ROUND;
    let caption = rgb(247, 247, 244);
    let border = rgb(226, 226, 220);
    let text = rgb(28, 28, 30);

    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_WINDOW_CORNER_PREFERENCE,
        &corner as *const _ as *const _,
        mem::size_of_val(&corner) as u32,
    );
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_CAPTION_COLOR,
        &caption as *const _ as *const _,
        mem::size_of_val(&caption) as u32,
    );
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_BORDER_COLOR,
        &border as *const _ as *const _,
        mem::size_of_val(&border) as u32,
    );
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_TEXT_COLOR,
        &text as *const _ as *const _,
        mem::size_of_val(&text) as u32,
    );
}

unsafe fn draw_header(hdc: HDC) {
    DrawIconEx(
        hdc,
        28,
        24,
        load_app_icon(),
        52,
        52,
        0,
        ptr::null_mut(),
        DI_NORMAL,
    );
    draw_text(
        hdc,
        "Numlon",
        UiRect::new(94, 24, 410, 52),
        22,
        700,
        rgb(28, 28, 30),
        DT_LEFT | DT_SINGLELINE,
    );
    draw_text(
        hdc,
        "Tiny keypad control, without LED drama.",
        UiRect::new(94, 56, 470, 78),
        12,
        400,
        rgb(104, 104, 102),
        DT_LEFT | DT_SINGLELINE,
    );
    draw_pill(
        hdc,
        UiRect::new(500, 30, 610, 62),
        &config::app_version_label(),
    );
}

unsafe fn draw_card(hdc: HDC, rect: UiRect, fill: COLORREF) {
    draw_rounded_rect(hdc, rect, 18, fill, rgb(229, 229, 224));
}

unsafe fn draw_choice_row(
    hdc: HDC,
    rect: UiRect,
    selected: bool,
    title: &str,
    subtitle: &str,
    enabled: bool,
) {
    let fill = if selected {
        rgb(255, 250, 225)
    } else {
        rgb(250, 250, 248)
    };
    let border = if selected {
        rgb(250, 204, 21)
    } else {
        rgb(232, 232, 228)
    };
    draw_rounded_rect(hdc, rect, 13, fill, border);
    draw_radio(hdc, UiRect::new(rect.left + 14, rect.top + 16, rect.left + 34, rect.top + 36), selected);

    let title_color = if enabled {
        rgb(28, 28, 30)
    } else {
        rgb(150, 150, 146)
    };
    let subtitle_color = if enabled {
        rgb(104, 104, 102)
    } else {
        rgb(166, 166, 162)
    };

    draw_text(
        hdc,
        title,
        UiRect::new(rect.left + 48, rect.top + 8, rect.right - 16, rect.top + 28),
        13,
        700,
        title_color,
        DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
    draw_text(
        hdc,
        subtitle,
        UiRect::new(rect.left + 48, rect.top + 29, rect.right - 16, rect.bottom - 5),
        11,
        400,
        subtitle_color,
        DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
}

unsafe fn draw_switch(hdc: HDC, rect: UiRect, enabled: bool) {
    let track = if enabled {
        rgb(250, 204, 21)
    } else {
        rgb(214, 214, 210)
    };
    draw_rounded_rect(hdc, rect, 18, track, track);

    let knob_left = if enabled {
        rect.right - 27
    } else {
        rect.left + 3
    };
    draw_ellipse(
        hdc,
        UiRect::new(knob_left, rect.top + 3, knob_left + 24, rect.bottom - 3),
        rgb(255, 255, 255),
        rgb(255, 255, 255),
    );
}

unsafe fn draw_radio(hdc: HDC, rect: UiRect, selected: bool) {
    draw_ellipse(
        hdc,
        rect,
        if selected {
            rgb(250, 204, 21)
        } else {
            rgb(255, 255, 255)
        },
        if selected {
            rgb(250, 204, 21)
        } else {
            rgb(190, 190, 186)
        },
    );

    if selected {
        draw_ellipse(
            hdc,
            UiRect::new(rect.left + 6, rect.top + 6, rect.right - 6, rect.bottom - 6),
            rgb(82, 63, 0),
            rgb(82, 63, 0),
        );
    }
}

unsafe fn draw_button(hdc: HDC, rect: UiRect, text: &str, primary: bool) {
    let fill = if primary {
        rgb(250, 204, 21)
    } else {
        rgb(242, 242, 238)
    };
    let border = if primary {
        rgb(250, 204, 21)
    } else {
        rgb(222, 222, 217)
    };
    draw_rounded_rect(hdc, rect, 12, fill, border);
    draw_text(
        hdc,
        text,
        rect,
        12,
        700,
        if primary {
            rgb(70, 53, 0)
        } else {
            rgb(48, 48, 46)
        },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
}

unsafe fn draw_pill(hdc: HDC, rect: UiRect, text: &str) {
    draw_rounded_rect(
        hdc,
        rect,
        16,
        rgb(255, 249, 214),
        rgb(250, 221, 91),
    );
    draw_text(
        hdc,
        text,
        rect,
        11,
        700,
        rgb(91, 69, 0),
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
}

unsafe fn draw_rounded_rect(
    hdc: HDC,
    rect: UiRect,
    radius: i32,
    fill: COLORREF,
    border: COLORREF,
) {
    let brush = CreateSolidBrush(fill);
    let pen = CreatePen(PS_SOLID, 1, border);
    let old_brush = SelectObject(hdc, brush as HGDIOBJ);
    let old_pen = SelectObject(hdc, pen as HGDIOBJ);

    RoundRect(
        hdc,
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        radius,
        radius,
    );

    SelectObject(hdc, old_pen);
    SelectObject(hdc, old_brush);
    DeleteObject(pen as HGDIOBJ);
    DeleteObject(brush as HGDIOBJ);
}

unsafe fn draw_ellipse(hdc: HDC, rect: UiRect, fill: COLORREF, border: COLORREF) {
    let brush = CreateSolidBrush(fill);
    let pen = CreatePen(PS_SOLID, 1, border);
    let old_brush = SelectObject(hdc, brush as HGDIOBJ);
    let old_pen = SelectObject(hdc, pen as HGDIOBJ);

    Ellipse(hdc, rect.left, rect.top, rect.right, rect.bottom);

    SelectObject(hdc, old_pen);
    SelectObject(hdc, old_brush);
    DeleteObject(pen as HGDIOBJ);
    DeleteObject(brush as HGDIOBJ);
}

unsafe fn fill_rect(hdc: HDC, rect: RECT, color: COLORREF) {
    let brush: HBRUSH = CreateSolidBrush(color);
    FillRect(hdc, &rect, brush);
    DeleteObject(brush as HGDIOBJ);
}

unsafe fn draw_text(
    hdc: HDC,
    text: &str,
    rect: UiRect,
    size: i32,
    weight: i32,
    color: COLORREF,
    format: u32,
) {
    let face = str_wide_null("Segoe UI Variable Text");
    let font = CreateFontW(
        -size,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        1,
        0,
        0,
        5,
        0,
        face.as_ptr(),
    );

    let old_font = if font.is_null() {
        ptr::null_mut()
    } else {
        SelectObject(hdc, font as HGDIOBJ)
    };

    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, color);

    let mut text = str_wide_null(text);
    let mut rect = rect.to_rect();
    DrawTextW(hdc, text.as_mut_ptr(), -1, &mut rect, format);

    if !font.is_null() {
        SelectObject(hdc, old_font);
        DeleteObject(font as HGDIOBJ);
    }
}

unsafe fn load_app_icon() -> *mut std::ffi::c_void {
    load_icon_resource(APP_ICON_RESOURCE_ID)
}

unsafe fn load_tray_icon(enabled: bool) -> *mut std::ffi::c_void {
    load_icon_resource(if enabled {
        APP_ICON_RESOURCE_ID
    } else {
        PAUSED_ICON_RESOURCE_ID
    })
}

unsafe fn load_icon_resource(resource_id: u16) -> *mut std::ffi::c_void {
    let instance = GetModuleHandleW(ptr::null());
    let icon = LoadIconW(instance, make_int_resource(resource_id));
    if icon.is_null() {
        LoadIconW(ptr::null_mut(), IDI_APPLICATION)
    } else {
        icon
    }
}

fn make_int_resource(id: u16) -> *const u16 {
    id as usize as *const u16
}

unsafe fn app_from_hwnd(hwnd: HWND) -> Option<&'static mut App> {
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App;
    pointer.as_mut()
}

unsafe fn message_box(hwnd: HWND, text: &str, title: &str, flags: u32) -> i32 {
    let text = str_wide_null(text);
    let title = str_wide_null(title);
    MessageBoxW(hwnd, text.as_ptr(), title.as_ptr(), flags)
}

unsafe fn append_menu(menu: HMENU, id: usize, text: &str, flags: u32) {
    let text = str_wide_null(text);
    AppendMenuW(menu, flags, id, text.as_ptr());
}

unsafe fn append_submenu(menu: HMENU, submenu: HMENU, text: &str) {
    let text = str_wide_null(text);
    AppendMenuW(menu, MF_POPUP | MF_STRING, submenu as usize, text.as_ptr());
}

unsafe fn append_separator(menu: HMENU) {
    AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
}

fn point_from_lparam(lparam: LPARAM) -> (i32, i32) {
    let x = lparam as i16 as i32;
    let y = ((lparam >> 16) as i16) as i32;
    (x, y)
}

const fn rgb(red: u8, green: u8, blue: u8) -> COLORREF {
    red as u32 | ((green as u32) << 8) | ((blue as u32) << 16)
}

#[derive(Clone, Copy)]
struct UiRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl UiRect {
    const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    const fn contains(self, x: i32, y: i32) -> bool {
        x >= self.left && x <= self.right && y >= self.top && y <= self.bottom
    }

    const fn to_rect(self) -> RECT {
        RECT {
            left: self.left,
            top: self.top,
            right: self.right,
            bottom: self.bottom,
        }
    }
}
