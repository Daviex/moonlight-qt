#![allow(dead_code)]

use std::os::raw::{c_char, c_int, c_schar, c_short, c_uchar, c_uint, c_ushort, c_void};

pub const STREAM_CFG_LOCAL: c_int = 0;
pub const STREAM_CFG_REMOTE: c_int = 1;
pub const STREAM_CFG_AUTO: c_int = 2;

pub const AUDIO_CONFIGURATION_STEREO: c_int = make_audio_configuration(2, 0x3);
pub const AUDIO_CONFIGURATION_51_SURROUND: c_int = make_audio_configuration(6, 0x3F);
pub const AUDIO_CONFIGURATION_71_SURROUND: c_int = make_audio_configuration(8, 0x63F);

pub const VIDEO_FORMAT_H264: c_int = 0x0001;
pub const VIDEO_FORMAT_H264_HIGH8_444: c_int = 0x0004;
pub const VIDEO_FORMAT_H265: c_int = 0x0100;
pub const VIDEO_FORMAT_H265_MAIN10: c_int = 0x0200;
pub const VIDEO_FORMAT_HEVC_REXT8_444: c_int = 0x0400;
pub const VIDEO_FORMAT_HEVC_REXT10_444: c_int = 0x0800;
pub const VIDEO_FORMAT_AV1_MAIN8: c_int = 0x1000;
pub const VIDEO_FORMAT_AV1_MAIN10: c_int = 0x2000;
pub const VIDEO_FORMAT_AV1_HIGH8_444: c_int = 0x4000;
pub const VIDEO_FORMAT_AV1_HIGH10_444: c_int = 0x8000;

pub const DR_OK: c_int = 0;
pub const DR_NEED_IDR: c_int = -1;

pub const STAGE_NONE: c_int = 0;
pub const STAGE_PLATFORM_INIT: c_int = 1;
pub const STAGE_NAME_RESOLUTION: c_int = 2;
pub const STAGE_AUDIO_STREAM_INIT: c_int = 3;
pub const STAGE_RTSP_HANDSHAKE: c_int = 4;
pub const STAGE_CONTROL_STREAM_INIT: c_int = 5;
pub const STAGE_VIDEO_STREAM_INIT: c_int = 6;
pub const STAGE_INPUT_STREAM_INIT: c_int = 7;
pub const STAGE_CONTROL_STREAM_START: c_int = 8;
pub const STAGE_VIDEO_STREAM_START: c_int = 9;
pub const STAGE_AUDIO_STREAM_START: c_int = 10;
pub const STAGE_INPUT_STREAM_START: c_int = 11;
pub const STAGE_MAX: c_int = 12;

pub const ML_ERROR_GRACEFUL_TERMINATION: c_int = 0;
pub const ML_ERROR_NO_VIDEO_TRAFFIC: c_int = -100;
pub const ML_ERROR_NO_VIDEO_FRAME: c_int = -101;
pub const ML_ERROR_UNEXPECTED_EARLY_TERMINATION: c_int = -102;
pub const ML_ERROR_PROTECTED_CONTENT: c_int = -103;
pub const ML_ERROR_FRAME_CONVERSION: c_int = -104;

pub const CONN_STATUS_OKAY: c_int = 0;
pub const CONN_STATUS_POOR: c_int = 1;

pub const BUTTON_ACTION_PRESS: c_char = 0x07;
pub const BUTTON_ACTION_RELEASE: c_char = 0x08;
pub const BUTTON_LEFT: c_int = 0x01;
pub const BUTTON_MIDDLE: c_int = 0x02;
pub const BUTTON_RIGHT: c_int = 0x03;
pub const BUTTON_X1: c_int = 0x04;
pub const BUTTON_X2: c_int = 0x05;

pub const KEY_ACTION_DOWN: c_char = 0x03;
pub const KEY_ACTION_UP: c_char = 0x04;
pub const MODIFIER_SHIFT: c_char = 0x01;
pub const MODIFIER_CTRL: c_char = 0x02;
pub const MODIFIER_ALT: c_char = 0x04;
pub const MODIFIER_META: c_char = 0x08;
pub const SS_KBE_FLAG_NON_NORMALIZED: c_char = 0x01;

