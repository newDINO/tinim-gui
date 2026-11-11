//! Native winit + wgpu demo for `tinim-gui`.

use std::sync::Arc;

use tinim_demo::Demo;
use tinim_gui::vec2;
use tinim_wgpu::wgpu;
use tinim_winit::{WinitPlatform, winit};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
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

    demo: Demo,
    platform: WinitPlatform,
    renderer: tinim_wgpu::Renderer,
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

        log::info!("adapter: {:?}", adapter.get_info());

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();

        let surface_config = {
            let surface_caps = surface.get_capabilities(&adapter);
            let surface_format = surface_caps
                .formats
                .iter()
                .find(|f| f.is_srgb())
                .copied()
                .unwrap_or(surface_caps.formats[0]);
            wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: surface_format,
                width: size.width,
                height: size.height,
                present_mode: wgpu::PresentMode::AutoVsync,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![surface_format.add_srgb_suffix()],
                desired_maximum_frame_latency: 2,
                color_space: wgpu::SurfaceColorSpace::Srgb,
            }
        };
        let surface_format = surface_config.view_formats[0];
        surface.configure(&device, &surface_config);

        let demo = Demo::new();
        let platform = WinitPlatform::new(window.scale_factor() as f32);
        let renderer = tinim_wgpu::Renderer::new(&device, &queue, surface_format);

        Self {
            window,
            device,
            queue,
            surface,
            surface_config,
            demo,
            platform,
            renderer,
        }
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
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.surface_config);
                return;
            }
            // Skip this frame; the surface is expected to recover on its own.
            _ => return,
        };
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.surface_config.format.add_srgb_suffix()),
            ..Default::default()
        });

        let pixels_per_point = self.platform.scale_factor();
        let viewport = vec2(
            physical_size.width as f32 / pixels_per_point,
            physical_size.height as f32 / pixels_per_point,
        );

        let input = self.platform.take_input();
        let frame = self.demo.run(viewport, pixels_per_point, input);
        self.platform.apply_output(&self.window, &frame.platform);

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
        if self.inner.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("tinim-gui native demo")
            .with_inner_size(LogicalSize::new(960.0, 720.0));
        let window = event_loop.create_window(attributes).unwrap();
        let window = Arc::new(window);
        self.inner = Some(pollster::block_on(AppInner::new(
            event_loop.owned_display_handle(),
            window,
        )));
        if let Some(inner) = &self.inner {
            inner.window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(inner) = &mut self.inner {
            inner.platform.on_window_event(&event);
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                if let Some(inner) = &mut self.inner {
                    inner.draw();
                    inner.window.request_redraw();
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(inner) = &mut self.inner {
                    inner.resize(size.width, size.height);
                }
            }
            _ => {}
        }
    }
}

fn main() {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
