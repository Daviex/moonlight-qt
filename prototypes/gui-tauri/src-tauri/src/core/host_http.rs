#![allow(dead_code)]

use super::error::CoreError;
use super::types::AppEntry;
use std::time::Duration;

const DEFAULT_HTTPS_PORT: u16 = 47984;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostEndpoint {
    address: String,
    https_port: u16,
}

impl HostEndpoint {
    pub fn new(address: impl Into<String>, https_port: u16) -> Result<Self, CoreError> {
        let address = address.into();
        if address.trim().is_empty() {
            return Err(CoreError::Validation("Host address is required.".into()));
        }

        Ok(Self {
            address: address.trim().to_string(),
            https_port: if https_port == 0 {
                DEFAULT_HTTPS_PORT
            } else {
                https_port
            },
        })
    }

    pub fn from_address(address: impl Into<String>) -> Result<Self, CoreError> {
        Self::new(address, DEFAULT_HTTPS_PORT)
    }

    pub fn server_info_url(&self) -> String {
        self.endpoint_url("serverinfo")
    }

    pub fn app_list_url(&self) -> String {
        self.endpoint_url("applist")
    }

    fn endpoint_url(&self, endpoint: &str) -> String {
        format!("https://{}:{}/{}", self.address, self.https_port, endpoint)
    }
}

pub trait HostHttpTransport {
    fn get_text(&self, url: &str) -> Result<String, CoreError>;
}

pub struct ReqwestHostHttpTransport {
    client: reqwest::blocking::Client,
}

impl ReqwestHostHttpTransport {
    pub fn new() -> Result<Self, CoreError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|error| {
                CoreError::Backend(format!("Unable to initialize host HTTP client: {error}"))
            })?;
        Ok(Self { client })
    }
}

impl HostHttpTransport for ReqwestHostHttpTransport {
    fn get_text(&self, url: &str) -> Result<String, CoreError> {
        let response = self.client.get(url).send().map_err(|error| {
            CoreError::Backend(format!("Host request failed for {url}: {error}"))
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(CoreError::Backend(format!(
                "Host request failed for {url}: HTTP {status}"
            )));
        }

        response.text().map_err(|error| {
            CoreError::Backend(format!("Unable to read host response from {url}: {error}"))
        })
    }
}

pub struct HostHttpClient<T> {
    transport: T,
}

impl<T> HostHttpClient<T>
where
    T: HostHttpTransport,
{
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    pub fn fetch_server_info(&self, endpoint: &HostEndpoint) -> Result<ServerInfo, CoreError> {
        let body = self.transport.get_text(&endpoint.server_info_url())?;
        parse_server_info(&body)
    }

    pub fn fetch_app_list(&self, endpoint: &HostEndpoint) -> Result<Vec<HostApp>, CoreError> {
        let body = self.transport.get_text(&endpoint.app_list_url())?;
        parse_app_list(&body)
    }
}

pub type BlockingHostHttpClient = HostHttpClient<ReqwestHostHttpTransport>;

impl BlockingHostHttpClient {
    pub fn connect() -> Result<Self, CoreError> {
        Ok(Self::new(ReqwestHostHttpTransport::new()?))
    }
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
    use super::{
        parse_app_list, parse_server_info, HostApp, HostEndpoint, HostHttpClient, HostHttpTransport,
    };
    use crate::core::error::CoreError;
    use std::cell::RefCell;

    #[derive(Default)]
    struct FakeTransport {
        requests: RefCell<Vec<String>>,
    }

    impl HostHttpTransport for FakeTransport {
        fn get_text(&self, url: &str) -> Result<String, CoreError> {
            self.requests.borrow_mut().push(url.to_string());
            if url.ends_with("/serverinfo") {
                return Ok("<root><appversion>Sunshine</appversion></root>".into());
            }
            Ok("<root><App><ID>1</ID><AppTitle>Desktop</AppTitle></App></root>".into())
        }
    }

    #[test]
    fn host_endpoint_builds_default_https_urls() {
        let endpoint = HostEndpoint::from_address("192.168.1.20").unwrap();

        assert_eq!(
            "https://192.168.1.20:47984/serverinfo",
            endpoint.server_info_url()
        );
        assert_eq!(
            "https://192.168.1.20:47984/applist",
            endpoint.app_list_url()
        );
    }

    #[test]
    fn host_endpoint_rejects_empty_addresses() {
        let error = HostEndpoint::from_address(" ").unwrap_err();

        assert_eq!("Host address is required.", error.to_string());
    }

    #[test]
    fn client_fetches_and_parses_server_info() {
        let transport = FakeTransport::default();
        let client = HostHttpClient::new(transport);
        let endpoint = HostEndpoint::from_address("sunshine.local").unwrap();

        let info = client.fetch_server_info(&endpoint).unwrap();

        assert_eq!("Sunshine", info.app_version);
    }

    #[test]
    fn client_fetches_and_parses_app_list() {
        let transport = FakeTransport::default();
        let client = HostHttpClient::new(transport);
        let endpoint = HostEndpoint::from_address("sunshine.local").unwrap();

        let apps = client.fetch_app_list(&endpoint).unwrap();

        assert_eq!("Desktop", apps[0].name);
    }

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
