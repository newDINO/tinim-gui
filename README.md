# tinim-gui

A tiny immediate-mode GUI.

## Crates

| crate | what it does |
|---|---|
| `tinim-paint` | math, colors, `Painter`, `PaintList`, tessellation |
| `tinim-text` | shaping, bidi, line breaking and a glyph atlas (harfrust + skrifa) |
| `tinim-wgpu` | one wgpu pipeline that draws a `PaintList` |
| `tinim-gui` | immediate-mode `Context`/`Ui`, vertical/horizontal layout, labels and press-only buttons |
| `tinim-winit` | `WindowEvent` → `RawInput` input adapter |

`tinim-paint` knows nothing about wgpu. `tinim-wgpu` knows nothing about font shaping. Both facts
are load-bearing.