pub const A_FLAG: c_int = 0x1000;
pub const B_FLAG: c_int = 0x2000;
pub const X_FLAG: c_int = 0x4000;
pub const Y_FLAG: c_int = 0x8000;
pub const UP_FLAG: c_int = 0x0001;
pub const DOWN_FLAG: c_int = 0x0002;
pub const LEFT_FLAG: c_int = 0x0004;
pub const RIGHT_FLAG: c_int = 0x0008;
pub const LB_FLAG: c_int = 0x0100;
pub const RB_FLAG: c_int = 0x0200;
pub const PLAY_FLAG: c_int = 0x0010;
pub const BACK_FLAG: c_int = 0x0020;
pub const LS_CLK_FLAG: c_int = 0x0040;
pub const RS_CLK_FLAG: c_int = 0x0080;
pub const SPECIAL_FLAG: c_int = 0x0400;
pub const PADDLE1_FLAG: c_int = 0x010000;
pub const PADDLE2_FLAG: c_int = 0x020000;
pub const PADDLE3_FLAG: c_int = 0x040000;
pub const PADDLE4_FLAG: c_int = 0x080000;
pub const TOUCHPAD_FLAG: c_int = 0x100000;
pub const MISC_FLAG: c_int = 0x200000;

pub const LI_CTYPE_UNKNOWN: c_uchar = 0x00;
pub const LI_CTYPE_XBOX: c_uchar = 0x01;
pub const LI_CTYPE_PS: c_uchar = 0x02;
pub const LI_CTYPE_NINTENDO: c_uchar = 0x03;
pub const LI_CCAP_ANALOG_TRIGGERS: c_ushort = 0x01;
pub const LI_CCAP_RUMBLE: c_ushort = 0x02;
pub const LI_CCAP_TRIGGER_RUMBLE: c_ushort = 0x04;
pub const LI_CCAP_TOUCHPAD: c_ushort = 0x08;
pub const LI_CCAP_ACCEL: c_ushort = 0x10;
pub const LI_CCAP_GYRO: c_ushort = 0x20;
pub const LI_CCAP_BATTERY_STATE: c_ushort = 0x40;
pub const LI_CCAP_RGB_LED: c_ushort = 0x80;

