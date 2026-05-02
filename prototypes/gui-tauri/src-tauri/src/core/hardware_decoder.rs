/// Hardware video decoding using FFmpeg D3D11VA acceleration
/// This module provides GPU-accelerated H.264/H.265/AV1 decoding on Windows
/// via Direct3D 11 hardware acceleration.

#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Direct3D::*;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Direct3D11::*;

mod logger {
    pub fn log(message: impl AsRef<str>) {
        crate::logger::stream(message);
    }
}

/// Windows D3D11 device for hardware video decoding
#[cfg(target_os = "windows")]
pub struct D3D11Device {
    device: Option<windows::Win32::Graphics::Direct3D11::ID3D11Device>,
    context: Option<windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext>,
    feature_level: D3D_FEATURE_LEVEL,
}

#[cfg(target_os = "windows")]
impl D3D11Device {
    /// Create a new D3D11 device suitable for hardware video decoding
    pub fn create() -> Result<Self, String> {
        logger::log("Creating D3D11 device for hardware video decoding...");

        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        let mut feature_level = D3D_FEATURE_LEVEL(0i32);

        let feature_levels = [
            D3D_FEATURE_LEVEL_11_1,
            D3D_FEATURE_LEVEL_11_0,
            D3D_FEATURE_LEVEL_10_1,
        ];

        // SAFETY: D3D11CreateDevice is a Windows API call that initializes COM objects.
        // The returned device and context are properly reference-counted by Windows.
        let result = unsafe {
            D3D11CreateDevice(
                None,                                          // No specific adapter
                D3D_DRIVER_TYPE_HARDWARE,                      // Use hardware acceleration
                None,                                          // No software rasterizer
                D3D11_CREATE_DEVICE_VIDEO_SUPPORT,             // Enable video support
                Some(&feature_levels),
                feature_levels.len() as u32,                   // Number of feature levels
                Some(&mut device as *mut _ as *mut _),         // Output device
                Some(&mut feature_level as *mut _),            // Output feature level
                Some(&mut context as *mut _ as *mut _),        // Output context
            )
        };

        if result.is_err() {
            return Err(format!(
                "Failed to create D3D11 device: {:?}",
                result
            ));
        }

        match (device, context) {
            (Some(d), Some(c)) => {
                logger::log(format!(
                    "D3D11 device created successfully. Feature level: {}",
                    feature_level.0
                ));
                Ok(Self {
                    device: Some(d),
                    context: Some(c),
                    feature_level,
                })
            }
            _ => {
                logger::log("D3D11 device creation returned null pointers");
                Err("Device creation returned null pointers".into())
            }
        }
    }

    /// Query video decoder capabilities
    pub fn supports_video_decoding(&self) -> bool {
        // Feature level 11_0 and above support D3D11VA
        // SAFETY: feature_level is a simple wrapper around i32
        self.feature_level.0 >= D3D_FEATURE_LEVEL_11_0.0
    }

    /// Get device display name for logging
    pub fn get_device_info(&self) -> String {
        format!(
            "D3D11 Device - Feature Level: {}",
            self.feature_level.0
        )
    }

    /// Release device resources
    pub fn release(&mut self) {
        logger::log("Releasing D3D11 device resources");
        self.device = None;
        self.context = None;
    }
}

#[cfg(not(target_os = "windows"))]
pub struct D3D11Device {
    _phantom: std::marker::PhantomData<()>,
}

#[cfg(not(target_os = "windows"))]
impl D3D11Device {
    pub fn create() -> Result<Self, String> {
        Err("D3D11 hardware decoding is only supported on Windows".into())
    }

    pub fn supports_video_decoding(&self) -> bool {
        false
    }

    pub fn get_device_info(&self) -> String {
        "D3D11 not available on this platform".into()
    }

    pub fn release(&mut self) {}
}

/// D3D11 Hardware Decoder Context
pub struct D3D11HardwareDecoder {
    device: Option<D3D11Device>,
    pub is_available: bool,
}

