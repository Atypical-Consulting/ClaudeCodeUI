//! Port of `Components/Session/TodoPanel.razor` — spec 07 §6, 02 §10. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// The agent's task list (`todo_list::from(items)` memo), collapsible.
#[component]
pub fn TodoPanel(
    session: SessionHandle,
) -> impl IntoView {
    // port: TodoPanel.razor markup + behaviour (placeholder root only)
    view! { <section class="todos" aria-label=t!("Todo.Title")></section> }
}
