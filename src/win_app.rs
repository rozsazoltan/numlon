use anyhow::{Context as _, Result};
use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, RichText, Sense, Stroke};
use global_hotkey::{
    hotkey::HotKey,
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use std::{
    env,
    ptr,
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};
use tray_icon::{
    menu::{IconMenuItem, Menu, MenuEvent},
    Icon as TrayImage, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowW, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
};

use crate::{
    config::{self, NumlockMode, SavedState},
    hotkey::HotkeyBinding,
    keyboard_hook::KeyboardHook,
    numlock, startup, updater,
    wide::str_wide_null,
};

const ENFORCE_INTERVAL: Duration = Duration::from_millis(300);
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(80);
const AUTO_UPDATE_INTERVAL_SECONDS: u64 = 60 * 60;

const BACKGROUND: Color32 = Color32::from_rgb(246, 246, 244);
const SURFACE: Color32 = Color32::from_rgb(255, 255, 255);
const SURFACE_MUTED: Color32 = Color32::from_rgb(249, 249, 247);
const BORDER: Color32 = Color32::from_rgb(226, 226, 223);
const TEXT: Color32 = Color32::from_rgb(28, 28, 30);
const MUTED: Color32 = Color32::from_rgb(103, 103, 100);
const YELLOW: Color32 = Color32::from_rgb(255, 201, 40);
const YELLOW_SOFT: Color32 = Color32::from_rgb(255, 248, 218);
const GRAPHITE: Color32 = Color32::from_rgb(37, 41, 50);

const MENU_OPEN: &str = "open";
const MENU_TOGGLE: &str = "toggle";
const MENU_FORCE: &str = "force";
const MENU_LED_OFF: &str = "led-off";
const MENU_SHORTCUT: &str = "shortcut";
const MENU_STARTUP: &str = "startup";
const MENU_PRERELEASE: &str = "prerelease";
const MENU_CHECK: &str = "check";
const MENU_INSTALL: &str = "install";
const MENU_RELEASES: &str = "releases";
const MENU_QUIT: &str = "quit";

pub fn started_from_startup() -> bool {
    env::args_os().any(|argument| argument == "--startup")
}

pub fn activate_existing_instance() {
    let title = str_wide_null(&config::window_title());

    for _ in 0..20 {
        let hwnd = unsafe { FindWindowW(ptr::null(), title.as_ptr()) };
        if !hwnd.is_null() {
            unsafe {
                ShowWindow(hwnd, if IsIconic(hwnd) != 0 { SW_RESTORE } else { SW_SHOW });
                SetForegroundWindow(hwnd);
            }
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

pub fn run() -> Result<()> {
    let icon = load_window_icon(include_bytes!("../assets/numlon.png"))?;
    let visible = config::is_dev_build() || !started_from_startup();
    let viewport = egui::ViewportBuilder::default()
        .with_title(config::window_title())
        .with_inner_size([620.0, 520.0])
        .with_min_inner_size([520.0, 420.0])
        .with_resizable(true)
        .with_visible(visible)
        .with_icon(icon);

    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        multisampling: 0,
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        config::app_name(),
        options,
        Box::new(|creation_context| Ok(Box::new(NumlonApp::new(creation_context)))),
    )
    .map_err(|error| anyhow::anyhow!("failed to run Numlon UI: {error}"))
}

struct TrayState {
    icon: TrayIcon,
    toggle: IconMenuItem,
    force: IconMenuItem,
    led_off: IconMenuItem,
    shortcut: IconMenuItem,
    startup: IconMenuItem,
    prerelease: Option<IconMenuItem>,
    install: Option<IconMenuItem>,
}

impl TrayState {
    fn new(state: &SavedState, keyboard_hook_available: bool) -> Result<Self> {
        let menu = Menu::new();
        let open = IconMenuItem::with_id(MENU_OPEN, "Open Numlon", true, None, None);
        let toggle = IconMenuItem::with_id(MENU_TOGGLE, "", true, None, None);
        let force = IconMenuItem::with_id(MENU_FORCE, "", true, None, None);
        let led_off = IconMenuItem::with_id(MENU_LED_OFF, "", keyboard_hook_available, None, None);
        let shortcut = IconMenuItem::with_id(MENU_SHORTCUT, "", true, None, None);
        let startup = IconMenuItem::with_id(
            MENU_STARTUP,
            "",
            !config::is_dev_build(),
            None,
            None,
        );
        let quit = IconMenuItem::with_id(MENU_QUIT, "Quit Numlon", true, None, None);

        menu.append_items(&[
            &open,
            &toggle,
            &force,
            &led_off,
            &shortcut,
            &startup,
        ])?;

        let (prerelease, install) = if config::is_dev_build() {
            (None, None)
        } else {
            let prerelease = IconMenuItem::with_id(MENU_PRERELEASE, "", true, None, None);
            let check = IconMenuItem::with_id(MENU_CHECK, "Check for updates", true, None, None);
            let install = IconMenuItem::with_id(
                MENU_INSTALL,
                "Install available update",
                false,
                None,
                None,
            );
            let releases =
                IconMenuItem::with_id(MENU_RELEASES, "Open releases", true, None, None);
            menu.append_items(&[&prerelease, &check, &install, &releases])?;
            (Some(prerelease), Some(install))
        };
        menu.append(&quit)?;

        let icon = TrayIconBuilder::new()
            .with_id("numlon")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_menu_on_right_click(true)
            .with_icon(load_tray_image(if state.always_enabled {
                include_bytes!("../assets/numlon-tray.png")
            } else {
                include_bytes!("../assets/numlon-paused-tray.png")
            })?)
            .with_tooltip(tray_tooltip(state))
            .build()?;

        let tray = Self {
            icon,
            toggle,
            force,
            led_off,
            shortcut,
            startup,
            prerelease,
            install,
        };
        tray.sync(state, keyboard_hook_available, false);
        Ok(tray)
    }

    fn sync(&self, state: &SavedState, keyboard_hook_available: bool, update_installable: bool) {
        self.toggle.set_text(if state.always_enabled {
            "✓ Numlon enabled"
        } else {
            "Numlon paused"
        });
        self.force.set_text(if state.numlock_mode == NumlockMode::ForceOn {
            "✓ Keep NumLock on"
        } else {
            "Keep NumLock on"
        });
        self.led_off.set_text(if state.numlock_mode == NumlockMode::LedOffDigits {
            "✓ Keep LED off, type digits"
        } else {
            "Keep LED off, type digits"
        });
        self.led_off.set_enabled(keyboard_hook_available);
        self.shortcut
            .set_text(format!("Change shortcut…  {}", state.hotkey.display()));
        self.startup.set_text(if state.startup_enabled {
            "✓ Start with Windows"
        } else {
            "Start with Windows"
        });
        if let Some(prerelease) = &self.prerelease {
            prerelease.set_text(if state.include_prereleases {
                "✓ Include prereleases"
            } else {
                "Include prereleases"
            });
        }
        if let Some(install) = &self.install {
            install.set_enabled(update_installable);
        }

        let icon = load_tray_image(if state.always_enabled {
            include_bytes!("../assets/numlon-tray.png")
        } else {
            include_bytes!("../assets/numlon-paused-tray.png")
        });
        if let Ok(icon) = icon {
            let _ = self.icon.set_icon(Some(icon));
        }
        let _ = self.icon.set_tooltip(Some(tray_tooltip(state)));
    }
}

struct NumlonApp {
    state: SavedState,
    status: String,
    keyboard_hook: Option<KeyboardHook>,
    hotkey_manager: Option<GlobalHotKeyManager>,
    registered_hotkey: Option<HotKey>,
    tray: Option<TrayState>,
    capturing_hotkey: bool,
    startup_prompt_open: bool,
    quit_requested: bool,
    last_enforce: Instant,
    last_update_check: Option<updater::UpdateCheck>,
    update_rx: Option<Receiver<anyhow::Result<updater::UpdateCheck>>>,
}

impl NumlonApp {
    fn new(creation_context: &eframe::CreationContext<'_>) -> Self {
        configure_egui(&creation_context.egui_ctx);

        let mut state = config::load_state();
        let mut status = if state.last_status.is_empty() {
            "Ready.".to_owned()
        } else {
            state.last_status.clone()
        };

        match startup::is_enabled() {
            Ok(enabled) => state.startup_enabled = enabled,
            Err(error) => status = format!("Startup check failed: {error}"),
        }

        let keyboard_hook = match KeyboardHook::install() {
            Ok(hook) => Some(hook),
            Err(error) => {
                status = format!("LED-off mode unavailable: {error}");
                None
            }
        };

        let hotkey_manager = match GlobalHotKeyManager::new() {
            Ok(manager) => Some(manager),
            Err(error) => {
                status = format!("Global shortcut manager failed: {error}");
                None
            }
        };

        let tray = match TrayState::new(&state, keyboard_hook.is_some()) {
            Ok(tray) => Some(tray),
            Err(error) => {
                status = format!("Tray initialization failed: {error}");
                None
            }
        };

        let startup_prompt_open = !config::is_dev_build() && !state.startup_prompted;
        let mut app = Self {
            state,
            status,
            keyboard_hook,
            hotkey_manager,
            registered_hotkey: None,
            tray,
            capturing_hotkey: false,
            startup_prompt_open,
            quit_requested: false,
            last_enforce: Instant::now() - ENFORCE_INTERVAL,
            last_update_check: None,
            update_rx: None,
        };
        app.register_saved_hotkey();
        app.apply_runtime_mode();
        app.maybe_start_auto_update_check();
        app.sync_tray();
        creation_context
            .egui_ctx
            .request_repaint_after(EVENT_POLL_INTERVAL);
        app
    }

    fn register_saved_hotkey(&mut self) {
        let Some(manager) = &self.hotkey_manager else {
            return;
        };
        match self.state.hotkey.to_global_hotkey() {
            Ok(hotkey) => match manager.register(hotkey) {
                Ok(()) => self.registered_hotkey = Some(hotkey),
                Err(error) => {
                    self.status = format!(
                        "Shortcut {} is unavailable: {error}",
                        self.state.hotkey.display()
                    );
                }
            },
            Err(error) => self.status = error,
        }
    }

    fn unregister_hotkey(&mut self) {
        if let (Some(manager), Some(hotkey)) = (&self.hotkey_manager, self.registered_hotkey.take())
        {
            let _ = manager.unregister(hotkey);
        }
    }

    fn save(&mut self) {
        self.state.last_status = self.status.clone();
        if let Err(error) = config::save_state(&self.state) {
            self.status = format!("Config save failed: {error}");
        }
    }

    fn sync_tray(&self) {
        if let Some(tray) = &self.tray {
            tray.sync(
                &self.state,
                self.keyboard_hook.is_some(),
                self.update_is_installable(),
            );
        }
    }

    fn show_window(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn hide_window(&mut self, ctx: &egui::Context) {
        self.save();
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    fn toggle_enabled(&mut self) {
        self.state.always_enabled = !self.state.always_enabled;
        self.apply_runtime_mode();
        self.status = if self.state.always_enabled {
            format!("Enabled: {}.", self.state.numlock_mode.label())
        } else {
            "Numlon paused. NumLock left untouched.".to_owned()
        };
        self.save();
        self.sync_tray();
    }

    fn set_mode(&mut self, mode: NumlockMode) {
        if mode == NumlockMode::LedOffDigits && self.keyboard_hook.is_none() {
            self.status = "LED-off digit mode unavailable: keyboard hook failed.".to_owned();
            return;
        }
        self.state.numlock_mode = mode;
        self.apply_runtime_mode();
        self.status = format!("Mode changed: {}.", mode.label());
        self.save();
        self.sync_tray();
    }

    fn apply_runtime_mode(&mut self) {
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
                    self.status = "LED-off digit mode unavailable: keyboard hook failed.".to_owned();
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

    fn enforce_numlock(&mut self) {
        if !self.state.always_enabled || self.last_enforce.elapsed() < ENFORCE_INTERVAL {
            return;
        }
        self.last_enforce = Instant::now();
        let result = match self.state.numlock_mode {
            NumlockMode::ForceOn => numlock::ensure_numlock_on(),
            NumlockMode::LedOffDigits => numlock::ensure_numlock_off(),
        };
        if let Err(error) = result {
            self.status = format!("NumLock state update failed: {error}");
        }
    }

    fn begin_hotkey_capture(&mut self, ctx: &egui::Context) {
        self.unregister_hotkey();
        self.capturing_hotkey = true;
        self.status = "Press shortcut now. Escape cancels.".to_owned();
        self.show_window(ctx);
    }

    fn poll_hotkey_capture(&mut self, ctx: &egui::Context) {
        if !self.capturing_hotkey {
            return;
        }
        let events = ctx.input(|input| input.events.clone());
        for event in events {
            let egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers,
                ..
            } = event
            else {
                continue;
            };

            if key == egui::Key::Escape {
                self.capturing_hotkey = false;
                self.register_saved_hotkey();
                self.status = "Shortcut change cancelled.".to_owned();
                return;
            }

            let Some(key_name) = egui_key_name(key) else {
                continue;
            };
            let candidate = HotkeyBinding {
                ctrl: modifiers.ctrl,
                alt: modifiers.alt,
                shift: modifiers.shift,
                win: key_is_down(windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_LWIN)
                    || key_is_down(windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_RWIN),
                key: key_name,
            };
            let Some(manager) = &self.hotkey_manager else {
                self.capturing_hotkey = false;
                self.status = "Global shortcut manager unavailable.".to_owned();
                return;
            };
            match candidate.to_global_hotkey() {
                Ok(hotkey) => match manager.register(hotkey) {
                    Ok(()) => {
                        self.registered_hotkey = Some(hotkey);
                        self.state.hotkey = candidate;
                        self.capturing_hotkey = false;
                        self.status = format!("Shortcut saved: {}.", self.state.hotkey.display());
                        self.save();
                        self.sync_tray();
                        return;
                    }
                    Err(error) => {
                        self.capturing_hotkey = false;
                        self.register_saved_hotkey();
                        self.status = format!("Shortcut unavailable: {error}");
                        return;
                    }
                },
                Err(error) => self.status = error,
            }
        }
    }

    fn toggle_startup(&mut self) {
        if config::is_dev_build() {
            self.status = "Startup changes disabled in dev builds.".to_owned();
            return;
        }
        let target = !self.state.startup_enabled;
        match startup::set_enabled(target) {
            Ok(()) => {
                self.state.startup_enabled = target;
                self.status = if target {
                    "Windows startup enabled.".to_owned()
                } else {
                    "Windows startup disabled.".to_owned()
                };
                self.save();
                self.sync_tray();
            }
            Err(error) => self.status = format!("Startup update failed: {error}"),
        }
    }

    fn toggle_prerelease_updates(&mut self) {
        if config::is_dev_build() {
            self.status = "Update checks disabled in dev builds.".to_owned();
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
        self.sync_tray();
    }

    fn maybe_start_auto_update_check(&mut self) {
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

    fn start_update_check(&mut self) {
        if config::is_dev_build() {
            self.status = "Update checks disabled in dev builds.".to_owned();
            return;
        }
        if self.update_rx.is_some() {
            return;
        }
        let include_prereleases = self.state.include_prereleases;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(updater::check_for_update(include_prereleases));
        });
        self.update_rx = Some(rx);
        self.status = "Checking for updates…".to_owned();
    }

    fn poll_update_check(&mut self) {
        let Some(receiver) = self.update_rx.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok(Ok(check)) => {
                self.status = update_status(&check);
                self.last_update_check = Some(check);
                self.update_rx = None;
                self.save();
                self.sync_tray();
            }
            Ok(Err(error)) => {
                self.status = format!("Update check failed: {error}");
                self.update_rx = None;
                self.save();
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.status = "Update worker disconnected.".to_owned();
                self.update_rx = None;
            }
        }
    }

    fn install_update(&mut self) {
        if config::is_dev_build() {
            self.status = "Updates disabled in dev builds.".to_owned();
            return;
        }
        let Some(check) = self.last_update_check.clone() else {
            self.status = "Check for updates first.".to_owned();
            return;
        };
        if !self.update_is_installable() {
            self.status = "No installable update available.".to_owned();
            return;
        }
        self.status = "Installing update…".to_owned();
        if let Err(error) = updater::install_update(&check) {
            self.status = format!("Update install failed: {error}");
        }
    }

    fn update_is_installable(&self) -> bool {
        self.last_update_check
            .as_ref()
            .is_some_and(|check| check.is_update_available && check.asset_download_url.is_some())
    }

    fn poll_global_hotkey(&mut self) {
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state != HotKeyState::Pressed {
                continue;
            }
            if self
                .registered_hotkey
                .as_ref()
                .is_some_and(|hotkey| hotkey.id() == event.id)
            {
                self.toggle_enabled();
            }
        }
    }

    fn poll_tray(&mut self, ctx: &egui::Context) {
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
                | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } => self.show_window(ctx),
                _ => {}
            }
        }

        while let Ok(event) = MenuEvent::receiver().try_recv() {
            match event.id.as_ref() {
                MENU_OPEN => self.show_window(ctx),
                MENU_TOGGLE => self.toggle_enabled(),
                MENU_FORCE => self.set_mode(NumlockMode::ForceOn),
                MENU_LED_OFF => self.set_mode(NumlockMode::LedOffDigits),
                MENU_SHORTCUT => self.begin_hotkey_capture(ctx),
                MENU_STARTUP => self.toggle_startup(),
                MENU_PRERELEASE => self.toggle_prerelease_updates(),
                MENU_CHECK => self.start_update_check(),
                MENU_INSTALL => self.install_update(),
                MENU_RELEASES => {
                    if let Err(error) = updater::open_releases_page() {
                        self.status = format!("Open releases failed: {error}");
                    }
                }
                MENU_QUIT => {
                    self.quit_requested = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                _ => {}
            }
        }
    }

    fn render_startup_prompt(&mut self, ctx: &egui::Context) {
        if !self.startup_prompt_open {
            return;
        }
        egui::Window::new("Start Numlon with Windows?")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_width(400.0);
                ui.label("Move numlon.exe to its final folder first. Windows stores its exact path; do not move it after enabling startup.");
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Not now").clicked() {
                        self.state.startup_prompted = true;
                        self.startup_prompt_open = false;
                        self.save();
                    }
                    if ui.button("Enable startup").clicked() {
                        self.state.startup_prompted = true;
                        self.startup_prompt_open = false;
                        if let Err(error) = startup::set_enabled(true) {
                            self.status = format!("Startup update failed: {error}");
                        } else {
                            self.state.startup_enabled = true;
                            self.status = "Windows startup enabled.".to_owned();
                        }
                        self.save();
                        self.sync_tray();
                    }
                });
            });
    }
}

