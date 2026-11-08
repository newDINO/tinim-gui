//! Renders a small text sample offscreen and writes `target/text_offscreen.png`.
//!
//! Run with `cargo run -p tinim-wgpu --example text`.
//! Without a GPU, install a software Vulkan driver first (on Fedora: `mesa-vulkan-drivers`).
//!
//! The fonts bundled with `font-test-data` are metrics/feature test fonts whose Latin glyphs are
//! solid rectangles, so the example loads every font it finds in the system font directories and
//! prints how many faces it added and how long that took. Pass a font file to load only that file
//! instead: `cargo run -p tinim-wgpu --example text -- /path/to/font.ttf`.
//!
//! The example goes through the whole pipeline: `TextSystem::layout` shapes the string,
//! `TextSystem::prepare` rasterizes the glyphs and builds meshes, `Painter` adds them to a draw
//! list, and `Renderer` draws that list into an offscreen texture.

use std::fs;
use std::path::PathBuf;

use tinim_paint::{Color32, Painter, TextureId, pos2};
use tinim_text::{TextStyle, TextSystem};
use tinim_wgpu::wgpu;
use tinim_wgpu::{Renderer, ScreenDescriptor};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 800;
const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Size and top edge of the atlas debug preview, in points.
const ATLAS_PREVIEW: f32 = 384.0;
const ATLAS_PREVIEW_TOP: f32 = 380.0;

const BACKGROUND: Color32 = Color32::from_rgb(24, 26, 32);
const FOREGROUND: Color32 = Color32::from_rgb(235, 238, 245);
const MUTED: Color32 = Color32::from_rgb(140, 148, 165);
const ACCENT: Color32 = Color32::from_rgb(90, 170, 255);

const GLYPH_TEXTURE: TextureId = TextureId(1);

/// Regular-weight system fonts to try as the primary face.
///
/// Discovery order depends on the filesystem, so without this the first face might be bold or
/// oblique. Every system font is still loaded afterwards, so the reported load time covers the
/// full directory scan.
const PRIMARY_FONT_FILES: &[&str] = &[
    "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/liberation-sans/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
    "C:/Windows/Fonts/segoeui.ttf",
    "C:/Windows/Fonts/arial.ttf",
];

/// CJK sample: Chinese, Japanese and Korean, which normally need fallback faces.
const CJK_TEXT: &str = "你好，世界 · こんにちは · 안녕하세요";

/// Right-to-left sample: Arabic and Hebrew, which also exercise bidi reordering and shaping.
const RTL_TEXT: &str = "مرحبا بالعالم · שלום עולם";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();
    pollster::block_on(run())?;
    println!("total: {:?}", start.elapsed());
    Ok(())
}

