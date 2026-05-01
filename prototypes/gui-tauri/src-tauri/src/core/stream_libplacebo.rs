#![allow(dead_code)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LibplaceboRendererStatus {
    Linked,
    DisabledAtBuildTime,
}

pub fn renderer_status() -> LibplaceboRendererStatus {
    if cfg!(all(target_os = "linux", libplacebo_renderer_linked)) {
        LibplaceboRendererStatus::Linked
    } else {
        LibplaceboRendererStatus::DisabledAtBuildTime
    }
}

pub fn renderer_status_message() -> &'static str {
    match renderer_status() {
        LibplaceboRendererStatus::Linked => {
            "Linux libplacebo/Vulkan renderer is linked and can be selected for HDR-capable streams."
        }
        LibplaceboRendererStatus::DisabledAtBuildTime => {
            "Linux libplacebo/Vulkan renderer is not linked; falling back to SDL software presentation."
        }
    }
}

#[cfg(all(target_os = "linux", libplacebo_renderer_linked))]
mod ffi {
    use std::os::raw::{c_char, c_int, c_void};

    pub const PL_API_VER: c_int = 360;

    #[repr(C)]
    pub struct PlLogOpaque {
        _private: [u8; 0],
    }

    #[repr(C)]
    pub struct PlLogParams {
        pub log_cb: Option<
            unsafe extern "C" fn(
                log: *mut PlLogOpaque,
                level: c_int,
                message: *const c_char,
                user_data: *mut c_void,
            ),
        >,
        pub log_level: c_int,
        pub user_data: *mut c_void,
    }

    unsafe extern "C" {
        pub static pl_log_default_params: PlLogParams;
        pub fn pl_log_create(api_ver: c_int, params: *const PlLogParams) -> *mut PlLogOpaque;
        pub fn pl_log_destroy(log: *mut *mut PlLogOpaque);
    }
}

#[cfg(test)]
mod tests {
    use super::{renderer_status, renderer_status_message, LibplaceboRendererStatus};

    #[test]
    fn renderer_status_matches_build_configuration() {
        #[cfg(all(target_os = "linux", libplacebo_renderer_linked))]
        assert_eq!(LibplaceboRendererStatus::Linked, renderer_status());

        #[cfg(not(all(target_os = "linux", libplacebo_renderer_linked)))]
        assert_eq!(
            LibplaceboRendererStatus::DisabledAtBuildTime,
            renderer_status()
        );
    }

    #[test]
    fn renderer_status_has_user_visible_message() {
        assert!(!renderer_status_message().is_empty());
    }
}
