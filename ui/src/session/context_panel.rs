//! Port of `Components/Session/ContextPanel.razor` — spec 07 §32. Owner: ui-session-inspector.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Default inspector: context usage, cost, extra dirs, memory, status, bookmarks, artifacts.
#[component]
pub fn ContextPanel(
    session: SessionHandle,
) -> impl IntoView {
    // port: ContextPanel.razor markup + behaviour (placeholder root only)
    view! { <div class="ph"></div> }
}
