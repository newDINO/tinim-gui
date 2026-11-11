//! System clipboard access for native and browser backends.
//!
//! Reading is request/response based because browser reads are asynchronous: [`Clipboard::request_read`]
//! starts a read and a later [`Clipboard::take_read`] returns its result. Native reads complete
//! immediately, so their result is simply waiting on the next call.

use std::sync::Arc;

use imp::Inner;
use winit::window::Window;

/// A single, long-lived system clipboard handle.
///
/// Some platforms, notably X11, require the process that set the clipboard contents to keep the
/// handle alive. Create one per application instead of one per frame.
///
/// On Wayland the clipboard keeps `window` alive, because its display pointer must stay valid for
/// as long as the clipboard worker thread runs.
///
/// If the platform clipboard cannot be initialized, reads and writes are silently ignored.
pub struct Clipboard {
    inner: Inner,
}

impl Clipboard {
    /// Connects to the system clipboard, or degrades to a no-op when unavailable.
    pub fn new(window: &Arc<Window>) -> Self {
        Self {
            inner: Inner::new(window),
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

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::sync::Arc;

    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::{JsFuture, spawn_local};
    use winit::window::Window;

    pub struct Inner {
        pending: Rc<RefCell<Option<String>>>,
        reading: Rc<Cell<bool>>,
    }

    impl Inner {
        pub fn new(_window: &Arc<Window>) -> Self {
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
    unix,
    not(any(
        target_os = "macos",
        target_os = "android",
        target_os = "ios",
        target_os = "emscripten"
    )),
    any(feature = "x11", feature = "wayland")
))]
mod imp {
    use std::sync::Arc;

    use copypasta::ClipboardProvider;
    use winit::window::Window;

    pub struct Inner {
        backend: Option<Backend>,
        pending: Option<String>,
    }

    enum Backend {
        /// Boxed because `copypasta::ClipboardContext` is much larger than the Wayland backend.
        /// This value is created once per application, but boxing also keeps the enum small and
        /// avoids clippy's `large_enum_variant` lint.
        #[cfg(feature = "x11")]
        X11(Box<copypasta::ClipboardContext>),
        #[cfg(feature = "wayland")]
        Wayland {
            clipboard: copypasta::wayland_clipboard::Clipboard,
            // Keep the window, and therefore its `wl_display`, alive for the clipboard thread.
            _window: Arc<Window>,
        },
    }

    impl Inner {
        pub fn new(window: &Arc<Window>) -> Self {
            Self {
                backend: Backend::new(window),
                pending: None,
            }
        }

        pub fn request_read(&mut self) {
            self.pending = self.backend.as_mut().and_then(|backend| match backend {
                #[cfg(feature = "x11")]
                Backend::X11(context) => context.get_contents().ok(),
                #[cfg(feature = "wayland")]
                Backend::Wayland { clipboard, .. } => clipboard.get_contents().ok(),
            });
        }

        pub fn take_read(&mut self) -> Option<String> {
            self.pending.take()
        }

        pub fn write(&mut self, text: &str) {
            let Some(backend) = self.backend.as_mut() else {
                return;
            };
            match backend {
                #[cfg(feature = "x11")]
                Backend::X11(context) => {
                    let _ = context.set_contents(text.to_owned());
                }
                #[cfg(feature = "wayland")]
                Backend::Wayland { clipboard, .. } => {
                    let _ = clipboard.set_contents(text.to_owned());
                }
            }
        }
    }

    impl Backend {
        fn new(window: &Arc<Window>) -> Option<Self> {
            #[cfg(feature = "wayland")]
            if let Some(backend) = wayland_backend(window) {
                return Some(backend);
            }

            #[cfg(feature = "x11")]
            if let Ok(context) = copypasta::ClipboardContext::new() {
                return Some(Self::X11(Box::new(context)));
            }

            let _ = window;
            None
        }
    }

    #[cfg(feature = "wayland")]
    fn wayland_backend(window: &Arc<Window>) -> Option<Backend> {
        use winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};

        let handle = window.display_handle().ok()?;
        let RawDisplayHandle::Wayland(display) = handle.as_raw() else {
            return None;
        };

        // SAFETY: `window` is cloned into the backend, so the `wl_display` it owns stays alive
        // for as long as the clipboard worker thread.
        let (_primary, clipboard) = unsafe {
            copypasta::wayland_clipboard::create_clipboards_from_external(display.display.as_ptr())
        };

        Some(Backend::Wayland {
            clipboard,
            _window: Arc::clone(window),
        })
    }
}

#[cfg(all(
    not(target_arch = "wasm32"),
    any(windows, target_os = "macos", target_os = "android", target_os = "ios")
))]
mod imp {
    use std::sync::Arc;

    use copypasta::{ClipboardContext, ClipboardProvider};
    use winit::window::Window;

    pub struct Inner {
        context: Option<ClipboardContext>,
        pending: Option<String>,
    }

    impl Inner {
        pub fn new(_window: &Arc<Window>) -> Self {
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
    not(any(windows, target_os = "macos", target_os = "android", target_os = "ios")),
    not(all(
        unix,
        not(any(
            target_os = "macos",
            target_os = "android",
            target_os = "ios",
            target_os = "emscripten"
        )),
        any(feature = "x11", feature = "wayland")
    ))
))]
mod imp {
    use std::sync::Arc;

    use winit::window::Window;

    pub struct Inner;

    impl Inner {
        pub fn new(_window: &Arc<Window>) -> Self {
            Self
        }

        pub fn request_read(&mut self) {}

        pub fn take_read(&mut self) -> Option<String> {
            None
        }

        pub fn write(&mut self, _text: &str) {}
    }
}
