//! Turning glyphs into RGBA8 bitmaps: outlines, COLRv0 and embedded bitmaps.

use skrifa::{
    FontRef, GlyphId, MetadataProvider,
    bitmap::{BitmapData, BitmapGlyph, Origin},
    color::{Brush, ColorGlyphFormat, ColorPainter, CompositeMode, Transform},
    instance::{LocationRef, Size},
    outline::{DrawSettings, pen::PathElement},
    raw::types::BoundingBox,
};

use crate::font::FontFace;

/// A glyph rasterized into RGBA8 pixels.
///
/// For coverage glyphs the RGB channels are opaque white and alpha is the coverage. For color
/// glyphs the RGB channels are *straight sRGB* color and alpha is the coverage. A paint backend
/// that samples the pixels through an sRGB texture gets linear RGB automatically.
pub struct RasterizedGlyph {
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
    /// Horizontal offset from the pen position, in physical pixels.
    pub left: f32,
    /// Distance from the baseline to the top edge, in physical pixels.
    pub top: f32,
    /// True for color glyphs (bitmaps and COLRv0), which ignore the text color.
    pub color: bool,
    /// RGBA8 pixels, row-major, `width * height * 4` bytes.
    pub pixels: Vec<u8>,
}

/// Rasterizes one glyph at the given pixels per em.
///
/// Embedded bitmaps are preferred, then COLRv0, then the regular outline. `None` means that
/// there is nothing to draw (a space, a missing glyph, or a format we cannot decode).
pub fn rasterize(face: &FontFace, glyph_id: u32, ppem: f32) -> Option<RasterizedGlyph> {
    let font = face.font().ok()?;
    rasterize_font(&font, GlyphId::new(glyph_id), ppem, face.units_per_em())
}

fn rasterize_font(
    font: &FontRef<'_>,
    glyph_id: GlyphId,
    ppem: f32,
    units_per_em: u16,
) -> Option<RasterizedGlyph> {
    if let Some(glyph) = rasterize_bitmap(font, glyph_id, ppem, units_per_em) {
        return Some(glyph);
    }
    if let Some(glyph) = rasterize_colr_v0(font, glyph_id, ppem) {
        return Some(glyph);
    }
    rasterize_outline(font, glyph_id, ppem)
}

fn rasterize_outline(font: &FontRef<'_>, glyph_id: GlyphId, ppem: f32) -> Option<RasterizedGlyph> {
    let glyph = font.outline_glyphs().get(glyph_id)?;
    let mut elements = Vec::new();
    glyph
        .draw(
            DrawSettings::unhinted(Size::new(ppem), LocationRef::default()),
            &mut elements,
        )
        .ok()?;
    if elements.is_empty() {
        return None;
    }

    let commands: Vec<zeno::Command> = elements
        .iter()
        .map(|element| match *element {
            PathElement::MoveTo { x, y } => zeno::Command::MoveTo(zeno::Point::new(x, y)),
            PathElement::LineTo { x, y } => zeno::Command::LineTo(zeno::Point::new(x, y)),
            PathElement::QuadTo { cx0, cy0, x, y } => {
                zeno::Command::QuadTo(zeno::Point::new(cx0, cy0), zeno::Point::new(x, y))
            }
            PathElement::CurveTo {
                cx0,
                cy0,
                cx1,
                cy1,
                x,
                y,
            } => zeno::Command::CurveTo(
                zeno::Point::new(cx0, cy0),
                zeno::Point::new(cx1, cy1),
                zeno::Point::new(x, y),
            ),
            PathElement::Close => zeno::Command::Close,
        })
        .collect();

    let mut mask = zeno::Mask::new(&commands);
    mask.origin(zeno::Origin::BottomLeft);
    // `Placement::top` is only correct once zeno knows the mask height; `inspect` forces that
    // before rendering, otherwise the glyph is placed as if it hung below the baseline.
    mask.inspect(|_, _, _| {});
    let (coverage, placement) = mask.render();
    let width = placement.width;
    let height = placement.height;
    if width == 0 || height == 0 {
        return None;
    }

    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    for (index, alpha) in coverage.iter().enumerate() {
        let offset = index * 4;
        pixels[offset] = 255;
        pixels[offset + 1] = 255;
        pixels[offset + 2] = 255;
        pixels[offset + 3] = *alpha;
    }

    Some(RasterizedGlyph {
        width,
        height,
        left: placement.left as f32,
        top: placement.top as f32,
        color: false,
        pixels,
    })
}

fn rasterize_bitmap(
    font: &FontRef<'_>,
    glyph_id: GlyphId,
    ppem: f32,
    units_per_em: u16,
) -> Option<RasterizedGlyph> {
    // Pick the strike closest to the requested size that actually contains this glyph.
    let (strike_ppem, bitmap) = font
        .bitmap_strikes()
        .iter()
        .filter_map(|strike| {
            let strike_ppem = strike.ppem();
            strike.get(glyph_id).map(|glyph| (strike_ppem, glyph))
        })
        .min_by(|(a, _), (b, _)| (a - ppem).abs().total_cmp(&(b - ppem).abs()))?;
    let (source, source_width, source_height) = decode_bitmap(&bitmap)?;
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
    // Keep the bitmap in sRGB space: the renderer's sRGB texture format decodes it on sample.
    let pixels = scale_rgba(&source, source_width, source_height, width, height);

    // The outer bearings (`bearing_x`/`bearing_y`) are in font units; the inner bearings are in
    // strike pixels. This mirrors Skia's fontations port, which is tested against CoreText.
    let units_to_pixels = ppem / units_per_em.max(1) as f32;
    let left = bitmap.bearing_x * units_to_pixels + bitmap.inner_bearing_x * scale;
    let top = bitmap.bearing_y * units_to_pixels
        + match bitmap.placement_origin {
            Origin::TopLeft => bitmap.inner_bearing_y * scale,
            Origin::BottomLeft => (bitmap.inner_bearing_y + source_height as f32) * scale,
        };

    Some(RasterizedGlyph {
        width,
        height,
        left,
        top,
        color: true,
        pixels,
    })
}

