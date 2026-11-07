//! The Tauri implementation of `ccui_core::UiSink`: pushes → one webview event, notifications → native.
//! Owner: tauri-glue.

use ccui_shared::push::{Push, PUSH_EVENT};
use tauri::{AppHandle, Emitter, Manager};

pub struct TauriSink {
    app: AppHandle,
}

impl TauriSink {
    pub fn new(app: AppHandle) -> Self {
        TauriSink { app }
    }
}

impl ccui_core::UiSink for TauriSink {
    fn emit(&self, push: Push) {
        let _ = self.app.emit(PUSH_EVENT, push);
    }

    fn notify(&self, session_id: &str, title: &str, body: &str) {
        crate::notify(&self.app, session_id.to_string(), title.to_string(), body.to_string());
    }

    fn window_focused(&self) -> bool {
        self.app.get_webview_window("main").and_then(|w| w.is_focused().ok()).unwrap_or(false)
    }
}
