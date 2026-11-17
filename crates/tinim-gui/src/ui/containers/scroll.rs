//! Scrollable content in a clipped viewport.

use tinim_paint::{Rect, RectShape, Shape, Vec2};

use super::super::Ui;
use super::super::layout::{pointer_over, sanitize_size};
use crate::style::ScrollBarStyle;

/// Velocity below which a fling stops, in logical points per second.
const REST_SPEED: f32 = 10.0;
/// Largest velocity derived from one frame of dragging. Keeps a short, fast flick reasonable.
const MAX_SCROLL_SPEED: f32 = 4000.0;
/// Inertia friction. Larger values stop a fling sooner.
const SCROLL_FRICTION: f32 = 4.0;

/// Which scrollbars a [`Ui::scrollable_with_bars`](crate::Ui::scrollable_with_bars) may show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollBars {
    vertical: bool,
    horizontal: bool,
}

impl ScrollBars {
    /// No scrollbars. This matches [`Ui::scrollable`](crate::Ui::scrollable).
    pub const NONE: Self = Self {
        vertical: false,
        horizontal: false,
    };
    /// Only a vertical scrollbar.
    pub const VERTICAL: Self = Self {
        vertical: true,
        horizontal: false,
    };
    /// Only a horizontal scrollbar.
    pub const HORIZONTAL: Self = Self {
        vertical: false,
        horizontal: true,
    };
    /// Both scrollbars.
    pub const BOTH: Self = Self {
        vertical: true,
        horizontal: true,
    };

    /// Enables bars on the given axes.
    pub const fn new(vertical: bool, horizontal: bool) -> Self {
        Self {
            vertical,
            horizontal,
        }
    }

    /// Whether a vertical scrollbar is allowed.
    pub const fn vertical(self) -> bool {
        self.vertical
    }

    /// Whether a horizontal scrollbar is allowed.
    pub const fn horizontal(self) -> bool {
        self.horizontal
    }

    /// Whether any scrollbar is allowed.
    pub const fn is_empty(self) -> bool {
        !self.vertical && !self.horizontal
    }
}

impl Default for ScrollBars {
    fn default() -> Self {
        Self::BOTH
    }
}

/// The axis whose thumb a [`ScrollState`] is currently dragging.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollBarAxis {
    Vertical,
    Horizontal,
}

/// Caller-owned state for [`Ui::scrollable`](crate::Ui::scrollable).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollState {
    /// Current scroll position, clamped to the content bounds. Zero shows the content origin.
    pub offset: Vec2,
    /// Content size measured on the previous frame, if any.
    ///
    /// When this is known, the scroll position is clamped before layout, so an out-of-range
    /// position is corrected in the same frame. It is `None` on the first frame.
    pub content_size: Option<Vec2>,
    /// Scroll velocity in logical points per second, in the same axes as [`Self::offset`].
    pub velocity: Vec2,
    /// Whether a touch drag currently owns the scroll position.
    pub dragging: bool,
    /// Scrollbar thumb currently dragged, if any.
    ///
    /// This is maintained by [`Ui::scrollable_with_bars`](crate::Ui::scrollable_with_bars); other
    /// code should only observe it.
    pub bar_drag: Option<ScrollBarAxis>,
    /// Pointer offset inside the dragged thumb, along its axis, in logical points.
    pub bar_grab: f32,
}

impl ScrollState {
    /// A scroll state at the content origin with no active animation.
    pub const fn new() -> Self {
        Self {
            offset: Vec2::ZERO,
            content_size: None,
            velocity: Vec2::ZERO,
            dragging: false,
            bar_drag: None,
            bar_grab: 0.0,
        }
    }
}

/// The result of building a [`Ui::scrollable`](crate::Ui::scrollable).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollResponse {
    /// The viewport rectangle allocated in the parent flow.
    pub rect: Rect,
    /// Extent of the in-flow content.
    pub content_size: Vec2,
    /// Largest valid scroll position.
    ///
    /// `(content_size - content_rect.size()).max(0)`, where the content rectangle is the viewport
    /// minus any reserved scrollbar strips.
    pub max_offset: Vec2,
    /// Whether the pointer was over the viewport when the scrollable was built.
    pub hovered: bool,
}

