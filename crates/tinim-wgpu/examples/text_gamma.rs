//! Renders bold text with the renderer's gamma-space blending and writes a self-contained HTML
//! bundle for comparing it against the browser's own text rendering.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p tinim-wgpu --example text_gamma
//! ```
//!
//! By default it uses Noto Sans Regular and Bold from the system font directory. To use another
//! pair (the same family with a real bold face is required for the comparison to be meaningful):
//!
//! ```sh
//! cargo run -p tinim-wgpu --example text_gamma -- /path/regular.ttf /path/bold.ttf [out_dir]
//! ```
//!
//! The bundle lands in `target/text_gamma` by default. Chrome refuses to load fonts over
//! `file://`, so serve it over HTTP:
//!
//! ```sh
//! cd target/text_gamma && python3 -m http.server 8080
//! ```
//!
//! Then open <http://localhost:8080/compare.html>. The page shows tinim and the browser for both
//! black-on-white and white-on-black. It also prints an `ink` metric, which is the mean coverage
//! in encoded sRGB space; tinim's gamma-space values should be very close to the browser's.

use std::fs;
use std::path::{Path, PathBuf};

use tinim_paint::{Color32, PaintList, Painter, TextureId, pos2};
use tinim_paint_text::TextSystem;
use tinim_text::{FontWeight, TextLayout, TextStyle};
use tinim_wgpu::wgpu;
use tinim_wgpu::{Renderer, ScreenDescriptor};

const LOGICAL_WIDTH: u32 = 560;
const LOGICAL_HEIGHT: u32 = 300;

/// Left and top padding of the text inside each panel, in logical points.
const PAD: f32 = 20.0;

/// Big enough that no glyph is ever dropped because the atlas cannot grow between themes.
const ATLAS_SIZE: u32 = 2048;
const GLYPH_TEXTURE: TextureId = TextureId(1);

/// Common system font pairs. The first one that exists is used when no paths are passed.
const FONT_PAIRS: &[(&str, &str)] = &[
    (
        "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/google-noto/NotoSans-Bold.ttf",
    ),
    (
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/noto/NotoSans-Bold.ttf",
    ),
    (
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
    ),
    (
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    ),
    (
        "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-Bold.ttf",
    ),
    (
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    ),
    (
        "/usr/share/fonts/liberation-sans/LiberationSans-Regular.ttf",
        "/usr/share/fonts/liberation-sans/LiberationSans-Bold.ttf",
    ),
    (
        "C:/Windows/Fonts/segoeui.ttf",
        "C:/Windows/Fonts/segoeuib.ttf",
    ),
    ("C:/Windows/Fonts/arial.ttf", "C:/Windows/Fonts/arialbd.ttf"),
    (
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
    ),
    (
        "/System/Library/Fonts/Supplemental/Verdana.ttf",
        "/System/Library/Fonts/Supplemental/Verdana Bold.ttf",
    ),
];

/// Font sizes and the string drawn at each size. Small sizes make the anti-aliasing difference
/// easiest to see.
const SAMPLES: &[(f32, &str)] = &[
    (10.0, "Hamburgefonstiv 0123456789"),
    (11.0, "Hamburgefonstiv 0123456789"),
    (12.0, "Hamburgefonstiv 0123456789"),
    (14.0, "Hamburgefonstiv 0123456789"),
    (16.0, "Hamburgefonstiv 0123456789"),
    (20.0, "Hamburgefonstiv 0123456789"),
    (24.0, "Hamburgefonstiv 0123456789"),
    (32.0, "Hamburgefonstiv 0123456789"),
    (40.0, "Bold 0123456789"),
];

/// The scales rendered for the HTML page. The page picks the one closest to its DPR.
const SCALES: &[u32] = &[1, 2];

struct Theme {
    name: &'static str,
    bg: Color32,
    fg: Color32,
    css_bg: &'static str,
    css_fg: &'static str,
}

const THEMES: [Theme; 2] = [
    Theme {
        name: "light",
        bg: Color32::WHITE,
        fg: Color32::BLACK,
        css_bg: "#ffffff",
        css_fg: "#000000",
    },
    Theme {
        name: "dark",
        bg: Color32::BLACK,
        fg: Color32::WHITE,
        css_bg: "#000000",
        css_fg: "#ffffff",
    },
];

