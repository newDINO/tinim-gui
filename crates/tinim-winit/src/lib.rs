//! Input translation from winit's [`WindowEvent`] to [`tinim_gui::RawInput`].
//!
//! This crate does not create a window, a wgpu surface or an event loop. The application owns
//! those and forwards events here; see `examples/demo.rs`.

use tinim_gui::{Modifiers, Pos2, RawInput, Vec2, pos2, vec2};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};

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
            WindowEvent::Focused(false) => {
                self.primary_down = false;
                self.primary_pressed = false;
                self.primary_released = false;
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor = sanitize_scale(*scale_factor as f32);
                // The old logical pointer position is no longer meaningful.
                self.pointer = None;
            }
            _ => {}
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
