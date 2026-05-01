#![allow(dead_code)]

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryRecord {
    pub name: String,
    pub address: String,
    pub https_port: u16,
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
    use super::{deduplicate_records, DiscoveryRecord};

    #[test]
    fn discovery_record_key_is_normalized() {
        let record = DiscoveryRecord {
            name: " Gaming PC ".into(),
            address: " MOONLIGHT.LOCAL ".into(),
            https_port: 47984,
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
            },
            DiscoveryRecord {
                name: " gaming pc ".into(),
                address: "MOONLIGHT.local".into(),
                https_port: 47984,
            },
            DiscoveryRecord {
                name: String::new(),
                address: "missing-name.local".into(),
                https_port: 47984,
            },
        ]);

        assert_eq!(1, records.len());
        assert_eq!("Gaming PC", records[0].name);
    }
}
