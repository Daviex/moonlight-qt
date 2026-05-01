use super::backend::MoonlightCore;
use super::host_store::{HostStore, StoredHost};
use super::session::SessionMachine;
use super::settings::{default_streaming_settings, validate_streaming_settings};
use super::types::{
    AppEntry, BackendInfo, CommandStatus, DisplayInfo, HostDetails, HostEntry, NetworkTestResult,
    PairingChallenge, StreamingSettings, SystemInfo,
};

pub struct RustBackend {
    hosts: HostStore,
    apps: Vec<AppEntry>,
    settings: StreamingSettings,
    session: SessionMachine,
    next_host_number: u32,
}

impl RustBackend {
    pub fn new() -> Self {
        let mut hosts = HostStore::new();
        hosts.add_or_update(StoredHost {
            id: "gaming-pc".into(),
            name: "Gaming PC".into(),
            manual_address: "192.168.1.20".into(),
            uuid: "rust-gaming-pc".into(),
            paired: true,
            mac_address: "00:11:22:33:44:55".into(),
        });
        hosts.add_or_update(StoredHost {
            id: "living-room".into(),
            name: "Living Room PC".into(),
            manual_address: "192.168.1.30".into(),
            uuid: "rust-living-room".into(),
            paired: true,
            mac_address: "00:11:22:33:44:66".into(),
        });
        hosts.add_or_update(StoredHost {
            id: "new-host".into(),
            name: "New Host".into(),
            manual_address: "192.168.1.40".into(),
            uuid: "rust-new-host".into(),
            paired: false,
            mac_address: String::new(),
        });

        Self {
            hosts,
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
            settings: default_streaming_settings(),
            session: SessionMachine::default(),
            next_host_number: 1,
        }
    }

    fn stored_host(&self, host_id: &str) -> Result<StoredHost, String> {
        self.hosts
            .hosts()
            .iter()
            .find(|host| host.id == host_id)
            .cloned()
            .ok_or_else(|| format!("Host '{host_id}' was not found."))
    }

    fn update_host(&mut self, host: StoredHost) {
        self.hosts.add_or_update(host);
    }

