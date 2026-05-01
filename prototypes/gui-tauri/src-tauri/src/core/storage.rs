#![allow(dead_code)]

use super::error::CoreError;
use super::host_store::HostStore;
use super::identity::ClientIdentity;
use super::settings::default_streaming_settings;
use super::types::{AppEntry, StreamingSettings};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const CURRENT_STATE_VERSION: u32 = 1;
const STATE_FILE_NAME: &str = "rust-backend-state.json";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredState {
    #[serde(default = "current_state_version")]
    pub version: u32,
    #[serde(default)]
    pub hosts: HostStore,
    #[serde(default = "default_app_entries")]
    pub apps: Vec<AppEntry>,
    #[serde(default = "default_streaming_settings")]
    pub settings: StreamingSettings,
    #[serde(default)]
    pub client_identity: Option<ClientIdentity>,
    #[serde(default = "default_next_host_number")]
    pub next_host_number: u32,
}

impl Default for StoredState {
    fn default() -> Self {
        Self {
            version: CURRENT_STATE_VERSION,
            hosts: HostStore::new(),
            apps: default_app_entries(),
            settings: default_streaming_settings(),
            client_identity: None,
            next_host_number: default_next_host_number(),
        }
    }
}

impl StoredState {
    pub fn normalized(mut self) -> Self {
        self.version = CURRENT_STATE_VERSION;
        self.next_host_number = self
            .next_host_number
            .max(next_manual_host_number(&self.hosts));
        self
    }

    pub fn persisted_apps(&self) -> Vec<AppEntry> {
        self.apps
            .iter()
            .cloned()
            .map(|mut app| {
                app.running = false;
                app
            })
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsonStateStore {
    path: PathBuf,
}

impl JsonStateStore {
    pub fn in_app_data_dir(app_data_dir: impl Into<PathBuf>) -> Self {
        Self {
            path: app_data_dir.into().join(STATE_FILE_NAME),
        }
    }

    pub fn from_file(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<StoredState, CoreError> {
        if !self.path.exists() {
            return Ok(StoredState::default());
        }

        let contents = fs::read_to_string(&self.path).map_err(|error| {
            CoreError::Backend(format!(
                "Unable to read Rust backend state at {}: {error}",
                self.path.display()
            ))
        })?;
        let state = serde_json::from_str::<StoredState>(&contents).map_err(|error| {
            CoreError::Backend(format!(
                "Unable to parse Rust backend state at {}: {error}",
                self.path.display()
            ))
        })?;

        Ok(state.normalized())
    }

    pub fn save(&self, state: &StoredState) -> Result<(), CoreError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                CoreError::Backend(format!(
                    "Unable to create Rust backend state directory {}: {error}",
                    parent.display()
                ))
            })?;
        }

        let mut stored_state = state.clone().normalized();
        stored_state.apps = stored_state.persisted_apps();
        let contents = serde_json::to_string_pretty(&stored_state).map_err(|error| {
            CoreError::Backend(format!("Unable to serialize Rust backend state: {error}"))
        })?;
        fs::write(&self.path, contents).map_err(|error| {
            CoreError::Backend(format!(
                "Unable to write Rust backend state at {}: {error}",
                self.path.display()
            ))
        })
    }
}

pub fn default_app_entries() -> Vec<AppEntry> {
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

fn current_state_version() -> u32 {
    CURRENT_STATE_VERSION
}

fn default_next_host_number() -> u32 {
    1
}

fn next_manual_host_number(hosts: &HostStore) -> u32 {
    hosts
        .hosts()
        .iter()
        .filter_map(|host| host.id.strip_prefix("manual-host-"))
        .filter_map(|suffix| suffix.parse::<u32>().ok())
        .max()
        .map(|number| number.saturating_add(1))
        .unwrap_or(default_next_host_number())
}

#[cfg(test)]
mod tests {
    use super::{JsonStateStore, StoredState, CURRENT_STATE_VERSION};
    use crate::core::host_store::{HostStore, StoredHost};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_state_path(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join("moonlight-tauri-storage-tests")
            .join(format!("{name}-{nonce}.json"))
    }

    fn sample_host(id: &str) -> StoredHost {
        StoredHost {
            id: id.into(),
            name: "Gaming PC".into(),
            manual_address: "192.168.1.20".into(),
            uuid: "uuid-1".into(),
            paired: true,
            mac_address: "00:11:22:33:44:55".into(),
            server_certificate_pem: String::new(),
        }
    }

    #[test]
    fn missing_state_file_loads_default_state() {
        let store = JsonStateStore::from_file(unique_state_path("missing"));

        let state = store.load().unwrap();

        assert_eq!(CURRENT_STATE_VERSION, state.version);
        assert!(state.hosts.hosts().is_empty());
        assert!(state.apps.iter().any(|app| app.id == "steam"));
    }

    #[test]
    fn state_round_trip_preserves_hosts_and_settings() {
        let store = JsonStateStore::from_file(unique_state_path("round-trip"));
        let mut state = StoredState::default();
        state.hosts.add_or_update(sample_host("manual-host-3"));
        state.settings.width = 2560;
        state.next_host_number = 4;

        store.save(&state).unwrap();
        let loaded = store.load().unwrap();

        assert_eq!(2560, loaded.settings.width);
        assert_eq!(1, loaded.hosts.hosts().len());
        assert_eq!(4, loaded.next_host_number);
    }

    #[test]
    fn malformed_state_reports_parse_error() {
        let path = unique_state_path("malformed");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{not-json").unwrap();
        let store = JsonStateStore::from_file(&path);

        let error = store.load().unwrap_err();

        assert!(error
            .to_string()
            .contains("Unable to parse Rust backend state"));
    }

    #[test]
    fn missing_fields_are_migrated_to_defaults() {
        let path = unique_state_path("migration");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, r#"{"version":0}"#).unwrap();
        let store = JsonStateStore::from_file(&path);

        let state = store.load().unwrap();

        assert_eq!(CURRENT_STATE_VERSION, state.version);
        assert_eq!(1, state.next_host_number);
        assert!(!state.apps.is_empty());
    }

    #[test]
    fn next_host_number_is_migrated_past_manual_hosts() {
        let store = JsonStateStore::from_file(unique_state_path("next-host"));
        let mut hosts = HostStore::new();
        hosts.add_or_update(sample_host("manual-host-9"));
        let state = StoredState {
            hosts,
            next_host_number: 1,
            ..StoredState::default()
        };

        store.save(&state).unwrap();
        let loaded = store.load().unwrap();

        assert_eq!(10, loaded.next_host_number);
    }

    #[test]
    fn running_state_is_not_persisted_for_apps() {
        let store = JsonStateStore::from_file(unique_state_path("app-running"));
        let mut state = StoredState::default();
        state.apps[0].running = true;

        store.save(&state).unwrap();
        let loaded = store.load().unwrap();

        assert!(!loaded.apps[0].running);
    }
}
