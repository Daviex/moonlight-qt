use super::backend::MoonlightCore;
use crate::ipc_backend::IpcBackend;
use crate::logger;
use crate::mock_backend::MockBackend;

const BACKEND_MODE_ENV: &str = "MOONLIGHT_TAURI_BACKEND";

#[derive(Debug, Eq, PartialEq)]
enum BackendSelection {
    ForcedIpc,
    Mock,
    StagedIpc(String),
}

pub fn create_backend(app_handle: tauri::AppHandle) -> Box<dyn MoonlightCore> {
    match select_backend(
        std::env::var(BACKEND_MODE_ENV).ok().as_deref(),
        IpcBackend::staged_helper_path(),
    ) {
        BackendSelection::ForcedIpc => create_forced_ipc_backend(app_handle),
        BackendSelection::StagedIpc(helper_path) => {
            create_staged_ipc_backend(helper_path, app_handle)
        }
        BackendSelection::Mock => create_mock_backend(),
    }
}

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

fn create_staged_ipc_backend(
    helper_path: String,
    app_handle: tauri::AppHandle,
) -> Box<dyn MoonlightCore> {
    logger::log(format!(
        "creating IPC backend from staged helper; helper_path={helper_path}"
    ));
    match IpcBackend::from_helper_path(helper_path, app_handle) {
        Ok(backend) => {
            logger::log("staged IPC backend ready");
            Box::new(backend)
        }
        Err(error) => {
            logger::log(format!(
                "staged IPC backend initialization failed; error={error}"
            ));
            panic!("failed to initialize staged IPC backend: {error}");
        }
    }
}

fn create_mock_backend() -> Box<dyn MoonlightCore> {
    logger::log("creating mock backend");
    Box::new(MockBackend::new())
}

fn select_backend(mode: Option<&str>, staged_helper_path: Option<String>) -> BackendSelection {
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

    staged_helper_path
        .map(BackendSelection::StagedIpc)
        .unwrap_or(BackendSelection::Mock)
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
    fn staged_helper_is_used_by_default_when_present() {
        let selection = select_backend(None, Some("native/Moonlight.exe".into()));

        assert_eq!(
            BackendSelection::StagedIpc("native/Moonlight.exe".into()),
            selection
        );
    }

    #[test]
    fn mock_backend_is_default_without_staged_helper() {
        let selection = select_backend(None, None);

        assert_eq!(BackendSelection::Mock, selection);
    }
}
