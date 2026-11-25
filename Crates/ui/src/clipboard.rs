//! Clipboard abstraction for text widgets.
//!
//! The platform integration lives in `ignit_ui_render`, which replaces the
//! in-memory default with a system clipboard. Headless tests and tools keep the
//! memory buffer.

use std::sync::Mutex;

/// Text clipboard used by [`crate::widgets::UiWidgetSpec::TextBox`].
pub trait UiClipboard: Send {
    fn get_text(&mut self) -> Option<String>;
    fn set_text(&mut self, text: String);
}

/// Process-local clipboard used when no platform implementation is installed.
#[derive(Default)]
pub struct UiMemoryClipboard {
    text: Option<String>,
}

impl UiClipboard for UiMemoryClipboard {
    fn get_text(&mut self) -> Option<String> {
        self.text.clone()
    }

    fn set_text(&mut self, text: String) {
        self.text = Some(text);
    }
}

/// ECS resource holding the active clipboard implementation.
pub struct UiClipboardResource(Mutex<Box<dyn UiClipboard>>);

impl Default for UiClipboardResource {
    fn default() -> Self {
        Self(Mutex::new(Box::new(UiMemoryClipboard::default())))
    }
}

impl UiClipboardResource {
    pub fn new(clipboard: impl UiClipboard + 'static) -> Self {
        Self(Mutex::new(Box::new(clipboard)))
    }

    pub fn get_text(&self) -> Option<String> {
        self.0.lock().ok()?.get_text()
    }

    pub fn set_text(&self, text: String) {
        if let Ok(mut clipboard) = self.0.lock() {
            clipboard.set_text(text);
        }
    }
}
