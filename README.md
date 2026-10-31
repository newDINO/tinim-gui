# tinim-gui

A tiny immediate-mode GUI, in the spirit of [egui](https://github.com/emilk/egui) but with a much
smaller surface area: no multi-window/viewport support, no paint callbacks, no renderer of its own.

The drawing model is intentionally the same as egui's: build `Shape`s, tessellate them into
`ClippedPrimitive`s, hand those to a wgpu renderer. If you know egui, you already know this.

> Status: **v0.1, paint layer only.** `tinim-paint` + `tinim-wgpu` render shapes and text.
> `tinim-gui` (widgets/layout) and `tinim-winit` (input) come next.

## Crates

| crate | what it does |
|---|---|
| `tinim-paint` | math, colors, `Shape`, tessellation, text (cosmic-text + glyph atlas) |
| `tinim-wgpu` | one wgpu pipeline that draws `ClippedPrimitive`s |
| `tinim-gui` | *planned*: `Context`, `Ui`, layout, interaction |
| `tinim-winit` | *planned*: `WindowEvent` → `RawInput` |

`tinim-paint` knows nothing about wgpu. `tinim-wgpu` knows nothing about cosmic-text. Both facts are
load-bearing.

## Quick start

```rust
use tinim_paint::{pos2, vec2, ClippedShape, Color32, Rect, Shape, Tessellator};
use tinim_wgpu::{Renderer, ScreenDescriptor};

let shapes = vec![ClippedShape::new(
    Rect::from_min_size(pos2(0.0, 0.0), vec2(480.0, 240.0)),
    Shape::rect(
        Rect::from_min_size(pos2(140.0, 90.0), vec2(200.0, 56.0)),
        Color32::from_rgb(58, 62, 74),
    ),
)];
let primitives = Tessellator::new().tessellate(&shapes, 1.0); // 1.0 = pixels_per_point

// once:
let mut renderer = Renderer::new(&device, &queue, target_format); // target_format: *Srgb
// every frame:
renderer.render(&device, &queue, &mut render_pass, ScreenDescriptor::new(480, 240), &primitives);
```

See `crates/tinim-wgpu/examples/headless.rs` for a complete, runnable example:

```sh
cargo run -p tinim-wgpu --example headless
# writes target/headless.png
```

## Design decisions

- **`Vec<ClippedPrimitive>`, like egui.** One mesh + one clip rect per shape. No `PaintCallback`;
  custom geometry uses `Shape::Mesh`.
- **Scissor, not per-vertex clip.** The clip rect becomes `set_scissor_rect`. The vertex stays 16
  bytes (pos `f32x2`, uv `u16x2`, color `u8x4`), and `discard` is never used.
- **16-byte vertices, one texture format.** The atlas is `Rgba8Unorm`: coverage in alpha, white RGB.
  An opaque white texel at `(0, 0)` is what untextured geometry samples, so one pipeline draws
  everything.
- **sRGB in, linear out.** Vertex colors are sRGB bytes; the shader converts to linear and outputs
  premultiplied alpha with `One / OneMinusSrcAlpha`. Use an `*Srgb` target format.
- **No anti-aliasing for shapes.** Text is smooth because of glyph coverage; rectangles are not.
- **Text is not optional.** A UI is mostly text, so `cosmic-text` is a hard dependency of
  `tinim-paint` and its shaping result (`Galley`) is the only type that leaks into `tinim-gui`.
- **No time, no frame scheduling.** The host calls `render` every frame; the library never asks for
  a redraw.

## Known limitations

- Shape edges are aliased (no feathering/SDF yet).
- One texture format (`Rgba8Unorm`); user images with color would need a second pipeline.
- Glyph atlas is fixed at 2048²/4096² and is cleared when full (one frame of flicker).
- Line joins are not mitered; each segment is an independent quad.
- No clipping stack merging: every shape is its own draw call.

## Development

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace            # GPU tests skip themselves if no adapter exists
cargo run -p tinim-wgpu --example headless
```

On a headless Linux box, install a software Vulkan driver (Fedora: `dnf install mesa-vulkan-drivers
vulkan-tools`, Debian: `apt install mesa-vulkan-drivers`). `vulkaninfo --summary` should show
`llvmpipe`. If wgpu cannot find an adapter, pin it:
`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json`.

## Roadmap

- **v0.1** — `tinim-paint` + `tinim-wgpu`: shapes, text, headless PNG. *(in progress)*
- **v0.2** — `tinim-gui` + `tinim-winit`: `Context`/`Ui`, sequential-rect layout, `button` with
  hover/press, clickable.
- **later** — scroll areas, focus/text editing, sliders, popups, SDF text.

See `plan.md` for the full design and the reasoning behind each non-goal.
