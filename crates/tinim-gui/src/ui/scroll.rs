//! Scrollable content in a clipped viewport.

use tinim_paint::Vec2;

use super::Ui;
use super::layout::{next_rect, pointer_over, sanitize_size};
use super::response::{ScrollResponse, ScrollState};

/// Velocity below which a fling stops, in logical points per second.
const REST_SPEED: f32 = 10.0;
/// Overscroll below which the scroll snaps back to the boundary, in logical points.
const REST_DISTANCE: f32 = 0.25;
/// Largest velocity derived from one frame of dragging. Keeps a short, fast flick reasonable.
const MAX_SCROLL_SPEED: f32 = 4000.0;
/// Inertia friction. Larger values stop a fling sooner.
const SCROLL_FRICTION: f32 = 4.0;
/// Fraction of a drag that still moves the content once it is past a boundary.
const OVERSCROLL_RESISTANCE: f32 = 0.35;
/// Spring stiffness and damping used to return from overscroll.
const OVERSCROLL_STIFFNESS: f32 = 180.0;
const OVERSCROLL_DAMPING: f32 = 22.0;
/// Physics substep, so a long frame cannot make the spring unstable.
const MAX_PHYSICS_STEP: f32 = 1.0 / 120.0;

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
    /// Inertia and edge bounce are integrated before layout, using the frame time in
    /// [`RawInput::dt`](crate::RawInput::dt). While a touch drag owns the scroll area, the finger
    /// controls the position instead; releasing it turns the last drag velocity into a fling.
    ///
    /// Wheel and drag input is applied after the content is built, so nested scrollables can take
    /// priority. The innermost scrollable under the pointer scrolls first and keeps as much of
    /// the delta as it can use. For the wheel, any remainder is left for an enclosing scrollable.
    /// A touch drag instead rubber-bands at its bounds. Since input is applied after layout, its
    /// visual effect appears on the next frame.
    pub fn scrollable<R>(
        &mut self,
        size: Vec2,
        state: &mut ScrollState,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> (R, ScrollResponse) {
        let size = sanitize_size(size);
        let rect = next_rect(&self.layout, size, self.spacing());
        let hovered = pointer_over(self.clip, &self.ctx.input, rect);
        let dt = sanitize_dt(self.ctx.input.dt);
        let pointer_down = self.ctx.input.primary_down;
        let known_max_offset = state
            .content_size
            .map(|content_size| (content_size - size).max(Vec2::ZERO));

        sanitize_state(state);
        if let Some(max_offset) = known_max_offset {
            state.offset = state.offset.min(max_offset);
        }

        // Animate before layout so this frame reflects the new position. A drag owns the position
        // instead; it is released below, after its final delta has been applied.
        if !state.dragging {
            animate_scroll(state, dt, known_max_offset.unwrap_or(Vec2::ZERO));
        }

        // Inner scrollables get input first; whatever they cannot use stays in `scroll_remaining`
        // for an enclosing scrollable.
        let (result, pane) = self.pane(size, -(state.offset + state.overscroll), build);
        let max_offset = (pane.content_size - size).max(Vec2::ZERO);
        state.content_size = Some(pane.content_size);

        // Correct an offset that the new content size no longer permits before applying input,
        // otherwise the discarded distance would look like scroll input.
        state.offset = state.offset.clamp(Vec2::ZERO, max_offset);

        if hovered && self.scroll_remaining != Vec2::ZERO {
            // RawInput uses screen axes (positive y scrolls up), while `offset` counts how far the
            // content has moved left/up.
            let delta = Vec2::new(self.scroll_remaining.x, -self.scroll_remaining.y);
            if self.ctx.input.scroll_dragging {
                state.dragging = true;
                let remaining = drag_scroll(state, delta, max_offset);
                if dt > 0.0 {
                    state.velocity = (delta / dt).clamp_length_max(MAX_SCROLL_SPEED);
                }
                self.scroll_remaining = Vec2::new(remaining.x, -remaining.y);
            } else {
                let desired = state.offset + delta;
                state.offset = desired.clamp(Vec2::ZERO, max_offset);

                // Leave the part that could not be applied for an enclosing scrollable, converted
                // back to screen axes.
                let remaining = desired - state.offset;
                self.scroll_remaining = Vec2::new(remaining.x, -remaining.y);
            }
        } else if state.dragging && pointer_down {
            // The finger is down but did not move this frame, so do not fling from a stale
            // velocity when it is eventually released.
            state.velocity = Vec2::ZERO;
        }

        if state.dragging && !pointer_down {
            state.dragging = false;
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

fn sanitize_state(state: &mut ScrollState) {
    if !state.offset.is_finite() {
        state.offset = Vec2::ZERO;
    }
    if !state.overscroll.is_finite() {
        state.overscroll = Vec2::ZERO;
    }
    if !state.velocity.is_finite() {
        state.velocity = Vec2::ZERO;
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

/// Applies a touch drag, rubber-banding past the content bounds. Returns the delta that should be
/// offered to an enclosing scrollable: only axes whose content cannot scroll at all.
fn drag_scroll(state: &mut ScrollState, delta: Vec2, max_offset: Vec2) -> Vec2 {
    let position = state.offset + state.overscroll;
    let next = Vec2::new(
        if max_offset.x > 0.0 {
            drag_axis(position.x, delta.x, max_offset.x)
        } else {
            position.x
        },
        if max_offset.y > 0.0 {
            drag_axis(position.y, delta.y, max_offset.y)
        } else {
            position.y
        },
    );
    let remaining = Vec2::new(
        if max_offset.x > 0.0 { 0.0 } else { delta.x },
        if max_offset.y > 0.0 { 0.0 } else { delta.y },
    );

    state.offset = next.clamp(Vec2::ZERO, max_offset);
    state.overscroll = next - state.offset;
    remaining
}

fn drag_axis(position: f32, delta: f32, max: f32) -> f32 {
    let excess = position - position.clamp(0.0, max);
    let target = position + delta;
    let target_bound = target.clamp(0.0, max);
    let target_excess = target - target_bound;

    if excess == 0.0 {
        // Free movement inside the content, damped movement past the boundary.
        target_bound + target_excess * OVERSCROLL_RESISTANCE
    } else if target_excess != 0.0 && (target_excess > excess) == (excess > 0.0) {
        // Pulling further past the same boundary.
        position + delta * OVERSCROLL_RESISTANCE
    } else {
        // Returning towards the content, or crossing back inside.
        target
    }
}

fn animate_scroll(state: &mut ScrollState, dt: f32, max_offset: Vec2) {
    if dt <= 0.0 || (state.velocity == Vec2::ZERO && state.overscroll == Vec2::ZERO) {
        return;
    }

    let steps = (dt / MAX_PHYSICS_STEP).ceil() as u32;
    let step = dt / steps.max(1) as f32;

    for _ in 0..steps.max(1) {
        let position = state.offset + state.overscroll;
        let excess = position - position.clamp(Vec2::ZERO, max_offset);

        if excess == Vec2::ZERO {
            state.velocity *= (-SCROLL_FRICTION * step).exp();
        } else {
            state.velocity -= excess * (OVERSCROLL_STIFFNESS * step);
            state.velocity *= (-OVERSCROLL_DAMPING * step).exp();
        }

        let position = position + state.velocity * step;
        state.offset = position.clamp(Vec2::ZERO, max_offset);
        state.overscroll = position - state.offset;
    }

    if state.overscroll.length() < REST_DISTANCE && state.velocity.length() < REST_SPEED {
        state.offset = state.offset.clamp(Vec2::ZERO, max_offset);
        state.overscroll = Vec2::ZERO;
        state.velocity = Vec2::ZERO;
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
    fn dragging_past_the_end_rubber_bands() {
        let mut state = ScrollState {
            offset: vec2(0.0, 200.0),
            ..Default::default()
        };
        let remaining = drag_scroll(&mut state, vec2(0.0, 50.0), vec2(0.0, 200.0));

        assert_eq!(remaining, Vec2::ZERO);
        assert_eq!(state.offset, vec2(0.0, 200.0));
        assert!(state.overscroll.y > 0.0 && state.overscroll.y < 50.0);
    }

    #[test]
    fn overscroll_springs_back_to_the_boundary() {
        let mut state = ScrollState {
            offset: vec2(0.0, 200.0),
            overscroll: vec2(0.0, 30.0),
            ..Default::default()
        };

        for _ in 0..240 {
            animate_scroll(&mut state, 1.0 / 60.0, vec2(0.0, 200.0));
        }

        assert_eq!(state.offset, vec2(0.0, 200.0));
        assert_eq!(state.overscroll, Vec2::ZERO);
        assert_eq!(state.velocity, Vec2::ZERO);
    }
}
