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
//! ## Color space
//!
//! All blending happens in encoded (gamma) space, like a browser's UI compositor. Textures are
//! `Rgba8Unorm` and keep their sRGB-encoded values, and the render target must be non-sRGB too.
//! [`gamma_surface_formats`] picks a matching surface/view pair, including the fallback of creating
//! a non-sRGB view of an sRGB-only surface. Using an sRGB target would blend in linear light and
//! change the weight of anti-aliased text.
//!
//! ## Hashing
//!
//! The texture table uses `rustc-hash` by default. Disable the default `rustc-hash` feature to use
//! the standard library's randomly seeded hasher instead.

mod hash;
mod renderer;
mod texture;

pub use renderer::{Renderer, ScreenDescriptor, gamma_surface_formats};
pub use texture::TEXTURE_FORMAT;
pub use tinim_paint as paint;
pub use wgpu;