fn timed<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let start = std::time::Instant::now();
    let value = f();
    println!("{label}: {:?}", start.elapsed());
    value
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let instance = timed("create instance", || {
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env())
    });
    let adapter = timed("request adapter", || {
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
    })?;
    println!("{:?}", adapter.get_info());

    let (device, queue) = timed("request device", || {
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("tinim text offscreen"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))
    })?;

    // AHEM is a metrics test font: every glyph is a solid rectangle. Prefer real system fonts (or
    // the font file passed on the command line) for legible text.
    let mut text = timed("create text system", || TextSystem::new(GLYPH_TEXTURE));
    let loaded = match std::env::args().nth(1) {
        Some(path) => {
            timed("add font file", || text.add_font_file(path))?;
            true
        }
        None => {
            let primary = timed("add primary font", || {
                PRIMARY_FONT_FILES
                    .iter()
                    .find_map(|path| text.add_font_file(path).ok())
            });
            let start = std::time::Instant::now();
            let faces = text.load_system_fonts();
            println!("loaded {faces} system font faces in {:?}", start.elapsed());
            primary.is_some() || faces > 0
        }
    };
    if !loaded {
        eprintln!(
            "no system font found; falling back to AHEM, whose glyphs are solid blocks.\n\
             pass a font file as the first argument to render real text."
        );
        text.add_font(font_test_data::AHEM.to_vec())?;
    }
    // CBDT supplies a PNG-backed color emoji. The emoji is used through the fallback path and
    // exercises the bitmap + color-glyph code.
    timed("add emoji font", || {
        text.add_font(font_test_data::CBDT.to_vec())
    })?;

    timed("check coverage", || {
        for (label, sample) in [("CJK", CJK_TEXT), ("RTL", RTL_TEXT)] {
            let missing: String = sample
                .chars()
                .filter(|ch| !ch.is_whitespace() && *ch != '·')
                .filter(|ch| !text.fonts().iter().any(|(_, face)| face.has_char(*ch)))
                .collect();
            if !missing.is_empty() {
                eprintln!("warning: no loaded font covers {label} character(s): {missing:?}");
            }
        }
    });

    let title = timed("layout title", || {
        text.layout(
            "Hello, tinim! \u{1F600}",
            &TextStyle {
                font_size: 36.0,
                color: FOREGROUND,
                ..Default::default()
            },
            Some((WIDTH - 80) as f32),
        )
    });
    let body = timed("layout body", || {
        text.layout(
            "Text shaping, line breaking and a glyph atlas.\n\
             Everything is drawn into one RGBA8 texture.",
            &TextStyle {
                font_size: 20.0,
                color: MUTED,
                line_height: Some(28.0),
                ..Default::default()
            },
            Some((WIDTH - 80) as f32),
        )
    });
    let cjk = timed("layout cjk", || {
        text.layout(
            CJK_TEXT,
            &TextStyle {
                font_size: 22.0,
                color: FOREGROUND,
                line_height: Some(32.0),
                ..Default::default()
            },
            Some((WIDTH - 80) as f32),
        )
    });
    let rtl = timed("layout rtl", || {
        text.layout(
            RTL_TEXT,
            &TextStyle {
                font_size: 24.0,
                color: ACCENT,
                line_height: Some(34.0),
                ..Default::default()
            },
            Some((WIDTH - 80) as f32),
        )
    });

    let mut painter = timed("create painter", || {
        Painter::new(WIDTH as f32, HEIGHT as f32, 1.0)
    });
    // Solid geometry samples the atlas's reserved white texel, so backgrounds and text share one
    // draw call instead of alternating between the atlas and the renderer's white texture.
    text.ensure_atlas_texture();
    painter.set_solid_texture(text.atlas_texture_id());
    painter.rect(painter.canvas_rect(), BACKGROUND);
    painter.rect(
        tinim_paint::Rect::from_min_size(pos2(40.0, 36.0), tinim_paint::vec2(4.0, 54.0)),
        ACCENT,
    );

    timed("prepare title", || {
        painter.add_mesh(text.prepare(&title, pos2(60.0, 32.0), FOREGROUND, 1.0))
    });
    timed("prepare body", || {
        painter.add_mesh(text.prepare(&body, pos2(60.0, 130.0), MUTED, 1.0))
    });
    timed("prepare cjk", || {
        painter.add_mesh(text.prepare(&cjk, pos2(60.0, 230.0), FOREGROUND, 1.0))
    });
    timed("prepare rtl", || {
        painter.add_mesh(text.prepare(&rtl, pos2(60.0, 280.0), ACCENT, 1.0))
    });

    // Draw the whole atlas as a debug preview. It uses the same texture as the solid background,
    // so it stays in the same draw call.
    let atlas_size = text.atlas_size();
    let preview = tinim_paint::Rect::from_min_size(
        pos2((WIDTH as f32 - ATLAS_PREVIEW) * 0.5, ATLAS_PREVIEW_TOP),
        tinim_paint::vec2(ATLAS_PREVIEW, ATLAS_PREVIEW),
    );
    painter.rect(preview.expand(2.0), Color32::from_rgb(70, 76, 92));
    painter.rect(preview, Color32::from_rgb(16, 18, 24));
    painter.add_mesh(text.atlas_mesh(preview, Color32::WHITE));

    let list = timed("finish painter", || painter.finish());
    println!(
        "{} lines, {} draws, {} vertices, {}×{} atlas",
        title.line_count() + body.line_count() + cjk.line_count() + rtl.line_count(),
        list.draws.len(),
        list.vertices.len(),
        atlas_size[0],
        atlas_size[1],
    );

    let textures = timed("take textures", || text.take_textures());
    assert_eq!(
        textures.create.len(),
        1,
        "the first frame creates the glyph atlas once"
    );
    assert!(
        !textures.set.is_empty(),
        "the first frame uploads the rasterized glyphs"
    );

    let target = timed("create target", || {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tinim text target"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    });
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());

    let mut renderer = timed("create renderer", || {
        Renderer::new(&device, &queue, TARGET_FORMAT)
    });
    timed("update textures", || {
        renderer.update_textures(&device, &queue, &textures)
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("tinim text frame"),
    });
    timed("render pass", || {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("tinim text pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        renderer.render(
            &device,
            &queue,
            &mut pass,
            ScreenDescriptor::new(WIDTH, HEIGHT),
            list,
        );
    });

    let rgba = timed("read back", || read_back(&device, &queue, encoder, &target));

    // Loose checks: the title region must contain light text pixels and the emoji must bring in
    // actual chroma (the monochrome text and the muted background are nearly neutral).
    let title_light = timed("count title pixels", || {
        count_pixels(&rgba, 40, 20, 620, 120, |[r, g, b, _]| {
            r > 180 && g > 180 && b > 180
        })
    });
    assert!(title_light > 200, "title text is missing: {title_light} px");
    // The emoji sits after the text, so its x position depends on the font's advances; scan the
    // whole title band (the accent bar at x = 40..44 is outside it).
    let emoji_chroma = timed("count emoji pixels", || {
        count_pixels(&rgba, 60, 20, 620, 140, |[r, g, b, _]| {
            r.max(g).max(b) - r.min(g).min(b) > 40
        })
    });
    assert!(
        emoji_chroma > 40,
        "fallback emoji did not render in color: {emoji_chroma} px"
    );

    // The atlas preview below the text must contain actual glyph pixels.
    let preview_x = ((WIDTH as f32 - ATLAS_PREVIEW) * 0.5) as u32;
    let preview_y = ATLAS_PREVIEW_TOP as u32;
    let atlas_light = timed("count atlas pixels", || {
        count_pixels(
            &rgba,
            preview_x,
            preview_y,
            preview_x + ATLAS_PREVIEW as u32,
            preview_y + ATLAS_PREVIEW as u32,
            |[r, g, b, _]| r > 180 && g > 180 && b > 180,
        )
    });
    assert!(
        atlas_light > 100,
        "atlas preview is missing: {atlas_light} px"
    );

    timed("write png", || write_png(&rgba))?;
    Ok(())
}

