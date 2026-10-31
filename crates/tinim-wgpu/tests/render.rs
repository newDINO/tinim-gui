//! Smoke test: render two quads offscreen and read the pixels back.
//!
//! Skipped silently when no wgpu adapter is available (e.g. a machine without a GPU and without a
//! software Vulkan driver).

use tinim_paint::{Color32, Painter, Rect, pos2, vec2};
use tinim_wgpu::wgpu;
use tinim_wgpu::{Renderer, ScreenDescriptor};

const WIDTH: u32 = 32;
const HEIGHT: u32 = 16;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const CLEAR: wgpu::Color = wgpu::Color::BLACK;

#[test]
fn renders_quads_and_applies_the_scissor() {
    let Some((device, queue)) = setup() else {
        eprintln!("skipping: no wgpu adapter available");
        return;
    };

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("test target"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());

    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(WIDTH as f32, HEIGHT as f32));
    // Left half: green, clipped to the left half. Right half: red, clipped to the right half.
    let mut painter = Painter::new(WIDTH as f32, HEIGHT as f32, 1.0);
    painter.set_clip_rect(Rect::from_min_size(
        pos2(0.0, 0.0),
        vec2(16.0, HEIGHT as f32),
    ));
    painter.rect(screen, Color32::from_rgb(0, 255, 0));
    painter.set_clip_rect(Rect::from_min_size(
        pos2(16.0, 0.0),
        vec2(16.0, HEIGHT as f32),
    ));
    painter.rect(screen, Color32::from_rgb(255, 0, 0));
    let list = painter.finish();
    assert_eq!(list.draws.len(), 2);

    let mut renderer = Renderer::new(&device, &queue, FORMAT);
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(CLEAR),
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
    assert_pixel(&rgba, 4, 8, [0, 255, 0]);
    assert_pixel(&rgba, 20, 8, [255, 0, 0]);
}

fn setup() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .ok()?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("tinim test"),
        required_limits: wgpu::Limits::downlevel_defaults(),
        ..Default::default()
    }))
    .ok()?;
    Some((device, queue))
}

fn read_back(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut encoder: wgpu::CommandEncoder,
    target: &wgpu::Texture,
) -> Vec<u8> {
    let bytes_per_row = (WIDTH * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
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
        .expect("poll failed");
    receiver
        .recv()
        .expect("map callback missing")
        .expect("map failed");

    let mapped = slice.get_mapped_range().expect("mapping failed");
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

fn assert_pixel(rgba: &[u8], x: u32, y: u32, expected: [u8; 3]) {
    let i = ((y * WIDTH + x) * 4) as usize;
    let actual = [rgba[i], rgba[i + 1], rgba[i + 2]];
    let close = actual
        .iter()
        .zip(expected)
        .all(|(a, b)| (*a as i32 - b as i32).abs() <= 2);
    assert!(
        close,
        "pixel ({x}, {y}): got {actual:?}, expected {expected:?}"
    );
}
