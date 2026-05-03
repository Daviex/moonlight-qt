/// Native D3D11 renderer for hardware-decoded NV12 frames
///
/// Bypasses SDL3 rendering to render NV12 decode surfaces directly to a
/// D3D11 swapchain using a pixel shader for YUV→RGB conversion.
/// SDL3 is used only for window creation and input handling.

#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Direct3D11::*;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Direct3D::*;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Dxgi::*;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Dxgi::Common::*;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::*;

use std::mem;

mod logger {
    pub fn log(message: impl AsRef<str>) {
        crate::logger::stream(message);
    }
}

const VERTEX_SHADER_BYTECODE: &[u8] = include_bytes!("../../../../../app/shaders/d3d11_vertex.fxc");
const PIXEL_SHADER_BYTECODE: &[u8] = include_bytes!("../../../../../app/shaders/d3d11_yuv420_pixel.fxc");

#[repr(C)]
#[derive(Clone, Copy)]
struct CscConstants {
    csc_matrix: [f32; 12],
    offsets: [f32; 4],
    chroma_offset: [f32; 2],
    chroma_tex_max: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Vertex {
    pos: [f32; 2],
    tex: [f32; 2],
}

#[cfg(target_os = "windows")]
pub struct D3D11Renderer {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    swapchain: IDXGISwapChain,
    swapchain_rtv: Option<ID3D11RenderTargetView>,
    vertex_shader: ID3D11VertexShader,
    pixel_shader: ID3D11PixelShader,
    input_layout: ID3D11InputLayout,
    sampler: ID3D11SamplerState,
    csc_buffer: ID3D11Buffer,
    width: u32,
    height: u32,
}

#[cfg(target_os = "windows")]
impl D3D11Renderer {
    pub fn create(hwnd: *mut std::ffi::c_void, width: u32, height: u32) -> Result<Self, String> {
        logger::log(format!("D3D11 renderer: {}x{} HWND={:p}", width, height, hwnd));

        // ── D3D11 device ──
        let feature_levels = [
            D3D_FEATURE_LEVEL_11_1,
            D3D_FEATURE_LEVEL_11_0,
        ];
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        let mut feature_level = D3D_FEATURE_LEVEL_11_0;

        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
                Some(&feature_levels),
                7, // D3D11_SDK_VERSION
                Some(&mut device as *mut Option<ID3D11Device>),
                Some(&mut feature_level as *mut D3D_FEATURE_LEVEL),
                Some(&mut context as *mut Option<ID3D11DeviceContext>),
            )
        }
        .map_err(|e| format!("D3D11CreateDevice: {e:?}"))?;

        let device = device.ok_or("D3D11 device null")?;
        let context = context.ok_or("D3D11 context null")?;
        logger::log(format!("D3D11 device ready, FL {feature_level:?}"));

        // ── Swapchain ──
        let dxgi: IDXGIFactory2 = unsafe {
            CreateDXGIFactory2(0)
        }
        .map_err(|e| format!("CreateDXGIFactory2: {e:?}"))?;

        let swapchain = Self::create_swapchain(&dxgi, &device, hwnd, width, height)?;
        let swapchain_rtv = Self::create_backbuffer_rtv(&device, &swapchain)?;

        // ── Shaders ──
        let vertex_shader = {
            let mut sh: Option<ID3D11VertexShader> = None;
            unsafe {
                device.CreateVertexShader(
                    VERTEX_SHADER_BYTECODE,
                    None,
                    Some(&mut sh as *mut Option<ID3D11VertexShader>),
                )
            }
            .map_err(|e| format!("CreateVertexShader: {e:?}"))?;
            sh.ok_or("vertex shader null")?
        };

        let pixel_shader = {
            let mut sh: Option<ID3D11PixelShader> = None;
            unsafe {
                device.CreatePixelShader(
                    PIXEL_SHADER_BYTECODE,
                    None,
                    Some(&mut sh as *mut Option<ID3D11PixelShader>),
                )
            }
            .map_err(|e| format!("CreatePixelShader: {e:?}"))?;
            sh.ok_or("pixel shader null")?
        };

