#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod config;
mod hotkey;
mod updater;

#[cfg(windows)]
mod keyboard_hook;
#[cfg(windows)]
mod numlock;
#[cfg(windows)]
mod single_instance;
#[cfg(windows)]
mod startup;
#[cfg(windows)]
mod wide;
#[cfg(windows)]
mod win_app;

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    let _guard = match single_instance::acquire()? {
        Some(guard) => guard,
        None => {
            win_app::activate_existing_instance();
            return Ok(());
        }
    };

    win_app::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Numlon is a Windows-only tray app.");
}
