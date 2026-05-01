#![allow(dead_code)]

use super::error::CoreError;
use super::types::StreamingSettings;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamWindowMode {
    Fullscreen,
    BorderlessFullscreen,
    Windowed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputCapturePolicy {
    FirstPointerEnter,
    AfterRendererCreated,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamWindowDescriptor {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub mode: StreamWindowMode,
    pub resizable: bool,
    pub high_dpi: bool,
    pub input_capture_policy: InputCapturePolicy,
}

impl StreamWindowDescriptor {
    pub fn new(host_name: &str, settings: &StreamingSettings) -> Result<Self, CoreError> {
        if settings.width == 0 || settings.height == 0 {
            return Err(CoreError::Validation(
                "Stream window dimensions must be greater than zero.".into(),
            ));
        }

        Ok(Self {
            title: stream_window_title(host_name),
            width: settings.width,
            height: settings.height,
            mode: StreamWindowMode::from_raw(settings.window_mode)?,
            resizable: true,
            high_dpi: true,
            input_capture_policy: default_input_capture_policy(),
        })
    }

    pub fn starts_fullscreen(&self) -> bool {
        self.mode != StreamWindowMode::Windowed
    }

    pub fn uses_exclusive_fullscreen(&self) -> bool {
        self.mode == StreamWindowMode::Fullscreen
    }
}

impl StreamWindowMode {
    pub fn from_raw(value: i32) -> Result<Self, CoreError> {
        match value {
            0 => Ok(Self::Fullscreen),
            1 => Ok(Self::BorderlessFullscreen),
            2 => Ok(Self::Windowed),
            _ => Err(CoreError::Validation(format!(
                "Unsupported stream window mode: {value}."
            ))),
        }
    }
}

fn stream_window_title(host_name: &str) -> String {
    #[cfg(target_os = "macos")]
    {
        host_name.to_string()
    }

    #[cfg(not(target_os = "macos"))]
    {
        format!("{host_name} - Moonlight")
    }
}

fn default_input_capture_policy() -> InputCapturePolicy {
    #[cfg(target_os = "linux")]
    {
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            return InputCapturePolicy::FirstPointerEnter;
        }
    }

    InputCapturePolicy::AfterRendererCreated
}

#[cfg(test)]
mod tests {
    use super::{InputCapturePolicy, StreamWindowDescriptor, StreamWindowMode};
    use crate::core::settings::default_streaming_settings;

    #[test]
    fn window_descriptor_matches_native_defaults() {
        let settings = default_streaming_settings();
        let descriptor = StreamWindowDescriptor::new("Gaming PC", &settings).unwrap();

        assert_eq!(1920, descriptor.width);
        assert_eq!(1080, descriptor.height);
        assert_eq!(StreamWindowMode::BorderlessFullscreen, descriptor.mode);
        assert!(descriptor.starts_fullscreen());
        assert!(!descriptor.uses_exclusive_fullscreen());
        assert!(descriptor.resizable);
        assert!(descriptor.high_dpi);
        assert_eq!(
            InputCapturePolicy::AfterRendererCreated,
            descriptor.input_capture_policy
        );

        #[cfg(not(target_os = "macos"))]
        assert_eq!("Gaming PC - Moonlight", descriptor.title);
    }

    #[test]
    fn window_mode_values_match_native_preferences() {
        assert_eq!(
            StreamWindowMode::Fullscreen,
            StreamWindowMode::from_raw(0).unwrap()
        );
        assert_eq!(
            StreamWindowMode::BorderlessFullscreen,
            StreamWindowMode::from_raw(1).unwrap()
        );
        assert_eq!(
            StreamWindowMode::Windowed,
            StreamWindowMode::from_raw(2).unwrap()
        );
    }

    #[test]
    fn window_descriptor_rejects_invalid_dimensions() {
        let mut settings = default_streaming_settings();
        settings.width = 0;

        let error = StreamWindowDescriptor::new("Gaming PC", &settings).unwrap_err();

        assert_eq!(
            "Stream window dimensions must be greater than zero.",
            error.to_string()
        );
    }
}
