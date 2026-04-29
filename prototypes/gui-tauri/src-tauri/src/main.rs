use serde::Serialize;

#[derive(Serialize)]
struct HostEntry {
    id: &'static str,
    name: &'static str,
    status: &'static str,
    paired: bool,
    running: bool,
}

#[derive(Serialize)]
struct AppEntry {
    id: &'static str,
    name: &'static str,
    hidden: bool,
    direct_launch: bool,
    running: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamingSettings {
    width: u32,
    height: u32,
    fps: u32,
    bitrate_kbps: u32,
    enable_hdr: bool,
    gamepad_mouse: bool,
}

#[tauri::command]
fn list_hosts() -> Vec<HostEntry> {
    vec![
        HostEntry {
            id: "gaming-pc",
            name: "Gaming PC",
            status: "Online",
            paired: true,
            running: false,
        },
        HostEntry {
            id: "living-room",
            name: "Living Room PC",
            status: "Offline",
            paired: true,
            running: false,
        },
        HostEntry {
            id: "new-host",
            name: "New Host",
            status: "Pairing required",
            paired: false,
            running: false,
        },
    ]
}

#[tauri::command]
fn list_apps(_host_id: String) -> Vec<AppEntry> {
    vec![
        AppEntry {
            id: "steam",
            name: "Steam Big Picture",
            hidden: false,
            direct_launch: true,
            running: false,
        },
        AppEntry {
            id: "desktop",
            name: "Desktop",
            hidden: false,
            direct_launch: false,
            running: false,
        },
        AppEntry {
            id: "game",
            name: "Example Game",
            hidden: false,
            direct_launch: false,
            running: false,
        },
    ]
}

#[tauri::command]
fn load_settings() -> StreamingSettings {
    StreamingSettings {
        width: 1920,
        height: 1080,
        fps: 60,
        bitrate_kbps: 20_000,
        enable_hdr: false,
        gamepad_mouse: true,
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![list_hosts, list_apps, load_settings])
        .run(tauri::generate_context!())
        .expect("failed to run Moonlight Tauri prototype");
}