impl eframe::App for NumlonApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|input| input.viewport().close_requested()) && !self.quit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_window(ctx);
        }
        self.poll_global_hotkey();
        self.poll_tray(ctx);
        self.poll_hotkey_capture(ctx);
        self.enforce_numlock();
        self.poll_update_check();
        ctx.request_repaint_after(EVENT_POLL_INTERVAL);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.painter().rect_filled(ui.max_rect(), 0, BACKGROUND);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    ui.vertical(|ui| {
                        ui.set_max_width((ui.available_width() - 20.0).max(320.0));
                        header(ui);
                        ui.add_space(14.0);

                        status_card(ui, self);
                        ui.add_space(10.0);

                        section_label(ui, "Behavior");
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            let available = ui.available_width();
                            let width = ((available - 8.0) / 2.0).max(210.0);
                            if mode_option(
                                ui,
                                width,
                                self.state.numlock_mode == NumlockMode::ForceOn,
                                "NumLock on",
                                "Keeps keypad numeric",
                                true,
                            )
                            .clicked()
                            {
                                self.set_mode(NumlockMode::ForceOn);
                            }
                            if mode_option(
                                ui,
                                width,
                                self.state.numlock_mode == NumlockMode::LedOffDigits,
                                "LED off",
                                "Maps keypad to digits",
                                self.keyboard_hook.is_some(),
                            )
                            .clicked()
                            {
                                self.set_mode(NumlockMode::LedOffDigits);
                            }
                        });
                        ui.add_space(10.0);

                        settings_surface(ui, self);
                        ui.add_space(12.0);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&self.status).size(11.0).color(MUTED));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new("Hide").strong())
                                            .min_size(egui::vec2(92.0, 34.0))
                                            .corner_radius(9),
                                    )
                                    .clicked()
                                {
                                    self.hide_window(ui.ctx());
                                }
                            });
                        });
                        ui.add_space(16.0);
                    });
                });
            });
        self.render_startup_prompt(ui.ctx());
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.unregister_hotkey();
        KeyboardHook::set_remap_active(false);
        self.save();
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        BACKGROUND.to_normalized_gamma_f32()
    }
}

