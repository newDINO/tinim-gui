//! Widget showcase.

use tinim_gui::Ui;

pub(crate) struct WidgetsPage {
    counter: usize,
    checked: bool,
    radio: usize,
    radio_value: usize,
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
        if ui.button("+1").pressed {
            self.counter = self.counter.saturating_add(1);
        }
        if ui.button("-1").pressed {
            self.counter = self.counter.saturating_sub(1);
        }
        ui.label(&format!("Value: {}", self.counter));

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.checked);
            ui.label(if self.checked {
                "checkbox: checked"
            } else {
                "checkbox: unchecked"
            });
        });

        ui.label("radio(selected) is the label-free counterpart of selectable_label:");
        ui.horizontal(|ui| {
            for (index, label) in ["Zero", "One", "Two"].into_iter().enumerate() {
                if ui.radio(self.radio == index).pressed {
                    self.radio = index;
                }
                ui.label(label);
            }
        });

        ui.label("radio_value(&mut value, selected_value) updates the value by itself:");
        ui.horizontal(|ui| {
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
