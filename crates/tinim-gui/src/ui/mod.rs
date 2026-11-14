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

pub use containers::{FrameResponse, PaneResponse, ScrollResponse, ScrollState};
pub use layout::{Align, FlowConfig};
pub use response::Response;
pub use widgets::{NumberInputResponse, NumberInputState, TextEditResponse, TextEditState};

use tinim_paint::{Pos2, Rect, Vec2, vec2};

use crate::context::Context;
use crate::cursor::CursorIcon;
use crate::style::{Edges, Style};

use layout::{Axis, Direction, Layout, allocate_space, available_size, next_pos, sanitize_extent};

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

    /// Builds a content-sized region, inset by `inset` and optionally fixed on the cross axis.
    ///
    /// `cross_size` fixes the cross-axis extent of the child region: the width of a vertical flow
    /// or the height of a horizontal flow. `None` inherits the enclosing region's cross-axis
    /// extent. This changes layout only; the clip is inherited unchanged.
    ///
    /// The parent flow advances by the rectangle used by the children, expanded by `inset`. When
    /// `cross_size` is set, the cross-axis part of the region is included even if the children are
    /// narrower, so a fixed-width column or a fixed-height row reserves its space. The second
    /// return value is that outer rectangle, or a zero-size rectangle at the region start when
    /// nothing was placed.
    ///
    /// Only forward directions are supported; callers must check that the current region and
    /// `config` are forward flows before calling this.
    fn content_region<R>(
        &mut self,
        inset: Edges,
        cross_size: Option<f32>,
        config: FlowConfig,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> (R, Rect) {
        let start = next_pos(&self.layout, self.spacing());
        let origin = start + vec2(inset.left, inset.top);

        let axis = config.direction.axis();
        let cross_extent = cross_size.map(sanitize_extent);
        let parent_max = self.layout.rect.max - vec2(inset.right, inset.bottom);
        let max = match cross_extent {
            Some(cross_extent) => match axis {
                Axis::Vertical => Pos2::new(origin.x + cross_extent, parent_max.y),
                Axis::Horizontal => Pos2::new(parent_max.x, origin.y + cross_extent),
            },
            None => parent_max,
        };
        let rect = Rect::from_min_max(origin, max.max(origin));

        let (result, used) = self.layout_at(rect, origin, config, build);

        let mut used = if used.is_positive() {
            used
        } else {
            Rect::from_min_size(origin, Vec2::ZERO)
        };
        if let Some(cross_extent) = cross_extent {
            let cross_region = match axis {
                Axis::Vertical => Rect::from_min_max(
                    Pos2::new(origin.x, used.min.y),
                    Pos2::new(origin.x + cross_extent, used.max.y),
                ),
                Axis::Horizontal => Rect::from_min_max(
                    Pos2::new(used.min.x, origin.y),
                    Pos2::new(used.max.x, origin.y + cross_extent),
                ),
            };
            used = used.union(cross_region);
        }
        let outer = Rect::from_min_max(
            used.min - vec2(inset.left, inset.top),
            used.max + vec2(inset.right, inset.bottom),
        );

        if outer.is_positive() {
            match self.layout.direction {
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
