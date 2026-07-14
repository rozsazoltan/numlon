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
            Ellipse, EndPaint, FillRect, InvalidateRect, RestoreDC, RoundRect, SaveDC,
            SelectObject, SetBkMode, SetTextColor, SetViewportOrgEx, HBRUSH, HDC, HGDIOBJ,
            PAINTSTRUCT, PS_SOLID, TRANSPARENT, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT,
            DT_SINGLELINE, DT_VCENTER,
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
            GetMessageW, GetScrollInfo, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect,
            IsIconic, LoadCursorW, LoadIconW, LoadImageW, MessageBoxW, PostMessageW,
            PostQuitMessage, RegisterClassW, SendMessageW, SetForegroundWindow, SetMenuDefaultItem,
            SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow, TrackPopupMenu,
            TranslateMessage, CW_USEDEFAULT, DI_NORMAL, GWLP_USERDATA, HMENU, ICON_BIG,
            ICON_SMALL, IDC_ARROW, IDI_APPLICATION, IMAGE_ICON, LR_SHARED, MF_CHECKED,
            MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, MINMAXINFO, MSG,
            SB_BOTTOM, SB_LINEDOWN, SB_LINEUP, SB_PAGEDOWN, SB_PAGEUP, SB_THUMBPOSITION,
            SB_THUMBTRACK, SB_TOP, SB_VERT, SCROLLINFO, SIF_PAGE, SIF_POS, SIF_RANGE,
            SIF_TRACKPOS, SM_CXICON, SM_CXSMICON, SM_CYICON, SM_CYSMICON, SW_HIDE, SW_RESTORE,
            SW_SHOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, TPM_RIGHTBUTTON, WM_APP, WM_CLOSE,
            WM_COMMAND, WM_DESTROY, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_HOTKEY, WM_KEYDOWN,
            WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_MOUSEWHEEL, WM_NCDESTROY, WM_PAINT,
            WM_RBUTTONUP, WM_SETICON, WM_SIZE, WM_SYSKEYDOWN, WM_TIMER, WM_VSCROLL, WNDCLASSW,
            WS_CAPTION, WS_EX_APPWINDOW, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_OVERLAPPED,
            WS_SYSMENU, WS_THICKFRAME, WS_VSCROLL,
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

#[link(name = "user32")]
extern "system" {
    fn SetScrollInfo(
        hwnd: HWND,
        bar: i32,
        info: *const SCROLLINFO,
        redraw: i32,
    ) -> i32;
}

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

const CONTENT_WIDTH: i32 = 600;
const CONTENT_HEIGHT: i32 = 500;
const MIN_WINDOW_WIDTH: i32 = 620;
const MIN_WINDOW_HEIGHT: i32 = 400;
const SCROLL_STEP: i32 = 48;

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

