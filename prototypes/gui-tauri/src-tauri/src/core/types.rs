use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub native_width: i32,
    pub native_height: i32,
    pub safe_area_width: i32,
    pub safe_area_height: i32,
    pub refresh_rate: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandStatus {
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendInfo {
    pub mode: String,
    pub helper_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkTestResult {
    pub result: String,
    pub blocked_ports: Vec<String>,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingChallenge {
    pub pin: String,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum HostStatus {
    Online,
    Offline,
    #[serde(rename = "Pairing required")]
    PairingRequired,
}

#[cfg(test)]
mod tests {
    use super::{HostEntry, HostStatus, StreamingSettings};

    #[test]
    fn host_entry_uses_existing_camel_case_json() {
        let host = HostEntry {
            id: "gaming-pc".into(),
            name: "Gaming PC".into(),
            address: "192.168.1.20".into(),
            status: HostStatus::Online,
            paired: true,
            running: false,
            wakeable: true,
            server_supported: true,
        };

        let value = serde_json::to_value(host).unwrap();

        assert_eq!(value["serverSupported"], true);
        assert_eq!(value["status"], "Online");
    }

    #[test]
    fn pairing_required_status_keeps_existing_label() {
        let json = serde_json::to_string(&HostStatus::PairingRequired).unwrap();

        assert_eq!(r#""Pairing required""#, json);
    }

    #[test]
    fn yuv444_setting_keeps_existing_json_field_name() {
        let settings = StreamingSettings {
            width: 1920,
            height: 1080,
            fps: 60,
            bitrate_kbps: 20_000,
            packet_size: 0,
            audio_config: 0,
            video_codec_config: 0,
            video_decoder_selection: 0,
            window_mode: 1,
            ui_display_mode: 0,
            language: 0,
            capture_sys_keys_mode: 1,
            unlock_bitrate: false,
            auto_adjust_bitrate: false,
            enable_vsync: true,
            game_optimizations: false,
            play_audio_on_host: false,
            multi_controller: true,
            enable_mdns: true,
            quit_app_after: false,
            absolute_mouse_mode: false,
            absolute_touch_mode: true,
            frame_pacing: false,
            connection_warnings: true,
            configuration_warnings: true,
            rich_presence: true,
            enable_hdr: false,
            gamepad_mouse: true,
            detect_network_blocking: true,
            show_performance_overlay: false,
            swap_mouse_buttons: false,
            mute_on_focus_loss: false,
            background_gamepad: false,
            reverse_scroll_direction: false,
            swap_face_buttons: false,
            keep_awake: true,
            enable_yuv444: false,
        };

        let value = serde_json::to_value(settings).unwrap();

        assert!(value.get("enableYUV444").is_some());
        assert!(value.get("enableYuv444").is_none());
    }
}
