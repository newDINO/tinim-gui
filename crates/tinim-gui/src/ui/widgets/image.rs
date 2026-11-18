//! Image widgets.

use tinim_paint::{Color32, Pos2, Rect, Vec2};

use super::super::Ui;
use super::super::layout::allocate_space;
use super::super::response::Response;
use super::{interaction, is_visible};
use crate::texture::TextureHandle;

/// How an image is mapped into its allocated rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageFit {
    /// Draw at the source region's natural size, centered in the allocated rectangle.
    ///
    /// If the allocated rectangle is smaller than the source, the source is scaled down to fit
    /// while preserving its aspect ratio. It is never upscaled.
    #[default]
    Original,
    /// Stretch the source region to fill the allocated rectangle, ignoring the aspect ratio.
    Fill,
    /// Scale the source region uniformly so it fits inside the rectangle and center it.
    Contain,
    /// Scale the source region uniformly so it covers the rectangle and crop the extra source
    /// pixels inside the configured UV rectangle.
    Cover,
}

/// Options for [`Ui::image_with`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageOptions {
    /// Allocated size in logical points. `None` uses the natural size of the configured UV region.
    ///
    /// [`ImageFit::Original`] ignores this as a stretch target and centers the natural-size source
    /// inside it instead.
    pub size: Option<Vec2>,
    pub fit: ImageFit,
    /// Normalized source rectangle. The default is the whole texture.
    ///
    /// Layout and fitting operate on this region rather than on the full texture: a UV rectangle
    /// covering half the texture has half the natural size, and its aspect ratio is used by
    /// [`ImageFit::Contain`] and [`ImageFit::Cover`].
    pub uv: Rect,
    /// Multiplied with the sampled texels. Alpha scales the image's opacity.
    pub tint: Color32,
}

impl Default for ImageOptions {
    fn default() -> Self {
        Self {
            size: None,
            fit: ImageFit::Original,
            uv: Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            tint: Color32::WHITE,
        }
    }
}

impl ImageOptions {
    #[inline]
    pub fn with_size(mut self, size: Vec2) -> Self {
        self.size = Some(size);
        self
    }

    #[inline]
    pub fn with_fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }

    #[inline]
    pub fn with_uv(mut self, uv: Rect) -> Self {
        self.uv = uv;
        self
    }

    #[inline]
    pub fn with_tint(mut self, tint: Color32) -> Self {
        self.tint = tint;
        self
    }
}

impl Ui<'_> {
    /// Draws a texture at its natural size.
    ///
    /// The natural size is the texture's pixel size interpreted as logical points, like an HTML
    /// `<img>` without explicit dimensions. Use [`Ui::image_with`] to choose a size, fit mode,
    /// source rectangle or tint.
    pub fn image(&mut self, texture: &TextureHandle) -> Response {
        self.image_with(texture, ImageOptions::default())
    }

    /// Draws a texture with explicit options.
    pub fn image_with(&mut self, texture: &TextureHandle, options: ImageOptions) -> Response {
        let source = source_size(texture.size(), options.uv);
        let allocated = options.size.unwrap_or(source);
        let spacing = self.spacing();
        let rect = allocate_space(&mut self.layout, allocated, spacing);
        let hit = interaction(self.clip, &self.ctx.input, rect);

        if is_visible(&self.ctx.painter, rect)
            && let Some((draw_rect, uv)) = image_geometry(rect, source, options)
        {
            self.ctx
                .painter
                .image_uv(draw_rect, texture.id(), uv, options.tint);
        }

        Response {
            rect,
            hovered: hit.hovered,
            pressed: hit.pressed,
            down: hit.down,
        }
    }
}

/// The natural size, in logical points, of the selected UV region.
fn source_size(texture_size: Vec2, uv: Rect) -> Vec2 {
    Vec2::new(texture_size.x * uv.width(), texture_size.y * uv.height())
}

