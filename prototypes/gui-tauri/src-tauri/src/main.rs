mod backend;
mod core;
mod ipc_backend;
mod logger;
mod mock_backend;

use backend::{
    AppEntry, BackendInfo, BridgeEvent, BridgeEventKind, CommandStatus, ControllerAction,
    HostDetails, HostEntry, MoonlightBackend, NetworkTestResult, PairingChallenge,
    StreamingSettings, SystemInfo,
};
use core::factory::create_backend;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

const BRIDGE_EVENT: &str = "moonlight-bridge-event";

type BackendState = Mutex<Box<dyn MoonlightBackend>>;

fn emit_bridge_event(
    app_handle: &tauri::AppHandle,
    kind: BridgeEventKind,
    message: String,
    host_id: Option<String>,
    app_id: Option<String>,
) -> Result<(), String> {
    logger::log(format!(
        "emit bridge event; kind={kind:?}; message={message}; host_id={host_id:?}; app_id={app_id:?}"
    ));
    app_handle
        .emit(
            BRIDGE_EVENT,
            BridgeEvent {
                kind,
                message,
                host_id,
                app_id,
                controller_action: None,
                update_version: None,
                update_url: None,
            },
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn backend_info(backend: tauri::State<'_, BackendState>) -> Result<BackendInfo, String> {
    logger::log("command backend_info begin");
    let result = backend
        .lock()
        .map_err(|error| error.to_string())?
        .backend_info();
    logger::log(format!(
        "command backend_info complete; mode={}; helper_path={:?}",
        result.mode, result.helper_path
    ));
    Ok(result)
}

#[tauri::command]
fn debug_log(message: String) -> CommandStatus {
    logger::log(format!("frontend: {message}"));
    CommandStatus {
        message: "Debug log recorded.".into(),
    }
}

#[tauri::command]
fn emit_controller_action(
    action: ControllerAction,
    app_handle: tauri::AppHandle,
) -> Result<CommandStatus, String> {
    logger::log(format!(
        "command emit_controller_action begin; action={action:?}"
    ));
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
                update_version: None,
                update_url: None,
            },
        )
        .map_err(|error| error.to_string())?;
    Ok(CommandStatus { message })
}

#[tauri::command]
fn list_hosts(backend: tauri::State<'_, BackendState>) -> Result<Vec<HostEntry>, String> {
    logger::log("command list_hosts begin");
    let result = backend
        .lock()
        .map_err(|error| error.to_string())?
        .list_hosts();
    match &result {
        Ok(hosts) => logger::log(format!(
            "command list_hosts complete; count={}",
            hosts.len()
        )),
        Err(error) => logger::log(format!("command list_hosts failed; error={error}")),
    }
    result
}

