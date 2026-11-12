//! Small default style shared by the built-in widgets.

use tinim_paint::{Color32, Rect, Stroke, Vec2, vec2};
use tinim_text::TextStyle;

/// Insets for the four sides of a rectangle, in logical points.
///
/// Used for [`FrameStyle::padding`]. Values are clamped to zero when a frame computes its content
/// rectangle; negative insets are not supported.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edges {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Edges {
    pub const ZERO: Self = Self {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };

    /// The same inset on every side.
    pub const fn all(amount: f32) -> Self {
        Self {
            left: amount,
            top: amount,
            right: amount,
            bottom: amount,
        }
    }

    /// `horizontal` on the left and right, `vertical` on the top and bottom.
    pub const fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self {
            left: horizontal,
            top: vertical,
            right: horizontal,
            bottom: vertical,
        }
    }

    /// Insets on the left and right only.
    pub const fn horizontal(amount: f32) -> Self {
        Self {
            left: amount,
            top: 0.0,
            right: amount,
            bottom: 0.0,
        }
    }

    /// Insets on the top and bottom only.
    pub const fn vertical(amount: f32) -> Self {
        Self {
            left: 0.0,
            top: amount,
            right: 0.0,
            bottom: amount,
        }
    }

    /// Combined horizontal and vertical extent.
    pub fn size(self) -> Vec2 {
        vec2(self.left + self.right, self.top + self.bottom)
    }
}

/// Background, border and padding of a rectangular frame.
///
/// The border is drawn inside the rectangle, matching [`Stroke`]. Padding is inside the border.
/// The content rectangle is therefore `rect` shrunk by `border.width` and then by `padding`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameStyle {
    pub background: Color32,
    pub border: Stroke,
    pub padding: Edges,
    /// Corner radius in logical points; rounded after scaling to physical pixels.
    pub corner_radius: u8,
}

impl Default for FrameStyle {
    fn default() -> Self {
        Self {
            background: Color32::TRANSPARENT,
            border: Stroke::NONE,
            padding: Edges::ZERO,
            corner_radius: 0,
        }
    }
}

impl FrameStyle {
    /// Space between the outer rectangle and the content, with the border included.
    pub fn inset(self) -> Edges {
        let border = self.border.width.max(0.0);
        Edges {
            left: self.padding.left.max(0.0) + border,
            top: self.padding.top.max(0.0) + border,
            right: self.padding.right.max(0.0) + border,
            bottom: self.padding.bottom.max(0.0) + border,
        }
    }

    /// The content rectangle after removing the border and padding.
    pub fn content_rect(self, rect: Rect) -> Rect {
        let inset = self.inset();
        let size = (rect.size() - inset.size()).max(Vec2::ZERO);
        Rect::from_min_size(rect.min + vec2(inset.left, inset.top), size)
    }

    /// Whether this style draws anything.
    pub fn is_visible(self) -> bool {
        !self.background.is_invisible() || self.border.is_visible()
    }
}

/// Layout and widget styling.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// Default layout style for labels.
    pub font: TextStyle,
    /// Color used to draw labels.
    pub text_color: Color32,
    /// Space between children in a vertical or horizontal stack.
    pub spacing: f32,
    pub separator: SeparatorStyle,
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
            separator: SeparatorStyle::default(),
            button: ButtonStyle::default(),
            checkbox: CheckboxStyle::default(),
            radio: RadioStyle::default(),
            slider: SliderStyle::default(),
            number_input: NumberInputStyle::default(),
            text_edit: TextEditStyle::default(),
        }
    }
}

/// Appearance of a [`crate::Ui::separator`].
#[derive(Clone, Debug, PartialEq)]
pub struct SeparatorStyle {
    pub color: Color32,
    /// Line thickness in logical points.
    pub thickness: f32,
}

impl Default for SeparatorStyle {
    fn default() -> Self {
        Self {
            color: Color32::from_rgb(100, 100, 108),
            thickness: 1.0,
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

#[cfg(test)]
mod tests {
    use super::*;
    use tinim_paint::{pos2, vec2};

    #[test]
    fn frame_content_rect_removes_border_and_padding() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(100.0, 50.0));
        let style = FrameStyle {
            border: Stroke::new(2.0, Color32::BLACK),
            padding: Edges::symmetric(8.0, 4.0),
            ..FrameStyle::default()
        };

        assert_eq!(
            style.content_rect(rect),
            Rect::from_min_size(pos2(20.0, 26.0), vec2(80.0, 38.0))
        );
    }

    #[test]
    fn frame_content_rect_does_not_become_negative() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0));
        let style = FrameStyle {
            border: Stroke::new(4.0, Color32::BLACK),
            padding: Edges::all(10.0),
            ..FrameStyle::default()
        };

        let content = style.content_rect(rect);
        assert_eq!(content.size(), Vec2::ZERO);
    }

    #[test]
    fn edges_constructors_are_symmetric() {
        assert_eq!(Edges::all(3.0), Edges::symmetric(3.0, 3.0));
        assert_eq!(Edges::horizontal(2.0), Edges::symmetric(2.0, 0.0));
        assert_eq!(Edges::vertical(2.0), Edges::symmetric(0.0, 2.0));
    }
}
