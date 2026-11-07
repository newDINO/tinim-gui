//! Meshes, vertices and the final draw list.

use std::ops::Range;

use crate::{Color32, Pos2, Rect};

/// Which texture a [`Mesh`] samples from.
///
/// [`TextureId::WHITE`] is reserved for the renderer's 1×1 opaque white texture, which all
/// untextured geometry samples. User textures must use a different id.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureId(pub u32);

impl TextureId {
    /// The renderer's 1×1 opaque white texture, used by untextured geometry.
    ///
    /// This id is reserved. Since [`TextureId`] derives [`Default`], it is also what a
    /// default-constructed mesh samples.
    pub const WHITE: Self = Self(0);
}

/// The coordinate space of a [`Mesh`]'s vertex positions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MeshSpace {
    /// Positions are logical points. [`Painter::add_mesh`](crate::Painter::add_mesh) scales them
    /// by `pixels_per_point`.
    #[default]
    Logical,
    /// Positions are already physical pixels. [`Painter::add_mesh`](crate::Painter::add_mesh)
    /// appends them unchanged.
    Physical,
}

/// A single vertex. Kept at 16 bytes on purpose.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    /// Position in the owning mesh's [`MeshSpace`]. [`PaintList`] vertices are always physical
    /// pixels.
    pub pos: [f32; 2],
    /// Texture coordinates, normalized to `0..=65535`.
    pub uv: [u16; 2],
    /// sRGB color with straight alpha.
    pub color: [u8; 4],
}

impl Vertex {
    /// The uv of the opaque white texel in [`TextureId::WHITE`].
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

/// A list of triangles that all use the same texture and coordinate space.
///
/// Positions are in [`Mesh::space`]. [`Painter::add_mesh`](crate::Painter::add_mesh) scales
/// logical-point meshes by `pixels_per_point` and appends physical-pixel meshes unchanged.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub indices: Vec<u32>,
    pub vertices: Vec<Vertex>,
    pub texture_id: TextureId,
    pub space: MeshSpace,
}

impl Mesh {
    /// An empty mesh in logical points, the common authoring space.
    #[inline]
    pub fn with_texture(texture_id: TextureId) -> Self {
        Self {
            indices: Vec::new(),
            vertices: Vec::new(),
            texture_id,
            space: MeshSpace::Logical,
        }
    }

    /// An empty mesh whose positions are already in physical pixels.
    #[inline]
    pub fn physical(texture_id: TextureId) -> Self {
        Self {
            space: MeshSpace::Physical,
            ..Self::with_texture(texture_id)
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

    /// Append `other`, offsetting its indices by the current vertex count.
    ///
    /// # Errors
    ///
    /// Returns [`MeshMismatch`] if `other` uses a different texture or coordinate space. In that
    /// case `self` is left unchanged.
    pub fn append(&mut self, other: Self) -> Result<(), MeshMismatch> {
        if other.texture_id != self.texture_id {
            return Err(MeshMismatch::Texture {
                expected: self.texture_id,
                actual: other.texture_id,
            });
        }
        if other.space != self.space {
            return Err(MeshMismatch::Space {
                expected: self.space,
                actual: other.space,
            });
        }
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&other.vertices);
        self.indices.extend(other.indices.iter().map(|&i| i + base));
        Ok(())
    }
}

/// Error returned when two meshes with incompatible textures or coordinate spaces are appended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshMismatch {
    /// The meshes sample different textures.
    Texture {
        /// The texture of the mesh being appended to.
        expected: TextureId,
        /// The texture of the mesh that was appended.
        actual: TextureId,
    },
    /// The meshes use different coordinate spaces.
    Space {
        /// The space of the mesh being appended to.
        expected: MeshSpace,
        /// The space of the mesh that was appended.
        actual: MeshSpace,
    },
}

impl std::fmt::Display for MeshMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Texture { expected, actual } => write!(
                f,
                "cannot append mesh with texture {actual:?} to mesh with texture {expected:?}"
            ),
            Self::Space { expected, actual } => {
                write!(f, "cannot append a {actual:?} mesh to a {expected:?} mesh")
            }
        }
    }
}

impl std::error::Error for MeshMismatch {}

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
        let mut mesh = Mesh::with_texture(TextureId::WHITE);
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

    #[test]
    fn meshes_default_to_logical_space() {
        assert_eq!(Mesh::with_texture(TextureId(1)).space, MeshSpace::Logical);
        assert_eq!(Mesh::default().space, MeshSpace::Logical);
    }

    #[test]
    fn physical_constructor_sets_the_space() {
        let mesh = Mesh::physical(TextureId(1));
        assert_eq!(mesh.space, MeshSpace::Physical);
        assert_eq!(mesh.texture_id, TextureId(1));
    }

    fn quad_mesh(texture_id: TextureId) -> Mesh {
        let mut mesh = Mesh::with_texture(texture_id);
        mesh.add_rect_with_uv(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0)),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        mesh
    }

    #[test]
    fn append_offsets_indices_when_textures_match() {
        let mut a = quad_mesh(TextureId::WHITE);
        a.append(quad_mesh(TextureId::WHITE)).unwrap();

        assert_eq!(a.vertices.len(), 8);
        assert_eq!(a.indices.len(), 12);
        assert_eq!(&a.indices[6..], &[4, 5, 6, 4, 6, 7]);
    }

    #[test]
    fn append_rejects_a_different_texture() {
        let mut a = quad_mesh(TextureId::WHITE);
        let before = a.clone();

        let error = a.append(quad_mesh(TextureId(1))).unwrap_err();
        assert_eq!(
            error,
            MeshMismatch::Texture {
                expected: TextureId::WHITE,
                actual: TextureId(1),
            }
        );
        assert_eq!(a, before);
    }

    #[test]
    fn append_rejects_a_different_space() {
        let mut a = quad_mesh(TextureId::WHITE);
        let before = a.clone();

        let mut physical = quad_mesh(TextureId::WHITE);
        physical.space = MeshSpace::Physical;

        let error = a.append(physical).unwrap_err();
        assert_eq!(
            error,
            MeshMismatch::Space {
                expected: MeshSpace::Logical,
                actual: MeshSpace::Physical,
            }
        );
        assert_eq!(a, before);
    }
}
