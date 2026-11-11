//! Small default style shared by the built-in widgets.

use tinim_paint::{Color32, Vec2, vec2};
use tinim_text::TextStyle;

/// Layout and widget styling.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// Default layout style for labels.
    pub font: TextStyle,
    /// Color used to draw labels.
    pub text_color: Color32,
    /// Space between children in a vertical or horizontal stack.
    pub spacing: f32,
    pub button: ButtonStyle,
    pub checkbox: CheckboxStyle,
    pub radio: RadioStyle,
    pub slider: SliderStyle,
    pub number_input: NumberInputStyle,
    pub text_edit: TextEditStyle,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            font: TextStyle::default(),
            text_color: Color32::BLACK,
            spacing: 8.0,
            button: ButtonStyle::default(),
            checkbox: CheckboxStyle::default(),
            radio: RadioStyle::default(),
            slider: SliderStyle::default(),
            number_input: NumberInputStyle::default(),
            text_edit: TextEditStyle::default(),
        }
    }
}

/// Appearance of a [`crate::Ui::button`] or [`crate::Ui::selectable_value`].
#[derive(Clone, Debug, PartialEq)]
pub struct ButtonStyle {
    pub normal: Color32,
    pub hovered: Color32,
    pub pressed: Color32,
    /// Fill for the selected item of a [`crate::Ui::selectable_value`].
    pub selected: Color32,
    pub text: Color32,
    pub corner_radius: u8,
    pub min_size: Vec2,
    pub padding: Vec2,
}

impl Default for ButtonStyle {
    fn default() -> Self {
        Self {
            normal: Color32::from_rgb(58, 58, 62),
            hovered: Color32::from_rgb(76, 76, 82),
            pressed: Color32::from_rgb(42, 42, 46),
            selected: Color32::from_rgb(52, 88, 145),
            text: Color32::WHITE,
            corner_radius: 4,
            min_size: vec2(64.0, 28.0),
            padding: vec2(12.0, 6.0),
        }
    }
}

/// Appearance of a [`crate::Ui::checkbox`].
#[derive(Clone, Debug, PartialEq)]
pub struct CheckboxStyle {
    /// Fill when the box is unchecked.
    pub normal: Color32,
    /// Fill when the pointer is over an unchecked box.
    pub hovered: Color32,
    /// Fill while the box is held down.
    pub pressed: Color32,
    /// Fill when the box is checked.
    pub checked: Color32,
    pub border: Color32,
    /// Color of the check mark.
    pub check: Color32,
    pub corner_radius: u8,
    pub size: Vec2,
    pub border_width: f32,
    pub check_width: f32,
}

impl Default for CheckboxStyle {
    fn default() -> Self {
        Self {
            normal: Color32::from_rgb(58, 58, 62),
            hovered: Color32::from_rgb(76, 76, 82),
            pressed: Color32::from_rgb(42, 42, 46),
            checked: Color32::from_rgb(52, 88, 145),
            border: Color32::from_rgb(120, 120, 128),
            check: Color32::WHITE,
            corner_radius: 4,
            size: vec2(20.0, 20.0),
            border_width: 1.5,
            check_width: 2.0,
        }
    }
}

/// Appearance of a [`crate::Ui::radio`].
#[derive(Clone, Debug, PartialEq)]
pub struct RadioStyle {
    /// Fill when the radio is not selected.
    pub normal: Color32,
    /// Fill when the pointer is over an unselected radio.
    pub hovered: Color32,
    /// Fill while the radio is held down.
    pub pressed: Color32,
    /// Fill when the radio is selected.
    pub selected: Color32,
    pub border: Color32,
    /// Color of the inner dot.
    pub dot: Color32,
    /// Outer radius of the radio button.
    pub radius: f32,
    pub border_width: f32,
    pub dot_radius: f32,
}

impl Default for RadioStyle {
    fn default() -> Self {
        Self {
            normal: Color32::from_rgb(58, 58, 62),
            hovered: Color32::from_rgb(76, 76, 82),
            pressed: Color32::from_rgb(42, 42, 46),
            selected: Color32::from_rgb(52, 88, 145),
            border: Color32::from_rgb(120, 120, 128),
            dot: Color32::WHITE,
            radius: 10.0,
            border_width: 1.5,
            dot_radius: 4.0,
        }
    }
}

/// Appearance of a [`crate::Ui::slider`].
#[derive(Clone, Debug, PartialEq)]
pub struct SliderStyle {
    /// Fill of the unfilled part of the track.
    pub track: Color32,
    /// Fill of the track up to the handle.
    pub filled: Color32,
    /// Fill of the handle when it is neither hovered nor held.
    pub handle: Color32,
    /// Fill of the handle while the pointer is over the slider.
    pub handle_hovered: Color32,
    /// Fill of the handle while the pointer is held on the slider.
    pub handle_active: Color32,
    /// Height of the track.
    pub track_height: f32,
    /// Radius of the handle.
    pub handle_radius: f32,
    /// Minimum size. The width is also capped by the available space.
    pub min_size: Vec2,
}

impl Default for SliderStyle {
    fn default() -> Self {
        Self {
            track: Color32::from_rgb(38, 38, 42),
            filled: Color32::from_rgb(52, 88, 145),
            handle: Color32::from_rgb(200, 200, 208),
            handle_hovered: Color32::WHITE,
            handle_active: Color32::from_rgb(180, 210, 255),
            track_height: 6.0,
            handle_radius: 9.0,
            min_size: vec2(180.0, 24.0),
        }
    }
}

/// Appearance of a [`crate::Ui::number_input`].
///
/// Colors, padding and the caret come from [`Style::text_edit`]. Only the desired size is kept
/// separate so a compact numeric field does not inherit the wider text editor.
#[derive(Clone, Debug, PartialEq)]
pub struct NumberInputStyle {
    /// Desired size. The width is also capped by the available space.
    pub min_size: Vec2,
}

impl Default for NumberInputStyle {
    fn default() -> Self {
        Self {
            min_size: vec2(96.0, 28.0),
        }
    }
}

/// Appearance of a [`crate::Ui::text_edit_singleline`].
#[derive(Clone, Debug, PartialEq)]
pub struct TextEditStyle {
    /// Fill when the editor is neither hovered nor focused.
    pub normal: Color32,
    /// Fill when the pointer is over the editor.
    pub hovered: Color32,
    /// Fill while the editor has keyboard focus.
    pub focused: Color32,
    pub border: Color32,
    pub border_width: f32,
    pub corner_radius: u8,
    /// Space between the border and the text.
    pub padding: Vec2,
    /// Color of the text and the caret.
    pub text: Color32,
    /// Color of the selection highlight.
    pub selection: Color32,
    pub cursor: Color32,
    pub cursor_width: f32,
    /// Desired size. The width is also capped by the available space.
    pub min_size: Vec2,
}

impl Default for TextEditStyle {
    fn default() -> Self {
        Self {
            normal: Color32::from_rgb(38, 38, 42),
            hovered: Color32::from_rgb(46, 46, 52),
            focused: Color32::from_rgb(34, 34, 38),
            border: Color32::from_rgb(120, 120, 128),
            border_width: 1.0,
            corner_radius: 4,
            padding: vec2(8.0, 4.0),
            text: Color32::WHITE,
            selection: Color32::from_rgb(52, 88, 145),
            cursor: Color32::WHITE,
            cursor_width: 1.5,
            min_size: vec2(240.0, 28.0),
        }
    }
}
