use super::backend::MoonlightCore;
use super::discovery::{discover_nvstream_hosts, merge_discovered_hosts};
use super::error::CoreError;
use super::events::{BridgeEvent, BridgeEventKind};
use super::gamestream::{GameStreamRunner, StreamCallbacks};
use super::host_http::{
    BlockingHostHttpClient, HostEndpoint, HostRequestAuth, HostRequestContext,
    ReqwestHostHttpTransport, ServerInfo, StartAppRequest,
};
use super::host_store::{HostStore, StoredHost};
use super::identity::ClientIdentity;
use super::network::{blocked_ports, diagnose_tcp_ports, send_wake_packet, GAMESTREAM_TCP_PORTS};
use super::pairing::{generate_pairing_pin, PairingClient, PairingRequest};
use super::session::SessionMachine;
#[cfg(test)]
use super::settings::default_streaming_settings;
use super::settings::{default_bitrate_kbps, validate_streaming_settings};
use super::storage::{JsonStateStore, StoredState};
use super::stream_input::{
    ButtonAction, ControllerState, KeyAction, KeyModifiers, MouseButton, StreamInputSender,
};
use super::stream_launch::{PreparedStreamSession, StreamLaunchPlan};
use super::types::{
    AppEntry, BackendInfo, CommandStatus, DisplayInfo, HostDetails, HostEntry, HostStatus,
    NetworkTestResult, PairingChallenge, StreamingSettings, SystemInfo,
};
use crate::logger;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};
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
    stream_thread: Option<JoinHandle<()>>,
    box_art_dir: Option<PathBuf>,
    pending_box_art_fetches: HashSet<(String, String)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StreamStartRequestKind {
    Launch,
    Resume,
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
        let app_data_dir = app_data_dir.into();
        let state_store = JsonStateStore::in_app_data_dir(&app_data_dir);
        let state = state_store.load().map_err(|error| error.to_string())?;
        let mut backend =
            Self::from_state_with_event_sender(state, Some(state_store), event_sender);
        backend.box_art_dir = Some(app_data_dir.join("boxart"));
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
            stream_thread: None,
            box_art_dir: None,
            pending_box_art_fetches: HashSet::new(),
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

    fn host_request_auth(&self, host: &StoredHost) -> Result<HostRequestAuth, String> {
        if !host.paired {
            return Ok(HostRequestAuth::None);
        }
        let identity = self.client_identity.as_ref().ok_or_else(|| {
            "Client identity is unavailable for paired host requests.".to_string()
        })?;
        Ok(HostRequestAuth::client_identity(
            &identity.unique_id,
            &identity.certificate_pem,
            &identity.private_key_pem,
        ))
    }

    fn host_request_context<'a>(
        &'a self,
        host: &StoredHost,
        address: &str,
    ) -> Result<HostRequestContext<'a, ReqwestHostHttpTransport>, String> {
        let Some(client) = &self.host_http else {
            return Err("Host HTTP client is unavailable.".into());
        };
        let endpoint = HostEndpoint::from_address(address).map_err(|error| error.to_string())?;
        let auth = self.host_request_auth(host)?;
        Ok(client.request_context(endpoint, auth))
    }

    fn fetch_server_info_for_host(
        &self,
        host: &StoredHost,
        address: &str,
    ) -> Result<ServerInfo, String> {
        self.host_request_context(host, address)?
            .fetch_server_info()
            .map_err(|error| error.to_string())
    }

    fn fetch_unpaired_server_info_for_address(&self, address: &str) -> Result<ServerInfo, String> {
        let Some(client) = &self.host_http else {
            return Err("Host HTTP client is unavailable.".into());
        };
        let endpoint = HostEndpoint::from_address(address).map_err(|error| error.to_string())?;
        client
            .fetch_unpaired_server_info(&endpoint)
            .map_err(|error| error.to_string())
    }

    fn host_entries_with_live_status(&mut self) -> Vec<HostEntry> {
        let hosts = self.hosts.hosts().to_vec();
        let mut entries = Vec::with_capacity(hosts.len());
        let mut changed = false;

        for mut host in hosts {
            let live_info = if host.paired {
                self.fetch_server_info_for_host(&host, &host.manual_address)
                    .ok()
            } else {
                self.fetch_unpaired_server_info_for_address(&host.manual_address)
                    .ok()
            };
            if let Some(info) = live_info {
                let host_changed = update_host_from_server_info(&mut host, &info);
                let mut entry = host.clone().into_entry();
                entry.status = if host.paired {
                    HostStatus::Online
                } else {
                    HostStatus::PairingRequired
                };
                entry.running = info.current_game_id != 0;
                entry.wakeable = !host.mac_address.trim().is_empty();
                entries.push(entry);
                if host_changed {
                    self.hosts.add_or_update(host);
                    changed = true;
                }
            } else {
                entries.push(host.clone().into_entry());
            }
        }

        if changed {
            if let Err(error) = self.persist() {
                eprintln!("Rust backend host metadata persistence failed: {error}");
            }
        }

        entries
    }

    fn refresh_apps_from_host(
        &mut self,
        host: &StoredHost,
        address: &str,
        running_game_id: i32,
    ) -> Result<(), String> {
        let previous_apps = self.apps.clone();
        let host_apps = self
            .host_request_context(host, address)?
            .fetch_app_list()
            .map_err(|error| error.to_string())?;
        let apps = host_apps
            .into_iter()
            .map(|app| {
                let box_art_url = self.cached_box_art_url(&host.uuid, &app.id);
                let previous = previous_apps.iter().find(|entry| entry.id == app.id);
                let mut entry = app.into_entry(running_game_id, box_art_url);
                if let Some(previous) = previous {
                    entry.hidden = previous.hidden;
                    entry.direct_launch = previous.direct_launch;
                }
                entry
            })
            .collect::<Vec<_>>();
        self.apps = apps;
        self.queue_missing_box_art_fetches(host, address);
        self.persist()
    }

    fn cached_box_art_url(&self, host_uuid: &str, app_id: &str) -> String {
        let Some(path) = self.box_art_path(host_uuid, app_id) else {
            return String::new();
        };
        match fs::metadata(&path) {
            Ok(metadata) if metadata.len() > 0 => file_url_from_path(&path),
            _ => String::new(),
        }
    }

    fn box_art_path(&self, host_uuid: &str, app_id: &str) -> Option<PathBuf> {
        self.box_art_dir.as_ref().map(|dir| {
            dir.join(safe_cache_key(host_uuid))
                .join(format!("{}.png", safe_cache_key(app_id)))
        })
    }

    fn queue_missing_box_art_fetches(&mut self, host: &StoredHost, address: &str) {
        if self.box_art_dir.is_none() {
            return;
        }
        let Ok(endpoint) = HostEndpoint::from_address(address) else {
            return;
        };

        let host_uuid = host.uuid.clone();
        let host_id = host.id.clone();
        let event_sender = self.event_sender.clone();
        let mut fetches = Vec::new();
        for app in &self.apps {
            if !app.box_art_url.is_empty() {
                continue;
            }
            let Some(path) = self.box_art_path(&host_uuid, &app.id) else {
                continue;
            };
            let fetch_key = (host_uuid.clone(), app.id.clone());
            if !self.pending_box_art_fetches.insert(fetch_key) {
                continue;
            }
            fetches.push((app.id.clone(), app.name.clone(), path));
        }

        if fetches.is_empty() {
            return;
        }

        let auth = match self.host_request_auth(host) {
            Ok(auth) => auth,
            Err(error) => {
                eprintln!("Rust backend box art auth setup failed: {error}");
                return;
            }
        };
        thread::spawn(move || {
            let client = match BlockingHostHttpClient::connect() {
                Ok(client) => client,
                Err(error) => {
                    eprintln!("Rust backend box art client initialization failed: {error}");
                    return;
                }
            };

            for (app_id, app_name, path) in fetches {
                if path.exists() {
                    continue;
                }
                if let Some(parent) = path.parent() {
                    if let Err(error) = fs::create_dir_all(parent) {
                        eprintln!("Rust backend box art cache directory creation failed: {error}");
                        continue;
                    }
                }
                let bytes = client
                    .request_context(endpoint.clone(), auth.clone())
                    .fetch_box_art(&app_id);
                let bytes = match bytes {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        eprintln!("Rust backend box art fetch failed for {app_name}: {error}");
                        continue;
                    }
                };
                if let Err(error) = fs::write(&path, bytes) {
                    eprintln!("Rust backend box art cache write failed for {app_name}: {error}");
                    continue;
                }
                if let Some(event_sender) = &event_sender {
                    let _ = event_sender.send(BridgeEvent {
                        kind: BridgeEventKind::AppChanged,
                        message: format!("Box art loaded for {app_name}."),
                        host_id: Some(host_id.clone()),
                        app_id: Some(app_id),
                        controller_action: None,
                        update_version: None,
                        update_url: None,
                    });
                }
            }
        });
    }

    fn prepare_live_stream(
        &self,
        plan: &StreamLaunchPlan,
        host: &StoredHost,
        request_kind: StreamStartRequestKind,
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

        let request_context = self.host_request_context(host, &plan.host_address)?;
        let server_info = request_context
            .fetch_server_info()
            .map_err(|error| error.to_string())?;
        let stream_config = plan
            .stream_config
            .clone()
            .preferred_for_server(server_info.server_codec_mode_support);
        let app_id = plan
            .app_id
            .parse::<u32>()
            .map_err(|_| format!("App ID '{}' is not numeric.", plan.app_id))?;
        let local_audio =
            self.settings.play_audio_on_host || is_self_stream_address(&plan.host_address);
        if local_audio && !self.settings.play_audio_on_host {
            logger::log(format!(
                "enabling host-local audio for self-stream address {}",
                plan.host_address
            ));
        }
        let request = StartAppRequest {
            app_id,
            is_gfe: server_info.state.contains("MJOLNIR"),
            sops: if server_info.state.contains("MJOLNIR") {
                false
            } else {
                self.settings.game_optimizations
            },
            local_audio,
            gamepad_mask: 0,
            persist_game_controllers_on_disconnect: !self.settings.multi_controller,
        };
        let start_session = match request_kind {
            StreamStartRequestKind::Launch => request_context.launch_app(&request, &stream_config),
            StreamStartRequestKind::Resume => request_context.resume_app(&request, &stream_config),
        }
        .map_err(|error| error.to_string())?;

        plan.prepare_session(&server_info, &start_session, &stream_config)
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

    fn reap_finished_stream_thread(&mut self) {
        let finished = self
            .stream_thread
            .as_ref()
            .map(JoinHandle::is_finished)
            .unwrap_or(false);
        if !finished {
            return;
        }

        if let Some(handle) = self.stream_thread.take() {
            let _ = handle.join();
        }
        self.active_stream_plan = None;
        self.stream_callbacks = None;
        for app in &mut self.apps {
            app.running = false;
        }
        self.session.finish();
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
        apps: sample_app_entries(),
        settings: default_streaming_settings(),
        next_host_number: 1,
        ..StoredState::default()
    }
}

