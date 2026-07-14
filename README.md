# numlon

`numlon` is a tiny Windows tray app that keeps NumLock in charge. It can keep NumLock enabled, or keep the NumLock LED off while remapping the numpad navigation keys back to digits.

- [What it does](#what-it-does)
  - [NumLock modes](#numlock-modes)
  - [Shortcut](#shortcut)
  - [Tray mode](#tray-mode)
  - [Startup](#startup)
  - [Updates](#updates)
  - [Design assets](#design-assets)
- [Get started](#get-started)
- [Usage](#usage)
  - [Choose behavior](#choose-behavior)
  - [Change shortcut](#change-shortcut)
  - [Use tray menu](#use-tray-menu)
  - [Enable startup](#enable-startup)
  - [Check for updates](#check-for-updates)
- [Window behavior](#window-behavior)
- [Data location](#data-location)
- [Known limitations](#known-limitations)
- [Contributing](#contributing)
- [Development runner](#development-runner)
  - [WSL source and Windows runtime](#wsl-source-and-windows-runtime)

## What it does

Numlon runs as a small Windows tray utility with two behavior modes.

### NumLock modes

**Keep NumLock on** restores NumLock whenever another app, keyboard, or accidental key press turns it off.

**Keep LED off, type digits** keeps NumLock disabled and remaps numpad navigation events to digit characters:

```text
Insert      -> 0
End         -> 1
Down        -> 2
Page Down   -> 3
Left        -> 4
Clear       -> 5
Right       -> 6
Home        -> 7
Up          -> 8
Page Up     -> 9
Delete      -> .
```

Dedicated navigation-cluster keys keep their normal behavior. Numlon distinguishes them from numpad navigation events through the Windows extended-key flag.

The remap does not store typed content. It only inspects virtual-key metadata needed to recognize numpad navigation events.

### Shortcut

Default global shortcut:

```text
Win+Alt+Home
```

Open Numlon and choose **Change** to record another shortcut. Numlon validates the new global shortcut before saving it.

The shortcut is stored next to the executable in `.numlon-data/config.json`.

### Tray mode

Numlon lives in Windows notification area. Yellow keypad tray icon opens app on left click. Paused state uses same geometry with muted accent. Right click menu provides:

- enabled state
- behavior mode
- shortcut change
- startup state
- release update actions
- quit

### Startup

Startup is optional. On first production launch, Numlon asks whether it should start with Windows.

Before enabling startup, move `numlon.exe` to its final folder. Do not move it afterward. Windows stores exact executable path in current-user startup registry entry.

Startup launches Numlon with `--startup`, keeping main window hidden while tray service starts.

### Updates

Production builds check GitHub releases for `rozsazoltan/numlon`.

Stable releases are default. Prerelease channel can be enabled in app window or tray menu. Automatic production checks run at most once per hour.

Development builds perform no automatic or manual GitHub API update checks.

### Design assets

Primary accent:

```text
#FFB900
```

Executable and tray icon assets:

```text
assets/numlon.svg
assets/numlon-paused.svg
assets/numlon.ico
assets/numlon-paused.ico
```

`build.rs` embeds multi-size `assets/numlon.ico` into Windows executable. The ICO contains exact-size 16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 128, and 256 pixel frames. `assets/numlon.svg` remains vector source.

## Get started

Recommended layout:

```text
numlon/
├─ numlon.exe
└─ .numlon-data/
   └─ config.json
```

Move `numlon.exe` to final folder, run it, choose behavior, shortcut, and startup preference.

## Usage

### Choose behavior

Open Numlon from tray and select:

```text
Keep NumLock on
```

or:

```text
Keep LED off, type digits
```

Main enable switch pauses or resumes selected behavior. Global shortcut toggles same state.

### Change shortcut

1. Open Numlon.
2. Select **Change** beside current shortcut.
3. Press new shortcut.
4. Press `Esc` to cancel.

If Windows rejects shortcut because another app owns it, Numlon restores previous shortcut.

### Use tray menu

Left click yellow tray icon to open Numlon.

Right click for enabled state, behavior, shortcut, startup, updates, and quit.

### Enable startup

Use first-run prompt, app window, or tray menu.

Startup registry location:

```text
HKCU\Software\Microsoft\Windows\CurrentVersion\Run
```

Production startup command points to current executable and adds:

```text
--startup
```

### Check for updates

Production only. Use update card or tray menu.

Preferred Windows release asset names:

```text
numlon-windows-x64.exe
numlon.exe
```

Prerelease mode selects newest non-draft prerelease. Stable mode uses latest stable release.

## Window behavior

Numlon uses native Windows APIs with a compact Fluent-style settings surface, rounded Windows 11 frame, yellow accent, resizable window, vertical overflow scrolling, and embedded multi-size icons.

Development build title includes current package version and `dev`, for example:

```text
Numlon v0.1.0-dev
```

Production title:

```text
Numlon v0.1.0
```

Development builds open main window automatically. Production manual launches open it; startup launches stay in tray.

Closing window hides Numlon to tray. Use tray menu **Quit Numlon** to stop process.

Only one Numlon instance can run. Starting another copy activates existing window and exits new process.

## Data location

Numlon stores app data next to executable:

```text
.numlon-data/config.json
```

Config includes selected mode, enabled state, shortcut, startup UI state, update channel, window position, and status.

Development or portable testing override:

```powershell
$env:NUMLON_APP_DATA_DIR = "C:\path\to\numlon-data"
```

## Known limitations

Numlon is Windows-only.

LED-off digit mode uses `WH_KEYBOARD_LL` only to distinguish numpad navigation events. `SendInput` is subject to Windows integrity boundaries, so remapping may not reach elevated applications when Numlon runs unelevated.

Unicode digit injection works in standard desktop text inputs. Software that reads raw keyboard scan codes may ignore remapped characters.

Some remote desktop, virtual machine, BIOS, firmware, or keyboard-driver setups can manage NumLock independently.

Self-update replaces executable in place. Executable folder must be writable by current user.

Published update assets should be code-signed and/or accompanied by detached signatures verified before replacement.

## Contributing

Issues and pull requests are welcome. Keep changes small, focused, testable, and easy to review.

## License & Acknowledgments

Copyright (C) 2020–present [Zoltán Rózsa](https://github.com/rozsazoltan)

Numlon uses Rust and native Windows APIs to provide minimal tray-based NumLock control.

## Development runner

Run project-local development watcher:

```powershell
scripts\dev.ps1
```

Or:

```cmd
scripts\dev.cmd
```

Unix-like shell:

```sh
scripts/dev.sh
```

Dev runner builds `numlon`, opens app window, watches source and asset files, stops previous child process, and restarts after changes.

Development builds:

- open app window automatically
- include `-dev` in title version
- never call GitHub update API
- cannot enable Windows startup

### WSL source and Windows runtime

Recommended layout:

```text
WSL source:
  /github/rozsazoltan/numlon

Windows mirror:
  D:\github\rozsazoltan\numlon
```

Keep Git operations in WSL source workspace. Use Windows mirror only for native Windows build and runtime testing. Do not build or run app directly from `\\wsl$`.

Create Mutagen session once from Windows PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File "\\wsl$\CentOS-Stream-9\github\rozsazoltan\numlon\scripts\setup-mutagen-wsl-dev.ps1"
```

Daily Windows dev:

```powershell
cd D:\github\rozsazoltan\numlon
scripts\dev-win.ps1
```

Setup defaults to `one-way-replica`, WSL source and Windows mirror. Use `two-way-safe` only when editing Windows mirror too:

```powershell
powershell -ExecutionPolicy Bypass -File "\\wsl$\CentOS-Stream-9\github\rozsazoltan\numlon\scripts\setup-mutagen-wsl-dev.ps1" -SyncMode two-way-safe
```

Useful commands:

```powershell
mutagen sync list
mutagen sync monitor numlon-win-dev
mutagen sync flush numlon-win-dev
mutagen sync terminate numlon-win-dev
```

Preferred pattern: create once, flush often, monitor when something looks wrong, terminate only when session is broken or no longer needed.
