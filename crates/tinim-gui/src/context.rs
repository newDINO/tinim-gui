//! The per-application GUI context and the output of one frame.

use tinim_paint::{
    ImageDelta, PaintList, Painter, TextureDescriptor, TextureId, TextureOptions, TexturesDelta,
    Vec2,
};
use tinim_paint_text::TextSystem;

use crate::input::RawInput;
use crate::platform::PlatformOutput;
use crate::style::Style;
use crate::text_cache::TextCache;
use crate::texture::{ImageData, TextureHandle};
use crate::ui::Ui;

/// Owns the text system, painter and text cache.
///
/// Call [`Context::run`] once per frame. The returned [`FullOutput`] borrows the paint list, so it
/// must be rendered before the next frame starts.
pub struct Context {
    pub(crate) text_system: TextSystem,
    pub(crate) painter: Painter,
    pub(crate) text_cache: TextCache,
    style: Style,
    pub(crate) input: RawInput,
    size: Vec2,
    pub(crate) pixels_per_point: f32,
    pub(crate) platform: PlatformOutput,
    /// Textures allocated by [`Context::load_texture`] that have not been emitted yet.
    pending_textures: TexturesDelta,
    next_texture_id: u32,
}

/// Everything produced by one frame.
pub struct FullOutput<'a> {
    /// Tessellated geometry in physical pixels.
    pub paint: &'a PaintList,
    /// Glyph atlas and image texture uploads produced by the frame.
    pub textures: TexturesDelta,
    /// Non-rendering requests for the platform or application.
    ///
    /// This includes the mouse cursor requested by hovered widgets. Rendering uses [`paint`] and
    /// [`textures`]; [`platform`] is applied through a backend integration such as
    /// `tinim-winit`.
    ///
    /// [`paint`]: FullOutput::paint
    /// [`textures`]: FullOutput::textures
    /// [`platform`]: FullOutput::platform
    pub platform: PlatformOutput,
}

impl Context {
    /// Creates a context whose glyph atlas uses `texture_id`.
    ///
    /// Solid shapes sample the atlas's reserved white texel, so they batch with text instead of
    /// alternating between the atlas and the renderer's white texture. `TextureId::WHITE` is
    /// reserved by the renderer, so use another id.
    pub fn new(texture_id: TextureId) -> Self {
        Self {
            text_system: TextSystem::new(texture_id),
            painter: Painter::from_size(Vec2::ZERO, 1.0),
            text_cache: TextCache::default(),
            style: Style::default(),
            input: RawInput::default(),
            size: Vec2::ZERO,
            pixels_per_point: 1.0,
            platform: PlatformOutput::default(),
            pending_textures: TexturesDelta::default(),
            // Start above the atlas id so handles created by this context do not collide with it
            // or with the renderer's reserved white texture. The allocator skips both anyway.
            next_texture_id: texture_id.0.wrapping_add(1).max(1),
        }
    }

    /// Access the font stack. Add fonts before the first frame.
    ///
    /// Changing the font stack invalidates shaped layouts. Call
    /// [`Context::clear_text_cache`] if you add fonts after frames have already been built.
    pub fn text_system(&self) -> &TextSystem {
        &self.text_system
    }

    pub fn text_system_mut(&mut self) -> &mut TextSystem {
        &mut self.text_system
    }

    /// Uploads an image and returns a handle that can be drawn with
    /// [`Ui::image`](crate::Ui::image).
    ///
    /// The texture is allocated in the next frame's [`FullOutput::textures`], so uploading can
    /// happen before the first frame or between frames. The default sampler uses linear
    /// filtering and clamps to the texture edge; use [`Context::load_texture_with`] for nearest
    /// filtering or repeat addressing.
    ///
    /// Call [`Context::free_texture`] when the image is no longer used.
    pub fn load_texture(&mut self, image: ImageData) -> TextureHandle {
        self.load_texture_with(image, TextureOptions::default())
    }