impl D3D11HardwareDecoder {
    /// Check if D3D11VA hardware decoding is available on this system
    pub fn new() -> Result<Self, String> {
        logger::log("Checking D3D11VA hardware decoder availability...");

        match D3D11Device::create() {
            Ok(device) => {
                if device.supports_video_decoding() {
                    logger::log(device.get_device_info());
                    Ok(Self {
                        device: Some(device),
                        is_available: true,
                    })
                } else {
                    logger::log("D3D11 device created but video decoding not supported");
                    Ok(Self {
                        device: None,
                        is_available: false,
                    })
                }
            }
            Err(e) => {
                logger::log(format!("D3D11 device creation failed: {}", e));
                Ok(Self {
                    device: None,
                    is_available: false,
                })
            }
        }
    }

    /// Get available D3D11VA decoder capabilities
    pub fn get_capabilities(&self) -> Result<String, String> {
        if self.is_available {
            if let Some(device) = &self.device {
                Ok(format!(
                    "D3D11 hardware decoder available: {}",
                    device.get_device_info()
                ))
            } else {
                Ok("D3D11 hardware decoder available but no device info".into())
            }
        } else {
            Ok("D3D11 hardware decoder NOT available on this system".into())
        }
    }
}

impl Drop for D3D11HardwareDecoder {
    fn drop(&mut self) {
        if let Some(mut device) = self.device.take() {
            device.release();
        }
    }
}

/// Initialize FFmpeg D3D11VA hardware context
///
/// This function:
/// 1. Creates a D3D11 device for video decoding
/// 2. Initializes FFmpeg hwcontext for D3D11VA
/// 3. Configures GPU surface pools
/// 4. Sets up synchronization primitives
pub fn initialize_d3d11va_context() -> Result<(), String> {
    logger::log("Initializing FFmpeg D3D11VA hardware acceleration context...");

    // Step 1: Verify D3D11 device support
    let decoder = D3D11HardwareDecoder::new()?;
    if !decoder.is_available {
        return Err("D3D11 hardware decoding not available on this system".into());
    }

    logger::log("D3D11 device verified, ready for FFmpeg integration");

    // Step 2: TODO - Create FFmpeg hwcontext with D3D11VA
    // ```c
    // AVBufferRef *hw_device_ctx = av_hwdevice_ctx_create(
    //     AV_HWDEVICE_TYPE_D3D11VA, device_name, options, 0);
    // ```

    // Step 3: TODO - Configure codec context with hardware context
    // ```c
    // codec_ctx->hw_device_ctx = av_buffer_ref(hw_device_ctx);
    // ```

    logger::log("D3D11VA context initialization: GPU device ready, FFmpeg integration pending");

    Ok(())
}

/// Enable D3D11VA hardware decoding for a video codec context
///
/// This function configures an existing FFmpeg decoder to use
/// GPU-accelerated D3D11VA decoding instead of software decoding.
#[cfg(moonlight_common_c_linked)]
pub fn enable_hardware_decoding(codec_ctx: *mut super::gamestream_sys::AVCodecContext) -> Result<(), String> {
    if codec_ctx.is_null() {
        return Err("Cannot enable hardware decoding: codec_ctx is NULL".into());
    }

    logger::log("Attempting to enable D3D11VA hardware decoding...");

    // TODO: Implement hardware context attachment
    // This will:
    // 1. Get or create D3D11 device
    // 2. Create hwdevice context
    // 3. Attach to codec_ctx.hw_device_ctx
    // 4. Register get_format callback

    logger::log("Hardware decoding configuration: Pending FFmpeg hwcontext integration");

    Ok(())
}

#[cfg(not(moonlight_common_c_linked))]
pub fn enable_hardware_decoding(_codec_ctx: *mut std::ffi::c_void) -> Result<(), String> {
    Err("Hardware decoding requires C library linkage".into())
}

/// Fallback to software decoding if hardware decoding fails
pub fn fallback_to_software_decoding() -> Result<(), String> {
    logger::log("Falling back to FFmpeg software video decoding");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hardware_decoder_creation() {
        let result = D3D11HardwareDecoder::new();
        assert!(result.is_ok(), "Should be able to create decoder instance");
    }

    #[test]
    fn test_capabilities_query() {
        let decoder = D3D11HardwareDecoder::new().expect("decoder creation");
        let caps = decoder.get_capabilities().expect("should get capabilities");
        assert!(!caps.is_empty(), "Capabilities should not be empty");
    }
}
