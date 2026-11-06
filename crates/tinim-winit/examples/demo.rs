//! An integrated window demo for `tinim-gui`.
//!
//! Shows labels, buttons, selectable values, panes and scrollable regions.
//!
//! This example is not run by CI because it needs a display. Run it with:
//!
//! ```text
//! cargo run -p tinim-winit --example demo
//! ```

use std::sync::Arc;

use tinim_gui::{Color32, Context, TextureId, Vec2, pos2, vec2};
use tinim_wgpu::wgpu;
use tinim_winit::{WinitInput, winit};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, OwnedDisplayHandle};
use winit::window::{Window, WindowId};

/// Regular-weight UI fonts to try as the primary face.
///
/// `load_system_fonts` adds faces in filesystem order, so without an explicit primary the first
/// face might be an emoji or a bold/italic variant.
const PRIMARY_FONT_FILES: &[&str] = &[
    "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/liberation-sans/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
    "C:/Windows/Fonts/segoeui.ttf",
    "C:/Windows/Fonts/arial.ttf",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DemoPage {
    Buttons,
    Placed,
    Pane,
    Scroll,
}

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

    context: Context,
    input: WinitInput,
    renderer: tinim_wgpu::Renderer,
    page: DemoPage,
    pane_offset: Vec2,
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
            .find(|format| format.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &surface_config);

        let mut context = Context::new(TextureId(1));
        let preferred = PRIMARY_FONT_FILES
            .iter()
            .any(|path| context.text_system_mut().add_font_file(path).is_ok());
        if !preferred {
            eprintln!(
                "warning: no preferred UI font found; system font order may make the primary face \
                 unsuitable"
            );
        }
        context.text_system_mut().load_system_fonts();
        context.style_mut().font.color = Color32::from_rgb(230, 230, 230);

        let input = WinitInput::new(window.scale_factor() as f32);
        let renderer = tinim_wgpu::Renderer::new(&device, &queue, surface_format);

        Self {
            window,
            device,
            queue,
            surface,
            surface_config,
            context,
            input,
            renderer,
            page: DemoPage::Buttons,
            pane_offset: Vec2::ZERO,
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
        let frame = self.context.run(viewport, pixels_per_point, input, |ui| {
            ui.vertical(|ui| {
                ui.label("tinim-gui v0.1");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.page, DemoPage::Buttons, "Buttons");
                    ui.selectable_value(&mut self.page, DemoPage::Placed, "Placed");
                    ui.selectable_value(&mut self.page, DemoPage::Pane, "Pane");
                    ui.selectable_value(&mut self.page, DemoPage::Scroll, "Scroll");
                });

                match self.page {
                    DemoPage::Buttons => {
                        ui.label(
                            "Buttons report a press edge; there is no persistent widget state.",
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Press me").pressed {
                                println!("pressed: Press me");
                            }
                            if ui.button("And me").pressed {
                                println!("pressed: And me");
                            }
                        });
                    }
                    DemoPage::Placed => {
                        ui.label("place() is relative to the current region's content origin.");
                        ui.place(pos2(48.0, 150.0), |ui| {
                            ui.label("placed at (48, 150)");
                            if ui.button("Floating button").pressed {
                                println!("pressed: Floating button");
                            }
                        });
                        ui.place(pos2(320.0, 210.0), |ui| {
                            ui.label("placed at (320, 210)");
                        });
                    }
                    DemoPage::Pane => {
                        ui.label("pane() clips content to a fixed viewport; offset moves it.");
                        ui.horizontal(|ui| {
                            if ui.button("x -20").pressed {
                                self.pane_offset.x -= 20.0;
                            }
                            if ui.button("x +20").pressed {
                                self.pane_offset.x += 20.0;
                            }
                            if ui.button("y -20").pressed {
                                self.pane_offset.y -= 20.0;
                            }
                            if ui.button("y +20").pressed {
                                self.pane_offset.y += 20.0;
                            }
                            if ui.button("reset").pressed {
                                self.pane_offset = Vec2::ZERO;
                            }
                        });
                        ui.label(&format!(
                            "offset: ({:.0}, {:.0})",
                            self.pane_offset.x, self.pane_offset.y
                        ));

                        let (_, pane) = ui.pane(vec2(300.0, 150.0), self.pane_offset, |ui| {
                            ui.label("This content is clipped to a 300 x 150 viewport.");
                            for index in 0..12 {
                                if ui.button(&format!("Item {index}")).pressed {
                                    println!("pressed: Item {index}");
                                }
                            }
                        });
                        ui.label(&format!(
                            "content: {:.0} x {:.0}",
                            pane.content_size.x, pane.content_size.y
                        ));
                    }
                    DemoPage::Scroll => {
                        ui.label(
                            "scrollable() clips content, scrolls nested areas from the inside out, \
                             and clamps the caller-owned scroll position.",
                        );

                        let (_, scroll) = tinim_gui::scrollable!(ui, vec2(300.0, 150.0), |ui| {
                            ui.label("Wheel over this area, or use the buttons above.");
                            for index in 0..6 {
                                if ui.button(&format!("Scroll item {index}")).pressed {
                                    println!("pressed: Scroll item {index}");
                                }
                            }
                            tinim_gui::scrollable!(ui, vec2(200.0, 100.0), |ui| {
                                for _ in 0..10 {
                                    ui.label("inner scrollable");
                                }
                            });
                            for _ in 0..6 {
                                ui.label("test");
                            }
                        });

                        ui.label(&format!(
                            "content: {:.0} x {:.0}, hovered: {}",
                            scroll.content_size.x, scroll.content_size.y, scroll.hovered
                        ));
                    }
                }
            });
        });

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

        let window = event_loop
            .create_window(Window::default_attributes())
            .unwrap();
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
            inner.input.on_window_event(&event);
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
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
