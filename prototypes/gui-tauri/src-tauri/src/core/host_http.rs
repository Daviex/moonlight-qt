#![allow(dead_code)]

use super::error::CoreError;
use super::types::AppEntry;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerInfo {
    pub app_version: String,
    pub gfe_version: String,
    pub unique_id: String,
    pub current_game_id: i32,
    pub pair_status: String,
    pub server_codec_mode_support: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostApp {
    pub id: String,
    pub name: String,
    pub hidden: bool,
    pub direct_launch: bool,
    pub app_collector_game: bool,
}

impl HostApp {
    pub fn into_entry(self, running_game_id: i32, box_art_url: String) -> AppEntry {
        let running = self
            .id
            .parse::<i32>()
            .map(|id| id == running_game_id)
            .unwrap_or(false);

        AppEntry {
            id: self.id,
            name: self.name,
            box_art_url,
            hidden: self.hidden,
            direct_launch: self.direct_launch,
            running,
            app_collector_game: self.app_collector_game,
        }
    }
}

pub fn parse_server_info(xml: &str) -> Result<ServerInfo, CoreError> {
    Ok(ServerInfo {
        app_version: optional_tag(xml, "appversion"),
        gfe_version: optional_tag(xml, "GfeVersion"),
        unique_id: optional_tag(xml, "uniqueid"),
        current_game_id: optional_tag(xml, "currentgame").parse::<i32>().unwrap_or(0),
        pair_status: optional_tag(xml, "PairStatus"),
        server_codec_mode_support: optional_tag(xml, "ServerCodecModeSupport")
            .parse::<i32>()
            .unwrap_or(0),
    })
}

pub fn parse_app_list(xml: &str) -> Result<Vec<HostApp>, CoreError> {
    let app_blocks = repeated_tag_blocks(xml, "App");
    let mut apps = Vec::with_capacity(app_blocks.len());

    for block in app_blocks {
        let id = required_tag(block, "ID")?;
        let name = required_tag(block, "AppTitle")?;
        apps.push(HostApp {
            id,
            name,
            hidden: false,
            direct_launch: false,
            app_collector_game: false,
        });
    }

    Ok(apps)
}

fn required_tag(xml: &str, tag: &'static str) -> Result<String, CoreError> {
    let value = optional_tag(xml, tag);
    if value.is_empty() {
        return Err(CoreError::Validation(format!(
            "Missing required <{tag}> value in host response."
        )));
    }
    Ok(value)
}

fn optional_tag(xml: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let Some(start) = xml.find(&open) else {
        return String::new();
    };
    let content_start = start + open.len();
    let Some(relative_end) = xml[content_start..].find(&close) else {
        return String::new();
    };

    xml[content_start..content_start + relative_end]
        .trim()
        .to_string()
}

fn repeated_tag_blocks<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut remaining = xml;
    let mut blocks = Vec::new();

    while let Some(start) = remaining.find(&open) {
        let content_start = start + open.len();
        let Some(relative_end) = remaining[content_start..].find(&close) else {
            break;
        };
        let content_end = content_start + relative_end;
        blocks.push(&remaining[content_start..content_end]);
        remaining = &remaining[content_end + close.len()..];
    }

    blocks
}

#[cfg(test)]
mod tests {
    use super::{parse_app_list, parse_server_info, HostApp};

    #[test]
    fn parses_server_info_fixture() {
        let info = parse_server_info(
            r#"
            <root>
                <appversion>Sunshine v0.23.1</appversion>
                <GfeVersion>3.23</GfeVersion>
                <uniqueid>abc123</uniqueid>
                <currentgame>12345</currentgame>
                <PairStatus>1</PairStatus>
                <ServerCodecModeSupport>65535</ServerCodecModeSupport>
            </root>
            "#,
        )
        .unwrap();

        assert_eq!("Sunshine v0.23.1", info.app_version);
        assert_eq!("abc123", info.unique_id);
        assert_eq!(12345, info.current_game_id);
        assert_eq!(65535, info.server_codec_mode_support);
    }

    #[test]
    fn parses_app_list_fixture() {
        let apps = parse_app_list(
            r#"
            <root>
                <App><ID>1</ID><AppTitle>Desktop</AppTitle></App>
                <App><ID>2</ID><AppTitle>Steam Big Picture</AppTitle></App>
            </root>
            "#,
        )
        .unwrap();

        assert_eq!(2, apps.len());
        assert_eq!("1", apps[0].id);
        assert_eq!("Steam Big Picture", apps[1].name);
    }

    #[test]
    fn app_list_reports_missing_required_fields() {
        let error = parse_app_list("<root><App><ID>1</ID></App></root>").unwrap_err();

        assert_eq!(
            "Missing required <AppTitle> value in host response.",
            error.to_string()
        );
    }

    #[test]
    fn host_app_converts_to_entry_with_running_state() {
        let entry = HostApp {
            id: "42".into(),
            name: "Game".into(),
            hidden: false,
            direct_launch: true,
            app_collector_game: false,
        }
        .into_entry(42, String::new());

        assert!(entry.running);
        assert!(entry.direct_launch);
    }
}
