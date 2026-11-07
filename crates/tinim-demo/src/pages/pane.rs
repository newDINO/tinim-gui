//! Clipped pane showcase.

use tinim_gui::{Ui, Vec2, vec2};

pub(crate) struct PanePage {
    action: Option<String>,
    pane_offset: Vec2,
}

impl PanePage {
    pub(crate) fn new() -> Self {
        Self {
            action: None,
            pane_offset: Vec2::ZERO,
        }
    }

    pub(crate) fn draw(&mut self, ui: &mut Ui) {
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
