//! Input translation from winit's [`WindowEvent`] to [`tinim_gui::RawInput`].
//!
//! Touches drive the primary pointer: a tap clicks on release, and dragging scrolls.
//!
//! This crate does not create a window, a wgpu surface or an event loop. The application owns
//! those and forwards events here; see the `tinim-demo-native` and `tinim-demo-web` crates.
//!
//! With the `clipboard` feature, the `Clipboard` type wraps a long-lived `copypasta` handle. The
//! application reads it when it sees a paste shortcut and writes [`PlatformOutput::copy_text`]
//! after building a frame.
//!
//! On Linux, the `x11` feature also enables copypasta's X11 backend, and copypasta's own default
//! features stay disabled so X11-only builds do not pull in its Wayland dependencies. Without
//! `x11`, `Clipboard` is a no-op.

use tinim_gui::{CursorIcon, Key, Modifiers, PlatformOutput, Pos2, RawInput, Vec2, pos2, vec2};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::window::{CursorIcon as WinitCursorIcon, Window};

#[cfg(feature = "clipboard")]
mod clipboard;

#[cfg(feature = "clipboard")]
pub use clipboard::Clipboard;

/// One wheel line is translated to this many logical points.
const LINE_HEIGHT: f32 = 50.0;

/// How far a finger may wander before a touch becomes a drag instead of a click, in logical points.
const TOUCH_SLOP: f32 = 8.0;

/// Applies the platform output of a frame to a winit window.
///
/// Each part is applied through its own state, so future outputs such as IME state can be added
/// without turning this into one monolithic handler. Create one `WinitOutput` per window.
#[derive(Debug, Default)]
pub struct WinitOutput {
    cursor: CursorState,
}

impl WinitOutput {
    /// Applies `output` to `window`.
    ///
    /// Only values that differ from the previously applied state reach the platform.
    pub fn apply(&mut self, window: &Window, output: &PlatformOutput) {
        self.cursor.apply(window, output.cursor_icon);
    }

    /// Forgets the applied state so the next [`WinitOutput::apply`] updates the window even when
    /// nothing changed. Use this after the platform may have reset its state.
    pub fn invalidate(&mut self) {
        self.cursor.invalidate();
    }
}

/// Applies the cursor requested by [`PlatformOutput::cursor_icon`] to a winit window.
///
/// The requested icon is compared with the last applied icon so the platform cursor is only
/// changed when it actually differs. Create one `CursorState` per window.
#[derive(Debug, Default)]
pub struct CursorState {
    current: CursorIcon,
    /// Forces the next apply even when the requested icon equals `current`.
    dirty: bool,
}

impl CursorState {
    /// Applies `icon` when it differs from the icon applied previously.
    pub fn apply(&mut self, window: &Window, icon: CursorIcon) {
        if !self.dirty && self.current == icon {
            return;
        }
        window.set_cursor(cursor_icon(icon));
        self.current = icon;
        self.dirty = false;
    }

    /// Forgets the last applied icon, forcing the next [`CursorState::apply`] to update the
    /// window. Use this after the platform may have reset the cursor, or when reusing the state
    /// with a different window.
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }
}

/// Converts a backend-neutral [`CursorIcon`] to winit's cursor icon.
pub fn cursor_icon(icon: CursorIcon) -> WinitCursorIcon {
    match icon {
        CursorIcon::Default => WinitCursorIcon::Default,
        CursorIcon::Pointer => WinitCursorIcon::Pointer,
        CursorIcon::Text => WinitCursorIcon::Text,
    }
}

