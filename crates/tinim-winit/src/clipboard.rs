//! System clipboard access for native and browser backends.
//!
//! Reading is request/response based because browser reads are asynchronous: [`Clipboard::request_read`]
//! starts a read and a later [`Clipboard::take_read`] returns its result. Native reads complete
//! immediately, so their result is simply waiting on the next call.

use imp::Inner;

/// A single, long-lived system clipboard handle.
///
/// Some platforms, notably X11, require the process that set the clipboard contents to keep the
/// handle alive. Create one per application instead of one per frame.
///
/// If the platform clipboard cannot be initialized, reads and writes are silently ignored.
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

    /// Starts a clipboard read.
    ///
    /// The result becomes available from [`Clipboard::take_read`]. Calling this while a browser
    /// read is still in flight is ignored.
    pub fn request_read(&mut self) {
        self.inner.request_read();
    }

    /// Takes the result of a previous [`Clipboard::request_read`].
    pub fn take_read(&mut self) -> Option<String> {
        self.inner.take_read()
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

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::{JsFuture, spawn_local};

    pub struct Inner {
        pending: Rc<RefCell<Option<String>>>,
        reading: Rc<Cell<bool>>,
    }

    impl Inner {
        pub fn new() -> Self {
            Self {
                pending: Rc::new(RefCell::new(None)),
                reading: Rc::new(Cell::new(false)),
            }
        }

        pub fn request_read(&mut self) {
            if self.reading.replace(true) {
                return;
            }
            *self.pending.borrow_mut() = None;

            let pending = Rc::clone(&self.pending);
            let reading = Rc::clone(&self.reading);
            spawn_local(async move {
                *pending.borrow_mut() = read_clipboard().await;
                reading.set(false);
            });
        }

        pub fn take_read(&mut self) -> Option<String> {
            self.pending.borrow_mut().take()
        }

        pub fn write(&mut self, text: &str) {
            let Some(clipboard) = clipboard() else {
                return;
            };
            let text = text.to_owned();
            spawn_local(async move {
                if let Err(error) = JsFuture::from(clipboard.write_text(&text)).await {
                    log::error!("failed to write the clipboard: {error:?}");
                }
            });
        }
    }

    /// The browser clipboard, or `None` when the API is unavailable (for example in an insecure
    /// context).
    fn clipboard() -> Option<web_sys::Clipboard> {
        let navigator = web_sys::window()?.navigator();
        let navigator: &JsValue = navigator.as_ref();
        let value = js_sys::Reflect::get(navigator, &JsValue::from_str("clipboard")).ok()?;
        if value.is_undefined() || value.is_null() {
            return None;
        }
        value.dyn_into().ok()
    }

    async fn read_clipboard() -> Option<String> {
        let clipboard = clipboard()?;
        match JsFuture::from(clipboard.read_text()).await {
            Ok(value) => value.as_string(),
            Err(error) => {
                log::error!("failed to read the clipboard: {error:?}");
                None
            }
        }
    }
}

#[cfg(all(
    not(target_arch = "wasm32"),
    any(
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
            target_os = "ios"
        ))
    )
))]
mod imp {
    use copypasta::{ClipboardContext, ClipboardProvider};

    pub struct Inner {
        context: Option<ClipboardContext>,
        pending: Option<String>,
    }

    impl Inner {
        pub fn new() -> Self {
            Self {
                context: ClipboardContext::new().ok(),
                pending: None,
            }
        }

        pub fn request_read(&mut self) {
            self.pending = self
                .context
                .as_mut()
                .and_then(|context| context.get_contents().ok());
        }

        pub fn take_read(&mut self) -> Option<String> {
            self.pending.take()
        }

        pub fn write(&mut self, text: &str) {
            if let Some(context) = self.context.as_mut() {
                let _ = context.set_contents(text.to_owned());
            }
        }
    }
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(
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
            target_os = "ios"
        ))
    ))
))]
mod imp {
    pub struct Inner;

    impl Inner {
        pub fn new() -> Self {
            Self
        }

        pub fn request_read(&mut self) {}

        pub fn take_read(&mut self) -> Option<String> {
            None
        }

        pub fn write(&mut self, _text: &str) {}
    }
}
