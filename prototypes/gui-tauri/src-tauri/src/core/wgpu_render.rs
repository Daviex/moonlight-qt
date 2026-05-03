/// wgpu + winit rendering for hardware-decoded NV12 frames
///
/// FFmpeg D3D11VA decodes to NV12 GPU textures. We read them back
/// to CPU and upload as wgpu textures, then render via WGSL shader.

use std::sync::Arc;
use winit::window::Window;
use wgpu::*;

mod logger {
    pub fn log(msg: impl AsRef<str>) { crate::logger::stream(msg); }
}

const NV12_SHADER: &str = r#"
@group(0) @binding(0) var y_tex: texture_2d<f32>;
@group(0) @binding(1) var uv_tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(vi & 1u) * 2 - 1);
    let y = f32(1 - i32(vi & 2u));
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let y = textureSample(y_tex, samp, in.uv).r;
    let uv_sample = textureSample(uv_tex, samp, in.uv).rg;
    let y_val = 1.1644 * (y - 0.0625);
    let u_val = uv_sample.r - 0.5;
    let v_val = uv_sample.g - 0.5;
    let r = y_val + 1.7927 * v_val;
    let g = y_val - 0.2132 * u_val - 0.5329 * v_val;
    let b = y_val + 2.1124 * u_val;
    return vec4<f32>(r, g, b, 1.0);
}
"#;

pub struct WgpuRenderer {
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    config: SurfaceConfiguration,
    pipeline: RenderPipeline,
    bind_group_layout: BindGroupLayout,
}

impl WgpuRenderer {
    pub async fn new(window: Arc<Window>) -> Result<Self, String> {
        let size = window.inner_size();
        let instance = Instance::default();
        let surface = instance.create_surface(window.clone())
            .map_err(|e| format!("create_surface: {e}"))?;
        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or("no wgpu adapter")?;
        let (device, queue) = adapter
            .request_device(&DeviceDescriptor::default(), None)
            .await
            .map_err(|e| format!("request_device: {e}"))?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps.formats.iter()
            .find(|f| f.is_srgb())
            .or_else(|| surface_caps.formats.first())
            .copied()
            .ok_or("no surface format")?;

        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: PresentMode::Immediate,
            alpha_mode: CompositeAlphaMode::Auto,
            desired_maximum_frame_latency: 1,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("nv12_shader"),
            source: ShaderSource::Wgsl(NV12_SHADER.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("nv12_bgl"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0, visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1, visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2, visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("nv12_pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("nv12_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(ColorTargetState {
                    format: surface_format,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
        });

        logger::log("wgpu renderer ready");
        Ok(Self { surface, device, queue, config, pipeline, bind_group_layout })
    }

    /// Render an NV12 frame. `y_plane` is W*H bytes, `uv_plane` is (W/2)*(H/2)*2 bytes.
    pub fn render_nv12(&mut self, y_plane: &[u8], uv_plane: &[u8], width: u32, height: u32) -> Result<(), String> {
        let device = &self.device;
        let queue = &self.queue;

        // Y texture (R8)
        let y_tex = device.create_texture(&TextureDescriptor {
            label: Some("y_plane"),
            size: Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            ImageCopyTexture { texture: &y_tex, mip_level: 0, origin: Origin3d::ZERO, aspect: TextureAspect::All },
            y_plane,
            ImageDataLayout { offset: 0, bytes_per_row: Some(width), rows_per_image: Some(height) },
            Extent3d { width, height, depth_or_array_layers: 1 },
        );

        // UV texture (RG8, half resolution)
        let uv_w = width / 2;
        let uv_h = height / 2;
        let uv_tex = device.create_texture(&TextureDescriptor {
            label: Some("uv_plane"),
            size: Extent3d { width: uv_w, height: uv_h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rg8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            ImageCopyTexture { texture: &uv_tex, mip_level: 0, origin: Origin3d::ZERO, aspect: TextureAspect::All },
            uv_plane,
            ImageDataLayout { offset: 0, bytes_per_row: Some(width), rows_per_image: Some(uv_h) },
            Extent3d { width: uv_w, height: uv_h, depth_or_array_layers: 1 },
        );

        let y_view = y_tex.create_view(&TextureViewDescriptor::default());
        let uv_view = uv_tex.create_view(&TextureViewDescriptor::default());
        let sampler = device.create_sampler(&SamplerDescriptor {
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("nv12_bg"),
            layout: &self.bind_group_layout,
            entries: &[
                BindGroupEntry { binding: 0, resource: BindingResource::TextureView(&y_view) },
                BindGroupEntry { binding: 1, resource: BindingResource::TextureView(&uv_view) },
                BindGroupEntry { binding: 2, resource: BindingResource::Sampler(&sampler) },
            ],
        });

        let output = self.surface.get_current_texture()
            .map_err(|e| format!("get_current_texture: {e}"))?;
        let view = output.texture.create_view(&TextureViewDescriptor::default());

        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
        {
            let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("nv12_pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations { load: LoadOp::Clear(Color::BLACK), store: StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rpass.set_pipeline(&self.pipeline);
            rpass.set_bind_group(0, &bind_group, &[]);
            rpass.draw(0..3, 0..1);
        }
        queue.submit(Some(encoder.finish()));
        output.present();

        Ok(())
    }
}
