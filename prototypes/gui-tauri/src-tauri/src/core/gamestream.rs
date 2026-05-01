#![allow(dead_code)]

use super::error::CoreError;
use super::events::{BridgeEvent, BridgeEventKind};
use super::gamestream_sys;
use serde::{Deserialize, Serialize};
#[cfg(moonlight_common_c_linked)]
use std::ffi::CStr;
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};

const DEFAULT_VIDEO_PACKET_SIZE: u32 = 1392;

static STREAM_EVENT_CONTEXT: OnceLock<Mutex<Option<StreamEventContext>>> = OnceLock::new();
static AUDIO_SINK_STATE: OnceLock<Mutex<AudioSinkState>> = OnceLock::new();
static VIDEO_SINK_STATE: OnceLock<Mutex<VideoSinkState>> = OnceLock::new();

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

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamMediaStats {
    pub video_started: bool,
    pub video_frames: u64,
    pub video_bytes: u64,
    pub last_video_frame_number: c_int,
    pub audio_started: bool,
    pub audio_packets: u64,
    pub audio_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamOutputMode {
    Headless,
}

#[derive(Clone, Debug)]
struct StreamEventContext {
    sender: Sender<BridgeEvent>,
    host_id: String,
    app_id: String,
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

// SAFETY: Raw pointers in ServerInformation and StreamConfiguration point into CString
// buffers and fixed arrays owned by this struct. Moving the struct to the stream
// runner thread does not invalidate those heap allocations or inline arrays.
unsafe impl Send for RawSessionConfiguration {}

impl StreamCallbacks {
    pub fn connection_lifecycle() -> Self {
        Self::connection_lifecycle_for_output(StreamOutputMode::Headless)
    }

    pub fn connection_lifecycle_for_output(output_mode: StreamOutputMode) -> Self {
        let mut callbacks = Self::default();
        callbacks.connection.stage_starting = Some(connection_stage_starting);
        callbacks.connection.stage_complete = Some(connection_stage_complete);
        callbacks.connection.stage_failed = Some(connection_stage_failed);
        callbacks.connection.connection_started = Some(connection_started);
        callbacks.connection.connection_terminated = Some(connection_terminated);
        callbacks.connection.connection_status_update = Some(connection_status_update);
        let output_callbacks = output_mode.callbacks();
        callbacks.video = output_callbacks.video;
        callbacks.audio = output_callbacks.audio;
        callbacks
    }

    pub fn connection_lifecycle_with_events(
        sender: Sender<BridgeEvent>,
        host_id: impl Into<String>,
        app_id: impl Into<String>,
    ) -> Self {
        set_stream_event_context(StreamEventContext {
            sender,
            host_id: host_id.into(),
            app_id: app_id.into(),
        });
        Self::connection_lifecycle()
    }

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

#[derive(Clone, Debug)]
struct StreamOutputCallbacks {
    video: gamestream_sys::DecoderRendererCallbacks,
    audio: gamestream_sys::AudioRendererCallbacks,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AudioSinkConfiguration {
    audio_configuration: c_int,
    opus_config: gamestream_sys::OpusMultistreamConfiguration,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct AudioSinkState {
    configuration: Option<AudioSinkConfiguration>,
    started: bool,
    samples_received: u64,
    bytes_received: u64,
    last_packet: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VideoSinkConfiguration {
    video_format: c_int,
    width: c_int,
    height: c_int,
    redraw_rate: c_int,
    flags: c_int,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct VideoSinkState {
    configuration: Option<VideoSinkConfiguration>,
    started: bool,
    frames_received: u64,
    bytes_received: u64,
    last_frame_number: c_int,
    last_frame_payload: Vec<u8>,
}

impl StreamOutputMode {
    fn callbacks(self) -> StreamOutputCallbacks {
        match self {
            Self::Headless => StreamOutputCallbacks {
                video: headless_video_callbacks(),
                audio: headless_audio_callbacks(),
            },
        }
    }
}

pub fn stream_media_stats_snapshot() -> StreamMediaStats {
    let video = video_sink_state_snapshot();
    let audio = audio_sink_state_snapshot();

    StreamMediaStats {
        video_started: video.started,
        video_frames: video.frames_received,
        video_bytes: video.bytes_received,
        last_video_frame_number: video.last_frame_number,
        audio_started: audio.started,
        audio_packets: audio.samples_received,
        audio_bytes: audio.bytes_received,
    }
}

unsafe extern "C" fn connection_stage_starting(stage: c_int) {
    let stage = stage_name(stage);
    let message = format!("GameStream stage starting: {stage}.");
    eprintln!("{message}");
    emit_stream_event(BridgeEventKind::SessionChanged, message);
}

unsafe extern "C" fn connection_stage_complete(stage: c_int) {
    let stage = stage_name(stage);
    let message = format!("GameStream stage complete: {stage}.");
    eprintln!("{message}");
    emit_stream_event(BridgeEventKind::SessionChanged, message);
}

unsafe extern "C" fn connection_stage_failed(stage: c_int, error_code: c_int) {
    let stage = stage_name(stage);
    let message = format!("GameStream stage failed: {stage} (error {error_code}).");
    eprintln!("{message}");
    emit_stream_event(BridgeEventKind::Status, message);
}

unsafe extern "C" fn connection_started() {
    let message = "Stream session active.".to_string();
    eprintln!("{message}");
    emit_stream_event(BridgeEventKind::SessionChanged, message);
}

unsafe extern "C" fn connection_terminated(error_code: c_int) {
    let message = if error_code == gamestream_sys::ML_ERROR_GRACEFUL_TERMINATION {
        "Stream session cleanup completed.".to_string()
    } else {
        format!("Stream session terminated with code {error_code}.")
    };
    eprintln!("{message}");
    let kind = if error_code == gamestream_sys::ML_ERROR_GRACEFUL_TERMINATION {
        BridgeEventKind::SessionChanged
    } else {
        BridgeEventKind::Status
    };
    emit_stream_event(kind, message);
}

unsafe extern "C" fn connection_status_update(connection_status: c_int) {
    let message = connection_status_message(connection_status);
    eprintln!("{message}");
    emit_stream_event(BridgeEventKind::Status, message);
}

fn set_stream_event_context(context: StreamEventContext) {
    if let Ok(mut slot) = STREAM_EVENT_CONTEXT.get_or_init(|| Mutex::new(None)).lock() {
        *slot = Some(context);
    }
}

fn emit_stream_event(kind: BridgeEventKind, message: String) {
    let Some(context) = stream_event_context() else {
        return;
    };
    let _ = context.sender.send(BridgeEvent {
        kind,
        message,
        host_id: Some(context.host_id),
        app_id: Some(context.app_id),
        controller_action: None,
        update_version: None,
        update_url: None,
    });
}

fn stream_event_context() -> Option<StreamEventContext> {
    STREAM_EVENT_CONTEXT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|slot| slot.clone())
}

fn connection_status_message(connection_status: c_int) -> String {
    match connection_status {
        gamestream_sys::CONN_STATUS_OKAY => "GameStream connection quality is okay.".into(),
        gamestream_sys::CONN_STATUS_POOR => "GameStream connection quality is poor.".into(),
        _ => format!("GameStream connection status update: {connection_status}."),
    }
}

fn headless_video_callbacks() -> gamestream_sys::DecoderRendererCallbacks {
    gamestream_sys::DecoderRendererCallbacks {
        setup: Some(headless_video_setup),
        start: Some(headless_video_start),
        stop: Some(headless_video_stop),
        cleanup: Some(headless_video_cleanup),
        submit_decode_unit: Some(headless_video_submit_decode_unit),
        capabilities: 0,
    }
}

fn headless_audio_callbacks() -> gamestream_sys::AudioRendererCallbacks {
    gamestream_sys::AudioRendererCallbacks {
        init: Some(headless_audio_init),
        start: Some(headless_audio_start),
        stop: Some(headless_audio_stop),
        cleanup: Some(headless_audio_cleanup),
        decode_and_play_sample: Some(headless_audio_decode_and_play_sample),
        capabilities: gamestream_sys::CAPABILITY_SUPPORTS_ARBITRARY_AUDIO_DURATION,
    }
}

unsafe extern "C" fn headless_video_setup(
    video_format: c_int,
    width: c_int,
    height: c_int,
    redraw_rate: c_int,
    _context: *mut c_void,
    _dr_flags: c_int,
) -> c_int {
    store_video_sink_state(|state| {
        *state = VideoSinkState {
            configuration: Some(VideoSinkConfiguration {
                video_format,
                width,
                height,
                redraw_rate,
                flags: _dr_flags,
            }),
            ..VideoSinkState::default()
        };
    });
    emit_stream_event(
        BridgeEventKind::Status,
        format!(
            "Headless video sink configured for {width}x{height}@{redraw_rate} format {video_format}."
        ),
    );
    gamestream_sys::DR_OK
}

unsafe extern "C" fn headless_video_start() {
    store_video_sink_state(|state| {
        state.started = true;
    });
    emit_stream_event(
        BridgeEventKind::Status,
        "Headless video sink started.".into(),
    );
}

unsafe extern "C" fn headless_video_stop() {
    store_video_sink_state(|state| {
        state.started = false;
    });
    emit_stream_event(
        BridgeEventKind::Status,
        "Headless video sink stopped.".into(),
    );
}

unsafe extern "C" fn headless_video_cleanup() {
    store_video_sink_state(|state| {
        *state = VideoSinkState::default();
    });
    emit_stream_event(
        BridgeEventKind::Status,
        "Headless video sink cleaned up.".into(),
    );
}

unsafe extern "C" fn headless_video_submit_decode_unit(
    decode_unit: *mut gamestream_sys::DecodeUnit,
) -> c_int {
    if decode_unit.is_null() {
        return gamestream_sys::DR_OK;
    }

    // SAFETY: Limelight owns the decode unit for the duration of this callback.
    let decode_unit = unsafe { &*decode_unit };
    let bytes_received = decode_unit_bytes(decode_unit);
    let payload = unsafe { copy_decode_unit_payload(decode_unit) };
    let mut first_frame = false;
    store_video_sink_state(|state| {
        first_frame = state.frames_received == 0;
        state.frames_received = state.frames_received.saturating_add(1);
        state.bytes_received = state.bytes_received.saturating_add(bytes_received);
        state.last_frame_number = decode_unit.frame_number;
        state.last_frame_payload = payload;
    });
    if first_frame {
        emit_stream_event(
            BridgeEventKind::Status,
            format!(
                "Headless video sink received its first frame {} ({} bytes).",
                decode_unit.frame_number, bytes_received
            ),
        );
    }
    gamestream_sys::DR_OK
}

fn decode_unit_bytes(decode_unit: &gamestream_sys::DecodeUnit) -> u64 {
    let mut total = 0_u64;
    let mut entry = decode_unit.buffer_list;
    let mut entries_seen = 0;
    while !entry.is_null() && entries_seen < 256 {
        // SAFETY: Limelight supplies a valid linked list for the callback duration.
        let current = unsafe { &*entry };
        if current.length > 0 {
            total = total.saturating_add(current.length as u64);
        }
        entry = current.next;
        entries_seen += 1;
    }
    total
}

unsafe fn copy_decode_unit_payload(decode_unit: &gamestream_sys::DecodeUnit) -> Vec<u8> {
    let capacity = decode_unit_bytes(decode_unit)
        .try_into()
        .unwrap_or_default();
    let mut payload = Vec::with_capacity(capacity);
    let mut entry = decode_unit.buffer_list;
    let mut entries_seen = 0;
    while !entry.is_null() && entries_seen < 256 {
        // SAFETY: The caller guarantees the Limelight decode-unit linked list is valid.
        let current = unsafe { &*entry };
        if !current.data.is_null() && current.length > 0 {
            let length: usize = current.length.try_into().unwrap_or_default();
            // SAFETY: The caller guarantees each non-null buffer is valid for length bytes.
            let data = unsafe { std::slice::from_raw_parts(current.data.cast::<u8>(), length) };
            payload.extend_from_slice(data);
        }
        entry = current.next;
        entries_seen += 1;
    }
    payload
}

unsafe extern "C" fn headless_audio_init(
    audio_configuration: c_int,
    opus_config: *const gamestream_sys::OpusMultistreamConfiguration,
    _context: *mut c_void,
    _ar_flags: c_int,
) -> c_int {
    if opus_config.is_null() {
        emit_stream_event(
            BridgeEventKind::Status,
            "Headless audio sink failed to configure: missing Opus configuration.".into(),
        );
        return -1;
    }

    // SAFETY: Limelight supplies a valid OPUS_MULTISTREAM_CONFIGURATION pointer for init.
    let opus_config = unsafe { *opus_config };
    store_audio_sink_state(|state| {
        *state = AudioSinkState {
            configuration: Some(AudioSinkConfiguration {
                audio_configuration,
                opus_config,
            }),
            ..AudioSinkState::default()
        };
    });
    emit_stream_event(
        BridgeEventKind::Status,
        format!(
            "Headless audio sink configured for audio {audio_configuration} with {} channels.",
            opus_config.channel_count
        ),
    );
    0
}

unsafe extern "C" fn headless_audio_start() {
    store_audio_sink_state(|state| {
        state.started = true;
    });
    emit_stream_event(
        BridgeEventKind::Status,
        "Headless audio sink started.".into(),
    );
}

unsafe extern "C" fn headless_audio_stop() {
    store_audio_sink_state(|state| {
        state.started = false;
    });
    emit_stream_event(
        BridgeEventKind::Status,
        "Headless audio sink stopped.".into(),
    );
}

unsafe extern "C" fn headless_audio_cleanup() {
    store_audio_sink_state(|state| {
        *state = AudioSinkState::default();
    });
    emit_stream_event(
        BridgeEventKind::Status,
        "Headless audio sink cleaned up.".into(),
    );
}

unsafe extern "C" fn headless_audio_decode_and_play_sample(
    sample_data: *mut c_char,
    sample_length: c_int,
) {
    if sample_data.is_null() || sample_length <= 0 {
        return;
    }

    let mut first_sample = false;
    let packet = unsafe { copy_audio_packet(sample_data, sample_length) };
    store_audio_sink_state(|state| {
        first_sample = state.samples_received == 0;
        state.samples_received = state.samples_received.saturating_add(1);
        state.bytes_received = state.bytes_received.saturating_add(sample_length as u64);
        state.last_packet = packet;
    });
    if first_sample {
        emit_stream_event(
            BridgeEventKind::Status,
            format!("Headless audio sink received its first packet ({sample_length} bytes)."),
        );
    }
}

unsafe fn copy_audio_packet(sample_data: *const c_char, sample_length: c_int) -> Vec<u8> {
    if sample_data.is_null() || sample_length <= 0 {
        return Vec::new();
    }

    let length: usize = sample_length.try_into().unwrap_or_default();
    // SAFETY: The caller guarantees this audio packet pointer is valid for sample_length bytes.
    unsafe { std::slice::from_raw_parts(sample_data.cast::<u8>(), length) }.to_vec()
}

fn store_audio_sink_state(update: impl FnOnce(&mut AudioSinkState)) {
    if let Ok(mut state) = AUDIO_SINK_STATE
        .get_or_init(|| Mutex::new(AudioSinkState::default()))
        .lock()
    {
        update(&mut state);
    }
}

fn audio_sink_state_snapshot() -> AudioSinkState {
    AUDIO_SINK_STATE
        .get_or_init(|| Mutex::new(AudioSinkState::default()))
        .lock()
        .map(|state| state.clone())
        .unwrap_or_default()
}

fn store_video_sink_state(update: impl FnOnce(&mut VideoSinkState)) {
    if let Ok(mut state) = VIDEO_SINK_STATE
        .get_or_init(|| Mutex::new(VideoSinkState::default()))
        .lock()
    {
        update(&mut state);
    }
}

fn video_sink_state_snapshot() -> VideoSinkState {
    VIDEO_SINK_STATE
        .get_or_init(|| Mutex::new(VideoSinkState::default()))
        .lock()
        .map(|state| state.clone())
        .unwrap_or_default()
}

fn stage_name(stage: c_int) -> String {
    #[cfg(moonlight_common_c_linked)]
    {
        // SAFETY: LiGetStageName returns either a static null-terminated string or null.
        let ptr = unsafe { gamestream_sys::LiGetStageName(stage) };
        if ptr.is_null() {
            return format!("stage {stage}");
        }
        // SAFETY: Non-null stage names returned by Limelight are C strings.
        return unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
    }

    #[cfg(not(moonlight_common_c_linked))]
    {
        fallback_stage_name(stage).to_string()
    }
}

fn fallback_stage_name(stage: c_int) -> &'static str {
    match stage {
        gamestream_sys::STAGE_NONE => "none",
        gamestream_sys::STAGE_PLATFORM_INIT => "platform initialization",
        gamestream_sys::STAGE_NAME_RESOLUTION => "name resolution",
        gamestream_sys::STAGE_AUDIO_STREAM_INIT => "audio stream initialization",
        gamestream_sys::STAGE_RTSP_HANDSHAKE => "RTSP handshake",
        gamestream_sys::STAGE_CONTROL_STREAM_INIT => "control stream initialization",
        gamestream_sys::STAGE_VIDEO_STREAM_INIT => "video stream initialization",
        gamestream_sys::STAGE_INPUT_STREAM_INIT => "input stream initialization",
        gamestream_sys::STAGE_CONTROL_STREAM_START => "control stream start",
        gamestream_sys::STAGE_VIDEO_STREAM_START => "video stream start",
        gamestream_sys::STAGE_AUDIO_STREAM_START => "audio stream start",
        gamestream_sys::STAGE_INPUT_STREAM_START => "input stream start",
        _ => "unknown stage",
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GameStreamRunner;

impl GameStreamRunner {
    pub fn is_linked(&self) -> bool {
        cfg!(moonlight_common_c_linked)
    }

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
        connection_status_message, emit_stream_event, set_stream_event_context, AudioConfiguration,
        RawSessionConfiguration, RemoteInputCrypto, ServerConnectionConfiguration,
        StreamConfiguration, StreamEventContext, StreamingRemotely,
    };
    use crate::core::events::BridgeEventKind;
    use crate::core::gamestream_sys;
    use crate::core::settings::default_streaming_settings;
    use std::ffi::CStr;
    use std::os::raw::c_int;
    use std::sync::mpsc;

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

    #[test]
    fn connection_lifecycle_callbacks_install_observability_hooks() {
        let callbacks = super::StreamCallbacks::connection_lifecycle();

        assert!(callbacks.connection.stage_starting.is_some());
        assert!(callbacks.connection.stage_complete.is_some());
        assert!(callbacks.connection.stage_failed.is_some());
        assert!(callbacks.connection.connection_started.is_some());
        assert!(callbacks.connection.connection_terminated.is_some());
        assert!(callbacks.connection.connection_status_update.is_some());
        assert!(callbacks.video.setup.is_some());
        assert!(callbacks.video.submit_decode_unit.is_some());
        assert!(callbacks.audio.init.is_some());
        assert!(callbacks.audio.decode_and_play_sample.is_some());
    }

    #[test]
    fn output_mode_installs_media_callbacks() {
        let callbacks = super::StreamCallbacks::connection_lifecycle_for_output(
            super::StreamOutputMode::Headless,
        );

        assert!(callbacks.video.setup.is_some());
        assert!(callbacks.video.submit_decode_unit.is_some());
        assert!(callbacks.audio.init.is_some());
        assert!(callbacks.audio.decode_and_play_sample.is_some());
    }

    #[test]
    fn headless_media_callbacks_are_safe_noop_sinks() {
        let video = super::headless_video_callbacks();
        let audio = super::headless_audio_callbacks();
        let mut second_entry = gamestream_sys::LEntry {
            length: 20,
            ..gamestream_sys::LEntry::default()
        };
        let mut first_entry = gamestream_sys::LEntry {
            next: &mut second_entry,
            length: 10,
            ..gamestream_sys::LEntry::default()
        };
        let mut decode_unit = gamestream_sys::DecodeUnit {
            frame_number: 7,
            buffer_list: &mut first_entry,
            ..gamestream_sys::DecodeUnit::default()
        };
        let mut opus = gamestream_sys::OpusMultistreamConfiguration {
            sample_rate: 48_000,
            channel_count: 2,
            streams: 1,
            coupled_streams: 1,
            samples_per_frame: 240,
            mapping: [0; 8],
        };

        let video_setup = video.setup.unwrap();
        let video_start = video.start.unwrap();
        let video_stop = video.stop.unwrap();
        let video_cleanup = video.cleanup.unwrap();
        let video_submit = video.submit_decode_unit.unwrap();
        let audio_init = audio.init.unwrap();
        let audio_start = audio.start.unwrap();
        let audio_stop = audio.stop.unwrap();
        let audio_cleanup = audio.cleanup.unwrap();
        let decode_and_play_sample = audio.decode_and_play_sample.unwrap();

        let video_result = unsafe { video_setup(1, 1920, 1080, 60, std::ptr::null_mut(), 0) };
        let submit_result = unsafe {
            video_start();
            video_submit(&mut decode_unit)
        };
        let audio_result = unsafe {
            audio_init(
                gamestream_sys::AUDIO_CONFIGURATION_STEREO,
                &mut opus,
                std::ptr::null_mut(),
                0,
            )
        };
        let mut encoded_sample = [1_i8, 2, 3, 4];
        unsafe {
            audio_start();
            decode_and_play_sample(encoded_sample.as_mut_ptr(), encoded_sample.len() as i32);
            audio_stop();
        }
        let state = super::audio_sink_state_snapshot();
        let video_state = super::video_sink_state_snapshot();

        assert_eq!(gamestream_sys::DR_OK, video_result);
        assert_eq!(gamestream_sys::DR_OK, submit_result);
        assert_eq!(
            Some(super::VideoSinkConfiguration {
                video_format: 1,
                width: 1920,
                height: 1080,
                redraw_rate: 60,
                flags: 0,
            }),
            video_state.configuration
        );
        assert!(video_state.started);
        assert_eq!(1, video_state.frames_received);
        assert_eq!(30, video_state.bytes_received);
        assert_eq!(7, video_state.last_frame_number);
        assert_eq!(0, audio_result);
        assert_eq!(
            Some(opus),
            state.configuration.map(|config| config.opus_config)
        );
        assert!(!state.started);
        assert_eq!(1, state.samples_received);
        assert_eq!(encoded_sample.len() as u64, state.bytes_received);
        assert_eq!(vec![1, 2, 3, 4], state.last_packet);
        assert_eq!(
            gamestream_sys::CAPABILITY_SUPPORTS_ARBITRARY_AUDIO_DURATION,
            audio.capabilities
        );

        unsafe {
            video_stop();
            video_cleanup();
            audio_cleanup();
        }
        assert_eq!(
            super::VideoSinkState::default(),
            super::video_sink_state_snapshot()
        );
        assert_eq!(
            super::AudioSinkState::default(),
            super::audio_sink_state_snapshot()
        );
    }

    #[test]
    fn decode_unit_payload_is_copied_in_buffer_order() {
        let mut second_payload = [4_i8, 5, 6];
        let mut first_payload = [1_i8, 2, 3];
        let mut second_entry = gamestream_sys::LEntry {
            data: second_payload.as_mut_ptr(),
            length: second_payload.len() as i32,
            ..gamestream_sys::LEntry::default()
        };
        let mut first_entry = gamestream_sys::LEntry {
            next: &mut second_entry,
            data: first_payload.as_mut_ptr(),
            length: first_payload.len() as i32,
            ..gamestream_sys::LEntry::default()
        };
        let decode_unit = gamestream_sys::DecodeUnit {
            buffer_list: &mut first_entry,
            ..gamestream_sys::DecodeUnit::default()
        };

        let payload = unsafe { super::copy_decode_unit_payload(&decode_unit) };

        assert_eq!(vec![1, 2, 3, 4, 5, 6], payload);
        assert_eq!(6, super::decode_unit_bytes(&decode_unit));
    }

    #[test]
    fn audio_packet_payload_is_copied_to_owned_bytes() {
        let mut packet = [9_i8, 8, 7, 6];

        let payload = unsafe { super::copy_audio_packet(packet.as_mut_ptr(), packet.len() as i32) };

        assert_eq!(vec![9, 8, 7, 6], payload);
    }

    #[test]
    fn connection_status_messages_match_ui_contract() {
        assert_eq!(
            "GameStream connection quality is okay.",
            connection_status_message(gamestream_sys::CONN_STATUS_OKAY)
        );
        assert_eq!(
            "GameStream connection quality is poor.",
            connection_status_message(gamestream_sys::CONN_STATUS_POOR)
        );
    }

    #[test]
    fn stream_events_include_active_host_and_app() {
        let (sender, receiver) = mpsc::channel();
        set_stream_event_context(StreamEventContext {
            sender,
            host_id: "host-1".into(),
            app_id: "app-1".into(),
        });

        emit_stream_event(
            BridgeEventKind::SessionChanged,
            "Stream session active.".into(),
        );

        let event = receiver.recv().unwrap();
        assert_eq!(BridgeEventKind::SessionChanged, event.kind);
        assert_eq!("Stream session active.", event.message);
        assert_eq!(Some("host-1".into()), event.host_id);
        assert_eq!(Some("app-1".into()), event.app_id);
    }

    #[test]
    fn fallback_stage_names_cover_known_limelight_stages() {
        assert_eq!(
            "RTSP handshake",
            super::fallback_stage_name(gamestream_sys::STAGE_RTSP_HANDSHAKE)
        );
        assert_eq!("unknown stage", super::fallback_stage_name(12345));
    }
}
