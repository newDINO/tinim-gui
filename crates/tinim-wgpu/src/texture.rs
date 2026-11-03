//! Texture storage for the renderer.
//!
//! Textures are `Rgba8UnormSrgb`: RGB is sRGB color with straight alpha, and the sampler converts
//! it to linear space for the shader. Alpha is linear coverage (for example glyph coverage) and is
//! not affected by the sRGB conversion. Untextured geometry samples the renderer's 1×1 opaque
//! white texture, so there is a single texture format for everything.
//!
//! Uploads must therefore contain straight-alpha sRGB pixels. Non-color data (masks, normals,
//! lookup tables) and premultiplied-alpha images need a different format or a preprocessing step.

use tinim_paint::ImageDelta;

/// Shader-visible texture format, shared by the white texel and user textures.
///
/// The sampler decodes RGB from sRGB to linear; upload sRGB-encoded, straight-alpha pixels.
pub const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

pub(crate) struct TextureEntry {
    pub(crate) texture: wgpu::Texture,
    pub(crate) bind_group: wgpu::BindGroup,
}

impl TextureEntry {
    pub(crate) fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        size: [u32; 2],
        label: &str,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: size[0].max(1),
                height: size[1].max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        Self {
            texture,
            bind_group,
        }
    }

    pub(crate) fn upload(&self, queue: &wgpu::Queue, delta: &ImageDelta) {
        if delta.size[0] == 0 || delta.size[1] == 0 {
            return;
        }

        let origin = match delta.pos {
            Some([x, y]) => wgpu::Origin3d { x, y, z: 0 },
            None => wgpu::Origin3d::ZERO,
        };

        // Partial updates must stay inside the texture.
        debug_assert!(origin.x + delta.size[0] <= self.texture.width());
        debug_assert!(origin.y + delta.size[1] <= self.texture.height());

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin,
                aspect: wgpu::TextureAspect::All,
            },
            &delta.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(delta.size[0] * 4),
                rows_per_image: Some(delta.size[1]),
            },
            wgpu::Extent3d {
                width: delta.size[0],
                height: delta.size[1],
                depth_or_array_layers: 1,
            },
        );
    }
}
