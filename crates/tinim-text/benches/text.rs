//! Release-mode benchmark for text layout.
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
use tinim_text::{TextContext, TextStyle};

const WIDTH: u32 = 640;

#[cfg(feature = "system-fonts")]
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
                line_height: Some(34.0),
                ..Default::default()
            },
            max_width: (WIDTH - 80) as f32,
        },
    ]
}

fn text_context() -> TextContext {
    let mut text = TextContext::new();

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
    let text = text_context();
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

criterion_group!(benches, bench_layout);
criterion_main!(benches);
