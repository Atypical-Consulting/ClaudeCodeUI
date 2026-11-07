//! Port of `Components/Session/TasksPanel.razor` — spec 07 §25. Owner: ui-session-inspector.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Background tasks of this process: output tail (session_task_output), stop, clear ended.
#[component]
pub fn TasksPanel(
    session: SessionHandle,
) -> impl IntoView {
    // port: TasksPanel.razor markup + behaviour (placeholder root only)
    view! { <section class="section bg-tasks"></section> }
}
