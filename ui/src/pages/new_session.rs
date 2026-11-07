//! Port of `Components/Pages/NewSession.razor` — spec 06 §6.1. Owner: ui-pages-a.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Start card (folder + PickFolder + recents, extra dirs, worktree isolation & name, model,
/// fallback, agent, effort, permission mode, launch flags, from-PR) or the claude-not-found card. `?cwd=`.
#[component]
pub fn NewSession() -> impl IntoView {
    // port: NewSession.razor markup + behaviour (placeholder root only)
    view! { <div class="pane"></div> }
}
