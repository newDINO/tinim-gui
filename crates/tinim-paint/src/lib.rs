//! Backend-agnostic paint data for [`tinim`](https://github.com/) — a tiny immediate mode GUI.
//!
//! ```
//! use tinim_paint::{pos2, vec2, ClippedShape, Color32, Rect, Shape, Tessellator};
//!
//! let shape = ClippedShape::new(
//!     Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0)),
//!     Shape::rect(
//!         Rect::from_min_size(pos2(10.0, 10.0), vec2(80.0, 30.0)),
//!         Color32::from_rgb(60, 140, 255),
//!     ),
//! );
//!
//! let primitives = Tessellator::new().tessellate(&[shape], 1.0);
//! assert_eq!(primitives.len(), 1);
//! ```
//!
//! Steps:
//! 1. Build [`ClippedShape`]s (a [`Shape`] plus the rectangle it is clipped to).
//! 2. Tessellate them into [`ClippedPrimitive`]s (one mesh per shape).
//! 3. Hand those to a renderer (`tinim-wgpu`) together with [`TexturesDelta`].
//!
//! Note: geometry is authored in logical points. [`Tessellator::tessellate`] converts to physical
//! pixels using `pixels_per_point`; meshes you build yourself ([`Shape::Mesh`]) must already be in
//! physical pixels.

mod color;
mod math;
mod mesh;
mod shape;
mod tessellate;
mod textures;

pub use color::Color32;
pub use math::{Pos2, Rangef, Rect, Vec2, pos2, vec2};
pub use mesh::{ClippedPrimitive, Mesh, TextureId, Vertex};
pub use shape::{CircleShape, ClippedShape, LineShape, RectShape, Shape, Stroke};
pub use tessellate::Tessellator;
pub use textures::{ImageDelta, TexturesDelta};
