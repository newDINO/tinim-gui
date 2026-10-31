//! Renders a button-shaped frame offscreen, checks a few pixels and writes `target/headless.png`.
//!
//! Run with `cargo run -p tinim-wgpu --example headless`.
//! On a machine without a GPU, install a software Vulkan driver (mesa/lavapipe) first.

use std::fs;
use std::path::PathBuf;

use tinim_paint::{Color32, Painter, Rect, RectShape, Shape, Stroke, pos2, vec2};
use tinim_wgpu::wgpu;
use tinim_wgpu::{Renderer, ScreenDescriptor};

const WIDTH: u32 = 480;
const HEIGHT: u32 = 240;
const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

const BACKGROUND: Color32 = Color32::from_rgb(26, 27, 32);
const BUTTON_FILL: Color32 = Color32::from_rgb(58, 62, 74);
const BUTTON_STROKE: Color32 = Color32::from_rgb(110, 116, 132);
const CLIPPED_RED: Color32 = Color32::from_rgb(200, 80, 80);
const CIRCLE_GREEN: Color32 = Color32::from_rgb(80, 200, 120);
const LINE_GRAY: Color32 = Color32::from_rgb(200, 200, 200);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    pollster::block_on(run())
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        })
        .await?;

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("tinim headless"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        })
        .await?;

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("tinim target"),
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
    });
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());

    let mut renderer = Renderer::new(&device, &queue, TARGET_FORMAT);

    let mut painter = build_shapes();
    let list = painter.finish();
    println!(
        "{} draws, {} vertices, {} indices",
        list.draws.len(),
        list.vertices.len(),
        list.indices.len()
    );

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("tinim frame"),
    });

    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("tinim pass"),
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
    }

    let rgba = read_back(&device, &queue, encoder, &target);

    // A handful of checks so that a broken pipeline fails loudly instead of producing a pretty PNG.
    assert_pixel(&rgba, 5, 5, BACKGROUND);
    assert_pixel(&rgba, 240, 118, BUTTON_FILL);
    assert_pixel(&rgba, 240, 91, BUTTON_STROKE);
    assert_pixel(&rgba, 30, 30, CLIPPED_RED);
    assert_pixel(&rgba, 90, 30, BACKGROUND); // outside the clip rect
    assert_pixel(&rgba, 420, 40, CIRCLE_GREEN);
    assert_pixel(&rgba, 240, 200, LINE_GRAY);

    write_png(&rgba)?;
    Ok(())
}

fn build_shapes() -> Painter {
    let mut painter = Painter::new(WIDTH as f32, HEIGHT as f32, 1.0);
    let screen = painter.canvas_rect();

    let button = Rect::from_min_size(pos2(140.0, 90.0), vec2(200.0, 56.0));

    painter.rect(screen, BACKGROUND);
    painter.add(Shape::Rect(
        RectShape::new(button, BUTTON_FILL)
            .with_corner_radius(8)
            .with_stroke(Stroke::new(2.0, BUTTON_STROKE)),
    ));

    // Half of this rectangle is clipped away; the scissor must do the cutting.
    painter.set_clip_rect(Rect::from_min_max(pos2(20.0, 20.0), pos2(70.0, 60.0)));
    painter.rect(
        Rect::from_min_size(pos2(20.0, 20.0), vec2(100.0, 40.0)),
        CLIPPED_RED,
    );

    painter.reset_clip_rect();
    painter.circle(pos2(420.0, 40.0), 24.0, CIRCLE_GREEN);
    painter.line(&[pos2(20.0, 200.0), pos2(460.0, 200.0)], 4.0, LINE_GRAY);

    painter
}

fn read_back(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut encoder: wgpu::CommandEncoder,
    target: &wgpu::Texture,
) -> Vec<u8> {
    let bytes_per_row = (WIDTH * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("tinim readback"),
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

fn pixel(rgba: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * WIDTH + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

fn assert_pixel(rgba: &[u8], x: u32, y: u32, expected: Color32) {
    let actual = pixel(rgba, x, y);
    let expected = expected.0;
    let close = actual
        .iter()
        .zip(expected)
        .all(|(a, b)| (*a as i32 - b as i32).abs() <= 2);
    assert!(
        close,
        "pixel ({x}, {y}): got {actual:?}, expected {expected:?}"
    );
}

fn write_png(rgba: &[u8]) -> std::io::Result<()> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
    fs::create_dir_all(&dir)?;
    let path = dir.join("headless.png");
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
