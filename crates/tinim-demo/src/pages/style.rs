//! Scoped and persistent style showcase.

use tinim_gui::{Color32, ScrollState, Ui, tinim_text::FontFamilies};

use super::page;
use crate::{COOL_TEXT, Theme, WARM_TEXT};

pub(crate) struct StylePage {
    scroll: ScrollState,
    compact: bool,
    theme: Theme,
    font_families: FontFamilies,
}

impl StylePage {
    pub(crate) fn new() -> Self {
        Self {
            scroll: ScrollState::new(),
            compact: false,
            theme: Theme::Default,
            font_families: FontFamilies::new([
                "Verdana",
                "Helvetica",
                "Arial",
                "Noto Sans",
                "DejaVu Sans",
                "Adwaita Sans",
            ]),
        }
    }

    /// The theme selected on this page. The demo applies it to the whole application.
    pub(crate) fn theme(&self) -> Theme {
        self.theme
    }

    pub(crate) fn draw(&mut self, ui: &mut Ui) {
        let theme = self.theme;
        let scroll = &mut self.scroll;
        let compact = &mut self.compact;
        let selected_theme = &mut self.theme;
        let font_families = &self.font_families;

        page(ui, scroll, theme, |ui| {
            ui.label("Themes switch the widget style, page frames and window background together.");
            ui.horizontal(|ui| {
                ui.label("Theme:");
                if ui
                    .selectable_label(*selected_theme == Theme::Default, "Default")
                    .pressed
                {
                    *selected_theme = Theme::Default;
                }
                if ui
                    .selectable_label(*selected_theme == Theme::Plain, "Plain")
                    .pressed
                {
                    *selected_theme = Theme::Plain;
                }
            });
            ui.separator();

            ui.label("with_style() overrides the style for one scope and restores it afterwards.");

            let spacing = if *compact { 2.0 } else { 8.0 };
            let scope_label = if *compact {
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
                    style.button.text = Color32::WHITE;
                },
                |ui| {
                    ui.label(scope_label);
                    ui.button("Scoped button A");
                    ui.button("Scoped button B");
                },
            );

            if ui.button("Toggle scoped spacing").pressed {
                *compact = !*compact;
            }

            ui.separator();

            ui.frame(theme.page_frame(), |ui| {
                ui.label("Nested scopes restore in order:");
                ui.with_style(
                    |style| {
                        style.button.normal = Color32::from_rgb(160, 48, 48);
                        style.button.text = Color32::WHITE;
                    },
                    |ui| {
                        ui.button("Outer scope: red");
                        ui.with_style(
                            |style| {
                                style.button.normal = Color32::from_rgb(48, 96, 160);
                                style.button.text = Color32::WHITE;
                            },
                            |ui| {
                                ui.button("Inner scope: blue");
                            },
                        );
                        ui.button("Outer scope: red again");
                    },
                );
            });

            ui.separator();

            ui.label("style_mut() changes the style persistently, including future frames:");
            ui.horizontal(|ui| {
                if ui.button("Warm text").pressed {
                    ui.style_mut().text_color = WARM_TEXT;
                }
                if ui.button("Cool text").pressed {
                    ui.style_mut().text_color = COOL_TEXT;
                }
                if ui.button("Default text").pressed {
                    ui.style_mut().text_color = theme.text_color();
                }
            });
            ui.label("This label uses the persistent style set above.");

            ui.separator();

            ui.label("Change font");
            ui.with_style(
                |style| {
                    style.font.font_families = font_families.clone();
                },
                |ui| {
                    ui.label("This label may be rendered with sans-serif like fonts on native.");
                },
            );
        });
    }
}