const ENABLED_SWITCH: UiRect = UiRect::new(510, 92, 566, 122);
const MODE_FORCE_ROW: UiRect = UiRect::new(28, 160, 286, 204);
const MODE_LED_ROW: UiRect = UiRect::new(294, 160, 572, 204);
const HOTKEY_BUTTON: UiRect = UiRect::new(452, 226, 572, 262);
const STARTUP_SWITCH: UiRect = UiRect::new(510, 286, 566, 316);
const UPDATE_CHANNEL_SWITCH: UiRect = UiRect::new(510, 346, 566, 376);
const UPDATE_ACTION_BUTTON: UiRect = UiRect::new(410, 342, 496, 380);
const HIDE_BUTTON: UiRect = UiRect::new(476, 444, 584, 480);

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
    let _gdi_plus = crate::gdi_plus::Session::start();

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
            WS_OVERLAPPED
                | WS_CAPTION
                | WS_SYSMENU
                | WS_THICKFRAME
                | WS_MINIMIZEBOX
                | WS_MAXIMIZEBOX
                | WS_VSCROLL,
            x,
            y,
            CONTENT_WIDTH,
            CONTENT_HEIGHT,
            ptr::null_mut(),
            ptr::null_mut(),
            instance,
            ptr::null_mut(),
        );

        if hwnd.is_null() {
            anyhow::bail!("failed to create Numlon window: {}", GetLastError());
        }

        resize_to_client(hwnd, CONTENT_WIDTH, CONTENT_HEIGHT);
        style_window(hwnd);
        set_window_icons(hwnd);

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
            app.update_scrollbar();
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
    scroll_offset: i32,
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
            scroll_offset: 0,
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

    unsafe fn update_scrollbar(&mut self) {
        let mut client = RECT::default();
        GetClientRect(self.hwnd, &mut client);
        let viewport_height = (client.bottom - client.top).max(1);
        let max_offset = (CONTENT_HEIGHT - viewport_height).max(0);
        self.scroll_offset = self.scroll_offset.clamp(0, max_offset);

        let mut info: SCROLLINFO = mem::zeroed();
        info.cbSize = mem::size_of::<SCROLLINFO>() as u32;
        info.fMask = SIF_RANGE | SIF_PAGE | SIF_POS;
        info.nMin = 0;
        info.nMax = CONTENT_HEIGHT.saturating_sub(1);
        info.nPage = viewport_height as u32;
        info.nPos = self.scroll_offset;
        SetScrollInfo(self.hwnd, SB_VERT, &info, 1);
    }

    unsafe fn scroll_to(&mut self, position: i32) {
        let mut client = RECT::default();
        GetClientRect(self.hwnd, &mut client);
        let viewport_height = (client.bottom - client.top).max(1);
        let max_offset = (CONTENT_HEIGHT - viewport_height).max(0);
        let position = position.clamp(0, max_offset);

        if position == self.scroll_offset {
            return;
        }

        self.scroll_offset = position;
        self.update_scrollbar();
        self.repaint();
    }

    unsafe fn scroll_by(&mut self, delta: i32) {
        self.scroll_to(self.scroll_offset.saturating_add(delta));
    }

    unsafe fn handle_vscroll(&mut self, request: i32) {
        match request {
            SB_LINEUP => self.scroll_by(-SCROLL_STEP),
            SB_LINEDOWN => self.scroll_by(SCROLL_STEP),
            SB_PAGEUP => self.scroll_by(-CONTENT_HEIGHT / 2),
            SB_PAGEDOWN => self.scroll_by(CONTENT_HEIGHT / 2),
            SB_TOP => self.scroll_to(0),
            SB_BOTTOM => self.scroll_to(CONTENT_HEIGHT),
            SB_THUMBPOSITION | SB_THUMBTRACK => {
                let mut info: SCROLLINFO = mem::zeroed();
                info.cbSize = mem::size_of::<SCROLLINFO>() as u32;
                info.fMask = SIF_TRACKPOS;
                if GetScrollInfo(self.hwnd, SB_VERT, &mut info) != 0 {
                    self.scroll_to(info.nTrackPos);
                }
            }
            _ => {}
        }
    }

    unsafe fn content_origin_x(&self) -> i32 {
        let mut client = RECT::default();
        GetClientRect(self.hwnd, &mut client);
        ((client.right - client.left - CONTENT_WIDTH) / 2).max(0)
    }

    unsafe fn handle_click(&mut self, x: i32, y: i32) {
        let x = x - self.content_origin_x();
        let y = y + self.scroll_offset;

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
        fill_rect(hdc, client, rgb(245, 245, 243));

        let saved_dc = SaveDC(hdc);
        SetViewportOrgEx(
            hdc,
            self.content_origin_x(),
            -self.scroll_offset,
            ptr::null_mut(),
        );

        draw_header(hdc);
        draw_surface(hdc, UiRect::new(16, 74, 584, 410));
        self.draw_enabled_row(hdc);
        self.draw_behavior_row(hdc);
        self.draw_hotkey_row(hdc);
        self.draw_startup_row(hdc);
        self.draw_updates_row(hdc);
        self.draw_footer(hdc);

        if saved_dc != 0 {
            RestoreDC(hdc, saved_dc);
        }
        EndPaint(self.hwnd, &paint);
    }

    unsafe fn draw_enabled_row(&self, hdc: HDC) {
        draw_text(
            hdc,
            if self.state.always_enabled {
                "Numlon active"
            } else {
                "Numlon paused"
            },
            UiRect::new(30, 88, 420, 110),
            15,
            700,
            rgb(32, 33, 36),
            DT_LEFT | DT_SINGLELINE,
        );
        draw_text(
            hdc,
            if self.state.always_enabled {
                self.state.numlock_mode.label()
            } else {
                "Keyboard state remains unchanged"
            },
            UiRect::new(30, 112, 430, 128),
            11,
            400,
            rgb(96, 96, 100),
            DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        draw_switch(hdc, ENABLED_SWITCH, self.state.always_enabled);
        draw_divider(hdc, 30, 136, 570);
    }

    unsafe fn draw_behavior_row(&self, hdc: HDC) {
        draw_text(
            hdc,
            "Behavior",
            UiRect::new(30, 144, 250, 160),
            11,
            600,
            rgb(84, 84, 88),
            DT_LEFT | DT_SINGLELINE,
        );

        draw_compact_choice(
            hdc,
            MODE_FORCE_ROW,
            self.state.numlock_mode == NumlockMode::ForceOn,
            "NumLock on",
            "Keeps keypad numeric",
            true,
        );
        draw_compact_choice(
            hdc,
            MODE_LED_ROW,
            self.state.numlock_mode == NumlockMode::LedOffDigits,
            "LED off",
            "Maps keypad to digits",
            self.keyboard_hook.is_some(),
        );
        draw_divider(hdc, 30, 216, 570);
    }

    unsafe fn draw_hotkey_row(&self, hdc: HDC) {
        draw_text(
            hdc,
            "Toggle shortcut",
            UiRect::new(30, 228, 300, 248),
            14,
            600,
            rgb(32, 33, 36),
            DT_LEFT | DT_SINGLELINE,
        );

        let hotkey_display = self.state.hotkey.display();
        let hotkey_text = if self.capturing_hotkey {
            "Press shortcut now. Esc cancels."
        } else {
            hotkey_display.as_str()
        };

        draw_text(
            hdc,
            hotkey_text,
            UiRect::new(30, 250, 420, 268),
            11,
            if self.capturing_hotkey { 600 } else { 400 },
            if self.capturing_hotkey {
                rgb(125, 88, 0)
            } else {
                rgb(96, 96, 100)
            },
            DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        draw_button(
            hdc,
            HOTKEY_BUTTON,
            if self.capturing_hotkey {
                "Listening"
            } else {
                "Change"
            },
            true,
        );
        draw_divider(hdc, 30, 276, 570);
    }

    unsafe fn draw_startup_row(&self, hdc: HDC) {
        draw_text(
            hdc,
            "Start with Windows",
            UiRect::new(30, 288, 380, 308),
            14,
            600,
            rgb(32, 33, 36),
            DT_LEFT | DT_SINGLELINE,
        );
        draw_text(
            hdc,
            if config::is_dev_build() {
                "Unavailable in development builds"
            } else if self.state.startup_enabled {
                "Starts from current executable path"
            } else {
                "Move executable to final folder before enabling"
            },
            UiRect::new(30, 310, 460, 328),
            11,
            400,
            rgb(96, 96, 100),
            DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        draw_switch(
            hdc,
            STARTUP_SWITCH,
            self.state.startup_enabled && !config::is_dev_build(),
        );
        draw_divider(hdc, 30, 336, 570);
    }

    unsafe fn draw_updates_row(&self, hdc: HDC) {
        draw_text(
            hdc,
            "Updates",
            UiRect::new(30, 348, 250, 368),
            14,
            600,
            rgb(32, 33, 36),
            DT_LEFT | DT_SINGLELINE,
        );

        if config::is_dev_build() {
            draw_text(
                hdc,
                "Disabled in dev — no GitHub API requests",
                UiRect::new(30, 370, 500, 388),
                11,
                400,
                rgb(96, 96, 100),
                DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS,
            );
            return;
        }

        draw_text(
            hdc,
            if self.state.include_prereleases {
                "Prerelease channel included"
            } else {
                "Stable releases only"
            },
            UiRect::new(30, 370, 380, 388),
            11,
            400,
            rgb(96, 96, 100),
            DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS,
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
            UiRect::new(20, 438, 452, 478),
            11,
            400,
            rgb(88, 88, 92),
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
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
        WM_SIZE => {
            if let Some(app) = app_from_hwnd(hwnd) {
                app.update_scrollbar();
                app.repaint();
            }
            0
        }
        WM_VSCROLL => {
            if let Some(app) = app_from_hwnd(hwnd) {
                app.handle_vscroll((wparam & 0xffff) as i32);
            }
            0
        }
        WM_MOUSEWHEEL => {
            if let Some(app) = app_from_hwnd(hwnd) {
                let delta = ((wparam >> 16) & 0xffff) as u16 as i16 as i32;
                if delta != 0 {
                    app.scroll_by(-delta.signum() * SCROLL_STEP);
                }
            }
            0
        }
        WM_GETMINMAXINFO => {
            let info = lparam as *mut MINMAXINFO;
            if !info.is_null() {
                (*info).ptMinTrackSize.x = MIN_WINDOW_WIDTH;
                (*info).ptMinTrackSize.y = MIN_WINDOW_HEIGHT;
            }
            0
        }
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


unsafe fn resize_to_client(hwnd: HWND, client_width: i32, client_height: i32) {
    let mut window = RECT::default();
    let mut client = RECT::default();
    if GetWindowRect(hwnd, &mut window) == 0 || GetClientRect(hwnd, &mut client) == 0 {
        return;
    }

    let frame_width = (window.right - window.left) - (client.right - client.left);
    let frame_height = (window.bottom - window.top) - (client.bottom - client.top);
    SetWindowPos(
        hwnd,
        ptr::null_mut(),
        0,
        0,
        client_width + frame_width,
        client_height + frame_height,
        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
    );
}

unsafe fn set_window_icons(hwnd: HWND) {
    let large_icon = load_icon_resource_sized(
        APP_ICON_RESOURCE_ID,
        GetSystemMetrics(SM_CXICON),
        GetSystemMetrics(SM_CYICON),
    );
    let small_icon = load_icon_resource_sized(
        APP_ICON_RESOURCE_ID,
        GetSystemMetrics(SM_CXSMICON),
        GetSystemMetrics(SM_CYSMICON),
    );
    SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, large_icon as isize);
    SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, small_icon as isize);
}

unsafe fn style_window(hwnd: HWND) {
    let corner = DWMWCP_ROUND;
    let caption = rgb(243, 243, 240);
    let border = rgb(226, 226, 220);
    let text = rgb(28, 28, 30);

    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_WINDOW_CORNER_PREFERENCE as u32,
        &corner as *const _ as *const _,
        mem::size_of_val(&corner) as u32,
    );
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_CAPTION_COLOR as u32,
        &caption as *const _ as *const _,
        mem::size_of_val(&caption) as u32,
    );
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_BORDER_COLOR as u32,
        &border as *const _ as *const _,
        mem::size_of_val(&border) as u32,
    );
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_TEXT_COLOR as u32,
        &text as *const _ as *const _,
        mem::size_of_val(&text) as u32,
    );
}

