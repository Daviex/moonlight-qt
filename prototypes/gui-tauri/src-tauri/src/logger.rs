use std::env;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const DEBUG_ENV: &str = "MOONLIGHT_TAURI_DEBUG";
const LOG_PATH_ENV: &str = "MOONLIGHT_TAURI_LOG";
const STREAM_LOG_PATH_ENV: &str = "MOONLIGHT_TAURI_STREAM_LOG";

static LOG_FILE: OnceLock<Option<Mutex<File>>> = OnceLock::new();
static STREAM_LOG_FILE: OnceLock<Option<Mutex<File>>> = OnceLock::new();

pub fn enabled() -> bool {
    if env::var(LOG_PATH_ENV).is_ok() {
        return true;
    }

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
        log("logging initialized");
        stream("stream logging initialized");
    }
}

pub fn log(message: impl AsRef<str>) {
    write_log(message, log_file);
}

pub fn stream(message: impl AsRef<str>) {
    write_log(message, stream_log_file);
}

fn write_log(message: impl AsRef<str>, file: fn() -> Option<&'static Mutex<File>>) {
    if !enabled() {
        return;
    };

    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let line = format!(
        "[{timestamp_ms}] pid={} {}",
        std::process::id(),
        message.as_ref()
    );

    eprintln!("{line}");

    let Some(file) = file() else {
        return;
    };
    if let Ok(mut file) = file.lock() {
        let _ = writeln!(file, "{line}");
        let _ = file.flush();
    }
}

pub fn log_path() -> Option<PathBuf> {
    default_log_path(LOG_PATH_ENV, "MoonlightTauri.log")
}

pub fn stream_log_path() -> Option<PathBuf> {
    default_log_path(STREAM_LOG_PATH_ENV, "MoonlightTauriStream.log")
}

fn log_file() -> Option<&'static Mutex<File>> {
    log_file_for(&LOG_FILE, log_path)
}

fn stream_log_file() -> Option<&'static Mutex<File>> {
    log_file_for(&STREAM_LOG_FILE, stream_log_path)
}

fn default_log_path(env_name: &str, file_name: &str) -> Option<PathBuf> {
    if !enabled() {
        return None;
    }

    if let Ok(path) = env::var(env_name) {
        return Some(PathBuf::from(path));
    }

    env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join(file_name)))
        .or_else(|| Some(env::temp_dir().join(file_name)))
}

fn log_file_for(
    slot: &'static OnceLock<Option<Mutex<File>>>,
    path: fn() -> Option<PathBuf>,
) -> Option<&'static Mutex<File>> {
    slot.get_or_init(|| {
        if !enabled() {
            return None;
        }

        let path = path()?;
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
