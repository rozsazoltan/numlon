use anyhow::Result;
use std::{
    ptr,
    sync::mpsc::{self, Receiver},
    thread,
};
use windows_sys::Win32::{
    Foundation::{GetLastError, HWND, LPARAM, LRESULT, POINT, WPARAM},
    Graphics::Gdi::HBRUSH,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Input::KeyboardAndMouse::{
            RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_NOREPEAT, MOD_WIN, VK_HOME,
        },
        Shell::{
            Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
            NOTIFYICONDATAW,
        },
        WindowsAndMessaging::{
            AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
            DestroyWindow, DispatchMessageW, GetCursorPos, GetDlgItem, GetMessageW,
            GetWindowLongPtrW, LoadCursorW, LoadIconW, MessageBoxW, PostQuitMessage,
            RegisterClassW, SetForegroundWindow, SetTimer, SetWindowLongPtrW, SetWindowTextW,
            ShowWindow, TrackPopupMenu, TranslateMessage, GWLP_USERDATA, HMENU, IDC_ARROW,
            IDI_APPLICATION, MF_SEPARATOR, MF_STRING, MSG, SW_HIDE, SW_SHOW, TPM_RIGHTBUTTON,
            WM_APP, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_HOTKEY, WM_LBUTTONDBLCLK,
            WM_RBUTTONUP, WM_TIMER, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_EX_APPWINDOW,
            WS_OVERLAPPED, WS_SYSMENU, WS_VISIBLE,
        },
    },
};

use crate::{
    config::{self, SavedState},
    numlock, startup, updater,
    wide::{copy_wide_truncated, str_wide_null},
};

const CLASS_NAME: &str = "NumlonWindowClass";
const WINDOW_TITLE: &str = "Numlon";
const WM_TRAY_ICON: u32 = WM_APP + 1;
const TRAY_ID: u32 = 1;
const HOTKEY_TOGGLE_ALWAYS: i32 = 1;
const APP_ICON_RESOURCE_ID: u16 = 1;
const COLOR_WINDOW_INDEX: usize = 5;
const TIMER_ENFORCE: usize = 1;
const TIMER_POLL_UPDATES: usize = 2;
const ENFORCE_INTERVAL_MS: u32 = 350;
const UPDATE_POLL_INTERVAL_MS: u32 = 250;
const AUTO_UPDATE_INTERVAL_SECONDS: u64 = 60 * 60;

const ID_STATUS: usize = 1000;
const ID_ALWAYS: usize = 1001;
const ID_STARTUP: usize = 1002;
const ID_PRERELEASES: usize = 1003;
const ID_CHECK_UPDATES: usize = 1004;
const ID_INSTALL_UPDATE: usize = 1005;
const ID_OPEN_RELEASES: usize = 1006;
const ID_HIDE: usize = 1007;

const MENU_OPEN: usize = 2001;
const MENU_TOGGLE_ALWAYS: usize = 2002;
const MENU_TOGGLE_STARTUP: usize = 2003;
const MENU_CHECK_UPDATES: usize = 2004;
const MENU_INSTALL_UPDATE: usize = 2005;
const MENU_OPEN_RELEASES: usize = 2006;
const MENU_EXIT: usize = 2007;

pub fn run() -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(ptr::null());
        let class_name = str_wide_null(CLASS_NAME);
        let window_title = str_wide_null(WINDOW_TITLE);

        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: load_app_icon(),
            hCursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_WINDOW_INDEX + 1) as isize as HBRUSH,
            lpszMenuName: ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };

        RegisterClassW(&class);

        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            class_name.as_ptr(),
            window_title.as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
            240,
            240,
            430,
            270,
            ptr::null_mut(),
            ptr::null_mut(),
            instance,
            ptr::null_mut(),
        );

        if hwnd.is_null() {
            anyhow::bail!("failed to create Numlon window: {}", GetLastError());
        }

        let mut app = Box::new(App::new(hwnd));
        app.sync_startup_state();
        let app_ptr = Box::into_raw(app);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, app_ptr as isize);

        if let Some(app) = app_from_hwnd(hwnd) {
            app.create_controls();
            app.add_tray_icon();
            app.register_hotkey();
            app.refresh_controls();
            app.prompt_startup_on_first_run();
            app.maybe_start_auto_update_check();
        }

        SetTimer(hwnd, TIMER_ENFORCE, ENFORCE_INTERVAL_MS, None);
        SetTimer(hwnd, TIMER_POLL_UPDATES, UPDATE_POLL_INTERVAL_MS, None);

        let mut message = MSG::default();
        while GetMessageW(&mut message, ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }

        let app_ptr = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        if app_ptr != 0 {
            drop(Box::from_raw(app_ptr as *mut App));
        }
    }

    Ok(())
}