fn configure_egui(ctx: &egui::Context) {
    const THEME: egui::Theme = egui::Theme::Light;

    ctx.set_theme(THEME);

    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = SURFACE_MUTED;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(9);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(9);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(9);
    ctx.set_visuals_of(THEME, visuals);

    let mut style = (*ctx.style_of(THEME)).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 8.0);
    style.spacing.interact_size.y = 34.0;
    ctx.set_style_of(THEME, style);
}

fn header(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        draw_logo(ui, 44.0, true);
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.label(RichText::new("Numlon").size(21.0).strong().color(TEXT));
            ui.label(
                RichText::new("Tiny keypad control, without LED drama.")
                    .size(11.5)
                    .color(MUTED),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            egui::Frame::new()
                .fill(YELLOW_SOFT)
                .stroke(Stroke::new(1.0, YELLOW))
                .corner_radius(egui::CornerRadius::same(16))
                .inner_margin(egui::Margin::symmetric(12, 6))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(config::app_version_label())
                            .size(11.0)
                            .strong()
                            .color(GRAPHITE),
                    );
                });
        });
    });
}

fn status_card(ui: &mut egui::Ui, app: &mut NumlonApp) {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(if app.state.always_enabled {
                            "Numlon active"
                        } else {
                            "Numlon paused"
                        })
                        .size(15.0)
                        .strong()
                        .color(TEXT),
                    );
                    ui.label(
                        RichText::new(if app.state.always_enabled {
                            app.state.numlock_mode.label()
                        } else {
                            "Keyboard state remains untouched"
                        })
                        .size(11.0)
                        .color(MUTED),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let response = toggle_switch(ui, app.state.always_enabled, true);
                    if response.clicked() {
                        app.toggle_enabled();
                    }
                });
            });
        });
}

fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(12.0).strong().color(TEXT));
}

fn mode_option(
    ui: &mut egui::Ui,
    width: f32,
    selected: bool,
    title: &str,
    subtitle: &str,
    enabled: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 54.0), Sense::click());
    let response = if enabled { response } else { response.on_disabled_hover_text("Unavailable") };
    let fill = if selected { YELLOW_SOFT } else { SURFACE_MUTED };
    let stroke = if selected {
        Stroke::new(1.0, YELLOW)
    } else {
        Stroke::new(1.0, BORDER)
    };
    let corner_radius = egui::CornerRadius::same(10);
    ui.painter().rect_filled(rect, corner_radius, fill);
    ui.painter()
        .rect_stroke(rect, corner_radius, stroke, egui::StrokeKind::Inside);

    let radio_center = egui::pos2(rect.left() + 20.0, rect.center().y);
    ui.painter().circle_filled(radio_center, 9.0, SURFACE);
    ui.painter()
        .circle_stroke(radio_center, 9.0, Stroke::new(1.0, if selected { YELLOW } else { BORDER }));
    if selected {
        ui.painter().circle_filled(radio_center, 4.0, GRAPHITE);
    }
    let color = if enabled { TEXT } else { MUTED };
    ui.painter().text(
        egui::pos2(rect.left() + 38.0, rect.top() + 14.0),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(12.5),
        color,
    );
    ui.painter().text(
        egui::pos2(rect.left() + 38.0, rect.top() + 35.0),
        Align2::LEFT_CENTER,
        subtitle,
        FontId::proportional(10.5),
        MUTED,
    );
    response
}

