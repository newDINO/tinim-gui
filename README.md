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

## Development

On Linux, `winit` does not compile when no platform backend is enabled, which breaks
`cargo check` and `cargo doc` for `tinim-winit` on its own. `tinim-winit` therefore enables
winit's `x11` backend through its default `x11` feature, so both commands work without extra
flags.

Applications that depend on winit themselves (with its default features) are unaffected by this
feature. If you select winit's backend yourself, disable the default feature with
`default-features = false`.

Recheck whether this is still necessary when new version of rust or winit it released.