//! Flow layout state and helpers shared by the UI and its containers.

use tinim_paint::{Pos2, Rect, Vec2};

use crate::input::RawInput;

/// The main axis of a flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Axis {
    Vertical,
    Horizontal,
}

/// The direction in which a flow places its children.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Direction {
    Vertical,
    VerticalReverse,
    Horizontal,
    HorizontalReverse,
}

impl Direction {
    pub(super) fn axis(self) -> Axis {
        match self {
            Self::Vertical | Self::VerticalReverse => Axis::Vertical,
            Self::Horizontal | Self::HorizontalReverse => Axis::Horizontal,
        }
    }

    pub(super) fn is_reverse(self) -> bool {
        matches!(self, Self::VerticalReverse | Self::HorizontalReverse)
    }

    pub(super) fn is_forward(self) -> bool {
        !self.is_reverse()
    }
}

/// Cross-axis alignment for children of a flow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    /// Place children at the start of the cross axis.
    #[default]
    Start,
    /// Center children on the cross axis.
    Center,
    /// Place children at the end of the cross axis.
    End,
    /// Stretch children to the cross-axis extent of the region.
    Stretch,
}

/// Direction and cross-axis alignment of a flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlowConfig {
    pub(super) direction: Direction,
    pub(super) align: Align,
}

impl FlowConfig {
    /// Top-to-bottom flow, aligned at the start of the cross axis.
    pub fn vertical() -> Self {
        Self {
            direction: Direction::Vertical,
            align: Align::Start,
        }
    }

    /// Left-to-right flow, aligned at the start of the cross axis.
    pub fn horizontal() -> Self {
        Self {
            direction: Direction::Horizontal,
            align: Align::Start,
        }
    }

    /// Bottom-to-top flow, aligned at the start of the cross axis.
    ///
    /// Use this with [`Ui::sized_flow`](crate::Ui::sized_flow): a reverse flow needs a known
    /// extent to anchor its first child.
    pub fn bottom_to_top() -> Self {
        Self {
            direction: Direction::VerticalReverse,
            align: Align::Start,
        }
    }

    /// Right-to-left flow, aligned at the start of the cross axis.
    ///
    /// Use this with [`Ui::sized_flow`](crate::Ui::sized_flow): a reverse flow needs a known
    /// extent to anchor its first child.
    pub fn right_to_left() -> Self {
        Self {
            direction: Direction::HorizontalReverse,
            align: Align::Start,
        }
    }

    /// Sets the cross-axis alignment.
    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }
}

impl Default for FlowConfig {
    fn default() -> Self {
        Self::vertical()
    }
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
    ///
    /// For forward flows this is the far edge of the last child. For reverse flows it is the near
    /// edge, because the cursor moves towards the region origin.
    pub(super) cursor: Pos2,
    /// Union of the rectangles allocated by children.
    pub(super) used: Rect,
    /// Whether a child has been placed. The next child is separated from the last one by the
    /// spacing in effect when it is placed, not when its predecessor was placed.
    pub(super) has_children: bool,
    pub(super) direction: Direction,
    pub(super) align: Align,
}

impl Layout {
    pub(super) fn new(rect: Rect, origin: Pos2, config: FlowConfig) -> Self {
        let cursor = match config.direction {
            Direction::Vertical => origin,
            Direction::VerticalReverse => Pos2::new(origin.x, rect.max.y),
            Direction::Horizontal => origin,
            Direction::HorizontalReverse => Pos2::new(rect.max.x, origin.y),
        };
        Self {
            rect,
            origin,
            cursor,
            used: Rect::NOTHING,
            has_children: false,
            direction: config.direction,
            align: config.align,
        }
    }
}

pub(super) fn sanitize_size(size: Vec2) -> Vec2 {
    if size.is_finite() {
        Vec2::new(size.x.max(0.0), size.y.max(0.0))
    } else {
        Vec2::ZERO
    }
}

/// Clamps a single extent to a finite, non-negative value.
pub(super) fn sanitize_extent(extent: f32) -> f32 {
    if extent.is_finite() {
        extent.max(0.0)
    } else {
        0.0
    }
}

/// Start of the next child region, including the gap from the previous child if there is one.
pub(super) fn next_pos(layout: &Layout, spacing: f32) -> Pos2 {
    let gap = if layout.has_children {
        spacing.max(0.0)
    } else {
        0.0
    };
    match layout.direction {
        Direction::Vertical => Pos2::new(layout.origin.x, layout.cursor.y + gap),
        Direction::VerticalReverse => Pos2::new(layout.origin.x, layout.cursor.y - gap),
        Direction::Horizontal => Pos2::new(layout.cursor.x + gap, layout.origin.y),
        Direction::HorizontalReverse => Pos2::new(layout.cursor.x - gap, layout.origin.y),
    }
}

pub(super) fn next_rect(layout: &Layout, size: Vec2, spacing: f32) -> Rect {
    let gap = if layout.has_children {
        spacing.max(0.0)
    } else {
        0.0
    };

    let mut size = size;
    match (layout.direction.axis(), layout.align) {
        (Axis::Vertical, Align::Stretch) => size.x = layout.rect.width(),
        (Axis::Horizontal, Align::Stretch) => size.y = layout.rect.height(),
        _ => {}
    }

    let (main, cross) = match layout.direction.axis() {
        Axis::Vertical => (size.y, size.x),
        Axis::Horizontal => (size.x, size.y),
    };
    let (cursor_main, cross_min, cross_extent) = match layout.direction.axis() {
        Axis::Vertical => (layout.cursor.y, layout.origin.x, layout.rect.width()),
        Axis::Horizontal => (layout.cursor.x, layout.origin.y, layout.rect.height()),
    };

    let main_start = if layout.direction.is_reverse() {
        cursor_main - gap - main
    } else {
        cursor_main + gap
    };
    let cross_start = match layout.align {
        Align::Start | Align::Stretch => cross_min,
        Align::Center => cross_min + (cross_extent - cross) * 0.5,
        Align::End => cross_min + cross_extent - cross,
    };
    let min = match layout.direction.axis() {
        Axis::Vertical => Pos2::new(cross_start, main_start),
        Axis::Horizontal => Pos2::new(main_start, cross_start),
    };
    Rect::from_min_size(min, size)
}

pub(super) fn allocate_space(layout: &mut Layout, size: Vec2, spacing: f32) -> Rect {
    let size = sanitize_size(size);
    let rect = next_rect(layout, size, spacing);

    match layout.direction {
        Direction::Vertical => layout.cursor.y = rect.max.y,
        Direction::VerticalReverse => layout.cursor.y = rect.min.y,
        Direction::Horizontal => layout.cursor.x = rect.max.x,
        Direction::HorizontalReverse => layout.cursor.x = rect.min.x,
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
    match layout.direction.axis() {
        Axis::Vertical => Vec2::new(
            layout.rect.width(),
            if layout.direction.is_reverse() {
                (layout.cursor.y - layout.rect.min.y - gap).max(0.0)
            } else {
                (layout.rect.max.y - layout.cursor.y - gap).max(0.0)
            },
        ),
        Axis::Horizontal => Vec2::new(
            if layout.direction.is_reverse() {
                (layout.cursor.x - layout.rect.min.x - gap).max(0.0)
            } else {
                (layout.rect.max.x - layout.cursor.x - gap).max(0.0)
            },
            layout.rect.height(),
        ),
    }
}

pub(super) fn pointer_over(clip: Rect, input: &RawInput, rect: Rect) -> bool {
    input
        .pointer
        .is_some_and(|pointer| rect.contains(pointer) && clip.contains(pointer))
}
