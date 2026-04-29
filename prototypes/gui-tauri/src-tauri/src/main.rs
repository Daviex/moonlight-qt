use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HostEntry {
    id: String,
    name: String,
    address: String,
    status: HostStatus,
    paired: bool,
    running: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HostDetails {
    name: String,
    address: String,
    status: HostStatus,
    paired: bool,
    running: bool,
    server_version: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppEntry {
    id: String,
    name: String,
    hidden: bool,
    direct_launch: bool,
    running: bool,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamingSettings {
    width: u32,
    height: u32,
    fps: u32,
    bitrate_kbps: u32,
    enable_hdr: bool,
    gamepad_mouse: bool,
}

#[derive(Clone, Serialize)]
struct CommandStatus {
    message: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NetworkTestResult {
    result: String,
    blocked_ports: Vec<String>,
    message: String,
}

#[derive(Clone, Serialize)]
struct PairingChallenge {
    pin: String,
    message: String,
}

#[derive(Clone, Serialize)]
enum HostStatus {
    Online,
    Offline,
    #[serde(rename = "Pairing required")]
    PairingRequired,
}

struct MockBackend {
    hosts: Vec<HostEntry>,
    apps: Vec<AppEntry>,
    settings: StreamingSettings,
    next_host_number: u32,
}

impl MockBackend {
    fn new() -> Self {
        Self {
            hosts: vec![
                HostEntry {
                    id: "gaming-pc".into(),
                    name: "Gaming PC".into(),
                    address: "192.168.1.20".into(),
                    status: HostStatus::Online,
                    paired: true,
                    running: false,
                },
                HostEntry {
                    id: "living-room".into(),
                    name: "Living Room PC".into(),
                    address: "192.168.1.30".into(),
                    status: HostStatus::Offline,
                    paired: true,
                    running: false,
                },
                HostEntry {
                    id: "new-host".into(),
                    name: "New Host".into(),
                    address: "192.168.1.40".into(),
                    status: HostStatus::PairingRequired,
                    paired: false,
                    running: false,
                },
            ],
            apps: vec![
                AppEntry {
                    id: "steam".into(),
                    name: "Steam Big Picture".into(),
                    hidden: false,
                    direct_launch: true,
                    running: false,
                },
                AppEntry {
                    id: "desktop".into(),
                    name: "Desktop".into(),
                    hidden: false,
                    direct_launch: false,
                    running: false,
                },
                AppEntry {
                    id: "game".into(),
                    name: "Example Game".into(),
                    hidden: false,
                    direct_launch: false,
                    running: false,
                },
            ],
            settings: StreamingSettings {
                width: 1920,
                height: 1080,
                fps: 60,
                bitrate_kbps: 20_000,
                enable_hdr: false,
                gamepad_mouse: true,
            },
            next_host_number: 1,
        }
    }

    fn host_mut(&mut self, host_id: &str) -> Result<&mut HostEntry, String> {
        self.hosts
            .iter_mut()
            .find(|host| host.id == host_id)
            .ok_or_else(|| format!("Host '{host_id}' was not found."))
    }

    fn host(&self, host_id: &str) -> Result<&HostEntry, String> {
        self.hosts
            .iter()
            .find(|host| host.id == host_id)
            .ok_or_else(|| format!("Host '{host_id}' was not found."))
    }

    fn app_mut(&mut self, app_id: &str) -> Result<&mut AppEntry, String> {
        self.apps
            .iter_mut()
            .find(|app| app.id == app_id)
            .ok_or_else(|| format!("App '{app_id}' was not found."))
    }
}

type BackendState = Mutex<MockBackend>;

#[tauri::command]
fn list_hosts(backend: tauri::State<'_, BackendState>) -> Result<Vec<HostEntry>, String> {
    Ok(backend.lock().map_err(|error| error.to_string())?.hosts.clone())
}

#[tauri::command]
fn add_host(address: String, backend: tauri::State<'_, BackendState>) -> Result<CommandStatus, String> {
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    let id = format!("manual-host-{}", backend.next_host_number);
    backend.next_host_number += 1;
    backend.hosts.push(HostEntry {
        id,
        name: format!("Host {address}"),
        address: address.clone(),
        status: HostStatus::PairingRequired,
        paired: false,
        running: false,
    });
    Ok(CommandStatus {
        message: format!("Added host {address}."),
    })
}

#[tauri::command]
fn pair_host(host_id: String, backend: tauri::State<'_, BackendState>) -> Result<PairingChallenge, String> {
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    let host = backend.host_mut(&host_id)?;
    host.paired = true;
    host.status = HostStatus::Online;
    Ok(PairingChallenge {
        pin: "1234".into(),
        message: format!("Enter PIN 1234 on {} to complete pairing.", host.name),
    })
}

#[tauri::command]
fn wake_host(host_id: String, backend: tauri::State<'_, BackendState>) -> Result<CommandStatus, String> {
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    let host = backend.host_mut(&host_id)?;
    host.status = HostStatus::Online;
    Ok(CommandStatus {
        message: format!("Wake requested for {}.", host.name),
    })
}

#[tauri::command]
fn rename_host(host_id: String, name: String, backend: tauri::State<'_, BackendState>) -> Result<CommandStatus, String> {
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    let host = backend.host_mut(&host_id)?;
    host.name = name.clone();
    Ok(CommandStatus {
        message: format!("Renamed host to {name}."),
    })
}

#[tauri::command]
fn delete_host(host_id: String, backend: tauri::State<'_, BackendState>) -> Result<CommandStatus, String> {
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    let before = backend.hosts.len();
    backend.hosts.retain(|host| host.id != host_id);
    if backend.hosts.len() == before {
        return Err(format!("Host '{host_id}' was not found."));
    }
    Ok(CommandStatus {
        message: "Host deleted.".into(),
    })
}

#[tauri::command]
fn host_details(host_id: String, backend: tauri::State<'_, BackendState>) -> Result<HostDetails, String> {
    let backend = backend.lock().map_err(|error| error.to_string())?;
    let host = backend.host(&host_id)?;
    Ok(HostDetails {
        name: host.name.clone(),
        address: host.address.clone(),
        status: host.status.clone(),
        paired: host.paired,
        running: host.running,
        server_version: "Mock Sunshine 0.0".into(),
    })
}

#[tauri::command]
fn test_network(host_id: String, backend: tauri::State<'_, BackendState>) -> Result<NetworkTestResult, String> {
    let backend = backend.lock().map_err(|error| error.to_string())?;
    let host = backend.host(&host_id)?;
    Ok(NetworkTestResult {
        result: "ok".into(),
        blocked_ports: Vec::new(),
        message: format!("No blocked ports detected for {}.", host.name),
    })
}

#[tauri::command]
fn list_apps(host_id: String, show_hidden: bool, backend: tauri::State<'_, BackendState>) -> Result<Vec<AppEntry>, String> {
    let backend = backend.lock().map_err(|error| error.to_string())?;
    backend.host(&host_id)?;
    Ok(backend
        .apps
        .iter()
        .filter(|app| show_hidden || !app.hidden)
        .cloned()
        .collect())
}

#[tauri::command]
fn launch_app(host_id: String, app_id: String, backend: tauri::State<'_, BackendState>) -> Result<CommandStatus, String> {
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    let app_name = {
        let app = backend.app_mut(&app_id)?;
        app.running = true;
        app.name.clone()
    };
    let host = backend.host_mut(&host_id)?;
    host.running = true;
    Ok(CommandStatus {
        message: format!("Launch requested for {app_name}."),
    })
}

#[tauri::command]
fn quit_running_app(host_id: String, backend: tauri::State<'_, BackendState>) -> Result<CommandStatus, String> {
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    backend.host_mut(&host_id)?.running = false;
    for app in &mut backend.apps {
        app.running = false;
    }
    Ok(CommandStatus {
        message: "Quit requested for the running app.".into(),
    })
}

#[tauri::command]
fn set_app_hidden(host_id: String, app_id: String, hidden: bool, backend: tauri::State<'_, BackendState>) -> Result<CommandStatus, String> {
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    backend.host(&host_id)?;
    let app = backend.app_mut(&app_id)?;
    app.hidden = hidden;
    Ok(CommandStatus {
        message: if hidden {
            format!("{} is now hidden.", app.name)
        } else {
            format!("{} is now visible.", app.name)
        },
    })
}

#[tauri::command]
fn set_app_direct_launch(host_id: String, app_id: String, direct_launch: bool, backend: tauri::State<'_, BackendState>) -> Result<CommandStatus, String> {
    let mut backend = backend.lock().map_err(|error| error.to_string())?;
    backend.host(&host_id)?;
    for app in &mut backend.apps {
        app.direct_launch = false;
    }
    let app = backend.app_mut(&app_id)?;
    app.direct_launch = direct_launch;
    Ok(CommandStatus {
        message: if direct_launch {
            format!("{} is now the direct-launch app.", app.name)
        } else {
            "Direct launch disabled.".into()
        },
    })
}

#[tauri::command]
fn load_settings(backend: tauri::State<'_, BackendState>) -> Result<StreamingSettings, String> {
    Ok(backend.lock().map_err(|error| error.to_string())?.settings.clone())
}

#[tauri::command]
fn save_settings(settings: StreamingSettings, backend: tauri::State<'_, BackendState>) -> Result<CommandStatus, String> {
    backend.lock().map_err(|error| error.to_string())?.settings = settings;
    Ok(CommandStatus {
        message: "Settings saved.".into(),
    })
}

fn main() {
    tauri::Builder::default()
        .manage(Mutex::new(MockBackend::new()))
        .invoke_handler(tauri::generate_handler![
            list_hosts,
            add_host,
            pair_host,
            wake_host,
            rename_host,
            delete_host,
            host_details,
            test_network,
            list_apps,
            launch_app,
            quit_running_app,
            set_app_hidden,
            set_app_direct_launch,
            load_settings,
            save_settings
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Moonlight Tauri prototype");
}
