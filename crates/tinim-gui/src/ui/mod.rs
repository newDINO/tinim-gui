//! The per-frame UI builder.
//!
//! v0.1 deliberately has no ids and no persistent widget state. Widgets are rebuilt every frame;
//! text layout is the only cached thing.

mod containers;
mod layout;
mod response;
mod widgets;

#[cfg(test)]
mod test_util;

pub use containers::{
    FrameResponse, PaneResponse, ScrollBarAxis, ScrollBars, ScrollResponse, ScrollState,
};
pub use layout::{Align, Align2, FlowConfig, FlowSize};
pub use response::Response;
pub use widgets::{
    DragValueOptions, DragValueResponse, DragValueState, ImageFit, ImageOptions,
    NumberInputResponse, NumberInputState, TextEditResponse, TextEditState,
};

use tinim_paint::{Pos2, Rect, Shape, Vec2, vec2};

use crate::context::Context;
use crate::cursor::CursorIcon;
use crate::style::{Edges, Style};

use layout::{
    Axis, Direction, Layout, allocate_space, available_size, next_pos, pointer_over,
    sanitize_extent,
};

/// Builds one frame's widgets.
///
/// Widgets normally follow the current region's flow, or start at an absolute position with
/// [`Ui::place_at`] or an offset from the region origin with [`Ui::place_relative`]. Overlapping
/// widgets are painted in declaration order, but hit testing does not resolve occlusion: a later
/// widget does not suppress an earlier widget's `hovered`/`pressed`.
pub struct Ui<'a> {
    ctx: &'a mut Context,
    /// Layout state of the region currently being built. Nested containers swap it out and restore
    /// the parent when they finish.
    layout: Layout,
    /// Clip shared by painting and hit testing. Flow containers leave it untouched; clipped
    /// viewports narrow it for the duration of their `build` closure.
    clip: Rect,
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
            layout: Layout::new(rect, rect.min, FlowConfig::vertical()),
            clip: rect,
            scroll_remaining,
        }
    }

    /// Paint a shape with absolute coordinate.
    pub fn paint_shape(&mut self, shape: Shape) {
        self.ctx.painter.add(shape);
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

    /// Absolute position of the current region's content origin.
    ///
    /// This is the point relative coordinates are measured from. In an ordinary region it is the
    /// region's top-left corner; inside a [`Ui::pane`] it is shifted by the pane's offset. Custom
    /// widgets that paint in absolute screen space can use it to convert local coordinates.
    pub fn origin(&self) -> Pos2 {
        self.layout.origin
    }

    /// Reserves a rectangle in the current region and advances the cursor.
    pub fn allocate_space(&mut self, size: Vec2) -> Rect {
        let spacing = self.spacing();
        allocate_space(&mut self.layout, size, spacing)
    }

    /// Hit-tests an arbitrary rectangle against the pointer.
    ///
    /// `rect` is in absolute logical coordinates, like the rectangles returned by widgets. The
    /// result is clipped to the current region, so the part of `rect` outside a [`Ui::pane`] does
    /// not react. This does not allocate space, advance the cursor or paint anything, which makes
    /// it useful for containers and custom widgets whose rectangle is already known.
    ///
    /// This only reports the pointer state. Interactable regions that want a cursor can call
    /// [`Ui::set_cursor_icon`] while this response is hovered.
    pub fn interact(&self, rect: Rect) -> Response {
        let hovered = pointer_over(self.clip, &self.ctx.input, rect);
        Response {
            rect,
            hovered,
            pressed: hovered && self.ctx.input.primary_pressed,
            down: hovered && self.ctx.input.primary_down,
        }
    }

    /// Requests the mouse cursor icon used while the pointer is over a widget.
    ///
    /// The last request of a frame wins, so later widgets override earlier ones. Widgets only
    /// request an icon while they are hovered; the context resets to
    /// [`CursorIcon::Default`](crate::CursorIcon::Default) at the start of every frame.
    pub fn set_cursor_icon(&mut self, icon: CursorIcon) {
        self.ctx.platform.cursor_icon = icon;
    }

    /// Space between children in the current style, clamped to a non-negative value.
    fn spacing(&self) -> f32 {
        self.ctx.style().spacing.max(0.0)
    }

    /// Size of the current region.
    ///
    /// This is the full extent of the region, independent of how much of it has already been
    /// used. For the remaining space, use [`Ui::available_size`].
    pub fn size(&self) -> Vec2 {
        self.layout.rect.size()
    }

    /// Space left in the current region.
    pub fn available_size(&self) -> Vec2 {
        available_size(&self.layout, self.spacing())
    }

    /// Builds a content-sized region, inset by `inset` and optionally fixed on either axis.
    ///
    /// `size.main` fixes the flow's arrangement axis and `size.cross` fixes the other axis.
    /// `None` keeps an axis content-sized. This changes layout only; the clip is inherited
    /// unchanged.
    ///
    /// The parent flow advances by the rectangle reserved by the children, expanded by `inset`.
    /// Fixed extents are always reserved, so a fixed-width column or a fixed-height row keeps its
    /// space. Content-sized cross axes use only the measured part. The second return value is
    /// that outer rectangle, or a zero-size rectangle at the region's minimum corner when nothing
    /// was placed.
    ///
    /// Reverse parents grow from their minimum corner toward the cursor. A matching reverse child
    /// then anchors at `rect.max`, which is the current cursor rather than the parent's far
    /// corner. A fixed extent on the parent's main axis lets the region anchor its far edge at the
    /// cursor instead, so opposite-direction flows can be nested without measuring them first.
    fn content_region<R>(
        &mut self,
        inset: Edges,
        size: FlowSize,
        config: FlowConfig,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> (R, Rect) {
        let parent_direction = self.layout.direction;
        let parent_axis = parent_direction.axis();
        let child_axis = config.direction.axis();
        let cross_axis = parent_axis.other();

        let start = next_pos(&self.layout, self.spacing());
        let content_start = self.layout.origin + vec2(inset.left, inset.top);
        let content_end =
            (self.layout.rect.max - vec2(inset.right, inset.bottom)).max(content_start);

        // Express the requested extents in the parent's axes. A fixed extent on the parent's
        // main axis is what anchors an opposite-direction child without measuring it first.
        let (main_size, cross_size) = if child_axis == parent_axis {
            (size.main, size.cross)
        } else {
            (size.cross, size.main)
        };
        let main_size = main_size.map(sanitize_extent);
        let cross_size = cross_size.map(sanitize_extent);

        // The cursor-side inset is the one facing the cursor: top/left for forward flows and
        // bottom/right for reverse ones.
        let cursor_inset = match (parent_axis, parent_direction.is_reverse()) {
            (Axis::Vertical, false) => inset.top,
            (Axis::Vertical, true) => inset.bottom,
            (Axis::Horizontal, false) => inset.left,
            (Axis::Horizontal, true) => inset.right,
        };
        let anchor = parent_axis.value(start);
        let (main_start, main_end) = if parent_direction.is_reverse() {
            match main_size {
                Some(extent) => {
                    let end = anchor - cursor_inset;
                    (end - extent, end)
                }
                None => {
                    let available_start = parent_axis.value(content_start);
                    let end = (anchor - cursor_inset).max(available_start);
                    (available_start, end)
                }
            }
        } else {
            match main_size {
                Some(extent) => {
                    let start = anchor + cursor_inset;
                    (start, start + extent)
                }
                None => {
                    let available_end = parent_axis.value(content_end);
                    let start = anchor + cursor_inset;
                    (start, available_end.max(start))
                }
            }
        };

        // Fixed cross extents are aligned inside the parent's cross axis just like any allocated
        // widget. An explicit extent wins over `Stretch`, which only affects content-sized axes.
        let cross_start = cross_axis.value(content_start);
        let cross_end = cross_axis.value(content_end);
        let cross_extent = (cross_end - cross_start).max(0.0);
        let (cross_start, cross_end) = match cross_size {
            Some(extent) => {
                let offset = match self.layout.align {
                    Align::Start | Align::Stretch => 0.0,
                    Align::Center => (cross_extent - extent) * 0.5,
                    Align::End => cross_extent - extent,
                };
                let start = cross_start + offset;
                (start, start + extent)
            }
            None => (cross_start, cross_end),
        };

        let min = parent_axis.point(main_start, cross_start);
        let rect = Rect::from_min_max(min, parent_axis.point(main_end, cross_end).max(min));
        let origin = rect.min;

        let (result, used) = self.layout_at(rect, origin, config, build);

        let used = if used.is_positive() {
            used
        } else {
            Rect::from_min_size(origin, Vec2::ZERO)
        };
        // Reserve fixed extents even when the content is smaller. Content-sized axes use only the
        // measured part, which is how a content-sized cross axis follows its children.
        let reserved_main = if main_size.is_some() {
            (main_start, main_end)
        } else {
            (parent_axis.value(used.min), parent_axis.value(used.max))
        };
        let reserved_cross = if cross_size.is_some() {
            (cross_start, cross_end)
        } else {
            (cross_axis.value(used.min), cross_axis.value(used.max))
        };
        let reserved = Rect::from_min_max(
            parent_axis.point(reserved_main.0, reserved_cross.0),
            parent_axis.point(reserved_main.1, reserved_cross.1),
        );
        let outer = Rect::from_min_max(
            reserved.min - vec2(inset.left, inset.top),
            reserved.max + vec2(inset.right, inset.bottom),
        );

        if outer.is_positive() {
            match parent_direction {
                Direction::Vertical => {
                    self.layout.cursor.y = self.layout.cursor.y.max(outer.max.y);
                }
                Direction::VerticalReverse => {
                    self.layout.cursor.y = self.layout.cursor.y.min(outer.min.y);
                }
                Direction::Horizontal => {
                    self.layout.cursor.x = self.layout.cursor.x.max(outer.max.x);
                }
                Direction::HorizontalReverse => {
                    self.layout.cursor.x = self.layout.cursor.x.min(outer.min.x);
                }
            }
            self.layout.has_children = true;
            self.layout.used = if self.layout.used.is_positive() {
                self.layout.used.union(outer)
            } else {
                outer
            };
        }

        (result, outer)
    }

    /// Builds a child layout covering `rect`, with relative coordinates measured from `origin`.
    ///
    /// The current layout is replaced for the duration of `build` and restored afterwards, so the
    /// parent layout state lives on this call frame. The clip is not changed. Returns the closure's
    /// result and the rectangle used by in-flow children. The parent's cursor and used rectangle
    /// are not touched; callers decide whether the region participates in the parent's flow.
    fn layout_at<R>(
        &mut self,
        rect: Rect,
        origin: Pos2,
        config: FlowConfig,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> (R, Rect) {
        let parent = std::mem::replace(&mut self.layout, Layout::new(rect, origin, config));

        let result = build(self);
        let child = std::mem::replace(&mut self.layout, parent);

        (result, child.used)
    }

    /// Builds a child layout covering `rect` and narrows the clip to it.
    ///
    /// Used by viewports and placed regions. The previous clip is restored afterwards.
    fn clipped_layout_at<R>(
        &mut self,
        rect: Rect,
        origin: Pos2,
        config: FlowConfig,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> (R, Rect) {
        let clip = self.clip.intersect(rect);
        let previous_painter_clip = self.ctx.painter.clip_rect();
        self.ctx.painter.set_clip_rect(clip);
        let previous_clip = std::mem::replace(&mut self.clip, clip);

        let (result, used) = self.layout_at(rect, origin, config, build);

        self.clip = previous_clip;
        self.ctx.painter.set_clip_rect(previous_painter_clip);

        (result, used)
    }
}

/// Create a scrollable with thread local state.
#[macro_export]
macro_rules! scrollable {
    ($ui:expr, $size:expr, $closure:expr) => {{
        use core::cell::Cell;
        thread_local! {
            static STATE: Cell<$crate::ScrollState> = Cell::new($crate::ScrollState::new());
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
    use crate::{Color32, RawInput, pos2, vec2};

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

    #[test]
    fn interact_reports_pointer_state_for_an_arbitrary_rect() {
        let mut context = context();
        let rect = Rect::from_min_size(pos2(10.0, 10.0), vec2(40.0, 20.0));
        let outside = Rect::from_min_size(pos2(100.0, 100.0), vec2(10.0, 10.0));
        let mut inside_response = None;
        let mut outside_response = None;

        let input = RawInput {
            pointer: Some(pos2(15.0, 15.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(200.0, 200.0), 1.0, input, |ui| {
            inside_response = Some(ui.interact(rect));
            outside_response = Some(ui.interact(outside));
        });

        let inside = inside_response.unwrap();
        assert_eq!(inside.rect, rect);
        assert!(inside.hovered);
        assert!(inside.pressed);
        assert!(inside.down);

        let outside = outside_response.unwrap();
        assert_eq!(
            outside.rect,
            Rect::from_min_size(pos2(100.0, 100.0), vec2(10.0, 10.0))
        );
        assert!(!outside.hovered);
        assert!(!outside.pressed);
        assert!(!outside.down);
    }

    #[test]
    fn interact_is_clipped_to_the_current_region() {
        let mut context = context();
        let rect = Rect::from_min_size(pos2(0.0, 40.0), vec2(20.0, 20.0));
        let mut hovered_at = |pointer| {
            let mut hovered = None;
            let input = RawInput {
                pointer: Some(pointer),
                ..Default::default()
            };
            let _ = context.run(vec2(200.0, 200.0), 1.0, input, |ui| {
                ui.pane(vec2(100.0, 50.0), Vec2::ZERO, |ui| {
                    hovered = Some(ui.interact(rect).hovered);
                });
            });
            hovered.unwrap()
        };

        assert!(hovered_at(pos2(10.0, 45.0)));
        // The pointer is over `rect`, but the pane's clip ends at y=50.
        assert!(!hovered_at(pos2(10.0, 55.0)));
    }

    #[test]
    fn size_reports_the_current_region_extent() {
        let mut context = context();
        let mut root = Vec2::ZERO;
        let mut inside = Vec2::ZERO;

        let _ = context.run(vec2(200.0, 100.0), 1.0, RawInput::default(), |ui| {
            root = ui.size();
            ui.sized_flow(vec2(80.0, 40.0), FlowConfig::vertical(), |ui| {
                inside = ui.size();
            });
        });

        assert_eq!(root, vec2(200.0, 100.0));
        assert_eq!(inside, vec2(80.0, 40.0));
    }
}
