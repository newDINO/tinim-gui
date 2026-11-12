//! Widget showcase.

use std::ops::{Deref, DerefMut};

use tinim_gui::{Align, FlowConfig, NumberInputState, TextEditState, Ui, vec2};

pub(crate) struct WidgetsPage {
    counter: usize,
    checked: bool,
    radio: usize,
    radio_value: usize,
    text: String,
    text_edit: TextEditState,
    multiline_text: String,
    multiline_edit: TextEditState,
    value: f32,
    number_input: NumberInputState,
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
            text: "editable text".to_owned(),
            text_edit: TextEditState::new(),
            multiline_text: "multi-line text\nwith wrapping\nand Enter".to_owned(),
            multiline_edit: TextEditState::new(),
            value: 42.0,
            number_input: NumberInputState::new(),
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

        ui.label("separator() follows the flow direction:");
        ui.separator();
        ui.sized_horizontal(|ui| {
            ui.label("left");
            ui.separator();
            ui.label("right");
        });

        ui.sized_horizontal(|ui| {
            ui.checkbox(&mut self.checked);
            ui.label(if self.checked {
                "checkbox: checked"
            } else {
                "checkbox: unchecked"
            });
        });

        ui.label(
            "text_edit_singleline(): click to focus, drag to select, Ctrl+C/X/V, Enter submits:",
        );
        let edit = ui.text_edit_singleline(&mut self.text, &mut self.text_edit);
        ui.label(&format!(
            "text: {:?} | focused: {} | changed: {} | submitted: {}",
            self.text, edit.focused, edit.changed, edit.submitted
        ));

        ui.label(
            "text_edit_multiline(): wraps at the editor width, Enter inserts a newline, Up/Down and Home/End move by line:",
        );
        let edit = ui.text_edit_multiline(&mut self.multiline_text, &mut self.multiline_edit);
        ui.label(&format!(
            "multiline: {:?} | focused: {} | changed: {} | scroll_y: {:.1}",
            self.multiline_text,
            edit.focused,
            edit.changed,
            self.multiline_edit.scroll_y()
        ));

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

        ui.label("number_input and slider edit the same value (type a number or drag):");
        let number = ui.number_input(&mut self.value, &mut self.number_input);
        let slider = ui.slider(&mut self.value, 0.0..=100.0);
        ui.label(&format!(
            "value: {:.2} | input focused: {} | changed: {} | submitted: {} | slider hovered: {} | down: {}",
            self.value,
            number.focused,
            number.changed,
            number.submitted,
            slider.hovered,
            slider.down
        ));
    }
}
