//! Widget showcase.

use tinim_gui::{Align, FlowConfig, TextEditState, Ui, vec2};

pub(crate) struct WidgetsPage {
    counter: usize,
    checked: bool,
    radio: usize,
    radio_value: usize,
    text: String,
    text_edit: TextEditState,
}

trait UiExt {
    fn sized_horizontal(&mut self, height: f32, f: impl FnOnce(&mut Ui));
}

impl UiExt for Ui<'_> {
    fn sized_horizontal(&mut self, height: f32, f: impl FnOnce(&mut Ui)) {
        self.sized_flow(
            vec2(self.size().x, height),
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
            text: "editable text".to_owned(),
            text_edit: TextEditState::new(),
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

        let fixed_height = 19.0;

        ui.sized_horizontal(fixed_height, |ui| {
            ui.checkbox(&mut self.checked);
            ui.label(if self.checked {
                "checkbox: checked"
            } else {
                "checkbox: unchecked"
            });
        });

        ui.label("text_edit_singleline(): click to focus, drag to select, Enter submits:");
        let edit = ui.text_edit_singleline(&mut self.text, &mut self.text_edit);
        ui.label(&format!(
            "text: {:?} | focused: {} | changed: {} | submitted: {}",
            self.text, edit.focused, edit.changed, edit.submitted
        ));

        ui.label("radio(selected) is the label-free counterpart of selectable_label:");
        ui.sized_horizontal(fixed_height, |ui| {
            for (index, label) in ["Zero", "One", "Two"].into_iter().enumerate() {
                if ui.radio(self.radio == index).pressed {
                    self.radio = index;
                }
                ui.label(label);
            }
        });

        ui.label("radio_value(&mut value, selected_value) updates the value by itself:");
        ui.sized_horizontal(fixed_height, |ui| {
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
