//! Widget showcase.

use std::ops::{Deref, DerefMut};

use tinim_gui::{Align, FlowConfig, Ui, vec2};

pub(crate) struct WidgetsPage {
    counter: usize,
    checked: bool,
    radio: usize,
    radio_value: usize,
}

/// A custom wrapper of Ui with [`Self::sized_horizontal`] to
/// create a horizontal region with fixed height.
struct UiC<'a, 'u> {
    ui: &'a mut Ui<'u>,
    sized_horizontal_height: f32,
}
impl<'a, 'u> Deref for UiC<'a, 'u> {
    type Target = Ui<'u>;
    fn deref(&self) -> &Self::Target {
        self.ui
    }
}
impl<'a, 'u> DerefMut for UiC<'a, 'u> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ui
    }
}
impl UiC<'_, '_> {
    fn sized_horizontal(&mut self, f: impl FnOnce(&mut Ui)) {
        self.ui.sized_flow(
            vec2(self.size().x, self.sized_horizontal_height),
            FlowConfig::horizontal().align(Align::End),
            f,
        );
    }
}

impl WidgetsPage {
    pub(crate) fn new() -> Self {
        Self {
            counter: 0,
            checked: false,
            radio: 0,
            radio_value: 0,
        }
    }

    pub(crate) fn draw(&mut self, ui: &mut Ui) {
        let mut ui = UiC {
            sized_horizontal_height: 19.0,
            ui,
        };
        if ui.button("+1").pressed {
            self.counter = self.counter.saturating_add(1);
        }
        if ui.button("-1").pressed {
            self.counter = self.counter.saturating_sub(1);
        }
        ui.label(&format!("Value: {}", self.counter));

        ui.sized_horizontal(|ui| {
            ui.checkbox(&mut self.checked);
            ui.label(if self.checked {
                "checkbox: checked"
            } else {
                "checkbox: unchecked"
            });
        });

        ui.label("radio(selected) is the label-free counterpart of selectable_label:");
        ui.sized_horizontal(|ui| {
            for (index, label) in ["Zero", "One", "Two"].into_iter().enumerate() {
                if ui.radio(self.radio == index).pressed {
                    self.radio = index;
                }
                ui.label(label);
            }
        });

        ui.label("radio_value(&mut value, selected_value) updates the value by itself:");
        ui.sized_horizontal(|ui| {
            for (index, label) in ["Zero", "One", "Two"].into_iter().enumerate() {
                ui.radio_value(&mut self.radio_value, index);
                ui.label(label);
            }
        });

        ui.label(&format!(
            "radio: {}, radio_value: {}",
            self.radio, self.radio_value
        ));
    }
}
