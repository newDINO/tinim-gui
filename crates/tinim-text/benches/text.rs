//! Release-mode benchmark for text layout and cached glyph preparation.
//!
//! Run with:
//!
//! ```text
//! cargo bench -p tinim-text --features system-fonts --bench text
//! ```
//!
//! Without `system-fonts` the benchmark falls back to the bundled AHEM and CBDT fonts, which
//! keeps it runnable everywhere but does not exercise fallback across many system faces.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use tinim_paint::{Color32, TextureId, pos2};
use tinim_text::{TextStyle, TextSystem};

const WIDTH: u32 = 640;

const BACKGROUND_ACCENT: Color32 = Color32::from_rgb(90, 170, 255);
const FOREGROUND: Color32 = Color32::from_rgb(235, 238, 245);
const MUTED: Color32 = Color32::from_rgb(140, 148, 165);

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

const CJK_TEXT: &str = "你好，世界 · こんにちは · 안녕하세요";
const RTL_TEXT: &str = "مرحبا بالعالم · שלום עולם";

struct Sample {
    name: &'static str,
    text: &'static str,
    style: TextStyle,
    max_width: f32,
}

fn samples() -> Vec<Sample> {
    vec![
        Sample {
            name: "title",
            text: "Hello, tinim! \u{1F600}",
            style: TextStyle {
                font_size: 36.0,
                color: FOREGROUND,
                ..Default::default()
            },
            max_width: (WIDTH - 80) as f32,
        },
        Sample {
            name: "body",
            text: "Text shaping, line breaking and a glyph atlas.\n\
                   Everything is drawn into one RGBA8 texture.",
            style: TextStyle {
                font_size: 20.0,
                color: MUTED,
                line_height: Some(28.0),
                ..Default::default()
            },
            max_width: (WIDTH - 80) as f32,
        },
        Sample {
            name: "cjk",
            text: CJK_TEXT,
            style: TextStyle {
                font_size: 22.0,
                color: FOREGROUND,
                line_height: Some(32.0),
                ..Default::default()
            },
            max_width: (WIDTH - 80) as f32,
        },
        Sample {
            name: "rtl",
            text: RTL_TEXT,
            style: TextStyle {
                font_size: 24.0,
                color: BACKGROUND_ACCENT,
                line_height: Some(34.0),
                ..Default::default()
            },
            max_width: (WIDTH - 80) as f32,
        },
    ]
}

fn text_system() -> TextSystem {
    let mut text = TextSystem::new(TextureId(1));

    #[cfg(feature = "system-fonts")]
    let loaded = {
        let primary = PRIMARY_FONT_FILES
            .iter()
            .find_map(|path| text.add_font_file(path).ok());
        let faces = text.load_system_fonts();
        primary.is_some() || faces > 0
    };
    #[cfg(not(feature = "system-fonts"))]
    let loaded = false;

    if !loaded {
        text.add_font(font_test_data::AHEM.to_vec()).unwrap();
    }
    // Keep the emoji/bitmap path in the benchmark, matching the rendering example.
    text.add_font(font_test_data::CBDT.to_vec()).unwrap();
    text
}

fn bench_layout(c: &mut Criterion) {
    let text = text_system();
    let mut group = c.benchmark_group("text/layout");
    group.sample_size(50);

    for sample in samples() {
        group.bench_with_input(
            BenchmarkId::from_parameter(sample.name),
            &sample,
            |b, sample| {
                b.iter(|| {
                    black_box(text.layout(
                        black_box(sample.text),
                        black_box(&sample.style),
                        Some(sample.max_width),
                    ))
                })
            },
        );
    }

    group.finish();
}

fn bench_prepare_warm(c: &mut Criterion) {
    let mut text = text_system();
    let samples = samples();
    let layouts: Vec<_> = samples
        .iter()
        .map(|sample| text.layout(sample.text, &sample.style, Some(sample.max_width)))
        .collect();

    // Rasterize every glyph once so the benchmark measures mesh building with cached atlas slots.
    for layout in &layouts {
        black_box(text.prepare(layout, pos2(0.0, 0.0), 1.0));
    }
    let _ = text.take_textures();

    let mut group = c.benchmark_group("text/prepare_warm");
    group.sample_size(50);
    for (sample, layout) in samples.iter().zip(&layouts) {
        group.bench_with_input(
            BenchmarkId::from_parameter(sample.name),
            layout,
            |b, layout| b.iter(|| black_box(text.prepare(black_box(layout), pos2(0.0, 0.0), 1.0))),
        );
    }
    group.finish();
}

fn bench_layout_and_prepare_warm(c: &mut Criterion) {
    let mut text = text_system();
    let samples = samples();

    // Warm the atlas with one layout of each sample before measuring the combined path.
    for sample in &samples {
        let layout = text.layout(sample.text, &sample.style, Some(sample.max_width));
        black_box(text.prepare(&layout, pos2(0.0, 0.0), 1.0));
    }
    let _ = text.take_textures();

    let mut group = c.benchmark_group("text/layout+prepare_warm");
    group.sample_size(50);
    for sample in &samples {
        group.bench_with_input(
            BenchmarkId::from_parameter(sample.name),
            sample,
            |b, sample| {
                b.iter(|| {
                    let layout = text.layout(sample.text, &sample.style, Some(sample.max_width));
                    black_box(text.prepare(&layout, pos2(0.0, 0.0), 1.0))
                })
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_layout,
    bench_prepare_warm,
    bench_layout_and_prepare_warm
);
criterion_main!(benches);
