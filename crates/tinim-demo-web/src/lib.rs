#![cfg(target_arch = "wasm32")]

//! WebAssembly entry point for the `tinim-gui` demo.
//!
//! Build with `wasm-pack build --target web`; `index.html` in this crate loads the generated
//! `pkg/tinim_demo_web.js`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use tinim_demo::Demo;
use tinim_gui::vec2;
use tinim_wgpu::wgpu;
use tinim_winit::{WinitInput, winit};
use wasm_bindgen::prelude::*;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys, WindowExtWebSys};
use winit::window::{Window, WindowId};

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    let event_loop = EventLoop::new().expect("failed to create the winit event loop");
    event_loop.spawn_app(App::default());
}

#[derive(Default)]
struct App {
    inner: Rc<RefCell<Option<AppInner>>>,
    initializing: bool,
}

struct AppInner {
    window: Arc<Window>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,

    demo: Demo,
    input: WinitInput,
    renderer: tinim_wgpu::Renderer,
}

impl AppInner {
    async fn new(window: Arc<Window>) -> Result<Self, String> {
        let size = {
            let mut size = window.inner_size();
            // Currently this is a hack, because root cause of width and height equals 0 is not found.
            if size.width == 0 {
                size.width = 128;
            }
            if size.height == 0 {
                size.height = 128;
            }
            size
        };

        let instance = wgpu::Instance::default();
        let canvas = window
            .canvas()
            .ok_or_else(|| "the winit window has no canvas".to_owned())?;
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|error| format!("failed to create the browser surface: {error}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|error| format!("failed to request a GPU adapter: {error}"))?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                // WebGL2 supports fewer limits than the wgpu defaults (notably no storage
                // buffers), so request exactly what this adapter provides.
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await
            .map_err(|error| format!("failed to create the GPU device: {error}"))?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|format| format.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps
                .present_modes
                .first()
                .copied()
                .unwrap_or(wgpu::PresentMode::Fifo),
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &surface_config);

        let demo = Demo::new();
        let input = WinitInput::new(window.scale_factor() as f32);
        let renderer = tinim_wgpu::Renderer::new(&device, &queue, surface_format);

        Ok(Self {
            window,
            device,
            queue,
            surface,
            surface_config,
            demo,
            input,
            renderer,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
    }

    fn draw(&mut self) {
        let physical_size = self.window.inner_size();
        if physical_size.width == 0 || physical_size.height == 0 {
            return;
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(surface_texture) => surface_texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(surface_texture) => surface_texture,
            _ => return,
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let pixels_per_point = self.input.scale_factor();
        let viewport = vec2(
            physical_size.width as f32 / pixels_per_point,
            physical_size.height as f32 / pixels_per_point,
        );

        let input = self.input.take();
        let frame = self.demo.run(viewport, pixels_per_point, input);

        self.renderer
            .update_textures(&self.device, &self.queue, &frame.textures);

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("tinim-gui pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.07,
                            g: 0.07,
                            b: 0.09,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            self.renderer.render(
                &self.device,
                &self.queue,
                &mut pass,
                tinim_wgpu::ScreenDescriptor::new(
                    self.surface_config.width,
                    self.surface_config.height,
                ),
                frame.paint,
            );
        }

        self.queue.submit([encoder.finish()]);
        self.queue.present(output);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.initializing || self.inner.borrow().is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("tinim-gui web demo")
            .with_inner_size(LogicalSize::new(960.0, 720.0))
            .with_append(true);
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                log_error(&format!("failed to create the browser window: {error}"));
                return;
            }
        };

        self.initializing = true;
        let slot = Rc::clone(&self.inner);
        wasm_bindgen_futures::spawn_local(async move {
            match AppInner::new(window).await {
                Ok(inner) => {
                    let window = Arc::clone(&inner.window);
                    *slot.borrow_mut() = Some(inner);
                    window.request_redraw();
                }
                Err(message) => log_error(&message),
            }
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let mut slot = self.inner.borrow_mut();
        let Some(inner) = slot.as_mut() else {
            return;
        };

        inner.input.on_window_event(&event);
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                inner.draw();
                inner.window.request_redraw();
            }
            WindowEvent::Resized(size) => inner.resize(size.width, size.height),
            _ => {}
        }
    }
}

fn log_error(message: &str) {
    web_sys::console::error_1(&JsValue::from_str(message));
}
