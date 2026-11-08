//! Input translation from winit's [`WindowEvent`] to [`tinim_gui::RawInput`].
//!
//! This crate does not create a window, a wgpu surface or an event loop. The application owns
//! those and forwards events here; see the `tinim-demo-native` and `tinim-demo-web` crates.

use tinim_gui::{Modifiers, Pos2, RawInput, Vec2, pos2, vec2};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};

/// One wheel line is translated to this many logical points.
const LINE_HEIGHT: f32 = 50.0;

/// Accumulates winit events until the next frame.
#[derive(Debug)]
pub struct WinitInput {
    scale_factor: f32,
    pointer: Option<Pos2>,
    primary_down: bool,
    primary_pressed: bool,
    primary_released: bool,
    scroll: Vec2,
    modifiers: Modifiers,
    text: String,
    /// The touch currently driving the primary pointer, if any. The GUI has a single pointer,
    /// so extra fingers are ignored until this one is released.
    active_touch: Option<u64>,
}

impl Default for WinitInput {
    fn default() -> Self {
        Self::new(1.0)
    }
}

impl WinitInput {
    /// `scale_factor` is physical pixels per logical point, usually `window.scale_factor()`.
    pub fn new(scale_factor: f32) -> Self {
        Self {
            scale_factor: sanitize_scale(scale_factor),
            pointer: None,
            primary_down: false,
            primary_pressed: false,
            primary_released: false,
            scroll: Vec2::ZERO,
            modifiers: Modifiers::default(),
            text: String::new(),
            active_touch: None,
        }
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// Feed one window event. Events not related to input are ignored.
    pub fn on_window_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer = Some(pos2(
                    position.x as f32 / self.scale_factor,
                    position.y as f32 / self.scale_factor,
                ));
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer = None;
            }
            WindowEvent::MouseInput { state, button, .. } if *button == MouseButton::Left => {
                match state {
                    ElementState::Pressed => {
                        if !self.primary_down {
                            self.primary_pressed = true;
                        }
                        self.primary_down = true;
                    }
                    ElementState::Released => {
                        if self.primary_down {
                            self.primary_released = true;
                        }
                        self.primary_down = false;
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.scroll += match delta {
                    MouseScrollDelta::LineDelta(x, y) => vec2(x * LINE_HEIGHT, y * LINE_HEIGHT),
                    MouseScrollDelta::PixelDelta(position) => vec2(
                        position.x as f32 / self.scale_factor,
                        position.y as f32 / self.scale_factor,
                    ),
                };
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                self.modifiers = Modifiers {
                    shift: state.shift_key(),
                    ctrl: state.control_key(),
                    alt: state.alt_key(),
                    logo: state.super_key(),
                };
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed
                    && !event.repeat
                    && let Some(text) = &event.text
                {
                    self.text.push_str(text);
                }
            }
            WindowEvent::Touch(touch) => self.on_touch(touch),
            WindowEvent::Focused(false) => {
                self.primary_down = false;
                self.primary_pressed = false;
                self.primary_released = false;
                self.active_touch = None;
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor = sanitize_scale(*scale_factor as f32);
                // The old logical pointer position is no longer meaningful.
                self.pointer = None;
            }
            _ => {}
        }
    }

    /// Translates a touch into the primary pointer. The first finger becomes the pointer; other
    /// fingers are ignored until it is released, because [`RawInput`] only has one pointer.
    fn on_touch(&mut self, touch: &winit::event::Touch) {
        let position = pos2(
            touch.location.x as f32 / self.scale_factor,
            touch.location.y as f32 / self.scale_factor,
        );

        match touch.phase {
            TouchPhase::Started => {
                if self.active_touch.is_none() && !self.primary_down {
                    self.active_touch = Some(touch.id);
                    self.pointer = Some(position);
                    self.primary_pressed = true;
                    self.primary_down = true;
                }
            }
            TouchPhase::Moved => {
                if self.active_touch == Some(touch.id) {
                    self.pointer = Some(position);
                }
            }
            TouchPhase::Ended => {
                if self.active_touch == Some(touch.id) {
                    self.active_touch = None;
                    self.pointer = Some(position);
                    self.primary_down = false;
                    self.primary_released = true;
                }
            }
            TouchPhase::Cancelled => {
                if self.active_touch == Some(touch.id) {
                    self.active_touch = None;
                    // A cancelled touch must not look like a click.
                    self.pointer = None;
                    self.primary_down = false;
                    self.primary_pressed = false;
                    self.primary_released = false;
                }
            }
        }
    }

