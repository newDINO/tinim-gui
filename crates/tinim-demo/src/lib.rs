//! Shared UI used by the native and web demos.

use tinim_gui::{Color32, Context, Frame, RawInput, TextureId, Ui, Vec2, pos2, vec2};

/// Comic Mono is the demo's primary and fallback font on every platform.
const FONT: &[u8] = include_bytes!("../assets/ComicMono.ttf");

const DEFAULT_TEXT: Color32 = Color32::from_rgb(230, 230, 230);
const WARM_TEXT: Color32 = Color32::from_rgb(255, 196, 120);
const COOL_TEXT: Color32 = Color32::from_rgb(180, 210, 255);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DemoPage {
    Buttons,
    Placed,
    Pane,
    Scroll,
    Style,
}

/// Demo state and the UI built from it.
pub struct Demo {
    context: Context,

    page: DemoPage,

    button_page: ButtonsPage,
    pane_page: PanePage,
    scroll_page: ScrollPage,
    style_page: StylePage,

    /// Time spent building the previous frame's UI.
    ui_dt: Option<web_time::Duration>,
    /// Start of the previous frame, used to measure the frame interval.
    last_run: Option<web_time::Instant>,
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

        Self {
            context,
            page: DemoPage::Buttons,

            button_page: ButtonsPage::new(),
            pane_page: PanePage::new(),
            scroll_page: ScrollPage::new(),
            style_page: StylePage::new(),

            ui_dt: None,
            last_run: None,
        }
    }

    /// Builds one frame and returns its paint data.
    pub fn run(&mut self, viewport: Vec2, pixels_per_point: f32, input: RawInput) -> Frame<'_> {
        let start = web_time::Instant::now();
        let frame_dt = self.last_run.map(|last| start.duration_since(last));
        self.last_run = Some(start);

        let frame = self.context.run(viewport, pixels_per_point, input, |ui| {
            ui.label("tinim-gui demo");
            let ui_time = match self.ui_dt {
                Some(ui_dt) => format!("{:.2} ms", ui_dt.as_secs_f64() * 1e3),
                None => "--".to_owned(),
            };
            let frame_time = match frame_dt {
                Some(frame_dt) if frame_dt.as_secs_f64() > 0.0 => {
                    let frame_secs = frame_dt.as_secs_f64();
                    format!("{:.2} ms | FPS: {:.1}", frame_secs * 1e3, 1.0 / frame_secs)
                }
                _ => "-- ms | FPS: --".to_owned(),
            };
            ui.label(&format!("UI: {ui_time} | Frame: {frame_time}"));

            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.page, DemoPage::Buttons, "Buttons");
                ui.selectable_value(&mut self.page, DemoPage::Placed, "Placed");
                ui.selectable_value(&mut self.page, DemoPage::Pane, "Pane");
                ui.selectable_value(&mut self.page, DemoPage::Scroll, "Scroll");
                ui.selectable_value(&mut self.page, DemoPage::Style, "Style");
            });
            match self.page {
                DemoPage::Buttons => self.button_page.draw(ui),
                DemoPage::Placed => draw_placed_page(ui),
                DemoPage::Pane => self.pane_page.draw(ui),
                DemoPage::Scroll => self.scroll_page.draw(ui),
                DemoPage::Style => self.style_page.draw(ui),
            }
        });

        self.ui_dt = Some(start.elapsed());

        frame
    }
}

struct ButtonsPage {
    counter: usize,
}
impl ButtonsPage {
    pub fn new() -> Self {
        Self { counter: 0 }
    }
    pub fn draw(&mut self, ui: &mut Ui) {
        if ui.button("+1").pressed {
            self.counter = self.counter.saturating_add(1);
        }
        if ui.button("-1").pressed {
            self.counter = self.counter.saturating_sub(1);
        }
        ui.label(&format!("Value: {}", self.counter));
    }
}

