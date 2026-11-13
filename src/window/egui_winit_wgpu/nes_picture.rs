//! Draws the NES picture with sharp bilinear filtering (`nes_picture.wgsl`), as an egui paint
//! callback: it's part of the UI like any widget, under the menu and the messages.

use egui_wgpu::{CallbackResources, CallbackTrait, ScreenDescriptor};

use super::{Renderer, texture::Texture};

/// The pipeline and its bindings, made once and kept in egui's callback resources.
struct Resources {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniforms: wgpu::Buffer,
    srgb_target: bool,
}

/// `Uniforms` in the shader: four vec2/vec4 rows of f32.
const UNIFORMS_SIZE: u64 = 12 * 4;

/// Sets up drawing `texture` (the NES frame) for `NesPicture`.
pub fn register(renderer: &mut Renderer, texture: &Texture) {
    let device = &renderer.device;
    let format = renderer.config.format;

    let module = device.create_shader_module(wgpu::include_wgsl!("nes_picture.wgsl"));
    let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("nes picture uniforms"),
        size: UNIFORMS_SIZE,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    // Linear: the shader picks where to sample so that it only blends across a pixel edge.
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("nes picture sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("nes picture bind group layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(UNIFORMS_SIZE),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let view = texture.view();
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("nes picture bind group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("nes picture pipeline layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("nes picture pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        multiview_mask: None,
        cache: None,
    });

    renderer.egui.renderer.callback_resources.insert(Resources {
        pipeline,
        bind_group,
        uniforms,
        srgb_target: format.is_srgb(),
    });
}

/// The NES picture, `size` physical pixels big (as drawn: the paint callback's rectangle), tinted
/// by `tint` (gamma-space RGBA, like egui's colors).
pub struct NesPicture {
    pub size: [f32; 2],
    pub tint: [f32; 4],
}

impl CallbackTrait for NesPicture {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(resources) = callback_resources.get::<Resources>() {
            let uniforms: [f32; 12] = [
                crate::emulation::NES_WIDTH as f32,
                crate::emulation::NES_HEIGHT as f32,
                self.size[0],
                self.size[1],
                self.tint[0],
                self.tint[1],
                self.tint[2],
                self.tint[3],
                if resources.srgb_target { 1.0 } else { 0.0 },
                0.0,
                0.0,
                0.0,
            ];
            let bytes: Vec<u8> = uniforms
                .iter()
                .flat_map(|value| value.to_ne_bytes())
                .collect();
            queue.write_buffer(&resources.uniforms, 0, &bytes);
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &CallbackResources,
    ) {
        if let Some(resources) = callback_resources.get::<Resources>() {
            render_pass.set_pipeline(&resources.pipeline);
            render_pass.set_bind_group(0, &resources.bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }
    }
}
