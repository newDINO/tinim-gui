//! Optional system clipboard access for native backends.

use copypasta::{ClipboardContext, ClipboardProvider};

/// A single, long-lived system clipboard handle.
///
/// Some platforms, notably X11, require the process that set the clipboard contents to keep the
/// handle alive. Create one per application instead of one per frame.
///
/// If the platform clipboard cannot be initialized, reads and writes are silently ignored.
pub struct Clipboard {
    context: Option<ClipboardContext>,
}

impl Clipboard {
    /// Connects to the system clipboard, or degrades to a no-op when unavailable.
    pub fn new() -> Self {
        Self {
            context: ClipboardContext::new().ok(),
        }
    }

    /// Reads the current clipboard contents, or `None` when unavailable.
    pub fn read(&mut self) -> Option<String> {
        self.context.as_mut()?.get_contents().ok()
    }

    /// Replaces the clipboard contents. Errors are ignored.
    pub fn write(&mut self, text: &str) {
        if let Some(context) = self.context.as_mut() {
            let _ = context.set_contents(text.to_owned());
        }
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        Self::new()
    }
}
