//! The per-frame UI builder.
//!
//! v0.1 deliberately has no ids and no persistent widget state. Widgets are rebuilt every frame;
//! text layout is the only cached thing.

mod containers;
mod layout;
mod response;
mod scroll;
mod widgets;

#[cfg(test)]
mod test_util;

pub use response::{PaneResponse, Response, ScrollResponse, ScrollState};

use tinim_paint::{Pos2, Rect, Vec2};

use crate::context::Context;

use layout::{Direction, Region, allocate_space, available_size};

/// Builds one frame's widgets.
///
/// Widgets normally follow the current region's flow, or start at an arbitrary position with
/// [`Ui::place`]. Overlapping widgets are painted in declaration order, but hit testing does not
/// resolve occlusion: a later widget does not suppress an earlier widget's `hovered`/`pressed`.
pub struct Ui<'a> {
    ctx: &'a mut Context,
    regions: Vec<Region>,
    /// Scroll delta not yet consumed by a scrollable.
    scroll_remaining: Vec2,
}

impl<'a> Ui<'a> {
    pub(crate) fn new(ctx: &'a mut Context) -> Self {
        let rect = Rect::from_min_size(Pos2::ZERO, ctx.size());
        let spacing = ctx.style().spacing.max(0.0);
        let scroll_remaining = if ctx.input.scroll.is_finite() {
            ctx.input.scroll
        } else {
            Vec2::ZERO
        };
        Self {
            ctx,
            regions: vec![Region {
                rect,
                clip: rect,
                origin: rect.min,
                cursor: rect.min,
                used: Rect::NOTHING,
                direction: Direction::Vertical,
                spacing,
            }],
            scroll_remaining,
        }
    }

    /// Reserves a rectangle in the current stack and advances the cursor.
    pub fn allocate_space(&mut self, size: Vec2) -> Rect {
        allocate_space(&mut self.regions, size)
    }

    /// Space left in the current stack.
    pub fn available_size(&self) -> Vec2 {
        available_size(&self.regions)
    }

    fn with_region<R>(&mut self, direction: Direction, build: impl FnOnce(&mut Ui<'_>) -> R) -> R {
        let start = self.regions.last().expect("root region").cursor;
        self.with_region_at(direction, start, true, build)
    }

    fn with_region_at<R>(
        &mut self,
        direction: Direction,
        start: Pos2,
        advance_parent: bool,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> R {
        let (max, clip, spacing) = {
            let parent = self.regions.last().expect("root region");
            (parent.rect.max, parent.clip, parent.spacing)
        };
        let rect = Rect::from_min_max(start, max);

        self.regions.push(Region {
            rect,
            clip: clip.intersect(rect),
            origin: start,
            cursor: start,
            used: Rect::NOTHING,
            direction,
            spacing,
        });

        let result = build(self);
        let child = self.regions.pop().expect("child region");

        if advance_parent && child.used.is_positive() {
            let parent = self.regions.last_mut().expect("root region");
            match parent.direction {
                Direction::Vertical => parent.cursor.y += child.used.height() + parent.spacing,
                Direction::Horizontal => parent.cursor.x += child.used.width() + parent.spacing,
            }
            parent.used = if parent.used.is_positive() {
                parent.used.union(child.used)
            } else {
                child.used
            };
        }

        result
    }
}

/// Create a scrollable with thread local state.
#[macro_export]
macro_rules! scrollable {
    ($ui:expr, $size:expr, $closure:expr) => {{
        use core::cell::Cell;
        thread_local! {
            static STATE: Cell<$crate::ScrollState> = Cell::new($crate::ScrollState { offset: $crate::Vec2::ZERO, content_size: None });
        }
        let mut state = STATE.get();
        let result = $ui.scrollable($size, &mut state, $closure);
        STATE.set(state);
        result
    }};
}
