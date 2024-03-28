use super::Renderer;

pub struct Texture {
    texture: wgpu::Texture,
    size: wgpu::Extent3d,
    id: egui::TextureId,
}

impl Texture {
    pub fn new(
        renderer: &mut Renderer,
        width: u32,
        height: u32,
        label: Option<&'static str>,
    ) -> Self {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
            label,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let id = renderer.egui.renderer.register_native_texture(
            &renderer.device,
            &view,
            wgpu::FilterMode::Nearest,
        );
        Self { id, texture, size }
    }

    pub fn update(&self, queue: &wgpu::Queue, bytes: &[u8]) {
        queue.write_texture(
            wgpu::ImageCopyTexture {
                aspect: wgpu::TextureAspect::All,
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            bytes,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * self.size.width),
                rows_per_image: Some(self.size.height),
            },
            self.size,
        );
    }
    pub fn get_id(&self) -> egui::TextureId {
        self.id
    }
}
