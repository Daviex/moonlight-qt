#![allow(dead_code)]

use super::error::CoreError;
use super::gamestream::{
    RawSessionConfiguration, RemoteInputCrypto, ServerConnectionConfiguration, StreamConfiguration,
};
use super::host_http::{ServerInfo, StartAppSession};
use super::host_store::StoredHost;
use super::stream_window::StreamWindowDescriptor;
use super::types::{AppEntry, StreamingSettings};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamLaunchPlan {
    pub host_id: String,
    pub host_address: String,
    pub app_id: String,
    pub app_name: String,
    pub server_certificate_pem: String,
    pub stream_config: StreamConfiguration,
    pub window: StreamWindowDescriptor,
}

#[derive(Debug)]
pub struct PreparedStreamSession {
    pub host_id: String,
    pub app_id: String,
    pub server: ServerConnectionConfiguration,
    pub raw: RawSessionConfiguration,
}

impl StreamLaunchPlan {
    pub fn new(
        host: &StoredHost,
        app: &AppEntry,
        settings: &StreamingSettings,
    ) -> Result<Self, CoreError> {
        if !host.paired {
            return Err(CoreError::Validation(format!(
                "{} must be paired before launching apps.",
                host.name
            )));
        }
        if host.server_certificate_pem.trim().is_empty() {
            return Err(CoreError::Validation(format!(
                "{} is missing its paired server certificate.",
                host.name
            )));
        }
        if app.id.trim().is_empty() {
            return Err(CoreError::Validation(
                "App ID is required to start a stream.".into(),
            ));
        }

        Ok(Self {
            host_id: host.id.clone(),
            host_address: host.manual_address.clone(),
            app_id: app.id.clone(),
            app_name: app.name.clone(),
            server_certificate_pem: host.server_certificate_pem.clone(),
            stream_config: StreamConfiguration::from(settings)
                .with_remote_input_crypto(RemoteInputCrypto::generate()),
            window: StreamWindowDescriptor::new(&host.name, settings)?,
        })
    }

    pub fn prepare_session(
        &self,
        server_info: &ServerInfo,
        start_session: &StartAppSession,
    ) -> Result<PreparedStreamSession, CoreError> {
        let server = ServerConnectionConfiguration {
            address: self.host_address.clone(),
            app_version: server_info.app_version.clone(),
            gfe_version: optional_string(server_info.gfe_version.clone()),
            rtsp_session_url: start_session.rtsp_session_url.clone(),
            codec_mode_support: server_info.server_codec_mode_support,
        };
        let raw = RawSessionConfiguration::new(&server, &self.stream_config)?;

        Ok(PreparedStreamSession {
            host_id: self.host_id.clone(),
            app_id: self.app_id.clone(),
            server,
            raw,
        })
    }
}

fn optional_string(value: String) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::StreamLaunchPlan;
    use crate::core::gamestream_sys;
    use crate::core::host_http::{ServerInfo, StartAppSession};
    use crate::core::host_store::StoredHost;
    use crate::core::settings::default_streaming_settings;
    use crate::core::types::AppEntry;

    fn host() -> StoredHost {
        StoredHost {
            id: "gaming-pc".into(),
            name: "Gaming PC".into(),
            manual_address: "192.168.1.20".into(),
            uuid: "uuid".into(),
            paired: true,
            mac_address: String::new(),
            server_certificate_pem:
                "-----BEGIN CERTIFICATE-----\npaired\n-----END CERTIFICATE-----".into(),
        }
    }

    fn app() -> AppEntry {
        AppEntry {
            id: "123".into(),
            name: "Desktop".into(),
            box_art_url: String::new(),
            hidden: false,
            direct_launch: false,
            running: false,
            app_collector_game: false,
        }
    }

    #[test]
    fn launch_plan_converts_settings_to_stream_config() {
        let mut settings = default_streaming_settings();
        settings.width = 2560;
        settings.height = 1440;
        settings.fps = 120;
        settings.bitrate_kbps = 60_000;
        settings.audio_config = gamestream_sys::AUDIO_CONFIGURATION_51_SURROUND;
        settings.video_codec_config =
            gamestream_sys::VIDEO_FORMAT_H265 | gamestream_sys::VIDEO_FORMAT_H265_MAIN10;

        let plan = StreamLaunchPlan::new(&host(), &app(), &settings).unwrap();
        let raw = plan.stream_config.to_raw();

        assert_eq!("gaming-pc", plan.host_id);
        assert_eq!(2560, plan.window.width);
        assert_eq!(1440, plan.window.height);
        assert_eq!(2560, raw.width);
        assert_eq!(1440, raw.height);
        assert_eq!(120, raw.fps);
        assert_eq!(60_000, raw.bitrate);
        assert_eq!(
            gamestream_sys::AUDIO_CONFIGURATION_51_SURROUND,
            raw.audio_configuration
        );
        assert_eq!(
            gamestream_sys::VIDEO_FORMAT_H265 | gamestream_sys::VIDEO_FORMAT_H265_MAIN10,
            raw.supported_video_formats
        );
    }

    #[test]
    fn launch_plan_requires_pairing_certificate() {
        let mut host = host();
        host.server_certificate_pem.clear();

        let error =
            StreamLaunchPlan::new(&host, &app(), &default_streaming_settings()).unwrap_err();

        assert_eq!(
            "Gaming PC is missing its paired server certificate.",
            error.to_string()
        );
    }

    #[test]
    fn launch_plan_prepares_raw_session_inputs_after_http_launch() {
        let plan = StreamLaunchPlan::new(&host(), &app(), &default_streaming_settings()).unwrap();
        let server_info = ServerInfo {
            app_version: "Sunshine".into(),
            gfe_version: "3.0".into(),
            state: String::new(),
            unique_id: "uuid".into(),
            current_game_id: 0,
            pair_status: "1".into(),
            server_codec_mode_support: 0x101,
        };
        let start_session = StartAppSession {
            rtsp_session_url: Some("rtsp://session".into()),
        };

        let prepared = plan.prepare_session(&server_info, &start_session).unwrap();

        assert_eq!("gaming-pc", prepared.host_id);
        assert_eq!("123", prepared.app_id);
        assert_eq!("192.168.1.20", prepared.server.address);
        assert_eq!(
            Some("rtsp://session".into()),
            prepared.server.rtsp_session_url
        );
        assert_eq!(0x101, prepared.raw.server_info().server_codec_mode_support);
        assert_eq!(1920, prepared.raw.stream_config().width);
    }
}
