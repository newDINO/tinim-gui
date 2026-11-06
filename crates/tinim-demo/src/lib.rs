//! Shared UI used by the native and web demos.

use tinim_gui::{Color32, Context, Frame, RawInput, TextureId, Ui, Vec2, pos2, vec2};

/// Comic Mono is the demo's primary and fallback font on every platform.
const FONT: &[u8] = include_bytes!("../assets/ComicMono.ttf");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DemoPage {
    Buttons,
    Placed,
    Pane,
    Scroll,
}

/// Demo state and the UI built from it.
pub struct Demo {
    context: Context,
    page: DemoPage,
    pane_offset: Vec2,
    last_action: Option<String>,
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
        context.style_mut().font.color = Color32::from_rgb(230, 230, 230);

        Self {
            context,
            page: DemoPage::Buttons,
            pane_offset: Vec2::ZERO,
            last_action: None,
        }
    }

    /// Builds one frame and returns its paint data.
    pub fn run(&mut self, viewport: Vec2, pixels_per_point: f32, input: RawInput) -> Frame<'_> {
        let Self {
            context,
            page,
            pane_offset,
            last_action,
        } = self;

        context.run(viewport, pixels_per_point, input, |ui| {
            build_ui(ui, page, pane_offset, last_action);
        })
    }
}

fn build_ui(
    ui: &mut Ui<'_>,
    page: &mut DemoPage,
    pane_offset: &mut Vec2,
    last_action: &mut Option<String>,
) {
    ui.vertical(|ui| {
        ui.label("tinim-gui v0.1");
        ui.horizontal(|ui| {
            ui.selectable_value(page, DemoPage::Buttons, "Buttons");
            ui.selectable_value(page, DemoPage::Placed, "Placed");
            ui.selectable_value(page, DemoPage::Pane, "Pane");
            ui.selectable_value(page, DemoPage::Scroll, "Scroll");
        });

        let mut action = None;

        match *page {
            DemoPage::Buttons => {
                ui.label("Buttons report a press edge; there is no persistent widget state.");
                ui.horizontal(|ui| {
                    if ui.button("Press me").pressed {
                        action = Some("pressed \"Press me\"".to_owned());
                    }
                    if ui.button("And me").pressed {
                        action = Some("pressed \"And me\"".to_owned());
                    }
                });
            }
            DemoPage::Placed => {
                ui.label("place() is relative to the current region's content origin.");
                ui.place(pos2(48.0, 150.0), |ui| {
                    ui.label("placed at (48, 150)");
                    if ui.button("Floating button").pressed {
                        action = Some("pressed \"Floating button\"".to_owned());
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
                        pane_offset.x -= 20.0;
                    }
                    if ui.button("x +20").pressed {
                        pane_offset.x += 20.0;
                    }
                    if ui.button("y -20").pressed {
                        pane_offset.y -= 20.0;
                    }
                    if ui.button("y +20").pressed {
                        pane_offset.y += 20.0;
                    }
                    if ui.button("reset").pressed {
                        *pane_offset = Vec2::ZERO;
                    }
                });
                ui.label(&format!(
                    "offset: ({:.0}, {:.0})",
                    pane_offset.x, pane_offset.y
                ));

                let (_, pane) = ui.pane(vec2(300.0, 150.0), *pane_offset, |ui| {
                    ui.label("This content is clipped to a 300 x 150 viewport.");
                    for index in 0..12 {
                        if ui.button(&format!("Item {index}")).pressed {
                            action = Some(format!("pressed \"Item {index}\""));
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
                            action = Some(format!("pressed \"Scroll item {index}\""));
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

        if let Some(action) = action {
            *last_action = Some(action);
        }

        match last_action.as_deref() {
            Some(action) => ui.label(&format!("last action: {action}")),
            None => ui.label("last action: none"),
        };
    });
}