unsafe fn draw_header(hdc: HDC) {
    DrawIconEx(
        hdc,
        20,
        16,
        load_icon_resource_sized(APP_ICON_RESOURCE_ID, 40, 40),
        40,
        40,
        0,
        ptr::null_mut(),
        DI_NORMAL,
    );
    draw_text(
        hdc,
        "Numlon",
        UiRect::new(72, 14, 360, 38),
        20,
        700,
        rgb(32, 33, 36),
        DT_LEFT | DT_SINGLELINE,
    );
    draw_text(
        hdc,
        "NumLock control for Windows",
        UiRect::new(72, 40, 420, 58),
        11,
        400,
        rgb(96, 96, 100),
        DT_LEFT | DT_SINGLELINE,
    );
    draw_pill(
        hdc,
        UiRect::new(474, 18, 584, 48),
        &config::app_version_label(),
    );
}

unsafe fn draw_surface(hdc: HDC, rect: UiRect) {
    draw_rounded_rect(
        hdc,
        rect,
        14,
        rgb(255, 255, 255),
        rgb(224, 224, 226),
    );
}

unsafe fn draw_divider(hdc: HDC, left: i32, top: i32, right: i32) {
    fill_rect(
        hdc,
        RECT {
            left,
            top,
            right,
            bottom: top + 1,
        },
        rgb(232, 232, 234),
    );
}