        // ── Input layout ──
        let input_elements = [
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: windows::core::s!("POSITION"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 0,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: windows::core::s!("TEXCOORD"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 8,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
        ];

        let input_layout = {
            let mut layout: Option<ID3D11InputLayout> = None;
            unsafe {
                device.CreateInputLayout(
                    &input_elements,
                    VERTEX_SHADER_BYTECODE,
                    Some(&mut layout as *mut Option<ID3D11InputLayout>),
                )
            }
            .map_err(|e| format!("CreateInputLayout: {e:?}"))?;
            layout.ok_or("input layout null")?
        };

        // ── Sampler ──
        let sampler = {
            let desc = D3D11_SAMPLER_DESC {
                Filter: D3D11_FILTER_MIN_MAG_LINEAR_MIP_POINT,
                AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                MipLODBias: 0.0,
                MaxAnisotropy: 1,
                ComparisonFunc: D3D11_COMPARISON_NEVER,
                BorderColor: [0.0_f32, 0.0, 0.0, 0.0],
                MinLOD: -f32::MAX,
                MaxLOD: f32::MAX,
            };
            let mut s: Option<ID3D11SamplerState> = None;
            unsafe { device.CreateSamplerState(&desc, Some(&mut s as *mut Option<ID3D11SamplerState>)) }
                .map_err(|e| format!("CreateSamplerState: {e:?}"))?;
            s.ok_or("sampler null")?
        };

        // ── CSC constant buffer ──
        let csc_buffer = Self::create_csc_buffer(&device)?;

        // ── Viewport ──
        let viewport = D3D11_VIEWPORT {
            TopLeftX: 0.0,
            TopLeftY: 0.0,
            Width: width as f32,
            Height: height as f32,
            MinDepth: 0.0,
            MaxDepth: 1.0,
        };
        unsafe { context.RSSetViewports(Some(&[viewport])) };

        logger::log("D3D11 renderer ready");
        Ok(Self {
            device,
            context,
            swapchain,
            swapchain_rtv: Some(swapchain_rtv),
            vertex_shader,
            pixel_shader,
            input_layout,
            sampler,
            csc_buffer,
            width,
            height,
        })
    }

    fn create_swapchain(
        factory: &IDXGIFactory2,
        device: &ID3D11Device,
        hwnd: *mut std::ffi::c_void,
        width: u32,
        height: u32,
    ) -> Result<IDXGISwapChain, String> {
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: width,
            Height: height,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            Stereo: false.into(),
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            AlphaMode: DXGI_ALPHA_MODE_IGNORE,
            Flags: 0,
        };

        let mut swapchain: Option<IDXGISwapChain> = None;
        // CreateSwapChainForHwnd returns the swapchain directly via generic T
        let result = unsafe {
            factory.CreateSwapChainForHwnd(
                device,
                HWND(hwnd as isize),
                &desc,
                None,
                None::<&IDXGIOutput>,
            )
        }
        .map_err(|e| format!("CreateSwapChainForHwnd: {e:?}"))?;
        let swapchain: IDXGISwapChain = result.into();
        Ok(swapchain)
    }

    fn create_backbuffer_rtv(
        device: &ID3D11Device,
        swapchain: &IDXGISwapChain,
    ) -> Result<ID3D11RenderTargetView, String> {
        let backbuffer: ID3D11Texture2D = unsafe { swapchain.GetBuffer(0) }
            .map_err(|e| format!("GetBuffer: {e:?}"))?;

        let mut rtv: Option<ID3D11RenderTargetView> = None;
        unsafe {
            device.CreateRenderTargetView(
                &backbuffer,
                None,
                Some(&mut rtv as *mut Option<ID3D11RenderTargetView>),
            )
        }
        .map_err(|e| format!("CreateRenderTargetView: {e:?}"))?;
        rtv.ok_or("RTV null".into())
    }