struct App {
    hwnd: HWND,
    state: SavedState,
    last_update_check: Option<updater::UpdateCheck>,
    update_rx: Option<Receiver<anyhow::Result<updater::UpdateCheck>>>,
    status: String,
}

impl App {
    fn new(hwnd: HWND) -> Self {
        let state = config::load_state();
        let status = state.last_status.clone();
        Self {
            hwnd,
            state,
            last_update_check: None,
            update_rx: None,
            status,
        }
    }

    unsafe fn create_controls(&self) {
        create_static(self.hwnd, ID_STATUS, 18, 18, 380, 48, "Numlon active.");
        create_button(self.hwnd, ID_ALWAYS, 18, 78, 185, 32, "Disable always-on");
        create_button(self.hwnd, ID_STARTUP, 218, 78, 185, 32, "Enable startup");
        create_button(self.hwnd, ID_PRERELEASES, 18, 120, 185, 32, "Watch prereleases");
        create_button(self.hwnd, ID_CHECK_UPDATES, 218, 120, 185, 32, "Check updates");
        create_button(self.hwnd, ID_INSTALL_UPDATE, 18, 162, 185, 32, "Install update");
        create_button(self.hwnd, ID_OPEN_RELEASES, 218, 162, 185, 32, "Open releases");
        create_button(self.hwnd, ID_HIDE, 18, 204, 385, 32, "Hide to tray");
    }

    fn sync_startup_state(&mut self) {
        match startup::is_enabled() {
            Ok(enabled) => self.state.startup_enabled = enabled,
            Err(error) => self.status = format!("Startup check failed: {error}"),
        }
    }

    fn save(&mut self) {
        self.state.last_status = self.status.clone();
        if let Err(error) = config::save_state(&self.state) {
            self.status = format!("State save failed: {error}");
        }
    }

    unsafe fn refresh_controls(&self) {
        let status = format!(
            "{} {}\r\nAlways-on: {} | Startup: {} | Updates: {}",
            config::app_name(),
            config::app_version_label(),
            if self.state.always_enabled { "on" } else { "off" },
            if self.state.startup_enabled { "on" } else { "off" },
            if self.state.include_prereleases { "prerelease" } else { "stable" },
        );
        set_control_text(self.hwnd, ID_STATUS, &status);
        set_control_text(
            self.hwnd,
            ID_ALWAYS,
            if self.state.always_enabled { "Disable always-on" } else { "Enable always-on" },
        );
        set_control_text(
            self.hwnd,
            ID_STARTUP,
            if self.state.startup_enabled { "Disable startup" } else { "Enable startup" },
        );
        set_control_text(
            self.hwnd,
            ID_PRERELEASES,
            if self.state.include_prereleases { "Use stable updates" } else { "Watch prereleases" },
        );
        set_control_text(
            self.hwnd,
            ID_INSTALL_UPDATE,
            if self.update_is_installable() { "Install update" } else { "No installable update" },
        );
    }

    fn update_is_installable(&self) -> bool {
        self.last_update_check
            .as_ref()
            .map(|check| check.is_update_available && check.asset_download_url.is_some())
            .unwrap_or(false)
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
        let mut data: NOTIFYICONDATAW = std::mem::zeroed();
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = self.hwnd;
        data.uID = TRAY_ID;
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.uCallbackMessage = WM_TRAY_ICON;
        data.hIcon = load_app_icon();
        copy_wide_truncated(
            &mut data.szTip,
            if self.state.always_enabled { "Numlon: always-on" } else { "Numlon: paused" },
        );
        data
    }

    unsafe fn register_hotkey(&mut self) {
        let ok = RegisterHotKey(
            self.hwnd,
            HOTKEY_TOGGLE_ALWAYS,
            MOD_WIN | MOD_ALT | MOD_NOREPEAT,
            VK_HOME as u32,
        );
        if ok == 0 {
            self.status = format!("Hotkey registration failed: {}", GetLastError());
            self.save();
        }
    }

    unsafe fn unregister_hotkey(&self) {
        UnregisterHotKey(self.hwnd, HOTKEY_TOGGLE_ALWAYS);
    }

