# tinim-gui

A tiny immediate-mode GUI.

## Crates

| crate | what it does |
|---|---|
| `tinim-paint` | math, colors, `Painter`, `PaintList`, tessellation |
| `tinim-text` | shaping, bidi, line breaking and a glyph atlas (harfrust + skrifa) |
| `tinim-wgpu` | one wgpu pipeline that draws a `PaintList` |
| `tinim-gui` | immediate-mode `Context`/`Ui`, vertical/horizontal layout, labels, checkboxes and press-only buttons |
| `tinim-winit` | `WindowEvent` → `RawInput` input adapter |
| `tinim-demo` | shared demo UI and the embedded Comic Mono font |
| `tinim-demo-native` | native winit + wgpu demo binary |
| `tinim-demo-web` | WebAssembly demo (WebGPU, with WebGL fallback) |

`tinim-paint` knows nothing about wgpu. `tinim-wgpu` knows nothing about font shaping. Both facts
are load-bearing.

## Demos

Run the native demo:

```sh
cargo run -p tinim-demo-native
```

Run the web demo with [wasm-pack](https://rustwasm.github.io/wasm-pack/):

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
cd crates/tinim-demo-web
wasm-pack build --target web
python3 -m http.server 8080
```

Then open <http://localhost:8080>. The repository's `index.html` loads the generated
`pkg/tinim_demo_web.js`.

The web demo uses WebGPU when the browser provides it and falls back to WebGL otherwise.
The fallback is wgpu's `webgl` feature, so applications opt in by enabling it on their own
`wgpu` dependency:

```toml
wgpu = { version = "30", features = ["webgl"] }
```

## Development

On Linux, `winit` does not compile when no platform backend is enabled, which breaks
`cargo check` and `cargo doc` for `tinim-winit` on its own. `tinim-winit` therefore enables
winit's `x11` backend through its default `x11` feature, so both commands work without extra
flags.

Applications that depend on winit themselves (with its default features) are unaffected by this
feature. If you select winit's backend yourself, disable the default feature with
`default-features = false`.

Recheck whether this is still necessary when new version of rust or winit it released.