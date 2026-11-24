//! Turning glyphs into RGBA8 bitmaps: outlines, COLRv0 and embedded bitmaps.

use skrifa::{
    Font, GlyphId,
    font::glyph::{
        GlyphSourceOrder,
        bitmap::{BitmapGlyph, Mask, Origin, Pixels},
        color::{Brush, ColorVersion, CompositeMode, PaintSink},
        outline::{DrawMemory, DrawOptions, PathSink},
    },
    raw::types::{BoundingBox, Matrix},
};

use crate::font::FontFace;

/// A glyph rasterized into RGBA8 pixels.
///
/// For coverage glyphs the RGB channels are opaque white and alpha is the coverage. For color
/// glyphs the RGB channels are *straight sRGB* color and alpha is the coverage. A paint backend
/// that samples the pixels through an `Rgba8Unorm` texture gets those encoded values unchanged,
/// which is what the gamma-space renderer expects.
pub struct RasterizedGlyph {
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
    /// Horizontal offset from the pen position, in physical pixels.
    pub left: f32,
    /// Distance from the baseline to the top edge, in physical pixels.
    pub top: f32,
    /// True for color glyphs (color bitmaps and COLRv0), which ignore the text color.
    ///
    /// Monochrome and grayscale embedded bitmaps are coverage glyphs; their pixels are white with
    /// the bitmap coverage in alpha and they are tinted with the text color.
    pub color: bool,
    /// RGBA8 pixels, row-major, `width * height * 4` bytes.
    pub pixels: Vec<u8>,
}

/// Rasterizes one glyph at the given pixels per em.
///
/// Embedded bitmaps are preferred, then COLRv0, then the regular outline. `None` means that
/// there is nothing to draw (a space, a missing glyph, or a format we cannot decode).
pub fn rasterize(face: &FontFace, glyph_id: u32, ppem: f32) -> Option<RasterizedGlyph> {
    rasterize_font(
        face.font(),
        GlyphId::new(glyph_id),
        ppem,
        face.units_per_em(),
        face.has_bitmap_tables(),
        face.has_color_tables(),
    )
}

fn rasterize_font(
    font: &Font,
    glyph_id: GlyphId,
    ppem: f32,
    units_per_em: u16,
    has_bitmap_tables: bool,
    has_color_tables: bool,
) -> Option<RasterizedGlyph> {
    if has_bitmap_tables && let Some(glyph) = rasterize_bitmap(font, glyph_id, ppem, units_per_em) {
        return Some(glyph);
    }
    if has_color_tables && let Some(glyph) = rasterize_colr_v0(font, glyph_id, ppem) {
        return Some(glyph);
    }
    rasterize_outline(font, glyph_id, ppem)
}

fn rasterize_outline(font: &Font, glyph_id: GlyphId, ppem: f32) -> Option<RasterizedGlyph> {
    // Pixel bounds from the glyph metrics. Asking for outlines only avoids measuring a bitmap or
    // color representation instead of the ink we are about to draw.
    let extents = font
        .glyph_metrics()
        .with_extents_source_order(GlyphSourceOrder::OUTLINES)
        .extents_exact(glyph_id)?;
    let scale = ppem / font.metrics().units_per_em.max(1) as f32;
    let left = extents.x_bearing.to_f32() * scale;
    let top = extents.y_bearing.to_f32() * scale;
    let min_x = left.floor();
    let max_x = (left + extents.width.to_f32() * scale).ceil();
    let max_y = top.ceil();
    let min_y = (top - extents.height.to_f32() * scale).floor();
    let width = (max_x - min_x) as i32;
    let height = (max_y - min_y) as i32;
    if width <= 0 || height <= 0 {
        return None;
    }

    let glyph = font.outline_glyphs().get(glyph_id)?;
    let mut pen = RasterPen::new(width as usize, height as usize, min_x, max_y);
    glyph
        .draw(
            &DrawOptions::new().with_ppem(Some(ppem)),
            DrawMemory::Scratch,
            &mut pen,
        )
        .ok()?;

    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    let mut index = 0;
    pen.rasterizer.for_each_pixel(|_, alpha| {
        let offset = index * 4;
        pixels[offset] = 255;
        pixels[offset + 1] = 255;
        pixels[offset + 2] = 255;
        pixels[offset + 3] = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
        index += 1;
    });

    Some(RasterizedGlyph {
        width: width as u32,
        height: height as u32,
        left: min_x,
        top: max_y,
        color: false,
        pixels,
    })
}

