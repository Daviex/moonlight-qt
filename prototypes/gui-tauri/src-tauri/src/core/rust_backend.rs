use super::backend::MoonlightCore;
use super::discovery::{discover_nvstream_hosts, merge_discovered_hosts};
use super::error::CoreError;
use super::events::{BridgeEvent, BridgeEventKind};
use super::gamestream::{GameStreamRunner, StreamCallbacks};
use super::host_http::{
    BlockingHostHttpClient, HostEndpoint, ReqwestHostHttpTransport, ServerInfo, StartAppRequest,
};
use super::host_store::{HostStore, StoredHost};
use super::identity::ClientIdentity;
use super::network::{blocked_ports, diagnose_tcp_ports, send_wake_packet, GAMESTREAM_TCP_PORTS};
use super::pairing::{generate_pairing_pin, PairingClient, PairingRequest};
use super::session::SessionMachine;
#[cfg(test)]
use super::settings::default_streaming_settings;
use super::settings::{default_bitrate_kbps, validate_streaming_settings};
#[cfg(test)]
use super::storage::default_app_entries;
use super::storage::{JsonStateStore, StoredState};
use super::stream_input::{
    ButtonAction, ControllerState, KeyAction, KeyModifiers, MouseButton, StreamInputSender,
};
use super::stream_launch::{PreparedStreamSession, StreamLaunchPlan};
use super::types::{
    AppEntry, BackendInfo, CommandStatus, DisplayInfo, HostDetails, HostEntry, HostStatus,
    NetworkTestResult, PairingChallenge, StreamingSettings, SystemInfo,
};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

pub struct RustBackend {
    hosts: HostStore,
    apps: Vec<AppEntry>,
    settings: StreamingSettings,
    session: SessionMachine,
    next_host_number: u32,
    client_identity: Option<ClientIdentity>,
    state_store: Option<JsonStateStore>,
    host_http: Option<BlockingHostHttpClient>,
    active_stream_plan: Option<StreamLaunchPlan>,
    event_sender: Option<Sender<BridgeEvent>>,
    stream_callbacks: Option<StreamCallbacks>,
}