struct Row {
    text: &'static str,
    size: f32,
    x: f32,
    origin_y: f32,
    baseline: f32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    pollster::block_on(run())
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (regular, bold) = match (args.next(), args.next()) {
        (Some(regular), Some(bold)) => (PathBuf::from(regular), PathBuf::from(bold)),
        (Some(regular), None) => {
            let (_, bold) = default_font_pair();
            (PathBuf::from(regular), bold)
        }
        _ => default_font_pair(),
    };
    let out_dir = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "../../target/text_gamma".to_string()),
    );
    let out_dir = if out_dir.is_absolute() {
        out_dir
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(out_dir)
    };
    fs::create_dir_all(&out_dir)?;

    println!("regular: {}", regular.display());
    println!("bold:    {}", bold.display());
    println!("output:  {}", out_dir.display());
    write_font_files(&regular, &bold, &out_dir)?;

    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        })
        .await?;
    println!("adapter: {:?}", adapter.get_info());

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("tinim text gamma"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        })
        .await?;

    // The layout is independent of pixels_per_point, so compute it once and reuse it for every
    // scale. Font ids are add-order indices, so a fresh text system with the same fonts resolves
    // the same ids.
    let mut layout_system = TextSystem::new(GLYPH_TEXTURE);
    add_fonts(&mut layout_system, &regular, &bold)?;
    let rows = build_rows(&layout_system);
    print_selected_face(&layout_system, &rows);
    write_layout_json(&out_dir.join("layout.json"), &rows)?;

    for &scale in SCALES {
        let mut text = TextSystem::with_atlas_size(GLYPH_TEXTURE, ATLAS_SIZE);
        add_fonts(&mut text, &regular, &bold)?;

        let mut lists = Vec::with_capacity(THEMES.len());
        for theme in &THEMES {
            lists.push(build_painter(&mut text, &rows, theme, scale as f32));
        }
        let textures = text.take_textures();

        // Gamma-space blending needs a non-sRGB target; Rgba8Unorm is the offscreen equivalent of
        // the non-sRGB surface formats used by the demo.
        let target_format = wgpu::TextureFormat::Rgba8Unorm;
        let mut renderer = Renderer::new(&device, &queue, target_format);
        renderer.update_textures(&device, &queue, &textures);

        for (theme, list) in THEMES.iter().zip(&lists) {
            let width = LOGICAL_WIDTH * scale;
            let height = LOGICAL_HEIGHT * scale;
            let target = create_target(&device, width, height, target_format);
            let view = target.create_view(&wgpu::TextureViewDescriptor::default());

            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("tinim text gamma pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
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
                    ScreenDescriptor::new(width, height),
                    list,
                );
            }

            let rgba = read_back(&device, &queue, encoder, &target, width, height);
            let suffix = if scale == 1 {
                String::new()
            } else {
                format!("@{scale}x")
            };
            let path = out_dir.join(format!("gamma-{}{suffix}.png", theme.name));
            write_png(&path, width, height, &rgba)?;
            println!("wrote {}", path.display());
        }
    }

    fs::write(out_dir.join("compare.html"), COMPARE_HTML)?;
    println!("wrote {}", out_dir.join("compare.html").display());
    println!(
        "\nserve it over HTTP (Chrome blocks local font URLs):\n  cd {} && python3 -m http.server 8080\n  open http://localhost:8080/compare.html",
        out_dir.display()
    );
    Ok(())
}

fn default_font_pair() -> (PathBuf, PathBuf) {
    for &(regular, bold) in FONT_PAIRS {
        if Path::new(regular).is_file() {
            return (regular.into(), bold.into());
        }
    }
    // The paths are kept even when they do not exist: `write_font_files` then writes AHEM so the
    // example still runs, and the warning tells the user to pass a real pair.
    (
        "/usr/share/fonts/google-noto/NotoSans-Regular.ttf".into(),
        "/usr/share/fonts/google-noto/NotoSans-Bold.ttf".into(),
    )
}

