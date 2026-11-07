//! Scrollable showcase.

use tinim_gui::{Ui, vec2};

pub(crate) struct ScrollPage {
    action: Option<String>,
}

impl ScrollPage {
    pub(crate) fn new() -> Self {
        Self { action: None }
    }

    pub(crate) fn draw(&mut self, ui: &mut Ui) {
        ui.label(
            "scrollable() clips content, scrolls nested areas from the inside out, \
                     and clamps the caller-owned scroll position.",
        );

        let (_, scroll) = tinim_gui::scrollable!(ui, vec2(300.0, 150.0), |ui| {
            ui.label("Wheel over this area to scroll; the inner area scrolls independently.");
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