fn settings_surface(ui: &mut egui::Ui, app: &mut NumlonApp) {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::symmetric(14, 4))
        .show(ui, |ui| {
            setting_row(ui, "Toggle shortcut", &app.state.hotkey.display(), |ui| {
                if ui
                    .add(
                        egui::Button::new(if app.capturing_hotkey {
                            "Listening…"
                        } else {
                            "Change"
                        })
                        .min_size(egui::vec2(100.0, 34.0))
                        .fill(YELLOW)
                        .stroke(Stroke::NONE)
                        .corner_radius(9),
                    )
                    .clicked()
                {
                    app.begin_hotkey_capture(ui.ctx());
                }
            });
            ui.separator();
            setting_row(
                ui,
                "Start with Windows",
                if config::is_dev_build() {
                    "Unavailable in development builds"
                } else if app.state.startup_enabled {
                    "Uses current executable path"
                } else {
                    "Keep executable in final folder before enabling"
                },
                |ui| {
                    let response = toggle_switch(
                        ui,
                        app.state.startup_enabled && !config::is_dev_build(),
                        !config::is_dev_build(),
                    );
                    if response.clicked() && !config::is_dev_build() {
                        app.toggle_startup();
                    }
                },
            );
            ui.separator();
            setting_row(
                ui,
                "Updates",
                if config::is_dev_build() {
                    "Disabled in dev — no GitHub API requests"
                } else if app.state.include_prereleases {
                    "Prerelease channel"
                } else {
                    "Stable channel"
                },
                |ui| {
                    if config::is_dev_build() {
                        ui.label(RichText::new("Dev").size(11.0).color(MUTED));
                    } else {
                        if ui
                            .add(
                                egui::Button::new(if app.update_is_installable() {
                                    "Install"
                                } else {
                                    "Check"
                                })
                                .min_size(egui::vec2(74.0, 32.0))
                                .corner_radius(8),
                            )
                            .clicked()
                        {
                            if app.update_is_installable() {
                                app.install_update();
                            } else {
                                app.start_update_check();
                            }
                        }
                        let response = toggle_switch(ui, app.state.include_prereleases, true);
                        if response.clicked() {
                            app.toggle_prerelease_updates();
                        }
                    }
                },
            );
        });
}

