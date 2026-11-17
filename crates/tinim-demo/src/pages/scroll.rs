//! Scrollable showcase.

use tinim_gui::{ScrollState, Ui, vec2};

use super::page;
use crate::Theme;

pub(crate) struct ScrollPage {
    scroll: ScrollState,
    action: Option<String>,
}

impl ScrollPage {
    pub(crate) fn new() -> Self {
        Self {
            scroll: ScrollState::new(),
            action: None,
        }
    }

    pub(crate) fn draw(&mut self, ui: &mut Ui, theme: Theme) {
        page(ui, &mut self.scroll, theme, |ui| {
            ui.label(
                "scrollable() clips content, scrolls nested areas from the inside out, \
                 and clamps the caller-owned scroll position.",
            );

            let (scroll, _) = ui.frame(theme.page_frame(), |ui| {
                let (_, scroll) = tinim_gui::scrollable!(ui, vec2(300.0, 150.0), |ui| {
                    ui.label(
                        "Wheel or drag over this area to scroll; the inner area scrolls independently.",
                    );
                    for index in 0..6 {
                        if ui.button(&format!("Scroll item {index}")).pressed {
                            self.action = Some(format!("pressed \"Scroll item {index}\""));
                        }
                    }
                    ui.frame(theme.panel_frame(), |ui| {
                        tinim_gui::scrollable!(ui, vec2(200.0, 100.0), |ui| {
                            for _ in 0..10 {
                                ui.label("inner scrollable");
                            }
                        });
                    });
                    for _ in 0..6 {
                        ui.label("test");
                    }
                });
                scroll
            });

            ui.label(&format!(
                "content: {:.0} x {:.0}, hovered: {}",
                scroll.content_size.x, scroll.content_size.y, scroll.hovered
            ));
            ui.label(&format!(
                "action: {}",
                self.action.as_deref().unwrap_or("(none)")
            ));
        });
    }
}