    fn host_entry(&self, host_id: &str) -> Result<HostEntry, String> {
        self.hosts
            .entries()
            .into_iter()
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

impl MoonlightCore for RustBackend {
    fn backend_info(&self) -> BackendInfo {
        BackendInfo {
            mode: "rust".into(),
            helper_path: None,
        }
    }

    fn list_hosts(&mut self) -> Result<Vec<HostEntry>, String> {
        Ok(self.hosts.entries())
    }

    fn add_host(&mut self, address: String) -> Result<(CommandStatus, String), String> {
        if address.trim().is_empty() {
            return Err("Host address is required.".into());
        }

        let id = format!("manual-host-{}", self.next_host_number);
        self.next_host_number += 1;
        self.hosts.add_or_update(StoredHost {
            id: id.clone(),
            name: format!("Host {address}"),
            manual_address: address.clone(),
            uuid: format!("manual-{id}"),
            paired: false,
            mac_address: String::new(),
        });

        Ok((
            CommandStatus {
                message: format!("Added host {address}."),
            },
            id,
        ))
    }

    fn pair_host(&mut self, host_id: &str) -> Result<PairingChallenge, String> {
        let mut host = self.stored_host(host_id)?;
        host.paired = true;
        self.update_host(host.clone());

        Ok(PairingChallenge {
            pin: "1234".into(),
            message: format!("Enter PIN 1234 on {} to complete pairing.", host.name),
        })
    }

    fn wake_host(&mut self, host_id: &str) -> Result<CommandStatus, String> {
        let host = self.stored_host(host_id)?;
        Ok(CommandStatus {
            message: format!("Wake requested for {}.", host.name),
        })
    }

    fn rename_host(&mut self, host_id: &str, name: String) -> Result<CommandStatus, String> {
        if name.trim().is_empty() {
            return Err("Host name is required.".into());
        }

        let mut host = self.stored_host(host_id)?;
        host.name = name.clone();
        self.update_host(host);

        Ok(CommandStatus {
            message: format!("Renamed host to {name}."),
        })
    }

    fn delete_host(&mut self, host_id: &str) -> Result<CommandStatus, String> {
        self.hosts
            .remove(host_id)
            .map_err(|error| error.to_string())?;
        Ok(CommandStatus {
            message: "Host deleted.".into(),
        })
    }

    fn host_details(&mut self, host_id: &str) -> Result<HostDetails, String> {
        let host = self.stored_host(host_id)?;
        let entry = host.clone().into_entry();

        Ok(HostDetails {
            name: host.name.clone(),
            address: host.manual_address.clone(),
            status: entry.status,
            paired: host.paired,
            running: self.apps.iter().any(|app| app.running),
            wakeable: !host.mac_address.is_empty(),
            server_supported: true,
            uuid: host.uuid,
            local_address: host.manual_address.clone(),
            remote_address: String::new(),
            ipv6_address: String::new(),
            manual_address: host.manual_address.clone(),
            mac_address: host.mac_address,
            pair_state: if host.paired { "Paired" } else { "Unpaired" }.into(),
            running_game_id: self
                .apps
                .iter()
                .find(|app| app.running)
                .and_then(|app| app.id.parse::<i32>().ok())
                .unwrap_or(0),
            https_port: 47984,
            app_version: "Rust in-process backend".into(),
            gfe_version: String::new(),
            server_version: "Rust in-process backend".into(),
            gpu_model: "Unknown".into(),
            details: format!(
                "Name: {}\nActive Address: {}",
                host.name, host.manual_address
            ),
        })
    }

    fn test_network(&mut self, host_id: &str) -> Result<NetworkTestResult, String> {
        let host = self.stored_host(host_id)?;
        Ok(NetworkTestResult {
            result: "ok".into(),
            blocked_ports: Vec::new(),
            message: format!("No blocked ports detected for {}.", host.name),
        })
    }

    fn list_apps(&mut self, host_id: &str, show_hidden: bool) -> Result<Vec<AppEntry>, String> {
        let host = self.host_entry(host_id)?;
        if !host.paired {
            return Ok(Vec::new());
        }

        Ok(self
            .apps
            .iter()
            .filter(|app| show_hidden || !app.hidden)
            .cloned()
            .collect())
    }

    fn launch_app(&mut self, host_id: &str, app_id: &str) -> Result<CommandStatus, String> {
        let host = self.host_entry(host_id)?;
        if !host.paired {
            return Err(format!(
                "{} must be paired before launching apps.",
                host.name
            ));
        }

        let app_name = {
            let app = self.app_mut(app_id)?;
            app.running = true;
            app.name.clone()
        };

        self.session
            .launch(host_id.to_string(), app_id.to_string())
            .map_err(|error| error.to_string())?;
        self.session
            .mark_active()
            .map_err(|error| error.to_string())?;

        Ok(CommandStatus {
            message: format!("Launch requested for {app_name}."),
        })
    }

    fn resume_session(&mut self, host_id: &str) -> Result<CommandStatus, String> {
        let host = self.host_entry(host_id)?;
        let Some(app) = self.apps.iter().find(|app| app.running) else {
            return Err(format!("{} has no running session to resume.", host.name));
        };

        Ok(CommandStatus {
            message: format!("Resume requested for {}.", app.name),
        })
    }

    fn quit_running_app(&mut self, host_id: &str) -> Result<CommandStatus, String> {
        self.host_entry(host_id)?;
        for app in &mut self.apps {
            app.running = false;
        }
        self.session.finish();

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
        self.host_entry(host_id)?;
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
        self.host_entry(host_id)?;
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
        validate_streaming_settings(&settings).map_err(|error| error.to_string())?;
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
            version: env!("CARGO_PKG_VERSION").into(),
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
            supports_hdr: false,
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

#[cfg(test)]
mod tests {
    use super::RustBackend;
    use crate::core::backend::MoonlightCore;

    #[test]
    fn rust_backend_reports_in_process_mode() {
        let backend = RustBackend::new();

        let info = backend.backend_info();

        assert_eq!("rust", info.mode);
        assert_eq!(None, info.helper_path);
    }

    #[test]
    fn rust_backend_can_add_pair_and_list_apps_for_manual_host() {
        let mut backend = RustBackend::new();

        let (_, host_id) = backend.add_host("192.168.1.50".into()).unwrap();
        assert!(backend.list_apps(&host_id, false).unwrap().is_empty());

        backend.pair_host(&host_id).unwrap();
        let apps = backend.list_apps(&host_id, false).unwrap();

        assert!(apps.iter().any(|app| app.id == "steam"));
    }

    #[test]
    fn rust_backend_uses_session_machine_for_launch_resume_and_quit() {
        let mut backend = RustBackend::new();

        backend.launch_app("gaming-pc", "steam").unwrap();
        let resume = backend.resume_session("gaming-pc").unwrap();
        backend.quit_running_app("gaming-pc").unwrap();

        assert_eq!("Resume requested for Steam Big Picture.", resume.message);
        assert!(backend.resume_session("gaming-pc").is_err());
    }

    #[test]
    fn rust_backend_validates_settings_before_saving() {
        let mut backend = RustBackend::new();
        let mut settings = backend.load_settings().unwrap();
        settings.width = 128;

        let error = backend.save_settings(settings).unwrap_err();

        assert_eq!("Width must be between 256 and 8192.", error);
    }
}