    unsafe fn prompt_startup_on_first_run(&mut self) {
        if self.state.startup_prompted {
            return;
        }

        let result = message_box(
            self.hwnd,
            "Enable Numlon at Windows startup?\n\nBefore enabling startup, move numlon.exe to its final folder. Do not move it afterward, because Windows stores the exact executable path.",
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

    unsafe fn toggle_always_enabled(&mut self) {
        self.state.always_enabled = !self.state.always_enabled;
        self.status = if self.state.always_enabled {
            let _ = numlock::ensure_numlock_on();
            "Always-on NumLock enabled.".to_owned()
        } else {
            "Always-on NumLock disabled.".to_owned()
        };
        self.save();
        self.refresh_controls();
        self.update_tray_icon();
    }

    unsafe fn toggle_startup(&mut self) {
        let target = !self.state.startup_enabled;
        if target {
            let result = message_box(
                self.hwnd,
                "Before enabling startup, move numlon.exe to its final folder. Do not move it afterward, because Windows stores the exact executable path.\n\nEnable startup now?",
                "Numlon startup",
                windows_sys::Win32::UI::WindowsAndMessaging::MB_YESNO
                    | windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONWARNING,
            );
            if result != windows_sys::Win32::UI::WindowsAndMessaging::IDYES {
                return;
            }
        }
        self.set_startup_enabled(target);
        self.refresh_controls();
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
        self.state.include_prereleases = !self.state.include_prereleases;
        self.last_update_check = None;
        self.status = if self.state.include_prereleases {
            "Prerelease update checks enabled.".to_owned()
        } else {
            "Stable update checks enabled.".to_owned()
        };
        self.save();
        self.refresh_controls();
        self.start_update_check();
    }

    unsafe fn enforce_numlock(&mut self) {
        if !self.state.always_enabled {
            return;
        }
        match numlock::ensure_numlock_on() {
            Ok(true) => self.status = "NumLock restored.".to_owned(),
            Ok(false) => {}
            Err(error) => self.status = format!("NumLock restore failed: {error}"),
        }
    }

    unsafe fn maybe_start_auto_update_check(&mut self) {
        if cfg!(debug_assertions) || self.update_rx.is_some() {
            return;
        }
        let now = config::seconds_since_unix_epoch();
        if now.saturating_sub(self.state.last_auto_update_check_unix_seconds) < AUTO_UPDATE_INTERVAL_SECONDS {
            return;
        }
        self.state.last_auto_update_check_unix_seconds = now;
        self.save();
        self.start_update_check();
    }

    unsafe fn start_update_check(&mut self) {
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
        self.refresh_controls();
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
                self.refresh_controls();
            }
            Ok(Err(error)) => {
                self.status = format!("Update check failed: {error}");
                self.update_rx = None;
                self.save();
                self.refresh_controls();
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.status = "Update check failed: worker disconnected.".to_owned();
                self.update_rx = None;
                self.save();
                self.refresh_controls();
            }
        }
    }

    unsafe fn install_update(&mut self) {
        let Some(check) = self.last_update_check.clone() else {
            self.status = "No update check result yet.".to_owned();
            self.refresh_controls();
            return;
        };
        if !check.is_update_available || check.asset_download_url.is_none() {
            self.status = "No installable update available.".to_owned();
            self.refresh_controls();
            return;
        }
        self.status = "Installing update...".to_owned();
        self.save();
        self.refresh_controls();
        if let Err(error) = updater::install_update(&check) {
            self.status = format!("Update install failed: {error}");
            self.save();
            self.refresh_controls();
        }
    }

    unsafe fn show_window(&self) {
        ShowWindow(self.hwnd, SW_SHOW);
        SetForegroundWindow(self.hwnd);
    }

    unsafe fn hide_window(&self) {
        ShowWindow(self.hwnd, SW_HIDE);
    }

    unsafe fn show_tray_menu(&mut self) {
        let menu = CreatePopupMenu();
        append_menu(menu, MENU_OPEN, "Open Numlon");
        append_menu(
            menu,
            MENU_TOGGLE_ALWAYS,
            if self.state.always_enabled { "Disable always-on" } else { "Enable always-on" },
        );
        append_menu(
            menu,
            MENU_TOGGLE_STARTUP,
            if self.state.startup_enabled { "Disable startup" } else { "Enable startup" },
        );
        append_separator(menu);
        append_menu(menu, MENU_CHECK_UPDATES, "Check updates");
        append_menu(menu, MENU_INSTALL_UPDATE, "Install update");
        append_menu(menu, MENU_OPEN_RELEASES, "Open releases");
        append_separator(menu);
        append_menu(menu, MENU_EXIT, "Exit");

        let mut point = POINT::default();
        GetCursorPos(&mut point);
        SetForegroundWindow(self.hwnd);
        TrackPopupMenu(menu, TPM_RIGHTBUTTON, point.x, point.y, 0, self.hwnd, ptr::null());
        DestroyMenu(menu);
    }

