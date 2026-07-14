use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::hotkey::HotkeyBinding;

const DATA_ENV_VAR: &str = "NUMLON_APP_DATA_DIR";
const DATA_DIR_NAME: &str = ".numlon-data";
const CONFIG_FILE_NAME: &str = "config.json";
const LEGACY_STATE_FILE_NAME: &str = "state.json";
const BACKUP_FILE_NAME: &str = "config.json.bak";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NumlockMode {
    #[default]
    ForceOn,
    LedOffDigits,
}

impl NumlockMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::ForceOn => "Keep NumLock on",
            Self::LedOffDigits => "Keep LED off, type digits",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedState {
    #[serde(default = "default_true")]
    pub always_enabled: bool,
    #[serde(default)]
    pub numlock_mode: NumlockMode,
    #[serde(default)]
    pub hotkey: HotkeyBinding,
    #[serde(default)]
    pub startup_enabled: bool,
    #[serde(default)]
    pub startup_prompted: bool,
    #[serde(default)]
    pub include_prereleases: bool,
    #[serde(default)]
    pub last_auto_update_check_unix_seconds: u64,
    #[serde(default)]
    pub last_status: String,
    #[serde(default)]
    pub window_x: i32,
    #[serde(default)]
    pub window_y: i32,
}

impl Default for SavedState {
    fn default() -> Self {
        Self {
            always_enabled: true,
            numlock_mode: NumlockMode::ForceOn,
            hotkey: HotkeyBinding::default(),
            startup_enabled: false,
            startup_prompted: false,
            include_prereleases: false,
            last_auto_update_check_unix_seconds: 0,
            last_status: "Numlon active.".to_owned(),
            window_x: i32::MIN,
            window_y: i32::MIN,
        }
    }
}

fn default_true() -> bool {
    true
}

pub fn app_name() -> &'static str {
    "Numlon"
}

pub fn is_dev_build() -> bool {
    cfg!(debug_assertions)
}

pub fn app_version_label() -> String {
    if is_dev_build() {
        env::var("NUMLON_DEV_VERSION")
            .unwrap_or_else(|_| format!("v{}-dev", env!("CARGO_PKG_VERSION")))
    } else {
        format!("v{}", env!("CARGO_PKG_VERSION"))
    }
}

pub fn window_title() -> String {
    format!("{} {}", app_name(), app_version_label())
}

pub fn app_data_dir() -> Result<PathBuf> {
    if let Some(path) = env::var_os(DATA_ENV_VAR).filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }

    let current_exe = env::current_exe().context("failed to resolve current executable path")?;
    let exe_dir = current_exe
        .parent()
        .context("failed to resolve current executable folder")?;
    Ok(exe_dir.join(DATA_DIR_NAME))
}

pub fn state_path() -> Result<PathBuf> {
    Ok(app_data_dir()?.join(CONFIG_FILE_NAME))
}

pub fn load_state() -> SavedState {
    let data_dir = match app_data_dir() {
        Ok(path) => path,
        Err(_) => return SavedState::default(),
    };

    let candidates = [
        data_dir.join(CONFIG_FILE_NAME),
        data_dir.join(BACKUP_FILE_NAME),
        data_dir.join(LEGACY_STATE_FILE_NAME),
    ];

    for path in candidates {
        if let Ok(state) = read_state_from_path(&path) {
            return state;
        }
    }

    SavedState::default()
}

fn read_state_from_path(path: &Path) -> Result<SavedState> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read config file: {}", path.display()))?;
    serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse config file: {}", path.display()))
}

pub fn save_state(state: &SavedState) -> Result<()> {
    let path = state_path()?;
    let parent = path.parent().context("failed to resolve config folder")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create data folder: {}", parent.display()))?;

    let temp = parent.join("config.json.tmp");
    let backup = parent.join(BACKUP_FILE_NAME);
    let serialized = serde_json::to_vec_pretty(state)?;

    let mut file = fs::File::create(&temp)
        .with_context(|| format!("failed to create temporary config file: {}", temp.display()))?;
    file.write_all(&serialized)
        .with_context(|| format!("failed to write temporary config file: {}", temp.display()))?;
    file.write_all(b"\n")
        .with_context(|| format!("failed to finish temporary config file: {}", temp.display()))?;
    file.sync_all().ok();
    drop(file);

    replace_config_file(&temp, &path, &backup)?;
    Ok(())
}

fn replace_config_file(temp: &Path, path: &Path, backup: &Path) -> Result<()> {
    if !path.exists() {
        return fs::rename(temp, path).with_context(|| {
            format!(
                "failed to move temporary config file: {} -> {}",
                temp.display(),
                path.display()
            )
        });
    }

    if backup.exists() {
        fs::remove_file(backup)
            .with_context(|| format!("failed to remove old config backup: {}", backup.display()))?;
    }

    fs::rename(path, backup).with_context(|| {
        format!(
            "failed to create config backup: {} -> {}",
            path.display(),
            backup.display()
        )
    })?;

    match fs::rename(temp, path) {
        Ok(()) => {
            fs::remove_file(backup).ok();
            Ok(())
        }
        Err(error) => {
            let _ = fs::rename(backup, path);
            Err(error).with_context(|| {
                format!(
                    "failed to replace config file: {} -> {}",
                    temp.display(),
                    path.display()
                )
            })
        }
    }
}

pub fn seconds_since_unix_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
