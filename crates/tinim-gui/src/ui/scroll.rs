//! Scrollable content in a clipped viewport.

use tinim_paint::Vec2;

use super::Ui;
use super::layout::{next_rect, pointer_over, sanitize_size};
use super::response::{ScrollResponse, ScrollState};

impl Ui<'_> {
    /// Builds scrollable content in an in-flow viewport of a fixed `size`.
    ///
    /// `state.offset` is the scroll position: zero shows the content origin, and positive values
    /// reveal content to the right/below, moving the content left/up.
    ///
    /// When `state.content_size` is known from a previous frame, the offset and wheel delta are
    /// clamped before layout, so reaching a boundary takes effect in the same frame. After layout
    /// the new content size is stored in `state`, and the offset is clamped again as a fallback
    /// for content that changed size. On the first frame, and when content changes size, the
    /// fallback clamp is only visible on the next frame.
    ///
    /// When the pointer is over the viewport, this consumes the frame's scroll input. Nested
    /// scrollables are not prioritized: the outermost scrollable under the pointer consumes it.
    pub fn scrollable<R>(
        &mut self,
        size: Vec2,
        state: &mut ScrollState,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> (R, ScrollResponse) {
        let size = sanitize_size(size);
        let rect = next_rect(&self.regions, size);
        let hovered = pointer_over(&self.regions, &self.ctx.input, rect);
        let known_max_offset = state
            .content_size
            .map(|content_size| (content_size - size).max(Vec2::ZERO));

        if !state.offset.is_finite() {
            state.offset = Vec2::ZERO;
        }
        state.offset = state.offset.max(Vec2::ZERO);
        if let Some(max_offset) = known_max_offset {
            state.offset = state.offset.min(max_offset);
        }

        // Inner scrollable consumes input first.
        let (result, pane) = self.pane(size, -state.offset, build);

        if hovered && self.scroll_remaining != Vec2::ZERO {
            // RawInput uses screen axes (positive y scrolls up), while `offset` counts how far the
            // content has moved left/up.
            let delta = Vec2::new(self.scroll_remaining.x, -self.scroll_remaining.y);
            state.offset = (state.offset + delta).max(Vec2::ZERO);
            if let Some(max_offset) = known_max_offset {
                state.offset = state.offset.min(max_offset);
            }
            self.scroll_remaining = Vec2::ZERO;
        }

        let max_offset = (pane.content_size - size).max(Vec2::ZERO);
        state.content_size = Some(pane.content_size);
        state.offset = state.offset.clamp(Vec2::ZERO, max_offset);

        (
            result,
            ScrollResponse {
                rect,
                content_size: pane.content_size,
                max_offset,
                hovered,
            },
        )
    }
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
    fn scrollable_consumes_wheel_while_hovered() {
        let mut context = context();
        let mut state = ScrollState {
            offset: vec2(0.0, 50.0),
            content_size: Some(vec2(200.0, 300.0)),
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
    fn scrollable_preclamps_wheel_with_previous_content_size() {
        let mut context = context();
        let mut state = ScrollState {
            offset: vec2(0.0, 190.0),
            content_size: Some(vec2(200.0, 300.0)),
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

        // The wheel asks for 210, but the previous content size clamps it to 200 before layout.
        assert_eq!(state.offset, vec2(0.0, 200.0));
        assert_eq!(inside.min, pos2(0.0, -200.0));
    }

    #[test]
    fn scrollable_clamps_first_frame_without_a_previous_content_size() {
        let mut context = context();
        let mut state = ScrollState {
            offset: vec2(1000.0, 1000.0),
            content_size: None,
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
    fn nested_scrollables_let_the_outer_one_consume_the_wheel() {
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

        assert_eq!(outer.offset, vec2(0.0, 20.0));
        assert_eq!(inner.offset, Vec2::ZERO);
    }
}
