//! Meshes, vertices and the final draw list.

use std::ops::Range;

use crate::{Color32, Pos2, Rect};

/// Which texture a [`Mesh`] samples from.
///
/// `TextureId(0)` is the font atlas, which also holds an opaque white texel at `(0, 0)` used by
/// all untextured geometry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureId(pub u64);

impl TextureId {
    /// The font atlas (and the white texel).
    pub const FONT_ATLAS: Self = Self(0);
}

/// A single vertex. Kept at 16 bytes on purpose.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    /// Position in physical pixels.
    pub pos: [f32; 2],
    /// Texture coordinates, normalized to `0..=65535`.
    pub uv: [u16; 2],
    /// sRGB color with straight alpha.
    pub color: [u8; 4],
}

impl Vertex {
    /// The uv of the opaque white texel in the atlas.
    pub const WHITE_UV: [u16; 2] = [0, 0];

    #[inline]
    pub fn new(pos: Pos2, uv: [u16; 2], color: Color32) -> Self {
        Self {
            pos: [pos.x, pos.y],
            uv,
            color: color.0,
        }
    }

    #[inline]
    pub fn pos(&self) -> Pos2 {
        Pos2::new(self.pos[0], self.pos[1])
    }
}

/// A list of triangles that all use the same texture.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub indices: Vec<u32>,
    pub vertices: Vec<Vertex>,
    pub texture_id: TextureId,
}

impl Mesh {
    #[inline]
    pub fn with_texture(texture_id: TextureId) -> Self {
        Self {
            indices: Vec::new(),
            vertices: Vec::new(),
            texture_id,
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Add an axis-aligned quad sampling the given `uv` rectangle (in `0..=1`).
    pub fn add_rect_with_uv(&mut self, rect: Rect, uv: Rect, color: Color32) {
        let base = self.vertices.len() as u32;
        let uv_min = uv.min;
        let uv_max = uv.max;

        let corners = [
            (rect.min, Pos2::new(uv_min.x, uv_min.y)),
            (
                Pos2::new(rect.max.x, rect.min.y),
                Pos2::new(uv_max.x, uv_min.y),
            ),
            (rect.max, Pos2::new(uv_max.x, uv_max.y)),
            (
                Pos2::new(rect.min.x, rect.max.y),
                Pos2::new(uv_min.x, uv_max.y),
            ),
        ];
        for (p, t) in corners {
            self.vertices.push(Vertex::new(
                p,
                [
                    (t.x.clamp(0.0, 1.0) * 65535.0).round() as u16,
                    (t.y.clamp(0.0, 1.0) * 65535.0).round() as u16,
                ],
                color,
            ));
        }
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    pub fn append(&mut self, other: Self) {
        if other.texture_id != self.texture_id {
            // Merging meshes with different textures would silently draw one of them wrong.
            debug_assert_eq!(other.texture_id, self.texture_id);
        }
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&other.vertices);
        self.indices.extend(other.indices.iter().map(|&i| i + base));
    }
}

/// One draw call: a contiguous range of indices sharing a clip rectangle and texture.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawCommand {
    /// Clip rectangle in physical pixels, matching [`PaintList::vertices`].
    pub clip_rect: Rect,
    pub texture_id: TextureId,
    /// Range into [`PaintList::indices`]. Indices are absolute.
    pub index_range: Range<u32>,
}

/// A flat vertex/index arena plus the draw commands that reference it.
///
/// Vertices and clip rectangles are already in physical pixels. Indices are absolute indices into
/// `vertices`, so the renderer can upload both buffers once and issue one `draw_indexed` per
/// [`DrawCommand`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PaintList {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub draws: Vec<DrawCommand>,
}

impl PaintList {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Remove all geometry and draw commands, keeping the allocated capacity.
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.draws.clear();
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// Append raw geometry, offsetting indices by the current vertex count.
    pub(crate) fn append_geometry(&mut self, vertices: &[Vertex], indices: &[u32]) {
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(vertices);
        self.indices
            .extend(indices.iter().map(|&index| index + base));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{pos2, vec2};

    #[test]
    fn vertex_is_16_bytes() {
        assert_eq!(std::mem::size_of::<Vertex>(), 16);
        assert_eq!(std::mem::align_of::<Vertex>(), 4);
    }

    #[test]
    fn add_rect_with_uv_covers_the_quad() {
        let mut mesh = Mesh::with_texture(TextureId::FONT_ATLAS);
        mesh.add_rect_with_uv(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0)),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.indices.len(), 6);
        assert_eq!(mesh.vertices[2].uv, [65535, 65535]);
        assert!(mesh.indices.iter().all(|&i| i < 4));
    }
}