fn decode_bitmap(bitmap: &BitmapGlyph<'_>) -> Option<(Vec<u8>, u32, u32)> {
    let width = bitmap.width;
    let height = bitmap.height;
    match &bitmap.data {
        BitmapData::Bgra(data) => {
            let expected = width as usize * height as usize * 4;
            if data.len() < expected {
                return None;
            }
            let mut rgba = Vec::with_capacity(expected);
            for &[blue, green, red, alpha] in data[..expected].as_chunks::<4>().0 {
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
            Some((rgba, width, height))
        }
        BitmapData::Png(data) => {
            #[cfg(feature = "emoji-bitmap")]
            {
                decode_png(data)
            }
            #[cfg(not(feature = "emoji-bitmap"))]
            {
                let _ = data;
                None
            }
        }
        BitmapData::Mask(mask) => {
            let rgba = decode_mask(mask, width, height)?;
            Some((rgba, width, height))
        }
    }
}

fn decode_mask(mask: &skrifa::bitmap::MaskData<'_>, width: u32, height: u32) -> Option<Vec<u8>> {
    let bpp = mask.bpp as usize;
    if bpp == 0 || bpp > 8 {
        return None;
    }
    let max = ((1u16 << bpp) - 1) as f32;
    let row_bits = width as usize * bpp;
    let stride_bits = if mask.is_packed {
        row_bits
    } else {
        row_bits.div_ceil(8) * 8
    };

    let mut rgba = vec![0u8; width as usize * height as usize * 4];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let bit = y * stride_bits + x * bpp;
            let byte = mask.data.get(bit / 8).copied().unwrap_or(0);
            let shift = 8 - bpp - (bit % 8);
            let value = ((byte as u16) >> shift) & ((1u16 << bpp) - 1);
            let coverage = if bpp == 8 {
                value as u8
            } else {
                (value as f32 / max * 255.0).round() as u8
            };
            let offset = (y * width as usize + x) * 4;
            rgba[offset] = 255;
            rgba[offset + 1] = 255;
            rgba[offset + 2] = 255;
            rgba[offset + 3] = coverage;
        }
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

fn rasterize_colr_v0(font: &FontRef<'_>, glyph_id: GlyphId, ppem: f32) -> Option<RasterizedGlyph> {
    let palette: Vec<[u8; 4]> = font
        .color_palettes()
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
        .get_with_format(glyph_id, ColorGlyphFormat::ColrV0)?;
    let mut painter = ColrV0Painter {
        palette: &palette,
        layers: Vec::new(),
    };
    color_glyph
        .paint(LocationRef::default(), &mut painter)
        .ok()?;
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
        // Palette colors are sRGB with straight alpha: only RGB goes through the transfer curve.
        let color = [
            srgb_to_linear(color[0]),
            srgb_to_linear(color[1]),
            srgb_to_linear(color[2]),
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
                // Layers are composited in linear space above, but the output stores sRGB.
                pixels[offset + channel] = linear_to_srgb_u8(premultiplied[channel] / alpha);
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

impl ColorPainter for ColrV0Painter<'_> {
    fn push_transform(&mut self, _transform: Transform) {}

    fn pop_transform(&mut self) {}

    fn push_clip_glyph(&mut self, _glyph_id: GlyphId) {}

    fn push_clip_box(&mut self, _clip_box: BoundingBox<f32>) {}

    fn pop_clip(&mut self) {}

    fn fill(&mut self, _brush: Brush<'_>) {}

    fn fill_glyph(
        &mut self,
        glyph_id: GlyphId,
        _brush_transform: Option<Transform>,
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

fn srgb_to_linear(channel: u8) -> f32 {
    let value = channel as f32 / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb_u8(channel: f32) -> u8 {
    let value = channel.clamp(0.0, 1.0);
    let encoded = if value <= 0.003_130_8 {
        12.92 * value
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_round_trips_through_linear() {
        for channel in [0u8, 1, 16, 64, 128, 192, 254, 255] {
            let round_trip = linear_to_srgb_u8(srgb_to_linear(channel));
            assert!(
                (round_trip as i32 - channel as i32).abs() <= 1,
                "{channel} became {round_trip}"
            );
        }
    }

    #[test]
    fn scaling_keeps_a_single_color() {
        let source = [255u8, 0, 0, 255].repeat(4);
        let scaled = scale_rgba(&source, 2, 2, 4, 4);
        for pixel in scaled.as_chunks::<4>().0 {
            assert_eq!(pixel, &[255, 0, 0, 255]);
        }
    }

    #[test]
    fn mask_unpacking_handles_bpp() {
        let mask = skrifa::bitmap::MaskData {
            bpp: 1,
            is_packed: false,
            data: &[0b1010_0000, 0b0101_0000],
        };
        let rgba = decode_mask(&mask, 4, 2).unwrap();
        let alpha: Vec<u8> = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| pixel[3])
            .collect();
        assert_eq!(alpha, [255, 0, 255, 0, 0, 255, 0, 255]);
    }
}
