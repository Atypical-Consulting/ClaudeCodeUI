//! Port of `Components/Shared/FileView.razor` — spec 06 §7. Owner: ui-shell.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Read tool output as numbered lines (`^\\s*(\\d+)[→\\t](.*?)\\r?$`) else `<pre class="cmdlog">`.
/// Rendered static once (app.js rewrites code.lc); keyed by tool id by callers.
#[component]
pub fn FileView(
    path: String,
    text: String,
) -> impl IntoView {
    // port: FileView.razor markup + behaviour (placeholder root only)
    view! { <pre class="cmdlog"></pre> }
}
