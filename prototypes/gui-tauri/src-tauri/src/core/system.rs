#![allow(dead_code)]

use super::types::{DisplayInfo, SystemInfo};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplaySummary {
    pub native_width: i32,
    pub native_height: i32,
    pub refresh_rate: i32,
}

impl From<DisplayInfo> for DisplaySummary {
    fn from(display: DisplayInfo) -> Self {
        Self {
            native_width: display.native_width,
            native_height: display.native_height,
            refresh_rate: display.refresh_rate,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemCapabilities {
    pub version: String,
    pub architecture: String,
    pub supports_hdr: bool,
    pub has_hardware_acceleration: bool,
    pub displays: Vec<DisplaySummary>,
}

impl From<SystemInfo> for SystemCapabilities {
    fn from(info: SystemInfo) -> Self {
        Self {
            version: info.version,
            architecture: info.friendly_native_arch_name,
            supports_hdr: info.supports_hdr,
            has_hardware_acceleration: info.has_hardware_acceleration,
            displays: info
                .displays
                .into_iter()
                .map(DisplaySummary::from)
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SystemCapabilities;
    use crate::core::types::{DisplayInfo, SystemInfo};

    #[test]
    fn system_info_converts_to_capabilities_summary() {
        let capabilities = SystemCapabilities::from(SystemInfo {
            version: "1.0.0".into(),
            friendly_native_arch_name: "x64".into(),
            is_running_wayland: false,
            is_running_xwayland: false,
            is_wow64: false,
            has_desktop_environment: true,
            has_browser: true,
            has_discord_integration: false,
            uses_material3_theme: false,
            has_hardware_acceleration: true,
            renderer_always_full_screen: false,
            maximum_resolution_width: 3840,
            maximum_resolution_height: 2160,
            supports_hdr: true,
            unmapped_gamepads: String::new(),
            displays: vec![DisplayInfo {
                native_width: 1920,
                native_height: 1080,
                safe_area_width: 1920,
                safe_area_height: 1080,
                refresh_rate: 60,
            }],
        });

        assert_eq!("x64", capabilities.architecture);
        assert!(capabilities.supports_hdr);
        assert_eq!(1, capabilities.displays.len());
    }
}