/// Accumulates winit events until the next frame.
#[derive(Debug)]
pub struct WinitInput {
    scale_factor: f32,
    pointer: Option<Pos2>,
    primary_down: bool,
    primary_pressed: bool,
    primary_released: bool,
    scroll: Vec2,
    /// Whether this frame's `scroll` came from a touch drag.
    scroll_dragging: bool,
    modifiers: Modifiers,
    text: String,
    keys: Vec<Key>,
    /// The touch currently driving the primary pointer, if any. The GUI has a single pointer,
    /// so extra fingers are ignored until this one is released.
    active_touch: Option<u64>,
    /// Logical position where the active touch started.
    touch_start: Pos2,
    /// Logical position of the most recent touch event.
    touch_last: Pos2,
    /// Whether the active touch has moved far enough to scroll rather than click.
    touch_dragged: bool,
    /// Timestamp of the previous [`Self::take`], used to report the frame duration.
    last_frame: web_time::Instant,
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
            scroll_dragging: false,
            modifiers: Modifiers::default(),
            text: String::new(),
            keys: Vec::new(),
            active_touch: None,
            touch_start: Pos2::ZERO,
            touch_last: Pos2::ZERO,
            touch_dragged: false,
            last_frame: web_time::Instant::now(),
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
                if event.state == ElementState::Pressed {
                    self.on_key_pressed(&event.logical_key, event.text.as_deref(), event.repeat);
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
    ///
    /// A tap clicks on release, so dragging can scroll without also pressing whatever was under
    /// the finger when it landed.
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
                    self.touch_start = position;
                    self.touch_last = position;
                    self.touch_dragged = false;
                    self.primary_down = true;
                }
            }
            TouchPhase::Moved => {
                if self.active_touch == Some(touch.id) {
                    self.pointer = Some(position);
                    let delta = position - self.touch_last;
                    self.touch_last = position;
                    self.touch_scroll(delta);
                }
            }
            TouchPhase::Ended => {
                if self.active_touch == Some(touch.id) {
                    self.pointer = Some(position);
                    let delta = position - self.touch_last;
                    self.touch_last = position;
                    self.touch_scroll(delta);

                    self.active_touch = None;
                    self.primary_down = false;
                    self.primary_released = true;
                    if !self.touch_dragged {
                        self.primary_pressed = true;
                    }
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

    /// Converts finger movement into scroll deltas, once the touch has moved beyond [`TOUCH_SLOP`].
    /// Content follows the finger, so dragging right/up moves the content right/up.
    fn touch_scroll(&mut self, delta: Pos2) {
        if !self.touch_dragged {
            if (self.touch_last - self.touch_start).length() < TOUCH_SLOP {
                return;
            }
            // Start scrolling from the press point so the movement that crossed the slop is
            // not lost.
            self.touch_dragged = true;
            let moved = self.touch_last - self.touch_start;
            // `scroll` uses wheel axes: positive x scrolls right and positive y scrolls up.
            self.scroll += vec2(-moved.x, moved.y);
            self.scroll_dragging = true;
        } else {
            self.scroll += vec2(-delta.x, delta.y);
            self.scroll_dragging = true;
        }
    }

    /// Handles one pressed key. Kept separate from [`Self::on_window_event`] so tests can feed
    /// logical keys without constructing winit's `KeyEvent`, which has private fields.
    fn on_key_pressed(
        &mut self,
        logical_key: &winit::keyboard::Key,
        text: Option<&str>,
        repeat: bool,
    ) {
        if let Some(key) = translate_key(logical_key) {
            // Repeats are useful for held Backspace/Delete/arrow keys.
            self.keys.push(key);
        }
        if !repeat && let Some(text) = text {
            // Enter, Backspace and friends may carry control characters as text.
            self.text.extend(text.chars().filter(|ch| !ch.is_control()));
        }
    }

    /// Takes this frame's input. Edges, scroll and text are cleared; pointer, held buttons and
    /// modifiers are kept.
    pub fn take(&mut self) -> RawInput {
        let now = web_time::Instant::now();
        // Clamp the step so a stalled tab does not make animations jump.
        let dt = (now - self.last_frame).as_secs_f32().clamp(0.0001, 0.1);
        self.last_frame = now;

        RawInput {
            pointer: self.pointer,
            primary_down: self.primary_down,
            primary_pressed: std::mem::take(&mut self.primary_pressed),
            primary_released: std::mem::take(&mut self.primary_released),
            scroll: std::mem::take(&mut self.scroll),
            modifiers: self.modifiers,
            text: std::mem::take(&mut self.text),
            keys: std::mem::take(&mut self.keys),
            pasted_text: None,
            scroll_dragging: std::mem::take(&mut self.scroll_dragging),
            dt,
        }
    }
}

/// Translates a winit logical key into the subset the GUI understands.
fn translate_key(key: &winit::keyboard::Key) -> Option<Key> {
    use winit::keyboard::{Key as WinitKey, NamedKey};

    match key {
        WinitKey::Named(NamedKey::Backspace) => Some(Key::Backspace),
        WinitKey::Named(NamedKey::Delete) => Some(Key::Delete),
        WinitKey::Named(NamedKey::Enter) => Some(Key::Enter),
        WinitKey::Named(NamedKey::Escape) => Some(Key::Escape),
        WinitKey::Named(NamedKey::Tab) => Some(Key::Tab),
        WinitKey::Named(NamedKey::ArrowLeft) => Some(Key::ArrowLeft),
        WinitKey::Named(NamedKey::ArrowRight) => Some(Key::ArrowRight),
        WinitKey::Named(NamedKey::Home) => Some(Key::Home),
        WinitKey::Named(NamedKey::End) => Some(Key::End),
        WinitKey::Character(text) => text
            .chars()
            .next()
            .map(|ch| Key::Character(ch.to_ascii_lowercase())),
        _ => None,
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
    use winit::keyboard::{Key as WinitKey, NamedKey};

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
    fn tap_clicks_the_primary_pointer_on_release() {
        let mut input = WinitInput::new(1.0);

        input.on_window_event(&touch(7, TouchPhase::Started, 10.0, 20.0));
        let frame = input.take();
        assert_eq!(frame.pointer, Some(pos2(10.0, 20.0)));
        assert!(frame.primary_down);
        assert!(!frame.primary_pressed);
        assert!(!frame.primary_released);

        input.on_window_event(&touch(7, TouchPhase::Ended, 10.0, 20.0));
        let frame = input.take();
        assert_eq!(frame.pointer, Some(pos2(10.0, 20.0)));
        assert!(!frame.primary_down);
        assert!(frame.primary_pressed);
        assert!(frame.primary_released);
        assert_eq!(frame.scroll, Vec2::ZERO);
    }

    #[test]
    fn dragging_scrolls_and_does_not_click() {
        let mut input = WinitInput::new(1.0);

        input.on_window_event(&touch(7, TouchPhase::Started, 100.0, 100.0));
        input.take();

        // Move up by 20 points: the content follows the finger, so the viewport scrolls down.
        input.on_window_event(&touch(7, TouchPhase::Moved, 100.0, 80.0));
        let frame = input.take();
        assert_eq!(frame.scroll, vec2(0.0, -20.0));
        assert!(frame.scroll_dragging);
        assert!(frame.dt > 0.0 && frame.dt <= 0.1);
        assert!(!frame.primary_pressed);

        input.on_window_event(&touch(7, TouchPhase::Moved, 100.0, 60.0));
        let frame = input.take();
        assert_eq!(frame.scroll, vec2(0.0, -20.0));

        input.on_window_event(&touch(7, TouchPhase::Ended, 100.0, 60.0));
        let frame = input.take();
        assert!(!frame.primary_down);
        assert!(!frame.primary_pressed);
        assert!(frame.primary_released);
    }

    #[test]
    fn touch_scroll_uses_wheel_axes() {
        let mut input = WinitInput::new(2.0);
        input.on_window_event(&touch(7, TouchPhase::Started, 100.0, 100.0));
        input.take();

        // Logical delta is (+10, +20); scrolling right/up is (+x, -y) in wheel axes.
        input.on_window_event(&touch(7, TouchPhase::Moved, 120.0, 140.0));
        let frame = input.take();
        assert_eq!(frame.scroll, vec2(-10.0, 20.0));
    }

    #[test]
    fn small_touch_movement_is_still_a_tap() {
        let mut input = WinitInput::new(1.0);
        input.on_window_event(&touch(7, TouchPhase::Started, 10.0, 10.0));
        input.on_window_event(&touch(7, TouchPhase::Moved, 12.0, 12.0));
        let frame = input.take();
        assert_eq!(frame.scroll, Vec2::ZERO);
        assert!(!frame.primary_pressed);

        input.on_window_event(&touch(7, TouchPhase::Ended, 12.0, 12.0));
        let frame = input.take();
        assert_eq!(frame.scroll, Vec2::ZERO);
        assert!(frame.primary_pressed);
        assert!(frame.primary_released);
    }

    #[test]
    fn extra_fingers_are_ignored() {
        let mut input = WinitInput::new(1.0);
        input.on_window_event(&touch(1, TouchPhase::Started, 10.0, 10.0));
        input.on_window_event(&touch(2, TouchPhase::Started, 90.0, 90.0));
        input.on_window_event(&touch(2, TouchPhase::Moved, 80.0, 80.0));
        input.on_window_event(&touch(2, TouchPhase::Ended, 80.0, 80.0));
        let frame = input.take();
        assert_eq!(frame.pointer, Some(pos2(10.0, 10.0)));
        assert_eq!(frame.scroll, Vec2::ZERO);
        assert!(frame.primary_down);
        assert!(!frame.primary_pressed);
        assert!(!frame.primary_released);

        input.on_window_event(&touch(1, TouchPhase::Ended, 10.0, 10.0));
        let frame = input.take();
        assert!(!frame.primary_down);
        assert!(frame.primary_pressed);
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

    #[test]
    fn named_keys_are_translated_and_text_is_kept() {
        let mut input = WinitInput::new(1.0);
        input.on_key_pressed(&WinitKey::Named(NamedKey::Backspace), Some("\u{8}"), false);
        input.on_key_pressed(&WinitKey::Named(NamedKey::ArrowLeft), None, false);
        input.on_key_pressed(&WinitKey::Character("A".into()), Some("A"), false);

        let frame = input.take();
        assert_eq!(
            frame.keys,
            vec![Key::Backspace, Key::ArrowLeft, Key::Character('a')]
        );
        assert_eq!(frame.text, "A", "control characters are not text");
    }

    #[test]
    fn key_repeats_are_reported_but_text_is_not_duplicated() {
        let mut input = WinitInput::new(1.0);
        input.on_key_pressed(&WinitKey::Named(NamedKey::Backspace), None, true);
        input.on_key_pressed(&WinitKey::Character("a".into()), Some("a"), true);

        let frame = input.take();
        assert_eq!(frame.keys, vec![Key::Backspace, Key::Character('a')]);
        assert!(frame.text.is_empty());
    }
}
