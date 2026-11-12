# tinim-gui

A tiny immediate-mode GUI.

## Crates

| crate | what it does |
|---|---|
| `tinim-paint` | math, colors, `Painter`, `PaintList`, tessellation |
| `tinim-text` | shaping, bidi, line breaking and glyph rasterization (harfrust + skrifa) |
| `tinim-paint-text` | glyph atlas and mesh generation for `tinim-text` on top of `tinim-paint` |
| `tinim-wgpu` | one wgpu pipeline that draws a `PaintList` |
| `tinim-gui` | immediate-mode `Context`/`Ui`, vertical/horizontal layout, rectangular frames with background/border/padding, labels, separators, checkboxes, radios, sliders, press-only buttons, single-line text editing with clipboard shortcuts and IME composition, and numeric inputs |
| `tinim-winit` | `WindowEvent` → `RawInput` input adapter, including clipboard and native IME state |
| `tinim-demo` | shared demo UI, application loop and the embedded Comic Mono font |
| `tinim-demo-native` | native winit + wgpu demo binary |
| `tinim-demo-web` | WebAssembly demo (WebGPU, with WebGL fallback) |

`tinim-paint` knows nothing about wgpu. `tinim-wgpu` knows nothing about font shaping.
`tinim-text` knows nothing about textures, atlases or meshes; `tinim-paint-text` is the only
crate that turns laid-out glyphs into paint data. All of these facts are load-bearing.

## Demos

Run the native demo:

```sh
cargo run -p tinim-demo-native
```

Both demos log through the `log` facade. The native demo initializes `env_logger` with an
`info` default, so `RUST_LOG=debug cargo run -p tinim-demo-native` enables more detail. The web
demo initializes `console_log` and writes to the browser console.

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

The web demo uses the browser Clipboard API for Ctrl/Cmd+C, X and V. Browsers may ask for
clipboard permission, and the API requires a secure context such as HTTPS or localhost.

The web demo uses WebGPU when the browser provides it and falls back to WebGL otherwise.
`tinim-demo` enables wgpu's `webgl` feature for wasm builds. Applications using `tinim-wgpu`
directly opt in by enabling it on their own `wgpu` dependency:

```toml
wgpu = { version = "30", features = ["webgl"] }
```

### IME and mobile keyboards

Native builds forward IME composition and commit events, and place the candidate window at the
focused text editor's caret. This works on the desktop platforms supported by winit.

The web backend of winit 0.30 does not implement `set_ime_allowed` or `set_ime_cursor_area`, and
it does not emit `WindowEvent::Ime`. As a result, the canvas cannot request a mobile soft keyboard
and IME composition is not forwarded to the GUI. Browsers only open the soft keyboard for a
focused editable DOM element, so this cannot be fixed by the canvas alone.

eframe/egui works around this by maintaining a hidden DOM `<input>` element, moving it to the
caret, focusing it on a user gesture, and forwarding `input`, composition and keyboard events to
egui. That is a browser-specific bridge rather than a winit feature. `tinim-gui` does not implement
such a bridge yet, so the web demo has no mobile keyboard or IME input. Applications that need it
must add a DOM bridge of their own.

Native builds load system fonts as fallback, so IME composition strings outside the embedded
font's coverage can still be displayed.

## Development

On Linux, `winit` does not compile when no platform backend is enabled, which breaks
`cargo check` and `cargo doc` for `tinim-winit` on its own. `tinim-winit` therefore enables
winit's `x11` backend through its default `x11` feature, so both commands work without extra
flags.

Applications that depend on winit themselves (with its default features) are unaffected by this
feature. If you select winit's backend yourself, disable the default feature with
`default-features = false`.

`tinim-winit`'s optional `clipboard` feature uses `copypasta` with its default features
disabled. On Linux, `x11` enables copypasta's X11 backend and `wayland` enables its Wayland
backend, so X11-only builds do not pull in copypasta's Wayland dependencies and Wayland-only
builds do not pull in X11. The `wayland` feature also enables winit's Wayland backend and uses
the window's raw display handle to create the Wayland clipboard. The backend is selected at
runtime from that display handle, so enabling both features works on X11 and Wayland sessions.

For a Wayland-only application, disable the default features and select `wayland` explicitly.
Use `wayland-dlopen` instead of `wayland` to load libwayland at runtime rather than linking it
at build time:

```toml
tinim-winit = { version = "0.1.0-alpha.1", default-features = false, features = ["clipboard", "wayland-dlopen"] }
```

On Linux, enabling `clipboard` without either backend feature compiles but leaves `Clipboard`
as a no-op.

To try the native demo with the Wayland clipboard, enable its `wayland-dlopen` feature, which
forwards to `tinim-demo` and then to `tinim-winit`:

```sh
cargo run -p tinim-demo-native --features wayland-dlopen
```

Recheck whether this is still necessary when new version of rust or winit it released.
