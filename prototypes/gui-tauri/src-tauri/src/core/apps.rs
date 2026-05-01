#![allow(dead_code)]

use super::types::AppEntry;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct AppId(String);

impl AppId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppSummary {
    pub id: AppId,
    pub name: String,
    pub hidden: bool,
    pub direct_launch: bool,
    pub running: bool,
    pub app_collector_game: bool,
}

impl From<AppEntry> for AppSummary {
    fn from(entry: AppEntry) -> Self {
        Self {
            id: AppId::new(entry.id),
            name: entry.name,
            hidden: entry.hidden,
            direct_launch: entry.direct_launch,
            running: entry.running,
            app_collector_game: entry.app_collector_game,
        }
    }
}

impl AppSummary {
    pub fn into_entry(self, box_art_url: String) -> AppEntry {
        AppEntry {
            id: self.id.0,
            name: self.name,
            box_art_url,
            hidden: self.hidden,
            direct_launch: self.direct_launch,
            running: self.running,
            app_collector_game: self.app_collector_game,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AppId, AppSummary};
    use crate::core::types::AppEntry;

    #[test]
    fn app_id_exposes_borrowed_string() {
        let id = AppId::new("steam");

        assert_eq!("steam", id.as_str());
    }

    #[test]
    fn app_entry_converts_to_domain_summary_without_box_art() {
        let summary = AppSummary::from(AppEntry {
            id: "steam".into(),
            name: "Steam Big Picture".into(),
            box_art_url: "data:image/png;base64,abc".into(),
            hidden: false,
            direct_launch: true,
            running: false,
            app_collector_game: false,
        });

        assert_eq!("steam", summary.id.as_str());
        assert!(summary.direct_launch);
    }

    #[test]
    fn app_summary_restores_entry_with_supplied_box_art() {
        let entry = AppSummary {
            id: AppId::new("desktop"),
            name: "Desktop".into(),
            hidden: false,
            direct_launch: false,
            running: true,
            app_collector_game: false,
        }
        .into_entry("file:///boxart.png".into());

        assert_eq!("file:///boxart.png", entry.box_art_url);
        assert!(entry.running);
    }
}
