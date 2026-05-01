#![allow(dead_code)]

use super::error::CoreError;
use super::types::{HostEntry, HostStatus};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredHost {
    pub id: String,
    pub name: String,
    pub manual_address: String,
    pub uuid: String,
    pub paired: bool,
    pub mac_address: String,
    #[serde(default)]
    pub server_certificate_pem: String,
}

impl StoredHost {
    pub fn into_entry(self) -> HostEntry {
        HostEntry {
            id: self.id,
            name: self.name,
            address: self.manual_address,
            status: if self.paired {
                HostStatus::Offline
            } else {
                HostStatus::PairingRequired
            },
            paired: self.paired,
            running: false,
            wakeable: !self.mac_address.is_empty(),
            server_supported: true,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostStore {
    hosts: Vec<StoredHost>,
}

impl HostStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn hosts(&self) -> &[StoredHost] {
        &self.hosts
    }

    pub fn add_or_update(&mut self, host: StoredHost) {
        if let Some(existing) = self
            .hosts
            .iter_mut()
            .find(|existing| existing.id == host.id)
        {
            *existing = host;
        } else {
            self.hosts.push(host);
        }
    }

    pub fn remove(&mut self, id: &str) -> Result<StoredHost, CoreError> {
        let Some(index) = self.hosts.iter().position(|host| host.id == id) else {
            return Err(CoreError::NotFound {
                entity: "Host",
                id: id.to_string(),
            });
        };

        Ok(self.hosts.remove(index))
    }

    pub fn entries(&self) -> Vec<HostEntry> {
        self.hosts
            .iter()
            .cloned()
            .map(StoredHost::into_entry)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{HostStore, StoredHost};
    use crate::core::types::HostStatus;

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
    fn store_adds_and_updates_hosts_by_id() {
        let mut store = HostStore::new();
        store.add_or_update(sample_host("gaming-pc"));
        store.add_or_update(StoredHost {
            name: "Renamed PC".into(),
            ..sample_host("gaming-pc")
        });

        assert_eq!(1, store.hosts().len());
        assert_eq!("Renamed PC", store.hosts()[0].name);
    }

    #[test]
    fn store_remove_reports_missing_hosts() {
        let mut store = HostStore::new();
        let error = store.remove("missing").unwrap_err();

        assert_eq!("Host 'missing' was not found.", error.to_string());
    }

    #[test]
    fn stored_host_serializes_with_camel_case_fields() {
        let value = serde_json::to_value(sample_host("gaming-pc")).unwrap();

        assert_eq!("192.168.1.20", value["manualAddress"]);
        assert_eq!("00:11:22:33:44:55", value["macAddress"]);
    }

    #[test]
    fn store_entries_convert_to_host_dtos() {
        let mut store = HostStore::new();
        store.add_or_update(sample_host("gaming-pc"));

        let entries = store.entries();

        assert_eq!(HostStatus::Offline, entries[0].status);
        assert!(entries[0].wakeable);
    }
}
