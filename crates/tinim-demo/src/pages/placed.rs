//! Absolute and relative placement showcase.

use tinim_gui::{Ui, pos2, vec2};

use super::FRAME_STYLE1;

pub(crate) fn draw(ui: &mut Ui) {
    ui.label(
        "place_at() uses absolute coordinates; place_relative() offsets from the region origin.",
    );
    ui.place_at(pos2(48.0, 150.0), |ui| {
        ui.sized_frame(vec2(260.0, 132.0), FRAME_STYLE1, |ui| {
            ui.label("placed at absolute (48, 150)");
            ui.button("Floating button");
            ui.place_relative(vec2(30.0, 70.0), |ui| {
                ui.label("nested at + (30, 70)");
            });
        });
    });
    ui.place_relative(vec2(320.0, 210.0), |ui| {
        ui.frame(FRAME_STYLE1, |ui| {
            ui.label("placed at origin + (320, 210)");
        });
    });
}
