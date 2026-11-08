//! A canvas like library that build shapes into triangle meshes.
//!
//! ```
//! use tinim_paint::{pos2, vec2, Color32, Painter, PaintList, Rect};
//!
//! let mut painter = Painter::new(100.0, 100.0, 1.0);
//! painter.rect(
//!     Rect::from_min_size(pos2(10.0, 10.0), vec2(80.0, 30.0)),
//!     Color32::from_rgb(60, 140, 255),
//! );
//!
//! let list: &PaintList = painter.finish();
//! assert_eq!(list.draws.len(), 1);
//! ```
//!
//! Steps:
//! 1. Create a [`Painter`] with the canvas size and `pixels_per_point`. It tracks the current clip
//!    rectangle, clamps it to the canvas and tessellates geometry immediately.
//! 2. Add shapes with [`Painter::rect`], [`Painter::circle`], [`Painter::line`] or
//!    [`Painter::add`]. Geometry is appended to one shared vertex/index arena.
//! 3. Call [`Painter::finish`] to flush the last run and get a [`PaintList`]. Each
//!    [`DrawCommand`] covers a contiguous index range with one clip rectangle and texture.
//! 4. Hand the list to a renderer (`tinim-wgpu`) together with [`TexturesDelta`].
//!
//! Note: geometry is authored in logical points and converted to physical pixels when it is added.
//! A [`Mesh`] declares its coordinate space with [`MeshSpace`]; meshes built with
//! [`Mesh::with_texture`] default to logical points and are scaled on insertion, while
//! [`Mesh::physical`] meshes are already in physical pixels and are appended unchanged.
//!
//! ## Backend contract
//!
//! A renderer consuming a [`PaintList`] and [`TexturesDelta`] must uphold these rules:
//!
//! * [`TextureId::WHITE`] always exists and contains a 1×1 opaque white texel at `(0, 0)`.
//!   Untextured geometry samples [`Vertex::WHITE_UV`], which is `(0, 0)`.
//! * Textures listed in `TexturesDelta::create` are allocated and zero-initialized. Creating an id
//!   that already exists replaces the texture and discards every previous texel. Apply `create`
//!   before `set`, and apply `set` entries in order.
//! * An [`ImageDelta`] with `pos = Some(..)` is a partial update and is only valid after the
//!   texture has been created, either earlier or by the same delta.
//! * Textures not mentioned by the delta keep their contents; ids in `TexturesDelta::free` are
//!   released.
//!
//! An application may point [`Painter::set_solid_texture`] at a different texture that reserves
//! an opaque white texel at `(0, 0)`, such as a glyph atlas. It then owns creating and uploading
//! that texture before the first draw. `tinim-text` exposes
//! `TextSystem::ensure_atlas_texture` for this and re-uploads the white texel whenever the atlas
//! texture is created or replaced.

mod color;
mod math;
mod mesh;
mod painter;
mod shape;
mod tessellate;
mod textures;

pub use color::Color32;
pub use math::{Pos2, Rangef, Rect, Vec2, pos2, vec2};
pub use mesh::{DrawCommand, Mesh, MeshMismatch, MeshSpace, PaintList, TextureId, Vertex};
pub use painter::Painter;
pub use shape::{CircleShape, LineShape, RectShape, Shape, Stroke};
pub use textures::{ImageDelta, TexturesDelta};