#[cfg(test)]
fn sample_app_entries() -> Vec<AppEntry> {
    vec![
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
    ]
}

impl MoonlightCore for RustBackend {
    fn backend_info(&self) -> BackendInfo {
        BackendInfo {
            mode: "rust".into(),
            helper_path: None,
        }
    }

    fn list_hosts(&mut self) -> Result<Vec<HostEntry>, String> {
        self.reap_finished_stream_thread();
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
        Ok(self.host_entries_with_live_status())
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
        let live_info = self
            .fetch_server_info_for_host(&host, &host.manual_address)
            .ok();
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
        let name = live_info
            .as_ref()
            .and_then(|info| non_empty_string(&info.hostname))
            .unwrap_or_else(|| host.name.clone());
        let uuid = live_info
            .as_ref()
            .and_then(|info| non_empty_string(&info.unique_id))
            .unwrap_or_else(|| host.uuid.clone());
        let local_address = live_info
            .as_ref()
            .and_then(|info| non_empty_string(&info.local_ip))
            .unwrap_or_else(|| host.manual_address.clone());
        let remote_address = live_info
            .as_ref()
            .and_then(|info| non_empty_string(&info.external_ip))
            .unwrap_or_default();
        let mac_address = live_info
            .as_ref()
            .and_then(|info| non_empty_string(&info.mac_address))
            .unwrap_or_else(|| host.mac_address.clone());
        let https_port = live_info
            .as_ref()
            .map(|info| i32::from(info.https_port))
            .unwrap_or(47984);
        let gpu_model = live_info
            .as_ref()
            .and_then(|info| non_empty_string(&info.gpu_model))
            .unwrap_or_else(|| "Unknown".into());
        let details = match live_info.as_ref() {
            Some(info) => format!(
                "Name: {name}\nActive Address: {}\nLocal Address: {local_address}\nRemote Address: {}\nHTTPS Port: {}\nGPU: {gpu_model}\nCodec Support: {}",
                host.manual_address,
                if remote_address.is_empty() { "Unavailable" } else { &remote_address },
                info.https_port,
                info.server_codec_mode_support,
            ),
            None => format!(
                "Name: {name}\nActive Address: {}\nHost is offline or unreachable.",
                host.manual_address
            ),
        };

        Ok(HostDetails {
            name,
            address: host.manual_address.clone(),
            status: if live_info.is_some() {
                HostStatus::Online
            } else {
                entry.status
            },
            paired: host.paired,
            running: running_game_id != 0 || self.apps.iter().any(|app| app.running),
            wakeable: !mac_address.is_empty(),
            server_supported: true,
            uuid,
            local_address,
            remote_address,
            ipv6_address: String::new(),
            manual_address: host.manual_address.clone(),
            mac_address,
            pair_state,
            running_game_id,
            https_port,
            app_version,
            gfe_version,
            server_version,
            gpu_model,
            details,
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
            .fetch_server_info_for_host(&stored_host, &stored_host.manual_address)
            .map(|info| info.current_game_id)
            .unwrap_or(0);
        // Keep a persisted app list when the host is offline, but do not mask a fresh empty
        // production state with the old prototype's sample app list.
        if let Err(error) =
            self.refresh_apps_from_host(&stored_host, &stored_host.manual_address, running_game_id)
        {
            if self.apps.is_empty() {
                return Err(format!(
                    "Unable to refresh app list from {}: {error}",
                    host.name
                ));
            }
            eprintln!(
                "Rust backend app list refresh failed for {}: {error}",
                host.name
            );
        }

        Ok(self
            .apps
            .iter()
            .filter(|app| show_hidden || !app.hidden)
            .cloned()
            .collect())
    }

    fn launch_app(&mut self, host_id: &str, app_id: &str) -> Result<CommandStatus, String> {
        if self.active_stream_plan.is_some() {
            return Err("A Rust stream session is already active.".into());
        }

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
        let prepared_stream =
            self.prepare_live_stream(&plan, &stored_host, StreamStartRequestKind::Launch)?;
        let stream_thread = if let Some(prepared_stream) = prepared_stream {
            Some(start_stream_runner_thread(
                prepared_stream,
                self.event_sender.clone(),
            ))
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
        self.stream_callbacks = None;
        self.stream_thread = stream_thread;

        Ok(CommandStatus {
            message: format!("Launch requested for {app_name}."),
        })
    }

    fn resume_session(&mut self, host_id: &str) -> Result<CommandStatus, String> {
        if self.active_stream_plan.is_some() {
            return Err("A Rust stream session is already active.".into());
        }

        let host = self.host_entry(host_id)?;
        if !host.paired {
            return Err(format!(
                "{} must be paired before resuming sessions.",
                host.name
            ));
        }

        if self.state_store.is_none() {
            let Some(app) = self.apps.iter().find(|app| app.running) else {
                return Err(format!("{} has no running session to resume.", host.name));
            };
            return Ok(CommandStatus {
                message: format!("Resume requested for {}.", app.name),
            });
        }

        let stored_host = self.stored_host(host_id)?;
        let running_game_id = self
            .fetch_server_info_for_host(&stored_host, &stored_host.manual_address)?
            .current_game_id;
        if running_game_id == 0 {
            return Err(format!("{} has no running session to resume.", host.name));
        };

        let _ =
            self.refresh_apps_from_host(&stored_host, &stored_host.manual_address, running_game_id);
        let Some(app) = self
            .apps
            .iter()
            .find(|app| app.id.parse::<i32>().ok() == Some(running_game_id))
            .cloned()
        else {
            return Err(format!(
                "{} is running app ID {running_game_id}, but it was not found in the app list.",
                host.name
            ));
        };

        let plan = StreamLaunchPlan::new(&stored_host, &app, &self.settings)
            .map_err(|error| error.to_string())?;
        let prepared_stream =
            self.prepare_live_stream(&plan, &stored_host, StreamStartRequestKind::Resume)?;
        let stream_thread = if let Some(prepared_stream) = prepared_stream {
            Some(start_stream_runner_thread(
                prepared_stream,
                self.event_sender.clone(),
            ))
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
        self.app_mut(&app.id)?.running = true;
        self.active_stream_plan = Some(plan);
        self.stream_callbacks = None;
        self.stream_thread = stream_thread;

        Ok(CommandStatus {
            message: format!("Resume requested for {app_name}."),
        })
    }

    fn quit_running_app(&mut self, host_id: &str) -> Result<CommandStatus, String> {
        self.host_entry(host_id)?;
        let mut ignored_cancel_error = None;
        if self.state_store.is_some() {
            let stored_host = self.stored_host(host_id)?;
            if let Err(error) = self
                .host_request_context(&stored_host, &stored_host.manual_address)?
                .quit_app()
            {
                let message = error.to_string();
                if is_benign_cancel_shutdown_error(&message) {
                    logger::stream(format!(
                        "quit_running_app ignored cancel failure after stream shutdown; host_id={host_id}; error={message}"
                    ));
                    ignored_cancel_error = Some(message);
                } else {
                    return Err(message);
                }
            }
        }
        for app in &mut self.apps {
            app.running = false;
        }
        GameStreamRunner.stop();
        self.active_stream_plan = None;
        self.stream_callbacks = None;
        self.stream_thread = None;
        self.session.finish();

        Ok(CommandStatus {
            message: if ignored_cancel_error.is_some() {
                "Local stream stopped; host cancel connection was already closed.".into()
            } else {
                "Quit requested for the running app.".into()
            },
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
        let url = validate_external_url(url)?;
        open_url_in_system_browser(url)?;

        Ok(CommandStatus {
            message: format!("Opened URL: {url}"),
        })
    }

    fn active_stream_window(
        &mut self,
    ) -> Result<Option<super::stream_window::StreamWindowDescriptor>, String> {
        self.reap_finished_stream_thread();
        Ok(self
            .active_stream_plan
            .as_ref()
            .map(|plan| plan.window.clone()))
    }

    fn active_stream_session(
        &mut self,
    ) -> Result<Option<super::stream_launch::ActiveStreamSession>, String> {
        self.reap_finished_stream_thread();
        Ok(self
            .active_stream_plan
            .as_ref()
            .map(StreamLaunchPlan::active_session))
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

fn is_benign_cancel_shutdown_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("/cancel")
        && (lower.contains("os error 10054")
            || lower.contains("connection reset")
            || lower.contains("forzatamente")
            || lower.contains("forcibly closed")
            || lower.contains("connection was aborted"))
}

fn is_self_stream_address(address: &str) -> bool {
    let Ok(mut candidates) = (address, 9).to_socket_addrs() else {
        return false;
    };
    candidates.any(|target| is_self_stream_socket_addr(target))
}

fn is_self_stream_socket_addr(target: SocketAddr) -> bool {
    if target.ip().is_loopback() {
        return true;
    }

    let bind_address = match target.ip() {
        IpAddr::V4(_) => "0.0.0.0:0",
        IpAddr::V6(_) => "[::]:0",
    };
    let Ok(socket) = UdpSocket::bind(bind_address) else {
        return false;
    };
    if socket.connect(target).is_err() {
        return false;
    }
    socket
        .local_addr()
        .map(|local| local.ip() == target.ip())
        .unwrap_or(false)
}

fn start_stream_runner_thread(
    mut prepared_stream: PreparedStreamSession,
    event_sender: Option<Sender<BridgeEvent>>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let host_id = prepared_stream.host_id.clone();
        let app_id = prepared_stream.app_id.clone();
        logger::log(format!(
            "Rust GameStream runner thread starting; host_id={host_id}; app_id={app_id}; address={}",
            prepared_stream.server.address
        ));
        let mut callbacks = event_sender
            .clone()
            .map(|sender| {
                StreamCallbacks::connection_lifecycle_with_events(
                    sender,
                    host_id.clone(),
                    app_id.clone(),
                )
            })
            .unwrap_or_else(StreamCallbacks::connection_lifecycle);

        match GameStreamRunner.start(&mut prepared_stream.raw, &mut callbacks) {
            Ok(()) => {
                logger::log(format!(
                    "Rust GameStream runner returned successfully; host_id={host_id}; app_id={app_id}"
                ));
            }
            Err(error) => {
                logger::log(format!(
                    "Rust GameStream runner failed; host_id={host_id}; app_id={app_id}; error={error}"
                ));
                if let Some(sender) = event_sender {
                    let _ = sender.send(BridgeEvent {
                        kind: BridgeEventKind::Status,
                        message: format!("Rust GameStream runner failed: {error}"),
                        host_id: Some(host_id),
                        app_id: Some(app_id),
                        controller_action: None,
                        update_version: None,
                        update_url: None,
                    });
                }
            }
        }
    })
}

fn safe_cache_key(value: &str) -> String {
    let trimmed = value.trim();
    if !trimmed.is_empty()
        && trimmed.len() <= 64
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return trimmed.to_string();
    }

    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn non_empty_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn update_host_from_server_info(host: &mut StoredHost, info: &ServerInfo) -> bool {
    let original = host.clone();
    if let Some(name) = non_empty_string(&info.hostname) {
        host.name = name;
    }
    if let Some(uuid) = non_empty_string(&info.unique_id) {
        host.uuid = uuid;
    }
    if let Some(mac_address) = non_empty_string(&info.mac_address) {
        host.mac_address = mac_address;
    }
    *host != original
}

fn file_url_from_path(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        format!("file:///{}", percent_encode_file_path(&normalized))
    } else {
        format!("file://{}", percent_encode_file_path(&normalized))
    }
}

fn percent_encode_file_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b':' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn validate_external_url(url: &str) -> Result<&str, String> {
    let trimmed = url.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Ok(trimmed);
    }

    Err("Only HTTP and HTTPS URLs can be opened from the Tauri bridge.".into())
}

fn open_url_in_system_browser(url: &str) -> Result<(), String> {
    let (program, args) = system_url_command(url);
    Command::new(program)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Failed to open URL '{url}': {error}"))
}

#[cfg(target_os = "windows")]
fn system_url_command(url: &str) -> (&'static str, Vec<&str>) {
    ("rundll32.exe", vec!["url.dll,FileProtocolHandler", url])
}

#[cfg(target_os = "macos")]
fn system_url_command(url: &str) -> (&'static str, Vec<&str>) {
    ("open", vec![url])
}

#[cfg(all(unix, not(target_os = "macos")))]
fn system_url_command(url: &str) -> (&'static str, Vec<&str>) {
    ("xdg-open", vec![url])
}

#[cfg(test)]
mod tests {
    use super::{is_self_stream_address, system_url_command, validate_external_url, RustBackend};
    use crate::core::backend::MoonlightCore;
    #[cfg(not(moonlight_common_c_linked))]
    use crate::core::storage::JsonStateStore;
    use std::path::PathBuf;
    use std::thread;
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
    fn loopback_address_is_treated_as_self_stream() {
        assert!(is_self_stream_address("127.0.0.1"));
        assert!(is_self_stream_address("localhost"));
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
        backend.quit_running_app("gaming-pc").unwrap();
        backend.app_mut("steam").unwrap().running = true;
        let resume = backend.resume_session("gaming-pc").unwrap();

        assert_eq!("Resume requested for Steam Big Picture.", resume.message);
    }

    #[test]
    fn active_stream_queries_reap_finished_runner_thread() {
        let mut backend = RustBackend::new();
        backend.launch_app("gaming-pc", "steam").unwrap();
        backend.stream_thread = Some(thread::spawn(|| {}));
        while !backend
            .stream_thread
            .as_ref()
            .map(std::thread::JoinHandle::is_finished)
            .unwrap_or(false)
        {
            thread::yield_now();
        }

        let active = backend.active_stream_session().unwrap();

        assert!(active.is_none());
        assert!(backend.active_stream_plan.is_none());
        assert!(backend.apps.iter().all(|app| !app.running));
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
    fn url_opening_accepts_only_http_links() {
        assert_eq!(
            Ok("https://moonlight-stream.org"),
            validate_external_url(" https://moonlight-stream.org ")
        );
        assert!(validate_external_url("file:///tmp/secret").is_err());
        assert!(validate_external_url("javascript:alert(1)").is_err());

        let (_program, args) = system_url_command("https://moonlight-stream.org");
        assert!(args.contains(&"https://moonlight-stream.org"));
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
