# tinim-gui

A tiny immediate-mode GUI.

## Crates

| crate | what it does |
|---|---|
| `tinim-paint` | math, colors, `Painter`, `PaintList`, tessellation, text (cosmic-text + glyph atlas) |
| `tinim-wgpu` | one wgpu pipeline that draws a `PaintList` |
| `tinim-gui` | *planned*: `Context`, `Ui`, layout, interaction |
| `tinim-winit` | *planned*: `WindowEvent` → `RawInput` |

`tinim-paint` knows nothing about wgpu. `tinim-wgpu` knows nothing about cosmic-text. Both facts are
load-bearing.

