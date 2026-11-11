//! Native winit + wgpu demo for `tinim-gui`.

use tinim_demo::App;
use tinim_demo::winit::event_loop::EventLoop;

fn main() {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