/// Resolve the draw rectangle and source UV for an image inside `rect`.
///
/// `source` is the natural size of the configured UV region, not necessarily the full texture.
fn image_geometry(rect: Rect, source: Vec2, options: ImageOptions) -> Option<(Rect, Rect)> {
    if !rect.is_positive() || !options.uv.is_positive() || source.x <= 0.0 || source.y <= 0.0 {
        return None;
    }

    match options.fit {
        ImageFit::Original => {
            let scale = (rect.width() / source.x)
                .min(rect.height() / source.y)
                .min(1.0);
            Some((
                Rect::from_center_size(rect.center(), source * scale),
                options.uv,
            ))
        }
        ImageFit::Fill => Some((rect, options.uv)),
        ImageFit::Contain => {
            let scale = (rect.width() / source.x).min(rect.height() / source.y);
            Some((
                Rect::from_center_size(rect.center(), source * scale),
                options.uv,
            ))
        }
        ImageFit::Cover => {
            let source_aspect = source.x / source.y;
            let target_aspect = rect.width() / rect.height();
            let mut uv = options.uv;

            if source_aspect > target_aspect {
                // The source is wider than the target: crop the left and right edges.
                let fraction = target_aspect / source_aspect;
                let span = uv.width() * fraction;
                let center = uv.center().x;
                uv.min.x = center - span * 0.5;
                uv.max.x = center + span * 0.5;
            } else if source_aspect < target_aspect {
                // The source is taller than the target: crop the top and bottom edges.
                let fraction = source_aspect / target_aspect;
                let span = uv.height() * fraction;
                let center = uv.center().y;
                uv.min.y = center - span * 0.5;
                uv.max.y = center + span * 0.5;
            }

            Some((rect, uv))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_util::context;
    use crate::{RawInput, TextureId, vec2};

    fn handle(size: [u32; 2]) -> TextureHandle {
        TextureHandle::new(TextureId(7), size)
    }

    #[test]
    fn image_uses_its_pixel_size_as_the_natural_logical_size() {
        let mut context = context();
        let mut allocated = Rect::NOTHING;

        let frame = context.run(vec2(200.0, 100.0), 2.0, RawInput::default(), |ui| {
            allocated = ui.image(&handle([20, 10])).rect;
        });

        assert_eq!(allocated.size(), vec2(20.0, 10.0));
        assert_eq!(frame.paint.draws.len(), 1);
        assert_eq!(frame.paint.draws[0].texture_id, TextureId(7));
        assert_eq!(frame.paint.vertices[0].pos, [0.0, 0.0]);
        assert_eq!(frame.paint.vertices[2].pos, [40.0, 20.0]);
    }

    #[test]
    fn image_advances_the_flow_by_its_allocated_size() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut first = Rect::NOTHING;
        let mut second = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            first = ui.image(&handle([20, 10])).rect;
            second = ui.image(&handle([20, 10])).rect;
        });

        assert_eq!(second.min.y, first.max.y + spacing);
        assert_eq!(second.min.x, first.min.x);
    }

    #[test]
    fn uv_region_defines_the_natural_size() {
        let mut context = context();
        let mut response = None;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            response = Some(
                ui.image_with(
                    &handle([100, 100]),
                    ImageOptions::default()
                        .with_uv(Rect::from_min_max(Pos2::ZERO, Pos2::new(0.5, 0.5))),
                ),
            );
        });

        // The selected quarter is 50x50, so it allocates and draws at 50x50, not 100x100.
        assert_eq!(response.unwrap().rect.size(), vec2(50.0, 50.0));
    }

    #[test]
    fn original_preserves_the_aspect_ratio_when_it_must_shrink() {
        let mut context = context();
        let frame = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.image_with(
                &handle([20, 10]),
                ImageOptions::default()
                    .with_size(vec2(10.0, 40.0))
                    .with_fit(ImageFit::Original),
            );
        });

        // 20x10 shrinks by 0.5 to 10x5; it must not become 10x10.
        assert_eq!(frame.paint.vertices[0].pos, [0.0, 17.5]);
        assert_eq!(frame.paint.vertices[2].pos, [10.0, 22.5]);
    }

    #[test]
    fn contain_uses_the_uv_regions_aspect_ratio() {
        let mut context = context();
        let frame = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.image_with(
                &handle([100, 100]),
                ImageOptions::default()
                    .with_size(vec2(40.0, 40.0))
                    .with_fit(ImageFit::Contain)
                    .with_uv(Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 0.5))),
            );
        });

        // The UV region is 100x50 (2:1), so containing it in 40x40 gives 40x20 at y=10..30.
        assert_eq!(frame.paint.vertices[0].pos, [0.0, 10.0]);
        assert_eq!(frame.paint.vertices[2].pos, [40.0, 30.0]);
    }

    #[test]
    fn cover_crops_inside_the_configured_uv_region() {
        let mut context = context();
        let frame = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.image_with(
                &handle([100, 100]),
                ImageOptions::default()
                    .with_size(vec2(10.0, 10.0))
                    .with_fit(ImageFit::Cover)
                    .with_uv(Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 0.5))),
            );
        });

        // The UV region is 2:1 and the target is 1:1, so crop x to 0.25..=0.75 inside the region.
        // y stays 0..=0.5 because the configured UV does not cover the whole texture.
        assert_eq!(frame.paint.vertices[0].uv, [16384, 0]);
        assert_eq!(frame.paint.vertices[1].uv, [49151, 0]);
        assert_eq!(frame.paint.vertices[3].uv, [16384, 32768]);
    }

    #[test]
    fn contain_preserves_the_aspect_ratio_and_centers_the_image() {
        let mut context = context();
        let mut response = None;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            response = Some(
                ui.image_with(
                    &handle([20, 10]),
                    ImageOptions::default()
                        .with_size(vec2(40.0, 40.0))
                        .with_fit(ImageFit::Contain),
                ),
            );
        });

        let response = response.unwrap();
        assert_eq!(response.rect.size(), vec2(40.0, 40.0));
    }

    #[test]
    fn contain_draws_inside_the_allocated_rectangle() {
        let mut context = context();
        let frame = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.image_with(
                &handle([20, 10]),
                ImageOptions::default()
                    .with_size(vec2(40.0, 40.0))
                    .with_fit(ImageFit::Contain),
            );
        });

        // 20x10 contained in 40x40 becomes 40x20, centered vertically at y=10.
        assert_eq!(frame.paint.vertices[0].pos, [0.0, 10.0]);
        assert_eq!(frame.paint.vertices[2].pos, [40.0, 30.0]);
    }

    #[test]
    fn cover_crops_the_source_uv_instead_of_changing_the_clip() {
        let mut context = context();
        let frame = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.image_with(
                &handle([20, 10]),
                ImageOptions::default()
                    .with_size(vec2(10.0, 10.0))
                    .with_fit(ImageFit::Cover),
            );
        });

        // 20x10 into 10x10 keeps the target rect and crops x to 0.25..=0.75.
        assert_eq!(frame.paint.vertices[0].pos, [0.0, 0.0]);
        assert_eq!(frame.paint.vertices[2].pos, [10.0, 10.0]);
        assert_eq!(frame.paint.vertices[0].uv, [16384, 0]);
        assert_eq!(frame.paint.vertices[1].uv, [49151, 0]);
        assert_eq!(frame.paint.vertices[3].uv, [16384, 65535]);
    }

    #[test]
    fn image_tint_is_applied_to_the_vertices() {
        let mut context = context();
        let tint = Color32::from_rgba_unmultiplied(255, 0, 0, 128);
        let frame = context.run(vec2(200.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.image_with(&handle([10, 10]), ImageOptions::default().with_tint(tint));
        });

        assert!(
            frame
                .paint
                .vertices
                .iter()
                .all(|vertex| vertex.color == tint.to_array())
        );
    }
}