fn setting_row(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    trailing: impl FnOnce(&mut egui::Ui),
) {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 58.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(title).size(13.0).strong().color(TEXT));
                ui.label(RichText::new(subtitle).size(10.5).color(MUTED));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), trailing);
        },
    );
}

fn toggle_switch(ui: &mut egui::Ui, on: bool, enabled: bool) -> egui::Response {
    let size = egui::vec2(48.0, 28.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let response = if enabled { response } else { response.on_disabled_hover_text("Unavailable") };
    let animation = ui.ctx().animate_bool(response.id, on);
    let track = if enabled {
        if on { YELLOW } else { Color32::from_rgb(206, 207, 204) }
    } else {
        Color32::from_rgb(220, 220, 217)
    };
    let track_radius = rect.height() * 0.5;
    let knob_radius = 10.0;
    ui.painter().rect_filled(rect, track_radius, track);
    let x = egui::lerp(
        (rect.left() + track_radius)..=(rect.right() - track_radius),
        animation,
    );
    ui.painter()
        .circle_filled(egui::pos2(x, rect.center().y), knob_radius, SURFACE);
    response
}

fn draw_logo(ui: &mut egui::Ui, size: f32, active: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 8, GRAPHITE);
    let gap = size * 0.085;
    let padding = size * 0.18;
    let key = (size - padding * 2.0 - gap * 2.0) / 3.0;
    for row in 0..3 {
        for column in 0..3 {
            let min = egui::pos2(
                rect.left() + padding + column as f32 * (key + gap),
                rect.top() + padding + row as f32 * (key + gap),
            );
            let key_rect = egui::Rect::from_min_size(min, egui::vec2(key, key));
            let is_active = row == 2 && column == 2;
            painter.rect_filled(
                key_rect,
                2,
                if is_active {
                    if active { YELLOW } else { Color32::from_rgb(139, 145, 157) }
                } else {
                    Color32::from_rgb(233, 235, 239)
                },
            );
        }
    }
}

