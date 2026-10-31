//! wgpu renderer for tinim's paint data.
//!
//! ```no_run
//! # use tinim_wgpu::{paint, wgpu, Renderer, ScreenDescriptor};
//! # fn demo(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) {
//! let mut renderer = Renderer::new(device, queue, format);
//! // ... and every frame, inside a render pass:
//! // renderer.render(device, queue, &mut pass, ScreenDescriptor::new(800, 600), &primitives);
//! # let _ = &mut renderer;
//! # }
//! ```
//!
//! The renderer is a dumb consumer of [`tinim_paint::ClippedPrimitive`]: no layout, no widgets, no
//! text shaping. It only knows vertices, indices, a scissor rectangle and a texture id.

mod renderer;
mod texture;

pub use renderer::{ATLAS_FORMAT, Renderer, ScreenDescriptor};
pub use tinim_paint as paint;
pub use wgpu;
