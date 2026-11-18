//! Widget showcase.

use std::ops::{Deref, DerefMut};

use tinim_gui::{
    Align, Color32, Context, CursorIcon, Edges, FlowConfig, FrameStyle, ImageData, ImageFit,
    ImageOptions, NumberInputState, Rect, Response, ScrollState, Stroke, TextEditState,
    TextureHandle, Ui, vec2,
};

use super::page;
use crate::Theme;

pub(crate) struct WidgetsPage {
    scroll: ScrollState,
    icon: TextureHandle,
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
    pub(crate) fn new(context: &mut Context) -> Self {
        Self {
            scroll: ScrollState::new(),
            icon: context.load_texture(plus_icon()),
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

    pub(crate) fn draw(&mut self, ui: &mut Ui, theme: Theme) {
        page(ui, &mut self.scroll, theme, |ui| {
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

            ui.label(
                "A custom button built from interact(), paint_frame(), image_with() and place_at():",
            );
            let icon = icon_button(&mut ui, &self.icon, theme, "Add");
            if icon.pressed {
                self.counter = self.counter.saturating_add(1);
            }
            ui.label(&format!(
                "icon button: hovered: {} | down: {}",
                icon.hovered, icon.down
            ));

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
        });
    }
}

/// A fixed-size button with an icon and a label, built only from public `Ui` APIs.
///
/// `allocate_space` reserves the rectangle in the parent flow, `interact` supplies the pointer
/// state, `paint_frame` paints the state-dependent background before the content, and `place_at`
/// draws the icon and label without advancing the parent flow.
fn icon_button(ui: &mut Ui<'_>, icon: &TextureHandle, theme: Theme, label: &str) -> Response {
    let button = ui.style().button.clone();
    let rect = ui.allocate_space(vec2(128.0, button.min_size.y.max(28.0)));
    let response = ui.interact(rect);
    if response.hovered {
        ui.set_cursor_icon(CursorIcon::Pointer);
    }

    let background = if response.down || response.pressed {
        button.pressed
    } else if response.hovered {
        button.hovered
    } else {
        button.normal
    };
    ui.paint_frame(
        rect,
        FrameStyle {
            background,
            border: Stroke::NONE,
            padding: Edges::ZERO,
            corner_radius: button.corner_radius,
        },
    );

    let inner = Rect::from_min_size(
        rect.min + button.padding,
        (rect.size() - button.padding * 2.0).max(vec2(0.0, 0.0)),
    );
    let tint = match theme {
        Theme::Default => Color32::from_rgb(120, 200, 255),
        Theme::Plain => Color32::from_rgb(0, 117, 255),
    };

    ui.place_at(inner.min, |ui| {
        ui.sized_flow(
            inner.size(),
            FlowConfig::horizontal().align(Align::Center),
            |ui| {
                ui.image_with(
                    icon,
                    ImageOptions::default()
                        .with_size(vec2(16.0, 16.0))
                        .with_fit(ImageFit::Contain)
                        .with_tint(tint),
                );
                ui.with_style(
                    |style| style.text_color = button.text,
                    |ui| {
                        ui.label(label);
                    },
                );
            },
        );
    });

    response
}

/// A white plus sign with a transparent background, tinted by [`icon_button`].
fn plus_icon() -> ImageData {
    const SIZE: u32 = 16;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let horizontal = (6..10).contains(&y) && (2..14).contains(&x);
            let vertical = (2..14).contains(&y) && (6..10).contains(&x);
            let alpha = if horizontal || vertical { 255 } else { 0 };
            rgba.extend_from_slice(&[255, 255, 255, alpha]);
        }
    }
    ImageData::new([SIZE, SIZE], rgba)
}