unsafe fn draw_compact_choice(
    hdc: HDC,
    rect: UiRect,
    selected: bool,
    title: &str,
    subtitle: &str,
    enabled: bool,
) {
    let fill = if selected {
        rgb(255, 247, 214)
    } else {
        rgb(248, 248, 249)
    };
    let border = if selected {
        rgb(255, 185, 0)
    } else {
        rgb(224, 224, 226)
    };
    draw_rounded_rect(hdc, rect, 10, fill, border);
    draw_radio(
        hdc,
        UiRect::new(rect.left + 12, rect.top + 13, rect.left + 30, rect.top + 31),
        selected,
    );

    let title_color = if enabled {
        rgb(32, 33, 36)
    } else {
        rgb(148, 148, 152)
    };
    let subtitle_color = if enabled {
        rgb(96, 96, 100)
    } else {
        rgb(164, 164, 168)
    };

    draw_text(
        hdc,
        title,
        UiRect::new(rect.left + 40, rect.top + 7, rect.right - 10, rect.top + 23),
        12,
        600,
        title_color,
        DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
    draw_text(
        hdc,
        subtitle,
        UiRect::new(rect.left + 40, rect.top + 24, rect.right - 10, rect.bottom - 5),
        10,
        400,
        subtitle_color,
        DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
}

unsafe fn draw_switch(hdc: HDC, rect: UiRect, enabled: bool) {
    let track = if enabled {
        rgb(255, 200, 32)
    } else {
        rgb(214, 214, 210)
    };
    draw_rounded_rect(hdc, rect, 16, track, track);

    let knob_left = if enabled {
        rect.right - 28
     } else {
        rect.left + 4
    };
    draw_ellipse(
        hdc,
        UiRect::new(knob_left, rect.top + 4, knob_left + 22, rect.bottom - 4),
        rgb(255, 255, 255),
        rgb(255, 255, 255),
    );
}

unsafe fn draw_radio(hdc: HDC, rect: UiRect, selected: bool) {
    draw_ellipse(
        hdc,
        rect,
        if selected {
            rgb(255, 200, 32)
        } else {
            rgb(255, 255, 255)
        },
        if selected {
            rgb(255, 200, 32)
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
        rgb(255, 200, 32)
    } else {
        rgb(246, 246, 243)
    };
    let border = if primary {
        rgb(255, 200, 32)
    } else {
        rgb(230, 230, 226)
    };
    draw_rounded_rect(hdc, rect, 14, fill, border);
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
        18,
        rgb(255, 250, 225),
        rgb(255, 215, 84),
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
    if crate::gdi_plus::draw_rounded_rect(
        hdc,
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        radius,
        fill,
        border,
    ) {
        return;
    }

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
        radius * 2,
        radius * 2,
    );

    SelectObject(hdc, old_pen);
    SelectObject(hdc, old_brush);
    DeleteObject(pen as HGDIOBJ);
    DeleteObject(brush as HGDIOBJ);
}

unsafe fn draw_ellipse(hdc: HDC, rect: UiRect, fill: COLORREF, border: COLORREF) {
    if crate::gdi_plus::draw_ellipse(
        hdc,
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        fill,
        border,
    ) {
        return;
    }

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

    SetBkMode(hdc, TRANSPARENT as i32);
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
    load_icon_resource_sized(
        APP_ICON_RESOURCE_ID,
        GetSystemMetrics(SM_CXICON),
        GetSystemMetrics(SM_CYICON),
    )
}

unsafe fn load_tray_icon(enabled: bool) -> *mut std::ffi::c_void {
    load_icon_resource_sized(
        if enabled {
            APP_ICON_RESOURCE_ID
        } else {
            PAUSED_ICON_RESOURCE_ID
        },
        GetSystemMetrics(SM_CXSMICON),
        GetSystemMetrics(SM_CYSMICON),
    )
}

unsafe fn load_icon_resource_sized(
    resource_id: u16,
    width: i32,
    height: i32,
) -> *mut std::ffi::c_void {
    let instance = GetModuleHandleW(ptr::null());
    let icon = LoadImageW(
        instance,
        make_int_resource(resource_id),
        IMAGE_ICON,
        width.max(1),
        height.max(1),
        LR_SHARED,
    );
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