fn draw_placed_page(ui: &mut Ui) {
    ui.label(
        "place_at() uses absolute coordinates; place_relative() offsets from the region origin.",
    );
    ui.place_at(pos2(48.0, 150.0), |ui| {
        ui.label("placed at absolute (48, 150)");
        ui.button("Floating button");
        ui.place_relative(vec2(10.0, 40.0), |ui| {
            ui.label("nested at + (10, 40)");
        });
    });
    ui.place_relative(vec2(320.0, 210.0), |ui| {
        ui.label("placed at origin + (320, 210)");
    });
}

struct PanePage {
    action: Option<String>,
    pane_offset: Vec2,
}
impl PanePage {
    pub fn new() -> Self {
        Self {
            action: None,
            pane_offset: Vec2::ZERO,
        }
    }
    pub fn draw(&mut self, ui: &mut Ui) {
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
                    self.action = Some(format!("pressed \"Item {index}\""));
                }
            }
        });
        ui.label(&format!(
            "content: {:.0} x {:.0}",
            pane.content_size.x, pane.content_size.y
        ));
        ui.label(&format!(
            "action: {}",
            self.action.as_deref().unwrap_or("(none)")
        ));
    }
}

struct ScrollPage {
    action: Option<String>,
}
impl ScrollPage {
    pub fn new() -> Self {
        Self { action: None }
    }
    pub fn draw(&mut self, ui: &mut Ui) {
        ui.label(
            "scrollable() clips content, scrolls nested areas from the inside out, \
                     and clamps the caller-owned scroll position.",
        );

        let (_, scroll) = tinim_gui::scrollable!(ui, vec2(300.0, 150.0), |ui| {
            ui.label("Wheel over this area, or use the buttons above.");
            for index in 0..6 {
                if ui.button(&format!("Scroll item {index}")).pressed {
                    self.action = Some(format!("pressed \"Scroll item {index}\""));
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
        ui.label(&format!(
            "action: {}",
            self.action.as_deref().unwrap_or("(none)")
        ));
    }
}

struct StylePage {
    compact: bool,
}

impl StylePage {
    fn new() -> Self {
        Self { compact: false }
    }

    fn draw(&mut self, ui: &mut Ui) {
        ui.label("with_style() overrides the style for one scope and restores it afterwards.");

        let spacing = if self.compact { 2.0 } else { 8.0 };
        let scope_label = if self.compact {
            "Compact scope (spacing 2)"
        } else {
            "Roomy scope (spacing 8)"
        };

        ui.with_style(
            |style| {
                style.spacing = spacing;
                style.button.normal = Color32::from_rgb(56, 112, 168);
                style.button.hovered = Color32::from_rgb(72, 136, 196);
                style.button.pressed = Color32::from_rgb(44, 88, 132);
            },
            |ui| {
                ui.label(scope_label);
                ui.button("Scoped button A");
                ui.button("Scoped button B");
            },
        );

        if ui.button("Toggle scoped spacing").pressed {
            self.compact = !self.compact;
        }

        ui.label("Nested scopes restore in order:");
        ui.with_style(
            |style| style.button.normal = Color32::from_rgb(160, 48, 48),
            |ui| {
                ui.button("Outer scope: red");
                ui.with_style(
                    |style| style.button.normal = Color32::from_rgb(48, 96, 160),
                    |ui| {
                        ui.button("Inner scope: blue");
                    },
                );
                ui.button("Outer scope: red again");
            },
        );

        ui.label("style_mut() changes the style persistently, including future frames:");
        ui.horizontal(|ui| {
            if ui.button("Warm text").pressed {
                ui.style_mut().font.color = WARM_TEXT;
            }
            if ui.button("Cool text").pressed {
                ui.style_mut().font.color = COOL_TEXT;
            }
            if ui.button("Default text").pressed {
                ui.style_mut().font.color = DEFAULT_TEXT;
            }
        });
        ui.label("This label uses the persistent style set above.");
    }
}
