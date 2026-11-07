//! Port of `Components/Session/SessionHeader.razor` — spec 07 §26. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Title (rename), branch/worktree, PR, remote, fork, stop, export, inspector toggle.
#[component]
pub fn SessionHeader(
    session: SessionHandle,
) -> impl IntoView {
    // port: SessionHeader.razor markup + behaviour (placeholder root only)
    view! { <div class="ph"></div> }
}
