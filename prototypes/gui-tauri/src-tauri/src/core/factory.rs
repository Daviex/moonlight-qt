use super::backend::MoonlightCore;
use super::rust_backend::RustBackend;
#[cfg(feature = "legacy-ipc")]
use crate::ipc_backend::IpcBackend;
use crate::logger;
use crate::mock_backend::MockBackend;
use tauri::Manager;

const BACKEND_MODE_ENV: &str = "MOONLIGHT_TAURI_BACKEND";

#[derive(Debug, Eq, PartialEq)]
enum BackendSelection {
    ForcedIpc,
    Mock,
    Rust,
}

pub fn create_backend(app_handle: tauri::AppHandle) -> Box<dyn MoonlightCore> {
    match select_backend(std::env::var(BACKEND_MODE_ENV).ok().as_deref(), None) {
        BackendSelection::ForcedIpc => create_forced_ipc_backend(app_handle),
        BackendSelection::Rust => create_rust_backend(app_handle),
        BackendSelection::Mock => create_mock_backend(),
    }
}

#[cfg(feature = "legacy-ipc")]
fn create_forced_ipc_backend(app_handle: tauri::AppHandle) -> Box<dyn MoonlightCore> {
    logger::log("creating IPC backend");
    match IpcBackend::from_environment(app_handle) {
        Ok(backend) => {
            logger::log("IPC backend ready");
            Box::new(backend)
        }
        Err(error) => {
            logger::log(format!("IPC backend initialization failed; error={error}"));
            panic!("failed to initialize IPC backend: {error}");
        }
    }
}

#[cfg(not(feature = "legacy-ipc"))]
fn create_forced_ipc_backend(_app_handle: tauri::AppHandle) -> Box<dyn MoonlightCore> {
    logger::log("legacy IPC backend requested but legacy-ipc feature is disabled");
    panic!(
        "legacy IPC backend requested, but this binary was built without the legacy-ipc feature"
    );
}

fn create_mock_backend() -> Box<dyn MoonlightCore> {
    logger::log("creating mock backend");
    Box::new(MockBackend::new())
}

fn create_rust_backend(app_handle: tauri::AppHandle) -> Box<dyn MoonlightCore> {
    logger::log("creating in-process Rust backend");
    let app_data_dir = match app_handle.path().app_data_dir() {
        Ok(path) => path,
        Err(error) => panic!("failed to resolve Tauri app data directory: {error}"),
    };
    match RustBackend::from_storage_dir(app_data_dir) {
        Ok(backend) => Box::new(backend),
        Err(error) => panic!("failed to initialize Rust backend state: {error}"),
    }
}

fn select_backend(mode: Option<&str>, staged_helper_path: Option<String>) -> BackendSelection {
    let _ = staged_helper_path;

    if mode
        .map(|value| value.eq_ignore_ascii_case("ipc"))
        .unwrap_or(false)
    {
        return BackendSelection::ForcedIpc;
    }

    if mode
        .map(|value| value.eq_ignore_ascii_case("mock"))
        .unwrap_or(false)
    {
        return BackendSelection::Mock;
    }

    BackendSelection::Rust
}

#[cfg(test)]
mod tests {
    use super::{select_backend, BackendSelection};

    #[test]
    fn forced_ipc_mode_wins_over_staged_helper() {
        let selection = select_backend(Some("ipc"), Some("native/Moonlight.exe".into()));

        assert_eq!(BackendSelection::ForcedIpc, selection);
    }

    #[test]
    fn forced_mock_mode_ignores_staged_helper() {
        let selection = select_backend(Some("mock"), Some("native/Moonlight.exe".into()));

        assert_eq!(BackendSelection::Mock, selection);
    }

    #[test]
    fn rust_backend_is_default_even_with_staged_helper() {
        let selection = select_backend(None, Some("native/Moonlight.exe".into()));

        assert_eq!(BackendSelection::Rust, selection);
    }

    #[test]
    fn rust_backend_is_default_without_staged_helper() {
        let selection = select_backend(None, None);

        assert_eq!(BackendSelection::Rust, selection);
    }
}
