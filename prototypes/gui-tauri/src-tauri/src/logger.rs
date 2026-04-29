use std::env;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const DEBUG_ENV: &str = "MOONLIGHT_TAURI_DEBUG";
const LOG_PATH_ENV: &str = "MOONLIGHT_TAURI_LOG";

static LOG_FILE: OnceLock<Option<Mutex<File>>> = OnceLock::new();

pub fn enabled() -> bool {
    env::var(DEBUG_ENV)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

pub fn init() {
    if enabled() {
        log("debug logging initialized");
    }
}

pub fn log(message: impl AsRef<str>) {
    let Some(file) = log_file() else {
        return;
    };

    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();

    if let Ok(mut file) = file.lock() {
        let _ = writeln!(
            file,
            "[{timestamp_ms}] pid={} {}",
            std::process::id(),
            message.as_ref()
        );
        let _ = file.flush();
    }
}

pub fn log_path() -> Option<PathBuf> {
    if !enabled() {
        return None;
    }

    if let Ok(path) = env::var(LOG_PATH_ENV) {
        return Some(PathBuf::from(path));
    }

    env::current_exe()
        .ok()
        .and_then(|path| {
            path.parent()
                .map(|parent| parent.join("MoonlightTauri.log"))
        })
        .or_else(|| Some(env::temp_dir().join("MoonlightTauri.log")))
}

fn log_file() -> Option<&'static Mutex<File>> {
    LOG_FILE
        .get_or_init(|| {
            if !enabled() {
                return None;
            }

            let path = log_path()?;
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map(Mutex::new)
                .ok()
        })
        .as_ref()
}
