//! Port of `Components/Session/DiffPanel.razor` — spec 07 §17. Owner: ui-session-inspector.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Uncommitted changes of the session folder (git_diff) as DiffViews.
#[component]
pub fn DiffPanel(
    session: SessionHandle,
) -> impl IntoView {
    // port: DiffPanel.razor markup + behaviour (placeholder root only)
    view! { <div class="ph"></div> }
}