    /// Takes this frame's input. Edges, scroll and text are cleared; pointer, held buttons and
    /// modifiers are kept.
    pub fn take(&mut self) -> RawInput {
        RawInput {
            pointer: self.pointer,
            primary_down: self.primary_down,
            primary_pressed: std::mem::take(&mut self.primary_pressed),
            primary_released: std::mem::take(&mut self.primary_released),
            scroll: std::mem::take(&mut self.scroll),
            modifiers: self.modifiers,
            text: std::mem::take(&mut self.text),
        }
    }
}

fn sanitize_scale(scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

pub use winit;

#[cfg(test)]
mod tests {
    use super::*;
    use winit::dpi::PhysicalPosition;
    use winit::event::{DeviceId, Touch};

    fn touch(id: u64, phase: TouchPhase, x: f64, y: f64) -> WindowEvent {
        WindowEvent::Touch(Touch {
            device_id: DeviceId::dummy(),
            phase,
            location: PhysicalPosition::new(x, y),
            force: None,
            id,
        })
    }

    #[test]
    fn touch_becomes_the_primary_pointer() {
        let mut input = WinitInput::new(2.0);

        input.on_window_event(&touch(7, TouchPhase::Started, 100.0, 50.0));
        let frame = input.take();
        assert_eq!(frame.pointer, Some(pos2(50.0, 25.0)));
        assert!(frame.primary_down);
        assert!(frame.primary_pressed);
        assert!(!frame.primary_released);

        input.on_window_event(&touch(7, TouchPhase::Moved, 120.0, 90.0));
        let frame = input.take();
        assert_eq!(frame.pointer, Some(pos2(60.0, 45.0)));
        assert!(frame.primary_down);
        assert!(!frame.primary_pressed);
        assert!(!frame.primary_released);

        input.on_window_event(&touch(7, TouchPhase::Ended, 120.0, 90.0));
        let frame = input.take();
        assert_eq!(frame.pointer, Some(pos2(60.0, 45.0)));
        assert!(!frame.primary_down);
        assert!(!frame.primary_pressed);
        assert!(frame.primary_released);
    }

    #[test]
    fn extra_fingers_are_ignored() {
        let mut input = WinitInput::new(1.0);
        input.on_window_event(&touch(1, TouchPhase::Started, 10.0, 10.0));
        input.on_window_event(&touch(2, TouchPhase::Started, 90.0, 90.0));
        input.on_window_event(&touch(2, TouchPhase::Ended, 90.0, 90.0));
        let frame = input.take();
        assert_eq!(frame.pointer, Some(pos2(10.0, 10.0)));
        assert!(frame.primary_down);
        assert!(frame.primary_pressed);
        assert!(!frame.primary_released);

        input.on_window_event(&touch(1, TouchPhase::Ended, 10.0, 10.0));
        let frame = input.take();
        assert!(!frame.primary_down);
        assert!(frame.primary_released);
    }

    #[test]
    fn cancelled_touch_does_not_click() {
        let mut input = WinitInput::new(1.0);
        input.on_window_event(&touch(1, TouchPhase::Started, 10.0, 10.0));
        input.on_window_event(&touch(1, TouchPhase::Cancelled, 10.0, 10.0));
        let frame = input.take();
        assert_eq!(frame.pointer, None);
        assert!(!frame.primary_down);
        assert!(!frame.primary_pressed);
        assert!(!frame.primary_released);
    }

    #[test]
    fn touch_does_not_disturb_a_held_mouse_button() {
        let mut input = WinitInput::new(1.0);
        input.on_window_event(&WindowEvent::MouseInput {
            device_id: DeviceId::dummy(),
            state: ElementState::Pressed,
            button: MouseButton::Left,
        });
        input.on_window_event(&touch(1, TouchPhase::Started, 10.0, 10.0));
        input.on_window_event(&touch(1, TouchPhase::Ended, 10.0, 10.0));
        let frame = input.take();
        assert!(frame.primary_down);
        assert!(frame.primary_pressed);
        assert!(!frame.primary_released);
        assert_eq!(frame.pointer, None);
    }
}
