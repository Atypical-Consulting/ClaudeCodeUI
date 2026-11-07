//! Port of `Components/Session/Inspector.razor` — spec 07 §10. Owner: ui-session-inspector.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// `aside.pane.insp` when open: PermissionCard (first pending), TasksPanel, then WorkflowPanel | AgentPanel |
/// ToolDetail (selected) | DiffPanel | ContextPanel.
#[component]
pub fn Inspector(
    session: SessionHandle,
) -> impl IntoView {
    // port: Inspector.razor markup + behaviour (placeholder root only)
    view! { <aside class="pane insp" aria-label=t!("Inspector")></aside> }
}