    /// Uploads an image with explicit sampler options.
    pub fn load_texture_with(
        &mut self,
        image: ImageData,
        options: TextureOptions,
    ) -> TextureHandle {
        let id = self.allocate_texture_id();
        let size = image.size;
        self.pending_textures
            .create
            .push((id, TextureDescriptor::new(size).with_options(options)));
        self.pending_textures
            .set
            .push((id, ImageDelta::full(size, image.rgba)));
        TextureHandle::new(id, size)
    }

    /// Releases a texture at the start of the next frame.
    ///
    /// Draw commands that still reference the handle during the frame in which it is freed are a
    /// contract violation; renderers may treat the texture as missing.
    pub fn free_texture(&mut self, handle: TextureHandle) {
        debug_assert_ne!(handle.id(), TextureId::WHITE);
        debug_assert_ne!(handle.id(), self.text_system.atlas_texture_id());
        self.pending_textures.free.push(handle.id());
    }

    fn allocate_texture_id(&mut self) -> TextureId {
        let atlas_id = self.text_system.atlas_texture_id();
        loop {
            let id = TextureId(self.next_texture_id);
            self.next_texture_id = self.next_texture_id.wrapping_add(1);
            if id != TextureId::WHITE && id != atlas_id {
                return id;
            }
        }
    }

    pub fn style(&self) -> &Style {
        &self.style
    }

    pub fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }

    /// Drops every cached shaped text layout.
    pub fn clear_text_cache(&mut self) {
        self.text_cache.clear();
    }

    /// Logical viewport size used by the current/last frame.
    pub fn size(&self) -> Vec2 {
        self.size
    }

    pub fn pixels_per_point(&self) -> f32 {
        self.pixels_per_point
    }

    pub fn input(&self) -> &RawInput {
        &self.input
    }

    /// Builds one frame.
    ///
    /// The closure runs immediately. `paint` is valid until the next call to `run`.
    pub fn run(
        &mut self,
        viewport: Vec2,
        pixels_per_point: f32,
        input: RawInput,
        build: impl FnOnce(&mut Ui<'_>),
    ) -> FullOutput<'_> {
        self.begin_frame(viewport, pixels_per_point, input);

        {
            let mut ui = Ui::new(self);
            build(&mut ui);
        }

        let paint = self.painter.finish();
        let mut textures = self.text_system.take_textures();
        textures.append(std::mem::take(&mut self.pending_textures));
        let platform = std::mem::take(&mut self.platform);
        FullOutput {
            paint,
            textures,
            platform,
        }
    }

    fn begin_frame(&mut self, viewport: Vec2, pixels_per_point: f32, input: RawInput) {
        self.size = sanitize_size(viewport);
        self.pixels_per_point = sanitize_scale(pixels_per_point);
        self.input = input;
        self.platform = PlatformOutput::default();

        if (self.painter.pixels_per_point() - self.pixels_per_point).abs() > f32::EPSILON {
            self.painter = Painter::from_size(self.size, self.pixels_per_point);
        } else {
            self.painter.set_size(self.size.x, self.size.y);
        }

        // Solid geometry samples the atlas's reserved white texel so it batches with text. The
        // painter is recreated when the DPI scale changes, so reapply the setting every frame.
        self.text_system.ensure_atlas_texture();
        let solid_texture = self.text_system.atlas_texture_id();
        self.painter.set_solid_texture(solid_texture);

        self.painter.clear();
        self.painter.reset_clip_rect();
    }
}

fn sanitize_size(size: Vec2) -> Vec2 {
    if size.is_finite() {
        Vec2::new(size.x.max(0.0), size.y.max(0.0))
    } else {
        Vec2::ZERO
    }
}