fn write_font_files(regular: &Path, bold: &Path, out_dir: &Path) -> std::io::Result<()> {
    let regular_bytes = fs::read(regular)
        .or_else(|_| fs::read(bold))
        .unwrap_or_else(|_| {
            println!(
                "warning: no usable font found; falling back to AHEM, whose glyphs are solid \
                 blocks. Pass a regular and a bold font file to render real text."
            );
            font_test_data::AHEM.to_vec()
        });
    let bold_bytes = if bold.is_file() && bold != regular {
        fs::read(bold)?
    } else {
        regular_bytes.clone()
    };
    if !bold.is_file() || bold == regular {
        println!(
            "warning: no bold face at {}; the browser and tinim will both use the regular face.",
            bold.display()
        );
    }
    fs::write(out_dir.join("compare-regular.ttf"), regular_bytes)?;
    fs::write(out_dir.join("compare-bold.ttf"), bold_bytes)?;
    Ok(())
}

fn add_fonts(
    text: &mut TextSystem,
    regular: &Path,
    bold: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // Add bold first: with no preferred family, the first face's family is resolved with the CSS
    // weight rules, so this makes the bold face the primary candidate instead of the regular one.
    let regular_ok = regular.is_file();
    let bold_ok = bold.is_file() && bold != regular;
    if bold_ok {
        text.add_font_file(bold)?;
    }
    if regular_ok {
        text.add_font_file(regular)?;
    }
    if !regular_ok && !bold_ok {
        text.add_font(font_test_data::AHEM.to_vec())?;
    }
    Ok(())
}

fn build_rows(text: &TextSystem) -> Vec<(Row, TextLayout)> {
    let mut y = PAD;
    let mut rows = Vec::with_capacity(SAMPLES.len());
    for &(size, sample) in SAMPLES {
        let style = TextStyle {
            font_size: size,
            line_height: Some(size * 1.6),
            font_weight: FontWeight::BOLD,
            ..Default::default()
        };
        let layout = text.layout(sample, &style, Some(LOGICAL_WIDTH as f32 - PAD * 2.0));
        let baseline = y + layout.lines()[0].baseline();
        rows.push((
            Row {
                text: sample,
                size,
                x: PAD,
                origin_y: y,
                baseline,
            },
            layout,
        ));
        y += size * 1.6;
    }
    rows
}

fn print_selected_face(text: &TextSystem, rows: &[(Row, TextLayout)]) {
    let Some(glyph) = rows
        .iter()
        .flat_map(|(_, layout)| layout.lines())
        .flat_map(|line| line.glyphs())
        .next()
    else {
        return;
    };
    let Some(face) = text.font(glyph.font()) else {
        return;
    };
    println!(
        "selected face: {} (weight {})",
        face.full_name(),
        face.weight().value()
    );
    if face.weight().value() < 600 {
        println!(
            "warning: requested weight 700 but the selected face is {}; the font stack has no \
             bold face and tinim does not synthesize one",
            face.weight().value()
        );
    }
}

fn build_painter(
    text: &mut TextSystem,
    rows: &[(Row, TextLayout)],
    theme: &Theme,
    scale: f32,
) -> PaintList {
    let mut painter = Painter::new(LOGICAL_WIDTH as f32, LOGICAL_HEIGHT as f32, scale);
    // Backgrounds sample the atlas's reserved white texel, so they batch with the text.
    text.ensure_atlas_texture();
    painter.set_solid_texture(text.atlas_texture_id());
    painter.rect(painter.canvas_rect(), theme.bg);
    for (row, layout) in rows {
        let mesh = text.prepare(layout, pos2(row.x, row.origin_y), theme.fg, scale);
        painter.add_mesh(mesh);
    }
    painter.finish().clone()
}

