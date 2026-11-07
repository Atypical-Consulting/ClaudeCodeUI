//! Port of `Components/Pages/Overview.razor` — spec 06 §6.3. Owner: ui-pages-a.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Busy mode (table + decision queue) or at rest (resume cards snapshot, quota strip, unsaved worktrees).
#[component]
pub fn Overview() -> impl IntoView {
    // port: Overview.razor markup + behaviour (placeholder root only)
    view! { <div class="pane"></div> }
}
