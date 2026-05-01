#![allow(dead_code)]

use super::error::CoreError;
use super::gamestream;
use super::gamestream_sys;
use super::types::AppEntry;
use std::time::Duration;

const DEFAULT_HTTPS_PORT: u16 = 47984;
const DEFAULT_HTTP_PORT: u16 = 47989;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerInfo {
    pub app_version: String,
    pub gfe_version: String,
    pub state: String,
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
    http_port: u16,
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
            http_port: DEFAULT_HTTP_PORT,
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

    pub fn http_server_info_url(&self) -> String {
        self.url_with_query("http", self.http_port, "serverinfo", "")
    }

    pub fn app_list_url(&self) -> String {
        self.endpoint_url("applist")
    }

    pub fn http_pair_url(&self, query: &str) -> String {
        self.url_with_query("http", self.http_port, "pair", query)
    }

    pub fn https_pair_url(&self, query: &str) -> String {
        self.url_with_query("https", self.https_port, "pair", query)
    }

    pub fn http_unpair_url(&self) -> String {
        self.url_with_query("http", self.http_port, "unpair", "")
    }

    pub fn launch_url(&self, query: &str) -> String {
        self.url_with_query("https", self.https_port, "launch", query)
    }

    pub fn resume_url(&self, query: &str) -> String {
        self.url_with_query("https", self.https_port, "resume", query)
    }

    pub fn cancel_url(&self) -> String {
        self.endpoint_url("cancel")
    }

    fn endpoint_url(&self, endpoint: &str) -> String {
        self.url_with_query("https", self.https_port, endpoint, "")
    }

    fn url_with_query(&self, scheme: &str, port: u16, endpoint: &str, query: &str) -> String {
        let host = self.host_for_url();
        if query.is_empty() {
            return format!("{scheme}://{host}:{port}/{endpoint}");
        }

        format!("{scheme}://{host}:{port}/{endpoint}?{query}")
    }

    fn host_for_url(&self) -> String {
        let address = self.address.trim();
        if address.starts_with('[') && address.ends_with(']') {
            return address.to_string();
        }
        if !address.contains(':') {
            return address.to_string();
        }

        format!("[{}]", address.replace('%', "%25"))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartAppRequest {
    pub app_id: u32,
    pub is_gfe: bool,
    pub sops: bool,
    pub local_audio: bool,
    pub gamepad_mask: u32,
    pub persist_game_controllers_on_disconnect: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartAppSession {
    pub rtsp_session_url: Option<String>,
}

impl StartAppRequest {
    pub fn launch_query(&self, stream: &gamestream::StreamConfiguration) -> String {
        self.query(stream)
    }

    fn query(&self, stream: &gamestream::StreamConfiguration) -> String {
        let raw = stream.to_raw();
        let fps = if raw.fps > 60 && self.is_gfe {
            0
        } else {
            raw.fps
        };
        let ri_key_id = u32::from_be_bytes([
            raw.remote_input_aes_iv[0] as u8,
            raw.remote_input_aes_iv[1] as u8,
            raw.remote_input_aes_iv[2] as u8,
            raw.remote_input_aes_iv[3] as u8,
        ]);

        format!(
            "appid={}&mode={}x{}x{}&additionalStates=1&sops={}&rikey={}&rikeyid={}{}&localAudioPlayMode={}&surroundAudioInfo={}&remoteControllersBitmap={}&gcmap={}&gcpersist={}&corever=1",
            self.app_id,
            raw.width,
            raw.height,
            fps,
            bool_as_int(self.sops),
            hex_c_chars(raw.remote_input_aes_key),
            ri_key_id,
            hdr_query_parameters(raw.supported_video_formats),
            bool_as_int(self.local_audio),
            surround_audio_info(raw.audio_configuration),
            self.gamepad_mask,
            self.gamepad_mask,
            bool_as_int(self.persist_game_controllers_on_disconnect),
        )
    }
}

fn bool_as_int(value: bool) -> u8 {
    u8::from(value)
}

fn hex_c_chars(bytes: [std::os::raw::c_char; 16]) -> String {
    bytes
        .iter()
        .map(|value| format!("{:02x}", *value as u8))
        .collect()
}

fn surround_audio_info(audio_configuration: i32) -> i32 {
    let channel_count = (audio_configuration >> 8) & 0xFF;
    let channel_mask = (audio_configuration >> 16) & 0xFFFF;
    (channel_mask << 16) | channel_count
}

fn hdr_query_parameters(supported_video_formats: i32) -> &'static str {
    const VIDEO_FORMAT_MASK_10BIT: i32 = gamestream_sys::VIDEO_FORMAT_H265_MAIN10
        | gamestream_sys::VIDEO_FORMAT_HEVC_REXT10_444
        | gamestream_sys::VIDEO_FORMAT_AV1_MAIN10
        | gamestream_sys::VIDEO_FORMAT_AV1_HIGH10_444;

    if supported_video_formats & VIDEO_FORMAT_MASK_10BIT == 0 {
        ""
    } else {
        "&hdrMode=1&clientHdrCapVersion=0&clientHdrCapSupportedFlagsInUint32=0&clientHdrCapMetaDataId=NV_STATIC_METADATA_TYPE_1&clientHdrCapDisplayData=0x0x0x0x0x0x0x0x0x0x0"
    }
}

pub trait HostHttpTransport {
    fn get_text(&self, url: &str) -> Result<String, CoreError>;

    fn get_text_with_client_identity(
        &self,
        url: &str,
        _certificate_pem: &str,
        _private_key_pem: &str,
    ) -> Result<String, CoreError> {
        self.get_text(url)
    }
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

    fn client_with_identity(
        &self,
        certificate_pem: &str,
        private_key_pem: &str,
    ) -> Result<reqwest::blocking::Client, CoreError> {
        let identity_pem = format!("{certificate_pem}\n{private_key_pem}");
        let identity = reqwest::Identity::from_pem(identity_pem.as_bytes()).map_err(|error| {
            CoreError::Validation(format!("Client identity is not valid PEM: {error}"))
        })?;

        reqwest::blocking::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .danger_accept_invalid_certs(true)
            .identity(identity)
            .build()
            .map_err(|error| {
                CoreError::Backend(format!(
                    "Unable to initialize authenticated host HTTP client: {error}"
                ))
            })
    }

    fn get_text_with_client(
        &self,
        client: &reqwest::blocking::Client,
        url: &str,
    ) -> Result<String, CoreError> {
        let response = client.get(url).send().map_err(|error| {
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

impl HostHttpTransport for ReqwestHostHttpTransport {
    fn get_text(&self, url: &str) -> Result<String, CoreError> {
        self.get_text_with_client(&self.client, url)
    }

    fn get_text_with_client_identity(
        &self,
        url: &str,
        certificate_pem: &str,
        private_key_pem: &str,
    ) -> Result<String, CoreError> {
        let client = self.client_with_identity(certificate_pem, private_key_pem)?;
        self.get_text_with_client(&client, url)
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

    pub fn fetch_unpaired_server_info(
        &self,
        endpoint: &HostEndpoint,
    ) -> Result<ServerInfo, CoreError> {
        let body = self.transport.get_text(&endpoint.http_server_info_url())?;
        parse_server_info(&body)
    }

    pub fn fetch_app_list(&self, endpoint: &HostEndpoint) -> Result<Vec<HostApp>, CoreError> {
        let body = self.transport.get_text(&endpoint.app_list_url())?;
        parse_app_list(&body)
    }

    pub fn launch_app(
        &self,
        endpoint: &HostEndpoint,
        request: &StartAppRequest,
        stream: &gamestream::StreamConfiguration,
    ) -> Result<StartAppSession, CoreError> {
        let body = self
            .transport
            .get_text(&endpoint.launch_url(&request.launch_query(stream)))?;
        parse_start_app_response(&body)
    }

    pub fn resume_app(
        &self,
        endpoint: &HostEndpoint,
        request: &StartAppRequest,
        stream: &gamestream::StreamConfiguration,
    ) -> Result<StartAppSession, CoreError> {
        let body = self
            .transport
            .get_text(&endpoint.resume_url(&request.launch_query(stream)))?;
        parse_start_app_response(&body)
    }

    pub fn quit_app(&self, endpoint: &HostEndpoint) -> Result<(), CoreError> {
        let body = self.transport.get_text(&endpoint.cancel_url())?;
        verify_response_status(&body)
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
        state: optional_tag(xml, "state"),
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

pub fn parse_start_app_response(xml: &str) -> Result<StartAppSession, CoreError> {
    verify_response_status(xml)?;
    let rtsp_session_url = optional_tag(xml, "sessionUrl0");

    Ok(StartAppSession {
        rtsp_session_url: if rtsp_session_url.is_empty() {
            None
        } else {
            Some(rtsp_session_url)
        },
    })
}

fn verify_response_status(xml: &str) -> Result<(), CoreError> {
    let Some(root_start) = xml.find("<root") else {
        return Err(CoreError::Backend(
            "Host returned malformed XML: missing root element.".into(),
        ));
    };
    let Some(root_end) = xml[root_start..].find('>') else {
        return Err(CoreError::Backend(
            "Host returned malformed XML: unterminated root element.".into(),
        ));
    };
    let root = &xml[root_start..root_start + root_end + 1];
    let status_code = root_attribute(root, "status_code")
        .ok_or_else(|| CoreError::Backend("Host response is missing status_code.".into()))?;
    if status_code == "200" {
        return Ok(());
    }

    let status_message = root_attribute(root, "status_message").unwrap_or_default();
    Err(CoreError::Backend(format!(
        "Host request failed with status {status_code}: {status_message}"
    )))
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

fn root_attribute(root: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}=\"");
    let start = root.find(&prefix)? + prefix.len();
    let end = root[start..].find('"')?;
    Some(root[start..start + end].to_string())
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
        parse_app_list, parse_server_info, parse_start_app_response, HostApp, HostEndpoint,
        HostHttpClient, HostHttpTransport, StartAppRequest,
    };
    use crate::core::error::CoreError;
    use crate::core::gamestream::{AudioConfiguration, RemoteInputCrypto, StreamConfiguration};
    use crate::core::gamestream_sys;
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
            if url.contains("/launch?") || url.contains("/resume?") {
                return Ok(
                    r#"<root status_code="200"><sessionUrl0>rtsp://session</sessionUrl0></root>"#
                        .into(),
                );
            }
            if url.ends_with("/cancel") {
                return Ok(r#"<root status_code="200"></root>"#.into());
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
            "http://192.168.1.20:47989/serverinfo",
            endpoint.http_server_info_url()
        );
        assert_eq!(
            "https://192.168.1.20:47984/applist",
            endpoint.app_list_url()
        );
        assert_eq!(
            "http://192.168.1.20:47989/unpair",
            endpoint.http_unpair_url()
        );
        assert_eq!(
            "https://192.168.1.20:47984/launch?appid=123",
            endpoint.launch_url("appid=123")
        );
        assert_eq!(
            "https://192.168.1.20:47984/resume?appid=123",
            endpoint.resume_url("appid=123")
        );
        assert_eq!("https://192.168.1.20:47984/cancel", endpoint.cancel_url());
    }

    #[test]
    fn host_endpoint_rejects_empty_addresses() {
        let error = HostEndpoint::from_address(" ").unwrap_err();

        assert_eq!("Host address is required.", error.to_string());
    }

    #[test]
    fn host_endpoint_formats_ipv6_and_scoped_ipv6_urls() {
        let ipv6 = HostEndpoint::from_address("fd00::1234").unwrap();
        let scoped = HostEndpoint::from_address("fe80::6673:b6d8:21d5:a620%6").unwrap();

        assert_eq!(
            "https://[fd00::1234]:47984/serverinfo",
            ipv6.server_info_url()
        );
        assert_eq!(
            "https://[fe80::6673:b6d8:21d5:a620%256]:47984/serverinfo",
            scoped.server_info_url()
        );
        assert_eq!(
            "http://[fe80::6673:b6d8:21d5:a620%256]:47989/pair?devicename=roth",
            scoped.http_pair_url("devicename=roth")
        );
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
    fn client_fetches_unpaired_server_info_over_http() {
        let transport = FakeTransport::default();
        let client = HostHttpClient::new(transport);
        let endpoint = HostEndpoint::from_address("sunshine.local").unwrap();

        let info = client.fetch_unpaired_server_info(&endpoint).unwrap();

        assert_eq!("Sunshine", info.app_version);
        assert_eq!(
            vec!["http://sunshine.local:47989/serverinfo"],
            client.transport.requests.into_inner()
        );
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
                <state>MJOLNIR_SERVER</state>
                <uniqueid>abc123</uniqueid>
                <currentgame>12345</currentgame>
                <PairStatus>1</PairStatus>
                <ServerCodecModeSupport>65535</ServerCodecModeSupport>
            </root>
            "#,
        )
        .unwrap();

        assert_eq!("Sunshine v0.23.1", info.app_version);
        assert_eq!("MJOLNIR_SERVER", info.state);
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

    #[test]
    fn start_app_request_builds_native_launch_query() {
        let stream = StreamConfiguration {
            width: 3840,
            height: 2160,
            fps: 120,
            bitrate_kbps: 80_000,
            packet_size: 1392,
            streaming_remotely: crate::core::gamestream::StreamingRemotely::Auto,
            audio_configuration: AudioConfiguration::Surround51,
            supported_video_formats: gamestream_sys::VIDEO_FORMAT_H265_MAIN10,
            remote_input_crypto: RemoteInputCrypto {
                aes_key: [0xAB; 16],
                aes_iv: [0x01, 0x02, 0x03, 0x04, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            },
        };
        let request = StartAppRequest {
            app_id: 123,
            is_gfe: true,
            sops: true,
            local_audio: false,
            gamepad_mask: 3,
            persist_game_controllers_on_disconnect: true,
        };

        let query = request.launch_query(&stream);

        assert!(query.contains("appid=123"));
        assert!(query.contains("mode=3840x2160x0"));
        assert!(query.contains("sops=1"));
        assert!(query.contains("rikey=abababababababababababababababab"));
        assert!(query.contains("rikeyid=16909060"));
        assert!(query.contains("hdrMode=1"));
        assert!(query.contains("localAudioPlayMode=0"));
        assert!(query.contains("surroundAudioInfo=4128774"));
        assert!(query.contains("remoteControllersBitmap=3"));
        assert!(query.contains("gcmap=3"));
        assert!(query.contains("gcpersist=1"));
        assert!(query.ends_with("&corever=1"));
    }

    #[test]
    fn client_launch_resume_and_quit_use_native_endpoints() {
        let transport = FakeTransport::default();
        let client = HostHttpClient::new(transport);
        let endpoint = HostEndpoint::from_address("sunshine.local").unwrap();
        let stream = StreamConfiguration::default().with_remote_input_crypto(RemoteInputCrypto {
            aes_key: [0x11; 16],
            aes_iv: [0x01, 0x02, 0x03, 0x04, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        });
        let request = StartAppRequest {
            app_id: 7,
            is_gfe: false,
            sops: true,
            local_audio: false,
            gamepad_mask: 1,
            persist_game_controllers_on_disconnect: true,
        };

        let launched = client.launch_app(&endpoint, &request, &stream).unwrap();
        let resumed = client.resume_app(&endpoint, &request, &stream).unwrap();
        client.quit_app(&endpoint).unwrap();

        assert_eq!(Some("rtsp://session".into()), launched.rtsp_session_url);
        assert_eq!(Some("rtsp://session".into()), resumed.rtsp_session_url);
    }

    #[test]
    fn start_app_response_reports_host_status_errors() {
        let error =
            parse_start_app_response(r#"<root status_code="599" status_message="Busy"></root>"#)
                .unwrap_err();

        assert_eq!(
            "Host request failed with status 599: Busy",
            error.to_string()
        );
    }
}