impl Ui<'_> {
    /// Builds scrollable content in an in-flow viewport of a fixed `size`.
    ///
    /// `state.offset` is the scroll position: zero shows the content origin, and positive values
    /// reveal content to the right/below, moving the content left/up.
    ///
    /// When `state.content_size` is known from a previous frame, the offset is clamped before
    /// layout, so an out-of-range position is corrected in the same frame. The content size is
    /// measured during layout and stored back in `state`; the offset is clamped again afterwards
    /// to handle content that changed size.
    ///
    /// Inertia is integrated before layout, using the frame time in
    /// [`RawInput::dt`](crate::RawInput::dt). While a touch drag owns the scroll area, the finger
    /// controls the position instead; releasing it turns the last drag velocity into a fling.
    /// Both dragging and flinging stop at the content bounds.
    ///
    /// Wheel and drag input is applied after the content is built, so nested scrollables can take
    /// priority. The innermost scrollable under the pointer scrolls first and keeps as much of
    /// the delta as it can use. Any remainder, from the wheel or a touch drag, is offered to an
    /// enclosing scrollable. Since input is applied after layout, its visual effect appears on the
    /// next frame.
    ///
    /// This variant has no scrollbars; use [`Ui::scrollable_with_bars`] to reserve space for them.
    pub fn scrollable<R>(
        &mut self,
        size: Vec2,
        state: &mut ScrollState,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> (R, ScrollResponse) {
        self.scrollable_impl(size, state, ScrollBars::NONE, build)
    }

    /// Builds scrollable content with room reserved for scrollbars.
    ///
    /// `size` is the outer viewport, including the strips reserved for the bars. Content is laid
    /// out, painted and hit tested in the remaining rectangle, so the bars never overlap content
    /// or steal its pointer input.
    ///
    /// `bars` selects which axes may show a bar. A bar appears on an axis whose content overflows
    /// the content rectangle; the decision uses the content size measured on the previous frame,
    /// so no bar is shown on the first frame. Bars are styled by
    /// [`Style::scroll_bar`](crate::Style::scroll_bar).
    pub fn scrollable_with_bars<R>(
        &mut self,
        size: Vec2,
        bars: ScrollBars,
        state: &mut ScrollState,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> (R, ScrollResponse) {
        self.scrollable_impl(size, state, bars, build)
    }

    fn scrollable_impl<R>(
        &mut self,
        size: Vec2,
        state: &mut ScrollState,
        bars: ScrollBars,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> (R, ScrollResponse) {
        let size = sanitize_size(size);
        let rect = self.allocate_space(size);
        let size = rect.size();
        let hovered = pointer_over(self.clip, &self.ctx.input, rect);
        let dt = sanitize_dt(self.ctx.input.dt);
        let pointer_down = self.ctx.input.primary_down;

        let bar_style = self.ctx.style().scroll_bar;
        let thickness = if bars.is_empty() {
            0.0
        } else {
            bar_style.thickness.max(0.0)
        };
        let (vertical_bar, horizontal_bar) = bar_visibility(
            state.content_size.unwrap_or(Vec2::ZERO),
            size,
            bars,
            thickness,
        );
        let inner_size = content_viewport(size, vertical_bar, horizontal_bar, thickness);

        let known_max_offset = state
            .content_size
            .map(|content_size| (content_size - inner_size).max(Vec2::ZERO));

        sanitize_state(state);
        if bars.is_empty() {
            // The state may have been used with bars before; do not leave a stale drag behind.
            state.bar_drag = None;
        }
        if let Some(max_offset) = known_max_offset {
            state.offset = state.offset.min(max_offset);
        }

        // Animate before layout so this frame reflects the new position. A drag owns the position
        // instead; it is released below, after its final delta has been applied.
        if !state.dragging && state.bar_drag.is_none() {
            animate_scroll(state, dt, known_max_offset.unwrap_or(Vec2::ZERO));
        }

        // Inner scrollables get input first; whatever they cannot use stays in `scroll_remaining`
        // for an enclosing scrollable.
        let content_rect = Rect::from_min_size(rect.min, inner_size);
        let (result, content_size) = self.pane_in(content_rect, -state.offset, build);
        let max_offset = (content_size - inner_size).max(Vec2::ZERO);
        state.content_size = Some(content_size);

        // Correct an offset that the new content size no longer permits before applying input,
        // otherwise the discarded distance would look like scroll input.
        state.offset = state.offset.clamp(Vec2::ZERO, max_offset);

        if vertical_bar || horizontal_bar {
            self.update_scroll_bars(
                state,
                rect,
                inner_size,
                content_size,
                max_offset,
                vertical_bar,
                horizontal_bar,
                bar_style,
            );
        } else {
            state.bar_drag = None;
        }

        if state.bar_drag.is_some() {
            // The thumb owns the pointer; do not turn the same gesture into content scrolling.
            state.velocity = Vec2::ZERO;
            self.scroll_remaining = Vec2::ZERO;
        } else {
            // A drag keeps control of the area after the finger leaves it, until it is released.
            let drag_captured = self.ctx.input.scroll_dragging && state.dragging;
            if self.scroll_remaining != Vec2::ZERO && (hovered || drag_captured) {
                // RawInput uses screen axes (positive y scrolls up), while `offset` counts how far
                // the content has moved left/up.
                let delta = Vec2::new(self.scroll_remaining.x, -self.scroll_remaining.y);
                if self.ctx.input.scroll_dragging {
                    state.dragging = true;
                    let before = state.offset;
                    let remaining = drag_scroll(state, delta, max_offset);
                    if dt > 0.0 {
                        // Base the fling on the movement that actually happened, so pushing against
                        // a boundary does not build up velocity.
                        let applied = state.offset - before;
                        state.velocity = (applied / dt).clamp_length_max(MAX_SCROLL_SPEED);
                    }
                    self.scroll_remaining = Vec2::new(remaining.x, -remaining.y);
                } else {
                    let desired = state.offset + delta;
                    state.offset = desired.clamp(Vec2::ZERO, max_offset);

                    // Leave the part that could not be applied for an enclosing scrollable,
                    // converted back to screen axes.
                    let remaining = desired - state.offset;
                    self.scroll_remaining = Vec2::new(remaining.x, -remaining.y);
                }
            } else if state.dragging && pointer_down {
                // The finger is down but did not move this frame, so do not fling from a stale
                // velocity when it is eventually released.
                state.velocity = Vec2::ZERO;
            }
        }

        if state.dragging && !pointer_down {
            state.dragging = false;
        }

        if vertical_bar || horizontal_bar {
            self.paint_scroll_bars(
                state,
                rect,
                inner_size,
                content_size,
                max_offset,
                vertical_bar,
                horizontal_bar,
                bar_style,
            );
        }

        (
            result,
            ScrollResponse {
                rect,
                content_size,
                max_offset,
                hovered,
            },
        )
    }

    /// Handles presses and drags on the scrollbar strips.
    ///
    /// Bars occupy their own strips outside the content rectangle, so no overlap arbitration is
    /// needed: content simply never sees the pointer there.
    #[allow(clippy::too_many_arguments)]
    fn update_scroll_bars(
        &mut self,
        state: &mut ScrollState,
        rect: Rect,
        inner_size: Vec2,
        content_size: Vec2,
        max_offset: Vec2,
        vertical: bool,
        horizontal: bool,
        style: ScrollBarStyle,
    ) {
        let pointer = self.ctx.input.pointer;
        let pressed = self.ctx.input.primary_pressed;
        let down = self.ctx.input.primary_down;

        if state.bar_drag.is_some() && !down {
            state.bar_drag = None;
        }
        if state
            .bar_drag
            .is_some_and(|axis| !bar_enabled(axis, vertical, horizontal))
        {
            state.bar_drag = None;
        }

        let geometry = |axis, offset| {
            bar_geometry(
                rect,
                inner_size,
                content_size,
                offset,
                max_offset,
                axis,
                style,
            )
        };

        if state.bar_drag.is_none()
            && pressed
            && let Some(pointer) = pointer
            && self.clip.contains(pointer)
        {
            let hit = [ScrollBarAxis::Vertical, ScrollBarAxis::Horizontal]
                .into_iter()
                .filter(|axis| bar_enabled(*axis, vertical, horizontal))
                .map(|axis| (axis, geometry(axis, state.offset)))
                .find(|(_, geometry)| geometry.strip.contains(pointer));

            if let Some((axis, geometry)) = hit {
                let thumb_start = bar_start(geometry.thumb, axis);
                let thumb_len = bar_len(geometry.thumb, axis);
                let along = pointer_along(pointer, axis);
                // Pressing the track grabs the thumb at its center, which also jumps the
                // thumb to the pointer on this frame.
                state.bar_grab = if (thumb_start..=thumb_start + thumb_len).contains(&along) {
                    along - thumb_start
                } else {
                    thumb_len * 0.5
                };
                state.bar_drag = Some(axis);
                state.dragging = false;
                state.velocity = Vec2::ZERO;
            }
        }

        if let (Some(axis), Some(pointer)) = (state.bar_drag, pointer) {
            let geometry = geometry(axis, state.offset);
            let track_start = bar_start(geometry.track, axis);
            let travel = bar_len(geometry.track, axis) - bar_len(geometry.thumb, axis);
            let max = axis_value(max_offset, axis);
            if travel > 0.0 {
                let along = pointer_along(pointer, axis);
                let value = (along - state.bar_grab - track_start) / travel * max;
                set_axis_value(&mut state.offset, axis, value.clamp(0.0, max));
            } else {
                set_axis_value(&mut state.offset, axis, 0.0);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_scroll_bars(
        &mut self,
        state: &ScrollState,
        rect: Rect,
        inner_size: Vec2,
        content_size: Vec2,
        max_offset: Vec2,
        vertical: bool,
        horizontal: bool,
        style: ScrollBarStyle,
    ) {
        let pointer = self
            .ctx
            .input
            .pointer
            .filter(|pointer| self.clip.contains(*pointer));

        for axis in [ScrollBarAxis::Vertical, ScrollBarAxis::Horizontal] {
            if !bar_enabled(axis, vertical, horizontal) {
                continue;
            }

            let geometry = bar_geometry(
                rect,
                inner_size,
                content_size,
                state.offset,
                max_offset,
                axis,
                style,
            );
            let dragging = state.bar_drag == Some(axis);
            let hovered = pointer.is_some_and(|pointer| geometry.track.contains(pointer));

            if geometry.track.is_positive() {
                self.ctx.painter.add(Shape::Rect(
                    RectShape::new(geometry.track, style.track)
                        .with_corner_radius(style.corner_radius),
                ));
            }

            if geometry.thumb.is_positive() {
                let fill = if dragging {
                    style.thumb_dragging
                } else if hovered {
                    style.thumb_hovered
                } else {
                    style.thumb
                };
                self.ctx.painter.add(Shape::Rect(
                    RectShape::new(geometry.thumb, fill).with_corner_radius(style.corner_radius),
                ));
            }
        }
    }
}

/// Geometry of one scrollbar inside the strip reserved for it.
#[derive(Clone, Copy, Debug)]
struct BarGeometry {
    /// The full strip, including the margin around the track.
    strip: Rect,
    /// The area the thumb travels in.
    track: Rect,
    /// The draggable thumb.
    thumb: Rect,
}

fn bar_enabled(axis: ScrollBarAxis, vertical: bool, horizontal: bool) -> bool {
    match axis {
        ScrollBarAxis::Vertical => vertical,
        ScrollBarAxis::Horizontal => horizontal,
    }
}

/// Whether each enabled axis needs a bar for `content`.
///
/// Bars only appear for overflowing content. Since one bar shrinks the other axis, it can make the
/// other axis overflow too, so the flags are propagated once.
fn bar_visibility(content: Vec2, size: Vec2, bars: ScrollBars, thickness: f32) -> (bool, bool) {
    if thickness <= 0.0 {
        return (false, false);
    }

    let can_vertical = bars.vertical() && size.x > thickness;
    let can_horizontal = bars.horizontal() && size.y > thickness;
    let mut vertical = can_vertical && content.y > size.y;
    let mut horizontal = can_horizontal && content.x > size.x;

    horizontal |= can_horizontal && vertical && content.x > size.x - thickness;
    vertical |= can_vertical && horizontal && content.y > size.y - thickness;

    (vertical, horizontal)
}

/// The rectangle left for content after reserving the shown bars.
fn content_viewport(size: Vec2, vertical: bool, horizontal: bool, thickness: f32) -> Vec2 {
    Vec2::new(
        if vertical {
            (size.x - thickness).max(0.0)
        } else {
            size.x
        },
        if horizontal {
            (size.y - thickness).max(0.0)
        } else {
            size.y
        },
    )
}

fn bar_geometry(
    rect: Rect,
    inner_size: Vec2,
    content_size: Vec2,
    offset: Vec2,
    max_offset: Vec2,
    axis: ScrollBarAxis,
    style: ScrollBarStyle,
) -> BarGeometry {
    let thickness = style.thickness.max(0.0);
    let strip = match axis {
        ScrollBarAxis::Vertical => Rect::from_min_max(
            Vec2::new(rect.max.x - thickness, rect.min.y),
            Vec2::new(rect.max.x, rect.min.y + inner_size.y),
        ),
        ScrollBarAxis::Horizontal => Rect::from_min_max(
            Vec2::new(rect.min.x, rect.max.y - thickness),
            Vec2::new(rect.min.x + inner_size.x, rect.max.y),
        ),
    };

    let margin = style.margin.max(0.0);
    let track = Rect::from_min_size(
        strip.min + Vec2::splat(margin),
        (strip.size() - Vec2::splat(margin * 2.0)).max(Vec2::ZERO),
    );

    let (viewport_len, content_len, offset, max_offset) = match axis {
        ScrollBarAxis::Vertical => (inner_size.y, content_size.y, offset.y, max_offset.y),
        ScrollBarAxis::Horizontal => (inner_size.x, content_size.x, offset.x, max_offset.x),
    };
    let track_len = bar_len(track, axis);
    let ratio = if content_len > 0.0 {
        (viewport_len / content_len).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let min_len = style.min_thumb_length.max(0.0).min(track_len);
    let thumb_len = (track_len * ratio).clamp(min_len, track_len);
    let travel = track_len - thumb_len;
    let progress = if max_offset > 0.0 {
        (offset / max_offset).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let thumb = match axis {
        ScrollBarAxis::Vertical => Rect::from_min_size(
            Vec2::new(track.min.x, track.min.y + travel * progress),
            Vec2::new(track.width(), thumb_len),
        ),
        ScrollBarAxis::Horizontal => Rect::from_min_size(
            Vec2::new(track.min.x + travel * progress, track.min.y),
            Vec2::new(thumb_len, track.height()),
        ),
    };

    BarGeometry {
        strip,
        track,
        thumb,
    }
}

fn pointer_along(pointer: Vec2, axis: ScrollBarAxis) -> f32 {
    match axis {
        ScrollBarAxis::Vertical => pointer.y,
        ScrollBarAxis::Horizontal => pointer.x,
    }
}

fn axis_value(value: Vec2, axis: ScrollBarAxis) -> f32 {
    match axis {
        ScrollBarAxis::Vertical => value.y,
        ScrollBarAxis::Horizontal => value.x,
    }
}

fn set_axis_value(value: &mut Vec2, axis: ScrollBarAxis, along: f32) {
    match axis {
        ScrollBarAxis::Vertical => value.y = along,
        ScrollBarAxis::Horizontal => value.x = along,
    }
}

fn bar_start(rect: Rect, axis: ScrollBarAxis) -> f32 {
    match axis {
        ScrollBarAxis::Vertical => rect.min.y,
        ScrollBarAxis::Horizontal => rect.min.x,
    }
}

fn bar_len(rect: Rect, axis: ScrollBarAxis) -> f32 {
    match axis {
        ScrollBarAxis::Vertical => rect.height(),
        ScrollBarAxis::Horizontal => rect.width(),
    }
}

fn sanitize_state(state: &mut ScrollState) {
    if !state.offset.is_finite() {
        state.offset = Vec2::ZERO;
    }
    if !state.velocity.is_finite() {
        state.velocity = Vec2::ZERO;
    }
    if !state.bar_grab.is_finite() {
        state.bar_grab = 0.0;
    }
    state.offset = state.offset.max(Vec2::ZERO);
}

fn sanitize_dt(dt: f32) -> f32 {
    if dt.is_finite() && dt > 0.0 {
        dt.min(0.1)
    } else {
        0.0
    }
}

/// Applies a touch drag, stopping at the content bounds. Returns the part of `delta` that the
/// bounds did not allow, which should be offered to an enclosing scrollable.
fn drag_scroll(state: &mut ScrollState, delta: Vec2, max_offset: Vec2) -> Vec2 {
    let desired = state.offset + delta;
    state.offset = desired.clamp(Vec2::ZERO, max_offset);
    desired - state.offset
}

fn animate_scroll(state: &mut ScrollState, dt: f32, max_offset: Vec2) {
    if dt <= 0.0 || state.velocity == Vec2::ZERO {
        return;
    }

    state.velocity *= (-SCROLL_FRICTION * dt).exp();
    if state.velocity.length() < REST_SPEED {
        state.velocity = Vec2::ZERO;
        return;
    }

    let next = state.offset + state.velocity * dt;
    let clamped = next.clamp(Vec2::ZERO, max_offset);
    // Stop dead at the bounds instead of overshooting.
    if clamped.x != next.x {
        state.velocity.x = 0.0;
    }
    if clamped.y != next.y {
        state.velocity.y = 0.0;
    }
    state.offset = clamped;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_util::context;
    use crate::{RawInput, Rect, pos2, vec2};

    #[test]
    fn scrollable_translates_content_and_reports_limits() {
        let mut context = context();
        let mut state = ScrollState {
            offset: vec2(10.0, 20.0),
            content_size: None,
            ..Default::default()
        };
        let mut inside = Rect::NOTHING;
        let mut response = None;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            let ((), result) = ui.scrollable(vec2(100.0, 100.0), &mut state, |ui| {
                inside = ui.allocate_space(vec2(200.0, 300.0));
            });
            response = Some(result);
        });

        let response = response.unwrap();
        assert_eq!(
            response.rect,
            Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0))
        );
        assert_eq!(response.content_size, vec2(200.0, 300.0));
        assert_eq!(response.max_offset, vec2(100.0, 200.0));
        assert_eq!(inside.min, pos2(-10.0, -20.0));
        assert_eq!(state.offset, vec2(10.0, 20.0));
        assert_eq!(state.content_size, Some(vec2(200.0, 300.0)));
    }

    #[test]
    fn scrollable_advances_the_parent_flow() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut state = ScrollState::default();
        let mut after = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.scrollable(vec2(100.0, 50.0), &mut state, |ui| {
                ui.allocate_space(vec2(200.0, 300.0));
            });
            after = ui.allocate_space(vec2(10.0, 10.0));
        });

        assert_eq!(after.min, pos2(0.0, 50.0 + spacing));
    }

    #[test]
    fn scrollable_with_bars_advances_the_parent_flow_by_its_outer_size() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut state = ScrollState {
            content_size: Some(vec2(500.0, 500.0)),
            ..Default::default()
        };
        let mut after = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.scrollable_with_bars(vec2(100.0, 50.0), ScrollBars::BOTH, &mut state, |ui| {
                ui.allocate_space(vec2(500.0, 500.0));
            });
            after = ui.allocate_space(vec2(10.0, 10.0));
        });

        assert_eq!(after.min, pos2(0.0, 50.0 + spacing));
    }

    #[test]
    fn scrollable_consumes_wheel_while_hovered() {
        let mut context = context();
        let mut state = ScrollState {
            offset: vec2(0.0, 50.0),
            content_size: Some(vec2(200.0, 300.0)),
            ..Default::default()
        };
        let input = RawInput {
            pointer: Some(pos2(5.0, 5.0)),
            scroll: vec2(10.0, -20.0),
            ..Default::default()
        };

        let _ = context.run(vec2(200.0, 200.0), 1.0, input, |ui| {
            ui.scrollable(vec2(100.0, 100.0), &mut state, |ui| {
                ui.allocate_space(vec2(200.0, 300.0));
            });
        });

        // Wheel down (negative y) increases offset.y; wheel right increases offset.x.
        assert_eq!(state.offset, vec2(10.0, 70.0));
    }

    #[test]
    fn scrollable_ignores_wheel_outside_its_viewport() {
        let mut context = context();
        let mut state = ScrollState {
            offset: vec2(0.0, 50.0),
            content_size: Some(vec2(200.0, 300.0)),
            ..Default::default()
        };
        let input = RawInput {
            pointer: Some(pos2(500.0, 500.0)),
            scroll: vec2(0.0, -20.0),
            ..Default::default()
        };

        let _ = context.run(vec2(200.0, 200.0), 1.0, input, |ui| {
            ui.scrollable(vec2(100.0, 100.0), &mut state, |ui| {
                ui.allocate_space(vec2(200.0, 300.0));
            });
        });

        assert_eq!(state.offset, vec2(0.0, 50.0));
    }

    #[test]
    fn scrollable_clamps_wheel_to_the_content_boundary() {
        let mut context = context();
        let mut state = ScrollState {
            offset: vec2(0.0, 190.0),
            content_size: Some(vec2(200.0, 300.0)),
            ..Default::default()
        };
        let mut inside = Rect::NOTHING;
        let input = RawInput {
            pointer: Some(pos2(5.0, 5.0)),
            scroll: vec2(0.0, -20.0),
            ..Default::default()
        };

        let _ = context.run(vec2(200.0, 200.0), 1.0, input, |ui| {
            ui.scrollable(vec2(100.0, 100.0), &mut state, |ui| {
                inside = ui.allocate_space(vec2(200.0, 300.0));
            });
        });

        // The wheel asks for 210, but the content boundary clamps the offset to 200. Layout used
        // the old offset, so the visual position catches up on the next frame.
        assert_eq!(state.offset, vec2(0.0, 200.0));
        assert_eq!(inside.min, pos2(0.0, -190.0));
    }

    #[test]
    fn scrollable_clamps_first_frame_without_a_previous_content_size() {
        let mut context = context();
        let mut state = ScrollState {
            offset: vec2(1000.0, 1000.0),
            content_size: None,
            ..Default::default()
        };
        let mut inside = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.scrollable(vec2(100.0, 100.0), &mut state, |ui| {
                inside = ui.allocate_space(vec2(200.0, 300.0));
            });
        });

        // Without a previous measurement the content is laid out at the old position, then clamped.
        assert_eq!(state.offset, vec2(100.0, 200.0));
        assert_eq!(inside.min, pos2(-1000.0, -1000.0));
        assert_eq!(state.content_size, Some(vec2(200.0, 300.0)));
    }

    #[test]
    fn scrollable_still_clamps_after_content_shrinks() {
        let mut context = context();
        let mut state = ScrollState {
            offset: vec2(100.0, 200.0),
            content_size: Some(vec2(200.0, 300.0)),
            ..Default::default()
        };

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.scrollable(vec2(100.0, 100.0), &mut state, |ui| {
                ui.allocate_space(vec2(50.0, 50.0));
            });
        });

        // The previous max still allows the old offset for this frame; the fallback clamp fixes it.
        assert_eq!(state.offset, Vec2::ZERO);
        assert_eq!(state.content_size, Some(vec2(50.0, 50.0)));
    }

    #[test]
    fn scrollable_resets_scroll_when_content_fits() {
        let mut context = context();
        let mut state = ScrollState {
            offset: vec2(10.0, 10.0),
            content_size: None,
            ..Default::default()
        };
        let mut max_offset = None;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            let ((), response) = ui.scrollable(vec2(100.0, 100.0), &mut state, |ui| {
                ui.allocate_space(vec2(50.0, 50.0));
            });
            max_offset = Some(response.max_offset);
        });

        assert_eq!(max_offset, Some(Vec2::ZERO));
        assert_eq!(state.offset, Vec2::ZERO);
    }

    #[test]
    fn nested_scrollables_let_the_inner_one_consume_the_wheel_first() {
        let mut context = context();
        let mut outer = ScrollState::default();
        let mut inner = ScrollState::default();
        let input = RawInput {
            pointer: Some(pos2(5.0, 5.0)),
            scroll: vec2(0.0, -20.0),
            ..Default::default()
        };

        let _ = context.run(vec2(200.0, 200.0), 1.0, input, |ui| {
            ui.scrollable(vec2(100.0, 100.0), &mut outer, |ui| {
                ui.scrollable(vec2(50.0, 50.0), &mut inner, |ui| {
                    ui.allocate_space(vec2(10.0, 200.0));
                });
                ui.allocate_space(vec2(10.0, 200.0));
            });
        });

        assert_eq!(outer.offset, Vec2::ZERO);
        assert_eq!(inner.offset, vec2(0.0, 20.0));
    }

    #[test]
    fn enclosing_scrollable_consumes_the_wheel_when_inner_content_fits() {
        let mut context = context();
        let mut outer = ScrollState::default();
        let mut inner = ScrollState::default();
        let input = RawInput {
            pointer: Some(pos2(5.0, 5.0)),
            scroll: vec2(0.0, -20.0),
            ..Default::default()
        };

        let _ = context.run(vec2(200.0, 200.0), 1.0, input, |ui| {
            ui.scrollable(vec2(100.0, 100.0), &mut outer, |ui| {
                ui.scrollable(vec2(50.0, 50.0), &mut inner, |ui| {
                    ui.allocate_space(vec2(10.0, 10.0));
                });
                ui.allocate_space(vec2(10.0, 200.0));
            });
        });

        assert_eq!(inner.offset, Vec2::ZERO);
        assert_eq!(outer.offset, vec2(0.0, 20.0));
    }

    #[test]
    fn inner_scrollable_passes_unused_wheel_delta_to_the_enclosing_one() {
        let mut context = context();
        let mut outer = ScrollState::default();
        let mut inner = ScrollState {
            offset: vec2(0.0, 140.0),
            content_size: Some(vec2(10.0, 200.0)),
            ..Default::default()
        };
        let input = RawInput {
            pointer: Some(pos2(5.0, 5.0)),
            scroll: vec2(0.0, -20.0),
            ..Default::default()
        };

        let _ = context.run(vec2(200.0, 200.0), 1.0, input, |ui| {
            ui.scrollable(vec2(100.0, 100.0), &mut outer, |ui| {
                ui.scrollable(vec2(50.0, 50.0), &mut inner, |ui| {
                    ui.allocate_space(vec2(10.0, 200.0));
                });
                ui.allocate_space(vec2(10.0, 200.0));
            });
        });

        // The inner scrollable reaches its 150 boundary and passes the remaining 10 to the outer
        // scrollable.
        assert_eq!(inner.offset, vec2(0.0, 150.0));
        assert_eq!(outer.offset, vec2(0.0, 10.0));
    }

    #[test]
    fn nested_scrollables_let_the_inner_one_consume_the_drag_first() {
        let mut context = context();
        let mut outer = ScrollState::default();
        let mut inner = ScrollState::default();
        let input = RawInput {
            pointer: Some(pos2(5.0, 5.0)),
            primary_down: true,
            scroll: vec2(0.0, -20.0),
            scroll_dragging: true,
            dt: 1.0 / 60.0,
            ..Default::default()
        };

        let _ = context.run(vec2(200.0, 200.0), 1.0, input, |ui| {
            ui.scrollable(vec2(100.0, 100.0), &mut outer, |ui| {
                ui.scrollable(vec2(50.0, 50.0), &mut inner, |ui| {
                    ui.allocate_space(vec2(10.0, 200.0));
                });
                ui.allocate_space(vec2(10.0, 200.0));
            });
        });

        assert_eq!(outer.offset, Vec2::ZERO);
        assert_eq!(inner.offset, vec2(0.0, 20.0));
    }

    #[test]
    fn inner_scrollable_passes_unused_drag_delta_to_the_enclosing_one() {
        let mut context = context();
        let mut outer = ScrollState::default();
        let mut inner = ScrollState {
            offset: vec2(0.0, 140.0),
            content_size: Some(vec2(10.0, 200.0)),
            ..Default::default()
        };
        let input = RawInput {
            pointer: Some(pos2(5.0, 5.0)),
            primary_down: true,
            scroll: vec2(0.0, -20.0),
            scroll_dragging: true,
            dt: 1.0 / 60.0,
            ..Default::default()
        };

        let _ = context.run(vec2(200.0, 200.0), 1.0, input, |ui| {
            ui.scrollable(vec2(100.0, 100.0), &mut outer, |ui| {
                ui.scrollable(vec2(50.0, 50.0), &mut inner, |ui| {
                    ui.allocate_space(vec2(10.0, 200.0));
                });
                ui.allocate_space(vec2(10.0, 200.0));
            });
        });

        // The inner scrollable reaches its 150 boundary and passes the remaining 10 to the outer
        // scrollable, just like the wheel.
        assert_eq!(inner.offset, vec2(0.0, 150.0));
        assert_eq!(outer.offset, vec2(0.0, 10.0));
    }

    #[test]
    fn enclosing_scrollable_consumes_the_drag_when_inner_content_fits() {
        let mut context = context();
        let mut outer = ScrollState::default();
        let mut inner = ScrollState::default();
        let input = RawInput {
            pointer: Some(pos2(5.0, 5.0)),
            primary_down: true,
            scroll: vec2(0.0, -20.0),
            scroll_dragging: true,
            dt: 1.0 / 60.0,
            ..Default::default()
        };

        let _ = context.run(vec2(200.0, 200.0), 1.0, input, |ui| {
            ui.scrollable(vec2(100.0, 100.0), &mut outer, |ui| {
                ui.scrollable(vec2(50.0, 50.0), &mut inner, |ui| {
                    ui.allocate_space(vec2(10.0, 10.0));
                });
                ui.allocate_space(vec2(10.0, 200.0));
            });
        });

        assert_eq!(inner.offset, Vec2::ZERO);
        assert_eq!(outer.offset, vec2(0.0, 20.0));
    }

    #[test]
    fn shrinking_inner_content_does_not_forward_stale_offset_as_wheel_delta() {
        let mut context = context();
        let mut outer = ScrollState::default();
        let mut inner = ScrollState {
            offset: vec2(0.0, 200.0),
            content_size: Some(vec2(10.0, 300.0)),
            ..Default::default()
        };
        let input = RawInput {
            pointer: Some(pos2(5.0, 5.0)),
            scroll: vec2(0.0, -20.0),
            ..Default::default()
        };

        let _ = context.run(vec2(200.0, 200.0), 1.0, input, |ui| {
            ui.scrollable(vec2(100.0, 100.0), &mut outer, |ui| {
                ui.scrollable(vec2(50.0, 50.0), &mut inner, |ui| {
                    // The inner content shrank since the previous frame, so its old offset is no
                    // longer valid. Only the wheel delta should reach the outer scrollable.
                    ui.allocate_space(vec2(10.0, 10.0));
                });
                ui.allocate_space(vec2(10.0, 200.0));
            });
        });

        assert_eq!(inner.offset, Vec2::ZERO);
        assert_eq!(outer.offset, vec2(0.0, 20.0));
    }

    fn run_scroll(context: &mut crate::Context, state: &mut ScrollState, input: RawInput) {
        let _ = context.run(vec2(200.0, 200.0), 1.0, input, |ui| {
            ui.scrollable(vec2(100.0, 100.0), state, |ui| {
                ui.allocate_space(vec2(200.0, 300.0));
            });
        });
    }

    #[test]
    fn releasing_a_drag_starts_a_fling() {
        let mut context = context();
        let mut state = ScrollState::default();

        run_scroll(
            &mut context,
            &mut state,
            RawInput {
                pointer: Some(pos2(5.0, 5.0)),
                primary_down: true,
                scroll: vec2(0.0, -20.0),
                scroll_dragging: true,
                dt: 1.0 / 60.0,
                ..Default::default()
            },
        );
        assert!(state.dragging);
        let dragged = state.offset.y;
        assert_eq!(dragged, 20.0);

        // The finger is released: the drag ends, but the velocity is kept.
        run_scroll(
            &mut context,
            &mut state,
            RawInput {
                pointer: Some(pos2(5.0, 5.0)),
                dt: 1.0 / 60.0,
                ..Default::default()
            },
        );
        assert!(!state.dragging);
        assert!(state.velocity.y > 0.0);

        // Without more input the content keeps moving and slows down.
        let mut previous = state.offset.y;
        for _ in 0..10 {
            run_scroll(
                &mut context,
                &mut state,
                RawInput {
                    dt: 1.0 / 60.0,
                    ..Default::default()
                },
            );
            assert!(state.offset.y >= previous);
            previous = state.offset.y;
        }
        assert!(state.offset.y > dragged);
    }

    #[test]
    fn a_held_finger_does_not_fling_when_released() {
        let mut context = context();
        let mut state = ScrollState::default();

        run_scroll(
            &mut context,
            &mut state,
            RawInput {
                pointer: Some(pos2(5.0, 5.0)),
                primary_down: true,
                scroll: vec2(0.0, -20.0),
                scroll_dragging: true,
                dt: 1.0 / 60.0,
                ..Default::default()
            },
        );
        assert!(state.velocity.y > 0.0);

        // Holding still clears the stale velocity.
        run_scroll(
            &mut context,
            &mut state,
            RawInput {
                pointer: Some(pos2(5.0, 5.0)),
                primary_down: true,
                dt: 1.0 / 60.0,
                ..Default::default()
            },
        );
        assert_eq!(state.velocity, Vec2::ZERO);

        run_scroll(
            &mut context,
            &mut state,
            RawInput {
                pointer: Some(pos2(5.0, 5.0)),
                dt: 1.0 / 60.0,
                ..Default::default()
            },
        );
        let released = state.offset.y;
        for _ in 0..10 {
            run_scroll(
                &mut context,
                &mut state,
                RawInput {
                    dt: 1.0 / 60.0,
                    ..Default::default()
                },
            );
        }
        assert_eq!(state.offset.y, released);
    }

    #[test]
    fn a_drag_keeps_scrolling_after_the_pointer_leaves() {
        let mut context = context();
        let mut state = ScrollState::default();

        run_scroll(
            &mut context,
            &mut state,
            RawInput {
                pointer: Some(pos2(5.0, 5.0)),
                primary_down: true,
                scroll: vec2(0.0, -20.0),
                scroll_dragging: true,
                dt: 1.0 / 60.0,
                ..Default::default()
            },
        );
        assert_eq!(state.offset.y, 20.0);

        // The finger is now outside the 100x100 viewport, but the drag still owns the area.
        run_scroll(
            &mut context,
            &mut state,
            RawInput {
                pointer: Some(pos2(300.0, 300.0)),
                primary_down: true,
                scroll: vec2(0.0, -10.0),
                scroll_dragging: true,
                dt: 1.0 / 60.0,
                ..Default::default()
            },
        );
        assert_eq!(state.offset.y, 30.0);
        assert!(state.dragging);

        run_scroll(
            &mut context,
            &mut state,
            RawInput {
                pointer: Some(pos2(300.0, 300.0)),
                dt: 1.0 / 60.0,
                ..Default::default()
            },
        );
        assert!(!state.dragging);
    }

    #[test]
    fn dragging_past_the_end_offers_the_excess_to_the_enclosing_scrollable() {
        let mut state = ScrollState {
            offset: vec2(0.0, 200.0),
            ..Default::default()
        };
        let remaining = drag_scroll(&mut state, vec2(0.0, 50.0), vec2(0.0, 200.0));

        assert_eq!(remaining, vec2(0.0, 50.0));
        assert_eq!(state.offset, vec2(0.0, 200.0));
    }

    #[test]
    fn scrollable_with_bars_reserves_strips_for_the_visible_bars() {
        let mut context = context();
        let thickness = context.style().scroll_bar.thickness;
        let mut state = ScrollState {
            content_size: Some(vec2(200.0, 300.0)),
            ..Default::default()
        };
        let mut response = None;
        let mut inside = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            let ((), result) =
                ui.scrollable_with_bars(vec2(100.0, 100.0), ScrollBars::BOTH, &mut state, |ui| {
                    inside = ui.allocate_space(vec2(200.0, 300.0));
                });
            response = Some(result);
        });

        let response = response.unwrap();
        let content = 100.0 - thickness;
        assert_eq!(
            response.rect,
            Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0))
        );
        assert_eq!(response.content_size, vec2(200.0, 300.0));
        assert_eq!(response.max_offset, vec2(200.0 - content, 300.0 - content));
        // The content rectangle starts at the viewport origin and shrinks at the right/bottom.
        assert_eq!(inside.min, pos2(0.0, 0.0));
    }

    #[test]
    fn horizontal_only_scrollable_reserves_the_bottom_strip() {
        let mut context = context();
        let thickness = context.style().scroll_bar.thickness;
        let mut state = ScrollState {
            content_size: Some(vec2(400.0, 80.0)),
            ..Default::default()
        };
        let mut response = None;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            let ((), result) = ui.scrollable_with_bars(
                vec2(100.0, 100.0),
                ScrollBars::HORIZONTAL,
                &mut state,
                |ui| {
                    ui.allocate_space(vec2(400.0, 80.0));
                },
            );
            response = Some(result);
        });

        let response = response.unwrap();
        assert_eq!(response.max_offset, vec2(300.0, 0.0));
        assert!(thickness > 0.0);
    }

    #[test]
    fn bars_are_hidden_when_content_fits() {
        let mut context = context();
        let mut state = ScrollState {
            content_size: Some(vec2(80.0, 80.0)),
            ..Default::default()
        };
        let mut response = None;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            let ((), result) =
                ui.scrollable_with_bars(vec2(100.0, 100.0), ScrollBars::BOTH, &mut state, |ui| {
                    ui.allocate_space(vec2(80.0, 80.0));
                });
            response = Some(result);
        });

        assert_eq!(response.unwrap().max_offset, Vec2::ZERO);
    }

    #[test]
    fn content_is_clipped_out_of_the_reserved_bar_strip() {
        let mut context = context();
        let thickness = context.style().scroll_bar.thickness;
        let mut state = ScrollState {
            content_size: Some(vec2(80.0, 400.0)),
            ..Default::default()
        };

        let frame = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.scrollable_with_bars(vec2(100.0, 100.0), ScrollBars::VERTICAL, &mut state, |ui| {
                ui.label("content");
            });
        });

        let content_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0 - thickness, 100.0));
        assert!(
            frame
                .paint
                .draws
                .iter()
                .any(|draw| draw.clip_rect == content_rect)
        );
        // The bar is painted with the outer clip, over its own strip rather than the content clip.
        assert!(
            frame
                .paint
                .draws
                .iter()
                .any(|draw| draw.clip_rect
                    == Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 200.0)))
        );
    }

    fn run_bars(
        context: &mut crate::Context,
        state: &mut ScrollState,
        bars: ScrollBars,
        content: Vec2,
        input: RawInput,
    ) {
        let _ = context.run(vec2(200.0, 200.0), 1.0, input, |ui| {
            ui.scrollable_with_bars(vec2(100.0, 100.0), bars, state, |ui| {
                ui.allocate_space(content);
            });
        });
    }

    #[test]
    fn dragging_the_vertical_bar_thumb_scrolls_the_content() {
        let mut context = context();
        let bar = context.style().scroll_bar;
        let mut state = ScrollState {
            content_size: Some(vec2(80.0, 400.0)),
            ..Default::default()
        };
        let track_len = 100.0 - bar.margin * 2.0;
        let thumb_len = (track_len * 100.0 / 400.0).max(bar.min_thumb_length);
        let x = 100.0 - bar.thickness / 2.0;

        run_bars(
            &mut context,
            &mut state,
            ScrollBars::VERTICAL,
            vec2(80.0, 400.0),
            RawInput {
                pointer: Some(pos2(x, bar.margin + thumb_len / 2.0)),
                primary_down: true,
                primary_pressed: true,
                ..Default::default()
            },
        );
        assert_eq!(state.bar_drag, Some(ScrollBarAxis::Vertical));
        assert_eq!(state.offset, Vec2::ZERO);

        run_bars(
            &mut context,
            &mut state,
            ScrollBars::VERTICAL,
            vec2(80.0, 400.0),
            RawInput {
                pointer: Some(pos2(x, 100.0 - bar.margin)),
                primary_down: true,
                ..Default::default()
            },
        );
        // Dragging the thumb to the bottom of the track reaches the content boundary.
        assert_eq!(state.offset.y, 300.0);

        run_bars(
            &mut context,
            &mut state,
            ScrollBars::VERTICAL,
            vec2(80.0, 400.0),
            RawInput::default(),
        );
        assert_eq!(state.bar_drag, None);
        assert_eq!(state.offset.y, 300.0);
    }

    #[test]
    fn dragging_the_horizontal_bar_thumb_updates_the_x_offset() {
        let mut context = context();
        let bar = context.style().scroll_bar;
        let mut state = ScrollState {
            content_size: Some(vec2(400.0, 80.0)),
            ..Default::default()
        };
        let track_len = 100.0 - bar.margin * 2.0;
        let thumb_len = (track_len * 100.0 / 400.0).max(bar.min_thumb_length);
        let y = 100.0 - bar.thickness / 2.0;

        run_bars(
            &mut context,
            &mut state,
            ScrollBars::HORIZONTAL,
            vec2(400.0, 80.0),
            RawInput {
                pointer: Some(pos2(bar.margin + thumb_len / 2.0, y)),
                primary_down: true,
                primary_pressed: true,
                ..Default::default()
            },
        );
        assert_eq!(state.bar_drag, Some(ScrollBarAxis::Horizontal));

        run_bars(
            &mut context,
            &mut state,
            ScrollBars::HORIZONTAL,
            vec2(400.0, 80.0),
            RawInput {
                pointer: Some(pos2(100.0 - bar.margin, y)),
                primary_down: true,
                ..Default::default()
            },
        );
        assert_eq!(state.offset, vec2(300.0, 0.0));
    }

    #[test]
    fn a_fling_stops_at_the_boundary() {
        let mut state = ScrollState {
            offset: vec2(0.0, 190.0),
            velocity: vec2(0.0, 1200.0),
            ..Default::default()
        };

        animate_scroll(&mut state, 1.0 / 60.0, vec2(0.0, 200.0));

        assert_eq!(state.offset, vec2(0.0, 200.0));
        assert_eq!(state.velocity, Vec2::ZERO);
    }
}
