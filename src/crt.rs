//! Runs the CRT shader over the framebuffer. Windows and headless renders share this one path.
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    size: [f32; 2],
    src: [f32; 2],
    scale: f32,
    zoom: f32,
    srgb: f32,
    _pad: f32,
}

/// What the shader needs to know about the output.
#[derive(Clone, Copy)]
pub struct View {
    pub size: (f32, f32),
    pub scale: f32,
    pub zoom: f32,
}

pub struct Crt {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
    source: Option<Source>,
    srgb: bool,
}

struct Source {
    texture: wgpu::Texture,
    bind: wgpu::BindGroup,
    size: (u32, u32),
}

impl Crt {
    pub async fn new(adapter: &wgpu::Adapter, format: wgpu::TextureFormat) -> Self {
        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.expect("no gpu device");
        let shader = device.create_shader_module(wgpu::include_wgsl!("crt.wgsl"));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("crt"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { device, queue, pipeline, layout, sampler, uniforms, source: None, srgb: format.is_srgb() }
    }

    /// Upload the painted screen. `rgba` is width x height x 4, in font pixels.
    pub fn upload(&mut self, width: u32, height: u32, rgba: &[u8]) {
        if self.source.as_ref().map(|s| s.size) != Some((width, height)) {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("screen"),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                // Not sRGB: the Swift shader works on gamma values, so this one must too or the glow changes.
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: self.uniforms.as_entire_binding() },
                ],
            });
            self.source = Some(Source { texture, bind, size: (width, height) });
        }
        let source = self.source.as_ref().unwrap();
        self.queue.write_texture(
            source.texture.as_image_copy(),
            rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * 4), rows_per_image: Some(height) },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
    }

    /// Draw into `target`. Call `upload` first.
    pub fn draw(&self, target: &wgpu::TextureView, encoder: &mut wgpu::CommandEncoder, view: View) {
        let source = self.source.as_ref().expect("upload before draw");
        self.queue.write_buffer(
            &self.uniforms,
            0,
            bytemuck::bytes_of(&Uniforms {
                size: [view.size.0, view.size.1],
                src: [source.size.0 as f32, source.size.1 as f32],
                scale: view.scale,
                zoom: view.zoom,
                srgb: if self.srgb { 1.0 } else { 0.0 },
                _pad: 0.0,
            }),
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                depth_slice: None,
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &source.bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

impl Crt {
    /// Finish `encoder`, copy `texture` out and wait for the pixels. Row order top down, in the texture's own format.
    pub fn read_back(&self, texture: &wgpu::Texture, mut encoder: wgpu::CommandEncoder) -> Option<Vec<u8>> {
        let (width, height) = (texture.width(), texture.height());
        let row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(height) },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        self.queue.submit(Some(encoder.finish()));
        readback.map_async(wgpu::MapMode::Read, .., |r| r.expect("map failed"));
        self.device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        let data = readback.get_mapped_range(..).ok()?;
        let mut out = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height as usize {
            let start = y * row as usize;
            out.extend_from_slice(&data[start..start + width as usize * 4]);
        }
        Some(out)
    }
}

/// Render without a window and return RGBA pixels. No display needed.
pub async fn headless(width: u32, height: u32, fb: (u32, u32, &[u8]), view: View) -> Option<Vec<u8>> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.ok()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut crt = Crt::new(&adapter, format).await;
    crt.upload(fb.0, fb.1, fb.2);
    let extent = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
    let target = crt.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("headless"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut encoder = crt.device.create_command_encoder(&Default::default());
    crt.draw(&target.create_view(&Default::default()), &mut encoder, view);
    crt.read_back(&target, encoder)
}
