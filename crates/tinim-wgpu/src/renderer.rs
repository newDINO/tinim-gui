//! The wgpu renderer: one pipeline, one texture per [`TextureId`], one scissor per draw command.

use std::collections::HashMap;
use std::hash::{BuildHasher, RandomState};
use std::mem::size_of;

use tinim_paint::{PaintList, Rect, TextureId, TexturesDelta, Vertex};

use crate::texture::TextureEntry;

const INITIAL_VERTEX_BYTES: u64 = 16 * 1024;
const INITIAL_INDEX_BYTES: u64 = 8 * 1024;

/// The render target size, in physical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScreenDescriptor {
    pub size_in_pixels: [u32; 2],
}

impl ScreenDescriptor {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            size_in_pixels: [width, height],
        }
    }
}

/// Renders [`PaintList`]s with wgpu.
///
/// Usage: create one `Renderer` per window/surface, call [`Renderer::update_textures`] with the
/// [`TexturesDelta`] of the frame, then [`Renderer::render`] inside a render pass.
///
/// `target_format` should be an sRGB format (`*Srgb`) so that the linear-space blending done by
/// the shader is converted correctly on write.
pub struct Renderer<S = RandomState> {
    pipeline: wgpu::RenderPipeline,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    textures: HashMap<TextureId, TextureEntry, S>,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
}

impl Renderer<RandomState> {
    /// Create a renderer using the standard library's default hasher.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
    ) -> Self {
        Self::new_with_hasher(device, queue, target_format, RandomState::new())
    }
}

impl<S: BuildHasher> Renderer<S> {
    /// Create a renderer with a custom hasher for its texture map.
    pub fn new_with_hasher(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
        hasher: S,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tinim shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tinim globals layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tinim texture layout"),
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
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tinim pipeline layout"),
            bind_group_layouts: &[Some(&globals_layout), Some(&texture_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("tinim pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Unorm16x2,
                            offset: 8,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Unorm8x4,
                            offset: 12,
                            shader_location: 2,
                        },
                    ],
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tinim globals"),
            size: 4 * size_of::<f32>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tinim globals bind group"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("tinim sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let mut textures = HashMap::with_hasher(hasher);
        // A single opaque white texel: every untextured shape samples it through `WHITE_UV`.
        let white = TextureEntry::new(
            device,
            &texture_layout,
            &sampler,
            [1, 1],
            "tinim white texel",
        );
        white.upload(
            queue,
            &tinim_paint::ImageDelta::full([1, 1], vec![255, 255, 255, 255]),
        );
        textures.insert(TextureId::WHITE, white);

        Self {
            pipeline,
            globals_buffer,
            globals_bind_group,
            texture_layout,
            sampler,
            textures,
            vertex_buffer: create_buffer(device, INITIAL_VERTEX_BYTES, wgpu::BufferUsages::VERTEX),
            index_buffer: create_buffer(device, INITIAL_INDEX_BYTES, wgpu::BufferUsages::INDEX),
        }
    }

    /// Apply this frame's texture uploads. Call before [`Renderer::render`].
    pub fn update_textures(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        delta: &TexturesDelta,
    ) {
        for (id, size) in &delta.create {
            let entry = TextureEntry::new(
                device,
                &self.texture_layout,
                &self.sampler,
                [size[0].max(1), size[1].max(1)],
                "tinim texture",
            );
            self.textures.insert(*id, entry);
        }

        for (id, image) in &delta.set {
            let needs_new_texture = image.pos.is_none() || !self.textures.contains_key(id);
            if needs_new_texture {
                let size = match image.pos {
                    Some([x, y]) => [x + image.size[0], y + image.size[1]],
                    None => image.size,
                };
                let entry = TextureEntry::new(
                    device,
                    &self.texture_layout,
                    &self.sampler,
                    size,
                    "tinim texture",
                );
                self.textures.insert(*id, entry);
            }
            if let Some(entry) = self.textures.get(id) {
                entry.upload(queue, image);
            }
        }

        for id in &delta.free {
            self.textures.remove(id);
        }
    }

    /// Upload the draw list's shared buffers and issue its draw commands.
    ///
    /// Each draw command gets its own scissor rectangle; the scissor is restored to the full target
    /// at the end, so it is safe to draw other things afterwards.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        screen: ScreenDescriptor,
        list: &PaintList,
    ) {
        let [width, height] = screen.size_in_pixels;
        if list.draws.is_empty()
            || list.vertices.is_empty()
            || list.indices.is_empty()
            || width == 0
            || height == 0
        {
            return;
        }

        let vertex_bytes = (list.vertices.len() * size_of::<Vertex>()) as u64;
        if vertex_bytes > self.vertex_buffer.size() {
            self.vertex_buffer = create_buffer(
                device,
                vertex_bytes.next_power_of_two().max(INITIAL_VERTEX_BYTES),
                wgpu::BufferUsages::VERTEX,
            );
        }
        let index_bytes = (list.indices.len() * size_of::<u32>()) as u64;
        if index_bytes > self.index_buffer.size() {
            self.index_buffer = create_buffer(
                device,
                index_bytes.next_power_of_two().max(INITIAL_INDEX_BYTES),
                wgpu::BufferUsages::INDEX,
            );
        }

        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&list.vertices));
        queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&list.indices));

        let globals = [width as f32, height as f32, 0.0, 0.0];
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::cast_slice(&globals));

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.globals_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);

        let mut bound_texture = None;
        for draw in &list.draws {
            if bound_texture != Some(draw.texture_id) {
                let Some(texture) = self.textures.get(&draw.texture_id) else {
                    // Missing textures are skipped rather than aborting the whole frame.
                    debug_assert!(false, "no texture for {:?}", draw.texture_id);
                    continue;
                };
                pass.set_bind_group(1, &texture.bind_group, &[]);
                bound_texture = Some(draw.texture_id);
            }

            let Some([x, y, w, h]) = clamp_scissor(draw.clip_rect, screen.size_in_pixels) else {
                continue;
            };
            pass.set_scissor_rect(x, y, w, h);
            pass.draw_indexed(draw.index_range.clone(), 0, 0..1);
        }

        // Leave the pass in a clean state.
        pass.set_scissor_rect(0, 0, width, height);
    }
}

