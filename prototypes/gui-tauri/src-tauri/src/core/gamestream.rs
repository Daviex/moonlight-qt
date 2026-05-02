#![allow(dead_code)]

use super::error::CoreError;
use super::events::{BridgeEvent, BridgeEventKind};
use super::gamestream_sys;
#[cfg(all(moonlight_common_c_linked, target_os = "windows"))]
use super::hardware_decoder;
use super::stream_input::{
    ButtonAction, ControllerCapabilities, ControllerState, ControllerType, KeyAction, KeyModifiers,
    MouseButton as StreamMouseButton, StreamInputSender,
};
use super::stream_renderer::StreamRendererPlan;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
#[cfg(moonlight_common_c_linked)]
use std::ffi::CStr;
use std::ffi::CString;
#[cfg(moonlight_common_c_linked)]
use std::os::raw::c_uchar;
use std::os::raw::{c_char, c_int, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

mod logger {
    pub fn log(message: impl AsRef<str>) {
        crate::logger::stream(message);
    }
}

const DEFAULT_VIDEO_PACKET_SIZE: u32 = 1392;
const NATIVE_VIDEO_INPUT_POLL_TIMEOUT: Duration = Duration::from_millis(1);
const NATIVE_VIDEO_DIAGNOSTIC_INTERVAL: Duration = Duration::from_secs(2);
const SDL3_CONTROLLER_DIAGNOSTIC_INTERVAL: Duration = Duration::from_secs(2);
const SDL_SOFTWARE_RENDERER_VIDEO_FORMATS: c_int = gamestream_sys::VIDEO_FORMAT_H264
    | gamestream_sys::VIDEO_FORMAT_H265
    | gamestream_sys::VIDEO_FORMAT_AV1_MAIN8;
const VIDEO_CODEC_CONFIG_AUTO: c_int = 0;
const VIDEO_CODEC_CONFIG_FORCE_H264: c_int = 1;
const VIDEO_CODEC_CONFIG_FORCE_HEVC: c_int = 2;
const VIDEO_CODEC_CONFIG_FORCE_AV1: c_int = 4;

// Decoder capability bits
const DECODER_CAP_SLICES_MASK: u32 = 0x0F;
const DECODER_CAP_HEVC_RFI: u32 = 0x10;
const DECODER_CAP_AV1_RFI: u32 = 0x20;
const DECODER_CAP_PULL_THREAD: u32 = 0x40;

static STREAM_EVENT_CONTEXT: OnceLock<Mutex<Option<StreamEventContext>>> = OnceLock::new();
static AUDIO_SINK_STATE: OnceLock<Mutex<AudioSinkState>> = OnceLock::new();
#[cfg(moonlight_common_c_linked)]
static AUDIO_DECODER_STATE: OnceLock<Mutex<Option<OpusAudioDecoder>>> = OnceLock::new();
static AUDIO_PLAYBACK_STATE: OnceLock<Mutex<Option<AudioPlayback>>> = OnceLock::new();
static VIDEO_SINK_STATE: OnceLock<Mutex<VideoSinkState>> = OnceLock::new();
#[cfg(moonlight_common_c_linked)]
static VIDEO_DECODER_STATE: OnceLock<Mutex<Option<SoftwareVideoDecoder>>> = OnceLock::new();
#[cfg(all(moonlight_common_c_linked, target_os = "windows"))]
static HARDWARE_DECODER_STATE: OnceLock<Mutex<Option<hardware_decoder::D3D11HardwareDecoder>>> =
    OnceLock::new();
#[cfg(all(moonlight_common_c_linked, target_os = "windows"))]
static GPU_SYNC_STATE: OnceLock<Mutex<Option<hardware_decoder::GpuSync>>> = OnceLock::new();
#[cfg(moonlight_common_c_linked)]
static VIDEO_DECODER_THREAD: OnceLock<Mutex<Option<VideoDecoderThread>>> = OnceLock::new();
static VIDEO_RENDERER_STATE: OnceLock<Mutex<Option<NativeVideoRenderer>>> = OnceLock::new();
static VIDEO_QUEUE_DIAGNOSTICS: OnceLock<Mutex<NativeVideoQueueDiagnostics>> = OnceLock::new();
static VIDEO_DECODE_DIAGNOSTICS: OnceLock<Mutex<NativeVideoDecodeDiagnostics>> = OnceLock::new();

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

/// Get decoder capabilities based on CPU count and platform
/// This matches the behavior of FFmpegVideoDecoder::getDecoderCapabilities()
fn get_decoder_capabilities() -> u32 {
    // Check environment variable override first
    if let Ok(caps_str) = std::env::var("DECODER_CAPS") {
        if let Ok(caps) = u32::from_str_radix(&caps_str, 16) {
            logger::log(format!(
                "Using decoder capability override: 0x{:x}",
                caps
            ));
            return caps;
        }
    }

    // For software FFmpeg decoder (CPU-based):
    // - Calculate parallel decode slices based on CPU core count (max 4)
    // - Enable HEVC Reference Frame Invalidation (RFI)
    // - Enable AV1 RFI
    // - Mark that we use pull-model rendering thread

    let cpu_count = num_cpus::get() as u32;
    let slices = cpu_count.min(4);

    let mut capabilities = slices & DECODER_CAP_SLICES_MASK;
    capabilities |= DECODER_CAP_HEVC_RFI;
    capabilities |= DECODER_CAP_AV1_RFI;
    capabilities |= DECODER_CAP_PULL_THREAD;

    logger::log(format!(
        "Decoder capabilities: slices={}; hevc_rfi=true; av1_rfi=true; pull_thread=true; raw=0x{:x}",
        slices, capabilities
    ));

    capabilities
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
    last_decoded_frame: Option<DecodedVideoFrame>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum DecodedVideoFrame {
    Rgba(RgbaVideoFrame),
    Yuv420(Yuv420VideoFrame),
    Nv12(NvVideoFrame),
    Nv21(NvVideoFrame),
}

impl DecodedVideoFrame {
    fn width(&self) -> c_int {
        match self {
            Self::Rgba(frame) => frame.width,
            Self::Yuv420(frame) => frame.width,
            Self::Nv12(frame) | Self::Nv21(frame) => frame.width,
        }
    }

    fn height(&self) -> c_int {
        match self {
            Self::Rgba(frame) => frame.height,
            Self::Yuv420(frame) => frame.height,
            Self::Nv12(frame) | Self::Nv21(frame) => frame.height,
        }
    }

    fn frame_number(&self) -> c_int {
        match self {
            Self::Rgba(frame) => frame.frame_number,
            Self::Yuv420(frame) => frame.frame_number,
            Self::Nv12(frame) | Self::Nv21(frame) => frame.frame_number,
        }
    }

    fn decoded_at(&self) -> Instant {
        match self {
            Self::Rgba(frame) => frame.decoded_at,
            Self::Yuv420(frame) => frame.decoded_at,
            Self::Nv12(frame) | Self::Nv21(frame) => frame.decoded_at,
        }
    }

    fn texture_format(&self) -> Sdl3VideoTextureFormat {
        match self {
            Self::Rgba(_) => Sdl3VideoTextureFormat::Rgba,
            Self::Yuv420(_) => Sdl3VideoTextureFormat::Yuv420,
            Self::Nv12(_) => Sdl3VideoTextureFormat::Nv12,
            Self::Nv21(_) => Sdl3VideoTextureFormat::Nv21,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RgbaVideoFrame {
    width: c_int,
    height: c_int,
    frame_number: c_int,
    decoded_at: Instant,
    pixels: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Yuv420VideoFrame {
    width: c_int,
    height: c_int,
    frame_number: c_int,
    decoded_at: Instant,
    y: VideoPlane,
    u: VideoPlane,
    v: VideoPlane,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NvVideoFrame {
    width: c_int,
    height: c_int,
    frame_number: c_int,
    decoded_at: Instant,
    y: VideoPlane,
    uv: VideoPlane,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VideoPlane {
    pixels: Vec<u8>,
    pitch: usize,
}

struct NativeVideoRenderer {
    frame_slot: Arc<LatestVideoFrameSlot>,
    stop_sender: Option<Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(moonlight_common_c_linked)]
struct VideoDecoderThread {
    should_stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

struct LatestVideoFrameSlot {
    frame: Mutex<Option<DecodedVideoFrame>>,
    available: Condvar,
    accepting_frames: AtomicBool,
}

impl Default for LatestVideoFrameSlot {
    fn default() -> Self {
        Self {
            frame: Mutex::new(None),
            available: Condvar::new(),
            accepting_frames: AtomicBool::new(true),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Sdl3VideoTextureFormat {
    Rgba,
    Yuv420,
    Nv12,
    Nv21,
}

struct Sdl3VideoTexture<'a> {
    texture: sdl3::render::Texture<'a>,
    width: usize,
    height: usize,
    format: Sdl3VideoTextureFormat,
}

struct NativeVideoQueueDiagnostics {
    started_at: Instant,
    last_log_at: Instant,
    submitted_frames: u64,
    queued_frames: u64,
    replaced_stale_frames: u64,
    disconnected_frames: u64,
    last_frame_number: c_int,
}

struct NativeVideoDecodeDiagnostics {
    started_at: Instant,
    last_log_at: Instant,
    decode_units: u64,
    decoded_frames: u64,
    bytes_received: u64,
    total_decode_us: u128,
    max_decode_us: u128,
    last_frame_number: c_int,
    missing_decode_units: u64,
    max_decode_unit_gap: c_int,
    rgba_frames: u64,
    yuv420_frames: u64,
    nv12_frames: u64,
    nv21_frames: u64,
}

struct NativeVideoRenderDiagnostics {
    started_at: Instant,
    last_log_at: Instant,
    displayed_frames: u64,
    recreated_textures: u64,
    total_update_us: u128,
    total_render_us: u128,
    max_update_us: u128,
    max_render_us: u128,
    total_frame_age_us: u128,
    max_frame_age_us: u128,
    skipped_frame_numbers: u64,
    stale_queue_frames: u64,
    max_frame_gap: c_int,
    last_frame_number: c_int,
    rgba_frames: u64,
    yuv420_frames: u64,
    nv12_frames: u64,
    nv21_frames: u64,
}

#[derive(Debug, Eq, PartialEq)]
struct VideoSinkUpdate {
    first_frame: bool,
    first_decoded_frame: bool,
    frame_number: c_int,
    bytes_received: u64,
}

#[derive(Debug, Default)]
struct NativeVideoInputState {
    last_mouse_position: Option<(i16, i16)>,
    left_mouse_down: bool,
    middle_mouse_down: bool,
    right_mouse_down: bool,
}

struct Sdl3Controller {
    _gamepad: sdl3::gamepad::Gamepad,
    controller_number: u8,
    state: ControllerState,
}

struct Sdl3ControllerManager {
    subsystem: sdl3::GamepadSubsystem,
    controllers: HashMap<u32, Sdl3Controller>,
    axis_events: u64,
    button_events: u64,
    unknown_events: u64,
    last_event_log_at: Instant,
    last_unknown_log_at: Instant,
}

impl Default for NativeVideoQueueDiagnostics {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            started_at: now,
            last_log_at: now,
            submitted_frames: 0,
            queued_frames: 0,
            replaced_stale_frames: 0,
            disconnected_frames: 0,
            last_frame_number: 0,
        }
    }
}

impl Default for NativeVideoDecodeDiagnostics {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            started_at: now,
            last_log_at: now,
            decode_units: 0,
            decoded_frames: 0,
            bytes_received: 0,
            total_decode_us: 0,
            max_decode_us: 0,
            last_frame_number: 0,
            missing_decode_units: 0,
            max_decode_unit_gap: 0,
            rgba_frames: 0,
            yuv420_frames: 0,
            nv12_frames: 0,
            nv21_frames: 0,
        }
    }
}

impl NativeVideoRenderDiagnostics {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            started_at: now,
            last_log_at: now,
            displayed_frames: 0,
            recreated_textures: 0,
            total_update_us: 0,
            total_render_us: 0,
            max_update_us: 0,
            max_render_us: 0,
            total_frame_age_us: 0,
            max_frame_age_us: 0,
            skipped_frame_numbers: 0,
            stale_queue_frames: 0,
            max_frame_gap: 0,
            last_frame_number: 0,
            rgba_frames: 0,
            yuv420_frames: 0,
            nv12_frames: 0,
            nv21_frames: 0,
        }
    }

    fn record_frame(
        &mut self,
        frame_number: c_int,
        format: Sdl3VideoTextureFormat,
        update_us: u128,
        render_us: u128,
        frame_age_us: u128,
        stale_queue_frames: u64,
    ) {
        if self.last_frame_number > 0 && frame_number > self.last_frame_number + 1 {
            let gap = frame_number - self.last_frame_number - 1;
            self.skipped_frame_numbers = self.skipped_frame_numbers.saturating_add(gap as u64);
            self.max_frame_gap = self.max_frame_gap.max(gap);
        }
        self.displayed_frames = self.displayed_frames.saturating_add(1);
        match format {
            Sdl3VideoTextureFormat::Rgba => {
                self.rgba_frames = self.rgba_frames.saturating_add(1);
            }
            Sdl3VideoTextureFormat::Yuv420 => {
                self.yuv420_frames = self.yuv420_frames.saturating_add(1);
            }
            Sdl3VideoTextureFormat::Nv12 => {
                self.nv12_frames = self.nv12_frames.saturating_add(1);
            }
            Sdl3VideoTextureFormat::Nv21 => {
                self.nv21_frames = self.nv21_frames.saturating_add(1);
            }
        }
        self.last_frame_number = frame_number;
        self.total_update_us = self.total_update_us.saturating_add(update_us);
        self.total_render_us = self.total_render_us.saturating_add(render_us);
        self.max_update_us = self.max_update_us.max(update_us);
        self.max_render_us = self.max_render_us.max(render_us);
        self.total_frame_age_us = self.total_frame_age_us.saturating_add(frame_age_us);
        self.max_frame_age_us = self.max_frame_age_us.max(frame_age_us);
        self.stale_queue_frames = self.stale_queue_frames.saturating_add(stale_queue_frames);
    }

    fn maybe_log(
        &mut self,
        texture_width: usize,
        texture_height: usize,
        canvas: &sdl3::render::WindowCanvas,
    ) {
        if self.last_log_at.elapsed() < NATIVE_VIDEO_DIAGNOSTIC_INTERVAL {
            return;
        }
        let elapsed = self.started_at.elapsed().as_secs_f64().max(0.001);
        let average_update_us = if self.displayed_frames == 0 {
            0
        } else {
            self.total_update_us / self.displayed_frames as u128
        };
        let average_render_us = if self.displayed_frames == 0 {
            0
        } else {
            self.total_render_us / self.displayed_frames as u128
        };
        let average_frame_age_us = if self.displayed_frames == 0 {
            0
        } else {
            self.total_frame_age_us / self.displayed_frames as u128
        };
        let output_size = canvas
            .output_size()
            .map(|(width, height)| format!("{width}x{height}"))
            .unwrap_or_else(|error| format!("unavailable:{error}"));
        logger::log(format!(
            "SDL3 video diagnostics: displayed={}; fps={:.1}; texture={}x{}; output={}; texture_recreates={}; rgba_frames={}; yuv420_frames={}; nv12_frames={}; nv21_frames={}; last_frame={}; skipped_frame_numbers={}; stale_queue_frames={}; max_frame_gap={}; avg_frame_age_us={}; max_frame_age_us={}; avg_update_us={}; max_update_us={}; avg_render_us={}; max_render_us={}",
            self.displayed_frames,
            self.displayed_frames as f64 / elapsed,
            texture_width,
            texture_height,
            output_size,
            self.recreated_textures,
            self.rgba_frames,
            self.yuv420_frames,
            self.nv12_frames,
            self.nv21_frames,
            self.last_frame_number,
            self.skipped_frame_numbers,
            self.stale_queue_frames,
            self.max_frame_gap,
            average_frame_age_us,
            self.max_frame_age_us,
            average_update_us,
            self.max_update_us,
            average_render_us,
            self.max_render_us,
        ));
        self.last_log_at = Instant::now();
    }
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
impl Drop for VideoDecoderThread {
    fn drop(&mut self) {
        self.should_stop.store(true, Ordering::Release);
        // SAFETY: Wakes the Limelight pull-renderer wait so the decoder thread can observe should_stop.
        unsafe { gamestream_sys::LiWakeWaitForVideoFrame() };
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
        submit_decode_unit: None,
        capabilities: native_software_video_capabilities(),
    }
}

fn native_software_video_capabilities() -> c_int {
    let slices = std::thread::available_parallelism()
        .map(|parallelism| parallelism.get().min(4) as u8)
        .unwrap_or(1);
    gamestream_sys::CAPABILITY_PULL_RENDERER
        | gamestream_sys::CAPABILITY_REFERENCE_FRAME_INVALIDATION_HEVC
        | gamestream_sys::CAPABILITY_REFERENCE_FRAME_INVALIDATION_AV1
        | gamestream_sys::capability_slices_per_frame(slices)
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
    {
        // Try hardware decoding first (D3D11 on Windows)
        #[cfg(target_os = "windows")]
        let hw_decoder_result = hardware_decoder::create_complete_hardware_decoder(
            width as u32,
            height as u32,
        );

        #[cfg(target_os = "windows")]
        if let Ok((decoder, sync)) = hw_decoder_result {
            if let Ok(mut slot) = HARDWARE_DECODER_STATE.get_or_init(|| Mutex::new(None)).lock() {
                *slot = Some(decoder);
            }
            if let Ok(mut slot) = GPU_SYNC_STATE.get_or_init(|| Mutex::new(None)).lock() {
                *slot = Some(sync);
            }
            emit_stream_event(
                BridgeEventKind::Status,
                format!("D3D11VA hardware decoder initialized for {width}x{height}@{redraw_rate}."),
            );
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
            start_native_video_renderer(width, height);
            emit_stream_event(
                BridgeEventKind::Status,
                format!("Headless video sink configured for {width}x{height}@{redraw_rate} with hardware decoding."),
            );
            return gamestream_sys::DR_OK;
        }

        #[cfg(target_os = "windows")]
        {
            emit_stream_event(
                BridgeEventKind::Status,
                "D3D11VA hardware decoder unavailable, falling back to software decoding.".into(),
            );
        }
    }

    // Fallback to software decoding
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
    #[cfg(moonlight_common_c_linked)]
    start_video_decoder_thread();
    emit_stream_event(
        BridgeEventKind::Status,
        "Headless video sink started.".into(),
    );
}

unsafe extern "C" fn headless_video_stop() {
    #[cfg(moonlight_common_c_linked)]
    stop_video_decoder_thread();
    store_video_sink_state(|state| {
        state.started = false;
    });
    emit_stream_event(
        BridgeEventKind::Status,
        "Headless video sink stopped.".into(),
    );
}

unsafe extern "C" fn headless_video_cleanup() {
    #[cfg(moonlight_common_c_linked)]
    stop_video_decoder_thread();
    
    // Cleanup hardware decoder
    #[cfg(all(moonlight_common_c_linked, target_os = "windows"))]
    {
        if let Ok(mut slot) = HARDWARE_DECODER_STATE.get_or_init(|| Mutex::new(None)).lock() {
            *slot = None;
        }
        if let Ok(mut slot) = GPU_SYNC_STATE.get_or_init(|| Mutex::new(None)).lock() {
            *slot = None;
        }
        logger::log("D3D11 hardware decoder released");
    }
    
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

#[cfg(moonlight_common_c_linked)]
fn start_video_decoder_thread() {
    stop_video_decoder_thread();
    let decoder = VIDEO_DECODER_STATE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|mut slot| slot.take());
    let Some(decoder) = decoder else {
        logger::log("Video decoder thread was not started because no decoder is configured");
        return;
    };
    let should_stop = Arc::new(AtomicBool::new(false));
    let thread_should_stop = Arc::clone(&should_stop);
    let thread = std::thread::spawn(move || video_decoder_thread_loop(decoder, thread_should_stop));
    if let Ok(mut slot) = VIDEO_DECODER_THREAD.get_or_init(|| Mutex::new(None)).lock() {
        *slot = Some(VideoDecoderThread {
            should_stop,
            thread: Some(thread),
        });
    }
    logger::log("Rust video decoder thread started with moonlight-common-c pull renderer");
}

#[cfg(moonlight_common_c_linked)]
fn stop_video_decoder_thread() {
    if let Ok(mut slot) = VIDEO_DECODER_THREAD.get_or_init(|| Mutex::new(None)).lock() {
        if slot.is_some() {
            logger::log("Stopping Rust video decoder thread");
        }
        *slot = None;
    }
}

#[cfg(moonlight_common_c_linked)]
fn video_decoder_thread_loop(mut decoder: SoftwareVideoDecoder, should_stop: Arc<AtomicBool>) {
    while !should_stop.load(Ordering::Acquire) {
        let mut frame_handle: *mut c_void = std::ptr::null_mut();
        let mut decode_unit: *mut gamestream_sys::DecodeUnit = std::ptr::null_mut();
        // SAFETY: Output pointers are valid stack locals. Limelight owns the returned decode unit
        // until LiCompleteVideoFrame() is called below.
        let got_frame =
            unsafe { gamestream_sys::LiWaitForNextVideoFrame(&mut frame_handle, &mut decode_unit) };
        if !got_frame {
            continue;
        }
        if should_stop.load(Ordering::Acquire) {
            // SAFETY: frame_handle was returned by LiWaitForNextVideoFrame and must be completed once.
            unsafe { gamestream_sys::LiCompleteVideoFrame(frame_handle, gamestream_sys::DR_OK) };
            break;
        }
        let status = process_pull_video_decode_unit(&mut decoder, decode_unit);
        // SAFETY: frame_handle was returned by LiWaitForNextVideoFrame and must be completed once.
        unsafe { gamestream_sys::LiCompleteVideoFrame(frame_handle, status) };
    }
    logger::log("Rust video decoder thread stopped");
}

#[cfg(moonlight_common_c_linked)]
fn process_pull_video_decode_unit(
    decoder: &mut SoftwareVideoDecoder,
    decode_unit: *mut gamestream_sys::DecodeUnit,
) -> c_int {
    if decode_unit.is_null() {
        return gamestream_sys::DR_OK;
    }

    // SAFETY: Limelight owns the decode unit for the duration of this callback.
    let decode_unit = unsafe { &*decode_unit };
    let bytes_received = decode_unit_bytes(decode_unit);
    let payload = unsafe { copy_decode_unit_payload(decode_unit) };
    let decode_start = Instant::now();
    let decoded_frame = decode_pull_video_payload(decoder, &payload, decode_unit.frame_number);
    let decode_us = decode_start.elapsed().as_micros();
    let decoded = decoded_frame.is_some();
    let decoded_format = decoded_frame
        .as_ref()
        .map(DecodedVideoFrame::texture_format);
    record_native_video_decode_diagnostics(
        decode_unit.frame_number,
        bytes_received,
        decoded,
        decoded_format,
        decode_us,
    );
    let update = update_video_sink_after_decode(
        decode_unit.frame_number,
        bytes_received,
        payload,
        decoded_frame,
    );
    emit_video_sink_update_events(&update);
    gamestream_sys::DR_OK
}

#[cfg(moonlight_common_c_linked)]
fn decode_pull_video_payload(
    decoder: &mut SoftwareVideoDecoder,
    payload: &[u8],
    frame_number: c_int,
) -> Option<DecodedVideoFrame> {
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

fn update_video_sink_after_decode(
    frame_number: c_int,
    bytes_received: u64,
    payload: Vec<u8>,
    decoded_frame: Option<DecodedVideoFrame>,
) -> VideoSinkUpdate {
    let mut update = VideoSinkUpdate {
        first_frame: false,
        first_decoded_frame: false,
        frame_number,
        bytes_received,
    };
    store_video_sink_state(|state| {
        update.first_frame = state.frames_received == 0;
        state.frames_received = state.frames_received.saturating_add(1);
        state.bytes_received = state.bytes_received.saturating_add(bytes_received);
        state.last_frame_number = frame_number;
        state.last_frame_payload = payload;
        if let Some(frame) = decoded_frame {
            update.first_decoded_frame = state.decoded_frames == 0;
            state.decoded_frames = state.decoded_frames.saturating_add(1);
            send_native_video_frame(frame.clone());
            state.last_decoded_frame = Some(frame);
        }
    });
    update
}

fn emit_video_sink_update_events(update: &VideoSinkUpdate) {
    if update.first_frame {
        emit_stream_event(
            BridgeEventKind::Status,
            format!(
                "Headless video sink received its first frame {} ({} bytes).",
                update.frame_number, update.bytes_received
            ),
        );
    }
    if update.first_decoded_frame {
        emit_stream_event(
            BridgeEventKind::Status,
            "First decoded video frame is ready for native presentation.".into(),
        );
    }
}

fn start_native_video_renderer(width: c_int, height: c_int) {
    stop_native_video_renderer();
    let width = width.max(1) as usize;
    let height = height.max(1) as usize;
    reset_native_video_diagnostics();
    logger::log(format!(
        "starting native video renderer; width={width}; height={height}"
    ));
    let frame_slot = Arc::new(LatestVideoFrameSlot::default());
    let (stop_sender, stop_receiver) = mpsc::channel();
    let thread_frame_slot = Arc::clone(&frame_slot);
    let thread = std::thread::spawn(move || {
        if let Err(error) =
            native_video_renderer_loop(width, height, thread_frame_slot, stop_receiver)
        {
            emit_stream_event(
                BridgeEventKind::Status,
                format!("Native video renderer stopped: {error}."),
            );
        }
    });
    if let Ok(mut slot) = VIDEO_RENDERER_STATE.get_or_init(|| Mutex::new(None)).lock() {
        *slot = Some(NativeVideoRenderer {
            frame_slot,
            stop_sender: Some(stop_sender),
            thread: Some(thread),
        });
    }
}

fn reset_native_video_diagnostics() {
    if let Ok(mut diagnostics) = VIDEO_QUEUE_DIAGNOSTICS
        .get_or_init(|| Mutex::new(NativeVideoQueueDiagnostics::default()))
        .lock()
    {
        *diagnostics = NativeVideoQueueDiagnostics::default();
    }
    if let Ok(mut diagnostics) = VIDEO_DECODE_DIAGNOSTICS
        .get_or_init(|| Mutex::new(NativeVideoDecodeDiagnostics::default()))
        .lock()
    {
        *diagnostics = NativeVideoDecodeDiagnostics::default();
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

fn send_native_video_frame(frame: DecodedVideoFrame) {
    let frame_number = frame.frame_number();
    if let Ok(slot) = VIDEO_RENDERER_STATE.get_or_init(|| Mutex::new(None)).lock() {
        if let Some(renderer) = slot.as_ref() {
            if !renderer.frame_slot.accepting_frames.load(Ordering::Acquire) {
                record_native_video_queue_diagnostics(frame_number, VideoQueueResult::Disconnected);
                return;
            }
            let Ok(mut pending_frame) = renderer.frame_slot.frame.lock() else {
                record_native_video_queue_diagnostics(frame_number, VideoQueueResult::Disconnected);
                return;
            };
            if !renderer.frame_slot.accepting_frames.load(Ordering::Acquire) {
                record_native_video_queue_diagnostics(frame_number, VideoQueueResult::Disconnected);
                return;
            }
            let result = if pending_frame.replace(frame).is_some() {
                VideoQueueResult::ReplacedStale
            } else {
                VideoQueueResult::Queued
            };
            renderer.frame_slot.available.notify_one();
            record_native_video_queue_diagnostics(frame_number, result);
        }
    }
}

enum VideoQueueResult {
    Queued,
    ReplacedStale,
    Disconnected,
}

fn record_native_video_queue_diagnostics(frame_number: c_int, result: VideoQueueResult) {
    let Ok(mut diagnostics) = VIDEO_QUEUE_DIAGNOSTICS
        .get_or_init(|| Mutex::new(NativeVideoQueueDiagnostics::default()))
        .lock()
    else {
        return;
    };
    diagnostics.submitted_frames = diagnostics.submitted_frames.saturating_add(1);
    diagnostics.last_frame_number = frame_number;
    match result {
        VideoQueueResult::Queued => {
            diagnostics.queued_frames = diagnostics.queued_frames.saturating_add(1);
        }
        VideoQueueResult::ReplacedStale => {
            diagnostics.replaced_stale_frames = diagnostics.replaced_stale_frames.saturating_add(1);
        }
        VideoQueueResult::Disconnected => {
            diagnostics.disconnected_frames = diagnostics.disconnected_frames.saturating_add(1);
        }
    }
    if diagnostics.last_log_at.elapsed() >= NATIVE_VIDEO_DIAGNOSTIC_INTERVAL {
        let elapsed = diagnostics.started_at.elapsed().as_secs_f64().max(0.001);
        logger::log(format!(
            "SDL3 video queue diagnostics: submitted={}; queued={}; replaced_stale={}; disconnected={}; replace_pct={:.1}; input_fps={:.1}; last_frame={}",
            diagnostics.submitted_frames,
            diagnostics.queued_frames,
            diagnostics.replaced_stale_frames,
            diagnostics.disconnected_frames,
            diagnostics.replaced_stale_frames as f64 * 100.0 / diagnostics.submitted_frames.max(1) as f64,
            diagnostics.submitted_frames as f64 / elapsed,
            diagnostics.last_frame_number,
        ));
        diagnostics.last_log_at = Instant::now();
    }
}

fn record_native_video_decode_diagnostics(
    frame_number: c_int,
    bytes_received: u64,
    decoded: bool,
    decoded_format: Option<Sdl3VideoTextureFormat>,
    decode_us: u128,
) {
    let Ok(mut diagnostics) = VIDEO_DECODE_DIAGNOSTICS
        .get_or_init(|| Mutex::new(NativeVideoDecodeDiagnostics::default()))
        .lock()
    else {
        return;
    };
    diagnostics.decode_units = diagnostics.decode_units.saturating_add(1);
    diagnostics.bytes_received = diagnostics.bytes_received.saturating_add(bytes_received);
    diagnostics.total_decode_us = diagnostics.total_decode_us.saturating_add(decode_us);
    diagnostics.max_decode_us = diagnostics.max_decode_us.max(decode_us);
    if diagnostics.last_frame_number > 0 && frame_number > diagnostics.last_frame_number + 1 {
        let gap = frame_number - diagnostics.last_frame_number - 1;
        diagnostics.missing_decode_units =
            diagnostics.missing_decode_units.saturating_add(gap as u64);
        diagnostics.max_decode_unit_gap = diagnostics.max_decode_unit_gap.max(gap);
    }
    diagnostics.last_frame_number = frame_number;
    if decoded {
        diagnostics.decoded_frames = diagnostics.decoded_frames.saturating_add(1);
    }
    match decoded_format {
        Some(Sdl3VideoTextureFormat::Rgba) => {
            diagnostics.rgba_frames = diagnostics.rgba_frames.saturating_add(1);
        }
        Some(Sdl3VideoTextureFormat::Yuv420) => {
            diagnostics.yuv420_frames = diagnostics.yuv420_frames.saturating_add(1);
        }
        Some(Sdl3VideoTextureFormat::Nv12) => {
            diagnostics.nv12_frames = diagnostics.nv12_frames.saturating_add(1);
        }
        Some(Sdl3VideoTextureFormat::Nv21) => {
            diagnostics.nv21_frames = diagnostics.nv21_frames.saturating_add(1);
        }
        None => {}
    }
    if diagnostics.last_log_at.elapsed() >= NATIVE_VIDEO_DIAGNOSTIC_INTERVAL {
        let elapsed = diagnostics.started_at.elapsed().as_secs_f64().max(0.001);
        let average_decode_us = if diagnostics.decode_units == 0 {
            0
        } else {
            diagnostics.total_decode_us / diagnostics.decode_units as u128
        };
        logger::log(format!(
            "FFmpeg software decode diagnostics: units={}; decoded={}; decode_ratio_pct={:.1}; unit_fps={:.1}; decoded_fps={:.1}; rgba_frames={}; yuv420_frames={}; nv12_frames={}; nv21_frames={}; missing_units={}; max_unit_gap={}; avg_decode_us={}; max_decode_us={}; mb_received={:.1}; last_frame={}",
            diagnostics.decode_units,
            diagnostics.decoded_frames,
            diagnostics.decoded_frames as f64 * 100.0 / diagnostics.decode_units.max(1) as f64,
            diagnostics.decode_units as f64 / elapsed,
            diagnostics.decoded_frames as f64 / elapsed,
            diagnostics.rgba_frames,
            diagnostics.yuv420_frames,
            diagnostics.nv12_frames,
            diagnostics.nv21_frames,
            diagnostics.missing_decode_units,
            diagnostics.max_decode_unit_gap,
            average_decode_us,
            diagnostics.max_decode_us,
            diagnostics.bytes_received as f64 / (1024.0 * 1024.0),
            diagnostics.last_frame_number,
        ));
        diagnostics.last_log_at = Instant::now();
    }
}

fn native_video_renderer_loop(
    width: usize,
    height: usize,
    frame_slot: Arc<LatestVideoFrameSlot>,
    stop_receiver: mpsc::Receiver<()>,
) -> Result<(), String> {
    logger::log(format!(
        "SDL3 native video renderer creating window; width={width}; height={height}"
    ));
    let sdl = sdl3::init().map_err(|error| error.to_string())?;
    let video = sdl.video().map_err(|error| error.to_string())?;
    let input_sender = StreamInputSender;
    let mut controllers = match sdl.gamepad() {
        Ok(gamepad) => Some(Sdl3ControllerManager::new(gamepad, &input_sender)),
        Err(error) => {
            logger::log(format!("SDL3 gamepad subsystem unavailable: {error}"));
            None
        }
    };

    let window = video
        .window("Moonlight Stream", width as u32, height as u32)
        .position_centered()
        .resizable()
        .build()
        .map_err(|error| error.to_string())?;
    let mut canvas = window.into_canvas();
    let texture_creator = canvas.texture_creator();
    disable_sdl3_renderer_vsync(&canvas);
    let mut video_texture = create_sdl3_video_texture(
        &texture_creator,
        width,
        height,
        Sdl3VideoTextureFormat::Rgba,
    )?;
    sdl.mouse().show_cursor(false);
    sdl.mouse().capture(true);
    sdl.mouse().set_relative_mouse_mode(canvas.window(), true);
    logger::log("SDL3 native video renderer window created");

    let mut event_pump = sdl.event_pump().map_err(|error| error.to_string())?;
    let mut requested_stop = false;
    let mut diagnostics = NativeVideoRenderDiagnostics::new();

    'running: loop {
        if stop_receiver.try_recv().is_ok() {
            requested_stop = true;
            break;
        }
        let mut pending_controller_axis_updates = Vec::new();
        for event in event_pump.poll_iter() {
            if handle_sdl3_video_event(
                event,
                &input_sender,
                video_texture.width,
                video_texture.height,
                &mut canvas,
                &mut controllers,
                &mut pending_controller_axis_updates,
            ) {
                break 'running;
            }
        }
        flush_sdl3_controller_axis_updates(
            &mut controllers,
            &input_sender,
            pending_controller_axis_updates,
        );
        match receive_latest_video_frame(&frame_slot) {
            Some(frame) => {
                validate_decoded_video_frame(&frame)?;
                let frame_width = frame.width() as usize;
                let frame_height = frame.height() as usize;
                let frame_format = frame.texture_format();
                if frame_width != video_texture.width
                    || frame_height != video_texture.height
                    || frame_format != video_texture.format
                {
                    video_texture = create_sdl3_video_texture(
                        &texture_creator,
                        frame_width,
                        frame_height,
                        frame_format,
                    )?;
                    diagnostics.recreated_textures =
                        diagnostics.recreated_textures.saturating_add(1);
                    logger::log(format!(
                        "SDL3 video texture recreated; width={}; height={}; format={:?}; frame={}",
                        video_texture.width,
                        video_texture.height,
                        video_texture.format,
                        frame.frame_number()
                    ));
                }
                let update_start = Instant::now();
                update_sdl3_video_texture(&mut video_texture.texture, &frame)
                    .map_err(|error| error.to_string())?;
                let update_us = update_start.elapsed().as_micros();
                let render_start = Instant::now();
                render_sdl3_video_frame(
                    &mut canvas,
                    &video_texture.texture,
                    video_texture.width,
                    video_texture.height,
                )?;
                let render_us = render_start.elapsed().as_micros();
                let frame_age_us = frame.decoded_at().elapsed().as_micros();
                diagnostics.record_frame(
                    frame.frame_number(),
                    frame_format,
                    update_us,
                    render_us,
                    frame_age_us,
                    0,
                );
                diagnostics.maybe_log(video_texture.width, video_texture.height, &canvas);
            }
            None => {}
        }
    }

    sdl.mouse().set_relative_mouse_mode(canvas.window(), false);
    sdl.mouse().capture(false);
    sdl.mouse().show_cursor(true);
    frame_slot.accepting_frames.store(false, Ordering::Release);
    frame_slot.available.notify_all();
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

fn receive_latest_video_frame(frame_slot: &LatestVideoFrameSlot) -> Option<DecodedVideoFrame> {
    let Ok(mut pending_frame) = frame_slot.frame.lock() else {
        return None;
    };
    if pending_frame.is_none() {
        let Ok((next_pending_frame, _)) = frame_slot
            .available
            .wait_timeout(pending_frame, NATIVE_VIDEO_INPUT_POLL_TIMEOUT)
        else {
            return None;
        };
        pending_frame = next_pending_frame;
    }
    pending_frame.take()
}

fn create_sdl3_video_texture<'a>(
    texture_creator: &'a sdl3::render::TextureCreator<sdl3::video::WindowContext>,
    width: usize,
    height: usize,
    format: Sdl3VideoTextureFormat,
) -> Result<Sdl3VideoTexture<'a>, String> {
    let pixel_format = match format {
        Sdl3VideoTextureFormat::Rgba => sdl3::pixels::PixelFormat::RGBA32,
        Sdl3VideoTextureFormat::Yuv420 => sdl3::pixels::PixelFormat::IYUV,
        Sdl3VideoTextureFormat::Nv12 => sdl3::pixels::PixelFormat::NV12,
        Sdl3VideoTextureFormat::Nv21 => sdl3::pixels::PixelFormat::NV21,
    };
    let texture = texture_creator
        .create_texture_streaming(pixel_format, width as u32, height as u32)
        .map_err(|error| error.to_string())?;
    Ok(Sdl3VideoTexture {
        texture,
        width,
        height,
        format,
    })
}

fn update_sdl3_video_texture(
    texture: &mut sdl3::render::Texture<'_>,
    frame: &DecodedVideoFrame,
) -> Result<(), String> {
    match frame {
        DecodedVideoFrame::Rgba(frame) => texture
            .update(None, &frame.pixels, frame.width as usize * 4)
            .map_err(|error| error.to_string()),
        DecodedVideoFrame::Yuv420(frame) => texture
            .update_yuv(
                None,
                &frame.y.pixels,
                frame.y.pitch,
                &frame.u.pixels,
                frame.u.pitch,
                &frame.v.pixels,
                frame.v.pitch,
            )
            .map_err(|error| error.to_string()),
        DecodedVideoFrame::Nv12(frame) | DecodedVideoFrame::Nv21(frame) => {
            update_sdl3_nv_texture(texture, &frame.y, &frame.uv)
        }
    }
}

fn update_sdl3_nv_texture(
    texture: &mut sdl3::render::Texture<'_>,
    y: &VideoPlane,
    uv: &VideoPlane,
) -> Result<(), String> {
    let y_pitch = c_int::try_from(y.pitch).map_err(|_| "NV12 Y pitch overflows int")?;
    let uv_pitch = c_int::try_from(uv.pitch).map_err(|_| "NV12 UV pitch overflows int")?;
    let updated = unsafe {
        sdl3::sys::render::SDL_UpdateNVTexture(
            texture.raw(),
            std::ptr::null(),
            y.pixels.as_ptr(),
            y_pitch,
            uv.pixels.as_ptr(),
            uv_pitch,
        )
    };
    if updated {
        Ok(())
    } else {
        Err(sdl3::get_error().to_string())
    }
}

fn disable_sdl3_renderer_vsync(canvas: &sdl3::render::WindowCanvas) {
    // The stream already arrives paced by GameStream; an extra renderer vsync wait makes input feel delayed.
    let _ = unsafe {
        sdl3::sys::render::SDL_SetRenderVSync(
            canvas.raw(),
            sdl3::sys::render::SDL_RENDERER_VSYNC_DISABLED,
        )
    };
}

fn load_sdl3_game_controller_mappings(gamepad: &sdl3::GamepadSubsystem) {
    for path in sdl3_controller_mapping_candidates() {
        if path.is_file() {
            match gamepad.load_mappings(&path) {
                Ok(count) => logger::log(format!(
                    "loaded {count} SDL3 game controller mappings from {}",
                    path.display()
                )),
                Err(error) => logger::log(format!(
                    "failed to load SDL3 game controller mappings from {}: {error}",
                    path.display()
                )),
            }
            return;
        }
    }
    logger::log("SDL3 game controller mapping database was not found");
}

impl Sdl3ControllerManager {
    fn new(subsystem: sdl3::GamepadSubsystem, input: &StreamInputSender) -> Self {
        load_sdl3_game_controller_mappings(&subsystem);
        let now = Instant::now();
        let mut manager = Self {
            subsystem,
            controllers: HashMap::new(),
            axis_events: 0,
            button_events: 0,
            unknown_events: 0,
            last_event_log_at: now,
            last_unknown_log_at: now,
        };
        match manager.subsystem.gamepads() {
            Ok(gamepads) => {
                logger::log(format!(
                    "SDL3 gamepad diagnostics: enumerated {} gamepad(s)",
                    gamepads.len()
                ));
                for gamepad_id in gamepads {
                    manager.open_controller(gamepad_id, input);
                }
            }
            Err(error) => logger::log(format!("failed to enumerate SDL3 gamepads: {error}")),
        }
        manager
    }

    fn open_controller(
        &mut self,
        gamepad_id: sdl3::joystick::JoystickId,
        input: &StreamInputSender,
    ) {
        let gamepad_key = u32::from(gamepad_id);
        if self.controllers.contains_key(&gamepad_key) {
            return;
        }
        let Some(controller_number) = self.first_available_controller_number() else {
            logger::log(format!(
                "ignoring SDL3 gamepad {gamepad_key}: maximum controller count reached"
            ));
            return;
        };
        let gamepad = match self.subsystem.open(gamepad_id) {
            Ok(gamepad) => gamepad,
            Err(error) => {
                logger::log(format!(
                    "failed to open SDL3 gamepad {gamepad_key}: {error}"
                ));
                return;
            }
        };
        let controller_type = sdl3_gamepad_type(gamepad.r#type());
        let sdl3_type = format!("{:?}", gamepad.r#type());
        let name = gamepad.name().unwrap_or_else(|| "Unknown gamepad".into());
        let state = ControllerState {
            controller_number,
            active_gamepad_mask: 0,
            ..ControllerState::default()
        };
        self.controllers.insert(
            gamepad_key,
            Sdl3Controller {
                _gamepad: gamepad,
                controller_number,
                state,
            },
        );
        self.update_active_masks();
        let active_gamepad_mask = self.active_gamepad_mask();
        if let Err(error) = input.send_controller_arrival(
            controller_number,
            active_gamepad_mask,
            controller_type,
            SDL3_CONTROLLER_SUPPORTED_BUTTONS as u32,
            ControllerCapabilities {
                analog_triggers: true,
                rumble: true,
                trigger_rumble: false,
                touchpad: true,
                accelerometer: false,
                gyroscope: false,
                battery_state: false,
                rgb_led: false,
            },
        ) {
            logger::log(format!(
                "SDL3 gamepad diagnostics: controller arrival send failed; id={gamepad_key}; controller={controller_number}; error={error}"
            ));
        }
        if let Some(controller) = self.controllers.get(&gamepad_key) {
            if let Err(error) = input.send_controller(controller.state) {
                logger::log(format!(
                    "SDL3 gamepad diagnostics: initial controller state send failed; id={gamepad_key}; controller={controller_number}; error={error}"
                ));
            }
        }
        logger::log(format!(
            "SDL3 gamepad diagnostics: opened id={gamepad_key}; controller={controller_number}; name={name}; sdl_type={sdl3_type:?}; mapped_type={controller_type:?}; active_mask={active_gamepad_mask:#x}"
        ));
    }

    fn remove_controller(&mut self, gamepad_id: u32, input: &StreamInputSender) {
        let Some(removed) = self.controllers.remove(&gamepad_id) else {
            return;
        };
        self.update_active_masks();
        if let Err(error) = input.send_controller(ControllerState {
            controller_number: removed.controller_number,
            active_gamepad_mask: self.active_gamepad_mask(),
            ..ControllerState::default()
        }) {
            logger::log(format!(
                "SDL3 gamepad diagnostics: removal state send failed; id={gamepad_id}; controller={}; error={error}",
                removed.controller_number
            ));
        }
        logger::log(format!(
            "SDL3 gamepad diagnostics: removed id={gamepad_id}; controller={}",
            removed.controller_number
        ));
    }

    fn handle_axis(
        &mut self,
        gamepad_id: u32,
        axis: sdl3::gamepad::Axis,
        value: i16,
    ) -> Option<u32> {
        self.axis_events = self.axis_events.saturating_add(1);
        let Some(controller) = self.controllers.get_mut(&gamepad_id) else {
            self.record_unknown_event(format!(
                "axis event for unopened id={gamepad_id}; axis={axis:?}; value={value}"
            ));
            return None;
        };
        match axis {
            sdl3::gamepad::Axis::LeftX => controller.state.left_stick_x = value,
            sdl3::gamepad::Axis::LeftY => {
                controller.state.left_stick_y = invert_sdl3_stick_axis(value)
            }
            sdl3::gamepad::Axis::RightX => controller.state.right_stick_x = value,
            sdl3::gamepad::Axis::RightY => {
                controller.state.right_stick_y = invert_sdl3_stick_axis(value)
            }
            sdl3::gamepad::Axis::TriggerLeft => {
                controller.state.left_trigger = sdl3_trigger_to_u8(value)
            }
            sdl3::gamepad::Axis::TriggerRight => {
                controller.state.right_trigger = sdl3_trigger_to_u8(value)
            }
        }
        self.maybe_log_events();
        Some(gamepad_id)
    }

    fn send_controller_state(&mut self, gamepad_id: u32, input: &StreamInputSender, reason: &str) {
        let Some(controller) = self.controllers.get(&gamepad_id) else {
            return;
        };
        if let Err(error) = input.send_controller(controller.state) {
            logger::log(format!(
                "SDL3 gamepad diagnostics: {reason} state send failed; id={gamepad_id}; controller={}; error={error}",
                controller.controller_number
            ));
        }
    }

    fn handle_button(
        &mut self,
        gamepad_id: u32,
        button: sdl3::gamepad::Button,
        pressed: bool,
        input: &StreamInputSender,
    ) {
        self.button_events = self.button_events.saturating_add(1);
        let Some(flag) = sdl3_gamepad_button_flag(button) else {
            self.record_unknown_event(format!(
                "unmapped button event; id={gamepad_id}; button={button:?}; pressed={pressed}"
            ));
            return;
        };
        let Some(controller) = self.controllers.get_mut(&gamepad_id) else {
            self.record_unknown_event(format!(
                "button event for unopened id={gamepad_id}; button={button:?}; pressed={pressed}"
            ));
            return;
        };
        if pressed {
            controller.state.button_flags |= flag;
        } else {
            controller.state.button_flags &= !flag;
        }
        let controller_number = controller.controller_number;
        let button_flags = controller.state.button_flags;
        self.send_controller_state(gamepad_id, input, "button");
        logger::log(format!(
            "SDL3 gamepad diagnostics: button event; id={gamepad_id}; controller={}; button={button:?}; pressed={pressed}; flags={:#x}",
            controller_number,
            button_flags,
        ));
        self.maybe_log_events();
    }

    fn record_unknown_event(&mut self, message: String) {
        self.unknown_events = self.unknown_events.saturating_add(1);
        if self.last_unknown_log_at.elapsed() >= SDL3_CONTROLLER_DIAGNOSTIC_INTERVAL {
            logger::log(format!(
                "SDL3 gamepad diagnostics: {message}; unknown_events={}",
                self.unknown_events
            ));
            self.last_unknown_log_at = Instant::now();
        }
    }

    fn maybe_log_events(&mut self) {
        if self.last_event_log_at.elapsed() < SDL3_CONTROLLER_DIAGNOSTIC_INTERVAL {
            return;
        }
        logger::log(format!(
            "SDL3 gamepad diagnostics: controllers={}; axis_events={}; button_events={}; unknown_events={}; active_mask={:#x}",
            self.controllers.len(),
            self.axis_events,
            self.button_events,
            self.unknown_events,
            self.active_gamepad_mask(),
        ));
        self.last_event_log_at = Instant::now();
    }

    fn first_available_controller_number(&self) -> Option<u8> {
        (0..4).find(|number| {
            self.controllers
                .values()
                .all(|controller| controller.controller_number != *number)
        })
    }

    fn active_gamepad_mask(&self) -> u16 {
        self.controllers.values().fold(0, |mask, controller| {
            mask | (1 << controller.controller_number)
        })
    }

    fn update_active_masks(&mut self) {
        let active_gamepad_mask = self.active_gamepad_mask();
        for controller in self.controllers.values_mut() {
            controller.state.active_gamepad_mask = active_gamepad_mask;
        }
    }
}

const SDL3_CONTROLLER_SUPPORTED_BUTTONS: i32 = gamestream_sys::A_FLAG
    | gamestream_sys::B_FLAG
    | gamestream_sys::X_FLAG
    | gamestream_sys::Y_FLAG
    | gamestream_sys::UP_FLAG
    | gamestream_sys::DOWN_FLAG
    | gamestream_sys::LEFT_FLAG
    | gamestream_sys::RIGHT_FLAG
    | gamestream_sys::LB_FLAG
    | gamestream_sys::RB_FLAG
    | gamestream_sys::PLAY_FLAG
    | gamestream_sys::BACK_FLAG
    | gamestream_sys::LS_CLK_FLAG
    | gamestream_sys::RS_CLK_FLAG
    | gamestream_sys::SPECIAL_FLAG
    | gamestream_sys::PADDLE1_FLAG
    | gamestream_sys::PADDLE2_FLAG
    | gamestream_sys::PADDLE3_FLAG
    | gamestream_sys::PADDLE4_FLAG
    | gamestream_sys::TOUCHPAD_FLAG
    | gamestream_sys::MISC_FLAG;

fn sdl3_gamepad_button_flag(button: sdl3::gamepad::Button) -> Option<i32> {
    match button {
        sdl3::gamepad::Button::South => Some(gamestream_sys::A_FLAG),
        sdl3::gamepad::Button::East => Some(gamestream_sys::B_FLAG),
        sdl3::gamepad::Button::West => Some(gamestream_sys::X_FLAG),
        sdl3::gamepad::Button::North => Some(gamestream_sys::Y_FLAG),
        sdl3::gamepad::Button::Back => Some(gamestream_sys::BACK_FLAG),
        sdl3::gamepad::Button::Guide => Some(gamestream_sys::SPECIAL_FLAG),
        sdl3::gamepad::Button::Start => Some(gamestream_sys::PLAY_FLAG),
        sdl3::gamepad::Button::LeftStick => Some(gamestream_sys::LS_CLK_FLAG),
        sdl3::gamepad::Button::RightStick => Some(gamestream_sys::RS_CLK_FLAG),
        sdl3::gamepad::Button::LeftShoulder => Some(gamestream_sys::LB_FLAG),
        sdl3::gamepad::Button::RightShoulder => Some(gamestream_sys::RB_FLAG),
        sdl3::gamepad::Button::DPadUp => Some(gamestream_sys::UP_FLAG),
        sdl3::gamepad::Button::DPadDown => Some(gamestream_sys::DOWN_FLAG),
        sdl3::gamepad::Button::DPadLeft => Some(gamestream_sys::LEFT_FLAG),
        sdl3::gamepad::Button::DPadRight => Some(gamestream_sys::RIGHT_FLAG),
        sdl3::gamepad::Button::Misc1 => Some(gamestream_sys::MISC_FLAG),
        sdl3::gamepad::Button::RightPaddle1 => Some(gamestream_sys::PADDLE1_FLAG),
        sdl3::gamepad::Button::LeftPaddle1 => Some(gamestream_sys::PADDLE2_FLAG),
        sdl3::gamepad::Button::RightPaddle2 => Some(gamestream_sys::PADDLE3_FLAG),
        sdl3::gamepad::Button::LeftPaddle2 => Some(gamestream_sys::PADDLE4_FLAG),
        sdl3::gamepad::Button::Touchpad => Some(gamestream_sys::TOUCHPAD_FLAG),
        _ => None,
    }
}

fn sdl3_trigger_to_u8(value: i16) -> u8 {
    (((value.max(0) as i32) * 255 + 16_383) / 32_767).clamp(0, 255) as u8
}

fn invert_sdl3_stick_axis(value: i16) -> i16 {
    value.saturating_neg()
}

fn sdl3_gamepad_type(gamepad_type: sdl3::gamepad::GamepadType) -> ControllerType {
    match gamepad_type {
        sdl3::gamepad::GamepadType::Xbox360 | sdl3::gamepad::GamepadType::XboxOne => {
            ControllerType::Xbox
        }
        sdl3::gamepad::GamepadType::PS3
        | sdl3::gamepad::GamepadType::PS4
        | sdl3::gamepad::GamepadType::PS5 => ControllerType::PlayStation,
        sdl3::gamepad::GamepadType::NintendoSwitchPro
        | sdl3::gamepad::GamepadType::NintendoSwitchJoyconLeft
        | sdl3::gamepad::GamepadType::NintendoSwitchJoyconRight
        | sdl3::gamepad::GamepadType::NintendoSwitchJoyconPair => ControllerType::Nintendo,
        _ => ControllerType::Xbox,
    }
}

fn sdl3_controller_mapping_candidates() -> Vec<std::path::PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join("gamecontrollerdb.txt"));
        candidates.push(
            current_dir
                .join("app")
                .join("SDL_GameControllerDB")
                .join("gamecontrollerdb.txt"),
        );
    }
    candidates.push(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("..")
            .join("app")
            .join("SDL_GameControllerDB")
            .join("gamecontrollerdb.txt"),
    );
    candidates
}

fn render_sdl3_video_frame(
    canvas: &mut sdl3::render::WindowCanvas,
    texture: &sdl3::render::Texture,
    frame_width: usize,
    frame_height: usize,
) -> Result<(), String> {
    let (output_width, output_height) = canvas.output_size().map_err(|error| error.to_string())?;
    let video_region = scaled_video_region(
        frame_width,
        frame_height,
        output_width as usize,
        output_height as usize,
    );
    canvas.clear();
    canvas
        .copy(
            texture,
            None,
            sdl3::render::FRect::new(
                video_region.x,
                video_region.y,
                video_region.width,
                video_region.height,
            ),
        )
        .map_err(|error| error.to_string())?;
    if !canvas.present() {
        return Err(sdl3::get_error().to_string());
    }
    Ok(())
}

fn handle_sdl3_video_event(
    event: sdl3::event::Event,
    input: &StreamInputSender,
    reference_width: usize,
    reference_height: usize,
    canvas: &mut sdl3::render::WindowCanvas,
    controllers: &mut Option<Sdl3ControllerManager>,
    pending_controller_axis_updates: &mut Vec<u32>,
) -> bool {
    match event {
        sdl3::event::Event::Quit { .. }
        | sdl3::event::Event::Window {
            win_event: sdl3::event::WindowEvent::CloseRequested,
            ..
        } => return true,
        sdl3::event::Event::MouseMotion {
            x, y, xrel, yrel, ..
        } => {
            if xrel != 0.0 || yrel != 0.0 {
                let _ = input.send_mouse_move(clamp_f32_to_i16(xrel), clamp_f32_to_i16(yrel));
            } else {
                send_sdl3_mouse_position(input, x, y, reference_width, reference_height, canvas);
            }
        }
        sdl3::event::Event::MouseButtonDown {
            mouse_btn, x, y, ..
        } => {
            send_sdl3_mouse_position(input, x, y, reference_width, reference_height, canvas);
            send_sdl3_mouse_button(input, mouse_btn, ButtonAction::Press);
        }
        sdl3::event::Event::MouseButtonUp {
            mouse_btn, x, y, ..
        } => {
            send_sdl3_mouse_position(input, x, y, reference_width, reference_height, canvas);
            send_sdl3_mouse_button(input, mouse_btn, ButtonAction::Release);
        }
        sdl3::event::Event::MouseWheel { x, y, .. } => {
            let scroll_x = (x * 120.0).round();
            let scroll_y = (y * 120.0).round();
            if scroll_x != 0.0 {
                let _ = input.send_high_res_horizontal_scroll(clamp_f32_to_i16(scroll_x));
            }
            if scroll_y != 0.0 {
                let _ = input.send_high_res_scroll(clamp_f32_to_i16(scroll_y));
            }
        }
        sdl3::event::Event::KeyDown {
            keycode,
            keymod,
            repeat,
            ..
        } => {
            if !repeat {
                if let Some(key_code) = keycode.and_then(sdl3_keycode_to_js_key_code) {
                    let _ = input.send_keyboard(
                        key_code,
                        KeyAction::Down,
                        sdl3_key_modifiers(keymod),
                        true,
                    );
                }
            }
        }
        sdl3::event::Event::KeyUp {
            keycode, keymod, ..
        } => {
            if let Some(key_code) = keycode.and_then(sdl3_keycode_to_js_key_code) {
                let _ =
                    input.send_keyboard(key_code, KeyAction::Up, sdl3_key_modifiers(keymod), true);
            }
        }
        sdl3::event::Event::ControllerDeviceAdded { which, .. } => {
            if let Some(controllers) = controllers.as_mut() {
                controllers.open_controller(sdl3::sys::joystick::SDL_JoystickID(which), input);
            }
        }
        sdl3::event::Event::ControllerDeviceRemoved { which, .. } => {
            if let Some(controllers) = controllers.as_mut() {
                controllers.remove_controller(which, input);
            }
        }
        sdl3::event::Event::ControllerAxisMotion {
            which, axis, value, ..
        } => {
            if let Some(controllers) = controllers.as_mut() {
                if let Some(gamepad_id) = controllers.handle_axis(which, axis, value) {
                    if !pending_controller_axis_updates.contains(&gamepad_id) {
                        pending_controller_axis_updates.push(gamepad_id);
                    }
                }
            }
        }
        sdl3::event::Event::ControllerButtonDown { which, button, .. } => {
            if let Some(controllers) = controllers.as_mut() {
                controllers.handle_button(which, button, true, input);
            }
        }
        sdl3::event::Event::ControllerButtonUp { which, button, .. } => {
            if let Some(controllers) = controllers.as_mut() {
                controllers.handle_button(which, button, false, input);
            }
        }
        _ => {}
    }
    false
}

fn send_sdl3_mouse_position(
    input: &StreamInputSender,
    x: f32,
    y: f32,
    reference_width: usize,
    reference_height: usize,
    canvas: &sdl3::render::WindowCanvas,
) {
    let Ok((output_width, output_height)) = canvas.output_size() else {
        return;
    };
    let video_region = scaled_video_region(
        reference_width,
        reference_height,
        output_width as usize,
        output_height as usize,
    );
    let x = clamp_f32_to_stream_i16(x - video_region.x, video_region.width);
    let y = clamp_f32_to_stream_i16(y - video_region.y, video_region.height);
    let _ = input.send_mouse_position(
        x,
        y,
        clamp_f32_to_i16(video_region.width),
        clamp_f32_to_i16(video_region.height),
    );
}

fn send_sdl3_mouse_button(
    input: &StreamInputSender,
    mouse_btn: sdl3::mouse::MouseButton,
    action: ButtonAction,
) {
    let button = match mouse_btn {
        sdl3::mouse::MouseButton::Left => StreamMouseButton::Left,
        sdl3::mouse::MouseButton::Middle => StreamMouseButton::Middle,
        sdl3::mouse::MouseButton::Right => StreamMouseButton::Right,
        sdl3::mouse::MouseButton::X1 => StreamMouseButton::X1,
        sdl3::mouse::MouseButton::X2 => StreamMouseButton::X2,
        sdl3::mouse::MouseButton::Unknown => return,
    };
    let _ = input.send_mouse_button(action, button);
}

fn flush_sdl3_controller_axis_updates(
    controllers: &mut Option<Sdl3ControllerManager>,
    input: &StreamInputSender,
    gamepad_ids: Vec<u32>,
) {
    let Some(controllers) = controllers.as_mut() else {
        return;
    };
    for gamepad_id in gamepad_ids {
        controllers.send_controller_state(gamepad_id, input, "batched axis");
    }
}

fn sdl3_key_modifiers(keymod: sdl3::keyboard::Mod) -> KeyModifiers {
    KeyModifiers {
        shift: keymod.intersects(sdl3::keyboard::Mod::LSHIFTMOD | sdl3::keyboard::Mod::RSHIFTMOD),
        ctrl: keymod.intersects(sdl3::keyboard::Mod::LCTRLMOD | sdl3::keyboard::Mod::RCTRLMOD),
        alt: keymod.intersects(sdl3::keyboard::Mod::LALTMOD | sdl3::keyboard::Mod::RALTMOD),
        meta: keymod.intersects(sdl3::keyboard::Mod::LGUIMOD | sdl3::keyboard::Mod::RGUIMOD),
    }
}

fn sdl3_keycode_to_js_key_code(key: sdl3::keyboard::Keycode) -> Option<i16> {
    let key_code = match key {
        sdl3::keyboard::Keycode::Backspace => 8,
        sdl3::keyboard::Keycode::Tab => 9,
        sdl3::keyboard::Keycode::Return | sdl3::keyboard::Keycode::KpEnter => 13,
        sdl3::keyboard::Keycode::LShift | sdl3::keyboard::Keycode::RShift => 16,
        sdl3::keyboard::Keycode::LCtrl | sdl3::keyboard::Keycode::RCtrl => 17,
        sdl3::keyboard::Keycode::LAlt | sdl3::keyboard::Keycode::RAlt => 18,
        sdl3::keyboard::Keycode::Pause => 19,
        sdl3::keyboard::Keycode::CapsLock => 20,
        sdl3::keyboard::Keycode::Escape => 27,
        sdl3::keyboard::Keycode::Space => 32,
        sdl3::keyboard::Keycode::PageUp => 33,
        sdl3::keyboard::Keycode::PageDown => 34,
        sdl3::keyboard::Keycode::End => 35,
        sdl3::keyboard::Keycode::Home => 36,
        sdl3::keyboard::Keycode::Left => 37,
        sdl3::keyboard::Keycode::Up => 38,
        sdl3::keyboard::Keycode::Right => 39,
        sdl3::keyboard::Keycode::Down => 40,
        sdl3::keyboard::Keycode::Insert => 45,
        sdl3::keyboard::Keycode::Delete => 46,
        sdl3::keyboard::Keycode::_0 => 48,
        sdl3::keyboard::Keycode::_1 => 49,
        sdl3::keyboard::Keycode::_2 => 50,
        sdl3::keyboard::Keycode::_3 => 51,
        sdl3::keyboard::Keycode::_4 => 52,
        sdl3::keyboard::Keycode::_5 => 53,
        sdl3::keyboard::Keycode::_6 => 54,
        sdl3::keyboard::Keycode::_7 => 55,
        sdl3::keyboard::Keycode::_8 => 56,
        sdl3::keyboard::Keycode::_9 => 57,
        sdl3::keyboard::Keycode::A => 65,
        sdl3::keyboard::Keycode::B => 66,
        sdl3::keyboard::Keycode::C => 67,
        sdl3::keyboard::Keycode::D => 68,
        sdl3::keyboard::Keycode::E => 69,
        sdl3::keyboard::Keycode::F => 70,
        sdl3::keyboard::Keycode::G => 71,
        sdl3::keyboard::Keycode::H => 72,
        sdl3::keyboard::Keycode::I => 73,
        sdl3::keyboard::Keycode::J => 74,
        sdl3::keyboard::Keycode::K => 75,
        sdl3::keyboard::Keycode::L => 76,
        sdl3::keyboard::Keycode::M => 77,
        sdl3::keyboard::Keycode::N => 78,
        sdl3::keyboard::Keycode::O => 79,
        sdl3::keyboard::Keycode::P => 80,
        sdl3::keyboard::Keycode::Q => 81,
        sdl3::keyboard::Keycode::R => 82,
        sdl3::keyboard::Keycode::S => 83,
        sdl3::keyboard::Keycode::T => 84,
        sdl3::keyboard::Keycode::U => 85,
        sdl3::keyboard::Keycode::V => 86,
        sdl3::keyboard::Keycode::W => 87,
        sdl3::keyboard::Keycode::X => 88,
        sdl3::keyboard::Keycode::Y => 89,
        sdl3::keyboard::Keycode::Z => 90,
        sdl3::keyboard::Keycode::LGui | sdl3::keyboard::Keycode::RGui => 91,
        sdl3::keyboard::Keycode::Menu => 93,
        sdl3::keyboard::Keycode::Kp0 => 96,
        sdl3::keyboard::Keycode::Kp1 => 97,
        sdl3::keyboard::Keycode::Kp2 => 98,
        sdl3::keyboard::Keycode::Kp3 => 99,
        sdl3::keyboard::Keycode::Kp4 => 100,
        sdl3::keyboard::Keycode::Kp5 => 101,
        sdl3::keyboard::Keycode::Kp6 => 102,
        sdl3::keyboard::Keycode::Kp7 => 103,
        sdl3::keyboard::Keycode::Kp8 => 104,
        sdl3::keyboard::Keycode::Kp9 => 105,
        sdl3::keyboard::Keycode::KpMultiply => 106,
        sdl3::keyboard::Keycode::KpPlus => 107,
        sdl3::keyboard::Keycode::KpMinus => 109,
        sdl3::keyboard::Keycode::KpPeriod => 110,
        sdl3::keyboard::Keycode::KpDivide => 111,
        sdl3::keyboard::Keycode::F1 => 112,
        sdl3::keyboard::Keycode::F2 => 113,
        sdl3::keyboard::Keycode::F3 => 114,
        sdl3::keyboard::Keycode::F4 => 115,
        sdl3::keyboard::Keycode::F5 => 116,
        sdl3::keyboard::Keycode::F6 => 117,
        sdl3::keyboard::Keycode::F7 => 118,
        sdl3::keyboard::Keycode::F8 => 119,
        sdl3::keyboard::Keycode::F9 => 120,
        sdl3::keyboard::Keycode::F10 => 121,
        sdl3::keyboard::Keycode::F11 => 122,
        sdl3::keyboard::Keycode::F12 => 123,
        sdl3::keyboard::Keycode::F13 => 124,
        sdl3::keyboard::Keycode::F14 => 125,
        sdl3::keyboard::Keycode::F15 => 126,
        sdl3::keyboard::Keycode::NumLockClear => 144,
        sdl3::keyboard::Keycode::ScrollLock => 145,
        sdl3::keyboard::Keycode::Semicolon => 186,
        sdl3::keyboard::Keycode::Equals => 187,
        sdl3::keyboard::Keycode::Comma => 188,
        sdl3::keyboard::Keycode::Minus => 189,
        sdl3::keyboard::Keycode::Period => 190,
        sdl3::keyboard::Keycode::Slash => 191,
        sdl3::keyboard::Keycode::Grave => 192,
        sdl3::keyboard::Keycode::LeftBracket => 219,
        sdl3::keyboard::Keycode::Backslash => 220,
        sdl3::keyboard::Keycode::RightBracket => 221,
        sdl3::keyboard::Keycode::Apostrophe => 222,
        sdl3::keyboard::Keycode::Unknown => return None,
        _ => return None,
    };
    Some(key_code)
}

fn clamp_f32_to_i16(value: f32) -> i16 {
    value.clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

fn clamp_f32_to_stream_i16(value: f32, reference: f32) -> i16 {
    value.clamp(0.0, reference.max(1.0)) as i16
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScaledVideoRegion {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

fn scaled_video_region(
    source_width: usize,
    source_height: usize,
    target_width: usize,
    target_height: usize,
) -> ScaledVideoRegion {
    let source_width = source_width.max(1) as f32;
    let source_height = source_height.max(1) as f32;
    let mut region = ScaledVideoRegion {
        x: 0.0,
        y: 0.0,
        width: target_width.max(1) as f32,
        height: target_height.max(1) as f32,
    };
    let scaled_height = (region.width * source_height / source_width).ceil();
    let scaled_width = (region.height * source_width / source_height).ceil();

    if scaled_height > region.height {
        region.x += (region.width - scaled_width) / 2.0;
        region.width = scaled_width;
    } else {
        region.y += (region.height - scaled_height) / 2.0;
        region.height = scaled_height;
    }

    region
}

fn validate_decoded_video_frame(frame: &DecodedVideoFrame) -> Result<(), String> {
    if frame.width() <= 0 || frame.height() <= 0 {
        return Err("decoded frame has invalid dimensions".into());
    }
    match frame {
        DecodedVideoFrame::Rgba(frame) => {
            let pixel_count = frame.width as usize * frame.height as usize;
            if frame.pixels.len() < pixel_count * 4 {
                return Err("decoded RGBA frame buffer is shorter than expected".into());
            }
        }
        DecodedVideoFrame::Yuv420(frame) => {
            validate_video_plane(&frame.y, frame.height as usize, "Y")?;
            validate_video_plane(&frame.u, frame.height as usize / 2, "U")?;
            validate_video_plane(&frame.v, frame.height as usize / 2, "V")?;
        }
        DecodedVideoFrame::Nv12(frame) | DecodedVideoFrame::Nv21(frame) => {
            validate_video_plane(&frame.y, frame.height as usize, "Y")?;
            validate_video_plane(&frame.uv, frame.height as usize / 2, "UV")?;
        }
    }
    Ok(())
}

fn validate_video_plane(plane: &VideoPlane, rows: usize, name: &str) -> Result<(), String> {
    let expected_len = plane.pitch.saturating_mul(rows);
    if plane.pixels.len() < expected_len {
        return Err(format!(
            "decoded {name} plane is shorter than expected: {} < {expected_len}",
            plane.pixels.len()
        ));
    }
    Ok(())
}

#[cfg(moonlight_common_c_linked)]
fn configure_ffmpeg_decoder_threading(codec_context: *mut gamestream_sys::AVCodecContext) {
    static THREADS_OPTION: &[u8] = b"threads\0";
    static THREAD_TYPE_OPTION: &[u8] = b"thread_type\0";
    static FLAGS_OPTION: &[u8] = b"flags\0";
    static FLAGS2_OPTION: &[u8] = b"flags2\0";
    static ERR_DETECT_OPTION: &[u8] = b"err_detect\0";

    if set_ffmpeg_decoder_int_option(codec_context, THREADS_OPTION, 0, "thread count") {
        logger::log("FFmpeg decoder threading configured with automatic thread count");
    }
    if set_ffmpeg_decoder_int_option(
        codec_context,
        THREAD_TYPE_OPTION,
        gamestream_sys::FF_THREAD_SLICE.into(),
        "slice threading",
    ) {
        logger::log("FFmpeg decoder slice threading requested");
    }
    if set_ffmpeg_decoder_int_option(
        codec_context,
        FLAGS_OPTION,
        (gamestream_sys::AV_CODEC_FLAG_LOW_DELAY | gamestream_sys::AV_CODEC_FLAG_OUTPUT_CORRUPT)
            .into(),
        "low-delay flags",
    ) {
        logger::log("FFmpeg decoder low-delay flags requested");
    }
    if set_ffmpeg_decoder_int_option(
        codec_context,
        FLAGS2_OPTION,
        gamestream_sys::AV_CODEC_FLAG2_SHOW_ALL.into(),
        "show-all flag",
    ) {
        logger::log("FFmpeg decoder show-all flag requested");
    }
    if set_ffmpeg_decoder_int_option(
        codec_context,
        ERR_DETECT_OPTION,
        gamestream_sys::AV_EF_EXPLODE.into(),
        "error detection",
    ) {
        logger::log("FFmpeg decoder error detection configured");
    }
}

#[cfg(moonlight_common_c_linked)]
fn set_ffmpeg_decoder_int_option(
    codec_context: *mut gamestream_sys::AVCodecContext,
    option: &[u8],
    value: i64,
    description: &str,
) -> bool {
    // SAFETY: codec_context is a newly allocated FFmpeg AVCodecContext and option is NUL-terminated.
    let result = unsafe {
        gamestream_sys::av_opt_set_int(codec_context.cast(), option.as_ptr().cast(), value, 0)
    };
    if result < 0 {
        logger::log(format!(
            "FFmpeg decoder {description} option was rejected; code={result}"
        ));
        return false;
    }
    true
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
        configure_ffmpeg_decoder_threading(codec_context);

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
    ) -> Result<Option<DecodedVideoFrame>, String> {
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
            last_frame = Some(self.copy_current_frame(frame_number)?);
            // SAFETY: frame is owned by this decoder and may be reused after unref.
            unsafe { gamestream_sys::av_frame_unref(self.frame) };
        }

        Ok(last_frame)
    }

    fn copy_current_frame(&mut self, frame_number: c_int) -> Result<DecodedVideoFrame, String> {
        // SAFETY: frame is currently filled by avcodec_receive_frame.
        let frame = unsafe { &*self.frame };
        let width = frame.width;
        let height = frame.height;
        let source_format = frame.format;
        if width <= 0 || height <= 0 {
            return Err("FFmpeg returned a decoded frame with invalid dimensions".into());
        }
        match source_format {
            gamestream_sys::AV_PIX_FMT_YUV420P | gamestream_sys::AV_PIX_FMT_YUVJ420P => {
                return copy_yuv420_frame(frame, frame_number);
            }
            gamestream_sys::AV_PIX_FMT_NV12 => {
                return copy_nv_frame(frame, frame_number, false);
            }
            gamestream_sys::AV_PIX_FMT_NV21 => {
                return copy_nv_frame(frame, frame_number, true);
            }
            _ => {}
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
                    gamestream_sys::SWS_FAST_BILINEAR,
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

        Ok(DecodedVideoFrame::Rgba(RgbaVideoFrame {
            width,
            height,
            frame_number,
            decoded_at: Instant::now(),
            pixels,
        }))
    }
}

#[cfg(moonlight_common_c_linked)]
fn copy_yuv420_frame(
    frame: &gamestream_sys::AVFrame,
    frame_number: c_int,
) -> Result<DecodedVideoFrame, String> {
    let width = frame.width as usize;
    let height = frame.height as usize;
    let chroma_width = width / 2;
    let chroma_height = height / 2;
    Ok(DecodedVideoFrame::Yuv420(Yuv420VideoFrame {
        width: frame.width,
        height: frame.height,
        frame_number,
        decoded_at: Instant::now(),
        y: copy_av_frame_plane(frame.data[0], frame.linesize[0], width, height, "Y")?,
        u: copy_av_frame_plane(
            frame.data[1],
            frame.linesize[1],
            chroma_width,
            chroma_height,
            "U",
        )?,
        v: copy_av_frame_plane(
            frame.data[2],
            frame.linesize[2],
            chroma_width,
            chroma_height,
            "V",
        )?,
    }))
}

#[cfg(moonlight_common_c_linked)]
fn copy_nv_frame(
    frame: &gamestream_sys::AVFrame,
    frame_number: c_int,
    nv21: bool,
) -> Result<DecodedVideoFrame, String> {
    let width = frame.width as usize;
    let height = frame.height as usize;
    let decoded_at = Instant::now();
    let frame = NvVideoFrame {
        width: frame.width,
        height: frame.height,
        frame_number,
        decoded_at,
        y: copy_av_frame_plane(frame.data[0], frame.linesize[0], width, height, "Y")?,
        uv: copy_av_frame_plane(frame.data[1], frame.linesize[1], width, height / 2, "UV")?,
    };
    if nv21 {
        Ok(DecodedVideoFrame::Nv21(frame))
    } else {
        Ok(DecodedVideoFrame::Nv12(frame))
    }
}

#[cfg(moonlight_common_c_linked)]
fn copy_av_frame_plane(
    data: *mut c_uchar,
    linesize: c_int,
    row_bytes: usize,
    rows: usize,
    name: &str,
) -> Result<VideoPlane, String> {
    if data.is_null() {
        return Err(format!("FFmpeg returned a null {name} plane"));
    }
    if linesize < row_bytes as c_int {
        return Err(format!(
            "FFmpeg returned a short {name} linesize: {linesize} < {row_bytes}"
        ));
    }
    let stride = linesize as usize;
    let mut pixels = vec![0; row_bytes.saturating_mul(rows)];
    for row in 0..rows {
        let source_offset = row.saturating_mul(stride);
        let target_offset = row.saturating_mul(row_bytes);
        unsafe {
            std::ptr::copy_nonoverlapping(
                data.add(source_offset),
                pixels.as_mut_ptr().add(target_offset),
                row_bytes,
            );
        }
    }
    Ok(VideoPlane {
        pixels,
        pitch: row_bytes,
    })
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
fn decode_video_payload(payload: &[u8], frame_number: c_int) -> Option<DecodedVideoFrame> {
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

    pub fn from_settings_for_renderer(
        settings: &crate::core::types::StreamingSettings,
        renderer: &StreamRendererPlan,
    ) -> Self {
        stream_configuration_from_settings(settings, renderer.supports_hdr_formats())
    }

    pub fn preferred_for_server(mut self, server_codec_modes: c_int) -> Self {
        // First, find and log the preferred codec
        let Some(preferred_format) =
            preferred_available_video_format(self.supported_video_formats, server_codec_modes)
        else {
            logger::log(format!(
                "No preferred codec match found; keeping supported formats=0x{:x}; server_modes=0x{server_codec_modes:x}",
                self.supported_video_formats
            ));
            return self;
        };
        
        if preferred_format != self.supported_video_formats {
            logger::log(format!(
                "Selected preferred stream codec; requested_formats=0x{:x}; server_modes=0x{server_codec_modes:x}; selected=0x{preferred_format:x}",
                self.supported_video_formats
            ));
        }
        
        // Filter to server-supported formats while preserving the full bitmask of what's available.
        // This preserves HDR format information for hdr_query_parameters().
        let supported_by_server = self.supported_video_formats & server_codec_modes;
        if supported_by_server != 0 && supported_by_server != self.supported_video_formats {
            self.supported_video_formats = supported_by_server;
        }
        
        self
    }
    
    pub fn prefer_hdr_codecs_if_requested(mut self, enable_hdr: bool) -> Self {
        if !enable_hdr {
            return self;
        }

        const HDR_10BIT_CODECS: &[c_int] = &[
            gamestream_sys::VIDEO_FORMAT_AV1_HIGH10_444,
            gamestream_sys::VIDEO_FORMAT_AV1_MAIN10,
            gamestream_sys::VIDEO_FORMAT_HEVC_REXT10_444,
            gamestream_sys::VIDEO_FORMAT_H265_MAIN10,
        ];

        // Find the first (highest priority) 10-bit codec available
        if let Some(&preferred) = HDR_10BIT_CODECS
            .iter()
            .find(|&&codec| self.supported_video_formats & codec != 0)
        {
            logger::log(format!(
                "HDR enabled; locking to 10-bit codec 0x{:x} (was 0x{:x})",
                preferred, self.supported_video_formats
            ));
            self.supported_video_formats = preferred;
        }

        self
    }
}

impl From<&crate::core::types::StreamingSettings> for StreamConfiguration {
    fn from(settings: &crate::core::types::StreamingSettings) -> Self {
        stream_configuration_from_settings(settings, false)
    }
}

fn stream_configuration_from_settings(
    settings: &crate::core::types::StreamingSettings,
    supports_hdr_formats: bool,
) -> StreamConfiguration {
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
    let requested_formats = requested_video_formats_for_settings(settings, supports_hdr_formats);
    let supported_video_formats = if supports_hdr_formats {
        requested_formats
    } else {
        sdl_software_renderer_video_formats(requested_formats, settings.enable_hdr)
    };

    StreamConfiguration {
        width: settings.width,
        height: settings.height,
        fps: settings.fps,
        bitrate_kbps: settings.bitrate_kbps,
        packet_size,
        streaming_remotely,
        audio_configuration: AudioConfiguration::from_raw(settings.audio_config),
        supported_video_formats,
        remote_input_crypto: RemoteInputCrypto::default(),
    }
    .prefer_hdr_codecs_if_requested(settings.enable_hdr)
}

fn requested_video_formats_for_settings(
    settings: &crate::core::types::StreamingSettings,
    supports_hdr_formats: bool,
) -> c_int {
    match settings.video_codec_config {
        VIDEO_CODEC_CONFIG_FORCE_H264 => gamestream_sys::VIDEO_FORMAT_H264,
        VIDEO_CODEC_CONFIG_FORCE_HEVC => {
            hevc_video_formats_for_renderer(settings, supports_hdr_formats)
        }
        VIDEO_CODEC_CONFIG_FORCE_AV1 => {
            // Match native intent: forced AV1 may fall back to HEVC before H.264 if AV1 is unavailable.
            av1_video_formats_for_renderer(settings, supports_hdr_formats)
                | hevc_video_formats_for_renderer(settings, supports_hdr_formats)
        }
        VIDEO_CODEC_CONFIG_AUTO => automatic_video_formats_for_renderer(supports_hdr_formats),
        raw_formats => raw_formats,
    }
}

fn automatic_video_formats_for_renderer(supports_hdr_formats: bool) -> c_int {
    if supports_hdr_formats {
        gamestream_sys::VIDEO_FORMAT_AV1_HIGH10_444
            | gamestream_sys::VIDEO_FORMAT_AV1_MAIN10
            | gamestream_sys::VIDEO_FORMAT_AV1_HIGH8_444
            | gamestream_sys::VIDEO_FORMAT_AV1_MAIN8
            | gamestream_sys::VIDEO_FORMAT_HEVC_REXT10_444
            | gamestream_sys::VIDEO_FORMAT_H265_MAIN10
            | gamestream_sys::VIDEO_FORMAT_HEVC_REXT8_444
            | gamestream_sys::VIDEO_FORMAT_H265
            | gamestream_sys::VIDEO_FORMAT_H264_HIGH8_444
            | gamestream_sys::VIDEO_FORMAT_H264
    } else {
        SDL_SOFTWARE_RENDERER_VIDEO_FORMATS
    }
}

fn av1_video_formats_for_renderer(
    settings: &crate::core::types::StreamingSettings,
    supports_hdr_formats: bool,
) -> c_int {
    let mut formats = gamestream_sys::VIDEO_FORMAT_AV1_MAIN8;
    if supports_hdr_formats && settings.enable_yuv444 {
        formats |= gamestream_sys::VIDEO_FORMAT_AV1_HIGH8_444;
    }
    if supports_hdr_formats && settings.enable_hdr {
        formats |= gamestream_sys::VIDEO_FORMAT_AV1_MAIN10;
        if settings.enable_yuv444 {
            formats |= gamestream_sys::VIDEO_FORMAT_AV1_HIGH10_444;
        }
    }
    formats
}

fn hevc_video_formats_for_renderer(
    settings: &crate::core::types::StreamingSettings,
    supports_hdr_formats: bool,
) -> c_int {
    let mut formats = gamestream_sys::VIDEO_FORMAT_H265;
    if supports_hdr_formats && settings.enable_yuv444 {
        formats |= gamestream_sys::VIDEO_FORMAT_HEVC_REXT8_444;
    }
    if supports_hdr_formats && settings.enable_hdr {
        formats |= gamestream_sys::VIDEO_FORMAT_H265_MAIN10;
        if settings.enable_yuv444 {
            formats |= gamestream_sys::VIDEO_FORMAT_HEVC_REXT10_444;
        }
    }
    formats
}

fn sdl_software_renderer_video_formats(requested_formats: c_int, hdr_requested: bool) -> c_int {
    let requested_formats = if requested_formats == 0 {
        gamestream_sys::VIDEO_FORMAT_H264
    } else {
        requested_formats
    };
    let filtered_formats = requested_formats & SDL_SOFTWARE_RENDERER_VIDEO_FORMATS;
    let supported_formats = if filtered_formats == 0 {
        gamestream_sys::VIDEO_FORMAT_H264
    } else {
        filtered_formats
    };
    if supported_formats != requested_formats || hdr_requested {
        logger::log(format!(
            "SDL software renderer filtered requested video formats; requested=0x{requested_formats:x}; supported=0x{supported_formats:x}; hdr_requested={hdr_requested}; reason=no 10-bit/HDR/YUV444 presenter yet"
        ));
    }
    supported_formats
}

fn preferred_available_video_format(
    requested_formats: c_int,
    server_codec_modes: c_int,
) -> Option<c_int> {
    const PREFERRED_FORMATS: [c_int; 10] = [
        gamestream_sys::VIDEO_FORMAT_AV1_HIGH10_444,
        gamestream_sys::VIDEO_FORMAT_AV1_MAIN10,
        gamestream_sys::VIDEO_FORMAT_AV1_HIGH8_444,
        gamestream_sys::VIDEO_FORMAT_AV1_MAIN8,
        gamestream_sys::VIDEO_FORMAT_HEVC_REXT10_444,
        gamestream_sys::VIDEO_FORMAT_H265_MAIN10,
        gamestream_sys::VIDEO_FORMAT_HEVC_REXT8_444,
        gamestream_sys::VIDEO_FORMAT_H265,
        gamestream_sys::VIDEO_FORMAT_H264_HIGH8_444,
        gamestream_sys::VIDEO_FORMAT_H264,
    ];
    let available_formats = requested_formats & server_codec_modes;
    PREFERRED_FORMATS
        .iter()
        .copied()
        .find(|format| available_formats & *format != 0)
        .or_else(|| {
            PREFERRED_FORMATS
                .iter()
                .copied()
                .find(|format| requested_formats & *format != 0)
        })
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
    // Detect and log decoder capabilities
    let _decoder_caps = get_decoder_capabilities();
    
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
    fn sdl_software_renderer_filters_unsupported_video_formats() {
        let requested = gamestream_sys::VIDEO_FORMAT_H265
            | gamestream_sys::VIDEO_FORMAT_H265_MAIN10
            | gamestream_sys::VIDEO_FORMAT_HEVC_REXT8_444
            | gamestream_sys::VIDEO_FORMAT_AV1_HIGH10_444;

        assert_eq!(
            gamestream_sys::VIDEO_FORMAT_H265,
            super::sdl_software_renderer_video_formats(requested, true)
        );
    }

    #[test]
    fn sdl_software_renderer_falls_back_to_h264_when_only_hdr_is_requested() {
        assert_eq!(
            gamestream_sys::VIDEO_FORMAT_H264,
            super::sdl_software_renderer_video_formats(
                gamestream_sys::VIDEO_FORMAT_H265_MAIN10,
                true
            )
        );
    }

    #[test]
    fn automatic_codec_selection_prefers_av1_then_hevc_then_h264() {
        let requested = StreamConfiguration::from(&default_streaming_settings());

        assert_eq!(
            gamestream_sys::VIDEO_FORMAT_AV1_MAIN8,
            requested
                .clone()
                .preferred_for_server(
                    gamestream_sys::VIDEO_FORMAT_H264
                        | gamestream_sys::VIDEO_FORMAT_H265
                        | gamestream_sys::VIDEO_FORMAT_AV1_MAIN8
                )
                .supported_video_formats
        );
        assert_eq!(
            gamestream_sys::VIDEO_FORMAT_H265,
            requested
                .clone()
                .preferred_for_server(
                    gamestream_sys::VIDEO_FORMAT_H264 | gamestream_sys::VIDEO_FORMAT_H265
                )
                .supported_video_formats
        );
        assert_eq!(
            gamestream_sys::VIDEO_FORMAT_H264,
            requested
                .preferred_for_server(gamestream_sys::VIDEO_FORMAT_H264)
                .supported_video_formats
        );
    }

    #[test]
    fn forced_codec_settings_map_to_real_video_format_masks() {
        let mut settings = default_streaming_settings();
        settings.video_codec_config = super::VIDEO_CODEC_CONFIG_FORCE_HEVC;
        assert_eq!(
            gamestream_sys::VIDEO_FORMAT_H265,
            StreamConfiguration::from(&settings).supported_video_formats
        );

        settings.video_codec_config = super::VIDEO_CODEC_CONFIG_FORCE_AV1;
        assert_eq!(
            gamestream_sys::VIDEO_FORMAT_AV1_MAIN8 | gamestream_sys::VIDEO_FORMAT_H265,
            StreamConfiguration::from(&settings).supported_video_formats
        );
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
        assert!(callbacks.video.submit_decode_unit.is_none());
        assert!(callbacks.video.capabilities & gamestream_sys::CAPABILITY_PULL_RENDERER != 0);
        assert!(callbacks.audio.init.is_some());
        assert!(callbacks.audio.decode_and_play_sample.is_some());
    }

    #[test]
    fn output_mode_installs_media_callbacks() {
        let callbacks = super::StreamCallbacks::connection_lifecycle_for_output(
            super::StreamOutputMode::Headless,
        );

        assert!(callbacks.video.setup.is_some());
        assert!(callbacks.video.submit_decode_unit.is_none());
        assert!(callbacks.video.capabilities & gamestream_sys::CAPABILITY_PULL_RENDERER != 0);
        assert!(callbacks.audio.init.is_some());
        assert!(callbacks.audio.decode_and_play_sample.is_some());
    }

    #[test]
    fn headless_media_callbacks_are_safe_noop_sinks() {
        let video = super::headless_video_callbacks();
        let audio = super::headless_audio_callbacks();
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
        let audio_init = audio.init.unwrap();
        let audio_start = audio.start.unwrap();
        let audio_stop = audio.stop.unwrap();
        let audio_cleanup = audio.cleanup.unwrap();
        let decode_and_play_sample = audio.decode_and_play_sample.unwrap();

        let video_result = unsafe { video_setup(1, 1920, 1080, 60, std::ptr::null_mut(), 0) };
        unsafe { video_start() };
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
        assert_eq!(0, video_state.frames_received);
        assert_eq!(0, video_state.bytes_received);
        assert_eq!(0, video_state.last_frame_number);
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

    #[cfg(moonlight_common_c_linked)]
    #[test]
    fn av_frame_planes_are_copied_without_padding() {
        let mut source = [1_u8, 2, 3, 99, 4, 5, 6, 98];

        let plane = super::copy_av_frame_plane(source.as_mut_ptr(), 4, 3, 2, "test").unwrap();

        assert_eq!(3, plane.pitch);
        assert_eq!(vec![1, 2, 3, 4, 5, 6], plane.pixels);
    }

    #[test]
    fn decoded_frame_texture_formats_match_sdl_upload_paths() {
        let decoded_at = std::time::Instant::now();
        let rgba = super::DecodedVideoFrame::Rgba(super::RgbaVideoFrame {
            width: 4,
            height: 4,
            frame_number: 7,
            decoded_at,
            pixels: vec![0; 4 * 4 * 4],
        });
        let yuv = super::DecodedVideoFrame::Yuv420(super::Yuv420VideoFrame {
            width: 4,
            height: 4,
            frame_number: 8,
            decoded_at,
            y: super::VideoPlane {
                pixels: vec![0; 16],
                pitch: 4,
            },
            u: super::VideoPlane {
                pixels: vec![0; 4],
                pitch: 2,
            },
            v: super::VideoPlane {
                pixels: vec![0; 4],
                pitch: 2,
            },
        });

        assert_eq!(super::Sdl3VideoTextureFormat::Rgba, rgba.texture_format());
        assert_eq!(super::Sdl3VideoTextureFormat::Yuv420, yuv.texture_format());
        assert!(super::validate_decoded_video_frame(&rgba).is_ok());
        assert!(super::validate_decoded_video_frame(&yuv).is_ok());
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