fn read_back(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut encoder: wgpu::CommandEncoder,
    target: &wgpu::Texture,
) -> Vec<u8> {
    let bytes_per_row = (WIDTH * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("tinim text readback"),
        size: (bytes_per_row * HEIGHT) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(HEIGHT),
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );

    queue.submit(Some(encoder.finish()));

    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("failed to wait for the GPU");
    receiver
        .recv()
        .expect("map_async never fired")
        .expect("map_async failed");

    let mapped = slice
        .get_mapped_range()
        .expect("failed to map the readback buffer");
    let row_bytes = (WIDTH * 4) as usize;
    let mut rgba = Vec::with_capacity(row_bytes * HEIGHT as usize);
    for row in 0..HEIGHT as usize {
        let start = row * bytes_per_row as usize;
        rgba.extend_from_slice(&mapped[start..start + row_bytes]);
    }
    drop(mapped);
    buffer.unmap();
    rgba
}

fn count_pixels(
    rgba: &[u8],
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    predicate: impl Fn([u8; 4]) -> bool,
) -> usize {
    let mut count = 0;
    for y in y0..y1.min(HEIGHT) {
        for x in x0..x1.min(WIDTH) {
            let i = ((y * WIDTH + x) * 4) as usize;
            if predicate([rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]) {
                count += 1;
            }
        }
    }
    count
}

fn write_png(rgba: &[u8]) -> std::io::Result<()> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
    fs::create_dir_all(&dir)?;
    let path = dir.join("text_offscreen.png");
    let file = fs::File::create(&path)?;
    let writer = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, WIDTH, HEIGHT);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut header| header.write_image_data(rgba))
        .expect("failed to write PNG");
    println!("wrote {}", path.display());
    Ok(())
}