/// Draws an outline straight into a coverage rasterizer, flipping the y axis to match the
/// rasterizer's top-left origin.
struct RasterPen {
    rasterizer: ab_glyph_rasterizer::Rasterizer,
    min_x: f32,
    max_y: f32,
    start: (f32, f32),
    current: (f32, f32),
}

impl RasterPen {
    fn new(width: usize, height: usize, min_x: f32, max_y: f32) -> Self {
        Self {
            rasterizer: ab_glyph_rasterizer::Rasterizer::new(width, height),
            min_x,
            max_y,
            start: (0.0, 0.0),
            current: (0.0, 0.0),
        }
    }

    #[inline]
    fn point(&self, x: f32, y: f32) -> ab_glyph_rasterizer::Point {
        ab_glyph_rasterizer::point(x - self.min_x, self.max_y - y)
    }
}

impl PathSink for RasterPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.start = (x, y);
        self.current = (x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.rasterizer
            .draw_line(self.point(self.current.0, self.current.1), self.point(x, y));
        self.current = (x, y);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.rasterizer.draw_quad(
            self.point(self.current.0, self.current.1),
            self.point(cx0, cy0),
            self.point(x, y),
        );
        self.current = (x, y);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.rasterizer.draw_cubic(
            self.point(self.current.0, self.current.1),
            self.point(cx0, cy0),
            self.point(cx1, cy1),
            self.point(x, y),
        );
        self.current = (x, y);
    }

    fn close(&mut self) {
        self.rasterizer.draw_line(
            self.point(self.current.0, self.current.1),
            self.point(self.start.0, self.start.1),
        );
        self.current = self.start;
    }
}

fn rasterize_bitmap(
    font: &Font,
    glyph_id: GlyphId,
    ppem: f32,
    units_per_em: u16,
) -> Option<RasterizedGlyph> {
    // Pick the strike closest to the requested size that actually contains this glyph.
    let (strike_ppem, bitmap) = font
        .bitmap_strikes()
        .iter()
        .filter_map(|strike| {
            let strike_ppem = strike.ppem() as f32;
            strike.get(glyph_id).map(|glyph| (strike_ppem, glyph))
        })
        .min_by(|(a, _), (b, _)| (a - ppem).abs().total_cmp(&(b - ppem).abs()))?;
    let (source, source_width, source_height, color) = decode_bitmap(&bitmap)?;
    if source_width == 0 || source_height == 0 {
        return None;
    }

    let strike_ppem = strike_ppem.max(1.0);
    let scale = (ppem / strike_ppem).max(0.01);
    let width = ((source_width as f32 * scale).round() as u32).max(1);
    let height = ((source_height as f32 * scale).round() as u32).max(1);
    if width > 4096 || height > 4096 {
        return None;
    }
    // Keep the bitmap in sRGB space: the renderer's non-sRGB texture samples it unchanged.
    let pixels = scale_rgba(&source, source_width, source_height, width, height);

    // The outer bearings (`x_bearing`/`y_bearing`) are in font units; the inner bearings are in
    // strike pixels. This mirrors Skia's fontations port, which is tested against CoreText.
    let units_to_pixels = ppem / units_per_em.max(1) as f32;
    let left =
        bitmap.x_bearing() as f32 * units_to_pixels + bitmap.inner_x_bearing() as f32 * scale;
    let top = bitmap.y_bearing() as f32 * units_to_pixels
        + match bitmap.origin() {
            Origin::TopLeft => bitmap.inner_y_bearing() as f32 * scale,
            Origin::BottomLeft => (bitmap.inner_y_bearing() as f32 + source_height as f32) * scale,
        };

    Some(RasterizedGlyph {
        width,
        height,
        left,
        top,
        color,
        pixels,
    })
}

