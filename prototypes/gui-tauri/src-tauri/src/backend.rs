use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostEntry {
    pub id: String,
    pub name: String,
    pub address: String,
    pub status: HostStatus,
    pub paired: bool,
    pub running: bool,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostDetails {
    pub name: String,
    pub address: String,
    pub status: HostStatus,
    pub paired: bool,
    pub running: bool,
    pub server_version: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub hidden: bool,
    pub direct_launch: bool,
    pub running: bool,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamingSettings {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub enable_hdr: bool,
    pub gamepad_mouse: bool,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandStatus {
    pub message: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendInfo {
    pub mode: String,
    pub helper_path: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkTestResult {
    pub result: String,
    pub blocked_ports: Vec<String>,
    pub message: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingChallenge {
    pub pin: String,
    pub message: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeEvent {
    pub kind: BridgeEventKind,
    pub message: String,
    pub host_id: Option<String>,
    pub app_id: Option<String>,
    pub controller_action: Option<ControllerAction>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BridgeEventKind {
    HostChanged,
    AppChanged,
    SessionChanged,
    SettingsChanged,
    Status,
    ControllerAction,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ControllerAction {
    Up,
    Down,
    Left,
    Right,
    Accept,
    Back,
    ContextMenu,
    Settings,
    NextControl,
    PreviousControl,
    ActivateControl,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum HostStatus {
    Online,
    Offline,
    #[serde(rename = "Pairing required")]
    PairingRequired,
}

pub trait MoonlightBackend: Send {
    fn backend_info(&self) -> BackendInfo;

    fn emits_native_events(&self) -> bool {
        false
    }

    fn list_hosts(&mut self) -> Result<Vec<HostEntry>, String>;
    fn add_host(&mut self, address: String) -> Result<(CommandStatus, String), String>;
    fn pair_host(&mut self, host_id: &str) -> Result<PairingChallenge, String>;
    fn wake_host(&mut self, host_id: &str) -> Result<CommandStatus, String>;
    fn rename_host(&mut self, host_id: &str, name: String) -> Result<CommandStatus, String>;
    fn delete_host(&mut self, host_id: &str) -> Result<CommandStatus, String>;
    fn host_details(&mut self, host_id: &str) -> Result<HostDetails, String>;
    fn test_network(&mut self, host_id: &str) -> Result<NetworkTestResult, String>;
    fn list_apps(&mut self, host_id: &str, show_hidden: bool) -> Result<Vec<AppEntry>, String>;
    fn launch_app(&mut self, host_id: &str, app_id: &str) -> Result<CommandStatus, String>;
    fn resume_session(&mut self, host_id: &str) -> Result<CommandStatus, String>;
    fn quit_running_app(&mut self, host_id: &str) -> Result<CommandStatus, String>;
    fn set_app_hidden(
        &mut self,
        host_id: &str,
        app_id: &str,
        hidden: bool,
    ) -> Result<CommandStatus, String>;
    fn set_app_direct_launch(
        &mut self,
        host_id: &str,
        app_id: &str,
        direct_launch: bool,
    ) -> Result<CommandStatus, String>;
    fn load_settings(&mut self) -> Result<StreamingSettings, String>;
    fn save_settings(&mut self, settings: StreamingSettings) -> Result<CommandStatus, String>;
}
