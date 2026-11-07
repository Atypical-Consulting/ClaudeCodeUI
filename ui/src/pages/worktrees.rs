//! Port of `Components/Pages/Worktrees.razor` — spec 06 §6.6, 04 §1. Owner: ui-pages-a.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Repos → worktree rows (state, why, size), plan preview, run log (Push::WorktreeRunStep), push. `?path=`.
#[component]
pub fn Worktrees() -> impl IntoView {
    // port: Worktrees.razor markup + behaviour (placeholder root only)
    view! { <div class="pane"></div> }
}