/// The decoded pixels plus whether they carry color or only coverage.
fn decode_bitmap(bitmap: &BitmapGlyph<'_>) -> Option<(Vec<u8>, u32, u32, bool)> {
    let width = bitmap.width();
    let height = bitmap.height();
    let pixels = bitmap.pixels();
    let color = pixels_are_color(pixels);
    match pixels {
        Pixels::Bgra(data) => {
            let expected = width as usize * height as usize * 4;
            let source = data.data();
            if source.len() < expected {
                return None;
            }
            let mut rgba = Vec::with_capacity(expected);
            for &[blue, green, red, alpha] in source[..expected].as_chunks::<4>().0 {
                // CBDT stores premultiplied BGRA in sRGB space; the output uses straight alpha.
                if alpha == 0 {
                    rgba.extend_from_slice(&[0, 0, 0, 0]);
                } else {
                    let unpremultiply = |channel: u8| {
                        ((channel as u32 * 255 + alpha as u32 / 2) / alpha as u32).min(255) as u8
                    };
                    rgba.extend_from_slice(&[
                        unpremultiply(red),
                        unpremultiply(green),
                        unpremultiply(blue),
                        alpha,
                    ]);
                }
            }
            Some((rgba, width, height, color))
        }
        Pixels::Png(data) => {
            #[cfg(feature = "emoji-bitmap")]
            {
                decode_png(data).map(|(rgba, width, height)| (rgba, width, height, true))
            }
            #[cfg(not(feature = "emoji-bitmap"))]
            {
                let _ = data;
                None
            }
        }
        Pixels::Mask(mask) => {
            let rgba = decode_mask(&mask)?;
            Some((rgba, width, height, false))
        }
    }
}

/// Whether the pixel data is color rather than coverage.
fn pixels_are_color(pixels: Pixels<'_>) -> bool {
    match pixels {
        Pixels::Bgra(_) | Pixels::Png(_) => true,
        Pixels::Mask(_) => false,
    }
}

fn decode_mask(mask: &Mask<'_>) -> Option<Vec<u8>> {
    let width = mask.width();
    let height = mask.height();
    let mut rgba = vec![0u8; width as usize * height as usize * 4];
    for (index, coverage) in mask.pixels().enumerate() {
        let offset = index * 4;
        rgba[offset] = 255;
        rgba[offset + 1] = 255;
        rgba[offset + 2] = 255;
        rgba[offset + 3] = coverage;
    }
    Some(rgba)
}

#[cfg(feature = "emoji-bitmap")]
fn decode_png(data: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(data));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let buffer_size = reader.output_buffer_size()?;
    let mut buffer = vec![0; buffer_size];
    let info = reader.next_frame(&mut buffer).ok()?;
    let bytes = &buffer[..info.buffer_size()];
    let width = info.width;
    let height = info.height;

    let rgba = match info.color_type {
        png::ColorType::Rgba => bytes.to_vec(),
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity(bytes.len() / 3 * 4);
            for pixel in bytes.as_chunks::<3>().0 {
                out.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
            out
        }
        png::ColorType::Grayscale => {
            let mut out = Vec::with_capacity(bytes.len() * 4);
            for &gray in bytes {
                out.extend_from_slice(&[gray, gray, gray, 255]);
            }
            out
        }
        png::ColorType::GrayscaleAlpha => {
            let mut out = Vec::with_capacity(bytes.len() * 2);
            for &[gray, alpha] in bytes.as_chunks::<2>().0 {
                out.extend_from_slice(&[gray, gray, gray, alpha]);
            }
            out
        }
        png::ColorType::Indexed => return None,
    };
    Some((rgba, width, height))
}

