//! Flow layout state and helpers shared by the UI and its containers.

use tinim_paint::{Pos2, Rect, Vec2};

use crate::input::RawInput;

/// A basic vertical/horizontal flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Direction {
    Vertical,
    Horizontal,
}

/// The flow layout state of the region currently being built.
///
/// This is deliberately independent of clipping: a layout container such as `Ui::vertical` or
/// `Ui::horizontal` only changes the flow state, while viewports such as `Ui::pane` also narrow
/// the clip.
pub(super) struct Layout {
    /// Area available to this region.
    pub(super) rect: Rect,
    /// Position that relative coordinates in this region are measured from.
    pub(super) origin: Pos2,
    /// The end of the last child, before the gap to the next one.
    pub(super) cursor: Pos2,
    /// Union of the rectangles allocated by children.
    pub(super) used: Rect,
    /// Whether a child has been placed. The next child is separated from the last one by the
    /// spacing in effect when it is placed, not when its predecessor was placed.
    pub(super) has_children: bool,
    pub(super) direction: Direction,
}

pub(super) fn sanitize_size(size: Vec2) -> Vec2 {
    if size.is_finite() {
        Vec2::new(size.x.max(0.0), size.y.max(0.0))
    } else {
        Vec2::ZERO
    }
}

/// Start of the next child, including the gap from the previous child if there is one.
pub(super) fn next_pos(layout: &Layout, spacing: f32) -> Pos2 {
    let gap = if layout.has_children {
        spacing.max(0.0)
    } else {
        0.0
    };
    match layout.direction {
        Direction::Vertical => Pos2::new(layout.origin.x, layout.cursor.y + gap),
        Direction::Horizontal => Pos2::new(layout.cursor.x + gap, layout.origin.y),
    }
}

pub(super) fn next_rect(layout: &Layout, size: Vec2, spacing: f32) -> Rect {
    Rect::from_min_size(next_pos(layout, spacing), size)
}

pub(super) fn allocate_space(layout: &mut Layout, size: Vec2, spacing: f32) -> Rect {
    let size = sanitize_size(size);
    let rect = next_rect(layout, size, spacing);

    match layout.direction {
        Direction::Vertical => layout.cursor.y = rect.max.y,
        Direction::Horizontal => layout.cursor.x = rect.max.x,
    }
    layout.has_children = true;
    layout.used = if layout.used.is_positive() {
        layout.used.union(rect)
    } else {
        rect
    };
    rect
}

pub(super) fn available_size(layout: &Layout, spacing: f32) -> Vec2 {
    let gap = if layout.has_children {
        spacing.max(0.0)
    } else {
        0.0
    };
    match layout.direction {
        Direction::Vertical => Vec2::new(
            layout.rect.width(),
            (layout.rect.max.y - layout.cursor.y - gap).max(0.0),
        ),
        Direction::Horizontal => Vec2::new(
            (layout.rect.max.x - layout.cursor.x - gap).max(0.0),
            layout.rect.height(),
        ),
    }
}

pub(super) fn pointer_over(clip: Rect, input: &RawInput, rect: Rect) -> bool {
    input
        .pointer
        .is_some_and(|pointer| rect.contains(pointer) && clip.contains(pointer))
}