impl RustBackend {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::from_state(sample_state(), None)
    }

    #[cfg(test)]
    pub fn from_storage_dir(app_data_dir: impl Into<PathBuf>) -> Result<Self, String> {
        Self::from_storage_dir_with_event_sender_optional(app_data_dir, None)
    }

    pub fn from_storage_dir_with_event_sender(
        app_data_dir: impl Into<PathBuf>,
        event_sender: Sender<BridgeEvent>,
    ) -> Result<Self, String> {
        Self::from_storage_dir_with_event_sender_optional(app_data_dir, Some(event_sender))
    }

    fn from_storage_dir_with_event_sender_optional(
        app_data_dir: impl Into<PathBuf>,
        event_sender: Option<Sender<BridgeEvent>>,
    ) -> Result<Self, String> {
        let state_store = JsonStateStore::in_app_data_dir(app_data_dir);
        let state = state_store.load().map_err(|error| error.to_string())?;
        let mut backend =
            Self::from_state_with_event_sender(state, Some(state_store), event_sender);
        backend.ensure_client_identity()?;
        Ok(backend)
    }

    #[cfg(test)]
    fn from_state(state: StoredState, state_store: Option<JsonStateStore>) -> Self {
        Self::from_state_with_event_sender(state, state_store, None)
    }

    fn from_state_with_event_sender(
        state: StoredState,
        state_store: Option<JsonStateStore>,
        event_sender: Option<Sender<BridgeEvent>>,
    ) -> Self {
        Self {
            hosts: state.hosts,
            apps: state.apps,
            settings: state.settings,
            session: SessionMachine::default(),
            next_host_number: state.next_host_number,
            client_identity: state.client_identity,
            state_store,
            host_http: BlockingHostHttpClient::connect().ok(),
            active_stream_plan: None,
            event_sender,
            stream_callbacks: None,
        }
    }

    fn state_snapshot(&self) -> StoredState {
        StoredState {
            hosts: self.hosts.clone(),
            apps: self.apps.clone(),
            settings: self.settings.clone(),
            client_identity: self.client_identity.clone(),
            next_host_number: self.next_host_number,
            ..StoredState::default()
        }
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(state_store) = &self.state_store {
            state_store
                .save(&self.state_snapshot())
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn reload_persisted_state(&mut self) -> Result<(), String> {
        let Some(state_store) = &self.state_store else {
            return Ok(());
        };
        let state = state_store.load().map_err(|error| error.to_string())?;
        self.hosts = state.hosts;
        self.apps = state.apps;
        self.settings = state.settings;
        self.client_identity = state.client_identity;
        self.next_host_number = state.next_host_number;
        Ok(())
    }

    fn ensure_client_identity(&mut self) -> Result<(), String> {
        if self.client_identity.is_none() {
            self.client_identity =
                Some(ClientIdentity::generate().map_err(|error| error.to_string())?);
            self.persist()?;
        }
        Ok(())
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

    fn complete_pairing_in_store(
        state_store: JsonStateStore,
        host_id: String,
        server_uuid: String,
        server_certificate_pem: String,
    ) -> Result<(), String> {
        let mut state = state_store.load().map_err(|error| error.to_string())?;
        let Some(mut host) = state
            .hosts
            .hosts()
            .iter()
            .find(|host| host.id == host_id)
            .cloned()
        else {
            return Err(format!("Host '{host_id}' was not found."));
        };
        host.paired = true;
        if !server_uuid.trim().is_empty() {
            host.uuid = server_uuid;
        }
        host.server_certificate_pem = server_certificate_pem;
        state.hosts.add_or_update(host);
        state_store.save(&state).map_err(|error| error.to_string())
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

    fn fetch_server_info(&self, address: &str) -> Result<ServerInfo, String> {
        let Some(client) = &self.host_http else {
            return Err("Host HTTP client is unavailable.".into());
        };
        let endpoint = HostEndpoint::from_address(address).map_err(|error| error.to_string())?;
        client
            .fetch_server_info(&endpoint)
            .map_err(|error| error.to_string())
    }

    fn refresh_apps_from_host(
        &mut self,
        address: &str,
        running_game_id: i32,
    ) -> Result<(), String> {
        let Some(client) = &self.host_http else {
            return Err("Host HTTP client is unavailable.".into());
        };
        let endpoint = HostEndpoint::from_address(address).map_err(|error| error.to_string())?;
        let apps = client
            .fetch_app_list(&endpoint)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|app| app.into_entry(running_game_id, String::new()))
            .collect();
        self.apps = apps;
        self.persist()
    }

    fn prepare_live_stream(
        &self,
        plan: &StreamLaunchPlan,
    ) -> Result<Option<PreparedStreamSession>, String> {
        if self.state_store.is_none() {
            return Ok(None);
        }
        let runner = GameStreamRunner;
        if !runner.is_linked() {
            return Err(
                "C GameStream library is not linked. Set MOONLIGHT_COMMON_C_LIB_DIR to enable streaming."
                    .into(),
            );
        }

        let Some(client) = &self.host_http else {
            return Err("Host HTTP client is unavailable.".into());
        };
        let endpoint =
            HostEndpoint::from_address(&plan.host_address).map_err(|error| error.to_string())?;
        let server_info = client
            .fetch_server_info(&endpoint)
            .map_err(|error| error.to_string())?;
        let app_id = plan
            .app_id
            .parse::<u32>()
            .map_err(|_| format!("App ID '{}' is not numeric.", plan.app_id))?;
        let request = StartAppRequest {
            app_id,
            is_gfe: server_info.state.contains("MJOLNIR"),
            sops: if server_info.state.contains("MJOLNIR") {
                false
            } else {
                self.settings.game_optimizations
            },
            local_audio: self.settings.play_audio_on_host,
            gamepad_mask: 0,
            persist_game_controllers_on_disconnect: !self.settings.multi_controller,
        };
        let start_session = client
            .launch_app(&endpoint, &request, &plan.stream_config)
            .map_err(|error| error.to_string())?;

        plan.prepare_session(&server_info, &start_session)
            .map(Some)
            .map_err(|error| error.to_string())
    }

    fn ensure_stream_input_active(&self) -> Result<(), String> {
        if self.active_stream_plan.is_some() {
            Ok(())
        } else {
            Err("No active Rust stream session is ready for input.".into())
        }
    }
}

#[cfg(test)]
fn sample_state() -> StoredState {
    let mut hosts = HostStore::new();
    hosts.add_or_update(StoredHost {
        id: "gaming-pc".into(),
        name: "Gaming PC".into(),
        manual_address: "192.168.1.20".into(),
        uuid: "rust-gaming-pc".into(),
        paired: true,
        mac_address: "00:11:22:33:44:55".into(),
        server_certificate_pem: "-----BEGIN CERTIFICATE-----\npaired\n-----END CERTIFICATE-----"
            .into(),
    });
    hosts.add_or_update(StoredHost {
        id: "living-room".into(),
        name: "Living Room PC".into(),
        manual_address: "192.168.1.30".into(),
        uuid: "rust-living-room".into(),
        paired: true,
        mac_address: "00:11:22:33:44:66".into(),
        server_certificate_pem: "-----BEGIN CERTIFICATE-----\npaired\n-----END CERTIFICATE-----"
            .into(),
    });
    hosts.add_or_update(StoredHost {
        id: "new-host".into(),
        name: "New Host".into(),
        manual_address: "192.168.1.40".into(),
        uuid: "rust-new-host".into(),
        paired: false,
        mac_address: String::new(),
        server_certificate_pem: String::new(),
    });

    StoredState {
        hosts,
        apps: default_app_entries(),
        settings: default_streaming_settings(),
        next_host_number: 1,
        ..StoredState::default()
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
        self.reload_persisted_state()?;
        if self.settings.enable_mdns {
            match discover_nvstream_hosts(Duration::from_millis(250)) {
                Ok(records) => {
                    if !merge_discovered_hosts(&mut self.hosts, records).is_empty() {
                        self.persist()?;
                    }
                }
                Err(error) => eprintln!("Rust backend mDNS discovery failed: {error}"),
            }
        }
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
            server_certificate_pem: String::new(),
        });
        self.persist()?;

        Ok((
            CommandStatus {
                message: format!("Added host {address}."),
            },
            id,
        ))
    }

    fn pair_host(&mut self, host_id: &str) -> Result<PairingChallenge, String> {
        let mut host = self.stored_host(host_id)?;
        let pin = generate_pairing_pin();

        if self.state_store.is_none() {
            host.paired = true;
            self.update_host(host.clone());
            self.persist()?;
        } else {
            let state_store = self.state_store.clone().unwrap();
            let endpoint = HostEndpoint::from_address(host.manual_address.clone())
                .map_err(|error| error.to_string())?;
            let identity = self
                .client_identity
                .clone()
                .ok_or_else(|| "Client identity is unavailable.".to_string())?;
            let host_id = host.id.clone();
            let host_name = host.name.clone();
            let pin = pin.clone();
            let event_sender = self.event_sender.clone();

            thread::spawn(move || {
                let result = BlockingHostHttpClient::connect()
                    .and_then(|client| client.fetch_unpaired_server_info(&endpoint))
                    .and_then(|server_info| {
                        let request =
                            PairingRequest::new(host_id.clone(), pin, server_info.app_version)?;
                        let completed = ReqwestHostHttpTransport::new()
                            .map(PairingClient::new)
                            .and_then(|client| client.pair(&endpoint, &request, &identity))?;
                        Self::complete_pairing_in_store(
                            state_store,
                            host_id.clone(),
                            server_info.unique_id,
                            completed.server_certificate_pem,
                        )
                        .map_err(CoreError::Backend)
                    })
                    .map_err(|error| error.to_string());
                match result {
                    Ok(()) => {
                        if let Some(event_sender) = event_sender {
                            let _ = event_sender.send(BridgeEvent {
                                kind: BridgeEventKind::HostChanged,
                                message: format!("Pairing completed for {host_name}."),
                                host_id: Some(host_id),
                                app_id: None,
                                controller_action: None,
                                update_version: None,
                                update_url: None,
                            });
                        }
                    }
                    Err(error) => {
                        eprintln!("Rust backend pairing failed: {error}");
                        if let Some(event_sender) = event_sender {
                            let _ = event_sender.send(BridgeEvent {
                                kind: BridgeEventKind::Status,
                                message: format!("Pairing failed for {host_name}: {error}"),
                                host_id: Some(host_id),
                                app_id: None,
                                controller_action: None,
                                update_version: None,
                                update_url: None,
                            });
                        }
                    }
                }
            });
        }

        Ok(PairingChallenge {
            pin: pin.clone(),
            message: format!("Enter PIN {pin} on {} to complete pairing.", host.name),
        })
    }

    fn wake_host(&mut self, host_id: &str) -> Result<CommandStatus, String> {
        let host = self.stored_host(host_id)?;
        if host.mac_address.trim().is_empty() {
            return Err(format!("{} does not have a known MAC address.", host.name));
        }
        send_wake_packet(&host.mac_address).map_err(|error| error.to_string())?;

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
        self.persist()?;

        Ok(CommandStatus {
            message: format!("Renamed host to {name}."),
        })
    }

    fn delete_host(&mut self, host_id: &str) -> Result<CommandStatus, String> {
        self.hosts
            .remove(host_id)
            .map_err(|error| error.to_string())?;
        self.persist()?;
        Ok(CommandStatus {
            message: "Host deleted.".into(),
        })
    }

    fn host_details(&mut self, host_id: &str) -> Result<HostDetails, String> {
        let host = self.stored_host(host_id)?;
        let entry = host.clone().into_entry();
        let live_info = self.fetch_server_info(&host.manual_address).ok();
        let running_game_id = live_info
            .as_ref()
            .map(|info| info.current_game_id)
            .unwrap_or_else(|| {
                self.apps
                    .iter()
                    .find(|app| app.running)
                    .and_then(|app| app.id.parse::<i32>().ok())
                    .unwrap_or(0)
            });
        let app_version = live_info
            .as_ref()
            .map(|info| info.app_version.clone())
            .unwrap_or_else(|| "Rust in-process backend".into());
        let gfe_version = live_info
            .as_ref()
            .map(|info| info.gfe_version.clone())
            .unwrap_or_default();
        let server_version = app_version.clone();
        let pair_state = live_info
            .as_ref()
            .map(|info| info.pair_status.clone())
            .filter(|pair_status| !pair_status.is_empty())
            .unwrap_or_else(|| if host.paired { "Paired" } else { "Unpaired" }.into());

        Ok(HostDetails {
            name: host.name.clone(),
            address: host.manual_address.clone(),
            status: if live_info.is_some() {
                HostStatus::Online
            } else {
                entry.status
            },
            paired: host.paired,
            running: running_game_id != 0 || self.apps.iter().any(|app| app.running),
            wakeable: !host.mac_address.is_empty(),
            server_supported: true,
            uuid: host.uuid,
            local_address: host.manual_address.clone(),
            remote_address: String::new(),
            ipv6_address: String::new(),
            manual_address: host.manual_address.clone(),
            mac_address: host.mac_address,
            pair_state,
            running_game_id,
            https_port: 47984,
            app_version,
            gfe_version,
            server_version,
            gpu_model: "Unknown".into(),
            details: format!(
                "Name: {}\nActive Address: {}",
                host.name, host.manual_address
            ),
        })
    }

    fn test_network(&mut self, host_id: &str) -> Result<NetworkTestResult, String> {
        let host = self.stored_host(host_id)?;
        let diagnostics = diagnose_tcp_ports(&host.manual_address, GAMESTREAM_TCP_PORTS);
        let blocked_ports = blocked_ports(&diagnostics);
        if !blocked_ports.is_empty() {
            return Ok(NetworkTestResult {
                result: "blocked".into(),
                message: format!("{} has unreachable GameStream TCP ports.", host.name),
                blocked_ports,
            });
        }

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

        let stored_host = self.stored_host(host_id)?;
        let running_game_id = self
            .fetch_server_info(&stored_host.manual_address)
            .map(|info| info.current_game_id)
            .unwrap_or(0);
        // Keep the persisted app list when the host is offline or not reachable yet.
        let _ = self.refresh_apps_from_host(&stored_host.manual_address, running_game_id);

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

        let stored_host = self.stored_host(host_id)?;
        let app = self.app_mut(app_id)?.clone();
        let plan = StreamLaunchPlan::new(&stored_host, &app, &self.settings)
            .map_err(|error| error.to_string())?;
        let prepared_stream = self.prepare_live_stream(&plan)?;
        let callbacks = if let Some(mut prepared_stream) = prepared_stream {
            let mut callbacks = self
                .event_sender
                .clone()
                .map(|sender| {
                    StreamCallbacks::connection_lifecycle_with_events(
                        sender,
                        plan.host_id.clone(),
                        plan.app_id.clone(),
                    )
                })
                .unwrap_or_else(StreamCallbacks::connection_lifecycle);
            GameStreamRunner
                .start(&mut prepared_stream.raw, &mut callbacks)
                .map_err(|error| error.to_string())?;
            Some(callbacks)
        } else {
            None
        };

        self.session
            .launch(plan.host_id.clone(), plan.app_id.clone())
            .map_err(|error| error.to_string())?;
        self.session
            .mark_active()
            .map_err(|error| error.to_string())?;

        let app_name = plan.app_name.clone();
        self.app_mut(app_id)?.running = true;
        self.active_stream_plan = Some(plan);
        self.stream_callbacks = callbacks;

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
        let host = self.host_entry(host_id)?;
        if self.state_store.is_some() {
            if let Some(client) = &self.host_http {
                let endpoint =
                    HostEndpoint::from_address(&host.address).map_err(|error| error.to_string())?;
                client
                    .quit_app(&endpoint)
                    .map_err(|error| error.to_string())?;
            }
        }
        for app in &mut self.apps {
            app.running = false;
        }
        GameStreamRunner.stop();
        self.active_stream_plan = None;
        self.stream_callbacks = None;
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
        let app_name = app.name.clone();
        self.persist()?;

        Ok(CommandStatus {
            message: if hidden {
                format!("{app_name} is now hidden.")
            } else {
                format!("{app_name} is now visible.")
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
        let app_name = app.name.clone();
        self.persist()?;

        Ok(CommandStatus {
            message: if direct_launch {
                format!("{app_name} is now the direct-launch app.")
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
        self.persist()?;

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
        default_bitrate_kbps(width, height, fps, yuv444).map_err(|error| error.to_string())
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

    fn active_stream_window(
        &mut self,
    ) -> Result<Option<super::stream_window::StreamWindowDescriptor>, String> {
        Ok(self
            .active_stream_plan
            .as_ref()
            .map(|plan| plan.window.clone()))
    }

    fn stream_mouse_move(&mut self, delta_x: i16, delta_y: i16) -> Result<CommandStatus, String> {
        self.ensure_stream_input_active()?;
        StreamInputSender
            .send_mouse_move(delta_x, delta_y)
            .map_err(|error| error.to_string())?;
        Ok(CommandStatus {
            message: "Mouse movement sent to stream.".into(),
        })
    }

    fn stream_mouse_position(
        &mut self,
        x: i16,
        y: i16,
        reference_width: i16,
        reference_height: i16,
    ) -> Result<CommandStatus, String> {
        self.ensure_stream_input_active()?;
        StreamInputSender
            .send_mouse_position(x, y, reference_width, reference_height)
            .map_err(|error| error.to_string())?;
        Ok(CommandStatus {
            message: "Mouse position sent to stream.".into(),
        })
    }

    fn stream_mouse_button(
        &mut self,
        button: String,
        pressed: bool,
    ) -> Result<CommandStatus, String> {
        self.ensure_stream_input_active()?;
        StreamInputSender
            .send_mouse_button(
                if pressed {
                    ButtonAction::Press
                } else {
                    ButtonAction::Release
                },
                parse_mouse_button(&button)?,
            )
            .map_err(|error| error.to_string())?;
        Ok(CommandStatus {
            message: "Mouse button sent to stream.".into(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn stream_keyboard(
        &mut self,
        key_code: i16,
        pressed: bool,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
        non_normalized: bool,
    ) -> Result<CommandStatus, String> {
        self.ensure_stream_input_active()?;
        StreamInputSender
            .send_keyboard(
                key_code,
                if pressed {
                    KeyAction::Down
                } else {
                    KeyAction::Up
                },
                KeyModifiers {
                    shift,
                    ctrl,
                    alt,
                    meta,
                },
                non_normalized,
            )
            .map_err(|error| error.to_string())?;
        Ok(CommandStatus {
            message: "Keyboard event sent to stream.".into(),
        })
    }

    fn stream_text(&mut self, text: String) -> Result<CommandStatus, String> {
        self.ensure_stream_input_active()?;
        StreamInputSender
            .send_utf8_text(&text)
            .map_err(|error| error.to_string())?;
        Ok(CommandStatus {
            message: "Text input sent to stream.".into(),
        })
    }

    fn stream_scroll(&mut self, delta_x: i16, delta_y: i16) -> Result<CommandStatus, String> {
        self.ensure_stream_input_active()?;
        if delta_y != 0 {
            StreamInputSender
                .send_high_res_scroll(delta_y)
                .map_err(|error| error.to_string())?;
        }
        if delta_x != 0 {
            StreamInputSender
                .send_high_res_horizontal_scroll(delta_x)
                .map_err(|error| error.to_string())?;
        }
        Ok(CommandStatus {
            message: "Scroll input sent to stream.".into(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn stream_controller(
        &mut self,
        controller_number: u8,
        active_gamepad_mask: u16,
        button_flags: i32,
        left_trigger: u8,
        right_trigger: u8,
        left_stick_x: i16,
        left_stick_y: i16,
        right_stick_x: i16,
        right_stick_y: i16,
    ) -> Result<CommandStatus, String> {
        self.ensure_stream_input_active()?;
        StreamInputSender
            .send_controller(ControllerState {
                controller_number,
                active_gamepad_mask,
                button_flags,
                left_trigger,
                right_trigger,
                left_stick_x,
                left_stick_y,
                right_stick_x,
                right_stick_y,
            })
            .map_err(|error| error.to_string())?;
        Ok(CommandStatus {
            message: "Controller input sent to stream.".into(),
        })
    }
}

fn parse_mouse_button(button: &str) -> Result<MouseButton, String> {
    match button {
        "left" => Ok(MouseButton::Left),
        "middle" => Ok(MouseButton::Middle),
        "right" => Ok(MouseButton::Right),
        "x1" => Ok(MouseButton::X1),
        "x2" => Ok(MouseButton::X2),
        _ => Err(format!("Unsupported mouse button '{button}'.")),
    }
}

#[cfg(test)]
mod tests {
    use super::RustBackend;
    use crate::core::backend::MoonlightCore;
    use crate::core::storage::JsonStateStore;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_state_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join("moonlight-tauri-rust-backend-tests")
            .join(format!("{name}-{nonce}"))
    }

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
        assert!(backend.active_stream_plan.is_some());
        let resume = backend.resume_session("gaming-pc").unwrap();
        backend.quit_running_app("gaming-pc").unwrap();

        assert_eq!("Resume requested for Steam Big Picture.", resume.message);
        assert!(backend.active_stream_plan.is_none());
        assert!(backend.resume_session("gaming-pc").is_err());
    }

    #[cfg(not(moonlight_common_c_linked))]
    #[test]
    fn persistent_launch_requires_linked_gamestream_runner() {
        let state_store =
            JsonStateStore::from_file(unique_state_dir("runner-link").join("state.json"));
        let mut backend = RustBackend::from_state(super::sample_state(), Some(state_store));

        let error = backend.launch_app("gaming-pc", "steam").unwrap_err();

        assert_eq!(
            "C GameStream library is not linked. Set MOONLIGHT_COMMON_C_LIB_DIR to enable streaming.",
            error
        );
        assert!(backend.active_stream_plan.is_none());
    }

    #[test]
    fn rust_backend_validates_settings_before_saving() {
        let mut backend = RustBackend::new();
        let mut settings = backend.load_settings().unwrap();
        settings.width = 128;

        let error = backend.save_settings(settings).unwrap_err();

        assert_eq!("Width must be between 256 and 8192.", error);
    }

    #[test]
    fn rust_backend_persists_manual_hosts_between_instances() {
        let state_dir = unique_state_dir("manual-hosts");
        let host_id = {
            let mut backend = RustBackend::from_storage_dir(&state_dir).unwrap();
            let (_, host_id) = backend.add_host("192.168.1.51".into()).unwrap();
            host_id
        };

        let mut reloaded = RustBackend::from_storage_dir(&state_dir).unwrap();
        let hosts = reloaded.list_hosts().unwrap();

        assert!(hosts.iter().any(|host| host.id == host_id && !host.paired));
    }

    #[test]
    fn rust_backend_persists_settings_between_instances() {
        let state_dir = unique_state_dir("settings");
        {
            let mut backend = RustBackend::from_storage_dir(&state_dir).unwrap();
            let mut settings = backend.load_settings().unwrap();
            settings.width = 2560;
            backend.save_settings(settings).unwrap();
        }

        let mut reloaded = RustBackend::from_storage_dir(&state_dir).unwrap();
        let settings = reloaded.load_settings().unwrap();

        assert_eq!(2560, settings.width);
    }

    #[test]
    fn rust_backend_generates_and_persists_identity() {
        let state_dir = unique_state_dir("identity");
        {
            let backend = RustBackend::from_storage_dir(&state_dir).unwrap();
            let identity = backend.client_identity.as_ref().unwrap();

            assert_eq!(16, identity.unique_id.len());
            assert!(identity.certificate_pem.contains("BEGIN CERTIFICATE"));
        }

        let reloaded = RustBackend::from_storage_dir(&state_dir).unwrap();

        assert!(reloaded.client_identity.is_some());
    }

    #[test]
    fn completed_pairing_updates_persisted_host_certificate() {
        let state_dir = unique_state_dir("completed-pairing");
        let state_store = crate::core::storage::JsonStateStore::in_app_data_dir(&state_dir);
        let host_id = {
            let mut backend = RustBackend::from_storage_dir(&state_dir).unwrap();
            let (_, host_id) = backend.add_host("192.168.1.52".into()).unwrap();
            host_id
        };

        RustBackend::complete_pairing_in_store(
            state_store.clone(),
            host_id.clone(),
            "server-uuid".into(),
            "-----BEGIN CERTIFICATE-----\npaired\n-----END CERTIFICATE-----".into(),
        )
        .unwrap();

        let state = state_store.load().unwrap();
        let host = state
            .hosts
            .hosts()
            .iter()
            .find(|host| host.id == host_id)
            .unwrap();

        assert!(host.paired);
        assert_eq!("server-uuid", host.uuid);
        assert!(host.server_certificate_pem.contains("paired"));
    }

    #[test]
    fn list_hosts_reloads_background_pairing_completion() {
        let state_dir = unique_state_dir("reload-completed-pairing");
        let state_store = crate::core::storage::JsonStateStore::in_app_data_dir(&state_dir);
        let (mut backend, host_id) = {
            let mut backend = RustBackend::from_storage_dir(&state_dir).unwrap();
            let (_, host_id) = backend.add_host("192.168.1.52".into()).unwrap();
            (backend, host_id)
        };

        RustBackend::complete_pairing_in_store(
            state_store,
            host_id.clone(),
            "server-uuid".into(),
            "-----BEGIN CERTIFICATE-----\npaired\n-----END CERTIFICATE-----".into(),
        )
        .unwrap();

        let hosts = backend.list_hosts().unwrap();
        let host = hosts.iter().find(|host| host.id == host_id).unwrap();

        assert!(host.paired);
    }
}
