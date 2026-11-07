//! Port of `Components/Session/TaskActions.razor` — spec 07 §5. Owner: ui-session-inspector.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Background (Ctrl+B) / stop buttons for a running shell or agent tool.
#[component]
pub fn TaskActions(
    session: SessionHandle,
    tool: ToolItem,
) -> impl IntoView {
    // port: TaskActions.razor markup + behaviour (placeholder root only)
    view! { <button type="button" class="btn ghost"></button> }
}