    unsafe fn handle_command(&mut self, command_id: usize) {
        match command_id {
            ID_ALWAYS | MENU_TOGGLE_ALWAYS => self.toggle_always_enabled(),
            ID_STARTUP | MENU_TOGGLE_STARTUP => self.toggle_startup(),
            ID_PRERELEASES => self.toggle_prerelease_updates(),
            ID_CHECK_UPDATES | MENU_CHECK_UPDATES => self.start_update_check(),
            ID_INSTALL_UPDATE | MENU_INSTALL_UPDATE => self.install_update(),
            ID_OPEN_RELEASES | MENU_OPEN_RELEASES => {
                if let Err(error) = updater::open_releases_page() {
                    self.status = format!("Open releases failed: {error}");
                    self.refresh_controls();
                }
            }
            ID_HIDE => self.hide_window(),
            MENU_OPEN => self.show_window(),
            MENU_EXIT => {
                DestroyWindow(self.hwnd);
            }
            _ => {}
        }
    }
}

fn update_status(check: &updater::UpdateCheck) -> String {
    let kind = if check.prerelease { "prerelease" } else { "stable" };
    if check.is_update_available {
        if check.asset_download_url.is_some() {
            format!("Update available: {kind} v{}.", check.latest_version)
        } else {
            format!("Update available: {kind} v{}, but no Windows executable asset was found.", check.latest_version)
        }
    } else {
        format!("No newer {kind} release. Current version: v{}.", check.current_version)
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_CREATE => 0,
        WM_COMMAND => {
            if let Some(app) = app_from_hwnd(hwnd) {
                let command_id = wparam & 0xffff;
                app.handle_command(command_id as usize);
            }
            0
        }
        WM_HOTKEY => {
            if wparam as i32 == HOTKEY_TOGGLE_ALWAYS {
                if let Some(app) = app_from_hwnd(hwnd) {
                    app.toggle_always_enabled();
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
                    WM_LBUTTONDBLCLK => app.show_window(),
                    _ => {}
                }
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
                app.remove_tray_icon();
                app.save();
            }
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn load_app_icon() -> *mut std::ffi::c_void {
    let instance = GetModuleHandleW(ptr::null());
    let icon = LoadIconW(instance, make_int_resource(APP_ICON_RESOURCE_ID));
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
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App;
    ptr.as_mut()
}

unsafe fn create_static(parent: HWND, id: usize, x: i32, y: i32, width: i32, height: i32, text: &str) {
    let class = str_wide_null("STATIC");
    let text = str_wide_null(text);
    CreateWindowExW(
        0,
        class.as_ptr(),
        text.as_ptr(),
        WS_CHILD | WS_VISIBLE,
        x,
        y,
        width,
        height,
        parent,
        id as HMENU,
        GetModuleHandleW(ptr::null()),
        ptr::null_mut(),
    );
}

unsafe fn create_button(parent: HWND, id: usize, x: i32, y: i32, width: i32, height: i32, text: &str) {
    let class = str_wide_null("BUTTON");
    let text = str_wide_null(text);
    CreateWindowExW(
        0,
        class.as_ptr(),
        text.as_ptr(),
        WS_CHILD | WS_VISIBLE,
        x,
        y,
        width,
        height,
        parent,
        id as HMENU,
        GetModuleHandleW(ptr::null()),
        ptr::null_mut(),
    );
}

unsafe fn set_control_text(parent: HWND, id: usize, text: &str) {
    let control = GetDlgItem(parent, id as i32);
    if !control.is_null() {
        let text = str_wide_null(text);
        SetWindowTextW(control, text.as_ptr());
    }
}

unsafe fn message_box(hwnd: HWND, text: &str, title: &str, flags: u32) -> i32 {
    let text = str_wide_null(text);
    let title = str_wide_null(title);
    MessageBoxW(hwnd, text.as_ptr(), title.as_ptr(), flags)
}

unsafe fn append_menu(menu: HMENU, id: usize, text: &str) {
    let text = str_wide_null(text);
    AppendMenuW(menu, MF_STRING, id, text.as_ptr());
}

unsafe fn append_separator(menu: HMENU) {
    AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
}
