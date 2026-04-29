mod backend;
mod ipc_backend;
mod mock_backend;

use backend::{
    AppEntry, BackendInfo, BridgeEvent, BridgeEventKind, CommandStatus, ControllerAction,
    HostDetails, HostEntry, MoonlightBackend, NetworkTestResult, PairingChallenge,
    StreamingSettings,
};
use ipc_backend::{ipc_backend_requested, IpcBackend};
use mock_backend::MockBackend;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

const BRIDGE_EVENT: &str = "moonlight-bridge-event";

type BackendState = Mutex<Box<dyn MoonlightBackend>>;

fn create_backend(app_handle: tauri::AppHandle) -> Box<dyn MoonlightBackend> {
    if ipc_backend_requested() {
        Box::new(
            IpcBackend::from_environment(app_handle).expect("failed to initialize IPC backend"),
        )
    } else {
        Box::new(MockBackend::new())
    }
}

fn emit_bridge_event(
    app_handle: &tauri::AppHandle,
    kind: BridgeEventKind,
    message: String,
    host_id: Option<String>,
    app_id: Option<String>,
) -> Result<(), String> {
    app_handle
        .emit(
            BRIDGE_EVENT,
            BridgeEvent {
                kind,
                message,
                host_id,
                app_id,
                controller_action: None,
            },
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn backend_info(backend: tauri::State<'_, BackendState>) -> Result<BackendInfo, String> {
    Ok(backend
        .lock()
        .map_err(|error| error.to_string())?
        .backend_info())
}

#[tauri::command]
fn emit_controller_action(
    action: ControllerAction,
    app_handle: tauri::AppHandle,
) -> Result<CommandStatus, String> {
    let message = format!("Controller action: {action:?}");
    app_handle
        .emit(
            BRIDGE_EVENT,
            BridgeEvent {
                kind: BridgeEventKind::ControllerAction,
                message: message.clone(),
                host_id: None,
                app_id: None,
                controller_action: Some(action),
            },
        )
        .map_err(|error| error.to_string())?;
    Ok(CommandStatus { message })
}

#[tauri::command]
fn list_hosts(backend: tauri::State<'_, BackendState>) -> Result<Vec<HostEntry>, String> {
    backend
        .lock()
        .map_err(|error| error.to_string())?
        .list_hosts()
}

#[tauri::command]
fn add_host(
    address: String,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<CommandStatus, String> {
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    let (status, host_id) = backend.add_host(address)?;
    let emit_command_events = !backend.emits_native_events();
    drop(backend);

    if emit_command_events {
        emit_bridge_event(
            &app_handle,
            BridgeEventKind::HostChanged,
            status.message.clone(),
            Some(host_id),
            None,
        )?;
    }
    Ok(status)
}

#[tauri::command]
fn pair_host(
    host_id: String,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<PairingChallenge, String> {
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    let challenge = backend.pair_host(&host_id)?;
    let emit_command_events = !backend.emits_native_events();
    drop(backend);

    if emit_command_events {
        emit_bridge_event(
            &app_handle,
            BridgeEventKind::HostChanged,
            challenge.message.clone(),
            Some(host_id),
            None,
        )?;
    }
    Ok(challenge)
}

#[tauri::command]
fn wake_host(
    host_id: String,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<CommandStatus, String> {
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    let status = backend.wake_host(&host_id)?;
    let emit_command_events = !backend.emits_native_events();
    drop(backend);

    if emit_command_events {
        emit_bridge_event(
            &app_handle,
            BridgeEventKind::HostChanged,
            status.message.clone(),
            Some(host_id),
            None,
        )?;
    }
    Ok(status)
}

#[tauri::command]
fn rename_host(
    host_id: String,
    name: String,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<CommandStatus, String> {
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    let status = backend.rename_host(&host_id, name)?;
    let emit_command_events = !backend.emits_native_events();
    drop(backend);

    if emit_command_events {
        emit_bridge_event(
            &app_handle,
            BridgeEventKind::HostChanged,
            status.message.clone(),
            Some(host_id),
            None,
        )?;
    }
    Ok(status)
}

#[tauri::command]
fn delete_host(
    host_id: String,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<CommandStatus, String> {
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    let status = backend.delete_host(&host_id)?;
    let emit_command_events = !backend.emits_native_events();
    drop(backend);

    if emit_command_events {
        emit_bridge_event(
            &app_handle,
            BridgeEventKind::HostChanged,
            status.message.clone(),
            Some(host_id),
            None,
        )?;
    }
    Ok(status)
}

#[tauri::command]
fn host_details(
    host_id: String,
    backend: tauri::State<'_, BackendState>,
) -> Result<HostDetails, String> {
    backend
        .lock()
        .map_err(|error| error.to_string())?
        .host_details(&host_id)
}

#[tauri::command]
fn test_network(
    host_id: String,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<NetworkTestResult, String> {
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    let result = backend.test_network(&host_id)?;
    let emit_command_events = !backend.emits_native_events();
    drop(backend);

    if emit_command_events {
        emit_bridge_event(
            &app_handle,
            BridgeEventKind::Status,
            result.message.clone(),
            Some(host_id),
            None,
        )?;
    }
    Ok(result)
}

#[tauri::command]
fn list_apps(
    host_id: String,
    show_hidden: bool,
    backend: tauri::State<'_, BackendState>,
) -> Result<Vec<AppEntry>, String> {
    backend
        .lock()
        .map_err(|error| error.to_string())?
        .list_apps(&host_id, show_hidden)
}

#[tauri::command]
fn launch_app(
    host_id: String,
    app_id: String,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<CommandStatus, String> {
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    let status = backend.launch_app(&host_id, &app_id)?;
    let emit_command_events = !backend.emits_native_events();
    drop(backend);

    if emit_command_events {
        emit_bridge_event(
            &app_handle,
            BridgeEventKind::SessionChanged,
            status.message.clone(),
            Some(host_id.clone()),
            Some(app_id.clone()),
        )?;
        emit_bridge_event(
            &app_handle,
            BridgeEventKind::AppChanged,
            status.message.clone(),
            Some(host_id),
            Some(app_id),
        )?;
    }
    Ok(status)
}

#[tauri::command]
fn resume_session(
    host_id: String,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<CommandStatus, String> {
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    let status = backend.resume_session(&host_id)?;
    let emit_command_events = !backend.emits_native_events();
    drop(backend);

    if emit_command_events {
        emit_bridge_event(
            &app_handle,
            BridgeEventKind::SessionChanged,
            status.message.clone(),
            Some(host_id.clone()),
            None,
        )?;
        emit_bridge_event(
            &app_handle,
            BridgeEventKind::HostChanged,
            status.message.clone(),
            Some(host_id),
            None,
        )?;
    }
    Ok(status)
}

#[tauri::command]
fn quit_running_app(
    host_id: String,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<CommandStatus, String> {
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    let status = backend.quit_running_app(&host_id)?;
    let emit_command_events = !backend.emits_native_events();
    drop(backend);

    if emit_command_events {
        emit_bridge_event(
            &app_handle,
            BridgeEventKind::SessionChanged,
            status.message.clone(),
            Some(host_id.clone()),
            None,
        )?;
        emit_bridge_event(
            &app_handle,
            BridgeEventKind::AppChanged,
            status.message.clone(),
            Some(host_id),
            None,
        )?;
    }
    Ok(status)
}

#[tauri::command]
fn set_app_hidden(
    host_id: String,
    app_id: String,
    hidden: bool,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<CommandStatus, String> {
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    let status = backend.set_app_hidden(&host_id, &app_id, hidden)?;
    let emit_command_events = !backend.emits_native_events();
    drop(backend);

    if emit_command_events {
        emit_bridge_event(
            &app_handle,
            BridgeEventKind::AppChanged,
            status.message.clone(),
            Some(host_id),
            Some(app_id),
        )?;
    }
    Ok(status)
}

#[tauri::command]
fn set_app_direct_launch(
    host_id: String,
    app_id: String,
    direct_launch: bool,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<CommandStatus, String> {
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    let status = backend.set_app_direct_launch(&host_id, &app_id, direct_launch)?;
    let emit_command_events = !backend.emits_native_events();
    drop(backend);

    if emit_command_events {
        emit_bridge_event(
            &app_handle,
            BridgeEventKind::AppChanged,
            status.message.clone(),
            Some(host_id),
            Some(app_id),
        )?;
    }
    Ok(status)
}

#[tauri::command]
fn load_settings(backend: tauri::State<'_, BackendState>) -> Result<StreamingSettings, String> {
    backend
        .lock()
        .map_err(|error| error.to_string())?
        .load_settings()
}

#[tauri::command]
fn save_settings(
    settings: StreamingSettings,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<CommandStatus, String> {
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    let status = backend.save_settings(settings)?;
    let emit_command_events = !backend.emits_native_events();
    drop(backend);

    if emit_command_events {
        emit_bridge_event(
            &app_handle,
            BridgeEventKind::SettingsChanged,
            status.message.clone(),
            None,
            None,
        )?;
    }
    Ok(status)
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let app_handle = app.handle().clone();
            app.manage(Mutex::new(create_backend(app_handle)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_hosts,
            backend_info,
            add_host,
            pair_host,
            wake_host,
            rename_host,
            delete_host,
            host_details,
            test_network,
            list_apps,
            launch_app,
            resume_session,
            quit_running_app,
            set_app_hidden,
            set_app_direct_launch,
            load_settings,
            save_settings,
            emit_controller_action
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Moonlight Tauri prototype");
}
