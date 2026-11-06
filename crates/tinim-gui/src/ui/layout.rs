//! Flow layout state and helpers shared by the UI and its containers.

use tinim_paint::{Pos2, Rect, Vec2};

use crate::input::RawInput;

/// A basic vertical/horizontal flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Direction {
    Vertical,
    Horizontal,
}

pub(super) struct Region {
    /// Area available to this stack.
    pub(super) rect: Rect,
    /// Clip inherited from the parent, narrowed to `rect`.
    pub(super) clip: Rect,
    /// Position that relative coordinates in this region are measured from.
    pub(super) origin: Pos2,
    /// Where the next child starts.
    pub(super) cursor: Pos2,
    /// Union of the rectangles allocated by children.
    pub(super) used: Rect,
    pub(super) direction: Direction,
    pub(super) spacing: f32,
}

pub(super) fn sanitize_size(size: Vec2) -> Vec2 {
    if size.is_finite() {
        Vec2::new(size.x.max(0.0), size.y.max(0.0))
    } else {
        Vec2::ZERO
    }
}

pub(super) fn next_rect(regions: &[Region], size: Vec2) -> Rect {
    let region = regions.last().expect("root region");
    match region.direction {
        Direction::Vertical => {
            Rect::from_min_size(Pos2::new(region.origin.x, region.cursor.y), size)
        }
        Direction::Horizontal => {
            Rect::from_min_size(Pos2::new(region.cursor.x, region.origin.y), size)
        }
    }
}

pub(super) fn allocate_space(regions: &mut [Region], size: Vec2) -> Rect {
    let size = sanitize_size(size);
    let rect = next_rect(regions, size);

    let region = regions.last_mut().expect("root region");
    match region.direction {
        Direction::Vertical => region.cursor.y += size.y + region.spacing,
        Direction::Horizontal => region.cursor.x += size.x + region.spacing,
    }
    region.used = if region.used.is_positive() {
        region.used.union(rect)
    } else {
        rect
    };
    rect
}

pub(super) fn available_size(regions: &[Region]) -> Vec2 {
    let region = regions.last().expect("root region");
    match region.direction {
        Direction::Vertical => Vec2::new(
            region.rect.width(),
            (region.rect.max.y - region.cursor.y).max(0.0),
        ),
        Direction::Horizontal => Vec2::new(
            (region.rect.max.x - region.cursor.x).max(0.0),
            region.rect.height(),
        ),
    }
}

pub(super) fn current_clip(regions: &[Region]) -> Rect {
    regions.last().expect("root region").clip
}

pub(super) fn pointer_over(regions: &[Region], input: &RawInput, rect: Rect) -> bool {
    let clip = current_clip(regions);
    input
        .pointer
        .is_some_and(|pointer| rect.contains(pointer) && clip.contains(pointer))
}
