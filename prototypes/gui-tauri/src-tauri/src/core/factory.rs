use super::backend::MoonlightCore;
use super::events::BridgeEvent;
use super::rust_backend::RustBackend;
use crate::logger;
use crate::mock_backend::MockBackend;
use std::sync::mpsc;
use tauri::{Emitter, Manager};

const BACKEND_MODE_ENV: &str = "MOONLIGHT_TAURI_BACKEND";
const BRIDGE_EVENT: &str = "moonlight-bridge-event";

#[derive(Debug, Eq, PartialEq)]
enum BackendSelection {
    Mock,
    Rust,
}

pub fn create_backend(app_handle: tauri::AppHandle) -> Box<dyn MoonlightCore> {
    match select_backend(std::env::var(BACKEND_MODE_ENV).ok().as_deref()) {
        BackendSelection::Rust => create_rust_backend(app_handle),
        BackendSelection::Mock => create_mock_backend(),
    }
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
    let (event_sender, event_receiver) = mpsc::channel::<BridgeEvent>();
    thread_rust_events_to_tauri(app_handle, event_receiver);
    match RustBackend::from_storage_dir_with_event_sender(app_data_dir, event_sender) {
        Ok(backend) => Box::new(backend),
        Err(error) => panic!("failed to initialize Rust backend state: {error}"),
    }
}

fn thread_rust_events_to_tauri(
    app_handle: tauri::AppHandle,
    event_receiver: mpsc::Receiver<BridgeEvent>,
) {
    std::thread::spawn(move || {
        for event in event_receiver {
            let event_log = format!(
                "rust backend event; kind={:?}; host_id={:?}; app_id={:?}; message={}",
                event.kind, event.host_id, event.app_id, event.message
            );
            if event.host_id.is_some() || event.app_id.is_some() {
                logger::stream(event_log);
            } else {
                logger::log(event_log);
            }
            if let Err(error) = app_handle.emit(BRIDGE_EVENT, event) {
                logger::log(format!("failed to emit rust backend event; error={error}"));
            }
        }
    });
}

fn select_backend(mode: Option<&str>) -> BackendSelection {
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
    fn retired_ipc_mode_falls_back_to_rust() {
        let selection = select_backend(Some("ipc"));

        assert_eq!(BackendSelection::Rust, selection);
    }

    #[test]
    fn forced_mock_mode_wins() {
        let selection = select_backend(Some("mock"));

        assert_eq!(BackendSelection::Mock, selection);
    }

    #[test]
    fn rust_backend_is_default() {
        let selection = select_backend(None);

        assert_eq!(BackendSelection::Rust, selection);
    }
}