fn load_window_icon(bytes: &[u8]) -> Result<egui::IconData> {
    let image = image::load_from_memory(bytes)
        .context("failed to decode embedded window icon")?
        .into_rgba8();
    let (width, height) = image.dimensions();
    Ok(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

fn load_tray_image(bytes: &[u8]) -> Result<TrayImage> {
    let image = image::load_from_memory(bytes)
        .context("failed to decode embedded tray icon")?
        .into_rgba8();
    let (width, height) = image.dimensions();
    TrayImage::from_rgba(image.into_raw(), width, height)
        .map_err(|error| anyhow::anyhow!("invalid tray icon: {error}"))
}

fn tray_tooltip(state: &SavedState) -> String {
    format!(
        "Numlon {} — {} — {}",
        config::app_version_label(),
        if state.always_enabled {
            state.numlock_mode.label()
        } else {
            "Paused"
        },
        state.hotkey.display()
    )
}

fn update_status(check: &updater::UpdateCheck) -> String {
    if check.is_update_available {
        format!("Update available: v{}.", check.latest_version)
    } else {
        format!("No newer release. Current version: v{}.", check.current_version)
    }
}

fn egui_key_name(key: egui::Key) -> Option<String> {
    use egui::Key;
    let name = match key {
        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "PageUp",
        Key::PageDown => "PageDown",
        Key::Insert => "Insert",
        Key::Delete => "Delete",
        Key::ArrowLeft => "Left",
        Key::ArrowRight => "Right",
        Key::ArrowUp => "Up",
        Key::ArrowDown => "Down",
        Key::Space => "Space",
        Key::Tab => "Tab",
        Key::Enter => "Enter",
        Key::Escape => "Escape",
        Key::Num0 => "0",
        Key::Num1 => "1",
        Key::Num2 => "2",
        Key::Num3 => "3",
        Key::Num4 => "4",
        Key::Num5 => "5",
        Key::Num6 => "6",
        Key::Num7 => "7",
        Key::Num8 => "8",
        Key::Num9 => "9",
        Key::A => "A",
        Key::B => "B",
        Key::C => "C",
        Key::D => "D",
        Key::E => "E",
        Key::F => "F",
        Key::G => "G",
        Key::H => "H",
        Key::I => "I",
        Key::J => "J",
        Key::K => "K",
        Key::L => "L",
        Key::M => "M",
        Key::N => "N",
        Key::O => "O",
        Key::P => "P",
        Key::Q => "Q",
        Key::R => "R",
        Key::S => "S",
        Key::T => "T",
        Key::U => "U",
        Key::V => "V",
        Key::W => "W",
        Key::X => "X",
        Key::Y => "Y",
        Key::Z => "Z",
        Key::F1 => "F1",
        Key::F2 => "F2",
        Key::F3 => "F3",
        Key::F4 => "F4",
        Key::F5 => "F5",
        Key::F6 => "F6",
        Key::F7 => "F7",
        Key::F8 => "F8",
        Key::F9 => "F9",
        Key::F10 => "F10",
        Key::F11 => "F11",
        Key::F12 => "F12",
        Key::F13 => "F13",
        Key::F14 => "F14",
        Key::F15 => "F15",
        Key::F16 => "F16",
        Key::F17 => "F17",
        Key::F18 => "F18",
        Key::F19 => "F19",
        Key::F20 => "F20",
        Key::F21 => "F21",
        Key::F22 => "F22",
        Key::F23 => "F23",
        Key::F24 => "F24",
        _ => return None,
    };
    Some(name.to_owned())
}

fn key_is_down(key: u16) -> bool {
    unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState(key as i32) < 0 }
}
