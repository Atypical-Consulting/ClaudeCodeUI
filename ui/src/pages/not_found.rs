//! Port of `Components/Pages/NotFound.razor` — spec 06 §6.8. Owner: ui-shell.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Router fallback: StatusScreen 404 with the current path.
#[component]
pub fn NotFound() -> impl IntoView {
    // port: NotFound.razor markup + behaviour (placeholder root only)
    view! { <div class="pane"></div> }
}
