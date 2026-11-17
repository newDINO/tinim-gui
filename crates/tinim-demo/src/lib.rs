//! Shared UI and application loop used by the native and web demos.

mod app;
mod pages;
mod theme;

pub use app::{App, AppInner};
pub use tinim_winit::winit;

use web_time::{Duration, Instant};

use tinim_gui::{
    Context, FrameStyle, FullOutput, Pos2, RawInput, Rect, ScrollState, TextureId, Vec2,
};

use pages::{ImagePage, LayoutPage, PanePage, PlacedPage, ScrollPage, StylePage, WidgetsPage};
use theme::Theme;
pub(crate) use theme::{COOL_TEXT, WARM_TEXT};

/// Comic Mono is the demo's primary and fallback font on every platform.
const FONT: &[u8] = include_bytes!("../assets/ComicMono.ttf");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DemoPage {
    Widgets,
    Placed,
    Layout,
    Pane,
    Scroll,
    Style,
    Image,
}

/// Demo state and the UI built from it.
pub struct Demo {
    context: Context,
    theme: Theme,

    page: DemoPage,
    /// Horizontal scroll position of the page selector.
    page_bar: ScrollState,

    widgets_page: WidgetsPage,
    layout_page: LayoutPage,
    placed_page: PlacedPage,
    pane_page: PanePage,
    scroll_page: ScrollPage,
    style_page: StylePage,
    image_page: ImagePage,

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
    ///
    /// Native builds also load system fonts as fallback. Comic Mono stays primary, but it does not
    /// cover every script, so system faces are needed for text such as IME composition strings.
    pub fn new() -> Self {
        let mut context = Context::new(TextureId(1));
        context
            .text_system_mut()
            .add_font(FONT)
            .expect("the embedded Comic Mono font must be valid");
        #[cfg(not(target_arch = "wasm32"))]
        {
            let added = context.text_system_mut().load_system_fonts();
            log::info!("loaded {added} system font faces for fallback");
        }
        let theme = Theme::Default;
        *context.style_mut() = theme.style();

        let image_page = ImagePage::new(&mut context);

        let now = Instant::now();

        Self {
            context,
            theme,
            page: DemoPage::Widgets,
            page_bar: ScrollState::new(),

            widgets_page: WidgetsPage::new(),
            layout_page: LayoutPage::new(),
            placed_page: PlacedPage::new(),
            pane_page: PanePage::new(),
            scroll_page: ScrollPage::new(),
            style_page: StylePage::new(),
            image_page,

            max_accum_secs: 0.5,
            last_accum_time: now,

            frames_since_last: 0,
            average_fps: 0.0,

            accum_ui_dt: Duration::ZERO,
            average_ui_dt: 0.0,
        }
    }

    /// Builds one frame and returns its rendering and platform output.
    pub fn run(
        &mut self,
        viewport: Vec2,
        pixels_per_point: f32,
        input: RawInput,
    ) -> FullOutput<'_> {
        let start_time = web_time::Instant::now();

        // The style page owns the theme selection. Apply it once when it changes so that
        // `style_mut()` changes made elsewhere in the demo are not overwritten every frame.
        let theme = self.style_page.theme();
        if theme != self.theme {
            self.theme = theme;
            *self.context.style_mut() = theme.style();
        }

        // The selector is a single row of buttons. Measure its natural height so the viewport
        // does not clip the buttons, and let it scroll horizontally when the window is narrow.
        let style = self.context.style();
        let page_bar_height = (self.context.text_system().line_height(&style.font)
            + style.button.padding.y * 2.0)
            .max(style.button.min_size.y);

        let frame = self.context.run(viewport, pixels_per_point, input, |ui| {
            ui.paint_frame(
                Rect::from_min_size(Pos2::ZERO, viewport),
                FrameStyle {
                    background: theme.background(),
                    ..FrameStyle::default()
                },
            );

            ui.label("tinim-gui demo");
            let ui_time = format!("{:.2} ms", self.average_ui_dt * 1e3);
            let frame_secs = 1.0 / self.average_fps;
            let frame_time = format!("{:.2} ms | FPS: {:.1}", frame_secs * 1e3, self.average_fps);
            ui.label(&format!("UI: {ui_time} | Frame: {frame_time}"));

            let page_bar_size = Vec2::new(ui.size().x, page_bar_height);
            let _ = ui.scrollable(page_bar_size, &mut self.page_bar, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.page, DemoPage::Widgets, "Widgets");
                    ui.selectable_value(&mut self.page, DemoPage::Placed, "Placed");
                    ui.selectable_value(&mut self.page, DemoPage::Layout, "Layout");
                    ui.selectable_value(&mut self.page, DemoPage::Pane, "Pane");
                    ui.selectable_value(&mut self.page, DemoPage::Scroll, "Scroll");
                    ui.selectable_value(&mut self.page, DemoPage::Style, "Style");
                    ui.selectable_value(&mut self.page, DemoPage::Image, "Image");
                });
            });
            match self.page {
                DemoPage::Widgets => self.widgets_page.draw(ui, theme),
                DemoPage::Placed => self.placed_page.draw(ui, theme),
                DemoPage::Layout => self.layout_page.draw(ui, theme),
                DemoPage::Pane => self.pane_page.draw(ui, theme),
                DemoPage::Scroll => self.scroll_page.draw(ui, theme),
                DemoPage::Style => self.style_page.draw(ui),
                DemoPage::Image => self.image_page.draw(ui, theme),
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
