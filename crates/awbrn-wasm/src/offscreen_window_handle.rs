use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::{ptr::NonNull, thread::ThreadId};
use web_sys::OffscreenCanvas;

pub(crate) struct OffscreenWindowHandle {
    window_handle: raw_window_handle::RawWindowHandle,
    display_handle: raw_window_handle::DisplayHandle<'static>,
    thread_id: ThreadId,
}

impl OffscreenWindowHandle {
    pub(crate) fn new(canvas: &OffscreenCanvas) -> Self {
        // Equivalent to WebOffscreenCanvasWindowHandle::from_wasm_bindgen_0_2
        let ptr = NonNull::from(canvas).cast();
        let handle = raw_window_handle::WebOffscreenCanvasWindowHandle::new(ptr);
        let window_handle = raw_window_handle::RawWindowHandle::WebOffscreenCanvas(handle);
        let display_handle = raw_window_handle::DisplayHandle::web();

        Self {
            window_handle,
            display_handle,
            thread_id: std::thread::current().id(),
        }
    }
}

/// # Safety
///
/// At runtime we ensure that OffscreenWrapper is only accessed from the thread it was created on
unsafe impl Send for OffscreenWindowHandle {}

unsafe impl Sync for OffscreenWindowHandle {}

impl HasWindowHandle for OffscreenWindowHandle {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        if self.thread_id != std::thread::current().id() {
            // OffscreenWrapper can only be accessed from the thread it was
            // created on and considering web workers are only single threaded,
            // this error should never happen.
            return Err(raw_window_handle::HandleError::NotSupported);
        }

        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(self.window_handle) })
    }
}

impl HasDisplayHandle for OffscreenWindowHandle {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        Ok(self.display_handle)
    }
}