    fn create_csc_buffer(device: &ID3D11Device) -> Result<ID3D11Buffer, String> {
        let constants = CscConstants {
            csc_matrix: [1.1644, 1.1644, 1.1644, 0.0, 0.0, -0.2132, 2.1124, 0.0, 1.7927, -0.5329, 0.0, 0.0],
            offsets: [16.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0, 0.0],
            chroma_offset: [0.0, 0.0],
            chroma_tex_max: [0.5, 0.5],
        };
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: mem::size_of::<CscConstants>() as u32,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
            StructureByteStride: 0,
        };
        let init_data = D3D11_SUBRESOURCE_DATA {
            pSysMem: &constants as *const _ as *const _,
            SysMemPitch: 0,
            SysMemSlicePitch: 0,
        };
        let mut buffer: Option<ID3D11Buffer> = None;
        unsafe {
            device.CreateBuffer(&desc, Some(&init_data), Some(&mut buffer as *mut Option<ID3D11Buffer>))
        }
        .map_err(|e| format!("CreateBuffer(CSC): {e:?}"))?;
        buffer.ok_or("CSC buffer null".into())
    }

    fn create_vertex_buffer(device: &ID3D11Device, vertices: &[Vertex]) -> Result<ID3D11Buffer, String> {
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: (vertices.len() * mem::size_of::<Vertex>()) as u32,
            Usage: D3D11_USAGE_IMMUTABLE,
            BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
            StructureByteStride: 0,
        };
        let init_data = D3D11_SUBRESOURCE_DATA {
            pSysMem: vertices.as_ptr() as *const _,
            SysMemPitch: 0,
            SysMemSlicePitch: 0,
        };
        let mut buffer: Option<ID3D11Buffer> = None;
        unsafe {
            device.CreateBuffer(&desc, Some(&init_data), Some(&mut buffer as *mut Option<ID3D11Buffer>))
        }
        .map_err(|e| format!("CreateBuffer(VB): {e:?}"))?;
        buffer.ok_or("VB null".into())
    }

    fn fullscreen_quad() -> [Vertex; 6] {
        [
            Vertex { pos: [-1.0, -1.0], tex: [0.0, 1.0] },
            Vertex { pos: [ 1.0, -1.0], tex: [1.0, 1.0] },
            Vertex { pos: [-1.0,  1.0], tex: [0.0, 0.0] },
            Vertex { pos: [ 1.0, -1.0], tex: [1.0, 1.0] },
            Vertex { pos: [ 1.0,  1.0], tex: [1.0, 0.0] },
            Vertex { pos: [-1.0,  1.0], tex: [0.0, 0.0] },
        ]
    }

    /// Get the raw ID3D11Device pointer for sharing with FFmpeg.
    pub fn get_device_ptr(&self) -> *mut std::ffi::c_void {
        // Clone bumps refcount, then we extract raw pointer
        let cloned = self.device.clone();
        let ptr = unsafe {
            let raw: *mut std::ffi::c_void = std::mem::transmute_copy(&cloned);
            raw
        };
        std::mem::forget(cloned);
        ptr
    }

    pub fn device(&self) -> &ID3D11Device { &self.device }
    pub fn context(&self) -> &ID3D11DeviceContext { &self.context }

    /// Render an NV12 decode surface to the swapchain backbuffer.
    pub fn render_nv12_frame(
        &mut self,
        nv12_texture: &ID3D11Texture2D,
        frame_width: u32,
        frame_height: u32,
    ) -> Result<(), String> {
        // Update CSC constants (must happen before taking ctx ref)
        self.update_csc_constants(frame_width, frame_height)?;

        unsafe {
            let ctx = &self.context;

            // Y-plane SRV: R8_UNORM
            let luma_desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
                Format: DXGI_FORMAT_R8_UNORM,
                ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_SRV { MostDetailedMip: 0, MipLevels: 1 },
                },
            };
            let mut luma_srv: Option<ID3D11ShaderResourceView> = None;
            self.device.CreateShaderResourceView(
                nv12_texture,
                Some(&luma_desc),
                Some(&mut luma_srv as *mut Option<ID3D11ShaderResourceView>),
            )
            .map_err(|e| format!("Y SRV: {e:?}"))?;
            let luma_srv = luma_srv.ok_or("Y SRV null")?;

            // UV-plane SRV: R8G8_UNORM
            let chroma_desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
                Format: DXGI_FORMAT_R8G8_UNORM,
                ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_SRV { MostDetailedMip: 0, MipLevels: 1 },
                },
            };
            let mut chroma_srv: Option<ID3D11ShaderResourceView> = None;
            self.device.CreateShaderResourceView(
                nv12_texture,
                Some(&chroma_desc),
                Some(&mut chroma_srv as *mut Option<ID3D11ShaderResourceView>),
            )
            .map_err(|e| format!("UV SRV: {e:?}"))?;
            let chroma_srv = chroma_srv.ok_or("UV SRV null")?;

            // Clear + set render target
            let rtv = self.swapchain_rtv.as_ref().ok_or("no backbuffer RTV")?;
            let clear_color = [0.0f32, 0.0, 0.0, 1.0];
            ctx.ClearRenderTargetView(rtv, &clear_color);
            ctx.OMSetRenderTargets(
                Some(&[Some(rtv.clone())]),
                None,
            );

            // Pipeline
            ctx.IASetInputLayout(&self.input_layout);
            ctx.VSSetShader(&self.vertex_shader, None);
            ctx.PSSetShader(&self.pixel_shader, None);
            ctx.PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
            ctx.PSSetConstantBuffers(0, Some(&[Some(self.csc_buffer.clone())]));
            ctx.PSSetShaderResources(0, Some(&[Some(luma_srv)]));
            ctx.PSSetShaderResources(1, Some(&[Some(chroma_srv)]));

            // Fullscreen quad
            let vertices = Self::fullscreen_quad();
            let vb = Self::create_vertex_buffer(&self.device, &vertices)?;
            let vb_ptr: Option<ID3D11Buffer> = Some(vb);
            let stride = mem::size_of::<Vertex>() as u32;
            let offset = 0u32;
            ctx.IASetVertexBuffers(
                0,
                1,
                Some(&vb_ptr as *const Option<ID3D11Buffer>),
                Some(&stride as *const u32),
                Some(&offset as *const u32),
            );
            ctx.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);

            // Draw
            ctx.Draw(6, 0);
        }
        Ok(())
    }

    fn update_csc_constants(&mut self, frame_width: u32, frame_height: u32) -> Result<(), String> {
        let y_scale = 255.0 / 219.0;
        let uv_scale = 255.0 / 224.0;
        let csc: [f32; 12] = [
            y_scale,              y_scale,              y_scale,              0.0,
            0.0,                  -0.1873 * uv_scale,   1.8556 * uv_scale,   0.0,
            1.5748 * uv_scale,   -0.4681 * uv_scale,   0.0,                  0.0,
        ];
        let chroma_off_x = if frame_width > 0 { 0.5 / (frame_width / 2) as f32 } else { 0.0 };
        let chroma_off_y = if frame_height > 0 { 0.5 / (frame_height / 2) as f32 } else { 0.0 };
        let constants = CscConstants {
            csc_matrix: csc,
            offsets: [16.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0, 0.0],
            chroma_offset: [chroma_off_x, chroma_off_y],
            chroma_tex_max: [0.5, 0.5],
        };
        unsafe {
            self.context.UpdateSubresource(
                &self.csc_buffer,
                0,
                None,
                &constants as *const _ as *const _,
                0,
                0,
            );
        }
        Ok(())
    }

    pub fn present(&self) -> Result<(), String> {
        unsafe { self.swapchain.Present(0, 0) }
            .ok()
            .map_err(|e| format!("Present: {e:?}"))
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        if width == 0 || height == 0 || (width == self.width && height == self.height) {
            return Ok(());
        }
        logger::log(format!("D3D11 resize: {}x{} -> {}x{}", self.width, self.height, width, height));
        self.width = width;
        self.height = height;
        // Release RTV before resize
        self.swapchain_rtv = None;
        unsafe {
            self.swapchain.ResizeBuffers(2, width, height, DXGI_FORMAT_B8G8R8A8_UNORM, 0)
        }
        .map_err(|e| format!("ResizeBuffers: {e:?}"))?;
        self.swapchain_rtv = Some(Self::create_backbuffer_rtv(&self.device, &self.swapchain)?);
        Ok(())
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

// ── Non-Windows stub ──
#[cfg(not(target_os = "windows"))]
pub struct D3D11Renderer;

#[cfg(not(target_os = "windows"))]
impl D3D11Renderer {
    pub fn create(_: *mut std::ffi::c_void, _: u32, _: u32) -> Result<Self, String> {
        Err("D3D11 unsupported platform".into())
    }
    pub fn render_nv12_frame(&mut self, _: &(), _: u32, _: u32) -> Result<(), String> { Ok(()) }
    pub fn present(&self) -> Result<(), String> { Ok(()) }
    pub fn resize(&mut self, _: u32, _: u32) -> Result<(), String> { Ok(()) }
    pub fn dimensions(&self) -> (u32, u32) { (0, 0) }
    pub fn get_device_ptr(&self) -> *mut std::ffi::c_void { std::ptr::null_mut() }
    pub fn device(&self) -> &() { &() }
    pub fn context(&self) -> &() { &() }
}
