#![allow(dead_code)]

use super::error::CoreError;
use super::gamestream;
use super::gamestream_sys;
use super::types::AppEntry;
use crate::logger;
use rand::rngs::OsRng;
use rand::RngCore;
#[cfg(moonlight_common_c_linked)]
use std::ffi::CStr;
use std::time::Duration;

const DEFAULT_HTTPS_PORT: u16 = 47984;
const DEFAULT_HTTP_PORT: u16 = 47989;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerInfo {
    pub hostname: String,
    pub app_version: String,
    pub gfe_version: String,
    pub state: String,
    pub unique_id: String,
    pub mac_address: String,
    pub local_ip: String,
    pub external_ip: String,
    pub external_port: u16,
    pub https_port: u16,
    pub current_game_id: i32,
    pub pair_status: String,
    pub server_codec_mode_support: i32,
    pub gpu_model: String,
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum HostRequestAuth {
    #[default]
    None,
    ClientIdentity {
        unique_id: String,
        certificate_pem: String,
        private_key_pem: String,
    },
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

    pub fn app_asset_url(&self, app_id: &str) -> String {
        self.url_with_query(
            "https",
            self.https_port,
            "appasset",
            &format!("appid={app_id}&AssetType=2&AssetIdx=0"),
        )
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
            "appid={}&mode={}x{}x{}&additionalStates=1&sops={}&rikey={}&rikeyid={}{}&localAudioPlayMode={}&surroundAudioInfo={}&remoteControllersBitmap={}&gcmap={}&gcpersist={}{}",
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
            launch_url_query_parameters(),
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

#[cfg(moonlight_common_c_linked)]
fn launch_url_query_parameters() -> String {
    // SAFETY: moonlight-common-c returns a process-static, NUL-terminated string.
    let ptr = unsafe { gamestream_sys::LiGetLaunchUrlQueryParameters() };
    if ptr.is_null() {
        logger::log("moonlight-common-c returned null launch URL query parameters");
        return String::new();
    }

    // SAFETY: The C API contract returns a valid NUL-terminated string pointer or NULL.
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(not(moonlight_common_c_linked))]
fn launch_url_query_parameters() -> String {
    "&corever=1".into()
}

pub trait HostHttpTransport {
    fn get_text(&self, url: &str) -> Result<String, CoreError>;

    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, CoreError>;

    fn get_text_with_client_identity(
        &self,
        url: &str,
        _certificate_pem: &str,
        _private_key_pem: &str,
    ) -> Result<String, CoreError> {
        self.get_text(url)
    }

    fn get_bytes_with_client_identity(
        &self,
        url: &str,
        _certificate_pem: &str,
        _private_key_pem: &str,
    ) -> Result<Vec<u8>, CoreError> {
        self.get_bytes(url)
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

    fn get_bytes_with_client(
        &self,
        client: &reqwest::blocking::Client,
        url: &str,
    ) -> Result<Vec<u8>, CoreError> {
        let response = client.get(url).send().map_err(|error| {
            CoreError::Backend(format!("Host request failed for {url}: {error}"))
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(CoreError::Backend(format!(
                "Host request failed for {url}: HTTP {status}"
            )));
        }

        response
            .bytes()
            .map(|bytes| bytes.to_vec())
            .map_err(|error| {
                CoreError::Backend(format!("Unable to read host response from {url}: {error}"))
            })
    }
}

impl HostHttpTransport for ReqwestHostHttpTransport {
    fn get_text(&self, url: &str) -> Result<String, CoreError> {
        self.get_text_with_client(&self.client, url)
    }

    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, CoreError> {
        self.get_bytes_with_client(&self.client, url)
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

    fn get_bytes_with_client_identity(
        &self,
        url: &str,
        certificate_pem: &str,
        private_key_pem: &str,
    ) -> Result<Vec<u8>, CoreError> {
        let client = self.client_with_identity(certificate_pem, private_key_pem)?;
        self.get_bytes_with_client(&client, url)
    }
}

pub struct HostHttpClient<T> {
    transport: T,
}

pub struct HostRequestContext<'a, T> {
    client: &'a HostHttpClient<T>,
    endpoint: HostEndpoint,
    auth: HostRequestAuth,
}

impl<T> HostHttpClient<T>
where
    T: HostHttpTransport,
{
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    pub fn request_context(
        &self,
        endpoint: HostEndpoint,
        auth: HostRequestAuth,
    ) -> HostRequestContext<'_, T> {
        HostRequestContext {
            client: self,
            endpoint,
            auth,
        }
    }

    pub fn fetch_server_info(&self, endpoint: &HostEndpoint) -> Result<ServerInfo, CoreError> {
        self.request_context(endpoint.clone(), HostRequestAuth::None)
            .fetch_server_info()
    }

    pub fn fetch_unpaired_server_info(
        &self,
        endpoint: &HostEndpoint,
    ) -> Result<ServerInfo, CoreError> {
        let body = self.transport.get_text(&endpoint.http_server_info_url())?;
        parse_server_info(&body)
    }

    pub fn fetch_app_list(&self, endpoint: &HostEndpoint) -> Result<Vec<HostApp>, CoreError> {
        self.request_context(endpoint.clone(), HostRequestAuth::None)
            .fetch_app_list()
    }

    pub fn fetch_app_list_with_client_identity(
        &self,
        endpoint: &HostEndpoint,
        certificate_pem: &str,
        private_key_pem: &str,
    ) -> Result<Vec<HostApp>, CoreError> {
        self.request_context(
            endpoint.clone(),
            HostRequestAuth::client_identity("", certificate_pem, private_key_pem),
        )
        .fetch_app_list()
    }

    pub fn fetch_box_art(
        &self,
        endpoint: &HostEndpoint,
        app_id: &str,
    ) -> Result<Vec<u8>, CoreError> {
        self.request_context(endpoint.clone(), HostRequestAuth::None)
            .fetch_box_art(app_id)
    }

    pub fn fetch_box_art_with_client_identity(
        &self,
        endpoint: &HostEndpoint,
        app_id: &str,
        certificate_pem: &str,
        private_key_pem: &str,
    ) -> Result<Vec<u8>, CoreError> {
        self.request_context(
            endpoint.clone(),
            HostRequestAuth::client_identity("", certificate_pem, private_key_pem),
        )
        .fetch_box_art(app_id)
    }

    pub fn launch_app(
        &self,
        endpoint: &HostEndpoint,
        request: &StartAppRequest,
        stream: &gamestream::StreamConfiguration,
    ) -> Result<StartAppSession, CoreError> {
        self.request_context(endpoint.clone(), HostRequestAuth::None)
            .launch_app(request, stream)
    }

    pub fn launch_app_with_client_identity(
        &self,
        endpoint: &HostEndpoint,
        request: &StartAppRequest,
        stream: &gamestream::StreamConfiguration,
        certificate_pem: &str,
        private_key_pem: &str,
    ) -> Result<StartAppSession, CoreError> {
        self.request_context(
            endpoint.clone(),
            HostRequestAuth::client_identity("", certificate_pem, private_key_pem),
        )
        .launch_app(request, stream)
    }

    pub fn resume_app(
        &self,
        endpoint: &HostEndpoint,
        request: &StartAppRequest,
        stream: &gamestream::StreamConfiguration,
    ) -> Result<StartAppSession, CoreError> {
        self.request_context(endpoint.clone(), HostRequestAuth::None)
            .resume_app(request, stream)
    }

    pub fn resume_app_with_client_identity(
        &self,
        endpoint: &HostEndpoint,
        request: &StartAppRequest,
        stream: &gamestream::StreamConfiguration,
        certificate_pem: &str,
        private_key_pem: &str,
    ) -> Result<StartAppSession, CoreError> {
        self.request_context(
            endpoint.clone(),
            HostRequestAuth::client_identity("", certificate_pem, private_key_pem),
        )
        .resume_app(request, stream)
    }

    pub fn quit_app(&self, endpoint: &HostEndpoint) -> Result<(), CoreError> {
        self.request_context(endpoint.clone(), HostRequestAuth::None)
            .quit_app()
    }

    pub fn quit_app_with_client_identity(
        &self,
        endpoint: &HostEndpoint,
        certificate_pem: &str,
        private_key_pem: &str,
    ) -> Result<(), CoreError> {
        self.request_context(
            endpoint.clone(),
            HostRequestAuth::client_identity("", certificate_pem, private_key_pem),
        )
        .quit_app()
    }
}

impl HostRequestAuth {
    pub fn client_identity(unique_id: &str, certificate_pem: &str, private_key_pem: &str) -> Self {
        Self::ClientIdentity {
            unique_id: unique_id.to_string(),
            certificate_pem: certificate_pem.to_string(),
            private_key_pem: private_key_pem.to_string(),
        }
    }
}

impl<T> HostRequestContext<'_, T>
where
    T: HostHttpTransport,
{
    pub fn fetch_server_info(&self) -> Result<ServerInfo, CoreError> {
        let body = self.get_text("serverinfo", &self.endpoint.server_info_url())?;
        parse_server_info(&body)
    }

    pub fn fetch_app_list(&self) -> Result<Vec<HostApp>, CoreError> {
        let body = self.get_text("applist", &self.endpoint.app_list_url())?;
        parse_app_list(&body)
    }

    pub fn fetch_box_art(&self, app_id: &str) -> Result<Vec<u8>, CoreError> {
        let bytes = self.get_bytes("appasset", &self.endpoint.app_asset_url(app_id))?;
        if bytes.is_empty() {
            return Err(CoreError::Backend(format!(
                "Host returned empty box art for app {app_id}."
            )));
        }
        Ok(bytes)
    }

    pub fn launch_app(
        &self,
        request: &StartAppRequest,
        stream: &gamestream::StreamConfiguration,
    ) -> Result<StartAppSession, CoreError> {
        let body = self.get_text(
            "launch",
            &self.endpoint.launch_url(&request.launch_query(stream)),
        )?;
        parse_start_app_response(&body).map_err(|error| {
            logger::log(format!(
                "host request launch response rejected; error={error}; response={}",
                sanitized_response_preview(&body)
            ));
            error
        })
    }

    pub fn resume_app(
        &self,
        request: &StartAppRequest,
        stream: &gamestream::StreamConfiguration,
    ) -> Result<StartAppSession, CoreError> {
        let body = self.get_text(
            "resume",
            &self.endpoint.resume_url(&request.launch_query(stream)),
        )?;
        parse_start_app_response(&body).map_err(|error| {
            logger::log(format!(
                "host request resume response rejected; error={error}; response={}",
                sanitized_response_preview(&body)
            ));
            error
        })
    }

    pub fn quit_app(&self) -> Result<(), CoreError> {
        let body = self.get_text("cancel", &self.endpoint.cancel_url())?;
        verify_response_status(&body).map_err(|error| {
            logger::log(format!(
                "host request cancel response rejected; error={error}; response={}",
                sanitized_response_preview(&body)
            ));
            error
        })
    }

    fn get_text(&self, action: &str, url: &str) -> Result<String, CoreError> {
        let url = self.url_for_request(url);
        logger::log(format!(
            "host request {action} begin; auth={}; url={}",
            self.auth_label(),
            sanitized_url(&url)
        ));
        let result = match &self.auth {
            HostRequestAuth::None => self.client.transport.get_text(&url),
            HostRequestAuth::ClientIdentity {
                unique_id: _,
                certificate_pem,
                private_key_pem,
            } => self.client.transport.get_text_with_client_identity(
                &url,
                certificate_pem,
                private_key_pem,
            ),
        };
        match &result {
            Ok(body) => logger::log(format!(
                "host request {action} complete; response={}",
                sanitized_response_preview(body)
            )),
            Err(error) => logger::log(format!("host request {action} failed; error={error}")),
        }
        result
    }

    fn get_bytes(&self, action: &str, url: &str) -> Result<Vec<u8>, CoreError> {
        let url = self.url_for_request(url);
        logger::log(format!(
            "host request {action} begin; auth={}; url={}",
            self.auth_label(),
            sanitized_url(&url)
        ));
        let result = match &self.auth {
            HostRequestAuth::None => self.client.transport.get_bytes(&url),
            HostRequestAuth::ClientIdentity {
                unique_id: _,
                certificate_pem,
                private_key_pem,
            } => self.client.transport.get_bytes_with_client_identity(
                &url,
                certificate_pem,
                private_key_pem,
            ),
        };
        match &result {
            Ok(bytes) => logger::log(format!(
                "host request {action} complete; bytes={}",
                bytes.len()
            )),
            Err(error) => logger::log(format!("host request {action} failed; error={error}")),
        }
        result
    }

    fn url_for_request(&self, url: &str) -> String {
        match &self.auth {
            HostRequestAuth::None => url.to_string(),
            HostRequestAuth::ClientIdentity { unique_id, .. } => {
                if unique_id.trim().is_empty() {
                    url.to_string()
                } else {
                    with_native_request_prefix(url, unique_id)
                }
            }
        }
    }

    fn auth_label(&self) -> &'static str {
        match &self.auth {
            HostRequestAuth::None => "none",
            HostRequestAuth::ClientIdentity { .. } => "client-identity",
        }
    }
}

fn with_native_request_prefix(url: &str, unique_id: &str) -> String {
    let prefix = format!(
        "uniqueid={}&uuid={}",
        unique_id.trim(),
        random_request_uuid()
    );
    if let Some((base, query)) = url.split_once('?') {
        format!("{base}?{prefix}&{query}")
    } else {
        format!("{url}?{prefix}")
    }
}

fn random_request_uuid() -> String {
    let mut bytes = [0_u8; 16];
    OsRng.fill_bytes(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn sanitized_url(url: &str) -> String {
    sanitize_query_value(sanitize_query_value(url.to_string(), "rikey"), "rikeyid")
}

fn sanitize_query_value(mut value: String, key: &str) -> String {
    let needle = format!("{key}=");
    let mut search_from = 0;
    while let Some(relative_start) = value[search_from..].find(&needle) {
        let value_start = search_from + relative_start + needle.len();
        let value_end = value[value_start..]
            .find('&')
            .map(|relative_end| value_start + relative_end)
            .unwrap_or(value.len());
        value.replace_range(value_start..value_end, "REDACTED");
        search_from = value_start + "REDACTED".len();
    }
    value
}

fn sanitized_response_preview(response: &str) -> String {
    let compact = response.split_whitespace().collect::<Vec<_>>().join(" ");
    let preview: String = compact.chars().take(512).collect();
    sanitized_url(&preview)
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
    let state = optional_tag(xml, "state");
    let current_game_id = if state.ends_with("_SERVER_BUSY") {
        optional_tag(xml, "currentgame").parse::<i32>().unwrap_or(0)
    } else {
        0
    };

    Ok(ServerInfo {
        hostname: optional_tag(xml, "hostname"),
        app_version: optional_tag(xml, "appversion"),
        gfe_version: optional_tag(xml, "GfeVersion"),
        unique_id: optional_tag(xml, "uniqueid"),
        mac_address: normalized_mac_address(&optional_tag(xml, "mac")),
        local_ip: usable_local_ip(&optional_tag(xml, "LocalIP")),
        external_ip: optional_tag(xml, "ExternalIP"),
        external_port: optional_tag(xml, "ExternalPort")
            .parse::<u16>()
            .unwrap_or(DEFAULT_HTTP_PORT),
        https_port: optional_tag(xml, "HttpsPort")
            .parse::<u16>()
            .unwrap_or(DEFAULT_HTTPS_PORT),
        state,
        current_game_id,
        pair_status: optional_tag(xml, "PairStatus"),
        server_codec_mode_support: optional_tag(xml, "ServerCodecModeSupport")
            .parse::<i32>()
            .unwrap_or(0),
        gpu_model: optional_tag(xml, "gputype"),
    })
}

fn usable_local_ip(value: &str) -> String {
    if value.starts_with("127.") {
        String::new()
    } else {
        value.to_string()
    }
}

fn normalized_mac_address(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "00:00:00:00:00:00" {
        String::new()
    } else {
        trimmed.to_string()
    }
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
            app_collector_game: optional_tag(block, "IsAppCollectorGame") == "1",
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
        parse_app_list, parse_server_info, parse_start_app_response, with_native_request_prefix,
        HostApp, HostEndpoint, HostHttpClient, HostHttpTransport, HostRequestAuth, StartAppRequest,
    };
    use crate::core::error::CoreError;
    use crate::core::gamestream::{AudioConfiguration, RemoteInputCrypto, StreamConfiguration};
    use crate::core::gamestream_sys;
    use std::cell::RefCell;

    #[derive(Default)]
    struct FakeTransport {
        requests: RefCell<Vec<String>>,
        identity_requests: RefCell<Vec<String>>,
    }

    impl HostHttpTransport for FakeTransport {
        fn get_text(&self, url: &str) -> Result<String, CoreError> {
            self.requests.borrow_mut().push(url.to_string());
            if url.contains("/serverinfo") {
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

        fn get_bytes(&self, url: &str) -> Result<Vec<u8>, CoreError> {
            self.requests.borrow_mut().push(url.to_string());
            Ok(vec![0x89, b'P', b'N', b'G'])
        }

        fn get_text_with_client_identity(
            &self,
            url: &str,
            _certificate_pem: &str,
            _private_key_pem: &str,
        ) -> Result<String, CoreError> {
            self.identity_requests.borrow_mut().push(url.to_string());
            self.get_text(url)
        }

        fn get_bytes_with_client_identity(
            &self,
            url: &str,
            _certificate_pem: &str,
            _private_key_pem: &str,
        ) -> Result<Vec<u8>, CoreError> {
            self.identity_requests.borrow_mut().push(url.to_string());
            self.get_bytes(url)
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
            "https://192.168.1.20:47984/appasset?appid=123&AssetType=2&AssetIdx=0",
            endpoint.app_asset_url("123")
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
    fn paired_request_context_uses_client_identity_for_server_info() {
        let transport = FakeTransport::default();
        let client = HostHttpClient::new(transport);
        let endpoint = HostEndpoint::from_address("sunshine.local").unwrap();
        let context = client.request_context(
            endpoint,
            HostRequestAuth::client_identity("client123", "cert", "key"),
        );

        let info = context.fetch_server_info().unwrap();

        assert_eq!("Sunshine", info.app_version);
        let requests = client.transport.identity_requests.into_inner();
        assert_eq!(1, requests.len());
        assert!(requests[0]
            .starts_with("https://sunshine.local:47984/serverinfo?uniqueid=client123&uuid="));
    }

    #[test]
    fn authenticated_request_prefix_preserves_existing_query() {
        let url = with_native_request_prefix(
            "https://sunshine.local:47984/launch?appid=7&mode=1920x1080x60",
            "client123",
        );

        assert!(url.starts_with("https://sunshine.local:47984/launch?uniqueid=client123&uuid="));
        assert!(url.contains("&appid=7&mode=1920x1080x60"));
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
    fn paired_app_list_uses_client_identity_transport() {
        let transport = FakeTransport::default();
        let client = HostHttpClient::new(transport);
        let endpoint = HostEndpoint::from_address("sunshine.local").unwrap();

        let apps = client
            .fetch_app_list_with_client_identity(&endpoint, "cert", "key")
            .unwrap();

        assert_eq!("Desktop", apps[0].name);
        assert_eq!(
            vec!["https://sunshine.local:47984/applist"],
            client.transport.identity_requests.into_inner()
        );
    }

    #[test]
    fn parses_server_info_fixture() {
        let info = parse_server_info(
            r#"
            <root>
                <hostname>DESKTOP-1234</hostname>
                <appversion>Sunshine v0.23.1</appversion>
                <GfeVersion>3.23</GfeVersion>
                <state>MJOLNIR_SERVER_BUSY</state>
                <uniqueid>abc123</uniqueid>
                <mac>00:11:22:33:44:55</mac>
                <LocalIP>192.168.1.20</LocalIP>
                <ExternalIP>203.0.113.4</ExternalIP>
                <ExternalPort>48000</ExternalPort>
                <HttpsPort>47985</HttpsPort>
                <currentgame>12345</currentgame>
                <PairStatus>1</PairStatus>
                <ServerCodecModeSupport>65535</ServerCodecModeSupport>
                <gputype>NVIDIA RTX</gputype>
            </root>
            "#,
        )
        .unwrap();

        assert_eq!("DESKTOP-1234", info.hostname);
        assert_eq!("Sunshine v0.23.1", info.app_version);
        assert_eq!("MJOLNIR_SERVER_BUSY", info.state);
        assert_eq!("abc123", info.unique_id);
        assert_eq!("00:11:22:33:44:55", info.mac_address);
        assert_eq!("192.168.1.20", info.local_ip);
        assert_eq!("203.0.113.4", info.external_ip);
        assert_eq!(48000, info.external_port);
        assert_eq!(47985, info.https_port);
        assert_eq!(12345, info.current_game_id);
        assert_eq!(65535, info.server_codec_mode_support);
        assert_eq!("NVIDIA RTX", info.gpu_model);
    }

    #[test]
    fn parses_server_info_ignores_stale_current_game_when_idle() {
        let info = parse_server_info(
            r#"
            <root>
                <state>SERVER_AVAILABLE</state>
                <currentgame>12345</currentgame>
                <mac>00:00:00:00:00:00</mac>
                <LocalIP>127.0.0.1</LocalIP>
            </root>
            "#,
        )
        .unwrap();

        assert_eq!(0, info.current_game_id);
        assert!(info.mac_address.is_empty());
        assert!(info.local_ip.is_empty());
        assert_eq!(47984, info.https_port);
    }

    #[test]
    fn parses_app_list_fixture() {
        let apps = parse_app_list(
            r#"
            <root>
                <App><ID>1</ID><AppTitle>Desktop</AppTitle></App>
                <App><ID>2</ID><AppTitle>Steam Big Picture</AppTitle><IsAppCollectorGame>1</IsAppCollectorGame></App>
            </root>
            "#,
        )
        .unwrap();

        assert_eq!(2, apps.len());
        assert_eq!("1", apps[0].id);
        assert_eq!("Steam Big Picture", apps[1].name);
        assert!(apps[1].app_collector_game);
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
    fn client_fetches_box_art_bytes() {
        let transport = FakeTransport::default();
        let client = HostHttpClient::new(transport);
        let endpoint = HostEndpoint::from_address("sunshine.local").unwrap();

        let bytes = client.fetch_box_art(&endpoint, "7").unwrap();

        assert_eq!(vec![0x89, b'P', b'N', b'G'], bytes);
        assert_eq!(
            vec!["https://sunshine.local:47984/appasset?appid=7&AssetType=2&AssetIdx=0"],
            client.transport.requests.into_inner()
        );
    }

    #[test]
    fn paired_box_art_uses_client_identity_transport() {
        let transport = FakeTransport::default();
        let client = HostHttpClient::new(transport);
        let endpoint = HostEndpoint::from_address("sunshine.local").unwrap();

        let bytes = client
            .fetch_box_art_with_client_identity(&endpoint, "7", "cert", "key")
            .unwrap();

        assert_eq!(vec![0x89, b'P', b'N', b'G'], bytes);
        assert_eq!(
            vec!["https://sunshine.local:47984/appasset?appid=7&AssetType=2&AssetIdx=0"],
            client.transport.identity_requests.into_inner()
        );
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
