#![allow(dead_code)]

use super::error::CoreError;
use super::events::{BridgeEvent, BridgeEventKind};
use super::gamestream_sys;
use super::stream_input::{
    ButtonAction, KeyAction, KeyModifiers, MouseButton as StreamMouseButton, StreamInputSender,
};
use crate::logger;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
#[cfg(moonlight_common_c_linked)]
use std::ffi::CStr;
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_uchar, c_void};
use std::sync::mpsc::{self, Sender, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};

const DEFAULT_VIDEO_PACKET_SIZE: u32 = 1392;

static STREAM_EVENT_CONTEXT: OnceLock<Mutex<Option<StreamEventContext>>> = OnceLock::new();
static AUDIO_SINK_STATE: OnceLock<Mutex<AudioSinkState>> = OnceLock::new();
#[cfg(moonlight_common_c_linked)]
static AUDIO_DECODER_STATE: OnceLock<Mutex<Option<OpusAudioDecoder>>> = OnceLock::new();
static AUDIO_PLAYBACK_STATE: OnceLock<Mutex<Option<AudioPlayback>>> = OnceLock::new();
static VIDEO_SINK_STATE: OnceLock<Mutex<VideoSinkState>> = OnceLock::new();
#[cfg(moonlight_common_c_linked)]
static VIDEO_DECODER_STATE: OnceLock<Mutex<Option<SoftwareVideoDecoder>>> = OnceLock::new();
static VIDEO_RENDERER_STATE: OnceLock<Mutex<Option<NativeVideoRenderer>>> = OnceLock::new();

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

#[derive(Clone, Debug, Default, PartialEq)]
struct AudioSinkState {
    configuration: Option<AudioSinkConfiguration>,
    started: bool,
    samples_received: u64,
    bytes_received: u64,
    decoded_samples: u64,
    last_packet: Vec<u8>,
    last_pcm_frame: Vec<f32>,
}

#[cfg(moonlight_common_c_linked)]
#[derive(Debug)]
struct OpusAudioDecoder {
    decoder: *mut gamestream_sys::OpusMSDecoder,
    channel_count: usize,
    samples_per_frame: c_int,
}

