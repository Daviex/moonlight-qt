#![allow(dead_code)]

use super::error::CoreError;
use super::types::StreamingSettings;

const MIN_WIDTH: u32 = 256;
const MAX_WIDTH: u32 = 8192;
const MIN_HEIGHT: u32 = 256;
const MAX_HEIGHT: u32 = 8192;
const MIN_FPS: u32 = 10;
const MAX_FPS: u32 = 9999;
const MIN_BITRATE_KBPS: u32 = 500;
const MAX_BITRATE_KBPS: u32 = 500_000;
const MIN_PACKET_SIZE: u32 = 0;
const MAX_PACKET_SIZE: u32 = 9000;
const MIN_LANGUAGE: i32 = 0;
const MAX_LANGUAGE: i32 = 31;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsBounds {
    pub min_width: u32,
    pub max_width: u32,
    pub min_height: u32,
    pub max_height: u32,
    pub min_fps: u32,
    pub max_fps: u32,
    pub min_bitrate_kbps: u32,
    pub max_bitrate_kbps: u32,
    pub min_packet_size: u32,
    pub max_packet_size: u32,
}

impl Default for SettingsBounds {
    fn default() -> Self {
        Self {
            min_width: MIN_WIDTH,
            max_width: MAX_WIDTH,
            min_height: MIN_HEIGHT,
            max_height: MAX_HEIGHT,
            min_fps: MIN_FPS,
            max_fps: MAX_FPS,
            min_bitrate_kbps: MIN_BITRATE_KBPS,
            max_bitrate_kbps: MAX_BITRATE_KBPS,
            min_packet_size: MIN_PACKET_SIZE,
            max_packet_size: MAX_PACKET_SIZE,
        }
    }
}

pub fn validate_streaming_settings(settings: &StreamingSettings) -> Result<(), CoreError> {
    validate_u32("Width", settings.width, MIN_WIDTH, MAX_WIDTH)?;
    validate_u32("Height", settings.height, MIN_HEIGHT, MAX_HEIGHT)?;
    validate_u32("FPS", settings.fps, MIN_FPS, MAX_FPS)?;
    validate_u32(
        "Bitrate",
        settings.bitrate_kbps,
        MIN_BITRATE_KBPS,
        MAX_BITRATE_KBPS,
    )?;
    validate_u32(
        "Packet size",
        settings.packet_size,
        MIN_PACKET_SIZE,
        MAX_PACKET_SIZE,
    )?;
    validate_i32("Audio configuration", settings.audio_config, 0, 2)?;
    validate_i32("Video codec", settings.video_codec_config, 0, 4)?;
    validate_i32("Video decoder", settings.video_decoder_selection, 0, 2)?;
    validate_i32("Stream window mode", settings.window_mode, 0, 2)?;
    validate_i32("UI startup mode", settings.ui_display_mode, 0, 2)?;
    validate_i32("Language", settings.language, MIN_LANGUAGE, MAX_LANGUAGE)?;
    validate_i32(
        "Capture system keys mode",
        settings.capture_sys_keys_mode,
        0,
        2,
    )?;
    Ok(())
}

pub fn default_streaming_settings() -> StreamingSettings {
    StreamingSettings {
        width: 1920,
        height: 1080,
        fps: 60,
        bitrate_kbps: 20_000,
        packet_size: 0,
        audio_config: 0,
        video_codec_config: 0,
        video_decoder_selection: 0,
        window_mode: 1,
        ui_display_mode: 0,
        language: 0,
        capture_sys_keys_mode: 1,
        unlock_bitrate: false,
        auto_adjust_bitrate: false,
        enable_vsync: true,
        game_optimizations: false,
        play_audio_on_host: false,
        multi_controller: true,
        enable_mdns: true,
        quit_app_after: false,
        absolute_mouse_mode: false,
        absolute_touch_mode: true,
        frame_pacing: false,
        connection_warnings: true,
        configuration_warnings: true,
        rich_presence: true,
        enable_hdr: false,
        gamepad_mouse: true,
        detect_network_blocking: true,
        show_performance_overlay: false,
        swap_mouse_buttons: false,
        mute_on_focus_loss: false,
        background_gamepad: false,
        reverse_scroll_direction: false,
        swap_face_buttons: false,
        keep_awake: true,
        enable_yuv444: false,
    }
}

fn validate_u32(
    label: &'static str,
    value: u32,
    minimum: u32,
    maximum: u32,
) -> Result<(), CoreError> {
    if value < minimum || value > maximum {
        return Err(CoreError::Validation(format!(
            "{label} must be between {minimum} and {maximum}."
        )));
    }
    Ok(())
}

fn validate_i32(
    label: &'static str,
    value: i32,
    minimum: i32,
    maximum: i32,
) -> Result<(), CoreError> {
    if value < minimum || value > maximum {
        return Err(CoreError::Validation(format!(
            "{label} must be between {minimum} and {maximum}."
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{default_streaming_settings, validate_streaming_settings, SettingsBounds};

    #[test]
    fn default_settings_are_valid() {
        let settings = default_streaming_settings();

        assert!(validate_streaming_settings(&settings).is_ok());
    }

    #[test]
    fn settings_bounds_match_native_validation_ranges() {
        let bounds = SettingsBounds::default();

        assert_eq!(256, bounds.min_width);
        assert_eq!(8192, bounds.max_width);
        assert_eq!(500, bounds.min_bitrate_kbps);
        assert_eq!(500_000, bounds.max_bitrate_kbps);
    }

    #[test]
    fn invalid_width_returns_native_style_error() {
        let mut settings = default_streaming_settings();
        settings.width = 128;

        let error = validate_streaming_settings(&settings).unwrap_err();

        assert_eq!("Width must be between 256 and 8192.", error.to_string());
    }

    #[test]
    fn invalid_language_returns_native_style_error() {
        let mut settings = default_streaming_settings();
        settings.language = 32;

        let error = validate_streaming_settings(&settings).unwrap_err();

        assert_eq!("Language must be between 0 and 31.", error.to_string());
    }
}
