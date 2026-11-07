//! Port of `Components/Session/StatusLine.razor` — spec 07 §22. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Live status under the thread: running / waiting / compacting / retry / hooks / custom statusLine text.
#[component]
pub fn StatusLine(
    session: SessionHandle,
) -> impl IntoView {
    // port: StatusLine.razor markup + behaviour (placeholder root only)
    view! { <div role="status" class="status-live"></div> }
}