fn create_buffer(device: &wgpu::Device, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("tinim vertex/index buffer"),
        size: size.max(4),
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// Convert a physical-pixel clip rectangle to an integer scissor rect inside the target.
///
/// Rounds outward (never cuts a pixel that should be visible), clamps to the target (wgpu turns an
/// out-of-bounds scissor into a validation error), and returns `None` for empty rectangles.
fn clamp_scissor(clip: Rect, target: [u32; 2]) -> Option<[u32; 4]> {
    if !clip.is_positive() {
        return None;
    }

    let max_x = target[0] as f32;
    let max_y = target[1] as f32;

    let x0 = clip.min.x.floor().clamp(0.0, max_x) as u32;
    let y0 = clip.min.y.floor().clamp(0.0, max_y) as u32;
    let x1 = clip.max.x.ceil().clamp(0.0, max_x) as u32;
    let y1 = clip.max.y.ceil().clamp(0.0, max_y) as u32;

    if x1 <= x0 || y1 <= y0 {
        None
    } else {
        Some([x0, y0, x1 - x0, y1 - y0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tinim_paint::{pos2, vec2};

    #[test]
    fn scissor_rounds_outward() {
        let clip = Rect::from_min_size(pos2(10.2, 20.8), vec2(30.1, 40.1));
        assert_eq!(clamp_scissor(clip, [100, 100]), Some([10, 20, 31, 41]));
    }

    #[test]
    fn scissor_is_clamped_to_target() {
        let clip = Rect::from_min_max(pos2(-50.0, -50.0), pos2(500.0, 500.0));
        assert_eq!(clamp_scissor(clip, [100, 50]), Some([0, 0, 100, 50]));
    }

    #[test]
    fn empty_scissor_is_skipped() {
        let inside = Rect::from_min_size(pos2(10.0, 10.0), vec2(10.0, 10.0));
        assert_eq!(clamp_scissor(inside, [5, 5]), None);
        assert_eq!(clamp_scissor(Rect::NOTHING, [100, 100]), None);
        let zero = Rect::from_min_max(pos2(10.0, 10.0), pos2(10.0, 10.0));
        assert_eq!(clamp_scissor(zero, [100, 100]), None);
    }
}