fn scale_rgba(
    source: &[u8],
    source_width: u32,
    source_height: u32,
    width: u32,
    height: u32,
) -> Vec<u8> {
    if source_width == width && source_height == height {
        return source.to_vec();
    }

    let mut out = vec![0u8; width as usize * height as usize * 4];
    for y in 0..height {
        let source_y = ((y as f32 + 0.5) * source_height as f32 / height as f32 - 0.5).max(0.0);
        let y0 = source_y.floor() as u32;
        let y1 = (y0 + 1).min(source_height - 1);
        let fy = source_y - y0 as f32;
        for x in 0..width {
            let source_x = ((x as f32 + 0.5) * source_width as f32 / width as f32 - 0.5).max(0.0);
            let x0 = source_x.floor() as u32;
            let x1 = (x0 + 1).min(source_width - 1);
            let fx = source_x - x0 as f32;

            let p00 = sample_premultiplied(source, source_width, x0, y0);
            let p10 = sample_premultiplied(source, source_width, x1, y0);
            let p01 = sample_premultiplied(source, source_width, x0, y1);
            let p11 = sample_premultiplied(source, source_width, x1, y1);

            let mut pixel = [0.0f32; 4];
            for channel in 0..4 {
                let top = p00[channel] * (1.0 - fx) + p10[channel] * fx;
                let bottom = p01[channel] * (1.0 - fx) + p11[channel] * fx;
                pixel[channel] = top * (1.0 - fy) + bottom * fy;
            }

            let alpha = pixel[3];
            let offset = (y as usize * width as usize + x as usize) * 4;
            if alpha > 0.0 {
                out[offset] = ((pixel[0] / alpha) * 255.0).round().clamp(0.0, 255.0) as u8;
                out[offset + 1] = ((pixel[1] / alpha) * 255.0).round().clamp(0.0, 255.0) as u8;
                out[offset + 2] = ((pixel[2] / alpha) * 255.0).round().clamp(0.0, 255.0) as u8;
            }
            out[offset + 3] = (alpha * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

fn sample_premultiplied(source: &[u8], width: u32, x: u32, y: u32) -> [f32; 4] {
    let offset = (y as usize * width as usize + x as usize) * 4;
    if offset + 4 > source.len() {
        return [0.0; 4];
    }
    let alpha = source[offset + 3] as f32 / 255.0;
    [
        source[offset] as f32 / 255.0 * alpha,
        source[offset + 1] as f32 / 255.0 * alpha,
        source[offset + 2] as f32 / 255.0 * alpha,
        alpha,
    ]
}

fn rasterize_colr_v0(font: &Font, glyph_id: GlyphId, ppem: f32) -> Option<RasterizedGlyph> {
    let palette: Vec<[u8; 4]> = font
        .palettes()
        .get(0)
        .map(|palette| {
            palette
                .colors()
                .iter()
                .map(|color| [color.red(), color.green(), color.blue(), color.alpha()])
                .collect()
        })
        .unwrap_or_default();
    if palette.is_empty() {
        return None;
    }

    let color_glyph = font
        .color_glyphs()
        .get_with_version(glyph_id, ColorVersion::V0)?;
    let mut painter = ColrV0Painter {
        palette: &palette,
        layers: Vec::new(),
    };
    color_glyph.paint(&mut painter).ok()?;
    if painter.layers.is_empty() {
        return None;
    }

    let mut layers = Vec::new();
    for (layer_id, color) in painter.layers {
        if let Some(raster) = rasterize_outline(font, layer_id, ppem) {
            layers.push((raster, color));
        }
    }
    if layers.is_empty() {
        return None;
    }

    let min_x = layers
        .iter()
        .map(|(raster, _)| raster.left)
        .fold(f32::INFINITY, f32::min)
        .floor();
    let max_x = layers
        .iter()
        .map(|(raster, _)| raster.left + raster.width as f32)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil();
    let min_y = layers
        .iter()
        .map(|(raster, _)| raster.top - raster.height as f32)
        .fold(f32::INFINITY, f32::min)
        .floor();
    let max_y = layers
        .iter()
        .map(|(raster, _)| raster.top)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil();
    if !min_x.is_finite() || max_x <= min_x || max_y <= min_y {
        return None;
    }

    let width = (max_x - min_x) as u32;
    let height = (max_y - min_y) as u32;
    if width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || width as u64 * height as u64 > 16 * 1024 * 1024
    {
        return None;
    }

    let mut canvas = vec![[0.0f32; 4]; width as usize * height as usize];
    for (raster, color) in &layers {
        // Composite layers in encoded sRGB space, like the renderer and a browser's UI compositor.
        let color = [
            color[0] as f32 / 255.0,
            color[1] as f32 / 255.0,
            color[2] as f32 / 255.0,
            color[3] as f32 / 255.0,
        ];
        let layer_alpha = color[3];
        let offset_x = (raster.left - min_x).round() as i32;
        let offset_y = (max_y - raster.top).round() as i32;
        for y in 0..raster.height as i32 {
            let canvas_y = offset_y + y;
            if canvas_y < 0 || canvas_y >= height as i32 {
                continue;
            }
            for x in 0..raster.width as i32 {
                let canvas_x = offset_x + x;
                if canvas_x < 0 || canvas_x >= width as i32 {
                    continue;
                }
                let coverage =
                    raster.pixels[((y as u32 * raster.width + x as u32) * 4 + 3) as usize] as f32
                        / 255.0;
                if coverage <= 0.0 {
                    continue;
                }
                let source_alpha = coverage * layer_alpha;
                let inverse = 1.0 - source_alpha;
                let target = &mut canvas[canvas_y as usize * width as usize + canvas_x as usize];
                target[0] = color[0] * source_alpha + target[0] * inverse;
                target[1] = color[1] * source_alpha + target[1] * inverse;
                target[2] = color[2] * source_alpha + target[2] * inverse;
                target[3] = source_alpha + target[3] * inverse;
            }
        }
    }

    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    for (index, premultiplied) in canvas.iter().enumerate() {
        let alpha = premultiplied[3].clamp(0.0, 1.0);
        let offset = index * 4;
        if alpha > 0.0 {
            for channel in 0..3 {
                pixels[offset + channel] = ((premultiplied[channel] / alpha) * 255.0)
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
        }
        pixels[offset + 3] = (alpha * 255.0).round() as u8;
    }

    Some(RasterizedGlyph {
        width,
        height,
        left: min_x,
        top: max_y,
        color: true,
        pixels,
    })
}

struct ColrV0Painter<'a> {
    palette: &'a [[u8; 4]],
    layers: Vec<(GlyphId, [u8; 4])>,
}

impl PaintSink for ColrV0Painter<'_> {
    fn push_transform(&mut self, _transform: Matrix<f32>) {}

    fn pop_transform(&mut self) {}

    fn push_clip_glyph(&mut self, _glyph_id: GlyphId) {}

    fn push_clip_box(&mut self, _clip_box: BoundingBox<f32>) {}

    fn pop_clip(&mut self) {}

    fn fill(&mut self, _brush: Brush<'_>) {}

    fn fill_glyph(
        &mut self,
        glyph_id: GlyphId,
        _brush_transform: Option<Matrix<f32>>,
        brush: Brush<'_>,
    ) {
        let Brush::Solid {
            palette_index,
            alpha,
        } = brush
        else {
            return;
        };
        if let Some(color) = self.palette.get(palette_index as usize) {
            let alpha = (color[3] as f32 * alpha).clamp(0.0, 255.0) as u8;
            self.layers
                .push((glyph_id, [color[0], color[1], color[2], alpha]));
        }
    }

    fn push_layer(&mut self, _composite_mode: CompositeMode) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaling_keeps_a_single_color() {
        let source = [255u8, 0, 0, 255].repeat(4);
        let scaled = scale_rgba(&source, 2, 2, 4, 4);
        for pixel in scaled.as_chunks::<4>().0 {
            assert_eq!(pixel, &[255, 0, 0, 255]);
        }
    }

    #[test]
    fn monochrome_masks_are_not_color() {
        let mask = Mask::new(&[0xFF], 1, 1, 8, false).unwrap();
        assert!(!pixels_are_color(Pixels::Mask(mask)));

        let bgra = skrifa::font::glyph::bitmap::Bgra::new(&[0, 0, 255, 255], 1, 1).unwrap();
        assert!(pixels_are_color(Pixels::Bgra(bgra)));
    }

    #[test]
    fn mask_unpacking_handles_bpp() {
        let mask = Mask::new(&[0b1010_0000, 0b0101_0000], 4, 2, 1, false).unwrap();
        let rgba = decode_mask(&mask).unwrap();
        let alpha: Vec<u8> = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| pixel[3])
            .collect();
        assert_eq!(alpha, [255, 0, 255, 0, 0, 255, 0, 255]);
    }
}
