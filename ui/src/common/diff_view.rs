//! Port of `Components/Shared/DiffView.razor` — spec 06 §7. Owner: ui-shell.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// File header (+adds −dels) and up to 2000 rows; `data-hl` = `edit_diff::lang(path)`.
#[component]
pub fn DiffView(
    path: String,
    lines: Vec<ccui_shared::edit_diff::DiffLine>,
) -> impl IntoView {
    // port: DiffView.razor markup + behaviour (placeholder root only)
    view! { <div class="diff"></div> }
}
