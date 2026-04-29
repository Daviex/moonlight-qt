use crate::backend::{
    AppEntry, CommandStatus, HostDetails, HostEntry, HostStatus, MoonlightBackend,
    NetworkTestResult, PairingChallenge, StreamingSettings,
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

impl MoonlightBackend for MockBackend {
    fn list_hosts(&self) -> Result<Vec<HostEntry>, String> {
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

    fn host_details(&self, host_id: &str) -> Result<HostDetails, String> {
        let host = self.host(host_id)?;
        Ok(HostDetails {
            name: host.name.clone(),
            address: host.address.clone(),
            status: host.status.clone(),
            paired: host.paired,
            running: host.running,
            server_version: "Mock Sunshine 0.0".into(),
        })
    }

    fn test_network(&self, host_id: &str) -> Result<NetworkTestResult, String> {
        let host = self.host(host_id)?;
        Ok(NetworkTestResult {
            result: "ok".into(),
            blocked_ports: Vec::new(),
            message: format!("No blocked ports detected for {}.", host.name),
        })
    }

    fn list_apps(&self, host_id: &str, show_hidden: bool) -> Result<Vec<AppEntry>, String> {
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

    fn load_settings(&self) -> Result<StreamingSettings, String> {
        Ok(self.settings.clone())
    }

    fn save_settings(&mut self, settings: StreamingSettings) -> Result<CommandStatus, String> {
        self.settings = settings;
        Ok(CommandStatus {
            message: "Settings saved.".into(),
        })
    }
}
