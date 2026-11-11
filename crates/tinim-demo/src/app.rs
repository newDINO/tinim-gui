//! Shared winit + wgpu application state for the native and web demos.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use tinim_gui::vec2;
use tinim_wgpu::wgpu;
use tinim_winit::WinitPlatform;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

#[cfg(target_arch = "wasm32")]
use std::cell::Cell;
#[cfg(not(target_arch = "wasm32"))]
use winit::dpi::LogicalSize;
#[cfg(target_arch = "wasm32")]
use winit::platform::web::{WindowAttributesExtWebSys, WindowExtWebSys};

use crate::Demo;

/// The winit application shared by the native and web demos.
#[derive(Default)]
pub struct App {
    inner: Rc<RefCell<Option<AppInner>>>,
    /// Shared with the web initialization task so it can clear the flag when it finishes.
    #[cfg(target_arch = "wasm32")]
    initializing: Rc<Cell<bool>>,
}

/// The initialized window, GPU state and demo UI.
pub struct AppInner {
    window: Arc<Window>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,

    demo: Demo,
    platform: WinitPlatform,
    renderer: tinim_wgpu::Renderer,
}

impl App {
    #[cfg(not(target_arch = "wasm32"))]
    fn resume_native(&mut self, event_loop: &ActiveEventLoop) {
        if self.inner.borrow().is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("tinim-gui native demo")
            .with_inner_size(LogicalSize::new(960.0, 720.0));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                log::error!("failed to create the native window: {error}");
                return;
            }
        };
        let size = window.inner_size();
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(
                Box::new(event_loop.owned_display_handle()),
            ));
        let surface = match instance.create_surface(Arc::clone(&window)) {
            Ok(surface) => surface,
            Err(error) => {
                log::error!("failed to create the native surface: {error}");
                return;
            }
        };

        match pollster::block_on(AppInner::new(window, instance, surface, size)) {
            Ok(inner) => {
                let window = Arc::clone(&inner.window);
                *self.inner.borrow_mut() = Some(inner);
                window.request_redraw();
            }
            Err(message) => log::error!("{message}"),
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn resume_web(&mut self, event_loop: &ActiveEventLoop) {
        if self.initializing.get() || self.inner.borrow().is_some() {
            return;
        }

        let Some(canvas) = canvas_element() else {
            log::error!("failed to find the #tinim-canvas element");
            return;
        };
        // The canvas is laid out by CSS to fill the page, so winit must not manage its size.
        let attributes = Window::default_attributes()
            .with_title("tinim-gui web demo")
            .with_canvas(Some(canvas));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                log::error!("failed to create the browser window: {error}");
                return;
            }
        };
        let Some(canvas) = window.canvas() else {
            log::error!("the winit window has no canvas");
            return;
        };
        // `Window::inner_size` is populated by winit's `ResizeObserver`, which may not have run
        // yet. Measuring the canvas CSS box directly gives the correct size immediately.
        let size = canvas_physical_size(&canvas);

        let instance = wgpu::Instance::default();
        let surface = match instance.create_surface(wgpu::SurfaceTarget::Canvas(canvas)) {
            Ok(surface) => surface,
            Err(error) => {
                log::error!("failed to create the browser surface: {error}");
                return;
            }
        };

        self.initializing.set(true);
        let slot = Rc::clone(&self.inner);
        let initializing = Rc::clone(&self.initializing);
        wasm_bindgen_futures::spawn_local(async move {
            match AppInner::new(window, instance, surface, size).await {
                Ok(inner) => {
                    let window = Arc::clone(&inner.window);
                    *slot.borrow_mut() = Some(inner);
                    initializing.set(false);
                    window.request_redraw();
                }
                Err(message) => {
                    initializing.set(false);
                    log::error!("{message}");
                }
            }
        });
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(not(target_arch = "wasm32"))]
        self.resume_native(event_loop);
        #[cfg(target_arch = "wasm32")]
        self.resume_web(event_loop);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let mut slot = self.inner.borrow_mut();
        let Some(inner) = slot.as_mut() else {
            return;
        };

        inner.platform.on_window_event(&event);

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

impl AppInner {
    /// Creates the GPU state and demo UI for `window`.
    ///
    /// `instance` and `surface` are created by the platform entry point, because native builds
    /// need an owned display handle while browsers need the canvas.
    pub async fn new(
        window: Arc<Window>,
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        size: PhysicalSize<u32>,
    ) -> Result<Self, String> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|error| format!("failed to request a GPU adapter: {error}"))?;

        log::info!("adapter: {:?}", adapter.get_info());

        // WebGL2 supports fewer limits than the wgpu defaults (notably no storage buffers), so
        // browsers request exactly what their adapter provides.
        #[cfg(target_arch = "wasm32")]
        let required_limits = adapter.limits();
        #[cfg(not(target_arch = "wasm32"))]
        let required_limits = wgpu::Limits::default();

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_limits,
                ..Default::default()
            })
            .await
            .map_err(|error| format!("failed to create the GPU device: {error}"))?;

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
        let platform = WinitPlatform::new(&window);
        let renderer = tinim_wgpu::Renderer::new(&device, &queue, surface_format);

        Ok(Self {
            window,
            device,
            queue,
            surface,
            surface_config,
            demo,
            platform,
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
        // A resize can happen while the app is still being initialized, in which case the
        // `Resized` event was ignored. Reconcile here so the surface always matches the window.
        if physical_size.width != self.surface_config.width
            || physical_size.height != self.surface_config.height
        {
            self.resize(physical_size.width, physical_size.height);
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

#[cfg(target_arch = "wasm32")]
fn canvas_element() -> Option<web_sys::HtmlCanvasElement> {
    use wasm_bindgen::JsCast;

    web_sys::window()?
        .document()?
        .get_element_by_id("tinim-canvas")?
        .dyn_into()
        .ok()
}

/// The size of the canvas CSS box in physical pixels.
#[cfg(target_arch = "wasm32")]
fn canvas_physical_size(canvas: &web_sys::HtmlCanvasElement) -> PhysicalSize<u32> {
    let scale = web_sys::window()
        .map(|window| window.device_pixel_ratio())
        .unwrap_or(1.0);
    let width = (canvas.client_width() as f64 * scale).round().max(1.0);
    let height = (canvas.client_height() as f64 * scale).round().max(1.0);
    PhysicalSize::new(width as u32, height as u32)
}