fn create_target(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("tinim text gamma target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn read_back(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut encoder: wgpu::CommandEncoder,
    target: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let bytes_per_row = (width * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("tinim text gamma readback"),
        size: (bytes_per_row * height) as u64,
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
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
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
    let row_bytes = (width * 4) as usize;
    let mut rgba = Vec::with_capacity(row_bytes * height as usize);
    for row in 0..height as usize {
        let start = row * bytes_per_row as usize;
        rgba.extend_from_slice(&mapped[start..start + row_bytes]);
    }
    drop(mapped);
    buffer.unmap();
    rgba
}

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> std::io::Result<()> {
    let file = fs::File::create(path)?;
    let writer = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut header| header.write_image_data(rgba))
        .expect("failed to write PNG");
    Ok(())
}

fn write_layout_json(path: &Path, rows: &[(Row, TextLayout)]) -> std::io::Result<()> {
    let mut json = String::new();
    json.push_str("{\n");
    json.push_str(&format!(
        "  \"width\": {LOGICAL_WIDTH},\n  \"height\": {LOGICAL_HEIGHT},\n"
    ));
    json.push_str("  \"themes\": [\n");
    for (index, theme) in THEMES.iter().enumerate() {
        json.push_str(&format!(
            "    {{\"name\": \"{}\", \"bg\": \"{}\", \"fg\": \"{}\"}}{}\n",
            theme.name,
            theme.css_bg,
            theme.css_fg,
            if index + 1 == THEMES.len() { "" } else { "," }
        ));
    }
    json.push_str("  ],\n  \"rows\": [\n");
    for (index, (row, _)) in rows.iter().enumerate() {
        json.push_str(&format!(
            "    {{\"text\": \"{}\", \"size\": {:.0}, \"x\": {:.3}, \"baseline\": {:.3}}}{}\n",
            json_escape(row.text),
            row.size,
            row.x,
            row.baseline,
            if index + 1 == rows.len() { "" } else { "," }
        ));
    }
    json.push_str("  ]\n}\n");
    fs::write(path, json)
}

fn json_escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '"' | '\\' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

const COMPARE_HTML: &str = r##"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>tinim 与浏览器文字渲染对比</title>
<style>
  @font-face {
    font-family: "Tinim Compare";
    src: url("compare-regular.ttf") format("truetype");
    font-weight: 400;
    font-style: normal;
  }
  @font-face {
    font-family: "Tinim Compare";
    src: url("compare-bold.ttf") format("truetype");
    font-weight: 700;
    font-style: normal;
  }
  :root { color-scheme: dark; }
  body {
    margin: 24px;
    background: #17181c;
    color: #e6e6e6;
    font: 14px/1.55 system-ui, -apple-system, "Segoe UI", sans-serif;
  }
  h1 { font-size: 20px; margin: 0 0 8px; }
  h2 { font-size: 16px; margin: 28px 0 10px; }
  .note { color: #a8adba; max-width: 1180px; margin: 0 0 8px; }
  .columns {
    display: flex;
    gap: 14px;
    align-items: flex-start;
    overflow-x: auto;
    padding-bottom: 8px;
  }
  figure { margin: 0; }
  figcaption { color: #c8ccd6; font-size: 13px; margin: 0 0 6px; }
  .shot {
    width: 560px;
    height: 300px;
    display: block;
    border: 1px solid #3a3d46;
    box-sizing: border-box;
    background: #000;
  }
  .stat { color: #8f96a8; font: 12px/1.6 ui-monospace, SFMono-Regular, Consolas, monospace; }
  .stat strong { color: #d8dbe4; font-weight: 600; }
  footer { color: #8f96a8; margin-top: 28px; max-width: 1180px; }
  code { color: #d8dbe4; }
</style>
</head>
<body>
<h1>tinim 与浏览器文字渲染对比</h1>
<p class="note">
  两列使用同一组字体、字号和字符串。左边是 tinim（<code>Rgba8Unorm</code> target，gamma 空间混合），
  右边是当前浏览器的 Canvas2D 文字渲染。
</p>
<p class="note">
  <strong>ink</strong> 是整块面板在 sRGB 编码空间里的平均着色量：白底黑字统计变暗量，黑底白字统计变亮量。
  它近似等于字形覆盖率的总和，可以直接量化“看起来有多粗”。tinim 的数值应该和浏览器很接近。
</p>

<section data-theme="light">
  <h2>白底黑字（bold 700）</h2>
  <div class="columns">
    <figure>
      <figcaption>tinim（gamma 空间混合）</figcaption>
      <img class="shot" data-shot="gamma-light" alt="tinim gamma light theme">
      <div class="stat" data-stat="gamma-light">ink …</div>
    </figure>
    <figure>
      <figcaption>Browser（Canvas2D）</figcaption>
      <canvas class="shot" data-browser="light"></canvas>
      <div class="stat" data-stat="browser-light">ink …</div>
    </figure>
  </div>
</section>

<section data-theme="dark">
  <h2>黑底白字（bold 700）</h2>
  <div class="columns">
    <figure>
      <figcaption>tinim（gamma 空间混合）</figcaption>
      <img class="shot" data-shot="gamma-dark" alt="tinim gamma dark theme">
      <div class="stat" data-stat="gamma-dark">ink …</div>
    </figure>
    <figure>
      <figcaption>Browser（Canvas2D）</figcaption>
      <canvas class="shot" data-browser="dark"></canvas>
      <div class="stat" data-stat="browser-dark">ink …</div>
    </figure>
  </div>
</section>

<footer>
  浏览器使用自己的 hinting、抗锯齿和字体合成策略（例如 Linux/Windows 可能启用 LCD 子像素，
  macOS 使用灰度抗锯齿），因此不同平台的 Browser 列会有差异。它们的 ink 应该落在 tinim 附近。
  请在 100% 缩放下查看；页面会按 <code>devicePixelRatio</code> 自动选择 1x 或 2x 的 tinim 图片。
  <br>如果直接以 <code>file://</code> 打开，Chrome 会拒绝加载字体和读取像素，请用
  <code>python3 -m http.server</code> 提供服务。
</footer>

<script>
(async () => {
  const layout = await (await fetch("layout.json")).json();
  const dpr = window.devicePixelRatio || 1;
  const suffix = dpr >= 1.5 ? "@2x" : "";

  await Promise.all([
    document.fonts.load('700 16px "Tinim Compare"'),
    document.fonts.load('400 16px "Tinim Compare"'),
  ]);
  await document.fonts.ready;

  function drawBrowser(canvas, theme) {
    const scale = Math.max(1, dpr);
    canvas.width = Math.round(layout.width * scale);
    canvas.height = Math.round(layout.height * scale);
    canvas.style.width = layout.width + "px";
    canvas.style.height = layout.height + "px";
    const ctx = canvas.getContext("2d", { alpha: false });
    ctx.scale(scale, scale);
    ctx.fillStyle = theme.bg;
    ctx.fillRect(0, 0, layout.width, layout.height);
    ctx.fillStyle = theme.fg;
    ctx.textAlign = "left";
    ctx.textBaseline = "alphabetic";
    for (const row of layout.rows) {
      ctx.font = `700 ${row.size}px "Tinim Compare", sans-serif`;
      ctx.fillText(row.text, row.x, row.baseline);
    }
  }

  function inkOfPixels(data, theme) {
    let sum = 0;
    const pixels = data.length / 4;
    for (let i = 0; i < data.length; i += 4) {
      // Rec. 709 luma on encoded sRGB bytes, which is close to perceived brightness.
      const luma = (0.2126 * data[i] + 0.7152 * data[i + 1] + 0.0722 * data[i + 2]) / 255;
      sum += theme.name === "light" ? 1 - luma : luma;
    }
    return (sum / pixels) * 100;
  }

  async function inkOfImage(img, theme) {
    const canvas = document.createElement("canvas");
    canvas.width = img.naturalWidth;
    canvas.height = img.naturalHeight;
    const ctx = canvas.getContext("2d", { alpha: false });
    ctx.drawImage(img, 0, 0);
    return inkOfPixels(ctx.getImageData(0, 0, canvas.width, canvas.height).data, theme);
  }

  function setStat(section, key, value) {
    const stat = section.querySelector(`.stat[data-stat="${key}"]`);
    if (stat) stat.innerHTML = `ink <strong>${value.toFixed(3)}%</strong>`;
  }

  for (const theme of layout.themes) {
    const section = document.querySelector(`section[data-theme="${theme.name}"]`);
    if (!section) continue;

    const images = [...section.querySelectorAll("img[data-shot]")];
    await Promise.all(images.map((img) => new Promise((resolve, reject) => {
      img.onload = resolve;
      img.onerror = () => reject(new Error(`failed to load ${img.src}`));
      img.src = img.dataset.shot + suffix + ".png";
    })));

    for (const img of images) {
      setStat(section, img.dataset.shot, await inkOfImage(img, theme));
    }

    const canvas = section.querySelector("canvas[data-browser]");
    drawBrowser(canvas, theme);
    const ctx = canvas.getContext("2d");
    setStat(section, `browser-${theme.name}`, inkOfPixels(
      ctx.getImageData(0, 0, canvas.width, canvas.height).data,
      theme,
    ));
  }
})().catch((error) => {
  document.body.insertAdjacentHTML("beforeend",
    `<p style="color:#ff8080">对比脚本失败：${String(error)}。请通过 HTTP 打开本页。</p>`);
  console.error(error);
});
</script>
</body>
</html>
"##;
