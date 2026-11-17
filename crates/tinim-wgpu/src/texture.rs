//! Texture storage for the renderer.
//!
//! Uploads are straight-alpha sRGB pixels. Textures are `Rgba8Unorm`, not `Rgba8UnormSrgb`, so the
//! sampler returns the stored values unchanged and the renderer blends them in encoded (gamma)
//! space. This matches a browser's UI compositor; see the crate-level renderer docs.
//!
//! Untextured geometry samples the renderer's 1×1 opaque white texture, so there is one texture
//! format for everything. Non-color data (masks, normals, lookup tables) and premultiplied-alpha
//! images need a different format or a preprocessing step.

use tinim_paint::{
    ImageDelta, TextureAddressMode, TextureDescriptor, TextureFilter, TextureOptions,
};

/// Texture format shared by the white texel and every user texture.
///
/// RGB stays encoded; upload sRGB-encoded, straight-alpha pixels.
pub const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub(crate) struct TextureEntry {
    pub(crate) texture: wgpu::Texture,
    pub(crate) bind_group: wgpu::BindGroup,
}

impl TextureEntry {
    pub(crate) fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        descriptor: TextureDescriptor,
        label: &str,
    ) -> Self {
        let size = descriptor.size;
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

        // Each texture owns its sampler so images can choose filtering and address modes
        // independently (for example nearest-neighbor pixel art, or repeat for tiling).
        let sampler = device.create_sampler(&sampler_descriptor(descriptor.options));

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
                    resource: wgpu::BindingResource::Sampler(&sampler),
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

fn sampler_descriptor(options: TextureOptions) -> wgpu::SamplerDescriptor<'static> {
    wgpu::SamplerDescriptor {
        label: Some("tinim texture sampler"),
        address_mode_u: address_mode(options.address_mode_u),
        address_mode_v: address_mode(options.address_mode_v),
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: filter_mode(options.mag_filter),
        min_filter: filter_mode(options.min_filter),
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    }
}

fn address_mode(mode: TextureAddressMode) -> wgpu::AddressMode {
    match mode {
        TextureAddressMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
        TextureAddressMode::Repeat => wgpu::AddressMode::Repeat,
        TextureAddressMode::MirrorRepeat => wgpu::AddressMode::MirrorRepeat,
    }
}

fn filter_mode(filter: TextureFilter) -> wgpu::FilterMode {
    match filter {
        TextureFilter::Nearest => wgpu::FilterMode::Nearest,
        TextureFilter::Linear => wgpu::FilterMode::Linear,
    }
}
