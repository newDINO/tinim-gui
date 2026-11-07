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
use crate::style::Style;

use layout::{Direction, Region, allocate_space, available_size, next_pos};

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
                has_children: false,
                direction: Direction::Vertical,
            }],
            scroll_remaining,
        }
    }

    /// The style used by subsequently built widgets.
    pub fn style(&self) -> &Style {
        self.ctx.style()
    }

    /// Mutable access to the style used by subsequently built widgets.
    ///
    /// Changes persist beyond the current frame because the style lives on [`Context`]. Use
    /// [`Ui::with_style`] for temporary overrides.
    pub fn style_mut(&mut self) -> &mut Style {
        self.ctx.style_mut()
    }

    /// Runs `build` with a temporary style change.
    ///
    /// `change_style` receives the current style, which is restored when `build` returns. Changes
    /// apply to widgets and layout built inside `build`, including [`Style::spacing`], and to
    /// nested [`Ui::with_style`] scopes. Widgets built before the call are not affected.
    ///
    /// The style lives on the [`Context`], so it is restored on normal return but not if `build`
    /// panics.
    pub fn with_style<R>(
        &mut self,
        change_style: impl FnOnce(&mut Style),
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> R {
        let previous = self.ctx.style().clone();
        change_style(self.ctx.style_mut());
        let result = build(self);
        *self.ctx.style_mut() = previous;
        result
    }

    /// Reserves a rectangle in the current stack and advances the cursor.
    pub fn allocate_space(&mut self, size: Vec2) -> Rect {
        let spacing = self.spacing();
        allocate_space(&mut self.regions, size, spacing)
    }

    /// Space between children in the current style, clamped to a non-negative value.
    fn spacing(&self) -> f32 {
        self.ctx.style().spacing.max(0.0)
    }

    /// Space left in the current stack.
    pub fn available_size(&self) -> Vec2 {
        available_size(&self.regions, self.spacing())
    }

    fn with_region<R>(&mut self, direction: Direction, build: impl FnOnce(&mut Ui<'_>) -> R) -> R {
        let (start, rect) = {
            let start = next_pos(&self.regions, self.spacing());
            let parent = self.regions.last().expect("root region");
            (start, Rect::from_min_max(start, parent.rect.max))
        };
        let (result, used) = self.region_at(rect, start, direction, build);

        if used.is_positive() {
            let parent = self.regions.last_mut().expect("root region");
            match parent.direction {
                Direction::Vertical => parent.cursor.y = parent.cursor.y.max(used.max.y),
                Direction::Horizontal => parent.cursor.x = parent.cursor.x.max(used.max.x),
            }
            parent.has_children = true;
            parent.used = if parent.used.is_positive() {
                parent.used.union(used)
            } else {
                used
            };
        }

        result
    }

    /// Builds a child region covering `rect`, with relative coordinates measured from `origin`.
    ///
    /// Painting inside `build` is clipped to the region, and the previous painter clip is
    /// restored afterwards. Returns the closure's result and the rectangle used by in-flow
    /// children. The parent's cursor and used rectangle are not touched; callers decide whether
    /// the region participates in the parent's flow.
    fn region_at<R>(
        &mut self,
        rect: Rect,
        origin: Pos2,
        direction: Direction,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> (R, Rect) {
        let clip = self.regions.last().expect("root region").clip;
        let clip = clip.intersect(rect);
        let previous_clip = self.ctx.painter.clip_rect();
        self.ctx.painter.set_clip_rect(clip);

        self.regions.push(Region {
            rect,
            clip,
            origin,
            cursor: origin,
            used: Rect::NOTHING,
            has_children: false,
            direction,
        });

        let result = build(self);
        let child = self.regions.pop().expect("child region");

        self.ctx.painter.set_clip_rect(previous_clip);

        (result, child.used)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_util::context;
    use crate::{Color32, RawInput, vec2};

    #[test]
    fn with_style_restores_the_previous_style() {
        let mut context = context();
        let default = context.style().button.normal;
        let red = Color32::from_rgb(200, 0, 0);
        let mut inside = None;
        let mut after = None;

        let _ = context.run(vec2(200.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.with_style(
                |style| style.button.normal = red,
                |ui| {
                    inside = Some(ui.style().button.normal);
                },
            );
            after = Some(ui.style().button.normal);
        });

        assert_eq!(inside, Some(red));
        assert_eq!(after, Some(default));
        assert_eq!(context.style().button.normal, default);
    }

    #[test]
    fn nested_styles_restore_in_order() {
        let mut context = context();
        let default = context.style().button.normal;
        let red = Color32::from_rgb(200, 0, 0);
        let blue = Color32::from_rgb(0, 0, 200);
        let mut outer = None;
        let mut inner = None;
        let mut restored = None;
        let mut after = None;

        let _ = context.run(vec2(200.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.with_style(
                |style| style.button.normal = red,
                |ui| {
                    outer = Some(ui.style().button.normal);
                    ui.with_style(
                        |style| style.button.normal = blue,
                        |ui| {
                            inner = Some(ui.style().button.normal);
                        },
                    );
                    restored = Some(ui.style().button.normal);
                },
            );
            after = Some(ui.style().button.normal);
        });

        assert_eq!(outer, Some(red));
        assert_eq!(inner, Some(blue));
        assert_eq!(restored, Some(red));
        assert_eq!(after, Some(default));
    }

    #[test]
    fn with_style_spacing_applies_inside_the_scope() {
        let mut context = context();
        let default = context.style().spacing;
        let mut first = Rect::NOTHING;
        let mut second = Rect::NOTHING;
        let mut after = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.with_style(
                |style| style.spacing = 2.0,
                |ui| {
                    first = ui.button("one").rect;
                    second = ui.button("two").rect;
                },
            );
            after = ui.button("three").rect;
        });

        assert_eq!(second.min.y, first.max.y + 2.0);
        assert_eq!(after.min.y, second.max.y + default);
    }

    #[test]
    fn with_style_applies_to_widget_painting() {
        let mut context = context();
        let red = Color32::from_rgb(200, 0, 0);
        let frame = context.run(vec2(200.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.with_style(
                |style| {
                    style.button.normal = red;
                    style.button.corner_radius = 0;
                },
                |ui| {
                    ui.button("x");
                },
            );
        });

        assert!(
            frame
                .paint
                .vertices
                .iter()
                .any(|vertex| vertex.color == red.to_array())
        );
    }

    #[test]
    fn style_mut_persists_for_subsequent_widgets() {
        let mut context = context();
        let mut first = Rect::NOTHING;
        let mut second = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.style_mut().spacing = 3.0;
            first = ui.button("one").rect;
            second = ui.button("two").rect;
        });

        assert_eq!(second.min.y, first.max.y + 3.0);
        assert_eq!(context.style().spacing, 3.0);
    }
}
