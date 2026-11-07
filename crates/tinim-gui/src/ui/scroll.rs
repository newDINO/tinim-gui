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
    /// When `state.content_size` is known from a previous frame, the offset is clamped before
    /// layout, so an out-of-range position is corrected in the same frame. The content size is
    /// measured during layout and stored back in `state`; the offset is clamped again afterwards
    /// to handle content that changed size.
    ///
    /// Wheel input is applied after the content is built, so nested scrollables can take
    /// priority. The innermost scrollable under the pointer scrolls first and keeps as much of
    /// the delta as it can use. Any remainder, because it hit a boundary or its content does not
    /// overflow, is left for an enclosing scrollable. Since the wheel is applied after layout,
    /// its visual effect appears on the next frame.
    pub fn scrollable<R>(
        &mut self,
        size: Vec2,
        state: &mut ScrollState,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> (R, ScrollResponse) {
        let size = sanitize_size(size);
        let rect = next_rect(&self.region, size, self.spacing());
        let hovered = pointer_over(&self.region, &self.ctx.input, rect);
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

        // Inner scrollables get the wheel first; whatever they cannot use stays in
        // `scroll_remaining` for an enclosing scrollable.
        let (result, pane) = self.pane(size, -state.offset, build);
        let max_offset = (pane.content_size - size).max(Vec2::ZERO);
        state.content_size = Some(pane.content_size);

        // Correct an offset that the new content size no longer permits before applying the wheel,
        // otherwise the discarded distance would be forwarded to an enclosing scrollable.
        state.offset = state.offset.clamp(Vec2::ZERO, max_offset);

        if hovered && self.scroll_remaining != Vec2::ZERO {
            // RawInput uses screen axes (positive y scrolls up), while `offset` counts how far the
            // content has moved left/up.
            let delta = Vec2::new(self.scroll_remaining.x, -self.scroll_remaining.y);
            let desired = state.offset + delta;
            state.offset = desired.clamp(Vec2::ZERO, max_offset);

            // Leave the part that could not be applied for an enclosing scrollable, converted
            // back to screen axes.
            let remaining = desired - state.offset;
            self.scroll_remaining = Vec2::new(remaining.x, -remaining.y);
        }

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
    fn scrollable_clamps_wheel_to_the_content_boundary() {
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
    fn shrinking_inner_content_does_not_forward_stale_offset_as_wheel_delta() {
        let mut context = context();
        let mut outer = ScrollState::default();
        let mut inner = ScrollState {
            offset: vec2(0.0, 200.0),
            content_size: Some(vec2(10.0, 300.0)),
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
}