#[tauri::command]
fn add_host(
    address: String,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<CommandStatus, String> {
    logger::log(format!("command add_host begin; address={address}"));
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
    logger::log(format!(
        "command add_host complete; message={}",
        status.message
    ));
    Ok(status)
}

#[tauri::command]
fn pair_host(
    host_id: String,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<PairingChallenge, String> {
    logger::log(format!("command pair_host begin; host_id={host_id}"));
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    let challenge = backend.pair_host(&host_id)?;
    let emit_command_events = !backend.emits_native_events();
    drop(backend);

    if emit_command_events {
        emit_bridge_event(
            &app_handle,
            BridgeEventKind::HostChanged,
            challenge.message.clone(),
            Some(host_id.clone()),
            None,
        )?;
    }
    logger::log(format!(
        "command pair_host complete; host_id={host_id}; message={}",
        challenge.message
    ));
    Ok(challenge)
}

#[tauri::command]
fn wake_host(
    host_id: String,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<CommandStatus, String> {
    logger::log(format!("command wake_host begin; host_id={host_id}"));
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
    logger::log(format!(
        "command wake_host complete; message={}",
        status.message
    ));
    Ok(status)
}

#[tauri::command]
fn rename_host(
    host_id: String,
    name: String,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<CommandStatus, String> {
    logger::log(format!(
        "command rename_host begin; host_id={host_id}; name={name}"
    ));
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
    logger::log(format!(
        "command rename_host complete; message={}",
        status.message
    ));
    Ok(status)
}

#[tauri::command]
fn delete_host(
    host_id: String,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<CommandStatus, String> {
    logger::log(format!("command delete_host begin; host_id={host_id}"));
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
    logger::log(format!(
        "command delete_host complete; message={}",
        status.message
    ));
    Ok(status)
}

#[tauri::command]
fn host_details(
    host_id: String,
    backend: tauri::State<'_, BackendState>,
) -> Result<HostDetails, String> {
    logger::log(format!("command host_details begin; host_id={host_id}"));
    let result = backend
        .lock()
        .map_err(|error| error.to_string())?
        .host_details(&host_id);
    match &result {
        Ok(details) => logger::log(format!(
            "command host_details complete; host={}; status={:?}",
            details.name, details.status
        )),
        Err(error) => logger::log(format!("command host_details failed; error={error}")),
    }
    result
}

#[tauri::command]
fn test_network(
    host_id: String,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<NetworkTestResult, String> {
    logger::log(format!("command test_network begin; host_id={host_id}"));
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
    logger::log(format!(
        "command test_network complete; result={}; message={}",
        result.result, result.message
    ));
    Ok(result)
}

#[tauri::command]
fn list_apps(
    host_id: String,
    show_hidden: bool,
    backend: tauri::State<'_, BackendState>,
) -> Result<Vec<AppEntry>, String> {
    logger::log(format!(
        "command list_apps begin; host_id={host_id}; show_hidden={show_hidden}"
    ));
    let result = backend
        .lock()
        .map_err(|error| error.to_string())?
        .list_apps(&host_id, show_hidden);
    match &result {
        Ok(apps) => logger::log(format!("command list_apps complete; count={}", apps.len())),
        Err(error) => logger::log(format!("command list_apps failed; error={error}")),
    }
    result
}

#[tauri::command]
fn launch_app(
    host_id: String,
    app_id: String,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<CommandStatus, String> {
    logger::log(format!(
        "command launch_app begin; host_id={host_id}; app_id={app_id}"
    ));
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
    logger::log(format!(
        "command launch_app complete; message={}",
        status.message
    ));
    Ok(status)
}

#[tauri::command]
fn resume_session(
    host_id: String,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<CommandStatus, String> {
    logger::log(format!("command resume_session begin; host_id={host_id}"));
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
    logger::log(format!(
        "command resume_session complete; message={}",
        status.message
    ));
    Ok(status)
}

#[tauri::command]
fn quit_running_app(
    host_id: String,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<CommandStatus, String> {
    logger::log(format!("command quit_running_app begin; host_id={host_id}"));
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
    logger::log(format!(
        "command quit_running_app complete; message={}",
        status.message
    ));
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
    logger::log(format!(
        "command set_app_hidden begin; host_id={host_id}; app_id={app_id}; hidden={hidden}"
    ));
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
    logger::log(format!(
        "command set_app_hidden complete; message={}",
        status.message
    ));
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
    logger::log(format!(
        "command set_app_direct_launch begin; host_id={host_id}; app_id={app_id}; direct_launch={direct_launch}"
    ));
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
    logger::log(format!(
        "command set_app_direct_launch complete; message={}",
        status.message
    ));
    Ok(status)
}

#[tauri::command]
fn load_settings(backend: tauri::State<'_, BackendState>) -> Result<StreamingSettings, String> {
    logger::log("command load_settings begin");
    let result = backend
        .lock()
        .map_err(|error| error.to_string())?
        .load_settings();
    match &result {
        Ok(settings) => logger::log(format!(
            "command load_settings complete; {}x{} {}fps bitrate={} hdr={} gamepad_mouse={}",
            settings.width,
            settings.height,
            settings.fps,
            settings.bitrate_kbps,
            settings.enable_hdr,
            settings.gamepad_mouse
        )),
        Err(error) => logger::log(format!("command load_settings failed; error={error}")),
    }
    result
}

#[tauri::command]
fn default_bitrate(
    width: u32,
    height: u32,
    fps: u32,
    yuv444: bool,
    backend: tauri::State<'_, BackendState>,
) -> Result<u32, String> {
    logger::log(format!(
        "command default_bitrate begin; width={width}; height={height}; fps={fps}; yuv444={yuv444}"
    ));
    let result = backend
        .lock()
        .map_err(|error| error.to_string())?
        .default_bitrate(width, height, fps, yuv444);
    match &result {
        Ok(bitrate) => logger::log(format!(
            "command default_bitrate complete; bitrate={bitrate}"
        )),
        Err(error) => logger::log(format!("command default_bitrate failed; error={error}")),
    }
    result
}

#[tauri::command]
fn system_info(backend: tauri::State<'_, BackendState>) -> Result<SystemInfo, String> {
    logger::log("command system_info begin");
    let result = backend
        .lock()
        .map_err(|error| error.to_string())?
        .system_info();
    match &result {
        Ok(info) => logger::log(format!(
            "command system_info complete; version={}; arch={}; displays={}; hdr={}; hardware_accel={}; unmapped_gamepads={}",
            info.version,
            info.friendly_native_arch_name,
            info.displays.len(),
            info.supports_hdr,
            info.has_hardware_acceleration,
            !info.unmapped_gamepads.is_empty()
        )),
        Err(error) => logger::log(format!("command system_info failed; error={error}")),
    }
    result
}

#[tauri::command]
fn open_url(url: String, backend: tauri::State<'_, BackendState>) -> Result<CommandStatus, String> {
    logger::log(format!("command open_url begin; url={url}"));
    let result = backend
        .lock()
        .map_err(|error| error.to_string())?
        .open_url(&url);
    match &result {
        Ok(status) => logger::log(format!(
            "command open_url complete; message={}",
            status.message
        )),
        Err(error) => logger::log(format!("command open_url failed; error={error}")),
    }
    result
}

#[tauri::command]
fn save_settings(
    settings: StreamingSettings,
    backend: tauri::State<'_, BackendState>,
    app_handle: tauri::AppHandle,
) -> Result<CommandStatus, String> {
    logger::log(format!(
        "command save_settings begin; {}x{} {}fps bitrate={} hdr={} gamepad_mouse={}",
        settings.width,
        settings.height,
        settings.fps,
        settings.bitrate_kbps,
        settings.enable_hdr,
        settings.gamepad_mouse
    ));
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
    logger::log(format!(
        "command save_settings complete; message={}",
        status.message
    ));
    Ok(status)
}

fn main() {
    logger::init();
    logger::log(format!(
        "starting Moonlight Tauri prototype; exe={:?}; log_path={:?}",
        std::env::current_exe().ok(),
        logger::log_path()
    ));
    tauri::Builder::default()
        .setup(|app| {
            logger::log("tauri setup begin");
            let app_handle = app.handle().clone();
            app.manage(Mutex::new(create_backend(app_handle)));
            logger::log("tauri setup complete");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_hosts,
            backend_info,
            debug_log,
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
            default_bitrate,
            system_info,
            open_url,
            save_settings,
            emit_controller_action
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Moonlight Tauri prototype");
}
