use std::sync::Arc;

use tinim_paint::{Color32, vec2};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, OwnedDisplayHandle};
use winit::window::{Window, WindowId};

#[derive(Default)]
struct App {
    inner: Option<AppInner>,
}

struct AppInner {
    window: Arc<Window>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,

    painter: tinim_paint::Painter,
    d2_renderer: tinim_wgpu::Renderer,
}

impl AppInner {
    async fn new(display_handle: OwnedDisplayHandle, window: Arc<Window>) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(
            wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(display_handle)),
        );
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &surface_config);

        let painter = tinim_paint::Painter::new(size.width as f32, size.height as f32, 1.0);
        let d2_renderer = tinim_wgpu::Renderer::new(&device, &queue, surface_format);

        Self {
            window,
            device,
            queue,
            surface,
            surface_config,

            painter,
            d2_renderer,
        }
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.surface_config.width = width;
            self.surface_config.height = height;
            self.surface.configure(&self.device, &self.surface_config);
        }
    }
    pub fn draw(&mut self) {
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

        self.painter.clear();
        self.painter.rect(
            tinim_paint::Rect {
                min: vec2(100.0, 100.0),
                max: vec2(300.0, 200.0),
            },
            Color32::from_rgb(127, 127, 127),
        );
        self.painter
            .circle(vec2(100.0, 190.0), 50.0, Color32::from_rgb(255, 255, 0));

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("tinim pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            let paint_list = self.painter.finish();
            self.d2_renderer.render(
                &self.device,
                &self.queue,
                &mut pass,
                tinim_wgpu::ScreenDescriptor::new(
                    self.surface_config.width,
                    self.surface_config.height,
                ),
                paint_list,
            );
        }

        self.queue.submit([encoder.finish()]);
        self.queue.present(output);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = event_loop
            .create_window(Window::default_attributes())
            .unwrap();
        self.inner = Some(pollster::block_on(AppInner::new(
            event_loop.owned_display_handle(),
            Arc::new(window),
        )));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                println!("The close button was pressed; stopping");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                if let Some(inner) = &mut self.inner {
                    inner.draw();
                    inner.window.request_redraw();
                }
            }
            WindowEvent::Resized(new_size) => {
                if let Some(inner) = &mut self.inner {
                    inner.resize(new_size.width, new_size.height);
                }
            }
            _ => (),
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();

    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
