//! Port of `Components/Session/MemoryPanel.razor` — spec 07 §34, 05 §3. Owner: ui-session-inspector.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Memory files + auto memories: open, edit (SessionHandle.memory), save with conflict, delete.
#[component]
pub fn MemoryPanel(
    session: SessionHandle,
) -> impl IntoView {
    // port: MemoryPanel.razor markup + behaviour (placeholder root only)
    view! { <div class="section last"></div> }
}
