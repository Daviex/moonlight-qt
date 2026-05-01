#![allow(dead_code)]

use super::gamestream_sys;
use std::os::raw::c_int;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioConfiguration {
    Stereo,
    Surround51,
    Surround71,
}

impl AudioConfiguration {
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
}

impl StreamConfiguration {
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
            ..gamestream_sys::StreamConfiguration::default()
        }
    }
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
        }
    }
}

fn saturated_c_int(value: u32) -> c_int {
    value.min(c_int::MAX as u32) as c_int
}

#[cfg(test)]
mod tests {
    use super::{AudioConfiguration, StreamConfiguration, StreamingRemotely};
    use crate::core::gamestream_sys;
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
    fn all_audio_and_remote_variants_have_raw_values() {
        assert_eq!(
            gamestream_sys::AUDIO_CONFIGURATION_51_SURROUND,
            AudioConfiguration::Surround51.as_raw()
        );
        assert_eq!(
            gamestream_sys::STREAM_CFG_LOCAL,
            StreamingRemotely::Local.as_raw()
        );
    }
}
