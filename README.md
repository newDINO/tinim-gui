# tinim-gui

A tiny immediate-mode GUI.

## Crates

| crate | what it does |
|---|---|
| `tinim-paint` | math, colors, `Shape`, tessellation, text (cosmic-text + glyph atlas) |
| `tinim-wgpu` | one wgpu pipeline that draws `ClippedPrimitive`s |
| `tinim-gui` | *planned*: `Context`, `Ui`, layout, interaction |
| `tinim-winit` | *planned*: `WindowEvent` → `RawInput` |

`tinim-paint` knows nothing about wgpu. `tinim-wgpu` knows nothing about cosmic-text. Both facts are
load-bearing.

