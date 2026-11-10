//! Shared UI used by the native and web demos.

mod pages;

use web_time::{Duration, Instant};

use tinim_gui::{Color32, Context, Frame, RawInput, TextureId, Vec2};

use pages::{LayoutPage, PanePage, ScrollPage, StylePage, WidgetsPage, draw_placed_page};

/// Comic Mono is the demo's primary and fallback font on every platform.
const FONT: &[u8] = include_bytes!("../assets/ComicMono.ttf");

const DEFAULT_TEXT: Color32 = Color32::from_rgb(230, 230, 230);
const WARM_TEXT: Color32 = Color32::from_rgb(255, 196, 120);
const COOL_TEXT: Color32 = Color32::from_rgb(180, 210, 255);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DemoPage {
    Widgets,
    Placed,
    Layout,
    Pane,
    Scroll,
    Style,
}

/// Demo state and the UI built from it.
pub struct Demo {
    context: Context,

    page: DemoPage,

    widgets_page: WidgetsPage,
    layout_page: LayoutPage,
    pane_page: PanePage,
    scroll_page: ScrollPage,
    style_page: StylePage,

    max_accum_secs: f64,
    last_accum_time: Instant,

    frames_since_last: u32,
    average_fps: f64,

    accum_ui_dt: Duration,
    average_ui_dt: f64,
}

impl Default for Demo {
    fn default() -> Self {
        Self::new()
    }
}

impl Demo {
    /// Creates the demo with the embedded Comic Mono font.
    pub fn new() -> Self {
        let mut context = Context::new(TextureId(1));
        context
            .text_system_mut()
            .add_font(FONT)
            .expect("the embedded Comic Mono font must be valid");
        context.style_mut().font.color = DEFAULT_TEXT;
        context.style_mut().text_edit.text = DEFAULT_TEXT;

        let now = Instant::now();

        Self {
            context,
            page: DemoPage::Widgets,

            widgets_page: WidgetsPage::new(),
            layout_page: LayoutPage::new(),
            pane_page: PanePage::new(),
            scroll_page: ScrollPage::new(),
            style_page: StylePage::new(),

            max_accum_secs: 0.5,
            last_accum_time: now,

            frames_since_last: 0,
            average_fps: 0.0,

            accum_ui_dt: Duration::ZERO,
            average_ui_dt: 0.0,
        }
    }

    /// Builds one frame and returns its paint data.
    pub fn run(&mut self, viewport: Vec2, pixels_per_point: f32, input: RawInput) -> Frame<'_> {
        let start_time = web_time::Instant::now();

        let frame = self.context.run(viewport, pixels_per_point, input, |ui| {
            ui.label("tinim-gui demo");
            let ui_time = format!("{:.2} ms", self.average_ui_dt * 1e3);
            let frame_secs = 1.0 / self.average_fps;
            let frame_time = format!("{:.2} ms | FPS: {:.1}", frame_secs * 1e3, self.average_fps);
            ui.label(&format!("UI: {ui_time} | Frame: {frame_time}"));

            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.page, DemoPage::Widgets, "Widgets");
                ui.selectable_value(&mut self.page, DemoPage::Placed, "Placed");
                ui.selectable_value(&mut self.page, DemoPage::Layout, "Layout");
                ui.selectable_value(&mut self.page, DemoPage::Pane, "Pane");
                ui.selectable_value(&mut self.page, DemoPage::Scroll, "Scroll");
                ui.selectable_value(&mut self.page, DemoPage::Style, "Style");
            });
            match self.page {
                DemoPage::Widgets => self.widgets_page.draw(ui),
                DemoPage::Placed => draw_placed_page(ui),
                DemoPage::Layout => self.layout_page.draw(ui),
                DemoPage::Pane => self.pane_page.draw(ui),
                DemoPage::Scroll => self.scroll_page.draw(ui),
                DemoPage::Style => self.style_page.draw(ui),
            }
        });

        let ui_dt = start_time.elapsed();
        self.accum_ui_dt += ui_dt;

        self.frames_since_last += 1;

        let secs_since_last = start_time
            .saturating_duration_since(self.last_accum_time)
            .as_secs_f64();
        if secs_since_last > self.max_accum_secs {
            let n_frames = self.frames_since_last as f64;
            let fps = n_frames / secs_since_last;
            self.average_fps = fps;

            self.average_ui_dt = self.accum_ui_dt.as_secs_f64() / n_frames;

            self.accum_ui_dt = Duration::ZERO;
            self.frames_since_last = 0;
            self.last_accum_time = start_time;
        }

        frame
    }
}
