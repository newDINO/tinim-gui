//! Small default style shared by the built-in widgets.

use tinim_paint::{Color32, Vec2, vec2};
use tinim_text::TextStyle;

/// Layout and widget styling.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// Default text style for labels.
    pub font: TextStyle,
    /// Space between children in a vertical or horizontal stack.
    pub spacing: f32,
    pub button: ButtonStyle,
    pub checkbox: CheckboxStyle,
    pub radio: RadioStyle,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            font: TextStyle::default(),
            spacing: 8.0,
            button: ButtonStyle::default(),
            checkbox: CheckboxStyle::default(),
            radio: RadioStyle::default(),
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
