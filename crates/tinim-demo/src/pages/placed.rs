//! Absolute and relative placement showcase.

use tinim_gui::{FlowConfig, ScrollState, Ui, vec2};

use super::page;
use crate::Theme;

pub(crate) struct PlacedPage {
    scroll: ScrollState,
}

impl PlacedPage {
    pub(crate) fn new() -> Self {
        Self {
            scroll: ScrollState::new(),
        }
    }

    pub(crate) fn draw(&mut self, ui: &mut Ui, theme: Theme) {
        page(ui, &mut self.scroll, theme, |ui| {
            ui.label(
                "place_at() uses absolute coordinates; place_relative() offsets from the region origin.",
            );

            // Out-of-flow content does not advance the cursor, so reserve a box for it.
            ui.sized_flow(vec2(600.0, 340.0), FlowConfig::vertical(), |ui| {
                // Absolute placement follows the page scroll by starting from the region origin.
                let origin = ui.origin();
                ui.place_at(origin + vec2(48.0, 40.0), |ui| {
                    ui.sized_frame(vec2(260.0, 132.0), theme.page_frame(), |ui| {
                        ui.label("placed at origin + (48, 40)");
                        ui.button("Floating button");
                        ui.place_relative(vec2(30.0, 70.0), |ui| {
                            ui.label("nested at + (30, 70)");
                        });
                    });
                });
                ui.place_relative(vec2(320.0, 210.0), |ui| {
                    ui.frame(theme.page_frame(), |ui| {
                        ui.label("placed at origin + (320, 210)");
                    });
                });
            });
        });
    }
}
