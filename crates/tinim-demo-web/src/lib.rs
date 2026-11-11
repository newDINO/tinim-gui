#![cfg(target_arch = "wasm32")]

//! WebAssembly entry point for the `tinim-gui` demo.
//!
//! `index.html` provides the full-page `#tinim-canvas` element that winit renders into. Build
//! with `wasm-pack build --target web`; the page loads the generated `pkg/tinim_demo_web.js`.

use tinim_demo::App;
use tinim_demo::winit::event_loop::EventLoop;
use tinim_demo::winit::platform::web::EventLoopExtWebSys;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).expect("failed to initialize the logger");
    let event_loop = EventLoop::new().expect("failed to create the winit event loop");
    event_loop.spawn_app(App::default());
}
