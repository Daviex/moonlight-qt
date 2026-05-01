#![allow(dead_code)]

use super::types::{HostEntry, HostStatus};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct HostId(String);

impl HostId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostSummary {
    pub id: HostId,
    pub name: String,
    pub address: String,
    pub online: bool,
    pub paired: bool,
    pub running: bool,
    pub wakeable: bool,
    pub server_supported: bool,
}

impl From<HostEntry> for HostSummary {
    fn from(entry: HostEntry) -> Self {
        Self {
            id: HostId::new(entry.id),
            name: entry.name,
            address: entry.address,
            online: entry.status == HostStatus::Online,
            paired: entry.paired,
            running: entry.running,
            wakeable: entry.wakeable,
            server_supported: entry.server_supported,
        }
    }
}

impl From<HostSummary> for HostEntry {
    fn from(summary: HostSummary) -> Self {
        Self {
            id: summary.id.0,
            name: summary.name,
            address: summary.address,
            status: if summary.online {
                HostStatus::Online
            } else if summary.paired {
                HostStatus::Offline
            } else {
                HostStatus::PairingRequired
            },
            paired: summary.paired,
            running: summary.running,
            wakeable: summary.wakeable,
            server_supported: summary.server_supported,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HostId, HostSummary};
    use crate::core::types::{HostEntry, HostStatus};

    #[test]
    fn host_id_exposes_borrowed_string() {
        let id = HostId::new("gaming-pc");

        assert_eq!("gaming-pc", id.as_str());
    }

    #[test]
    fn host_entry_converts_to_domain_summary() {
        let summary = HostSummary::from(HostEntry {
            id: "gaming-pc".into(),
            name: "Gaming PC".into(),
            address: "192.168.1.20".into(),
            status: HostStatus::Online,
            paired: true,
            running: false,
            wakeable: true,
            server_supported: true,
        });

        assert_eq!("gaming-pc", summary.id.as_str());
        assert!(summary.online);
        assert!(summary.paired);
    }

    #[test]
    fn unpaired_offline_summary_converts_to_pairing_required_entry() {
        let entry = HostEntry::from(HostSummary {
            id: HostId::new("new-host"),
            name: "New Host".into(),
            address: "192.168.1.40".into(),
            online: false,
            paired: false,
            running: false,
            wakeable: false,
            server_supported: true,
        });

        assert_eq!(HostStatus::PairingRequired, entry.status);
    }
}
