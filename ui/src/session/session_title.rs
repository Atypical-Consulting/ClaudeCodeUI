//! Port of `Components/Session/SessionTitle.razor` — spec 07 §1. Owner: ui-session-thread-composer.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Sets `document.title` = `{prefix}{name} · Claude Code` (decision count / running prefix). Renders nothing.
#[component]
pub fn SessionTitle(
    session: SessionHandle,
) -> impl IntoView {
    // port: SessionTitle.razor markup + behaviour (placeholder root only)
    view! { <span hidden></span> }
}
