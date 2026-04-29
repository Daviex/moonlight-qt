use crate::backend::{
    AppEntry, BackendInfo, CommandStatus, DisplayInfo, HostDetails, HostEntry, HostStatus,
    MoonlightBackend, NetworkTestResult, PairingChallenge, StreamingSettings, SystemInfo,
};

pub struct MockBackend {
    hosts: Vec<HostEntry>,
    apps: Vec<AppEntry>,
    settings: StreamingSettings,
    next_host_number: u32,
}

impl MockBackend {
    pub fn new() -> Self {
        Self {
            hosts: vec![
                HostEntry {
                    id: "gaming-pc".into(),
                    name: "Gaming PC".into(),
                    address: "192.168.1.20".into(),
                    status: HostStatus::Online,
                    paired: true,
                    running: false,
                    wakeable: true,
                    server_supported: true,
                },
                HostEntry {
                    id: "living-room".into(),
                    name: "Living Room PC".into(),
                    address: "192.168.1.30".into(),
                    status: HostStatus::Offline,
                    paired: true,
                    running: false,
                    wakeable: true,
                    server_supported: true,
                },
                HostEntry {
                    id: "new-host".into(),
                    name: "New Host".into(),
                    address: "192.168.1.40".into(),
                    status: HostStatus::PairingRequired,
                    paired: false,
                    running: false,
                    wakeable: false,
                    server_supported: true,
                },
            ],
            apps: vec![
                AppEntry {
                    id: "steam".into(),
                    name: "Steam Big Picture".into(),
                    box_art_url: String::new(),
                    hidden: false,
                    direct_launch: true,
                    running: false,
                    app_collector_game: false,
                },
                AppEntry {
                    id: "desktop".into(),
                    name: "Desktop".into(),
                    box_art_url: String::new(),
                    hidden: false,
                    direct_launch: false,
                    running: false,
                    app_collector_game: false,
                },
                AppEntry {
                    id: "game".into(),
                    name: "Example Game".into(),
                    box_art_url: String::new(),
                    hidden: false,
                    direct_launch: false,
                    running: false,
                    app_collector_game: true,
                },
            ],
            settings: StreamingSettings {
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

impl MoonlightBackend for MockBackend {
    fn backend_info(&self) -> BackendInfo {
        BackendInfo {
            mode: "mock".into(),
            helper_path: None,
        }
    }

    fn list_hosts(&mut self) -> Result<Vec<HostEntry>, String> {
        Ok(self.hosts.clone())
    }

    fn add_host(&mut self, address: String) -> Result<(CommandStatus, String), String> {
        let id = format!("manual-host-{}", self.next_host_number);
        self.next_host_number += 1;
        self.hosts.push(HostEntry {
            id: id.clone(),
            name: format!("Host {address}"),
            address: address.clone(),
            status: HostStatus::PairingRequired,
            paired: false,
            running: false,
            wakeable: false,
            server_supported: true,
        });
        Ok((
            CommandStatus {
                message: format!("Added host {address}."),
            },
            id,
        ))
    }

    fn pair_host(&mut self, host_id: &str) -> Result<PairingChallenge, String> {
        let host = self.host_mut(host_id)?;
        host.paired = true;
        host.status = HostStatus::Online;
        Ok(PairingChallenge {
            pin: "1234".into(),
            message: format!("Enter PIN 1234 on {} to complete pairing.", host.name),
        })
    }

    fn wake_host(&mut self, host_id: &str) -> Result<CommandStatus, String> {
        let host = self.host_mut(host_id)?;
        host.status = HostStatus::Online;
        Ok(CommandStatus {
            message: format!("Wake requested for {}.", host.name),
        })
    }

    fn rename_host(&mut self, host_id: &str, name: String) -> Result<CommandStatus, String> {
        let host = self.host_mut(host_id)?;
        host.name = name.clone();
        Ok(CommandStatus {
            message: format!("Renamed host to {name}."),
        })
    }

    fn delete_host(&mut self, host_id: &str) -> Result<CommandStatus, String> {
        let before = self.hosts.len();
        self.hosts.retain(|host| host.id != host_id);
        if self.hosts.len() == before {
            return Err(format!("Host '{host_id}' was not found."));
        }
        Ok(CommandStatus {
            message: "Host deleted.".into(),
        })
    }

    fn host_details(&mut self, host_id: &str) -> Result<HostDetails, String> {
        let host = self.host(host_id)?;
        Ok(HostDetails {
            name: host.name.clone(),
            address: host.address.clone(),
            status: host.status.clone(),
            paired: host.paired,
            running: host.running,
            wakeable: true,
            server_supported: true,
            uuid: format!("mock-{}", host.id),
            local_address: host.address.clone(),
            remote_address: String::new(),
            ipv6_address: String::new(),
            manual_address: host.address.clone(),
            mac_address: "00:11:22:33:44:55".into(),
            pair_state: if host.paired { "Paired" } else { "Unpaired" }.into(),
            running_game_id: if host.running { 1 } else { 0 },
            https_port: 47984,
            app_version: "Mock Sunshine 0.0".into(),
            gfe_version: String::new(),
            server_version: "Mock Sunshine 0.0".into(),
            gpu_model: "Mock GPU".into(),
            details: format!("Name: {}\nActive Address: {}", host.name, host.address),
        })
    }

    fn test_network(&mut self, host_id: &str) -> Result<NetworkTestResult, String> {
        let host = self.host(host_id)?;
        Ok(NetworkTestResult {
            result: "ok".into(),
            blocked_ports: Vec::new(),
            message: format!("No blocked ports detected for {}.", host.name),
        })
    }

    fn list_apps(&mut self, host_id: &str, show_hidden: bool) -> Result<Vec<AppEntry>, String> {
        self.host(host_id)?;
        Ok(self
            .apps
            .iter()
            .filter(|app| show_hidden || !app.hidden)
            .cloned()
            .collect())
    }

    fn launch_app(&mut self, host_id: &str, app_id: &str) -> Result<CommandStatus, String> {
        let app_name = {
            let app = self.app_mut(app_id)?;
            app.running = true;
            app.name.clone()
        };
        let host = self.host_mut(host_id)?;
        host.running = true;
        Ok(CommandStatus {
            message: format!("Launch requested for {app_name}."),
        })
    }

    fn resume_session(&mut self, host_id: &str) -> Result<CommandStatus, String> {
        let host = self.host(host_id)?;
        if !host.running {
            return Err(format!("{} has no running session to resume.", host.name));
        }

        let app_name = self
            .apps
            .iter()
            .find(|app| app.running)
            .map(|app| app.name.clone())
            .unwrap_or_else(|| "the running app".into());
        Ok(CommandStatus {
            message: format!("Resume requested for {app_name}."),
        })
    }

    fn quit_running_app(&mut self, host_id: &str) -> Result<CommandStatus, String> {
        self.host_mut(host_id)?.running = false;
        for app in &mut self.apps {
            app.running = false;
        }
        Ok(CommandStatus {
            message: "Quit requested for the running app.".into(),
        })
    }

    fn set_app_hidden(
        &mut self,
        host_id: &str,
        app_id: &str,
        hidden: bool,
    ) -> Result<CommandStatus, String> {
        self.host(host_id)?;
        let app = self.app_mut(app_id)?;
        app.hidden = hidden;
        Ok(CommandStatus {
            message: if hidden {
                format!("{} is now hidden.", app.name)
            } else {
                format!("{} is now visible.", app.name)
            },
        })
    }

    fn set_app_direct_launch(
        &mut self,
        host_id: &str,
        app_id: &str,
        direct_launch: bool,
    ) -> Result<CommandStatus, String> {
        self.host(host_id)?;
        for app in &mut self.apps {
            app.direct_launch = false;
        }
        let app = self.app_mut(app_id)?;
        app.direct_launch = direct_launch;
        Ok(CommandStatus {
            message: if direct_launch {
                format!("{} is now the direct-launch app.", app.name)
            } else {
                "Direct launch disabled.".into()
            },
        })
    }

    fn load_settings(&mut self) -> Result<StreamingSettings, String> {
        Ok(self.settings.clone())
    }

    fn save_settings(&mut self, settings: StreamingSettings) -> Result<CommandStatus, String> {
        self.settings = settings;
        Ok(CommandStatus {
            message: "Settings saved.".into(),
        })
    }

    fn default_bitrate(
        &mut self,
        width: u32,
        height: u32,
        fps: u32,
        yuv444: bool,
    ) -> Result<u32, String> {
        if width == 0 || height == 0 || fps == 0 {
            return Err("Width, height, and FPS must be greater than zero.".into());
        }

        let pixels_per_second = width.saturating_mul(height).saturating_mul(fps);
        let yuv_multiplier = if yuv444 { 3 } else { 2 };
        Ok((pixels_per_second / 7_500)
            .saturating_mul(yuv_multiplier)
            .max(5_000))
    }

    fn system_info(&mut self) -> Result<SystemInfo, String> {
        Ok(SystemInfo {
            version: "Mock 0.0".into(),
            friendly_native_arch_name: std::env::consts::ARCH.into(),
            is_running_wayland: false,
            is_running_xwayland: false,
            is_wow64: false,
            has_desktop_environment: true,
            has_browser: true,
            has_discord_integration: false,
            uses_material3_theme: false,
            has_hardware_acceleration: true,
            renderer_always_full_screen: false,
            maximum_resolution_width: 3840,
            maximum_resolution_height: 2160,
            supports_hdr: true,
            unmapped_gamepads: String::new(),
            displays: vec![DisplayInfo {
                native_width: 1920,
                native_height: 1080,
                safe_area_width: 1920,
                safe_area_height: 1080,
                refresh_rate: 60,
            }],
        })
    }

    fn open_url(&mut self, url: &str) -> Result<CommandStatus, String> {
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err("Only HTTP and HTTPS URLs can be opened from the Tauri bridge.".into());
        }

        Ok(CommandStatus {
            message: format!("Open URL requested: {url}"),
        })
    }
}
