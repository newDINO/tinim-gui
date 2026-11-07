//! wgpu renderer for tinim's paint data.
//!
//! ```no_run
//! # use tinim_wgpu::{paint, wgpu, Renderer, ScreenDescriptor};
//! # fn demo(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) {
//! let mut renderer = Renderer::new(device, queue, format);
//! // ... and every frame, inside a render pass:
//! // renderer.render(device, queue, &mut pass, ScreenDescriptor::new(800, 600), &list);
//! # let _ = &mut renderer;
//! # }
//! ```
//!
//! The renderer is a dumb consumer of [`tinim_paint::PaintList`]: no layout, no widgets, no text
//! shaping. It only knows vertices, indices, draw ranges, scissor rectangles and texture ids.
//!
//! ## Hashing
//!
//! The texture table uses `rustc-hash` by default. Disable the default `rustc-hash` feature to use
//! the standard library's randomly seeded hasher instead.

mod hash;
mod renderer;
mod texture;

pub use renderer::{Renderer, ScreenDescriptor};
pub use texture::TEXTURE_FORMAT;
pub use tinim_paint as paint;
pub use wgpu;
