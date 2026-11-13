use super::Renderer;

pub struct Texture {
    texture: wgpu::Texture,
    size: wgpu::Extent3d,
    bytes_per_texel: u32,
}

impl Texture {
    pub fn new(
        renderer: &mut Renderer,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
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
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        Self {
            texture,
            size,
            bytes_per_texel: format
                .block_copy_size(None)
                .expect("an uncompressed texture format"),
        }
    }

    pub fn view(&self) -> wgpu::TextureView {
        self.texture
            .create_view(&wgpu::TextureViewDescriptor::default())
    }

    pub fn update(&self, queue: &wgpu::Queue, bytes: &[u8]) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                aspect: wgpu::TextureAspect::All,
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.bytes_per_texel * self.size.width),
                rows_per_image: Some(self.size.height),
            },
            self.size,
        );
    }
}
