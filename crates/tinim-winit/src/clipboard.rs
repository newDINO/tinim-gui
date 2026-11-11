//! Optional system clipboard access for native backends.
//!
//! copypasta exposes `ClipboardContext` only for targets that have a backend. On Linux the
//! alias requires its `x11` feature, which `tinim-winit` forwards from its own `x11` feature.
//! Other combinations fall back to a no-op so the `clipboard` feature always compiles.

use imp::Inner;

/// A single, long-lived system clipboard handle.
///
/// Some platforms, notably X11, require the process that set the clipboard contents to keep the
/// handle alive. Create one per application instead of one per frame.
///
/// If the platform clipboard cannot be initialized, reads and writes are silently ignored.
/// That includes Linux builds without a clipboard backend feature.
pub struct Clipboard {
    inner: Inner,
}

impl Clipboard {
    /// Connects to the system clipboard, or degrades to a no-op when unavailable.
    pub fn new() -> Self {
        Self {
            inner: Inner::new(),
        }
    }

    /// Reads the current clipboard contents, or `None` when unavailable.
    pub fn read(&mut self) -> Option<String> {
        self.inner.read()
    }

    /// Replaces the clipboard contents. Errors are ignored.
    pub fn write(&mut self, text: &str) {
        self.inner.write(text);
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(
    windows,
    target_os = "macos",
    target_os = "android",
    target_os = "ios",
    all(
        unix,
        not(any(
            target_os = "macos",
            target_os = "android",
            target_os = "ios",
            target_os = "emscripten"
        )),
        feature = "x11"
    ),
    not(any(
        unix,
        windows,
        target_os = "macos",
        target_os = "android",
        target_os = "ios",
        target_os = "emscripten"
    ))
))]
mod imp {
    use copypasta::{ClipboardContext, ClipboardProvider};

    pub struct Inner {
        context: Option<ClipboardContext>,
    }

    impl Inner {
        pub fn new() -> Self {
            Self {
                context: ClipboardContext::new().ok(),
            }
        }

        pub fn read(&mut self) -> Option<String> {
            self.context.as_mut()?.get_contents().ok()
        }

        pub fn write(&mut self, text: &str) {
            if let Some(context) = self.context.as_mut() {
                let _ = context.set_contents(text.to_owned());
            }
        }
    }
}

#[cfg(not(any(
    windows,
    target_os = "macos",
    target_os = "android",
    target_os = "ios",
    all(
        unix,
        not(any(
            target_os = "macos",
            target_os = "android",
            target_os = "ios",
            target_os = "emscripten"
        )),
        feature = "x11"
    ),
    not(any(
        unix,
        windows,
        target_os = "macos",
        target_os = "android",
        target_os = "ios",
        target_os = "emscripten"
    ))
)))]
mod imp {
    pub struct Inner;

    impl Inner {
        pub fn new() -> Self {
            Self
        }

        pub fn read(&mut self) -> Option<String> {
            None
        }

        pub fn write(&mut self, _text: &str) {}
    }
}
