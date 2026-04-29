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
    pub wakeable: bool,
    pub server_supported: bool,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostDetails {
    pub name: String,
    pub address: String,
    pub status: HostStatus,
    pub paired: bool,
    pub running: bool,
    pub wakeable: bool,
    pub server_supported: bool,
    pub uuid: String,
    pub local_address: String,
    pub remote_address: String,
    pub ipv6_address: String,
    pub manual_address: String,
    pub mac_address: String,
    pub pair_state: String,
    pub running_game_id: i32,
    pub https_port: i32,
    pub app_version: String,
    pub gfe_version: String,
    pub server_version: String,
    pub gpu_model: String,
    pub details: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub box_art_url: String,
    pub hidden: bool,
    pub direct_launch: bool,
    pub running: bool,
    pub app_collector_game: bool,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamingSettings {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub packet_size: u32,
    pub audio_config: i32,
    pub video_codec_config: i32,
    pub video_decoder_selection: i32,
    pub window_mode: i32,
    pub ui_display_mode: i32,
    pub language: i32,
    pub capture_sys_keys_mode: i32,
    pub unlock_bitrate: bool,
    pub auto_adjust_bitrate: bool,
    pub enable_vsync: bool,
    pub game_optimizations: bool,
    pub play_audio_on_host: bool,
    pub multi_controller: bool,
    pub enable_mdns: bool,
    pub quit_app_after: bool,
    pub absolute_mouse_mode: bool,
    pub absolute_touch_mode: bool,
    pub frame_pacing: bool,
    pub connection_warnings: bool,
    pub configuration_warnings: bool,
    pub rich_presence: bool,
    pub enable_hdr: bool,
    pub gamepad_mouse: bool,
    pub detect_network_blocking: bool,
    pub show_performance_overlay: bool,
    pub swap_mouse_buttons: bool,
    pub mute_on_focus_loss: bool,
    pub background_gamepad: bool,
    pub reverse_scroll_direction: bool,
    pub swap_face_buttons: bool,
    pub keep_awake: bool,
    #[serde(rename = "enableYUV444")]
    pub enable_yuv444: bool,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub native_width: i32,
    pub native_height: i32,
    pub safe_area_width: i32,
    pub safe_area_height: i32,
    pub refresh_rate: i32,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub version: String,
    pub friendly_native_arch_name: String,
    pub is_running_wayland: bool,
    pub is_running_xwayland: bool,
    pub is_wow64: bool,
    pub has_desktop_environment: bool,
    pub has_browser: bool,
    pub has_discord_integration: bool,
    pub uses_material3_theme: bool,
    pub has_hardware_acceleration: bool,
    pub renderer_always_full_screen: bool,
    pub maximum_resolution_width: i32,
    pub maximum_resolution_height: i32,
    pub supports_hdr: bool,
    pub unmapped_gamepads: String,
    pub displays: Vec<DisplayInfo>,
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
    pub update_version: Option<String>,
    pub update_url: Option<String>,
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
    UpdateAvailable,
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
    fn default_bitrate(
        &mut self,
        width: u32,
        height: u32,
        fps: u32,
        yuv444: bool,
    ) -> Result<u32, String>;
    fn system_info(&mut self) -> Result<SystemInfo, String>;
    fn open_url(&mut self, url: &str) -> Result<CommandStatus, String>;
}
