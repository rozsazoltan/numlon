use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    env,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const DATA_ENV_VAR: &str = "NUMLON_APP_DATA_DIR";
const DATA_DIR_NAME: &str = ".numlon-data";
const STATE_FILE_NAME: &str = "state.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedState {
    #[serde(default = "default_true")]
    pub always_enabled: bool,
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

pub fn app_version_label() -> String {
    if cfg!(debug_assertions) {
        env::var("NUMLON_DEV_VERSION").unwrap_or_else(|_| "dev".to_owned())
    } else {
        format!("v{}", env!("CARGO_PKG_VERSION"))
    }
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
    Ok(app_data_dir()?.join(STATE_FILE_NAME))
}

pub fn load_state() -> SavedState {
    match state_path().and_then(|path| read_state_from_path(&path)) {
        Ok(state) => state,
        Err(_) => SavedState::default(),
    }
}

fn read_state_from_path(path: &Path) -> Result<SavedState> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read state file: {}", path.display()))?;
    serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse state file: {}", path.display()))
}

pub fn save_state(state: &SavedState) -> Result<()> {
    let path = state_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create data folder: {}", parent.display()))?;
    }

    let temp = path.with_extension("json.tmp");
    let mut file = fs::File::create(&temp)
        .with_context(|| format!("failed to create temporary state file: {}", temp.display()))?;
    file.write_all(serde_json::to_string_pretty(state)?.as_bytes())
        .with_context(|| format!("failed to write temporary state file: {}", temp.display()))?;
    file.sync_all().ok();
    fs::rename(&temp, &path).with_context(|| {
        format!(
            "failed to replace state file: {} -> {}",
            temp.display(),
            path.display()
        )
    })?;
    Ok(())
}

pub fn seconds_since_unix_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
