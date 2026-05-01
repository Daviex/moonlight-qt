#![allow(dead_code)]

use std::os::raw::{c_char, c_int};

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

#[cfg(test)]
mod tests {
    use super::{
        make_audio_configuration, AUDIO_CONFIGURATION_51_SURROUND, AUDIO_CONFIGURATION_71_SURROUND,
        AUDIO_CONFIGURATION_STEREO, STREAM_CFG_AUTO,
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
}
