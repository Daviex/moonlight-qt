#![allow(dead_code)]

use super::error::CoreError;
use super::stream_libplacebo;
use super::types::StreamingSettings;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VideoDecoderPreference {
    Automatic,
    ForceHardware,
    ForceSoftware,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StreamRendererBackend {
    D3d11,
    LibplaceboVulkan,
    SoftwareSdl,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamRendererPlan {
    pub decoder_preference: VideoDecoderPreference,
    pub backend: StreamRendererBackend,
    pub hdr_requested: bool,
    pub yuv444_requested: bool,
    pub vsync: bool,
}

impl StreamRendererPlan {
    pub fn new(settings: &StreamingSettings) -> Result<Self, CoreError> {
        let decoder_preference =
            VideoDecoderPreference::from_raw(settings.video_decoder_selection)?;
        let backend = select_backend(decoder_preference);
        crate::logger::stream(format!(
            "renderer plan selected; backend={backend:?}; decoder_preference={decoder_preference:?}; hdr_requested={}; yuv444_requested={}; libplacebo_status={}",
            settings.enable_hdr,
            settings.enable_yuv444,
            stream_libplacebo::renderer_status_message()
        ));

        Ok(Self {
            decoder_preference,
            backend,
            hdr_requested: settings.enable_hdr,
            yuv444_requested: settings.enable_yuv444,
            vsync: settings.enable_vsync,
        })
    }

    pub fn supports_hdr_formats(&self) -> bool {
        self.backend.supports_hdr_formats()
    }
}

impl VideoDecoderPreference {
    fn from_raw(value: i32) -> Result<Self, CoreError> {
        match value {
            0 => Ok(Self::Automatic),
            1 => Ok(Self::ForceHardware),
            2 => Ok(Self::ForceSoftware),
            _ => Err(CoreError::Validation(format!(
                "Unsupported video decoder selection: {value}."
            ))),
        }
    }
}

fn select_backend(preference: VideoDecoderPreference) -> StreamRendererBackend {
    match preference {
        VideoDecoderPreference::ForceSoftware => StreamRendererBackend::SoftwareSdl,
        VideoDecoderPreference::Automatic | VideoDecoderPreference::ForceHardware => {
            platform_hardware_backend()
        }
    }
}

impl StreamRendererBackend {
    pub fn supports_hdr_formats(self) -> bool {
        matches!(self, Self::LibplaceboVulkan | Self::D3d11)
    }
}

fn platform_hardware_backend() -> StreamRendererBackend {
    #[cfg(target_os = "windows")]
    {
        StreamRendererBackend::D3d11
    }

    #[cfg(all(target_os = "linux", libplacebo_renderer_linked))]
    {
        StreamRendererBackend::LibplaceboVulkan
    }

    #[cfg(not(any(target_os = "windows", all(target_os = "linux", libplacebo_renderer_linked))))]
    {
        StreamRendererBackend::SoftwareSdl
    }
}

#[cfg(test)]
mod tests {
    use super::{StreamRendererBackend, StreamRendererPlan, VideoDecoderPreference};
    use crate::core::settings::default_streaming_settings;

    #[test]
    fn renderer_plan_reports_available_backend_by_default() {
        let settings = default_streaming_settings();
        let plan = StreamRendererPlan::new(&settings).unwrap();

        assert_eq!(VideoDecoderPreference::Automatic, plan.decoder_preference);
        #[cfg(target_os = "windows")]
        assert_eq!(StreamRendererBackend::D3d11, plan.backend);
        #[cfg(all(target_os = "linux", libplacebo_renderer_linked))]
        assert_eq!(StreamRendererBackend::LibplaceboVulkan, plan.backend);
        #[cfg(not(any(target_os = "windows", all(target_os = "linux", libplacebo_renderer_linked))))]
        assert_eq!(StreamRendererBackend::SoftwareSdl, plan.backend);
        assert!(plan.vsync);
    }

    #[test]
    fn renderer_plan_respects_forced_software_decoder() {
        let mut settings = default_streaming_settings();
        settings.video_decoder_selection = 2;
        settings.enable_hdr = true;
        settings.enable_yuv444 = true;

        let plan = StreamRendererPlan::new(&settings).unwrap();

        assert_eq!(
            VideoDecoderPreference::ForceSoftware,
            plan.decoder_preference
        );
        assert_eq!(StreamRendererBackend::SoftwareSdl, plan.backend);
        assert!(plan.hdr_requested);
        assert!(plan.yuv444_requested);
    }

    #[test]
    fn renderer_plan_rejects_unknown_decoder_selection() {
        let mut settings = default_streaming_settings();
        settings.video_decoder_selection = 99;

        let error = StreamRendererPlan::new(&settings).unwrap_err();

        assert_eq!(
            "Unsupported video decoder selection: 99.",
            error.to_string()
        );
    }
}
