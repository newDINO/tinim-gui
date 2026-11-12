//! Flow layout showcase.

use tinim_gui::{Align, FlowConfig, Ui, vec2};

use super::FRAME_STYLE1;

pub(crate) struct LayoutPage {
    align: Align,
}

impl LayoutPage {
    pub(crate) fn new() -> Self {
        Self {
            align: Align::Start,
        }
    }

    pub(crate) fn draw(&mut self, ui: &mut Ui) {
        ui.label(
            "flow() is content-sized and sets direction + cross-axis alignment; \
             sized_flow() gives a reverse flow a fixed box.",
        );

        ui.horizontal(|ui| {
            ui.label("cross-axis alignment:");
            for (align, label) in [
                (Align::Start, "Start"),
                (Align::Center, "Center"),
                (Align::End, "End"),
                (Align::Stretch, "Stretch"),
            ] {
                if ui.selectable_label(self.align == align, label).pressed {
                    self.align = align;
                }
            }
        });

        ui.horizontal(|ui| {
            ui.frame(FRAME_STYLE1, |ui| {
                ui.sized_flow(
                    vec2(220.0, 130.0),
                    FlowConfig::vertical().align(self.align),
                    |ui| {
                        ui.label("flow(vertical)");
                        ui.button("one");
                        ui.button("two");
                    },
                );
            });
            ui.frame(FRAME_STYLE1, |ui| {
                ui.sized_flow(vec2(220.0, 130.0), FlowConfig::bottom_to_top(), |ui| {
                    ui.label("bottom_to_top");
                    ui.button("first");
                    ui.button("second");
                });
            });
        });

        ui.frame(FRAME_STYLE1, |ui| {
            ui.sized_flow(vec2(460.0, 70.0), FlowConfig::right_to_left(), |ui| {
                ui.button("right");
                ui.button("middle");
                ui.button("left");
            });
        });

        ui.label("The parent flow continues after the fixed-size boxes.");
    }
}
