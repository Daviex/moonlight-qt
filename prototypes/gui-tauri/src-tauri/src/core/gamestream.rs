#![allow(dead_code)]

use super::error::CoreError;
use super::gamestream_sys;
use std::ffi::CString;
use std::os::raw::{c_char, c_int};

const DEFAULT_VIDEO_PACKET_SIZE: u32 = 1392;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioConfiguration {
    Stereo,
    Surround51,
    Surround71,
}

impl AudioConfiguration {
    pub fn from_raw(value: c_int) -> Self {
        match value {
            gamestream_sys::AUDIO_CONFIGURATION_51_SURROUND => Self::Surround51,
            gamestream_sys::AUDIO_CONFIGURATION_71_SURROUND => Self::Surround71,
            _ => Self::Stereo,
        }
    }

    fn as_raw(self) -> c_int {
        match self {
            Self::Stereo => gamestream_sys::AUDIO_CONFIGURATION_STEREO,
            Self::Surround51 => gamestream_sys::AUDIO_CONFIGURATION_51_SURROUND,
            Self::Surround71 => gamestream_sys::AUDIO_CONFIGURATION_71_SURROUND,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamingRemotely {
    Local,
    Remote,
    Auto,
}

impl StreamingRemotely {
    fn as_raw(self) -> c_int {
        match self {
            Self::Local => gamestream_sys::STREAM_CFG_LOCAL,
            Self::Remote => gamestream_sys::STREAM_CFG_REMOTE,
            Self::Auto => gamestream_sys::STREAM_CFG_AUTO,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamConfiguration {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub packet_size: u32,
    pub streaming_remotely: StreamingRemotely,
    pub audio_configuration: AudioConfiguration,
    pub supported_video_formats: c_int,
    pub remote_input_crypto: RemoteInputCrypto,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RemoteInputCrypto {
    pub aes_key: [u8; 16],
    pub aes_iv: [u8; 16],
}

impl RemoteInputCrypto {
    pub fn generate() -> Self {
        use rand::RngCore;

        let mut crypto = Self::default();
        let mut rng = rand::rngs::OsRng;
        rng.fill_bytes(&mut crypto.aes_key);
        rng.fill_bytes(&mut crypto.aes_iv[..4]);
        crypto
    }
}

#[derive(Clone, Debug, Default)]
pub struct StreamCallbacks {
    pub connection: gamestream_sys::ConnectionListenerCallbacks,
    pub video: gamestream_sys::DecoderRendererCallbacks,
    pub audio: gamestream_sys::AudioRendererCallbacks,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerConnectionConfiguration {
    pub address: String,
    pub app_version: String,
    pub gfe_version: Option<String>,
    pub rtsp_session_url: Option<String>,
    pub codec_mode_support: c_int,
}

#[derive(Debug)]
pub struct RawSessionConfiguration {
    server_info: gamestream_sys::ServerInformation,
    stream_config: gamestream_sys::StreamConfiguration,
    _address: CString,
    _app_version: CString,
    _gfe_version: Option<CString>,
    _rtsp_session_url: Option<CString>,
}

impl StreamCallbacks {
    pub fn as_raw_parts(
        &mut self,
    ) -> (
        *mut gamestream_sys::ConnectionListenerCallbacks,
        *mut gamestream_sys::DecoderRendererCallbacks,
        *mut gamestream_sys::AudioRendererCallbacks,
    ) {
        (&mut self.connection, &mut self.video, &mut self.audio)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GameStreamRunner;

impl GameStreamRunner {
    pub fn start(
        &self,
        session: &mut RawSessionConfiguration,
        callbacks: &mut StreamCallbacks,
    ) -> Result<(), CoreError> {
        start_gamestream_session(session, callbacks)
    }

    pub fn stop(&self) {
        stop_gamestream_session();
    }

    pub fn interrupt(&self) {
        interrupt_gamestream_session();
    }
}

impl StreamConfiguration {
    pub fn with_remote_input_crypto(mut self, crypto: RemoteInputCrypto) -> Self {
        self.remote_input_crypto = crypto;
        self
    }

    pub fn to_raw(&self) -> gamestream_sys::StreamConfiguration {
        gamestream_sys::StreamConfiguration {
            width: saturated_c_int(self.width),
            height: saturated_c_int(self.height),
            fps: saturated_c_int(self.fps),
            bitrate: saturated_c_int(self.bitrate_kbps),
            packet_size: saturated_c_int(self.packet_size),
            streaming_remotely: self.streaming_remotely.as_raw(),
            audio_configuration: self.audio_configuration.as_raw(),
            supported_video_formats: self.supported_video_formats,
            remote_input_aes_key: bytes_to_c_chars(self.remote_input_crypto.aes_key),
            remote_input_aes_iv: bytes_to_c_chars(self.remote_input_crypto.aes_iv),
            ..gamestream_sys::StreamConfiguration::default()
        }
    }
}

impl From<&crate::core::types::StreamingSettings> for StreamConfiguration {
    fn from(settings: &crate::core::types::StreamingSettings) -> Self {
        let packet_size = if settings.packet_size == 0 {
            DEFAULT_VIDEO_PACKET_SIZE
        } else {
            settings.packet_size
        };
        let streaming_remotely = if settings.packet_size == 0 {
            StreamingRemotely::Auto
        } else {
            StreamingRemotely::Local
        };

        Self {
            width: settings.width,
            height: settings.height,
            fps: settings.fps,
            bitrate_kbps: settings.bitrate_kbps,
            packet_size,
            streaming_remotely,
            audio_configuration: AudioConfiguration::from_raw(settings.audio_config),
            supported_video_formats: if settings.video_codec_config == 0 {
                gamestream_sys::VIDEO_FORMAT_H264
            } else {
                settings.video_codec_config
            },
            remote_input_crypto: RemoteInputCrypto::default(),
        }
    }
}

impl RawSessionConfiguration {
    pub fn new(
        server: &ServerConnectionConfiguration,
        stream: &StreamConfiguration,
    ) -> Result<Self, CoreError> {
        let address = c_string_field("server address", &server.address)?;
        let app_version = c_string_field("server app version", &server.app_version)?;
        let gfe_version = optional_c_string_field("server GFE version", &server.gfe_version)?;
        let rtsp_session_url =
            optional_c_string_field("RTSP session URL", &server.rtsp_session_url)?;

        let server_info = gamestream_sys::ServerInformation {
            address: address.as_ptr(),
            server_info_app_version: app_version.as_ptr(),
            server_info_gfe_version: gfe_version
                .as_ref()
                .map(|value| value.as_ptr())
                .unwrap_or(std::ptr::null()),
            rtsp_session_url: rtsp_session_url
                .as_ref()
                .map(|value| value.as_ptr())
                .unwrap_or(std::ptr::null()),
            server_codec_mode_support: server.codec_mode_support,
        };

        Ok(Self {
            server_info,
            stream_config: stream.to_raw(),
            _address: address,
            _app_version: app_version,
            _gfe_version: gfe_version,
            _rtsp_session_url: rtsp_session_url,
        })
    }

    pub fn server_info(&self) -> &gamestream_sys::ServerInformation {
        &self.server_info
    }

    pub fn stream_config(&self) -> &gamestream_sys::StreamConfiguration {
        &self.stream_config
    }

    fn server_info_mut(&mut self) -> *mut gamestream_sys::ServerInformation {
        &mut self.server_info
    }

    fn stream_config_mut(&mut self) -> *mut gamestream_sys::StreamConfiguration {
        &mut self.stream_config
    }
}

#[cfg(moonlight_common_c_linked)]
fn start_gamestream_session(
    session: &mut RawSessionConfiguration,
    callbacks: &mut StreamCallbacks,
) -> Result<(), CoreError> {
    let (connection_callbacks, video_callbacks, audio_callbacks) = callbacks.as_raw_parts();
    // SAFETY: RawSessionConfiguration owns the C strings referenced by SERVER_INFORMATION,
    // and StreamCallbacks owns the callback structs passed to Limelight for this call.
    // The caller must keep both values alive until LiStartConnection returns.
    let result = unsafe {
        gamestream_sys::LiStartConnection(
            session.server_info_mut(),
            session.stream_config_mut(),
            connection_callbacks,
            video_callbacks,
            audio_callbacks,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            0,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(CoreError::Backend(format!(
            "GameStream connection failed with code {result}."
        )))
    }
}

#[cfg(not(moonlight_common_c_linked))]
fn start_gamestream_session(
    _session: &mut RawSessionConfiguration,
    _callbacks: &mut StreamCallbacks,
) -> Result<(), CoreError> {
    Err(CoreError::Backend(
        "C GameStream library is not linked. Set MOONLIGHT_COMMON_C_LIB_DIR to enable the stream runner.".into(),
    ))
}

#[cfg(moonlight_common_c_linked)]
fn stop_gamestream_session() {
    // SAFETY: Limelight exposes LiStopConnection as a process-global stop hook.
    unsafe {
        gamestream_sys::LiStopConnection();
    }
}

#[cfg(not(moonlight_common_c_linked))]
fn stop_gamestream_session() {}

#[cfg(moonlight_common_c_linked)]
fn interrupt_gamestream_session() {
    // SAFETY: Limelight exposes LiInterruptConnection as a process-global interrupt hook.
    unsafe {
        gamestream_sys::LiInterruptConnection();
    }
}

#[cfg(not(moonlight_common_c_linked))]
fn interrupt_gamestream_session() {}

fn c_string_field(field_name: &str, value: &str) -> Result<CString, CoreError> {
    CString::new(value).map_err(|_| {
        CoreError::Validation(format!("{field_name} cannot contain embedded NUL bytes."))
    })
}

fn optional_c_string_field(
    field_name: &str,
    value: &Option<String>,
) -> Result<Option<CString>, CoreError> {
    value
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(|value| c_string_field(field_name, value))
        .transpose()
}

impl Default for StreamConfiguration {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 60,
            bitrate_kbps: 20_000,
            packet_size: 0,
            streaming_remotely: StreamingRemotely::Auto,
            audio_configuration: AudioConfiguration::Stereo,
            supported_video_formats: gamestream_sys::VIDEO_FORMAT_H264,
            remote_input_crypto: RemoteInputCrypto::default(),
        }
    }
}

fn saturated_c_int(value: u32) -> c_int {
    value.min(c_int::MAX as u32) as c_int
}

fn bytes_to_c_chars(bytes: [u8; 16]) -> [c_char; 16] {
    bytes.map(|value| value as c_char)
}

#[cfg(test)]
mod tests {
    use super::{
        AudioConfiguration, RawSessionConfiguration, RemoteInputCrypto,
        ServerConnectionConfiguration, StreamConfiguration, StreamingRemotely,
    };
    use crate::core::gamestream_sys;
    use crate::core::settings::default_streaming_settings;
    use std::ffi::CStr;
    use std::os::raw::c_int;

    #[test]
    fn stream_configuration_maps_to_raw_c_layout_values() {
        let config = StreamConfiguration {
            width: 3840,
            height: 2160,
            fps: 120,
            bitrate_kbps: 80_000,
            packet_size: 1024,
            streaming_remotely: StreamingRemotely::Remote,
            audio_configuration: AudioConfiguration::Surround71,
            supported_video_formats: gamestream_sys::VIDEO_FORMAT_H265
                | gamestream_sys::VIDEO_FORMAT_H265_MAIN10,
            remote_input_crypto: RemoteInputCrypto {
                aes_key: [0xF1; 16],
                aes_iv: [0x22; 16],
            },
        };

        let raw = config.to_raw();

        assert_eq!(3840, raw.width);
        assert_eq!(2160, raw.height);
        assert_eq!(120, raw.fps);
        assert_eq!(80_000, raw.bitrate);
        assert_eq!(1024, raw.packet_size);
        assert_eq!(gamestream_sys::STREAM_CFG_REMOTE, raw.streaming_remotely);
        assert_eq!(
            gamestream_sys::AUDIO_CONFIGURATION_71_SURROUND,
            raw.audio_configuration
        );
        assert_eq!(
            gamestream_sys::VIDEO_FORMAT_H265 | gamestream_sys::VIDEO_FORMAT_H265_MAIN10,
            raw.supported_video_formats
        );
        assert_eq!(0xF1, raw.remote_input_aes_key[0] as u8);
        assert_eq!(0x22, raw.remote_input_aes_iv[0] as u8);
    }

    #[test]
    fn stream_configuration_saturates_values_that_exceed_c_int() {
        let config = StreamConfiguration {
            width: u32::MAX,
            ..StreamConfiguration::default()
        };

        assert_eq!(c_int::MAX, config.to_raw().width);
    }

    #[test]
    fn streaming_settings_conversion_uses_native_packet_size_defaults() {
        let mut settings = default_streaming_settings();
        settings.packet_size = 0;

        let raw = StreamConfiguration::from(&settings).to_raw();

        assert_eq!(1392, raw.packet_size);
        assert_eq!(gamestream_sys::STREAM_CFG_AUTO, raw.streaming_remotely);

        settings.packet_size = 1024;
        let raw = StreamConfiguration::from(&settings).to_raw();

        assert_eq!(1024, raw.packet_size);
        assert_eq!(gamestream_sys::STREAM_CFG_LOCAL, raw.streaming_remotely);
    }

    #[test]
    fn remote_input_crypto_generation_populates_key_and_first_iv_word() {
        let crypto = RemoteInputCrypto::generate();

        assert_ne!([0; 16], crypto.aes_key);
        assert!(crypto.aes_iv[..4].iter().any(|value| *value != 0));
        assert!(crypto.aes_iv[4..].iter().all(|value| *value == 0));
    }

    #[test]
    fn raw_session_configuration_owns_c_string_inputs() {
        let server = ServerConnectionConfiguration {
            address: "192.168.1.20".into(),
            app_version: "99.1.1.1".into(),
            gfe_version: Some("Sunshine".into()),
            rtsp_session_url: Some("rtsp://session".into()),
            codec_mode_support: 0x101,
        };
        let stream = StreamConfiguration::default();

        let raw = RawSessionConfiguration::new(&server, &stream).unwrap();

        let address = unsafe { CStr::from_ptr(raw.server_info().address) };
        let gfe_version = unsafe { CStr::from_ptr(raw.server_info().server_info_gfe_version) };
        assert_eq!("192.168.1.20", address.to_str().unwrap());
        assert_eq!("Sunshine", gfe_version.to_str().unwrap());
        assert_eq!(0x101, raw.server_info().server_codec_mode_support);
        assert_eq!(1920, raw.stream_config().width);
    }

    #[test]
    fn raw_session_configuration_rejects_embedded_nul_bytes() {
        let server = ServerConnectionConfiguration {
            address: "192.168.1.20\0bad".into(),
            app_version: "99.1.1.1".into(),
            gfe_version: None,
            rtsp_session_url: None,
            codec_mode_support: 1,
        };

        let error = RawSessionConfiguration::new(&server, &StreamConfiguration::default())
            .unwrap_err()
            .to_string();

        assert_eq!("server address cannot contain embedded NUL bytes.", error);
    }

    #[cfg(not(moonlight_common_c_linked))]
    #[test]
    fn gamestream_runner_reports_unlinked_library() {
        let server = ServerConnectionConfiguration {
            address: "192.168.1.20".into(),
            app_version: "Sunshine".into(),
            gfe_version: None,
            rtsp_session_url: None,
            codec_mode_support: gamestream_sys::VIDEO_FORMAT_H264,
        };
        let mut session =
            RawSessionConfiguration::new(&server, &StreamConfiguration::default()).unwrap();
        let mut callbacks = super::StreamCallbacks::default();

        let error = super::GameStreamRunner
            .start(&mut session, &mut callbacks)
            .unwrap_err();

        assert_eq!(
            "C GameStream library is not linked. Set MOONLIGHT_COMMON_C_LIB_DIR to enable the stream runner.",
            error.to_string()
        );
    }

    #[test]
    fn all_audio_and_remote_variants_have_raw_values() {
        assert_eq!(
            gamestream_sys::AUDIO_CONFIGURATION_51_SURROUND,
            AudioConfiguration::Surround51.as_raw()
        );
        assert_eq!(
            AudioConfiguration::Surround71,
            AudioConfiguration::from_raw(gamestream_sys::AUDIO_CONFIGURATION_71_SURROUND)
        );
        assert_eq!(
            gamestream_sys::STREAM_CFG_LOCAL,
            StreamingRemotely::Local.as_raw()
        );
    }

    #[test]
    fn stream_callbacks_expose_mutable_raw_parts() {
        let mut callbacks = super::StreamCallbacks::default();
        let (connection, video, audio) = callbacks.as_raw_parts();

        assert!(!connection.is_null());
        assert!(!video.is_null());
        assert!(!audio.is_null());
    }
}