struct AudioPlayback {
    queue: Arc<Mutex<VecDeque<f32>>>,
    input_channels: usize,
    stop_sender: Option<Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for AudioPlayback {
    fn drop(&mut self) {
        if let Some(stop_sender) = self.stop_sender.take() {
            let _ = stop_sender.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(moonlight_common_c_linked)]
unsafe impl Send for OpusAudioDecoder {}

#[cfg(moonlight_common_c_linked)]
impl Drop for OpusAudioDecoder {
    fn drop(&mut self) {
        if !self.decoder.is_null() {
            // SAFETY: decoder is created by opus_multistream_decoder_create and owned here.
            unsafe { gamestream_sys::opus_multistream_decoder_destroy(self.decoder) };
            self.decoder = std::ptr::null_mut();
        }
    }
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
    decoded_frames: u64,
    bytes_received: u64,
    last_frame_number: c_int,
    last_frame_payload: Vec<u8>,
    last_rgba_frame: Option<RgbaVideoFrame>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RgbaVideoFrame {
    width: c_int,
    height: c_int,
    frame_number: c_int,
    pixels: Vec<u8>,
}

struct NativeVideoRenderer {
    frame_sender: SyncSender<RgbaVideoFrame>,
    stop_sender: Option<Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[derive(Debug, Default)]
struct NativeVideoInputState {
    last_mouse_position: Option<(i16, i16)>,
    left_mouse_down: bool,
    middle_mouse_down: bool,
    right_mouse_down: bool,
}

impl Drop for NativeVideoRenderer {
    fn drop(&mut self) {
        if let Some(stop_sender) = self.stop_sender.take() {
            let _ = stop_sender.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(moonlight_common_c_linked)]
#[derive(Debug)]
struct SoftwareVideoDecoder {
    codec_context: *mut gamestream_sys::AVCodecContext,
    frame: *mut gamestream_sys::AVFrame,
    sws_context: *mut gamestream_sys::SwsContext,
    codec_name: &'static [u8],
    sws_source_width: c_int,
    sws_source_height: c_int,
    sws_source_format: c_int,
}

#[cfg(moonlight_common_c_linked)]
unsafe impl Send for SoftwareVideoDecoder {}

#[cfg(moonlight_common_c_linked)]
impl Drop for SoftwareVideoDecoder {
    fn drop(&mut self) {
        if !self.sws_context.is_null() {
            // SAFETY: sws_context is created by sws_getContext and owned by this decoder.
            unsafe { gamestream_sys::sws_freeContext(self.sws_context) };
            self.sws_context = std::ptr::null_mut();
        }
        if !self.frame.is_null() {
            // SAFETY: frame is allocated by av_frame_alloc and owned by this decoder.
            unsafe { gamestream_sys::av_frame_free(&mut self.frame) };
        }
        if !self.codec_context.is_null() {
            // SAFETY: codec_context is allocated by avcodec_alloc_context3 and owned here.
            unsafe { gamestream_sys::avcodec_free_context(&mut self.codec_context) };
        }
    }
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
        logger::log(format!(
            "stream event without context; kind={kind:?}; message={message}"
        ));
        return;
    };
    logger::log(format!(
        "stream event; kind={kind:?}; host_id={}; app_id={}; message={message}",
        context.host_id, context.app_id
    ));
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
    #[cfg(moonlight_common_c_linked)]
    let decoder_result = SoftwareVideoDecoder::new(video_format);

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
    #[cfg(moonlight_common_c_linked)]
    match decoder_result {
        Ok(decoder) => {
            let codec = decoder.codec_display_name();
            if let Ok(mut slot) = VIDEO_DECODER_STATE.get_or_init(|| Mutex::new(None)).lock() {
                *slot = Some(decoder);
            }
            emit_stream_event(
                BridgeEventKind::Status,
                format!("Software video decoder configured for {codec}."),
            );
        }
        Err(error) => {
            emit_stream_event(
                BridgeEventKind::Status,
                format!("Software video decoder setup failed: {error}."),
            );
            return gamestream_sys::DR_NEED_IDR;
        }
    }
    start_native_video_renderer(width, height);
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
    stop_native_video_renderer();
    #[cfg(moonlight_common_c_linked)]
    if let Ok(mut slot) = VIDEO_DECODER_STATE.get_or_init(|| Mutex::new(None)).lock() {
        *slot = None;
    }
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
    #[cfg(moonlight_common_c_linked)]
    let decoded_frame = decode_video_payload(&payload, decode_unit.frame_number);
    #[cfg(not(moonlight_common_c_linked))]
    let decoded_frame: Option<RgbaVideoFrame> = None;
    let mut first_frame = false;
    let mut first_decoded_frame = false;
    store_video_sink_state(|state| {
        first_frame = state.frames_received == 0;
        state.frames_received = state.frames_received.saturating_add(1);
        state.bytes_received = state.bytes_received.saturating_add(bytes_received);
        state.last_frame_number = decode_unit.frame_number;
        state.last_frame_payload = payload;
        if let Some(frame) = decoded_frame {
            first_decoded_frame = state.decoded_frames == 0;
            state.decoded_frames = state.decoded_frames.saturating_add(1);
            send_native_video_frame(frame.clone());
            state.last_rgba_frame = Some(frame);
        }
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
    if first_decoded_frame {
        emit_stream_event(
            BridgeEventKind::Status,
            "First decoded video frame is ready for native presentation.".into(),
        );
    }
    gamestream_sys::DR_OK
}

fn start_native_video_renderer(width: c_int, height: c_int) {
    stop_native_video_renderer();
    let width = width.max(1) as usize;
    let height = height.max(1) as usize;
    logger::log(format!(
        "starting native video renderer; width={width}; height={height}"
    ));
    let (frame_sender, frame_receiver) = mpsc::sync_channel::<RgbaVideoFrame>(1);
    let (stop_sender, stop_receiver) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        if let Err(error) = native_video_renderer_loop(width, height, frame_receiver, stop_receiver)
        {
            emit_stream_event(
                BridgeEventKind::Status,
                format!("Native video renderer stopped: {error}."),
            );
        }
    });
    if let Ok(mut slot) = VIDEO_RENDERER_STATE.get_or_init(|| Mutex::new(None)).lock() {
        *slot = Some(NativeVideoRenderer {
            frame_sender,
            stop_sender: Some(stop_sender),
            thread: Some(thread),
        });
    }
}

fn stop_native_video_renderer() {
    if let Ok(mut slot) = VIDEO_RENDERER_STATE.get_or_init(|| Mutex::new(None)).lock() {
        if slot.is_some() {
            logger::log("stopping native video renderer");
        }
        *slot = None;
    }
}

fn send_native_video_frame(frame: RgbaVideoFrame) {
    if let Ok(slot) = VIDEO_RENDERER_STATE.get_or_init(|| Mutex::new(None)).lock() {
        if let Some(renderer) = slot.as_ref() {
            match renderer.frame_sender.try_send(frame) {
                Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    emit_stream_event(
                        BridgeEventKind::Status,
                        "Native video renderer is no longer accepting frames.".into(),
                    );
                }
            }
        }
    }
}

fn native_video_renderer_loop(
    width: usize,
    height: usize,
    frame_receiver: mpsc::Receiver<RgbaVideoFrame>,
    stop_receiver: mpsc::Receiver<()>,
) -> Result<(), String> {
    logger::log(format!(
        "native video renderer creating window; width={width}; height={height}"
    ));
    let mut window = minifb::Window::new(
        "Moonlight Stream",
        width,
        height,
        minifb::WindowOptions::default(),
    )
    .map_err(|error| error.to_string())?;
    logger::log("native video renderer window created");
    let mut last_buffer = vec![0; width * height];
    let mut last_width = width;
    let mut last_height = height;
    let mut input_state = NativeVideoInputState::default();
    let input_sender = StreamInputSender;
    let mut requested_stop = false;

    while window.is_open() {
        if stop_receiver.try_recv().is_ok() {
            requested_stop = true;
            break;
        }
        poll_native_video_input(
            &window,
            &mut input_state,
            &input_sender,
            last_width,
            last_height,
        );
        match frame_receiver.recv_timeout(std::time::Duration::from_millis(16)) {
            Ok(frame) => {
                let converted = rgba_to_minifb_buffer(&frame)?;
                last_width = frame.width as usize;
                last_height = frame.height as usize;
                last_buffer = converted;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        window
            .update_with_buffer(&last_buffer, last_width, last_height)
            .map_err(|error| error.to_string())?;
    }

    if !requested_stop {
        emit_stream_event(
            BridgeEventKind::SessionChanged,
            "Native stream window closed; interrupting GameStream session.".into(),
        );
        #[cfg(moonlight_common_c_linked)]
        unsafe {
            gamestream_sys::LiInterruptConnection();
        }
    }

    Ok(())
}

fn poll_native_video_input(
    window: &minifb::Window,
    state: &mut NativeVideoInputState,
    input: &StreamInputSender,
    reference_width: usize,
    reference_height: usize,
) {
    if let Some((x, y)) = window.get_mouse_pos(minifb::MouseMode::Clamp) {
        let x = clamp_f32_to_i16(x);
        let y = clamp_f32_to_i16(y);
        if state.last_mouse_position != Some((x, y)) {
            let _ = input.send_mouse_position(
                x,
                y,
                clamp_usize_to_i16(reference_width),
                clamp_usize_to_i16(reference_height),
            );
            state.last_mouse_position = Some((x, y));
        }
    }

    update_native_mouse_button(
        input,
        &mut state.left_mouse_down,
        window.get_mouse_down(minifb::MouseButton::Left),
        StreamMouseButton::Left,
    );
    update_native_mouse_button(
        input,
        &mut state.middle_mouse_down,
        window.get_mouse_down(minifb::MouseButton::Middle),
        StreamMouseButton::Middle,
    );
    update_native_mouse_button(
        input,
        &mut state.right_mouse_down,
        window.get_mouse_down(minifb::MouseButton::Right),
        StreamMouseButton::Right,
    );

    if let Some((scroll_x, scroll_y)) = window.get_scroll_wheel() {
        let scroll_x = (scroll_x * 120.0).round();
        let scroll_y = (scroll_y * 120.0).round();
        if scroll_x != 0.0 {
            let _ = input.send_high_res_horizontal_scroll(clamp_f32_to_i16(scroll_x));
        }
        if scroll_y != 0.0 {
            let _ = input.send_high_res_scroll(clamp_f32_to_i16(scroll_y));
        }
    }

    let modifiers = native_key_modifiers(window);
    for key in window.get_keys_pressed(minifb::KeyRepeat::No) {
        if let Some(key_code) = minifb_key_to_js_key_code(key) {
            let _ = input.send_keyboard(key_code, KeyAction::Down, modifiers, true);
        }
    }
    for key in window.get_keys_released() {
        if let Some(key_code) = minifb_key_to_js_key_code(key) {
            let _ = input.send_keyboard(key_code, KeyAction::Up, modifiers, true);
        }
    }
}

fn update_native_mouse_button(
    input: &StreamInputSender,
    previous: &mut bool,
    current: bool,
    button: StreamMouseButton,
) {
    if *previous == current {
        return;
    }
    let action = if current {
        ButtonAction::Press
    } else {
        ButtonAction::Release
    };
    let _ = input.send_mouse_button(action, button);
    *previous = current;
}

fn native_key_modifiers(window: &minifb::Window) -> KeyModifiers {
    let keys = window.get_keys();
    KeyModifiers {
        shift: keys.contains(&minifb::Key::LeftShift) || keys.contains(&minifb::Key::RightShift),
        ctrl: keys.contains(&minifb::Key::LeftCtrl) || keys.contains(&minifb::Key::RightCtrl),
        alt: keys.contains(&minifb::Key::LeftAlt) || keys.contains(&minifb::Key::RightAlt),
        meta: keys.contains(&minifb::Key::LeftSuper) || keys.contains(&minifb::Key::RightSuper),
    }
}

fn minifb_key_to_js_key_code(key: minifb::Key) -> Option<i16> {
    let key_code = match key {
        minifb::Key::Backspace => 8,
        minifb::Key::Tab => 9,
        minifb::Key::Enter | minifb::Key::NumPadEnter => 13,
        minifb::Key::LeftShift | minifb::Key::RightShift => 16,
        minifb::Key::LeftCtrl | minifb::Key::RightCtrl => 17,
        minifb::Key::LeftAlt | minifb::Key::RightAlt => 18,
        minifb::Key::Pause => 19,
        minifb::Key::CapsLock => 20,
        minifb::Key::Escape => 27,
        minifb::Key::Space => 32,
        minifb::Key::PageUp => 33,
        minifb::Key::PageDown => 34,
        minifb::Key::End => 35,
        minifb::Key::Home => 36,
        minifb::Key::Left => 37,
        minifb::Key::Up => 38,
        minifb::Key::Right => 39,
        minifb::Key::Down => 40,
        minifb::Key::Insert => 45,
        minifb::Key::Delete => 46,
        minifb::Key::Key0 => 48,
        minifb::Key::Key1 => 49,
        minifb::Key::Key2 => 50,
        minifb::Key::Key3 => 51,
        minifb::Key::Key4 => 52,
        minifb::Key::Key5 => 53,
        minifb::Key::Key6 => 54,
        minifb::Key::Key7 => 55,
        minifb::Key::Key8 => 56,
        minifb::Key::Key9 => 57,
        minifb::Key::A => 65,
        minifb::Key::B => 66,
        minifb::Key::C => 67,
        minifb::Key::D => 68,
        minifb::Key::E => 69,
        minifb::Key::F => 70,
        minifb::Key::G => 71,
        minifb::Key::H => 72,
        minifb::Key::I => 73,
        minifb::Key::J => 74,
        minifb::Key::K => 75,
        minifb::Key::L => 76,
        minifb::Key::M => 77,
        minifb::Key::N => 78,
        minifb::Key::O => 79,
        minifb::Key::P => 80,
        minifb::Key::Q => 81,
        minifb::Key::R => 82,
        minifb::Key::S => 83,
        minifb::Key::T => 84,
        minifb::Key::U => 85,
        minifb::Key::V => 86,
        minifb::Key::W => 87,
        minifb::Key::X => 88,
        minifb::Key::Y => 89,
        minifb::Key::Z => 90,
        minifb::Key::LeftSuper | minifb::Key::RightSuper => 91,
        minifb::Key::Menu => 93,
        minifb::Key::NumPad0 => 96,
        minifb::Key::NumPad1 => 97,
        minifb::Key::NumPad2 => 98,
        minifb::Key::NumPad3 => 99,
        minifb::Key::NumPad4 => 100,
        minifb::Key::NumPad5 => 101,
        minifb::Key::NumPad6 => 102,
        minifb::Key::NumPad7 => 103,
        minifb::Key::NumPad8 => 104,
        minifb::Key::NumPad9 => 105,
        minifb::Key::NumPadAsterisk => 106,
        minifb::Key::NumPadPlus => 107,
        minifb::Key::NumPadMinus => 109,
        minifb::Key::NumPadDot => 110,
        minifb::Key::NumPadSlash => 111,
        minifb::Key::F1 => 112,
        minifb::Key::F2 => 113,
        minifb::Key::F3 => 114,
        minifb::Key::F4 => 115,
        minifb::Key::F5 => 116,
        minifb::Key::F6 => 117,
        minifb::Key::F7 => 118,
        minifb::Key::F8 => 119,
        minifb::Key::F9 => 120,
        minifb::Key::F10 => 121,
        minifb::Key::F11 => 122,
        minifb::Key::F12 => 123,
        minifb::Key::NumLock => 144,
        minifb::Key::ScrollLock => 145,
        minifb::Key::Semicolon => 186,
        minifb::Key::Equal => 187,
        minifb::Key::Comma => 188,
        minifb::Key::Minus => 189,
        minifb::Key::Period => 190,
        minifb::Key::Slash => 191,
        minifb::Key::Backquote => 192,
        minifb::Key::LeftBracket => 219,
        minifb::Key::Backslash => 220,
        minifb::Key::RightBracket => 221,
        minifb::Key::Apostrophe => 222,
        minifb::Key::F13 => 124,
        minifb::Key::F14 => 125,
        minifb::Key::F15 => 126,
        minifb::Key::Unknown => return None,
        minifb::Key::Count => return None,
    };
    Some(key_code)
}

fn clamp_f32_to_i16(value: f32) -> i16 {
    value.clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

fn clamp_usize_to_i16(value: usize) -> i16 {
    value.min(i16::MAX as usize) as i16
}

fn rgba_to_minifb_buffer(frame: &RgbaVideoFrame) -> Result<Vec<u32>, String> {
    if frame.width <= 0 || frame.height <= 0 {
        return Err("decoded frame has invalid dimensions".into());
    }
    let pixel_count = frame.width as usize * frame.height as usize;
    if frame.pixels.len() < pixel_count * 4 {
        return Err("decoded frame buffer is shorter than expected".into());
    }

    let mut output = Vec::with_capacity(pixel_count);
    for pixel in frame.pixels.chunks_exact(4).take(pixel_count) {
        output.push(((pixel[0] as u32) << 16) | ((pixel[1] as u32) << 8) | pixel[2] as u32);
    }
    Ok(output)
}

#[cfg(moonlight_common_c_linked)]
impl SoftwareVideoDecoder {
    fn new(video_format: c_int) -> Result<Self, String> {
        let codec_name = ffmpeg_codec_name(video_format)
            .ok_or_else(|| format!("unsupported GameStream video format {video_format}"))?;
        // SAFETY: codec_name is a static null-terminated C string.
        let codec =
            unsafe { gamestream_sys::avcodec_find_decoder_by_name(codec_name.as_ptr().cast()) };
        if codec.is_null() {
            return Err(format!(
                "FFmpeg decoder {} was not found",
                codec_name_without_nul(codec_name)
            ));
        }

        // SAFETY: codec is a valid decoder returned by FFmpeg.
        let codec_context = unsafe { gamestream_sys::avcodec_alloc_context3(codec) };
        if codec_context.is_null() {
            return Err("FFmpeg decoder context allocation failed".into());
        }

        // SAFETY: codec_context is newly allocated and options may be NULL.
        let open_result =
            unsafe { gamestream_sys::avcodec_open2(codec_context, codec, std::ptr::null_mut()) };
        if open_result < 0 {
            let mut context_to_free = codec_context;
            // SAFETY: context_to_free is owned locally after allocation failure.
            unsafe { gamestream_sys::avcodec_free_context(&mut context_to_free) };
            return Err(format!(
                "FFmpeg decoder open failed with code {open_result}"
            ));
        }

        // SAFETY: returns an FFmpeg-owned frame object on success.
        let frame = unsafe { gamestream_sys::av_frame_alloc() };
        if frame.is_null() {
            let mut context_to_free = codec_context;
            // SAFETY: context_to_free is owned locally.
            unsafe { gamestream_sys::avcodec_free_context(&mut context_to_free) };
            return Err("FFmpeg frame allocation failed".into());
        }

        Ok(Self {
            codec_context,
            frame,
            sws_context: std::ptr::null_mut(),
            codec_name,
            sws_source_width: 0,
            sws_source_height: 0,
            sws_source_format: c_int::MIN,
        })
    }

    fn codec_display_name(&self) -> String {
        codec_name_without_nul(self.codec_name)
    }

    fn decode(
        &mut self,
        payload: &[u8],
        frame_number: c_int,
    ) -> Result<Option<RgbaVideoFrame>, String> {
        if payload.is_empty() {
            return Ok(None);
        }
        let mut packet = FfmpegPacket::from_payload(payload)?;
        // SAFETY: codec_context and packet are valid for the duration of the call.
        let send_result =
            unsafe { gamestream_sys::avcodec_send_packet(self.codec_context, packet.as_ptr()) };
        packet.unref();
        if send_result < 0
            && send_result != gamestream_sys::AVERROR_EAGAIN
            && send_result != gamestream_sys::AVERROR_EOF
        {
            return Err(format!(
                "FFmpeg packet submit failed with code {send_result}"
            ));
        }

        let mut last_frame = None;
        loop {
            // SAFETY: codec_context is open and frame is owned by this decoder.
            let receive_result =
                unsafe { gamestream_sys::avcodec_receive_frame(self.codec_context, self.frame) };
            if receive_result == gamestream_sys::AVERROR_EAGAIN
                || receive_result == gamestream_sys::AVERROR_EOF
            {
                break;
            }
            if receive_result < 0 {
                return Err(format!(
                    "FFmpeg frame receive failed with code {receive_result}"
                ));
            }
            last_frame = Some(self.convert_current_frame(frame_number)?);
            // SAFETY: frame is owned by this decoder and may be reused after unref.
            unsafe { gamestream_sys::av_frame_unref(self.frame) };
        }

        Ok(last_frame)
    }

    fn convert_current_frame(&mut self, frame_number: c_int) -> Result<RgbaVideoFrame, String> {
        // SAFETY: frame is currently filled by avcodec_receive_frame.
        let frame = unsafe { &*self.frame };
        let width = frame.width;
        let height = frame.height;
        let source_format = frame.format;
        if width <= 0 || height <= 0 {
            return Err("FFmpeg returned a decoded frame with invalid dimensions".into());
        }
        if self.sws_context.is_null()
            || self.sws_source_width != width
            || self.sws_source_height != height
            || self.sws_source_format != source_format
        {
            if !self.sws_context.is_null() {
                // SAFETY: sws_context is owned by this decoder.
                unsafe { gamestream_sys::sws_freeContext(self.sws_context) };
            }
            // SAFETY: all scalar arguments are from a decoded frame; filters and params may be NULL.
            self.sws_context = unsafe {
                gamestream_sys::sws_getContext(
                    width,
                    height,
                    source_format,
                    width,
                    height,
                    gamestream_sys::AV_PIX_FMT_RGBA,
                    gamestream_sys::SWS_BILINEAR,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            if self.sws_context.is_null() {
                return Err("FFmpeg swscale context allocation failed".into());
            }
            self.sws_source_width = width;
            self.sws_source_height = height;
            self.sws_source_format = source_format;
        }

        let byte_len = width as usize * height as usize * 4;
        let mut pixels = vec![0; byte_len];
        let src_slices = frame.data.map(|ptr| ptr as *const c_uchar);
        let dst_slices = [
            pixels.as_mut_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ];
        let dst_stride = [width * 4, 0, 0, 0, 0, 0, 0, 0];
        // SAFETY: src slices/strides reference the live AVFrame; dst points to a large enough RGBA buffer.
        let scaled_rows = unsafe {
            gamestream_sys::sws_scale(
                self.sws_context,
                src_slices.as_ptr(),
                frame.linesize.as_ptr(),
                0,
                height,
                dst_slices.as_ptr(),
                dst_stride.as_ptr(),
            )
        };
        if scaled_rows != height {
            return Err(format!(
                "FFmpeg swscale converted {scaled_rows} of {height} rows"
            ));
        }

        Ok(RgbaVideoFrame {
            width,
            height,
            frame_number,
            pixels,
        })
    }
}

#[cfg(moonlight_common_c_linked)]
struct FfmpegPacket {
    packet: *mut gamestream_sys::AVPacket,
}

#[cfg(moonlight_common_c_linked)]
impl FfmpegPacket {
    fn from_payload(payload: &[u8]) -> Result<Self, String> {
        // SAFETY: returns an owned packet or NULL on allocation failure.
        let packet = unsafe { gamestream_sys::av_packet_alloc() };
        if packet.is_null() {
            return Err("FFmpeg packet allocation failed".into());
        }

        let padded_len = payload.len() + 64;
        // SAFETY: av_malloc returns memory suitable for av_packet_from_data ownership.
        let buffer = unsafe { gamestream_sys::av_malloc(padded_len) as *mut c_uchar };
        if buffer.is_null() {
            let mut packet_to_free = packet;
            // SAFETY: packet_to_free is owned locally.
            unsafe { gamestream_sys::av_packet_free(&mut packet_to_free) };
            return Err("FFmpeg packet buffer allocation failed".into());
        }

        // SAFETY: buffer has padded_len bytes; payload.len() bytes are initialized from payload and padding is zeroed.
        unsafe {
            std::ptr::copy_nonoverlapping(payload.as_ptr(), buffer, payload.len());
            std::ptr::write_bytes(buffer.add(payload.len()), 0, 64);
        }

        // SAFETY: packet and buffer are owned; packet takes ownership of buffer on success.
        let packet_result =
            unsafe { gamestream_sys::av_packet_from_data(packet, buffer, payload.len() as c_int) };
        if packet_result < 0 {
            let mut packet_to_free = packet;
            // SAFETY: packet is still owned locally; buffer ownership was not transferred on error.
            unsafe {
                gamestream_sys::av_free(buffer.cast());
                gamestream_sys::av_packet_free(&mut packet_to_free);
            }
            return Err(format!(
                "FFmpeg packet data attach failed with code {packet_result}"
            ));
        }

        Ok(Self { packet })
    }

    fn as_ptr(&self) -> *const gamestream_sys::AVPacket {
        self.packet
    }

    fn unref(&mut self) {
        if !self.packet.is_null() {
            // SAFETY: packet is valid and owned by this wrapper.
            unsafe { gamestream_sys::av_packet_unref(self.packet) };
        }
    }
}

#[cfg(moonlight_common_c_linked)]
impl Drop for FfmpegPacket {
    fn drop(&mut self) {
        if !self.packet.is_null() {
            // SAFETY: packet is allocated by av_packet_alloc and owned by this wrapper.
            unsafe { gamestream_sys::av_packet_free(&mut self.packet) };
        }
    }
}

#[cfg(moonlight_common_c_linked)]
fn decode_video_payload(payload: &[u8], frame_number: c_int) -> Option<RgbaVideoFrame> {
    let mut slot = VIDEO_DECODER_STATE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()?;
    let decoder = slot.as_mut()?;
    match decoder.decode(payload, frame_number) {
        Ok(frame) => frame,
        Err(error) => {
            emit_stream_event(
                BridgeEventKind::Status,
                format!("Software video decode failed: {error}."),
            );
            None
        }
    }
}

#[cfg(moonlight_common_c_linked)]
fn ffmpeg_codec_name(video_format: c_int) -> Option<&'static [u8]> {
    if video_format
        & (gamestream_sys::VIDEO_FORMAT_AV1_MAIN8
            | gamestream_sys::VIDEO_FORMAT_AV1_MAIN10
            | gamestream_sys::VIDEO_FORMAT_AV1_HIGH8_444
            | gamestream_sys::VIDEO_FORMAT_AV1_HIGH10_444)
        != 0
    {
        Some(b"av1\0")
    } else if video_format
        & (gamestream_sys::VIDEO_FORMAT_H265
            | gamestream_sys::VIDEO_FORMAT_H265_MAIN10
            | gamestream_sys::VIDEO_FORMAT_HEVC_REXT8_444
            | gamestream_sys::VIDEO_FORMAT_HEVC_REXT10_444)
        != 0
    {
        Some(b"hevc\0")
    } else if video_format
        & (gamestream_sys::VIDEO_FORMAT_H264 | gamestream_sys::VIDEO_FORMAT_H264_HIGH8_444)
        != 0
    {
        Some(b"h264\0")
    } else {
        None
    }
}

#[cfg(moonlight_common_c_linked)]
fn codec_name_without_nul(name: &'static [u8]) -> String {
    String::from_utf8_lossy(name.strip_suffix(b"\0").unwrap_or(name)).into_owned()
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
    #[cfg(moonlight_common_c_linked)]
    if let Err(error) = initialize_opus_audio_decoder(&opus_config) {
        emit_stream_event(
            BridgeEventKind::Status,
            format!("Rust audio decoder failed to configure: {error}."),
        );
        return -1;
    }
    if let Err(error) = initialize_audio_playback(&opus_config) {
        emit_stream_event(
            BridgeEventKind::Status,
            format!("Rust audio playback is unavailable: {error}."),
        );
    }
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
            "Rust audio sink configured for audio {audio_configuration} with {} channels.",
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
    #[cfg(moonlight_common_c_linked)]
    clear_opus_audio_decoder();
    clear_audio_playback();
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
    #[cfg(moonlight_common_c_linked)]
    let decoded = decode_opus_audio_packet(&packet).unwrap_or_else(|error| {
        emit_stream_event(
            BridgeEventKind::Status,
            format!("Rust audio decoder failed to decode packet: {error}."),
        );
        Vec::new()
    });
    #[cfg(not(moonlight_common_c_linked))]
    let decoded = Vec::new();
    if !decoded.is_empty() {
        queue_audio_playback(&decoded);
    }
    store_audio_sink_state(|state| {
        first_sample = state.samples_received == 0;
        state.samples_received = state.samples_received.saturating_add(1);
        state.bytes_received = state.bytes_received.saturating_add(sample_length as u64);
        state.decoded_samples = state.decoded_samples.saturating_add(
            (decoded.len()
                / state
                    .configuration
                    .map(|c| c.opus_config.channel_count.max(1) as usize)
                    .unwrap_or(1)) as u64,
        );
        state.last_packet = packet;
        state.last_pcm_frame = decoded;
    });
    if first_sample {
        emit_stream_event(
            BridgeEventKind::Status,
            format!("Rust audio sink decoded its first packet ({sample_length} bytes)."),
        );
    }
}

fn initialize_audio_playback(
    opus_config: &gamestream_sys::OpusMultistreamConfiguration,
) -> Result<(), String> {
    let input_channels = usize::try_from(opus_config.channel_count)
        .map_err(|_| format!("invalid channel count {}", opus_config.channel_count))?
        .max(1);
    let queue = Arc::new(Mutex::new(VecDeque::new()));
    let (ready_sender, ready_receiver) = mpsc::channel();
    let (stop_sender, stop_receiver) = mpsc::channel();
    let thread_queue = Arc::clone(&queue);
    let thread = std::thread::spawn(move || {
        run_audio_playback_thread(thread_queue, input_channels, stop_receiver, ready_sender);
    });
    match ready_receiver.recv().map_err(|error| error.to_string())? {
        Ok(()) => {}
        Err(error) => {
            let _ = stop_sender.send(());
            let _ = thread.join();
            return Err(error);
        }
    }

    let mut slot = AUDIO_PLAYBACK_STATE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|error| error.to_string())?;
    *slot = Some(AudioPlayback {
        queue,
        input_channels,
        stop_sender: Some(stop_sender),
        thread: Some(thread),
    });
    Ok(())
}

fn run_audio_playback_thread(
    queue: Arc<Mutex<VecDeque<f32>>>,
    input_channels: usize,
    stop_receiver: mpsc::Receiver<()>,
    ready_sender: Sender<Result<(), String>>,
) {
    let host = cpal::default_host();
    let Some(device) = host.default_output_device() else {
        let _ = ready_sender.send(Err("no default output device".into()));
        return;
    };
    let supported_config = match device.default_output_config() {
        Ok(config) => config,
        Err(error) => {
            let _ = ready_sender.send(Err(error.to_string()));
            return;
        }
    };
    let sample_format = supported_config.sample_format();
    let config = supported_config.config();
    let output_channels = usize::from(config.channels.max(1));
    let stream = match build_audio_output_stream(
        &device,
        &config,
        sample_format,
        queue,
        input_channels,
        output_channels,
    ) {
        Ok(stream) => stream,
        Err(error) => {
            let _ = ready_sender.send(Err(error));
            return;
        }
    };
    if let Err(error) = stream.play() {
        let _ = ready_sender.send(Err(error.to_string()));
        return;
    }
    let _ = ready_sender.send(Ok(()));
    let _ = stop_receiver.recv();
    drop(stream);
}

fn build_audio_output_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    queue: Arc<Mutex<VecDeque<f32>>>,
    input_channels: usize,
    output_channels: usize,
) -> Result<cpal::Stream, String> {
    let error_callback = |error| eprintln!("Rust audio output stream error: {error}");
    match sample_format {
        cpal::SampleFormat::F32 => device
            .build_output_stream(
                config,
                move |data: &mut [f32], _| {
                    write_audio_output(data, &queue, input_channels, output_channels)
                },
                error_callback,
                None,
            )
            .map_err(|error| error.to_string()),
        cpal::SampleFormat::I16 => device
            .build_output_stream(
                config,
                move |data: &mut [i16], _| {
                    write_audio_output(data, &queue, input_channels, output_channels)
                },
                error_callback,
                None,
            )
            .map_err(|error| error.to_string()),
        cpal::SampleFormat::U16 => device
            .build_output_stream(
                config,
                move |data: &mut [u16], _| {
                    write_audio_output(data, &queue, input_channels, output_channels)
                },
                error_callback,
                None,
            )
            .map_err(|error| error.to_string()),
        other => Err(format!("unsupported audio sample format {other:?}")),
    }
}

fn write_audio_output<T>(
    data: &mut [T],
    queue: &Arc<Mutex<VecDeque<f32>>>,
    input_channels: usize,
    output_channels: usize,
) where
    T: cpal::Sample + cpal::FromSample<f32>,
{
    let Ok(mut queue) = queue.lock() else {
        for sample in data {
            *sample = T::from_sample(0.0);
        }
        return;
    };

    let mut input_frame = vec![0.0_f32; input_channels];
    for output_frame in data.chunks_mut(output_channels) {
        for sample in &mut input_frame {
            *sample = queue.pop_front().unwrap_or(0.0);
        }
        for (channel, sample) in output_frame.iter_mut().enumerate() {
            let source = if channel < input_channels {
                input_frame[channel]
            } else {
                input_frame[0]
            };
            *sample = T::from_sample(source);
        }
    }
}

fn queue_audio_playback(samples: &[f32]) {
    let Ok(slot) = AUDIO_PLAYBACK_STATE.get_or_init(|| Mutex::new(None)).lock() else {
        return;
    };
    let Some(playback) = slot.as_ref() else {
        return;
    };
    let Ok(mut queue) = playback.queue.lock() else {
        return;
    };

    let max_queued_samples = playback.input_channels.saturating_mul(48_000);
    if queue.len() > max_queued_samples {
        queue.clear();
    }
    queue.extend(samples.iter().copied());
}

fn clear_audio_playback() {
    if let Ok(mut slot) = AUDIO_PLAYBACK_STATE.get_or_init(|| Mutex::new(None)).lock() {
        *slot = None;
    }
}

#[cfg(moonlight_common_c_linked)]
fn initialize_opus_audio_decoder(
    opus_config: &gamestream_sys::OpusMultistreamConfiguration,
) -> Result<(), String> {
    let mut error = 0;
    // SAFETY: Opus reads the fixed mapping array during construction only.
    let decoder = unsafe {
        gamestream_sys::opus_multistream_decoder_create(
            opus_config.sample_rate,
            opus_config.channel_count,
            opus_config.streams,
            opus_config.coupled_streams,
            opus_config.mapping.as_ptr(),
            &mut error,
        )
    };
    if decoder.is_null() {
        return Err(format!(
            "opus_multistream_decoder_create failed with {error}"
        ));
    }

    let channel_count = usize::try_from(opus_config.channel_count)
        .map_err(|_| format!("invalid channel count {}", opus_config.channel_count))?;
    let decoder = OpusAudioDecoder {
        decoder,
        channel_count,
        samples_per_frame: opus_config.samples_per_frame,
    };
    let mut slot = AUDIO_DECODER_STATE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|error| error.to_string())?;
    *slot = Some(decoder);
    Ok(())
}

#[cfg(moonlight_common_c_linked)]
fn clear_opus_audio_decoder() {
    if let Ok(mut slot) = AUDIO_DECODER_STATE.get_or_init(|| Mutex::new(None)).lock() {
        *slot = None;
    }
}

#[cfg(moonlight_common_c_linked)]
fn decode_opus_audio_packet(packet: &[u8]) -> Result<Vec<f32>, String> {
    let mut slot = AUDIO_DECODER_STATE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|error| error.to_string())?;
    let decoder = slot
        .as_mut()
        .ok_or_else(|| "decoder is not initialized".to_string())?;
    let samples_per_frame = usize::try_from(decoder.samples_per_frame)
        .map_err(|_| format!("invalid samples per frame {}", decoder.samples_per_frame))?;
    let mut pcm = vec![0.0_f32; samples_per_frame.saturating_mul(decoder.channel_count)];
    // SAFETY: decoder is owned by AUDIO_DECODER_STATE, packet and pcm are valid for the call.
    let samples = unsafe {
        gamestream_sys::opus_multistream_decode_float(
            decoder.decoder,
            packet.as_ptr(),
            c_int::try_from(packet.len()).unwrap_or(c_int::MAX),
            pcm.as_mut_ptr(),
            decoder.samples_per_frame,
            0,
        )
    };
    if samples < 0 {
        return Err(format!(
            "opus_multistream_decode_float failed with {samples}"
        ));
    }

    let sample_count = usize::try_from(samples).unwrap_or_default();
    pcm.truncate(sample_count.saturating_mul(decoder.channel_count));
    Ok(pcm)
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
