//! Port of `Components/Session/SessionTitle.razor` — spec 07 §1. Owner: ui-session-thread-composer.

use crate::state::{use_app, SessionHandle};
use ccui_shared::model::SessionStatus;
use ccui_shared::t;
use leptos::prelude::*;
use wasm_bindgen::{JsCast, JsValue};

/// `{prefix}{name} · Claude Code`: "(n) " = decisions waiting across all sessions, else a running marker for this one,
/// so the state shows in the taskbar / tab while the user is in another window. `name` None = no session.
pub fn title(decisions: usize, status: SessionStatus, name: Option<&str>) -> String {
    let prefix = if decisions > 0 {
        format!("({decisions}) ")
    } else if matches!(status, SessionStatus::Running | SessionStatus::Starting) {
        format!("{} · ", t!("Session.TitleRunning"))
    } else {
        String::new()
    };
    let name = name.map_or_else(|| t!("Session.Session"), str::to_string);
    format!("{prefix}{name} · Claude Code")
}

/// `document.title` and the native window title (document.title alone does not reach every title bar).
pub fn set_window_title(text: &str) {
    let Some(w) = web_sys::window() else { return };
    if let Some(d) = w.document() {
        d.set_title(text);
    }
    let get = |o: &JsValue, k: &str| js_sys::Reflect::get(o, &JsValue::from_str(k)).ok().filter(|v| !v.is_undefined());
    let Some(win) = get(&w, "__TAURI__").and_then(|t| get(&t, "window")) else { return };
    let Some(current) = get(&win, "getCurrentWindow").and_then(|f| f.dyn_into::<js_sys::Function>().ok()) else {
        return;
    };
    let Ok(handle) = current.call0(&win) else { return };
    if let Some(set) = get(&handle, "setTitle").and_then(|f| f.dyn_into::<js_sys::Function>().ok()) {
        if let Ok(p) = set.call1(&handle, &JsValue::from_str(text)) {
            if let Ok(p) = p.dyn_into::<js_sys::Promise>() {
                leptos::task::spawn_local(async move {
                    let _ = wasm_bindgen_futures::JsFuture::from(p).await;
                });
            }
        }
    }
}

/// Sets `document.title` = `{prefix}{name} · Claude Code` (decision count / running prefix). Renders nothing.
#[component]
pub fn SessionTitle(session: SessionHandle) -> impl IntoView {
    let app = use_app();
    let text = Memo::new(move |_| {
        let n = app.summary.with(|s| s.decision_queue.len());
        let found = session.found.get() == Some(true);
        session.meta.with(|m| title(n, m.status, found.then_some(m.name.as_str()).filter(|n| !n.is_empty())))
    });
    Effect::new(move |_| set_window_title(&text.get()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes() {
        assert_eq!(title(2, SessionStatus::Running, Some("fix")), "(2) fix · Claude Code");
        assert_eq!(title(0, SessionStatus::Idle, Some("fix")), "fix · Claude Code");
        assert!(title(0, SessionStatus::Starting, Some("fix")).ends_with(" · fix · Claude Code"));
        assert_eq!(title(0, SessionStatus::Exited, None), format!("{} · Claude Code", t!("Session.Session")));
    }
}
