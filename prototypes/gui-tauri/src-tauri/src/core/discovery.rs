#![allow(dead_code)]

use super::error::CoreError;
use super::host_store::{HostStore, StoredHost};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::time::{Duration, Instant};

const NVSTREAM_SERVICE_TYPE: &str = "_nvstream._tcp.local.";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryRecord {
    pub name: String,
    pub address: String,
    pub https_port: u16,
    pub mac_address: String,
}

impl DiscoveryRecord {
    pub fn normalized_key(&self) -> String {
        let name = self.name.trim().to_casefolded();
        let address = self.address.trim().to_casefolded();
        format!("{name}\x1f{address}\x1f{}", self.https_port)
    }

    pub fn is_usable(&self) -> bool {
        !self.name.trim().is_empty() && !self.address.trim().is_empty() && self.https_port != 0
    }
}

pub fn deduplicate_records(records: Vec<DiscoveryRecord>) -> Vec<DiscoveryRecord> {
    let mut unique = Vec::new();
    let mut keys = std::collections::HashSet::new();

    for record in records {
        if !record.is_usable() {
            continue;
        }

        let key = record.normalized_key();
        if keys.insert(key) {
            unique.push(record);
        }
    }

    unique
}

pub fn merge_discovered_hosts(hosts: &mut HostStore, records: Vec<DiscoveryRecord>) -> Vec<String> {
    let mut changed_host_ids = Vec::new();
    for (index, record) in deduplicate_records(records).into_iter().enumerate() {
        let existing = hosts
            .hosts()
            .iter()
            .find(|host| same_host(host, &record))
            .cloned();
        let is_new = existing.is_none();
        let mut host = existing.unwrap_or_else(|| StoredHost {
            id: format!(
                "discovered-{}",
                record.normalized_key().replace('\x1f', "-")
            ),
            name: record.name.trim().to_string(),
            manual_address: record.address.trim().to_string(),
            uuid: format!("discovered-{index}"),
            paired: false,
            mac_address: String::new(),
            server_certificate_pem: String::new(),
        });

        let original = host.clone();
        host.name = record.name.trim().to_string();
        host.manual_address = record.address.trim().to_string();
        if !record.mac_address.trim().is_empty() {
            host.mac_address = record.mac_address.trim().to_string();
        }

        if is_new || host != original {
            changed_host_ids.push(host.id.clone());
            hosts.add_or_update(host);
        }
    }
    changed_host_ids
}

pub fn discover_nvstream_hosts(timeout: Duration) -> Result<Vec<DiscoveryRecord>, CoreError> {
    let mdns = ServiceDaemon::new()
        .map_err(|error| CoreError::Backend(format!("Unable to start mDNS discovery: {error}")))?;
    let receiver = mdns.browse(NVSTREAM_SERVICE_TYPE).map_err(|error| {
        CoreError::Backend(format!("Unable to browse for GameStream hosts: {error}"))
    })?;
    let deadline = Instant::now() + timeout;
    let mut records = Vec::new();

    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        if remaining.is_zero() {
            break;
        }
        match receiver.recv_timeout(remaining.min(Duration::from_millis(250))) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                if !info.is_valid() {
                    continue;
                }
                let Some(address) = info.addresses.iter().next().map(ToString::to_string) else {
                    continue;
                };
                let mac_address = info
                    .txt_properties
                    .get_property_val_str("mac")
                    .unwrap_or_default()
                    .to_string();
                records.push(DiscoveryRecord {
                    name: service_instance_name(&info.fullname),
                    address,
                    https_port: info.port,
                    mac_address,
                });
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    let _ = mdns.shutdown();
    Ok(deduplicate_records(records))
}

fn same_host(host: &StoredHost, record: &DiscoveryRecord) -> bool {
    let host_address = host.manual_address.trim().to_casefolded();
    let record_address = record.address.trim().to_casefolded();
    let host_name = host.name.trim().to_casefolded();
    let record_name = record.name.trim().to_casefolded();

    host_address == record_address || host_name == record_name
}

fn service_instance_name(fullname: &str) -> String {
    fullname
        .strip_suffix(NVSTREAM_SERVICE_TYPE)
        .unwrap_or(fullname)
        .trim_end_matches('.')
        .to_string()
}

trait CaseFold {
    fn to_casefolded(&self) -> String;
}

impl CaseFold for str {
    fn to_casefolded(&self) -> String {
        self.to_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        deduplicate_records, merge_discovered_hosts, service_instance_name, DiscoveryRecord,
    };
    use crate::core::host_store::{HostStore, StoredHost};

    #[test]
    fn discovery_record_key_is_normalized() {
        let record = DiscoveryRecord {
            name: " Gaming PC ".into(),
            address: " MOONLIGHT.LOCAL ".into(),
            https_port: 47984,
            mac_address: String::new(),
        };

        assert_eq!(
            "gaming pc\x1fmoonlight.local\x1f47984",
            record.normalized_key()
        );
    }

    #[test]
    fn discovery_deduplication_drops_unusable_and_duplicate_records() {
        let records = deduplicate_records(vec![
            DiscoveryRecord {
                name: "Gaming PC".into(),
                address: "moonlight.local".into(),
                https_port: 47984,
                mac_address: String::new(),
            },
            DiscoveryRecord {
                name: " gaming pc ".into(),
                address: "MOONLIGHT.local".into(),
                https_port: 47984,
                mac_address: String::new(),
            },
            DiscoveryRecord {
                name: String::new(),
                address: "missing-name.local".into(),
                https_port: 47984,
                mac_address: String::new(),
            },
        ]);

        assert_eq!(1, records.len());
        assert_eq!("Gaming PC", records[0].name);
    }

    #[test]
    fn discovery_merge_updates_existing_host_by_address() {
        let mut hosts = HostStore::new();
        hosts.add_or_update(StoredHost {
            id: "manual-host".into(),
            name: "Old Name".into(),
            manual_address: "192.168.1.20".into(),
            uuid: "manual".into(),
            paired: true,
            mac_address: String::new(),
            server_certificate_pem: String::new(),
        });

        let changed = merge_discovered_hosts(
            &mut hosts,
            vec![DiscoveryRecord {
                name: "Gaming PC".into(),
                address: "192.168.1.20".into(),
                https_port: 47984,
                mac_address: "00:11:22:33:44:55".into(),
            }],
        );

        assert_eq!(vec!["manual-host"], changed);
        assert_eq!("Gaming PC", hosts.hosts()[0].name);
        assert_eq!("00:11:22:33:44:55", hosts.hosts()[0].mac_address);
        assert!(hosts.hosts()[0].paired);
    }

    #[test]
    fn discovery_merge_adds_new_unpaired_host() {
        let mut hosts = HostStore::new();

        let changed = merge_discovered_hosts(
            &mut hosts,
            vec![DiscoveryRecord {
                name: "New Host".into(),
                address: "192.168.1.30".into(),
                https_port: 47984,
                mac_address: String::new(),
            }],
        );

        assert_eq!(1, changed.len());
        assert_eq!("New Host", hosts.hosts()[0].name);
        assert!(!hosts.hosts()[0].paired);
    }

    #[test]
    fn service_instance_name_strips_nvstream_suffix() {
        assert_eq!(
            "Gaming PC",
            service_instance_name("Gaming PC._nvstream._tcp.local.")
        );
    }
}
