//! Scoped and persistent style showcase.

use tinim_gui::{Color32, Ui};

use crate::{COOL_TEXT, DEFAULT_TEXT, WARM_TEXT};

pub(crate) struct StylePage {
    compact: bool,
}

impl StylePage {
    pub(crate) fn new() -> Self {
        Self { compact: false }
    }

    pub(crate) fn draw(&mut self, ui: &mut Ui) {
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
                ui.style_mut().text_color = WARM_TEXT;
            }
            if ui.button("Cool text").pressed {
                ui.style_mut().text_color = COOL_TEXT;
            }
            if ui.button("Default text").pressed {
                ui.style_mut().text_color = DEFAULT_TEXT;
            }
        });
        ui.label("This label uses the persistent style set above.");
    }
}
