# numlon

`numlon` is a tiny Windows tray app that keeps NumLock in charge. It keeps NumLock enabled in the background so the numeric keypad keeps typing numbers, even after NumLock is turned off accidentally.

- [What it does](#what-it-does)
  - [NumLock guard](#numlock-guard)
  - [Hotkey](#hotkey)
  - [Tray mode](#tray-mode)
  - [Startup](#startup)
  - [Updates](#updates)
  - [Design assets](#design-assets)
- [Get started](#get-started)
- [Usage](#usage)
  - [Toggle always-on mode](#toggle-always-on-mode)
  - [Use the tray menu](#use-the-tray-menu)
  - [Enable startup](#enable-startup)
  - [Check for updates](#check-for-updates)
- [Window behavior](#window-behavior)
- [Data location](#data-location)
- [Known limitations](#known-limitations)
- [Contributing](#contributing)
- [Development runner](#development-runner)
  - [WSL source and Windows runtime](#wsl-source-and-windows-runtime)

## What it does

Numlon runs as a small Windows tray utility. Its default behavior is simple: keep NumLock enabled while the always-on feature is active.

### NumLock guard

Numlon checks the NumLock state on a short interval. If NumLock is off and always-on mode is enabled, Numlon turns it back on through normal Windows keyboard input APIs.

It does not install a low-level keyboard hook, does not capture typed keys, does not log input, and does not require administrator privileges.

### Hotkey

Press `Win+Alt+Home` to toggle always-on mode.

When always-on mode is disabled, Numlon leaves NumLock alone. Press the hotkey again to resume enforcement.

### Tray mode

Numlon lives in the Windows notification area. The tray icon supports a right-click menu with actions for opening the small control window, toggling always-on mode, toggling startup, checking updates, opening GitHub Releases, installing an available update, and exiting the app.

The main window is intentionally minimal. Closing the window hides Numlon back to tray instead of exiting it.

### Startup

Startup is optional. On first launch, Numlon asks whether it should start with Windows.

Before enabling startup, move `numlon.exe` to its final folder. Do not move it afterward. Windows stores the exact executable path in the current user's startup registry entry, so moving the file later breaks startup until the setting is disabled and enabled again.

### Updates

Numlon checks GitHub releases for `rozsazoltan/numlon` and can replace the current Windows executable when a newer build is available.

Stable releases are checked by default. Prerelease watching can be enabled from the app window. When prerelease watching is disabled, Numlon returns to stable release checks.

Release builds perform a background update check at most once per hour. Manual checks are available from the app window and tray menu.

### Design assets

Numlon uses yellow as its primary accent color.

```text
Primary accent:
  #FACC15
```

Executable and tray icon assets live in:

```text
assets/numlon.ico
assets/numlon-icon-source.png
```

## Get started

Recommended layout:

```text
numlon/
├─ numlon.exe
└─ .numlon-data/
   └─ state.json
```

Move `numlon.exe` to its final folder, run it, then choose whether startup should be enabled.

## Usage

### Toggle always-on mode

Use `Win+Alt+Home`, the app window, or the tray menu.

When always-on mode is active, Numlon restores NumLock if it becomes disabled. When always-on mode is paused, Numlon does not change NumLock.

### Use the tray menu

Right-click the Numlon tray icon to access common actions:

- open the app window
- enable or disable always-on mode
- enable or disable startup
- check updates
- install an available update
- open GitHub Releases
- exit Numlon

### Enable startup

Use the first-run prompt, the app window, or the tray menu.

Startup is written only for the current Windows user under:

```text
HKCU\Software\Microsoft\Windows\CurrentVersion\Run
```

The startup command points to the current `numlon.exe` path.

### Check for updates

Use **Check updates** from the app window or tray menu.

Numlon looks for a Windows executable asset in the latest GitHub release. Preferred asset names are:

```text
numlon-windows-x64.exe
numlon.exe
```

Prerelease watching uses the newest non-draft prerelease version. Stable mode uses GitHub's latest stable release endpoint.

## Window behavior

Numlon opens a small native Windows control window. Closing the window hides it to tray. Use **Exit** from the tray menu to stop the app.

Only one Numlon instance can run at a time. Starting a second copy exits immediately.

## Data location

Numlon stores app data next to the executable:

```text
.numlon-data/state.json
```

The data folder can be overridden for development or portable testing:

```powershell
$env:NUMLON_APP_DATA_DIR = "C:\path\to\numlon-data"
```

## Known limitations

Numlon is Windows-only.

Numlon keeps NumLock enabled; it does not remap numpad keys when always-on mode is disabled.

Some remote desktop, virtual machine, BIOS, firmware, or keyboard-driver setups may manage NumLock independently. In those environments, Numlon can only restore the Windows-visible NumLock state.

Self-update replaces the executable in place. The folder containing `numlon.exe` must be writable by the current user.

For stronger release security, published update assets should be code-signed and/or shipped with a detached signature that Numlon verifies before replacement.

## Contributing

Issues and pull requests are welcome. Keep changes small, focused, and easy to review.

## License & Acknowledgments

Copyright (C) 2020–present [Zoltán Rózsa](https://github.com/rozsazoltan)

Numlon uses Rust and Windows APIs to provide a minimal tray-based NumLock guard.

## Development runner

Run the development watcher without `cargo-watch`:

```powershell
scripts\dev.ps1
```

Or:

```cmd
scripts\dev.cmd
```

On Unix-like shells:

```sh
scripts/dev.sh
```

The dev runner builds `numlon`, starts it, watches source files, and restarts the app after changes.

### WSL source and Windows runtime

Recommended Windows desktop development layout:

```text
WSL source:
  /github/rozsazoltan/numlon

Windows mirror:
  D:\github\rozsazoltan\numlon
```

Keep Git operations on the WSL source workspace. Use the Windows mirror only for native Windows build and runtime testing. Do not build or run the Windows app directly from `\\wsl$`.

Create the Mutagen sync session once from Windows PowerShell. Run the script from the WSL UNC path so the WSL project becomes the source:

```powershell
powershell -ExecutionPolicy Bypass -File "\\wsl$\Ubuntu\github\rozsazoltan\numlon\scripts\setup-mutagen-wsl-dev.ps1"
```

Replace `Ubuntu` with your WSL distribution name, for example `CentOS-Stream-9`.

If your distro name is different, list it with:

```powershell
wsl -l -v
```

Then run daily Windows dev from the native mirror:

```powershell
cd D:\github\rozsazoltan\numlon
scripts\dev-win.ps1
```

The setup script defaults to `one-way-replica`, with WSL as the source and Windows as the mirror. Use `two-way-safe` only if you also edit files in the Windows mirror:

```powershell
powershell -ExecutionPolicy Bypass -File "\\wsl$\Ubuntu\github\rozsazoltan\numlon\scripts\setup-mutagen-wsl-dev.ps1" -SyncMode two-way-safe
```

Replace `Ubuntu` with your WSL distribution name, for example `CentOS-Stream-9`.

Useful Mutagen commands:

```powershell
mutagen sync list
mutagen sync monitor numlon-win-dev
mutagen sync flush numlon-win-dev
mutagen sync terminate numlon-win-dev
```

Preferred pattern: create once, flush often, monitor when something looks wrong, terminate only when the session is broken or no longer needed.