pub const fn make_audio_configuration(channel_count: c_int, channel_mask: c_int) -> c_int {
    (channel_mask << 16) | (channel_count << 8) | 0xCA
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamConfiguration {
    pub width: c_int,
    pub height: c_int,
    pub fps: c_int,
    pub bitrate: c_int,
    pub packet_size: c_int,
    pub streaming_remotely: c_int,
    pub audio_configuration: c_int,
    pub supported_video_formats: c_int,
    pub client_refresh_rate_x100: c_int,
    pub color_space: c_int,
    pub color_range: c_int,
    pub encryption_flags: c_int,
    pub remote_input_aes_key: [c_char; 16],
    pub remote_input_aes_iv: [c_char; 16],
}

impl Default for StreamConfiguration {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            fps: 0,
            bitrate: 0,
            packet_size: 0,
            streaming_remotely: STREAM_CFG_AUTO,
            audio_configuration: AUDIO_CONFIGURATION_STEREO,
            supported_video_formats: VIDEO_FORMAT_H264,
            client_refresh_rate_x100: 0,
            color_space: 0,
            color_range: 0,
            encryption_flags: 0,
            remote_input_aes_key: [0; 16],
            remote_input_aes_iv: [0; 16],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServerInformation {
    pub address: *const c_char,
    pub server_info_app_version: *const c_char,
    pub server_info_gfe_version: *const c_char,
    pub rtsp_session_url: *const c_char,
    pub server_codec_mode_support: c_int,
}

impl Default for ServerInformation {
    fn default() -> Self {
        Self {
            address: std::ptr::null(),
            server_info_app_version: std::ptr::null(),
            server_info_gfe_version: std::ptr::null(),
            rtsp_session_url: std::ptr::null(),
            server_codec_mode_support: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LEntry {
    pub next: *mut LEntry,
    pub data: *mut c_char,
    pub length: c_int,
    pub buffer_type: c_int,
}

impl Default for LEntry {
    fn default() -> Self {
        Self {
            next: std::ptr::null_mut(),
            data: std::ptr::null_mut(),
            length: 0,
            buffer_type: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecodeUnit {
    pub frame_number: c_int,
    pub frame_type: c_int,
    pub frame_host_processing_latency: u16,
    pub receive_time_us: u64,
    pub enqueue_time_us: u64,
    pub presentation_time_us: u64,
    pub rtp_timestamp: u32,
    pub full_length: c_int,
    pub buffer_list: *mut LEntry,
    pub hdr_active: bool,
    pub colorspace: c_uchar,
}

impl Default for DecodeUnit {
    fn default() -> Self {
        Self {
            frame_number: 0,
            frame_type: 0,
            frame_host_processing_latency: 0,
            receive_time_us: 0,
            enqueue_time_us: 0,
            presentation_time_us: 0,
            rtp_timestamp: 0,
            full_length: 0,
            buffer_list: std::ptr::null_mut(),
            hdr_active: false,
            colorspace: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpusMultistreamConfiguration {
    pub sample_rate: c_int,
    pub channel_count: c_int,
    pub streams: c_int,
    pub coupled_streams: c_int,
    pub samples_per_frame: c_int,
    pub mapping: [c_uchar; 8],
}

impl Default for OpusMultistreamConfiguration {
    fn default() -> Self {
        Self {
            sample_rate: 0,
            channel_count: 0,
            streams: 0,
            coupled_streams: 0,
            samples_per_frame: 0,
            mapping: [0; 8],
        }
    }
}

pub type DecoderRendererSetup = Option<
    unsafe extern "C" fn(
        video_format: c_int,
        width: c_int,
        height: c_int,
        redraw_rate: c_int,
        context: *mut c_void,
        dr_flags: c_int,
    ) -> c_int,
>;
pub type DecoderRendererStart = Option<unsafe extern "C" fn()>;
pub type DecoderRendererStop = Option<unsafe extern "C" fn()>;
pub type DecoderRendererCleanup = Option<unsafe extern "C" fn()>;
pub type DecoderRendererSubmitDecodeUnit =
    Option<unsafe extern "C" fn(decode_unit: *mut DecodeUnit) -> c_int>;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DecoderRendererCallbacks {
    pub setup: DecoderRendererSetup,
    pub start: DecoderRendererStart,
    pub stop: DecoderRendererStop,
    pub cleanup: DecoderRendererCleanup,
    pub submit_decode_unit: DecoderRendererSubmitDecodeUnit,
    pub capabilities: c_int,
}

impl Default for DecoderRendererCallbacks {
    fn default() -> Self {
        Self {
            setup: None,
            start: None,
            stop: None,
            cleanup: None,
            submit_decode_unit: None,
            capabilities: 0,
        }
    }
}

pub type AudioRendererInit = Option<
    unsafe extern "C" fn(
        audio_configuration: c_int,
        opus_config: *const OpusMultistreamConfiguration,
        context: *mut c_void,
        ar_flags: c_int,
    ) -> c_int,
>;
pub type AudioRendererStart = Option<unsafe extern "C" fn()>;
pub type AudioRendererStop = Option<unsafe extern "C" fn()>;
pub type AudioRendererCleanup = Option<unsafe extern "C" fn()>;
pub type AudioRendererDecodeAndPlaySample =
    Option<unsafe extern "C" fn(sample_data: *mut c_char, sample_length: c_int)>;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AudioRendererCallbacks {
    pub init: AudioRendererInit,
    pub start: AudioRendererStart,
    pub stop: AudioRendererStop,
    pub cleanup: AudioRendererCleanup,
    pub decode_and_play_sample: AudioRendererDecodeAndPlaySample,
    pub capabilities: c_int,
}

impl Default for AudioRendererCallbacks {
    fn default() -> Self {
        Self {
            init: None,
            start: None,
            stop: None,
            cleanup: None,
            decode_and_play_sample: None,
            capabilities: 0,
        }
    }
}

pub type ConnListenerStageStarting = Option<unsafe extern "C" fn(stage: c_int)>;
pub type ConnListenerStageComplete = Option<unsafe extern "C" fn(stage: c_int)>;
pub type ConnListenerStageFailed = Option<unsafe extern "C" fn(stage: c_int, error_code: c_int)>;
pub type ConnListenerConnectionStarted = Option<unsafe extern "C" fn()>;
pub type ConnListenerConnectionTerminated = Option<unsafe extern "C" fn(error_code: c_int)>;
pub type ConnListenerLogMessage = Option<unsafe extern "C" fn(format: *const c_char, ...)>;
pub type ConnListenerRumble = Option<
    unsafe extern "C" fn(
        controller_number: c_ushort,
        low_freq_motor: c_ushort,
        high_freq_motor: c_ushort,
    ),
>;
pub type ConnListenerConnectionStatusUpdate =
    Option<unsafe extern "C" fn(connection_status: c_int)>;
pub type ConnListenerSetHdrMode = Option<unsafe extern "C" fn(hdr_enabled: bool)>;
pub type ConnListenerRumbleTriggers = Option<
    unsafe extern "C" fn(controller_number: u16, left_trigger_motor: u16, right_trigger_motor: u16),
>;
pub type ConnListenerSetMotionEventState =
    Option<unsafe extern "C" fn(controller_number: u16, motion_type: c_uchar, report_rate_hz: u16)>;
pub type ConnListenerSetControllerLed =
    Option<unsafe extern "C" fn(controller_number: u16, r: c_uchar, g: c_uchar, b: c_uchar)>;
pub type ConnListenerSetAdaptiveTriggers = Option<
    unsafe extern "C" fn(
        controller_number: u16,
        event_flags: c_uchar,
        type_left: c_uchar,
        type_right: c_uchar,
        left: *mut c_uchar,
        right: *mut c_uchar,
    ),
>;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ConnectionListenerCallbacks {
    pub stage_starting: ConnListenerStageStarting,
    pub stage_complete: ConnListenerStageComplete,
    pub stage_failed: ConnListenerStageFailed,
    pub connection_started: ConnListenerConnectionStarted,
    pub connection_terminated: ConnListenerConnectionTerminated,
    pub log_message: ConnListenerLogMessage,
    pub rumble: ConnListenerRumble,
    pub connection_status_update: ConnListenerConnectionStatusUpdate,
    pub set_hdr_mode: ConnListenerSetHdrMode,
    pub rumble_triggers: ConnListenerRumbleTriggers,
    pub set_motion_event_state: ConnListenerSetMotionEventState,
    pub set_controller_led: ConnListenerSetControllerLed,
    pub set_adaptive_triggers: ConnListenerSetAdaptiveTriggers,
}

impl Default for ConnectionListenerCallbacks {
    fn default() -> Self {
        Self {
            stage_starting: None,
            stage_complete: None,
            stage_failed: None,
            connection_started: None,
            connection_terminated: None,
            log_message: None,
            rumble: None,
            connection_status_update: None,
            set_hdr_mode: None,
            rumble_triggers: None,
            set_motion_event_state: None,
            set_controller_led: None,
            set_adaptive_triggers: None,
        }
    }
}

extern "C" {
    pub fn LiInitializeStreamConfiguration(stream_config: *mut StreamConfiguration);
    pub fn LiInitializeVideoCallbacks(callbacks: *mut DecoderRendererCallbacks);
    pub fn LiInitializeAudioCallbacks(callbacks: *mut AudioRendererCallbacks);
    pub fn LiInitializeConnectionCallbacks(callbacks: *mut ConnectionListenerCallbacks);
    pub fn LiInitializeServerInformation(server_info: *mut ServerInformation);
    pub fn LiStartConnection(
        server_info: *mut ServerInformation,
        stream_config: *mut StreamConfiguration,
        connection_callbacks: *mut ConnectionListenerCallbacks,
        video_callbacks: *mut DecoderRendererCallbacks,
        audio_callbacks: *mut AudioRendererCallbacks,
        render_context: *mut c_void,
        video_flags: c_int,
        audio_context: *mut c_void,
        audio_flags: c_int,
    ) -> c_int;
    pub fn LiStopConnection();
    pub fn LiInterruptConnection();
    pub fn LiGetStageName(stage: c_int) -> *const c_char;
    pub fn LiSendMouseMoveEvent(delta_x: c_short, delta_y: c_short) -> c_int;
    pub fn LiSendMousePositionEvent(
        x: c_short,
        y: c_short,
        reference_width: c_short,
        reference_height: c_short,
    ) -> c_int;
    pub fn LiSendMouseButtonEvent(action: c_char, button: c_int) -> c_int;
    pub fn LiSendKeyboardEvent2(
        key_code: c_short,
        key_action: c_char,
        modifiers: c_char,
        flags: c_char,
    ) -> c_int;
    pub fn LiSendUtf8TextEvent(text: *const c_char, length: c_uint) -> c_int;
    pub fn LiSendControllerEvent(
        button_flags: c_int,
        left_trigger: c_uchar,
        right_trigger: c_uchar,
        left_stick_x: c_short,
        left_stick_y: c_short,
        right_stick_x: c_short,
        right_stick_y: c_short,
    ) -> c_int;
    pub fn LiSendMultiControllerEvent(
        controller_number: c_short,
        active_gamepad_mask: c_short,
        button_flags: c_int,
        left_trigger: c_uchar,
        right_trigger: c_uchar,
        left_stick_x: c_short,
        left_stick_y: c_short,
        right_stick_x: c_short,
        right_stick_y: c_short,
    ) -> c_int;
    pub fn LiSendControllerArrivalEvent(
        controller_number: c_uchar,
        active_gamepad_mask: c_ushort,
        controller_type: c_uchar,
        supported_button_flags: c_uint,
        capabilities: c_ushort,
    ) -> c_int;
    pub fn LiSendScrollEvent(scroll_clicks: c_schar) -> c_int;
    pub fn LiSendHighResScrollEvent(scroll_amount: c_short) -> c_int;
    pub fn LiSendHScrollEvent(scroll_clicks: c_schar) -> c_int;
    pub fn LiSendHighResHScrollEvent(scroll_amount: c_short) -> c_int;
}

#[cfg(test)]
mod tests {
    use super::{
        make_audio_configuration, AudioRendererCallbacks, ConnectionListenerCallbacks, DecodeUnit,
        DecoderRendererCallbacks, OpusMultistreamConfiguration, AUDIO_CONFIGURATION_51_SURROUND,
        AUDIO_CONFIGURATION_71_SURROUND, AUDIO_CONFIGURATION_STEREO, CONN_STATUS_OKAY, DR_NEED_IDR,
        DR_OK, ML_ERROR_NO_VIDEO_TRAFFIC, STAGE_MAX, STAGE_NONE, STREAM_CFG_AUTO,
    };

    #[test]
    fn audio_configuration_matches_c_macro_values() {
        assert_eq!(0x302CA, AUDIO_CONFIGURATION_STEREO);
        assert_eq!(0x3F06CA, AUDIO_CONFIGURATION_51_SURROUND);
        assert_eq!(0x63F08CA, AUDIO_CONFIGURATION_71_SURROUND);
        assert_eq!(AUDIO_CONFIGURATION_STEREO, make_audio_configuration(2, 0x3));
    }

    #[test]
    fn stream_configuration_default_uses_safe_auto_values() {
        let config = super::StreamConfiguration::default();

        assert_eq!(STREAM_CFG_AUTO, config.streaming_remotely);
        assert_eq!(AUDIO_CONFIGURATION_STEREO, config.audio_configuration);
    }

    #[test]
    fn callback_defaults_are_null_and_safe_for_optional_callbacks() {
        let video = DecoderRendererCallbacks::default();
        let audio = AudioRendererCallbacks::default();
        let connection = ConnectionListenerCallbacks::default();

        assert!(video.setup.is_none());
        assert!(audio.init.is_none());
        assert!(connection.stage_starting.is_none());
        assert!(connection.log_message.is_none());
    }

    #[test]
    fn decode_and_audio_struct_defaults_have_null_buffers() {
        let decode_unit = DecodeUnit::default();
        let opus = OpusMultistreamConfiguration::default();

        assert!(decode_unit.buffer_list.is_null());
        assert_eq!(0, decode_unit.full_length);
        assert_eq!([0; 8], opus.mapping);
    }

    #[test]
    fn gamestream_status_constants_match_limelight_header_values() {
        assert_eq!(0, DR_OK);
        assert_eq!(-1, DR_NEED_IDR);
        assert_eq!(0, STAGE_NONE);
        assert_eq!(12, STAGE_MAX);
        assert_eq!(0, CONN_STATUS_OKAY);
        assert_eq!(-100, ML_ERROR_NO_VIDEO_TRAFFIC);
    }
}
