//! Port of `Components/Pages/SessionPage.razor` — spec 06 §6.2, 07 §0.1. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Resolve `/session/:id` (live or transcript via AppStore::load_session — which watches it; `release` in
/// on_cleanup — else not found), then the
/// pane keyed by id (SessionHeader, Thread, TodoPanel, Composer) + Inspector. Relayed Escape interrupts a running turn.
#[component]
pub fn SessionPage() -> impl IntoView {
    // port: SessionPage.razor markup + behaviour (placeholder root only)
    view! { <div class="pane"></div> }
}