fn sanitize_scale(scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tinim_paint::{TextureAddressMode, TextureFilter};

    #[test]
    fn first_frame_allocates_the_atlas_even_without_text() {
        let mut context = Context::new(TextureId(1));
        let frame = context.run(Vec2::new(100.0, 100.0), 1.0, RawInput::default(), |_| {});

        assert!(frame.paint.draws.is_empty());
        assert_eq!(
            frame.textures.create,
            vec![(TextureId(1), TextureDescriptor::new([512, 512]))]
        );
        assert_eq!(frame.textures.set.len(), 1);
        assert_eq!(frame.textures.set[0].1.pos, Some([0, 0]));
        assert_eq!(frame.textures.set[0].1.size, [1, 1]);
        assert_eq!(frame.textures.set[0].1.rgba, vec![255, 255, 255, 255]);

        drop(frame);
        let frame = context.run(Vec2::new(100.0, 100.0), 1.0, RawInput::default(), |_| {});
        assert!(frame.textures.is_empty(), "the texture already exists");
    }

    #[test]
    fn solid_texture_follows_the_atlas_after_a_dpi_change() {
        let mut context = Context::new(TextureId(1));
        context
            .text_system_mut()
            .add_font(font_test_data::AHEM.to_vec())
            .unwrap();
        let frame = context.run(Vec2::new(100.0, 100.0), 2.0, RawInput::default(), |ui| {
            ui.button("go");
        });
        assert_eq!(frame.paint.draws.len(), 1, "rect and text share the atlas");
        assert_eq!(frame.paint.draws[0].texture_id, TextureId(1));
    }

    #[test]
    fn load_texture_allocates_an_id_and_uploads_through_the_frame() {
        let mut context = Context::new(TextureId(1));
        let handle = context.load_texture(ImageData::new([2, 2], vec![9; 16]));
        assert_ne!(handle.id(), TextureId::WHITE);
        assert_ne!(handle.id(), TextureId(1));
        assert_eq!(handle.pixel_size(), [2, 2]);
        assert_eq!(handle.size(), Vec2::new(2.0, 2.0));

        let frame = context.run(Vec2::new(100.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.image(&handle);
        });

        assert_eq!(
            frame.textures.create,
            vec![
                (TextureId(1), TextureDescriptor::new([512, 512])),
                (handle.id(), TextureDescriptor::new([2, 2])),
            ]
        );
        assert_eq!(frame.paint.draws.len(), 1);
        assert_eq!(frame.paint.draws[0].texture_id, handle.id());
        assert!(
            frame
                .textures
                .set
                .iter()
                .any(|(id, delta)| *id == handle.id() && delta.pos.is_none())
        );
    }

    #[test]
    fn load_texture_with_carries_sampler_options() {
        let mut context = Context::new(TextureId(1));
        let options = TextureOptions {
            mag_filter: TextureFilter::Nearest,
            min_filter: TextureFilter::Nearest,
            address_mode_u: TextureAddressMode::Repeat,
            address_mode_v: TextureAddressMode::ClampToEdge,
        };
        let handle = context.load_texture_with(ImageData::new([1, 1], vec![1, 2, 3, 4]), options);

        let frame = context.run(Vec2::new(100.0, 100.0), 1.0, RawInput::default(), |_| {});

        let (_, descriptor) = frame
            .textures
            .create
            .iter()
            .find(|(id, _)| *id == handle.id())
            .unwrap();
        assert_eq!(descriptor.options, options);
    }

    #[test]
    fn free_texture_is_emitted_with_the_next_frame() {
        let mut context = Context::new(TextureId(1));
        let handle = context.load_texture(ImageData::new([1, 1], vec![1, 2, 3, 4]));

        let frame = context.run(Vec2::new(100.0, 100.0), 1.0, RawInput::default(), |_| {});
        assert!(frame.textures.free.is_empty());
        drop(frame);

        context.free_texture(handle);
        let frame = context.run(Vec2::new(100.0, 100.0), 1.0, RawInput::default(), |_| {});
        assert_eq!(frame.textures.free, vec![handle.id()]);
    }

    #[test]
    fn texture_ids_skip_the_atlas_and_white() {
        let mut context = Context::new(TextureId(2));
        let a = context.load_texture(ImageData::new([1, 1], vec![0; 4]));
        let b = context.load_texture(ImageData::new([1, 1], vec![0; 4]));

        assert_ne!(a.id(), TextureId::WHITE);
        assert_ne!(a.id(), TextureId(2));
        assert_ne!(b.id(), TextureId::WHITE);
        assert_ne!(b.id(), TextureId(2));
        assert_ne!(a.id(), b.id());
    }
}
