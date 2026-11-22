//! Flow layout state and helpers shared by the UI and its containers.

use tinim_paint::{Pos2, Rect, Vec2};

use crate::input::RawInput;

/// The main axis of a flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Axis {
    Vertical,
    Horizontal,
}

impl Axis {
    /// The other axis.
    pub(super) fn other(self) -> Self {
        match self {
            Self::Vertical => Self::Horizontal,
            Self::Horizontal => Self::Vertical,
        }
    }

    /// Reads the component along this axis from `value`.
    pub(super) fn value(self, value: Vec2) -> f32 {
        match self {
            Self::Vertical => value.y,
            Self::Horizontal => value.x,
        }
    }

    /// Builds a point from a main-axis and a cross-axis coordinate.
    pub(super) fn point(self, main: f32, cross: f32) -> Pos2 {
        match self {
            Self::Vertical => Pos2::new(cross, main),
            Self::Horizontal => Pos2::new(main, cross),
        }
    }
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

    /// Whether this direction grows toward decreasing coordinates (up or left).
    pub(super) fn is_reverse(self) -> bool {
        matches!(self, Self::VerticalReverse | Self::HorizontalReverse)
    }

    /// Whether this direction grows toward increasing coordinates (down or right).
    ///
    /// This is an absolute property of the axis, not a direction relative to another flow.
    pub(super) fn is_forward(self) -> bool {
        !self.is_reverse()
    }

    /// Whether a content-sized flow with `child` direction can be nested in this direction.
    ///
    /// Content-sized flows need a known near edge. A child can be built when it grows in the
    /// same direction on the same axis, or when both flows are forward. The remaining
    /// combinations need a known extent along the parent's main axis: pass
    /// [`FlowSize::main`](crate::FlowSize::main) for a same-axis flow or
    /// [`FlowSize::cross`](crate::FlowSize::cross) for a perpendicular one, or use
    /// [`Ui::sized_flow`](crate::Ui::sized_flow).
    pub(super) fn can_host_content_flow(self, child: Self) -> bool {
        (self.axis() == child.axis() && self == child) || (self.is_forward() && child.is_forward())
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

impl Align {
    /// Fraction of the free space before the content on this axis.
    ///
    /// [`Align::Stretch`] only makes sense for flow layout, so it is treated like
    /// [`Align::Start`] by two-axis alignment.
    fn fraction(self) -> f32 {
        match self {
            Self::Start | Self::Stretch => 0.0,
            Self::Center => 0.5,
            Self::End => 1.0,
        }
    }
}

/// Alignment of a rectangle inside another rectangle on both axes.
///
/// This is used where content is painted into an explicitly known rectangle, such as
/// [`Ui::text_in`](crate::Ui::text_in), rather than placed by a flow. [`Align::Stretch`] has no
/// meaning here and behaves like [`Align::Start`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Align2 {
    /// Alignment on the horizontal axis.
    pub x: Align,
    /// Alignment on the vertical axis.
    pub y: Align,
}

impl Align2 {
    /// Left and top aligned.
    pub const LEFT_TOP: Self = Self {
        x: Align::Start,
        y: Align::Start,
    };
    /// Horizontally centered, top aligned.
    pub const CENTER_TOP: Self = Self {
        x: Align::Center,
        y: Align::Start,
    };
    /// Right and top aligned.
    pub const RIGHT_TOP: Self = Self {
        x: Align::End,
        y: Align::Start,
    };
    /// Left aligned, vertically centered.
    pub const LEFT_CENTER: Self = Self {
        x: Align::Start,
        y: Align::Center,
    };
    /// Centered on both axes.
    pub const CENTER: Self = Self {
        x: Align::Center,
        y: Align::Center,
    };
    /// Right aligned, vertically centered.
    pub const RIGHT_CENTER: Self = Self {
        x: Align::End,
        y: Align::Center,
    };
    /// Left and bottom aligned.
    pub const LEFT_BOTTOM: Self = Self {
        x: Align::Start,
        y: Align::End,
    };
    /// Horizontally centered, bottom aligned.
    pub const CENTER_BOTTOM: Self = Self {
        x: Align::Center,
        y: Align::End,
    };
    /// Right and bottom aligned.
    pub const RIGHT_BOTTOM: Self = Self {
        x: Align::End,
        y: Align::End,
    };

    /// Position of a `size`-sized rectangle aligned inside `rect`.
    pub fn anchor(self, rect: Rect, size: Vec2) -> Pos2 {
        rect.min
            + Vec2::new(
                (rect.width() - size.x) * self.x.fraction(),
                (rect.height() - size.y) * self.y.fraction(),
            )
    }
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
    /// A content-sized reverse flow needs a known near edge. Use [`FlowSize::main`] when nesting
    /// it in an opposite-direction flow, or [`Ui::sized_flow`](crate::Ui::sized_flow) when both
    /// extents are known.
    pub fn bottom_to_top() -> Self {
        Self {
            direction: Direction::VerticalReverse,
            align: Align::Start,
        }
    }

    /// Right-to-left flow, aligned at the start of the cross axis.
    ///
    /// A content-sized reverse flow needs a known near edge. Use [`FlowSize::main`] when nesting
    /// it in an opposite-direction flow, or [`Ui::sized_flow`](crate::Ui::sized_flow) when both
    /// extents are known.
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

/// Requested extents of a content-sized flow.
///
/// Both values are measured in the flow's own axes: `main` is the arrangement direction (height
/// for a vertical flow, width for a horizontal flow) and `cross` is the other axis.
///
/// Leaving an axis as `None` keeps it content-sized. A content-sized main axis needs a known near
/// edge; nesting an opposite-direction flow on the same axis therefore requires `main`. Leaving
/// `cross` as `None` inherits the parent's cross-axis extent for layout and alignment while the
/// parent reserves only the measured part.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlowSize {
    /// Extent along the flow's arrangement axis. `None` fills the space left by the parent.
    pub main: Option<f32>,
    /// Extent along the flow's cross axis. `None` inherits the parent's cross-axis extent.
    pub cross: Option<f32>,
}

impl FlowSize {
    /// Both axes are content-sized.
    pub const CONTENT: Self = Self {
        main: None,
        cross: None,
    };

    /// Fixes the arrangement axis; the cross axis stays content-sized.
    pub const fn main(main: f32) -> Self {
        Self {
            main: Some(main),
            cross: None,
        }
    }

    /// Fixes the cross axis; the arrangement axis stays content-sized.
    pub const fn cross(cross: f32) -> Self {
        Self {
            main: None,
            cross: Some(cross),
        }
    }

    /// Fixes both axes.
    pub const fn fixed(main: f32, cross: f32) -> Self {
        Self {
            main: Some(main),
            cross: Some(cross),
        }
    }

    /// Fixes the cross axis, keeping the current main-axis setting.
    pub const fn with_cross(self, cross: f32) -> Self {
        Self {
            cross: Some(cross),
            ..self
        }
    }

    /// Fixes the arrangement axis, keeping the current cross-axis setting.
    pub const fn with_main(self, main: f32) -> Self {
        Self {
            main: Some(main),
            ..self
        }
    }

    /// Converts a physical size to flow-relative extents.
    pub(super) fn from_vec(size: Vec2, config: FlowConfig) -> Self {
        match config.direction.axis() {
            Axis::Vertical => Self::fixed(size.y, size.x),
            Axis::Horizontal => Self::fixed(size.x, size.y),
        }
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
